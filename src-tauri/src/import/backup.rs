//! `.liqlip` backups (ARCHITECTURE §16.2): a zip with `manifest.json`, `clipboard.db` (a
//! consistent snapshot of `liqlip.db`), `media/<id>.png`, `media/thumbs/<id>.png`,
//! `icons/<id>.png` and `settings.json`.
//!
//! - **Export** writes `<target>.<pid>.part` beside the target and renames it into place.
//! - **Replace** extracts into `<data>\restore-staging-*` (same volume → every swap step is
//!   a rename) and hands it to `Store::replace_with` (moves the current data aside, rolls
//!   back on failure); the caller then applies `settings.json`.
//! - **Merge** extracts into `%TEMP%\liqlip-merge-*` and feeds the backup's records through
//!   the same import loop as Liquid Tools (dedup by hash, folders by name, media copied),
//!   then merges snippets.

use super::runner::{self, Page, Progress, PAGE};
use super::tmp::TempDir;
use crate::model::{ClipMeta, ImportReport, Kind};
use crate::store::bulk::{parse_files_blob, ImportItem, SnippetRow, SCHEMA_VERSION};
use crate::store::{ClipFiles, NewClip, SourceApp, Store};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub(crate) const FORMAT: &str = "liqlip-backup";
pub(crate) const FORMAT_VERSION: u32 = 1;
/// Prefix of the restore staging folder inside the data directory.
const STAGING_PREFIX: &str = "restore-staging-";
/// `manifest.json` / `settings.json` larger than this are not read.
const JSON_MAX: u64 = 4 * 1024 * 1024;
/// A media / icon entry larger than this is refused.
const PNG_MAX: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    /// `PRAGMA user_version` of `clipboard.db`.
    pub schema_version: i64,
    /// Unix ms.
    pub created_at: i64,
    pub counts: ManifestCounts,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestCounts {
    pub clips: i64,
    pub favorites: i64,
    pub folders: i64,
    pub snippets: i64,
    pub apps: i64,
    /// Files under `media/` and `icons/`.
    pub media: i64,
}

/// `Liqlip-YYYY-MM-DD.liqlip`.
pub(crate) fn default_file_name(year: u16, month: u16, day: u16) -> String {
    format!("Liqlip-{year:04}-{month:02}-{day:02}.liqlip")
}

fn ro(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
}

fn ids(conn: &Connection, sql: &str) -> rusqlite::Result<Vec<i64>> {
    let mut st = conn.prepare(sql)?;
    let rows = st.query_map([], |r| r.get::<_, i64>(0))?;
    rows.collect()
}

fn count(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |r| r.get(0))
}

/// Deletes the `.part` file unless the export completed.
struct PartGuard {
    path: PathBuf,
    armed: bool,
}

