<script lang="ts">
  import Tooltip from "$lib/components/Tooltip.svelte";
</script>

<div class="app">
  <aside class="side"></aside>
  <main class="main">
    <header class="top"></header>
  </main>
</div>
<Tooltip />

<style>
  .app { display: grid; grid-template-columns: var(--side-w) minmax(0, 1fr); height: 100%; }
  .side { background: var(--side); border-right: 1px solid var(--line); }
  .main { position: relative; overflow: auto; min-height: 0; padding: 0 var(--s7) 140px; }
  .top { position: sticky; top: 0; z-index: 5; height: 84px; margin: 0 calc(-1 * var(--s7)); padding: 0 var(--s7); }
</style>
