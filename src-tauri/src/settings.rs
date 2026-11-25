//! Settings: defaults (§9.4), atomic load/save of `settings.json`, version migration,
//! validation, `settings_get` / `settings_set`, broadcast of `settings:changed`, module
//! subscriptions, and the small "about" commands (`app_info`, `open_path`, `pick_exe`).
//! Coordinator file (W0). Modules react to changes through [`subscribe`]; hotkeys get a
//! synchronous hook ([`crate::hotkeys::apply_settings`]) because a taken chord must be
//! reported back in the same `SettingsResult`.

use crate::model::{
    AppMeta, Bounds, ClickAction, GeneralSettings, HotkeySettings, Language, MergeSeparator,
    OcrSettings, PathKind, PerKind, PickerSettings, Position, PrivacySettings, RememberSearch,
    Retention, Settings, SettingsError, SettingsErrorCode, SettingsResult, StackSettings,
    StorageSettings, ThemePref, WindowSettings,
};
use crate::{autostart, events, i18n_native, logx, theme};
use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

pub const CURRENT_VERSION: u32 = 1;
const FILE: &str = "settings.json";

impl Default for Settings {
    /// ARCHITECTURE §9.4 defaults. `tests/fixtures/settings.default.json` pins them and the
    /// browser mock (`src/lib/ipc/mock/settings.ts`) is tested against the same fixture.
    fn default() -> Self {
        Settings {
            version: CURRENT_VERSION,
            general: GeneralSettings {
                language: Language::System,
                theme: ThemePref::System,
                autostart: true,
                onboarding_done: false,
            },
            picker: PickerSettings {
                position: Position::Center,
                center_row: false,
                monitor: "cursor".into(),
                hover_open: false,
                hints: true,
                favorites_first: false,
                click_action: ClickAction::Paste,
                close_on_copy: true,
                always_plain: false,
                bump_on_paste: false,
                remember_search: RememberSearch::Sec60,
                merge_separator: MergeSeparator::Newline,
                paste_keys: Vec::new(),
            },
            hotkeys: HotkeySettings {
                summon: "Alt + V".into(),
                stack_next: "Alt + Shift + V".into(),
                pause_toggle: String::new(),
                ignore_next: String::new(),
            },
            storage: StorageSettings {
                retention: Retention::Month3,
                max_entries: 5000,
                max_size_mb: 1024,
                types: PerKind::splat(true),
            },
            privacy: PrivacySettings {
                respect_markers: true,
                ignored_apps: vec![
                    "keepass.exe".into(),
                    "keepassxc.exe".into(),
                    "1password.exe".into(),
                    "bitwarden.exe".into(),
                ],
                secret_patterns: true,
            },
            stack: StackSettings { newline_after_paste: false },
            ocr: OcrSettings { enabled: true, language: "auto".into() },
            window: WindowSettings { settings_bounds: None },
        }
    }
}

// ── State ───────────────────────────────────────────────────────────────────

type Subscriber = Arc<dyn Fn(&Settings, &Settings) + Send + Sync>;

/// Managed state (`app.manage`), created in `setup()` step 3 before any window exists.
pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Settings>,
    /// Serializes whole read-modify-write operations (`settings_set`, bounds patches).
    write: Mutex<()>,
    subscribers: Mutex<Vec<Subscriber>>,
}

