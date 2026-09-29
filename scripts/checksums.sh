#!/usr/bin/env bash
# 发行物完整性（FR-11.7）：生成 checksums.txt（SHA-256，覆盖 dist 内全部二进制与内嵌技能物）
# 用法：scripts/checksums.sh [release 目录]（缺省 dist/）
# 单平台：cargo build --release && mkdir -p dist && cp target/release/dex dist/
# 多平台：release.yml 汇齐各目标产物（dex-v<version>-<target>）后调用
set -euo pipefail
cd "$(dirname "$0")/.."
DIST="${1:-dist}"
[[ -d "$DIST" ]] || { echo "缺 $DIST 目录（先建目录并放入发行物）"; exit 1; }
ls "$DIST"/dex* >/dev/null 2>&1 || { echo "缺 $DIST/dex* 二进制（单平台：cargo build --release && cp target/release/dex $DIST/；多平台：由 release.yml 汇齐）"; exit 1; }

{
  (cd "$DIST" && find . -type f ! -name 'checksums.txt' | sort | xargs shasum -a 256)
  (cd skills && find . -type f | sort | xargs shasum -a 256)
} > "$DIST/checksums.txt"

echo "OK: $DIST/checksums.txt"
cat "$DIST/checksums.txt"
