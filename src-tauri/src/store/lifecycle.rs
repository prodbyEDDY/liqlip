//! Record lifecycle (ARCHITECTURE §4.2): soft delete + undo tokens (in memory, 2 min),
//! hard "delete recent", clear history / clear all, purge of expired soft deletes, and
//! retention (age → count → size, history only).
//!
//! Privacy actions ("delete recent", "clear all") run with `PRAGMA secure_delete = ON`,
//! then hand freed pages back (`PRAGMA incremental_vacuum`) and truncate the WAL
//! (`PRAGMA wal_checkpoint(TRUNCATE)`), so the removed content does not linger in the
//! database files. The FTS tables use FTS5 'secure-delete' (schema v1).

use super::media;
use super::{db_err, placeholders, Store, CHUNK};
use crate::logx;
use crate::model::{ClearScope, ClipsChange, Retention, StorageSettings};
use rusqlite::{params, params_from_iter, OptionalExtension, TransactionBehavior};
use std::collections::{HashMap, HashSet};

/// Undo tokens and soft-deleted rows live this long (§4.2).
pub(crate) const UNDO_TTL_MS: i64 = 120_000;
/// `clips_delete_recent` accepts up to ten years.
const RECENT_MAX_MINUTES: i64 = 10 * 366 * 24 * 60;
/// Free pages worth returning to the OS after routine deletes (4 MiB at 4 KiB pages).
pub(crate) const VACUUM_MIN_FREE_PAGES: i64 = 1024;
const DAY_MS: i64 = 86_400_000;

// Hot background statements (the plan test in feed.rs checks exactly these). Retention
// names its covering partial index: INDEXED BY turns a silently degraded plan into an
// error (lang_indexedby.html).
pub(crate) const RETENTION_AGE_SQL: &str = "SELECT id FROM clips INDEXED BY idx_clips_history
     WHERE deleted_at IS NULL AND favorite = 0 AND folder_id IS NULL AND copied_at < ?1
     ORDER BY copied_at, id LIMIT ?2";
pub(crate) const RETENTION_OLDEST_SQL: &str = "SELECT id, size FROM clips INDEXED BY idx_clips_history
     WHERE deleted_at IS NULL AND favorite = 0 AND folder_id IS NULL
     ORDER BY copied_at, id LIMIT ?1";
pub(crate) const RETENTION_TOTALS_SQL: &str = "SELECT count(*), coalesce(sum(size), 0) FROM clips INDEXED BY idx_clips_history
     WHERE deleted_at IS NULL AND favorite = 0 AND folder_id IS NULL";
pub(crate) const PURGE_SQL: &str = "SELECT id FROM clips WHERE deleted_at < ?1 LIMIT ?2";
/// Two disjoint halves (live / soft-deleted) so each can use its partial index.
pub(crate) const RECENT_SQL: &str = "SELECT id FROM clips WHERE deleted_at IS NULL AND copied_at >= ?1
     UNION ALL SELECT id FROM clips WHERE deleted_at IS NOT NULL AND copied_at >= ?1";

/// What an undo token restores.
#[derive(Debug, Clone)]
pub(crate) enum UndoOp {
    /// Soft delete (`clips_delete`, `clips_clear('history')`): clear `deleted_at` of rows
    /// still carrying this operation's timestamp.
    Revive { ids: Vec<i64>, at: i64, change: ClipsChange },
    /// `clip_set_folder`: previous folder of each record.
    Refolder { prev: Vec<(i64, Option<i64>)> },
    /// `folder_delete`: the folder row and its members (revived if deleted with it).
    FolderDelete { id: i64, name: String, position: f64, created_at: i64, members: Vec<i64>, revive_at: Option<i64> },
}

#[derive(Debug)]
struct UndoEntry {
    expires: i64,
    op: UndoOp,
}

#[derive(Debug, Default)]
pub(crate) struct UndoBook {
    seq: u64,
    entries: HashMap<String, UndoEntry>,
}

impl UndoBook {
    fn expire(&mut self, now: i64) {
        self.entries.retain(|_, e| e.expires > now);
    }
}

