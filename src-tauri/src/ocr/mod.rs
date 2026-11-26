//! OCR on images — Windows.Media.Ocr, background queue, search (SPEC §3.12, ARCHITECTURE §5a).
//! Owner: **W2-A** (port of Liquid Tools `ocr.rs`, `git show 0295998:src-tauri/src/ocr.rs`).
//!
//! - `engine` — the WinRT recognizer (PNG → text) and its pure helpers.
//! - `worker` — the queue loop + control block (`ocr_state = pending` rows in the store).
//! - `store/ocr.rs` — the queue's SQL (owned by W2-A inside the store).
//! - `commands` — `ocr_status`, `ocr_reindex`, `clip_ocr` (§9.6).
//!
//! Threads: `liqlip-ocr` (the queue: MTA, supervised; decode + recognition at the lowest
//! CPU priority, `engine::low_priority`) and one short `liqlip-ocr-now` thread per
//! `clip_ocr` that has to recognise. Nothing blocking runs on the UI thread or the async
//! runtime. Logs carry ids, durations and error codes only.

pub mod commands;
mod engine;
mod worker;

use crate::model::{ChangeOrigin, ClipsChange, ClipsChanged, OcrProgress, OcrState};
use crate::store::ocr::OcrWrite;
use crate::store::Store;
use crate::{events, logx, settings};
use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use worker::{Control, Recognize, Sink};

/// `clip_ocr` waits this long for a recognition it had to start (the work itself goes on
/// and its result is still stored).
const DIRECT_TIMEOUT: Duration = Duration::from_secs(10);
/// At most this many `clip_ocr` recognitions at once (different images); more → `busy`.
const DIRECT_MAX: usize = 4;

/// Managed state (`app.manage` in [`start`]).
pub(crate) struct Ocr {
    ctl: Arc<Control>,
    /// Serialises `ocr_reindex` (check → requeue → begin) so two clicks never double it.
    reindex_gate: Mutex<()>,
    direct: Arc<Direct>,
}

/// `clips:changed` / `ocr:progress` through `events.rs`.
struct AppSink(AppHandle);

impl Sink for AppSink {
    fn changed(&self, ids: Vec<i64>) {
        if ids.is_empty() {
            return;
        }
        // Batches are small (≤ a few hundred ms of work); the 1 000-id cap of the store
        // commands is never reached, but keep the contract shape anyway.
        let ids = (ids.len() <= 1000).then_some(ids);
        events::clips_changed(&self.0, &ClipsChanged { origin: ChangeOrigin::User, change: ClipsChange::Updated, ids });
    }
    fn progress(&self, p: OcrProgress) {
        events::ocr_progress(&self.0, &p);
    }
}

impl Recognize for engine::Engine {
    fn recognize(&self, png: &[u8]) -> Result<String, String> {
        self.recognize_png(png)
    }
    fn label(&self) -> String {
        if self.setting == "auto" {
            format!("auto → {}", self.tag)
        } else if self.setting.eq_ignore_ascii_case(&self.tag) {
            self.tag.clone()
        } else {
            format!("{} unavailable → {}", self.setting, self.tag)
        }
    }
}

