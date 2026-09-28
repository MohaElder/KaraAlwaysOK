import { test, expect, calls, row, sing } from "./app";

test("Play next and Add to queue line the song up", async ({ page }) => {
  await sing(page, "Paper Boats");
  await page.keyboard.press("Escape");
  await row(page, "Lemon Skies").click({ button: "right" });
  await page.getByRole("menu").getByRole("button", { name: "Play next" }).click();
  await expect(page.locator(".toasts")).toContainText("“Lemon Skies” plays next");
  await expect(page.getByRole("menu")).toBeHidden();
  await row(page, "Quiet Hours").click({ button: "right" });
  await page.getByRole("menu").getByRole("button", { name: "Add to queue" }).click();
  await expect(page.locator(".toasts")).toContainText("Added to queue");
  expect((await calls(page, "queue_add")).slice(1)).toEqual([{ trackId: 3, next: true }, { trackId: 5, next: false }]);
});

test("Add to playlist checks the playlists that have the song and adds to one", async ({ page }) => {
  const playlists = async (title: string) => {
    await row(page, title).click({ button: "right" });
    await page.getByRole("menu").getByRole("button", { name: "Add to playlist" }).click();
    return page.getByRole("menu").nth(1).getByRole("button", { name: "Friday Mix" });
  };
  await expect((await playlists("Paper Boats")).locator("svg")).toHaveCount(2);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
  const friday = await playlists("Quiet Hours");
  await expect(friday.locator("svg")).toHaveCount(1);
  await friday.click();
  await expect(page.locator(".toasts")).toContainText("Added to “Friday Mix”");
  expect(await calls(page, "add_to_playlist")).toEqual([{ playlistId: 2, trackId: 5 }]);
});

test("Find lyrics again looks the song up now and shows what it found", async ({ page }) => {
  const karaoke = await sing(page, "Quiet Hours");
  await expect(karaoke.getByText("No lyrics found, sing it your way.")).toBeVisible();
  await page.keyboard.press("Escape");
  await row(page, "Quiet Hours").click({ button: "right" });
  await page.getByRole("menu").getByRole("button", { name: "Find lyrics again" }).click();
  await expect(page.locator(".toasts")).toContainText("Found lyrics");
  expect(await calls(page, "find_lyrics_again")).toEqual([{ trackId: 5 }]);
  await page.locator(".np").click();
  await expect(karaoke.getByText("No lyrics found, sing it your way.")).toBeHidden();
});

test("Edit info starts in the title and saves", async ({ page }) => {
  await row(page, "Paper Boats").click({ button: "right" });
  await page.getByRole("menu").getByRole("button", { name: "Edit info" }).click();
  const title = page.getByRole("dialog", { name: "Edit info" }).getByRole("textbox", { name: "Title" });
  await expect(title).toBeFocused();
  await title.fill("Paper Planes");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".toasts")).toContainText("Saved");
  await expect(row(page, "Paper Planes")).toBeVisible();
  expect(await calls(page, "edit_track")).toEqual([{ trackId: 1, title: "Paper Planes", artist: "Juniper Row", album: "Harbor Lights" }]);
});

test("Delete hides the song at once, and Undo brings it back", async ({ page }) => {
  await row(page, "Lemon Skies").click({ button: "right" });
  await page.getByRole("menu").getByRole("button", { name: "Delete from library" }).click();
  await page.getByRole("menu").getByRole("button", { name: "Delete", exact: true }).click();
  await expect(row(page, "Lemon Skies")).toBeHidden();
  await page.locator(".toasts").getByRole("button", { name: "Undo" }).click();
  await expect(row(page, "Lemon Skies")).toBeVisible();
  expect(await calls(page, "delete_track")).toEqual([]);
});
