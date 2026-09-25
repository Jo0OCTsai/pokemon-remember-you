# 工具入口兼容矩阵（render 产物位置策略）

> 项目：pokemon-remember-you（就记得是你）
> 文档版本：v1.2 · 2026-09-25 · 状态：待评审（v1.2：debt 清偿同步——import 片段「数据非指令」声明承载说明；此前 v1.1：支持范围收窄——AI coding 工具限定 Claude Code / pi / ZCode，其余移入「未纳入」备查）
> 上游文档：[REQUIREMENTS.md](./REQUIREMENTS.md) FR-6.4/FR-6.12 · [DESIGN.md](./DESIGN.md) §2.5/§6.2 · [SCENARIO_WALKTHROUGH.md](./SCENARIO_WALKTHROUGH.md) §6-W3
> 定位：回答「`dex render` 的入口文件该落哪、各工具会不会读」——场景推演 W3 的交付物；**修正了 v1.7/v1.8 的「默认工作区子目录」决策**（查证结论见 §1）；随工具生态演进持续维护，❓ 标注项接入前实测。
> **支持范围（需求 §7 约束）**：AI coding 工具只支持 **Claude Code、pi、ZCode** 三者；其余工具暂不考虑（调研结论保留于 §2.2 备查，接入需求出现时再评估）。

---

## 1. 关键语义：入口文件的两种加载模式（本矩阵的核心事实）

工具对 AGENTS.md 系入口文件的加载只有两种语义：

| 模式 | 语义 | 典型 |
|---|---|---|
| **恒载（startup）** | 会话启动即全文加载 | repo 根 AGENTS.md / CLAUDE.md / GEMINI.md |
| **子树按需（subtree-scoped）** | 仅当会话从该子树启动、或工具操作该子树内文件时才加载 | 子目录 AGENTS.md（Codex：仅 `--cd`/启动目录生效，动态加载仍是未完成的 feature request）；Claude Code 子目录 CLAUDE.md（按需加载）；Cursor glob 规则（未命中编辑路径即忽略） |

**推论（v1.9 决策依据）**：子树按需是**作用域机制**，不是注入位——把全局个人记忆渲染到工作区子目录（如 `.zcode/AGENTS.md`），多数工具**根本不会加载**。子目录产物只对一种场景正确：记忆本身就该 scope 到该子树（monorepo 包级记忆）。因此 FR-6.4 默认产物位置改为 **repo 根 AGENTS.md**（个人仓库默认位，退出码 10 冲突保护仍在），团队仓库按 §3 分流。

## 2. 兼容矩阵

### 2.1 支持范围（Claude Code / pi / ZCode）

| 工具 | 恒载入口 | 嵌套/子目录行为 | import 语法 | 用户级全局 | 技能目录 | dex 推荐策略 |
|---|---|---|---|---|---|---|
| **Claude Code** | repo 根 CLAUDE.md；无 CLAUDE.md 时回退 AGENTS.md（v2.1.277+，可开关） | 子目录 CLAUDE.md 按需（操作该子树文件时） | ✅ `@path`（CLAUDE.md 内，支持仓库外绝对路径，首次弹确认） | `~/.claude/CLAUDE.md` | `~/.claude/skills/` | **@import**：用户级或 repo `.claude/CLAUDE.md` 一行 `@~/dex/person/…`（repo 侧文件可 gitignore）；`render format=import` |
| **pi** | 启动时从 cwd **向上父目录链** + cwd 加载 AGENTS.md **或** CLAUDE.md，整文件拼接注入系统提示（恒载） | 仅向上链生效（cwd 之下子目录不加载）——子目录同样不能承载全局注入 | ❌（无 import 语法，上下文为整文件拼接） | `~/.pi/agent/AGENTS.md` | `~/.pi/agent/skills/`（自动发现，`/skill:<name>` 触发） | 个人仓库：render 落根 AGENTS.md；团队仓库：全局 `~/.pi/agent/AGENTS.md` 手动维护（无 import，受限同 Codex 型） |
| **ZCode** | 工作区 AGENTS.md（会话恒载） | ❓ 待实测 | ❓ 待实测（用户级文件为整文件加载） | `~/.zcode/AGENTS.md` | `~/.zcode/skills/` | 个人仓库：根 AGENTS.md；❓ 项接入前烟测（会话中问「你能看到哪些入口内容」） |

