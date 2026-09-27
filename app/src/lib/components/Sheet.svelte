<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Icon } from "$lib/icons";
  import { t } from "$lib/i18n/index.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { fade, slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import XIcon from "phosphor-svelte/lib/XIcon";

  let { icon: IconC, title, subtitle, subtitleIcon: SubIcon, wide = false, onClose, children }: {
    icon: Icon;
    title: string;
    subtitle?: string;
    subtitleIcon?: Icon;
    wide?: boolean;
    onClose: () => void;
    children: Snippet;
  } = $props();
</script>

<div class="scrim" class:dk={ui.karaoke} role="presentation" transition:fade onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="sheet" class:wide role="dialog" aria-label={title} transition:slide|global={{ y: 8 }}>
    <div class="shead">
      <div class="badge"><IconC size={20} /></div>
      <div class="grow"><h2>{title}</h2>{#if subtitle}<p class="muted hstack">{#if SubIcon}<SubIcon size={14} />{/if}{subtitle}</p>{/if}</div>
      <button class="ib" use:tip={t("common.close")} onclick={onClose}><XIcon size={18} /></button>
    </div>
    {@render children()}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 40; display: grid; place-items: center; padding: var(--s4); background: var(--scrim); -webkit-backdrop-filter: blur(3px); backdrop-filter: blur(3px); }
  .sheet { width: min(460px, 100%); max-height: calc(100vh - 32px); overflow: auto; padding: var(--s6); border-radius: var(--r-lg); background: var(--surface); border: 1px solid var(--line); box-shadow: 0 30px 80px -30px var(--shadow); }
  .sheet.wide { width: min(720px, 100%); }
  .shead { display: flex; align-items: center; gap: var(--s3); margin-bottom: var(--s5); }
  .shead p { margin-top: 2px; }
  h2 { font: 800 22px/1.2 var(--display); letter-spacing: -.02em; }
  .badge { width: 40px; height: 40px; border-radius: var(--r-sm); display: grid; place-items: center; background: var(--raised); flex: none; }
</style>
