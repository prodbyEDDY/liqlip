//! Windows side of the hotkey engine: the `RegisterHotKey` registrar thread and the
//! on-demand `WH_KEYBOARD_LL` hook. Pure grammar/planning lives in `combo.rs`.
//!
//! Threads:
//! - **registrar** — owns every `RegisterHotKey` registration (they belong to the thread
//!   that made them: "If the hWnd parameter is NULL, WM_HOTKEY messages are posted to the
//!   message queue of the calling thread", RegisterHotKey docs) and runs `combo::reconcile`.
//!   Requests arrive through a channel + a thread-message doorbell.
//! - **hook** — exists only while a hook-only chord is bound or the recorder is armed. Its
//!   callback does no work (CLAUDE.md hard rule 3): it reads/updates small state with
//!   `try_lock` (never blocks), decides "swallow", posts a job to the dispatcher, returns.
//!   https://learn.microsoft.com/windows/win32/winmsg/lowlevelkeyboardproc — a callback that
//!   exceeds `LowLevelHooksTimeout` is silently removed on Windows 7+, and "there is no way for
//!   the application to know whether the hook is removed". So the hook thread re-installs the
//!   hook every [`HOOK_REFRESH_MS`] while it exists — the watchdog (W4, FIX-12).

use super::combo::{self, Backend, Combo, Mods, Slots, SLOTS};
use super::{Capture, Job, CAPTURE};
use crate::logx;
use crate::model::SettingsErrorCode;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;
use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL,
    MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KillTimer, PeekMessageW, PostThreadMessageW,
    SetTimer, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_APP, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
};

/// Liqlip's own SendInput (W1-C `paste::input::{EXTRA_INFO_TAG, MASK_VK}`): every injected
/// event carries this `dwExtraInfo` ("LQLP"), and the unassigned VK 0xE8 masks a bare
/// Alt/Win release. The hook
/// ignores both — they are neither the user's chords nor physical modifier state.
const LIQLIP_EXTRA_INFO: usize = 0x4C51_4C50;
const LIQLIP_MASK_VK: u32 = 0xE8;

/// Doorbell for the registrar / stop request for the hook thread.
const WM_WAKE: u32 = WM_APP + 0x4C;
/// `RegisterHotKey` ids: "An application must specify an id value in the range 0x0000
/// through 0xBFFF" (RegisterHotKey docs).
const ID_BASE: i32 = 0x4C10;
/// A chord taken by another app at startup is retried this often until it binds.
const RETRY_MS: u32 = 30_000;
/// Hook watchdog period (FIX-12): a hook Windows removed silently is back within this time.
const HOOK_REFRESH_MS: u32 = 10_000;
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

// ── Registrar ───────────────────────────────────────────────────────────────

pub(super) enum Request {
    /// Reconcile to `want` (`prev` = what a refused slot falls back to).
    Apply {
        prev: Slots<Option<Combo>>,
        want: Slots<Option<Combo>>,
        reply: mpsc::Sender<Slots<Option<SettingsErrorCode>>>,
    },
    /// Report slots whose wanted chord is not live (after one retry), without changes.
    Status { reply: mpsc::Sender<Slots<Option<SettingsErrorCode>>> },
    /// Recording: release every OS registration / take them back.
    Suspend { reply: mpsc::Sender<()> },
    Resume { reply: mpsc::Sender<()> },
}

struct Registrar {
    thread_id: u32,
    tx: mpsc::Sender<Request>,
}

static REGISTRAR: OnceLock<Result<Registrar, String>> = OnceLock::new();
/// Serializes callers so a doorbell always pairs with its request.
static CALL: Mutex<()> = Mutex::new(());

/// Hook-backed bindings, read by the hook callback (`try_lock` only).
struct HookBinding {
    combo: Combo,
    /// Latch: true while the main key is held (collapses auto-repeat, matches the up).
    down: bool,
}
static HOOK_TABLE: Mutex<Slots<Option<HookBinding>>> = Mutex::new([const { None }; SLOTS]);

