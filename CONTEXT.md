# Flashcards

A multi-user Anki-like study app: each User has a private space of Decks and Cards, reviews what's due, and reschedules by Rating. Stores everything in a local SQLite file.

## Language

**User**:
An account that owns a private space of Decks and Cards. Identified by a unique username (case-insensitive) and authenticates with a password (Argon2id hash only). May be disabled (login blocked, sessions wiped). Exactly one bootstrap admin exists in v1.3 (no promote UI); that admin can create, disable, delete other Users and reset their passwords.
_Avoid_: Account (as the primary type name), member, profile

**Session**:
A server-side login record in SQLite, referenced by an HTTP-only Secure SameSite=Strict cookie. Sliding idle expiry (30 days). Many Sessions per User are allowed. Logout may end this Session or all Sessions; password change, admin password reset, and disable wipe all of that User's Sessions.
_Avoid_: JWT (as the primary mechanism), token (ambiguous)

**Card**:
A flashcard with a front (prompt) and a back (answer). Plain text only until markdown (v2 grill). Each Card belongs to a Deck (hence to one User). Each Card has a **phase**, optional learning step index, FSRS memory (when in Review / after lapse), and a due time. Scheduling policy lives in domain (`Card::apply_rating` / phase transitions); persistence only stores the result. (Future Courses may copy Cards into a subscriber's space with a source link — not v1.3.)
_Avoid_: Note, item, flashcard (as a type name)

**Deck**:
A named collection of Cards owned by one User. When a User is created, a Deck named "Default" is seeded for that User; afterward zero Decks is allowed for that User (no recreate-on-last-delete); the User creates a Deck before adding Cards or studying. Missing or other-User Deck ids in the UI are 404.
_Avoid_: Folder, set, pile, collection

**Phase**:
Where a Card sits in the scheduler: **New**, **Learning**, **Review**, or **Relearning**. New has never left New via a Rating. Learning and Relearning use fixed minute steps; Review uses unfloored FSRS.
_Avoid_: State, status, queue position

**Learning**:
The short-term step ladder after leaving New (defaults: 1 minute, then 10 minutes). The first Rating on a New Card uses Learning rules at step 0. Again restarts at the first step; Hard repeats the current step; Good advances (graduates on the last step); Easy graduates early into Review with FSRS Easy.
_Avoid_: Drill, cram

**Relearning**:
Step ladder after Again on a Review Card (default: 10 minutes). Same button rules as Learning. FSRS Again updates memory immediately when entering Relearning; steps only gate when the Card is due again.
_Avoid_: Lapse queue (as the primary noun)

**Study**:
A session drawn from one of the current User's Decks: Cards with due ≤ now (any phase), plus New Cards under the daily cap (20/day per Deck; day = local midnight). Queue policy lives in domain. A separate Anki-like “learn mode” UI is deferred (future grill #55).
_Avoid_: Study all, quiz mode, session (as the primary noun for this act)

**New Card**:
A Card in phase New. The first Rating leaves New using Learning rules at step 0 and consumes one slot of the Deck’s daily new-card cap (including Easy that graduates straight to Review); further Learning steps that day do not consume another slot.
_Avoid_: Unseen, unseen card, freshman

**Review**:
(1) The phase for long-term FSRS scheduling. (2) One attempt to recall a Card's back: show front, reveal back, then a Rating. No typed answer.
_Avoid_: Study, quiz, attempt (as the noun for this act)

**Review log**:
An append-only record of every Rating (including Learning/Relearning steps): card, timestamp, rating. Written with the Card update via `Store::commit_review`. Rows belong to the Card (and thus the owning User).
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
The single persistence port in `domain` (one trait, not per-entity repositories). Includes Deck/Card/study ports and User/Session ports. Implemented by `db` (SQLx/SQLite). Use cases load through Store and call domain policy; HTTP never imports SQLite.
_Avoid_: Repository (as the primary noun), Dao, Unit of Work
