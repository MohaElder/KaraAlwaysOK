<script lang="ts">
  import { link } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { loudness } from "$lib/format";
  import MicrophoneSlashIcon from "phosphor-svelte/lib/MicrophoneSlashIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import WifiSlashIcon from "phosphor-svelte/lib/WifiSlashIcon";
</script>

<div class="grow"></div>
<button
  class="micbtn"
  class:muted={!link.live}
  disabled={link.reconnecting}
  aria-label={link.live ? t("phone.mute") : t("phone.unmute")}
  style:--lv={link.live && !link.reconnecting ? loudness(link.level) : 0}
  onclick={() => link.toggleLive()}
>
  <span class="ring"></span>
  {#if link.live}<MicrophoneStageIcon size={64} />{:else}<MicrophoneSlashIcon size={64} />{/if}
</button>
<p class="pstate" class:muted={!link.live}>
  {#if link.reconnecting}<WifiSlashIcon size={16} />{t("phone.waitingWifi")}{:else}<span class="dot"></span>{link.live ? t("phone.tapToMute") : t("phone.tapToSing")}{/if}
</p>

<style>
  .micbtn { position: relative; flex: none; width: 156px; height: 156px; margin: 0 auto; border-radius: 50%; display: grid; place-items: center; background: var(--accent); color: var(--on-accent); }
  .micbtn > :global(svg) { position: relative; }
  .ring { position: absolute; inset: -14px; border-radius: 50%; border: 6px solid var(--accent); opacity: calc(var(--lv, 0) * .9); transform: scale(calc(1 + var(--lv, 0) * .12 * var(--motion))); transition: opacity 120ms linear, transform 120ms linear; }
  .micbtn:disabled { opacity: .4; }
  .micbtn.muted { background: var(--raised); color: var(--text); }
  .micbtn.muted .ring { opacity: 0; }
  .pstate { display: flex; justify-content: center; align-items: center; gap: var(--s2); margin-top: 36px; font-size: 13px; font-weight: 500; color: var(--muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ready); }
  .pstate.muted .dot { background: var(--faint); }
</style>
