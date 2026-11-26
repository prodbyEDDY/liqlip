//! Liquid Tools → Liqlip (ARCHITECTURE §16.1). Source: `%APPDATA%\com.liquidtools.app\
//! clipboard\clipboard.db` (schema: Liquid Tools `docs/library.md` §4 and
//! `src-tauri/src/clipboard/store.rs`) plus the `<id>.png` files beside it.
//!
//! **Read-only, without touching Liquid Tools at all.** Its files are only ever opened for
//! reading. The database is never opened in place: it is copied (with its `-journal` /
//! `-wal`, if any) into `%TEMP%\liqlip-lt-*` and the copy is opened. Opening the original —
//! even with `SQLITE_OPEN_READONLY` — would take a shared lock (Liquid Tools sets no busy
//! timeout, so its next capture INSERT could fail with SQLITE_BUSY) and, for a WAL database,
//! could create or write `-shm` in its folder. The copy needs no lock; a write that lands
//! mid-copy is detected (size / mtime / `-journal` before and after) and the copy retried.
//!
//! Mapping: `clipboard_entries` rows of kind text|link|code|contact|color|image|file|screenshot;
//! dictations (they are not in this database at all) and unknown kinds are skipped. Screenshots
//! (W6-B, ARCHITECTURE §18.5) → `screenshot` records without an annotation document (Liquid
//! Tools baked annotations into the picture): the file is `content_ref`, read only when it lies
//! inside Liquid Tools' screenshot cache (`%LOCALAPPDATA%\com.liquidtools.app\screenshots`); a
//! missing file, a path elsewhere or a format this build cannot decode counts as `skipped`.
//! Text kinds are re-classified with `capture::classify::classify_text` (all text
//! kinds share one hash family, so the label never affects dedup). `pinned` → favorite,
//! `category` → folder, `created_at` (ms; pre-v3 seconds lifted like Liquid Tools does) →
//! `created_at` = `copied_at`, `source_app` (an exe file stem) → app `<stem>.exe`, icons from
//! `app_icon` data URLs (Liquid Tools stores them on screenshot rows), `html` (raw CF_HTML
//! with header) → the HTML format without header + `SourceURL`. Texts that look like a
//! secret are left out when `privacy.secretPatterns` is on (`runner`, FIX-01), and so are
//! records copied from an app on `privacy.ignoredApps` (W3).

use super::runner::{self, Page, Progress, PAGE};
use super::tmp::TempDir;
use crate::capture::classify;
use crate::content::{self, FILES_MAX, IMAGE_MAX_PIXELS};
use crate::model::{ClipMeta, ImportProbe, ImportReport, Kind, ShotMeta};
use crate::store::bulk::{folder_name, ImportItem};
use crate::store::{ClipFiles, NewClip, SourceApp, Store};
use base64::Engine as _;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, RgbaImage};
use rusqlite::{Connection, OpenFlags};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Liquid Tools' bundle identifier (its data folder under `%APPDATA%`).
pub(crate) const LT_IDENTIFIER: &str = "com.liquidtools.app";
/// The kinds that are imported (everything else — dictations, unknown kinds — is skipped).
const KINDS: &str = "'text','link','code','contact','color','image','file','screenshot'";
/// `created_at` in ms: rows older than Liquid Tools v3 stored seconds (its own
/// `migrate_timestamps` threshold: 1e11 separates s from ms for 1973…5138).
const TS: &str = "(CASE WHEN created_at > 0 AND created_at < 100000000000 THEN created_at * 1000 ELSE created_at END)";
/// Largest PNG read from Liquid Tools (a 64 MP RGBA image compresses well below this).
const PNG_MAX_BYTES: u64 = 512 * 1024 * 1024;
const THUMB_MAX: u32 = 360;
/// `content_ref` capped at 2.1 M characters: always more than `content::TEXT_MAX_BYTES`
/// bytes, so the stored (cut) text is the same as from the full value, while one huge
/// Liquid Tools text cannot blow up the memory of a 200-row page.
const CONTENT_REF: &str = "substr(content_ref, 1, 2100000)";

/// `%APPDATA%\com.liquidtools.app\clipboard\clipboard.db`.
pub(crate) fn default_db(roaming: &Path) -> PathBuf {
    roaming.join(LT_IDENTIFIER).join("clipboard").join("clipboard.db")
}

/// Liquid Tools' screenshot cache: Tauri `app_cache_dir()` + `screenshots` (its
/// `screenshot/mod.rs::cache_dir`); `app_cache_dir` on Windows is `FOLDERID_LocalAppData` +
/// the identifier. Screenshot rows are read only from inside it.
pub(crate) fn shots_dir(local_app_data: &Path) -> PathBuf {
    local_app_data.join(LT_IDENTIFIER).join("screenshots")
}

/// `FOLDERID_LocalAppData` (what Tauri's `app_cache_dir` resolves on Windows). Tests never read
/// the real profile: there it is `None` (no screenshot is imported through [`run`]).
/// https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath
#[cfg(all(windows, not(test)))]
fn local_app_data() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    // SAFETY: the returned string belongs to the caller, who frees it with CoTaskMemFree (the
    // function's docs); it is copied into a PathBuf first and never used after the free.
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None).ok()?;
        let path = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
        path
    }
}

