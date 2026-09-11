# Flashcards

Simple Anki-like flashcards: **Rust** backend + **HTMX** UI. Single-user, local, FSRS scheduling.

Repo: https://github.com/ataias/flashcards

## Status

v1 is **specified**, not implemented yet. See:

- [`CONTEXT.md`](CONTEXT.md) — domain language
- [`docs/SPEC-v1.md`](docs/SPEC-v1.md) — accepted v1 spec
- [`docs/adr/`](docs/adr/) — architecture decisions

## Planned stack

Axum · Askama · HTMX (vendored) · SQLx / SQLite · [`fsrs`](https://crates.io/crates/fsrs)

Workspace (planned): `crates/web`, `crates/domain`, `crates/db`.

## Run (once implemented)

```bash
cargo run -p web
# open http://127.0.0.1:3000
```

Optional:

- `FLASHCARDS_DB` — SQLite path (default `./data/flashcards.db`)
- `FLASHCARDS_BIND` — listen address (default `127.0.0.1:3000`)
