//! The recognizer: Windows.Media.Ocr over a stored PNG (W2-A). Thin WinRT wrappers plus the
//! pure helpers they need (grey conversion, size plan, tile ownership, line normalisation),
//! which are unit-tested.
//!
//! Pipeline (port of Liquid Tools `ocr.rs`, `git show 0295998:src-tauri/src/ocr.rs`, minus the
//! WinRT decoder): PNG bytes → `image` decode → composite on white → 8-bit grey → size plan
//! ([`plan`]) → per tile: `DataWriter` → `IBuffer` → `SoftwareBitmap::CreateCopyFromBuffer
//! (Gray8)` → `OcrEngine::RecognizeAsync` → lines joined by `\n`. Decoding with `image`
//! (already a dependency) gives one path for every size.
//!
//! **Size limit.** `OcrEngine.MaxImageDimension` (10 000 px on Windows 11, read at runtime)
//! bounds each side. An image whose short side exceeds it is scaled down with its aspect
//! kept (`image` crate). An image that is only *long* — a full-page capture (1 200 × 30 000),
//! three monitors in one screenshot (11 520 × 2 160) — is not shrunk until its text is
//! unreadable: it is cut into overlapping tiles along the long side and every word is kept
//! by exactly one tile (the one owning the word's centre), so nothing is lost at a cut and
//! nothing is doubled in an overlap. At most [`TILES_MAX`] tiles; longer images are scaled so
//! they fit.
//!
//! **Why grey.** The docs do not list the pixel formats `RecognizeAsync` accepts; Gray8 is
//! verified by the in-memory WinRT test on Windows 11 and is a quarter of BGRA8 (a 64 MP image,
//! the capture limit, costs 64 MB per copy instead of 256 MB). If an engine rejects it, the
//! bitmap is converted to BGRA8 once (`SoftwareBitmap.Convert`) and recognised again.
//!
//! Every thread that touches WinRT first enters the multithreaded apartment ([`Apartment`]):
//! "All threads that activate and interact with Windows Runtime objects must be initialized
//! prior to calling into the Windows Runtime" (RoInitialize remarks). `OcrEngine` is agile
//! (MarshalingType.Agile, ThreadingModel.Both — OcrEngine class page).
//!
//! Privacy: nothing here logs recognised text.

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, ImageFormat};

/// Text kept per record: a huge result is cut on a char boundary (search and copy still
/// work on the first 256 KB; a real screenshot yields a few KB).
pub(crate) const TEXT_MAX_BYTES: usize = 256 * 1024;
/// Most tiles one image is cut into (≈ 120 000 px of length at the 10 000 px limit).
pub(crate) const TILES_MAX: u32 = 12;
/// Overlap between neighbouring tiles: any text line up to this tall lies wholly inside the
/// tile that owns its centre.
const OVERLAP: u32 = 256;

/// Along which side the tiles are cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
}

/// One tile along the cut axis: pixels `[start, start + len)`; it keeps the words whose
/// centre lies in `[own_lo, own_hi)` (whole-image coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub start: u32,
    pub len: u32,
    pub own_lo: u32,
    pub own_hi: u32,
}

/// How an image goes to the engine: its working size and its tiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub width: u32,
    pub height: u32,
    /// `None` = one tile, the whole image.
    pub axis: Option<Axis>,
    pub spans: Vec<Span>,
}

