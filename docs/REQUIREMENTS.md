# 个人记忆中枢（dex）需求文档

> 项目：pokemon-remember-you（就记得是你）
> 上游文档：[PERSONAL_MEMORY_HUB_PROPOSAL.md](./PERSONAL_MEMORY_HUB_PROPOSAL.md)（方案提案）
> 文档版本：v1.11 · 2026-09-25 · 状态：待评审（v1.11：debt 清偿（Medium 16 + Low 14 全量）——注入优先级字典序权威化（FR-3.2/US-05）、收割暂存区迁 `inbox/staging/`（FR-6.14/FR-2.8）、周回顾七段权威段序（FR-6.6/FR-11.2/US-04）、近义预筛 v1 运行时不落 index/、空目录约束收窄（FR-2.9）、人速记归 journal（FR-4.2/FR-5.1）、FR-2.3 期别 v0、evidence ≤2000 字符（FR-4.1）、scope 越权整单拒绝（FR-7.3）、NFR-3 量化 P95、NFR-5 明示 git 例外（FR-4.5）、config 两层存放（FR-10.2/US-07）、FR-1.6 `.dex-ignore` 回补、FR-6.15 `dex index`、FR-9.5 `--group`、US-01/US-13/v1 验收措辞与量化；此前 v1.10：支持范围收窄——AI coding 工具限定 Claude Code / pi / ZCode（§7 约束、US-02/US-14、FR-11 组、工具矩阵 §2.1），其余移入 TOOL_COMPATIBILITY.md 未纳入备查；更早 v1.9：W3 收口——入口文件策略矩阵化（FR-6.4/FR-6.12）；v1.8：场景推演修订；v1.7：实施前评审修订；v1.6 连接器层架构、v1.5 场景语义回调、v1.3 结构治理、v1.2 零手写冷启动与配套技能、v1.1 AGENTS.md 与内容层安全）
> 配套文档：[DESIGN.md](./DESIGN.md)（详细设计）· [WORK_LIFE_SCENARIOS.md](./WORK_LIFE_SCENARIOS.md)（使用场景调研·指南层）· [SCENARIO_WALKTHROUGH.md](./SCENARIO_WALKTHROUGH.md)（用户场景业务流程推演）· [TOOL_COMPATIBILITY.md](./TOOL_COMPATIBILITY.md)（工具入口兼容矩阵）

---

## 1. 引言

### 1.1 背景

agent 时代，个人的上下文散落在互不可见的孤岛：编码 agent 各自持有 per-project 配置与用户级记忆、各应用持有自己的反馈数据、个人笔记沉淀在 Obsidian/Notion 中 AI 读不到。同一个「我」的事实要在 N 处重复声明；某应用学到的教训别的 agent 无从知晓。

提案（见上游文档）给出了解法：建立一个**应用无关的个人记忆中枢（Hub）**——一棵 git 管理的 markdown 文件树，同时是个人知识库；Spoke（各应用/agent）持有过程数据，Hub 只持有结论；访问通道为文件投影 / CLI / MCP 三条，全部无守护进程。

### 1.2 目标

1. **跨应用记忆共享**：任何 agent / 应用按 scope 声明接入，读取与提案记忆，换工具不失忆。
2. **人机共用的知识库**：同一棵文件树，人用编辑器 / Obsidian 直接维护，AI 经 MCP / CLI / 文件投影消费。
3. **本地优先与主权**：全本地、git 管仓、无守护进程；人是唯一的知识写入口（周回顾确认），agent 永远只能提案。
4. **克制可维护**：强制结构只有「目录即 scope」，条目为纯 markdown 无 frontmatter，规避分类法腐化。

### 1.3 非目标（明确不做）

| 不做 | 理由 |
|---|---|
| 把候选池、置信度计数、审计流水、原始反馈搬进 Hub | 过程数据寿命与知识不同，会阻塞所有应用（提案「大一统诱惑」风险） |
| 云服务、账号体系、遥测 | 与「完全本地」原则冲突 |
| 常驻守护进程 / 服务化 Hub | 文件是稳定契约，协议只是适配器 |
| 重 frontmatter / 本体论 / 自动知识图谱 | 历史教训：schema 越重烂得越快 |
| agent 直写 scope 目录 | 写入主权分级：propose-only 是硬约束 |
| 自动改写 / 自动合并人的知识 | 一切归位与合并由人在周回顾中裁决 |
| 向 Hub 写回使用计数（命中统计） | 避免高频改写文件；使用数据留在 Spoke 侧 |

### 1.4 术语表

| 术语 | 定义 |
|---|---|
| Hub | 个人记忆中枢：`~/dex` 这棵 git 管理的 markdown 文件树 |
| Spoke | 接入 Hub 的应用 / agent（编码 agent、待办应用、写作应用…），持有自己的过程数据 |
| dex | CLI 二进制名；`dex mcp` 子命令即 MCP server；数据仓库 `~/dex` 亦称「图鉴」 |
| scope | 记忆的可见范围层级：`person/ ⊃ domains/ ⊃ apps/ ⊃ projects/`，由目录路径承载 |
| 条目（Entry） | scope 文件中的一个知识点（一条要点），文件按主题组织、一条一个要点 |
| 提案（Proposal） | agent 写入 `inbox/` 的候选记忆，带 source / kind / confidence / evidence 元数据 |
| 固化 | Spoke 内部把反复出现的模式沉淀为结论的管道（其内部另有确认门） |
| 周回顾 | 每周一次的人工经营动作，是系统唯一的知识写入口（确认归位 / 改写 / 否决 / 归档 / scope 升降级） |
| 入口文件 | 渲染给某消费方 scope 合并视图的文件（默认名 `AGENTS.md`，跨工具事实标准），或 Claude Code 的 `@import` 引用 |
| 配套技能（skills） | 本仓 `skills/` 分发的 agent 行为包（SKILL.md 单一源：`dex-bootstrap` / `dex-propose` / `dex-review`）；行为来自用户安装的技能（可信通道），与记忆数据（不可信通道）分离 |
| 连接器页（connector） | 每数据源一页的获取知识（`skills/connectors/<source>.md`，六要素：数据清单/locator 格式/摘录抓取/高发区提示/隐私红线特化/烟测命令）；行为知识而非代码（FR-12） |
| 注入预算 | 消费方单次获得的记忆上限（默认 ≤10 条 / ≤2000 字） |
| 衰减 | 条目 90 天未实质变更且未被引用 → 进入归档候选 |
| 保留豁免（keep-until） | 衰减候选被裁决「保留」时的持久化注释 `<!-- keep-until: YYYY-MM-DD 原因 -->`：到期前不再进入衰减清单（条目保持活性），到期后强制复审；注释类变更不计实质变更（FR-2.10） |
| 客户端（client） | 接入 dex 的消费方统一注册单元（人、本地 Spoke、远程 agent）：持凭证、定义 scopes / 提案权 / 预算 / 限流 / allowed_sources；CLI 与 MCP 共用同一注册表（FR-10.1） |
| 派生索引 | 存于 `.cache/` 的 FTS / 向量索引，gitignore，每机可重建 |

### 1.5 用户画像总览

| 角色 | 身份 | 能力 | 限制 |
|---|---|---|---|
| 训练家（Trainer） | 记忆的主人，唯一的人类用户 | 直接编辑任何文件（最高主权）；执行周回顾；裁决一切写入 | — |
| 本地 Spoke agent | 编码 agent、待办应用、写作/IM 应用等 | 按客户端 scopes 检索读取；向 inbox 提案；经 `dex journal` 向自有小节供稿 | 不能写 scope 目录；写入仅经 propose 与 journal 两命令；提案必须带证据 |
| 远程 / 第三方 agent | 云端或不可信环境中的 agent | 经网关以受限客户端凭证访问，只能读取该客户端授权的 scope 子集 | 默认无任何访问（未注册客户端全拒）；无提案权（可配置降级开放） |
| 人类工具 | 编辑器、Obsidian、grep | 对文件树的一切只读操作 | Obsidian 插件私有缓存留在 `.obsidian/`，不算 Hub 结构 |

---

## 2. 用户场景

以下场景按用户旅程组织，覆盖 v0 → v3 各期。每个场景给出：背景、交互过程、验收要点。

### US-01 零工具冷启动（v0 · 无需手写）

**角色**：训练家
**背景**：Alice 刚决定建仓，还没有安装任何 dex 工具，希望当天就让编码 agent「认识自己」——且不想面对空白页手写。

**过程**：
1. Alice 创建 git 私仓 `~/dex`，建立 `person/`、`domains/`、`apps/`、`projects/`、`journal/`、`inbox/`、`archive/`、`index/` 目录；
2. 安装 `dex-bootstrap` 技能（symlink 本仓 `skills/`，见 FR-11；不支持技能的工具用附录引导词兜底），对任一编码 agent 发起冷启动：技能先采访 5 个核心问题（角色与主业／主力栈与工具／语言与沟通偏好／硬性禁区／常用输出格式），再扫描既有 CLAUDE.md / AGENTS.md / auto memory，产出 `person/profile.md`、`person/preferences.md` 与对应 `projects/<proj>/` 草稿——每条标源（面试条目指回原话、挖掘条目指回源文件，FR-2.7），全部落 `inbox/bootstrap/`，首批 ≤30 条（FR-4.3）；
3. Alice 首次回顾：逐条对照来源裁决（编辑 / 确认 / 否决）后 `git mv` 归位——只有裁决，没有从零手写（手写可选，直接编辑的最高主权随时可用）；
4. 在 `~/.claude/CLAUDE.md` 追加 `@~/dex/person/profile.md`、`@~/dex/person/preferences.md` 引用；项目仓库的 `.claude/CLAUDE.md` 追加 `@~/dex/projects/foo/`（仓库外路径首次 import 会弹一次确认框，同意即可；import 启动即全量加载、不省 context，条目变多后注入预算改由 `dex render` 承担）；
5. 用 Obsidian 打开 `~/dex` 作为 vault，日常直接编辑。

