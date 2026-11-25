//! One clipboard snapshot = ONE `OpenClipboard` … `CloseClipboard` (CLAUDE.md hard rule 4,
//! ARCHITECTURE §5), written against the [`ClipboardApi`] seam so the whole algorithm is
//! unit-tested on an in-memory clipboard (tests never touch the user's clipboard).
//!
//! Order inside the open clipboard:
//! 1. `OpenClipboard` with retries (10 × 25 ms) — another app may hold it briefly;
//! 2. sequence number (dedupe / self-copy), owner process (self → skip; ignore list → skip,
//!    without reading any content);
//! 3. `EnumClipboardFormats`; privacy markers first (`ExcludeClipboardContentFromMonitorProcessing`
//!    / `Clipboard Viewer Ignore` by presence, `CanIncludeInClipboardHistory` /
//!    `CanUploadToCloudClipboard` by DWORD value 0 — unreadable counts as 0);
//! 4. ONLY the formats Liqlip stores, by priority: `CF_HDROP` (+`Preferred DropEffect`) →
//!    `CF_UNICODETEXT` + `HTML Format` + `Rich Text Format` → `PNG` → `CF_DIBV5` → `CF_DIB`;
//!    sizes are checked from headers before large payloads are copied;
//! 5. close. Decoding/encoding happens later, on the worker.
//!
//! **Hung source app (delayed rendering).** `GetClipboardData` for a delay-rendered format
//! makes Windows send `WM_RENDERFORMAT` to the owner and wait for it while *we* hold the
//! clipboard open (learn.microsoft.com "Clipboard Operations → Delayed Rendering"). The call
//! cannot be cancelled, so the snapshot runs on its own short-lived reader thread and the
//! capture loop waits at most `format_timeout` (1 s) per format and `budget` (3 s) per
//! snapshot ([`snapshot_with_deadline`]). On timeout the loop gives up that format and every
//! later one, keeps what was already read (only if the privacy checks completed), and moves
//! on; the stuck reader sees the cancel flag the moment the owner answers (or dies), skips
//! the rest and closes the clipboard. This is safe because the abandoned thread owns nothing
//! shared except the result slot (taken under a mutex) and an atomic flag, is never killed
//! (`TerminateThread` would corrupt user32/heap state), and no new snapshot starts while it is
//! alive (`reader_busy`). The capture loop additionally probes the owner window with
//! `SendMessageTimeoutW(WM_NULL, SMTO_ABORTIFHUNG)` before opening the clipboard, so a hung
//! owner is normally never asked to render at all (see `pipeline.rs`).

use super::formats::{self, RICH_MAX_BYTES, TEXT_READ_UNITS};
use super::imaging::ImageSource;
use super::state::CaptureState;
use super::CaptureConfig;
use crate::content::{FILES_MAX, IMAGE_MAX_PIXELS};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Standard clipboard formats (learn.microsoft.com "Standard Clipboard Formats"; same values
// as `windows::Win32::System::Ole::CF_*`).
pub const CF_TEXT: u32 = 1;
pub const CF_BITMAP: u32 = 2;
pub const CF_OEMTEXT: u32 = 7;
pub const CF_DIB: u32 = 8;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_HDROP: u32 = 15;
pub const CF_DIBV5: u32 = 17;

/// `DROPEFFECT_COPY` — default `Preferred DropEffect` when the source sets none.
pub const DROPEFFECT_COPY: u32 = 1;

/// Ids of the registered formats Liqlip reads (`RegisterClipboardFormatW` at start-up).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatIds {
    pub html: u32,
    pub rtf: u32,
    pub png: u32,
    pub drop_effect: u32,
    pub exclude_monitor: u32,
    pub can_include_history: u32,
    pub can_upload_cloud: u32,
    pub viewer_ignore: u32,
}

/// The process behind a clipboard change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerInfo {
    pub pid: u32,
    /// Full image path, lower-cased (`apps.exe`); `None` if the process can't be queried.
    pub exe: Option<String>,
    /// `true` = no clipboard owner window; this is the foreground window's process.
    pub from_foreground: bool,
}

/// Win32 seam (`win32.rs` in the app, a fake in tests). Every method is a thin wrapper over
/// one documented call; the policy lives in this module.
pub trait ClipboardApi: Send + Sync + 'static {
    fn ids(&self) -> FormatIds;
    /// `GetClipboardSequenceNumber` (0 = no access).
    fn sequence(&self) -> u32;
    fn own_pid(&self) -> u32;
    /// `GetClipboardOwner` → process; falls back to the foreground window's process.
    fn owner(&self) -> Option<OwnerInfo>;
    /// The owner window answers `WM_NULL` within `timeout` (true when there is no owner).
    fn owner_responsive(&self, timeout: Duration) -> bool;
    /// One `OpenClipboard(NULL)` attempt.
    fn try_open(&self) -> bool;
    fn close(&self);
    /// `EnumClipboardFormats` (clipboard open).
    fn formats(&self) -> Vec<u32>;
    /// `IsClipboardFormatAvailable` (covers synthesized formats).
    fn available(&self, fmt: u32) -> bool;
    /// `GetClipboardData(fmt)` + `GlobalLock`: `visit` sees the whole object and copies what it
    /// needs; unlocked afterwards. `false` = no data / lock failed.
    fn read_with(&self, fmt: u32, visit: &mut dyn FnMut(&[u8])) -> bool;
    /// `CF_HDROP` via `DragQueryFileW`: at most `max` paths + the total count.
    fn read_files(&self, max: usize) -> Option<(Vec<String>, usize)>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawFiles {
    pub paths: Vec<String>,
    /// Paths in the clipboard object (may exceed `paths.len()` = `FILES_MAX`).
    pub total: usize,
    pub drop_effect: u32,
}

