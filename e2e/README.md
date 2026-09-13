# E2e harness (Playwright + Lightpanda)

Minimal Playwright TypeScript harness. Tests attach to a **already running** Lightpanda CDP server with `chromium.connectOverCDP`. This tree is the smoke home; the product happy path lands in a follow-up. CI wiring (`ci.yml` `e2e` job) is also a follow-up.

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
| `LIGHTPANDA_SHA256` | (0.4.0 assets baked in) | Required checksum if you override the version |

`FLASHCARDS_BIND` / `FLASHCARDS_DB` are process env for the binary (see the repo root README). Point `E2E_BASE_URL` at the listen address you chose.

## Local run (binary + Lightpanda)

From the repo root, three processes: the app, Lightpanda, then Playwright.

```bash
# 1. App
cargo build -p flashcards
FLASHCARDS_DB="$(mktemp -d)/flashcards.db" FLASHCARDS_BIND=127.0.0.1:3000 \
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

Today the only spec is a CDP attach stub (`tests/harness.spec.ts`). It does not need the flashcards process. Keep the binary running when you add the smoke path so `E2E_BASE_URL` resolves.

## Lightpanda only (harness check)

```bash
cd e2e
PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci
./scripts/install-lightpanda.sh
./.lightpanda/lightpanda serve --host 127.0.0.1 --port 9222
# other terminal:
npx playwright test
```

## CDP notes

Lightpanda’s Playwright path is `chromium.connectOverCDP` ([quickstart](https://lightpanda.io/docs/quickstart)). Start CDP with:

```bash
./.lightpanda/lightpanda serve --host 127.0.0.1 --port 9222
```

Then either default `CDP_URL=http://127.0.0.1:9222` (Playwright reads `/json/version`) or `CDP_URL=ws://127.0.0.1:9222` (same as the Lightpanda example).
