import { chunkPcm, playbackInfo, type EngineEvent, type PlaybackInfo } from "$lib/api";
import { canStart, chunkAt, clampSeek, planStart, RATE, RateMeter, readyEnd, splitChunk, startEta, waitLabel, windowRange, type Label } from "./chunks";
import { KeyShift } from "./keyshift";

export type Phase = "idle" | "waiting" | "playing" | "paused" | "stalled" | "ended";

interface Chunk {
  vocals: AudioBuffer;
  inst: AudioBuffer;
}

const START_AHEAD = 6;
const RESUME_AHEAD = 3;
const LEAD = 0.1;
const FADE = 0.015;

/** Plays one song's separated audio as it gets ready: fetches chunks ahead of the playhead,
 *  lines them up on the audio clock, mixes the singer in and shifts the key. */
export class Streamer {
  phase: Phase = "idle";
  private ctx = new AudioContext({ sampleRate: RATE });
  private vocals = this.ctx.createGain();
  private key = new KeyShift(this.ctx, () => this.onChange());
  private track: number | null = null;
  private info: PlaybackInfo | null = null;
  private duration = Infinity;
  private exact = false;
  private failed = false;
  private chunks = new Map<number, Chunk>();
  private fetching = new Set<number>();
  private sources: AudioBufferSourceNode[] = [];
  /** This run's vocal and instrumental volume, faded out when it stops. */
  private fades: [GainNode, GainNode] | null = null;
  private nextToSchedule = 0;
  private scheduledEnd = 0;
  private anchor = 0;
  private startPos = 0;
  private pos = 0;
  private want = false;
  private rate = new RateMeter();
  private seq = 0;

  constructor(
    private onChange: () => void,
    private onEnded: () => void,
    private onFailed: (error: unknown) => void,
  ) {
    this.vocals.connect(this.key.input);
    setInterval(() => this.tick(), 100);
  }

  private get chunkSec() {
    return (this.info?.chunkFrames ?? RATE * 10) / RATE;
  }

  /** False when the key shifter can't run here; the key control hides. */
  get keyWorks() {
    return this.key.works;
  }

  get songDuration() {
    return Number.isFinite(this.duration) ? this.duration : 0;
  }

  get ready() {
    return this.info ? readyEnd(this.info.chunksDone, this.info.chunksTotal, this.chunkSec, this.duration) : 0;
  }

  get waiting(): Label {
    const eta = startEta(this.pos, START_AHEAD, this.info?.chunksDone ?? 0, this.chunkSec, this.duration, this.rate.secPerChunk());
    return waitLabel(this.phase === "stalled", eta);
  }

  /** The song time the listener hears now, in seconds. */
  position(): number {
    if (this.phase !== "playing") return this.pos;
    return Math.min(this.duration, Math.max(this.startPos, this.ctx.currentTime - this.anchor - this.key.latency));
  }

  /** Lets audio start; call it from a click. */
  resume() {
    void this.ctx.resume();
  }

  /** Switches to a song; with `autoplay` it starts once enough is ready. */
  async load(trackId: number, autoplay: boolean) {
    this.halt();
    const seq = ++this.seq;
    this.track = trackId;
    this.info = null;
    this.duration = Infinity;
    this.exact = false;
    this.failed = false;
    this.pos = 0;
    this.want = autoplay;
    this.rate = new RateMeter();
    this.chunks.clear();
    this.fetching.clear();
    this.phase = autoplay ? "waiting" : "paused";
    this.onChange();
    const info = await playbackInfo(trackId);
    if (seq === this.seq) this.setInfo(info);
  }

  unload() {
    this.halt();
    this.seq++;
    this.track = null;
    this.info = null;
    this.chunks.clear();
    this.pos = 0;
    this.phase = "idle";
    this.onChange();
  }

  /** Follows the engine's progress on the loaded song. */
  onEngine(e: EngineEvent) {
    if (!this.info || e.trackId !== this.track) return;
    if (e.kind === "progress" || e.kind === "ready") this.failed = false;
    if (e.kind === "progress") {
      this.rate.add(performance.now() / 1000, e.chunksDone);
      this.setInfo({ ...this.info, chunksDone: e.chunksDone, chunksTotal: e.chunksTotal });
    } else if (e.kind === "ready") {
      const seq = this.seq;
      void playbackInfo(e.trackId).then((info) => seq === this.seq && this.setInfo(info));
    }
  }

  play() {
    this.want = true;
    if (this.phase === "ended") this.pos = 0;
    if (this.phase === "paused" || this.phase === "ended") this.phase = "waiting";
    this.onChange();
  }

  pause() {
    this.want = false;
    this.halt();
    if (this.phase !== "idle") this.phase = "paused";
    this.onChange();
  }

  /** Moves the playhead; false when the target isn't ready yet (it lands just inside what is). */
  seek(target: number): boolean {
    const { pos, clamped } = clampSeek(target, this.ready, this.duration);
    const moving = this.phase === "playing" || this.phase === "stalled";
    this.halt();
    this.pos = pos;
    if (moving) this.phase = "waiting";
    if (this.phase === "ended") this.phase = "paused";
    this.onChange();
    return !clamped;
  }

