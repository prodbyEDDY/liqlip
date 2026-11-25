// Card display helpers (W1-E): format badges and file families (ported from Liquid Tools
// tests/libraryPreview.test.mjs), auto titles, colour re-validation, sizes, highlighting.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const m = await importTs("src/windows/picker/preview.ts", "src/windows/picker/format.ts", "src/windows/picker/highlight.ts");
const { entryFormat, fileIcon, autoTitle, baseName, colorValue, formatSize, hostPath, linkHost, pluralCategory, searchTerms, segments } = m;

const clip = (over) => ({ kind: "text", preview: "", meta: {}, formats: ["text"], width: null, height: null, ...over });

test("file badge = extension of the first file; families", () => {
  assert.equal(entryFormat(clip({ kind: "file", meta: { ext: "xlsx" }, preview: "Plan.XLSX" })), "XLSX");
  assert.equal(entryFormat(clip({ kind: "file", preview: "C:\\reports\\Plan.XLSX\nC:\\report.pdf" })), "XLSX");
  assert.equal(entryFormat(clip({ kind: "file", preview: "C:\\Projects\\design" })), "FILE");
  assert.equal(fileIcon("XLSX"), "table");
  assert.equal(fileIcon("ZIP"), "archive");
  assert.equal(fileIcon("PDF"), "document");
  assert.equal(fileIcon("MP4"), "video");
  assert.equal(fileIcon("MP3"), "music");
  assert.equal(fileIcon("PNG"), "image");
  assert.equal(fileIcon("RS"), "code");
  assert.equal(fileIcon("FILE"), "folder");
  assert.equal(fileIcon("UNKNOWN"), "file");
});

test("contact, colour, link, code, image badges", () => {
  assert.equal(entryFormat(clip({ kind: "contact", preview: "person@example.com", meta: { contact: "email" } })), "EMAIL");
  assert.equal(entryFormat(clip({ kind: "contact", preview: "+84 (90) 123-4567" })), "TEL");
  assert.equal(entryFormat(clip({ kind: "color", preview: "#ffffff" })), "HEX");
  assert.equal(entryFormat(clip({ kind: "color", preview: "rgba(1, 2, 3, .5)" })), "RGB");
  assert.equal(entryFormat(clip({ kind: "color", preview: "hsl(48, 96%, 53%)" })), "HSL");
  assert.equal(entryFormat(clip({ kind: "link" })), "URL");
  assert.equal(entryFormat(clip({ kind: "code" })), "CODE");
  assert.equal(entryFormat(clip({ kind: "image", formats: ["png"] })), "PNG");
  assert.equal(entryFormat(clip({})), "TXT");
});

test("auto title: app for text-like, host+path for links, dimensions for images", () => {
  assert.equal(autoTitle(clip({}), "Telegram"), "Telegram");
  assert.equal(autoTitle(clip({}), null), null);
  assert.equal(autoTitle(clip({ kind: "link", preview: "https://www.github.com/tauri-apps/" }), "Chrome"), "github.com/tauri-apps");
  assert.equal(autoTitle(clip({ kind: "image", width: 1920, height: 1080 }), "Figma"), "1920 × 1080");
  assert.equal(baseName("C:\\Users\\me\\Report Q3.pdf"), "Report Q3.pdf");
});

test("colour value is re-validated before reaching CSS", () => {
  assert.equal(colorValue(clip({ kind: "color", preview: "#FF5733" })), "#FF5733");
  assert.equal(colorValue(clip({ kind: "color", preview: "rgb(34, 197, 94)" })), "rgb(34, 197, 94)");
  assert.equal(colorValue(clip({ kind: "color", preview: "red; background:url(x)" })), null);
  assert.equal(colorValue(clip({ kind: "color", preview: "x", meta: { color: { hex: "#22C55E" } } })), "#22C55E");
  assert.equal(colorValue(clip({ kind: "text", preview: "#fff" })), null);
});

test("sizes, hosts, plurals", () => {
  assert.equal(formatSize(0), null);
  assert.deepEqual(formatSize(512), { unit: "bytes", n: "512" });
  assert.deepEqual(formatSize(1536), { unit: "kb", n: "1.5" });
  assert.deepEqual(formatSize(5 * 1024 * 1024), { unit: "mb", n: "5.0" });
  assert.equal(hostPath("https://docs.rs/rusqlite/"), "docs.rs/rusqlite");
  assert.equal(linkHost(clip({ preview: "www.example.com/pricing" })), "www.example.com");
  assert.equal(linkHost(clip({ preview: "x", meta: { host: "a.b" } })), "a.b");
  assert.equal(pluralCategory(1, "ru"), "one");
  assert.equal(pluralCategory(3, "ru"), "few");
  assert.equal(pluralCategory(5, "ru"), "many");
  assert.equal(pluralCategory(21, "ru"), "one");
  assert.equal(pluralCategory(2, "en"), "many");
});

test("highlight: case-insensitive, Cyrillic, longest term first", () => {
  assert.deepEqual(segments("Встреча в четверг", searchTerms("встреча")), [
    { text: "Встреча", hit: true },
    { text: " в четверг", hit: false },
  ]);
  const s = segments("GitHub releases on github.com", searchTerms("github"));
  assert.equal(s.filter((x) => x.hit).length, 2);
  assert.deepEqual(searchTerms('"brown fox" quick'), ['"brown fox" quick', "brown fox", "quick"]);
  assert.deepEqual(segments("a+b (c)", searchTerms("+b (")).map((x) => x.hit), [false, true, false]);
  assert.deepEqual(segments("plain", []), [{ text: "plain", hit: false }]);
});
