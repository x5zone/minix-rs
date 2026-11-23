#!/usr/bin/env python3
"""extract-chapter-slice.py — 按 `## N.` 提取章切片（V3.13 合并流程）

规范源：`tmp/review-loop/01-stage/PROMPT-merged-chapter-workflow.md` §3。
章级 reviewer 只拿本工具产出的切片 + 章契约 + 上一章摘要，不拿整篇。

用法：
  python3 tools/extract-chapter-slice.py <doc> --chapter 2
  python3 tools/extract-chapter-slice.py <doc> --chapter 2 --out DIR
  python3 tools/extract-chapter-slice.py <doc> --list
  python3 tools/extract-chapter-slice.py --self-test

默认输出：tmp/review-loop/01-stage/chapters/<doc-stem>/ch<N>.md
切片从该章 `## N.` 标题起，到下一个围栏外的 `## ` 标题或文末；围栏内的
`##` 不参与切分。找不到章号、章号重复、围栏未闭合均报错退出。
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

CHAPTER_RE = re.compile(r"^##\s+(\d+)\.(?!\d)")
H2_RE = re.compile(r"^##\s+")
FENCE_OPEN_RE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
FENCE_CLOSE_RE = re.compile(r"^\s*(`{3,}|~{3,})\s*$")


def _fence_open(line: str):
    m = FENCE_OPEN_RE.match(line)
    return (m.group(1)[0], len(m.group(1))) if m else None


def _fence_close(line: str, ch: str, ln: int) -> bool:
    m = FENCE_CLOSE_RE.match(line)
    return bool(m and m.group(1)[0] == ch and len(m.group(1)) >= ln)


def scan(doc: str) -> tuple[list[tuple[int, int, str]], bool]:
    """返回 [(行号, 章号, 标题), ...] 与围栏是否闭合；只认围栏外标题。"""
    lines = doc.splitlines()
    marks: list[tuple[int, int, str]] = []
    ch = None
    ln = 0
    for i, line in enumerate(lines):
        if ch is None:
            op = _fence_open(line)
            if op:
                ch, ln = op
                continue
            m = CHAPTER_RE.match(line)
            if m:
                marks.append((i, int(m.group(1)), line.strip()))
        elif _fence_close(line, ch, ln):
            ch = None
    return marks, ch is None


def extract(doc: str, chapter: int) -> str:
    marks, closed = scan(doc)
    if not closed:
        raise ValueError("围栏代码块未闭合，无法可靠切章")
    if not marks:
        raise ValueError("找不到任何 `## N.` 章")
    nums = [n for _, n, _ in marks]
    dup = sorted({n for n in nums if nums.count(n) > 1})
    if dup:
        raise ValueError("章号重复：" + "、".join(str(n) for n in dup))
    missing = [n for n in range(1, max(nums) + 1) if n not in nums]
    if missing:
        raise ValueError("章号序列不完整，缺：" + "、".join(str(n) for n in missing))
    hit = [m for m in marks if m[1] == chapter]
    if not hit:
        raise ValueError(f"找不到第 {chapter} 章；现有章号：{nums}")
    start = hit[0][0]
    end = len(doc.splitlines())
    lines = doc.splitlines()
    ch = None
    ln = 0
    for i in range(start + 1, len(lines)):
        if ch is None:
            op = _fence_open(lines[i])
            if op:
                ch, ln = op
                continue
            if H2_RE.match(lines[i]):
                end = i
                break
        elif _fence_close(lines[i], ch, ln):
            ch = None
    body = "\n".join(lines[start:end]).rstrip()
    return body + "\n"


def default_out(doc: Path, chapter: int) -> Path:
    return Path("tmp/review-loop/01-stage/chapters") / doc.stem / f"ch{chapter}.md"


def run(doc: Path, chapter: int, out: Path | None, to_stdout: bool) -> int:
    try:
        text = extract(doc.read_text(encoding="utf-8"), chapter)
    except ValueError as exc:
        print(f"extract-chapter-slice: FAIL（{doc}）：{exc}", file=sys.stderr)
        return 1
    if to_stdout:
        sys.stdout.write(text)
        return 0
    target = out / f"ch{chapter}.md" if out else default_out(doc, chapter)
    if out and out.suffix == ".md":
        target = out
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")
    print(f"extract-chapter-slice: {target}（{len(text.splitlines())} 行）")
    return 0


def list_chapters(doc: Path) -> int:
    marks, closed = scan(doc.read_text(encoding="utf-8"))
    if not closed:
        print("extract-chapter-slice: FAIL：围栏未闭合", file=sys.stderr)
        return 1
    if not marks:
        print("extract-chapter-slice: FAIL：找不到章", file=sys.stderr)
        return 1
    nums = [n for _, n, _ in marks]
    dup = sorted({n for n in nums if nums.count(n) > 1})
    if dup:
        print("extract-chapter-slice: FAIL：章号重复：" + "、".join(str(n) for n in dup), file=sys.stderr)
        return 1
    missing = [n for n in range(1, max(nums) + 1) if n not in nums]
    if missing:
        print("extract-chapter-slice: FAIL：章号序列不完整，缺：" + "、".join(str(n) for n in missing),
              file=sys.stderr)
        return 1
    for _, n, title in marks:
        print(f"{n}\t{title}")
    return 0


def self_test() -> int:
    good = """# 样章

