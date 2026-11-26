// Shelf pills (W1-E): order and counters (SPEC §2.1), drop targets, cycling, folder reorder —
// replaces Liquid Tools tests/libraryPills.test.mjs.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { buildPills, cyclePill, folderDropBefore, folderIdOf, pillFilter, pillLabelKey } = await importTs("src/windows/picker/pills.ts");

const counts = {
  all: 20,
  kinds: { text: 5, link: 3, code: 2, color: 1, contact: 2, image: 4, screenshot: 3, file: 3 },
  favorites: 2,
  snippets: 6,
  folders: { 7: 4, 9: 1 },
};
const folders = [
  { id: 9, name: "Design", position: 2, count: 0 },
  { id: 7, name: "Работа", position: 1, count: 0 },
];

test("order: all, types, favorites, snippets, folders by position", () => {
  const pills = buildPills(counts, folders);
  assert.deepEqual(
    pills.map((p) => p.id),
    ["all", "text", "link", "image", "screenshot", "file", "code", "color", "favorites", "snippets", "folder:7", "folder:9"],
  );
  assert.equal(pills.find((p) => p.id === "folder:7").name, "Работа");
});

test("counters: Text = text + contact; folders from counts", () => {
  const pills = buildPills(counts, folders);
  const c = (id) => pills.find((p) => p.id === id).count;
  assert.equal(c("all"), 20);
  assert.equal(c("text"), 7);
  assert.equal(c("snippets"), 6);
  assert.equal(c("folder:7"), 4);
  assert.equal(buildPills(null, []).every((p) => p.count === 0), true);
});

test("W6-F: Images count pictures and screenshots; Screenshots its own pill after Images", () => {
  const pills = buildPills(counts, folders);
  const c = (id) => pills.find((p) => p.id === id).count;
  assert.equal(c("image"), 7, "image 4 + screenshot 3 (SPEC §9.6: shots show in Images too)");
  assert.equal(c("screenshot"), 3);
  assert.deepEqual(pillFilter("image"), { kinds: ["image", "screenshot"] });
  assert.deepEqual(pillFilter("screenshot"), { kinds: ["screenshot"] });
  assert.equal(buildPills({ ...counts, kinds: { ...counts.kinds, screenshot: undefined } }, []).find((p) => p.id === "image").count, 4, "older counts without the key");
  assert.equal(pillLabelKey("screenshot"), "picker.shot.pill");
  assert.equal(pillLabelKey("image"), "picker.pill.image");
});

test("filters and modes", () => {
  assert.deepEqual(pillFilter("text"), { kinds: ["text", "contact"] });
  assert.deepEqual(pillFilter("favorites"), { favorites: true });
  assert.deepEqual(pillFilter("folder:7"), { folderId: 7 });
  assert.deepEqual(pillFilter("all"), {});
  assert.equal(buildPills(counts, folders).find((p) => p.id === "snippets").mode, "snippets");
  assert.equal(folderIdOf("folder:12"), 12);
  assert.equal(folderIdOf("link"), null);
});

test("only Favorites and folders accept drops", () => {
  const pills = buildPills(counts, folders);
  assert.deepEqual(pills.filter((p) => p.droppable).map((p) => p.id), ["favorites", "folder:7", "folder:9"]);
});

test("Ctrl+Tab cycling wraps", () => {
  const pills = buildPills(counts, folders);
  assert.equal(cyclePill(pills, "all", 1), "text");
  assert.equal(cyclePill(pills, "all", -1), "folder:9");
  assert.equal(cyclePill(pills, "folder:9", 1), "all");
});

test("folder drop position → beforeId", () => {
  assert.equal(folderDropBefore([1, 2, 3], 1, 0), 2);
  assert.equal(folderDropBefore([1, 2, 3], 1, 2), null, "past the end");
  assert.equal(folderDropBefore([1, 2, 3], 3, 0), 1);
  assert.equal(folderDropBefore([1, 2, 3], 2, 1), 3);
});
