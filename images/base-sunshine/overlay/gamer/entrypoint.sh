#!/usr/bin/env bash
set -e

# Setup all runtime config before starting services
source /gamer/bin/setup-all.sh

# Start supervisord as PID 1
exec /usr/bin/supervisord --nodaemon --user root -c /gamer/conf/supervisor/supervisord.conf
