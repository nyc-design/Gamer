#!/bin/bash
###############################################################################
# TensorDock GPU VM Deployment — Multi-Emulator Gaming
#
# Generalized deployment script that supports ALL emulators (not just Azahar).
# The emulator selection happens via Wolf app profiles in config.toml —
# the user picks which emulator to launch from their Moonlight client.
#
# Two-step deployment:
#   1. provision — Create VM via TensorDock API (returns instance ID + IP)
#   2. setup    — SSH in, bootstrap host, pull images, start Wolf
#
# Prerequisites:
#   - TENSORDOCK_API_TOKEN in .env file or environment
#   - SSH public key at ~/.ssh/id_ed25519.pub
#   - jq, curl, python3 installed locally
#
# Usage:
#   ./deploy-tensordock.sh provision         # Create VM, save instance ID + IP
#   ./deploy-tensordock.sh setup [IP]        # Bootstrap host + start Wolf
#   ./deploy-tensordock.sh deploy            # provision + setup in one shot
#   ./deploy-tensordock.sh --list            # List available GPUs near Dallas
#   ./deploy-tensordock.sh --status          # Check instance status
#   ./deploy-tensordock.sh --stop [ID]       # Stop an instance
#   ./deploy-tensordock.sh --delete [ID]     # Delete an instance
#   ./deploy-tensordock.sh --ssh [IP]        # SSH into instance
#
# Environment variables:
#   TENSORDOCK_API_TOKEN        Required: TensorDock API auth
#   TENSORDOCK_SSH_PUBLIC_KEY   Optional: override SSH key detection
#   GAMER_REPO_URL              Optional: Git repo URL (default: GitHub)
#   GAMER_REPO_REF              Optional: Git branch (default: current branch)
#   WOLF_IMAGE                  Optional: wolf image (default: stock stable)
#   EMULATOR_IMAGES             Optional: space-separated list of extra images to pull
###############################################################################

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API_BASE="https://dashboard.tensordock.com/api/v2"
VM_NAME="gamer-vm"
STATE_FILE="$SCRIPT_DIR/.tensordock-instance"
GAMER_REPO_URL="${GAMER_REPO_URL:-https://github.com/nyc-design/Gamer.git}"
if git -C "$SCRIPT_DIR/../.." rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    GAMER_REPO_REF="${GAMER_REPO_REF:-$(git -C "$SCRIPT_DIR/../.." rev-parse --abbrev-ref HEAD)}"
else
    GAMER_REPO_REF="${GAMER_REPO_REF:-main}"
fi
TENSORDOCK_SSH_PUBLIC_KEY="${TENSORDOCK_SSH_PUBLIC_KEY:-${SSH_PUBLIC_KEY:-}}"

# ── Load API token ───────────────────────────────────────────────────────────
load_token() {
    if [ -n "${TENSORDOCK_API_TOKEN:-}" ]; then return; fi
    for envfile in "$SCRIPT_DIR/../../.env" "$SCRIPT_DIR/.env" "$HOME/.env"; do
        if [ -f "$envfile" ]; then
            TENSORDOCK_API_TOKEN=$(grep -E '^TENSORDOCK_API_TOKEN' "$envfile" | sed 's/.*=\s*//' | tr -d '"' | tr -d "'" || true)
            if [ -n "$TENSORDOCK_API_TOKEN" ]; then return; fi
            TENSORDOCK_API_TOKEN=$(grep -E '^TENSORDOCK_API_KEY' "$envfile" | sed 's/.*=\s*//' | tr -d '"' | tr -d "'" || true)
            if [ -n "$TENSORDOCK_API_TOKEN" ]; then return; fi
        fi
    done
    echo "ERROR: TENSORDOCK_API_TOKEN not found."
    echo "Add to .env:  TENSORDOCK_API_TOKEN=your-token"
    exit 1
}

