//! The background OCR queue (W2-A, ARCHITECTURE §5a): one thread, one image at a time,
//! newest first. The work list is the database (`ocr_state = pending`, `store/ocr.rs`); this
//! file is the loop and its control block, written against small traits so the tests drive
//! it with a fake recognizer and a real temporary store.
//!
//! Wake-ups: an image insert (store hook), `ocr.*` settings changes, `ocr_reindex`, start-up,
//! `ocr::wake` (other modules, e.g. an import) — and a periodic re-check (a language pack
//! installed while Liqlip runs is picked up without a restart).
//!
//! Invalidation: `epoch` grows when a result computed now would be wrong to keep — OCR turned
//! off, language changed, a reindex started. A result whose job began in an older epoch is
//! dropped and the record stays `pending` (it is redone under the new conditions).

use super::engine::Unavailable;
use crate::logx;
use crate::model::OcrProgress;
use crate::store::ocr::OcrWrite;
use crate::store::Store;
use parking_lot::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// A ready recognizer (the WinRT engine in the app, a fake in tests).
pub(crate) trait Recognize {
    /// Text of one PNG (`""` = no text). `Err("failed: …")` = the engine failed,
    /// `Err("invalid: …")` = the image itself is unusable.
    fn recognize(&self, png: &[u8]) -> Result<String, String>;
    /// For the log: the language actually used.
    fn label(&self) -> String;
}

/// Where results go (events in the app, a recorder in tests).
pub(crate) trait Sink {
    /// Records whose OCR text/state changed → `clips:changed { origin: 'user', change: 'updated', ids }`.
    fn changed(&self, ids: Vec<i64>);
    /// `ocr:progress` during a reindex.
    fn progress(&self, p: OcrProgress);
}

/// Pacing of the loop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    /// Queue empty / OCR off: sleep until woken, at most this long.
    pub idle: Duration,
    /// No engine (no language): re-check this often.
    pub recheck: Duration,
    /// Database error: retry after this.
    pub retry: Duration,
    /// `clips:changed` for finished records is batched over this window.
    pub batch: Duration,
    /// `ocr:progress` at most this often (the final event always goes out).
    pub progress: Duration,
}

pub(crate) const TIMING: Timing = Timing {
    idle: Duration::from_secs(300),
    recheck: Duration::from_secs(60),
    retry: Duration::from_secs(5),
    batch: Duration::from_millis(400),
    progress: Duration::from_millis(150),
};

/// After this many engine failures in a row the engine is rebuilt.
const ENGINE_FAILURES_MAX: u32 = 3;
/// A successful recognition slower than this is logged (id + duration only).
const SLOW_MS: u128 = 5_000;

#[derive(Debug, Clone, Copy)]
struct Reindex {
    done: i64,
    total: i64,
    last_emit: Option<Instant>,
}

#[derive(Debug, Default)]
struct Ctl {
    enabled: bool,
    language: String,
    /// Grows on every wake-up; the loop sleeps until it changes.
    wake: u64,
    /// Grows when an in-flight result must be dropped (see module docs).
    epoch: u64,
    /// Queue never-queued images (`none`) before the next job: at start, when OCR is turned
    /// on, on `ocr::wake`.
    catch_up: bool,
    reindex: Option<Reindex>,
    stop: bool,
}

/// What one loop iteration works with.
#[derive(Debug, Clone)]
struct Snapshot {
    enabled: bool,
    language: String,
    wake: u64,
    epoch: u64,
}

/// Shared control block of the queue (managed by `ocr/mod.rs`, read by the commands).
pub(crate) struct Control {
    st: Mutex<Ctl>,
    cv: Condvar,
}

impl Control {
    pub(crate) fn new(enabled: bool, language: &str) -> Control {
        Control {
            st: Mutex::new(Ctl { enabled, language: language.to_string(), catch_up: enabled, ..Ctl::default() }),
            cv: Condvar::new(),
        }
    }

    /// New work may be waiting (an image was inserted).
    pub(crate) fn wake(&self) {
        let mut st = self.st.lock();
        st.wake += 1;
        self.cv.notify_all();
    }