fn registrar() -> Result<&'static Registrar, String> {
    match REGISTRAR.get_or_init(spawn_registrar) {
        Ok(r) => Ok(r),
        Err(e) => Err(e.clone()),
    }
}

fn spawn_registrar() -> Result<Registrar, String> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Request>();
    std::thread::Builder::new()
        .name("liqlip-hotkeys".into())
        .spawn(move || registrar_loop(ready_tx, rx))
        .map_err(|e| format!("spawn: {e}"))?;
    let thread_id = ready_rx
        .recv_timeout(REPLY_TIMEOUT)
        .map_err(|_| "registrar start timed out".to_string())?;
    Ok(Registrar { thread_id, tx })
}

/// Send a request and wait for its reply (bounded).
pub(super) fn request<T>(make: impl FnOnce(mpsc::Sender<T>) -> Request) -> Result<T, String> {
    let _call = CALL.lock();
    let r = registrar()?;
    let (reply_tx, reply_rx) = mpsc::channel();
    r.tx.send(make(reply_tx)).map_err(|_| "registrar gone".to_string())?;
    // SAFETY: plain thread message to a thread that created its queue (PeekMessage at start).
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-postthreadmessagew
    unsafe { PostThreadMessageW(r.thread_id, WM_WAKE, WPARAM(0), LPARAM(0)) }
        .map_err(|e| format!("doorbell: {e}"))?;
    reply_rx
        .recv_timeout(REPLY_TIMEOUT)
        .map_err(|_| "registrar reply timed out".to_string())
}

fn os_modifiers(m: Mods) -> HOT_KEY_MODIFIERS {
    // MOD_NOREPEAT: "keyboard auto-repeat does not yield multiple hotkey notifications".
    let mut v = MOD_NOREPEAT.0;
    if m.alt {
        v |= MOD_ALT.0;
    }
    if m.ctrl {
        v |= MOD_CONTROL.0;
    }
    if m.shift {
        v |= MOD_SHIFT.0;
    }
    if m.win {
        v |= MOD_WIN.0;
    }
    HOT_KEY_MODIFIERS(v)
}

/// State owned by the registrar thread.
struct Engine {
    live: Slots<Option<Combo>>,
    want: Slots<Option<Combo>>,
    suspended: bool,
    retry_timer: usize,
}

