import { test, expect, calls, row, sing } from "./app";

test("the queue shows what's up next, reorders by dragging and removes", async ({ page }) => {
  await sing(page, "Paper Boats");
  await page.keyboard.press("Escape");
  await row(page, "Lemon Skies").click();
  await row(page, "Kettle Duet").click();
  await page.getByRole("region", { name: "Player" }).getByRole("button", { name: "Queue" }).click();
  const panel = page.getByRole("complementary", { name: "Queue" });
  const upNext = panel.locator("[data-qi]");
  await expect(panel).toContainText("Up next · 2");
  await expect(upNext).toHaveText(["Lemon Skies", "Kettle Duet"].map((t) => new RegExp(t)));

  await upNext.filter({ hasText: "Kettle Duet" }).locator(".grip").hover();
  await page.mouse.down();
  await upNext.filter({ hasText: "Lemon Skies" }).hover();
  await page.mouse.up();
  await expect.poll(() => calls(page, "queue_move")).toEqual([{ key: 3, to: 1 }]);
  await expect(upNext).toHaveText([/Kettle Duet/, /Lemon Skies/]);

  await upNext.filter({ hasText: "Lemon Skies" }).getByRole("button", { name: "Remove" }).click();
  await expect(panel).toContainText("Up next · 1");
  await expect(upNext).toHaveText([/Kettle Duet/]);
});

test("songs a guest added say who added them", async ({ page }) => {
  await sing(page, "Paper Boats");
  await page.evaluate(() => window.fake.guestAdds(3, "Aiko"));
  await page.keyboard.press("Escape");
  await page.getByRole("region", { name: "Player" }).getByRole("button", { name: "Queue" }).click();
  const row = page.getByRole("complementary", { name: "Queue" }).locator("[data-qi]", { hasText: "Lemon Skies" });
  await expect(row).toContainText("The Porchlights · added by Aiko");
});
