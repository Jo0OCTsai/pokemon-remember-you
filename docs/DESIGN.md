# 个人记忆中枢（dex）详细设计文档

> 项目：pokemon-remember-you（就记得是你）
> 上游文档：[PERSONAL_MEMORY_HUB_PROPOSAL.md](./PERSONAL_MEMORY_HUB_PROPOSAL.md)（方案提案）· [REQUIREMENTS.md](./REQUIREMENTS.md)（需求文档）
> 文档版本：v1.12 · 2026-09-25 · 状态：待评审（v1.12：安装路径工具化同步（对应需求 FR-6.9/FR-11.6 修订）——§8.1 `dex init` 行澄清含 `git init` 与首次提交、新增 `dex skills` 行、管理命令权限分档补 skills，§7.1 主流程 init/skills 分支，§8.3 退出码 10 口径泛化；此前 v1.11：debt 清偿——P5 注入/检索语义修正（§1.1/§1.2）、两层 config（§2.5）、提交信息模板与幂等 hash 域（§2.3）、Superseded 状态（§3.3）、entries 代理键（§4.1）、延迟重建与并发写锁（§4.2）、截断语义与字典序注（§5.2）、keep-until 两级作用域（§5.4）、evidence 上限与并发 propose（§5.5）、lint 白名单/staging 豁免/superseded 悬空（§5.6）、staging 迁 inbox/staging（§5.7）、import 声明承载（§6.2/§13）、七段段序（§6.3）、CLI 面 --skills/--group/dex index/--json 拼写（§8.1）、scope fail-closed 整单拒绝（§8.2/§10）、退出码 7 收窄（§8.3）、新增 §8.4 JSON 信封与 E_*/W_* 枚举；此前 v1.10：支持范围收窄——AI coding 工具限定 Claude Code / pi / ZCode（§2.5 示例与 FR-6.4 分流口径同步，其余备查 TOOL_COMPATIBILITY.md §2.2）；更早 v1.9：W3 收口；v1.8：场景推演修订；v1.7：实施前评审修订；v1.6 连接器层、v1.5 场景语义回调、v1.3 结构治理、v1.2 冷启动与技能、v1.1 AGENTS.md 与安全）

---

## 1. 设计总览

### 1.1 设计目标与原则（继承提案，作为设计硬约束）

| # | 原则 | 设计含义 |
|---|---|---|
| P1 | Hub 持有结论，Spoke 持有过程 | dex 不提供任何候选池/计数/审计类存储；`.cache/` 只缓存派生索引，不是事实源 |
| P2 | 文件是稳定契约，协议是适配器 | 领域逻辑全部定义在「文件树 + git」之上；CLI/MCP/render 只是 I/O 适配层，可整体重写而不动数据 |
| P3 | 目录即 scope | 不建路由表；scope 解析 = 纯函数（路径 → scope 标识），无持久化路由状态 |
| P4 | 写入主权分级 | 人直写（最高）＞ 周回顾归位 ＞ agent 写 inbox；dex 代码层面**不存在**任何写 scope 目录的代码路径 |
| P5 | 消费有预算 | **注入类输出**（render 入口文件 / @import 视图）固定走注入管线：scope 过滤 → 优先级合并 → 预算截断；**检索（search）共享 scope 过滤与客户端守卫，但为检索语义**（limit 截断，无优先级合并与预算截断）——P5 约束的是注入预算，不约束检索 |
| P6 | 无守护进程 | 所有派生状态可由 `git + 文件树` 完全重建；MCP stdio 按需拉起即退 |

### 1.2 总体架构图

```mermaid
flowchart TB
    subgraph ACTORS["角色"]
        HUMAN["训练家（人）"]
        LOCAL["本地 Spoke agent / 应用"]
        REMOTE["远程 / 第三方 agent"]
    end

    subgraph CHANNEL["访问通道层（协议即适配器，可替换）"]
        direction LR
        CH_FILE["文件投影适配器<br/>dex render → 入口文件 / @import"]
        CH_CLI["CLI 适配器<br/>dex search/read/propose/…"]
        CH_MCP["MCP stdio 适配器<br/>dex mcp → dex_search/read/propose/journal"]
        CH_HUMAN["人类工具适配器<br/>编辑器 / Obsidian / grep（零代码直连文件）"]
    end

    subgraph APP["应用层（dex 二进制内）"]
        direction LR
        CMD["命令分发<br/>cmd"]
        TOOLS["MCP 工具面<br/>tools"]
        GUARD["接入守卫<br/>客户端凭证 · scope 白名单 · 提案/供稿校验 · 限流"]
    end

    subgraph DOMAIN["领域层（纯逻辑，无 I/O）"]
        direction LR
        SCOPE_R["scope 解析器<br/>path ⇄ scope 标识 · 并集展开"]
        INJECT["注入管线<br/>收集→优先级合并→预算截断"]
        QUERY["检索器<br/>ripgrep / FTS / vec 三后端"]
        PROPOSE["提案服务<br/>frontmatter 校验·落盘·git 提交"]
        DECAY["衰减扫描器<br/>git log → 最后实质变更"]
        REVIEW["回顾编排器<br/>inbox+journal+stale+近义 清单"]
        IDXSYNC["索引同步器<br/>增量(git)/全量(mtime)"]
    end

    subgraph INFRA["基础设施层"]
        direction LR
        FS["文件树访问<br/>~/dex markdown"]
        GIT["git 适配<br/>log / mv / commit"]
        CACHE[".cache/ 派生索引<br/>FTS5 · sqlite-vec（可重建）"]
    end

    HUMAN --> CH_HUMAN & CH_CLI
    LOCAL --> CH_CLI & CH_MCP & CH_FILE
    REMOTE --> CH_MCP

    CH_FILE & CH_CLI --> CMD
    CH_MCP --> TOOLS
    CMD & TOOLS --> GUARD --> SCOPE_R & QUERY & PROPOSE
    SCOPE_R --> INJECT
    QUERY & INJECT & PROPOSE & DECAY & REVIEW & IDXSYNC --> FS & GIT
    QUERY & IDXSYNC --> CACHE
    CH_HUMAN -.->|"绕过 dex 直连（最高主权）"| FS
```

要点：

- **三条通道共享同一守卫与 scope 解析**：CLI 与 MCP 的 propose 走同一校验（source/evidence/限流）；render（入口文件）走注入管线（合并＋预算），search 走检索器（limit 截断，无合并预算）——注入语义只归注入通道、检索语义只归检索通道，「换通道不换语义」（P5 口径，§5.2/§5.3）。
- **人类工具直连文件树**是架构内的合法路径（虚线），不经过 dex；这是「文件是稳定契约」的体现。
- **领域层无 I/O**：scope 解析、合并、预算均为纯函数，便于测试与多实现。

### 1.3 dex 二进制组件图

```mermaid
flowchart LR
    subgraph BIN["dex（单二进制）"]
        subgraph FRONT["前端"]
            CLI["cli<br/>参数解析·--json"]
            MCP["mcp<br/>stdio JSON-RPC"]
        end
        subgraph CORE["core（领域服务）"]
            SVC_SCOPE["scope"]
            SVC_INJECT["inject"]
            SVC_QUERY["query"]
            SVC_PROPOSE["propose"]
            SVC_DECAY["stale"]
            SVC_REVIEW["review"]
            SVC_RENDER["render"]
            SVC_INDEX["index"]
        end
        subgraph STORE["store（基础设施）"]
            STORE_FS["fs<br/>文件树遍历"]
            STORE_GIT["git<br/>log/mv/commit"]
            STORE_SQL["sqlite<br/>FTS5/vec0"]
        end
        CFG["config<br/>TOML 加载"]
    end
    CLI --> SVC_SCOPE & SVC_QUERY & SVC_PROPOSE & SVC_RENDER & SVC_DECAY & SVC_REVIEW & SVC_INDEX
    MCP --> SVC_SCOPE & SVC_QUERY & SVC_PROPOSE
    CORE --> STORE_FS & STORE_GIT & STORE_SQL
    CFG -.-> BIN
```

### 1.4 技术选型（建议实现，语言不是契约）

| 项 | 选择 | 理由 |
|---|---|---|
| 实现语言 | **Rust** | 单二进制跨平台（NFR-5）；直接内嵌 ripgrep 同源 crates（`ignore` + `grep-searcher` + `grep-regex`）获得 v1 检索能力；`rusqlite` 带 FTS5 feature；生态内有官方 `mcp-sdk`（rust-sdk）实现 stdio server |
| 备选 | Go | 交叉编译方便、MCP SDK 成熟；ripgrep 需以子进程或 `go-fork` 依赖存在，检索语义略偏 |
| 派生索引 | SQLite（FTS5；v2 可选 sqlite-vec vec0 虚表） | 单文件、零服务、嵌入 rusqlite；与「每机可重建」天然契合 |
| 配置 | TOML（`~/.config/dex/config.toml`） | 人可读可改，与 git 生态习惯一致 |
| 嵌入（可选） | 本地 ONNX/whisper 级小模型或外部 API，经 trait 注入 | 默认关闭（NFR-1 无网络）；接口先留好 |

> 任何实现（含社区重写）只要满足 §2 的文件契约与 §8 的接口语义，即为合法实现——这是 P2 原则的落地。

---

## 2. 数据设计：文件契约（稳定层）

### 2.1 仓库目录规范

