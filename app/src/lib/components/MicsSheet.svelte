<script lang="ts">
  import { onMount } from "svelte";
  import { phones } from "$lib/state/phones.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { loudness } from "$lib/format";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import Sheet from "./Sheet.svelte";
  import AndroidLogoIcon from "phosphor-svelte/lib/AndroidLogoIcon";
  import AppleLogoIcon from "phosphor-svelte/lib/AppleLogoIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import DeviceMobileIcon from "phosphor-svelte/lib/DeviceMobileIcon";
  import GlobeIcon from "phosphor-svelte/lib/GlobeIcon";
  import KeyboardIcon from "phosphor-svelte/lib/KeyboardIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import ShieldCheckIcon from "phosphor-svelte/lib/ShieldCheckIcon";
  import ShieldWarningIcon from "phosphor-svelte/lib/ShieldWarningIcon";
  import SpeakerHighIcon from "phosphor-svelte/lib/SpeakerHighIcon";
  import SpeakerSlashIcon from "phosphor-svelte/lib/SpeakerSlashIcon";
  import UserMinusIcon from "phosphor-svelte/lib/UserMinusIcon";
  import WifiHighIcon from "phosphor-svelte/lib/WifiHighIcon";

  const BARS = [5, 8, 11, 14, 16];
  const MOST_PHONES = 4;
  const join = $derived(phones.view.join);
  const close = () => (ui.sheet = null);
  let drafts = $state<Record<string, number>>({});

  onMount(() => {
    void phones.open();
    return () => void phones.close();
  });
</script>

{#snippet bold(key: Key, param: string, value: string)}
  {@const [before, after] = t(key, { [param]: "\u0000" }).split("\u0000")}
  <span>{before}<b>{value}</b>{after}</span>
{/snippet}

<Sheet icon={MicrophoneStageIcon} title={t("mics.title")} subtitle={t("mics.scan")} subtitleIcon={WifiHighIcon} wide onClose={close}>
  <div class="join">
    <div>
      <div class="qr">{#if join}{@html join.qr}{/if}</div>
      <p class="cap hstack typed"><KeyboardIcon size={14} />{t("mics.typeCode")}</p>
      <div class="code">{join?.code ?? ""}</div>
      <p class="curl hstack"><GlobeIcon size={14} />{@render bold("mics.on", "host", join?.host ?? "")}</p>
    </div>
    <div>
      <div class="tut">
        <b class="hstack"><ShieldWarningIcon size={16} />{t("mics.warnTitle")}</b>
        <p class="muted">{t("mics.warnSafe")}</p>
        <p class="hstack"><AppleLogoIcon size={16} />{@render bold("mics.iphone", "os", "iPhone")}</p>
        <p class="hstack"><AndroidLogoIcon size={16} />{@render bold("mics.android", "os", "Android")}</p>
        <p class="hstack"><ShieldCheckIcon size={16} />{t("mics.firewall")}</p>
        <p class="hstack"><SpeakerSlashIcon size={16} />{t("mics.apart")}</p>
      </div>
      <div class="mlist">
        <p class="cap hstack"><DeviceMobileIcon size={14} />{t("mics.phones", { n: phones.view.phones.length })}</p>
        {#each phones.view.phones as p, i (p.id)}
          {@const level = phones.levels[p.id]}
          {@const volume = drafts[p.id] ?? p.volume}
          <div class="mic" class:away={!p.connected} transition:slide={{ y: 8 }}>
            <DeviceMobileIcon size={18} />
            <div class="grow">
              <b class="ell">{p.name}</b>
              <small class="cap" class:hot={level?.down}>{!p.connected ? t("mics.away") : level?.down ? t("mics.turnedDown") : t("mics.micN", { n: i + 1 })}</small>
            </div>
            <span class="meter" aria-hidden="true">
              {#each BARS as h, b (b)}<i style:height="{h}px" class:lit={b < Math.round(loudness(level?.level ?? 0) * 5)}></i>{/each}
            </span>
            <label class="vol" use:tip={t("mics.volume")}>
              <SpeakerHighIcon size={16} />
              <input
                class="vs"
                type="range"
                min="0"
                max="100"
                value={volume}
                style:--v="{volume}%"
                aria-label={t("mics.volumeFor", { name: p.name })}
                oninput={(e) => {
                  drafts[p.id] = +e.currentTarget.value;
                  phones.setVolume(p.id, drafts[p.id]);
                }}
                onchange={() => delete drafts[p.id]}
              />
            </label>
            <button class="ib" use:tip={t("mics.remove", { name: p.name })} onclick={() => phones.remove(p.id, p.name)}><UserMinusIcon size={18} /></button>
          </div>
        {/each}
        {#if phones.view.phones.length < MOST_PHONES}
          <div class="mic wait"><WifiHighIcon size={18} />{phones.view.phones.length ? t("mics.waitingMore") : t("mics.waiting")}</div>
        {/if}
      </div>
    </div>
  </div>
  <div class="sfoot"><button class="btn accent" onclick={close}><CheckIcon />{t("common.done")}</button></div>
</Sheet>

<style>
  .join { display: grid; grid-template-columns: 200px minmax(0, 1fr); gap: var(--s6); }
  .qr { width: 200px; height: 200px; padding: var(--s3); border-radius: var(--r-lg); color: var(--qr-dark); background: var(--qr-light); border: 1px solid var(--line); }
  .qr :global(svg) { display: block; width: 100%; height: 100%; }
  .typed { margin-top: var(--s4); }
  .code { font: 500 26px/1 var(--mono); letter-spacing: .08em; margin-top: var(--s2); }
  .curl { margin-top: var(--s2); color: var(--muted); font-size: 13px; }
  .tut { display: grid; gap: var(--s2); padding: var(--s3) var(--s4); border-radius: var(--r-sm); background: var(--raised); font-size: 13px; }
  .curl, .tut .hstack { align-items: flex-start; }
  .curl :global(svg), .tut :global(svg) { flex: none; margin-top: 2px; }
  .mlist { margin-top: var(--s4); }
  .mlist .cap { margin-bottom: var(--s2); }
  .mic { display: flex; align-items: center; gap: var(--s3); min-height: var(--row); border-top: 1px solid var(--line); transition: opacity var(--t) var(--ease); }
  .mic > :global(svg) { color: var(--muted); }
  .mic small { display: block; }
  .mic.away { opacity: .5; }
  .hot { color: var(--busy); }
  .mic.wait { color: var(--muted); font-size: 13px; }
  .mic.wait > :global(svg) { animation: pulse 1.6s var(--ease) infinite; }
  .meter { display: flex; align-items: flex-end; gap: 2px; height: 16px; }
  .meter i { width: 3px; border-radius: 1px; background: var(--line); transition: background-color 120ms linear; }
  .meter i.lit { background: var(--ready); }
  .vol { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  .vol .vs { width: 80px; }
  .sfoot { display: flex; justify-content: flex-end; margin-top: var(--s5); }
</style>
