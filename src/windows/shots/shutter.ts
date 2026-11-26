// The shutter click (SPEC §9.4, `settings.screenshot.sound`): synthesized with the Web Audio API
// (MDN: AudioContext, AudioBuffer, BiquadFilterNode, GainNode) — no audio file (LT's MP3 did
// not play in its checks, research §1.13). Two short filtered noise bursts, ~60 ms in all: the
// "open" and the softer "close" of a mechanical shutter.
//
// Autoplay: an AudioContext made without a user gesture may start "suspended"; `resume()` is
// tried and a refusal is silent (no sound beats an error). Whether WebView2 lets the QAO play
// without a gesture is checked at integration (report W6-E, risks).

let ctx: AudioContext | null = null;

function burst(c: AudioContext, at: number, ms: number, freq: number, level: number): void {
  const length = Math.max(1, Math.floor((c.sampleRate * ms) / 1000));
  const buffer = c.createBuffer(1, length, c.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < length; i++) data[i] = (Math.random() * 2 - 1) * Math.exp(-i / (length * 0.22));
  const src = c.createBufferSource();
  src.buffer = buffer;
  const band = c.createBiquadFilter();
  band.type = "bandpass";
  band.frequency.value = freq;
  band.Q.value = 0.8;
  const gain = c.createGain();
  gain.gain.value = level;
  src.connect(band).connect(gain).connect(c.destination);
  src.start(at);
}

export function playShutter(): void {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === "suspended") void ctx.resume().catch(() => {});
    const t0 = ctx.currentTime + 0.005;
    burst(ctx, t0, 28, 2600, 0.5);
    burst(ctx, t0 + 0.034, 26, 3600, 0.28);
  } catch (e) {
    console.warn("[shots] shutter", e); // no audio device / Web Audio unavailable
  }
}
