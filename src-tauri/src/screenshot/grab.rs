//! Screen freeze (owner: W6-A; ARCHITECTURE §18.3) — port of Liquid Tools
//! `screenshot/capture.rs:150-230` (GDI) for EVERY monitor, plus the monitor list.
//!
//! The process is Per-Monitor-V2 DPI aware (Tauri's manifest), so every Win32 coordinate here
//! is a PHYSICAL pixel of the virtual screen. Per monitor: `GetDC(NULL)` → `BitBlt(SRCCOPY |
//! CAPTUREBLT)` of its rectangle into a compatible bitmap → `GetDIBits` 32 bpp top-down straight
//! into the frame's BMP buffer (`frame.rs`: one buffer per monitor, no second copy). A partial
//! `GetDIBits` is an error (it would leave black rows). Limits of GDI capture (research §4):
//! HDR and DRM-protected content come out black, the cursor is not in the frame.
//!
//! - https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumdisplaymonitors
//! - https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getmonitorinfow
//!   (`MONITORINFOEXW.szDevice` — the same device name Tauri reports as `Monitor::name`)
//! - https://learn.microsoft.com/windows/win32/api/shellscalingapi/nf-shellscalingapi-getdpiformonitor
//! - https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-bitblt ("CAPTUREBLT —
//!   Includes any windows that are layered on top of your window in the resulting image")
//! - https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-getdibits ("The bitmap
//!   identified by the hbmp parameter must not be selected into a device context when the
//!   application calls this function")
//! - https://learn.microsoft.com/windows/win32/api/dwmapi/nf-dwmapi-dwmflush

use super::frame::Frame;

/// One monitor: physical px of the virtual screen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Monitor {
    /// Device name (`\\.\DISPLAY1`), = `MonitorInfo.id` / Tauri's `Monitor::name`.
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Effective DPI / 96.
    pub scale: f64,
}

impl Monitor {
    fn contains(&self, (px, py): (i32, i32)) -> bool {
        px >= self.x && py >= self.y && i64::from(px) < i64::from(self.x) + i64::from(self.width) && i64::from(py) < i64::from(self.y) + i64::from(self.height)
    }

    /// Squared distance from a point to the monitor's rectangle (0 inside).
    fn distance2(&self, (px, py): (i32, i32)) -> i64 {
        let gap = |p: i32, lo: i32, len: i32| {
            let (p, lo, hi) = (i64::from(p), i64::from(lo), i64::from(lo) + i64::from(len) - 1);
            if p < lo { lo - p } else if p > hi { p - hi } else { 0 }
        };
        let (dx, dy) = (gap(px, self.x, self.width), gap(py, self.y, self.height));
        dx * dx + dy * dy
    }
}

/// The monitor that holds a point; between monitors (or off the desktop) the nearest one, like
/// `MonitorFromPoint(MONITOR_DEFAULTTONEAREST)`. `None` only for an empty list.
pub(crate) fn monitor_at(monitors: &[Monitor], p: (i32, i32)) -> Option<usize> {
    monitors.iter().position(|m| m.contains(p)).or_else(|| {
        monitors.iter().enumerate().min_by_key(|(_, m)| m.distance2(p)).map(|(i, _)| i)
    })
}

/// Monitors in a stable order (left to right, then top to bottom) — the `index` of a session.
pub(crate) fn sorted(mut monitors: Vec<Monitor>) -> Vec<Monitor> {
    monitors.retain(|m| m.width > 0 && m.height > 0);
    monitors.sort_by_key(|m| (m.x, m.y));
    monitors
}

#[cfg(windows)]
pub(crate) use win::{cursor_pos, dwm_flush, grab, monitors, window_bounds};

#[cfg(windows)]
mod win {
    use super::{sorted, Frame, Monitor};
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
    use windows::Win32::Graphics::Dwm::{DwmFlush, DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC,
        GetDIBits, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, HDC, HGDIOBJ, HMONITOR, MONITORINFO, MONITORINFOEXW, SRCCOPY,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};

