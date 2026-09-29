# dex 实施计划（v0 + v1）

> 项目：pokemon-remember-you（就记得是你）——个人记忆中枢 dex
> 文档版本：v1.1 · 2026-09-25 · 状态：**已实施**（2026-09-25 实施会话完成 v0+v1；验收机械项全过、人工 dogfood 项已标注——报告属一次性过程产物，已随 `docs/reports/` 清理出仓，结论以本行为准。v1.1 增补：A5 `repo-knowledge` 技能，对应需求 v1.17／FR-11.2 四技能）
> 关卡：已过「需求+设计+架构通过」（2026-09-25；需求/设计 v1.14、安全 v1.1 实施前最终评审清偿完毕）
> 背景文档（实施会话必读，本计划按 FR/§ 编号引用、不复制正文）：
> [REQUIREMENTS.md](./REQUIREMENTS.md)（SDD·v1.14）· [DESIGN.md](./DESIGN.md)（AD·v1.14）· [SECURITY.md](./SECURITY.md)（威胁模型·v1.1）· [TOOL_COMPATIBILITY.md](./proposals/TOOL_COMPATIBILITY.md)（工具矩阵·v1.2）· [debt.md](./debt.md)（实施期待定参数）
> 本计划自包含：实施会话不依赖任何会话讨论记忆，冲突时以 REQUIREMENTS/DESIGN 为权威。

---

## 0. 范围

- **本计划覆盖**：v0（配套技能单一源，纯 markdown 零代码）＋ v1（Rust CLI 全命令面）。v1 验收 = REQUIREMENTS §6 v1 行逐项。
- **不在本计划**：v2（`dex mcp` + FTS/sqlite 索引）、v3（周回顾 UI / `index/` 导览 / Spoke 周报协议）——v1 验收后另立增量计划。v2 前置已在 DESIGN 定死（四工具 Schema §8.2、索引同步 §4），届时无需重开需求。
- **外部依赖**：姊妹项目 choose-you（首个 Spoke）的真实供稿联调受其排期影响——本计划用 mock Spoke 脚本完成机械验收，真实联调与 dogfood 项在验收报告中标注「待 choose-you / 待人工」。

## 1. 技术栈与仓库结构（组件树，DESIGN §1.4/§9）

```text
pokemon-remember-you/
├── crates/
│   ├── dex-core/            # 领域层：纯逻辑、无 I/O（TDD 主战场）
│   │   └── src/{scope, entry, inject, proposal, guard, decay, errors}.rs
│   ├── dex-store/           # 基础设施层
│   │   └── src/{fs, git}.rs + src/search/{mod, rg}.rs   # sqlite/fts 留 v2
│   └── dex-cli/             # CLI 前端（clap）
│       └── src/main.rs + src/{config, guard_runtime, audit}.rs + src/cmd/{search,read,propose,journal,render,stale,review,lint,reindex_stub,init,skills,harvest,interview}.rs
├── skills/                  # v0 交付：dex-bootstrap / dex-propose / dex-review + connectors/（DESIGN §5.7 六要素）；v1 增补 repo-knowledge（FR-11.2）
├── scripts/                 # gen_fixture.py（10⁴ 条目合成仓库）等
├── docs/                    # 权威文档 + 本计划 + proposals/（方案与调研归档）
└── Cargo.toml               # workspace
```

- 依赖方向严格单向：`cli → core ← store`；core 不依赖任何 I/O crate——守卫校验序 6/7（inbox 计数与幂等查重）经 core 内 `InboxView` trait 端口声明、store 实现（DESIGN §9 注）。
- 关键依赖：`clap`(derive)、`serde`/`serde_json`、`toml`、`thiserror`、`ignore` + `grep-searcher` + `grep-regex`（内嵌 ripgrep 同源引擎，FR-6.1 口径——**禁止** shell out 到 `rg` 二进制）、`include_dir`（发行物内嵌技能，FR-11.6）；git 一律子进程调用 ≥2.20（不用 libgit2）；dev：`tempfile`、`assert_cmd`、`predicates`。rusqlite/FTS5 不进 v1 依赖树。
- `dex-mcp` crate 本期不建（v2）。

## 2. 契约冻结清单（实施时以这些条文为验收口径，不得自行变通）

