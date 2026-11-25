//! Files next to the database (ARCHITECTURE §4.1): `media\<id>.png` (full images and the
//! PNG alternative of text records), `media\thumbs\<id>.png` (≤ 360 px previews),
//! `icons\<appId>.png` (source-app icons), and the `liq` URI scheme (§9.8).
//!
//! File names are derived from ids only (never from stored paths), and ids are never
//! reused (`AUTOINCREMENT`), so a file can be deleted after its row's transaction commits
//! without racing a new record.

use super::{db_err, Store};
use crate::content::IMAGE_MAX_PIXELS;
use crate::{fsx, logx};
use image::{ImageFormat, ImageReader};
use rusqlite::OptionalExtension;
use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Longest side of a thumbnail, px.
pub(crate) const THUMB_MAX: u32 = 360;
/// A source-app icon larger than this is refused (they are 32–256 px PNGs).
const ICON_MAX_BYTES: usize = 1024 * 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const STAGING_PREFIX: &str = ".staging-";

static STAGE_SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Paths {
    pub data: PathBuf,
    pub db: PathBuf,
    pub media: PathBuf,
    pub thumbs: PathBuf,
    pub icons: PathBuf,
}

impl Paths {
    /// All paths are absolute (`PastePayload.png_path` goes to the clipboard writer as is);
    /// a relative `data` (never the case for `app_data_dir`) is resolved against the cwd.
    pub fn new(data: &Path) -> Paths {
        let data = std::path::absolute(data).unwrap_or_else(|_| data.to_path_buf());
        let media = data.join("media");
        Paths {
            data: data.clone(),
            db: data.join("liqlip.db"),
            thumbs: media.join("thumbs"),
            media,
            icons: data.join("icons"),
        }
    }

    pub fn create_dirs(&self) -> Result<(), String> {
        for d in [&self.data, &self.media, &self.thumbs, &self.icons] {
            std::fs::create_dir_all(d).map_err(|e| format!("failed: create {}: {e}", d.display()))?;
        }
        Ok(())
    }

    pub fn media_png(&self, id: i64) -> PathBuf {
        self.media.join(format!("{id}.png"))
    }

    pub fn thumb_png(&self, id: i64) -> PathBuf {
        self.thumbs.join(format!("{id}.png"))
    }

    pub fn icon_png(&self, app_id: i64) -> PathBuf {
        self.icons.join(format!("{app_id}.png"))
    }

    /// Dropped after a runtime `SQLITE_CORRUPT`: the next open runs `PRAGMA quick_check`.
    pub fn check_marker(&self) -> PathBuf {
        self.data.join("liqlip.db.check")
    }

    /// `liqlip.db-wal` (WAL file next to the database).
    pub fn wal(&self) -> PathBuf {
        self.data.join("liqlip.db-wal")
    }

    /// Move an unusable database and its media aside (`*.corrupt-<ms>`), then recreate
    /// empty directories. Old media must not be picked up by the new database's ids.
    pub fn quarantine(&self, now: i64) -> Result<(), String> {
        let suffix = format!("corrupt-{now}");
        let aside = |p: &Path| -> Result<(), String> {
            if !p.exists() {
                return Ok(());
            }
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let dest = p.with_file_name(format!("{name}.{suffix}"));
            std::fs::rename(p, &dest).map_err(|e| format!("failed: move aside {}: {e}", p.display()))
        };
        aside(&self.db)?;
        aside(&self.wal())?;
        let _ = std::fs::remove_file(self.data.join("liqlip.db-shm"));
        aside(&self.media)?;
        aside(&self.icons)?;
        self.create_dirs()
    }
}

/// Relative path recorded in `clip_formats.path` / `apps.icon_path` (informational: files
/// are always located by id).
pub(crate) fn media_rel(id: i64) -> String {
    format!("media\\{id}.png")
}

pub(crate) fn icon_rel(app_id: i64) -> String {
    format!("icons\\{app_id}.png")
}

pub(crate) fn is_png(bytes: &[u8]) -> bool {
    bytes.len() > PNG_SIGNATURE.len() && bytes.starts_with(PNG_SIGNATURE)
}

