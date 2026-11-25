//! Paste stack (SPEC §3.11, ARCHITECTURE §6 "Очередь вставки") — owner **W1-C**.
//! Every mutation emits `stack:changed` (`events::stack_changed`) and calls
//! `crate::tray::refresh(app)` (the tray shows "Paste stack: N").
//!
//! In memory only (lives until exit). Items carry a unique `key` — the same record may be in
//! the stack twice. A record deleted from history is dropped from the stack on the next stack
//! operation ([`prune`]: `store.get_for_paste` → `not_found`). `snapshot`/`len` never touch the
//! database (the tray calls them from any thread, the UI thread included).

use super::compose::file_paths;
use super::flow::{AfterPaste, Origin};
use super::{focus, input, Delivery};
use crate::model::{HudKind, Kind, PasteMode, PasteOutcome, PasteReason, PasteResult, Stack, StackItem};
use crate::store::{PastePayload, Store};
use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

/// The pure stack: ordered items with unique keys.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StackState {
    items: Vec<StackItem>,
    seq: u64,
}

impl StackState {
    pub const fn new() -> StackState {
        StackState { items: Vec::new(), seq: 0 }
    }

    pub fn push(&mut self, clip_id: i64, kind: Kind, preview: String) -> String {
        self.seq += 1;
        let key = format!("s{}", self.seq);
        self.items.push(StackItem { key: key.clone(), clip_id, kind, preview });
        key
    }

    fn index(&self, key: &str) -> Result<usize, String> {
        self.items.iter().position(|i| i.key == key).ok_or_else(|| format!("not_found: stack item {key}"))
    }

    pub fn remove(&mut self, key: &str) -> Result<StackItem, String> {
        let i = self.index(key)?;
        Ok(self.items.remove(i))
    }

    /// Move `key` before `before` (`None` = to the end). Both keys are validated first, so a
    /// bad `before` leaves the stack untouched.
    pub fn move_before(&mut self, key: &str, before: Option<&str>) -> Result<(), String> {
        let from = self.index(key)?;
        if let Some(b) = before {
            self.index(b)?;
            if b == key {
                return Ok(());
            }
        }
        let item = self.items.remove(from);
        match before {
            None => self.items.push(item),
            Some(b) => {
                let to = self.items.iter().position(|i| i.key == b).unwrap_or(self.items.len());
                self.items.insert(to, item);
            }
        }
        Ok(())
    }

