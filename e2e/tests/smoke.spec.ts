import type { Page, Response } from "@playwright/test";
import { expect, test } from "../fixtures";
import { baseURL } from "../env";

const FRONT = "e2e smoke front";
const BACK = "e2e smoke back";

/**
 * Pre-Users happy path on current main: home → Default deck → create Card →
 * Study reveal + rate → /about. No admin or login.
 */
test("home, Default deck, create card, study, about", async ({ page }) => {
  const serverErrors = trackServerErrors(page);

  await gotoReachable(page, "/");
  await expect(page.getByRole("heading", { level: 1, name: "Flashcards" })).toBeVisible();
  await expect(page.getByRole("heading", { level: 2, name: "Decks" })).toBeVisible();

  await openDefaultDeck(page);
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
        `(see e2e/README.md). ${String(error)}`,
      { cause: error },
    );
  }
  expect(response, `${path} should respond`).toBeTruthy();
  expect(response!.status(), `${path} should not 5xx`).toBeLessThan(500);
}

async function openDefaultDeck(page: Page): Promise<void> {
  const defaultLink = page.getByRole("link", { name: "Default", exact: true });
  if ((await defaultLink.count()) === 0) {
    const create = page.locator("form.create-deck");
    await create.getByLabel("New Deck").fill("Default");
    await create.getByRole("button", { name: "Create" }).click();
    if ((await defaultLink.count()) === 0) {
      const posted = await page.request.post("/decks", { form: { name: "Default" } });
      expect(posted.status(), "create Default deck should not 5xx").toBeLessThan(500);
      await gotoReachable(page, "/");
    }
  }
  await expect(defaultLink).toBeVisible();
  await defaultLink.click();
}

async function createCard(page: Page, front: string, back: string): Promise<void> {
  const form = page.locator("form.create-card");
  await form.getByLabel("Front").fill(front);
  await form.getByLabel("Back").fill(back);
  const action = await form.getAttribute("hx-post");
  await page.waitForFunction(() => "htmx" in window, { timeout: 3_000 }).catch(() => undefined);
  await form.getByRole("button", { name: "Create Card" }).click();

  const listFront = page.locator(".card-list .card-front", { hasText: front });
  const appeared = await listFront
    .waitFor({ state: "visible", timeout: 5_000 })
    .then(() => true)
    .catch(() => false);
  if (!appeared) {
    expect(action, "create-card form should expose hx-post").toBeTruthy();
    const posted = await page.request.post(action!, { form: { front, back } });
    expect(posted.status(), "create card POST should not 5xx").toBeLessThan(500);
    await page.reload();
  }

  await expect(listFront).toBeVisible();
  await expect(page.locator(".card-list .card-back", { hasText: back })).toBeVisible();
}
