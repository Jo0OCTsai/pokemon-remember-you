#!/usr/bin/env bash
# mock Spoke（PLAN E5）：模拟 choose-you 经 CLI 供稿 journal + 提案 inbox，
# 替代真实联调完成机械验收（真实联调标注「待 choose-you」）。
# 用法：DEX_ROOT=<fixture 仓库> scripts/mock_spoke.sh <dex 二进制> [--client choose-you]
# 前置：fixture 仓库 config 已注册 [clients."choose-you"]（scopes/propose=true/allowed_sources），
#       本机凭据文件含其 token（或 DEX_TOKEN 提供相同值）。
set -euo pipefail

DEX="${1:?用法: mock_spoke.sh <dex 二进制> [--client choose-you]}"
CLIENT="${2:---client choose-you}"
ROOT="${DEX_ROOT:?需要 DEX_ROOT 指向 fixture 仓库}"

echo "==> journal 供稿 ×2（同日重复应追加至小节尾部）"
"$DEX" journal --source choose-you "捕捉 3 / 逃走 2（原因码：闲聊×2）" $CLIENT
"$DEX" journal --source choose-you "捕捉 1 / 逃走 0（傍晚校正）" $CLIENT

echo "==> propose 无证据（应拒绝，退出码 4）"
set +e
"$DEX" propose --source choose-you --kind pattern --evidence "" "无证据提案" --json $CLIENT
rc=$?
set -e
[[ $rc -eq 4 ]] || { echo "FAIL: 无证据提案应退出码 4，实际 $rc"; exit 1; }

echo "==> propose 正常提案 ×1"
"$DEX" propose --source choose-you --kind pattern --confidence 85 \
  --evidence "chat_feedback #1234 #1301 #1355" \
  "群聊「摸鱼俱乐部」的消息全为闲聊，对该用户无待办含义" --json $CLIENT

echo "==> propose 幂等重放（应返回既有文件路径，退出码 0）"
"$DEX" propose --source choose-you --kind pattern --confidence 70 \
  --evidence "chat_feedback #9999" \
  "群聊「摸鱼俱乐部」的消息全为闲聊，对该用户无待办含义" --json $CLIENT

echo "==> mock Spoke 全部机械项通过"
