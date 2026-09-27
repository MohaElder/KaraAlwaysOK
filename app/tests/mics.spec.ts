import { test, expect, calls, sing } from "./app";

test("the mic pill opens the phone window with the code, the address and how to get past the warning", async ({ page }) => {
  await page.getByRole("button", { name: "Phone mics" }).click();
  const sheet = page.getByRole("dialog", { name: "Sing into your phone" });
  await expect(sheet).toContainText("Scan with a phone on the same Wi-Fi.");
  await expect(sheet).toContainText("OKI-4827");
  await expect(sheet).toContainText("on Test-Mac.local");
  await expect(sheet).toContainText("iPhone: tap Show Details, then visit this website.");
  await expect(sheet).toContainText("Keep phones away from the speakers.");
  await expect(sheet.locator(".qr svg")).toBeVisible();
  await expect(sheet).toContainText("Waiting for phones to join…");
  await page.evaluate(() => window.fake.sessionEnded());
  await expect.poll(async () => (await calls(page, "phones_open")).length).toBe(2);
  await expect(sheet).toContainText("OKI-4827");
  await page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();
  expect(await calls(page, "phones_close")).toHaveLength(1);
});

test("joined phones show their name, level, volume and Remove, and the pill counts them", async ({ page }) => {
  await page.getByRole("button", { name: "Phone mics" }).click();
  const sheet = page.getByRole("dialog", { name: "Sing into your phone" });
  await page.evaluate(() =>
    window.fake.phones({
      join: { qr: "<svg></svg>", code: "OKI-4827", host: "Test-Mac.local" },
      phones: [
        { id: "a", name: "Aiko", connected: true, volume: 80 },
        { id: "b", name: "Ben", connected: false, volume: 80 },
        { id: "c", name: "Chie", connected: true, volume: 80 },
      ],
    }),
  );
  await page.evaluate(() => window.fake.levels([{ id: "a", level: 0.5, down: false }, { id: "c", level: 0.9, down: true }]));
  await expect(sheet).toContainText("Phones · 3");
  await expect(sheet.locator(".mic", { hasText: "Aiko" }).locator(".meter .lit")).toHaveCount(4);
  await expect(sheet.locator(".mic", { hasText: "Ben" })).toContainText("Reconnecting…");
  await expect(sheet.locator(".mic", { hasText: "Chie" })).toContainText("Turned down: too close to the speakers");
  await sheet.getByRole("slider", { name: "Volume for Aiko" }).fill("40");
  expect(await calls(page, "phone_volume")).toContainEqual({ id: "a", volume: 40 });
  await sheet.getByRole("button", { name: "Remove Aiko" }).click();
  expect(await calls(page, "phone_remove")).toEqual([{ id: "a" }]);
  await expect(page.getByText("Aiko removed")).toBeVisible();
  await sheet.getByRole("button", { name: "Done" }).click();
  await expect(page.getByRole("button", { name: "Phone mics" })).toContainText("3");
});

test("the app tells phones which song is playing and where, also when the phone window opens mid-song", async ({ page }) => {
  const playing = async (from = 0) => (await calls(page, "phones_clock")).slice(from).some((c) => c.key === 1 && c.playing === true);
  const karaoke = await sing(page, "Paper Boats");
  await expect.poll(() => playing()).toBe(true);
  const before = (await calls(page, "phones_clock")).length;
  await karaoke.getByRole("button", { name: "Phone mics" }).click();
  await expect.poll(() => playing(before)).toBe(true);
});

test("a guest's song starts when nothing is playing, and the Mac says who joined and who added what", async ({ page }) => {
  await page.evaluate(() => window.fake.guestAdds(3, "Aiko"));
  await expect(page.getByRole("region", { name: "Karaoke" })).toBeVisible();
  await page.waitForFunction(() => window.fake.started);
  await page.evaluate(() => window.fake.news({ kind: "joined", name: "Ben", mic: 2 }));
  await expect(page.getByText("Ben joined as Mic 2")).toBeVisible();
  await page.evaluate(() => window.fake.news({ kind: "added", name: "Ben", title: "Paper Boats" }));
  await expect(page.getByText("Ben added “Paper Boats”")).toBeVisible();
});
