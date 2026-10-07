#!/usr/bin/env python3
"""独立审计脚本 6：断链集合对账（不依赖迁移方 compare-links.py 的「仅映射文件名」口径）。

方法：
  1. 迁移前基线（broken-links.before.txt）：每条记录 (旧文档路径, 行号, 目标文本)。
     把旧文档路径映射进新坐标；把「按旧文档目录解析出的绝对路径」也映射进新坐标
     （能映射到映射表内的路径就映射，否则原样保留），得到规范化键 (新文档, 解析坐标)。
  2. 迁移后（最新扫描 broken-links.now.txt，仅三棵树）同样规范化为 (新文档, 解析坐标)。
  3. 迁移引入的新断链 = 迁移后集合 − 迁移前映射集合。期望 0。
"""
import posixpath
import re
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"

rows = [l.split("\t") for l in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]]
old_to_new = {r[0]: r[1] for r in rows}
# 目录级前缀映射（最长优先）
from collections import Counter
support = Counter()
for old, new, kind, rule in rows:
    o, n = old.split("/"), new.split("/")
    i = 0
    while i < len(o) and i < len(n) and o[-1 - i] == n[-1 - i]:
        i += 1
    if i:
        support[("/".join(o[:-i]), "/".join(n[:-i]))] += 1
# 同一旧前缀可能对应多个新前缀（树根散文件分流到 coordination/misc）；
# 映射一个「不在表内」的路径时按支持行数取主流（结构保持的那一支），
# 例如 notes/rewrite/fork-syscall-rewrite/01-stage-pm/x.md → rewrite-notes/01-stage-pm/x.md
by_a = {}
for (a, b), c in support.items():
    if not a:
        continue
    by_a.setdefault(a, []).append((c, b))
prefix = sorted(((a, max(lst)[1]) for a, lst in by_a.items()), key=lambda x: -len(x[0]))


def map_path(p):
    if p in old_to_new:
        return old_to_new[p]
    for a, b in prefix:
        if p == a or p.startswith(a + "/"):
            return b + p[len(a):]
    return p


def load_report(path, doc_key="old"):
    out = []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        parts = line.split("\t")
        doc, ln = parts[0].rpartition(":")[0], parts[0].rpartition(":")[2]
        target = parts[1]
        out.append((doc, ln, target))
    return out


before = load_report(PRE / "broken-links.before.txt")
after = load_report(R / "migrate_notes_plan/DS/evidence/broken-links.now.txt")
print(f"基线记录 {len(before)} 条；迁移后记录 {len(after)} 条")

def canon(doc, target, mappedoc=True):
    """规范化一条断链记录：返回 (解析坐标, 锚点尾)。解析坐标已映射进新坐标系。"""
    base = target.split("#", 1)[0]
    if not base or target.startswith(("http://", "https://", "mailto:")):
        return (target,)
    if base.startswith(("notes/", "os/", "minix3/", "book/", "prompt/")):
        p = posixpath.normpath(base)
    else:
        p = posixpath.normpath(posixpath.join(posixpath.dirname(doc), base))
    if mappedoc:
        p = map_path(p)
    frag = "#" + target.split("#", 1)[1] if "#" in target else ""
    return (p, frag)


bm = set()
for doc, ln, t in before:
    if not (doc.startswith("notes/rewrite") or doc.startswith("notes/study") or doc.startswith("notes/redesign")):
        continue
    doc_new = map_path(doc)
    bm.add((doc_new, canon(doc, t)))

am = set()
for doc, ln, t in after:
    am.add((doc, canon(doc, t)))

new = am - bm
gone = bm - am
print(f"基线（三树内）规范化后 {len(bm)} 条；迁移后 {len(am)} 条")
print(f"迁移引入的新断链 = {len(new)}（期望 0）")
for x in sorted(new)[:20]:
    print("   NEW:", x)
print(f"消失或修好 = {len(gone)} 条（可解释类）")
