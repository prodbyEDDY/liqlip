//! Text classification (SPEC §2.1) — owner: **W1-B**. Port of Liquid Tools
//! `clipboard/capture.rs::classify_text` with the SPEC fixes:
//! - colour only with `#` (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`) or an explicit
//!   `rgb()/rgba()/hsl()/hsla()` — six bare digits are never a colour (one-time codes);
//! - link = the whole text is one URL line (`http/https/ftp/file://` or `www.`);
//! - contact = the whole text is one email or phone number (dates, IPv4 addresses and
//!   thousands-grouped numbers are not phones);
//! - code = at least two independent heuristics agree.
//!
//! Pure logic, unit-tested. Shared: the store (W1-A) calls [`classify_text`] for
//! `clip_create_text` / `clip_update_text`.

use crate::model::{ClipColor, ClipMeta, ContactKind, Kind};

/// Classify a whole text: `color` (`meta.color`), `link` (`meta.url/host`), `contact`
/// (`meta.contact`), `code` (≥ 2 heuristics) or `text`. Never fails. Leading/trailing
/// whitespace is ignored for the whole-string kinds.
pub fn classify_text(text: &str) -> (Kind, ClipMeta) {
    let t = text.trim();
    if t.is_empty() {
        return (Kind::Text, ClipMeta::default());
    }
    if let Some(color) = parse_color(t) {
        return (Kind::Color, ClipMeta { color: Some(color), ..Default::default() });
    }
    if let Some((url, host)) = parse_link(t) {
        return (Kind::Link, ClipMeta { url: Some(url), host, ..Default::default() });
    }
    if let Some(contact) = contact_kind(t) {
        return (Kind::Contact, ClipMeta { contact: Some(contact), ..Default::default() });
    }
    if looks_like_code(text) {
        return (Kind::Code, ClipMeta::default());
    }
    (Kind::Text, ClipMeta::default())
}

// ── Colour ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    /// 0..=255
    a: u8,
}

/// Whole string is one CSS colour literal → its HEX / RGB / HSL spellings.
/// HEX is upper-case `#RRGGBB` (+`AA` when not opaque); `rgb()`/`hsl()` use the legacy
/// comma syntax (`rgba()`/`hsla()` when not opaque), integers, like the mock seed.
fn parse_color(t: &str) -> Option<ClipColor> {
    if t.len() > 64 {
        return None;
    }
    let c = match t.strip_prefix('#') {
        Some(hex) => parse_hex(hex)?,
        None => parse_color_fn(t)?,
    };
    Some(format_color(c))
}

fn parse_hex(h: &str) -> Option<Rgba> {
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok();
    let pair = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        3 | 4 => {
            let a = if h.len() == 4 { digit(3)? * 17 } else { 255 };
            Some(Rgba { r: digit(0)? * 17, g: digit(1)? * 17, b: digit(2)? * 17, a })
        }
        6 | 8 => {
            let a = if h.len() == 8 { pair(6)? } else { 255 };
            Some(Rgba { r: pair(0)?, g: pair(2)?, b: pair(4)?, a })
        }
        _ => None,
    }
}

/// `rgb()/rgba()/hsl()/hsla()` in the legacy (`rgb(1, 2, 3)`, `rgba(1, 2, 3, .5)`) or the
/// modern (`rgb(1 2 3 / 50%)`) syntax. Out-of-range components reject the string.
fn parse_color_fn(t: &str) -> Option<Rgba> {
    let lower = t.to_ascii_lowercase();
    let (is_hsl, rest) = ["rgba(", "rgb(", "hsla(", "hsl("]
        .iter()
        .find_map(|p| lower.strip_prefix(p).map(|r| (p.starts_with('h'), r)))?;
    let inner = rest.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return None;
    }
    let parts: Vec<&str> = if inner.contains(',') {
        if inner.contains('/') {
            return None;
        }
        inner.split(',').map(str::trim).collect()
    } else {
        let (main, alpha) = match inner.split_once('/') {
            Some((m, a)) => (m, Some(a.trim())),
            None => (inner, None),
        };
        let mut v: Vec<&str> = main.split_whitespace().collect();
        if let Some(a) = alpha {
            if a.is_empty() || a.contains('/') {
                return None;
            }
            v.push(a);
        }
        v
    };
    if !(parts.len() == 3 || parts.len() == 4) || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let a = match parts.get(3) {
        Some(p) => alpha_u8(p)?,
        None => 255,
    };
    if is_hsl {
        let h = hue_deg(parts[0])?;
        let s = percent_or_number(parts[1])?;
        let l = percent_or_number(parts[2])?;
        let (r, g, b) = hsl_to_rgb(h, s / 100.0, l / 100.0);
        Some(Rgba { r, g, b, a })
    } else {
        Some(Rgba { r: rgb_channel(parts[0])?, g: rgb_channel(parts[1])?, b: rgb_channel(parts[2])?, a })
    }
}

