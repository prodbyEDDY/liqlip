//! Import / backup tests on synthetic data only (CLAUDE.md, dispatch W2: never the real
//! Liquid Tools database): a Liquid Tools-schema database built in a temporary folder
//! (≤ 50 rows of every kind, a screenshot and a "dictation" that must be skipped), stores in
//! temporary folders, everything removed on drop.

use super::runner::Progress;
use super::tmp::TempDir;
use super::{backup, lt};
use crate::content;
use crate::model::{ClipMeta, Kind, SnippetInput};
use crate::store::{ClipFiles, NewClip, SourceApp, Store};
use image::{ImageFormat, ImageReader, Rgba, RgbaImage};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// "Now" of the test clock (2026-09).
const T: i64 = 1_790_000_000_000;
const DAY: i64 = 86_400_000;

fn quiet() -> impl Fn(&str, i64, i64) {
    |_: &str, _: i64, _: i64| {}
}

/// A store in a temporary folder with a manual clock. Field order: the store closes first.
struct Kit {
    store: Store,
    now: Arc<AtomicI64>,
    dir: TempDir,
}

impl Kit {
    fn new() -> Kit {
        let dir = TempDir::new("liqlip-import-test-").unwrap();
        let now = Arc::new(AtomicI64::new(T));
        let n = now.clone();
        let store = Store::open_with_clock(&dir.path().join("data"), Box::new(move || n.load(Ordering::SeqCst))).unwrap();
        Kit { store, now, dir }
    }

    fn cutoff(&self) -> Option<i64> {
        self.store.retention_cutoff(crate::model::Retention::Month3)
    }

    fn one<V: rusqlite::types::FromSql>(&self, sql: &str, p: impl rusqlite::Params) -> V {
        self.store.lock().query_row(sql, p, |r| r.get(0)).unwrap()
    }

    fn id_of(&self, text: &str) -> i64 {
        self.one("SELECT id FROM clips WHERE text = ?1", [text])
    }
}

fn png(w: u32, h: u32, alpha: u8) -> Vec<u8> {
    let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x % 251) as u8, (y % 241) as u8, 77, alpha]));
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).unwrap();
    out
}

fn decode(bytes: &[u8]) -> RgbaImage {
    ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png).decode().unwrap().into_rgba8()
}

// ── synthetic Liquid Tools database ─────────────────────────────────────────

/// Liquid Tools `clipboard/store.rs`: `SCHEMA` + every additive migration.
const LT_SCHEMA: &str = "
CREATE TABLE clipboard_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, title TEXT NOT NULL, preview TEXT NOT NULL,
    content_ref TEXT, source_app TEXT, size INTEGER, width INTEGER, height INTEGER,
    created_at INTEGER NOT NULL, pinned INTEGER NOT NULL DEFAULT 0);
CREATE INDEX idx_entries_created ON clipboard_entries(created_at);
CREATE TABLE categories (name TEXT PRIMARY KEY, position INTEGER NOT NULL);
ALTER TABLE clipboard_entries ADD COLUMN html TEXT;
ALTER TABLE clipboard_entries ADD COLUMN category TEXT NOT NULL DEFAULT '';
ALTER TABLE clipboard_entries ADD COLUMN thumb TEXT;
ALTER TABLE clipboard_entries ADD COLUMN app_icon TEXT;
ALTER TABLE clipboard_entries ADD COLUMN ocr_text TEXT;
CREATE INDEX idx_entries_feed ON clipboard_entries(pinned DESC, created_at DESC, id DESC);
CREATE INDEX idx_entries_kind ON clipboard_entries(kind);
CREATE INDEX idx_entries_category ON clipboard_entries(category);";

#[derive(Default)]
struct Row<'a> {
    kind: &'a str,
    preview: &'a str,
    content_ref: Option<&'a str>,
    source: Option<&'a str>,
    at: i64,
    pinned: bool,
    category: &'a str,
    html: Option<&'a str>,
    app_icon: Option<String>,
}