    /// Like [`Control::wake`], and also queue never-queued images (imports).
    pub(crate) fn wake_with_catch_up(&self) {
        let mut st = self.st.lock();
        st.catch_up = st.enabled;
        st.wake += 1;
        self.cv.notify_all();
    }

    /// `ocr.enabled` / `ocr.language` changed. Called from the settings subscriber: only
    /// state changes here, the loop does the work.
    pub(crate) fn apply_settings(&self, enabled: bool, language: &str) {
        let mut st = self.st.lock();
        if st.enabled == enabled && st.language == language {
            return;
        }
        if enabled && !st.enabled {
            st.catch_up = true;
        }
        st.enabled = enabled;
        st.language = language.to_string();
        st.epoch += 1;
        st.wake += 1;
        self.cv.notify_all();
    }

    pub(crate) fn enabled(&self) -> bool {
        self.st.lock().enabled
    }

    pub(crate) fn language(&self) -> String {
        self.st.lock().language.clone()
    }

    /// End the loop (tests; the app's thread lives as long as the process).
    #[cfg(test)]
    pub(crate) fn stop(&self) {
        let mut st = self.st.lock();
        st.stop = true;
        self.cv.notify_all();
    }

    /// Progress of the running reindex, if any.
    pub(crate) fn reindex_progress(&self) -> Option<OcrProgress> {
        self.st.lock().reindex.map(|r| OcrProgress { done: r.done, total: r.total })
    }

    /// Start a reindex over `total` pending images. A running reindex is NOT restarted:
    /// `(false, its progress)`. `total == 0` → `(true, {0, 0})` and nothing runs. Otherwise
    /// in-flight results are invalidated and the loop is woken: `(true, {0, total})`.
    pub(crate) fn begin_reindex(&self, total: i64) -> (bool, OcrProgress) {
        let mut st = self.st.lock();
        if let Some(r) = st.reindex {
            return (false, OcrProgress { done: r.done, total: r.total });
        }
        if total <= 0 {
            return (true, OcrProgress { done: 0, total: 0 });
        }
        st.reindex = Some(Reindex { done: 0, total, last_emit: Some(Instant::now()) });
        st.epoch += 1;
        st.wake += 1;
        self.cv.notify_all();
        (true, OcrProgress { done: 0, total })
    }

    fn snapshot(&self) -> Option<Snapshot> {
        let st = self.st.lock();
        if st.stop {
            return None;
        }
        Some(Snapshot { enabled: st.enabled, language: st.language.clone(), wake: st.wake, epoch: st.epoch })
    }

    fn take_catch_up(&self) -> bool {
        std::mem::take(&mut self.st.lock().catch_up)
    }

    fn set_catch_up(&self) {
        self.st.lock().catch_up = true;
    }

    fn epoch(&self) -> u64 {
        self.st.lock().epoch
    }

    /// Sleep until a wake-up newer than `since`, a stop, or `timeout`.
    fn wait(&self, since: u64, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut st = self.st.lock();
        while st.wake == since && !st.stop {
            if self.cv.wait_until(&mut st, deadline).timed_out() {
                break;
            }
        }
    }

    /// One job of a running reindex finished; `remaining` = pending images now. The total
    /// grows if images arrived meanwhile; the last event always reads `done == total`.
    fn after_job(&self, remaining: i64, every: Duration) -> Option<OcrProgress> {
        let mut st = self.st.lock();
        let r = st.reindex.as_mut()?;
        r.done += 1;
        r.total = r.total.max(r.done + remaining);
        if remaining <= 0 {
            let total = r.total;
            st.reindex = None;
            return Some(OcrProgress { done: total, total });
        }
        let due = r.last_emit.is_none_or(|t| t.elapsed() >= every);
        if due {
            r.last_emit = Some(Instant::now());
            return Some(OcrProgress { done: r.done, total: r.total });
        }
        None
    }

    /// Queue empty: a running reindex is complete.
    fn finish_reindex(&self) -> Option<OcrProgress> {
        let r = self.st.lock().reindex.take()?;
        Some(OcrProgress { done: r.total, total: r.total })
    }
}

/// Finished ids, emitted together at most once per window.
struct Batch {
    ids: Vec<i64>,
    since: Option<Instant>,
    every: Duration,
}

