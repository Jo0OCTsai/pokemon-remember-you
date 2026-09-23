# 使用场景调研：工作与生活（示例分域用法）

> 项目：pokemon-remember-you（就记得是你）
> 定位：**使用指南层，非工具语义**——「工作 / 生活」是用户级分域约定（P3：目录即 scope、不做本体论）。本文以这一最常见分法为样本做场景调研；换一种分法（按客户三分、不分）同样适用，工具侧零改动。
> 文档版本：v1.0 · 2026-09-23 · 状态：待评审
> 上游文档：[PERSONAL_MEMORY_HUB_PROPOSAL.md](./PERSONAL_MEMORY_HUB_PROPOSAL.md)（方案提案）· [REQUIREMENTS.md](./REQUIREMENTS.md)（需求，§3.8/FR-3.7）· [DESIGN.md](./DESIGN.md)（设计）

---

## 1. 为什么值得分域（用户侧动机，非工具需求）

| 动机 | 说明 |
|---|---|
| 访问控制的天然分界 | 哪些消费方看哪部分记忆，按分域组合 scope 最直观：公司机编码 agent 不该看到生活记忆，家里 IM bot 不需要项目细节 |
| person/ 恒注入的谨慎判据 | `person/` 全消费方默认可见（FR-3.1）⇒ 内容自动暴露给所有 agent ⇒ 只宜放通用自我事实，场景特定的下放分域 |
| 记忆节奏差异 | 工作记忆随项目快节奏（项目退役即归档）；生活记忆慢而稳（人物页、健康模式季度级不变）——衰减分档与回顾分组受益于分域清晰 |

## 2. 分域模式（示例）

```text
person/              ← 恒注入层：愿意让全部消费方看到的通用自我事实
domains/work/        ← 工作：技术栈、团队、会议结论、职业偏好
domains/life/        ← 生活：健康、家庭、爱好（先内容后结构，不预建）
domains/people/      ← 人物页：第一人称，只存「我与某人的关系」
apps/ projects/      ← 天然偏向 work（projects 基本都是）；apps 按应用定
```

可替换分法示例：自由职业者按 `domains/client-a|client-b|personal` 三分；不分域（全放 person + 主题文件）在记忆量小时同样合法。分法变更走 scope 改名协议（设计 §5.6）。

## 3. 分流决策树（语义中立版）

```text
① 愿意让所有消费方看到吗？—— 是 → person/（恒注入层，只放通用自我事实）
                                否 → domains/<你的分域>/
② 关于我，还是关于项目/应用？—— person vs projects/apps（需求 §3.7）
③ 换一个应用还成立吗？—— 不成立则留 Spoke（需求 §3.2）
```

工作/生活示例对照：「我周末爬山」→ ①否 → `domains/life/`；「提交前必跑 lint」→ ①是 ②跨项目成立 → `person/`；「我对某同事的看法」→ ①否，且绝不能进任何工作消费方可见层。

## 4. person/ 收紧判据

只放愿意让**全部**消费方看到的通用自我事实：身份与角色、通用偏好（写作/沟通风格）、硬性禁区、跨场景成立的习惯。判断口径：「这条事实让任何一个接入的 agent 知道都无害且有用吗？」

## 5. 消费方组合与最小授权（示例）

```toml
[render.work-laptop-zcode]           # 示例分法的工作侧：公司机编码 agent
scopes = ["person", "domains/work", "projects/current"]

[mcp.clients."tg-bot"]               # 示例分法的生活侧：IM bot
scopes = ["person", "domains/life", "apps/todo"]
propose = true

[mcp.clients."cloud-writer"]         # 远程/第三方（US-08）：最小授权子集
scopes = ["person", "domains/work"]
propose = false
```

**最小授权原则**（语义中立，覆盖一切分法）：按消费方实际需要授予 scope 子集，默认全拒（FR-10.3）。公司环境的托管 agent「不挂个人库全量」是该原则的实例而非场景特例——多机同步（US-07）之外最易被忽视的暴露面。

## 6. 记忆来源接入地图（按本分法组织）

> 三判据：**结论密度**（单位数据里「活过三个月的关于我的结论」占比）／**过程可留源侧**／**噪音隐私比**。
> 形态三种：**收割**（一次性批量蒸馏）、**digest**（周期模式蒸馏）、**常驻**（双向集成）。优先级：★★★ 高 / ★★ 中 / ★ 低 / ☆ 谨慎。
> 只推荐常驻的是每天用的（编码 agent、IM bot、待办），其余来源冷启动价值大于持续同步价值——避免把 Spoke 生态做成集成地狱。

### 工作（domains/work · projects）