impl Drop for PartGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Export a backup to `dest` (overwritten). Progress: `clips` (snapshot 0/1 → 1/1), then
/// `media` (files packed). Media referenced by the snapshot but deleted meanwhile (purge,
/// retention between the snapshot and packing) is left out.
pub(crate) fn export(
    store: &Store,
    settings_json: &[u8],
    app_version: &str,
    created_at: i64,
    zip_time: zip::DateTime,
    dest: &Path,
    progress: Progress<'_>,
) -> Result<Manifest, String> {
    let tmp = TempDir::new("liqlip-backup-")?;
    progress("clips", 0, 1);
    let snap = tmp.path().join("clipboard.db");
    store.snapshot_to(&snap)?;
    let db_err = |e: rusqlite::Error| format!("failed: backup snapshot: {e}");
    let (mut counts, png_ids, image_ids, icon_ids) = {
        let c = ro(&snap).map_err(db_err)?;
        let counts = ManifestCounts {
            clips: count(&c, "SELECT count(*) FROM clips").map_err(db_err)?,
            favorites: count(&c, "SELECT count(*) FROM clips WHERE favorite = 1").map_err(db_err)?,
            folders: count(&c, "SELECT count(*) FROM folders").map_err(db_err)?,
            snippets: count(&c, "SELECT count(*) FROM snippets").map_err(db_err)?,
            apps: count(&c, "SELECT count(*) FROM apps").map_err(db_err)?,
            media: 0,
        };
        (
            counts,
            ids(&c, "SELECT clip_id FROM clip_formats WHERE format = 'png' ORDER BY clip_id").map_err(db_err)?,
            ids(&c, "SELECT id FROM clips WHERE kind = 'image' ORDER BY id").map_err(db_err)?,
            ids(&c, "SELECT id FROM apps WHERE icon_path IS NOT NULL ORDER BY id").map_err(db_err)?,
        )
    };
    progress("clips", 1, 1);
    let data = store.data_dir();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    files.extend(png_ids.iter().map(|id| (format!("media/{id}.png"), data.join("media").join(format!("{id}.png")))));
    files.extend(
        image_ids.iter().map(|id| (format!("media/thumbs/{id}.png"), data.join("media").join("thumbs").join(format!("{id}.png")))),
    );
    files.extend(icon_ids.iter().map(|id| (format!("icons/{id}.png"), data.join("icons").join(format!("{id}.png")))));
    files.retain(|(_, p)| p.is_file());
    counts.media = files.len() as i64;
    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        app_version: app_version.into(),
        schema_version: SCHEMA_VERSION,
        created_at,
        counts,
    };

    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "backup.liqlip".into());
    let part = dest.with_file_name(format!("{name}.{}.part", std::process::id()));
    let mut guard = PartGuard { path: part.clone(), armed: true };
    let zerr = |e: zip::result::ZipError| format!("failed: write backup: {e}");
    let ioerr = |e: std::io::Error| format!("failed: write backup: {e}");
    let out = File::create(&part).map_err(ioerr)?;
    let mut zip = ZipWriter::new(out);
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).last_modified_time(zip_time);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored).last_modified_time(zip_time);
    zip.start_file("manifest.json", deflate).map_err(zerr)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| format!("failed: {e}"))?).map_err(ioerr)?;
    let db_len = std::fs::metadata(&snap).map_err(ioerr)?.len();
    zip.start_file("clipboard.db", deflate.large_file(db_len >= zip::ZIP64_BYTES_THR)).map_err(zerr)?;
    std::io::copy(&mut File::open(&snap).map_err(ioerr)?, &mut zip).map_err(ioerr)?;
    zip.start_file("settings.json", deflate).map_err(zerr)?;
    zip.write_all(settings_json).map_err(ioerr)?;
    let total = files.len() as i64;
    progress("media", 0, total);
    for (i, (entry, path)) in files.iter().enumerate() {
        let mut f = match File::open(path) {
            Ok(f) => f,
            // Deleted since the snapshot (purge / retention): left out.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(ioerr(e)),
        };
        // PNGs are already compressed: stored as is.
        zip.start_file(entry.as_str(), stored).map_err(zerr)?;
        std::io::copy(&mut f, &mut zip).map_err(ioerr)?;
        if (i + 1) % 50 == 0 || i + 1 == files.len() {
            progress("media", i as i64 + 1, total);
        }
    }
    let out = zip.finish().map_err(zerr)?;
    out.sync_all().map_err(ioerr)?;
    drop(out);
    std::fs::rename(&part, dest).map_err(ioerr)?;
    guard.armed = false;
    Ok(manifest)
}

