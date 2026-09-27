import {
  onEngine, onPlayer, playerState, playTracks, queueAdd, queueMove, queueRemove, retryPrepare, setKey, setLyricOffset, setSinger, skipSong, songEnded, trackLyrics,
  type EngineEvent, type Lyrics, type PlayerSnapshot,
} from "$lib/api";
import type { Label } from "$lib/audio/chunks";
import { Streamer, type Phase } from "$lib/audio/streamer";
import { say } from "$lib/i18n/engine";
import { t } from "$lib/i18n/index.svelte";
import { library } from "./library.svelte";
import { toasts } from "./toasts.svelte";
import { ui } from "./ui.svelte";
import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";
import HourglassMediumIcon from "phosphor-svelte/lib/HourglassMediumIcon";
import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

/** A setting changed in the UI: shown at once, saved one request at a time with only the latest value. */
class Draft {
  value = $state<number | null>(null);
  private saving = false;
  private pending: number | null = null;

  constructor(
    private save: (value: number) => Promise<PlayerSnapshot>,
    private refused: (e: unknown) => void,
  ) {}

  set(value: number) {
    this.value = value;
    this.pending = value;
    if (!this.saving) void this.flush();
  }

  /** Drops the unsaved value, including one still waiting to be saved. */
  reset() {
    this.value = null;
    this.pending = null;
  }

  /** Lets the saved value show again once no save is running. */
  settle() {
    if (!this.saving) this.value = null;
  }

  private async flush() {
    this.saving = true;
    while (this.pending != null) {
      const value = this.pending;
      this.pending = null;
      await this.save(value).catch((e) => {
        this.pending = null;
        this.value = null;
        this.refused(e);
      });
    }
    this.saving = false;
  }
}

class PlayerState {
  private drafts = {
    singer: new Draft(setSinger, (e) => this.undo(e)),
    key: new Draft(setKey, (e) => this.undo(e)),
    offset: new Draft(setLyricOffset, (e) => this.undo(e)),
  };
  snapshot = $state<PlayerSnapshot>({ entries: [], current: null, ended: false, lyricOffsetMs: 0 });
  phase = $state<Phase>("idle");
  position = $state(0);
  duration = $state(0);
  ready = $state(0);
  waitLabel = $state<Label>({ key: "wait.soon" });
  lyrics = $state<Lyrics | null>(null);
  keyWorks = $state(true);

  current = $derived(this.snapshot.current == null ? null : (this.snapshot.entries[this.snapshot.current] ?? null));
  track = $derived(this.current?.track ?? null);
  singer = $derived(this.drafts.singer.value ?? this.track?.vocalRemoval ?? 100);
  key = $derived(this.drafts.key.value ?? this.track?.keySemitones ?? 0);
  lyricOffset = $derived(this.drafts.offset.value ?? this.snapshot.lyricOffsetMs);
  idle = $derived(this.snapshot.current == null || this.snapshot.ended);
  active = $derived(this.phase === "playing" || this.phase === "waiting" || this.phase === "stalled");

  private streamer!: Streamer;
  private loadedKey: number | null = null;
  private wantPlay = false;
  private frame = 0;

  /** Starts the streamer and follows the queue and the engine. */
  async init() {
    this.streamer = new Streamer(
      () => this.sync(),
      () => void this.songFinished(),
      (e) => this.showProblem(e),
    );
    await onPlayer((s) => this.apply(s));
    await onEngine((e) => this.onEngine(e));
    await this.refresh();
  }

  /** Lets audio start; call it first thing in a click that will play. */
  unlock() {
    this.streamer.resume();
  }

  async refresh() {
    this.apply(await playerState());
  }

  /** Takes a queue snapshot from the `player` event; loads the song when the current entry changed. */
  apply(s: PlayerSnapshot) {
    this.snapshot = s;
    const cur = s.current == null ? null : s.entries[s.current];
    const same = (cur?.key ?? null) === this.loadedKey;
    for (const d of Object.values(this.drafts)) {
      if (same) d.settle();
      else d.reset();
    }
    if (cur) this.hearSettings();
    if (same) return;
    this.loadedKey = cur?.key ?? null;
    this.lyrics = null;
    if (!cur) return this.streamer.unload();
    void this.streamer.load(cur.track.id, this.wantPlay && !s.ended);
    void this.loadLyrics(cur.track.id);
  }

  /** A row tap: queues the song; with nothing playing it starts now in the karaoke view. */
  async enqueue(trackId: number, next = false) {
    const wasIdle = this.idle;
    if (wasIdle) {
      this.unlock();
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
    this.unlock();
    this.wantPlay = true;
    const order = shuffle ? [...trackIds].sort(() => Math.random() - 0.5) : trackIds;
    if (await playTracks(order, 0).catch(this.refused)) ui.karaoke = true;
  }

  toggle() {
    this.unlock();
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

  async moveQueued(key: number, to: number) {
    await queueMove(key, to).catch(this.refused);
  }

  async removeQueued(key: number) {
    await queueRemove(key).catch(this.refused);
  }

  seek(target: number) {
    if (!this.streamer.seek(target)) toasts.show(t("toast.notReady"), { icon: HourglassMediumIcon });
  }

  setSinger(value: number) {
    this.drafts.singer.set(value);
    this.hearSettings();
  }

  setKey(semitones: number) {
    this.drafts.key.set(semitones);
    this.hearSettings();
  }

  setLyricOffset(ms: number) {
    this.drafts.offset.set(ms);
  }

  private hearSettings() {
    this.streamer.setSinger(this.singer);
    this.streamer.setKey(this.key);
  }

  /** Tells the user a setting wasn't saved and goes back to the saved one. */
  private undo(e: unknown) {
    this.refused(e);
    this.hearSettings();
  }

  private async loadLyrics(trackId: number) {
    const lyrics = await trackLyrics(trackId);
    if (this.track?.id === trackId) this.lyrics = lyrics;
  }

  private onEngine(e: EngineEvent) {
    this.streamer.onEngine(e);
    if (e.kind === "renamed") void Promise.all([library.refresh(), this.refresh()]);
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
    this.position = this.streamer.position();
    if (this.phase === "playing" && !this.frame) this.tick();
    this.duration = this.streamer.songDuration;
    this.ready = this.streamer.ready;
    this.waitLabel = this.streamer.waiting;
    this.keyWorks = this.streamer.keyWorks;
  }

  /** Follows the playing position every animation frame until playback stops. */
  private tick = () => {
    this.position = this.streamer.position();
    this.frame = this.streamer.phase === "playing" ? requestAnimationFrame(this.tick) : 0;
  };

  private async songFinished() {
    await songEnded().catch(this.refused);
  }
}

export const player = new PlayerState();
