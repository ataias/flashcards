-- Initial schema: decks, cards (FSRS memory state + due), review_logs.
--
-- Cascade:
--   DELETE FROM decks  → deletes that Deck's Cards (cards.deck_id ON DELETE CASCADE)
--   DELETE FROM cards  → deletes that Card's Review logs (review_logs.card_id ON DELETE CASCADE)
-- So deleting a Deck also removes its Review logs (via Cards).
--
-- SQLite foreign keys are off by default. `db::open` enables them on every
-- connection (`PRAGMA foreign_keys = ON`); this file documents the FKs only.

CREATE TABLE decks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE cards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    deck_id INTEGER NOT NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    -- FSRS MemoryState (fsrs crate). Both NULL means New Card (never Reviewed).
    stability REAL,
    difficulty REAL,
    -- Next due instant (RFC3339 UTC). NULL for New Cards.
    due TEXT,
    -- Instant of the last Review (RFC3339 UTC). NULL for New Cards.
    last_review TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (deck_id) REFERENCES decks (id) ON DELETE CASCADE
);

CREATE TABLE review_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id INTEGER NOT NULL,
    rated_at TEXT NOT NULL,
    -- FSRS grades: 1=Again, 2=Hard, 3=Good, 4=Easy.
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 4),
    FOREIGN KEY (card_id) REFERENCES cards (id) ON DELETE CASCADE
);

CREATE INDEX cards_deck_id ON cards (deck_id);
CREATE INDEX cards_due ON cards (due);
CREATE INDEX review_logs_card_id ON review_logs (card_id);
