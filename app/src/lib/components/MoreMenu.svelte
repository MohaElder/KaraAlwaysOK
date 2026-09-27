<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { manage } from "$lib/state/manage.svelte";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import NumberText from "./NumberText.svelte";
  import PianoKeysIcon from "phosphor-svelte/lib/PianoKeysIcon";
  import TimerIcon from "phosphor-svelte/lib/TimerIcon";
  import QuotesIcon from "phosphor-svelte/lib/QuotesIcon";
  import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";
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
      <NumberText text={signed(key)} label={t("menu.key")} onEnter={(n) => player.setKey(Math.max(-6, Math.min(6, Math.round(n))))} />
      <button use:tip={t("menu.higher")} disabled={key >= 6} onclick={() => player.setKey(key + 1)}><PlusIcon size={13} /></button>
    </span>
  </div>
  {/if}
  <div class="mrow">
    <TimerIcon size={18} /><span class="grow">{t("menu.lyricsTiming")}</span>
    <span class="stepper">
      <button use:tip={t("menu.earlier")} onclick={() => player.setLyricOffset(offset - 100)}><MinusIcon size={13} /></button>
      <NumberText text={t("menu.seconds", { n: signed(offset / 1000, 1) })} label={t("menu.lyricsTiming")} onEnter={(n) => player.setLyricOffset(Math.round(n * 10) * 100)} />
      <button use:tip={t("menu.later")} onclick={() => player.setLyricOffset(offset + 100)}><PlusIcon size={13} /></button>
    </span>
  </div>
  <div class="msep"></div>
  <div class="mrow">
    <QuotesIcon size={18} /><span class="grow">{t("menu.lyrics")}</span>
    {#if player.lyrics?.source === "none" && player.track}
      {@const track = player.track}
      <button class="btn soft pill" disabled={manage.findingLyrics} onclick={() => manage.findLyrics(track)}><ArrowClockwiseIcon size={13} />{t("lyricsSource.findAgain")}</button>
    {:else}
      <span class="muted">{source}</span>
    {/if}
  </div>
</div>

<style>
  .menu { position: fixed; }
  .pill { height: 24px; padding: 0 var(--s3); border-radius: 999px; font-size: 12.5px; gap: var(--s1); }
</style>
