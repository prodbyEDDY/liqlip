//! Synthetic input (ARCHITECTURE §6 "Ввод") — owner **W1-C**. `SendInput` directly, no `enigo`.
//!
//! Sequence for one paste (one `SendInput` call, so the user's own keystrokes cannot
//! interleave — "These events are not interspersed with other keyboard or mouse input events",
//! learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput):
//! 1. if Alt or Win is physically held: one press of the unassigned VK `0xE8` (Virtual-Key
//!    Codes table: "0xE8 Unassigned") so the coming Alt/Win release is not "bare" — a bare
//!    Alt release puts the target into menu mode, a bare Win release opens Start (the
//!    AutoHotkey "menu mask key", same trick as Liquid Tools `dictation::mask_bare_modifier`);
//! 2. key-up for every physically held modifier (L/R Shift, Ctrl, Alt, Win —
//!    `GetAsyncKeyState`; SendInput "does not reset the keyboard's current state … check the
//!    keyboard's state with the GetAsyncKeyState function and correct as necessary");
//! 3. the chord (`Ctrl+V` / `Shift+Insert` / `Ctrl+Shift+V`);
//! 4. key-down again for the modifiers that were held, then another mask tap if Alt/Win is
//!    among them. The logical state then matches the physical keyboard again, so holding
//!    Alt+Shift and tapping V repeatedly pastes item after item instead of typing "v", and
//!    the user's eventual physical release is not "bare" (no menu, no Start, no Alt+Shift
//!    layout switch — the mask key intervenes).
//!
//! Keys are sent as **VK + scan code** (no `KEYEVENTF_SCANCODE`): applications match Ctrl+V by
//! the virtual key, which is `'V'` in the Russian layout too, while the scan code comes from
//! `MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC_EX, <target thread layout>)`; a `0xE0` high byte sets
//! `KEYEVENTF_EXTENDEDKEY` (Insert, arrows, right Ctrl/Alt, Win —
//! learn.microsoft.com/windows/win32/api/winuser/nf-winuser-mapvirtualkeyexw,
//! learn.microsoft.com/windows/win32/api/winuser/ns-winuser-keybdinput).

use crate::model::{PasteChord, PasteKey};

pub const VK_RETURN: u16 = 0x0D;
pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_LEFT: u16 = 0x25;
pub const VK_INSERT: u16 = 0x2D;
pub const VK_V: u16 = 0x56;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
/// "0xE8 Unassigned" (learn.microsoft.com/windows/win32/inputdev/virtual-key-codes).
pub const MASK_VK: u16 = 0xE8;

/// `dwExtraInfo` of every event Liqlip injects ("LQLP") — lets a low-level hook (hotkeys,
/// W1-D) recognise and ignore our own keystrokes.
pub const EXTRA_INFO_TAG: usize = 0x4C51_4C50;

/// Sided modifiers checked before a chord.
pub const MODIFIERS: [u16; 8] = [VK_LSHIFT, VK_RSHIFT, VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN];

/// Most clicks per caret move — a `{cursor}` deep inside a huge snippet is still bounded.
pub const MAX_CARET_BACK: u32 = 5000;

/// One synthetic key transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub vk: u16,
    pub up: bool,
}

const fn down(vk: u16) -> Key {
    Key { vk, up: false }
}
const fn up(vk: u16) -> Key {
    Key { vk, up: true }
}

/// Mask (if Alt/Win held) + key-ups for every held modifier, in [`MODIFIERS`] order.
pub fn release_sequence(held: &[u16]) -> Vec<Key> {
    let mut out = Vec::new();
    if has_menu_modifier(held) {
        out.push(down(MASK_VK));
        out.push(up(MASK_VK));
    }
    out.extend(MODIFIERS.iter().filter(|vk| held.contains(vk)).map(|&vk| up(vk)));
    out
}

