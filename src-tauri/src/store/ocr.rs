//! OCR queue in the store (W2-A, ARCHITECTURE §5a). The queue IS the database: an image
//! with `ocr_state = pending` waits for the recognizer, so the queue survives restarts.
//!
//! `clips.ocr_state`: 0 `none` (not queued: every non-image; an image captured while OCR was
//! off, or imported) · 1 `pending` · 2 `done` (`ocr_text` set, `""` = no text on the image) ·
//! 3 `error` (recognition failed; retried only by `ocr_reindex` / `clip_ocr`; a previous
//! `ocr_text` is kept). `clips_fts` indexes `ocr_text` through the `clips_fts_au` trigger, so
//! a written text is searchable at once.
//!
//! Queries over the queue use `idx_clips_kind (kind, copied_at DESC, id DESC, deleted_at)
//! WHERE deleted_at IS NULL` with `kind IN ('image', 'screenshot')` — only picture rows are
//! visited (W6-A: screenshots are recognised like images). The next job is one newest-first seek
//! per picture kind, so no sort is needed (test `queue_statements_use_the_kind_index`).
//!
//! Nothing here logs recognised text.

use super::{db_err, Store};
use crate::model::{Kind, OcrState};
use parking_lot::Mutex;
use rusqlite::{params, OptionalExtension, Transaction};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) const STATE_NONE: i64 = 0;
pub(crate) const STATE_PENDING: i64 = 1;
pub(crate) const STATE_DONE: i64 = 2;
pub(crate) const STATE_ERROR: i64 = 3;

/// Called (quickly, from any thread) when new work may be in the queue.
pub(crate) type Waker = Arc<dyn Fn() + Send + Sync>;

/// The OCR module's hook into the capture insert (`Store.ocr`): whether a new image is
/// queued, and how to wake the recognizer. Both are set by `ocr::start` and by the
/// `ocr.*` settings subscription; before that (and in tests) nothing is queued.
#[derive(Default)]
pub(crate) struct OcrHook {
    queue_new: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

/// One image to recognise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcrJob {
    pub id: i64,
    /// `media\<id>.png` (absolute when the data dir is).
    pub png: PathBuf,
    /// [`Store::generation`] when the job was taken: a `replace` restore in between makes
    /// `id` name another record, and the result is dropped ([`Store::ocr_finish_job`]).
    pub generation: u64,
}

/// What `clip_ocr` needs to decide whether to recognise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcrRecord {
    pub kind: Kind,
    pub state: OcrState,
    pub text: Option<String>,
}

/// Who writes a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OcrWrite {
    /// The background queue: only while the record is still `pending` (a `clip_ocr` result
    /// that landed first wins; a deleted record silently drops the result).
    Queued,
    /// `clip_ocr`: whatever the current state (still only a live image).
    Direct,
}

pub(crate) fn state_of(v: i64) -> OcrState {
    match v {
        STATE_PENDING => OcrState::Pending,
        STATE_DONE => OcrState::Done,
        STATE_ERROR => OcrState::Error,
        _ => OcrState::None,
    }
}

/// The `UPDATE` of [`Store::ocr_finish_job`] (conditions in its doc).
fn finish(conn: &rusqlite::Connection, id: i64, outcome: Result<&str, ()>, mode: OcrWrite) -> Result<bool, String> {
    let only_pending = if mode == OcrWrite::Queued { " AND ocr_state = 1" } else { "" };
    let n = match outcome {
        Ok(text) => conn.execute(
            &format!(
                "UPDATE clips SET ocr_text = ?1, ocr_state = ?2
                 WHERE id = ?3 AND kind IN ('image', 'screenshot') AND deleted_at IS NULL{only_pending}"
            ),
            params![text, STATE_DONE, id],
        ),
        Err(()) => conn.execute(
            &format!(
                "UPDATE clips SET ocr_state = ?1
                 WHERE id = ?2 AND kind IN ('image', 'screenshot') AND deleted_at IS NULL{only_pending}"
            ),
            params![STATE_ERROR, id],
        ),
    }
    .map_err(db_err)?;
    Ok(n > 0)
}