impl Engine {
    fn register(&self, slot: usize, c: Combo) -> Result<(), SettingsErrorCode> {
        match combo::backend(&c) {
            Backend::Hook => {
                HOOK_TABLE.lock()[slot] = Some(HookBinding { combo: c, down: false });
                Ok(())
            }
            Backend::Os => {
                // SAFETY: thread-associated hotkey (hWnd NULL) on this thread's queue.
                match unsafe { RegisterHotKey(None, ID_BASE + slot as i32, os_modifiers(c.mods), c.vk) } {
                    Ok(()) => Ok(()),
                    // "RegisterHotKey also fails if the keystrokes specified for the hot key
                    // have already been registered for another hot key" → ERROR_HOTKEY_ALREADY_REGISTERED.
                    Err(e) if e.code() == windows::core::HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0) => {
                        Err(SettingsErrorCode::Taken)
                    }
                    Err(e) => {
                        logx::warn("hotkeys", &format!("RegisterHotKey slot {slot}: {e}"));
                        Err(SettingsErrorCode::Invalid)
                    }
                }
            }
        }
    }

    fn unregister(&self, slot: usize, c: Combo) {
        match combo::backend(&c) {
            Backend::Hook => HOOK_TABLE.lock()[slot] = None,
            Backend::Os => {
                // SAFETY: id registered by this thread.
                if let Err(e) = unsafe { UnregisterHotKey(None, ID_BASE + slot as i32) } {
                    logx::warn("hotkeys", &format!("UnregisterHotKey slot {slot}: {e}"));
                }
            }
        }
    }

    fn reconcile(&mut self, prev: Slots<Option<Combo>>, want: Slots<Option<Combo>>) -> Slots<Option<SettingsErrorCode>> {
        // Interior state is only touched through `&self` in the closures; `live` is returned.
        let me = &*self;
        let (live, errs) = combo::reconcile(
            me.live,
            prev,
            want,
            &mut |slot, c| me.register(slot, c),
            &mut |slot, c| me.unregister(slot, c),
        );
        // A refused slot falls back to `prev` (what settings_set saves); others get `want`.
        let mut final_want = want;
        for i in 0..SLOTS {
            if errs[i].is_some() {
                final_want[i] = if live[i].is_some() { live[i] } else { prev[i] };
            }
        }
        self.live = live;
        self.want = final_want;
        self.arm_retry();
        errs
    }

    fn pending(&self) -> bool {
        (0..SLOTS).any(|i| self.want[i].is_some() && self.live[i] != self.want[i])
    }

    /// Thread timer (hWnd NULL → WM_TIMER on this thread's queue) while something is pending.
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-settimer
    fn arm_retry(&mut self) {
        let pending = self.pending() && !self.suspended;
        // SAFETY: thread timers owned by this thread.
        unsafe {
            if pending && self.retry_timer == 0 {
                self.retry_timer = SetTimer(None, 0, RETRY_MS, None);
            } else if !pending && self.retry_timer != 0 {
                let _ = KillTimer(None, self.retry_timer);
                self.retry_timer = 0;
            }
        }
    }

    fn suspend(&mut self) {
        if self.suspended {
            return;
        }
        self.suspended = true;
        for i in 0..SLOTS {
            if let Some(c) = self.live[i] {
                if combo::backend(&c) == Backend::Os {
                    self.unregister(i, c);
                }
            }
        }
        self.arm_retry();
    }

    fn resume(&mut self) {
        if !self.suspended {
            return;
        }
        self.suspended = false;
        for i in 0..SLOTS {
            if let Some(c) = self.live[i] {
                if combo::backend(&c) == Backend::Os && self.register(i, c).is_err() {
                    // Someone grabbed it while we were recording: pending, retried.
                    logx::warn("hotkeys", &format!("slot {i}: chord taken during recording"));
                    self.live[i] = None;
                }
            }
        }
        self.arm_retry();
    }

    fn status(&mut self) -> Slots<Option<SettingsErrorCode>> {
        if !self.suspended && self.pending() {
            let want = self.want;
            return self.reconcile(want, want);
        }
        let mut errs = [None; SLOTS];
        for i in 0..SLOTS {
            if self.want[i].is_some() && self.live[i] != self.want[i] {
                errs[i] = Some(SettingsErrorCode::Taken);
            }
        }
        errs
    }

    fn handle(&mut self, req: Request) {
        match req {
            Request::Apply { prev, want, reply } => {
                // A settings change ends a recording session (the recorder has committed).
                self.resume();
                let _ = reply.send(self.reconcile(prev, want));
            }
            Request::Status { reply } => {
                let _ = reply.send(self.status());
            }
            Request::Suspend { reply } => {
                self.suspend();
                let _ = reply.send(());
            }
            Request::Resume { reply } => {
                self.resume();
                let _ = reply.send(());
            }
        }
    }
}

