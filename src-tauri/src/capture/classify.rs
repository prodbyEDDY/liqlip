//! Text classification (SPEC §2.1) — owner: **W1-B** (port of Liquid Tools
//! `clipboard/capture.rs::classify_text` with the SPEC fixes). Pure logic, unit-tested.
//! Shared: the store (W1-A) uses it for `clip_create_text` / `clip_update_text`.

use crate::model::{ClipMeta, Kind};

/// Classify a whole text: `link` (one URL line; `meta.url/host`), `color` (`#rgb`,
/// `#rrggbb`, `#rrggbbaa`, `rgb()/rgba()/hsl()/hsla()`; `meta.color`), `contact`
/// (`meta.contact`), `code` (≥ 2 heuristics) or `text`. Never fails.
#[allow(dead_code)] // W1-A / W1-B call it
pub fn classify_text(text: &str) -> (Kind, ClipMeta) {
    let _ = text; // W1-B
    (Kind::Text, ClipMeta::default())
}
