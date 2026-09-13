# Flashcards

Simple Anki-like flashcards: **Rust** backend + **HTMX** UI. Multi-user (private Decks per User), local SQLite, FSRS scheduling.

Repo: https://github.com/ataias/flashcards

## Status

v1, v1.1, and v1.2 are on `main`. **v1.3** (Users + private spaces + login) is specified; implementation tracked from the Users parent issue.

- [`CONTEXT.md`](CONTEXT.md) — domain language
- [`docs/SPEC-v1.md`](docs/SPEC-v1.md) — completed v1 product/spec
- [`docs/SPEC-v1.1.md`](docs/SPEC-v1.1.md) — accepted v1.1 architecture
- [`docs/SPEC-v1.2.md`](docs/SPEC-v1.2.md) — accepted v1.2 learning steps
- [`docs/SPEC-v1.3.md`](docs/SPEC-v1.3.md) — accepted v1.3 Users / private spaces
- [`docs/SPEC-ci.md`](docs/SPEC-ci.md) — accepted CI spec
- [`docs/adr/`](docs/adr/) — architecture decisions
- [`.cursor/skills/do-work/SKILL.md`](.cursor/skills/do-work/SKILL.md) — how Full Stack implements issues / PRs

## Prerequisites

[Rust](https://www.rust-lang.org/learn/get-started) via [rustup](https://rustup.rs/). This repo pins the channel in [`rust-toolchain.toml`](rust-toolchain.toml) (`rustfmt` + `clippy` included). rustup installs it on first `cargo` in the tree.

## Run

From the repo root (`crates/flashcards` is the workspace default member):

```bash
cargo run
# open http://127.0.0.1:3000
```

First start creates `./data/flashcards.db`. With v1.3, an empty User table shows a one-time bootstrap admin form; that admin (and each later User) gets a Deck named `Default`.

| Variable | Default | Meaning |
| --- | --- | --- |
| `FLASHCARDS_DB` | `./data/flashcards.db` | SQLite path (`data/` is created if missing) |
| `FLASHCARDS_BIND` | `127.0.0.1:3000` | Listen address (e.g. `0.0.0.0:3000`) |

`GET /about` shows the Cargo package version, git commit, and **uncompressed** image size (the packed filesystem for that arch — not the compressed GHCR download size). `/about` stays reachable without login.

Published images write small files at pack time under `crates/web/pack/` (`git-sha`, optional `git-tag`, and `image-size-bytes` from `deploy/Containerfile`). `/about` reads those at runtime so the release binary is not rebuilt just to stamp identity. Local `cargo run` / CI without those files show `unknown` (size is omitted as `unknown`). Optional compile-time fallback: set `FLASHCARDS_GIT_SHA` (or `GITHUB_SHA`) when running `cargo build` / `cargo run`.

## Deploy

The root [`Containerfile`](Containerfile) is the **CI** toolchain image (fmt, clippy, lychee). The runtime app image is [`deploy/Containerfile`](deploy/Containerfile): it **packs a prebuilt binary** (no `cargo` in that image build).

[`.github/workflows/publish-image.yml`](.github/workflows/publish-image.yml) publishes a **multi-arch** image (`linux/amd64` and `linux/arm64`) to `ghcr.io/ataias/flashcards` on push to `main` and on `workflow_dispatch` from `main`. Tags: `latest` on `main`, plus the short commit SHA (for example `a1b2c3d`). Both tags are multi-arch manifests, not amd64-only images. The publish job writes `crates/web/pack/git-sha` (and `git-tag` on a tag build); `deploy/Containerfile` COPYs those files and records uncompressed per-arch size. `/about` in the image reads them at runtime.

Rust is compiled **natively** on GitHub-hosted runners (`ubuntu-latest` → amd64, `ubuntu-24.04-arm` → arm64) inside `rust:1.98.1-bookworm`, then Buildx only COPY-packs the binaries into `debian:bookworm-slim`. Publish **does not compile Rust under QEMU**. Each platform is packed with an explicit `--platform` / `TARGETARCH`, then `docker buildx imagetools create` writes the multi-arch tags.

CI verifies architecture before the image is published: [`scripts/assert-deploy-bin-arch.sh`](scripts/assert-deploy-bin-arch.sh) checks each downloaded binary with `file` and `readelf -h` (amd64 must be ELF x86-64, arm64 must be ELF AArch64); `deploy/Containerfile` runs `file` on `/usr/local/bin/flashcards` and fails the pack if it does not match `TARGETARCH`; after push, `docker buildx imagetools inspect` must list both `linux/amd64` and `linux/arm64`.

Confirm architectures after publish:

```bash
docker buildx imagetools inspect ghcr.io/ataias/flashcards:latest
```

```bash
docker pull ghcr.io/ataias/flashcards:latest
docker run --rm -p 3000:3000 -v flashcards-data:/data ghcr.io/ataias/flashcards:latest
# open http://127.0.0.1:3000
```

| Variable | Image default | Meaning |
| --- | --- | --- |
| `FLASHCARDS_BIND` | `0.0.0.0:3000` | Listen address (all interfaces so the published port works) |
| `FLASHCARDS_DB` | `/data/flashcards.db` | SQLite path (mount a volume on `/data`) |

Build locally from the repo root. Compile at `/src` so ServeDir’s baked `CARGO_MANIFEST_DIR` matches the image, then pack (`amd64` on x86_64 hosts; use `deploy/bin/arm64` on aarch64):

```bash
docker run --rm -v "$PWD":/src -w /src \
  rust:1.98.1-bookworm \
  bash -lc 'apt-get update && apt-get install -y --no-install-recommends libsqlite3-dev pkg-config && cargo build --release --locked --bin flashcards'
mkdir -p deploy/bin/amd64 crates/web/pack
cp target/release/flashcards deploy/bin/amd64/flashcards
git rev-parse HEAD > crates/web/pack/git-sha
docker build -f deploy/Containerfile --build-arg TARGETARCH=amd64 -t flashcards:local .
docker run --rm -p 3000:3000 -v flashcards-data:/data flashcards:local
```

## Use

1. First visit on an empty DB: create the bootstrap admin, then log in.
2. Home (`/`) lists **your** Decks with due / new counts. Create, rename, or delete Decks. With no Decks, home shows an empty list and a create-Deck form.
3. Open a Deck to create, edit, or delete plain-text Cards (front / back).
4. Study a Deck: front → Show answer → Again / Hard / Good / Easy. The queue is due Cards plus up to 20 New Cards per local calendar day.
5. `/about` shows the package version, git commit, and uncompressed image size when pack-time files are present (public; not linked from home).
6. Admin (bootstrap only): manage Users (create / disable / delete / reset password).

The UI is offline (vendored HTMX + CSS; no CDN).

## Stack

Axum · Askama · HTMX (vendored) · SQLx / SQLite · [`fsrs`](https://crates.io/crates/fsrs) · Argon2id

Workspace: `crates/flashcards` (binary), `crates/web` (library), `crates/domain`, `crates/db`.

## Implementing

Full Stack must follow **do-work**: one child issue per PR, pre-PR cargo/lychee gates, Code Reviewer then Ataias approval, stacking allowed after Code Reviewer approves unless the ticket needs `main`.
