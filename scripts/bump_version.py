#!/usr/bin/env python3
"""bump_version.py <version> —— 抬高 workspace 版本并同步全部版本引用点。

版本唯一来源 = 根 Cargo.toml 的 [workspace.package].version（三 crate 经 version.workspace 继承）。
同步面：
  1. [workspace.package] version
  2. [workspace.dependencies] 里 dex-core / dex-store 的 path 依赖版本钉（发布形态需要显式 version）
  3. Cargo.lock 三个本包条目（匹配不到时留给 cargo 重建，提示不阻断）

bump 后提交改动并打 tag：git tag "v<version>" && git push origin "v<version>"，
release.yml 随 tag 构建多平台产物并创建 GitHub Release（notes 自动分类 = .github/release.yml）。
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NEXT = sys.argv[1] if len(sys.argv) > 1 else ""
if not re.fullmatch(r"\d+\.\d+\.\d+(-[\w.]+)?", NEXT):
    print('用法: python3 scripts/bump_version.py <version>，如 python3 scripts/bump_version.py 0.2.0', file=sys.stderr)
    sys.exit(1)

# 1+2) Cargo.toml：workspace 版本单源 + path 依赖版本钉
cargo_toml = ROOT / "Cargo.toml"
text = cargo_toml.read_text(encoding="utf-8")
new_text, n_version = re.subn(r'(?m)^version\s*=\s*"[^"]+"', f'version = "{NEXT}"', text, count=1)
if n_version != 1:
    print("Cargo.toml 未匹配到 [workspace.package] version——结构被改坏？", file=sys.stderr)
    sys.exit(1)
# [workspace.package] version 在前、[workspace.dependencies] 的 dex-* 版本钉在后，逐个替换
new_text, n_deps = re.subn(
    r'(?m)^(dex-(?:core|store)\s*=\s*\{[^}]*version\s*=\s*)"[^"]+"', rf'\g<1>"{NEXT}"', new_text
)
cargo_toml.write_text(new_text, encoding="utf-8")
print(f"✔ Cargo.toml（workspace 版本 + {n_deps} 处 path 依赖版本钉）→ {NEXT}")

# 3) Cargo.lock：只改三个本包条目，依赖树不动
lock = ROOT / "Cargo.lock"
content = lock.read_text(encoding="utf-8")
count = 0
for name in ("dex-core", "dex-store", "dex-cli"):
    content, n = re.subn(
        rf'(?m)(\[\[package\]\]\nname = "{name}"\nversion = ")[^"]+', rf"\g<1>{NEXT}", content, count=1
    )
    count += n
lock.write_text(content, encoding="utf-8")
if count == 3:
    print(f"✔ Cargo.lock（3 个本包条目）→ {NEXT}")
else:
    print(f"⚠ Cargo.lock 只匹配到 {count}/3 个本包条目——跑一次 cargo check 会自动同步")
