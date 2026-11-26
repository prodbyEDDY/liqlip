<script lang="ts">
  // The editor window (SPEC §9.5, ARCHITECTURE §18.9). An ordinary window (system title bar, set
  // by Rust: "Name — Liqlip"): tools and properties on top, the canvas with zoom and pan (fit by
  // default), Copy · Pin · Save as… · Done top right. On load `shot_editor_get()`; the source is
  // `baseUrl` (`<img crossorigin="anonymous">` → `createImageBitmap`, the canvas stays readable).
  // Done (`Enter` at rest, `Ctrl+S`) saves into the same record through `shot_editor_commit` (raw
  // body §18.4: the document + the flattened RGBA) and Rust closes the window (`editor::commit`;
  // not while a switch to another record waits — then the next record loads here). Done without
  // changes sends no pixels: the record is not rewritten, Rust copies it (if "Copy" is on in
  // Settings) and closes. Pin with unsaved changes sends the pixels: Rust applies the edit first
  // (as Done, the window stays), then pins. Closing with unsaved changes (`onCloseRequested`)
  // and `shot:editor` (another picture, `dirtyGuard: true`) ask "Save / Don't save / Cancel".
  // Browser preview on the mock: /editor.html?clip=<id>.
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import Toast from "$lib/components/Toast.svelte";
  import type { IconName } from "$lib/icons";
  import { isBrowserShortcut } from "$lib/browserKeys";
  import { t } from "$lib/i18n/index.svelte";
  import { shotEditorCommit, shotEditorGet, shotEditorSwitch } from "$lib/ipc/commands";
  import { onShotEditor } from "$lib/ipc/events";
  import { errorMessage } from "$lib/ipc/errors";
  import { isTauri } from "$lib/ipc/runtime";
  import type { AnnotDoc, Clip, ShotEditorAction, ShotEditorPayload } from "$lib/ipc/types";
  import { Editor, PropsBar, Toolbar, createEditorState, loadDoc, type EditorIntent } from "$lib/editor";
  import Glyph from "$lib/editor/glyphs/Glyph.svelte";
  import Tip from "$lib/editor/Tip.svelte";
  import UnsavedDialog, { type Answer } from "./UnsavedDialog.svelte";
  import { LoadOrder } from "./loadOrder";

  type Phase = "loading" | "ready" | "empty" | "error";

  const ed = createEditorState();
  const order = new LoadOrder();
  let phase = $state<Phase>("loading");
  let payload = $state<ShotEditorPayload | null>(null);
  let base = $state.raw<ImageBitmap | null>(null);
  let doc = $state.raw<AnnotDoc | null>(null);
  let api = $state<ReturnType<typeof Editor> | null>(null);
  let busy = $state<ShotEditorAction | null>(null);
  let dialog = $state<{ switching: ShotEditorPayload | null } | null>(null);
  let toast = $state<{ message: string; icon?: IconName; error?: boolean } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  const name = $derived(payload?.name ?? "");
  const ready = $derived(phase === "ready" && !!base);
  const zoomPct = $derived(`${Math.round(ed.zoom * 100)}%`);

  function say(message: string, icon?: IconName, error = false): void {
    clearTimeout(toastTimer);
    toast = { message, icon, error };
    toastTimer = setTimeout(() => (toast = null), error ? 6000 : 2400);
  }

  /** `ticket` — taken when the load was decided (a switch takes it before awaiting Rust). */
  async function load(p: ShotEditorPayload | null, ticket = order.begin()): Promise<void> {
    if (!order.current(ticket)) return;
    dialog = null;
    payload = p;
    if (!p) {
      phase = "empty";
      base?.close();
      base = null;
      return;
    }
    phase = "loading";
    if (typeof document !== "undefined") document.title = `${p.name} — Liqlip`;
    try {
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.src = p.baseUrl;
      await img.decode();
      const bmp = await createImageBitmap(img);
      if (!order.current(ticket)) {
        bmp.close(); // a newer record is loading: this picture must not land under its id
        return;
      }
      const res = loadDoc(p.doc, bmp.width, bmp.height, 1);
      if (res.unknownVersion) say(t("editor.toast.newerDoc"), "warning", true);
      base?.close();
      base = bmp;
      doc = res.doc;
      phase = "ready";
    } catch (e) {
      if (!order.current(ticket)) return;
      console.error("[editor] cannot open the picture", e);
      phase = "error";
    }
  }

  async function reload(): Promise<void> {
    phase = "loading";
    const mark = order.mark();
    try {
      const p = await shotEditorGet();
      if (order.superseded(mark)) return; // a `shot:editor` came meanwhile — it is newer
      await load(p);
    } catch (e) {
      if (order.superseded(mark)) return;
      console.error("[editor] shot_editor_get failed", e);
      phase = "error";
    }
  }

  /** Close the window for good (Tauri); the browser preview has nothing to close. */
  async function destroyWindow(): Promise<void> {
    if (isTauri()) await getCurrentWindow().destroy();
  }

  async function commit(action: ShotEditorAction): Promise<boolean> {
    if (!api || !payload || busy) return false;
    const dirty = api.dirty();
    busy = action;
    try {
      // Done / Pin without changes send no pixels: the record stays as it is (no rewrite, no
      // duplicate — LT П-23); Copy / Save as always send what the editor shows.
      const unchanged = !dirty && (action === "done" || action === "pin");
      let px: { width: number; height: number; data: Uint8Array } | null = null;
      if (!unchanged) {
        const d = api.doc();
        px = await api.exportRGBA(d.crop ?? { x: 0, y: 0, w: d.w, h: d.h });
      }
      const head = { clipId: payload.clipId, action, doc: api.doc(), width: px?.width ?? 0, height: px?.height ?? 0 };
      // `null` — "Save as…" was cancelled (W7-R2; the contract answers it since W8-A): no toast
      // (the record may have had a saved path before, so the path alone doesn't tell a cancel)
      const clip: Clip | null = await shotEditorCommit(head, px?.data ?? null);
      if (action === "done") {
        api.markSaved();
        // Rust closes the window; the browser preview stays and says what happened.
        say(t(unchanged ? "editor.toast.nothing" : "editor.toast.saved"), "check");
      } else if (action === "copy") say(t("editor.toast.copied"), "copy");
      else if (action === "pin") {
        if (!unchanged) api.markSaved(); // the edit went into the record before the pin
        say(t("editor.toast.pinned"), "check");
      }
      else if (action === "save_as" && clip) say(t("editor.toast.savedAs"), "download");
      return true;
    } catch (e) {
      console.error(`[editor] shot_editor_commit ${action} failed`, e);
      say(errorMessage(e), "warning", true);
      return false;
    } finally {
      busy = null;
    }
  }

  function onIntent(i: EditorIntent): void {
    if (dialog) return;
    if (i === "done" || i === "save") void commit("done");
    else if (i === "copy") void commit("copy");
    else if (i === "save_as") void commit("save_as");
    else if (i === "pin") void commit("pin");
    else if (i === "close") void requestClose();
  }

  async function requestClose(): Promise<void> {
    if (api?.dirty()) dialog = { switching: null };
    else await destroyWindow();
  }

  async function answer(a: Answer): Promise<void> {
    const d = dialog;
    if (!d) return;
    if (a === "cancel") {
      dialog = null;
      if (d.switching) await shotEditorSwitch(false).catch((e) => console.error("[editor] switch", e));
      return;
    }
    if (a === "save") {
      const ok = await commit("done");
      if (!ok) return; // the error toast is up; the dialog stays
    }
    dialog = null;
    if (d.switching) {
      await shotEditorSwitch(true).catch((e) => console.error("[editor] switch", e));
      await load(d.switching);
    } else if (a === "discard") {
      if (isTauri()) await destroyWindow();
      else await load(payload); // the preview "reopens" the record without the changes
    }
  }

  /** `shot:editor` — another record in this window (the window asks first if needed). */
  async function onSwitch(p: ShotEditorPayload): Promise<void> {
    order.switched();
    if (api?.dirty()) {
      dialog = { switching: p };
      return;
    }
    const ticket = order.begin(); // in event order — not in the order Rust acknowledges the switches
    if (p.dirtyGuard) await shotEditorSwitch(true).catch((e) => console.error("[editor] switch", e));
    await load(p, ticket);
  }

  function onKeydown(e: KeyboardEvent): void {
    if (dialog) return; // the dialog owns the keyboard (Esc = its `cancel`)
    const target = e.target as HTMLElement | null;
    const pressable = target?.tagName === "BUTTON" || target?.tagName === "A";
    if (ready && api && !(pressable && ["Enter", "NumpadEnter", "Space"].includes(e.code))) {
      if (api.handleKey(e)) return;
      if (e.code === "Escape") {
        e.preventDefault();
        api.escape(); // the window never closes on Esc
        return;
      }
    }
    if (isBrowserShortcut(e, { find: true })) e.preventDefault();
  }

  /** The page takes no drops (a dropped file would navigate the webview away from the work). */
  function refuseDrop(e: DragEvent): void {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "none";
  }

  onMount(() => {
    const offs: (UnlistenFn | Promise<UnlistenFn>)[] = [];
    // listen first, then ask (W7-R2): a switch emitted while `shot_editor_get` is in flight is
    // not lost, and its payload wins over the (older) answer — `order`
    const sub = onShotEditor((p) => void onSwitch(p));
    offs.push(sub);
    void Promise.resolve(sub)
      .catch((e) => console.error("[editor] shot:editor subscription failed", e))
      .finally(() => void reload());
    if (isTauri()) {
      offs.push(
        getCurrentWindow().onCloseRequested(async (ev) => {
          if (api?.dirty()) {
            ev.preventDefault();
            dialog = { switching: null };
          }
        }),
      );
    }
    return () => {
      clearTimeout(toastTimer);
      for (const o of offs) void Promise.resolve(o).then((f) => f());
    };
  });

  const keep = (e: MouseEvent) => e.preventDefault();