```text
~/dex/                                # DEX_ROOT（默认 ~/dex，可配置）
├── person/                           # [scope:person] 全应用默认可见
│   ├── profile.md
│   └── preferences.md
├── domains/                          # [scope:domains/*] 领域层
│   ├── coding/…                      #   scope:domains/coding（递归）
│   └── people/李四.md                #   scope:domains/people
├── apps/<app>/…                      # [scope:apps/*] 应用层
├── projects/<proj>/…                 # [scope:projects/*] 项目层
├── journal/YYYY-MM-DD.md             # [scope:journal] 情景层，默认不注入
├── inbox/YYYY-MM-DD-<source>-<id>.md # 提案区（经 dex propose 写入）；journal 供稿小节为第二 agent 写入面（FR-5.4）
├── archive/<原scope路径镜像>/…        # 归档：默认不注入、可检索需显式
├── index/                            # 生成式导览（工具产出，人不维护）
├── .cache/                           # 派生索引（gitignore，每机重建）
│   └── index.sqlite3
├── .dex-ignore                       # 可选：检索/注入忽略规则（语法同 gitignore）
└── .gitignore                        # 忽略 .cache/ 与 .obsidian/
```

规则：

- scope 标识 grammar：`person | domains/<seg>(/<seg>)* | apps/<seg> | projects/<seg> | journal | inbox | archive | index`；`<seg>` 不含 `/`，目录即语义，**无**路由表文件。
- `archive/` 内部镜像原 scope 路径（`archive/projects/foo/x.md`），保证「从哪来、可回哪去」。
- scope 子目录递归归属（FR-3.5）：`projects/foo/sub/deep.md` ∈ `projects/foo`。
- `projects/<proj>` 的 `<proj>` 默认取 git remote 仓库名，个人别名经 config 映射（FR-3.6），保证多机多工具一致。
- 目录命名 kebab-case、`domains/` 深度 ≤2、顶层目录白名单与 scope 改名协议等结构不变量见 §5.6（FR-2.8/2.9）。

### 2.2 scope 条目文件格式

```markdown
# 偏好                                    ← 一文件一主题（H1）
- 周报类任务多在周四下午被提到，标题习惯「写周报-MM/DD」
- 中文写作避免「进行」「予以」一类冗词
<!-- src: choose-you 固化 2026-09 · 证据×6 -->   ← 可选溯源注释（HTML 注释，人读不干扰）
```

- 条目单元 = 列表项（`-` 开头）或独立段落；H2/H3 为文件内分组。
- `src` 注释存在 ⇒「固化」内容；不存在 ⇒「手写」（参与优先级合并，见 §5.2）。
- `superseded-by` 注释（FR-2.5）：旧结论被人确认推翻时标注（如 `<!-- superseded-by: person/preferences.md#周报 -->`），被标注条目不参与注入；从 journal 提升的条目建议带 `<!-- src: journal YYYY-MM-DD -->` 溯源（FR-2.6）。
- `keep-until` 注释（FR-2.10）：衰减候选被裁决「保留」时标注（如 `<!-- keep-until: 2026-12-15 预期重启该项目 -->`）——到期前 `dex stale` 不再列示（条目保持活性），到期后重新进入清单强制复审；属注释类变更，不计实质变更（§5.4）；**两级作用域**：条目级（紧随条目行）与文件级（紧随 H1 标题行、对该文件全部条目生效——「预期重启」类项目级豁免用文件级）。
- **禁止 frontmatter**（FR-2.1）；`dex propose` 归位辅助与 `dex review` 负责剥离（FR-2.3）。

### 2.3 inbox 提案文件格式

```markdown
---
source: choose-you          # 必填：来源应用/agent 标识（^[a-z0-9][a-z0-9-]{0,31}$）
kind: pattern               # 必填：fact | preference | pattern
confidence: 85              # 可选：0–100 整数，缺省 50
evidence: chat_feedback #1234 #1301 #1355   # 必填：非空，指向 Spoke 侧过程数据
---
群聊「摸鱼俱乐部」的消息全为闲聊，对该用户无待办含义
```

- 文件名：`inbox/YYYY-MM-DD-<source>-<shortid>.md`；`shortid` = 幂等 hash（SHA-256(source + kind + content)）十六进制前 6 位，防重放；同日同 source 前缀碰撞 ⇒ 顺延取后续 6 位段，仍碰撞报 E_BAD_ARGS。
- 收割类提案 evidence 为双件套（FR-4.1/§5.7）：locator（回源指针，如 `im-x:<chat_id>#<msg_id>`）＋原文摘录片段——回放默认用片段，不依赖源在线；evidence 总长 ≤2000 字符（locator＋摘录合计，§5.5-2）。
- 写入即 `git commit -m "inbox: propose from <source>"`（可配置关闭自动提交：`[git].auto_commit`，§2.5）；git 不可用时降级为仅落盘不提交 + warning W_GIT_UNAVAILABLE（NFR-5：git 为唯一已声明外部依赖，§8.4）。

**git 提交信息模板**（一切写动作的固定格式——REVIEW_ACTION 回放与审计依赖；前缀与路径由命令/技能生成，人可追加正文注）：

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

### 2.4 journal 每日页格式

```markdown
# 2026-09-20
## 供稿 · choose-you          ← 每个 Spoke 一个小节，追加不覆盖
- 捕捉 3 / 逃走 2（原因码：闲聊×2）
## 手写                        ← 人的小节
- 今天定了个人记忆中枢的方案；周报改为周四下午处理
```

- 供稿冲突策略：同 source 同日重复供稿追加到该小节尾部；不跨小节改写。
- 供稿唯一通道是 `dex journal` / `dex_journal`（FR-5.4）：小节定位与追加、密钥扫描、git 自动提交、source 客户端绑定（§5.5）均由命令保证，供稿方不直写文件。

### 2.5 配置文件（两层存放，FR-10.2）

**仓库层** `~/dex/.dex/config.toml`（可选，git 跟踪）：非凭证配置——[clients] 注册表、budget/stale/harvest 等，多机随 clone/pull 同步。**本机层** `~/.config/dex/config.toml`（必选缺省）：机器特有覆盖与凭据文件路径，**同键覆盖仓库层**；凭证只存本机凭据文件（0600，不入 git）——凭证类键永不进仓库层。单机用户仅用本机层即可。下例为本机层：

```toml
root = "~/dex"                       # 仓库根；环境变量 DEX_ROOT 优先

[budget]                             # 注入预算默认值（FR-3.3）
entries = 10
chars  = 2000

[clients.human]                        # 训练家本人（FR-10.3/10.4）：交互式终端默认身份
interactive_default = true             # TTY 下免显式 --client；凭证从本机凭据文件（0600）自动取
read = "full"                          # 全量可读（与直接编辑最高主权一致）
allowed_sources = ["human"]            # source 绑定（FR-4.7）

[clients."work-laptop-zcode"]          # 示例分法的工作侧消费方：最小授权 scope 子集（FR-3.7，示例见 WORK_LIFE_SCENARIOS.md）
scopes = ["person", "domains/work", "projects/current"]
# budget = { entries = 10, chars = 2000 }                  # 可选 per-client 覆盖（FR-3.3）
render = { out = "AGENTS.md", format = "merged" }        # 默认 repo 根（个人仓库；嵌套入口为子树按需语义，
                                                         # 不作通用默认——工具矩阵见 TOOL_COMPATIBILITY.md）
# format：merged / import / rules（.claude/rules/、.cursor/rules/*.mdc，P2，FR-6.12——团队仓库分流用）

# 未纳入工具预留示例（Cursor 暂不考虑——支持范围见 TOOL_COMPATIBILITY.md §2.1/2.2）：
# [clients.cursor]                     # rules 适配形态（FR-6.12，接入需求出现时启用）
# scopes = ["person", "domains/coding"]
# render = { format = "rules", out = ".cursor/rules/dex.mdc" }   # Always 激活

[clients.claude]                       # Claude Code 走 @import，不复制正文
scopes = ["person", "domains/coding"]
render = { format = "import" }         # 产出 @~/dex/... 引用片段

[clients."choose-you"]                 # 首个 Spoke（CLI 集成；非交互调用须 --client + 凭证）
scopes = ["person", "apps/todo"]
propose = true
rate_limit = { proposals_per_day = 20 }
allowed_sources = ["choose-you"]       # 缺省即 {客户端 id}，显式写出便于阅读

[clients."tg-bot"]                     # 示例分法的生活侧消费方：IM bot（FR-3.7/US-08）
scopes = ["person", "domains/life", "apps/todo"]
propose = true
rate_limit = { proposals_per_day = 20 }

[clients."cloud-writer"]               # 远程/第三方（US-08）：经网关持凭证访问，最小授权子集
scopes = ["person", "domains/work"]
propose = false

[clients."harvest-im-x"]               # 收割客户端（FR-12.5 映射）：收割会话/脚本的落盘身份——
scopes = []                            # 生产者非读者：不授任何读 scope
propose = true                         # 仅可经 dex harvest / dex propose 落盘 inbox（受 harvest 限流约束）
allowed_sources = ["im-x"]             # 仅可署名其连接器 source（FR-4.7 绑定闭环；FR-10.4 允许其执行 harvest/interview）

# 未注册客户端（CLI 无身份/凭证、MCP 未知 clientInfo）一律全拒（FR-10.3）。

[stale]
days = 90                            # 缺省；可按 scope/层分档覆盖（FR-6.5；示例值见 WORK_LIFE_SCENARIOS.md）

[git]
auto_commit = true                   # 提案/供稿落盘即自动提交（FR-4.5）；git 不可用时降级仅落盘＋warning（NFR-5）

[harvest]                            # 收割会话（FR-12，§5.7）
budget = { pulls = 50, tokens = 500000 }   # 三道缰绳之一：会话预算硬上限——由收割技能（dex-bootstrap）
                                           # 在 agent 会话内执行自限并汇报；dex 只承载配置并在落盘侧
                                           # 硬闸（首批 ≤30、日限流），不内嵌蒸馏模型（FR-6.14）

[harvest.sources."im-x"]             # 收割源注册（FR-12.5）：生产者非读者，不授任何 scope
type = "page"                        # 连接层三形态（FR-12.6）：page 连接器页 / script 收割脚本 / external 外部 Spoke
first_batch = 30                     # bootstrap 首批上限（FR-4.3）
proposals_per_day = 20

[index]
fts = true                           # v2
vectors = false                      # 默认关闭（NFR-1）
```

