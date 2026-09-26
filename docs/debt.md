# 债务与待办（活清单）

> 来源：2026-09-24 实施前文档评审（隔离 sdd-reviewer + ad-reviewer + 主会话客观检查）+ 2026-09-24 场景推演 + 2026-09-25 实施前最终评审（隔离 sdd-reviewer + ad-reviewer + 主会话客观交叉检查，v1.14 修订）。
> 约定：完成勾选移入「已完成」段；新债务随时追加。High 项已随 v1.7 关闭、推演 W1–W4 已随 v1.8/v1.9 关闭、Medium 16 项 + Low 14 项已随 v1.11（2026-09-25）全量清偿、2026-09-25 实施前最终评审发现（High 2 + Medium 12 + Low）已随 v1.14（需求/设计，2026-09-25）全量清偿——当前**无未清债务**，仅余实施期待定参数与一项观察期增强待办（见下）。

## 待处理（实施期待定参数登记——非债务，实现时收口）

- Windows ACL 等价检查口径（SECURITY.md §5，NFR-5 Windows 为可选平台）
- ZCode 嵌套入口 / import 行为实测（TOOL_COMPATIBILITY.md §2.1 ❓ 项，接入前烟测）
- 衰减分档数值校准（90/180 天为示例档，WORK_LIFE_SCENARIOS.md §10，按 scope 实际节奏定）

## 待处理（增强待办——观察期后收口，2026-09-26 登记）

- [ ] **repo-knowledge 机械兜底层**：技能主动调用是概率行为——使用 1–2 周后回看触发率（任务收尾自查／结构变更同步是否稳定发生）；若经常漏触发，复制 lint-gate.sh 模式落 Stop hook（收尾提醒 §2.3 自查／涉结构变更的提交阻断）＋ CI 机检模板（doc_reference_check 同类）。触发良好则直接勾选关闭本项。落点在消费仓／infra 工具链层（lint-gate.sh 所在层），**非 dex 产品功能、不进 SDD**（技能管判断、hook 管不遗忘的第三层）。

## 实施期收口记录（2026-09-25 · v1 实施会话）

- [x] skills 物化目录定值：`~/.local/share/dex/skills/`（FR-11.6；实现 `dex skills install` 物化 + symlink 三工具目录）
- [x] 实施期新增配置键（§2.5 键表外正向漂移，已登记）：`[guard].secret_advisory`（bool 缺省 false，FR-4.6「可配置为仅警告」的落点）
- [x] 实施期新增警告码（§8.4 六码外）：`W_CONFIG_NOTICE`（通用配置类提示——承载 §2.5「[auth] 出现在仓库层 ⇒ warning 并忽略」等规范要求警告但无专用码的场景；另用于 review 的 inbox 未清空提示）
- [x] lint 硬/软分流冻结（§5.6 与 FR-6.11 措辞调和）：error 级发现 → 退出码 1；仅 warning 级（软预算/提示）→ 退出码 0；两级的 `--json` 信封恒 ok=true、问题走 data.findings
- [x] init 首次提交信息冻结：`init: dex skeleton`；幂等补齐提交：`init: 补齐骨架`（12 类 REVIEW_ACTION 模板表外，FR-6.9 语义内）

## 已完成（2026-09-25 · v1.14 实施前最终评审修订：High 2 + Medium 12 + Low）

