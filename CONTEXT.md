# Flashcards

A multi-user Anki-like study app: each User has a private space of Decks and Cards, can subscribe to Courses, reviews what's due, and reschedules by Rating. Stores everything in a local SQLite file.

## Language

**User**:
An account that owns a private space of Decks and Cards and may hold Subscriptions to Courses. Identified by a unique username (case-insensitive) and authenticates with a password (Argon2id hash only). May be disabled (login blocked, sessions wiped). Exactly one bootstrap admin exists until a roles grill (#101); that admin can manage Users and Courses.
_Avoid_: Account (as the primary type name), member, profile

**Session**:
A server-side login record in SQLite, referenced by an HTTP-only Secure SameSite=Strict cookie. Sliding idle expiry (30 days). Many Sessions per User are allowed. Logout may end this Session or all Sessions; password change, admin password reset, and disable wipe all of that User's Sessions.
_Avoid_: JWT (as the primary mechanism), token (ambiguous)

**Course**:
A named curriculum with a title and short description, owned as catalog content (not a User's private space). Contains Course-owned Decks and Cards that only the bootstrap admin edits (MVP). May be **archived** (hidden from the catalog) while existing Subscriptions keep syncing until unsubscribe. Soft-archive is how admin "deletes" a Course that still has subscribers.
_Avoid_: Class, curriculum (as the type name), lesson pack

**Subscription**:
A User's enrollment in a Course. On subscribe, the app creates one flattened Course Deck in the User's space (all Course Cards copied into it) with source links. Home lists these under a separate Courses section. Unsubscribe asks keep-as-private (break source links) or delete the Deck.
_Avoid_: Enrollment (as the primary noun), join, follow

**Card**:
A flashcard whose front and back are stored as **Markdown** (CommonMark + GFM tables, fenced code blocks, strikethrough; no raw HTML, images, or math in this slice). `web` renders to sanitized HTML with server-side code highlighting and shared light/dark prose CSS; domain/Store keep the raw Markdown strings. Private Cards belong to a User-owned Deck. Subscriber copies belong to the User's Course Deck and may link to a Course source Card (Course diffs compare Markdown source). Each Card has a **phase**, optional learning step index, FSRS memory (when in Review / after lapse), and a due time. Scheduling stays on the Card. Subscriber copies may be **skipped** (suspended, not due) or **retired** (source removed; User may keep or delete). Content on Course copies is read-only except applying Course updates.
_Avoid_: Note, item, flashcard (as a type name), HTML card (as the stored format)

**Deck**:
A named collection of Cards. **Private Decks** are owned by one User (Default seeded on User create; empty list allowed). **Course Decks** on the subscriber side are one flattened Deck per Subscription, listed in the home Courses section. Canonical Course content uses Course-owned Decks (admin Course UI), separate from anyone's private Decks. Missing or inaccessible ids are 404.
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
A session drawn from one Deck (private or Course Deck): Cards with due ≤ now (any phase), plus New Cards under the daily cap (20/day per Deck; day = local midnight). Skipped and retired Cards are excluded. Queue policy lives in domain. Card faces show rendered Markdown. A separate Anki-like “learn mode” UI is deferred (future grill #55).
_Avoid_: Study all, quiz mode, session (as the primary noun for this act)

**New Card**:
A Card in phase New. The first Rating leaves New using Learning rules at step 0 and consumes one slot of the Deck’s daily new-card cap (including Easy that graduates straight to Review); further Learning steps that day do not consume another slot.
_Avoid_: Unseen, unseen card, freshman

**Review**:
(1) The phase for long-term FSRS scheduling. (2) One attempt to recall a Card's back: show front, reveal back, then a Rating. No typed answer. On each Rating, subscriber Course copies store a front/back Markdown snapshot used later for update diffs.
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
The single persistence port in `domain` (one trait, not per-entity repositories). Includes Deck/Card/study ports, User/Session ports, and Course/Subscription/copy-sync ports. Implemented by `db` (SQLx/SQLite). Use cases load through Store and call domain policy; HTTP never imports SQLite. Markdown rendering is not part of Store.
_Avoid_: Repository (as the primary noun), Dao, Unit of Work
