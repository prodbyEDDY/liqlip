//! The clipboard's current text on demand — `clipboard_text` (FIX-09, W3-A): `{clipboard}` in
//! snippets is what the clipboard holds NOW, not the newest history record (that one misses
//! private copies — KeePass marks them — and whatever the stack hotkey just pasted).
//!
//! Same rules as a capture snapshot (CLAUDE.md hard rule 4), against the same
//! [`ClipboardApi`] seam, so it is unit-tested on the in-memory clipboard:
//! - the owner window is probed first (`WM_NULL`, `SMTO_ABORTIFHUNG`): a hung owner is never
//!   asked to render a delayed format;
//! - ONE `OpenClipboard` (retried: 10 × 25 ms), only `CF_UNICODETEXT` is read, closed at once
//!   (guard — every path, panics included);
//! - the read runs on its own short thread and the caller waits at most the open budget + 1 s:
//!   `GetClipboardData` cannot be cancelled while an owner renders, so an owner that hangs
//!   mid-render costs the caller one timeout, never a hang (the abandoned thread closes the
//!   clipboard as soon as the owner answers — `snapshot.rs` explains why it is not killed).
//!
//! Privacy markers are deliberately NOT honoured here: this is an explicit user action
//! (pasting a snippet that says `{clipboard}`), the text is not stored and never logged.

use super::formats::{self, TEXT_READ_UNITS};
use super::snapshot::{ClipboardApi, Timing, CF_UNICODETEXT};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Arc};

/// Why the text could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    /// Another app kept the clipboard open through every retry.
    Busy,
    /// The owner window did not answer `WM_NULL` (hung) — not asked to render.
    OwnerNotResponding,
    /// The read did not finish in time (an owner stuck rendering a delayed format).
    Timeout,
}

impl TextError {
    /// Command error (`busy: …`, ARCHITECTURE §9.6) — the shelf falls back to the history.
    pub fn to_command_error(self) -> String {
        match self {
            TextError::Busy => "busy: the clipboard is held by another application".into(),
            TextError::OwnerNotResponding => "busy: the clipboard owner is not responding".into(),
            TextError::Timeout => "busy: the clipboard did not answer in time".into(),
        }
    }
}

/// Closes the clipboard on every exit path once it is open.
struct Close<'a, C: ClipboardApi + ?Sized>(&'a C);
impl<C: ClipboardApi + ?Sized> Drop for Close<'_, C> {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// `CF_UNICODETEXT` (UTF-16LE, up to the first NUL) → text, at most `content::TEXT_MAX_BYTES`
/// (2 MB, cut on a char boundary — the limit every other text in Liqlip has).
pub fn decode_text(bytes: &[u8]) -> String {
    let units = bytes.len() / 2;
    let take = units.min(TEXT_READ_UNITS);
    let v: Vec<u16> = bytes.chunks_exact(2).take(take).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let (text, _cut) = formats::decode_utf16_text(&v, units > take);
    let (kept, _) = crate::content::truncate_text(&text);
    if kept.len() == text.len() {
        text
    } else {
        kept.to_string()
    }
}

/// One `OpenClipboard` … `CloseClipboard`: the text, or `None` when there is no text format.
fn read_once<C: ClipboardApi + ?Sized>(api: &C, t: &Timing) -> Result<Option<String>, TextError> {
    let attempts = t.open_attempts.max(1);
    let mut opened = false;
    for attempt in 0..attempts {
        if api.try_open() {
            opened = true;
            break;
        }
        if attempt + 1 < attempts {
            std::thread::sleep(t.open_delay);
        }
    }
    if !opened {
        return Err(TextError::Busy);
    }
    let _close = Close(api);
    // `IsClipboardFormatAvailable` covers CF_TEXT / CF_OEMTEXT, which Windows converts.
    if !api.available(CF_UNICODETEXT) {
        return Ok(None);
    }
    let mut text = None;
    api.read_with(CF_UNICODETEXT, &mut |b| text = Some(decode_text(b)));
    Ok(text)
}

