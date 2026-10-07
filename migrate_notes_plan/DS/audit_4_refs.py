#!/usr/bin/env python3
"""独立审计脚本 5：必改域旧路径残留（字节级，绕开 ugrep 对无效 UTF-8 静默吞输出的问题）。

扫描模式：notes/rewrite、notes/study、notes/redesign、fork-syscall-rewrite（裸名另计）。
排除目录：与原 prompt 一致（evidence、.design、legacy-fork-bak、target、target_smp、.review、
.git、minix3、.cargo-shared、.dockercargo、__pycache__）。
输出：逐条 file:line:pattern，并按文件汇总；再单独统计三个冻结区的命中文件数（应与基线一致）。
"""
import os
import sys
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PATTERNS = [b"notes/rewrite", b"notes/study", b"notes/redesign"]
BARE = b"fork-syscall-rewrite"

DOMAIN = ["rewrite-notes", "redesign-notes", "study-notes", "os", "tools", "prompt",
          ".claude", ".codex", ".trae", "CLAUDE.md", "AGENTS.md", "README.md"]
EXCLUDE_DIRS = {"evidence", ".design", "legacy-fork-bak", "target", "target_smp", ".review",
                ".git", "minix3", ".cargo-shared", ".dockercargo", "__pycache__"}

hits = {}          # file -> [(line, pattern)]
bare_hits = {}     # 裸名命中（单独统计）

for top in DOMAIN:
    p = R / top
    if p.is_file():
        files = [p]
    else:
        files = []
        for dirpath, dirnames, filenames in os.walk(p):
            dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIRS]
            files.extend(Path(dirpath) / f for f in filenames)
    for f in files:
        try:
            data = f.read_bytes()
        except Exception:
            continue
        rel = str(f.relative_to(R))
        lines = data.split(b"\n")
        for i, ln in enumerate(lines, 1):
            for pat in PATTERNS:
                if pat in ln:
                    hits.setdefault(rel, []).append((i, pat.decode(), ln[:200].decode("utf-8", "replace")))
            if BARE in ln and not any(x in ln for x in PATTERNS):
                bare_hits.setdefault(rel, []).append((i, ln[:200].decode("utf-8", "replace")))

print(f"必改域命中文件数（含两个白名单）: {len(hits)}")
total = sum(len(v) for v in hits.values())
print(f"命中行数: {total}")
for f in sorted(hits):
    print(f"  {f}: {len(hits[f])} 行")
    for i, pat, txt in hits[f][:3]:
        print(f"      L{i} [{pat}] {txt.strip()[:150]}")
    if len(hits[f]) > 3:
        print(f"      …（余 {len(hits[f]) - 3} 行）")

print()
print(f"裸名 fork-syscall-rewrite 命中（不含路径形态）文件数: {len(bare_hits)}，行数: {sum(len(v) for v in bare_hits.values())}")
for f in sorted(bare_hits)[:5]:
    print(f"  {f}: {len(bare_hits[f])} 行")

# 冻结区命中文件数（与执行日志声称的基线：evidence 4、legacy-fork-bak 8、.design 56）
print()
FROZEN = {
    "rewrite-notes/evidence": "evidence",
    "rewrite-notes/archive/legacy-fork-bak": "legacy-fork-bak",
}
for sub, name in FROZEN.items():
    p = R / sub
    n = 0
    for dirpath, dirnames, filenames in os.walk(p):
        for f in filenames:
            try:
                if b"notes/rewrite" in (Path(dirpath) / f).read_bytes():
                    n += 1
            except Exception:
                pass
    print(f"冻结区 {name}: {n} 个文件含旧路径（基线 evidence=4、legacy-fork-bak=8）")

n = 0
for dirpath, dirnames, filenames in os.walk(R):
    if ".design" in Path(dirpath).parts:
        for f in filenames:
            try:
                if b"notes/rewrite" in (Path(dirpath) / f).read_bytes():
                    n += 1
            except Exception:
                pass
nd = sum(1 for dirpath, _, files in os.walk(R) if ".design" in Path(dirpath).parts for f in files
         if b"notes/rewrite" in (Path(dirpath) / f).read_bytes() if True) if False else None
print(f"各 .design 快照区: {n} 个文件含旧路径（基线 56）")