**验收**：当天内，`person/` 经面试 + 收割有人确认归位 ≥10 条且每条带 src 溯源；Claude Code 在任意项目会话中已能遵守 Alice 的写作偏好；Obsidian 可正常打开与编辑；`git log` 有初始提交与首轮回顾提交。全程零代码、零安装，无需手写即可达成上述验收（手写为可选最高主权，US-05）。

### US-02 编码 agent 按项目 scope 消费（v1/v2）

**角色**：本地编码 agent（Claude Code / pi / ZCode——支持范围见 §7 约束）
**背景**：agent 在项目 `foo` 中工作，需要该项目与个人层的上下文，而不是全库倾倒。

**过程**：
1. agent（或其配置）声明消费 scope：`person + domains/coding + projects/foo`；
2. 通道 A（文件投影）：`dex render zcode` 把该 scope 的合并视图渲染为 repo 根入口文件 `AGENTS.md`（个人仓库默认位；若根已有团队入口文件则拒绝覆盖（退出码 10），团队仓库改走 @import / rules 适配——工具兼容矩阵见 [TOOL_COMPATIBILITY.md](./TOOL_COMPATIBILITY.md)，FR-6.4）；
3. 通道 B（检索）：agent 调 `dex search "部署流程" --scope projects/foo`，v1 走 ripgrep，v2 命中 FTS 派生索引，毫秒级返回；
4. 通道 C（MCP）：agent 通过 `dex mcp` 的 `dex_search` / `dex_read` 工具按需检索；
5. 通道 A 的注入内容经过：scope 过滤 → 优先级合并（字典序：① 项目级压过个人层 ② 同级手写压过固化 ③ 新证据压过旧证据，FR-3.2）→ 预算截断（≤10 条 / ≤2000 字）；通道 B/C 检索为 scope 过滤 + limit 截断，不做合并与预算（P5 口径：预算约束注入、不约束检索，设计 §1.1）。

**验收**：注入内容不含未声明 scope（如 `apps/todo`、他人 `domains/personal`）的任何条目；项目级与个人层冲突时，注入项目级并注明来源；超出预算时输出 omitted 计数而非静默截断。

### US-03 应用学到教训后提案（v1/v2）

**角色**：待办应用 choose-you（Spoke）
**背景**：choose-you 的固化管道发现：群聊「摸鱼俱乐部」的消息经 6 次裁决反馈确认全为闲聊，对该用户无待办含义。

**过程**：
1. choose-you 调用 `echo "…" | dex propose --source choose-you --kind pattern --confidence 85 --evidence "chat_feedback #1234 #1301 #1355"`；
2. dex 校验提案：source 与 evidence 必填、confidence ∈ [0,100]、正文 ≤4000 字符，通过则写入 `inbox/2026-09-24-choose-you-7812.md`（含轻 frontmatter），git 自动提交留痕；
3. 校验失败（如无证据）→ 拒绝写入并返回明确错误；
4. 单 source 每日提案数超限（默认 20 条）→ 限流拒绝。

**验收**：`inbox/` 中出现该提案文件且 frontmatter 完整；无证据提案被拒绝；scope 目录无任何变更；`git log` 可追溯提案人与时间；source 与 choose-you 客户端身份一致（FR-4.7）。

### US-04 周回顾：唯一的知识写入口（v1 起，v3 强化）

**角色**：训练家
**背景**：周日下午，Alice 花 15 分钟经营记忆库。

**过程**：
1. `dex review` 汇总生成本周回顾清单（七段，权威段序见 FR-6.6）：
   - ① `dex lint` 结构体检输出（frontmatter 残留 / inbox 命名与字段 / archive 镜像路径 / 注释格式 / 可疑密钥模式；结构面：目录软预算 / 待拆分大文件 / 孤儿目录 / 悬空 scope 引用）；
   - ② inbox 待裁决提案（含来源、证据、置信度）；
   - ③ 本周 journal 每日页中值得「提升」的候选事实（v1 人工浏览，v3 工具预筛）；
   - ④ `dex stale` 衰减清单（90 天未实质变更）＋各 Spoke 使用周报粘贴区；
   - ⑤ scope 升降级候选（应用记忆跨应用成立 → 升；个人记忆收窄 → 降，§3.4 S4）；
   - ⑥ 近义预筛条目组（运行时字符串相似度聚类，仅聚类不裁决、不落 `index/`——`index/` 为 v3 导览，FR-1.5；矛盾组单独列出，见 FR-2.5）；
   - ⑦ 结构整理候选（由 lint 结构面发现生成，§3.4 S6）；
2. Alice 逐条操作：确认归位（编辑内容、剥掉 frontmatter、`git mv` 到目标 scope）/ 否决（删除）/ 改写 / 归档 / scope 升降级 / 合并冗余 / 矛盾旧条目标注 `superseded-by` / 结构整理（目录增删改名走 scope 变更协议、文件拆并）；
3. 每个动作都是 git 提交，历史即审计。

**验收**：`inbox/` 周清空（要么归位要么删除，不堆积）；所有归位条目无 frontmatter 残留、落位目录与内容 scope 匹配；`git log` 完整记录本轮回顾的全部动作。

### US-05 人直接编辑知识库（v0 起）

**角色**：训练家（经 Obsidian / 编辑器）
**背景**：Alice 想更新 `domains/people/李四.md` 的沟通偏好，并补写今日手写日志。

**过程**：直接在 Obsidian 中编辑保存，或 `vim ~/dex/domains/people/李四.md`；`journal/2026-09-21.md` 的「手写」小节随手补写。无需通知任何工具。

**验收**：同一 scope 层内手写内容优先级高于固化内容（跨层冲突按 FR-3.2 字典序——具体性优先，如 projects/ 固化例外压过 person/ 手写通则）；下次 `dex render` / 检索即时反映新内容（含未提交编辑——脏工作区调和，FR-8.3）；git diff 可见变更。手写操作不要求经过任何确认门——直接编辑是最高主权。

### US-06 衰减与归档（v1）

**角色**：训练家 + dex stale
**背景**：`projects/old-website/` 下的项目记忆已闲置。

**过程**：
1. `dex stale --days 90` 扫描：对每个 scope 文件用 `git log` 求「最后实质变更」（排除纯移动/纯格式提交），叠加 Alice 粘贴的 Spoke 引用周报（引用过的条目豁免）；
2. 输出归档候选清单（文件、条目、最后实质变更日期、最近引用记录）；
3. Alice 裁决：`git mv` 4 个文件到 `archive/projects/old-website/`、改写 1 个、保留 1 个（预期近期重启——加 `<!-- keep-until: 2026-12-15 预期重启 -->` 注释，到期前不再列示、到期复审，FR-2.10）。

**验收**：归档文件原文保留、git 历史完整；归档后任何 scope 注入与检索默认不再包含 archive 内容；人重新启用只需 `git mv` 回 scope 目录。

### US-07 多机同步（v0 起，git）

**角色**：训练家（公司机 / 家用机 / 笔记本）
**背景**：三台机器都要用同一个 Hub。

**过程**：正常 `git pull / commit / push` 到私有仓库（自托管或平台私库）。Hub 写入低频（周回顾为主）、条目原子、纯文本，merge 友好；`.cache/` 被 gitignore，各机各自重建。冲突罕见且即内容问题，人解决。

**新机接入**（四步）：① `git clone` 私仓到 `~/dex`（启用仓库层 config 时 `[clients]` 注册表随 clone 自动到位，FR-10.2）；② 部署本机层 config（`~/.config/dex/config.toml`：机器覆盖项与凭据文件路径）；③ 部署本机凭据文件（0600，不入 git）；④ `dex reindex`（全量一次）→ 全功能可用。

**验收**：任一机器的手写编辑、周回顾动作可同步到其他机器；`.cache/`、`.obsidian/` 不进 git；换新机按四步接入（clone → config → 凭据 → reindex）恢复全部能力。

### US-08 远程 agent 受限访问（v2）

**角色**：远程 / 第三方 agent
**背景**：Alice 想让一个云端 agent 记住她的工作偏好，但不信任它接触全部记忆。

**过程**：Alice 在客户端注册表中为该 agent 建立受限客户端（如仅 `person/ + domains/work/`）并颁发独立凭证；远程 agent 经网关（MCP 或受控的 CLI 包装）持该凭证访问时，服务端强制按该客户端白名单过滤读取结果；提案默认关闭（可显式降级开放到 inbox 并加严限流）。

