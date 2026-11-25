//! Content identity and content rules shared by capture (W1-B) and the store (W1-A).
//! Coordinator module (W0): there must be exactly ONE hashing algorithm in Liqlip —
//! dedup (`clips.hash UNIQUE`) breaks silently if two modules hash differently.
//!
//! Hash: XXH3-128 (`xxhash-rust`, https://docs.rs/xxhash-rust/0.8.21 — output is
//! stable by the xxHash spec since v0.8.0), seed 0, rendered as 32 lowercase hex chars.
//! Input layout (never change after release — it would orphan every stored hash):
//!
//! ```text
//! b"liqlip:1\0" ‖ family ‖ b"\0" ‖ canonical bytes
//! family: "text" for text|link|code|color|contact, "image", "file"
//! ```
//!
//! The family (not the exact kind) is hashed so that a better classifier in a later
//! version still bumps the old record instead of creating a twin (the text is the
//! identity; "link" vs "text" is a label). Canonical bytes (ARCHITECTURE §4.2):
//! - text: the UTF-8 text exactly as stored (after the 2 MB truncation), no trimming;
//! - image: `width u32 LE ‖ height u32 LE ‖ RGBA8 pixels` of the decoded image — pixels,
//!   not PNG bytes, so the identity does not depend on the PNG encoder version;
//! - files: paths lower-cased, sorted, joined with `\n`.

// Shared API surface for W1-A / W1-B: unused until those modules land.
#![allow(dead_code)] // W1-A, W1-B

use crate::model::Kind;
use xxhash_rust::xxh3::Xxh3;

/// Card preview length (`clips.preview`), in Unicode scalar values.
pub const PREVIEW_MAX_CHARS: usize = 512;
/// Text longer than this is stored truncated (`meta.truncated = true`), SPEC §2.3.
pub const TEXT_MAX_BYTES: usize = 2 * 1024 * 1024;
/// At most this many paths of one CF_HDROP copy are kept, SPEC §2.3.
pub const FILES_MAX: usize = 2000;
/// Images above 64 megapixels are not stored, SPEC §2.3.
pub const IMAGE_MAX_PIXELS: u64 = 64_000_000;

const DOMAIN: &[u8] = b"liqlip:1\0";

fn family(kind: Kind) -> &'static [u8] {
    match kind {
        Kind::Text | Kind::Link | Kind::Code | Kind::Color | Kind::Contact => b"text",
        Kind::Image => b"image",
        Kind::File => b"file",
    }
}

fn hasher(kind: Kind) -> Xxh3 {
    let mut h = Xxh3::new();
    h.update(DOMAIN);
    h.update(family(kind));
    h.update(b"\0");
    h
}

fn hex(h: u128) -> String {
    format!("{h:032x}")
}

/// THE content hash. `canonical` must come from [`canonical_text`] or
/// [`canonical_files`]; for images use [`image_hash`] (same result, no pixel copy).
pub fn content_hash(kind: Kind, canonical: &[u8]) -> String {
    let mut h = hasher(kind);
    h.update(canonical);
    hex(h.digest128())
}

/// `content_hash(Kind::Image, width ‖ height ‖ rgba)` without concatenating the pixels.
pub fn image_hash(width: u32, height: u32, rgba: &[u8]) -> String {
    let mut h = hasher(Kind::Image);
    h.update(&width.to_le_bytes());
    h.update(&height.to_le_bytes());
    h.update(rgba);
    hex(h.digest128())
}

/// Canonical bytes of a text record: the text as stored.
pub fn canonical_text(text: &str) -> &[u8] {
    text.as_bytes()
}

/// Canonical bytes of a file-list record: lower-cased, sorted, `\n`-joined paths.
pub fn canonical_files(paths: &[String]) -> Vec<u8> {
    let mut lower: Vec<String> = paths.iter().map(|p| p.to_lowercase()).collect();
    lower.sort();
    lower.join("\n").into_bytes()
}

