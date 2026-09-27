<script lang="ts">
  import type { PlayerSnapshot, QueueEntry } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";
  import { duration } from "$lib/format";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import Artwork from "./Artwork.svelte";
  import QueueIcon from "phosphor-svelte/lib/QueueIcon";
  import XIcon from "phosphor-svelte/lib/XIcon";
  import WaveformIcon from "phosphor-svelte/lib/WaveformIcon";
  import DotsSixVerticalIcon from "phosphor-svelte/lib/DotsSixVerticalIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";

  let { snapshot, art = 36, nothingBody, emptyBody, onMove, onRemove }: {
    snapshot: PlayerSnapshot;
    art?: number;
    nothingBody: string;
    emptyBody: string;
    onMove: (key: number, to: number) => void;
    onRemove: (key: number) => void;
  } = $props();

  let dragKey = $state<number | null>(null);
  let over = $state<number | null>(null);
  const now = $derived(snapshot.current == null ? null : (snapshot.entries[snapshot.current] ?? null));
  const upcoming = $derived.by(() => {
    const c = snapshot.current;
    return c == null ? [] : snapshot.entries.map((entry, index) => ({ entry, index })).filter(({ index }) => index > c);
  });
  const dragFrom = $derived(upcoming.find(({ entry }) => entry.key === dragKey)?.index ?? null);

  /** The artist, and who added the song when a guest did. */
  const byline = (e: QueueEntry) => [e.track.artist, e.by && t("queue.addedBy", { name: e.by })].filter(Boolean).join(" · ");

  /** The queue index of the upcoming row under the pointer. */
  function rowAt(e: PointerEvent) {
    const row = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-qi]");
    return row ? Number(row.dataset.qi) : null;
  }

  function drop(to: number | null) {
    const key = dragKey;
    const from = dragFrom;
    dragKey = over = null;
    if (key != null && from != null && to != null && from !== to) onMove(key, to);
  }
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === "Escape" && dragKey != null) dragKey = over = null;
}} />

{#if !now}
  <div class="qempty"><QueueIcon size={20} /><b>{t("queue.nothingPlaying")}</b><span>{nothingBody}</span></div>
{:else}
  <p class="cap hstack"><WaveformIcon size={14} />{t("queue.nowPlaying")}</p>
  <div class="qrow">
    <Artwork track={now.track} size={art} />
    <span class="grow"><b class="ell">{now.track.title}</b><small class="ell">{byline(now)}</small></span>
    <span class="num">{duration(now.track.durationMs)}</span>
  </div>
  <p class="cap hstack"><QueueIcon size={14} />{t("queue.upNext", { n: upcoming.length })}</p>
  {#each upcoming as { entry, index } (entry.key)}
    <div
      class="qrow"
      class:dragging={dragFrom === index}
      class:over={dragFrom != null && over === index}
      class:below={dragFrom != null && dragFrom < index}
      data-qi={index}
      transition:slide={{ x: 8, y: 0 }}
    >
      <span
        class="grip"
        aria-hidden="true"
        use:tip={t("queue.drag")}
        onpointerdown={(e) => {
          if (e.button !== 0) return;
          e.preventDefault();
          e.currentTarget.setPointerCapture(e.pointerId);
          dragKey = entry.key;
        }}
        onpointermove={(e) => {
          if (dragFrom != null) over = rowAt(e);
        }}
        onpointerup={(e) => drop(rowAt(e))}
        onpointercancel={() => (dragKey = over = null)}
      ><DotsSixVerticalIcon size={16} /></span>
      <Artwork track={entry.track} size={art} />
      <span class="grow"><b class="ell">{entry.track.title}</b><small class="ell">{byline(entry)}</small></span>
      <button class="ib" use:tip={t("queue.remove")} onclick={() => onRemove(entry.key)}><XIcon size={16} /></button>
    </div>
  {:else}
    <div class="qempty"><ListPlusIcon size={20} /><b>{t("queue.empty")}</b><span>{emptyBody}</span></div>
  {/each}
{/if}

<style>
  .cap { padding: var(--s3) var(--s2) var(--s1); }
  .qrow { display: flex; align-items: center; gap: var(--s2); min-height: var(--row); padding: 0 var(--s1) 0 var(--s2); border-radius: var(--r-sm); transition: opacity var(--t) var(--ease), background-color var(--t) var(--ease); }
  .qrow[data-qi]:hover { background: color-mix(in srgb, var(--text) 5%, transparent); }
  .qrow b { display: block; font-weight: 500; }
  .qrow small { display: block; color: var(--muted); font-size: 12.5px; }
  .grip { color: var(--faint); cursor: grab; display: grid; touch-action: none; }
  .dragging { opacity: .4; }
  .over { box-shadow: inset 0 2px 0 var(--accent); }
  .over.below { box-shadow: inset 0 -2px 0 var(--accent); }
  .qempty { display: grid; justify-items: center; text-align: center; gap: var(--s2); padding: var(--s7) var(--s4); color: var(--muted); font-size: 13px; }
  .qempty b { color: var(--text); font-size: 15px; }
</style>
