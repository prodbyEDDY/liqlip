// Window-level UI services of the settings window: one toast at a time (with an optional
// action such as "Undo") and one modal dialog at a time (promise-based, so destructive
// flows read top to bottom: `if (await dialogs.confirm(…)) …`). Rendered by
// ui/ToastHost.svelte and ui/DialogHost.svelte, mounted once in Settings.svelte.
import type { IconName } from "$lib/icons";

export interface ToastAction {
  label: string;
  run: () => void | Promise<void>;
}

export interface ToastSpec {
  message: string;
  icon?: IconName;
  action?: ToastAction;
  /** ms; default 6000 with an action, 3200 without. */
  duration?: number;
}

class Toasts {
  current = $state<(ToastSpec & { id: number }) | null>(null);
  private timer: ReturnType<typeof setTimeout> | undefined;
  private seq = 0;

  show(spec: ToastSpec): void {
    clearTimeout(this.timer);
    const id = ++this.seq;
    this.current = { ...spec, id };
    this.timer = setTimeout(() => {
      if (this.current?.id === id) this.current = null;
    }, spec.duration ?? (spec.action ? 6000 : 3200));
  }

  dismiss(): void {
    clearTimeout(this.timer);
    this.current = null;
  }
}

export const toasts = new Toasts();

export interface DialogAction {
  value: string;
  label: string;
  /** primary = ink fill (one per dialog), danger = destructive, secondary = plain. */
  variant?: "primary" | "secondary" | "danger";
}

export interface DialogSpec {
  title: string;
  body?: string;
  actions: DialogAction[];
  /** Label of the dismiss button (Esc does the same). Default "Cancel". */
  cancel?: string;
}

class Dialogs {
  current = $state<(DialogSpec & { id: number; resolve: (v: string | null) => void }) | null>(null);
  private seq = 0;

  /** Show a dialog; resolves with the chosen action value, or `null` when dismissed. */
  ask(spec: DialogSpec): Promise<string | null> {
    this.current?.resolve(null);
    return new Promise((resolve) => {
      this.current = { ...spec, id: ++this.seq, resolve };
    });
  }

  /** Yes/no confirmation. */
  async confirm(spec: { title: string; body?: string; confirm: string; danger?: boolean }): Promise<boolean> {
    const v = await this.ask({
      title: spec.title,
      body: spec.body,
      actions: [{ value: "ok", label: spec.confirm, variant: spec.danger ? "danger" : "primary" }],
    });
    return v === "ok";
  }

  /** Called by DialogHost. */
  settle(value: string | null): void {
    const c = this.current;
    if (!c) return;
    this.current = null;
    c.resolve(value);
  }
}

export const dialogs = new Dialogs();
