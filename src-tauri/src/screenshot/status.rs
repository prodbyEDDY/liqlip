//! `PrtSc` vs the Snipping Tool, hotkey errors, Windows settings (owner: W6-B; ARCHITECTURE
//! §18.11). W5-A stub.
//!
//! To do (W6-B): read `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled` (DWORD;
//! missing → by Windows version, checked against Microsoft's docs; never written), the
//! registration errors of the five `hotkeys.shot*` slots (from the hotkey engine, W6-A), and
//! open `ms-settings:easeofaccess-keyboard`.

use crate::model::{ShotStatus, WindowsSettingsPage};
use tauri::AppHandle;

/// `shot_status`.
pub(crate) fn status(_app: &AppHandle) -> Result<ShotStatus, String> {
    Err("unimplemented: shot_status".into())
}

/// `open_windows_settings { page }`.
pub(crate) fn open_windows_settings(_app: &AppHandle, _page: WindowsSettingsPage) -> Result<(), String> {
    Err("unimplemented: open_windows_settings".into())
}
