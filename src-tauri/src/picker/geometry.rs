//! Shelf / HUD geometry — pure, physical pixels, unit-tested.
//!
//! Ported from Liquid Tools `screenshot/shotlib.rs` (`dock`, the strict edge trigger, the
//! keep-open rect, the two-speed poll band). All rectangles are PHYSICAL px: the process is
//! Per-Monitor-V2 DPI aware (tao `dpi.rs::become_dpi_aware` →
//! `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`), so `GetCursorPos`,
//! `GetMonitorInfoW::rcWork` (Tauri `Monitor::work_area`) and window positions share one
//! coordinate space. https://learn.microsoft.com/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows

use crate::model::{PickerLayout, Position};

/// Panel sizes in logical px (SPEC §3.1 / ARCHITECTURE §7).
const CENTER: (f64, f64) = (760.0, 640.0);
const CENTER_ROW: (f64, f64) = (760.0, 360.0);
const SIDE: (f64, f64) = (400.0, 826.0);
const BAR: (f64, f64) = (800.0, 360.0);
/// Transparent margin for `--shadow-summoned` on the non-docked sides (logical px).
pub const BLEED: f64 = 72.0;
/// Width of the strip along the docked edge where the hover poll runs fast (logical px).
const NEAR_BAND: f64 = 240.0;
/// The cursor must rest on the trigger line this long (SPEC §3.1 "~200 ms").
pub const DWELL_MS: u64 = 200;

/// HUD window (logical px). Larger than the capsule: the transparent margin holds
/// `--shadow-pop` (0 12px 38px) without clipping; the capsule is centred in it.
pub const HUD_SIZE: (f64, f64) = (440.0, 112.0); // fits "Окно администратора — нажмите Ctrl+Shift+V" (W2-F)
/// Capsule height in `src/windows/hud/Hud.svelte` (logical px).
const HUD_CAPSULE_H: f64 = 40.0;
/// Gap between the cursor hotspot and the capsule (logical px).
const HUD_GAP: f64 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, width: i32, height: i32) -> Rect {
        Rect { left, top, right: left + width, bottom: top + height }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// One monitor as Tauri reports it (`Monitor::{name, position, size, work_area, scale_factor}`).
#[derive(Clone, Debug, PartialEq)]
pub struct Mon {
    /// GDI device name (`\\.\DISPLAY1`) — what `settings.picker.monitor` stores.
    pub id: String,
    pub primary: bool,
    /// rcMonitor — the true screen edges.
    pub screen: Rect,
    /// rcWork — clear of the taskbar.
    pub work: Rect,
    pub scale: f64,
}

/// Where the docked window and its panel are.
#[derive(Clone, Debug, PartialEq)]
pub struct Dock {
    pub position: Position,
    pub center_row: bool,
    /// Window box = panel + bleed (physical).
    pub window: Rect,
    /// The visible panel inside the window (physical).
    pub panel: Rect,
    /// Monitor rect of the docked monitor (true edges for the trigger line).
    pub screen: Rect,
    pub monitor_id: String,
    pub scale: f64,
    pub bleed_px: i32,
    pub band_px: i32,
    /// The docked edge touches another monitor inside the panel's span: no hover trigger
    /// there (the cursor would cross into the neighbour, SPEC §3.1 / prompt W1-D).
    pub seam: bool,
}

fn panel_size(position: Position, center_row: bool) -> (f64, f64) {
    match position {
        Position::Center if center_row => CENTER_ROW,
        Position::Center => CENTER,
        Position::Left | Position::Right => SIDE,
        Position::Top | Position::Bottom => BAR,
    }
}

/// The monitor for a summon: `"cursor"` → under the cursor (else nearest), `"primary"`,
/// or a device name (unplugged → primary). `None` only for an empty list.
pub fn pick_monitor<'a>(mons: &'a [Mon], setting: &str, cursor: Option<(i32, i32)>) -> Option<&'a Mon> {
    let primary = || mons.iter().find(|m| m.primary).or_else(|| mons.first());
    match setting {
        "cursor" => {
            let Some((x, y)) = cursor else { return primary() };
            mons.iter().find(|m| m.screen.contains(x, y)).or_else(|| {
                // Between monitors (a gap in the layout): the nearest one.
                mons.iter().min_by_key(|m| {
                    let dx = (m.screen.left - x).max(0).max(x - (m.screen.right - 1));
                    let dy = (m.screen.top - y).max(0).max(y - (m.screen.bottom - 1));
                    i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)
                })
            })
        }
        "primary" => primary(),
        name => mons.iter().find(|m| m.id.eq_ignore_ascii_case(name)).or_else(primary),
    }
}