/// Raw bytes of one snapshot, copied out of the clipboard; nothing decoded yet.
#[derive(Debug, Clone, Default)]
pub struct RawSnapshot {
    /// Sequence number while the clipboard was open, before reading.
    pub seq_open: u32,
    /// … and right before closing (delayed rendering during our read may bump it).
    pub seq_close: u32,
    pub owner: Option<OwnerInfo>,
    /// Privacy markers were checked and none applies (or `respectMarkers` is off).
    pub markers_checked: bool,
    pub files: Option<RawFiles>,
    pub text: Option<String>,
    /// CF_UNICODETEXT was longer than the read limit.
    pub text_cut: bool,
    /// Raw CF_HTML payload (header included).
    pub html: Option<Vec<u8>>,
    pub rtf: Option<Vec<u8>>,
    pub image: Option<(ImageSource, Vec<u8>)>,
    /// An image above 64 MP was present (not copied).
    pub image_too_large: Option<(u32, u32)>,
    /// A format timed out: it and every later one were dropped.
    pub incomplete: bool,
    pub format_count: usize,
}

impl RawSnapshot {
    pub fn has_content(&self) -> bool {
        self.files.is_some() || self.text.is_some() || self.image.is_some()
    }
}

/// Why a change was not captured (logged by reason only — never content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Liqlip's own write (sequence number or owner process).
    Own,
    IgnoredApp,
    PrivacyMarker,
    TypeDisabled,
    Secret,
    Empty,
    TooLarge,
    /// Privacy checks could not finish in time (hung owner).
    Timeout,
    /// The owner window did not answer; retries exhausted.
    OwnerNotResponding,
    /// The clipboard stayed busy; retries exhausted.
    Busy,
    /// Decoding failed.
    Invalid,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::Own => "own write",
            SkipReason::IgnoredApp => "ignored app",
            SkipReason::PrivacyMarker => "privacy marker",
            SkipReason::TypeDisabled => "type disabled",
            SkipReason::Secret => "looks like a secret",
            SkipReason::Empty => "nothing to store",
            SkipReason::TooLarge => "too large",
            SkipReason::Timeout => "privacy checks timed out",
            SkipReason::OwnerNotResponding => "owner not responding",
            SkipReason::Busy => "clipboard busy",
            SkipReason::Invalid => "undecodable",
        }
    }
}

/// Tunables (tests shrink them).
#[derive(Debug, Clone)]
pub struct Timing {
    pub debounce: Duration,
    pub debounce_max: Duration,
    pub open_attempts: u32,
    pub open_delay: Duration,
    pub format_timeout: Duration,
    pub budget: Duration,
    pub owner_probe: Duration,
    pub retry_backoff: Vec<Duration>,
}

impl Default for Timing {
    fn default() -> Self {
        let ms = Duration::from_millis;
        Timing {
            debounce: ms(30),
            debounce_max: ms(120),
            open_attempts: 10,
            open_delay: ms(25),
            format_timeout: ms(1000),
            budget: ms(3000),
            owner_probe: ms(250),
            retry_backoff: vec![ms(100), ms(250), ms(500), ms(1000), ms(2000)],
        }
    }
}

/// Is `exe` (full path, lower-case) on the ignore list? Entries match the file name
/// (`keepass.exe`, or the bare stem `keepass`) or the full path, case-insensitively, with
/// `/` and `\` equivalent.
pub fn is_ignored(exe: &str, ignored: &[String]) -> bool {
    let full = exe.replace('/', "\\").to_lowercase();
    let name = full.rsplit('\\').next().unwrap_or(&full);
    let stem = name.strip_suffix(".exe").unwrap_or(name);
    ignored.iter().any(|entry| {
        let e = entry.trim().replace('/', "\\").to_lowercase();
        !e.is_empty() && (e == full || e == name || (!e.contains('\\') && !e.contains('.') && e == stem))
    })
}

// ── Reader ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadStatus {
    Complete,
    Busy,
    Skipped(SkipReason),
    Cancelled,
}

#[derive(Default)]
struct ReaderShared {
    partial: Mutex<RawSnapshot>,
    cancel: AtomicBool,
}

enum ReaderEvent {
    /// A potentially blocking call starts now.
    Phase(Instant),
    Done(ReadStatus),
}

struct Progress<'a> {
    shared: &'a ReaderShared,
    tx: &'a mpsc::Sender<ReaderEvent>,
}