/// Open a backup and validate its manifest: format, format version and schema version not
/// newer than this build.
pub(crate) fn open_archive(path: &Path) -> Result<(ZipArchive<File>, Manifest), String> {
    let not_backup = || "invalid: not a Liqlip backup".to_string();
    let file = File::open(path).map_err(|e| format!("failed: open backup: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|_| not_backup())?;
    let manifest: Manifest = {
        let f = zip.by_name("manifest.json").map_err(|_| not_backup())?;
        let mut raw = Vec::new();
        f.take(JSON_MAX).read_to_end(&mut raw).map_err(|_| not_backup())?;
        serde_json::from_slice(&raw).map_err(|_| not_backup())?
    };
    if manifest.format != FORMAT {
        return Err(not_backup());
    }
    if manifest.format_version > FORMAT_VERSION || manifest.schema_version > SCHEMA_VERSION {
        return Err("invalid: the backup was made by a newer Liqlip".into());
    }
    if manifest.schema_version < 1 {
        return Err(not_backup());
    }
    Ok((zip, manifest))
}

/// Destination of an archive entry inside the extraction folder; `None` = not extracted.
/// Only the exact layout is accepted (`<digits>.png` names), so no entry can escape.
fn entry_target(name: &str) -> Option<(PathBuf, u64)> {
    let png_id = |rest: &str| rest.strip_suffix(".png").filter(|s| !s.is_empty() && s.len() <= 19 && s.bytes().all(|b| b.is_ascii_digit())).map(str::to_string);
    if name == "clipboard.db" {
        return Some((PathBuf::from("liqlip.db"), u64::MAX));
    }
    if let Some(rest) = name.strip_prefix("media/thumbs/") {
        return png_id(rest).map(|id| (Path::new("media").join("thumbs").join(format!("{id}.png")), PNG_MAX));
    }
    if let Some(rest) = name.strip_prefix("media/") {
        return png_id(rest).map(|id| (Path::new("media").join(format!("{id}.png")), PNG_MAX));
    }
    if let Some(rest) = name.strip_prefix("icons/") {
        return png_id(rest).map(|id| (Path::new("icons").join(format!("{id}.png")), PNG_MAX));
    }
    None
}

/// Extract the database, media and icons into `dir`; returns `settings.json` (if any).
/// The zip reader verifies each entry's CRC-32 at its end.
fn extract(zip: &mut ZipArchive<File>, dir: &Path, progress: Progress<'_>) -> Result<Option<Vec<u8>>, String> {
    let bad = |e: String| format!("invalid: damaged backup ({e})");
    let n = zip.len();
    let mut settings = None;
    let mut has_db = false;
    progress("media", 0, n as i64);
    for i in 0..n {
        let f = zip.by_index(i).map_err(|e| bad(e.to_string()))?;
        if !f.is_file() {
            continue;
        }
        let name = f.name().map_err(|e| bad(e.to_string()))?.into_owned();
        if name == "settings.json" {
            let mut raw = Vec::new();
            f.take(JSON_MAX).read_to_end(&mut raw).map_err(|e| bad(e.to_string()))?;
            settings = Some(raw);
            continue;
        }
        let Some((rel, limit)) = entry_target(&name) else { continue };
        if f.size() > limit {
            return Err("invalid: backup entry too large".into());
        }
        let target = dir.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed: {e}"))?;
        }
        let mut out = File::create(&target).map_err(|e| format!("failed: extract: {e}"))?;
        let mut limited = f.take(limit.saturating_add(1));
        let written = std::io::copy(&mut limited, &mut out).map_err(|e| bad(e.to_string()))?;
        if written > limit {
            return Err("invalid: backup entry too large".into());
        }
        has_db |= name == "clipboard.db";
        if (i + 1) % 50 == 0 || i + 1 == n {
            progress("media", i as i64 + 1, n as i64);
        }
    }
    if !has_db {
        return Err("invalid: the backup has no database".into());
    }
    Ok(settings)
}

/// Counts of an extracted database after validation: `(all clips rows, live clips, folders)`.
fn check_db(db: &Path) -> Result<(i64, i64, i64), String> {
    let damaged = || "invalid: the backup database is damaged".to_string();
    let c = ro(db).map_err(|_| damaged())?;
    let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|_| damaged())?;
    if version < 1 || version > SCHEMA_VERSION {
        return Err(if version > SCHEMA_VERSION { "invalid: the backup was made by a newer Liqlip".into() } else { damaged() });
    }
    let check: String = c.query_row("PRAGMA quick_check(1)", [], |r| r.get(0)).map_err(|_| damaged())?;
    if check != "ok" {
        return Err(damaged());
    }
    let all = count(&c, "SELECT count(*) FROM clips").map_err(|_| damaged())?;
    let live = count(&c, "SELECT count(*) FROM clips WHERE deleted_at IS NULL").map_err(|_| damaged())?;
    let folders = count(&c, "SELECT count(*) FROM folders").map_err(|_| damaged())?;
    // The other tables a restore relies on.
    for t in ["apps", "clip_formats", "snippets"] {
        count(&c, &format!("SELECT count(*) FROM {t}")).map_err(|_| damaged())?;
    }
    Ok((all, live, folders))
}

/// `backup_import('replace')`: returns the report (restored records / folders) and the
/// backup's `settings.json` for the caller to apply.
pub(crate) fn restore_replace(store: &Store, archive: &Path, progress: Progress<'_>) -> Result<(ImportReport, Option<Vec<u8>>), String> {
    let (mut zip, _manifest) = open_archive(archive)?;
    // Leftovers of an interrupted restore.
    if let Ok(rd) = std::fs::read_dir(store.data_dir()) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(STAGING_PREFIX) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let staging = TempDir::new_in(store.data_dir(), STAGING_PREFIX)?;
    let settings = extract(&mut zip, staging.path(), progress)?;
    drop(zip);
    let (all, live, folders) = check_db(&staging.path().join("liqlip.db"))?;
    progress("clips", 0, 1);
    store.replace_with(staging.path(), all)?;
    progress("clips", 1, 1);
    Ok((ImportReport { imported: live, skipped: 0, folders, errors: 0 }, settings))
}