#[cfg(any(not(windows), test))]
fn local_app_data() -> Option<PathBuf> {
    None
}

/// Optional columns (added by Liquid Tools' additive migrations over time).
#[derive(Debug, Default)]
struct Cols {
    content_ref: bool,
    source_app: bool,
    pinned: bool,
    html: bool,
    category: bool,
    app_icon: bool,
}

/// The opened copy. Field order matters: the connection closes before the temp folder goes.
struct LtDb {
    conn: Connection,
    cols: Cols,
    has_categories: bool,
    /// Liquid Tools' clipboard folder (its `<id>.png` files), read-only.
    dir: PathBuf,
    _tmp: TempDir,
}

fn invalid_format() -> String {
    "invalid: unknown Liquid Tools database format".into()
}

/// Size + mtime of the database and its sidecars (`None` = absent).
fn stamp(paths: &[PathBuf]) -> Vec<Option<(u64, Option<SystemTime>)>> {
    paths.iter().map(|p| std::fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok()))).collect()
}

/// Read-only copy of one file (std opens with FILE_SHARE_READ | WRITE | DELETE, so a file
/// Liquid Tools holds open for writing can be read).
fn copy_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut src = std::fs::OpenOptions::new().read(true).open(from)?;
    let mut dst = std::fs::File::create(to)?;
    std::io::copy(&mut src, &mut dst)?;
    dst.sync_all()
}

/// Copy `db` (+ `-wal` / `-journal`; `-shm` is rebuilt from the WAL) into `dest`. Returns
/// whether a sidecar was copied (the copy then needs a writable open for recovery).
fn snapshot_copy(db: &Path, dest: &Path) -> Result<bool, String> {
    let side = |p: &Path, suffix: &str| {
        let mut s = p.as_os_str().to_owned();
        s.push(suffix);
        PathBuf::from(s)
    };
    let watched = [db.to_path_buf(), side(db, "-wal"), side(db, "-journal")];
    let mut sidecars = false;
    for attempt in 0..4u64 {
        let before = stamp(&watched);
        copy_file(db, dest).map_err(|e| format!("failed: read the Liquid Tools database: {e}"))?;
        sidecars = false;
        for suffix in ["-wal", "-journal"] {
            let (from, to) = (side(db, suffix), side(dest, suffix));
            let _ = std::fs::remove_file(&to);
            if from.is_file() {
                copy_file(&from, &to).map_err(|e| format!("failed: read the Liquid Tools database: {e}"))?;
                sidecars = true;
            }
        }
        if stamp(&watched) == before {
            return Ok(sidecars);
        }
        std::thread::sleep(Duration::from_millis(100 * (attempt + 1)));
    }
    Ok(sidecars) // still changing: SQLite recovery + quick_check judge the last copy
}

fn open(db: &Path) -> Result<LtDb, String> {
    let mut header = [0u8; 16];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(db).map_err(|e| format!("failed: open the Liquid Tools database: {e}"))?;
        if f.read_exact(&mut header).is_err() || &header != b"SQLite format 3\0" {
            return Err("invalid: not a Liquid Tools database".into());
        }
    }
    let tmp = TempDir::new("liqlip-lt-")?;
    let copy = tmp.path().join("clipboard.db");
    let sidecars = snapshot_copy(db, &copy)?;
    let flags = if sidecars {
        // Our own copy: writable so SQLite can roll back a hot journal / replay the WAL.
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let damaged = |e: rusqlite::Error| match e.sqlite_error().map(|s| s.code) {
        Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase) => {
            "invalid: the Liquid Tools database is damaged".to_string()
        }
        _ => format!("failed: Liquid Tools database: {e}"),
    };
    let conn = Connection::open_with_flags(&copy, flags).map_err(damaged)?;
    let check: String = conn.query_row("PRAGMA quick_check(1)", [], |r| r.get(0)).map_err(damaged)?;
    if check != "ok" {
        return Err("invalid: the Liquid Tools database is damaged".into());
    }
    let mut names = HashSet::new();
    {
        let mut st = conn.prepare("SELECT name FROM pragma_table_info('clipboard_entries')").map_err(damaged)?;
        let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(damaged)?;
        for row in rows {
            names.insert(row.map_err(damaged)?.to_lowercase());
        }
    }
    if !["id", "kind", "preview", "created_at"].iter().all(|c| names.contains(*c)) {
        return Err(invalid_format());
    }
    let cols = Cols {
        content_ref: names.contains("content_ref"),
        source_app: names.contains("source_app"),
        pinned: names.contains("pinned"),
        html: names.contains("html"),
        category: names.contains("category"),
        app_icon: names.contains("app_icon"),
    };
    let has_categories: bool = conn
        .query_row(
            "SELECT count(*) FROM pragma_table_info('categories') WHERE name = 'name'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(damaged)?
        > 0;
    let dir = db.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok(LtDb { conn, cols, has_categories, dir, _tmp: tmp })
}

