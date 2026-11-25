//! Source-app icons: one 64×64 PNG per exe, extracted once and cached in memory (the store
//! keeps a copy per app via `Store::set_app_icon`). Port of Liquid Tools
//! `screenshot/mod.rs::app_icon_from_exe` (HICON → `GetIconInfo` → `GetDIBits` → RGBA, the
//! AND mask as alpha when the colour bitmap has none), with a sized extraction first:
//! - https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shdefextracticonw
//!   (`nIconSize` LOWORD = 64; the caller destroys the icon);
//! - fallback https://learn.microsoft.com/windows/win32/api/shellapi/nf-shellapi-shgetfileinfow
//!   (`SHGFI_ICON | SHGFI_LARGEICON`; COM must be initialized on the calling thread);
//! - https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-geticoninfo (the caller
//!   deletes both bitmaps), https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-getdibits.
//! Every GDI object / icon / COM init is released on every path.

use super::win32::wide;
use base64::Engine;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use windows::core::PCWSTR;
use windows::Win32::Foundation::S_OK;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
use windows::Win32::UI::Shell::{SHDefExtractIconW, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

/// Side of the stored icon PNG (crisp at 200 % for the 20–32 px glyphs of the UI).
pub const ICON_SIZE: u32 = 64;
const CACHE_MAX: usize = 512;

/// One extracted icon: the PNG (for `Store::set_app_icon`) and its data URL, encoded once.
pub struct IconData {
    pub png: Vec<u8>,
    pub data_url: String,
}

#[derive(Clone)]
enum Entry {
    /// Extraction queued on the background icon thread.
    Pending,
    /// Done; `None` = the exe has no usable icon (remembered: never extracted again).
    Ready(Option<Arc<IconData>>),
}

type Cache = Mutex<HashMap<String, Entry>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Extract `key` (lower-cased path) and store the result; returns it.
fn extract_and_store(key: &str) -> Option<Arc<IconData>> {
    let data = extract_png(key).map(|png| {
        let data_url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
        Arc::new(IconData { png, data_url })
    });
    let mut c = cache().lock();
    if c.len() >= CACHE_MAX && !c.contains_key(key) {
        c.retain(|_, e| matches!(e, Entry::Pending)); // keep in-flight markers consistent
    }
    c.insert(key.to_string(), Entry::Ready(data.clone()));
    data
}

/// Background extraction for the non-blocking path: one long-lived thread, requests
/// de-duplicated by the `Pending` marker.
fn queue(key: String) {
    static TX: OnceLock<Option<Mutex<std::sync::mpsc::Sender<String>>>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::Builder::new()
            .name("liqlip-clip-icons".into())
            .spawn(move || {
                for key in rx {
                    let still_pending = matches!(cache().lock().get(&key), Some(Entry::Pending));
                    if still_pending && std::panic::catch_unwind(|| extract_and_store(&key)).is_err() {
                        cache().lock().insert(key, Entry::Ready(None));
                    }
                }
            })
            .ok()
            .map(|_| Mutex::new(tx))
    });
    let sent = tx.as_ref().is_some_and(|tx| tx.lock().send(key.clone()).is_ok());
    if !sent {
        // No background thread: drop the marker so a blocking caller can still extract.
        cache().lock().remove(&key);
    }
}

/// Icon of `exe` (full path), extracted on first use — **blocking** on a miss (capture worker,
/// `apps_running`). Never extracts the same exe twice once a result is cached.
pub fn icon_for(exe: &str) -> Option<Arc<IconData>> {
    let key = exe.to_lowercase();
    if let Some(Entry::Ready(hit)) = cache().lock().get(&key) {
        return hit.clone();
    }
    extract_and_store(&key)
}

/// PNG icon of `exe` (blocking on a miss, see [`icon_for`]).
pub fn png_for(exe: &str) -> Option<Arc<IconData>> {
    icon_for(exe)
}

/// `data:image/png;base64,…` (blocking on a miss) — `RunningApp.iconUrl`.
pub fn data_url(exe: &str) -> Option<String> {
    icon_for(exe).map(|d| d.data_url.clone())
}

