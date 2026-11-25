//! Clipboard writer (ARCHITECTURE §6 "Запись в буфер") — owner **W1-C**.
//!
//! Two halves:
//! - **pure**: what goes on the clipboard ([`Content`] → [`Entry`] list) and the byte
//!   layouts (`HTML Format` header with byte offsets, DROPFILES, CF_DIB, UTF-16) — unit-tested;
//! - **Win32**: one transaction `OpenClipboard(owner) → EmptyClipboard → SetClipboardData… →
//!   CloseClipboard` with retries, every HGLOBAL freed on any failure, then the self-copy
//!   mark (`capture::mark_own_write(GetClipboardSequenceNumber())`).
//!
//! The clipboard owner is a hidden message-only window on its own thread, which pumps
//! messages: `OpenClipboard(NULL)` makes `EmptyClipboard` set a NULL owner, "this causes
//! SetClipboardData to fail" (learn.microsoft.com/windows/win32/api/winuser/nf-winuser-openclipboard),
//! and a non-pumping owner would stall the next app's `EmptyClipboard` (it sends
//! WM_DESTROYCLIPBOARD to the previous owner).
//!
//! Formats are placed most descriptive first ("Clipboard formats that contain the most
//! information should be placed on the clipboard first" —
//! learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#multiple-clipboard-formats).

use std::path::PathBuf;

/// Registered format names (RegisterClipboardFormatW).
pub const FMT_HTML: &str = "HTML Format";
pub const FMT_RTF: &str = "Rich Text Format";
pub const FMT_PNG: &str = "PNG";
/// `CFSTR_PREFERREDDROPEFFECT` (learn.microsoft.com/windows/win32/shell/clipboard).
pub const FMT_DROP_EFFECT: &str = "Preferred DropEffect";
/// `DROPEFFECT_COPY` — a pasted history item never moves the original files.
pub const DROPEFFECT_COPY: u32 = 1;

/// A clipboard format to place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fmt {
    UnicodeText,
    Dib,
    HDrop,
    Registered(&'static str),
}

/// One encoded format. `required`: the write fails if this one could not be placed (the
/// format a paste target actually needs); optional ones are best-effort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub fmt: Fmt,
    pub bytes: Vec<u8>,
    pub required: bool,
}

/// What goes on the clipboard, before encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// `CF_UNICODETEXT` + optional `HTML Format` (context HTML without the header) + RTF.
    Text { text: String, html: Option<String>, rtf: Option<Vec<u8>> },
    /// `PNG` (the stored file as-is) + `CF_DIB` (decoded once, alpha flattened over white).
    Image { png_path: PathBuf },
    /// `CF_HDROP` + `Preferred DropEffect = COPY` + `CF_UNICODETEXT` with the paths.
    Files { paths: Vec<String> },
}

/// Why a write did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// Another app kept the clipboard open through every retry.
    Busy,
    /// OS / data failure (allocation, a required format rejected, unreadable image).
    Failed(String),
}

impl WriteError {
    /// Command-error string (`busy: …` / `failed: …`, ARCHITECTURE §9.6).
    pub fn to_command_error(&self) -> String {
        match self {
            WriteError::Busy => "busy: the clipboard is held by another application".into(),
            WriteError::Failed(e) => format!("failed: {e}"),
        }
    }
}

// ── Pure encoders ───────────────────────────────────────────────────────────