# ── API helpers ──────────────────────────────────────────────────────────────
api_get()    { curl -sf "$API_BASE$1" -H "Authorization: Bearer $TENSORDOCK_API_TOKEN" -H "Accept: application/json"; }
api_post()   { curl -sf "$API_BASE$1" -H "Authorization: Bearer $TENSORDOCK_API_TOKEN" -H "Content-Type: application/json" -H "Accept: application/json" -d "$2"; }
api_delete() { curl -sf -X DELETE "$API_BASE$1" -H "Authorization: Bearer $TENSORDOCK_API_TOKEN" -H "Accept: application/json"; }

# ── Dependencies ─────────────────────────────────────────────────────────────
check_deps() {
    for cmd in curl jq python3; do
        if ! command -v "$cmd" &>/dev/null; then
            echo "ERROR: $cmd is required but not installed."
            exit 1
        fi
    done
}

validate_ssh_pubkey() {
    echo "$1" | grep -Eq '^ssh-(ed25519|rsa|ecdsa)[[:space:]][A-Za-z0-9+/=]+([[:space:]].*)?$'
}

# ── State file helpers ───────────────────────────────────────────────────────
save_state() {
    local id="$1" ip="${2:-}"
    echo "INSTANCE_ID=$id" > "$STATE_FILE"
    [ -n "$ip" ] && echo "IP=$ip" >> "$STATE_FILE"
}

load_state() {
    INSTANCE_ID="" ; IP=""
    if [ -f "$STATE_FILE" ]; then
        # shellcheck source=/dev/null
        source "$STATE_FILE"
    fi
}

# ── SSH helpers ──────────────────────────────────────────────────────────────
ssh_cmd() {
    local ip="$1"; shift
    ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 -o ServerAliveInterval=15 "user@$ip" "$@"
}

wait_for_ssh() {
    local ip="$1"
    echo "  Waiting for SSH on $ip..."
    for i in $(seq 1 60); do
        if ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 "user@$ip" "echo ok" >/dev/null 2>&1; then
            echo "  ✓ SSH reachable"
            return 0
        fi
        sleep 5
        [ $((i % 6)) -eq 0 ] && echo "  ... attempt $i/60"
    done
    echo "  ✗ SSH not reachable after 5 min"
    return 1
}

# ── Find closest capable GPU location near Dallas ───────────────────────────
find_best_location_for_dallas() {
    local response
    response=$(curl -sf "$API_BASE/locations" -H "Accept: application/json")

    python3 - "$response" <<'PY'
import json, math, sys

data = json.loads(sys.argv[1])
DALLAS = (32.7767, -96.7970)

CITY_COORDS = {
    ("Chubbuck", "Idaho"): (42.9207, -112.4667),
    ("Joplin", "Missouri"): (37.0842, -94.5133),
    ("Wilmington", "Delaware"): (39.7447, -75.5484),
    ("Delaware", "Wilmington"): (39.7447, -75.5484),
    ("Manassas", "Virginia"): (38.7509, -77.4753),
    ("New York City", "New York"): (40.7128, -74.0060),
    ("Orlando", "Florida"): (28.5383, -81.3792),
    ("Seattle", "Washington"): (47.6062, -122.3321),
    ("Miami", "Florida"): (25.7617, -80.1918),
    ("Chicago", "Illinois"): (41.8781, -87.6298),
    ("Los Angeles", "California"): (34.0522, -118.2437),
    ("Dallas", "Texas"): (32.7767, -96.7970),
    ("Texas", "Texas"): (32.7767, -96.7970),
    ("Houston", "Texas"): (29.7604, -95.3698),
    ("Austin", "Texas"): (30.2672, -97.7431),
    ("Atlanta", "Georgia"): (33.7490, -84.3880),
    ("Denver", "Colorado"): (39.7392, -104.9903),
    ("Phoenix", "Arizona"): (33.4484, -112.0740),
    ("Winnipeg", "Manitoba"): (49.8951, -97.1384),
    ("Tallinn", "Harjumaa"): (59.4370, 24.7536),
    ("Mölln", "Hamburg"): (53.6306, 10.6925),
    ("Wolverhampton", "Midlands"): (52.5862, -2.1289),
    ("Mischii", "Dolj"): (44.3167, 23.7958),
    ("Mumbai", "Maharashtra"): (19.0760, 72.8777),
}

CAPABLE_GPU_KEYS = [
    "l4", "rtx 3090", "rtx 4090", "rtx 4070", "rtx 4080",
    "rtx 5000 ada", "rtx 6000 ada", "rtx a4000", "rtx a5000", "rtx a6000",
    "l40s", "v100",
]

def haversine_km(a, b):
    lat1, lon1 = map(math.radians, a)
    lat2, lon2 = map(math.radians, b)
    dlat, dlon = lat2 - lat1, lon2 - lon1
    h = math.sin(dlat/2)**2 + math.cos(lat1)*math.cos(lat2)*math.sin(dlon/2)**2
    return 2 * 6371.0 * math.asin(math.sqrt(h))

def is_capable(name):
    n = (name or "").lower()
    return any(k in n for k in CAPABLE_GPU_KEYS)

candidates = []
for loc in data.get("data", {}).get("locations", []):
    city, state = loc.get("city"), loc.get("stateprovince")
    coords = CITY_COORDS.get((city, state))
    if not coords:
        continue
    dist = haversine_km(DALLAS, coords)
    for gpu in loc.get("gpus", []):
        if gpu.get("max_count", 0) < 1:
            continue
        if not gpu.get("network_features", {}).get("dedicated_ip_available", False):
            continue
        if not is_capable(gpu.get("displayName", "")):
            continue
        candidates.append({
            "location_id": loc.get("id"),
            "city": city, "state": state, "country": loc.get("country"),
            "distance_km": dist,
            "gpu_name": gpu.get("v0Name"),
            "gpu_display": gpu.get("displayName", ""),
            "price": gpu.get("price_per_hr", 9999),
            "max_count": gpu.get("max_count"),
        })

if not candidates:
    print("null")
    raise SystemExit(0)

candidates.sort(key=lambda x: (x["distance_km"], x["price"]))
print(json.dumps(candidates[0]))
PY
}

