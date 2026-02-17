#!/bin/bash
###############################################################################
# TensorDock GPU VM Deployment Script
#
# Deploys a GPU VM on TensorDock closest to Dallas, TX for 3DS streaming PoC
# testing, picking a GPU tier that is comfortably capable of Azahar at 5x.
# Uses TensorDock v2 API with cloud-init to auto-run setup-vm.sh on boot.
#
# Prerequisites:
#   - TENSORDOCK_API_TOKEN in .env file or environment
#   - SSH public key at ~/.ssh/id_ed25519.pub
#   - jq installed (for JSON parsing)
#   - curl installed
#
# Usage:
#   ./deploy-tensordock.sh              # Deploy with defaults (closest capable GPU to Dallas)
#   ./deploy-tensordock.sh --list       # List available GPUs near Dallas
#   ./deploy-tensordock.sh --status     # Check status of running instances
#   ./deploy-tensordock.sh --stop ID    # Stop an instance
#   ./deploy-tensordock.sh --delete ID  # Delete an instance
#   ./deploy-tensordock.sh --ssh        # SSH into the most recent instance
###############################################################################

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API_BASE="https://dashboard.tensordock.com/api/v2"
VM_NAME="gamer-3ds-poc"
GAMER_REPO_URL="${GAMER_REPO_URL:-https://github.com/nyc-design/Gamer.git}"
if git -C "$SCRIPT_DIR/../.." rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    GAMER_REPO_REF="${GAMER_REPO_REF:-$(git -C "$SCRIPT_DIR/../.." rev-parse --abbrev-ref HEAD)}"
else
    GAMER_REPO_REF="${GAMER_REPO_REF:-main}"
fi
WOLF_DUAL_GST_WD_REPO="${WOLF_DUAL_GST_WD_REPO:-https://github.com/nyc-design/gst-wayland-display.git}"
WOLF_DUAL_GST_WD_BRANCH="${WOLF_DUAL_GST_WD_BRANCH:-multi-output}"

# ── Load API token ───────────────────────────────────────────────────────────
load_token() {
    if [ -n "${TENSORDOCK_API_TOKEN:-}" ]; then
        return
    fi
    # Try .env in project root (check both TOKEN and KEY variants)
    for envfile in "$SCRIPT_DIR/../../.env" "$SCRIPT_DIR/.env" "$HOME/.env"; do
        if [ -f "$envfile" ]; then
            TENSORDOCK_API_TOKEN=$(grep -E '^TENSORDOCK_API_TOKEN' "$envfile" | sed 's/.*=\s*//' | tr -d '"' | tr -d "'" || true)
            if [ -n "$TENSORDOCK_API_TOKEN" ]; then
                return
            fi
            # Fall back to API_KEY (works as Bearer token on v2 API)
            TENSORDOCK_API_TOKEN=$(grep -E '^TENSORDOCK_API_KEY' "$envfile" | sed 's/.*=\s*//' | tr -d '"' | tr -d "'" || true)
            if [ -n "$TENSORDOCK_API_TOKEN" ]; then
                return
            fi
        fi
    done
    echo "ERROR: TENSORDOCK_API_TOKEN not found."
    echo "Add it to your .env file:"
    echo "  TENSORDOCK_API_TOKEN=your-bearer-token-here"
    echo ""
    echo "Get your token from: https://dashboard.tensordock.com (Developer Settings)"
    exit 1
}

# ── API helpers ──────────────────────────────────────────────────────────────
api_get() {
    curl -sf "$API_BASE$1" \
        -H "Authorization: Bearer $TENSORDOCK_API_TOKEN" \
        -H "Accept: application/json"
}

api_post() {
    curl -sf "$API_BASE$1" \
        -H "Authorization: Bearer $TENSORDOCK_API_TOKEN" \
        -H "Content-Type: application/json" \
        -H "Accept: application/json" \
        -d "$2"
}

api_delete() {
    curl -sf -X DELETE "$API_BASE$1" \
        -H "Authorization: Bearer $TENSORDOCK_API_TOKEN" \
        -H "Accept: application/json"
}

# ── Check dependencies ───────────────────────────────────────────────────────
check_deps() {
    for cmd in curl jq; do
        if ! command -v "$cmd" &>/dev/null; then
            echo "ERROR: $cmd is required but not installed."
            exit 1
        fi
    done
}