/// Write `bytes` to a unique staging file inside `media\` (outside the DB lock); the
/// insert transaction then renames it into place (`place`).
pub(crate) fn stage(paths: &Paths, bytes: &[u8]) -> Result<PathBuf, String> {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let seq = STAGE_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = paths.media.join(format!("{STAGING_PREFIX}{}-{nanos}-{seq}.png", std::process::id()));
    std::fs::write(&path, bytes).map_err(|e| format!("failed: write media: {e}"))?;
    Ok(path)
}

/// Move a staged file into `dest`, or write `bytes` atomically when nothing was staged.
pub(crate) fn place(staged: Option<&Path>, bytes: &[u8], dest: &Path) -> Result<(), String> {
    match staged {
        Some(s) => std::fs::rename(s, dest).map_err(|e| format!("failed: place media: {e}")),
        None => fsx::write_atomic(dest, bytes).map_err(|e| format!("failed: write media: {e}")),
    }
}

/// Best-effort removal (missing files are fine).
pub(crate) fn remove_files<'a>(files: impl IntoIterator<Item = &'a PathBuf>) {
    for f in files {
        if let Err(e) = std::fs::remove_file(f) {
            if e.kind() != std::io::ErrorKind::NotFound {
                logx::warn("store", &format!("remove file failed: {e}"));
            }
        }
    }
}

/// Delete `media\<id>.png` and `media\thumbs\<id>.png` of hard-deleted records.
pub(crate) fn remove_clip_files(paths: &Paths, ids: &[i64]) {
    let files: Vec<PathBuf> = ids.iter().flat_map(|&id| [paths.media_png(id), paths.thumb_png(id)]).collect();
    remove_files(&files);
}

/// Thumbnail of a PNG: ≤ [`THUMB_MAX`] px on the long side, aspect kept; a small image is
/// returned as is. Refuses images above `IMAGE_MAX_PIXELS` (decoder limits apply too).
pub(crate) fn make_thumbnail(png: &[u8]) -> Result<Vec<u8>, String> {
    let (w, h) = ImageReader::with_format(Cursor::new(png), ImageFormat::Png)
        .into_dimensions()
        .map_err(|e| format!("invalid: png: {e}"))?;
    if u64::from(w) * u64::from(h) > IMAGE_MAX_PIXELS {
        return Err("invalid: image too large".into());
    }
    if w.max(h) <= THUMB_MAX {
        return Ok(png.to_vec());
    }
    let img = ImageReader::with_format(Cursor::new(png), ImageFormat::Png)
        .decode()
        .map_err(|e| format!("invalid: png: {e}"))?;
    let thumb = img.thumbnail(THUMB_MAX, THUMB_MAX);
    let mut out = Vec::new();
    thumb
        .write_to(Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| format!("failed: encode thumbnail: {e}"))?;
    Ok(out)
}

/// Total size of the regular files under `dir` (recursive).
pub(crate) fn dir_size(dir: &Path) -> i64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut total = 0i64;
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            total += dir_size(&entry.path());
        } else if ft.is_file() {
            total += entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
        }
    }
    total
}

/// `<id>.png` → id.
fn file_id(name: &str) -> Option<i64> {
    name.strip_suffix(".png")?.parse::<i64>().ok().filter(|id| *id > 0)
}

fn png_ids(dir: &Path) -> Vec<(i64, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| file_id(&e.file_name().to_string_lossy()).map(|id| (id, e.path())))
        .collect()
}

