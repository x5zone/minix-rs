#!/usr/bin/env python3
"""doc-citation-check.py — 文档代码引文与源码的机械对账（V3）

扫描文档中的 fenced code block，读取块首的引文标注：

    // 代码引用，os/boot-shim/src/uefi_helpers.rs:fn exit_boot_services。
    // 节选（...），os/libs/minix-boot/src/boot_prepare.rs:struct BootPrepareResult。

然后做三件事：

1. 标注里的每个 `os/**/*.rs` 路径必须存在；
2. 标注里的符号（`fn`/`struct`/`trait`/`impl`/`const`/...）必须在源码中存在；
3. 块内代码行（去掉纯注释、中文旁注、省略号、空行后）必须按顺序出现在源码中。
   这是**子序列**检查，不是连续 diff：允许源码中间有省略，但不允许文档里出现
   源码没有的行——R25 的 `records_in_wire = true` 缺失正是这一类。

刻意保守：纯注释、含中文的行、含 `...`/`…` 的行、`}`/`};` 等单符号行跳过；
`节选` 允许省略但不允许改写——块内每行具体代码必须仍在源码中（V3.2 收紧，
R30 FACT-B-01 判例）；`改写`/`示意`/`反例`/`重排`/`译述` 显式声明的块豁免
逐行检查（它们声明了自己不是逐字引文）。

用法：
    python3 tools/doc-citation-check.py <doc.md> [--root .] [--json] [--verbose]
    python3 tools/doc-citation-check.py --self-test

退出码：
    0 = 所有有标注的块通过（无标注的块给 WARN，不算失败）
    1 = 有路径消失、符号消失或代码行不在源码中
    2 = 用法/环境错误
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path

FENCE_RE = re.compile(r"^(```+|~~~+)\s*([A-Za-z0-9_+-]*)\s*$")
# V3.4 最小内核（P2-8 判例：D03/D04 的 C 侧引文 42/86 块全部静默跳过，门读数
# 「failed=0」实为「checked=0」的假绿）：标注支持三种形态——`// kind，rest`、
# `/* kind，rest。 */`（D03 形态）、`/* path:sym —— kind（notes） */`（D04 形态）；
# 路径扩展 `minix3/*.c|h`。kind 词表补「逐字引用」（D04 实用）。
ANNOTATION_RE = re.compile(
    r"^\s*(?://|/\*)\s*(代码引用|逐字引用|节选|改写|示意|反例)(?:\s*（([^）]*)）)?\s*[，,]\s*(.+?)\s*。?\s*(?:\*/)?\s*$"
)
C_ANNOTATION_RE = re.compile(r"^/\*\s*(?P<inner>.+?)\s*\*/$")
C_KIND_RE = re.compile(
    r"(?P<paths>.+?)\s*(?:——|—|--)\s*"
    r"(?P<kind>代码引用|逐字引用|节选|改写|示意|签名示意|反例)\s*(?:（(?P<notes>[^）]*)）)?\s*$"
)
PATH_RE = re.compile(
    r"(?:os/[\w./-]+\.(?:rs|S|lds|ld|inc)|minix3/[\w./-]+\.(?:[chS]|lds|ld|inc))"
)
ANCHOR_KIND_RE = re.compile(
    r"\b(fn|struct|trait|impl|const|static|type|enum|mod|union)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
TYPE_METHOD_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)$")
SKIP_LINE_RE = re.compile(r"^[{}();,]+$")
NON_ASCII_RE = re.compile(r"[^\x00-\x7f]")


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


# String literals must not span a newline: a stray `"` in a doc comment would
# otherwise swallow the rest of the file into one token and hide real code.
TOKEN_RE = re.compile(r'"(?:\\.|[^"\\\n])*"|[A-Za-z_][A-Za-z0-9_]*|\d+(?:\.\d+)?|[^\s]')


def normalize(line: str) -> str:
    """Normalize a code line before tokenization.

    Drops trailing comments, visibility modifiers and common path prefixes so
    `pub fn foo` / `fn foo` and `minix_boot::PhysRange` / `PhysRange` compare
    equal. Tokenization (not whitespace-sensitive substring matching) is what
    makes wrapped source lines compare equal.
    """
    line = line.strip()
    line = re.sub(r"\s+//.*$", "", line).strip()
    # V3.4：C 侧行内块注释剔除后再比对。
    line = re.sub(r"/\*.*?\*/", " ", line).strip()
    line = re.sub(r"\bpub(?:\([^)]*\))?\s+", "", line)
    for prefix in ("crate::", "minix_boot::", "minix_types::", "core::"):
        line = line.replace(prefix, "")
    return line


def tokenize(text: str) -> list[str]:
    return TOKEN_RE.findall(normalize(text))


def tokenize_source(source: str) -> list[str]:
    """Tokenize source line-by-line so comments are stripped before matching.

    Tokenizing the whole file at once lets a doc line match words inside a
    comment (false pass) and lets a stray quote in a comment swallow code into
    one token.
    """
    cleaned = "\n".join(normalize(line) for line in source.splitlines())
    return TOKEN_RE.findall(cleaned)


def find_tokens(haystack: list[str], needle: list[str], start: int) -> int:
    if not needle:
        return start
    limit = len(haystack) - len(needle)
    for i in range(start, limit + 1):
        if haystack[i : i + len(needle)] == needle:
            return i + len(needle)
    return -1


def should_skip(line: str) -> bool:
    stripped = line.strip()
    if not stripped:
        return True
    if stripped.startswith("//"):
        return True
    # V3.4：C 侧块注释行（`/* … */`、`* …` 续行、行尾 `*/`）跳过。
    if stripped.startswith("/*") or stripped.startswith("* ") or stripped.startswith("*/"):
        return True
    # r7：make/asm 的 `# 注释` 跳过（`#include`/`#define`/`#if` 是代码，不跳）。
    if re.match(r"^#\s", stripped):
        return True
    if "..." in stripped or "…" in stripped:
        return True
    # V3.6-b1（轨迹分析判例，替代「含非 ASCII 整行跳过」）：非 ASCII 判定必须
    # 在**剔除行内注释之后**做。原实现在原始行上判，于是
    # `Err(X) => { /* 扩容后重试 */ }` 这类「真代码 + 中文块注释」整行被豁免——
    # 中文化成了错代码的豁免卡（实测 152/~4000 引文行受影响，其中含无 `//`
    # 的真代码行）。先 normalize（已剔 `//` 尾注与 `/* */`），再判残余非 ASCII：
    # 残余非 ASCII 只可能来自字符串字面量或 CJK 标识符，那才是无可对账的形态。
    if NON_ASCII_RE.search(normalize(stripped)):
        return True
    if SKIP_LINE_RE.fullmatch(stripped):
        return True
    return False


def _parse_annotation(line: str) -> dict | None:
    """Parse `//`-style and (V3.4) `/* … —— kind（notes） */`-style annotations
    into one internal dict so downstream checks share a single shape."""
    m = ANNOTATION_RE.match(line)
    if m:
        return {"kind": m.group(1), "notes": m.group(2) or "", "rest": m.group(3)}
    # r7：第四形态 `// path:sym —— kind（notes）`（D04/D05 实用）——`//` 行同样
    # 按 C 侧破折号语序解析。
    s = line.strip()
    if s.startswith("//"):
        k = C_KIND_RE.match(s[2:].strip())
        if k:
            return {
                "kind": k.group("kind"),
                "notes": k.group("notes") or "",
                "rest": k.group("paths"),
            }
    c = C_ANNOTATION_RE.match(s)
    if c:
        k = C_KIND_RE.match(c.group("inner"))
        if k:
            return {
                "kind": k.group("kind"),
                "notes": k.group("notes") or "",
                "rest": k.group("paths"),
            }
    return None


def parse_blocks(text: str) -> list[dict]:
    """Return fenced blocks with annotation metadata.

    Supports the annotation inside the fence (current style) and on the line
    immediately before the fence (older style).
    """
    lines = text.splitlines()
    blocks: list[dict] = []
    i = 0
    while i < len(lines):
        m = FENCE_RE.match(lines[i])
        if not m:
            i += 1
            continue
        fence = m.group(1)
        start = i + 1
        body: list[str] = []
        i += 1
        while i < len(lines):
            close = FENCE_RE.match(lines[i])
            if close and close.group(1)[0] == fence[0] and len(close.group(1)) >= len(fence):
                break
            body.append(lines[i])
            i += 1
        end = i + 1
        annotation = None
        annotation_line = None
        if body:
            parsed = _parse_annotation(body[0])
            if parsed is not None:
                annotation_line = body[0]
                annotation = parsed
                body = body[1:]
        if annotation is None and start >= 2:
            prev = lines[start - 2].strip()
            parsed = _parse_annotation(prev)
            if parsed is not None:
                annotation_line = prev
                annotation = parsed
        blocks.append(
            {
                "start": start,
                "end": end,
                "lang": m.group(2),
                "annotation_line": annotation_line,
                "annotation": annotation,
                "body": body,
            }
        )
        i += 1
    return blocks


def parse_paths(rest: str) -> list[tuple[str, str]]:
    """Return (path, anchor_text) pairs from the annotation tail."""
    matches = list(PATH_RE.finditer(rest))
    result: list[tuple[str, str]] = []
    for idx, match in enumerate(matches):
        path = match.group(0)
        tail_end = matches[idx + 1].start() if idx + 1 < len(matches) else len(rest)
        tail = rest[match.end() : tail_end]
        anchor = ""
        am = re.match(r"\s*:\s*([^，。；]+)", tail)
        if am:
            anchor = am.group(1).strip()
            anchor = re.split(r"\s+与\s+|\s+的\s+|，|。|；", anchor)[0].strip()
        result.append((path, anchor))
    return result


def anchor_symbols(anchor: str) -> list[tuple[str | None, str]]:
    """Extract (kind, symbol) candidates from an anchor phrase."""
    if not anchor:
        return []
    found = ANCHOR_KIND_RE.findall(anchor)
    if found:
        return [(kind, name) for kind, name in found]
    tm = TYPE_METHOD_RE.match(anchor)
    if tm:
        return [("type", tm.group(1)), (None, tm.group(1)), (None, tm.group(2))]
    m = re.match(r"([A-Za-z_][A-Za-z0-9_]*)", anchor)
    if m:
        return [(None, m.group(1))]
    return []


def symbol_present(source: str, kind: str | None, name: str) -> bool:
    if kind and kind != "type":
        if re.search(rf"\b{re.escape(kind)}\s+{re.escape(name)}\b", source):
            return True
    if re.search(rf"\b{re.escape(name)}\b", source):
        return True
    return False


def declared_source_symbols(notes: str | None) -> list[str]:
    """`source-symbol:` names listed in a rewritten block's annotation."""
    if not notes:
        return []
    match = re.search(r"source-symbol\s*[:：]\s*([^;；]*)", notes)
    if not match:
        return []
    return [
        name
        for name in re.findall(r"[A-Za-z_][A-Za-z0-9_:]*", match.group(1))
        if name
    ]