/// Size plan for a `w × h` image and the engine's `max` side (`0` = unknown → as is).
pub(crate) fn plan(w: u32, h: u32, max: u32) -> Plan {
    let single = |w: u32, h: u32| Plan { width: w, height: h, axis: None, spans: Vec::new() };
    if max == 0 || (w <= max && h <= max) {
        return single(w, h);
    }
    let overlap = OVERLAP.min(max / 4);
    let step = f64::from(max - overlap);
    let (short, long) = (f64::from(w.min(h)), f64::from(w.max(h)));
    // Short side must fit; the long side must fit in TILES_MAX tiles.
    let reach = step * f64::from(TILES_MAX) + f64::from(overlap);
    let scale = (f64::from(max) / short).min(reach / long).min(1.0);
    let sw = ((f64::from(w) * scale).floor() as u32).max(1);
    let sh = ((f64::from(h) * scale).floor() as u32).max(1);
    let (sw, sh) = if w <= h { (sw.min(max), sh) } else { (sw, sh.min(max)) };
    if sw <= max && sh <= max {
        return single(sw, sh);
    }
    let (axis, length) = if sh > max { (Axis::Y, sh) } else { (Axis::X, sw) };
    let step = max - overlap;
    let mut spans = Vec::new();
    let mut start = 0u32;
    loop {
        let len = max.min(length - start);
        let last = start + len >= length;
        let own_lo = if start == 0 { 0 } else { start + overlap / 2 };
        let own_hi = if last { length } else { start + step + overlap / 2 };
        spans.push(Span { start, len, own_lo, own_hi });
        if last {
            break;
        }
        start += step;
    }
    Plan { width: sw, height: sh, axis: Some(axis), spans }
}

/// Does the tile own a word whose box (tile-local px) is `pos .. pos + size` along the axis?
pub(crate) fn owns(span: &Span, pos: f32, size: f32) -> bool {
    let centre = f64::from(span.start) + f64::from(pos) + f64::from(size) / 2.0;
    centre >= f64::from(span.own_lo) && centre < f64::from(span.own_hi)
}

/// Normalise recognised lines: whitespace runs inside a line collapse to one space, lines
/// are trimmed, empty lines dropped, the rest joined with `\n`, cut to [`TEXT_MAX_BYTES`].
/// A line's words are taken exactly as `OcrLine.Text` gives them — for languages written
/// without spaces Windows decides the spacing, we only collapse runs.
pub(crate) fn join_lines<I, S>(lines: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = String::new();
    for line in lines {
        let norm = line.as_ref().split_whitespace().collect::<Vec<_>>().join(" ");
        if norm.is_empty() {
            continue;
        }
        if out.len() + norm.len() + 1 > TEXT_MAX_BYTES {
            let room = TEXT_MAX_BYTES.saturating_sub(out.len() + 1);
            let mut cut = room.min(norm.len());
            while cut > 0 && !norm.is_char_boundary(cut) {
                cut -= 1;
            }
            if cut > 0 {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&norm[..cut]);
            }
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&norm);
    }
    out
}

/// Rec. 601 luma of an sRGB colour composited onto white with alpha `a`.
fn grey(r: u8, g: u8, b: u8, a: u8) -> u8 {
    let over = |c: u8| -> u32 { (u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a)) + 127) / 255 };
    ((299 * over(r) + 587 * over(g) + 114 * over(b) + 500) / 1000) as u8
}

/// Decode a PNG into 8-bit grey and apply the size [`plan`] for `max`: transparent areas are
/// composited onto white (dark text on a transparent background stays readable); a scale uses
/// the Triangle filter (keeps glyph edges). The colour image is dropped before the scale.
pub(crate) fn prepare_gray(png: &[u8], max: u32) -> Result<(GrayImage, Plan), String> {
    let img = image::load_from_memory_with_format(png, ImageFormat::Png).map_err(|e| format!("invalid: png: {e}"))?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err("invalid: empty image".into());
    }
    let luma: Vec<u8> = match img {
        DynamicImage::ImageRgba8(i) => i.as_raw().chunks_exact(4).map(|p| grey(p[0], p[1], p[2], p[3])).collect(),
        DynamicImage::ImageRgb8(i) => i.as_raw().chunks_exact(3).map(|p| grey(p[0], p[1], p[2], 255)).collect(),
        DynamicImage::ImageLuma8(i) => i.into_raw(),
        DynamicImage::ImageLumaA8(i) => i.as_raw().chunks_exact(2).map(|p| grey(p[0], p[0], p[0], p[1])).collect(),
        other => other.into_rgba8().as_raw().chunks_exact(4).map(|p| grey(p[0], p[1], p[2], p[3])).collect(),
    };
    let img = GrayImage::from_raw(w, h, luma).ok_or("invalid: pixel buffer")?;
    let p = plan(w, h, max);
    let img = if (p.width, p.height) != (w, h) { image::imageops::resize(&img, p.width, p.height, FilterType::Triangle) } else { img };
    Ok((img, p))
}

