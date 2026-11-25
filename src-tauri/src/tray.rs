//! Tray icon + menu (SPEC §5). Coordinator module (W0, polished in W2-D). Labels come
//! from `i18n_native`; actions call the module APIs of §15. The menu, icon and tooltip are
//! rebuilt by [`refresh`] — call it after anything they show changes (pause state, stack
//! size, language, summon hotkey, taskbar theme or scale); it is cheap and callable from any
//! thread.

use crate::i18n_native::{self, Key, Lang};
use crate::model::{ChangeOrigin, ClipsChange, ClipsChanged};
use crate::{capture, events, logx, paste, picker, settings, store};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "liqlip";

/// Tray glyphs from `scripts/brand.mjs` (ARCHITECTURE §16): black on transparent, hinted per
/// pixel size. Windows draws a notification-area icon at SM_CXSMICON for the taskbar's DPI —
/// 16 px at 100 %, 20 at 125 %, 24 at 150 %, 28 at 175 %, 32 at 200 %, 40 at 250 %, 48 at
/// 300 % — so the exact size is handed over instead of letting the shell resample one PNG.
/// `(px, recording, paused)`; ascending.
const ICONS: [(u32, &[u8], &[u8]); 7] = [
    (16, include_bytes!("../icons/tray/16.png"), include_bytes!("../icons/tray/16-paused.png")),
    (20, include_bytes!("../icons/tray/20.png"), include_bytes!("../icons/tray/20-paused.png")),
    (24, include_bytes!("../icons/tray/24.png"), include_bytes!("../icons/tray/24-paused.png")),
    (28, include_bytes!("../icons/tray/28.png"), include_bytes!("../icons/tray/28-paused.png")),
    (32, include_bytes!("../icons/tray/32.png"), include_bytes!("../icons/tray/32-paused.png")),
    (40, include_bytes!("../icons/tray/40.png"), include_bytes!("../icons/tray/40-paused.png")),
    (48, include_bytes!("../icons/tray/48.png"), include_bytes!("../icons/tray/48-paused.png")),
];

/// `CaptureStatus.pausedUntil` for "paused until resumed" (contract §9.5, `model.rs`).
const PAUSED_UNTIL_RESUMED: i64 = -1;

/// Pause / delete-recent periods offered by the menu, in minutes.
const PAUSES: [(&str, Option<i64>, Key); 4] = [
    ("pause:5", Some(5), Key::TrayPause5m),
    ("pause:30", Some(30), Key::TrayPause30m),
    ("pause:60", Some(60), Key::TrayPause1h),
    ("pause:until", None, Key::TrayPauseUntilResume),
];
const DELETES: [(&str, i64, Key); 5] = [
    ("delete:5", 5, Key::TrayDelete5m),
    ("delete:15", 15, Key::TrayDelete15m),
    ("delete:30", 30, Key::TrayDelete30m),
    ("delete:60", 60, Key::TrayDelete1h),
    ("delete:1440", 1440, Key::TrayDelete24h),
];

/// `setup()` step 8.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let state = TrayState::read(app);
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(state.icon())
        .tooltip(state.tooltip())
        .menu(&build_menu(app, &state)?)
        // Left click opens the shelf; the menu stays on right click.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                picker::summon(tray.app_handle());
            }
        })
        .on_menu_event(on_menu_event)
        .build(app)?;

    // Language / summon-hotkey changes re-label the menu.
    settings::subscribe(app, {
        let app = app.clone();
        move |old, new| {
            if old.general.language != new.general.language || old.hotkeys.summon != new.hotkeys.summon {
                refresh(&app);
            }
        }
    });
    Ok(())
}

/// Rebuild the menu, icon and tooltip from the current state.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let state = TrayState::read(app);
    match build_menu(app, &state) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                logx::warn("tray", &format!("set_menu: {e}"));
            }
        }
        Err(e) => logx::warn("tray", &format!("menu: {e}")),
    }
    let _ = tray.set_icon(Some(state.icon()));
    let _ = tray.set_tooltip(Some(state.tooltip()));
}

struct TrayState {
    lang: Lang,
    summon: String,
    paused: bool,
    /// `CaptureStatus.pausedUntil`: `None` recording, [`PAUSED_UNTIL_RESUMED`], or Unix ms.
    paused_until: Option<i64>,
    stack: usize,
}

impl TrayState {
    fn read(app: &AppHandle) -> TrayState {
        let s = settings::get(app);
        let paused_until = capture::status(app).paused_until;
        TrayState {
            lang: i18n_native::resolve(s.general.language),
            summon: s.hotkeys.summon,
            paused: paused_until.is_some(),
            paused_until,
            stack: paste::stack::len(app),
        }
    }

