# Private User spaces with Card-local scheduling

Multi-user access is modeled as **private spaces**: each User owns Decks and Cards. Scheduling state (phase, due, FSRS, learning step, Review log) stays on the User's own Card — not a shared Card with a separate progress table. Future Courses are expected to **copy** Cards into a subscriber's space (with a source link for content updates), reusing the same Card-local scheduling model. Auth uses server-side SQLite Sessions exposed through `Store` so `web` never imports `db`.
