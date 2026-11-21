#!/usr/bin/env bash
# Builds a Debian package from an already-built release binary and drops it in
# artifacts/ under a space-free name, with a SHA-256 next to it.
#
#   ./scripts/package-linux.sh
#
# Requires dist/ and src-tauri/target/release/duplicate-cleaner, i.e. run
# ./scripts/bootstrap.sh --release first.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

VERSION="$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")"
ARCH="$(dpkg --print-architecture)"
BIN="src-tauri/target/release/duplicate-cleaner"
DEB_DIR="src-tauri/target/release/bundle/deb"

[[ -x "$BIN" ]] || { echo "missing $BIN — run ./scripts/bootstrap.sh --release"; exit 1; }
[[ -d dist/assets ]] || { echo "missing dist/assets — run npm run build"; exit 1; }

step "tauri bundle (deb)"
npx tauri bundle --bundles deb

# Tauri names the file after productName, which carries a space. Debian tooling
# and shell one-liners are much happier without it.
SRC="$(find "$DEB_DIR" -maxdepth 1 -name '*.deb' -print -quit)"
OUT="$ROOT/artifacts/duplicate-cleaner_${VERSION}_${ARCH}.deb"
mkdir -p "$ROOT/artifacts"
cp "$SRC" "$OUT"

# The bundler rewrites the binary it ships (bundle type information), so the
# copy users get is not byte-identical to target/release. Publish the shipped
# one, otherwise the two hashes disagree for no visible reason.
step "artifact binary (as shipped inside the package)"
TMPBIN="$(mktemp -d)"
dpkg-deb -x "$OUT" "$TMPBIN"
cp "$TMPBIN/usr/bin/duplicate-cleaner" "$ROOT/artifacts/duplicate-cleaner-linux-x86_64"
rm -rf "$TMPBIN"
(cd "$ROOT/artifacts" && sha256sum duplicate-cleaner-linux-x86_64 \
   > duplicate-cleaner-linux-x86_64.sha256)

step "package metadata"
dpkg-deb -I "$OUT" | sed -n '2,$p'
echo
dpkg-deb -c "$OUT" | awk '{$1=$2=$3=$4=$5=""; sub(/^ +/, ""); print}'

if command -v desktop-file-validate >/dev/null 2>&1; then
  step "desktop-file-validate"
  TMP="$(mktemp -d)"
  dpkg-deb -R "$OUT" "$TMP/deb"
  desktop-file-validate "$TMP/deb/usr/share/applications/"*.desktop && echo "valid"
  rm -rf "$TMP"
fi

step "artifact"
(cd "$ROOT/artifacts" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256")
ls -l "$ROOT/artifacts"