impl Progress<'_> {
    fn phase(&self) -> bool {
        let _ = self.tx.send(ReaderEvent::Phase(Instant::now()));
        !self.cancelled()
    }
    fn cancelled(&self) -> bool {
        self.shared.cancel.load(Ordering::SeqCst)
    }
    fn put(&self, f: impl FnOnce(&mut RawSnapshot)) {
        f(&mut self.shared.partial.lock());
    }
}

/// Closes the clipboard on every exit path, panics included.
struct OpenGuard<'a, C: ClipboardApi + ?Sized>(&'a C);
impl<C: ClipboardApi + ?Sized> Drop for OpenGuard<'_, C> {
    fn drop(&mut self) {
        self.0.close();
    }
}

fn read_snapshot<C: ClipboardApi + ?Sized>(
    api: &C,
    state: &CaptureState,
    cfg: &CaptureConfig,
    t: &Timing,
    p: &Progress,
) -> ReadStatus {
    // 1. Open, retried: another app may be holding the clipboard for a moment.
    let mut opened = false;
    for attempt in 0..t.open_attempts.max(1) {
        if !p.phase() {
            return ReadStatus::Cancelled;
        }
        if api.try_open() {
            opened = true;
            break;
        }
        if attempt + 1 < t.open_attempts {
            std::thread::sleep(t.open_delay);
        }
    }
    if !opened {
        return ReadStatus::Busy;
    }
    let _guard = OpenGuard(api);

    // 2. Identity: sequence, own write, owner, ignore list — no content read yet.
    let seq_open = api.sequence();
    p.put(|r| {
        r.seq_open = seq_open;
        r.seq_close = seq_open;
    });
    if state.is_own(seq_open) {
        return ReadStatus::Skipped(SkipReason::Own);
    }
    let mut owner = api.owner();
    if let Some(o) = &owner {
        if o.pid != 0 && o.pid == api.own_pid() {
            if !o.from_foreground {
                return ReadStatus::Skipped(SkipReason::Own); // our window owns the clipboard
            }
            // No owner, and Liqlip happens to be in front: the source is unknown, not us.
            owner = None;
        }
    }
    if let Some(o) = &owner {
        if let Some(exe) = &o.exe {
            if is_ignored(exe, &cfg.ignored_apps) {
                return ReadStatus::Skipped(SkipReason::IgnoredApp);
            }
        }
    }
    p.put(|r| r.owner = owner.clone());

    // 3. Formats + privacy markers.
    let ids = api.ids();
    let present = api.formats();
    p.put(|r| r.format_count = present.len());
    let has = |f: u32| f != 0 && present.contains(&f);
    if cfg.respect_markers {
        if has(ids.exclude_monitor) || has(ids.viewer_ignore) {
            return ReadStatus::Skipped(SkipReason::PrivacyMarker);
        }
        for marker in [ids.can_include_history, ids.can_upload_cloud] {
            if !has(marker) {
                continue;
            }
            if !p.phase() {
                return ReadStatus::Cancelled;
            }
            let mut value = None;
            api.read_with(marker, &mut |b| value = formats::marker_dword(b));
            // DWORD 0 = "do not include"; unreadable/malformed is treated the same way.
            if value.unwrap_or(0) == 0 {
                return ReadStatus::Skipped(SkipReason::PrivacyMarker);
            }
        }
    }
    p.put(|r| r.markers_checked = true);

    // 4. Content, only what is stored, by priority.
    if has(CF_HDROP) || api.available(CF_HDROP) {
        if !cfg.types.file {
            return ReadStatus::Skipped(SkipReason::TypeDisabled);
        }
        if !p.phase() {
            return ReadStatus::Cancelled;
        }
        if let Some((paths, total)) = api.read_files(FILES_MAX) {
            let mut drop_effect = DROPEFFECT_COPY;
            if has(ids.drop_effect) && p.phase() {
                api.read_with(ids.drop_effect, &mut |b| {
                    if let Some(v) = formats::marker_dword(b) {
                        drop_effect = v;
                    }
                });
            }
            if !paths.is_empty() {
                p.put(|r| r.files = Some(RawFiles { paths, total, drop_effect }));
                p.put(|r| r.seq_close = api.sequence());
                return ReadStatus::Complete;
            }
        }
    }

    let dib_ok = |f: u32| has(f) || api.available(f);
    let candidates = [
        (ids.png, ImageSource::Png, has(ids.png)),
        (CF_DIBV5, ImageSource::Dib, dib_ok(CF_DIBV5)),
        (CF_DIB, ImageSource::Dib, dib_ok(CF_DIB) || has(CF_BITMAP)),
    ];
    let image_wanted = cfg.types.image && candidates.iter().any(|c| c.2);

    let mut text_ok = false;
    let has_text = has(CF_UNICODETEXT) || has(CF_TEXT) || has(CF_OEMTEXT) || api.available(CF_UNICODETEXT);
    if has_text {
        if !p.phase() {
            return ReadStatus::Cancelled;
        }
        let mut got: Option<(String, bool)> = None;
        api.read_with(CF_UNICODETEXT, &mut |b| {
            let units = b.len() / 2;
            let take = units.min(TEXT_READ_UNITS);
            let v: Vec<u16> = b.chunks_exact(2).take(take).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            got = Some(formats::decode_utf16_text(&v, units > take));
        });
        if let Some((text, cut)) = got {
            if !text.trim().is_empty() {
                text_ok = true;
                p.put(|r| {
                    r.text = Some(text);
                    r.text_cut = cut;
                });
            }
        }
    }
    // HTML goes with text (rich copy) and with an image (browser "Copy image": `<img>` +
    // SourceURL); RTF only with text.
    for (fmt, is_html) in [(ids.html, true), (ids.rtf, false)] {
        let wanted = text_ok || (is_html && image_wanted);
        if !wanted || !has(fmt) {
            continue;
        }
        if !p.phase() {
            return ReadStatus::Cancelled;
        }
        let mut data = None;
        api.read_with(fmt, &mut |b| {
            if b.len() <= RICH_MAX_BYTES + 64 * 1024 {
                let clean = formats::clean_rtf(b);
                if !clean.is_empty() && clean.len() <= RICH_MAX_BYTES {
                    data = Some(clean.to_vec());
                }
            }
        });
        p.put(|r| if is_html { r.html = data } else { r.rtf = data });
    }

    if image_wanted {
        for (fmt, source, present) in candidates {
            if !present {
                continue;
            }
            if !p.phase() {
                return ReadStatus::Cancelled;
            }
            let mut copied: Option<Vec<u8>> = None;
            let mut too_large = None;
            api.read_with(fmt, &mut |b| {
                let dims = match source {
                    ImageSource::Png => formats::png_dimensions(b),
                    ImageSource::Dib => formats::dib_dimensions(b),
                };
                let Some((w, h)) = dims else { return };
                if u64::from(w) * u64::from(h) > IMAGE_MAX_PIXELS {
                    too_large = Some((w, h));
                    return;
                }
                // A PNG is copied whole (a 16-bit source exceeds the 8-bit size), unless it is
                // absurdly larger than its pixels; a DIB only up to header + 32-bpp pixels
                // (anything after them — e.g. an ICC profile — is not needed), whatever
                // `GlobalSize` claims.
                copied = match source {
                    ImageSource::Png if b.len() > formats::png_copy_cap(w, h) => return,
                    ImageSource::Png => Some(b.to_vec()),
                    ImageSource::Dib => Some(b[..b.len().min(formats::dib_copy_cap(w, h))].to_vec()),
                };
            });
            if let Some(dims) = too_large {
                p.put(|r| r.image_too_large = Some(dims));
                break;
            }
            if let Some(bytes) = copied {
                p.put(|r| r.image = Some((source, bytes)));
                break;
            }
            // Unreadable / malformed: try the next representation.
        }
    }

    p.put(|r| r.seq_close = api.sequence());
    ReadStatus::Complete
}

