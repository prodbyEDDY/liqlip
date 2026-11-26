//! Files in the screenshot folder (owner: W6-A per ARCHITECTURE §18.1; §18.4 step 5).
//!
//! - **Folder**: `settings.screenshot.folder`, or `""` = `<Pictures>\Liqlip` — the Pictures
//!   known folder (`SHGetKnownFolderPath(FOLDERID_Pictures)`, it follows a relocated Pictures
//!   folder). Created on demand.
//! - **Name**: the template `settings.screenshot.fileName` with `{date}` = `YYYY-MM-DD`,
//!   `{time}` = `HHMMSS` (the Snipping Tool's style — no `:`), `{app}` = the source app, `{n}` =
//!   the smallest number ≥ 1 that makes the name free. Without `{n}` a taken name gets ` (2)`,
//!   ` (3)`… Characters Windows forbids in names (`<>:"/\|?*`, control characters) are dropped,
//!   spaces collapsed, trailing dots/spaces trimmed, reserved device names (`CON`, `COM1`…)
//!   prefixed with `_`. The name is claimed atomically (`create_new`), so two shots in the same
//!   second never overwrite each other.
//! - **Format**: PNG (the record's bytes as they are) or JPG (`image`'s encoder, quality
//!   60–100; JPEG has no alpha, so translucent pixels are composited over white).
//! - Commands take a record id, never a path from the webview (research §6.2.9). A shot kept
//!   out of history (§18.4) is a temp PNG named by its file name only ([`temp_file`]: a plain
//!   file directly inside [`temp_dir`], checked after `canonicalize`).
//!
//! https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath
//! ("…the calling process is responsible for freeing this resource … by calling CoTaskMemFree")
//! · https://learn.microsoft.com/windows/win32/fileio/naming-a-file (reserved characters and
//! names, no trailing period or space)

use crate::i18n_native::{self, Key};
use crate::model::{ChangeOrigin, ClipsChange, ClipsChanged, ScreenshotSettings, ShotFormat};
use crate::store::Store;
use crate::{events, fsx, settings};
use image::codecs::jpeg::JpegEncoder;
use image::ExtendedColorType;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager};

/// Sub-folder of Pictures used when `settings.screenshot.folder` is empty.
pub(crate) const SUBFOLDER: &str = "Liqlip";
/// A file name (without extension) is cut to this many characters.
const NAME_MAX_CHARS: usize = 150;
/// Attempts to find a free name before giving up.
const NAME_TRIES: u32 = 10_000;
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Local date and time of a shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

/// A name part safe for Windows (see the module doc); may be empty.
pub(crate) fn sanitize(raw: &str) -> String {
    let kept: String = raw.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { ' ' } else { c }).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The template rendered for one number (`n` replaces `{n}`), sanitized; never empty.
pub(crate) fn render(template: &str, stamp: Stamp, app: Option<&str>, n: u32) -> String {
    let date = format!("{:04}-{:02}-{:02}", stamp.year, stamp.month, stamp.day);
    let time = format!("{:02}{:02}{:02}", stamp.hour, stamp.minute, stamp.second);
    let raw = template
        .replace("{date}", &date)
        .replace("{time}", &time)
        .replace("{app}", &sanitize(app.unwrap_or("")))
        .replace("{n}", &n.to_string());
    let mut name = sanitize(&raw);
    if let Some((cut, _)) = name.char_indices().nth(NAME_MAX_CHARS) {
        name.truncate(cut);
    }
    let name = name.trim_end_matches(['.', ' ']).to_string();
    let stem = name.split('.').next().unwrap_or("");
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return format!("_{name}");
    }
    if name.is_empty() { "Liqlip".into() } else { name }
}

/// The `attempt`-th candidate name (0-based) for a template: `{n}` counts 1, 2, 3…; without
/// it the name, then `name (2)`, `name (3)`…
pub(crate) fn candidate(template: &str, stamp: Stamp, app: Option<&str>, attempt: u32) -> String {
    if template.contains("{n}") {
        render(template, stamp, app, attempt + 1)
    } else {
        let base = render(template, stamp, app, 1);
        if attempt == 0 { base } else { format!("{base} ({})", attempt + 1) }
    }
}

