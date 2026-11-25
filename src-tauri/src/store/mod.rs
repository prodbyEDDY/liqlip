//! Store — SQLite history, folders, favorites, snippets, media, search, retention, undo
//! (ARCHITECTURE §4). Owner: **W1-A**.
//!
//! Rules of this API (§15): `Store` methods never emit events — the caller does (it knows
//! the `ChangeOrigin`); background work (retention, purge) emits through the `AppHandle`
//! handed to [`init`]. All methods are blocking: call them from a worker thread or
//! `tauri::async_runtime::spawn_blocking`, never from the UI thread.
//!
//! Layout: `schema` (open, pragmas, migrations, corruption recovery) · `media` (files under
//! `media\`, `icons\`, thumbnails, the `liq` scheme) · `write` (capture insert, paste data,
//! record mutations) · `feed` (feed query, cursor, search, counts, detail, apps) ·
//! `lifecycle` (soft delete, undo, delete-recent, clear, purge, retention) · `folders` ·
//! `snippets` · `maint` (background thread) · `commands` (§9.6).
//!
//! Privacy: nothing here logs clipboard content — only ids, counts, sizes and error codes.

pub mod commands;
mod feed;
mod folders;
mod lifecycle;
mod maint;
mod media;
mod schema;
mod snippets;
#[cfg(test)]
mod testutil;
mod write;

use crate::model::{ClipMeta, Format, Kind};
use parking_lot::{Mutex, MutexGuard};
use rusqlite::{Connection, ErrorCode};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Source application of a capture (`apps` row, upserted by exe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceApp {
    /// Full path, lower-cased (`apps.exe`).
    pub exe: String,
    /// FileDescription from the version resource, else the file name without `.exe`.
    pub name: String,
}

/// A CF_HDROP file list as captured / as it must be written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipFiles {
    /// At most `content::FILES_MAX` paths, original case and order.
    pub paths: Vec<String>,
    /// `Preferred DropEffect` (DROPEFFECT_COPY = 1, MOVE = 2, LINK = 4); 1 if absent.
    pub drop_effect: u32,
}

/// What capture hands to the store: everything already decoded, classified, hashed
/// (`content::content_hash` / `content::image_hash`) and measured (`content::text_metrics`).
/// The store does no clipboard work and no classification.
///
/// Store-side normalisation (W1-A): text longer than `content::TEXT_MAX_BYTES` and file
/// lists longer than `content::FILES_MAX` are cut and re-hashed with the same `content.rs`
/// functions; `html`/`rtf` above 2 MB are dropped; `size` is recomputed from the bytes the
/// store actually persists (text + rich formats + full PNG, §4.2) — the field is advisory.
#[allow(dead_code)] // W1-B builds it (`size` is advisory, see above)
#[derive(Debug, Clone)]
pub struct NewClip {
    pub kind: Kind,
    /// `content::*hash` — the dedup key (`clips.hash UNIQUE`).
    pub hash: String,
    /// `content::make_preview` of the text; for images "" ; for files the first file name.
    pub preview: String,
    /// Full text for text|link|code|color|contact (≤ `content::TEXT_MAX_BYTES`, see
    /// `meta.truncated`); for file records the paths joined by `\n`; `None` for images.
    pub text: Option<String>,
    pub meta: ClipMeta,
    /// Bytes Liqlip itself stores for this record (text + formats + PNG), never the size of
    /// external files.
    pub size: i64,
    pub char_count: Option<i64>,
    pub word_count: Option<i64>,
    pub line_count: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_count: Option<i64>,
    /// Full PNG (kind = image: the record itself; text kinds: Office/Excel range image kept
    /// for "Paste as… PNG"). Written to `media\<id>.png` + a `clip_formats` 'png' row.
    pub png: Option<Vec<u8>>,
    /// Thumbnail PNG (≤ 360 px long side) → `media\thumbs\<id>.png`. Images only.
    pub thumb_png: Option<Vec<u8>>,
    /// HTML from the CF_HTML payload with the header stripped (StartHTML..EndHTML, UTF-8,
    /// fragment markers kept). The paste writer rebuilds the header.
    pub html: Option<String>,
    /// Raw "Rich Text Format" bytes.
    pub rtf: Option<Vec<u8>>,
    /// kind = file.
    pub files: Option<ClipFiles>,
    pub source: Option<SourceApp>,
    /// Capture time, Unix ms (`created_at` of a new row / new `copied_at` of a bumped one).
    pub copied_at: i64,
}

