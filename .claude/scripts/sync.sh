#!/usr/bin/env bash
set -euo pipefail

# This script reads a summary from a specific file and copies it to the teamspace directory
# Usage: sync.sh [SOURCE_PROJECT_DIR]

TEAMSPACE_DIR="/Users/ceciliazhang/Code/aomi-teamspace/updates/cecilia"
REPO_ROOT="/Users/ceciliazhang/Code/aomi-teamspace"

# Accept source project directory as first argument, default to current script's project
if [ $# -gt 0 ]; then
  SOURCE_PROJECT_DIR="$1"
else
  # Default to the directory where this script is located (go up 2 levels from .claude/scripts/)
  SOURCE_PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fi

SOURCE_DIR="${SOURCE_PROJECT_DIR}/.claude/history"
DATE="$(date +%F)"                             # e.g., 2025-09-22
SOURCE_FILE="${SOURCE_DIR}/${DATE}-summary.md"
OUTFILE="${TEAMSPACE_DIR}/${DATE}.md"

mkdir -p "$TEAMSPACE_DIR"

# Check if source file exists
if [ ! -f "$SOURCE_FILE" ]; then
  echo "Error: Source file does not exist: $SOURCE_FILE" >&2
  exit 1
fi

# Get today's git activity from the source repository
cd "$SOURCE_PROJECT_DIR"

# Get today's date in git format
TODAY_DATE=$(date +%Y-%m-%d)

# Collect git activity information
GIT_ACTIVITY=$(cat <<EOF

## Repository Activities & Active Branches

### Today's Commits ($TODAY_DATE)
$(git log --oneline --since="$TODAY_DATE 00:00:00" --until="$TODAY_DATE 23:59:59" --all --pretty=format:"- %h %s (%an, %ar)" 2>/dev/null || echo "No commits found for today")

### Branches Worked On
$(git for-each-ref --format='- %(refname:short) (last commit: %(committerdate:relative))' refs/heads/ 2>/dev/null | head -10)

### Current Branch Status
- Current branch: $(git branch --show-current 2>/dev/null || echo "unknown")
- Working directory status: $(git status --porcelain 2>/dev/null | wc -l | xargs echo) files with changes

EOF
)

# Copy the original summary and append git activity
cp "$SOURCE_FILE" "$OUTFILE"
echo "$GIT_ACTIVITY" >> "$OUTFILE"

cd "$REPO_ROOT"

# Pull latest changes from main branch
git pull origin main

# Stage the updated file
git add "$OUTFILE"

# Commit if there are staged changes
if ! git diff --cached --quiet; then
  git commit -m "update: daily summary for ${DATE} by cecilia"
  git push origin main
else
  echo "No changes to commit. Skipping push."
fi

echo "Synced to: $OUTFILE"