/// The clipboard's current text (`None` = no text on the clipboard). Blocking for at most the
/// owner probe + the open retries + `t.format_timeout` (≈ 1.5 s with the defaults).
pub fn current_text<C: ClipboardApi>(api: &Arc<C>, t: &Timing) -> Result<Option<String>, TextError> {
    if !api.owner_responsive(t.owner_probe) {
        return Err(TextError::OwnerNotResponding);
    }
    let (tx, rx) = mpsc::channel();
    let (reader, timing) = (api.clone(), t.clone());
    std::thread::Builder::new()
        .name("liqlip-clip-text".into())
        .spawn(move || {
            let r = catch_unwind(AssertUnwindSafe(|| read_once(&*reader, &timing))).unwrap_or(Err(TextError::Busy));
            let _ = tx.send(r);
        })
        .map_err(|_| TextError::Busy)?;
    let wait = t.open_delay * t.open_attempts.max(1) + t.format_timeout;
    rx.recv_timeout(wait).unwrap_or(Err(TextError::Timeout))
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::fake::{utf16z, FakeClipboard, IDS};
    use super::super::snapshot::CF_DIB;
    use super::*;
    use std::time::{Duration, Instant};

    /// Content tests: deadlines no loaded machine can hit (only a real hang could).
    fn generous() -> Timing {
        let ms = Duration::from_millis;
        Timing { open_delay: ms(2), format_timeout: ms(10_000), ..Timing::default() }
    }

    fn wait_closed(cb: &FakeClipboard) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while cb.is_open() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!cb.is_open(), "the clipboard was left open");
    }

    #[test]
    fn reads_the_text_in_one_open_and_closes() {
        let cb = FakeClipboard::new();
        cb.set_text("Привет, {мир} 👋");
        assert_eq!(current_text(&cb, &generous()), Ok(Some("Привет, {мир} 👋".to_string())));
        assert_eq!(cb.opens(), 1);
        assert_eq!(cb.take_reads(), vec![CF_UNICODETEXT], "only the text format is read");
        wait_closed(&cb);
    }

    #[test]
    fn no_text_on_the_clipboard_is_none() {
        let cb = FakeClipboard::new();
        cb.set(vec![(CF_DIB, vec![0; 64])], None, None);
        assert_eq!(current_text(&cb, &generous()), Ok(None));
        assert!(cb.take_reads().is_empty(), "the image is never read");
        cb.set(vec![], Some(vec!["C:\\a.txt".into()]), None);
        assert_eq!(current_text(&cb, &generous()), Ok(None), "files are not text");
        wait_closed(&cb);
    }

    /// FIX-09: an explicit paste of a snippet may use a password manager's copy — the privacy
    /// markers only keep such text out of the HISTORY.
    #[test]
    fn private_copies_are_returned() {
        let cb = FakeClipboard::new();
        cb.set(
            vec![
                (CF_UNICODETEXT, utf16z("s3cret")),
                (IDS.exclude_monitor, vec![]),
                (IDS.can_include_history, 0u32.to_le_bytes().to_vec()),
                (IDS.viewer_ignore, vec![]),
            ],
            None,
            None,
        );
        assert_eq!(current_text(&cb, &generous()), Ok(Some("s3cret".to_string())));
    }

    #[test]
    fn empty_text_stops_at_the_first_nul() {
        let cb = FakeClipboard::new();
        cb.set(vec![(CF_UNICODETEXT, vec![0, 0, b'x', 0])], None, None);
        assert_eq!(current_text(&cb, &generous()), Ok(Some(String::new())));
    }

    #[test]
    fn busy_clipboard_is_retried_then_reported() {
        let cb = FakeClipboard::new();
        cb.set_text("x");
        cb.fail_next_opens(3);
        assert_eq!(current_text(&cb, &generous()), Ok(Some("x".to_string())), "3 failed opens are retried");
        cb.fail_next_opens(1_000);
        let r = current_text(&cb, &generous());
        assert_eq!(r, Err(TextError::Busy));
        assert!(r.unwrap_err().to_command_error().starts_with("busy:"));
        assert!(!cb.is_open());
    }

    #[test]
    fn hung_owner_is_not_asked_to_render() {
        let cb = FakeClipboard::new();
        cb.set_text("x");
        cb.set_unresponsive(true);
        assert_eq!(current_text(&cb, &generous()), Err(TextError::OwnerNotResponding));
        assert_eq!(cb.opens(), 0, "the clipboard is not even opened");
    }

    /// An owner hung in WM_RENDERFORMAT: the caller gets a timeout instead of a hang, and the
    /// abandoned read closes the clipboard once the owner answers. The hang is a gate the test
    /// releases (FIX-04 style), so the result does not depend on the machine's speed.
    #[test]
    fn owner_hung_mid_render_times_out_and_the_clipboard_is_closed_later() {
        let cb = FakeClipboard::new();
        cb.set_text("x");
        cb.hang(CF_UNICODETEXT);
        let t = Timing { open_delay: Duration::from_millis(2), format_timeout: Duration::from_millis(100), ..Timing::default() };
        assert_eq!(current_text(&cb, &t), Err(TextError::Timeout));
        cb.release();
        wait_closed(&cb);
    }

    #[test]
    fn huge_text_is_cut_to_the_text_limit_on_a_char_boundary() {
        let max = crate::content::TEXT_MAX_BYTES;
        // 'я' is 2 UTF-8 bytes: 2 MB + 2 bytes of it, cut to exactly 2 MB.
        let s = "я".repeat(max / 2 + 1);
        let out = decode_text(&utf16z(&s));
        assert_eq!(out.len(), max);
        assert!(out.chars().all(|c| c == 'я'));
        assert_eq!(decode_text(&utf16z("short")), "short");
        assert_eq!(decode_text(&[]), "");
        assert_eq!(decode_text(&[b'a']), "", "an odd trailing byte is ignored");
    }
}