/// Grey pixels of one tile (`None` axis → the whole image), row-major, and its size.
pub(crate) fn tile_pixels(img: &GrayImage, axis: Option<Axis>, span: Option<&Span>) -> (u32, u32, Vec<u8>) {
    let (w, h) = img.dimensions();
    match (axis, span) {
        (Some(Axis::Y), Some(s)) => {
            let raw = img.as_raw();
            let from = (s.start * w) as usize;
            let to = ((s.start + s.len) * w) as usize;
            (w, s.len, raw[from..to].to_vec())
        }
        (Some(Axis::X), Some(s)) => {
            let raw = img.as_raw();
            let mut out = Vec::with_capacity((s.len * h) as usize);
            for row in 0..h {
                let base = (row * w + s.start) as usize;
                out.extend_from_slice(&raw[base..base + s.len as usize]);
            }
            (s.len, h, out)
        }
        _ => (w, h, img.as_raw().clone()),
    }
}

#[cfg(windows)]
pub(crate) use win::*;

#[cfg(windows)]
mod win {
    use super::{join_lines, owns, prepare_gray, tile_pixels, Axis, Span};
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::{OcrEngine, OcrResult};
    use windows::Storage::Streams::DataWriter;
    use windows::Win32::System::Threading::{GetCurrentThread, GetThreadPriority, SetThreadPriority, THREAD_PRIORITY, THREAD_PRIORITY_LOWEST};
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    /// The thread is in the Windows Runtime multithreaded apartment while this lives.
    /// <https://learn.microsoft.com/windows/win32/api/roapi/nf-roapi-roinitialize>: "Each
    /// successful call to RoInitialize, including those that return S_FALSE, must be balanced
    /// by a corresponding call to RoUninitialize" — so only a successful call is undone.
    /// `RPC_E_CHANGED_MODE` (the thread is already an STA) is not an error for us: agile
    /// objects work from any apartment.
    pub(crate) struct Apartment(bool);

