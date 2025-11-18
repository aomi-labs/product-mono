#!/bin/bash

# Setup git hooks for this repository
# Run this script after cloning the repo to enable pre-commit checks

echo "Setting up git hooks..."
git config core.hooksPath hooks

echo "✓ Git hooks configured successfully!"
echo ""
echo "The following checks will run before each commit:"
echo "  - cargo fmt (code formatting)"
echo "  - cargo clippy (linting)"
echo ""
echo "To bypass hooks temporarily, use: git commit --no-verify"