/// A backup app: exe, name and its icon (if packed).
struct BackupApp {
    exe: String,
    name: String,
    icon: Option<Vec<u8>>,
}

struct BackupRow {
    id: i64,
    kind: String,
    title: Option<String>,
    preview: String,
    text: Option<String>,
    hash: String,
    metrics: (Option<i64>, Option<i64>, Option<i64>),
    width: Option<i64>,
    height: Option<i64>,
    file_count: Option<i64>,
    app_id: Option<i64>,
    created_at: i64,
    copied_at: i64,
    copy_count: i64,
    used_at: Option<i64>,
    use_count: i64,
    favorite: bool,
    folder: Option<String>,
    ocr_text: Option<String>,
    ocr_state: i64,
    meta: Option<String>,
}

fn backup_item(
    src: &Connection,
    dir: &Path,
    r: BackupRow,
    apps: &HashMap<i64, BackupApp>,
    favs: &HashMap<i64, usize>,
) -> Result<ImportItem, String> {
    let kind = Kind::parse(&r.kind).ok_or("invalid: unknown kind")?;
    let mut formats: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    {
        let mut st = src
            .prepare_cached("SELECT format, data FROM clip_formats WHERE clip_id = ?1")
            .map_err(|e| format!("failed: {e}"))?;
        let rows = st
            .query_map([r.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?)))
            .map_err(|e| format!("failed: {e}"))?;
        for row in rows {
            let (f, d) = row.map_err(|e| format!("failed: {e}"))?;
            formats.insert(f, d);
        }
    }
    let media = dir.join("media").join(format!("{}.png", r.id));
    let png = if formats.contains_key("png") { std::fs::read(&media).ok() } else { None };
    if kind == Kind::Image && png.is_none() {
        return Err("not_found: image file missing in the backup".into());
    }
    let thumb = (kind == Kind::Image)
        .then(|| std::fs::read(dir.join("media").join("thumbs").join(format!("{}.png", r.id))).ok())
        .flatten();
    let files = match formats.get("files") {
        Some(Some(d)) => parse_files_blob(d),
        _ => None,
    }
    .or_else(|| {
        (kind == Kind::File).then(|| ClipFiles {
            paths: r.text.as_deref().unwrap_or("").lines().filter(|l| !l.is_empty()).map(str::to_string).collect(),
            drop_effect: 1,
        })
    });
    let app = r.app_id.and_then(|id| apps.get(&id));
    let meta: ClipMeta = r.meta.as_deref().and_then(|m| serde_json::from_str(m).ok()).unwrap_or_default();
    let clip = NewClip {
        kind,
        hash: r.hash,
        preview: r.preview,
        text: r.text,
        meta,
        size: 0,
        char_count: r.metrics.0,
        word_count: r.metrics.1,
        line_count: r.metrics.2,
        width: r.width,
        height: r.height,
        file_count: r.file_count,
        png,
        thumb_png: thumb,
        html: match formats.get("html") {
            Some(Some(d)) => Some(String::from_utf8_lossy(d).into_owned()),
            _ => None,
        },
        rtf: formats.get("rtf").cloned().flatten(),
        files,
        source: app.map(|a| SourceApp { exe: a.exe.clone(), name: a.name.clone() }),
        copied_at: r.copied_at,
    };
    Ok(ImportItem {
        clip,
        created_at: r.created_at,
        title: r.title,
        copy_count: r.copy_count,
        used_at: r.used_at,
        use_count: r.use_count,
        favorite: r.favorite,
        fav_order: favs.get(&r.id).map(|&rank| rank as f64),
        folder: r.folder,
        ocr_text: r.ocr_text,
        ocr_state: r.ocr_state,
        app_icon: app.and_then(|a| a.icon.clone()),
    })
}