### 2.6 派生索引存储（`.cache/index.sqlite3`）

缓存非事实源，仅由 `index` 服务读写；结构见 §4。

---

## 3. 逻辑数据模型（ER 图）

### 3.1 领域实体关系

说明：Hub 物理上是文件树 + git，以下为**逻辑模型**；「物理映射」列指出每个实体的实际存储。

```mermaid
erDiagram
    SCOPE ||--o{ ENTRY : "目录包含"
    SCOPE ||--o{ SCOPE : "父 scope（层级递归）"
    SOURCE ||--o{ PROPOSAL : "提交"
    SOURCE ||--o{ EVIDENCE : "持有（留在 Spoke 侧）"
    PROPOSAL ||--o{ EVIDENCE : "引用"
    PROPOSAL ||--o| ENTRY : "确认归位后成为（人裁决）"
    ENTRY ||--o{ EVIDENCE : "溯源注释引用"
    JOURNAL_PAGE ||--o{ JOURNAL_SECTION : "分节（供稿/手写）"
    SOURCE ||--o{ JOURNAL_SECTION : "供稿"
    JOURNAL_PAGE ||--o| ENTRY : "周回顾提升（弱关系）"
    ENTRY ||--o{ GIT_COMMIT : "变更历史"
    PROPOSAL ||--o{ GIT_COMMIT : "写入/裁决历史"
    AGENT_PROFILE ||--o{ SCOPE : "消费授权（白名单）"
    AGENT_PROFILE ||--o| BUDGET : "注入预算"
    REVIEW_SESSION ||--o{ REVIEW_ACTION : "执行"
    REVIEW_ACTION }o--|| ENTRY : "作用于（归位/改写/归档/合并/升降级）"
    REVIEW_ACTION }o--|| PROPOSAL : "作用于（确认/否决）"

    SCOPE {
        string scope_id PK "person|domains/coding|apps/todo|projects/foo"
        string kind "person|domain|app|project|journal|inbox|archive|index"
        string path "目录相对路径"
        string parent_id FK "可空，自引用"
    }
    ENTRY {
        string entry_id PK "path:lineNo（逻辑定位；缓存物理键为代理键，§4.1）"
        string file_path "scope 内 .md 相对路径"
        string topic "H1 主题"
        text content "一条要点"
        bool handwritten "无 src 注释=true"
        date last_substantive_change "git log 求得"
    }
    PROPOSAL {
        string file_path PK "inbox/…md"
        string source FK
        string kind "fact|preference|pattern"
        int confidence "0-100"
        date created_at
        string status "pending|promoted|rejected"
    }
    EVIDENCE {
        string ref "chat_feedback #1234（Spoke 侧指针）"
        string source FK
    }
    JOURNAL_PAGE {
        date page_date PK
    }
    JOURNAL_SECTION {
        string source FK "供稿方；手写=human"
        text summary
    }
    SOURCE {
        string source_id PK "choose-you|zcode|human"
        string trust "local|remote"
    }
    AGENT_PROFILE {
        string agent_id PK "配置中的消费方名"
        bool propose_allowed
    }
    BUDGET {
        int max_entries
        int max_chars
    }
    REVIEW_SESSION {
        date session_date PK
    }
    REVIEW_ACTION {
        string action_type "promote|reject|rewrite|archive|merge|scope_up|scope_down"
        string resulting_commit FK
    }
    GIT_COMMIT {
        string commit_sha PK
        datetime at
        string actor "人|source 应用"
    }
```

### 3.2 实体 → 物理存储映射

| 逻辑实体 | 物理存储 | 读取方 |
|---|---|---|
| SCOPE | 目录路径本身（无持久化状态） | `scope` 解析器（纯函数） |
| ENTRY | markdown 文件中的列表项/段落 | 注入管线、检索器 |
| PROPOSAL | `inbox/*.md` + frontmatter | `propose`/`review` |
| EVIDENCE | **不在 Hub**——指针字符串留在提案 frontmatter / `src` 注释，实体数据在 Spoke | 人（周回顾）经 Spoke 查证 |
| JOURNAL_PAGE / SECTION | `journal/YYYY-MM-DD.md` 小节 | `review` |
| AGENT_PROFILE / BUDGET | `config.toml` | 接入守卫 |
| REVIEW_SESSION / ACTION | git 提交序列（`git mv`、删除、改写各为一 commit） | 人 / `review` 回放 |
| GIT_COMMIT | git 对象库 | 衰减扫描器 |

> 设计含义：**ER 图中除 `.cache` 外没有任何 dex 自有数据库**——审计=git log，路由=目录，内容=markdown。这是 P2/P3 原则的直接推论。

### 3.3 状态模型（提案生命周期，细化提案第六节）

```mermaid
stateDiagram-v2
    direction LR
    [*] --> Proposal_Pending : dex propose 校验通过<br/>落 inbox/（git commit #1）
    Proposal_Pending --> Active : 周回顾确认<br/>剥frontmatter + git mv（#2）
    Proposal_Pending --> Rejected : 否决·删除（git 留痕）
    state Active {
        [*] : 位于 scope 目录
        Injected : 被检索/注入引用<br/>（计数留在 Spoke）
        Updating : 新证据出现
        Injected --> Updating
        Updating --> Injected : 周回顾改写原文（旧版入 git 历史）
    }
    Active --> Archived : 衰减·git mv archive/<镜像路径>
    Archived --> Active : 人重新启用
    Active --> Superseded : 人标注 superseded-by（FR-2.5，不再注入）
    Superseded --> Active : 人撤销标注（恢复注入）
    Rejected --> [*]
    Archived --> [*]
    Superseded --> [*]
```

---

## 4. 派生索引设计（`.cache/`）

### 4.1 SQLite 表结构（ER 图）

```mermaid
erDiagram
    files ||--o{ entries : "解析出"
    entries ||--o| entries_fts : "全文索引（rowid 关联）"
    entries ||--o{ embeddings : "向量（可选）"
    meta ||--o{ files : "记录同步水位"

    files {
        INTEGER file_id PK
        TEXT path UK "相对 ~/dex 的路径"
        INTEGER mtime_ns
        TEXT last_commit "最后内容变更 commit sha（增量同步时更新；stale/注入比较器的精确缓存源，§5.4）"
        INTEGER size
    }
    entries {
        INTEGER id PK "自增代理键（行漂移安全，见下注）"
        INTEGER file_id FK
        INTEGER line_no "条目起始行（定位用；行漂移时更新）"
        TEXT content_hash "条目内容 hash（重解析匹配键）"
        TEXT scope_id "冗余，免 join"
        TEXT topic "H1 主题"
        TEXT content "条目文本"
        INTEGER handwritten "0|1"
    }
    entries_fts {
        TEXT content "FTS5 虚表（tokenizer=unicode61）"
    }
    embeddings {
        INTEGER entry_id FK
        TEXT model
        BLOB vector "vec0 虚表（v2 可选）"
    }
    meta {
        TEXT key PK "schema_version|head_commit|last_full_scan"
        TEXT value
    }
```

> entries 物理主键为**自增代理键**而非 `path:line`（§3.1 的 `path:lineNo` 仅为逻辑定位）：行漂移（上方插/删行）不使 FTS/向量关联作废——重解析时按（file_id、条目序、content_hash）匹配既有行，命中则保留 id 与 embeddings（同内容移动不重算向量），内容变更才删除重插；entries_fts 以 rowid 关联、embeddings 以 id 关联。

### 4.2 同步流程（系统流程图）

```mermaid
flowchart TD
    NEED(["需要索引<br/>search/reindex/render"]) --> FRESH{"meta.head_commit == git HEAD<br/>∧ worktree 干净（无未提交变更）？"}
    FRESH -->|"是"| HIT["索引可用"]
    FRESH -->|"否：HEAD 前进"| DELTA{"能取 git 增量？<br/>（log meta.head..HEAD）"}
    FRESH -->|"否：worktree 脏<br/>（未提交编辑，US-05 最高主权路径）"| MTIME{"脏文件 mtime 比对<br/>files.mtime_ns"}
    MTIME -->|"有差异"| RE["对差异文件集<br/>删旧行→重新解析写入<br/>（meta.head_commit 不动）"]
    MTIME -->|"无差异"| HIT
    DELTA -->|"是（常规）"| INC["增量路径：对变更文件集<br/>删旧行→重新解析写入<br/>更新 meta.head_commit"]
    DELTA -->|"否（shallow/损坏）"| FULL["全量路径：mtime 扫描全树<br/>重建 files/entries/fts"]
    INC & RE & FULL --> VLD["校验：抽样 files.path 实际存在"]
    VLD --> HIT
    HIT --> USE["查询走 FTS / vec0"]
    VLD -.->|"失败"| DEGRADE["标记索引不可信<br/>search 本次降级 ripgrep<br/>置脏标记：下次命令进程内重建"]
```

- 任何时刻 `.cache/` 可删除：`search` 检测缺失/过期 → 降级 ripgrep（v1 行为）+ 提示（FR-8.3/8.4）。
- `dex reindex --force` = 强制全量路径。
- **脏工作区调和（FR-8.3）**：人直接编辑后未 commit 时 git HEAD 不动——FRESH 判定叠加 `git status --porcelain` 快速检查，脏文件经 mtime 与 `files.mtime_ns` 比对后即时重解析（US-05「即时反映新内容」的保证）；提交后自然回到增量路径。
- **无后台任务（P6）**：所谓「异步/后台重建」一律指**延迟到下一次命令在本进程内同步执行**——置脏标记（meta 表）只是状态，不产生任何后台进程/线程。
- **并发写策略（v2）**：同机多 dex 进程（如两个 MCP 会话）并发写 `index.sqlite3`——写事务以 `BEGIN IMMEDIATE` + `busy_timeout`（默认 5s，可配）串行化；等待超时方降级：search → ripgrep（退出码 0 + W_INDEX_DEGRADED，§8.4）、reindex → E_CACHE · 7；脏标记与重建幂等（以 meta 水位为准），并发重复重建结果一致。

