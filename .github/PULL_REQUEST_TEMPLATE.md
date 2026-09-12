## Summary

<!-- PR title: no issue number (`#N`, `Fixes #N`, `issue N`). `Fixes #` stays in this body. -->
<!-- What changed and why -->

Fixes #

## Test plan

- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace`
- [ ] lychee relative (when available)
- [ ] `.sqlx/` updated if queries changed
- [ ] Name regression tests / HTTP suite (e.g. `crates/flashcards/tests/http.rs`); do not N/A wiring-only without naming them
- [ ] UI visual proof (screenshots and/or short video) in this Test plan — required for UI-facing PRs; N/A for CI-only / non-UI

## SPEC / CONTEXT deviations

none

## Review

- [ ] Code Reviewer requested
- [ ] Stacking notes (base branch) if not targeting `main` alone