    unsafe extern "system" fn collect(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Vec<HMONITOR>` passed by `monitors`, alive for the
        // synchronous enumeration.
        let list = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
        list.push(hmon);
        BOOL(1)
    }

    /// Every display monitor (thread-safe Win32 reads; no Tauri main-thread round trip).
    pub fn monitors() -> Vec<Monitor> {
        let mut handles: Vec<HMONITOR> = Vec::new();
        // SAFETY: callback + pointer to a local that outlives the synchronous enumeration.
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut handles as *mut _ as isize));
        }
        sorted(handles.into_iter().filter_map(describe).collect())
    }

    fn describe(h: HMONITOR) -> Option<Monitor> {
        let mut mi = MONITORINFOEXW::default();
        mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: "set cbSize to sizeof(MONITORINFOEX)" and pass it as a MONITORINFO pointer
        // (GetMonitorInfoW docs); `mi` outlives the call.
        if !unsafe { GetMonitorInfoW(h, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
            return None;
        }
        let rc = mi.monitorInfo.rcMonitor;
        let len = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(mi.szDevice.len());
        let (mut dx, mut dy) = (0u32, 0u32);
        // SAFETY: out-pointers to locals.
        let dpi_ok = unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) }.is_ok() && dx > 0;
        Some(Monitor {
            id: String::from_utf16_lossy(&mi.szDevice[..len]),
            x: rc.left,
            y: rc.top,
            width: rc.right - rc.left,
            height: rc.bottom - rc.top,
            scale: if dpi_ok { f64::from(dx) / 96.0 } else { 1.0 },
        })
    }

    /// Physical cursor position.
    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getcursorpos
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut pt = POINT::default();
        // SAFETY: `pt` outlives the call.
        unsafe { GetCursorPos(&mut pt) }.ok()?;
        Some((pt.x, pt.y))
    }

    /// Wait for the next DWM composition, so a window hidden just before is gone from the
    /// screen the BitBlt reads. Errors are ignored (no composition = nothing to wait for).
    pub fn dwm_flush() {
        // SAFETY: no arguments.
        let _ = unsafe { DwmFlush() };
    }

    /// Visible bounds of a top-level window: `DWMWA_EXTENDED_FRAME_BOUNDS` (without the
    /// invisible resize border), else `GetWindowRect`. `(left, top, right, bottom)`.
    /// https://learn.microsoft.com/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
    pub fn window_bounds(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
        let h = HWND(hwnd as *mut core::ffi::c_void);
        let mut rc = RECT::default();
        // SAFETY: `rc` is a RECT of the size passed; an invalid handle just fails.
        let ok = unsafe {
            DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut rc as *mut RECT as *mut core::ffi::c_void, std::mem::size_of::<RECT>() as u32)
                .is_ok()
                || GetWindowRect(h, &mut rc).is_ok()
        };
        (ok && rc.right > rc.left && rc.bottom > rc.top).then_some((rc.left, rc.top, rc.right, rc.bottom))
    }

    /// Freeze one monitor (see the module doc). Every GDI object is released on every path.
    pub fn grab(m: &Monitor) -> Result<Frame, String> {
        let (w, h) = (m.width, m.height);
        let mut frame = Frame::new(w.max(0) as u32, h.max(0) as u32).ok_or_else(|| format!("failed: monitor size {w}×{h}"))?;
        // SAFETY: standard GDI sequence; every handle is checked, deselected before GetDIBits
        // and released below; the destination buffer holds exactly w*h*4 bytes.
        let (blt, lines) = unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err("failed: GetDC(screen)".into());
            }
            let mem = CreateCompatibleDC(Some(screen));
            if mem.is_invalid() {
                ReleaseDC(None, screen);
                return Err("failed: CreateCompatibleDC".into());
            }
            let bmp = CreateCompatibleBitmap(screen, w, h);
            if bmp.is_invalid() {
                let _ = DeleteDC(mem);
                ReleaseDC(None, screen);
                return Err("failed: CreateCompatibleBitmap".into());
            }
            let old = SelectObject(mem, HGDIOBJ(bmp.0));
            let blt = BitBlt(mem, 0, 0, w, h, Some(screen), m.x, m.y, SRCCOPY | CAPTUREBLT);
            let _ = SelectObject(mem, old);
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down rows, as the BMP header says
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let lines = if blt.is_ok() {
                GetDIBits(mem, bmp, 0, h as u32, Some(frame.pixels_mut().as_mut_ptr().cast()), &mut bmi, DIB_RGB_COLORS)
            } else {
                0
            };
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            (blt, lines)
        };
        blt.map_err(|e| format!("failed: BitBlt: {e}"))?;
        if lines != h {
            return Err(format!("failed: GetDIBits copied {lines} of {h} rows"));
        }
        frame.make_opaque();
        Ok(frame)
    }
}

#[cfg(not(windows))]
pub(crate) fn monitors() -> Vec<Monitor> {
    Vec::new()
}
#[cfg(not(windows))]
pub(crate) fn cursor_pos() -> Option<(i32, i32)> {
    None
}
#[cfg(not(windows))]
pub(crate) fn dwm_flush() {}
#[cfg(not(windows))]
pub(crate) fn window_bounds(_hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    None
}
#[cfg(not(windows))]
pub(crate) fn grab(_m: &Monitor) -> Result<Frame, String> {
    Err("failed: screen capture is Windows-only".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(id: &str, x: i32, y: i32, w: i32, h: i32) -> Monitor {
        Monitor { id: id.into(), x, y, width: w, height: h, scale: 1.0 }
    }

    #[test]
    fn the_monitor_under_a_point_else_the_nearest() {
        // Primary 1920×1080 at 0,0; a 4K monitor to its left, lower; a gap below the primary.
        let ms = vec![mon("L", -3840, 200, 3840, 2160), mon("P", 0, 0, 1920, 1080)];
        assert_eq!(monitor_at(&ms, (100, 100)), Some(1));
        assert_eq!(monitor_at(&ms, (-1, 300)), Some(0), "the left edge pixel");
        assert_eq!(monitor_at(&ms, (0, 0)), Some(1));
        assert_eq!(monitor_at(&ms, (1919, 1079)), Some(1));
        assert_eq!(monitor_at(&ms, (1920, 500)), Some(1), "right of everything → nearest");
        // 421 px below P, but 11 px right of L's last column and inside its rows → L.
        assert_eq!(monitor_at(&ms, (10, 1500)), Some(0));
        assert_eq!(monitor_at(&[], (0, 0)), None);
    }

    #[test]
    fn monitors_are_ordered_left_to_right() {
        let ms = sorted(vec![mon("B", 1920, 0, 1920, 1080), mon("Z", 0, 0, 0, 0), mon("A", -1280, 100, 1280, 1024), mon("C", 0, 0, 1920, 1080)]);
        let ids: Vec<&str> = ms.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["A", "C", "B"], "empty monitors are dropped");
    }

    /// The Win32 monitor list (no screen content is read): device names like Tauri's, sane
    /// sizes and scales. The GDI freeze itself is checked live (it needs an unlocked desktop).
    #[cfg(windows)]
    #[test]
    fn the_monitor_list_reads_device_names() {
        for m in monitors() {
            assert!(m.id.starts_with(r"\\.\"), "{}", m.id);
            assert!(m.width > 0 && m.height > 0 && m.scale >= 1.0, "{m:?}");
        }
    }
}
