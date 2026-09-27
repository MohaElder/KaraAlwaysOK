<script lang="ts">
  import type { Lyrics } from "$lib/api";
  import { dotProgress, itemAt, timeline, wordProgress } from "$lib/lyrics/timeline";
  import { t as text } from "$lib/i18n/index.svelte";
  import { fade } from "$lib/motion";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import UsersIcon from "phosphor-svelte/lib/UsersIcon";

  let { lyrics, t }: { lyrics: Lyrics; t: number } = $props();

  const PARTS = { m: "phone.partM", f: "phone.partF", both: "phone.partBoth" } as const;
  const items = $derived(timeline(lyrics.lines));
  const i = $derived(itemAt(items, t));
  const now = $derived(items[i]);
  const next = $derived(items.slice(i + 1).find((x) => !x.gap));
</script>

{#if lyrics.source === "none"}
  <p class="now">{text("karaoke.noLyrics")}</p>
{:else if now}
  {#key i}
    <div in:fade>
      {#if now.gap}
        <p class="now dots">{#each [0, 1, 2] as d (d)}<i style:--p={dotProgress(now, d, t)}></i>{/each}</p>
      {:else}
        <div class="v-{now.line.voice ?? 'none'}">
          {#if now.line.voice}
            <span class="part cap">{#if now.line.voice === "both"}<UsersIcon size={14} />{:else}<UserIcon size={14} />{/if}{text(PARTS[now.line.voice])}</span>
          {/if}
          <p class="now">{#each now.line.words as w, j (j)}<span class="w" style:--p={wordProgress(w, t)}>{w.text}</span>{" "}{/each}</p>
        </div>
      {/if}
      {#if next && !next.gap}<p class="next v-{next.line.voice ?? 'none'}">{next.line.text}</p>{/if}
    </div>
  {/key}
{/if}

<style>
  .now { font: 800 26px/1.15 var(--display); letter-spacing: -.02em; text-wrap: balance; }
  .next { margin-top: var(--s2); font-size: 17px; color: var(--muted); }
  .v-m { text-align: left; }
  .v-f { text-align: right; }
  .v-both { text-align: center; }
  .part { display: inline-flex; align-items: center; gap: 4px; margin-bottom: var(--s1); }
  .w { --p: 0; background: linear-gradient(90deg, var(--text) calc(var(--p) * 100%), color-mix(in srgb, var(--text) 40%, transparent) calc(var(--p) * 100%)); -webkit-background-clip: text; background-clip: text; color: transparent; }
  .dots { display: flex; align-items: center; gap: 12px; height: 34px; }
  .dots i { --p: 0; width: 12px; height: 12px; border-radius: 50%; background: var(--text); opacity: calc(.25 + var(--p) * .75); }
</style>
