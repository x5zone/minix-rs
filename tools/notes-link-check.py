#!/usr/bin/env python3
"""
notes-link-check.py — Markdown 相对链接断链扫描（迁移前后各跑一次，做集合对账）

用途（目录迁移的验收前置工具）：
  迁移前跑一次建立"既有坏链基线"，迁移后再跑一次，比较两个集合。
  没有基线，迁移后的断链扫描会把历史遗留坏链误算成迁移引入的破坏。

判定规则：
  1. 只检查 *.md 文件中的 Markdown 链接 `[文字](目标)` 与 `<目标>` 自动链接。
  2. 跳过 fenced code block（``` 或 ~~~ 包裹）内的行 —— 那是代码内容，不是链接。
  3. 跳过外链（http/https/mailto/ftp）、纯锚点（# 开头）、空目标。
  4. 去掉 `#fragment` 后再解析；行号锚点（如 `...md#L78-L96`）天然被去掉。
  5. 解析基准依次尝试，命中任一即视为可解析：
       a) 相对当前文件所在目录（标准 Markdown 语义）
       b) 相对仓库根（文档里写 `rewrite-notes/...` 这类仓库内绝对路径的存量习惯）
     两者都落空 → 记 BROKEN，输出文件行、原始目标、两种解析结果。

用法：
    python3 tools/notes-link-check.py rewrite-notes/ redesign-notes/ study-notes/
    python3 tools/notes-link-check.py rewrite-notes/ --output /tmp/links-before.txt
    python3 tools/notes-link-check.py --self-test

退出码：
    0 = 无断链
    1 = 存在断链（报告已输出）
    2 = 用法/环境错误
"""

import argparse
import re
import sys
import tempfile
from pathlib import Path

# [文字](目标)  —— 目标可带 "title"；不支持嵌套方括号（文档实测无此形态）
INLINE_LINK = re.compile(r"\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+[\"'][^\"']*[\"'])?\s*\)")
# 自动链接 <scheme:path> 或 <relative/path>
AUTOLINK = re.compile(r"<([^<>\s]+)>")

FENCE = re.compile(r"^\s*(```|~~~)")

EXTERNAL_PREFIXES = ("http://", "https://", "mailto:", "ftp://", "file://", "data:")


def find_project_root(start: Path) -> Path:
    """向上找含 .git 的目录作为仓库根；找不到就用 tools/ 的上级。"""
    for base in [start] + list(start.parents):
        if (base / ".git").exists():
            return base
    return Path(__file__).resolve().parent.parent


def strip_fragment(target: str) -> str:
    return target.split("#", 1)[0]


def is_checkable(target: str) -> bool:
    if not target:
        return False
    low = target.lower()
    if any(low.startswith(p) for p in EXTERNAL_PREFIXES):
        return False
    if target.startswith("#"):
        return False
    # 纯邮箱 / 纯锚点 / 单字符噪声
    if len(target) < 2:
        return False
    return True


def iter_target_files(paths: list) -> list:
    files = []
    for raw in paths:
        p = Path(raw)
        if p.is_file():
            if p.suffix == ".md":
                files.append(p)
        elif p.is_dir():
            files.extend(sorted(p.rglob("*.md")))
        else:
            print(f"⚠️  路径不存在：{raw}", file=sys.stderr)
    return files


def collect_broken(files: list, root: Path) -> list:
    """返回 [(rel_file, lineno, target, via_file_resolved, via_root_resolved)]"""
    broken = []
    for f in files:
        try:
            lines = f.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError as exc:
            print(f"⚠️  读取失败 {f}: {exc}", file=sys.stderr)
            continue
        in_fence = False
        try:
            rel = f.resolve().relative_to(root)
        except ValueError:
            rel = f
        for lineno, line in enumerate(lines, 1):
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            targets = [m.group(1) for m in INLINE_LINK.finditer(line)]
            targets += [m.group(1) for m in AUTOLINK.finditer(line) if "/" in m.group(1)]
            for t in targets:
                if not is_checkable(t):
                    continue
                path_part = strip_fragment(t)
                if not path_part:
                    continue
                via_file = (f.parent / path_part).resolve()
                via_root = (root / path_part.lstrip("/")).resolve()
                if via_file.exists() or via_root.exists():
                    continue
                broken.append((rel, lineno, t, via_file, via_root))
    return broken