/// File extension of a format.
pub(crate) fn extension(format: ShotFormat) -> &'static str {
    match format {
        ShotFormat::Png => "png",
        ShotFormat::Jpg => "jpg",
    }
}

/// The format a path asks for by its extension (`.jpg` / `.jpeg` → JPG, anything else PNG).
pub(crate) fn format_of(path: &Path) -> ShotFormat {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("jpg" | "jpeg") => ShotFormat::Jpg,
        _ => ShotFormat::Png,
    }
}

/// The bytes of a file: PNG as given, or JPG from the RGBA.
pub(crate) fn encode(format: ShotFormat, quality: u32, rgba: &[u8], width: u32, height: u32, png: &[u8]) -> Result<Vec<u8>, String> {
    match format {
        ShotFormat::Png => Ok(png.to_vec()),
        ShotFormat::Jpg => {
            let rgb: Vec<u8> = rgba
                .chunks_exact(4)
                .flat_map(|p| {
                    let a = u32::from(p[3]);
                    let over_white = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
                    [over_white(p[0]), over_white(p[1]), over_white(p[2])]
                })
                .collect();
            if rgb.len() as u64 != u64::from(width) * u64::from(height) * 3 {
                return Err("invalid: picture size".into());
            }
            let mut out = Vec::new();
            JpegEncoder::new_with_quality(&mut out, quality.clamp(60, 100) as u8)
                .encode(&rgb, width, height, ExtendedColorType::Rgb8)
                .map_err(|e| format!("failed: jpeg: {e}"))?;
            Ok(out)
        }
    }
}

/// Write `bytes` under the first free candidate name in `dir` (created if needed); returns
/// the path. A partly written file is removed.
pub(crate) fn write_unique(dir: &Path, template: &str, stamp: Stamp, app: Option<&str>, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("failed: create {}: {e}", dir.display()))?;
    for attempt in 0..NAME_TRIES {
        let path = dir.join(format!("{}.{ext}", candidate(template, stamp, app, attempt)));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("failed: {}: {e}", path.display())),
        };
        if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(format!("failed: write {}: {e}", path.display()));
        }
        return Ok(path);
    }
    Err("failed: no free file name".into())
}

/// The screenshot folder (not created here).
pub(crate) fn folder(s: &ScreenshotSettings) -> Result<PathBuf, String> {
    let custom = s.folder.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        return if p.is_absolute() { Ok(p) } else { Err(format!("invalid: folder {custom} is not an absolute path")) };
    }
    Ok(pictures()?.join(SUBFOLDER))
}

#[cfg(windows)]
fn pictures() -> Result<PathBuf, String> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Pictures, KF_FLAG_DEFAULT};
    // SAFETY: the returned string is read once and freed with CoTaskMemFree, as documented.
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Pictures, KF_FLAG_DEFAULT, None).map_err(|e| format!("failed: Pictures folder: {e}"))?;
        let s = p.to_string();
        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
        s.map(PathBuf::from).map_err(|e| format!("failed: Pictures folder: {e}"))
    }
}

#[cfg(not(windows))]
fn pictures() -> Result<PathBuf, String> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Pictures")).ok_or_else(|| "failed: no home folder".into())
}

/// Local date/time of a Unix-ms instant (UTC → the user's time zone).
/// https://learn.microsoft.com/windows/win32/api/timezoneapi/nf-timezoneapi-systemtimetotzspecificlocaltime
#[cfg(windows)]
pub(crate) fn stamp_of(unix_ms: i64) -> Stamp {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    let local = (|| {
        // FILETIME = 100 ns ticks since 1601-01-01 UTC (11 644 473 600 s before Unix time).
        let ticks = u64::try_from(unix_ms).ok()?.checked_add(11_644_473_600_000)?.checked_mul(10_000)?;
        let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
        let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
        // SAFETY: pointers to live locals of the right types.
        unsafe {
            FileTimeToSystemTime(&ft, &mut utc).ok()?;
            SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
        }
        Some(Stamp { year: local.wYear, month: local.wMonth, day: local.wDay, hour: local.wHour, minute: local.wMinute, second: local.wSecond })
    })();
    local.unwrap_or_else(|| utc_stamp(unix_ms))
}