**验收**：白名单外的任何文件内容不出现在该 agent 的任何响应中（CLI `dex read` 与 MCP `dex_read` 同规则）；越权请求被拒绝并写入本机审计日志（`.cache/audit.log`，尽力而为）且 `--json` 返回明确错误；默认配置下新 agent 无任何访问权。

### US-09 跨应用记忆生效（核心价值场景）

**角色**：训练家 + 多个 Spoke
**背景**：choose-you 学到的「周报类任务多在周四下午被提到」经周回顾归位到 `person/preferences.md`；写作 agent 与编码 agent 都该受益。

**过程**：
1. 该条目进入 `person/` 后，属于全应用默认可见层；
2. 写作 agent 周四起草周报时经 `dex_search` 命中；编码 agent 的入口文件（`dex render` 产物）下次刷新即包含该条目；
3. 若三个月后 Alice 习惯改变，她直接手写改写原条目（手写压过固化），或等新提案在周回顾中覆盖。

**验收**：一条记忆一次归位、多应用消费；各消费方无需感知彼此存在。

### US-10 journal 供稿与提升（v1）

**角色**：Spoke 应用 + 训练家
**背景**：choose-you 想把每天的裁决情况留给周回顾用。

**过程**：choose-you 每日经 `dex journal --source choose-you --text "捕捉 3 / 逃走 2（原因码：闲聊×2）"` 向 `journal/2026-09-20.md` 的「供稿 · choose-you」小节追加摘要（命令保证追加不覆盖、密钥扫描与 git 自动提交，FR-5.4；不直写文件）；人随手在「手写」小节补写。周回顾时从每日页「提升」值得长期保留的事实进 scope，其余留在 journal 自然衰减。

**验收**：多来源供稿按小节隔离、不互相覆盖；journal 页面不做注入默认源（情景层）；提升动作留 git 痕迹。

### US-11 派生索引的降级与重建（v2）

**角色**：dex 自身
**背景**：`.cache/` 损坏或版本升级。

**过程**：`dex search` 发现 FTS 索引缺失/过期时，自动降级为 ripgrep 直扫并返回结果（退出码 0 + warning），同时置脏标记——重建延迟到下一次命令在进程内同步执行（无后台任务，设计 §4.2）；`dex reindex [--force]` 全量重建。索引同步有快速路径（git log 增量）与全量路径（mtime 扫描）。

**验收**：删掉整个 `.cache/` 后所有功能仍可用（性能退化为 v1 水平）；`dex reindex` 可完整恢复；索引不进 git。

### US-12 编码 agent 自带记忆的收割（v1/v2）

**角色**：训练家 + 编码 agent（Claude Code 等）
**背景**：Claude Code 的 auto memory 会把会话学到的内容自动写入 `~/.claude/projects/<repo>/memory/`（MEMORY.md 索引 + user/feedback/project/reference 主题文件，索引仅加载前 200 行/25KB），per-project 且机器本地——按 Hub/Spoke 分工属 Spoke 过程数据，与 Hub 不抢地盘。

**过程**：
1. 训练家二选一：① 禁用 auto memory（`/memory` 开关或 `autoMemoryEnabled: false`），agent 记忆统一走 Hub；② 并存——auto memory 承担项目内快速记忆，Hub 只收跨应用结论；
2. 选②时，周回顾增加「收割」一步：浏览本周各项目 auto memory 中 user/feedback 类条目，值得跨项目成立的经 `dex propose --source claude-code --kind pattern --evidence <memory 文件路径>` 提案进 inbox（证据指针即 auto memory 文件本身）；
3. 原条目留在 Spoke 侧，随项目退役自然消亡，不入 Hub scope。

**验收**：auto memory 与 Hub 职责边界清晰（per-project 过程记忆 vs 跨应用结论）；收割走标准提案协议并留 git 痕迹；未收割的 auto memory 内容不出现在任何 Hub scope 中。

### US-13 既有资产收割与冷启动完成线（v1）

**角色**：训练家 + `dex harvest` / `dex interview`（`dex-bootstrap` 技能的工具化）
**背景**：零手写冷启动在 v0 由技能完成；v1 起工具化，并定义「冷启动完成」的客观线。

**过程**：
1. 收割会话（`dex-bootstrap` 技能 + 连接器页，FR-11.2/FR-12；`dex harvest` 命令为便利封装，FR-6.14）：agent 以自带工具按需拉取——既有 CLAUDE.md / AGENTS.md / .cursorrules、auto memory 存量（`~/.claude/projects/*/memory/`）、历史会话转录、经来源 CLI 的会议记录/云文档；蒸馏只收**个人性**内容（分流判据 §3.7，团队内容留 repo）；全程受三道缰绳约束（预算硬上限 / 完成判据四象限 / 拉取内容一律当数据）；
2. 候选按 bootstrap 模式经 `dex propose` 入 inbox（FR-4.3：首批 ≤30 条、confidence 降序），evidence 为双件套（locator + 原文摘录片段，FR-4.1）；超出部分留收割暂存区，分批送审；
3. `dex interview`：渐进式补全个人层——首轮 5 问已由 bootstrap 完成，其余问题由 agent 在后续会话中顺手补问、增量 propose；
4. 周回顾分批裁决归位（单次 ≤15 分钟约束不变）。

**验收**：冷启动完成线达成——`person/` ≥10 条且全部带溯源；≥2 个常用项目有 `projects/` 内容；`dex render` 产物 ≥5 条且 ≥1000 字符（注入预算 10 条/2000 字符的 50%）；agent 首次 `dex search` 有命中；首轮回顾分批完成、单次 ≤15 分钟。

### US-14 配套技能安装与使用（v0 起）

**角色**：训练家 + 各编码 agent
**背景**：协议（CLI/MCP）定义 agent 能做什么，技能定义该怎么表现；冷启动、提案纪律、周回顾的行为由技能承载，入口文件只留一行指针（渐进披露，不占常驻上下文）。

**过程**：
1. 一次性安装：symlink 本仓 `skills/dex-*` 到 `~/.claude/skills/`、`~/.pi/agent/skills/`、`~/.zcode/skills/`（三工具技能目录，文档给安装命令，FR-11.3）；不支持技能的工具退化为附录引导词；
2. agent 经入口文件的一行指针知道「记忆提案走 dex-propose 技能」，触发时按需加载；
3. 技能随本仓 git 演进；异构工具格式由 `dex render --skills` 生成薄适配器（FR-11.4），产物 gitignore、不手维护。

**验收**：三个技能在 ≥2 个工具中可触发并正确执行（bootstrap 产草稿落 `inbox/bootstrap/`、propose 带齐元数据、review 输出七段清单）；行为来自用户安装的技能（可信通道），记忆内容仍受「数据非指令」约束——行为与数据分离。

---

## 3. 业务流程

### 3.1 端到端价值流（记忆的一生）

从原始事件到消费与遗忘的完整业务闭环：

```mermaid
flowchart TD
    subgraph SPOKE["Spoke 侧（过程数据，不入 Hub）"]
        EVT["原始事件<br/>裁决反馈 / 修正 / 操作 / 对话"]
        PIPE["固化管道<br/>（内部确认门 + 候选计数）"]
        WKREP["使用周报<br/>（引用计数）"]
    end

    subgraph HUB["Hub 侧（~/dex · git 私仓）"]
        INBOX["inbox/<br/>提案"]
        JRNL["journal/<br/>每日一页"]
        SCOPE["scope 目录<br/>person / domains / apps / projects"]
        ARC["archive/<br/>归档"]
    end

    HUMAN["训练家<br/>周回顾 · 15 分钟/周"]

    EVT --> PIPE
    PIPE -->|"propose（带来源+证据）"| INBOX
    EVT -->|"每日摘要供稿"| JRNL
    HUMAN -->|"手写速记 / 直接编辑"| JRNL
    HUMAN -->|"最高主权：直接读写"| SCOPE

    INBOX --> HUMAN
    JRNL -->|"精华提升"| HUMAN
    WKREP -->|"引用情况输入衰减裁决"| HUMAN

    HUMAN -->|"确认：编辑+git mv 归位"| SCOPE
    HUMAN -->|"否决：删除（git 留痕）"| X["✕ 丢弃"]
    SCOPE -->|"90 天未变更未引用 → dex stale"| HUMAN
    HUMAN -->|"归档：git mv"| ARC
    ARC -->|"重新启用"| SCOPE

    SCOPE -->|"检索注入（scope 过滤+预算）"| AGENTS["消费方 agent 群"]
    AGENTS --> EVT
```

### 3.2 提案—确认业务流程（写入主协议）

