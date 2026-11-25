//! Capture loop + worker (ARCHITECTURE §5), generic over the clipboard seam and the output
//! sink so the whole pipeline — debounce, dedupe, pause / skip-next / self-copy, retries,
//! timeouts, classification, secrets, type toggles, image encoding, hashing — runs in unit
//! tests on an in-memory clipboard, including the 10 copies/s × 10 s reliability bench.
//!
//! ```text
//! listener (WM_CLIPBOARDUPDATE) ──Notify──▶ capture loop ──WorkItem──▶ worker ──▶ Store
//!                                           debounce 30 ms (≤ 120 ms)    classify, secrets,
//!                                           seq dedupe / self-copy       toggles, PNG, hash,
//!                                           pause / skip-next            insert_clip, events,
//!                                           owner probe, snapshot        stack push, icon
//!                                           (reader thread + deadlines)
//! ```
//! The listener only forwards; the loop never decodes; the worker never touches the
//! clipboard — a slow image encode cannot delay the next snapshot.

use super::classify;
use super::formats;
use super::imaging;
use super::secrets;
use super::snapshot::{self, ClipboardApi, RawFiles, RawSnapshot, SkipReason, SnapshotOutcome, Timing};
use super::state::CaptureState;
use super::CaptureConfig;
use crate::content;
use crate::logx;
use crate::model::{CaptureStatus, ClipMeta, Kind};
use crate::store::{ClipFiles, InsertOutcome, NewClip, SourceApp};
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Instant;

/// Messages to the capture loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMsg {
    /// `WM_CLIPBOARDUPDATE` arrived.
    Notify,
    Stop,
}

/// One snapshot handed to the worker.
pub struct WorkItem {
    pub raw: RawSnapshot,
    pub cfg: Arc<CaptureConfig>,
    /// Unix ms of the change.
    pub copied_at: i64,
}

/// Everything the capture loop needs besides the clipboard.
pub struct LoopDeps<C: ClipboardApi> {
    pub api: Arc<C>,
    pub state: Arc<CaptureState>,
    /// Current privacy/storage settings (read per change).
    pub config: Box<dyn Fn() -> CaptureConfig + Send>,
    /// Called when the loop changes the status itself (skip-next consumed).
    pub on_status: Box<dyn Fn(CaptureStatus) + Send>,
    pub worker: Sender<WorkItem>,
    pub timing: Timing,
    pub now_ms: fn() -> i64,
}

/// Sequence numbers already handled (events for them are duplicates).
struct Processed(VecDeque<u32>);

impl Processed {
    fn contains(&self, seq: u32) -> bool {
        self.0.contains(&seq)
    }
    fn mark(&mut self, seq: u32) {
        if seq != 0 && !self.0.contains(&seq) {
            if self.0.len() == 32 {
                self.0.pop_front();
            }
            self.0.push_back(seq);
        }
    }
}

enum Step {
    Done,
    /// Try the change with this sequence number again later.
    Retry(SkipReason, u32),
}

struct Loop<C: ClipboardApi> {
    d: LoopDeps<C>,
    processed: Processed,
    reader_busy: Arc<AtomicBool>,
    warned_no_access: bool,
}

impl<C: ClipboardApi> Loop<C> {
    fn process(&mut self) -> Step {
        let d = &self.d;
        let seq = d.api.sequence();
        if seq == 0 {
            if !self.warned_no_access {
                logx::warn("capture", "GetClipboardSequenceNumber = 0 (no WINSTA_ACCESSCLIPBOARD)");
                self.warned_no_access = true;
            }
        } else if self.processed.contains(seq) {
            return Step::Done;
        }
        if d.state.is_own(seq) {
            self.processed.mark(seq);
            return Step::Done;
        }
        let now = (d.now_ms)();
        if d.state.is_paused(now) {
            self.processed.mark(seq);
            return Step::Done;
        }
        if d.state.take_ignore_next() {
            self.processed.mark(seq);
            (d.on_status)(d.state.status(now));
            logx::warn("capture", "skipped one change (skip-next)");
            return Step::Done;
        }
        if !d.api.owner_responsive(d.timing.owner_probe) {
            return Step::Retry(SkipReason::OwnerNotResponding, seq);
        }
        let cfg = Arc::new((d.config)());
        match snapshot::snapshot_with_deadline(&d.api, &d.state, &cfg, &d.timing, &self.reader_busy) {
            SnapshotOutcome::Busy => Step::Retry(SkipReason::Busy, seq),
            SnapshotOutcome::Skipped { reason, seq: s } => {
                self.processed.mark(if s != 0 { s } else { seq });
                if reason != SkipReason::Own {
                    logx::warn("capture", &format!("skipped: {}", reason.as_str()));
                }
                Step::Done
            }
            SnapshotOutcome::Snapshot(raw) => {
                self.processed.mark(raw.seq_open);
                self.processed.mark(raw.seq_close);
                if raw.incomplete {
                    logx::warn("capture", "a clipboard format did not render within 1 s; it and later formats were skipped");
                }
                if let Some((w, h)) = raw.image_too_large {
                    logx::warn("capture", &format!("image {w}×{h} exceeds 64 MP; not stored"));
                }
                let _ = d.worker.send(WorkItem { raw, cfg, copied_at: now });
                Step::Done
            }
        }
    }
}

