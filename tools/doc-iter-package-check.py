#!/usr/bin/env python3
"""doc-iter-package-check.py — 轮次包全包读数互校（C10 增强 / 裁决 AX 的工具化；v2：四类精确模式）

用法：python3 tools/doc-iter-package-check.py <package.md> [<records.md> ...]
      python3 tools/doc-iter-package-check.py --self-test

做什么：对给定文件（含内嵌附录）正则抽取三类读数并打印比对记录——
  ① 文档行数：`N 行`（过滤 3~4 位数字，含 "N→M 行" 形态取 M）
  ② 台账行数：`N 行台账`
  ③ 编辑处数：`N+ 处编辑`
  ④ 门读数：`N/N`（分母 ≤ 200 的形态）
判据：同一指标的取值集合大小 > 1 → 报 MISMATCH（要求同处写明口径差，或统一）。

退出码：0 = 全部一致；1 = 存在不一致；2 = 用法错误。
"""
import io, re, sys


def extract(path):
    t = io.open(path, encoding='utf-8').read()
    rows = set()
    # ① 文档行数（精确模式：规模读数行 / A→**B 行**）
    for m in re.finditer(r'\*\*(\d{3,4}) 行 @ ', t):
        rows.add(('doc-lines', m.group(1)))
    for m in re.finditer(r'\d{3,4}→\**(\d{3,4}) 行', t):
        rows.add(('doc-lines', m.group(1)))
    for m in re.finditer(r'\*\*(\d{3,4}) 行\*\*', t):
        rows.add(('doc-lines', m.group(1)))
    for m in re.finditer(r'文档 (\d{3,4}) 行', t):
        rows.add(('doc-lines', m.group(1)))
    # ② 台账行数
    for m in re.finditer(r'(\d+) 行台账', t):
        rows.add(('ledger-rows', m.group(1)))
    # ③ 编辑处数
    for m in re.finditer(r'(\d+)\+? 处编辑', t):
        rows.add(('edit-count', m.group(1)))
    # ④ 五门头条读数（"五门全绿（A/B、C/D" / "五门（终稿…）A/B、C/D"）
    for pat in (r'五门全绿（(\d{1,3}/\d{1,3})、(\d{1,3}/\d{1,3})',
                r'五门（终稿[^）]*）\**(\d{1,3}/\d{1,3})、(\d{1,3}/\d{1,3})'):
        for m in re.finditer(pat, t):
            rows.add(('gate', m.group(1)))
            rows.add(('gate', m.group(2)))
    return rows


def report(paths):
    per = [(p, extract(p)) for p in paths]
    bad = 0
    # 标量指标（doc-lines/ledger-rows/edit-count）：跨文件取值集合须为单值
    for k in ('doc-lines', 'ledger-rows', 'edit-count'):
        vals = {}
        for p, rs in per:
            for kk, v in rs:
                if kk == k:
                    vals.setdefault(v, []).append(p.split('/')[-1])
        if not vals:
            continue
        if len(vals) > 1:
            bad += 1
            print(f'[MISMATCH] {k}: ' + ' | '.join(f'{v} <- {",".join(ps)}' for v, ps in sorted(vals.items())))
        else:
            v = next(iter(vals))
            print(f'[OK]       {k}: {v}（{len(vals[v])} 处）')
    # 门读数：按"每文件的 N/N 集合"比对（不同门天然不同值，只要求各层集合一致）
    gate_sets = {}
    for p, rs in per:
        gs = frozenset(v for kk, v in rs if kk == 'gate')
        gate_sets.setdefault(gs, []).append(p.split('/')[-1])
    if len(gate_sets) > 1:
        bad += 1
        print('[MISMATCH] gate: ' + ' | '.join(
            f'{{{",".join(sorted(g))}}} <- {",".join(ps)}' for g, ps in sorted(gate_sets.items(), key=lambda x: sorted(x[0]))))
    elif gate_sets:
        g = next(iter(gate_sets))
        print(f'[OK]       gate: {{{",".join(sorted(g))}}}（各层集合一致）')
    return bad


def self_test():
    import tempfile, os
    d = tempfile.mkdtemp()
    p1 = os.path.join(d, 'pkg.md')
    io.open(p1, 'w', encoding='utf-8').write('**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）\n')
    p2 = os.path.join(d, 'rec.md')
    io.open(p2, 'w', encoding='utf-8').write('**1617 行 @ 终稿**；56 行台账；50+ 处编辑；五门全绿（77/77、8/8）\n')
    bad = report([p1, p2])
    assert bad >= 1, 'self-test: expected mismatch'
    io.open(p2, 'w', encoding='utf-8').write('**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）\n')
    bad2 = report([p1, p2])
    assert bad2 == 0, 'self-test: expected clean'
    print('self-test OK（正例检出 1+ 不一致；反例 0）')


if __name__ == '__main__':
    if len(sys.argv) == 2 and sys.argv[1] == '--self-test':
        self_test()
    elif len(sys.argv) >= 2:
        sys.exit(1 if report(sys.argv[1:]) else 0)
    else:
        print(__doc__)
        sys.exit(2)