impl Store {
    /// Store the PNG icon of a source app (`icons\<app_id>.png`) and record it.
    #[allow(dead_code)] // W1-B calls it
    pub fn set_app_icon(&self, app_id: i64, png: &[u8]) -> Result<(), String> {
        if !is_png(png) {
            return Err("invalid: icon is not a PNG".into());
        }
        if png.len() > ICON_MAX_BYTES {
            return Err("invalid: icon too large".into());
        }
        let conn = self.lock();
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM apps WHERE id = ?1", [app_id], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        if exists.is_none() {
            return Err(format!("not_found: app {app_id}"));
        }
        fsx::write_atomic(&self.paths().icon_png(app_id), png).map_err(|e| format!("failed: write icon: {e}"))?;
        conn.execute("UPDATE apps SET icon_path = ?1 WHERE id = ?2", rusqlite::params![icon_rel(app_id), app_id])
            .map_err(db_err)?;
        Ok(())
    }

    /// `liq` scheme body for `path` = `/thumb/<id>` | `/media/<id>` | `/icon/<appId>`.
    /// Only live records are served; a missing thumbnail is rebuilt from the full image.
    pub(crate) fn serve_path(&self, path: &str) -> Option<(String, Vec<u8>)> {
        let mut parts = path.trim_matches('/').splitn(2, '/');
        let (what, id) = (parts.next()?, parts.next()?);
        let id: i64 = id.trim_end_matches('/').parse().ok().filter(|id: &i64| *id > 0)?;
        let png = |bytes: Vec<u8>| Some(("image/png".to_string(), bytes));
        let live_image = || -> Option<bool> {
            let conn = self.lock();
            let found = conn
                .query_row("SELECT 1 FROM clips WHERE id = ?1 AND kind = 'image' AND deleted_at IS NULL", [id], |_| Ok(()))
                .optional()
                .ok()?;
            Some(found.is_some())
        };
        match what {
            "thumb" => {
                if !live_image()? {
                    return None;
                }
                let thumb = self.paths().thumb_png(id);
                if let Ok(bytes) = std::fs::read(&thumb) {
                    return png(bytes);
                }
                // Lazy rebuild (the thumbnail was never written, or was deleted).
                let full = std::fs::read(self.paths().media_png(id)).ok()?;
                match make_thumbnail(&full) {
                    Ok(bytes) => {
                        if let Err(e) = fsx::write_atomic(&thumb, &bytes) {
                            logx::warn("store", &format!("thumbnail {id}: write failed: {e}"));
                        }
                        // Deleted while the thumbnail was being built (e.g. "delete recent"):
                        // its files are already gone — don't leave this one behind (W3).
                        if live_image() != Some(true) {
                            remove_files(&[thumb]);
                            return None;
                        }
                        png(bytes)
                    }
                    Err(e) => {
                        logx::warn("store", &format!("thumbnail {id}: {e}"));
                        None
                    }
                }
            }
            "media" => {
                let has_png = {
                    let conn = self.lock();
                    conn.query_row(
                        "SELECT 1 FROM clips c JOIN clip_formats f ON f.clip_id = c.id AND f.format = 'png'
                         WHERE c.id = ?1 AND c.deleted_at IS NULL",
                        [id],
                        |_| Ok(()),
                    )
                    .optional()
                    .ok()?
                    .is_some()
                };
                if !has_png {
                    return None;
                }
                png(std::fs::read(self.paths().media_png(id)).ok()?)
            }
            "icon" => {
                let has_icon = {
                    let conn = self.lock();
                    conn.query_row("SELECT 1 FROM apps WHERE id = ?1 AND icon_path IS NOT NULL", [id], |_| Ok(()))
                        .optional()
                        .ok()?
                        .is_some()
                };
                if !has_icon {
                    return None;
                }
                png(std::fs::read(self.paths().icon_png(id)).ok()?)
            }
            _ => None,
        }
    }

    /// Remove files that no row references: a crash between writing a file and committing,
    /// a thumbnail of a deleted image, stale staging files (> 1 h), icons of unknown apps.
    /// Lists the directories BEFORE reading the database: a file renamed in by an insert is
    /// only visible together with its committed row (both happen under the DB lock).
    pub(crate) fn sweep_orphans(&self) -> Result<usize, String> {
        let media = png_ids(&self.paths().media);
        let thumbs = png_ids(&self.paths().thumbs);
        let icons = png_ids(&self.paths().icons);
        let stale_staging: Vec<PathBuf> = std::fs::read_dir(&self.paths().media)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with(STAGING_PREFIX))
                    .filter(|e| {
                        e.metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.elapsed().ok())
                            .is_some_and(|age| age > Duration::from_secs(3600))
                    })
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        let (with_png, images, apps) = {
            let conn = self.lock();
            let set = |sql: &str| -> Result<HashSet<i64>, String> {
                let mut st = conn.prepare(sql).map_err(db_err)?;
                let rows = st.query_map([], |r| r.get::<_, i64>(0)).map_err(db_err)?;
                rows.collect::<Result<HashSet<i64>, _>>().map_err(db_err)
            };
            (
                set("SELECT clip_id FROM clip_formats WHERE format = 'png'")?,
                set("SELECT id FROM clips WHERE kind = 'image'")?,
                set("SELECT id FROM apps WHERE icon_path IS NOT NULL")?,
            )
        };
        let mut doomed: Vec<PathBuf> = stale_staging;
        doomed.extend(media.into_iter().filter(|(id, _)| !with_png.contains(id)).map(|(_, p)| p));
        doomed.extend(thumbs.into_iter().filter(|(id, _)| !images.contains(id)).map(|(_, p)| p));
        doomed.extend(icons.into_iter().filter(|(id, _)| !apps.contains(id)).map(|(_, p)| p));
        remove_files(&doomed);
        Ok(doomed.len())
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{png_bytes, TestStore};
    use super::*;

    #[test]
    fn thumbnail_fits_and_keeps_aspect() {
        let big = png_bytes(800, 400);
        let t = make_thumbnail(&big).unwrap();
        let (w, h) = ImageReader::with_format(Cursor::new(&t), ImageFormat::Png).into_dimensions().unwrap();
        assert_eq!((w, h), (360, 180));
        let small = png_bytes(100, 50);
        assert_eq!(make_thumbnail(&small).unwrap(), small, "small images are kept as is");
        assert!(make_thumbnail(b"not a png").unwrap_err().starts_with("invalid:"));
    }

    #[test]
    fn serve_paths_and_lazy_thumbnail() {
        let t = TestStore::new();
        let img = t.add_image(640, 320, 1_000, false);
        // No thumbnail was provided → rebuilt on first request, then cached on disk.
        assert!(!t.paths().thumb_png(img).exists());
        let (mime, bytes) = t.serve_path(&format!("/thumb/{img}")).expect("thumb");
        assert_eq!(mime, "image/png");
        assert!(is_png(&bytes));
        assert!(t.paths().thumb_png(img).exists());
        assert!(t.serve_path(&format!("/media/{img}")).is_some());
        // Text records have no thumbnail; unknown routes / ids → 404.
        let txt = t.add_text("hello", 2_000);
        assert!(t.serve_path(&format!("/thumb/{txt}")).is_none());
        assert!(t.serve_path(&format!("/media/{txt}")).is_none());
        assert!(t.serve_path("/thumb/999").is_none());
        assert!(t.serve_path("/thumb/abc").is_none());
        assert!(t.serve_path("/nope/1").is_none());
        assert!(t.serve_path("/").is_none());
        // Soft-deleted records are not served.
        t.delete(&[img]).unwrap();
        assert!(t.serve_path(&format!("/media/{img}")).is_none());
    }

    #[test]
    fn app_icons_are_validated_stored_and_served() {
        let t = TestStore::new();
        let out = t.add_text_from("from app", 1_000, "c:\\apps\\a.exe");
        let app = out.app_id.unwrap();
        assert!(out.app_icon_missing);
        assert!(t.serve_path(&format!("/icon/{app}")).is_none());
        assert!(t.set_app_icon(app, b"nope").unwrap_err().starts_with("invalid:"));
        assert!(t.set_app_icon(999, &png_bytes(4, 4)).unwrap_err().starts_with("not_found:"));
        t.set_app_icon(app, &png_bytes(32, 32)).unwrap();
        assert!(t.serve_path(&format!("/icon/{app}")).is_some());
        let again = t.add_text_from("from app 2", 2_000, "C:\\Apps\\A.exe");
        assert_eq!(again.app_id, Some(app), "exe is matched case-insensitively");
        assert!(!again.app_icon_missing);
    }

    #[test]
    fn orphan_sweep_removes_unreferenced_files_only() {
        let t = TestStore::new();
        let img = t.add_image(64, 64, 1_000, true);
        let txt = t.add_text("plain", 2_000);
        std::fs::write(t.paths().media_png(9_999), b"orphan").unwrap();
        std::fs::write(t.paths().thumb_png(txt), b"orphan").unwrap();
        std::fs::write(t.paths().icon_png(77), b"orphan").unwrap();
        let removed = t.sweep_orphans().unwrap();
        assert_eq!(removed, 3);
        assert!(t.paths().media_png(img).exists());
        assert!(t.paths().thumb_png(img).exists());
        assert!(!t.paths().media_png(9_999).exists());
    }

    #[test]
    fn dir_size_counts_nested_files() {
        let t = TestStore::new();
        t.add_image(64, 64, 1_000, true);
        let size = dir_size(&t.paths().media);
        let expect = std::fs::metadata(t.paths().media_png(1)).unwrap().len()
            + std::fs::metadata(t.paths().thumb_png(1)).unwrap().len();
        assert_eq!(size as u64, expect);
    }
}
