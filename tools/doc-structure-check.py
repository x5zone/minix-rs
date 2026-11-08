#!/usr/bin/env python3
"""doc-structure-check.py — 文档表结构与内部 .md 相对链接门（P-2 第一版）

R18 裁决 P-2（`DECISIONS-PENDING-R18.md` §8.2）：
  - 解析器驱动，不靠肉眼。表块连续性：表头紧跟分隔行、表体无意外空行、
    fenced code block 排除、转义竖线与行内代码正确。
  - 内部 `.md` 相对链接按**文档目录**解析；外链不查；fenced code 与行内
    代码里的示例不查。
  - 第一版不做标题锚点校验与外部链接存活检查。
  - 检查器自身配正/负 fixture（`--self-test` 用临时目录现场生成）。

为什么需要：五道既有文档门不检查 Markdown 表格结构，也不解析跨文档链接的
相对基址。本轮出现过「一张字段表被空行劈成两半，五门全绿」与「6 个 .md
链接里 1 个从文档目录解析不到」。

用法：
    python3 tools/doc-structure-check.py <doc.md> [more.md | dir ...]
    python3 tools/doc-structure-check.py --self-test

退出码：
    0 = 无问题
    1 = 有问题（逐条输出）
    2 = 用法/环境错误
"""

import argparse
import re
import sys
import tempfile
from pathlib import Path

FENCE = re.compile(r"^\s*(```|~~~)")
INLINE_LINK = re.compile(r"\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+[\"'][^\"']*[\"'])?\s*\)")
SEPARATOR_CELL = re.compile(r"^:?-+:?$")
EXTERNAL_PREFIXES = ("http://", "https://", "mailto:", "ftp://", "file://", "data:")
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


def mask_code_spans(line: str) -> str:
    """Replace inline code spans (backtick runs) with spaces of equal width.

    Link examples and pipe characters inside inline code are content, not
    markup; masking keeps column counting and link scanning honest without
    shifting character offsets.
    """
    out = []
    i = 0
    while i < len(line):
        if line[i] != "`":
            out.append(line[i])
            i += 1
            continue
        j = i
        while j < len(line) and line[j] == "`":
            j += 1
        run = line[i:j]
        close = line.find(run, j)
        if close < 0:
            out.append(run)
            i = j
            continue
        out.append(" " * (close + len(run) - i))
        i = close + len(run)
    return "".join(out)


def iter_markup_lines(text: str):
    """Yield `(lineno, masked_line)` for lines outside fenced code blocks."""
    in_fence = False
    for lineno, raw in enumerate(text.splitlines(), 1):
        if FENCE.match(raw):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        yield lineno, mask_code_spans(raw)


def split_cells(line: str) -> list:
    """Split a table row into cells.

    Escaped pipes (`\\|`) stay inside their cell; pipes inside inline code
    spans are content. Leading/trailing border pipes are structural.
    """
    s = line.strip()
    if s.startswith("|"):
        s = s[1:]
    if s.endswith("|") and not s.endswith("\\|"):
        s = s[:-1]
    cells = []
    cur = []
    i = 0
    in_code = False
    code_run = 0
    while i < len(s):
        ch = s[i]
        if ch == "\\" and i + 1 < len(s) and s[i + 1] == "|":
            cur.append("|")
            i += 2
            continue
        if ch == "`":
            j = i
            while j < len(s) and s[j] == "`":
                j += 1
            run = j - i
            if not in_code:
                in_code = True
                code_run = run
            elif run == code_run:
                in_code = False
            cur.append("`" * run)
            i = j
            continue
        if ch == "|" and not in_code:
            cells.append("".join(cur))
            cur = []
            i += 1
            continue
        cur.append(ch)
        i += 1
    cells.append("".join(cur))
    return [cell.strip() for cell in cells]


def is_separator_row(cells: list) -> bool:
    return bool(cells) and all(SEPARATOR_CELL.match(cell.replace(" ", "")) for cell in cells)


def check_tables(text: str) -> list:
    """Return `(lineno, message)` issues for table blocks in `text`."""
    issues = []
    blocks = []
    current = None
    for lineno, line in iter_markup_lines(text):
        if line.lstrip().startswith("|"):
            if current is None:
                current = [lineno, [line]]
            else:
                current[1].append(line)
        elif current is not None:
            blocks.append(current)
            current = None
    if current is not None:
        blocks.append(current)

    for start, rows in blocks:
        if len(rows) < 2:
            issues.append(
                (start, "表格块没有分隔行（表头后必须紧跟 |---| 分隔行）")
            )
            continue
        header = split_cells(rows[0])
        separator = split_cells(rows[1])
        if not is_separator_row(separator):
            issues.append(
                (
                    start + 1,
                    "表格块第 2 行不是分隔行——表可能被空行劈成两半，"
                    "或表头缺失（每个表块必须以「表头 + 分隔行」开始）",
                )
            )
            continue
        if len(header) != len(separator):
            issues.append(
                (
                    start + 1,
                    f"表头 {len(header)} 列、分隔行 {len(separator)} 列，列数不一致",
                )
            )
        for offset, row in enumerate(rows[2:], 2):
            cells = split_cells(row)
            if len(cells) != len(header):
                issues.append(
                    (
                        start + offset,
                        f"表体 {len(cells)} 列、表头 {len(header)} 列，列数不一致",
                    )
                )
    return issues