# ── List available GPUs near Dallas ──────────────────────────────────────────
list_gpus() {
    echo "Querying TensorDock for available GPUs..."
    echo ""
    local response
    response=$(curl -sf "$API_BASE/locations" -H "Accept: application/json")
    echo "$response" | jq -r '
        .data.locations[] |
        .city as $city | .stateprovince as $state | .id as $loc_id |
        .gpus[] |
        "\($city), \($state) | \(.displayName) | $\(.price_per_hr)/hr | \(.max_count) avail | DedicatedIP: \(.network_features.dedicated_ip_available) | \($loc_id)"
    ' | sort | head -30
    echo ""
}

###############################################################################
# PROVISION — Create VM via TensorDock API
###############################################################################
cmd_provision() {
    load_token
    echo "========================================="
    echo " Provisioning TensorDock Gaming VM"
    echo "========================================="
    echo ""

    # Find best GPU
    echo "Finding closest Dallas-area GPU..."
    local gpu_info
    gpu_info=$(find_best_location_for_dallas)
    if [ -z "$gpu_info" ] || [ "$gpu_info" = "null" ]; then
        echo "No suitable GPUs found near Dallas with dedicated IP."
        list_gpus
        exit 1
    fi

    local location_id gpu_name gpu_display price city state distance_km
    location_id=$(echo "$gpu_info" | jq -r '.location_id')
    gpu_name=$(echo "$gpu_info" | jq -r '.gpu_name')
    gpu_display=$(echo "$gpu_info" | jq -r '.gpu_display')
    price=$(echo "$gpu_info" | jq -r '.price')
    city=$(echo "$gpu_info" | jq -r '.city')
    state=$(echo "$gpu_info" | jq -r '.state')
    distance_km=$(echo "$gpu_info" | jq -r '.distance_km')

    echo "  GPU:      $gpu_display"
    echo "  Price:    \$$price/hr (GPU only)"
    echo "  Location: $city, $state (~${distance_km%.*} km from Dallas)"
    echo ""

    # Load SSH key
    local ssh_key=""
    if [ -n "$TENSORDOCK_SSH_PUBLIC_KEY" ]; then
        ssh_key="$(echo "$TENSORDOCK_SSH_PUBLIC_KEY" | tr -d '\r')"
    else
        for keyfile in "$HOME/.ssh/id_ed25519.pub" "$HOME/.ssh/id_rsa.pub"; do
            if [ -f "$keyfile" ]; then
                ssh_key=$(cat "$keyfile")
                break
            fi
        done
    fi
    if [ -z "$ssh_key" ]; then
        echo "ERROR: No SSH public key found."
        exit 1
    fi
    if ! validate_ssh_pubkey "$ssh_key"; then
        echo "ERROR: SSH key is not valid OpenSSH format."
        exit 1
    fi

    echo "  Creating VM: $VM_NAME (4 vCPU, 16GB RAM, 100GB storage, 1x $gpu_name)..."

    local create_payload
    create_payload=$(jq -n \
        --arg name "$VM_NAME" \
        --arg gpu "$gpu_name" \
        --arg loc "$location_id" \
        --arg ssh "$ssh_key" \
        '{
            data: {
                type: "virtualmachine",
                attributes: {
                    name: $name,
                    type: "virtualmachine",
                    image: "ubuntu2404",
                    resources: {
                        vcpu_count: 4,
                        ram_gb: 16,
                        storage_gb: 100,
                        gpus: { ($gpu): { count: 1 } }
                    },
                    location_id: $loc,
                    useDedicatedIp: true,
                    ssh_key: $ssh
                }
            }
        }')

    local result
    result=$(api_post "/instances" "$create_payload")

    if ! echo "$result" | jq -e '.data.id' &>/dev/null; then
        echo "  ✗ Failed to create VM:"
        echo "$result" | jq . 2>/dev/null || echo "$result"
        exit 1
    fi

    local instance_id
    instance_id=$(echo "$result" | jq -r '.data.id')
    echo "  ✓ VM created! Instance ID: $instance_id"
    echo ""

    # Wait for IP
    echo "  Waiting for IP address..."
    local ip_addr=""
    for i in $(seq 1 30); do
        sleep 10
        local status_result status
        status_result=$(api_get "/instances/$instance_id" 2>/dev/null || echo '{}')
        status=$(echo "$status_result" | jq -r '.data.status // .status // "unknown"' 2>/dev/null || echo "unknown")

        if [ "$status" = "running" ]; then
            ip_addr=$(echo "$status_result" | jq -r '.data.ip // .data.attributes.ip // .data.ipAddress // .ip // .ipAddress // empty' 2>/dev/null || true)
            if [ -n "$ip_addr" ]; then
                break
            fi
        fi
        echo "  ... status: $status (attempt $i/30)"
    done

    if [ -z "$ip_addr" ]; then
        save_state "$instance_id"
        echo "  ⚠ VM created but no IP yet. Check: $0 --status"
        exit 1
    fi

    save_state "$instance_id" "$ip_addr"

    echo ""
    echo "========================================="
    echo " VM Provisioned"
    echo "========================================="
    echo "  Instance: $instance_id"
    echo "  IP:       $ip_addr"
    echo "  GPU:      $gpu_display (\$$price/hr)"
    echo ""
    echo "  Next: $0 setup $ip_addr"
    echo ""
}