/// Dock geometry for `position` on `mon` (LT `dock`): the panel is flush to the docked
/// work-area edge and centred on the other axis (the centre sheet on both), clamped to the
/// work area; the window adds `BLEED` on every non-docked side (all four for the centre).
pub fn dock(position: Position, center_row: bool, mon: &Mon, all: &[Mon]) -> Dock {
    let s = mon.scale.max(0.5);
    let w = mon.work;
    let (lw, lh) = panel_size(position, center_row);
    let pw = ((lw * s).round() as i32).clamp(1, w.width().max(1));
    let ph = ((lh * s).round() as i32).clamp(1, w.height().max(1));
    let bleed = (BLEED * s).round() as i32;
    let (px, py) = match position {
        Position::Left => (w.left, w.top + (w.height() - ph) / 2),
        Position::Right => (w.right - pw, w.top + (w.height() - ph) / 2),
        Position::Top => (w.left + (w.width() - pw) / 2, w.top),
        Position::Bottom => (w.left + (w.width() - pw) / 2, w.bottom - ph),
        Position::Center => (w.left + (w.width() - pw) / 2, w.top + (w.height() - ph) / 2),
    };
    let panel = Rect::new(px, py, pw, ph);
    let window = match position {
        Position::Left => Rect::new(px, py - bleed, pw + bleed, ph + 2 * bleed),
        Position::Right => Rect::new(px - bleed, py - bleed, pw + bleed, ph + 2 * bleed),
        Position::Top => Rect::new(px - bleed, py, pw + 2 * bleed, ph + bleed),
        Position::Bottom => Rect::new(px - bleed, py - bleed, pw + 2 * bleed, ph + bleed),
        Position::Center => Rect::new(px - bleed, py - bleed, pw + 2 * bleed, ph + 2 * bleed),
    };
    let seam = edge_is_seam(position, mon, all, &panel);
    Dock {
        position,
        center_row,
        window,
        panel,
        screen: mon.screen,
        monitor_id: mon.id.clone(),
        scale: s,
        bleed_px: bleed,
        band_px: (NEAR_BAND * s).round() as i32,
        seam,
    }
}

/// Does another monitor continue past the docked edge within the panel's span?
pub fn edge_is_seam(position: Position, mon: &Mon, all: &[Mon], panel: &Rect) -> bool {
    let m = mon.screen;
    all.iter().filter(|o| o.id != mon.id).any(|o| {
        let r = o.screen;
        let overlap_y = r.top < panel.bottom && r.bottom > panel.top;
        let overlap_x = r.left < panel.right && r.right > panel.left;
        match position {
            Position::Left => r.right == m.left && overlap_y,
            Position::Right => r.left == m.right && overlap_y,
            Position::Top => r.bottom == m.top && overlap_x,
            Position::Bottom => r.top == m.bottom && overlap_x,
            Position::Center => false,
        }
    })
}

/// `picker:layout` payload: panel size and bleed in LOGICAL px of the docked monitor.
pub fn layout(d: &Dock) -> PickerLayout {
    PickerLayout {
        position: d.position,
        center_row: d.center_row,
        width: f64::from(d.panel.width()) / d.scale,
        height: f64::from(d.panel.height()) / d.scale,
        bleed: f64::from(d.bleed_px) / d.scale,
    }
}

/// The 1-px trigger line: the outermost physical pixel of the true screen edge, within the
/// panel's span (not the bleed — screen corners stay free). `GetCursorPos` clamps there
/// when the cursor is pushed into the edge.
pub fn on_trigger_line(d: &Dock, x: i32, y: i32) -> bool {
    let (s, p) = (d.screen, d.panel);
    let in_y = y >= p.top && y < p.bottom;
    let in_x = x >= p.left && x < p.right;
    match d.position {
        Position::Left => x == s.left && in_y,
        Position::Right => x == s.right - 1 && in_y,
        Position::Top => y == s.top && in_x,
        Position::Bottom => y == s.bottom - 1 && in_x,
        Position::Center => false,
    }
}

/// Close to the docked edge → poll fast (the trigger can only fire there).
pub fn near_edge(d: &Dock, x: i32, y: i32) -> bool {
    let s = d.screen;
    match d.position {
        Position::Left => x - s.left < d.band_px,
        Position::Right => s.right - x <= d.band_px,
        Position::Top => y - s.top < d.band_px,
        Position::Bottom => s.bottom - y <= d.band_px,
        Position::Center => false,
    }
}

