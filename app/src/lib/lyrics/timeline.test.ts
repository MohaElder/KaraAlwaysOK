import { expect, it } from "vitest";
import type { LyricLine } from "$lib/api";
import { dotProgress, itemAt, timeline, wordProgress } from "./timeline";

const line = (start: number, end: number, text: string): LyricLine => {
  const words = text.split(" ");
  const step = (end - start) / words.length;
  return {
    start_ms: start * 1000,
    end_ms: end * 1000,
    text,
    words: words.map((w, i) => ({ start_ms: (start + i * step) * 1000, end_ms: (start + (i + 1) * step) * 1000, text: w })),
  };
};

it("countdowns_before_the_first_line_and_after_long_breaks", () => {
  const items = timeline([line(5, 9, "paper boats along the gutter"), line(9, 13, "still floating after rain"), line(20, 24, "we sail them anyway")]);
  expect(items.map((x) => (x.gap ? `gap ${x.start}-${x.end}` : x.line.text))).toEqual([
    "gap 0-5",
    "paper boats along the gutter",
    "still floating after rain",
    "gap 13-20",
    "we sail them anyway",
  ]);
  expect(timeline([line(2, 6, "short intro")]).map((x) => x.gap)).toEqual([false]);
  expect(timeline([])).toEqual([]);
  const tail = { ...line(20, 30, "a b"), words: [{ start_ms: 20_000, end_ms: 20_600, text: "a" }, { start_ms: 20_600, end_ms: 21_200, text: "b" }] };
  expect(timeline([line(15, 20, "one two"), tail, line(30, 34, "after the break")]).map((x) => x.gap && `${x.start}-${x.end}`)).toEqual(["0-15", false, false, "21.2-30", false]);
});

it("lights a line just before it is sung", () => {
  const items = timeline([line(5, 9, "a b"), line(9, 13, "c d")]);
  expect([itemAt(items, 1), itemAt(items, 4.7), itemAt(items, 8.7)]).toEqual([0, 1, 2]);
});

it("fills words and countdown dots over time", () => {
  const w = { start_ms: 1000, end_ms: 2000, text: "x" };
  expect([wordProgress(w, 0.5), wordProgress(w, 1.5), wordProgress(w, 3)]).toEqual([0, 0.5, 1]);
  expect([0, 1, 2].map((i) => dotProgress({ end: 10 }, i, 8.5))).toEqual([1, 0.5, 0]);
});