###############################################################################
# SETUP — SSH in, bootstrap host, pull images, start Wolf
###############################################################################
cmd_setup() {
    local ip="${1:-}"

    # Resolve IP from state file if not provided
    if [ -z "$ip" ]; then
        load_state
        ip="${IP:-}"
    fi
    if [ -z "$ip" ]; then
        echo "Usage: $0 setup <IP>"
        echo "  Or run '$0 provision' first to save the IP."
        exit 1
    fi

    echo "========================================="
    echo " Setting Up Gaming VM: $ip"
    echo "========================================="
    echo ""

    # ── Wait for SSH ──────────────────────────────────────────────────────
    if ! wait_for_ssh "$ip"; then
        echo "  ✗ Cannot reach $ip via SSH"
        exit 1
    fi

    # ── Step 1: Clone repo ────────────────────────────────────────────────
    echo "[Step 1/5] Cloning Gamer repo..."
    ssh_cmd "$ip" "
        if [ -d /opt/gamer/.git ]; then
            cd /opt/gamer && sudo git fetch --all && sudo git checkout $GAMER_REPO_REF && sudo git pull --ff-only origin $GAMER_REPO_REF || true
            echo 'Repo updated'
        else
            sudo apt-get update -y && sudo apt-get install -y git curl
            sudo git clone --branch $GAMER_REPO_REF $GAMER_REPO_URL /opt/gamer
            echo 'Repo cloned'
        fi
    "
    echo "  ✓ Repo ready at /opt/gamer ($GAMER_REPO_REF)"

    # ── Step 2: Host bootstrap ────────────────────────────────────────────
    echo "[Step 2/5] Running host bootstrap (setup-vm.sh)..."
    echo "  This installs NVIDIA driver, Docker, toolkit, creates driver volume."
    echo ""

    # setup-vm.sh is shared with poc-3ds (same host-level requirements)
    local setup_exit=0
    ssh_cmd "$ip" "cd /opt/gamer/infrastructure/poc-3ds && sudo bash setup-vm.sh --auto-reboot 2>&1" || setup_exit=$?

    if [ "$setup_exit" -ne 0 ]; then
        echo "  SSH disconnected (exit $setup_exit) — likely rebooting for driver install..."
        echo "  Waiting 60s for VM to come back..."
        sleep 60

        if ! wait_for_ssh "$ip"; then
            echo "  ✗ VM did not come back after reboot"
            exit 1
        fi

        # Wait for the continue service to finish
        echo "  Waiting for post-reboot setup to complete..."
        for i in $(seq 1 60); do
            if ssh_cmd "$ip" "grep -q 'Host Bootstrap Complete' /var/log/gamer-setup.log 2>/dev/null" 2>/dev/null; then
                echo "  ✓ Host bootstrap completed after reboot"
                break
            fi
            if [ "$i" -eq 60 ]; then
                echo "  ✗ Post-reboot setup did not complete in 10 min"
                echo "  Check: ssh user@$ip 'tail -50 /var/log/gamer-setup.log'"
                exit 1
            fi
            sleep 10
            [ $((i % 6)) -eq 0 ] && echo "  ... waiting ($i/60)"
        done
    else
        echo "  ✓ Host bootstrap completed (no reboot needed)"
    fi

    # ── Step 2.5: Create shader presets directory ─────────────────────────
    echo "  Creating shader presets directory..."
    ssh_cmd "$ip" "sudo mkdir -p /home/gamer/shaders && sudo chown user:user /home/gamer/shaders"
    echo "  ✓ /home/gamer/shaders ready (mount shaders here, or set GST_WD_SHADER_PRESET)"

    # ── Step 3: Pull/build Docker images ──────────────────────────────────
    echo "[Step 3/5] Pulling emulator images..."
    ssh_cmd "$ip" "
        set -e
        cd /opt/gamer/infrastructure/gaming-vm

        # Build Azahar (local image, not on GHCR yet)
        echo '  Building gamer/azahar:poc...'
        sudo docker build -t gamer/azahar:poc -f ../poc-3ds/azahar/Dockerfile ../poc-3ds/azahar/

        # Pull Wolf
        echo '  Pulling Wolf...'
        sudo docker pull ghcr.io/games-on-whales/wolf:stable

        # Pull all GHCR emulator images
        IMAGES=(
            ghcr.io/nyc-design/gamer-melonds:latest
            ghcr.io/nyc-design/gamer-dolphin:latest
            ghcr.io/nyc-design/gamer-citra:latest
            ghcr.io/nyc-design/gamer-ppsspp:latest
            ghcr.io/nyc-design/gamer-ryujinx:latest
            ghcr.io/nyc-design/gamer-steam:latest
        )
        for img in \"\${IMAGES[@]}\"; do
            echo \"  Pulling \$img...\"
            sudo docker pull \"\$img\" 2>/dev/null || echo \"    ⚠ Failed to pull \$img (not built yet?)\"
        done
    " 2>&1 | while IFS= read -r line; do echo "  $line"; done
    echo "  ✓ Images ready"

    # ── Step 4: Generate Wolf config + merge emulator apps ─────────────────
    echo "[Step 4/5] Generating Wolf config with all emulator apps..."
    ssh_cmd "$ip" "
        set -e
        cd /opt/gamer/infrastructure/gaming-vm

        # If Wolf already generated a config with our apps, skip regeneration
        if sudo grep -q 'Azahar' /etc/wolf/cfg/config.toml 2>/dev/null && \
           sudo grep -q 'melonDS' /etc/wolf/cfg/config.toml 2>/dev/null; then
            echo '  Wolf config already has emulator apps — skipping regeneration'
        else
            # Start Wolf briefly to generate base config
            sudo docker compose down 2>/dev/null || true
            sudo docker compose up -d wolf
            echo '  Waiting for Wolf to generate base config...'
            sleep 8
            sudo docker compose down
            echo '  Wolf stopped after config generation'

            # Merge: keep Wolf's header (gstreamer, paired_clients, uuid)
            # but replace default apps with our emulator apps
            sudo python3 -c \"
import sys

with open('/etc/wolf/cfg/config.toml') as f:
    wolf_config = f.read()

with open('/opt/gamer/infrastructure/gaming-vm/wolf/config.toml') as f:
    our_config = f.read()

# Find where Wolf's profiles start
profiles_start = wolf_config.find('[[profiles]]')
if profiles_start == -1:
    header = wolf_config.rstrip()
else:
    header = wolf_config[:profiles_start].rstrip()

# Find where our profiles/apps start
our_profiles_start = our_config.find('[[profiles]]')
if our_profiles_start == -1:
    print('ERROR: No [[profiles]] found in our config!')
    sys.exit(1)

our_apps = our_config[our_profiles_start:]

with open('/etc/wolf/cfg/config.toml', 'w') as f:
    f.write(header + chr(10) + chr(10) + our_apps)

print('  Config merged: Wolf base + all emulator apps')
\"
        fi

        # Verify apps are in config
        app_count=\$(grep -c '^\[\[profiles\.apps\]\]' /etc/wolf/cfg/config.toml || echo 0)
        echo \"  ✓ Wolf config has \$app_count app profile(s)\"
    "

    # ── Step 5: Start Wolf ────────────────────────────────────────────────
    echo "[Step 5/5] Starting Wolf..."
    ssh_cmd "$ip" "
        set -e
        cd /opt/gamer/infrastructure/gaming-vm

        # Stop existing Wolf if running
        sudo docker compose down 2>/dev/null || true

        # Start Wolf (emulators are spawned by Wolf on-demand)
        sudo docker compose up -d wolf
        echo 'Wolf started'

        # Verify
        sleep 5
        if sudo docker ps --format '{{.Names}}' | grep -qi wolf; then
            echo 'Wolf container confirmed running'
            app_count=\$(grep -c '^\[\[profiles\.apps\]\]' /etc/wolf/cfg/config.toml || echo 0)
            echo \"Wolf has \$app_count app profile(s) registered\"
        else
            echo 'WARNING: Wolf may not be running'
            sudo docker ps
            sudo docker compose logs wolf --tail=20
        fi
    "
    echo "  ✓ Wolf running"

    echo ""
    echo "========================================="
    echo " Gaming VM Ready"
    echo "========================================="
    echo ""
    echo "  IP: $ip"
    echo ""
    echo "  Available emulators (select in Moonlight):"
    echo "    - Azahar 3DS (Dual Screen)    — 3DS dual-screen"
    echo "    - Azahar 3DS (Bottom Screen)  — 3DS second screen"
    echo "    - Azahar 3DS                  — 3DS single screen"
    echo "    - melonDS (DS)                — Nintendo DS"
    echo "    - Dolphin (GC/Wii)            — GameCube/Wii"
    echo "    - Citra (3DS Legacy)          — 3DS (legacy emulator)"
    echo "    - PPSSPP (PSP)                — PlayStation Portable"
    echo "    - Ryujinx (Switch)            — Nintendo Switch"
    echo "    - Steam                       — PC Gaming"
    echo ""
    echo "  ROMs:"
    echo "    scp 'your-rom.ext' user@$ip:/home/gamer/roms/"
    echo ""
    echo "  Connect:"
    echo "    Open Moonlight → Add Host → $ip → Pair → Select emulator"
    echo ""
}