/// The capture loop. Returns on [`LoopMsg::Stop`] or when every sender is gone. The
/// clipboard content present at start-up predates this session and is not captured.
pub fn run_capture_loop<C: ClipboardApi>(rx: &Receiver<LoopMsg>, deps: LoopDeps<C>) {
    let mut lp = Loop {
        processed: Processed(VecDeque::new()),
        reader_busy: Arc::new(AtomicBool::new(false)),
        warned_no_access: false,
        d: deps,
    };
    lp.processed.mark(lp.d.api.sequence());
    // (first event time, due time) of the pending debounce; (due, attempt) of a retry.
    let mut pending: Option<(Instant, Instant)> = None;
    let mut retry: Option<(Instant, usize)> = None;
    loop {
        let next = [pending.map(|p| p.1), retry.map(|r| r.0)].into_iter().flatten().min();
        let msg = match next {
            Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(LoopMsg::Notify) => {
                let now = Instant::now();
                let t = &lp.d.timing;
                pending = Some(match pending {
                    None => (now, now + t.debounce),
                    Some((first, _)) => (first, (now + t.debounce).min(first + t.debounce_max)),
                });
                // No `continue`: under a flood of events the max-wait still fires below.
            }
            Ok(LoopMsg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let now = Instant::now();
        let attempt = if pending.is_some_and(|p| p.1 <= now) {
            pending = None;
            retry = None; // a fresh change restarts the retry ladder
            0
        } else if let Some((_, attempt)) = retry.filter(|r| r.0 <= now) {
            retry = None;
            attempt
        } else {
            continue;
        };
        let step = catch_unwind(AssertUnwindSafe(|| lp.process())).unwrap_or_else(|_| {
            logx::warn("capture", "snapshot panicked; change skipped");
            Step::Done
        });
        if let Step::Retry(reason, seq) = step {
            match lp.d.timing.retry_backoff.get(attempt) {
                Some(delay) => retry = Some((Instant::now() + *delay, attempt + 1)),
                None => {
                    // Give up on THAT change only; a newer one has its own event.
                    lp.processed.mark(seq);
                    logx::warn("capture", &format!("skipped after retries: {}", reason.as_str()));
                }
            }
        }
    }
}

// ── Worker ──────────────────────────────────────────────────────────────────

/// Where finished records go (`AppSink` → Store + events in the app; a collector in tests).
pub trait Sink {
    fn insert(&mut self, clip: NewClip) -> Result<InsertOutcome, String>;
    /// After a successful insert: emit `clips:changed`, push to the paste stack in collect
    /// mode, extract the source app icon if the store asks for it.
    fn inserted(&mut self, outcome: &InsertOutcome, exe: Option<&str>, collect: bool);
    /// Display name of a source exe (FileDescription, cached).
    fn app_name(&mut self, exe: &str) -> String {
        default_app_name(exe)
    }
}

/// File name without `.exe` — the fallback display name of an app.
pub fn default_app_name(exe: &str) -> String {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    let stem = match name.len().checked_sub(4) {
        Some(i) if name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(".exe") => &name[..i],
        _ => name,
    };
    if stem.is_empty() { name.to_string() } else { stem.to_string() }
}

/// The worker loop: returns when the capture loop is gone.
pub fn run_worker<S: Sink>(rx: &Receiver<WorkItem>, sink: &mut S, state: &CaptureState) {
    while let Ok(item) = rx.recv() {
        let r = catch_unwind(AssertUnwindSafe(|| process_item(item, sink, state)));
        if r.is_err() {
            logx::warn("capture", "worker panicked on one record; skipped");
        }
    }
}

fn process_item<S: Sink>(item: WorkItem, sink: &mut S, state: &CaptureState) {
    // Late self-copy check: a paste whose `mark_own_write` lost the race with the listener.
    if state.is_own(item.raw.seq_open) {
        return;
    }
    let exe = item.raw.owner.as_ref().and_then(|o| o.exe.clone());
    let started = Instant::now();
    let clip = match build_clip(item.raw, &item.cfg, item.copied_at, &mut |e| sink.app_name(e)) {
        Ok(c) => c,
        Err(reason) => {
            if reason != SkipReason::Empty {
                logx::warn("capture", &format!("skipped: {}", reason.as_str()));
            }
            return;
        }
    };
    let (kind, size) = (clip.kind, clip.size);
    match sink.insert(clip) {
        Ok(outcome) => {
            sink.inserted(&outcome, exe.as_deref(), state.collect());
            let ms = started.elapsed().as_millis();
            if ms > 250 {
                logx::warn("capture", &format!("slow record: {} {size} B in {ms} ms", kind.as_str()));
            }
        }
        Err(e) => logx::warn("capture", &format!("insert failed ({} {size} B): {e}", kind.as_str())),
    }
}

/// Turn a raw snapshot into a [`NewClip`] (ARCHITECTURE §5 worker steps): primary kind,
/// classification → secrets → type toggles, truncation, hash (`content.rs` only), preview,
/// metrics, PNG/thumbnail, formats, size (only Liqlip's own bytes).
pub fn build_clip(
    raw: RawSnapshot,
    cfg: &CaptureConfig,
    copied_at: i64,
    name_of: &mut dyn FnMut(&str) -> String,
) -> Result<NewClip, SkipReason> {
    if cfg.respect_markers && !raw.markers_checked {
        return Err(SkipReason::PrivacyMarker);
    }
    let source = raw
        .owner
        .as_ref()
        .and_then(|o| o.exe.as_ref())
        .map(|exe| SourceApp { exe: exe.clone(), name: name_of(exe) });
    if let Some(files) = raw.files {
        return build_files(files, cfg, source, copied_at);
    }
    let html = raw.html.as_deref().and_then(formats::parse_cf_html);
    let text = raw.text.filter(|t| !t.trim().is_empty());
    let image_primary = match (&text, &raw.image) {
        (None, Some(_)) => true,
        // Browser "Copy image": the HTML is just an <img>; the text (alt / URL) is secondary.
        (Some(_), Some(_)) => html.as_ref().is_some_and(|h| formats::html_is_image_only(&h.fragment)),
        _ => false,
    };
    if image_primary {
        let (src, bytes) = raw.image.as_ref().expect("checked above");
        match build_image(*src, bytes, html.clone(), cfg, source.clone(), copied_at) {
            Ok(c) => return Ok(c),
            Err(e) if text.is_none() => return Err(e),
            Err(_) => {} // undecodable image: fall back to the text
        }
    }
    match text {
        Some(text) => build_text(text, raw.text_cut, html, raw.rtf, raw.image, cfg, source, copied_at),
        None if raw.image_too_large.is_some() => Err(SkipReason::TooLarge),
        None => Err(SkipReason::Empty),
    }
}

fn source_meta(meta: &mut ClipMeta, html: &Option<formats::CfHtml>) {
    if meta.url.is_none() {
        if let Some(url) = html.as_ref().and_then(|h| h.source_url.clone()) {
            meta.host = classify::url_host(&url);
            meta.url = Some(url);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_text(
    text: String,
    read_cut: bool,
    html: Option<formats::CfHtml>,
    rtf: Option<Vec<u8>>,
    image: Option<(imaging::ImageSource, Vec<u8>)>,
    cfg: &CaptureConfig,
    source: Option<SourceApp>,
    copied_at: i64,
) -> Result<NewClip, SkipReason> {
    let (stored, cut) = content::truncate_text(&text);
    let stored = stored.to_string();
    drop(text);
    let (kind, mut meta) = classify::classify_text(&stored);
    if cfg.secret_patterns && secrets::looks_like_secret(&stored) {
        return Err(SkipReason::Secret);
    }
    if !*cfg.types.get(kind) {
        return Err(SkipReason::TypeDisabled);
    }
    if cut || read_cut {
        meta.truncated = Some(true);
    }
    source_meta(&mut meta, &html);
    // Office/Excel range: the picture is kept as a "Paste as… PNG" alternative.
    let png = match image {
        Some((src, bytes)) if cfg.types.image => match imaging::decode(src, &bytes) {
            Ok(d) => imaging::encode_png(d.width, d.height, &d.rgba).ok(),
            Err(imaging::DecodeError::TooLarge(w, h)) => {
                logx::warn("capture", &format!("PNG alternative {w}×{h} exceeds 64 MP; dropped"));
                None
            }
            Err(_) => None,
        },
        _ => None,
    };
    let html = html.map(|h| h.html);
    let m = content::text_metrics(&stored);
    let size = stored.len()
        + html.as_ref().map_or(0, String::len)
        + rtf.as_ref().map_or(0, Vec::len)
        + png.as_ref().map_or(0, Vec::len);
    Ok(NewClip {
        kind,
        hash: content::content_hash(kind, content::canonical_text(&stored)),
        preview: content::make_preview(&stored),
        text: Some(stored),
        meta,
        size: size as i64,
        char_count: Some(m.chars),
        word_count: Some(m.words),
        line_count: Some(m.lines),
        width: None,
        height: None,
        file_count: None,
        png,
        thumb_png: None,
        html,
        rtf,
        files: None,
        source,
        copied_at,
    })
}

fn build_image(
    src: imaging::ImageSource,
    bytes: &[u8],
    html: Option<formats::CfHtml>,
    cfg: &CaptureConfig,
    source: Option<SourceApp>,
    copied_at: i64,
) -> Result<NewClip, SkipReason> {
    if !cfg.types.image {
        return Err(SkipReason::TypeDisabled);
    }
    let d = imaging::decode(src, bytes).map_err(|e| match e {
        imaging::DecodeError::TooLarge(w, h) => {
            logx::warn("capture", &format!("image {w}×{h} exceeds 64 MP; not stored"));
            SkipReason::TooLarge
        }
        imaging::DecodeError::Invalid(_) => SkipReason::Invalid,
    })?;
    let hash = content::image_hash(d.width, d.height, &d.rgba);
    let png = imaging::encode_png(d.width, d.height, &d.rgba).map_err(|_| SkipReason::Invalid)?;
    let thumb = imaging::thumbnail_png(&d).map_err(|_| SkipReason::Invalid)?.unwrap_or_else(|| png.clone());
    let mut meta = ClipMeta::default();
    source_meta(&mut meta, &html);
    let html = html.map(|h| h.html);
    let size = png.len() + thumb.len() + html.as_ref().map_or(0, String::len);
    Ok(NewClip {
        kind: Kind::Image,
        hash,
        preview: String::new(),
        text: None,
        meta,
        size: size as i64,
        char_count: None,
        word_count: None,
        line_count: None,
        width: Some(i64::from(d.width)),
        height: Some(i64::from(d.height)),
        file_count: None,
        png: Some(png),
        thumb_png: Some(thumb),
        html,
        rtf: None,
        files: None,
        source,
        copied_at,
    })
}

fn build_files(
    files: RawFiles,
    cfg: &CaptureConfig,
    source: Option<SourceApp>,
    copied_at: i64,
) -> Result<NewClip, SkipReason> {
    if !cfg.types.file {
        return Err(SkipReason::TypeDisabled);
    }
    if files.total > files.paths.len() {
        logx::warn(
            "capture",
            &format!("file copy of {} paths: first {} kept", files.total, files.paths.len()),
        );
    }
    let paths = files.paths;
    let first = paths.first().ok_or(SkipReason::Empty)?;
    let name = std::path::Path::new(first)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| first.clone());
    // No file-system access here (a UNC path to an offline server could block): the
    // extension comes from the name only.
    let ext = std::path::Path::new(first)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty());
    let text = paths.join("\n");
    Ok(NewClip {
        kind: Kind::File,
        hash: content::content_hash(Kind::File, &content::canonical_files(&paths)),
        preview: content::make_preview(&name),
        size: text.len() as i64,
        text: Some(text),
        meta: ClipMeta { ext, ..Default::default() },
        char_count: None,
        word_count: None,
        line_count: None,
        width: None,
        height: None,
        file_count: Some(paths.len() as i64),
        png: None,
        thumb_png: None,
        html: None,
        rtf: None,
        files: Some(ClipFiles { paths, drop_effect: files.drop_effect }),
        source,
        copied_at,
    })
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::fake::{owner, utf16z, FakeClipboard, IDS};
    use super::super::snapshot::{OwnerInfo, CF_DIB, CF_UNICODETEXT};
    use super::*;
    use crate::model::PerKind;
    use parking_lot::Mutex;
    use std::sync::mpsc;
    use std::time::Duration;

    fn cfg() -> CaptureConfig {
        CaptureConfig {
            respect_markers: true,
            ignored_apps: vec!["keepass.exe".into()],
            secret_patterns: true,
            types: PerKind::splat(true),
        }
    }

    fn raw_text(t: &str) -> RawSnapshot {
        RawSnapshot {
            seq_open: 1,
            seq_close: 1,
            owner: owner("C:\\Apps\\Code.exe"),
            markers_checked: true,
            text: Some(t.to_string()),
            ..Default::default()
        }
    }

    /// CF_HTML payload without context (`StartHTML:-1`), correct zero-padded offsets.
    fn html_payload(frag: &str, source: Option<&str>) -> Vec<u8> {
        let src = source.map(|s| format!("SourceURL:{s}\r\n")).unwrap_or_default();
        let head = |a: usize, b: usize| {
            format!("Version:0.9\r\nStartHTML:-1\r\nEndHTML:-1\r\nStartFragment:{a:010}\r\nEndFragment:{b:010}\r\n{src}")
        };
        let start = head(0, 0).len();
        format!("{}{frag}", head(start, start + frag.len())).into_bytes()
    }

    fn build(raw: RawSnapshot, c: &CaptureConfig) -> Result<NewClip, SkipReason> {
        build_clip(raw, c, 1_700_000_000_000, &mut default_app_name)
    }

    #[test]
    fn text_record_fields() {
        let c = build(raw_text("Привет, мир\nвторая строка"), &cfg()).unwrap();
        assert_eq!(c.kind, Kind::Text);
        assert_eq!(c.hash, content::content_hash(Kind::Text, "Привет, мир\nвторая строка".as_bytes()));
        assert_eq!(c.preview, "Привет, мир\nвторая строка");
        assert_eq!((c.char_count, c.word_count, c.line_count), (Some(25), Some(4), Some(2)));
        assert_eq!(c.size, "Привет, мир\nвторая строка".len() as i64);
        assert_eq!(c.source, Some(SourceApp { exe: "c:\\apps\\code.exe".into(), name: "code".into() }));
        assert_eq!(c.copied_at, 1_700_000_000_000);
        assert!(c.png.is_none() && c.thumb_png.is_none() && c.files.is_none());
    }

    #[test]
    fn link_color_and_source_url_meta() {
        let c = build(raw_text("https://github.com/x"), &cfg()).unwrap();
        assert_eq!(c.kind, Kind::Link);
        assert_eq!(c.meta.host.as_deref(), Some("github.com"));
        let c = build(raw_text("#FF5733"), &cfg()).unwrap();
        assert_eq!(c.kind, Kind::Color);
        // Rich copy: SourceURL of the HTML becomes meta.url of a text record.
        let mut raw = raw_text("Some quoted paragraph");
        raw.html = Some(html_payload("<p>Some quoted paragraph</p>", Some("https://news.example.org/a")));
        let c = build(raw, &cfg()).unwrap();
        assert_eq!(c.meta.url.as_deref(), Some("https://news.example.org/a"));
        assert_eq!(c.meta.host.as_deref(), Some("news.example.org"));
        assert!(c.html.is_some());
    }

    #[test]
    fn secrets_and_type_toggles() {
        assert_eq!(build(raw_text("4111 1111 1111 1111"), &cfg()).unwrap_err(), SkipReason::Secret);
        assert_eq!(build(raw_text("  AKIAIOSFODNN7EXAMPLE "), &cfg()).unwrap_err(), SkipReason::Secret);
        let mut off = cfg();
        off.secret_patterns = false;
        assert!(build(raw_text("4111 1111 1111 1111"), &off).is_ok());
        let mut no_links = cfg();
        no_links.types.link = false;
        assert_eq!(build(raw_text("https://a.io"), &no_links).unwrap_err(), SkipReason::TypeDisabled);
        assert!(build(raw_text("plain"), &no_links).is_ok());
        // Unchecked markers never reach the store.
        let mut raw = raw_text("x");
        raw.markers_checked = false;
        assert_eq!(build(raw, &cfg()).unwrap_err(), SkipReason::PrivacyMarker);
    }

    #[test]
    fn truncation_flag() {
        let mut raw = raw_text("short");
        raw.text_cut = true;
        assert_eq!(build(raw, &cfg()).unwrap().meta.truncated, Some(true));
        assert_eq!(build(raw_text("short"), &cfg()).unwrap().meta.truncated, None);
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, 7, 9, 255]).collect();
        imaging::encode_png(w, h, &rgba).unwrap()
    }

    #[test]
    fn image_record_and_text_plus_image() {
        let raw = RawSnapshot {
            markers_checked: true,
            image: Some((imaging::ImageSource::Png, png_bytes(800, 400))),
            ..Default::default()
        };
        let c = build(raw, &cfg()).unwrap();
        assert_eq!(c.kind, Kind::Image);
        assert_eq!((c.width, c.height), (Some(800), Some(400)));
        assert_eq!(c.preview, "");
        assert!(c.text.is_none());
        let thumb = imaging::decode(imaging::ImageSource::Png, c.thumb_png.as_ref().unwrap()).unwrap();
        assert_eq!((thumb.width, thumb.height), (360, 180));
        assert_eq!(c.size as usize, c.png.as_ref().unwrap().len() + c.thumb_png.as_ref().unwrap().len());

        // Excel range: text primary + PNG alternative.
        let mut raw = raw_text("A\tB\n1\t2");
        raw.image = Some((imaging::ImageSource::Png, png_bytes(20, 10)));
        raw.html = Some(html_payload("<table><tr><td>A</td></tr></table>", None));
        let c = build(raw, &cfg()).unwrap();
        assert_eq!(c.kind, Kind::Text);
        assert!(c.png.is_some() && c.thumb_png.is_none());

        // Images disabled: the alternative is dropped, the text kept.
        let mut no_img = cfg();
        no_img.types.image = false;
        let mut raw = raw_text("A\tB");
        raw.image = Some((imaging::ImageSource::Png, png_bytes(2, 2)));
        assert!(build(raw, &no_img).unwrap().png.is_none());

        // Browser "Copy image": HTML is only an <img> → the image is primary.
        let mut raw = raw_text("https://a.io/cat.png");
        raw.html = Some(html_payload(r#"<img src="https://a.io/cat.png">"#, Some("https://a.io/")));
        raw.image = Some((imaging::ImageSource::Png, png_bytes(4, 4)));
        assert_eq!(build(raw, &cfg()).unwrap().kind, Kind::Image);
    }

    #[test]
    fn identical_pixels_dedupe_across_formats() {
        let rgba = vec![10u8, 20, 30, 255];
        let png = imaging::encode_png(1, 1, &rgba).unwrap();
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&[0u8; 24]);
        dib.extend_from_slice(&[30, 20, 10, 0]);
        let a = RawSnapshot { markers_checked: true, image: Some((imaging::ImageSource::Png, png)), ..Default::default() };
        let b = RawSnapshot { markers_checked: true, image: Some((imaging::ImageSource::Dib, dib)), ..Default::default() };
        assert_eq!(build(a, &cfg()).unwrap().hash, build(b, &cfg()).unwrap().hash);
    }

    #[test]
    fn file_record() {
        let raw = RawSnapshot {
            markers_checked: true,
            owner: owner("C:\\Windows\\explorer.exe"),
            files: Some(RawFiles {
                paths: vec!["C:\\Docs\\Report.PDF".into(), "C:\\Docs\\folder".into()],
                total: 2,
                drop_effect: 5,
            }),
            ..Default::default()
        };
        let c = build(raw, &cfg()).unwrap();
        assert_eq!(c.kind, Kind::File);
        assert_eq!(c.preview, "Report.PDF");
        assert_eq!(c.meta.ext.as_deref(), Some("pdf"));
        assert_eq!(c.file_count, Some(2));
        assert_eq!(c.text.as_deref(), Some("C:\\Docs\\Report.PDF\nC:\\Docs\\folder"));
        assert_eq!(c.size, "C:\\Docs\\Report.PDF\nC:\\Docs\\folder".len() as i64, "own bytes only");
        assert_eq!(c.files.as_ref().unwrap().drop_effect, 5);
        assert_eq!(
            c.hash,
            content::content_hash(Kind::File, &content::canonical_files(&c.files.as_ref().unwrap().paths))
        );
        let mut no_files = cfg();
        no_files.types.file = false;
        let raw = RawSnapshot {
            markers_checked: true,
            files: Some(RawFiles { paths: vec!["C:\\a".into()], total: 1, drop_effect: 1 }),
            ..Default::default()
        };
        assert_eq!(build(raw, &no_files).unwrap_err(), SkipReason::TypeDisabled);
    }

    #[test]
    fn empty_and_too_large() {
        let raw = RawSnapshot { markers_checked: true, ..Default::default() };
        assert_eq!(build(raw, &cfg()).unwrap_err(), SkipReason::Empty);
        let raw = RawSnapshot { markers_checked: true, image_too_large: Some((9000, 9000)), ..Default::default() };
        assert_eq!(build(raw, &cfg()).unwrap_err(), SkipReason::TooLarge);
        assert_eq!(default_app_name("c:\\x\\WINWORD.EXE"), "WINWORD");
        assert_eq!(default_app_name("c:\\x\\tool"), "tool");
    }

    // ── Pipeline harness (in-memory clipboard, collecting sink) ─────────────

    #[derive(Clone, Default)]
    struct Collected(Arc<Mutex<Vec<NewClip>>>, Arc<Mutex<Vec<Instant>>>);

    struct TestSink {
        out: Collected,
        pushed: Arc<Mutex<Vec<i64>>>,
        next: i64,
    }

    impl Sink for TestSink {
        fn insert(&mut self, clip: NewClip) -> Result<InsertOutcome, String> {
            self.out.1.lock().push(Instant::now());
            self.out.0.lock().push(clip);
            self.next += 1;
            Ok(InsertOutcome { id: self.next, created: true, app_id: None, app_icon_missing: false })
        }
        fn inserted(&mut self, outcome: &InsertOutcome, _exe: Option<&str>, collect: bool) {
            if collect {
                self.pushed.lock().push(outcome.id);
            }
        }
    }

    struct Harness {
        cb: Arc<FakeClipboard>,
        state: Arc<CaptureState>,
        tx: mpsc::Sender<LoopMsg>,
        out: Collected,
        pushed: Arc<Mutex<Vec<i64>>>,
        statuses: Arc<Mutex<Vec<CaptureStatus>>>,
        threads: Vec<std::thread::JoinHandle<()>>,
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn harness(timing: Timing) -> Harness {
        let cb = FakeClipboard::new();
        let state = Arc::new(CaptureState::new());
        let (tx, rx) = mpsc::channel();
        let (wtx, wrx) = mpsc::channel();
        let out = Collected::default();
        let pushed = Arc::new(Mutex::new(Vec::new()));
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let deps = LoopDeps {
            api: cb.clone(),
            state: state.clone(),
            config: Box::new(cfg),
            on_status: Box::new({
                let s = statuses.clone();
                move |st| s.lock().push(st)
            }),
            worker: wtx,
            timing,
            now_ms,
        };
        let t1 = std::thread::spawn(move || run_capture_loop(&rx, deps));
        let t2 = std::thread::spawn({
            let (out, pushed, state) = (out.clone(), pushed.clone(), state.clone());
            move || {
                let mut sink = TestSink { out, pushed, next: 0 };
                run_worker(&wrx, &mut sink, &state)
            }
        });
        // Let the loop seed its "already processed" sequence number (the content present at
        // start-up is never captured) before the test starts copying.
        std::thread::sleep(Duration::from_millis(50));
        Harness { cb, state, tx, out, pushed, statuses, threads: vec![t1, t2] }
    }

    impl Harness {
        fn copy_text(&self, t: &str) -> u32 {
            let seq = self.cb.set_text(t);
            self.tx.send(LoopMsg::Notify).unwrap();
            seq
        }
        fn wait_for(&self, n: usize, within: Duration) -> Vec<NewClip> {
            let end = Instant::now() + within;
            while self.out.0.lock().len() < n && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(5));
            }
            self.out.0.lock().clone()
        }
        fn texts(&self) -> Vec<String> {
            self.out.0.lock().iter().filter_map(|c| c.text.clone()).collect()
        }
        fn stop(self) {
            let _ = self.tx.send(LoopMsg::Stop);
            for t in self.threads {
                t.join().unwrap();
            }
        }
    }

    #[test]
    fn startup_content_is_not_captured_and_duplicate_events_collapse() {
        let h = harness(Timing::default());
        std::thread::sleep(Duration::from_millis(20));
        // Pre-existing content: an event without a new sequence number is a duplicate.
        h.tx.send(LoopMsg::Notify).unwrap();
        std::thread::sleep(Duration::from_millis(80));
        assert!(h.texts().is_empty());
        h.copy_text("one");
        for _ in 0..5 {
            h.tx.send(LoopMsg::Notify).unwrap();
        }
        h.wait_for(1, Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(h.texts(), vec!["one"]);
        h.stop();
    }

    /// Contract with paste (W1-C): `mark_own_write(seq)` after its `CloseClipboard` — also after
    /// a partially failed write — skips exactly that update, never the next one, even when the
    /// next copy follows immediately (inside the same debounce window).
    #[test]
    fn own_write_skips_exactly_its_sequence_number() {
        let h = harness(Timing::default());
        // A (partial) Liqlip write, marked, and an external copy right after it.
        let own = h.cb.set_text("written by liqlip (maybe only CF_UNICODETEXT made it)");
        h.state.mark_own(own);
        h.tx.send(LoopMsg::Notify).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let next = h.copy_text("external copy right after");
        assert_eq!(next, own + 1);
        h.wait_for(1, Duration::from_secs(2));
        // Another own write, then a slower external copy.
        let own2 = h.cb.set_text("liqlip again");
        h.state.mark_own(own2);
        h.tx.send(LoopMsg::Notify).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        h.copy_text("external later");
        h.wait_for(2, Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(h.texts(), vec!["external copy right after", "external later"]);
        h.stop();
    }

    #[test]
    fn event_flood_still_captures_within_the_max_wait() {
        let h = harness(Timing::default());
        let started = Instant::now();
        let mut i = 0;
        // An app rewriting the clipboard every ~5 ms for 400 ms: the 30 ms debounce never
        // goes quiet, the 120 ms max-wait still snapshots.
        while started.elapsed() < Duration::from_millis(400) {
            h.copy_text(&format!("burst {i}"));
            i += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
        let during = h.out.0.lock().len();
        assert!(during >= 2, "only {during} snapshots during the flood");
        // The final content is captured once the flood stops.
        let last = format!("burst {}", i - 1);
        let end = Instant::now() + Duration::from_secs(2);
        while !h.texts().contains(&last) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(h.texts().contains(&last));
        h.stop();
    }

    #[test]
    fn self_copy_pause_and_skip_next() {
        let h = harness(Timing::default());
        // Self-copy: the paste writer marks its sequence number first.
        let seq = h.cb.set_text("written by liqlip");
        h.state.mark_own(seq);
        h.tx.send(LoopMsg::Notify).unwrap();
        std::thread::sleep(Duration::from_millis(120));
        // Paused: nothing recorded.
        h.state.pause(None, now_ms()).unwrap();
        h.copy_text("while paused");
        std::thread::sleep(Duration::from_millis(120));
        h.state.resume();
        // Skip-next: exactly one change skipped, status emitted.
        h.state.set_ignore_next();
        h.copy_text("skipped once");
        std::thread::sleep(Duration::from_millis(120));
        h.copy_text("recorded");
        h.wait_for(1, Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(h.texts(), vec!["recorded"]);
        assert!(!h.state.status(now_ms()).ignore_next);
        assert_eq!(h.statuses.lock().len(), 1);
        h.stop();
    }

    #[test]
    fn collect_mode_pushes_to_the_stack() {
        let h = harness(Timing::default());
        h.state.set_collect(true);
        h.copy_text("a");
        h.wait_for(1, Duration::from_secs(2));
        h.state.set_collect(false);
        std::thread::sleep(Duration::from_millis(60));
        h.copy_text("b");
        h.wait_for(2, Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*h.pushed.lock(), vec![1]);
        h.stop();
    }

    #[test]
    fn busy_clipboard_and_unresponsive_owner_are_retried() {
        let h = harness(Timing::default());
        // Another app holds the clipboard for ~400 ms (longer than 10 × 25 ms).
        h.cb.hold_open(true);
        h.copy_text("after busy");
        std::thread::sleep(Duration::from_millis(400));
        h.cb.hold_open(false);
        assert_eq!(h.wait_for(1, Duration::from_secs(3)).len(), 1);
        // The owner does not answer WM_NULL for a while, then recovers.
        h.cb.set_unresponsive(true);
        h.copy_text("after hang");
        std::thread::sleep(Duration::from_millis(300));
        h.cb.set_unresponsive(false);
        h.wait_for(2, Duration::from_secs(4));
        assert_eq!(h.texts(), vec!["after busy", "after hang"]);
        h.stop();
    }

    #[test]
    fn ignored_app_and_markers_through_the_pipeline() {
        let h = harness(Timing::default());
        h.cb.set(vec![(CF_UNICODETEXT, utf16z("pw"))], None, owner("C:\\KeePass\\KeePass.exe"));
        h.tx.send(LoopMsg::Notify).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        h.cb.set(
            vec![(IDS.can_include_history, 0u32.to_le_bytes().to_vec()), (CF_UNICODETEXT, utf16z("pw2"))],
            None,
            Some(OwnerInfo { pid: 77, exe: Some("c:\\bw\\bitwarden-like.exe".into()), from_foreground: false }),
        );
        h.tx.send(LoopMsg::Notify).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        h.copy_text("visible");
        h.wait_for(1, Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(h.texts(), vec!["visible"]);
        h.stop();
    }

    #[test]
    fn hung_delayed_render_does_not_block_the_next_copy() {
        let ms = Duration::from_millis;
        let h = harness(Timing { format_timeout: ms(200), budget: ms(500), ..Timing::default() });
        h.cb.set(
            vec![(CF_UNICODETEXT, utf16z("table")), (CF_DIB, vec![0; 64])],
            None,
            owner("c:\\apps\\excel.exe"),
        );
        h.cb.delay(CF_DIB, ms(900));
        h.tx.send(LoopMsg::Notify).unwrap();
        // The text arrives within the timeout window even though the bitmap hangs.
        let got = h.wait_for(1, Duration::from_millis(700));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text.as_deref(), Some("table"));
        // The next copy (after the stuck reader released the clipboard) is captured.
        std::thread::sleep(ms(500));
        h.cb.delay(CF_DIB, Duration::ZERO);
        h.copy_text("next");
        h.wait_for(2, Duration::from_secs(4));
        assert_eq!(h.texts(), vec!["table", "next"]);
        h.stop();
    }

    /// Reliability bench (prompt W1-B): 10 clipboard writes per second from another thread
    /// for 10 s → every write is recorded, in order, nothing lost.
    #[test]
    fn bench_10_copies_per_second_for_10_seconds() {
        let h = harness(Timing::default());
        std::thread::sleep(Duration::from_millis(20));
        let started = Instant::now();
        let writer = {
            let (cb, tx) = (h.cb.clone(), h.tx.clone());
            std::thread::spawn(move || {
                let mut written = Vec::with_capacity(100);
                for i in 0..100 {
                    let due = started + Duration::from_millis(100 * i as u64);
                    std::thread::sleep(due.saturating_duration_since(Instant::now()));
                    cb.set_text(&format!("bench copy #{i}"));
                    written.push(Instant::now());
                    tx.send(LoopMsg::Notify).unwrap();
                }
                written
            })
        };
        let written = writer.join().unwrap();
        let got = h.wait_for(100, Duration::from_secs(3));
        let elapsed = started.elapsed();
        let texts: Vec<String> = got.iter().filter_map(|c| c.text.clone()).collect();
        let expected: Vec<String> = (0..100).map(|i| format!("bench copy #{i}")).collect();
        assert_eq!(texts, expected, "lost or reordered copies");
        let mut hashes: Vec<&str> = got.iter().map(|c| c.hash.as_str()).collect();
        hashes.sort();
        hashes.dedup();
        assert_eq!(hashes.len(), 100);
        // Copy → record latency (SPEC §7: ≤ 300 ms for text). Debounce alone is 30 ms.
        let mut lat: Vec<Duration> =
            h.out.1.lock().iter().zip(&written).map(|(at, w)| at.saturating_duration_since(*w)).collect();
        lat.sort();
        let (p50, max) = (lat[49], lat[99]);
        assert!(max < Duration::from_millis(300), "max latency {max:?}");
        eprintln!("bench: 100/100 copies recorded in {elapsed:?}; latency p50 {p50:?}, max {max:?}");
        h.stop();
    }
}
