<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { clearStorage, setStorageLimit, storageInfo, type StorageInfo } from "$lib/api";
  import { i18n, LOCALE_NAMES, t, type Locale } from "$lib/i18n/index.svelte";
  import { say } from "$lib/i18n/engine";
  import { ui } from "$lib/state/ui.svelte";
  import { toasts } from "$lib/state/toasts.svelte";
  import { adding } from "$lib/state/adding.svelte";
  import { fade } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import Sheet from "./Sheet.svelte";
  import GearIcon from "phosphor-svelte/lib/GearIcon";
  import HardDrivesIcon from "phosphor-svelte/lib/HardDrivesIcon";
  import SlidersHorizontalIcon from "phosphor-svelte/lib/SlidersHorizontalIcon";
  import MinusIcon from "phosphor-svelte/lib/MinusIcon";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";
  import TranslateIcon from "phosphor-svelte/lib/TranslateIcon";

  const GB = 1024 ** 3;
  const LIMITS = [1, 2, 5, 10, 20, 50];
  const shown = $derived(ui.sheet?.kind === "settings");
  let info = $state<StorageInfo | null>(null);
  let version = $state("");
  let asking = $state(false);
  const failed = (e: unknown) => void toasts.show(say(e), { icon: WarningIcon });

  $effect(() => {
    if (!shown) return;
    asking = false;
    void storageInfo().then((i) => (info = i), failed);
    void getVersion().then((v) => (version = v));
  });

  const limitGb = $derived(info ? Math.round(info.limitBytes / GB) : 5);
  const gb = (bytes: number) => (bytes / GB).toFixed(1);

  async function stepLimit(delta: number) {
    const at = LIMITS.findIndex((l) => l >= limitGb);
    const next = LIMITS[Math.max(0, Math.min(LIMITS.length - 1, (at < 0 ? LIMITS.length - 1 : at) + delta))];
    try {
      await setStorageLimit(next * GB);
      info = await storageInfo();
    } catch (e) {
      failed(e);
    }
  }

  async function clear() {
    asking = false;
    try {
      info = await clearStorage();
      toasts.show(t("settings.cleared"));
    } catch (e) {
      failed(e);
    }
  }
</script>

{#if shown}
  <Sheet icon={GearIcon} title={t("settings.title")} onClose={() => (ui.sheet = null)}>
    <section class="set">
      <h3><HardDrivesIcon size={18} />{t("settings.storage")}</h3>
      {#if info}
        <div class="meter"><i style:width="{Math.min(100, (info.usedBytes / Math.max(1, info.limitBytes)) * 100)}%"></i></div>
        <div class="hstack between">
          <span class="num">{t("settings.used", { n: gb(info.usedBytes) })}</span>
          <span class="num">{t("settings.free", { n: gb(Math.max(0, info.limitBytes - info.usedBytes)) })}</span>
        </div>
      {/if}
      <div class="hstack">
        <SlidersHorizontalIcon size={16} />
        <span class="grow">{t("settings.limit")}</span>
        <span class="stepper">
          <button use:tip={t("settings.lowerLimit")} disabled={limitGb <= LIMITS[0]} onclick={() => stepLimit(-1)}><MinusIcon size={13} /></button>
          <output>{t("settings.gb", { n: limitGb })}</output>
          <button use:tip={t("settings.raiseLimit")} disabled={limitGb >= LIMITS[LIMITS.length - 1]} onclick={() => stepLimit(1)}><PlusIcon size={13} /></button>
        </span>
      </div>
      <p class="help">{t("settings.limitHelp")}</p>
      {#if asking}
        <div class="hstack confirm" in:fade>
          <WarningIcon /><span class="grow">{t("settings.clearAsk")}</span>
          <button class="btn" onclick={() => (asking = false)}>{t("common.cancel")}</button>
          <button class="btn accent" disabled={adding.ids.size > 0} onclick={clear}><TrashIcon />{t("settings.clearConfirm")}</button>
        </div>
      {:else}
        <button class="btn start" in:fade disabled={adding.ids.size > 0} onclick={() => (asking = true)}><TrashIcon />{t("settings.clear")}</button>
      {/if}
    </section>
    <section class="set">
      <h3><TranslateIcon size={18} />{t("settings.language")}</h3>
      <select value={i18n.choice ?? ""} aria-label={t("settings.language")} onchange={(e) => i18n.pick((e.currentTarget.value || null) as Locale | null)}>
        <option value="">{t("settings.systemLanguage")}</option>
        {#each Object.entries(LOCALE_NAMES) as [code, name] (code)}<option value={code}>{name}</option>{/each}
      </select>
    </section>
    <section class="about">
      <img src="/icon.png" alt="" width="72" height="72" />
      <div class="grow">
        <b>{t("app.name")}</b>
        <span class="muted num">{t("settings.version", { v: version })}</span>
      </div>
    </section>
  </Sheet>
{/if}

<style>
  .set { padding: var(--s4) 0; border-top: 1px solid var(--line); display: grid; gap: var(--s3); }
  .set:first-of-type { border-top: 0; padding-top: 0; }
  h3 { display: flex; align-items: center; gap: var(--s2); font-size: 15px; font-weight: 600; }
  h3 :global(svg), .set > .hstack > :global(svg) { color: var(--muted); }
  .meter { height: 8px; border-radius: 4px; background: var(--raised); overflow: hidden; }
  .meter i { display: block; height: 100%; border-radius: 4px; background: var(--accent); transition: width var(--t) var(--ease); }
  .between { justify-content: space-between; }
  .help { color: var(--muted); font-size: 12.5px; }
  .confirm { font-size: 13px; }
  .confirm :global(svg) { color: var(--muted); }
  .start { justify-self: start; }
  select { height: 34px; padding: 0 var(--s3); border-radius: var(--r-sm); border: 1px solid var(--line); background: var(--bg); color: var(--text); font: inherit; }
  .about { display: flex; align-items: center; gap: var(--s3); padding-top: var(--s5); border-top: 1px solid var(--line); }
  .about img { margin: -4px; }
  .about div { display: grid; gap: 2px; }
  .about b { font: 800 17px var(--display); letter-spacing: -.02em; }
  .about span { font-size: 13px; }
</style>
