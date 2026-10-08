#!/usr/bin/env python3
"""anchor-suspect-scan.py — 锚点句意复核清单生成器（按侧对称，2026-10-08）

背景：迁移把旧行号锚点按「向上就近定义」启发式转成符号锚点，解析出的符号**未必是句子讨论的符号**
（比行号漂移更隐蔽：符号看似权威）。历史基线 `tools/anchor-suspect-baseline.txt` 只覆盖 `os/`（868 条），
`minix3/`（C）侧零覆盖 —— 三轮干跑（pm-10 / rs-11 / sched-14）实测 C 锚点成片指错对象而没有任何工具预警。

本工具按同一判据扫描**两侧**：锚点里的符号名若未出现在该行「锚点之外」的行文里，即为可疑项。

用法：
  tools/anchor-suspect-scan.py rewrite-notes/06-stage-sched/14-rs-interaction.md      # 单篇
  tools/anchor-suspect-scan.py rewrite-notes/ --side c --baseline tools/anchor-suspect-baseline-c.txt
  tools/anchor-suspect-scan.py rewrite-notes/ --side rust --baseline tools/anchor-suspect-baseline.txt

退出码：0 正常（可疑项即输出清单，不作门禁）；2 用法错误。
"""
import argparse
import os
import re
import sys

# 锚点形态（与 tools/anchor-resolve.sh 的提取口径对齐）
ANCHOR = re.compile(
    r'([A-Za-z0-9_./-]+\.(?:rs|c|h|S|asm|ld)):'
    r'((?:fn|struct|enum|trait|const|static|type|impl)\s+[A-Za-z_][A-Za-z0-9_:]*'
    r'|[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)?)'
)
KEYWORDS = ('fn', 'struct', 'enum', 'trait', 'const', 'static', 'type', 'impl')

# R9-D3：另一类坐标形态——正文里的「`sem.c` 第 697 行」式引用。三个锚点工具都不解析它，
# 于是整族文档 vacuous green。此处只做**可见化**（列为卫生项待迁移为符号锚点），不做校验。
PROSE_LINE = re.compile(
    r'`?([A-Za-z0-9_./-]+\.(?:c|h|rs))`?\s*第\s*'
    r'(?:\d+|[零一二三四五六七八九十百千]+)'
    r'(?:\s*[-–~]\s*(?:\d+|[零一二三四五六七八九十百千]+))?\s*行')


def symbol_of(token: str) -> str:
    parts = token.split()
    name = parts[1] if len(parts) == 2 and parts[0] in KEYWORDS else token
    return name.split('::')[-1]


def side_of(path: str) -> str:
    return 'rust' if path.startswith('os/') else 'c'


def scan_doc(path: str, side: str):
    hits = []
    prose = []
    with open(path, 'r', encoding='utf-8', errors='replace') as f:
        in_fence = False
        for lineno, line in enumerate(f, 1):
            if line.lstrip().startswith('```') or line.lstrip().startswith('~~~'):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            for m in ANCHOR.finditer(line):
                fpath, token = m.group(1), m.group(2)
                if side != 'all' and side_of(fpath) != side:
                    continue
                sym = symbol_of(token)
                rest = line[:m.start()] + line[m.end():]
                if not re.search(r'(?<![A-Za-z0-9_])' + re.escape(sym) + r'(?![A-Za-z0-9_])', rest):
                    hits.append((lineno, f"{fpath}:{token}", sym))
            for pm in PROSE_LINE.finditer(line):
                # R12-D2：只进 prose（不重复进 hits），且按 --side 过滤（.rs→rust，其余→c）
                f = pm.group(1)
                if side != 'all' and ('rust' if f.endswith('.rs') else 'c') != side:
                    continue
                prose.append((lineno, pm.group(0)))
    return hits, prose


def main():
    ap = argparse.ArgumentParser(description='锚点句意复核清单生成器（按侧对称）')
    ap.add_argument('target', help='文档路径或目录（目录递归扫描 .md）')
    ap.add_argument('--side', choices=['c', 'rust', 'all'], default='all')
    ap.add_argument('--baseline', help='输出基线文件（doc:line: anchor → 符号未出现在句意）')
    args = ap.parse_args()

    docs = []
    if os.path.isdir(args.target):
        for root, dirs, files in os.walk(args.target):
            dirs[:] = [d for d in dirs if not d.startswith('.')]
            docs += [os.path.join(root, f) for f in sorted(files) if f.endswith('.md')]
    elif os.path.isfile(args.target):
        docs = [args.target]
    else:
        print(f'ERROR: {args.target} 不存在', file=sys.stderr)
        sys.exit(2)

    lines, total, per_side = [], 0, {'c': 0, 'rust': 0}
    for d in docs:
        hits_d, prose_d = scan_doc(d, args.side)
        for lineno, anchor in prose_d:  # R11-D2：卫生项标签必须出现在 stdout
            print(f'{d}:{lineno}: {anchor} → [卫生项] 文件+第N行 形态，工具不解析')
        for lineno, anchor, sym in hits_d:
            total += 1
            per_side[side_of(anchor.split(':')[0])] += 1
            lines.append(f'{d}:{lineno}: {anchor} → 符号名未出现在本句，需句意复核')

    if args.baseline:
        with open(args.baseline, 'w', encoding='utf-8') as f:
            f.write('# anchor-suspect-baseline（按侧对称，2026-10-08 起）\n')
            f.write('# 判据：锚点符号名未出现在该行「锚点之外」的行文里 → 句子讨论的可能不是它。\n')
            f.write('# 复核完成后本文件应清空；生成：tools/anchor-suspect-scan.py <tree> --side <c|rust>\n')
            f.write('\n'.join(lines) + ('\n' if lines else ''))
    for ln in lines:
        print(ln)
    print(f'--- 可疑锚点 {total} 处（C={per_side["c"]}，Rust={per_side["rust"]}）｜ 文档 {len(docs)} 篇',
          file=sys.stderr)


if __name__ == '__main__':
    main()
