import type { Page } from "@playwright/test";
import { test, expect, calls, composingEnter } from "./app";

const box = (page: Page) => page.getByRole("textbox", { name: "Search anything, or paste a link" });

test("typing shows matching songs, also by part of a CJK title", async ({ page }) => {
  await box(page).fill("lemon");
  await expect(page.getByRole("heading", { name: "Songs" })).toBeVisible();
  await expect(page.locator(".row")).toHaveText([/Lemon Skies/]);
  await box(page).fill("夜空");
  await expect(page.locator(".row")).toHaveText([/東京の夜空/]);
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