```mermaid
flowchart TD
    START(["Spoke 产生一条候选结论"]) --> JUDGE{"换一个应用还成立吗？<br/>（Hub 入库判据）"}
    JUDGE -->|"否：仅本应用生效"| LOCAL["留在 Spoke 应用内记忆<br/>（apps/ 也算本地，但先过判据）"]
    JUDGE -->|"是"| GEN["生成提案<br/>内容 + source + kind + confidence + evidence"]
    GEN --> VLD{"dex propose 校验<br/>source/evidence 必填 · 大小 · 限流"}
    VLD -->|"拒绝"| ERR["返回错误给 Spoke<br/>（不落盘）"]
    VLD -->|"通过"| W["写入 inbox/ 提案文件<br/>（轻 frontmatter，git 提交）"]

    W --> REVIEW["周回顾（人裁决）"]
    REVIEW --> R1["① 确认归位：人编辑内容、剥掉元数据、<br/>git mv 到目标 scope 目录"]
    REVIEW --> R2["② 编辑后归位：先改写再移动"]
    REVIEW --> R3["③ 否决：删除文件（git 历史留痕）"]
    R1 & R2 --> SCOPE["成为活性记忆，可被检索注入"]
    R3 --> REJ["✕（活在 git 历史）"]
    SCOPE --> UPD{"后续新证据出现？"}
    UPD -->|"是"| RW["周回顾改写原文<br/>旧版在 git 历史"]
    RW --> SCOPE
    UPD -->|"90 天未变更未引用"| ARCH["归档 archive/（不再注入）"]
```

### 3.3 消费注入流程（scope 合并）

```mermaid
flowchart TD
    REQ(["消费方发起检索/注入请求<br/>声明 scope：person + domains/coding + projects/foo"]) --> EXPAND["scope 展开<br/>person 恒在；按声明取目录并集<br/>inbox / archive / index / .cache 恒排除"]
    EXPAND --> COLLECT["收集条目<br/>目录内 .md → 段落/要点级条目"]
    COLLECT --> MERGE["优先级合并排序（字典序依序判定）<br/>① scope 具体性：projects ＞ apps ＞ domains ＞ person<br/>② 手写 ＞ 固化（有 src 注释＝固化，仅同级裁决）<br/>③ 新证据 ＞ 旧证据（git 最后实质变更）"]
    MERGE --> CONFLICT{"同主题近义冲突？"}
    CONFLICT -->|"是"| KEEP["保留最高优先级者并注明来源<br/>（多处冗余是允许的，周回顾再合并）"]
    CONFLICT -->|"否"| NEXT["继续"]
    KEEP --> BUDGET
    NEXT --> BUDGET["预算截断<br/>≤10 条 / ≤2000 字<br/>超出部分丢弃并输出 omitted 计数"]
    BUDGET --> OUT1["入口文件（dex render / @import）"]
    BUDGET --> OUT2["检索结果（dex search / dex_search）"]
```

### 3.4 周回顾操作流程

```mermaid
flowchart TD
    START(["周日 · dex review 生成回顾清单"]) --> S1
    subgraph S1["① 清 inbox"]
        A1["逐条阅读提案<br/>（来源 / 证据 / 置信度）"] --> A2{"裁决"}
        A2 -->|"确认/编辑后归位"| A3["剥 frontmatter · git mv 到目标 scope"]
        A2 -->|"否决"| A4["删除（无证据的直接否决）"]
        A3 --> A5{"inbox 清空？"}
        A4 --> A5
        A5 -->|"否"| A1
    end
    S1 --> S2
    subgraph S2["② 提 journal"]
        B1["浏览本周每日页"] --> B2["值得长期保留的事实 → 提升/新建进 scope"]
        B2 --> B3["其余留在 journal 自然衰减"]
    end
    S2 --> S3
    subgraph S3["③ 处理衰减清单"]
        C1["dex stale 输出 + Spoke 使用周报"] --> C2{"逐条裁决"}
        C2 -->|"归档"| C3["git mv → archive/"]
        C2 -->|"改写"| C4["更新原文（更新即续命）"]
        C2 -->|"保留"| C5["加 keep-until 注释（FR-2.10）<br/>到期前不再列示 · 到期复审"]
    end
    S3 --> S4
    subgraph S4["④ scope 升降级"]
        D1{"某条应用记忆跨应用成立？"}
        D1 -->|"是（升级）"| D2["apps/x → domains/ 或 person/"]
        D1 -->|"否（降级）"| D3["person/domains → apps/x 或 projects/x"]
    end
    S4 --> S5
    subgraph S5["⑤ 合并冗余与矛盾"]
        E1["近义预筛（运行时相似度聚类，不落 index/）"] --> E2["近义：人裁决合并 · 以 git 最近改写为准"]
        E2 --> E3["矛盾：旧条目标 superseded-by<br/>（FR-2.5，禁止静默裁决）"]
    end
    S5 --> S6
    subgraph S6["⑥ 结构整理"]
        F1["lint 结构体检：目录软预算 / 待拆分大文件 /<br/>孤儿目录 / 悬空 scope 引用"] --> F2{"需要变更？"}
        F2 -->|"是"| F3["先内容后结构准入 + scope 改名协议<br/>（git mv + 同步配置与 @import 引用）"]
        F2 -->|"否"| F4["跳过"]
    end
    S6 --> DONE(["结束 · 全程 git 留痕 · 单批 ≤15 分钟 · 分批直至清空"])
```

> 段数口径：本节六步为**裁决操作序**；`dex review` 输出的**清单七段**（FR-6.6）＝ 段① lint 体检置首 ＋ 段②–⑦ 依序对应本节六步（S1 清 inbox → 段②，……，S5 合并冗余与矛盾 → 段⑥，S6 结构整理 → 段⑦）。

### 3.5 多机协作同步流程

```mermaid
sequenceDiagram
    participant HM as 家用机（周回顾发生地）
    participant GIT as git 私仓（远端）
    participant OM as 公司机（日常消费）
    participant CM as 笔记本（旅途中手写）

    Note over HM: 周日 · 周回顾：inbox 清空 + 归位 + 归档
    HM->>HM: 本地 git commit（多次，每动作一提交）
    HM->>GIT: git push

    GIT--)OM: pull（自动或手动）
    OM->>OM: dex reindex 增量更新 .cache/（本机私有）
    OM->>OM: dex render 刷新入口文件

    CM->>CM: 手写 journal + 手写 scope 编辑
    CM->>GIT: git push
    GIT--)HM: pull
    alt 同一文件冲突（罕见）
        HM->>HM: 人工合并（冲突即内容问题，人解决）
    end
    Note over GIT: .cache/ .obsidian/ 已 gitignore<br/>各机独立重建，永不冲突
```

### 3.6 新 Spoke 接入流程

```mermaid
flowchart TD
    A(["一个新应用/agent 想接入 Hub"]) --> B{"接入深度？"}
    B -->|"零依赖"| C["文件投影：dex render 生成入口文件<br/>或 Claude Code 直接 @import"]
    B -->|"脚本/进程集成"| D["CLI：dex search / read / propose"]
    B -->|"agent 生态通用"| E["MCP stdio：dex mcp 拉起<br/>dex_search / dex_read / dex_propose"]
    C & D & E --> F["在配置中声明消费方身份：<br/>哪类应用 · 哪个项目 · 允许的 scope"]
    F --> G["读：按声明 scope 过滤 + 预算截断"]
    F --> H["写：仅 propose → inbox（带证据）"]
    F --> I["供稿：dex journal → journal 自有小节（可选，FR-5.4）"]
    G & H & I --> J(["接入完成 · 无需 Hub 侧任何改动"])
```

### 3.7 开发项目的知识分流（AI coding 场景）

开发项目的记忆本就散落在多层（repo 的 CLAUDE.md/AGENTS.md、agent 的 auto memory、repo 的 docs/ADR、git history）。Hub 不替代其中任何一层，只回答「一条新知识落到哪层」——判据：**属于项目还是属于我**。

| 判据 | 去处 | 例子 |
|---|---|---|
| 只在本次会话/本工具有意义 | Spoke 过程数据（auto memory / journal 供稿） | 「这次重构改了哪些文件」 |
| 属于项目、换机器换人也要 | repo（CLAUDE.md / docs / ADR） | 「构建命令 make dev」「选型决策记录」 |
| 属于「我」在该项目的个人视角、不该 commit | Hub `projects/<proj>/` | 「owner 是李四、周五不发布」「我的部署踩坑」 |
| 换个项目仍成立（关于「我」的开发模式） | Hub `person/` 或 `domains/coding/` | 「提交前必跑 lint」「偏好 uv 而非 pip」 |

三层注入拓扑（互补不替代，优先级合并 FR-3.2 现成可用）：

```text
repo 的 CLAUDE.md/AGENTS.md   → 团队指令层：build/test/规范（不动，随 repo 走）
~/dex/projects/<proj>/        → 个人项目记忆层：我的视角/例外/踩坑
~/dex/person/ + domains/      → 个人通用层：跨项目的我
```

- repo 的 `.claude/CLAUDE.md` 加一行 `@~/dex/projects/<proj>/` 即完成叠加（US-01 步骤 4）；「收编散落的 CLAUDE.md」收窄定义为只收编**个人性**散落内容（CLAUDE.local.md、auto memory 里的个人偏好），团队文件留在 repo；
- coding 是 scope 升降级的高频来源（`projects/foo/` → `domains/coding/`），周回顾 S4 重点关照；
- 项目快照类事实（「本周把认证换成 OAuth」）默认留 journal 自然衰减，只有影响后续决策的才提升 `projects/`。

### 3.8 scope 划分模式（语义中立，示例见场景调研文档）

分域方式是用户约定，工具不内置任何分域本体（设计原则 P3「目录即 scope」/ FR-3.7）——分域粒度与使用场合正交；「工作 / 生活」是场景调研的场景轴（调研各场合下对接哪些类型的应用），不是推荐分域。完整的场景调研（应用接入地图、分域粒度讨论、消费方组合、节奏建议）见 [WORK_LIFE_SCENARIOS.md](./WORK_LIFE_SCENARIOS.md)。

