<script lang="ts">
  import { link } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { composing } from "$lib/keys";
  import { slide } from "$lib/motion";
  import ArrowRightIcon from "phosphor-svelte/lib/ArrowRightIcon";
  import CheckCircleIcon from "phosphor-svelte/lib/CheckCircleIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import QrCodeIcon from "phosphor-svelte/lib/QrCodeIcon";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  const REASONS = { wrongCode: "phone.wrongCode", full: "phone.full", unreachable: "phone.unreachable" } as const;
  const ready = $derived(link.digits.length === 4 && !!link.name.trim());
  const go = () => ready && link.join();
</script>

<div class="logo"><MicrophoneStageIcon size={24} /></div>
<h1 class="title">{t("phone.joinTitle")}</h1>
<p class="lead">{t("phone.joinLead")}</p>
<label class="pfield">
  <span class="cap hstack"><QrCodeIcon size={14} />{t("phone.code")}</span>
  <span class="pinput">
    <span class="prefix">OKI-</span>
    <input class="code-in" bind:value={link.code} maxlength="4" inputmode="numeric" autocomplete="off" />
    {#if link.fromQr}<CheckCircleIcon size={20} />{/if}
  </span>
  {#if link.fromQr}<small class="muted">{t("phone.codeFromQr")}</small>{/if}
</label>
<label class="pfield">
  <span class="cap hstack"><UserIcon size={14} />{t("phone.name")}</span>
  <span class="pinput"><input bind:value={link.name} maxlength="40" autocomplete="nickname" onkeydown={(e) => e.key === "Enter" && !composing(e) && go()} /></span>
</label>
{#if link.refusal}
  <p class="why hstack" in:slide={{ y: -8 }}><WarningIcon size={16} />{t(REASONS[link.refusal])}</p>
{/if}
<div class="grow"></div>
<button class="btn accent pbtn" disabled={!ready} onclick={go}><ArrowRightIcon size={20} />{t("phone.join")}</button>

<style>
  .title { margin-top: var(--s5); }
  .lead { margin-top: var(--s2); }
  .why { margin-top: var(--s4); color: var(--accent); font-size: 14px; font-weight: 500; }
  .prefix { margin-right: -6px; font: 500 20px var(--mono); letter-spacing: .08em; color: var(--muted); }
</style>
