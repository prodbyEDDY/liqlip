//! Typed emitters for every §9.7 event — the ONE place event names and addressing
//! live (coordinator module, W0). Modules never call `app.emit*` with a string literal;
//! they call these. Emission is best-effort: a failed emit is logged, never fatal.
//!
//! Addressing (§9.7): `emit_to` a window label, or a label filter for several windows;
//! ⚑ events go to every window. JS listeners registered with the global `listen()`
//! (target `Any`) receive targeted events too (tauri `event/listener.rs:
//! match_any_or_filter`), so the frontend just uses `listen`.

// Every §9.7 event has an emitter here; most are called by W1 modules.
#![allow(dead_code)] // W1-B, W1-C, W1-D, W2

use crate::model::{
    CaptureStatus, ClipsChanged, HotkeysCaptured, HudShow, ImportProgress, OcrProgress,
    PickerClose, PickerLayout, PickerOpen, ResolvedTheme, Settings, SettingsNavigate, ShotAdded,
    ShotEditorPayload, ShotEnd, ShotSession, Stack,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget};

/// Window labels (created hidden in `lib.rs::setup`).
pub const PICKER: &str = "picker";
pub const SETTINGS: &str = "settings";
pub const HUD: &str = "hud";
/// Capture overlay (W5): pre-created hidden in `setup()` like the three above.
pub const CAPTURE: &str = "capture";
/// Screenshot windows built on demand (W6-B, ARCHITECTURE §18.2): the QAO stack, the editor,
/// and the pinned pictures `pin-<n>` (labels share this prefix; capabilities match `pin-*`).
pub const SHOTS: &str = "shots";
pub const EDITOR: &str = "editor";
pub const PIN_PREFIX: &str = "pin-";

pub const CLIPS_CHANGED: &str = "clips:changed";
pub const PICKER_OPEN: &str = "picker:open";
pub const PICKER_CLOSE: &str = "picker:close";
pub const PICKER_LAYOUT: &str = "picker:layout";
pub const CAPTURE_STATUS: &str = "capture:status";
pub const STACK_CHANGED: &str = "stack:changed";
pub const SETTINGS_CHANGED: &str = "settings:changed";
pub const SETTINGS_NAVIGATE: &str = "settings:navigate";
pub const HOTKEYS_CAPTURED: &str = "hotkeys:captured";
pub const HUD_SHOW: &str = "hud:show";
pub const OCR_PROGRESS: &str = "ocr:progress";
pub const IMPORT_PROGRESS: &str = "import:progress";
pub const THEME_CHANGED: &str = "theme://changed";
pub const SHOT_SESSION: &str = "shot:session";
pub const SHOT_END: &str = "shot:end";
pub const SHOT_ADDED: &str = "shot:added";
pub const SHOT_EDITOR: &str = "shot:editor";

fn log_err(event: &str, r: tauri::Result<()>) {
    if let Err(e) = r {
        crate::logx::warn("events", &format!("emit {event}: {e}"));
    }
}

fn to(app: &AppHandle, label: &str, event: &str, payload: impl Serialize + Clone) {
    log_err(event, app.emit_to(label, event, payload));
}

fn to_many(app: &AppHandle, labels: &'static [&'static str], event: &str, payload: impl Serialize + Clone) {
    log_err(
        event,
        app.emit_filter(event, payload, |t| match t {
            EventTarget::AnyLabel { label }
            | EventTarget::Window { label }
            | EventTarget::Webview { label }
            | EventTarget::WebviewWindow { label } => labels.contains(&label.as_str()),
            _ => false,
        }),
    );
}

fn all(app: &AppHandle, event: &str, payload: impl Serialize + Clone) {
    log_err(event, app.emit(event, payload));
}

/// store/capture → picker, settings, shots (QAO drops a deleted shot), editor (W5, §18)
pub fn clips_changed(app: &AppHandle, payload: &ClipsChanged) {
    to_many(app, &[PICKER, SETTINGS, SHOTS, EDITOR], CLIPS_CHANGED, payload);
}

/// picker → picker
pub fn picker_open(app: &AppHandle, payload: &PickerOpen) {
    to(app, PICKER, PICKER_OPEN, payload);
}

/// picker → picker (free rows after 260 ms)
pub fn picker_close(app: &AppHandle) {
    to(app, PICKER, PICKER_CLOSE, PickerClose {});
}

/// picker → picker
pub fn picker_layout(app: &AppHandle, payload: &PickerLayout) {
    to(app, PICKER, PICKER_LAYOUT, payload);
}

/// capture → picker, settings
pub fn capture_status(app: &AppHandle, payload: &CaptureStatus) {
    to_many(app, &[PICKER, SETTINGS], CAPTURE_STATUS, payload);
}

/// paste → picker
pub fn stack_changed(app: &AppHandle, payload: &Stack) {
    to(app, PICKER, STACK_CHANGED, payload);
}

/// settings → all ⚑
pub fn settings_changed(app: &AppHandle, payload: &Settings) {
    all(app, SETTINGS_CHANGED, payload);
}

/// picker (`settings_open`) → settings
pub fn settings_navigate(app: &AppHandle, section: &str) {
    to(app, SETTINGS, SETTINGS_NAVIGATE, SettingsNavigate { section: section.to_string() });
}

/// hotkeys → settings. `combo == ""` means the recording was cancelled.
pub fn hotkeys_captured(app: &AppHandle, combo: &str) {
    to(app, SETTINGS, HOTKEYS_CAPTURED, HotkeysCaptured { combo: combo.to_string() });
}

/// picker/paste → hud
pub fn hud_show(app: &AppHandle, payload: &HudShow) {
    to(app, HUD, HUD_SHOW, payload);
}

/// ocr → settings
pub fn ocr_progress(app: &AppHandle, payload: &OcrProgress) {
    to(app, SETTINGS, OCR_PROGRESS, payload);
}

/// import → settings
pub fn import_progress(app: &AppHandle, payload: &ImportProgress) {
    to(app, SETTINGS, IMPORT_PROGRESS, payload);
}

/// theme → all ⚑
pub fn theme_changed(app: &AppHandle, theme: ResolvedTheme) {
    all(app, THEME_CHANGED, theme);
}

// ── Screenshots (W5, ARCHITECTURE §18.10) ───────────────────────────────────

/// screenshot → capture: a session started, or moved to another monitor.
pub fn shot_session(app: &AppHandle, payload: &ShotSession) {
    to(app, CAPTURE, SHOT_SESSION, payload);
}

/// screenshot → capture, shots: the session ended (committed or cancelled).
pub fn shot_end(app: &AppHandle, session_id: i64) {
    to_many(app, &[CAPTURE, SHOTS], SHOT_END, ShotEnd { session_id });
}

/// screenshot → shots: a shot was taken (the QAO adds a card).
pub fn shot_added(app: &AppHandle, payload: &ShotAdded) {
    to(app, SHOTS, SHOT_ADDED, payload);
}

/// screenshot → editor: show another record in the open editor window.
pub fn shot_editor(app: &AppHandle, payload: &ShotEditorPayload) {
    to(app, EDITOR, SHOT_EDITOR, payload);
}