分流判据的语义中立版（两级决策树）：

```text
① 愿意让所有消费方看到吗？—— 是 → person/（恒注入层，只放通用自我事实，FR-3.1）
                                 否 → domains/<你的分域>/
② 关于我，还是关于项目/应用？—— person vs projects/apps（§3.7）
③ 换一个应用还成立吗？—— 不成立则留 Spoke（§3.2）
```

- **授权最小化**：按消费方实际需要授予 scope 子集，默认全拒（FR-10.3 兜底）——公司机/公司托管 agent「不挂个人库全量」是该原则的实例而非场景特例；
- **配置按 scope 键**：衰减分档（FR-6.5）与回顾分组（FR-9.5）均以 scope/一级域为键，不感知任何分域语义。

---

## 4. 功能需求

优先级：P0 = 必须/blocking；P1 = 应该有；P2 = 锦上添花。期别对应提案的 v0–v3。

### 4.1 仓库与结构（FR-1）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-1.1 | `dex` 数据仓库为 git 私仓，默认路径 `~/dex`，可经环境变量 `DEX_ROOT` / 配置覆盖 | P0 | v0 |
| FR-1.2 | 顶层目录固定为：`person/ domains/ apps/ projects/ journal/ inbox/ archive/ index/`，另有 gitignore 的 `.cache/` | P0 | v0 |
| FR-1.3 | scope 层级语义由目录路径承载：`person`（全应用默认可见）＞ `domains/<d>` ＞ `apps/<app>` / `projects/<proj>`；不建路由表、不做本体论 | P0 | v0 |
| FR-1.4 | agent 可写位置仅两处且都必须经命令：提案写入 `inbox/`（经 `dex propose` / `dex_propose`），journal 供稿写入自有小节（经 `dex journal` / `dex_journal`，FR-5.4）——除此之外不存在任何 agent 写入面；`archive/` 内容默认不进入任何注入与检索，显式声明 scope（如 `--scope archive`）可检索 | P0 | v0 |
| FR-1.5 | `index/` 为工具生成的导览（MOC），人不手维护 | P2 | v3 |
| FR-1.6 | `.dex-ignore`（可选，仓库根）：检索与注入的忽略规则（语法同 gitignore）——生效于检索遍历与注入收集（设计 §5.1 排除集），不改变 scope 白名单授权语义；lint 顶层白名单固定收录（FR-2.8） | P2 | v1 |

### 4.2 条目格式（FR-2）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-2.1 | scope 内文件按主题一文件、一条一个要点，**不加 frontmatter** | P0 | v0 |
| FR-2.2 | 来源追溯用 HTML 注释（如 `<!-- src: choose-you 固化 2026-09 · 证据×6 -->`），人读不干扰；有 src 注释视为「固化」内容，无则视为「手写」 | P0 | v0 |
| FR-2.3 | 归位动作必须剥掉提案元数据，scope 内不残留 frontmatter（v0 人工归位即含该动作，US-01/FR-9.1；v1 起 `dex lint` 机检兜底，FR-6.11） | P0 | v0（人工）/v1（机检） |
| FR-2.4 | 同一事实多处出现是允许的冗余；以 git 最近改写为准，周回顾合并 | P1 | v0 |
| FR-2.5 | 矛盾显式化：人确认某条目被新结论推翻时，给旧条目加 `<!-- superseded-by: <目标条目/文件> -->` 注释而非静默删除；被标注条目不再参与注入；禁止任何一侧静默裁决矛盾 | P1 | v1 |
| FR-2.6 | 从 journal 提升条目时建议补溯源注释（如 `<!-- src: journal 2026-09-20 -->`），强化周回顾与衰减裁决信号 | P2 | v1 |
| FR-2.7 | bootstrap 来源条目（面试/收割）一律带 src 注释（如 `<!-- src: bootstrap-interview 2026-09-22 -->`、`<!-- src: harvest ~/.claude/projects/foo/memory -->`），视为固化档；人实质改写后可更新或抹掉注释即升手写 | P1 | v0 |
| FR-2.8 | 结构不变量由 `dex lint` 机检：顶层白名单 = 八大 scope 目录（FR-1.2）＋固定合法项 `.git/`、`.cache/`、`.obsidian/`、`.gitignore`、`.dex-ignore`（名单外顶层目录/文件报错）、目录命名 kebab-case（文件名允许 CJK）、`domains/` 深度 ≤2、`apps/`/`projects/` 一级、inbox 顶层无 >7 天未裁决文件（周清空机检；`inbox/staging/` 收割暂存豁免——未送审缓冲，FR-6.14，转正时全量走守卫）、单文件条目数/字数软上限（超出提示拆分） | P1 | v1 |
| FR-2.9 | 目录准入「先内容后结构」：不建空目录（约束对象 = 内容驱动子目录——domains/apps/projects 及以下；顶层八大结构目录豁免，`journal/inbox/archive/index/` 允许长期为空，US-01 与 `dex init` 一次建齐合法，FR-6.9）；候选新域的散条先落最近 scope，同类条目 ≥10 且连续两周进入回顾清单才建域迁移；`domains/` 一级软预算 ≤8（超出 lint 提示合并）；目录改名走 scope 变更协议（`git mv` + 同步 config/render/MCP 白名单/@import 引用 + 提交留痕）——目录即消费方声明的 API，稳定性即契约 | P1 | v1 |
| FR-2.10 | 保留豁免（keep-until）：衰减候选被裁决「保留」时，在条目加 `<!-- keep-until: YYYY-MM-DD 原因 -->` 注释——到期前 `dex stale` 不再列示（条目保持活性、照常注入与检索），到期后重新进入清单强制复审（豁免永远不是永久决定）；keep-until 属注释类变更，不计实质变更（不重置衰减计时）；格式由 `dex lint` 校验，条目实质变更后残留的过期注释由 lint 提示清理；keep-until 注释两级作用域：条目级（紧随条目行）与文件级（紧随 H1 标题行、对该文件全部条目生效——「预期重启」类项目级豁免用文件级），lint 校验两级格式与位置 | P1 | v1 |

### 4.3 scope 声明与注入（FR-3）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-3.1 | 消费方声明 scope 列表（如 `person,domains/coding,projects/foo`）；`person` 恒在并集；`inbox/archive/index/.cache` 恒排除 | P0 | v1 |
| FR-3.2 | 合并优先级为**字典序比较器**（依序判定，先①后②后③）：① 具体 ＞ 泛化（projects ＞ apps ＞ domains ＞ person）→ ② 手写 ＞ 固化（**仅同级 scope 之间裁决**——如 projects/ 固化例外仍压过 person/ 手写通则，与 FR-3.4 一致）→ ③ 新证据 ＞ 旧证据 | P0 | v1 |
| FR-3.3 | 注入预算：默认 ≤10 条 / ≤2000 字，可按客户端配置（`[clients.<id>].budget`）；「字」= Unicode 字符数，含 markdown 语法、不含 src/superseded-by/keep-until 等注释；超限按优先级截断并输出 omitted 计数 | P0 | v1 |
| FR-3.4 | 项目级与个人层冲突时，注入项目级并注明来源 | P1 | v1 |
| FR-3.5 | scope 子目录递归生效（`projects/foo/**` 全部属于 `projects/foo`） | P0 | v1 |
| FR-3.6 | `projects/<proj>` 的 `<proj>` 命名约定：默认取 git remote 仓库名，个人别名经配置映射（保证多机多工具一致）；repo 个人记忆的叠加经 repo 内 CLAUDE.md 一行 `@import` 或 `dex render` 完成（分流判据见 §3.7） | P1 | v1 |
| FR-3.7 | scope 语义中立：工具不内置任何分域本体（工作/生活等分法均为用户约定，指南见 WORK_LIFE_SCENARIOS.md）；授权遵循最小化原则——按消费方实际需要授予 scope 子集（FR-10.3 默认全拒兜底）；`person/` 恒注入（FR-3.1）⇒ 只宜存放愿意让全部消费方看到的通用自我事实 | P1 | v1 |