impl SettingsStore {
    /// Load `settings.json` from `data_dir` (missing → defaults). Never fails: unreadable
    /// or partially invalid files degrade field by field to the defaults (logged). The
    /// file itself is only rewritten on the next change.
    pub fn load(data_dir: &Path) -> SettingsStore {
        let path = data_dir.join(FILE);
        let settings = match read_retrying(&path) {
            Ok(raw) => {
                let (s, clean) = parse_lenient(&raw);
                if !clean {
                    logx::warn("settings", "settings.json had invalid fields; defaults used for them");
                    // The next change rewrites the file from what was understood: keep the
                    // original for the user (one copy, overwritten next time).
                    if let Err(e) = std::fs::write(data_dir.join(INVALID_COPY), &raw) {
                        logx::warn("settings", &format!("keeping a copy of settings.json failed: {e}"));
                    }
                }
                s
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                logx::warn("settings", &format!("read failed ({e}); using defaults"));
                Settings::default()
            }
        };
        SettingsStore {
            path,
            current: RwLock::new(settings),
            write: Mutex::new(()),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    fn save(&self, s: &Settings) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
        write_durable(&self.path, &json)
    }
}

/// A copy of a `settings.json` that did not fully parse, kept before it is rewritten.
const INVALID_COPY: &str = "settings.json.invalid";

/// Read the file, retrying a transient failure (an antivirus or a sync tool holding it) a
/// few times: giving up too early would start on the defaults and the next change would
/// overwrite the user's real file with them. `NotFound` is final at once.
fn read_retrying(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut last = None;
    for delay in [0u64, 50, 150, 300] {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        match std::fs::read(path) {
            Ok(raw) => return Ok(raw),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(e),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("read failed")))
}

/// Atomic AND durable replace (W3): `<file>.tmp` is written and flushed to the disk
/// (`sync_all` → FlushFileBuffers) before it is renamed over the target. Without the flush, a
/// power loss right after the rename can leave an empty `settings.json` — every setting,
/// the user's ignored apps included, would silently fall back to the defaults.
fn write_durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Current settings (a clone; cheap — read lock only).
pub fn get(app: &AppHandle) -> Settings {
    app.state::<SettingsStore>().current.read().clone()
}

/// Register `f(old, new)`, called after every applied change (from the thread that ran
/// `settings_set`, after the file is saved, before `settings:changed` is emitted).
/// Subscribers must be quick and must NOT call `settings_set` / [`patch_window_bounds`]
/// (the write lock is held). Call from a module's `init`/`start` in `setup()`.
pub fn subscribe(app: &AppHandle, f: impl Fn(&Settings, &Settings) + Send + Sync + 'static) {
    app.state::<SettingsStore>().subscribers.lock().push(Arc::new(f));
}

/// Persist the settings window geometry without broadcasting (not UI-observable).
pub fn patch_window_bounds(app: &AppHandle, bounds: Bounds) {
    let store = app.state::<SettingsStore>();
    let _guard = store.write.lock();
    let mut next = store.current.read().clone();
    if next.window.settings_bounds == Some(bounds) {
        return;
    }
    next.window.settings_bounds = Some(bounds);
    let (next, errors) = validate(&store.current.read().clone(), next);
    if !errors.is_empty() {
        return;
    }
    if let Err(e) = store.save(&next) {
        logx::warn("settings", &format!("save bounds failed: {e}"));
    }
    *store.current.write() = next;
}

/// `setup()` step 3, right after `manage`: reconcile launch-on-login with the setting.
/// (The theme is applied in step 9, once the windows exist.)
pub fn apply_startup(app: &AppHandle) {
    autostart::sync_startup(app, get(app).general.autostart);
}

// ── Load: migration + field-level leniency ──────────────────────────────────

/// Bring a parsed file up to [`CURRENT_VERSION`]. v1 is the first version: a missing
/// `version` means v1; a NEWER file (downgrade) is read leniently as v1 and logged.
fn migrate(mut file: Value) -> Value {
    let version = file.get("version").and_then(Value::as_u64).unwrap_or(1);
    // Future: `if version < 2 { …rewrite v1 → v2… }` — one step per version.
    if version > u64::from(CURRENT_VERSION) {
        logx::warn("settings", &format!("settings.json v{version} is newer than this build; reading as v1"));
    }
    if let Value::Object(map) = &mut file {
        map.insert("version".into(), Value::from(CURRENT_VERSION));
    }
    file
}

/// A `settings.json` read field by field exactly like at start-up ([`parse_lenient`]:
/// migration, a field that does not fit keeps its default, semantic validation). Used to
/// restore the settings of a backup (`import::commands`, FIX-03), so one broken field no
/// longer throws away the whole file.
pub(crate) fn parse_file(raw: &[u8]) -> Settings {
    parse_lenient(raw).0
}

/// Overlay the file onto the defaults one field at a time; a field whose value does not
/// deserialize (wrong type, unknown enum value) keeps its default. Returns the settings
/// and whether the file was fully valid.
fn parse_lenient(raw: &[u8]) -> (Settings, bool) {
    let defaults = Settings::default();
    let Ok(file) = serde_json::from_slice::<Value>(raw) else {
        return (defaults, false);
    };
    let file = migrate(file);
    let Ok(mut acc) = serde_json::to_value(&defaults) else {
        return (defaults, false);
    };
    let sections: Vec<(String, Value)> = match &acc {
        Value::Object(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => Vec::new(),
    };
    let fits = |v: &Value| serde_json::from_value::<Settings>(v.clone()).is_ok();
    let mut clean = file.is_object();
    for (section, def) in sections {
        let Some(file_sec) = file.get(&section) else { continue };
        match (&def, file_sec) {
            (Value::Object(fields), Value::Object(_)) => {
                for field in fields.keys() {
                    let Some(fv) = file_sec.get(field) else { continue };
                    let mut cand = acc.clone();
                    cand[&section][field] = fv.clone();
                    if fits(&cand) {
                        acc = cand;
                    } else {
                        clean = false;
                    }
                }
            }
            _ => {
                let mut cand = acc.clone();
                cand[&section] = file_sec.clone();
                if fits(&cand) {
                    acc = cand;
                } else {
                    clean = false;
                }
            }
        }
    }
    let parsed: Settings = serde_json::from_value(acc).unwrap_or_else(|_| defaults.clone());
    let (s, errors) = validate(&defaults, parsed);
    (s, clean && errors.is_empty())
}

// ── Validation ──────────────────────────────────────────────────────────────

const MAX_PATH_CHARS: usize = 260;

fn invalid(errors: &mut Vec<SettingsError>, field: &str) {
    errors.push(SettingsError { field: field.into(), code: SettingsErrorCode::Invalid });
}

fn bad_name(s: &str, max: usize) -> bool {
    s.is_empty() || s.chars().count() > max || s.chars().any(char::is_control)
}

/// Semantic checks the type system can't express. Every invalid field keeps its `prev`
/// value and yields `{ field, code: "invalid" }`; valid fields are applied. Strings are
/// trimmed. (Type-level garbage — wrong JSON types, unknown enum values — is rejected by
/// Tauri's argument deserialization before `settings_set` runs.)
pub(crate) fn validate(prev: &Settings, mut next: Settings) -> (Settings, Vec<SettingsError>) {
    let mut errors = Vec::new();

    if next.version != CURRENT_VERSION {
        next.version = CURRENT_VERSION;
        invalid(&mut errors, "version");
    }

    next.picker.monitor = next.picker.monitor.trim().to_string();
    if bad_name(&next.picker.monitor, 256) {
        next.picker.monitor = prev.picker.monitor.clone();
        invalid(&mut errors, "picker.monitor");
    }

    for pk in &mut next.picker.paste_keys {
        pk.exe = pk.exe.trim().to_string();
    }
    let keys_ok = next.picker.paste_keys.len() <= 500
        && next.picker.paste_keys.iter().all(|k| !bad_name(&k.exe, MAX_PATH_CHARS))
        && !has_case_insensitive_dupes(next.picker.paste_keys.iter().map(|k| k.exe.as_str()));
    if !keys_ok {
        next.picker.paste_keys = prev.picker.paste_keys.clone();
        invalid(&mut errors, "picker.pasteKeys");
    }

    for (field, value, old) in [
        ("hotkeys.summon", &mut next.hotkeys.summon, &prev.hotkeys.summon),
        ("hotkeys.stackNext", &mut next.hotkeys.stack_next, &prev.hotkeys.stack_next),
        ("hotkeys.pauseToggle", &mut next.hotkeys.pause_toggle, &prev.hotkeys.pause_toggle),
        ("hotkeys.ignoreNext", &mut next.hotkeys.ignore_next, &prev.hotkeys.ignore_next),
    ] {
        *value = value.trim().to_string();
        // "" = not set. Parsing the chord itself is the hotkey engine's job (W1-D).
        if value.chars().count() > 64 || value.chars().any(char::is_control) {
            *value = old.clone();
            invalid(&mut errors, field);
        }
    }

    if !(0..=10_000_000).contains(&next.storage.max_entries) {
        next.storage.max_entries = prev.storage.max_entries;
        invalid(&mut errors, "storage.maxEntries");
    }
    if !(0..=1_048_576).contains(&next.storage.max_size_mb) {
        next.storage.max_size_mb = prev.storage.max_size_mb;
        invalid(&mut errors, "storage.maxSizeMb");
    }

    for app in &mut next.privacy.ignored_apps {
        *app = app.trim().to_string();
    }
    let apps_ok = next.privacy.ignored_apps.len() <= 1000
        && next.privacy.ignored_apps.iter().all(|a| !bad_name(a, MAX_PATH_CHARS))
        && !has_case_insensitive_dupes(next.privacy.ignored_apps.iter().map(String::as_str));
    if !apps_ok {
        next.privacy.ignored_apps = prev.privacy.ignored_apps.clone();
        invalid(&mut errors, "privacy.ignoredApps");
    }

    next.ocr.language = next.ocr.language.trim().to_string();
    let lang = &next.ocr.language;
    let lang_ok = !lang.is_empty()
        && lang.len() <= 35
        && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !lang_ok {
        next.ocr.language = prev.ocr.language.clone();
        invalid(&mut errors, "ocr.language");
    }

    if let Some(b) = next.window.settings_bounds {
        let ok = (100..=100_000).contains(&b.w)
            && (100..=100_000).contains(&b.h)
            && b.x.abs() <= 100_000
            && b.y.abs() <= 100_000;
        if !ok {
            next.window.settings_bounds = prev.window.settings_bounds;
            invalid(&mut errors, "window.settingsBounds");
        }
    }

    (next, errors)
}

fn has_case_insensitive_dupes<'a>(items: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().any(|s| !seen.insert(s.to_lowercase()))
}

fn revert_hotkey(next: &mut HotkeySettings, prev: &HotkeySettings, field: &str) {
    match field {
        "hotkeys.summon" => next.summon = prev.summon.clone(),
        "hotkeys.stackNext" => next.stack_next = prev.stack_next.clone(),
        "hotkeys.pauseToggle" => next.pause_toggle = prev.pause_toggle.clone(),
        "hotkeys.ignoreNext" => next.ignore_next = prev.ignore_next.clone(),
        _ => {}
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Settings {
    get(&app)
}

/// Apply a full `Settings` object. Invalid / rejected fields keep their previous value and
/// are listed in `errors`; everything else is applied, saved atomically, broadcast
/// (`settings:changed` ⚑) and passed to subscribers. Never returns `Err` for field-level
/// problems — only for a broken state.
#[tauri::command]
pub async fn settings_set(app: AppHandle, settings: Settings) -> Result<SettingsResult, String> {
    let store = app.state::<SettingsStore>();
    let _guard = store.write.lock();
    let prev = store.current.read().clone();
    let (mut next, mut errors) = validate(&prev, settings);

    // Hotkeys (W1-D): always consult the engine — it rebinds changed chords and also reports a
    // chord that is still not active (taken at startup), after one retry. A rejected chord
    // keeps its previous binding.
    let hk_errors = crate::hotkeys::apply_settings(&app, &prev.hotkeys, &next.hotkeys);
    for e in &hk_errors {
        revert_hotkey(&mut next.hotkeys, &prev.hotkeys, &e.field);
    }
    errors.extend(hk_errors);

    // Launch on login — an explicit user action is applied in every build.
    if next.general.autostart != prev.general.autostart {
        if let Err(e) = autostart::set(&app, next.general.autostart) {
            logx::warn("settings", &format!("autostart: {e}"));
            next.general.autostart = prev.general.autostart;
            invalid(&mut errors, "general.autostart");
        }
    }

    if next != prev {
        // A failed write keeps the change for this session (logged) — the side effects
        // above already happened, so refusing now would leave a half-applied state.
        if let Err(e) = store.save(&next) {
            logx::warn("settings", &format!("save failed: {e}"));
        }
        *store.current.write() = next.clone();
        let subscribers: Vec<Subscriber> = store.subscribers.lock().clone();
        for s in subscribers {
            s(&prev, &next);
        }
        events::settings_changed(&app, &next);
        if next.general.theme != prev.general.theme {
            theme::apply(&app, next.general.theme);
            theme::emit_changed(&app, next.general.theme);
        }
    }

    Ok(SettingsResult { settings: next, errors })
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| format!("failed: {e}"))
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> Result<AppMeta, String> {
    let data = data_dir(&app)?;
    Ok(AppMeta {
        version: app.package_info().version.to_string(),
        data_dir: data.to_string_lossy().into_owned(),
        log_dir: data.join("logs").to_string_lossy().into_owned(),
    })
}

/// Open the data or log folder in Explorer (created first if missing).
#[tauri::command]
pub async fn open_path(app: AppHandle, which: PathKind) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let data = data_dir(&app)?;
    let dir = match which {
        PathKind::Data => data,
        PathKind::Logs => data.join("logs"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed: {e}"))?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| format!("failed: {e}"))
}

/// Native "choose an .exe" dialog (ignored apps, paste chords). `null` = cancelled.
#[tauri::command]
pub async fn pick_exe(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let lang = i18n_native::resolve(get(&app).general.language);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app
        .dialog()
        .file()
        .set_title(i18n_native::tr(lang, i18n_native::Key::PickExeTitle))
        .add_filter(i18n_native::tr(lang, i18n_native::Key::PickExeFilter), &["exe"]);
    if let Some(win) = app.get_webview_window(events::SETTINGS) {
        dialog = dialog.set_parent(&win);
    }
    dialog.pick_file(move |picked| {
        let _ = tx.send(picked);
    });
    match rx.await.map_err(|_| "failed: dialog closed".to_string())? {
        None => Ok(None),
        Some(file) => file
            .into_path()
            .map(|p| Some(p.to_string_lossy().into_owned()))
            .map_err(|e| format!("invalid: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PasteChord, PasteKey};

    /// The cross-language defaults fixture: the browser mock is tested against the same
    /// file (`tests/settingsDefaults.test.mjs`), so TS and Rust defaults cannot drift.
    #[test]
    fn defaults_match_fixture() {
        let fixture = include_str!("../../tests/fixtures/settings.default.json");
        let expected: Value = serde_json::from_str(fixture).unwrap();
        assert_eq!(serde_json::to_value(Settings::default()).unwrap(), expected);
    }

    #[test]
    fn lenient_load_keeps_valid_fields() {
        let raw = br#"{
            "general": { "language": "ru", "theme": "neon", "autostart": false },
            "picker": { "position": "left", "monitor": "", "pasteKeys": "nope" },
            "storage": { "maxEntries": -5, "retention": "1y" },
            "unknownSection": { "x": 1 }
        }"#;
        let (s, clean) = parse_lenient(raw);
        assert!(!clean);
        assert_eq!(s.general.language, Language::Ru);
        assert_eq!(s.general.theme, ThemePref::System); // invalid enum → default
        assert!(!s.general.autostart);
        assert_eq!(s.picker.position, Position::Left);
        assert_eq!(s.picker.monitor, "cursor"); // empty → default
        assert!(s.picker.paste_keys.is_empty());
        assert_eq!(s.storage.max_entries, 5000); // negative → default
        assert_eq!(s.storage.retention, Retention::Year1);
        assert_eq!(s.version, CURRENT_VERSION);
    }

    #[test]
    fn parse_file_is_the_lenient_load() {
        let raw = br#"{ "picker": { "hints": "yes", "favoritesFirst": true }, "storage": { "maxEntries": 20000 } }"#;
        let s = parse_file(raw);
        assert!(s.picker.hints, "a broken field keeps its default");
        assert!(s.picker.favorites_first);
        assert_eq!(s.storage.max_entries, 20_000);
        assert_eq!(s, parse_lenient(raw).0);
    }

    #[test]
    fn garbage_file_falls_back_to_defaults() {
        assert_eq!(parse_lenient(b"not json").0, Settings::default());
        assert_eq!(parse_lenient(b"[1,2]").0, Settings::default());
        let (s, clean) = parse_lenient(serde_json::to_vec(&Settings::default()).unwrap().as_slice());
        assert!(clean);
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn validate_reverts_only_bad_fields() {
        let prev = Settings::default();
        let mut next = prev.clone();
        next.picker.center_row = true;
        next.storage.max_size_mb = -1;
        next.privacy.ignored_apps = vec!["KeePass.exe".into(), "keepass.exe".into()];
        next.picker.paste_keys = vec![PasteKey { exe: "  wt.exe ".into(), chord: PasteChord::CtrlShiftV }];
        next.ocr.language = "en-US".into();
        let (out, errors) = validate(&prev, next);
        assert!(out.picker.center_row);
        assert_eq!(out.storage.max_size_mb, 1024);
        assert_eq!(out.privacy.ignored_apps, prev.privacy.ignored_apps);
        assert_eq!(out.picker.paste_keys[0].exe, "wt.exe");
        assert_eq!(out.ocr.language, "en-US");
        let fields: Vec<_> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, vec!["storage.maxSizeMb", "privacy.ignoredApps"]);
        assert!(errors.iter().all(|e| e.code == SettingsErrorCode::Invalid));
    }

    /// W3: the save is atomic and durable (no temp file left), and a file that did not fully
    /// parse is kept as `settings.json.invalid` before the next change rewrites it.
    #[test]
    fn save_is_durable_and_a_broken_file_is_kept() {
        let dir = std::env::temp_dir().join(format!("liqlip-settings-test-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::load(&dir);
        assert_eq!(*store.current.read(), Settings::default(), "no file → defaults");
        assert!(!dir.join(INVALID_COPY).exists());
        let mut s = Settings::default();
        s.privacy.ignored_apps.push("vault.exe".into());
        store.save(&s).unwrap();
        assert!(!dir.join("settings.json.tmp").exists(), "no temp file left");
        assert_eq!(SettingsStore::load(&dir).current.read().privacy.ignored_apps, s.privacy.ignored_apps);
        assert!(!dir.join(INVALID_COPY).exists(), "a clean file is not copied");
        std::fs::write(dir.join(FILE), br#"{"privacy": {"ignoredApps": "oops"}, "picker": {"hints": false}}"#).unwrap();
        let loaded = SettingsStore::load(&dir);
        assert!(!loaded.current.read().picker.hints, "valid fields are kept");
        assert_eq!(std::fs::read(dir.join(INVALID_COPY)).unwrap(), std::fs::read(dir.join(FILE)).unwrap());
        assert!(read_retrying(&dir.join("missing.json")).unwrap_err().kind() == std::io::ErrorKind::NotFound);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revert_hotkey_by_field() {
        let prev = Settings::default().hotkeys;
        let mut next = prev.clone();
        next.summon = "Ctrl + Space".into();
        next.stack_next = "Ctrl + B".into();
        revert_hotkey(&mut next, &prev, "hotkeys.summon");
        assert_eq!(next.summon, "Alt + V");
        assert_eq!(next.stack_next, "Ctrl + B");
    }
}
