//! Store — SQLite history, folders, favorites, snippets, media, search, retention, undo
//! (ARCHITECTURE §4). Owner: **W1-A**. W0 declares the inter-module API (§15) with
//! panic-free stubs so capture (W1-B) and paste (W1-C) can code against it in parallel.
//!
//! Rules of this API: `Store` methods never emit events — the caller does (it knows the
//! `ChangeOrigin`); background work (retention, purge) emits through the `AppHandle`
//! handed to [`init`]. All methods are blocking: call them from a worker thread or
//! `tauri::async_runtime::spawn_blocking`, never from the UI thread.

pub mod commands;

use crate::model::{ClipMeta, Format, Kind};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

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
#[allow(dead_code)] // W1-A reads, W1-B builds
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

/// Managed state (`app.manage(store)` in `setup()` step 4, before any window).
pub struct Store {
    // W1-A: connection (parking_lot::Mutex<rusqlite::Connection>), data/media dirs, undo map.
    _private: (),
}

impl Store {
    /// Open (create / migrate) `liqlip.db` in `data_dir`. Must only fail on unrecoverable
    /// I/O (a corrupt database is moved aside and recreated — W1-A); `setup()` aborts
    /// start-up on `Err`.
    pub fn open(data_dir: &Path) -> Result<Store, String> {
        let _ = data_dir; // W1-A
        Ok(Store { _private: () })
    }

    /// Insert a captured record, or bump the existing one with the same hash.
    #[allow(dead_code)] // W1-B calls it
    pub fn insert_clip(&self, clip: NewClip) -> Result<InsertOutcome, String> {
        let _ = clip; // W1-A
        Err("unimplemented: store::insert_clip".into())
    }

    /// Store the PNG icon of a source app (`icons\<app_id>.png`).
    #[allow(dead_code)] // W1-B calls it
    pub fn set_app_icon(&self, app_id: i64, png: &[u8]) -> Result<(), String> {
        let _ = (app_id, png); // W1-A
        Err("unimplemented: store::set_app_icon".into())
    }

    /// Data of one record for the clipboard writer. `Err("not_found: …")` for a missing or
    /// deleted id.
    #[allow(dead_code)] // W1-C calls it
    pub fn get_for_paste(&self, id: i64) -> Result<PastePayload, String> {
        let _ = id; // W1-A
        Err("unimplemented: store::get_for_paste".into())
    }

    /// Record a paste/copy from Liqlip: `used_at = now`, `use_count += 1`; with `bump`
    /// (`settings.picker.bumpOnPaste`) also `copied_at = now`. The caller emits
    /// `clips:changed { origin: 'user', change: 'updated', ids }`.
    #[allow(dead_code)] // W1-C calls it
    pub fn mark_used(&self, ids: &[i64], bump: bool) -> Result<(), String> {
        let _ = (ids, bump); // W1-A
        Err("unimplemented: store::mark_used".into())
    }

    /// Hard-delete everything copied in the last `minutes` (files included); returns the
    /// count. Shared by the `clips_delete_recent` command and the tray. The caller emits.
    pub fn delete_recent(&self, minutes: i64) -> Result<i64, String> {
        let _ = minutes; // W1-A
        Err("unimplemented: store::delete_recent".into())
    }
}

/// `setup()` step 4, right after `manage`: background work (retention at start / hourly,
/// purge of soft-deleted rows, `settings::subscribe` for `storage.*`). Keeps `app` for
/// emitting `clips:changed { origin: 'retention' }`.
pub fn init(app: &AppHandle) {
    let _ = app; // W1-A
}

/// Body of the `liq` URI scheme (`http://liq.localhost/<path>`, §9.8): `/thumb/<id>`,
/// `/media/<id>`, `/icon/<appId>` → `(mime, bytes)`; `None` → 404. `path` excludes the
/// query (`?v=` is cache-busting only). Called on a blocking thread.
pub fn serve_uri(app: &AppHandle, path: &str) -> Option<(String, Vec<u8>)> {
    let _ = (app, path); // W1-A
    None
}

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
        assert!(parts[0] == 3 && parts[1] >= 34, "trigram needs SQLite ≥ 3.34, got {version}");
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
