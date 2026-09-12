---
name: do-work
description: >-
  Use when implementing any flashcards issue or opening/updating a PR on
  ataias/flashcards — ticket pick, pre-PR checks, stacking, rebase hygiene,
  and review rules.
---

# do-work (ataias/flashcards)

Mandatory workflow for **Full Stack** on this repo. Run at the **start** of an implementation task and again **before opening or updating a PR**.

## 1. Pick work

1. Choose **one open child issue** (not parent epics such as #1 or #10).
2. Skip issues whose **Blocked by** is unmet; do the blocker first.
3. Require these sections in the issue body — if any are missing, **comment and stop** (do not invent scope):
   - **Goal**
   - **Where to start**
   - **Work**
   - **Done when**
   - **Refs**
   - **Blocked by**
4. If the issue is fuzzy or conflicts with docs, ping **Architect** / Ataias — do not enlarge the ticket.

## 2. Read before coding

1. The issue body end-to-end.
2. [`CONTEXT.md`](../../../CONTEXT.md)
3. Every SPEC/ADR linked under **Refs** (typically [`docs/SPEC-v1.md`](../../../docs/SPEC-v1.md), [`docs/SPEC-ci.md`](../../../docs/SPEC-ci.md), [`docs/adr/`](../../../docs/adr/)).
4. If code and docs disagree: **stop and ask**. Do not “fix” docs in the same PR unless the issue says to.

## 3. Branch

- From up-to-date `main`, or from an approved stacked base when stacking (see §6–§7).
- Name: `issue-N-short-slug` (example: `issue-2-workspace-scaffold`).
- **One child issue ↔ one PR.** No issue work committed straight to `main`. No PR whose only job is an epic parent.

## 4. Implement

- Stay inside **Work** / **Done when**.
- Prefer small, reviewable commits.
- Match house style already in the repo; follow CONTEXT vocabulary.

## 5. Before opening (or pushing to) the PR

All must pass locally:

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
```

When `lychee.toml` / CI exist, also run **relative-only** lychee the same way `ci.yml` does.

If SQL queries changed: regenerate `.sqlx/` and **commit** it (`SQLX_OFFLINE=true` in CI).

Until CI exists, still require fmt / clippy / test / build.

**UI visual proof:** For **UI-facing PRs** (HTMX pages/fragments, forms, Study, Deck/Card CRUD, empty states, etc.), cargo checks alone are not enough. Put screenshots and/or a short video in the PR **Test plan** **before** requesting Code Reviewer. CI-only / non-UI PRs do not need screenshots.

**Basic regression proof:** For refactor / architecture / wiring PRs, follow [`regression-proof.md`](./regression-proof.md) in the Test plan. UI visual-proof rules above still apply when behavior is user-visible.

## 6. Open the PR

- Base: `main` (or the stacked parent branch when stacking).
- Title: clear, human-scoped, and scoped to the issue. **Do not** put issue numbers in the title (`#38`, `Fixes #38`, `issue 38`, etc.). The issue link belongs in the body only.
- Body must include:
  - Summary of what changed
  - `Fixes #N` (body only — never in the title)
  - **Test plan** (commands you ran; name the HTTP/integration suite that still proves the app — do not N/A wiring-only without naming it; UI-facing PRs must include visual proof — screenshots and/or short video — before requesting Code Reviewer)
  - **SPEC/CONTEXT deviations** (or `none`)
  - Stacking notes when base ≠ `main`
- Use `.github/PULL_REQUEST_TEMPLATE.md` when present.

## 7. Stack rebase hygiene

Stacked PRs go stale when `main` (or a lower stack branch) moves. **Check before every new stacked PR and whenever Architect / Ataias asks for a rebase.**

1. Fetch latest remotes.
2. Identify the stack bottom-up (PR whose base is `main`, then each PR that bases on the previous head).
3. For each branch from **bottom to tip**:
   - If it is behind its intended base, **rebase** onto that base (not merge).
   - Push with `--force-with-lease` only (never bare `--force`).
4. Keep GitHub PR **base** branches correct after rebases (tip still targets the parent stack branch; bottom targets `main`).
5. **Every open stack must include a PR with base = `main` (the bottom).** Never leave orphan tips whose parent PRs are closed.
6. Do **not** close stacked PRs, retarget a mid-stack tip onto `main` as a mega-diff, or force-push / reset a parent stack branch onto `main`. Only Ataias closes or merges. If `gh stack` 403s or would close/destroy parents, stop and set bases with `gh pr edit --base` (or the API) instead.
7. Re-run §5 checks on the tip after the full stack rebase.
8. If rebase conflicts are non-trivial or change behavior, stop and ping **Architect**.

Do **not** open a fresh PR to “fix” a rebase — update the existing issue branch.

## 8. Review and merge rules

1. Request review from **Code Reviewer**. Do not merge your own PR.
2. Merge requires **Code Reviewer approval** and **Ataias approval**.
3. If CI fails: fix on the **same branch** and push — do not open a second PR for the same issue.
4. **Stacking:** after Code Reviewer approves, you may open the next issue’s PR **on top of that branch** without waiting for Ataias’s merge — unless **Blocked by** / **Where to start** says the work needs `main` first (some CI/default-branch cases). If unsure, ask Ataias once.
5. Before stacking a new PR, run §7 (rebase) so you are not building on a stale base.

## 9. Done

- PR merged (by Ataias after approvals) closes the issue via `Fixes #N`.
- Only then is the child issue complete.