### 4.4 inbox 提案协议（FR-4）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-4.1 | 提案文件落 `inbox/`，命名含日期、来源与短 id；frontmatter 字段：`source`（必填）、`kind`（fact/preference/pattern，必填）、`confidence`（0–100，可选）、`evidence`（必填）；evidence 定位为人审时的查证指针而非永久引用（源文件后续清理允许悬空，审结即完成使命）；收割类提案的 evidence 为双件套：locator（回源指针）＋蒸馏时抓取的原文摘录片段（回放默认用片段，不依赖源在线，设计 §5.7）；evidence 总长 ≤2000 字符（Unicode 字符数；locator＋摘录片段合计同限——防绕过正文上限的洪水向量） | P0 | v1 |
| FR-4.2 | 无 `source` 或 `evidence` 的提案在写入时即被拒绝（并在周回顾中直接否决存量无证据提案）；人速记/剪藏默认归 journal 手写小节（FR-5.1）、不走 inbox——证据必填的提案协议面向 agent；人显式自提案经 `dex propose --source human` 并自拟 evidence（FR-4.7） | P0 | v1 |
| FR-4.3 | 提案正文大小限制（默认 4000 字符，Unicode 字符数，CLI 与 MCP 同一口径）；单 source 每日限流（默认 20 条，视为异常上限——常态提案远低于此，持续触顶说明该 source 异常），均可配置；周回顾按「单批 ≤15 分钟、分批直至清空」消化 inbox（§3.4）；bootstrap 模式例外：冷启动收割首批入 inbox ≤30 条（confidence 降序），超出留收割暂存区分批送审（US-13） | P1 | v1 |
| FR-4.4 | `inbox/` 周清空约束：`dex review` 显式提示未清空项，不允许堆积成第二待办清单 | P1 | v1 |
| FR-4.5 | 提案写入即 git 自动提交（留痕），提交信息含 source（模板全集见设计 §2.3）；git 为唯一已声明外部依赖（NFR-5）——不可用时降级为仅落盘不提交 + warning（数据不丢、留痕缺失），可经 config `[git].auto_commit = false` 关闭 | P1 | v1 |
| FR-4.6 | `dex propose` 做机械密钥模式扫描（gitleaks 类正则；默认拒绝、可配置为仅警告），不做语义内容审查——人是裁决者；`dex journal` 供稿同款扫描（FR-5.4）；手写路径建议配置同款 git pre-commit 扫描 | P1 | v1 |
| FR-4.7 | source 强制绑定客户端身份：每个客户端配 allowed_sources 集合（默认 = {客户端 id}），propose 与 journal 供稿的 source 必须属于该集合，否则拒绝——防伪报 source 绕过 per-source 限流与伪造审计；human 客户端限定 source = human；收割脚本/收割会话经注册显式授予其连接器 source（如 harvest-im-x 客户端授 im-x） | P0 | v1 |

### 4.5 journal 每日一页（FR-5）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-5.1 | 每日一页 `journal/YYYY-MM-DD.md`；结构为「供稿 · <source>」小节（应用摘要）+「手写」小节（人）；「手写」小节同时是人速记/剪藏的默认落点（提案生命周期「人直接捕获」路径——不经 inbox 与提案协议，FR-4.2） | P0 | v0 |
| FR-5.2 | 多来源供稿按小节隔离，追加不覆盖；同日缺页可由供稿方或人创建 | P1 | v1 |
| FR-5.3 | journal 属情景层，默认不进入注入 scope；其价值经周回顾「提升」进入 scope | P0 | v0 |
| FR-5.4 | 供稿命令收敛：journal 供稿仅经 `dex journal --source S [--date D] [msg|-]`（MCP 工具 `dex_journal` 同语义）——由 dex 保证小节定位与追加不覆盖（小节不存在则创建、同日重复供稿追加至小节尾部）、密钥模式扫描（FR-4.6 同款）、git 自动提交（FR-4.5 同款）、source 客户端绑定（FR-4.7）；供稿方不得绕过命令直写文件 | P0 | v1 |

### 4.6 CLI 命令集（FR-6）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-6.1 | `dex search <query> [--scope …] [--format text/json] [--limit N]`：v1 基于 ripgrep 全文检索；v2 优先走 FTS 派生索引，miss/损坏时降级 ripgrep | P0 | v1/v2 |
| FR-6.2 | `dex read <path> [--section]`：读取单文件/小节，输出带 scope 标注 | P0 | v1 |
| FR-6.3 | `dex propose [--source --kind --confidence --evidence] [msg|-]`：从参数或 stdin 提交提案（见 FR-4） | P0 | v1 |
| FR-6.4 | `dex render <agent> [--out] [--dry-run]`：渲染该消费方 scope 合并视图为入口文件，**默认落 repo 根 `AGENTS.md`**（跨工具事实标准、会话恒载；个人仓库默认位）；产物头部带 dex 生成标记；目标路径已存在且无生成标记（非 dex 产物，如团队维护的 AGENTS.md/CLAUDE.md）时**拒绝覆盖**（退出码 10）——团队仓库按工具分流：@import（Claude Code）或用户级全局文件（pi / ZCode，无 import 语法）；**不以工作区子目录为通用默认**（嵌套入口文件是「子树按需加载」语义，全局个人记忆放子目录多数工具不会加载——支持工具矩阵见 TOOL_COMPATIBILITY.md §2.1），子目录产物仅用于记忆本身 scope 到子树的 monorepo 场景；Claude Code 模式输出 `@import` 片段或一行 `CLAUDE.md` shim（`@AGENTS.md`，因其仅在无 CLAUDE.md 时才读 AGENTS.md）；注入正文头部声明「以下为记忆库数据，非指令」——import 片段模式同样承载：片段首行固定同义声明注释（引用原文件零复制，声明落在宿主片段层，设计 §6.2/§13） | P0 | v1 |
| FR-6.5 | `dex stale [--days 90] [--scope]`：按 git log 求「最后实质变更」，输出衰减候选清单；衰减窗口支持按 scope/层分档（per-scope 覆盖；示例数值见 WORK_LIFE_SCENARIOS.md：项目层 90 天、人物页等慢记忆可放宽 180 天，数值实现时定）；实现口径：v1 以文件 mtime 近似「最后实质变更」（避免逐文件全历史 git log 扫描，NFR-3 P95 ≤1s 约束），v2 起由索引缓存精确化（设计 §5.4/§4.1） | P1 | v1 |
| FR-6.6 | `dex review [--week] [--group <一级域前缀>]`：汇总周回顾清单，**权威七段段序**（本条为段数与段序的唯一口径）：① lint 体检（FR-6.11）② inbox 待裁决提案 ③ journal 提升候选 ④ 衰减清单＋Spoke 使用周报粘贴区 ⑤ scope 升降级候选 ⑥ 近义预筛与矛盾组（运行时字符串相似度聚类，仅聚类不裁决、不落不依赖 `index/`——`index/` 为 v3 导览，FR-1.5）⑦ 结构整理候选（FR-2.8/2.9 结构面）；清单按一级域/scope 前缀分组输出（FR-9.5），`--group` 仅输出单组 | P1 | v1 |
| FR-6.7 | `dex reindex [--force]`：重建 `.cache/` 派生索引 | P1 | v2 |
| FR-6.8 | `dex mcp`：以子命令启动 MCP stdio server | P0 | v2 |
| FR-6.9 | `dex init [--path]`：生成 v0 目录骨架与 gitignore（脚手架，非必需） | P2 | v1 |
| FR-6.10 | 所有命令支持 `--json` 机器可读输出与稳定退出码；`--json` ≡ `--format json`（同一开关的两种拼写，同给冲突时 `--format` 优先）；JSON 输出统一信封与 `E_*/W_*` 错误/警告枚举见设计 §8.4 | P1 | v1 |
| FR-6.11 | `dex lint`：结构体检——scope 内 frontmatter 残留、inbox 命名与必填字段、archive 镜像路径一致性、src/superseded-by/keep-until 注释格式（含 keep-until 过期残留与两级作用域位置校验，FR-2.10）、superseded-by 目标存在性（目标被归档/删除后悬空提示，FR-2.5）、可疑密钥模式；结构不变量与目录准入检查（FR-2.8/2.9：顶层白名单、命名/深度、目录软预算、空目录、inbox 滞留 >7 天、大文件提示、悬空 scope 引用）；输出问题清单（结果并入 `dex review` 首段） | P1 | v1 |
| FR-6.12 | `dex render` 以 AGENTS.md 为源支持生成各工具适配格式（如 `.claude/rules/`、`.cursor/rules/*.mdc` 路径作用域规则——团队仓库分流用，见 TOOL_COMPATIBILITY.md §3）；适配产物为生成物，人不手维护 | P2 | v2 |
| FR-6.13 | `dex journal`：journal 供稿追加的唯一合法通道（CLI 与 MCP 共用同一校验，见 FR-5.4） | P0 | v1 |
| FR-6.14 | `dex harvest` / `dex interview` 为收割会话**便利封装**（不蒸馏）：harvest 加载指定连接器页与预算配置、管理收割暂存区（`inbox/staging/<source>/`——git 跟踪、随仓库多机同步；待审候选属数据非缓存，不可因删 `.cache/` 丢失，FR-8.4/NFR-4 语义不变；staging 豁免 inbox 滞留 lint（FR-2.8），转正时全量走守卫）、把收割技能产出的候选批量走 bootstrap 模式提案（FR-4.3）；interview 输出渐进式面试草稿提案——蒸馏由 `dex-bootstrap` 技能（agent 会话）完成，命令只提供载荷与门禁（FR-12.4 实现分流的 CLI 侧落点） | P1 | v1 |
| FR-6.15 | `dex index [--rebuild]`：生成/刷新 `index/` 导览（MOC，人不手维护，FR-1.5；与 FR-6.6 段⑥近义预筛共用聚类思路、互不依赖） | P2 | v3 |