/// A hover-opened shelf stays open while the cursor is in the window box extended to the
/// true screen edge on the docked side (with a taskbar there the trigger pixel lies below
/// the work area, and the travel up into the panel must not close it).
pub fn keep_open_rect(d: &Dock) -> Rect {
    let (s, w) = (d.screen, d.window);
    match d.position {
        Position::Left => Rect { left: s.left, ..w },
        Position::Right => Rect { right: s.right, ..w },
        Position::Top => Rect { top: s.top, ..w },
        Position::Bottom => Rect { bottom: s.bottom, ..w },
        Position::Center => w,
    }
}

/// The strict hover trigger: the cursor rests on the trigger line for `DWELL_MS` with no
/// mouse button held. Reaching the edge with a button down (dragging a window) disarms it
/// until the cursor leaves the edge; so does a close (no instant re-open under the cursor).
#[derive(Debug, Default)]
pub struct HoverTracker {
    since: Option<u64>,
    disarmed: bool,
}

impl HoverTracker {
    pub fn tick(&mut self, on_edge: bool, button_down: bool, now_ms: u64) -> bool {
        if !on_edge {
            self.disarmed = false;
            self.since = None;
            return false;
        }
        if button_down || self.disarmed {
            self.disarmed = true;
            self.since = None;
            return false;
        }
        let since = *self.since.get_or_insert(now_ms);
        if now_ms.saturating_sub(since) >= DWELL_MS {
            self.since = None;
            self.disarmed = true;
            return true;
        }
        false
    }

    /// After a close: the cursor has to leave the edge before the next trigger.
    pub fn disarm(&mut self) {
        self.disarmed = true;
        self.since = None;
    }
}

/// Mouse press outside the panel while open (one report per press).
#[derive(Debug, Default)]
pub struct ClickTracker {
    was_down: bool,
}

impl ClickTracker {
    /// Start from the current button state (a button already held when the shelf opened —
    /// the tray click — is not a click outside).
    pub fn reset(&mut self, down_now: bool) {
        self.was_down = down_now;
    }

    pub fn tick(&mut self, down: bool, inside_panel: bool) -> bool {
        let pressed = down && !self.was_down;
        self.was_down = down;
        pressed && !inside_panel
    }
}

