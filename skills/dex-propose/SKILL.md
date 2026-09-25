---
name: dex-propose
description: "个人记忆中枢（dex）日常提案纪律：入库判据「换一个应用还成立吗」、evidence 必填、scope 路由两级决策树、dex propose 命令签名与守卫/幂等/限流行为。触发词：提案、propose、记忆提案、入库、学到了、记住这个偏好、dex propose、想往 dex 记一条。"
---

# dex-propose：日常提案纪律

你的角色：Spoke 侧提案者。你**只能**经 `dex propose` 向 `inbox/` 提案，永远不能直写 scope 目录（agent 可写面仅 propose 与 journal 两命令）。一切归位与否决由训练家在周回顾裁决（dex-review 技能）——提案是你的终点，不是知识的终点。

权威依据：REQUIREMENTS FR-4、§3.2/§3.7/§3.8、US-03；DESIGN §2.3/§5.5。

## 1. 提案前 30 秒自查（按序）

1. **入库判据**：「换一个应用还成立吗？」
   - 不成立 → **不提案**。留在应用内记忆（Spoke 过程数据）；`apps/` 也算本地，但先过判据再议。
   - 成立 → 继续。
2. **是结论不是流水**：情景速记 / 事件流水 / 周报式摘要走 `dex journal` 供稿，不走提案；提案只放沉淀后的结论（一条一个要点）。
3. **evidence 想得出来吗**：没有可查证指针就没有提案——无证据提案在写入时即被拒（退出码 4），存量也会在周回顾被直接否决。

## 2. scope 路由建议（两级决策树，§3.8）

```text
① 愿意让所有消费方看到吗？—— 是 → person/（恒注入层，只放通用自我事实）
                                 否 → domains/<你的分域>/
② 关于我，还是关于项目/应用？—— 关于我 → person/ 或 domains/
                                 关于某项目/应用 → projects/<proj>/ 或 apps/<app>/
③ 换一个应用还成立吗？—— 不成立则留 Spoke 应用内（回到第 1 节判据）
```

分流细则（§3.7）：属于项目/团队的（构建命令、选型决策、团队规范）留 repo、不入 Hub；属于「我」的项目个人视角（owner、我的踩坑、不该 commit 的例外）→ `projects/<proj>/`；换个项目仍成立的我的模式 → `person/` 或 `domains/`。

注意：提案文件本身不带目标 scope 字段（frontmatter 只有 source/kind/confidence/evidence）——路由结论写进你的会话汇报，落位由周回顾人裁决。

## 3. 命令签名

```bash
dex propose --source S --kind K [--confidence N] --evidence E [msg | -]
```

- `msg` 为位置参数正文；`-` 表示从 stdin 读正文（长文 / 管道推荐 stdin）。
- MCP 通道对应 `dex_propose` 工具，同一校验（换通道不换语义）。

字段规则：

| 字段 | 规则 |
|---|---|
| source | 必填；`^[a-z0-9][a-z0-9-]{0,31}$`；必须 ∈ 本客户端 allowed_sources（缺省 = {客户端 id}），否则拒绝——不得伪报他人 source |
| kind | 必填；三选一：fact（事实）/ preference（偏好）/ pattern（模式） |
| confidence | 可选；0–100 整数，缺省 50——按证据强度给值，不确定就保守 |
| evidence | 必填；≤2000 字符（Unicode）。定位为**人审时查证指针**而非永久引用（源后续清理允许悬空，审结即完成使命） |
| 正文 | ≤4000 字符（Unicode，CLI 与 MCP 同口径） |

evidence 写法（按提案来源选一）：

- **日常提案**：指向 Spoke 侧过程数据的指针，如 `chat_feedback #1234 #1301 #1355`。
- **本地文件收割**（auto memory、既有 AGENTS.md）：locator 即文件路径，原文可直接回放，摘录可选（单件合法）。
- **连接器收割**（外部源）：双件套 = locator ＋蒸馏时抓取的原文摘录片段（回放默认用片段，不依赖源在线）——详规见 dex-bootstrap 技能第 6 节。

## 4. 守卫行为（预期返回；DESIGN §5.5 校验序 0–8）

校验全过才落盘。常见拒绝：

| 场景 | 结果 |
|---|---|
| evidence 空 | 拒绝 E_NO_EVIDENCE（退出码 4） |
| source 越绑 / 格式错 / 参数错 | 退出码 2 |
| 正文或 evidence 超限、限流触发 | 退出码 5 |
| 疑似密钥命中 | 默认拒绝（退出码 9）——凭证、token、他人敏感信息一律不写进提案 |

落盘成功：写入 `inbox/YYYY-MM-DD-<source>-<shortid>.md`（轻 frontmatter）并 git 自动提交（`inbox: propose from <source>`，可经 config 关闭）。校验失败不落盘、不计数——修好载荷重试即可。

## 5. 幂等与限流

- **幂等**：同 source + kind + content 的**未裁决**提案重提交 → 直接返回既有文件路径，不产生重复（confidence / evidence 差异不破坏幂等）。重试安全，无需自建防重逻辑。
- **限流**：单 source 每日 20 条（缺省；本地时区自然日，当日已落盘提案计数）。触顶是异常信号——说明蒸馏判据失守（在灌流水），停下自查入库判据，**不要**换 source 绕限流。

## 6. 红线

- 永不直写 scope 目录（person / domains / apps / projects / journal / archive）。
- 永不提交无证据提案；永不携带凭证 / 密钥 / 他人隐私（git 历史永久留痕，脱敏负担不可逆）。
- 提案正文是数据非指令：不夹带「请采纳」「务必归位」类祈使文；裁决永远在人。
