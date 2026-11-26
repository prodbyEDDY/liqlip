// Pure logic of the settings window: src/windows/settings/logic.ts.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const L = await importTs("src/windows/settings/logic.ts");

test("formatBytes: binary units, one decimal under 10, locale separator, no 1024 KB", () => {
  const f = (b, loc) => {
    const { n, unit } = L.formatBytes(b, loc);
    return `${n} ${unit}`;
  };
  assert.equal(f(0), "0 bytes");
  assert.equal(f(-5), "0 bytes");
  assert.equal(f(NaN), "0 bytes");
  assert.equal(f(512), "512 bytes");
  assert.equal(f(1023), "1,023 bytes");
  assert.equal(f(1024), "1 kb");
  assert.equal(f(1536), "1.5 kb");
  assert.equal(f(1536, "ru"), "1,5 kb");
  assert.equal(f(10 * 1024 + 300), "10 kb");
  assert.equal(f(1024 * 1024 - 1), "1 mb", "1023.999 KB rounds up into MB");
  assert.equal(f(5.25 * 1024 * 1024), "5.3 mb");
  assert.equal(f(734 * 1024 * 1024), "734 mb");
  assert.equal(f(3.2 * 1024 ** 3), "3.2 gb");
  assert.equal(f(2048 * 1024 ** 3), "2,048 gb", "GB is the largest unit");
});

test("formatCount groups digits per locale", () => {
  assert.equal(L.formatCount(1240), "1,240");
  assert.equal(L.formatCount(1240, "ru").replace(/\s/g, " "), "1 240");
  assert.equal(L.formatCount(-3), "0");
});

test("folder names: trim, length, control chars, case-insensitive uniqueness", () => {
  const existing = [
    { id: 1, name: "Работа" },
    { id: 2, name: "Design" },
  ];
  assert.equal(L.validateFolderName("  Inbox ", existing), null);
  assert.equal(L.validateFolderName("   ", existing), "empty");
  assert.equal(L.validateFolderName("x".repeat(65), existing), "tooLong");
  assert.equal(L.validateFolderName("😀".repeat(64), existing), null, "counts code points, not UTF-16 units");
  assert.equal(L.validateFolderName("a\tb", existing), "control");
  assert.equal(L.validateFolderName("design", existing), "exists");
  assert.equal(L.validateFolderName("РАБОТА", existing), "exists");
  assert.equal(L.validateFolderName("Design", existing, 2), null, "renaming to itself is fine");
});

test("snippet name and body", () => {
  assert.equal(L.validateSnippetName("Signature"), null);
  assert.equal(L.validateSnippetName(""), "empty");
  assert.equal(L.validateSnippetName("x".repeat(101)), "tooLong");
  assert.equal(L.validateSnippetName("a\nb"), "control");
  assert.equal(L.validateSnippetBody("hello {cursor}"), null);
  assert.equal(L.validateSnippetBody("я".repeat(1024 * 1024 + 1)), "tooLong", "2 MB of UTF-8, not chars");
});

test("exe names: file name, lower case, quotes, stems", () => {
  assert.equal(L.exeFileName("C:\\Program Files\\KeePass Password Safe 2\\KeePass.exe"), "keepass.exe");
  assert.equal(L.exeFileName('"C:/Tools/WT.exe"'), "wt.exe");
  assert.equal(L.exeFileName("bitwarden.exe"), "bitwarden.exe");
  assert.ok(L.sameExe("c:\\x\\Code.exe", "CODE.EXE"));
  assert.equal(L.exeStem("KeePassXC.exe"), "keepassxc");
});

test("hotkey checks: duplicates, typing keys need Ctrl/Alt/Win, modifiers alone", () => {
  const hk = { summon: "Alt + V", stackNext: "Alt + Shift + V", pauseToggle: "", ignoreNext: "" };
  assert.deepEqual(L.hotkeyIssue("Alt + Shift + V", "summon", hk), { kind: "duplicate", other: "stackNext" });
  assert.equal(L.hotkeyIssue("Alt + V", "summon", hk), null, "its own value is not a duplicate");
  assert.deepEqual(L.hotkeyIssue("V", "pauseToggle", hk), { kind: "needsModifier" });
  assert.deepEqual(L.hotkeyIssue("Shift + V", "pauseToggle", hk), { kind: "needsModifier" });
  assert.deepEqual(L.hotkeyIssue("Space", "pauseToggle", hk), { kind: "needsModifier" });
  assert.deepEqual(L.hotkeyIssue("Ctrl + LShift", "pauseToggle", hk), { kind: "needsKey" });
  assert.equal(L.hotkeyIssue("F9", "pauseToggle", hk), null);
  assert.equal(L.hotkeyIssue("Shift + F9", "pauseToggle", hk), null);
  assert.equal(L.hotkeyIssue("Pause", "pauseToggle", hk), null);
  assert.equal(L.hotkeyIssue("Ctrl + Alt + P", "pauseToggle", hk), null);
  assert.equal(L.hotkeyIssue("Super + Shift + Q", "ignoreNext", hk), null);
  assert.equal(L.hotkeyIssue("", "ignoreNext", hk), null, "clearing is always fine");
  assert.deepEqual(L.comboKeys("Alt + Shift + V"), ["Alt", "Shift", "V"]);
  assert.deepEqual(L.comboKeys(""), []);
});