序言。

## 1. 概述

第一章正文。

```md
## 2. 围栏里的假章
```

## 2. C 源码

第二章第一段。

### 2.1 小节

第二章小节。

## 3. 设计

第三章。
"""
    no_ch = "# 无章\n\n只有前言。\n"
    unclosed = "## 1. 章\n\n```md\n没有闭合\n"
    dup = "## 1. 甲\n\n## 1. 乙\n"
    missing = "## 1. 甲\n\n## 3. 丙\n"
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "good.md").write_text(good, encoding="utf-8")
        (root / "no.md").write_text(no_ch, encoding="utf-8")
        (root / "unclosed.md").write_text(unclosed, encoding="utf-8")
        (root / "dup.md").write_text(dup, encoding="utf-8")
        (root / "missing.md").write_text(missing, encoding="utf-8")
        ch1 = extract(good, 1)
        assert ch1.startswith("## 1. 概述"), ch1
        assert "围栏里的假章" in ch1  # 围栏内 `## 2.` 不切章，仍属第一章正文
        assert "第二章第一段" not in ch1
        ch2 = extract(good, 2)
        assert ch2.startswith("## 2. C 源码"), ch2
        assert "第二章小节" in ch2
        assert "## 3. 设计" not in ch2
        assert "第一章正文" not in ch2
        out = root / "out"
        rc = run(root / "good.md", 2, out, False)
        if rc != 0 or not (out / "ch2.md").is_file():
            print("self-test FAIL: 输出文件未生成", file=sys.stderr)
            return 1
        if (out / "ch2.md").read_text(encoding="utf-8") != ch2:
            print("self-test FAIL: 文件内容与 extract 不一致", file=sys.stderr)
            return 1
        for name, needle in (("no.md", "找不到"), ("unclosed.md", "围栏"), ("dup.md", "重复"), ("missing.md", "序列不完整")):
            try:
                extract((root / name).read_text(encoding="utf-8"), 1)
            except ValueError as exc:
                if needle not in str(exc):
                    print(f"self-test FAIL: {name} 未命中 {needle}：{exc}", file=sys.stderr)
                    return 1
            else:
                print(f"self-test FAIL: {name} 未报错", file=sys.stderr)
                return 1
        if list_chapters(root / "good.md") != 0:
            print("self-test FAIL: --list 对合法文档报错", file=sys.stderr)
            return 1
        if list_chapters(root / "missing.md") != 1:
            print("self-test FAIL: --list 未拦截缺章", file=sys.stderr)
            return 1
        try:
            extract(good, 9)
        except ValueError as exc:
            if "找不到第 9 章" not in str(exc):
                print(f"self-test FAIL: 越界章号错误信息不符：{exc}", file=sys.stderr)
                return 1
        else:
            print("self-test FAIL: 越界章号未报错", file=sys.stderr)
            return 1
    print("self-test PASS: 章边界、围栏内假章、无章/未闭合围栏/重复章号/越界章号均按预期")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="按 `## N.` 提取章切片")
    ap.add_argument("doc", nargs="?")
    ap.add_argument("--chapter", type=int)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--stdout", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.doc:
        ap.print_help()
        return 2
    doc = Path(args.doc)
    if not doc.is_file():
        print(f"文档不存在：{doc}", file=sys.stderr)
        return 2
    if args.list:
        return list_chapters(doc)
    if args.chapter is None:
        ap.print_help()
        return 2
    return run(doc, args.chapter, args.out, args.stdout)


if __name__ == "__main__":
    raise SystemExit(main())
