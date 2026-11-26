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
    /// `{time}` = local time a timed pause ends ("14:30" / "2:30 PM")
    TrayTooltipPausedUntil,
    /// Tooltip part while "skip the next copy" is armed (FIX-05)
    TrayTooltipSkipNext,
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
    BackupExportTitle,
    BackupRestoreTitle,
    BackupFilter,
    /// Tray, screenshots (W6-A, SPEC §9.8)
    TrayShotRegion,
    TrayShotScreen,
    TrayShotWindow,
    TrayShotRepeat,
    TrayShotDelay,
    TrayShotDelay3s,
    TrayShotDelay5s,
    TrayShotDelay10s,
    TrayShotText,
    /// "Save as…" dialog of a screenshot (W6-A, §18.4)
    ShotSaveAsTitle,
    ShotFilterPng,
    ShotFilterJpg,
    /// Editor window: the name of a record without a title or a saved file (INT-5, §18.9)
    EditorUntitledShot,
    EditorUntitledImage,
    /// "Choose a folder" dialog of the screenshot folder (INT-5, `pick_folder`)
    PickFolderTitle,
}

pub fn tr(lang: Lang, key: Key) -> &'static str {
    use Key::*;
    match (lang, key) {
        (Lang::En, TrayTooltip) => "Liqlip — recording",
        (Lang::Ru, TrayTooltip) => "Liqlip — запись идёт",
        (Lang::En, TrayTooltipPausedUntil) => "Liqlip — paused until {time}",
        (Lang::Ru, TrayTooltipPausedUntil) => "Liqlip — пауза до {time}",
        (Lang::En, TrayTooltipPaused) => "Liqlip — recording paused",
        (Lang::Ru, TrayTooltipPaused) => "Liqlip — запись на паузе",
        (Lang::En, TrayTooltipSkipNext) => "The next copy won't be saved",
        (Lang::Ru, TrayTooltipSkipNext) => "Следующее копирование не сохранится",
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
        (Lang::En, BackupExportTitle) => "Export a Liqlip backup",
        (Lang::Ru, BackupExportTitle) => "Экспорт резервной копии Liqlip",
        (Lang::En, BackupRestoreTitle) => "Restore from a Liqlip backup",
        (Lang::Ru, BackupRestoreTitle) => "Восстановление из резервной копии Liqlip",
        (Lang::En, BackupFilter) => "Liqlip backup",
        (Lang::Ru, BackupFilter) => "Резервная копия Liqlip",
        (Lang::En, TrayShotRegion) => "Capture region",
        (Lang::Ru, TrayShotRegion) => "Снимок области",
        (Lang::En, TrayShotScreen) => "Capture screen",
        (Lang::Ru, TrayShotScreen) => "Снимок экрана",
        (Lang::En, TrayShotWindow) => "Capture active window",
        (Lang::Ru, TrayShotWindow) => "Снимок активного окна",
        (Lang::En, TrayShotRepeat) => "Capture previous region",
        (Lang::Ru, TrayShotRepeat) => "Повторить прошлую область",
        (Lang::En, TrayShotDelay) => "Capture with delay",
        (Lang::Ru, TrayShotDelay) => "Снимок с задержкой",
        (Lang::En, TrayShotDelay3s) => "3 seconds",
        (Lang::Ru, TrayShotDelay3s) => "3 секунды",
        (Lang::En, TrayShotDelay5s) => "5 seconds",
        (Lang::Ru, TrayShotDelay5s) => "5 секунд",
        (Lang::En, TrayShotDelay10s) => "10 seconds",
        (Lang::Ru, TrayShotDelay10s) => "10 секунд",
        (Lang::En, TrayShotText) => "Copy text from screen",
        (Lang::Ru, TrayShotText) => "Скопировать текст с экрана",
        (Lang::En, ShotSaveAsTitle) => "Save screenshot as",
        (Lang::Ru, ShotSaveAsTitle) => "Сохранить снимок как",
        (Lang::En, ShotFilterPng) => "PNG image",
        (Lang::Ru, ShotFilterPng) => "Изображение PNG",
        (Lang::En, ShotFilterJpg) => "JPEG image",
        (Lang::Ru, ShotFilterJpg) => "Изображение JPEG",
        (Lang::En, EditorUntitledShot) => "Screenshot",
        (Lang::Ru, EditorUntitledShot) => "Снимок экрана",
        (Lang::En, EditorUntitledImage) => "Image",
        (Lang::Ru, EditorUntitledImage) => "Картинка",
        (Lang::En, PickFolderTitle) => "Choose a folder for screenshots",
        (Lang::Ru, PickFolderTitle) => "Папка для снимков",
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
            TrayTooltip, TrayTooltipPaused, TrayTooltipPausedUntil, TrayTooltipSkipNext, TrayOpen, TrayPause, TrayPause5m, TrayPause30m,
            TrayPause1h, TrayPauseUntilResume, TrayResume, TrayIgnoreNext, TrayDeleteRecent,
            TrayDelete5m, TrayDelete15m, TrayDelete30m, TrayDelete1h, TrayDelete24h, TrayStack,
            TraySettings, TrayQuit, SettingsWindowTitle, ConfirmDeleteRecentTitle,
            ConfirmDeleteRecentBody, ConfirmDelete, Cancel, PickExeTitle, PickExeFilter,
            BackupExportTitle, BackupRestoreTitle, BackupFilter, TrayShotRegion, TrayShotScreen,
            TrayShotWindow, TrayShotRepeat, TrayShotDelay, TrayShotDelay3s, TrayShotDelay5s,
            TrayShotDelay10s, TrayShotText, ShotSaveAsTitle, ShotFilterPng, ShotFilterJpg,
            EditorUntitledShot, EditorUntitledImage, PickFolderTitle,
        ];
        for k in keys {
            assert!(!tr(Lang::En, k).is_empty());
            assert!(!tr(Lang::Ru, k).is_empty());
        }
        assert!(tr(Lang::Ru, TrayStack).contains("{n}"));
        assert!(tr(Lang::Ru, TrayTooltipPausedUntil).contains("{time}"));
        assert!(tr(Lang::En, TrayTooltipPausedUntil).contains("{time}"));
    }
}