---

## 5. 关键算法设计

### 5.1 scope 解析（纯函数）

```
输入：声明 scope 列表 D（如 ["domains/coding", "projects/foo"]）＋ 仓库根
输出：可见目录集合 V

V = { person/ }                                # 恒在
V ∪= { 展开(d) | d ∈ D }                       # 递归子目录
V −= { inbox/, archive/, index/, .cache/, .obsidian/ }   # 恒排除
V −= ignore(.dex-ignore)                       # 可选忽略
非法输入（未知 scope、路径穿越）→ 拒绝并报错
```

### 5.2 注入管线（合并 + 预算）

```
输入：V，预算 B=(entries=10, chars=2000)
1. 收集：V 内 *.md → 解析条目（列表项/段落），得候选集 C
2. 打分排序（**字典序比较器**，依序判定——① 先于 ② 先于 ③；「手写压过固化」仅同级裁决，跨层以具体性优先，FR-3.2）：
   a. scope 具体性：projects > apps > domains > person（路径深度近似）
   b. 手写 > 固化（handwritten 标志）
   c. last_substantive_change 新 > 旧（v1 = mtime 近似 / v2 = files.last_commit 缓存，§5.4）
3. 冲突处理：同主题（topic 相同或近义组）多条 → 取最高优先级者注入并附来源标注；
   已标 superseded-by 的条目直接排除；矛盾未裁决时输出附「冲突未决」提示（禁止静默裁决，FR-2.5）；
   其余保留在库中（允许冗余，周回顾合并）——注入侧只做选择，不改数据
4. 预算截断：按优先级序逐条累加，加入后使 entries/chars 任一越界的下一条**整条丢弃并停止累加**（条目原子性：截断可能丢限定词反转语义；不跳过续填——输出恒为优先级前缀，render 金样本可复现）；首条即越界 ⇒ 输出空 + warning W_OVER_BUDGET（提示拆分，与 FR-2.8 单文件软上限同源信号）；
   输出 omitted = |C| - |注入集|，显式告知（FR-3.3）
输出：有序条目列表 + omitted 计数
```

### 5.3 检索后端（三态降级）

```
dex search q [--scope D]：
  D 缺省 = person + 全部 domains + 全部 apps + 全部 projects（不含 journal/archive/inbox）
  1. v1：ripgrep（ignore + grep-searcher crates）直扫 V ∪ .dex-ignore
  2. v2：先查 .cache FTS（schema_version 匹配且未过期）
       命中 → 返回（毫秒级）
       miss/损坏 → 降级 ripgrep 并置脏标记（下次命令触发重建）
  3. 输出行：path:lineNo:scope_id:content，--json 时结构化
```

### 5.4 衰减扫描（`dex stale`）

```
对 V 内每个文件 f：
  last_substantive(f) = max{ commit.date | commit ∈ git log --follow f
                             ∧ diff 触及内容行（非纯 rename/空白/frontmatter-only/注释行——
                             src/superseded-by/keep-until 等注释变更不计实质变更，FR-2.10） }
候选 = { f | now - last_substantive(f) ≥ days(90) }
候选 −= { 条目级 keep-until 未到期的条目 } ∪ { 所在文件带未到期文件级 keep-until 的条目 }   # 保留豁免（FR-2.10 两级作用域）
keep-until 已过期的条目 → 重新列入（强制复审）
叠加 Spoke 使用周报（人粘贴 / v3 协议文件）中被引用条目 → 豁免
输出：文件、条目、最后实质变更、最近引用、建议（archive/rewrite/keep-until）
```

- 分档（FR-6.5）：`days` 可按 scope/层覆盖（示例：项目层 90 天、人物页等慢记忆 180 天，见 WORK_LIFE_SCENARIOS.md）；具体数值 config `[stale]` 定。
- 实现口径（v1 近似 / v2 精确）：v1 无 `.cache`，`last_substantive` 以文件 mtime 近似（stale 与注入比较器 §5.2-c 共用）；v2 起由索引 `files.last_commit` 缓存精确值（增量同步时更新，含义 = 最后内容变更 commit，§4.1），miss 时回退 mtime——禁止 render/stale 每次全量 `git log --follow` 扫描（万条目级下与 NFR-3 P95 ≤1s 约束冲突）。

### 5.5 提案校验（守卫，CLI 与 MCP 共用）

```
校验序（全部通过才落盘；CLI 与 MCP 共用）：
 0 客户端身份与 source 绑定：非交互调用须 --client + 有效凭证；source 必须属于
   该客户端 allowed_sources（缺省 = {客户端 id}；human 客户端 = {human}），
   否则 E_SOURCE_MISMATCH · 退出码 2（FR-4.7/FR-10.3）
 1 source 非空 ∧ 匹配 ^[a-z0-9][a-z0-9-]{0,31}$
  2 evidence 非空 ∧ ≤2000 字符（Unicode；收割双件套 locator＋摘录合计同限。空 ⇒ E_NO_EVIDENCE · 退出码 4；超长 ⇒ E_TOO_LARGE · 退出码 5）
 3 kind ∈ {fact, preference, pattern}
 4 confidence ∈ [0,100]（缺省 50）
 5 正文 ≤ 4000 字符（Unicode 字符数，CLI 与 MCP 同一口径，FR-4.3）
 6 限流：count(inbox 今日该 source) < proposals_per_day（缺省 20）
  7 幂等：hash(source+kind+content) 已存在于未裁决 inbox ⇒ 返回已有文件（幂等成功；confidence/evidence 差异不破坏幂等）
 8 密钥守卫：gitleaks 类正则命中 ⇒ 默认拒绝（E_SECRET · 退出码 9，可配置 advisory 仅警告）
通过 → 写 inbox/ + git commit
（journal 供稿走同一守卫的子集：步骤 0/1/5/8 + 小节级追加，FR-5.4）
```

- bootstrap 模式（FR-4.3）：`dex harvest` / bootstrap 技能与普通提案走同一校验（含密钥守卫），仅第 6 步限流放宽——首批入 inbox ≤30 条（confidence 降序），超出留收割暂存区 `inbox/staging/<source>/`（git 跟踪、随仓库多机同步——待审候选是数据不是缓存，删 `.cache/` 不受影响，FR-8.4；staging 豁免 inbox 滞留 lint（§5.6），转正时全量走守卫 0–8），分批转 inbox 顶层送审。
- **并发 propose / journal（v2）**：git 自动提交遇 `index.lock` 占用（同机多 dex 进程，如两个 MCP 会话）⇒ 固定退避重试（50ms × ≤10 次）；超限报 E_REPO_STATE · 8——调用方重试安全：内容 hash 幂等（校验序 7）保证重放不产生重复提案。

### 5.6 结构不变量（dex lint 检查集）

目录即消费方声明的 API（render 配置、MCP 白名单、`@import` 引用均指向 scope 路径），结构稳定性是契约问题——lint 按以下四层断言（FR-2.8/2.9）：

```text
结构层：顶层白名单 = 八大 scope 目录（FR-1.2）＋ `.git/`、`.cache/`、`.obsidian/`、
        `.gitignore`、`.dex-ignore`（名单外顶层目录/文件 → 错误）；
        目录命名 kebab-case（文件名允许 CJK：people/李四.md 合法）；
        domains/ 深度 ≤2；apps/ projects/ 一级
准入层：无空的内容子目录（domains/apps/projects 及以下；顶层八大结构目录豁免
        ——FR-2.9，US-01/`dex init` 建骨架合法）；domains/ 一级软预算 ≤8（超出提示合并候选）；
        新域准入 = 同类条目 ≥10 且连续两周出现在回顾清单
文件层：单文件条目数/字数软上限（超出提示拆分）；
        H1 主题与文件名一致性（弱提示）
流转层：inbox 顶层无 >7 天未裁决文件（周清空机检；`inbox/staging/` 收割暂存豁免，FR-6.14）；
        archive 镜像路径与来源一致；src/superseded-by/keep-until 注释格式
        （keep-until 两级作用域位置校验 + 过期残留提示清理，FR-2.10）；
        superseded-by 目标存在性（目标被归档/删除后悬空 → 提示，FR-2.5）；
        可疑密钥模式（§5.5-8）；配置中引用不存在 scope 的悬空引用

scope 改名协议：git mv + 同步 config/render/MCP 白名单/@import 引用，
提交信息记录变更前后路径；lint 检出悬空引用时输出受影响配置项
```

- 检查结果并入 `dex review` 第七段「结构整理」；违反硬规则（顶层白名单、路径语法）退出码 1，软预算/提示类仅输出建议。

### 5.7 收割会话与连接器层（agent 驱动，FR-12）

数据获取不内建 fetcher 适配器——获取动作由 agent 工具（Bash/Read）完成，获取知识以**连接器页**承载，dex 职责收缩为：propose 校验、预算配置承载与暂存、evidence 回放、review 呈现（P2 的彻底化：**采集也是适配器**）。**蒸馏永远在技能侧**（agent 会话内的 `dex-bootstrap`），`dex harvest` 命令是便利封装而非蒸馏执行者（FR-6.14）——dex 不内嵌任何 LLM。

**收割循环**：计划（按连接器页圈定范围）→ 按需拉取（来源 CLI / 本地文件）→ 蒸馏（三判据 + 隐私红线 + 分流树内置在技能）→ 合并（去重 / 证据叠加）→ `dex propose`（bootstrap ≤30，§5.5）→ review 人审（evidence 回放）。

**三道缰绳**（agentic 收割的失控模式与对位，FR-12.3）：