/// Result of [`snapshot_with_deadline`].
#[derive(Debug)]
pub enum SnapshotOutcome {
    Snapshot(RawSnapshot),
    /// Could not open the clipboard (or a stuck reader still holds it): retry later.
    Busy,
    /// Not captured; `seq` = sequence number seen (0 if unknown).
    Skipped { reason: SkipReason, seq: u32 },
}

/// Run one snapshot on a fresh reader thread and wait with per-format and total deadlines
/// (module docs). `reader_busy` stays `true` while any reader thread is alive.
pub fn snapshot_with_deadline<C: ClipboardApi>(
    api: &Arc<C>,
    state: &Arc<CaptureState>,
    cfg: &Arc<CaptureConfig>,
    t: &Timing,
    reader_busy: &Arc<AtomicBool>,
) -> SnapshotOutcome {
    if reader_busy.swap(true, Ordering::SeqCst) {
        return SnapshotOutcome::Busy;
    }
    let shared = Arc::new(ReaderShared::default());
    let (tx, rx) = mpsc::channel::<ReaderEvent>();
    let spawned = {
        let (api, state, cfg, t, shared, busy) =
            (api.clone(), state.clone(), cfg.clone(), t.clone(), shared.clone(), reader_busy.clone());
        std::thread::Builder::new().name("liqlip-clip-reader".into()).spawn(move || {
            let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let p = Progress { shared: &shared, tx: &tx };
                read_snapshot(&*api, &state, &cfg, &t, &p)
            }))
            .unwrap_or(ReadStatus::Cancelled);
            busy.store(false, Ordering::SeqCst);
            let _ = tx.send(ReaderEvent::Done(status));
        })
    };
    if spawned.is_err() {
        reader_busy.store(false, Ordering::SeqCst);
        return SnapshotOutcome::Busy;
    }

    let start = Instant::now();
    let mut phase_start = start;
    let status = loop {
        let deadline = (phase_start + t.format_timeout).min(start + t.budget);
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(ReaderEvent::Phase(at)) => phase_start = at,
            Ok(ReaderEvent::Done(s)) => break Some(s),
            Err(RecvTimeoutError::Timeout) => {
                shared.cancel.store(true, Ordering::SeqCst);
                break None;
            }
            Err(RecvTimeoutError::Disconnected) => break Some(ReadStatus::Cancelled),
        }
    };
    let mut raw = std::mem::take(&mut *shared.partial.lock());
    let seq = raw.seq_open;
    match status {
        Some(ReadStatus::Complete) => SnapshotOutcome::Snapshot(raw),
        Some(ReadStatus::Busy) => SnapshotOutcome::Busy,
        Some(ReadStatus::Skipped(reason)) => SnapshotOutcome::Skipped { reason, seq },
        Some(ReadStatus::Cancelled) => SnapshotOutcome::Skipped { reason: SkipReason::Invalid, seq },
        None => {
            // Timed out: keep what was read, but only behind completed privacy checks.
            raw.incomplete = true;
            if raw.markers_checked && raw.has_content() {
                SnapshotOutcome::Snapshot(raw)
            } else if seq == 0 {
                SnapshotOutcome::Busy
            } else {
                SnapshotOutcome::Skipped { reason: SkipReason::Timeout, seq }
            }
        }
    }
}

