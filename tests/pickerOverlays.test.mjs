// Shelf layers (W2-C): the actions menu model (items per kind / selection / formats, search,
// shortcuts) and the Quick Look text runs (search hits + invisible characters).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const m = await importTs("src/windows/picker/overlays/menuModel.ts", "src/windows/picker/overlays/textView.ts");
const { buildMenu, searchMenu, chordNode, normalize, textRuns } = m;

const clip = (over = {}) => ({ id: 1, kind: "text", formats: ["text"], favorite: false, folderId: null, meta: {}, ocr: "none", fileCount: null, ...over });
const base = { folders: [{ id: 7, name: "Work" }, { id: 8, name: "Личное" }], appName: "Chrome", ocrEnabled: true, separator: "newline" };
const ids = (nodes) => nodes.map((n) => n.id);
const find = (nodes, id) => nodes.find((n) => n.id === id);

test("single text record: every SPEC §3.7 group, in order, gaps between groups", () => {
  const menu = buildMenu({ ...base, clips: [clip()] });
  assert.deepEqual(ids(menu), [
    "paste",
    "pastePlain",
    "pasteKeepOpen",
    "copy",
    "preview",
    "transform",
    "saveSnippet",
    "favorite",
    "folder",
    "rename",
    "edit",
    "stack",
    "showStack",
    "moreFromApp",
    "delete",
    "deleteRecent",
  ]);
  assert.deepEqual(
    menu.filter((n) => n.gapBefore).map((n) => n.id),
    ["copy", "preview", "transform", "favorite", "stack", "moreFromApp", "delete"],
  );
  assert.equal(find(menu, "moreFromApp").vars.app, "Chrome");
  assert.equal(find(menu, "transform").children.length, 17);
  assert.deepEqual(ids(find(menu, "transform").children[0].children), ["t.upper.paste", "t.upper.copy", "t.upper.save"]);
  assert.deepEqual(ids(find(menu, "deleteRecent").children), ["recent.5", "recent.15", "recent.30", "recent.60", "recent.1440"]);
});

test("Paste as… / Copy as… only with more than one format, listing only available ones", () => {
  const rich = buildMenu({ ...base, clips: [clip({ formats: ["text", "html", "rtf"] })] });
  assert.deepEqual(ids(find(rich, "pasteAs").children), ["pasteAs.html", "pasteAs.rtf", "pasteAs.text"]);
  assert.deepEqual(ids(find(rich, "copyAs").children), ["copyAs.html", "copyAs.rtf", "copyAs.text"]);
  const plain = buildMenu({ ...base, clips: [clip()] });
  assert.equal(find(plain, "pasteAs"), undefined);
  assert.equal(find(plain, "copyAs"), undefined);
});

test("kind-specific items: link, file, image, colour", () => {
  const link = ids(buildMenu({ ...base, clips: [clip({ kind: "link" })] }));
  assert.ok(link.includes("open") && !link.includes("reveal") && !link.includes("openWith"));
  const file = buildMenu({ ...base, clips: [clip({ kind: "file", formats: ["files", "text"], fileCount: 3 })] });
  assert.ok(["open", "openWith", "reveal", "copyPath"].every((id) => ids(file).includes(id)));
  assert.equal(find(file, "copyPath").label, "picker.menu.copyPaths");
  assert.ok(!ids(file).includes("transform") && !ids(file).includes("edit"));
  const image = buildMenu({ ...base, clips: [clip({ kind: "image", formats: ["png"] })] });
  assert.ok(ids(image).includes("ocr") && !ids(image).includes("pastePlain") && ids(image).includes("reveal"));
  const color = buildMenu({ ...base, clips: [clip({ kind: "color", meta: { color: { hex: "#3B82F6", rgb: "rgb(59, 130, 246)", hsl: "hsl(217, 91%, 60%)" } } })] });
  assert.deepEqual(
    color.filter((n) => n.cmd === "copyColor").map((n) => [n.arg, n.value]),
    [
      ["#3B82F6", "#3B82F6"],
      ["rgb(59, 130, 246)", "rgb(59, 130, 246)"],
      ["hsl(217, 91%, 60%)", "hsl(217, 91%, 60%)"],
    ],
  );
});

test("OCR item: off when recognition is off and the image isn't recognized yet", () => {
  const off = find(buildMenu({ ...base, ocrEnabled: false, clips: [clip({ kind: "image", formats: ["png"] })] }), "ocr");
  assert.equal(off.disabled, true);
  assert.equal(off.hint, "picker.menu.ocrOff");
  const done = find(buildMenu({ ...base, ocrEnabled: false, clips: [clip({ kind: "image", formats: ["png"], ocr: "done" })] }), "ocr");
  assert.equal(done.disabled, false);
});

test("folders submenu: current folder checked, New folder…, Remove from folder only when filed", () => {
  const filed = find(buildMenu({ ...base, clips: [clip({ folderId: 8 })] }), "folder").children;
  assert.deepEqual(ids(filed), ["folder.7", "folder.8", "folderNew", "folderRemove"]);
  assert.equal(filed[1].checked, true);
  assert.equal(filed[1].text, "Личное");
  assert.equal(filed[2].gapBefore, true);
  const loose = find(buildMenu({ ...base, folders: [], clips: [clip()] }), "folder").children;
  assert.deepEqual(ids(loose), ["folderNew"]);
  assert.ok(!loose[0].gapBefore);
});