/// `setup()` step 5 (after `store::init`): manage the OCR state, hook the store (new images
/// are queued while OCR is on and wake the queue), subscribe to `ocr.*` settings and start
/// the queue thread. Images left `pending` by a previous run are picked up right away.
pub fn start(app: &AppHandle) {
    let s = settings::get(app).ocr;
    let ctl = Arc::new(Control::new(s.enabled, &s.language));
    let Some(store) = app.try_state::<Store>() else {
        logx::warn("ocr", "start: store not managed");
        return;
    };
    let wake = ctl.clone();
    store.ocr_configure(s.enabled, Some(Arc::new(move || wake.wake())));
    app.manage(Ocr { ctl: ctl.clone(), reindex_gate: Mutex::new(()), direct: Arc::new(Direct::default()) });

    // Runs under the settings write lock: flags + a wake-up only, the queue does the work.
    settings::subscribe(app, {
        let ctl = ctl.clone();
        let app = app.clone();
        move |old, new| {
            if old.ocr == new.ocr {
                return;
            }
            if let Some(store) = app.try_state::<Store>() {
                store.ocr_set_queue_new(new.ocr.enabled);
            }
            ctl.apply_settings(new.ocr.enabled, &new.ocr.language);
        }
    });

    let app = app.clone();
    let spawned = std::thread::Builder::new().name("liqlip-ocr".into()).spawn(move || {
        let _apartment = engine::Apartment::enter();
        let mut backoff = Duration::from_secs(1);
        loop {
            let ran = catch_unwind(AssertUnwindSafe(|| {
                let Some(store) = app.try_state::<Store>() else { return };
                let sink = AppSink(app.clone());
                worker::run(&ctl, &store, make_engine, &sink, &worker::TIMING);
            }));
            match ran {
                Ok(()) => return,
                Err(_) => logx::warn("ocr", "queue panicked; restarting"),
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });
    if let Err(e) = spawned {
        logx::warn("ocr", &format!("cannot start the queue thread: {e}"));
    }
}

/// Wake the queue and let it pick up images that were never queued (`ocr_state = none`):
/// for modules that add images outside the capture insert (W2-B import). Cheap; any thread.
#[allow(dead_code)] // W2-B / coordinator: call after an import
pub fn wake(app: &AppHandle) {
    if let Some(ocr) = app.try_state::<Ocr>() {
        ocr.ctl.wake_with_catch_up();
    }
}

fn make_engine(setting: &str) -> Result<Box<dyn Recognize>, engine::Unavailable> {
    engine::Engine::create(setting).map(|e| Box::new(e) as Box<dyn Recognize>)
}

// ── commands' logic ─────────────────────────────────────────────────────────

fn store_of(app: &AppHandle) -> Result<tauri::State<'_, Store>, String> {
    app.try_state::<Store>().ok_or_else(|| "failed: store is not ready".to_string())
}

fn ocr_of(app: &AppHandle) -> Result<tauri::State<'_, Ocr>, String> {
    app.try_state::<Ocr>().ok_or_else(|| "failed: ocr is not started".to_string())
}

/// Installed OCR languages, on this (blocking) thread inside the WinRT apartment.
fn installed_languages() -> Result<Vec<String>, String> {
    let _apartment = engine::Apartment::enter();
    engine::languages()
}

/// `ocr_status` (blocking). Languages are read fresh every time — a pack installed while
/// Liqlip runs shows up on the next call.
pub(crate) fn status(app: &AppHandle) -> Result<crate::model::OcrStatus, String> {
    let languages = installed_languages()?;
    let pending = store_of(app)?.ocr_pending()?;
    Ok(crate::model::OcrStatus { available: !languages.is_empty(), languages, pending })
}

/// `ocr_reindex` (blocking): every image back to `pending`, progress through `ocr:progress`.
/// A reindex already running is not restarted (its current progress is re-sent).
pub(crate) fn reindex(app: &AppHandle) -> Result<(), String> {
    let ocr = ocr_of(app)?;
    if !ocr.ctl.enabled() {
        return Err("invalid: text recognition is turned off".into());
    }
    let _gate = ocr.reindex_gate.lock();
    if let Some(p) = ocr.ctl.reindex_progress() {
        events::ocr_progress(app, &p);
        return Ok(());
    }
    if installed_languages()?.is_empty() {
        return Err("failed: no OCR language is installed".into());
    }
    let store = store_of(app)?;
    store.ocr_requeue_all()?;
    let total = store.ocr_pending()?;
    let (_, p) = ocr.ctl.begin_reindex(total);
    logx::warn("ocr", &format!("reindex started ({total} images)"));
    events::ocr_progress(app, &p);
    Ok(())
}

/// `clip_ocr` (blocking): a `done` record answers from the database; otherwise the image is
/// recognised now, outside the queue, and the result is stored like a queue result.
pub(crate) fn clip_text(app: &AppHandle, id: i64) -> Result<String, String> {
    let store = store_of(app)?;
    let rec = store.ocr_record(id)?;
    if !rec.kind.is_image() {
        return Err(format!("invalid: clip {id} is not an image"));
    }
    if rec.state == OcrState::Done {
        if let Some(text) = rec.text {
            return Ok(text);
        }
    }
    let ocr = ocr_of(app)?;
    if !ocr.ctl.enabled() {
        return Err("invalid: text recognition is turned off".into());
    }
    let language = ocr.ctl.language();
    let app = app.clone();
    ocr.direct.run(id, DIRECT_TIMEOUT, move || {
        let _apartment = engine::Apartment::enter();
        let store = store_of(&app)?;
        let sink = AppSink(app.clone());
        recognize_direct(&store, id, || make_engine(&language), &sink)
    })
}