/// `data:image/png;base64,…` **without blocking**: a cache hit (a map lookup + a copy of the
/// pre-encoded URL), or `None` on a miss while the icon is extracted in the background — the
/// next call returns it. Used on the shelf summon path (`TargetInfo.iconUrl`).
pub fn data_url_cached(exe: &str) -> Option<String> {
    let key = exe.to_lowercase();
    {
        let mut c = cache().lock();
        match c.get(&key) {
            Some(Entry::Ready(hit)) => return hit.as_ref().map(|d| d.data_url.clone()),
            Some(Entry::Pending) => return None,
            None => {
                c.insert(key.clone(), Entry::Pending);
            }
        }
    }
    queue(key);
    None
}

/// Balanced COM init for this call (SHGetFileInfoW requires it). `S_FALSE` (already
/// initialized) also needs `CoUninitialize`; `RPC_E_CHANGED_MODE` (thread is MTA) does not.
/// https://learn.microsoft.com/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex
struct ComScope(bool);

impl ComScope {
    fn enter() -> Self {
        // SAFETY: no preconditions; result checked.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        // `is_ok()` covers S_OK and S_FALSE; RPC_E_CHANGED_MODE (an MTA thread) is a failure.
        ComScope(hr.is_ok())
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful CoInitializeEx on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// Owns an HICON.
struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: icon created for us by SHDefExtractIconW / SHGetFileInfoW.
        let _ = unsafe { DestroyIcon(self.0) };
    }
}

fn extract_png(exe: &str) -> Option<Vec<u8>> {
    let _com = ComScope::enter();
    let path = wide(exe);
    let icon = sized_icon(&path).or_else(|| shell_icon(&path))?;
    let (w, h, rgba) = rasterize(icon.0)?;
    drop(icon);
    let img = image::RgbaImage::from_raw(w, h, rgba)?;
    let img = if (w, h) == (ICON_SIZE, ICON_SIZE) {
        img
    } else {
        image::imageops::resize(&img, ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Triangle)
    };
    super::imaging::encode_png(ICON_SIZE, ICON_SIZE, img.as_raw()).ok()
}

fn sized_icon(path: &[u16]) -> Option<Icon> {
    let mut large = HICON::default();
    // SAFETY: `path` is NUL-terminated; `large` receives an icon we must destroy.
    let hr = unsafe { SHDefExtractIconW(PCWSTR(path.as_ptr()), 0, 0, Some(&mut large as *mut HICON), None, ICON_SIZE) };
    if hr == S_OK && !large.is_invalid() {
        Some(Icon(large))
    } else {
        if !large.is_invalid() {
            drop(Icon(large));
        }
        None
    }
}

fn shell_icon(path: &[u16]) -> Option<Icon> {
    let mut sfi = SHFILEINFOW::default();
    // SAFETY: `sfi` is sized as passed; `path` NUL-terminated; COM initialized (ComScope).
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut sfi as *mut SHFILEINFOW),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    (ok != 0 && !sfi.hIcon.is_invalid()).then(|| Icon(sfi.hIcon))
}

