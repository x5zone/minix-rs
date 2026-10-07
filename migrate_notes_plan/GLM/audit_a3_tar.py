#!/usr/bin/env python3
"""GLM 审计 · A3：tar 卷与 manifest 的集合级对账 + 内容抽样。

两个独立载体（tar 快照、manifest 清单）互相印证"迁移前全貌"：
1. notes/ 文件条目集合 == manifest 路径集合（双向差应为 0）。
2. 抽样条目从 tar 流式解出后 sha256 == manifest 记录值。
   抽样覆盖：随机 30 个 + 615MB 串口日志 + 在制两件 + 每类代表（.design/.log/.bak/.backup/.review 内嵌）。
3. 冻结未跟踪件（new_laptop_migrate、agents-workflow-optim.md、p7.c）：tar 解出的 sha256
   既对 00-SNAPSHOT §5.2 记录的前 12 位（有记录的），也对当前磁盘文件（冻结域不应变化）。
"""
import hashlib
import random
import subprocess
import sys
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
TAR = "tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz"
PRE = R / "migrate_notes_plan/pre-migrate-20261007"

def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()

def tar_file_entries() -> list[str]:
    out = subprocess.run(["tar", "-tzf", TAR], cwd=R, capture_output=True, text=True, check=True).stdout
    return [l for l in out.splitlines() if l and not l.endswith("/")]

def tar_extract(member: str) -> bytes:
    r = subprocess.run(["tar", "-xzf", TAR, "-O", member], cwd=R, capture_output=True, check=True)
    return r.stdout

manifest = {}
for line in (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    cols = line.split("\t")
    manifest[cols[0]] = (cols[1], cols[3], cols[4])  # size, sha256, git_status

entries = tar_file_entries()
notes_entries = [e for e in entries if e.startswith("notes/")]
print(f"tar 文件条目 {len(entries)}，其中 notes/ {len(notes_entries)}，manifest {len(manifest)}")

mset, tset = set(manifest.keys()), set(notes_entries)
only_m = sorted(mset - tset)
only_t = sorted(tset - mset)
print(f"仅在 manifest 不在 tar: {len(only_m)}")
for p in only_m[:10]:
    print(f"  {p}")
print(f"仅在 tar 不在 manifest: {len(only_t)}")
for p in only_t[:10]:
    print(f"  {p}")

# ---- 抽样 ----
rng = random.Random(20261007)
sample = set(rng.sample(notes_entries, 30))
# 大日志与在制件、每类代表
must = ["notes/rewrite/fork-syscall-rewrite/evidence/20260922-nk4a-iter11-18/serial_c9a.log",
        "notes/rewrite/fork-syscall-rewrite/TODO-3ARCH-PARITY-20261006.md",
        "notes/rewrite/fork-syscall-rewrite/PENDING-DECISIONS-3ARCH-PARITY.md",
        "notes/rewrite/fork-syscall-rewrite/01-stage-kernel/.design/01-design.v1.md",
        "notes/rewrite/fork-syscall-rewrite/evidence/20260922-nk4b-p3-m34/virt-gic3.dtb",
        "notes/rewrite/archive_bak/fork-rewr-01.md.bak",
        "notes/redesign/improve_minix.md.backup",
        "notes/rewrite/fork-syscall-rewrite/.review/STATE.md"]
sample.update(m for m in must if m in tset)
missing_must = [m for m in must if m not in tset]
print(f"指定代表件不在 tar 的（期望 0）: {missing_must}")

ok = bad = 0
for m in sorted(sample):
    data = tar_extract(m)
    rec_size, rec_sha, _st = manifest[m]
    if sha(data) == rec_sha and str(len(data)) == rec_size:
        ok += 1
    else:
        bad += 1
        print(f"  !! 抽样不一致: {m}")
print(f"抽样内容对账: {ok} OK / {bad} 不一致 / 共 {len(sample)}（含 615MB 串口日志与在制两件）")

# ---- 冻结未跟踪件：tar vs 磁盘 vs 记录前缀 ----
frozen = ["migrate_notes_plan/agents-workflow-optim.md",
          "tools/atf-c-compat/probes/p7.c",
          "new_laptop_migrate/diseased-scan-20261007/commits.tsv",
          "new_laptop_migrate/diseased-scan-20261007/paths.tsv",
          "new_laptop_migrate/diseased-scan-20261007/summary.json"]
prefixes = {"migrate_notes_plan/agents-workflow-optim.md": "04485db7f180",
            "tools/atf-c-compat/probes/p7.c": "b8da7f7b726",
            "new_laptop_migrate/diseased-scan-20261007/commits.tsv": "e0503c6b9c29",
            "new_laptop_migrate/diseased-scan-20261007/paths.tsv": "bc450e9032e3",
            "new_laptop_migrate/diseased-scan-20261007/summary.json": "92f7ae146315"}
for f in frozen:
    t = sha(tar_extract(f))
    d = sha((R / f).read_bytes())
    rec = prefixes.get(f, "")
    line = f"{f}\n  tar={t[:16]} 磁盘={d[:16]} 记录前缀={rec}"
    verdict = []
    if t == d:
        verdict.append("tar==磁盘")
    else:
        verdict.append("!! tar!=磁盘")
    if rec and t.startswith(rec):
        verdict.append("前缀吻合")
    elif rec:
        verdict.append("!! 前缀不符")
    print(line + "  [" + ", ".join(verdict) + "]")
