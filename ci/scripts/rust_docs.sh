#!/usr/bin/env bash
# Note: cargo doc does not support an auto-fix mode; this script runs the check-only build.
set -ex
export RUSTDOCFLAGS="-D warnings"
cargo doc --document-private-items --no-deps --workspace
