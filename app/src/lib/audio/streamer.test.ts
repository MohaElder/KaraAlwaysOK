import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Streamer } from "./streamer";

interface FakeNode {
  to: FakeNode | null;
  gainEvents: [number, number][];
  disconnected: boolean;
  started: number | null;
  stopped: number | null;
}

const audio = vi.hoisted(() => {
  const clock = { now: 0 };
  const sources: FakeNode[] = [];
  const node = () => {
    const n: FakeNode & Record<string, unknown> = { to: null, gainEvents: [], disconnected: false, started: null, stopped: null };
    const automate = (value: number, time: number) => void n.gainEvents.push([time, value]);
    Object.assign(n, {
      connect: (to: FakeNode) => void (n.to = to),
      disconnect: () => void (n.disconnected = true),
      start: (when: number) => void (n.started = when),
      stop: (when: number) => void (n.stopped = when),
      gain: { setTargetAtTime: automate, setValueAtTime: automate },
    });
    return n;
  };
  return { clock, sources, node };
});

const api = vi.hoisted(() => ({ chunkPcm: vi.fn(), playbackInfo: vi.fn() }));
vi.mock("$lib/api", () => api);
vi.mock("./keyshift", () => ({
  KeyShift: class {
    input = audio.node();
    works = true;
    latency = 0;
    async set() {}
  },
}));
vi.stubGlobal(
  "AudioContext",
  class {
    get currentTime() {
      return audio.clock.now;
    }
    destination = audio.node();
    createGain = audio.node;
    createBufferSource = () => {
      const s = audio.node();
      audio.sources.push(s);
      return s;
    };
    createBuffer = (_channels: number, length: number, rate: number) => ({ duration: length / rate, copyToChannel() {} });
    resume = async () => {};
  },
);

/** Gain along a node's path at `time`, taking each gain change as reached at once. */
function gainAt(from: FakeNode | null, time: number): number {
  let gain = 1;
  for (let n = from; n; n = n.to) {
    const past = n.gainEvents.filter(([t]) => t <= time);
    if (past.length) gain *= past[past.length - 1][1];
  }
  return gain;
}

const chunkBytes = (frames: number) => new ArrayBuffer(frames * 4 * 4);

beforeEach(() => {
  vi.useFakeTimers();
  audio.clock.now = 0;
  audio.sources.length = 0;
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

it("fades the old run out on its own and starts the new one at full volume after a seek", async () => {
  api.playbackInfo.mockResolvedValue({ chunkFrames: 44_100, chunksTotal: 10, chunksDone: 10, durationMs: 10_000 });
  api.chunkPcm.mockResolvedValue(chunkBytes(44_100));
  const streamer = new Streamer(() => {}, () => {}, () => {});
  await streamer.load(1, true);
  await vi.advanceTimersByTimeAsync(200);
  expect(streamer.phase).toBe("playing");
  const old = [...audio.sources];

  audio.clock.now = 1;
  streamer.seek(5.5);
  audio.clock.now = 1.05;
  await vi.advanceTimersByTimeAsync(250);
  expect(streamer.phase).toBe("playing");
  const fresh = audio.sources.slice(old.length);
  expect(fresh.length).toBeGreaterThan(0);
  for (const s of fresh) expect(gainAt(s.to, s.started!)).toBe(1);
  for (const s of old) {
    expect(s.stopped).toBeCloseTo(1.015);
    expect(s.to!.disconnected).toBe(true);
    expect(gainAt(s.to, 1.015)).toBe(0);
  }
});
