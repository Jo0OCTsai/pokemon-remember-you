# dex v0+v1 验收报告（acceptance-report）

> 生成：2026-09-25 · 执行人：ZCode 实施会话（PLAN.md v1.0）· 验收口径：REQUIREMENTS §6 分期验收 + PLAN §2 契约冻结清单
> 机械项 = 脚本/测试可证（证据链：[raw/junit-cli.xml](./raw/junit-cli.xml) · [api-summary.md](./api-summary.md) · [e2e-summary.md](./e2e-summary.md)）；
> 人工项 = 需训练家 dogfood（本报告标注「待人工」，附执行建议）。

## 1. REQUIREMENTS §6 · v0 行（约定先行）

| 验收项 | 状态 | 证据 / 说明 |
|---|---|---|
| git 私仓建立，八大目录就位 | ✅ 机械 | `dex init`（FR-6.9 工具化）：八目录 + .gitignore + git init + 首次提交；测试 `tests/init_skills.rs` 8 例 |
| `person/` 经面试+收割 ≥10 条且带溯源（零手写） | ⏳ 待人工 | 前置全部就绪：`dex-bootstrap` 技能（面试 5 问+收割+标源纪律）+ `dex interview`/`dex harvest` 命令；实际面试需训练家与 agent 会话完成（US-01/US-13 dogfood） |
| 三技能 symlink 安装且 bootstrap 可完成面试+收割 | ✅ 机械（安装面）/ ⏳ 会话面 | `dex skills install`：物化 + symlink 三工具目录（幂等/死链/冲突 10，测试 9 例）；技能内容 frontmatter 合规 + FR-6.6 段序逐段客观校验通过。实际面试轮次待人工 |
| Claude Code 会话引用个人层 | ⏳ 待人工 | `dex render <agent> --format import` 产出 `@<root>/...` 片段（含声明注释与生成标记）；真实会话消费待 dogfood |
| Obsidian 打开同一 vault | ⏳ 待人工 | 数据面为纯 markdown（US-05 直连路径零代码，架构保证）；打开验证属人工 |
| 全程零代码、零安装 | ✅（已升级） | v0 窗口以技能直写 `inbox/bootstrap/` 为既定例外；本次交付同时完成 v1 工具化（lint 对 v0 遗留目录提示迁移，FR-1.4 注记闭环） |

## 2. REQUIREMENTS §6 · v1 行（CLI）

| 验收项 | 状态 | 证据 |
|---|---|---|
| 七命令（search/read/propose/render/stale/review/lint）全部可用且 `--json` 稳定 | ✅ 机械 | 247 例全绿（junit）；信封/退出码/枚举表驱动锁定（§8.3/§8.4）；一致性 5 例证 `--json` ≡ text。另 journal/init/skills/harvest/interview + reindex 存根全部交付 |
| ≥1 个应用（choose-you）稳定供稿 journal 与 inbox（目标 2） | 🟡 部分 | mock Spoke 机械链路全通（journal 追加/提案/幂等/守卫拒绝，e2e-summary §1）；**真实联调待 choose-you 排期**（PLAN §0 既定替代口径） |
| 一次真实周回顾单批 ≤15 分钟、分批直至 inbox 清空 | ⏳ 待人工 | 工具就绪：`dex review` 七段清单（权威段序测试断言）+ 建议命令 + inbox 未清空警告；实际计时属 dogfood |
| render 产物被至少一个 agent 实际消费 | ⏳ 待人工 | 机械等价物已证：金样本字节级一致 + 头部声明/技能指针/净化断言；真实 agent 会话消费待 dogfood（建议：`dex render zcode` 后在 ZCode 会话问「你能看到哪些入口内容」——TOOL_COMPATIBILITY §2.1 烟测） |
| 注入预算与 scope 过滤生效（越权不泄露） | ✅ 机械 | `tests/security.rs` 越权整单拒绝 + 响应零泄漏；inject 管线预算/omitted/suppressed 分计性质测试 20 例 |
| render 产物净化生效（HTML 转义 + 注释不透传，FR-6.16） | ✅ 机械 | `render_sanitization_and_declaration` + 金样本断言（`&lt;script&gt;`、无 `<!-- src:`/`keep-until`） |
| 冷启动完成线（US-13） | ⏳ 待人工 | 完成线四要件（person/ ≥10 全溯源、≥2 项目、render 产物 ≥5 条 ≥1000 字符、首轮回顾分批）均为运行时数据；工具链全部就绪（bootstrap 技能 + harvest 分批 30 + interview 渐进补全） |

