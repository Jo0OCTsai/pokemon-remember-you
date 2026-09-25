#!/usr/bin/env python3
"""容量基准（PLAN E2 / NFR-3）：10⁴ 条目仓库上 v1 search / render P95 ≤ 1s（端到端口径）。

用法：DEX_ROOT=<fixture 仓库> python3 scripts/bench.py <dex 二进制> [--n 20]
输出：JSON 结果（搜索 P95、render P95、环境信息）——结果粘贴进 e2e-summary。
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import time


def p95(samples):
    s = sorted(samples)
    idx = min(len(s) - 1, max(0, round(0.95 * len(s)) - 1))
    return s[idx]


def run_timed(cmd, env, cwd=None):
    t0 = time.monotonic()
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env, cwd=cwd)
    dt = time.monotonic() - t0
    return dt, proc


def run_timed_subdir(cmd, env, cwd):
    return run_timed(cmd, env, cwd)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dex")
    ap.add_argument("--n", type=int, default=20)
    ap.add_argument("--query", default="周报")
    args = ap.parse_args()

    root = os.environ.get("DEX_ROOT")
    if not root or not os.path.isdir(root):
        print("需要 DEX_ROOT 指向已生成的 fixture 仓库（先跑 scripts/gen_fixture.py）", file=sys.stderr)
        sys.exit(2)

    env = dict(os.environ)
    # search 走 bench 客户端（read=full）；render 是管理命令（FR-10.4 仅 human）→ 用 human 凭证。
    # 凭证经 DEX_TOKEN 注入（≥32 字符）；config 须注册两客户端。
    token = env.get("DEX_TOKEN", "")
    if len(token) < 32:
        print("需要 DEX_TOKEN（≥32 字符，human/bench 共用）", file=sys.stderr)
        sys.exit(2)

    # 预热一次（首次含冷启动效应，不进样本）
    run_timed([args.dex, "search", args.query, "--limit", "20", "--json", "--client", "bench"], env)

    search_times = []
    for _ in range(args.n):
        dt, proc = run_timed([args.dex, "search", args.query, "--limit", "20", "--json", "--client", "bench"], env)
        if proc.returncode not in (0, 1):
            print(f"search 失败 rc={proc.returncode}: {proc.stderr[:400]}", file=sys.stderr)
            sys.exit(1)
        search_times.append(dt)

    render_times = []
    out = os.path.join(root, "AGENTS.md")
    bench_env = dict(env)
    bench_env["DEX_TOKEN"] = token
    for i in range(args.n):
        dt, proc = run_timed_subdir(
            [args.dex, "render", "bench", "--out", "AGENTS.md", "--force", "--json", "--client", "human"],
            bench_env, root,
        )
        if proc.returncode != 0:
            print(f"render 失败 rc={proc.returncode}: {proc.stderr[:400]}", file=sys.stderr)
            sys.exit(1)
        render_times.append(dt)
    if os.path.exists(out):
        os.remove(out)

    result = {
        "fixture_root": root,
        "samples": args.n,
        "search_p95_s": round(p95(search_times), 4),
        "search_median_s": round(statistics.median(search_times), 4),
        "render_p95_s": round(p95(render_times), 4),
        "render_median_s": round(statistics.median(render_times), 4),
        "nfr3_targets": {"search_p95_s": 1.0, "render_p95_s": 1.0},
        "pass": p95(search_times) <= 1.0 and p95(render_times) <= 1.0,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    sys.exit(0 if result["pass"] else 1)


if __name__ == "__main__":
    main()