</script>

<svelte:window onkeydown={onKeydown} ondragover={refuseDrop} ondrop={refuseDrop} />

<div class="win">
  <!-- two rows (tools · actions / properties · zoom); a narrow window (< 820 px, e.g. the 640 px
       minimum for a small picture) gets three: tools / properties / zoom · actions -->
  <header class="bar">
    <!-- tools and properties wait for a picture (inert + dimmed while loading / empty / error) -->
    <div class="bar__tools bar__tools--tools" class:is-idle={!ready} inert={!ready}><Toolbar state={ed} /></div>
    <div class="bar__end" role="group" aria-label={t("editor.actions")}>
      <button type="button" class="act" disabled={!ready || !!busy} aria-label={`${t("editor.action.copy")} (Ctrl+C)`} onmousedown={keep} onclick={() => void commit("copy")}>
        <Ico name={busy === "copy" ? "loaderCircle" : "copy"} size={18} />
        <Tip label={t("editor.action.copy")} keys={["Ctrl", "C"]} />
      </button>
      <button type="button" class="act" disabled={!ready || !!busy} aria-label={`${t("editor.action.pin")} (Ctrl+P)`} onmousedown={keep} onclick={() => void commit("pin")}>
        <Ico name="pin" size={18} />
        <Tip label={t("editor.action.pin")} keys={["Ctrl", "P"]} />
      </button>
      <button type="button" class="act" disabled={!ready || !!busy} aria-label={`${t("editor.action.saveAs")} (Ctrl+Shift+S)`} onmousedown={keep} onclick={() => void commit("save_as")}>
        <Ico name={busy === "save_as" ? "loaderCircle" : "download"} size={18} />
        <Tip label={t("editor.action.saveAs")} keys={["Ctrl", "Shift", "S"]} />
      </button>
      <span class="done">
        <Button icon="check" disabled={!ready || (!!busy && busy !== "done")} loading={busy === "done"} onmousedown={keep} onclick={() => void commit("done")}>
          {t("editor.action.done")}
        </Button>
        <Tip label={t("editor.action.done")} keys={["Enter"]} />
      </span>
    </div>
    <div class="bar__tools bar__tools--props" class:is-idle={!ready} inert={!ready}><PropsBar state={ed} /></div>
    <div class="zoom" role="group" aria-label={t("editor.zoom.label")}>
      <button type="button" class="act act--sm" disabled={!ready} aria-label={t("editor.zoom.out")} onmousedown={keep} onclick={() => ed.viewAction("out")}>
        <Glyph name="zoomOut" size={16} />
        <Tip label={t("editor.zoom.out")} keys={["Ctrl", "−"]} />
      </button>
      <button type="button" class="zoom__pct" disabled={!ready} aria-label={t("editor.zoom.fit")} onmousedown={keep} onclick={() => ed.viewAction(ed.zoom === 1 ? "fit" : "actual")}>
        {zoomPct}
        <Tip label={ed.zoom === 1 ? t("editor.zoom.fit") : t("editor.zoom.actual")} keys={["Ctrl", ed.zoom === 1 ? "0" : "1"]} />
      </button>
      <button type="button" class="act act--sm" disabled={!ready} aria-label={t("editor.zoom.in")} onmousedown={keep} onclick={() => ed.viewAction("in")}>
        <Glyph name="zoomIn" size={16} />
        <Tip label={t("editor.zoom.in")} keys={["Ctrl", "+"]} />
      </button>
      <button type="button" class="act act--sm" aria-label={t("editor.keys.title")} onmousedown={keep} onclick={() => (ed.cheatsheet = !ed.cheatsheet)}>
        <Glyph name="help" size={16} />
        <Tip label={t("editor.keys.title")} keys={["?"]} />
      </button>
    </div>
  </header>

  <main class="stage" aria-busy={phase === "loading"}>
    {#if phase === "loading"}
      <div class="stage__skel" style:aspect-ratio={payload && payload.width && payload.height ? `${payload.width} / ${payload.height}` : "16 / 10"}>
        <Skeleton width="100%" height="100%" radius={12} />
      </div>
    {:else if phase === "empty"}
      <div class="stage__msg">
        <EmptyState icon="image" companions={["edit", "copy"]} title={t("editor.empty.title")} desc={t("editor.empty.desc")} />
      </div>
    {:else if phase === "error"}
      <div class="stage__msg">
        <EmptyState icon="warning" title={t("editor.error.title")} desc={t("editor.error.desc")}>
          <Button variant="secondary" size="sm" icon="refresh" onclick={() => void reload()}>{t("common.retry")}</Button>
        </EmptyState>
      </div>
    {:else if base && doc}
      <Editor bind:this={api} {base} {doc} mode="window" state={ed} onintent={onIntent} />
    {/if}
  </main>

  {#if toast}
    <div class="toasts" aria-live="polite">
      <Toast message={toast.message} icon={toast.icon} error={toast.error} onClose={() => (toast = null)} />
    </div>
  {/if}

  {#if dialog}
    <UnsavedDialog {name} switching={!!dialog.switching} busy={busy === "done"} onanswer={(a) => void answer(a)} />
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: var(--color-ground);
    overflow: hidden;
  }
  .win {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
  }
  .bar {
    position: relative;
    z-index: 10;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas:
      "tools end"
      "props zoom";
    align-items: center;
    gap: 8px;
    padding: 12px 12px 8px;
  }
  /* narrow (the window's 640 px minimum): the tools take a row of their own; actions go right */
  @media (max-width: 819px) {
    .bar {
      grid-template-columns: auto minmax(0, 1fr);
      grid-template-areas:
        "tools tools"
        "props props"
        "zoom end";
    }
  }
  .bar__tools {
    display: inline-flex;
    min-width: 0;
    transition: opacity var(--dur-base) var(--ease-out-soft);
  }
  .bar__tools--tools {
    grid-area: tools;
  }
  .bar__tools--props {
    grid-area: props;
  }
  .bar__tools.is-idle {
    opacity: 0.45;
  }
  .bar__end,
  .zoom {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .bar__end {
    grid-area: end;
    justify-self: end;
  }
  .zoom {
    grid-area: zoom;
    justify-self: end;
  }
  @media (max-width: 819px) {
    .zoom {
      justify-self: start;
    }
  }
  .done {
    position: relative;
    display: inline-flex;
  }
  .zoom {
    height: 40px;
    padding: 4px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
  }
  .act {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--h-lg);
    height: var(--h-lg);
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
    color: var(--color-ink);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  .act:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-ink) 6%, var(--color-raised));
  }
  .act:active:not(:disabled) {
    transform: scale(0.95);
  }
  .act:disabled {
    opacity: 0.45;
  }
  .act--sm {
    width: var(--h-md);
    height: var(--h-md);
    background: transparent;
    box-shadow: none;
    color: var(--color-ink-2);
  }
  .act--sm:hover:not(:disabled) {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .zoom__pct {
    position: relative;
    min-width: 56px;
    height: var(--h-md);
    padding: 0 8px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .zoom__pct:hover:not(:disabled) {
    background: var(--color-track);
  }
  .zoom__pct:disabled {
    opacity: 0.45;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .stage__skel {
    position: absolute;
    top: 50%;
    left: 50%;
    width: min(70%, 960px);
    max-height: 80%;
    transform: translate(-50%, -50%);
  }
  .stage__msg {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .toasts {
    position: fixed;
    bottom: 16px;
    left: 50%;
    z-index: 60;
    max-width: calc(100vw - 32px);
    transform: translateX(-50%);
  }
</style>
