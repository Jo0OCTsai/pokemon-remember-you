# connector: lark

> 飞书云文档 + 会议纪要（2026-09-30 侦察建页；lark-cli v1.0.95，6 只读 scope：`search:docs:read` `docx:document:readonly` `minutes:minutes.search:read` `minutes:minutes.basic:read` `calendar:calendar.event:read` `vc:note:read`）。

## 1. 数据清单

| 资源类型 | CLI 命令模板 | 输出字段 |
|---|---|---|
| 文档搜索 | `lark-cli docs +search --query <kw> --page-size 30 --format json` | `data.results[]`：`result_meta.{token, doc_types, owner_name, update_time_iso, url}` + `title_highlighted`（含 `<h>` 高亮标记，入库前剥掉）；`data.{total, has_more, page_token}` |
| 文档正文（新版 docx） | `lark-cli docs +fetch --doc <token> --doc-format markdown --scope full` | `data.document.{content, document_id, revision_id}`；content 为 markdown |
| 文档大纲（先看结构再定范围） | `lark-cli docs +fetch --doc <token> --doc-format markdown --scope outline` | 同上，content 仅含标题层级 |
| 关键词截段（evidence 摘录用） | `lark-cli docs +fetch --doc <token> --doc-format markdown --scope keyword --keyword <词>` | 同上，content 为命中块及邻块 |
| 会议纪要文档 | 同「文档搜索」，`--query "会议 \|纪要 \|复盘 \|周会"` 圈定 | 同上 |

注意：分页用 `--page-token <token>` + `has_more`（page-size 上限 30）；`docs +fetch` **仅支持新版 docx**——旧版 `doccn*` token 报错 3380002（`Unsupported document type 'doc'`），BITABLE / SHEET / SLIDES 不可 fetch（多维表格读取需 base 域 scope，未授权）；缺 scope 时错误里 `hint` 给出补授权命令（`lark-cli auth login --scope … --no-wait --json` → 用户扫码 → `--device-code` 续接）。

**本账号数据分布（侦察事实，2026-09-30）**：云文档有数据（「会议」46 条命中）；**妙记（minutes）与日历会议（calendar events）全空**（2025–2026 全年 0 条）——会议纪要实际以**云文档形态**存在（纪要文档 + 「会议纪要 AI 拆解」BITABLE 汇总表）。妙记/日历链路命令留档备查，当前无数据可拉：`minutes +search --participant-ids me --start <date>`、`calendar +search-event --start --end` → `calendar +meeting`（拿 note_id）→ `note +transcript`。

## 2. locator 格式

`lark:<doc_token>`（如 `lark:SajFdSPhyoEd8NxgchFcZk66nnc`）——token 是文档稳定唯一 ID（可定位性已验证）；人点回源用 `result_meta.url`（`https://my.feishu.cn/docs/<token>`）。指到小节：`lark:<doc_token>#<H2标题原文>`（约定式锚点，非 API 块 ID；块级锚在 fetch `--detail with-ids` 下有 block id，仅在需要更细粒度时用）。

## 3. 摘录抓取

蒸馏时对候选文档跑 `docs +fetch --scope keyword --keyword <结论关键词> --doc-format markdown`，从 content 取命中块原文逐字摘录，≤500 字符（超出取含结论句的前 500 字符＋「…」）。双件套拼为 `lark:<token> | 摘录`，evidence 总长 ≤2000 字符（FR-4.1）；回放默认用片段，不依赖源在线。

## 4. 高发区提示

- 项目复盘 / 计划类文档（搜索词「复盘」「计划」「项目」）→ 决策与承诺（domains/work/）
- 会议纪要文档（AI 拆解表与纪要正文）→ 会议决策、我认领的行动项（domains/work/）
- 文档结构与措辞习惯（我写的文档 vs 他人写的，看 owner_name）→ 写作偏好（person/preferences.md）
- 反复出现的整理框架（如固定章节的复盘模板）→ 模式（person/patterns.md）

## 5. 隐私红线特化

- 团队文档（owner 非我、或含团队规范 / 待办 / 排期）：团队结论留 repo 不入 Hub——只收「我」的个人结论与承诺（REQUIREMENTS §3.7 分流判据）
- 他人署名的原话 / 观点一律不收；文档内出现的客户信息、密钥、内部数据永不入库
- 事故定级类文档（如「线上事故定级与复盘」）只收「我」从中得到的个人教训，不收事故细节本身

## 6. 烟测命令

`lark-cli doctor` 全 pass 且 `lark-cli docs +search --query "会议" --page-size 1 --jq '.data.total'` 返回 >0 ⇒ 通过；`missing_scope` ⇒ 先按 hint 补授权再收割；401 / identity 未就绪 ⇒ `lark-cli auth login`（不带 --scope 续登）后重试。
