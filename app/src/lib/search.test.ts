import { expect, it } from "vitest";
import type { LinkPreview, SearchOutcome } from "./api";
import { fromOutcome, withPreview } from "./search";

const link = (url: string): SearchOutcome => ({ kind: "link", url, host: "youtu.be" });
const text: SearchOutcome = { kind: "text", tracks: [], collections: [] };
const preview = (title: string, more: Partial<LinkPreview> = {}): LinkPreview => ({ title, channel: null, durationMs: null, thumbnail: null, ...more });

it("a_late_preview_for_an_older_link_is_ignored", () => {
  let v = fromOutcome({ kind: "none" }, "youtu.be/a", link("https://youtu.be/a"));
  v = fromOutcome(v, "youtu.be/b", link("https://youtu.be/b"));
  v = withPreview(v, "https://youtu.be/a", preview("Old Song"));
  expect(v).toMatchObject({ kind: "link", url: "https://youtu.be/b", preview: null });
  v = fromOutcome(v, "paper", text);
  expect(withPreview(v, "https://youtu.be/b", preview("Late"))).toEqual(v);
});

it("keeps a link's preview while it is retyped, fills in later details, and clears on empty text", () => {
  let v = fromOutcome({ kind: "none" }, "youtu.be/a", link("https://youtu.be/a"));
  v = withPreview(v, "https://youtu.be/a", preview("Made Up Song", { thumbnail: "t.jpg", channel: "Made Up Channel" }));
  v = withPreview(v, "https://youtu.be/a", preview("Made Up Song", { durationMs: 205_000, thumbnail: "other.webp" }));
  v = fromOutcome(v, " youtu.be/a ", link("https://youtu.be/a"));
  expect(v).toMatchObject({ preview: { channel: "Made Up Channel", durationMs: 205_000, thumbnail: "t.jpg" } });
  expect(fromOutcome(v, "  ", text)).toEqual({ kind: "none" });
});
