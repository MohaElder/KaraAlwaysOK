<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import PianoKeysIcon from "phosphor-svelte/lib/PianoKeysIcon";
  import TimerIcon from "phosphor-svelte/lib/TimerIcon";
  import QuotesIcon from "phosphor-svelte/lib/QuotesIcon";
  import MinusIcon from "phosphor-svelte/lib/MinusIcon";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";

  const SOURCES: Record<"lrclib" | "embedded" | "none", Key> = { lrclib: "lyricsSource.lrclib", embedded: "lyricsSource.embedded", none: "lyricsSource.none" };
  const signed = (n: number, digits = 0) => (n > 0 ? "+" : n < 0 ? "−" : "") + Math.abs(n).toFixed(digits);
  let { bar, anchor }: { bar: HTMLElement; anchor: HTMLElement } = $props();
  const key = $derived(player.key);
  const offset = $derived(player.lyricOffset);
  let pos = $state(place());

  /** Sits above the bar, right-aligned with the button that opened it. */
  function place() {
    const b = anchor.getBoundingClientRect();
    return { right: Math.max(16, innerWidth - b.right), bottom: innerHeight - bar.getBoundingClientRect().top + 10 };
  }
  const source = $derived(t(player.lyrics?.source ? SOURCES[player.lyrics.source] : "lyricsSource.looking"));
</script>

<svelte:window onresize={() => (pos = place())} onpointerdown={(e) => { if (!(e.target as Element).closest(".more, .menu")) ui.moreOpen = false; }} />

<div class="menu glass" class:dk={ui.karaoke} role="menu" style:right="{pos.right}px" style:bottom="{pos.bottom}px" transition:slide={{ y: 8 }}>
  {#if player.keyWorks}
  <div class="mrow">
    <PianoKeysIcon size={18} /><span class="grow">{t("menu.key")}</span>
    <span class="stepper">
      <button use:tip={t("menu.lower")} disabled={key <= -6} onclick={() => player.setKey(key - 1)}><MinusIcon size={13} /></button>
      <output>{signed(key)}</output>
      <button use:tip={t("menu.higher")} disabled={key >= 6} onclick={() => player.setKey(key + 1)}><PlusIcon size={13} /></button>
    </span>
  </div>
  {/if}
  <div class="mrow">
    <TimerIcon size={18} /><span class="grow">{t("menu.lyricsTiming")}</span>
    <span class="stepper">
      <button use:tip={t("menu.earlier")} disabled={offset <= -5000} onclick={() => player.setLyricOffset(offset - 100)}><MinusIcon size={13} /></button>
      <output>{t("menu.seconds", { n: signed(offset / 1000, 1) })}</output>
      <button use:tip={t("menu.later")} disabled={offset >= 5000} onclick={() => player.setLyricOffset(offset + 100)}><PlusIcon size={13} /></button>
    </span>
  </div>
  <div class="msep"></div>
  <div class="mrow"><QuotesIcon size={18} /><span class="grow">{t("menu.lyrics")}</span><span class="muted">{source}</span></div>
</div>

<style>
  .menu { position: fixed; }
</style>
