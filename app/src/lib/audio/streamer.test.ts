import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Streamer } from "./streamer";

const api = vi.hoisted(() => ({ chunkPcm: vi.fn(), playbackInfo: vi.fn() }));
vi.mock("$lib/api", () => api);
vi.mock("./keyshift", () => ({
  KeyShift: class {
    input = { gain: { setTargetAtTime() {}, setValueAtTime() {} } };
    works = true;
    latency = 0;
    async set() {}
  },
}));
vi.stubGlobal(
  "AudioContext",
  class {
    currentTime = 0;
    createGain = () => ({ connect() {}, gain: { setTargetAtTime() {} } });
    createBuffer = (_channels: number, length: number, rate: number) => ({ duration: length / rate, copyToChannel() {} });
    resume = async () => {};
  },
);

const chunkBytes = (frames: number) => new ArrayBuffer(frames * 4 * 4);

beforeEach(() => {
  vi.useFakeTimers();
  api.chunkPcm.mockReset();
  api.playbackInfo.mockReset();
});
afterEach(() => vi.useRealTimers());

it("reads every chunk again after two reads failed at once and the song was prepared again", async () => {
  api.playbackInfo.mockResolvedValue({ chunkFrames: 44_100, chunksTotal: 2, chunksDone: 2, durationMs: 2000 });
  api.chunkPcm.mockRejectedValue({ problem: "notPrepared", message: "" });
  const onFailed = vi.fn();
  const streamer = new Streamer(() => {}, () => {}, onFailed);
  await streamer.load(1, false);
  await vi.advanceTimersByTimeAsync(250);
  expect(api.chunkPcm).toHaveBeenCalledTimes(2);
  expect(onFailed).toHaveBeenCalledTimes(1);

  api.chunkPcm.mockReset().mockResolvedValue(new ArrayBuffer(16));
  streamer.onEngine({ kind: "progress", trackId: 1, chunksDone: 2, chunksTotal: 2 });
  await vi.advanceTimersByTimeAsync(100);
  expect(api.chunkPcm.mock.calls.map(([, index]) => index).sort()).toEqual([0, 1]);
});

it("keeps the song's exact length once its last chunk is in, whatever the tags say", async () => {
  api.playbackInfo.mockResolvedValue({ chunkFrames: 44_100, chunksTotal: 2, chunksDone: 2, durationMs: 2500 });
  api.chunkPcm.mockImplementation(async (_track: number, index: number) => chunkBytes(index === 0 ? 44_100 : 22_050));
  const streamer = new Streamer(() => {}, () => {}, () => {});
  await streamer.load(1, false);
  await vi.advanceTimersByTimeAsync(100);
  expect(streamer.songDuration).toBe(1.5);

  streamer.onEngine({ kind: "ready", trackId: 1 });
  await vi.advanceTimersByTimeAsync(100);
  expect(streamer.songDuration).toBe(1.5);
  expect(streamer.seek(2)).toBe(false);
  expect(streamer.position()).toBe(1.5);
});

it("counts down from the new song's pace, not the last song's", async () => {
  api.playbackInfo.mockResolvedValue({ chunkFrames: 44_100, chunksTotal: 200, chunksDone: 0, durationMs: 200_000 });
  api.chunkPcm.mockResolvedValue(chunkBytes(44_100));
  const streamer = new Streamer(() => {}, () => {}, () => {});
  const prepare = async (trackId: number) => {
    await streamer.load(trackId, true);
    streamer.onEngine({ kind: "progress", trackId, chunksDone: 0, chunksTotal: 200 });
    await vi.advanceTimersByTimeAsync(1000);
    streamer.onEngine({ kind: "progress", trackId, chunksDone: 1, chunksTotal: 200 });
  };
  await prepare(1);
  await vi.advanceTimersByTimeAsync(300_000);
  await prepare(2);
  expect(streamer.waiting).toEqual({ key: "wait.startingIn", params: { n: 5 } });
});
