#!/usr/bin/env python3
"""独立审计脚本 2：对 333 处内容差异做归因分类。

方法：
  A. 按 kind 分组，并对每个差异文件取 迁前 blob（pre-tag 旧路径）与 迁移后 blob（HEAD 新路径）的逐行 diff。
  B. 对 diff 的每一对「删除行 / 新增行」尝试合同变换（path-map 派生的旧前缀→新前缀最长优先替换，
     含文件级改名与目录级前缀），若变换后逐行相等，则该差异判为「机械前缀替换」。
  C. 剩余无法由前缀替换解释的差异，输出逐行明细供人工判定（预期来源：相对链接重定基 138 处、
     Phase 7 入口文档重写、以及迁移自述的其它编辑）。
"""
import subprocess
import sys
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"


def sh(*a):
    r = subprocess.run(a, cwd=R, capture_output=True, text=True)
    return r.stdout


def blob(ref, path):
    r = subprocess.run(["git", "show", f"{ref}:{path}"], cwd=R, capture_output=True)
    return r.stdout if r.returncode == 0 else None


rows = [l.split("\t") for l in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]]
manifest = {}
for l in (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    p = l.split("\t")
    manifest[p[0]] = (p[3], p[4])

import hashlib
def sha(b):
    return hashlib.sha256(b).hexdigest()

# 派生替换对：文件级（整路径）+ 目录级前缀
pairs = set()
for old, new, kind, rule in rows:
    pairs.add((old, new))
    # 逐级共同后缀推出目录前缀对
    o, n = old.split("/"), new.split("/")
    i = 0
    while i < len(o) and i < len(n) and o[-1 - i] == n[-1 - i]:
        i += 1
    if i:
        pairs.add(("/".join(o[:-i]) if i < len(o) else "", "/".join(n[:-i]) if i < len(n) else ""))
pairs = {(a, b) for a, b in pairs if a}
# 最长优先
pairs = sorted(pairs, key=lambda x: -len(x[0]))
print(f"派生替换对 {len(pairs)} 组（目录级 + 文件级）")

def transform(text: str) -> str:
    for a, b in pairs:
        text = text.replace(a, b)
    return text

# 工作树 vs HEAD 差异限制：只对 tracked 使用 git blob；ignored 用磁盘与 tar 比较
mismatch = []
for old, new, kind, rule in rows:
    f = R / new
    if not f.is_file():
        continue
    cur = f.read_bytes()
    if sha(cur) == manifest[old][0]:
        continue
    mismatch.append((old, new, kind))

print(f"差异文件 {len(mismatch)} 条；按 kind 分布：")
from collections import Counter
print("   ", Counter(k for _, _, k in mismatch))

mech, resid = [], []
for old, new, kind in mismatch:
    before = blob("notes/pre-migrate-20261007", old)
    after = blob("HEAD", new)
    if before is None or after is None:
        resid.append((old, new, kind, "blob 不可得（ignored/untracked 需 tar 对读）"))
        continue
    if transform(before.decode("utf-8", "surrogateescape")) == after.decode("utf-8", "surrogateescape"):
        mech.append((old, new, kind))
    else:
        resid.append((old, new, kind, "有残差"))

print(f"机械前缀替换可完全解释：{len(mech)} 条")
print(f"有残差：{len(resid)} 条")

# 残差逐文件输出 diff（截断）
out = []
for old, new, kind, why in resid:
    if why.startswith("blob"):
        out.append(f"===== {kind} {old} -> {new} ({why}) =====")
        continue
    before = blob("notes/pre-migrate-20261007", old).decode("utf-8", "surrogateescape")
    after = blob("HEAD", new).decode("utf-8", "surrogateescape")
    import difflib
    d = list(difflib.unified_diff(before.splitlines(), after.splitlines(), lineterm="", n=1))
    changed = [l for l in d if (l.startswith("+") or l.startswith("-")) and not l.startswith(("+++", "---"))]
    out.append(f"===== {kind} {old} -> {new} ：{len(changed)} 行变化 =====")
    for l in changed[:40]:
        out.append("  " + l[:300])
    if len(changed) > 40:
        out.append(f"  …（余 {len(changed)-40} 行）")

(R / "migrate_notes_plan/DS/evidence/item1d-residual-diffs.txt").write_text("\n".join(out), encoding="utf-8")
print("残差明细已写入 evidence/item1d-residual-diffs.txt")

# 残差文件清单落盘（供后续逐条定性）
with open(R / "migrate_notes_plan/DS/evidence/item1d-residual-list.tsv", "w", encoding="utf-8") as fh:
    for old, new, kind, why in resid:
        fh.write(f"{old}\t{new}\t{kind}\t{why}\n")
with open(R / "migrate_notes_plan/DS/evidence/item1d-mech-list.tsv", "w", encoding="utf-8") as fh:
    for old, new, kind in mech:
        fh.write(f"{old}\t{new}\t{kind}\n")
