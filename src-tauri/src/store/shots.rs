//! Screenshots and edited pictures in the store (W6-A, ARCHITECTURE §18.5).
//!
//! A screenshot is an ordinary picture record (`clips.kind = 'screenshot'`, `media\<id>.png`,
//! thumbnail, dedup by pixel hash, retention) plus one `shot_docs` row: the annotation document
//! (`AnnotDoc`, opaque JSON here), whether `media\base\<id>.png` holds the picture without the
//! annotations, and the last file written to the screenshot folder. An `image` gets a row the
//! first time it is edited or saved to the folder. `clips.meta.shot` mirrors the row for the
//! feed (`savedPath`, `edited`) and is written in the same transaction.
//!
//! Like the rest of the store nothing here emits events (the caller does) and nothing logs
//! content — only ids and error codes.

use super::feed::clip_by_id;
use super::media::{self, media_rel};
use super::write::{meta_json, parse_meta, recompute_size, upsert_format};
use super::{db_err, InsertOutcome, NewClip, SourceApp, Store};
use crate::logx;
use crate::model::{Clip, ClipMeta, Kind, ShotMeta};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// An annotation document larger than this is refused (it can carry pasted pictures as
/// `data:` URLs; the commit body caps its header at the same size).
pub(crate) const DOC_MAX_BYTES: usize = 64 * 1024 * 1024;

/// A screenshot as the commit hands it over (`insert_screenshot`).
#[derive(Debug, Clone)]
pub(crate) struct NewShot {
    /// The record's picture (annotations drawn in), PNG.
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// `content::image_hash` of the picture's RGBA — the dedup key.
    pub hash: String,
    /// ≤ 360 px PNG; `None` → built lazily by `/thumb`.
    pub thumb_png: Option<Vec<u8>>,
    /// The picture without annotations (annotated shots only).
    pub base_png: Option<Vec<u8>>,
    /// `AnnotDoc` (coordinates relative to the picture); `None` → an empty document.
    pub doc: Option<Value>,
    /// Picture px per logical px (the monitor scale) — `unit` of an empty document.
    pub unit: f64,
    /// File already written to the screenshot folder, if any.
    pub saved_path: Option<String>,
    pub source: Option<SourceApp>,
    pub copied_at: i64,
}

/// An edit of a picture record (`apply_edit`, the editor's "Done").
#[derive(Debug, Clone)]
pub(crate) struct ShotEdit {
    /// New picture (annotations drawn in), PNG, and its pixel hash.
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub hash: String,
    pub thumb_png: Option<Vec<u8>>,
    /// `None` = keep the stored base picture; when there is none yet (first edit of an image),
    /// the current picture becomes the base before it is replaced.
    pub base_png: Option<Vec<u8>>,
    pub doc: Value,
}

/// A `shot_docs` row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShotDoc {
    pub doc: Value,
    pub has_base: bool,
    pub saved_path: Option<String>,
    /// Last change of the picture or document (Unix ms) — the `?v=` of its URLs.
    pub updated_at: i64,
}

/// What `files.rs` / the editor / pins need to know about a picture record.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShotSource {
    pub kind: Kind,
    /// `media\<id>.png` — the record's picture.
    pub png: PathBuf,
    pub width: i64,
    pub height: i64,
    pub created_at: i64,
    /// Source application name (`{app}` of the file name template).
    pub app: Option<String>,
    pub saved_path: Option<String>,
    /// `?v=` of the picture URLs (last edit, else creation).
    pub version: i64,
}

/// A `shot_docs` row of a `.liqlip` backup being merged, keyed by its record's hash.
#[derive(Debug, Clone)]
pub(crate) struct ShotDocImport {
    pub hash: String,
    pub doc: String,
    /// The backup's row said it had a base picture (`has_base = 1`).
    pub had_base: bool,
    /// That base picture, when the archive holds a valid PNG of it.
    pub base_png: Option<Vec<u8>>,
    pub saved_path: Option<String>,
    pub updated_at: i64,
}

/// `{"v":1,…,"objects":[]}` — the document of a picture without annotations (§18.6).
pub(crate) fn empty_doc(width: i64, height: i64, unit: f64) -> Value {
    let unit = if unit.is_finite() && unit > 0.0 { unit } else { 1.0 };
    json!({ "v": 1, "w": width, "h": height, "unit": unit, "crop": null, "objects": [] })
}

/// The document changes the picture: it has objects or a crop (`ClipMeta.shot.edited`).
pub(crate) fn is_edited(doc: &Value) -> bool {
    let objects = doc.get("objects").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    let crop = doc.get("crop").is_some_and(|c| !c.is_null());
    objects || crop
}

/// W7-R1: the document of a record whose base picture is lost (a partial backup, a file removed
/// by hand) while its picture has the annotations drawn in. Live objects (or a crop) on top of
/// that picture would show twice in the editor, so they go: an empty document of the picture's
/// own size (nothing visible is lost — it is all in the pixels). Any other document is kept.
pub(crate) fn without_lost_base(doc: Value, base_lost: bool, width: Option<i64>, height: Option<i64>) -> Value {
    if !base_lost || !is_edited(&doc) {
        return doc;
    }
    let unit = doc.get("unit").and_then(Value::as_f64).unwrap_or(1.0);
    empty_doc(width.unwrap_or(0), height.unwrap_or(0), unit)
}

