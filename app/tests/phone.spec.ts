import { test, expect, sent, frames, joinAs } from "./phone";

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
  await expect(page.getByRole("heading", { name: "The host ended the session" })).toBeVisible();
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