test("hotkey defaults for 'Reset' equal the Rust defaults (fixture)", async () => {
  const fixture = JSON.parse(await readFile(new URL("./fixtures/settings.default.json", import.meta.url), "utf8"));
  assert.deepEqual({ ...L.HOTKEY_DEFAULTS }, fixture.hotkeys);
  // W5: the five `shot*` slots get their own group in the section (W6-F), not the global list.
  const global = Object.keys(fixture.hotkeys).filter((k) => !k.startsWith("shot"));
  assert.deepEqual([...L.HOTKEY_ACTIONS].sort(), global.sort());
});

test("field errors map to i18n keys", () => {
  assert.equal(L.fieldErrorKey("hotkeys.summon", "taken"), "settings.error.hotkeyTaken");
  assert.equal(L.fieldErrorKey("hotkeys.ignoreNext", "invalid"), "settings.error.hotkeyInvalid");
  assert.equal(L.fieldErrorKey("picker.pasteKeys", "invalid"), "settings.error.pasteKeys");
  assert.equal(L.fieldErrorKey("privacy.ignoredApps", "invalid"), "settings.error.ignoredApps");
  assert.equal(L.fieldErrorKey("picker.monitor", "invalid"), "settings.error.monitor");
  assert.equal(L.fieldErrorKey("ocr.language", "invalid"), "settings.error.ocrLanguage");
  assert.equal(L.fieldErrorKey("storage.maxEntries", "invalid"), "settings.error.invalid");
  assert.equal(L.fieldErrorKey("general.autostart", "invalid"), "settings.error.autostart", "W3-D: its own words");
  assert.deepEqual(
    L.errorsByField([
      { field: "hotkeys.summon", code: "taken" },
      { field: "storage.maxEntries", code: "invalid" },
    ]),
    { "hotkeys.summon": "taken", "storage.maxEntries": "invalid" },
  );
});

test("every key fieldErrorKey can return exists in en.ts", async () => {
  const en = await readFile(new URL("../src/lib/i18n/en.ts", import.meta.url), "utf8");
  for (const [f, c] of [
    ["hotkeys.summon", "taken"],
    ["hotkeys.summon", "invalid"],
    ["picker.pasteKeys", "invalid"],
    ["privacy.ignoredApps", "invalid"],
    ["picker.monitor", "invalid"],
    ["ocr.language", "invalid"],
    ["general.autostart", "invalid"],
    ["x.y", "invalid"],
  ]) {
    assert.ok(en.includes(`"${L.fieldErrorKey(f, c)}":`), L.fieldErrorKey(f, c));
  }
});

test("paths: get/set immutably, overlap", () => {
  const s = { a: { b: { c: 1 }, list: [1, 2] }, z: { k: true } };
  const t = L.setPath(s, "a.b.c", 2);
  assert.equal(t.a.b.c, 2);
  assert.equal(s.a.b.c, 1, "original untouched");
  assert.equal(t.z, s.z, "untouched branches are shared");
  assert.equal(L.getPath(t, "a.b.c"), 2);
  assert.equal(L.getPath(t, "a.nope.c"), undefined);
  assert.deepEqual(L.setPath(s, "a.list", [3]).a.list, [3]);
  assert.ok(L.pathsOverlap("hotkeys.summon", "hotkeys.summon"));
  assert.ok(L.pathsOverlap("hotkeys", "hotkeys.summon"));
  assert.ok(L.pathsOverlap("picker.pasteKeys.0", "picker.pasteKeys"));
  assert.ok(!L.pathsOverlap("hotkeys.summon", "hotkeys.stackNext"));
  assert.ok(!L.pathsOverlap("picker.position", "picker.positionX"));
});

