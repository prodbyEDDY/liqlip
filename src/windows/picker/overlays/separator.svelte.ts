// The separator of "Paste all" / "Copy all" / "Merge" (SPEC §3.7: new line / space / tab /
// nothing — remembered). Remembered per window in localStorage; until the user picks one,
// `settings.picker.mergeSeparator` is the default. Enter on a multi-selection uses it too.
import type { MergeSeparator } from "$lib/ipc/types";
import { shelf } from "../shelf.svelte";

const KEY = "liqlip.picker.separator";
const VALUES: readonly MergeSeparator[] = ["newline", "space", "tab", "none"];

/** The text a separator joins records with (Rust turns a lone LF into CRLF). */
export const SEPARATOR_TEXT: Record<MergeSeparator, string> = { newline: "\n", space: " ", tab: "\t", none: "" };

function load(): MergeSeparator | null {
  try {
    const v = localStorage.getItem(KEY);
    return VALUES.includes(v as MergeSeparator) ? (v as MergeSeparator) : null;
  } catch {
    return null;
  }
}

class SeparatorPref {
  #saved = $state<MergeSeparator | null>(load());

  get value(): MergeSeparator {
    return this.#saved ?? shelf.settings?.picker.mergeSeparator ?? "newline";
  }

  set(value: MergeSeparator): void {
    this.#saved = value;
    try {
      localStorage.setItem(KEY, value);
    } catch {
      /* private mode: remembered for this session only */
    }
  }

  get text(): string {
    return SEPARATOR_TEXT[this.value];
  }
}

export const separatorPref = new SeparatorPref();
