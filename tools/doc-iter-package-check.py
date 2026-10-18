#!/usr/bin/env python3
"""doc-iter-package-check.py — 轮次包全包读数互校（C10 增强 / 裁决 AX 的工具化；v3：五类精确模式）

用法：python3 tools/doc-iter-package-check.py <package.md> [<records.md> ...]
      python3 tools/doc-iter-package-check.py --self-test

做什么：对给定文件（含内嵌附录）正则抽取五类读数并打印比对记录——
  ① 文档行数：`N 行`（过滤 3~4 位数字，含 "N→M 行" 形态取 M）
  ② 台账行数：`N 行台账`
  ③ 编辑处数：`N+ 处编辑`
  ④ 门读数：`N/N`（分母 ≤ 200 的形态）
  ⑤ 处置分项之和（裁决 BA）：含 `C4` 或 `台账` 的行内 `①/②/③/④/⑤ + N` 计数求和，
     与台账行数比对（判例：11 R01 报 ①66/④9/③1=76 与 74 行差 2）
判据：同一指标的取值集合大小 > 1 → 报 MISMATCH（要求同处写明口径差，或统一）。

阴性声明（裁决 BB——本工具是白名单，模式宽度即敏感度，与 C38 词表同构）。
以下写法实测不覆盖（出现时应先扩工具并补 self-test 用例，不靠人眼）：
  - `N 处`（无"编辑"后缀），如"R02 仅 1 处"；
  - 基线门读数（`五门（基线）：anchor 76/76｜…`——`）：` 与 `｜` 分隔，门模式不匹配）；
  - 未加粗、无前缀的裸 `N 行`（如 C16 形态画像表内 `` `1618` ``）；
  - 自由散文读数（如"FID 221 项/20 不一致"）；
  - 含 `→` 的原值-现值段（修正留痕/口径差双列，如"修 66→65"）；
  - 工具自身输出行（`[OK]`/`[MISMATCH]`/`[SCOPE]`——粘贴回包后防二次解析）；
  - 不含 ① 的分项片段（如交付检查卡的"④8/③2 全有新状态"——片段不是和）。

退出码：0 = 全部一致；1 = 存在不一致；2 = 用法错误。
"""
import io, re, sys

SCOPE = ('[SCOPE] 白名单 5 类：doc-lines / ledger-rows / edit-count / gate / disposition-sum；'
         '不覆盖：N 处（无编辑后缀）/ 基线门读数（）：｜ 分隔）/ 裸 N 行 / 自由散文读数（见 docstring 阴性声明）')

DISP = re.compile(r'([①②③④⑤])\s*(?:修|登记|假阳性|转出|证据不足|挂起)?\s*\**\s*(\d+)')


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
    # ⑤ 处置分项之和（裁决 BA）：含 \bC4\b 或"台账"的行，按「；。」分段后每段 ≥2 个 ①…⑤ 计数。
    # 跳过：工具自身输出行（[OK]/[MISMATCH]/[SCOPE]——防粘贴回包后被二次解析）与含 `→` 的
    # 原值-现值段（修正留痕/口径差）。
    for line in t.splitlines():
        if not (re.search(r'\bC4\b', line) or '台账' in line):
            continue
        if '[OK]' in line or '[MISMATCH]' in line or '[SCOPE]' in line:
            continue
        for seg in re.split(r'[；。]', line):
            if '→' in seg:
                continue
            ms = DISP.findall(seg)
            # 分项和须含 ①（片段引用如"④8/③2 全有新状态"不是和，不参与比对）
            if len(ms) < 2 or not any(mk == '①' for mk, _ in ms):
                continue
            total = sum(int(n) for _, n in ms)
            detail = '+'.join(f'{mk}{n}' for mk, n in ms)
            rows.add(('disposition-sum', f'{total}（{detail}）'))
    return rows


def report(paths):
    per = [(p, extract(p)) for p in paths]
    bad = 0
    print(SCOPE)
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
    # ⑤ 处置分项之和：跨文件明细须单值，且与台账行数一致
    ds = {}
    for p, rs in per:
        for kk, v in rs:
            if kk == 'disposition-sum':
                ds.setdefault(v, []).append(p.split('/')[-1])
    if ds:
        lrs = set(v for _, rs in per for kk, v in rs if kk == 'ledger-rows')
        if len(ds) > 1:
            bad += 1
            print('[MISMATCH] disposition-sum: ' + ' | '.join(
                f'{v} <- {",".join(ps)}' for v, ps in sorted(ds.items())) + f' vs ledger-rows {sorted(lrs)}')
        elif lrs and set(x.split('（')[0] for x in ds) != lrs:
            bad += 1
            v = next(iter(ds))
            print(f'[MISMATCH] disposition-sum: {v} vs ledger-rows {sorted(lrs)}（分项之和与台账行数不符）')
        else:
            v = next(iter(ds))
            print(f'[OK]       disposition-sum: {v} = ledger-rows（{len(ds[v])} 处）')
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
    p2 = os.path.join(d, 'rec.md')

    def w(f, s):
        io.open(f, 'w', encoding='utf-8').write(s)

    # 反例一：标量读数不一致（AX 判例形态）
    w(p1, '**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）；C4：①65/④8/③1 全在册\n')
    w(p2, '**1617 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）；C4：①65/④8/③1 全在册\n')
    bad = report([p1, p2])
    assert bad >= 1, 'self-test: expected doc-lines mismatch'
    # 反例二：标量一致、处置分项之和 ≠ 台账行数（BA 判例形态）
    w(p1, '**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）；台账 **F-11-001…074**：①修 66 + ④登记 9 + ③假阳性 1\n')
    w(p2, '**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）\n')
    bad = report([p1, p2])
    assert bad >= 1, 'self-test: expected disposition-sum mismatch'
    # 反例三：分项之和跨层不同（76 vs 74）
    w(p2, '**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）；C4：①66/④9/③1\n')
    bad = report([p1, p2])
    assert bad >= 1, 'self-test: expected cross-layer disposition mismatch'
    # 正例：全一致
    w(p1, '**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）；台账 **F-11-001…074**：①修 65 + ④登记 8 + ③假阳性 1；C4：①65/④8/③1 全在册\n')
    w(p2, '**1618 行 @ 终稿**；74 行台账；90+ 处编辑；五门全绿（77/77、8/8）；C4：①65/④8/③1\n')
    bad2 = report([p1, p2])
    assert bad2 == 0, 'self-test: expected clean'
    print('self-test OK（反例一/二/三各检出 1+ 不一致；正例 0）')


if __name__ == '__main__':
    if len(sys.argv) == 2 and sys.argv[1] == '--self-test':
        self_test()
    elif len(sys.argv) >= 2:
        sys.exit(1 if report(sys.argv[1:]) else 0)
    else:
        print(__doc__)
        sys.exit(2)
