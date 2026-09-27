<script lang="ts">
  import { phones } from "$lib/state/phones.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";

  const n = $derived(phones.view.phones.length);
</script>

<button class="pill glass" use:tip={t("mics.pill")} onclick={() => (ui.sheet = { kind: "mics" })}>
  <MicrophoneStageIcon size={22} />
  {#if n}<span>{n}</span>{:else}<PlusIcon size={14} />{/if}
</button>

<style>
  .pill { grid-column: 3; justify-self: end; display: flex; align-items: center; gap: var(--s2); height: 50px; padding: 0 var(--s5); border-radius: 999px; font: 500 15px var(--mono); }
  .pill:hover { background: var(--glass-hi); }
</style>
