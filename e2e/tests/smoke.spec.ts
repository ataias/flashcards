import type { Page, Response } from "@playwright/test";
import { expect, test } from "../fixtures";
import { baseURL } from "../env";

const ADMIN = "admin";
const PASSWORD = "secret";
const FRONT = "e2e smoke front";
const BACK = "e2e smoke back";

/**
 * Users happy path: empty DB stays up → bootstrap → login → Default
 * (seeded on bootstrap) → create Card → Study reveal + rate → /about.
 * This spec is the prepare path (no seed binary): Playwright creates the
 * first admin through the real bootstrap form. Auth and Deck/Card/Study
 * posts go through real forms so CSRF cookie+field stay in sync. No API
 * fallbacks that omit csrf.
 *
 * Loopback HTTP omits cookie Secure (see FLASHCARDS_COOKIE_SECURE). If a
 * process still emits Secure cookies, Lightpanda will not attach them to
 * http:// form POSTs — allowHttpCookies copies those without Secure.
 */
test("bootstrap, login, Default deck, create card, study, about", async ({ page }) => {
  const serverErrors = trackServerErrors(page);

  await gotoReachable(page, "/");
  await expect(page).toHaveURL(/\/bootstrap\/?$/);
  await expect(page.getByRole("heading", { level: 1, name: "Create admin" })).toBeVisible();
  await expect(page.getByText("No Users yet")).toBeVisible();

  await submitAuthForm(page, "Create admin", ADMIN, PASSWORD);
  await expect(page).toHaveURL(/\/login\/?$/);
  await expect(page.getByRole("heading", { level: 1, name: "Log in" })).toBeVisible();

  await submitAuthForm(page, "Log in", ADMIN, PASSWORD);
  await allowHttpCookies(page);
  if (await page.getByRole("heading", { level: 1, name: "Log in" }).isVisible()) {
    await gotoReachable(page, "/");
  }
  await expect(page.getByRole("heading", { level: 1, name: "Flashcards" })).toBeVisible();
  await expect(page.getByRole("heading", { level: 2, name: "Decks" })).toBeVisible();

  const defaultLink = page.getByRole("link", { name: "Default", exact: true });
  await expect(defaultLink).toBeVisible();
  await defaultLink.click();
  await expect(page.getByRole("heading", { level: 1, name: "Default" })).toBeVisible();
  await expect(page.getByRole("heading", { level: 2, name: "Cards" })).toBeVisible();

  await createCard(page, FRONT, BACK);

  await page.getByRole("link", { name: "Study" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Study" })).toBeVisible();
  await expect(page.locator("#study .card-front", { hasText: FRONT })).toBeVisible();

  await allowHttpCookies(page);
  await page.getByRole("button", { name: "Show answer" }).click();
  await expect(page.locator("#study .card-back", { hasText: BACK })).toBeVisible();
  await expect(page.getByRole("button", { name: /Good/ })).toBeVisible();

  await allowHttpCookies(page);
  await page.getByRole("button", { name: /Good/ }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Study" })).toBeVisible();
  await expect(page.getByText("No Cards to Review.")).toBeVisible();

  await gotoReachable(page, "/about");
  await expect(page.getByRole("heading", { level: 1, name: "Flashcards" })).toBeVisible();
  await expect(page.getByText(/Version/)).toBeVisible();
  await expect(page.getByText(/Commit/)).toBeVisible();

  expect(serverErrors(), "no 5xx during the smoke path").toEqual([]);
});

function trackServerErrors(page: Page): () => string[] {
  const errors: string[] = [];
  page.on("response", (response: Response) => {
    if (response.status() >= 500) {
      errors.push(`${response.status()} ${response.request().method()} ${response.url()}`);
    }
  });
  return () => errors;
}

async function gotoReachable(page: Page, path: string): Promise<void> {
  let response: Response | null;
  try {
    response = await page.goto(path);
  } catch (error) {
    throw new Error(
      `Failed to open ${baseURL}${path}. Start the flashcards binary first ` +
        `(see e2e/README.md). Empty-DB boot must stay up (bootstrap wall, not process exit). ${String(error)}`,
      { cause: error },
    );
  }
  expect(response, `${path} should respond`).toBeTruthy();
  expect(response!.status(), `${path} should not 5xx`).toBeLessThan(500);
  await allowHttpCookies(page);
}

/**
 * No-op when cookies already omit Secure (loopback HTTP default). If Secure
 * is still set, Lightpanda will not send those cookies on http:// POSTs —
 * Playwright still exposes them, so copy without Secure.
 */
async function allowHttpCookies(page: Page): Promise<void> {
  const cookies = await page.context().cookies();
  const secure = cookies.filter((cookie) => cookie.secure);
  if (secure.length === 0) {
    return;
  }
  await page.context().addCookies(
    secure.map((cookie) => ({
      name: cookie.name,
      value: cookie.value,
      url: baseURL,
      httpOnly: cookie.httpOnly,
      secure: false,
      sameSite: cookie.sameSite,
    })),
  );
}

async function waitForHtmx(page: Page): Promise<void> {
  await page.waitForFunction(() => "htmx" in window, { timeout: 10_000 });
}

async function submitAuthForm(
  page: Page,
  buttonName: string,
  username: string,
  password: string,
): Promise<void> {
  await allowHttpCookies(page);
  const form = page.locator("form.auth-form");
  await form.getByLabel("Username").fill(username);
  await form.getByLabel("Password").fill(password);
  await form.getByRole("button", { name: buttonName }).click();
  await allowHttpCookies(page);
}

async function createCard(page: Page, front: string, back: string): Promise<void> {
  await allowHttpCookies(page);
  const form = page.locator("form.create-card");
  await form.getByLabel("Front").fill(front);
  await form.getByLabel("Back").fill(back);
  await waitForHtmx(page);
  await form.getByRole("button", { name: "Create Card" }).click();

  await expect(
    page.locator(".card-list .card-front", { hasText: front }),
    "Create Card should add the front to the list",
  ).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(".card-list .card-back", { hasText: back })).toBeVisible();
}
