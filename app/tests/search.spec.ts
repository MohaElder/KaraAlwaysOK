import type { Page } from "@playwright/test";
import { test, expect, calls, composingEnter } from "./app";

const box = (page: Page) => page.getByRole("combobox", { name: "Search anything, or paste a link" });

test("typing shows matching songs, also by part of a CJK title", async ({ page }) => {
  await box(page).fill("lemon");
  await expect(page.getByRole("heading", { name: "Songs" })).toBeVisible();
  await expect(page.locator(".row")).toHaveText([/Lemon Skies/]);
  await box(page).fill("夜空");
  await expect(page.locator(".row")).toHaveText([/東京の夜空/]);
});

test("typing also shows YouTube, then Bilibili videos after the library's songs, and tapping one adds it without playing", async ({ page }) => {
  await box(page).fill("lemon");
  await expect(page.getByRole("main").getByRole("heading", { level: 2 })).toHaveText(["Songs", "YouTube", "Bilibili"]);
  await expect(page.getByRole("button", { name: "Add Made-up Cover to your library" })).toContainText("Someone Else · 3:20");
  await page.getByRole("button", { name: "Add Made-up Clip to your library" }).click();
  expect(await calls(page, "youtube_search")).toEqual([{ query: "lemon" }]);
  await expect(page.locator(".toasts")).toContainText("Adding “Made-up Clip”");
  expect(await calls(page, "add_link")).toEqual([{ url: "https://www.youtube.com/watch?v=madeup" }]);
  expect(await calls(page, "queue_add")).toEqual([]);
  await expect(page.getByRole("region", { name: "Player" })).toBeHidden();
});

test("typing suggests your songs, then YouTube's and Bilibili's phrases once each, and ↓ + Enter runs a phrase's search", async ({ page }) => {
  await box(page).pressSequentially("lemon");
  const suggestions = page.getByRole("listbox", { name: "Suggestions" });
  await expect(suggestions.getByRole("option")).toHaveText(["Lemon Skies", "lemon karaoke", "lemon live", "lemon cover"]);
  await box(page).press("ArrowDown");
  await box(page).press("ArrowDown");
  await box(page).press("Enter");
  await expect(suggestions).toBeHidden();
  await expect(box(page)).toHaveValue("lemon karaoke");
  await expect.poll(() => calls(page, "youtube_search")).toContainEqual({ query: "lemon karaoke" });
});

test("a pasted link shows its preview, and Enter adds it only once composing is done", async ({ page }) => {
  await box(page).fill("https://youtu.be/abc123");
  await expect(page.getByRole("heading", { name: "From this link" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Add Made-up Clip to your library" })).toContainText("Someone Sings · 3:05");
  await composingEnter(page);
  expect(await calls(page, "add_link")).toEqual([]);
  await page.keyboard.press("Enter");
  await expect.poll(() => calls(page, "add_link")).toEqual([{ url: "https://youtu.be/abc123" }]);
});

test("a streaming link is refused in plain words", async ({ page }) => {
  await box(page).fill("https://open.spotify.com/track/1");
  await expect(page.getByRole("search").getByRole("status")).toHaveText(/open.spotify.com links can't be downloaded\s*Search for the song instead\./);
});