/// UTF-16LE with a terminating NUL (`CF_UNICODETEXT`).
pub fn utf16z(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2 + 2);
    for u in text.encode_utf16().chain(std::iter::once(0)) {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// File paths as one text block (CRLF — the Windows text convention).
pub fn paths_text(paths: &[String]) -> String {
    paths.join("\r\n")
}

/// A little-endian DWORD (`Preferred DropEffect`).
pub fn dword(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// RTF as stored, guaranteed NUL-terminated (it is a text format).
pub fn rtf_bytes(rtf: &[u8]) -> Vec<u8> {
    let mut out = rtf.to_vec();
    if out.last() != Some(&0) {
        out.push(0);
    }
    out
}

/// `CF_HDROP`: a DROPFILES header (`pFiles` = 20, `pt` = 0,0, `fNC` = 0, `fWide` = 1) followed
/// by the UTF-16 paths, each NUL-terminated, plus a final NUL ("double null-terminated" —
/// learn.microsoft.com/windows/win32/shell/clipboard#cf_hdrop,
/// learn.microsoft.com/windows/win32/api/shlobj_core/ns-shlobj_core-dropfiles).
pub fn dropfiles(paths: &[String]) -> Vec<u8> {
    const HEADER: u32 = 20; // DWORD pFiles + POINT pt (2×LONG) + BOOL fNC + BOOL fWide
    let mut out = Vec::with_capacity(HEADER as usize + paths.iter().map(|p| p.len() * 2 + 2).sum::<usize>() + 2);
    out.extend_from_slice(&HEADER.to_le_bytes()); // pFiles
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    out.extend_from_slice(&0u32.to_le_bytes()); // fNC = FALSE
    out.extend_from_slice(&1u32.to_le_bytes()); // fWide = TRUE
    for p in paths {
        for u in p.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]); // list terminator
    out
}

/// Packed `CF_DIB`: BITMAPINFOHEADER (40 bytes, 32 bpp, BI_RGB, positive height = bottom-up)
/// + BGRA rows. CF_DIB carries no reliable alpha, so translucent pixels are composited over
/// white (classic targets would otherwise show them black); alpha-aware targets read the PNG.
pub fn dib_from_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let px = width as usize * height as usize;
    debug_assert_eq!(rgba.len(), px * 4);
    let mut out = Vec::with_capacity(40 + px * 4);
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&(height as i32).to_le_bytes()); // biHeight (bottom-up)
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&((px * 4) as u32).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&[0u8; 16]); // biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant
    let stride = width as usize * 4;
    for row in (0..height as usize).rev() {
        for p in rgba[row * stride..row * stride + stride].chunks_exact(4) {
            let a = p[3] as u32;
            let over_white = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
            out.extend_from_slice(&[over_white(p[2]), over_white(p[1]), over_white(p[0]), 255]);
        }
    }
    out
}

const START_MARKER: &str = "<!--StartFragment-->";
const END_MARKER: &str = "<!--EndFragment-->";

/// Find an HTML comment marker such as `<!--StartFragment-->`, tolerating the whitespace
/// variants seen in the wild (`<!-- StartFragment -->`), ASCII case-insensitively.
/// Returns `(start, end)` byte offsets of the whole comment.
fn find_marker(html: &str, name: &str) -> Option<(usize, usize)> {
    let bytes = html.as_bytes();
    let mut from = 0;
    while let Some(rel) = html[from..].find("<!--") {
        let start = from + rel;
        let mut i = start + 4;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.len() >= i + name.len() && bytes[i..i + name.len()].eq_ignore_ascii_case(name.as_bytes()) {
            let mut j = i + name.len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes[j..].starts_with(b"-->") {
                return Some((start, j + 3));
            }
        }
        from = start + 4;
    }
    None
}

/// ASCII case-insensitive `find`.
fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// If `html` is a full CF_HTML payload (starts with a `Version:` header), keep only the
/// context (`StartHTML..EndHTML`). The store keeps the header-less context (§15), this is a
/// guard against a payload that still has one.
fn strip_cf_header(html: &str) -> &str {
    if !html.starts_with("Version:") {
        return html;
    }
    let value = |key: &str| -> Option<usize> {
        let at = html.find(key)? + key.len();
        let digits: String = html[at..].chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    };
    match (value("StartHTML:"), value("EndHTML:")) {
        (Some(s), Some(e)) if s < e && e <= html.len() && html.is_char_boundary(s) && html.is_char_boundary(e) => &html[s..e],
        (Some(s), _) if s < html.len() && html.is_char_boundary(s) => &html[s..],
        _ => html,
    }
}