impl Store {
    /// Set whether new images are queued and who to wake (`ocr::start`).
    pub(crate) fn ocr_configure(&self, queue_new: bool, waker: Option<Waker>) {
        self.ocr.queue_new.store(queue_new, Ordering::SeqCst);
        *self.ocr.waker.lock() = waker;
    }

    /// `ocr.enabled` changed (settings subscription — must stay quick).
    pub(crate) fn ocr_set_queue_new(&self, on: bool) {
        self.ocr.queue_new.store(on, Ordering::SeqCst);
    }

    /// Nudge the recognizer (no-op before `ocr::start`).
    pub(crate) fn ocr_wake(&self) {
        let waker = self.ocr.waker.lock().clone();
        if let Some(w) = waker {
            w();
        }
    }

    /// Inside the capture insert transaction (`write.rs`): a new image — or a re-copied one
    /// that was never queued — enters the queue while OCR is on. `done`/`error` stay as they
    /// are (re-copying is not a retry).
    pub(super) fn ocr_enqueue_in(&self, tx: &Transaction<'_>, id: i64) -> Result<(), String> {
        if !self.ocr.queue_new.load(Ordering::SeqCst) {
            return Ok(());
        }
        tx.execute(
            "UPDATE clips SET ocr_state = ?1 WHERE id = ?2 AND kind IN ('image', 'screenshot') AND ocr_state = ?3",
            params![STATE_PENDING, id, STATE_NONE],
        )
        .map(|_| ())
        .map_err(db_err)
    }