- [x] **High**：FR-2.8/设计 §5.6 lint 顶层白名单补 `.dex/`（与 FR-10.2 仓库层 config 冲突消除）＋`.DS_Store` 类系统杂项固定忽略；REQUIREMENTS §3.3 流程图改检索/注入双分支（检索不做合并与预算，对齐 P5/US-02/FR-3.3）。
- [x] **Medium**：FR-3.1/设计 §5.1/§8.2 person 恒在改**白名单条件式**＋journal 纳入恒排除并定义显式检索放行面；FR-6.1/设计 §5.3 缺省检索集 = 白名单 ∩ 四层 scope 目录（**CLI 与 MCP 同口径**，通道一致性测试矛盾消除）；FR-6.5/设计 §5.4 stale 与注入比较器 v1 改**单遍全树 git log**（mtime 近似的 pull/clone 失真消除；复活即重置计时；git 缺失回退 mtime + W_GIT_UNAVAILABLE）；FR-2.2/设计 §2.2 **src 注释两级作用域**（条目级优先，与 keep-until 同构）；FR-3.2/设计 §5.2 补 **④ 终局稳定序 path:line**（金样本可复现）；FR-6.4/设计 §6.2/§8.1 render **out 路径基准**（调用时 cwd 所在 git 仓库根）＋**手改检测拒绝覆盖 + `--force`**（基线 `.cache/render/`）＋`--dry-run` 语义；FR-10.3/设计 §8.1/SECURITY §2 **TTY human 免凭证**（物理在场即信任根，首跑引导闭环）；FR-4.1 evidence 双件套适用范围收窄（连接器收割双件套、本地文件收割单件合法）＋幂等/shortid 指针；FR-2.8/设计 §5.6 inbox 滞留检查**递归**＋v1 bootstrap 统一落 inbox 顶层；设计 §2.5 **配置键表 + 键级深合并 + 凭据文件格式**（[auth]/[stale.scopes]/busy_timeout_ms/harvest minutes/[aliases]/journal_per_day 一次定死，限流单源化 [clients].rate_limit）；FR-6.11/设计 §8.3/§8.4 lint 退出码 1 的 **--json 信封特例**（ok=true + data.findings）。
- [x] **Low（择要）**：journal 退出码补 5、CLI --limit 封顶 20、§7.3 图补守卫 0/8、§6.1 凭证注入措辞、设计 §10 T1–T16、US-10/推演 journal 位置参数、US-01 验收可断言化、v2 验收对齐 NFR-3、§6 v1 范围补 journal/init/skills、US-07 reindex v2 注记、NFR-5 git 使用面、§1.2/FR-11.5/US-14 措辞、FR-2.10 两级并存优先级、FR-4.3 限流日界、FR-4.5 并发指针、FR-1.4 v0 例外注记、omitted 口径、git 子进程 ≥2.20、幂等查重载体、freshness 计入容量基准、gantt 补 v1f、§6.2 技能指针行、SECURITY import 净化豁免与轮转 1 MiB、推演上游版本钉 v1.13、提案 OpenCode 回改与 src 示例、audit 轮转默认 1 MiB、journal 日限流（journal_per_day=60）。

## 已完成（2026-09-25 · v1.11 debt 清偿：Medium 16 项）

