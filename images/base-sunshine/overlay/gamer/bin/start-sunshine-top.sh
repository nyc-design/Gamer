#!/usr/bin/env bash

# Wait for X server
/gamer/bin/wait-x.sh

# Set credentials on first run
if [ ! -f "$GAMER_DATA_DIR/sunshine-top/credentials.json" ]; then
    if [ -n "$SUNSHINE_PASSWORD_BASE64" ] && [ -n "$SUNSHINE_USERNAME" ]; then
        SUNSHINE_PASSWORD=$(echo "$SUNSHINE_PASSWORD_BASE64" | base64 -d)
        sunshine /gamer/conf/sunshine/sunshine-top.conf --creds "$SUNSHINE_USERNAME" "$SUNSHINE_PASSWORD"
    fi
fi

exec sunshine /gamer/conf/sunshine/sunshine-top.conf
