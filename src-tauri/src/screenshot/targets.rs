//! Window tree under the overlay (owner: W6-A; ARCHITECTURE §18.3) — port of Liquid Tools
//! `screenshot/capture.rs:54-104, 239-354` with its limits.
//!
//! `EnumWindows` walks top-level windows in z-order (topmost first), so the first rectangle
//! under the cursor is the window the user sees. Skipped (the ShareX rules LT used): windows
//! of this process (the overlay, shelf, HUD, QAO, editor, pins, settings, dialogs),
//! invisible, minimized, `WS_EX_TOOLWINDOW` + `WS_EX_NOACTIVATE` helpers (tooltips, palettes),
//! DWM-cloaked ones (other virtual desktops, suspended UWP). A window's rectangle is its DWM
//! extended frame bounds (no invisible resize border), clipped to the monitor, at least 8 px a
//! side, in physical px of the monitor's frame. Children (`EnumChildWindows`, which also walks
//! grandchildren) give the inner surfaces: Chromium page viewports
//! (`Chrome_RenderWidgetHostHWND`, client rect) first, then the HWND tree, then the client
//! area when it differs from the frame. Limits: 2048 nodes and 35 ms per window, 32 levels,
//! 120 ms for all children (the window under the cursor, then Chromium windows, go first).
//! The tree is read BEFORE the overlay shows (Chromium may hide renderer HWNDs it thinks are
//! occluded).
//!
//! - https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumwindows
//! - https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumchildwindows
//! - https://learn.microsoft.com/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
//!   (`DWMWA_CLOAKED`, `DWMWA_EXTENDED_FRAME_BOUNDS`)

use super::grab::Monitor;
use crate::model::WinRect;
use std::time::{Duration, Instant};

pub(crate) const MAX_NODES: usize = 2048;
pub(crate) const MAX_DEPTH: usize = 32;
pub(crate) const PER_WINDOW: Duration = Duration::from_millis(35);
pub(crate) const CHILD_BUDGET: Duration = Duration::from_millis(120);
/// A rectangle must keep more than this many px a side after clipping.
const MIN_SIDE: i32 = 8;

/// `(left, top, right, bottom)` in physical screen px.
pub(crate) type Ltrb = (i32, i32, i32, i32);

/// A screen rectangle → monitor-local (`ox`, `oy` = the monitor's origin), clipped to `clip`;
/// `None` when no more than 8 px a side remain.
pub(crate) fn local_region(rc: Ltrb, clip: &WinRect, ox: i32, oy: i32) -> Option<WinRect> {
    let x = (rc.0 - ox).max(clip.x);
    let y = (rc.1 - oy).max(clip.y);
    let right = (rc.2 - ox).min(clip.x + clip.w);
    let bottom = (rc.3 - oy).min(clip.y + clip.h);
    (right - x > MIN_SIDE && bottom - y > MIN_SIDE).then(|| WinRect { x, y, w: right - x, h: bottom - y, children: Vec::new() })
}

/// One visible child window as `EnumChildWindows` reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub handle: isize,
    pub parent: isize,
    pub rect: Ltrb,
    /// A Chromium page viewport (its client rect is the page, emulation included).
    pub renderer: bool,
}

/// The HWND tree under `parent`, clipped level by level; a helper without a usable rectangle
/// hands its own children up (they may still be visible surfaces). At most [`MAX_DEPTH`] levels.
pub(crate) fn tree(nodes: &[Node], parent: isize, clip: &WinRect, ox: i32, oy: i32, depth: usize) -> Vec<WinRect> {
    if depth >= MAX_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for n in nodes.iter().filter(|n| n.parent == parent) {
        match local_region(n.rect, clip, ox, oy) {
            Some(mut region) => {
                region.children = tree(nodes, n.handle, &region, ox, oy, depth + 1);
                out.push(region);
            }
            None => out.extend(tree(nodes, n.handle, clip, ox, oy, depth + 1)),
        }
    }
    out
}

