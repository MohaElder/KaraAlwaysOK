<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { dotProgress, itemAt, timeline, wordProgress } from "$lib/lyrics/timeline";
  import { t as text } from "$lib/i18n/index.svelte";
  import { fade } from "$lib/motion";
  import { manage } from "$lib/state/manage.svelte";
  import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";

  const items = $derived(player.lyrics ? timeline(player.lyrics.lines) : []);
  const t = $derived(player.position - player.lyricOffset / 1000);
  const now = $derived(itemAt(items, t));
  const none = $derived(player.lyrics?.source === "none");
  let list: HTMLDivElement | undefined = $state();
  let size = $state(0);
  let placed = $state(false);

  /** Re-centers when the lyrics change size (window resize, a late-loading font). */
  $effect(() => {
    if (!list) return;
    const ro = new ResizeObserver(() => size++);
    ro.observe(list);
    return () => ro.disconnect();
  });

  /** Centers the current line; the first placement jumps there without animating. */
  $effect(() => {
    void size;
    const el = list?.children[now] as HTMLElement | undefined;
    if (!list || !el) return;
    list.style.transform = `translateY(${-(el.offsetTop + el.offsetHeight / 2)}px)`;
    if (!placed) {
      void list.offsetWidth;
      placed = true;
    }
  });
</script>

<div class="lyr">
  {#if none}
    <div class="nolyr" in:fade>
      <h2>{text("karaoke.noLyrics")}</h2>
      {#if player.track}
        {@const track = player.track}
        <button class="btn glass pill" disabled={manage.findingLyrics} onclick={() => manage.findLyrics(track)}><ArrowClockwiseIcon size={16} />{text("menu.findLyrics")}</button>
      {/if}
    </div>
  {:else}
    <div class="track" class:placed bind:this={list}>
      {#each items as x, i (i)}
        {#if x.gap}
          <p class="line gap" class:now={i === now}>
            {#each [0, 1, 2] as d (d)}<i style:--p={i === now ? dotProgress(x, d, t) : 0}></i>{/each}
          </p>
        {:else}
          <p class="line v-{x.line.voice ?? 'none'}" class:now={i === now} class:past={i < now}>
            {#each x.line.words as w, j (j)}<span class="w" style:--p={i === now ? wordProgress(w, t) : 0}>{w.text}</span>{" "}{/each}
          </p>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .lyr { position: relative; overflow: hidden; margin-bottom: 112px; -webkit-mask-image: linear-gradient(transparent, #000 20%, #000 80%, transparent); mask-image: linear-gradient(transparent, #000 20%, #000 80%, transparent); }
  .track { position: absolute; left: 0; right: 0; top: 50%; max-width: 1100px; margin: 0 auto; padding: 0 max(var(--s6), 6vw); text-align: center; }
  .placed { transition: transform var(--t) var(--ease); }
  @media (prefers-reduced-motion: reduce) { .placed { transition: none; } }
  .line { font: 800 clamp(28px, 3.4vw, 40px)/1.15 var(--display); letter-spacing: -.02em; margin: 0 0 .5em; opacity: .3; transition: opacity var(--t) var(--ease); text-wrap: balance; }
  .line.now { opacity: 1; }
  .line.past { opacity: .14; }
  .w { --p: 0; background: linear-gradient(90deg, var(--fill, var(--text)) calc(var(--p) * 100%), color-mix(in srgb, var(--fill, var(--text)) 40%, transparent) calc(var(--p) * 100%)); -webkit-background-clip: text; background-clip: text; color: transparent; }
  .line:not(.now) .w { background: none; color: inherit; }
  .v-m { text-align: left; }
  .v-f { text-align: right; --fill: color-mix(in srgb, var(--accent) 55%, var(--text)); color: var(--fill); }
  .v-both { text-align: center; }
  .gap { display: flex; justify-content: center; align-items: center; gap: 14px; height: .7em; }
  .gap:not(.now) { opacity: 0; }
  .gap i { --p: 0; width: 14px; height: 14px; border-radius: 50%; background: var(--text); opacity: calc(.25 + var(--p) * .75); transform: scale(calc(1 + (var(--p) * .35 - .15) * var(--motion))); }
  .nolyr { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: var(--s4); text-align: center; }
  .pill { border-radius: 999px; }
  .nolyr h2 { font: 800 clamp(28px, 3.4vw, 40px)/1.1 var(--display); letter-spacing: -.02em; }
</style>