fn doc_json(doc: &Value) -> Result<String, String> {
    if !doc.is_object() {
        return Err("invalid: annotation document".into());
    }
    let s = serde_json::to_string(doc).map_err(|e| format!("failed: {e}"))?;
    if s.len() > DOC_MAX_BYTES {
        return Err("invalid: annotation document too large".into());
    }
    Ok(s)
}

fn is_picture(kind: &str) -> bool {
    Kind::parse(kind).is_some_and(Kind::is_image)
}

/// Copy the `shot_docs` row into `clips.meta.shot` (inside the caller's transaction).
fn sync_meta(conn: &Connection, id: i64) -> Result<(), String> {
    let row: Option<(Option<String>, String, Option<String>)> = conn
        .query_row(
            "SELECT c.meta, s.doc, s.saved_path FROM clips c JOIN shot_docs s ON s.clip_id = c.id WHERE c.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(db_err)?;
    let Some((meta, doc, saved_path)) = row else { return Ok(()) };
    let edited = serde_json::from_str::<Value>(&doc).map(|d| is_edited(&d)).unwrap_or(false);
    let mut meta: ClipMeta = parse_meta(meta.as_deref());
    meta.shot = Some(ShotMeta { saved_path, edited });
    conn.execute("UPDATE clips SET meta = ?1 WHERE id = ?2", params![meta_json(&meta), id]).map_err(db_err)?;
    Ok(())
}

/// Stage `bytes` beside the media (outside the DB lock); `None` stays `None`.
fn stage_opt(store: &Store, bytes: Option<&[u8]>) -> Result<Option<PathBuf>, String> {
    bytes.map(|b| media::stage(store.paths(), b)).transpose()
}

fn remove_leftovers(staged: &[&Option<PathBuf>]) {
    let left: Vec<PathBuf> = staged.iter().filter_map(|s| s.as_ref()).filter(|p| p.exists()).cloned().collect();
    media::remove_files(&left);
}

impl Store {
    /// Insert a screenshot (or raise the record with the same pixels — a picture keeps its
    /// kind, `write.rs`), then attach its `shot_docs` row and base picture. The row is a second
    /// short transaction: if it fails the record still exists and opens as a plain picture
    /// (logged). A record that already has a row keeps its document; only a new saved path is
    /// recorded.
    pub(crate) fn insert_screenshot(&self, s: NewShot) -> Result<InsertOutcome, String> {
        let doc = s.doc.clone().unwrap_or_else(|| empty_doc(i64::from(s.width), i64::from(s.height), s.unit));
        let doc = doc_json(&doc)?;
        let staged_base = stage_opt(self, s.base_png.as_deref())?;
        let clip = NewClip {
            kind: Kind::Screenshot,
            hash: s.hash,
            preview: String::new(),
            text: None,
            meta: ClipMeta::default(),
            size: s.png.len() as i64,
            char_count: None,
            word_count: None,
            line_count: None,
            width: Some(i64::from(s.width)),
            height: Some(i64::from(s.height)),
            file_count: None,
            png: Some(s.png),
            thumb_png: s.thumb_png,
            html: None,
            rtf: None,
            files: None,
            source: s.source,
            copied_at: s.copied_at,
        };
        let out = match self.insert_clip(clip) {
            Ok(o) => o,
            Err(e) => {
                remove_leftovers(&[&staged_base]);
                return Err(e);
            }
        };
        if let Err(e) = self.attach_doc(out.id, &doc, staged_base.as_deref(), s.saved_path.as_deref()) {
            logx::warn("store", &format!("screenshot {}: annotation document not stored: {e}", out.id));
        }
        remove_leftovers(&[&staged_base]);
        Ok(out)
    }

    fn attach_doc(&self, id: i64, doc: &str, staged_base: Option<&Path>, saved_path: Option<&str>) -> Result<(), String> {
        let now = self.now();
        let base_dest = self.paths().base_png(id);
        let mut placed = false;
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let res = (|| -> Result<(), String> {
            let kind: String = tx
                .query_row("SELECT kind FROM clips WHERE id = ?1", [id], |r| r.get(0))
                .optional()
                .map_err(db_err)?
                .ok_or_else(|| format!("not_found: clip {id}"))?;
            let has_row: bool = tx
                .query_row("SELECT 1 FROM shot_docs WHERE clip_id = ?1", [id], |_| Ok(()))
                .optional()
                .map_err(db_err)?
                .is_some();
            if has_row {
                if let Some(p) = saved_path {
                    tx.execute("UPDATE shot_docs SET saved_path = ?1 WHERE clip_id = ?2", params![p, id]).map_err(db_err)?;
                }
            } else if kind == Kind::Screenshot.as_str() || saved_path.is_some() || staged_base.is_some() {
                if let Some(staged) = staged_base {
                    media::place(Some(staged), &[], &base_dest)?;
                    placed = true;
                }
                tx.execute(
                    "INSERT INTO shot_docs (clip_id, doc, has_base, saved_path, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, doc, i64::from(placed), saved_path, now],
                )
                .map_err(db_err)?;
            }
            sync_meta(&tx, id)
        })();
        match res.and_then(|()| tx.commit().map_err(db_err)) {
            Ok(()) => Ok(()),
            Err(e) => {
                if placed {
                    media::remove_files(&[base_dest]);
                }
                Err(e)
            }
        }
    }

    /// The `shot_docs` row of a live record (`None` — it has none); `not_found` for a missing
    /// or deleted record. (§18.5 `doc(id)`; the editor payload, W6-B.) A row whose base picture
    /// file is gone reports `has_base: false` and no live objects ([`without_lost_base`]).
    pub(crate) fn shot_doc(&self, id: i64) -> Result<Option<ShotDoc>, String> {
        type Row = (Option<String>, Option<i64>, Option<String>, Option<i64>, Option<i64>, Option<i64>);
        let row: Option<Row> = self
            .lock()
            .query_row(
                "SELECT s.doc, s.has_base, s.saved_path, s.updated_at, c.width, c.height
                 FROM clips c LEFT JOIN shot_docs s ON s.clip_id = c.id WHERE c.id = ?1 AND c.deleted_at IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()
            .map_err(db_err)?;
        let (doc, has_base, saved_path, updated_at, width, height) = row.ok_or_else(|| format!("not_found: clip {id}"))?;
        let (Some(doc), Some(has_base), Some(updated_at)) = (doc, has_base, updated_at) else { return Ok(None) };
        let doc: Value = serde_json::from_str(&doc).unwrap_or(Value::Null);
        let base_lost = has_base != 0 && !self.paths().base_png(id).is_file();
        if base_lost {
            logx::warn("store", &format!("clip {id}: base picture missing — the document opens without live objects"));
        }
        let doc = without_lost_base(doc, base_lost, width, height);
        Ok(Some(ShotDoc { doc, has_base: has_base != 0 && !base_lost, saved_path, updated_at }))
    }

    /// A live record as the feed shows it (`shot:added`, the editor commit's answer).
    pub(crate) fn shot_clip(&self, id: i64) -> Result<Clip, String> {
        clip_by_id(&self.lock(), id)
    }

    /// A live picture record (`not_found` / `invalid: … not a picture`).
    pub(crate) fn shot_source(&self, id: i64) -> Result<ShotSource, String> {
        type Row = (String, i64, Option<i64>, Option<i64>, Option<String>, Option<String>, Option<i64>);
        let row: Option<Row> = self
            .lock()
            .query_row(
                "SELECT c.kind, c.created_at, c.width, c.height, a.name, s.saved_path, s.updated_at
                 FROM clips c LEFT JOIN apps a ON a.id = c.app_id LEFT JOIN shot_docs s ON s.clip_id = c.id
                 WHERE c.id = ?1 AND c.deleted_at IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
            )
            .optional()
            .map_err(db_err)?;
        let (kind, created_at, width, height, app, saved_path, updated_at) = row.ok_or_else(|| format!("not_found: clip {id}"))?;
        let kind = Kind::parse(&kind).filter(|k| k.is_image()).ok_or_else(|| format!("invalid: clip {id} is not a picture"))?;
        Ok(ShotSource {
            kind,
            png: self.paths().media_png(id),
            width: width.unwrap_or(0),
            height: height.unwrap_or(0),
            created_at,
            app: app.filter(|a| !a.trim().is_empty()),
            saved_path,
            version: updated_at.unwrap_or(created_at),
        })
    }

    /// `liq://…/base/<id>`: the picture without annotations of a live picture record —
    /// `media\base\<id>.png` when the row says it exists, else the record's picture.
    pub(crate) fn serve_base(&self, id: i64) -> Option<Vec<u8>> {
        let has_base: Option<Option<i64>> = self
            .lock()
            .query_row(
                "SELECT s.has_base FROM clips c LEFT JOIN shot_docs s ON s.clip_id = c.id
                 WHERE c.id = ?1 AND c.kind IN ('image', 'screenshot') AND c.deleted_at IS NULL",
                [id],
                |r| r.get(0),
            )
            .optional()
            .ok()?;
        let has_base = has_base?.unwrap_or(0) != 0;
        if has_base {
            if let Ok(bytes) = std::fs::read(self.paths().base_png(id)) {
                return Some(bytes);
            }
        }
        std::fs::read(self.paths().media_png(id)).ok()
    }

    /// The editor's "Done": replace the picture of a live picture record (PNG, thumbnail,
    /// hash, size, width/height), keep its base picture (the first edit of an image saves the
    /// current picture as the base), store the document, queue the new picture for OCR. A hash
    /// already taken by another record gets a `-e<id>` suffix (two records may then show the
    /// same pixels — dedup is for captures, an edit never merges records). The caller emits
    /// `clips:changed { origin: 'user', change: 'updated' }`.
    pub(crate) fn apply_edit(&self, id: i64, e: ShotEdit) -> Result<Clip, String> {
        let doc = doc_json(&e.doc)?;
        let has_base = {
            let conn = self.lock();
            let row: Option<(String, Option<i64>)> = conn
                .query_row(
                    "SELECT c.kind, s.has_base FROM clips c LEFT JOIN shot_docs s ON s.clip_id = c.id
                     WHERE c.id = ?1 AND c.deleted_at IS NULL",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db_err)?;
            let (kind, has_base) = row.ok_or_else(|| format!("not_found: clip {id}"))?;
            if !is_picture(&kind) {
                return Err(format!("invalid: clip {id} is not a picture"));
            }
            has_base.unwrap_or(0) != 0 && self.paths().base_png(id).exists()
        };
        // Files first, outside the lock.
        let base_bytes = match (&e.base_png, has_base) {
            (Some(b), _) => Some(b.clone()),
            (None, false) => Some(std::fs::read(self.paths().media_png(id)).map_err(|_| format!("not_found: picture of clip {id}"))?),
            (None, true) => None,
        };
        let staged_base = stage_opt(self, base_bytes.as_deref())?;
        drop(base_bytes);
        let staged_png = match media::stage(self.paths(), &e.png) {
            Ok(p) => Some(p),
            Err(err) => {
                remove_leftovers(&[&staged_base]);
                return Err(err);
            }
        };
        let staged_thumb = match stage_opt(self, e.thumb_png.as_deref()) {
            Ok(p) => p,
            Err(err) => {
                remove_leftovers(&[&staged_base, &staged_png]);
                return Err(err);
            }
        };
        let now = self.now();
        let result = (|| -> Result<(), String> {
            let mut conn = self.lock();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
            let res = (|| -> Result<(), String> {
                let live: bool = tx
                    .query_row("SELECT 1 FROM clips WHERE id = ?1 AND deleted_at IS NULL", [id], |_| Ok(()))
                    .optional()
                    .map_err(db_err)?
                    .is_some();
                if !live {
                    return Err(format!("not_found: clip {id}"));
                }
                let taken: bool = tx
                    .query_row("SELECT 1 FROM clips WHERE hash = ?1 AND id <> ?2", params![e.hash, id], |_| Ok(()))
                    .optional()
                    .map_err(db_err)?
                    .is_some();
                let hash = if taken { format!("{}-e{id}", e.hash) } else { e.hash.clone() };
                tx.execute(
                    "UPDATE clips SET hash = ?1, width = ?2, height = ?3, ocr_text = NULL, ocr_state = 0 WHERE id = ?4",
                    params![hash, i64::from(e.width), i64::from(e.height), id],
                )
                .map_err(db_err)?;
                self.ocr_enqueue_in(&tx, id)?;
                tx.execute(
                    "INSERT INTO shot_docs (clip_id, doc, has_base, saved_path, updated_at) VALUES (?1, ?2, 1, NULL, ?3)
                     ON CONFLICT (clip_id) DO UPDATE SET doc = excluded.doc, has_base = 1,
                       updated_at = max(excluded.updated_at, shot_docs.updated_at + 1)",
                    params![id, doc, now],
                )
                .map_err(db_err)?;
                sync_meta(&tx, id)?;
                upsert_format(&tx, id, "png", None, Some(&media_rel(id)), e.png.len())?;
                recompute_size(&tx, id)?;
                // Files last: a failed rename rolls the row back (the order keeps the base
                // picture safe — it is placed before the picture it preserves is replaced).
                if let Some(b) = &staged_base {
                    media::place(Some(b.as_path()), &[], &self.paths().base_png(id))?;
                }
                if let Some(t) = &staged_thumb {
                    media::place(Some(t.as_path()), &[], &self.paths().thumb_png(id))?;
                }
                if let Some(p) = &staged_png {
                    media::place(Some(p.as_path()), &[], &self.paths().media_png(id))?;
                }
                Ok(())
            })();
            res.and_then(|()| tx.commit().map_err(db_err))
        })();
        remove_leftovers(&[&staged_base, &staged_png, &staged_thumb]);
        result?;
        if e.thumb_png.is_none() {
            // The old preview no longer matches: `/thumb` rebuilds it from the new picture.
            media::remove_files(&[self.paths().thumb_png(id)]);
        }
        self.ocr_wake();
        clip_by_id(&self.lock(), id)
    }

    /// Record (or clear) the last file written to the screenshot folder for a live picture
    /// record; an image without a row gets one with an empty document.
    pub(crate) fn set_saved_path(&self, id: i64, path: Option<&str>) -> Result<(), String> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let row: Option<(String, Option<i64>, Option<i64>, Option<i64>)> = tx
            .query_row(
                "SELECT c.kind, c.width, c.height, s.clip_id FROM clips c LEFT JOIN shot_docs s ON s.clip_id = c.id
                 WHERE c.id = ?1 AND c.deleted_at IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(db_err)?;
        let (kind, w, h, has_row) = row.ok_or_else(|| format!("not_found: clip {id}"))?;
        if !is_picture(&kind) {
            return Err(format!("invalid: clip {id} is not a picture"));
        }
        if has_row.is_some() {
            tx.execute("UPDATE shot_docs SET saved_path = ?1 WHERE clip_id = ?2", params![path, id]).map_err(db_err)?;
        } else {
            let doc = doc_json(&empty_doc(w.unwrap_or(0), h.unwrap_or(0), 1.0))?;
            tx.execute(
                "INSERT INTO shot_docs (clip_id, doc, has_base, saved_path, updated_at) VALUES (?1, ?2, 0, ?3, ?4)",
                params![id, doc, path, now],
            )
            .map_err(db_err)?;
        }
        sync_meta(&tx, id)?;
        tx.commit().map_err(db_err)
    }

    /// The §18.5 invariant — every `screenshot` has a `shot_docs` row — for records that came in
    /// without one: the Liquid Tools import (W8-A; its shots carry no annotation document, any
    /// annotations are pixels already). Each gets an empty document of the picture's size, `unit`
    /// 1 (Liquid Tools keeps no monitor scale; the editor's default too), no base picture
    /// (`has_base = 0`), no saved file. Rows already there are never touched. Returns how many
    /// were added.
    pub(crate) fn attach_missing_shot_docs(&self) -> Result<i64, String> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        let rows: Vec<(i64, Option<i64>, Option<i64>)> = {
            let mut st = tx
                .prepare(
                    "SELECT c.id, c.width, c.height FROM clips c LEFT JOIN shot_docs s ON s.clip_id = c.id
                     WHERE c.kind = ?1 AND s.clip_id IS NULL",
                )
                .map_err(db_err)?;
            let found = st.query_map([Kind::Screenshot.as_str()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(db_err)?;
            found.collect::<Result<_, _>>().map_err(db_err)?
        };
        for &(id, w, h) in &rows {
            let doc = doc_json(&empty_doc(w.unwrap_or(0), h.unwrap_or(0), 1.0))?;
            tx.execute(
                "INSERT INTO shot_docs (clip_id, doc, has_base, saved_path, updated_at) VALUES (?1, ?2, 0, NULL, ?3)",
                params![id, doc, now],
            )
            .map_err(db_err)?;
            sync_meta(&tx, id)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(rows.len() as i64)
    }

    /// `.liqlip` merge (§17.5): attach a backup's documents to the records the import created
    /// or found (by hash). A record that already has a row keeps it; a malformed document is
    /// skipped; a row whose base picture is missing from the archive loses its live objects
    /// ([`without_lost_base`]). Returns how many rows were added.
    pub(crate) fn import_shot_docs(&self, rows: Vec<ShotDocImport>) -> Result<i64, String> {
        let now = self.now();
        let mut added = 0;
        for r in rows {
            let Ok(doc) = serde_json::from_str::<Value>(&r.doc) else { continue };
            if doc_json(&doc).is_err() {
                continue;
            }
            let staged = stage_opt(self, r.base_png.as_deref().filter(|b| media::is_png(b)))?;
            let res = (|| -> Result<bool, String> {
                let mut conn = self.lock();
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
                type Target = (i64, String, Option<i64>, Option<i64>, Option<i64>);
                let target: Option<Target> = tx
                    .query_row(
                        "SELECT c.id, c.kind, s.clip_id, c.width, c.height FROM clips c LEFT JOIN shot_docs s ON s.clip_id = c.id
                         WHERE c.hash = ?1 AND c.deleted_at IS NULL",
                        [&r.hash],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                    )
                    .optional()
                    .map_err(db_err)?;
                let Some((id, kind, None, width, height)) = target else { return Ok(false) };
                if !is_picture(&kind) {
                    return Ok(false);
                }
                let doc = doc_json(&without_lost_base(doc.clone(), r.had_base && staged.is_none(), width, height))?;
                let base_dest = self.paths().base_png(id);
                let has_base = staged.is_some();
                tx.execute(
                    "INSERT INTO shot_docs (clip_id, doc, has_base, saved_path, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, doc, i64::from(has_base), r.saved_path, if r.updated_at > 0 { r.updated_at } else { now }],
                )
                .map_err(db_err)?;
                sync_meta(&tx, id)?;
                if let Some(s) = &staged {
                    media::place(Some(s.as_path()), &[], &base_dest)?;
                }
                if let Err(e) = tx.commit() {
                    if has_base {
                        media::remove_files(&[base_dest]);
                    }
                    return Err(db_err(e));
                }
                Ok(true)
            })();
            remove_leftovers(&[&staged]);
            if res? {
                added += 1;
            }
        }
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{png_bytes, TestStore};
    use super::*;
    use crate::content;

    fn rgba(w: u32, h: u32, seed: u8) -> Vec<u8> {
        (0..w * h).flat_map(|i| [(i % 251) as u8, seed, (i / 7 % 256) as u8, 255]).collect()
    }

    fn encode(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::from_raw(w, h, px.to_vec())
            .unwrap()
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn shot(w: u32, h: u32, seed: u8, at: i64) -> NewShot {
        let px = rgba(w, h, seed);
        NewShot {
            png: encode(w, h, &px),
            width: w,
            height: h,
            hash: content::image_hash(w, h, &px),
            thumb_png: None,
            base_png: None,
            doc: None,
            unit: 1.5,
            saved_path: None,
            source: Some(SourceApp { exe: "c:\\apps\\chat.exe".into(), name: "Chat".into() }),
            copied_at: at,
        }
    }

    fn kind_of(t: &TestStore, id: i64) -> String {
        t.lock().query_row("SELECT kind FROM clips WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn a_plain_shot_is_a_screenshot_with_an_empty_document() {
        let t = TestStore::new();
        let out = t.insert_screenshot(shot(8, 6, 1, 1_000)).unwrap();
        assert!(out.created);
        assert_eq!(kind_of(&t, out.id), "screenshot");
        let d = t.shot_doc(out.id).unwrap().expect("row");
        assert_eq!(d.doc, empty_doc(8, 6, 1.5));
        assert!(!d.has_base);
        assert_eq!(d.saved_path, None);
        let clip = clip_by_id(&t.lock(), out.id).unwrap();
        assert_eq!(clip.kind, Kind::Screenshot);
        assert_eq!(clip.meta.shot, Some(ShotMeta { saved_path: None, edited: false }));
        assert_eq!((clip.width, clip.height), (Some(8), Some(6)));
        assert!(clip.thumb_url.is_some() && clip.formats == vec![crate::model::Format::Png]);
        let src = t.shot_source(out.id).unwrap();
        assert_eq!((src.kind, src.app.as_deref()), (Kind::Screenshot, Some("Chat")));
        // Served like an image; no base picture → `/base` is the picture itself.
        assert!(t.serve_path(&format!("/thumb/{}", out.id)).is_some());
        assert_eq!(t.serve_base(out.id), std::fs::read(t.paths().media_png(out.id)).ok());
    }

    #[test]
    fn an_annotated_shot_keeps_its_base_and_document() {
        let t = TestStore::new();
        let mut s = shot(8, 6, 2, 1_000);
        let base = encode(8, 6, &rgba(8, 6, 99));
        s.base_png = Some(base.clone());
        s.doc = Some(json!({ "v": 1, "w": 8, "h": 6, "unit": 2, "crop": null, "objects": [{ "id": "a", "kind": "line" }] }));
        s.saved_path = Some("D:\\Shots\\a.png".into());
        let id = t.insert_screenshot(s).unwrap().id;
        let d = t.shot_doc(id).unwrap().unwrap();
        assert!(d.has_base && is_edited(&d.doc));
        assert_eq!(d.saved_path.as_deref(), Some("D:\\Shots\\a.png"));
        assert_eq!(t.serve_base(id), Some(base));
        let meta = clip_by_id(&t.lock(), id).unwrap().meta.shot.unwrap();
        assert!(meta.edited);
        assert_eq!(meta.saved_path.as_deref(), Some("D:\\Shots\\a.png"));
        // Deleting the record removes the base picture and the row.
        t.hard_delete(&[id]).unwrap();
        assert!(!t.paths().base_png(id).exists());
        let rows: i64 = t.lock().query_row("SELECT count(*) FROM shot_docs", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn the_same_pixels_raise_the_record_and_an_image_stays_an_image() {
        let t = TestStore::new();
        let first = t.insert_screenshot(shot(8, 6, 3, 1_000)).unwrap();
        let again = t.insert_screenshot(shot(8, 6, 3, 2_000)).unwrap();
        assert_eq!((again.id, again.created), (first.id, false));
        let copied_at: i64 = t.lock().query_row("SELECT copied_at FROM clips WHERE id = ?1", [first.id], |r| r.get(0)).unwrap();
        assert_eq!(copied_at, 2_000, "raised to the top like a copy");
        // A clipboard image with the same pixels as a shot keeps the screenshot…
        let mut img = super::super::testutil::image_clip(4, 4, 3_000, false);
        let px = rgba(4, 4, 7);
        img.png = Some(encode(4, 4, &px));
        img.hash = content::image_hash(4, 4, &px);
        let mut as_shot = shot(4, 4, 7, 2_500);
        as_shot.saved_path = Some("C:\\x.png".into());
        let s = t.insert_screenshot(as_shot).unwrap();
        assert_eq!(t.insert_clip(img).unwrap().id, s.id);
        assert_eq!(kind_of(&t, s.id), "screenshot");
        // …and a shot equal to an existing image raises the image, which stays an image.
        let image_id = t.add_image(5, 5, 4_000, false);
        let mut same = shot(5, 5, 0, 5_000);
        let px = image::RgbaImage::from_fn(5, 5, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
        same.png = png_bytes(5, 5);
        same.hash = content::image_hash(5, 5, px.as_raw());
        assert_eq!(t.insert_screenshot(same).unwrap().id, image_id);
        assert_eq!(kind_of(&t, image_id), "image");
        assert_eq!(t.shot_doc(image_id).unwrap(), None, "an unsaved, unedited image gets no row");
    }

    #[test]
    fn the_first_edit_of_an_image_saves_its_picture_as_the_base() {
        let t = TestStore::new();
        let id = t.add_image(6, 4, 1_000, true);
        let original = std::fs::read(t.paths().media_png(id)).unwrap();
        let before = clip_by_id(&t.lock(), id).unwrap();
        let px = rgba(6, 4, 42);
        t.set_now(2_000_000_000);
        let edit = ShotEdit {
            png: encode(6, 4, &px),
            width: 6,
            height: 4,
            hash: content::image_hash(6, 4, &px),
            thumb_png: None,
            base_png: None,
            doc: json!({ "v": 1, "w": 6, "h": 4, "unit": 1, "crop": null, "objects": [{ "id": "t", "kind": "text" }] }),
        };
        let clip = t.apply_edit(id, edit.clone()).unwrap();
        assert_eq!(clip.kind, Kind::Image, "an edited image stays an image");
        assert!(clip.meta.shot.as_ref().unwrap().edited);
        assert_ne!(clip.thumb_url, before.thumb_url, "a new ?v= for the edited picture");
        assert_eq!(std::fs::read(t.paths().base_png(id)).unwrap(), original);
        assert_eq!(std::fs::read(t.paths().media_png(id)).unwrap(), edit.png);
        assert!(!t.paths().thumb_png(id).exists(), "the stale preview is dropped");
        let hash: String = t.lock().query_row("SELECT hash FROM clips WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(hash, edit.hash);
        // A second edit keeps the first base and bumps the version again.
        let v1 = t.shot_doc(id).unwrap().unwrap().updated_at;
        let mut again = edit.clone();
        again.doc = json!({ "v": 1, "w": 6, "h": 4, "unit": 1, "crop": null, "objects": [] });
        t.apply_edit(id, again).unwrap();
        let d = t.shot_doc(id).unwrap().unwrap();
        assert!(d.updated_at > v1 && d.has_base && !is_edited(&d.doc));
        assert_eq!(std::fs::read(t.paths().base_png(id)).unwrap(), original);
        // An edit that equals another record's pixels does not merge them.
        let other = t.add_image(7, 7, 3_000, false);
        let px7 = image::RgbaImage::from_fn(7, 7, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
        let clash = ShotEdit { png: png_bytes(7, 7), width: 7, height: 7, hash: content::image_hash(7, 7, px7.as_raw()), ..edit };
        t.apply_edit(id, clash).unwrap();
        assert!(clip_by_id(&t.lock(), other).is_ok());
        let hash: String = t.lock().query_row("SELECT hash FROM clips WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert!(hash.ends_with(&format!("-e{id}")), "{hash}");
        // Text records cannot be edited as pictures.
        let txt = t.add_text("words", 4_000);
        assert!(t.apply_edit(txt, ShotEdit { png: png_bytes(2, 2), width: 2, height: 2, hash: "x".into(), thumb_png: None, base_png: None, doc: json!({}) }).unwrap_err().starts_with("invalid:"));
    }

    #[test]
    fn saved_path_is_recorded_for_shots_and_images() {
        let t = TestStore::new();
        let id = t.insert_screenshot(shot(4, 4, 5, 1_000)).unwrap().id;
        t.set_saved_path(id, Some("C:\\a.png")).unwrap();
        assert_eq!(t.shot_doc(id).unwrap().unwrap().saved_path.as_deref(), Some("C:\\a.png"));
        assert_eq!(t.shot_source(id).unwrap().saved_path.as_deref(), Some("C:\\a.png"));
        let img = t.add_image(3, 3, 2_000, false);
        t.set_saved_path(img, Some("C:\\b.png")).unwrap();
        let d = t.shot_doc(img).unwrap().unwrap();
        assert_eq!((d.saved_path.as_deref(), d.has_base), (Some("C:\\b.png"), false));
        assert_eq!(clip_by_id(&t.lock(), img).unwrap().meta.shot.unwrap().saved_path.as_deref(), Some("C:\\b.png"));
        let txt = t.add_text("not a picture", 3_000);
        assert!(t.set_saved_path(txt, Some("C:\\c.png")).unwrap_err().starts_with("invalid:"));
        assert!(t.set_saved_path(9_999, None).unwrap_err().starts_with("not_found:"));
        t.delete(&[id]).unwrap();
        assert!(t.shot_source(id).unwrap_err().starts_with("not_found:"));
        assert!(t.serve_base(id).is_none());
    }

    #[test]
    fn orphan_base_pictures_are_swept() {
        let t = TestStore::new();
        let mut s = shot(4, 4, 6, 1_000);
        s.base_png = Some(png_bytes(4, 4));
        let id = t.insert_screenshot(s).unwrap().id;
        std::fs::write(t.paths().base_png(9_999), b"orphan").unwrap();
        assert_eq!(t.sweep_orphans().unwrap(), 1);
        assert!(t.paths().base_png(id).exists());
        assert!(!t.paths().base_png(9_999).exists());
    }

    #[test]
    fn imported_documents_attach_by_hash_once() {
        let t = TestStore::new();
        let s = shot(4, 4, 8, 1_000);
        let hash = s.hash.clone();
        let id = t.insert_screenshot(s).unwrap().id;
        t.lock().execute("DELETE FROM shot_docs WHERE clip_id = ?1", [id]).unwrap();
        let row = |doc: &str| ShotDocImport {
            hash: hash.clone(),
            doc: doc.into(),
            had_base: true,
            base_png: Some(png_bytes(4, 4)),
            saved_path: Some("E:\\old.png".into()),
            updated_at: 5,
        };
        assert_eq!(t.import_shot_docs(vec![row("not json"), row("{\"v\":1,\"objects\":[{}]}")]).unwrap(), 1);
        let d = t.shot_doc(id).unwrap().unwrap();
        assert!(d.has_base && is_edited(&d.doc));
        assert!(t.paths().base_png(id).exists());
        assert_eq!(t.import_shot_docs(vec![row("{\"v\":1}")]).unwrap(), 0, "an existing row is kept");
        let unknown = ShotDocImport { hash: "nope".into(), ..row("{}") };
        assert_eq!(t.import_shot_docs(vec![unknown]).unwrap(), 0);
    }

    /// W7-R1: annotations drawn into the picture are not drawn a second time as live objects when
    /// the base picture is gone — neither after a partial restore nor after a merge without it.
    #[test]
    fn a_lost_base_picture_drops_the_live_objects() {
        let doc = json!({ "v": 1, "w": 8, "h": 6, "unit": 2, "crop": null, "objects": [{ "id": "a", "kind": "arrow" }] });
        assert_eq!(without_lost_base(doc.clone(), false, Some(8), Some(6)), doc, "base present → kept");
        assert_eq!(without_lost_base(empty_doc(8, 6, 2.0), true, Some(8), Some(6)), empty_doc(8, 6, 2.0));
        assert_eq!(without_lost_base(doc.clone(), true, Some(5), Some(4)), empty_doc(5, 4, 2.0), "the picture's size");

        // The base file of an annotated shot disappears.
        let t = TestStore::new();
        let mut s = shot(8, 6, 21, 1_000);
        s.base_png = Some(encode(8, 6, &rgba(8, 6, 50)));
        s.doc = Some(doc.clone());
        let id = t.insert_screenshot(s).unwrap().id;
        assert_eq!(t.shot_doc(id).unwrap().unwrap().doc, doc);
        std::fs::remove_file(t.paths().base_png(id)).unwrap();
        let d = t.shot_doc(id).unwrap().unwrap();
        assert_eq!((d.doc, d.has_base), (empty_doc(8, 6, 2.0), false));

        // A merged row whose base picture the archive lacks.
        let t = TestStore::new();
        let plain = shot(8, 6, 22, 1_000);
        let hash = plain.hash.clone();
        let id = t.insert_screenshot(plain).unwrap().id;
        t.lock().execute("DELETE FROM shot_docs WHERE clip_id = ?1", [id]).unwrap();
        let row = ShotDocImport { hash, doc: doc.to_string(), had_base: true, base_png: None, saved_path: None, updated_at: 5 };
        assert_eq!(t.import_shot_docs(vec![row]).unwrap(), 1);
        let d = t.shot_doc(id).unwrap().unwrap();
        assert_eq!((d.doc, d.has_base), (empty_doc(8, 6, 2.0), false));
        assert!(!clip_by_id(&t.lock(), id).unwrap().meta.shot.unwrap().edited);
    }

    /// W8-A: a screenshot that came in without a `shot_docs` row (the Liquid Tools import) gets the
    /// empty one; rows already there — and other kinds — stay as they are.
    #[test]
    fn screenshots_without_a_document_get_the_empty_one() {
        let t = TestStore::new();
        let mut annotated = shot(8, 6, 31, 1_000);
        annotated.base_png = Some(encode(8, 6, &rgba(8, 6, 32)));
        annotated.doc = Some(json!({ "v": 1, "w": 8, "h": 6, "unit": 2, "crop": null, "objects": [{ "id": "a" }] }));
        let kept = t.insert_screenshot(annotated).unwrap().id;
        let bare = t.insert_screenshot(shot(5, 4, 33, 2_000)).unwrap().id;
        t.lock().execute("DELETE FROM shot_docs WHERE clip_id = ?1", [bare]).unwrap();
        let image = t.add_image(3, 3, 3_000, false);
        let before = t.shot_doc(kept).unwrap();
        assert_eq!(t.attach_missing_shot_docs().unwrap(), 1);
        let d = t.shot_doc(bare).unwrap().expect("the row now exists");
        assert_eq!((d.doc, d.has_base, d.saved_path), (empty_doc(5, 4, 1.0), false, None));
        assert_eq!(clip_by_id(&t.lock(), bare).unwrap().meta.shot, Some(ShotMeta { saved_path: None, edited: false }));
        assert_eq!(t.shot_doc(kept).unwrap(), before, "an existing row is never touched");
        assert_eq!(t.shot_doc(image).unwrap(), None, "an image gets a row only when edited or saved");
        assert_eq!(t.attach_missing_shot_docs().unwrap(), 0, "idempotent");
    }

    #[test]
    fn documents_are_validated() {
        assert!(doc_json(&json!([1])).unwrap_err().starts_with("invalid:"));
        assert!(doc_json(&json!({ "v": 1 })).is_ok());
        assert!(!is_edited(&empty_doc(1, 1, 1.0)));
        assert!(is_edited(&json!({ "crop": { "x": 0 } })));
        assert_eq!(empty_doc(2, 3, f64::NAN)["unit"], json!(1.0));
    }
}