###############################################################################
# DEPLOY — provision + setup in one shot
###############################################################################
cmd_deploy() {
    cmd_provision
    load_state
    cmd_setup "${IP:-}"
}

###############################################################################
# STATUS / STOP / DELETE / SSH
###############################################################################
cmd_status() {
    load_token
    echo "TensorDock instances:"
    echo ""
    local result
    result=$(api_get "/instances")
    echo "$result" | jq -r '
        (.data[]?, .[]?) |
        select(type == "object") |
        "  \(.id) | \(.name // "unnamed") | \(.status // "unknown") | \(.ip // .ipAddress // "no ip")"
    ' 2>/dev/null || echo "  (no instances or error)"
    echo ""
    if [ -f "$STATE_FILE" ]; then
        echo "Saved state:"
        cat "$STATE_FILE"
    fi
}

cmd_stop() {
    load_token
    local id="${1:-}"
    if [ -z "$id" ]; then load_state; id="${INSTANCE_ID:-}"; fi
    if [ -z "$id" ]; then echo "Usage: $0 --stop [INSTANCE_ID]"; exit 1; fi
    echo "Stopping instance $id..."
    api_post "/instances/$id/stop" '{}' | jq . 2>/dev/null
    echo "✓ Stop requested"
}

cmd_delete() {
    load_token
    local id="${1:-}"
    if [ -z "$id" ]; then load_state; id="${INSTANCE_ID:-}"; fi
    if [ -z "$id" ]; then echo "Usage: $0 --delete [INSTANCE_ID]"; exit 1; fi
    echo "Deleting instance $id..."
    api_delete "/instances/$id" | jq . 2>/dev/null
    echo "✓ Delete requested"
    rm -f "$STATE_FILE"
}

