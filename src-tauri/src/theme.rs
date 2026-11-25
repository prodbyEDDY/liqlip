//! App theme (ported from Liquid Tools `theme.rs`, re-pointed at `Settings.general.theme`
//! instead of a separate `theme.json`). The visual switch is CSS-driven on the frontend
//! (`<html data-theme>` + `:root[data-theme="dark"]` token overrides); here we only keep
//! the native bits (title bar, scrollbars, WebView `prefers-color-scheme`) consistent and
//! broadcast `theme://changed` with the RESOLVED theme (§9.7: `'light' | 'dark'`).

use crate::events;
use crate::model::{ResolvedTheme, ThemePref};
use tauri::{AppHandle, Manager, Theme};

/// light/dark force the native theme; system = `None` (follow the OS), which keeps the
/// webview's `prefers-color-scheme` tracking Windows so the frontend can resolve it too.
fn pref_to_theme(pref: ThemePref) -> Option<Theme> {
    match pref {
        ThemePref::Light => Some(Theme::Light),
        ThemePref::Dark => Some(Theme::Dark),
        ThemePref::System => None,
    }
}

/// Push the preference to every pre-created window's native chrome.
pub fn apply(app: &AppHandle, pref: ThemePref) {
    let theme = pref_to_theme(pref);
    for (_, win) in app.webview_windows() {
        let _ = win.set_theme(theme);
    }
}

/// The theme actually in effect. For `system`, a window's `theme()` reports what Windows
/// uses for apps (tao reads `AppsUseLightTheme`), since no override is set.
pub fn resolved(app: &AppHandle, pref: ThemePref) -> ResolvedTheme {
    let theme = match pref_to_theme(pref) {
        Some(t) => t,
        None => app
            .webview_windows()
            .values()
            .next()
            .and_then(|w| w.theme().ok())
            .unwrap_or(Theme::Light),
    };
    match theme {
        Theme::Dark => ResolvedTheme::Dark,
        _ => ResolvedTheme::Light,
    }
}

/// Broadcast the resolved theme to every window (`theme://changed` ⚑).
pub fn emit_changed(app: &AppHandle, pref: ThemePref) {
    events::theme_changed(app, resolved(app, pref));
}
