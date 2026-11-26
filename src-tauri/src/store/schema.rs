//! Opening `liqlip.db`: pragmas, the `liq_icontains` SQL function, migrations by
//! `PRAGMA user_version`, and recovery from a corrupt file (ARCHITECTURE §4.2).
//!
//! References: https://sqlite.org/pragma.html (auto_vacuum, journal_mode, synchronous,
//! secure_delete, journal_size_limit, optimize, quick_check, user_version),
//! https://sqlite.org/fts5.html §4.4.3 (external content + triggers), §4.3.5 (trigram),
//! §6.11 ('secure-delete'), https://sqlite.org/lang_createtrigger.html (`UPDATE OF`),
//! https://sqlite.org/autoinc.html (AUTOINCREMENT never reuses rowids),
//! https://sqlite.org/partialindex.html (partial-index usability rules).

use super::media::Paths;
use crate::logx;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, ErrorCode};
use std::path::Path;
use std::time::Duration;

/// Current schema version (`PRAGMA user_version`).
pub(crate) const VERSION: i64 = 2;

/// FTS sync triggers for `clips` (fts5.html §4.4.3). `UPDATE OF` keeps feed bumps
/// (`copied_at`, counters, flags) from re-tokenising the text. Kept separate so
/// `clear('all')` can drop/re-create the delete trigger around a bulk `DELETE`.
pub(crate) const CLIPS_FTS_DELETE_TRIGGER: &str = "
CREATE TRIGGER clips_fts_ad AFTER DELETE ON clips BEGIN
  INSERT INTO clips_fts(clips_fts, rowid, title, text, ocr_text)
    VALUES ('delete', old.id, old.title, old.text, old.ocr_text);
END;";

/// Schema v1 — ARCHITECTURE §4.2 (authoritative copy; the doc mirrors it).
const V1: &str = "
CREATE TABLE apps (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  exe       TEXT NOT NULL UNIQUE,
  name      TEXT NOT NULL,
  icon_path TEXT
);

