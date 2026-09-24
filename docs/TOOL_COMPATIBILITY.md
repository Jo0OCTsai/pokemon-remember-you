# 工具入口兼容矩阵（render 产物位置策略）

> 项目：pokemon-remember-you（就记得是你）
> 文档版本：v1.0 · 2026-09-24 · 状态：待评审
> 上游文档：[REQUIREMENTS.md](./REQUIREMENTS.md) FR-6.4/FR-6.12 · [DESIGN.md](./DESIGN.md) §2.5/§6.2 · [SCENARIO_WALKTHROUGH.md](./SCENARIO_WALKTHROUGH.md) §6-W3
> 定位：回答「`dex render` 的入口文件该落哪、各工具会不会读」——场景推演 W3 的交付物；**修正了 v1.7/v1.8 的「默认工作区子目录」决策**（查证结论见 §1）；随工具生态演进持续维护，❓ 标注项接入前实测。

---

## 1. 关键语义：入口文件的两种加载模式（本矩阵的核心事实）

工具对 AGENTS.md 系入口文件的加载只有两种语义：

| 模式 | 语义 | 典型 |
|---|---|---|
| **恒载（startup）** | 会话启动即全文加载 | repo 根 AGENTS.md / CLAUDE.md / GEMINI.md |
| **子树按需（subtree-scoped）** | 仅当会话从该子树启动、或工具操作该子树内文件时才加载 | 子目录 AGENTS.md（Codex：仅 `--cd`/启动目录生效，动态加载仍是未完成的 feature request）；Claude Code 子目录 CLAUDE.md（按需加载）；Cursor glob 规则（未命中编辑路径即忽略） |

**推论（v1.9 决策依据）**：子树按需是**作用域机制**，不是注入位——把全局个人记忆渲染到工作区子目录（如 `.zcode/AGENTS.md`），多数工具**根本不会加载**。子目录产物只对一种场景正确：记忆本身就该 scope 到该子树（monorepo 包级记忆）。因此 FR-6.4 默认产物位置改为 **repo 根 AGENTS.md**（个人仓库默认位，退出码 10 冲突保护仍在），团队仓库按 §3 分流。

## 2. 兼容矩阵

| 工具 | 恒载入口 | 嵌套/子目录行为 | import 语法 | 用户级全局 | dex 推荐策略 |
|---|---|---|---|---|---|
| **Claude Code** | repo 根 CLAUDE.md；无 CLAUDE.md 时回退 AGENTS.md（v2.1.277+，可开关） | 子目录 CLAUDE.md 按需（操作该子树文件时） | ✅ `@path`（CLAUDE.md 内，支持仓库外绝对路径，首次弹确认） | `~/.claude/CLAUDE.md` | **@import**：用户级或 repo `.claude/CLAUDE.md` 一行 `@~/dex/person/…`（repo 侧文件可 gitignore）；`render format=import` |
| **Codex CLI** | repo 根 AGENTS.md | 子目录 AGENTS.md 仅会话从该子树启动时生效；按文件触达动态加载为 feature request（未落地） | ❌ 无 | `~/.codex/AGENTS.md` | 个人仓库：render 落根 AGENTS.md；**团队仓库受限**（无 import）——全局层手动维护 `~/.codex/AGENTS.md`，或与团队协商根文件（见 §3 注） |
| **OpenCode** | repo 根 AGENTS.md | 嵌套 AGENTS.md（子树语义） | ❓ 待验证 | `~/.config/opencode/AGENTS.md` | 同 Codex |
| **Cursor** | AGENTS.md（根，另有全局） | AGENTS.md 子目录 + `.cursor/rules/*.mdc` 四种激活：Always / Auto-Attached（glob）/ Agent-Requested / Manual | ❌（以 rules 机制承担） | 全局 rules | **rules 适配**：`.cursor/rules/dex.mdc`（Always 激活），`render format=rules`（FR-6.12）；注意 Always 级常驻占 context，宜配小预算产物 |
| **Gemini CLI** | GEMINI.md（根），层级加载（全局/根/子目录） | 子目录 GEMINI.md（层级） | ✅ `@file.md`（相对/绝对路径，仅 .md） | `~/.gemini/GEMINI.md` | **@import**：GEMINI.md 一行 `@<dex 文件或根 AGENTS.md>` |
| **Zed** | 根 AGENTS.md | ❓ worktree 级规则待验证 | ❓ | — | 个人仓库：根 AGENTS.md |
| **ZCode** | 工作区 AGENTS.md | ❓ 待验证 | ❓ | 用户级 AGENTS.md | 个人仓库：根 AGENTS.md |

> ❓ = 截至本文档调研未证实，接入前以烟测为准（在目标工具会话中问「你能看到哪些入口文件内容」即可验证）。

## 3. render 策略决策树（FR-6.4 的操作化）

```text
repo 根是否已有团队入口文件（AGENTS.md / CLAUDE.md 等非 dex 产物）？
├─ 否（个人仓库）→ dex render 落根 AGENTS.md（默认；退出码 10 冲突保护恒在）
└─ 是（团队仓库）→ 按消费工具分流，各落各的适配位（render 按 client 分别配置）：
    ├─ Claude Code → format=import：repo .claude/CLAUDE.md 一行 @import（该文件 gitignore）
    │                或用户级 ~/.claude/CLAUDE.md（零 repo 足迹，全局生效）
    ├─ Gemini CLI  → GEMINI.md 一行 @import（同上）
    ├─ Cursor      → format=rules：.cursor/rules/dex.mdc（Always 激活，gitignore 该文件）
    ├─ Codex       → 无 import 语法，最受限：全局 ~/.codex/AGENTS.md 手动维护
    │                （或与团队协商：根 AGENTS.md 由 dex 管理、团队内容另放 rules——属团队治理决策）
    └─ 子树级记忆（monorepo 包级）→ 子目录 AGENTS.md（子树按需语义恰好匹配，唯一正确的子目录用法）
```

**通用注意**：
- 所有适配位产物（根 AGENTS.md / rules / import 宿主文件若在 repo 内）涉及个人记忆的，建议 gitignore 或使用用户级宿主——防个人记忆被误 commit 进团队仓库（FR-6.4 原始动机不变）；
- `@import` 类宿主（CLAUDE.md/GEMINI.md）启动即全量加载、不省 context——条目多后注入预算改由 `dex render` 产物承担（与 REQUIREMENTS US-01 步骤 4 既有口径一致）。

## 4. 调研来源（2026-09-24）

- AGENTS.md 约定：<https://agents.md>
- Codex AGENTS.md 嵌套加载语义与动态加载 feature request：<https://learn.chatgpt.com> · <https://github.com>（openai/codex issue）
- Claude Code 记忆层级（子目录按需 / AGENTS.md 回退 v2.1.277+ / @import）：<https://code.claude.com/docs/en/memory> · <https://github.com/anthropics/claude-code/issues/6235>
- Cursor rules 四种激活与 AGENTS.md 支持：<https://cursor.com/docs>（Rules）
- Gemini CLI GEMINI.md 层级与 `@file.md` import：<https://geminicli.com/docs>