  /** 0 keeps the original singer, 100 removes them. */
  setSinger(value: number) {
    this.vocals.gain.setTargetAtTime(1 - value / 100, this.ctx.currentTime, 0.03);
  }

  setKey(semitones: number) {
    void this.key.set(semitones);
  }

  private setInfo(info: PlaybackInfo) {
    this.info = info;
    if (!this.exact) {
      if (info.durationMs) this.duration = info.durationMs / 1000;
      else if (info.chunksTotal != null) this.duration = info.chunksTotal * this.chunkSec;
    }
    this.onChange();
  }

  private tick() {
    if (!this.info || this.track == null) return;
    const pos = this.position();
    this.fetchAround(pos);
    if (this.phase === "playing") {
      this.scheduleReady();
      if (pos >= this.duration - 0.02) return this.finish();
      if (pos >= this.scheduledEnd - 0.05) {
        this.halt();
        this.pos = Math.min(pos, this.scheduledEnd);
        this.phase = "stalled";
        this.onChange();
      }
    } else if (this.want && (this.phase === "waiting" || this.phase === "stalled")) {
      const need = this.phase === "stalled" ? RESUME_AHEAD : START_AHEAD;
      if (canStart(this.pos, this.ready, this.duration, need) && this.chunks.has(chunkAt(this.pos, this.chunkSec))) this.startFrom(this.pos);
      else this.onChange();
    }
  }

  private finish() {
    this.halt();
    this.pos = this.duration;
    this.phase = "ended";
    this.onChange();
    this.onEnded();
  }

  private startFrom(pos: number) {
    this.anchor = this.ctx.currentTime + LEAD - pos;
    this.startPos = pos;
    this.nextToSchedule = chunkAt(pos, this.chunkSec);
    this.scheduledEnd = pos;
    this.phase = "playing";
    this.fades = [this.fadeInto(this.vocals), this.fadeInto(this.key.input)];
    this.scheduleReady();
    this.onChange();
  }

  /** Lines up every fetched chunk that comes next, back to back on the audio clock. */
  private scheduleReady() {
    for (let c = this.chunks.get(this.nextToSchedule); c; c = this.chunks.get(this.nextToSchedule)) {
      const i = this.nextToSchedule++;
      this.scheduledEnd = i * this.chunkSec + c.inst.duration;
      const plan = planStart(i, this.chunkSec, c.inst.duration, this.anchor, this.ctx.currentTime);
      if (!plan) continue;
      for (const [buffer, dest] of [[c.vocals, this.fades![0]], [c.inst, this.fades![1]]] as const) {
        const s = this.ctx.createBufferSource();
        s.buffer = buffer;
        s.connect(dest);
        s.start(plan.when, plan.offset);
        s.onended = () => (this.sources = this.sources.filter((x) => x !== s));
        this.sources.push(s);
      }
    }
  }

  private halt() {
    if (this.phase === "playing") this.pos = this.position();
    const now = this.ctx.currentTime;
    for (const s of this.sources) {
      s.onended = null;
      s.stop(now + FADE);
    }
    this.sources = [];
    for (const fade of this.fades ?? []) {
      fade.gain.setTargetAtTime(0, now, FADE / 5);
      setTimeout(() => fade.disconnect(), FADE * 1000);
    }
    this.fades = null;
  }

  private fadeInto(dest: AudioNode): GainNode {
    const fade = this.ctx.createGain();
    fade.connect(dest);
    return fade;
  }

  /** Keeps the chunks around the playhead decoded and drops the rest. */
  private fetchAround(pos: number) {
    const total = this.info?.chunksTotal;
    if (total == null || this.failed) return;
    const [from, to] = windowRange(pos, this.chunkSec, total);
    for (const i of this.chunks.keys()) if (i < from || i > to) this.chunks.delete(i);
    for (let i = from; i <= Math.min(to, this.info!.chunksDone - 1); i++) {
      if (!this.chunks.has(i) && !this.fetching.has(i)) void this.fetch(i, total);
    }
  }

  private async fetch(index: number, total: number) {
    const seq = this.seq;
    this.fetching.add(index);
    let bytes: ArrayBuffer;
    try {
      bytes = await chunkPcm(this.track!, index);
    } catch (e) {
      if (seq !== this.seq) return;
      this.fetching.delete(index);
      if (this.failed) return;
      this.failed = true;
      this.onFailed(e);
      return;
    }
    if (seq !== this.seq) return;
    this.fetching.delete(index);
    const { vocals, inst } = splitChunk(bytes);
    const chunk = { vocals: this.buffer(vocals), inst: this.buffer(inst) };
    this.chunks.set(index, chunk);
    if (index === total - 1) {
      this.duration = index * this.chunkSec + chunk.inst.duration;
      this.exact = true;
      this.onChange();
    }
  }

  private buffer([left, right]: [Float32Array, Float32Array]): AudioBuffer {
    const b = this.ctx.createBuffer(2, left.length, RATE);
    b.copyToChannel(left, 0);
    b.copyToChannel(right, 1);
    return b;
  }
}
