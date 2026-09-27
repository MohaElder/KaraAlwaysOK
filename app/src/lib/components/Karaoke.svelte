<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import KaraokeBackground from "./KaraokeBackground.svelte";
  import Lyrics from "./Lyrics.svelte";
  import CaretDownIcon from "phosphor-svelte/lib/CaretDownIcon";
  import HourglassMediumIcon from "phosphor-svelte/lib/HourglassMediumIcon";

  const waiting = $derived(player.phase === "waiting" || player.phase === "stalled");
</script>

{#if ui.karaoke && player.track}
  <section class="kara dk" aria-label={t("karaoke.label")} transition:slide={{ y: 24 }}>
    <KaraokeBackground track={player.track} />
    <div class="shade"></div>
    <div class="ktop">
      <div class="hstack who">
        <button class="ib glass round" use:tip={t("player.backToLibrary")} onclick={() => (ui.karaoke = false)}><CaretDownIcon size={18} /></button>
        <div class="grow"><div class="kt ell">{player.track.title}</div><div class="muted ell">{player.track.artist ?? ""}</div></div>
      </div>
      {#if waiting}
        <div class="kwait glass" transition:slide={{ y: -8 }}><HourglassMediumIcon size={16} />{t(player.waitLabel.key, player.waitLabel.params)}</div>
      {:else}
        <div></div>
      {/if}
      <div></div>
    </div>
    <Lyrics />
  </section>
{/if}

<style>
  .kara { position: fixed; inset: 0; z-index: 20; display: grid; grid-template-rows: auto 1fr; overflow: hidden; background: var(--bg); }
  .shade { position: absolute; inset: 0; background: radial-gradient(ellipse at 50% 45%, color-mix(in srgb, var(--bg) 25%, transparent), color-mix(in srgb, var(--bg) 85%, transparent)); }
  .ktop { position: relative; display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; gap: var(--s3); padding: var(--s4) var(--s6); }
  .who { gap: var(--s3); min-width: 0; }
  .round { border-radius: 50%; width: 36px; height: 36px; color: var(--text); }
  .round:hover { background: var(--glass-hi); }
  .kt { font: 800 17px/1.2 var(--display); letter-spacing: -.02em; }
  .kwait { display: flex; align-items: center; gap: var(--s2); height: 36px; padding: 0 var(--s4); border-radius: 999px; font-weight: 600; }
  .kwait :global(svg) { color: var(--busy); }
</style>