impl Batch {
    fn push(&mut self, id: i64) {
        if !self.ids.contains(&id) {
            self.ids.push(id);
        }
        self.since.get_or_insert_with(Instant::now);
    }

    fn maybe_flush(&mut self, sink: &dyn Sink) {
        if self.since.is_some_and(|t| t.elapsed() >= self.every) {
            self.flush(sink);
        }
    }

    fn flush(&mut self, sink: &dyn Sink) {
        self.since = None;
        if !self.ids.is_empty() {
            sink.changed(std::mem::take(&mut self.ids));
        }
    }
}

/// The queue loop. Returns only after [`Control::stop`] (tests).
pub(crate) fn run<F>(ctl: &Control, store: &Store, mut make: F, sink: &dyn Sink, t: &Timing)
where
    F: FnMut(&str) -> Result<Box<dyn Recognize>, Unavailable>,
{
    let mut engine: Option<(String, Box<dyn Recognize>)> = None;
    let mut failures = 0u32;
    let mut unavailable_logged = false;
    let mut last_label: Option<String> = None;
    let mut batch = Batch { ids: Vec::new(), since: None, every: t.batch };

    while let Some(snap) = ctl.snapshot() {
        if !snap.enabled {
            // OCR off: the queue stops; recognised text stays in the database.
            batch.flush(sink);
            engine = None;
            ctl.wait(snap.wake, t.idle);
            continue;
        }
        if ctl.take_catch_up() {
            match store.ocr_enqueue_unprocessed() {
                Ok(n) if n > 0 => logx::warn("ocr", &format!("queued {n} images that were never recognised")),
                Ok(_) => {}
                Err(e) => {
                    logx::warn("ocr", &format!("catch-up: {e}"));
                    ctl.set_catch_up();
                }
            }
        }
        let job = match store.ocr_next() {
            Ok(Some(job)) => job,
            Ok(None) => {
                // Idle: release the engine (its language data) until the next image.
                engine = None;
                batch.flush(sink);
                if let Some(p) = ctl.finish_reindex() {
                    logx::warn("ocr", &format!("reindex finished ({} images)", p.total));
                    sink.progress(p);
                }
                ctl.wait(snap.wake, t.idle);
                continue;
            }
            Err(e) => {
                logx::warn("ocr", &format!("next job: {e}"));
                ctl.wait(snap.wake, t.retry);
                continue;
            }
        };
        if engine.as_ref().is_none_or(|(setting, _)| *setting != snap.language) {
            engine = None;
            match make(&snap.language) {
                Ok(e) => {
                    let label = e.label();
                    if last_label.as_deref() != Some(label.as_str()) {
                        logx::warn("ocr", &format!("engine ready ({label})"));
                        last_label = Some(label);
                    }
                    engine = Some((snap.language.clone(), e));
                    failures = 0;
                    unavailable_logged = false;
                }
                Err(u) => {
                    // No language (or WinRT failed): the image stays pending; re-checked
                    // periodically, so a language pack installed meanwhile is picked up.
                    if !unavailable_logged {
                        logx::warn("ocr", &format!("engine unavailable: {u:?}"));
                        unavailable_logged = true;
                        last_label = None;
                    }
                    batch.flush(sink);
                    ctl.wait(snap.wake, t.recheck);
                    continue;
                }
            }
        }
        let Some((_, recognizer)) = engine.as_ref() else { continue };
        let started = Instant::now();
        // Decode + recognise at the lowest CPU priority; DB calls stay at normal priority.
        let outcome = super::engine::low_priority(|| match std::fs::read(&job.png) {
            Ok(png) => recognizer.recognize(&png),
            Err(e) => Err(format!("invalid: png file: {:?}", e.kind())),
        });
        let ms = started.elapsed().as_millis();
        if ctl.epoch() != snap.epoch {
            // Turned off / language changed / reindex started while recognising: the record
            // stays pending and is redone under the new conditions.
            continue;
        }
        let engine_failed = matches!(&outcome, Err(e) if e.starts_with("failed:"));
        match &outcome {
            Err(e) => logx::warn("ocr", &format!("clip {} failed after {ms} ms: {}", job.id, error_code(e))),
            Ok(_) if ms >= SLOW_MS => logx::warn("ocr", &format!("clip {} took {ms} ms", job.id)),
            Ok(_) => {}
        }
        match store.ocr_finish_job(&job, outcome.as_deref().map_err(|_| ()), OcrWrite::Queued) {
            Ok(true) => batch.push(job.id),
            Ok(false) => {} // deleted / answered by clip_ocr / database replaced meanwhile: dropped
            Err(e) => {
                logx::warn("ocr", &format!("clip {} write: {e}", job.id));
                ctl.wait(snap.wake, t.retry);
                continue;
            }
        }
        if engine_failed {
            failures += 1;
            if failures >= ENGINE_FAILURES_MAX {
                logx::warn("ocr", "engine failed repeatedly; rebuilding it");
                engine = None;
                failures = 0;
            }
        } else {
            failures = 0;
        }
        if ctl.reindex_progress().is_some() {
            match store.ocr_pending() {
                Ok(remaining) => {
                    if let Some(p) = ctl.after_job(remaining, t.progress) {
                        if p.done == p.total {
                            logx::warn("ocr", &format!("reindex finished ({} images)", p.total));
                        }
                        sink.progress(p);
                    }
                }
                Err(e) => logx::warn("ocr", &format!("pending count: {e}")),
            }
        }
        batch.maybe_flush(sink);
    }
    batch.flush(sink);
}

