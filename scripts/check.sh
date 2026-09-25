#!/usr/bin/env bash
# 机械质量门（PLAN E4）：cargo fmt --check + clippy -D warnings + test 全绿
# 用法：scripts/check.sh [--no-test]
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

if [[ "${1:-}" != "--no-test" ]]; then
  echo "==> cargo test --workspace"
  cargo test --workspace
fi

echo "==> check.sh 全部通过"
