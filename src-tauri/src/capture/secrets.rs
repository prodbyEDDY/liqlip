//! "Looks like a secret" (SPEC §2.4 item 3, `settings.privacy.secretPatterns`) — owner:
//! **W1-B**. Pure logic, unit-tested.
//!
//! A text is a secret only when the WHOLE text (after trimming) is one such value, so prose
//! or code that merely mentions a key is still captured:
//! - a payment card number: 13–19 digits, optionally grouped by single spaces or hyphens,
//!   passing the Luhn check, first digit 2–6 (the issuer ranges of Mir, Visa, Mastercard,
//!   Amex, JCB, Diners, Discover, UnionPay, Maestro — this keeps Luhn-valid ids such as
//!   IMEIs starting with 0/8/9 out);
//! - an API key with one of the SPEC prefixes (`sk-`, `ghp_`, `gho_`, `github_pat_`,
//!   `xoxb-`/`xoxa-`/`xoxp-`, `AKIA…`, `AIza…`), one token, with the issuer's length and
//!   alphabet;
//! - a PEM private-key block `-----BEGIN … PRIVATE KEY----- … -----END … PRIVATE KEY-----`
//!   (incl. `PGP PRIVATE KEY BLOCK`).

/// Above this size a text cannot be one of the values above (a 4096-bit RSA PEM is ~3.3 KB).
const MAX_SECRET_BYTES: usize = 16 * 1024;

/// True when the whole (trimmed) text is a card number, an API key or a private-key block.
pub fn looks_like_secret(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.len() > MAX_SECRET_BYTES {
        return false;
    }
    is_card_number(t) || is_api_key(t) || is_private_key_block(t)
}

fn is_card_number(t: &str) -> bool {
    if t.len() > 19 * 2 {
        return false;
    }
    let bytes = t.as_bytes();
    if !bytes[0].is_ascii_digit() || !bytes[bytes.len() - 1].is_ascii_digit() {
        return false;
    }
    let mut digits = Vec::with_capacity(19);
    let mut prev_sep = false;
    for &b in bytes {
        match b {
            b'0'..=b'9' => {
                digits.push(b - b'0');
                prev_sep = false;
            }
            b' ' | b'-' if !prev_sep => prev_sep = true,
            _ => return false,
        }
    }
    (13..=19).contains(&digits.len()) && (2..=6).contains(&digits[0]) && luhn(&digits)
}

/// ISO/IEC 7812-1 Luhn check.
fn luhn(digits: &[u8]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            let d = d as u32;
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    sum % 10 == 0
}

struct KeyRule {
    prefix: &'static str,
    min: usize,
    max: usize,
    allowed: fn(u8) -> bool,
}

fn alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}
fn alnum_dash_underscore(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}
fn alnum_underscore(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
fn alnum_dash(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}
fn upper_digit(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}

/// Rest-of-token length ranges follow the issuers' formats (OpenAI/Anthropic `sk-…` keys are
/// long and use `-`/`_`; GitHub classic tokens are 36 chars after the prefix; fine-grained
/// `github_pat_` 82; Slack tokens are dash-separated groups; AWS access key ids are
/// `AKIA` + 16 upper-case alphanumerics; Google API keys `AIza` + 35).
const KEY_RULES: [KeyRule; 9] = [
    KeyRule { prefix: "sk-", min: 20, max: 256, allowed: alnum_dash_underscore },
    KeyRule { prefix: "ghp_", min: 30, max: 255, allowed: alnum },
    KeyRule { prefix: "gho_", min: 30, max: 255, allowed: alnum },
    KeyRule { prefix: "github_pat_", min: 40, max: 255, allowed: alnum_underscore },
    KeyRule { prefix: "xoxb-", min: 10, max: 255, allowed: alnum_dash },
    KeyRule { prefix: "xoxa-", min: 10, max: 255, allowed: alnum_dash },
    KeyRule { prefix: "xoxp-", min: 10, max: 255, allowed: alnum_dash },
    KeyRule { prefix: "AKIA", min: 16, max: 16, allowed: upper_digit },
    KeyRule { prefix: "AIza", min: 35, max: 35, allowed: alnum_dash_underscore },
];

/// Issued keys are random base62-ish strings: they contain both letters and digits, which
/// keeps hyphenated words (`sk-learn-is-a-python-library`) out.
fn is_api_key(t: &str) -> bool {
    KEY_RULES.iter().any(|r| {
        t.strip_prefix(r.prefix).is_some_and(|rest| {
            (r.min..=r.max).contains(&rest.len())
                && rest.bytes().all(r.allowed)
                && rest.bytes().any(|b| b.is_ascii_digit())
                && rest.bytes().any(|b| b.is_ascii_alphabetic())
        })
    })
}

fn pem_label<'a>(line: &'a str, kind: &str) -> Option<&'a str> {
    line.trim()
        .strip_prefix("-----")?
        .strip_prefix(kind)?
        .strip_prefix(' ')?
        .strip_suffix("-----")
}

