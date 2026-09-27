import { test, expect, calls, row } from "./app";

test("a dropped song steps through its toasts into the library without playing", async ({ page }) => {
  const drop = () => page.evaluate(() => window.fake.drop(["/Users/me/Music/Moth Lantern.m4a"]));
  const toasts = page.locator(".toasts");
  const added = row(page, "Moth Lantern");
  await drop();
  await expect(toasts).toContainText("Adding “Moth Lantern”");
  await expect(added).toHaveAttribute("aria-disabled", "true");
  await added.click({ force: true });

  const id = Number(await added.getAttribute("data-track"));
  const engine = (e: object) => page.evaluate((e) => window.fake.engine(e as never), { trackId: id, ...e });
  await engine({ kind: "stage", stage: "fetching" });
  await expect(toasts).toContainText("Adding “Moth Lantern” · Reading the file");
  await engine({ kind: "stage", stage: "findingLyrics" });
  await expect(toasts).toContainText("Adding “Moth Lantern” · Finding the lyrics");
  await engine({ kind: "added" });
  await expect(toasts).toContainText("Added “Moth Lantern” to your library");
  await expect(added).toHaveAttribute("aria-disabled", "false");
  expect(await calls(page, "queue_add")).toEqual([]);
  await expect(page.getByRole("region", { name: "Player" })).toBeHidden();

  await drop();
  await expect(toasts).toContainText("“Moth Lantern” is already in your library");
});
