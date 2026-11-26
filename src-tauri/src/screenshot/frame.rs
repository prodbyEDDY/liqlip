//! Frozen frames and base pictures over the `liq` scheme (owner: W6-A; ARCHITECTURE §18.3).
//!
//! A frozen monitor is ONE buffer: a whole BMP file — `BITMAPFILEHEADER` (14 bytes) +
//! `BITMAPINFOHEADER` (40 bytes, 32 bpp `BI_RGB`, negative height = top-down rows) + the
//! `GetDIBits` rows (B, G, R, X) with X forced to 255. The scheme serves it as is
//! (`/shot/<sessionId>/<monitorIndex>.bmp`) and the commit crops from the same bytes — no
//! parallel RGBA copy (defect П-21 of Liquid Tools). Layout:
//! https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-bitmapfileheader ·
//! https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-bitmapinfoheader ("If
//! biHeight is negative, the bitmap is a top-down DIB and its origin is the upper-left corner").
//! Port of Liquid Tools `screenshot/mod.rs` `push_bmp_headers` / `bmp_from_bgrx` / `clamp_crop`.
//!
//! `lib.rs` routes `http://liq.localhost/shot/…` and `/base/…` here (`screenshot::owns_uri`);
//! the response gets `Cache-Control: no-store` and ACAO `*` (the overlay draws the frame into a
//! canvas: `<img crossorigin="anonymous">`).

use crate::model::Rect;
use crate::store::Store;
use tauri::{AppHandle, Manager};

/// `BITMAPFILEHEADER` + `BITMAPINFOHEADER`.
pub(crate) const HEADER: usize = 14 + 40;

/// One frozen monitor (see the module doc). Immutable once grabbed; shared by the session and
/// the scheme through an `Arc`.
pub(crate) struct Frame {
    bmp: Vec<u8>,
    width: u32,
    height: u32,
}

