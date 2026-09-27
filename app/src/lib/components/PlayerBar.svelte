<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import { fade, slide } from "$lib/motion";
  import Artwork from "./Artwork.svelte";
  import SingerSlider from "./SingerSlider.svelte";
  import MoreMenu from "./MoreMenu.svelte";
  import SkipBackIcon from "phosphor-svelte/lib/SkipBackIcon";
  import SkipForwardIcon from "phosphor-svelte/lib/SkipForwardIcon";
  import PlayIcon from "phosphor-svelte/lib/PlayIcon";
  import PauseIcon from "phosphor-svelte/lib/PauseIcon";
  import DotsThreeIcon from "phosphor-svelte/lib/DotsThreeIcon";

  let prog: HTMLDivElement | undefined = $state();
  let bar: HTMLDivElement | undefined = $state();
  let moreButton: HTMLButtonElement | undefined = $state();
  const pct = (seconds: number) => (player.duration > 0 ? Math.min(100, (seconds / player.duration) * 100) : 0);

  function seekAt(clientX: number) {
    if (!prog) return;
    const r = prog.getBoundingClientRect();
    player.seek(((clientX - r.left) / r.width) * player.duration);
  }

  function toggleKaraoke() {
    ui.karaoke = !ui.karaoke;
  }

  $effect(() => {
    if (!player.track) ui.moreOpen = false;
  });
</script>

{#if player.track}
  <div class="bar glass" bind:this={bar} class:dk={ui.karaoke} class:wide={ui.karaoke} role="region" aria-label={t("player.label")} transition:slide={{ y: 16 }}>
    <div class="transport">
      <button use:tip={t("player.previous")} onclick={() => player.previous()}><SkipBackIcon weight="fill" /></button>
      <button use:tip={t(player.active ? "player.pause" : "player.play")} onclick={() => player.toggle()}>
        {#key player.active}
          <span class="pp" in:fade>{#if player.active}<PauseIcon weight="fill" size={19} />{:else}<PlayIcon weight="fill" size={19} />{/if}</span>
        {/key}
      </button>
      <button use:tip={t("player.next")} onclick={() => player.next()}><SkipForwardIcon weight="fill" /></button>
    </div>
    <div class="np" role="button" tabindex="0" use:tip={t(ui.karaoke ? "player.backToLibrary" : "player.openKaraoke")} onclick={toggleKaraoke} onkeydown={(e) => {
      if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
      e.preventDefault();
      toggleKaraoke();
    }}>
      <Artwork track={player.track} size={40} />
      <div class="t ell">{player.track.title}</div>
      <div class="a ell">{[player.track.artist, player.track.album].filter(Boolean).join(" – ")}</div>
      <div
        class="prog"
        bind:this={prog}
        role="slider"
        tabindex="0"
        aria-label={t("player.position")}
        aria-valuemin={0}
        aria-valuemax={Math.round(player.duration)}
        aria-valuenow={Math.round(player.position)}
        onclick={(e) => {
          e.stopPropagation();
          seekAt(e.clientX);
        }}
        onkeydown={(e) => {
          if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
          e.stopPropagation();
          player.seek(player.position + (e.key === "ArrowRight" ? 5 : -5));
        }}
      >
        <i class="buf" style:width="{pct(player.ready)}%"></i>
        <i class="played" style:width="{pct(player.position)}%"></i>
      </div>
    </div>
    <SingerSlider />
    <button class="ib more" bind:this={moreButton} use:tip={t("common.more")} aria-expanded={ui.moreOpen} onclick={() => (ui.moreOpen = !ui.moreOpen)}><DotsThreeIcon size={15} /></button>
  </div>
  {#if ui.moreOpen && bar && moreButton}<MoreMenu {bar} anchor={moreButton} />{/if}
{/if}

<style>
  .bar { position: fixed; z-index: 30; bottom: var(--s5); left: calc(var(--side-w) + var(--s7)); right: var(--s7); max-width: 900px; margin: 0 auto; display: flex; align-items: center; gap: var(--s4); padding: var(--s2) var(--s3) var(--s2) var(--s5); border-radius: 999px; transition: left var(--t) var(--ease), background-color var(--t) var(--ease), color var(--t) var(--ease); }
  .bar.wide { left: var(--s7); }
  .transport { display: flex; align-items: center; gap: 2px; }
  .transport button { width: 36px; height: 36px; display: grid; place-items: center; border-radius: var(--r-sm); }
  .transport button:hover { opacity: .7; }
  .transport button:active { transform: scale(calc(1 - .08 * var(--motion))); }
  .pp { display: grid; }
  .np { flex: 1; min-width: 0; display: grid; grid-template-columns: 40px minmax(0, 1fr); column-gap: var(--s3); align-items: center; cursor: pointer; padding: 2px; border-radius: var(--r-sm); }
  .np > :global(.art) { grid-row: span 2; }
  .t { font-weight: 600; }
  .a { font-size: 12.5px; color: var(--muted); }
  .prog { grid-column: 1 / -1; position: relative; height: 3px; margin-top: var(--s2); border-radius: 2px; background: color-mix(in srgb, var(--text) 12%, transparent); cursor: pointer; }
  .prog::before { content: ""; position: absolute; inset: -6px 0; }
  .prog i { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 2px; }
  .buf { background: color-mix(in srgb, var(--text) 26%, transparent); transition: width var(--t) var(--ease); }
  .played { background: var(--text); }
  .bar :global(.ib) { border-radius: 50%; background: var(--glass-btn); box-shadow: inset 0 0 0 1px var(--glass-edge); }
  .bar :global(.ib:hover) { background: var(--glass-hi); }
</style>
