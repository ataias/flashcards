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
-- Existing Decks keep user_id NULL (orphans). Bootstrap assigns them to
-- the first admin instead of seeding a second Default.
--
-- Add the column in place. Rebuilding `decks` would CASCADE-delete Cards
-- (ON DELETE actions run immediately, even with deferred FK checks).

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

ALTER TABLE decks ADD COLUMN user_id INTEGER REFERENCES users (id) ON DELETE CASCADE;

CREATE INDEX decks_user_id ON decks (user_id);
