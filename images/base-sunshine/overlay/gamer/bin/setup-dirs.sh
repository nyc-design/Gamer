#!/usr/bin/env bash
set -e

echo "[setup-dirs] Setting up directories and permissions..."

# Ensure uinput is accessible for Sunshine virtual input
chmod 0666 /dev/uinput 2>/dev/null || true

mkdir -p $GAMER_DATA_DIR $GAMER_LOG_DIR $XDG_RUNTIME_DIR \
         $XDG_CACHE_HOME $XDG_CONFIG_HOME $XDG_DATA_HOME $GAMER_HOME

# Fix ownership — bind mounts may have files from different UIDs
chown -R $GAMER_USER:$GAMER_USER \
    $GAMER_DATA_DIR $GAMER_LOG_DIR $XDG_RUNTIME_DIR \
    $XDG_CACHE_HOME $XDG_CONFIG_HOME $XDG_DATA_HOME $GAMER_HOME

chmod 0700 \
    $GAMER_DATA_DIR $GAMER_LOG_DIR $XDG_RUNTIME_DIR \
    $XDG_CACHE_HOME $XDG_CONFIG_HOME $XDG_DATA_HOME
