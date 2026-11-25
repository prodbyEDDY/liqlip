//! Test harness: a `Store` in a fresh temporary directory (deleted on drop), a manual
//! clock and builders for `NewClip`s. Data stays small (≤ 2 000 rows per test).

use super::{ClipFiles, InsertOutcome, NewClip, SourceApp, Store};
use crate::content;
use crate::model::{ClipMeta, Kind};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

static SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TestStore {
    store: Option<Store>,
    dir: PathBuf,
    now: Arc<AtomicI64>,
}

impl TestStore {
    /// A store in a new temp dir; the clock starts at 10 000 000 ms.
    pub fn new() -> TestStore {
        TestStore::empty_dir().open()
    }

    /// Only the directory (no database yet) — for open/recovery tests.
    pub fn empty_dir() -> TestStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "liqlip-store-test-{}-{}-{nanos}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TestStore { store: None, dir, now: Arc::new(AtomicI64::new(10_000_000)) }
    }

    pub fn try_open(&self) -> Result<Store, String> {
        let now = self.now.clone();
        Store::open_with_clock(&self.dir, Box::new(move || now.load(Ordering::SeqCst)))
    }

    pub fn open(mut self) -> TestStore {
        self.store = Some(self.try_open().unwrap());
        self
    }

    /// Close the database (keep the directory).
    pub fn close(mut self) -> TestStore {
        self.store = None;
        self
    }

    pub fn reopen(self) -> TestStore {
        self.close().open()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn set_now(&self, ms: i64) {
        self.now.store(ms, Ordering::SeqCst);
    }

    pub fn add_text(&self, text: &str, at: i64) -> i64 {
        self.insert_clip(text_clip(text, at)).unwrap().id
    }

    pub fn add_text_from(&self, text: &str, at: i64, exe: &str) -> InsertOutcome {
        let mut c = text_clip(text, at);
        c.source = Some(SourceApp { exe: exe.into(), name: "App".into() });
        self.insert_clip(c).unwrap()
    }

    pub fn add_image(&self, w: u32, h: u32, at: i64, with_thumb: bool) -> i64 {
        self.insert_clip(image_clip(w, h, at, with_thumb)).unwrap().id
    }

    pub fn add_files(&self, paths: &[&str], at: i64) -> i64 {
        self.insert_clip(files_clip(paths, at)).unwrap().id
    }
}

impl Deref for TestStore {
    type Target = Store;
    fn deref(&self) -> &Store {
        self.store.as_ref().expect("store is closed")
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        self.store = None; // close the DB first: Windows can't delete open files
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub(crate) fn text_clip(text: &str, at: i64) -> NewClip {
    let m = content::text_metrics(text);
    NewClip {
        kind: Kind::Text,
        hash: content::content_hash(Kind::Text, content::canonical_text(text)),
        preview: content::make_preview(text),
        text: Some(text.to_string()),
        meta: ClipMeta::default(),
        size: text.len() as i64,
        char_count: Some(m.chars),
        word_count: Some(m.words),
        line_count: Some(m.lines),
        width: None,
        height: None,
        file_count: None,
        png: None,
        thumb_png: None,
        html: None,
        rtf: None,
        files: None,
        source: None,
        copied_at: at,
    }
}

/// A deterministic RGBA test image, PNG-encoded.
pub(crate) fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

pub(crate) fn image_clip(w: u32, h: u32, at: i64, with_thumb: bool) -> NewClip {
    let png = png_bytes(w, h);
    let rgba = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
    NewClip {
        kind: Kind::Image,
        hash: content::image_hash(w, h, rgba.as_raw()),
        preview: String::new(),
        text: None,
        meta: ClipMeta::default(),
        size: png.len() as i64,
        char_count: None,
        word_count: None,
        line_count: None,
        width: Some(i64::from(w)),
        height: Some(i64::from(h)),
        file_count: None,
        thumb_png: with_thumb.then(|| super::media::make_thumbnail(&png).unwrap()),
        png: Some(png),
        html: None,
        rtf: None,
        files: None,
        source: None,
        copied_at: at,
    }
}

pub(crate) fn files_clip(paths: &[&str], at: i64) -> NewClip {
    let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    let joined = paths.join("\n");
    NewClip {
        kind: Kind::File,
        hash: content::content_hash(Kind::File, &content::canonical_files(&paths)),
        preview: paths.first().map(|p| p.rsplit('\\').next().unwrap_or(p).to_string()).unwrap_or_default(),
        text: Some(joined),
        meta: ClipMeta::default(),
        size: 0,
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
        files: Some(ClipFiles { paths, drop_effect: 1 }),
        source: None,
        copied_at: at,
    }
}