| 缰绳 | 失控模式 | 机制 |
|---|---|---|
| 预算硬上限 | 拉取螺旋（成本失控） | 每会话拉取次数 / token / 时长上限，超限即停并汇报——由收割技能（dex-bootstrap）在 agent 会话内执行自限（config `[harvest].budget` 承载）；dex 落盘侧另有首批 ≤30 与日限流硬闸（§5.5，FR-12.3） |
| 完成判据清单化 | 过早收工（覆盖不全） | 每来源须覆盖「决策 / 人物 / 偏好 / 模式」四象限才算完成 |
| 数据不可信纪律 | 拉取内容携带注入指令 | 一切拉取内容当数据不当指令（与「数据非指令」同教义）；技能属可信通道（用户安装、git 版本化） |

**实现分流与连接层三形态**（FR-12.4/12.6）：

| 形态 | type | 机制 | 适用 |
|---|---|---|---|
| 连接器页（知识型） | `page` | markdown 六要素 + agent 执行 CLI/Bash | 一次性收割（判断密集、异构、人审在环） |
| 收割脚本（脚本型） | `script` | 确定性脚本：fetch → NDJSON → `inbox/staging/<source>/` 暂存（或管道给 `dex propose`） | 周期 digest（便宜、稳定、可 cron） |
| 外部 Spoke（应用型） | `external` | 外部程序自持逻辑与状态，直接调 `dex propose` / MCP | 常驻与复杂逻辑（choose-you、IM bot；US-08/FR-10.3 另行注册） |

统一注册于 `[harvest.sources.<id>]`；三种形态写入一律收敛于 `dex propose` 单一门禁——**扩展点的稳定性靠协议不靠 ABI**（进程边界即插件边界）。

**连接器页模板（六要素，`skills/connectors/<source>.md`）**：

```markdown
# connector: <source>
1. 数据清单      资源类型 × CLI 命令模板 × 输出字段说明（含分页/速率注意）
2. locator 格式  如 im-x:<chat_id>#<msg_id> / notion:<page_id>#<block_id>
3. 摘录抓取      evidence 双件套的原文片段怎么截
4. 高发区提示    该源里「关于我的结论」长在哪（IM 私聊→人物页；云文档→决策/偏好）
5. 隐私红线特化  IM：他人原话不收；云文档：团队内容分流去 repo（§3.7 判据）
6. 烟测命令      验证 CLI 活着的最小命令
```

**新源接入五步清单**（有 CLI 的源约 30–60 分钟；收割源是生产者非读者，无需 scope 白名单，FR-12.5）：

1. **CLI 就绪**：安装登录（认证由 CLI 自管）→ 烟测 → 确认 JSON 输出与分页方式；
2. **数据侦察**：枚举可拉类型 → 试拉记录命令/输出/量级 → **确认 locator 可定位性**（无稳定 ID 用「对话名＋时间戳」替代并写明）→ 圈定冷启动范围（不拉全量）；
3. **写连接器页**（六要素模板；个人化信息收割时现场发现、不预写）；
4. **注册 source**：config `[harvest.sources.<id>]` 标识 + 限流（bootstrap 首批 30 / 常规 20/日）；
5. **首轮收割 + 人审反向验证**：review 的 evidence 回放是否点得开、locator 是否指对位置（最常暴露 locator 设计缺陷，改连接器页即闭环）；收尾在 WORK_LIFE_SCENARIOS 来源地图加一行。

无 CLI 的源退化路径：第 1–2 步换成「手动导出文件 + 连接器页描述文件格式与存放路径」，其余不变——获取动作从「agent 跑命令」变为「人定期导出」。

**evidence 双件套**（FR-4.1）：locator（回源指针）＋ 蒸馏时抓取的原文摘录片段——回放默认用片段（快、不依赖源在线、源清理后不悬空），需要深查时按 locator 调 CLI 再拉。

---

## 6. 关键时序图

### 6.1 MCP 消费链路（search / read / propose）

```mermaid
sequenceDiagram
    autonumber
    participant AG as Spoke agent（MCP 客户端）
    participant SRV as dex mcp（stdio，按需拉起）
    participant GD as 接入守卫
    participant DM as 领域服务
    participant FS as 文件树 ~/dex
    participant CA as .cache/index
    participant GT as git

    Note over AG,SRV: 会话启动：客户端 spawn `dex mcp --client <id>`（凭证经参数/环境变量注入）
    AG->>SRV: initialize（stdio JSON-RPC）
    SRV-->>AG: tools = [dex_search, dex_read, dex_propose, dex_journal]

    AG->>SRV: dex_search("部署流程", scope=["projects/foo"])
    SRV->>GD: 校验客户端凭证与 scope 白名单
    alt 未注册 / 越权 scope
        GD-->>SRV: 拒绝
        SRV-->>AG: error E_SCOPE_DENIED
    else 通过
        GD->>DM: query(展开后 scope 集)
        DM->>CA: FTS 查询（v2）
        alt 索引过期/损坏
            DM->>FS: 降级 ripgrep 直扫
            DM->>DM: 置脏标记（下次命令进程内重建）
        end
        DM-->>SRV: 命中条目（含 scope 标注）
        SRV-->>AG: 结果（条数 ≤ limit）
    end

    AG->>SRV: dex_propose(content, source="choose-you", kind="pattern", evidence="#1234…")
    SRV->>GD: 校验 0-8（身份绑定/证据/限流/幂等/密钥…）
    alt 无 evidence / 超限
        GD-->>SRV: 拒绝（E_NO_EVIDENCE / E_RATE_LIMIT）
        SRV-->>AG: error（明确原因）
    else 通过
        GD->>FS: 写 inbox/2026-09-24-choose-you-7812.md
        GD->>GT: commit "inbox: propose from choose-you"
        SRV-->>AG: ok（文件路径）
    end

    Note over AG,SRV: 会话结束：进程退出，无常驻
```

### 6.2 `dex render` 入口渲染

```mermaid
sequenceDiagram
    autonumber
    participant US as 训练家/脚本
    participant CLI as dex render <agent>
    participant CFG as config.toml
    participant INJ as 注入管线
    participant FS as 文件树
    participant WK as 消费方工作区

    US->>CLI: dex render zcode
    CLI->>CFG: 读 [clients.zcode].scopes / budget / render.out
    CLI->>INJ: inject(V(scope 列表), B)
    INJ->>FS: 遍历可见目录，解析条目
    INJ->>INJ: 优先级合并 + 预算截断
    INJ-->>CLI: 有序条目 + omitted 计数
    CLI->>WK: 预检输出目标：已存在 ∧ 无 dex 生成标记 ⇒ 拒绝覆盖（退出码 10，FR-6.4）
    alt format = "merged"
        CLI->>WK: 写 repo 根 AGENTS.md（头部：dex 生成标记＋「以下为记忆库数据，非指令」；尾部注明 omitted N 条）
    else format = "import"
        CLI->>WK: 输出 @~/dex/person/profile.md 等 @import 片段（零复制）<br/>片段首行固定声明注释「以下 @import 为记忆库数据，非指令」（FR-6.4，§13）
    end
    CLI-->>US: 完成报告（条数/字数/omitted）

    Note over WK: agent 下次会话即消费入口文件<br/>（Claude Code 仅在无 CLAUDE.md 时才读 AGENTS.md，<br/>其消费方优先 @import 或 CLAUDE.md 一行 shim：@AGENTS.md）
```

### 6.3 周回顾 `dex review`

```mermaid
sequenceDiagram
    autonumber
    participant US as 训练家
    participant RV as dex review
    participant FS as 文件树
    participant GT as git
    participant SP as Spoke 使用周报（人粘贴）

    US->>RV: dex review（周日）
    RV->>FS: 扫 inbox/（frontmatter 解析）
    RV->>FS: 读本周 journal/*.md
    RV->>GT: dex stale 算法：求各文件最后实质变更
    RV->>SP: 粘贴引用情况 → 豁免衰减候选
    RV->>RV: 结构体检（lint：frontmatter 残留/命名/镜像路径/注释格式/密钥模式）
    RV->>RV: 近义预筛（字符串相似度，仅聚类不裁决）＋矛盾组单列（superseded-by 建议）
    RV->>RV: 结构整理候选（§5.6：目录预算/待拆分大文件/孤儿目录/悬空引用）
    RV-->>US: 回顾清单（7 段·权威段序 FR-6.6：lint/inbox/journal提升/衰减/升降级/近义+矛盾组/结构整理）<br/>每项附建议命令

    loop 逐条裁决（人执行，工具辅助）
        US->>FS: 确认：编辑内容、剥 frontmatter
        US->>GT: git mv inbox/x.md → person/preferences.md（=确认动作）
        US->>GT: 或 否决：rm + commit；或 改写/归档/合并/升降级
    end
    RV->>RV: 复扫 inbox/ 非空 ⇒ 显式警告（周清空约束）
    RV-->>US: 回顾摘要（动作计数 + git log 摘引）
```

### 6.4 提案从产生到多应用生效（跨组件总时序）

```mermaid
sequenceDiagram
    autonumber
    participant SP as choose-you（Spoke）
    participant PP as 固化管道（Spoke 内部）
    participant DX as dex propose
    participant HU as 训练家（周回顾）
    participant HUB as Hub scope 目录
    participant WA as 写作 agent
    participant CA as 编码 agent 入口

    SP->>PP: 6 次裁决反馈收敛出模式
    PP->>PP: Spoke 内确认门通过（过程数据留 Spoke）
    PP->>DX: propose（source/kind/confidence/evidence）
    DX-->>SP: ok → inbox/
    Note over HU: …周日…
    HU->>HU: 周回顾：查证据（经 Spoke 指针）→ 编辑 → git mv
    HU->>HUB: person/preferences.md 新增一条（无 frontmatter）
    HU->>CA: dex render 刷新入口文件
    CA-->>CA: 编码 agent 会话即时生效
    WA->>DX: dex_search("周报 习惯")
    DX-->>WA: 命中该条（跨应用生效，US-09）
```

