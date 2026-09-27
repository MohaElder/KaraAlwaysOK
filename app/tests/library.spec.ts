import { test, expect, row } from "./app";

test("the sidebar tabs list playlists, albums and artists", async ({ page }) => {
  const side = page.getByRole("complementary").first();
  await expect(side.getByRole("button", { name: "Friday Mix" })).toBeVisible();
  await side.getByRole("button", { name: "Albums" }).click();
  await expect(side.getByRole("button", { name: "Harbor Lights" })).toBeVisible();
  await expect(side.getByRole("button", { name: "Friday Mix" })).toBeHidden();
  await side.getByRole("button", { name: "Artists" }).click();
  await expect(side.getByRole("button", { name: "Juniper Row" })).toBeVisible();
});

test("a playlist shows a 2×2 cover, its songs and its length", async ({ page }) => {
  await page.getByRole("button", { name: "Friday Mix" }).click();
  const hero = page.locator(".hero");
  await expect(hero.getByRole("heading", { name: "Friday Mix" })).toBeVisible();
  await expect(hero.locator(".grid .art")).toHaveCount(4);
  await expect(hero.locator(".sub")).toHaveText("4 songs · 14 min");
  await expect(page.locator(".rows .row")).toHaveCount(4);
  await expect(row(page, "Paper Boats")).toContainText("Juniper Row – Harbor Lights");
  await expect(row(page, "Paper Boats")).toContainText("3:20");
});
