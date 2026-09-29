#!/usr/bin/env bash
# PostToolUse (Edit|Write)：按 .devcontainer/stack.json 的栈声明路由 lint 自动修复。
# 环境无关基建层（apply-dev-env templates/carrier/ 分发；devcontainer / wsl / native 三载体同源）：
#   - 读点 = .devcontainer/stack.json services[]（载体字段 carrier 对本脚本透明，零读点差异）
#   - native 载体经 git-for-windows bash 执行；jq 非 git-bash 内建，依赖 winget 基座 jqlang.jq
#     （宿主引导 host-setup.ps1 winget-base 模块安装，见 dev-env-settings AD §8.5）
#   - CLAUDE_PROJECT_DIR 由 agent 注入；手动 / pre-commit 场景回退 git 顶层
#   - 与根仓 .claude/hooks/post-edit-lint.sh 同逻辑（双副本同源，改动须双向同步）
# 为什么需要路由：
#   hook 的 cwd = 仓库根，但工具链在服务目录内（子目录或仓库根）——
#   必须进服务目录再用其工具。路由来源：services[] 的 dir 前缀匹配（栈单源声明，换栈只改 json）。
#   dir="."（服务在仓库根）归一化为空前缀 → 匹配整个仓库（根级服务拥有全仓）。
#   无 stack.json / 文件不属于任何服务 → 警告 + exit 0（不硬编码 backend/frontend——
#   任意目录布局下扩展名猜测只会 cd 失败静默空转，见 tests/test-hooks.sh 回归用例）。
# stdin：agent 注入的 PostToolUse JSON（取 tool_input.file_path，绝对路径）。
# 自测：LINT_HOOK_DRY_RUN=1 只打印将执行的命令（宿主机无 uv/pnpm 时验证路由）。
set +e

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
STACK_JSON="$PROJECT_DIR/.devcontainer/stack.json"

f=$(jq -r '.tool_input.file_path // empty')
[ -z "$f" ] && exit 0

run_in() {  # run_in <服务目录> <命令（含 {file} 占位）>
  local dir="$1" cmd="$2"
  cmd="${cmd//\{file\}/$f}"
  if [ "${LINT_HOOK_DRY_RUN:-0}" = "1" ]; then
    echo "[dry-run] cd $dir && $cmd"
  else
    ( cd "$dir" && eval "$cmd" )
  fi
}

# stack.json 路由：文件命中 <dir>/ 前缀的 service → 用其 lint_fix
# dir 归一化：剥 "./" 前缀与尾斜杠；dir="."（或 "./"）→ 归一为根级，前缀退化为
# "$PROJECT_DIR/" 即全仓（根级服务拥有全仓）。bash case 不做路径归一化，"." /
# "backend/" / "./backend" 若不归一化永不匹配（回归教训：dir="." 曾使 case 变
# "$ROOT/.//*"；且归一后不能直接拼 "$PROJECT_DIR/$d/"——d 空时会变 "$ROOT//" 双斜杠同样永不匹配）。
if [ -f "$STACK_JSON" ]; then
  while IFS=$'\t' read -r dir fix; do
    [ -z "$dir" ] && continue
    d="${dir#./}"; d="${d%/}"
    if [ "$d" = "." ] || [ -z "$d" ]; then svc_dir="$PROJECT_DIR"
    else svc_dir="$PROJECT_DIR/$d"; fi
    case "$f" in
      "$svc_dir/"*)
        run_in "$svc_dir" "$fix"
        exit 0
        ;;
    esac
  done < <(jq -r '.services[] | "\(.dir)\t\(.lint_fix)"' "$STACK_JSON" 2>/dev/null)
fi

# 无 stack.json / 无命中 → 警告后放行（与同目录 lint-gate 的空声明不拦截口径一致）
echo "post-edit-lint: $f 未命中任何服务（$STACK_JSON 缺失或 dir 前缀不匹配），跳过 lint_fix" >&2
exit 0
