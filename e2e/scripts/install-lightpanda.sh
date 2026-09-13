#!/usr/bin/env bash
# Download the pinned Lightpanda binary for this host into e2e/.lightpanda/.
# Bump LIGHTPANDA_VERSION (and checksums) deliberately with Playwright.
#
#   ./e2e/scripts/install-lightpanda.sh
#   LIGHTPANDA_DIR=/tmp/lp ./e2e/scripts/install-lightpanda.sh
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Pin: https://github.com/lightpanda-io/browser/releases/tag/0.4.0
LIGHTPANDA_VERSION="${LIGHTPANDA_VERSION:-0.4.0}"
DEST_DIR="${LIGHTPANDA_DIR:-$PWD/.lightpanda}"
DEST="${DEST_DIR}/lightpanda"
STAMP="${DEST_DIR}/VERSION"

asset_for_host() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}-${arch}" in
    Linux-x86_64) echo "lightpanda-x86_64-linux" ;;
    Linux-aarch64 | Linux-arm64) echo "lightpanda-aarch64-linux" ;;
    Darwin-x86_64) echo "lightpanda-x86_64-macos" ;;
    Darwin-arm64) echo "lightpanda-aarch64-macos" ;;
    *)
      echo "error: unsupported platform ${os}-${arch}" >&2
      exit 1
      ;;
  esac
}

# SHA-256 of the 0.4.0 GitHub release assets (from the release API digest).
checksum_for_asset() {
  case "$1" in
    lightpanda-x86_64-linux)
      echo "bfcf9bd7e80939b87232aa114a49d8f397f51af0c2632d9fc58d4a6d4386624f"
      ;;
    lightpanda-aarch64-linux)
      echo "5e3b54deed642ffeb2b8f24a1931e54c51161f44d9d728135da3d4863cb722fb"
      ;;
    lightpanda-x86_64-macos)
      echo "fe50a51d4983dd1b93d410c5ac176bb14b7e57da940a5a0eb381a69f18bc57bd"
      ;;
    lightpanda-aarch64-macos)
      echo "840547bb7b98743a3e32618a4d120ac4a75e7c3c2d227ecf5ce8d508ddc118b7"
      ;;
    *)
      echo ""
      ;;
  esac
}

verify_sha256() {
  local file="$1"
  local expected="$2"
  local actual
  if command -v sha256sum >/dev/null; then
    actual="$(sha256sum "${file}" | awk '{print $1}')"
  else
    actual="$(shasum -a 256 "${file}" | awk '{print $1}')"
  fi
  if [[ "${actual}" != "${expected}" ]]; then
    echo "error: checksum mismatch for ${file}" >&2
    echo "  expected ${expected}" >&2
    echo "  got      ${actual}" >&2
    rm -f "${file}"
    exit 1
  fi
}

if [[ -x "${DEST}" && -f "${STAMP}" && "$(cat "${STAMP}")" == "${LIGHTPANDA_VERSION}" ]]; then
  echo "Lightpanda ${LIGHTPANDA_VERSION} already at ${DEST}"
  echo "${DEST}"
  exit 0
fi

asset="$(asset_for_host)"
url="https://github.com/lightpanda-io/browser/releases/download/${LIGHTPANDA_VERSION}/${asset}"
expected_sha="$(checksum_for_asset "${asset}")"

mkdir -p "${DEST_DIR}"
tmp="$(mktemp "${DEST_DIR}/lightpanda.download.XXXXXX")"
trap 'rm -f "${tmp}"' EXIT

echo "Downloading Lightpanda ${LIGHTPANDA_VERSION} (${asset})"
curl --fail --location --show-error --output "${tmp}" "${url}"

if [[ "${LIGHTPANDA_VERSION}" == "0.4.0" && -n "${expected_sha}" ]]; then
  verify_sha256 "${tmp}" "${expected_sha}"
elif [[ -n "${LIGHTPANDA_SHA256:-}" ]]; then
  verify_sha256 "${tmp}" "${LIGHTPANDA_SHA256}"
else
  echo "warning: no checksum for ${LIGHTPANDA_VERSION}/${asset}; skip verify" >&2
fi

chmod a+x "${tmp}"
mv "${tmp}" "${DEST}"
trap - EXIT
printf '%s\n' "${LIGHTPANDA_VERSION}" >"${STAMP}"

echo "Installed Lightpanda ${LIGHTPANDA_VERSION} -> ${DEST}"
echo "${DEST}"
