import type { LyricLine, Word } from "$lib/api";

export const GAP_MIN = 3;
const LEAD = 0.35;

export type Item = { gap: true; start: number; end: number } | { gap: false; start: number; end: number; line: LyricLine };

const clamp01 = (x: number) => Math.min(1, Math.max(0, x));

/** When a line's singing stops: its last word's end, or the line's end when it has no words. */
const sungEnd = (line: LyricLine) => (line.words.at(-1)?.end_ms ?? line.end_ms) / 1000;

/** Lyric lines in seconds, with a countdown before the first line and after every break longer than GAP_MIN (measured from the last sung word). */
export function timeline(lines: LyricLine[]): Item[] {
  const items: Item[] = [];
  let prevEnd = 0;
  for (const line of lines) {
    const start = line.start_ms / 1000;
    if (start - prevEnd > GAP_MIN) items.push({ gap: true, start: prevEnd, end: start });
    items.push({ gap: false, start, end: line.end_ms / 1000, line });
    prevEnd = sungEnd(line);
  }
  return items;
}

/** Index of the item showing at time `t`; a line lights up slightly before it's sung. */
export function itemAt(items: Item[], t: number): number {
  let current = 0;
  items.forEach((x, i) => {
    if (t >= x.start - (x.gap ? 0 : LEAD)) current = i;
  });
  return current;
}

/** How much of a word is sung at time `t`, 0 to 1. */
export function wordProgress(w: Word, t: number): number {
  return clamp01((t - w.start_ms / 1000) / Math.max(0.001, (w.end_ms - w.start_ms) / 1000));
}

/** Fill of countdown dot `i` (0, 1, 2) at time `t`: one dot per second over a break's last three seconds. */
export function dotProgress(gap: { end: number }, i: number, t: number): number {
  return clamp01(t - (gap.end - 3) - i);
}