CREATE TABLE folders (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  position   REAL NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE clips (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  kind        TEXT NOT NULL,
  title       TEXT,
  preview     TEXT NOT NULL,
  text        TEXT,
  hash        TEXT NOT NULL UNIQUE,
  size        INTEGER NOT NULL DEFAULT 0,
  char_count  INTEGER, word_count INTEGER, line_count INTEGER,
  width       INTEGER, height INTEGER,
  file_count  INTEGER,
  app_id      INTEGER REFERENCES apps(id) ON DELETE SET NULL,
  created_at  INTEGER NOT NULL,
  copied_at   INTEGER NOT NULL,
  copy_count  INTEGER NOT NULL DEFAULT 1,
  used_at     INTEGER,
  use_count   INTEGER NOT NULL DEFAULT 0,
  favorite    INTEGER NOT NULL DEFAULT 0,
  fav_order   REAL,
  folder_id   INTEGER REFERENCES folders(id) ON DELETE SET NULL,
  deleted_at  INTEGER,
  ocr_text    TEXT,
  ocr_state   INTEGER NOT NULL DEFAULT 0,
  meta        TEXT
);
-- Feed orders (§4.3) and filters (§9.3); every term of a partial index's WHERE is repeated
-- literally in the queries that use it. The planner does not treat a partial index's WHERE
-- as satisfied for covering purposes, so indexes that must cover a query carry the
-- filtered columns as trailing keys (always NULL / 0 there — a header byte each).
CREATE INDEX idx_clips_feed     ON clips(copied_at DESC, id DESC) WHERE deleted_at IS NULL;
-- favoritesFirst order; also the Favorites pill (favorite = 1 seek + small sort).
CREATE INDEX idx_clips_favfirst ON clips(favorite DESC, copied_at DESC, id DESC) WHERE deleted_at IS NULL;
-- Kind pills; covering for the per-kind counters.
CREATE INDEX idx_clips_kind     ON clips(kind, copied_at DESC, id DESC, deleted_at) WHERE deleted_at IS NULL;
-- Full (not partial): also serve the ON DELETE SET NULL foreign-key lookups.
CREATE INDEX idx_clips_folder   ON clips(folder_id, copied_at DESC, id DESC);
CREATE INDEX idx_clips_app      ON clips(app_id, copied_at DESC, id DESC);
-- Retention: exactly the deletable history, oldest first, covering (count, sum(size), ids);
-- the retention statements name it with INDEXED BY.
CREATE INDEX idx_clips_history  ON clips(copied_at, id, size, favorite, folder_id, deleted_at)
  WHERE deleted_at IS NULL AND favorite = 0 AND folder_id IS NULL;
-- Purge of soft-deleted rows; delete-recent over soft-deleted rows.
CREATE INDEX idx_clips_deleted  ON clips(deleted_at, copied_at) WHERE deleted_at IS NOT NULL;

CREATE TABLE clip_formats (
  clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
  format  TEXT NOT NULL,
  data    BLOB,
  path    TEXT,
  size    INTEGER NOT NULL,
  PRIMARY KEY (clip_id, format)
);

CREATE TABLE snippets (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL,
  body       TEXT NOT NULL,
  keyword    TEXT UNIQUE,
  position   REAL NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  use_count  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_snippets_position ON snippets(position, id);

CREATE VIRTUAL TABLE clips_fts USING fts5(
  title, text, ocr_text, content='clips', content_rowid='id', tokenize='trigram'
);
INSERT INTO clips_fts(clips_fts, rank) VALUES ('secure-delete', 1);
CREATE TRIGGER clips_fts_ai AFTER INSERT ON clips BEGIN
  INSERT INTO clips_fts(rowid, title, text, ocr_text)
    VALUES (new.id, new.title, new.text, new.ocr_text);
END;
CREATE TRIGGER clips_fts_ad AFTER DELETE ON clips BEGIN
  INSERT INTO clips_fts(clips_fts, rowid, title, text, ocr_text)
    VALUES ('delete', old.id, old.title, old.text, old.ocr_text);
END;
CREATE TRIGGER clips_fts_au AFTER UPDATE OF title, text, ocr_text ON clips BEGIN
  INSERT INTO clips_fts(clips_fts, rowid, title, text, ocr_text)
    VALUES ('delete', old.id, old.title, old.text, old.ocr_text);
  INSERT INTO clips_fts(rowid, title, text, ocr_text)
    VALUES (new.id, new.title, new.text, new.ocr_text);
END;

CREATE VIRTUAL TABLE snippets_fts USING fts5(
  name, body, content='snippets', content_rowid='id', tokenize='trigram'
);
INSERT INTO snippets_fts(snippets_fts, rank) VALUES ('secure-delete', 1);
CREATE TRIGGER snippets_fts_ai AFTER INSERT ON snippets BEGIN
  INSERT INTO snippets_fts(rowid, name, body) VALUES (new.id, new.name, new.body);
END;
CREATE TRIGGER snippets_fts_ad AFTER DELETE ON snippets BEGIN
  INSERT INTO snippets_fts(snippets_fts, rowid, name, body)
    VALUES ('delete', old.id, old.name, old.body);
END;
CREATE TRIGGER snippets_fts_au AFTER UPDATE OF name, body ON snippets BEGIN
  INSERT INTO snippets_fts(snippets_fts, rowid, name, body)
    VALUES ('delete', old.id, old.name, old.body);
  INSERT INTO snippets_fts(rowid, name, body) VALUES (new.id, new.name, new.body);
END;

PRAGMA user_version = 1;
";

/// Schema v2 (W6-A, ARCHITECTURE §18.5): the annotation document of a screenshot / an edited
/// image. `clips.kind` is a TEXT column without a CHECK, so the new kind `screenshot` needs no
/// table rebuild. `IF NOT EXISTS`: running the step twice is harmless (idempotent migration).
const V2: &str = "
CREATE TABLE IF NOT EXISTS shot_docs (
  clip_id    INTEGER PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
  doc        TEXT NOT NULL,
  has_base   INTEGER NOT NULL,
  saved_path TEXT,
  updated_at INTEGER NOT NULL
);
PRAGMA user_version = 2;
";

#[derive(Debug)]
pub(crate) enum OpenError {
    /// The file is not a usable database (move aside + recreate).
    Corrupt(String),
    /// Anything else (I/O, newer schema): start-up must fail rather than destroy data.
    Fatal(String),
}

impl OpenError {
    fn message(self) -> String {
        match self {
            OpenError::Corrupt(m) => format!("failed: database unusable: {m}"),
            OpenError::Fatal(m) => m,
        }
    }
}

impl From<rusqlite::Error> for OpenError {
    fn from(e: rusqlite::Error) -> Self {
        match e.sqlite_error().map(|s| s.code) {
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => OpenError::Corrupt(e.to_string()),
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => OpenError::Fatal(format!("busy: {e}")),
            _ => OpenError::Fatal(format!("failed: {e}")),
        }
    }
}

/// Open `paths.db`; on corruption move it (and its media) aside and start fresh.
pub(crate) fn open_or_recover(paths: &Paths, now: i64) -> Result<rusqlite::Connection, String> {
    let marker = paths.check_marker();
    let verify = marker.exists();
    match open_db(&paths.db, verify) {
        Ok(conn) => {
            if verify {
                let _ = std::fs::remove_file(&marker);
            }
            Ok(conn)
        }
        Err(OpenError::Corrupt(why)) => {
            // `why` is a SQLite message (no user content).
            logx::warn("store", &format!("database unusable ({why}); moving it aside and recreating"));
            paths.quarantine(now)?;
            let _ = std::fs::remove_file(&marker);
            open_db(&paths.db, false).map_err(OpenError::message)
        }
        Err(e) => Err(e.message()),
    }
}

fn open_db(path: &Path, verify: bool) -> Result<Connection, OpenError> {
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(3000))?;
    // Must precede the first CREATE TABLE (pragma.html#pragma_auto_vacuum); this also
    // reads page 1, so a non-database file fails here with SQLITE_NOTADB.
    let tables: i64 = conn.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))?;
    if tables == 0 {
        conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
    }
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") && !mode.eq_ignore_ascii_case("memory") {
        logx::warn("store", &format!("journal_mode is {mode}, not wal"));
    }
    // synchronous=NORMAL is durable-enough in WAL mode (a power loss may roll back the last
    // commits, never corrupt). secure_delete=FAST zeroes deleted content on b-tree pages at
    // no extra I/O; privacy actions switch to ON for their duration (lifecycle.rs).
    // journal_size_limit keeps the WAL from staying huge after a large transaction.
    conn.execute_batch(
        "PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA secure_delete = FAST;
         PRAGMA journal_size_limit = 16777216;",
    )?;
    register_functions(&conn)?;
    if verify {
        let res: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if res != "ok" {
            return Err(OpenError::Corrupt("quick_check failed".into()));
        }
    }
    migrate(&mut conn, tables)?;
    // Invariant of the manual Favorites order: every favorite has a fav_order.
    conn.execute(
        "UPDATE clips SET fav_order = id WHERE favorite = 1 AND fav_order IS NULL",
        [],
    )?;
    // Long-lived connection: recommended once at open (pragma.html#pragma_optimize).
    conn.execute_batch("PRAGMA optimize = 0x10002;")?;
    Ok(conn)
}

