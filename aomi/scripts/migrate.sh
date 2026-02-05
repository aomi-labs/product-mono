#!/bin/bash
# Run database migrations (idempotent)
# Usage: DATABASE_URL=... ./scripts/migrate.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MIGRATIONS_DIR="$SCRIPT_DIR/migrations"

if [ -z "${DATABASE_URL:-}" ]; then
    echo "❌ ERROR: DATABASE_URL is required"
    exit 1
fi

echo "🗄️  Running migrations..."
echo "   Target: ${DATABASE_URL%%@*}@****"

# Run each migration file in order
for migration in "$MIGRATIONS_DIR"/*.sql; do
    if [ -f "$migration" ]; then
        filename=$(basename "$migration")
        echo "   📄 Applying: $filename"
        
        # psql with ON_ERROR_STOP so we fail fast on errors
        # Using -v ON_ERROR_STOP=1 to exit on first error
        psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$migration" 2>&1 | while read -r line; do
            # Filter out noise, only show important messages
            case "$line" in
                *"already exists"*|*"NOTICE"*) ;;
                *"CREATE"*|*"ALTER"*|*"INSERT"*) echo "      ✓ $line" ;;
                *"ERROR"*) echo "      ❌ $line" ;;
            esac
        done
    fi
done

echo "✅ Migrations complete!"
