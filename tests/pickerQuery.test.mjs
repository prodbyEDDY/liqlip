// Search string → FeedQuery (W1-E): tokens, quotes, Cyrillic, exclusions, unknown tokens as
// text, pending tokens, pill merge, chips and autocomplete.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { parseQuery, mergeFilters, extractChips, suggest, serializeToken, wordAt, dateBounds, chipToken, completeToken } = await importTs(
  "src/windows/picker/query.ts",
);

const NOW = new Date(2026, 10, 26, 15, 30).getTime();
const ctx = {
  now: NOW,
  apps: [
    { id: 1, exe: "c:\\program files\\google\\chrome\\application\\chrome.exe", name: "Google Chrome", iconUrl: "x" },
    { id: 2, exe: "c:\\users\\me\\telegram.exe", name: "Telegram" },
    { id: 3, exe: "c:\\windows\\explorer.exe", name: "Проводник" },
  ],
  folders: [
    { id: 10, name: "Работа" },
    { id: 11, name: "My stuff" },
  ],
};
const p = (s) => parseQuery(s, ctx);

test("plain text, Cyrillic and quoted phrases", () => {
  assert.deepEqual(p("встреча четверг").text, "встреча четверг");
  assert.deepEqual(p('"quick brown" fox').text, '"quick brown" fox', "phrases keep their quotes for the store");
  assert.deepEqual(p('"" fox').text, "fox", "an empty phrase is dropped");
  assert.deepEqual(p("  ").text, "");
  assert.deepEqual(p("hello").filter, {});
});

test("filter tokens: app, type (aliases, lists), in, is, date", () => {
  assert.deepEqual(p("app:chrome").filter, { appIds: [1] });
  assert.deepEqual(p("app:провод").filter, { appIds: [3] }, "Cyrillic substring of the app name");
  assert.deepEqual(p("type:link").filter, { kinds: ["link"] });
  assert.deepEqual(p("type:ссылки").filter, { kinds: ["link"] });
  assert.deepEqual(p("type:code,image").filter, { kinds: ["code", "image"] });
  assert.deepEqual(p("type:link type:file").filter, { kinds: ["link", "file"] });
  assert.deepEqual(p("in:работа").filter, { folderId: 10 });
  assert.deepEqual(p('in:"My stuff" note').filter, { folderId: 11 });
  assert.equal(p('in:"My stuff" note').text, "note");
  assert.deepEqual(p("is:fav").filter, { favorites: true });
  assert.equal(p("is:snippet").snippets, true);
  assert.equal(p("is:сниппеты").snippets, true);
  const today = new Date(NOW);
  today.setHours(0, 0, 0, 0);
  assert.deepEqual(p("date:today").filter, { since: today.getTime() });
  assert.deepEqual(p("date:вчера").filter, dateBounds("yesterday", NOW));
  assert.deepEqual(p("date:week").filter, { since: NOW - 7 * 86_400_000 });
  assert.equal(p("TYPE:Link").tokens[0].value, "link", "keys and values are case-insensitive");
});

test("W6-F: type:screenshot and its synonym type:shot (SPEC §3.4)", () => {
  for (const v of ["screenshot", "screenshots", "shot", "SHOTS", "скриншот", "снимки"]) {
    assert.deepEqual(p(`type:${v}`).filter, { kinds: ["screenshot"] }, v);
    assert.equal(p(`type:${v}`).tokens[0].value, "screenshot", "one canonical value for the chip");
  }
  assert.deepEqual(p("type:image").filter, { kinds: ["image"] }, "a token filters one kind; the Images pill shows both");
  assert.deepEqual(p("type:image,shot").filter, { kinds: ["image", "screenshot"] });
  assert.deepEqual(mergeFilters({ kinds: ["image", "screenshot"] }, p("type:shot").filter), { kinds: ["screenshot"] }, "Images pill + type:shot");
  assert.equal(mergeFilters({ kinds: ["screenshot"] }, p("type:link").filter), null);
  assert.deepEqual(suggest("type:sh", ctx).map((s) => s.insert), ["type:screenshot"]);
  assert.deepEqual(suggest("type:скр", ctx).map((s) => s.value), ["screenshot"]);
  assert.ok(suggest("type:", ctx).some((s) => s.value === "screenshot"), "listed with the other kinds");
  assert.equal(completeToken("type:sho", ctx), "type:screenshot");
  assert.deepEqual(extractChips("type:shot ", ctx).chips, ["type:shot"]);
});