impl LtDb {
    fn col<'a>(&self, has: bool, name: &'a str, fallback: &'a str) -> &'a str {
        if has {
            name
        } else {
            fallback
        }
    }

    /// The import filter: supported kinds; history older than the retention cut-off is left
    /// out (Liqlip's next retention pass would delete it), favorites and folder members never.
    fn filter(&self) -> String {
        let pinned = self.col(self.cols.pinned, "pinned", "0");
        let category = self.col(self.cols.category, "coalesce(category, '')", "''");
        format!("kind IN ({KINDS}) AND ({pinned} = 1 OR {category} <> '' OR {TS} >= ?1)")
    }

    fn count(&self, extra: &str, cutoff: i64) -> Result<i64, String> {
        self.conn
            .query_row(&format!("SELECT count(*) FROM clipboard_entries WHERE {} {extra}", self.filter()), [cutoff], |r| r.get(0))
            .map_err(|e| format!("failed: Liquid Tools database: {e}"))
    }

    /// Folder names: the `categories` table in its order, then names only used by rows.
    fn folders(&self) -> Result<Vec<String>, String> {
        let err = |e: rusqlite::Error| format!("failed: Liquid Tools database: {e}");
        let mut raw: Vec<String> = Vec::new();
        if self.has_categories {
            let order: bool = self
                .conn
                .query_row("SELECT count(*) FROM pragma_table_info('categories') WHERE name = 'position'", [], |r| {
                    r.get::<_, i64>(0)
                })
                .map_err(err)?
                > 0;
            let sql = if order {
                "SELECT name FROM categories ORDER BY position, name"
            } else {
                "SELECT name FROM categories ORDER BY name"
            };
            let mut st = self.conn.prepare(sql).map_err(err)?;
            let rows = st.query_map([], |r| r.get::<_, Option<String>>(0)).map_err(err)?;
            for row in rows {
                raw.extend(row.map_err(err)?);
            }
        }
        if self.cols.category {
            let mut st = self
                .conn
                .prepare(&format!(
                    "SELECT DISTINCT category FROM clipboard_entries
                     WHERE coalesce(category, '') <> '' AND kind IN ({KINDS}) ORDER BY category"
                ))
                .map_err(err)?;
            let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(err)?;
            for row in rows {
                raw.push(row.map_err(err)?);
            }
        }
        let mut seen = HashSet::new();
        Ok(raw
            .into_iter()
            .filter_map(|n| folder_name(&n))
            .filter(|n| seen.insert(n.to_lowercase()))
            .collect())
    }