fn migrate(conn: &mut Connection, tables: i64) -> Result<(), OpenError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > VERSION {
        return Err(OpenError::Fatal(format!(
            "failed: database schema v{version} is newer than this build (v{VERSION})"
        )));
    }
    if version == 0 && tables > 0 {
        // Some other SQLite file sits at our path: not ours, never overwrite it in place.
        return Err(OpenError::Corrupt("unknown schema".into()));
    }
    if version < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(V1)?;
        tx.commit()?;
        logx::warn("store", "created schema v1");
    }
    // One step per version, each in its own transaction (a crash between steps resumes at the
    // next open from the version that was committed).
    if version < 2 {
        let tx = conn.transaction()?;
        tx.execute_batch(V2)?;
        tx.commit()?;
        logx::warn("store", "migrated schema to v2");
    }
    Ok(())
}

/// `liq_icontains(haystack, needle)` — Unicode case-insensitive substring test. SQLite's
/// own LIKE/lower() fold ASCII only (lang_expr.html), which would break 1–2 character
/// Cyrillic searches (the trigram index can't serve them, fts5.html §4.3.5).
fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        "liq_icontains",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let (ValueRef::Text(hay), ValueRef::Text(needle)) = (ctx.get_raw(0), ctx.get_raw(1)) else {
                return Ok(false);
            };
            Ok(icontains(&String::from_utf8_lossy(hay), &String::from_utf8_lossy(needle)))
        },
    )
}

