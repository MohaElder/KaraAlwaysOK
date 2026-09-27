import {
  addToPlaylist,
  createPlaylist,
  deletePlaylist,
  deleteTrack,
  editTrack,
  moveInPlaylist,
  removeFromPlaylist,
  renamePlaylist,
  type CollectionCard,
  type Track,
} from "$lib/api";
import type { Icon } from "$lib/icons";
import { say } from "$lib/i18n/engine";
import { t } from "$lib/i18n/index.svelte";
import { cardName, library } from "./library.svelte";
import { player } from "./player.svelte";
import { toasts } from "./toasts.svelte";
import { ui } from "./ui.svelte";
import ArrowCounterClockwiseIcon from "phosphor-svelte/lib/ArrowCounterClockwiseIcon";
import InfoIcon from "phosphor-svelte/lib/InfoIcon";
import MinusCircleIcon from "phosphor-svelte/lib/MinusCircleIcon";
import PlaylistIcon from "phosphor-svelte/lib/PlaylistIcon";
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
        await library.refresh();
        library.hidden.delete(key);
        await player.refresh();
      },
    });
  }

  /** Takes a song out of Up next now and out of the library (and the queue) when its Undo toast runs out. */
  async deleteSong(track: Track) {
    this.undoable(`track:${track.id}`, t("toast.deleted", { name: track.title }), TrashIcon, () => deleteTrack(track.id));
    for (const e of player.snapshot.entries.filter((e) => e.track.id === track.id)) await player.removeQueued(e.key);
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

  /** Hides a playlist now and deletes it when its Undo toast runs out. */
  async deletePlaylist(card: CollectionCard) {
    this.undoable(`playlist:${card.id}`, t("toast.deleted", { name: cardName(card) }), TrashIcon, () => deletePlaylist(card.id));
    await library.refresh();
  }

  /** Hides a song from a playlist now and removes it when its Undo toast runs out. */
  removeFromPlaylist(playlistId: number, track: Track) {
    const name = library.cards.find((c) => c.id === playlistId)?.name ?? "";
    this.undoable(`entry:${playlistId}:${track.id}`, t("toast.removedFrom", { name }), MinusCircleIcon, () => removeFromPlaylist(playlistId, track.id));
  }

  async addToPlaylist(card: CollectionCard, track: Track) {
    try {
      const added = await addToPlaylist(card.id, track.id);
      toasts.show(t(added ? "toast.addedTo" : "toast.alreadyIn", { name: card.name }), { icon: added ? PlaylistIcon : InfoIcon });
    } catch (e) {
      return failed(e);
    }
    await library.refresh();
  }

  /** Makes a playlist (with any given songs), opens it and starts typing its name in the sidebar. */
  async newPlaylist(trackIds: number[] = []) {
    let id: number;
    try {
      id = await createPlaylist(t("library.newPlaylist"), trackIds);
    } catch (e) {
      return failed(e);
    }
    ui.karaoke = false;
    ui.clearSearch();
    await library.show("playlist", id);
    ui.renaming = { id, place: "sidebar", isNew: true, withSongs: trackIds.length > 0 };
  }

  /** Ends typing a playlist's name: saves it, or removes a new playlist when typing was cancelled. */
  async finishRename(name: string | null) {
    const r = ui.renaming;
    if (!r) return;
    ui.renaming = null;
    try {
      if (name == null && r.isNew) await deletePlaylist(r.id);
      else if (name) await renamePlaylist(r.id, name);
    } catch (e) {
      failed(e);
    }
    await library.refresh();
    if (r.isNew && name != null) toasts.show(t(r.withSongs ? "toast.addedTo" : "toast.created", { name: name || t("library.newPlaylist") }), { icon: PlaylistIcon });
  }

  async move(playlistId: number, trackId: number, to: number) {
    await moveInPlaylist(playlistId, trackId, to).catch(failed);
    await library.refresh();
  }
}

export const manage = new Manage();
