#!/usr/bin/env bash
# Makes sure the machine has swap before a release build.
#
# Why: `[profile.release]` in src-tauri/Cargo.toml sets `lto = true` and
# `codegen-units = 1`. Compiling a single crate that way (gtk is the offender)
# easily needs more than 2 GB of RAM, and with no swap the kernel kills rustc
# with SIGKILL even at `-j 1`. A 4 GB swapfile is enough to get through it.
#
#   ./scripts/dev-swap.sh          # add 4 GB swap if there is none
#   ./scripts/dev-swap.sh --remove # drop it again
#
# Non-fatal: if anything fails the script says so and exits 0, because the
# caller may still succeed on a machine with plenty of RAM.
set -uo pipefail

SIZE="${SWAP_SIZE:-4G}"
FILE="/swapfile"

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }

if [[ "${1:-}" == "--remove" ]]; then
  if sudo -n swapoff "$FILE" 2>/dev/null; then
    sudo -n rm -f "$FILE" && say "swap removed"
  else
    say "nothing to remove"
  fi
  exit 0
fi

if free --mega | awk '/^Swap:/ {exit !($2 > 0)}'; then
  say "swap already present ($(free -h | awk '/^Swap:/ {print $2}'))"
  exit 0
fi

say "adding $SIZE swap at $FILE"
if ! sudo -n fallocate -l "$SIZE" "$FILE" 2>/dev/null; then
  say "cannot create swap (no passwordless sudo or no space) — continuing without it"
  exit 0
fi
sudo -n chmod 600 "$FILE"
sudo -n mkswap "$FILE" >/dev/null
if sudo -n swapon "$FILE"; then
  say "swap active: $(free -h | awk '/^Swap:/ {print $2}')"
else
  say "swapon failed — continuing without swap"
fi