def strip_fragment(target: str) -> str:
    return target.split("#", 1)[0]


def check_links(path: Path, text: str) -> list:
    """Return `(lineno, message)` issues for internal `.md` links."""
    issues = []
    for lineno, line in iter_markup_lines(text):
        for match in INLINE_LINK.finditer(line):
            target = match.group(1)
            low = target.lower()
            if any(low.startswith(prefix) for prefix in EXTERNAL_PREFIXES):
                continue
            if SCHEME.match(target) and not low.endswith(".md") and ".md#" not in low:
                # Non-http schemes (e.g. `os/kernel/src/lib.rs:fn foo`) are
                # source-symbol anchors, handled by anchor-resolve.sh.
                continue
            if target.startswith("#") or not target:
                continue
            if low.startswith("/"):
                # Site-absolute targets are not doc-relative; out of scope.
                continue
            file_part = strip_fragment(target)
            if not file_part.lower().endswith(".md"):
                continue
            resolved = (path.parent / file_part).resolve()
            if not resolved.exists():
                issues.append(
                    (
                        lineno,
                        f"内部 .md 链接解析不到：{target}（按文档目录解析为 {resolved}）",
                    )
                )
    return issues


def check_file(path: Path) -> list:
    text = path.read_text(encoding="utf-8", errors="replace")
    return check_tables(text) + check_links(path, text)


def iter_files(raw_paths: list) -> list:
    files = []
    for raw in raw_paths:
        p = Path(raw)
        if p.is_file():
            if p.suffix == ".md":
                files.append(p)
            else:
                print(f"⚠ 跳过非 .md 文件：{raw}", file=sys.stderr)
        elif p.is_dir():
            files.extend(sorted(p.rglob("*.md")))
        else:
            print(f"⚠ 路径不存在：{raw}", file=sys.stderr)
            raise SystemExit(2)
    return files


def run(paths: list) -> int:
    files = iter_files(paths)
    if not files:
        print("doc-structure-check: 没有可检查的 .md 文件", file=sys.stderr)
        return 2
    total = 0
    for path in files:
        issues = check_file(path)
        for lineno, message in issues:
            print(f"⛔ {path}:{lineno}: {message}")
            total += 1
    if total:
        print(f"⛔ doc-structure-check: {total} 处问题（{len(files)} 个文件）")
        return 1
    print(f"✅ doc-structure-check: {len(files)} 个文件表结构与内部 .md 链接均可解析")
    return 0


def self_test() -> int:
    """Positive and negative fixtures; the fixtures test the checker itself."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)

        def write(name: str, body: str) -> Path:
            p = root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(body, encoding="utf-8")
            return p

        good_table = "| A | B |\n|---|---|\n| 1 | 2 |\n"
        split_table = "| A | B |\n|---|---|\n| 1 | 2 |\n\n| 3 | 4 |\n"
        bad_columns = "| A | B |\n|---|---|\n| 1 | 2 | 3 |\n"
        escaped = "| A | B |\n|---|---|\n| a \\| b | `x | y` |\n"
        fenced = "```\n| not | a table |\n\n| broken |\n```\n"

        checks = []

        p = write("good.md", good_table + "[ok](target.md)\n")
        write("target.md", "# target\n")
        checks.append(("正夹具（合法表 + 可解析链接）", p, 0))

        p = write("split.md", split_table)
        checks.append(("负夹具（空行劈开表格）", p, 1))

        p = write("columns.md", bad_columns)
        checks.append(("负夹具（表体列数不符）", p, 1))

        p = write("escaped.md", escaped)
        checks.append(("正夹具（转义竖线与行内代码里的竖线）", p, 0))

        p = write("fenced.md", fenced)
        checks.append(("正夹具（fenced code 内的假表格不报）", p, 0))

        p = write("broken-link.md", "[gone](missing.md)\n[ext](https://example.com)\n")
        checks.append(("负夹具（断链；外链不报）", p, 1))

        p = write("code-link.md", "```\n[example](missing.md)\n```\n`[inline](missing.md)`\n")
        checks.append(("正夹具（代码块与行内代码里的示例链接不报）", p, 0))

        p = write(
            "anchor.md",
            "[file](target.md#section)\n[anchor](#local)\n[sym](os/x.rs:fn y)\n",
        )
        checks.append(("正夹具（锚点剥掉后按文件解析；纯锚点与符号链接不查）", p, 0))

        failures = 0
        for label, path, expected in checks:
            got = len(check_file(path))
            ok = (got == 0) if expected == 0 else (got > 0)
            print(f"{'PASS' if ok else 'FAIL'} self-test: {label}（问题数 {got}）")
            failures += 0 if ok else 1

        # The positive run over the fixture tree must also pass as a whole.
        rc = run([str(root / "good.md"), str(root / "escaped.md"), str(root / "fenced.md")])
        if rc != 0:
            print("FAIL self-test: 正夹具整树运行应为 0")
            failures += 1
        return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Markdown 表结构与内部 .md 相对链接门（P-2 第一版）"
    )
    parser.add_argument("paths", nargs="*", help="要检查的 .md 文件或目录")
    parser.add_argument("--self-test", action="store_true", help="用正/负 fixture 测试检查器自身")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.paths:
        parser.error("至少给一个 .md 文件或目录（或用 --self-test）")
    return run(args.paths)


if __name__ == "__main__":
    sys.exit(main())
