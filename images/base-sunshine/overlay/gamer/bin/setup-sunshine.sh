#!/usr/bin/env bash
set -e

echo "[setup-sunshine] Configuring Sunshine instances..."

# Create data dirs for both instances
mkdir -p $GAMER_DATA_DIR/sunshine-top $GAMER_DATA_DIR/sunshine-bottom
chown -R $GAMER_USER:$GAMER_USER $GAMER_DATA_DIR/sunshine-top $GAMER_DATA_DIR/sunshine-bottom

# NVFBC output indices — both on same X screen, stacked vertically
# NVFBC index 0 = first connected output (DP-0), index 1 = second (DP-2)
export SUNSHINE_TOP_OUTPUT=0
export SUNSHINE_BOTTOM_OUTPUT=1

# Xrandr display names — used by setup-screen-mode.sh
export SUNSHINE_TOP_DISPLAY=DP-0
export SUNSHINE_BOTTOM_DISPLAY=DP-2

# Template Sunshine configs
envsubst < /gamer/conf/sunshine/sunshine-top.conf.template > /gamer/conf/sunshine/sunshine-top.conf
envsubst < /gamer/conf/sunshine/sunshine-bottom.conf.template > /gamer/conf/sunshine/sunshine-bottom.conf

# Copy apps.json
cp /gamer/conf/sunshine/apps.json /gamer/conf/sunshine/apps-top.json
cp /gamer/conf/sunshine/apps.json /gamer/conf/sunshine/apps-bottom.json

chown -R $GAMER_USER:$GAMER_USER /gamer/conf/sunshine/
