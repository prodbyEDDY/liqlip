//! `picker` commands (ARCHITECTURE §9.6) — owner: W1-D.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use super::machine::{Event, Phase};
use super::{dispatch, dispatch_wait, win, PICKER};
use crate::events;
use crate::model::{MonitorInfo, TargetInfo};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Close the shelf (Esc); `restoreFocus` returns focus to the window it was summoned from
/// (only while the shelf still holds focus). Idempotent.
#[tauri::command]
pub async fn picker_hide(app: AppHandle, restore_focus: bool) -> Result<(), String> {
    dispatch(&app, Event::Hide { restore: restore_focus });
    Ok(())
}

/// A passive shelf (hover-opened, or a summon whose activation Windows refused) → focusable
/// + foreground, so the keyboard works (the frontend calls it on a click into search). No
/// second `picker:open`. `Err("failed: …")` if Windows refuses the foreground.
#[tauri::command]
pub async fn picker_make_interactive(app: AppHandle) -> Result<(), String> {
    if !dispatch_wait(&app, Event::MakeInteractive, Duration::from_secs(1)) {
        return Err("busy: main thread did not answer".into());
    }
    let phase = PICKER.get().map(|p| p.state.lock().m.phase);
    match phase {
        Some(Phase::Open { interactive: true, .. }) => Ok(()),
        Some(Phase::Open { .. }) => Err("failed: foreground refused by Windows".into()),
        _ => Err("invalid: the shelf is not open".into()),
    }
}

/// The window the shelf will paste into (W1-C's remembered target, re-validated).
#[tauri::command]
pub async fn picker_target(app: AppHandle) -> Result<Option<TargetInfo>, String> {
    Ok(crate::paste::focus::current_target(&app))
}

/// Show + activate the settings window, then `settings:navigate { section }` if given.
#[tauri::command]
pub async fn settings_open(app: AppHandle, section: Option<String>) -> Result<(), String> {
    if let Some(s) = &section {
        if s.is_empty() || s.len() > 64 || s.chars().any(char::is_control) {
            return Err("invalid: section".into());
        }
    }
    super::show_settings(&app, section.as_deref());
    Ok(())
}

/// Every connected monitor for the "Monitor" setting: `id` = GDI device name (what
/// `settings.picker.monitor` stores), `name` = the monitor's EDID name when Windows knows
/// it (else the device name), physical resolution and scale.
#[tauri::command]
pub async fn monitors_list(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || list_monitors(&app))
        .await
        .map_err(|e| format!("failed: {e}"))?
}

fn list_monitors(app: &AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let w = app.get_webview_window(events::PICKER).ok_or("failed: picker window missing")?;
    let mons = w.available_monitors().map_err(|e| format!("failed: {e}"))?;
    let primary = w.primary_monitor().map_err(|e| format!("failed: {e}"))?;
    let names = win::monitor_friendly_names();
    let mut out: Vec<MonitorInfo> = mons
        .iter()
        .map(|m| {
            let id = m.name().cloned().unwrap_or_default();
            let name = names.get(&id).cloned().unwrap_or_else(|| id.trim_start_matches(r"\\.\").to_string());
            MonitorInfo {
                primary: primary.as_ref().is_some_and(|p| p.name() == m.name() && p.position() == m.position()),
                width: i64::from(m.size().width),
                height: i64::from(m.size().height),
                scale: m.scale_factor(),
                id,
                name,
            }
        })
        .collect();
    // Primary first, then left-to-right as arranged (stable for the dropdown).
    let pos = |id: &str| mons.iter().find(|m| m.name().is_some_and(|n| n == id)).map(|m| (m.position().x, m.position().y));
    out.sort_by_key(|m| (!m.primary, pos(&m.id)));
    Ok(out)
}
