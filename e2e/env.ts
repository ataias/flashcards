/** HTTP origin of the flashcards binary Playwright should open. */
export const baseURL = process.env.E2E_BASE_URL ?? "http://127.0.0.1:3000";

/**
 * Lightpanda CDP endpoint for `chromium.connectOverCDP`.
 * Accepts `http://host:port` (Playwright fetches `/json/version`) or `ws://host:port`.
 */
export const cdpUrl = process.env.CDP_URL ?? "http://127.0.0.1:9222";
