<script lang="ts" module>
  import { t } from "$lib/i18n/index.svelte";

  /** How much of the singer's voice is left, in words. */
  export const singerText = (v: number) => (v === 0 ? t("singer.original") : v === 100 ? t("singer.removed") : t("singer.partly", { n: v }));
</script>

<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import UserSoundIcon from "phosphor-svelte/lib/UserSoundIcon";

  const label = $derived(singerText(player.singer));
</script>

<label class="vox" class:off={player.singer === 0} use:tip={label}>
  <UserSoundIcon size={18} />
  <input
    class="vs"
    type="range"
    min="0"
    max="100"
    value={player.singer}
    style:--v="{player.singer}%"
    aria-label={t("singer.aria")}
    aria-valuetext={label}
    oninput={(e) => player.setSinger(+e.currentTarget.value)}
  />
</label>

<style>
  .vox { display: flex; align-items: center; gap: var(--s2); }
  .vox :global(svg) { color: var(--text); transition: color var(--t) var(--ease); }
  .vox.off :global(svg) { color: var(--faint); }
</style>