### 6.5 多机同步与索引重建

```mermaid
sequenceDiagram
    autonumber
    participant A as 机 A（周回顾）
    participant R as git 远端私仓
    participant B as 机 B（日常）
    participant C as 机 C（新机）

    A->>A: 周回顾多动作 = 多 commit（原子、纯文本）
    A->>R: push
    R--)B: pull
    B->>B: dex search → 检测 meta.head ≠ HEAD → 增量索引（git log 快速路径）
    Note over B: .cache/ 本机私有，永不进 git
    C->>R: clone（新机接入①）
    C->>C: 部署 config（[clients] 注册表）② + 本机凭据文件（0600）③
    C->>C: dex reindex（全量路径一次）④
    C-->>C: 全功能可用（US-07 四步接入）
    alt 罕见：A、B 同文件并发修改
        R-->>A: push 冲突
        A->>A: 人工合并（冲突即内容问题）
    end
```

---

## 7. 系统流程图

### 7.1 CLI 主流程（命令分发与仓库定位）

```mermaid
flowchart TD
    START(["dex <cmd> …"]) --> LOC{"定位仓库<br/>DEX_ROOT → config.root → ~/dex"}
    LOC -->|"不存在"| ERR["错误：未找到图鉴仓库<br/>提示 dex init"]
    LOC -->|"存在"| LOAD["加载 config.toml + .dex-ignore<br/>+ 解析客户端身份（--client / TTY→human，FR-10.3）"]
    LOAD --> CMD{"cmd ?"}
    CMD -->|search| Q["检索器：三态降级（§5.3）"]
    CMD -->|read| RD["读文件/小节<br/>路径安全检查"]
    CMD -->|propose| PP["提案服务：校验 0-8 → 落盘 → git commit"]
    CMD -->|journal| JN["供稿服务：守卫子集 → 小节追加 → git commit（FR-5.4）"]
    CMD -->|render| RR["注入管线 → 入口文件/@import 片段"]
    CMD -->|stale| SD["衰减扫描：git log + 周报豁免"]
    CMD -->|harvest| HR["收割便利封装：连接器页/预算配置/暂存区<br/>+ bootstrap 模式提案（不蒸馏，FR-6.14）"]
    CMD -->|interview| IV["渐进式面试：person/ 草稿提案"]
    CMD -->|review| RV["回顾编排：七段清单生成"]
    CMD -->|lint| LT["结构体检：FR-6.11 检查集"]
    CMD -->|reindex| IX["索引同步器：增量/全量"]
    CMD -->|init| IT["脚手架：目录骨架 + .gitignore<br/>+ git init 与首次提交"]
    CMD -->|skills| SK["技能安装：内嵌技能物化 →<br/>symlink 工具技能目录（FR-11.6）"]
    CMD -->|mcp| MC["进入 stdio JSON-RPC 循环（§6.1）"]
    Q & RD & PP & JN & RR & SD & RV & LT & HR & IV & IX & IT & SK --> OUT{"--json ?"}
    OUT -->|"是"| J["结构化输出 + 稳定退出码"]
    OUT -->|"否"| T["人读输出 + 稳定退出码"]
    MC --> LOOP(["按需运行 · 会话结束即退出"])
```

### 7.2 检索三态降级（v2）

```mermaid
flowchart TD
    S(["dex search q --scope D"]) --> GV{"守卫：scope 白名单通过？"}
    GV -->|"否"| DENY["E_SCOPE_DENIED · 退出码 3"]
    GV -->|"是"| IDX{".cache 存在 ∧ schema 匹配 ∧ 未过期？"}
    IDX -->|"是"| FTS["FTS5 查询（毫秒级）"]
    IDX -->|"否"| RG["ripgrep 直扫（v1 行为）"]
    FTS --> CHK{"结果为空 ∧ 疑似索引漂移？"}
    CHK -->|"是"| RG
    CHK -->|"否"| MERGE
    RG --> DIRTY["置脏标记 → 下次命令进程内同步 reindex"]
    DIRTY --> MERGE
    MERGE["scope 标注 + limit 截断"] --> RES(["text / json 输出"])
```

### 7.3 提案落盘流程（含失败路径）

```mermaid
flowchart TD
    P(["propose 载荷<br/>content+source+kind+confidence+evidence"]) --> V1{"source 合法标识？"}
    V1 -->|"否"| E1["E_BAD_SOURCE · 2"]
    V1 -->|"是"| V2{"evidence 非空？"}
    V2 -->|"否"| E2["E_NO_EVIDENCE · 4（FR-4.2）"]
    V2 -->|"是"| V3{"kind ∈ 枚举 ∧ confidence ∈ 0..100？"}
    V3 -->|"否"| E3["E_BAD_ARGS · 2"]
    V3 -->|"是"| V4{"正文 ≤ 4000 字符？"}
    V4 -->|"否"| E4["E_TOO_LARGE · 5"]
    V4 -->|"是"| V5{"今日该 source 提案数 < 限？"}
    V5 -->|"否"| E5["E_RATE_LIMIT · 5（提案洪水防线）"]
    V5 -->|"是"| V6{"内容 hash 幂等命中？"}
    V6 -->|"是"| IDEM["返回既有文件路径（幂等成功）"]
    V6 -->|"否"| W["写 inbox/YYYY-MM-DD-source-id.md<br/>+ git commit"]
    IDEM & W --> OK(["成功 · 0"])
```

---

## 8. 接口设计

### 8.1 CLI 命令规格

| 命令 | 形式 | 输出 | 主要退出码 |
|---|---|---|---|
| `dex search` | `dex search <query> [--scope a,b] [--limit N] [--format text\|json] [--no-index]` | 行：`path:line:scope:content` | 0 命中/1 无结果/2 参数错/3 scope 拒绝 |
| `dex read` | `dex read <relpath> [--section H2标题]` | 文件或小节内容，头部附 scope 标注 | 0/1 不存在/3 scope 超出客户端白名单/6 路径非法（含 `..`、symlink 逃逸） |
| `dex propose` | `dex propose --source S --kind K [--confidence N] --evidence E [msg \| - ]` | 创建的文件路径 | 0/2 元数据或 source 绑定错/4 无证据/5 超限或限流/9 密钥命中 |
| `dex journal` | `dex journal --source S [--date D] [msg \| - ]` | 追加的 journal 小节位置 | 0/2 元数据或 source 绑定错/9 密钥命中 |
| `dex render` | `dex render <agent> [--out PATH] [--dry-run] [--format merged\|import] [--skills]` | 写入路径 + 条数/字数/omitted；`--skills` 生成技能薄适配器（FR-11.4，v2，产物 gitignore） | 0/2 未知 agent/3 scope 拒绝/10 拒绝覆盖（目标非 dex 产物） |
| `dex stale` | `dex stale [--days 90] [--scope …] [--format json]` | 衰减候选清单（含建议动作） | 0（空清单也 0） |
| `dex review` | `dex review [--week N] [--group <一级域前缀>] [--format text\|json]` | 七段回顾清单（权威段序 FR-6.6，含 lint 结果）按一级域分组（FR-9.5；`--group` 仅单组）+ 建议命令 + inbox 清空警告 | 0 |
| `dex lint` | `dex lint [--scope …] [--format text\|json]` | 结构体检问题清单（frontmatter 残留/inbox 命名与字段/archive 镜像路径/注释格式/可疑密钥） | 0 无问题/1 发现问题/2 参数错 |
| `dex reindex` | `dex reindex [--force]` | 重建统计 | 0/7 缓存写失败 |
| `dex harvest` | `dex harvest --from <connector> [--limit 30] [--dry-run]` | 收割会话便利封装（**不蒸馏**，FR-6.14）：加载 `skills/connectors/<source>` 连接器页与 `[harvest].budget` 配置、管理 `inbox/staging/` 暂存区、把收割技能（dex-bootstrap 会话）产出的候选批量走 bootstrap 模式提案——获取与蒸馏均在 agent 侧技能完成，命令只提供载荷与门禁（FR-12/US-13） | 0/2 参数错/5 超出首批上限/9 密钥命中 |
| `dex interview` | `dex interview [--round core\|follow-up]` | 渐进式面试 → person/ 草稿提案（首轮 5 核心问） | 0 |
| `dex init` | `dex init [--path ~/dex]` | 八大目录骨架 + .gitignore；非 git 目录时 `git init` + 首次提交；幂等补齐缺失骨架；git 缺失降级 warning（FR-6.9/FR-4.5） | 0/8 已完整初始化 |
| `dex skills` | `dex skills install [--tool <id>…] [--from PATH]`；`dex skills uninstall [--tool <id>…]` | 内嵌技能物化到 dex 管理目录 → symlink 各工具技能目录（幂等 / 死链重建 / 版本漂移重物化，`--from` 走本仓工作副本；FR-11.6） | 0/2 未知工具/10 拒绝覆盖（目标非本仓产物） |
| `dex index` | `dex index [--rebuild]` | 生成/刷新 `index/` 导览（MOC，人不手维护，FR-1.5/FR-6.15；v3） | 0/2 参数错 |
| `dex mcp` | `dex mcp --client <id>`（凭证经参数/环境变量） | stdio JSON-RPC 循环 | — |

> 全局：非交互调用需显式 `--client <id>`，token 自动从本机凭据文件（0600，不入 git）解析、`DEX_TOKEN` 环境变量可覆盖——命令行不明文传 token（FR-10.3，推演 W4）；交互式终端默认解析为 human 客户端；管理类命令权限分档（FR-10.4）：render/review/stale/lint/reindex/init/skills 仅 human，harvest/interview 另允许显式授权的收割客户端（FR-12.5 映射，如 `dex harvest --from im-x --client harvest-im-x`）。`--format text|json` 与全局 `--json` 为同一开关的两种拼写（`--json` ≡ `--format json`，同给冲突时以 `--format` 为准）；JSON 输出统一信封与 `E_*/W_*` 枚举见 §8.4；降级成功不改变退出码（§8.3-7）。

