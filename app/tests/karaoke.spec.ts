import { test, expect, sing, songAt } from "./app";

test("the current line and its words fill with the song's position", async ({ page }) => {
  const karaoke = await sing(page, "Paper Boats");
  const now = karaoke.locator(".line.now");
  await songAt(page, 6.5);
  await expect(now).toHaveText("Fold the morning paper");
  const fills = () => now.locator(".w").evaluateAll((ws) => ws.map((w) => (w as HTMLElement).style.getPropertyValue("--p")));
  await expect.poll(fills).toEqual(["1", "1", "0.5", "0"]);
  await songAt(page, 9);
  await expect(now).toHaveText("Send it down the drain");
  await expect(karaoke.locator(".line.past")).toHaveText("Fold the morning paper");
  await expect(karaoke.getByText("No lyrics found, sing it your way.")).toBeHidden();
});

test("three dots count down the last seconds of a break", async ({ page }) => {
  const karaoke = await sing(page, "Paper Boats");
  await songAt(page, 18.5);
  const dots = () => karaoke.locator(".gap.now i").evaluateAll((ds) => ds.map((d) => (d as HTMLElement).style.getPropertyValue("--p")));
  await expect.poll(dots).toEqual(["1", "0.5", "0"]);
});

test("duet lines sit left, right and center by voice", async ({ page }) => {
  const karaoke = await sing(page, "Kettle Duet");
  await expect(karaoke.locator(".line", { hasText: "Who boiled the water" })).toHaveCSS("text-align", "left");
  await expect(karaoke.locator(".line", { hasText: "I did, of course" })).toHaveCSS("text-align", "right");
  await expect(karaoke.locator(".line", { hasText: "Tea for two" })).toHaveCSS("text-align", "center");
});

test("a song without lyrics says so", async ({ page }) => {
  const karaoke = await sing(page, "Quiet Hours");
  await expect(karaoke.getByText("No lyrics found, sing it your way.")).toBeVisible();
});
