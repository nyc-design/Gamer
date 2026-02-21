#!/usr/bin/env bash
mkdir -p $(dirname $DBUS_SYSTEM_BUS_ADDRESS | sed 's|unix:path=||')
exec dbus-daemon --system --nofork --address=$DBUS_SYSTEM_BUS_ADDRESS