/// One-off recognition of a PNG that is not a record (W6-A: "Copy text from the screen",
/// ARCHITECTURE §18.4): on its own short thread inside the WinRT apartment, at normal priority
/// (the user waits for it), in the `ocr.language` language. Not gated by `ocr.enabled` — that
/// switch is about indexing the history; this is an explicit action on a picture nobody keeps.
/// Waits at most [`DIRECT_TIMEOUT`]. Nothing is stored or logged.
pub(crate) fn recognize_png(app: &AppHandle, png: Vec<u8>) -> Result<String, String> {
    let language = settings::get(app).ocr.language;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("liqlip-ocr-shot".into())
        .spawn(move || {
            let _apartment = engine::Apartment::enter();
            let result = match make_engine(&language) {
                Ok(engine) => engine.recognize(&png),
                Err(engine::Unavailable::NoLanguages) => Err("failed: no OCR language is installed".to_string()),
                Err(engine::Unavailable::Failed(e)) => Err(e),
            };
            let _ = tx.send(result);
        })
        .map_err(|e| format!("failed: {e}"))?;
    rx.recv_timeout(DIRECT_TIMEOUT).map_err(|_| "failed: text recognition timed out".to_string())?
}

/// One direct recognition: engine → PNG → text → `ocr_finish(Direct)` → event. A record
/// deleted meanwhile drops the result (`not_found`); a recognition failure marks `error`.
fn recognize_direct<F>(store: &Store, id: i64, make: F, sink: &dyn Sink) -> Result<String, String>
where
    F: FnOnce() -> Result<Box<dyn Recognize>, engine::Unavailable>,
{
    let job = store.ocr_job(id)?;
    let recognizer = make().map_err(|u| match u {
        engine::Unavailable::NoLanguages => "failed: no OCR language is installed".to_string(),
        engine::Unavailable::Failed(e) => e,
    })?;
    let started = Instant::now();
    let outcome = engine::low_priority(|| match std::fs::read(&job.png) {
        Ok(png) => recognizer.recognize(&png),
        Err(e) => Err(format!("invalid: png file: {:?}", e.kind())),
    });
    let ms = started.elapsed().as_millis();
    if let Err(e) = &outcome {
        logx::warn("ocr", &format!("clip {id} (direct) failed after {ms} ms: {}", e.chars().take(120).collect::<String>()));
    }
    match store.ocr_finish_job(&job, outcome.as_deref().map_err(|_| ()), OcrWrite::Direct) {
        Ok(true) => sink.changed(vec![id]),
        Ok(false) => return Err(format!("not_found: clip {id}")),
        Err(e) => logx::warn("ocr", &format!("clip {id} (direct) write: {e}")),
    }
    outcome
}

// ── direct recognitions: one thread per image, shared by concurrent callers ──

#[derive(Default)]
struct Slot {
    result: Mutex<Option<Result<String, String>>>,
    cv: Condvar,
}

impl Slot {
    fn set(&self, r: Result<String, String>) {
        *self.result.lock() = Some(r);
        self.cv.notify_all();
    }

    fn wait(&self, timeout: Duration) -> Option<Result<String, String>> {
        let deadline = Instant::now() + timeout;
        let mut res = self.result.lock();
        while res.is_none() {
            if self.cv.wait_until(&mut res, deadline).timed_out() {
                break;
            }
        }
        res.clone()
    }
}

/// In-flight `clip_ocr` recognitions by record id.
#[derive(Default)]
struct Direct {
    inflight: Mutex<HashMap<i64, Arc<Slot>>>,
}

