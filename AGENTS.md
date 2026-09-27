# pokemon-remember-you（就记得是你）

个人记忆中枢（Hub）本体仓：git 管理的 markdown 文件树 + `dex` CLI。定位与快速开始见 [README.md](./README.md)；权威语义以 docs/ 三文档为准（下节路由）。

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
- 机械质量门：`scripts/check.sh` = cargo fmt --check + clippy -D warnings + cargo test；CI（`.github/workflows/ci.yml`，ubuntu + macos 矩阵）同源执行它——本仓无 pre-commit hook，交付前本地跑一次

## 命令速查

- `cargo build` / `cargo test --workspace`
- `scripts/check.sh [--no-test]` —— 提交前一键质量门
- 安装与其余命令面见 README「快速开始」与 DESIGN §8.1