def render_report(broken: list, files: list) -> str:
    out = [
        "# notes-link-check 报告",
        f"# 扫描 md 文件数：{len(files)}    断链数：{len(broken)}",
        "# 格式：相对文件路径:行号<TAB>原始目标<TAB>按文件目录解析<TAB>按仓库根解析",
        "",
    ]
    for rel, lineno, t, via_file, via_root in broken:
        out.append(f"{rel}:{lineno}\t{t}\t{via_file}\t{via_root}")
    return "\n".join(out) + "\n"


# ===== 自测：正/反例夹具 =====

def do_self_test() -> int:
    ok = True
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / ".git").mkdir()
        docs = root / "docs"
        docs.mkdir()
        (docs / "a.md").write_text(
            "兄弟链接存在 [x](./b.md)\n"
            "父目录链接存在 [x](../docs/b.md)\n"
            "断链 [x](./missing.md)\n"
            "带行号锚点且存在 [x](./b.md#L78-L96)\n"
            "外链跳过 [x](https://example.com/nope.md)\n"
            "纯锚点跳过 [x](#section)\n"
            "仓库根形态 [x](docs/b.md)\n"
            "代码块内断链不检：\n"
            "```markdown\n"
            "[x](./in-fence-missing.md)\n"
            "```\n",
            encoding="utf-8",
        )
        (docs / "b.md").write_text("# b\n", encoding="utf-8")
        broken = collect_broken(sorted(root.rglob("*.md")), root)
        got = {t for _, _, t, _, _ in broken}
        expect = {"./missing.md"}
        forbid = {
            "./b.md",
            "../docs/b.md",
            "./b.md#L78-L96",
            "https://example.com/nope.md",
            "#section",
            "docs/b.md",
            "./in-fence-missing.md",
        }
        if got != expect:
            print(f"❌ FAIL 断链集合应为 {sorted(expect)}，实际 {sorted(got)}")
            ok = False
        if got & forbid:
            print(f"❌ FAIL 误报了不该检查的目标：{sorted(got & forbid)}")
            ok = False
    print("✅ PASS notes-link-check 自测通过" if ok else "❌ FAIL notes-link-check 自测失败")
    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Markdown 相对链接断链扫描（迁移前后集合对账用）",
        add_help=True,
    )
    parser.add_argument("paths", nargs="*", help="要扫描的目录或 .md 文件（相对仓库根）")
    parser.add_argument("--output", help="把报告写入该文件（同时 stdout 打印摘要）")
    parser.add_argument("--self-test", action="store_true", help="跑内置正/反例自测")
    args = parser.parse_args()

    if args.self_test:
        return do_self_test()
    if not args.paths:
        parser.print_usage(sys.stderr)
        print("错误：至少要指定一个扫描路径", file=sys.stderr)
        return 2

    root = find_project_root(Path(__file__).resolve().parent)
    files = iter_target_files(args.paths)
    if not files:
        print("错误：没有匹配到任何 .md 文件", file=sys.stderr)
        return 2

    broken = collect_broken(files, root)
    report = render_report(broken, files)
    if args.output:
        Path(args.output).write_text(report, encoding="utf-8")
        print(f"报告已写入 {args.output}（扫描 {len(files)} 个文件，断链 {len(broken)} 处）")
    else:
        print(report)
    print(f"扫描 md 文件数：{len(files)}    断链数：{len(broken)}")
    return 0 if not broken else 1


if __name__ == "__main__":
    sys.exit(main())