fn ins(c: &Connection, r: Row<'_>) -> i64 {
    c.execute(
        "INSERT INTO clipboard_entries (kind, title, preview, content_ref, source_app, created_at, pinned, category, html, app_icon)
         VALUES (?1, 'auto', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![r.kind, r.preview, r.content_ref, r.source, r.at, i64::from(r.pinned), r.category, r.html, r.app_icon],
    )
    .unwrap();
    c.last_insert_rowid()
}

const LONG_WORD: &str = "lorem ";
/// A CF_HTML payload exactly as Liquid Tools stored it (header with byte offsets).
fn cf_html(fragment: &str, url: &str) -> String {
    let (pre, post) = ("<html><body><!--StartFragment-->", "<!--EndFragment--></body></html>");
    let header = |a: usize, b: usize, c: usize, d: usize| {
        format!(
            "Version:0.9\r\nStartHTML:{a:010}\r\nEndHTML:{b:010}\r\nStartFragment:{c:010}\r\nEndFragment:{d:010}\r\nSourceURL:{url}\r\n"
        )
    };
    let start_html = header(0, 0, 0, 0).len();
    let start_frag = start_html + pre.len();
    let end_frag = start_frag + fragment.len();
    let end_html = end_frag + post.len();
    format!("{}{pre}{fragment}{post}", header(start_html, end_html, start_frag, end_frag))
}

struct LtFixture {
    dir: TempDir,
    db: PathBuf,
    image: Vec<u8>,
    transparent: Vec<u8>,
}

/// 22 rows: 20 counted after the 3-month retention filter (2 of them unreadable, 1 a
/// duplicate, 1 a screenshot outside Liquid Tools' cache — skipped), a "dictation", an
/// ancient history row; 5 folders. (Screenshot import itself: `lt::tests`, W6-B.)
fn lt_fixture() -> LtFixture {
    let dir = TempDir::new("liqlip-lt-fixture-").unwrap();
    let clip_dir = dir.path().join("com.liquidtools.app").join("clipboard");
    std::fs::create_dir_all(&clip_dir).unwrap();
    let db = clip_dir.join("clipboard.db");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(LT_SCHEMA).unwrap();
    for (i, name) in ["Work", "Личное", "Empty", "work", "Shots"].iter().enumerate() {
        c.execute("INSERT INTO categories (name, position) VALUES (?1, ?2)", params![name, i as i64 + 1]).unwrap();
    }
    let long = LONG_WORD.repeat(70); // 420 chars
    let long_preview: String = long.trim().chars().take(279).chain(std::iter::once('…')).collect();
    let icon = format!(
        "data:image/png;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png(16, 16, 255))
    );
    let image = png(500, 300, 255);
    let transparent = png(20, 10, 0);
    let ago = |ms: i64| T - ms;
    ins(&c, Row { kind: "text", preview: "Hello from Liquid Tools", source: Some("Code"), at: ago(50_000), ..Default::default() });
    ins(&c, Row { kind: "text", preview: &long_preview, content_ref: Some(&long), at: ago(49_000), ..Default::default() });
    ins(&c, Row { kind: "link", preview: "https://example.com/page", source: Some("chrome"), at: ago(48_000), ..Default::default() });
    ins(&c, Row { kind: "code", preview: "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}", at: ago(47_000), ..Default::default() });
    ins(&c, Row { kind: "contact", preview: "user@example.com", at: ago(46_000), ..Default::default() });
    ins(&c, Row { kind: "color", preview: "#FF8800", at: ago(45_000), ..Default::default() });
    ins(&c, Row { kind: "color", preview: "123456", at: ago(44_000), ..Default::default() }); // a one-time code, not a colour
    let rich = cf_html("<b>Bold text</b>", "https://news.example/a");
    ins(&c, Row { kind: "text", preview: "Bold text", html: Some(&rich), at: ago(43_000), ..Default::default() });
    let img = ins(&c, Row { kind: "image", preview: "123", source: Some("chrome"), at: ago(42_000), pinned: true, ..Default::default() });
    std::fs::write(clip_dir.join(format!("{img}.png")), &image).unwrap();
    ins(&c, Row { kind: "image", preview: "456", at: ago(41_000), ..Default::default() }); // file missing
    let tr = ins(&c, Row { kind: "image", preview: "789", at: ago(40_000), ..Default::default() });
    std::fs::write(clip_dir.join(format!("{tr}.png")), &transparent).unwrap();
    let paths = "C:\\Docs\\a.txt\nC:\\Docs\\b.pdf";
    ins(&c, Row { kind: "file", preview: paths, content_ref: Some(paths), at: ago(39_000), pinned: true, category: "Work", ..Default::default() });
    ins(&c, Row { kind: "screenshot", preview: "C:\\Shots\\1.png", content_ref: Some("C:\\Shots\\1.png"), source: Some("chrome"), at: ago(38_000), category: "Shots", app_icon: Some(icon), ..Default::default() });
    ins(&c, Row { kind: "dictation", preview: "spoken words", at: ago(37_000), ..Default::default() });
    ins(&c, Row { kind: "text", preview: "Hello from Liquid Tools", source: Some("Code"), at: ago(36_000), ..Default::default() });
    ins(&c, Row { kind: "text", preview: "Pinned note", at: ago(35_000), pinned: true, category: "Личное", ..Default::default() });
    ins(&c, Row { kind: "text", preview: "Work memo", at: ago(34_000), category: "Work", ..Default::default() });
    ins(&c, Row { kind: "text", preview: "Legacy seconds", at: ago(10 * DAY) / 1000, ..Default::default() });
    ins(&c, Row { kind: "text", preview: "Ancient history", at: ago(200 * DAY), ..Default::default() });
    ins(&c, Row { kind: "text", preview: "Ancient favorite", at: ago(300 * DAY), pinned: true, ..Default::default() });
    ins(&c, Row { kind: "text", preview: "", at: ago(30_000), ..Default::default() }); // unreadable: empty
    ins(&c, Row { kind: "text", preview: "Orphan folder member", at: ago(29_000), category: "Orphan", ..Default::default() });
    drop(c);
    LtFixture { dir, db, image, transparent }
}

/// Name, size, mtime and content hash of every file under `dir`.
fn fingerprint(dir: &Path) -> BTreeMap<String, (u64, Option<std::time::SystemTime>, String)> {
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            for (k, v) in fingerprint(&p) {
                out.insert(format!("{}/{k}", e.file_name().to_string_lossy()), v);
            }
        } else {
            let m = std::fs::metadata(&p).unwrap();
            let bytes = std::fs::read(&p).unwrap();
            out.insert(
                e.file_name().to_string_lossy().into_owned(),
                (m.len(), m.modified().ok(), content::content_hash(Kind::File, &bytes)),
            );
        }
    }
    out
}

