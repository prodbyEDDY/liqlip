//! What a paste / copy puts on the clipboard (SPEC §3.7 "Вставить как…", §3.11, §6.5) —
//! pure logic over the store's [`PastePayload`]s, owner **W1-C**.
//!
//! One record:
//! - `auto` — text kinds: text + HTML + RTF (only text with `picker.alwaysPlain`); image:
//!   PNG + DIB; file: CF_HDROP + text paths;
//! - `plain` — `CF_UNICODETEXT` only (files → their paths); an image has no text, so it
//!   pastes as the image (Shift+Enter on an image card still does something useful);
//! - `{ format }` — exactly that representation (+ the plain text as the fallback for HTML /
//!   RTF); a format the record does not have → `invalid`.
//!
//! Several records (multi-select "Вставить всё", the paste stack):
//! - all non-image records are files → ONE CF_HDROP with the union of the paths;
//! - otherwise → text joined by the separator (files contribute their paths);
//! - images cannot be joined → left out, the outcome carries `reason: 'partial'`; if there is
//!   nothing but images, the first image is pasted (also `partial`);
//! - `{ format: 'files' }` → union of the file records; `{ format: html|rtf|png }` needs a
//!   single record.

use super::writer::{paths_text, Content};
use crate::model::{Format, Kind, MergeSeparator, PasteMode};
use crate::store::PastePayload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composed {
    pub content: Content,
    /// Records actually included (→ `store.mark_used`).
    pub used: Vec<i64>,
    /// Some records were left out (images in a text paste, non-files in a file paste).
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeError {
    /// Nothing that could go on the clipboard.
    Nothing,
    /// The record has no such representation.
    FormatUnavailable(Format),
    /// HTML / RTF / PNG of several records at once.
    MultiFormat(Format),
}

impl ComposeError {
    pub fn to_command_error(&self) -> String {
        match self {
            ComposeError::Nothing => "invalid: nothing to paste".into(),
            ComposeError::FormatUnavailable(f) => format!("invalid: the record has no {} format", f.as_str()),
            ComposeError::MultiFormat(f) => format!("invalid: {} applies to a single record", f.as_str()),
        }
    }
}

/// `settings.picker.mergeSeparator` → the string between joined records.
pub fn merge_separator(sep: MergeSeparator) -> &'static str {
    match sep {
        MergeSeparator::Newline => "\r\n",
        MergeSeparator::Space => " ",
        MergeSeparator::Tab => "\t",
        MergeSeparator::None => "",
    }
}

/// A separator from the front (`"\n"`) in Windows text form: every lone LF becomes CRLF.
pub fn normalize_separator(sep: &str) -> String {
    let mut out = String::with_capacity(sep.len() + 2);
    let mut prev = '\0';
    for c in sep.chars() {
        if c == '\n' && prev != '\r' {
            out.push('\r');
        }
        out.push(c);
        prev = c;
    }
    out
}

fn is_texty(kind: Kind) -> bool {
    !matches!(kind, Kind::Image | Kind::File)
}

/// Paths of a file record (the structured list, else the `\n`-joined text).
pub fn file_paths(p: &PastePayload) -> Vec<String> {
    match &p.files {
        Some(f) if !f.paths.is_empty() => f.paths.clone(),
        _ => p
            .text
            .as_deref()
            .unwrap_or("")
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
    }
}

/// Plain-text form of a record (`None` for images).
fn text_of(p: &PastePayload) -> Option<String> {
    match p.kind {
        Kind::Image => None,
        Kind::File => Some(paths_text(&file_paths(p))),
        _ => Some(p.text.clone().unwrap_or_default()),
    }
}

