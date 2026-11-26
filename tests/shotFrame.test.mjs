// Raw-body framing of shot_commit / shot_editor_commit (W5-A, ARCHITECTURE §18.4):
// src/lib/ipc/frame.ts mirrors src-tauri/src/screenshot/body.rs (its Rust tests cover the same
// cases) — `[u32 LE N][N bytes JSON][RGBA width*height*4 | nothing]`.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { frameBody, parseFrameBody, MAX_SIDE } = await importTs("src/lib/ipc/frame.ts");

const commit = (width, height) => ({ sessionId: 3, rect: { x: 10.5, y: 20, w: 2, h: 1 }, action: "save_as", doc: null, width, height });

test("layout: little-endian length, UTF-8 JSON, then the pixels", () => {
  const head = { ...commit(1, 1), note: "снимок" };
  const px = new Uint8ClampedArray([1, 2, 3, 255]);
  const body = frameBody(head, px);
  const json = new TextEncoder().encode(JSON.stringify(head));
  assert.equal(new DataView(body.buffer).getUint32(0, true), json.length);
  assert.deepEqual([...body.subarray(4, 4 + json.length)], [...json]);
  assert.deepEqual([...body.subarray(4 + json.length)], [1, 2, 3, 255]);
  assert.equal(body.length, 4 + json.length + 4);
});

test("round trip with and without pixels; a subarray view is framed by its own bytes", () => {
  const none = parseFrameBody(frameBody(commit(0, 0)));
  assert.deepEqual(none.head, commit(0, 0));
  assert.equal(none.rgba, null);
  const big = new Uint8Array(16).map((_, i) => i);
  const view = big.subarray(8, 16); // 2×1 RGBA in the middle of a larger buffer
  const both = parseFrameBody(frameBody(commit(2, 1), view));
  assert.deepEqual([...both.rgba], [8, 9, 10, 11, 12, 13, 14, 15]);
});

test("broken bodies are rejected like Rust does (`invalid: …`)", () => {
  const bad = (body, why) => assert.throws(() => parseFrameBody(body), (e) => typeof e === "string" && e.startsWith("invalid: "), why);
  bad(new Uint8Array(0), "empty");
  bad(new Uint8Array([5, 0, 0]), "no length prefix");
  bad(new Uint8Array([0, 0, 0, 0]), "empty header");
  bad(new Uint8Array([255, 255, 255, 255, 123]), "header longer than the body");
  const cut = frameBody(commit(0, 0));
  bad(cut.subarray(0, cut.length - 1), "JSON cut short");
  bad(frameBody([1, 2]), "not an object");
  bad(frameBody(commit(0, 0), new Uint8Array(4)), "pixels for 0×0");
  bad(frameBody(commit(2, 0)), "one side 0");
  bad(frameBody(commit(2, 1), new Uint8Array(4)), "too few bytes");
  bad(frameBody(commit(1, 1), new Uint8Array(5)), "too many bytes");
  bad(frameBody(commit(MAX_SIDE + 1, 1)), "absurd size");
  bad(frameBody(commit(1.5, 1), new Uint8Array(6)), "fractional size");
  bad({ length: 9 }, "not bytes");
});
