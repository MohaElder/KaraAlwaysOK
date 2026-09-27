<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Icon } from "$lib/icons";
  import { fade } from "$lib/motion";

  let { icon: IconC, title, children }: { icon: Icon; title: string; children?: Snippet } = $props();
</script>

<div class="empty" in:fade>
  <div class="badge"><IconC size={26} /></div>
  <h2>{title}</h2>
  {@render children?.()}
</div>

<style>
  .empty { display: grid; justify-items: center; text-align: center; gap: var(--s3); padding: 96px var(--s4) 0; color: var(--muted); max-width: 420px; margin: 0 auto; }
  h2 { font: 800 22px/1.2 var(--display); letter-spacing: -.02em; color: var(--text); }
  .badge { width: 56px; height: 56px; border-radius: var(--r-lg); display: grid; place-items: center; background: var(--raised); color: var(--text); }
</style>
