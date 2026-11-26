// QAO controller (SPEC §9.4, ARCHITECTURE §18.8): the stack, its timers, the buttons and the
// drag-out lifecycle — framework-free so node can test it (tests/shotsQao.test.mjs). The window
// root passes a `$state` object (mutations through the proxy re-render) and the real commands;
// tests pass a plain object and fakes. Lifecycle rules carried over from Liquid Tools'
// `hudLifecycle` tests: a drop closes only the dragged card, a cancelled or failed drag resumes
// the timer and clears the hover the native drag loop left behind, `pointercancel` never opens
// the editor, an old drag finishing never touches newer cards.
import type { TranslationKey } from "$lib/i18n/en";
import type { IconName } from "$lib/icons";
import type {
  ClipDetail,
  ClipsChanged,
  CopyRequest,
  DragOutcome,
  PasteOutcome,
  ShotAdded,
  ShotCorner,
  UndoResult,
} from "$lib/ipc/types";
import { addShot, makeShot, removeShot, stackHeight, tempName, tickShots, type Shot, type ShotBusy } from "./stack";

/** What the buttons of a card do (SPEC §9.4) + the save notice's "Change folder". */
export type ShotCmd = ShotBusy | "close" | "folder";

/** Text recognition for the "Text" button: usable, turned off in Settings, or no language pack. */
export type OcrGate = "ready" | "off" | "nolang";

export interface QaoState {
  items: Shot[];
  /** the pointer is over the stack — every timer waits (a card under the cursor never moves) */
  hovered: boolean;
  /** key of the card being dragged out (a native OLE loop is running) */
  dragging: string | null;
  /** a capture session is open (`shot:session` → `shot:end`): Rust hid the window, timers wait */
  capturing: boolean;
  /** the page is hidden (`document.hidden`) — timers wait */
  hidden: boolean;
  /** `settings.screenshot.timeout` (s, 0 = never), `.sound`, `.corner` */
  timeout: number;
  sound: boolean;
  corner: ShotCorner;
  ocr: OcrGate;
}

export function initialState(): QaoState {
  return { items: [], hovered: false, dragging: null, capturing: false, hidden: false, timeout: 10, sound: true, corner: "left", ocr: "ready" };
}

/** The commands the QAO uses (§18.10 + existing ones), a clock and a scheduler. */
export interface QaoApi {
  clipCopy(req: CopyRequest): Promise<PasteOutcome>;
  clipDetail(id: number): Promise<ClipDetail>;
  clipDragOut(ids: number[]): Promise<DragOutcome>;
  clipOcr(id: number): Promise<string>;
  clipsDelete(ids: number[]): Promise<UndoResult>;
  textCopy(text: string): Promise<void>;
  shotEdit(id: number): Promise<void>;
  shotPin(id: number): Promise<void>;
  shotSave(id: number): Promise<string>;
  shotReveal(id: number): Promise<void>;
  /** a card without a record (`clip: null`): its temp file by name (INT-5) */
  shotTempCopy(name: string): Promise<void>;
  shotTempSaveAs(name: string): Promise<string | null>;
  shotsHudLayout(height: number): Promise<void>;
  shotsHudHide(): Promise<void>;
  settingsOpen(section: string | null): Promise<void>;
  /** the shutter click (WebAudio, `shutter.ts`) */
  shutter(): void;
  /** monotonic clock, ms */
  now(): number;
  /** run `fn` after `ms`; returns a cancel */
  later(fn: () => void, ms: number): () => void;
}

export interface PointerPoint {
  pointerId: number;
  x: number;
  y: number;
}

/** A press moves more than this (px) → it is a drag-out, not a click. */
export const DRAG_PX = 6;
/** The window shrinks after the leaving card's exit and its neighbours' slide (160 / 200 ms). */
export const SHRINK_MS = 220;
export const TOAST_MS = 1600;
export const TOAST_ERROR_MS = 2600;
/** "Recognizing…" stays until recognition answers (bounded, in case it never does). */
const TOAST_RUNNING_MS = 15_000;
/** After sleep / a long stall the timers move at most this much in one tick. */
const MAX_TICK_S = 1;

type Code = "not_found" | "busy" | "invalid" | "failed" | "unimplemented" | "unknown";
const CODES: readonly Code[] = ["not_found", "busy", "invalid", "failed", "unimplemented"];

/** Machine code of a rejected command ("<code>: <detail>", ARCHITECTURE §9.6) — `errors.ts`
 *  without its i18n import, so this module stays runnable in node. */
