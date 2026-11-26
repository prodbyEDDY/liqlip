<script lang="ts">
  // The capture overlay (SPEC §9.2, ARCHITECTURE §18.7; W6-D). The window is pre-created in
  // `setup()` and loaded once: it listens to `shot:session` and asks `shotSession()` on load
  // (the overlay may open before the subscription lands). The frozen frame is an
  // `<img crossorigin="anonymous">` (never tainted) → `ImageBitmap` for the magnifier and the
  // editor. Pointer positions go to frame px through `frame.width / innerWidth`.
  //
  //   select (stage 1) — dim ~20 %, the window under the cursor (Alt — the outer one); click — that
  //     window (the desktop — the whole monitor); drag — an area (Shift square, Alt from the
  //     centre, Space moves it); magnifier (M, wheel); arrows move the cursor, Enter starts and
  //     ends an area; F — the monitor; R — the last area; Esc — cancel.
  //   frame (stage 2) — the frame with handles OUTSIDE it and the editor (`mode:'overlay'`,
  //     `clip` = the frame) inside; the docked panel; Enter / a double click on empty space with
  //     Select (the editor's `done` intent) — Done; Ctrl+C/S/Shift+S/P; Ctrl+arrows move the
  //     frame, Alt+arrows resize it (Shift — 10 px; INT-5: Ctrl+Alt+arrows rotate the screen on
  //     some Intel drivers).
  //   committing — `shot_commit` with the framed body (§18.4); an error keeps the overlay.
  //
  // Text sessions (OCR) stop after stage 1; "capture on release" (`session.quick`) skips stage 2.
  // Browser preview on the mock: /capture.html (?mock-shot=text, ?mock-shot-quick=1, ?mock-scale=1.5).
  import { onMount } from "svelte";
  import { isBrowserShortcut } from "$lib/browserKeys";
  import { refuseDrop } from "$lib/dropGuard";
  import { t } from "$lib/i18n/index.svelte";
  import { ocrStatus, shotCancel, shotCommit, shotSession, shotStage } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { onShotEnd, onShotSession } from "$lib/ipc/events";
  import type { AnnotDoc, Rect, ShotAction, ShotSession, ShotStage } from "$lib/ipc/types";
  // The ONE place the overlay imports the editor (W6-C, ARCHITECTURE §18.6).
  import {
    createEditorState,
    Editor,
    emptyDoc,
    frameDoc,
    PropsBar,
    Toolbar,
    type EditorApi,
    type EditorIntent,
    type EditorState,
  } from "$lib/editor";
  import { targetAt } from "./captureTargets";
  import { dockPanel } from "./dock";
  import {
    arrowDelta,
    clampPoint,
    contains,
    framePoint,
    fullRect,
    HANDLE_CURSOR,
    hitFrame,
    isDrag,
    moveRect,
    nudgeRect,
    rectToView,
    regionFrom,
    resizeRect,
    snapRect,
    toView,
    viewScale,
    type Handle,
    type Point,
  } from "./geometry";
  import { LOUPE_DEFAULT_ZOOM, zoomBy } from "./loupe";
  import { begin, choose, CONFIRM_MS, commitFailed, commitHeader, escapeLadder, IDLE, reportedStage, sessionChange, startCommit, type HostState } from "./machine";
  import FrameChrome from "./FrameChrome.svelte";
  import HintBar from "./HintBar.svelte";
  import Loupe from "./Loupe.svelte";
  import Notice, { type NoticeData } from "./Notice.svelte";
  import Panel, { type PanelAction } from "./Panel.svelte";
  import Shade from "./Shade.svelte";

  /** Dim around the target: stage 1 (SPEC §9.2 "~20 %") and around the stage-2 frame. */
  const DIM_SELECT = 0.2;
  const DIM_FRAME = 0.3;
  /** Error notices stay this long. */
  const ERROR_MS = 6000;
  /** A tooltip row needs this much room below / above the panel (view px). */
  const TIP_ROOM = 44;

  // ── session ──
  let session = $state.raw<ShotSession | null>(null);
  let host = $state.raw<HostState>(IDLE);
  let bitmap = $state.raw<ImageBitmap | null>(null);
  let editorDoc = $state.raw<AnnotDoc | null>(null);
  let editorState = $state.raw<EditorState | null>(null);
  let editor = $state<EditorApi | undefined>();
  let ocrOff = $state(false);
  let magnifier = $state(true);
  let zoom = $state(LOUPE_DEFAULT_ZOOM);
  let root = $state<HTMLDivElement | null>(null);

  // ── viewport ──
  let vw = $state(0);
  let vh = $state(0);
  const view = $derived({ w: vw, h: vh });
  const frameSize = $derived(session ? { w: session.monitor.width, h: session.monitor.height } : { w: 0, h: 0 });
  const k = $derived(viewScale(frameSize, view));

  // ── pointer and keys ──
  /** The host's own gestures (the editor tracks its own). `press` — down, not yet a drag. */
  type Gesture =
    | { kind: "press"; id: number; down: Point; anchor: Point }
    | { kind: "region"; id: number; anchor: Point; last: Point }
    | { kind: "move"; id: number; down: Point; start: Rect }
    | { kind: "resize"; id: number; down: Point; start: Rect; handle: Handle };
  let gesture = $state.raw<Gesture | null>(null);
  /** Enter … Enter: an area from the keyboard (its anchor). */
  let kbAnchor = $state.raw<Point | null>(null);
  /** The cursor: view px and the frame grid position (arrows move it, the mouse resets it). */
  let cursorView = $state.raw<Point | null>(null);
  let cursor = $state.raw<Point | null>(null);
  let shiftHeld = $state(false);
  let altHeld = $state(false);
  let spaceHeld = false;
  /** A pointer gesture inside the editor is running (the panel turns see-through under it). */
  let editorGesture = $state(false);
  let outsideCursor = $state("crosshair");

  // ── notices ──
  let notice = $state.raw<NoticeData | null>(null);
  let armedAt: number | null = null;
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;

  // ── panel ──
  let panelW = $state(0);
  let panelH = $state(0);

  const busy = $derived(host.stage === "committing");
  const selecting = $derived(host.stage === "select" || (busy && host.resume === "select"));
  const inFrame = $derived(host.stage === "frame" || (busy && host.resume === "frame"));
  const center = $derived({ x: Math.floor(frameSize.w / 2), y: Math.floor(frameSize.h / 2) });

  /** The area being drawn — a mouse drag (stage 1, or a new area outside the stage-2 frame) or the keyboard. */
  const drawing = $derived.by((): Rect | null => {
    if (gesture?.kind === "region" && cursor) return regionFrom(gesture.anchor, cursor, { square: shiftHeld, fromCenter: altHeld }, frameSize);
    if (kbAnchor && cursor) return regionFrom(kbAnchor, cursor, { square: false, fromCenter: false }, frameSize);
    return null;
  });

  /** The window under the cursor (Alt — the outer window); the desktop — the whole monitor. */
  function targetRect(p: Point): Rect {
    const w = session ? targetAt(session.windows, p.x, p.y, altHeld) : null;
    return w ? { x: w.x, y: w.y, w: w.w, h: w.h } : fullRect(frameSize);
  }
  const hover = $derived(selecting && cursor && !drawing ? targetRect(cursor) : null);

  /** The clear hole: the area being drawn, the chosen target while it commits, the hovered target, the frame. */
  const holeFrame = $derived(drawing ?? (busy && host.frame ? host.frame : selecting ? hover : inFrame ? host.frame : null));
  const hole = $derived(holeFrame ? rectToView(holeFrame, k) : null);
  const frameView = $derived(host.frame ? rectToView(host.frame, k) : null);
  const drawingView = $derived(drawing ? rectToView(drawing, k) : null);

  const docked = $derived(frameView && panelW ? dockPanel(frameView, { w: panelW, h: panelH }, view) : { x: 0, y: 0, side: "below" as const });
  const panelRect = $derived(panelW ? { x: docked.x, y: docked.y, w: panelW, h: panelH } : null);
  const anyGesture = $derived(gesture !== null || editorGesture);
  const panelGhost = $derived(anyGesture && !!cursorView && !!panelRect && contains(panelRect, cursorView));
  /** Tooltips of the editor's bars open away from the frame (below a panel under it, above
   *  otherwise) and flip when the monitor edge leaves no room. */
  const tips = $derived.by((): "below" | "above" => {
    if (!panelRect) return "below";
    const want = docked.side === "below" ? "below" : "above";
    if (want === "below" && panelRect.y + panelRect.h + TIP_ROOM > vh) return "above";
    if (want === "above" && panelRect.y < TIP_ROOM) return "below";
    return want;
  });

  const sizeText = (r: Rect) => `${r.w} × ${r.h}`;
  const loupeOn = $derived(selecting && magnifier && !!bitmap && !!cursor && !!cursorView && !busy);
  const loupeCaption = $derived(drawing && session?.dimensions ? sizeText(drawing) : cursor ? `${Math.min(cursor.x, frameSize.w - 1)}, ${Math.min(cursor.y, frameSize.h - 1)}` : "");

  const rootCursor = $derived(
    busy
      ? "wait"
      : gesture?.kind === "resize"
        ? HANDLE_CURSOR[gesture.handle]
        : gesture?.kind === "move"
          ? "move"
          : gesture
            ? "crosshair"
            : "default",
  );

  // ── session lifecycle ──

  function reportStage(id: number, stage: ShotStage): void {
    void shotStage(id, stage).catch((e) => console.warn("[capture] shot_stage", stage, e));
  }

  // `shot_stage` (W7-R4): Rust follows the cursor to another monitor only on `select`, so the
  // stage goes out as soon as a selection starts in stage 1 (a press, an area from the keyboard)
  // and comes back to `select` when it is abandoned — `reportedStage`. One report per change.
  let reported: { id: number; stage: ShotStage } | null = null;
  $effect(() => {
    const id = host.sessionId;
    const want = reportedStage(host, gesture !== null || kbAnchor !== null);
    if (id === null || want === null) {
      reported = null;
      return;
    }
    if (reported?.id === id && reported.stage === want) return;
    reported = { id, stage: want };
    reportStage(id, want);
  });

  function dropBitmap(): void {
    bitmap?.close();
    bitmap = null;
  }

  function clearNotice(): void {
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = null;
    notice = null;
    armedAt = null;
  }

  function showNotice(n: NoticeData, ms: number): void {
    if (noticeTimer) clearTimeout(noticeTimer);
    notice = n;
    noticeTimer = setTimeout(clearNotice, ms);
  }

  function showError(message: string): void {
    armedAt = null;
    showNotice({ kind: "error", message }, ERROR_MS);
  }

  /** Forget the pointer, the keys and the notices (a new session, a monitor change, the end). */
  function resetInput(): void {
    gesture = null;
    kbAnchor = null;
    editorGesture = false;
    clearNotice();
  }

  /** The session is over (committed, cancelled, `shot:end`): nothing stays in memory. */
  function toIdle(): void {
    resetInput();
    host = IDLE;
    session = null;
    dropBitmap();
    editorDoc = null;
    editorState = null;
    editor = undefined;
    cursor = null;
    cursorView = null;
  }

  function applySession(s: ShotSession): void {
    const change = sessionChange(session, host.stage, s);
    if (change === "ignore") return;
    const frameChanged = !session || session.monitor.frameUrl !== s.monitor.frameUrl;
    resetInput();
    if (change === "new") {
      host = begin(s);
      magnifier = s.magnifier;
      zoom = LOUPE_DEFAULT_ZOOM;
      editorState = createEditorState();
      editor = undefined;
      ocrOff = false;
      void ocrStatus()
        .then((st) => (ocrOff = !st.available))
        .catch(() => (ocrOff = false));
    }
    if (frameChanged) dropBitmap();
    session = s;
    editorDoc = emptyDoc(s.monitor.width, s.monitor.height, s.monitor.scale);
    cursor = cursorView ? framePoint(cursorView, viewScale({ w: s.monitor.width, h: s.monitor.height }, view), { w: s.monitor.width, h: s.monitor.height }) : null;
  }

  async function onFrameLoad(e: Event): Promise<void> {
    const img = e.currentTarget as HTMLImageElement;
    const want = session?.monitor.frameUrl;
    try {
      const bmp = await createImageBitmap(img);
      if (!session || session.monitor.frameUrl !== want) return bmp.close();
      dropBitmap();
      bitmap = bmp;
    } catch (err) {
      console.error("[capture] frame decode failed", err);
      showError(t("capture.error.frame"));
    }
  }

  function onFrameError(): void {
    console.error("[capture] frame failed to load", session?.monitor.frameUrl);
    showError(t("capture.error.frame"));
  }

  onMount(() => {
    let alive = true;
    const offs: (() => void)[] = [];
    const keep = (p: Promise<() => void>) => void p.then((off) => (alive ? offs.push(off) : off()));
    keep(onShotSession((s) => applySession(s)));
    keep(
      onShotEnd((p) => {
        if (host.sessionId === null || p.sessionId === host.sessionId) toIdle();
      }),
    );
    void shotSession()
      .then((s) => alive && s && applySession(s))
      .catch((e) => console.error("[capture] shot_session failed", e));
    return () => {
      alive = false;
      offs.forEach((off) => off());
      if (noticeTimer) clearTimeout(noticeTimer);
      bitmap?.close();
    };
  });

  // ── choosing, committing, closing ──

  /** A target was chosen (window, area, monitor, last area; a new area in stage 2). */
  function pick(rect: Rect | null): void {
    const r = rect && snapRect(rect, frameSize);
    gesture = null;
    kbAnchor = null;
    if (!r || host.sessionId === null) return;
    const { state, commit: action } = choose(host, r);
    host = state;
    if (action) void commit(action);
  }

  async function commit(action: ShotAction): Promise<void> {
    const next = startCommit(host, action);
    const rect = next?.frame ? snapRect(next.frame, frameSize) : null;
    if (!next || !rect) return;
    const id = next.sessionId!;
    host = next;
    gesture = null;
    clearNotice();
    try {
      let header = commitHeader(id, rect, action, null, null);
      let rgba: Uint8Array | null = null;
      // the drawing goes as the flattened crop + the frame-relative document (§18.4); text reads the screen
      if (action !== "text" && editor?.hasObjects()) {
        const doc = frameDoc(editor.doc(), rect);
        if (doc.objects.length) {
          const px = await editor.exportRGBA(rect);
          header = commitHeader(id, rect, action, doc, { width: px.width, height: px.height });
          rgba = px.data;
        }
      }
      await shotCommit(header, rgba);
      if (host.sessionId === id) toIdle();
    } catch (e) {
      console.error("[capture] shot_commit failed", e);
      if (host.sessionId !== id) return;
      host = commitFailed(host);
      showError(`${t("capture.error.commit")} ${errorMessage(e)}`);
    }
  }

  async function cancelSession(): Promise<void> {
    const id = host.sessionId;
    if (id === null || busy) return;
    try {
      await shotCancel(id);
      if (host.sessionId === id) toIdle();
    } catch (e) {
      console.error("[capture] shot_cancel failed", e);
      showError(`${t("capture.error.cancel")} ${errorMessage(e)}`);
    }
  }

  /** Arm "Close without saving?"; a second Esc (or Cancel) within 3 s closes. */
  function armClose(now: number): void {
    showNotice({ kind: "confirm" }, CONFIRM_MS);
    armedAt = now;
  }

  /** Esc: the host's gesture → the editor's steps → close (asking first when something is drawn). */
  function escape(): void {
    const now = Date.now();
    const hostGesture = gesture !== null || kbAnchor !== null;
    const out = escapeLadder({
      hostGesture,
      editorEscaped: !hostGesture && inFrame && !!editor?.escape(),
      hasObjects: inFrame && !!editor?.hasObjects(),
      armedAt,
      now,
    });
    if (out === "gesture") cancelGesture();
    else if (out === "arm") armClose(now);
    else if (out === "cancel") void cancelSession();
  }

  /** The Cancel button / the editor's `close` intent: only the last step of the ladder. */
  function requestClose(): void {
    const now = Date.now();
    const out = escapeLadder({ hostGesture: false, editorEscaped: false, hasObjects: inFrame && !!editor?.hasObjects(), armedAt, now });
    if (out === "arm") armClose(now);
    else void cancelSession();
  }

  function cancelGesture(): void {
    const g = gesture;
    if (g?.kind === "move" || g?.kind === "resize") host = { ...host, frame: g.start };
    gesture = null;
    kbAnchor = null;
  }

  function onPanelAction(a: PanelAction): void {
    if (a === "cancel") requestClose();
    else void commit(a);
  }

  function onIntent(i: EditorIntent): void {
    if (i === "close") requestClose();
    else void commit(i);
  }

  // ── pointer ──

  function track(e: PointerEvent): void {
    cursorView = { x: e.clientX, y: e.clientY };
    cursor = framePoint(cursorView, k, frameSize);
    shiftHeld = e.shiftKey;
    altHeld = e.altKey;
  }

  /** Host gestures capture on the root, so they survive the shade boxes changing under them. */
  function grab(e: PointerEvent, g: Gesture): void {
    try {
      root?.setPointerCapture(e.pointerId);
    } catch {
      // the pointer is already gone (released before the handler ran): the window listeners still see it
    }
    gesture = g;
  }

  function onSelectDown(e: PointerEvent): void {
    if (host.stage !== "select") return;
    if (e.button === 2) {
      if (gesture) cancelGesture(); // right click during a drag cancels it; otherwise nothing
      return;
    }
    if (e.button !== 0) return;
    track(e);
    kbAnchor = null;
    grab(e, { kind: "press", id: e.pointerId, down: cursorView!, anchor: cursor! });
  }

  /** May a drag outside the frame start a new area? Only without a drawing tool (SPEC §9.2). */
  const canRedraw = () => !editor || editor.tool() === "select";

  function onOutsideDown(e: PointerEvent): void {
    if (host.stage !== "frame" || !frameView || !host.frame) return;
    if (e.button === 2) {
      if (gesture) cancelGesture();
      return;
    }
    if (e.button !== 0) return;
    track(e);
    const hit = hitFrame(cursorView!, frameView, view);
    if (hit.kind === "handle") onHandle(hit.handle, e);
    else if (hit.kind === "band") onChip(e);
    else if (hit.kind === "outside" && canRedraw()) grab(e, { kind: "press", id: e.pointerId, down: cursorView!, anchor: cursor! });
  }

  function onOutsideMove(e: PointerEvent): void {
    if (gesture || !frameView) return;
    const hit = hitFrame({ x: e.clientX, y: e.clientY }, frameView, view);
    outsideCursor = hit.kind === "band" ? "move" : hit.kind === "handle" ? HANDLE_CURSOR[hit.handle] : canRedraw() ? "crosshair" : "default";
  }

  function onHandle(handle: Handle, e: PointerEvent): void {
    if (host.stage !== "frame" || !host.frame || e.button !== 0) return;
    track(e);
    grab(e, { kind: "resize", id: e.pointerId, down: cursorView!, start: host.frame, handle });
  }

  function onChip(e: PointerEvent): void {
    if (host.stage !== "frame" || !host.frame || e.button !== 0) return;
    track(e);
    grab(e, { kind: "move", id: e.pointerId, down: cursorView!, start: host.frame });
  }

  function onPointerMove(e: PointerEvent): void {
    if (!session) return;
    // An editor gesture whose release never reached this window (let go over another monitor)
    // would keep the panel see-through and click-through: no button held = it is over (W7-R4).
    if (editorGesture && (e.buttons & 1) === 0) editorGesture = false;
    const g = gesture;
    if (g && g.id === e.pointerId && e.buttons === 0) return finishGesture(e); // released outside the window
    track(e);
    if (!g || g.id !== e.pointerId) return;
    const at = cursorView!;
    const c = cursor!;
    if (g.kind === "press") {
      if (isDrag(g.down, at)) gesture = { kind: "region", id: g.id, anchor: g.anchor, last: c };
    } else if (g.kind === "region") {
      // Space held: the area moves with the cursor instead of growing
      const anchor = spaceHeld ? clampPoint({ x: g.anchor.x + c.x - g.last.x, y: g.anchor.y + c.y - g.last.y }, frameSize) : g.anchor;
      gesture = { ...g, anchor, last: c };
    } else {
      const dx = Math.round((at.x - g.down.x) * k.kx);
      const dy = Math.round((at.y - g.down.y) * k.ky);
      const frame = g.kind === "move" ? moveRect(g.start, dx, dy, frameSize) : resizeRect(g.start, g.handle, dx, dy, frameSize);
      host = { ...host, frame };
    }
  }

  function finishGesture(e: PointerEvent): void {
    const g = gesture;
    if (!g) return;
    if (root?.hasPointerCapture(e.pointerId)) root.releasePointerCapture(e.pointerId);
    if (g.kind === "press") {
      gesture = null;
      if (host.stage === "select") pick(targetRect(cursor ?? g.anchor)); // a click: the window under it
    } else if (g.kind === "region") {
      pick(regionFrom(g.anchor, cursor ?? g.last, { square: shiftHeld, fromCenter: altHeld }, frameSize));
    } else gesture = null;
  }

  function onPointerUp(e: PointerEvent): void {
    if (e.button === 0) editorGesture = false;
    if (!gesture || gesture.id !== e.pointerId) return;
    track(e);
    finishGesture(e);
  }

  function onPointerCancel(e: PointerEvent): void {
    editorGesture = false;
    if (gesture?.id === e.pointerId) cancelGesture();
  }

  function onEditorDown(e: PointerEvent): void {
    if (e.button === 0) editorGesture = true;
  }

  function onWheel(e: WheelEvent): void {
    if (!loupeOn || e.deltaY === 0) return;
    e.preventDefault();
    zoom = zoomBy(zoom, e.deltaY < 0 ? 1 : -1);
  }

  // ── keys (KeyboardEvent.code — the Russian layout too) ──

  const isEditable = (el: EventTarget | null) =>
    el instanceof HTMLElement && (el.isContentEditable || el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement);

  function moveCursor(p: Point): void {
    cursor = clampPoint(p, frameSize);
    cursorView = toView({ x: cursor.x + 0.5, y: cursor.y + 0.5 }, k);
  }

  /** Enter in stage 1: the first starts an area at the cursor, the second takes it (an empty one — the target under the cursor). */
  function keyboardEnter(): void {
    const c = cursor ?? center;
    if (!kbAnchor) {
      kbAnchor = c;
      moveCursor(c);
      return;
    }
    const r = regionFrom(kbAnchor, c, { square: false, fromCenter: false }, frameSize);
    pick(r.w >= 1 && r.h >= 1 ? r : targetRect(c));
  }

  function selectKey(e: KeyboardEvent): boolean {
    const plain = !e.ctrlKey && !e.altKey && !e.metaKey;
    if (e.code === "Escape") {
      escape();
      return true;
    }
    if (e.code === "Space") {
      spaceHeld = true; // moves the area being drawn
      return true;
    }
    if (!plain) return false;
    if (e.code === "Enter" || e.code === "NumpadEnter") {
      if (!gesture) keyboardEnter();
      return true;
    }
    const d = arrowDelta(e.code);
    if (d) {
      const step = e.shiftKey ? 10 : 1;
      const c = cursor ?? center;
      moveCursor({ x: c.x + d.x * step, y: c.y + d.y * step });
      return true;
    }
    if (e.shiftKey || gesture) return false;
    switch (e.code) {
      case "KeyF":
        pick(fullRect(frameSize));
        return true;
      case "KeyR":
        if (session?.lastRegion) pick(session.lastRegion);
        return true;
      case "KeyM":
        magnifier = !magnifier;
        return true;
      default:
        return false;
    }
  }

  function frameKey(e: KeyboardEvent): boolean {
    if (editor?.handleKey(e)) return true; // the editor first (§18.7)
    if (e.code === "Escape") {
      escape();
      return true;
    }
    if (isEditable(e.target)) return false;
    const ctrl = e.ctrlKey && !e.metaKey;
    let action: ShotAction | null = null;
    if ((e.code === "Enter" || e.code === "NumpadEnter") && !ctrl && !e.altKey && !e.shiftKey) {
      if (e.target instanceof HTMLButtonElement) return false; // a keyboard-focused button: Enter presses it
      action = "done";
    } else if (ctrl && !e.altKey) {
      if (e.code === "KeyC" && !e.shiftKey) action = "copy";
      else if (e.code === "KeyS") action = e.shiftKey ? "save_as" : "save";
      else if (e.code === "KeyP" && !e.shiftKey) action = "pin";
    }
    if (action) {
      void commit(action);
      return true;
    }
    // Ctrl+arrows move the frame, Alt+arrows resize it (Shift — 10 px). Not Ctrl+Alt+arrows:
    // some Intel graphics drivers rotate the screen on it (INT-5).
    const d = arrowDelta(e.code);
    const mode = ctrl && !e.altKey ? "move" : e.altKey && !ctrl ? "resize" : null;
    if (d && mode && host.frame && !gesture) {
      host = { ...host, frame: nudgeRect(host.frame, d, e.shiftKey ? 10 : 1, mode, frameSize) };
      return true;
    }
    return false;
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Shift") shiftHeld = true;
    if (e.key === "Alt") {
      altHeld = true;
      e.preventDefault(); // no menu-key behaviour in the webview
    }
    if (e.isComposing) return;
    let handled = false;
    if (host.stage === "committing") handled = true;
    else if (host.stage === "frame") handled = frameKey(e);
    else if (host.stage === "select") handled = selectKey(e);
    // WebView2 browser keys (reload, print, …) never run here (FIX-15)
    if (handled || isBrowserShortcut(e, { find: true })) e.preventDefault();
  }

  function onKeyup(e: KeyboardEvent): void {
    if (e.key === "Shift") shiftHeld = false;
    if (e.key === "Alt") {
      altHeld = false;
      e.preventDefault();
    }
    if (e.code === "Space") spaceHeld = false;
  }

  function onBlur(): void {
    shiftHeld = false;
    altHeld = false;
    spaceHeld = false;
  }
