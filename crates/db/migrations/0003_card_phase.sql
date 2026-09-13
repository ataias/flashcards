-- Persist explicit Card phase + learning step.
-- Backfill: no FSRS memory → New; has memory → Review (step unused).
-- Learning may have due/last_review without memory, so the previous
-- "all four FSRS fields together" CHECK is replaced by phase rules.
-- SQLite cannot ADD/DROP CHECK; rebuild the table.

PRAGMA defer_foreign_keys = ON;

CREATE TABLE cards_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    deck_id INTEGER NOT NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    phase TEXT NOT NULL DEFAULT 'new',
    learning_step INTEGER,
    stability REAL,
    difficulty REAL,
    due TEXT,
    last_review TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (deck_id) REFERENCES decks (id) ON DELETE CASCADE,
    CHECK (phase IN ('new', 'learning', 'review', 'relearning')),
    CHECK (learning_step IS NULL OR learning_step >= 0),
    CHECK ((stability IS NULL) = (difficulty IS NULL)),
    CHECK ((due IS NULL) = (last_review IS NULL)),
    CHECK (
        (phase = 'new' AND stability IS NULL AND due IS NULL AND learning_step IS NULL)
        OR (phase = 'learning' AND stability IS NULL AND due IS NOT NULL AND learning_step IS NOT NULL)
        OR (phase = 'review' AND stability IS NOT NULL AND due IS NOT NULL AND learning_step IS NULL)
        OR (phase = 'relearning' AND stability IS NOT NULL AND due IS NOT NULL AND learning_step IS NOT NULL)
    )
);

INSERT INTO cards_new (
    id, deck_id, front, back, phase, learning_step,
    stability, difficulty, due, last_review, created_at
)
SELECT
    id,
    deck_id,
    front,
    back,
    CASE WHEN stability IS NULL THEN 'new' ELSE 'review' END,
    NULL,
    stability,
    difficulty,
    due,
    last_review,
    created_at
FROM cards;

DROP TABLE cards;
ALTER TABLE cards_new RENAME TO cards;

CREATE INDEX cards_deck_id ON cards (deck_id);
CREATE INDEX cards_due ON cards (due);

PRAGMA foreign_key_check;
PRAGMA defer_foreign_keys = OFF;
