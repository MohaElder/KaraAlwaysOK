<script lang="ts">
  import type { Track } from "$lib/api";
  import Artwork from "./Artwork.svelte";

  let { covers, size, grid = false, round = false }: { covers: Track[]; size: number | string; grid?: boolean; round?: boolean } = $props();
  const px = $derived(typeof size === "number" ? `${size}px` : size);
</script>

{#if grid && covers.length >= 4}
  <span class="grid" style:width={px} style:height={px}>{#each covers.slice(0, 4) as t (t.id)}<Artwork track={t} size="100%" square />{/each}</span>
{:else}
  <Artwork track={covers[0] ?? null} {size} {round} />
{/if}

<style>
  .grid { display: grid; grid-template-columns: 1fr 1fr; flex: none; border-radius: 6px; overflow: hidden; }
</style>