| 契约 | 权威源 |
|---|---|
| CLI 命令面 / 退出码 0–10 | DESIGN §8.1 / §8.3 |
| `--json` 信封与 E_\*/W_\* 枚举（lint 特例 ok=true + data.findings） | DESIGN §8.4 |
| 提案守卫校验序 0–8（journal 子集 0/1/5/6/8，journal_per_day=60） | DESIGN §5.5 |
| 配置两层键级深合并 + 键表 + 凭据文件格式 | DESIGN §2.5 |
| 文件契约：目录白名单（含 `.dex/`、系统杂项忽略）/ 条目与注释两级作用域 / inbox frontmatter / journal 小节 | DESIGN §2 / §5.6 |
| scope 解析（person 白名单条件式、恒排除集、显式 archive/journal 放行）| DESIGN §5.1、FR-3.1 |
| 缺省检索集 = 白名单 ∩ 四层 scope 目录（CLI/MCP 同口径） | DESIGN §5.3、FR-6.1 |
| 注入管线：字典序①②③ + ④ `path:line` 终局序、预算整条丢弃、omitted/suppressed 分计、输出净化 | DESIGN §5.2、FR-3.2/3.3/6.16 |
| `last_substantive`：v1 单遍全树 `git log --name-only` 快照（复活即重置、git 缺失回退 mtime + W_GIT_UNAVAILABLE）；v2 才做 diff 级过滤 | DESIGN §5.4、FR-6.5 |
| render：out 以调用时 cwd 所在 git 仓库根为基准、预检/手改检测（基线 `.cache/render/`）、`--force`/`--dry-run`、技能指针一行 | FR-6.4、DESIGN §6.2 |
| TTY human 免凭证（内建缺省客户端）；非交互 `--client` + 凭证 | FR-10.3、DESIGN §8.1 全局注 |
| git 提交信息模板（12 类 REVIEW_ACTION） | DESIGN §2.3 |
| 威胁边界（不防同用户恶意进程等） | SECURITY §4 |

## 3. 任务分解（并行组 · TDD 分诊）

依赖图：`A ‖ B → C → D → E`（A 无代码依赖可与全程序并行；C 在 B 的 trait/类型签名冻结后即可开工）。

### 并行组 A：v0 技能单一源（纯 markdown，无 TDD）

| # | 任务 | 产出 | 验收 |
|---|---|---|---|
| A1 | `dex-bootstrap` 技能（面试 5 问＋收割循环、三道缰绳自限、首批 ≤30、逐条标源、evidence 双件套规则——本地文件收割单件合法） | `skills/dex-bootstrap/SKILL.md` | frontmatter 合规（name/description/触发词）；对照 FR-11.2/US-01/US-13 |
| A2 | `dex-propose` 技能（入库判据「换一个应用还成立吗」、evidence 必填、scope 路由建议） | `skills/dex-propose/SKILL.md` | 同上；FR-11.2 |
| A3 | `dex-review` 技能（七段段序＝FR-6.6 权威序 + 每项建议命令） | `skills/dex-review/SKILL.md` | 段序与 FR-6.6 逐段一致 |
| A4 | 连接器层脚手架（`skills/connectors/README.md` + 六要素空模板 + 「新源五步清单」） | `skills/connectors/*` | 对照 DESIGN §5.7 |
| A5 | `repo-knowledge` 技能（**v1.17 增补**，FR-11.2：仓库侧知识库维护纪律——知识路由〔子目录 AGENTS.md／ARCHITECTURE／ADR／debt／Hub〕、制度化写入时机、防漂移机检、修剪；`dex skills install` 受管面同步扩为四技能 + connectors） | `skills/repo-knowledge/SKILL.md` | frontmatter 合规；对照 FR-11.2／§3.7 分流判据 |

### 并行组 B：dex-core 领域层（TDD——先写失败测试再实现，判据：错了会以同样方式再现）

