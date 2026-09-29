# pokemon-remember-you（就记得是你）

个人记忆中枢（Hub）本体仓：git 管理的 markdown 文件树 + `dex` CLI。定位与快速开始见 [README.md](./README.md)；权威语义以 docs/ 三文档为准（下节路由）。

开发载体：macos（mac 宿主直跑，无容器件；载体声明单源 `.devcontainer/stack.json` 的 `carrier` 字段）——agent 会话在本机直接运行；依赖工具（rustfmt / clippy / pre-commit 等）宿主已备（缺则经 host-bootstrap `host-setup.sh apply --only dev-tools` 供给），操作前宿主机本仓先 `git pull` 至最新版（ADR-0012）。

## 知识路由

- 权威源三文档：`docs/REQUIREMENTS.md` / `docs/DESIGN.md` / `docs/SECURITY.md`——行为语义与代码或本文冲突时以它们为准；特性变更随文档版本号升版回写，不静默漂移
- 实施计划 `docs/PLAN.md`；验收报告 `docs/reports/`；场景与工具矩阵备查：`docs/WORK_LIFE_SCENARIOS.md` / `docs/SCENARIO_WALKTHROUGH.md` / `docs/TOOL_COMPATIBILITY.md`
- 债务与待办单源：`docs/debt.md`（活清单，不散落别处）
- 配置示例：`dex.toml.example`（仓库层 / 本机层键级深合并语义见 DESIGN §2.5）
- 「我」的个人视角（owner 例外、个人踩坑、偏好）：不入本仓——`dex propose` 入 Hub（`~/dex`，经 dex-propose 技能）

## 模块不变量

- `crates/` 三层单向依赖：`dex-core`（领域层：纯逻辑、无 I/O）← `dex-store`（基础设施：文件树 / git / 检索 / 审计）← `dex-cli`（命令面）；禁止反向依赖（DESIGN §9）
- `E_*` / `W_*` 错误与警告码单源在 `crates/dex-core/src/errors.rs`，CLI 只透传不定义
- `skills/` 是受管技能单一源（dex-bootstrap / dex-propose / dex-review / repo-knowledge + connectors）：改技能改仓内源，经 `dex skills install` 发行到 `~/.local/share/dex/skills`（各 agent 入口软链该处），不手改发行物副本
- 机械质量门：`scripts/check.sh` = cargo fmt --check + clippy -D warnings + cargo test；CI（`.github/workflows/ci.yml`，ubuntu + macos 矩阵）同源执行它；另有 pre-commit + `.claude/hooks`（Stop / PostToolUse，经 `.devcontainer/stack.json` 路由到同款 fmt + clippy）作机械兜底——交付前本地跑一次 check.sh

## 分支与 Worktree 约定

- 分支模型：短生命周期 feature 分支（GitHub Flow，type 前缀命名如 `build/xxx` / `feat/xxx`）；squash 合并保线性历史——平台侧已配 squash-only（主题 = PR 标题）+ main 分支保护（require PR + 六 status checks + linear history；enforce_admins=false 保留 owner 直推通道）
- 提交信息：Conventional Commits——本地 commit-msg 钩子 + CI PR 标题/逐提交双校验（见命令速查「提交规范」）
- 本仓无 submodule（无 .gitmodules）：主/子仓指针铁律暂不适用；未来引入时按全局铁律先子仓 push 再更新主仓指针
- 功能开发隔离用 worktree：`create-repo-worktree` 技能（放置分流 / 运行时隔离 / 清理规范见该技能，此处不复制）；纯 CLI 无 dev server，无并行端口错开需求

## 禁止事项

- 不引入 `.devcontainer/` 容器件（devcontainer.json / Dockerfile / features 等）——本仓载体已声明 macos，混入两套开发环境定义会被 apply 互斥保护拒绝（换载体走 `apply --carrier <新载体> --migrate-carrier`）

## 命令速查

- `cargo build` / `cargo test --workspace`
- `scripts/check.sh [--no-test]` —— 提交前一键质量门
- `python3 .devcontainer/scripts/quality_ratchet_check.py` —— 质量棘轮（复杂度基线只准收缩；`--bootstrap` 首次建账仅限一次、已有基线自动拒绝；`--selftest` 自测）
- 秘密扫描：`gitleaks detect --source . --redact`（全历史；豁免 = `.gitleaks.toml` 只按具体假凭证 regex）；提交时 pre-commit 跑 `gitleaks protect --staged`，CI gitleaks job 兜底
- rust 供应链：`cargo deny --log-level error check`（漏洞 / license 白名单 / bans / 来源；配置 = 根目录 `deny.toml`，license 建账 = `cargo deny list`；`--log-level` 须在子命令前），CI deny job 同口径
- 覆盖率趋势：Codecov（`codecov.yml` status informational——硬门禁在 CI 阈值 + 棘轮地板；单一 rust 服务 flag = `rust`，private 仓配 `CODECOV_TOKEN`）
- 提交规范：Conventional Commits `<type>(<scope>)?!?: <subject>`——本地 commit-msg 钩子 + CI PR 标题/逐提交双校验（同一脚本 `.github/scripts/commit-lint.sh`）
- 安装与其余命令面见 README「快速开始」与 DESIGN §8.1
