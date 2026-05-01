#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
cargo install --path . --locked --force
echo
echo "installed to $(command -v gitty || echo "$HOME/.cargo/bin/gitty")"
