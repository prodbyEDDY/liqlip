//! Bulk writes for import and backup restore (owner **W2-B**, ARCHITECTURE §16): records
//! keep their original times (an existing record is never bumped), folders are matched by
//! name, favorites keep the source's manual order, snippets are merged, a consistent
//! snapshot feeds the export, and a `replace` restore swaps the database and media with a
//! rollback. Like the rest of the store this never emits — the import module does
//! (`origin: 'import'`).

use super::folders::FOLDER_NAME_MAX;
use super::lifecycle::{retention_ms, UndoBook};
use super::maint::Wake;
use super::media::{self, is_png, media_rel};
use super::snippets::{SNIPPET_KEYWORD_MAX, SNIPPET_NAME_MAX};
use super::write::{app_exe_key, match_app, meta_json, FilesBlob, FORMAT_MAX_BYTES, TITLE_MAX_CHARS};
use super::{db_err, placeholders, schema, ClipFiles, NewClip, SourceApp, Store, CHUNK};
use crate::content::{self, FILES_MAX, IMAGE_MAX_PIXELS, TEXT_MAX_BYTES};
use crate::logx;
use crate::model::{Kind, Retention};
use image::{ImageFormat, ImageReader};
use rusqlite::{params, params_from_iter, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Schema version a restored database may have at most (`PRAGMA user_version`).
pub(crate) const SCHEMA_VERSION: i64 = schema::VERSION;
/// Prefix of the directory the current data is moved into by a `replace` restore.
pub(crate) const ASIDE_PREFIX: &str = "backup-before-restore-";
/// Written (with the aside directory's name) before a `replace` restore moves anything and
/// removed when it has finished or rolled back: found at start-up, it means the process died
/// mid-swap, and [`recover_interrupted_restore`] puts the previous data back (W3).
pub(crate) const RESTORE_MARKER: &str = "restore.pending";
/// Prefix of the restore staging folder inside the data directory (`import::backup`).
pub(crate) const STAGING_PREFIX: &str = "restore-staging-";

/// The live files a `replace` restore swaps, by name: the database trio, then the media.
fn live_entries(p: &media::Paths) -> [(PathBuf, &'static str); 5] {
    [
        (p.db.clone(), "liqlip.db"),
        (p.wal(), "liqlip.db-wal"),
        (p.data.join("liqlip.db-shm"), "liqlip.db-shm"),
        (p.media.clone(), "media"),
        (p.icons.clone(), "icons"),
    ]
}

/// Start-up (`Store::open`, before the database is opened): finish what a `replace` restore
/// interrupted by a crash or power loss left behind. With [`RESTORE_MARKER`] present, every
/// entry already moved into the aside folder goes back to its place (the database with its
/// own `-wal`/`-shm` — a restored database's sidecars are dropped — then `media`, `icons`),
/// the marker and the emptied folder are removed. Without it nothing is touched. Leftover
/// staging folders (only ever useful during a restore) are removed either way. A failed
/// move keeps the marker, so the next start tries again; the data stays in the aside folder.
pub(crate) fn recover_interrupted_restore(p: &media::Paths) {
    if let Ok(rd) = std::fs::read_dir(&p.data) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(STAGING_PREFIX) {
                remove_path(&e.path());
            }
        }
    }
    let marker = p.data.join(RESTORE_MARKER);
    let Ok(raw) = std::fs::read_to_string(&marker) else { return };
    let name = raw.trim();
    let aside = p.data.join(name);
    if !name.starts_with(ASIDE_PREFIX) || name.contains(['\\', '/']) || !aside.is_dir() {
        let _ = std::fs::remove_file(&marker); // nothing was moved yet (or a foreign file)
        return;
    }
    logx::warn("store", "an interrupted restore was found; putting the previous data back");
    let live = live_entries(p);
    let db_aside = aside.join("liqlip.db").exists();
    let mut ok = true;
    for (i, (path, entry)) in live.iter().enumerate() {
        let from = aside.join(entry);
        let in_db_group = i < 3;
        if from.exists() || (in_db_group && db_aside) {
            if path.exists() {
                remove_path(path);
            }
        }
        if from.exists() {
            if let Err(e) = rename_retry(&from, path) {
                logx::warn("store", &format!("restore recovery: {entry}: {e}"));
                ok = false;
            }
        }
    }
    if ok {
        let _ = std::fs::remove_file(&marker);
        if let Err(e) = std::fs::remove_dir(&aside) {
            logx::warn("store", &format!("restore recovery: aside folder kept: {e}"));
        }
    }
}

/// One record of an import source (Liquid Tools, a `.liqlip` backup), hashed and measured
/// with the same `content.rs` functions as a capture.
#[derive(Debug, Clone)]
pub(crate) struct ImportItem {
    /// `copied_at` = the original copy time.
    pub clip: NewClip,
    pub created_at: i64,
    /// User title (`None` = automatic).
    pub title: Option<String>,
    pub copy_count: i64,
    pub used_at: Option<i64>,
    pub use_count: i64,
    pub favorite: bool,
    /// Absolute `fav_order` of a favorite (the source's manual order); `None` = at the end.
    pub fav_order: Option<f64>,
    /// Folder name (matched case-insensitively, see [`Store::import_folders`]).
    pub folder: Option<String>,
    pub ocr_text: Option<String>,
    pub ocr_state: i64,
    /// PNG icon of `clip.source`, used only when that app has no icon yet.
    pub app_icon: Option<Vec<u8>>,
}

/// Outcome counters of one batch.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BatchStats {
    pub imported: i64,
    /// Already in Liqlip (same hash): not bumped; favorite / folder may have been added.
    pub skipped: i64,
    pub errors: i64,
}

impl std::ops::AddAssign for BatchStats {
    fn add_assign(&mut self, o: BatchStats) {
        self.imported += o.imported;
        self.skipped += o.skipped;
        self.errors += o.errors;
    }
}

/// State shared by the batches of one import run.
#[derive(Debug, Default)]
pub(crate) struct ImportSession {
    /// Records created by this run: a later duplicate inside the same source (Liquid Tools
    /// kept every non-consecutive copy as its own row) merges into them — earliest
    /// `created_at`, latest `copied_at`, counts added — instead of being dropped.
    created: HashSet<i64>,
    /// Folder name (Unicode lower-case) → id.
    folders: HashMap<String, i64>,
}

/// A snippet of a backup being merged.
#[derive(Debug, Clone)]
pub(crate) struct SnippetRow {
    pub name: String,
    pub body: String,
    pub keyword: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub use_count: i64,
}

/// A validated item plus its staged files.
struct Ready {
    item: ImportItem,
    /// `clip_formats` 'files' JSON (file records).
    files_blob: Option<Vec<u8>>,
    staged_png: Option<PathBuf>,
    staged_thumb: Option<PathBuf>,
}

/// `apps` row cache of one transaction.
struct AppRow {
    id: i64,
    exe: String,
    has_icon: bool,
}

fn no_controls(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect::<String>().trim().to_string()
}

/// Folder name as stored: no control characters, trimmed, ≤ 64 characters; `None` if empty.
pub(crate) fn folder_name(raw: &str) -> Option<String> {
    let n = truncate_chars(&no_controls(raw), FOLDER_NAME_MAX);
    (!n.is_empty()).then_some(n)
}

fn folder_key(raw: &str) -> Option<String> {
    folder_name(raw).map(|n| n.to_lowercase())
}