#[test]
fn lt_probe_counts_what_will_be_imported() {
    let kit = Kit::new();
    let none = lt::probe(&kit.dir.path().join("nope").join("clipboard.db"), kit.cutoff()).unwrap();
    assert_eq!((none.found, none.path, none.clips), (false, None, 0));
    let fx = lt_fixture();
    let p = lt::probe(&fx.db, kit.cutoff()).unwrap();
    assert!(p.found);
    assert_eq!(p.path.as_deref(), Some(fx.db.to_string_lossy().as_ref()));
    assert_eq!((p.clips, p.favorites, p.folders), (20, 4, 5));
    // Retention "forever": the ancient history row counts too.
    assert_eq!(lt::probe(&fx.db, None).unwrap().clips, 21);
    assert_eq!(lt::default_db(Path::new("C:\\R")), Path::new("C:\\R\\com.liquidtools.app\\clipboard\\clipboard.db"));
}

#[test]
fn lt_import_maps_everything_and_reimport_adds_nothing() {
    let kit = Kit::new();
    let fx = lt_fixture();
    let before = fingerprint(fx.dir.path());
    let phases = std::sync::Mutex::new(Vec::<(String, i64, i64)>::new());
    let record = |p: &str, d: i64, t: i64| phases.lock().unwrap().push((p.to_string(), d, t));
    let s = &kit.store;
    let r = lt::run(s, &fx.db, kit.cutoff(), true, &[], &record).unwrap();
    assert_eq!((r.imported, r.skipped, r.folders, r.errors), (16, 2, 5, 2), "{r:?}");
    let phases = phases.into_inner().unwrap();
    assert_eq!(phases.first().map(|p| p.0.as_str()), Some("folders"));
    assert_eq!(phases.last().cloned(), Some(("clips".to_string(), 20, 20)));
    assert_eq!(s.counts().unwrap().all, 16);

    // Duplicate rows of one source → one record: earliest created, latest copied, counted.
    let hello = kit.id_of("Hello from Liquid Tools");
    assert_eq!(kit.one::<i64>("SELECT created_at FROM clips WHERE id = ?1", [hello]), T - 50_000);
    assert_eq!(kit.one::<i64>("SELECT copied_at FROM clips WHERE id = ?1", [hello]), T - 36_000);
    assert_eq!(kit.one::<i64>("SELECT copy_count FROM clips WHERE id = ?1", [hello]), 2);
    let detail = s.clip_detail(hello).unwrap();
    let apps = s.apps_list().unwrap();
    let code = apps.iter().find(|a| Some(a.id) == detail.clip.app_id).unwrap();
    assert_eq!((code.exe.as_str(), code.name.as_str()), ("code.exe", "Code"));

    // Text: full text from content_ref; re-classified kinds and meta.
    let long = LONG_WORD.repeat(70);
    assert_eq!(kit.one::<String>("SELECT kind FROM clips WHERE text = ?1", [long.as_str()]), "text");
    let kind = |t: &str| kit.one::<String>("SELECT kind FROM clips WHERE text = ?1", [t]);
    assert_eq!(kind("https://example.com/page"), "link");
    assert_eq!(kind("user@example.com"), "contact");
    assert_eq!(kind("#FF8800"), "color");
    assert_eq!(kind("123456"), "text", "Liqlip's stricter classifier");
    assert_eq!(kind("fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}"), "code");
    let link = s.clip_detail(kit.id_of("https://example.com/page")).unwrap();
    assert_eq!(link.clip.meta.host.as_deref(), Some("example.com"));
    let hash: String = kit.one("SELECT hash FROM clips WHERE text = 'user@example.com'", []);
    assert_eq!(hash, content::content_hash(Kind::Contact, b"user@example.com"));

    // Rich text: CF_HTML header stripped, SourceURL → meta.url.
    let bold = kit.id_of("Bold text");
    let paste = s.get_for_paste(bold).unwrap();
    let html = paste.html.unwrap();
    assert!(html.starts_with("<html><body><!--StartFragment--><b>Bold text</b>"), "{html}");
    assert_eq!(s.clip_detail(bold).unwrap().clip.meta.url.as_deref(), Some("https://news.example/a"));

    // Images: pixel hash, PNG in media, thumbnail; all-transparent normalised like capture.
    let imgs: Vec<(i64, i64, i64, String)> = {
        let c = s.lock();
        let mut st = c.prepare("SELECT id, width, height, hash FROM clips WHERE kind = 'image' ORDER BY width DESC").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(imgs.len(), 2);
    let (big, w, h, hash) = imgs[0].clone();
    assert_eq!((w, h), (500, 300));
    let rgba = decode(&fx.image);
    assert_eq!(hash, content::image_hash(500, 300, rgba.as_raw()));
    let media = s.get_for_paste(big).unwrap().png_path.unwrap();
    assert_eq!(std::fs::read(media).unwrap(), fx.image, "PNG copied as is");
    assert!(s.data_dir().join("media").join("thumbs").join(format!("{big}.png")).exists());
    let (small, _, _, small_hash) = imgs[1].clone();
    let mut norm = decode(&fx.transparent);
    norm.pixels_mut().for_each(|p| p.0[3] = 255);
    assert_eq!(small_hash, content::image_hash(20, 10, norm.as_raw()));
    let stored = decode(&std::fs::read(s.get_for_paste(small).unwrap().png_path.unwrap()).unwrap());
    assert!(stored.pixels().all(|p| p.0[3] == 255));

    // Files.
    let files: i64 = kit.one("SELECT id FROM clips WHERE kind = 'file'", []);
    let fp = s.get_for_paste(files).unwrap().files.unwrap();
    assert_eq!(fp.paths, vec!["C:\\Docs\\a.txt", "C:\\Docs\\b.pdf"]);
    assert_eq!(s.clip_detail(files).unwrap().clip.meta.ext.as_deref(), Some("txt"));

    // Favorites in Liquid Tools' order (newest first), folders, legacy seconds lifted.
    let favs: Vec<String> = {
        let c = s.lock();
        let mut st = c.prepare("SELECT coalesce(text, kind) FROM clips WHERE favorite = 1 ORDER BY fav_order, id").unwrap();
        st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(favs, vec!["Pinned note", "C:\\Docs\\a.txt\nC:\\Docs\\b.pdf", "image", "Ancient favorite"]);
    let folders = s.folders_list().unwrap();
    let names: Vec<&str> = folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["Work", "Личное", "Empty", "Shots", "Orphan"]);
    let count = |n: &str| folders.iter().find(|f| f.name == n).unwrap().count;
    assert_eq!((count("Work"), count("Личное"), count("Empty"), count("Orphan")), (2, 1, 0, 1));
    let legacy: i64 = kit.one("SELECT copied_at FROM clips WHERE text = 'Legacy seconds'", []);
    assert_eq!(legacy, (T - 10 * DAY) / 1000 * 1000);
    let ancient: i64 = kit.one("SELECT count(*) FROM clips WHERE text = 'Ancient history'", []);
    assert_eq!(ancient, 0, "older than the retention period");
    // The chrome icon comes from the screenshot row.
    let chrome = s.apps_list().unwrap().into_iter().find(|a| a.exe == "chrome.exe").unwrap();
    assert!(chrome.icon_url.is_some());
    // Neither the screenshot outside Liquid Tools' cache nor the dictation.
    let foreign: i64 = kit.one("SELECT count(*) FROM clips WHERE text IN ('spoken words', 'C:\\Shots\\1.png')", []);
    assert_eq!(foreign, 0);

    // Re-import: nothing new, nothing bumped.
    let copied_before: i64 = kit.one("SELECT sum(copied_at) FROM clips", []);
    let r2 = lt::run(s, &fx.db, kit.cutoff(), true, &[], &quiet()).unwrap();
    assert_eq!((r2.imported, r2.skipped, r2.folders, r2.errors), (0, 18, 0, 2), "{r2:?}");
    assert_eq!(kit.one::<i64>("SELECT sum(copied_at) FROM clips", []), copied_before);
    assert_eq!(kit.one::<i64>("SELECT copy_count FROM clips WHERE id = ?1", [hello]), 2);

    // Liquid Tools' folder did not change by a single byte.
    assert_eq!(fingerprint(fx.dir.path()), before);
}

#[test]
fn lt_running_with_a_pending_write_is_read_without_touching_it() {
    let kit = Kit::new();
    let fx = lt_fixture();
    // Liquid Tools mid-transaction: a rollback journal exists, the INSERT is not committed.
    let writer = Connection::open(&fx.db).unwrap();
    writer.execute_batch("PRAGMA cache_size = 1; BEGIN IMMEDIATE;").unwrap();
    for i in 0..200 {
        ins(&writer, Row { kind: "text", preview: &format!("uncommitted {i} {}", "x".repeat(500)), at: T - 1_000, ..Default::default() });
    }
    assert!(fx.db.with_file_name("clipboard.db-journal").exists(), "the writer has a live journal");
    let before = fingerprint(fx.dir.path());
    let p = lt::probe(&fx.db, kit.cutoff()).unwrap();
    assert_eq!(p.clips, 20, "only committed rows");
    let r = lt::run(&kit.store, &fx.db, kit.cutoff(), true, &[], &quiet()).unwrap();
    assert_eq!(r.imported, 16);
    assert_eq!(fingerprint(fx.dir.path()), before, "the writer's files are untouched");
    writer.execute_batch("ROLLBACK;").unwrap();
}

#[test]
fn lt_wal_database_with_an_open_writer() {
    let kit = Kit::new();
    let dir = TempDir::new("liqlip-lt-wal-").unwrap();
    let db = dir.path().join("clipboard.db");
    let writer = Connection::open(&db).unwrap();
    let mode: String = writer.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    writer.execute_batch(&format!("PRAGMA wal_autocheckpoint = 0; {LT_SCHEMA}")).unwrap();
    ins(&writer, Row { kind: "text", preview: "only in the WAL", at: T - 5_000, ..Default::default() });
    assert!(dir.path().join("clipboard.db-wal").exists());
    let before = fingerprint(dir.path());
    assert_eq!(lt::probe(&db, kit.cutoff()).unwrap().clips, 1);
    let r = lt::run(&kit.store, &db, kit.cutoff(), true, &[], &quiet()).unwrap();
    assert_eq!(r.imported, 1);
    assert_eq!(fingerprint(dir.path()), before, "no -shm written, nothing checkpointed");
    drop(writer);
}

#[test]
fn lt_bad_files_give_clear_errors() {
    let kit = Kit::new();
    let dir = TempDir::new("liqlip-lt-bad-").unwrap();
    let junk = dir.path().join("junk.db");
    std::fs::write(&junk, vec![7u8; 4096]).unwrap();
    assert_eq!(lt::probe(&junk, None).unwrap_err(), "invalid: not a Liquid Tools database");
    let other = dir.path().join("other.db");
    Connection::open(&other).unwrap().execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);").unwrap();
    assert_eq!(lt::probe(&other, None).unwrap_err(), "invalid: unknown Liquid Tools database format");
    assert!(lt::run(&kit.store, &dir.path().join("missing.db"), None, true, &[], &quiet()).unwrap_err().starts_with("not_found:"));
    // An old Liquid Tools database without the additive columns still imports.
    let old = dir.path().join("old.db");
    let c = Connection::open(&old).unwrap();
    c.execute_batch(
        "CREATE TABLE clipboard_entries (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, title TEXT NOT NULL,
           preview TEXT NOT NULL, content_ref TEXT, source_app TEXT, size INTEGER, width INTEGER, height INTEGER,
           created_at INTEGER NOT NULL, pinned INTEGER NOT NULL DEFAULT 0);",
    )
    .unwrap();
    c.execute("INSERT INTO clipboard_entries (kind, title, preview, created_at) VALUES ('text', 't', 'v1 row', ?1)", [T - 1])
        .unwrap();
    drop(c);
    let r = lt::run(&kit.store, &old, kit.cutoff(), true, &[], &quiet()).unwrap();
    assert_eq!((r.imported, r.folders), (1, 0));
}

