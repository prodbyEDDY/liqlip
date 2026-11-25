//! Reads: the feed (filters §9.3, order §4.3, opaque keyset cursor, search), pill
//! counters, Quick Look detail, source apps, storage stats.
//!
//! Search: free text is split into terms (whitespace; `"…"` keeps a phrase); every term
//! must match (AND). Terms of ≥ 3 characters go to the trigram FTS index (`clips_fts MATCH`,
//! each term a quoted FTS5 string with `"` doubled — fts5.html §3.1); shorter terms can't
//! use a trigram index (fts5.html §4.3.5) and fall back to a Unicode case-insensitive
//! substring test on `preview`/`title` (`liq_icontains`). Exclusions work the same way.
//! The order never depends on relevance (§4.3).

use super::write::{parse_meta, FilesBlob};
use super::{db_err, placeholders, Store};
use crate::content::{content_hash, FILES_MAX};
use crate::model::{AppInfo, Clip, ClipDetail, ClipFile, Counts, FeedQuery, Format, Kind, OcrState, Page, PerKind, StorageStats};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, OptionalExtension};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub(crate) const PAGE_DEFAULT: i64 = 40;
pub(crate) const PAGE_MAX: i64 = 200;
/// At most this many search / exclusion terms are used.
const TERMS_MAX: usize = 32;
/// Trigram tokenizer: shorter substrings never match (fts5.html §4.3.5).
const TRIGRAM_MIN_CHARS: usize = 3;
/// Total time `clip_detail` spends checking file paths.
const STAT_BUDGET: Duration = Duration::from_secs(2);

/// Columns of a feed row (no full text — it can be 2 MB). Indexes used by [`row_to_clip`].
pub(crate) const CLIP_COLS: &str = "c.id, c.kind, c.title, c.preview, c.app_id, c.created_at, c.copied_at, \
     c.copy_count, c.used_at, c.use_count, c.favorite, c.folder_id, c.size, c.width, c.height, \
     c.char_count, c.file_count, c.meta, c.ocr_state, c.fav_order, \
     (SELECT group_concat(f.format) FROM clip_formats f WHERE f.clip_id = c.id)";
const CLIP_COLS_N: usize = 21;

/// `Clip.formats` in canonical order from the stored `clip_formats` rows.
pub(crate) fn formats_for<'a>(kind: Kind, present: impl IntoIterator<Item = &'a str>) -> Vec<Format> {
    let present: Vec<&str> = present.into_iter().collect();
    let has = |f: &str| present.contains(&f);
    match kind {
        Kind::Image => if has("png") { vec![Format::Png] } else { Vec::new() },
        Kind::File => vec![Format::Files, Format::Text],
        _ => {
            let mut v = vec![Format::Text];
            for (name, f) in [("html", Format::Html), ("rtf", Format::Rtf), ("png", Format::Png)] {
                if has(name) {
                    v.push(f);
                }
            }
            v
        }
    }
}

pub(crate) fn thumb_url(id: i64, created_at: i64) -> String {
    format!("http://liq.localhost/thumb/{id}?v={created_at}")
}

pub(crate) fn media_url(id: i64, created_at: i64) -> String {
    format!("http://liq.localhost/media/{id}?v={created_at}")
}

/// `?v=` from the exe path: a recreated database restarts app ids, and the webview must
/// not show a cached icon of another app.
pub(crate) fn icon_url(app_id: i64, exe: &str) -> String {
    let v = &content_hash(Kind::Text, exe.as_bytes())[..8];
    format!("http://liq.localhost/icon/{app_id}?v={v}")
}

/// Map a [`CLIP_COLS`] row (starting at column 0) → `(Clip, fav_order)`.
pub(crate) fn row_to_clip(r: &rusqlite::Row<'_>) -> rusqlite::Result<(Clip, Option<f64>)> {
    let id: i64 = r.get(0)?;
    let kind = Kind::parse(&r.get::<_, String>(1)?).unwrap_or(Kind::Text);
    let created_at: i64 = r.get(5)?;
    let formats: Option<String> = r.get(20)?;
    let clip = Clip {
        id,
        kind,
        title: r.get(2)?,
        preview: r.get(3)?,
        app_id: r.get(4)?,
        created_at,
        copied_at: r.get(6)?,
        copy_count: r.get(7)?,
        used_at: r.get(8)?,
        use_count: r.get(9)?,
        favorite: r.get::<_, i64>(10)? != 0,
        folder_id: r.get(11)?,
        size: r.get(12)?,
        width: r.get(13)?,
        height: r.get(14)?,
        char_count: r.get(15)?,
        file_count: r.get(16)?,
        thumb_url: (kind == Kind::Image).then(|| thumb_url(id, created_at)),
        formats: formats_for(kind, formats.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty())),
        meta: parse_meta(r.get::<_, Option<String>>(17)?.as_deref()),
        ocr: match r.get::<_, i64>(18)? {
            1 => OcrState::Pending,
            2 => OcrState::Done,
            3 => OcrState::Error,
            _ => OcrState::None,
        },
    };
    Ok((clip, r.get(19)?))
}