fn file_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['\\', '/']);
    trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed)
}

/// `clip_formats` 'files' blob → paths (shared with the backup reader).
pub(crate) fn parse_files_blob(data: &[u8]) -> Option<ClipFiles> {
    FilesBlob::parse(data)
}

/// Validate and normalise like the capture insert (`write.rs::prepare`): hash, PNG header
/// and 64 MP limit, text cut to 2 MB (re-hashed), file lists cut to 2 000 paths (re-hashed),
/// rich formats ≤ 2 MB, metrics, preview, title, counters and times.
fn normalize(mut item: ImportItem, now: i64) -> Result<Ready, String> {
    let c = &mut item.clip;
    if c.hash.is_empty() || c.hash.len() > 128 || !c.hash.is_ascii() {
        return Err("invalid: hash".into());
    }
    let mut files_blob = None;
    match c.kind {
        k if k.is_image() => {
            let png = c.png.take().filter(|b| is_png(b)).ok_or("invalid: image record without a PNG")?;
            let (w, h) = ImageReader::with_format(Cursor::new(&png), ImageFormat::Png)
                .into_dimensions()
                .map_err(|e| format!("invalid: png: {e}"))?;
            if u64::from(w) * u64::from(h) > IMAGE_MAX_PIXELS {
                return Err("invalid: image too large".into());
            }
            c.width = Some(i64::from(w));
            c.height = Some(i64::from(h));
            c.thumb_png = c.thumb_png.take().filter(|b| is_png(b));
            c.png = Some(png);
            c.preview = content::make_preview(c.preview.trim());
            c.text = None;
            c.rtf = None;
            c.files = None;
        }
        Kind::File => {
            let f = c.files.take().ok_or("invalid: file record without paths")?;
            let mut paths: Vec<String> = f.paths.into_iter().filter(|p| !p.trim().is_empty()).collect();
            if paths.is_empty() {
                return Err("invalid: file record without paths".into());
            }
            if paths.len() > FILES_MAX {
                paths.truncate(FILES_MAX);
                c.hash = content::content_hash(Kind::File, &content::canonical_files(&paths));
            }
            if c.preview.trim().is_empty() {
                c.preview = file_name(&paths[0]).to_string();
            }
            c.preview = content::make_preview(c.preview.trim());
            c.meta.dir = content::file_dir(&paths[0]); // FIX-08, same rule as the capture insert
            c.text = Some(paths.join("\n"));
            c.file_count = Some(paths.len() as i64);
            let blob = FilesBlob { drop_effect: if f.drop_effect == 0 { 1 } else { f.drop_effect }, paths };
            files_blob = Some(serde_json::to_vec(&blob).map_err(|e| format!("failed: {e}"))?);
            c.png = None;
            c.thumb_png = None;
            c.html = None;
            c.rtf = None;
            c.width = None;
            c.height = None;
        }
        _ => {
            let mut text = c.text.take().filter(|t| !t.is_empty()).ok_or("invalid: empty text")?;
            if text.len() > TEXT_MAX_BYTES {
                let (cut, _) = content::truncate_text(&text);
                text = cut.to_string();
                c.hash = content::content_hash(c.kind, content::canonical_text(&text));
                c.meta.truncated = Some(true);
                (c.char_count, c.word_count, c.line_count) = (None, None, None);
            }
            if c.char_count.is_none() || c.word_count.is_none() || c.line_count.is_none() {
                let m = content::text_metrics(&text);
                (c.char_count, c.word_count, c.line_count) = (Some(m.chars), Some(m.words), Some(m.lines));
            }
            c.preview = content::make_preview(if c.preview.is_empty() { &text } else { &c.preview });
            c.text = Some(text);
            c.png = c.png.take().filter(|b| is_png(b));
            if c.png.is_none() {
                (c.width, c.height) = (None, None);
            }
            c.thumb_png = None;
            c.files = None;
            c.file_count = None;
        }
    }
    c.html = c.html.take().filter(|h| !h.is_empty() && h.len() <= FORMAT_MAX_BYTES);
    c.rtf = c.rtf.take().filter(|r| !r.is_empty() && r.len() <= FORMAT_MAX_BYTES);
    if c.copied_at <= 0 {
        c.copied_at = now;
    }
    if item.created_at <= 0 || item.created_at > item.clip.copied_at {
        item.created_at = item.clip.copied_at;
    }
    item.title = item.title.as_deref().map(|t| truncate_chars(&no_controls(t), TITLE_MAX_CHARS)).filter(|t| !t.is_empty());
    item.copy_count = item.copy_count.max(1);
    item.use_count = item.use_count.max(0);
    // Only images have an OCR state, and only the four known values (a backup is input).
    let image = item.clip.kind.is_image();
    if !image || !(super::ocr::STATE_NONE..=super::ocr::STATE_ERROR).contains(&item.ocr_state) {
        item.ocr_state = super::ocr::STATE_NONE;
    }
    if !image {
        item.ocr_text = None;
    }
    item.app_icon = item.app_icon.filter(|b| is_png(b) && b.len() <= 1024 * 1024);
    Ok(Ready { item, files_blob, staged_png: None, staged_thumb: None })
}

