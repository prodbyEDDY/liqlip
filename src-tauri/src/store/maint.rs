//! Background thread of the store: purge of expired soft deletes (every 30 s), retention
//! (at start, hourly, ≤ once per 30 s after inserts, immediately when `storage.*` settings
//! change), `PRAGMA optimize` + incremental vacuum (hourly), orphan media sweep (at start),
//! and the corruption marker. It owns an `AppHandle` to emit
//! `clips:changed { origin: 'retention', change: 'deleted' }`.

use super::lifecycle::VACUUM_MIN_FREE_PAGES;
use super::Store;
use crate::model::{ChangeOrigin, ClipsChange, ClipsChanged};
use crate::{events, logx, settings};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const PURGE_EVERY: Duration = Duration::from_secs(30);
const HOURLY: Duration = Duration::from_secs(3600);
/// Retention after an insert runs at most this often.
const AFTER_INSERT_GAP: Duration = Duration::from_secs(30);
/// Event payloads list at most this many ids (the frontend reloads without ids).
const EVENT_IDS_MAX: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wake {
    /// A record was added (capture or user): retention, throttled.
    Inserted,
    /// `settings.storage` changed: retention now.
    Settings,
}

pub(crate) fn start(app: &AppHandle) {
    let Some(store) = app.try_state::<Store>() else {
        logx::warn("store", "init: store not managed");
        return;
    };
    let (tx, rx) = mpsc::channel::<Wake>();
    *store.maint.lock() = Some(tx.clone());
    // Subscribers run under the settings write lock: only send, never work here.
    settings::subscribe(app, move |old, new| {
        if old.storage != new.storage {
            let _ = tx.send(Wake::Settings);
        }
    });
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("liqlip-store".into()).spawn(move || run(app, rx));
    if let Err(e) = spawned {
        logx::warn("store", &format!("background thread failed to start: {e}"));
    }
}

fn emit_deleted(app: &AppHandle, ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    let ids = (ids.len() <= EVENT_IDS_MAX).then_some(ids);
    events::clips_changed(app, &ClipsChanged { origin: ChangeOrigin::Retention, change: ClipsChange::Deleted, ids });
}

fn retention(app: &AppHandle, store: &Store) {
    let storage = settings::get(app).storage;
    match store.run_retention(&storage) {
        Ok(ids) => {
            let many = ids.len() >= 100;
            emit_deleted(app, ids);
            if many {
                if let Err(e) = store.vacuum_if_needed(VACUUM_MIN_FREE_PAGES) {
                    logx::warn("store", &format!("vacuum: {e}"));
                }
            }
        }
        Err(e) => logx::warn("store", &format!("retention: {e}")),
    }
}

fn tick(store: &Store) {
    store.expire_undo();
    if let Err(e) = store.purge_deleted() {
        logx::warn("store", &format!("purge: {e}"));
    }
    if super::corruption_seen() {
        let marker = store.paths().check_marker();
        if !marker.exists() {
            logx::warn("store", "database corruption reported; quick_check on next start");
            let _ = std::fs::write(&marker, b"");
        }
    }
}

fn hourly(app: &AppHandle, store: &Store) {
    retention(app, store);
    if let Err(e) = store.lock().execute_batch("PRAGMA optimize;") {
        logx::warn("store", &format!("optimize: {e}"));
    }
    if let Err(e) = store.vacuum_if_needed(VACUUM_MIN_FREE_PAGES) {
        logx::warn("store", &format!("vacuum: {e}"));
    }
}

fn run(app: AppHandle, rx: mpsc::Receiver<Wake>) {
    let Some(store) = app.try_state::<Store>() else { return };
    // Start-up: leftovers from the last session, retention, orphan files.
    tick(&store);
    retention(&app, &store);
    match store.sweep_orphans() {
        Ok(n) if n > 0 => logx::warn("store", &format!("orphan sweep: {n} files removed")),
        Ok(_) => {}
        Err(e) => logx::warn("store", &format!("orphan sweep: {e}")),
    }
    let mut last_retention = Instant::now();
    let mut pending: Option<Instant> = None;
    let mut next_tick = Instant::now() + PURGE_EVERY;
    let mut next_hour = Instant::now() + HOURLY;
    loop {
        let now = Instant::now();
        let deadline = [Some(next_tick), Some(next_hour), pending].into_iter().flatten().min().unwrap_or(next_tick);
        match rx.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(Wake::Inserted) => {
                let at = (last_retention + AFTER_INSERT_GAP).max(Instant::now());
                pending = Some(pending.map_or(at, |p| p.min(at)));
            }
            Ok(Wake::Settings) => pending = Some(Instant::now()),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        if pending.is_some_and(|p| p <= now) {
            pending = None;
            retention(&app, &store);
            last_retention = Instant::now();
        }
        if next_tick <= now {
            tick(&store);
            next_tick = now + PURGE_EVERY;
        }
        if next_hour <= now {
            hourly(&app, &store);
            last_retention = Instant::now();
            next_hour = now + HOURLY;
        }
    }
}
