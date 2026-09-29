#!/usr/bin/env python3
"""质量棘轮守卫（通用版，carrier 模板分发；源流：dev-env-settings 仓库守卫 2026-09-26 批次）。

复杂度与覆盖率阈值只紧不松——对准 AI 屎山（迭代残留 + 过度清理）的机检兜底。

三项校验（出现回退 = 非零退出）：
  ① 复杂度棘轮：python-uv 服务 ruff（C901 复杂度 / PLR0911 返回数 / PLR0912 分支数 / PLR0915 语句数，
     显式 --select，独立于主 lint 配置）+ node-pnpm 服务 eslint（complexity / max-lines-per-function /
     max-lines，需服务目录有 eslint.ratchet.config.js——内容见 apply-dev-env 技能
     templates/stacks/node-pnpm.md 复杂度棘轮节）。当前违规集合与基线 quality-ratchet-baseline.json
     比对：新增条目或计数回涨即红；收缩放行，--prune 回写锁定新地板（只紧不松）
  ② 覆盖率地板：按 stack.json services[] 自动发现声明面（test_cmd 的 --cov-fail-under /
     <svc>/pyproject.toml fail_under / <svc>/vite.config.ts 四阈值）——发现到的声明须彼此一致，
     且 ≥ 基线 coverage_floor（声明下调即红）；上调后 --prune 抬升地板（只抬不降）。
     未发现任何声明且地板 > 0 = 失败（地板悬空）；两者皆无 = 提示可选接入
  ③ 代码卫生棘轮（2026-09-28 pokemon-choose-you 消费侧实践回流，rust 目录经 stack.json 泛化）：
     git 跟踪的源码文件计数 TODO/FIXME（全仓）与 unsafe 块（rust 源码），rust 服务另计
     pub API missing_docs 告警（cargo rustc -W missing_docs，服务目录取自 stack.json；
     无 rust 服务 / cargo 不可用 / 编译失败 = 0 计）——与基线 hygiene 段比对：超基线即红
     （存量债先还或入账），下降后 --prune 收缩（只降不升）。基线无 hygiene 段 = 未启用
     （提示可选接入；接入 = 基线加空 "hygiene": {} 后跑 --bootstrap 建账）；键缺省 0（新增键从严）

落位：<项目根>/.devcontainer/scripts/（读 ../stack.json；python3 标准库，零三方依赖）。

用法（在项目根执行）：
  python3 .devcontainer/scripts/quality_ratchet_check.py                     # 全服务扫描 + 地板校验（需 uv/pnpm 与依赖就位）
  python3 .devcontainer/scripts/quality_ratchet_check.py --service <name>    # 只扫单服务 + 全量地板校验（CI lint job）
  python3 .devcontainer/scripts/quality_ratchet_check.py --selftest          # 比对逻辑沙箱自测（零工具链依赖）
  python3 .devcontainer/scripts/quality_ratchet_check.py --prune [--service] # 无回退时回写基线（只紧不松）
退出码：0=全过；1=有回退、依赖缺失或自测失败。
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

# 落位 <项目根>/.devcontainer/scripts/ → 上两级即项目根（stack.json 在 .devcontainer/ 下，三载体同路径）
ROOT = Path(__file__).resolve().parents[2]
BASELINE_REL = ".devcontainer/scripts/quality-ratchet-baseline.json"
STACK_REL = ".devcontainer/stack.json"

# ① 复杂度扫描注册表：栈 → 扫描命令（相对服务目录执行）。新栈接入复杂度棘轮时在此登记；
# 未登记的栈显式打印跳过说明（不静默假绿）。
COMPLEXITY_SCANS = {
    "python-uv": [
        "uv", "run", "ruff", "check", ".",
        "--select", "C901,PLR0911,PLR0912,PLR0915",
        "--output-format", "json", "--exit-zero", "--no-cache",
    ],
    "node-pnpm": [
        "pnpm", "exec", "eslint", ".", "--config", "eslint.ratchet.config.js", "--format", "json",
    ],
    "rust": [
        "cargo", "clippy", "--all-targets", "--message-format=json", "--",
        "-A", "clippy::all",
        "-W", "clippy::cognitive_complexity",
        "-W", "clippy::too_many_lines",
    ],
}
# rust 扫描的入账规则集（cargo JSON 流里只认这两条；其余 clippy/rustc 诊断归主 lint 门槛）
RUST_RULES = {"clippy::cognitive_complexity", "clippy::too_many_lines"}

# ② 覆盖率声明发现规则（相对服务目录；文件存在才采点，全部不存在 = 该服务未声明门禁）
RE_COV_CMD = re.compile(r"--cov-fail-under=(\d+)")
RE_RUST_FAIL_UNDER = re.compile(r"--fail-under-lines[= ](\d+)")   # cargo-llvm-cov（空格/等号两种形态）
RE_FAIL_UNDER = re.compile(r"fail_under\s*=\s*(\d+)")
RE_VITE_THRESHOLDS = re.compile(
    r"thresholds:\s*\{\s*lines:\s*(\d+),\s*branches:\s*(\d+),\s*functions:\s*(\d+),\s*statements:\s*(\d+)"
)


def read(path: Path) -> str:
    if not path.is_file():
        raise FileNotFoundError(f"{path} 不存在（守卫前提文件缺失）")
    return path.read_text(encoding="utf-8")


def load_baseline() -> dict:
    data = json.loads(read(ROOT / BASELINE_REL))
    for section in ("complexity", "coverage_floor"):
        if section not in data:
            raise KeyError(f"{BASELINE_REL} 缺 '{section}' 段（结构被改坏？）")
    return data


def services_from_stack() -> list[dict]:
    stack = json.loads(read(ROOT / STACK_REL))
    services = stack.get("services")
    if not isinstance(services, list) or not services:
        raise KeyError(f"{STACK_REL} services[] 为空或缺失——先按 apply-dev-env 流程填充")
    return [{"name": s["name"], "dir": s["dir"], "stack": s["stack"], "test_cmd": s.get("test_cmd") or ""}
            for s in services]


# ── ① 复杂度扫描 ─────────────────────────────────────────────────────────────


def parse_rust_ndjson(stdout: str, svc_dir: Path) -> dict[str, int]:
    """cargo --message-format=json 的 NDJSON 流 → 违规计数表（纯函数，供 selftest）。

    只认 RUST_RULES 的 clippy 诊断；cargo registry 等工作区外绝对路径不入账
    （命令行 -W 会连依赖 crate 一起 lint，依赖的复杂度不是本项目的账）。
    """
    counts: dict[str, int] = {}
    for line in stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue  # cargo 夹带的非 JSON 行（进度输出等）
        if msg.get("reason") != "compiler-message":
            continue
        inner = msg.get("message") or {}
        code = ((inner.get("code") or {}).get("code")) or ""
        if code not in RUST_RULES:
            continue
        spans = inner.get("spans") or []
        if not spans:
            continue
        file_path = spans[0].get("file_name") or ""
        p = Path(file_path)
        if p.is_absolute():
            try:
                rel = str(p.relative_to(svc_dir))
            except ValueError:
                continue  # ~/.cargo/registry 等外部路径
        else:
            rel = str(p)
        key = f"{rel}:{code}"
        counts[key] = counts.get(key, 0) + 1
    return counts


def scan_complexity(stack: str, svc_dir: Path) -> tuple[dict[str, int], list[str]]:
    """返回 (违规计数表 {文件:规则: 计数}, 失败/跳过说明清单)。"""
    if stack not in COMPLEXITY_SCANS:
        return {}, [f"SKIP: 栈 '{stack}' 未接入复杂度棘轮（COMPLEXITY_SCANS 登记）"]
    cmd = COMPLEXITY_SCANS[stack]
    if shutil.which(cmd[0]) is None:
        return {}, [f"缺 '{cmd[0]}'（本机未安装）——先安装服务依赖（uv sync / pnpm install / rustup）再跑复杂度扫描"]
    if stack == "node-pnpm" and not (svc_dir / "eslint.ratchet.config.js").is_file():
        return {}, [f"缺 {svc_dir}/eslint.ratchet.config.js——内容见 apply-dev-env 技能 "
                    f"templates/stacks/node-pnpm.md「复杂度棘轮」节"]
    proc = subprocess.run(cmd, cwd=svc_dir, capture_output=True, text=True)
    if stack == "rust":
        # clippy -W 警告形态 exit 0；非 0 = 编译失败——不可靠的扫描不出账。
        # 错误取 stderr 尾部（cargo 的 error/Caused by 在最后，前面是 Compiling 噪声）
        if proc.returncode != 0:
            tail = proc.stderr.strip()[-400:]
            return {}, [f"复杂度扫描命令失败（exit={proc.returncode}；常见原因 = ①系统依赖缺失——-sys crate "
                        f"构建脚本要开发库（Tauri 需 webkit/glib 系，装法见 stacks/rust.md「Tauri 项目注意」）；"
                        f"②依赖未编译——cd {svc_dir} && cargo build 看完整错误）。"
                        f"错误尾部: {tail or '（stderr 空——编译错误在 JSON 流，跑 cargo build 查看）'}"]
        return parse_rust_ndjson(proc.stdout, svc_dir), []
    # ruff --exit-zero 恒 0；eslint 有违规时 exit 1 但 stdout 仍是完整 JSON——两者都按 stdout 解析
    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return {}, [f"复杂度扫描命令失败（exit={proc.returncode}；常见原因 = 服务依赖未装——先 cd {svc_dir} "
                    f"&& uv sync / pnpm install）: {proc.stderr.strip()[:200]}"]

    counts: dict[str, int] = {}
    if stack == "python-uv":
        for item in payload:
            key = f"{item.get('filename') or ''}:{item.get('code') or '?'}"
            counts[key] = counts.get(key, 0) + 1
    else:
        for entry in payload:
            file_path = entry.get("filePath") or ""
            try:
                rel = str(Path(file_path).relative_to(svc_dir))
            except ValueError:
                rel = file_path
            for msg in entry.get("messages", []):
                rule = msg.get("ruleId")
                if not rule:
                    continue  # 解析级错误归主 lint 门槛，棘轮只统计规则违规
                key = f"{rel}:{rule}"
                counts[key] = counts.get(key, 0) + 1
    return counts, []


def compare_complexity(
    service: str, current: dict[str, int], baseline: dict[str, int]
) -> tuple[list[str], list[str]]:
    """current vs baseline：新增/回涨 = 失败；收缩/消失 = 放行并记收缩提示。"""
    failures: list[str] = []
    shrinks: list[str] = []
    for key in sorted(set(current) | set(baseline)):
        cur, base = current.get(key, 0), baseline.get(key, 0)
        if key not in baseline:
            failures.append(f"① {service} 新增复杂度违规 {key}（×{cur}）——修复代码；棘轮不收新账")
        elif cur > base:
            failures.append(f"① {service} 复杂度违规回涨 {key}: {cur} > 基线 {base}")
        elif cur < base:
            shrinks.append(f"① {service} {key}: {base} → {cur}（可 --prune 收缩基线）")
    return failures, shrinks


# ── ② 覆盖率地板（按服务自动发现声明面）──────────────────────────────────────


def discover_coverage_spots(svc: dict) -> list[tuple[str, int]]:
    """stack.json test_cmd + 服务目录内已知配置文件；文件不存在不采点。"""
    spots: list[tuple[str, int]] = []
    for m in RE_COV_CMD.finditer(svc["test_cmd"]):
        spots.append((f"stack.json:{svc['name']}.test_cmd", int(m.group(1))))
    for m in RE_RUST_FAIL_UNDER.finditer(svc["test_cmd"]):
        spots.append((f"stack.json:{svc['name']}.test_cmd", int(m.group(1))))
    svc_dir = ROOT / svc["dir"]
    # 声明标签路径：dir="."（服务在仓库根）时显示为 "pyproject.toml" 等，不带 "./" 前缀
    d = svc["dir"][2:] if svc["dir"].startswith("./") else svc["dir"]
    rel = f"{d}/" if d not in ("", ".") else ""
    pyproject = svc_dir / "pyproject.toml"
    if pyproject.is_file():
        m = RE_FAIL_UNDER.search(pyproject.read_text(encoding="utf-8"))
        if m:
            spots.append((f"{rel}pyproject.toml", int(m.group(1))))
    vite = svc_dir / "vite.config.ts"
    if vite.is_file():
        m = RE_VITE_THRESHOLDS.search(vite.read_text(encoding="utf-8"))
        if m:
            spots.append((f"{rel}vite.config.ts", int(m.group(1))))
            for label, val in zip(("branches", "functions", "statements"), m.groups()[1:]):
                spots.append((f"{rel}vite.config.ts:{label}", int(val)))
    return spots


def check_floor(service: str, spots: list[tuple[str, int]], floor: int) -> tuple[list[str], list[str]]:
    failures: list[str] = []
    notes: list[str] = []
    if not spots:
        if floor > 0:
            failures.append(
                f"② {service} 覆盖率地板 {floor} 悬空——声明面一处都没发现（test_cmd --cov-fail-under / "
                f"pyproject fail_under / vite thresholds 至少声明一处，或 --prune 降回 0 放弃门禁）"
            )
        else:
            notes.append(f"SKIP: {service} 未声明覆盖率门禁（可选接入：pytest-cov / vitest coverage 阈值）")
        return failures, notes
    values = sorted({v for _, v in spots})
    if len(values) > 1:
        detail = " ↔ ".join(f"{rel}={v}" for rel, v in spots)
        failures.append(f"② {service} 覆盖率阈值声明不一致: {detail}——各声明点须对齐同一值")
        return failures, notes
    declared = values[0]
    if declared < floor:
        failures.append(
            f"② {service} 覆盖率阈值 {declared} < 地板 {floor}（下调即红；"
            f"确需下调：项目 debt/决策记录留档后手改 {BASELINE_REL}）"
        )
    return failures, notes


def prune_floor(floor: int, declared: int) -> int:
    """地板只抬不降（回写时调用）。"""
    return max(floor, declared)


# ── ③ 代码卫生：TODO/FIXME 与 unsafe 计数（git 跟踪文件，避开生成物） ─────────

HYGIENE_SOURCE_SUFFIXES = {".ts", ".tsx", ".vue", ".js", ".mjs", ".rs", ".py"}
RE_TODO = re.compile(r"\b(TODO|FIXME)\b")
RE_UNSAFE = re.compile(r"unsafe\s*(?:\{|extern|impl|fn)")


def git_tracked_sources() -> list[Path]:
    """git ls-files 的源码文件（untracked / node_modules / target 等天然排除）。
    .devcontainer/ 基建文件豁免：守卫脚本自身docstring/正则必含 TODO/FIXME 字面量（自指）。"""
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=False
    )
    if out.returncode != 0:
        return []
    return [
        ROOT / line
        for line in out.stdout.splitlines()
        if Path(line).suffix in HYGIENE_SOURCE_SUFFIXES and not line.startswith(".devcontainer/")
    ]


def rust_service_dirs(services: list[dict], root: Path = ROOT) -> list[Path]:
    """stack.json services[] 的 rust 服务目录（missing_docs 的扫描落点；纯函数供 selftest）。"""
    return [root / s["dir"] for s in services if s["stack"] == "rust"]


def count_missing_docs(rust_dirs: list[Path]) -> int:
    """rust 服务 pub API 文档缺失告警数（各 rust 服务目录求和）。crate 不挂 #![warn]（会被
    clippy -D warnings 硬升 error 且 crate 属性优先于命令行 -A），由本检查显式 -W 开启；
    独立 target 目录隔离 fingerprint（首次全量后增量秒级，不污染主编译缓存）。
    无 rust 服务 / cargo 不可用 / 编译失败 = 0 计。"""
    if not rust_dirs or shutil.which("cargo") is None:
        return 0
    total = 0
    for d in rust_dirs:
        out = subprocess.run(
            ["cargo", "rustc", "--lib", "--message-format=short", "--", "-W", "missing_docs"],
            cwd=d,
            capture_output=True,
            text=True,
            check=False,
            env={**os.environ, "CARGO_TARGET_DIR": "target/ratchet"},
        )
        total += sum(
            1
            for line in (out.stdout + out.stderr).splitlines()
            if "missing documentation" in line and "warning" in line
        )
    return total


def scan_hygiene(services: list[dict]) -> dict[str, int]:
    counts = {"todo": 0, "rust_unsafe": 0, "missing_docs": 0}
    for path in git_tracked_sources():
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue  # 二进制 / 编码异常文件不入口径
        counts["todo"] += len(RE_TODO.findall(text))
        if path.suffix == ".rs":
            counts["rust_unsafe"] += len(RE_UNSAFE.findall(text))
    counts["missing_docs"] = count_missing_docs(rust_service_dirs(services))
    return counts


def compare_hygiene(counts: dict[str, int], baseline: dict[str, int]) -> tuple[list[str], list[str]]:
    failures: list[str] = []
    notes: list[str] = []
    for key, actual in sorted(counts.items()):
        allowed = baseline.get(key, 0)
        if actual > allowed:
            failures.append(
                f"③ 代码卫生 {key} = {actual} 超基线 {allowed}——清掉新增；确属存量建账：基线入账后跑 --prune 收缩"
            )
        elif actual < allowed:
            notes.append(f"③ {key} 收缩 {allowed} → {actual}（--prune 回写锁定）")
    return failures, notes


# ── 编排 ─────────────────────────────────────────────────────────────────────


def run_checks(only_service: str | None) -> tuple[list[str], list[str], dict]:
    baseline = load_baseline()
    failures: list[str] = []
    notes: list[str] = []
    scanned: dict[str, dict[str, int]] = {}
    declared_by_service: dict[str, int] = {}
    services = services_from_stack()
    for svc in services:
        if only_service and svc["name"] != only_service:
            continue
        spots = discover_coverage_spots(svc)
        f, n = check_floor(svc["name"], spots, baseline["coverage_floor"].get(svc["name"], 0))
        failures += f
        notes += n
        vals = {v for _, v in spots}
        if len(vals) == 1:
            declared_by_service[svc["name"]] = vals.pop()
        counts, errs = scan_complexity(svc["stack"], ROOT / svc["dir"])
        if errs:
            notes += errs
            if not errs[0].startswith("SKIP"):
                failures += errs
                continue
            continue
        scanned[svc["name"]] = counts
        f, s = compare_complexity(svc["name"], counts, baseline["complexity"].get(svc["name"], {}))
        failures += f
        notes += s
    # ③ 全仓口径（不随 --service 收窄）：CI 每 matrix job 各跑一遍全仓卫生，幂等
    hygiene_baseline = baseline.get("hygiene")
    ctx_hygiene: dict[str, int] | None = None
    if hygiene_baseline is None:
        notes.append("SKIP: ③ 代码卫生未入账（可选接入：基线加空 \"hygiene\": {} 后跑 --bootstrap 建账）")
    else:
        hygiene_counts = scan_hygiene(services)
        f, n = compare_hygiene(hygiene_counts, hygiene_baseline)
        failures += f
        notes += n
        ctx_hygiene = hygiene_counts
    return failures, notes, {"baseline": baseline, "scanned": scanned, "declared": declared_by_service, "hygiene": ctx_hygiene}


def prune(only_service: str | None) -> int:
    failures, _notes, ctx = run_checks(only_service)
    if failures:
        print("quality ratchet prune 拒绝回写（存在回退，先清零再 prune）:", file=sys.stderr)
        for f in failures:
            print(f"- {f}", file=sys.stderr)
        return 1
    baseline = ctx["baseline"]
    for svc, counts in ctx["scanned"].items():
        baseline["complexity"][svc] = dict(sorted(counts.items()))
    for svc, declared in ctx["declared"].items():
        baseline["coverage_floor"][svc] = prune_floor(baseline["coverage_floor"].get(svc, 0), declared)
    if ctx["hygiene"] is not None and baseline.get("hygiene") is not None:
        for key, actual in ctx["hygiene"].items():
            baseline["hygiene"][key] = min(baseline["hygiene"].get(key, 0), actual)
    path = ROOT / BASELINE_REL
    path.write_text(json.dumps(baseline, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"quality ratchet baseline pruned → {BASELINE_REL}")
    return 0


def bootstrap(only_service: str | None) -> int:
    """首次建账：把当前扫描的复杂度存量写入基线（新栈/新服务接入用）。

    仅当该服务复杂度基线为空时允许——bootstrap 不是 prune 的宽松版，
    重开账 = 塞债通道（确需重建：手改 baseline 并在项目 debt/决策记录留档）。
    覆盖率地板复用 prune 口径（声明值抬升）。③ 卫生段采纳同语义：基线含空 hygiene 段
    （{} = 采纳意图）时物化当前计数，非空 hygiene 段不重开账。tolerate 的失败仅限
    「新增复杂度违规 / 回涨」与空 hygiene 段采纳期的「③ 超基线」（对空基线这是
    建账前的预期输出），其余照常拒绝。
    """
    failures, _notes, ctx = run_checks(only_service)
    baseline = ctx["baseline"]
    hygiene_adopting = baseline.get("hygiene") == {}
    blocked = [s for s in ctx["scanned"] if baseline["complexity"].get(s)]
    real_failures = [
        f for f in failures
        if ("新增复杂度违规" not in f and "回涨" not in f
            and not (hygiene_adopting and f.startswith("③ ")))
    ]
    if blocked or real_failures:
        print("quality ratchet bootstrap 拒绝:", file=sys.stderr)
        for s in blocked:
            print(f"- {s} 已有复杂度基线，重开账 = 塞债通道；确需重建手改 {BASELINE_REL} 并留档", file=sys.stderr)
        for f in real_failures:
            print(f"- {f}", file=sys.stderr)
        return 1
    for svc, counts in ctx["scanned"].items():
        baseline["complexity"][svc] = dict(sorted(counts.items()))
    for svc, declared in ctx["declared"].items():
        baseline["coverage_floor"][svc] = prune_floor(baseline["coverage_floor"].get(svc, 0), declared)
    if hygiene_adopting and ctx["hygiene"] is not None:
        baseline["hygiene"] = dict(sorted(ctx["hygiene"].items()))
    (ROOT / BASELINE_REL).write_text(json.dumps(baseline, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    names = ", ".join(ctx["scanned"]) or "无（检查扫描是否可用）"
    suffix = "；hygiene 段已建账" if hygiene_adopting and ctx["hygiene"] is not None else ""
    print(f"quality ratchet baseline bootstrapped → {BASELINE_REL}（服务: {names}{suffix}）")
    return 0


# ── 自测（TDD 回归：比对/地板纯函数的正负例，零工具链依赖）───────────────────


def run_selftest() -> int:
    failed = 0

    def case(name: str, ok: bool, detail: str = "") -> None:
        nonlocal failed
        print(f"  {'✓' if ok else '✗'} {name}{'' if ok else f'（{detail}）'}")
        if not ok:
            failed += 1

    f, _ = compare_complexity("svc", {"a.py:C901": 1}, {})
    case("新增违规被检出", any("新增" in x for x in f), str(f))
    f, _ = compare_complexity("svc", {"a.py:C901": 3}, {"a.py:C901": 2})
    case("计数回涨被检出", any("回涨" in x for x in f), str(f))
    f, s = compare_complexity("svc", {"a.ts:complexity": 1}, {"a.ts:complexity": 2, "b.ts:complexity": 1})
    case("收缩放行并记两处收缩", not f and len(s) == 2, f"{f} | {s}")
    f, _ = compare_complexity("svc", {"a.py:C901": 2}, {"a.py:C901": 2})
    case("持平通过", not f, str(f))

    f, _ = check_floor("svc", [("s1", 95), ("s2", 50)], 50)
    case("阈值声明不一致被检出", any("不一致" in x for x in f), str(f))
    f, _ = check_floor("svc", [("s1", 50)], 95)
    case("低于地板被检出", any("下调即红" in x for x in f), str(f))
    f, _ = check_floor("svc", [("s1", 96)], 95)
    case("高于地板通过", not f, str(f))
    f, _ = check_floor("svc", [], 95)
    case("地板悬空被检出", any("悬空" in x for x in f), str(f))
    _, n = check_floor("svc", [], 0)
    case("未声明门禁记跳过提示", any("SKIP" in x for x in n), str(n))

    case("prune 地板抬升", prune_floor(95, 97) == 97)
    case("prune 地板不降", prune_floor(96, 90) == 96)

    f, _ = compare_hygiene({"todo": 1, "rust_unsafe": 0}, {"todo": 0, "rust_unsafe": 4})
    case("卫生计数超基线被检出", any("todo" in x for x in f), str(f))
    f, n = compare_hygiene({"todo": 0, "rust_unsafe": 3}, {"todo": 0, "rust_unsafe": 4})
    case("卫生收缩放行并记收缩", not f and any("收缩" in x for x in n), f"{f} | {n}")
    f, _ = compare_hygiene({"todo": 1, "rust_unsafe": 4}, {"rust_unsafe": 4})
    case("卫生键缺省从严（todo 无基线=0，非零即红）", any("todo" in x for x in f), str(f))
    f, _ = compare_hygiene({"todo": 0, "rust_unsafe": 4}, {"rust_unsafe": 4})
    case("卫生缺省键为 0 合规", not f, str(f))

    case("TODO 正则命中且不误伤复数", RE_TODO.findall("// TODO fix / see FIXME / todos") == ["TODO", "FIXME"])
    case("unsafe 正则命中块/impl/fn 且不误伤标识符", RE_UNSAFE.findall("unsafe { } unsafe impl {} unsafe fn x() unsafe_mode") ==
         ["unsafe {", "unsafe impl", "unsafe fn"])
    case("rust 服务目录推导（stack.json 泛化；非 rust 不入）",
         rust_service_dirs([{"dir": ".", "stack": "node-pnpm"}, {"dir": "src-tauri", "stack": "rust"}], root=Path("/repo"))
         == [Path("/repo/src-tauri")])

    m = RE_VITE_THRESHOLDS.search("thresholds: { lines: 90, branches: 90, functions: 90, statements: 90 },")
    case("vite 四阈值解析", bool(m) and len(set(m.groups())) == 1, str(m and m.groups()))
    m = RE_FAIL_UNDER.search("[tool.coverage.report]\nfail_under = 95\n")
    case("pyproject fail_under 解析", bool(m) and m.group(1) == "95")
    m = RE_COV_CMD.findall("uv run pytest --cov-fail-under=95")
    case("test_cmd 阈值解析", m == ["95"], str(m))
    m = RE_RUST_FAIL_UNDER.findall("cargo llvm-cov --fail-under-lines 55 && echo ok")
    case("rust test_cmd fail-under 解析（空格形态）", m == ["55"], str(m))
    m = RE_RUST_FAIL_UNDER.findall("cargo llvm-cov --fail-under-lines=60")
    case("rust test_cmd fail-under 解析（等号形态）", m == ["60"], str(m))

    rust_stream = "\n".join([
        '{"reason":"compiler-message","message":{"code":{"code":"clippy::cognitive_complexity"},"level":"warning","spans":[{"file_name":"src/main.rs"}]}}',
        '{"reason":"compiler-message","message":{"code":{"code":"clippy::cognitive_complexity"},"level":"warning","spans":[{"file_name":"src/main.rs"}]}}',
        '{"reason":"compiler-message","message":{"code":{"code":"clippy::too_many_lines"},"level":"warning","spans":[{"file_name":"src/commands/mod.rs"}]}}',
        '{"reason":"compiler-message","message":{"code":{"code":"clippy::needless_borrow"},"level":"warning","spans":[{"file_name":"src/main.rs"}]}}',
        '{"reason":"build-finished","message":null}',
        'not-a-json-progress-line',
        '{"reason":"compiler-message","message":{"code":{"code":"clippy::cognitive_complexity"},"level":"warning","spans":[{"file_name":"/Users/x/.cargo/registry/src/dep/lib.rs"}]}}',
    ])
    counts = parse_rust_ndjson(rust_stream, Path("/repo/src-tauri"))
    case(
        "rust NDJSON 解析（入账/过滤规则集/非 JSON 行/外部依赖路径）",
        counts == {"src/main.rs:clippy::cognitive_complexity": 2, "src/commands/mod.rs:clippy::too_many_lines": 1},
        str(counts),
    )

    return failed


def main() -> int:
    args = sys.argv[1:]
    only_service = None
    if "--service" in args:
        idx = args.index("--service")
        if idx + 1 >= len(args):
            print("--service 缺服务名", file=sys.stderr)
            return 1
        only_service = args[idx + 1]
        args = args[:idx] + args[idx + 2:]
    mode = args[0] if args else ""

    if mode == "--prune":
        return prune(only_service)

    if mode == "--bootstrap":
        return bootstrap(only_service)

    if mode == "--selftest":
        failed = run_selftest()
        if failed:
            print(f"quality ratchet selftest failed ({failed} cases)", file=sys.stderr)
            return 1
        print("quality ratchet selftest passed")
        return 0

    try:
        failures, notes, _ctx = run_checks(only_service)
    except (FileNotFoundError, KeyError, json.JSONDecodeError) as exc:
        print(f"quality ratchet check failed: {exc}", file=sys.stderr)
        return 1
    if failures:
        print("quality ratchet check failed:", file=sys.stderr)
        for f in failures:
            print(f"- {f}", file=sys.stderr)
        print("→ 修复方向：复杂度/卫生违规改代码（存量债走建账入账）；覆盖率阈值对齐声明面；"
              "收紧后的新地板用 --prune 回写基线（只紧不松）", file=sys.stderr)
        return 1
    for n in notes:
        print(f"  ↘ {n}")
    print("quality ratchet check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