/// Rename with a few short retries: a media file may be open for a moment (the `liq`
/// scheme reads files outside the DB lock).
fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for delay in [0u64, 50, 100, 200, 400, 800] {
        std::thread::sleep(Duration::from_millis(delay));
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

fn remove_path(p: &Path) {
    let res = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
    if let Err(e) = res {
        if e.kind() != std::io::ErrorKind::NotFound {
            logx::warn("store", &format!("restore: remove failed: {e}"));
        }
    }
}

/// Write `bytes` and flush them to the disk before returning (the marker must survive the
/// very power loss it is there for).
fn write_durable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

impl Store {
    /// Retention cut-off for imported history (`copied_at` below it would be deleted by the
    /// next retention pass anyway); `None` = keep forever.
    pub(crate) fn retention_cutoff(&self, retention: Retention) -> Option<i64> {
        retention_ms(retention).map(|ms| self.now() - ms)
    }

    /// Highest manual favorites position (0 if none) — the base of an import's order.
    pub(crate) fn fav_order_base(&self) -> Result<f64, String> {
        let max: f64 = self
            .lock()
            .query_row("SELECT coalesce(max(fav_order), 0) FROM clips WHERE favorite = 1 AND deleted_at IS NULL", [], |r| {
                r.get(0)
            })
            .map_err(db_err)?;
        Ok(max.floor())
    }

    /// Data directory (`liqlip.db`, `media\`, `icons\`).
    pub(crate) fn data_dir(&self) -> &Path {
        &self.paths.data
    }

    /// Create the folders that do not exist yet (case-insensitive, Unicode), appended in
    /// the given order; remembers every name → id in `session`. Returns how many were created.
    pub(crate) fn import_folders(&self, names: &[String], session: &mut ImportSession) -> Result<i64, String> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        {
            let mut st = tx.prepare("SELECT id, name FROM folders").map_err(db_err)?;
            let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(db_err)?;
            for row in rows {
                let (id, name) = row.map_err(db_err)?;
                session.folders.insert(name.to_lowercase(), id);
            }
        }
        let mut pos: f64 = tx
            .query_row("SELECT coalesce(max(position), 0) FROM folders", [], |r| r.get(0))
            .map_err(db_err)?;
        let mut created = 0;
        for raw in names {
            let Some(name) = folder_name(raw) else { continue };
            let key = name.to_lowercase();
            if session.folders.contains_key(&key) {
                continue;
            }
            pos = pos.floor() + 1.0;
            tx.execute("INSERT INTO folders (name, position, created_at) VALUES (?1, ?2, ?3)", params![name, pos, now])
                .map_err(db_err)?;
            session.folders.insert(key, tx.last_insert_rowid());
            created += 1;
        }
        tx.commit().map_err(db_err)?;
        Ok(created)
    }

    /// Insert one batch (≈ 200 records) in one transaction, each record in its own
    /// savepoint: a failing record counts as an error and the batch goes on. A record whose
    /// hash exists is skipped without touching its times (only favorite / folder are added
    /// when it has none); a duplicate of a record created by this same run merges into it.
    /// Big files are staged before the DB lock is taken, like the capture insert.
    pub(crate) fn import_batch(&self, items: Vec<ImportItem>, session: &mut ImportSession) -> Result<BatchStats, String> {
        let now = self.now();
        let mut stats = BatchStats::default();
        let mut ready: Vec<Ready> = Vec::with_capacity(items.len());
        for item in items {
            match normalize(item, now) {
                Ok(r) => ready.push(r),
                Err(e) => {
                    stats.errors += 1;
                    logx::warn("import", &format!("record rejected: {e}"));
                }
            }
        }
        if ready.is_empty() {
            return Ok(stats);
        }
        // Which hashes exist already (short read; re-checked inside the transaction).
        let present: HashSet<String> = {
            let conn = self.lock();
            let mut out = HashSet::new();
            let hashes: Vec<&str> = ready.iter().map(|r| r.item.clip.hash.as_str()).collect();
            for chunk in hashes.chunks(CHUNK) {
                let sql = format!("SELECT hash FROM clips WHERE hash IN ({})", placeholders(chunk.len()));
                let mut st = conn.prepare(&sql).map_err(db_err)?;
                let rows = st.query_map(params_from_iter(chunk), |r| r.get::<_, String>(0)).map_err(db_err)?;
                for row in rows {
                    out.insert(row.map_err(db_err)?);
                }
            }
            out
        };
        let mut staging_failed = None;
        for r in ready.iter_mut().filter(|r| !present.contains(&r.item.clip.hash)) {
            if let Some(png) = &r.item.clip.png {
                match media::stage(self.paths(), png) {
                    Ok(p) => r.staged_png = Some(p),
                    Err(e) => staging_failed = Some(e),
                }
            }
            if let Some(thumb) = &r.item.clip.thumb_png {
                match media::stage(self.paths(), thumb) {
                    Ok(p) => r.staged_thumb = Some(p),
                    Err(e) => staging_failed = Some(e),
                }
            }
            if staging_failed.is_some() {
                break;
            }
        }
        let result = match staging_failed {
            Some(e) => Err(e),
            None => self.import_tx(&mut ready, session, &mut stats),
        };
        let leftovers: Vec<PathBuf> = ready
            .iter()
            .flat_map(|r| r.staged_png.iter().chain(r.staged_thumb.iter()))
            .filter(|p| p.exists())
            .cloned()
            .collect();
        media::remove_files(&leftovers);
        let icons = result?;
        for (app_id, png) in icons {
            if let Err(e) = self.set_app_icon(app_id, &png) {
                logx::warn("import", &format!("app icon {app_id}: {e}"));
            }
        }
        if stats.imported > 0 {
            self.wake(Wake::Inserted);
        }
        Ok(stats)
    }

    /// The transaction of [`Self::import_batch`]; returns the app icons to write afterwards
    /// (an icon file named by an id that a rolled-back savepoint might hand out again must
    /// not be written inside the transaction).
    fn import_tx(
        &self,
        ready: &mut [Ready],
        session: &mut ImportSession,
        stats: &mut BatchStats,
    ) -> Result<Vec<(i64, Vec<u8>)>, String> {
        let mut conn = self.lock();
        let mut tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let mut apps = load_apps(&tx)?;
        let mut icons: Vec<(i64, Vec<u8>)> = Vec::new();
        let mut placed: Vec<PathBuf> = Vec::new();
        let mut created_now: HashSet<i64> = HashSet::new();
        let mut local = BatchStats::default();
        for r in ready.iter_mut() {
            let mut placed_here: Vec<PathBuf> = Vec::new();
            let mut icons_here: Vec<(i64, Vec<u8>)> = Vec::new();
            let res = {
                let sp = tx.savepoint().map_err(db_err)?;
                match self.import_one(&sp, r, session, &created_now, &mut apps, &mut icons_here, &mut placed_here) {
                    Ok(out) => sp.commit().map(|_| out).map_err(db_err),
                    Err(e) => Err(e), // `sp` dropped here → rolled back
                }
            };
            match res {
                Ok(Some(id)) => {
                    local.imported += 1;
                    created_now.insert(id);
                    placed.extend(placed_here);
                    icons.extend(icons_here);
                }
                Ok(None) => {
                    local.skipped += 1;
                    icons.extend(icons_here);
                }
                Err(e) => {
                    // Rolled back: undo the file moves of this record and re-read the app
                    // cache (a rolled-back app insert must not be reused).
                    media::remove_files(&placed_here);
                    local.errors += 1;
                    logx::warn("import", &format!("record failed: {e}"));
                    apps = load_apps(&tx)?;
                }
            }
        }
        if let Err(e) = tx.commit() {
            media::remove_files(&placed);
            return Err(db_err(e));
        }
        session.created.extend(created_now);
        *stats += local;
        Ok(icons)
    }

    /// One record inside its savepoint. `Ok(Some(id))` = created, `Ok(None)` = existed.
    #[allow(clippy::too_many_arguments)]
    fn import_one(
        &self,
        conn: &Connection,
        r: &mut Ready,
        session: &ImportSession,
        created_now: &HashSet<i64>,
        apps: &mut Vec<AppRow>,
        icons: &mut Vec<(i64, Vec<u8>)>,
        placed: &mut Vec<PathBuf>,
    ) -> Result<Option<i64>, String> {
        let it = &mut r.item;
        let app_id = match &it.clip.source {
            Some(src) => resolve_app(conn, src, apps, it.app_icon.take(), icons)?,
            None => None,
        };
        let folder_id = it.folder.as_deref().and_then(folder_key).and_then(|k| session.folders.get(&k).copied());
        let existing: Option<(i64, bool, Option<i64>, Option<i64>)> = conn
            .query_row(
                "SELECT id, favorite, folder_id, deleted_at FROM clips WHERE hash = ?1",
                [&it.clip.hash],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(db_err)?;
        let next_fav = |conn: &Connection| -> Result<f64, String> {
            let max: f64 = conn
                .query_row("SELECT coalesce(max(fav_order), 0) FROM clips WHERE favorite = 1 AND deleted_at IS NULL", [], |r| {
                    r.get(0)
                })
                .map_err(db_err)?;
            Ok(max.floor() + 1.0)
        };
        if let Some((id, favorite, folder, deleted_at)) = existing {
            if deleted_at.is_some() {
                return Ok(None); // deleted by the user moments ago: not revived by an import
            }
            let ours = session.created.contains(&id) || created_now.contains(&id);
            if ours {
                conn.execute(
                    "UPDATE clips SET created_at = min(created_at, ?1), copied_at = max(copied_at, ?2),
                       copy_count = copy_count + ?3, use_count = use_count + ?4 WHERE id = ?5",
                    params![it.created_at, it.clip.copied_at, it.copy_count, it.use_count, id],
                )
                .map_err(db_err)?;
            }
            if it.favorite && !favorite {
                let pos = match it.fav_order {
                    Some(p) => p,
                    None => next_fav(conn)?,
                };
                conn.execute("UPDATE clips SET favorite = 1, fav_order = ?1 WHERE id = ?2", params![pos, id])
                    .map_err(db_err)?;
            } else if it.favorite && ours {
                if let Some(p) = it.fav_order {
                    conn.execute("UPDATE clips SET fav_order = min(fav_order, ?1) WHERE id = ?2", params![p, id])
                        .map_err(db_err)?;
                }
            }
            if folder.is_none() {
                if let Some(fid) = folder_id {
                    conn.execute("UPDATE clips SET folder_id = ?1 WHERE id = ?2", params![fid, id]).map_err(db_err)?;
                }
            }
            return Ok(None);
        }
        let fav_order = match (it.favorite, it.fav_order) {
            (false, _) => None,
            (true, Some(p)) => Some(p),
            (true, None) => Some(next_fav(conn)?),
        };
        let c = &it.clip;
        conn.execute(
            "INSERT INTO clips (kind, title, preview, text, hash, char_count, word_count, line_count, width, height,
               file_count, app_id, created_at, copied_at, copy_count, used_at, use_count, favorite, fav_order,
               folder_id, ocr_text, ocr_state, meta)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
               ?22, ?23)",
            params![
                c.kind.as_str(),
                it.title,
                c.preview,
                c.text,
                c.hash,
                c.char_count,
                c.word_count,
                c.line_count,
                c.width,
                c.height,
                c.file_count,
                app_id,
                it.created_at,
                c.copied_at,
                it.copy_count,
                it.used_at,
                it.use_count,
                i64::from(it.favorite),
                fav_order,
                folder_id,
                it.ocr_text,
                it.ocr_state,
                meta_json(&c.meta),
            ],
        )
        .map_err(db_err)?;
        let id = conn.last_insert_rowid();
        if let Some(png) = &c.png {
            let dest = self.paths().media_png(id);
            media::place(r.staged_png.as_deref(), png, &dest)?;
            placed.push(dest);
            upsert_format(conn, id, "png", None, Some(&media_rel(id)), png.len())?;
        }
        if let Some(thumb) = &c.thumb_png {
            let dest = self.paths().thumb_png(id);
            media::place(r.staged_thumb.as_deref(), thumb, &dest)?;
            placed.push(dest);
        }
        if let Some(html) = &c.html {
            upsert_format(conn, id, "html", Some(html.as_bytes()), None, html.len())?;
        }
        if let Some(rtf) = &c.rtf {
            upsert_format(conn, id, "rtf", Some(rtf), None, rtf.len())?;
        }
        if let Some(files) = &r.files_blob {
            upsert_format(conn, id, "files", Some(files), None, files.len())?;
        }
        recompute_size(conn, id)?;
        Ok(Some(id))
    }

    /// Merge snippets of a backup: one with the same name and body already exists → skipped;
    /// otherwise appended at the end, its keyword kept only if free. Returns how many were added.
    pub(crate) fn merge_snippets(&self, rows: &[SnippetRow]) -> Result<i64, String> {
        if rows.is_empty() {
            return Ok(0);
        }
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let mut pos: f64 =
            tx.query_row("SELECT coalesce(max(position), 0) FROM snippets", [], |r| r.get(0)).map_err(db_err)?;
        let mut added = 0;
        for s in rows {
            let name = truncate_chars(&no_controls(&s.name), SNIPPET_NAME_MAX);
            if name.is_empty() || s.body.len() > TEXT_MAX_BYTES {
                continue;
            }
            let same: Option<i64> = tx
                .query_row("SELECT id FROM snippets WHERE name = ?1 AND body = ?2", params![name, s.body], |r| r.get(0))
                .optional()
                .map_err(db_err)?;
            if same.is_some() {
                continue;
            }
            let keyword = s
                .keyword
                .as_deref()
                .map(str::trim)
                .filter(|k| !k.is_empty() && k.chars().count() <= SNIPPET_KEYWORD_MAX && !k.chars().any(char::is_control));
            let keyword = match keyword {
                Some(k) => {
                    let taken: Option<i64> = tx
                        .query_row("SELECT id FROM snippets WHERE keyword = ?1", [k], |r| r.get(0))
                        .optional()
                        .map_err(db_err)?;
                    taken.is_none().then_some(k)
                }
                None => None,
            };
            pos = pos.floor() + 1.0;
            let created = if s.created_at > 0 { s.created_at } else { now };
            let updated = if s.updated_at >= created { s.updated_at } else { created };
            tx.execute(
                "INSERT INTO snippets (name, body, keyword, position, created_at, updated_at, use_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![name, s.body, keyword, pos, created, updated, s.use_count.max(0)],
            )
            .map_err(db_err)?;
            added += 1;
        }
        tx.commit().map_err(db_err)?;
        Ok(added)
    }

    /// Consistent single-file copy of the database for a backup: `VACUUM INTO` on a second,
    /// read-only connection (WAL: capture and the UI keep working), then the copy loses its
    /// soft-deleted records (`secure_delete`, foreign keys on, so their formats go too), is
    /// vacuumed and left in rollback-journal mode (no `-wal` beside it).
    pub(crate) fn snapshot_to(&self, dest: &Path) -> Result<(), String> {
        let _ = std::fs::remove_file(dest);
        let dest_str = dest.to_str().ok_or("failed: snapshot path is not UTF-8")?;
        {
            let src = Connection::open_with_flags(
                &self.paths.db,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(db_err)?;
            src.busy_timeout(Duration::from_millis(5000)).map_err(db_err)?;
            src.execute("VACUUM INTO ?1", [dest_str]).map_err(db_err)?;
        }
        let copy = Connection::open(dest).map_err(db_err)?;
        copy.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA secure_delete = ON;
             DELETE FROM clips WHERE deleted_at IS NOT NULL;
             VACUUM;
             PRAGMA journal_mode = DELETE;",
        )
        .map_err(db_err)?;
        copy.close().map_err(|(_, e)| db_err(e))
    }

    /// `replace` restore: swap `liqlip.db`, `media\` and `icons\` with the restored ones in
    /// `staging` (same volume as the data directory — every step is a rename). The current
    /// data is moved into `backup-before-restore-<ms>\` (only the newest such copy is kept).
    /// Any failure moves everything back and reopens the old database. `expected_clips` =
    /// `count(*)` of the restored `clips` table, checked after opening (an unusable file
    /// would otherwise be silently recreated empty by `open_or_recover`).
    pub(crate) fn replace_with(&self, staging: &Path, expected_clips: i64) -> Result<PathBuf, String> {
        self.replace_with_hook(staging, expected_clips, &|_| Ok(()))
    }

    pub(crate) fn replace_with_hook(
        &self,
        staging: &Path,
        expected_clips: i64,
        hook: &dyn Fn(&str) -> Result<(), String>,
    ) -> Result<PathBuf, String> {
        let now = self.now();
        let p = &self.paths;
        let shm = p.data.join("liqlip.db-shm");
        let live = live_entries(p);
        let aside_name = format!("{ASIDE_PREFIX}{now}");
        let aside = p.data.join(&aside_name);
        let marker = p.data.join(RESTORE_MARKER);
        let mut conn = self.lock();
        // Close the live connection (checkpoints and removes -wal/-shm) behind a placeholder.
        let placeholder = Connection::open_in_memory().map_err(db_err)?;
        let old = std::mem::replace(&mut *conn, placeholder);
        if let Err((old, e)) = old.close() {
            *conn = old;
            return Err(format!("failed: close database: {e}"));
        }
        let mut moved_aside: Vec<(PathBuf, PathBuf)> = Vec::new(); // (live, aside)
        let mut moved_in: Vec<PathBuf> = Vec::new();
        let attempt = (|| -> Result<Connection, String> {
            std::fs::create_dir_all(&aside).map_err(|e| format!("failed: create {}: {e}", aside.display()))?;
            // Before anything moves: a crash from here on is undone at the next start.
            write_durable(&marker, aside_name.as_bytes()).map_err(|e| format!("failed: write {RESTORE_MARKER}: {e}"))?;
            for (path, name) in &live {
                if path.exists() {
                    let to = aside.join(name);
                    rename_retry(path, &to).map_err(|e| format!("failed: move aside {name}: {e}"))?;
                    moved_aside.push((path.clone(), to));
                }
            }
            hook("aside")?;
            for (path, name) in &live[..1] {
                let from = staging.join(name);
                rename_retry(&from, path).map_err(|e| format!("failed: move in {name}: {e}"))?;
                moved_in.push(path.clone());
            }
            for (path, name) in &live[3..] {
                let from = staging.join(name);
                if from.exists() {
                    rename_retry(&from, path).map_err(|e| format!("failed: move in {name}: {e}"))?;
                    moved_in.push(path.clone());
                }
            }
            p.create_dirs()?;
            hook("moved")?;
            let new = schema::open_or_recover(p, now)?;
            let n: i64 = new.query_row("SELECT count(*) FROM clips", [], |r| r.get(0)).map_err(db_err)?;
            if n != expected_clips {
                return Err(format!("failed: restored database has {n} records, expected {expected_clips}"));
            }
            hook("opened")?;
            Ok(new)
        })();
        match attempt {
            Ok(new) => {
                *conn = new;
                // Ids taken before now name records of the old database (OCR jobs, store/ocr.rs).
                self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = std::fs::remove_file(&marker);
                drop(conn);
                *self.undo.lock() = UndoBook::default();
                // Keep only the newest copy of the replaced data.
                if let Ok(rd) = std::fs::read_dir(&p.data) {
                    for e in rd.flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if name.starts_with(ASIDE_PREFIX) && e.path() != aside {
                            remove_path(&e.path());
                        }
                    }
                }
                self.wake(Wake::Inserted);
                Ok(aside)
            }
            Err(e) => {
                logx::warn("store", &format!("restore failed, rolling back: {e}"));
                // Drop whatever the restore put in place (also files a failed open created),
                // then move the previous data back.
                let mut doomed = moved_in.clone();
                doomed.extend([p.wal(), shm.clone()]);
                // `open_or_recover` may have quarantined the restored files as `*.corrupt-<now>`.
                let quarantined = format!(".corrupt-{now}");
                for q in p.data.read_dir().into_iter().flatten().flatten() {
                    if q.file_name().to_string_lossy().ends_with(&quarantined) {
                        doomed.push(q.path());
                    }
                }
                for path in &doomed {
                    remove_path(path);
                }
                let mut restore_err = None;
                for (live_path, aside_path) in moved_aside.iter().rev() {
                    if live_path.exists() {
                        remove_path(live_path);
                    }
                    if let Err(err) = rename_retry(aside_path, live_path) {
                        restore_err = Some(format!("{}: {err}", live_path.display()));
                    }
                }
                let _ = p.create_dirs();
                match (restore_err, schema::open_or_recover(p, now)) {
                    (None, Ok(old)) => {
                        *conn = old;
                        let _ = std::fs::remove_file(&marker);
                        let _ = std::fs::remove_dir_all(&aside);
                        Err(e)
                    }
                    (why, reopen) => {
                        // The marker stays: the next start retries moving the data back.
                        if let Ok(c) = reopen {
                            *conn = c;
                        }
                        let detail = why.unwrap_or_else(|| "reopen failed".into());
                        logx::warn("store", &format!("restore rollback incomplete ({detail}); data kept in {}", aside.display()));
                        Err(format!("failed: restore failed and the previous data could not be fully put back ({e}); it is kept in {}", aside.display()))
                    }
                }
            }
        }
    }
}

