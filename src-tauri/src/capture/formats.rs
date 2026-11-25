//! Pure parsers for raw clipboard payloads (no Win32): CF_UNICODETEXT with a read limit,
//! CF_HTML ("HTML Format") header + SourceURL, privacy-marker DWORDs, PNG / DIB header
//! dimensions (checked BEFORE a large image is copied), and the "explicit image source"
//! test used to pick text vs. image as the primary record (ARCHITECTURE §5).
//! Clipboard data is untrusted (GetClipboardData remarks): every offset is bounds-checked.

/// Read at most this many UTF-16 units of CF_UNICODETEXT: every unit yields ≥ 1 UTF-8 byte,
/// so this always fills `content::TEXT_MAX_BYTES` (2 MB) before `truncate_text` cuts.
pub const TEXT_READ_UNITS: usize = crate::content::TEXT_MAX_BYTES + 1;

/// HTML / RTF payloads above this are dropped (not truncated — that would break markup);
/// `clip_formats.data` is ≤ 2 MB (ARCHITECTURE §4.2).
pub const RICH_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Decode CF_UNICODETEXT units (already capped at [`TEXT_READ_UNITS`]). The string ends at
/// the first NUL (`GlobalSize` may include padding). `more_beyond` = the clipboard object
/// had more units than were copied. Returns the text and whether it was cut.
pub fn decode_utf16_text(units: &[u16], more_beyond: bool) -> (String, bool) {
    match units.iter().position(|&u| u == 0) {
        Some(end) => (String::from_utf16_lossy(&units[..end]), false),
        None => {
            let mut end = units.len();
            // Do not split a surrogate pair at the read limit.
            if more_beyond && end > 0 && (0xD800..0xDC00).contains(&units[end - 1]) {
                end -= 1;
            }
            (String::from_utf16_lossy(&units[..end]), more_beyond)
        }
    }
}

/// A privacy-marker DWORD (`CanIncludeInClipboardHistory`, `CanUploadToCloudClipboard`):
/// little-endian, at least 4 bytes. `None` = malformed (treated as "exclude" by the caller).
pub fn marker_dword(bytes: &[u8]) -> Option<u32> {
    let b: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(b))
}

/// Parsed CF_HTML payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfHtml {
    /// `StartHTML..EndHTML` (the context, fragment comments kept), or — when the context is
    /// absent (`-1`) — the fragment wrapped in a minimal document with the comments.
    pub html: String,
    /// `StartFragment..EndFragment` only (for the image-only test).
    pub fragment: String,
    pub source_url: Option<String>,
}

/// Parse a CF_HTML payload (learn.microsoft.com "HTML Clipboard Format"): an ASCII header of
/// `Key:Value` lines (CRLF, LF or CR), byte offsets from the start of the payload, UTF-8
/// body. Offsets may be zero-padded; `StartHTML`/`EndHTML` may be `-1` (no context).
pub fn parse_cf_html(raw: &[u8]) -> Option<CfHtml> {
    let raw = trim_trailing_nuls(raw);
    let mut start_html: Option<i64> = None;
    let mut end_html: Option<i64> = None;
    let mut start_frag: Option<i64> = None;
    let mut end_frag: Option<i64> = None;
    let mut source_url = None;
    let mut pos = 0usize;
    let mut saw_version = false;
    // Header lines until the first one that is not `Key:Value` with a known-looking key.
    while pos < raw.len() {
        let line_end = raw[pos..].iter().position(|&b| b == b'\r' || b == b'\n').map(|i| pos + i);
        let end = line_end.unwrap_or(raw.len());
        let Ok(line) = std::str::from_utf8(&raw[pos..end]) else { break };
        if line.starts_with('<') {
            break;
        }
        let Some((key, value)) = line.split_once(':') else { break };
        let value = value.trim();
        match key.trim() {
            "Version" => saw_version = true,
            "StartHTML" => start_html = value.parse().ok(),
            "EndHTML" => end_html = value.parse().ok(),
            "StartFragment" => start_frag = value.parse().ok(),
            "EndFragment" => end_frag = value.parse().ok(),
            "SourceURL" => {
                if !value.is_empty() && !value.contains(char::is_whitespace) {
                    source_url = Some(value.to_string());
                }
            }
            "StartSelection" | "EndSelection" => {}
            _ => break,
        }
        // Advance over CRLF / LF / CR.
        pos = end;
        if pos < raw.len() && raw[pos] == b'\r' {
            pos += 1;
        }
        if pos < raw.len() && raw[pos] == b'\n' {
            pos += 1;
        }
        if line_end.is_none() {
            break;
        }
    }
    if !saw_version {
        return None;
    }
    let slice = |s: Option<i64>, e: Option<i64>| -> Option<&[u8]> {
        let (s, e) = (s?, e?);
        if s < 0 || e < 0 {
            return None;
        }
        let (s, e) = (s as usize, (e as usize).min(raw.len()));
        (s < e).then(|| &raw[s..e])
    };
    let fragment = slice(start_frag, end_frag).map(|b| String::from_utf8_lossy(b).into_owned());
    let html = match slice(start_html, end_html) {
        Some(ctx) => String::from_utf8_lossy(ctx).into_owned(),
        None => {
            let f = fragment.as_deref()?;
            format!("<html><body><!--StartFragment-->{f}<!--EndFragment--></body></html>")
        }
    };
    Some(CfHtml { fragment: fragment.unwrap_or_default(), html, source_url })
}

