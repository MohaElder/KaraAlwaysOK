import type { Key } from "$lib/i18n/index.svelte";

export const RATE = 44_100;

/** UI text to show, as a key and its placeholder values. */
export interface Label {
  key: Key;
  params?: Record<string, number>;
}

/** Index of the chunk holding song time `pos`. */
export function chunkAt(pos: number, chunkSec: number): number {
  return Math.max(0, Math.floor(pos / chunkSec));
}

/** Chunks kept decoded around `pos`: the previous one and the next three. */
export function windowRange(pos: number, chunkSec: number, total: number): [number, number] {
  const c = chunkAt(pos, chunkSec);
  return [Math.max(0, c - 1), Math.min(total - 1, c + 3)];
}

/** When and from where to start chunk `index` so it lines up with the song clock; null once it has passed. */
export function planStart(index: number, chunkSec: number, length: number, anchor: number, now: number): { when: number; offset: number } | null {
  const start = anchor + index * chunkSec;
  if (start >= now) return { when: start, offset: 0 };
  const offset = now - start;
  return offset < length ? { when: now, offset } : null;
}

/** Seconds of the song that are ready to play. */
export function readyEnd(done: number, total: number | null, chunkSec: number, duration: number): number {
  return total != null && done >= total ? duration : Math.min(done * chunkSec, duration);
}

/** Whether enough is ready past `pos` to play on: `need` seconds, or the rest of the song. */
export function canStart(pos: number, ready: number, duration: number, need: number): boolean {
  return ready >= duration || ready - pos >= need;
}

/** A seek target kept inside what's ready. */
export function clampSeek(target: number, ready: number, duration: number): { pos: number; clamped: boolean } {
  const limit = ready >= duration ? duration : Math.max(0, ready - 1);
  return target > limit ? { pos: limit, clamped: true } : { pos: Math.max(0, target), clamped: false };
}

/** Seconds until playback can start, from how fast chunks have been arriving; null until that's known. */
export function startEta(pos: number, need: number, done: number, chunkSec: number, duration: number, secPerChunk: number | null): number | null {
  if (secPerChunk == null) return null;
  const needed = Math.ceil(Math.min(pos + need, duration) / chunkSec) - done;
  return Math.max(0, needed) * secPerChunk;
}

export function waitLabel(stalled: boolean, eta: number | null): Label {
  if (stalled) return { key: "wait.catchingUp" };
  return eta == null ? { key: "wait.soon" } : { key: "wait.startingIn", params: { n: Math.max(1, Math.ceil(eta)) } };
}

/** How fast chunks arrive, from the engine's progress reports. */
export class RateMeter {
  private first: [number, number] | null = null;
  private last: [number, number] | null = null;

  add(time: number, done: number) {
    if (!this.first || done < this.first[1]) this.first = [time, done];
    this.last = [time, done];
  }

  secPerChunk(): number | null {
    if (!this.first || !this.last || this.last[1] <= this.first[1]) return null;
    return (this.last[0] - this.first[0]) / (this.last[1] - this.first[1]);
  }
}

/** Splits `chunk_pcm` bytes (vocals then instrumental, each interleaved stereo) into four channels. */
export function splitChunk(bytes: ArrayBuffer): { vocals: [Float32Array, Float32Array]; inst: [Float32Array, Float32Array] } {
  const all = new Float32Array(bytes);
  const frames = all.length / 4;
  const take = (base: number): [Float32Array, Float32Array] => {
    const left = new Float32Array(frames);
    const right = new Float32Array(frames);
    for (let i = 0; i < frames; i++) {
      left[i] = all[base + 2 * i];
      right[i] = all[base + 2 * i + 1];
    }
    return [left, right];
  };
  return { vocals: take(0), inst: take(2 * frames) };
}
