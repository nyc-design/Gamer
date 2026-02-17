#!/bin/bash
###############################################################################
# TensorDock GPU VM Deployment Script
#
# Deploys a GPU VM on TensorDock near Dallas, TX for 3DS streaming PoC testing.
# Uses TensorDock v2 API with cloud-init to auto-run setup-vm.sh on boot.
#
# Prerequisites:
#   - TENSORDOCK_API_TOKEN in .env file or environment
#   - SSH public key at ~/.ssh/id_ed25519.pub
#   - jq installed (for JSON parsing)
#   - curl installed
#
# Usage:
#   ./deploy-tensordock.sh              # Deploy with defaults (V100 in Texas)
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

# ── Load API token ───────────────────────────────────────────────────────────
load_token() {
    if [ -n "${TENSORDOCK_API_TOKEN:-}" ]; then
        return
    fi
    # Try .env in project root
    for envfile in "$SCRIPT_DIR/../../.env" "$SCRIPT_DIR/.env" "$HOME/.env"; do
        if [ -f "$envfile" ]; then
            TENSORDOCK_API_TOKEN=$(grep -E '^TENSORDOCK_API_TOKEN' "$envfile" | sed 's/.*=\s*//' | tr -d '"' | tr -d "'" || true)
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
    echo "Tip: Texas locations are closest to Dallas (~123mi)"
}

# ── Find best location near Dallas ───────────────────────────────────────────
find_texas_location() {
    local response
    response=$(curl -sf "$API_BASE/locations" -H "Accept: application/json")

    # Find Texas location with cheapest GPU that has dedicated IP
    echo "$response" | jq -r '
        [.data.locations[] |
         select(.stateprovince == "Texas" or .city == "Texas") |
         .id as $loc_id |
         .city as $city |
         .gpus[] |
         select(.network_features.dedicated_ip_available == true) |
         {
             location_id: $loc_id,
             city: $city,
             gpu_name: .v0Name,
             gpu_display: .displayName,
             price: .price_per_hr,
             max_count: .max_count,
             max_vcpus: .resources.max_vcpus,
             max_ram: .resources.max_ram_gb
         }] |
        sort_by(.price) |
        first
    '
}

# ── Deploy VM ────────────────────────────────────────────────────────────────
deploy() {
    load_token
    echo "Finding cheapest GPU near Dallas, TX..."

    local gpu_info
    gpu_info=$(find_texas_location)

    if [ -z "$gpu_info" ] || [ "$gpu_info" = "null" ]; then
        echo "No available GPUs with dedicated IP found in Texas."
        echo "Available GPUs across all locations:"
        list_gpus
        exit 1
    fi

    local location_id gpu_name gpu_display price
    location_id=$(echo "$gpu_info" | jq -r '.location_id')
    gpu_name=$(echo "$gpu_info" | jq -r '.gpu_name')
    gpu_display=$(echo "$gpu_info" | jq -r '.gpu_display')
    price=$(echo "$gpu_info" | jq -r '.price')

    echo "  GPU: $gpu_display"
    echo "  Price: \$$price/hr (GPU only)"
    echo "  Location: Texas (ID: $location_id)"
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
    cloud_init_runcmd=$(cat <<'CLOUDINIT'
[
    "apt-get update -y",
    "apt-get install -y git docker.io docker-compose-v2",
    "systemctl enable docker && systemctl start docker",
    "git clone https://github.com/nyc-design/Gamer.git /opt/gamer",
    "cd /opt/gamer/infrastructure/poc-3ds && bash setup-vm.sh --skip-driver --auto-reboot 2>&1 | tee /var/log/gamer-setup.log"
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
