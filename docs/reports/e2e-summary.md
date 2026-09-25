# dex v1 端到端与安全、容量总结（e2e-summary）

> 生成：2026-09-25 · 实施：PLAN §5 / E1–E5
> 数据源：[raw/junit-cli.xml](./raw/junit-cli.xml)（安全与一致性用例并入 `tests/security.rs` 18 例 + `tests/consistency.rs` 5 例）、`scripts/bench.py` 实测输出、mock Spoke 联调记录

## 1. 端到端业务链路（mock Spoke 替代 choose-you，PLAN §0）

`scripts/mock_spoke.sh`（经 CLI 供稿 journal + propose，环境 `/tmp/dexmock`，二进制 `target/release/dex`）：

| 步骤 | 结果 |
|---|---|
| `dex init`（非 TTY human + DEX_TOKEN） | ✅ 八目录骨架 + git init + 首次提交 `init: dex skeleton` |
| journal 供稿 ×2（同日同 source） | ✅ 两行均追加至 `## 供稿 · choose-you` 小节**尾部**，`## 手写` 不受影响，git 提交 `journal: append from choose-you` |
| propose 无证据 | ✅ 拒绝，`E_NO_EVIDENCE` · 退出码 4，inbox 不落盘 |
| propose 正常（confidence 85 + evidence） | ✅ `inbox/2026-09-25-choose-you-f3ecf4.md`，git 提交 `inbox: propose from choose-you` |
| propose 幂等重放（不同 evidence） | ✅ 返回既有文件路径（`idempotent:true`，`committed:false`），inbox 仍 1 个文件 |

**真实联调标注：待 choose-you**（受其排期影响，PLAN §0 既定替代口径）。

## 2. 安全用例集（E1，对照 DESIGN §12 安全行逐条——`tests/security.rs` 18 例全绿）

| DESIGN §12 安全行 | 用例 | 结果 |
|---|---|---|
| 越权 scope（fail-closed 整单拒绝） | `scope_escape_whole_request_denied`：person 已授权 + domains/coding 越权 ⇒ 整单 3，响应零内容泄漏 | ✅ |
| `..` 路径 / 禁区 | `path_traversal_and_symlink_escape_rejected`：`../`、`.git/config` → 6 | ✅ |
| symlink 逃逸 | 同上：inbox 内符号链指向仓库外 → 6 + audit | ✅ |
| 无证据提案 | `propose_no_evidence_rejected`：→ 4，inbox 保持空 | ✅ |
| 限流触发 | `propose_rate_limit_triggers_and_audits`：20 条当日配额满 → 第 21 条 E_RATE_LIMIT·5 + audit | ✅ |
| 幂等重放 | `propose_idempotent_replay_returns_existing`：不同 evidence 重放 → 既有文件、不重复落盘 | ✅ |
| 密钥命中拒绝 + advisory | `secret_guard_reject_then_advisory_pass`：AKIA 样本 → 9；`[guard].secret_advisory=true` → 0 + W_SECRET | ✅ |
| 未注册客户端全拒 | `unregistered_client_denied`：`--client ghost` → 3 + audit(unknown) | ✅ |
| 无凭证全拒 | `no_credentials_denied` / `non_interactive_without_client_denied` | ✅ |
| token<32 全拒 | `short_token_treated_as_no_credential`：短 token 按无凭证处理 → 3 | ✅ |
| 吊销后旧 token 全拒 | `revoked_client_old_token_denied`：删除 `[clients]` 条目后持旧 token → 3 | ✅ |
| source 不匹配（E_SOURCE_MISMATCH） | `source_mismatch_rejected_and_audited`：choose-you 伪报 human → 2 + audit | ✅ |
| superseded 不注入 | `superseded_entry_not_injected`：被标注条目不出现在 render 产物 | ✅ |
| 净化与声明（FR-6.16/T1/T7） | `render_sanitization_and_declaration`：`<script>` → `&lt;script&gt;`；src/keep-until 注释不透传；「数据非指令」头部在场 | ✅ |
| W_CREDS_PERMS | `creds_perms_warning_on_wide_permissions`：凭据文件 0644 → 警告不阻断 | ✅ |
| W_CONFIG_CHANGED | `config_changed_warning_on_repo_layer_mutation`：首次静默建基线 → 变更告警 → 基线更新后不再告警（三段断言） | ✅ |
| 管理命令权限分档（FR-10.4） | `management_commands_denied_for_non_human`：render/review/stale/lint/reindex 对 choose-you → 3 | ✅ |
| audit.log 字段（FR-10.6） | `audit_log_field_shape_and_no_content_leak`：五字段管道行 + UTC 时间戳 + 结果码；**不含 token/提案内容/密钥样本** | ✅ |