fn trim_trailing_nuls(b: &[u8]) -> &[u8] {
    let end = b.iter().rposition(|&x| x != 0).map_or(0, |i| i + 1);
    &b[..end]
}

/// Raw RTF bytes without trailing NUL padding.
pub fn clean_rtf(raw: &[u8]) -> &[u8] {
    trim_trailing_nuls(raw)
}

/// The HTML fragment is just an image (browser "Copy image"): it has an `<img` element and
/// no visible text once tags, comments and non-breaking spaces are removed.
pub fn html_is_image_only(fragment: &str) -> bool {
    let lower = fragment.to_ascii_lowercase();
    if !lower.contains("<img") {
        return false;
    }
    visible_text(&lower).trim().is_empty()
}

/// Text outside tags/comments, with `&nbsp;`-style entities dropped (enough for the
/// image-only test; not a general HTML-to-text converter).
fn visible_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |i| &r[i + 3..]);
        } else if let Some(i) = rest.find('<') {
            out.push_str(&rest[..i]);
            let r = &rest[i..];
            if r.starts_with("<!--") {
                rest = r;
                continue;
            }
            rest = r.find('>').map_or("", |j| &r[j + 1..]);
        } else {
            out.push_str(rest);
            break;
        }
    }
    out.replace("&nbsp;", " ").replace("&#160;", " ").replace("&#xa0;", " ")
}

/// Width × height from a PNG signature + IHDR, without decoding.
pub fn png_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if b.len() < 24 || b[..8] != SIG || &b[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Width × |height| from a packed DIB (BITMAPCOREHEADER, BITMAPINFOHEADER, V4, V5).
pub fn dib_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    let size = u32::from_le_bytes(b.get(..4)?.try_into().ok()?);
    if size == 12 {
        let w = u16::from_le_bytes(b.get(4..6)?.try_into().ok()?) as u32;
        let h = u16::from_le_bytes(b.get(6..8)?.try_into().ok()?) as u32;
        return (w > 0 && h > 0).then_some((w, h));
    }
    if size < 40 {
        return None;
    }
    let w = i32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
    let h = i32::from_le_bytes(b.get(8..12)?.try_into().ok()?);
    if w <= 0 || h == 0 || h == i32::MIN {
        return None;
    }
    Some((w as u32, h.unsigned_abs()))
}

/// Upper bound of bytes worth copying for a DIB of `w × h` (header + palette/masks slack +
/// 32 bpp pixels); protects against a lying `GlobalSize`.
pub fn dib_copy_cap(w: u32, h: u32) -> usize {
    (w as usize).saturating_mul(h as usize).saturating_mul(4).saturating_add(64 * 1024)
}

