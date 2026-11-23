#!/usr/bin/env python3
"""outline-contract-check.py — outline 合同门（V3.13）

规范源：`prompt/review-rules/review-chapter-contract.md` §二。
要求每份 outline 含：核心命题、知识点/节序、不写什么、下游导出接口、上一章回顾。
用法：python3 tools/outline-contract-check.py <outline.md> [--self-test]
"""
from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

REQUIRED = {
    "核心命题": ("核心命题", "核心问题"),
    "不写什么": ("不写什么", "不覆盖", "边界"),
    "下游导出接口": ("下游导出接口", "导出接口", "下游接口"),
    "上一章回顾": ("上一章回顾", "上章回顾", "一页回顾"),
}


def check(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    heads = [ln.strip() for ln in text.splitlines() if ln.startswith("## ")]
    missing: list[str] = []
    for name, keys in REQUIRED.items():
        if not any(any(k in h for k in keys) for h in heads):
            missing.append(name)
    if not any("知识点" in h or "节序" in h for h in heads):
        missing.append("知识点/节序")
    return missing


def run(path: Path) -> int:
    missing = check(path)
    if missing:
        print(f"[FAIL] {path}: 缺 " + "、".join(missing))
        return 1
    print(f"outline-contract-check: PASS（{path}）")
    return 0


def self_test() -> int:
    good = """# Outline

## 核心命题
交接问题的形状。

## 知识点与节序
1. 三份数据
2. 不变量

## 不写什么
不写项目类型与代码。

## 下游导出接口
下一章可引用三份数据。

## 上一章回顾
无。
"""
    bad = "# Outline\n\n## 核心命题\n只有这一节。\n"
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "good.md").write_text(good, encoding="utf-8")
        (root / "bad.md").write_text(bad, encoding="utf-8")
        if run(root / "good.md") != 0:
            return 1
        if run(root / "bad.md") == 0:
            print("self-test FAIL: 缺项 outline 被判绿", file=sys.stderr)
            return 1
    print("self-test PASS: 缺“不写什么/导出接口/上一章回顾”被拦截")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="outline 合同门（V3.13）")
    ap.add_argument("outline", nargs="?")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.outline:
        ap.print_help()
        return 2
    p = Path(args.outline)
    if not p.is_file():
        print(f"outline 不存在：{p}", file=sys.stderr)
        return 2
    return run(p)


if __name__ == "__main__":
    raise SystemExit(main())
