import { expect, it } from "vitest";
import { mergePhrases } from "./suggest";

it("merges every source's phrases in order, once each, leaving out what was typed", () => {
  expect(mergePhrases("Paper b", [["paper b", "paper boats", "Paper Boats karaoke"], ["PAPER BOATS", "paper birds"]])).toEqual([
    "paper boats",
    "Paper Boats karaoke",
    "paper birds",
  ]);
});
