#!/bin/sh
# Optimized preview by default. Use `dev` only when debugging Rust code.
set -eu
cd "$(dirname "$0")/.."
profile="${1:-release}"
if [ "$#" -gt 0 ]; then shift; fi
./scripts/build-macos.sh "$profile" "$@"
case "$profile" in
  release) bundle='target/release/Canopy.app' ;;
  profiling) bundle='target/profiling/Canopy Profile.app' ;;
  dev) bundle='target/debug/Canopy Dev.app' ;;
esac
open "$bundle"