### 8.2 MCP 工具 JSON Schema

```json
{
  "tools": [
    {
      "name": "dex_search",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query":    { "type": "string", "description": "全文检索词" },
          "scope":    { "type": "array", "items": { "type": "string" },
                        "description": "申请的 scope 列表；须全部 ⊆ 客户端白名单（任一越权整单拒绝，§8.2）；缺省 = 白名单全集" },
          "limit":    { "type": "integer", "default": 10, "maximum": 20 }
        },
        "required": ["query"]
      }
    },
    {
      "name": "dex_read",
      "inputSchema": {
        "type": "object",
        "properties": {
          "path":     { "type": "string", "description": "仓库内相对路径" },
          "section":  { "type": "string", "description": "可选 H2 小节标题" }
        },
        "required": ["path"]
      }
    },
    {
      "name": "dex_propose",
      "inputSchema": {
        "type": "object",
        "properties": {
          "content":    { "type": "string", "maxLength": 4000 },
          "source":     { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]{0,31}$" },
          "kind":       { "type": "string", "enum": ["fact", "preference", "pattern"] },
          "confidence": { "type": "integer", "minimum": 0, "maximum": 100, "default": 50 },
          "evidence":   { "type": "string", "minLength": 1, "maxLength": 2000,
                          "description": "指向 Spoke 侧过程数据的证据引用；必填；≤2000 字符（收割 locator＋摘录合计同限）" }
        },
        "required": ["content", "source", "kind", "evidence"]
      }
    },
    {
      "name": "dex_journal",
      "inputSchema": {
        "type": "object",
        "properties": {
          "text":     { "type": "string", "maxLength": 4000, "description": "供稿摘要正文" },
          "source":   { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]{0,31}$" },
          "date":     { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$", "description": "可选，缺省今日" }
        },
        "required": ["text", "source"]
      }
    }
  ]
}
```

> 服务端行为：客户端身份来自 spawn 参数（`--client` + 凭证，不自报 clientInfo）；**scope 授权为 fail-closed 整单判定**：申请的 scope 列表必须全部 ⊆ `config.toml [clients.<id>].scopes` 白名单，任一越权即整单拒绝 E_SCOPE_DENIED · 3（不静默剔除越权项后继续——调用方必须感知申请被拒；「交集」只发生在授权通过后的目录并集展开，不用于授权判定）；`dex_search` 的 `scope` 参数缺省 = 该客户端白名单全集（省略即按已授权范围检索）；`dex_read` 请求路径解析出的 scope 必须落在该客户端白名单内（journal/inbox 等路径同规则），否则 E_SCOPE_DENIED；source 必须属于该客户端 allowed_sources（FR-4.7）；**不存在任何写 scope 的工具**（P4）。

### 8.3 退出码表

| 码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 语义性无结果（search 无命中、read 文件不存在） |
| 2 | 参数/元数据错误 |
| 3 | scope 拒绝（未注册、白名单外） |
| 4 | 提案无证据 |
| 5 | 大小超限 / 限流触发 |
| 6 | 路径非法（穿越、symlink 逃逸、禁区内路径） |
| 7 | 缓存写失败**且操作未完成**（如 reindex 写库失败）；降级成功（search 转 ripgrep 仍返回结果、git 缺失跳过自动提交）不算失败——退出码 0，提示进 `--json` warnings（W_INDEX_DEGRADED / W_GIT_UNAVAILABLE，§8.4） |
| 8 | 仓库状态错误（已存在、未初始化） |
| 9 | 提案含疑似密钥（密钥守卫触发；advisory 模式下警告但成功） |
| 10 | 拒绝覆盖（目标已存在且非 dex 产物：render 入口文件无生成标记，FR-6.4；skills install 目标技能目录非本仓条目，FR-11.6） |

### 8.4 `--json` 输出契约与错误枚举（E_\* / W_\*）

所有命令的 `--json` 输出统一信封（稳定 schema，机器可依赖）：

```json
// 成功（含降级成功——退出码恒 0）
{ "ok": true,
  "warnings": [ { "code": "W_INDEX_DEGRADED", "message": "FTS 索引过期，已降级 ripgrep" } ],
  "data": { "…": "命令相关负载（search 命中条目、render 写入路径与 omitted 计数等）" } }

// 失败（退出码见 §8.3）
{ "ok": false,
  "error": { "code": "E_NO_EVIDENCE", "message": "人读错误信息", "details": { "field": "evidence" } } }
```

错误码全集（`E_*`；CLI 退出码 ↔ MCP JSON-RPC `error.data.code` 同一枚举——MCP 无进程退出码，映射即该字段）：

| E_* | 含义 | 退出码 | 触发例 |
|---|---|---|---|
| E_BAD_ARGS | 参数/元数据错误 | 2 | kind 非枚举、confidence 越界、`--format` 值非法 |
| E_BAD_SOURCE | source 标识非法 | 2 | 不匹配 `^[a-z0-9][a-z0-9-]{0,31}$` |
| E_SOURCE_MISMATCH | source ∉ 客户端 allowed_sources | 2 | §5.5-0（FR-4.7） |
| E_SCOPE_DENIED | scope 越权（整单拒绝） | 3 | 未注册客户端、申请 ⊄ 白名单（§8.2） |
| E_NO_EVIDENCE | 提案无证据 | 4 | evidence 空（FR-4.2） |
| E_TOO_LARGE | 大小超限 | 5 | 正文 >4000、evidence >2000（§5.5-2） |
| E_RATE_LIMIT | 限流触发 | 5 | 单 source 日限（FR-4.3） |
| E_NOT_FOUND | 语义性无结果 | 1 | search 无命中、read 文件不存在 |
| E_BAD_PATH | 路径非法 | 6 | `..`、symlink 逃逸、禁区路径 |
| E_CACHE | 缓存写失败且操作未完成 | 7 | reindex 写库失败、busy_timeout 超时（§4.2；降级成功用 W_INDEX_DEGRADED + 0） |
| E_REPO_STATE | 仓库状态错误 | 8 | init 目标已存在、git index.lock 冲突重试超限（§5.5） |
| E_SECRET | 疑似密钥命中 | 9 | 密钥守卫（advisory 模式 → W_SECRET + 0） |
| E_RENDER_REFUSE | render 拒绝覆盖 | 10 | 目标非 dex 产物（FR-6.4） |

警告码（`W_*`，进 `warnings[]`，**永不改变退出码**）：W_INDEX_DEGRADED（索引降级）、W_GIT_UNAVAILABLE（git 缺失、跳过自动提交）、W_SECRET（advisory 密钥命中）、W_OVER_BUDGET（预算截断首条即越界、输出为空，§5.2-4）。

---

## 9. 模块与代码结构（Rust 建议实现）

```text
dex/
├── crates/
│   ├── dex-core/            # 领域层（纯逻辑，无 I/O，首选测试对象）
│   │   ├── scope.rs         #   §5.1 解析：path ⇄ scope 标识、并集展开
│   │   ├── entry.rs         #   §2.2 条目解析（列表项/段落、handwritten 判定）
│   │   ├── inject.rs        #   §5.2 注入管线（合并比较器、预算截断）
│   │   ├── proposal.rs      #   §5.5 校验序、frontmatter 生成/剥离、journal 供稿追加
│   │   └── errors.rs        #   §8.3 错误码
│   ├── dex-store/           # 基础设施层
│   │   ├── fs.rs            #   文件树遍历（ignore crate，尊重 .dex-ignore）
│   │   ├── git.rs           #   log --follow/diff 过滤、mv/commit 包装
│   │   ├── sqlite.rs        #   §4 缓存库（rusqlite + FTS5）
│   │   └── search/          #   ripgrep 后端（grep-searcher）/ fts 后端 / vec 后端
│   ├── dex-cli/             # CLI 前端（clap）：§8.1 全部命令
│   └── dex-mcp/             # MCP stdio 前端（rust-sdk）：§8.2 四工具
├── dex.toml.example         # §2.5 示例配置
├── skills/                  # 配套技能单一源（FR-11.1）：dex-bootstrap/propose/review，非二进制组件
├── skills/connectors/       # 连接器页（FR-12.2）：每来源一页获取知识（六要素），随技能同渠道分发
└── Cargo.toml               # workspace
```

依赖方向（严格单向）：`cli/mcp → core ← store`；`core` 不依赖任何 I/O crate。

---

## 10. 安全与信任边界设计

| 面 | 措施 |
|---|---|
| 身份与凭证 | 每个消费方注册为客户端并持独立凭证（`[clients]`，FR-10.1/10.3）：CLI 与 MCP 同一注册表；非交互调用须 `--client` + 凭证，交互式终端默认 human；未注册/无凭证 = 全拒 |
| 写边界 | 代码层面无写 scope 路径；agent 写入仅两处且都经命令守卫：`propose` → `inbox/`、`journal` → 自有小节（追加，FR-5.4）；`render` 写消费方工作区，不写 Hub |
| 读边界 | CLI 与 MCP 一致按客户端 scopes 过滤（申请 ≠ 授权：申请须全部 ⊆ 白名单，任一越权整单拒绝、不静默剔除，§8.2；`dex_read` 路径解析出的 scope 必须在白名单内）；未注册客户端全拒；`journal/inbox/archive/index/.cache` 恒不在默认注入集；human 全量可读（FR-10.4） |
| 路径安全 | `dex read/propose` 拒绝 `..`、绝对路径、symlink 逃逸出 `DEX_ROOT`、`.git/.cache` 内路径（退出码 6） |
| 提案洪水 | 证据必填 + 4000 字符上限 + 单 source 日限流 + 内容 hash 幂等 + source 与客户端身份绑定（§5.5-0，防伪报绕过限流） |
| 内容风险 | 敏感内容不入库为文档级约束；可选 git-crypt 整仓加密；`propose` 不做语义审查（人是裁决者），但做机械密钥模式扫描（§5.5-8） |
| 内容注入 / 记忆投毒 | render 产物头部统一声明「数据而非指令」＋条目附来源标注；提案经周回顾人审确认门；矛盾显式化（superseded-by）防止错误结论静默扩散；密钥守卫防凭证入库（MINJA / AgentPoison 类威胁的内容层防御） |
| 进程边界 | 无守护进程；MCP stdio 生命周期 = 客户端会话；无网络监听端口（NFR-1/2） |
| 审计 | 一切写动作 = git commit（propose、journal 供稿、归位、否决、归档、改写），`git log` 即完整审计流；越权拒绝写入本机审计日志（`.cache/audit.log`，尽力而为、可随缓存重建丢失） |
| 威胁模型边界 | 凭证防误配置、跨客户端最小授权、为远程网关提供身份载体；**不防同用户恶意进程**（token 本地可读、文件树为明文，后者物理不可防）；scope 白名单约束协议通道，不约束磁盘文件——「公司机不持有生活域」属 clone 内容问题而非授权配置问题（WORK_LIFE §8） |