/// `write::upsert_format` for a savepoint (`&Connection` instead of `&Transaction`).
fn upsert_format(conn: &Connection, id: i64, format: &str, data: Option<&[u8]>, path: Option<&str>, size: usize) -> Result<(), String> {
    conn.execute(
        "INSERT INTO clip_formats (clip_id, format, data, path, size) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (clip_id, format) DO UPDATE SET data = excluded.data, path = excluded.path, size = excluded.size",
        params![id, format, data, path, size as i64],
    )
    .map(|_| ())
    .map_err(db_err)
}

/// `write::recompute_size` for a savepoint: text + every `clip_formats` row (§4.2).
fn recompute_size(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE clips SET size = coalesce(length(CAST(text AS BLOB)), 0)
           + coalesce((SELECT sum(size) FROM clip_formats WHERE clip_id = ?1), 0)
         WHERE id = ?1",
        [id],
    )
    .map(|_| ())
    .map_err(db_err)
}

fn load_apps(conn: &Connection) -> Result<Vec<AppRow>, String> {
    let mut st = conn.prepare("SELECT id, exe, icon_path IS NOT NULL FROM apps").map_err(db_err)?;
    let rows = st
        .query_map([], |r| Ok(AppRow { id: r.get(0)?, exe: r.get(1)?, has_icon: r.get::<_, i64>(2)? != 0 }))
        .map_err(db_err)?;
    rows.collect::<Result<_, _>>().map_err(db_err)
}

