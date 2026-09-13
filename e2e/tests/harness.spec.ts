import { expect, test } from "../fixtures";

/**
 * Harness stub: proves Playwright can attach to Lightpanda over CDP.
 * Product smoke lives in `smoke.spec.ts`.
 */
test("connects to Lightpanda over CDP", async ({ browser }) => {
  expect(browser.isConnected()).toBe(true);
});