fn single(p: &PastePayload, mode: PasteMode, always_plain: bool) -> Result<Content, ComposeError> {
    let text = || p.text.clone().unwrap_or_default();
    let image = || p.png_path.clone().map(|png_path| Content::Image { png_path }).ok_or(ComposeError::FormatUnavailable(Format::Png));
    let files = || {
        let paths = file_paths(p);
        if paths.is_empty() {
            Err(ComposeError::FormatUnavailable(Format::Files))
        } else {
            Ok(Content::Files { paths })
        }
    };
    match mode {
        PasteMode::Auto => match p.kind {
            Kind::Image => image(),
            Kind::File => files(),
            _ if always_plain => Ok(Content::Text { text: text(), html: None, rtf: None }),
            _ => Ok(Content::Text { text: text(), html: p.html.clone(), rtf: p.rtf.clone() }),
        },
        PasteMode::Plain => match text_of(p) {
            Some(text) => Ok(Content::Text { text, html: None, rtf: None }),
            None => image(),
        },
        PasteMode::Format(Format::Text) => {
            text_of(p).map(|text| Content::Text { text, html: None, rtf: None }).ok_or(ComposeError::FormatUnavailable(Format::Text))
        }
        PasteMode::Format(Format::Html) => match (&p.html, is_texty(p.kind)) {
            (Some(h), true) => Ok(Content::Text { text: text(), html: Some(h.clone()), rtf: None }),
            _ => Err(ComposeError::FormatUnavailable(Format::Html)),
        },
        PasteMode::Format(Format::Rtf) => match (&p.rtf, is_texty(p.kind)) {
            (Some(r), true) => Ok(Content::Text { text: text(), html: None, rtf: Some(r.clone()) }),
            _ => Err(ComposeError::FormatUnavailable(Format::Rtf)),
        },
        PasteMode::Format(Format::Png) => image(),
        PasteMode::Format(Format::Files) => files(),
    }
}

/// Case-insensitive de-duplication, first occurrence wins.
fn union_paths(payloads: &[&PastePayload]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in payloads {
        for path in file_paths(p) {
            if seen.insert(path.to_lowercase()) {
                out.push(path);
            }
        }
    }
    out
}