    pub fn reverse(&mut self) {
        self.items.reverse();
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn front(&self) -> Option<&StackItem> {
        self.items.first()
    }

    /// Drop items whose record is gone; `true` if anything changed.
    pub fn retain_clips(&mut self, keep: impl Fn(i64) -> bool) -> bool {
        let before = self.items.len();
        self.items.retain(|i| keep(i.clip_id));
        self.items.len() != before
    }

    pub fn clip_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.items.iter().map(|i| i.clip_id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn snapshot(&self) -> Stack {
        Stack { items: self.items.clone() }
    }
}

static STACK: Mutex<StackState> = Mutex::new(StackState::new());
/// Serializes stack pastes: two quick hotkey presses paste two different items, never the
/// same top twice.
static PASTING: Mutex<()> = Mutex::new(());

/// Card-like preview of a record for the stack popover (content rules from `content.rs`).
pub fn preview_of(p: &PastePayload) -> String {
    match p.kind {
        Kind::Image => String::new(),
        Kind::File => {
            let paths = file_paths(p);
            let first = paths.first().map(|s| s.rsplit(['\\', '/']).next().unwrap_or(s).to_string()).unwrap_or_default();
            if paths.len() > 1 {
                format!("{first} +{}", paths.len() - 1)
            } else {
                first
            }
        }
        _ => crate::content::make_preview(p.text.as_deref().unwrap_or("")),
    }
}

fn emit(app: &AppHandle) -> Stack {
    let snap = STACK.lock().snapshot();
    crate::events::stack_changed(app, &snap);
    crate::tray::refresh(app);
    snap
}

fn is_not_found(e: &str) -> bool {
    e.starts_with("not_found")
}

/// Drop items whose record was deleted from history (blocking: store reads). Emits if it
/// changed anything.
pub fn prune(app: &AppHandle) {
    let ids = STACK.lock().clip_ids();
    if ids.is_empty() {
        return;
    }
    let store = app.state::<Store>();
    let gone: Vec<i64> = ids
        .into_iter()
        .filter(|&id| matches!(store.get_for_paste(id), Err(e) if is_not_found(&e)))
        .collect();
    if !gone.is_empty() && STACK.lock().retain_clips(|id| !gone.contains(&id)) {
        emit(app);
    }
}

/// Append a record to the stack — capture's "collect copies" mode calls it after a
/// successful insert.
#[allow(dead_code)] // W1-B calls it
pub fn push(app: &AppHandle, clip_id: i64) -> Result<(), String> {
    let p = app.state::<Store>().get_for_paste(clip_id)?;
    STACK.lock().push(p.id, p.kind, preview_of(&p));
    emit(app);
    Ok(())
}

/// Current stack (no database access).
#[allow(dead_code)] // §15 API; `stack_get` uses the pruning `get`
pub fn snapshot(app: &AppHandle) -> Stack {
    let _ = app;
    STACK.lock().snapshot()
}

/// Number of items (tray label).
pub fn len(app: &AppHandle) -> usize {
    let _ = app;
    STACK.lock().len()
}

/// `stack_add`: every id must exist (all-or-nothing, like the mock), appended in order.
pub fn add(app: &AppHandle, ids: &[i64]) -> Result<Stack, String> {
    prune(app);
    let store = app.state::<Store>();
    let payloads = ids.iter().map(|&id| store.get_for_paste(id)).collect::<Result<Vec<_>, _>>()?;
    {
        let mut s = STACK.lock();
        for p in &payloads {
            s.push(p.id, p.kind, preview_of(p));
        }
    }
    Ok(emit(app))
}

/// Apply a mutation, then emit.
pub fn mutate(app: &AppHandle, f: impl FnOnce(&mut StackState) -> Result<(), String>) -> Result<Stack, String> {
    prune(app);
    f(&mut STACK.lock())?;
    Ok(emit(app))
}

/// `stack_get`: pruned snapshot (emits only if pruning changed something).
pub fn get(app: &AppHandle) -> Stack {
    prune(app);
    STACK.lock().snapshot()
}

fn empty() -> PasteOutcome {
    PasteOutcome { result: PasteResult::Failed, reason: Some(PasteReason::StackEmpty) }
}

/// Where a stack paste goes: from the shelf (the shelf is the active window — e.g. the stack
/// popover) → the remembered target through the shelf flow; otherwise the CURRENT foreground
/// window, without the shelf.
fn origin_and_target(app: &AppHandle) -> (Origin, Option<focus::Target>) {
    #[cfg(windows)]
    {
        let fg = focus::foreground();
        if focus::picker_hwnd(app) == Some(fg) {
            return (Origin::Shelf { keep_open: false }, focus::current());
        }
        (Origin::Foreground, focus::target_for(app, fg))
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        (Origin::Foreground, None)
    }
}

fn hotkey_wait_keys(app: &AppHandle) -> Vec<u16> {
    input::hotkey_main_vk(&crate::settings::get(app).hotkeys.stack_next).into_iter().collect()
}

/// Paste the top item into the CURRENT foreground window (no shelf) and pop it; empty
/// stack → `hud::show(stack_empty)`. Blocking — the hotkey engine (W1-D) calls it on a
/// worker thread, `stack_paste_next` wraps it.
#[allow(dead_code)] // W1-D calls it
pub fn paste_next(app: &AppHandle) -> Result<PasteOutcome, String> {
    paste_next_with(app, true)
}

/// `hud_on_empty`: the hotkey path shows "stack is empty"; the command (UI) does not.
pub fn paste_next_with(app: &AppHandle, hud_on_empty: bool) -> Result<PasteOutcome, String> {
    let _serial = PASTING.lock();
    prune(app);
    let Some(top) = STACK.lock().front().cloned() else {
        if hud_on_empty {
            crate::picker::hud::show(app, HudKind::StackEmpty, None);
        }
        return Ok(empty());
    };
    let payload = match app.state::<Store>().get_for_paste(top.clip_id) {
        Ok(p) => p,
        Err(e) if is_not_found(&e) => {
            // Deleted between prune and now: drop it, report not_found.
            let _ = STACK.lock().remove(&top.key);
            emit(app);
            return Ok(PasteOutcome { result: PasteResult::Failed, reason: Some(PasteReason::NotFound) });
        }
        Err(e) => return Err(e),
    };
    let (origin, target) = origin_and_target(app);
    let after = if crate::settings::get(app).stack.newline_after_paste { AfterPaste::Enter } else { AfterPaste::Nothing };
    let out = super::deliver(
        app,
        &[payload],
        Delivery { mode: PasteMode::Auto, separator: None, origin, target, after, wait_keys: hotkey_wait_keys(app) },
    )?;
    if out.result != PasteResult::Failed {
        // Popped once it is on the clipboard (pasted, or copied for a manual Ctrl+V).
        if STACK.lock().remove(&top.key).is_ok() {
            emit(app);
        }
    }
    Ok(out)
}

/// `stack_paste_all(separator)`: the whole stack as one multi-paste (the stack is kept —
/// "Очистить" empties it explicitly).
pub fn paste_all(app: &AppHandle, separator: &str) -> Result<PasteOutcome, String> {
    let _serial = PASTING.lock();
    prune(app);
    let ids: Vec<i64> = STACK.lock().snapshot().items.iter().map(|i| i.clip_id).collect();
    if ids.is_empty() {
        return Ok(empty());
    }
    let payloads = super::load_payloads(app, &ids)?;
    if payloads.is_empty() {
        return Ok(PasteOutcome { result: PasteResult::Failed, reason: Some(PasteReason::NotFound) });
    }
    let (origin, target) = origin_and_target(app);
    let after = if crate::settings::get(app).stack.newline_after_paste { AfterPaste::Enter } else { AfterPaste::Nothing };
    super::deliver(
        app,
        &payloads,
        Delivery { mode: PasteMode::Auto, separator: Some(separator), origin, target, after, wait_keys: Vec::new() },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste::compose::tests::{files, image, text};

    fn keys(s: &StackState) -> Vec<String> {
        s.snapshot().items.into_iter().map(|i| i.key).collect()
    }

    fn filled() -> StackState {
        let mut s = StackState::new();
        for id in [10, 20, 30] {
            s.push(id, Kind::Text, format!("p{id}"));
        }
        s
    }

    #[test]
    fn keys_are_unique_even_for_the_same_record() {
        let mut s = StackState::new();
        let a = s.push(1, Kind::Text, "x".into());
        let b = s.push(1, Kind::Text, "x".into());
        assert_ne!(a, b);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn move_remove_reverse_clear() {
        let mut s = filled();
        assert_eq!(keys(&s), ["s1", "s2", "s3"]);
        s.move_before("s3", Some("s1")).unwrap();
        assert_eq!(keys(&s), ["s3", "s1", "s2"]);
        s.move_before("s3", None).unwrap();
        assert_eq!(keys(&s), ["s1", "s2", "s3"]);
        s.move_before("s2", Some("s2")).unwrap();
        assert_eq!(keys(&s), ["s1", "s2", "s3"], "before itself = no-op");
        assert!(s.move_before("s1", Some("nope")).is_err());
        assert_eq!(keys(&s), ["s1", "s2", "s3"], "bad target leaves the stack intact");
        assert!(s.move_before("zz", None).unwrap_err().starts_with("not_found"));
        assert_eq!(s.remove("s2").unwrap().clip_id, 20);
        assert!(s.remove("s2").is_err());
        s.reverse();
        assert_eq!(keys(&s), ["s3", "s1"]);
        assert_eq!(s.front().unwrap().clip_id, 30);
        s.clear();
        assert!(s.len() == 0 && s.front().is_none());
    }

    #[test]
    fn prune_drops_deleted_records() {
        let mut s = filled();
        s.push(20, Kind::Text, "again".into());
        assert_eq!(s.clip_ids(), vec![10, 20, 30]);
        assert!(s.retain_clips(|id| id != 20));
        assert_eq!(keys(&s), ["s1", "s3"]);
        assert!(!s.retain_clips(|_| true), "nothing changed");
    }

    #[test]
    fn previews() {
        assert_eq!(preview_of(&text(1, "hello")), "hello");
        assert_eq!(preview_of(&image(2)), "");
        assert_eq!(preview_of(&files(3, &["C:\\dir\\a.txt", "C:\\dir\\b.txt", "C:\\c"])), "a.txt +2");
        assert_eq!(preview_of(&files(4, &["C:\\dir\\only.pdf"])), "only.pdf");
    }
}
