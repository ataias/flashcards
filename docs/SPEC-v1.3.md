# Flashcards v1.3 — Users and private spaces

Status: **accepted** (grilled 2026-09-13; migration amended 2026-09-13 to **wipe** pre-v1.3 rows). Implementation via parent [#85](https://github.com/ataias/flashcards/issues/85).

Domain language: [`CONTEXT.md`](../CONTEXT.md). Decisions: [`adr/0006-private-user-spaces.md`](adr/0006-private-user-spaces.md), updated [`adr/0004-default-deck-seed-only.md`](adr/0004-default-deck-seed-only.md).

## Goal

Gate the app behind login. Each User owns a private space of Decks and Cards. Keep v1.2 scheduling on each User's own Cards. Preserve the v1.1 rule that `web` never imports `db` (User/Session go through `Store`).

## Product delta vs v1.2

| Topic | Before | v1.3 |
| --- | --- | --- |
| Access | Open local UI | Login required except login, bootstrap (if empty), `/about` |
| Ownership | Single shared DB space | Decks/Cards owned by a User; cross-User ids → 404 |
| Accounts | None | Username + password; one bootstrap admin; admin creates others |
| Default Deck | Seeded on first DB init | Seeded when each User is created |
| Existing data | N/A | **Wipe** pre-v1.3 Decks/Cards/review_logs on migration; bootstrap gets a **fresh Default** (no orphan assign) |
| Sessions | N/A | Server-side SQLite sessions + cookie |

## Accounts

- **Bootstrap:** If zero Users exist, show a one-time create-admin form; afterward that path is gone.
- **Admin (v1.3):** Only the bootstrap admin is admin. No promote/demote UI.
- **Admin can:** create Users (sets initial password), list Users, disable, delete, reset another User's password.
- **Guards:** Cannot delete/disable yourself; cannot delete/disable the last admin.
- **Delete User:** Hard cascade — User, Decks, Cards, Review logs, Sessions.
- **Disable:** Sets disabled + deletes all that User's Sessions immediately.
- **Username:** 3–32 chars, `[A-Za-z0-9_.-]`; unique ignoring case; stored as entered.
- **Password:** Argon2id; self-service change; admin reset. Create-User: admin sets initial password; no forced change-on-first-login.
- **No public signup / email / OAuth** in this slice.

## Sessions and security

- Server-side Session rows in SQLite; cookie is a random id only.
- Cookie: `HttpOnly`, `Secure`, `SameSite=Strict`.
- CSRF synchronizer token on all state-changing requests (plus SameSite).
- Sliding idle timeout: **30 days** (refresh on use).
- Many Sessions per User allowed.
- Logout: this Session, and "log out everywhere".
- Password change (self), admin password reset, and disable: wipe **all** that User's Sessions.
- In-memory rate limit on login and bootstrap (per-IP, optional per-username).
- No User-Agent / IP session binding.

## Public routes

- Login (GET/POST)
- Bootstrap create-admin (only when User count is 0)
- `GET /about`
- Static assets needed for those pages

All other routes require a valid Session for a non-disabled User.

## Architecture

- `User` is a domain type; Session persistence is via **Store ports** (create/get/delete/list-by-user, etc.).
- `web` middleware and auth handlers call domain use cases / Store only — **never** `db` / SQLx.
- Deck/Card/study use cases take the current User and scope all loads/writes to that owner.
- Migration: add `users` / `sessions` and `decks.user_id` **NOT NULL**; **delete** any pre-v1.3 ownerless Decks/Cards/review_logs; bootstrap always seeds a fresh Default (no `assign_orphan_decks`).

## Out of scope (v1.3)

- Courses → v1.4 ([SPEC-v1.4](SPEC-v1.4.md), #91)
- Roles beyond the single bootstrap admin (#101)
- Email, magic links, OAuth, passkeys
- Learn mode UI ([#55](https://github.com/ataias/flashcards/issues/55))

## Acceptance criteria

1. Fresh empty DB: bootstrap form creates the only admin; form gone afterward; Default Deck seeded for that User.
2. Existing pre-v1.3 DB: migration **wipes** prior Decks/Cards/review_logs; bootstrap admin gets a **fresh Default** (study data from before Users is not preserved).
3. Unauthenticated users hitting study/CRUD are redirected to login (or equivalent); `/about` stays public.
4. Admin can create a User with username+password; new User gets Default Deck; can log in and only see their own Decks.
5. Cross-User Deck/Card ids return 404.
6. Disable and password reset wipe Sessions; disabled User cannot keep using an old cookie.
7. Delete User cascades Decks/Cards/logs/Sessions; self-delete and last-admin removal are rejected.
8. CSRF + SameSite=Strict on mutating requests; Argon2id hashes only; login rate-limited.
9. `web` has no `db` dependency; Session/User go through Store.
10. HTTP/`cargo test` cover bootstrap, login/logout, isolation, admin happy paths, and cascade/disable session wipe; CI green; do-work PR rules apply.
