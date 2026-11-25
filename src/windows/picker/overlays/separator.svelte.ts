// The separator of "Paste all" / "Copy all" / "Merge" (SPEC §3.7: new line / space / tab /
// nothing — remembered). ONE value: `settings.picker.mergeSeparator`, the same one the
// settings window shows ("Separator between items"). Picking another one in the shelf saves
// it there (`settings_set`), so a later change in either place is never shadowed by a stale
// copy (W3: a per-window localStorage copy used to win over the setting forever). Until the
// save lands — or if it fails — the pick holds for this session. Enter on a multi-selection
// uses it too.
import { settingsGet, settingsSet } from "$lib/ipc/commands";
import type { MergeSeparator } from "$lib/ipc/types";
import { shelf } from "../shelf.svelte";

/** The text a separator joins records with (Rust turns a lone LF into CRLF). */
export const SEPARATOR_TEXT: Record<MergeSeparator, string> = { newline: "\n", space: " ", tab: "\t", none: "" };

class SeparatorPref {
  /** A pick not (yet) reflected in the settings. */
  #pending = $state<MergeSeparator | null>(null);

  get value(): MergeSeparator {
    return this.#pending ?? shelf.settings?.picker.mergeSeparator ?? "newline";
  }

  set(value: MergeSeparator): void {
    if (shelf.settings?.picker.mergeSeparator === value) {
      this.#pending = null;
      return;
    }
    this.#pending = value;
    void this.#save(value);
  }

  async #save(value: MergeSeparator): Promise<void> {
    try {
      // A fresh copy: fields the shelf doesn't watch (the settings window's bounds are
      // patched without a broadcast) must not be written back stale.
      const s = await settingsGet();
      const r = await settingsSet({ ...s, picker: { ...s.picker, mergeSeparator: value } });
      shelf.settings = r.settings;
      // Saved (or refused — then the setting wins, as everywhere else).
      if (this.#pending === value) this.#pending = null;
    } catch {
      /* the store is unavailable: the pick holds for this session */
    }
  }

  get text(): string {
    return SEPARATOR_TEXT[this.value];
  }
}

export const separatorPref = new SeparatorPref();
