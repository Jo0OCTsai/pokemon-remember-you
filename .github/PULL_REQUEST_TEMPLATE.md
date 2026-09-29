<!-- PR 标题与提交信息须符合 Conventional Commits（采纳 commit-lint 规范模块时本地 commit-msg 钩子与 CI 双校验）：feat(scope): xxx -->

## 变更说明

<!-- 做了什么、为什么；关联权威源文档（docs/REQUIREMENTS.md 等，特性变更随版本号升版回写）与 docs/debt.md 条目 -->

## 自查清单

<!-- 按项目守卫链增删；命令单源 = .devcontainer/stack.json services[] -->
- [ ] lint 通过（`bash .claude/hooks/lint-gate.sh` 或 `scripts/check.sh [--no-test]`）
- [ ] 测试通过（`cargo test --workspace`；新增/变更逻辑有测试覆盖）
- [ ] 改动了目录结构/命令/配置路径：已 grep 全仓引用点并同步受影响文档与技能
- [ ] 债务清单已更新（docs/debt.md：新债务入账 / 完成项勾选移入已完成段）