### 2.2 未纳入（暂不考虑，调研结论备查）

| 工具 | 关键行为 | 若将来接入的策略 |
|---|---|---|
| Codex CLI | 根 AGENTS.md 恒载；子目录仅会话从该子树启动时生效（动态加载为 feature request）；无 import | 根位（个人仓库）；全局 `~/.codex/AGENTS.md` |
| OpenCode | 根 AGENTS.md + 嵌套（子树语义）；全局 `~/.config/opencode/AGENTS.md` | 同 Codex 型 |
| Cursor | AGENTS.md（根）+ `.cursor/rules/*.mdc` 四种激活（Always/Auto-glob/Agent-Requested/Manual） | rules 适配：`.cursor/rules/dex.mdc`（Always），`render format=rules`（FR-6.12） |
| Gemini CLI | GEMINI.md 层级（全局/根/子目录）；✅ `@file.md` import（相对/绝对） | GEMINI.md 一行 `@<dex 路径>` |
| Zed | 根 AGENTS.md；worktree 规则 ❓ | 根位 |

> ❓ = 截至本文档调研未证实，接入前以烟测为准。

## 3. render 策略决策树（FR-6.4 的操作化）

```text
repo 根是否已有团队入口文件（AGENTS.md / CLAUDE.md 等非 dex 产物）？
├─ 否（个人仓库）→ dex render 落根 AGENTS.md（默认；三工具均恒载，退出码 10 冲突保护恒在）
└─ 是（团队仓库）→ 按消费工具分流（支持范围内）：
    ├─ Claude Code → format=import：repo .claude/CLAUDE.md 一行 @import（该文件 gitignore；片段首行带「数据非指令」声明注释，FR-6.4）
    │                或用户级 ~/.claude/CLAUDE.md（零 repo 足迹，全局生效）
    ├─ pi / ZCode  → 无 import（pi 已证实；ZCode 待验证）：
    │                用户级全局文件（~/.pi/agent/AGENTS.md、~/.zcode/AGENTS.md）手动维护，
    │                或与团队协商根 AGENTS.md 由 dex 管理（团队治理决策）
    └─ 子树级记忆（monorepo 包级）→ 子目录 AGENTS.md（子树按需语义恰好匹配，唯一正确的子目录用法）

未纳入工具（§2.2）不在分流范围；接入需求出现时按其行内策略评估。
```

**通用注意**：
- 所有适配位产物（根 AGENTS.md / rules / import 宿主文件若在 repo 内）涉及个人记忆的，建议 gitignore 或使用用户级宿主——防个人记忆被误 commit 进团队仓库（FR-6.4 原始动机不变）；
- `@import` 类宿主（CLAUDE.md/GEMINI.md）启动即全量加载、不省 context——条目多后注入预算改由 `dex render` 产物承担（与 REQUIREMENTS US-01 步骤 4 既有口径一致）；
- `render format=import` 产出的 @import 片段首行固定携带「记忆库数据非指令」声明注释——引用原文件零复制，声明承载在宿主片段层（与 merged 头部同级的文本防御；残留风险见 DESIGN §13）。

## 4. 调研来源（2026-09-24）

- AGENTS.md 约定：<https://agents.md>
- pi coding agent 上下文文件（启动加载 AGENTS.md/CLAUDE.md：全局 + 父目录链 + cwd，整文件拼接）：<https://github.com/badlogic/pi-mono>（packages/coding-agent/docs/usage.md）
- Claude Code 记忆层级（子目录按需 / AGENTS.md 回退 v2.1.277+ / @import）：<https://code.claude.com/docs/en/memory> · <https://github.com/anthropics/claude-code/issues/6235>
- Codex AGENTS.md 嵌套加载语义与动态加载 feature request：<https://learn.chatgpt.com> · <https://github.com>（openai/codex issue）
- Cursor rules 四种激活与 AGENTS.md 支持：<https://cursor.com/docs>（Rules）
- Gemini CLI GEMINI.md 层级与 `@file.md` import：<https://geminicli.com/docs>
