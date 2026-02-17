#!/bin/bash
set -euo pipefail

###############################################################################
# setup-rclone.sh
#
# Configures rclone remotes for:
#   - Cloudflare R2 (ROMs)
#   - Google Cloud Storage (saves/configs/firmware/steam)
#
# Usage:
#   sudo ./setup-rclone.sh
#
# Required env (R2):
#   R2_ACCOUNT_ID
#   R2_ACCESS_KEY_ID
#   R2_SECRET_ACCESS_KEY
#
# Optional env:
#   R2_ENDPOINT                        (default: https://<account>.r2.cloudflarestorage.com)
#   R2_BUCKET_NAME                     (default: gamer-roms)
#   GCS_BUCKET_NAME                    (default: gamer-data)
#   GCS_SERVICE_ACCOUNT_JSON           (raw json)
#   GCS_SERVICE_ACCOUNT_JSON_B64       (base64-encoded json)
#   GCS_SERVICE_ACCOUNT_FILE           (path to existing json key file)
#
# If no GCS_* credential env is provided, only the R2 remote is configured.
###############################################################################

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RCLONE_CONF_DIR="/etc/rclone"
RCLONE_CONF="${RCLONE_CONF_DIR}/rclone.conf"
SECRETS_DIR="/etc/gamer"
GCS_KEY_FILE="${SECRETS_DIR}/gcs-service-account.json"

R2_BUCKET_NAME="${R2_BUCKET_NAME:-gamer-roms}"
GCS_BUCKET_NAME="${GCS_BUCKET_NAME:-gamer-data}"

if ! command -v rclone >/dev/null 2>&1; then
  echo "[rclone] Installing rclone..."
  curl -fsSL https://rclone.org/install.sh | bash
fi

if [ -z "${R2_ACCOUNT_ID:-}" ] || [ -z "${R2_ACCESS_KEY_ID:-}" ] || [ -z "${R2_SECRET_ACCESS_KEY:-}" ]; then
  echo "ERROR: Missing required R2 credentials."
  echo "Required: R2_ACCOUNT_ID, R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY"
  exit 1
fi

R2_ENDPOINT="${R2_ENDPOINT:-https://${R2_ACCOUNT_ID}.r2.cloudflarestorage.com}"

mkdir -p "${RCLONE_CONF_DIR}" "${SECRETS_DIR}"
chmod 700 "${RCLONE_CONF_DIR}" "${SECRETS_DIR}"

GCS_FILE_TO_USE=""
if [ -n "${GCS_SERVICE_ACCOUNT_FILE:-}" ] && [ -f "${GCS_SERVICE_ACCOUNT_FILE}" ]; then
  GCS_FILE_TO_USE="${GCS_SERVICE_ACCOUNT_FILE}"
elif [ -n "${GCS_SERVICE_ACCOUNT_JSON_B64:-}" ]; then
  echo "${GCS_SERVICE_ACCOUNT_JSON_B64}" | base64 -d > "${GCS_KEY_FILE}"
  chmod 600 "${GCS_KEY_FILE}"
  GCS_FILE_TO_USE="${GCS_KEY_FILE}"
elif [ -n "${GCS_SERVICE_ACCOUNT_JSON:-}" ]; then
  printf "%s" "${GCS_SERVICE_ACCOUNT_JSON}" > "${GCS_KEY_FILE}"
  chmod 600 "${GCS_KEY_FILE}"
  GCS_FILE_TO_USE="${GCS_KEY_FILE}"
fi

cat > "${RCLONE_CONF}" <<EOF
[r2]
type = s3
provider = Cloudflare
access_key_id = ${R2_ACCESS_KEY_ID}
secret_access_key = ${R2_SECRET_ACCESS_KEY}
endpoint = ${R2_ENDPOINT}
acl = private
no_check_bucket = true
EOF

if [ -n "${GCS_FILE_TO_USE}" ]; then
cat >> "${RCLONE_CONF}" <<EOF

[gcs]
type = google cloud storage
service_account_file = ${GCS_FILE_TO_USE}
bucket_policy_only = true
EOF
fi

chmod 600 "${RCLONE_CONF}"

echo "[rclone] Config written to ${RCLONE_CONF}"
echo "[rclone] Validating remotes..."
rclone --config "${RCLONE_CONF}" listremotes

echo "[rclone] Testing R2 remote access..."
rclone --config "${RCLONE_CONF}" lsd "r2:${R2_BUCKET_NAME}" >/dev/null
echo "  ✓ R2 access OK (bucket: ${R2_BUCKET_NAME})"

if [ -n "${GCS_FILE_TO_USE}" ]; then
  echo "[rclone] Testing GCS remote access..."
  rclone --config "${RCLONE_CONF}" lsd "gcs:${GCS_BUCKET_NAME}" >/dev/null
  echo "  ✓ GCS access OK (bucket: ${GCS_BUCKET_NAME})"
else
  echo "[rclone] GCS credentials not provided; configured R2 only."
fi

echo "[rclone] Done."