    /// One icon per source app (lower-case stem → PNG) from `app_icon` data URLs.
    fn app_icons(&self) -> HashMap<String, Vec<u8>> {
        let mut out = HashMap::new();
        if !self.cols.app_icon || !self.cols.source_app {
            return out;
        }
        let Ok(mut st) = self.conn.prepare(
            "SELECT source_app, app_icon FROM clipboard_entries
             WHERE coalesce(app_icon, '') <> '' AND coalesce(source_app, '') <> '' GROUP BY lower(source_app)",
        ) else {
            return out;
        };
        let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) else {
            return out;
        };
        for (app, url) in rows.flatten() {
            if let Some(png) = data_url_png(&url) {
                out.insert(app.trim().to_lowercase(), png);
            }
        }
        out
    }

    /// Ranks in Liquid Tools' favorites pill order (`pinned = 1 ORDER BY created_at DESC`).
    fn favorite_ranks(&self, cutoff: i64) -> Result<HashMap<i64, usize>, String> {
        let mut out = HashMap::new();
        if !self.cols.pinned {
            return Ok(out);
        }
        let err = |e: rusqlite::Error| format!("failed: Liquid Tools database: {e}");
        let mut st = self
            .conn
            .prepare(&format!(
                "SELECT id FROM clipboard_entries WHERE {} AND pinned = 1 ORDER BY {TS} DESC, id DESC",
                self.filter()
            ))
            .map_err(err)?;
        let rows = st.query_map([cutoff], |r| r.get::<_, i64>(0)).map_err(err)?;
        for (rank, id) in rows.enumerate() {
            out.insert(id.map_err(err)?, rank);
        }
        Ok(out)
    }

    /// One page in `(created_at, id)` order after `after`.
    fn page(&self, cutoff: i64, after: (i64, i64)) -> Result<Vec<LtRow>, String> {
        let sql = format!(
            "SELECT id, kind, preview, {content_ref}, {source_app}, {pinned}, {html}, {category}, {TS}
             FROM clipboard_entries WHERE {filter} AND ({TS}, id) > (?2, ?3) ORDER BY {TS}, id LIMIT {PAGE}",
            content_ref = self.col(self.cols.content_ref, CONTENT_REF, "NULL"),
            source_app = self.col(self.cols.source_app, "source_app", "NULL"),
            pinned = self.col(self.cols.pinned, "pinned", "0"),
            html = self.col(self.cols.html, "html", "NULL"),
            category = self.col(self.cols.category, "coalesce(category, '')", "''"),
            filter = self.filter(),
        );
        let err = |e: rusqlite::Error| format!("failed: Liquid Tools database: {e}");
        let mut st = self.conn.prepare(&sql).map_err(err)?;
        let rows = st
            .query_map(rusqlite::params![cutoff, after.0, after.1], |r| {
                Ok(LtRow {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    preview: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    content_ref: r.get(3)?,
                    source_app: r.get(4)?,
                    pinned: r.get::<_, Option<i64>>(5)?.unwrap_or(0) != 0,
                    html: r.get(6)?,
                    category: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    ts: r.get(8)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }
}

struct LtRow {
    id: i64,
    kind: String,
    preview: String,
    content_ref: Option<String>,
    source_app: Option<String>,
    pinned: bool,
    html: Option<String>,
    category: String,
    ts: i64,
}

fn data_url_png(url: &str) -> Option<Vec<u8>> {
    let b64 = url.strip_prefix("data:image/png;base64,")?;
    let png = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    (png.starts_with(b"\x89PNG\r\n\x1a\n") && png.len() <= 1024 * 1024).then_some(png)
}

/// `StartHTML..EndHTML` of a raw CF_HTML payload (Liqlip stores HTML without the header —
/// the paste writer rebuilds it) and its `SourceURL` (learn.microsoft.com "HTML Clipboard
/// Format"). Without a context (`-1`) the fragment is wrapped in a minimal document.
fn cf_html(raw: &str) -> Option<(String, Option<String>)> {
    let raw = raw.trim_end_matches('\0');
    let mut vals: HashMap<&str, &str> = HashMap::new();
    for line in raw.lines() {
        if line.starts_with('<') {
            break;
        }
        let Some((key, value)) = line.split_once(':') else { break };
        match key.trim() {
            k @ ("Version" | "StartHTML" | "EndHTML" | "StartFragment" | "EndFragment" | "SourceURL") => {
                vals.insert(k, value.trim());
            }
            "StartSelection" | "EndSelection" => {}
            _ => break,
        }
    }
    vals.get("Version")?;
    let bytes = raw.as_bytes();
    let slice = |s: &str, e: &str| -> Option<String> {
        let s: i64 = vals.get(s)?.parse().ok()?;
        let e: i64 = vals.get(e)?.parse().ok()?;
        if s < 0 || e < 0 {
            return None;
        }
        let (s, e) = (s as usize, (e as usize).min(bytes.len()));
        (s < e).then(|| String::from_utf8_lossy(&bytes[s..e]).into_owned())
    };
    let html = match slice("StartHTML", "EndHTML") {
        Some(ctx) => ctx,
        None => format!("<html><body><!--StartFragment-->{}<!--EndFragment--></body></html>", slice("StartFragment", "EndFragment")?),
    };
    let url = vals.get("SourceURL").filter(|u| !u.is_empty() && !u.contains(char::is_whitespace)).map(|u| u.to_string());
    Some((html, url))
}

fn base_clip(kind: Kind, hash: String, copied_at: i64, source: Option<SourceApp>) -> NewClip {
    NewClip {
        kind,
        hash,
        preview: String::new(),
        text: None,
        meta: ClipMeta::default(),
        size: 0,
        char_count: None,
        word_count: None,
        line_count: None,
        width: None,
        height: None,
        file_count: None,
        png: None,
        thumb_png: None,
        html: None,
        rtf: None,
        files: None,
        source,
        copied_at,
    }
}

fn text_clip(r: &LtRow, source: Option<SourceApp>) -> Result<NewClip, String> {
    // ≤ 280 characters: `preview` is the (trimmed) text; longer: `content_ref` holds it all.
    let full = r.content_ref.as_deref().filter(|c| !c.is_empty()).unwrap_or(&r.preview);
    if full.trim().is_empty() {
        return Err("invalid: empty text".into());
    }
    let (stored, cut) = content::truncate_text(full);
    let (kind, mut meta) = classify::classify_text(stored);
    if cut {
        meta.truncated = Some(true);
    }
    let html = r.html.as_deref().and_then(cf_html);
    if meta.url.is_none() {
        if let Some(url) = html.as_ref().and_then(|h| h.1.clone()) {
            meta.host = classify::url_host(&url);
            meta.url = Some(url);
        }
    }
    let m = content::text_metrics(stored);
    let mut c = base_clip(kind, content::content_hash(kind, content::canonical_text(stored)), r.ts, source);
    c.preview = content::make_preview(stored);
    c.text = Some(stored.to_string());
    c.meta = meta;
    (c.char_count, c.word_count, c.line_count) = (Some(m.chars), Some(m.words), Some(m.lines));
    c.html = html.map(|h| h.0);
    Ok(c)
}

fn files_clip(r: &LtRow, source: Option<SourceApp>) -> Result<NewClip, String> {
    let joined = r.content_ref.as_deref().filter(|c| !c.is_empty()).unwrap_or(&r.preview);
    let mut paths: Vec<String> =
        joined.split('\n').map(|p| p.trim_end_matches('\r').to_string()).filter(|p| !p.trim().is_empty()).collect();
    paths.truncate(FILES_MAX);
    let first = paths.first().ok_or("invalid: file record without paths")?;
    let name = Path::new(first).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| first.clone());
    let ext = Path::new(first).extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| !e.is_empty());
    let mut c = base_clip(Kind::File, content::content_hash(Kind::File, &content::canonical_files(&paths)), r.ts, source);
    c.preview = content::make_preview(&name);
    c.text = Some(paths.join("\n"));
    c.meta = ClipMeta { ext, ..Default::default() };
    c.file_count = Some(paths.len() as i64);
    c.files = Some(ClipFiles { paths, drop_effect: 1 });
    Ok(c)
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out).write_image(rgba, w, h, ExtendedColorType::Rgba8).map_err(|e| format!("failed: png: {e}"))?;
    Ok(out)
}

