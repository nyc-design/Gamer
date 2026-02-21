#!/usr/bin/env bash
# Check top Sunshine instance is responding
response=$(curl -k -s -o /dev/null -w "%{http_code}" https://localhost:47990/api/configLocale 2>/dev/null)
if [ "$response" = "200" ]; then
    exit 0
else
    exit 1
fi
