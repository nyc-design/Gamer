#!/usr/bin/env bash
set -euo pipefail

# Wrapper: run a command with gamemoderun when enabled and available.
# GAMEMODE=off disables wrapping. Default is "auto".

if [ "${GAMEMODE:-auto}" = "off" ]; then
  exec "$@"
fi

if command -v gamemoderun >/dev/null 2>&1; then
  exec gamemoderun "$@"
fi

# Ubuntu's gamemode package places this under /usr/games, which is often not in PATH.
if [ -x /usr/games/gamemoderun ]; then
  exec /usr/games/gamemoderun "$@"
fi

exec "$@"