fn registrar_loop(ready: mpsc::Sender<u32>, rx: mpsc::Receiver<Request>) {
    let mut msg = MSG::default();
    // SAFETY: forces creation of this thread's message queue before anyone posts to it
    // ("Call PeekMessage as shown here to force the system to create the message queue",
    // PostThreadMessage docs).
    unsafe {
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
    }
    // SAFETY: trivial.
    if ready.send(unsafe { GetCurrentThreadId() }).is_err() {
        return;
    }
    let mut engine = Engine { live: [None; SLOTS], want: [None; SLOTS], suspended: false, retry_timer: 0 };
    loop {
        // SAFETY: standard message loop on this thread.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            logx::warn("hotkeys", "registrar message loop exited");
            return;
        }
        match msg.message {
            WM_WAKE => {
                while let Ok(req) = rx.try_recv() {
                    engine.handle(req);
                }
            }
            WM_HOTKEY => {
                let slot = msg.wParam.0 as i64 - i64::from(ID_BASE);
                if (0..SLOTS as i64).contains(&slot) && !engine.suspended {
                    super::schedule(Job::Action(super::ACTIONS[slot as usize]));
                }
            }
            WM_TIMER if msg.wParam.0 == engine.retry_timer && engine.retry_timer != 0 => {
                if !engine.suspended && engine.pending() {
                    let want = engine.want;
                    let errs = engine.reconcile(want, want);
                    if errs.iter().all(Option::is_none) {
                        logx::warn("hotkeys", "pending hotkey bound after retry");
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Hook ────────────────────────────────────────────────────────────────────

struct HookThread {
    thread_id: u32,
    join: std::thread::JoinHandle<()>,
}

static HOOK: Mutex<Option<HookThread>> = Mutex::new(None);

/// Install or remove the hook to match `needed`. Callable from any thread except the hook
/// thread; serialized by the `HOOK` lock (never taken inside the callback).
pub(super) fn sync_hook(needed: bool) -> Result<(), String> {
    let mut hook = HOOK.lock();
    match (needed, hook.is_some()) {
        (true, false) => {
            *hook = Some(install()?);
            Ok(())
        }
        (false, true) => {
            if let Some(h) = hook.take() {
                // SAFETY: the hook thread created its queue before reporting ready.
                let _ = unsafe { PostThreadMessageW(h.thread_id, WM_WAKE, WPARAM(0), LPARAM(0)) };
                let _ = h.join.join();
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// True while a hook-backed chord is bound.
pub(super) fn has_hook_bindings() -> bool {
    HOOK_TABLE.lock().iter().any(Option::is_some)
}

fn install() -> Result<HookThread, String> {
    seed_modifiers();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let join = std::thread::Builder::new()
        .name("liqlip-kbd-hook".into())
        .spawn(move || hook_loop(ready_tx))
        .map_err(|e| format!("spawn: {e}"))?;
    match ready_rx.recv_timeout(REPLY_TIMEOUT) {
        Ok(Ok(thread_id)) => Ok(HookThread { thread_id, join }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("hook install timed out".into()),
    }
}

/// `SetWindowsHookExW(WH_KEYBOARD_LL)` from the calling thread — always the hook thread: "this
/// hook is called in the context of the thread that installed it … the thread that installed
/// the hook must have a message loop" (LowLevelKeyboardProc docs).
fn install_ll_hook() -> Result<HHOOK, String> {
    // SAFETY: global LL hook with this module's procedure; no other preconditions.
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).ok();
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0)
    }
    .map_err(|e| format!("SetWindowsHookExW: {e}"))
}

fn remove_ll_hook(hook: HHOOK) -> Result<(), String> {
    // SAFETY: a handle returned by SetWindowsHookExW on this thread.
    unsafe { UnhookWindowsHookEx(hook) }.map_err(|e| format!("UnhookWindowsHookEx: {e}"))
}

/// One watchdog step (FIX-12). The fresh hook goes in FIRST — "the SetWindowsHookEx function
/// always installs a hook procedure at the beginning of a hook chain" (Hooks Overview) — and
/// only then the old one is removed, so there is no moment without a hook. Both calls run
/// back to back on the hook thread, which pumps no messages in between, so no key event is
/// handled twice. A failed install keeps the old hook (tried again on the next tick); a failed
/// removal is only logged (Windows may have removed that hook already).
fn refresh_hook<H>(
    old: H,
    install: impl FnOnce() -> Result<H, String>,
    remove: impl FnOnce(H) -> Result<(), String>,
) -> (H, bool) {
    match install() {
        Ok(new) => {
            if let Err(e) = remove(old) {
                logx::warn("hotkeys", &format!("hook watchdog: old hook: {e}"));
            }
            (new, true)
        }
        Err(e) => {
            logx::warn("hotkeys", &format!("hook watchdog: {e}; the current hook is kept"));
            (old, false)
        }
    }
}

fn hook_loop(ready: mpsc::Sender<Result<u32, String>>) {
    let mut msg = MSG::default();
    // SAFETY: create the queue first (see registrar_loop), then install the global LL hook.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
    }
    let mut hook = match install_ll_hook() {
        Ok(h) => h,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // SAFETY: trivial.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    // The watchdog: a thread timer (hWnd NULL → WM_TIMER on this thread's queue, SetTimer docs),
    // alive exactly as long as the hook is needed — this thread exists only then.
    // SAFETY: thread timer owned by this thread, killed below.
    let watchdog = unsafe { SetTimer(None, 0, HOOK_REFRESH_MS, None) };
    if watchdog == 0 {
        logx::warn("hotkeys", "hook watchdog timer could not be created");
    }
    loop {
        // SAFETY: message loop on the hook thread (required for LL hooks).
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 || msg.message == WM_WAKE {
            break;
        }
        if msg.message == WM_TIMER && watchdog != 0 && msg.wParam.0 == watchdog {
            let (h, fresh) = refresh_hook(hook, install_ll_hook, remove_ll_hook);
            hook = h;
            if fresh {
                // A modifier that changed while a removed hook was gone is not missed. Safe
                // between callbacks: the key state is current for every event already seen.
                seed_modifiers();
            }
            continue;
        }
        // SAFETY: msg came from GetMessageW.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    if watchdog != 0 {
        // SAFETY: the thread timer created above.
        let _ = unsafe { KillTimer(None, watchdog) };
    }
    if let Err(e) = remove_ll_hook(hook) {
        logx::warn("hotkeys", &e);
    }
}

/// Held modifiers as the hook saw them. "LowLevelKeyboardProc … is called before the
/// asynchronous key state is updated", so the hook tracks transitions itself.
static MODIFIER_KEYS: AtomicU8 = AtomicU8::new(0);

fn modifier_bit(vk: u32) -> Option<u8> {
    Some(match vk {
        0x11 | 0xA2 => 1 << 0,
        0xA3 => 1 << 1,
        0x12 | 0xA4 => 1 << 2,
        0xA5 => 1 << 3,
        0x10 | 0xA0 => 1 << 4,
        0xA1 => 1 << 5,
        0x5B => 1 << 6,
        0x5C => 1 << 7,
        _ => return None,
    })
}

fn mods_from_bits(bits: u8) -> Mods {
    Mods {
        ctrl: bits & 0b0000_0011 != 0,
        alt: bits & 0b0000_1100 != 0,
        shift: bits & 0b0011_0000 != 0,
        win: bits & 0b1100_0000 != 0,
    }
}

/// At install (not in the callback): start from the real key state, so a modifier held
/// while the hook appears is not missed.
fn seed_modifiers() {
    let mut bits = 0u8;
    for vk in [0xA2u32, 0xA3, 0xA4, 0xA5, 0xA0, 0xA1, 0x5B, 0x5C] {
        // SAFETY: plain key-state query; high bit = currently down.
        if unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0 {
            bits |= modifier_bit(vk).unwrap_or(0);
        }
    }
    MODIFIER_KEYS.store(bits, Ordering::SeqCst);
}

fn track_modifiers(vk: u32, pressed: bool) -> Mods {
    if let Some(bit) = modifier_bit(vk) {
        if pressed {
            MODIFIER_KEYS.fetch_or(bit, Ordering::SeqCst);
        } else {
            MODIFIER_KEYS.fetch_and(!bit, Ordering::SeqCst);
        }
    }
    mods_from_bits(MODIFIER_KEYS.load(Ordering::SeqCst))
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lParam points to a KBDLLHOOKSTRUCT (LowLevelKeyboardProc docs).
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if kb.dwExtraInfo == LIQLIP_EXTRA_INFO || kb.vkCode == LIQLIP_MASK_VK {
            // SAFETY: pass our own synthetic input on unchanged.
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }
        let msg = wparam.0 as u32;
        let pressed = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let released = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if pressed || released {
            let mods = track_modifiers(kb.vkCode, pressed);
            // "If the hook procedure processed the message, it may return a nonzero value to
            // prevent the system from passing the message to the rest of the hook chain or
            // the target window procedure."
            if let Some(swallow) = on_capture(kb.vkCode, pressed, mods) {
                if swallow {
                    return LRESULT(1);
                }
            } else if on_input(kb.vkCode, pressed, mods) {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: pass the event on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The armed recorder sees every key first (normal hook bindings are suspended). Returns
/// `None` when not armed (or the state is momentarily locked — then the key passes).
fn on_capture(vk: u32, pressed: bool, mods: Mods) -> Option<bool> {
    let mut guard = CAPTURE.try_lock()?;
    let cap: &mut Capture = guard.as_mut()?;
    let mut done: Option<String> = None;
    let mut swallow = false;
    if vk == combo::VK_ESCAPE && pressed && mods == Mods::default() {
        done = Some(String::new()); // cancelled
        swallow = true;
    } else if let Some(token) = combo::sided_modifier_token(vk) {
        // Modifiers are only observed (swallowing Shift breaks typing). A modifier pressed
        // over other modifiers forms a chord ("Ctrl + LWin") committed on its release.
        if pressed {
            cap.last_mod = Some(vk);
            let preceding = combo::mods_without_vk(mods, vk);
            cap.modifier_chord = preceding.any().then(|| combo::combo_string(preceding, token));
        } else if cap.last_mod == Some(vk) {
            done = cap.modifier_chord.take();
            cap.last_mod = None;
        }
    } else if pressed {
        cap.last_mod = None;
        cap.modifier_chord = None;
        if let Some(token) = combo::vk_token(vk) {
            done = Some(combo::combo_string(mods, &token));
            swallow = true; // the focused window never sees the recorded key
            if combo::masks_release(mods) {
                super::schedule(Job::Mask); // recording "Super + X" must not open Start
            }
        }
    }
    if let Some(combo) = done {
        *guard = None;
        super::schedule(Job::Captured(combo));
    }
    Some(swallow)
}

/// Hook-backed bindings. Returns true when the event must be swallowed.
fn on_input(vk: u32, pressed: bool, mods: Mods) -> bool {
    let Some(mut table) = HOOK_TABLE.try_lock() else { return false };
    let mut fired: Option<usize> = None;
    let mut mask = false;
    let mut swallow = false;
    for (slot, b) in table.iter_mut().enumerate() {
        let Some(b) = b else { continue };
        if b.combo.vk != vk {
            continue;
        }
        if pressed {
            if combo::mods_without_vk(mods, vk) == b.combo.mods {
                if !b.down {
                    b.down = true;
                    fired = Some(slot);
                    mask = combo::masks_release(b.combo.mods);
                }
                swallow = true;
            }
        } else if b.down {
            // The release matches by key alone (modifiers may lift first).
            b.down = false;
            swallow = true;
        }
    }
    drop(table);
    if let Some(slot) = fired {
        if mask {
            super::schedule(Job::Mask); // `Alt + F12`: no menu mode on the Alt release
        }
        super::schedule(Job::Action(super::ACTIONS[slot]));
    }
    swallow
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// W4, FIX-12: the watchdog puts the new hook in before it takes the old one out (never a
    /// moment without a hook); a failed install keeps the old hook and removes nothing.
    #[test]
    fn hook_watchdog_installs_first_then_removes_the_old_hook() {
        let calls = RefCell::new(Vec::new());
        let (h, fresh) = refresh_hook(
            1u32,
            || {
                calls.borrow_mut().push("install".to_string());
                Ok(2)
            },
            |old| {
                calls.borrow_mut().push(format!("remove {old}"));
                Ok(())
            },
        );
        assert_eq!((h, fresh), (2, true));
        assert_eq!(*calls.borrow(), ["install", "remove 1"]);

        // Windows had already removed the old hook: the new one is still taken.
        let (h, fresh) = refresh_hook(2u32, || Ok(3), |_| Err("invalid hook handle".into()));
        assert_eq!((h, fresh), (3, true));

        // The install fails (e.g. resources): keep the current hook, remove nothing.
        calls.borrow_mut().clear();
        let (h, fresh) = refresh_hook(
            3u32,
            || Err("SetWindowsHookExW: failed".into()),
            |old| {
                calls.borrow_mut().push(format!("remove {old}"));
                Ok(())
            },
        );
        assert_eq!((h, fresh), (3, false));
        assert!(calls.borrow().is_empty());
    }
}