/// One live record (`not_found` for missing / soft-deleted ids).
pub(crate) fn clip_by_id(conn: &Connection, id: i64) -> Result<Clip, String> {
    conn.query_row(&format!("SELECT {CLIP_COLS} FROM clips c WHERE c.id = ?1 AND c.deleted_at IS NULL"), [id], row_to_clip)
        .optional()
        .map_err(db_err)?
        .map(|(c, _)| c)
        .ok_or_else(|| format!("not_found: clip {id}"))
}

/// Feed order (§4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// `copied_at DESC, id DESC`
    Recent,
    /// `favorite DESC, copied_at DESC, id DESC` (`settings.picker.favoritesFirst`)
    FavFirst,
    /// Favorites pill: `fav_order ASC, id ASC`
    Manual,
}

/// Keyset position after the last row of a page; opaque (base64url) on the wire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Cursor {
    Recent { copied_at: i64, id: i64 },
    FavFirst { favorite: i64, copied_at: i64, id: i64 },
    Manual { fav_order: f64, id: i64 },
}

impl Cursor {
    fn after(order: Order, c: &Clip, fav_order: Option<f64>) -> Cursor {
        match order {
            Order::Recent => Cursor::Recent { copied_at: c.copied_at, id: c.id },
            Order::FavFirst => Cursor::FavFirst { favorite: i64::from(c.favorite), copied_at: c.copied_at, id: c.id },
            Order::Manual => Cursor::Manual { fav_order: fav_order.unwrap_or(0.0), id: c.id },
        }
    }

    pub(crate) fn encode(&self) -> String {
        let raw = match self {
            Cursor::Recent { copied_at, id } => format!("r1:{copied_at}:{id}"),
            Cursor::FavFirst { favorite, copied_at, id } => format!("f1:{favorite}:{copied_at}:{id}"),
            Cursor::Manual { fav_order, id } => format!("m1:{}:{id}", fav_order.to_bits()),
        };
        URL_SAFE_NO_PAD.encode(raw)
    }

    /// `invalid: cursor` for garbage or for a cursor of another order (e.g. the
    /// favorites-first setting changed between pages — the feed must restart).
    pub(crate) fn decode(s: &str, order: Order) -> Result<Cursor, String> {
        let bad = || "invalid: cursor".to_string();
        let raw = URL_SAFE_NO_PAD.decode(s.trim()).map_err(|_| bad())?;
        let raw = String::from_utf8(raw).map_err(|_| bad())?;
        let parts: Vec<&str> = raw.split(':').collect();
        let int = |s: &str| s.parse::<i64>().map_err(|_| bad());
        let cursor = match parts.as_slice() {
            ["r1", c, i] => Cursor::Recent { copied_at: int(c)?, id: int(i)? },
            ["f1", f, c, i] => Cursor::FavFirst { favorite: int(f)?, copied_at: int(c)?, id: int(i)? },
            ["m1", b, i] => {
                let bits = b.parse::<u64>().map_err(|_| bad())?;
                let fav_order = f64::from_bits(bits);
                if !fav_order.is_finite() {
                    return Err(bad());
                }
                Cursor::Manual { fav_order, id: int(i)? }
            }
            _ => return Err(bad()),
        };
        let matches = matches!(
            (order, cursor),
            (Order::Recent, Cursor::Recent { .. }) | (Order::FavFirst, Cursor::FavFirst { .. }) | (Order::Manual, Cursor::Manual { .. })
        );
        if matches { Ok(cursor) } else { Err(bad()) }
    }
}