/// The context with fragment markers: kept if present, otherwise inserted inside `<body>`
/// (or the whole thing wrapped in a minimal document). Returns the context and the byte
/// range of the fragment *between* the markers.
fn with_fragment(html: &str) -> (String, usize, usize) {
    let html = strip_cf_header(html);
    if let (Some((_, s_end)), Some((e_start, _))) = (find_marker(html, "StartFragment"), find_marker(html, "EndFragment")) {
        if s_end <= e_start {
            return (html.to_string(), s_end, e_start);
        }
    }
    let body_open = find_ci(html, "<body").and_then(|i| html[i..].find('>').map(|j| i + j + 1));
    let body_close = find_ci(html, "</body");
    let doc = match (body_open, body_close) {
        (Some(o), Some(c)) if o <= c => format!("{}{START_MARKER}{}{END_MARKER}{}", &html[..o], &html[o..c], &html[c..]),
        _ => format!("<html><body>{START_MARKER}{html}{END_MARKER}</body></html>"),
    };
    let s = doc.find(START_MARKER).map(|i| i + START_MARKER.len()).unwrap_or(0);
    let e = doc.rfind(END_MARKER).unwrap_or(doc.len());
    (doc, s, e)
}

/// `HTML Format` (CF_HTML) bytes: an ASCII header with byte offsets counted from the start of
/// the clipboard data, then the UTF-8 context, then a NUL (outside `EndHTML`). Offsets are
/// zero-padded to 10 digits so the header length is fixed ("the offset values may be
/// optionally left-padded with an arbitrary amount of zero digits"); `StartFragment` points
/// just past `<!--StartFragment-->`, `EndFragment` at `<!--EndFragment-->`
/// (learn.microsoft.com/windows/win32/dataxchg/html-clipboard-format). `Version:0.9` — every
/// reader accepts it.
pub fn cf_html(html: &str) -> Vec<u8> {
    let (doc, frag_start, frag_end) = with_fragment(html);
    let header = |sh: usize, eh: usize, sf: usize, ef: usize| {
        format!("Version:0.9\r\nStartHTML:{sh:010}\r\nEndHTML:{eh:010}\r\nStartFragment:{sf:010}\r\nEndFragment:{ef:010}\r\n")
    };
    let base = header(0, 0, 0, 0).len();
    let head = header(base, base + doc.len(), base + frag_start, base + frag_end);
    debug_assert_eq!(head.len(), base);
    let mut out = Vec::with_capacity(base + doc.len() + 1);
    out.extend_from_slice(head.as_bytes());
    out.extend_from_slice(doc.as_bytes());
    out.push(0);
    out
}

/// Text / HTML / RTF / file entries (images need disk I/O — see [`encode`]).
pub fn encode_text(text: &str, html: Option<&str>, rtf: Option<&[u8]>) -> Vec<Entry> {
    let mut out = Vec::new();
    if let Some(h) = html.filter(|h| !h.trim().is_empty()) {
        out.push(Entry { fmt: Fmt::Registered(FMT_HTML), bytes: cf_html(h), required: false });
    }
    if let Some(r) = rtf.filter(|r| !r.is_empty()) {
        out.push(Entry { fmt: Fmt::Registered(FMT_RTF), bytes: rtf_bytes(r), required: false });
    }
    out.push(Entry { fmt: Fmt::UnicodeText, bytes: utf16z(text), required: true });
    out
}

pub fn encode_files(paths: &[String]) -> Vec<Entry> {
    vec![
        Entry { fmt: Fmt::HDrop, bytes: dropfiles(paths), required: true },
        Entry { fmt: Fmt::Registered(FMT_DROP_EFFECT), bytes: dword(DROPEFFECT_COPY), required: false },
        Entry { fmt: Fmt::UnicodeText, bytes: utf16z(&paths_text(paths)), required: false },
    ]
}

/// PNG file bytes + the CF_DIB built from ONE decode of those bytes.
pub fn encode_png(png: Vec<u8>) -> Result<Vec<Entry>, String> {
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .map_err(|e| format!("png decode: {e}"))?
        .to_rgba8();
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err("empty image".into());
    }
    let dib = dib_from_rgba(w, h, img.as_raw());
    drop(img);
    Ok(vec![
        Entry { fmt: Fmt::Registered(FMT_PNG), bytes: png, required: false },
        Entry { fmt: Fmt::Dib, bytes: dib, required: true },
    ])
}