/// The image of row `id`: `<dir>\<id>.png` (Liquid Tools' `image_path`), else the absolute
/// `content_ref` it recorded. Decoded to RGBA8 and normalised exactly like a capture
/// (all-transparent → opaque) so `content::image_hash` matches a later copy of the same
/// pixels; the PNG is copied as is (re-encoded only if the normalisation changed it).
fn image_clip(r: &LtRow, dir: &Path, source: Option<SourceApp>) -> Result<NewClip, String> {
    let mut candidates = vec![dir.join(format!("{}.png", r.id))];
    if let Some(p) = r.content_ref.as_deref().map(Path::new) {
        if p.is_absolute() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
            candidates.push(p.to_path_buf());
        }
    }
    let path = candidates.into_iter().find(|p| p.is_file()).ok_or("not_found: image file")?;
    picture_clip(Kind::Image, read_picture(&path)?, ImageFormat::Png, r.ts, source)
}

fn read_picture(path: &Path) -> Result<Vec<u8>, String> {
    let len = std::fs::metadata(path).map_err(|e| format!("failed: {e}"))?.len();
    if len > PNG_MAX_BYTES {
        return Err("invalid: image file too large".into());
    }
    std::fs::read(path).map_err(|e| format!("failed: read image: {e}"))
}

/// A picture record from a file's bytes in `format`: decoded to RGBA8 and normalised like a
/// capture; the PNG is the file itself when it is a PNG the normalisation left alone, else a
/// fresh encode (history keeps PNG only, ARCHITECTURE §18.4).
fn picture_clip(kind: Kind, bytes: Vec<u8>, format: ImageFormat, ts: i64, source: Option<SourceApp>) -> Result<NewClip, String> {
    let what = if format == ImageFormat::Png { "png" } else { "picture" };
    let (w, h) = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|e| format!("invalid: {what}: {e}"))?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > IMAGE_MAX_PIXELS {
        return Err("invalid: image size".into());
    }
    let mut rgba: RgbaImage = ImageReader::with_format(Cursor::new(&bytes), format)
        .decode()
        .map_err(|e| format!("invalid: {what}: {e}"))?
        .into_rgba8();
    let transparent = rgba.pixels().all(|p| p.0[3] == 0);
    if transparent {
        for p in rgba.pixels_mut() {
            p.0[3] = 255;
        }
    }
    let png = if transparent || format != ImageFormat::Png { encode_png(w, h, rgba.as_raw())? } else { bytes };
    let long = w.max(h);
    let thumb = if long <= THUMB_MAX {
        png.clone()
    } else {
        let scale = f64::from(THUMB_MAX) / f64::from(long);
        let tw = ((f64::from(w) * scale).round() as u32).clamp(1, THUMB_MAX);
        let th = ((f64::from(h) * scale).round() as u32).clamp(1, THUMB_MAX);
        let small = image::imageops::thumbnail(&rgba, tw, th);
        encode_png(tw, th, small.as_raw())?
    };
    let mut c = base_clip(kind, content::image_hash(w, h, rgba.as_raw()), ts, source);
    c.width = Some(i64::from(w));
    c.height = Some(i64::from(h));
    c.png = Some(png);
    c.thumb_png = Some(thumb);
    Ok(c)
}

/// The file of a screenshot row, if it can be imported: `content_ref` (else `preview`, where
/// Liquid Tools also wrote the path) as an absolute path that — with `..` and links resolved —
/// is a file inside `shots` (Liquid Tools' screenshot cache), in a format this build decodes.
/// `None` → the row is skipped (missing file, a path elsewhere, an unsupported format).
fn shot_source(r: &LtRow, shots: Option<&Path>) -> Option<(PathBuf, ImageFormat)> {
    let root = shots?.canonicalize().ok()?;
    let raw = r.content_ref.as_deref().filter(|c| !c.trim().is_empty()).unwrap_or(&r.preview);
    let path = Path::new(raw.trim());
    if !path.is_absolute() {
        return None;
    }
    let file = path.canonicalize().ok()?;
    if !file.starts_with(&root) || !file.is_file() {
        return None;
    }
    // Sniff the content (Liquid Tools saved PNG / JPG / WebP / AVIF by its format setting).
    let format = ImageReader::open(&file).ok()?.with_guessed_format().ok()?.format()?;
    format.reading_enabled().then_some((file, format))
}

