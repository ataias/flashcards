# Flashcards

A single-user Anki-like study app: create cards, review what's due, reschedule by rating. Stores everything in a local SQLite file.

## Language

**Card**:
A flashcard with a front (prompt) and a back (answer). Plain text only until markdown (v2 grill). Each Card has a **phase**, optional learning step index, FSRS memory (when in Review / after lapse), and a due time. Scheduling policy lives in domain (`Card::apply_rating` / phase transitions); persistence only stores the result.
_Avoid_: Note, item, flashcard (as a type name)

**Deck**:
A named collection of Cards. On first DB init, a Deck named "Default" is created automatically; additional named Decks are allowed. After that, zero Decks is allowed (no recreate-on-last-delete); the user creates a Deck before adding Cards or studying. Missing Deck ids in the UI are 404.
_Avoid_: Folder, set, pile, collection

**Phase**:
Where a Card sits in the scheduler: **New**, **Learning**, **Review**, or **Relearning**. New has never entered Learning. Learning and Relearning use fixed minute steps; Review uses unfloored FSRS.
_Avoid_: State, status, queue position

**Learning**:
The short-term step ladder for a Card that just left New (defaults: 1 minute, then 10 minutes). Again restarts at the first step; Hard repeats the current step; Good advances (graduates on the last step); Easy graduates early into Review with FSRS Easy.
_Avoid_: Drill, cram

**Relearning**:
Step ladder after Again on a Review Card (default: 10 minutes). Same button rules as Learning. FSRS Again updates memory immediately when entering Relearning; steps only gate when the Card is due again.
_Avoid_: Lapse queue (as the primary noun)

**Study**:
A session drawn from one Deck: Cards with due ≤ now (any phase), plus New Cards under the daily cap (20/day per Deck; day = local midnight). Queue policy lives in domain. A separate Anki-like “learn mode” UI is deferred (future grill).
_Avoid_: Study all, quiz mode, session (as the primary noun)

**New Card**:
A Card in phase New. Entering Learning for the first time consumes one slot of the Deck’s daily new-card cap; further Learning steps that day do not.
_Avoid_: Unseen, unseen card, freshman

**Review**:
(1) The phase for long-term FSRS scheduling. (2) One attempt to recall a Card's back: show front, reveal back, then a Rating. No typed answer.
_Avoid_: Study, quiz, attempt (as the noun for this act)

**Review log**:
An append-only record of every Rating (including Learning/Relearning steps): card, timestamp, rating. Written with the Card update via `Store::commit_review`.
_Avoid_: History entry, audit row

**Rating**:
Again, Hard, Good, or Easy. In Review, mapped to FSRS grades; in Learning/Relearning, drives step transitions as above.
_Avoid_: Grade, score, button

**Due**:
A Card is due when its next due time ≤ now. Learning/Relearning dues are step deadlines (minutes); Review dues come from unfloored FSRS intervals (fractional days allowed).
_Avoid_: Ready, overdue (as the primary term)

**Scheduler**:
Hybrid: fixed Learning/Relearning steps, then FSRS (`fsrs` crate, FSRS v6, default parameters) for Review with **no** ≥1 day floor. Not SM-2; not `rs-fsrs`.
_Avoid_: SM-2, Anki algorithm (ambiguous), SRS (generic), rs-fsrs

**Store**:
The single persistence port in `domain` (one trait, not per-entity repositories). Implemented by `db` (SQLx/SQLite). Use cases load through Store and call domain policy; HTTP never imports SQLite.
_Avoid_: Repository (as the primary noun), Dao, Unit of Work
