# Flashcards — CI Spec

Status: **accepted** (grilled 2026-09-11; e2e amend grilled 2026-09-13).

Grounded in app SPECs. Decision records: [`docs/adr/0002-ci-containerfile.md`](adr/0002-ci-containerfile.md), [`docs/adr/0009-e2e-playwright-lightpanda.md`](adr/0009-e2e-playwright-lightpanda.md).

## Goals

Gate PRs and `main` on format, lint, tests, build, **relative** doc links, and a **minimal browser e2e smoke** against the real `flashcards` binary. Daily job checks **external** doc links. Reproducible toolchain via a root **Containerfile** + matching `rust-toolchain.toml`.

## Non-goals

- Deploy / release artifacts / coverage gates
- Publishing the CI image to GHCR (may add later if builds are slow)
- Dependabot / `cargo audit` in this pass (deferred)
- Full browser suite / Chromium-on-every-PR (parked; Lightpanda keeps PRs light)
- Running e2e inside the CI Containerfile (Node + Lightpanda stay on the runner job)

## Containerfile (source of truth)

Path: `/Containerfile` (repo root).

| Concern | Choice |
| --- | --- |
| Base | Pinned `rust:<stable-minor>-bookworm` (explicit minor; bump deliberately) |
| Components | `rustfmt`, `clippy` |
| Link checker | **lychee**, version-pinned in the image |
| Native deps | `libsqlite3-dev` (and friends) as needed for SQLx/SQLite linking |
| App code | **Not** baked in — Actions checks out the repo at runtime |
| Local match | `rust-toolchain.toml` at repo root matches the Containerfile Rust pin |

GitHub Actions **builds** this image in the workflow (layer cache via GHA cache). The **checks** job runs **inside** that image.

## Workflows

### `ci.yml`

**Triggers:** pull_request; push to `main`.

**Hygiene:**

- `permissions: contents: read` (plus `actions: write` only if required for GHA cache)
- `concurrency` with cancel-in-progress per PR (or ref)

**Job `checks` (sequential):**

1. Build image from `Containerfile` (cache layers to GHA cache).
2. Run in that container:
   - `cargo fmt --check`
   - `cargo clippy --workspace -- -D warnings`
   - `cargo test --workspace`
   - `cargo build --workspace`
   - lychee **relative links only** over markdown docs (`README.md`, `CONTEXT.md`, `docs/**`), config from `lychee.toml`
3. Cache Cargo (e.g. `Swatinem/rust-cache`) inside the job; fix root-owned paths after `docker run` if needed.

**Job `e2e` (same workflow, required green):**

1. On `ubuntu-latest` (not inside the CI image): install Rust toolchain from `rust-toolchain.toml`, Node (for Playwright), pinned **Lightpanda** binary.
2. `cargo build -p flashcards` (debug OK for speed).
3. Start `flashcards` with `FLASHCARDS_DB` in a temp dir and `FLASHCARDS_BIND=127.0.0.1:<ephemeral-port>`.
4. Start Lightpanda CDP; run Playwright (`e2e/`) via `chromium.connectOverCDP`.
5. **Smoke critical path (must match current product on the PR tip):**
   - **On main before Users merges:** open `/` → Default Deck (or create) → create Card → Study reveal + rate → `/about` → assert success (process stays up; pages not 5xx).
   - **When a PR changes that happy path** (e.g. Users login wall): **the same PR must update** the smoke so it still proves boot → core study. Code Reviewer rejects stale e2e.
6. Tear down server + browser. Fail the job on any step failure.

**SQLx (checks job):**

- Compile with `SQLX_OFFLINE=true` and checked-in `.sqlx/` when used
- Tests use a **temp SQLite** file with migrations applied in setup

### `links-daily.yml`

**Triggers:** schedule `0 6 * * *` (06:00 UTC); `workflow_dispatch` optional.

**Job:** build/reuse CI image → lychee **external** URLs for the same doc set → **fail on 404** (allow retries/timeouts per lychee config so flaky networks don’t dominate).

## Config / tree (checked in)

| Path | Role |
| --- | --- |
| `Containerfile` | CI checks image |
| `rust-toolchain.toml` | Same Rust channel/minor as Containerfile |
| `lychee.toml` | Include paths; sensible excludes |
| `.sqlx/` | Offline query metadata when used |
| `e2e/` | Playwright TypeScript smoke + package metadata; pin Lightpanda version in docs/script |

## Acceptance criteria

1. PR against `main` runs `ci.yml`; **checks** and **e2e** must both pass.
2. Push to `main` runs the same.
3. Daily workflow runs external lychee and fails the job on 404.
4. Broken relative doc link fails `checks`.
5. Rust version in checks matches `rust-toolchain.toml` / Containerfile pin.
6. E2e boots the real binary and completes the smoke critical path against Lightpanda.
7. A PR that introduces login (or otherwise changes the happy path) updates `e2e/` in the **same** PR; Code Reviewer verifies smoke coverage is still sufficient for the minimal path.

## Implementer notes

- Prefer failing closed: do not weaken `-D warnings` or skip e2e to get green.
- Keep the smoke **minimal** — one happy path, not a full suite.
- Pin Lightpanda and Playwright versions; document local run (`cargo build -p flashcards`, start binary, Lightpanda, `npx playwright test`).
