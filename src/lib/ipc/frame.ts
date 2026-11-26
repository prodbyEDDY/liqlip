// Raw-body framing of `shot_commit` / `shot_editor_commit` (docs/ARCHITECTURE.md §18.4; hub,
// W5-A). Mirror of `src-tauri/src/screenshot/body.rs`:
//
//   [u32 LE: N] [N bytes UTF-8 JSON header] [RGBA: width*height*4 bytes, or nothing]
//
// The bytes go to Rust as the invoke payload (`invoke(cmd, Uint8Array)` — @tauri-apps/api 2.11
// `InvokeArgs`), so megabytes of pixels never become JSON and nothing rides in headers. Pure —
// node-tested (tests/shotFrame.test.mjs); the browser mock parses with `parseFrameBody`.

/** Pixels as `getImageData().data` (Uint8ClampedArray) or any byte view. */
export type Pixels = Uint8Array | Uint8ClampedArray;

/** Build a framed body: the JSON header (`{…, width, height}`), then the RGBA block if any. */
export function frameBody(json: unknown, rgba?: Pixels | null): Uint8Array {
  const head = new TextEncoder().encode(JSON.stringify(json));
  const px = rgba ? new Uint8Array(rgba.buffer, rgba.byteOffset, rgba.byteLength) : new Uint8Array(0);
  const out = new Uint8Array(4 + head.length + px.length);
  new DataView(out.buffer).setUint32(0, head.length, true);
  out.set(head, 4);
  out.set(px, 4 + head.length);
  return out;
}

/** Upper bounds — the same as Rust's `body::MAX_HEADER_BYTES` / `body::MAX_SIDE`. */
export const MAX_HEADER_BYTES = 64 * 1024 * 1024;
export const MAX_SIDE = 32_768;

/** Split a framed body. Throws `"invalid: …"` (the Rust error shape) for any malformed body;
 *  the header must carry `width`/`height` and the pixel block must be exactly `width*height*4`. */
export function parseFrameBody<T extends { width: number; height: number }>(body: Uint8Array): { head: T; rgba: Uint8Array | null } {
  if (!(body instanceof Uint8Array)) throw "invalid: expected a raw body (frameBody)";
  if (body.length < 4) throw "invalid: body shorter than its length prefix";
  const n = new DataView(body.buffer, body.byteOffset, body.byteLength).getUint32(0, true);
  if (n === 0 || n > MAX_HEADER_BYTES) throw `invalid: header length ${n}`;
  if (4 + n > body.length) throw `invalid: header of ${n} bytes is cut short`;
  let head: T;
  try {
    head = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(body.subarray(4, 4 + n))) as T;
  } catch (e) {
    throw `invalid: header: ${String(e)}`;
  }
  if (head === null || typeof head !== "object" || Array.isArray(head)) throw "invalid: header is not an object";
  const { width: w, height: h } = head;
  const rest = body.subarray(4 + n);
  if (!Number.isInteger(w) || !Number.isInteger(h) || w < 0 || h < 0) throw `invalid: picture size ${String(w)}×${String(h)}`;
  if (w === 0 && h === 0) {
    if (rest.length) throw `invalid: ${rest.length} pixel bytes for a 0×0 picture`;
    return { head, rgba: null };
  }
  if (w === 0 || h === 0 || w > MAX_SIDE || h > MAX_SIDE) throw `invalid: picture size ${w}×${h}`;
  if (rest.length !== w * h * 4) throw `invalid: ${w}×${h} needs ${w * h * 4} pixel bytes, got ${rest.length}`;
  return { head, rgba: rest };
}
