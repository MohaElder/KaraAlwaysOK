import type { Track } from "$lib/api";
import type { Icon } from "$lib/icons";

export interface ToastAction { label: string; icon: Icon; run: () => void }
export interface ToastOptions {
  icon?: Icon;
  spin?: boolean;
  art?: Pick<Track, "artSeed" | "artworkPath">;
  action?: ToastAction;
  onClick?: () => void;
  sticky?: boolean;
  onExpire?: () => void;
}
export interface Toast extends ToastOptions { id: number; text: string }

class Toasts {
  list = $state<Toast[]>([]);
  private seq = 0;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  /** Shows a toast; the returned controls change it (for progress) or close it. */
  show(text: string, options: ToastOptions = {}) {
    const id = ++this.seq;
    this.list.push({ id, text, ...options });
    this.arm(id);
    return { update: (t: string, o: ToastOptions = {}) => this.update(id, t, o), close: () => this.close(id) };
  }

  update(id: number, text: string, options: ToastOptions) {
    const i = this.list.findIndex((t) => t.id === id);
    if (i < 0) return;
    this.list[i] = { id, text, ...options };
    this.arm(id);
  }

  /** Closes a toast; `expired` also runs its `onExpire`. */
  close(id: number, expired = false) {
    clearTimeout(this.timers.get(id));
    this.timers.delete(id);
    const toast = this.list.find((t) => t.id === id);
    this.list = this.list.filter((t) => t.id !== id);
    if (expired) toast?.onExpire?.();
  }

  private arm(id: number) {
    clearTimeout(this.timers.get(id));
    const t = this.list.find((x) => x.id === id);
    if (!t || t.sticky) return;
    this.timers.set(id, setTimeout(() => this.close(id, true), t.action ? 6000 : 2800));
  }
}

export const toasts = new Toasts();
