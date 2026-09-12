# Flashcards v1 — Spec

Status: **accepted** (grilled 2026-09-11; Again min-interval clarified 2026-09-11). Implementation largely complete; closeout = parent issues + acceptance smoke.

Domain language lives in [`CONTEXT.md`](../CONTEXT.md). Hard decisions: [`docs/adr/`](adr/).

## Goal

A single-user, local Anki-like study app: create Decks and plain-text Cards, Study a Deck, Review with FSRS (Again / Hard / Good / Easy).

## Out of scope (v1)

- User accounts / auth / sync
- Import / export
- Media, cloze, markdown (markdown → v2 grill after v1)
- Stats charts
- Global “Study all”
- FSRS parameter optimization UI (store Review log so it can come later)
- Sub-day / learning-step intervals (Again floored to ≥1 day; future grill #21)
- Layered DDD seam so `web` does not call `db` directly (v1.1 architecture grill #36)
- Docker / cloud hosting

## Stack

| Layer | Choice |
| --- | --- |
| Runtime | Local `cargo run` binary; browser is UI only |
| HTTP | Axum + Tokio |
| Templates | Askama (full pages + HTMX fragments) |
| Front-end | Vendored `htmx.min.js` + small local CSS in `crates/web/static/` |
| DB | SQLite via SQLx + migrations |
| Scheduler | `fsrs` crate (FSRS v6), default parameters; intervals floored to ≥1 day |

## Workspace

```
flashcards/
  Cargo.toml                 # workspace
  CONTEXT.md
  docs/SPEC-v1.md
  docs/adr/
  crates/
    web/                     # binary: Axum, Askama, static, routes
    domain/                  # Card/Deck/Review/FSRS orchestration
    db/                      # SQLx models, migrations, queries
  data/                      # created at runtime (gitignored)
```

Note: v1 code still has `web → db` for HTTP orchestration; pure FSRS/queue lives in `domain`. Aligning the dependency graph with this layout is **#36** (v1.1), not a v1 blocker.

## Configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| *(none)* | `./data/flashcards.db` | SQLite path (create `data/` if missing) |
| `FLASHCARDS_DB` | — | Override DB path |
| *(none)* | `127.0.0.1:3000` | Listen address |
| `FLASHCARDS_BIND` | — | Override bind (e.g. `0.0.0.0:3000`) |

## Domain behavior

### Deck

- Named collection of Cards.
- On first DB init: create Deck named `Default`.
- Create / rename / delete in UI.
- Delete cascades to Cards (confirm in UI).
- If zero Decks remain after a delete: immediately recreate `Default`.

### Card

- Belongs to one Deck.
- Plain-text `front` and `back` only.
- Create / edit / delete in UI.
- Stores FSRS memory state + next due timestamp.
- **New Card**: never Reviewed; enters Study under the Deck’s daily new cap.

### Study

- Always scoped to one Deck.
- Queue = due Cards for that Deck + up to **20 New Cards** per local calendar day.
- “Day” = local machine timezone midnight (not UTC).

### Review

1. Show front.
2. User taps “Show answer”.
3. Show back + Again / Hard / Good / Easy.
4. Persist updated FSRS state + due; append Review log; HTMX-load next Card (or empty state).

No typed answers in v1. Scheduled due times use FSRS intervals **rounded and floored to at least 1 day** (including Again).

### Review log

Append-only: card id, timestamp, rating. Enables future parameter optimization.

## Suggested routes (indicative)

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/` | Deck list + due/new counts |
| POST | `/decks` | Create Deck |
| POST | `/decks/{id}/rename` | Rename |
| POST | `/decks/{id}/delete` | Delete (+ Default recreate if needed) |
| GET | `/decks/{id}` | Card list for Deck |
| POST | `/decks/{id}/cards` | Create Card |
| POST | `/cards/{id}` | Edit Card |
| POST | `/cards/{id}/delete` | Delete Card |
| GET | `/decks/{id}/study` | Study session (next Card or done) |
| POST | `/cards/{id}/reveal` | HTMX: show back + ratings |
| POST | `/cards/{id}/rate` | Submit Rating; return next Card fragment |

Exact paths may shift during implementation; behavior must not.

## Acceptance criteria (v1 done)

1. `cargo run` from repo root serves the app on `127.0.0.1:3000`.
2. Fresh start creates `data/flashcards.db` and a `Default` Deck.
3. User can manage Decks and Cards as above.
4. Study on a Deck runs the Review flow; ratings change due dates via FSRS (≥1 day intervals).
5. New Cards respect the 20/day local-day cap per Deck.
6. Deleting the last Deck recreates `Default`.
7. `FLASHCARDS_DB` and `FLASHCARDS_BIND` work.
8. App works offline (no CDN).

## Non-goals for implementers

Do not add markdown, auth, sync, import/export, optimizer UI, sub-day learning steps, or the v1.1 architecture refactor in the v1 pass.