/// Upper bound for a PNG of `w × h`: 16-bit RGBA raw size + per-row filter bytes + 1 MiB for
/// zlib/chunk overhead and metadata. A larger payload is not a sane PNG of that size.
pub fn png_copy_cap(w: u32, h: u32) -> usize {
    (w as usize)
        .saturating_mul(h as usize)
        .saturating_mul(8)
        .saturating_add((h as usize).saturating_mul(2))
        .saturating_add(1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn utf16_text_stops_at_nul_and_reports_cuts() {
        let mut u = utf16("Привет\r\nмир");
        u.push(0);
        u.extend_from_slice(&[0x41, 0x42]); // garbage after the terminator
        assert_eq!(decode_utf16_text(&u, false), ("Привет\r\nмир".to_string(), false));
        assert_eq!(decode_utf16_text(&u, true), ("Привет\r\nмир".to_string(), false));

        let u = utf16("abc");
        assert_eq!(decode_utf16_text(&u, true), ("abc".to_string(), true));
        assert_eq!(decode_utf16_text(&u, false), ("abc".to_string(), false));

        // A surrogate pair split by the read limit is dropped, not turned into U+FFFD.
        let mut u = utf16("a😀");
        u.pop();
        assert_eq!(decode_utf16_text(&u, true), ("a".to_string(), true));
        assert_eq!(decode_utf16_text(&[], false), (String::new(), false));
    }

    #[test]
    fn marker_dwords() {
        assert_eq!(marker_dword(&[0, 0, 0, 0]), Some(0));
        assert_eq!(marker_dword(&[1, 0, 0, 0, 0, 0]), Some(1));
        assert_eq!(marker_dword(&[1, 0]), None);
        assert_eq!(marker_dword(&[]), None);
    }

    fn cf_html(fragment: &str, source: Option<&str>) -> Vec<u8> {
        // Build a CF_HTML payload with correct, zero-padded offsets.
        let pre = "<html><body>\r\n<!--StartFragment-->";
        let post = "<!--EndFragment-->\r\n</body></html>";
        let src = source.map(|s| format!("SourceURL:{s}\r\n")).unwrap_or_default();
        let header_len = format!(
            "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n{src}"
        )
        .len();
        let start_html = header_len;
        let start_frag = start_html + pre.len();
        let end_frag = start_frag + fragment.len();
        let end_html = end_frag + post.len();
        format!(
            "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_frag:010}\r\nEndFragment:{end_frag:010}\r\n{src}{pre}{fragment}{post}\0\0"
        )
        .into_bytes()
    }

    #[test]
    fn cf_html_header_and_source_url() {
        let raw = cf_html("<b>Жирный</b> text", Some("https://example.com/page"));
        let p = parse_cf_html(&raw).unwrap();
        assert_eq!(p.fragment, "<b>Жирный</b> text");
        assert!(p.html.starts_with("<html><body>"));
        assert!(p.html.contains("<!--StartFragment--><b>Жирный</b> text<!--EndFragment-->"));
        assert!(p.html.ends_with("</body></html>"));
        assert_eq!(p.source_url.as_deref(), Some("https://example.com/page"));

        let p = parse_cf_html(&cf_html("<i>x</i>", None)).unwrap();
        assert_eq!(p.source_url, None);
    }

    #[test]
    fn cf_html_without_context_and_malformed() {
        let frag = "<p>hi</p>";
        // Zero-padded offsets keep the header length fixed: 84 bytes.
        let (start, end) = (84, 84 + frag.len());
        let header =
            format!("Version:1.0\nStartHTML:-1\nEndHTML:-1\nStartFragment:{start:010}\nEndFragment:{end:010}\n");
        assert_eq!(header.len(), 84);
        let raw = format!("{header}{frag}");
        let p = parse_cf_html(raw.as_bytes()).unwrap();
        assert_eq!(p.fragment, frag);
        assert_eq!(p.html, "<html><body><!--StartFragment--><p>hi</p><!--EndFragment--></body></html>");

        assert!(parse_cf_html(b"").is_none());
        assert!(parse_cf_html(b"<html>no header</html>").is_none());
        // Offsets out of range / inverted do not panic.
        let bad = b"Version:0.9\r\nStartHTML:9999\r\nEndHTML:10\r\nStartFragment:5\r\nEndFragment:1\r\n<b>x</b>";
        assert!(parse_cf_html(bad).is_none());
        // EndHTML past the end is clamped.
        let raw = cf_html("<b>x</b>", None);
        let s = String::from_utf8(raw).unwrap().trim_end_matches('\0').to_string();
        let cut = &s[..s.len() - 5];
        assert!(parse_cf_html(cut.as_bytes()).is_some());
    }

    #[test]
    fn image_only_fragments() {
        assert!(html_is_image_only(r#"<img src="https://a.io/x.png" alt="x">"#));
        assert!(html_is_image_only("<!--StartFragment--><IMG SRC=\"a.png\"/>&nbsp;<!--EndFragment-->"));
        assert!(!html_is_image_only("<table><tr><td>Item 6</td></tr></table>"));
        assert!(!html_is_image_only("<p>Caption</p><img src=a.png>"));
        assert!(!html_is_image_only("<p>no image</p>"));
        assert!(!html_is_image_only(""));
    }

    #[test]
    fn image_header_dimensions() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((640, 480)));
        assert_eq!(png_dimensions(&png[..20]), None);
        assert_eq!(png_dimensions(b"GIF89a................."), None);

        let mut dib = vec![0u8; 40];
        dib[..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&100i32.to_le_bytes());
        dib[8..12].copy_from_slice(&(-50i32).to_le_bytes()); // top-down
        assert_eq!(dib_dimensions(&dib), Some((100, 50)));
        dib[8..12].copy_from_slice(&50i32.to_le_bytes());
        assert_eq!(dib_dimensions(&dib), Some((100, 50)));
        dib[4..8].copy_from_slice(&0i32.to_le_bytes());
        assert_eq!(dib_dimensions(&dib), None);
        let mut core = vec![0u8; 12];
        core[..4].copy_from_slice(&12u32.to_le_bytes());
        core[4..6].copy_from_slice(&7u16.to_le_bytes());
        core[6..8].copy_from_slice(&9u16.to_le_bytes());
        assert_eq!(dib_dimensions(&core), Some((7, 9)));
        assert_eq!(dib_dimensions(&[1, 2]), None);
    }

    #[test]
    fn rtf_padding_is_trimmed() {
        assert_eq!(clean_rtf(b"{\\rtf1 x}\0\0\0"), b"{\\rtf1 x}");
        assert_eq!(clean_rtf(b"\0\0"), b"");
    }
}