test("pause state: recording, indefinite, until (minutes left rounds up)", () => {
  const now = 1_000_000;
  assert.deepEqual(L.pauseState(null, now), { kind: "recording" });
  assert.deepEqual(L.pauseState({ pausedUntil: null, ignoreNext: false, collectToStack: false }, now), { kind: "recording" });
  assert.deepEqual(L.pauseState({ pausedUntil: -1, ignoreNext: false, collectToStack: false }, now), { kind: "indefinite" });
  assert.deepEqual(L.pauseState({ pausedUntil: now + 61_000, ignoreNext: false, collectToStack: false }, now), {
    kind: "until",
    at: now + 61_000,
    minutesLeft: 2,
  });
  assert.deepEqual(L.pauseState({ pausedUntil: now - 1, ignoreNext: false, collectToStack: false }, now), { kind: "recording" });
});

test("options keep a custom current value; navigation helpers", () => {
  const opts = [
    { value: "1000", label: "1 000" },
    { value: "5000", label: "5 000" },
  ];
  assert.equal(L.withCurrent(opts, "5000", (v) => v), opts);
  assert.deepEqual(L.withCurrent(opts, "3000", (v) => `${v}*`).at(-1), { value: "3000", label: "3000*" });
  assert.equal(L.stepIndex(0, -1, 9), 8);
  assert.equal(L.stepIndex(8, 1, 9), 0);
  assert.equal(L.stepIndex(3, 1, 0), 0);
  const keys = ["general", "picker", "about"];
  assert.equal(L.resolveSection("Picker", keys), "picker");
  assert.equal(L.resolveSection("nope", keys), null);
  assert.equal(L.resolveSection(null, keys), null);
  assert.deepEqual(L.moveBefore([1, 2, 3, 4], 4, 2), [1, 4, 2, 3]);
  assert.deepEqual(L.moveBefore([1, 2, 3, 4], 1, null), [2, 3, 4, 1]);
  assert.deepEqual(L.moveBefore([1, 2, 3], 2, 9), [1, 2, 3], "unknown anchor → unchanged");
});

test("import / backup errors map to their own messages; a cancelled dialog says nothing", () => {
  assert.equal(L.importErrorKey("invalid: cancelled"), "cancelled");
  assert.equal(L.importErrorKey("invalid: not a Liquid Tools database"), "settings.import.error.lt");
  assert.equal(L.importErrorKey("invalid: unknown Liquid Tools database format"), "settings.import.error.lt");
  assert.equal(L.importErrorKey("invalid: the Liquid Tools database is damaged"), "settings.import.error.lt");
  assert.equal(L.importErrorKey("invalid: not a Liqlip backup"), "settings.import.error.notBackup");
  assert.equal(L.importErrorKey("invalid: the backup was made by a newer Liqlip"), "settings.import.error.newer");
  assert.equal(L.importErrorKey("invalid: damaged backup (crc)"), "settings.import.error.backupDamaged");
  assert.equal(L.importErrorKey("invalid: the backup database is damaged"), "settings.import.error.backupDamaged");
  assert.equal(L.importErrorKey("busy: an import or a backup is already running"), "settings.import.error.busy");
  assert.equal(L.importErrorKey("failed: disk full"), null);
  assert.equal(L.importErrorKey("invalid: empty text"), null);
});

// W3-D
test("snippet autosave name: the draft when valid, else the last saved name", () => {
  assert.equal(L.snippetSaveName("  Signature ", "Old"), "Signature");
  assert.equal(L.snippetSaveName("", "Old"), "Old", "an emptied name never blocks the body");
  assert.equal(L.snippetSaveName("   ", "Old"), "Old");
  assert.equal(L.snippetSaveName("x".repeat(101), "Old"), "Old");
  assert.equal(L.snippetSaveName("a\u0001b", "Old"), "Old");
});

test("chord labels say Win, not Super", () => {
  assert.equal(L.comboLabel("Super + Shift + V"), "Win + Shift + V");
  assert.equal(L.comboLabel("Alt + V"), "Alt + V");
  assert.equal(L.comboLabel(""), "");
});

