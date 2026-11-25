<script lang="ts">
  // Editor of one snippet: name, body (highlighted), live preview of the expansion
  // (`expandSnippet` — the same function the shelf pastes with), diagnostics, autosave
  // (debounced; flushed when switching snippets or leaving), delete with confirmation.
  import { onDestroy } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import { snippetSave } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import type { Snippet } from "$lib/ipc/types";
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { expandSnippet, parseSnippet } from "$lib/snippets/expand";
  import { formatCount, SNIPPET_NAME_MAX, validateSnippetBody, validateSnippetName } from "../logic";
  import BodyEditor from "./BodyEditor.svelte";
  import Reference from "./Reference.svelte";

  interface Props {
    snippet: Snippet;
    /** Text for {clipboard} in the preview (latest copied text), or null. */
    clipboard: string | null;
    focusName?: boolean;
    onSaved: (s: Snippet) => void;
    onDelete: () => void;
  }
  let { snippet, clipboard, focusName = false, onSaved, onDelete }: Props = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(snippet.name);
  // svelte-ignore state_referenced_locally
  let body = $state(snippet.body);
  let saveState = $state<"idle" | "saving" | "saved" | "error">("idle");
  let saveError = $state<string | null>(null);
  let dirty = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let editor = $state<ReturnType<typeof BodyEditor>>();

  const nameIssue = $derived(validateSnippetName(name));
  const bodyIssue = $derived(validateSnippetBody(body));
  const nameError = $derived(
    nameIssue === null ? null : nameIssue === "tooLong" ? t("settings.name.tooLong", { max: SNIPPET_NAME_MAX }) : t(`settings.name.${nameIssue}`),
  );

  // Preview: one sample uuid per editor so the preview doesn't flicker while typing.
  const sampleUuid = typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : "3f2b8c1e-5d4a-4e7b-9c0d-1a2b3c4d5e6f";
  let now = $state(Date.now());
  const tick = setInterval(() => (now = Date.now()), 30_000);
  const preview = $derived(
    expandSnippet(body, {
      now,
      clipboard: clipboard ?? t("settings.snippets.preview.sample"),
      uuid: () => sampleUuid,
      locale: getLocale(),
    }),
  );
  const issues = $derived(parseSnippet(body).filter((tok) => tok.kind === "placeholder" && tok.issue !== null));
  const usesClipboard = $derived(parseSnippet(body).some((tok) => tok.kind === "placeholder" && tok.name === "clipboard" && !tok.issue));

  function schedule() {
    dirty = true;
    saveState = "idle";
    clearTimeout(timer);
    timer = setTimeout(() => void save(), 500);
  }

  async function save() {
    clearTimeout(timer);
    if (!dirty || nameIssue || bodyIssue) return;
    dirty = false;
    saveState = "saving";
    try {
      const saved = await snippetSave({ id: snippet.id, name: name.trim(), body, keyword: snippet.keyword });
      onSaved(saved);
      saveState = dirty ? "idle" : "saved";
      saveError = null;
    } catch (e) {
      dirty = true;
      saveState = "error";
      saveError = errorMessage(e);
    }
  }

  onDestroy(() => {
    clearInterval(tick);
    if (dirty) void save();
  });

  const ids = $props.id();
  const ISSUE_KEYS = {
    unknown: "settings.snippets.issue.unknown",
    syntax: "settings.snippets.issue.syntax",
    attribute: "settings.snippets.issue.attribute",
    modifier: "settings.snippets.issue.modifier",
    format: "settings.snippets.issue.format",
    offset: "settings.snippets.issue.offset",
    cursor: "settings.snippets.issue.cursor",
  } as const;

  function focusSelect(node: HTMLInputElement) {
    if (!focusName) return;
    queueMicrotask(() => {
      node.focus();
      node.select();
    });
  }
</script>

