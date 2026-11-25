//! Native (non-webview) strings: tray menu, tray tooltip, settings window title, native
//! dialogs. EN + RU, chosen by `Settings.general.language` (`system` → the Windows UI
//! language). Coordinator module; W1 agents add keys here through their report.

use crate::model::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ru,
}

/// Resolve the setting to a concrete language.
pub fn resolve(language: Language) -> Lang {
    match language {
        Language::En => Lang::En,
        Language::Ru => Lang::Ru,
        Language::System => system_lang(),
    }
}

/// Windows UI language. `GetUserDefaultUILanguage` returns a LANGID; its primary
/// language is the low 10 bits, LANG_RUSSIAN = 0x19.
/// https://learn.microsoft.com/windows/win32/api/winnls/nf-winnls-getuserdefaultuilanguage
/// https://learn.microsoft.com/windows/win32/intl/language-identifier-constants-and-strings
#[cfg(windows)]
fn system_lang() -> Lang {
    // SAFETY: no arguments, no out-pointers; always succeeds.
    let langid = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    if langid & 0x3ff == 0x19 {
        Lang::Ru
    } else {
        Lang::En
    }
}

#[cfg(not(windows))]
fn system_lang() -> Lang {
    Lang::En
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    TrayTooltip,
    TrayTooltipPaused,
    TrayOpen,
    TrayPause,
    TrayPause5m,
    TrayPause30m,
    TrayPause1h,
    TrayPauseUntilResume,
    TrayResume,
    TrayIgnoreNext,
    TrayDeleteRecent,
    TrayDelete5m,
    TrayDelete15m,
    TrayDelete30m,
    TrayDelete1h,
    TrayDelete24h,
    /// `{n}` = number of items
    TrayStack,
    TraySettings,
    TrayQuit,
    SettingsWindowTitle,
    /// `{period}` = one of the TrayDelete* labels, lower-cased by the caller if needed
    ConfirmDeleteRecentTitle,
    ConfirmDeleteRecentBody,
    ConfirmDelete,
    Cancel,
    PickExeTitle,
    PickExeFilter,
}

pub fn tr(lang: Lang, key: Key) -> &'static str {
    use Key::*;
    match (lang, key) {
        (Lang::En, TrayTooltip) => "Liqlip",
        (Lang::Ru, TrayTooltip) => "Liqlip",
        (Lang::En, TrayTooltipPaused) => "Liqlip — recording paused",
        (Lang::Ru, TrayTooltipPaused) => "Liqlip — запись на паузе",
        (Lang::En, TrayOpen) => "Open Liqlip",
        (Lang::Ru, TrayOpen) => "Открыть Liqlip",
        (Lang::En, TrayPause) => "Pause recording",
        (Lang::Ru, TrayPause) => "Пауза записи",
        (Lang::En, TrayPause5m) => "For 5 minutes",
        (Lang::Ru, TrayPause5m) => "На 5 минут",
        (Lang::En, TrayPause30m) => "For 30 minutes",
        (Lang::Ru, TrayPause30m) => "На 30 минут",
        (Lang::En, TrayPause1h) => "For 1 hour",
        (Lang::Ru, TrayPause1h) => "На 1 час",
        (Lang::En, TrayPauseUntilResume) => "Until I resume",
        (Lang::Ru, TrayPauseUntilResume) => "До возобновления",
        (Lang::En, TrayResume) => "Resume recording",
        (Lang::Ru, TrayResume) => "Продолжить запись",
        (Lang::En, TrayIgnoreNext) => "Skip next copy",
        (Lang::Ru, TrayIgnoreNext) => "Пропустить следующее копирование",
        (Lang::En, TrayDeleteRecent) => "Delete recent",
        (Lang::Ru, TrayDeleteRecent) => "Удалить недавнее",
        (Lang::En, TrayDelete5m) => "Last 5 minutes",
        (Lang::Ru, TrayDelete5m) => "За последние 5 минут",
        (Lang::En, TrayDelete15m) => "Last 15 minutes",
        (Lang::Ru, TrayDelete15m) => "За последние 15 минут",
        (Lang::En, TrayDelete30m) => "Last 30 minutes",
        (Lang::Ru, TrayDelete30m) => "За последние 30 минут",
        (Lang::En, TrayDelete1h) => "Last hour",
        (Lang::Ru, TrayDelete1h) => "За последний час",
        (Lang::En, TrayDelete24h) => "Last 24 hours",
        (Lang::Ru, TrayDelete24h) => "За последние 24 часа",
        (Lang::En, TrayStack) => "Paste stack: {n}",
        (Lang::Ru, TrayStack) => "Очередь: {n}",
        (Lang::En, TraySettings) => "Settings…",
        (Lang::Ru, TraySettings) => "Настройки…",
        (Lang::En, TrayQuit) => "Quit",
        (Lang::Ru, TrayQuit) => "Выход",
        (Lang::En, SettingsWindowTitle) => "Liqlip — Settings",
        (Lang::Ru, SettingsWindowTitle) => "Liqlip — Настройки",
        (Lang::En, ConfirmDeleteRecentTitle) => "Delete recent history?",
        (Lang::Ru, ConfirmDeleteRecentTitle) => "Удалить недавнюю историю?",
        (Lang::En, ConfirmDeleteRecentBody) => {
            "Everything copied in this period will be deleted permanently, including favorites and items in folders. This cannot be undone."
        }
        (Lang::Ru, ConfirmDeleteRecentBody) => {
            "Всё, что скопировано за этот период, будет удалено навсегда — включая избранное и записи в папках. Отменить это нельзя."
        }
        (Lang::En, ConfirmDelete) => "Delete",
        (Lang::Ru, ConfirmDelete) => "Удалить",
        (Lang::En, Cancel) => "Cancel",
        (Lang::Ru, Cancel) => "Отмена",
        (Lang::En, PickExeTitle) => "Choose an application",
        (Lang::Ru, PickExeTitle) => "Выберите приложение",
        (Lang::En, PickExeFilter) => "Applications",
        (Lang::Ru, PickExeFilter) => "Приложения",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_both_languages() {
        // `tr` is an exhaustive match, so this mainly guards against empty strings.
        use Key::*;
        let keys = [
            TrayTooltip, TrayTooltipPaused, TrayOpen, TrayPause, TrayPause5m, TrayPause30m,
            TrayPause1h, TrayPauseUntilResume, TrayResume, TrayIgnoreNext, TrayDeleteRecent,
            TrayDelete5m, TrayDelete15m, TrayDelete30m, TrayDelete1h, TrayDelete24h, TrayStack,
            TraySettings, TrayQuit, SettingsWindowTitle, ConfirmDeleteRecentTitle,
            ConfirmDeleteRecentBody, ConfirmDelete, Cancel, PickExeTitle, PickExeFilter,
        ];
        for k in keys {
            assert!(!tr(Lang::En, k).is_empty());
            assert!(!tr(Lang::Ru, k).is_empty());
        }
        assert!(tr(Lang::Ru, TrayStack).contains("{n}"));
    }
}
