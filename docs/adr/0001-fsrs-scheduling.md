# Use FSRS via the `fsrs` crate

We want Anki-compatible modern scheduling, not classic SM-2. v1 uses the maintained `fsrs` crate (open-spaced-repetition/fsrs-rs, FSRS v6) with default parameters; personal parameter optimization can come later. We rejected `rs-fsrs` because it lags on crates.io and tracks older FSRS v5.

## Minimum interval (v1)

After FSRS returns an interval, v1 rounds and floors to **at least 1 day** (including Again). That matches the crate’s basic schedule example and keeps Study free of minute-level relearning queues. Sub-day / Anki-style learning steps are explicitly out of v1 and need a later grill (https://github.com/ataias/flashcards/issues/21).
