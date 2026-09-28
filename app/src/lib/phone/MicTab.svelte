<script lang="ts">
  import { singerText } from "$lib/components/SingerSlider.svelte";
  import { onMount } from "svelte";
  import { link, type EffectKind } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { loudness } from "$lib/format";
  import { fade, slide, unfold } from "$lib/motion";
  import Artwork from "$lib/components/Artwork.svelte";
  import PhoneLyrics from "./PhoneLyrics.svelte";
  import MagicWandIcon from "phosphor-svelte/lib/MagicWandIcon";
  import MicrophoneSlashIcon from "phosphor-svelte/lib/MicrophoneSlashIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import PauseIcon from "phosphor-svelte/lib/PauseIcon";
  import PlayIcon from "phosphor-svelte/lib/PlayIcon";
  import ProhibitIcon from "phosphor-svelte/lib/ProhibitIcon";
  import SkipBackIcon from "phosphor-svelte/lib/SkipBackIcon";
  import SkipForwardIcon from "phosphor-svelte/lib/SkipForwardIcon";
  import SparkleIcon from "phosphor-svelte/lib/SparkleIcon";
  import SpeakerHighIcon from "phosphor-svelte/lib/SpeakerHighIcon";
  import UserSoundIcon from "phosphor-svelte/lib/UserSoundIcon";
  import WaveformIcon from "phosphor-svelte/lib/WaveformIcon";
  import WifiSlashIcon from "phosphor-svelte/lib/WifiSlashIcon";

  const PRESETS = [
    { kind: "none", label: "phone.fxNone", icon: ProhibitIcon },
    { kind: "karaokeMix", label: "phone.fxKaraoke", icon: WaveformIcon },
    { kind: "autoTune", label: "phone.fxAutotune", icon: SparkleIcon },
  ] as const satisfies readonly { kind: EffectKind; label: string; icon: unknown }[];

  let now = $state(0);
  let open = $state<"voice" | "singer" | "effect" | null>(null);
  let singerDraft = $state<number | null>(null);
  const track = $derived(link.current?.track ?? null);
  const upNext = $derived(link.next ? t("phone.upNext", { song: [link.next.track.title, link.next.track.artist].filter(Boolean).join(" · ") }) : "");
  const singer = $derived(singerDraft ?? track?.vocalRemoval ?? 100);
  const singerLabel = $derived(singerText(singer));

  $effect(() => {
    if (singerDraft === track?.vocalRemoval) singerDraft = null;
  });

  onMount(() => {
    let frame = requestAnimationFrame(function follow() {
      now = link.position();
      frame = requestAnimationFrame(follow);
    });
    return () => cancelAnimationFrame(frame);
  });
</script>

