#!/usr/bin/env bash

DISPLAY=${DISPLAY:-":0"}
DISPLAY_NUMBER=${DISPLAY#":"}

# Wait for udev
/gamer/bin/wait-udev.sh

# Clean stale lock
if [ -f /tmp/.X${DISPLAY_NUMBER}-lock ]; then
    X_PID=$(cat /tmp/.X${DISPLAY_NUMBER}-lock | tr -d ' ')
    if [ -n "$X_PID" ]; then
        kill -KILL $X_PID 2>/dev/null || true
    fi
    rm -f /tmp/.X${DISPLAY_NUMBER}-lock
fi

exec /usr/bin/Xorg $DISPLAY \
    -ac \
    -noreset \
    -novtswitch \
    +extension RANDR \
    +extension RENDER \
    +extension GLX \
    +extension XVideo \
    +extension DOUBLE-BUFFER \
    +extension SECURITY \
    +extension DAMAGE \
    +extension X-Resource \
    -extension XINERAMA -xinerama \
    +extension Composite \
    +extension COMPOSITE \
    -s off \
    -nolisten tcp \
    -verbose
