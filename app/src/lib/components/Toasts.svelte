<script lang="ts">
  import { toasts, type Toast } from "$lib/state/toasts.svelte";
  import { slide } from "$lib/motion";
  import Artwork from "./Artwork.svelte";
  import CheckCircleIcon from "phosphor-svelte/lib/CheckCircleIcon";

  function tap(t: Toast) {
    if (!t.onClick) return;
    toasts.close(t.id);
    t.onClick();
  }
</script>

<div class="toasts" role="status">
  {#each toasts.list as t (t.id)}
    <div class="toast glass" class:link={!!t.onClick} transition:slide={{ y: 8 }} {...(t.onClick && { role: "button", tabindex: 0 })} onclick={() => tap(t)} onkeydown={(e) => e.key === "Enter" && tap(t)}>
      {#if t.art}
        <Artwork track={t.art} size={28} />
      {:else if t.spin}
        <span class="spin"></span>
      {:else}
        {@const I = t.icon ?? CheckCircleIcon}
        <I size={16} />
      {/if}
      <span class="ell">{t.text}</span>
      {#if t.action}
        {@const action = t.action}
        <button class="tact" onclick={(e) => { e.stopPropagation(); toasts.close(t.id); action.run(); }}><action.icon size={14} />{action.label}</button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; z-index: 50; left: 0; right: 0; bottom: 108px; display: flex; flex-direction: column; align-items: center; gap: var(--s2); pointer-events: none; }
  .toast { pointer-events: auto; max-width: calc(100% - 32px); display: flex; align-items: center; gap: var(--s2); padding: 10px var(--s4); border-radius: 999px; font-weight: 500; cursor: default; }
  .toast.link { cursor: pointer; }
  .toast :global(.art) { margin-left: -6px; }
  .tact { display: inline-flex; align-items: center; gap: 6px; margin-left: var(--s2); padding: 4px 10px; border-radius: 999px; background: var(--glass-btn); color: var(--accent); font-weight: 600; }
  .tact:hover { background: var(--glass-hi); }
</style>
