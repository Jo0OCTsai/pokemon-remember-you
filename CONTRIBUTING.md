# 贡献指南

感谢有意向「就记得是你」贡献！这是一个 Rust workspace 的个人记忆中枢：git 管理的 markdown 文件树 + `dex` CLI。

## 快速开始

开发载体为 **macos**（mac 宿主直跑，无容器件；声明单源 `.devcontainer/stack.json` 的 `carrier` 字段）。工具链（rustfmt / clippy / pre-commit 等）经 host-bootstrap `host-setup.sh apply --only dev-tools` 供给。

```bash
git clone https://github.com/Jo0OCTsai/pokemon-remember-you.git
cd pokemon-remember-you
cargo build                # 构建 dex
scripts/check.sh           # 机械质量门（fmt + clippy -D warnings + test）
pre-commit install         # 本地钩子（lint-gate + commitlint + gitleaks）
```

## 日常开发

- `scripts/check.sh [--no-test]` —— 提交前一键质量门（CI 同源执行）。
- `python3 .devcontainer/scripts/quality_ratchet_check.py` —— 质量棘轮：复杂度基线只准收缩、覆盖率地板（85）、代码卫生（TODO / unsafe / missing_docs）。
- 改目录结构 / 命令 / 配置路径后，grep 全仓引用点并同步受影响文档（机器兜底见 AGENTS.md「模块不变量」）。

## 提交与 PR

- 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/)：`feat: ...` / `fix(scope): ...`（本地 commit-msg 钩子 + CI 双校验，脚本 `.github/scripts/commit-lint.sh`）。
- 短生命周期 feature 分支（`feat/xxx` / `fix/xxx` / `build/xxx`）；合并仅 squash（主题 = PR 标题），main 受分支保护：require PR + 线性历史 + status checks（check × ubuntu/macos、gitleaks、deny、coverage、ratchet、commit-lint）。

## 发布流程

```bash
python3 scripts/bump_version.py <version>   # 抬 workspace 版本（Cargo.toml 单源 + Cargo.lock 本包条目）
git add -A && git commit -m "chore(release): v<version>"
git tag "v<version>" && git push origin main "v<version>"
```

push tag 触发 [`.github/workflows/release.yml`](.github/workflows/release.yml)：三目标（macOS aarch64/x86_64 + Linux x86_64）release 构建 → `checksums.txt`（SHA-256，覆盖二进制与内嵌技能物，FR-11.7）→ GitHub Release（notes 按 [`.github/release.yml`](.github/release.yml) 自动分类）。变更日志以 Release notes 承载，不另维护 CHANGELOG.md。

## 架构速览

```
crates/dex-core/    领域层：纯逻辑、无 I/O（entry / scope / guard / inject / decay / proposal）
crates/dex-store/   基础设施层：文件树 / git 适配 / ripgrep 检索 / inbox 与审计
crates/dex-cli/     命令面：DESIGN §8.1 全命令（bin = dex）
skills/             受管技能单一源（dex-bootstrap / dex-propose / dex-review / repo-knowledge）
docs/               权威三文档（REQUIREMENTS / DESIGN / SECURITY）+ debt.md 债务单源
```

- 三层单向依赖 `dex-core ← dex-store ← dex-cli`，禁止反向（DESIGN §9）。
- `E_*` / `W_*` 错误与警告码单源在 `crates/dex-core/src/errors.rs`，CLI 只透传不定义。
- 行为语义与代码或文档冲突时，以 docs/ 三文档为准；特性变更随文档版本号升版回写。

## 安全

漏洞上报与威胁模型见 [SECURITY](/.github/SECURITY.md)（权威源 docs/SECURITY.md）；提交前 pre-commit 跑 gitleaks，CI 全历史扫描兜底。
