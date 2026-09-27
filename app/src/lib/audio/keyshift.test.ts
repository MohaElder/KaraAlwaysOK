import { expect, it, vi } from "vitest";
import { KeyShift } from "./keyshift";

vi.mock("signalsmith-stretch", () => ({ default: () => new Promise(() => {}) }));

it("gives up on a key shifter that never starts and says so once", async () => {
  vi.useFakeTimers();
  const node = { connect() {}, disconnect() {} };
  const ctx = { state: "running", destination: {}, createGain: () => node } as unknown as AudioContext;
  const onBroken = vi.fn();
  const key = new KeyShift(ctx, onBroken);
  const shifts = [key.set(2), key.set(3)];
  await vi.advanceTimersByTimeAsync(10_000);
  await Promise.all(shifts);
  expect(key.works).toBe(false);
  expect(onBroken).toHaveBeenCalledTimes(1);
  vi.useRealTimers();
});
