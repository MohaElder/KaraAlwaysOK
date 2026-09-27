import { test, expect, calls, composingEnter } from "./app";

test("a new playlist is named in place; Enter while composing waits, Enter saves", async ({ page }) => {
  await page.getByRole("button", { name: "New playlist" }).click();
  const name = page.getByRole("textbox", { name: "Playlist name" });
  await expect(name).toBeFocused();
  await page.keyboard.type("Road Trip");
  await composingEnter(page);
  await expect(name).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.locator(".toasts")).toContainText("Created “Road Trip”");
  await expect(page.getByRole("button", { name: "Road Trip" })).toBeVisible();
  expect(await calls(page, "rename_playlist")).toEqual([{ id: 100, name: "Road Trip" }]);
});

test("Escape on a new playlist removes it", async ({ page }) => {
  const items = page.locator(".side .item");
  await expect(items).toHaveCount(3);
  await page.getByRole("button", { name: "New playlist" }).click();
  await expect(items).toHaveCount(4);
  await page.getByRole("textbox", { name: "Playlist name" }).press("Escape");
  await expect(items).toHaveCount(3);
  expect(await calls(page, "delete_playlist")).toEqual([{ id: 100 }]);
});