/// `"failed: RecognizeAsync: 0x…"` → `"failed: RecognizeAsync: 0x…"`; messages never carry
/// recognised text, but keep only the first 120 chars to be safe in the log.
fn error_code(e: &str) -> String {
    e.chars().take(120).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::ocr::STATE_DONE;
    use crate::store::testutil::TestStore;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Fake engine: the "text" of an image is `img-<width>x<height>`; width 13 fails, width
    /// 17 blocks until `gate` opens (to race a deletion / setting change against it).
    pub(crate) struct Fake {
        pub label: String,
        pub calls: Arc<AtomicUsize>,
        pub gate: Arc<(Mutex<bool>, Condvar)>,
    }

    impl Recognize for Fake {
        fn recognize(&self, png: &[u8]) -> Result<String, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let img = image::load_from_memory(png).map_err(|e| format!("invalid: {e}"))?;
            if img.width() == 13 {
                return Err("failed: fake engine error".into());
            }
            if img.width() == 17 {
                let (m, cv) = &*self.gate;
                let mut open = m.lock();
                while !*open {
                    cv.wait(&mut open);
                }
            }
            Ok(format!("img-{}x{} {}", img.width(), img.height(), self.label))
        }
        fn label(&self) -> String {
            self.label.clone()
        }
    }

    #[derive(Default)]
    pub(crate) struct Rec {
        pub changed: Mutex<Vec<i64>>,
        pub progress: Mutex<Vec<OcrProgress>>,
    }

    impl Sink for Rec {
        fn changed(&self, ids: Vec<i64>) {
            self.changed.lock().extend(ids);
        }
        fn progress(&self, p: OcrProgress) {
            self.progress.lock().push(p);
        }
    }

    const FAST: Timing = Timing {
        idle: Duration::from_millis(50),
        recheck: Duration::from_millis(50),
        retry: Duration::from_millis(20),
        batch: Duration::from_millis(0),
        progress: Duration::from_millis(0),
    };

    struct Harness {
        t: Arc<TestStore>,
        ctl: Arc<Control>,
        rec: Arc<Rec>,
        calls: Arc<AtomicUsize>,
        gate: Arc<(Mutex<bool>, Condvar)>,
        langs_installed: Arc<Mutex<bool>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Harness {
        fn new(enabled: bool) -> Harness {
            let t = Arc::new(TestStore::new());
            let ctl = Arc::new(Control::new(enabled, "auto"));
            let wake = ctl.clone();
            t.ocr_configure(enabled, Some(Arc::new(move || wake.wake())));
            Harness {
                t,
                ctl,
                rec: Arc::new(Rec::default()),
                calls: Arc::new(AtomicUsize::new(0)),
                gate: Arc::new((Mutex::new(true), Condvar::new())),
                langs_installed: Arc::new(Mutex::new(true)),
                thread: None,
            }
        }

        fn start(&mut self) {
            let (t, ctl, rec, calls, gate, langs) =
                (self.t.clone(), self.ctl.clone(), self.rec.clone(), self.calls.clone(), self.gate.clone(), self.langs_installed.clone());
            self.thread = Some(std::thread::spawn(move || {
                let make = |setting: &str| -> Result<Box<dyn Recognize>, Unavailable> {
                    if !*langs.lock() {
                        return Err(Unavailable::NoLanguages);
                    }
                    Ok(Box::new(Fake { label: setting.to_string(), calls: calls.clone(), gate: gate.clone() }))
                };
                run(&ctl, &t, make, &*rec, &FAST);
            }));
        }

        fn state(&self, id: i64) -> (i64, Option<String>) {
            self.t
                .lock()
                .query_row("SELECT ocr_state, ocr_text FROM clips WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
        }

        fn wait_until(&self, what: &str, f: impl Fn() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !f() {
                assert!(Instant::now() < deadline, "timed out waiting for: {what}");
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn set_gate(&self, open: bool) {
            let (m, cv) = &*self.gate;
            *m.lock() = open;
            cv.notify_all();
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            self.set_gate(true);
            self.ctl.stop();
            if let Some(th) = self.thread.take() {
                let _ = th.join();
            }
        }
    }

    #[test]
    fn recognises_new_images_in_the_background_and_emits_changes() {
        let mut h = Harness::new(true);
        h.start();
        let a = h.t.add_image(20, 10, 1_000, false);
        let b = h.t.add_image(13, 10, 2_000, false); // fake engine error
        h.wait_until("both processed", || h.t.ocr_pending().unwrap() == 0);
        assert_eq!(h.state(a), (STATE_DONE, Some("img-20x10 auto".into())));
        assert_eq!(h.state(b).0, crate::store::ocr::STATE_ERROR);
        h.wait_until("events", || h.rec.changed.lock().len() == 2);
        let mut ids = h.rec.changed.lock().clone();
        ids.sort();
        assert_eq!(ids, vec![a, b]);
        // An error is not retried by the queue.
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(h.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn pending_rows_from_a_previous_run_are_processed_on_start() {
        let mut h = Harness::new(true);
        let a = h.t.add_image(21, 10, 1_000, false); // queued, no worker yet ("previous run")
        h.t.ocr_set_queue_new(false);
        let never = h.t.add_image(22, 10, 2_000, false); // captured while OCR was off
        h.t.ocr_set_queue_new(true);
        assert_eq!(h.state(never).0, crate::store::ocr::STATE_NONE);
        h.start();
        h.wait_until("both done (start = catch-up)", || h.state(a).0 == STATE_DONE && h.state(never).0 == STATE_DONE);
    }

    #[test]
    fn disabling_stops_the_queue_and_keeps_text() {
        let mut h = Harness::new(true);
        h.start();
        let a = h.t.add_image(23, 10, 1_000, false);
        h.wait_until("a done", || h.state(a).0 == STATE_DONE);
        h.ctl.apply_settings(false, "auto");
        h.t.ocr_set_queue_new(false);
        let off = h.t.add_image(24, 10, 2_000, false);
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(h.state(off).0, crate::store::ocr::STATE_NONE, "not queued while off");
        assert_eq!(h.state(a), (STATE_DONE, Some("img-23x10 auto".into())), "recognised text stays");
        // Turning it back on catches up.
        h.t.ocr_set_queue_new(true);
        h.ctl.apply_settings(true, "auto");
        h.wait_until("caught up", || h.state(off).0 == STATE_DONE);
    }

    #[test]
    fn a_result_is_dropped_when_the_record_is_deleted_mid_recognition() {
        let mut h = Harness::new(true);
        h.set_gate(false);
        h.start();
        let id = h.t.add_image(17, 10, 1_000, false); // blocks in the fake engine
        h.wait_until("recognising", || h.calls.load(Ordering::SeqCst) == 1);
        h.t.lock().execute("DELETE FROM clips WHERE id = ?1", [id]).unwrap();
        h.set_gate(true);
        let other = h.t.add_image(25, 10, 2_000, false);
        h.wait_until("next image done", || h.state(other).0 == STATE_DONE);
        assert!(!h.rec.changed.lock().contains(&id), "no event for a deleted record");
    }

    #[test]
    fn a_language_change_mid_recognition_redoes_the_image_with_the_new_language() {
        let mut h = Harness::new(true);
        h.set_gate(false);
        h.start();
        let id = h.t.add_image(17, 10, 1_000, false);
        h.wait_until("recognising", || h.calls.load(Ordering::SeqCst) == 1);
        h.ctl.apply_settings(true, "ru");
        h.set_gate(true);
        h.wait_until("redone in ru", || h.state(id) == (STATE_DONE, Some("img-17x10 ru".into())));
        // Earlier results are not touched by a language change.
        let before = h.t.add_image(26, 10, 2_000, false);
        h.wait_until("done", || h.state(before).0 == STATE_DONE);
        h.ctl.apply_settings(true, "en-US");
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(h.state(before), (STATE_DONE, Some("img-26x10 ru".into())));
    }

    #[test]
    fn reindex_reports_progress_to_done_equals_total_and_is_not_duplicated() {
        let mut h = Harness::new(true);
        h.start();
        let ids: Vec<i64> = (0..4).map(|i| h.t.add_image(30 + i, 10, 1_000 + i64::from(i), false)).collect();
        h.wait_until("initial pass", || h.t.ocr_pending().unwrap() == 0);
        // Pause the queue so the requeue → begin sequence is deterministic.
        h.ctl.apply_settings(false, "auto");
        assert_eq!(h.t.ocr_requeue_all().unwrap(), 4);
        let (started, p) = h.ctl.begin_reindex(h.t.ocr_pending().unwrap());
        assert!(started);
        assert_eq!(p, OcrProgress { done: 0, total: 4 });
        assert_eq!(h.ctl.begin_reindex(99), (false, OcrProgress { done: 0, total: 4 }), "a running reindex is not restarted");
        h.ctl.apply_settings(true, "auto");
        h.wait_until("final progress", || h.rec.progress.lock().last().is_some_and(|p| p.done == p.total && p.total == 4));
        let events = h.rec.progress.lock().clone();
        assert!(events.windows(2).all(|w| w[0].done <= w[1].done), "{events:?}");
        assert_eq!(h.ctl.reindex_progress(), None);
        for id in ids {
            assert_eq!(h.state(id).0, STATE_DONE);
        }
        // Nothing to reindex → {0, 0} at once, nothing runs.
        assert_eq!(h.ctl.begin_reindex(0), (true, OcrProgress { done: 0, total: 0 }));
    }

    #[test]
    fn no_languages_keeps_images_pending_until_a_language_appears() {
        let mut h = Harness::new(true);
        *h.langs_installed.lock() = false;
        h.start();
        let id = h.t.add_image(27, 10, 1_000, false);
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(h.state(id).0, crate::store::ocr::STATE_PENDING);
        *h.langs_installed.lock() = true; // picked up by the periodic re-check
        h.wait_until("recognised after the pack appeared", || h.state(id).0 == STATE_DONE);
    }

    #[test]
    fn a_missing_png_is_an_error_not_a_crash() {
        let mut h = Harness::new(true);
        let id = h.t.add_image(28, 10, 1_000, false);
        std::fs::remove_file(h.t.paths().media_png(id)).unwrap();
        h.start();
        h.wait_until("error", || h.state(id).0 == crate::store::ocr::STATE_ERROR);
        assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn after_job_grows_total_when_images_arrive_and_ends_at_done_equals_total() {
        let ctl = Control::new(true, "auto");
        assert!(ctl.begin_reindex(2).0);
        assert_eq!(ctl.after_job(1, Duration::ZERO), Some(OcrProgress { done: 1, total: 2 }));
        // A new capture arrived: 2 remaining after 2 done.
        assert_eq!(ctl.after_job(2, Duration::ZERO), Some(OcrProgress { done: 2, total: 4 }));
        assert_eq!(ctl.after_job(0, Duration::ZERO), Some(OcrProgress { done: 4, total: 4 }));
        assert_eq!(ctl.reindex_progress(), None);
        assert_eq!(ctl.after_job(0, Duration::ZERO), None, "no reindex → no event");
    }
}
