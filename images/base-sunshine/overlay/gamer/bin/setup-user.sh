#!/usr/bin/env bash
set -e

echo "[setup-user] Configuring user groups and device access..."

# Add to standard groups
for group in video audio input pulse render; do
    if getent group "${group}" > /dev/null 2>&1; then
        usermod -aG "${group}" "${GAMER_USER}" 2>/dev/null || true
    fi
done

# Dynamically add user to groups for all device nodes
# This ensures controllers, joysticks, GPU DRI nodes all work
for device in /dev/input/event* /dev/input/js* /dev/uinput /dev/dri/*; do
    if [[ ! -c "${device}" ]]; then continue; fi
    device_group=$(stat -c "%G" "${device}")
    device_gid=$(stat -c "%g" "${device}")
    if [[ "${device_gid}" = "0" ]]; then continue; fi
    if [[ "${device_group}" = "UNKNOWN" ]]; then
        device_group="dev-gid-${device_gid}"
        groupadd -g $device_gid "${device_group}" 2>/dev/null || true
    fi
    if ! id -nG "${GAMER_USER}" | grep -qw "${device_group}"; then
        usermod -aG ${device_group} ${GAMER_USER} 2>/dev/null || true
    fi
done
