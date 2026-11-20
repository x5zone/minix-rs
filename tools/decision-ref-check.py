#!/usr/bin/env python3
"""decision-ref-check.py — 判决账编号唯一性与引用存在性门（V3.6-r6）

背景（三次真实事故）：
  - 两个并发会话各自分配 D-061，双行并存一个 commit；
  - 顺延产生物理顺序倒挂（D-061 → D-063 → D-062）；
  - D06 的 `OPEN-DEBT.md` 引用 D-077，而该编号在提交时点尚不存在（两小时后被
    另一条裁决占用），同篇其余 13 处写 D-076——同一延期批两个号。

本工具做三件事：
  1. **唯一性**：DECISIONS.md 里的 `D-NNN` 不得重复（表格行 + `## D-NNN` 标题两种形态）。
  2. **连续性**：编号缺口只报 WARN（历史上 D-059 是真实空缺）；显式登记在
     DECISIONS.md 的「已知空缺」段里的编号不再报。
  3. **引用存在性**：扫描给定路径下的 `.md` 文件，凡出现 `D-NNN` 而账本无此编号 →
     FAIL。**允许标记豁免**：同一行含 `[D-REF-OK]` 视为「在谈论编号本身而不是引用它」。

用法：
    python3 tools/decision-ref-check.py                     # 仓库默认范围
    python3 tools/decision-ref-check.py --scan DIR [DIR...]
    python3 tools/decision-ref-check.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DECISIONS = ROOT / "rewrite-notes/coordination/review-eval/DECISIONS.md"
DEFAULT_SCAN = (
    ROOT / "rewrite-notes/coordination/review-eval/runs",
    ROOT / "prompt",
)
ROW_RE = re.compile(r"^\|\s*D-(\d{3})")
HEAD_RE = re.compile(r"^##\s*D-(\d{3})")
REF_RE = re.compile(r"\bD-(\d{3})\b")
KNOWN_GAP_RE = re.compile(r"已知空缺[^\n]*", re.MULTILINE)


def load_ids(decisions: Path) -> tuple[set[int], list[int], set[int]]:
    """返回 (全部 id, 重复 id, 已知空缺 id)。"""
    text = decisions.read_text(encoding="utf-8")
    ids: list[int] = []
    for line in text.splitlines():
        m = ROW_RE.match(line) or HEAD_RE.match(line)
        if m:
            ids.append(int(m.group(1)))
    seen: set[int] = set()
    dups: list[int] = []
    for n in ids:
        if n in seen:
            dups.append(n)
        seen.add(n)
    gaps: set[int] = set()
    for m in KNOWN_GAP_RE.finditer(text):
        gaps |= {int(x) for x in re.findall(r"D-(\d{3})", m.group(0))}
    return seen, dups, gaps


def _tracked_files() -> set[Path] | None:
    """返回 git 跟踪的 .md 绝对路径集合；不在 git 仓库或 git 不可用时返回 None。

    r6-F04：默认扫描域与 `lint-review-rules.sh` 一致——只查**入库**规则内容，
    未跟踪的报告（如会话产物）不得让门变红。
    """
    try:
        import subprocess

        out = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "--", "*.md"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except Exception:
        return None
    return {(ROOT / line).resolve() for line in out.splitlines() if line}


def _rel(f: Path) -> str:
    try:
        return str(f.relative_to(ROOT))
    except ValueError:
        return str(f)


def iter_md(paths: list[Path], respect_tracked: bool = True):
    tracked = _tracked_files() if respect_tracked else None
    for p in paths:
        if p.is_file() and p.suffix == ".md":
            if tracked is None or p.resolve() in tracked:
                yield p
        elif p.is_dir():
            for f in sorted(p.rglob("*.md")):
                if tracked is None or f.resolve() in tracked:
                    yield f


def check(
    decisions: Path,
    scan: list[Path],
    report: list[dict],
    respect_tracked: bool = True,
) -> int:
    ids, dups, known_gaps = load_ids(decisions)
    if not ids:
        report.append({"level": "FAIL", "msg": f"未从 {decisions} 解析到任何 D-NNN 编号"})
        return 1
    for n in sorted(set(dups)):
        report.append({"level": "FAIL", "msg": f"D-{n:03d} 重复出现 {dups.count(n)} 次（唯一性）"})
    max_id = max(ids)
    missing = [n for n in range(1, max_id + 1) if n not in ids and n not in known_gaps]
    if missing:
        report.append({
            "level": "WARN",
            "msg": "编号缺口（若非有意，请在 DECISIONS.md 的『已知空缺』段登记）："
                   + "、".join(f"D-{n:03d}" for n in missing),
        })
    dangling = 0
    for f in iter_md(scan, respect_tracked):
        try:
            lines = f.read_text(encoding="utf-8").splitlines()
        except Exception:
            continue
        in_fence = False
        for i, line in enumerate(lines, 1):
            s = line.strip()
            if s.startswith("```") or s.startswith("~~~"):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            for m in REF_RE.finditer(line):
                n = int(m.group(1))
                # r6-F03：豁免必须**指名**该编号（`D-059[D-REF-OK]` 或
                # `[D-REF-OK: D-059]`），不再整行豁免——否则一行里混入的
                # 真悬空引用会被同行的标记一起放过。
                if re.search(rf"D-0*{n}[^\w]{{0,4}}\[D-REF-OK\]|\[D-REF-OK:\s*D-0*{n}\]", line):
                    continue
                if n not in ids:
                    dangling += 1
                    report.append({
                        "level": "FAIL",
                        "msg": f"{_rel(f)}:{i} 引用不存在的 D-{n:03d}",
                    })
    if not any(r["level"] == "FAIL" for r in report):
        report.append({
            "level": "PASS",
            "msg": f"{len(ids)} 个编号无重复；扫描范围内 0 处悬空引用",
        })
    return 0 if not any(r["level"] == "FAIL" for r in report) else 1


def _write(p: Path, text: str) -> None:
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")


def self_test() -> int:
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        dec = root / "DECISIONS.md"
        _write(dec, "## D-001 甲\n\n| D-002（x） | a |\n|---|---|\n| D-003（y） | b |\n")
        scan = root / "scan"
        _write(scan / "ok.md", "见 D-002 与 D-003。\n")
        rep: list[dict] = []
        if check(dec, [scan], rep, respect_tracked=False) != 0:
            print("self-test FAIL: 合规样本被判红")
            return 1
        # 重复编号 → FAIL
        _write(dec, "## D-001 甲\n\n| D-001（dup） | a |\n|---|---|\n")
        rep = []
        if check(dec, [scan], rep, respect_tracked=False) == 0:
            print("self-test FAIL: 重复编号未被拦截")
            return 1
        # 悬空引用 → FAIL
        _write(dec, "## D-001 甲\n")
        rep = []
        if check(dec, [scan], rep, respect_tracked=False) == 0:
            print("self-test FAIL: 悬空引用未被拦截")
            return 1
        # 标记豁免 → PASS
        _write(scan / "ok.md", "历史空缺 D-059 [D-REF-OK]\n")
        rep = []
        if check(dec, [scan], rep, respect_tracked=False) != 0:
            print("self-test FAIL: [D-REF-OK] 豁免未生效")
            return 1
        # 已知空缺登记 → 不报缺口
        _write(dec, "## D-001 甲\n\n已知空缺：D-002\n")
        rep = []
        check(dec, [scan], rep, respect_tracked=False)
        if any("编号缺口" in r["msg"] for r in rep):
            print("self-test FAIL: 已登记的已知空缺仍报缺口")
            return 1
    print("self-test PASS: 唯一性/悬空引用/标记豁免/已知空缺 均正确")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="判决账编号唯一性与引用存在性门（V3.6-r6）")
    ap.add_argument("--decisions", default=str(DEFAULT_DECISIONS))
    ap.add_argument("--scan", nargs="*", help="扫描目录或文件（默认 runs + prompt）")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    scan = [Path(p) for p in args.scan] if args.scan else list(DEFAULT_SCAN)
    report: list[dict] = []
    rc = check(Path(args.decisions), scan, report, respect_tracked=not args.scan)
    for r in report:
        print(f"[{r['level']}] {r['msg']}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
