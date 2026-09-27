import { SvelteSet } from "svelte/reactivity";
import { addFile, addLink, deleteTrack, getTrack, onEngine, startAdding, type EngineEvent, type Track } from "$lib/api";
import { say } from "$lib/i18n/engine";
import { t, type Key } from "$lib/i18n/index.svelte";
import { library } from "./library.svelte";
import { player } from "./player.svelte";
import { toasts } from "./toasts.svelte";
import { ui } from "./ui.svelte";
import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

type Then = "" | "queue" | "next";

interface Pending {
  title: string;
  link: string | null;
  then: Then;
  toast: ReturnType<typeof toasts.show>;
}

class Adding {
  /** Songs still being fetched; their rows show a spinner. */
  ids = new SvelteSet<number>();
  private pending = new Map<number, Pending>();

  async init() {
    await onEngine((e) => void this.follow(e));
  }

  async link(url: string, then: Then = "", title = "") {
    try {
      await this.start(await addLink(url), title, url, then);
    } catch (e) {
      toasts.show(say(e), { icon: WarningIcon });
    }
  }

  async file(path: string) {
    try {
      await this.start(await addFile(path), "", null, "");
    } catch (e) {
      toasts.show(say(e), { icon: WarningIcon });
    }
  }

  /** Opens Imported at a song and highlights it for a moment. */
  async reveal(id: number) {
    ui.karaoke = false;
    ui.clearSearch();
    await library.setKind("playlist");
    const imported = library.cards.find((c) => !c.user);
    if (imported) await library.select(imported.id);
    ui.flash = id;
    setTimeout(() => {
      if (ui.flash === id) ui.flash = null;
    }, 1400);
    const smooth = !matchMedia("(prefers-reduced-motion: reduce)").matches;
    requestAnimationFrame(() => document.querySelector(`[data-track="${id}"]`)?.scrollIntoView({ block: "center", behavior: smooth ? "smooth" : "auto" }));
  }

  /** The page's songs that can be queued now (not still being added). */
  singable(tracks: Track[]): number[] {
    return tracks.filter((x) => !this.ids.has(x.id)).map((x) => x.id);
  }

  private async start(track: Track, title: string, link: string | null, then: Then) {
    if (this.ids.has(track.id)) return;
    const name = title || (link ? t("adding.fromHost", { host: new URL(link).hostname.replace(/^www\./, "") }) : track.title);
    this.ids.add(track.id);
    const toast = toasts.show(t("adding.started", { title: name }), { spin: true, sticky: true });
    this.pending.set(track.id, { title: name, link, then, toast });
    const started = await startAdding(track.id).catch((e) => {
      toast.update(say(e), { icon: WarningIcon });
      return null;
    });
    if (started === null) return this.finish(track.id);
    if (!started) {
      this.finish(track.id);
      toast.update(t("adding.already", { title: track.title }), { art: track, onClick: () => void this.reveal(track.id) });
      if (then) await player.enqueue(track.id, then === "next");
      return;
    }
    await library.refresh();
  }

  private async follow(e: EngineEvent) {
    const p = this.pending.get(e.trackId);
    if (!p) return;
    if (e.kind === "stage") {
      const step: Key = e.stage === "fetching" ? (p.link ? "adding.downloading" : "adding.reading") : e.stage === "findingLyrics" ? "adding.lyrics" : "adding.preparing";
      p.toast.update(t(step, { title: p.title }), { spin: true, sticky: true });
    } else if (e.kind === "added") {
      const track = await getTrack(e.trackId);
      await library.refresh();
      this.finish(e.trackId);
      p.toast.update(t("adding.done", { title: track.title }), { art: track, onClick: () => void this.reveal(track.id) });
      if (p.then) await player.enqueue(track.id, p.then === "next");
    } else if (e.kind === "failed") {
      const link = p.link;
      if (link) await deleteTrack(e.trackId);
      await library.refresh();
      this.finish(e.trackId);
      const retry = link ? { label: t("common.tryAgain"), icon: ArrowClockwiseIcon, run: () => void this.link(link, p.then, p.title) } : undefined;
      p.toast.update(say(e), { icon: WarningIcon, action: retry });
    }
  }

  private finish(id: number) {
    this.ids.delete(id);
    this.pending.delete(id);
  }

  /** A row's song, shown under the title it was added with until its real details arrive. */
  shown(track: Track): Track {
    const p = this.pending.get(track.id);
    return p && this.ids.has(track.id) ? { ...track, title: p.title } : track;
  }
}

export const adding = new Adding();
