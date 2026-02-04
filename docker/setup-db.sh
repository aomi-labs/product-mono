#!/bin/bash
# Aomi Database Setup Script (LOCAL DEVELOPMENT ONLY)
#
# ⚠️  PRODUCTION uses DigitalOcean Managed PostgreSQL
# ⚠️  This script is for LOCAL DEVELOPMENT only
#
# For production database, see: https://github.com/aomi-labs/db-master
#
# Usage: POSTGRES_PASSWORD=... ./setup-db.sh
#    or: ./setup-db.sh [POSTGRES_PASSWORD]
#
# Example:
#   POSTGRES_PASSWORD=my_secure_password ./setup-db.sh
#   ./setup-db.sh my_secure_password

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Optional: Set password from argument
if [ -n "${1:-}" ]; then
    export POSTGRES_PASSWORD="$1"
fi
if [ -z "${POSTGRES_PASSWORD:-}" ]; then
    echo "❌ ERROR: POSTGRES_PASSWORD is required."
    echo "Set it as an environment variable or pass it as an argument."
    exit 1
fi

echo "🗄️  Aomi Database Setup"
echo "========================"
echo ""

# Check if database is already running
if docker ps --format '{{.Names}}' | grep -q '^aomi-db$'; then
    echo "✅ Database (aomi-db) is already running."
    docker exec aomi-db pg_isready -U aomi -d chatbot
    echo ""
    echo "To restart the database (will preserve data):"
    echo "  docker compose -f docker-compose-db.yml restart"
    echo ""
    echo "To completely reset (WARNING: destroys all data):"
    echo "  docker compose -f docker-compose-db.yml down -v"
    echo "  docker compose -f docker-compose-db.yml up -d"
    exit 0
fi

# Check if there's a stopped database container
if docker ps -a --format '{{.Names}}' | grep -q '^aomi-db$'; then
    echo "⚠️  Database container exists but is not running. Starting..."
    docker compose -f docker-compose-db.yml up -d
else
    echo "📦 Creating new database..."
    docker compose -f docker-compose-db.yml up -d
fi

# Wait for database to be ready
echo "⏳ Waiting for database to be ready..."
for i in {1..30}; do
    if docker exec aomi-db pg_isready -U aomi -d chatbot >/dev/null 2>&1; then
        echo ""
        echo "✅ Database is ready!"
        echo ""
        echo "Connection details:"
        echo "  Host:     localhost (or aomi-db from Docker network)"
        echo "  Port:     5432"
        echo "  Database: chatbot"
        echo "  User:     aomi"
        echo ""
        echo "The backend can now be deployed with:"
        echo "  docker compose -f docker-compose-backend.yml up -d"
        exit 0
    fi
    sleep 1
    printf "."
done

echo ""
echo "❌ Database failed to start. Check logs with:"
echo "  docker logs aomi-db"
exit 1
