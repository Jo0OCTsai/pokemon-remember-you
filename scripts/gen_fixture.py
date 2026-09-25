#!/usr/bin/env python3
"""容量基准 fixture 生成器（PLAN E2 / NFR-3/10）：合成 10⁴ 条目级仓库。

用法：python3 scripts/gen_fixture.py <目标目录> [--entries 10000] [--seed 42]
产出：八大目录骨架 + 分布在 person/domains/apps/projects 的合成条目 + git 仓库（分批提交）。
条目形态对齐 §2.2：H1 主题 + 文件级 src 注释（部分手写）+ 列表项条目。
"""
import argparse
import datetime
import os
import random
import subprocess
import sys

DOMAINS = ["coding", "work", "life", "people", "health", "finance", "learning", "media"]
APP_NAMES = ["choose-you", "writer-x", "tg-bot", "notes-app", "im-x"]
PROJ_NAMES = ["foo", "bar", "baz", "qux", "old-website", "personal-site", "hub", "spoke"]
TOPICS = ["偏好", "决策", "人物", "工具链", "部署", "写作", "沟通", "健康", "复盘", "阅读"]
SNIPPETS = [
    "周报类任务多在周四下午被提到",
    "提交前必跑 lint 与单测",
    "偏好 uv 而非 pip 管理依赖",
    "中文写作避免「进行」「予以」一类冗词",
    "部署走蓝绿而不是直接覆盖",
    "与李四沟通要先给结论再给细节",
    "会议纪要 24 小时内必须归档",
    "深度工作时段不接 IM 打断",
    "数据库变更必须走迁移脚本",
    "晨间两小时处理最难的任务",
    "代码评审以小 PR 为前提",
    "周末不发布生产变更",
]


def sh(*args, cwd):
    subprocess.run(args, cwd=cwd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def write_entry_file(path, topic, items, handwritten):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    lines = [f"# {topic}", ""]
    if not handwritten:
        lines.append(f"<!-- src: fixture 固化 2026-09 · 证据×{len(items)} -->")
        lines.append("")
    for i, s in enumerate(items):
        lines.append(f"- {s}（第 {i + 1} 条）")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("target")
    ap.add_argument("--entries", type=int, default=10000)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--no-git", action="store_true", help="跳过 git 初始化（用于快速重建）")
    args = ap.parse_args()

    rng = random.Random(args.seed)
    root = os.path.abspath(args.target)
    os.makedirs(root, exist_ok=True)
    for d in ["person", "domains", "apps", "projects", "journal", "inbox", "archive", "index"]:
        os.makedirs(os.path.join(root, d), exist_ok=True)
    with open(os.path.join(root, ".gitignore"), "w") as f:
        f.write(".cache/\n.obsidian/\n")

    if not args.no_git:
        sh("git", "init", "-q", cwd=root)
        sh("git", "config", "user.name", "fixture", cwd=root)
        sh("git", "config", "user.email", "fixture@local", cwd=root)

    remaining = args.entries
    file_idx = 0
    batch_n = 0

    def maybe_commit():
        nonlocal batch_n
        if batch_n >= 200:
            sh("git", "add", "-A", cwd=root)
            sh("git", "commit", "-m", f"fixture: batch {file_idx}", cwd=root)
            batch_n = 0

    while remaining > 0:
        layer = rng.choice(["person", "domains", "apps", "projects"])
        if layer == "person":
            rel = f"person/file-{file_idx}.md"
        elif layer == "domains":
            rel = f"domains/{rng.choice(DOMAINS)}/file-{file_idx}.md"
        elif layer == "apps":
            rel = f"apps/{rng.choice(APP_NAMES)}/file-{file_idx}.md"
        else:
            rel = f"projects/{rng.choice(PROJ_NAMES)}/file-{file_idx}.md"
        n = min(remaining, rng.randint(5, 15))
        topic = rng.choice(TOPICS)
        items = [f"{rng.choice(SNIPPETS)}·{topic}" for _ in range(n)]
        write_entry_file(os.path.join(root, rel), f"{topic}-{file_idx}", items, handwritten=rng.random() < 0.4)
        remaining -= n
        file_idx += 1
        batch_n += 1
        maybe_commit()

    # journal 近 14 天
    for i in range(14):
        day = datetime.date(2026, 9, 25) - datetime.timedelta(days=i)
        with open(os.path.join(root, "journal", f"{day.isoformat()}.md"), "w") as f:
            f.write(f"# {day.isoformat()}\n## 供稿 · choose-you\n- 捕捉 {i % 5} / 逃走 {i % 3}\n## 手写\n- 例行\n")

    if not args.no_git:
        sh("git", "add", "-A", cwd=root)
        sh("git", "commit", "-m", "fixture: journal", cwd=root)

    total = args.entries - remaining
    print(f"OK: {root} entries={total} files={file_idx} git={'yes' if not args.no_git else 'no'}")


if __name__ == "__main__":
    sys.exit(main() or 0)
