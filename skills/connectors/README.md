# 连接器层：数据获取知识（FR-12）

## 定位

连接器页 = 每个数据源**一页 markdown 获取知识**，存于 `skills/connectors/<source>.md`，与配套技能同渠道分发、可复用（FR-12.2）。数据获取不内建 fetcher 适配器——获取动作由 agent 工具（Bash / Read）按页执行，获取知识承载在页里；外部源一律 **CLI-first**（认证由来源 CLI 自管，dex 不托管任何第三方 token，FR-12.1）。连接器页属**行为知识、可信通道**（用户自可信源安装、随发行物版本化）：「个人化信息在哪」由收割时现场发现、不预写。

本目录内容：

- `_TEMPLATE.md`：六要素空模板，新源接入时复制填写。
- `<source>.md`：各源连接器页（如 `im-x.md`）。

## 连接层三形态（FR-12.6，统一注册于 config `[harvest.sources.<id>]`，type 字段区分）

| 形态 | type | 机制 | 适用 |
|---|---|---|---|
| 连接器页 | `page` | markdown 六要素 + agent 执行 CLI / Bash | 一次性收割（判断密集、异构、人审在环） |
| 收割脚本 | `script` | 确定性脚本：fetch → NDJSON → `inbox/staging/<source>/` 暂存（或管道给 `dex propose`） | 周期 digest（便宜、稳定、可 cron） |
| 外部 Spoke | `external` | 外部程序自持逻辑与状态，直接调 `dex propose` / MCP | 常驻与复杂逻辑 |

三种形态写入一律收敛于 `dex propose` 单一门禁——扩展点的稳定性**靠协议不靠 ABI**（进程边界即插件边界）。

## 收割源是生产者非读者（FR-12.5）

注册 `[harvest.sources.<id>]` 只登记 source 标识与限流参数（type / first_batch），**不授任何 scope 白名单**。配套注册收割客户端实现 source 绑定闭环：

```toml
[clients."harvest-<source>"]   # 收割客户端映射：scopes = []、propose = true、
scopes = []                    # allowed_sources = ["<source>"]——收割会话经
propose = true                 # dex harvest --client harvest-<source> 落盘
allowed_sources = ["<source>"]
```

## 新源接入五步清单（详文见 DESIGN §5.7；有 CLI 的源约 30–60 分钟）

1. **CLI 就绪**：安装并登录来源 CLI（认证由 CLI 自管）→ 用连接器页第 6 要素烟测命令验证活着 → 确认 JSON 输出与分页方式。
2. **数据侦察**：枚举可拉资源类型 → 试拉并记录命令 / 输出 / 量级 → **确认 locator 可定位性**（无稳定 ID 时用「对话名＋时间戳」替代并在连接器页写明）→ 圈定冷启动范围（不拉全量）。
3. **写连接器页**：复制 `_TEMPLATE.md`，按六要素填写（个人化信息收割时现场发现、不预写）。
4. **注册 source**：config `[harvest.sources.<id>]` 登记 type 与 first_batch；限流单源于收割客户端 `[clients].rate_limit`，此处不设第二限流键。
5. **首轮收割＋人审反向验证**：跑一轮收割 → 周回顾时逐条验证 evidence 回放是否点得开、locator 是否指对位置（此步最常暴露 locator 设计缺陷，改连接器页即闭环）→ 收尾在 WORK_LIFE_SCENARIOS 来源地图加一行。

**无 CLI 的源退化路径**（FR-12.1）：第 1–2 步换成「手动导出文件 + 连接器页描述文件格式与存放路径」，其余不变——获取动作从「agent 跑命令」变为「人定期导出」。

## 相关技能

收割会话的执行纪律（三道缰绳、蒸馏分流判据、evidence 双件套、首批 ≤30）见 `skills/dex-bootstrap/SKILL.md`。
