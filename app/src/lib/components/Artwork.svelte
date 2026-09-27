<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Track } from "$lib/api";
  import { artBackground } from "$lib/art";

  let { track, size = 36, round = false, square = false, children }: {
    track: Pick<Track, "artSeed" | "artworkPath"> | null;
    size?: number | string;
    round?: boolean;
    square?: boolean;
    children?: Snippet;
  } = $props();
  const px = $derived(typeof size === "number" ? `${size}px` : size);
</script>

<span class="art" class:round class:square style:width={px} style:height={px} style:background={track ? artBackground(track) : "var(--raised)"}>
  {@render children?.()}
</span>

<style>
  .art { display: grid; place-items: center; flex: none; border-radius: 6px; color: var(--thumb); }
  .art > :global(*) { position: relative; z-index: 1; }
  .round { border-radius: 50%; }
  .square { border-radius: 0; }
</style>