/// A plain decimal number (`12`, `-4`, `.5`, `0.25`): no exponent, no `inf`/`nan`.
fn number(s: &str) -> Option<f64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-')) {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn rgb_channel(s: &str) -> Option<u8> {
    let v = match s.strip_suffix('%') {
        Some(p) => {
            let p = number(p)?;
            if !(0.0..=100.0).contains(&p) {
                return None;
            }
            p * 2.55
        }
        None => number(s)?,
    };
    if !(0.0..=255.0).contains(&v) {
        return None;
    }
    Some(v.round() as u8)
}

fn percent_or_number(s: &str) -> Option<f64> {
    let v = number(s.strip_suffix('%').unwrap_or(s))?;
    (0.0..=100.0).contains(&v).then_some(v)
}

fn alpha_u8(s: &str) -> Option<u8> {
    let v = match s.strip_suffix('%') {
        Some(p) => number(p)? / 100.0,
        None => number(s)?,
    };
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    Some((v * 255.0).round() as u8)
}

fn hue_deg(s: &str) -> Option<f64> {
    let v = if let Some(n) = s.strip_suffix("deg") {
        number(n)?
    } else if let Some(n) = s.strip_suffix("turn") {
        number(n)? * 360.0
    } else {
        number(s)?
    };
    Some(v.rem_euclid(360.0))
}

/// CSS Color 4 §7.1 `hslToRgb` (h in degrees, s/l in 0..=1).
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        let v = l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (f(0.0), f(8.0), f(4.0))
}

/// (h°, s%, l%) rounded to integers.
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (i64, i64, i64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    let (h, s) = if d == 0.0 {
        (0.0, 0.0)
    } else {
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        (h, s)
    };
    let h = (h.round() as i64).rem_euclid(360);
    (h, (s * 100.0).round() as i64, (l * 100.0).round() as i64)
}

fn alpha_str(a: u8) -> String {
    let v = (a as f64 / 255.0 * 100.0).round() / 100.0;
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn format_color(c: Rgba) -> ClipColor {
    let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
    if c.a == 255 {
        ClipColor {
            hex: format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b),
            rgb: format!("rgb({}, {}, {})", c.r, c.g, c.b),
            hsl: format!("hsl({h}, {s}%, {l}%)"),
        }
    } else {
        let a = alpha_str(c.a);
        ClipColor {
            hex: format!("#{:02X}{:02X}{:02X}{:02X}", c.r, c.g, c.b, c.a),
            rgb: format!("rgba({}, {}, {}, {a})", c.r, c.g, c.b),
            hsl: format!("hsla({h}, {s}%, {l}%, {a})"),
        }
    }
}

// ── Link ────────────────────────────────────────────────────────────────────

/// Longest text still considered "one URL" (long tracking links exist; data: URLs are
/// not links here anyway).
const URL_MAX_BYTES: usize = 16 * 1024;

/// Whole string is one URL → (`meta.url`, `meta.host`). `www.` links get `https://`.
fn parse_link(t: &str) -> Option<(String, Option<String>)> {
    if t.len() > URL_MAX_BYTES || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    for scheme in ["http://", "https://", "ftp://", "file://"] {
        if lower.starts_with(scheme) {
            let rest = &t[scheme.len()..];
            let host = authority_host(rest);
            if scheme == "file://" {
                return (!rest.is_empty()).then(|| (t.to_string(), host));
            }
            let host = host.filter(|h| valid_host(h))?;
            return Some((t.to_string(), Some(host)));
        }
    }
    if lower.starts_with("www.") {
        let host = authority_host(t)?;
        let labels: Vec<&str> = host.split(':').next()?.split('.').collect();
        let tld = labels.last()?;
        if labels.len() >= 3
            && labels.iter().all(|l| !l.is_empty())
            && tld.chars().count() >= 2
            && tld.chars().all(char::is_alphabetic)
            && valid_host(&host)
        {
            return Some((format!("https://{t}"), Some(host)));
        }
    }
    None
}