/// Result of [`Store::insert_clip`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertOutcome {
    pub id: i64,
    /// `false` = an existing record with the same hash was bumped (and revived if it was
    /// soft-deleted) — emit `clips:changed { change: 'updated' }` instead of `'added'`.
    pub created: bool,
    /// `apps.id` of `NewClip.source`, if any.
    pub app_id: Option<i64>,
    /// The app has no icon yet → capture extracts it once and calls [`Store::set_app_icon`].
    pub app_icon_missing: bool,
}

/// Everything the paste writer needs for one record ([`Store::get_for_paste`]).
#[allow(dead_code)] // W1-A builds, W1-C reads
#[derive(Debug, Clone)]
pub struct PastePayload {
    pub id: i64,
    pub kind: Kind,
    /// Plain text (text kinds); the `\n`-joined paths for file records.
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<Vec<u8>>,
    /// `media\<id>.png` (images, or the PNG alternative of a text record).
    pub png_path: Option<PathBuf>,
    pub files: Option<ClipFiles>,
    /// Formats actually available (same list as `Clip.formats`).
    pub formats: Vec<Format>,
}

/// The single source of "now" (Unix ms) — tests swap it for a manual clock.
pub(crate) type Clock = Box<dyn Fn() -> i64 + Send + Sync>;

/// Wall clock in Unix milliseconds.
pub(crate) fn system_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Managed state (`app.manage(store)` in `setup()` step 4, before any window).
pub struct Store {
    /// One connection, serialised by a non-poisoning mutex (a panic in one command must not
    /// brick every later one).
    conn: Mutex<Connection>,
    paths: media::Paths,
    clock: Clock,
    undo: Mutex<lifecycle::UndoBook>,
    /// Wake-up channel of the background thread (set by [`init`]; `None` in tests).
    maint: Mutex<Option<mpsc::Sender<maint::Wake>>>,
}

impl Store {
    /// Open (create / migrate) `liqlip.db` in `data_dir`. Only fails on unrecoverable I/O
    /// or on a database written by a newer Liqlip; a corrupt database (and its media) is
    /// moved aside as `*.corrupt-<ms>` and recreated. `setup()` aborts start-up on `Err`.
    pub fn open(data_dir: &Path) -> Result<Store, String> {
        Self::open_with_clock(data_dir, Box::new(system_now))
    }

    pub(crate) fn open_with_clock(data_dir: &Path, clock: Clock) -> Result<Store, String> {
        let paths = media::Paths::new(data_dir);
        paths.create_dirs()?;
        let conn = schema::open_or_recover(&paths, clock())?;
        Ok(Store {
            conn: Mutex::new(conn),
            paths,
            clock,
            undo: Mutex::new(lifecycle::UndoBook::default()),
            maint: Mutex::new(None),
        })
    }

