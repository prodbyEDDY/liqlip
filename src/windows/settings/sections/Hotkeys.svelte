<script lang="ts">
  // Hotkeys: the four global actions (record / reset / remove, conflicts at the field) and
  // the read-only reference of shelf keys (SPEC §3.5).
  import Kbd from "$lib/components/Kbd.svelte";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { HOTKEY_ACTIONS } from "../logic";
  import { settings } from "../settings.svelte";
  import Group from "../ui/Group.svelte";
  import HotkeyRow from "../ui/HotkeyRow.svelte";

  // SPEC §3.5. Each entry: alternatives (each a chord of keycaps) → action. Keycap names
  // are printed in English on keyboards in both locales; words go through i18n (`@…`).
  const KEYS: [string[][], string][] = [
    [[["@typing"]], "search"],
    [[["↑"], ["↓"], ["←"], ["→"]], "navigate"],
    [[["Home"], ["End"]], "homeEnd"],
    [[["PgUp"], ["PgDn"]], "page"],
    [[["Enter"]], "paste"],
    [[["Shift", "Enter"]], "pastePlain"],
    [[["Ctrl", "Enter"]], "copy"],
    [[["Alt", "Enter"]], "pasteKeep"],
    [[["Ctrl", "1…9"]], "pasteN"],
    [[["Space"], ["Ctrl", "Y"]], "preview"],
    [[["Ctrl", "K"], ["@rightClick"], ["⋯"]], "actions"],
    [[["Ctrl", "D"]], "favorite"],
    [[["F2"]], "rename"],
    [[["Ctrl", "E"]], "edit"],
    [[["Ctrl", "O"]], "open"],
    [[["Ctrl", "Shift", "O"]], "reveal"],
    [[["Delete"]], "delete"],
    [[["Ctrl", "Z"]], "undo"],
    [[["Ctrl", "A"]], "selectAll"],
    [[["Ctrl", "Shift", "S"]], "stack"],
    [[["Ctrl", "Tab"], ["Ctrl", "Shift", "Tab"]], "pills"],
    [[["Ctrl", "F"]], "find"],
    [[["Ctrl", ","]], "settings"],
    [[["Esc"]], "escape"],
  ];

  // Conflicts that exist since startup show at the field as soon as the section opens.
  $effect(() => settings.watchConflicts());

  const cap = (k: string) => (k.startsWith("@") ? t(`settings.keys.k.${k.slice(1)}` as TranslationKey) : k);
</script>

<Group title={t("settings.hotkeys.global")}>
  {#each HOTKEY_ACTIONS as action (action)}
    <HotkeyRow {action} />
  {/each}
</Group>

<Group title={t("settings.hotkeys.reference")} desc={t("settings.hotkeys.reference.desc")}>
  <div class="ref">
    <table class="keys">
      <thead>
        <tr>
          <th scope="col">{t("settings.keys.col.keys")}</th>
          <th scope="col">{t("settings.keys.col.action")}</th>
        </tr>
      </thead>
      <tbody>
        {#each KEYS as [alts, action] (action)}
          <tr>
            <td class="keys__combo">
              {#each alts as chord, i (i)}
                {#if i > 0}<span class="keys__or">{t("settings.keys.or")}</span>{/if}
                <span class="keys__chord">
                  {#each chord as k, j (j)}
                    {#if j > 0}<span class="keys__plus">+</span>{/if}
                    {#if k.startsWith("@")}<span class="keys__word">{cap(k)}</span>{:else}<Kbd size="sm">{k}</Kbd>{/if}
                  {/each}
                </span>
              {/each}
            </td>
            <td class="keys__action">{t(`settings.keys.a.${action}` as TranslationKey)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</Group>

<style>
  .ref {
    padding: 0;
  }
  .keys {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
  }
  .keys th {
    padding: 10px 12px 6px;
    color: var(--color-ink-3);
    font-size: var(--text-micro);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
  }
  .keys td {
    padding: 6px 12px;
    vertical-align: middle;
  }
  /* Zebra rows: a faint wash, rounded like the tile's inner edge (r12 = 16 − 4). */
  .keys tbody tr:nth-child(odd) td {
    background: color-mix(in srgb, var(--color-ink) 3.5%, transparent);
  }
  .keys tbody tr td:first-child {
    border-radius: var(--r-tile-in) 0 0 var(--r-tile-in);
  }
  .keys tbody tr td:last-child {
    border-radius: 0 var(--r-tile-in) var(--r-tile-in) 0;
  }
  .keys__combo {
    width: 44%;
  }
  .keys__chord {
    white-space: nowrap;
  }
  .keys__chord {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin: 2px 0;
  }
  .keys__or {
    margin: 0 6px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .keys__plus {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .keys__word {
    font-size: var(--text-secondary);
    font-weight: 600;
    color: var(--color-ink-2);
  }
  .keys__action {
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink);
  }
</style>
