#!/bin/bash
# Kill all orphaned anvil processes

count=$(pgrep -f "anvil" | wc -l | tr -d ' ')

if [ "$count" -eq 0 ]; then
    echo "No anvil processes found"
    exit 0
fi

echo "Found $count anvil process(es), killing..."
pkill -9 -f "anvil"
sleep 1

remaining=$(pgrep -f "anvil" | wc -l | tr -d ' ')
if [ "$remaining" -eq 0 ]; then
    echo "All anvil processes killed"
else
    echo "Warning: $remaining process(es) still running"
fi