/// In-memory clipboard for tests (never the system clipboard).
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::HashMap;

    pub const IDS: FormatIds = FormatIds {
        html: 0xC001,
        rtf: 0xC002,
        png: 0xC003,
        drop_effect: 0xC004,
        exclude_monitor: 0xC005,
        can_include_history: 0xC006,
        can_upload_cloud: 0xC007,
        viewer_ignore: 0xC008,
    };

    #[derive(Default)]
    struct Inner {
        seq: u32,
        data: Vec<(u32, Vec<u8>)>,
        files: Option<Vec<String>>,
        owner: Option<OwnerInfo>,
        open: bool,
        fail_opens: u32,
        delays: HashMap<u32, Duration>,
        unresponsive: bool,
        reads: Vec<u32>,
        opens: u32,
    }

    #[derive(Default)]
    pub struct FakeClipboard {
        inner: Mutex<Inner>,
    }

    pub fn utf16z(s: &str) -> Vec<u8> {
        s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
    }

    pub fn owner(exe: &str) -> Option<OwnerInfo> {
        Some(OwnerInfo { pid: 4242, exe: Some(exe.to_lowercase()), from_foreground: false })
    }

    impl FakeClipboard {
        pub fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }
        /// Replace the content (like an app's copy): bumps the sequence number.
        pub fn set(&self, data: Vec<(u32, Vec<u8>)>, files: Option<Vec<String>>, owner: Option<OwnerInfo>) -> u32 {
            let mut i = self.inner.lock();
            i.seq += 1;
            i.data = data;
            i.files = files;
            i.owner = owner;
            i.seq
        }
        pub fn set_text(&self, s: &str) -> u32 {
            self.set(vec![(CF_UNICODETEXT, utf16z(s))], None, owner("c:\\apps\\notepad.exe"))
        }
        pub fn fail_next_opens(&self, n: u32) {
            self.inner.lock().fail_opens = n;
        }
        pub fn delay(&self, fmt: u32, d: Duration) {
            self.inner.lock().delays.insert(fmt, d);
        }
        pub fn set_unresponsive(&self, v: bool) {
            self.inner.lock().unresponsive = v;
        }
        pub fn hold_open(&self, v: bool) {
            self.inner.lock().open = v;
        }
        pub fn is_open(&self) -> bool {
            self.inner.lock().open
        }
        pub fn take_reads(&self) -> Vec<u32> {
            std::mem::take(&mut self.inner.lock().reads)
        }
        pub fn opens(&self) -> u32 {
            self.inner.lock().opens
        }
    }

    impl ClipboardApi for FakeClipboard {
        fn ids(&self) -> FormatIds {
            IDS
        }
        fn sequence(&self) -> u32 {
            self.inner.lock().seq
        }
        fn own_pid(&self) -> u32 {
            1
        }
        fn owner(&self) -> Option<OwnerInfo> {
            self.inner.lock().owner.clone()
        }
        fn owner_responsive(&self, _timeout: Duration) -> bool {
            !self.inner.lock().unresponsive
        }
        fn try_open(&self) -> bool {
            let mut i = self.inner.lock();
            if i.open {
                return false;
            }
            if i.fail_opens > 0 {
                i.fail_opens -= 1;
                return false;
            }
            i.open = true;
            i.opens += 1;
            true
        }
        fn close(&self) {
            self.inner.lock().open = false;
        }
        fn formats(&self) -> Vec<u32> {
            let i = self.inner.lock();
            let mut v: Vec<u32> = i.data.iter().map(|(f, _)| *f).collect();
            if i.files.is_some() {
                v.push(CF_HDROP);
            }
            v
        }
        fn available(&self, fmt: u32) -> bool {
            self.formats().contains(&fmt)
        }
        fn read_with(&self, fmt: u32, visit: &mut dyn FnMut(&[u8])) -> bool {
            let (bytes, delay) = {
                let mut i = self.inner.lock();
                assert!(i.open, "read while the clipboard is closed");
                i.reads.push(fmt);
                let bytes = i.data.iter().find(|(f, _)| *f == fmt).map(|(_, b)| b.clone());
                (bytes, i.delays.get(&fmt).copied())
            };
            if let Some(d) = delay {
                std::thread::sleep(d); // a slow / hung WM_RENDERFORMAT
            }
            match bytes {
                Some(b) => {
                    visit(&b);
                    true
                }
                None => false,
            }
        }
        fn read_files(&self, max: usize) -> Option<(Vec<String>, usize)> {
            let mut i = self.inner.lock();
            i.reads.push(CF_HDROP);
            let files = i.files.clone()?;
            let total = files.len();
            Some((files.into_iter().take(max).collect(), total))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{owner, utf16z, FakeClipboard, IDS};
    use super::*;
    use crate::model::PerKind;

    fn cfg() -> Arc<CaptureConfig> {
        Arc::new(CaptureConfig {
            respect_markers: true,
            ignored_apps: vec!["keepass.exe".into(), "c:\\tools\\vault.exe".into(), "bitwarden".into()],
            secret_patterns: true,
            types: PerKind::splat(true),
        })
    }

    fn fast() -> Timing {
        let ms = Duration::from_millis;
        Timing {
            open_delay: ms(2),
            format_timeout: ms(150),
            budget: ms(400),
            ..Timing::default()
        }
    }

    fn snap(cb: &Arc<FakeClipboard>, c: &Arc<CaptureConfig>) -> SnapshotOutcome {
        let state = Arc::new(CaptureState::new());
        snapshot_with_deadline(cb, &state, c, &fast(), &Arc::new(AtomicBool::new(false)))
    }

    fn snapshot(o: SnapshotOutcome) -> RawSnapshot {
        match o {
            SnapshotOutcome::Snapshot(r) => r,
            other => panic!("expected a snapshot, got {other:?}"),
        }
    }

    fn skipped(o: SnapshotOutcome) -> SkipReason {
        match o {
            SnapshotOutcome::Skipped { reason, .. } => reason,
            other => panic!("expected a skip, got {other:?}"),
        }
    }

    #[test]
    fn text_with_html_and_rtf_in_one_open() {
        let cb = FakeClipboard::new();
        cb.set(
            vec![
                (IDS.html, b"Version:0.9\r\n<b>x</b>\0".to_vec()),
                (IDS.rtf, b"{\\rtf1 x}\0".to_vec()),
                (CF_UNICODETEXT, utf16z("Привет")),
                (0xC0FF, vec![1, 2, 3]), // some app-private format: never read
            ],
            None,
            owner("C:\\Apps\\Word.exe"),
        );
        let r = snapshot(snap(&cb, &cfg()));
        assert_eq!(r.text.as_deref(), Some("Привет"));
        assert_eq!(r.rtf.as_deref(), Some(&b"{\\rtf1 x}"[..]));
        assert!(r.html.is_some());
        assert!(r.markers_checked);
        assert_eq!(r.owner.unwrap().exe.as_deref(), Some("c:\\apps\\word.exe"));
        assert_eq!(cb.opens(), 1, "exactly one OpenClipboard");
        assert!(!cb.is_open(), "closed afterwards");
        let reads = cb.take_reads();
        assert!(!reads.contains(&0xC0FF), "only the needed formats are read");
        assert_eq!(reads, vec![CF_UNICODETEXT, IDS.html, IDS.rtf]);
    }

    #[test]
    fn files_win_and_carry_drop_effect() {
        let cb = FakeClipboard::new();
        cb.set(
            vec![(IDS.drop_effect, 2u32.to_le_bytes().to_vec()), (CF_UNICODETEXT, utf16z("C:\\a.txt"))],
            Some(vec!["C:\\a.txt".into(), "C:\\B".into()]),
            owner("c:\\windows\\explorer.exe"),
        );
        let r = snapshot(snap(&cb, &cfg()));
        let f = r.files.unwrap();
        assert_eq!(f.paths, vec!["C:\\a.txt", "C:\\B"]);
        assert_eq!(f.drop_effect, 2);
        assert!(r.text.is_none(), "text is not read when files are present");
    }

    #[test]
    fn privacy_markers_skip_without_reading_content() {
        let text = (CF_UNICODETEXT, utf16z("hunter2"));
        let cases: Vec<(Vec<(u32, Vec<u8>)>, bool)> = vec![
            (vec![(IDS.exclude_monitor, vec![]), text.clone()], true),
            (vec![(IDS.viewer_ignore, vec![0]), text.clone()], true),
            (vec![(IDS.can_include_history, 0u32.to_le_bytes().to_vec()), text.clone()], true),
            (vec![(IDS.can_upload_cloud, 0u32.to_le_bytes().to_vec()), text.clone()], true),
            // Bitwarden sets only the two DWORDs.
            (
                vec![
                    (IDS.can_include_history, 0u32.to_le_bytes().to_vec()),
                    (IDS.can_upload_cloud, 0u32.to_le_bytes().to_vec()),
                    text.clone(),
                ],
                true,
            ),
            // Malformed DWORD → treated as 0.
            (vec![(IDS.can_include_history, vec![1]), text.clone()], true),
            // Explicit "include" (1) is allowed.
            (
                vec![
                    (IDS.can_include_history, 1u32.to_le_bytes().to_vec()),
                    (IDS.can_upload_cloud, 1u32.to_le_bytes().to_vec()),
                    text.clone(),
                ],
                false,
            ),
        ];
        for (i, (data, private)) in cases.into_iter().enumerate() {
            let cb = FakeClipboard::new();
            cb.set(data, None, owner("c:\\x\\keepassxc-proxy.exe"));
            let out = snap(&cb, &cfg());
            let reads = cb.take_reads();
            if private {
                assert_eq!(skipped(out), SkipReason::PrivacyMarker, "case {i}");
                assert!(!reads.contains(&CF_UNICODETEXT), "case {i}: content must not be read");
            } else {
                assert_eq!(snapshot(out).text.as_deref(), Some("hunter2"), "case {i}");
            }
        }
        // respectMarkers off → captured.
        let cb = FakeClipboard::new();
        cb.set(vec![(IDS.exclude_monitor, vec![]), text], None, None);
        let mut c = (*cfg()).clone();
        c.respect_markers = false;
        assert!(snapshot(snap(&cb, &Arc::new(c))).text.is_some());
    }

    #[test]
    fn ignore_list_and_own_process() {
        for exe in ["C:\\Program Files\\KeePass\\KeePass.exe", "c:/tools/VAULT.exe", "d:\\bw\\Bitwarden.exe"] {
            let cb = FakeClipboard::new();
            cb.set(vec![(CF_UNICODETEXT, utf16z("secret"))], None, owner(exe));
            assert_eq!(skipped(snap(&cb, &cfg())), SkipReason::IgnoredApp, "{exe}");
            assert!(cb.take_reads().is_empty(), "{exe}: nothing read");
        }
        let cb = FakeClipboard::new();
        cb.set(
            vec![(CF_UNICODETEXT, utf16z("mine"))],
            None,
            Some(OwnerInfo { pid: 1, exe: Some("c:\\liqlip.exe".into()), from_foreground: false }),
        );
        assert_eq!(skipped(snap(&cb, &cfg())), SkipReason::Own);
        // No owner window + Liqlip in the foreground (`echo x | clip` while settings are
        // focused): captured, with an unknown source.
        cb.set(
            vec![(CF_UNICODETEXT, utf16z("from a console tool"))],
            None,
            Some(OwnerInfo { pid: 1, exe: Some("c:\\liqlip.exe".into()), from_foreground: true }),
        );
        let r = snapshot(snap(&cb, &cfg()));
        assert_eq!(r.text.as_deref(), Some("from a console tool"));
        assert!(r.owner.is_none());
        // Own sequence number.
        let cb = FakeClipboard::new();
        let seq = cb.set_text("pasted by liqlip");
        let state = Arc::new(CaptureState::new());
        state.mark_own(seq);
        let out = snapshot_with_deadline(&cb, &state, &cfg(), &fast(), &Arc::new(AtomicBool::new(false)));
        assert_eq!(skipped(out), SkipReason::Own);
    }

    #[test]
    fn ignore_matching_rules() {
        let list = vec!["KeePass.exe".to_string(), "C:\\Tools\\Vault.exe".into(), "1password".into()];
        assert!(is_ignored("c:\\program files\\keepass password safe 2\\keepass.exe", &list));
        assert!(is_ignored("c:\\tools\\vault.exe", &list));
        assert!(is_ignored("c:\\users\\я\\appdata\\local\\1password\\app\\1password.exe", &list));
        assert!(!is_ignored("d:\\other\\vault.exe", &list));
        assert!(!is_ignored("c:\\apps\\keepassxc.exe", &list));
        assert!(!is_ignored("c:\\apps\\notepad.exe", &[]));
        assert!(!is_ignored("c:\\apps\\notepad.exe", &["  ".to_string()]));
    }

    #[test]
    fn busy_clipboard_is_retried_then_reported() {
        let cb = FakeClipboard::new();
        cb.set_text("hello");
        cb.fail_next_opens(3);
        assert_eq!(snapshot(snap(&cb, &cfg())).text.as_deref(), Some("hello"));
        cb.fail_next_opens(100);
        assert!(matches!(snap(&cb, &cfg()), SnapshotOutcome::Busy));
    }

    #[test]
    fn image_priority_and_limits() {
        let png = super::super::imaging::encode_png(1, 1, &[1, 2, 3, 255]).unwrap();
        let mut dib = vec![0u8; 44];
        dib[..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1i32.to_le_bytes());
        let cb = FakeClipboard::new();
        cb.set(vec![(CF_DIB, dib.clone()), (CF_DIBV5, dib.clone()), (IDS.png, png.clone())], None, None);
        let r = snapshot(snap(&cb, &cfg()));
        assert_eq!(r.image.as_ref().map(|(s, _)| *s), Some(ImageSource::Png));
        assert_eq!(cb.take_reads(), vec![IDS.png]);
        // Broken PNG → falls back to DIBV5.
        cb.set(vec![(CF_DIB, dib.clone()), (CF_DIBV5, dib.clone()), (IDS.png, b"junk".to_vec())], None, None);
        let r = snapshot(snap(&cb, &cfg()));
        assert_eq!(r.image.as_ref().map(|(s, _)| *s), Some(ImageSource::Dib));
        assert_eq!(cb.take_reads(), vec![IDS.png, CF_DIBV5]);
        // > 64 MP: noted, not copied.
        let mut huge = dib.clone();
        huge[4..8].copy_from_slice(&10_000i32.to_le_bytes());
        huge[8..12].copy_from_slice(&10_000i32.to_le_bytes());
        cb.set(vec![(CF_DIB, huge)], None, None);
        let r = snapshot(snap(&cb, &cfg()));
        assert!(r.image.is_none());
        assert_eq!(r.image_too_large, Some((10_000, 10_000)));
        cb.take_reads();
        // Image type disabled → not read at all.
        cb.set(vec![(IDS.png, png)], None, None);
        let mut c = (*cfg()).clone();
        c.types.image = false;
        let r = snapshot(snap(&cb, &Arc::new(c)));
        assert!(r.image.is_none());
        assert!(cb.take_reads().is_empty());
    }

    #[test]
    fn copy_image_keeps_its_html_but_no_rtf() {
        let png = super::super::imaging::encode_png(1, 1, &[1, 2, 3, 255]).unwrap();
        let cb = FakeClipboard::new();
        cb.set(
            vec![
                (IDS.html, b"Version:0.9\r\n<img src=a.png>".to_vec()),
                (IDS.rtf, b"{\\rtf1}".to_vec()),
                (IDS.png, png),
            ],
            None,
            owner("c:\\apps\\chrome.exe"),
        );
        let r = snapshot(snap(&cb, &cfg()));
        assert!(r.image.is_some() && r.html.is_some() && r.rtf.is_none());
        assert_eq!(cb.take_reads(), vec![IDS.html, IDS.png]);
    }

    #[test]
    fn huge_text_is_cut_at_the_read_limit_and_rich_formats_capped() {
        let cb = FakeClipboard::new();
        let big = "a".repeat(TEXT_READ_UNITS + 10);
        let mut big_rtf = vec![b'x'; RICH_MAX_BYTES + 1];
        big_rtf.push(0);
        cb.set(vec![(CF_UNICODETEXT, utf16z(&big)), (IDS.rtf, big_rtf)], None, None);
        let r = snapshot(snap(&cb, &cfg()));
        assert_eq!(r.text.as_ref().unwrap().len(), TEXT_READ_UNITS);
        assert!(r.text_cut);
        assert!(r.rtf.is_none(), "RTF over 2 MB is dropped, not truncated");
    }

    #[test]
    fn whitespace_only_text_is_not_content() {
        let cb = FakeClipboard::new();
        cb.set(vec![(CF_UNICODETEXT, utf16z("  \r\n\t "))], None, None);
        let r = snapshot(snap(&cb, &cfg()));
        assert!(!r.has_content());
    }

    #[test]
    fn hung_format_is_abandoned_and_the_rest_kept() {
        let cb = FakeClipboard::new();
        cb.set(
            vec![(CF_UNICODETEXT, utf16z("visible text")), (IDS.html, b"Version:0.9\r\n".to_vec())],
            None,
            owner("c:\\apps\\excel.exe"),
        );
        cb.delay(IDS.html, Duration::from_millis(700)); // owner "renders" far beyond the timeout
        let busy = Arc::new(AtomicBool::new(false));
        let state = Arc::new(CaptureState::new());
        let started = Instant::now();
        let out = snapshot_with_deadline(&cb, &state, &cfg(), &fast(), &busy);
        let waited = started.elapsed();
        assert!(waited < Duration::from_millis(450), "waited {waited:?}");
        let r = snapshot(out);
        assert!(r.incomplete);
        assert_eq!(r.text.as_deref(), Some("visible text"));
        assert!(r.html.is_none(), "the hung format is given up");
        // While the stuck reader still holds the clipboard no new snapshot starts.
        assert!(busy.load(Ordering::SeqCst));
        assert!(matches!(
            snapshot_with_deadline(&cb, &state, &cfg(), &fast(), &busy),
            SnapshotOutcome::Busy
        ));
        // When the owner finally answers, the reader closes the clipboard and exits.
        let deadline = Instant::now() + Duration::from_secs(3);
        while busy.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!busy.load(Ordering::SeqCst));
        assert!(!cb.is_open());
    }

    #[test]
    fn hung_privacy_marker_never_leaks_content() {
        let cb = FakeClipboard::new();
        cb.set(
            vec![(IDS.can_include_history, 1u32.to_le_bytes().to_vec()), (CF_UNICODETEXT, utf16z("pw"))],
            None,
            None,
        );
        cb.delay(IDS.can_include_history, Duration::from_millis(500));
        assert_eq!(skipped(snap(&cb, &cfg())), SkipReason::Timeout);
        std::thread::sleep(Duration::from_millis(600));
        assert!(!cb.is_open());
    }
}