fn shot_clip(path: &Path, format: ImageFormat, r: &LtRow, source: Option<SourceApp>) -> Result<NewClip, String> {
    let mut c = picture_clip(Kind::Screenshot, read_picture(path)?, format, r.ts, source)?;
    // Never saved to the screenshot folder by Liqlip; annotations, if any, are pixels already.
    c.meta.shot = Some(ShotMeta { saved_path: None, edited: false });
    Ok(c)
}

/// One row → an item; `Ok(None)` = skipped (a screenshot without an importable file).
fn to_item(
    r: &LtRow,
    dir: &Path,
    shots: Option<&Path>,
    favs: &HashMap<i64, usize>,
    icons: &HashMap<String, Vec<u8>>,
) -> Result<Option<ImportItem>, String> {
    let stem = r.source_app.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let source = stem.map(|s| SourceApp { exe: format!("{}.exe", s.to_lowercase()), name: s.to_string() });
    let clip = match r.kind.as_str() {
        "image" => image_clip(r, dir, source)?,
        "file" => files_clip(r, source)?,
        "screenshot" => match shot_source(r, shots) {
            Some((path, format)) => shot_clip(&path, format, r, source)?,
            None => return Ok(None),
        },
        _ => text_clip(r, source)?,
    };
    Ok(Some(ImportItem {
        clip,
        created_at: r.ts,
        title: None, // Liquid Tools titles are automatic (first line / file name / "image")
        copy_count: 1,
        used_at: None,
        use_count: 0,
        favorite: r.pinned,
        fav_order: favs.get(&r.id).map(|&rank| rank as f64),
        folder: (!r.category.is_empty()).then(|| r.category.clone()),
        ocr_text: None,
        ocr_state: 0,
        app_icon: stem.and_then(|s| icons.get(&s.to_lowercase()).cloned()),
    }))
}

/// `import_lt_probe`: what an import would bring (after the retention filter).
pub(crate) fn probe(db: &Path, cutoff: Option<i64>) -> Result<ImportProbe, String> {
    if !db.is_file() {
        return Ok(ImportProbe { found: false, path: None, clips: 0, favorites: 0, folders: 0 });
    }
    let lt = open(db)?;
    let cutoff = cutoff.unwrap_or(i64::MIN);
    let clips = lt.count("", cutoff)?;
    let favorites = if lt.cols.pinned { lt.count("AND pinned = 1", cutoff)? } else { 0 };
    let folders = lt.folders()?.len() as i64;
    let found = clips > 0 || folders > 0;
    Ok(ImportProbe { found, path: found.then(|| db.to_string_lossy().into_owned()), clips, favorites, folders })
}

/// `import_lt_run`. `skip_secrets` = `settings.privacy.secretPatterns`, `ignored_apps` =
/// `settings.privacy.ignoredApps` (see `runner`). Screenshots are read from Liquid Tools' cache
/// under `FOLDERID_LocalAppData` ([`shots_dir`]).
pub(crate) fn run(
    store: &Store,
    db: &Path,
    cutoff: Option<i64>,
    skip_secrets: bool,
    ignored_apps: &[String],
    progress: Progress<'_>,
) -> Result<ImportReport, String> {
    let shots = local_app_data().map(|d| shots_dir(&d));
    run_in(store, db, shots.as_deref(), cutoff, skip_secrets, ignored_apps, progress)
}

/// [`run`] with an explicit screenshot cache (`None`: every screenshot row is skipped).
pub(crate) fn run_in(
    store: &Store,
    db: &Path,
    shots: Option<&Path>,
    cutoff: Option<i64>,
    skip_secrets: bool,
    ignored_apps: &[String],
    progress: Progress<'_>,
) -> Result<ImportReport, String> {
    if !db.is_file() {
        return Err("not_found: Liquid Tools history".into());
    }
    let lt = open(db)?;
    let cutoff = cutoff.unwrap_or(i64::MIN);
    let folders = lt.folders()?;
    let total = lt.count("", cutoff)?;
    let favs = lt.favorite_ranks(cutoff)?;
    let icons = lt.app_icons();
    let mut after = (i64::MIN, i64::MIN);
    // Screenshots without an importable file never reach the runner: counted here.
    let skipped_shots = std::cell::Cell::new(0i64);
    let mut report = runner::run(store, &folders, total, skip_secrets, ignored_apps, progress, || -> Result<Option<Page>, String> {
        let rows = lt.page(cutoff, after)?;
        let Some(last) = rows.last() else { return Ok(None) };
        after = (last.ts, last.id);
        let mut page = Page::with_capacity(rows.len());
        for r in &rows {
            match to_item(r, &lt.dir, shots, &favs, &icons) {
                Ok(Some(item)) => page.push(Ok(item)),
                Ok(None) => skipped_shots.set(skipped_shots.get() + 1),
                Err(e) => page.push(Err(e)),
            }
        }
        Ok(Some(page))
    })?;
    let skipped = skipped_shots.get();
    if skipped > 0 {
        report.skipped += skipped;
        // The runner counted only the rows it was handed: end the bar full.
        progress("clips", total, total);
        crate::logx::warn("import", &format!("{skipped} screenshots skipped (file missing, outside the cache or unsupported)"));
    }
    Ok(report)
}