（恒定时间比较与 token 熵按 DESIGN §12 归代码审查：token 为「持有即凭证」模型，无比较面——见实现注记。）

## 3. 容量基准（E2，NFR-3/NFR-10：10⁴ 条目，P95 ≤1s）

- fixture：`scripts/gen_fixture.py` 合成 **10,000 条目 / 1,004 条目文件**（person/domains/apps/projects 分布 + 14 天 journal 页），git 分批提交；`--seed 42` 可复现。
- 口径：端到端（进程冷启动 → 检索含遍历；render 含单遍 `git log` 快照 + 注入管线 + 预检）；release 构建；`--n 20` 取 P95。

| 操作 | P95 | 中位 | 目标 | 结果 |
|---|---|---|---|---|
| `dex search "周报" --limit 20` | **0.009 s** | 0.008 s | ≤1 s | ✅（余量 ~110×） |
| `dex render --out AGENTS.md --force`（全量管线） | **0.035 s** | 0.033 s | ≤1 s | ✅（余量 ~28×） |

结论：单遍 git log 快照在 10⁴ 条目仓库成本可忽略（render 端到端 35ms 内），PLAN §6「超标才缓存」的预案无需启用。

## 4. 输出一致性（E3，`tests/consistency.rs` 5 例全绿）

同一操作 `--json` 与 text 断言等价（v2 MCP 通道一致性的 CLI 侧前置）：

- search：json `results[]`（path/line/scope/content）与 text `path:line:scope:content` 行**逐字段相等**，`count` 与行数一致；
- 无命中错误：两模式退出码一致（1），json `ok:false` + `E_NOT_FOUND`；
- propose：json `data.file` 与 text 输出路径一致（幂等重放跨模式验证）；
- lint：`data.findings` 与 text `[error]` 行——check 名与 error 计数一致，信封特例 ok=true 两模式一致；
- stale：`count` 与 text 清单行数一致。

## 5. 工程化（E4）

- `scripts/check.sh`：fmt --check + clippy `-D warnings` + test 全绿（本地实测通过；CI workflow `.github/workflows/ci.yml` 已就位——ubuntu/macos 双矩阵 + JUnit 产物上传，**远端为 GitHub 私仓，本次未推送，首次 push 后生效**）；
- JUnit 导出 `scripts/junit_export.py` → `docs/reports/raw/junit-cli.xml`（15 套件 / 247 例 / 0 失败）；
- pre-commit 样例：`scripts/check.sh --no-test` 可直接挂 git hook（fmt+clippy 快通道）；
- 发行物完整性脚本 `scripts/checksums.sh`（FR-11.7，release 时生成 checksums.txt）。

## 6. 遗留与边界

- choose-you 真实供稿联调：**待 choose-you**（机械项已由 mock Spoke 覆盖）；
- v2 项（`dex mcp`、FTS 索引、`render --skills`、通道一致性 MCP 侧）不在本期；
- Windows ACL 等价检查、ZCode 入口实测：debt.md 既有待定项（Windows 为可选平台；ZCode 实测属接入期烟测）。