/// `backup_import('merge')`: the backup's live records through the shared import loop
/// (history older than the retention cut-off is left out, as for Liquid Tools; with
/// `skip_secrets` = `privacy.secretPatterns`, texts that look like a secret too), then its
/// snippets (`Store::merge_snippets`). Settings are not touched.
pub(crate) fn restore_merge(
    store: &Store,
    archive: &Path,
    cutoff: Option<i64>,
    skip_secrets: bool,
    progress: Progress<'_>,
) -> Result<ImportReport, String> {
    let (mut zip, _manifest) = open_archive(archive)?;
    let tmp = TempDir::new("liqlip-merge-")?;
    extract(&mut zip, tmp.path(), progress)?;
    drop(zip);
    let db = tmp.path().join("liqlip.db");
    check_db(&db)?;
    let err = |e: rusqlite::Error| format!("failed: backup database: {e}");
    let src = ro(&db).map_err(err)?; // declared after `tmp`: closed first
    let cutoff = cutoff.unwrap_or(i64::MIN);
    let folders: Vec<String> = {
        let mut st = src.prepare("SELECT name FROM folders ORDER BY position, id").map_err(err)?;
        let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)?
    };
    let favs: HashMap<i64, usize> = ids(&src, "SELECT id FROM clips WHERE favorite = 1 AND deleted_at IS NULL ORDER BY fav_order, id")
        .map_err(err)?
        .into_iter()
        .enumerate()
        .map(|(rank, id)| (id, rank))
        .collect();
    let apps: HashMap<i64, BackupApp> = {
        let mut st = src.prepare("SELECT id, exe, name, icon_path IS NOT NULL FROM apps").map_err(err)?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)? != 0)))
            .map_err(err)?;
        let mut out = HashMap::new();
        for row in rows {
            let (id, exe, name, has_icon) = row.map_err(err)?;
            let icon = has_icon.then(|| std::fs::read(tmp.path().join("icons").join(format!("{id}.png"))).ok()).flatten();
            out.insert(id, BackupApp { exe, name, icon });
        }
        out
    };
    const FILTER: &str = "c.deleted_at IS NULL AND (c.favorite = 1 OR c.folder_id IS NOT NULL OR c.copied_at >= ?1)";
    let total: i64 = src.query_row(&format!("SELECT count(*) FROM clips c WHERE {FILTER}"), [cutoff], |r| r.get(0)).map_err(err)?;
    let page_sql = format!(
        "SELECT c.id, c.kind, c.title, c.preview, c.text, c.hash, c.char_count, c.word_count, c.line_count, c.width,
           c.height, c.file_count, c.app_id, c.created_at, c.copied_at, c.copy_count, c.used_at, c.use_count, c.favorite,
           f.name, c.ocr_text, c.ocr_state, c.meta
         FROM clips c LEFT JOIN folders f ON f.id = c.folder_id
         WHERE {FILTER} AND (c.copied_at, c.id) > (?2, ?3) ORDER BY c.copied_at, c.id LIMIT {PAGE}"
    );
    let mut after = (i64::MIN, i64::MIN);
    let dir = tmp.path().to_path_buf();
    let report = runner::run(store, &folders, total, skip_secrets, progress, || -> Result<Option<Page>, String> {
        let rows: Vec<BackupRow> = {
            let mut st = src.prepare_cached(&page_sql).map_err(err)?;
            let rows = st
                .query_map(rusqlite::params![cutoff, after.0, after.1], |r| {
                    Ok(BackupRow {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        title: r.get(2)?,
                        preview: r.get(3)?,
                        text: r.get(4)?,
                        hash: r.get(5)?,
                        metrics: (r.get(6)?, r.get(7)?, r.get(8)?),
                        width: r.get(9)?,
                        height: r.get(10)?,
                        file_count: r.get(11)?,
                        app_id: r.get(12)?,
                        created_at: r.get(13)?,
                        copied_at: r.get(14)?,
                        copy_count: r.get(15)?,
                        used_at: r.get(16)?,
                        use_count: r.get(17)?,
                        favorite: r.get::<_, i64>(18)? != 0,
                        folder: r.get(19)?,
                        ocr_text: r.get(20)?,
                        ocr_state: r.get(21)?,
                        meta: r.get(22)?,
                    })
                })
                .map_err(err)?;
            rows.collect::<Result<_, _>>().map_err(err)?
        };
        let Some(last) = rows.last() else { return Ok(None) };
        after = (last.copied_at, last.id);
        Ok(Some(
            rows.into_iter().map(|r| backup_item(&src, &dir, r, &apps, &favs)).collect(),
        ))
    })?;
    let snippets: Vec<SnippetRow> = {
        let mut st = src
            .prepare("SELECT name, body, keyword, created_at, updated_at, use_count FROM snippets ORDER BY position, id")
            .map_err(err)?;
        let rows = st
            .query_map([], |r| {
                Ok(SnippetRow {
                    name: r.get(0)?,
                    body: r.get(1)?,
                    keyword: r.get(2)?,
                    created_at: r.get(3)?,
                    updated_at: r.get(4)?,
                    use_count: r.get(5)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)?
    };
    store.merge_snippets(&snippets)?;
    Ok(report)
}