/// The children of one top-level window `root` (clip = its own rectangle): Chromium renderers
/// first (independent candidates — tiny intermediate hosts must not clip them away), then the
/// tree, then the client area when it is not the whole frame.
pub(crate) fn children(nodes: &[Node], root: isize, client: Option<Ltrb>, clip: &WinRect, ox: i32, oy: i32) -> Vec<WinRect> {
    let mut out: Vec<WinRect> = nodes.iter().filter(|n| n.renderer).filter_map(|n| local_region(n.rect, clip, ox, oy)).collect();
    out.extend(tree(nodes, root, clip, ox, oy, 0));
    if let Some(c) = client.and_then(|r| local_region(r, clip, ox, oy)) {
        if (c.x, c.y, c.w, c.h) != (clip.x, clip.y, clip.w, clip.h) {
            out.push(c);
        }
    }
    out
}

/// Which top-level windows get their children first within [`CHILD_BUDGET`]: the one under
/// the cursor (monitor-local point), then Chromium windows, then the rest — z-order kept inside
/// each group.
pub(crate) fn scan_order(tops: &[WinRect], cursor: Option<(i32, i32)>, chromium: &[bool]) -> Vec<usize> {
    let under = cursor.and_then(|(cx, cy)| tops.iter().position(|r| cx >= r.x && cx < r.x + r.w && cy >= r.y && cy < r.y + r.h));
    let mut order: Vec<usize> = (0..tops.len()).collect();
    order.sort_by_key(|&i| if Some(i) == under { 0 } else if chromium.get(i).copied().unwrap_or(false) { 1 } else { 2 });
    order
}

/// The child walk of one window goes on while under the node cap and its time slice.
pub(crate) fn keep_collecting(count: usize, now: Instant, until: Instant) -> bool {
    count < MAX_NODES && now < until
}

#[cfg(windows)]
pub(crate) use win::enumerate;

