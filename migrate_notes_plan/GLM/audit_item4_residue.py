#!/usr/bin/env python3
"""GLM 审计 · 第 4 项：必改域旧路径残留扫描（python3 字节级，规避 ugrep 对无效 UTF-8 的静默吞输出）。

复刻 REVIEW-PROMPT 第 4 项的域与排除目录，另外输出冻结区的保留计数做对照。
patterns = notes/rewrite | notes/study | notes/redesign（裸名 fork-syscall-rewrite 按裁决不在必改范围）。
"""
import sys
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PATTERNS = [b"notes/rewrite", b"notes/study", b"notes/redesign"]
DOMAINS = ["rewrite-notes", "redesign-notes", "study-notes", "os", "tools", "prompt",
           ".claude", ".codex", ".trae"]
ROOT_FILES = ["CLAUDE.md", "AGENTS.md", "README.md"]
EXCLUDE_DIRS = {".design", "evidence", "legacy-fork-bak", "target", "target_smp",
                ".review", ".git", "minix3", ".cargo-shared", ".dockercargo", "__pycache__"}
WHITELIST_FILES = {"rewrite-notes/MIGRATION.md", ".claude/settings.local.json"}

def is_binary(data: bytes) -> bool:
    return b"\x00" in data

hits = {}  # relpath -> {pattern: count}
total = 0
def scan_file(p: Path, rel: str):
    global total
    try:
        data = p.read_bytes()
    except OSError as e:
        print(f"  [不可读] {rel}: {e}")
        return
    if is_binary(data):
        return
    n = 0
    detail = []
    for i, line in enumerate(data.split(b"\n"), 1):
        for pat in PATTERNS:
            c = line.count(pat)
            if c:
                n += c
                detail.append((i, pat.decode(), c, line[:120]))
    if n:
        hits[rel] = (n, detail)
        total += n

for d in DOMAINS:
    base = R / d
    if not base.exists():
        print(f"[域缺失] {d}")
        continue
    for p in base.rglob("*"):
        if not p.is_file():
            continue
        rel = str(p.relative_to(R))
        if any(part in EXCLUDE_DIRS for part in p.parts):
            continue
        scan_file(p, rel)
for f in ROOT_FILES:
    scan_file(R / f, f)

print(f"\n必改域旧路径总命中: {total} 处，分布在 {len(hits)} 个文件")
wl_total = 0
residual = {}
for rel, (n, detail) in sorted(hits.items()):
    if rel in WHITELIST_FILES:
        wl_total += n
        print(f"  [白名单] {rel}: {n} 处")
    else:
        residual[rel] = (n, detail)
print(f"白名单文件命中合计: {wl_total}（期望 MIGRATION.md + settings.local.json）")
print(f"白名单外残留: {sum(n for n, _ in residual.values())} 处，{len(residual)} 个文件（期望 ≤3：notes-layout.conf/CLAUDE.md/AGENTS.md 各 1 处描述退役本身）")
for rel, (n, detail) in sorted(residual.items()):
    print(f"  {rel}: {n} 处")
    for i, pat, c, line in detail[:3]:
        try:
            text = line.decode("utf-8")
        except UnicodeDecodeError:
            text = repr(line)
        print(f"    L{i}: {text}")

# ---- 冻结区保留计数（内容不改写 => 旧路径必须还在）----
print("\n冻结区旧路径保留计数（对照执行日志声称 evidence=4 / legacy-fork-bak=8 / .design=56）:")
for d in ["rewrite-notes/evidence", "rewrite-notes/archive/legacy-fork-bak"]:
    base = R / d
    cnt = 0
    files = 0
    for p in base.rglob("*"):
        if p.is_file() and not is_binary(p.read_bytes()):
            files += 1
            if any(pat in p.read_bytes() for pat in PATTERNS):
                cnt += 1
    print(f"  {d}: {cnt} 个文件含旧路径（{files} 个文本文件）")
design_cnt = 0
for p in (R / "rewrite-notes").rglob(".design"):
    if p.is_dir():
        for f in p.rglob("*"):
            if f.is_file():
                data = f.read_bytes()
                if not is_binary(data) and any(pat in data for pat in PATTERNS):
                    design_cnt += 1
print(f"  rewrite-notes/**/.design/: {design_cnt} 个文件含旧路径")
