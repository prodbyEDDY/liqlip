//! Folders (SPEC §3.9): manual order (fractional `position`), names unique
//! case-insensitively — checked in Rust with Unicode lower-casing, because SQLite's NOCASE
//! folds ASCII only (the `UNIQUE COLLATE NOCASE` column is just the ASCII backstop).

use super::lifecycle::UndoOp;
use super::write::{place_before, Placement};
use super::{db_err, is_unique_violation, Store};
use crate::model::Folder;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

pub(crate) const FOLDER_NAME_MAX: usize = 64;

/// Trimmed, 1–64 characters, no control characters.
fn valid_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > FOLDER_NAME_MAX || n.chars().any(char::is_control) {
        return Err("invalid: name".into());
    }
    Ok(n.to_string())
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// `name`, or `name 2`, `name 3`… — the first not in `taken` (case-insensitive).
pub(crate) fn unique_name(name: &str, taken: &[String]) -> String {
    let free = |n: &str| !taken.iter().any(|t| same_name(t, n));
    if free(name) {
        return name.to_string();
    }
    (2..).map(|i| format!("{name} {i}")).find(|n| free(n)).unwrap_or_else(|| name.to_string())
}

fn folder_out(conn: &Connection, id: i64) -> Result<Folder, String> {
    conn.query_row(
        "SELECT f.id, f.name, f.position,
           (SELECT count(*) FROM clips c WHERE c.folder_id = f.id AND c.deleted_at IS NULL)
         FROM folders f WHERE f.id = ?1",
        [id],
        |r| Ok(Folder { id: r.get(0)?, name: r.get(1)?, position: r.get(2)?, count: r.get(3)? }),
    )
    .optional()
    .map_err(db_err)?
    .ok_or_else(|| format!("not_found: folder {id}"))
}

fn names(conn: &Connection, except: Option<i64>) -> Result<Vec<String>, String> {
    let mut st = conn.prepare("SELECT id, name FROM folders").map_err(db_err)?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(db_err)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name) = row.map_err(db_err)?;
        if Some(id) != except {
            out.push(name);
        }
    }
    Ok(out)
}

impl Store {
    /// Folders in manual order, with live record counts.
    pub(crate) fn folders_list(&self) -> Result<Vec<Folder>, String> {
        let conn = self.lock();
        let mut st = conn
            .prepare(
                "SELECT f.id, f.name, f.position,
                   (SELECT count(*) FROM clips c WHERE c.folder_id = f.id AND c.deleted_at IS NULL)
                 FROM folders f ORDER BY f.position, f.id",
            )
            .map_err(db_err)?;
        let rows = st
            .query_map([], |r| Ok(Folder { id: r.get(0)?, name: r.get(1)?, position: r.get(2)?, count: r.get(3)? }))
            .map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    /// New folder at the end. `invalid: name` / `invalid: exists`.
    pub(crate) fn folder_create(&self, name: &str) -> Result<Folder, String> {
        let name = valid_name(name)?;
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        if names(&tx, None)?.iter().any(|n| same_name(n, &name)) {
            return Err("invalid: exists".into());
        }
        let last: Option<f64> = tx.query_row("SELECT max(position) FROM folders", [], |r| r.get(0)).map_err(db_err)?;
        tx.execute(
            "INSERT INTO folders (name, position, created_at) VALUES (?1, ?2, ?3)",
            params![name, last.map_or(1.0, |p| p.floor() + 1.0), now],
        )
        .map_err(|e| if is_unique_violation(&e) { "invalid: exists".to_string() } else { db_err(e) })?;
        let folder = folder_out(&tx, tx.last_insert_rowid())?;
        tx.commit().map_err(db_err)?;
        Ok(folder)
    }

    pub(crate) fn folder_rename(&self, id: i64, name: &str) -> Result<Folder, String> {
        let name = valid_name(name)?;
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        folder_out(&tx, id)?;
        if names(&tx, Some(id))?.iter().any(|n| same_name(n, &name)) {
            return Err("invalid: exists".into());
        }
        tx.execute("UPDATE folders SET name = ?1 WHERE id = ?2", params![name, id])
            .map_err(|e| if is_unique_violation(&e) { "invalid: exists".to_string() } else { db_err(e) })?;
        let folder = folder_out(&tx, id)?;
        tx.commit().map_err(db_err)?;
        Ok(folder)
    }

    /// Put folder `id` before `before_id` (None = at the end).
    pub(crate) fn folder_move(&self, id: i64, before_id: Option<i64>) -> Result<(), String> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        folder_out(&tx, id)?;
        if before_id == Some(id) {
            return Ok(());
        }
        let others: Vec<(i64, f64)> = {
            let mut st =
                tx.prepare("SELECT id, position FROM folders WHERE id <> ?1 ORDER BY position, id").map_err(db_err)?;
            let rows = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(db_err)?;
            rows.collect::<Result<_, _>>().map_err(db_err)?
        };
        match place_before(id, &others, before_id) {
            Err(()) => return Err(format!("not_found: folder {}", before_id.unwrap_or_default())),
            Ok(Placement::At(pos)) => {
                tx.execute("UPDATE folders SET position = ?1 WHERE id = ?2", params![pos, id]).map_err(db_err)?;
            }
            Ok(Placement::Renumber(all)) => {
                for (fid, pos) in all {
                    tx.execute("UPDATE folders SET position = ?1 WHERE id = ?2", params![pos, fid]).map_err(db_err)?;
                }
            }
        }
        tx.commit().map_err(db_err)
    }