    fn t(&self, key: Key) -> &'static str {
        i18n_native::tr(self.lang, key)
    }

    /// "Liqlip — recording" / "Liqlip — paused until 14:30" / "Liqlip — recording paused",
    /// plus " · Paste stack: 3" while the stack is not empty.
    fn tooltip(&self) -> String {
        let until = self.paused_until.filter(|&ms| ms != PAUSED_UNTIL_RESUMED).and_then(local_time_label);
        tooltip_text(self.lang, self.paused, until.as_deref(), self.stack)
    }

    fn icon(&self) -> Image<'static> {
        let px = tray_icon_px();
        let (_, recording, paused) = ICONS.iter().find(|(size, _, _)| *size >= px).unwrap_or(&ICONS[ICONS.len() - 1]);
        let bytes = if self.paused { paused } else { recording };
        // The PNGs are compiled in (and decoded by a test); failure means they were replaced by junk.
        let Ok(img) = Image::from_bytes(bytes) else { return Image::new_owned(vec![0, 0, 0, 0], 1, 1) };
        if taskbar_is_light() {
            img
        } else {
            whitened(&img)
        }
    }
}

/// The tooltip from its parts (pure, so it is unit-tested). `until` = the local time a
/// timed pause ends, already formatted.
fn tooltip_text(lang: Lang, paused: bool, until: Option<&str>, stack: usize) -> String {
    let t = |k| i18n_native::tr(lang, k);
    let state = match (paused, until) {
        (false, _) => t(Key::TrayTooltip).to_string(),
        (true, Some(time)) => paused_until_template(lang).replace("{time}", time),
        (true, None) => t(Key::TrayTooltipPaused).to_string(),
    };
    if stack == 0 {
        state
    } else {
        format!("{state} · {}", t(Key::TrayStack).replace("{n}", &stack.to_string()))
    }
}

/// TEMPORARY (W2-D): this string belongs in `i18n_native` as `Key::TrayTooltipPausedUntil`
/// — the coordinator's file; the snippet is in the W2-D report. Replace the call with
/// `t(Key::TrayTooltipPausedUntil)` and delete this function when it lands.
fn paused_until_template(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Liqlip — paused until {time}",
        Lang::Ru => "Liqlip — пауза до {time}",
    }
}

/// The glyph recoloured white for a dark taskbar (alpha kept: the glyph is one colour).
fn whitened(img: &Image<'_>) -> Image<'static> {
    let mut rgba = img.rgba().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        px[..3].fill(255);
    }
    Image::new_owned(rgba, img.width(), img.height())
}

/// `"Alt + V"` → `"Alt+V"` for the right-aligned menu hint.
fn chord_hint(combo: &str) -> String {
    combo.split('+').map(str::trim).collect::<Vec<_>>().join("+")
}

