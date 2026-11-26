//! `PrtSc` vs the Snipping Tool, screenshot hotkey errors, Windows settings (owner: W6-B;
//! ARCHITECTURE §18.11; SPEC §9.1).
//!
//! **Snipping key.** `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled` (DWORD,
//! 1 = Windows opens the Snipping Tool on `PrtSc`, the toggle in Settings → Accessibility →
//! Keyboard). It is only read, never written (SPEC §9.1). When the value is missing the user
//! never touched the toggle and the Windows default applies. Microsoft, KB5027303 (June 27, 2023,
//! OS Build 22621.1928, Windows 11 22H2): "This update changes the default print screen (prt scr)
//! key behavior. Pressing the print screen key opens the Snipping Tool by default. … If you have
//! previously changed this setting, Windows will preserve your preference."
//! (https://support.microsoft.com/help/5027303). So a missing value means **on** for Windows 11
//! build 22621 with UBR ≥ 1928 and every later build (22631 23H2, 26100 24H2, …), and **off**
//! for older Windows 11 (22000) and Windows 10 (no Microsoft note changes its default). The build
//! and UBR come from `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` (`CurrentBuildNumber`,
//! `UBR`) — not version APIs, which depend on the manifest and do not give the UBR.
//!
//! **Hotkey errors** of the five `hotkeys.shot*` slots: `hotkeys::current_errors` (W6-A, made for
//! this — the unparsable / duplicate fields of the saved settings and every wanted chord the OS
//! has not given us, after one retry), filtered to those fields. Read-only: unlike
//! `apply_settings(prev = next)` it does not re-sync the keyboard hook from a status query.

use crate::hotkeys;
use crate::model::{SettingsError, ShotStatus, WindowsSettingsPage};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// `shot_status` (an `async` command: `current_errors` waits for the hotkey registrar, ≤ 2 s).
pub(crate) fn status(app: &AppHandle) -> Result<ShotStatus, String> {
    let hotkey_errors = shot_errors(hotkeys::current_errors(app));
    Ok(ShotStatus { snipping_key: snipping_key(), hotkey_errors })
}

/// `open_windows_settings { page }` — the `ms-settings:` page through the shell (the opener
/// plugin's Rust API = `ShellExecuteExW`, not subject to the JS scope). URIs: learn.microsoft.com
/// "Launch Windows Settings", Ease of access → Keyboard = `ms-settings:easeofaccess-keyboard`.
pub(crate) fn open_windows_settings(app: &AppHandle, page: WindowsSettingsPage) -> Result<(), String> {
    app.opener().open_url(settings_uri(page), None::<&str>).map_err(|e| format!("failed: {e}"))
}

fn settings_uri(page: WindowsSettingsPage) -> &'static str {
    match page {
        WindowsSettingsPage::Keyboard => "ms-settings:easeofaccess-keyboard",
    }
}

fn shot_errors(all: Vec<SettingsError>) -> Vec<SettingsError> {
    all.into_iter().filter(|e| e.field.starts_with("hotkeys.shot")).collect()
}

/// The Windows default of the snipping key when the user never set it (see the module docs).
fn snipping_default(build: u32, ubr: u32) -> bool {
    build > 22621 || (build == 22621 && ubr >= 1928)
}

#[cfg(windows)]
fn snipping_key() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::HKEY_CURRENT_USER;
    match reg::dword(HKEY_CURRENT_USER, w!(r"Control Panel\Keyboard"), w!("PrintScreenKeyForSnippingEnabled")) {
        Some(v) => v != 0,
        None => reg::windows_build().is_some_and(|(build, ubr)| snipping_default(build, ubr)),
    }
}

#[cfg(not(windows))]
fn snipping_key() -> bool {
    false
}

#[cfg(windows)]
mod reg {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};

    /// A DWORD value (`RRF_RT_REG_DWORD`: anything else fails), `None` when absent.
    /// https://learn.microsoft.com/windows/win32/api/winreg/nf-winreg-reggetvaluew
    pub fn dword(key: HKEY, subkey: PCWSTR, value: PCWSTR) -> Option<u32> {
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: constant key / value names; `data` / `size` outlive the call and `size` is the
        // buffer's byte length.
        let status = unsafe {
            RegGetValueW(key, subkey, value, RRF_RT_REG_DWORD, None, Some(&mut data as *mut u32 as *mut core::ffi::c_void), Some(&mut size))
        };
        (status == ERROR_SUCCESS).then_some(data)
    }

    /// A short REG_SZ value (`RegGetValueW` with `RRF_RT_REG_SZ` NUL-terminates it).
    fn string(key: HKEY, subkey: PCWSTR, value: PCWSTR) -> Option<String> {
        let mut buf = [0u16; 64];
        let mut size = std::mem::size_of_val(&buf) as u32;
        // SAFETY: as above; `size` is the buffer's byte length.
        let status = unsafe {
            RegGetValueW(key, subkey, value, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut core::ffi::c_void), Some(&mut size))
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }

    /// `(CurrentBuildNumber, UBR)` of the running Windows (`UBR` 0 when absent).
    pub fn windows_build() -> Option<(u32, u32)> {
        let key = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
        let build = string(HKEY_LOCAL_MACHINE, key, w!("CurrentBuildNumber"))?.trim().parse().ok()?;
        Some((build, dword(HKEY_LOCAL_MACHINE, key, w!("UBR")).unwrap_or(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SettingsErrorCode;

    #[test]
    fn the_snipping_key_default_follows_kb5027303() {
        assert!(!snipping_default(19045, 6456), "Windows 10 22H2");
        assert!(!snipping_default(22000, 3260), "Windows 11 21H2");
        assert!(!snipping_default(22621, 1848), "22H2 before the June 2023 update");
        assert!(snipping_default(22621, 1928), "KB5027303");
        assert!(snipping_default(22631, 0), "23H2");
        assert!(snipping_default(26100, 1), "24H2");
    }

    #[test]
    fn only_the_screenshot_slots_are_reported() {
        let e = |f: &str| SettingsError { field: f.into(), code: SettingsErrorCode::Taken };
        let out = shot_errors(vec![e("hotkeys.summon"), e("hotkeys.shotRegion"), e("hotkeys.stackNext"), e("hotkeys.shotText")]);
        let fields: Vec<&str> = out.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, ["hotkeys.shotRegion", "hotkeys.shotText"]);
        assert_eq!(settings_uri(WindowsSettingsPage::Keyboard), "ms-settings:easeofaccess-keyboard");
    }

    /// Reads the real registry of the machine running the tests (read-only).
    #[cfg(windows)]
    #[test]
    fn the_registry_reads_do_not_fail() {
        let (build, _) = reg::windows_build().expect("CurrentBuildNumber");
        assert!(build >= 10240, "Windows 10 or later: {build}");
        let _ = snipping_key();
    }
}
