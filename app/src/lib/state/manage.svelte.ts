import { deleteTrack, editTrack, type Track } from "$lib/api";
import type { Icon } from "$lib/icons";
import { say } from "$lib/i18n/engine";
import { t } from "$lib/i18n/index.svelte";
import { library } from "./library.svelte";
import { player } from "./player.svelte";
import { toasts } from "./toasts.svelte";
import ArrowCounterClockwiseIcon from "phosphor-svelte/lib/ArrowCounterClockwiseIcon";
import TrashIcon from "phosphor-svelte/lib/TrashIcon";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

const failed = (e: unknown) => void toasts.show(say(e), { icon: WarningIcon });

class Manage {
  /** Hides an item now and runs `commit` when its toast runs out, unless Undo is pressed. */
  undoable(key: string, text: string, icon: Icon, commit: () => Promise<unknown>) {
    library.hidden.add(key);
    toasts.show(text, {
      icon,
      action: { label: t("common.undo"), icon: ArrowCounterClockwiseIcon, run: () => library.hidden.delete(key) },
      onExpire: async () => {
        await commit().catch(failed);
        library.hidden.delete(key);
        await library.refresh();
        await player.refresh();
      },
    });
  }

  deleteSong(track: Track) {
    this.undoable(`track:${track.id}`, t("toast.deleted", { name: track.title }), TrashIcon, () => deleteTrack(track.id));
  }

  async edit(track: Track, title: string, artist: string, album: string) {
    try {
      await editTrack(track.id, title, artist, album);
    } catch (e) {
      return failed(e);
    }
    await library.refresh();
    await player.refresh();
    toasts.show(t("toast.saved"));
  }
}

export const manage = new Manage();
