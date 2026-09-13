import { defineConfig } from "@playwright/test";
import { baseURL } from "./env";

/**
 * Tests attach to Lightpanda with `chromium.connectOverCDP` (see `fixtures.ts`).
 * Do not set `use.connectOptions` — that uses Playwright's own protocol, not CDP.
 */
export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL,
  },
});
