//! Window tree under the overlay (owner: W6-A; ARCHITECTURE §18.3). Empty in W5: no command
//! delegates here.
//!
//! To do: port of Liquid Tools `screenshot/capture.rs:54-104, 239-354` — `EnumWindows` in
//! z-order (skip ours, invisible, minimized, tool+noactivate, cloaked), DWM extended frame
//! bounds clipped to the monitor, children via `EnumChildWindows` with the limits 2048 nodes /
//! 32 levels / 35 ms per window / 120 ms for all children → `Vec<WinRect>` in physical px of
//! the monitor frame.
