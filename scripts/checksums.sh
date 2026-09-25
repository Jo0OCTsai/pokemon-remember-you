#!/usr/bin/env bash
# 发行物完整性（FR-11.7）：生成 checksums.txt（SHA-256，覆盖二进制与内嵌技能物）
# 用法：scripts/checksums.sh [release 目录]（缺省 dist/；需先 cargo build --release 并 cp 二进制）
set -euo pipefail
cd "$(dirname "$0")/.."
DIST="${1:-dist}"
[[ -f "$DIST/dex" ]] || { echo "缺 $DIST/dex（先 cargo build --release && mkdir -p $DIST && cp target/release/dex $DIST/）"; exit 1; }

{
  (cd "$DIST" && shasum -a 256 dex)
  (cd skills && find . -type f | sort | xargs shasum -a 256)
} > "$DIST/checksums.txt"

echo "OK: $DIST/checksums.txt"
cat "$DIST/checksums.txt"