impl Frame {
    /// A black frame with its header written; `None` when the size is empty or too large for a
    /// BMP / the address space.
    pub(crate) fn new(width: u32, height: u32) -> Option<Frame> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return None;
        }
        let pixels = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        if HEADER + pixels > u32::MAX as usize {
            return None;
        }
        let mut bmp = vec![0u8; HEADER + pixels];
        bmp[..HEADER].copy_from_slice(&bmp_header(width, height));
        Some(Frame { bmp, width, height })
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    /// The whole BMP file.
    pub(crate) fn bmp(&self) -> &[u8] {
        &self.bmp
    }

    /// The rows (B, G, R, X), top-down, `width * 4` bytes each.
    pub(crate) fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.bmp[HEADER..]
    }

    /// The desktop has no alpha: X (undefined after `GetDIBits`) → 255, so a BMP decoder
    /// that reads the fourth byte as alpha still shows an opaque picture.
    pub(crate) fn make_opaque(&mut self) {
        for px in self.pixels_mut().chunks_exact_mut(4) {
            px[3] = 255;
        }
    }

    /// RGBA8 of a crop already inside the frame (`clamp_crop`): one pass, B↔R swapped.
    pub(crate) fn crop_rgba(&self, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
        debug_assert!(x + w <= self.width && y + h <= self.height);
        let stride = self.width as usize * 4;
        let row = w as usize * 4;
        let mut out = Vec::with_capacity(row * h as usize);
        for r in 0..h as usize {
            let start = HEADER + (y as usize + r) * stride + x as usize * 4;
            for px in self.bmp[start..start + row].chunks_exact(4) {
                out.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        out
    }
}

/// The 54-byte header of a top-down 32 bpp `BI_RGB` BMP (little-endian fields).
pub(crate) fn bmp_header(w: u32, h: u32) -> [u8; HEADER] {
    let image = w * h * 4;
    let mut out = [0u8; HEADER];
    let mut put = |at: usize, bytes: &[u8]| out[at..at + bytes.len()].copy_from_slice(bytes);
    // BITMAPFILEHEADER: "BM", file size, 2 reserved words, offset of the pixels.
    put(0, b"BM");
    put(2, &(HEADER as u32 + image).to_le_bytes());
    put(10, &(HEADER as u32).to_le_bytes());
    // BITMAPINFOHEADER: size, width, height (negative = top-down), planes, bpp, BI_RGB,
    // image size; resolution and palette fields stay 0.
    put(14, &40u32.to_le_bytes());
    put(18, &(w as i32).to_le_bytes());
    put(22, &(-(h as i32)).to_le_bytes());
    put(26, &1u16.to_le_bytes());
    put(28, &32u16.to_le_bytes());
    put(30, &0u32.to_le_bytes());
    put(34, &image.to_le_bytes());
    out
}

/// Clamp a crop (origin may be negative) to a `fw`×`fh` frame → `(x, y, w, h)` inside it, or
/// `None` when nothing of it is inside (Liquid Tools `clamp_crop`, same tests).
pub(crate) fn clamp_crop(x: i32, y: i32, w: u32, h: u32, fw: u32, fh: u32) -> Option<(u32, u32, u32, u32)> {
    if w == 0 || h == 0 {
        return None;
    }
    let right = x.checked_add(i32::try_from(w).ok()?)?;
    let bottom = y.checked_add(i32::try_from(h).ok()?)?;
    if right <= 0 || bottom <= 0 || x >= fw as i32 || y >= fh as i32 {
        return None;
    }
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let cw = (right.min(fw as i32) as u32) - x0;
    let ch = (bottom.min(fh as i32) as u32) - y0;
    if cw == 0 || ch == 0 {
        return None;
    }
    Some((x0, y0, cw, ch))
}

/// The commit's `rect` (physical px of the monitor frame, possibly fractional) → a crop: every
/// edge rounded to the nearest pixel, then [`clamp_crop`]. The overlay exports exactly this
/// rectangle when it sends pixels (§18.4).
pub(crate) fn rect_to_crop(r: &Rect, fw: u32, fh: u32) -> Option<(u32, u32, u32, u32)> {
    if ![r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) || r.w <= 0.0 || r.h <= 0.0 {
        return None;
    }
    let edge = |v: f64| v.round().clamp(i32::MIN as f64, i32::MAX as f64) as i64;
    let (left, top, right, bottom) = (edge(r.x), edge(r.y), edge(r.x + r.w), edge(r.y + r.h));
    let w = u32::try_from(right - left).ok()?;
    let h = u32::try_from(bottom - top).ok()?;
    clamp_crop(i32::try_from(left).ok()?, i32::try_from(top).ok()?, w, h, fw, fh)
}

/// A `liq` path of this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    /// `/shot/<sessionId>/<monitorIndex>.bmp`
    Shot { session: i64, monitor: usize },
    /// `/base/<clipId>` (the query `?v=` is not part of the path)
    Base { clip: i64 },
}

pub(crate) fn parse_route(path: &str) -> Option<Route> {
    let id = |s: &str| s.parse::<i64>().ok().filter(|v| *v > 0);
    let mut parts = path.trim_start_matches('/').split('/');
    let route = match (parts.next()?, parts.next(), parts.next()) {
        ("shot", Some(session), Some(file)) => {
            let monitor = file.strip_suffix(".bmp")?;
            if monitor.is_empty() || !monitor.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Route::Shot { session: id(session)?, monitor: monitor.parse().ok()? }
        }
        ("base", Some(clip), None) => Route::Base { clip: id(clip)? },
        _ => return None,
    };
    parts.next().is_none().then_some(route)
}