# ── List available GPUs near Dallas ──────────────────────────────────────────
list_gpus() {
    echo "Querying TensorDock for available GPUs..."
    echo ""

    # Locations endpoint is public (no auth needed)
    local response
    response=$(curl -sf "$API_BASE/locations" -H "Accept: application/json")

    echo "$response" | jq -r '
        .data.locations[] |
        .city as $city |
        .stateprovince as $state |
        .id as $loc_id |
        .gpus[] |
        "\($city), \($state) | \(.displayName) | $\(.price_per_hr)/hr GPU | \(.max_count) avail | Dedicated IP: \(.network_features.dedicated_ip_available) | Location: \($loc_id)"
    ' | sort | head -20

    echo ""
    echo "Tip: Deployment auto-selects nearest Dallas-proximate capable GPU"
}

# ── Find closest capable location near Dallas ───────────────────────────────
find_best_location_for_dallas() {
    local response
    response=$(curl -sf "$API_BASE/locations" -H "Accept: application/json")

    # We prioritize distance to Dallas first, then price.
    # "Capable" means GPUs strong enough for Azahar @ 5x on this PoC.
    python3 - "$response" <<'PY'
import json, math, sys

data = json.loads(sys.argv[1])

# Dallas, TX
DALLAS = (32.7767, -96.7970)

# Hand-curated coordinates for common TensorDock US locations.
# (Avoids runtime geocoder dependency for automation reliability.)
CITY_COORDS = {
    ("Chubbuck", "Idaho"): (42.9207, -112.4667),
    ("Joplin", "Missouri"): (37.0842, -94.5133),
    ("Wilmington", "Delaware"): (39.7447, -75.5484),
    ("Manassas", "Virginia"): (38.7509, -77.4753),
    ("New York City", "New York"): (40.7128, -74.0060),
    ("Orlando", "Florida"): (28.5383, -81.3792),
    ("Seattle", "Washington"): (47.6062, -122.3321),
    ("Miami", "Florida"): (25.7617, -80.1918),
    ("Chicago", "Illinois"): (41.8781, -87.6298),
    ("Los Angeles", "California"): (34.0522, -118.2437),
}

# GPU names considered comfortably capable for Azahar 5x.
CAPABLE_GPU_KEYS = [
    "l4",
    "rtx 3090",
    "rtx 4090",
    "rtx 4070",
    "rtx 4080",
    "rtx 5000 ada",
    "rtx 6000 ada",
    "rtx a4000",
    "rtx a5000",
    "rtx a6000",
    "l40s",
    "v100",
]

def haversine_km(a, b):
    lat1, lon1 = map(math.radians, a)
    lat2, lon2 = map(math.radians, b)
    dlat = lat2 - lat1
    dlon = lon2 - lon1
    h = math.sin(dlat / 2) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2) ** 2
    return 2 * 6371.0 * math.asin(math.sqrt(h))

def is_capable(name):
    n = (name or "").lower()
    return any(k in n for k in CAPABLE_GPU_KEYS)

candidates = []
for loc in data.get("data", {}).get("locations", []):
    city = loc.get("city")
    state = loc.get("stateprovince")
    country = loc.get("country")
    coords = CITY_COORDS.get((city, state))
    if not coords:
        # Skip unknown geos rather than guessing incorrectly in automation.
        continue
    dist = haversine_km(DALLAS, coords)
    for gpu in loc.get("gpus", []):
        if gpu.get("max_count", 0) < 1:
            continue
        if not gpu.get("network_features", {}).get("dedicated_ip_available", False):
            continue
        display = gpu.get("displayName", "")
        if not is_capable(display):
            continue
        candidates.append({
            "location_id": loc.get("id"),
            "city": city,
            "state": state,
            "country": country,
            "distance_km": dist,
            "gpu_name": gpu.get("v0Name"),
            "gpu_display": display,
            "price": gpu.get("price_per_hr", 9999),
            "max_count": gpu.get("max_count"),
            "max_vcpus": gpu.get("resources", {}).get("max_vcpus"),
            "max_ram": gpu.get("resources", {}).get("max_ram_gb"),
        })

if not candidates:
    print("null")
    raise SystemExit(0)

