//! Tray icon + menu (SPEC §5). Coordinator module (W0). Labels come from `i18n_native`;
//! actions call the module APIs of §15 (stubs until W1). The menu is rebuilt by
//! [`refresh`] — call it after anything the menu shows changes (pause state, stack size,
//! language, summon hotkey); it is cheap and callable from any thread.

use crate::i18n_native::{self, Key, Lang};
use crate::model::{ChangeOrigin, ClipsChange, ClipsChanged};
use crate::{capture, events, logx, paste, picker, settings, store};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "liqlip";

const ICON_LIGHT: &[u8] = include_bytes!("../icons/tray-light.png");
const ICON_DARK: &[u8] = include_bytes!("../icons/tray-dark.png");
const ICON_LIGHT_PAUSED: &[u8] = include_bytes!("../icons/tray-light-paused.png");
const ICON_DARK_PAUSED: &[u8] = include_bytes!("../icons/tray-dark-paused.png");

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
    stack: usize,
}

impl TrayState {
    fn read(app: &AppHandle) -> TrayState {
        let s = settings::get(app);
        TrayState {
            lang: i18n_native::resolve(s.general.language),
            summon: s.hotkeys.summon,
            paused: capture::status(app).paused_until.is_some(),
            stack: paste::stack::len(app),
        }
    }

    fn t(&self, key: Key) -> &'static str {
        i18n_native::tr(self.lang, key)
    }

    fn tooltip(&self) -> &'static str {
        self.t(if self.paused { Key::TrayTooltipPaused } else { Key::TrayTooltip })
    }

    fn icon(&self) -> Image<'static> {
        let light_taskbar = taskbar_is_light();
        let bytes = match (light_taskbar, self.paused) {
            (true, false) => ICON_LIGHT,
            (true, true) => ICON_LIGHT_PAUSED,
            (false, false) => ICON_DARK,
            (false, true) => ICON_DARK_PAUSED,
        };
        // The PNGs are compiled in; decoding can only fail if they were replaced by junk.
        Image::from_bytes(bytes).unwrap_or_else(|_| Image::new(&[0, 0, 0, 0], 1, 1).to_owned())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_hint_compacts_spaces() {
        assert_eq!(chord_hint("Alt + V"), "Alt+V");
        assert_eq!(chord_hint("Ctrl + Shift + NumAdd"), "Ctrl+Shift+NumAdd");
    }

    #[test]
    fn tray_icons_decode() {
        for bytes in [ICON_LIGHT, ICON_DARK, ICON_LIGHT_PAUSED, ICON_DARK_PAUSED] {
            let img = Image::from_bytes(bytes).unwrap();
            assert_eq!((img.width(), img.height()), (32, 32));
        }
    }
}