cmd_ssh() {
    local ip="${1:-}"
    if [ -z "$ip" ]; then
        load_state
        ip="${IP:-}"
    fi
    if [ -z "$ip" ]; then
        load_token
        load_state
        if [ -n "${INSTANCE_ID:-}" ]; then
            local result
            result=$(api_get "/instances/$INSTANCE_ID" 2>/dev/null || echo '{}')
            ip=$(echo "$result" | jq -r '.data.ip // .data.attributes.ip // .data.ipAddress // empty' 2>/dev/null || true)
        fi
    fi
    if [ -z "$ip" ]; then
        echo "Usage: $0 --ssh [IP]"
        echo "  Or run '$0 provision' first."
        exit 1
    fi
    echo "Connecting to $ip..."
    ssh -o StrictHostKeyChecking=no "user@$ip"
}

###############################################################################
# Main
###############################################################################
check_deps

case "${1:-deploy}" in
    provision)  cmd_provision ;;
    setup)      cmd_setup "${2:-}" ;;
    deploy)     cmd_deploy ;;
    --list)     list_gpus ;;
    --status)   cmd_status ;;
    --stop)     cmd_stop "${2:-}" ;;
    --delete)   cmd_delete "${2:-}" ;;
    --ssh)      cmd_ssh "${2:-}" ;;
    --help|-h)
        echo "Usage: $0 <command> [args]"
        echo ""
        echo "Commands:"
        echo "  provision       Create VM via TensorDock API"
        echo "  setup [IP]      Bootstrap host + pull images + start Wolf"
        echo "  deploy          provision + setup in one shot"
        echo "  --list          List available GPUs near Dallas"
        echo "  --status        Check instance status"
        echo "  --stop [ID]     Stop instance"
        echo "  --delete [ID]   Delete instance"
        echo "  --ssh [IP]      SSH into instance"
        ;;
    *)
        echo "Unknown command: $1  (use --help)"
        exit 1
        ;;
esac
