import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/public",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 30_000 },
  reporter: [["list"], ["html", { open: "never", outputFolder: "playwright-report/public" }]],
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "https://gpt-jungle.pages.dev",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
  },
  projects: [{ name: "public-chromium", use: { ...devices["Desktop Chrome"] } }],
});