# Closest first, then cheaper.
candidates.sort(key=lambda x: (x["distance_km"], x["price"]))
print(json.dumps(candidates[0]))
PY
}

# ── Deploy VM ────────────────────────────────────────────────────────────────
deploy() {
    load_token
    echo "Finding closest Dallas-area GPU capable of Azahar @ 5x..."

    local gpu_info
    gpu_info=$(find_best_location_for_dallas)

    if [ -z "$gpu_info" ] || [ "$gpu_info" = "null" ]; then
        echo "No suitable GPUs found near Dallas with dedicated IP."
        echo "Available GPUs across all locations:"
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

    echo "  GPU: $gpu_display"
    echo "  Price: \$$price/hr (GPU only)"
    echo "  Location: $city, $state (~${distance_km%.*} km from Dallas) (ID: $location_id)"
    echo ""

    # Load SSH key
    local ssh_key=""
    for keyfile in "$HOME/.ssh/id_ed25519.pub" "$HOME/.ssh/id_rsa.pub"; do
        if [ -f "$keyfile" ]; then
            ssh_key=$(cat "$keyfile")
            echo "  SSH key: $keyfile"
            break
        fi
    done
    if [ -z "$ssh_key" ]; then
        echo "ERROR: No SSH public key found at ~/.ssh/id_ed25519.pub or ~/.ssh/id_rsa.pub"
        exit 1
    fi

    # Build cloud-init that clones repo and runs setup
    # Note: setup-vm.sh --skip-driver because TensorDock has NVIDIA drivers pre-installed
    local cloud_init_runcmd
    cloud_init_runcmd=$(cat <<CLOUDINIT
[
    "apt-get update -y",
    "apt-get install -y git curl docker.io docker-compose-plugin docker-compose-v2 || apt-get install -y git curl docker.io docker-compose-plugin || apt-get install -y git curl docker.io docker-compose",
    "systemctl enable docker && systemctl start docker",
    "if [ ! -d /opt/gamer/.git ]; then git clone $GAMER_REPO_URL /opt/gamer; fi",
    "cd /opt/gamer && git fetch --all --tags --prune",
    "cd /opt/gamer && git checkout $GAMER_REPO_REF || git checkout -b $GAMER_REPO_REF origin/$GAMER_REPO_REF || true",
    "cd /opt/gamer && git pull --ff-only origin $GAMER_REPO_REF || true",
    "cd /opt/gamer/infrastructure/poc-3ds && ENABLE_DUAL_WOLF_BUILD=1 WOLF_DUAL_GST_WD_REPO='$WOLF_DUAL_GST_WD_REPO' WOLF_DUAL_GST_WD_BRANCH='$WOLF_DUAL_GST_WD_BRANCH' bash setup-vm.sh --skip-driver --auto-reboot 2>&1 | tee /var/log/gamer-setup.log"
]
CLOUDINIT
)

    echo "  Creating VM: $VM_NAME (4 vCPU, 16GB RAM, 100GB storage, 1x $gpu_name)..."
    echo ""

    local create_payload
    create_payload=$(jq -n \
        --arg name "$VM_NAME" \
        --arg gpu "$gpu_name" \
        --arg loc "$location_id" \
        --arg ssh "$ssh_key" \
        --argjson runcmd "$cloud_init_runcmd" \
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
                        gpus: {
                            ($gpu): { count: 1 }
                        }
                    },
                    location_id: $loc,
                    useDedicatedIp: true,
                    ssh_key: $ssh,
                    cloud_init: {
                        runcmd: $runcmd
                    }
                }
            }
        }')

    local result
    result=$(api_post "/instances" "$create_payload")

    if echo "$result" | jq -e '.data.id' &>/dev/null; then
        local instance_id ip_addr
        instance_id=$(echo "$result" | jq -r '.data.id')
        echo "  ✓ VM created! Instance ID: $instance_id"
        echo ""

        # Save instance ID for later
        echo "$instance_id" > "$SCRIPT_DIR/.tensordock-instance-id"

        # Poll for IP address
        echo "  Waiting for VM to get an IP address..."
        for i in $(seq 1 30); do
            sleep 10
            local status_result
            status_result=$(api_get "/instances/$instance_id" 2>/dev/null || echo '{}')
            local status
            status=$(echo "$status_result" | jq -r '.data.status // "unknown"' 2>/dev/null || echo "unknown")

            if [ "$status" = "running" ]; then
                ip_addr=$(echo "$status_result" | jq -r '.data.ip // .data.attributes.ip // empty' 2>/dev/null || true)
                if [ -n "$ip_addr" ]; then
                    echo ""
                    echo "========================================="
                    echo " VM Running!"
                    echo "========================================="
                    echo ""
                    echo " Instance ID: $instance_id"
                    echo " IP Address:  $ip_addr"
                    echo " GPU:         $gpu_display"
                    echo " Cost:        ~\$$price/hr (GPU) + compute"
                    echo ""
                    echo " SSH:  ssh root@$ip_addr"
                    echo ""
                    echo " Check setup progress:"
                    echo "   ssh root@$ip_addr 'tail -f /var/log/gamer-setup.log'"
                    echo ""
                    echo " Once setup completes (~5-10 min):"
                    echo "   1. SCP your ROM:"
                    echo "      scp 'Pokemon Alpha Sapphire*.3ds' root@$ip_addr:/home/gamer/roms/pokemon-alpha-sapphire.3ds"
                    echo "   2. Open Moonlight → Add Host → $ip_addr"
                    echo "   3. Pair and play!"
                    echo ""
                    echo " Stop VM (pauses billing except storage):"
                    echo "   $0 --stop $instance_id"
                    echo ""
                    echo " Delete VM (stops all billing):"
                    echo "   $0 --delete $instance_id"
                    echo ""
                    exit 0
                fi
            fi
            echo "  ... status: $status (attempt $i/30)"
        done

        echo "  ⚠ VM created but IP not ready yet. Check status:"
        echo "     $0 --status"
    else
        echo "  ✗ Failed to create VM:"
        echo "$result" | jq . 2>/dev/null || echo "$result"
        exit 1
    fi
}

