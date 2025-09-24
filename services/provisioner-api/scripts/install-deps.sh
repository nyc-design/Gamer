#!/bin/bash
# Environment-specific dependency installation script

set -e

ENVIRONMENT=${1:-"dev"}

case $ENVIRONMENT in
  "production" | "prod")
    echo "Installing production dependencies only..."
    poetry install --only main
    ;;
  "development" | "dev")
    echo "Installing development dependencies (prod + dev tools)..."
    poetry install --with dev --without test,integration,performance,reporting
    ;;
  "testing" | "test")
    echo "Installing testing dependencies (prod + test)..."
    poetry install --with test --without dev,integration,performance,reporting
    ;;
  "full" | "all")
    echo "Installing all dependencies..."
    poetry install --with dev,test,integration,performance,reporting
    ;;
  "ci")
    echo "Installing CI dependencies (prod + test + reporting)..."
    poetry install --with test,reporting --without dev,integration,performance
    ;;
  *)
    echo "Usage: $0 {production|development|testing|full|ci}"
    echo ""
    echo "Environments:"
    echo "  production  - Only production dependencies"
    echo "  development - Production + development tools"
    echo "  testing     - Production + core testing tools"
    echo "  full        - All dependencies (for local development)"
    echo "  ci          - CI/CD pipeline dependencies"
    exit 1
    ;;
esac

echo "✅ Dependencies installed for $ENVIRONMENT environment"