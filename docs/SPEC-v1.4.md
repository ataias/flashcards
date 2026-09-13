# Flashcards v1.4 — Courses

Status: **accepted** (grilled 2026-09-13). Docs/tickets now; **do not implement until v1.3 Users is done** and Ataias triggers.

Domain language: [`CONTEXT.md`](../CONTEXT.md). Decision: [`adr/0007-courses-copy-subscribe.md`](adr/0007-courses-copy-subscribe.md). Builds on [`SPEC-v1.3.md`](SPEC-v1.3.md) and [`adr/0006-private-user-spaces.md`](adr/0006-private-user-spaces.md).

## Goal

Add Courses: admin-authored curriculum, User subscribe via catalog, copy-into-private-space study with living content sync, skip, and version diffs — without a separate per-User progress table.

## Product delta vs v1.3

| Topic | Before | v1.4 |
| --- | --- | --- |
| Content | Private Decks only | Course-owned Decks/Cards + subscriber copies |
| Discovery | N/A | `/courses` catalog; self-subscribe |
| Home | Private Decks list | Private Decks + separate Courses section |
| Updates | N/A | Flag + optional apply; diff vs last-Review snapshot |
| Skip | N/A | Lasting suspend on Course copies |

## Course & admin

- Course: title + description; Course-owned Decks/Cards (plain front/back CRUD, same forms as private).
- Only bootstrap **admin** creates/edits/archives Courses (roles → #101).
- Soft-**archive**: hide from catalog; existing Subscriptions keep syncing until unsubscribe.
- Richer authoring → #99; drip scheduling → #100; content suggestions → #102.

## Subscribe & copies

- Any logged-in User browses `/courses` and subscribes.
- Subscribe creates **one** Deck named for the Course under the User, flattening all Course Cards into it (source link per Card). Scheduling starts **New**.
- Home: subscribed Decks in a **Courses** section (not mixed with private Decks).
- Living sync: new source Cards → new copies (New). Removed source Cards → **retired** on the copy; User may keep or delete later.
- Content on copies is **read-only** except applying Course updates.

## Updates & diffs

- On each Rating of a Course copy, store front/back **snapshot**.
- When source front/back changes: mark pending update. Study still allowed on old text; banner offers apply.
- If the copy was previously reviewed: show **diff** vs last-Review snapshot before/with apply.
- If still New (never reviewed): apply without diff.
- Apply updates front/back only; scheduling unchanged.

## Skip

- “I already know this” → lasting **skip/suspend** (not due); reversible from the Deck list. Not a one-day bury; not delete.

## Unsubscribe

- User chooses: **keep** Deck as private (break source links, no more sync) or **delete** Deck. UI default: keep.

## Study

- Same queue rules as private Decks; skipped and retired Cards excluded until un-skipped / resolved.

## Out of scope (v1.4)

- Drip / scheduled unlock (#100)
- Deep authoring UX (#99)
- Roles beyond bootstrap admin (#101)
- Content suggestions (#102)
- Learn mode (#55)

## Acceptance criteria

1. Admin can CRUD Course metadata and Course Decks/Cards; catalog lists non-archived Courses.
2. User subscribes → one Course Deck in Courses section with copies; Study works with New/due rules.
3. Skip removes from queue; un-skip restores.
4. Source edit → pending update; optional apply; diff when previously reviewed; New has no diff; study not blocked.
5. New source Card appears as New copy; removed source → retired with keep/delete choice.
6. Unsubscribe offers keep-as-private or delete.
7. Archive hides from catalog; subscribers keep syncing until unsubscribe.
8. Subscriber cannot edit Course copy front/back (except apply update).
9. `web` still never imports `db`; Course/Subscription via Store.
10. HTTP tests cover subscribe/study/skip/update/unsubscribe/archive happy paths; CI green; do-work applies when triggered.

## Implement note

Parent/children track work. **Full Stack must not start until v1.3 Users (#85) is complete** and Ataias explicitly triggers.