fn is_private_key_block(t: &str) -> bool {
    if !t.starts_with("-----BEGIN ") {
        return false;
    }
    let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.len() < 3 {
        return false;
    }
    let (Some(begin), Some(end)) = (pem_label(lines[0], "BEGIN"), pem_label(lines[lines.len() - 1], "END"))
    else {
        return false;
    };
    let private = begin.ends_with("PRIVATE KEY") || begin == "PGP PRIVATE KEY BLOCK";
    private && begin == end && !lines[1..lines.len() - 1].iter().any(|l| l.starts_with("-----"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_numbers_with_luhn() {
        for t in [
            "4111111111111111",
            "4111 1111 1111 1111",
            "4111-1111-1111-1111",
            "  5555 5555 5555 4444  ",
            "378282246310005",      // Amex, 15
            "2200 0000 0000 0004", // Mir-like range, Luhn-valid
            "6011111111111117",
        ] {
            assert!(looks_like_secret(t), "{t} must be a secret");
        }
        for t in [
            "4111111111111112",        // Luhn fails
            "4111  1111 1111 1111",    // double separator
            "4111 1111 1111 1111 9999 9", // too many digits
            "411111111111",            // 12 digits
            "Card: 4111 1111 1111 1111", // mentioned inside text
            "0000000000000",          // Luhn-valid, but not an issuer range
            "8612345678901238",
            "4111.1111.1111.1111",
        ] {
            assert!(!looks_like_secret(t), "{t} must NOT be a secret");
        }
    }

    #[test]
    fn api_keys_by_prefix() {
        for t in [
            "sk-proj-AbCdEf0123456789_xyz-QWERTYUIOP",
            "sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789",
            "ghp_1234567890abcdefghijABCDEFGHIJ123456",
            "gho_1234567890abcdefghijABCDEFGHIJ123456",
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
            "xoxb-123456789012-1234567890123-AbCdEfGhIjKlMnOpQrStUvWx",
            "xoxp-1234567890-abcdef",
            "AKIAIOSFODNN7EXAMPLE",
            "AIzaSyA-1234567890abcdefghijklmnopqrstu",
            "\n  AKIAIOSFODNN7EXAMPLE \n",
        ] {
            assert!(looks_like_secret(t), "{t} must be a secret");
        }
        for t in [
            "sk-short",
            "my key is sk-proj-AbCdEf0123456789_xyz-QWERTYUIOP",
            "sk-proj-AbCdEf0123456789 xyz-QWERTYUIOP",
            "AKIAiosfodnn7example",
            "AKIAIOSFODNN7EXAMPLE1",
            "AIzaShort",
            "ghp_",
            "xoxz-1234567890-abcdef",
            "skip-this-line-please-it-is-just-text",
            "sk-learn-is-a-python-library-for-machine-learning",
        ] {
            assert!(!looks_like_secret(t), "{t} must NOT be a secret");
        }
    }

    #[test]
    fn pem_private_key_blocks() {
        let rsa = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA1\nabcDEF==\n-----END RSA PRIVATE KEY-----";
        let pkcs8 = "  -----BEGIN PRIVATE KEY-----\r\nMIIEvQIBADANBgkq\r\n-----END PRIVATE KEY-----\r\n";
        let ssh = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----";
        let pgp = "-----BEGIN PGP PRIVATE KEY BLOCK-----\n\nlQOYBF\n-----END PGP PRIVATE KEY BLOCK-----";
        for t in [rsa, pkcs8, ssh, pgp] {
            assert!(looks_like_secret(t), "{t}");
        }
        let public = "-----BEGIN PUBLIC KEY-----\nMIIBIjANBg\n-----END PUBLIC KEY-----";
        let cert = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----";
        let mismatched = "-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----";
        let mentioned = format!("Here is my key:\n{rsa}");
        let two = format!("{rsa}\n{rsa}");
        for t in [public, cert, mismatched, &mentioned, &two, "-----BEGIN PRIVATE KEY-----"] {
            assert!(!looks_like_secret(t), "{t}");
        }
    }

    #[test]
    fn ordinary_text_is_not_secret() {
        for t in ["", "   ", "Hello", "123456", "https://example.com", "+7 999 123-45-67"] {
            assert!(!looks_like_secret(t), "{t}");
        }
        assert!(!looks_like_secret(&"a".repeat(MAX_SECRET_BYTES + 1)));
    }
}
