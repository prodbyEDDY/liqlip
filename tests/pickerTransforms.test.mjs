// Text transforms of the actions menu (W2-C, SPEC §3.7): case, lines, JSON, URL, Base64,
// HTML → text, slug — RU/EN and edge cases (CRLF, empty input, invalid input → typed error).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { applyTransform, eolOf, lineColumn, slugify, htmlToText, TRANSFORM_GROUPS } = await importTs("src/windows/picker/transforms.ts");

const run = (id, input, opts) => {
  const r = applyTransform(id, input, opts);
  assert.equal(r.ok, true, `${id} failed: ${JSON.stringify(r)}`);
  return r.text;
};
const fails = (id, input, opts) => {
  const r = applyTransform(id, input, opts);
  assert.equal(r.ok, false, `${id} should fail on ${JSON.stringify(input)}`);
  return r.error;
};

test("every listed transform runs on empty input without throwing", () => {
  const ids = TRANSFORM_GROUPS.flat();
  assert.equal(ids.length, 17);
  for (const id of ids) {
    const r = applyTransform(id, "", { parseHtml: () => node(1, "BODY", []) });
    if (id === "jsonFormat" || id === "jsonMinify") assert.equal(r.ok, false, id);
    else assert.deepEqual(r, { ok: true, text: "" }, id);
  }
});

test("case: upper / lower in Russian and English", () => {
  assert.equal(run("upper", "Привет, world ёж"), "ПРИВЕТ, WORLD ЁЖ");
  assert.equal(run("lower", "ПРИВЕТ, World ЁЖ"), "привет, world ёж");
});

test("case: Каждое Слово — letters after non-letters, apostrophes and digits respected", () => {
  assert.equal(run("title", "привет МИР-тест o'neil 3d"), "Привет Мир-Тест O'neil 3d");
  assert.equal(run("title", "hello\nworld\tagain"), "Hello\nWorld\tAgain");
  assert.equal(run("title", "  «цитата» (скобки)"), "  «Цитата» (Скобки)");
});

test("case: sentence — text start, after . ! ? … and at every line start", () => {
  assert.equal(run("sentence", "HELLO WORLD. how ARE you?\nновая СТРОКА! ещё… и всё"), "Hello world. How are you?\nНовая строка! Ещё… И всё");
  assert.equal(run("sentence", "1. first item\n- second"), "1. First item\n- Second");
  assert.equal(run("sentence", 'she said "STOP." then left'), 'She said "stop." Then left');
});

test("lines: trim keeps CRLF and drops blank edges", () => {
  assert.equal(run("trim", "  a  \r\n\tb\t\r\n\r\n"), "a\r\nb");
  assert.equal(run("trim", "\n\n  только пробелы  \n"), "только пробелы");
  assert.equal(eolOf("a\r\nb"), "\r\n");
  assert.equal(eolOf("a\rb"), "\r");
  assert.equal(eolOf("a\nb"), "\n");
});

test("lines: remove line breaks joins with single spaces, blank lines included", () => {
  assert.equal(run("joinLines", "a\n b \r\n\r\nc"), "a b c");
  assert.equal(run("joinLines", "строка  с  пробелами\nдальше"), "строка  с  пробелами дальше");
});

test("lines: remove empty lines (whitespace-only too)", () => {
  assert.equal(run("dropEmpty", "a\n\n \t\nb\n"), "a\nb");
  assert.equal(run("dropEmpty", "x\r\n\r\ny"), "x\r\ny");
});

test("lines: sort — Russian collation, numbers by value, trailing break kept", () => {
  assert.equal(run("sortLines", "яблоко\nАрбуз\nбанан\n10\n9\n"), "9\n10\nАрбуз\nбанан\nяблоко\n");
  assert.equal(run("sortLines", "b\r\na"), "a\r\nb");
  assert.equal(run("sortLines", "жук\nель\nёж"), "ёж\nель\nжук", "ё sorts as е (dictionary order)");
});

test("lines: dedupe keeps the first occurrence, exact and case-sensitive", () => {
  assert.equal(run("dedupeLines", "a\nb\na\nB\nb\n"), "a\nb\nB\n");
  assert.equal(run("dedupeLines", "один\r\nдва\r\nодин"), "один\r\nдва");
});

test("JSON: format with 2 spaces, minify; literals survive verbatim", () => {
  const src = '{"a":1,"b":[1,2,{"c":null}],"d":{},"e":[],"big":12345678901234567890,"s":"\\u0041\\n"}';
  assert.equal(
    run("jsonFormat", src),
    '{\n  "a": 1,\n  "b": [\n    1,\n    2,\n    {\n      "c": null\n    }\n  ],\n  "d": {},\n  "e": [],\n  "big": 12345678901234567890,\n  "s": "\\u0041\\n"\n}',
  );
  assert.equal(run("jsonMinify", '{\n  "a" : [ 1 , true ],\r\n "a": "dup"\n}'), '{"a":[1,true],"a":"dup"}');
  assert.equal(run("jsonFormat", "﻿  42  "), "42");
  assert.equal(run("jsonFormat", '"Привет"'), '"Привет"');
  assert.equal(run("jsonMinify", "[-0.5e+10, 0, \"x\"]"), '[-0.5e+10,0,"x"]');
});

