import { test as base, expect, type Page } from "@playwright/test";

export { expect };

/** The phone page with its computer, mic and wake lock faked (tests/fake-phone.ts); fails a test that throws in the page. */
export const test = base.extend({
  page: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.stack ?? e.message));
    await page.route(
      (url) => url.pathname === "/phone",
      async (route) => {
        const response = await route.fetch();
        const body = (await response.text()).replace("<head>", '<head><script type="module" src="/tests/fake-phone.ts"></script>');
        await route.fulfill({ response, body });
      },
    );
    await use(page);
    expect(errors).toEqual([]);
  },
});

/** Every message the page sent to the computer. */
export const sent = (page: Page) => page.evaluate(() => window.fakePhone.sent);

/** How many frames of sound the page sent. */
export const frames = (page: Page) => page.evaluate(() => window.fakePhone.frames);

/** Sends a message from the computer to the page. */
export const server = (page: Page, msg: object) => page.evaluate((m) => window.fakePhone.server(m), msg);

/** Opens the page from the QR code, joins as `name` and allows the mic. */
export async function joinAs(page: Page, name = "Aiko") {
  await page.goto("/phone?code=4827");
  await page.getByLabel("Your name").fill(name);
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await page.getByRole("button", { name: "Allow microphone" }).click();
  await expect(page.getByRole("button", { name: "Mute" })).toBeVisible();
}
