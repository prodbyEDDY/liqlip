//! Global hotkey engine — `RegisterHotKey` first, `WH_KEYBOARD_LL` only when a chord
//! needs it; chord recording (ARCHITECTURE §8). Owner: **W1-D** (port of Liquid Tools
//! `hotkeys.rs` without dictation / PrintScreen / Esc / `Feature`). W0 declares the API.
//! Invariant: the LL-hook callback does no work (no IPC, windows or I/O).

pub mod commands;

use crate::model::{HotkeySettings, SettingsError};
use tauri::AppHandle;

/// What a global hotkey does.
#[allow(dead_code)] // W1-D
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    /// `picker::summon`
    Summon,
    /// `paste::stack::paste_next` (on a worker thread)
    StackNext,
    /// `capture::toggle_pause`
    PauseToggle,
    /// `capture::ignore_next`
    IgnoreNext,
}

/// `setup()` step 7: start the registrar thread and bind the saved chords. Conflicts are
/// logged (the UI learns about them on the next `settings_set`).
pub fn start(app: &AppHandle, hotkeys: &HotkeySettings) -> Vec<SettingsError> {
    let _ = (app, hotkeys); // W1-D
    Vec::new()
}

/// Synchronous hook from `settings_set` when `hotkeys` changed: (re)bind every changed
/// action. Return one error per rejected field (`field: "hotkeys.summon"`, code `taken`
/// when RegisterHotKey refuses, `invalid` for an unparsable or duplicated chord); the
/// engine must keep the PREVIOUS binding for those — `settings_set` reverts the fields.
pub fn apply_settings(
    app: &AppHandle,
    prev: &HotkeySettings,
    next: &HotkeySettings,
) -> Vec<SettingsError> {
    let _ = (app, prev, next); // W1-D
    Vec::new()
}