## 3. PLAN §2 契约冻结清单核对（14 项）

| 契约 | 落点 | 状态 |
|---|---|---|
| CLI 命令面 / 退出码 0–10 | errors.rs 表驱动 + 每码触发用例 | ✅ |
| `--json` 信封与 E_*/W_*（lint 特例） | output.rs + §8.4 测试；硬/软分流冻结（debt 登记） | ✅ |
| 守卫校验序 0–8（journal 子集 0/1/5/6/8，journal_per_day=60） | guard.rs + 每失败路径用例 + 校验序优先级测试 | ✅ |
| 配置两层键级深合并 + 键表 + 凭据文件 | config.rs + deep_merge 测试 + W_CREDS_PERMS/W_CONFIG_CHANGED 用例 | ✅ |
| 文件契约（目录白名单/两级作用域/inbox frontmatter/journal 小节） | entry.rs 解析 + lint 16 检查项 | ✅ |
| scope 解析（条件式 person/恒排除/显式放行） | scope.rs（含修复：Full+空声明不再 panic） | ✅ |
| 缺省检索集 = 白名单 ∩ 四层（CLI/MCP 同口径） | scope.rs default_search_dirs + search 测试 | ✅ |
| 注入管线（字典序①②③+④终局序/预算整条丢弃/分计/净化） | inject.rs 性质测试 + 金样本字节级 | ✅ |
| last_substantive 单遍快照（复活重置/mtime 回退 W_GIT_UNAVAILABLE） | git.rs 单进程 log + stale/render 共用 + 回退测试 | ✅ |
| render（out 基准/预检/基线/--force/--dry-run/技能指针一行） | render.rs + 13 例（含双 10 拒绝形态） | ✅ |
| TTY human 免凭证；非交互 --client+凭证 | guard_runtime.rs（stdin∧stdout 终端口径）+ 全拒用例 | ✅ |
| git 提交信息模板（12 类） | 命令侧 5 类自动产出（测试断言）；review 建议命令承载其余 | ✅ |
| 威胁边界（不防清单外一律按设计实现） | 未为调试放宽任何守卫（advisory 为配置项非默认） | ✅ |

## 4. 交付物清单

- **代码**：`crates/{dex-core,dex-store,dex-cli}`（14,193 行，247 测试）；单二进制 `dex`
- **v0 技能**：`skills/dex-{bootstrap,propose,review}/SKILL.md` + `skills/connectors/`（README + 六要素模板，381 行）
- **脚本**：`scripts/{check.sh,junit_export.py,gen_fixture.py,bench.py,mock_spoke.sh,checksums.sh}`
- **CI**：`.github/workflows/ci.yml`（双 OS 矩阵 + JUnit 上传；**未推送，首次 push 生效**）
- **报告**：本目录四件套 + `raw/junit-cli.xml`
- **文档同步**：`docs/debt.md` 实施期收口段（物化目录定值、新键/新码登记、lint 分流冻结）

## 5. 遗留事项（人工 / 下期）

1. **待人工 dogfood（v1 验收收尾）**：真实周回顾计时、render 产物真实 agent 消费截图（`screenshots/` 预留）、冷启动完成线数据、Obsidian vault 打开。
2. **待 choose-you**：真实 Spoke 供稿联调（mock 已覆盖机械面）。
3. **v2 前置已定死**（PLAN §0）：四工具 Schema §8.2、索引同步 §4——届时无需重开需求。
4. 实施期新增面（`W_CONFIG_NOTICE`、`[guard].secret_advisory`、lint 分流、init 提交信息）已登记 debt.md，建议下轮文档评审回写 REQUIREMENTS/DESIGN 对应节。
