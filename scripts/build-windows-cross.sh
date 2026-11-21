#!/usr/bin/env bash
# Cross-build the full Windows x64 app and NSIS setup from Linux/macOS with
# cargo-xwin. MSI and native-Windows installation testing still require Windows.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
TARGET=x86_64-pc-windows-msvc

for tool in node npm rustup cargo makensis clang lld-link; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "Missing prerequisite: $tool. See PUBLISHING.md → Linux cross-build." >&2
    exit 1
  }
done

rustup target add "$TARGET"
if ! cargo xwin --version >/dev/null 2>&1; then
  cargo install --locked cargo-xwin
fi
if [[ ! -d node_modules ]]; then npm ci; fi

npx tauri build --runner cargo-xwin --target "$TARGET" --bundles nsis

BIN="$ROOT/src-tauri/target/$TARGET/release/duplicate-cleaner.exe"
SETUP="$ROOT/src-tauri/target/$TARGET/release/bundle/nsis/Duplicate Cleaner_1.0.0_x64-setup.exe"
ASSET=$(find "$ROOT/dist/assets" -maxdepth 1 -name 'index-*.js' -print -quit)
[[ -s "$BIN" ]] || { echo "Missing Windows app: $BIN" >&2; exit 1; }
[[ -s "$SETUP" ]] || { echo "Missing NSIS setup: $SETUP" >&2; exit 1; }
[[ -n "$ASSET" ]] || { echo "Missing built UI asset in dist/assets" >&2; exit 1; }

python3 - "$BIN" "$ASSET" <<'PY'
from pathlib import Path
import struct, sys
exe, asset = map(Path, sys.argv[1:])
data = exe.read_bytes()
if data[:2] != b"MZ":
    raise SystemExit("not a PE executable")
pe = struct.unpack_from("<I", data, 0x3c)[0]
if data[pe:pe + 4] != b"PE\0\0":
    raise SystemExit("invalid PE signature")
machine = struct.unpack_from("<H", data, pe + 4)[0]
if machine != 0x8664:
    raise SystemExit(f"expected x64 PE (0x8664), got 0x{machine:04x}")
if asset.name.encode() not in data:
    raise SystemExit(f"embedded UI asset missing from executable: {asset.name}")
print(f"verified x64 Windows PE with embedded UI: {asset.name}")
PY

OUT="$ROOT/artifacts/windows"
mkdir -p "$OUT"
cp "$BIN" "$OUT/DuplicateCleaner-1.0.0-win-x64-portable.exe"
cp "$SETUP" "$OUT/DuplicateCleaner-1.0.0-win-x64-setup.exe"
(
  cd "$OUT"
  sha256sum DuplicateCleaner-1.0.0-win-x64-portable.exe DuplicateCleaner-1.0.0-win-x64-setup.exe \
    | tee SHA256SUMS.txt
)
printf '\nWindows artifacts:\n'
ls -lh "$OUT"/*.exe "$OUT/SHA256SUMS.txt"
