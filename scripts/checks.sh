#!/usr/bin/env bash
# SPEC-ci PR/main gate (edition, fmt, clippy, test, build, relative lychee).
# Run from any cwd. Needs rustfmt, clippy, jq, and lychee on PATH — or:
#   docker run --rm -v "$PWD:/workspace" -w /workspace flashcards-ci:local ./scripts/checks.sh
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if [ -d .sqlx ]; then
  export SQLX_OFFLINE=true
fi

cargo metadata --format-version=1 --no-deps \
  | jq -e '[.packages[] | select(.source==null) | .edition] | length > 0 and all(. == "2024")'
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
lychee --offline -- README.md CONTEXT.md docs
