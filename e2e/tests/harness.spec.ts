import { expect, test } from "../fixtures";

/**
 * Harness stub: proves Playwright can attach to Lightpanda over CDP.
 * The product smoke path (home → deck → card → study → /about) is a later issue.
 */
test("connects to Lightpanda over CDP", async ({ browser }) => {
  expect(browser.isConnected()).toBe(true);
});
