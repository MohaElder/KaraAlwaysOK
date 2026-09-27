import { expect, it } from "vitest";
import { say } from "./engine";
import { matchLocale } from "./index.svelte";

it("follows the system language, English when it isn't one of the six", () => {
  const tags = ["ja-JP", "zh-Hant-TW", "zh-TW", "zh-Hans-CN", "ko-KR", "es-MX", "fr-FR"];
  expect(tags.map(matchLocale)).toEqual(["ja", "zh-Hant", "zh-Hant", "zh-Hans", "ko", "es", "en"]);
});

it("says a problem code it doesn't know, or none at all, as a generic message instead of raw text", () => {
  expect(say({ problem: "fromANewerEngine", message: "Something new." })).toBe("Something went wrong.");
  expect(say({ problem: "fileMoved", message: "" })).toBe("The file was moved or deleted.");
  expect(say({ problem: null, message: "Plain text." })).toBe("Something went wrong.");
});