/// Screenshot rows (W6-B, ARCHITECTURE §17.1 / §18.5) on a synthetic Liquid Tools database and a
/// fake screenshot cache in a temporary folder — never the real profile.
#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use rusqlite::params;

    const T: i64 = 1_790_000_000_000;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x * 7 % 256) as u8, (y * 5 % 256) as u8, 90, 255]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).unwrap();
        out
    }

    fn row(id: i64, kind: &str, content_ref: Option<&Path>) -> LtRow {
        let path = content_ref.map(|p| p.to_string_lossy().into_owned());
        LtRow {
            id,
            kind: kind.into(),
            preview: path.clone().unwrap_or_default(),
            content_ref: path,
            source_app: Some("chrome".into()),
            pinned: false,
            html: None,
            category: String::new(),
            ts: T - id,
        }
    }

    /// `<tmp>/Local/com.liquidtools.app/screenshots` with `shot.png`, `shot.bmp`, `anim.gif`,
    /// `broken.png`, and `<tmp>/elsewhere.png` outside the cache.
    struct Cache {
        dir: TempDir,
        shots: PathBuf,
    }

    fn cache() -> Cache {
        let dir = TempDir::new("liqlip-lt-shots-").unwrap();
        let shots = shots_dir(&dir.path().join("Local"));
        std::fs::create_dir_all(&shots).unwrap();
        std::fs::write(shots.join("shot.png"), png(64, 40)).unwrap();
        let bmp = RgbaImage::from_fn(5, 3, |x, y| Rgba([x as u8 * 40, y as u8 * 60, 7, 255]));
        bmp.save_with_format(shots.join("shot.bmp"), ImageFormat::Bmp).unwrap();
        std::fs::write(shots.join("anim.gif"), b"GIF89a\x01\x00\x01\x00\x00\x00\x00;").unwrap();
        std::fs::write(shots.join("broken.png"), b"\x89PNG\r\n\x1a\nnot really").unwrap();
        std::fs::write(dir.path().join("elsewhere.png"), png(8, 8)).unwrap();
        Cache { dir, shots }
    }

    #[test]
    fn a_screenshot_row_becomes_a_screenshot_record() {
        let c = cache();
        let none = HashMap::new();
        let file = c.shots.join("shot.png");
        let item = to_item(&row(3, "screenshot", Some(&file)), c.dir.path(), Some(&c.shots), &HashMap::new(), &none)
            .unwrap()
            .expect("imported");
        let clip = &item.clip;
        assert_eq!(clip.kind, Kind::Screenshot);
        assert_eq!((clip.width, clip.height), (Some(64), Some(40)));
        assert_eq!(clip.png.as_deref(), Some(std::fs::read(&file).unwrap().as_slice()), "PNG copied as is");
        let rgba = ImageReader::open(&file).unwrap().decode().unwrap().into_rgba8();
        assert_eq!(clip.hash, content::image_hash(64, 40, rgba.as_raw()), "same family as the same picture copied");
        assert!(clip.thumb_png.is_some());
        assert_eq!(clip.meta.shot, Some(ShotMeta { saved_path: None, edited: false }), "no annotation document");
        assert_eq!((item.created_at, clip.copied_at), (T - 3, T - 3));
        assert_eq!(clip.source.as_ref().map(|s| s.exe.as_str()), Some("chrome.exe"));

        // Another decodable format (BMP here; JPG once `image` has `jpeg`) is stored as PNG.
        let bmp = c.shots.join("shot.bmp");
        let item = to_item(&row(4, "screenshot", Some(&bmp)), c.dir.path(), Some(&c.shots), &HashMap::new(), &none)
            .unwrap()
            .expect("imported");
        let stored = item.clip.png.unwrap();
        assert!(stored.starts_with(b"\x89PNG\r\n\x1a\n"));
        let want = ImageReader::open(&bmp).unwrap().decode().unwrap().into_rgba8();
        let got = ImageReader::with_format(Cursor::new(&stored), ImageFormat::Png).decode().unwrap().into_rgba8();
        assert_eq!(got.as_raw(), want.as_raw());
    }

    #[test]
    fn screenshots_without_an_importable_file_are_skipped() {
        let c = cache();
        let none = HashMap::new();
        let item = |r: LtRow, shots: Option<&Path>| to_item(&r, c.dir.path(), shots, &HashMap::new(), &none);
        let skipped = |r: LtRow| matches!(item(r, Some(&c.shots)), Ok(None));
        assert!(skipped(row(1, "screenshot", Some(&c.shots.join("gone.png")))), "missing file");
        assert!(skipped(row(2, "screenshot", Some(&c.dir.path().join("elsewhere.png")))), "outside the cache");
        let escape = c.shots.join("..").join("..").join("..").join("elsewhere.png");
        assert!(escape.is_file());
        assert!(skipped(row(3, "screenshot", Some(&escape))), "`..` out of the cache");
        assert!(skipped(row(4, "screenshot", Some(Path::new("shot.png")))), "relative path");
        assert!(skipped(row(5, "screenshot", Some(&c.shots.join("anim.gif")))), "a format this build cannot decode");
        assert!(skipped(row(6, "screenshot", Some(&c.shots))), "a folder");
        assert!(skipped(row(7, "screenshot", None)), "no path at all");
        assert!(matches!(item(row(8, "screenshot", Some(&c.shots.join("shot.png"))), None), Ok(None)), "no cache known");
        // A damaged file inside the cache is an unreadable record (an error), not a skip.
        assert!(item(row(9, "screenshot", Some(&c.shots.join("broken.png"))), Some(&c.shots)).is_err());
    }

    /// A Liquid Tools database: one text, one importable screenshot, two that are skipped.
    fn lt_db(c: &Cache) -> PathBuf {
        let clip_dir = c.dir.path().join("Roaming").join(LT_IDENTIFIER).join("clipboard");
        std::fs::create_dir_all(&clip_dir).unwrap();
        let db = clip_dir.join("clipboard.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE clipboard_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, title TEXT NOT NULL,
                preview TEXT NOT NULL, content_ref TEXT, source_app TEXT, created_at INTEGER NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 0, category TEXT NOT NULL DEFAULT '', app_icon TEXT);",
        )
        .unwrap();
        let ins = |kind: &str, path: &str, at: i64| {
            conn.execute(
                "INSERT INTO clipboard_entries (kind, title, preview, content_ref, created_at) VALUES (?1, 'auto', ?2, ?3, ?4)",
                params![kind, path, (kind == "screenshot").then_some(path), at],
            )
            .unwrap();
        };
        ins("text", "a note", T - 40);
        ins("screenshot", &c.shots.join("shot.png").to_string_lossy(), T - 30);
        ins("screenshot", &c.shots.join("gone.png").to_string_lossy(), T - 20);
        ins("screenshot", &c.dir.path().join("elsewhere.png").to_string_lossy(), T - 10);
        db
    }

    fn store_in(dir: &Path) -> Store {
        Store::open_with_clock(&dir.join("data"), Box::new(|| T)).unwrap()
    }

    fn quiet(_: &str, _: i64, _: i64) {}

    #[test]
    fn the_import_counts_every_screenshot_row() {
        let c = cache();
        let db = lt_db(&c);
        assert_eq!(probe(&db, None).unwrap().clips, 4, "screenshots count in the probe");
        let s = store_in(c.dir.path());
        let last = std::sync::Mutex::new((String::new(), 0, 0));
        let record = |p: &str, d: i64, t: i64| *last.lock().unwrap() = (p.to_string(), d, t);
        let r = run_in(&s, &db, Some(&c.shots), None, true, &[], &record).unwrap();
        assert_eq!(r.skipped, 2, "missing file + outside the cache: {r:?}");
        // The importable screenshot reaches the store (imported with W6-A's `screenshot`
        // support in store/bulk.rs; until then the store refuses the kind — an error).
        assert_eq!(r.imported + r.errors, 2, "{r:?}");
        assert_eq!(r.imported + r.skipped + r.errors, 4, "probe = imported + skipped + errors");
        assert_eq!(*last.lock().unwrap(), ("clips".to_string(), 4, 4), "the bar ends full");
        // Without a known cache no screenshot is read at all.
        let other = TempDir::new("liqlip-lt-nocache-").unwrap();
        let r = run_in(&store_in(other.path()), &db, None, None, true, &[], &quiet).unwrap();
        assert_eq!((r.imported, r.skipped, r.errors), (1, 3, 0), "{r:?}");
    }

    /// `store/bulk.rs::normalize` treats `Kind::Screenshot` like `Image` (W6-A, ARCHITECTURE §18.5).
    #[test]
    fn screenshots_land_as_screenshot_records_once() {
        let c = cache();
        let db = lt_db(&c);
        let s = store_in(c.dir.path());
        let r = run_in(&s, &db, Some(&c.shots), None, true, &[], &quiet).unwrap();
        assert_eq!((r.imported, r.skipped, r.errors), (2, 2, 0), "{r:?}");
        let shots: i64 = s.lock().query_row("SELECT count(*) FROM clips WHERE kind = 'screenshot'", [], |r| r.get(0)).unwrap();
        assert_eq!(shots, 1);
        let again = run_in(&s, &db, Some(&c.shots), None, true, &[], &quiet).unwrap();
        assert_eq!((again.imported, again.skipped, again.errors), (0, 4, 0), "re-import adds nothing: {again:?}");
    }
}