### 4.7 MCP 工具面（FR-7）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-7.1 | 工具仅四类：`dex_search`、`dex_read`、`dex_propose`、`dex_journal`；**没有直写 scope 的工具** | P0 | v2 |
| FR-7.2 | `dex mcp` 随 CLI 同一二进制分发，stdio 按需拉起，无常驻进程 | P0 | v2 |
| FR-7.3 | 读与检索按客户端 scopes 强制过滤（CLI 与 MCP 同一规则：`dex read` / `dex_search` 解析出的 scope 必须落在调用客户端白名单内，否则拒绝）；默认拒绝未注册客户端；`dex_read` 请求 journal/inbox 等路径时同样按白名单判定；越权判定为**整单拒绝**（不静默剔除越权项后继续——调用方必须感知申请被拒，防误以为获得完整范围）；MCP `dex_search` 的 `scope` 参数缺省 = 该客户端白名单全集（省略即按已授权范围检索） | P0 | v2 |
| FR-7.4 | MCP 会话的 propose 同样受 FR-4 校验与限流 | P0 | v2 |

### 4.8 派生索引（FR-8）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-8.1 | `.cache/` 存 FTS5 全文索引（v2 起），gitignore，每机可重建 | P0 | v2 |
| FR-8.2 | 向量索引（sqlite-vec）与嵌入模型可插拔，默认关闭 | P2 | v2 |
| FR-8.3 | 索引同步：git log 增量快速路径 + mtime 全量路径 + **脏工作区调和**（未提交编辑经脏文件 mtime 与索引比对后即时重解析，US-05「即时反映」由这条保证，设计 §4.2）；`search` 发现索引过期时降级（ripgrep）＋置脏标记——重建延迟到下一次命令在进程内同步执行（无后台任务，P6；设计 §4.2） | P1 | v2 |
| FR-8.4 | 删除 `.cache/` 后系统功能完整（性能退化到 v1） | P0 | v2 |

### 4.9 周回顾与治理（FR-9）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-9.1 | 周回顾是唯一知识写入口；操作集固定：清 inbox / 提 journal / 处理衰减 / scope 升降级 / 合并冗余与矛盾裁决（superseded-by）/ 结构整理（目录增删改名走 scope 变更协议、文件拆并） | P0 | v0（人工）/v1（工具辅助） |
| FR-9.2 | `dex review` 输出的每项操作附建议命令（如 `git mv` 提示），人执行 | P1 | v1 |
| FR-9.3 | v3：周回顾 UI（独立页面或寄生在某应用的复盘向导） | P2 | v3 |
| FR-9.4 | Spoke 使用周报汇总协议：各 Spoke 按 `dex review` 可消费的固定格式提交引用情况 | P2 | v3 |
| FR-9.5 | `dex review` 清单按一级域/scope 前缀分组输出（组序可配），支持仅清理单组以适配碎片时间；CLI 落点 `dex review --group <一级域前缀>`（FR-6.6） | P2 | v1 |

### 4.10 配置（FR-10）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-10.1 | 消费方统一注册为客户端（`[clients.<id>]`，人也是客户端）：凭证（token）、scopes（读授权）、propose（提案权）、allowed_sources（FR-4.7）、budget（FR-3.3）、rate_limit、render 输出定义（out/format）；CLI 与 MCP 共用同一注册表——换通道不换身份与语义 | P0 | v1（CLI）/v2（MCP） |
| FR-10.2 | 配置为纯文本（TOML）**两层存放**并文档化：仓库层 `~/dex/.dex/config.toml`（可选，git 跟踪）承载非凭证配置（[clients] 注册表、budget/stale/harvest 等，多机随 clone/pull 同步）＋ 本机层 `~/.config/dex/config.toml`（必选缺省）承载机器特有覆盖与凭据文件路径，**同键本机层覆盖仓库层**；客户端凭证存本机凭据文件（0600 权限，不入 git）——凭证永不进仓库层 | P1 | v1 |
| FR-10.3 | 每个客户端持有独立凭证：非交互调用（脚本/Spoke/远程网关）必须显式 `--client <id>`，token 自动从本机凭据文件（0600，不入 git）解析、`DEX_TOKEN` 环境变量可覆盖——命令行不明文传 token；交互式终端默认解析为 human 客户端；未注册或无凭证 = 全拒 | P0 | v1 |
| FR-10.4 | 管理类命令权限分档：render/review/stale/lint/reindex/init 仅 human 客户端可执行；harvest/interview 允许 human 或**显式授权的收割客户端**（FR-12.5 映射）执行——agentic 收割会话由 agent 非交互发起，属合法路径；`dex render <target>` 要求调用方 scopes ⊇ 目标消费方 scopes；human 客户端全量可读（与直接编辑最高主权一致）、allowed_sources = {human} | P0 | v1 |

### 4.11 配套技能（FR-11）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-11.1 | 本仓 `skills/` 为技能单一源（一技能一目录 + SKILL.md，含 name/description/触发词 frontmatter）；技能属工具仓库、不进 `~/dex` 数据仓——数据与行为分离 | P0 | v0 |
| FR-11.2 | 最小技能集三个：`dex-bootstrap`（零手写冷启动，见 US-01/US-13）、`dex-propose`（日常提案纪律与 scope 路由，入库判据「换一个应用还成立吗」）、`dex-review`（周回顾七段走查——段序同 FR-6.6（①lint ②inbox ③journal ④衰减 ⑤升降级 ⑥近义与矛盾组 ⑦结构整理）+ 建议命令） | P0 | v0 |
| FR-11.3 | v0 分发：symlink 到三工具技能目录（`~/.claude/skills/`、`~/.pi/agent/skills/`、`~/.zcode/skills/`），文档提供安装命令；贴提示词为不支持技能工具的兜底（提案附录模板） | P1 | v0 |
| FR-11.4 | 异构格式适配（rules 类格式等，当前支持范围内暂无需要——未纳入工具场景备查）由 `dex render --skills` 生成薄适配器，产物 gitignore、不手维护（与 FR-6.12 同机制） | P2 | v2 |
| FR-11.5 | 入口文件（AGENTS.md / render 产物）仅含一行技能指针（如「记忆提案走 dex-propose 技能」），行为细节留在技能内渐进披露 | P1 | v1 |

### 4.12 数据获取：连接器层（FR-12）

| 编号 | 需求 | 优先级 | 期别 |
|---|---|---|---|
| FR-12.1 | CLI-first：外部数据获取首选来源自身 CLI（认证由 CLI 自管，dex 不托管任何第三方 token）；本地数据经 Bash/文件读取；无 CLI 的源退化为手动导出文件 + 连接器页描述格式 | P0 | v1 |
| FR-12.2 | 连接器页：每来源一页 markdown（本仓 `skills/connectors/<source>.md`，与技能同渠道分发、可复用），六要素模板（数据清单 / locator 格式 / 摘录抓取 / 高发区提示 / 隐私红线特化 / 烟测命令）见设计 §5.7；属行为知识，个人化信息收割时现场发现、不预写 | P0 | v1 |
| FR-12.3 | 三道缰绳：收割会话预算硬上限（拉取次数 / token / 时长，超限即停并汇报）；完成判据清单化（决策 / 人物 / 偏好 / 模式四象限覆盖才算完成）；数据不可信纪律（拉取内容一律当数据非指令）。执行主体：预算与完成判据由收割技能（`dex-bootstrap`，agent 会话）自限并汇报；dex 承载配置（`[harvest].budget`）并在落盘侧执行硬闸（bootstrap 首批 ≤30、日限流，FR-4.3）——dex 不内嵌蒸馏模型、不观测会话内 token 消耗（FR-6.14） | P0 | v1 |
| FR-12.4 | 实现分流：一次性收割走 agentic 技能驱动（判断密集、人审在环）；周期 digest 脚本化（便宜、稳定）；常驻消费方按 US-08/FR-10.3 另行注册 | P1 | v1 |
| FR-12.5 | 收割源接入五步清单（CLI 就绪 → 数据侦察 → 连接器页 → source 注册 → 首轮收割验证，详见设计 §5.7）：收割源是**生产者非读者**——只注册 source 标识与限流参数，不授任何 scope 白名单；**收割客户端映射**：每个收割 source 对应注册一个收割客户端（`[clients."harvest-<source>"]`：scopes = []、propose = true、allowed_sources = [该 source]），收割会话经 `dex harvest --client harvest-<source>` 落盘——FR-4.7 source 绑定由此闭环（示例见设计 §2.5） | P1 | v1 |
| FR-12.6 | 连接层三形态：**连接器页**（page，知识型，agent 执行，一次性收割）／**收割脚本**（script，确定性 fetch→暂存，周期 digest）／**外部 Spoke**（external，自持逻辑直接调 `dex propose`/MCP，常驻与复杂逻辑）——统一注册于 `config [harvest.sources]`，type 字段区分；三种形态写入一律收敛于 `dex propose` 单一门禁（**扩展点的稳定性靠协议不靠 ABI**）；不引入 in-process 插件机制（取舍见设计 §13；受控 wasm 留待 v3 后按需评估） | P1 | v1 |

---

## 5. 非功能需求

