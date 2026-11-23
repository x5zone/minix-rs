#!/usr/bin/env python3
"""chapter-contract-check.py — 章契约机械门（V3.13）

规范源：`prompt/review-rules/review-chapter-contract.md`。
只判机械可判部分：Ch1 禁用项目标识符/字段表/代码块、Ch2 禁用 Rust 项目名、
章内围栏代码总行数 >25 只报 WARN、Ch4 先报 WARN。教学合理性与节序由章级六视角判定。

用法：
  python3 tools/chapter-contract-check.py <doc>
  python3 tools/chapter-contract-check.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

CHAPTER_RE = re.compile(r"^##\s+(\d+)\.(?!\d)", re.MULTILINE)

CH1_FORBIDDEN = [
    "KernelInfo", "BootReport", "BootStorage", "RamAuthority", "SourceRecord",
    "KernelInfoView", "BOOT_HANDOFF", "KERNEL_INFO", "wire", "v1", "v9", "v10",
    "MemoryRegion", "BootModule", "PlatformDescSource",
]
PREFIX_TERMS = {"BOOT_HANDOFF", "KERNEL_INFO"}
CASELESS_TERMS = {"wire", "v1", "v9", "v10"}

CH2_RUST = [
    "KernelInfo", "BootReport", "BootStorage", "KernelInfoView", "RamAuthority",
    "SourceRecord", "BootPrepareResult", "BootAlloc", "BumpRegion", "MemMap",
    "arch_boot", "store_boot_report", "store_kernel_info", "prepare_boot",
    "run_self_boot", "kernel_info", "land_and_store", "MODULE_NAMES", "boot_modules", ".rs",
]

def derive_rust_symbols() -> set[str]:
    """从 os/ 公开符号机械派生；只收长度 >=5 且含大写/下划线的项目特定名。"""
    root = Path(__file__).resolve().parents[1] / "os"
    out: set[str] = set()
    if not root.is_dir():
        return out
    pat = re.compile(r"\bpub\s+(?:struct|fn|const|enum|trait)\s+([A-Za-z_][A-Za-z0-9_]*)")
    for f in root.rglob("*.rs"):
        try:
            txt = f.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for m in pat.finditer(txt):
            name = m.group(1)
            if len(name) < 5:
                continue
            if not re.match(r"^(Kernel|Boot|Wire|Source|Ram|Mem|Region|Handoff|Platform|Module|Authority|Record|Report|Storage|Bump|Bootface|DirectMap|Vm)[A-Z0-9_]", name):
                continue
            out.add(name)
    return out


CH2_CASELESS = {"KernelInfo", "BootReport", "BootStorage", "KernelInfoView", "RamAuthority", "SourceRecord", "BootPrepareResult", "BootAlloc", "BumpRegion"}

CH4_WARN = [
    "KernelInfo", "BootReport", "BootStorage", "RamAuthority", "SourceRecord",
    "设计裁决", "为什么选择", "取舍",
]


def _fence_open(line: str):
    m = re.match(r"^\s*(`{3,}|~{3,})(.*)$", line)
    if not m:
        return None
    return m.group(1)[0], len(m.group(1)), m.group(2).strip()


def _fence_close(line: str, ch: str, ln: int) -> bool:
    m = re.match(r"^\s*(`{3,}|~{3,})\s*$", line)
    return bool(m and m.group(1)[0] == ch and len(m.group(1)) >= ln)


def _scan_fences(text: str) -> tuple[list[int], bool]:
    """CommonMark 语义：同字符、闭合长度不小于开启长度；错配视为未闭合。"""
    starts: list[int] = []
    ch = None
    ln = 0
    for i, line in enumerate(text.splitlines()):
        if ch is None:
            op = _fence_open(line)
            if op:
                ch, ln, _ = op
                starts.append(i)
        elif _fence_close(line, ch, ln):
            ch = None
    return starts, ch is None


def split_chapters(text: str) -> tuple[dict[int, str], str, list[str]]:
    """按章切分；只在围栏外识别 `## N.`。返回 (章, 前言, 错误)。"""
    lines = text.splitlines()
    errs: list[str] = []
    _, closed = _scan_fences(text)
    if not closed:
        errs.append("围栏代码块未闭合（无法可靠切章）")
    in_fence: tuple[str, int] | None = None
    marks: list[tuple[int, int]] = []
    for i, line in enumerate(lines):
        if in_fence is None:
            op = _fence_open(line)
            if op:
                in_fence = (op[0], op[1])
                continue
        elif _fence_close(line, *in_fence):
            in_fence = None
            continue
        else:
            continue
        m = CHAPTER_RE.match(line)
        if m:
            marks.append((i, int(m.group(1))))
    if not marks:
        errs.append("找不到任何 `## N.` 章")
        return {}, text, errs
    nums = [n for _, n in marks]
    dup = sorted({n for n in nums if nums.count(n) > 1})
    if dup:
        errs.append("章号重复：" + "、".join(str(n) for n in dup))
    preamble = "\n".join(lines[: marks[0][0]])
    out: dict[int, str] = {}
    for idx, (line_no, n) in enumerate(marks):
        end = marks[idx + 1][0] if idx + 1 < len(marks) else len(lines)
        out[n] = "\n".join(lines[line_no + 1:end])
    return out, preamble, errs


def fenced_blocks(body: str) -> list[list[str]]:
    blocks: list[list[str]] = []
    ch = None
    ln = 0
    cur: list[str] = []
    for line in body.splitlines():
        if ch is None:
            op = _fence_open(line)
            if op:
                ch, ln, _ = op
                cur = []
            continue
        if _fence_close(line, ch, ln):
            blocks.append(cur)
            ch = None
        else:
            cur.append(line)
    if ch is not None:
        blocks.append(cur)
    return blocks


def strip_fences(body: str) -> str:
    out: list[str] = []
    in_fence: tuple[str, int] | None = None
    for line in body.splitlines():
        if in_fence is None:
            op = _fence_open(line)
            if op:
                in_fence = (op[0], op[1])
                continue
            out.append(line)
        elif _fence_close(line, *in_fence):
            in_fence = None
    return "\n".join(out)


def hit_terms(body: str, terms: list[str], include_fences: bool = False,
              caseless_extra: set[str] | None = None) -> list[str]:
    text = body if include_fences else strip_fences(body)
    hits: list[str] = []
    caseless_extra = caseless_extra or set()
    for term in terms:
        if term == ".rs":
            if re.search(r"(?<!docs)\.rs\b", text):
                hits.append(term)
            continue
        if term.startswith("."):
            if term in text:
                hits.append(term)
            continue
        flags = re.IGNORECASE if (term in CASELESS_TERMS or term in caseless_extra) else 0
        suffix = r"[A-Za-z0-9_]*" if term in PREFIX_TERMS else r"(?![A-Za-z0-9_])"
        pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(term) + suffix, flags)
        if pat.search(text):
            hits.append(term)
    return list(dict.fromkeys(hits))


def check(doc: Path) -> tuple[list[str], list[str]]:
    text = doc.read_text(encoding="utf-8")
    chs, preamble, errs = split_chapters(text)
    warns: list[str] = []
    derived = derive_rust_symbols()
    if 1 in chs:
        ch1 = chs[1]
        scan1 = (preamble + "\n" + ch1) if preamble.strip() else ch1
        blocks = fenced_blocks(scan1)
        if blocks:
            errs.append(f"Ch1 含 {len(blocks)} 个围栏代码块（禁止）")
        hits = hit_terms(scan1, CH1_FORBIDDEN + sorted(derived), caseless_extra=set(CH1_FORBIDDEN))
        if hits:
            errs.append("Ch1 含禁用项目标识符：" + "、".join(hits))
        if re.search(r"^\s*\|\s*(字段|字段名|Field)\s*\|\s*(类型|Type)\s*\|",
                     scan1, re.MULTILINE | re.IGNORECASE):
            errs.append("Ch1 含字段表（字段/类型表头）")
    if 2 in chs:
        hits = hit_terms(chs[2], CH2_RUST + sorted(derived), include_fences=True,
                         caseless_extra=CH2_CASELESS)
        if hits:
            errs.append("Ch2 含 Rust 项目名/路径：" + "、".join(hits))
    for n, body in chs.items():
        total = sum(len([l for l in b if l.strip()]) for b in fenced_blocks(body))
        if total > 25:
            warns.append(f"Ch{n} 围栏代码总行数 {total} > 25（WARN；由冷读/编辑判断是否可缩）")
    if 4 in chs:
        hits = hit_terms(chs[4], CH4_WARN, caseless_extra=set(CH4_WARN))
        if hits:
            warns.append("Ch4 出现设计/类型词（WARN）：" + "、".join(hits))
    return errs, warns


def run(doc: Path) -> int:
    errs, warns = check(doc)
    for w in warns:
        print(f"[WARN] {w}")
    for e in errs:
        print(f"[FAIL] {e}")
    if errs:
        print(f"chapter-contract-check: FAIL（{doc}）")
        return 1
    print(f"chapter-contract-check: PASS（{doc}）")
    return 0


def self_test() -> int:
    good = """# 好文档

