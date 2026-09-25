# dex v1 接口测试总结（api-summary）

> 生成：2026-09-25 · 实施：PLAN §5 报告约定
> 数据源：[raw/junit-cli.xml](./raw/junit-cli.xml)（`scripts/junit_export.py` 产出——**测试结果唯一数据源，本文只做解读**）
> 门禁：`scripts/check.sh` 全绿（`cargo fmt --check` + `cargo clippy -D warnings` + `cargo test --workspace`）

## 1. 总量

| 指标 | 值 |
|---|---|
| 测试套件 | 15（dex-core 1 · dex-store 5 · dex-cli 9） |
| 测试用例 | **247** |
| 失败 | **0** |
| Rust 代码量 | 14,193 行（含内联测试） |

## 2. 分层明细（读 junit 解读）

### dex-core（领域层，TDD——B 组 red→green）

| 套件 | 例数 | 覆盖 |
|---|---|---|
| `src/lib.rs` | 99 | scope 解析（grammar/递归归属/白名单 fail-closed/person 条件式/缺省检索集）；entry 解析（两级作用域注释/列表项/段落/续行/frontmatter 跳过）；inject 管线（字典序比较器全序性质测试/预算整条丢弃/omitted 与 suppressed 分计/Unicode 字数/净化）；proposal（frontmatter 字节级往返/shortid 碰撞顺延/journal 小节追加不跨节）；guard（校验序 1–8 每条失败路径/幂等域/限流边界 19|20、29|30、59|60/密钥 6 模式/advisory）；decay（分档最长前缀/keep-until 两级豁免与到期强制重列/引用豁免）；errors 表驱动（E_*↔退出码 13 项三向映射完整、W_* 7 项） |

### dex-store（基础设施层，临时 git 仓库 fixture）

| 套件 | 例数 | 覆盖 |
|---|---|---|
| `tests/fs.rs` | 9 | `.dex-ignore` 生效、隐藏项跳过、多前缀合并去重 |
| `tests/git.rs` | 9 | 单遍 `log --name-only` 快照（多日提交/GIT_COMMITTER_DATE 造旧提交）、index.lock 退避与超限 E_REPO_STATE·8、NothingToCommit、身份注入 |
| `tests/search.rs` | 7 | rg 直扫命中/limit/scope 标注/坏正则 E_BAD_ARGS |
| `tests/inbox.rs` | 6 | InboxView 计数/清单/pending、staging 读写与转正移动 |
| `tests/audit.rs` | 6 | 字段脱敏（控制字符剥离）、五字段行格式、>1MiB 真实阈值轮转保尾 |

### dex-cli（命令面，assert_cmd 端到端）

| 套件 | 例数 | 覆盖（每个退出码至少一例） |
|---|---|---|
| `src/main.rs` | 2 | config 键级深合并、human 内建缺省 |
| `tests/search_read.rs` | 14 | search 0/1/2/3 + limit clamp 20 + 缺省集不泄漏；read 0/1/3/6（`..`/绝对/禁区/symlink 逃逸）+ `--section` + journal 显式授权面 |
| `tests/propose_journal.rs` | 16 | propose 守卫 0–8 全失败路径（2/4/5/9）+ 幂等重放 0 + advisory W_SECRET + git 缺失降级 W_GIT_UNAVAILABLE + git log 留痕；journal 建页/小节隔离/同日追加尾部/超限 5/坏日期 2 |
| `tests/render_golden.rs` | 13 | **金样本字节级**（固定提交日期 fixture ↔ `tests/golden/AGENTS.expected.md` 逐字节相等）+ 二次渲染稳定；预检 10（非 dex 产物 `--force` 不可越 / 人手改可越）；out 基准（cwd git 根探测/非 git 警告）；import 双格式；dry-run；净化与注释不透传 |
| `tests/stale_review_lint.rs` | 17 | lint 检查集 16 项正反例 + 信封特例（error→1 且 ok=true；仅 warning→0）；stale mtime 回退 + keep-until 到期复审；review 七段段序断言 + `--group` + inbox 未清空警告 |
| `tests/init_skills.rs` | 17 | init 全新/幂等→8/半成品补齐/git 缺失降级；skills 物化+symlink 幂等/死链重建/冲突→10/未知工具→2/`--from`/uninstall/状态清单 |
| `tests/harvest_interview.rs` | 9 | harvest 未注册→2/staging confidence 降序首批截断/单条拒绝不阻断批次/dry-run；interview 问题集/stdin 批量走守卫 |
| `tests/security.rs` | 18 | E1 安全用例集（见 [e2e-summary.md](./e2e-summary.md)§2） |
| `tests/consistency.rs` | 5 | E3 输出一致性（`--json` ≡ text） |

## 3. 接口契约冻结落点（实现与 DESIGN §8 对齐说明）

- **退出码 0–10**：`crates/dex-core/src/errors.rs` 表驱动为单一源，测试锁定 13 项 E_* 三向映射。
- **`--json` 信封**：`{ok, warnings[], data}` / `{ok:false, error{code,message}}`（serde_json 字典序键——跨运行字节稳定）。lint 特例（ok=true + data.findings）按 §8.4 落地；**硬/软分流冻结**：error 级→退出码 1，仅 warning 级→0（§5.6 分流语义，已登记 debt.md）。
- **render `--format` 特例**：render 的 `--format merged|import` 为产物格式（非输出格式），其机器可读输出走 `--json`——全局「同给冲突 --format 优先」在 render 上因域不同而冻结为此形态。
- **TTY 判定冻结**：stdin ∧ stdout 均终端 = 交互式（SECURITY §2 边界⑤）；测试经 assert_cmd 全走非交互路径（`--client` + DEX_TOKEN）。
- **12 类提交模板**（§2.3）：propose/journal/harvest/init 五类由命令自动产出（`inbox: propose from <source>` 等，git log 断言在测试内）；review 七类为周回顾人执行动作，由 `dex review` 建议命令输出承载。
- **已知实现期决策**（全部登记 debt.md 实施期收口段）：`[guard].secret_advisory` 新键、`W_CONFIG_NOTICE` 新警告码、init 提交信息 `init: dex skeleton` / `init: 补齐骨架`、skills 物化目录 `~/.local/share/dex/skills/`。

## 4. 结论

命令面（§8.1 全 13 命令 + reindex 存根）接口契约按冻结清单实现，247 例全绿；唯一遗留为 v2 项（`dex mcp`、FTS 索引、`render --skills` 适配器）不在本期范围。