/// Result of a successful undo: the `clips:changed` the caller emits.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Undone {
    pub change: ClipsChange,
    pub ids: Option<Vec<i64>>,
}

pub(crate) fn retention_ms(r: Retention) -> Option<i64> {
    match r {
        Retention::Day1 => Some(DAY_MS),
        Retention::Week1 => Some(7 * DAY_MS),
        Retention::Month1 => Some(30 * DAY_MS),
        Retention::Month3 => Some(90 * DAY_MS),
        Retention::Month6 => Some(180 * DAY_MS),
        Retention::Year1 => Some(365 * DAY_MS),
        Retention::Forever => None,
    }
}

/// `?1` + `?2…` placeholders for one leading parameter and `n` ids.
fn after_first(n: usize) -> String {
    (2..n + 2).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",")
}

impl Store {
    pub(crate) fn push_undo(&self, op: UndoOp) -> String {
        let now = self.now();
        let mut book = self.undo.lock();
        book.expire(now);
        book.seq += 1;
        let token = format!("u{:x}.{:x}", book.seq, now);
        book.entries.insert(token.clone(), UndoEntry { expires: now + UNDO_TTL_MS, op });
        token
    }

    /// Drop expired undo tokens (background tick).
    pub(crate) fn expire_undo(&self) {
        let now = self.now();
        self.undo.lock().expire(now);
    }

    /// Soft delete (all or nothing): rows disappear from every listing, files stay until
    /// the purge (> 2 min). Returns the ids and an undo token.
    pub(crate) fn delete(&self, ids: &[i64]) -> Result<(Vec<i64>, String), String> {
        let ids = super::dedup_ids(ids);
        let now = self.now();
        {
            let mut conn = self.lock();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
            let mut found: HashSet<i64> = HashSet::with_capacity(ids.len());
            for chunk in ids.chunks(CHUNK) {
                let sql =
                    format!("SELECT id FROM clips WHERE deleted_at IS NULL AND id IN ({})", placeholders(chunk.len()));
                let mut st = tx.prepare(&sql).map_err(db_err)?;
                let rows = st.query_map(params_from_iter(chunk), |r| r.get::<_, i64>(0)).map_err(db_err)?;
                for row in rows {
                    found.insert(row.map_err(db_err)?);
                }
            }
            if let Some(missing) = ids.iter().find(|id| !found.contains(id)) {
                return Err(format!("not_found: clip {missing}"));
            }
            for chunk in ids.chunks(CHUNK) {
                let sql = format!("UPDATE clips SET deleted_at = ?1 WHERE id IN ({})", after_first(chunk.len()));
                let mut values = vec![now];
                values.extend_from_slice(chunk);
                tx.execute(&sql, params_from_iter(values)).map_err(db_err)?;
            }
            tx.commit().map_err(db_err)?;
        }
        let token = self.push_undo(UndoOp::Revive { ids: ids.clone(), at: now, change: ClipsChange::Added });
        Ok((ids, token))
    }