/// Unicode-aware case-insensitive `contains`.
pub(crate) fn icontains(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    hay.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::super::testutil::TestStore;
    use super::*;

    #[test]
    fn fresh_database_has_schema_and_pragmas() {
        let t = TestStore::new();
        let c = t.lock();
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, VERSION);
        let av: i64 = c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap();
        assert_eq!(av, 2, "auto_vacuum must be INCREMENTAL");
        let jm: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(jm, "wal");
        let fk: i64 = c.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1);
        let sd: i64 = c.query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap();
        assert_eq!(sd, 2, "secure_delete must be FAST");
        let fts_sd: String = c
            .query_row("SELECT v FROM clips_fts_config WHERE k = 'secure-delete'", [], |r| r.get::<_, i64>(0).map(|v| v.to_string()))
            .unwrap();
        assert_eq!(fts_sd, "1");
    }

    #[test]
    fn reopen_keeps_data_and_version() {
        let t = TestStore::new();
        let id = t.add_text("persist me", 1_000);
        let dir = t.dir().to_path_buf();
        let t2 = t.reopen();
        assert_eq!(t2.dir(), dir.as_path());
        let c = t2.lock();
        let n: i64 = c.query_row("SELECT count(*) FROM clips WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn garbage_file_is_moved_aside_and_recreated() {
        let t = TestStore::empty_dir();
        std::fs::write(t.dir().join("liqlip.db"), vec![0x42u8; 8192]).unwrap();
        std::fs::create_dir_all(t.dir().join("media")).unwrap();
        std::fs::write(t.dir().join("media").join("7.png"), b"old").unwrap();
        let t = t.open();
        let names: Vec<String> = std::fs::read_dir(t.dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.starts_with("liqlip.db.corrupt-")), "{names:?}");
        assert!(names.iter().any(|n| n.starts_with("media.corrupt-")), "{names:?}");
        assert!(!t.dir().join("media").join("7.png").exists(), "stale media must not be reused");
        let c = t.lock();
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, VERSION);
    }

    #[test]
    fn foreign_sqlite_file_is_not_overwritten_in_place() {
        let t = TestStore::empty_dir();
        {
            let c = Connection::open(t.dir().join("liqlip.db")).unwrap();
            c.execute_batch("CREATE TABLE other (x); INSERT INTO other VALUES (1);").unwrap();
        }
        let t = t.open();
        let names: Vec<String> = std::fs::read_dir(t.dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.starts_with("liqlip.db.corrupt-")), "{names:?}");
    }

    #[test]
    fn newer_schema_refuses_to_open() {
        let t = TestStore::empty_dir();
        {
            let c = Connection::open(t.dir().join("liqlip.db")).unwrap();
            c.execute_batch("CREATE TABLE x (y); PRAGMA user_version = 99;").unwrap();
        }
        let err = t.try_open().err().expect("must fail");
        assert!(err.starts_with("failed:") && err.contains("newer"), "{err}");
        assert!(t.dir().join("liqlip.db").exists(), "a newer database must stay in place");
    }

    #[test]
    fn check_marker_triggers_quick_check_and_is_removed() {
        let t = TestStore::new();
        let t = t.close();
        std::fs::write(t.dir().join("liqlip.db.check"), b"").unwrap();
        let t = t.open();
        assert!(!t.dir().join("liqlip.db.check").exists());
    }

    /// W6-A: a v1 database (Liqlip 1.0) is migrated to v2 at open, keeps its records, and the
    /// step is idempotent.
    #[test]
    fn v1_database_migrates_to_v2() {
        let t = TestStore::empty_dir();
        {
            let c = Connection::open(t.dir().join("liqlip.db")).unwrap();
            c.execute_batch(V1).unwrap();
            c.execute(
                "INSERT INTO clips (kind, preview, text, hash, created_at, copied_at) VALUES ('text', 'old', 'old', 'h1', 1, 1)",
                [],
            )
            .unwrap();
        }
        let t = t.open();
        {
            let c = t.lock();
            let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            assert_eq!(v, 2);
            let docs: i64 = c.query_row("SELECT count(*) FROM shot_docs", [], |r| r.get(0)).unwrap();
            assert_eq!(docs, 0);
            let clips: i64 = c.query_row("SELECT count(*) FROM clips", [], |r| r.get(0)).unwrap();
            assert_eq!(clips, 1, "records survive the migration");
            c.execute_batch(V2).unwrap(); // a second run changes nothing
        }
        let t = t.reopen();
        let v: i64 = t.lock().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 2);
    }

    #[test]
    fn icontains_is_unicode_aware() {
        assert!(icontains("Привет, Мир", "мир"));
        assert!(icontains("ЁЖИК", "ёж"));
        assert!(icontains("Hello", "LL"));
        assert!(!icontains("abc", "abd"));
        assert!(icontains("x", ""));
    }
}
