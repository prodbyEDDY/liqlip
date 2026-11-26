//! Screen freeze (owner: W6-A; ARCHITECTURE §18.3). Empty in W5: no command delegates here.
//!
//! To do: per monitor `GetDC(None)` + `BitBlt(SRCCOPY | CAPTUREBLT)` of its rectangle →
//! `GetDIBits` 32 bpp top-down (port of Liquid Tools `screenshot/capture.rs:150-230`), monitor
//! geometry in physical px of the virtual screen (Per-Monitor-V2). Before the blit the shelf is
//! hidden + `DwmFlush`; the QAO and `capture` carry `WDA_EXCLUDEFROMCAPTURE`
//! (`windows::exclude_from_capture`).
