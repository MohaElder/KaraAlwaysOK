import { test, expect, calls, sing } from "./app";

test("tapping a song opens karaoke, and collapsing leaves it in the player bar", async ({ page }) => {
  const karaoke = await sing(page, "Paper Boats");
  await expect(karaoke).toContainText("Paper Boats");
  await karaoke.getByRole("button", { name: "Back to library" }).click();
  await expect(karaoke).toBeHidden();
  await expect(page.getByRole("region", { name: "Player" })).toContainText("Paper Boats");
});

test("the singer slider names how much of the singer is removed", async ({ page }) => {
  await sing(page, "Paper Boats");
  const slider = page.getByRole("slider", { name: /^Singer/ });
  await expect(slider).toHaveAttribute("aria-valuetext", "Singer: removed");
  await slider.fill("0");
  await expect(slider).toHaveAttribute("aria-valuetext", "Singer: original");
  await slider.fill("40");
  await expect(slider).toHaveAttribute("aria-valuetext", "Singer: 40% removed");
  await expect.poll(async () => (await calls(page, "set_singer")).at(-1)).toEqual({ value: 40 });
});

test("the … menu opens outside the player bar and closes on an outside click or Escape", async ({ page }) => {
  await sing(page, "Paper Boats");
  const more = page.getByRole("region", { name: "Player" }).getByRole("button", { name: "More" });
  const menu = page.getByRole("menu");
  await more.click();
  await expect(menu).toContainText("Lyrics timing");
  expect(await menu.evaluate((el) => el.closest(".bar"))).toBeNull();
  await page.mouse.click(640, 400);
  await expect(menu).toBeHidden();
  await more.click();
  await expect(menu).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
});