/// Keys of a chord: modifiers first, the main key last.
pub fn chord_keys(chord: PasteChord) -> &'static [u16] {
    match chord {
        PasteChord::CtrlV => &[VK_CONTROL, VK_V],
        PasteChord::ShiftInsert => &[VK_SHIFT, VK_INSERT],
        PasteChord::CtrlShiftV => &[VK_CONTROL, VK_SHIFT, VK_V],
    }
}

/// Presses in order, releases in reverse.
pub fn chord_sequence(chord: PasteChord) -> Vec<Key> {
    let keys = chord_keys(chord);
    keys.iter().map(|&vk| down(vk)).chain(keys.iter().rev().map(|&vk| up(vk))).collect()
}

/// `n` taps of one key.
pub fn taps(vk: u16, n: u32) -> Vec<Key> {
    (0..n).flat_map(|_| [down(vk), up(vk)]).collect()
}

fn has_menu_modifier(held: &[u16]) -> bool {
    held.iter().any(|vk| matches!(*vk, VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN))
}

/// Key-downs restoring the held modifiers (reverse [`MODIFIERS`] order), then a mask tap if
/// Alt/Win is among them so their later physical release is not bare.
pub fn restore_sequence(held: &[u16]) -> Vec<Key> {
    let mut out: Vec<Key> = MODIFIERS.iter().rev().filter(|vk| held.contains(vk)).map(|&vk| down(vk)).collect();
    if has_menu_modifier(held) {
        out.push(down(MASK_VK));
        out.push(up(MASK_VK));
    }
    out
}

/// `keys` with the held modifiers released before and restored after — one batch.
pub fn wrapped(held: &[u16], keys: &[Key]) -> Vec<Key> {
    let mut seq = release_sequence(held);
    seq.extend_from_slice(keys);
    seq.extend(restore_sequence(held));
    seq
}

/// The whole paste batch: release held modifiers, the chord, restore them.
pub fn paste_sequence(held: &[u16], chord: PasteChord) -> Vec<Key> {
    wrapped(held, &chord_sequence(chord))
}

/// Key-ups for the chord's modifiers — sent if a batch was only partly inserted, so no
/// synthetic Ctrl/Shift stays logically down.
pub fn chord_cleanup(chord: PasteChord) -> Vec<Key> {
    let keys = chord_keys(chord);
    keys[..keys.len() - 1].iter().map(|&vk| up(vk)).collect()
}

/// File name of an exe path, lower-cased (`C:\x\WindowsTerminal.exe` → `windowsterminal.exe`).
pub fn exe_basename(exe: &str) -> String {
    exe.rsplit(['\\', '/']).next().unwrap_or(exe).to_lowercase()
}

/// Built-in chords for apps whose Ctrl+V is not a paste, each checked against the app's own
/// documentation:
/// - mintty (Git Bash, Cygwin, MSYS2): "Shift+Ins: Paste"; Ctrl+V pastes only with
///   `CtrlExchangeShift=yes` (mintty.github.io/mintty.1.html);
/// - PuTTY: "Pressing Shift-Ins … [has] the same effect" as the right-button paste
///   (the.earth.li/~sgtatham/putty/0.83/htmldoc/Chapter3.html §3.1.1).
///
/// Not listed (Ctrl+V already pastes by default): Windows Terminal — default bindings
/// `ctrl+v`, `ctrl+shift+v`, `shift+insert` (learn.microsoft.com/windows/terminal/customize-settings/actions);
/// the Windows 10/11 console host — "Enable Ctrl key shortcuts" is on by default.
pub fn builtin_chord(exe_base: &str) -> Option<PasteChord> {
    match exe_base {
        "mintty.exe" | "putty.exe" => Some(PasteChord::ShiftInsert),
        _ => None,
    }
}

/// The chord for a target exe (full path): the user's `settings.picker.pasteKeys` first (an
/// entry with a path separator matches the full path, otherwise the file name;
/// case-insensitive), then [`builtin_chord`], then Ctrl+V.
pub fn resolve_chord(exe: &str, keys: &[PasteKey]) -> PasteChord {
    let base = exe_basename(exe);
    let full = exe.to_lowercase();
    let user = keys.iter().find(|k| {
        let want = k.exe.trim().to_lowercase();
        if want.contains(['\\', '/']) {
            want.replace('/', "\\") == full.replace('/', "\\")
        } else {
            !want.is_empty() && want == base
        }
    });
    user.map(|k| k.chord).or_else(|| builtin_chord(&base)).unwrap_or(PasteChord::CtrlV)
}