| 来源 | 贡献 | 落点 | 形态 | 优先级 |
|---|---|---|---|---|
| 编码 agent / IDE（Claude Code、ZCode、Cursor…） | 项目个人记忆、跨项目开发模式 | `projects/`、`domains/work` | 常驻（已接） | ★★★ |
| 代码仓库（CLAUDE.md / ADR / docs） | 个人性结论蒸馏（分流判据过滤，团队内容留 repo） | `projects/` | 收割 + 增量 | ★★★ |
| 会议记录/转录（飞书妙记、Granola） | 决策、人物观点、工作约定 | `domains/work`、`domains/people` | digest（周/月） | ★★★ |
| 工作日历 | 节奏与周期模式（「周报多在周四下午」的原生来源） | `person/` 或 `domains/work` | digest（月度模式） | ★★ |
| 工作 IM（Slack/企微/飞书） | 群组性质、协作约定 | `domains/work`、`apps/` | 常驻（choose-you 同构） | ★★ |
| 工作文档（Confluence / Notion 工作区） | 已定结论的蒸馏 | `domains/work` | 收割 + digest | ★★ |
| 待办/项目管理（Jira、Linear、choose-you） | 任务模式、优先级习惯 | `apps/` | 常驻（已接） | ★★ |
| 职业学习（技术阅读、课程、论文） | 技术栈演进、观点 | `domains/work` | digest | ★★ |
| 简历/绩效自评 | 职业事实与成就（写材料时 dex 反哺） | `person/profile` | 收割 | ★ |

### 生活（domains/life · domains/people）

| 来源 | 贡献 | 落点 | 形态 | 优先级 |
|---|---|---|---|---|
| 个人 IM（微信 / Telegram 私聊） | 人物关系、个人事实、约定 | `domains/people`、`domains/life` | 增量为主（历史收割仅 Telegram 可行） | ★★★ |
| 阅读（微信读书 / Kindle / Readwise） | 观点与偏好信号（不搬高亮原文） | `person/preferences`、`domains/life` | 收割 + digest | ★★★ |
| 生活日历 | 纪念日、家庭约定、生活节奏 | `domains/life`、`domains/people` | digest | ★★ |
| 家人朋友（联系人、生日、喜好） | 人物页供给 | `domains/people` | 收割 + 增量 | ★★ |
| 健康/睡眠（Apple Health、手表） | 生理与作息模式（模式不数据） | `person/` 或 `domains/life` | digest（月/季） | ★★ |
| 媒体消费（豆瓣 / Goodreads / Spotify） | taste profile | `person/profile` | digest（季度） | ★ |
| 生活待办/习惯打卡 | 生活模式 | `apps/` | 常驻 | ★ |
| 旅行（计划与回顾） | 偏好与经历 | `domains/life` | 手动增量 | ★ |
| 购物/订阅 | 消费习惯结论（明细不收） | `domains/life` | digest | ☆ |

### 双场景通用（person / 全库）

| 来源 | 贡献 | 落点 | 形态 | 优先级 |
|---|---|---|---|---|
| 存量笔记（Obsidian / Apple Notes） | 冷启动第一矿 | 各 scope | 一次性收割 | ★★★ |
| 全局工具配置（`~/.claude/CLAUDE.md`、`~/.zcode/AGENTS.md`） | 已成型的个人原则与偏好 | `person/`、`domains/work` | 一次性收割 | ★★★ |
| 个人写作（博客、周记、日记） | 写作即记忆——最真实的自我表达 | 各 scope | 收割 + 手动增量 | ★★★ |
| 浏览器（书签、剪藏、save-later） | 反复保存的规律（不收链接本身） | 按内容定 | digest | ★ |

### 反推荐（写明防漂移——「大一统诱惑」在来源维度的防线）

- **凭证/密码类**：任何形态不入库（NFR-1 + 密钥守卫）；
- **财务明细、位置轨迹**：风险收益不成比例，至多「我订阅了 X」级结论；
- **原始日志全量**（浏览历史、shell history、邮件原文）：违反「过程留源侧」；
- **公司机密与源码细节**：repo 自己是事实源；
- **他人原话**：任何来源（IM/会议）只蒸馏第一人称结论，人物页只存「我与某人的关系」。

## 7. 节奏与参数建议（示例值，实现时定）

- **衰减分档**：work 项目层缺省 90 天；life 慢记忆（人物页、健康模式）可放宽至 180 天——机制为 per-scope 覆盖（FR-6.5 / 设计 §5.4），数值是本分法下的示例；
- **回顾分组**：`dex review` 按一级域分组（FR-9.5）；本分法下可先 work 后 life，工作日碎片时间只清 work 段；
- **升降级高频来源**：coding 场景（`projects/foo/` → `domains/work` 或跨场景成立则 `person/`），周回顾重点关照。

## 8. 待定数值

90/180 天衰减窗口、domains 一级软预算 ≤8（FR-2.9，通用约束非本分法特有）、新域准入阈值（≥10 条 × 2 周）——均在实现阶段经真实使用校准。
