// Snippet placeholder expansion (SPEC §3.10): src/lib/snippets/expand.ts — shared by the
// settings preview (W1-F) and the shelf paste path (W2-C).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { expandSnippet, parseSnippet, applyOffset, parseOffset, formatDate, caretSteps } = await importTs(
  "src/lib/snippets/expand.ts",
);

// Thursday, 26 November 2026, 14:05:09 local time.
const NOW = new Date(2026, 10, 26, 14, 5, 9);
let uuids = 0;
const ctx = (over = {}) => ({
  now: NOW,
  clipboard: "  Copied Text  ",
  uuid: () => `uuid-${++uuids}`,
  locale: "en",
  ...over,
});
const x = (body, over) => expandSnippet(body, ctx(over));

test("plain text passes through; no cursor → caretBack 0", () => {
  assert.deepEqual(x("Hello, world"), { text: "Hello, world", caretBack: 0, caretIndex: null });
  assert.deepEqual(x(""), { text: "", caretBack: 0, caretIndex: null });
});

test("{clipboard} and modifiers apply left to right", () => {
  assert.equal(x("[{clipboard}]").text, "[  Copied Text  ]");
  assert.equal(x("{clipboard | trim}").text, "Copied Text");
  assert.equal(x("{clipboard|trim|uppercase}").text, "COPIED TEXT");
  assert.equal(x("{clipboard | lowercase | trim}").text, "copied text");
  assert.equal(x("{clipboard | uppercase}", { clipboard: "привет", locale: "ru" }).text, "ПРИВЕТ");
});

test("{date} {time} {datetime} {weekday} with format", () => {
  assert.equal(x('{date format="yyyy-MM-dd"}').text, "2026-11-26");
  assert.equal(x('{time format="HH:mm:ss"}').text, "14:05:09");
  assert.equal(x('{datetime format="dd.MM.yy H:m"}').text, "26.11.26 14:5");
  assert.equal(x('{time format="h:mm tt"}').text, "2:05 PM");
  assert.equal(x('{time format="hh"}', { now: new Date(2026, 0, 1, 0, 30) }).text, "12");
  assert.equal(x('{weekday format="ddd"}').text, "Thu");
  assert.equal(x("{weekday}").text, "Thursday");
  assert.equal(x("{weekday}", { locale: "ru" }).text, "четверг");
  assert.equal(x("{weekday | uppercase}").text, "THURSDAY");
});

test("month names: standalone vs genitive (ru) next to a day", () => {
  assert.equal(x('{date format="MMMM yyyy"}').text, "November 2026");
  assert.equal(x('{date format="d MMMM"}').text, "26 November");
  assert.equal(x('{date format="MMMM"}', { locale: "ru" }).text, "ноябрь");
  assert.equal(x('{date format="d MMMM yyyy"}', { locale: "ru" }).text, "26 ноября 2026");
});

test("quoted literals in formats", () => {
  assert.equal(x("{date format=\"yyyy-MM-dd'T'HH:mm\"}").text, "2026-11-26T14:05");
  assert.equal(x("{time format=\"H 'h' m 'min'\"}").text, "14 h 5 min");
  assert.equal(x("{time format=\"HH''mm\"}").text, "14'05");
  // Single-quoted attribute value with double quotes inside.
  assert.equal(x(`{date format='"yyyy"'}`).text, '"2026"');
});

test("default formats follow the locale", () => {
  assert.equal(x("{date}").text, "11/26/2026");
  assert.equal(x("{date}", { locale: "ru" }).text, "26.11.2026");
  assert.equal(x("{time}", { locale: "ru" }).text, "14:05");
  assert.match(x("{datetime}", { locale: "ru" }).text, /^26\.11\.2026,? 14:05$/);
});

test("offsets: units, chains, inherited sign, month-end clamp", () => {
  assert.equal(x('{date format="yyyy-MM-dd" offset="+2d"}').text, "2026-11-28");
  assert.equal(x('{date format="yyyy-MM-dd" offset="-7d"}').text, "2026-11-19");
  assert.equal(x('{date format="yyyy-MM-dd" offset="+1w"}').text, "2026-12-03");
  assert.equal(x('{date format="yyyy-MM-dd" offset="+1M"}').text, "2026-12-26");
  assert.equal(x('{date format="yyyy-MM-dd" offset="-1y"}').text, "2025-11-26");
  assert.equal(x('{time format="HH:mm" offset="+90m"}').text, "15:35");
  assert.equal(x('{datetime format="yyyy-MM-dd HH:mm" offset="+1d-2h"}').text, "2026-11-27 12:05");
  assert.equal(x('{datetime format="yyyy-MM-dd HH:mm" offset="-1d 2h"}').text, "2026-11-25 12:05", "unsigned part inherits −");
  assert.equal(x('{date format="ddd" offset="+1d"}').text, "Fri");
  const jan31 = new Date(2027, 0, 31, 10, 0);
  assert.equal(formatDate(applyOffset(jan31, "+1M"), "yyyy-MM-dd"), "2027-02-28");
  assert.equal(formatDate(applyOffset(new Date(2028, 0, 31), "+1M"), "yyyy-MM-dd"), "2028-02-29");
  assert.deepEqual(parseOffset("+1d-2h"), [
    { amount: 1, unit: "d" },
    { amount: -2, unit: "h" },
  ]);
  for (const bad of ["", "2", "+2x", "d", "+1d junk", "++1d"]) assert.equal(parseOffset(bad), null, bad);
});

