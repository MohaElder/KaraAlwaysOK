import { test, expect, sing } from "./app";

test("Space plays and pauses, but not while typing", async ({ page }) => {
  await sing(page, "Paper Boats");
  const bar = page.getByRole("region", { name: "Player" });
  await page.evaluate(() => (document.activeElement as HTMLElement).blur());
  await page.keyboard.press(" ");
  await expect(bar.getByRole("button", { name: "Play", exact: true })).toBeVisible();
  await page.keyboard.press(" ");
  await expect(bar.getByRole("button", { name: "Pause" })).toBeVisible();
  const search = page.getByRole("combobox", { name: "Search anything, or paste a link" });
  await search.focus();
  await page.keyboard.type(" /");
  await expect(search).toHaveValue(" /");
  await expect(bar.getByRole("button", { name: "Pause" })).toBeVisible();
});

test("/ leaves karaoke and focuses search", async ({ page }) => {
  const karaoke = await sing(page, "Paper Boats");
  await page.keyboard.press("/");
  await expect(karaoke).toBeHidden();
  await expect(page.getByRole("combobox", { name: "Search anything, or paste a link" })).toBeFocused();
});

test("Escape closes the menu, then the queue, then karaoke", async ({ page }) => {
  const karaoke = await sing(page, "Paper Boats");
  const bar = page.getByRole("region", { name: "Player" });
  const queue = page.getByRole("complementary", { name: "Queue" });
  await bar.getByRole("button", { name: "Queue" }).click();
  await bar.getByRole("button", { name: "More" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toBeHidden();
  await expect(queue).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(queue).toBeHidden();
  await expect(karaoke).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(karaoke).toBeHidden();
});
