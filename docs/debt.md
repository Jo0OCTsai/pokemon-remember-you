# 债务与待办（活清单）

> 来源：2026-09-24 实施前文档评审（隔离 sdd-reviewer + ad-reviewer + 主会话客观检查）。
> 约定：完成勾选移入「已完成」段；新债务随时追加；High 项已在 v1.7 修订中全部关闭。

## 待处理

### 推演新增（2026-09-24 · 场景推演发现，详见 [SCENARIO_WALKTHROUGH.md](./SCENARIO_WALKTHROUGH.md) §6）

- [ ] **W1（高·文档矛盾，v1.7 引入）**：FR-10.4 把 harvest/interview 列入「仅 human」管理命令，但收割会话发生在 agent 会话内、由 agent 非交互执行 `dex harvest`——按现行规则会被拒。建议：移出仅 human 清单，允许 human 或显式授权的收割客户端执行。
- [ ] **W2**：收割落盘身份未打通——收割提案 source = 连接器源（im-x），落盘客户端与 `[harvest.sources]` → `[clients].allowed_sources` 的映射未定义。建议：每个收割 source 对应一个收割客户端（`[clients."harvest-im-x"].allowed_sources = ["im-x"]`），会话经 `dex harvest --client harvest-im-x` 落盘（与 W1 一并裁决）。
- [ ] **W3**：入口文件默认子目录（FR-6.4，`.zcode/AGENTS.md`）vs 工具事实标准读取位置（repo 根）——需接入兼容矩阵（认子目录 / `--out` 指根 / `@import` shim），归接入文档。
- [ ] **W4**：非交互 CLI 凭证注入路径未细化——token 从本机凭据文件自动取还是必须 `DEX_TOKEN`；建议显式 `--client` + 凭据文件自动解析（0600），`DEX_TOKEN` 覆盖。
- [ ] v0 bootstrap 直写 inbox 无命令守卫（密钥/限流/幂等缺位）——「零代码」既定取舍、v1 收敛，接入文档写明；新机接入 checklist（config + 凭据部署）并入下方「config 多机一致性」。

### Medium（进入实施前建议收敛，或明确记为待定决策）

- [ ] **注入优先级组合次序矛盾**（FR-3.2/§3.3 vs US-05 验收/提案「手写压过一切固化」）：同一场景（person 手写通则 vs projects 固化例外）三处两种相反答案——核心注入语义，直接影响实现与测试断言。
- [ ] **收割暂存区 `.cache/harvest/` 与 FR-8.4/NFR-4 冲突**：待审内容落在「可任意删除」的缓存区，删缓存/换机即丢；需改落点（如 `inbox/staging/`）或修订 FR-8.4 措辞并处理 FR-2.8 滞留判定的张力。
- [ ] **周回顾段数四种口径**（FR-11.2「七段」未定义构成 / §3.4 六段 / FR-6.6 四项 / DESIGN §5.6「第七段」 vs §6.3 第 1 段 vs FR-6.11「首段」）：需给权威段数与段序。
- [ ] **近义预筛 v1 落点**：US-04/FR-6.6（v1）依赖 `index/`（FR-1.5 为 v3）——明确 v1 为运行时计算不落 index/，或提前。
- [ ] **空目录悖论**：US-01/`dex init` 一次建八大目录 vs FR-2.9「不建空目录」+ lint 空目录检查——脚手架产物会被自身 lint 判错；需白名单豁免或延后建目录。
- [ ] **人速记流改道未显式声明**：提案「人速记/剪藏 → inbox」按 FR-4.2 会被系统性否决删除；应显式声明「人速记归 journal 手写小节」。
- [ ] **FR-2.3 期别**：「归位剥 frontmatter」标 v1，但 v0 人工归位（US-01/FR-9.1）已含该动作——期别应改 v0（v1 起 lint 机检兜底）。
- [ ] **退出码 7 成功却非零**（DESIGN §8.3）：已降级、功能完成但 `$?`≠0，脚本误判失败；建议降级成功返回 0 + warning 进 `--json`。
- [ ] **evidence 无长度上限**（§5.5/§8.2）：洪水向量可绕过正文大小限制；补 maxLength（含收割摘录片段）。
- [ ] **E_\* 错误码无枚举表、`--json` 输出 schema 空白**：实施首日即需要；建议 DESIGN 增补 §8.4（错误对象 `{code, message, details}` + 全集 + MCP ↔ 退出码映射）。
- [ ] **scope「求交集」语义边界**（§8.2 vs §6.1）：申请 3 个 scope 其中 1 个越权——全拒还是剔除越权项继续？MCP scope 参数缺省默认值？需选定。
- [ ] **P5「三通道共用注入管线」表述失实**：search 实现是 ripgrep/FTS + limit，无优先级合并与预算截断；修正 P5 或明确所指。
- [ ] **「后台/异步 reindex」表述矛盾 + SQLite 并发写锁策略**（§4.2 vs §7.2；无守护进程下「后台」语义未定义；两 MCP 会话并发 reindex 锁策略）。
- [ ] **format=import 产物缺「数据非指令」声明承载**（§6.2 vs §10）：import 片段引用原文件零复制，防御声明丢失；需承载方案或记入 §13 接受残留风险。
- [ ] **NFR-5「无运行时依赖」与 git 硬依赖矛盾**：git 缺失时 propose 自动提交的失败行为未定义（可挂钩 §2.3 已有开关）。
- [ ] **config 多机一致性**：FR-10.2「随仓库或本机存放」与 §2.5 仅 `~/.config` 一处；客户端注册表/白名单多机如何同步未讨论。

