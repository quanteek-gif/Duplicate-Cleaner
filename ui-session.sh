#!/usr/bin/env bash
# Brings up a headless desktop and runs the shipped build inside it.
#
# Needed because this sandbox keeps only /home/user: the Rust toolchain, apt
# packages and anything installed with dpkg disappear between sessions, while
# the release binary in artifacts/ survives. So UI checks start from the
# artifact, not from a rebuild.
#
#   ./scripts/ui-session.sh install   # apt runtime deps + Xvfb, install the .deb
#   ./scripts/ui-session.sh start     # start Xvfb :99 (if needed) and the app
#   ./scripts/ui-session.sh shot f.png
#   ./scripts/ui-session.sh stop
#
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
DISPLAY_NUM="${DISPLAY_NUM:-:99}"
export DISPLAY="$DISPLAY_NUM"
export WEBKIT_DISABLE_COMPOSITING_MODE=1
DEB="$(find "$ROOT/artifacts" -name '*.deb' -print -quit)"

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }

case "${1:-}" in
  install)
    say "runtime + test packages"
    sudo -n apt-get update -qq
    sudo -n apt-get install -y -qq \
      libwebkit2gtk-4.1 libgtk-3-0 libayatana-appindicator3-1 \
      xvfb xdotool xclip imagemagick xdg-utils sqlite3 desktop-file-utils >/dev/null
    say "installing $(basename "$DEB")"
    sudo -n dpkg -i "$DEB" >/dev/null
    dpkg -s duplicate-cleaner | grep -E '^(Status|Version)'
    ;;

  start)
    if ! pgrep -f "Xvfb $DISPLAY_NUM" >/dev/null; then
      say "starting Xvfb on $DISPLAY_NUM"
      setsid Xvfb "$DISPLAY_NUM" -screen 0 1440x900x24 >/tmp/xvfb.log 2>&1 &
      sleep 3
    fi
    say "starting the app"
    setsid duplicate-cleaner >/tmp/app-ui.log 2>&1 &
    sleep 9
    xdotool search --name "Duplicate Cleaner" >/dev/null && echo "window up"
    ;;

  shot)
    import -window root "${2:-/tmp/shot.png}" && echo "wrote ${2:-/tmp/shot.png}"
    ;;

  stop)
    pkill -f "^duplicate-cleaner" && say "app stopped"
    pkill -f "Xvfb $DISPLAY_NUM" && say "Xvfb stopped"
    ;;

  *)
    sed -n '2,16p' "$0"
    exit 1
    ;;
esac
