//! Writes: the capture insert with global dedup (§4.2 rules), data for the paste writer,
//! usage marks, and record mutations (favorite + manual order, folder, rename, text edit,
//! text create). Every method is one transaction; files are staged outside the DB lock.

use super::feed::{clip_by_id, formats_for};
use super::lifecycle::UndoOp;
use super::maint::Wake;
use super::media::{self, media_rel};
use super::{db_err, dedup_ids, placeholders, ClipFiles, InsertOutcome, NewClip, PastePayload, SourceApp, Store, CHUNK};
use crate::content::{self, FILES_MAX, IMAGE_MAX_PIXELS, TEXT_MAX_BYTES};
use crate::logx;
use crate::model::{Clip, ClipMeta, Kind};
use image::{ImageFormat, ImageReader};
use rusqlite::{params, params_from_iter, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Rich formats (`clip_formats.data`) above this are not kept (§4.2: ≤ 2 MB).
pub(crate) const FORMAT_MAX_BYTES: usize = 2 * 1024 * 1024;
/// User titles (`clip_rename`).
pub(crate) const TITLE_MAX_CHARS: usize = 200;
/// Positions closer than this are renumbered (fractional ordering ran out of room).
const MIN_GAP: f64 = 1e-6;

/// `clip_formats.data` of a 'files' row.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FilesBlob {
    pub drop_effect: u32,
    pub paths: Vec<String>,
}

impl FilesBlob {
    pub fn parse(data: &[u8]) -> Option<ClipFiles> {
        let b: FilesBlob = serde_json::from_slice(data).ok()?;
        Some(ClipFiles { paths: b.paths, drop_effect: if b.drop_effect == 0 { 1 } else { b.drop_effect } })
    }
}

pub(crate) fn is_text_kind(kind: Kind) -> bool {
    !matches!(kind, Kind::Image | Kind::File)
}

/// A validated, normalised [`NewClip`].
struct Prepared {
    kind: Kind,
    hash: String,
    preview: String,
    text: Option<String>,
    meta: ClipMeta,
    char_count: Option<i64>,
    word_count: Option<i64>,
    line_count: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    file_count: Option<i64>,
    png: Option<Vec<u8>>,
    thumb: Option<Vec<u8>>,
    html: Option<String>,
    rtf: Option<Vec<u8>>,
    files: Option<Vec<u8>>,
    source: Option<(String, String, String)>, // exe, name for INSERT, name for UPDATE ("" = keep)
    copied_at: i64,
}

fn file_name(path: &str) -> String {
    exe_file_name(path).to_string()
}

/// The file name of an exe path (`c:\apps\code.exe` → `code.exe`; `/` ≡ `\`).
pub(crate) fn exe_file_name(exe: &str) -> &str {
    let trimmed = exe.trim_end_matches(['\\', '/']);
    trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed)
}

/// `apps.exe` as stored: trimmed, Unicode lower-case, `/` → `\`.
pub(crate) fn app_exe_key(exe: &str) -> String {
    exe.trim().to_lowercase().replace('/', "\\")
}

/// The existing `apps` row a source exe ([`app_exe_key`]) belongs to (FIX-02). Liquid Tools
/// kept only the file stem, so its import writes `<stem>.exe` while a capture knows the full
/// path; both must land on ONE row. In order:
/// 1. the same exe (case-insensitive, `/` ≡ `\`);
/// 2. a full path, and a row holding only its file name → that row, `rewrite = true` (it takes
///    the full path);
/// 3. a bare file name, and a row with a full path of that file name → the first such row.
///
/// `rows` = `(id, stored exe)`. `None` = a new app.
pub(crate) fn match_app(exe: &str, rows: &[(i64, &str)]) -> Option<(i64, bool)> {
    if let Some(&(id, _)) = rows.iter().find(|(_, s)| *s == exe).or_else(|| rows.iter().find(|(_, s)| app_exe_key(s) == exe)) {
        return Some((id, false));
    }
    let name = exe_file_name(exe);
    let bare = name.len() == exe.len();
    rows.iter().find_map(|&(id, stored)| {
        let key = app_exe_key(stored);
        let stored_name = exe_file_name(&key);
        if stored_name != name {
            return None;
        }
        match (bare, stored_name.len() == key.len()) {
            (false, true) => Some((id, true)),
            (true, false) => Some((id, false)),
            _ => None, // two different full paths: two apps
        }
    })
}

/// [`match_app`] against the table: the indexed exact lookup first, the scan only for an exe
/// that is not stored as is (normally: once per app).
fn find_app(tx: &Transaction<'_>, exe: &str) -> Result<Option<(i64, bool)>, String> {
    let exact: Option<i64> =
        tx.query_row("SELECT id FROM apps WHERE exe = ?1", [exe], |r| r.get(0)).optional().map_err(db_err)?;
    if let Some(id) = exact {
        return Ok(Some((id, false)));
    }
    let mut st = tx.prepare_cached("SELECT id, exe FROM apps ORDER BY id").map_err(db_err)?;
    let rows: Vec<(i64, String)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(db_err)?
        .collect::<Result<_, _>>()
        .map_err(db_err)?;
    let rows: Vec<(i64, &str)> = rows.iter().map(|(id, e)| (*id, e.as_str())).collect();
    Ok(match_app(exe, &rows))
}

fn prepare_source(s: SourceApp) -> Option<(String, String, String)> {
    let exe = app_exe_key(&s.exe);
    if exe.is_empty() {
        return None;
    }
    let given: String = s.name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string();
    let fallback = {
        let f = file_name(&exe);
        f.strip_suffix(".exe").map(str::to_string).unwrap_or(f)
    };
    let insert_name = if given.is_empty() { fallback } else { given.clone() };
    Some((exe, insert_name, given))
}