| 编号 | 类别 | 需求 |
|---|---|---|
| NFR-1 | 隐私 | 完全本地：无工具自有云依赖、无遥测；采集类命令（harvest 等）经用户显式发起访问其自有数据源（来源 CLI / 本地文件），不违反本地主权；LLM 蒸馏由收割技能在用户自己的 agent 会话内完成——模型选择随该 agent 环境，dex 不内嵌、不托管任何模型（FR-6.14）；敏感内容（凭证、他人隐私）不入 Hub 的约束写入文档（git 历史永久留痕是审计能力也是脱敏负担） |
| NFR-2 | 架构 | 无守护进程：v0/v1 纯文件与 CLI；MCP stdio 按需拉起即退 |
| NFR-3 | 性能 | v1 ripgrep 检索在万条目级仓库 P95 ≤ 1s；v2 FTS 命中 P95 ≤ 50ms；`dex render`（10 条/2000 字预算）P95 ≤ 1s（合成基准见设计 §12 容量行） |
| NFR-4 | 可靠性 | `.cache/` 任意损坏不影响正确性（自动降级 + 可 `reindex` 重建）；git 历史是最终事实源 |
| NFR-5 | 分发 | 单二进制跨平台（macOS/Linux 优先，Windows 可选），无运行时依赖——**git 为唯一已声明的外部依赖**（§7 假设）：仅 propose/journal 自动提交与 stale/review 的 git log 口径用到；git 缺失时读取与落盘不受影响，自动提交降级为 warning（FR-4.5） |
| NFR-6 | 寿命 | 数据为纯 markdown + git：10 年后无任何本工具也可完整读写；协议/工具全部可替换 |
| NFR-7 | 同步 | 条目原子、纯文本、低频写入，git merge 友好；`.cache/` `.obsidian/` 不进 git |
| NFR-8 | 安全 | 路径穿越防护（`..`、symlink 逃逸拒绝）；CLI 与 MCP 强制客户端凭证与 scope 白名单；source 与客户端身份绑定（FR-4.7）；提案限流防洪水；内容层防御——render 产物统一标注「数据非指令」、密钥模式扫描（FR-4.6）防记忆投毒与凭证入库。威胁模型边界（设计 §10）：凭证防误配置与跨客户端最小授权、为远程网关提供身份载体；**不防同用户恶意进程**（token 本地可读、文件树为明文，后者物理不可防） |
| NFR-9 | 可维护 | 强制结构仅「目录即 scope」；frontmatter 仅限 inbox；工具行为可用 grep/git 手工复现（不锁死） |
| NFR-10 | 容量 | 单仓库设计容量 10⁴ 条目级；超出依赖归档衰减而非扩容机制 |

---

## 6. 分期验收标准

| 期 | 范围 | 验收标准（可勾选） |
|---|---|---|
| **v0 约定先行** | 建仓 + 目录 + 零手写冷启动（bootstrap 技能）+ @import + Obsidian | ☐ git 私仓建立，八大目录就位 ☐ `person/` 经面试 + 收割人确认 ≥10 条且每条带溯源（零手写）☐ 三技能 symlink 安装且 `dex-bootstrap` 可完成一轮面试 + 收割 ☐ Claude Code 会话能引用到个人层内容 ☐ Obsidian 打开同一 vault 正常编辑 ☐ 全程零代码、无需手写即可达成全部验收项（手写为可选最高主权，US-05） |
| **v1 CLI** | ripgrep 版 search/read/propose/render/stale/review/lint + interview/harvest（冷启动工具化） | ☐ 七命令（search/read/propose/render/stale/review/lint）全部可用且 `--json` 输出稳定 ☐ ≥1 个应用（choose-you）稳定供稿 journal 与 inbox（目标 2 个） ☐ 一次真实周回顾单批 ≤15 分钟、分批直至 inbox 清空 ☐ render 产物被至少一个 agent 实际消费 ☐ 注入预算与 scope 过滤生效（构造越权用例不泄露）☐ 冷启动完成线达成（US-13） |
| **v2 MCP + 索引** | `dex mcp` + FTS5 → sqlite-vec + 多 agent render | ☐ ≥2 个 MCP 客户端经 `dex mcp` 完成 search/read/propose 全链路 ☐ MCP 无直写工具且白名单外不可读 ☐ FTS 索引使 search 进入毫秒级 ☐ 删除 `.cache/` 后功能不回退为零 |
| **v3 经营强化** | 周回顾 UI + index/ 导览 + Spoke 周报协议 | ☐ 周回顾 UI 完成一轮真实回顾 ☐ `index/` 导览自动生成且人未手维护 ☐ ≥2 个 Spoke 按协议提交使用周报并进入衰减裁决 |

---

## 7. 约束、假设与依赖

**约束**
- 提案既定的架构决策为硬约束：Hub/Spoke 分工、目录即 scope、propose-only、无守护进程、文件为稳定契约。
- **AI coding 工具支持范围**：仅 Claude Code、pi、ZCode 三者；其余（Codex / Cursor / Gemini CLI / OpenCode / Zed 等）暂不考虑——调研结论备查于 TOOL_COMPATIBILITY.md §2.2，接入需求出现时再评估。
- 语言/技术栈未定为契约（设计文档给出建议实现），任何实现必须满足 NFR-5/6。

**假设**
- 用户是单人（训练家本人），无私有多用户需求。
- Spoke 应用具备调用 CLI / MCP 或写文件的能力之一。
- git 为可用基础设施，用户具备基础 git 操作能力（周回顾的 `git mv`/commit）。
- 使用频率假设：Hub 写入低频（周级）；读取中频（会话级）；检索高频但可缓存。

**依赖**
- 姊妹项目 pokemon-choose-you（choose-you）：首个 Spoke，供稿 journal 与 inbox，并提供使用周报（v3 协议）。
- MCP 生态：客户端侧 stdio 支持（成熟）；协议演进风险由「文件是稳定契约」对冲。

---

## 8. 风险与应对（需求侧）

| 风险 | 等级 | 应对（需求层面） |
|---|---|---|
| 大一统诱惑：应用运营数据被搬进 Hub | 高 | FR 级硬约束「不持有」清单（§1.3）；入库判据「换一个应用还成立吗」写入周回顾提示 |
| agent 提案洪水灌满 inbox | 高 | FR-4.2/4.3 证据强制 + 大小/限流；FR-4.7 source 绑定防伪报绕过限流；FR-4.4 分批清空 |
| 分类法腐化（重 schema 烂尾 / 目录蔓延） | 高 | FR-2.1 无 frontmatter 约束；FR-1.5 index/ 工具生成；FR-2.8 结构不变量机检 + FR-2.9 先内容后结构准入与目录软预算；深度组织外包给检索（目录只承载 scope 语义） |
| 上下文污染（错误 scope 注入） | 中 | FR-3.1 白名单 + FR-3.3 预算双保险（需求 US-02 验收） |
| 记忆投毒 / 内容注入（提案正文携带注入指令，归位后随注入扩散到所有消费方，MINJA / AgentPoison 类威胁） | 中 | 周回顾人审确认门（第一道）；render 产物标注「数据非指令」（FR-6.4）；密钥扫描（FR-4.6）；矛盾显式化（FR-2.5）防错误结论静默扩散 |
| 衰减信号弱导致误归档 | 中 | FR-6.5 stale 只出建议；US-06 人工三选一裁决（保留经 keep-until 持久化，FR-2.10，防僵尸候选反复重现）；v3 周报协议补强 |
| 周回顾坚持不下来（人的纪律风险） | 中 | 15 分钟量级设计；dex review 清单化降低操作成本；inbox 周清空硬提示 |
| 冷启动死亡谷（空库无消费价值 → 弃用） | 高 | 零手写冷启动：挖掘 + 面试两源、bootstrap 技能承载、首批 ≤30 条分批人审；冷启动完成线验收（US-13/§6） |
| bootstrap 草稿幻觉 / 以泛充真 | 中 | 逐条标源纪律（面试指回原话、挖掘指回源文件）+ 人审对照 + evidence 必填（FR-4.1/4.2）；达不到溯源要求的条目不进草稿；v0 窗口草稿由技能直写 inbox（无命令守卫——密钥扫描/限流/幂等暂缺位，既定取舍），v1 起统一收敛 `dex propose` |
| git 历史脱敏负担 | 中 | 文档约束敏感内容不入库；git-crypt 整仓加密为可选路径 |
| MCP 协议演进 | 低 | 文件层稳定契约，适配器可替换（NFR-6） |

---

## 9. 附录：本需求与提案的映射

| 提案章节 | 需求文档落点 |
|---|---|
| 三、设计原则 | §1.2 目标、§1.3 非目标、NFR 系列 |
| 四、总体架构 | §3.1 端到端价值流 |
| 五、Hub 组织 / scope 规则 / 条目格式 | FR-1、FR-2、FR-3、§3.3 |
| 六、生命周期 | §3.2 提案—确认流程、US-03/04/06 |
| 七、访问通道与写入协议 | FR-4、FR-6、FR-7、§3.6 |
| 八、维护循环 | §3.4 周回顾流程、FR-9 |
| 九、多机/隐私/信任 | US-07、US-08、NFR-1/7/8、FR-10.3 |
| 十、分期落地 | §6 分期验收 |
| 十一、风险与取舍 | §8 |
| 二之10 / 七之（配套技能行为层） | FR-11、US-14 |
| 八之（冷启动零手写） | US-01、US-13、FR-2.7、FR-4.3 |
| 五之（项目层定位与分流判据） | §3.7、FR-3.6 |
| 场景调研文档 WORK_LIFE_SCENARIOS.md（指南层） | §3.8、FR-3.7、FR-6.5、FR-9.5 |
| 七之（连接器层）/ 十一之（收割缰绳） | FR-12、US-13、FR-4.1、NFR-1 |
