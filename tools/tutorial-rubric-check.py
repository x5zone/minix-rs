#!/usr/bin/env python3
"""tutorial-rubric-check.py — 教程质量 rubric 检测器（V3.8 / TR-1..TR-6）

规则源：`prompt/review-rules/review-tutorial-rubric.md`。
本工具做**存在性与形状**检查（标题关键词、小节非空、示例含代码块、小结条数）；
「前置概念是否真的在更早章节」「失败对照是否真的指出后果」仍需面板/人判定。

判据（每个 `## ` 章；`参见/附录/索引/参考文献` 与 <20 行章豁免）：
  TR-1 学习目标  标题含 学习目标|本章目标|读完本章|你将
  TR-2 前置概念  标题含 前置|预备|前提
  TR-3 最小示例  标题含 示例|最小|可运行|动手，且该节含围栏代码块或编号步骤
  TR-4 失败对照  标题含 如果|失败|反例|错误做法|常见错误
  TR-5 小结      标题含 小结|总结|本章要点|要点回顾，条目 1–5
  TR-6 术语      篇内有 术语/名词表 章，或章内有 术语 小节
概念密度（报告项）：每 100 行 `###` 数 > 8 → WARN。

用法：
  python3 tools/tutorial-rubric-check.py <doc>
  python3 tools/tutorial-rubric-check.py <doc> --report   # 只报告，不 exit 1
  python3 tools/tutorial-rubric-check.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

CHAPTER_RE = re.compile(r"^##\s+(.+)$", re.MULTILINE)
SUB_RE = re.compile(r"^#{3,4}\s+(.+)$", re.MULTILINE)
SKIP_TITLE = re.compile(r"参见|附录|索引|参考文献|后记|致谢")
PATTERNS = {
    "TR-1": re.compile(r"学习目标|本章目标|读完本章|你将"),
    "TR-2": re.compile(r"前置|预备|前提"),
    "TR-3": re.compile(r"示例|最小|可运行|动手"),
    "TR-4": re.compile(r"如果|失败|反例|错误做法|常见错误"),
    "TR-5": re.compile(r"小结|总结|本章要点|要点回顾"),
    "TR-6": re.compile(r"术语|名词表"),
}
NEED = ("TR-1", "TR-2", "TR-3", "TR-4", "TR-5")


def split_chapters(text: str) -> list[tuple[str, str]]:
    marks = list(CHAPTER_RE.finditer(text))
    out: list[tuple[str, str]] = []
    for i, m in enumerate(marks):
        end = marks[i + 1].start() if i + 1 < len(marks) else len(text)
        out.append((m.group(1).strip(), text[m.end():end]))
    return out


def subsection_bodies(chapter: str) -> list[tuple[str, str]]:
    marks = list(SUB_RE.finditer(chapter))
    out: list[tuple[str, str]] = []
    for i, m in enumerate(marks):
        end = marks[i + 1].start() if i + 1 < len(marks) else len(chapter)
        out.append((m.group(1).strip(), chapter[m.end():end]))
    return out


PLACEHOLDER = re.compile(r"^\s*[（(]\s*空\s*[）)]\s*$")


def _meaningful_len(text: str) -> int:
    lines = [ln for ln in text.splitlines() if not PLACEHOLDER.match(ln)]
    return len(re.sub(r"\s+", "", "\n".join(lines)))


def check_chapter(title: str, body: str, has_glossary: bool) -> list[str]:
    missing: list[str] = []
    subs = subsection_bodies(body)
    have: dict[str, tuple[str, str] | None] = {k: None for k in PATTERNS}
    for k, pat in PATTERNS.items():
        for st, sb in subs:
            if pat.search(st):
                have[k] = (st, sb)
                break
    for k in NEED:
        if have[k] is None:
            missing.append(f"{k}（关键词 {PATTERNS[k].pattern.split('|')[0]}…）")
        elif k in ("TR-1", "TR-2", "TR-4") and _meaningful_len(have[k][1]) < 20:
            missing.append(f"{k}（正文不足 20 字或只有占位）")
    # TR-3：示例节必须含围栏代码块或编号步骤
    if have["TR-3"] is not None:
        sb = have["TR-3"][1]
        if "```" not in sb and not re.search(r"^\s*(?:\d+[.)]|[-*]\s+\d+)", sb, re.MULTILINE):
            missing.append("TR-3（示例节无代码块/编号步骤）")
    # TR-5：小结条目 1–5，且每条有内容
    if have["TR-5"] is not None:
        items = [
            ln for ln in have["TR-5"][1].splitlines()
            if re.match(r"^\s*(?:[-*]|\d+[.)])\s+\S", ln)
        ]
        if not (1 <= len(items) <= 5):
            missing.append(f"TR-5（小结条目 {len(items)}，要求 1–5 且有内容）")
    # TR-6：篇末术语表或章内术语小节，且正文非空
    if not has_glossary and have["TR-6"] is None:
        missing.append("TR-6（无篇末术语表，也无章内术语小节）")
    elif have["TR-6"] is not None and _meaningful_len(have["TR-6"][1]) < 20:
        missing.append("TR-6（术语节正文不足 20 字）")
    return missing


def run(doc: Path, report_only: bool) -> int:
    text = doc.read_text(encoding="utf-8")
    chapters = split_chapters(text)
    has_glossary = any(re.search(r"术语|名词表", t) for t, _ in chapters) or any(
        re.search(r"术语|名词表", st)
        for _, body in chapters
        for st, _ in subsection_bodies(body)
    )
    failures = 0
    checked = 0
    short = 0
    for title, body in chapters:
        if SKIP_TITLE.search(title):
            continue
        if len(body.splitlines()) < 20:
            short += 1
            print(f"[WARN] {title}: 整章不足 20 行，跳过 rubric 判定")
            continue
        checked += 1
        missing = check_chapter(title, body, has_glossary)
        nlines = len(body.splitlines())
        density = len(re.findall(r"^###\s+", body, re.MULTILINE)) / max(1, nlines) * 100
        if nlines >= 100 and density > 8:
            print(f"[WARN] {title}: 概念密度 {density:.1f} 个三级小节/100 行（>8）")
        if missing:
            failures += 1
            print(f"[FAIL] {title}: 缺 " + "；".join(missing))
    print(f"tutorial-rubric-check: 检查 {checked} 章，{failures} 章缺项"
          + (f"，{short} 章因过短跳过" if short else "") + f"（{doc}）")
    if report_only:
        return 0
    if checked == 0:
        print("tutorial-rubric-check: 没有可判定章（全部 <20 行或均被跳过）", file=sys.stderr)
        return 2
    return 1 if failures else 0


def self_test() -> int:
    import tempfile

    good = """# 好教程

