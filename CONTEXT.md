# Flashcards

A single-user Anki-like study app: create cards, review what's due, reschedule by rating. Stores everything in a local SQLite file.

## Language

**Card**:
A flashcard with a front (prompt) and a back (answer). v1 is plain text only; markdown is planned for v2 (to be grilled after v1). Each Card carries FSRS memory state and its next due time. Applying a Rating uses `Card::apply_rating`, which returns an updated Card and a Review log entry — scheduling policy lives in domain, not in persistence.
_Avoid_: Note, item, flashcard (as a type name)

**Deck**:
A named collection of Cards. On first DB init, a Deck named "Default" is created automatically; additional named Decks are allowed. After that, zero Decks is allowed (no recreate-on-last-delete); the user creates a Deck before adding Cards or studying. Missing Deck ids in the UI are 404.
_Avoid_: Folder, set, pile, collection

**Study**:
A session of Reviews drawn from one Deck: due Cards plus a daily cap of new Cards (never reviewed; 20/day per Deck in v1). The new-card day boundary is local midnight. Queue selection policy lives in domain; persistence only loads the inputs.
_Avoid_: Study all, quiz mode, session (as the primary noun)

**New Card**:
A Card that has never been Reviewed. Enters Study under the Deck’s daily new cap.
_Avoid_: Unseen, unseen card, freshman

**Review**:
One attempt to recall a Card's back: show front, reveal back, then a Rating that updates scheduling. No typed answer in v1.
_Avoid_: Study, quiz, attempt (as the noun for this act)

**Review log**:
An append-only record of a Review (card, timestamp, rating) kept for history and future FSRS parameter optimization. Written atomically with the Card update via `Store::commit_review`.
_Avoid_: History entry, audit row

**Rating**:
The outcome of a Review: Again, Hard, Good, or Easy. Mapped into FSRS as the four standard grades.
_Avoid_: Grade, score, button

**Due**:
A Card is due when its next-review time has arrived or passed; Reviews are drawn from due Cards. In v1, scheduled intervals are floored to at least one whole day (no sub-day / learning-step dues).
_Avoid_: Ready, overdue (as the primary term)

**Scheduler**:
FSRS (Free Spaced Repetition Scheduler) via the `fsrs` crate (FSRS v6, default parameters in v1). Not SM-2; not the lighter `rs-fsrs` crate. v1 applies a minimum interval of 1 day after rounding (Again included). Sub-day / learning-step scheduling is deferred to a future grill (issue #21).
_Avoid_: SM-2, Anki algorithm (ambiguous), SRS (generic), rs-fsrs

**Store**:
The single persistence port in `domain` (one trait, not per-entity repositories). Implemented by `db` (SQLx/SQLite). Use cases load through Store and call domain policy (`apply_rating`, study/home caps); HTTP never imports SQLite.
_Avoid_: Repository (as the primary noun), Dao, Unit of Work