## 1. 概述

内核第一次拿到控制权时，必须先理解交接问题的形状：哪些数据必须带走，哪些只是临时视图，
以及四条通用不变量如何把“交接成立”变成可检查的条件。本项目支持三架构与多引导形态。

## 2. C 源码分析

C 侧由 `pre_init` 把引导器数据拷进 `kinfo`，Rust 侧只是对照它的外部行为。

## 3. Rust 设计

设计契约把交接载荷分成按值传递的结构；这里只给取舍，不给逐行代码。

```rust
let x = 1;
```

## 4. 实现

实现沿 `main` → `prepare` → 入口走，失败落在断言。
"""
    bad_ch1 = good.replace(
        "本项目支持三架构与多引导形态。",
        "本项目支持三架构；`KernelInfo` 有十个字段，wire 版本 v10。\n\n```rust\nfn x() {}\n```",
    )
    bad_ch2 = good.replace(
        "Rust 侧只是对照它的外部行为。",
        "`arch_boot` 与 `store_boot_report` 是 Rust 侧的入口。",
    )
    bad_ch2_fence = good.replace(
        "Rust 侧只是对照它的外部行为。",
        "```rust\nfn f() -> KernelInfo { store_boot_report() }\n```",
    )
    bad_ch3 = good.replace(
        "```rust\nlet x = 1;\n```",
        "```rust\n" + "\n".join(f"let x{i} = {i};" for i in range(30)) + "\n```",
    )
    bad_boot = good.replace(
        "本项目支持三架构与多引导形态。",
        "交接协议是 `BOOT_HANDOFF_V1`，常量是 `KERNEL_INFO_V10`。",
    )
    bad_field = good.replace(
        "本项目支持三架构与多引导形态。",
        "本项目支持三架构。\n\n| 字段名 | 类型 |\n|---|---|\n| x | u8 |",
    )
    no_chapters = "# 没有章\n\n这里写 `KernelInfo`、`wire`。\n\n```rust\nfn main() {}\n```\n"
    unclosed = good.replace("```rust\nlet x = 1;\n```", "```rust\nlet x = 1;")
    stray_in_fence = """# 样章

