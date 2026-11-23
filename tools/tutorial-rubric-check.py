#!/usr/bin/env python3
"""tutorial-rubric-check.py — 教程质量 rubric 检测器（V3.12 / TR-0、TR-3..TR-6）

规则源：`prompt/review-rules/review-tutorial-rubric.md`。
V3.12 起不再用标题关键词强迫章首出现「学习目标 / 前置概念」小节：
TR-1 学习目标与 TR-2 前置知识由 structure.md §11 复述测试与 G18 冷读判定。
本工具做**存在性与形状**检查（问题入口、示例含代码块、小结条数、术语表）；
「前置概念是否真的在更早章节」「失败对照是否真的指出后果」仍需面板/人判定。

判据（每个 `## ` 章；`参见/附录/索引/参考文献` 与 <20 行章豁免）：
  TR-0 问题入口  章内第一个非空内容必须是散文段落（非列表/表格/围栏代码），实义字符 ≥20
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
    "TR-3": re.compile(r"示例|最小|可运行|动手"),
    "TR-4": re.compile(r"如果|失败|反例|错误做法|常见错误"),
    "TR-5": re.compile(r"小结|总结|本章要点|要点回顾"),
    "TR-6": re.compile(r"术语|名词表"),
}
NEED = ("TR-3", "TR-4", "TR-5")
BAD_OPENERS = ("-", "*", "+", "|", "```", "~~~")
PLACEHOLDER = re.compile(r"^\s*[（(]\s*空\s*[）)]\s*$")


FENCE_OPEN_RE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
FENCE_CLOSE_RE = re.compile(r"^\s*(`{3,}|~{3,})\s*$")


def split_chapters(text: str) -> list[tuple[str, str]]:
    """围栏感知的 `## ` 切章；围栏里的 `##` 不算章。"""
    lines = text.splitlines(keepends=True)
    marks: list[tuple[int, str]] = []
    ch = None
    ln = 0
    off = 0
    for line in lines:
        if ch is None:
            m = FENCE_OPEN_RE.match(line)
            if m:
                ch = m.group(1)[0]; ln = len(m.group(1))
            elif re.match(r"^##\s+", line):
                marks.append((off, line[2:].strip()))
        else:
            m = FENCE_CLOSE_RE.match(line)
            if m and m.group(1)[0] == ch and len(m.group(1)) >= ln:
                ch = None
        off += len(line)
    out: list[tuple[str, str]] = []
    for i, (pos, title) in enumerate(marks):
        end = marks[i + 1][0] if i + 1 < len(marks) else len(text)
        nl = text.find("\n", pos)
        body_start = nl + 1 if nl >= 0 else len(text)
        out.append((title, text[body_start:end]))
    return out


def subsection_bodies(chapter: str) -> list[tuple[str, str]]:
    marks = list(SUB_RE.finditer(chapter))
    out: list[tuple[str, str]] = []
    for i, m in enumerate(marks):
        end = marks[i + 1].start() if i + 1 < len(marks) else len(chapter)
        out.append((m.group(1).strip(), chapter[m.end():end]))
    return out


N0_RE = re.compile(r"^###\s+(\d+)\.0\b", re.MULTILINE)
H3_RE = re.compile(r"^###\s+", re.MULTILINE)
H4_RE = re.compile(r"^####\s+(.+)$", re.MULTILINE)
TERM_RE = re.compile(r"术语|名词表")
SUMMARY_RE = re.compile(r"小结|总结|本章要点|要点回顾")
N0_ALLOWED = ("问题入口", "前置概念")


def _n0_body(chapter: str) -> tuple[int, int] | None:
    m = N0_RE.search(chapter)
    if not m:
        return None
    start = m.end()
    nxt = H3_RE.search(chapter, start)
    return start, (nxt.start() if nxt else len(chapter))


def n0_errors(chapter: str) -> list[str]:
    """规则一：N.0 只保留问题入口与前置概念两个子节，术语不得在其中。"""
    span = _n0_body(chapter)
    if span is None:
        return []
    start, end = span
    subs = [m.group(1).strip() for m in H4_RE.finditer(chapter, start, end)]
    errs: list[str] = []
    if len(subs) > 2:
        errs.append(f"N.0 有 {len(subs)} 个子节（要求 ≤2）")
    allowed = [s for s in subs if any(k in s for k in N0_ALLOWED)]
    if len(allowed) != len(subs):
        extra = [s for s in subs if s not in allowed]
        errs.append("N.0 含非入门子节：" + "、".join(extra))
    if any(TERM_RE.search(s) for s in subs):
        errs.append("N.0 内含术语小节（应移到章末或行内）")
    return errs


def placement_errors(chapter: str) -> list[str]:
    """规则一：小结在章末（术语可紧随），术语不在 N.0 内。"""
    errs: list[str] = []
    marks = [(m.start(), m.group(1).strip()) for m in H4_RE.finditer(chapter)]
    sum_pos = [p for p, t in marks if SUMMARY_RE.search(t)]
    term_pos = [p for p, t in marks if TERM_RE.search(t)]
    if sum_pos:
        last_sum = max(sum_pos)
        after = [t for p, t in marks if p > last_sum and not TERM_RE.search(t)]
        if after:
            errs.append("小结之后仍有非术语小节：" + "、".join(after[:3]))
    span = _n0_body(chapter)
    if span is not None and term_pos:
        start, end = span
        if any(start <= p < end for p in term_pos):
            errs.append("术语小节位于 N.0 内")
    return errs


def long_paragraph_warnings(chapter: str) -> list[str]:
    """报告项：单段 >500 实义字符。跳过标题/列表/表格/围栏代码。"""
    warns: list[str] = []
    in_fence = False
    block: list[str] = []
    for line in chapter.splitlines() + [""]:
        stripped = line.strip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if not stripped:
            if block:
                text = "".join(block)
                if not re.match(r"^(?:[-*+]|\d+[.)]|\||<|>)", text) and _meaningful_len(text) > 500:
                    warns.append(text[:36] + "…")
                block = []
            continue
        if re.match(r"^#{1,6}\s+", stripped) or re.match(r"^(?:[-*+]|\d+[.)]|\|)", stripped):
            if block:
                block = []
            continue
        block.append(stripped)
    return warns


def _meaningful_len(text: str) -> int:
    lines = [ln for ln in text.splitlines() if not PLACEHOLDER.match(ln)]
    return len(re.sub(r"\s+", "", "\n".join(lines)))


def _tr0_error(body: str) -> str | None:
    """章内第一个非空内容必须是自然语言段落，或 N.0 的“问题入口”子节。"""
    lines = body.splitlines()
    first_sub = next((i for i, ln in enumerate(lines) if re.match(r"^###\s+", ln)), len(lines))
    first = next((i for i in range(first_sub) if lines[i].strip()), None)
    if first is None:
        # N.0 结构：章首直接进入 ### N.0，问题入口以 #### 问题入口 承载。
        m = re.search(r"^####\s+问题入口\s*$(.*?)(?=^####\s|^###\s|\Z)",
                      body, re.MULTILINE | re.DOTALL)
        if m and _meaningful_len(m.group(1)) >= 20:
            return None
        return "TR-0（章首没有问题入口段落）"
    opener = lines[first].strip()
    if opener.startswith(BAD_OPENERS):
        return "TR-0（章首以清单/表格/代码开场，未先说明问题）"
    chunk = "\n".join(ln for ln in lines[first:first_sub] if ln.strip())
    chunk = re.sub(r"^\s*>\s?", "", chunk, flags=re.MULTILINE)
    if _meaningful_len(chunk) < 20:
        return "TR-0（问题入口不足 20 实义字符）"
    return None


def check_chapter(title: str, body: str, has_glossary: bool) -> list[str]:
    missing: list[str] = []
    tr0 = _tr0_error(body)
    if tr0:
        missing.append(tr0)
    missing.extend(n0_errors(body))
    missing.extend(placement_errors(body))
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
        elif k == "TR-4" and _meaningful_len(have[k][1]) < 20:
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
        for snippet in long_paragraph_warnings(body):
            print(f"[WARN] {title}: 单段超过 500 实义字符（{snippet}）")
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

读者第一次打开机器时，最先遇到的问题是 CPU 还在实地址模式，而内核代码按高地址链接。
本章先把这个问题讲清楚，再决定第一条跳转指令应该放在哪里。

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

上一章只建了根表，还没有回答内核入口应该使用哪个栈。本章沿着这个缺口继续往下走。

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
        "# 坏教程\n\n## 1. 只有术语清单\n\n"
        "- DirectMapLayout\n- plan_dm_policy\n- LiveBootstrap\n- source 4\n\n"
        + "这里讲了很多机制，但没有目标、没有示例、没有小结。\n" * 25
        + "\n## 2. 有示例但无问题入口\n\n### 示例\n\n```rust\nfn x() {}\n```\n"
        + "补充叙述。\n" * 25
    )
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "good.md").write_text(good, encoding="utf-8")
        (root / "bad.md").write_text(bad, encoding="utf-8")
        if run(root / "good.md", False) != 0:
            print("self-test FAIL: 合规文档被判红")
            return 1
        if run(root / "bad.md", False) == 0:
            print("self-test FAIL: 缺项文档未被拦截")
            return 1
    print("self-test PASS: 问题入口/示例/对照/小结齐备通过；清单开场与缺项被拦截")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="教程质量 rubric 检测器（V3.12）")
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