## 1. 起步

### 学习目标

- 能说出启动第一阶段需要先建立根页表，再打开分页，最后跳转到内核入口。

### 前置概念

- 无（开篇）：读者只需要知道虚拟地址和页表这两个词，不要求读过前章。

### 最小示例

```rust
fn main() { boot(); }
```

### 如果先开分页会怎样

会取指故障，因为页表还没建。所以这里先建根。

### 小结

- 先建根再开页

### 术语

- **根表**：顶层页表，页表树的起点；内核通过它把虚拟地址翻译成物理地址。

## 2. 下一章

### 学习目标

- 能解释从低地址启动到高半区内核入口的跳转步骤与前提条件。

### 前置概念

- 第 1 章建立的根表，以及分页开启后虚拟地址到物理地址的映射关系。

### 最小示例

```asm
jmp entry
```

### 失败对照

如果先清栈再保存返回地址，跳转后就找不到返回路径，所以先保存寄存器再跳转。

### 小结

- 先保存再跳
"""
    bad = (
        "# 坏教程\n\n## 1. 只有叙述\n\n"
        + "这里讲了很多机制，但没有目标、没有示例、没有小结。\n" * 25
        + "\n## 2. 有示例但无其它\n\n### 示例\n\n```rust\nfn x() {}\n```\n"
        + "补充叙述。\n" * 25
    )
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "good.md").write_text(good, encoding="utf-8")
        (root / "bad.md").write_text(bad, encoding="utf-8")
        # 好文档：两章都齐（第 2 章术语由篇内第 1 章术语表满足）
        if run(root / "good.md", False) != 0:
            print("self-test FAIL: 合规文档被判红")
            return 1
        # 坏文档：两章都缺项 → exit 1
        if run(root / "bad.md", False) == 0:
            print("self-test FAIL: 缺项文档未被拦截")
            return 1
    print("self-test PASS: 合规文档通过；缺目标/前置/示例/对照/小结被拦截")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="教程质量 rubric 检测器（V3.8）")
    ap.add_argument("doc", nargs="?")
    ap.add_argument("--report", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.doc:
        ap.print_help()
        return 2
    return run(Path(args.doc), args.report)


if __name__ == "__main__":
    sys.exit(main())
