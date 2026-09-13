#!/usr/bin/env bash
# Fail if publish artifacts are the wrong ELF architecture.
# Used by .github/workflows/publish-image.yml after download, before pack.
#
#   deploy/bin/amd64/flashcards  must be ELF x86-64
#   deploy/bin/arm64/flashcards  must be ELF AArch64
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

assert_elf() {
  local path="$1"
  local file_re="$2"
  local machine_re="$3"

  if [[ ! -f "${path}" ]]; then
    echo "error: missing ${path}" >&2
    exit 1
  fi

  local info machine
  info="$(file -b "${path}")"
  machine="$(readelf -h "${path}" | awk -F: '/Machine:/ { sub(/^ +/, "", $2); print $2; exit }')"

  echo "${path}"
  echo "  file:    ${info}"
  echo "  machine: ${machine}"

  if ! grep -Eq "${file_re}" <<<"${info}"; then
    echo "error: ${path} file(1) did not match /${file_re}/" >&2
    echo "       got: ${info}" >&2
    exit 1
  fi
  if ! grep -Eiq "${machine_re}" <<<"${machine}"; then
    echo "error: ${path} readelf Machine did not match /${machine_re}/" >&2
    echo "       got: ${machine}" >&2
    exit 1
  fi
}

assert_elf deploy/bin/amd64/flashcards 'x86-64|x86_64' 'X86-64|Advanced Micro Devices X86-64'
assert_elf deploy/bin/arm64/flashcards 'ARM aarch64|aarch64' 'AArch64'

echo "artifact architectures match amd64=x86-64 and arm64=AArch64"
