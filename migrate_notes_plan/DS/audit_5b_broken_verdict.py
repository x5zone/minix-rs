#!/usr/bin/env python3
"""独立审计脚本 7：对「现状全部断链」逐条做唯一判据检查——迁前是否可解析。

判据：新断链 = 现状断链 且 迁前该目标在当前文件（旧位置）的旧坐标系下可解析。
凡「迁前即断」的，一律是既存断链（迁移前就是这么坏的），不计入新增。
另对「迁前可解析」的逐条打出，即为 P0 候选。
对迁移期新建文件（无旧对应）单列，检查其链接是否有坏链。
"""
import posixpath
import subprocess
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"

rows = [l.split("\t") for l in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]]
old_to_new = {r[0]: r[1] for r in rows}
new_to_old = {}
for o, n in old_to_new.items():
    new_to_old.setdefault(n, o)
manifest = set(l.split("\t")[0] for l in (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:])

entries = []
for line in (R / "migrate_notes_plan/DS/evidence/broken-links.now.txt").read_text(encoding="utf-8").splitlines():
    if line.startswith("#") or not line.strip():
        continue
    parts = line.split("\t")
    doc, ln = parts[0].rpartition(":")[0], parts[0].rpartition(":")[2]
    entries.append((doc, ln, parts[1]))
print(f"现状断链 {len(entries)} 条")

FROZEN = ("/evidence/", "/.design/", "/legacy-fork-bak/")
new_breaks, prebroken, newfile = [], [], []
for doc, ln, t in entries:
    old_doc = new_to_old.get(doc)
    base = t.split("#", 1)[0]
    if not base or t.startswith(("http://", "https://", "mailto:")):
        prebroken.append((doc, ln, t, "非路径目标"))
        continue
    if old_doc is None:
        newfile.append((doc, ln, t))
        continue
    if base.startswith(("notes/", "os/", "minix3/", "book/", "prompt/")):
        ro = posixpath.normpath(base)
    else:
        ro = posixpath.normpath(posixpath.join(posixpath.dirname(old_doc), base))
    exists_old = (ro in manifest) or (R / ro).is_file()
    tag = "FROZEN" if any(f in doc for f in FROZEN) else ""
    if exists_old:
        new_breaks.append((doc, ln, t, ro, tag))
    else:
        prebroken.append((doc, ln, t, ro, tag))

print(f"迁前可解析（真新断链，P0 候选）: {len(new_breaks)}")
for x in new_breaks:
    print("   !!!", x)
print(f"迁前即断（既存断链）: {len(prebroken)}")
from collections import Counter
print("   既存断链按文件分布（前 15）:")
for f, c in Counter(x[0] for x in prebroken).most_common(15):
    print(f"      {c:4d}  {f}")
frozen = [x for x in prebroken if len(x) > 4 and x[4] == "FROZEN"]
print(f"   其中冻结区（evidence/.design/legacy-fork-bak）: {len(frozen)} 条")
print(f"迁移期新建文件的断链: {len(newfile)} 条")
for x in newfile:
    print("   NEWFILE:", x)
