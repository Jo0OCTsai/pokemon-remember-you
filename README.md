# pokemon-remember-you（就记得是你）

个人记忆中枢（Hub）：一棵 git 管理的 markdown 文件树 + `dex` CLI——任何 agent / 应用按
scope 接入，读取与提案记忆，换工具不失忆。

- 需求 / 设计 / 安全：[docs/](./docs)（REQUIREMENTS · DESIGN · SECURITY 为权威源）
- 实施计划：[docs/PLAN.md](./docs/PLAN.md)（v0 + v1）
- 代码：`crates/`（dex-core 领域层 → dex-store 基础设施 → dex-cli 命令面）
- 配套技能单一源：`skills/`（dex-bootstrap / dex-propose / dex-review + connectors）
- 工程化：`scripts/check.sh`（fmt + clippy + test）；验收报告 `docs/reports/`

快速开始（开发者）：

```bash
cargo build            # 构建 dex
DEX_ROOT=/tmp/dexroom cargo run -- init          # 建仓骨架
cargo run -- search "偏好" --json                # 检索
scripts/check.sh       # 机械质量门
```
