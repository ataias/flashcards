# CI toolchain image. Rust pin matches rust-toolchain.toml (channel 1.88).
# App sources are not copied — Actions checks out the repo at runtime (SPEC-ci).
FROM rust:1.88-bookworm

RUN rustup component add rustfmt clippy

# Native deps for SQLx / SQLite linking.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libsqlite3-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Pinned lychee; lychee.toml / ci.yml land in later issues.
ARG LYCHEE_VERSION=0.24.2
RUN set -eux; \
    arch="$(uname -m)"; \
    case "$arch" in \
      x86_64) \
        lychee_arch="x86_64-unknown-linux-musl"; \
        lychee_sha256="73657a111819a30c47c08352896796f23d64e4eb2b3ed39b6d32149241566fc5"; \
        ;; \
      aarch64) \
        lychee_arch="aarch64-unknown-linux-musl"; \
        lychee_sha256="5d0b0e3aeab240f41920c633a6eaf97599be6eedda034b36e858ede7dba5e535"; \
        ;; \
      *) echo "unsupported architecture: ${arch}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/lychee.tar.gz \
      "https://github.com/lycheeverse/lychee/releases/download/lychee-v${LYCHEE_VERSION}/lychee-${lychee_arch}.tar.gz"; \
    echo "${lychee_sha256}  /tmp/lychee.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/lychee.tar.gz -C /usr/local/bin; \
    rm /tmp/lychee.tar.gz; \
    lychee --version
