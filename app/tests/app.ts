import { test as base, expect, type Locator, type Page } from "@playwright/test";

export { expect };

/** The app with its backend faked (tests/fake-backend.ts), opened on the library; fails a test that throws in the page. */
export const test = base.extend({
  page: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.stack ?? e.message));
    await page.route(
      (url) => url.pathname === "/",
      async (route) => {
        const response = await route.fetch();
        const body = (await response.text()).replace("<head>", '<head><script type="module" src="/tests/fake-backend.ts"></script>');
        await route.fulfill({ response, body });
      },
    );
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Imported" })).toBeVisible();
    await use(page);
    expect(errors).toEqual([]);
  },
});

/** The arguments of every call the app made to `cmd`. */
export const calls = (page: Page, cmd: string) =>
  page.evaluate((cmd) => window.fake.calls.filter((c) => c.cmd === cmd).map((c) => c.args), cmd);

export const row = (page: Page, title: string) => page.locator(".row", { hasText: title });

/** Taps a song row, waits for its audio to start, and returns the karaoke view. */
export async function sing(page: Page, title: string): Promise<Locator> {
  await row(page, title).click();
  await page.waitForFunction(() => window.fake.started);
  return page.getByRole("region", { name: "Karaoke" });
}

/** Moves the playing song to `seconds`; the fake audio clock starts songs 0.1 s after it was set. */
export const songAt = (page: Page, seconds: number) => page.evaluate((s) => (window.fake.time = 0.1 + s), seconds);

/** Presses Enter as an input method does while it is still composing. */
export async function composingEnter(page: Page) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 229, text: "\r" });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 229 });
}