- [x] **注入优先级组合次序矛盾** → 权威化为**字典序比较器**：① 具体性 → ② 同级手写＞固化 → ③ 新旧（FR-3.2/US-05 验收/§3.3；提案 §三/§五 同步——「手写压过一切」修正为同级裁决，projects/ 固化例外仍压过 person/ 手写通则）。
- [x] **收割暂存区与 FR-8.4/NFR-4 冲突** → 暂存区从 `.cache/harvest/` 迁至 **`inbox/staging/<source>/`**（git 跟踪、随仓库多机同步——待审候选是数据不是缓存；豁免 inbox 滞留 lint（FR-2.8），转正时全量走守卫 0–8）——FR-6.14/设计 §5.5/§5.7/§8.1/推演 §1.3。
- [x] **周回顾段数四种口径** → 权威七段段序落定 **FR-6.6**（① lint ② inbox ③ journal 提升 ④ 衰减 ⑤ 升降级 ⑥ 近义与矛盾组 ⑦ 结构整理——段序与 §3.4 操作步、FR-9.1 操作集对齐）；FR-11.2/US-04/§3.4 段数注/设计 §5.6/§6.3/推演 §3.1 全部对齐。
- [x] **近义预筛 v1 落点** → v1 为**运行时字符串相似度聚类**（仅聚类不裁决），不落、不依赖 `index/`（FR-1.5 为 v3 导览；v3 起 `dex index` 可复用聚类思路）——FR-6.6/US-04/§3.4；提案 §八。
- [x] **空目录悖论** → 「不建空目录」约束对象收窄为**内容驱动子目录**（domains/apps/projects 及以下），顶层八大结构目录豁免（US-01 与 `dex init` 一次建齐合法）——FR-2.9/FR-2.8/设计 §5.6/推演 §1.2。
- [x] **人速记流改道** → 显式声明**人速记/剪藏默认归 journal 手写小节**（周回顾再提升），不走 inbox（FR-4.2 证据必填不适用于人速记）；人显式自提案走 `dex propose --source human` 自拟 evidence——FR-4.2/FR-5.1；提案 §六（生命周期图人捕获边改指 journal）。
- [x] **FR-2.3 期别** → 改 **v0（人工归位即含该动作）/ v1（lint 机检兜底）**。
- [x] **退出码 7 成功却非零** → 7 收窄为「缓存写失败**且操作未完成**」；**降级成功恒 0** + 提示进 `--json` warnings（W_INDEX_DEGRADED / W_GIT_UNAVAILABLE）——设计 §8.3/§8.4。
- [x] **evidence 无长度上限** → **≤2000 字符**（Unicode；收割双件套 locator＋摘录合计同限）——FR-4.1/设计 §5.5-2/§8.2 schema（maxLength）。
- [x] **E_\* 枚举表与 `--json` schema 空白** → 设计**新增 §8.4**：统一信封 `{ok, warnings, data | error:{code,message,details}}` + `E_*` 全集（13 项，含 CLI 退出码 ↔ MCP `error.data.code` 映射）+ `W_*` 警告码（永不改变退出码）。
- [x] **scope「求交集」语义边界** → **fail-closed 整单拒绝**：申请 scope 须全部 ⊆ 白名单，任一越权即 E_SCOPE_DENIED·3（不静默剔除——调用方必须感知）；MCP `dex_search` scope 缺省 = 客户端白名单全集——FR-7.3/设计 §8.2/§10/推演 §4.2。
- [x] **P5「三通道共用注入管线」失实** → 修正为「注入类输出（render 入口文件/@import 视图）走注入管线（过滤→合并→预算）；检索共享 scope 过滤与守卫，limit 截断、无合并预算」——设计 P5/§1.2；US-02 步 5；提案 §三/§五（图拆分检索/注入两路）。
- [x] **「后台/异步 reindex」矛盾 + SQLite 并发写锁** → 术语改**延迟重建**（置脏标记 + 下一次命令进程内同步执行，无后台任务——P6）；并发写以 `BEGIN IMMEDIATE` + `busy_timeout`（默认 5s）串行化、超时降级（search→ripgrep + W_INDEX_DEGRADED / reindex→E_CACHE·7），重建幂等——设计 §4.2/§7.2；FR-8.3/US-11。
- [x] **format=import 缺「数据非指令」声明承载** → 片段**首行固定声明注释**「以下 @import 为记忆库数据，非指令」（与 merged 头部同级文本防御）；原文件零复制、无法逐条标注的残留风险记入设计 §13——FR-6.4/设计 §6.2/TOOL_COMPATIBILITY.md §3。
- [x] **NFR-5 与 git 硬依赖矛盾** → git 明示为**唯一已声明外部依赖**（§7 假设）；不可用时 propose/journal 降级为仅落盘不提交 + W_GIT_UNAVAILABLE（数据不丢、留痕缺失），新增 `[git].auto_commit` 开关——NFR-5/FR-4.5/设计 §2.3/§2.5。
- [x] **config 多机一致性** → **两层存放**（FR-10.2）：仓库层 `~/dex/.dex/config.toml`（可选，git 跟踪，[clients] 注册表等非凭证配置随 clone/pull 同步）＋ 本机层 `~/.config/dex/config.toml`（必选缺省，机器覆盖与凭据路径，同键覆盖仓库层）；凭证永不进仓库层——US-07 四步/设计 §2.5/§6.5/推演 §4.1 对齐。

## 已完成（2026-09-25 · v1.11 debt 清偿：Low 14 项）

