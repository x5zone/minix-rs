#!/usr/bin/env python3
"""summary-anchor-check.py — 章摘要锚点校验（V3.13 / §4.1）

规则源：`PROMPT-final-workflow-v2.md` Phase 1.3、`review-chapter-contract.md` §4.1。
摘要每条带原文锚点；本脚本只做机械部分：行号在切片内、节号标题存在、锚点数量 ≥3。
完整性与忠实性由 1 个 agent 按 `prompt/review-panel-chapters/summary-verifier.md` 判。

用法：
  python3 tools/summary-anchor-check.py <summary.md> <slice.md>
  python3 tools/summary-anchor-check.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

LINE_RE = re.compile(r"第\s*(\d+)(?:\s*[–—-]\s*(\d+))?\s*行")
SEC_RE = re.compile(r"§\s*(\d+(?:\.\d+)*)")


def heading_lines(slice_text: str) -> set[str]:
    out: set[str] = set()
    for line in slice_text.splitlines():
        m = re.match(r"^#{2,6}\s+(\d+(?:\.\d+)*)[.、]?\s", line)
        if m:
            out.add(m.group(1))
    return out


def check(summary: Path, slice_path: Path) -> list[str]:
    errs: list[str] = []
    st = summary.read_text(encoding="utf-8", errors="replace")
    sl = slice_path.read_text(encoding="utf-8", errors="replace")
    n = len(sl.splitlines())
    heads = heading_lines(sl)
    ranges = []
    for i, line in enumerate(st.splitlines(), 1):
        for m in LINE_RE.finditer(line):
            a = int(m.group(1)); b = int(m.group(2) or a)
            ranges.append((i, a, b, line.strip()))
            if a < 1 or b < a or b > n:
                errs.append(f"摘要第 {i} 行锚点越界：第 {a}–{b} 行（切片 {n} 行）")
        for m in SEC_RE.finditer(line):
            sec = m.group(1)
            if sec not in heads:
                errs.append(f"摘要第 {i} 行节号 §{sec} 在切片中找不到标题")
    if len(ranges) < 3:
        errs.append(f"摘要只有 {len(ranges)} 个行号锚点；至少 3 个")
    return errs


def self_test() -> int:
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        sl = root / "slice.md"
        sl.write_text("# x\n\n## 1. 概述\n\n### 1.1 节\n\na\n\n### 1.2 节\n\nb\n", encoding="utf-8")
        good = root / "good.md"
        good.write_text(
            "- 锚点：§1.1 第 5–7 行\n- 锚点：§1.2 第 9 行\n- 锚点：§1 第 3–4 行\n",
            encoding="utf-8",
        )
        bad_range = root / "bad-range.md"
        bad_range.write_text("- 锚点：§1.1 第 5–99 行\n- 锚点：§1.2 第 9 行\n- 锚点：§1 第 3 行\n", encoding="utf-8")
        bad_sec = root / "bad-sec.md"
        bad_sec.write_text("- 锚点：§9.9 第 5 行\n- 锚点：§1.2 第 9 行\n- 锚点：§1 第 3 行\n", encoding="utf-8")
        few = root / "few.md"
        few.write_text("- 锚点：§1.1 第 5 行\n", encoding="utf-8")
        if check(good, sl):
            print("self-test FAIL: 合法摘要被判红", file=sys.stderr); return 1
        for f, needle in ((bad_range, "越界"), (bad_sec, "找不到标题"), (few, "至少 3")):
            if not any(needle in e for e in check(f, sl)):
                print(f"self-test FAIL: {f.name} 未命中 {needle}", file=sys.stderr); return 1
    print("self-test PASS: 行号越界/节号缺失/锚点不足均被拦截")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="章摘要锚点校验")
    ap.add_argument("summary", nargs="?")
    ap.add_argument("slice", nargs="?")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.summary or not args.slice:
        ap.print_help(); return 2
    s, sl = Path(args.summary), Path(args.slice)
    if not s.is_file() or not sl.is_file():
        print("摘要或切片不存在", file=sys.stderr); return 2
    errs = check(s, sl)
    for e in errs:
        print(f"[FAIL] {e}")
    if errs:
        print(f"summary-anchor-check: FAIL（{s}）")
        return 1
    print(f"summary-anchor-check: PASS（{s}）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
