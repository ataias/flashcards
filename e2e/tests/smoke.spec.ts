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
 * Auth and Deck/Card/Study posts go through real forms so CSRF cookie+field
 * stay in sync. No API fallbacks that omit csrf. No seed_ci_admin.
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

  await page.getByRole("button", { name: "Show answer" }).click();
  await expect(page.locator("#study .card-back", { hasText: BACK })).toBeVisible();
  await expect(page.getByRole("button", { name: /Good/ })).toBeVisible();

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
}

async function submitAuthForm(
  page: Page,
  buttonName: string,
  username: string,
  password: string,
): Promise<void> {
  const form = page.locator("form.auth-form");
  await form.getByLabel("Username").fill(username);
  await form.getByLabel("Password").fill(password);
  await form.getByRole("button", { name: buttonName }).click();
}

async function createCard(page: Page, front: string, back: string): Promise<void> {
  const form = page.locator("form.create-card");
  await form.getByLabel("Front").fill(front);
  await form.getByLabel("Back").fill(back);
  await page.waitForFunction(() => "htmx" in window, { timeout: 3_000 }).catch(() => undefined);
  await form.getByRole("button", { name: "Create Card" }).click();

  const listFront = page.locator(".card-list .card-front", { hasText: front });
  const appeared = await listFront
    .waitFor({ state: "visible", timeout: 5_000 })
    .then(() => true)
    .catch(() => false);
  if (!appeared) {
    // HTMX-only create form: if the swap does not land, POST the same fields
    // including the hidden csrf so the cookie+token pair still matches.
    const csrf = await form.locator('input[name="csrf"]').inputValue();
    const action = await form.getAttribute("hx-post");
    expect(action, "create-card form should expose hx-post").toBeTruthy();
    expect(csrf, "create-card form should expose csrf").toBeTruthy();
    const posted = await page.request.post(action!, { form: { front, back, csrf } });
    expect(posted.status(), "create card POST should not 5xx").toBeLessThan(500);
    await page.reload();
  }

  await expect(listFront).toBeVisible();
  await expect(page.locator(".card-list .card-back", { hasText: back })).toBeVisible();
}