test("{uuid} calls the generator once per placeholder", () => {
  uuids = 0;
  assert.equal(x("{uuid} {uuid}").text, "uuid-1 uuid-2");
  assert.equal(x("{uuid|uppercase}").text, "UUID-3");
});

test("{cursor}: caretBack counts code points after it (CRLF = 1); extra cursors are dropped", () => {
  const r = x("Hello {cursor}world");
  assert.equal(r.text, "Hello world");
  assert.equal(r.caretBack, 5);
  assert.equal(r.caretIndex, 6);
  assert.equal(x("{cursor}").caretBack, 0);
  assert.equal(x("ab{cursor}").caretIndex, 2);
  const two = x("a{cursor}b{cursor}c");
  assert.equal(two.text, "abc");
  assert.equal(two.caretBack, 2, "first cursor wins");
  // Paste contract (W1-C): one ← per code point, a surrogate pair is one, CRLF is one.
  assert.equal(x("{cursor}\u{1F600}").caretBack, 1, "surrogate pair = 1");
  assert.equal(x("{cursor}a\r\nb").caretBack, 3, "CRLF = 1");
  assert.equal(x("{cursor}a\nb").caretBack, 3, "LF = 1");
  assert.equal(x("{cursor}\r\r\n").caretBack, 2, "lone CR, then CRLF");
  assert.equal(x("{cursor}\u{1F44D}\u{1F3FD}").caretBack, 2, "emoji + skin-tone modifier = 2 code points");
  assert.equal(x("{cursor}é").caretBack, 2, "a combining mark is its own code point");
  // Placeholders after the cursor are expanded before counting.
  assert.equal(x("{cursor} {clipboard|trim}").caretBack, 12);
  assert.equal(caretSteps("\u{1F468}‍\u{1F469}‍\u{1F467}"), 5, "ZWJ family = 3 emoji + 2 joiners");
  assert.equal(caretSteps(""), 0);
  assert.equal(caretSteps("\uD800x"), 2, "an unpaired surrogate still counts once");
});

test("escapes: {{ and }} are literal braces", () => {
  assert.equal(x("{{date}}").text, "{date}");
  assert.equal(x("a {{ b }} c").text, "a { b } c");
  assert.equal(x("{{{clipboard|trim}}}").text, "{Copied Text}");
});

test("non-placeholder braces stay literal (code, JSON)", () => {
  for (const s of ['{ "a": 1 }', "function f() { return 1; }", "{a: 1}", "{", "}", "{date", "{date\n}", "{1}", "{}"]) {
    assert.equal(x(s).text, s, s);
  }
});

test("problems are reported by parseSnippet and pasted as written", () => {
  const cases = [
    ["{foo}", "unknown"],
    ["{date fmt=\"x\"}", "attribute"],
    ["{uuid format=\"x\"}", "attribute"],
    ["{date format=\"yyyy 'x\"}", "format"],
    ["{date offset=\"tomorrow\"}", "offset"],
    ["{clipboard | shout}", "modifier"],
    ["{cursor | trim}", "modifier"],
    ["{date format=\"x\"offset=\"+1d\"}", "syntax"],
    ["{date format}", "syntax"],
    ["{date format=\"a\" format=\"b\"}", "syntax"],
    ["{date |}", "syntax"],
  ];
  for (const [body, issue] of cases) {
    const toks = parseSnippet(body).filter((t) => t.kind === "placeholder");
    assert.equal(toks.length, 1, body);
    assert.equal(toks[0].issue, issue, body);
    assert.equal(x(body).text, body, `${body} pasted as written`);
  }
  const cur = parseSnippet("{cursor}{cursor}").filter((t) => t.kind === "placeholder");
  assert.deepEqual(cur.map((t) => t.issue), [null, "cursor"]);
});

test("parseSnippet tokens cover the body with UTF-16 offsets", () => {
  const body = 'Hi {date format="d"} {{x}} {clipboard|trim}!';
  const toks = parseSnippet(body);
  let at = 0;
  for (const t of toks) {
    assert.equal(t.start, at);
    at = t.end;
  }
  assert.equal(at, body.length);
  const ph = toks.filter((t) => t.kind === "placeholder");
  assert.deepEqual(ph.map((t) => t.name), ["date", "clipboard"]);
  assert.deepEqual(ph[0].attrs, { format: "d" });
  assert.deepEqual(ph[1].modifiers, ["trim"]);
  assert.deepEqual(toks.filter((t) => t.kind === "escape").map((t) => t.text), ["{", "}"]);
  // Names and modifiers are case-insensitive.
  assert.equal(x("{DATE format=\"yyyy\" | LowerCase}").text, "2026");
});

test("seeded snippet bodies expand", () => {
  const r = x('Meeting {date format="yyyy-MM-dd"} — {time}\n\nAttendees:\nAgenda:\n{cursor}');
  assert.ok(r.text.startsWith("Meeting 2026-11-26 — "));
  assert.equal(r.caretBack, 0);
  assert.equal(r.caretIndex, r.text.length);
});