/// Split free text into search terms: whitespace separates, `"…"` keeps a phrase (an
/// unclosed quote runs to the end). Blank terms dropped, duplicates (case-insensitive)
/// dropped, at most [`TERMS_MAX`].
pub(crate) fn parse_terms(s: &str) -> Vec<String> {
    let mut raw: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in s.chars() {
        if ch == '"' {
            raw.push(std::mem::take(&mut cur));
            quoted = !quoted;
        } else if ch.is_whitespace() && !quoted {
            raw.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    raw.push(cur);
    let mut seen = std::collections::HashSet::new();
    raw.into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
        .take(TERMS_MAX)
        .collect()
}

/// One FTS5 string token (fts5.html §3.1: `"` inside a string is doubled).
pub(crate) fn fts_quote(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Terms an FTS trigram MATCH can serve (≥ 3 characters) vs the rest.
pub(crate) fn split_terms(terms: Vec<String>) -> (Vec<String>, Vec<String>) {
    terms.into_iter().partition(|t| t.chars().count() >= TRIGRAM_MIN_CHARS)
}

pub(crate) fn normalize_limit(limit: i64) -> i64 {
    if limit <= 0 { PAGE_DEFAULT } else { limit.min(PAGE_MAX) }
}

/// SQL + parameters of a feed page (`fetch` rows).
pub(crate) struct Built {
    pub sql: String,
    pub params: Vec<Value>,
}

pub(crate) fn build_feed(q: &FeedQuery, order: Order, cursor: Option<&Cursor>, fetch: i64) -> Built {
    let mut w: Vec<String> = vec!["c.deleted_at IS NULL".into()];
    let mut p: Vec<Value> = Vec::new();
    let f = &q.filter;
    if let Some(kinds) = f.kinds.as_ref().filter(|k| !k.is_empty()) {
        let mut kinds = kinds.clone();
        kinds.sort();
        kinds.dedup();
        w.push(format!("c.kind IN ({})", placeholders(kinds.len())));
        p.extend(kinds.iter().map(|k| Value::Text(k.as_str().into())));
    }
    if f.favorites == Some(true) {
        w.push("c.favorite = 1".into());
    }
    if let Some(fid) = f.folder_id {
        w.push("c.folder_id = ?".into());
        p.push(Value::Integer(fid));
    }
    if let Some(apps) = f.app_ids.as_ref().filter(|a| !a.is_empty()) {
        let apps = super::dedup_ids(apps);
        w.push(format!("c.app_id IN ({})", placeholders(apps.len())));
        p.extend(apps.into_iter().map(Value::Integer));
    }
    if let Some(since) = f.since {
        w.push("c.copied_at >= ?".into());
        p.push(Value::Integer(since));
    }
    if let Some(until) = f.until {
        w.push("c.copied_at <= ?".into());
        p.push(Value::Integer(until));
    }
    let (long, short) = split_terms(parse_terms(&q.text));
    if !long.is_empty() {
        w.push("c.id IN (SELECT rowid FROM clips_fts WHERE clips_fts MATCH ?)".into());
        p.push(Value::Text(long.iter().map(|t| fts_quote(t)).collect::<Vec<_>>().join(" AND ")));
    }
    for t in short {
        w.push("(liq_icontains(c.preview, ?) OR liq_icontains(c.title, ?))".into());
        p.push(Value::Text(t.clone()));
        p.push(Value::Text(t));
    }
    let excluded: Vec<String> = parse_terms(&q.exclude.join(" "));
    let (ex_long, ex_short) = split_terms(excluded);
    if !ex_long.is_empty() {
        w.push("c.id NOT IN (SELECT rowid FROM clips_fts WHERE clips_fts MATCH ?)".into());
        p.push(Value::Text(ex_long.iter().map(|t| fts_quote(t)).collect::<Vec<_>>().join(" OR ")));
    }
    for t in ex_short {
        w.push("NOT liq_icontains(c.preview, ?) AND NOT liq_icontains(c.title, ?)".into());
        p.push(Value::Text(t.clone()));
        p.push(Value::Text(t));
    }
    match cursor {
        Some(Cursor::Recent { copied_at, id }) => {
            w.push("(c.copied_at, c.id) < (?, ?)".into());
            p.extend([Value::Integer(*copied_at), Value::Integer(*id)]);
        }
        Some(Cursor::FavFirst { favorite, copied_at, id }) => {
            w.push("(c.favorite, c.copied_at, c.id) < (?, ?, ?)".into());
            p.extend([Value::Integer(*favorite), Value::Integer(*copied_at), Value::Integer(*id)]);
        }
        Some(Cursor::Manual { fav_order, id }) => {
            w.push("(c.fav_order, c.id) > (?, ?)".into());
            p.extend([Value::Real(*fav_order), Value::Integer(*id)]);
        }
        None => {}
    }
    let order_by = match order {
        Order::Recent => "c.copied_at DESC, c.id DESC",
        Order::FavFirst => "c.favorite DESC, c.copied_at DESC, c.id DESC",
        Order::Manual => "c.fav_order ASC, c.id ASC",
    };
    p.push(Value::Integer(fetch));
    Built {
        sql: format!("SELECT {CLIP_COLS} FROM clips c WHERE {} ORDER BY {order_by} LIMIT ?", w.join(" AND ")),
        params: p,
    }
}

/// The two counter statements (kept here so the plan test checks exactly these).
pub(crate) const COUNTS_SQL: &str = "SELECT kind, count(*) FROM clips WHERE deleted_at IS NULL GROUP BY kind
     UNION ALL SELECT '#fav', count(*) FROM clips WHERE favorite = 1 AND deleted_at IS NULL
     UNION ALL SELECT '#snippets', count(*) FROM snippets";
pub(crate) const FOLDER_COUNTS_SQL: &str =
    "SELECT f.id, (SELECT count(*) FROM clips c WHERE c.folder_id = f.id AND c.deleted_at IS NULL) FROM folders f";

impl Store {
    /// One page of the feed. `favorites_first` = `settings.picker.favoritesFirst`.
    pub(crate) fn clips_list(&self, q: &FeedQuery, favorites_first: bool) -> Result<Page, String> {
        let limit = normalize_limit(q.limit);
        let order = if q.filter.favorites == Some(true) {
            Order::Manual
        } else if favorites_first {
            Order::FavFirst
        } else {
            Order::Recent
        };
        let cursor = q.cursor.as_deref().filter(|s| !s.is_empty()).map(|s| Cursor::decode(s, order)).transpose()?;
        let built = build_feed(q, order, cursor.as_ref(), limit + 1);
        let conn = self.lock();
        let mut st = conn.prepare(&built.sql).map_err(db_err)?;
        let rows = st.query_map(params_from_iter(built.params.iter()), row_to_clip).map_err(db_err)?;
        let mut items: Vec<(Clip, Option<f64>)> = rows.collect::<Result<_, _>>().map_err(db_err)?;
        let more = items.len() as i64 > limit;
        items.truncate(limit as usize);
        let next_cursor = match items.last() {
            Some((c, fo)) if more => Some(Cursor::after(order, c, *fo).encode()),
            _ => None,
        };
        Ok(Page { items: items.into_iter().map(|(c, _)| c).collect(), next_cursor })
    }

    /// Pill counters in two aggregate statements.
    pub(crate) fn counts(&self) -> Result<Counts, String> {
        let conn = self.lock();
        let mut counts = Counts { all: 0, kinds: PerKind::default(), favorites: 0, snippets: 0, folders: BTreeMap::new() };
        let mut st = conn.prepare(COUNTS_SQL).map_err(db_err)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).map_err(db_err)?;
        for row in rows {
            let (tag, n) = row.map_err(db_err)?;
            match tag.as_str() {
                "#fav" => counts.favorites = n,
                "#snippets" => counts.snippets = n,
                kind => {
                    counts.all += n;
                    if let Some(k) = Kind::parse(kind) {
                        *counts.kinds.get_mut(k) += n;
                    }
                }
            }
        }
        let mut st = conn.prepare(FOLDER_COUNTS_SQL).map_err(db_err)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(db_err)?;
        for row in rows {
            let (id, n) = row.map_err(db_err)?;
            counts.folders.insert(id, n);
        }
        Ok(counts)
    }

    /// Quick Look record: full text, metrics, files with `exists`/`size` (stat outside the
    /// DB lock, at most `FILES_MAX` paths), image URL, OCR text.
    pub(crate) fn clip_detail(&self, id: i64) -> Result<ClipDetail, String> {
        type DetailRow = ((Clip, Option<f64>), Option<String>, Option<i64>, Option<i64>, Option<String>, Option<Vec<u8>>);
        let row: DetailRow = {
            let conn = self.lock();
            conn.query_row(
                &format!(
                    "SELECT {CLIP_COLS}, c.text, c.word_count, c.line_count, c.ocr_text,
                       (SELECT data FROM clip_formats WHERE clip_id = c.id AND format = 'files')
                     FROM clips c WHERE c.id = ?1 AND c.deleted_at IS NULL"
                ),
                [id],
                |r| {
                    let n = CLIP_COLS_N;
                    Ok((row_to_clip(r)?, r.get(n)?, r.get(n + 1)?, r.get(n + 2)?, r.get(n + 3)?, r.get(n + 4)?))
                },
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| format!("not_found: clip {id}"))?
        };
        let ((clip, _), text, word_count, line_count, ocr_text, files_blob) = row;
        let files = (clip.kind == Kind::File).then(|| {
            let paths: Vec<String> = files_blob
                .as_deref()
                .and_then(FilesBlob::parse)
                .map(|f| f.paths)
                .unwrap_or_else(|| text.as_deref().unwrap_or("").lines().filter(|l| !l.is_empty()).map(str::to_string).collect());
            // An offline network share can make each stat slow: past the budget the rest is
            // reported as "exists, size unknown" rather than as a false "file not found".
            let deadline = Instant::now() + STAT_BUDGET;
            paths
                .into_iter()
                .take(FILES_MAX)
                .map(|path| {
                    if Instant::now() > deadline {
                        return ClipFile { path, exists: true, size: None };
                    }
                    match std::fs::metadata(&path) {
                        Ok(m) => ClipFile { size: m.is_file().then(|| m.len() as i64), exists: true, path },
                        Err(_) => ClipFile { path, exists: false, size: None },
                    }
                })
                .collect()
        });
        let image_url = (clip.kind == Kind::Image).then(|| media_url(clip.id, clip.created_at));
        Ok(ClipDetail { clip, text, word_count, line_count, files, image_url, ocr_text })
    }

    /// Source apps that have live records, most used first.
    pub(crate) fn apps_list(&self) -> Result<Vec<AppInfo>, String> {
        let conn = self.lock();
        let mut st = conn
            .prepare(
                "SELECT id, exe, name, icon_path, n FROM (
                   SELECT a.id, a.exe, a.name, a.icon_path,
                     (SELECT count(*) FROM clips c WHERE c.app_id = a.id AND c.deleted_at IS NULL) AS n
                   FROM apps a)
                 WHERE n > 0 ORDER BY n DESC, name COLLATE NOCASE, id",
            )
            .map_err(db_err)?;
        let rows = st
            .query_map([], |r| {
                let id: i64 = r.get(0)?;
                let exe: String = r.get(1)?;
                let icon: Option<String> = r.get(3)?;
                Ok(AppInfo {
                    id,
                    icon_url: icon.map(|_| icon_url(id, &exe)),
                    exe,
                    name: r.get(2)?,
                    clip_count: Some(r.get(4)?),
                })
            })
            .map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    /// Live record count, `liqlip.db` + WAL size, `media\` size (thumbnails included).
    pub(crate) fn storage_stats(&self) -> Result<StorageStats, String> {
        let clips: i64 = self
            .lock()
            .query_row("SELECT count(*) FROM clips WHERE deleted_at IS NULL", [], |r| r.get(0))
            .map_err(db_err)?;
        let len = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len() as i64).unwrap_or(0);
        Ok(StorageStats {
            clips,
            db_bytes: len(&self.paths().db) + len(&self.paths().wal()),
            media_bytes: super::media::dir_size(&self.paths().media),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{text_clip, TestStore};
    use super::*;
    use crate::model::{Filter, SnippetInput};

    fn q(text: &str) -> FeedQuery {
        FeedQuery { text: text.into(), exclude: Vec::new(), filter: Filter::default(), cursor: None, limit: 40 }
    }

    fn ids(page: &Page) -> Vec<i64> {
        page.items.iter().map(|c| c.id).collect()
    }

    /// Every page via the cursor.
    fn all_pages(t: &TestStore, mut query: FeedQuery, fav_first: bool) -> Vec<i64> {
        let mut out = Vec::new();
        for _ in 0..1_000 {
            let page = t.clips_list(&query, fav_first).unwrap();
            out.extend(ids(&page));
            match page.next_cursor {
                Some(c) => query.cursor = Some(c),
                None => return out,
            }
        }
        panic!("cursor never ended");
    }

    #[test]
    fn cursor_walks_every_row_once_across_page_borders() {
        let t = TestStore::new();
        // 95 rows, with equal copied_at values straddling page borders (tie → id DESC).
        let mut expect = Vec::new();
        for i in 0..95 {
            expect.push(t.add_text(&format!("row {i}"), 1_000 + (i / 3)));
        }
        expect.reverse(); // newest first; equal times → higher id first
        let mut query = q("");
        query.limit = 10;
        assert_eq!(all_pages(&t, query.clone(), false), expect);
        query.limit = 95;
        let page = t.clips_list(&query, false).unwrap();
        assert_eq!(page.items.len(), 95);
        assert!(page.next_cursor.is_none(), "an exactly full last page has no cursor");
        query.limit = 94;
        assert!(t.clips_list(&query, false).unwrap().next_cursor.is_some());
        // limit normalisation
        query.limit = 0;
        assert_eq!(t.clips_list(&query, false).unwrap().items.len(), 40);
        query.limit = 10_000;
        assert_eq!(t.clips_list(&query, false).unwrap().items.len(), 95);
    }

    #[test]
    fn cursor_is_opaque_and_checked() {
        let t = TestStore::new();
        for i in 0..5 {
            t.add_text(&format!("c{i}"), 1_000 + i);
        }
        let mut query = q("");
        query.limit = 2;
        let page = t.clips_list(&query, false).unwrap();
        let cur = page.next_cursor.unwrap();
        assert!(!cur.contains(':'), "opaque");
        query.cursor = Some("garbage!!".into());
        assert_eq!(t.clips_list(&query, false).unwrap_err(), "invalid: cursor");
        query.cursor = Some(cur.clone());
        assert_eq!(t.clips_list(&query, true).unwrap_err(), "invalid: cursor", "order changed");
        let m = Cursor::Manual { fav_order: 1.5, id: 3 };
        assert_eq!(Cursor::decode(&m.encode(), Order::Manual).unwrap(), m);
        query.cursor = Some(String::new());
        assert_eq!(t.clips_list(&query, false).unwrap().items.len(), 2, "empty cursor = first page");
    }

    #[test]
    fn filters_combine() {
        let t = TestStore::new();
        let a = t.add_text_from("alpha text", 1_000, "c:\\a.exe");
        let b = t.add_text_from("bravo text", 2_000, "c:\\b.exe");
        let img = t.add_image(8, 8, 3_000, false);
        let f = t.add_files(&["c:\\x\\report.pdf"], 4_000);
        let folder = t.folder_create("Box").unwrap();
        t.set_folder(&[b.id, img], Some(folder.id)).unwrap();
        t.set_favorite(&[a.id], true).unwrap();
        let run = |filter: Filter| ids(&t.clips_list(&FeedQuery { filter, ..q("") }, false).unwrap());
        assert_eq!(run(Filter::default()), vec![f, img, b.id, a.id]);
        assert_eq!(run(Filter { kinds: Some(vec![Kind::Image, Kind::File]), ..Default::default() }), vec![f, img]);
        assert_eq!(run(Filter { kinds: Some(vec![]), ..Default::default() }).len(), 4, "empty kinds = all");
        assert_eq!(run(Filter { favorites: Some(true), ..Default::default() }), vec![a.id]);
        assert_eq!(run(Filter { favorites: Some(false), ..Default::default() }).len(), 4);
        assert_eq!(run(Filter { folder_id: Some(folder.id), ..Default::default() }), vec![img, b.id]);
        assert_eq!(run(Filter { app_ids: Some(vec![a.app_id.unwrap()]), ..Default::default() }), vec![a.id]);
        assert_eq!(run(Filter { since: Some(2_000), until: Some(3_000), ..Default::default() }), vec![img, b.id]);
        assert_eq!(
            run(Filter { folder_id: Some(folder.id), kinds: Some(vec![Kind::Text]), ..Default::default() }),
            vec![b.id]
        );
        // Soft-deleted rows never show.
        t.delete(&[f]).unwrap();
        assert_eq!(run(Filter::default()), vec![img, b.id, a.id]);
        // Clip fields.
        let page = t.clips_list(&q(""), false).unwrap();
        let im = page.items.iter().find(|c| c.id == img).unwrap();
        assert_eq!(im.thumb_url.as_deref(), Some(thumb_url(img, 3_000).as_str()));
        assert_eq!((im.width, im.height, im.folder_id), (Some(8), Some(8), Some(folder.id)));
        assert_eq!(im.formats, vec![Format::Png]);
        let fa = page.items.iter().find(|c| c.id == a.id).unwrap();
        assert!(fa.favorite && fa.thumb_url.is_none());
        assert_eq!(fa.formats, vec![Format::Text]);
    }

    #[test]
    fn favorites_first_and_manual_order() {
        let t = TestStore::new();
        let ids_: Vec<i64> = (0..6).map(|i| t.add_text(&format!("item {i}"), 1_000 + i)).collect();
        t.set_favorite(&[ids_[1], ids_[4]], true).unwrap();
        let mut query = q("");
        query.limit = 2;
        assert_eq!(
            all_pages(&t, query.clone(), true),
            vec![ids_[4], ids_[1], ids_[5], ids_[3], ids_[2], ids_[0]]
        );
        t.move_favorite(ids_[4], None).unwrap();
        query.filter.favorites = Some(true);
        query.limit = 1;
        assert_eq!(all_pages(&t, query, false), vec![ids_[1], ids_[4]]);
    }

    #[test]
    fn search_fts_cyrillic_short_terms_and_exclusions() {
        let t = TestStore::new();
        let ru = t.add_text("Встреча переносится на Четверг", 1_000);
        let en = t.add_text("Meeting moved to Thursday", 2_000);
        let quote = t.add_text("He said \"hello\" (twice) AND left; col:val * NEAR", 3_000);
        let ok = t.add_text("ok", 4_000);
        let long_tail = t.add_text(&format!("{} needle-at-the-end", "x ".repeat(600)), 5_000);
        let find = |text: &str| ids(&t.clips_list(&q(text), false).unwrap());
        assert_eq!(find("четверг"), vec![ru], "Cyrillic, case-insensitive");
        assert_eq!(find("ЧЕТВ"), vec![ru], "substring");
        assert_eq!(find("thursday meeting"), vec![en], "AND of terms, any order");
        assert_eq!(find("\"moved to\""), vec![en], "quoted phrase");
        assert!(find("\"to moved\"").is_empty());
        assert_eq!(find("needle-at-the-end"), vec![long_tail], "full text, beyond the preview");
        // FTS syntax characters are literal.
        assert_eq!(find("\"hello\""), vec![quote]);
        assert_eq!(find("(twice)"), vec![quote]);
        assert_eq!(find("AND left;"), vec![quote]);
        assert_eq!(find("col:val"), vec![quote]);
        assert_eq!(find("* NEAR"), vec![quote]);
        assert_eq!(find("\"unclosed phrase AND"), Vec::<i64>::new());
        // < 3 characters: preview/title substring, Unicode case-insensitive.
        assert_eq!(find("OK"), vec![ok]);
        assert_eq!(find("Вс"), vec![ru]);
        t.rename(en, Some("Ёж")).unwrap();
        assert_eq!(find("ёж"), vec![en], "title is searched");
        assert_eq!(find("ёж thursday"), vec![en], "mixed short + long terms");
        // Exclusions.
        let mut query = q("");
        query.exclude = vec!["thursday".into(), "ok".into()];
        let got = ids(&t.clips_list(&query, false).unwrap());
        assert!(!got.contains(&en) && !got.contains(&ok) && got.contains(&ru));
        query.text = "e".into();
        query.exclude = vec!["\"said \"".into()];
        let got = ids(&t.clips_list(&query, false).unwrap());
        assert!(got.contains(&en) && !got.contains(&quote));
        // Titles are indexed by FTS too.
        t.rename(ok, Some("Groceries list")).unwrap();
        assert_eq!(find("grocer"), vec![ok]);
        // Search + filter + cursor.
        for i in 0..30 {
            t.add_text(&format!("common token {i}"), 10_000 + i);
        }
        let mut query = q("common token");
        query.limit = 7;
        let all = all_pages(&t, query, false);
        assert_eq!(all.len(), 30);
        let mut sorted = all.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(all, sorted, "search keeps the feed order");
    }

    #[test]
    fn parse_terms_rules() {
        assert_eq!(parse_terms("  a  b\tc "), vec!["a", "b", "c"]);
        assert_eq!(parse_terms("\"one two\" three"), vec!["one two", "three"]);
        assert_eq!(parse_terms("x \"open quote"), vec!["x", "open quote"]);
        assert_eq!(parse_terms("Dup dup DUP"), vec!["Dup"]);
        assert_eq!(parse_terms(""), Vec::<String>::new());
        assert_eq!(fts_quote("a\"b"), "\"a\"\"b\"");
        assert_eq!(parse_terms(&"w ".repeat(100)).len(), 1);
        let many: String = (0..100).map(|i| format!("t{i} ")).collect();
        assert_eq!(parse_terms(&many).len(), TERMS_MAX);
    }

    #[test]
    fn counts_are_aggregated() {
        let t = TestStore::new();
        let a = t.add_text("a1", 1);
        t.add_text("a2", 2);
        let img = t.add_image(4, 4, 3, false);
        t.add_files(&["c:\\f"], 4);
        let gone = t.add_text("gone", 5);
        t.delete(&[gone]).unwrap();
        let f1 = t.folder_create("One").unwrap();
        let f2 = t.folder_create("Two").unwrap();
        t.set_folder(&[a, img], Some(f1.id)).unwrap();
        t.set_favorite(&[a], true).unwrap();
        t.snippet_save(&SnippetInput { id: None, name: "S".into(), body: "b".into(), keyword: None }).unwrap();
        let c = t.counts().unwrap();
        assert_eq!(c.all, 4);
        assert_eq!((c.kinds.text, c.kinds.image, c.kinds.file, c.kinds.link), (2, 1, 1, 0));
        assert_eq!(c.favorites, 1);
        assert_eq!(c.snippets, 1);
        assert_eq!(c.folders.get(&f1.id), Some(&2));
        assert_eq!(c.folders.get(&f2.id), Some(&0), "empty folders are listed with 0");
    }

    #[test]
    fn detail_files_and_image() {
        let t = TestStore::new();
        let real = t.dir().join("real.txt");
        std::fs::write(&real, b"12345").unwrap();
        let real_s = real.to_string_lossy().into_owned();
        let missing = t.dir().join("missing.txt").to_string_lossy().into_owned();
        let dir_s = t.dir().to_string_lossy().into_owned();
        let f = t.add_files(&[&real_s, &missing, &dir_s], 1_000);
        let d = t.clip_detail(f).unwrap();
        let files = d.files.unwrap();
        assert_eq!(files.len(), 3);
        assert!(files[0].exists && files[0].size == Some(5));
        assert!(!files[1].exists && files[1].size.is_none());
        assert!(files[2].exists && files[2].size.is_none(), "folders have no size");
        let txt = t.insert_clip(text_clip("line one\nline two", 2_000)).unwrap().id;
        let d = t.clip_detail(txt).unwrap();
        assert_eq!(d.text.as_deref(), Some("line one\nline two"));
        assert_eq!((d.word_count, d.line_count), (Some(4), Some(2)));
        assert!(d.files.is_none() && d.image_url.is_none());
        let img = t.add_image(4, 4, 3_000, false);
        assert_eq!(t.clip_detail(img).unwrap().image_url, Some(media_url(img, 3_000)));
        assert!(t.clip_detail(9_999).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn apps_list_and_storage_stats() {
        let t = TestStore::new();
        let a = t.add_text_from("one", 1, "c:\\apps\\one.exe");
        t.add_text_from("two", 2, "c:\\apps\\one.exe");
        let b = t.add_text_from("three", 3, "c:\\apps\\two.exe");
        t.delete(&[b.id]).unwrap();
        let apps = t.apps_list().unwrap();
        assert_eq!(apps.len(), 1, "apps without live records are hidden");
        assert_eq!((apps[0].id, apps[0].clip_count, apps[0].name.as_str()), (a.app_id.unwrap(), Some(2), "App"));
        assert!(apps[0].icon_url.is_none());
        t.set_app_icon(a.app_id.unwrap(), &super::super::testutil::png_bytes(8, 8)).unwrap();
        let url = t.apps_list().unwrap()[0].icon_url.clone().unwrap();
        assert!(url.starts_with(&format!("http://liq.localhost/icon/{}?v=", a.app_id.unwrap())));
        t.add_image(16, 16, 4, true);
        let s = t.storage_stats().unwrap();
        assert_eq!(s.clips, 3);
        assert!(s.db_bytes > 0 && s.media_bytes > 0);
    }
}

/// `EXPLAIN QUERY PLAN` of every hot statement: the feed (each order, each filter, search,
/// exclusions, cursor), the counters and the retention / purge / delete-recent scans must
/// use an index — never a plain `SCAN` of `clips` (§2: performance by construction, no
/// 50k-row bench). Indexes are checked by name where the choice matters.
#[cfg(test)]
mod plans {
    use super::super::lifecycle::{PURGE_SQL, RECENT_SQL, RETENTION_AGE_SQL, RETENTION_OLDEST_SQL, RETENTION_TOTALS_SQL};
    use super::super::testutil::TestStore;
    use super::*;
    use crate::model::Filter;

    fn plan(t: &TestStore, sql: &str, params: &[Value]) -> Vec<String> {
        let c = t.lock();
        let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let rows = st.query_map(params_from_iter(params.iter()), |r| r.get::<_, String>(3)).unwrap();
        rows.map(Result::unwrap).collect()
    }

    /// A plain `SCAN clips` / `SCAN c` (no index) anywhere in the plan.
    fn full_scan(plan: &[String]) -> bool {
        plan.iter().any(|d| {
            let d = d.trim();
            (d == "SCAN c" || d == "SCAN clips" || d.starts_with("SCAN c ") || d.starts_with("SCAN clips "))
                && !d.contains("USING")
        })
    }

    fn uses(plan: &[String], needle: &str) -> bool {
        plan.iter().any(|d| d.contains(needle))
    }

    fn feed(t: &TestStore, filter: Filter, text: &str, exclude: &[&str], order: Order, cursor: bool) -> Vec<String> {
        let q = FeedQuery {
            text: text.into(),
            exclude: exclude.iter().map(|s| s.to_string()).collect(),
            filter,
            cursor: None,
            limit: 40,
        };
        let cur = cursor.then_some(match order {
            Order::Recent => Cursor::Recent { copied_at: 5, id: 5 },
            Order::FavFirst => Cursor::FavFirst { favorite: 1, copied_at: 5, id: 5 },
            Order::Manual => Cursor::Manual { fav_order: 2.0, id: 5 },
        });
        let b = build_feed(&q, order, cur.as_ref(), 41);
        plan(t, &b.sql, &b.params)
    }

    #[test]
    fn hot_statements_use_indexes() {
        let t = TestStore::new();
        for i in 0..300 {
            t.add_text(&format!("seed row {i}"), 1_000 + i);
        }
        let kind = |k: Vec<Kind>| Filter { kinds: Some(k), ..Default::default() };
        let cases: Vec<(&str, Vec<String>, &str)> = vec![
            ("recent", feed(&t, Filter::default(), "", &[], Order::Recent, false), "idx_clips_feed"),
            ("recent+cursor", feed(&t, Filter::default(), "", &[], Order::Recent, true), "idx_clips_feed (copied_at<?)"),
            ("favfirst+cursor", feed(&t, Filter::default(), "", &[], Order::FavFirst, true), "idx_clips_favfirst"),
            (
                "favorites pill+cursor",
                feed(&t, Filter { favorites: Some(true), ..Default::default() }, "", &[], Order::Manual, true),
                "idx_clips_favfirst (favorite=?)",
            ),
            ("kind+cursor", feed(&t, kind(vec![Kind::Link]), "", &[], Order::Recent, true), "idx_clips_kind (kind=? AND copied_at<?)"),
            ("kinds", feed(&t, kind(vec![Kind::Text, Kind::Contact]), "", &[], Order::Recent, true), "idx_clips_kind"),
            (
                "folder+cursor",
                feed(&t, Filter { folder_id: Some(1), ..Default::default() }, "", &[], Order::Recent, true),
                "idx_clips_folder (folder_id=? AND copied_at<?)",
            ),
            ("apps", feed(&t, Filter { app_ids: Some(vec![1, 2]), ..Default::default() }, "", &[], Order::Recent, true), "idx_clips_app"),
            (
                "since/until",
                feed(&t, Filter { since: Some(1), until: Some(9), ..Default::default() }, "", &[], Order::Recent, false),
                "idx_clips_feed (copied_at>? AND copied_at<?)",
            ),
            ("fts+cursor", feed(&t, Filter::default(), "seed row", &[], Order::Recent, true), "VIRTUAL TABLE"),
            ("fts+kind", feed(&t, kind(vec![Kind::Text]), "seed", &[], Order::Recent, false), "VIRTUAL TABLE"),
            ("short term", feed(&t, Filter::default(), "ok", &[], Order::Recent, false), "idx_clips_feed"),
            ("exclusions", feed(&t, Filter::default(), "", &["seed", "x"], Order::Recent, false), "VIRTUAL TABLE"),
            ("counts", plan(&t, COUNTS_SQL, &[]), "COVERING INDEX idx_clips_kind"),
            ("folder counts", plan(&t, FOLDER_COUNTS_SQL, &[]), "idx_clips_folder (folder_id=?)"),
            (
                "retention age",
                plan(&t, RETENTION_AGE_SQL, &[Value::Integer(5), Value::Integer(500)]),
                "COVERING INDEX idx_clips_history (copied_at<?)",
            ),
            ("retention oldest", plan(&t, RETENTION_OLDEST_SQL, &[Value::Integer(500)]), "COVERING INDEX idx_clips_history"),
            ("retention totals", plan(&t, RETENTION_TOTALS_SQL, &[]), "COVERING INDEX idx_clips_history"),
            ("purge", plan(&t, PURGE_SQL, &[Value::Integer(5), Value::Integer(500)]), "idx_clips_deleted (deleted_at<?)"),
            ("delete recent", plan(&t, RECENT_SQL, &[Value::Integer(5)]), "idx_clips_deleted"),
            ("hash lookup", plan(&t, "SELECT id FROM clips WHERE hash = ?1", &[Value::Text("x".into())]), "sqlite_autoindex_clips"),
        ];
        for (name, p, needle) in &cases {
            eprintln!("plan {name}: {p:?}");
            assert!(!full_scan(p), "{name}: full scan of clips: {p:?}");
            assert!(uses(p, needle), "{name}: expected {needle:?} in {p:?}");
        }
        // Keyset pages need no sort in the default feed and the single-kind / folder pills.
        for (name, p, _) in cases.iter().filter(|c| ["recent+cursor", "kind+cursor", "folder+cursor"].contains(&c.0)) {
            assert!(!uses(p, "TEMP B-TREE"), "{name}: unexpected sort: {p:?}");
        }
    }
}