### Low（实施期 backlog）

- [ ] 提案状态图缺 superseded 节点与恢复转移（FR-2.5）；superseded-by 目标被归档/删除后悬空，lint 未覆盖目标存在性。
- [ ] 幂等 hash 输入域未定义（content vs content+source+kind）；shortid 4–6 位同日碰撞处理。
- [ ] `--format text|json` 与全局 `--json` 两套开关关系未说明（FR-6.10 vs §8.1）。
- [ ] `dex render --skills`（FR-11.4）无 CLI 标志对应。
- [ ] `index/` 导览（gantt v3b）无生成命令；FR-9.5 review 分组输出未在 §8.1 覆盖。
- [ ] lint 顶层白名单未枚举 `.git/`、`.obsidian/`、`.gitignore`、`.dex-ignore`。
- [ ] 预算截断「使 chars 超限的那一条」整条丢弃还是截断包含，未定义（影响 render 金样本）。
- [ ] 归位/否决/归档/改写/升降级的 commit message 模板缺失（REVIEW_ACTION 回放依赖）。
- [ ] `.dex-ignore` 为设计新增，需求无对应 FR（正向漂移，回补）。
- [ ] entries 主键 `path:line` 行漂移 → v2 向量 FK 作废，需内容 hash 键或重算策略。
- [ ] §3.x 悬空「P3」引用（需求优先级体系仅 P0–P2）；US-13 验收「~50%」「1–2 个」量词模糊；NFR-3 无量化 P95。
- [ ] 衰减豁免 keep-until 与周报豁免之外，「预期重启」类项目级豁免粒度（文件 vs 条目）实现时定。
- [ ] MCP 并发 propose 的 git index.lock 冲突重试/排队策略（v2）。
- [ ] US-01 验收「零手写（手写可选）」自我消解，措辞改「无需手写即可达成」。

## 已完成（2026-09-24 · v1.7 评审修订）

- [x] High 全部 7 项：journal 供稿矛盾（FR-5.4/6.13）、render 冲突策略（FR-6.4/退出码 10）、MCP 身份凭证（FR-10 组/[clients]）、dex_read scope 过滤（FR-7.3）、索引脏工作区调和（FR-8.3）、last_substantive v1 mtime 近似（FR-6.5/设计 §5.4）、harvest 职责定位（FR-6.14/FR-12.3）。
- [x] 决策落地：keep-until 保留豁免（FR-2.10）、source 强制绑定（FR-4.7）、字数口径统一（FR-3.3/FR-4.3）、周回顾分批语义、per-client budget 配置（[clients].budget）、威胁模型边界声明（设计 §10）、harvest/interview FR 条目补齐（FR-6.14）、「公司机 clone 内容 vs 授权配置」边界说明（WORK_LIFE §5）。
- [x] 客观一致性：校验序 0–8 统一、propose 退出码 9 补齐、4KB→4000 字符统一、[mcp.clients]/[render.*]→[clients] 引用清零、dex-harvest 技能名对齐、[harvest].model 移除。