#[cfg(windows)]
mod win {
    use super::{children, keep_collecting, local_region, scan_order, Ltrb, Monitor, Node, CHILD_BUDGET, PER_WINDOW};
    use crate::model::WinRect;
    use std::time::Instant;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetClassNameW, GetClientRect, GetParent, GetWindowLongW, GetWindowRect,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    fn class_name(h: HWND) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: the wrapper passes the buffer length.
        let n = unsafe { GetClassNameW(h, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    fn ltrb(rc: RECT) -> Ltrb {
        (rc.left, rc.top, rc.right, rc.bottom)
    }

    /// Client area in screen coordinates.
    fn client_screen_rect(h: HWND) -> Option<Ltrb> {
        // SAFETY: plain window queries with out-pointers to locals.
        unsafe {
            let mut rc = RECT::default();
            GetClientRect(h, &mut rc).ok()?;
            let mut a = POINT { x: rc.left, y: rc.top };
            let mut b = POINT { x: rc.right, y: rc.bottom };
            if !ClientToScreen(h, &mut a).as_bool() || !ClientToScreen(h, &mut b).as_bool() {
                return None;
            }
            Some((a.x, a.y, b.x, b.y))
        }
    }

    struct Tops {
        clip: WinRect,
        ox: i32,
        oy: i32,
        own_pid: u32,
        rects: Vec<WinRect>,
        handles: Vec<HWND>,
    }

    unsafe extern "system" fn top(h: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Tops` passed by `enumerate`, alive for the enumeration.
        let ctx = unsafe { &mut *(data.0 as *mut Tops) };
        // SAFETY: plain queries on a handle EnumWindows just handed us.
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == ctx.own_pid || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
                return BOOL(1);
            }
            let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_NOACTIVATE.0 != 0 {
                return BOOL(1);
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut core::ffi::c_void, 4);
            if cloaked != 0 {
                return BOOL(1);
            }
        }
        let Some(bounds) = super::super::grab::window_bounds(h.0 as isize) else { return BOOL(1) };
        if let Some(region) = local_region(bounds, &ctx.clip, ctx.ox, ctx.oy) {
            ctx.rects.push(region);
            ctx.handles.push(h);
        }
        BOOL(1)
    }

    struct Scan {
        nodes: Vec<Node>,
        until: Instant,
    }

    unsafe extern "system" fn child(h: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Scan` passed by `child_regions`, alive for the enumeration.
        let scan = unsafe { &mut *(data.0 as *mut Scan) };
        if !keep_collecting(scan.nodes.len(), Instant::now(), scan.until) {
            return BOOL(0); // stop
        }
        // SAFETY: plain queries on a handle EnumChildWindows just handed us.
        unsafe {
            if !IsWindowVisible(h).as_bool() {
                return BOOL(1);
            }
            let mut rc = RECT::default();
            if GetWindowRect(h, &mut rc).is_err() {
                return BOOL(1);
            }
            let renderer = class_name(h) == "Chrome_RenderWidgetHostHWND";
            let rect = if renderer { client_screen_rect(h).unwrap_or(ltrb(rc)) } else { ltrb(rc) };
            let parent = GetParent(h).map(|p| p.0 as isize).unwrap_or(0);
            scan.nodes.push(Node { handle: h.0 as isize, parent, rect, renderer });
        }
        BOOL(1)
    }

    fn child_regions(root: HWND, clip: &WinRect, ox: i32, oy: i32, deadline: Instant) -> Vec<WinRect> {
        let mut scan = Scan { nodes: Vec::new(), until: deadline.min(Instant::now() + PER_WINDOW) };
        // SAFETY: callback + pointer to a local that outlives the synchronous enumeration.
        unsafe {
            let _ = EnumChildWindows(Some(root), Some(child), LPARAM(&mut scan as *mut Scan as isize));
        }
        children(&scan.nodes, root.0 as isize, client_screen_rect(root), clip, ox, oy)
    }

    /// The window tree of one monitor; `cursor` (screen px) puts the window under it first in
    /// the child budget.
    pub fn enumerate(m: &Monitor, cursor: Option<(i32, i32)>) -> Vec<WinRect> {
        let mut ctx = Tops {
            clip: WinRect { x: 0, y: 0, w: m.width, h: m.height, children: Vec::new() },
            ox: m.x,
            oy: m.y,
            // SAFETY: no arguments.
            own_pid: unsafe { GetCurrentProcessId() },
            rects: Vec::new(),
            handles: Vec::new(),
        };
        // SAFETY: callback + pointer to a local that outlives the synchronous enumeration.
        unsafe {
            let _ = EnumWindows(Some(top), LPARAM(&mut ctx as *mut Tops as isize));
        }
        let chromium: Vec<bool> = ctx.handles.iter().map(|&h| class_name(h).starts_with("Chrome_WidgetWin_")).collect();
        let local = cursor.map(|(x, y)| (x - m.x, y - m.y));
        let deadline = Instant::now() + CHILD_BUDGET;
        for i in scan_order(&ctx.rects, local, &chromium) {
            if Instant::now() >= deadline {
                break;
            }
            let kids = child_regions(ctx.handles[i], &ctx.rects[i], m.x, m.y, deadline);
            ctx.rects[i].children = kids;
        }
        ctx.rects
    }
}

#[cfg(not(windows))]
pub(crate) fn enumerate(_m: &Monitor, _cursor: Option<(i32, i32)>) -> Vec<WinRect> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wr(x: i32, y: i32, w: i32, h: i32) -> WinRect {
        WinRect { x, y, w, h, children: Vec::new() }
    }

    fn node(handle: isize, parent: isize, rect: Ltrb) -> Node {
        Node { handle, parent, rect, renderer: false }
    }

    #[test]
    fn regions_are_monitor_local_clipped_and_not_tiny() {
        let mon = wr(0, 0, 1920, 1080);
        // A window on a monitor at (1920, 0): screen 2000..2400 → local 80..480.
        assert_eq!(local_region((2000, 100, 2400, 500), &mon, 1920, 0), Some(wr(80, 100, 400, 400)));
        // Hanging off the monitor's left and top edges.
        assert_eq!(local_region((1800, -50, 2100, 300), &mon, 1920, 0), Some(wr(0, 0, 180, 300)));
        // 8 px or less left after clipping → not a target.
        assert_eq!(local_region((1912, 0, 1928, 300), &mon, 1920, 0), None);
        assert_eq!(local_region((0, 0, 9, 9), &mon, 0, 0), Some(wr(0, 0, 9, 9)));
        assert_eq!(local_region((0, 0, 8, 100), &mon, 0, 0), None);
    }

    #[test]
    fn the_tree_nests_clips_and_lifts_children_of_helpers() {
        let root = wr(0, 0, 1000, 800);
        let nodes = vec![
            node(2, 1, (0, 0, 1000, 100)),   // toolbar
            node(3, 1, (0, 100, 1000, 800)), // body
            node(4, 3, (10, 110, 2000, 300)), // panel, wider than its parent → clipped to it
            node(5, 1, (0, 0, 4, 4)),        // a tiny helper …
            node(6, 5, (500, 500, 700, 700)), // … whose child is lifted to the root level
        ];
        let t = tree(&nodes, 1, &root, 0, 0, 0);
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].x, t[0].y, t[0].w, t[0].h), (0, 0, 1000, 100));
        assert_eq!(t[1].children, vec![wr(10, 110, 990, 190)]);
        assert_eq!((t[2].x, t[2].y, t[2].w, t[2].h), (500, 500, 200, 200));
    }

    #[test]
    fn the_tree_stops_at_32_levels() {
        let nodes: Vec<Node> = (0..40).map(|i| node(i + 2, i + 1, (0, 0, 500, 500))).collect();
        let mut depth = 0;
        let mut level = tree(&nodes, 1, &wr(0, 0, 500, 500), 0, 0, 0);
        while let Some(first) = level.into_iter().next() {
            depth += 1;
            level = first.children;
        }
        assert_eq!(depth, MAX_DEPTH);
    }

    #[test]
    fn renderers_come_first_and_the_client_area_last() {
        let frame = wr(100, 100, 800, 600);
        let mut viewport = node(9, 8, (120, 200, 880, 690));
        viewport.renderer = true;
        let nodes = vec![node(8, 1, (110, 190, 890, 700)), viewport];
        let c = children(&nodes, 1, Some((108, 130, 892, 700)), &frame, 0, 0);
        assert_eq!((c[0].x, c[0].y, c[0].w, c[0].h), (120, 200, 760, 490), "the page viewport, flat, first");
        assert_eq!((c[1].x, c[1].y), (110, 190), "then the HWND tree");
        assert_eq!(c[1].children.len(), 1);
        assert_eq!((c[2].x, c[2].y, c[2].w, c[2].h), (108, 130, 784, 570), "then the client area");
        // A client area equal to the frame adds nothing.
        let same = children(&[], 1, Some((100, 100, 900, 700)), &frame, 0, 0);
        assert!(same.is_empty());
    }

    #[test]
    fn children_are_read_for_the_window_under_the_cursor_first() {
        let tops = vec![wr(0, 0, 100, 100), wr(50, 50, 500, 500), wr(0, 0, 1920, 1080), wr(900, 0, 100, 100)];
        let chromium = [false, false, true, true];
        assert_eq!(scan_order(&tops, Some((300, 300)), &chromium), vec![1, 2, 3, 0]);
        // The topmost window wins where they overlap.
        assert_eq!(scan_order(&tops, Some((60, 60)), &chromium), vec![0, 2, 3, 1]);
        assert_eq!(scan_order(&tops, None, &chromium), vec![2, 3, 0, 1]);
    }

    #[test]
    fn child_walks_are_capped() {
        let now = Instant::now();
        assert!(keep_collecting(0, now, now + PER_WINDOW));
        assert!(!keep_collecting(MAX_NODES, now, now + PER_WINDOW));
        assert!(!keep_collecting(1, now + PER_WINDOW, now + PER_WINDOW));
    }
}
