<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Track } from "$lib/api";
  import { duration } from "$lib/format";
  import { t } from "$lib/i18n/index.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import Artwork from "./Artwork.svelte";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
  import WaveformIcon from "phosphor-svelte/lib/WaveformIcon";
  import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
  import DotsThreeIcon from "phosphor-svelte/lib/DotsThreeIcon";

  let { track, playing = false, busy = false, flash = false, grip, onTap, onNext, onMenu }: {
    track: Track;
    playing?: boolean;
    busy?: boolean;
    flash?: boolean;
    grip?: Snippet;
    onTap?: () => void;
    onNext?: () => void;
    onMenu?: (x: number, y: number) => void;
  } = $props();
</script>

<div
  class="row"
  class:on={playing}
  class:busy
  class:flash
  class:grip={!!grip}
  data-track={track.id}
  role="button"
  tabindex="0"
  aria-label={t("song.addToQueueLabel", { title: track.title })}
  onclick={() => !busy && onTap?.()}
  onkeydown={(e) => e.key === "Enter" && !busy && onTap?.()}
  oncontextmenu={(e) => {
    e.preventDefault();
    onMenu?.(e.clientX, e.clientY);
  }}
>
  {@render grip?.()}
  <Artwork {track}>
    {#if busy}<span class="spin"></span>{:else if playing}<WaveformIcon />{:else}<ListPlusIcon />{/if}
  </Artwork>
  <span class="grow">
    <div class="t ell">{track.title}</div>
    <div class="a ell">{[track.artist, track.album].filter(Boolean).join(" – ")}</div>
  </span>
  <button class="ib nx" use:tip={t("song.playNext")} onclick={(e) => { e.stopPropagation(); if (!busy) onNext?.(); }}><ArrowBendDownRightIcon size={18} /></button>
  <button
    class="ib nx"
    use:tip={t("common.more")}
    onclick={(e) => {
      e.stopPropagation();
      const r = e.currentTarget.getBoundingClientRect();
      onMenu?.(r.right, r.bottom);
    }}><DotsThreeIcon size={18} /></button>
  <span class="num">{duration(track.durationMs)}</span>
</div>

<style>
  .row { display: grid; grid-template-columns: 36px minmax(0, 1fr) auto auto auto; gap: var(--s3); align-items: center; width: 100%; height: var(--row); padding: 0 var(--s2); border-radius: var(--r-sm); cursor: pointer; transition: background-color var(--t) var(--ease), opacity var(--t) var(--ease); }
  .row.grip { grid-template-columns: 14px 36px minmax(0, 1fr) auto auto auto; }
  .row:hover { background: color-mix(in srgb, var(--text) 5%, transparent); }
  .row:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .row :global(.art > svg) { opacity: 0; transition: opacity var(--t) var(--ease); filter: drop-shadow(0 1px 2px rgba(0, 0, 0, .5)); }
  .row:hover :global(.art > svg), .row.on :global(.art > svg) { opacity: 1; }
  .nx { opacity: 0; }
  .row:hover .nx, .row:focus-within .nx { opacity: 1; }
  .t { font-weight: 500; transition: color var(--t) var(--ease); }
  .a { font-size: 12.5px; color: var(--muted); }
  .on .t { color: var(--accent); }
  .busy { pointer-events: none; opacity: .55; }
  .flash { background: color-mix(in srgb, var(--accent) 16%, transparent); }
</style>
