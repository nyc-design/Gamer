#!/bin/bash
# Deploy dual-screen fixes to the TensorDock VM
# Usage: ./deploy-fixes.sh [VM_IP]
set -euo pipefail

VM_IP="${1:-206.168.81.17}"
SSH_KEY="$HOME/.ssh/id_ed25519"
SSH="ssh -i $SSH_KEY -o StrictHostKeyChecking=no user@$VM_IP"
SCP="scp -i $SSH_KEY -o StrictHostKeyChecking=no"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "=== Deploying dual-screen fixes to $VM_IP ==="

# 1. Upload fixed source files + Dockerfile
echo "[1/5] Uploading fixed source files..."
$SCP "$SCRIPT_DIR/Dockerfile.fix" "user@$VM_IP:/tmp/gst-wd-xwayland/Dockerfile.fix"
$SCP "$SCRIPT_DIR/comp_mod.rs" "user@$VM_IP:/tmp/gst-wd-xwayland/wayland-display-core/src/comp/mod.rs"
$SCP "$SCRIPT_DIR/waylandsecondary_imp.rs" "user@$VM_IP:/tmp/gst-wd-xwayland/gst-plugin-wayland-display/src/waylandsecondary/imp.rs"
$SCP "$SCRIPT_DIR/waylandsrc_imp.rs" "user@$VM_IP:/tmp/gst-wd-xwayland/gst-plugin-wayland-display/src/waylandsrc/imp.rs"
$SCP "$SCRIPT_DIR/handlers_compositor.rs" "user@$VM_IP:/tmp/gst-wd-xwayland/wayland-display-core/src/wayland/handlers/compositor.rs"
$SCP "$SCRIPT_DIR/handlers_x11.rs" "user@$VM_IP:/tmp/gst-wd-xwayland/wayland-display-core/src/wayland/handlers/x11.rs"
$SCP "$SCRIPT_DIR/gst_plugin_Cargo.toml" "user@$VM_IP:/tmp/gst-wd-xwayland/gst-plugin-wayland-display/Cargo.toml"

# 2. Upload fixed Wolf config
echo "[2/5] Uploading Wolf config..."
$SSH "sudo cp /etc/wolf/cfg/config.toml /etc/wolf/cfg/config.toml.bak"
$SCP "$SCRIPT_DIR/wolf-config.toml" "user@$VM_IP:/tmp/wolf-config.toml"
$SSH "sudo cp /tmp/wolf-config.toml /etc/wolf/cfg/config.toml"

# 3. Stop Wolf and clean up
echo "[3/5] Stopping Wolf and cleaning up..."
$SSH "sudo docker stop wolf-dual 2>/dev/null || true; sudo docker rm wolf-dual 2>/dev/null || true; sudo killall Xwayland 2>/dev/null || true; sudo docker rm -f \$(sudo docker ps -aq) 2>/dev/null || true"
sleep 2

# 4. Rebuild Docker image
echo "[4/5] Rebuilding Docker image (this takes ~90 seconds)..."
$SSH "cd /tmp/gst-wd-xwayland && sudo docker build --no-cache -f Dockerfile.fix -t wolf-dual-local:fixed . 2>&1 | tail -5"

# 5. Start Wolf
echo "[5/5] Starting Wolf..."
$SSH "sudo docker run -d --name wolf-dual --network host --privileged --runtime nvidia \
  -e NVIDIA_VISIBLE_DEVICES=all -e NVIDIA_DRIVER_CAPABILITIES=all \
  -e XDG_RUNTIME_DIR=/tmp/sockets -e HOST_APPS_STATE_FOLDER=/etc/wolf \
  -e WOLF_CFG_FILE=/etc/wolf/cfg/config.toml -e WOLF_LOG_LEVEL=DEBUG \
  -e WOLF_RENDER_NODE=/dev/dri/renderD128 \
  -e GST_WD_MULTI_OUTPUT=1 -e GST_WD_SECONDARY_SINK_NAME=secondary_video \
  -e GST_DEBUG='waylanddisplaysrc:6,waylanddisplaysecondary:6,interpipe*:4' \
  -e GST_REGISTRY_FORK=no \
  -v /etc/wolf/cfg:/etc/wolf/cfg:rw -v /var/run/docker.sock:/var/run/docker.sock:rw \
  -v /home/gamer:/home/gamer:rw -v /tmp/sockets:/tmp/sockets:rw \
  -v /dev/shm:/dev/shm:rw -v /dev/input:/dev/input:rw \
  -v nvidia-driver-vol:/usr/nvidia:rw \
  wolf-dual-local:fixed"

sleep 3
echo ""
echo "=== Deployment complete ==="
echo "Wolf is running. Connect Moonlight clients to $VM_IP"
echo ""
echo "Check logs: ssh -i $SSH_KEY user@$VM_IP 'sudo docker logs wolf-dual 2>&1 | grep -v \"Key code\" | tail -20'"