    /// Delete a folder. Its live records stay in history (`with_clips = false`, now without
    /// a folder) or are soft-deleted with it. Returns an undo token and the member ids.
    pub(crate) fn folder_delete(&self, id: i64, with_clips: bool) -> Result<(String, Vec<i64>), String> {
        let now = self.now();
        let (folder, members) = {
            let mut conn = self.lock();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
            let folder: (String, f64, i64) = tx
                .query_row("SELECT name, position, created_at FROM folders WHERE id = ?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .optional()
                .map_err(db_err)?
                .ok_or_else(|| format!("not_found: folder {id}"))?;
            let members: Vec<i64> = {
                let mut st = tx
                    .prepare("SELECT id FROM clips WHERE folder_id = ?1 AND deleted_at IS NULL ORDER BY id")
                    .map_err(db_err)?;
                let rows = st.query_map([id], |r| r.get::<_, i64>(0)).map_err(db_err)?;
                rows.collect::<Result<_, _>>().map_err(db_err)?
            };
            if with_clips {
                tx.execute("UPDATE clips SET deleted_at = ?1 WHERE folder_id = ?2 AND deleted_at IS NULL", params![now, id])
                    .map_err(db_err)?;
            }
            // ON DELETE SET NULL detaches every member (live or soft-deleted).
            tx.execute("DELETE FROM folders WHERE id = ?1", [id]).map_err(db_err)?;
            tx.commit().map_err(db_err)?;
            (folder, members)
        };
        let (name, position, created_at) = folder;
        let token = self.push_undo(UndoOp::FolderDelete {
            id,
            name,
            position,
            created_at,
            members: members.clone(),
            revive_at: with_clips.then_some(now),
        });
        Ok((token, members))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::TestStore;
    use super::*;

    #[test]
    fn create_validates_and_rejects_duplicates_case_insensitively() {
        let t = TestStore::new();
        let a = t.folder_create("  Работа ").unwrap();
        assert_eq!((a.name.as_str(), a.count), ("Работа", 0));
        assert_eq!(t.folder_create("РАБОТА").unwrap_err(), "invalid: exists", "Cyrillic case folding");
        assert_eq!(t.folder_create("работа").unwrap_err(), "invalid: exists");
        assert_eq!(t.folder_create("").unwrap_err(), "invalid: name");
        assert_eq!(t.folder_create("   ").unwrap_err(), "invalid: name");
        assert_eq!(t.folder_create(&"x".repeat(65)).unwrap_err(), "invalid: name");
        assert_eq!(t.folder_create("a\nb").unwrap_err(), "invalid: name");
        let b = t.folder_create("Home").unwrap();
        assert!(b.position > a.position);
    }

    #[test]
    fn rename_and_errors() {
        let t = TestStore::new();
        let a = t.folder_create("One").unwrap();
        t.folder_create("Two").unwrap();
        assert_eq!(t.folder_rename(a.id, "two").unwrap_err(), "invalid: exists");
        assert_eq!(t.folder_rename(a.id, "ONE").unwrap().name, "ONE", "renaming to itself in another case");
        assert!(t.folder_rename(999, "x").unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn move_reorders() {
        let t = TestStore::new();
        let ids: Vec<i64> = ["A", "B", "C"].iter().map(|n| t.folder_create(n).unwrap().id).collect();
        let order = |t: &TestStore| t.folders_list().unwrap().into_iter().map(|f| f.id).collect::<Vec<_>>();
        t.folder_move(ids[2], Some(ids[0])).unwrap();
        assert_eq!(order(&t), vec![ids[2], ids[0], ids[1]]);
        t.folder_move(ids[2], None).unwrap();
        assert_eq!(order(&t), vec![ids[0], ids[1], ids[2]]);
        t.folder_move(ids[0], Some(ids[0])).unwrap();
        assert_eq!(order(&t), vec![ids[0], ids[1], ids[2]]);
        for _ in 0..60 {
            t.folder_move(ids[2], Some(ids[1])).unwrap();
            t.folder_move(ids[1], Some(ids[2])).unwrap();
        }
        assert_eq!(order(&t), vec![ids[0], ids[1], ids[2]]);
        assert!(t.folder_move(ids[0], Some(999)).unwrap_err().starts_with("not_found:"));
        assert!(t.folder_move(999, None).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn delete_detaches_or_deletes_members() {
        let t = TestStore::new();
        let a = t.add_text("a", 1);
        let f = t.folder_create("F").unwrap();
        t.set_folder(&[a], Some(f.id)).unwrap();
        assert_eq!(t.folders_list().unwrap()[0].count, 1);
        let (_, members) = t.folder_delete(f.id, false).unwrap();
        assert_eq!(members, vec![a]);
        assert_eq!(t.clip_detail(a).unwrap().clip.folder_id, None);
        assert!(t.folder_delete(f.id, false).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn unique_name_suffixes() {
        let taken = vec!["Keep".to_string(), "keep 2".to_string()];
        assert_eq!(unique_name("Keep", &taken), "Keep 3");
        assert_eq!(unique_name("Other", &taken), "Other");
    }
}