/// Main (non-modifier) key of a hotkey in the Liquid Tools notation (`"Alt + Shift + V"` →
/// `0x56`): letters, digits, F1–F24 and a few named keys. `None` when unknown — the caller
/// then simply does not wait for it.
pub fn hotkey_main_vk(combo: &str) -> Option<u16> {
    let last = combo.rsplit('+').next()?.trim();
    let upper = last.to_ascii_uppercase();
    let b = upper.as_bytes();
    if b.len() == 1 && (b[0].is_ascii_uppercase() || b[0].is_ascii_digit()) {
        return Some(b[0] as u16);
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x70 + n - 1);
        }
    }
    match upper.as_str() {
        "SPACE" => Some(0x20),
        "ENTER" | "RETURN" => Some(VK_RETURN),
        "TAB" => Some(0x09),
        "INSERT" | "INS" => Some(VK_INSERT),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" => Some(0x21),
        "PAGEDOWN" => Some(0x22),
        "`" | "BACKQUOTE" => Some(0xC0),
        _ => None,
    }
}

/// Keys that could still be physically down when a paste starts and would autorepeat into
/// the target once focus moves (Enter / Ctrl+1…9 in the shelf, the hotkey's main key).
pub fn trigger_keys(extra: &[u16]) -> Vec<u16> {
    let mut v: Vec<u16> = vec![VK_RETURN];
    v.extend(0x31..=0x39u16); // '1'..'9'
    v.extend(0x61..=0x69u16); // VK_NUMPAD1..9
    for &k in extra {
        if !v.contains(&k) {
            v.push(k);
        }
    }
    v
}

/// Fallback extended-key table when `MapVirtualKeyExW` has no translation.
pub fn is_extended_vk(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x28 // PgUp PgDn End Home ← ↑ → ↓
            | 0x2D | 0x2E // Insert Delete
            | VK_RCONTROL | VK_RMENU | VK_LWIN | VK_RWIN | 0x5D /* Apps */ | 0x6F /* Divide */ | 0x90 /* NumLock */
    )
}

#[cfg(windows)]
pub use win::{foreground_layout, held_modifiers, send, wait_released};

#[cfg(windows)]
mod win {
    use super::{is_extended_vk, Key, EXTRA_INFO_TAG, MODIFIERS};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyboardLayout, MapVirtualKeyExW, SendInput, HKL, INPUT, INPUT_0, INPUT_KEYBOARD,
        KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC_EX, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    /// Keyboard layout of the target's thread (scan codes are looked up in it).
    #[derive(Clone, Copy)]
    pub struct Layout(HKL);

    /// `GetKeyboardLayout(GetWindowThreadProcessId(hwnd))` — the active input locale of the
    /// window's thread (learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getkeyboardlayout).
    pub fn foreground_layout(hwnd: isize) -> Layout {
        // SAFETY: plain queries on a window handle; a stale handle yields thread 0 → the
        // calling thread's layout, which is still a valid HKL.
        unsafe {
            let thread = GetWindowThreadProcessId(HWND(hwnd as *mut _), None);
            Layout(GetKeyboardLayout(thread))
        }
    }

    /// Is `vk` down right now ("The most significant bit … is set if the specified key is
    /// currently down" — learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getasynckeystate).
    fn is_down(vk: u16) -> bool {
        // SAFETY: no preconditions.
        unsafe { GetAsyncKeyState(vk as i32) < 0 }
    }

    pub fn held_modifiers() -> Vec<u16> {
        MODIFIERS.iter().copied().filter(|&vk| is_down(vk)).collect()
    }

    pub fn keys_down(vks: &[u16]) -> bool {
        vks.iter().any(|&vk| is_down(vk))
    }

