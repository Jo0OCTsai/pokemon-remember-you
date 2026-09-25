#!/usr/bin/env python3
"""JUnit 导出（PLAN §5）：跑 cargo test --workspace --no-fail-fast，解析 libtest 输出，
生成 docs/reports/raw/junit-cli.xml——测试结果唯一数据源（api/e2e summary 据此解读，不手填）。

用法：python3 scripts/junit_export.py
退出码：0 = 全绿；1 = 有失败或 cargo 非零退出。
"""
import datetime
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from xml.dom import minidom

TEST_LINE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)(?: - (.+))?$")
RESULT_LINE = re.compile(
    r"^test result: (ok|FAILED)\. \d+ passed; (\d+) failed;"
)


def collect(root_dir):
    # cargo 的 Running 行走 stderr、测试输出走 stdout——必须合并交错（stdout+stderr 顺序拼接会丢失套件归属）
    proc = subprocess.run(
        ["cargo", "test", "--workspace", "--no-fail-fast", "--", "--test-threads=4"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, cwd=root_dir,
    )
    suites, current, current_name = [], [], "unknown"
    for line in proc.stdout.splitlines():
        line = line.strip()
        if line.startswith("Running "):
            m2 = re.search(r"Running (?:unittests )?(\S+)?\s*\(", line)
            current_name = (m2.group(1) if m2 and m2.group(1) else line[8:].split(" (")[0])
            continue
        m = TEST_LINE.match(line)
        if m:
            current.append((m.group(1), m.group(2), m.group(3) or ""))
            continue
        if RESULT_LINE.match(line) and current:
            suites.append((current_name, current))
            current, current_name = [], "unknown"
    if current:
        suites.append((current_name, current))
    return suites, proc.returncode


def main():
    root_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = os.path.join(root_dir, "docs", "reports", "raw")
    os.makedirs(out_dir, exist_ok=True)

    suites, rc = collect(root_dir)

    root_el = ET.Element("testsuites", {
        "name": "cargo test --workspace",
        "timestamp": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat(),
    })
    total, failed = 0, 0
    for name, tests in suites:
        suite = ET.SubElement(root_el, "testsuite", {
            "name": name,
            "tests": str(len(tests)),
            "failures": str(sum(1 for _, s, _ in tests if s == "FAILED")),
        })
        for tname, status, msg in tests:
            total += 1
            tc = ET.SubElement(suite, "testcase", {"name": tname, "classname": name})
            if status == "FAILED":
                failed += 1
                ET.SubElement(tc, "failure", {"message": (msg or "failed")[:500]})

    xml = minidom.parseString(ET.tostring(root_el)).toprettyxml(indent="  ")
    out = os.path.join(out_dir, "junit-cli.xml")
    with open(out, "w") as f:
        f.write(xml)
    print(f"suites={len(suites)} tests={total} failed={failed} -> {out}")
    sys.exit(0 if (failed == 0 and rc == 0) else 1)


if __name__ == "__main__":
    main()