test("JSON: invalid input → error with line and column, never empty output", () => {
  assert.deepEqual(fails("jsonFormat", '{\n  "a": 1,\n  "b": }'), { code: "json", line: 3, column: 8 });
  assert.deepEqual(fails("jsonFormat", "{'a': 1}"), { code: "json", line: 1, column: 2 });
  assert.deepEqual(fails("jsonMinify", "[1, 2"), { code: "json", line: 1, column: 6 });
  assert.deepEqual(fails("jsonFormat", "[01]"), { code: "json", line: 1, column: 3 });
  assert.deepEqual(fails("jsonFormat", '{"a": 1,}'), { code: "json", line: 1, column: 9 });
  assert.deepEqual(fails("jsonFormat", '"bad \\x escape"'), { code: "json", line: 1, column: 6 });
  assert.deepEqual(fails("jsonFormat", "{} extra"), { code: "json", line: 1, column: 4 });
  assert.deepEqual(fails("jsonFormat", "просто текст"), { code: "json", line: 1, column: 1 });
  assert.deepEqual(lineColumn("a\r\nbc", 4), { line: 2, column: 2 });
});

test("URL encode / decode (UTF-8), malformed escapes are an error", () => {
  assert.equal(run("urlEncode", "привет мир?&=/"), "%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82%20%D0%BC%D0%B8%D1%80%3F%26%3D%2F");
  assert.equal(run("urlDecode", "%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82%20world"), "привет world");
  assert.equal(run("urlDecode", "a+b"), "a+b", "plus is literal (component decoding)");
  assert.deepEqual(fails("urlDecode", "100%"), { code: "url" });
  assert.deepEqual(fails("urlDecode", "%E0%A4%A"), { code: "url" });
  assert.deepEqual(fails("urlEncode", "\uD800"), { code: "url" }, "lone surrogate");
});

test("Base64 encode / decode as UTF-8; padding, URL-safe, data URI; invalid and binary → errors", () => {
  assert.equal(run("base64Encode", "Привет"), "0J/RgNC40LLQtdGC");
  assert.equal(run("base64Encode", "Привет, мир!"), "0J/RgNC40LLQtdGCLCDQvNC40YAh");
  assert.equal(run("base64Encode", "hello"), "aGVsbG8=");
  assert.equal(run("base64Decode", "aGVsbG8="), "hello");
  assert.equal(run("base64Decode", "aGVsbG8"), "hello", "missing padding");
  assert.equal(run("base64Decode", " 0J/RgNC4\n0LLQtdGC "), "Привет", "whitespace inside");
  assert.equal(run("base64Decode", "0J_RgNC40LLQtdGC"), "Привет", "URL-safe alphabet");
  assert.equal(run("base64Decode", "data:text/plain;base64,aGk="), "hi");
  assert.deepEqual(fails("base64Decode", "@@@"), { code: "base64" });
  assert.deepEqual(fails("base64Decode", "aGVsbG8=x"), { code: "base64" }, "padding in the middle");
  assert.deepEqual(fails("base64Decode", "a"), { code: "base64" }, "impossible length");
  assert.deepEqual(fails("base64Decode", "aGk=="), { code: "base64" }, "wrong padding length");
  assert.deepEqual(fails("base64Decode", "/w=="), { code: "base64Binary" }, "0xFF is not UTF-8");
  const long = "Тест ".repeat(20000);
  assert.equal(run("base64Decode", run("base64Encode", long)), long, "round trip of a long text");
});

// A tiny DOM double for htmlToText (node has no DOMParser).
function node(type, name, kids = [], value = null) {
  return { nodeType: type, nodeName: name, nodeValue: value, childNodes: kids };
}
const el = (name, ...kids) => node(1, name, kids);
const tx = (value) => node(3, "#text", [], value);

test("HTML → text: blocks, breaks, lists, tables, pre; scripts and styles skipped", () => {
  const body = el(
    "BODY",
    el("H1", tx("Заголовок")),
    el("P", tx("Первый   абзац\n с "), el("B", tx("жирным")), tx(" текстом.")),
    el("SCRIPT", tx("alert(1)")),
    el("STYLE", tx("p{}")),
    el("P", tx("Строка"), el("BR"), tx("после переноса")),
    el("UL", el("LI", tx(" один ")), el("LI", tx("два"))),
    el("OL", el("LI", tx("first")), el("LI", tx("second"))),
    el("TABLE", el("TR", el("TD", tx("a")), el("TD", tx(" b"))), el("TR", el("TD", tx("c")), el("TD", tx("d")))),
    el("PRE", tx("  keep\n   spacing")),
    el("DIV", tx("end")),
  );
  const r = htmlToText("<ignored>", () => body);
  assert.deepEqual(r, {
    ok: true,
    text:
      "Заголовок\n\nПервый абзац с жирным текстом.\n\nСтрока\nпосле переноса\n\n• один\n• два\n\n1. first\n2. second\n\na\tb\nc\td\n\n  keep\n   spacing\n\nend",
  });
  assert.deepEqual(htmlToText("<p>x</p>", () => null), { ok: false, error: { code: "html" } });
  assert.deepEqual(applyTransform("htmlToText", "<b>x</b>"), { ok: false, error: { code: "html" } }, "no DOMParser in node");
});

test("slug: Cyrillic transliteration, accents, separators", () => {
  assert.equal(slugify("Привет, Мир! Hello World — 2024"), "privet-mir-hello-world-2024");
  assert.equal(slugify("Щука и ёжик: съешь же ещё"), "shchuka-i-yozhik-sesh-zhe-eshchyo");
  assert.equal(slugify("Café déjà vu"), "cafe-deja-vu");
  assert.equal(slugify("Їжак і Ґанок"), "yizhak-i-ganok");
  assert.equal(slugify("  ---  "), "");
  assert.equal(run("slug", "Хорошая цена_100%"), "khoroshaya-tsena-100");
});
