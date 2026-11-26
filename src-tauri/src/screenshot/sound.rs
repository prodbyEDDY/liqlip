//! The shutter click (W8-A; SPEC §9.4 "a short synthesized click — no audio file",
//! `settings.screenshot.sound`; ARCHITECTURE §18.4 step 1a). Played by Rust at the moment a shot
//! is committed — every action except `text`, whatever follows on screen (the QAO, the editor, a
//! pin or nothing; `Ctrl+C` / `Ctrl+S` in the overlay; the captures without overlay). Before W8 the
//! QAO page played it on `shot:added`, so every commit that showed no card was silent.
//!
//! The sound is a 60 ms mono 16-bit PCM WAV built once in memory (no file in the repository):
//! two short noise bursts through a band-pass filter, the "open" and the softer "close" of a
//! mechanical shutter — the same recipe the QAO used with Web Audio (`BiquadFilterNode`
//! "bandpass", whose coefficients are the Audio EQ Cookbook's band-pass with constant 0 dB peak
//! gain: https://www.w3.org/TR/audio-eq-cookbook/). Deterministic noise (xorshift32), so the
//! click is the same every time.
//!
//! Playback: winmm `PlaySoundW(wav, NULL, SND_MEMORY | SND_ASYNC | SND_NODEFAULT)`
//! (https://learn.microsoft.com/previous-versions/dd743680(v=vs.85)): `SND_MEMORY` — "The
//! pszSound parameter points to a sound loaded in memory"; `SND_ASYNC` — returns "immediately
//! after beginning the sound"; `SND_NODEFAULT` — never the system default sound instead. Remarks:
//! "If you combine the SND_MEMORY and SND_ASYNC flags, the memory buffer that contains the sound
//! must remain valid until the sound has completed playing" — the buffer is a `static` (built once,
//! never freed). A new click stops the previous one of this process (no `SND_NOSTOP`), which is
//! what rapid shots want. windows 0.62.2: `Win32::Media::Audio::PlaySoundW` (feature
//! `Win32_Media_Audio`, linked through `windows-link` — no import library, no new crate).

use std::sync::OnceLock;

/// Samples per second.
const RATE: u32 = 44_100;
/// Length of the click.
const LENGTH_MS: u32 = 60;
/// The two bursts: start (ms), length (ms), band centre (Hz), level.
const BURSTS: [(u32, u32, f32, f32); 2] = [(0, 28, 2600.0, 0.5), (34, 26, 3600.0, 0.28)];
/// Band-pass Q (as the Web Audio version).
const Q: f32 = 0.8;
/// Peak of the finished click, of full scale (≈ −6 dBFS).
const PEAK: f32 = 0.5;

/// Biquad band-pass, constant 0 dB peak gain (Audio EQ Cookbook): `b0 = α, b1 = 0, b2 = −α,
/// a0 = 1 + α, a1 = −2·cos ω0, a2 = 1 − α`, `ω0 = 2π·f0/Fs`, `α = sin ω0 / (2Q)`.
struct BandPass {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BandPass {
    fn new(freq: f32, q: f32, rate: f32) -> Self {
        let w0 = std::f32::consts::TAU * freq / rate;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        BandPass { b0: alpha / a0, b2: -alpha / a0, a1: -2.0 * w0.cos() / a0, a2: (1.0 - alpha) / a0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        (self.x2, self.x1, self.y2, self.y1) = (self.x1, x, self.y1, y);
        y
    }
}

/// The click as samples in −1…1.
fn samples() -> Vec<f32> {
    let n = (RATE * LENGTH_MS / 1000) as usize;
    let mut out = vec![0f32; n];
    let mut seed: u32 = 0x9E37_79B9;
    let mut noise = move || {
        // xorshift32 → −1…1
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    for (at_ms, ms, freq, level) in BURSTS {
        let start = (RATE * at_ms / 1000) as usize;
        let len = (RATE * ms / 1000) as usize;
        let mut band = BandPass::new(freq, Q, RATE as f32);
        for i in 0..len.min(n.saturating_sub(start)) {
            // A sharp attack, an exponential decay (as the Web Audio buffer).
            let x = noise() * (-(i as f32) / (len as f32 * 0.22)).exp();
            out[start + i] += level * band.run(x);
        }
    }
    let peak = out.iter().fold(0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        out.iter_mut().for_each(|s| *s *= PEAK / peak);
    }
    out
}

/// A RIFF/WAVE file: `fmt ` (PCM, mono, 16-bit, [`RATE`]) + `data`.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
    w.extend_from_slice(&2u16.to_le_bytes()); // block align
    w.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&((s * f32::from(i16::MAX)).round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16).to_le_bytes());
    }
    w
}

/// The click, built on first use and kept for the life of the process (`SND_ASYNC` reads it
/// after `PlaySoundW` has returned).
fn click() -> &'static [u8] {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    WAV.get_or_init(|| wav(&samples()))
}

/// Play the click without waiting for it (a refusal — no audio device — is only logged).
pub(crate) fn shutter() {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
        let wav = click();
        // SAFETY: with SND_MEMORY the pointer is the in-memory WAV image (read as bytes, never as
        // UTF-16); it is a `static` buffer, valid until the asynchronous sound has finished.
        let ok = unsafe { PlaySoundW(PCWSTR(wav.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
        if !ok.as_bool() {
            crate::logx::warn("screenshot", "shutter: PlaySound refused (no audio device?)");
        }
    }
    #[cfg(not(windows))]
    let _ = click();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }
    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    #[test]
    fn the_click_is_a_60_ms_pcm_wave_file() {
        let w = click();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32_at(w, 4) as usize, w.len() - 8, "RIFF size");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!((u32_at(w, 16), u16_at(w, 20), u16_at(w, 22)), (16, 1, 1), "PCM, mono");
        assert_eq!((u32_at(w, 24), u32_at(w, 28), u16_at(w, 32), u16_at(w, 34)), (RATE, RATE * 2, 2, 16));
        assert_eq!(&w[36..40], b"data");
        let data = u32_at(w, 40) as usize;
        assert_eq!(data, w.len() - 44);
        assert_eq!(data / 2, (RATE * LENGTH_MS / 1000) as usize, "60 ms of samples");
        // The same buffer every time (a static: SND_ASYNC reads it after the call returns).
        assert!(std::ptr::eq(click(), w));
    }

    #[test]
    fn the_click_is_two_bursts_at_minus_6_dbfs_without_clipping() {
        let s = samples();
        let peak = s.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - PEAK).abs() < 1e-4, "peak {peak}");
        let ms = |a: u32, b: u32| &s[(RATE * a / 1000) as usize..(RATE * b / 1000) as usize];
        let loud = |part: &[f32]| part.iter().fold(0f32, |m, v| m.max(v.abs()));
        // Both bursts sound; each decays to near silence before the next one / the end (no click
        // at the edges), and nothing is clipped by the 16-bit conversion.
        assert!(loud(ms(0, 6)) > 0.2, "the opening burst");
        assert!(loud(ms(34, 40)) > 0.05, "the closing burst");
        assert!(loud(ms(27, 28)) < 0.05 * PEAK * 2.0, "the first burst has died away");
        assert!(loud(ms(59, 60)) < 0.05 * PEAK * 2.0, "silence at the end");
        assert_eq!(samples(), s, "deterministic");
    }
}
