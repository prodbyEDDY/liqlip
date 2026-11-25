//! Image work for capture — runs on the worker thread, never while the clipboard is open:
//! decode the clipboard image (`PNG` → RGBA8, `CF_DIBV5`/`CF_DIB` → RGBA8 via the `image`
//! crate's headerless BMP decoder), normalize it (so the pixel hash — `content::image_hash`
//! — is stable across sources), encode the stored PNG and the ≤ 360 px thumbnail.

use crate::content::IMAGE_MAX_PIXELS;
use image::codecs::bmp::BmpDecoder;
use image::codecs::png::{PngDecoder, PngEncoder};
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, RgbaImage};
use std::io::Cursor;

/// Long side of `media\thumbs\<id>.png` (ARCHITECTURE §4.1).
pub const THUMB_MAX: u32 = 360;

/// Which clipboard format the bytes came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource {
    /// Registered "PNG" format: a complete PNG file.
    Png,
    /// `CF_DIBV5` / `CF_DIB`: a packed DIB (header [+ masks] [+ palette] + pixels), no
    /// BITMAPFILEHEADER.
    Dib,
}

/// Decoded, normalized RGBA8 pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Above `content::IMAGE_MAX_PIXELS` (SPEC §2.3: not stored, logged).
    TooLarge(u32, u32),
    Invalid(String),
}

fn check_size(w: u32, h: u32) -> Result<(), DecodeError> {
    if w == 0 || h == 0 {
        return Err(DecodeError::Invalid("empty image".into()));
    }
    if u64::from(w) * u64::from(h) > IMAGE_MAX_PIXELS {
        return Err(DecodeError::TooLarge(w, h));
    }
    Ok(())
}

/// Decode clipboard image bytes to normalized RGBA8. The size limit is checked from the
/// header before any pixel buffer is allocated.
pub fn decode(source: ImageSource, bytes: &[u8]) -> Result<Decoded, DecodeError> {
    let inv = |e: image::ImageError| DecodeError::Invalid(e.to_string());
    let img = match source {
        ImageSource::Png => {
            let dec = PngDecoder::new(Cursor::new(bytes)).map_err(inv)?;
            let (w, h) = dec.dimensions();
            check_size(w, h)?;
            DynamicImage::from_decoder(dec).map_err(inv)?
        }
        ImageSource::Dib => {
            let dec = BmpDecoder::new_without_file_header(Cursor::new(bytes)).map_err(inv)?;
            let (w, h) = dec.dimensions();
            check_size(w, h)?;
            DynamicImage::from_decoder(dec).map_err(inv)?
        }
    };
    let mut rgba = img.into_rgba8();
    normalize_alpha(&mut rgba);
    let (width, height) = rgba.dimensions();
    Ok(Decoded { width, height, rgba: rgba.into_raw() })
}

/// Some writers put a 32 bpp DIB with an alpha mask but zero alpha everywhere; shown as is
/// the image would be invisible. Fully transparent → opaque (a deliberately transparent
/// image is never *entirely* alpha 0).
fn normalize_alpha(img: &mut RgbaImage) {
    if img.pixels().all(|p| p.0[3] == 0) {
        for p in img.pixels_mut() {
            p.0[3] = 255;
        }
    }
}

