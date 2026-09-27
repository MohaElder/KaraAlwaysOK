<script lang="ts">
  import { tipState } from "$lib/tooltip.svelte";
  import { fade } from "$lib/motion";

  let w = $state(0);
  let h = $state(0);
  const pos = $derived.by(() => {
    const r = tipState.current?.rect;
    if (!r) return null;
    const below = r.bottom + 6;
    const top = below + h > innerHeight - 4 ? r.top - h - 6 : below;
    return { top, left: Math.max(4, Math.min(innerWidth - w - 4, r.left + r.width / 2 - w / 2)) };
  });
</script>

{#if tipState.current && pos}
  <div class="tip glass" role="tooltip" style:top="{pos.top}px" style:left="{pos.left}px" bind:offsetWidth={w} bind:offsetHeight={h} transition:fade>
    {tipState.current.text}
  </div>
{/if}

<style>
  .tip { position: fixed; z-index: 70; padding: 4px var(--s2); border-radius: 6px; font-size: 12px; font-weight: 500; white-space: nowrap; pointer-events: none; }
</style>