export function errorCode(e: unknown): Code {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : String(e ?? "");
  const code = raw.split(":", 1)[0]?.trim() as Code;
  return CODES.includes(code) ? code : "unknown";
}

export type Qao = ReturnType<typeof createQao>;

export function createQao(state: QaoState, api: QaoApi) {
  let down: (PointerPoint & { key: string }) | null = null;
  /** last height sent to Rust (0 — hidden) */
  let height = 0;
  let cancelShrink: (() => void) | null = null;
  let last: number | null = null;
  const pending = new Set<Promise<unknown>>();

  const find = (key: string): Shot | undefined => state.items.find((i) => i.key === key);

  function patch(key: string, change: Partial<Shot>): void {
    if (!find(key)) return;
    state.items = state.items.map((i) => (i.key === key ? { ...i, ...change } : i));
  }

  function track<T>(p: Promise<T>): Promise<T> {
    pending.add(p);
    void p.finally(() => pending.delete(p)).catch(() => {});
    return p;
  }

  function send(h: number): void {
    height = h;
    const call = h === 0 ? api.shotsHudHide() : api.shotsHudLayout(h);
    void call.catch((e) => console.error("[shots] layout", e));
  }

  /** Grow at once (the stack is anchored at the bottom, the window grows upward); shrink after
   *  the leaving card's animation so nothing is cut by the window's top edge. */
  function relayout(grow: boolean): void {
    cancelShrink?.();
    cancelShrink = null;
    const h = stackHeight(state.items.length);
    if (grow) {
      send(h);
    } else if (h !== height) {
      cancelShrink = api.later(() => {
        cancelShrink = null;
        send(stackHeight(state.items.length));
      }, SHRINK_MS);
    }
  }

  function toast(key: string, k: TranslationKey, icon: IconName, error = false, ms = error ? TOAST_ERROR_MS : TOAST_MS): void {
    patch(key, { toast: { key: k, icon, error, until: api.now() + ms } });
  }

  function failed(key: string, e: unknown, fallback: TranslationKey): void {
    const code = errorCode(e);
    console.error("[shots]", fallback, e);
    toast(key, code === "not_found" ? "shots.toast.gone" : code === "busy" ? "shots.toast.busy" : fallback, "warning", true);
  }

  function dismiss(key: string): void {
    if (!find(key)) return;
    state.items = removeShot(state.items, key);
    if (down?.key === key) down = null;
    relayout(false);
  }

  /** A shot was taken: a card at the bottom, the window grows, the shutter clicks. */
  function add(p: ShotAdded): void {
    state.capturing = false; // a finished shot ends any capture pause, even without `shot:end`
    const shot = makeShot(p, state.timeout);
    if (!shot) return;
    state.items = addShot(state.items, shot);
    relayout(true);
    if (state.sound) api.shutter();
    // No thumbnail in the payload (e.g. a store without `thumbUrl` yet): the full picture.
    if (shot.clip && !shot.preview) {
      const id = shot.clip.id;
      void track(
        api.clipDetail(id).then(
          (d) => patch(shot.key, { preview: d.thumbUrl ?? d.imageUrl }),
          (e) => console.error("[shots] clip_detail", e),
        ),
      );
    }
  }

  async function run(key: string, cmd: Exclude<ShotBusy, "saveAs">, id: number): Promise<void> {
    switch (cmd) {
      case "copy": {
        let out: PasteOutcome;
        try {
          out = await api.clipCopy({ ids: [id], mode: "auto" });
        } catch (e) {
          return failed(key, e, "shots.toast.copyFailed");
        }
        if (out.result === "failed") {
          const k: TranslationKey =
            out.reason === "not_found" ? "shots.toast.gone" : out.reason === "clipboard_busy" ? "shots.toast.busy" : "shots.toast.copyFailed";
          return toast(key, k, "warning", true);
        }
        return toast(key, "shots.toast.copied", "check");
      }
      case "edit":
        try {
          await api.shotEdit(id);
        } catch (e) {
          failed(key, e, "shots.toast.editFailed");
        }
        return;
      case "pin":
        try {
          await api.shotPin(id);
        } catch (e) {
          failed(key, e, "shots.toast.pinFailed");
        }
        return;
      case "save":
        try {
          const path = await api.shotSave(id);
          patch(key, { savedPath: path, error: null });
          toast(key, "shots.toast.saved", "check");
        } catch (e) {
          console.error("[shots] shot_save", e);
          if (errorCode(e) === "not_found") toast(key, "shots.toast.gone", "warning", true);
          else patch(key, { error: "save_failed", toast: null });
        }
        return;
      case "reveal":
        try {
          await api.shotReveal(id);
        } catch (e) {
          console.error("[shots] shot_reveal", e);
          if (errorCode(e) === "not_found") {
            // The saved file was moved or deleted: offer "Save" again.
            patch(key, { savedPath: null });
            toast(key, "shots.toast.fileMissing", "warning", true);
          } else {
            toast(key, "shots.toast.revealFailed", "warning", true);
          }
        }
        return;
      case "text": {
        toast(key, "shots.toast.textRunning", "ocr", false, TOAST_RUNNING_MS);
        let text: string;
        try {
          text = await api.clipOcr(id);
        } catch (e) {
          const code = errorCode(e);
          console.error("[shots] clip_ocr", e);
          const k: TranslationKey =
            code === "busy" ? "shots.toast.textBusy" : code === "invalid" ? "shots.textOff" : code === "not_found" ? "shots.toast.gone" : "shots.toast.textFailed";
          return toast(key, k, "warning", true);
        }
        if (!text.trim()) return toast(key, "shots.toast.textEmpty", "info");
        try {
          await api.textCopy(text);
        } catch (e) {
          return failed(key, e, "shots.toast.copyFailed");
        }
        return toast(key, "shots.toast.textCopied", "check");
      }
      case "delete":
        try {
          await api.clipsDelete([id]);
          dismiss(key); // from history only — the saved file stays (SPEC §9.4)
        } catch (e) {
          if (errorCode(e) === "not_found") dismiss(key);
          else failed(key, e, "shots.toast.deleteFailed");
        }
        return;
    }
  }

  /** A card without a record (screenshots not kept, or > 64 MP): Copy and Save as… work on its
   *  temp file by name; a missing file (the stack let it go, or a restart) says so. */
  async function runTemp(key: string, cmd: "copy" | "saveAs", name: string): Promise<void> {
    const gone = (e: unknown) => errorCode(e) === "not_found";
    if (cmd === "copy") {
      try {
        await api.shotTempCopy(name);
        toast(key, "shots.toast.copied", "check");
      } catch (e) {
        console.error("[shots] shot_temp_copy", e);
        toast(key, gone(e) ? "shots.toast.tempGone" : errorCode(e) === "busy" ? "shots.toast.busy" : "shots.toast.copyFailed", "warning", true);
      }
      return;
    }
    try {
      const path = await api.shotTempSaveAs(name);
      if (path) toast(key, "shots.toast.saved", "check"); // null — the dialog was cancelled
    } catch (e) {
      console.error("[shots] shot_temp_save_as", e);
      toast(key, gone(e) ? "shots.toast.tempGone" : "shots.toast.saveFailed", "warning", true);
    }
  }

  /** A button of a card. One command at a time per card; a card without a record (`clip: null`)
   *  copies or saves its temp file, or closes. */
  function act(key: string, cmd: ShotCmd): Promise<void> {
    const s = find(key);
    if (!s) return Promise.resolve();
    if (cmd === "close") {
      dismiss(key);
      return Promise.resolve();
    }
    if (cmd === "folder") {
      return track(api.settingsOpen("screenshot").catch((e) => console.error("[shots] settings_open", e)));
    }
    if (s.busy) return Promise.resolve();
    if (!s.clip) {
      if ((cmd !== "copy" && cmd !== "saveAs") || !s.tempPath) return Promise.resolve();
      patch(key, { busy: cmd });
      return track(runTemp(key, cmd, tempName(s.tempPath)).finally(() => patch(key, { busy: null })));
    }
    const id = s.clip.id;
    if (cmd === "saveAs") return Promise.resolve(); // a record saves with Save / Show in folder
    if (cmd === "text" && state.ocr !== "ready") return Promise.resolve();
    patch(key, { busy: cmd });
    return track(run(key, cmd, id).finally(() => patch(key, { busy: null })));
  }

  /** The full time again (a cancelled drag gives the card back its whole countdown). */
  function restart(key: string): void {
    const s = find(key);
    if (s) patch(key, { remaining: s.timeout });
  }

  async function drag(key: string, id: number): Promise<void> {
    try {
      const out = await api.clipDragOut([id]);
      if (out === "dropped") dismiss(key);
      else restart(key);
    } catch (e) {
      restart(key);
      failed(key, e, "shots.toast.dragFailed");
    } finally {
      state.dragging = null;
      // The native drag loop swallowed `pointerleave`: the next pointer move re-arms the hover.
      state.hovered = false;
    }
  }

  return {
    add,
    dismiss,
    act,

    /** Left press on a thumbnail. `true` → the caller captures the pointer. */
    press(key: string, p: PointerPoint & { button: number }): boolean {
      if (p.button !== 0 || state.dragging !== null || !find(key)?.clip) return false;
      down = { key, pointerId: p.pointerId, x: p.x, y: p.y };
      return true;
    },

    /** Pointer move while pressed. `true` → a drag-out started: the caller releases the capture
     *  (the OLE loop owns the mouse until the drop). */
    move(key: string, p: PointerPoint & { buttons: number }): boolean {
      if (!down || down.key !== key || down.pointerId !== p.pointerId || state.dragging !== null) return false;
      if (!(p.buttons & 1)) {
        down = null;
        return false;
      }
      if (Math.hypot(p.x - down.x, p.y - down.y) <= DRAG_PX) return false;
      down = null;
      const id = find(key)?.clip?.id;
      if (id === undefined) return false;
      state.dragging = key;
      void track(drag(key, id));
      return true;
    },

    /** Release of the press that started on this card without a drag = a click → the editor. */
    release(key: string, p: PointerPoint): void {
      const click = down !== null && down.key === key && down.pointerId === p.pointerId;
      down = null;
      if (click) void act(key, "edit");
    },

    /** `pointercancel` / lost capture: forget the press — never a click. */
    cancel(): void {
      down = null;
    },

    enter(): void {
      if (!state.hovered) state.hovered = true;
    },
    leave(): void {
      if (state.hovered) state.hovered = false;
    },

    /** `shot:session` (the selection stage): Rust hides the window; timers wait until `shot:end`.
     *  The hover is forgotten (W7-R4): a window hidden under the pointer never gets
     *  `pointerleave`, and a stale hover would keep the stack forever after the shot; if the
     *  pointer is still there when the stack comes back, its next move re-arms the hover. */
    sessionStarted(): void {
      state.capturing = true;
      state.hovered = false;
    },
    sessionEnded(): void {
      state.capturing = false;
    },

    setHidden(hidden: boolean): void {
      state.hidden = hidden;
    },

    /** The root stopped ticking (empty stack): the next tick starts from zero elapsed. */
    stopClock(): void {
      last = null;
    },

    /** One timer step (the root calls it every 100 ms). */
    tick(): void {
      const now = api.now();
      const elapsed = last === null ? 0 : Math.min(MAX_TICK_S, Math.max(0, (now - last) / 1000));
      last = now;
      if (state.items.some((i) => i.toast && i.toast.until <= now)) {
        state.items = state.items.map((i) => (i.toast && i.toast.until <= now ? { ...i, toast: null } : i));
      }
      const paused = state.hovered || state.dragging !== null || state.capturing || state.hidden;
      const before = state.items.length;
      const next = tickShots(state.items, elapsed, paused);
      if (next === state.items) return;
      state.items = next;
      if (next.length !== before) {
        if (down && !find(down.key)) down = null;
        relayout(false);
      }
    },

    /** `clips:changed`: a deleted record's card leaves; an updated one (edited, saved) refreshes
     *  its picture and saved path; `reset` re-checks every card. */
    clipsChanged(p: ClipsChanged): Promise<void> {
      if (p.change === "added") return Promise.resolve();
      const cards = state.items.filter((i) => i.clip !== null);
      const hit = (s: Shot) => !p.ids || p.ids.includes(s.clip!.id);
      if (p.change === "deleted" && p.ids) {
        for (const s of cards.filter(hit)) dismiss(s.key);
        return Promise.resolve();
      }
      const refresh = cards.filter(hit).map((s) =>
        api.clipDetail(s.clip!.id).then(
          (d) => {
            const cur = find(s.key);
            if (!cur) return;
            patch(s.key, {
              clip: d,
              preview: d.thumbUrl ?? d.imageUrl ?? cur.preview,
              savedPath: d.meta.shot ? d.meta.shot.savedPath : cur.savedPath,
            });
          },
          (e) => {
            if (errorCode(e) === "not_found") dismiss(s.key);
            else console.error("[shots] clip_detail", e);
          },
        ),
      );
      return track(Promise.all(refresh).then(() => {}));
    },

    /** Tests: wait for every command in flight. */
    async settled(): Promise<void> {
      while (pending.size) await Promise.allSettled([...pending]);
    },
  };
}
