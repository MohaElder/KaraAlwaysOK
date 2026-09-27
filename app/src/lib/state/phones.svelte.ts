import { onPhoneLevels, onPhoneNews, onPhones, phoneRemove, phonesClose, phonesOpen, phoneVolume, type PhoneLevel, type PhoneNews, type PhonesView } from "$lib/api";
import { say } from "$lib/i18n/engine";
import { t } from "$lib/i18n/index.svelte";
import { player } from "./player.svelte";
import { toasts } from "./toasts.svelte";
import { ui } from "./ui.svelte";
import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
import UserMinusIcon from "phosphor-svelte/lib/UserMinusIcon";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

/** The phone session as the Mac window shows it, and the toasts about what phones do. */
class PhonesState {
  view = $state<PhonesView>({ join: null, phones: [] });
  levels = $state<Record<string, PhoneLevel>>({});
  private shown = false;

  async init() {
    await onPhones((v) => {
      this.view = v;
      this.levels = Object.fromEntries(Object.entries(this.levels).filter(([id]) => v.phones.some((p) => p.id === id)));
      if (!v.join && this.shown) void this.open();
    });
    await onPhoneLevels((levels) => (this.levels = Object.fromEntries(levels.map((l) => [l.id, l]))));
    await onPhoneNews((n) => this.toast(n));
  }

  /** The window opened: starts the session if needed; if it can't, the window closes with the reason. */
  async open() {
    this.shown = true;
    try {
      const view = await phonesOpen();
      player.tellPhones();
      if (this.shown) this.view = view;
      else await phonesClose();
    } catch (e) {
      if (ui.sheet?.kind === "mics") ui.sheet = null;
      toasts.show(say(e), { icon: WarningIcon });
    }
  }

  async close() {
    this.shown = false;
    await phonesClose().catch(() => {});
  }

  setVolume(id: string, volume: number) {
    void phoneVolume(id, volume).catch(() => {});
  }

  remove(id: string, name: string) {
    void phoneRemove(id).catch(() => {});
    toasts.show(t("mics.removed", { name }), { icon: UserMinusIcon });
  }

  private toast(n: PhoneNews) {
    if (n.kind === "joined") toasts.show(t("mics.joined", { name: n.name, n: n.mic }), { icon: MicrophoneStageIcon });
    else toasts.show(t("mics.added", { name: n.name, title: n.title }), { icon: ListPlusIcon });
  }
}

export const phones = new PhonesState();
