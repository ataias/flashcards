# Flashcards — CI Spec

Status: **accepted** (grilled 2026-09-11). Implementation deferred until explicitly triggered.

Grounded in [`docs/SPEC-v1.md`](SPEC-v1.md) (Cargo workspace, SQLx/SQLite, local app). Decision record: [`docs/adr/0002-ci-containerfile.md`](adr/0002-ci-containerfile.md).

## Goals

Gate PRs and `main` on format, lint, tests, build, and **relative** doc links. Daily job checks **external** doc links. Reproducible toolchain via a root **Containerfile** + matching `rust-toolchain.toml`.

## Non-goals

- Deploy / release artifacts / coverage gates
- Publishing the CI image to GHCR (may add later if builds are slow)
- Dependabot / `cargo audit` in this pass (deferred)
- Running the HTMX app as a service in CI

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

GitHub Actions **builds** this image in the workflow (layer cache via GHA cache). Jobs run **inside** that image.

## Workflows

### `ci.yml`

**Triggers:** pull_request; push to `main`.

**Hygiene:**

- `permissions: contents: read`
- `concurrency` with cancel-in-progress per PR (or ref)

**Job (single sequential job for v1):**

1. Build image from `Containerfile` (cache layers to GHA cache).
2. Run in that container:
   - `cargo fmt --check`
   - `cargo clippy --workspace -- -D warnings`
   - `cargo test --workspace`
   - `cargo build --workspace`
   - lychee **relative links only** over markdown docs (`README.md`, `CONTEXT.md`, `docs/**`), config from `lychee.toml`
3. Cache Cargo (e.g. `Swatinem/rust-cache`) inside the job.

**SQLx:**

- Compile with `SQLX_OFFLINE=true` and checked-in `.sqlx/`
- Tests use a **temp SQLite** file with migrations applied in setup (not the developer’s `./data/flashcards.db`)

### `links-daily.yml`

**Triggers:** schedule `0 6 * * *` (06:00 UTC); `workflow_dispatch` optional.

**Job:** build/reuse CI image → lychee **external** URLs for the same doc set → **fail on 404** (allow retries/timeouts per lychee config so flaky networks don’t dominate).

## Config files (checked in)

| File | Role |
| --- | --- |
| `Containerfile` | CI image |
| `rust-toolchain.toml` | Same Rust channel/minor as Containerfile |
| `lychee.toml` | Include paths; sensible excludes (e.g. mailto); mode differences documented for PR vs daily |
| `.sqlx/` | Offline query metadata (once code exists) |

## Acceptance criteria

1. PR opened against `main` runs `ci.yml` and must pass fmt, clippy, test, build, relative lychee.
2. Push to `main` runs the same.
3. Daily workflow runs external lychee and fails the job on 404.
4. Changing only docs with a broken relative link fails `ci.yml`.
5. Rust version used in CI matches `rust-toolchain.toml` / Containerfile pin.
6. No CDN or privileged token scopes required for the default CI path.

## Implementer notes

- Until the Cargo workspace exists, cargo steps may be introduced with the scaffold issue; CI issues can land Containerfile + workflows in an order that stays green (e.g. scaffolding first, or temporary `continue-on-error` only if unavoidable — prefer scaffold-first).
- Prefer failing closed: do not weaken `-D warnings` to get green.
