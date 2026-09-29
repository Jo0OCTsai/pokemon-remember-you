#!/usr/bin/env bash
# commit-lint.sh —— Conventional Commits 提交主题校验（零依赖，pre-commit 与 CI 共用）
# 用法：commit-lint.sh "<subject>"    （直接传主题字符串，CI 用）
#       commit-lint.sh <msg-file>     （传提交信息文件，取首行，pre-commit commit-msg 钩子用）
# 退出码 0=合法，1=非法（给出格式说明）。
# 格式：<type>(<scope>)?: <subject>
#   type: feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert|release
#   scope: 可选，小写字母/数字/._-
#   '!': breaking change 标记（可选，须在冒号前）
#   subject: 非空（中英文皆可；建议 ≤72 字符，超长仅提示不强拦）
# 与 @commitlint/config-conventional 的差异：不校验 body/footer 与 subject-case、不限长度
# （中文主题不适用 sentence-case；长度靠人工自觉）——零依赖换来的取舍。
# 源流：dev-env-settings 仓 .github/scripts/commit-lint.sh 泛化为 apply-dev-env 规范模块
# 分发版（逻辑逐字一致；模块文件分发后归项目所有，演进不自动跟）。
set -uo pipefail

if [ $# -ne 1 ]; then
  echo "用法: $0 <commit-subject | msg-file>" >&2
  exit 1
fi

if [ -f "$1" ]; then
  # 提交信息文件：校验首行（主题行）
  SUBJECT="$(head -n 1 "$1")"
else
  SUBJECT="$1"
fi

# 空行 / merge 提交跳过（merge 由平台生成，不作约定校验）
if [ -z "$SUBJECT" ] || [[ "$SUBJECT" == "Merge "* ]]; then
  exit 0
fi

PATTERN='^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert|release)(\([a-z0-9][a-z0-9._/-]*\))?!?: .+'

if ! printf '%s' "$SUBJECT" | LC_ALL=C grep -Eq "$PATTERN"; then
  echo "✗ 非法提交主题: $SUBJECT" >&2
  echo "  约定: <type>(<scope>)?!?: <subject>" >&2
  echo "  type ∈ feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert|release（scope 可选）" >&2
  echo "  示例: feat(backend): todo CRUD 接口" >&2
  exit 1
fi

if [ "${#SUBJECT}" -gt 100 ]; then
  echo "⚠ 主题长度 ${#SUBJECT} 超过 100 字符，建议精简（不强拦）" >&2
fi
exit 0
