<script lang="ts">
  import "$lib/phone/phone.css";
  import { onMount } from "svelte";
  import { link } from "$lib/phone/link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { slide } from "$lib/motion";
  import Toasts from "$lib/components/Toasts.svelte";
  import Center from "$lib/phone/Center.svelte";
  import JoinScreen from "$lib/phone/JoinScreen.svelte";
  import MicTab from "$lib/phone/MicTab.svelte";
  import AndroidLogoIcon from "phosphor-svelte/lib/AndroidLogoIcon";
  import AppleLogoIcon from "phosphor-svelte/lib/AppleLogoIcon";
  import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";
  import HandWavingIcon from "phosphor-svelte/lib/HandWavingIcon";
  import MicrophoneIcon from "phosphor-svelte/lib/MicrophoneIcon";
  import MicrophoneSlashIcon from "phosphor-svelte/lib/MicrophoneSlashIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import ShieldWarningIcon from "phosphor-svelte/lib/ShieldWarningIcon";
  import SignOutIcon from "phosphor-svelte/lib/SignOutIcon";
  import WifiSlashIcon from "phosphor-svelte/lib/WifiSlashIcon";

  onMount(() => link.start());
</script>

<div class="phone">
  {#key link.screen}
    <div class="pscreen" in:slide={{ y: 8 }}>
      {#if link.screen === "join"}
        <JoinScreen />
      {:else if link.screen === "connecting"}
        <Center title={t("phone.connecting")} lead={t("phone.joining", { code: `OKI-${link.digits}` })} />
      {:else if link.screen === "perm"}
        <Center icon={MicrophoneIcon} title={t("phone.permTitle")} lead={t("phone.permLead")}>
          <p class="pnote"><ShieldWarningIcon size={18} /><span>{t("phone.permWarning")}</span></p>
          {#snippet action()}<button class="btn accent pbtn" onclick={() => link.allowMic()}><MicrophoneIcon size={20} />{t("phone.allow")}</button>{/snippet}
        </Center>
      {:else if link.screen === "blocked"}
        <Center icon={MicrophoneSlashIcon} warn title={t("phone.blockedTitle")} lead={t("phone.blockedLead")}>
          <p class="pnote"><AppleLogoIcon size={18} /><span>{t("phone.blockedIphone")}</span></p>
          <p class="pnote"><AndroidLogoIcon size={18} /><span>{t("phone.blockedAndroid")}</span></p>
          {#snippet action()}<button class="btn accent pbtn" onclick={() => link.allowMic()}><ArrowClockwiseIcon size={20} />{t("common.tryAgain")}</button>{/snippet}
        </Center>
      {:else if link.screen === "ended" || link.screen === "lost"}
        {@const lost = link.screen === "lost"}
        <Center icon={lost ? WifiSlashIcon : HandWavingIcon} title={t(lost ? "phone.lostTitle" : "phone.endedTitle")} lead={t("phone.endedLead", { name: link.name })}>
          {#snippet action()}<button class="btn accent pbtn" onclick={() => link.again()}><ArrowClockwiseIcon size={20} />{t("phone.joinAgain")}</button>{/snippet}
        </Center>
      {:else}
        <header class="phead">
          <span class="chip"><MicrophoneStageIcon size={14} /><span class="ell">{link.name}</span></span>
          <span class="grow"></span>
          <button class="btn ghost" onclick={() => link.leave()}><SignOutIcon size={16} />{t("phone.leave")}</button>
        </header>
        <div class="ptab"><MicTab /></div>
        <nav class="ptabs glass">
          <button aria-pressed="true">{#if link.live}<MicrophoneStageIcon size={24} />{:else}<MicrophoneSlashIcon size={24} />{/if}<span>{t("phone.tabMic")}</span></button>
        </nav>
        {#if link.reconnecting}
          <div class="pbanner glass" role="status" transition:slide={{ y: -8 }}><span class="spin"></span><WifiSlashIcon size={18} /><span>{t("phone.reconnecting")}</span></div>
        {/if}
      {/if}
    </div>
  {/key}
  <Toasts />
</div>