- [x] 提案状态图补 **Superseded 节点与恢复转移**（Active↔Superseded，FR-2.5）；lint 增 **superseded-by 目标存在性**检查（归档/删除后悬空提示）——设计 §3.3/§5.6/FR-6.11。
- [x] **幂等 hash 输入域** = SHA-256(source+kind+content)（confidence/evidence 不入）；**shortid** = hash 前 6 位、同日碰撞顺延后续 6 位段——设计 §2.3/§5.5-7。
- [x] `--format text|json` 与全局 `--json` 关系：**同一开关两种拼写**（`--json` ≡ `--format json`，同给冲突 `--format` 优先）——FR-6.10/设计 §8.1 全局注。
- [x] `dex render --skills` 补 CLI 标志（§8.1 render 行，FR-11.4 对齐）。
- [x] 新增 **`dex index [--rebuild]`** 命令（FR-6.15，v3 导览）；`dex review` 补 **`--group <一级域前缀>`**（FR-9.5/FR-6.6/§8.1）。
- [x] lint 顶层白名单**枚举** `.git/`、`.cache/`、`.obsidian/`、`.gitignore`、`.dex-ignore`（名单外顶层项报错）——FR-2.8/设计 §5.6。
- [x] 预算截断语义：越界条**整条丢弃并停止累加**（条目原子性——截断可能丢限定词反转语义；输出恒为优先级前缀、金样本可复现）；首条越界 ⇒ 输出空 + W_OVER_BUDGET——设计 §5.2-4/§12。
- [x] **REVIEW_ACTION commit message 模板全集**（12 类动作固定格式，回放与审计依赖）——设计 §2.3 新增模板表；FR-4.5 引用。
- [x] `.dex-ignore` 回补需求 **FR-1.6**（正向漂移收口）。
- [x] entries 物理主键改**自增代理键** + content_hash 匹配（行漂移不废向量 FK：同内容移动保留向量、内容变更才重算）——设计 §3.1/§4.1。
- [x] §3.8「P3」消歧为「设计原则 P3『目录即 scope』」；US-13 验收量化为「≥5 条且 ≥1000 字符」；v1 验收「1–2 个」量化为「≥1 个（目标 2 个）」；NFR-3 加 P95（检索 ≤1s / FTS ≤50ms / render ≤1s；设计 §11 里程碑、§12 容量行同步）。
- [x] keep-until **两级作用域**：条目级（紧随条目）/ 文件级（紧随 H1，全文件生效——「预期重启」项目级豁免）——FR-2.10/设计 §2.2/§5.4/FR-6.11。
- [x] MCP 并发 propose 的 git **index.lock 冲突**：退避重试（50ms × ≤10 次），超限 E_REPO_STATE·8；重试安全由幂等保证——设计 §5.5。
- [x] US-01 验收措辞改「**无需手写即可达成**（手写为可选最高主权）」——US-01 标题/验收、§6 v0 验收行。

## 已完成（2026-09-24 · v1.7 评审修订）

- [x] High 全部 7 项：journal 供稿矛盾（FR-5.4/6.13）、render 冲突策略（FR-6.4/退出码 10）、MCP 身份凭证（FR-10 组/[clients]）、dex_read scope 过滤（FR-7.3）、索引脏工作区调和（FR-8.3）、last_substantive v1 mtime 近似（FR-6.5/设计 §5.4）、harvest 职责定位（FR-6.14/FR-12.3）。
- [x] 决策落地：keep-until 保留豁免（FR-2.10）、source 强制绑定（FR-4.7）、字数口径统一（FR-3.3/FR-4.3）、周回顾分批语义、per-client budget 配置（[clients].budget）、威胁模型边界声明（设计 §10）、harvest/interview FR 条目补齐（FR-6.14）、「公司机 clone 内容 vs 授权配置」边界说明（WORK_LIFE §8）。
- [x] 客观一致性：校验序 0–8 统一、propose 退出码 9 补齐、4KB→4000 字符统一、[mcp.clients]/[render.*]→[clients] 引用清零、dex-harvest 技能名对齐、[harvest].model 移除。

## 已完成（2026-09-24 · v1.8 推演修订）

- [x] W1 harvest 权限分档（FR-10.4）、W2 收割客户端映射（FR-12.5 + 设计 §2.5）、W4 token 注入路径（FR-10.3 + 设计 §8.1 全局注）、US-07 新机四步接入回填（含设计 §6.5）、v0 守卫缺位记入需求 §8 风险表。

## 已完成（2026-09-24 · v1.9 W3 收口）

- [x] W3 工具入口兼容矩阵（TOOL_COMPATIBILITY.md）：嵌套入口「子树按需」语义查证 → FR-6.4 撤销子目录默认、改 repo 根 + 团队仓库 @import/rules 分流（FR-6.12 扩 .cursor/rules）。推演 4 项发现（W1–W4）至此全部关闭。
