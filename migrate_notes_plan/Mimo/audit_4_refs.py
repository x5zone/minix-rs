#!/usr/bin/env python3
"""独立审计 item4：必改域旧路径残留扫描（字节级，不依赖 grep 实现）。

判据来自 REVIEW-PROMPT 第 4 项：必改域 = 三棵树 + os/ + tools/ + prompt/ + .claude/ + .codex/
+ .trae/ + CLAUDE.md + AGENTS.md + 根 README.md；白名单 = rewrite-notes/MIGRATION.md、
.claude/settings.local.json；冻结域（允许保留旧路径原文）= rewrite-notes/evidence、
rewrite-notes/archive/legacy-fork-bak、任何 .design/、.review/、migrate_notes_plan/、tmp/ 等。
只读脚本。
"""
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

R = Path(__file__).resolve().parents[2]
PAT = re.compile(rb"notes/(rewrite|study|redesign)")

MUST = ["rewrite-notes", "redesign-notes", "study-notes", "os", "tools", "prompt",
        ".claude", ".codex", ".trae", "CLAUDE.md", "AGENTS.md", "README.md"]
ALLOW = {
    "rewrite-notes/MIGRATION.md",
    ".claude/settings.local.json",
}
# 冻结区：命中只作统计，不算残留
FROZEN_DIR_PARTS = ("/evidence/", "/.design/", "/archive/legacy-fork-bak/", "/.review/",
                    "/migrate_notes_plan/", "/tmp/", "/AI-chats/", "/new_laptop_migrate/",
                    "/.qoder/", "/minix3/", "/target/", "/__pycache__/")
SKIP_DIR_NAMES = {".git", "target", "target_smp", "__pycache__", ".dockercargo",
                  ".cargo-shared", "minix3", "node_modules"}


def is_frozen(rel: str) -> bool:
    p = "/" + rel.replace("\\", "/") + "/"
    return any(seg in p for seg in FROZEN_DIR_PARTS)


def scan_file(p: Path) -> list:
    try:
        data = p.read_bytes()
    except OSError:
        return []
    hits = []
    for i, line in enumerate(data.split(b"\n"), 1):
        if PAT.search(line):
            hits.append((i, line.decode("utf-8", "replace")[:200]))
    return hits


def main() -> int:
    residue = Counter()
    residue_detail = defaultdict(list)
    frozen_hits = Counter()
    total_files = 0
    for m in MUST:
        base = R / m
        files = []
        if base.is_file():
            files = [base]
        elif base.is_dir():
            for p in base.rglob("*"):
                if p.is_file() and not (SKIP_DIR_NAMES & set(p.parts)):
                    files.append(p)
        for p in files:
            rel = str(p.relative_to(R))
            total_files += 1
            hits = scan_file(p)
            if not hits:
                continue
            if rel in ALLOW:
                residue["whitelist:" + rel] += len(hits)
            elif is_frozen(rel):
                frozen_hits[rel.split("/")[0] + ("/" + rel.split("/")[1] if "/" in rel else "")] += len(hits)
            else:
                residue[rel] += len(hits)
                residue_detail[rel] = hits

    print(f"scanned files in must-change domain: {total_files}")
    print("\n== 白名单命中（刻意保留） ==")
    for k, v in sorted(residue.items()):
        if k.startswith("whitelist:"):
            print(f"  {v:5d}  {k}")
    print("\n== 冻结域命中（按顶层目录汇总，不算残留） ==")
    for k, v in sorted(frozen_hits.items(), key=lambda x: -x[1]):
        print(f"  {v:5d}  {k}")
    real = {k: v for k, v in residue.items() if not k.startswith("whitelist:")}
    print(f"\n== 必改域残留（判据：应为 0）==  文件数={len(real)} 命中={sum(real.values())}")
    out = Path(__file__).with_name("evidence") / "item4-residue.txt"
    with open(out, "w", encoding="utf-8") as f:
        for k, v in sorted(real.items(), key=lambda x: -x[1]):
            f.write(f"### {k}  hits={v}\n")
            for ln, txt in residue_detail[k]:
                f.write(f"  {ln}: {txt}\n")
    print(f"detail -> {out}")
    return 0 if not real else 1


if __name__ == "__main__":
    sys.exit(main())