/// Encode a [`Content`] (reads the PNG file for images).
pub fn encode(content: &Content) -> Result<Vec<Entry>, WriteError> {
    match content {
        Content::Text { text, html, rtf } => Ok(encode_text(text, html.as_deref(), rtf.as_deref())),
        Content::Files { paths } => {
            if paths.is_empty() {
                return Err(WriteError::Failed("no file paths".into()));
            }
            Ok(encode_files(paths))
        }
        Content::Image { png_path } => {
            let png = std::fs::read(png_path).map_err(|e| WriteError::Failed(format!("read image: {e}")))?;
            encode_png(png).map_err(WriteError::Failed)
        }
    }
}

/// Waits (ms) before each `OpenClipboard` attempt: 11 tries, ≈ 0.5 s in total — enough for
/// brief holders (other clipboard managers, Windows clipboard history, our own listener).
pub const OPEN_BACKOFF_MS: [u64; 11] = [0, 5, 10, 20, 30, 40, 50, 60, 80, 100, 100];

// ── Win32 ───────────────────────────────────────────────────────────────────

/// Write `entries` in ONE clipboard transaction and mark it as our own write. Blocking
/// (up to ≈ 0.5 s of retries) — call from a worker / `spawn_blocking`.
#[cfg(windows)]
pub fn write(entries: &[Entry]) -> Result<(), WriteError> {
    win::write(entries)
}

#[cfg(not(windows))]
pub fn write(_entries: &[Entry]) -> Result<(), WriteError> {
    Err(WriteError::Failed("clipboard writing is Windows-only".into()))
}

/// Start the hidden clipboard-owner window (idempotent; `paste::init`).
pub fn start_owner() {
    #[cfg(windows)]
    {
        let _ = win::owner_hwnd();
    }
}

