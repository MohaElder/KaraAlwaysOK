import {
  onEngine, onPlayer, playerState, playTracks, queueAdd, retryPrepare, setKey, setLyricOffset, setSinger, skipSong, songEnded, trackLyrics,
  type EngineEvent, type Lyrics, type PlayerSnapshot,
} from "$lib/api";
import type { Label } from "$lib/audio/chunks";
import { Streamer, type Phase } from "$lib/audio/streamer";
import { say } from "$lib/i18n/engine";
import { t } from "$lib/i18n/index.svelte";
import { toasts } from "./toasts.svelte";
import { ui } from "./ui.svelte";
import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";
import HourglassMediumIcon from "phosphor-svelte/lib/HourglassMediumIcon";
import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

class PlayerState {
  snapshot = $state<PlayerSnapshot>({ entries: [], current: null, ended: false, lyricOffsetMs: 0 });
  phase = $state<Phase>("idle");
  position = $state(0);
  duration = $state(0);
  ready = $state(0);
  waitLabel = $state<Label>({ key: "wait.soon" });
  lyrics = $state<Lyrics | null>(null);
  keyWorks = $state(true);
  private singerDraft = $state<number | null>(null);

  current = $derived(this.snapshot.current == null ? null : (this.snapshot.entries[this.snapshot.current] ?? null));
  track = $derived(this.current?.track ?? null);
  singer = $derived(this.singerDraft ?? this.track?.vocalRemoval ?? 100);
  idle = $derived(this.snapshot.current == null || this.snapshot.ended);
  active = $derived(this.phase === "playing" || this.phase === "waiting" || this.phase === "stalled");

  private streamer!: Streamer;
  private loadedKey: number | null = null;
  private wantPlay = false;
  private savingSinger = false;
  private pendingSinger: number | null = null;

  /** Starts the streamer and follows the queue and the engine. */
  async init() {
    this.streamer = new Streamer(
      () => this.sync(),
      () => void this.songFinished(),
      (e) => this.showProblem(e),
    );
    await onPlayer((s) => this.apply(s));
    await onEngine((e) => this.onEngine(e));
    this.apply(await playerState());
    const frame = () => {
      this.position = this.streamer.position();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  }

  /** Takes a queue snapshot from the `player` event; loads the song when the current entry changed. */
  apply(s: PlayerSnapshot) {
    this.snapshot = s;
    const cur = s.current == null ? null : s.entries[s.current];
    if ((cur?.key ?? null) === this.loadedKey) {
      if (cur && !this.savingSinger) {
        this.singerDraft = null;
        this.streamer.setSinger(cur.track.vocalRemoval);
      }
      if (cur) this.streamer.setKey(cur.track.keySemitones);
      return;
    }
    this.loadedKey = cur?.key ?? null;
    this.singerDraft = null;
    this.lyrics = null;
    if (!cur) return this.streamer.unload();
    this.streamer.setSinger(cur.track.vocalRemoval);
    this.streamer.setKey(cur.track.keySemitones);
    void this.streamer.load(cur.track.id, this.wantPlay && !s.ended);
    void this.loadLyrics(cur.track.id);
  }

  /** A row tap: queues the song; with nothing playing it starts now in the karaoke view. */
  async enqueue(trackId: number, next = false) {
    const wasIdle = this.idle;
    if (wasIdle) {
      this.streamer.resume();
      this.wantPlay = true;
    }
    const s = await queueAdd(trackId, next).catch(this.refused);
    if (!s) return;
    if (wasIdle) ui.karaoke = true;
    else {
      const title = s.entries.find((e) => e.track.id === trackId)?.track.title ?? "";
      toasts.show(next ? t("toast.playsNext", { title }) : t("toast.addedToQueue"), { icon: next ? ArrowBendDownRightIcon : ListPlusIcon });
    }
  }

  /** Sing or Shuffle: replaces the queue and opens the karaoke view. */
  async playAll(trackIds: number[], shuffle: boolean) {
    this.streamer.resume();
    this.wantPlay = true;
    const order = shuffle ? [...trackIds].sort(() => Math.random() - 0.5) : trackIds;
    if (await playTracks(order, 0).catch(this.refused)) ui.karaoke = true;
  }

  toggle() {
    this.streamer.resume();
    this.wantPlay = !this.active;
    if (this.wantPlay) this.streamer.play();
    else this.streamer.pause();
  }

  async previous() {
    if (this.streamer.position() > 3) this.streamer.seek(0);
    else await skipSong(-1).catch(this.refused);
  }

  async next() {
    await skipSong(1).catch(this.refused);
  }

  seek(target: number) {
    if (!this.streamer.seek(target)) toasts.show(t("toast.notReady"), { icon: HourglassMediumIcon });
  }

  /** The singer slider: heard at once, saved without flooding the backend. */
  setSinger(value: number) {
    this.singerDraft = value;
    this.streamer.setSinger(value);
    this.pendingSinger = value;
    if (!this.savingSinger) void this.saveSinger();
  }

  async setKey(semitones: number) {
    await setKey(semitones).catch(this.refused);
  }

  async setLyricOffset(ms: number) {
    await setLyricOffset(ms).catch(this.refused);
  }

  /** Saves the slider's latest value; a refused save puts the slider back to the saved one. */
  private async saveSinger() {
    this.savingSinger = true;
    while (this.pendingSinger != null) {
      const value = this.pendingSinger;
      this.pendingSinger = null;
      if (!(await setSinger(value).catch(this.refused))) {
        this.pendingSinger = null;
        this.singerDraft = null;
        this.streamer.setSinger(this.singer);
      }
    }
    this.savingSinger = false;
  }

  private async loadLyrics(trackId: number) {
    const lyrics = await trackLyrics(trackId);
    if (this.track?.id === trackId) this.lyrics = lyrics;
  }

  private onEngine(e: EngineEvent) {
    this.streamer.onEngine(e);
    if (e.trackId !== this.track?.id) return;
    if (e.kind === "lyrics") void this.loadLyrics(e.trackId);
    if (e.kind === "failed") this.showProblem(e);
  }

  /** Tells the user the song can't play right now, with a way to prepare it again. */
  private showProblem(e: unknown) {
    toasts.show(say(e), { icon: WarningIcon, action: { label: t("common.tryAgain"), icon: ArrowClockwiseIcon, run: () => void retryPrepare() } });
  }

  /** Tells the user why the queue or a setting didn't change. */
  private refused = (e: unknown) => {
    toasts.show(say(e), { icon: WarningIcon });
    if (this.idle) this.wantPlay = false;
    return null;
  };

  private sync() {
    this.phase = this.streamer.phase;
    this.duration = this.streamer.songDuration;
    this.ready = this.streamer.ready;
    this.waitLabel = this.streamer.waiting;
    this.keyWorks = this.streamer.keyWorks;
  }

  private async songFinished() {
    await songEnded().catch(this.refused);
  }
}

export const player = new PlayerState();