fn prepare(clip: NewClip, now: i64) -> Result<Prepared, String> {
    let NewClip {
        kind,
        mut hash,
        preview,
        text,
        mut meta,
        size: _,
        char_count,
        word_count,
        line_count,
        width,
        height,
        file_count: _,
        png,
        thumb_png,
        html,
        rtf,
        files,
        source,
        copied_at,
    } = clip;
    if hash.is_empty() || hash.len() > 128 || !hash.is_ascii() {
        return Err("invalid: hash".into());
    }
    let mut p = Prepared {
        kind,
        hash: String::new(),
        preview: String::new(),
        text: None,
        meta: ClipMeta::default(),
        char_count: None,
        word_count: None,
        line_count: None,
        width: None,
        height: None,
        file_count: None,
        png: None,
        thumb: None,
        html: None,
        rtf: None,
        files: None,
        source: source.and_then(prepare_source),
        copied_at: if copied_at > 0 { copied_at } else { now },
    };
    match kind {
        Kind::Image => {
            let png = png.filter(|b| media::is_png(b)).ok_or("invalid: image record without a PNG")?;
            // Dimensions from the PNG header (authoritative, cheap).
            let (w, h) = ImageReader::with_format(Cursor::new(&png), ImageFormat::Png)
                .into_dimensions()
                .map_err(|e| format!("invalid: png: {e}"))?;
            if u64::from(w) * u64::from(h) > IMAGE_MAX_PIXELS {
                return Err("invalid: image too large".into());
            }
            p.width = Some(i64::from(w));
            p.height = Some(i64::from(h));
            p.thumb = thumb_png.filter(|b| media::is_png(b));
            p.png = Some(png);
            p.preview = content::make_preview(preview.trim());
        }
        Kind::File => {
            let f = files.ok_or("invalid: file record without paths")?;
            let mut paths: Vec<String> = f.paths.into_iter().filter(|p| !p.trim().is_empty()).collect();
            if paths.is_empty() {
                return Err("invalid: file record without paths".into());
            }
            if paths.len() > FILES_MAX {
                paths.truncate(FILES_MAX);
                hash = content::content_hash(Kind::File, &content::canonical_files(&paths));
            }
            p.preview = if preview.trim().is_empty() { file_name(&paths[0]) } else { content::make_preview(preview.trim()) };
            meta.dir = content::file_dir(&paths[0]); // FIX-08: the card's path tail
            p.text = Some(paths.join("\n"));
            p.file_count = Some(paths.len() as i64);
            let blob = FilesBlob { drop_effect: if f.drop_effect == 0 { 1 } else { f.drop_effect }, paths };
            p.files = Some(serde_json::to_vec(&blob).map_err(|e| format!("failed: {e}"))?);
        }
        _ => {
            let mut text = text.filter(|t| !t.is_empty()).ok_or("invalid: empty text")?;
            let mut metrics = (char_count, word_count, line_count);
            if text.len() > TEXT_MAX_BYTES {
                let (cut, _) = content::truncate_text(&text);
                text = cut.to_string();
                hash = content::content_hash(kind, content::canonical_text(&text));
                meta.truncated = Some(true);
                metrics = (None, None, None);
            }
            if metrics.0.is_none() || metrics.1.is_none() || metrics.2.is_none() {
                let m = content::text_metrics(&text);
                metrics = (Some(m.chars), Some(m.words), Some(m.lines));
            }
            (p.char_count, p.word_count, p.line_count) = metrics;
            p.preview = content::make_preview(if preview.is_empty() { &text } else { &preview });
            p.text = Some(text);
            p.width = width.filter(|w| *w > 0);
            p.height = height.filter(|h| *h > 0);
            p.png = png.filter(|b| media::is_png(b));
            p.html = html.filter(|h| !h.is_empty() && h.len() <= FORMAT_MAX_BYTES);
            p.rtf = rtf.filter(|r| !r.is_empty() && r.len() <= FORMAT_MAX_BYTES);
        }
    }
    p.hash = hash;
    p.meta = meta;
    Ok(p)
}

/// Field-wise merge for a bumped record: the new capture wins where it says something.
fn merge_meta(old: ClipMeta, new: &ClipMeta) -> ClipMeta {
    ClipMeta {
        url: new.url.clone().or(old.url),
        host: new.host.clone().or(old.host),
        color: new.color.clone().or(old.color),
        contact: new.contact.or(old.contact),
        ext: new.ext.clone().or(old.ext),
        truncated: new.truncated.or(old.truncated),
        dir: new.dir.clone().or(old.dir),
        shot: new.shot.clone().or(old.shot), // W5
    }
}

pub(crate) fn meta_json(meta: &ClipMeta) -> String {
    serde_json::to_string(meta).unwrap_or_else(|_| "{}".into())
}

pub(crate) fn parse_meta(json: Option<&str>) -> ClipMeta {
    json.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default()
}

pub(crate) fn upsert_format(
    tx: &Transaction<'_>,
    id: i64,
    format: &str,
    data: Option<&[u8]>,
    path: Option<&str>,
    size: usize,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO clip_formats (clip_id, format, data, path, size) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (clip_id, format) DO UPDATE
           SET data = excluded.data, path = excluded.path, size = excluded.size",
        params![id, format, data, path, size as i64],
    )
    .map(|_| ())
    .map_err(db_err)
}

/// `size` = bytes Liqlip stores for the record: text + every `clip_formats` row (the PNG's
/// size is recorded in its row). Thumbnails are not counted.
pub(crate) fn recompute_size(tx: &Transaction<'_>, id: i64) -> Result<(), String> {
    tx.execute(
        "UPDATE clips SET size = coalesce(length(CAST(text AS BLOB)), 0)
           + coalesce((SELECT sum(size) FROM clip_formats WHERE clip_id = ?1), 0)
         WHERE id = ?1",
        [id],
    )
    .map(|_| ())
    .map_err(db_err)
}

/// Kind and meta of a user-made text (create / edit) — the capture classifier (§15).
fn classify(text: &str, truncated: bool) -> (Kind, ClipMeta) {
    let (kind, mut meta) = crate::capture::classify::classify_text(text);
    let kind = if is_text_kind(kind) { kind } else { Kind::Text };
    meta.truncated = truncated.then_some(true);
    (kind, meta)
}

/// Placement of an item moved before `before` (None = end) among `others` (sorted by
/// position, the moved item excluded).
pub(crate) enum Placement {
    At(f64),
    /// Positions ran out of room: renumber the whole list (moved item included) 1, 2, 3…
    Renumber(Vec<(i64, f64)>),
}

/// `Err(())` = `before` is not in `others`.
pub(crate) fn place_before(moved: i64, others: &[(i64, f64)], before: Option<i64>) -> Result<Placement, ()> {
    let Some(b) = before else {
        return Ok(Placement::At(others.last().map_or(1.0, |o| o.1.floor() + 1.0)));
    };
    let idx = others.iter().position(|o| o.0 == b).ok_or(())?;
    let bo = others[idx].1;
    match idx.checked_sub(1).map(|i| others[i].1) {
        None => Ok(Placement::At(bo - 1.0)),
        Some(prev) => {
            let mid = (prev + bo) / 2.0;
            if bo - prev > MIN_GAP && mid > prev && mid < bo {
                Ok(Placement::At(mid))
            } else {
                let mut ids: Vec<i64> = others.iter().map(|o| o.0).collect();
                ids.insert(idx, moved);
                Ok(Placement::Renumber(ids.into_iter().enumerate().map(|(i, id)| (id, (i + 1) as f64)).collect()))
            }
        }
    }
}

