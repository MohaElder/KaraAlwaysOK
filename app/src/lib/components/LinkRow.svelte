<script lang="ts">
  import type { LinkPreview } from "$lib/api";
  import type { Then } from "$lib/state/adding.svelte";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { duration } from "$lib/format";
  import { fade } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
  import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
  import YoutubeLogoIcon from "phosphor-svelte/lib/YoutubeLogoIcon";
  import SoundcloudLogoIcon from "phosphor-svelte/lib/SoundcloudLogoIcon";
  import TelevisionSimpleIcon from "phosphor-svelte/lib/TelevisionSimpleIcon";
  import GlobeIcon from "phosphor-svelte/lib/GlobeIcon";

  let {
    preview,
    host,
    label = "search.addToLibraryLabel",
    done = null,
    onAdd,
  }: { preview: LinkPreview | null; host?: string; label?: Key; done?: "queue" | "next" | null; onAdd: (then: Then) => void } = $props();
  const Site = $derived(
    host?.includes("youtu") ? YoutubeLogoIcon : host?.includes("soundcloud") ? SoundcloudLogoIcon : host?.includes("bilibili") || host?.includes("b23") ? TelevisionSimpleIcon : GlobeIcon,
  );
</script>

{#if !preview}
  <div class="lrow" in:fade><span class="lthumb"><i class="bone"></i></span><span class="grow"><i class="bone" style="width:55%"></i><i class="bone" style="width:30%"></i></span></div>
{:else}
  <div class="lrow" role="button" tabindex="0" aria-label={t(label, { title: preview.title })} in:fade onclick={() => onAdd("")} onkeydown={(e) => e.key === "Enter" && onAdd("")}>
    <span class="lthumb">
      {#if preview.thumbnail}<img src={preview.thumbnail} alt="" referrerpolicy="no-referrer" />{/if}
      <span class="lplus"><PlusIcon size={18} /></span>
      {#if preview.durationMs}<span class="ldur">{duration(preview.durationMs)}</span>{/if}
    </span>
    <span class="grow">
      <b class="ell">{preview.title}</b>
      <span class="hstack muted">{#if host}<span use:tip={host}><Site /></span>{/if}<span class="ell">{[preview.channel, duration(preview.durationMs)].filter(Boolean).join(" · ")}</span></span>
    </span>
    <button class="ib" class:done={done === "queue"} use:tip={t("song.addToQueue")} onclick={(e) => { e.stopPropagation(); onAdd("queue"); }}>
      {#if done === "queue"}<CheckIcon size={18} />{:else}<ListPlusIcon size={18} />{/if}
    </button>
    <button class="ib" class:done={done === "next"} use:tip={t("song.playNext")} onclick={(e) => { e.stopPropagation(); onAdd("next"); }}>
      {#if done === "next"}<CheckIcon size={18} />{:else}<ArrowBendDownRightIcon size={18} />{/if}
    </button>
  </div>
{/if}

<style>
  .lrow { display: flex; align-items: center; gap: var(--s4); max-width: 760px; padding: var(--s2); border-radius: var(--r-lg); cursor: pointer; transition: background-color var(--t) var(--ease); }
  @media (hover: hover) {
    .lrow:hover { background: color-mix(in srgb, var(--text) 5%, transparent); }
    .lrow:hover .lplus { opacity: 1; }
  }
  .lrow b { display: block; font-size: 16px; font-weight: 600; margin-bottom: 4px; }
  .lthumb { position: relative; width: 160px; aspect-ratio: 16 / 9; flex: none; border-radius: var(--r-sm); display: grid; place-items: center; background: var(--raised); overflow: hidden; }
  .lthumb img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .lthumb .bone { width: 100%; height: 100%; margin: 0; border-radius: 0; }
  .ldur { position: absolute; right: 6px; bottom: 6px; padding: 1px 5px; border-radius: 4px; background: color-mix(in srgb, var(--bezel) 75%, transparent); color: var(--thumb); font: 500 11px var(--mono); }
  .lplus { position: relative; width: 36px; height: 36px; border-radius: 50%; display: grid; place-items: center; background: color-mix(in srgb, var(--bezel) 55%, transparent); color: var(--thumb); opacity: 0; transition: opacity var(--t) var(--ease); }
  .lrow:focus-visible .lplus { opacity: 1; }
</style>