test("exclusions: -word and -\"phrase\"", () => {
  const q = p('report -draft -"old version" -app:chrome');
  assert.equal(q.text, "report");
  assert.deepEqual(q.exclude, ["draft", "old version", "app:chrome"]);
  assert.equal(p("-").text, "-", "a lone minus is text");
});

test("unknown tokens and unusable values stay plain text", () => {
  assert.equal(p("https://example.com").text, "https://example.com");
  assert.equal(p("foo:bar baz").text, "foo:bar baz");
  assert.equal(p("type:banana next").text, "type:banana next");
  assert.deepEqual(p("type:banana next").filter, {});
});

test("an unfinished token at the end is neither filter nor text", () => {
  const q = p("invoice type:li");
  assert.equal(q.text, "invoice");
  assert.deepEqual(q.filter, {});
  assert.deepEqual(p("app:").filter, {});
  assert.equal(p("app:").text, "");
});

test("unknown app / folder make the result impossible (empty without a call)", () => {
  assert.equal(p("app:photoshop").impossible, true);
  assert.equal(p("in:nowhere").impossible, true);
  assert.equal(p("app:chrome in:работа").impossible, false);
});

test("mergeFilters intersects kinds, rejects contradictions", () => {
  assert.deepEqual(mergeFilters({ kinds: ["text", "contact"] }, { kinds: ["contact", "link"] }), { kinds: ["contact"] });
  assert.equal(mergeFilters({ kinds: ["link"] }, { kinds: ["image"] }), null);
  assert.equal(mergeFilters({ folderId: 1 }, { folderId: 2 }), null);
  assert.deepEqual(mergeFilters({ favorites: true }, { appIds: [1] }), { favorites: true, appIds: [1] });
  assert.deepEqual(mergeFilters({}, {}), {});
});

test("chips: only finished valid tokens leave the input", () => {
  assert.deepEqual(extractChips("type:link ", ctx), { chips: ["type:link"], rest: "" });
  assert.deepEqual(extractChips("hello app:chrome world", ctx), { chips: ["app:chrome"], rest: "hello world" });
  assert.deepEqual(extractChips("type:link", ctx), { chips: [], rest: "type:link" }, "still typing");
  assert.deepEqual(extractChips("type:nope x", ctx), { chips: [], rest: "type:nope x" });
  assert.deepEqual(extractChips("foo ", ctx), { chips: [], rest: "foo " });
  assert.equal(chipToken('in:"My stuff"', ctx).value, "My stuff");
});

test("serializeToken quotes values with spaces", () => {
  assert.equal(serializeToken("in", "My stuff"), 'in:"My stuff"');
  assert.equal(serializeToken("app", "chrome"), "app:chrome");
});

test("autocomplete after ':' — apps, kinds, folders, flags, dates", () => {
  const apps = suggest("app:", ctx);
  assert.deepEqual(apps.map((s) => s.insert), ["app:chrome", "app:telegram", "app:explorer"]);
  assert.equal(apps[0].label, "Google Chrome");
  assert.deepEqual(suggest("app:tel", ctx).map((s) => s.value), ["telegram"]);
  assert.deepEqual(suggest("type:im", ctx).map((s) => s.value), ["image"]);
  assert.deepEqual(suggest("type:сс", ctx).map((s) => s.value), ["link"]);
  assert.deepEqual(suggest("in:my", ctx).map((s) => s.insert), ['in:"My stuff"']);
  assert.deepEqual(suggest("is:", ctx).map((s) => s.value), ["fav", "snippet"]);
  assert.deepEqual(suggest("date:y", ctx).map((s) => s.value), ["yesterday", "today"], "prefix matches first");
  assert.deepEqual(suggest("date:вч", ctx).map((s) => s.value), ["yesterday"]);
  assert.deepEqual(suggest("hello", ctx), []);
  assert.deepEqual(suggest("foo:", ctx), []);
});

test("Space completes a unique-prefix token; otherwise leaves it alone", () => {
  assert.equal(completeToken("type:te", ctx), "type:text");
  assert.equal(completeToken("type:сс", ctx), "type:link");
  assert.equal(completeToken("app:tele", ctx), null, "app values are open-ended: already a valid token");
  assert.equal(completeToken("type:link", ctx), null, "already valid");
  assert.equal(completeToken("type:zz", ctx), null);
  assert.equal(completeToken("type:", ctx), null, "nothing typed yet");
  assert.equal(completeToken("hello", ctx), null);
});

test("wordAt finds the word under the caret", () => {
  assert.deepEqual(wordAt("hello app:ch", 12), { word: "app:ch", start: 6 });
  assert.deepEqual(wordAt("hello ", 6), { word: "", start: 6 });
});