## 1. 概述

介绍。

## 2. C 源码分析

```c
int x = 1;
~~~
int y = 2;
## 9. 代码块里的假章
int z = 3;
```

## 3. Rust 设计

设计。

## 4. 实现

实现。
"""
    preamble_bypass = """# 样章

```md
~~~
## 9. 代码块里的假章
```

## 1. 概述

`KernelInfo` 有十个字段，wire 版本 v10。

```rust
fn f() {}
```
"""
    strip_after_fence = """# 样章

## 1. 概述

介绍。

## 4. 实现

```c
~~~
```

`KernelInfo` 与设计裁决。
"""
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        files = {
            "good.md": good, "ch1.md": bad_ch1, "ch2.md": bad_ch2,
            "ch2f.md": bad_ch2_fence, "ch3.md": bad_ch3, "boot.md": bad_boot,
            "field.md": bad_field, "none.md": no_chapters, "unclosed.md": unclosed,
            "stray.md": stray_in_fence, "bypass.md": preamble_bypass,
            "strip.md": strip_after_fence,
        }
        for name, text in files.items():
            (root / name).write_text(text, encoding="utf-8")
        if run(root / "good.md") != 0:
            print("self-test FAIL: 合法样本被判红", file=sys.stderr)
            return 1
        expected = {
            "ch1.md": "Ch1", "ch2.md": "Ch2", "ch2f.md": "Ch2",
            "boot.md": "Ch1", "field.md": "Ch1",
            "none.md": "章", "unclosed.md": "围栏",
        }
        for name, needle in expected.items():
            errs, _ = check(root / name)
            if not any(needle in e for e in errs):
                print(f"self-test FAIL: {name} 未命中 {needle}；实际 {errs}", file=sys.stderr)
                return 1
        errs3, warns3 = check(root / "ch3.md")
        if errs3 or not any("Ch3" in w and "总行数" in w for w in warns3):
            print(f"self-test FAIL: Ch3 30 行代码未按 WARN 处理；errs={errs3} warns={warns3}", file=sys.stderr)
            return 1
        _chs, _pre, stray_errs = split_chapters(stray_in_fence)
        if sorted(_chs) != [1, 2, 3, 4] or stray_errs:
            print(f"self-test FAIL: 围栏内 `~~~` 吞章；chs={sorted(_chs)} errs={stray_errs}", file=sys.stderr)
            return 1
        bypass_errs, _ = check(root / "bypass.md")
        if not any("Ch1" in e for e in bypass_errs):
            print(f"self-test FAIL: 前言围栏绕过 Ch1 检查；errs={bypass_errs}", file=sys.stderr)
            return 1
        strip_errs, strip_warns = check(root / "strip.md")
        if strip_errs or not any("Ch4" in w and "KernelInfo" in w for w in strip_warns):
            print(f"self-test FAIL: 围栏内 `~~~` 截断散文扫描；errs={strip_errs} warns={strip_warns}", file=sys.stderr)
            return 1
    print("self-test PASS: Ch1 标识符/字段表/代码块、Ch2 散文与围栏、Ch3 总行数 WARN、无章/未闭合围栏、大写常量、围栏内杂散标记不吞章均按预期")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="章契约机械门（V3.13）")
    ap.add_argument("doc", nargs="?")
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
    return run(doc)


if __name__ == "__main__":
    raise SystemExit(main())