#[cfg(not(windows))]
pub(crate) fn stamp_of(unix_ms: i64) -> Stamp {
    utc_stamp(unix_ms)
}

/// UTC date/time (the fallback): days → civil date (H. Hinnant's `civil_from_days`).
pub(crate) fn utc_stamp(unix_ms: i64) -> Stamp {
    let secs = unix_ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Stamp {
        year: year.clamp(0, 9999) as u16,
        month: month as u16,
        day: day as u16,
        hour: (rem / 3600) as u16,
        minute: (rem % 3600 / 60) as u16,
        second: (rem % 60) as u16,
    }
}

/// A new shot into the screenshot folder (commit step 5) → the path.
pub(crate) fn save_new(s: &ScreenshotSettings, app: Option<&str>, at_ms: i64, rgba: &[u8], width: u32, height: u32, png: &[u8]) -> Result<String, String> {
    let dir = folder(s)?;
    let bytes = encode(s.format, s.jpg_quality, rgba, width, height, png)?;
    let path = write_unique(&dir, &s.file_name, stamp_of(at_ms), app, extension(s.format), &bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

fn dialog_name(s: &ScreenshotSettings, app: Option<&str>, at_ms: i64) -> String {
    format!("{}.{}", candidate(&s.file_name, stamp_of(at_ms), app, 0), extension(s.format))
}

fn save_dialog(app: &AppHandle, s: &ScreenshotSettings, name: String) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
    use tauri_plugin_dialog::DialogExt;
    let lang = i18n_native::resolve(settings::get(app).general.language);
    let t = |k| i18n_native::tr(lang, k);
    let (first, second) = match s.format {
        ShotFormat::Png => ((t(Key::ShotFilterPng), &["png"][..]), (t(Key::ShotFilterJpg), &["jpg", "jpeg"][..])),
        ShotFormat::Jpg => ((t(Key::ShotFilterJpg), &["jpg", "jpeg"][..]), (t(Key::ShotFilterPng), &["png"][..])),
    };
    let mut d = app.dialog().file().set_title(t(Key::ShotSaveAsTitle)).add_filter(first.0, first.1).add_filter(second.0, second.1).set_file_name(name);
    if let Ok(dir) = folder(s) {
        if std::fs::create_dir_all(&dir).is_ok() {
            d = d.set_directory(dir);
        }
    }
    d
}

/// The chosen path, with the settings' extension when the user typed none.
fn chosen(file: tauri_plugin_dialog::FilePath, s: &ScreenshotSettings) -> Result<PathBuf, String> {
    let mut p = file.into_path().map_err(|e| format!("invalid: {e}"))?;
    if p.extension().is_none() {
        p.set_extension(extension(s.format));
    }
    Ok(p)
}

/// Write a picture to a path the user chose (format by its extension; replaces the file — the
/// dialog has asked).
fn write_chosen(path: &Path, s: &ScreenshotSettings, rgba: &[u8], width: u32, height: u32, png: &[u8]) -> Result<String, String> {
    let bytes = encode(format_of(path), s.jpg_quality, rgba, width, height, png)?;
    fsx::write_atomic(path, &bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

/// "Save as…" of a commit (overlay `save_as`, editor `save_as`): the dialog BLOCKS this thread
/// (`blocking_save_file` — documented for async commands; never the main thread) → the path,
/// `None` when cancelled.
pub(crate) fn save_new_as(app: &AppHandle, s: &ScreenshotSettings, source: Option<&str>, rgba: &[u8], width: u32, height: u32, png: &[u8]) -> Result<Option<String>, String> {
    let Some(file) = save_dialog(app, s, dialog_name(s, source, crate::store::system_now())).blocking_save_file() else { return Ok(None) };
    let path = chosen(file, s)?;
    write_chosen(&path, s, rgba, width, height, png).map(Some)
}

fn saved(app: &AppHandle, store: &Store, id: i64, path: &str) -> Result<(), String> {
    store.set_saved_path(id, Some(path))?;
    events::clips_changed(app, &ClipsChanged { origin: ChangeOrigin::User, change: ClipsChange::Updated, ids: Some(vec![id]) });
    Ok(())
}

/// `shot_save { id }` → the written path (the folder and the name template; `{date}` /
/// `{time}` = when the record was created).
pub(crate) fn save(app: &AppHandle, id: i64) -> Result<String, String> {
    let s = settings::get(app).screenshot;
    let store = app.state::<Store>();
    let src = store.shot_source(id)?;
    let png = std::fs::read(&src.png).map_err(|_| format!("not_found: picture of clip {id}"))?;
    let (rgba, w, h) = pixels_for(s.format, &png)?;
    let dir = folder(&s)?;
    let bytes = encode(s.format, s.jpg_quality, &rgba, w, h, &png)?;
    let path = write_unique(&dir, &s.file_name, stamp_of(src.created_at), src.app.as_deref(), extension(s.format), &bytes)?;
    let path = path.to_string_lossy().into_owned();
    saved(app, &store, id, &path)?;
    Ok(path)
}

/// `shot_save_as { id }` → the chosen path, `None` = the dialog was cancelled.
pub(crate) async fn save_as(app: AppHandle, id: i64) -> Result<Option<String>, String> {
    let src = app.state::<Store>().shot_source(id)?;
    let png = std::fs::read(&src.png).map_err(|_| format!("not_found: picture of clip {id}"))?;
    save_png_as(app, id, png).await
}

/// RGBA8 of a PNG, only when the target format needs pixels (JPG).
fn pixels_for(format: ShotFormat, png: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    match format {
        ShotFormat::Png => Ok((Vec::new(), 0, 0)),
        ShotFormat::Jpg => {
            let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).map_err(|e| format!("failed: picture: {e}"))?.to_rgba8();
            let (w, h) = (img.width(), img.height());
            Ok((img.into_raw(), w, h))
        }
    }
}

/// "Save as…" of a picture of record `id` (`shot_save_as`; the editor's "Save as…"): the dialog (no parent window; it answers through a callback, as
/// `settings::pick_exe`), then the PNG written in the format of the chosen extension, recorded
/// as the record's saved file (`clips:changed`). `None` = cancelled.
pub(crate) async fn save_png_as(app: AppHandle, id: i64, png: Vec<u8>) -> Result<Option<String>, String> {
    let s = settings::get(&app).screenshot;
    let src = app.state::<Store>().shot_source(id)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    save_dialog(&app, &s, dialog_name(&s, src.app.as_deref(), src.created_at)).save_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(file) = rx.await.map_err(|_| "failed: dialog closed".to_string())? else { return Ok(None) };
    let path = chosen(file, &s)?;
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (rgba, w, h) = pixels_for(format_of(&path), &png)?;
        let written = write_chosen(&path, &s, &rgba, w, h, &png)?;
        saved(&app2, &app2.state::<Store>(), id, &written)?;
        Ok(Some(written))
    })
    .await
    .map_err(|e| format!("failed: {e}"))?
}