impl Store {
    /// Insert a captured record, or bump the existing one with the same hash (`copied_at`,
    /// `copy_count + 1`, revive if soft-deleted, newest source app, newest classification).
    #[allow(dead_code)] // W1-B calls it
    pub fn insert_clip(&self, clip: NewClip) -> Result<InsertOutcome, String> {
        let p = prepare(clip, self.now())?;
        // Which files will the transaction need? (short read; re-checked inside the tx)
        let pre: Option<i64> = self
            .lock()
            .query_row("SELECT id FROM clips WHERE hash = ?1", [&p.hash], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        let missing = |path: PathBuf| !path.exists();
        let need_png = p.png.is_some() && pre.is_none_or(|id| missing(self.paths().media_png(id)));
        let need_thumb = p.thumb.is_some() && pre.is_none_or(|id| missing(self.paths().thumb_png(id)));
        // Big images are written here, not while holding the DB lock.
        let staged_png = match (&p.png, need_png) {
            (Some(b), true) => Some(media::stage(self.paths(), b)?),
            _ => None,
        };
        let staged_thumb = match (&p.thumb, need_thumb) {
            (Some(b), true) => match media::stage(self.paths(), b) {
                Ok(s) => Some(s),
                Err(e) => {
                    media::remove_files(staged_png.iter());
                    return Err(e);
                }
            },
            _ => None,
        };
        let result = self.insert_tx(&p, staged_png.as_deref(), staged_thumb.as_deref());
        // Staged files not renamed into place (bump of an existing image, failure).
        let leftovers: Vec<PathBuf> = staged_png.into_iter().chain(staged_thumb).filter(|s| s.exists()).collect();
        media::remove_files(&leftovers);
        let out = result?;
        self.wake(Wake::Inserted);
        if p.kind == Kind::Image {
            self.ocr_wake(); // W2-A: a new image may be queued for OCR
        }
        Ok(out)
    }

    fn insert_tx(&self, p: &Prepared, staged_png: Option<&Path>, staged_thumb: Option<&Path>) -> Result<InsertOutcome, String> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let mut app_id = None;
        let mut app_icon_missing = false;
        if let Some((exe, insert_name, update_name)) = &p.source {
            let row = |r: &rusqlite::Row<'_>| Ok((r.get(0)?, r.get(1)?));
            let (id, icon): (i64, Option<String>) = match find_app(&tx, exe)? {
                // A non-empty name from the capture renames the app, an empty one keeps it;
                // a row known only by its file name (an import) takes the full path (FIX-02).
                Some((id, rewrite)) => tx.query_row(
                    "UPDATE apps SET exe = CASE WHEN ?2 THEN ?3 ELSE exe END,
                       name = CASE WHEN ?4 <> '' THEN ?4 ELSE name END
                     WHERE id = ?1 RETURNING id, icon_path",
                    params![id, rewrite, exe, update_name],
                    row,
                ),
                None => tx.query_row(
                    "INSERT INTO apps (exe, name) VALUES (?1, ?2) RETURNING id, icon_path",
                    params![exe, insert_name],
                    row,
                ),
            }
            .map_err(db_err)?;
            app_id = Some(id);
            app_icon_missing = icon.is_none() || !self.paths().icon_png(id).exists();
        }
        let existing: Option<(i64, Option<String>)> = tx
            .query_row("SELECT id, meta FROM clips WHERE hash = ?1", [&p.hash], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(db_err)?;
        let mut written: Vec<PathBuf> = Vec::new();
        let res = (|| -> Result<(i64, bool), String> {
            let (id, created) = match existing {
                Some((id, old_meta)) => {
                    let meta = merge_meta(parse_meta(old_meta.as_deref()), &p.meta);
                    tx.execute(
                        "UPDATE clips SET copied_at = max(copied_at, ?1), copy_count = copy_count + 1,
                           deleted_at = NULL, app_id = coalesce(?2, app_id), kind = ?3, meta = ?4
                         WHERE id = ?5",
                        params![p.copied_at, app_id, p.kind.as_str(), meta_json(&meta), id],
                    )
                    .map_err(db_err)?;
                    (id, false)
                }
                None => {
                    tx.execute(
                        "INSERT INTO clips (kind, preview, text, hash, char_count, word_count, line_count,
                           width, height, file_count, app_id, created_at, copied_at, copy_count, meta)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, 1, ?13)",
                        params![
                            p.kind.as_str(),
                            p.preview,
                            p.text,
                            p.hash,
                            p.char_count,
                            p.word_count,
                            p.line_count,
                            p.width,
                            p.height,
                            p.file_count,
                            app_id,
                            p.copied_at,
                            meta_json(&p.meta),
                        ],
                    )
                    .map_err(db_err)?;
                    (tx.last_insert_rowid(), true)
                }
            };
            if p.kind == Kind::Image {
                self.ocr_enqueue_in(&tx, id)?; // W2-A: pending while OCR is on
            }
            if let Some(png) = &p.png {
                let dest = self.paths().media_png(id);
                if created || !dest.exists() {
                    media::place(staged_png, png, &dest)?;
                    written.push(dest);
                }
                upsert_format(&tx, id, "png", None, Some(&media_rel(id)), png.len())?;
            }
            if let Some(thumb) = &p.thumb {
                let dest = self.paths().thumb_png(id);
                if created || !dest.exists() {
                    media::place(staged_thumb, thumb, &dest)?;
                    written.push(dest);
                }
            }
            if let Some(html) = &p.html {
                upsert_format(&tx, id, "html", Some(html.as_bytes()), None, html.len())?;
            }
            if let Some(rtf) = &p.rtf {
                upsert_format(&tx, id, "rtf", Some(rtf), None, rtf.len())?;
            }
            if let Some(files) = &p.files {
                upsert_format(&tx, id, "files", Some(files), None, files.len())?;
            }
            recompute_size(&tx, id)?;
            Ok((id, created))
        })();
        match res.and_then(|v| tx.commit().map(|_| v).map_err(db_err)) {
            Ok((id, created)) => Ok(InsertOutcome { id, created, app_id, app_icon_missing }),
            Err(e) => {
                media::remove_files(&written);
                logx::warn("store", &format!("insert failed: {e}"));
                Err(e)
            }
        }
    }

    /// Data of one record for the clipboard writer. `Err("not_found: …")` for a missing or
    /// deleted id. A PNG whose file vanished is reported as unavailable (no `png_path`, no
    /// `png` in `formats`).
    #[allow(dead_code)] // W1-C calls it
    pub fn get_for_paste(&self, id: i64) -> Result<PastePayload, String> {
        let conn = self.lock();
        let (kind, text): (String, Option<String>) = conn
            .query_row("SELECT kind, text FROM clips WHERE id = ?1 AND deleted_at IS NULL", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| format!("not_found: clip {id}"))?;
        let kind = Kind::parse(&kind).unwrap_or(Kind::Text);
        let mut payload =
            PastePayload { id, kind, text, html: None, rtf: None, png_path: None, files: None, formats: Vec::new() };
        let mut present: Vec<String> = Vec::new();
        let mut st = conn.prepare("SELECT format, data FROM clip_formats WHERE clip_id = ?1").map_err(db_err)?;
        let rows = st
            .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?)))
            .map_err(db_err)?;
        for row in rows {
            let (format, data) = row.map_err(db_err)?;
            match (format.as_str(), data) {
                ("html", Some(d)) => payload.html = Some(String::from_utf8_lossy(&d).into_owned()),
                ("rtf", Some(d)) => payload.rtf = Some(d),
                ("files", Some(d)) => payload.files = FilesBlob::parse(&d),
                ("png", _) => {
                    let path = self.paths().media_png(id);
                    if !path.exists() {
                        logx::warn("store", &format!("paste {id}: media file missing"));
                        continue;
                    }
                    payload.png_path = Some(path);
                }
                _ => continue,
            }
            present.push(format);
        }
        if kind == Kind::File && payload.files.is_none() {
            let paths: Vec<String> =
                payload.text.as_deref().unwrap_or("").lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
            if !paths.is_empty() {
                payload.files = Some(ClipFiles { paths, drop_effect: 1 });
                present.push("files".into());
            }
        }
        payload.formats = formats_for(kind, present.iter().map(String::as_str));
        Ok(payload)
    }

    /// Record a paste/copy from Liqlip: `used_at = now`, `use_count += 1`; with `bump`
    /// (`settings.picker.bumpOnPaste`) also `copied_at = now`. Unknown/deleted ids are
    /// skipped. The caller emits `clips:changed { origin: 'user', change: 'updated', ids }`.
    #[allow(dead_code)] // W1-C calls it
    pub fn mark_used(&self, ids: &[i64], bump: bool) -> Result<(), String> {
        let ids = dedup_ids(ids);
        if ids.is_empty() {
            return Ok(());
        }
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let set = if bump {
            "used_at = ?1, use_count = use_count + 1, copied_at = max(copied_at, ?1)"
        } else {
            "used_at = ?1, use_count = use_count + 1"
        };
        for chunk in ids.chunks(CHUNK) {
            let sql = format!(
                "UPDATE clips SET {set} WHERE deleted_at IS NULL AND id IN ({})",
                (2..chunk.len() + 2).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",")
            );
            let mut values: Vec<i64> = vec![now];
            values.extend_from_slice(chunk);
            tx.execute(&sql, params_from_iter(values)).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)
    }

    /// `favorite` and `deleted_at IS NULL` check for a set of ids; any missing → not_found.
    fn live_favorites(tx: &Transaction<'_>, ids: &[i64]) -> Result<HashMap<i64, bool>, String> {
        let mut found = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK) {
            let sql = format!(
                "SELECT id, favorite FROM clips WHERE deleted_at IS NULL AND id IN ({})",
                placeholders(chunk.len())
            );
            let mut st = tx.prepare(&sql).map_err(db_err)?;
            let rows = st
                .query_map(params_from_iter(chunk), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? != 0)))
                .map_err(db_err)?;
            for row in rows {
                let (id, fav) = row.map_err(db_err)?;
                found.insert(id, fav);
            }
        }
        if let Some(missing) = ids.iter().find(|id| !found.contains_key(id)) {
            return Err(format!("not_found: clip {missing}"));
        }
        Ok(found)
    }

    /// Mark / unmark favorites, all or nothing. New favorites go to the END of the manual
    /// order, in the order given. Returns the (deduplicated) ids.
    pub(crate) fn set_favorite(&self, ids: &[i64], value: bool) -> Result<Vec<i64>, String> {
        let ids = dedup_ids(ids);
        if ids.is_empty() {
            return Ok(ids);
        }
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let current = Self::live_favorites(&tx, &ids)?;
        if value {
            let mut next: f64 = tx
                .query_row(
                    "SELECT coalesce(max(fav_order), 0) FROM clips WHERE favorite = 1 AND deleted_at IS NULL",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_err)?;
            next = next.floor();
            for id in &ids {
                if current.get(id) == Some(&false) {
                    next += 1.0;
                    tx.execute("UPDATE clips SET favorite = 1, fav_order = ?1 WHERE id = ?2", params![next, id])
                        .map_err(db_err)?;
                }
            }
        } else {
            for chunk in ids.chunks(CHUNK) {
                let sql =
                    format!("UPDATE clips SET favorite = 0, fav_order = NULL WHERE id IN ({})", placeholders(chunk.len()));
                tx.execute(&sql, params_from_iter(chunk)).map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(ids)
    }

    /// Manual order inside Favorites: put `id` before `before_id` (None = at the end).
    pub(crate) fn move_favorite(&self, id: i64, before_id: Option<i64>) -> Result<(), String> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let fav = Self::live_favorites(&tx, &[id])?;
        if fav.get(&id) != Some(&true) {
            return Err(format!("invalid: clip {id} is not a favorite"));
        }
        if before_id == Some(id) {
            return Ok(());
        }
        let others: Vec<(i64, f64)> = {
            let mut st = tx
                .prepare(
                    "SELECT id, fav_order FROM clips WHERE favorite = 1 AND deleted_at IS NULL AND id <> ?1
                     ORDER BY fav_order, id",
                )
                .map_err(db_err)?;
            let rows = st
                .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<f64>>(1)?.unwrap_or(0.0))))
                .map_err(db_err)?;
            rows.collect::<Result<_, _>>().map_err(db_err)?
        };
        match place_before(id, &others, before_id) {
            Err(()) => return Err(format!("not_found: favorite {}", before_id.unwrap_or_default())),
            Ok(Placement::At(pos)) => {
                tx.execute("UPDATE clips SET fav_order = ?1 WHERE id = ?2", params![pos, id]).map_err(db_err)?;
            }
            Ok(Placement::Renumber(all)) => {
                for (cid, pos) in all {
                    tx.execute("UPDATE clips SET fav_order = ?1 WHERE id = ?2", params![pos, cid]).map_err(db_err)?;
                }
            }
        }
        tx.commit().map_err(db_err)
    }

    /// Move records into a folder (`None` = out of any folder), all or nothing. Returns the
    /// ids and an undo token restoring each record's previous folder.
    pub(crate) fn set_folder(&self, ids: &[i64], folder_id: Option<i64>) -> Result<(Vec<i64>, String), String> {
        let ids = dedup_ids(ids);
        let mut prev: Vec<(i64, Option<i64>)> = Vec::with_capacity(ids.len());
        {
            let mut conn = self.lock();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
            if let Some(fid) = folder_id {
                let exists: Option<i64> = tx
                    .query_row("SELECT id FROM folders WHERE id = ?1", [fid], |r| r.get(0))
                    .optional()
                    .map_err(db_err)?;
                if exists.is_none() {
                    return Err(format!("not_found: folder {fid}"));
                }
            }
            let mut found: HashMap<i64, Option<i64>> = HashMap::with_capacity(ids.len());
            for chunk in ids.chunks(CHUNK) {
                let sql = format!(
                    "SELECT id, folder_id FROM clips WHERE deleted_at IS NULL AND id IN ({})",
                    placeholders(chunk.len())
                );
                let mut st = tx.prepare(&sql).map_err(db_err)?;
                let rows = st.query_map(params_from_iter(chunk), |r| Ok((r.get(0)?, r.get(1)?))).map_err(db_err)?;
                for row in rows {
                    let (id, f): (i64, Option<i64>) = row.map_err(db_err)?;
                    found.insert(id, f);
                }
            }
            for id in &ids {
                match found.get(id) {
                    Some(f) => prev.push((*id, *f)),
                    None => return Err(format!("not_found: clip {id}")),
                }
            }
            for chunk in ids.chunks(CHUNK) {
                let sql = format!(
                    "UPDATE clips SET folder_id = ?1 WHERE id IN ({})",
                    (2..chunk.len() + 2).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",")
                );
                let mut values: Vec<rusqlite::types::Value> = vec![folder_id.into()];
                values.extend(chunk.iter().map(|&i| rusqlite::types::Value::Integer(i)));
                tx.execute(&sql, params_from_iter(values)).map_err(db_err)?;
            }
            tx.commit().map_err(db_err)?;
        }
        let token = self.push_undo(UndoOp::Refolder { prev });
        Ok((ids, token))
    }

    /// User title; `None`/blank = automatic. Control characters become spaces.
    pub(crate) fn rename(&self, id: i64, title: Option<&str>) -> Result<Clip, String> {
        let title: Option<String> = title
            .map(|t| t.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty());
        if title.as_ref().is_some_and(|t| t.chars().count() > TITLE_MAX_CHARS) {
            return Err("invalid: title too long".into());
        }
        let conn = self.lock();
        let n = conn
            .execute("UPDATE clips SET title = ?1 WHERE id = ?2 AND deleted_at IS NULL", params![title, id])
            .map_err(db_err)?;
        if n == 0 {
            return Err(format!("not_found: clip {id}"));
        }
        clip_by_id(&conn, id)
    }

    /// Replace the text of a text record in place: new hash (`content_hash`), preview,
    /// metrics, classification; rich formats (html/rtf/png alternative) are dropped since
    /// they no longer match. Colliding with another live record → `invalid: duplicate`
    /// (a soft-deleted twin is purged instead).
    pub(crate) fn update_text(&self, id: i64, text: &str) -> Result<Clip, String> {
        if text.is_empty() {
            return Err("invalid: empty text".into());
        }
        let (text, truncated) = content::truncate_text(text);
        let hash = content::content_hash(Kind::Text, content::canonical_text(text));
        let mut doomed: Vec<i64> = Vec::new();
        let mut drop_media = false;
        let clip = {
            let mut conn = self.lock();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
            let (kind, old_hash): (String, String) = tx
                .query_row("SELECT kind, hash FROM clips WHERE id = ?1 AND deleted_at IS NULL", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()
                .map_err(db_err)?
                .ok_or_else(|| format!("not_found: clip {id}"))?;
            if !is_text_kind(Kind::parse(&kind).unwrap_or(Kind::Text)) {
                return Err("invalid: not a text record".into());
            }
            if old_hash != hash {
                let twin: Option<(i64, Option<i64>)> = tx
                    .query_row("SELECT id, deleted_at FROM clips WHERE hash = ?1 AND id <> ?2", params![hash, id], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                    .optional()
                    .map_err(db_err)?;
                match twin {
                    Some((_, None)) => return Err("invalid: duplicate".into()),
                    Some((twin_id, Some(_))) => {
                        tx.execute("DELETE FROM clips WHERE id = ?1", [twin_id]).map_err(db_err)?;
                        doomed.push(twin_id);
                    }
                    None => {}
                }
                let (kind, meta) = classify(text, truncated);
                let m = content::text_metrics(text);
                drop_media = tx
                    .query_row(
                        "SELECT count(*) FROM clip_formats WHERE clip_id = ?1 AND format = 'png'",
                        [id],
                        |r| r.get::<_, i64>(0),
                    )
                    .map_err(db_err)?
                    > 0;
                tx.execute("DELETE FROM clip_formats WHERE clip_id = ?1", [id]).map_err(db_err)?;
                tx.execute(
                    "UPDATE clips SET text = ?1, preview = ?2, hash = ?3, kind = ?4, meta = ?5,
                       char_count = ?6, word_count = ?7, line_count = ?8, width = NULL, height = NULL
                     WHERE id = ?9",
                    params![
                        text,
                        content::make_preview(text),
                        hash,
                        kind.as_str(),
                        meta_json(&meta),
                        m.chars,
                        m.words,
                        m.lines,
                        id
                    ],
                )
                .map_err(db_err)?;
                recompute_size(&tx, id)?;
            }
            let clip = clip_by_id(&tx, id)?;
            tx.commit().map_err(db_err)?;
            clip
        };
        media::remove_clip_files(self.paths(), &doomed);
        if drop_media {
            media::remove_files(&[self.paths().media_png(id)]);
        }
        Ok(clip)
    }

    /// FIX-08 (W3): file records stored before `ClipMeta.dir` existed get it — once, lazily,
    /// on the background thread at start-up, [`CHUNK`] rows per transaction, no schema change
    /// (only the `meta` JSON). Live file rows without `dir` are visited (the kind index); a
    /// row whose first path has no parent folder stays without it. Returns the updated ids.
    pub(crate) fn backfill_file_dirs(&self) -> Result<Vec<i64>, String> {
        let mut updated = Vec::new();
        let mut after = 0i64;
        loop {
            let mut conn = self.lock();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
            type Row = (i64, Option<String>, Option<String>, Option<Vec<u8>>);
            let rows: Vec<Row> = {
                let mut st = tx
                    .prepare_cached(
                        "SELECT c.id, c.meta, c.text,
                           (SELECT data FROM clip_formats f WHERE f.clip_id = c.id AND f.format = 'files')
                         FROM clips c
                         WHERE c.kind = 'file' AND c.deleted_at IS NULL AND c.id > ?1
                           AND (CASE WHEN json_valid(c.meta) THEN json_extract(c.meta, '$.dir') END) IS NULL
                         ORDER BY c.id LIMIT ?2",
                    )
                    .map_err(db_err)?;
                let rows = st
                    .query_map(params![after, CHUNK as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                    .map_err(db_err)?;
                rows.collect::<Result<_, _>>().map_err(db_err)?
            };
            let Some(last) = rows.last() else { break };
            after = last.0;
            for (id, meta, text, blob) in rows {
                let first = blob
                    .as_deref()
                    .and_then(FilesBlob::parse)
                    .and_then(|f| f.paths.into_iter().next())
                    .or_else(|| text.as_deref().and_then(|t| t.lines().find(|l| !l.trim().is_empty()).map(str::to_string)));
                let Some(dir) = first.as_deref().and_then(content::file_dir) else { continue };
                let mut m = parse_meta(meta.as_deref());
                m.dir = Some(dir);
                tx.execute("UPDATE clips SET meta = ?1 WHERE id = ?2", params![meta_json(&m), id]).map_err(db_err)?;
                updated.push(id);
            }
            tx.commit().map_err(db_err)?;
        }
        Ok(updated)
    }

    /// New record from text (merge, transform → save), `origin: 'user'`, no source app. An
    /// identical existing record is bumped (and revived) instead. Returns the record and
    /// whether it was created.
    pub(crate) fn create_text(&self, text: &str, folder_id: Option<i64>) -> Result<(Clip, bool), String> {
        if text.is_empty() {
            return Err("invalid: empty text".into());
        }
        let (text, truncated) = content::truncate_text(text);
        let hash = content::content_hash(Kind::Text, content::canonical_text(text));
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        if let Some(fid) = folder_id {
            let exists: Option<i64> =
                tx.query_row("SELECT id FROM folders WHERE id = ?1", [fid], |r| r.get(0)).optional().map_err(db_err)?;
            if exists.is_none() {
                return Err(format!("not_found: folder {fid}"));
            }
        }
        let existing: Option<i64> =
            tx.query_row("SELECT id FROM clips WHERE hash = ?1", [&hash], |r| r.get(0)).optional().map_err(db_err)?;
        let (id, created) = match existing {
            Some(id) => {
                tx.execute(
                    "UPDATE clips SET copied_at = max(copied_at, ?1), copy_count = copy_count + 1, deleted_at = NULL,
                       folder_id = coalesce(?2, folder_id)
                     WHERE id = ?3",
                    params![now, folder_id, id],
                )
                .map_err(db_err)?;
                (id, false)
            }
            None => {
                let (kind, meta) = classify(text, truncated);
                let m = content::text_metrics(text);
                tx.execute(
                    "INSERT INTO clips (kind, preview, text, hash, char_count, word_count, line_count,
                       created_at, copied_at, copy_count, folder_id, meta)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, ?9, ?10)",
                    params![
                        kind.as_str(),
                        content::make_preview(text),
                        text,
                        hash,
                        m.chars,
                        m.words,
                        m.lines,
                        now,
                        folder_id,
                        meta_json(&meta)
                    ],
                )
                .map_err(db_err)?;
                let id = tx.last_insert_rowid();
                recompute_size(&tx, id)?;
                (id, true)
            }
        };
        let clip = clip_by_id(&tx, id)?;
        tx.commit().map_err(db_err)?;
        drop(conn);
        if created {
            self.wake(Wake::Inserted);
        }
        Ok((clip, created))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{files_clip, image_clip, png_bytes, text_clip, TestStore};
    use super::*;
    use crate::model::Format;

    fn row_i64(t: &TestStore, sql: &str, id: i64) -> i64 {
        t.lock().query_row(sql, [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn dedup_bumps_and_revives() {
        let t = TestStore::new();
        let a = t.insert_clip(text_clip("same", 1_000)).unwrap();
        assert!(a.created);
        let b = t.insert_clip(text_clip("same", 5_000)).unwrap();
        assert_eq!((b.id, b.created), (a.id, false));
        assert_eq!(row_i64(&t, "SELECT copy_count FROM clips WHERE id = ?1", a.id), 2);
        assert_eq!(row_i64(&t, "SELECT copied_at FROM clips WHERE id = ?1", a.id), 5_000);
        assert_eq!(row_i64(&t, "SELECT created_at FROM clips WHERE id = ?1", a.id), 1_000);
        // An older capture never moves copied_at backwards.
        t.insert_clip(text_clip("same", 2_000)).unwrap();
        assert_eq!(row_i64(&t, "SELECT copied_at FROM clips WHERE id = ?1", a.id), 5_000);
        // Soft-deleted → revived by a new copy.
        t.delete(&[a.id]).unwrap();
        let c = t.insert_clip(text_clip("same", 9_000)).unwrap();
        assert_eq!((c.id, c.created), (a.id, false));
        assert_eq!(row_i64(&t, "SELECT count(*) FROM clips WHERE id = ?1 AND deleted_at IS NULL", a.id), 1);
        assert_eq!(row_i64(&t, "SELECT count(*) FROM clips WHERE ?1 > 0", 1), 1, "no twin row");
    }

    #[test]
    fn insert_validates_and_normalises() {
        let t = TestStore::new();
        let mut bad = text_clip("x", 1);
        bad.text = Some(String::new());
        assert!(t.insert_clip(bad).unwrap_err().starts_with("invalid:"));
        let mut bad = text_clip("x", 1);
        bad.hash = String::new();
        assert!(t.insert_clip(bad).unwrap_err().starts_with("invalid:"));
        let mut img = image_clip(10, 10, 1, false);
        img.png = Some(b"not png".to_vec());
        assert!(t.insert_clip(img).unwrap_err().starts_with("invalid:"));
        let mut files = files_clip(&["c:\\a.txt"], 1);
        files.files = None;
        assert!(t.insert_clip(files).unwrap_err().starts_with("invalid:"));
        // Oversized text is cut to TEXT_MAX_BYTES and re-hashed with content.rs.
        let big = "ж".repeat(TEXT_MAX_BYTES); // 2 bytes per char → twice the limit
        let id = t.insert_clip(text_clip(&big, 2)).unwrap().id;
        let (len, meta, hash): (i64, String, String) = t
            .lock()
            .query_row("SELECT length(CAST(text AS BLOB)), meta, hash FROM clips WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert!(len as usize <= TEXT_MAX_BYTES);
        assert!(meta.contains("\"truncated\":true"));
        let stored: String = t.lock().query_row("SELECT text FROM clips WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(hash, content::content_hash(Kind::Text, content::canonical_text(&stored)));
        // Oversized html is dropped, the record kept.
        let mut c = text_clip("with html", 3);
        c.html = Some("h".repeat(FORMAT_MAX_BYTES + 1));
        let id = t.insert_clip(c).unwrap().id;
        assert_eq!(t.get_for_paste(id).unwrap().formats, vec![Format::Text]);
    }

    #[test]
    fn formats_files_and_size_are_stored() {
        let t = TestStore::new();
        let mut c = text_clip("rich", 1_000);
        c.html = Some("<b>rich</b>".into());
        c.rtf = Some(b"{\\rtf1 rich}".to_vec());
        c.png = Some(png_bytes(8, 8));
        let id = t.insert_clip(c.clone()).unwrap().id;
        let p = t.get_for_paste(id).unwrap();
        assert_eq!(p.formats, vec![Format::Text, Format::Html, Format::Rtf, Format::Png]);
        assert_eq!(p.html.as_deref(), Some("<b>rich</b>"));
        assert_eq!(p.rtf.as_deref(), Some(&b"{\\rtf1 rich}"[..]));
        assert!(p.png_path.as_ref().unwrap().exists());
        assert!(p.png_path.as_ref().unwrap().is_absolute(), "the paste writer gets an absolute path");
        let expect = 4 + 11 + 12 + c.png.as_ref().unwrap().len() as i64;
        assert_eq!(row_i64(&t, "SELECT size FROM clips WHERE id = ?1", id), expect);

        let f = t.add_files(&["C:\\Docs\\a.txt", "C:\\Docs\\b.txt"], 2_000);
        let p = t.get_for_paste(f).unwrap();
        assert_eq!(p.kind, Kind::File);
        assert_eq!(p.formats, vec![Format::Files, Format::Text]);
        let files = p.files.unwrap();
        assert_eq!(files.paths, vec!["C:\\Docs\\a.txt", "C:\\Docs\\b.txt"]);
        assert_eq!(files.drop_effect, 1);
        assert_eq!(p.text.as_deref(), Some("C:\\Docs\\a.txt\nC:\\Docs\\b.txt"));

        let img = t.add_image(20, 10, 3_000, true);
        let p = t.get_for_paste(img).unwrap();
        assert_eq!(p.formats, vec![Format::Png]);
        std::fs::remove_file(p.png_path.unwrap()).unwrap();
        let p = t.get_for_paste(img).unwrap();
        assert!(p.formats.is_empty() && p.png_path.is_none(), "a vanished file is not offered");
        // Missing and soft-deleted records → `not_found:` (the paste stack prunes on it).
        assert!(t.get_for_paste(9_999).unwrap_err().starts_with("not_found:"));
        t.delete(&[f]).unwrap();
        assert!(t.get_for_paste(f).unwrap_err().starts_with("not_found:"));
        t.purge_deleted().unwrap();
        t.set_now(10_000_000 + super::super::lifecycle::UNDO_TTL_MS + 1);
        t.purge_deleted().unwrap();
        assert!(t.get_for_paste(f).unwrap_err().starts_with("not_found:"), "hard-deleted too");
    }

    #[test]
    fn image_bump_reuses_files() {
        let t = TestStore::new();
        let a = t.insert_clip(image_clip(30, 30, 1_000, true)).unwrap();
        let b = t.insert_clip(image_clip(30, 30, 2_000, true)).unwrap();
        assert_eq!((b.id, b.created), (a.id, false));
        let staging = std::fs::read_dir(&t.paths().media)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(".staging-"))
            .count();
        assert_eq!(staging, 0, "no staging leftovers");
        // A deleted media file is restored by the next copy.
        std::fs::remove_file(t.paths().media_png(a.id)).unwrap();
        t.insert_clip(image_clip(30, 30, 3_000, true)).unwrap();
        assert!(t.paths().media_png(a.id).exists());
    }

    #[test]
    fn mark_used_counts_and_bumps() {
        let t = TestStore::new();
        let a = t.add_text("a", 1_000);
        t.set_now(50_000);
        t.mark_used(&[a, a, 9_999], false).unwrap();
        assert_eq!(row_i64(&t, "SELECT use_count FROM clips WHERE id = ?1", a), 1);
        assert_eq!(row_i64(&t, "SELECT used_at FROM clips WHERE id = ?1", a), 50_000);
        assert_eq!(row_i64(&t, "SELECT copied_at FROM clips WHERE id = ?1", a), 1_000);
        t.mark_used(&[a], true).unwrap();
        assert_eq!(row_i64(&t, "SELECT use_count FROM clips WHERE id = ?1", a), 2);
        assert_eq!(row_i64(&t, "SELECT copied_at FROM clips WHERE id = ?1", a), 50_000);
    }

    #[test]
    fn favorites_are_appended_and_reordered() {
        let t = TestStore::new();
        let ids: Vec<i64> = (0..4).map(|i| t.add_text(&format!("fav {i}"), 1_000 + i)).collect();
        t.set_favorite(&[ids[2], ids[0]], true).unwrap();
        t.set_favorite(&[ids[1]], true).unwrap();
        let order = |t: &TestStore| -> Vec<i64> {
            let c = t.lock();
            let mut st = c
                .prepare("SELECT id FROM clips WHERE favorite = 1 AND deleted_at IS NULL ORDER BY fav_order, id")
                .unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(order(&t), vec![ids[2], ids[0], ids[1]]);
        // Re-favoriting keeps the position.
        t.set_favorite(&[ids[2]], true).unwrap();
        assert_eq!(order(&t), vec![ids[2], ids[0], ids[1]]);
        t.move_favorite(ids[1], Some(ids[2])).unwrap();
        assert_eq!(order(&t), vec![ids[1], ids[2], ids[0]]);
        t.move_favorite(ids[1], None).unwrap();
        assert_eq!(order(&t), vec![ids[2], ids[0], ids[1]]);
        t.move_favorite(ids[1], Some(ids[0])).unwrap();
        assert_eq!(order(&t), vec![ids[2], ids[1], ids[0]]);
        // Many moves into the same gap force a renumbering; the order stays right.
        for _ in 0..60 {
            t.move_favorite(ids[0], Some(ids[1])).unwrap();
            t.move_favorite(ids[1], Some(ids[0])).unwrap();
        }
        assert_eq!(order(&t), vec![ids[2], ids[1], ids[0]]);
        // Errors.
        assert!(t.move_favorite(ids[3], None).unwrap_err().starts_with("invalid:"));
        assert!(t.move_favorite(ids[0], Some(ids[3])).unwrap_err().starts_with("not_found:"));
        assert!(t.set_favorite(&[ids[0], 9_999], false).unwrap_err().starts_with("not_found:"));
        assert_eq!(order(&t).len(), 3, "all-or-nothing");
        t.set_favorite(&[ids[0]], false).unwrap();
        assert_eq!(order(&t), vec![ids[2], ids[1]]);
        assert_eq!(row_i64(&t, "SELECT count(*) FROM clips WHERE id = ?1 AND fav_order IS NULL", ids[0]), 1);
    }

    #[test]
    fn place_before_handles_edges() {
        let others = vec![(1, 1.0), (2, 2.0)];
        assert!(matches!(place_before(9, &others, None), Ok(Placement::At(p)) if p == 3.0));
        assert!(matches!(place_before(9, &others, Some(1)), Ok(Placement::At(p)) if p == 0.0));
        assert!(matches!(place_before(9, &others, Some(2)), Ok(Placement::At(p)) if p == 1.5));
        assert!(place_before(9, &others, Some(7)).is_err());
        assert!(matches!(place_before(9, &[], None), Ok(Placement::At(p)) if p == 1.0));
        let tight = vec![(1, 1.0), (2, 1.0 + 1e-9)];
        match place_before(9, &tight, Some(2)) {
            Ok(Placement::Renumber(all)) => assert_eq!(all, vec![(1, 1.0), (9, 2.0), (2, 3.0)]),
            _ => panic!("expected renumber"),
        }
    }

    #[test]
    fn rename_and_errors() {
        let t = TestStore::new();
        let a = t.add_text("rename me", 1_000);
        let c = t.rename(a, Some("  My\ttitle \n")).unwrap();
        assert_eq!(c.title.as_deref(), Some("My title"));
        assert_eq!(t.rename(a, Some("   ")).unwrap().title, None);
        assert_eq!(t.rename(a, None).unwrap().title, None);
        assert!(t.rename(a, Some(&"x".repeat(201))).unwrap_err().starts_with("invalid:"));
        assert!(t.rename(9_999, Some("x")).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn update_text_rehashes_and_drops_rich_formats() {
        let t = TestStore::new();
        let mut c = text_clip("old text", 1_000);
        c.html = Some("<i>old</i>".into());
        c.png = Some(png_bytes(4, 4));
        let id = t.insert_clip(c).unwrap().id;
        assert!(t.paths().media_png(id).exists());
        let clip = t.update_text(id, "new text here").unwrap();
        assert_eq!(clip.preview, "new text here");
        assert_eq!(clip.formats, vec![Format::Text]);
        assert_eq!(clip.char_count, Some(13));
        assert!(!t.paths().media_png(id).exists());
        let hash: String = t.lock().query_row("SELECT hash FROM clips WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(hash, content::content_hash(Kind::Text, b"new text here"));
        assert_eq!(row_i64(&t, "SELECT size FROM clips WHERE id = ?1", id), 13);
        // The FTS index follows the edit (trigger).
        let hits: i64 = t
            .lock()
            .query_row("SELECT count(*) FROM clips_fts WHERE clips_fts MATCH '\"here\"'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 1);
        // Collisions.
        let other = t.add_text("taken", 2_000);
        assert_eq!(t.update_text(id, "taken").unwrap_err(), "invalid: duplicate");
        t.delete(&[other]).unwrap();
        t.update_text(id, "taken").unwrap();
        assert_eq!(row_i64(&t, "SELECT count(*) FROM clips WHERE id = ?1", other), 0, "soft-deleted twin purged");
        // Errors.
        assert!(t.update_text(id, "").unwrap_err().starts_with("invalid:"));
        let img = t.add_image(4, 4, 3_000, false);
        assert_eq!(t.update_text(img, "x").unwrap_err(), "invalid: not a text record");
        assert!(t.update_text(9_999, "x").unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn create_text_inserts_or_bumps() {
        let t = TestStore::new();
        t.set_now(7_000);
        let (c, created) = t.create_text("made by user", None).unwrap();
        assert!(created);
        assert_eq!((c.app_id, c.copied_at, c.created_at), (None, 7_000, 7_000));
        let f = t.folder_create("Work").unwrap();
        t.set_now(8_000);
        let (c2, created) = t.create_text("made by user", Some(f.id)).unwrap();
        assert!(!created);
        assert_eq!((c2.id, c2.copy_count, c2.folder_id, c2.copied_at), (c.id, 2, Some(f.id), 8_000));
        assert!(t.create_text("", None).unwrap_err().starts_with("invalid:"));
        assert!(t.create_text("x", Some(999)).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn bump_updates_source_kind_and_merges_meta() {
        let t = TestStore::new();
        let mut first = text_clip("https://example.com/page", 1_000);
        first.meta.url = Some("https://source.example/".into());
        first.source = Some(SourceApp { exe: "C:\\Apps\\Browser.EXE".into(), name: "  ".into() });
        let a = t.insert_clip(first).unwrap();
        let apps = t.apps_list().unwrap();
        assert_eq!((apps[0].exe.as_str(), apps[0].name.as_str()), ("c:\\apps\\browser.exe", "browser"));
        let mut again = text_clip("https://example.com/page", 2_000);
        again.kind = Kind::Link; // a better classifier: same family hash, new label
        again.meta.host = Some("example.com".into());
        again.source = Some(SourceApp { exe: "c:\\apps\\editor.exe".into(), name: "Editor".into() });
        let b = t.insert_clip(again).unwrap();
        assert_eq!(b.id, a.id);
        let c = t.clip_detail(a.id).unwrap().clip;
        assert_eq!(c.kind, Kind::Link);
        assert_eq!(c.app_id, b.app_id, "newest source app");
        assert_eq!(c.meta.url.as_deref(), Some("https://source.example/"), "kept");
        assert_eq!(c.meta.host.as_deref(), Some("example.com"), "added");
        // A later capture with a real name renames the app; a blank one keeps it.
        let mut named = text_clip("x", 3_000);
        named.source = Some(SourceApp { exe: "c:\\apps\\browser.exe".into(), name: "Browser".into() });
        t.insert_clip(named).unwrap();
        let mut blank = text_clip("y", 4_000);
        blank.source = Some(SourceApp { exe: "c:\\apps\\browser.exe".into(), name: String::new() });
        t.insert_clip(blank).unwrap();
        let names: Vec<String> = t.apps_list().unwrap().into_iter().map(|a| a.name).collect();
        assert!(names.contains(&"Browser".to_string()), "{names:?}");
    }

    #[test]
    fn match_app_rules() {
        let rows = [(1, "code.exe"), (2, "c:\\apps\\editor.exe"), (3, "c:\\other\\editor.exe"), (4, "C:/Tools/Vault.EXE")];
        assert_eq!(match_app("code.exe", &rows), Some((1, false)));
        assert_eq!(match_app("c:\\program files\\vs code\\code.exe", &rows), Some((1, true)), "bare row takes the full path");
        assert_eq!(match_app("editor.exe", &rows), Some((2, false)), "bare import → the first full path");
        assert_eq!(match_app("c:\\other\\editor.exe", &rows), Some((3, false)), "exact wins");
        assert_eq!(match_app("d:\\x\\editor.exe", &rows), None, "another full path is another app");
        assert_eq!(match_app("c:\\tools\\vault.exe", &rows), Some((4, false)), "case-insensitive, / ≡ \\");
        assert_eq!(match_app("vault.exe", &rows), Some((4, false)));
        assert_eq!(match_app("notepad.exe", &rows), None);
        assert_eq!(match_app("c:\\x\\notepad.exe", &[]), None);
        assert_eq!(app_exe_key("  C:/Apps/Code.EXE "), "c:\\apps\\code.exe");
        assert_eq!(exe_file_name("c:/apps/code.exe"), "code.exe");
    }

    /// FIX-02: an import writes `<stem>.exe`, a capture the full path — one `apps` row either way.
    #[test]
    fn import_and_capture_share_one_app() {
        let t = TestStore::new();
        let app_of = |t: &TestStore, id: i64| row_i64(t, "SELECT app_id FROM clips WHERE id = ?1", id);
        // Imported first (bare file name, icon from Liquid Tools), captured later (full path).
        let mut imported = text_clip("from the import", 1_000);
        imported.source = Some(SourceApp { exe: "Code.exe".into(), name: "Code".into() });
        let a = t.insert_clip(imported).unwrap();
        t.set_app_icon(a.app_id.unwrap(), &png_bytes(16, 16)).unwrap();
        let mut captured = text_clip("from a capture", 2_000);
        captured.source = Some(SourceApp { exe: "C:/Program Files/VS Code/Code.EXE".into(), name: "Visual Studio Code".into() });
        let b = t.insert_clip(captured).unwrap();
        assert_eq!(b.app_id, a.app_id);
        assert!(!b.app_icon_missing, "the imported icon is kept");
        assert_eq!(row_i64(&t, "SELECT count(*) FROM apps WHERE ?1 > 0", 1), 1, "no second apps row");
        let apps = t.apps_list().unwrap();
        assert_eq!(apps[0].exe, "c:\\program files\\vs code\\code.exe");
        assert_eq!(apps[0].name, "Visual Studio Code");
        assert_eq!((app_of(&t, a.id), app_of(&t, b.id)), (a.app_id.unwrap(), a.app_id.unwrap()));
        // The other way round: captured first, a bare import name reuses the full-path row.
        let editor = t.add_text_from("captured in the editor", 3_000, "c:\\apps\\editor.exe");
        let mut later = text_clip("imported from the editor", 4_000);
        later.source = Some(SourceApp { exe: "EDITOR.exe".into(), name: "editor".into() });
        let c = t.insert_clip(later).unwrap();
        assert_eq!(c.app_id, editor.app_id);
        assert_eq!(row_i64(&t, "SELECT count(*) FROM apps WHERE ?1 > 0", 1), 2);
        let exes: Vec<String> = t.apps_list().unwrap().into_iter().map(|a| a.exe).collect();
        assert!(exes.contains(&"c:\\apps\\editor.exe".to_string()), "the full path stays: {exes:?}");
        // A different folder with the same file name is a different app.
        let other = t.add_text_from("another editor", 5_000, "d:\\portable\\editor.exe");
        assert_ne!(other.app_id, editor.app_id);
    }

    #[test]
    fn concurrent_writers_and_readers() {
        let t = TestStore::new();
        let store: &Store = &t;
        let q = crate::model::FeedQuery {
            text: "w".into(),
            exclude: vec![],
            filter: Default::default(),
            cursor: None,
            limit: 40,
        };
        std::thread::scope(|s| {
            for w in 0..4 {
                s.spawn(move || {
                    for i in 0..50 {
                        store.insert_clip(text_clip(&format!("writer {w} item {i}"), 1_000 + i)).unwrap();
                    }
                });
            }
            s.spawn(|| {
                for _ in 0..50 {
                    store.counts().unwrap();
                    store.clips_list(&q, false).unwrap();
                }
            });
        });
        assert_eq!(store.counts().unwrap().all, 200);
    }

    /// FIX-08: a file record knows the folder of its first file (for the card's path tail);
    /// records stored before get it from the one-time backfill.
    #[test]
    fn file_records_carry_their_folder() {
        let t = TestStore::new();
        let a = t.add_files(&["C:\\Users\\me\\Docs\\report.pdf", "C:\\Users\\me\\Docs\\b.txt"], 1_000);
        let dir = |t: &TestStore, id| t.clip_detail(id).unwrap().clip.meta.dir;
        assert_eq!(dir(&t, a).as_deref(), Some("C:\\Users\\me\\Docs"));
        let root = t.add_files(&["D:\\only.txt"], 2_000);
        assert_eq!(dir(&t, root).as_deref(), Some("D:\\"));
        let text = t.add_text("not a file", 3_000);
        assert_eq!(dir(&t, text), None);
        // Records written before FIX-08: no `dir` in their meta.
        let b = t.add_files(&["\\\\nas\\share\\photos\\cat.png"], 4_000);
        let gone = t.add_files(&["E:\\x\\deleted.txt"], 5_000);
        t.delete(&[gone]).unwrap();
        t.lock()
            .execute("UPDATE clips SET meta = json_remove(meta, '$.dir') WHERE kind = 'file'", [])
            .unwrap();
        t.lock().execute("UPDATE clips SET meta = 'not json' WHERE id = ?1", [root]).unwrap();
        assert_eq!(dir(&t, a), None);
        let mut filled = t.backfill_file_dirs().unwrap();
        filled.sort();
        assert_eq!(filled, vec![a, root, b], "live file records only");
        assert_eq!(dir(&t, a).as_deref(), Some("C:\\Users\\me\\Docs"));
        assert_eq!(dir(&t, root).as_deref(), Some("D:\\"), "a broken meta is rewritten");
        assert_eq!(dir(&t, b).as_deref(), Some("\\\\nas\\share\\photos"));
        assert!(t.backfill_file_dirs().unwrap().is_empty(), "once");
        // A re-copy (bump) keeps it.
        let again = t.insert_clip(files_clip(&["C:\\Users\\me\\Docs\\report.pdf", "C:\\Users\\me\\Docs\\b.txt"], 6_000)).unwrap();
        assert_eq!((again.id, again.created), (a, false));
        assert_eq!(dir(&t, a).as_deref(), Some("C:\\Users\\me\\Docs"));
    }

    #[test]
    fn set_folder_is_atomic_and_validated() {
        let t = TestStore::new();
        let a = t.add_text("a", 1);
        let f = t.folder_create("F").unwrap();
        assert!(t.set_folder(&[a], Some(999)).unwrap_err().starts_with("not_found:"));
        assert!(t.set_folder(&[a, 999], Some(f.id)).unwrap_err().starts_with("not_found:"));
        assert_eq!(row_i64(&t, "SELECT count(*) FROM clips WHERE id = ?1 AND folder_id IS NULL", a), 1);
        let (ids, token) = t.set_folder(&[a, a], Some(f.id)).unwrap();
        assert_eq!(ids, vec![a]);
        assert!(!token.is_empty());
        assert_eq!(row_i64(&t, "SELECT folder_id FROM clips WHERE id = ?1", a), f.id);
    }
}