/// Host of an absolute URL (`scheme://user@host:port/…` → `host:port`, lower-cased), e.g.
/// the CF_HTML `SourceURL` of a rich copy. `None` for URLs without an authority.
pub(crate) fn url_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    authority_host(rest).filter(|h| valid_host(h))
}

/// `user:pw@Host:port/path?q#f` → `host:port` lower-cased (like JS `URL.host`).
fn authority_host(rest: &str) -> Option<String> {
    let end = rest.find(['/', '?', '#', '\\']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    (!host.is_empty()).then(|| host.to_lowercase())
}

fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && !h.starts_with('.')
        && h.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']' | '_'))
}

// ── Contact ─────────────────────────────────────────────────────────────────

/// A single email address or phone number copied on its own. `mailto:` / `tel:` are
/// tolerated.
fn contact_kind(t: &str) -> Option<ContactKind> {
    let t = strip_contact_scheme(t);
    if is_email(t) {
        Some(ContactKind::Email)
    } else if is_phone(t) {
        Some(ContactKind::Phone)
    } else {
        None
    }
}

fn strip_contact_scheme(t: &str) -> &str {
    let lower = t.get(..7).map(str::to_ascii_lowercase).unwrap_or_default();
    if lower.starts_with("mailto:") {
        return &t[7..];
    }
    if lower.starts_with("tel:") {
        return &t[4..];
    }
    t
}

/// `local@domain.tld`, no whitespace, sane characters, alphabetic TLD ≥ 2.
fn is_email(t: &str) -> bool {
    if t.is_empty() || t.len() > 254 || t.contains(char::is_whitespace) {
        return false;
    }
    let Some((local, domain)) = t.split_once('@') else { return false };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return false;
    }
    if !local.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')) {
        return false;
    }
    if !domain.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-')) {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|l| l.is_empty()) {
        return false;
    }
    let tld = labels[labels.len() - 1];
    tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic)
}

/// One phone number: only `+`, digits and the separators ` - ( ) .`; 7–15 digits;
/// a leading `+` or a separator (bare digit runs are ids/codes). Not a date
/// (`2026-11-26`, `26.11.2026`), not an IPv4 address, not a thousands-grouped number
/// (`1 000 000`).
fn is_phone(t: &str) -> bool {
    if t.is_empty() || t.len() > 32 {
        return false;
    }
    let mut digits = 0usize;
    for (i, c) in t.char_indices() {
        match c {
            '0'..='9' => digits += 1,
            '+' if i == 0 => {}
            ' ' | '-' | '(' | ')' | '.' => {}
            _ => return false,
        }
    }
    if !(7..=15).contains(&digits) {
        return false;
    }
    let plus = t.starts_with('+');
    let has_sep = t.contains([' ', '-', '(', ')', '.']);
    if !(plus || has_sep) {
        return false;
    }
    if plus {
        return true;
    }
    !(is_date_like(t) || is_ipv4(t) || is_thousands_grouped(t))
}

/// Groups of digits split on one separator kind.
fn digit_groups(t: &str, sep: char) -> Option<Vec<&str>> {
    let groups: Vec<&str> = t.split(sep).collect();
    groups.iter().all(|g| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit())).then_some(groups)
}

fn is_date_like(t: &str) -> bool {
    [ '-', '.' ].iter().any(|&sep| {
        digit_groups(t, sep).is_some_and(|g| {
            let lens: Vec<usize> = g.iter().map(|s| s.len()).collect();
            lens == [4, 2, 2] || lens == [2, 2, 4] || lens == [1, 2, 4] || lens == [2, 1, 4] || lens == [1, 1, 4]
        })
    })
}

fn is_ipv4(t: &str) -> bool {
    digit_groups(t, '.').is_some_and(|g| g.len() == 4 && g.iter().all(|s| s.len() <= 3))
}

