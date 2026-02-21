#!/bin/bash
# Deploy the input-remap-proxy to the running Sunshine container on the VM.
# This is a hot-deploy for testing — for production, rebuild the image.
#
# Usage: ./deploy-input-remap.sh [VM_IP]
set -euo pipefail

VM_IP="${1:-206.168.81.17}"
SSH_KEY="$HOME/.ssh/id_ed25519"
SSH="ssh -i $SSH_KEY -o StrictHostKeyChecking=no user@$VM_IP"
SCP="scp -i $SSH_KEY -o StrictHostKeyChecking=no"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

echo "=== Deploying input-remap-proxy to $VM_IP ==="

# Find the running Sunshine container
CONTAINER=$($SSH "sudo docker ps --format '{{.Names}}' | head -1" 2>/dev/null || echo "")
if [ -z "$CONTAINER" ]; then
    echo "ERROR: No running container found on VM"
    exit 1
fi
echo "Target container: $CONTAINER"

# Upload the proxy script
echo "[1/4] Uploading input-remap-proxy.py..."
$SCP "$REPO_DIR/images/base-sunshine/overlay/gamer/bin/input-remap-proxy.py" \
    "user@$VM_IP:/tmp/input-remap-proxy.py"

# Copy into container
echo "[2/4] Installing in container..."
$SSH "sudo docker cp /tmp/input-remap-proxy.py $CONTAINER:/gamer/bin/input-remap-proxy.py"

# Install python3-evdev in container
echo "[3/4] Installing python3-evdev..."
$SSH "sudo docker exec $CONTAINER bash -c 'apt-get update -qq && apt-get install -y -qq python3-evdev 2>&1 | tail -2'"

# Reset any existing CTM transforms to identity
echo "[4/4] Resetting CTM and starting proxy..."
$SSH "sudo docker exec $CONTAINER bash -c '
# Reset all CTM transforms to identity
export DISPLAY=:0
for id in \$(xinput list 2>/dev/null | grep -iE \"passthrough|sunshine\" | grep -oP \"id=\\K\\d+\"); do
    xinput set-prop \$id \"Coordinate Transformation Matrix\" 1 0 0 0 1 0 0 0 1 2>/dev/null || true
done
echo \"CTM reset to identity on all devices\"

# Kill any existing proxy
pkill -f input-remap-proxy.py 2>/dev/null || true
sleep 1

# Start proxy in background
nohup python3 /gamer/bin/input-remap-proxy.py -v > /gamer/log/input-remap.log 2>&1 &
echo \"Proxy started (PID: \$!)\"
'"

echo ""
echo "=== Deployment complete ==="
echo "Monitor logs: ssh -i $SSH_KEY user@$VM_IP 'sudo docker exec $CONTAINER tail -f /gamer/log/input-remap.log'"
echo "Check devices: ssh -i $SSH_KEY user@$VM_IP 'sudo docker exec $CONTAINER bash -c \"DISPLAY=:0 xinput list\"'"
