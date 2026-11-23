#!/usr/bin/env python3
"""chapter-brief-check.py — 章级六视角 brief schema 检查（V3.13）

检查 `prompt/review-panel-chapters/*.md`：
- 每个 brief 有固定输出头（DOC/CHAPTER/HARD-REJECTIONS/VERDICT 或 DOC/CHAPTER/VERDICT）；
- reader-b.md 有“我重建的结构 / 与实际节序的对比 / 本章越界”三节。
"""
from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

HEADER_COMMON = ("DOC", "CHAPTER", "VERDICT")
READER_B_SECTIONS = ("我重建的结构", "与实际节序的对比", "本章越界")
REQUIRED_ROLES = ("beginner-me.md", "g18.md", "editor.md", "navigator.md", "instructional.md", "reader-b.md")
REFERENCE_ROLES = ("reference.md", "efficiency.md")
SIX_VIEW_INPUTS = ("本章切片", "本章契约", "上一章", "scope")
READER_B_INPUTS = SIX_VIEW_INPUTS + ("outline",)


def check_dir(d: Path) -> list[str]:
    errs: list[str] = []
    files = sorted(d.glob("*.md"))
    if not files:
        return [f"{d} 下没有 brief"]
    names = {f.name for f in files}
    for role in REQUIRED_ROLES:
        if role not in names:
            errs.append(f"缺角色 brief：{role}")
    if not any(r in names for r in REFERENCE_ROLES):
        errs.append("缺效率读者 brief（reference.md 或 efficiency.md）")
    for f in files:
        text = f.read_text(encoding="utf-8")
        for token in HEADER_COMMON:
            if token not in text:
                errs.append(f"{f.name} 缺输出头 {token}")
        if f.name == "g18.md" and "HARD-REJECTIONS" not in text:
            errs.append("g18.md 缺 HARD-REJECTIONS")
        if f.name == "beginner-me.md" and "HARD-REJECTIONS" not in text:
            errs.append("beginner-me.md 缺 HARD-REJECTIONS")
        if f.name == "reader-b.md":
            for sec in READER_B_SECTIONS:
                if sec not in text:
                    errs.append(f"reader-b.md 缺节：{sec}")
            for token in READER_B_INPUTS:
                if token not in text:
                    errs.append(f"reader-b.md 缺输入限定：{token}")
        elif f.name in REQUIRED_ROLES or f.name in REFERENCE_ROLES:
            for token in SIX_VIEW_INPUTS:
                if token not in text:
                    errs.append(f"{f.name} 缺输入限定：{token}")
    return errs


def self_test() -> int:
    with tempfile.TemporaryDirectory() as td:
        d = Path(td)
        for name in ("beginner-me.md", "g18.md", "editor.md", "navigator.md", "instructional.md", "reference.md"):
            extra = "HARD-REJECTIONS\n" if name in ("g18.md", "beginner-me.md") else ""
            (d / name).write_text(
                "本章切片 本章契约 上一章 scope\nDOC\nCHAPTER\n" + extra + "VERDICT\n",
                encoding="utf-8",
            )
        (d / "reader-b.md").write_text(
            "本章切片 本章契约 上一章 scope outline\nDOC\nCHAPTER\nVERDICT\n"
            "我重建的结构\n与实际节序的对比\n本章越界\n",
            encoding="utf-8",
        )
        if check_dir(d):
            print("self-test FAIL: 合法 brief 被判红", file=sys.stderr)
            return 1
        (d / "reader-b.md").write_text("DOC\nCHAPTER\nVERDICT\n", encoding="utf-8")
        if not check_dir(d):
            print("self-test FAIL: 缺节 reader-b 被判绿", file=sys.stderr)
            return 1
        (d / "reader-b.md").write_text(
            "本章切片 本章契约 上一章 scope\nDOC\nCHAPTER\nVERDICT\n"
            "我重建的结构\n与实际节序的对比\n本章越界\n",
            encoding="utf-8",
        )
        if not any("reader-b.md 缺输入限定：outline" in e for e in check_dir(d)):
            print("self-test FAIL: reader-b 缺 outline 未被拦截", file=sys.stderr)
            return 1
        (d / "navigator.md").write_text("本章切片 本章契约 上一章\nDOC\nCHAPTER\nVERDICT\n", encoding="utf-8")
        if not any("navigator.md 缺输入限定：scope" in e for e in check_dir(d)):
            print("self-test FAIL: navigator 缺 scope 未被拦截", file=sys.stderr)
            return 1
        (d / "navigator.md").write_text("本章切片 上一章 scope\nDOC\nCHAPTER\nVERDICT\n", encoding="utf-8")
        if not any("navigator.md 缺输入限定：本章契约" in e for e in check_dir(d)):
            print("self-test FAIL: navigator 缺本章契约未被拦截", file=sys.stderr)
            return 1
    print("self-test PASS: 输出头、Reader-B 三节、六视角四件输入与 Reader-B outline")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("dir", nargs="?")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.dir:
        ap.print_help()
        return 2
    d = Path(args.dir)
    if not d.is_dir():
        print(f"目录不存在：{d}", file=sys.stderr)
        return 2
    errs = check_dir(d)
    for e in errs:
        print(f"[FAIL] {e}")
    if errs:
        return 1
    print(f"chapter-brief-check: PASS（{d}）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
