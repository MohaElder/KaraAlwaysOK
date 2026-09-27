import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  use: { ...devices["Desktop Chrome"], baseURL: "http://localhost:1430", viewport: { width: 1280, height: 800 }, launchOptions: { args: ["--disable-features=LocalNetworkAccessChecks"] } },
  webServer: { command: "vite dev --config vite.e2e.config.js", url: "http://localhost:1430", reuseExistingServer: false },
});