test("selection of several: batch items, remembered separator checked, Delete N", () => {
  const menu = buildMenu({ ...base, separator: "tab", clips: [clip({ id: 1 }), clip({ id: 2, favorite: true }), clip({ id: 3, kind: "image" })] });
  assert.deepEqual(ids(menu), ["pasteAll", "pasteAllWith", "copyAll", "merge", "favorite", "folder", "stack", "showStack", "delete"]);
  assert.equal(find(menu, "pasteAll").arg, "tab");
  assert.equal(find(menu, "pasteAll").hint, "picker.menu.sep.tab");
  assert.deepEqual(
    find(menu, "pasteAllWith").children.map((n) => [n.arg, !!n.checked]),
    [
      ["newline", false],
      ["space", false],
      ["tab", true],
      ["none", false],
    ],
  );
  assert.equal(find(menu, "favorite").label, "picker.menu.favorite", "not all are favorites → add");
  assert.deepEqual(find(menu, "delete").vars, { n: 3 });
  const images = buildMenu({ ...base, clips: [clip({ id: 1, kind: "image" }), clip({ id: 2, kind: "image" })] });
  assert.ok(!ids(images).includes("merge"), "images alone have nothing to merge");
});

test("snippet menu", () => {
  assert.deepEqual(ids(buildMenu({ ...base, snippet: true, clips: [] })), [
    "paste",
    "pastePlain",
    "pasteKeepOpen",
    "copy",
    "preview",
    "snippetEdit",
    "snippetNew",
    "snippetSettings",
    "snippetDelete",
  ]);
  assert.deepEqual(buildMenu({ ...base, clips: [] }), []);
});

test("search: across submenus, prefix first, every word required, generic transform rows skipped", () => {
  const menu = buildMenu({ ...base, clips: [clip({ formats: ["text", "html"] })] });
  const en = (n) => [n.text ?? n.label.split(".").pop()];
  const hits = searchMenu(menu, "json", en);
  assert.deepEqual(
    hits.map((h) => [h.node.id, h.path.map((p) => p.id)]),
    [
      ["t.jsonFormat", ["transform"]],
      ["t.jsonMinify", ["transform"]],
    ],
  );
  const labelled = (n) => [{ "t.upper": "ВЕРХНИЙ РЕГИСТР", paste: "Вставить", pastePlain: "Вставить как обычный текст", "folder.8": "Личное" }[n.id] ?? n.id];
  assert.deepEqual(
    searchMenu(menu, "вставить", labelled).map((h) => h.node.id),
    ["paste", "pastePlain"],
  );
  assert.deepEqual(searchMenu(menu, "как текст", labelled).map((h) => h.node.id), ["pastePlain"], "words in any position");
  assert.deepEqual(searchMenu(menu, "лич", labelled).map((h) => h.node.id), ["folder.8"]);
  assert.equal(searchMenu(menu, "paste", (n) => [n.id]).some((h) => h.node.noSearch), false);
  assert.equal(searchMenu(menu, "", en).length, menu.length, "empty query = the level itself");
  assert.equal(normalize("  Ёлка   ПРИВЕТ "), "елка привет");
});

test("chords: physical keys with exact modifiers; disabled items ignored", () => {
  const menu = buildMenu({ ...base, clips: [clip()] });
  const key = (code, mods = {}) => ({ code, key: "", ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });
  assert.equal(chordNode(menu, key("KeyD", { ctrlKey: true })).id, "favorite");
  assert.equal(chordNode(menu, key("Enter", { ctrlKey: true })).id, "copy");
  assert.equal(chordNode(menu, key("NumpadEnter", { shiftKey: true })).id, "pastePlain");
  assert.equal(chordNode(menu, key("KeyO", { ctrlKey: true, shiftKey: true })), null, "a text record has no reveal");
  assert.equal(chordNode(menu, key("KeyD")), null);
  assert.equal(chordNode(menu, key("Delete")).id, "delete");
  const image = buildMenu({ ...base, ocrEnabled: false, clips: [clip({ kind: "image", formats: ["png"] })] });
  assert.equal(chordNode(image, key("KeyO", { ctrlKey: true, shiftKey: true })).id, "reveal");
});

test("text runs: hits, and one run per invisible character with copy-safe text", () => {
  assert.deepEqual(textRuns("a b", [], false), [{ text: "a b", hit: false }]);
  assert.deepEqual(textRuns("Мир мир", ["мир"], false), [
    { text: "Мир", hit: true },
    { text: " ", hit: false },
    { text: "мир", hit: true },
  ]);
  assert.deepEqual(textRuns("a  b\r\n\tc d​", [], true), [
    { text: "a", hit: false },
    { text: " ", hit: false, ws: "space" },
    { text: " ", hit: false, ws: "space" },
    { text: "b", hit: false },
    { text: "\r\n", hit: false, ws: "nl" },
    { text: "\t", hit: false, ws: "tab" },
    { text: "c", hit: false },
    { text: " ", hit: false, ws: "nbsp" },
    { text: "d", hit: false },
    { text: "​", hit: false, ws: "zw" },
  ]);
  const joined = textRuns("x y\nz", ["y"], true)
    .map((r) => r.text)
    .join("");
  assert.equal(joined, "x y\nz", "runs cover the text exactly");
});

test("snippet menu: Delete asks first — its chord is Delete, like a snippet card (W3)", () => {
  const menu = buildMenu({ ...base, snippet: true, clips: [] });
  const del = { code: "Delete", key: "Delete", ctrlKey: false, shiftKey: false, altKey: false, metaKey: false };
  assert.equal(chordNode(menu, del)?.id, "snippetDelete");
  assert.equal(find(menu, "snippetDelete").keys, "Del");
});