/// HUD window rect: the capsule centred under the cursor (above it near the bottom of the
/// work area), the window kept inside the work area of the cursor's monitor.
pub fn hud_rect(cursor: (i32, i32), work: Rect, scale: f64) -> Rect {
    let s = scale.max(0.5);
    let w = ((HUD_SIZE.0 * s).round() as i32).min(work.width().max(1));
    let h = ((HUD_SIZE.1 * s).round() as i32).min(work.height().max(1));
    let gap = (HUD_GAP * s).round() as i32;
    // Transparent margin above / below the capsule inside the window.
    let pad = ((HUD_SIZE.1 - HUD_CAPSULE_H) / 2.0 * s).round() as i32;
    let (cx, cy) = cursor;
    let mut x = cx - w / 2;
    let mut y = cy + gap - pad;
    if y + h - pad > work.bottom {
        y = cy - gap + pad - h;
    }
    // The capsule stays inside the work area; the transparent margin may overhang it.
    // `min` then `max`, not `clamp`: `clamp` panics on an empty range, and an empty work area
    // reported mid-reconfiguration must not take down the main thread (W3-A).
    x = x.min(work.right - w).max(work.left);
    y = y.min(work.bottom - h + pad).max(work.top - pad);
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1920×1080 @100 %, taskbar 48 px at the bottom.
    fn fhd() -> Mon {
        Mon {
            id: r"\\.\DISPLAY1".into(),
            primary: true,
            screen: Rect::new(0, 0, 1920, 1080),
            work: Rect::new(0, 0, 1920, 1032),
            scale: 1.0,
        }
    }

    /// 3840×2160 @150 % to the right of `fhd`, taskbar 72 px.
    fn uhd() -> Mon {
        Mon {
            id: r"\\.\DISPLAY2".into(),
            primary: false,
            screen: Rect::new(1920, 0, 3840, 2160),
            work: Rect::new(1920, 0, 3840, 2088),
            scale: 1.5,
        }
    }

    #[test]
    fn center_is_centred_with_bleed_all_round() {
        let m = fhd();
        let d = dock(Position::Center, false, &m, &[m.clone()]);
        assert_eq!(d.panel, Rect::new((1920 - 760) / 2, (1032 - 640) / 2, 760, 640));
        assert_eq!(d.window, Rect::new(d.panel.left - 72, d.panel.top - 72, 760 + 144, 640 + 144));
        let row = dock(Position::Center, true, &m, &[]);
        assert_eq!(row.panel.height(), 360);
        let l = layout(&d);
        assert_eq!((l.width, l.height, l.bleed), (760.0, 640.0, 72.0));
    }

    #[test]
    fn edges_are_flush_with_bleed_on_free_sides() {
        let m = fhd();
        let all = [m.clone()];
        let left = dock(Position::Left, false, &m, &all);
        assert_eq!(left.panel.left, 0);
        assert_eq!(left.window.left, 0, "no bleed on the docked side");
        assert_eq!(left.window.width(), 400 + 72);
        assert_eq!(left.window.height(), 826 + 144);
        assert_eq!(left.panel.top, (1032 - 826) / 2);

        let right = dock(Position::Right, false, &m, &all);
        assert_eq!(right.panel.right, 1920);
        assert_eq!(right.window.right, 1920);

        let top = dock(Position::Top, false, &m, &all);
        assert_eq!(top.panel.top, 0);
        assert_eq!(top.window.top, 0);
        assert_eq!(top.window.height(), 360 + 72);

        let bottom = dock(Position::Bottom, false, &m, &all);
        assert_eq!(bottom.panel.bottom, 1032, "above the taskbar (rcWork)");
        assert_eq!(bottom.window.bottom, 1032);
        assert_eq!(bottom.panel.left, (1920 - 800) / 2);
    }

    #[test]
    fn scale_multiplies_physical_size_and_layout_stays_logical() {
        let m = uhd();
        let d = dock(Position::Right, false, &m, &[fhd(), m.clone()]);
        assert_eq!(d.panel.width(), 600);
        assert_eq!(d.panel.height(), 1239);
        assert_eq!(d.bleed_px, 108);
        assert_eq!(d.panel.right, 1920 + 3840);
        let l = layout(&d);
        assert_eq!(l.width, 400.0);
        assert!((l.height - 826.0).abs() < 0.5);
        assert_eq!(l.bleed, 72.0);
    }

    #[test]
    fn small_work_area_clamps_the_panel() {
        let m = Mon { work: Rect::new(0, 0, 1366, 720), screen: Rect::new(0, 0, 1366, 768), ..fhd() };
        let d = dock(Position::Left, false, &m, &[]);
        assert_eq!(d.panel.height(), 720);
        assert_eq!(d.panel.top, 0);
        let l = layout(&d);
        assert_eq!(l.height, 720.0);
    }

    #[test]
    fn picks_monitor_by_setting() {
        let mons = [fhd(), uhd()];
        assert_eq!(pick_monitor(&mons, "cursor", Some((2500, 300))).unwrap().id, mons[1].id);
        assert_eq!(pick_monitor(&mons, "cursor", Some((10, 10))).unwrap().id, mons[0].id);
        assert_eq!(pick_monitor(&mons, "cursor", None).unwrap().id, mons[0].id);
        // Below the shorter FHD monitor, in the gap: nearest is FHD.
        assert_eq!(pick_monitor(&mons, "cursor", Some((100, 1500))).unwrap().id, mons[0].id);
        assert_eq!(pick_monitor(&mons, "primary", Some((2500, 300))).unwrap().id, mons[0].id);
        assert_eq!(pick_monitor(&mons, r"\\.\display2", None).unwrap().id, mons[1].id);
        assert_eq!(pick_monitor(&mons, r"\\.\DISPLAY9", None).unwrap().id, mons[0].id, "unplugged → primary");
        assert!(pick_monitor(&[], "cursor", None).is_none());
    }

    #[test]
    fn seam_edges_have_no_trigger() {
        let (a, b) = (fhd(), uhd());
        let all = [a.clone(), b.clone()];
        assert!(dock(Position::Right, false, &a, &all).seam, "FHD right edge touches UHD");
        assert!(!dock(Position::Left, false, &a, &all).seam);
        assert!(dock(Position::Left, false, &b, &all).seam);
        assert!(!dock(Position::Right, false, &b, &all).seam);
        assert!(!dock(Position::Bottom, false, &a, &all).seam);
    }

    #[test]
    fn trigger_line_is_the_outermost_pixel_within_the_span() {
        let m = fhd();
        let left = dock(Position::Left, false, &m, &[]);
        let mid = left.panel.top + 10;
        assert!(on_trigger_line(&left, 0, mid));
        assert!(!on_trigger_line(&left, 1, mid));
        assert!(!on_trigger_line(&left, 0, left.panel.top - 1), "corner stays free");
        let bottom = dock(Position::Bottom, false, &m, &[]);
        // True screen edge (1079), below the taskbar top — not the work-area edge.
        assert!(on_trigger_line(&bottom, 960, 1079));
        assert!(!on_trigger_line(&bottom, 960, 1031));
        let center = dock(Position::Center, false, &m, &[]);
        assert!(!on_trigger_line(&center, 0, 500));
    }

    #[test]
    fn keep_open_reaches_the_true_edge() {
        let m = fhd();
        let bottom = dock(Position::Bottom, false, &m, &[]);
        let k = keep_open_rect(&bottom);
        assert_eq!(k.bottom, 1080);
        assert!(k.contains(960, 1079));
        assert!(!k.contains(960, bottom.window.top - 1));
        assert!(near_edge(&bottom, 960, 1000));
        assert!(!near_edge(&bottom, 960, 500));
    }

    #[test]
    fn hover_needs_a_dwell_without_buttons() {
        let mut h = HoverTracker::default();
        assert!(!h.tick(true, false, 0));
        assert!(!h.tick(true, false, 120));
        assert!(h.tick(true, false, 200));
        // Fired once; stays quiet until the cursor leaves.
        assert!(!h.tick(true, false, 600));
        assert!(!h.tick(false, false, 660));
        assert!(!h.tick(true, false, 700));
        assert!(h.tick(true, false, 900));
    }

    #[test]
    fn hover_drag_to_edge_disarms_until_leave() {
        let mut h = HoverTracker::default();
        assert!(!h.tick(true, true, 0));
        assert!(!h.tick(true, false, 100));
        assert!(!h.tick(true, false, 1000), "button was down on arrival");
        assert!(!h.tick(false, false, 1060));
        assert!(!h.tick(true, false, 1100));
        assert!(h.tick(true, false, 1300));
        // A brief touch does not fire.
        let mut h = HoverTracker::default();
        assert!(!h.tick(true, false, 0));
        assert!(!h.tick(false, false, 60));
        assert!(!h.tick(true, false, 120));
        assert!(!h.tick(true, false, 260));
        // A close disarms.
        h.disarm();
        assert!(!h.tick(true, false, 2000));
    }

    #[test]
    fn click_outside_fires_once_per_press() {
        let mut c = ClickTracker::default();
        c.reset(true);
        assert!(!c.tick(true, false), "held since open");
        assert!(!c.tick(false, false));
        assert!(c.tick(true, false));
        assert!(!c.tick(true, false));
        assert!(!c.tick(false, false));
        assert!(!c.tick(true, true), "inside the panel");
    }

    #[test]
    fn hud_sits_under_the_cursor_inside_the_work_area() {
        let work = Rect::new(0, 0, 1920, 1032);
        let r = hud_rect((960, 500), work, 1.0);
        // Capsule (40 px, centred in 112) starts 18 px under the hotspot.
        assert_eq!(r, Rect::new(960 - 220, 500 + 18 - 36, 440, 112));
        // Near the bottom → the capsule ends 18 px above the cursor.
        let r = hud_rect((960, 1020), work, 1.0);
        assert_eq!(r.bottom - 36, 1020 - 18);
        // Near the left edge → clamped.
        assert_eq!(hud_rect((5, 500), work, 1.0).left, 0);
        // Another monitor at 150 %.
        let w2 = Rect::new(1920, 0, 3840, 2088);
        let r = hud_rect((5700, 100), w2, 1.5);
        assert_eq!((r.width(), r.height()), (660, 168));
        assert_eq!(r.right, 1920 + 3840);
        assert_eq!(r.top, 100 + 27 - 54);
    }

    /// W3-A: a degenerate work area (reported mid display change) used to panic in `clamp`
    /// on the main thread; now the toast just lands at the area's corner.
    #[test]
    fn hud_survives_an_empty_or_tiny_work_area() {
        let r = hud_rect((50, 50), Rect::new(100, 200, 0, 0), 1.0);
        assert_eq!((r.left, r.width(), r.height()), (100, 1, 1));
        let r = hud_rect((0, 0), Rect::new(0, 0, 300, 60), 2.0);
        assert!(r.width() <= 300 && r.height() <= 60, "{r:?}");
        assert_eq!(r.left, 0);
    }
}
