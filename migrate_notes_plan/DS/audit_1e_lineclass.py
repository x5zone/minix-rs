#!/usr/bin/env python3
"""独立审计脚本 3：把 153 个残差文件的逐行变化归入三类。

分类规则（对每个 diff 变化块内的行对）：
  M 前缀替换：把旧行按合同替换对变换后 == 新行
  L 链接重定基：两行剥掉全部 markdown 链接目标 `](...)` 后相等（只有目标变了，文本没变）
  S 结构性编辑：其余
输出：每文件的 M/L/S 计数汇总 + 全部 S 行的逐行明细。
"""
import difflib
import re
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"


def sh(*a):
    import subprocess
    return subprocess.run(a, cwd=R, capture_output=True).stdout


def blob(ref, path):
    return sh("git", "show", f"{ref}:{path}").decode("utf-8", "surrogateescape")


rows = [l.split("\t") for l in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]]
pairs = set()
for old, new, kind, rule in rows:
    pairs.add((old, new))
    o, n = old.split("/"), new.split("/")
    i = 0
    while i < len(o) and i < len(n) and o[-1 - i] == n[-1 - i]:
        i += 1
    if i:
        pairs.add(("/".join(o[:-i]) if i < len(o) else "", "/".join(n[:-i]) if i < len(n) else ""))
pairs = sorted({(a, b) for a, b in pairs if a}, key=lambda x: -len(x[0]))


def transform(text):
    for a, b in pairs:
        text = text.replace(a, b)
    return text


LINK = re.compile(r"\]\([^)]*\)")


def strip_links(line):
    return LINK.sub("](X)", line)


resid = [l.split("\t") for l in (R / "migrate_notes_plan/DS/evidence/item1d-residual-list.tsv").read_text(encoding="utf-8").splitlines()]

per_file = []
structural = []
for old, new, kind, why in resid:
    if why.startswith("blob"):
        per_file.append((new, 0, 0, 0, "blob 不可得"))
        continue
    a = blob("notes/pre-migrate-20261007", old).splitlines()
    b = blob("HEAD", new).splitlines()
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    m = l = s = 0
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == "equal":
            continue
        blk_a, blk_b = a[i1:i2], b[j1:j2]
        if len(blk_a) == len(blk_b):
            for ra, rb in zip(blk_a, blk_b):
                if transform(ra) == rb:
                    m += 1
                elif strip_links(ra) == strip_links(rb):
                    l += 1
                else:
                    s += 1
                    structural.append((new, ra, rb))
        else:
            # 不等长块：逐行尝试，剩下都算结构
            sm2 = difflib.SequenceMatcher(None, blk_a, blk_b, autojunk=False)
            for op2, k1, k2, t1, t2 in sm2.get_opcodes():
                if op2 == "equal":
                    continue
                sa, sb = blk_a[k1:k2], blk_b[t1:t2]
                if len(sa) == len(sb):
                    for ra, rb in zip(sa, sb):
                        if transform(ra) == rb:
                            m += 1
                        elif strip_links(ra) == strip_links(rb):
                            l += 1
                        else:
                            s += 1
                            structural.append((new, ra, rb))
                else:
                    for ra in sa:
                        s += 1
                        structural.append((new, ra, None))
                    for rb in sb:
                        s += 1
                        structural.append((new, None, rb))
    per_file.append((new, m, l, s, ""))

per_file.sort(key=lambda x: -x[3])
lines = []
lines.append("文件\t前缀替换\t链接重定基\t结构性\t备注")
for new, m, l, s, why in per_file:
    lines.append(f"{new}\t{m}\t{l}\t{s}\t{why}")
# 按结构性计数排序输出汇总
tot_m = sum(x[1] for x in per_file)
tot_l = sum(x[2] for x in per_file)
tot_s = sum(x[3] for x in per_file)
lines.append(f"合计\t{tot_m}\t{tot_l}\t{tot_s}")
(R / "migrate_notes_plan/DS/evidence/item1e-per-file-class.tsv").write_text("\n".join(lines), encoding="utf-8")
print(f"残差文件 {len(per_file)}：前缀替换 {tot_m} 行 / 链接重定基 {tot_l} 行 / 结构性 {tot_s} 行")

slines = ["文件\t旧行\t新行"]
for f, ra, rb in structural:
    slines.append(f"{f}\t{'' if ra is None else ra}\t{'' if rb is None else rb}")
(R / "migrate_notes_plan/DS/evidence/item1e-structural.tsv").write_text("\n".join(slines), encoding="utf-8")

# 结构性编辑按文件聚合
from collections import Counter
c = Counter(f for f, _, _ in structural)
print("结构性编辑最多的 25 个文件：")
for f, n in c.most_common(25):
    print(f"   {n:5d}  {f}")