fn is_thousands_grouped(t: &str) -> bool {
    [' ', '.'].iter().any(|&sep| {
        digit_groups(t, sep).is_some_and(|g| {
            g.len() >= 2 && (1..=3).contains(&g[0].len()) && g[1..].iter().all(|s| s.len() == 3)
        })
    })
}

// ── Code ────────────────────────────────────────────────────────────────────

/// Does the text read like source code? Conservative (prose is never misfiled as code):
/// multi-line text needs two of {indentation on ≥ 2 lines, code tokens on ≥ 2 lines,
/// punctuation density ≥ 5 %}; a single line needs a statement terminator *and* code
/// punctuation.
fn looks_like_code(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        let t = text.trim();
        return (t.ends_with(';') || t.ends_with('{') || t.ends_with('}'))
            && t.contains(['(', '=', '{']);
    }
    const TOKENS: [&str; 22] = [
        "fn ", "def ", "function ", "const ", "let ", "var ", "import ", "class ", "public ",
        "private ", "return ", "if (", "for (", "while (", "=>", "();", "{", "};", "#include", "</",
        "/>", "::",
    ];
    let indented = lines.iter().filter(|l| l.starts_with("  ") || l.starts_with('\t')).count();
    let token_hits = lines.iter().filter(|l| TOKENS.iter().any(|t| l.contains(t))).count();
    let total = text.chars().count().max(1);
    let code_chars = text
        .chars()
        .filter(|c| matches!(c, '{' | '}' | ';' | '(' | ')' | '<' | '>' | '=' | '[' | ']'))
        .count();
    let dense = code_chars * 100 / total >= 5;
    let signals = (indented >= 2) as u8 + (token_hits >= 2) as u8 + dense as u8;
    signals >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(t: &str) -> Kind {
        classify_text(t).0
    }

    #[test]
    fn colors_need_hash_or_explicit_function() {
        for c in [
            "#fff",
            "#FF5733",
            "#ff5733cc",
            "#abcd",
            "  #FF5733  ",
            "rgb(255, 87, 51)",
            "rgba(255, 87, 51, 0.4)",
            "RGB(255,87,51)",
            "rgb(255 87 51 / 40%)",
            "rgb(100%, 50%, 0%)",
            "hsl(14, 100%, 60%)",
            "hsla(14 100% 60% / 0.4)",
            "hsl(14deg 100% 60%)",
        ] {
            assert_eq!(kind(c), Kind::Color, "{c} must be a color");
        }
        for not in [
            "123456", // one-time code, no '#'
            "ff5733",
            "#12345",
            "#xyz",
            "#fff is my favourite",
            "rgb(call_me)",
            "rgb(300, 0, 0)",
            "rgb(1, 2)",
            "hsl(",
            "rgb(1, 2, 3 / 4)",
            "rgba(1, 2, 3, 2)",
            "rgb(inf, 0, 0)",
        ] {
            assert_ne!(kind(not), Kind::Color, "{not} must NOT be a color");
        }
    }

    #[test]
    fn color_meta_spellings() {
        let (_, m) = classify_text("#FF5733");
        let c = m.color.unwrap();
        assert_eq!(c.hex, "#FF5733");
        assert_eq!(c.rgb, "rgb(255, 87, 51)");
        assert_eq!(c.hsl, "hsl(11, 100%, 60%)");

        let (_, m) = classify_text("#fff");
        assert_eq!(m.color.unwrap().hex, "#FFFFFF");

        let (_, m) = classify_text("rgb(34, 197, 94)");
        let c = m.color.unwrap();
        assert_eq!(c.hex, "#22C55E");
        assert_eq!(c.hsl, "hsl(142, 71%, 45%)");

        // CSS Color 4 conversion of the exact integers (b = 0.0788 × 255 = 20.1).
        let (_, m) = classify_text("hsl(48, 96%, 53%)");
        let c = m.color.unwrap();
        assert_eq!(c.rgb, "rgb(250, 204, 20)");
        assert_eq!(c.hex, "#FACC14");
        assert_eq!(c.hsl, "hsl(48, 96%, 53%)");

        let (_, m) = classify_text("#8B5CF6CC");
        let c = m.color.unwrap();
        assert_eq!(c.hex, "#8B5CF6CC");
        assert_eq!(c.rgb, "rgba(139, 92, 246, 0.8)");
        assert_eq!(c.hsl, "hsla(258, 90%, 66%, 0.8)");
    }

    #[test]
    fn links_are_one_url_line() {
        for (t, host) in [
            ("https://example.com/a?b=1#c", "example.com"),
            ("HTTP://Example.COM:8080/x", "example.com:8080"),
            ("ftp://files.example.org/pub", "files.example.org"),
            ("https://user:pw@host.ru/", "host.ru"),
            ("https://пример.рф/путь", "пример.рф"),
            ("www.example.com/path", "www.example.com"),
        ] {
            let (k, m) = classify_text(t);
            assert_eq!(k, Kind::Link, "{t}");
            assert_eq!(m.host.as_deref(), Some(host), "{t}");
        }
        assert_eq!(classify_text("www.github.com").1.url.as_deref(), Some("https://www.github.com"));
        assert_eq!(classify_text(" https://a.io ").1.url.as_deref(), Some("https://a.io"));
        assert_eq!(kind("file:///C:/Users/me/a.txt"), Kind::Link);
        for not in [
            "see https://example.com",
            "https://example.com\nhttps://b.com",
            "https://",
            "www.x",
            "www.",
            "example.com",
            "mailto:a@b.co",
        ] {
            assert_ne!(kind(not), Kind::Link, "{not}");
        }
    }

    #[test]
    fn host_of_source_urls() {
        assert_eq!(url_host("https://Docs.Example.com:443/a/b?c").as_deref(), Some("docs.example.com:443"));
        assert_eq!(url_host("about:blank"), None);
        assert_eq!(url_host("file:///C:/x.html"), None);
    }

    #[test]
    fn contacts_are_whole_values() {
        for (t, c) in [
            ("john.doe+tag@example.co.uk", ContactKind::Email),
            ("mailto:me@site.ru", ContactKind::Email),
            ("+7 (999) 123-45-67", ContactKind::Phone),
            ("8 999 123 45 67", ContactKind::Phone),
            ("+79991234567", ContactKind::Phone),
            ("tel:+1-555-123-4567", ContactKind::Phone),
            ("(555) 123-4567", ContactKind::Phone),
        ] {
            let (k, m) = classify_text(t);
            assert_eq!(k, Kind::Contact, "{t}");
            assert_eq!(m.contact, Some(c), "{t}");
        }
        for not in [
            "write me at me@site.ru",
            "1234567",   // bare digits: an id
            "2026-11-26", // date
            "26.11.2026",
            "192.168.1.10", // IPv4
            "1 000 000",    // thousands
            "12.345.678",
            "a@b",
            "@site.ru",
            "+7 999 abc",
        ] {
            assert_ne!(kind(not), Kind::Contact, "{not}");
        }
    }

    #[test]
    fn code_needs_two_signals() {
        let rust = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}";
        assert_eq!(kind(rust), Kind::Code);
        let js = "const a = [1, 2];\nconst b = a.map((x) => x * 2);";
        assert_eq!(kind(js), Kind::Code);
        assert_eq!(kind("let x = foo(1);"), Kind::Code);
        let json = "{\n  \"a\": 1,\n  \"b\": [1, 2]\n}";
        assert_eq!(kind(json), Kind::Code);
        // Prose with some punctuation stays text.
        let prose = "Привет! Встречаемся завтра (в 10:00).\nНе забудь документы — и ноутбук.";
        assert_eq!(kind(prose), Kind::Text);
        assert_eq!(kind("Just a sentence."), Kind::Text);
        let list = "  - first item\n  - second item\n  - third item";
        assert_eq!(kind(list), Kind::Text);
    }

    #[test]
    fn plain_text_and_edge_cases() {
        assert_eq!(classify_text(""), (Kind::Text, ClipMeta::default()));
        assert_eq!(classify_text("   \n\t"), (Kind::Text, ClipMeta::default()));
        assert_eq!(kind("Hello, Liqlip"), Kind::Text);
        assert_eq!(kind("123456"), Kind::Text);
        // Huge input does not explode.
        let big = "word ".repeat(200_000);
        assert_eq!(kind(&big), Kind::Text);
    }
}
