#!/usr/bin/env bash
# Stop hook：交付前全量 lint/format 检查（按 .devcontainer/stack.json 路由）。
# 环境无关基建层（apply-dev-env templates/carrier/ 分发；devcontainer / wsl / native 三载体同源）：
#   - 读点 = .devcontainer/stack.json services[]（载体字段 carrier 对本脚本透明，零读点差异）
#   - native 载体经 git-for-windows bash 执行；jq 非 git-bash 内建，依赖 winget 基座 jqlang.jq
#     （宿主引导 host-setup.ps1 winget-base 模块安装，见 dev-env-settings AD §8.5）
#   - CLAUDE_PROJECT_DIR 由 agent 注入；手动 / pre-commit 场景回退 git 顶层
# 任一服务 lint_check 非零 → 汇总打印并 exit 1（阻断交付，agent 会看到失败原因）。
# 无 stack.json 或未解析出任何服务 → exit 0（空声明不拦截）。
# 自测：LINT_GATE_DRY_RUN=1 只打印将执行的命令。
set +e

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
STACK_JSON="$PROJECT_DIR/.devcontainer/stack.json"
FAILED=0
RAN=0

run_check() {  # run_check <服务名> <目录> <命令>
  local name="$1" dir="$2" cmd="$3"
  if [ "${LINT_GATE_DRY_RUN:-0}" = "1" ]; then
    echo "[dry-run] [$name] cd $dir && $cmd"
    return 0
  fi
  echo "==> lint gate: $name"
  if ( cd "$dir" && eval "$cmd" ); then
    echo "    ✓ $name 通过"
  else
    echo "    ✗ $name 未通过（见上方输出；机械质量不过则阻断交付）" >&2
    FAILED=1
  fi
}

if [ -f "$STACK_JSON" ]; then
  while IFS=$'\t' read -r name dir check; do
    [ -z "$name" ] && continue
    RAN=$((RAN+1))
    run_check "$name" "$PROJECT_DIR/$dir" "$check"
  done < <(jq -r '.services[] | "\(.name)\t\(.dir)\t\(.lint_check)"' "$STACK_JSON" 2>/dev/null)
fi

# stack.json 缺失或未解析出服务（如 services[] 为空）→ 无可路由检查，放行
if [ "$RAN" = 0 ]; then
  echo "lint gate: 无可路由服务（$STACK_JSON 缺失或 services[] 为空），跳过"
fi

exit $FAILED
