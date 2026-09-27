<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import QueueList from "./QueueList.svelte";
  import QueueIcon from "phosphor-svelte/lib/QueueIcon";
  import XIcon from "phosphor-svelte/lib/XIcon";
</script>

{#if ui.queueOpen}
  <aside class="qpanel glass" class:dk={ui.karaoke} aria-label={t("queue.title")} transition:slide={{ x: 24, y: 0 }}>
    <div class="qhead">
      <QueueIcon size={18} /><h2 class="grow">{t("queue.title")}</h2>
      <button class="ib" use:tip={t("common.close")} onclick={() => (ui.queueOpen = false)}><XIcon size={18} /></button>
    </div>
    <div class="qbody">
      <QueueList
        snapshot={player.snapshot}
        nothingBody={t("queue.nothingPlayingBody")}
        emptyBody={t("queue.emptyBody")}
        onMove={(key, to) => void player.moveQueued(key, to)}
        onRemove={(key) => void player.removeQueued(key)}
      />
    </div>
  </aside>
{/if}

<style>
  .qpanel { position: fixed; z-index: 35; top: 92px; right: var(--s4); bottom: 104px; width: min(340px, calc(100% - 32px)); display: flex; flex-direction: column; padding: var(--s4) var(--s3) var(--s3); border-radius: var(--r-lg); }
  .qhead { display: flex; align-items: center; gap: var(--s2); padding: 0 var(--s1) var(--s3) var(--s2); }
  h2 { font: 800 20px/1.2 var(--display); letter-spacing: -.02em; }
  .qbody { flex: 1; min-height: 0; overflow: auto; }
</style>
