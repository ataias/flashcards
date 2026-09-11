-- New Card FSRS invariants: all four fields NULL together, or all set.
-- SQLite cannot ADD CHECK; rebuild the table. `defer_foreign_keys` lets the
-- drop+rename succeed inside sqlx's migration transaction (review_logs FK
-- is checked at COMMIT, after `cards` exists again under the same name).

PRAGMA defer_foreign_keys = ON;

CREATE TABLE cards_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    deck_id INTEGER NOT NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    stability REAL,
    difficulty REAL,
    due TEXT,
    last_review TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (deck_id) REFERENCES decks (id) ON DELETE CASCADE,
    CHECK ((stability IS NULL) = (difficulty IS NULL)),
    CHECK ((due IS NULL) = (last_review IS NULL)),
    CHECK ((stability IS NULL) = (due IS NULL))
);

INSERT INTO cards_new (
    id, deck_id, front, back, stability, difficulty, due, last_review, created_at
)
SELECT
    id, deck_id, front, back, stability, difficulty, due, last_review, created_at
FROM cards;

DROP TABLE cards;
ALTER TABLE cards_new RENAME TO cards;

CREATE INDEX cards_deck_id ON cards (deck_id);
CREATE INDEX cards_due ON cards (due);

PRAGMA foreign_key_check;
PRAGMA defer_foreign_keys = OFF;
