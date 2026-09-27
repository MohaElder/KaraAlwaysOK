import { describe, expect, it } from "vitest";
import { canStart, clampSeek, planStart, RateMeter, readyEnd, splitChunk, startEta, waitLabel, windowRange } from "./chunks";

describe("lining chunks up on the audio clock", () => {
  it("starts a later chunk at its slot and a running one part-way in", () => {
    expect(planStart(2, 10, 10, 100, 105)).toEqual({ when: 120, offset: 0 });
    expect(planStart(0, 10, 10, 100, 104.5)).toEqual({ when: 104.5, offset: 4.5 });
    expect(planStart(0, 10, 10, 100, 111)).toBeNull();
  });

  it("lines back-to-back chunks up to the sample", () => {
    const anchor = 12.345;
    const starts = [0, 1, 2, 3].map((i) => planStart(i, 441_000 / 44_100, 10, anchor, anchor)!.when);
    expect(starts.slice(1).map((w, i) => Math.round((w - starts[i]) * 44_100))).toEqual([441_000, 441_000, 441_000]);
  });

  it("keeps the previous chunk and the next three decoded", () => {
    expect(windowRange(25, 10, 30)).toEqual([1, 5]);
    expect(windowRange(3, 10, 2)).toEqual([0, 1]);
  });
});

describe("waiting for the song to be ready", () => {
  it("a_song_shorter_than_one_chunk_can_start_and_ends_on_time", () => {
    const ready = readyEnd(1, 1, 10, 4.2);
    expect(ready).toBe(4.2);
    expect(canStart(0, ready, 4.2, 6)).toBe(true);
    expect(planStart(0, 10, 4.2, 50, 54.3)).toBeNull();
  });

  it("needs six seconds ahead to start, or the rest of the song, and never trusts an unknown length", () => {
    expect(canStart(0, readyEnd(0, 20, 10, 200), 200, 6)).toBe(false);
    expect(canStart(0, readyEnd(1, 20, 10, 200), 200, 6)).toBe(true);
    expect(canStart(15, readyEnd(2, 20, 10, 200), 200, 6)).toBe(false);
    expect(canStart(0, readyEnd(0, null, 10, Infinity), Infinity, 6)).toBe(false);
  });

  it("seeking_past_the_prepared_part_lands_inside_it", () => {
    expect(clampSeek(95, 30, 200)).toEqual({ pos: 29, clamped: true });
    expect(clampSeek(12, 30, 200)).toEqual({ pos: 12, clamped: false });
    expect(clampSeek(199, 200, 200)).toEqual({ pos: 199, clamped: false });
  });

  it("counts down from how fast parts are arriving", () => {
    const meter = new RateMeter();
    expect(waitLabel(false, startEta(0, 6, 0, 10, 200, meter.secPerChunk()))).toEqual({ key: "wait.soon" });
    meter.add(1.0, 0);
    meter.add(1.5, 1);
    meter.add(2.0, 2);
    expect(meter.secPerChunk()).toBe(0.5);
    expect(waitLabel(false, startEta(25, 6, 2, 10, 200, meter.secPerChunk()))).toEqual({ key: "wait.startingIn", params: { n: 1 } });
    expect(waitLabel(true, 0)).toEqual({ key: "wait.catchingUp" });
  });
});

it("splits chunk bytes into vocal and instrumental channels", () => {
  const { vocals, inst } = splitChunk(new Float32Array([1, 2, 3, 4, 5, 6, 7, 8]).buffer);
  expect([...vocals[0], ...vocals[1], ...inst[0], ...inst[1]]).toEqual([1, 3, 2, 4, 5, 7, 6, 8]);
});