#[cfg(windows)]
mod win {
    use super::{Entry, Fmt, WriteError, OPEN_BACKOFF_MS};
    use std::sync::OnceLock;
    use std::time::Duration;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
        SetClipboardData,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassExW, TranslateMessage,
        HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
    };

    /// A GMEM_MOVEABLE block we own until `SetClipboardData` takes it; freed on drop
    /// otherwise ("If SetClipboardData succeeds, the system owns the object" —
    /// learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setclipboarddata).
    struct OwnedGlobal(Option<HGLOBAL>);

    impl OwnedGlobal {
        fn new(bytes: &[u8]) -> Result<OwnedGlobal, String> {
            // SAFETY: GlobalAlloc/GlobalLock/GlobalUnlock per the clipboard contract
            // (learn.microsoft.com/windows/win32/dataxchg/using-the-clipboard#copying-information-to-the-clipboard):
            // the block is at least `bytes.len()` long, locked only while copying.
            unsafe {
                let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(|e| format!("GlobalAlloc: {e}"))?;
                let owned = OwnedGlobal(Some(h));
                let ptr = GlobalLock(h);
                if ptr.is_null() {
                    return Err("GlobalLock returned null".into()); // `owned` frees it
                }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
                // Returns FALSE/ERROR_NOT_LOCKED once the lock count reaches 0 — success here.
                let _ = GlobalUnlock(h);
                Ok(owned)
            }
        }

        fn handle(&self) -> HANDLE {
            HANDLE(self.0.map(|h| h.0).unwrap_or(std::ptr::null_mut()))
        }

        /// Ownership moved to the system.
        fn release(&mut self) {
            self.0 = None;
        }
    }

    impl Drop for OwnedGlobal {
        fn drop(&mut self) {
            if let Some(h) = self.0.take() {
                // SAFETY: we still own `h` (never handed to the clipboard). GlobalFree returns
                // NULL on success, which windows-rs reports as Err — ignored.
                unsafe {
                    let _ = GlobalFree(Some(h));
                }
            }
        }
    }

    fn format_id(fmt: &Fmt) -> u32 {
        match fmt {
            Fmt::UnicodeText => CF_UNICODETEXT.0 as u32,
            Fmt::Dib => CF_DIB.0 as u32,
            Fmt::HDrop => CF_HDROP.0 as u32,
            Fmt::Registered(name) => {
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                // SAFETY: NUL-terminated UTF-16 name; returns 0 on failure.
                unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) }
            }
        }
    }

    /// Closes the clipboard on every exit path once it is open.
    struct OpenGuard;
    impl Drop for OpenGuard {
        fn drop(&mut self) {
            // SAFETY: only constructed after a successful OpenClipboard on this thread.
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    pub fn write(entries: &[Entry]) -> Result<(), WriteError> {
        let owner = owner_hwnd().ok_or_else(|| WriteError::Failed("clipboard owner window unavailable".into()))?;
        // Allocate (and register formats) before opening, so the clipboard stays open only for
        // the transaction itself (CLAUDE.md hard rule 4).
        let mut blocks: Vec<(u32, bool, OwnedGlobal)> = Vec::with_capacity(entries.len());
        for e in entries {
            let id = format_id(&e.fmt);
            if id == 0 {
                if e.required {
                    return Err(WriteError::Failed("RegisterClipboardFormatW failed".into()));
                }
                continue;
            }
            blocks.push((id, e.required, OwnedGlobal::new(&e.bytes).map_err(WriteError::Failed)?));
        }

        let mut opened = false;
        for wait in OPEN_BACKOFF_MS {
            if wait > 0 {
                std::thread::sleep(Duration::from_millis(wait));
            }
            // SAFETY: `owner` is our message-only window (alive for the process lifetime).
            if unsafe { OpenClipboard(Some(HWND(owner as *mut _))) }.is_ok() {
                opened = true;
                break;
            }
        }
        if !opened {
            return Err(WriteError::Busy); // `blocks` drop → every HGLOBAL freed
        }
        let guard = OpenGuard;
        // SAFETY: the clipboard is open by this thread (guard closes it).
        if unsafe { EmptyClipboard() }.is_err() {
            drop(guard);
            return Err(WriteError::Failed("EmptyClipboard failed".into()));
        }
        let mut missing_required = false;
        for (id, required, block) in &mut blocks {
            // SAFETY: open clipboard, a valid unlocked GMEM_MOVEABLE block.
            match unsafe { SetClipboardData(*id, Some(block.handle())) } {
                Ok(_) => block.release(),
                Err(_) => missing_required |= *required, // still ours → freed on drop
            }
        }
        drop(guard); // CloseClipboard
        // Self-copy guard (§5 / §15): the sequence number right after our CloseClipboard.
        // SAFETY: no preconditions.
        crate::capture::mark_own_write(unsafe { GetClipboardSequenceNumber() });
        if missing_required {
            return Err(WriteError::Failed("SetClipboardData rejected a required format".into()));
        }
        Ok(())
    }

    static OWNER: OnceLock<Option<isize>> = OnceLock::new();

    /// The hidden clipboard-owner window, created once on its own message-pumping thread.
    pub fn owner_hwnd() -> Option<isize> {
        *OWNER.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<Option<isize>>();
            let spawned = std::thread::Builder::new().name("liqlip-clip-owner".into()).spawn(move || {
                let hwnd = create_owner_window();
                let ok = hwnd.is_some();
                let _ = tx.send(hwnd);
                if ok {
                    pump();
                }
            });
            if let Err(e) = spawned {
                crate::logx::warn("paste", &format!("clipboard owner thread: {e}"));
                return None;
            }
            let hwnd = rx.recv_timeout(Duration::from_secs(5)).ok().flatten();
            if hwnd.is_none() {
                crate::logx::warn("paste", "clipboard owner window could not be created");
            }
            hwnd
        })
    }

    unsafe extern "system" fn owner_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // No delayed rendering: everything is DefWindowProc (WM_DESTROYCLIPBOARD included).
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// A message-only window (parent `HWND_MESSAGE`):
    /// learn.microsoft.com/windows/win32/winmsg/window-features#message-only-windows.
    fn create_owner_window() -> Option<isize> {
        // SAFETY: standard class registration + CreateWindowExW with static, NUL-terminated
        // strings; the window lives on this thread until the process exits.
        unsafe {
            let hinstance = GetModuleHandleW(PCWSTR::null()).ok()?;
            let class = w!("LiqlipClipboardOwner");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(owner_proc),
                hInstance: hinstance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            if RegisterClassExW(&wc) == 0 {
                return None;
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Liqlip clipboard owner"),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(hinstance.into()),
                None,
            )
            .ok()?;
            Some(hwnd.0 as isize)
        }
    }

    fn pump() {
        let mut msg = MSG::default();
        // SAFETY: classic GetMessage loop on the thread that owns the window; GetMessageW
        // returns 0 on WM_QUIT and -1 on error (both end the loop).
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_value(bytes: &[u8], key: &str) -> usize {
        let s = String::from_utf8_lossy(bytes);
        let at = s.find(key).unwrap() + key.len();
        s[at..at + 10].parse().unwrap()
    }

    #[test]
    fn cf_html_offsets_point_at_markers_and_context() {
        let html = "<html><body><!--StartFragment--><b>Привет</b>, мир<!--EndFragment--></body></html>";
        let out = cf_html(html);
        assert!(out.starts_with(b"Version:0.9\r\nStartHTML:"));
        assert_eq!(*out.last().unwrap(), 0, "NUL-terminated");
        let (sh, eh) = (header_value(&out, "StartHTML:"), header_value(&out, "EndHTML:"));
        let (sf, ef) = (header_value(&out, "StartFragment:"), header_value(&out, "EndFragment:"));
        assert_eq!(&out[sh..eh], html.as_bytes(), "context is the stored HTML, UTF-8 bytes");
        assert_eq!(eh, out.len() - 1, "the NUL is outside EndHTML");
        assert_eq!(&out[sf - START_MARKER.len()..sf], START_MARKER.as_bytes());
        assert_eq!(&out[ef..ef + END_MARKER.len()], END_MARKER.as_bytes());
        assert_eq!(std::str::from_utf8(&out[sf..ef]).unwrap(), "<b>Привет</b>, мир");
        assert!(sh < sf && sf <= ef && ef < eh);
    }

    #[test]
    fn cf_html_inserts_markers_inside_body_or_wraps() {
        let out = cf_html("<html><body class=\"x\"><p>hi</p></body></html>");
        let (sf, ef) = (header_value(&out, "StartFragment:"), header_value(&out, "EndFragment:"));
        assert_eq!(&out[sf..ef], b"<p>hi</p>");
        let sh = header_value(&out, "StartHTML:");
        assert!(std::str::from_utf8(&out[sh..]).unwrap().starts_with("<html><body class=\"x\"><!--StartFragment-->"));

        let out = cf_html("<i>bare</i>");
        let (sh, sf, ef) = (header_value(&out, "StartHTML:"), header_value(&out, "StartFragment:"), header_value(&out, "EndFragment:"));
        assert_eq!(&out[sf..ef], b"<i>bare</i>");
        assert!(std::str::from_utf8(&out[sh..]).unwrap().starts_with("<html><body><!--StartFragment-->"));
    }

    #[test]
    fn cf_html_tolerates_spaced_markers_and_a_leftover_header() {
        let spaced = "<html><body><!-- StartFragment --><u>x</u><!-- EndFragment --></body></html>";
        let out = cf_html(spaced);
        let (sf, ef) = (header_value(&out, "StartFragment:"), header_value(&out, "EndFragment:"));
        assert_eq!(&out[sf..ef], b"<u>x</u>");

        // A payload that still carries a CF_HTML header: only the context is kept.
        let inner = cf_html("<p>z</p>");
        let as_text = String::from_utf8(inner[..inner.len() - 1].to_vec()).unwrap();
        let again = cf_html(&as_text);
        assert_eq!(again, inner, "re-encoding a full CF_HTML payload is idempotent");
    }

    #[test]
    fn dropfiles_layout_is_wide_and_double_nul_terminated() {
        let paths = vec!["C:\\a.txt".to_string(), "D:\\Папка\\b.png".to_string()];
        let b = dropfiles(&paths);
        assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 20, "pFiles");
        assert_eq!(&b[4..12], &[0u8; 8], "pt = 0,0");
        assert_eq!(u32::from_le_bytes(b[12..16].try_into().unwrap()), 0, "fNC");
        assert_eq!(u32::from_le_bytes(b[16..20].try_into().unwrap()), 1, "fWide");
        let wide: Vec<u16> = b[20..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let expected: Vec<u16> = "C:\\a.txt\0D:\\Папка\\b.png\0\0".encode_utf16().collect();
        assert_eq!(wide, expected);
        assert_eq!(b.len() % 2, 0);
    }

    #[test]
    fn utf16z_and_dword() {
        assert_eq!(utf16z("Яa"), vec![0x2F, 0x04, b'a', 0, 0, 0]);
        assert_eq!(dword(DROPEFFECT_COPY), vec![1, 0, 0, 0]);
        assert_eq!(rtf_bytes(b"{\\rtf1}"), b"{\\rtf1}\0".to_vec());
        assert_eq!(rtf_bytes(b"{\\rtf1}\0"), b"{\\rtf1}\0".to_vec());
    }

    #[test]
    fn dib_is_bottom_up_bgra_flattened_over_white() {
        // 1×2: top pixel opaque red, bottom pixel fully transparent.
        let rgba = [255, 0, 0, 255, 10, 20, 30, 0];
        let d = dib_from_rgba(1, 2, &rgba);
        assert_eq!(d.len(), 40 + 8);
        assert_eq!(u32::from_le_bytes(d[0..4].try_into().unwrap()), 40);
        assert_eq!(i32::from_le_bytes(d[8..12].try_into().unwrap()), 2, "positive height = bottom-up");
        assert_eq!(u16::from_le_bytes(d[14..16].try_into().unwrap()), 32);
        assert_eq!(&d[40..44], &[255, 255, 255, 255], "first row in memory = bottom pixel → white");
        assert_eq!(&d[44..48], &[0, 0, 255, 255], "top pixel red as BGRA");
    }

    #[test]
    fn encoders_order_and_required_flags() {
        let e = encode_text("t", Some("<b>t</b>"), Some(b"{\\rtf1 t}"));
        let fmts: Vec<_> = e.iter().map(|x| (x.fmt.clone(), x.required)).collect();
        assert_eq!(
            fmts,
            vec![
                (Fmt::Registered(FMT_HTML), false),
                (Fmt::Registered(FMT_RTF), false),
                (Fmt::UnicodeText, true),
            ]
        );
        assert_eq!(encode_text("t", None, None).len(), 1, "plain = CF_UNICODETEXT only");

        let f = encode_files(&["C:\\x".into(), "C:\\y".into()]);
        assert_eq!(f[0].fmt, Fmt::HDrop);
        assert!(f[0].required);
        assert_eq!(f[1].bytes, vec![1, 0, 0, 0], "Preferred DropEffect = COPY");
        assert_eq!(f[2].bytes, utf16z("C:\\x\r\nC:\\y"), "text fallback with the paths");
    }

    #[test]
    fn png_is_kept_as_is_and_decoded_once_for_the_dib() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_raw(2, 1, vec![0, 0, 255, 255, 0, 255, 0, 128]).unwrap();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let e = encode_png(png.clone()).unwrap();
        assert_eq!(e[0].fmt, Fmt::Registered(FMT_PNG));
        assert_eq!(e[0].bytes, png, "PNG bytes untouched (alpha preserved)");
        assert_eq!(e[1].fmt, Fmt::Dib);
        assert_eq!(e[1].bytes.len(), 40 + 8);
        assert!(encode_png(b"not a png".to_vec()).is_err());
    }

    #[test]
    fn backoff_totals_about_half_a_second() {
        let total: u64 = OPEN_BACKOFF_MS.iter().sum();
        assert!((400..=600).contains(&total), "{total}");
        assert_eq!(OPEN_BACKOFF_MS[0], 0, "first attempt is immediate");
    }
}