/// Find or create the `apps` row of an imported record — the capture's matching
/// (`write::match_app`, FIX-02): exact exe first; an exe without a directory (Liquid Tools
/// stored only the file stem) matches an existing app with that file name; a full path (a
/// backup) upgrades a row known only by its file name. An existing app is never renamed.
fn resolve_app(
    conn: &Connection,
    src: &SourceApp,
    apps: &mut Vec<AppRow>,
    icon: Option<Vec<u8>>,
    icons: &mut Vec<(i64, Vec<u8>)>,
) -> Result<Option<i64>, String> {
    let exe = app_exe_key(&src.exe);
    if exe.is_empty() {
        return Ok(None);
    }
    let found = {
        let rows: Vec<(i64, &str)> = apps.iter().map(|a| (a.id, a.exe.as_str())).collect();
        match_app(&exe, &rows)
    };
    let found = match found {
        Some((id, rewrite)) => {
            if rewrite {
                conn.execute("UPDATE apps SET exe = ?1 WHERE id = ?2", params![exe, id]).map_err(db_err)?;
            }
            apps.iter_mut().find(|a| a.id == id).map(|a| {
                if rewrite {
                    a.exe = exe.clone();
                }
                (a.id, a.has_icon)
            })
        }
        None => None,
    };
    let (id, has_icon) = match found {
        Some(f) => f,
        None => {
            let name = no_controls(&src.name);
            let name = if name.is_empty() {
                let f = file_name(&exe);
                f.strip_suffix(".exe").unwrap_or(f).to_string()
            } else {
                name
            };
            conn.execute("INSERT INTO apps (exe, name) VALUES (?1, ?2)", params![exe, name]).map_err(db_err)?;
            let id = conn.last_insert_rowid();
            apps.push(AppRow { id, exe, has_icon: false });
            (id, false)
        }
    };
    if let Some(png) = icon {
        if !has_icon && !icons.iter().any(|(a, _)| *a == id) {
            icons.push((id, png));
            if let Some(a) = apps.iter_mut().find(|a| a.id == id) {
                a.has_icon = true;
            }
        }
    }
    Ok(Some(id))
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{files_clip, image_clip, png_bytes, text_clip, TestStore};
    use super::*;
    use crate::model::{ClearScope, SnippetInput};

    fn item(clip: NewClip) -> ImportItem {
        ImportItem {
            created_at: clip.copied_at,
            clip,
            title: None,
            copy_count: 1,
            used_at: None,
            use_count: 0,
            favorite: false,
            fav_order: None,
            folder: None,
            ocr_text: None,
            ocr_state: 0,
            app_icon: None,
        }
    }

    fn row<T: rusqlite::types::FromSql>(t: &TestStore, sql: &str, id: i64) -> T {
        t.lock().query_row(sql, [id], |r| r.get(0)).unwrap()
    }

    fn id_of(t: &TestStore, text: &str) -> i64 {
        t.lock().query_row("SELECT id FROM clips WHERE text = ?1", [text], |r| r.get(0)).unwrap()
    }

    #[test]
    fn batch_keeps_original_times_and_never_bumps() {
        let t = TestStore::new();
        t.set_now(50_000_000);
        let existing = t.add_text("already here", 40_000_000);
        let mut session = ImportSession::default();
        let mut old = item(text_clip("from the past", 1_000));
        old.created_at = 900;
        old.title = Some("  My\ttitle ".into());
        let stats = t
            .import_batch(vec![old, item(text_clip("already here", 2_000)), item(files_clip(&["C:\\a.txt"], 3_000))], &mut session)
            .unwrap();
        assert_eq!(stats, BatchStats { imported: 2, skipped: 1, errors: 0 });
        let id = id_of(&t, "from the past");
        assert_eq!(row::<i64>(&t, "SELECT created_at FROM clips WHERE id = ?1", id), 900);
        assert_eq!(row::<i64>(&t, "SELECT copied_at FROM clips WHERE id = ?1", id), 1_000);
        assert_eq!(row::<String>(&t, "SELECT title FROM clips WHERE id = ?1", id), "My title");
        // The existing record is untouched (no bump, no copy_count).
        assert_eq!(row::<i64>(&t, "SELECT copied_at FROM clips WHERE id = ?1", existing), 40_000_000);
        assert_eq!(row::<i64>(&t, "SELECT copy_count FROM clips WHERE id = ?1", existing), 1);
        // Re-running the same import changes nothing.
        let again = t
            .import_batch(vec![item(text_clip("from the past", 1_000)), item(files_clip(&["C:\\a.txt"], 3_000))], &mut ImportSession::default())
            .unwrap();
        assert_eq!(again, BatchStats { imported: 0, skipped: 2, errors: 0 });
        assert_eq!(t.counts().unwrap().all, 3);
        // FTS follows the inserted text.
        let hits: i64 =
            t.lock().query_row("SELECT count(*) FROM clips_fts WHERE clips_fts MATCH '\"past\"'", [], |r| r.get(0)).unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn duplicates_inside_one_run_merge_into_one_record() {
        let t = TestStore::new();
        let mut session = ImportSession::default();
        t.import_batch(vec![item(text_clip("twice", 1_000))], &mut session).unwrap();
        let s = t.import_batch(vec![item(text_clip("twice", 5_000)), item(text_clip("twice", 3_000))], &mut session).unwrap();
        assert_eq!(s, BatchStats { imported: 0, skipped: 2, errors: 0 });
        let id = id_of(&t, "twice");
        assert_eq!(row::<i64>(&t, "SELECT created_at FROM clips WHERE id = ?1", id), 1_000);
        assert_eq!(row::<i64>(&t, "SELECT copied_at FROM clips WHERE id = ?1", id), 5_000);
        assert_eq!(row::<i64>(&t, "SELECT copy_count FROM clips WHERE id = ?1", id), 3);
        // Same batch.
        let s = t.import_batch(vec![item(text_clip("pair", 10)), item(text_clip("pair", 20))], &mut session).unwrap();
        assert_eq!(s, BatchStats { imported: 1, skipped: 1, errors: 0 });
        assert_eq!(row::<i64>(&t, "SELECT copy_count FROM clips WHERE id = ?1", id_of(&t, "pair")), 2);
    }

    #[test]
    fn favorites_and_folders_are_added_but_times_kept() {
        let t = TestStore::new();
        let a = t.add_text("plain", 1_000);
        let f = t.folder_create("Work").unwrap();
        let mut session = ImportSession::default();
        assert_eq!(t.import_folders(&["work".into(), "Личное".into(), "  ".into(), "x\u{1}y".into()], &mut session).unwrap(), 2);
        assert_eq!(t.import_folders(&["ЛИЧНОЕ".into()], &mut session).unwrap(), 0, "case-insensitive, Unicode");
        let base = t.fav_order_base().unwrap();
        let mut dup = item(text_clip("plain", 9_000));
        dup.favorite = true;
        dup.fav_order = Some(base + 1.0);
        dup.folder = Some("WORK".into());
        let mut fav_b = item(text_clip("fav b", 2_000));
        fav_b.favorite = true;
        fav_b.fav_order = Some(base + 3.0);
        fav_b.folder = Some("личное".into());
        let mut fav_a = item(text_clip("fav a", 3_000));
        fav_a.favorite = true;
        fav_a.fav_order = Some(base + 2.0);
        let s = t.import_batch(vec![dup, fav_b, fav_a], &mut session).unwrap();
        assert_eq!(s, BatchStats { imported: 2, skipped: 1, errors: 0 });
        assert_eq!(row::<i64>(&t, "SELECT favorite FROM clips WHERE id = ?1", a), 1);
        assert_eq!(row::<i64>(&t, "SELECT folder_id FROM clips WHERE id = ?1", a), f.id);
        assert_eq!(row::<i64>(&t, "SELECT copied_at FROM clips WHERE id = ?1", a), 1_000, "not bumped");
        let order: Vec<String> = {
            let c = t.lock();
            let mut st = c.prepare("SELECT text FROM clips WHERE favorite = 1 ORDER BY fav_order, id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(order, vec!["plain", "fav a", "fav b"]);
        let names: Vec<String> = t.folders_list().unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["Work", "Личное", "x y"]);
        // A folder already set is not changed.
        let mut other = item(text_clip("plain", 1));
        other.folder = Some("Личное".into());
        t.import_batch(vec![other], &mut ImportSession::default()).unwrap();
        assert_eq!(row::<i64>(&t, "SELECT folder_id FROM clips WHERE id = ?1", a), f.id);
    }

    #[test]
    fn images_files_apps_and_errors() {
        let t = TestStore::new();
        let mut session = ImportSession::default();
        let mut img = item(image_clip(500, 250, 1_000, false));
        img.clip.thumb_png = Some(media::make_thumbnail(img.clip.png.as_ref().unwrap()).unwrap());
        img.clip.source = Some(SourceApp { exe: "chrome.exe".into(), name: "chrome".into() });
        img.app_icon = Some(png_bytes(16, 16));
        let mut bad = item(image_clip(4, 4, 2_000, false));
        bad.clip.png = Some(b"not a png".to_vec());
        let mut empty = item(text_clip("x", 3_000));
        empty.clip.text = Some(String::new());
        let mut txt = item(text_clip("from chrome too", 4_000));
        txt.clip.source = Some(SourceApp { exe: "CHROME.exe".into(), name: "chrome".into() });
        let s = t.import_batch(vec![img, bad, empty, txt], &mut session).unwrap();
        assert_eq!(s, BatchStats { imported: 2, skipped: 0, errors: 2 });
        let img_id: i64 = t.lock().query_row("SELECT id FROM clips WHERE kind = 'image'", [], |r| r.get(0)).unwrap();
        assert!(t.paths().media_png(img_id).exists());
        assert!(t.paths().thumb_png(img_id).exists());
        assert_eq!(t.get_for_paste(img_id).unwrap().formats, vec![crate::model::Format::Png]);
        let apps = t.apps_list().unwrap();
        assert_eq!(apps.len(), 1, "stem exe matched case-insensitively");
        assert!(apps[0].icon_url.is_some(), "icon from the source");
        // A capture app with a full path is matched by an imported bare exe.
        let full = t.add_text_from("captured", 5_000, "c:\\program files\\app\\editor.exe");
        let mut from_editor = item(text_clip("imported from editor", 6_000));
        from_editor.clip.source = Some(SourceApp { exe: "editor.exe".into(), name: "editor".into() });
        t.import_batch(vec![from_editor], &mut session).unwrap();
        let editor_app: i64 = row(&t, "SELECT app_id FROM clips WHERE id = ?1", id_of(&t, "imported from editor"));
        assert_eq!(Some(editor_app), full.app_id);
        // No staging leftovers.
        let staging = std::fs::read_dir(&t.paths().media)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(".staging-"))
            .count();
        assert_eq!(staging, 0);
    }

    /// FIX-02, the merge direction: a backup's full path upgrades an app known only by its
    /// file name (an earlier Liquid Tools import) instead of adding a second row.
    #[test]
    fn full_path_upgrades_a_bare_app() {
        let apps = |t: &TestStore| -> Vec<String> {
            let c = t.lock();
            let mut st = c.prepare("SELECT exe FROM apps ORDER BY id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        let from = |text: &str, at: i64, exe: &str| {
            let mut it = item(text_clip(text, at));
            it.clip.source = Some(SourceApp { exe: exe.into(), name: "Editor".into() });
            it
        };
        let t = TestStore::new();
        let mut session = ImportSession::default();
        t.import_batch(vec![from("from liquid tools", 1_000, "editor.exe")], &mut session).unwrap();
        t.import_batch(vec![from("from a backup", 2_000, "C:\\Apps\\Editor.exe")], &mut session).unwrap();
        assert_eq!(apps(&t), vec!["c:\\apps\\editor.exe"]);
        // Inside one batch too: the transaction's app cache follows the rewrite.
        let t = TestStore::new();
        let batch = vec![
            from("a", 1_000, "editor.exe"),
            from("b", 2_000, "c:/apps/editor.exe"),
            from("c", 3_000, "EDITOR.EXE"),
        ];
        let s = t.import_batch(batch, &mut ImportSession::default()).unwrap();
        assert_eq!(s.imported, 3);
        assert_eq!(apps(&t), vec!["c:\\apps\\editor.exe"]);
        let distinct: i64 = row(&t, "SELECT count(DISTINCT app_id) FROM clips WHERE ?1 > 0", 1);
        assert_eq!(distinct, 1);
    }

    #[test]
    fn soft_deleted_twin_is_not_revived() {
        let t = TestStore::new();
        let a = t.add_text("gone", 1_000);
        t.delete(&[a]).unwrap();
        let s = t.import_batch(vec![item(text_clip("gone", 2_000))], &mut ImportSession::default()).unwrap();
        assert_eq!(s.skipped, 1);
        assert!(row::<Option<i64>>(&t, "SELECT deleted_at FROM clips WHERE id = ?1", a).is_some());
    }

    #[test]
    fn snippets_merge_by_name_and_body() {
        let t = TestStore::new();
        t.snippet_save(&SnippetInput { id: None, name: "Sig".into(), body: "Bye".into(), keyword: Some(";s".into()) })
            .unwrap();
        let rows = vec![
            SnippetRow { name: "Sig".into(), body: "Bye".into(), keyword: None, created_at: 1, updated_at: 1, use_count: 0 },
            SnippetRow { name: "Sig".into(), body: "Other".into(), keyword: Some(";s".into()), created_at: 5, updated_at: 6, use_count: 2 },
        ];
        assert_eq!(t.merge_snippets(&rows).unwrap(), 1);
        let list = t.snippets_list("").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!((list[1].body.as_str(), list[1].keyword.as_deref(), list[1].use_count), ("Other", None, 2));
        assert_eq!(t.merge_snippets(&rows).unwrap(), 0, "idempotent");
    }

    #[test]
    fn snapshot_drops_soft_deleted_records() {
        let t = TestStore::new();
        let keep = t.add_text("keep me", 1_000);
        let mut c = text_clip("secret html", 2_000);
        c.html = Some("<b>secret</b>".into());
        let gone = t.insert_clip(c).unwrap().id;
        t.delete(&[gone]).unwrap();
        let dest = t.dir().join("snap.db");
        t.snapshot_to(&dest).unwrap();
        let snap = Connection::open(&dest).unwrap();
        let ids: Vec<i64> = {
            let mut st = snap.prepare("SELECT id FROM clips").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(ids, vec![keep]);
        let formats: i64 = snap.query_row("SELECT count(*) FROM clip_formats", [], |r| r.get(0)).unwrap();
        assert_eq!(formats, 0, "formats of dropped records go too");
        let jm: String = snap.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(jm, "delete");
        drop(snap);
        assert!(!t.dir().join("snap.db-wal").exists());
        // The live store still works.
        t.add_text("after", 3_000);
    }

    /// Restore staging with a database holding `texts` (and one image) built by another store.
    fn staging_from(t: &TestStore, texts: &[&str]) -> (PathBuf, i64) {
        let other = TestStore::new();
        for (i, s) in texts.iter().enumerate() {
            other.add_text(s, 1_000 + i as i64);
        }
        other.add_image(40, 20, 9_000, true);
        let staging = t.dir().join("restore-staging");
        std::fs::create_dir_all(&staging).unwrap();
        other.snapshot_to(&staging.join("liqlip.db")).unwrap();
        let copy_dir = |from: &Path, to: &Path| {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap().flatten() {
                if e.file_type().unwrap().is_file() {
                    std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
                }
            }
        };
        copy_dir(&other.paths().media, &staging.join("media"));
        copy_dir(&other.paths().thumbs, &staging.join("media").join("thumbs"));
        (staging, texts.len() as i64 + 1)
    }

    #[test]
    fn replace_swaps_database_and_keeps_one_aside_copy() {
        let t = TestStore::new();
        t.add_text("old data", 1_000);
        std::fs::create_dir_all(t.dir().join(format!("{ASIDE_PREFIX}1"))).unwrap();
        let (staging, n) = staging_from(&t, &["restored a", "restored b"]);
        let aside = t.replace_with(&staging, n).unwrap();
        assert!(aside.join("liqlip.db").exists());
        assert!(!t.dir().join(format!("{ASIDE_PREFIX}1")).exists(), "older copy removed");
        let texts: Vec<String> = {
            let c = t.lock();
            let mut st = c.prepare("SELECT text FROM clips WHERE kind = 'text' ORDER BY id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(texts, vec!["restored a", "restored b"]);
        let img: i64 = t.lock().query_row("SELECT id FROM clips WHERE kind = 'image'", [], |r| r.get(0)).unwrap();
        assert!(t.get_for_paste(img).unwrap().png_path.is_some(), "media came along");
        // The store keeps working (WAL, FTS, writes).
        t.add_text("written after restore", 20_000);
        assert_eq!(t.counts().unwrap().all, 4);
        let jm: String = t.lock().query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(jm, "wal");
    }

    #[test]
    fn replace_failure_rolls_back_to_the_previous_data() {
        for step in ["aside", "moved", "opened"] {
            let t = TestStore::new();
            let old = t.add_text("precious", 1_000);
            let img = t.add_image(30, 30, 2_000, true);
            let (staging, n) = staging_from(&t, &["intruder"]);
            let err = t
                .replace_with_hook(&staging, n, &|s| if s == step { Err(format!("failed: injected at {s}")) } else { Ok(()) })
                .unwrap_err();
            assert!(err.contains("injected"), "{step}: {err}");
            assert_eq!(row::<String>(&t, "SELECT text FROM clips WHERE id = ?1", old), "precious", "{step}");
            assert!(t.paths().media_png(img).exists(), "{step}: media back");
            assert_eq!(t.counts().unwrap().all, 2, "{step}");
            t.add_text("still writable", 3_000);
            let asides = std::fs::read_dir(t.dir())
                .unwrap()
                .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(ASIDE_PREFIX))
                .count();
            assert_eq!(asides, 0, "{step}: no aside dir after a clean rollback");
        }
    }

    #[test]
    fn replace_rejects_a_database_that_does_not_match() {
        let t = TestStore::new();
        t.add_text("keep", 1_000);
        let (staging, n) = staging_from(&t, &["x"]);
        let err = t.replace_with(&staging, n + 5).unwrap_err();
        assert!(err.contains("expected"), "{err}");
        assert_eq!(t.counts().unwrap().all, 1);
        // Undo tokens of the old database do not survive a successful replace.
        let (staging, n) = staging_from(&t, &["y"]);
        let token = t.clear(ClearScope::History).unwrap().unwrap();
        t.replace_with(&staging, n).unwrap();
        assert!(t.undo(&token).unwrap().is_none());
    }

    /// W3: a `replace` restore cut short by a crash (simulated by a panic at each step) is
    /// undone at the next start — the previous data comes back from the aside folder.
    #[test]
    fn interrupted_restore_is_undone_at_the_next_start() {
        for step in ["aside", "moved", "opened"] {
            let t = TestStore::new();
            let old = t.add_text("precious", 1_000);
            let img = t.add_image(30, 30, 2_000, true);
            let (staging, n) = staging_from(&t, &["intruder"]);
            std::fs::create_dir_all(t.dir().join(format!("{STAGING_PREFIX}9"))).unwrap();
            let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = t.replace_with_hook(&staging, n, &|s| if s == step { panic!("power loss at {s}") } else { Ok(()) });
            }));
            assert!(crashed.is_err(), "{step}");
            assert!(t.dir().join(RESTORE_MARKER).exists(), "{step}: the marker survives the crash");
            let t = t.reopen();
            assert_eq!(row::<String>(&t, "SELECT text FROM clips WHERE id = ?1", old), "precious", "{step}");
            assert_eq!(t.counts().unwrap().all, 2, "{step}: no restored record left");
            assert!(t.paths().media_png(img).exists(), "{step}: media back");
            assert!(!t.dir().join(RESTORE_MARKER).exists(), "{step}");
            let leftovers: Vec<String> = std::fs::read_dir(t.dir())
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(ASIDE_PREFIX) || n.starts_with(STAGING_PREFIX))
                .collect();
            assert!(leftovers.is_empty(), "{step}: {leftovers:?}");
            t.add_text("still writable", 3_000);
        }
    }

    /// Without the marker nothing is touched: an older aside copy is not "restored".
    #[test]
    fn recovery_without_a_marker_keeps_the_current_data() {
        let t = TestStore::new();
        t.add_text("current", 1_000);
        let (staging, n) = staging_from(&t, &["restored"]);
        t.replace_with(&staging, n).unwrap();
        assert!(!t.dir().join(RESTORE_MARKER).exists(), "removed after a successful restore");
        let t = t.reopen();
        let texts: Vec<String> = {
            let c = t.lock();
            let mut st = c.prepare("SELECT text FROM clips WHERE kind = 'text'").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(texts, vec!["restored"]);
        // A stale marker naming nothing usable is just dropped.
        let t = t.close();
        std::fs::write(t.dir().join(RESTORE_MARKER), "..\\elsewhere").unwrap();
        let t = t.open();
        assert!(!t.dir().join(RESTORE_MARKER).exists());
        assert_eq!(t.counts().unwrap().all, 2);
    }

    /// W3: a successful swap bumps the generation (ids taken before name other records).
    #[test]
    fn replace_bumps_the_generation_only_on_success() {
        let t = TestStore::new();
        t.add_text("x", 1_000);
        let g = t.generation();
        let (staging, n) = staging_from(&t, &["y"]);
        assert!(t.replace_with(&staging, n + 1).is_err());
        assert_eq!(t.generation(), g, "a rolled-back restore keeps the old database");
        let (staging, n) = staging_from(&t, &["z"]);
        t.replace_with(&staging, n).unwrap();
        assert_eq!(t.generation(), g + 1);
    }

    /// FIX-08 on import; a backup's OCR state is input — clamped, images only.
    #[test]
    fn import_fills_the_file_folder_and_clamps_ocr_state() {
        let t = TestStore::new();
        let mut session = ImportSession::default();
        let mut files = item(files_clip(&["D:\\Work\\Q3\\plan.xlsx", "D:\\Work\\Q3\\notes.txt"], 1_000));
        files.ocr_state = 2;
        files.ocr_text = Some("not an image".into());
        let mut img = item(image_clip(12, 12, 2_000, false));
        img.ocr_state = 9;
        t.import_batch(vec![files, img], &mut session).unwrap();
        let (meta, st, ot): (String, i64, Option<String>) = t
            .lock()
            .query_row("SELECT meta, ocr_state, ocr_text FROM clips WHERE kind = 'file'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert!(meta.contains(r#""dir":"D:\\Work\\Q3""#), "{meta}");
        assert_eq!((st, ot), (0, None));
        let st: i64 = t.lock().query_row("SELECT ocr_state FROM clips WHERE kind = 'image'", [], |r| r.get(0)).unwrap();
        assert_eq!(st, 0, "unknown OCR state → none (the catch-up queues it)");
    }
}
