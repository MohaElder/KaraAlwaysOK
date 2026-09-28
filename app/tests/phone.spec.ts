import { test, expect, sent, frames, joinAs, server } from "./phone";

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

test("a guest joins with the code from the QR code, allows the mic and can mute", async ({ page }) => {
  await page.goto("/phone?code=4827");
  await expect(page.getByLabel("Join code")).toHaveValue("4827");
  await expect(page.getByText("Filled in from the QR code")).toBeVisible();
  const join = page.getByRole("button", { name: "Join", exact: true });
  await expect(join).toBeDisabled();
  await page.getByLabel("Your name").fill("Aiko");
  await join.click();
  await page.getByRole("button", { name: "Allow microphone" }).click();
  await expect(page.getByText("Tap to mute")).toBeVisible();
  await expect(page.locator(".chip")).toHaveText("Aiko");
  await expect.poll(async () => (await sent(page)).some((m) => m.t === "ping")).toBe(true);
  expect(await sent(page)).toEqual(expect.arrayContaining([
    { t: "join", code: "4827", id: expect.any(String), name: "Aiko" },
    { t: "live", on: true, rate: 48000 },
  ]));
  expect(await page.evaluate(() => window.fakePhone.wakeLocks)).toBe(1);
  await page.evaluate(() => window.fakePhone.frame());
  expect(await frames(page)).toBe(1);
  await page.getByRole("button", { name: "Mute" }).click();
  await expect(page.getByText("Tap to sing")).toBeVisible();
  await page.evaluate(() => window.fakePhone.frame());
  expect(await frames(page)).toBe(1);
  expect((await sent(page)).filter((m) => m.t === "live").at(-1)).toEqual({ t: "live", on: false, rate: 48000 });
});

test("a wrong code or a full room sends the guest back to Join with the reason", async ({ page }) => {
  await page.goto("/phone?code=1111");
  await page.getByLabel("Your name").fill("Aiko");
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await expect(page.getByText("That didn't work. Check the code and try again.")).toBeVisible();
  await page.getByLabel("Join code").fill("4827");
  await page.evaluate(() => (window.fakePhone.full = true));
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await expect(page.getByText("This room is full.")).toBeVisible();
});

test("a blocked mic shows how to allow it, and Try again asks again", async ({ page }) => {
  await page.goto("/phone?code=4827");
  await page.evaluate(() => (window.fakePhone.micAllowed = false));
  await page.getByLabel("Your name").fill("Aiko");
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await page.getByRole("button", { name: "Allow microphone" }).click();
  await expect(page.getByRole("heading", { name: "Mic blocked" })).toBeVisible();
  await expect(page.getByText(/iPhone: tap aA/)).toBeVisible();
  await page.evaluate(() => (window.fakePhone.micAllowed = true));
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByText("Tap to mute")).toBeVisible();
});

test("when Wi-Fi drops it rejoins as the same phone, sends no late sound, and gives up after two minutes", async ({ page }) => {
  await page.clock.install();
  await joinAs(page);
  await page.evaluate(() => window.fakePhone.drop());
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeVisible();
  await expect(page.getByText("Waiting for Wi-Fi")).toBeVisible();
  await page.evaluate(() => window.fakePhone.frame());
  await page.clock.fastForward(2_100);
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeHidden();
  const joins = (await sent(page)).filter((m) => m.t === "join");
  expect(joins).toHaveLength(2);
  expect(joins[1].id).toBe(joins[0].id);
  expect(await frames(page)).toBe(0);
  expect((await sent(page)).filter((m) => m.t === "live")).toHaveLength(2);
  await page.evaluate(() => {
    window.fakePhone.unreachable = true;
    window.fakePhone.drop();
  });
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeVisible();
  await page.clock.fastForward(121_000);
  await expect(page.getByRole("heading", { name: "Lost the connection to the computer" })).toBeVisible();
});