/// Truncate `text` to at most [`TEXT_MAX_BYTES`] on a char boundary.
/// Returns the stored text and whether it was truncated.
pub fn truncate_text(text: &str) -> (&str, bool) {
    if text.len() <= TEXT_MAX_BYTES {
        return (text, false);
    }
    let mut end = TEXT_MAX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// Card preview: the first [`PREVIEW_MAX_CHARS`] scalar values (line breaks kept —
/// the card shows up to 7 lines).
pub fn make_preview(text: &str) -> String {
    match text.char_indices().nth(PREVIEW_MAX_CHARS) {
        Some((cut, _)) => text[..cut].to_string(),
        None => text.to_string(),
    }
}

/// Character / word / line counts shown in Quick Look (`char_count`, `word_count`,
/// `line_count`). Computed once, by whoever creates or edits the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextMetrics {
    pub chars: i64,
    pub words: i64,
    pub lines: i64,
}

pub fn text_metrics(text: &str) -> TextMetrics {
    TextMetrics {
        chars: text.chars().count() as i64,
        words: text.split_whitespace().count() as i64,
        lines: text.lines().count() as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xxhash_rust::xxh3::xxh3_128;

    #[test]
    fn xxh3_128_matches_reference_vector() {
        // XXH3_128bits("") with seed 0, from the xxHash reference implementation.
        assert_eq!(xxh3_128(b""), 0x99aa06d3014798d86001c324468d497f);
    }

    #[test]
    fn hash_is_stable() {
        // Pinned values: if this test fails, the hash changed and every stored
        // record would stop deduplicating. Never "fix" it by updating the constants.
        assert_eq!(
            content_hash(Kind::Text, canonical_text("Hello, Liqlip")),
            "359d86775786be92bdc3e169c2f38d09"
        );
        assert_eq!(content_hash(Kind::Text, canonical_text("Привет")).len(), 32);
    }

    #[test]
    fn family_not_kind_is_hashed() {
        let t = canonical_text("https://example.com");
        assert_eq!(content_hash(Kind::Text, t), content_hash(Kind::Link, t));
        assert_eq!(content_hash(Kind::Code, t), content_hash(Kind::Color, t));
        assert_ne!(content_hash(Kind::Text, t), content_hash(Kind::File, t));
        assert_ne!(content_hash(Kind::Text, t), content_hash(Kind::Image, t));
    }

    #[test]
    fn image_hash_equals_concatenated_form() {
        let rgba = [1u8, 2, 3, 255, 4, 5, 6, 255];
        let mut canon = Vec::new();
        canon.extend_from_slice(&2u32.to_le_bytes());
        canon.extend_from_slice(&1u32.to_le_bytes());
        canon.extend_from_slice(&rgba);
        assert_eq!(image_hash(2, 1, &rgba), content_hash(Kind::Image, &canon));
        assert_ne!(image_hash(1, 2, &rgba), image_hash(2, 1, &rgba));
    }

    #[test]
    fn files_are_order_and_case_insensitive() {
        let a = vec!["C:\\B.txt".to_string(), "c:\\a.txt".to_string()];
        let b = vec!["c:\\A.TXT".to_string(), "c:\\b.txt".to_string()];
        assert_eq!(
            content_hash(Kind::File, &canonical_files(&a)),
            content_hash(Kind::File, &canonical_files(&b))
        );
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let s = "ж".repeat(TEXT_MAX_BYTES); // 2 bytes each → twice the limit
        let (t, cut) = truncate_text(&s);
        assert!(cut);
        assert!(t.len() <= TEXT_MAX_BYTES);
        assert!(t.chars().all(|c| c == 'ж'));
        assert_eq!(truncate_text("short"), ("short", false));
    }

    #[test]
    fn preview_and_metrics() {
        let long = "я".repeat(PREVIEW_MAX_CHARS + 10);
        assert_eq!(make_preview(&long).chars().count(), PREVIEW_MAX_CHARS);
        assert_eq!(make_preview("abc"), "abc");
        assert_eq!(text_metrics(""), TextMetrics { chars: 0, words: 0, lines: 0 });
        assert_eq!(
            text_metrics("Привет мир\r\nsecond line\n"),
            TextMetrics { chars: 24, words: 4, lines: 2 }
        );
    }
}