| # | 任务 | 文件 | 测试要点（red 用例先行） |
|---|---|---|---|
| B1 | scope 解析器 | `scope.rs` | 白名单条件式 person；恒排除集；显式 archive/journal 放行规则；未知 scope / 路径穿越 / 递归归属（FR-3.5）；`.dex-ignore` 排除 |
| B2 | 条目解析 | `entry.rs` | 列表项/段落/H2H3 分组；src 注释两级作用域（条目级优先、文件级紧随 H1、混合文件按条目）；superseded-by / keep-until 识别（含条目级优先于文件级） |
| B3 | 注入管线 | `inject.rs` | 字典序①②③④全序性质（性质测试：任意置换排序幂等、无并列不可比）；预算整条丢弃/首条越界 W_OVER_BUDGET；omitted 与 suppressed 分计；字数口径（Unicode、含语法不含注释）；净化（`<>&` 转义、注释不透传、转义前计数） |
| B4 | 提案规则 + InboxView 端口 | `proposal.rs`/`guard.rs` | 校验序 1–5/7 每条失败路径的纯函数；shortid（SHA-256 前 6 位、碰撞顺延）；幂等比对域（source+kind+content，confidence/evidence 不入）；frontmatter 生成/剥离往返；journal 小节定位与追加（不跨小节覆盖） |
| B5 | 错误/警告枚举 | `errors.rs` | 表驱动断言 E_\*↔退出码↔MCP error.data.code 三向映射完整（对照 §8.4 全集逐项） |
| B6 | 衰减判定 | `decay.rs` | keep-until 未到期豁免/到期强制重列/两级作用域；单遍 log 快照 → 候选集纯函数；周报引用豁免输入 |

### 并行组 C：dex-store 基础设施（集成测试为主，临时 git 仓库 fixture）

| # | 任务 | 文件 | 要点 |
|---|---|---|---|
| C1 | 文件树遍历 | `fs.rs` | `ignore` crate；尊重 `.dex-ignore`；顶层白名单口径供 lint 复用 |
| C2 | git 适配 | `git.rs` | 子进程 ≥2.20；单遍 `log --name-only` 快照（每文件最后触及 commit 日期）；12 类提交模板；`index.lock` 退避 50ms×≤10 → E_REPO_STATE·8；git 缺失探测 → W_GIT_UNAVAILABLE 降级 |
| C3 | ripgrep 检索后端 | `search/rg.rs` | grep-searcher 直扫；`--limit` 封顶 20；输出 `path:line:scope:content` |
| C4 | InboxView/audit 实现 | `inbox.rs`/`audit.rs` | 实现 B4 端口；audit.log 追加写（字段脱敏：剥离换行控制字符、不记内容/token，SECURITY §6）、软上限 1 MiB 保尾 |

### 并行组 D：dex-cli 命令面（端到端集成测试；核心逻辑已全部在 B 层 TDD 覆盖）

| # | 任务 | 要点 |
|---|---|---|
| D1 | 框架与守卫运行时 | clap 装配 §8.1 全命令；`--json` 信封（lint 特例）；`--client`/TTY human 免凭证判定；config 两层键级深合并加载（§2.5 键表逐键）+ `[auth]` 仓库层拒绝 + 凭据文件 0600/长度≥32 校验（W_CREDS_PERMS/全拒）+ W_CONFIG_CHANGED 基线（FR-10.5/10.7） |
| D2 | search / read | 缺省检索集口径；退出码 0/1/2/3；read 的路径安全（`..`/symlink 逃逸 → 6）与 journal/inbox 显式授权面 |
| D3 | propose / journal | 守卫 0–8 编排（audit.log 拒绝事件）；幂等返回既有文件；bootstrap 模式（首批 ≤30、confidence 降序）与 `inbox/staging/<source>/` 暂存管理 |
| D4 | render | 注入管线编排；out 路径基准（cwd 向上探测 `.git`）；预检（无标记 ⇒ 10；有标记偏离基线 ⇒ 10 + `--force`；无基线静默覆盖＋盲区声明进文档）；`--dry-run` 预览；merged/import 双格式（import 片段首行声明注释）；技能指针一行（FR-11.5） |
| D5 | stale / review / lint | stale 清单（建议动作 + keep-until 流转）；review 七段清单（权威段序 FR-6.6）＋`--group` 分组＋inbox 未清空警告；lint 检查集全量（§5.6 四层）＋信封特例 |
| D6 | init / skills | init：骨架＋`git init`＋首次提交、幂等补齐、退出码 8、git 缺失降级；skills install/uninstall：`include_dir` 内嵌物化 → symlink 三工具目录（幂等/死链重建/非本仓条目退出码 10）、`--from` 工作副本；skills 物化目录＝debt 待定参数，实现时定并回填 debt.md |
| D7 | harvest / interview | 便利封装（不蒸馏）：连接器页与 `[harvest].budget` 加载、staging 分批转正（全量走守卫）、`--dry-run`；interview 渐进草稿提案 |