    impl Apartment {
        pub(crate) fn enter() -> Apartment {
            // SAFETY: plain Win32 call; balanced in Drop on success.
            Apartment(unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok())
        }
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: balances the successful RoInitialize of this thread.
                // https://learn.microsoft.com/windows/win32/api/roapi/nf-roapi-rouninitialize
                unsafe { RoUninitialize() };
            }
        }
    }

    /// Run CPU-heavy work (decode, scale, recognition) at `THREAD_PRIORITY_LOWEST`, then
    /// restore the thread's priority — database calls (under the store lock) stay at normal
    /// priority, so capture never waits behind a starved OCR thread.
    /// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-setthreadpriority>
    /// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadpriority>
    pub(crate) fn low_priority<T>(f: impl FnOnce() -> T) -> T {
        // SAFETY: pseudo-handle of the calling thread; no ownership.
        let thread = unsafe { GetCurrentThread() };
        let before = match unsafe { GetThreadPriority(thread) } {
            0x7FFF_FFFF => 0, // THREAD_PRIORITY_ERROR_RETURN → restore THREAD_PRIORITY_NORMAL
            p => p,
        };
        let lowered = unsafe { SetThreadPriority(thread, THREAD_PRIORITY_LOWEST) }.is_ok();
        let out = f();
        if lowered {
            let _ = unsafe { SetThreadPriority(thread, THREAD_PRIORITY(before)) };
        }
        out
    }

    fn hr(context: &str, e: windows::core::Error) -> String {
        format!("failed: {context}: 0x{:08X}", e.code().0 as u32)
    }

    /// BCP-47 tags of the installed OCR languages (`Language.LanguageTag` of every entry of
    /// `OcrEngine.AvailableRecognizerLanguages` — "Returns a collection of all available OCR
    /// languages on the device"). Order as Windows lists them.
    /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrengine.availablerecognizerlanguages>
    /// <https://learn.microsoft.com/uwp/api/windows.globalization.language.languagetag>
    pub(crate) fn languages() -> Result<Vec<String>, String> {
        let list = OcrEngine::AvailableRecognizerLanguages().map_err(|e| hr("AvailableRecognizerLanguages", e))?;
        let mut tags = Vec::new();
        for lang in list {
            let tag = lang.LanguageTag().map_err(|e| hr("LanguageTag", e))?.to_string();
            if !tag.is_empty() && !tags.iter().any(|t: &String| t.eq_ignore_ascii_case(&tag)) {
                tags.push(tag);
            }
        }
        Ok(tags)
    }

    /// "The maximum image pixel dimensions supported by the OCR engine"; 0 if unreadable
    /// (then the image goes as is and the engine reports the failure).
    /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrengine.maximagedimension>
    pub(crate) fn max_dimension() -> u32 {
        OcrEngine::MaxImageDimension().unwrap_or(0)
    }

    /// A ready recognizer and the language it was built for.
    pub(crate) struct Engine {
        inner: OcrEngine,
        /// The `ocr.language` setting it was made for (`"auto"` or a tag).
        pub(crate) setting: String,
        /// The language actually used (`RecognizerLanguage.LanguageTag`), for the log.
        pub(crate) tag: String,
        max: u32,
    }

    /// Why no engine could be made.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Unavailable {
        /// No OCR language is installed (or none resolves) — `available: false`.
        NoLanguages,
        /// WinRT itself failed (message carries the HRESULT, never content).
        Failed(String),
    }

    impl Engine {
        /// Build an engine for the `ocr.language` setting.
        ///
        /// - `"auto"` → `TryCreateFromUserProfileLanguages` ("If any language from
        ///   GlobalizationPreferences.Languages can be resolved to any of the available OCR
        ///   languages returns new instance …, otherwise returns null"); null while OCR
        ///   languages exist (a profile language without its OCR pack) → the first available
        ///   language, so recognition still works.
        /// - a tag → only if it is in `AvailableRecognizerLanguages` (case-insensitive):
        ///   `TryCreateFromLanguage(new Language(tag))` (null → not resolvable). A tag whose
        ///   pack was removed falls back to `"auto"` (visible in the log label).
        ///
        /// windows-rs maps a null class result to `Err` with an empty error
        /// (windows-core `Type::from_abi`), so `Err` from the `TryCreate*` calls = null.
        /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrengine.trycreatefromuserprofilelanguages>
        /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrengine.trycreatefromlanguage>
        /// <https://learn.microsoft.com/uwp/api/windows.globalization.language.-ctor>
        pub(crate) fn create(setting: &str) -> Result<Engine, Unavailable> {
            let available = languages().map_err(Unavailable::Failed)?;
            if available.is_empty() {
                return Err(Unavailable::NoLanguages);
            }
            let wanted = available.iter().find(|t| t.eq_ignore_ascii_case(setting)).cloned();
            let made = match &wanted {
                Some(tag) => Self::from_tag(tag),
                None => OcrEngine::TryCreateFromUserProfileLanguages().ok().or_else(|| Self::from_tag(&available[0])),
            };
            let inner = made.ok_or(Unavailable::NoLanguages)?;
            // RecognizerLanguage: "Gets the language being used for text recognition."
            // https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrengine.recognizerlanguage
            let tag = inner
                .RecognizerLanguage()
                .and_then(|l| l.LanguageTag())
                .map(|t| t.to_string())
                .unwrap_or_default();
            Ok(Engine { inner, setting: setting.to_string(), tag, max: max_dimension() })
        }

        /// Tests: pretend the engine's size limit is `max` (exercises tiling on small images).
        #[cfg(test)]
        pub(crate) fn with_max(mut self, max: u32) -> Engine {
            self.max = max;
            self
        }

        fn from_tag(tag: &str) -> Option<OcrEngine> {
            let lang = Language::CreateLanguage(&HSTRING::from(tag)).ok()?;
            OcrEngine::TryCreateFromLanguage(&lang).ok()
        }

        /// Recognise the text of one PNG. An image without text → `Ok("")`.
        pub(crate) fn recognize_png(&self, png: &[u8]) -> Result<String, String> {
            let (img, plan) = prepare_gray(png, self.max)?;
            let mut lines: Vec<String> = Vec::new();
            if plan.spans.is_empty() {
                let (w, h, px) = tile_pixels(&img, None, None);
                drop(img);
                let result = self.recognize_gray(w, h, px)?;
                // Result → Lines → OcrLine.Text.
                // https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrresult.lines
                // https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrline.text
                for line in result.Lines().map_err(|e| hr("Lines", e))? {
                    lines.push(line.Text().map_err(|e| hr("OcrLine.Text", e))?.to_string());
                }
            } else {
                for span in &plan.spans {
                    let (w, h, px) = tile_pixels(&img, plan.axis, Some(span));
                    let result = self.recognize_gray(w, h, px)?;
                    collect_owned(&result, plan.axis.unwrap_or(Axis::Y), span, &mut lines)?;
                }
            }
            Ok(join_lines(lines))
        }

        /// One Gray8 bitmap → OcrResult (BGRA8 retry if the engine rejects Gray8).
        fn recognize_gray(&self, w: u32, h: u32, px: Vec<u8>) -> Result<OcrResult, String> {
            // IBuffer from bytes: DataWriter() → WriteBytes → DetachBuffer.
            // https://learn.microsoft.com/uwp/api/windows.storage.streams.datawriter.-ctor
            // https://learn.microsoft.com/uwp/api/windows.storage.streams.datawriter.writebytes
            // https://learn.microsoft.com/uwp/api/windows.storage.streams.datawriter.detachbuffer
            let writer = DataWriter::new().map_err(|e| hr("DataWriter", e))?;
            writer.WriteBytes(&px).map_err(|e| hr("WriteBytes", e))?;
            drop(px);
            let buffer = writer.DetachBuffer().map_err(|e| hr("DetachBuffer", e))?;
            // Deep copy into a Gray8 SoftwareBitmap (no alpha channel → Ignore).
            // https://learn.microsoft.com/uwp/api/windows.graphics.imaging.softwarebitmap.createcopyfrombuffer
            let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
                &buffer,
                BitmapPixelFormat::Gray8,
                w as i32,
                h as i32,
                BitmapAlphaMode::Ignore,
            )
            .map_err(|e| hr("CreateCopyFromBuffer", e))?;
            drop(buffer);
            match self.run(&bitmap) {
                Ok(r) => Ok(r),
                Err(first) => {
                    // An engine that does not take Gray8: convert once and retry.
                    // https://learn.microsoft.com/uwp/api/windows.graphics.imaging.softwarebitmap.convert
                    let bgra = SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8).map_err(|_| first.clone())?;
                    drop(bitmap);
                    self.run(&bgra).map_err(|_| first)
                }
            }
        }

        /// RecognizeAsync → IAsyncOperation<OcrResult>; `join()` (windows-future) blocks this
        /// worker thread until completion.
        /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrengine.recognizeasync>
        fn run(&self, bitmap: &SoftwareBitmap) -> Result<OcrResult, String> {
            self.inner
                .RecognizeAsync(bitmap)
                .map_err(|e| hr("RecognizeAsync", e))?
                .join()
                .map_err(|e| hr("RecognizeAsync", e))
        }
    }

    /// Lines of one tile, keeping only the words this tile owns (`OcrWord.BoundingRect`, tile
    /// pixels). A wholly owned line keeps `OcrLine.Text` as is; a line cut by the ownership
    /// boundary keeps its owned words joined by spaces.
    /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrline.words>
    /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrword.boundingrect>
    /// <https://learn.microsoft.com/uwp/api/windows.media.ocr.ocrword.text>
    fn collect_owned(result: &OcrResult, axis: Axis, span: &Span, out: &mut Vec<String>) -> Result<(), String> {
        for line in result.Lines().map_err(|e| hr("Lines", e))? {
            let mut kept = Vec::new();
            let mut all = true;
            for word in line.Words().map_err(|e| hr("Words", e))? {
                let r = word.BoundingRect().map_err(|e| hr("BoundingRect", e))?;
                let (pos, size) = match axis {
                    Axis::Y => (r.Y, r.Height),
                    Axis::X => (r.X, r.Width),
                };
                if owns(span, pos, size) {
                    kept.push(word.Text().map_err(|e| hr("OcrWord.Text", e))?.to_string());
                } else {
                    all = false;
                }
            }
            if all && !kept.is_empty() {
                out.push(line.Text().map_err(|e| hr("OcrLine.Text", e))?.to_string());
            } else if !kept.is_empty() {
                out.push(kept.join(" "));
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod other {
    //! Non-Windows builds compile (CI tooling) but have no recognizer.
    pub(crate) struct Apartment;
    impl Apartment {
        pub(crate) fn enter() -> Apartment {
            Apartment
        }
    }
    pub(crate) fn low_priority<T>(f: impl FnOnce() -> T) -> T {
        f()
    }
    pub(crate) fn languages() -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
    pub(crate) struct Engine {
        pub(crate) setting: String,
        pub(crate) tag: String,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Unavailable {
        NoLanguages,
        #[allow(dead_code)]
        Failed(String),
    }
    impl Engine {
        pub(crate) fn create(_setting: &str) -> Result<Engine, Unavailable> {
            Err(Unavailable::NoLanguages)
        }
        pub(crate) fn recognize_png(&self, _png: &[u8]) -> Result<String, String> {
            Err("failed: OCR is Windows-only".into())
        }
    }
}
#[cfg(not(windows))]
pub(crate) use other::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_images_go_as_they_are() {
        assert_eq!(plan(800, 600, 10_000), Plan { width: 800, height: 600, axis: None, spans: vec![] });
        assert_eq!(plan(10_000, 10_000, 10_000).axis, None);
        assert_eq!(plan(50_000, 50_000, 0).axis, None, "unknown limit → as is");
    }

    #[test]
    fn a_long_image_is_tiled_with_contiguous_ownership() {
        let p = plan(1_200, 30_000, 10_000);
        assert_eq!((p.width, p.height, p.axis), (1_200, 30_000, Some(Axis::Y)), "not shrunk");
        assert_eq!(p.spans.len(), 4);
        assert_eq!(p.spans[0].own_lo, 0);
        assert_eq!(p.spans.last().unwrap().own_hi, 30_000);
        for s in &p.spans {
            assert!(s.len <= 10_000 && s.start + s.len <= 30_000);
            assert!(s.own_lo >= s.start && s.own_hi <= s.start + s.len, "a tile owns only what it sees: {s:?}");
        }
        for w in p.spans.windows(2) {
            assert_eq!(w[0].own_hi, w[1].own_lo, "ownership is contiguous");
            assert_eq!(w[0].start + w[0].len - w[1].start, 256, "neighbours overlap");
        }
        // Three 4K monitors side by side → tiles along X.
        let p = plan(11_520, 2_160, 10_000);
        assert_eq!((p.width, p.height, p.axis, p.spans.len()), (11_520, 2_160, Some(Axis::X), 2));
    }

    #[test]
    fn short_side_over_the_limit_or_too_many_tiles_scale_down() {
        // Both sides over (an old engine's small limit): short side fits, long side tiled.
        let p = plan(6_000, 12_000, 2_600);
        assert!((2_599..=2_600).contains(&p.width), "{p:?}");
        assert!((5_199..=5_200).contains(&p.height), "{p:?}");
        assert_eq!(p.axis, Some(Axis::Y));
        // Extremely long: at most TILES_MAX tiles.
        let p = plan(1_000, 1_000_000, 10_000);
        assert!(p.spans.len() as u32 <= TILES_MAX, "{}", p.spans.len());
        assert!(p.width >= 100 && p.height < 1_000_000);
        // Never 0 px.
        let p = plan(100_000, 1, 2_600);
        assert!(p.width >= 1 && p.height >= 1);
    }

    #[test]
    fn every_word_belongs_to_exactly_one_tile() {
        let p = plan(1_200, 30_000, 10_000);
        // A 40-px line at every position along the image, in whole-image coordinates.
        for top in (0..30_000 - 40).step_by(7) {
            let owners: Vec<&Span> = p
                .spans
                .iter()
                .filter(|s| top >= s.start && top + 40 <= s.start + s.len)
                .filter(|s| owns(s, (top - s.start) as f32, 40.0))
                .collect();
            assert_eq!(owners.len(), 1, "line at {top}");
        }
    }

    #[test]
    fn tile_pixels_cut_rows_and_columns() {
        let img = GrayImage::from_fn(4, 3, |x, y| image::Luma([(y * 10 + x) as u8]));
        let s = Span { start: 1, len: 2, own_lo: 1, own_hi: 3 };
        assert_eq!(tile_pixels(&img, Some(Axis::Y), Some(&s)), (4, 2, vec![10, 11, 12, 13, 20, 21, 22, 23]));
        assert_eq!(tile_pixels(&img, Some(Axis::X), Some(&s)), (2, 3, vec![1, 2, 11, 12, 21, 22]));
        assert_eq!(tile_pixels(&img, None, None).2.len(), 12);
    }

    #[test]
    fn join_lines_normalises_whitespace_and_drops_empty_lines() {
        assert_eq!(join_lines(["  Total   due: $5 ", "", "   ", "Invoice\t#7"]), "Total due: $5\nInvoice #7");
        assert_eq!(join_lines(Vec::<String>::new()), "");
        assert_eq!(join_lines(["Привет  мир"]), "Привет мир");
    }

    #[test]
    fn join_lines_caps_the_text_on_a_char_boundary() {
        let line = "я".repeat(100_000); // 200 000 bytes
        let out = join_lines([line.clone(), line.clone()]);
        assert!(out.len() <= TEXT_MAX_BYTES);
        assert!(out.starts_with(&line));
        assert!(out.len() > 200_000);
    }

    fn png(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba(px));
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img).write_to(std::io::Cursor::new(&mut out), ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn prepare_gray_composites_on_white_and_scales() {
        // Fully transparent black → white; opaque black → black; red → its luma.
        let (img, p) = prepare_gray(&png(4, 2, [0, 0, 0, 0]), 0).unwrap();
        assert_eq!(img.dimensions(), (4, 2));
        assert_eq!(p.axis, None);
        assert_eq!(img.into_raw(), vec![255; 8]);
        assert_eq!(prepare_gray(&png(2, 2, [0, 0, 0, 255]), 0).unwrap().0.into_raw(), vec![0; 4]);
        assert_eq!(prepare_gray(&png(1, 1, [255, 0, 0, 255]), 0).unwrap().0.into_raw(), vec![76]);
        // Half-transparent black over white → mid grey.
        let mid = prepare_gray(&png(1, 1, [0, 0, 0, 128]), 0).unwrap().0.into_raw()[0];
        assert!((120..=135).contains(&mid), "{mid}");
        // Short side over the limit → scaled, aspect kept, one byte per pixel.
        let (img, p) = prepare_gray(&png(300, 200, [0, 0, 0, 255]), 100).unwrap();
        assert_eq!(img.dimensions(), (p.width, p.height));
        assert_eq!(p.height, 100);
        assert_eq!(img.as_raw().len(), (p.width * p.height) as usize);
        assert!(prepare_gray(b"not a png", 100).unwrap_err().starts_with("invalid:"));
    }
}
