# Deep modules + single Store port (v1.1)

We keep a hybrid of deep modules and light DDD: `domain` owns use cases, types, `Card::apply_rating`, and one `Store` trait; `db` implements Store; `web` is an HTTP library on `domain` only; a thin `flashcards` binary wires the pool. Rating and new-card cap policy stay in domain once; Store persists (including atomic `commit_review`). We deliberately skip per-entity repos, a generic unit-of-work, and renaming `domain`.