    /// Apply an undo token once. `Ok(None)` = unknown or expired token.
    pub(crate) fn undo(&self, token: &str) -> Result<Option<Undone>, String> {
        let now = self.now();
        let op = {
            let mut book = self.undo.lock();
            book.expire(now);
            match book.entries.remove(token) {
                Some(e) => e.op,
                None => return Ok(None),
            }
        };
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let undone = match op {
            UndoOp::Revive { ids, at, change } => {
                for chunk in ids.chunks(CHUNK) {
                    let sql = format!("UPDATE clips SET deleted_at = NULL WHERE deleted_at = ?1 AND id IN ({})", after_first(chunk.len()));
                    let mut values = vec![at];
                    values.extend_from_slice(chunk);
                    tx.execute(&sql, params_from_iter(values)).map_err(db_err)?;
                }
                let ids = (change != ClipsChange::Reset).then_some(ids);
                Undone { change, ids }
            }
            UndoOp::Refolder { prev } => {
                for (id, folder) in &prev {
                    // A folder deleted since then → the record stays without a folder.
                    tx.execute(
                        "UPDATE clips SET folder_id = (SELECT id FROM folders WHERE id = ?1) WHERE id = ?2",
                        params![folder, id],
                    )
                    .map_err(db_err)?;
                }
                Undone { change: ClipsChange::Updated, ids: Some(prev.into_iter().map(|(id, _)| id).collect()) }
            }
            UndoOp::FolderDelete { id, name, position, created_at, members, revive_at } => {
                let taken: Vec<String> = {
                    let mut st = tx.prepare("SELECT name FROM folders").map_err(db_err)?;
                    let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(db_err)?;
                    rows.collect::<Result<_, _>>().map_err(db_err)?
                };
                let name = super::folders::unique_name(&name, &taken);
                let id_free = tx
                    .query_row("SELECT 1 FROM folders WHERE id = ?1", [id], |_| Ok(()))
                    .optional()
                    .map_err(db_err)?
                    .is_none();
                let fid = if id_free {
                    tx.execute(
                        "INSERT INTO folders (id, name, position, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![id, name, position, created_at],
                    )
                    .map_err(db_err)?;
                    id
                } else {
                    tx.execute(
                        "INSERT INTO folders (name, position, created_at) VALUES (?1, ?2, ?3)",
                        params![name, position, created_at],
                    )
                    .map_err(db_err)?;
                    tx.last_insert_rowid()
                };
                for chunk in members.chunks(CHUNK) {
                    let (sql, mut values) = match revive_at {
                        // Members deleted together with the folder carry its timestamp.
                        Some(at) => (
                            format!(
                                "UPDATE clips SET folder_id = ?1,
                                   deleted_at = CASE WHEN deleted_at = ?2 THEN NULL ELSE deleted_at END
                                 WHERE id IN ({})",
                                (3..chunk.len() + 3).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",")
                            ),
                            vec![fid, at],
                        ),
                        None => (
                            format!("UPDATE clips SET folder_id = ?1 WHERE id IN ({})", after_first(chunk.len())),
                            vec![fid],
                        ),
                    };
                    values.extend_from_slice(chunk);
                    tx.execute(&sql, params_from_iter(values)).map_err(db_err)?;
                }
                Undone {
                    change: if revive_at.is_some() { ClipsChange::Added } else { ClipsChange::Updated },
                    ids: Some(members),
                }
            }
        };
        tx.commit().map_err(db_err)?;
        Ok(Some(undone))
    }

    /// Hard-delete rows (files included), [`CHUNK`] rows per transaction so capture and the
    /// UI interleave. Returns the ids actually removed.
    pub(crate) fn hard_delete(&self, ids: &[i64]) -> Result<Vec<i64>, String> {
        let mut gone = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK) {
            let removed: Vec<i64> = {
                let mut conn = self.lock();
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
                let removed = {
                    let sql = format!("DELETE FROM clips WHERE id IN ({}) RETURNING id", placeholders(chunk.len()));
                    let mut st = tx.prepare(&sql).map_err(db_err)?;
                    let rows = st.query_map(params_from_iter(chunk), |r| r.get::<_, i64>(0)).map_err(db_err)?;
                    rows.collect::<Result<Vec<i64>, _>>().map_err(db_err)?
                };
                tx.commit().map_err(db_err)?;
                removed
            };
            media::remove_clip_files(self.paths(), &removed);
            gone.extend(removed);
        }
        Ok(gone)
    }

    /// Run `f` with `secure_delete = ON`, then return freed pages to the OS and truncate
    /// the WAL. The FAST default is restored even if `f` fails.
    fn privacy_scope<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        self.lock().execute_batch("PRAGMA secure_delete = ON;").map_err(db_err)?;
        let result = f();
        let conn = self.lock();
        if let Err(e) = vacuum_all(&conn) {
            logx::warn("store", &format!("incremental_vacuum: {e}"));
        }
        if let Err(e) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())) {
            logx::warn("store", &format!("wal_checkpoint: {e}"));
        }
        if let Err(e) = conn.execute_batch("PRAGMA secure_delete = FAST;") {
            logx::warn("store", &format!("secure_delete reset: {e}"));
        }
        result
    }

    /// Hard-delete everything copied in the last `minutes` — favorites and folder members
    /// included, soft-deleted rows included (the UI confirms). No undo. Returns the ids.
    pub(crate) fn delete_recent_ids(&self, minutes: i64) -> Result<Vec<i64>, String> {
        if !(1..=RECENT_MAX_MINUTES).contains(&minutes) {
            return Err("invalid: minutes".into());
        }
        let cutoff = self.now() - minutes * 60_000;
        let ids: Vec<i64> = {
            let conn = self.lock();
            let mut st = conn.prepare(RECENT_SQL).map_err(db_err)?;
            let rows = st.query_map([cutoff], |r| r.get::<_, i64>(0)).map_err(db_err)?;
            rows.collect::<Result<_, _>>().map_err(db_err)?
        };
        if ids.is_empty() {
            return Ok(ids);
        }
        let gone = self.privacy_scope(|| self.hard_delete(&ids))?;
        logx::warn("store", &format!("delete recent: {} records removed", gone.len()));
        Ok(gone)
    }

    /// Hard-delete everything copied in the last `minutes` (files included); returns the
    /// count. Shared by the `clips_delete_recent` command and the tray. The caller emits.
    pub fn delete_recent(&self, minutes: i64) -> Result<i64, String> {
        self.delete_recent_ids(minutes).map(|ids| ids.len() as i64)
    }

    /// `History`: soft-delete everything except favorites and folder members (undoable).
    /// `All`: hard-delete every record and folder (snippets stay); no undo.
    pub(crate) fn clear(&self, scope: ClearScope) -> Result<Option<String>, String> {
        let now = self.now();
        match scope {
            ClearScope::History => {
                let ids: Vec<i64> = {
                    let mut conn = self.lock();
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
                    let ids = {
                        let mut st = tx
                            .prepare(
                                "UPDATE clips SET deleted_at = ?1
                                 WHERE deleted_at IS NULL AND favorite = 0 AND folder_id IS NULL RETURNING id",
                            )
                            .map_err(db_err)?;
                        let rows = st.query_map([now], |r| r.get::<_, i64>(0)).map_err(db_err)?;
                        rows.collect::<Result<Vec<i64>, _>>().map_err(db_err)?
                    };
                    tx.commit().map_err(db_err)?;
                    ids
                };
                Ok(Some(self.push_undo(UndoOp::Revive { ids, at: now, change: ClipsChange::Reset })))
            }
            ClearScope::All => {
                let ids = self.privacy_scope(|| {
                    let mut conn = self.lock();
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
                    let ids: Vec<i64> = {
                        let mut st = tx.prepare("SELECT id FROM clips").map_err(db_err)?;
                        let rows = st.query_map([], |r| r.get::<_, i64>(0)).map_err(db_err)?;
                        rows.collect::<Result<_, _>>().map_err(db_err)?
                    };
                    // One FTS 'delete-all' instead of a per-row 'delete' through the trigger
                    // (fts5.html §6.4, external-content tables).
                    tx.execute_batch(
                        "DROP TRIGGER clips_fts_ad;
                         DELETE FROM clips;
                         INSERT INTO clips_fts(clips_fts) VALUES ('delete-all');
                         DELETE FROM folders;",
                    )
                    .map_err(db_err)?;
                    tx.execute_batch(super::schema::CLIPS_FTS_DELETE_TRIGGER).map_err(db_err)?;
                    tx.commit().map_err(db_err)?;
                    Ok(ids)
                })?;
                media::remove_clip_files(self.paths(), &ids);
                self.undo.lock().entries.clear();
                if let Err(e) = self.sweep_orphans() {
                    logx::warn("store", &format!("sweep after clear: {e}"));
                }
                logx::warn("store", &format!("clear all: {} records removed", ids.len()));
                Ok(None)
            }
        }
    }

    /// Hard-delete rows soft-deleted more than [`UNDO_TTL_MS`] ago (their tokens expired).
    pub(crate) fn purge_deleted(&self) -> Result<usize, String> {
        let cutoff = self.now() - UNDO_TTL_MS;
        let mut total = 0;
        loop {
            let ids: Vec<i64> = {
                let conn = self.lock();
                let mut st = conn.prepare(PURGE_SQL).map_err(db_err)?;
                let rows = st.query_map(params![cutoff, CHUNK as i64], |r| r.get::<_, i64>(0)).map_err(db_err)?;
                rows.collect::<Result<_, _>>().map_err(db_err)?
            };
            if ids.is_empty() {
                return Ok(total);
            }
            total += self.hard_delete(&ids)?.len();
        }
    }

    /// Retention (§4.2): only `favorite = 0 AND folder_id IS NULL` live records ("history";
    /// favorites, folder members and snippets are never touched). By age (`copied_at`), then
    /// keep the newest `maxEntries`, then drop the oldest until the history's stored bytes
    /// (`size`: text + rich formats + full PNG) fit `maxSizeMb`. Hard delete with files.
    /// Returns the removed ids.
    pub(crate) fn run_retention(&self, s: &StorageSettings) -> Result<Vec<i64>, String> {
        let mut gone = Vec::new();
        let oldest = |n: i64| -> Result<Vec<(i64, i64)>, String> {
            let conn = self.lock();
            let mut st = conn.prepare(RETENTION_OLDEST_SQL).map_err(db_err)?;
            let rows = st.query_map([n], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(db_err)?;
            rows.collect::<Result<_, _>>().map_err(db_err)
        };
        let totals = || -> Result<(i64, i64), String> {
            self.lock().query_row(RETENTION_TOTALS_SQL, [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(db_err)
        };
        if let Some(age) = retention_ms(s.retention) {
            let cutoff = self.now() - age;
            loop {
                let ids: Vec<i64> = {
                    let conn = self.lock();
                    let mut st = conn.prepare(RETENTION_AGE_SQL).map_err(db_err)?;
                    let rows = st.query_map(params![cutoff, CHUNK as i64], |r| r.get::<_, i64>(0)).map_err(db_err)?;
                    rows.collect::<Result<_, _>>().map_err(db_err)?
                };
                if ids.is_empty() {
                    break;
                }
                gone.extend(self.hard_delete(&ids)?);
            }
        }
        if s.max_entries > 0 {
            loop {
                let excess = totals()?.0 - s.max_entries;
                if excess <= 0 {
                    break;
                }
                let ids: Vec<i64> = oldest(excess.min(CHUNK as i64))?.into_iter().map(|(id, _)| id).collect();
                if ids.is_empty() {
                    break;
                }
                gone.extend(self.hard_delete(&ids)?);
            }
        }
        if s.max_size_mb > 0 {
            let limit = s.max_size_mb.saturating_mul(1024 * 1024);
            loop {
                let mut over = totals()?.1 - limit;
                if over <= 0 {
                    break;
                }
                let mut batch = Vec::new();
                for (id, size) in oldest(CHUNK as i64)? {
                    batch.push(id);
                    over -= size;
                    if over <= 0 {
                        break;
                    }
                }
                if batch.is_empty() {
                    break;
                }
                gone.extend(self.hard_delete(&batch)?);
            }
        }
        if !gone.is_empty() {
            logx::warn("store", &format!("retention: {} records removed", gone.len()));
        }
        Ok(gone)
    }

    /// Return free pages to the OS when at least `min_free` have accumulated. Returns the
    /// number of pages released.
    pub(crate) fn vacuum_if_needed(&self, min_free: i64) -> Result<i64, String> {
        let conn = self.lock();
        let free: i64 = conn.query_row("PRAGMA freelist_count", [], |r| r.get(0)).map_err(db_err)?;
        if free < min_free.max(1) {
            return Ok(0);
        }
        vacuum_all(&conn).map_err(db_err)
    }
}

/// `PRAGMA incremental_vacuum` frees ONE page per `sqlite3_step` (its VDBE program emits a
/// result row per page — sqlite3.c, PragTyp_INCREMENTAL_VACUUM), so step it to the end.
fn vacuum_all(conn: &rusqlite::Connection) -> rusqlite::Result<i64> {
    let mut st = conn.prepare("PRAGMA incremental_vacuum")?;
    let mut rows = st.query([])?;
    let mut n = 0;
    while rows.next()?.is_some() {
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{text_clip, TestStore};
    use super::*;
    use crate::model::{FeedQuery, Filter, PerKind};

    fn live(t: &TestStore) -> Vec<i64> {
        let q = FeedQuery { text: String::new(), exclude: vec![], filter: Filter::default(), cursor: None, limit: 200 };
        t.clips_list(&q, false).unwrap().items.into_iter().map(|c| c.id).collect()
    }

    fn rows(t: &TestStore) -> i64 {
        t.lock().query_row("SELECT count(*) FROM clips", [], |r| r.get(0)).unwrap()
    }

    fn storage(retention: Retention, max_entries: i64, max_size_mb: i64) -> StorageSettings {
        StorageSettings { retention, max_entries, max_size_mb, types: PerKind::splat(true) }
    }

    #[test]
    fn soft_delete_and_undo() {
        let t = TestStore::new();
        let a = t.add_text("a", 1);
        let b = t.add_text("b", 2);
        let (ids, token) = t.delete(&[a, a]).unwrap();
        assert_eq!(ids, vec![a]);
        assert_eq!(live(&t), vec![b]);
        assert!(t.delete(&[a]).unwrap_err().starts_with("not_found:"), "already deleted");
        assert!(t.delete(&[b, 999]).unwrap_err().starts_with("not_found:"));
        assert_eq!(live(&t), vec![b], "all or nothing");
        let undone = t.undo(&token).unwrap().unwrap();
        assert_eq!(undone, Undone { change: ClipsChange::Added, ids: Some(vec![a]) });
        assert_eq!(live(&t), vec![b, a]);
        assert_eq!(t.undo(&token).unwrap(), None, "a token works once");
        assert_eq!(t.undo("bogus").unwrap(), None);
    }

    #[test]
    fn undo_expires_and_purge_removes_rows_and_files() {
        let t = TestStore::new();
        t.set_now(1_000_000);
        let img = t.add_image(8, 8, 1_000_000, true);
        let (_, token) = t.delete(&[img]).unwrap();
        t.set_now(1_000_000 + UNDO_TTL_MS - 1);
        assert_eq!(t.purge_deleted().unwrap(), 0, "still inside the undo window");
        t.set_now(1_000_000 + UNDO_TTL_MS + 1);
        assert_eq!(t.undo(&token).unwrap(), None, "expired");
        assert_eq!(t.purge_deleted().unwrap(), 1);
        assert_eq!(rows(&t), 0);
        assert!(!t.paths().media_png(img).exists() && !t.paths().thumb_png(img).exists());
    }

    #[test]
    fn undo_set_folder_restores_previous_folders() {
        let t = TestStore::new();
        let a = t.add_text("a", 1);
        let b = t.add_text("b", 2);
        let f1 = t.folder_create("One").unwrap();
        let f2 = t.folder_create("Two").unwrap();
        t.set_folder(&[a], Some(f1.id)).unwrap();
        let (_, token) = t.set_folder(&[a, b], Some(f2.id)).unwrap();
        let folder = |id| t.clip_detail(id).unwrap().clip.folder_id;
        assert_eq!((folder(a), folder(b)), (Some(f2.id), Some(f2.id)));
        let u = t.undo(&token).unwrap().unwrap();
        assert_eq!(u.change, ClipsChange::Updated);
        assert_eq!((folder(a), folder(b)), (Some(f1.id), None));
        // Previous folder deleted meanwhile → no folder.
        let (_, token) = t.set_folder(&[a], None).unwrap();
        t.folder_delete(f1.id, false).unwrap();
        t.undo(&token).unwrap().unwrap();
        assert_eq!(folder(a), None);
    }

    #[test]
    fn undo_folder_delete_with_and_without_clips() {
        let t = TestStore::new();
        let a = t.add_text("a", 1);
        let b = t.add_text("b", 2);
        let f = t.folder_create("Keep").unwrap();
        t.set_folder(&[a, b], Some(f.id)).unwrap();
        let (token, members) = t.folder_delete(f.id, false).unwrap();
        assert_eq!(members.len(), 2);
        assert!(t.folders_list().unwrap().is_empty());
        assert_eq!(live(&t).len(), 2, "records stay in history");
        let u = t.undo(&token).unwrap().unwrap();
        assert_eq!(u.change, ClipsChange::Updated);
        let folders = t.folders_list().unwrap();
        assert_eq!((folders[0].id, folders[0].name.as_str(), folders[0].count), (f.id, "Keep", 2));
        let (token, _) = t.folder_delete(f.id, true).unwrap();
        assert!(live(&t).is_empty(), "records deleted with the folder");
        // A folder with the same name appeared meanwhile → the restored one is renamed.
        t.folder_create("keep").unwrap();
        let u = t.undo(&token).unwrap().unwrap();
        assert_eq!(u.change, ClipsChange::Added);
        assert_eq!(live(&t).len(), 2);
        let names: Vec<String> = t.folders_list().unwrap().into_iter().map(|f| f.name).collect();
        assert!(names.contains(&"keep".to_string()) && names.contains(&"Keep 2".to_string()), "{names:?}");
    }

    #[test]
    fn delete_recent_is_hard_and_includes_protected() {
        let t = TestStore::new();
        t.set_now(100 * 60_000);
        let old = t.add_text("old", 10 * 60_000);
        let fav = t.add_text("fav", 99 * 60_000);
        let img = t.add_image(8, 8, 98 * 60_000, true);
        let deleted = t.add_text("soft-deleted", 97 * 60_000);
        t.set_favorite(&[fav], true).unwrap();
        t.delete(&[deleted]).unwrap();
        assert!(t.delete_recent(0).unwrap_err().starts_with("invalid:"));
        assert!(t.delete_recent(-5).unwrap_err().starts_with("invalid:"));
        let n = t.delete_recent(5).unwrap();
        assert_eq!(n, 3);
        assert_eq!(live(&t), vec![old]);
        assert_eq!(rows(&t), 1, "soft-deleted rows in the window are gone too");
        assert!(!t.paths().media_png(img).exists());
        let sd: i64 = t.lock().query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap();
        assert_eq!(sd, 2, "FAST restored after the privacy action");
        let wal = std::fs::metadata(t.paths().wal()).map(|m| m.len()).unwrap_or(0);
        assert_eq!(wal, 0, "WAL truncated");
        assert_eq!(t.delete_recent(1).unwrap(), 0);
    }

    #[test]
    fn clear_history_keeps_protected_and_is_undoable() {
        let t = TestStore::new();
        let a = t.add_text("plain", 1);
        let fav = t.add_text("fav", 2);
        let inf = t.add_text("in folder", 3);
        let f = t.folder_create("F").unwrap();
        t.set_folder(&[inf], Some(f.id)).unwrap();
        t.set_favorite(&[fav], true).unwrap();
        let token = t.clear(ClearScope::History).unwrap().unwrap();
        assert_eq!(live(&t), vec![inf, fav]);
        let u = t.undo(&token).unwrap().unwrap();
        assert_eq!(u, Undone { change: ClipsChange::Reset, ids: None });
        assert_eq!(live(&t), vec![inf, fav, a]);
    }

    #[test]
    fn clear_all_removes_everything_but_snippets() {
        let t = TestStore::new();
        let a = t.add_text("searchable words", 1);
        let img = t.add_image(8, 8, 2, true);
        let f = t.folder_create("F").unwrap();
        t.set_folder(&[a], Some(f.id)).unwrap();
        t.set_favorite(&[a], true).unwrap();
        let (_, token) = t.delete(&[img]).unwrap();
        t.snippet_save(&crate::model::SnippetInput { id: None, name: "keep".into(), body: "me".into(), keyword: None })
            .unwrap();
        assert_eq!(t.clear(ClearScope::All).unwrap(), None);
        assert_eq!(rows(&t), 0);
        assert!(t.folders_list().unwrap().is_empty());
        assert_eq!(t.snippets_list("").unwrap().len(), 1);
        assert!(!t.paths().media_png(img).exists());
        assert_eq!(t.undo(&token).unwrap(), None, "tokens dropped");
        let n: i64 = t.lock().query_row("SELECT count(*) FROM clips_fts WHERE clips_fts MATCH '\"words\"'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "FTS emptied");
        // The delete trigger is back: new rows still leave the index when deleted.
        let b = t.add_text("fresh words", 3);
        t.hard_delete(&[b]).unwrap();
        let n: i64 = t.lock().query_row("SELECT count(*) FROM clips_fts WHERE clips_fts MATCH '\"words\"'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let integrity = t.lock().execute("INSERT INTO clips_fts(clips_fts, rank) VALUES ('integrity-check', 1)", []);
        assert!(integrity.is_ok(), "{integrity:?}");
        // Ids are never reused (AUTOINCREMENT).
        let c = t.add_text("after clear", 4);
        assert!(c > img);
    }

    #[test]
    fn retention_by_age_count_and_size_spares_protected() {
        let t = TestStore::new();
        let now = 400 * DAY_MS;
        t.set_now(now);
        let ancient = t.add_text("ancient", now - 100 * DAY_MS);
        let ancient_fav = t.add_text("ancient fav", now - 100 * DAY_MS + 1);
        let ancient_in_folder = t.add_text("ancient in folder", now - 100 * DAY_MS + 2);
        t.set_favorite(&[ancient_fav], true).unwrap();
        let f = t.folder_create("F").unwrap();
        t.set_folder(&[ancient_in_folder], Some(f.id)).unwrap();
        let recent: Vec<i64> = (0..10).map(|i| t.add_text(&format!("recent {i}"), now - 1_000 + i)).collect();
        // Age: 3 months.
        let gone = t.run_retention(&storage(Retention::Month3, 0, 0)).unwrap();
        assert_eq!(gone, vec![ancient]);
        // Forever + count: keep the newest 4 history records.
        let gone = t.run_retention(&storage(Retention::Forever, 4, 0)).unwrap();
        assert_eq!(gone, recent[..6].to_vec());
        assert!(live(&t).contains(&ancient_fav) && live(&t).contains(&ancient_in_folder));
        // Size: the four "recent N" texts are 8 bytes each; an older 1.1 MB record pushes the
        // history over a 1 MB budget → the oldest go first, only as many as needed.
        let big = t.insert_clip(text_clip(&"z".repeat(1_100_000), now - 5_000)).unwrap().id;
        let big_fav = t.insert_clip(text_clip(&"y".repeat(2_000_000), now - 6_000)).unwrap().id;
        t.set_favorite(&[big_fav], true).unwrap();
        let gone = t.run_retention(&storage(Retention::Forever, 0, 1)).unwrap();
        assert_eq!(gone, vec![big], "favorites don't count against the budget");
        assert_eq!(live(&t).iter().filter(|id| recent.contains(id)).count(), 4);
        let gone = t.run_retention(&storage(Retention::Forever, 0, 1)).unwrap();
        assert!(gone.is_empty(), "within budget now");
        // Nothing to do → nothing removed.
        assert!(t.run_retention(&storage(Retention::Forever, 0, 0)).unwrap().is_empty());
        assert!(live(&t).contains(&ancient_fav) && live(&t).contains(&ancient_in_folder));
    }

    #[test]
    fn retention_ms_table() {
        assert_eq!(retention_ms(Retention::Day1), Some(DAY_MS));
        assert_eq!(retention_ms(Retention::Month3), Some(90 * DAY_MS));
        assert_eq!(retention_ms(Retention::Forever), None);
    }

    #[test]
    fn vacuum_returns_free_pages() {
        let t = TestStore::new();
        let ids: Vec<i64> = (0..40).map(|i| t.add_text(&format!("{i}{}", "v".repeat(50_000)), i)).collect();
        t.hard_delete(&ids).unwrap();
        let free: i64 = t.lock().query_row("PRAGMA freelist_count", [], |r| r.get(0)).unwrap();
        assert!(free > 0);
        let released = t.vacuum_if_needed(1).unwrap();
        assert_eq!(released, free);
        let free: i64 = t.lock().query_row("PRAGMA freelist_count", [], |r| r.get(0)).unwrap();
        assert_eq!(free, 0);
    }
}