### 并行组 E：横切验收与工程化（依赖 D 完成）

| # | 任务 | 要点 |
|---|---|---|
| E1 | 安全用例集（对照 DESIGN §12 安全行逐条） | 越权/穿越/无证据/限流/幂等重放/密钥命中与 advisory/未注册全拒/source 不匹配/superseded 不注入/净化转义/权限警告/token<32 全拒/吊销后旧 token 全拒/W_CONFIG_CHANGED/audit.log 字段断言 |
| E2 | 容量基准 | `scripts/gen_fixture.py` 合成 10⁴ 条目仓库；v1 search P95≤1s、render P95≤1s（端到端口径含单遍 log 快照与 git status 前置，DESIGN §12）；CI 外本地跑，结果入报告 |
| E3 | 输出一致性 | 同一操作 `--json` 与 text 输出断言等价（v2 才有 MCP 通道一致性） |
| E4 | 机械质量门 | `scripts/check.sh`：`cargo fmt --check` + `cargo clippy -D warnings` + `cargo test`；配 pre-commit 样例；远端 CI workflow（如无远端则记入报告跳过） |
| E5 | 验收报告 | 见 §5 报告约定；mock Spoke 脚本（`scripts/mock_spoke.sh`：经 CLI 供稿 journal + propose）替代 choose-you 完成机械项 |

## 4. TDD 纪律

- B 组全部任务 red → green → refactor；金样本放 `crates/dex-cli/tests/golden/`（render 产物快照，跨运行字节一致——④ 终局序保证）。
- C/D 组集成测试（`assert_cmd` + `tempfile` 临时 git 仓库 fixture）；每个退出码至少一条触发用例。
- A 组（markdown）与文案类跳过 TDD；完成后以客观校验兜底（frontmatter lint、段序比对）。

## 5. 验收与报告约定（本仓沿用 docs/ 结构，替代全局 specs/ 路径）

```text
docs/reports/
  raw/junit-cli.xml          # cargo test JUnit 导出（测试结果唯一数据源）
  api-summary.md             # 命令面接口测试总结（读 raw 解读）
  e2e-summary.md             # 端到端/安全用例/容量基准总结
  acceptance-report.md       # 对照 REQUIREMENTS §6 v1 行逐项：机械项（脚本可证）+ 人工项（真实周回顾 ≤15 分钟、render 产物被真实 agent 消费、冷启动完成线——需训练家 dogfood，报告中标注执行人与日期）
  screenshots/               # 人工验收截图（render 产物在工具中生效等）
```

> 2026-09-30 注：上表为实施期约定。验收产物属一次性过程产物，已清理出仓不再 commit——机械证据链改为 CI 工作区生成（`scripts/junit_export.py` → `docs/reports/raw/junit-cli.xml`，以 artifact 上传）；过程产物「验收后蒸馏再归档」纪律见 REQUIREMENTS v1.18 FR-11.2。

## 6. 实施期已知事项（新会话须知）

- 待定参数单源：`docs/debt.md` 待处理段（skills 物化目录、Windows ACL、ZCode 实测、衰减分档数值）——定值后回填 debt.md。
- 威胁模型边界不可扩权实现：不为「方便调试」放宽守卫（SECURITY §4 明确不防清单之外一律按设计实现）。
- 单遍 git log 快照若在 10⁴ 条目仓库 P95 超标：先实测成本，进程内缓存快照结果（不改契约），仍超标回报而不是改口径。
- v0 窗口 bootstrap 直写 `inbox/bootstrap/` 为既定例外（FR-1.4 注记），v1 命令面统一收敛顶层——lint 对 v0 遗留目录提示迁移。

## 7. 里程碑对齐（DESIGN §11 gantt）

B 完成 → C → D（对应 gantt v1a–v1d + v1f lint/journal/skills/安全机制）→ E + mock 供稿（v1e 替代）→ v1 验收报告 → 交人工 dogfood 项。
