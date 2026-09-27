<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { Track } from "$lib/api";
  import { hues } from "$lib/art";

  let { track }: { track: Track } = $props();
  const h = $derived(hues(track.artSeed));
  const image = $derived(track.artworkPath ? `url("${convertFileSrc(track.artworkPath)}")` : null);
</script>

<div class="kbg" style:--h0={h[0]} style:--h1={h[1]} style:--h2={h[2]}>
  {#each [0, 1, 2] as i (i)}<i class:img={!!image} style:background-image={image}></i>{/each}
</div>

<style>
  .kbg { position: absolute; inset: 0; overflow: hidden; background: hsl(var(--h1) 40% 12%); }
  i { position: absolute; width: 80vmax; height: 80vmax; border-radius: 50%; filter: blur(90px); opacity: .8; background-size: 300% 300%; animation: drift 28s ease-in-out infinite alternate; }
  i:nth-child(1) { background-color: hsl(var(--h0) 75% 45%); background-position: 0 0; left: -25vmax; top: -30vmax; }
  i:nth-child(2) { background-color: hsl(var(--h1) 70% 38%); background-position: 100% 0; right: -30vmax; top: -10vmax; animation-duration: 34s; animation-direction: alternate-reverse; }
  i:nth-child(3) { background-color: hsl(var(--h2) 80% 45%); background-position: 50% 100%; left: 5vmax; bottom: -45vmax; animation-duration: 40s; }
  .img { filter: blur(90px) saturate(1.6); }
  @keyframes drift { 50% { transform: translate(14vmax, 10vmax) scale(1.15); } 100% { transform: translate(-10vmax, 16vmax) scale(.9); } }
  @media (prefers-reduced-motion: reduce) { i { animation: none; } }
</style>
