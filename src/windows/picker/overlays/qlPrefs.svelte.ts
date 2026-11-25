// Quick Look view preferences that outlive one opening (per window, localStorage): show
// invisible characters, keep the "Recognized text" block open.
const KEY = "liqlip.picker.quicklook";

function load(): { invisibles: boolean; ocrOpen: boolean } {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, unknown>;
    return { invisibles: v.invisibles === true, ocrOpen: v.ocrOpen === true };
  } catch {
    return { invisibles: false, ocrOpen: false };
  }
}

class QlPrefs {
  #v = $state(load());

  get invisibles(): boolean {
    return this.#v.invisibles;
  }
  set invisibles(value: boolean) {
    this.#v = { ...this.#v, invisibles: value };
    this.#save();
  }
  get ocrOpen(): boolean {
    return this.#v.ocrOpen;
  }
  set ocrOpen(value: boolean) {
    this.#v = { ...this.#v, ocrOpen: value };
    this.#save();
  }

  #save(): void {
    try {
      localStorage.setItem(KEY, JSON.stringify(this.#v));
    } catch {
      /* private mode: this session only */
    }
  }
}

export const qlPrefs = new QlPrefs();
