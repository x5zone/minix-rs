#!/usr/bin/env python3
"""独立审计脚本 1：内容完整性 + 映射合同一致性（DS 审阅方自写，非复用迁移方脚本）。

对账口径：
  1. 按 path-map.tsv 逐条比对「旧路径迁前 sha256」与「新路径当前 sha256」。
  2. 映射类型 kind 与目标文件当前 git 状态（tracked/ignored/untracked）一致性。
  3. 三棵新树里不在映射表内的文件（迁移期新增文件）逐条列出。
  4. 映射表内的目标路径在磁盘上缺失的逐条列出。
"""
import hashlib
import subprocess
import sys
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"
TREES = ["rewrite-notes", "redesign-notes", "study-notes"]


def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""):
            h.update(c)
    return h.hexdigest()


def sh(*args):
    return subprocess.run(args, cwd=R, capture_output=True, text=True).stdout.splitlines()


# 载入 manifest：旧路径 -> (sha256, git_status)
manifest = {}
for line in (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    parts = line.split("\t")
    manifest[parts[0]] = (parts[3], parts[4])

# 载入 path-map：旧路径 -> (新路径, kind, rule)
pathmap = []
for line in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    old, new, kind, rule = line.split("\t")
    pathmap.append((old, new, kind, rule))

print(f"manifest 条目 {len(manifest)}，path-map 条目 {len(pathmap)}")

# 唯一性
olds = [p[0] for p in pathmap]
news = [p[1] for p in pathmap]
assert len(set(olds)) == len(olds), "旧路径有重复"
assert len(set(news)) == len(news), "新路径有重复"
print(f"唯一性：旧路径 {len(set(olds))} 条，新路径 {len(set(news))} 条，无重复 ✓")
assert set(olds) == set(manifest), "映射表源集合与清单集合不相等"

# 1. 逐条 sha256
ok = miss = bad = 0
for old, new, kind, rule in pathmap:
    f = R / new
    if not f.is_file():
        miss += 1
        print("  缺失:", old, "→", new)
        continue
    if sha(f) == manifest[old][0]:
        ok += 1
    else:
        bad += 1
        print("  不一致:", old, "→", new)
print(f"逐条对账：一致 {ok} / 缺失 {miss} / 不一致 {bad}（期望 2428/0/0）")

# 2. kind 与当前 git 状态一致性
tracked = set(sh("git", "ls-files", *TREES))
ignored = set(sh("git", "ls-files", "--others", "--ignored", "--exclude-standard", "--", *TREES))
untracked = set(
    ln[3:]
    for ln in sh("git", "status", "--porcelain", "-uall", "--", *TREES)
    if ln.startswith("?? ")
)
kind_expect = {"git-mv": tracked, "mv-ignored": ignored, "mv-untracked": untracked}
mismatch = []
for old, new, kind, rule in pathmap:
    if new.startswith(".review/"):
        continue  # .review 目录整域被忽略，状态判定另行处理
    pool = kind_expect[kind]
    if kind in ("git-mv", "mv-ignored") and new not in pool:
        mismatch.append((kind, new))
    if kind == "mv-untracked" and new not in pool:
        mismatch.append((kind, new))
print(f"kind 与 git 状态不一致条目：{len(mismatch)}")
for m in mismatch[:20]:
    print("    ", m)

# 3. 三棵树里不在映射表内的文件（迁移期新增）
tree_files = set()
for t in TREES:
    for p in (R / t).rglob("*"):
        if p.is_file():
            tree_files.add(str(p.relative_to(R)))
mapped_targets = {p[1] for p in pathmap}
extra = sorted(tree_files - mapped_targets)
print(f"三棵树磁盘文件 {len(tree_files)}；不在映射表内的新增文件 {len(extra)}：")
for e in extra:
    print("    +", e)

# 4. .review 归档区
arch = sorted(p for p in mapped_targets if p.startswith(".review/"))
print(f"映射到 .review 的文件 {len(arch)}：")
for a in arch:
    exists = "存在" if (R / a).is_file() else "缺失"
    print(f"    {a}  [{exists}]")

# 5. 合同未事后改动：path-map.tsv 与执行日志记录的 sha256 比对
claim = "c43f82c5977b2892c4f181c3b731ceda4e899142080152438cfc5b1888ca89f1"
actual = sha(PRE / "path-map.tsv")
print(f"path-map.tsv sha256 = {actual}")
print(f"与执行日志记录值一致：{actual == claim}")

sys.exit(0 if (miss == 0 and bad == 0 and not mismatch) else 1)
