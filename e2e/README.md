# E2e harness (Playwright + Lightpanda)

Minimal Playwright TypeScript harness. Tests attach to a **already running** Lightpanda CDP server with `chromium.connectOverCDP`. This tree is the smoke home (pre-Users study path in `tests/smoke.spec.ts`). CI runs the same path in the `e2e` job in `.github/workflows/ci.yml` (real binary, temp DB, pinned Lightpanda). That job is a required check; Ataias must add `e2e` to branch protection when it exists on `main`.

Pins (bump together when upgrading):

| Piece | Version |
| --- | --- |
| Playwright (`@playwright/test`) | `1.63.0` |
| Lightpanda | [`0.4.0`](https://github.com/lightpanda-io/browser/releases/tag/0.4.0) |

Do **not** run `npx playwright install`. Chromium is unused; Lightpanda is the browser.

## Env

| Variable | Default | Meaning |
| --- | --- | --- |
| `E2E_BASE_URL` | `http://127.0.0.1:3000` | HTTP origin of the `flashcards` binary (`use.baseURL`) |
| `CDP_URL` | `http://127.0.0.1:9222` | Lightpanda CDP endpoint (`http://` or `ws://`) |
| `LIGHTPANDA_VERSION` | `0.4.0` | Override for `scripts/install-lightpanda.sh` |
| `LIGHTPANDA_DIR` | `e2e/.lightpanda` | Install directory for the binary |
| `LIGHTPANDA_SHA256` | (0.4.0 assets baked in) | Required if you override the version; the install script exits 1 without it |
| `FLASHCARDS_SEED_BIN` | `target/debug/seed_ci_admin` | Stack-only CI harness helper (not product behavior) |

`FLASHCARDS_BIND` / `FLASHCARDS_DB` are process env for the binary (see the repo root README). Point `E2E_BASE_URL` at the listen address you chose.

`run-ci.sh` runs `seed_ci_admin` on the temp DB **before** starting the binary (`e2e` / `e2e-secret`, via `domain::bootstrap_admin` + `SqliteStore`). This is a **stack-only CI harness** for #96 — not product behavior. Production `main` stays fail-closed on an empty User table. **Do not merge #96 alone**; merge bottom-up through #98 so production never ships empty-DB-exit without bootstrap UI. **#98 must drop this pre-start seed** and keep bootstrap → login smoke.

## Local run (binary + Lightpanda)

From the repo root, three processes: the app, Lightpanda, then Playwright.

```bash
# 1. App (empty DB: seed first — the binary does not create an admin)
cargo build -p flashcards
DB="$(mktemp -d)/flashcards.db"
./target/debug/seed_ci_admin "$DB"
FLASHCARDS_DB="$DB" FLASHCARDS_BIND=127.0.0.1:3000 \
  ./target/debug/flashcards
```

```bash
# 2. Lightpanda CDP (other terminal)
cd e2e
PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci
./scripts/install-lightpanda.sh
./.lightpanda/lightpanda serve --host 127.0.0.1 --port 9222
```

```bash
# 3. Playwright (other terminal)
cd e2e
# defaults: E2E_BASE_URL=http://127.0.0.1:3000  CDP_URL=http://127.0.0.1:9222
npx playwright test
```

Specs:

- `tests/harness.spec.ts` — CDP attach stub (Lightpanda only; no flashcards process).
- `tests/smoke.spec.ts` — pre-Users happy path: `/` → Default deck → create Card → Study reveal + rate → `/about`. Needs the binary at `E2E_BASE_URL` (empty DB: run `seed_ci_admin` first).

## CI-equivalent local run

Same orchestration as the `e2e` job (temp DB, ephemeral ports, teardown):

```bash
cargo build -p flashcards
cd e2e
PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci
./scripts/install-lightpanda.sh
cd ..
./e2e/scripts/run-ci.sh
```

## Lightpanda only (harness check)

```bash
cd e2e
PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci
./scripts/install-lightpanda.sh
./.lightpanda/lightpanda serve --host 127.0.0.1 --port 9222
# other terminal (CDP attach only; smoke needs the binary):
npx playwright test tests/harness.spec.ts
```

## CDP notes

Lightpanda’s Playwright path is `chromium.connectOverCDP` ([quickstart](https://lightpanda.io/docs/quickstart)). Start CDP with:

```bash
./.lightpanda/lightpanda serve --host 127.0.0.1 --port 9222
```

Then either default `CDP_URL=http://127.0.0.1:9222` (Playwright reads `/json/version`) or `CDP_URL=ws://127.0.0.1:9222` (same as the Lightpanda example).
