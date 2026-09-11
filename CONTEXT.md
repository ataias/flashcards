# Flashcards

A single-user Anki-like study app: create cards, review what's due, reschedule by rating.

## Language

**Card**:
A flashcard with a front (prompt) and a back (answer). v1 is plain text only; markdown is planned for v2 (to be grilled after v1).
_Avoid_: Note, item, flashcard (as a type name)

**Deck**:
A named collection of Cards. v1 has a default Deck and allows additional named Decks.
_Avoid_: Folder, set, pile, collection

**Review**:
One attempt to recall a Card's back, ending in a Rating that updates scheduling.
_Avoid_: Study, quiz, attempt (as the noun for this act)

**Rating**:
The outcome of a Review: Again, Hard, Good, or Easy. Mapped into FSRS as the four standard grades.
_Avoid_: Grade, score, button

**Due**:
A Card is due when its next-review time has arrived or passed; Reviews are drawn from due Cards.
_Avoid_: Ready, overdue (as the primary term)

**Scheduler**:
FSRS (Free Spaced Repetition Scheduler) via the `fsrs` crate (FSRS v6, default parameters in v1). Not SM-2; not the lighter `rs-fsrs` crate.
_Avoid_: SM-2, Anki algorithm (ambiguous), SRS (generic), rs-fsrs
