# Use FSRS via the `fsrs` crate

We want Anki-compatible modern scheduling, not classic SM-2. The app uses the maintained `fsrs` crate (open-spaced-repetition/fsrs-rs, FSRS v6) with default parameters; personal parameter optimization can come later. We rejected `rs-fsrs` because it lags on crates.io and tracks older FSRS v5.

## Minimum interval

**v1:** After FSRS returned an interval, v1 rounded and floored to **at least 1 day** (including Again). See historical SPEC-v1.

**v1.2:** That floor is **removed for Review**. Learning/Relearning use fixed minute steps; Review uses the crate’s float-day interval as a real duration. Decision record: [`0005-learning-relearning-steps.md`](0005-learning-relearning-steps.md) (issue #21).
