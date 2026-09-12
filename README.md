# Flashcards

Simple Anki-like flashcards: **Rust** backend + **HTMX** UI. Single-user, local, FSRS scheduling.

Repo: https://github.com/ataias/flashcards

## Status

v1 is implemented: Decks, Cards, and a per-Deck Study / Review loop (Again / Hard / Good / Easy).

- [`CONTEXT.md`](CONTEXT.md) — domain language
- [`docs/SPEC-v1.md`](docs/SPEC-v1.md) — accepted v1 product/spec
- [`docs/SPEC-ci.md`](docs/SPEC-ci.md) — accepted CI spec
- [`docs/adr/`](docs/adr/) — architecture decisions
- [`.cursor/skills/do-work/SKILL.md`](.cursor/skills/do-work/SKILL.md) — how Full Stack implements issues / PRs

## Prerequisites

[Rust](https://www.rust-lang.org/learn/get-started) **1.88** via [rustup](https://rustup.rs/). This repo pins that channel in [`rust-toolchain.toml`](rust-toolchain.toml) (`rustfmt` + `clippy` included). rustup installs it on first `cargo` in the tree.

## Run

From the repo root (`crates/web` is the workspace default member, so both commands start the same binary):

```bash
cargo run
# equivalent: cargo run -p web
# open http://127.0.0.1:3000
```

First start creates `./data/flashcards.db` and a Deck named `Default`.

| Variable | Default | Meaning |
| --- | --- | --- |
| `FLASHCARDS_DB` | `./data/flashcards.db` | SQLite path (`data/` is created if missing) |
| `FLASHCARDS_BIND` | `127.0.0.1:3000` | Listen address (e.g. `0.0.0.0:3000`) |

## Use

1. Home (`/`) lists Decks with due / new counts. Create, rename, or delete Decks. Deleting the last Deck recreates `Default`.
2. Open a Deck to create, edit, or delete plain-text Cards (front / back).
3. Study a Deck: front → Show answer → Again / Hard / Good / Easy. The queue is due Cards plus up to 20 New Cards per local calendar day.

The UI is offline (vendored HTMX + CSS; no CDN).

## Stack

Axum · Askama · HTMX (vendored) · SQLx / SQLite · [`fsrs`](https://crates.io/crates/fsrs)

Workspace: `crates/web` (binary), `crates/domain`, `crates/db`.

## Implementing

Full Stack must follow **do-work**: one child issue per PR, pre-PR cargo/lychee gates, Code Reviewer then Ataias approval, stacking allowed after Code Reviewer approves unless the ticket needs `main`.
