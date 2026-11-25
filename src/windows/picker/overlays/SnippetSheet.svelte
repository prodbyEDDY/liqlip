<script lang="ts">
  // Snippet sheet (SPEC §3.10): "Save as snippet" from a text record, "New snippet" (the
  // Snippets pill menu, the actions menu) and editing a snippet from the shelf. Name + body
  // (the settings window's highlighting editor) with a live preview of the expansion — the
  // same expander the paste uses — its problems listed, and placeholder chips that insert at
  // the caret. Ctrl+S (or Ctrl+Enter) saves through `snippet_save`; Esc cancels, asking first
  // when something was typed.
  import { onMount, tick, untrack } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { LayerFooter, LayerHeader, LayerSurface } from "./kit";
  import { clipDetail, snippetSave } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { Snippet } from "$lib/ipc/types";
  import { parseSnippet, type PlaceholderToken } from "$lib/snippets/expand";
  import BodyEditor from "../../settings/snippets/BodyEditor.svelte";
  import { SNIPPET_NAME_MAX, validateSnippetBody, validateSnippetName } from "../../settings/logic";
  import type { LayerProps } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";
  import { toasts } from "../toasts.svelte";
  import { confirmDiscard } from "./layerActions";
  import { clipboardText, previewExpansion, usesClipboard } from "./snippetText";
  import { focusOnce, trapTab } from "./focus";

  let { layer, close }: LayerProps = $props();
  const uid = $props.id();

  type Mode = "new" | "edit" | "fromClip";
  const mode = untrack(() => (layer.ctx.mode as Mode | undefined) ?? "new");
  const editing: Snippet | null = untrack(() => (layer.ctx.snippet as Snippet | undefined) ?? null);
  const clipId = untrack(() => (layer.ctx.clipId as number | undefined) ?? null);

  let name = $state(editing?.name ?? "");
  let body = $state(editing?.body ?? "");
  let initial = { name: editing?.name ?? "", body: editing?.body ?? "" };
  let loading = $state(mode === "fromClip");
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let tried = $state(false);
  let sheet = $state<HTMLDivElement | null>(null);
  let nameEl = $state<HTMLInputElement | null>(null);
  let editor = $state<ReturnType<typeof BodyEditor> | null>(null);

  // Preview: {clipboard} shows what it would expand to now; one sample UUID (no flicker).
  const sampleUuid = "3f2b8c1e-5d4a-4e7b-9c0d-1a2b3c4d5e6f";
  let clipSample = $state<string | null>(null);
  let now = $state(Date.now());

  /** "Save as snippet": the name is the record's title or its first line. */
  function defaultName(text: string, title: string | null): string {
    if (title?.trim()) return title.trim().slice(0, SNIPPET_NAME_MAX);
    const line = text.split(/\r?\n/).find((l) => l.trim()) ?? "";
    const s = line.trim().replace(/\s+/g, " ");
    return s.length > 48 ? `${s.slice(0, 47).trimEnd()}…` : s;
  }

  async function loadClip(id: number) {
    loading = true;
    loadError = null;
    try {
      const d = await clipDetail(id);
      body = d.text ?? d.preview;
      name = defaultName(body, d.title);
      initial = { name, body };
    } catch (e) {
      loadError = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (clipId !== null) void loadClip(clipId);
    clipboardText()
      .then((s) => (clipSample = s))
      .catch(() => {});
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(timer);
  });

  // New / edit: straight into the name; "save as" waits for the text, then selects the name.
  const focused = { v: false };
  $effect(() => {
    if (mode === "fromClip") focusOnce(sheet, focused);
    else focusOnce(nameEl, focused, (el) => el.focus({ preventScroll: true }));
  });

  // After "Save as snippet" loads, the name is ready to be adjusted.
  $effect(() => {
    if (!loading && mode === "fromClip" && !loadError) {
      untrack(() =>
        void tick().then(() => {
          nameEl?.focus({ preventScroll: true });
          nameEl?.select();
        }),
      );
    }
  });

  const preview = $derived(previewExpansion(body, clipSample ?? t("settings.snippets.preview.sample"), now, sampleUuid));
  const issues = $derived(parseSnippet(body).filter((tok): tok is PlaceholderToken => tok.kind === "placeholder" && tok.issue !== null));
  const nameIssue = $derived(validateSnippetName(name));
  const nameError = $derived(
    nameIssue === null ? null : nameIssue === "tooLong" ? t("settings.name.tooLong", { max: SNIPPET_NAME_MAX }) : t(`settings.name.${nameIssue}` as TranslationKey),
  );
  const bodyError = $derived(body.length === 0 ? t("picker.snippet.bodyEmpty") : validateSnippetBody(body) ? t("settings.snippets.body.tooLong") : null);
  const dirty = $derived(name !== initial.name || body !== initial.body);

  const PLACEHOLDERS: [string, TranslationKey][] = [
    ["{clipboard}", "settings.snippets.ph.clipboard"],
    ["{date}", "settings.snippets.ph.date"],
    ["{time}", "settings.snippets.ph.time"],
    ["{datetime}", "settings.snippets.ph.datetime"],
    ["{weekday}", "settings.snippets.ph.weekday"],
    ["{uuid}", "settings.snippets.ph.uuid"],
    ["{cursor}", "settings.snippets.ph.cursor"],
  ];
  const ISSUE_KEYS: Record<NonNullable<PlaceholderToken["issue"]>, TranslationKey> = {
    unknown: "settings.snippets.issue.unknown",
    syntax: "settings.snippets.issue.syntax",
    attribute: "settings.snippets.issue.attribute",
    modifier: "settings.snippets.issue.modifier",
    format: "settings.snippets.issue.format",
    offset: "settings.snippets.issue.offset",
    cursor: "settings.snippets.issue.cursor",
  };

  async function save() {
    tried = true;
    if (saving || loading || nameError || bodyError) {
      if (nameError) nameEl?.focus();
      return;
    }
    saving = true;
    saveError = null;
    try {
      await snippetSave({ id: editing?.id, name: name.trim(), body, keyword: editing?.keyword ?? null });
      close();
      const elsewhere = shelf.pill !== "snippets";
      toasts.show(t("picker.toast.snippetSaved"), {
        icon: "document",
        action: elsewhere ? { label: t("picker.toast.show"), icon: "eye", run: () => shelf.setPill("snippets") } : undefined,
      });
    } catch (e) {
      saveError = errorMessage(e);
    } finally {
      saving = false;
    }
  }

  function cancel() {
    if (dirty && !loading) confirmDiscard(close, () => void tick().then(() => nameEl?.focus({ preventScroll: true })));
    else close();
  }

  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey;
      if (ctrl && (e.code === "KeyS" || e.code === "Enter" || e.code === "NumpadEnter")) {
        void save();
        return true;
      }
      if (e.key === "Escape") {
        cancel();
        return true;
      }
      return trapTab(e, sheet);
    };
  });

  const title = $derived(t(mode === "edit" ? "picker.snippet.titleEdit" : mode === "fromClip" ? "picker.snippet.titleFrom" : "picker.snippet.titleNew"));
