#!/bin/bash
set -e

# Simple test runner script

echo "🧪 Running tests in Docker container..."

# Build and run tests
docker-compose -f docker-compose.test.yml run --rm provisioner-api-test

echo "✅ Tests completed!"