<div class="editor">
  <div class="field">
    <label class="field__label" for="{ids}-name">{t("settings.snippets.name")}</label>
    <input
      id="{ids}-name"
      class="name squircle selectable"
      type="text"
      spellcheck="false"
      autocomplete="off"
      bind:value={name}
      use:focusSelect
      aria-invalid={!!nameError || undefined}
      aria-describedby={nameError ? `${ids}-ne` : undefined}
      oninput={schedule}
      onblur={() => void save()}
    />
    {#if nameError}<p class="msg" id="{ids}-ne" role="alert"><Ico name="warning" size={14} />{nameError}</p>{/if}
  </div>

  <div class="field">
    <label class="field__label" for="{ids}-body">{t("settings.snippets.body")}</label>
    <p class="field__desc" id="{ids}-bd">{t("settings.snippets.body.desc")}</p>
    <BodyEditor
      bind:this={editor}
      id="{ids}-body"
      value={body}
      invalid={!!bodyIssue}
      describedby="{ids}-bd{issues.length ? ` ${ids}-is` : ''}"
      oninput={(v) => {
        body = v;
        schedule();
      }}
    />
    {#if issues.length || bodyIssue}
      <ul class="issues" id="{ids}-is">
        {#if bodyIssue}<li><Ico name="warning" size={14} />{t("settings.snippets.body.tooLong")}</li>{/if}
        {#each issues as tok (tok.start)}
          {#if tok.kind === "placeholder" && tok.issue}
            <li><Ico name="info" size={14} />{t(ISSUE_KEYS[tok.issue] as TranslationKey, { ph: tok.raw })}</li>
          {/if}
        {/each}
      </ul>
    {/if}
  </div>

  <div class="field">
    <span class="field__label" id="{ids}-pl">{t("settings.snippets.preview")}</span>
    <p class="field__desc">{t("settings.snippets.preview.desc")}</p>
    <div class="preview squircle selectable" role="region" aria-labelledby="{ids}-pl" aria-live="polite">
      {#if preview.text}
        {#if preview.caretIndex === null}{preview.text}{:else}{preview.text.slice(0, preview.caretIndex)}<span class="caret" aria-hidden="true"
          ></span>{preview.text.slice(preview.caretIndex)}{/if}
      {:else}
        <span class="preview__empty">{t("settings.snippets.preview.empty")}</span>
      {/if}
    </div>
    {#if usesClipboard}<p class="field__desc">{t("settings.snippets.preview.clipboard")}</p>{/if}
  </div>

  <Reference
    onInsert={(text) => editor?.insert(text)}
    ctx={{ now, clipboard: clipboard ?? t("settings.snippets.preview.sample"), uuid: () => sampleUuid, locale: getLocale() }}
  />

  <footer class="foot">
    <span class="foot__state" aria-live="polite">
      {#if saveState === "saving"}
        {t("settings.snippets.saving")}
      {:else if saveState === "saved"}
        <Ico name="check" size={14} />{t("settings.snippets.saved")}
      {:else if saveState === "error"}
        <Ico name="warning" size={14} />{t("settings.error.notSaved", { reason: saveError ?? "" })}
        <Button variant="ghost" size="sm" onclick={() => ((dirty = true), void save())}>{t("common.retry")}</Button>
      {:else if snippet.useCount > 0}
        {t("settings.snippets.used", { n: formatCount(snippet.useCount, getLocale()) })}
      {/if}
    </span>
    <Button variant="ghost" size="sm" icon="trash" onclick={onDelete} style="color: var(--color-danger)">{t("settings.snippets.delete")}</Button>
  </footer>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 18px 18px 14px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field__label {
    font-size: var(--text-row);
    font-weight: 600;
    color: var(--color-ink);
  }
  .field__desc {
    margin: -2px 0 2px;
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink-3);
  }
  .name {
    height: 38px;
    padding: 0 12px;
    border: 0;
    border-radius: 12px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-panel);
    font-weight: 600;
    outline: none;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .name:hover {
    background: var(--color-hover);
  }
  .name:focus {
    background: var(--color-selected);
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
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
    font-weight: 600;
    color: var(--color-ink);
  }
  .msg :global(.ico),
  .issues :global(.ico) {
    margin-top: 2px;
    flex: 0 0 auto;
  }
  .issues {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 2px 0 0;
    padding: 0;
    list-style: none;
    color: var(--color-ink-2);
  }
  .preview {
    min-height: 64px;
    max-height: 240px;
    overflow: auto;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--s-inset);
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    color: var(--color-ink);
  }
  .preview__empty {
    font-family: var(--font-sans);
    color: var(--color-ink-3);
  }
  .caret {
    display: inline-block;
    width: 2px;
    height: 1.25em;
    margin: 0 -1px;
    vertical-align: text-bottom;
    border-radius: 1px;
    background: var(--color-ink);
    animation: caret-blink 1.1s steps(1) infinite;
  }
  @keyframes caret-blink {
    50% {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .caret {
      animation: none;
    }
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding-top: 4px;
  }
  .foot__state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    font-size: var(--text-secondary);
    color: var(--color-ink-3);
  }
</style>
