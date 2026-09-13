# Markdown as Card content

Card front/back are stored as Markdown (CommonMark + GFM tables/code/strikethrough), not Anki HTML or Portable Text. `web` renders to sanitized HTML with server-side highlighting via CSS token classes so light/dark theming needs no re-render. Domain keeps raw strings so Courses diffs and future LLM tooling stay on Markdown.
