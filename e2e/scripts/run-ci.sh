#!/usr/bin/env bash
# Start the flashcards binary + Lightpanda CDP, run Playwright, tear down.
# Expects a built debug binary and an installed Lightpanda (see e2e/README.md).
#
#   cargo build -p flashcards
#   ./e2e/scripts/install-lightpanda.sh
#   PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci --prefix e2e
#   ./e2e/scripts/run-ci.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
E2E="${ROOT}/e2e"
BIN="${FLASHCARDS_BIN:-${ROOT}/target/debug/flashcards}"
LIGHTPANDA="${LIGHTPANDA_BIN:-${E2E}/.lightpanda/lightpanda}"

if [[ ! -x "${BIN}" ]]; then
  echo "error: missing flashcards binary at ${BIN}" >&2
  echo "Run: cargo build -p flashcards" >&2
  exit 1
fi
if [[ ! -x "${LIGHTPANDA}" ]]; then
  echo "error: missing Lightpanda at ${LIGHTPANDA}" >&2
  echo "Run: ./e2e/scripts/install-lightpanda.sh" >&2
  exit 1
fi

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

APP_PORT="$(free_port)"
CDP_PORT="$(free_port)"
WORK_DIR="$(mktemp -d)"
DB_PATH="${WORK_DIR}/flashcards.db"
SERVER_LOG="${WORK_DIR}/flashcards.log"
LP_LOG="${WORK_DIR}/lightpanda.log"
SERVER_PID=""
LP_PID=""

dump_logs() {
  echo "----- flashcards (${SERVER_LOG}) -----" >&2
  if [[ -f "${SERVER_LOG}" ]]; then cat "${SERVER_LOG}" >&2; else echo "(missing)" >&2; fi
  echo "----- lightpanda (${LP_LOG}) -----" >&2
  if [[ -f "${LP_LOG}" ]]; then cat "${LP_LOG}" >&2; else echo "(missing)" >&2; fi
}

stop_pid() {
  local pid="${1:-}"
  if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
  fi
}

cleanup() {
  stop_pid "${SERVER_PID}"
  stop_pid "${LP_PID}"
  rm -rf "${WORK_DIR}"
}

on_err() {
  echo "error: e2e run failed; dumping process logs" >&2
  dump_logs
}

trap on_err ERR
trap cleanup EXIT

# Empty temp DB: no seed binary. The process stays up; Playwright smoke
# creates the first admin through the real bootstrap form.
FLASHCARDS_DB="${DB_PATH}" FLASHCARDS_BIND="127.0.0.1:${APP_PORT}" \
  "${BIN}" >"${SERVER_LOG}" 2>&1 &
SERVER_PID=$!

"${LIGHTPANDA}" serve --host 127.0.0.1 --port "${CDP_PORT}" \
  >"${LP_LOG}" 2>&1 &
LP_PID=$!

wait_http() {
  local url="$1"
  local pid="${2:-}"
  local i
  for i in $(seq 1 60); do
    if [[ -n "${pid}" ]] && ! kill -0 "${pid}" 2>/dev/null; then
      echo "error: process ${pid} died while waiting for ${url}" >&2
      return 1
    fi
    if curl --fail --silent --max-time 1 "${url}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  echo "error: timed out waiting for ${url}" >&2
  return 1
}

wait_http "http://127.0.0.1:${APP_PORT}/" "${SERVER_PID}"
wait_http "http://127.0.0.1:${CDP_PORT}/json/version" "${LP_PID}"

if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
  echo "error: flashcards exited before tests" >&2
  exit 1
fi
if ! kill -0 "${LP_PID}" 2>/dev/null; then
  echo "error: Lightpanda exited before tests" >&2
  exit 1
fi

echo "flashcards http://127.0.0.1:${APP_PORT}  Lightpanda CDP http://127.0.0.1:${CDP_PORT}"

cd "${E2E}"
E2E_BASE_URL="http://127.0.0.1:${APP_PORT}" \
  CDP_URL="http://127.0.0.1:${CDP_PORT}" \
  npx playwright test