test("when the host ends the session the guest sees it and can join again", async ({ page }) => {
  await joinAs(page, "Aiko");
  await page.evaluate(() => window.fakePhone.end());
  await expect(page.getByRole("heading", { name: "The host ended the session" })).toBeVisible();
  await expect(page.getByText("Thanks for singing, Aiko!")).toBeVisible();
  expect(await page.evaluate(() => window.fakePhone.released)).toBe(1);
  await page.getByRole("button", { name: "Join again" }).click();
  await expect(page.getByLabel("Your name")).toHaveValue("Aiko");
});

test("a connection gone silent is replaced, and a mic stopped by the lock screen waits for a tap", async ({ page }) => {
  await page.clock.install();
  await joinAs(page);
  await page.evaluate(() => (window.fakePhone.quiet = true));
  await page.clock.fastForward(3_500);
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeVisible();
  await page.evaluate(() => (window.fakePhone.quiet = false));
  await page.clock.fastForward(2_100);
  await expect.poll(async () => (await sent(page)).filter((m) => m.t === "join").length).toBe(2);
  await page.evaluate(() => {
    window.fakePhone.audio = "suspended";
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect(page.getByText("Tap to sing")).toBeVisible();
  await page.getByRole("button", { name: "Unmute" }).click();
  await expect(page.getByText("Tap to mute")).toBeVisible();
});

test.describe("on a phone set to Japanese", () => {
  test.use({ locale: "ja-JP" });

  test("the page follows the phone's language", async ({ page }) => {
    await page.goto("/phone?code=4827");
    await expect(page.getByRole("heading", { name: "カラオケに参加" })).toBeVisible();
  });
});

test("the Mic tab shows the song, what's next and the lyrics in time with the computer", async ({ page }) => {
  await joinAs(page);
  await expect(page.locator(".psong")).toContainText("Paper Boats");
  await expect(page.locator(".pnext")).toContainText("Up next: Lemon Skies · The Porchlights");
  expect(await sent(page)).toContainEqual({ t: "lyrics", trackId: 1 });
  await server(page, { t: "clock", key: 1, positionMs: 6_000, playing: false });
  await expect(page.locator(".plyr .now")).toHaveText("Fold the morning paper");
  await server(page, { t: "clock", key: 1, positionMs: 9_000, playing: false });
  await expect(page.locator(".plyr .now")).toHaveText("Send it down the drain");
  await expect(page.locator(".plyr .next")).toHaveText("Wave from the bridge");
  const key = await page.evaluate(() => window.fakePhone.play(2));
  await server(page, { t: "clock", key, positionMs: 1_200, playing: false });
  await expect(page.getByText("Male part")).toBeVisible();
  await page.evaluate(() => window.fakePhone.play(3));
  await expect(page.getByText("No lyrics found, sing it your way.")).toBeVisible();
  await server(page, { t: "player", snapshot: { entries: [], current: null, ended: false, lyricOffsetMs: 0 } });
  await expect(page.getByText("Waiting for a song")).toBeVisible();
});

test("Voice and Singer each open their own slider and send its value", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Voice" }).click();
  await page.getByRole("slider", { name: "Your voice volume" }).fill("55");
  await page.getByRole("button", { name: "Singer" }).click();
  await expect(page.getByRole("slider", { name: "Your voice volume" })).toBeHidden();
  const singer = page.getByRole("slider", { name: "Singer: left is the original, right removes the singer" });
  await expect(singer).toHaveValue("100");
  await singer.fill("30");
  await expect.poll(async () => (await sent(page)).filter((m) => m.t === "singer").at(-1)).toEqual({ t: "singer", v: 30 });
  expect(await sent(page)).toContainEqual({ t: "voice", v: 55 });
  await page.reload();
  await joinAs(page);
  expect(await sent(page)).toContainEqual({ t: "voice", v: 55 });
});

test("Effect picks a preset and its strength, and the phone remembers it", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Effect", exact: true }).click();
  await expect(page.getByRole("slider", { name: "Effect strength" })).toBeDisabled();
  await page.getByRole("button", { name: "Auto-tune" }).click();
  await page.getByRole("slider", { name: "Effect strength" }).fill("60");
  await expect.poll(async () => (await sent(page)).filter((m) => m.t === "effect").at(-1)).toEqual({ t: "effect", kind: "autoTune", amount: 60 });
  await page.reload();
  await joinAs(page);
  expect(await sent(page)).toContainEqual({ t: "effect", kind: "autoTune", amount: 60 });
  await page.getByRole("button", { name: "Effect", exact: true }).click();
  await expect(page.getByRole("button", { name: "Auto-tune" })).toHaveAttribute("aria-pressed", "true");
});

