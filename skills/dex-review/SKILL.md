---
name: dex-review
description: "个人记忆中枢（dex）周回顾七段走查：lint 体检、inbox 待裁决提案、journal 提升、衰减清单＋Spoke 周报、scope 升降级、近义与矛盾组、结构整理——段序同 FR-6.6 权威序，每段附操作说明与建议命令。触发词：周回顾、review、清 inbox、记忆库经营、每周整理、dex review。"
---

# dex-review：周回顾七段走查

你的角色：周回顾会话的走查助手。训练家主持并**裁决一切**；你按**七段权威段序**（FR-6.6，唯一口径——不得增删段、不得换序）逐段呈现清单并附建议命令。每个动作一个 git 提交，历史即审计；裁决权永远在人，你只呈现与执行人的指令。

权威依据：REQUIREMENTS FR-6.6/FR-9、US-04/US-06；DESIGN §2.3/§5.6/§6.3。

## 开场

- v1 起：运行 `dex review`（可 `--group <一级域前缀>` 只出单组）生成七段清单，按清单走查。
- v0（无 CLI）：按同一段序手动收集——每段下文附 v0 手动等价操作。
- 节奏纪律：**单批 ≤15 分钟**；到点未清完 → 记录断点，分批继续，直至 inbox 清空。**inbox 周清空是硬约束**：要么归位、要么删除，不允许堆积成第二待办清单。

## 段① lint 结构体检

- 操作：`dex lint`。
- v0 手动等价：检查 scope 文件 frontmatter 残留、顶层目录是否只有八大 scope 目录与固定合法项、目录命名 kebab-case（文件名允许 CJK）、`inbox/`（含 `inbox/bootstrap/` 草稿）滞留、src / superseded-by / keep-until 注释格式与两级作用域位置。
- 处置：发现项当场修或登记进段⑦。frontmatter 残留 = 归位动作未完成——回到段②补剥。

## 段② inbox 待裁决提案

逐条呈现：**来源 / 证据 / 置信度 / 正文**；有条件时按 evidence 回放原文供对照。**无证据提案直接否决**（FR-4.2），有疑义（找不到证据、疑似以泛充真）同样倾向否决。

裁决与建议命令：

- **确认归位**：人编辑内容 → 剥掉 frontmatter → `git mv inbox/<file> <scope>/<path>` → 提交 `review: promote inbox/<file> → <scope>/<path>`
- **编辑后归位**：先改写再移动，同一提交模板。
- **否决**：`git rm inbox/<file>`（活在 git 历史）→ 提交 `review: reject inbox/<file>`
- v0 遗留 `inbox/bootstrap/` 草稿同受裁决：逐条对照 src 溯源查证后归位或删除。

循环直至 inbox 清空，再进下一段。

## 段③ journal 提升候选

- 操作：浏览本周 `journal/*.md`（「供稿 · <source>」小节与「手写」小节）。
- 裁决：值得长期保留的事实 → 提升进 scope（新建或并入主题文件），建议补溯源 `<!-- src: journal YYYY-MM-DD -->`（FR-2.6）；项目快照类事实（「本周把认证换成 OAuth」）默认留 journal 自然衰减——只有影响后续决策的才提升。
- 建议命令：`git add` + 提交 `review: promote journal/<date>.md → <scope>/<path>`（沿用归位模板）。

## 段④ 衰减清单＋Spoke 使用周报粘贴区

- 操作：`dex stale`（缺省 90 天，可按 scope 分档）；v0 手动按 git log 最后变更日期近似。请训练家**粘贴各 Spoke 使用周报**——被引用条目豁免衰减。
- 裁决三选：
  - **归档**：`git mv <path> archive/<原scope镜像路径>` → 提交 `review: archive <path> → archive/<mirror>`
  - **改写**：更新原文（改写即续命，触及即重置衰减计时）→ 提交 `review: rewrite <path>`
  - **保留**：加 `<!-- keep-until: YYYY-MM-DD 原因 -->`（条目级紧随条目行 / 文件级紧随 H1，条目级优先；到期前不再列示、到期强制复审——豁免必须有到期日，防永久沉默）→ 提交 `review: keep-until <path> until <date>`

## 段⑤ scope 升降级候选

- **升级**：某应用记忆跨应用成立 → `apps/<x>` → `domains/` 或 `person/`（coding 是高频来源：`projects/foo/` → `domains/coding/`）。
- **降级**：个人记忆收窄 → `person/` 或 `domains/` → `apps/<x>` / `projects/<x>`。
- 建议命令：`git mv <old-path> <new-path>` → 提交 `review: move <old-path> → <new-path>`。
- 目录改名走 **scope 变更协议**：`git mv` ＋ 同步 config / render 白名单 / MCP 白名单 / @import 引用 ＋ 提交留痕——目录即消费方声明的 API，稳定性即契约。

## 段⑥ 近义预筛与矛盾组

- 操作：呈现字符串相似度聚类组（仅聚类不裁决、不落不依赖 `index/`）；矛盾组单独列出。
- 裁决：
  - **近义**：人裁决合并，以 git 最近改写为准 → 提交 `review: merge <a> + <b> → <target>`
  - **矛盾**：人确认旧结论被新结论推翻 → 旧条目加 `<!-- superseded-by: <目标条目/文件> -->`（被标注条目不再参与注入；**禁止任何一侧静默裁决**）→ 提交 `review: supersede <old> by <new>`

## 段⑦ 结构整理候选

- 来源：lint 结构面（段①）——目录软预算（`domains/` 一级 ≤8）、待拆分大文件（单文件条目数 / 字数软上限）、孤儿目录、悬空 scope 引用。
- 纪律：**先内容后结构**——不建空目录；新域准入 = 同类条目 ≥10 且连续两周进入回顾清单；文件拆并即时可做；目录增删改名走段⑤ scope 变更协议。

## 收尾

1. 复扫 `inbox/`：非空 → 显式警告，残留项列为下批首位。
2. 输出本轮摘要：动作计数（promote / reject / merge / move / archive / rewrite / supersede / keep-until）＋ 本轮 `git log` 摘引。
3. 提示：重新启用归档内容 = `git mv archive/<mirror> <path>` → 提交 `review: restore archive/<mirror> → <path>`（触及即重置衰减计时）。

## 附：12 类提交模板速查（一切写动作，DESIGN §2.3）

| 动作 | 模板 |
|---|---|
| 提案落盘 | `inbox: propose from <source>` |
| journal 供稿 | `journal: append from <source>` |
| 收割暂存 | `harvest: stage <source> ×<n>` |
| 归位（确认/编辑后） | `review: promote inbox/<file> → <scope>/<path>` |
| 否决 | `review: reject inbox/<file>` |
| 改写 | `review: rewrite <path>` |
| 归档 | `review: archive <path> → archive/<mirror>` |
| 重新启用 | `review: restore archive/<mirror> → <path>` |
| scope 升降级 | `review: move <old-path> → <new-path>` |
| 合并近义 | `review: merge <a> + <b> → <target>` |
| 矛盾标注 | `review: supersede <old> by <new>` |
| keep-until | `review: keep-until <path> until <date>` |
