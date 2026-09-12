# Flashcards v1.1 — Architecture

Status: **accepted** (grilled 2026-09-12; parent [#36](https://github.com/ataias/flashcards/issues/36)). Implementation not started until explicitly triggered.

Domain language: [`CONTEXT.md`](../CONTEXT.md). Decisions: [`adr/0003-deep-modules-store.md`](adr/0003-deep-modules-store.md), [`adr/0004-default-deck-seed-only.md`](adr/0004-default-deck-seed-only.md).

## Goal

Deep use-case façade + one persistence port so HTTP never imports SQLite, without per-entity repository ceremony.

## Product delta vs v1

| Topic | v1 | v1.1 |
| --- | --- | --- |
| Default Deck | Seed on init **and** recreate if last Deck deleted | Seed on **first DB init only**; empty Deck list allowed |
| Home with zero Decks | N/A (always ≥1) | Empty list + create-Deck form |
| Missing `/decks/{id}` | 404 | 404 (unchanged) |

All other v1 product behavior (FSRS ≥1 day, 20 new/day, Review log, routes) stays unless noted in child issues.

## Packages

```
crates/
  domain/       # types, Card::apply_rating, queue/home policy, use cases, Store, Error
  db/           # SQLx Store impl, migrations
  web/          # Axum + Askama **library** (depends on domain only)
  flashcards/   # thin **binary**: pool → Store → router (workspace default-member)
```

Dependency rule: `web` → `domain`; `db` → `domain`; `flashcards` → `web` + `db` (+ `domain` as needed). Handlers must not `use db::`.

## Façade (what `web` calls)

Use-case oriented API, for example:

- `list_home` / deck CRUD / card CRUD
- `next_study_card` (compose Store load + `select_study_queue`)
- rate path: load card → `Card::apply_rating` → `Store::commit_review`
- `reveal` stays in `web` (presentational only)

Home due/new caps: same rule as Study — Store loads inputs; domain applies the 20/day cap.

## Store

One trait in `domain` covering persistence the use cases need (including `commit_review` for atomic Card update + Review log). No `DeckStore` / `CardStore` split unless a later grill forces it.

## Errors

`domain::Error` at the façade; `web` maps to HTTP; `db` maps `sqlx` at the Store impl boundary.

## Out of scope (v1.1)

- Swapping SQLite / alternate Store backends beyond the trait seam
- Per-entity repository traits
- Generic unit-of-work / `transaction(|tx| …)` API
- Rewriting study-queue selection into SQL for performance
- Sub-day FSRS (#21), markdown (v2)

## Acceptance criteria

1. `cargo run` from repo root runs `crates/flashcards` and serves the app as today.
2. `crates/web` does not depend on `db` in Cargo.toml.
3. Handlers go through domain use cases / `Store`; no `SqlitePool` in handler signatures.
4. `Card::apply_rating` (+ Review log entry) lives in domain; `Store::commit_review` persists atomically.
5. Fresh DB still seeds `Default`; deleting the last Deck leaves zero Decks; home shows empty + create.
6. Study / home new-card cap policy remains in domain.
7. Existing CI gates still pass (`fmt`, `clippy -D warnings`, `test`, `build`, relative lychee).