    /// Poll (10 ms) until none of `vks` is down; `false` on timeout.
    pub fn wait_released(vks: &[u16], timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !keys_down(vks) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn input(key: Key, layout: Layout) -> INPUT {
        // SAFETY: pure translation query.
        let sc = unsafe { MapVirtualKeyExW(key.vk as u32, MAPVK_VK_TO_VSC_EX, Some(layout.0)) };
        let extended = matches!(sc >> 8, 0xE0 | 0xE1) || (sc == 0 && is_extended_vk(key.vk));
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if key.up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(key.vk),
                    wScan: (sc & 0xFF) as u16,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: EXTRA_INFO_TAG,
                },
            },
        }
    }

    /// Inject `keys` in one `SendInput` call; returns how many events were inserted (0 =
    /// blocked, e.g. by UIPI).
    pub fn send(keys: &[Key], layout: Layout) -> usize {
        if keys.is_empty() {
            return 0;
        }
        let inputs: Vec<INPUT> = keys.iter().map(|&k| input(k, layout)).collect();
        // SAFETY: a valid INPUT array and its element size, as the API requires.
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_chords() {
        assert_eq!(
            chord_sequence(PasteChord::CtrlV),
            vec![down(VK_CONTROL), down(VK_V), up(VK_V), up(VK_CONTROL)]
        );
        assert_eq!(
            chord_sequence(PasteChord::ShiftInsert),
            vec![down(VK_SHIFT), down(VK_INSERT), up(VK_INSERT), up(VK_SHIFT)]
        );
        assert_eq!(
            chord_sequence(PasteChord::CtrlShiftV),
            vec![down(VK_CONTROL), down(VK_SHIFT), down(VK_V), up(VK_V), up(VK_SHIFT), up(VK_CONTROL)]
        );
        assert_eq!(chord_cleanup(PasteChord::CtrlShiftV), vec![up(VK_CONTROL), up(VK_SHIFT)]);
    }

    #[test]
    fn held_alt_is_masked_released_and_restored_around_the_chord() {
        // Alt+V summoned the shelf and Alt is still down when Enter pastes.
        let seq = paste_sequence(&[VK_LMENU], PasteChord::CtrlV);
        assert_eq!(
            seq,
            vec![
                down(MASK_VK),
                up(MASK_VK),
                up(VK_LMENU),
                down(VK_CONTROL),
                down(VK_V),
                up(VK_V),
                up(VK_CONTROL),
                down(VK_LMENU),
                down(MASK_VK),
                up(MASK_VK),
            ]
        );
        // Nothing held → just the chord.
        assert_eq!(paste_sequence(&[], PasteChord::CtrlV), chord_sequence(PasteChord::CtrlV));
    }

    #[test]
    fn stack_hotkey_held_keeps_modifiers_logically_down_after_paste() {
        // Alt+Shift held (Alt+Shift+V): after the batch both are down again and the last
        // event is the mask tap, so the user's physical release is not bare.
        let seq = paste_sequence(&[VK_LSHIFT, VK_LMENU], PasteChord::CtrlV);
        let mut state = std::collections::HashMap::new();
        for k in &seq {
            state.insert(k.vk, !k.up);
        }
        assert_eq!(state.get(&VK_LMENU), Some(&true));
        assert_eq!(state.get(&VK_LSHIFT), Some(&true));
        assert_eq!(state.get(&VK_CONTROL), Some(&false));
        assert_eq!(&seq[seq.len() - 2..], &[down(MASK_VK), up(MASK_VK)]);
        // While the chord is sent, Alt and Shift are logically up (no Ctrl+Alt+Shift+V).
        let chord_at = seq.iter().position(|k| *k == down(VK_V)).unwrap();
        let before: Vec<_> = seq[..chord_at].iter().filter(|k| k.vk == VK_LMENU || k.vk == VK_LSHIFT).collect();
        assert!(before.iter().all(|k| k.up));
        // Restoring Shift alone needs no mask.
        assert_eq!(restore_sequence(&[VK_RSHIFT]), vec![down(VK_RSHIFT)]);
    }

    #[test]
    fn shift_only_needs_no_mask_and_win_does() {
        assert_eq!(release_sequence(&[VK_RSHIFT, VK_LSHIFT]), vec![up(VK_LSHIFT), up(VK_RSHIFT)]);
        assert_eq!(release_sequence(&[VK_LWIN]), vec![down(MASK_VK), up(MASK_VK), up(VK_LWIN)]);
        assert!(release_sequence(&[]).is_empty());
        // Alt+Shift+V (stack hotkey) with both still held; AltGr = LCtrl + RAlt.
        assert_eq!(
            release_sequence(&[VK_LMENU, VK_LSHIFT]),
            vec![down(MASK_VK), up(MASK_VK), up(VK_LSHIFT), up(VK_LMENU)]
        );
        assert_eq!(
            release_sequence(&[VK_RMENU, VK_LCONTROL]),
            vec![down(MASK_VK), up(MASK_VK), up(VK_LCONTROL), up(VK_RMENU)]
        );
    }

    #[test]
    fn chord_resolution_user_then_builtin_then_ctrl_v() {
        let keys = vec![
            PasteKey { exe: "WindowsTerminal.EXE".into(), chord: PasteChord::CtrlShiftV },
            PasteKey { exe: "C:\\Tools\\Putty.exe".into(), chord: PasteChord::CtrlV },
        ];
        assert_eq!(resolve_chord("c:\\program files\\wt\\windowsterminal.exe", &keys), PasteChord::CtrlShiftV);
        // Full-path entry matches only that path.
        assert_eq!(resolve_chord("c:\\tools\\putty.exe", &keys), PasteChord::CtrlV);
        assert_eq!(resolve_chord("d:\\other\\putty.exe", &keys), PasteChord::ShiftInsert, "built-in");
        assert_eq!(resolve_chord("C:\\Program Files\\Git\\usr\\bin\\mintty.exe", &[]), PasteChord::ShiftInsert);
        assert_eq!(resolve_chord("c:\\windows\\notepad.exe", &keys), PasteChord::CtrlV);
        assert_eq!(resolve_chord("", &keys), PasteChord::CtrlV, "unknown exe → Ctrl+V");
        assert_eq!(exe_basename("C:/a/B.EXE"), "b.exe");
    }

    #[test]
    fn hotkey_main_key_parsing() {
        assert_eq!(hotkey_main_vk("Alt + Shift + V"), Some(VK_V));
        assert_eq!(hotkey_main_vk("Ctrl + 1"), Some(0x31));
        assert_eq!(hotkey_main_vk("Alt + F12"), Some(0x7B));
        assert_eq!(hotkey_main_vk("Win + Space"), Some(0x20));
        assert_eq!(hotkey_main_vk("Alt + v"), Some(VK_V));
        assert_eq!(hotkey_main_vk("RAlt"), None);
        assert_eq!(hotkey_main_vk(""), None);
        assert_eq!(hotkey_main_vk("Alt + F25"), None);
    }

    #[test]
    fn trigger_keys_and_taps() {
        let t = trigger_keys(&[VK_V, VK_RETURN]);
        assert!(t.contains(&VK_RETURN) && t.contains(&0x31) && t.contains(&0x69) && t.contains(&VK_V));
        assert_eq!(t.iter().filter(|&&k| k == VK_RETURN).count(), 1, "no duplicates");
        assert_eq!(taps(VK_LEFT, 2), vec![down(VK_LEFT), up(VK_LEFT), down(VK_LEFT), up(VK_LEFT)]);
        assert!(taps(VK_LEFT, 0).is_empty());
    }

    #[test]
    fn extended_fallback_table() {
        assert!(is_extended_vk(VK_INSERT) && is_extended_vk(VK_LEFT) && is_extended_vk(VK_RMENU) && is_extended_vk(VK_LWIN));
        assert!(!is_extended_vk(VK_V) && !is_extended_vk(VK_LCONTROL) && !is_extended_vk(VK_RETURN) && !is_extended_vk(MASK_VK));
    }
}
