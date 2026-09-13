import { test as base, chromium } from "@playwright/test";
import { cdpUrl } from "./env";

/**
 * Attach every test to a running Lightpanda CDP server.
 * Import `test` / `expect` from this module, not `@playwright/test`.
 */
export const test = base.extend({
  browser: async ({}, use) => {
    let browser;
    try {
      browser = await chromium.connectOverCDP(cdpUrl);
    } catch (error) {
      throw new Error(
        `Failed to connectOverCDP at ${cdpUrl}. Start Lightpanda first ` +
          `(see e2e/README.md). ${String(error)}`,
        { cause: error },
      );
    }
    await use(browser);
    // On a CDP connection, close() disconnects and drops created contexts.
    // It does not stop the Lightpanda process.
    if (browser.isConnected()) {
      await browser.close();
    }
  },
});

export { expect } from "@playwright/test";