# ── Status ───────────────────────────────────────────────────────────────────
status() {
    load_token
    echo "TensorDock instances:"
    echo ""
    local result
    result=$(api_get "/instances")
    echo "$result" | jq -r '
        .data[]? |
        "  \(.id) | \(.name // "unnamed") | \(.status) | \(.ip // "no ip")"
    ' 2>/dev/null || echo "  (no instances or error)"
    echo ""
}

# ── Stop ─────────────────────────────────────────────────────────────────────
stop_instance() {
    load_token
    local id="$1"
    echo "Stopping instance $id..."
    api_post "/instances/$id/stop" '{}' | jq . 2>/dev/null
    echo "✓ Stop requested"
}

# ── Delete ───────────────────────────────────────────────────────────────────
delete_instance() {
    load_token
    local id="$1"
    echo "Deleting instance $id..."
    api_delete "/instances/$id" | jq . 2>/dev/null
    echo "✓ Delete requested"
    rm -f "$SCRIPT_DIR/.tensordock-instance-id"
}

# ── SSH ──────────────────────────────────────────────────────────────────────
ssh_instance() {
    load_token
    local id=""
    if [ -f "$SCRIPT_DIR/.tensordock-instance-id" ]; then
        id=$(cat "$SCRIPT_DIR/.tensordock-instance-id")
    else
        echo "No saved instance ID. Use --status to find your instance."
        exit 1
    fi
    local result
    result=$(api_get "/instances/$id")
    local ip
    ip=$(echo "$result" | jq -r '.data.ip // .data.attributes.ip // empty' 2>/dev/null || true)
    if [ -n "$ip" ]; then
        echo "Connecting to $ip..."
        ssh -o StrictHostKeyChecking=no "root@$ip"
    else
        echo "Could not determine IP for instance $id"
        echo "$result" | jq . 2>/dev/null
    fi
}

# ── Main ─────────────────────────────────────────────────────────────────────
check_deps

case "${1:-deploy}" in
    --list)     list_gpus ;;
    --status)   status ;;
    --stop)     stop_instance "${2:?Usage: $0 --stop INSTANCE_ID}" ;;
    --delete)   delete_instance "${2:?Usage: $0 --delete INSTANCE_ID}" ;;
    --ssh)      ssh_instance ;;
    deploy|*)   deploy ;;
esac
