# Flashcards v1.2 — Learning / Relearning steps

Status: **accepted** (grilled 2026-09-12; parent [#21](https://github.com/ataias/flashcards/issues/21)). Implementation not started until explicitly triggered.

Domain language: [`CONTEXT.md`](../CONTEXT.md). Decisions: [`adr/0005-learning-relearning-steps.md`](adr/0005-learning-relearning-steps.md), updated [`adr/0001-fsrs-scheduling.md`](adr/0001-fsrs-scheduling.md).

## Goal

Add Anki-like Learning and Relearning minute steps; remove the v1 ≥1 day floor for Review-phase FSRS.

## Product delta vs v1 / v1.1

| Topic | Before | v1.2 |
| --- | --- | --- |
| Phases | Implicit New vs “has memory” | Explicit New / Learning / Review / Relearning + step index |
| Again (new / learning) | FSRS then ≥1 day | Learning steps (`1m`, `10m`) |
| Again (review) | FSRS then ≥1 day | FSRS Again on memory, then Relearning (`10m`) |
| Review intervals | Floored to ≥1 day | Unfloored FSRS float-day duration |
| New-card cap | Never-reviewed Cards | Leaving New (first Rating) consumes one of 20/day |
| Review log | Each Rating | Unchanged — still every Rating, including steps |
| Study queue | due + new cap | Same shape; step dues reappear when due ≤ now |

## Leaving New

The first Rating on a New Card is applied with **Learning rules at step index 0** (as if the Card entered Learning at step 0, then that Rating ran). That first Rating **always consumes one new-card slot** for the Deck’s local day — including **Easy**, which may graduate straight into Review without remaining in Learning.

## Button rules (Learning / Relearning)

| Rating | Effect |
| --- | --- |
| Again | Restart at first step; due = now + step[0] |
| Hard | Repeat current step; due = now + step[i] |
| Good | Next step, or graduate on last step |
| Easy | Graduate early into Review using FSRS Easy |

**Graduate into Review:** set phase=Review; schedule with FSRS for the graduating Rating (Good or Easy) using current memory (after lapse, memory already reflects FSRS Again).

## Defaults (hard-coded this pass)

- Learning steps: `1m`, `10m`
- Relearning steps: `10m`
- No in-app settings UI

## Persistence

- Card stores `phase`, `learning_step` (index), existing FSRS fields + `due` / `last_review` as needed
- Migration from v1.1 rows: Cards with no memory → New; Cards with memory → Review (step unused)

## Out of scope (v1.2)

- Settings UI for custom steps
- Separate Anki-like **learn mode** UI (future grill — [#55](https://github.com/ataias/flashcards/issues/55))
- Markdown, FSRS optimizer UI, multi-user

## Acceptance criteria

1. New Card: first Rating uses Learning rules at step 0; consumes one new-card slot even if Easy graduates immediately to Review.
2. Learning Again/Hard/Good/Easy behave as the table; last-step Good and Easy reach Review with unfloored FSRS dues (can be &lt; 1 day).
3. Review Again updates FSRS memory immediately, enters Relearning, due in 10m (default); graduate returns to Review.
4. Study still lists due ≤ now then New under cap; Learning Cards reappear in-session when steps elapse (tests may fake clock).
5. Review log gains a row for every Rating including steps.
6. HTTP/`cargo test` regressions cover the happy paths above.
7. CI green; do-work PR rules apply.