---

## 11. 分期实现计划

```mermaid
gantt
    dateFormat YYYY-MM-DD
    axisFormat %m月
    section v0 约定先行（零代码）
    建仓与目录约定        :done, v0a, 2026-09-20, 1d
    手写 person+journal   :v0b, after v0a, 1d
    Claude Code @import + Obsidian 打开 :v0c, after v0b, 1d
    section v1 CLI
    dex-core（scope/inject/proposal） :v1a, 2026-10-08, 10d
    ripgrep search/read   :v1b, after v1a, 5d
    propose + render      :v1c, after v1b, 5d
    stale + review + init :v1d, after v1c, 7d
    首批 Spoke 供稿（choose-you） :v1e, after v1d, 7d
    section v2 MCP+索引
    dex-mcp 四工具 + 守卫 :v2a, after v1e, 10d
    sqlite FTS5 + 同步器  :v2b, after v2a, 10d
    sqlite-vec 可选后端   :v2c, after v2b, 7d
    section v3 经营强化
    周回顾 UI             :v3a, after v2c, 14d
    index/ 生成式导览     :v3b, after v3a, 7d
    Spoke 周报汇总协议    :v3c, after v3b, 7d
```

里程碑对齐需求 §6 验收：v1e 完成 = 「≥1 个应用（choose-you）稳定供稿（目标 2 个）」；v2a 完成 = MCP 全链路；v3c 完成 = 周报协议。

---

## 12. 测试策略

| 层 | 内容 |
|---|---|
| 单元（dex-core） | scope 解析边界（未知/穿越/递归）；合并比较器全序性质与字典序（FR-3.2）；预算截断含 omitted 计数、字数口径（Unicode 字符、含语法不含注释）与整条丢弃/首条越界语义（§5.2-4）；提案校验 0–8 每条失败路径（含 source 绑定）；keep-until 到期/未到期/两级作用域判定；journal 小节定位与追加；frontmatter 生成/剥离往返 |
| 集成（临时 git 仓库 fixture） | propose→review→git mv 归位全链路；stale 对「纯 rename/注释类提交不计实质变更」的判定与 keep-until 过期重现；render 产物金样本（快照测试）；harvest 首批 30 截断与暂存区分批；interview/harvest 草稿逐条带 src 溯源；lint 结构不变量（顶层白名单、目录软预算、inbox 滞留 >7 天、悬空 scope 引用）；journal 供稿（并发追加不覆盖、密钥命中拒绝）；连接器页烟测命令抽检与预算超限即停 |
| 通道一致性 | 同一操作经 CLI 与 MCP 断言等价输出（换通道不换语义）；同一客户端经两通道的 scope 过滤与限流一致 |
| 安全用例 | 越权 scope、`..` 路径、symlink 逃逸、无证据提案、限流触发、幂等重放、密钥守卫命中拒绝与 advisory 模式、未注册/无凭证客户端全拒、source 与客户端不匹配（E_SOURCE_MISMATCH）、superseded-by 条目不注入、render 产物含「数据非指令」声明 |
| 混沌 | 删除/截断 `.cache/index.sqlite3` → search 降级成功（0 + W_INDEX_DEGRADED）且下次命令完成进程内重建；两进程并发 reindex → busy_timeout 串行化或超时降级（§4.2）；git shallow 环境走全量索引路径；脏工作区（编辑未 commit）下 search/render 命中新内容（FR-8.3 调和路径）；render 目标已存在非 dex 产物 → 拒绝覆盖（退出码 10）；git 不可用时 propose 落盘成功 + W_GIT_UNAVAILABLE |
| 容量 | 10⁴ 条目合成仓库：v1 search P95 ≤1s、v2 FTS P95 ≤50ms、render P95 ≤1s（NFR-3/10） |

---

## 13. 设计风险与取舍记录

| 决策 | 取舍 | 备选与回退 |
|---|---|---|
| 无自有数据库，git 为审计源 | 查询能力受限（无跨文件事务） | `.cache` SQLite 承担查询；可随时重建 |
| 使用计数不写回 Hub | 衰减信号弱，依赖人审 | v3 Spoke 周报协议补强；接受误差（提案既定取舍） |
| frontmatter 仅限 inbox | scope 文件无元数据可用 | 优先级以「src 注释存在性 + git 时间」近似推断，精度换维护成本 |
| MCP 仅四工具（含 dex_journal） | 富客户端功能少（无 list/导航） | 刻意收缩攻击面与语义面；导航靠 `dex render` 与 `index/`；journal 写入不另开直写面 |
| FTS 默认 unicode61 分词 | 中文按字索引，短语查询可命中但召回一般 | v2 后期可换 jieba 类 tokenizer 或启用向量后端 |
| 单人单仓库 | 不支持家庭/团队共享 | 明确非目标（需求 §1.3）；如未来需要，scope 模型可扩展 `domains/shared-x`，不动核心 |
| 编码 agent 自带记忆（Claude Code auto memory 等）视为 Spoke 过程数据 | 不自动入 Hub，需人工收割（US-12） | 消费方接入文档写明「禁用或并存」二选一；收割走 propose 标准协议，证据指针即其 memory 文件 |
| 冷启动零手写（agent 起草、人裁决） | 草稿有幻觉风险，人审成本前移 | 逐条标源纪律（FR-2.7）+ 首批 ≤30 分批（FR-4.3）；手写路径保留为可选最高主权 |
| 数据获取走连接器页 + agent 工具（CLI-first），不内建 fetcher | 质量与成本控制依赖技能纪律而非代码；CLI 缺失源退化为手动导出 | 三道缰绳兜底（§5.7）；连接器页是 markdown，维护成本一行级；认证由 CLI 自管，dex 零 token 托管 |
| 连接层三形态（page/script/external），不引入 in-process 插件机制 | 无进程内插件的性能与深度集成；无插件市场类分发 | `dex propose`/MCP 即插件接口（协议即扩展点，进程边界即插件边界）；一切写入收敛单一门禁；受控 wasm 留待 v3 后出现明确需求再评估 |
| 衰减「保留」用 keep-until 到期注释持久化（FR-2.10） | 无机制时保留裁决不留痕，僵尸候选每周重现（SRE 告警疲劳模式）；豁免须有到期日防永久沉默 | 模式来源：GTD tickler file、运维告警 snooze-with-expiry、SRE「告警必须可操作」；系本项目组合设计而非记忆社区既有实践，出处已标注 |
| per-client 凭证覆盖 CLI 与 MCP（FR-10 组） | Spoke 接入多一步注册 + token；token 本地存储（0600） | 换来统一语义、跨客户端最小授权、source 绑定与远程网关身份；边界：不防同用户恶意进程（§10 威胁模型） |
| journal 供稿收敛为命令（FR-5.4） | 多一个命令面 | 换来追加原子性、密钥扫描、git 留痕与 source 绑定的一致保证；裸文件写不被承认 |
| render 产物默认落 repo 根 + 拒绝覆写非 dex 产物（FR-6.4）；团队仓库分流（支持范围 = Claude Code/pi/ZCode：@import 或用户级全局文件，TOOL_COMPATIBILITY.md） | 团队仓库不能直接用根位；pi/ZCode 无 import 语法，全局层手动维护 | 备选「合并写入既有 AGENTS.md」被否——个人记忆不得混入团队文件（误 commit 泄漏风险）；备选「默认工作区子目录」被 v1.9 撤销——嵌套入口是子树按需语义，全局记忆放子目录多数工具不会加载；产物带 dex 生成标记，`--out` 可显式落任意路径 |
| `dex harvest` 定位为便利封装（连接器页/预算/暂存/批量提案），不内嵌蒸馏模型（FR-6.14） | 蒸馏质量依赖技能纪律；命令面与技能面职责需文档区分 | 备选「二进制内嵌 LLM 蒸馏」被否——与 NFR-1（不托管模型）、无守护进程及「采集也是适配器」原则冲突；收割技能统一为 `dex-bootstrap`（FR-11.2，US-13 措辞对齐） |
| import 片段的「数据非指令」声明承载在片段首行（宿主层文本防御，FR-6.4/§6.2） | 引用的原文件零复制，其内条目无法逐条强制标注 | 与 merged 头部声明同级（均为上下文文本）；备选「复制正文进宿主」被否——破坏零复制与单一事实源；残留风险接受，周回顾人审确认门仍是第一道防线 |
| 索引重建为「置脏标记 + 下次命令进程内同步执行」（延迟重建），无后台任务（P6） | 触发重建的那次命令慢一次 | 备选后台守护/常驻线程被否（无守护进程原则）；并发写以 BEGIN IMMEDIATE + busy_timeout 串行化、超时降级（§4.2） |
