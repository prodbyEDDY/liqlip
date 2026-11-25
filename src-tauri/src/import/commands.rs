//! `import` commands (ARCHITECTURE §9.6, §16) — owner: W2-B.
//! JS-visible signatures are the W0 contract. The SQLite / file work runs on the blocking
//! pool; one import or backup operation at a time (`busy:` otherwise).
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`; a cancelled
//! restore dialog is `invalid: cancelled` (a cancelled export is `Ok(null)`).

use super::{backup, lt};
use crate::i18n_native::{self, Key};
use crate::model::{BackupMode, ChangeOrigin, ClipsChange, ClipsChanged, ImportProbe, ImportProgress, ImportReport, Settings};
use crate::store::Store;
use crate::{events, logx, settings};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

static BUSY: AtomicBool = AtomicBool::new(false);

/// Held while an import / export / restore runs.
struct Busy;

impl Busy {
    fn take() -> Result<Busy, String> {
        if BUSY.swap(true, Ordering::AcqRel) {
            return Err("busy: an import or a backup is already running".into());
        }
        Ok(Busy)
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

/// Run `f` with the managed `Store` on the blocking pool.
async fn blocking<T, F>(app: &AppHandle, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle, &Store) -> Result<T, String> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.try_state::<Store>().ok_or_else(|| "failed: store is not ready".to_string())?;
        f(&app, &store)
    })
    .await
    .map_err(|e| format!("failed: {e}"))?
}

fn progress(app: &AppHandle) -> impl Fn(&str, i64, i64) + '_ {
    move |phase: &str, done: i64, total: i64| {
        events::import_progress(app, &ImportProgress { phase: phase.into(), done, total });
    }
}

/// After an import / restore: tell the windows (no ids — they re-read) and let OCR pick up
/// the new images (they arrive with `ocr_state = none`; W2-A `ocr::wake` catches up).
fn changed(app: &AppHandle, change: ClipsChange) {
    events::clips_changed(app, &ClipsChanged { origin: ChangeOrigin::Import, change, ids: None });
    crate::ocr::wake(app);
}

fn lt_db(app: &AppHandle) -> Result<PathBuf, String> {
    let roaming = app.path().data_dir().map_err(|e| format!("failed: {e}"))?;
    Ok(lt::default_db(&roaming))
}

fn cutoff(app: &AppHandle, store: &Store) -> Option<i64> {
    store.retention_cutoff(settings::get(app).storage.retention)
}

/// `privacy.secretPatterns`: imports leave out texts that look like a secret (FIX-01).
fn skip_secrets(app: &AppHandle) -> bool {
    settings::get(app).privacy.secret_patterns
}

/// Local date and time (`GetLocalTime`) for the default file name and the zip entries.
#[cfg(windows)]
fn local_now() -> (u16, u16, u16, u16, u16, u16) {
    // SAFETY: no arguments; fills and returns a SYSTEMTIME.
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

#[cfg(not(windows))]
fn local_now() -> (u16, u16, u16, u16, u16, u16) {
    (2026, 1, 1, 0, 0, 0)
}

async fn file_dialog(app: &AppHandle, save_as: Option<String>) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let lang = i18n_native::resolve(settings::get(app).general.language);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let title = if save_as.is_some() { Key::BackupExportTitle } else { Key::BackupRestoreTitle };
    let mut dialog = app
        .dialog()
        .file()
        .set_title(i18n_native::tr(lang, title))
        .add_filter(i18n_native::tr(lang, Key::BackupFilter), &["liqlip"]);
    if let Some(win) = app.get_webview_window(events::SETTINGS) {
        dialog = dialog.set_parent(&win);
    }
    match save_as {
        Some(name) => dialog.set_file_name(name).save_file(move |p| {
            let _ = tx.send(p);
        }),
        None => dialog.pick_file(move |p| {
            let _ = tx.send(p);
        }),
    }
    match rx.await.map_err(|_| "failed: dialog closed".to_string())? {
        None => Ok(None),
        Some(file) => file.into_path().map(Some).map_err(|e| format!("invalid: {e}")),
    }
}

/// The backup's settings, read field by field like `settings.json` at start-up
/// (`settings::parse_file`, FIX-03): a field that does not fit — or that the backup lacks —
/// takes its default, every other field is restored. Kept from the current settings: the
/// settings window geometry (machine-specific) and a finished onboarding. A file that is not
/// a JSON object at all is ignored (logged by the caller): it would reset everything.
fn restored_settings(current: &Settings, raw: &[u8]) -> Option<Settings> {
    let file: serde_json::Value = serde_json::from_slice(raw).ok()?;
    if !file.is_object() {
        return None;
    }
    let mut s = settings::parse_file(raw);
    s.window = current.window.clone();
    s.general.onboarding_done |= current.general.onboarding_done;
    Some(s)
}

/// Look for a Liquid Tools clipboard database.
#[tauri::command]
pub async fn import_lt_probe(app: AppHandle) -> Result<ImportProbe, String> {
    blocking(&app, |app, store| lt::probe(&lt_db(app)?, cutoff(app, store))).await
}

/// Import Liquid Tools history (idempotent; `import:progress`).
#[tauri::command]
pub async fn import_lt_run(app: AppHandle) -> Result<ImportReport, String> {
    let report = blocking(&app, |app, store| {
        let _busy = Busy::take()?;
        lt::run(store, &lt_db(app)?, cutoff(app, store), skip_secrets(app), &progress(app))
    })
    .await?;
    changed(&app, ClipsChange::Added);
    Ok(report)
}

/// Export a `.liqlip` backup; returns the path (`null` = cancelled).
#[tauri::command]
pub async fn backup_export(app: AppHandle) -> Result<Option<String>, String> {
    let (y, mo, d, h, mi, s) = local_now();
    let Some(mut dest) = file_dialog(&app, Some(backup::default_file_name(y, mo, d))).await? else {
        return Ok(None);
    };
    if dest.extension().is_none() {
        dest.set_extension("liqlip");
    }
    let path = dest.clone();
    blocking(&app, move |app, store| {
        let _busy = Busy::take()?;
        let settings_json = serde_json::to_vec_pretty(&settings::get(app)).map_err(|e| format!("failed: {e}"))?;
        let zip_time = zip::DateTime::from_date_and_time(y, mo as u8, d as u8, h as u8, mi as u8, s as u8).unwrap_or_default();
        let version = app.package_info().version.to_string();
        backup::export(store, &settings_json, &version, store.now(), zip_time, &path, &progress(app))
    })
    .await?;
    Ok(Some(dest.to_string_lossy().into_owned()))
}

/// Restore a backup (replace or merge).
#[tauri::command]
pub async fn backup_import(app: AppHandle, mode: BackupMode) -> Result<ImportReport, String> {
    let Some(src) = file_dialog(&app, None).await? else {
        return Err("invalid: cancelled".into());
    };
    match mode {
        BackupMode::Merge => {
            let report = blocking(&app, move |app, store| {
                let _busy = Busy::take()?;
                backup::restore_merge(store, &src, cutoff(app, store), skip_secrets(app), &progress(app))
            })
            .await?;
            changed(&app, ClipsChange::Added);
            Ok(report)
        }
        BackupMode::Replace => {
            let (report, settings_json) = blocking(&app, move |app, store| {
                let _busy = Busy::take()?;
                let out = backup::restore_replace(store, &src, &progress(app))?;
                // Paste-stack items point at ids of the replaced database.
                if let Err(e) = crate::paste::stack::mutate(app, |s| {
                    s.clear();
                    Ok(())
                }) {
                    logx::warn("import", &format!("clear paste stack: {e}"));
                }
                Ok(out)
            })
            .await?;
            if let Some(raw) = settings_json {
                match restored_settings(&settings::get(&app), &raw) {
                    Some(s) => match settings::settings_set(app.clone(), s).await {
                        Ok(r) if !r.errors.is_empty() => {
                            logx::warn("import", &format!("restored settings: {} fields kept their value", r.errors.len()));
                        }
                        Ok(_) => {}
                        Err(e) => logx::warn("import", &format!("restored settings not applied: {e}")),
                    },
                    None => logx::warn("import", "backup settings.json unreadable; current settings kept"),
                }
            }
            changed(&app, ClipsChange::Reset);
            Ok(report)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_settings_keep_machine_bits() {
        let mut current = Settings::default();
        current.general.onboarding_done = true;
        current.window.settings_bounds = Some(crate::model::Bounds { x: 1, y: 2, w: 900, h: 700 });
        let mut backup = Settings::default();
        backup.picker.favorites_first = true;
        backup.storage.max_entries = 20_000;
        backup.window.settings_bounds = Some(crate::model::Bounds { x: 9, y: 9, w: 999, h: 999 });
        let raw = serde_json::to_vec(&backup).unwrap();
        let s = restored_settings(&current, &raw).unwrap();
        assert!(s.picker.favorites_first);
        assert_eq!(s.storage.max_entries, 20_000);
        assert_eq!(s.window, current.window, "geometry is machine-specific");
        assert!(s.general.onboarding_done);
        assert_eq!(s.version, crate::settings::CURRENT_VERSION);
        // A partial (older) file: what it lacks takes the default, like a settings.json at start-up.
        let s = restored_settings(&current, br#"{"picker":{"hints":false}}"#).unwrap();
        assert!(!s.picker.hints);
        assert_eq!(s.storage, Settings::default().storage);
        // Garbage → ignored (the current settings stay).
        assert!(restored_settings(&current, b"not json").is_none());
        assert!(restored_settings(&current, b"[1, 2]").is_none());
    }

    /// FIX-03: one broken field no longer throws away the whole file.
    #[test]
    fn restored_settings_survive_a_broken_field() {
        let mut current = Settings::default();
        current.general.onboarding_done = true;
        current.window.settings_bounds = Some(crate::model::Bounds { x: 1, y: 2, w: 900, h: 700 });
        let raw = br#"{
            "version": 1,
            "general": { "language": "ru", "theme": "neon", "onboardingDone": false },
            "picker": { "hints": "yes", "favoritesFirst": true, "position": "left" },
            "storage": { "maxEntries": 20000, "retention": "1y" },
            "privacy": { "ignoredApps": ["vault.exe"], "secretPatterns": false },
            "window": { "settingsBounds": { "x": 9, "y": 9, "w": 999, "h": 999 } }
        }"#;
        let s = restored_settings(&current, raw).unwrap();
        // Broken fields → defaults.
        assert_eq!(s.general.theme, Settings::default().general.theme);
        assert!(s.picker.hints);
        // Everything else is restored.
        assert_eq!(s.general.language, crate::model::Language::Ru);
        assert!(s.picker.favorites_first);
        assert_eq!(s.picker.position, crate::model::Position::Left);
        assert_eq!(s.storage.max_entries, 20_000);
        assert_eq!(s.storage.retention, crate::model::Retention::Year1);
        assert_eq!(s.privacy.ignored_apps, vec!["vault.exe".to_string()]);
        assert!(!s.privacy.secret_patterns);
        // Machine bits stay.
        assert_eq!(s.window, current.window);
        assert!(s.general.onboarding_done);
    }
}