fn build_menu(app: &AppHandle, st: &TrayState) -> tauri::Result<Menu<tauri::Wry>> {
    // A tab right-aligns the text after it in a Win32 menu — muda uses the same
    // mechanism for accelerators (muda windows/mod.rs), without parsing our chord.
    let open_label = if st.summon.is_empty() {
        st.t(Key::TrayOpen).to_string()
    } else {
        format!("{}\t{}", st.t(Key::TrayOpen), chord_hint(&st.summon))
    };
    let open = MenuItem::with_id(app, "open", open_label, true, None::<&str>)?;

    let pause_or_resume: Box<dyn IsMenuItem<tauri::Wry>> = if st.paused {
        Box::new(MenuItem::with_id(app, "resume", st.t(Key::TrayResume), true, None::<&str>)?)
    } else {
        let items = PAUSES
            .iter()
            .map(|(id, _, key)| MenuItem::with_id(app, *id, st.t(*key), true, None::<&str>))
            .collect::<tauri::Result<Vec<_>>>()?;
        let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|i| i as _).collect();
        Box::new(Submenu::with_items(app, st.t(Key::TrayPause), true, &refs)?)
    };
    let ignore = MenuItem::with_id(app, "ignore_next", st.t(Key::TrayIgnoreNext), true, None::<&str>)?;

    let delete_items = DELETES
        .iter()
        .map(|(id, _, key)| MenuItem::with_id(app, *id, st.t(*key), true, None::<&str>))
        .collect::<tauri::Result<Vec<_>>>()?;
    let delete_refs: Vec<&dyn IsMenuItem<tauri::Wry>> = delete_items.iter().map(|i| i as _).collect();
    let delete = Submenu::with_items(app, st.t(Key::TrayDeleteRecent), true, &delete_refs)?;

    let stack = MenuItem::with_id(
        app,
        "stack",
        st.t(Key::TrayStack).replace("{n}", &st.stack.to_string()),
        true,
        None::<&str>,
    )?;
    let settings_item = MenuItem::with_id(app, "settings", st.t(Key::TraySettings), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", st.t(Key::TrayQuit), true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let mut items: Vec<&dyn IsMenuItem<tauri::Wry>> =
        vec![&open, &sep1, pause_or_resume.as_ref(), &ignore, &delete];
    if st.stack > 0 {
        items.push(&stack);
    }
    items.extend([&sep2 as &dyn IsMenuItem<tauri::Wry>, &settings_item, &quit]);
    Menu::with_items(app, &items)
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id.as_ref();
    let result = match id {
        "open" | "stack" => {
            picker::summon(app);
            Ok(())
        }
        "resume" => capture::resume(app).map(|_| ()),
        "ignore_next" => capture::ignore_next(app).map(|_| ()),
        "settings" => {
            picker::show_settings(app, None);
            Ok(())
        }
        "quit" => {
            app.exit(0);
            Ok(())
        }
        _ => {
            if let Some((_, minutes, _)) = PAUSES.iter().find(|(pid, _, _)| *pid == id) {
                capture::pause(app, *minutes).map(|_| ())
            } else if let Some((_, minutes, key)) = DELETES.iter().find(|(did, _, _)| *did == id) {
                confirm_delete_recent(app, *minutes, *key);
                Ok(())
            } else {
                Ok(())
            }
        }
    };
    if let Err(e) = result {
        logx::warn("tray", &format!("{id}: {e}"));
    }
    refresh(app);
}

/// "Delete recent" is irreversible (SPEC §2.4.6) → native confirmation first.
fn confirm_delete_recent(app: &AppHandle, minutes: i64, period: Key) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let lang = i18n_native::resolve(settings::get(app).general.language);
    let t = |k| i18n_native::tr(lang, k);
    let app2 = app.clone();
    app.dialog()
        .message(format!("{}\n\n{}", t(period), t(Key::ConfirmDeleteRecentBody)))
        .title(t(Key::ConfirmDeleteRecentTitle))
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            t(Key::ConfirmDelete).into(),
            t(Key::Cancel).into(),
        ))
        .show(move |confirmed| {
            if !confirmed {
                return;
            }
            std::thread::spawn(move || match app2.state::<store::Store>().delete_recent(minutes) {
                Ok(n) => {
                    logx::warn("tray", &format!("deleted {n} recent records ({minutes} min)"));
                    events::clips_changed(
                        &app2,
                        &ClipsChanged { origin: ChangeOrigin::User, change: ClipsChange::Deleted, ids: None },
                    );
                }
                Err(e) => logx::warn("tray", &format!("delete recent: {e}")),
            });
        });
}

/// Windows taskbar theme: `SystemUsesLightTheme` (1 = light). Same registry read tao uses
/// for `AppsUseLightTheme` (tao `dark_mode.rs`). Missing value → dark (Windows 11 default).
/// https://learn.microsoft.com/windows/win32/api/winreg/nf-winreg-reggetvaluew
#[cfg(windows)]
fn taskbar_is_light() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: valid constant key/value names; `data`/`size` outlive the call and `size`
    // matches the buffer; RRF_RT_REG_DWORD restricts the value to 4 bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut core::ffi::c_void),
            Some(&mut size),
        )
    };
    status == ERROR_SUCCESS && data != 0
}

#[cfg(not(windows))]
fn taskbar_is_light() -> bool {
    false
}

/// The pixel size Windows draws a notification-area icon at: SM_CXSMICON for the taskbar's
/// DPI. The taskbar's own DPI (`GetDpiForWindow` on `Shell_TrayWnd`) follows a scale change
/// made after sign-in; the system DPI is the fallback when there is no Explorer taskbar.
/// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getsystemmetricsfordpi
/// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getdpiforwindow
#[cfg(windows)]
fn tray_icon_px() -> u32 {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SM_CXSMICON};
    // SAFETY: plain queries with constant arguments and no out-pointers; FindWindowW returns
    // an error instead of a null HWND, and GetDpiForWindow returns 0 for an invalid one.
    let px = unsafe {
        let taskbar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).map(|hwnd| GetDpiForWindow(hwnd)).unwrap_or(0);
        let dpi = if taskbar != 0 { taskbar } else { GetDpiForSystem() };
        GetSystemMetricsForDpi(SM_CXSMICON, dpi)
    };
    u32::try_from(px).ok().filter(|&p| p > 0).unwrap_or(16)
}

