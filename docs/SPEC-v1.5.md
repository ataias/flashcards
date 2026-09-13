# Flashcards v1.5 — Markdown Card content

Status: **accepted** (grilled 2026-09-13). Docs/tickets now; **do not implement until Ataias triggers** (typically after v1.3 Users).

Domain language: [`CONTEXT.md`](../CONTEXT.md). Decision: [`adr/0008-markdown-card-content.md`](adr/0008-markdown-card-content.md).

## Goal

Store Card front/back as Markdown; render safely to HTML in `web` with shared light/dark styling, tables, and syntax-highlighted code — without Anki-style per-card HTML/CSS.

## Product delta

| Topic | Before | v1.5 |
| --- | --- | --- |
| Storage | Plain text | Markdown source (same DB string columns) |
| Study/detail | Escaped plain text | Rendered sanitized HTML |
| Edit | Textareas | Textareas + rendered preview |
| Lists | Raw front string | Plain-text excerpt (markup stripped) |

## Format

- CommonMark + GFM **tables**, fenced **code** (language tag), **strikethrough**
- **No** raw HTML in Markdown source
- **Out of this slice:** images (#112), math/Ratex (#113), cloze/typed/progressive reveal (#111), WYSIWYG editor, LLM generation

## Rendering

- Only in **`web`**: Markdown → HTML → HTML sanitizer allowlist → optional server-side highlighter emitting **CSS token classes**
- Domain/Store keep raw Markdown; Course diffs compare Markdown
- Shared prose + code CSS; light/dark via `prefers-color-scheme` and/or site theme; **CSS-only** theme switch (no DOM re-render)
- No DB migration / newline rewrite of existing rows

## Acceptance criteria

1. Create/edit Card with Markdown; preview matches Study render.
2. Study shows rendered tables/code/strikethrough; scripts/raw HTML cannot execute.
3. Theme toggle (or OS scheme) restyles code without refetching HTML.
4. Deck lists show plain excerpts, not raw `**` noise or full HTML.
5. Domain has no markdown crate dependency.
6. HTTP tests cover render/sanitize happy paths and a basic XSS attempt rejected; CI green.

## Implement note

**Full Stack: do not start until Ataias triggers** (Users v1.3 should be done first).
