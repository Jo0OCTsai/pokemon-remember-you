# pokemon-remember-you（就记得是你）

[![coverage](https://codecov.io/gh/Jo0OCTsai/pokemon-remember-you/branch/main/graph/badge.svg)](https://codecov.io/gh/Jo0OCTsai/pokemon-remember-you)

个人记忆中枢（Hub）：一棵 git 管理的 markdown 文件树 + `dex` CLI——任何 agent / 应用按
scope 接入，读取与提案记忆，换工具不失忆。

- 需求 / 设计 / 安全：[docs/](./docs)（REQUIREMENTS · DESIGN · SECURITY 为权威源）
- 实施计划：[docs/PLAN.md](./docs/PLAN.md)（v0 + v1）
- 方案与调研：[docs/proposals/](./docs/proposals)（README 索引——方案提案 / 场景调研 / 流程推演 / 工具矩阵）
- 代码：`crates/`（dex-core 领域层 → dex-store 基础设施 → dex-cli 命令面）
- 配套技能单一源：`skills/`（dex-bootstrap / dex-propose / dex-review / repo-knowledge + connectors）
- 工程化：`scripts/check.sh`（fmt + clippy + test）；CI 另含 gitleaks 秘密扫描 / cargo-deny 供应链 / 覆盖率上传 / commit-lint 门禁；测试报告经 CI 导出 junit artifact，不入仓

快速开始（开发者，mac 宿主直跑）：

```bash
cargo build            # 构建 dex
DEX_ROOT=/tmp/dexroom cargo run -- init          # 建仓骨架
cargo run -- search "偏好" --json                # 检索
scripts/check.sh       # 机械质量门
```