/// The editor's "Done" (`editor::commit`): rewrite the record's saved file with a new picture,
/// in the file's own format (by extension). `Ok(None)` when the record was never saved or the
/// file is gone (nothing is created). Blocking — call from a worker.
pub(crate) fn overwrite_saved(app: &AppHandle, id: i64, png: &[u8]) -> Result<Option<String>, String> {
    let src = app.state::<Store>().shot_source(id)?;
    let Some(path) = src.saved_path.filter(|p| Path::new(p).is_file()) else { return Ok(None) };
    let quality = settings::get(app).screenshot.jpg_quality;
    let (rgba, w, h) = pixels_for(format_of(Path::new(&path)), png)?;
    rewrite(&path, quality, &rgba, w, h, png)?;
    Ok(Some(path))
}

/// `shot_reveal { id }` — Explorer with the saved file selected (`tauri-plugin-opener`'s
/// `reveal_item_in_dir` = `SHOpenFolderAndSelectItems`, as `clip_reveal`); `not_found` when the
/// record was never saved or the file is gone.
pub(crate) fn reveal(app: &AppHandle, id: i64) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let src = app.state::<Store>().shot_source(id)?;
    let path = src.saved_path.filter(|p| Path::new(p).is_file()).ok_or("not_found: the saved file no longer exists")?;
    app.opener().reveal_item_in_dir(path).map_err(|e| format!("failed: {e}"))
}