def check_block(block: dict, root: Path) -> dict:
    annotation = block["annotation"]
    if annotation is None:
        return {"status": "SKIP", "reason": "无引文标注"}
    kind = annotation["kind"]
    notes = annotation["notes"]
    rest = annotation["rest"]
    paths = parse_paths(rest)
    location = f"{block['start']}-{block['end']}"
    if not paths:
        return {
            "status": "WARN",
            "location": location,
            "kind": kind,
            "reason": "标注里没有可对账的源码路径（os/*.rs 或 minix3/*.c|h）",
        }

    sources: list[str] = []
    anchor_reports: list[dict] = []
    for rel, anchor in paths:
        path = (root / rel).resolve()
        if not path.exists():
            return {
                "status": "FAIL",
                "location": location,
                "kind": kind,
                "path": rel,
                "reason": "源文件已消失",
            }
        source = read_text(path)
        sources.append(source)
        for symbol_kind, symbol in anchor_symbols(anchor):
            if not symbol_present(source, symbol_kind, symbol):
                anchor_reports.append(
                    {
                        "path": rel,
                        "anchor": anchor,
                        "symbol": symbol,
                        "kind": symbol_kind,
                        "present": False,
                    }
                )
    if anchor_reports:
        return {
            "status": "FAIL",
            "location": location,
            "kind": kind,
            "reason": "标注符号在源码中不存在",
            "anchors": anchor_reports,
        }

    # V3.2 边界（FACT-B-01 判例）：`节选` 只允许省略（跳过 `...`/`…` 行），
    # 不允许改写——块内每行具体代码必须仍能在源码里找到。R30 的 P0
    # （§4.6 引文停在 `let _ = paging.map_huge`，源码已是 `match`）正是
    # structure-only 放过的陈旧行。显式声明了 改写/示意/反例/重排/译述 的
    # 块才豁免逐行检查：它们声明了自己不是逐字引文，机械对账无从执行，
    # 只能靠 review-evals §4.0e 第 8 条的显式标记兜底。
    rewrite_declared = kind in {"改写", "示意", "反例"} or "示意" in kind or any(
        word in notes for word in ("改写", "示意", "反例", "重排", "译述")
    )
    if rewrite_declared:
        # R36-FACT-A-S2：改写块仍需证明符号真的存在。没有 `source-symbol:`
        # 时只能是 WARN（无法机械核对）；给了名字却找不到就是 FAIL。
        symbols = declared_source_symbols(notes)
        if not symbols:
            return {
                "status": "WARN",
                "location": location,
                "kind": kind,
                "reason": "声明为非逐字引文且未给 source-symbol，符号存在性未核对",
                "mode": "structure-only(declared-rewrite)",
            }
        missing = [
            name
            for name in symbols
            if not any(
                re.search(rf"\b{re.escape(name.split('::')[-1])}\b", source)
                for source in sources
            )
        ]
        if missing:
            return {
                "status": "FAIL",
                "location": location,
                "kind": kind,
                "reason": f"source-symbol 在源码中不存在: {', '.join(missing)}",
                "mode": "rewrite-symbol-missing",
            }
        # r8（对抗判例）：声明式豁免此前计为 checked，虚高「引文已对账」读数——
        # 改为 WARN：不阻断，但覆盖率只数真正逐行对账的块。
        return {
            "status": "WARN",
            "location": location,
            "kind": kind,
            "reason": "声明为非逐字引文；source-symbol 已核对，内容未逐行对账",
            "mode": "structure-only(declared-rewrite,source-symbol-ok)",
        }

    # Per-line token existence: whitespace, line wrapping and
    # `pub`/path-qualifier differences do not matter, but a code line whose
    # tokens are absent from every cited source is reported.
    #
    # Deliberately not a global-order subsequence: a `节选` block may
    # intentionally interleave snippets from two files or reorder them around
    # an omitted section, and treating that as a failure produces noise. The
    # gate's primary job is to catch a line the source no longer has (R25's
    # missing `records_in_wire = true`), not to reconstruct exact order.
    source_token_lists = [tokenize_source(source) for source in sources]
    code_lines = [line for line in block["body"] if not should_skip(line)]
    missing: list[str] = []
    for line in code_lines:
        needle = tokenize(line)
        if not needle:
            continue
        if not any(find_tokens(tokens, needle, 0) >= 0 for tokens in source_token_lists):
            missing.append(line.strip())
    if missing:
        return {
            "status": "FAIL",
            "location": location,
            "kind": kind,
            "reason": "代码行不在源码中（子序列对账失败）",
            "missing": missing[:5],
            "missing_count": len(missing),
            "checked_lines": len(code_lines),
        }
    return {
        "status": "PASS",
        "location": location,
        "kind": kind,
        "checked_lines": len(code_lines),
    }