#[cfg(not(windows))]
fn tray_icon_px() -> u32 {
    16
}

/// A Unix-ms instant as the user's local short time ("14:30", "2:30 PM"): UTC → local time
/// zone, then the locale's time format without seconds.
/// https://learn.microsoft.com/windows/win32/api/timezoneapi/nf-timezoneapi-systemtimetotzspecificlocaltime
/// https://learn.microsoft.com/windows/win32/api/datetimeapi/nf-datetimeapi-gettimeformatex
#[cfg(windows)]
fn local_time_label(unix_ms: i64) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::Globalization::{GetTimeFormatEx, TIME_NOSECONDS};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    // FILETIME counts 100 ns ticks since 1601-01-01 UTC; Unix time starts 11 644 473 600 s later.
    let ticks = u64::try_from(unix_ms).ok()?.checked_add(11_644_473_600_000)?.checked_mul(10_000)?;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    let mut buf = [0u16; 64];
    // SAFETY: every pointer refers to a live local of the right type; the buffer length is
    // passed by the slice; a null locale name = LOCALE_NAME_USER_DEFAULT, a null format = the
    // locale's own.
    let written = unsafe {
        FileTimeToSystemTime(&ft, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
        GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, Some(&local), PCWSTR::null(), Some(&mut buf))
    };
    // The count includes the terminating NUL; 0 = failure.
    let len = usize::try_from(written).ok()?.checked_sub(1)?;
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(not(windows))]
fn local_time_label(_unix_ms: i64) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_hint_compacts_spaces() {
        assert_eq!(chord_hint("Alt + V"), "Alt+V");
        assert_eq!(chord_hint("Ctrl + Shift + NumAdd"), "Ctrl+Shift+NumAdd");
    }

    #[test]
    fn tray_icons_decode_at_their_size() {
        let mut last = 0;
        for (px, recording, paused) in ICONS {
            assert!(px > last, "ICONS must be ascending");
            last = px;
            for bytes in [recording, paused] {
                let img = Image::from_bytes(bytes).unwrap();
                assert_eq!((img.width(), img.height()), (px, px));
                // Monochrome glyph on transparency: some opaque ink, some empty pixels.
                assert!(img.rgba().chunks_exact(4).any(|p| p[3] == 255));
                assert!(img.rgba().chunks_exact(4).any(|p| p[3] == 0));
            }
        }
    }

    #[test]
    fn whitened_keeps_the_alpha() {
        let img = Image::new_owned(vec![0, 0, 0, 255, 0, 0, 0, 128, 0, 0, 0, 0], 3, 1);
        let white = whitened(&img);
        assert_eq!(white.rgba(), &[255, 255, 255, 255, 255, 255, 255, 128, 255, 255, 255, 0]);
        assert_eq!((white.width(), white.height()), (3, 1));
    }

    #[test]
    fn tooltip_says_the_state_and_the_stack() {
        let rec = tooltip_text(Lang::En, false, None, 0);
        assert_eq!(rec, i18n_native::tr(Lang::En, Key::TrayTooltip));
        assert_eq!(tooltip_text(Lang::En, true, Some("14:30"), 0), "Liqlip — paused until 14:30");
        assert_eq!(tooltip_text(Lang::Ru, true, Some("14:30"), 0), "Liqlip — пауза до 14:30");
        assert_eq!(tooltip_text(Lang::Ru, true, None, 0), i18n_native::tr(Lang::Ru, Key::TrayTooltipPaused));
        let with_stack = tooltip_text(Lang::Ru, true, Some("14:30"), 3);
        assert_eq!(with_stack, format!("Liqlip — пауза до 14:30 · {}", i18n_native::tr(Lang::Ru, Key::TrayStack).replace("{n}", "3")));
        // Shell_NotifyIcon truncates szTip at 128 UTF-16 units.
        assert!(with_stack.encode_utf16().count() < 128);
    }

    #[cfg(windows)]
    #[test]
    fn local_time_label_formats_a_time() {
        // 2023-11-14 22:13:20 UTC — a time of day in any zone; the format is the user's.
        let label = local_time_label(1_700_000_000_000).unwrap();
        assert!(label.chars().any(|c| c.is_ascii_digit()), "{label}");
        assert!(!label.contains('\0'));
        assert_eq!(local_time_label(-5), None);
    }

    #[test]
    fn tray_icon_px_is_a_real_size() {
        let px = tray_icon_px();
        assert!((16..=256).contains(&px), "{px}");
    }
}
