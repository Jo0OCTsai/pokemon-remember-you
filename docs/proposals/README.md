# 方案与调研（Proposals）

方案设计与调研文档的归档目录，回答「为什么这么设计」——方案提案、场景调研、业务流程推演与工具兼容结论。
与顶层权威源（[REQUIREMENTS](../REQUIREMENTS.md) / [DESIGN](../DESIGN.md) / [SECURITY](../SECURITY.md)，行为语义「是什么 / 怎么做」）与实施计划（[PLAN](../PLAN.md)）区分。

| 文档 | 主题 | 状态 |
|---|---|---|
| [PERSONAL_MEMORY_HUB_PROPOSAL.md](PERSONAL_MEMORY_HUB_PROPOSAL.md) | Hub + Spokes 方案提案（跨应用普适记忆 + 个人知识库） | v0+v1 已落地（v2 外源索引方案已定未实施） |
| [WORK_LIFE_SCENARIOS.md](WORK_LIFE_SCENARIOS.md) | 使用场景调研：工作/生活场合的应用接入地图（指南层） | 持续维护（v1.2） |
| [SCENARIO_WALKTHROUGH.md](SCENARIO_WALKTHROUGH.md) | 14 个用户场景端到端业务流程推演（需求/设计的验证性衍生） | 随上游同步（W1–W4 已全部关闭，v1.10） |
| [TOOL_COMPATIBILITY.md](TOOL_COMPATIBILITY.md) | 工具入口兼容矩阵（render 产物位置策略） | 持续维护（❓ 项接入前实测，v1.3） |

约定：新提案放本目录，命名 `TOPIC_PROPOSAL.md`（调研类 `_ANALYSIS.md`、场景类 `_SCENARIOS.md`）；实施状态写在文首引言并同步本表，随落地更新；本仓文件名全局唯一，正文互引沿用文档名，带路径引用以 `docs/proposals/` 为前缀（2026-09-30 自 docs/ 顶层迁入，历史文件不改名、避免散引断裂）。