<div class="psong">
  {#if track}
    <Artwork {track} size={48} />
    <div class="grow"><b class="ell">{track.title}</b><span class="muted ell">{track.artist ?? ""}</span></div>
  {:else}
    <span class="th"><MusicNotesIcon size={20} /></span>
    <div class="grow"><b>{t("phone.waitingSong")}</b><span class="muted">{t("phone.addFromSongs")}</span></div>
  {/if}
  <div class="ptr">
    <button aria-label={t("player.previous")} disabled={!track} onclick={() => link.transport("previous")}><SkipBackIcon weight="fill" size={22} /></button>
    <button aria-label={t(link.clock.playing ? "player.pause" : "player.play")} disabled={!track} onclick={() => link.transport("toggle")}>
      {#key link.clock.playing}
        <span class="pp" in:fade>{#if link.clock.playing}<PauseIcon weight="fill" size={26} />{:else}<PlayIcon weight="fill" size={26} />{/if}</span>
      {/key}
    </button>
    <button aria-label={t("player.next")} disabled={!track} onclick={() => link.transport("next")}><SkipForwardIcon weight="fill" size={22} /></button>
  </div>
</div>
<div class="pnext">
  {#key upNext}{#if upNext}<span in:slide={{ y: 4 }}>{upNext}</span><span aria-hidden="true">{upNext}</span>{/if}{/key}
</div>
<div class="plyr">
  {#if track && link.lyrics}<PhoneLyrics lyrics={link.lyrics.lyrics} t={now - (link.snapshot?.lyricOffsetMs ?? 0) / 1000} />{/if}
</div>
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
<div class="pctrls">
  <button class="pctl glass" aria-pressed={open === "voice"} onclick={() => (open = open === "voice" ? null : "voice")}><SpeakerHighIcon size={16} />{t("phone.voice")}</button>
  <button class="pctl glass" aria-pressed={open === "singer"} onclick={() => (open = open === "singer" ? null : "singer")}><UserSoundIcon size={16} />{t("phone.singer")}</button>
  <button class="pctl glass" aria-pressed={open === "effect"} onclick={() => (open = open === "effect" ? null : "effect")}><MagicWandIcon size={16} />{t("phone.effect")}</button>
</div>
{#if open === "voice"}
  <label class="pslide glass" transition:unfold>
    <SpeakerHighIcon size={18} />
    <input class="vs" type="range" min="0" max="100" value={link.voice} style:--v="{link.voice}%" aria-label={t("phone.voiceAria")} oninput={(e) => link.setVoice(+e.currentTarget.value)} />
    <output class="num">{link.voice}%</output>
  </label>
{/if}
{#if open === "singer"}
  <label class="pslide glass" transition:unfold>
    <UserSoundIcon size={18} />
    <input
      class="vs"
      type="range"
      min="0"
      max="100"
      value={singer}
      style:--v="{singer}%"
      aria-label={t("singer.aria")}
      aria-valuetext={singerLabel}
      disabled={!track}
      oninput={(e) => {
        singerDraft = +e.currentTarget.value;
        link.setSinger(singerDraft);
      }}
    />
  </label>
{/if}
{#if open === "effect"}
  <div class="pfx glass" transition:unfold>
    <div class="presets">
      {#each PRESETS as p (p.kind)}
        <button class="preset" aria-pressed={link.effect.kind === p.kind} onclick={() => link.setEffect(p.kind, link.effect.amount)}><p.icon size={16} />{t(p.label)}</button>
      {/each}
    </div>
    <label class="pslide">
      <MagicWandIcon size={18} />
      <input
        class="vs"
        type="range"
        min="0"
        max="100"
        value={link.effect.amount}
        style:--v="{link.effect.amount}%"
        aria-label={t("phone.fxStrength")}
        aria-valuetext={t("phone.fxStrengthValue", { n: link.effect.amount })}
        disabled={link.effect.kind === "none"}
        oninput={(e) => link.setEffect(link.effect.kind, +e.currentTarget.value)}
      />
      <output class="num">{link.effect.amount}%</output>
    </label>
  </div>
{/if}

<style>
  .psong { display: flex; align-items: center; gap: var(--s3); margin-top: var(--s5); }
  .psong b, .psong .muted { display: block; }
  .psong b { font-weight: 600; font-size: 16px; }
  .ptr { display: flex; flex: none; }
  .ptr button { width: 44px; height: 44px; display: grid; place-items: center; border-radius: 50%; }
  .ptr button:active { transform: scale(calc(1 - .08 * var(--motion))); }
  .ptr button:disabled { opacity: .4; }
  .pp { display: grid; }
  .th { width: 48px; height: 48px; flex: none; display: grid; place-items: center; border-radius: var(--r-sm); background: var(--raised); }
  .pnext { height: 18px; margin-top: var(--s3); overflow: hidden; white-space: nowrap; font-size: 13px; color: var(--muted); -webkit-mask-image: linear-gradient(90deg, transparent, #000 6%, #000 94%, transparent); mask-image: linear-gradient(90deg, transparent, #000 6%, #000 94%, transparent); }
  .pnext span { display: inline-block; padding-right: var(--s8); animation: ticker 14s linear infinite; }
  @keyframes ticker { to { transform: translateX(-100%); } }
  @media (prefers-reduced-motion: reduce) { .pnext span { animation: none; } }
  .plyr { min-height: 120px; margin-top: var(--s4); }
  .micbtn { position: relative; flex: none; width: 156px; height: 156px; margin: 0 auto; border-radius: 50%; display: grid; place-items: center; background: var(--accent); color: var(--on-accent); }
  .micbtn > :global(svg) { position: relative; }
  .ring { position: absolute; inset: -14px; border-radius: 50%; border: 6px solid var(--accent); opacity: calc(var(--lv, 0) * .9); transform: scale(calc(1 + var(--lv, 0) * .12 * var(--motion))); transition: opacity 120ms linear, transform 120ms linear; }
  .micbtn:disabled { opacity: .4; }
  .micbtn.muted { background: var(--raised); color: var(--text); }
  .micbtn.muted .ring { opacity: 0; }
  .pstate { display: flex; justify-content: center; align-items: center; gap: var(--s2); margin-top: 36px; font-size: 13px; font-weight: 500; color: var(--muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ready); }
  .pstate.muted .dot { background: var(--faint); }
  .pctrls { display: flex; flex-wrap: wrap; justify-content: center; gap: var(--s3); margin-top: var(--s4); }
  .pctl { display: inline-flex; align-items: center; gap: 6px; height: 36px; padding: 0 var(--s4); border-radius: 999px; font-size: 13px; font-weight: 500; }
  .pctl[aria-pressed="true"] { color: var(--accent); }
  .pslide { display: flex; align-items: center; gap: var(--s3); height: 44px; margin-top: var(--s3); padding: 0 var(--s4); border-radius: 999px; }
  .pslide > :global(svg) { color: var(--muted); }
  .pslide .vs { flex: 1; width: auto; }
  .pslide output { min-width: 4ch; text-align: right; }
  .pfx { margin-top: var(--s3); padding: var(--s2); border-radius: var(--r-lg); }
  .presets { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--s1); }
  .preset { display: grid; justify-items: center; gap: 2px; min-height: 52px; padding: var(--s2) var(--s1); border-radius: var(--r-sm); font-size: 12px; font-weight: 500; color: var(--muted); text-align: center; }
  .preset[aria-pressed="true"] { color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .pfx .pslide { margin-top: var(--s1); }
</style>
