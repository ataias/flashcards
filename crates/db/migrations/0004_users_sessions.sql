-- Users, Sessions, and Deck ownership.
--
-- Cascade:
--   DELETE FROM users
--     → that User's Decks (decks.user_id ON DELETE CASCADE)
--     → those Decks' Cards (cards.deck_id ON DELETE CASCADE)
--     → those Cards' Review logs (review_logs.card_id ON DELETE CASCADE)
--     → that User's Sessions (sessions.user_id ON DELETE CASCADE)
-- Disable only flips users.disabled; Session rows are deleted by Store.
--
-- Pre-v1.3 Decks/Cards/review_logs have no owner. Wipe them here so
-- bootstrap always seeds a fresh Default instead of inheriting leftovers.
-- Rebuild `decks` with user_id NOT NULL: the table is empty after DELETE,
-- and deferred FKs let us replace it while `cards` still references it.

CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    admin INTEGER NOT NULL CHECK (admin IN (0, 1)),
    disabled INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE UNIQUE INDEX users_username_nocase ON users (username COLLATE NOCASE);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX sessions_user_id ON sessions (user_id);

DELETE FROM decks;

PRAGMA defer_foreign_keys = ON;

CREATE TABLE decks_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE
);

DROP TABLE decks;
ALTER TABLE decks_new RENAME TO decks;

CREATE INDEX decks_user_id ON decks (user_id);

PRAGMA foreign_key_check;
PRAGMA defer_foreign_keys = OFF;