test("Songs lists every song, searches as you type, and queues with Play next or Add to queue", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Songs", exact: true }).click();
  await expect(page.getByText("All songs")).toBeVisible();
  await expect(page.locator(".prow")).toHaveCount(3);
  const box = page.getByPlaceholder("Search songs");
  await box.fill("lem");
  await expect(page.locator(".prow")).toHaveText([/Lemon Skies/]);
  await page.locator(".prow", { hasText: "Lemon Skies" }).getByRole("button", { name: "Play next" }).dblclick();
  expect((await sent(page)).filter((m) => m.t === "add")).toEqual([{ t: "add", trackId: 3, next: true }]);
  await expect(page.locator(".pib.done")).toHaveCount(1);
  await server(page, { t: "results", q: "old", outcome: { kind: "text", tracks: [], collections: [] } });
  await expect(page.locator(".prow")).toHaveText([/Lemon Skies/]);
  await box.fill("https://youtu.be/abc");
  await expect(page.getByText("Made-up Song")).toBeVisible();
  await page.getByRole("button", { name: "Add to queue", exact: true }).click();
  expect(await sent(page)).toContainEqual({ t: "addLink", url: "https://youtu.be/abc", next: false });
  await box.fill("https://open.spotify.com/track/x");
  await expect(page.getByText("spotify.com links can't be downloaded")).toBeVisible();
});

test("Queue shows who added songs, reorders by dragging and removes", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Queue", exact: true }).click();
  const rows = page.locator("[data-qi]");
  await expect(rows).toHaveText([/Lemon Skies/, /Kettle Duet/]);
  await expect(rows.first()).toContainText("added by Ben");
  await rows.filter({ hasText: "Kettle Duet" }).locator(".grip").hover();
  await page.mouse.down();
  const target = (await rows.filter({ hasText: "Lemon Skies" }).boundingBox())!;
  await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 5 });
  await page.mouse.up();
  expect(await sent(page)).toContainEqual({ t: "move", key: 3, to: 1 });
  await rows.filter({ hasText: "Lemon Skies" }).getByRole("button", { name: "Remove" }).click();
  expect(await sent(page)).toContainEqual({ t: "remove", key: 2 });
});

test("Songs also finds videos on YouTube and Bilibili, which queue like a pasted link", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Songs", exact: true }).click();
  const box = page.getByPlaceholder("Search songs");
  await box.fill("clip");
  await expect(page.locator(".cap")).toHaveText(["YouTube", "Bilibili"]);
  await expect(page.getByText("Someone Sings · 3:05")).toBeVisible();
  await expect(page.getByText("Made-up Cover")).toBeVisible();
  await expect(page.getByText("Nothing matches “clip”")).toBeHidden();
  await expect(page.getByRole("button", { name: "Add Made-up Clip to the queue" })).toBeVisible();
  await page.locator(".lrow", { hasText: "Made-up Clip" }).getByRole("button", { name: "Play next" }).dblclick();
  expect(await sent(page)).toEqual(expect.arrayContaining([{ t: "web", source: "youtube", q: "clip" }, { t: "web", source: "bilibili", q: "clip" }]));
  expect((await sent(page)).filter((m) => m.t === "addLink")).toEqual([{ t: "addLink", url: "https://www.youtube.com/watch?v=made-up", next: true }]);
  await expect(page.locator(".lrow .ib.done")).toHaveCount(1);
  await box.fill("zzqx");
  await expect(page.getByText("Nothing matches “zzqx”")).toBeVisible();
});
