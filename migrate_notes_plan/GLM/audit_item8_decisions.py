#!/usr/bin/env python3
"""GLM 审计 · 第 8 项：用户裁决执行对读（冻结域强验证 + 未跟踪件未入库 + 定性说明）。
"""
import hashlib
import subprocess
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
TAR = "tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz"

def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()

def git(args, text=True):
    r = subprocess.run(["git", *args], cwd=R, capture_output=True, text=text, check=True)
    return r.stdout

print("== 裁决 2/8.1: .review/ 冻结不改写 ==")
# .review 全量 tar↔磁盘哈希比对
listing = subprocess.run(["tar", "-tzf", TAR], cwd=R, capture_output=True, text=True, check=True).stdout
review_members = [l for l in listing.splitlines() if l.startswith(".review/") and not l.endswith("/")]
print(f".review tar 条目: {len(review_members)}")
# 批量解包到临时目录再比对（比逐条 -O 快）
import tempfile, shutil, os
tmpd = tempfile.mkdtemp(prefix="glm-review-check-")
try:
    subprocess.run(["tar", "-xzf", TAR, "-C", tmpd, ".review"], cwd=R, check=True)
    same = diff = missing = 0
    diff_list = []
    for m in review_members:
        src = Path(tmpd) / m
        dst = R / m
        if not dst.is_file():
            missing += 1
            diff_list.append(f"缺失: {m}")
            continue
        if sha(src.read_bytes()) == sha(dst.read_bytes()):
            same += 1
        else:
            diff += 1
            diff_list.append(f"不同: {m}")
    print(f".review 冻结比对: 相同 {same} / 不同 {diff} / 磁盘缺失 {missing}（期望 1728/0/0）")
    for d in diff_list[:5]:
        print(f"  {d}")
finally:
    shutil.rmtree(tmpd)
# 旧路径引用仍大量保留
n_hit = 0
for p in (R / ".review").rglob("*"):
    if p.is_file():
        try:
            if b"notes/rewrite" in p.read_bytes():
                n_hit += 1
        except OSError:
            pass
print(f".review 含旧路径的文件数（应仍很大，未被 sed）: {n_hit}")
arch = list((R / ".review/archive/notes-fork-syscall-rewrite-2026-09").rglob("*"))
arch_files = [a for a in arch if a.is_file()]
print(f".review/archive/notes-fork-syscall-rewrite-2026-09/ 文件数（期望 3）: {len(arch_files)}")
pm = R / ".review/PATH-MAPPING.md"
print(f".review/PATH-MAPPING.md 存在: {pm.is_file()}（{len(pm.read_bytes()) if pm.is_file() else 0} 字节）")

print("\n== 裁决 3/5: 未跟踪件未入库 + 在制件内容未动 ==")
untracked15 = ["migrate_notes_plan/agents-workflow-optim.md",
               "tools/atf-c-compat/probes/p7.c",
               "rewrite-notes/coordination/PENDING-DECISIONS-3ARCH-PARITY.md"]
for f in untracked15:
    tracked = git(["ls-files", "--", f]).strip()
    print(f"  {'!! 已被跟踪' if tracked else '未入库(正确)'}: {f}")
nlm = sorted(str(p.relative_to(R)) for p in (R / "new_laptop_migrate").rglob("*") if p.is_file())
tracked_nlm = [f for f in nlm if git(["ls-files", "--", f]).strip()]
print(f"  new_laptop_migrate/ 共 {len(nlm)} 件，被跟踪的（期望 0）: {tracked_nlm}")

print("\n== 裁决 6: study-notes 原样搬 + README 定性 ==")
sr = R / "study-notes/README.md"
print(f"study-notes/README.md 存在: {sr.is_file()}")
if sr.is_file():
    head = sr.read_text(encoding="utf-8", errors="replace")[:400]
    print("  开头 400 字:")
    for line in head.splitlines()[:6]:
        print(f"    {line}")

print("\n== 裁决 10: redesign-notes 按主题子目录 ==")
for d in ["architecture", "ipc", "fork", "vm"]:
    p = R / "redesign-notes" / d
    n = len(list(p.rglob("*"))) if p.is_dir() else -1
    print(f"  redesign-notes/{d}/: {'存在' if p.is_dir() else '!! 缺失'}（{n} 个条目）")

print("\n== 裁决 12: 其余冻结域旧路径保留计数（执行日志声称 .qoder=6 / AI-chats=10 / tmp=49 / new_laptop=71） ==")
for d in [".qoder", "AI-chats", "tmp", "new_laptop_migrate"]:
    base = R / d
    hits = 0
    if base.is_dir():
        for p in base.rglob("*"):
            if p.is_file():
                try:
                    data = p.read_bytes()
                    if b"\x00" not in data:
                        hits += data.count(b"notes/rewrite") + data.count(b"notes/study") + data.count(b"notes/redesign")
                except OSError:
                    pass
    print(f"  {d}/: {hits} 处")
q = R / ".qoder/specs/NK4C信号双形态落地_task-fc7.md"
print(f"  .qoder/specs 任务文件存在: {q.is_file()}")