/// `(mime, bytes)` of a `/shot/…` or `/base/…` path; `None` → 404. The frame is copied out of
/// the session after its lock is released (the scheme's body is an owned `Vec`).
pub(crate) fn serve(app: &AppHandle, path: &str) -> Option<(String, Vec<u8>)> {
    match parse_route(path)? {
        Route::Shot { session, monitor } => {
            let frame = super::session::frame(app, session, monitor)?;
            Some(("image/bmp".into(), frame.bmp().to_vec()))
        }
        Route::Base { clip } => {
            let bytes = app.try_state::<Store>()?.serve_base(clip)?;
            Some(("image/png".into(), bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_header_is_a_top_down_32bpp_bitmap() {
        let mut f = Frame::new(3, 2).unwrap();
        f.pixels_mut()[..4].copy_from_slice(&[10, 20, 30, 0]);
        f.make_opaque();
        let b = f.bmp();
        assert_eq!(b.len(), HEADER + 3 * 2 * 4);
        assert_eq!(&b[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(b[2..6].try_into().unwrap()) as usize, b.len());
        assert_eq!(u32::from_le_bytes(b[10..14].try_into().unwrap()), 54);
        assert_eq!(i32::from_le_bytes(b[18..22].try_into().unwrap()), 3);
        assert_eq!(i32::from_le_bytes(b[22..26].try_into().unwrap()), -2, "negative height = top-down");
        assert_eq!(u16::from_le_bytes(b[28..30].try_into().unwrap()), 32);
        assert_eq!(&b[HEADER..HEADER + 4], &[10, 20, 30, 255]);
        // The image crate reads it back as the same picture.
        let img = image::load_from_memory_with_format(b, image::ImageFormat::Bmp).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (3, 2));
        assert_eq!(img.get_pixel(0, 0).0, [30, 20, 10, 255]);
        assert!(Frame::new(0, 5).is_none());
    }

    #[test]
    fn crop_swaps_to_rgba_row_by_row() {
        let mut f = Frame::new(4, 3).unwrap();
        for (i, px) in f.pixels_mut().chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[i as u8, 100, 200, 0]); // B, G, R, X
        }
        let c = f.crop_rgba(1, 1, 2, 2);
        // Pixels 5, 6 (row 1) and 9, 10 (row 2).
        assert_eq!(c, vec![200, 100, 5, 255, 200, 100, 6, 255, 200, 100, 9, 255, 200, 100, 10, 255]);
    }

    #[test]
    fn clamp_crop_matches_liquid_tools() {
        assert_eq!(clamp_crop(10, 20, 30, 40, 100, 100), Some((10, 20, 30, 40)));
        assert_eq!(clamp_crop(80, 90, 50, 50, 100, 100), Some((80, 90, 20, 10)));
        assert_eq!(clamp_crop(-10, -5, 30, 30, 100, 100), Some((0, 0, 20, 25)));
        assert_eq!(clamp_crop(0, 0, 0, 10, 100, 100), None);
        assert_eq!(clamp_crop(0, 0, 10, 0, 100, 100), None);
        assert_eq!(clamp_crop(100, 0, 10, 10, 100, 100), None);
        assert_eq!(clamp_crop(0, 100, 10, 10, 100, 100), None);
        assert_eq!(clamp_crop(-20, 0, 10, 10, 100, 100), None);
        assert_eq!(clamp_crop(0, -20, 10, 10, 100, 100), None);
        assert_eq!(clamp_crop(i32::MAX, 0, 10, 10, 100, 100), None, "no overflow");
    }

    #[test]
    fn rect_edges_round_then_clamp() {
        let r = |x, y, w, h| Rect { x, y, w, h };
        assert_eq!(rect_to_crop(&r(10.4, 20.6, 30.2, 9.8), 100, 100), Some((10, 21, 31, 9)));
        assert_eq!(rect_to_crop(&r(-3.0, 95.0, 10.0, 10.0), 100, 100), Some((0, 95, 7, 5)));
        assert_eq!(rect_to_crop(&r(0.0, 0.0, 0.4, 10.0), 100, 100), None, "rounds to nothing");
        assert_eq!(rect_to_crop(&r(f64::NAN, 0.0, 5.0, 5.0), 100, 100), None);
        assert_eq!(rect_to_crop(&r(0.0, 0.0, -5.0, 5.0), 100, 100), None);
        assert_eq!(rect_to_crop(&r(0.0, 0.0, 1e12, 1e12), 100, 100), Some((0, 0, 100, 100)), "clamped to the frame");
    }

    #[test]
    fn routes() {
        assert_eq!(parse_route("/shot/12/0.bmp"), Some(Route::Shot { session: 12, monitor: 0 }));
        assert_eq!(parse_route("/shot/3/2.bmp"), Some(Route::Shot { session: 3, monitor: 2 }));
        assert_eq!(parse_route("/base/42"), Some(Route::Base { clip: 42 }));
        for bad in ["/shot/12/0.png", "/shot/0/0.bmp", "/shot/x/0.bmp", "/shot/1/-1.bmp", "/shot/1/.bmp", "/shot/1", "/base/0", "/base/1/2", "/base/", "/thumb/1", "/shot/1/0.bmp/x"] {
            assert_eq!(parse_route(bad), None, "{bad}");
        }
    }
}