impl Direct {
    /// Run `job` for `id` on its own thread (or join the one already running for `id`) and
    /// wait up to `timeout`. On timeout the job keeps going — its result is still stored.
    fn run<F>(self: &Arc<Self>, id: i64, timeout: Duration, job: F) -> Result<String, String>
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        let (slot, fresh) = {
            let mut map = self.inflight.lock();
            match map.get(&id) {
                Some(s) => (s.clone(), false),
                None => {
                    if map.len() >= DIRECT_MAX {
                        return Err("busy: text recognition is already running for other images".into());
                    }
                    let s = Arc::new(Slot::default());
                    map.insert(id, s.clone());
                    (s, true)
                }
            }
        };
        if fresh {
            let me = self.clone();
            let s = slot.clone();
            let spawned = std::thread::Builder::new().name("liqlip-ocr-now".into()).spawn(move || {
                let r = catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| Err("failed: recognition panicked".into()));
                s.set(r);
                me.inflight.lock().remove(&id);
            });
            if let Err(e) = spawned {
                self.inflight.lock().remove(&id);
                return Err(format!("failed: cannot start recognition: {e}"));
            }
        }
        slot.wait(timeout).unwrap_or_else(|| Err("failed: text recognition timed out".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::worker::tests::{Fake, Rec};
    use super::*;
    use crate::store::ocr::{STATE_DONE, STATE_ERROR};
    use crate::store::testutil::TestStore;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fake() -> Result<Box<dyn Recognize>, engine::Unavailable> {
        Ok(Box::new(Fake {
            label: "en-US".into(),
            calls: Arc::new(AtomicUsize::new(0)),
            gate: Arc::new((Mutex::new(true), Condvar::new())),
        }))
    }

    fn state(t: &TestStore, id: i64) -> (i64, Option<String>) {
        t.lock()
            .query_row("SELECT ocr_state, ocr_text FROM clips WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
    }

    #[test]
    fn direct_recognition_stores_the_result_and_reports_errors() {
        let t = TestStore::new();
        let rec = Rec::default();
        let id = t.add_image(40, 12, 1_000, false); // OCR off at capture → none
        assert_eq!(recognize_direct(&t, id, fake, &rec).unwrap(), "img-40x12 en-US");
        assert_eq!(state(&t, id), (STATE_DONE, Some("img-40x12 en-US".into())));
        assert_eq!(*rec.changed.lock(), vec![id]);

        let bad = t.add_image(13, 12, 2_000, false); // fake engine error
        assert!(recognize_direct(&t, bad, fake, &rec).unwrap_err().starts_with("failed:"));
        assert_eq!(state(&t, bad).0, STATE_ERROR);

        let none = t.add_image(41, 12, 3_000, false);
        let err = recognize_direct(&t, none, || Err(engine::Unavailable::NoLanguages), &rec).unwrap_err();
        assert!(err.starts_with("failed:"), "{err}");
        assert_eq!(state(&t, none).0, crate::store::ocr::STATE_NONE, "no language → record untouched");

        let txt = t.add_text("hello", 4_000);
        assert!(recognize_direct(&t, txt, fake, &rec).unwrap_err().starts_with("invalid:"));
        assert!(recognize_direct(&t, 9_999, fake, &rec).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn direct_runs_share_one_job_per_id_and_time_out_without_losing_it() {
        let d = Arc::new(Direct::default());
        let runs = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let job = |runs: Arc<AtomicUsize>, gate: Arc<(Mutex<bool>, Condvar)>| {
            move || {
                runs.fetch_add(1, Ordering::SeqCst);
                let (m, cv) = &*gate;
                let mut open = m.lock();
                while !*open {
                    cv.wait(&mut open);
                }
                Ok::<_, String>("text".to_string())
            }
        };
        // First caller times out while the job is blocked; the job is still in flight.
        let err = d.run(7, Duration::from_millis(30), job(runs.clone(), gate.clone())).unwrap_err();
        assert!(err.starts_with("failed:") && err.contains("timed out"), "{err}");
        // A second caller for the same id joins it instead of starting another.
        let d2 = d.clone();
        let (r2, g2) = (runs.clone(), gate.clone());
        let waiter = std::thread::spawn(move || d2.run(7, Duration::from_secs(5), job(r2, g2)));
        // Open the gate only once the second caller holds the slot (map + job + waiter = 3).
        // A fixed sleep raced with thread start-up on slow CI machines: the job finished first
        // and the late caller started a second run.
        let deadline = Instant::now() + Duration::from_secs(10);
        while d.inflight.lock().get(&7).map_or(0, Arc::strong_count) < 3 {
            assert!(Instant::now() < deadline, "the second caller never joined the job");
            std::thread::sleep(Duration::from_millis(2));
        }
        {
            let (m, cv) = &*gate;
            *m.lock() = true;
            cv.notify_all();
        }
        assert_eq!(waiter.join().unwrap().unwrap(), "text");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        // Too many different images at once → busy.
        let block = Arc::new((Mutex::new(false), Condvar::new()));
        for id in 100..100 + DIRECT_MAX as i64 {
            let _ = d.run(id, Duration::from_millis(1), job(runs.clone(), block.clone()));
        }
        let busy = d.run(999, Duration::from_millis(1), job(runs.clone(), block.clone())).unwrap_err();
        assert!(busy.starts_with("busy:"), "{busy}");
        let (m, cv) = &*block;
        *m.lock() = true;
        cv.notify_all();
    }

    /// Real Windows OCR on a picture drawn by the test (GDI text, 200×60, in memory).
    /// Runs wherever Windows has at least one OCR language; otherwise it reports and passes
    /// (the queue then shows `available: false`, which is the behaviour under test there).
    #[cfg(windows)]
    #[test]
    fn windows_ocr_reads_text_drawn_in_memory() {
        let _apartment = engine::Apartment::enter();
        let langs = engine::languages().expect("AvailableRecognizerLanguages");
        assert!(langs.iter().all(|t| !t.is_empty() && t.len() <= 35 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')));
        if langs.is_empty() {
            eprintln!("no OCR language installed — recognition not exercised");
            assert_eq!(engine::Engine::create("auto").err(), Some(engine::Unavailable::NoLanguages));
            return;
        }
        assert!(engine::max_dimension() >= 1000, "MaxImageDimension = {}", engine::max_dimension());
        let png = gdi_text_png(&[(10, 10, "Liqlip 42")], 200, 60);
        let engine = engine::Engine::create("auto").expect("engine");
        let text = engine.recognize_png(&png).expect("recognize");
        eprintln!("recognised {text:?} with {}", engine.tag);
        assert!(text.to_lowercase().contains("liqlip"), "recognised {text:?}");
        assert!(text.contains("42"), "recognised {text:?}");
        // A specific installed language works too; an unknown tag falls back to auto.
        let first = engine::Engine::create(&langs[0]).expect("engine for an installed tag");
        assert!(first.tag.eq_ignore_ascii_case(&langs[0]) || !first.tag.is_empty());
        assert!(engine::Engine::create("xx-ZZ").is_ok(), "unknown tag → auto fallback");
        // A blank image is "done" with no text, not an error.
        let blank = gdi_text_png(&[], 200, 60);
        assert_eq!(engine.recognize_png(&blank).unwrap(), "");
        // An image longer than the limit is tiled (the real limit is 10 000 px; a 1 000 px
        // limit keeps the picture small): text in the first tile, a line straddling the first
        // cut (1 000) and text in the last tile are all read — the straddling one once.
        // English engine when installed: the test text is Latin (a Cyrillic engine reads it
        // with l/I slips — the reason the language setting exists).
        let latin = langs.iter().find(|t| t.to_ascii_lowercase().starts_with("en")).map_or("auto", String::as_str);
        let tiled = engine::Engine::create(latin).expect("engine").with_max(1_000);
        let tall = gdi_text_png(&[(10, 10, "Liqlip 42"), (10, 980, "Tile 77"), (10, 1_400, "Bottom 5")], 600, 1_500);
        let text = tiled.recognize_png(&tall).expect("recognize tall");
        eprintln!("tiled: recognised {text:?}");
        assert!(text.contains("42"), "recognised {text:?}");
        assert_eq!(text.matches("77").count(), 1, "the line in the overlap is kept once: {text:?}");
        assert!(text.contains('5') && text.lines().count() == 3, "recognised {text:?}");
    }

    /// Black text on white, drawn with GDI into a 32-bpp DIB section, encoded as PNG.
    #[cfg(windows)]
    fn gdi_text_png(texts: &[(i32, i32, &str)], w: i32, h: i32) -> Vec<u8> {
        use windows::core::w;
        use windows::Win32::Foundation::COLORREF;
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush, SelectObject, SetBkMode,
            SetTextColor, TextOutW, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
            DEFAULT_CHARSET, DIB_RGB_COLORS, FW_BOLD, OUT_DEFAULT_PRECIS, TRANSPARENT,
        };
        unsafe {
            let dc = CreateCompatibleDC(None);
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).expect("CreateDIBSection");
            let len = (w * h * 4) as usize;
            std::ptr::write_bytes(bits as *mut u8, 0xFF, len); // white
            let old_bmp = SelectObject(dc, bmp.into());
            let font = CreateFontW(
                32, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY, 0, w!("Arial"),
            );
            let old_font = SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x000000));
            for (x, y, text) in texts {
                let wide: Vec<u16> = text.encode_utf16().collect();
                let _ = TextOutW(dc, *x, *y, &wide);
            }
            let _ = GdiFlush();
            let px = std::slice::from_raw_parts(bits as *const u8, len);
            let rgba: Vec<u8> = px.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
            SelectObject(dc, old_font);
            SelectObject(dc, old_bmp);
            let _ = DeleteObject(font.into());
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba).unwrap();
            let mut out = Vec::new();
            image::DynamicImage::ImageRgba8(img).write_to(std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
            out
        }
    }
}