def check_doc(doc: Path, root: Path, verbose: bool = False) -> tuple[list[dict], int]:
    text = read_text(doc)
    blocks = parse_blocks(text)
    results = [check_block(block, root) for block in blocks]
    failed = sum(1 for r in results if r["status"] == "FAIL")
    checked = sum(1 for r in results if r["status"] == "PASS")
    skipped = sum(1 for r in results if r["status"] == "SKIP")
    warned = sum(1 for r in results if r["status"] == "WARN")
    for r in results:
        # Failures always print their reason: a blocking gate that only prints
        # a count is unusable in a pre-commit hook.
        if r["status"] != "FAIL" and not verbose:
            continue
        line = f"[{r['status']}] block {r.get('location', '?')} {r.get('kind', '')}"
        if r["status"] == "FAIL":
            line += f" — {r.get('reason', '')}"
            if r.get("path"):
                line += f" ({r['path']})"
            if r.get("missing"):
                line += f"；缺: {r['missing'][0]}"
        print(line)
    print(
        f"{doc}: blocks={len(results)} checked={checked} failed={failed} "
        f"skipped={skipped} warned={warned}"
    )
    return results, 1 if failed else 0


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "os").mkdir()
        (root / "os" / "lib.rs").write_text(
            "pub fn foo() {\n    let x = 1;\n    let y = 2;\n}\n",
            encoding="utf-8",
        )
        good = root / "good.md"
        good.write_text(
            "```rust\n"
            "// 代码引用，os/lib.rs:fn foo。\n"
            "pub fn foo() {\n"
            "    let x = 1;\n"
            "    let y = 2;\n"
            "}\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(good, root)
        if code != 0:
            print("self-test FAIL: 正确引文被判 FAIL")
            return 1

        missing_line = root / "missing.md"
        missing_line.write_text(
            "```rust\n"
            "// 代码引用，os/lib.rs:fn foo。\n"
            "pub fn foo() {\n"
            "    let x = 1;\n"
            "    let z = 3;\n"
            "}\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(missing_line, root)
        if code == 0:
            print("self-test FAIL: 源码没有的代码行未被拦截")
            return 1

        gone = root / "gone.md"
        gone.write_text(
            "```rust\n// 代码引用，os/gone.rs:fn foo。\nfn foo() {}\n```\n",
            encoding="utf-8",
        )
        _, code = check_doc(gone, root)
        if code == 0:
            print("self-test FAIL: 源文件消失未被拦截")
            return 1

        bad_anchor = root / "anchor.md"
        bad_anchor.write_text(
            "```rust\n// 代码引用，os/lib.rs:fn bar。\nfn bar() {}\n```\n",
            encoding="utf-8",
        )
        _, code = check_doc(bad_anchor, root)
        if code == 0:
            print("self-test FAIL: 标注符号消失未被拦截")
            return 1

        skipped = root / "skip.md"
        skipped.write_text("```text\n不是引文\n```\n", encoding="utf-8")
        _, code = check_doc(skipped, root)
        if code != 0:
            print("self-test FAIL: 无标注块不应失败")
            return 1

        # V3.2 收紧的判例（FACT-B-01）：`节选` 块里的陈旧行必须被拦截；
        # 显式声明 改写 的块仍豁免；`节选` 的省略行（`...`）不拦。
        stale_excerpt = root / "stale_excerpt.md"
        stale_excerpt.write_text(
            "```rust\n"
            "// 节选（保留循环），os/lib.rs:fn foo。\n"
            "let _ = foo();\n"
            "let x = 1;\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(stale_excerpt, root)
        if code == 0:
            print("self-test FAIL: 节选块里的陈旧行未被拦截")
            return 1

        excerpt_omission = root / "excerpt_omission.md"
        excerpt_omission.write_text(
            "```rust\n"
            "// 节选（保留前两行），os/lib.rs:fn foo。\n"
            "pub fn foo() {\n"
            "    ...\n"
            "}\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(excerpt_omission, root)
        if code != 0:
            print("self-test FAIL: 节选的省略（...）被误拦")
            return 1

        declared_rewrite = root / "declared_rewrite.md"
        declared_rewrite.write_text(
            "```rust\n"
            "// 节选（改写：压成一行），os/lib.rs:fn foo。\n"
            "foo_with_defaults(1, 2);\n"
            "```\n",
            encoding="utf-8",
        )
        dr_results, code = check_doc(declared_rewrite, root)
        if code != 0:
            print("self-test FAIL: 显式声明改写的块被误拦")
            return 1
        # r8 判别：声明式豁免必须计 WARN、不计 checked（否则虚高覆盖率）。
        if not any(r["status"] == "WARN" for r in dr_results):
            print("self-test FAIL: structure-only 块未计 WARN（覆盖率虚高）")
            return 1

        # R36-FACT-A-S2：改写块可以用 source-symbol 证明符号仍存在；
        # 名字找不到时必须 FAIL，不能继续以“改写”豁免。
        rewrite_symbol_ok = root / "rewrite_symbol_ok.md"
        rewrite_symbol_ok.write_text(
            "```rust\n"
            "// 节选（改写：压成一行；source-symbol: foo），os/lib.rs:fn foo。\n"
            "foo_with_defaults(1, 2);\n"
            "```\n",
            encoding="utf-8",
        )
        rs_results, code = check_doc(rewrite_symbol_ok, root)
        if code != 0:
            print("self-test FAIL: source-symbol 存在的改写块被误拦")
            return 1
        if not any(
            r.get("mode") == "structure-only(declared-rewrite,source-symbol-ok)"
            for r in rs_results
        ):
            print("self-test FAIL: source-symbol 核对未进入读数")
            return 1
        rewrite_symbol_bad = root / "rewrite_symbol_bad.md"
        rewrite_symbol_bad.write_text(
            "```rust\n"
            "// 节选（改写：压成一行；source-symbol: ghost），os/lib.rs:fn foo。\n"
            "foo_with_defaults(1, 2);\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(rewrite_symbol_bad, root)
        if code == 0:
            print("self-test FAIL: source-symbol 指向不存在的符号未被拦截")
            return 1

        # V3.4：C 侧三种形态。
        (root / "minix3" / "k").mkdir(parents=True)
        (root / "minix3" / "k" / "main.c").write_text(
            "int cstart(void) {\n    boot();\n    return 0;\n}\n",
            encoding="utf-8",
        )
        c_d04 = root / "c_d04.md"
        c_d04.write_text(
            "```c\n"
            "/* minix3/k/main.c:cstart —— 代码引用 */\n"
            "int cstart(void) {\n    boot();\n    return 0;\n}\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(c_d04, root)
        if code != 0:
            print("self-test FAIL: C 侧 path——kind 形态被判 FAIL")
            return 1
        c_stale = root / "c_stale.md"
        c_stale.write_text(
            "```c\n"
            "/* minix3/k/main.c:cstart —— 节选（省略号处为其余初始化） */\n"
            "int cstart(void) {\n    not_in_source();\n    return 0;\n"
            "    /* 旁注：这行应被跳过 */\n}\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(c_stale, root)
        if code == 0:
            print("self-test FAIL: C 侧节选里的陈旧行未被拦截")
            return 1
        c_d03 = root / "c_d03.md"
        c_d03.write_text(
            "```c\n"
            "/* 代码引用，minix3/k/main.c:cstart（函数头到 boot 调用）。 */\n"
            "int cstart(void) {\n    boot();\n    return 0;\n}\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(c_d03, root)
        if code != 0:
            print("self-test FAIL: `/* kind，path */` 形态（D03）被判 FAIL")
            return 1

        # r7：第四形态 `// path —— kind`（D04/D05）+ .S/.inc 路径 + `#` 注释。
        (root / "minix3" / "k").mkdir(parents=True, exist_ok=True)
        (root / "minix3" / "k" / "head.S").write_text(
            "multiboot_init:\n    mov $k_initial_stktop, %esp\n    jmp main\n",
            encoding="utf-8",
        )
        fourth = root / "fourth.md"
        fourth.write_text(
            "```asm\n"
            "// minix3/k/head.S:multiboot_init —— 代码引用（换栈与跳转段）\n"
            "mov $k_initial_stktop, %esp\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(fourth, root)
        if code != 0:
            print("self-test FAIL: 第四形态 `// path —— kind` 被判 FAIL")
            return 1
        fourth_stale = root / "fourth_stale.md"
        fourth_stale.write_text(
            "```asm\n"
            "// minix3/k/head.S:multiboot_init —— 节选\n"
            "not_in_source_at_all();\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(fourth_stale, root)
        if code == 0:
            print("self-test FAIL: 第四形态陈旧行未被拦截")
            return 1
        mk = root / "mk.md"
        (root / "minix3" / "k" / "Makefile.inc").write_text(
            "# some object files we give a symbol prefix\nobj-y += main.o\n",
            encoding="utf-8",
        )
        mk.write_text(
            "```make\n"
            "/* 代码引用，minix3/k/Makefile.inc（前缀说明）。 */\n"
            "# 文档侧注释行——刻意不在源码中，验证 `#` 跳过规则在工作\n"
            "obj-y += main.o\n"
            "```\n",
            encoding="utf-8",
        )
        _, code = check_doc(mk, root)
        if code != 0:
            print("self-test FAIL: Makefile.inc 路径与 `#` 注释形态被判 FAIL")
            return 1

    print(
        "self-test PASS: 正确引文通过；缺行/缺文件/缺符号/节选陈旧行被拦截；"
        "省略与显式改写豁免；C 侧三形态与陈旧行对账；第四形态与 .S/.inc 对账"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="文档代码引文与源码机械对账")
    parser.add_argument("doc", nargs="?", help="文档路径")
    parser.add_argument("--root", default=".", help="仓库根（默认当前目录）")
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument("--verbose", "-v", action="store_true", help="逐块输出")
    parser.add_argument("--self-test", action="store_true", help="运行内置自测")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.doc:
        parser.print_help()
        return 2
    doc = Path(args.doc)
    if not doc.is_file():
        print(f"文档不存在: {doc}", file=sys.stderr)
        return 2
    root = Path(args.root)
    results, code = check_doc(doc, root, args.verbose)
    if args.json:
        print(json.dumps(results, ensure_ascii=False, indent=2))
    return code


if __name__ == "__main__":
    sys.exit(main())
