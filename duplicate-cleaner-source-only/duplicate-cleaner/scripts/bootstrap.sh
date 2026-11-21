#!/usr/bin/env bash
# Restores everything needed to build and run Duplicate Cleaner on a clean
# Debian/Ubuntu box, then produces dist/ and both debug and release binaries.
#
# Idempotent: safe to re-run. Used in this sandbox because build artefacts
# (target/, node_modules/, ~/.cargo) are not persisted between sessions.
#
#   ./scripts/bootstrap.sh            # deps + dist + debug build
#   ./scripts/bootstrap.sh --release  # ... plus the release binary
#
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
WITH_RELEASE=0
[[ "${1:-}" == "--release" ]] && WITH_RELEASE=1

export PATH="$HOME/.cargo/bin:$PATH"

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

# ---------------------------------------------------------------- toolchain
if ! command -v cargo >/dev/null 2>&1; then
  step "installing Rust"
  curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
    --default-toolchain 1.100.0 >/dev/null
fi
step "rust $(rustc --version)"
rustup component add rustfmt clippy >/dev/null 2>&1 || true

# ------------------------------------------------------------- system libs
step "system packages"
if command -v apt-get >/dev/null 2>&1; then
  sudo -n apt-get update -qq
  sudo -n apt-get install -y -qq \
    libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev patchelf build-essential pkg-config libssl-dev \
    xvfb xdotool xclip imagemagick xdg-utils sqlite3 >/dev/null
else
  echo "skipped (no apt-get) — install the Tauri 2 prerequisites manually"
fi

# ------------------------------------------------------------ frontend deps
step "npm ci"
npm ci --silent

step "vite build"
npm run build

# ------------------------------------------------------------------ rust
# Swap first: with 2 GB of RAM the debug build thrashes and the release build
# gets OOM-killed, and both happen before anyone remembers to add swap.
step "swap"
bash "$ROOT/scripts/dev-swap.sh"

# -j 1 everywhere: higher parallelism gets OOM-killed on a 2 GB box.
step "cargo build (debug, custom-protocol)"
(cd src-tauri && cargo build -j 1 --features custom-protocol)

if [[ $WITH_RELEASE -eq 1 ]]; then
  step "cargo build (release, project profile)"
  (cd src-tauri && cargo build --release -j 1 --features custom-protocol)

  step "artifact"
  bash "$ROOT/scripts/package-linux.sh"
fi

step "done"