    pub(crate) fn now(&self) -> i64 {
        (self.clock)()
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    pub(crate) fn paths(&self) -> &media::Paths {
        &self.paths
    }

    /// Nudge the background thread (no-op before [`init`] / in tests).
    pub(crate) fn wake(&self, w: maint::Wake) {
        if let Some(tx) = self.maint.lock().as_ref() {
            let _ = tx.send(w);
        }
    }
}

/// `setup()` step 4, right after `manage`: background work (retention at start / hourly /
/// after inserts / on `storage.*` changes, purge of soft-deleted rows, orphan media sweep,
/// `PRAGMA optimize`). Keeps `app` for emitting `clips:changed { origin: 'retention' }`.
pub fn init(app: &AppHandle) {
    maint::start(app);
}

/// Body of the `liq` URI scheme (`http://liq.localhost/<path>`, §9.8): `/thumb/<id>`,
/// `/media/<id>`, `/icon/<appId>` → `(mime, bytes)`; `None` → 404. `path` excludes the
/// query (`?v=` is cache-busting only). Called on a blocking thread.
pub fn serve_uri(app: &AppHandle, path: &str) -> Option<(String, Vec<u8>)> {
    app.try_state::<Store>()?.serve_path(path)
}

/// Set once a query hits `SQLITE_CORRUPT` / `SQLITE_NOTADB`; the background thread then
/// drops a marker so the next start runs `PRAGMA quick_check` (and recovers if needed).
static CORRUPTION_SEEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn corruption_seen() -> bool {
    CORRUPTION_SEEN.load(Ordering::Relaxed)
}

/// Map a SQLite error to the command error convention (`busy` / `failed`). The message
/// never contains user content (rusqlite errors carry SQL text, not bound values).
pub(crate) fn db_err(e: rusqlite::Error) -> String {
    match e.sqlite_error().map(|s| s.code) {
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => format!("busy: {e}"),
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => {
            CORRUPTION_SEEN.store(true, Ordering::Relaxed);
            format!("failed: {e}")
        }
        _ => format!("failed: {e}"),
    }
}

/// `true` for a UNIQUE / PRIMARY KEY constraint violation.
pub(crate) fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(e.sqlite_error(), Some(s) if s.code == ErrorCode::ConstraintViolation)
}

/// Deduplicate ids, keeping the first occurrence order.
pub(crate) fn dedup_ids(ids: &[i64]) -> Vec<i64> {
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

/// `?,?,…` with `n` placeholders (n ≥ 1).
pub(crate) fn placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n * 2);
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push('?');
    }
    s
}

/// Ids per `IN (…)` statement (well under SQLITE_MAX_VARIABLE_NUMBER = 32766).
pub(crate) const CHUNK: usize = 500;

/// W0 probe (kept as a regression guard): the bundled SQLite has FTS5 with the `trigram`
/// tokenizer, external-content tables work, and matching is case-insensitive for Latin
/// and Cyrillic (ARCHITECTURE §0 / §4.2).
#[cfg(test)]
mod sqlite_probe {
    #[test]
    fn bundled_sqlite_has_fts5_trigram() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        let version: String = c.query_row("SELECT sqlite_version()", [], |r| r.get(0)).unwrap();
        let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap()).collect();
        // trigram needs 3.34; FTS5 'secure-delete' (used by schema v1) needs 3.42.
        assert!(parts[0] == 3 && parts[1] >= 42, "schema v1 needs SQLite ≥ 3.42, got {version}");
        c.execute_batch(
            "CREATE TABLE clips (id INTEGER PRIMARY KEY, title TEXT, text TEXT, ocr_text TEXT);
             CREATE VIRTUAL TABLE clips_fts USING fts5(
               title, text, ocr_text, content='clips', content_rowid='id', tokenize='trigram');
             INSERT INTO clips (id, text) VALUES (1, 'Привет, Мир'), (2, 'Hello WORLD'), (3, 'другое');
             INSERT INTO clips_fts (rowid, title, text, ocr_text)
               SELECT id, title, text, ocr_text FROM clips;",
        )
        .unwrap();
        let hits = |q: &str| -> Vec<i64> {
            let mut st = c.prepare("SELECT rowid FROM clips_fts WHERE clips_fts MATCH ?1 ORDER BY rowid").unwrap();
            st.query_map([q], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(hits("\"мир\""), vec![1]); // Cyrillic, case-insensitive
        assert_eq!(hits("\"ривет\""), vec![1]); // substring
        assert_eq!(hits("\"world\""), vec![2]);
        assert!(hits("\"zzz\"").is_empty());
    }
}