test("hotkey suggestions skip chords in use, the rejected one and the current value", () => {
  const hk = { summon: "Alt + V", stackNext: "Alt + Shift + V", pauseToggle: "", ignoreNext: "" };
  assert.deepEqual(L.hotkeySuggestions("summon", hk), ["Ctrl + Alt + V", "Alt + Shift + C"]);
  assert.deepEqual(L.hotkeySuggestions("summon", hk, ["Ctrl + Alt + V"]), ["Alt + Shift + C", "Ctrl + Shift + Space"]);
  assert.deepEqual(
    L.hotkeySuggestions("summon", { ...hk, pauseToggle: "Ctrl + Alt + V" }),
    ["Alt + Shift + C", "Ctrl + Shift + Space"],
    "a chord another Liqlip action uses is not offered",
  );
  assert.deepEqual(L.hotkeySuggestions("summon", { ...hk, summon: "Ctrl + Alt + V" }), ["Alt + Shift + C", "Ctrl + Shift + Space"]);
  assert.equal(L.hotkeySuggestions("stackNext", hk).length, 2);
  assert.deepEqual(L.hotkeySuggestions("pauseToggle", hk), [], "unset-by-default actions get none");
  for (const a of L.HOTKEY_ACTIONS) for (const c of L.hotkeySuggestions(a, hk, [], 9)) assert.equal(L.hotkeyIssue(c, a, hk), null, c);
});

// ── W6-F: screenshots (SPEC §9.1, §9.7; ARCHITECTURE §18.10) ─────────────────

const HK9 = { summon: "Alt + V", stackNext: "Alt + Shift + V", pauseToggle: "", ignoreNext: "", shotRegion: "PrtSc", shotScreen: "Ctrl + PrtSc", shotWindow: "", shotRepeat: "", shotText: "" };

test("screenshot hotkeys: their own group, labels from the screenshot block, duplicates across all nine", async () => {
  const fixture = JSON.parse(await readFile(new URL("./fixtures/settings.default.json", import.meta.url), "utf8"));
  assert.deepEqual([...L.SHOT_HOTKEY_ACTIONS], Object.keys(fixture.hotkeys).filter((k) => k.startsWith("shot")), "slot order");
  assert.deepEqual([...L.ALL_HOTKEY_ACTIONS].sort(), Object.keys(fixture.hotkeys).sort());
  assert.equal(L.hotkeyLabelKey("shotRegion"), "settings.shot.hotkey.shotRegion");
  assert.equal(L.hotkeyDescKey("shotText"), "settings.shot.hotkey.shotText.desc");
  assert.equal(L.hotkeyLabelKey("summon"), "settings.hotkeys.summon");
  const en = await readFile(new URL("../src/lib/i18n/en.ts", import.meta.url), "utf8");
  for (const a of L.ALL_HOTKEY_ACTIONS) {
    assert.ok(en.includes(`"${L.hotkeyLabelKey(a)}":`), a);
    assert.ok(en.includes(`"${L.hotkeyDescKey(a)}":`), `${a}.desc`);
  }
  assert.deepEqual(L.hotkeyIssue("PrtSc", "shotScreen", HK9), { kind: "duplicate", other: "shotRegion" });
  assert.deepEqual(L.hotkeyIssue("Alt + V", "shotText", HK9), { kind: "duplicate", other: "summon" }, "global and screenshot slots share one pool");
  assert.deepEqual(L.hotkeyIssue("Ctrl + PrtSc", "pauseToggle", HK9), { kind: "duplicate", other: "shotScreen" });
  assert.equal(L.hotkeyIssue("PrtSc", "shotRegion", HK9), null, "bare PrtSc is a valid standalone key");
  assert.equal(L.hotkeyIssue("Shift + PrtSc", "shotWindow", HK9), null);
  assert.deepEqual(L.hotkeyIssue("S", "shotText", HK9), { kind: "needsModifier" });
});

test("screenshot hotkeys: errors in plain words, free PrtSc variants when taken", () => {
  assert.equal(L.fieldErrorKey("hotkeys.shotRegion", "taken"), "settings.shot.error.hotkeyTaken");
  assert.equal(L.fieldErrorKey("hotkeys.shotText", "invalid"), "settings.error.hotkeyInvalid");
  assert.equal(L.fieldErrorKey("screenshot.fileName", "invalid"), "settings.shot.error.fileName");
  assert.equal(L.fieldErrorKey("screenshot.folder", "invalid"), "settings.shot.error.folder");
  assert.equal(L.fieldErrorKey("screenshot.jpgQuality", "invalid"), "settings.error.invalid");
  assert.deepEqual(L.hotkeySuggestions("shotRegion", HK9), ["Shift + PrtSc", "Ctrl + Shift + PrtSc"]);
  assert.deepEqual(L.hotkeySuggestions("shotScreen", { ...HK9, shotRegion: "Ctrl + Shift + PrtSc" }), ["Shift + PrtSc", "Alt + Shift + F"], "a chord another slot uses is not offered");
  assert.deepEqual(L.hotkeySuggestions("shotWindow", HK9), []);
  for (const a of L.SHOT_HOTKEY_ACTIONS) for (const c of L.hotkeySuggestions(a, HK9, [], 9)) assert.equal(L.hotkeyIssue(c, a, HK9), null, c);
});