/// HICON → (w, h, RGBA8). Frees both bitmaps from `GetIconInfo` and the DC.
fn rasterize(hicon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut ii = ICONINFO::default();
    // SAFETY: valid icon; on success we own hbmColor/hbmMask.
    unsafe { GetIconInfo(hicon, &mut ii) }.ok()?;
    struct Bitmaps(HGDIOBJ, HGDIOBJ);
    impl Drop for Bitmaps {
        fn drop(&mut self) {
            // SAFETY: bitmaps returned by GetIconInfo; deleting a null handle is a no-op.
            unsafe {
                let _ = DeleteObject(self.0);
                let _ = DeleteObject(self.1);
            }
        }
    }
    let _bitmaps = Bitmaps(HGDIOBJ(ii.hbmColor.0), HGDIOBJ(ii.hbmMask.0));
    if ii.hbmColor.is_invalid() {
        return None; // monochrome icon
    }
    let mut bm = BITMAP::default();
    // SAFETY: `bm` is a BITMAP of the size passed.
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(ii.hbmColor.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut core::ffi::c_void),
        )
    };
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    if got == 0 || w <= 0 || h <= 0 || w > 512 || h > 512 {
        return None;
    }
    // SAFETY: memory DC compatible with the screen; deleted below.
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return None;
    }
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let px = (w as usize) * (h as usize);
    let mut color = vec![0u8; px * 4];
    // SAFETY: `color` holds w×h 32-bpp pixels as described by `bmi`.
    let lines = unsafe {
        GetDIBits(dc, ii.hbmColor, 0, h as u32, Some(color.as_mut_ptr().cast()), &mut bmi, DIB_RGB_COLORS)
    };
    if lines == 0 {
        // SAFETY: DC created above.
        let _ = unsafe { DeleteDC(dc) };
        return None;
    }
    for p in color.chunks_exact_mut(4) {
        p.swap(0, 2); // BGRA → RGBA
    }
    if !color.chunks_exact(4).any(|p| p[3] != 0) {
        // No per-pixel alpha: the 1-bpp AND mask decides (0 = opaque).
        let mut mask = vec![0u8; px * 4];
        let mlines = if ii.hbmMask.is_invalid() {
            0
        } else {
            // SAFETY: same geometry, read as 32 bpp.
            unsafe { GetDIBits(dc, ii.hbmMask, 0, h as u32, Some(mask.as_mut_ptr().cast()), &mut bmi, DIB_RGB_COLORS) }
        };
        for (c, m) in color.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
            c[3] = if mlines == 0 || m[0] == 0 { 255 } else { 0 };
        }
    }
    // SAFETY: DC created above.
    let _ = unsafe { DeleteDC(dc) };
    Some((w as u32, h as u32, color))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_icon_is_a_64px_png_and_cached() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let explorer = format!("{windir}\\explorer.exe");
        if !std::path::Path::new(&explorer).exists() {
            return;
        }
        let icon = png_for(&explorer).expect("explorer.exe has an icon");
        let d = super::super::imaging::decode(super::super::imaging::ImageSource::Png, &icon.png).unwrap();
        assert_eq!((d.width, d.height), (ICON_SIZE, ICON_SIZE));
        assert!(d.rgba.chunks(4).any(|p| p[3] > 0), "not fully transparent");
        let again = png_for(&explorer.to_uppercase()).unwrap();
        assert!(Arc::ptr_eq(&icon, &again), "served from the cache");
        assert!(data_url(&explorer).unwrap().starts_with("data:image/png;base64,"));
        assert_eq!(data_url_cached(&explorer.to_lowercase()), Some(icon.data_url.clone()));
    }

    #[test]
    fn missing_exe_has_no_icon_and_is_remembered() {
        // SHGetFileInfoW may still return a generic icon for a non-existent .exe path; either
        // way the call must not fail hard and the result is cached.
        let first = png_for("c:\\definitely\\not\\here\\nothing.exe");
        let t = std::time::Instant::now();
        let second = png_for("c:\\definitely\\not\\here\\nothing.exe");
        assert!(t.elapsed() < std::time::Duration::from_millis(5), "no second extraction");
        assert_eq!(first.is_some(), second.is_some());
    }

    /// The shelf summon path: never blocks on extraction; a miss queues it in the background
    /// and a later call returns the cached data URL.
    #[test]
    fn cached_data_url_never_blocks() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let exe = format!("{windir}\\System32\\cmd.exe").to_lowercase();
        if !std::path::Path::new(&exe).exists() {
            return;
        }
        let t = std::time::Instant::now();
        let first = data_url_cached(&exe);
        assert!(t.elapsed() < std::time::Duration::from_millis(5), "summon path blocked {:?}", t.elapsed());
        assert!(first.is_none(), "first call is a miss");
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut got = None;
        while got.is_none() && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(10));
            got = data_url_cached(&exe);
        }
        let url = got.expect("extracted in the background");
        assert!(url.starts_with("data:image/png;base64,"));
        let t = std::time::Instant::now();
        assert_eq!(data_url_cached(&exe), Some(url));
        assert!(t.elapsed() < std::time::Duration::from_millis(2));

        // A file without an icon: the failure is cached too, no re-extraction.
        let missing = "c:\\definitely\\not\\here\\summon-target.exe";
        assert_eq!(data_url_cached(missing), None);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while matches!(cache().lock().get(missing), Some(Entry::Pending)) && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(cache().lock().get(missing), Some(Entry::Ready(_))));
    }
}