    /// Newest pending live picture (image or screenshot), if any: the newest of each kind's
    /// own index seek.
    pub(crate) fn ocr_next(&self) -> Result<Option<OcrJob>, String> {
        let conn = self.lock();
        let generation = self.generation(); // read under the lock: a swap can't interleave
        let mut best: Option<(i64, i64)> = None; // (copied_at, id)
        for kind in ["image", "screenshot"] {
            let found: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT copied_at, id FROM clips
                     WHERE kind = ?1 AND deleted_at IS NULL AND ocr_state = ?2
                     ORDER BY copied_at DESC, id DESC LIMIT 1",
                    params![kind, STATE_PENDING],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db_err)?;
            best = best.max(found);
        }
        Ok(best.map(|(_, id)| OcrJob { id, png: self.paths().media_png(id), generation }))
    }

    /// Kind, state and text of a live record (`not_found: clip <id>` otherwise).
    pub(crate) fn ocr_record(&self, id: i64) -> Result<OcrRecord, String> {
        let row: Option<(String, i64, Option<String>)> = self
            .lock()
            .query_row(
                "SELECT kind, ocr_state, ocr_text FROM clips WHERE id = ?1 AND deleted_at IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db_err)?;
        let (kind, state, text) = row.ok_or_else(|| format!("not_found: clip {id}"))?;
        let kind = Kind::parse(&kind).unwrap_or(Kind::Text);
        Ok(OcrRecord { kind, state: state_of(state), text })
    }

    /// The PNG of a live image record, for a direct recognition.
    pub(crate) fn ocr_job(&self, id: i64) -> Result<OcrJob, String> {
        let generation = self.generation();
        let rec = self.ocr_record(id)?;
        if !rec.kind.is_image() {
            return Err(format!("invalid: clip {id} is not an image"));
        }
        Ok(OcrJob { id, png: self.paths().media_png(id), generation })
    }

    /// Write a recognition result in one statement: `Ok(text)` → `ocr_text = text`, `done`;
    /// `Err(())` → `error` (an earlier text is kept). Returns whether a row changed — `false`
    /// when the record was deleted meanwhile (the result is dropped), for
    /// [`OcrWrite::Queued`] when it is no longer pending, and when a `replace` restore swapped
    /// the database since the job was taken (its id now names another record — W3).
    pub(crate) fn ocr_finish_job(&self, job: &OcrJob, outcome: Result<&str, ()>, mode: OcrWrite) -> Result<bool, String> {
        let conn = self.lock();
        if self.generation() != job.generation {
            return Ok(false);
        }
        finish(&conn, job.id, outcome, mode)
    }

    /// [`Self::ocr_finish_job`] for the current database (tests).
    #[cfg(test)]
    pub(crate) fn ocr_finish(&self, id: i64, outcome: Result<&str, ()>, mode: OcrWrite) -> Result<bool, String> {
        finish(&self.lock(), id, outcome, mode)
    }

    /// Live images waiting for the recognizer (`OcrStatus.pending`).
    pub(crate) fn ocr_pending(&self) -> Result<i64, String> {
        self.lock()
            .query_row(
                "SELECT count(*) FROM clips WHERE kind IN ('image', 'screenshot') AND deleted_at IS NULL AND ocr_state = ?1",
                [STATE_PENDING],
                |r| r.get(0),
            )
            .map_err(db_err)
    }

    /// `ocr_reindex`: every live image back to `pending` (texts stay searchable until the new
    /// result replaces them). Returns the number of rows changed.
    pub(crate) fn ocr_requeue_all(&self) -> Result<i64, String> {
        self.lock()
            .execute(
                "UPDATE clips SET ocr_state = ?1 WHERE kind IN ('image', 'screenshot') AND deleted_at IS NULL AND ocr_state <> ?1",
                [STATE_PENDING],
            )
            .map(|n| n as i64)
            .map_err(db_err)
    }

    /// OCR (re)enabled or started: live images that were never queued (captured while OCR
    /// was off, imported) join the queue. Returns how many.
    pub(crate) fn ocr_enqueue_unprocessed(&self) -> Result<i64, String> {
        self.lock()
            .execute(
                "UPDATE clips SET ocr_state = ?1 WHERE kind IN ('image', 'screenshot') AND deleted_at IS NULL AND ocr_state = ?2",
                params![STATE_PENDING, STATE_NONE],
            )
            .map(|n| n as i64)
            .map_err(db_err)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::TestStore;
    use super::*;
    use crate::model::FeedQuery;
    use std::sync::atomic::AtomicUsize;

    fn state(t: &TestStore, id: i64) -> (i64, Option<String>) {
        t.lock()
            .query_row("SELECT ocr_state, ocr_text FROM clips WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
    }

    fn search(t: &TestStore, text: &str) -> Vec<i64> {
        let q = FeedQuery { text: text.into(), exclude: vec![], filter: Default::default(), cursor: None, limit: 40 };
        t.clips_list(&q, false).unwrap().items.into_iter().map(|c| c.id).collect()
    }

    #[test]
    fn new_images_are_queued_only_while_ocr_is_on_and_wake_the_worker() {
        let t = TestStore::new();
        let wakes = Arc::new(AtomicUsize::new(0));
        let w = wakes.clone();
        // Off: nothing queued, nobody woken... (the waker is set but queue_new is false)
        t.ocr_configure(false, Some(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        })));
        let off = t.add_image(8, 8, 1_000, false);
        assert_eq!(state(&t, off).0, STATE_NONE);
        t.ocr_set_queue_new(true);
        let on = t.add_image(9, 9, 2_000, false);
        assert_eq!(state(&t, on).0, STATE_PENDING);
        let text = t.add_text("not an image", 3_000);
        assert_eq!(state(&t, text).0, STATE_NONE);
        assert!(wakes.load(Ordering::SeqCst) >= 1, "an image insert wakes the recognizer");
        // Re-copying the never-queued image queues it; a done image is not re-queued.
        let again = t.add_image(8, 8, 4_000, false);
        assert_eq!(again, off);
        assert_eq!(state(&t, off).0, STATE_PENDING);
        assert!(t.ocr_finish(off, Ok("x"), OcrWrite::Queued).unwrap());
        t.add_image(8, 8, 5_000, false);
        assert_eq!(state(&t, off).0, STATE_DONE);
    }

    #[test]
    fn queue_is_newest_first_and_survives_reopen() {
        let mut t = TestStore::new();
        t.ocr_set_queue_new(true);
        let a = t.add_image(4, 4, 1_000, false);
        let b = t.add_image(5, 5, 2_000, false);
        assert_eq!(t.ocr_pending().unwrap(), 2);
        let job = t.ocr_next().unwrap().unwrap();
        assert_eq!(job.id, b);
        assert_eq!(job.png, t.paths().media_png(b));
        t = t.reopen();
        assert_eq!(t.ocr_pending().unwrap(), 2, "pending rows survive a restart");
        assert!(t.ocr_finish(b, Ok("hello"), OcrWrite::Queued).unwrap());
        assert_eq!(t.ocr_next().unwrap().unwrap().id, a);
        assert!(t.ocr_finish(a, Err(()), OcrWrite::Queued).unwrap());
        assert_eq!(t.ocr_next().unwrap(), None);
        assert_eq!(t.ocr_pending().unwrap(), 0);
    }

    #[test]
    fn result_is_searchable_and_empty_text_is_done() {
        let t = TestStore::new();
        t.ocr_set_queue_new(true);
        let id = t.add_image(6, 6, 1_000, false);
        let blank = t.add_image(7, 7, 2_000, false);
        assert!(search(&t, "invoice").is_empty());
        assert!(t.ocr_finish(id, Ok("Total due\nInvoice #2026"), OcrWrite::Queued).unwrap());
        assert!(t.ocr_finish(blank, Ok(""), OcrWrite::Queued).unwrap());
        assert_eq!(search(&t, "invoice"), vec![id], "FTS finds the recognised text");
        assert_eq!(search(&t, "2026"), vec![id]);
        let d = t.clip_detail(id).unwrap();
        assert_eq!(d.clip.ocr, OcrState::Done);
        assert_eq!(d.ocr_text.as_deref(), Some("Total due\nInvoice #2026"));
        let d = t.clip_detail(blank).unwrap();
        assert_eq!((d.clip.ocr, d.ocr_text.as_deref()), (OcrState::Done, Some("")));
    }

    #[test]
    fn deleted_records_drop_the_result() {
        let t = TestStore::new();
        t.ocr_set_queue_new(true);
        let soft = t.add_image(4, 4, 1_000, false);
        let hard = t.add_image(5, 5, 2_000, false);
        t.delete(&[soft]).unwrap();
        assert_eq!(t.ocr_pending().unwrap(), 1, "soft-deleted images leave the queue");
        assert!(!t.ocr_finish(soft, Ok("secret"), OcrWrite::Queued).unwrap());
        assert!(!t.ocr_finish(soft, Ok("secret"), OcrWrite::Direct).unwrap());
        assert_eq!(state(&t, soft), (STATE_PENDING, None));
        t.lock().execute("DELETE FROM clips WHERE id = ?1", [hard]).unwrap();
        assert!(!t.ocr_finish(hard, Ok("x"), OcrWrite::Queued).unwrap());
        assert!(t.ocr_record(hard).unwrap_err().starts_with("not_found:"));
        assert!(t.ocr_record(soft).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn queued_write_needs_pending_direct_write_does_not() {
        let t = TestStore::new();
        let id = t.add_image(4, 4, 1_000, false); // OCR off → none
        assert!(!t.ocr_finish(id, Ok("late"), OcrWrite::Queued).unwrap(), "not pending → queue result dropped");
        assert!(t.ocr_finish(id, Ok("direct"), OcrWrite::Direct).unwrap());
        assert_eq!(state(&t, id), (STATE_DONE, Some("direct".into())));
        // error keeps the earlier text
        assert!(t.ocr_finish(id, Err(()), OcrWrite::Direct).unwrap());
        assert_eq!(state(&t, id), (STATE_ERROR, Some("direct".into())));
        let txt = t.add_text("plain text", 2_000);
        assert!(!t.ocr_finish(txt, Ok("x"), OcrWrite::Direct).unwrap(), "only images get OCR text");
        assert!(t.ocr_job(txt).unwrap_err().starts_with("invalid:"));
        assert_eq!(t.ocr_record(txt).unwrap().kind, Kind::Text);
    }

    #[test]
    fn requeue_and_catch_up() {
        let t = TestStore::new();
        let none = t.add_image(4, 4, 1_000, false);
        t.ocr_set_queue_new(true);
        let done = t.add_image(5, 5, 2_000, false);
        let err = t.add_image(6, 6, 3_000, false);
        t.ocr_finish(done, Ok("kept"), OcrWrite::Queued).unwrap();
        t.ocr_finish(err, Err(()), OcrWrite::Queued).unwrap();
        assert_eq!(t.ocr_enqueue_unprocessed().unwrap(), 1);
        assert_eq!(state(&t, none).0, STATE_PENDING);
        assert_eq!(state(&t, err).0, STATE_ERROR, "catch-up does not retry errors");
        assert_eq!(t.ocr_requeue_all().unwrap(), 2);
        assert_eq!(t.ocr_pending().unwrap(), 3);
        assert_eq!(state(&t, done), (STATE_PENDING, Some("kept".into())), "text stays searchable during a reindex");
        assert_eq!(search(&t, "kept"), vec![done]);
    }

    #[test]
    fn queue_statements_use_the_kind_index() {
        let t = TestStore::new();
        let plan = |sql: &str| -> String {
            let c = t.lock();
            let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows: Vec<String> = st.query_map([], |r| r.get::<_, String>(3)).unwrap().map(Result::unwrap).collect();
            rows.join(" | ")
        };
        for kind in ["image", "screenshot"] {
            let next = plan(&format!(
                "SELECT copied_at, id FROM clips WHERE kind = '{kind}' AND deleted_at IS NULL AND ocr_state = 1
                 ORDER BY copied_at DESC, id DESC LIMIT 1"
            ));
            assert!(next.contains("idx_clips_kind") && !next.contains("TEMP B-TREE"), "{next}");
        }
        let count = plan(
            "SELECT count(*) FROM clips WHERE kind IN ('image', 'screenshot') AND deleted_at IS NULL AND ocr_state = 1",
        );
        assert!(count.contains("idx_clips_kind"), "{count}");
    }

    /// W6-A: screenshots join the queue like images, and the next job is the newest picture.
    #[test]
    fn screenshots_are_queued_with_images() {
        let t = TestStore::new();
        t.ocr_set_queue_new(true);
        let img = t.add_image(5, 5, 1_000, false);
        let mut shot = super::super::testutil::image_clip(6, 6, 2_000, false);
        shot.kind = Kind::Screenshot;
        let shot = t.insert_clip(shot).unwrap().id;
        assert_eq!(state(&t, shot).0, STATE_PENDING);
        assert_eq!(t.ocr_pending().unwrap(), 2);
        assert_eq!(t.ocr_next().unwrap().unwrap().id, shot, "newest picture first");
        assert!(t.ocr_job(shot).is_ok());
        assert!(t.ocr_finish(shot, Ok("from the screen"), OcrWrite::Queued).unwrap());
        assert_eq!(t.ocr_next().unwrap().unwrap().id, img);
        assert_eq!(search(&t, "screen"), vec![shot]);
    }

    /// W3: a job taken before a `replace` restore swapped the database names a record of the
    /// OLD database; its text must not land on whatever record has that id now.
    #[test]
    fn a_result_for_a_replaced_database_is_dropped() {
        let t = TestStore::new();
        t.ocr_set_queue_new(true);
        let id = t.add_image(9, 9, 1_000, false);
        let queued = t.ocr_next().unwrap().unwrap();
        let direct = t.ocr_job(id).unwrap();
        t.generation.fetch_add(1, Ordering::SeqCst); // what a successful replace does
        assert!(!t.ocr_finish_job(&queued, Ok("text of another image"), OcrWrite::Queued).unwrap());
        assert!(!t.ocr_finish_job(&direct, Ok("text of another image"), OcrWrite::Direct).unwrap());
        assert_eq!(state(&t, id), (STATE_PENDING, None), "untouched, still queued");
        let fresh = t.ocr_next().unwrap().unwrap();
        assert!(t.ocr_finish_job(&fresh, Ok("right text"), OcrWrite::Queued).unwrap());
        assert_eq!(state(&t, id), (STATE_DONE, Some("right text".into())));
    }
}
