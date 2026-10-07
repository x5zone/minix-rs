#!/usr/bin/env python3
"""GLM 审计 · 第 1 项内容对账（在 REVIEW-PROMPT 协议脚本基础上的扩展版）。

扩展点：
1. 旧路径集合与 manifest 路径集合做集合级相等断言（协议只对行数）。
2. 按 kind / rule / 新树三个维度统计分布，与执行日志声称值对读。
3. git 跟踪面与映射表对读：多出的跟踪文件（迁移后新增）与缺失的跟踪文件（应为 0）。
4. 三棵树磁盘文件里不属于映射表目标的"新增件"清单。
"""
import hashlib
import subprocess
import sys
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"

def sha(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""):
            h.update(c)
    return h.hexdigest()

def git(args: list[str]) -> str:
    return subprocess.run(["git", *args], cwd=R, capture_output=True, text=True, check=True).stdout

# ---- 载入 manifest（迁移前磁盘清单）----
pre_hash, pre_status = {}, {}
manifest_paths = []
disk_lines = (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()
header = disk_lines[0].split("\t")
print(f"manifest 表头: {header}")
for line in disk_lines[1:]:
    cols = line.split("\t")
    p, _size, _mt, d, st = cols[0], cols[1], cols[2], cols[3], cols[4]
    pre_hash[p] = d
    pre_status[p] = st
    manifest_paths.append(p)
print(f"manifest 行数（不含表头）: {len(manifest_paths)}")
from collections import Counter
print(f"manifest git_status 分布: {dict(Counter(pre_status.values()))}")

# ---- 载入 path-map ----
pm_old, pm_rows = [], []
pm_lines = (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()
print(f"path-map 表头: {pm_lines[0].split(chr(9))}")
for line in pm_lines[1:]:
    old, new, kind, rule = line.split("\t")
    pm_old.append(old)
    pm_rows.append((old, new, kind, rule))
print(f"path-map 行数（不含表头）: {len(pm_rows)}")

# ---- 1a 集合级对账 ----
set_manifest, set_pm = set(manifest_paths), set(pm_old)
only_m = sorted(set_manifest - set_pm)
only_p = sorted(set_pm - set_manifest)
print(f"仅在 manifest 不在 path-map: {len(only_m)}")
for p in only_m[:10]:
    print(f"  {p}")
print(f"仅在 path-map 不在 manifest: {len(only_p)}")
for p in only_p[:10]:
    print(f"  {p}")

# ---- 分布统计 ----
print(f"kind 分布: {dict(Counter(k for _, _, k, _ in pm_rows))}")
print(f"rule 分布: {dict(sorted(Counter(r for _, _, _, r in pm_rows).items()))}")
def top_of(new: str) -> str:
    if new.startswith("rewrite-notes/"): return "rewrite-notes"
    if new.startswith("redesign-notes/"): return "redesign-notes"
    if new.startswith("study-notes/"): return "study-notes"
    if new.startswith(".review/"): return ".review"
    return new.split("/")[0]
print(f"新树分布: {dict(Counter(top_of(n) for _, n, _, _ in pm_rows))}")

# ---- 1b 逐条内容对账 ----
ok = miss = bad = 0
bad_list = []
for old, new, kind, rule in pm_rows:
    f = R / new
    if not f.is_file():
        miss += 1
        bad_list.append(f"缺失 {kind} {rule} {old} -> {new}")
        continue
    if sha(f) == pre_hash[old]:
        ok += 1
    else:
        bad += 1
        bad_list.append(f"不一致 {kind} {rule} {old} -> {new}")
print(f"内容对账: 一致 {ok} / 缺失 {miss} / 不一致 {bad}（期望 2428 / 0 / 0）")
for b in bad_list[:20]:
    print(f"  {b}")

# ---- 跟踪面对账 ----
tracked = set(git(["ls-files", "rewrite-notes", "redesign-notes", "study-notes"]).splitlines())
tracked_archive = set(git(["ls-files", ".review/archive/notes-fork-syscall-rewrite-2026-09"]).splitlines())
tracked_all = tracked | tracked_archive
pm_new = {n for _, n, _, _ in pm_rows}
extra_tracked = sorted(tracked_all - pm_new)
missing_tracked = sorted(pm_new - tracked_all)
print(f"git 跟踪数（三树 + .review 归档区）: {len(tracked_all)}（期望 1102 + 迁移后新增）")
print(f"三树跟踪数（不含 .review 归档）: {len(tracked)}")
print(f"跟踪面中不属于映射表目标的迁移后新增件: {len(extra_tracked)}")
for p in extra_tracked:
    print(f"  新增: {p}")
print(f"映射表目标中不在跟踪面的: {len(missing_tracked)}（应全部是 kind=mv-ignored/mv-untracked）")
kinds_missing = Counter(k for o, n, k, r in pm_rows if n in set(missing_tracked))
print(f"  缺失件的 kind 分布: {dict(kinds_missing)}")
if missing_tracked and dict(kinds_missing).get("git-mv", 0) != 0:
    print("  !! git-mv 类目标居然不在跟踪面 —— 静默退跟踪未修复")
    for o, n, k, r in pm_rows:
        if n in missing_tracked and k == "git-mv":
            print(f"    {o} -> {n}")

# ---- 三棵树磁盘文件对映射表的增量 ----
disk_now = subprocess.run(
    ["find", "rewrite-notes", "redesign-notes", "study-notes", "-type", "f"],
    cwd=R, capture_output=True, text=True, check=True).stdout.splitlines()
disk_set = set(disk_now)
extra_disk = sorted(disk_set - pm_new)
pm_new_in_trees = {n for n in pm_new if top_of(n) in ("rewrite-notes", "redesign-notes", "study-notes")}
missing_disk = sorted(pm_new_in_trees - disk_set)
print(f"三棵树磁盘文件数: {len(disk_now)}")
print(f"磁盘上不属于映射表目标的文件: {len(extra_disk)}")
for p in extra_disk:
    st = "tracked" if p in tracked else "untracked/ignored"
    print(f"  {st}: {p}")
print(f"映射表目标（三树内）不在磁盘上的: {len(missing_disk)}")
for p in missing_disk[:10]:
    print(f"  {p}")