/// Build the clipboard content for `payloads` (in order). `sep` is already in Windows form
/// ([`normalize_separator`] / [`merge_separator`]).
pub fn compose(payloads: &[PastePayload], mode: PasteMode, sep: &str, always_plain: bool) -> Result<Composed, ComposeError> {
    match payloads {
        [] => Err(ComposeError::Nothing),
        [p] => single(p, mode, always_plain).map(|content| Composed { content, used: vec![p.id], partial: false }),
        _ => {
            if let PasteMode::Format(f @ (Format::Html | Format::Rtf | Format::Png)) = mode {
                return Err(ComposeError::MultiFormat(f));
            }
            let (images, others): (Vec<&PastePayload>, Vec<&PastePayload>) = payloads.iter().partition(|p| p.kind == Kind::Image);
            if mode == PasteMode::Format(Format::Files) {
                let files: Vec<&PastePayload> = payloads.iter().filter(|p| p.kind == Kind::File).collect();
                let paths = union_paths(&files);
                if paths.is_empty() {
                    return Err(ComposeError::FormatUnavailable(Format::Files));
                }
                return Ok(Composed {
                    content: Content::Files { paths },
                    used: files.iter().map(|p| p.id).collect(),
                    partial: files.len() != payloads.len(),
                });
            }
            if others.is_empty() {
                if mode == PasteMode::Format(Format::Text) {
                    return Err(ComposeError::FormatUnavailable(Format::Text));
                }
                let first = images.iter().find(|p| p.png_path.is_some()).ok_or(ComposeError::FormatUnavailable(Format::Png))?;
                return Ok(Composed {
                    content: Content::Image { png_path: first.png_path.clone().unwrap_or_default() },
                    used: vec![first.id],
                    partial: true,
                });
            }
            let partial = !images.is_empty();
            let used = others.iter().map(|p| p.id).collect();
            if mode == PasteMode::Auto && others.iter().all(|p| p.kind == Kind::File) {
                let paths = union_paths(&others);
                if !paths.is_empty() {
                    return Ok(Composed { content: Content::Files { paths }, used, partial });
                }
            }
            let text = others.iter().filter_map(|p| text_of(p)).collect::<Vec<_>>().join(sep);
            Ok(Composed { content: Content::Text { text, html: None, rtf: None }, used, partial })
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::ClipFiles;
    use std::path::PathBuf;

    pub(crate) fn text(id: i64, s: &str) -> PastePayload {
        PastePayload {
            id,
            kind: Kind::Text,
            text: Some(s.into()),
            html: None,
            rtf: None,
            png_path: None,
            files: None,
            formats: vec![Format::Text],
        }
    }

    pub(crate) fn rich(id: i64, s: &str) -> PastePayload {
        PastePayload {
            html: Some(format!("<b>{s}</b>")),
            rtf: Some(b"{\\rtf1 x}".to_vec()),
            formats: vec![Format::Text, Format::Html, Format::Rtf],
            ..text(id, s)
        }
    }

    pub(crate) fn image(id: i64) -> PastePayload {
        PastePayload {
            id,
            kind: Kind::Image,
            text: None,
            html: None,
            rtf: None,
            png_path: Some(PathBuf::from(format!("C:\\data\\media\\{id}.png"))),
            files: None,
            formats: vec![Format::Png],
        }
    }

    pub(crate) fn files(id: i64, paths: &[&str]) -> PastePayload {
        PastePayload {
            id,
            kind: Kind::File,
            text: Some(paths.join("\n")),
            html: None,
            rtf: None,
            png_path: None,
            files: Some(ClipFiles { paths: paths.iter().map(|s| s.to_string()).collect(), drop_effect: 2 }),
            formats: vec![Format::Files],
        }
    }

    fn content(r: Result<Composed, ComposeError>) -> Content {
        r.unwrap().content
    }

    #[test]
    fn single_auto_by_kind() {
        assert_eq!(
            content(compose(&[rich(1, "a")], PasteMode::Auto, "", false)),
            Content::Text { text: "a".into(), html: Some("<b>a</b>".into()), rtf: Some(b"{\\rtf1 x}".to_vec()) }
        );
        assert_eq!(
            content(compose(&[rich(1, "a")], PasteMode::Auto, "", true)),
            Content::Text { text: "a".into(), html: None, rtf: None },
            "picker.alwaysPlain"
        );
        assert_eq!(content(compose(&[image(2)], PasteMode::Auto, "", false)), Content::Image { png_path: "C:\\data\\media\\2.png".into() });
        assert_eq!(
            content(compose(&[files(3, &["C:\\a", "C:\\b"])], PasteMode::Auto, "", false)),
            Content::Files { paths: vec!["C:\\a".into(), "C:\\b".into()] }
        );
    }

    #[test]
    fn plain_is_text_only_and_image_falls_back_to_the_image() {
        assert_eq!(
            content(compose(&[rich(1, "a")], PasteMode::Plain, "", false)),
            Content::Text { text: "a".into(), html: None, rtf: None }
        );
        assert_eq!(
            content(compose(&[files(3, &["C:\\a", "C:\\b"])], PasteMode::Plain, "", false)),
            Content::Text { text: "C:\\a\r\nC:\\b".into(), html: None, rtf: None }
        );
        assert_eq!(content(compose(&[image(2)], PasteMode::Plain, "", false)), Content::Image { png_path: "C:\\data\\media\\2.png".into() });
    }

    #[test]
    fn explicit_formats() {
        let r = rich(1, "a");
        assert_eq!(
            content(compose(std::slice::from_ref(&r), PasteMode::Format(Format::Html), "", false)),
            Content::Text { text: "a".into(), html: Some("<b>a</b>".into()), rtf: None }
        );
        assert_eq!(
            content(compose(std::slice::from_ref(&r), PasteMode::Format(Format::Rtf), "", false)),
            Content::Text { text: "a".into(), html: None, rtf: Some(b"{\\rtf1 x}".to_vec()) }
        );
        assert_eq!(
            compose(&[text(1, "a")], PasteMode::Format(Format::Html), "", false),
            Err(ComposeError::FormatUnavailable(Format::Html))
        );
        assert_eq!(compose(&[image(2)], PasteMode::Format(Format::Text), "", false), Err(ComposeError::FormatUnavailable(Format::Text)));
        assert_eq!(compose(&[text(1, "a")], PasteMode::Format(Format::Png), "", false), Err(ComposeError::FormatUnavailable(Format::Png)));
        // A text record with a PNG alternative (Excel range) pastes as the picture on request.
        let excel = PastePayload { png_path: Some("C:\\m\\9.png".into()), ..rich(9, "A1") };
        assert_eq!(content(compose(&[excel], PasteMode::Format(Format::Png), "", false)), Content::Image { png_path: "C:\\m\\9.png".into() });
    }

    #[test]
    fn multi_text_joins_with_separator_and_skips_images() {
        let all = [text(1, "one"), image(2), files(3, &["C:\\f.txt"]), rich(4, "four")];
        let c = compose(&all, PasteMode::Auto, "\r\n", false).unwrap();
        assert_eq!(c.content, Content::Text { text: "one\r\nC:\\f.txt\r\nfour".into(), html: None, rtf: None });
        assert_eq!(c.used, vec![1, 3, 4]);
        assert!(c.partial, "image left out");
        let c = compose(&[text(1, "a"), text(2, "b")], PasteMode::Plain, " ", false).unwrap();
        assert_eq!(c.content, Content::Text { text: "a b".into(), html: None, rtf: None });
        assert!(!c.partial);
    }

    #[test]
    fn multi_files_union_into_one_hdrop() {
        let c = compose(&[files(1, &["C:\\a", "C:\\b"]), files(2, &["c:\\B", "C:\\c"]), image(3)], PasteMode::Auto, "\r\n", false).unwrap();
        assert_eq!(c.content, Content::Files { paths: vec!["C:\\a".into(), "C:\\b".into(), "C:\\c".into()] });
        assert_eq!(c.used, vec![1, 2]);
        assert!(c.partial);
        let c = compose(&[files(1, &["C:\\a"]), text(2, "t")], PasteMode::Format(Format::Files), "", false).unwrap();
        assert_eq!(c.content, Content::Files { paths: vec!["C:\\a".into()] });
        assert!(c.partial);
    }

    #[test]
    fn multi_only_images_pastes_the_first() {
        let c = compose(&[image(5), image(6)], PasteMode::Auto, "", false).unwrap();
        assert_eq!(c.content, Content::Image { png_path: "C:\\data\\media\\5.png".into() });
        assert_eq!(c.used, vec![5]);
        assert!(c.partial);
        assert_eq!(compose(&[image(5), image(6)], PasteMode::Format(Format::Text), "", false), Err(ComposeError::FormatUnavailable(Format::Text)));
    }

    #[test]
    fn multi_rejects_single_record_formats() {
        let two = [rich(1, "a"), rich(2, "b")];
        assert_eq!(compose(&two, PasteMode::Format(Format::Html), "", false), Err(ComposeError::MultiFormat(Format::Html)));
        assert_eq!(compose(&[], PasteMode::Auto, "", false), Err(ComposeError::Nothing));
    }

    #[test]
    fn separators() {
        assert_eq!(normalize_separator("\n"), "\r\n");
        assert_eq!(normalize_separator("\r\n"), "\r\n");
        assert_eq!(normalize_separator("a\nb\n\n"), "a\r\nb\r\n\r\n");
        assert_eq!(normalize_separator("\t"), "\t");
        assert_eq!(merge_separator(MergeSeparator::Newline), "\r\n");
        assert_eq!(merge_separator(MergeSeparator::None), "");
    }

    #[test]
    fn file_paths_fall_back_to_text_lines() {
        let p = PastePayload { files: None, text: Some("C:\\a\n\n C:\\b \n".into()), ..files(1, &[]) };
        assert_eq!(file_paths(&p), vec!["C:\\a".to_string(), "C:\\b".to_string()]);
    }
}
