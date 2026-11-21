#!/usr/bin/env bash
# Full verification matrix. Every gate is a hard failure — this is what the
# v1.0.0 acceptance run used.
#
#   ./scripts/verify.sh            # fmt, clippy, tests, tsc, vitest
#   ./scripts/verify.sh --fast     # skip clippy (the slow one)
#
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
FAST=0
[[ "${1:-}" == "--fast" ]] && FAST=1

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

step "cargo fmt --check"
(cd src-tauri && cargo fmt --check)

if [[ $FAST -eq 0 ]]; then
  step "cargo clippy -D warnings"
  (cd src-tauri && cargo clippy --all-targets -j 1 -- -D warnings)
fi

step "cargo test"
(cd src-tauri && cargo test -j 1)

step "tsc --noEmit"
npx tsc --noEmit

step "vitest"
npx vitest run

step "all checks passed"
