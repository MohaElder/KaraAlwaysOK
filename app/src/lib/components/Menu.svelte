<script lang="ts">
  import type { Snippet } from "svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { slide } from "$lib/motion";

  let { x, y, alignRight = false, onClose, children }: { x: number; y: number; alignRight?: boolean; onClose: () => void; children: Snippet } = $props();
  let w = $state(0);
  let h = $state(0);
  const left = $derived(Math.max(8, Math.min(innerWidth - w - 8, alignRight ? x - w : x)));
  const top = $derived(y + 4 + h > innerHeight - 8 ? Math.max(8, y - h - (alignRight ? 36 : 4)) : y + 4);
  const outside = (e: Event) => !(e.target as Element).closest?.(".menu");
</script>

<svelte:window onpointerdown={(e) => outside(e) && onClose()} onscrollcapture={(e) => outside(e) && onClose()} />

<div class="menu glass" class:dk={ui.karaoke} role="menu" bind:offsetWidth={w} bind:offsetHeight={h} style:left="{left}px" style:top="{top}px" transition:slide={{ y: 8 }}>
  {@render children()}
</div>

<style>
  .menu { position: fixed; }
</style>
