# Basic regression proof

When a PR is mainly refactor, architecture, package split, or wiring, `cargo test --workspace` green is necessary but not sufficient in the PR **Test plan**.

## Required in the Test plan

1. **Name** which HTTP/integration tests still prove the app works after the change (home, deck/card CRUD, study/rate as relevant).
2. Do **not** mark coverage N/A for “package split / wiring only” without naming the suite that still exercises the app (e.g. `crates/flashcards/tests/http.rs`).
3. If a layer changes user-visible behavior, keep the UI visual-proof rules in [`SKILL.md`](./SKILL.md) §5; if that proof lives on a stacked UI PR, say so with an explicit link.
