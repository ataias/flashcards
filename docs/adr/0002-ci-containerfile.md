# CI runs in an image built from a root Containerfile

We want reproducible CI for a Rust + SQLite workspace on GitHub Actions. Bare `ubuntu-latest` plus ad-hoc `dtolnay/rust-toolchain` drifts from local setups and makes lychee/SQLite deps easy to forget. v1 builds a root `Containerfile` (pinned Rust minor, rustfmt, clippy, lychee, sqlite libs) in Actions and runs checks inside that image, with `rust-toolchain.toml` matching the pin. Image publish to GHCR is deferred until rebuild cost hurts.