/// Encode RGBA8 as PNG (image crate default: fast deflate, adaptive filter).
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Thumbnail ≤ [`THUMB_MAX`] on the long side, aspect kept; `None` when the image already
/// fits (the caller reuses the full PNG).
pub fn thumbnail_png(d: &Decoded) -> Result<Option<Vec<u8>>, String> {
    let long = d.width.max(d.height);
    if long <= THUMB_MAX {
        return Ok(None);
    }
    let scale = f64::from(THUMB_MAX) / f64::from(long);
    let tw = ((f64::from(d.width) * scale).round() as u32).clamp(1, THUMB_MAX);
    let th = ((f64::from(d.height) * scale).round() as u32).clamp(1, THUMB_MAX);
    let src = RgbaImage::from_raw(d.width, d.height, d.rgba.clone())
        .ok_or_else(|| "rgba buffer/size mismatch".to_string())?;
    let small = image::imageops::thumbnail(&src, tw, th);
    encode_png(tw, th, small.as_raw()).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.iter().copied().cycle().take((w * h * 4) as usize).collect()
    }

    /// A packed bottom-up DIB: BITMAPINFOHEADER, 32 bpp BI_RGB (alpha byte "reserved").
    fn dib32(w: u32, h: u32, bgra: [u8; 4]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&(w as i32).to_le_bytes());
        d.extend_from_slice(&(h as i32).to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&32u16.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        d.extend_from_slice(&(w * h * 4).to_le_bytes());
        d.extend_from_slice(&[0u8; 16]);
        for _ in 0..w * h {
            d.extend_from_slice(&bgra);
        }
        d
    }

    /// 24 bpp bottom-up DIB with row padding: top row red, bottom row blue.
    fn dib24_two_rows(w: u32) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&(w as i32).to_le_bytes());
        d.extend_from_slice(&2i32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&24u16.to_le_bytes());
        d.extend_from_slice(&[0u8; 24]);
        let stride = (w as usize * 3).div_ceil(4) * 4;
        for bgr in [[255u8, 0, 0], [0, 0, 255]] {
            // bottom row first (blue), then top (red)
            let mut row: Vec<u8> = bgr.iter().copied().cycle().take(w as usize * 3).collect();
            row.resize(stride, 0);
            d.extend_from_slice(&row);
        }
        d
    }

    #[test]
    fn png_round_trip_is_pixel_exact() {
        let rgba = solid(3, 2, [10, 20, 30, 128]);
        let png = encode_png(3, 2, &rgba).unwrap();
        let d = decode(ImageSource::Png, &png).unwrap();
        assert_eq!((d.width, d.height), (3, 2));
        assert_eq!(d.rgba, rgba);
    }

    #[test]
    fn dib_32bpp_bi_rgb_is_opaque_and_oriented() {
        let d = decode(ImageSource::Dib, &dib32(2, 2, [30, 20, 10, 0])).unwrap();
        assert_eq!((d.width, d.height), (2, 2));
        assert_eq!(&d.rgba[..4], &[10, 20, 30, 255]);
        let d = decode(ImageSource::Dib, &dib24_two_rows(3)).unwrap();
        assert_eq!((d.width, d.height), (3, 2));
        assert_eq!(&d.rgba[..4], &[255, 0, 0, 255], "top row is red (bottom-up DIB)");
        assert_eq!(&d.rgba[d.rgba.len() - 4..], &[0, 0, 255, 255]);
    }

    #[test]
    fn same_pixels_same_hash_regardless_of_format() {
        let png = encode_png(2, 2, &solid(2, 2, [10, 20, 30, 255])).unwrap();
        let a = decode(ImageSource::Png, &png).unwrap();
        let b = decode(ImageSource::Dib, &dib32(2, 2, [30, 20, 10, 0])).unwrap();
        assert_eq!(
            crate::content::image_hash(a.width, a.height, &a.rgba),
            crate::content::image_hash(b.width, b.height, &b.rgba)
        );
    }

    #[test]
    fn fully_transparent_becomes_opaque() {
        let png = encode_png(2, 1, &solid(2, 1, [1, 2, 3, 0])).unwrap();
        let d = decode(ImageSource::Png, &png).unwrap();
        assert!(d.rgba.chunks(4).all(|p| p[3] == 255));
        // Partial transparency is kept.
        let mut rgba = solid(2, 1, [1, 2, 3, 0]);
        rgba[7] = 200;
        let d = decode(ImageSource::Png, &encode_png(2, 1, &rgba).unwrap()).unwrap();
        assert_eq!((d.rgba[3], d.rgba[7]), (0, 200));
    }

    #[test]
    fn limits_and_garbage() {
        // 9000 × 8000 = 72 MP > 64 MP: rejected from the header, nothing allocated.
        let mut hdr = dib32(1, 1, [0, 0, 0, 0]);
        hdr[4..8].copy_from_slice(&9000i32.to_le_bytes());
        hdr[8..12].copy_from_slice(&8000i32.to_le_bytes());
        assert_eq!(decode(ImageSource::Dib, &hdr), Err(DecodeError::TooLarge(9000, 8000)));
        assert!(matches!(decode(ImageSource::Png, b"not a png"), Err(DecodeError::Invalid(_))));
        assert!(matches!(decode(ImageSource::Dib, &[1, 2, 3]), Err(DecodeError::Invalid(_))));
    }

    #[test]
    fn thumbnails_keep_aspect() {
        let d = Decoded { width: 720, height: 360, rgba: solid(720, 360, [9, 9, 9, 255]) };
        let t = thumbnail_png(&d).unwrap().unwrap();
        let back = decode(ImageSource::Png, &t).unwrap();
        assert_eq!((back.width, back.height), (360, 180));
        let small = Decoded { width: 10, height: 360, rgba: solid(10, 360, [1, 1, 1, 255]) };
        assert_eq!(thumbnail_png(&small).unwrap(), None);
    }
}
