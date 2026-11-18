#!/usr/bin/env python3
"""panel-index.py — 面板制品索引与哈希校验（V3.5 制品链）

背景：V3.4-r8 的对抗轮证明，纯文本匹配门在「自报 + 未入库证据」模型下可被
伪造（空目录、假台账、假面板）。V3.5 换证据类型：面板原始输出**入库**到
`runs/<stage>/<doc>/panel/round<N>/`，并由本工具生成带 sha256 的 `INDEX.md`；
轨迹门（eval-trace-check.py）在 V3.4+ 文档的轮关闭时校验制品存在且哈希一致。

用法：
    python3 tools/panel-index.py <panel-dir>            # 生成/刷新 INDEX.md
    python3 tools/panel-index.py <panel-dir> --check    # 校验现状与索引一致
    python3 tools/panel-index.py --self-test

INDEX.md 是**生成物**：手写它会被 --check 的哈希校验当场发现。
"""
from __future__ import annotations

import argparse
import hashlib
import re
import sys
import tempfile
from pathlib import Path

FINDING_RE = re.compile(r"^##\s+FINDING\b", re.MULTILINE)
SUGG_RE = re.compile(
    r"^#{2,3}\s+SUGGESTION\s+[A-Za-z][A-Za-z0-9_-]*", re.MULTILINE
)  # V3.4-r12：与门同形（##/### + ASCII ID 含下划线）
CLAIM_RE = re.compile(r"^##\s+CLAIM\s+\d+", re.MULTILINE)
HEADER = "# 面板制品索引（panel-index，生成物——勿手改）"
TABLE_HEAD = "| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |"
TABLE_SEP = "|---|---|---|---|---|---|"


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _scan(panel_dir: Path) -> list[dict]:
    rows: list[dict] = []
    for f in sorted(panel_dir.glob("*.md")):
        if f.name == "INDEX.md":
            continue
        text = f.read_text(encoding="utf-8")
        nf = len(FINDING_RE.findall(text))
        ns = len(SUGG_RE.findall(text))
        nc = len(CLAIM_RE.findall(text))
        if nf or nc:
            shape = "finding"
        elif ns:
            shape = "suggestion-only"
        else:
            shape = "no-finding"
        rows.append(
            {
                "name": f.name,
                "sha256": _sha256(f),
                "findings": nf,
                "suggestions": ns,
                "claims": nc,
                "shape": shape,
            }
        )
    return rows


def write_index(panel_dir: Path) -> int:
    rows = _scan(panel_dir)
    if not rows:
        print(f"panel-index: {panel_dir} 下没有面板 .md 文件", file=sys.stderr)
        return 2
    lines = [HEADER, "", f"- 文件数: {len(rows)}", "", TABLE_HEAD, TABLE_SEP]
    for r in rows:
        lines.append(
            f"| {r['name']} | {r['sha256']} | {r['findings']} | "
            f"{r['suggestions']} | {r['claims']} | {r['shape']} |"
        )
    (panel_dir / "INDEX.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"panel-index: 写入 {panel_dir / 'INDEX.md'}（{len(rows)} 个文件）")
    return 0


def _parse_index(panel_dir: Path) -> dict[str, dict] | None:
    index = panel_dir / "INDEX.md"
    if not index.is_file():
        return None
    rows: dict[str, dict] = {}
    for line in index.read_text(encoding="utf-8").splitlines():
        m = re.match(
            r"^\|\s*([^|]+?)\s*\|\s*([0-9a-f]{64})\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*([a-z-]+)\s*\|$",
            line,
        )
        if m:
            rows[m.group(1)] = {
                "sha256": m.group(2),
                "findings": int(m.group(3)),
                "suggestions": int(m.group(4)),
                "claims": int(m.group(5)),
                "shape": m.group(6),
            }
    return rows


def check_index(panel_dir: Path) -> int:
    rows = _parse_index(panel_dir)
    if rows is None:
        print(f"panel-index: 缺少 {panel_dir / 'INDEX.md'}（先运行无 --check 生成）", file=sys.stderr)
        return 1
    current = {r["name"]: r for r in _scan(panel_dir)}
    errors = 0
    for name, rec in rows.items():
        if name not in current:
            print(f"[FAIL] {name}: 索引列出但文件不存在", file=sys.stderr)
            errors += 1
            continue
        cur = current[name]
        if cur["sha256"] != rec["sha256"]:
            print(f"[FAIL] {name}: sha256 与索引不一致（面板被改动而未重生成索引）", file=sys.stderr)
            errors += 1
        for key in ("findings", "suggestions", "claims"):
            if cur[key] != rec[key]:
                print(f"[FAIL] {name}: {key} 计数 {cur[key]} ≠ 索引 {rec[key]}", file=sys.stderr)
                errors += 1
    for name in current:
        if name not in rows:
            print(f"[FAIL] {name}: 文件存在但索引未列出（新增面板未入库索引）", file=sys.stderr)
            errors += 1
    if errors:
        print(f"panel-index: {errors} 处不一致", file=sys.stderr)
        return 1
    print(f"panel-index: {len(rows)} 个文件哈希与计数一致")
    return 0


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp) / "round1"
        d.mkdir()
        (d / "fact.md").write_text(
            "# FACT\n\n## FINDING FACT-01\n- 位置：§1\n", encoding="utf-8"
        )
        (d / "reader-a.md").write_text(
            "# READER-A\n\n## FINDING READER-A-01\n- 位置：§2\n", encoding="utf-8"
        )
        (d / "meta.md").write_text("# META\n\n无发现。\n", encoding="utf-8")
        if write_index(d) != 0:
            print("self-test FAIL: 索引生成失败")
            return 1
        if check_index(d) != 0:
            print("self-test FAIL: 生成后立即校验失败")
            return 1
        # 篡改面板 → 校验必须失败
        (d / "fact.md").write_text(
            "# FACT\n\n## FINDING FACT-01\n- 改过\n", encoding="utf-8"
        )
        if check_index(d) == 0:
            print("self-test FAIL: 面板篡改未被索引校验发现")
            return 1
        write_index(d)
        # 新增文件未入索引 → 校验必须失败
        (d / "reader-b.md").write_text("# B\n\n## FINDING READER-B-01\n", encoding="utf-8")
        if check_index(d) == 0:
            print("self-test FAIL: 新增未入库面板未被发现")
            return 1
        # no-finding 面板被登记为 no-finding
        rows = _parse_index(d)
        write_index(d)
        rows = _parse_index(d)
        if rows is None or rows["meta.md"]["shape"] != "no-finding":
            print("self-test FAIL: no-finding 形态登记错误")
            return 1
        if rows["fact.md"]["shape"] != "finding":
            print("self-test FAIL: finding 形态登记错误")
            return 1
    print("self-test PASS: 索引生成/校验/篡改检测/新增检测/no-finding 形态均正确")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="面板制品索引与哈希校验（V3.5 制品链）")
    parser.add_argument("panel_dir", nargs="?", help="面板目录（panel/round<N>/）")
    parser.add_argument("--check", action="store_true", help="只校验，不生成")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.panel_dir:
        parser.print_help()
        return 2
    d = Path(args.panel_dir)
    if not d.is_dir():
        print(f"panel 目录不存在: {d}", file=sys.stderr)
        return 2
    return check_index(d) if args.check else write_index(d)


if __name__ == "__main__":
    sys.exit(main())