test("every key fieldErrorKey returns for screenshot fields exists in en.ts", async () => {
  const en = await readFile(new URL("../src/lib/i18n/en.ts", import.meta.url), "utf8");
  for (const f of ["hotkeys.shotRegion", "screenshot.fileName", "screenshot.folder"]) {
    for (const c of ["taken", "invalid"]) assert.ok(en.includes(`"${L.fieldErrorKey(f, c)}":`), `${f} ${c}`);
  }
});

test("file name template: validation mirrors settings::validate", () => {
  assert.equal(L.shotNameIssue("Liqlip {date} {time}"), null);
  assert.equal(L.shotNameIssue("  Shot {n}  "), null, "trimmed like Rust");
  assert.equal(L.shotNameIssue(""), "empty");
  assert.equal(L.shotNameIssue("   "), "empty");
  assert.equal(L.shotNameIssue("x".repeat(120)), null);
  assert.equal(L.shotNameIssue("x".repeat(121)), "tooLong");
  assert.equal(L.shotNameIssue(` ${"я".repeat(120)} `), null, "counts characters after trimming");
  for (const c of ["<", ">", ":", '"', "/", "\\", "|", "?", "*"]) assert.equal(L.shotNameIssue(`Shot ${c} {date}`), "reserved", c);
  assert.equal(L.shotNameIssue("Shot\n{date}"), "control");
  assert.equal(L.shotNameIssue("Shot\u0085"), "control", "C1 controls too (char::is_control)");
});

test("file name template: live preview (Windows-style date and time, no ':')", () => {
  const at = new Date(2026, 10, 6, 9, 5, 7);
  assert.equal(L.shotFileName("Liqlip {date} {time}", at, "Chrome", 1), "Liqlip 2026-11-06 090507");
  assert.equal(L.shotFileName("{app} — {n}", at, "Telegram", 12), "Telegram — 12");
  assert.equal(L.shotFileName("{date}{date} {foo}", at, "x", 1), "2026-11-062026-11-06 {foo}", "unknown placeholders stay as typed");
  assert.equal(L.shotFileName("  Shot  ", at, "x", 1), "Shot");
  assert.ok(!L.shotFileName("{date} {time}", at, "x", 1).includes(":"));
});

test("JPG quality and thumbnail timeout ranges", () => {
  for (const q of [60, 90, 100]) assert.equal(L.jpgQualityValid(q), true, String(q));
  for (const q of [59, 101, 90.5, Number.NaN]) assert.equal(L.jpgQualityValid(q), false, String(q));
  assert.ok(L.JPG_QUALITY_PRESETS.every(L.jpgQualityValid));
  assert.ok(L.JPG_QUALITY_PRESETS.includes(90), "the default is a preset");
  assert.ok(L.SHOT_TIMEOUT_PRESETS.every(L.shotTimeoutValid) && L.SHOT_TIMEOUT_PRESETS.includes(10) && L.SHOT_TIMEOUT_PRESETS.includes(0));
  assert.equal(L.shotTimeoutValid(3601), false);
  assert.equal(L.shotTimeoutValid(-1), false);
});

test("Snipping Tool warning only while a screenshot slot is bare PrtSc", () => {
  assert.equal(L.snippingConflict(true, HK9), true);
  assert.equal(L.snippingConflict(false, HK9), false);
  assert.equal(L.snippingConflict(true, { ...HK9, shotRegion: "Shift + PrtSc" }), false, "Ctrl + PrtSc still reaches Liqlip");
  assert.equal(L.snippingConflict(true, { ...HK9, shotRegion: "", shotText: "PrtSc" }), true);
  assert.equal(L.snippingConflict(true, null), false);
});

test("resetting the editor's tool defaults forgets only their key", () => {
  const store = new Map([[L.EDITOR_TOOLS_KEY, "{}"], ["liqlip.other", "1"]]);
  const storage = { getItem: (k) => store.get(k) ?? null, removeItem: (k) => store.delete(k) };
  assert.equal(L.EDITOR_TOOLS_KEY, "liqlip.editor.tools", "ARCHITECTURE §18.6");
  assert.equal(L.resetEditorTools(storage), true);
  assert.equal(store.has(L.EDITOR_TOOLS_KEY), false);
  assert.equal(store.get("liqlip.other"), "1");
  assert.equal(L.resetEditorTools(storage), false, "nothing stored → nothing to reset");
  assert.equal(L.resetEditorTools(null), false);
  assert.equal(L.resetEditorTools({ getItem: () => { throw new Error("blocked"); }, removeItem: () => {} }), false);
});
