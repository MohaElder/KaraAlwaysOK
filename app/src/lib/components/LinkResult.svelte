<script lang="ts">
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { duration } from "$lib/format";
  import { fade, fadeAway } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import LinkIcon from "phosphor-svelte/lib/LinkIcon";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
  import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
  import YoutubeLogoIcon from "phosphor-svelte/lib/YoutubeLogoIcon";
  import SoundcloudLogoIcon from "phosphor-svelte/lib/SoundcloudLogoIcon";
  import GlobeIcon from "phosphor-svelte/lib/GlobeIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  type Then = "" | "queue" | "next";
  let { onAdd }: { onAdd?: (url: string, then: Then, title: string) => void } = $props();
  const view = $derived(ui.search.kind === "link" ? ui.search : null);
  const Site = $derived(view?.host.includes("youtu") ? YoutubeLogoIcon : view?.host.includes("soundcloud") ? SoundcloudLogoIcon : GlobeIcon);
</script>

{#if view}
  {#key view.url}
  <div in:fade|global out:fadeAway|global>
  <h2 class="sec"><LinkIcon size={18} />{t("search.fromLink")}</h2>
  {#if view.failed}
    <p class="hstack muted" in:fade><WarningIcon />{t("problem.noSongAtLink")}</p>
  {:else if !view.preview}
    <div class="lrow" in:fade><span class="lthumb"><i class="bone"></i></span><span class="grow"><i class="bone" style="width:55%"></i><i class="bone" style="width:30%"></i></span></div>
  {:else}
    {@const p = view.preview}
    {@const add = (then: Then) => onAdd?.(view.url, then, p.title)}
    <div class="lrow" role="button" tabindex="0" aria-label={t("search.addToLibraryLabel", { title: p.title })} in:fade onclick={() => add("")} onkeydown={(e) => e.key === "Enter" && add("")}>
      <span class="lthumb" style:background-image={p.thumbnail ? `url("${p.thumbnail}")` : null}>
        <span class="lplus"><PlusIcon size={18} /></span>
        {#if p.durationMs}<span class="ldur">{duration(p.durationMs)}</span>{/if}
      </span>
      <span class="grow">
        <b class="ell">{p.title}</b>
        <span class="hstack muted"><span use:tip={view.host}><Site /></span><span class="ell">{[p.channel, duration(p.durationMs)].filter(Boolean).join(" · ")}</span></span>
      </span>
      <button class="ib" use:tip={t("song.addToQueue")} onclick={(e) => { e.stopPropagation(); add("queue"); }}><ListPlusIcon size={18} /></button>
      <button class="ib" use:tip={t("song.playNext")} onclick={(e) => { e.stopPropagation(); add("next"); }}><ArrowBendDownRightIcon size={18} /></button>
    </div>
  {/if}
  </div>
  {/key}
{/if}

<style>
  .sec { display: flex; align-items: center; gap: var(--s2); margin: var(--s6) 0 var(--s2); font-size: 17px; font-weight: 600; }
  .sec :global(svg) { color: var(--muted); }
  .lrow { display: flex; align-items: center; gap: var(--s4); max-width: 760px; padding: var(--s2); border-radius: var(--r-lg); cursor: pointer; transition: background-color var(--t) var(--ease); }
  .lrow:hover { background: color-mix(in srgb, var(--text) 5%, transparent); }
  .lrow b { display: block; font-size: 16px; font-weight: 600; margin-bottom: 4px; }
  .lthumb { position: relative; width: 160px; aspect-ratio: 16 / 9; flex: none; border-radius: var(--r-sm); display: grid; place-items: center; background: var(--raised) center / cover no-repeat; overflow: hidden; }
  .lthumb .bone { width: 100%; height: 100%; margin: 0; border-radius: 0; }
  .ldur { position: absolute; right: 6px; bottom: 6px; padding: 1px 5px; border-radius: 4px; background: color-mix(in srgb, var(--bezel) 75%, transparent); color: var(--thumb); font: 500 11px var(--mono); }
  .lplus { width: 36px; height: 36px; border-radius: 50%; display: grid; place-items: center; background: color-mix(in srgb, var(--bezel) 55%, transparent); color: var(--thumb); opacity: 0; transition: opacity var(--t) var(--ease); }
  .lrow:hover .lplus, .lrow:focus-visible .lplus { opacity: 1; }
</style>