fn texts(s: &Store) -> Vec<String> {
    let c = s.lock();
    let mut st = c.prepare("SELECT text FROM clips ORDER BY id").unwrap();
    st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

/// FIX-01: with `privacy.secretPatterns` on, a text that IS a secret (a Luhn-valid card
/// number, an `sk-…` key) is not imported and counts as skipped; a phrase that merely contains
/// a key is imported. With the setting off everything comes in.
#[test]
fn lt_import_leaves_secrets_out() {
    let dir = TempDir::new("liqlip-lt-secrets-").unwrap();
    let db = dir.path().join("clipboard.db");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(LT_SCHEMA).unwrap();
    let key = "sk-proj-AbCdEf0123456789_xyz-QWERTYUIOP";
    let phrase = format!("my key is {key}");
    ins(&c, Row { kind: "text", preview: "4111 1111 1111 1111", at: T - 3_000, ..Default::default() });
    ins(&c, Row { kind: "text", preview: key, at: T - 2_000, pinned: true, ..Default::default() });
    ins(&c, Row { kind: "text", preview: &phrase, at: T - 1_000, category: "Keys", ..Default::default() });
    drop(c);

    let on = Kit::new();
    let r = lt::run(&on.store, &db, on.cutoff(), true, &[], &quiet()).unwrap();
    assert_eq!((r.imported, r.skipped, r.errors), (1, 2, 0), "{r:?}");
    assert_eq!(texts(&on.store), vec![phrase.clone()]);
    // The probe still counts them, so imported + skipped + errors = probe.clips.
    assert_eq!(lt::probe(&db, on.cutoff()).unwrap().clips, 3);
    // A second run skips them again (nothing slips in through the dedup path).
    let r = lt::run(&on.store, &db, on.cutoff(), true, &[], &quiet()).unwrap();
    assert_eq!((r.imported, r.skipped), (0, 3));
    assert_eq!(on.store.counts().unwrap().all, 1);

    let off = Kit::new();
    let r = lt::run(&off.store, &db, off.cutoff(), false, &[], &quiet()).unwrap();
    assert_eq!((r.imported, r.skipped, r.errors), (3, 0, 0), "{r:?}");
}

/// W3: records Liquid Tools took from an app on `privacy.ignoredApps` (a password manager)
/// are not imported — the capture would have refused them; they count as skipped.
#[test]
fn lt_import_leaves_ignored_apps_out() {
    let dir = TempDir::new("liqlip-lt-ignored-").unwrap();
    let db = dir.path().join("clipboard.db");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(LT_SCHEMA).unwrap();
    ins(&c, Row { kind: "text", preview: "hunter2-from-keepass", source: Some("KeePass"), at: T - 3_000, ..Default::default() });
    ins(&c, Row { kind: "text", preview: "vault entry", source: Some("Vault"), at: T - 2_500, pinned: true, ..Default::default() });
    ins(&c, Row { kind: "text", preview: "meeting notes", source: Some("Code"), at: T - 2_000, ..Default::default() });
    ins(&c, Row { kind: "text", preview: "no source", at: T - 1_000, ..Default::default() });
    drop(c);
    let kit = Kit::new();
    let ignored = vec!["keepass.exe".to_string(), "C:\\Tools\\Vault.exe".to_string()];
    let r = lt::run(&kit.store, &db, kit.cutoff(), true, &ignored, &quiet()).unwrap();
    assert_eq!((r.imported, r.skipped, r.errors), (2, 2, 0), "{r:?}");
    assert_eq!(texts(&kit.store), vec!["meeting notes".to_string(), "no source".to_string()]);
    assert_eq!(lt::probe(&db, kit.cutoff()).unwrap().clips, 4, "imported + skipped + errors = probe");
}

#[test]
fn ignore_rule_matches_like_the_capture() {
    // FIX-16: the import has no copy of the rule — this is the capture's function.
    use crate::capture::is_ignored;
    let list = vec!["keepass.exe".to_string(), "bitwarden".to_string(), "C:/Tools/Vault.exe".to_string()];
    assert!(is_ignored("c:\\program files\\keepass\\KeePass.exe", &list), "file name");
    assert!(is_ignored("d:\\apps\\bitwarden.exe", &list), "bare stem");
    assert!(is_ignored("c:\\tools\\vault.exe", &list), "full path, / ≡ \\");
    assert!(!is_ignored("d:\\other\\vault.exe", &list), "another folder: another app");
    assert!(is_ignored("vault.exe", &list), "a bare Liquid Tools name matches a full-path entry");
    assert!(is_ignored("KEEPASS.EXE", &list));
    assert!(!is_ignored("notepad.exe", &list));
    assert!(!is_ignored("c:\\x\\keepassxc.exe", &list));
    assert!(!is_ignored("c:\\x\\a.exe", &["  ".to_string()]), "blank entry");
}

// ── backups ─────────────────────────────────────────────────────────────────

fn text_clip(text: &str, at: i64) -> NewClip {
    let (kind, meta) = crate::capture::classify::classify_text(text);
    NewClip {
        kind,
        hash: content::content_hash(kind, content::canonical_text(text)),
        preview: content::make_preview(text),
        text: Some(text.into()),
        meta,
        size: 0,
        char_count: None,
        word_count: None,
        line_count: None,
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

fn image_clip(w: u32, h: u32, at: i64) -> NewClip {
    let bytes = png(w, h, 255);
    let rgba = decode(&bytes);
    let mut c = text_clip("x", at);
    c.kind = Kind::Image;
    c.hash = content::image_hash(w, h, rgba.as_raw());
    c.preview = String::new();
    c.text = None;
    c.meta = ClipMeta::default();
    c.png = Some(bytes);
    c.thumb_png = Some(png(360, 270, 255));
    c
}

/// A store with every kind of data a backup must carry.
fn populate(s: &Store) -> Vec<i64> {
    let mut rich = text_clip("Rich text", T - 9_000);
    rich.html = Some("<b>Rich</b>".into());
    rich.rtf = Some(b"{\\rtf1 Rich}".to_vec());
    rich.source = Some(SourceApp { exe: "c:\\apps\\word.exe".into(), name: "Word".into() });
    let rich = s.insert_clip(rich).unwrap();
    s.set_app_icon(rich.app_id.unwrap(), &png(32, 32, 255)).unwrap();
    let img = s.insert_clip(image_clip(640, 480, T - 8_000)).unwrap().id;
    let mut files = text_clip("C:\\a.txt", T - 7_000);
    files.kind = Kind::File;
    files.hash = content::content_hash(Kind::File, &content::canonical_files(&["C:\\a.txt".to_string()]));
    files.files = Some(ClipFiles { paths: vec!["C:\\a.txt".into()], drop_effect: 1 });
    let files = s.insert_clip(files).unwrap().id;
    let hello = s.insert_clip(text_clip("Hello", T - 6_000)).unwrap().id;
    let link = s.insert_clip(text_clip("https://liqlip.example/", T - 5_000)).unwrap().id;
    let gone = s.insert_clip(text_clip("deleted secret", T - 4_000)).unwrap().id;
    s.set_favorite(&[link, img, hello], true).unwrap();
    let work = s.folder_create("Work").unwrap();
    s.folder_create("Empty").unwrap();
    s.set_folder(&[files, rich.id], Some(work.id)).unwrap();
    s.rename(hello, Some("Greeting")).unwrap();
    s.mark_used(&[hello], false).unwrap();
    s.snippet_save(&SnippetInput { id: None, name: "Sig".into(), body: "Regards".into(), keyword: Some(";sig".into()) }).unwrap();
    s.delete(&[gone]).unwrap();
    vec![rich.id, img, files, hello, link]
}

/// Every live row of every table, plus the media files, as comparable text.
fn dump(s: &Store) -> Vec<String> {
    let c = s.lock();
    let mut out = Vec::new();
    for sql in [
        "SELECT * FROM clips WHERE deleted_at IS NULL ORDER BY id",
        "SELECT f.* FROM clip_formats f JOIN clips c ON c.id = f.clip_id WHERE c.deleted_at IS NULL ORDER BY f.clip_id, f.format",
        "SELECT * FROM folders ORDER BY id",
        "SELECT * FROM apps ORDER BY id",
        "SELECT * FROM snippets ORDER BY id",
    ] {
        let mut st = c.prepare(sql).unwrap();
        let n = st.column_count();
        let rows = st
            .query_map([], |r| {
                let v: Vec<rusqlite::types::Value> = (0..n).map(|i| r.get(i).unwrap()).collect();
                Ok(format!("{v:?}"))
            })
            .unwrap();
        out.extend(rows.map(Result::unwrap));
    }
    drop(c);
    for sub in ["media", "media/thumbs", "icons"] {
        let dir = s.data_dir().join(sub);
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_type().unwrap().is_file())
            .map(|e| format!("{sub}/{} {}", e.file_name().to_string_lossy(), content::content_hash(Kind::File, &std::fs::read(e.path()).unwrap())))
            .collect();
        files.sort();
        out.extend(files);
    }
    out
}

fn export_to(kit: &Kit, dest: &Path) -> backup::Manifest {
    let p: Progress<'_> = &quiet();
    backup::export(&kit.store, br#"{"version":1}"#, "1.0.0-test", T, zip::DateTime::default(), dest, p).unwrap()
}

#[test]
fn export_then_replace_restores_identical_data() {
    let a = Kit::new();
    populate(&a.store);
    a.store.purge_deleted().unwrap(); // not yet purged (2 min TTL): the export drops it itself
    let dest = a.dir.path().join("out").join("Liqlip-2026-09-21.liqlip");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let m = export_to(&a, &dest);
    assert_eq!((m.counts.clips, m.counts.favorites, m.counts.folders, m.counts.snippets, m.counts.apps), (5, 3, 2, 1, 1));
    assert_eq!(m.counts.media, 3, "image PNG + thumbnail + app icon");
    let names: Vec<String> = std::fs::read_dir(dest.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["Liqlip-2026-09-21.liqlip"], "no .part left");

    let b = Kit::new();
    b.store.insert_clip(text_clip("will be replaced", T - 1)).unwrap();
    let (report, settings) = backup::restore_replace(&b.store, &dest, &quiet()).unwrap();
    assert_eq!((report.imported, report.skipped, report.folders, report.errors), (5, 0, 2, 0));
    assert_eq!(settings.as_deref(), Some(&br#"{"version":1}"#[..]));
    assert_eq!(dump(&b.store), dump(&a.store), "replace returns identical data");
    // The previous data is kept aside once; the staging folder is gone.
    let entries: Vec<String> = std::fs::read_dir(b.store.data_dir()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(entries.iter().filter(|n| n.starts_with("backup-before-restore-")).count(), 1, "{entries:?}");
    assert!(!entries.iter().any(|n| n.starts_with("restore-staging-")), "{entries:?}");
    // And the restored store keeps working.
    b.store.insert_clip(text_clip("after restore", T)).unwrap();
}

#[test]
fn merge_adds_without_duplicates() {
    let a = Kit::new();
    let ids = populate(&a.store);
    let dest = a.dir.path().join("a.liqlip");
    export_to(&a, &dest);

    let b = Kit::new();
    b.now.store(T + DAY, Ordering::SeqCst);
    let hello_b = b.store.insert_clip(text_clip("Hello", T - 100 * 60_000)).unwrap().id;
    let work_b = b.store.folder_create("WORK").unwrap();
    b.store
        .snippet_save(&SnippetInput { id: None, name: "Sig".into(), body: "Regards".into(), keyword: None })
        .unwrap();
    let r = backup::restore_merge(&b.store, &dest, b.store.retention_cutoff(crate::model::Retention::Month3), true, &[], &quiet()).unwrap();
    assert_eq!((r.imported, r.skipped, r.folders, r.errors), (4, 1, 1, 0), "{r:?}");
    assert_eq!(b.store.counts().unwrap().all, 5);
    // The existing "Hello" keeps its time, gains favorite (and keeps its own title = none).
    assert_eq!(b.one::<i64>("SELECT copied_at FROM clips WHERE id = ?1", [hello_b]), T - 100 * 60_000);
    assert_eq!(b.one::<i64>("SELECT favorite FROM clips WHERE id = ?1", [hello_b]), 1);
    // Folder merged by name, case-insensitively; "Empty" created.
    let folders: Vec<String> = b.store.folders_list().unwrap().into_iter().map(|f| f.name).collect();
    assert_eq!(folders, vec!["WORK", "Empty"]);
    assert_eq!(b.one::<i64>("SELECT count(*) FROM clips WHERE folder_id = ?1", [work_b.id]), 2);
    // Media, formats, app + icon, times and counters came along.
    let img: i64 = b.one("SELECT id FROM clips WHERE kind = 'image'", []);
    assert!(b.store.get_for_paste(img).unwrap().png_path.is_some());
    let rich = b.id_of("Rich text");
    let p = b.store.get_for_paste(rich).unwrap();
    assert_eq!((p.html.as_deref(), p.rtf.as_deref()), (Some("<b>Rich</b>"), Some(&b"{\\rtf1 Rich}"[..])));
    let app = b.store.apps_list().unwrap().into_iter().find(|x| x.exe == "c:\\apps\\word.exe").unwrap();
    assert!(app.icon_url.is_some());
    let a_rich: i64 = a.one("SELECT copied_at FROM clips WHERE id = ?1", [ids[0]]);
    assert_eq!(b.one::<i64>("SELECT copied_at FROM clips WHERE id = ?1", [rich]), a_rich);
    // Favorites keep the backup's manual order after the existing ones.
    let favs: Vec<String> = {
        let c = b.store.lock();
        let mut st = c.prepare("SELECT coalesce(text, kind) FROM clips WHERE favorite = 1 ORDER BY fav_order, id").unwrap();
        st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(favs, vec!["https://liqlip.example/", "image", "Hello"]);
    // Snippet with the same name and body is not doubled; the soft-deleted record is absent.
    assert_eq!(b.store.snippets_list("").unwrap().len(), 1);
    assert_eq!(b.one::<i64>("SELECT count(*) FROM clips WHERE text = 'deleted secret'", []), 0);
    // A second merge adds nothing.
    let r2 = backup::restore_merge(&b.store, &dest, None, true, &[], &quiet()).unwrap();
    assert_eq!((r2.imported, r2.skipped, r2.folders), (0, 5, 0));
}

/// FIX-01 for `merge`: a backup made with `secretPatterns` off (or before the filter) brings
/// no secrets into a store that has it on.
#[test]
fn merge_leaves_secrets_out() {
    let a = Kit::new();
    let key = "ghp_1234567890abcdefghijABCDEFGHIJ123456";
    let phrase = format!("token {key} was revoked");
    for (i, t) in ["4111-1111-1111-1111", key, phrase.as_str()].into_iter().enumerate() {
        a.store.insert_clip(text_clip(t, T - 1_000 * (3 - i as i64))).unwrap();
    }
    let dest = a.dir.path().join("secrets.liqlip");
    export_to(&a, &dest);

    let on = Kit::new();
    let r = backup::restore_merge(&on.store, &dest, None, true, &[], &quiet()).unwrap();
    assert_eq!((r.imported, r.skipped, r.errors), (1, 2, 0), "{r:?}");
    assert_eq!(texts(&on.store), vec![phrase.clone()]);

    let off = Kit::new();
    let r = backup::restore_merge(&off.store, &dest, None, false, &[], &quiet()).unwrap();
    assert_eq!((r.imported, r.skipped), (3, 0));
}

#[test]
fn invalid_backups_are_refused_clearly() {
    let kit = Kit::new();
    let dir = kit.dir.path();
    let not_zip = dir.join("x.liqlip");
    std::fs::write(&not_zip, b"hello").unwrap();
    assert_eq!(backup::open_archive(&not_zip).unwrap_err(), "invalid: not a Liqlip backup");
    let write_zip = |name: &str, entries: &[(&str, Vec<u8>)]| -> PathBuf {
        let p = dir.join(name);
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        for (n, data) in entries {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut z, data).unwrap();
        }
        z.finish().unwrap();
        p
    };
    let no_manifest = write_zip("a.liqlip", &[("clipboard.db", b"x".to_vec())]);
    assert_eq!(backup::open_archive(&no_manifest).unwrap_err(), "invalid: not a Liqlip backup");
    let manifest = |format: &str, fv: u32, schema: i64| {
        serde_json::to_vec(&backup::Manifest {
            format: format.into(),
            format_version: fv,
            app_version: "9".into(),
            schema_version: schema,
            created_at: 0,
            counts: Default::default(),
        })
        .unwrap()
    };
    let foreign = write_zip("b.liqlip", &[("manifest.json", manifest("other", 1, 1))]);
    assert_eq!(backup::open_archive(&foreign).unwrap_err(), "invalid: not a Liqlip backup");
    let newer = write_zip("c.liqlip", &[("manifest.json", manifest(backup::FORMAT, 1, 99))]);
    assert_eq!(backup::open_archive(&newer).unwrap_err(), "invalid: the backup was made by a newer Liqlip");
    let newer_fmt = write_zip("d.liqlip", &[("manifest.json", manifest(backup::FORMAT, 2, 1))]);
    assert_eq!(backup::open_archive(&newer_fmt).unwrap_err(), "invalid: the backup was made by a newer Liqlip");
    // Valid manifest, no database / a damaged one: nothing is replaced.
    kit.store.insert_clip(text_clip("keep me", T)).unwrap();
    let empty = write_zip("e.liqlip", &[("manifest.json", manifest(backup::FORMAT, 1, 1))]);
    assert_eq!(backup::restore_replace(&kit.store, &empty, &quiet()).unwrap_err(), "invalid: the backup has no database");
    let junk_db = write_zip("f.liqlip", &[("manifest.json", manifest(backup::FORMAT, 1, 1)), ("clipboard.db", vec![0u8; 4096])]);
    assert_eq!(backup::restore_replace(&kit.store, &junk_db, &quiet()).unwrap_err(), "invalid: the backup database is damaged");
    assert!(backup::restore_merge(&kit.store, &junk_db, None, true, &[], &quiet()).unwrap_err().starts_with("invalid:"));
    // W3: a SQLite file with our version number but not our shape is refused too.
    let foreign_db = dir.join("foreign.db");
    {
        let c = Connection::open(&foreign_db).unwrap();
        c.execute_batch(
            "CREATE TABLE clips (id INTEGER PRIMARY KEY, deleted_at INTEGER); CREATE TABLE folders (id INTEGER);
             CREATE TABLE apps (id INTEGER); CREATE TABLE clip_formats (clip_id INTEGER); CREATE TABLE snippets (id INTEGER);
             INSERT INTO clips (id) VALUES (1); PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let shaped = write_zip("h.liqlip", &[("manifest.json", manifest(backup::FORMAT, 1, 1)), ("clipboard.db", std::fs::read(&foreign_db).unwrap())]);
    assert_eq!(backup::restore_replace(&kit.store, &shaped, &quiet()).unwrap_err(), "invalid: the backup database is damaged");
    // Path tricks in entry names are ignored, never written outside the staging folder.
    let evil = write_zip(
        "g.liqlip",
        &[("manifest.json", manifest(backup::FORMAT, 1, 1)), ("../evil.png", b"x".to_vec()), ("media/../../evil.png", b"x".to_vec())],
    );
    let _ = backup::restore_replace(&kit.store, &evil, &quiet());
    assert!(!kit.store.data_dir().parent().unwrap().join("evil.png").exists());
    assert_eq!(kit.store.counts().unwrap().all, 1, "current data untouched");
    assert_eq!(backup::default_file_name(2026, 9, 1), "Liqlip-2026-09-01.liqlip");
}

#[test]
fn export_failure_leaves_no_partial_file() {
    let kit = Kit::new();
    kit.store.insert_clip(text_clip("x", T)).unwrap();
    let dest = kit.dir.path().join("missing-dir").join("b.liqlip");
    let p: Progress<'_> = &quiet();
    assert!(backup::export(&kit.store, b"{}", "1", T, zip::DateTime::default(), &dest, p).unwrap_err().starts_with("failed:"));
    assert!(!kit.dir.path().join("missing-dir").exists());
}