/// The editor's "Done": the file saved before is rewritten in its own format (by extension).
pub(crate) fn rewrite(path: &str, quality: u32, rgba: &[u8], width: u32, height: u32, png: &[u8]) -> Result<(), String> {
    let p = Path::new(path);
    let bytes = encode(format_of(p), quality, rgba, width, height, png)?;
    fsx::write_atomic(p, &bytes)
}

/// `%TEMP%\Liqlip\shots\` — PNGs of shots that are not kept in the history (§18.4).
pub(crate) fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("Liqlip").join("shots")
}

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A temporary PNG for the QAO (deleted by `qao.rs` when the stack no longer needs it, and at
/// the next start).
pub(crate) fn write_temp(png: &[u8]) -> Result<String, String> {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed: {e}"))?;
    let name = format!("shot-{}-{}.png", crate::store::system_now(), TEMP_SEQ.fetch_add(1, Ordering::Relaxed));
    let path = dir.join(name);
    fsx::write_atomic(&path, png)?;
    Ok(path.to_string_lossy().into_owned())
}

/// A plain file directly inside `root` (after resolving `..` and links) — nothing else is ever
/// served or removed as a temp shot, whatever a payload or a page says.
pub(crate) fn is_temp_file(root: &Path, p: &Path) -> bool {
    let (Ok(root), Ok(file)) = (root.canonicalize(), p.canonicalize()) else { return false };
    file.parent() == Some(root.as_path()) && file.is_file()
}

/// A temp shot by its bare file name (`liq://temp/<name>`, `shot_temp_copy`,
/// `shot_temp_save_as`): ASCII letters, digits, `.`, `-`, `_` (what [`write_temp`] makes), no
/// leading dot, then [`is_temp_file`].
pub(crate) fn temp_file(name: &str) -> Result<PathBuf, String> {
    let plain = !name.is_empty() && name.len() <= 128 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !plain {
        return Err("invalid: temp file name".into());
    }
    let root = temp_dir();
    let path = root.join(name);
    if is_temp_file(&root, &path) {
        Ok(path)
    } else {
        Err("not_found: the temporary picture is gone".into())
    }
}

/// `liq://temp/<name>` — the PNG of a QAO card without a record.
pub(crate) fn serve_temp(name: &str) -> Option<Vec<u8>> {
    std::fs::read(temp_file(name).ok()?).ok()
}

/// `shot_temp_copy { name }` — the temp picture onto the clipboard (PNG + DIB).
pub(crate) fn temp_copy(name: &str) -> Result<(), String> {
    let png = std::fs::read(temp_file(name)?).map_err(|_| "not_found: the temporary picture is gone".to_string())?;
    crate::paste::copy_png(png, None)
}

/// `shot_temp_save_as { name }` — "Save as…" for a temp picture (no record: nothing else
/// changes) → the written path, `None` = cancelled. The name template uses the file's time.
pub(crate) async fn save_temp_as(app: AppHandle, name: String) -> Result<Option<String>, String> {
    let path = temp_file(&name)?;
    let png = std::fs::read(&path).map_err(|_| "not_found: the temporary picture is gone".to_string())?;
    let at = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or_else(crate::store::system_now, |d| d.as_millis() as i64);
    let s = settings::get(&app).screenshot;
    let (tx, rx) = tokio::sync::oneshot::channel();
    save_dialog(&app, &s, dialog_name(&s, None, at)).save_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(file) = rx.await.map_err(|_| "failed: dialog closed".to_string())? else { return Ok(None) };
    let target = chosen(file, &s)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (rgba, w, h) = pixels_for(format_of(&target), &png)?;
        write_chosen(&target, &s, &rgba, w, h, &png).map(Some)
    })
    .await
    .map_err(|e| format!("failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Stamp = Stamp { year: 2026, month: 11, day: 26, hour: 9, minute: 5, second: 7 };

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> TempDir {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!("liqlip-files-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_template_renders_date_time_app_and_number() {
        assert_eq!(render("Liqlip {date} {time}", AT, None, 1), "Liqlip 2026-11-26 090507");
        assert_eq!(render("{app} {date} {n}", AT, Some("Visual Studio Code"), 3), "Visual Studio Code 2026-11-26 3");
        assert_eq!(render("Shot {app} {time}", AT, None, 1), "Shot 090507", "no app → no double space");
        assert_eq!(render("{app}", AT, Some("a<b>:c\"d/e\\f|g?h*i\u{7}"), 1), "a b c d e f g h i");
        assert_eq!(render("trailing...  ", AT, None, 1), "trailing");
        assert_eq!(render("  ", AT, None, 1), "Liqlip");
        assert_eq!(render("CON", AT, None, 1), "_CON");
        assert_eq!(render("com1.backup", AT, None, 1), "_com1.backup");
        assert_eq!(render(&"x".repeat(300), AT, None, 1).chars().count(), 150);
    }

    #[test]
    fn candidates_count_with_n_or_append_a_number() {
        assert_eq!(candidate("Shot {n}", AT, None, 0), "Shot 1");
        assert_eq!(candidate("Shot {n}", AT, None, 4), "Shot 5");
        assert_eq!(candidate("Shot", AT, None, 0), "Shot");
        assert_eq!(candidate("Shot", AT, None, 1), "Shot (2)");
        assert_eq!(candidate("Shot", AT, None, 2), "Shot (3)");
    }

    #[test]
    fn names_are_claimed_without_overwriting() {
        let dir = TempDir::new();
        let a = write_unique(&dir.0.join("new sub"), "Liqlip {date}", AT, None, "png", b"one").unwrap();
        let b = write_unique(&dir.0.join("new sub"), "Liqlip {date}", AT, None, "png", b"two").unwrap();
        assert_eq!(a.file_name().unwrap(), "Liqlip 2026-11-26.png");
        assert_eq!(b.file_name().unwrap(), "Liqlip 2026-11-26 (2).png");
        assert_eq!(std::fs::read(&a).unwrap(), b"one");
        let n1 = write_unique(&dir.0, "S{n}", AT, None, "jpg", b"x").unwrap();
        let n2 = write_unique(&dir.0, "S{n}", AT, None, "jpg", b"y").unwrap();
        assert_eq!(n1.file_name().unwrap(), "S1.jpg");
        assert_eq!(n2.file_name().unwrap(), "S2.jpg");
    }

    #[test]
    fn jpg_is_encoded_over_white_and_png_kept() {
        let rgba = [255u8, 0, 0, 255, 0, 0, 0, 0];
        let png = crate::screenshot::commit::encode_png(2, 1, &rgba).unwrap();
        assert_eq!(encode(ShotFormat::Png, 90, &rgba, 2, 1, &png).unwrap(), png);
        let jpg = encode(ShotFormat::Jpg, 95, &rgba, 2, 1, &png).unwrap();
        let back = image::load_from_memory_with_format(&jpg, image::ImageFormat::Jpeg).unwrap().to_rgb8();
        assert!(back.get_pixel(1, 0).0.iter().all(|&c| c > 200), "transparent → white, {:?}", back.get_pixel(1, 0));
        assert!(back.get_pixel(0, 0).0[0] > 150);
        assert_eq!(format_of(Path::new("C:/a/b.JPEG")), ShotFormat::Jpg);
        assert_eq!(format_of(Path::new("C:/a/b.png")), ShotFormat::Png);
        assert_eq!(format_of(Path::new("C:/a/b")), ShotFormat::Png);
    }

    #[test]
    fn the_folder_is_the_setting_or_pictures() {
        let mut s = ScreenshotSettings::default();
        let default = folder(&s).unwrap();
        assert!(default.ends_with(SUBFOLDER) && default.is_absolute(), "{}", default.display());
        s.folder = "relative\\path".into();
        assert!(folder(&s).unwrap_err().starts_with("invalid:"));
        s.folder = std::env::temp_dir().to_string_lossy().into_owned();
        assert_eq!(folder(&s).unwrap(), std::env::temp_dir());
    }

    #[test]
    fn utc_dates_are_civil() {
        assert_eq!(utc_stamp(0), Stamp { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0 });
        // 2026-11-26 09:05:07 UTC
        assert_eq!(utc_stamp(1_795_683_907_000), AT);
        assert_eq!(utc_stamp(951_782_400_000), Stamp { year: 2000, month: 2, day: 29, hour: 0, minute: 0, second: 0 });
    }
}
