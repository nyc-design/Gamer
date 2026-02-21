#!/usr/bin/env bash

# Skip if dual screen is disabled
if [ "${DUAL_SCREEN:-1}" != "1" ]; then
    echo "[sunshine-bottom] DUAL_SCREEN=${DUAL_SCREEN}, not starting bottom instance."
    sleep infinity
fi

# Wait for X server
/gamer/bin/wait-x.sh

# Set credentials on first run
if [ ! -f "$GAMER_DATA_DIR/sunshine-bottom/credentials.json" ]; then
    if [ -n "$SUNSHINE_PASSWORD_BASE64" ] && [ -n "$SUNSHINE_USERNAME" ]; then
        SUNSHINE_PASSWORD=$(echo "$SUNSHINE_PASSWORD_BASE64" | base64 -d)
        sunshine /gamer/conf/sunshine/sunshine-bottom.conf --creds "$SUNSHINE_USERNAME" "$SUNSHINE_PASSWORD"
    fi
fi

exec sunshine /gamer/conf/sunshine/sunshine-bottom.conf