</script>

<LayerSurface variant="sheet" labelledby="{uid}-title" onclose={cancel} bind:el={sheet}>
  {#snippet header()}
    <LayerHeader {title} id="{uid}-title" icon="document" onclose={cancel} />
  {/snippet}

  <div class="wrap">
    <div class="form">
      <div class="col col--edit">
        <label class="field">
          <span class="field__label">{t("picker.snippet.name")}</span>
          <input
            bind:this={nameEl}
            bind:value={name}
            class="name selectable"
            type="text"
            spellcheck="false"
            autocomplete="off"
            maxlength={SNIPPET_NAME_MAX + 20}
            placeholder={t("picker.snippet.namePlaceholder")}
            aria-invalid={(tried && !!nameError) || undefined}
            aria-describedby={tried && nameError ? `${uid}-ne` : undefined}
            disabled={loading}
          />
        </label>
        {#if tried && nameError}<p class="msg" id="{uid}-ne" role="alert"><Ico name="warning" size={14} />{nameError}</p>{/if}

        <div class="field field--grow">
          <span class="field__label" id="{uid}-bl">{t("picker.snippet.body")}</span>
          {#if loading}
            <div class="loading" aria-busy="true">
              <Skeleton width="80%" height="11px" radius={5} /><Skeleton width="92%" height="11px" radius={5} /><Skeleton width="58%" height="11px" radius={5} />
            </div>
          {:else if loadError}
            <div class="loading loading--error">
              <p><Ico name="warning" size={14} />{t("picker.ql.loadError")} · {loadError}</p>
              <Button variant="secondary" size="sm" icon="refresh" onclick={() => clipId !== null && void loadClip(clipId)}>{t("common.retry")}</Button>
            </div>
          {:else}
            <BodyEditor
              bind:this={editor}
              id="{uid}-body"
              value={body}
              invalid={tried && !!bodyError}
              describedby="{uid}-bd"
              oninput={(v) => (body = v)}
            />
          {/if}
          <p class="hint" id="{uid}-bd">{t("settings.snippets.body.desc")}</p>
          {#if tried && bodyError}<p class="msg" role="alert"><Ico name="warning" size={14} />{bodyError}</p>{/if}
        </div>

        <div class="chips" role="group" aria-label={t("settings.snippets.placeholders")}>
          {#each PLACEHOLDERS as [ph, desc] (ph)}
            <button
              type="button"
              class="chip"
              title="{t(desc)} — {t('settings.snippets.insert', { ph })}"
              aria-label={t("settings.snippets.insert", { ph })}
              disabled={loading || !!loadError}
              onmousedown={(e) => e.preventDefault()}
              onclick={() => editor?.insert(ph)}>{ph}</button
            >
          {/each}
        </div>
      </div>

      <div class="col col--preview">
        <span class="field__label" id="{uid}-pl">{t("settings.snippets.preview")}</span>
        <div class="preview selectable" role="region" aria-labelledby="{uid}-pl" aria-live="polite">
          {#if preview.text}
            {#if preview.caretIndex === null}{preview.text}{:else}{preview.text.slice(0, preview.caretIndex)}<span class="caret" aria-hidden="true"></span>{preview.text.slice(
                preview.caretIndex,
              )}{/if}
          {:else}
            <span class="preview__empty">{t("settings.snippets.preview.empty")}</span>
          {/if}
        </div>
        {#if usesClipboard(body)}<p class="hint">{t("settings.snippets.preview.clipboard")}</p>{/if}
        {#if issues.length}
          <ul class="issues">
            {#each issues as tok (tok.start)}
              <li><Ico name="info" size={14} />{t(ISSUE_KEYS[tok.issue!], { ph: tok.raw })}</li>
            {/each}
          </ul>
        {/if}
        {#if saveError}<p class="msg" role="alert"><Ico name="warning" size={14} />{t("settings.error.notSaved", { reason: saveError })}</p>{/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <LayerFooter hints={[["Ctrl S", t("common.save")], ["Esc", t("common.cancel")]]}>
      {#snippet actions()}
        <Button variant="ghost" size="sm" onclick={cancel}>{t("common.cancel")}</Button>
        <Button variant="primary" size="sm" icon="check" loading={saving} disabled={loading || !!loadError} onclick={() => void save()}>{t("common.save")}</Button>
      {/snippet}
    </LayerFooter>
  {/snippet}
</LayerSurface>

<style>
  .wrap {
    container-type: inline-size;
    flex: 1 1 auto;
    min-height: 0;
    margin-top: 6px;
    overflow-y: auto;
  }
  .form {
    display: grid;
    grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr);
    gap: 16px;
    padding: 2px 4px 4px;
  }
  @container (max-width: 560px) {
    .form {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field--grow {
    flex: 1 1 auto;
  }
  .field__label {
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  /* Name: a 32 px capsule (not in a corner of the sheet → free radius). */
  .name {
    height: var(--h-md);
    padding: 0 14px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    font-weight: 600;
    outline: none;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .name:hover:not(:focus) {
    background: var(--color-hover);
  }
  .name:focus {
    outline: var(--focus-ring);
    outline-offset: var(--focus-offset);
  }
  .name::placeholder {
    color: var(--color-ink-3);
    font-weight: 500;
  }
  .hint {
    margin: 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .msg,
  .issues li {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    margin: 0;
    font-size: var(--text-secondary);
    line-height: 1.45;
  }
  .msg {
    color: var(--color-ink);
    font-weight: 600;
  }
  .msg :global(.ico),
  .issues :global(.ico) {
    flex: 0 0 auto;
    margin-top: 2px;
  }
  .issues {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    color: var(--color-ink-2);
    list-style: none;
  }
  .loading {
    display: flex;
    flex-direction: column;
    gap: 9px;
    min-height: 120px;
    padding: 12px 14px;
    border-radius: var(--r-field);
    background: var(--color-track);
  }
  .loading--error {
    align-items: flex-start;
    gap: 10px;
  }
  .loading--error p {
    display: flex;
    gap: 7px;
    margin: 0;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    height: var(--h-sm);
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .chip:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .chip:active:not(:disabled) {
    background: var(--color-selected);
  }
  .chip:disabled {
    opacity: 0.45;
  }
  .preview {
    min-height: 72px;
    max-height: 260px;
    padding: 10px 14px;
    overflow: auto;
    border-radius: var(--r-field);
    background: var(--color-track);
    color: var(--color-ink);
    font-size: var(--text-row);
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .preview__empty {
    color: var(--color-ink-3);
  }
  .caret {
    display: inline-block;
    width: 2px;
    height: 1.15em;
    margin: 0 1px;
    border-radius: 999px;
    background: var(--color-ink);
    vertical-align: text-bottom;
  }
</style>