</script>

<svelte:window
  bind:innerWidth={vw}
  bind:innerHeight={vh}
  onkeydown={onKeydown}
  onkeyup={onKeyup}
  onblur={onBlur}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerCancel}
  oncontextmenu={(e) => e.preventDefault()}
  ondragenter={refuseDrop}
  ondragover={refuseDrop}
  ondrop={refuseDrop}
/>

{#if session}
  <div bind:this={root} class="capture" style="cursor:{rootCursor}" onwheel={onWheel} role="application" aria-label={t("capture.label")} aria-busy={busy}>
    <img class="frame" crossorigin="anonymous" src={session.monitor.frameUrl} alt="" draggable="false" onload={onFrameLoad} onerror={onFrameError} />

    {#if inFrame && bitmap && editorState && editorDoc}
      {#key host.sessionId}
        <div class="editor-host" onpointerdowncapture={onEditorDown} role="presentation">
          <Editor bind:this={editor} state={editorState} base={bitmap} doc={editorDoc} mode="overlay" clip={host.frame} onintent={onIntent} />
        </div>
      {/key}
    {/if}

    <Shade
      {hole}
      {view}
      ring={drawing || selecting ? "hover" : "frame"}
      dim={inFrame ? DIM_FRAME : DIM_SELECT}
      interactive={inFrame}
      cursor={busy ? "wait" : outsideCursor}
      onpointerdown={onOutsideDown}
      onpointermove={onOutsideMove}
    />

    {#if selecting}
      <div class="surface" class:surface--busy={busy} onpointerdown={onSelectDown} role="presentation"></div>
    {/if}

    {#if drawingView && session.dimensions && drawing}
      <div class="size" class:size--inside={drawingView.y < 40} style="left:{Math.max(8, Math.min(drawingView.x, vw - 140))}px; top:{drawingView.y}px">
        {sizeText(drawing)}
      </div>
    {/if}

    {#if loupeOn && bitmap && cursor && cursorView}
      <Loupe {bitmap} pixel={cursor} at={cursorView} {view} cell={zoom} density={k.kx} caption={loupeCaption} />
    {/if}

    {#if selecting && !busy}
      <HintBar text={host.mode === "text"} last={!!session.lastRegion} top={!!cursorView && cursorView.y > vh - 120} hidden={!!drawing} />
    {/if}

    {#if inFrame && frameView && !drawing}
      <FrameChrome frame={frameView} {view} size={session.dimensions && host.frame ? sizeText(host.frame) : null} avoid={panelRect} {busy} gesture={anyGesture} onhandle={onHandle} onchip={onChip} />
      {#snippet tools()}
        {#if editorState}<Toolbar state={editorState} {tips} />{/if}
      {/snippet}
      {#snippet props()}
        {#if editorState}<PropsBar state={editorState} {tips} />{/if}
      {/snippet}
      <Panel
        tools={bitmap ? tools : null}
        props={bitmap ? props : null}
        x={docked.x}
        y={docked.y}
        {tips}
        ghost={panelGhost}
        passthrough={anyGesture}
        {busy}
        {ocrOff}
        measuring={!panelW}
        bind:width={panelW}
        bind:height={panelH}
        onaction={onPanelAction}
      />
    {/if}

    {#if notice}
      <Notice {notice} onclose={clearNotice} />
    {/if}
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  .capture {
    position: fixed;
    inset: 0;
    overflow: hidden;
    touch-action: none;
  }
  .frame {
    position: fixed;
    inset: 0;
    z-index: 0;
    width: 100%;
    height: 100%;
    object-fit: fill;
    pointer-events: none;
    user-select: none;
  }
  .editor-host {
    position: fixed;
    inset: 0;
    z-index: 1;
  }
  .surface {
    position: fixed;
    inset: 0;
    z-index: 4;
    cursor: crosshair;
    touch-action: none;
  }
  .surface--busy {
    cursor: wait;
  }
  /* live W × H while an area is drawn (Settings → "Show sizes"), above its top-left corner */
  .size {
    position: fixed;
    z-index: 6;
    display: inline-flex;
    align-items: center;
    height: var(--h-sm);
    padding: 0 10px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 500;
    white-space: nowrap;
    transform: translateY(calc(-100% - 8px));
    pointer-events: none;
  }
  .size--inside {
    transform: translate(8px, 8px);
  }
</style>
