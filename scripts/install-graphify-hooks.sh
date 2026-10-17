#!/bin/sh
root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 1
cd "$root" || exit 1
git config core.hooksPath .githooks
echo "core.hooksPath=.githooks"
