<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
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

  let dragKey = $state<number | null>(null);
  let over = $state<number | null>(null);
  const current = $derived(player.snapshot.current);
  const upcoming = $derived(current == null ? [] : player.snapshot.entries.map((entry, index) => ({ entry, index })).filter(({ index }) => index > current));
  const dragFrom = $derived(upcoming.find(({ entry }) => entry.key === dragKey)?.index ?? null);

  /** The queue index of the upcoming row under the pointer. */
  function rowAt(e: PointerEvent) {
    const row = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-qi]");
    return row ? Number(row.dataset.qi) : null;
  }

  function drop(to: number | null) {
    const key = dragKey;
    const from = dragFrom;
    dragKey = over = null;
    if (key != null && from != null && to != null && from !== to) void player.moveQueued(key, to);
  }
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === "Escape" && dragKey != null) dragKey = over = null;
}} />

{#if ui.queueOpen}
  <aside class="qpanel glass" class:dk={ui.karaoke} aria-label={t("queue.title")} transition:slide={{ x: 24, y: 0 }}>
    <div class="qhead">
      <QueueIcon size={18} /><h2 class="grow">{t("queue.title")}</h2>
      <button class="ib" use:tip={t("common.close")} onclick={() => (ui.queueOpen = false)}><XIcon size={18} /></button>
    </div>
    <div class="qbody">
      {#if !player.current}
        <div class="qempty"><QueueIcon size={20} /><b>{t("queue.nothingPlaying")}</b><span>{t("queue.nothingPlayingBody")}</span></div>
      {:else}
        <p class="cap hstack"><WaveformIcon size={14} />{t("queue.nowPlaying")}</p>
        <div class="qrow">
          <Artwork track={player.current.track} />
          <span class="grow"><b class="ell">{player.current.track.title}</b><small class="ell">{player.current.track.artist ?? ""}</small></span>
          <span class="num">{duration(player.current.track.durationMs)}</span>
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
            <Artwork track={entry.track} />
            <span class="grow"><b class="ell">{entry.track.title}</b><small class="ell">{entry.track.artist ?? ""}</small></span>
            <button class="ib" use:tip={t("queue.remove")} onclick={() => player.removeQueued(entry.key)}><XIcon size={16} /></button>
          </div>
        {:else}
          <div class="qempty"><ListPlusIcon size={20} /><b>{t("queue.empty")}</b><span>{t("queue.emptyBody")}</span></div>
        {/each}
      {/if}
    </div>
  </aside>
{/if}

<style>
  .qpanel { position: fixed; z-index: 35; top: 92px; right: var(--s4); bottom: 104px; width: min(340px, calc(100% - 32px)); display: flex; flex-direction: column; padding: var(--s4) var(--s3) var(--s3); border-radius: var(--r-lg); }
  .qhead { display: flex; align-items: center; gap: var(--s2); padding: 0 var(--s1) var(--s3) var(--s2); }
  h2 { font: 800 20px/1.2 var(--display); letter-spacing: -.02em; }
  .qbody { flex: 1; min-height: 0; overflow: auto; }
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
