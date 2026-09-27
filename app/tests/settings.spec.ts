import type { Page } from "@playwright/test";
import { test, expect, row } from "./app";

const openSettings = async (page: Page) => {
  await page.getByRole("button", { name: "Settings" }).click();
  return page.getByRole("dialog", { name: "Settings" });
};

test("storage shows what is used and clears after asking", async ({ page }) => {
  const settings = await openSettings(page);
  await expect(settings).toContainText("1.5 GB used");
  await expect(settings).toContainText("3.5 GB free");
  await settings.getByRole("button", { name: "Clear storage" }).click();
  await expect(settings).toContainText("Clear all prepared songs?");
  await settings.getByRole("button", { name: "Clear", exact: true }).click();
  await expect(page.locator(".toasts")).toContainText("Storage cleared");
  await expect(settings).toContainText("0.0 GB used");
});

test("Clear storage waits while a song is being added", async ({ page }) => {
  await page.evaluate(() => window.fake.drop(["/Users/me/Music/Moth Lantern.m4a"]));
  await expect(row(page, "Moth Lantern")).toBeVisible();
  const settings = await openSettings(page);
  await expect(settings.getByRole("button", { name: "Clear storage" })).toBeDisabled();
});

test("picking a language changes the text at once, but not song titles", async ({ page }) => {
  const settings = await openSettings(page);
  await settings.getByRole("combobox", { name: "Language" }).selectOption("日本語");
  await expect(page.getByRole("dialog", { name: "設定" })).toBeVisible();
  await expect(page.getByRole("button", { name: "プレイリスト", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /東京の夜空/ })).toBeVisible();
});

test("the sidebar tabs fit on one line at a readable size in every language", async ({ page }) => {
  for (const name of ["English", "日本語", "한국어", "简体中文", "繁體中文", "Español"]) {
    await page.locator(".side .foot button").click();
    await page.getByRole("dialog").getByRole("combobox").first().selectOption(name);
    await page.keyboard.press("Escape");
    await expect(page.locator(".side .tabs .fit")).toHaveCount(3);
    for (const label of await page.locator(".side .tabs .fit").all()) {
      await expect.poll(() => label.evaluate((l) => l.scrollWidth <= l.clientWidth && parseFloat(getComputedStyle(l).fontSize) >= 11)).toBe(true);
    }
  }
});

test("the About row shows the icon, the name and the version", async ({ page }) => {
  const about = (await openSettings(page)).locator(".about");
  await expect(about).toContainText("KaraAlwaysOK");
  await expect(about).toContainText("Version 0.1.0");
  await expect.poll(() => about.locator("img").evaluate((img: HTMLImageElement) => img.naturalWidth)).toBeGreaterThan(0);
});
