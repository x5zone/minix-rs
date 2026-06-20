#!/usr/bin/env python3
"""
coverage-extract.py — Minix-RS 覆盖率穷举清单生成器

从 Minix3 C 源码、Rust 实现、文档中提取符号清单，生成 SYMBOLS.md 骨架。
机器生成确定性骨架，AI 补充语义判断（如 Rust 函数对应哪个 C 函数）。

用法:
    python3 tools/coverage-extract/coverage-extract.py <module> <doc_dir> \
        [--rust-dir <path>] [--c-dir <path>] [--semantic-map <path>] \
        [--doc-file <name>] [--output <path>]

示例（模块级，服务器）:
    python3 tools/coverage-extract/coverage-extract.py vm \
        notes/rewrite/fork-syscall-rewrite/02-stage-vm \
        --rust-dir os --c-dir minix3/minix/servers/vm

示例（模块级，内核）:
    python3 tools/coverage-extract/coverage-extract.py kernel \
        notes/rewrite/fork-syscall-rewrite/03-stage-kernel \
        --rust-dir os --c-dir minix3/minix/kernel

示例（单文档级，推荐）:
    python3 tools/coverage-extract/coverage-extract.py kernel \
        notes/rewrite/fork-syscall-rewrite/03-stage-kernel \
        --rust-dir os --c-dir minix3/minix/kernel \
        --doc-file 03-kmain-cstart.md \
        --semantic-map tools/coverage-extract/kernel-semantic-map.json \
        --output .review/03-stage-kernel/03-kmain-cstart/SYMBOLS.md

输出:
    SYMBOLS.md  — 覆盖率穷举清单骨架（默认 .review/<module>/SYMBOLS.md）
    stdout      — 覆盖率统计摘要

说明:
    Rust 覆盖率默认使用"名称匹配"，对于 Minix-RS 这种 C→Rust 改写项目会严重低估
    （C 的 `prot_init` 对应 Rust 的 `ProtectionArch::init` 等方法）。必须提供
    `--semantic-map` 得到准确的 Rust 覆盖率；否则 0% 覆盖率往往是映射缺失，
    而非实现缺失。
"""

import os
import re
import sys
import json
import argparse
from pathlib import Path
from collections import defaultdict


# ===== C 源码符号提取 =====

C_FUNC_PATTERN = re.compile(
    r'^[a-zA-Z_][a-zA-Z0-9_\s\*]+?\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^;]*\)\s*\{?\s*$',
    re.MULTILINE
)

C_STRUCT_PATTERN = re.compile(
    r'^(?:typedef\s+)?struct\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\{',
    re.MULTILINE
)

C_TYPEDEF_STRUCT_PATTERN = re.compile(
    r'^typedef\s+struct\s+\{[^}]*\}\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*;',
    re.MULTILINE | re.DOTALL
)

C_MACRO_PATTERN = re.compile(
    r'^#define\s+([A-Z_][A-Z0-9_]*)\s',
    re.MULTILINE
)

C_ENUM_PATTERN = re.compile(
    r'^typedef\s+enum\s*\{([^}]*)\}\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*;',
    re.MULTILINE | re.DOTALL
)


def extract_c_symbols(c_dir):
    """从 C 源码目录提取所有函数/结构体/宏/枚举。"""
    symbols = {
        'functions': [],   # (name, file, line)
        'structs': [],     # (name, file, line)
        'macros': [],      # (name, file, line)
        'enums': [],       # (name, file, line)
        'typedefs': [],    # (name, file, line)
    }

    if not os.path.isdir(c_dir):
        print(f"WARNING: C source dir not found: {c_dir}", file=sys.stderr)
        return symbols

    for root, dirs, files in os.walk(c_dir):
        for fname in files:
            if not (fname.endswith('.c') or fname.endswith('.h')):
                continue
            fpath = os.path.join(root, fname)
            rel_path = os.path.relpath(fpath, c_dir)
            try:
                with open(fpath, 'r', encoding='utf-8', errors='replace') as f:
                    content = f.read()
            except Exception as e:
                print(f"WARNING: cannot read {fpath}: {e}", file=sys.stderr)
                continue

            # 函数定义：行首是返回类型，含函数名(参数)，不以分号结尾
            for i, line in enumerate(content.splitlines(), 1):
                stripped = line.strip()
                if not stripped or stripped.startswith('#') or stripped.startswith('//'):
                    continue
                # 跳过函数声明（以分号结尾）
                if stripped.endswith(';'):
                    continue
                # 匹配函数定义：returntype funcname(args) {
                m = re.match(
                    r'^(?:static\s+|extern\s+|inline\s+)*'
                    r'(?:[a-zA-Z_][a-zA-Z0-9_\s\*]+?)\s+'
                    r'([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^;]*\)\s*\{?\s*$',
                    stripped
                )
                if m:
                    name = m.group(1)
                    # 过滤控制关键字
                    if name not in ('if', 'for', 'while', 'switch', 'else', 'return', 'do'):
                        symbols['functions'].append((name, rel_path, i))

            # 结构体
            for m in C_STRUCT_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['structs'].append((m.group(1), rel_path, line_no))

            # typedef struct {} name;
            for m in C_TYPEDEF_STRUCT_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['typedefs'].append((m.group(1), rel_path, line_no))

            # 宏
            for m in C_MACRO_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['macros'].append((m.group(1), rel_path, line_no))

            # 枚举
            for m in C_ENUM_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['enums'].append((m.group(2), rel_path, line_no))

    # 去重（同名取首次出现）
    for key in symbols:
        seen = set()
        deduped = []
        for item in symbols[key]:
            if item[0] not in seen:
                seen.add(item[0])
                deduped.append(item)
        symbols[key] = sorted(deduped, key=lambda x: (x[1], x[2]))

    return symbols


# ===== Rust 符号提取 =====

RUST_FN_PATTERN = re.compile(
    r'^\s*(?:pub\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)',
    re.MULTILINE
)

RUST_STRUCT_PATTERN = re.compile(
    r'^\s*(?:pub\s+)?struct\s+([a-zA-Z_][a-zA-Z0-9_]*)',
    re.MULTILINE
)

RUST_ENUM_PATTERN = re.compile(
    r'^\s*(?:pub\s+)?enum\s+([a-zA-Z_][a-zA-Z0-9_]*)',
    re.MULTILINE
)

RUST_TRAIT_PATTERN = re.compile(
    r'^\s*(?:pub\s+)?trait\s+([a-zA-Z_][a-zA-Z0-9_]*)',
    re.MULTILINE
)

RUST_CONST_PATTERN = re.compile(
    r'^\s*(?:pub\s+)?const\s+([A-Z_][A-Z0-9_]*)',
    re.MULTILINE
)


def extract_rust_symbols(rust_dir):
    """从 Rust 源码目录提取所有 pub 项。"""
    symbols = {
        'functions': [],
        'structs': [],
        'enums': [],
        'traits': [],
        'consts': [],
    }

    if not rust_dir or not os.path.isdir(rust_dir):
        return symbols

    for root, dirs, files in os.walk(rust_dir):
        # 跳过 target 目录
        if 'target' in dirs:
            dirs.remove('target')
        for fname in files:
            if not fname.endswith('.rs'):
                continue
            fpath = os.path.join(root, fname)
            rel_path = os.path.relpath(fpath, rust_dir)
            try:
                with open(fpath, 'r', encoding='utf-8', errors='replace') as f:
                    content = f.read()
            except Exception:
                continue

            for m in RUST_FN_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['functions'].append((m.group(1), rel_path, line_no))

            for m in RUST_STRUCT_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['structs'].append((m.group(1), rel_path, line_no))

            for m in RUST_ENUM_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['enums'].append((m.group(1), rel_path, line_no))

            for m in RUST_TRAIT_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['traits'].append((m.group(1), rel_path, line_no))

            for m in RUST_CONST_PATTERN.finditer(content):
                line_no = content[:m.start()].count('\n') + 1
                symbols['consts'].append((m.group(1), rel_path, line_no))

    for key in symbols:
        symbols[key] = sorted(set(symbols[key]), key=lambda x: (x[1], x[2]))

    return symbols


def extract_rust_qualified_names(rust_dir):
    """
    提取 Rust impl 块中的限定名，如 `X86_64Protection::init`。
    这对 C→Rust 改写项目很有用：C 的 `prot_init` 对应 Rust 的 `ProtectionArch::init`。
    """
    qualified = []
    if not rust_dir or not os.path.isdir(rust_dir):
        return qualified

    impl_block_pattern = re.compile(
        r'impl\s+(?:<[^>]+>\s+)?(?:(\w+)\s+for\s+)?(\w+)\s*\{',
        re.DOTALL
    )

    for root, dirs, files in os.walk(rust_dir):
        if 'target' in dirs:
            dirs.remove('target')
        for fname in files:
            if not fname.endswith('.rs'):
                continue
            fpath = os.path.join(root, fname)
            rel_path = os.path.relpath(fpath, rust_dir)
            try:
                with open(fpath, 'r', encoding='utf-8', errors='replace') as f:
                    content = f.read()
            except Exception:
                continue

            # 找到每个 impl 块并提取其中的 fn
            pos = 0
            while True:
                m = impl_block_pattern.search(content, pos)
                if not m:
                    break
                trait_name = m.group(1)
                type_name = m.group(2)
                block_start = m.end()
                brace_depth = 1
                i = block_start
                while i < len(content) and brace_depth > 0:
                    if content[i] == '{':
                        brace_depth += 1
                    elif content[i] == '}':
                        brace_depth -= 1
                    i += 1
                block_end = i
                block_content = content[block_start:block_end]

                for fm in RUST_FN_PATTERN.finditer(block_content):
                    method_name = fm.group(1)
                    base_line = content[:block_start].count('\n') + 1
                    line_no = base_line + block_content[:fm.start()].count('\n')
                    if trait_name:
                        qualified.append((f"{trait_name}::{method_name}", rel_path, line_no))
                    qualified.append((f"{type_name}::{method_name}", rel_path, line_no))
                pos = block_end

    return sorted(set(qualified), key=lambda x: (x[1], x[2]))


def load_semantic_map(path):
    """加载 C 符号到 Rust 符号列表的语义映射表。"""
    if not path or not os.path.isfile(path):
        return {}
    try:
        with open(path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        # 支持 { "C": ["rust1", "rust2"] } 或 { "C": "rust1" }
        result = {}
        for k, v in data.items():
            if isinstance(v, str):
                result[k] = [v]
            elif isinstance(v, list):
                result[k] = v
            else:
                print(f"WARNING: semantic map entry for {k} is not str/list", file=sys.stderr)
        return result
    except Exception as e:
        print(f"WARNING: cannot load semantic map {path}: {e}", file=sys.stderr)
        return {}


# ===== 文档覆盖检查 =====

def check_doc_coverage(symbols, doc_dir):
    """检查每个 C 符号在文档中是否被提及。"""
    if not os.path.isdir(doc_dir):
        print(f"WARNING: doc dir not found: {doc_dir}", file=sys.stderr)
        return {}

    # 收集所有文档内容
    doc_contents = {}
    for root, dirs, files in os.walk(doc_dir):
        for fname in files:
            if fname.endswith('.md'):
                fpath = os.path.join(root, fname)
                rel = os.path.relpath(fpath, doc_dir)
                try:
                    with open(fpath, 'r', encoding='utf-8', errors='replace') as f:
                        doc_contents[rel] = f.read()
                except Exception:
                    continue

    coverage = {}  # symbol -> list of doc files that mention it

    all_c_names = set()
    for cat in ('functions', 'structs', 'macros', 'enums', 'typedefs'):
        for name, _, _ in symbols[cat]:
            all_c_names.add(name)

    for name in all_c_names:
        coverage[name] = []
        for doc_name, content in doc_contents.items():
            if name in content:
                coverage[name].append(doc_name)

    return coverage


def check_rust_coverage(symbols, rust_symbols, rust_qualified, semantic_map):
    """
    检查每个 C 符号在 Rust 中是否有对应实现。
    匹配策略（按优先级）：
      1. 语义映射表（semantic_map）
      2. Rust 限定名（如 Type::method / Trait::method）
      3. 简单名称匹配
    """
    rust_names = set()
    for cat in ('functions', 'structs', 'enums', 'consts'):
        for name, _, _ in rust_symbols[cat]:
            rust_names.add(name)
    for name, _, _ in rust_qualified:
        rust_names.add(name)

    coverage = {}
    matched_by = {}
    all_c_names = set()
    for cat in ('functions', 'structs', 'macros', 'enums', 'typedefs'):
        for name, _, _ in symbols[cat]:
            all_c_names.add(name)

    for name in all_c_names:
        # 1. 语义映射
        if semantic_map and name in semantic_map:
            mapped = semantic_map[name]
            if any(m in rust_names for m in mapped):
                coverage[name] = True
                matched_by[name] = f"语义映射 → {', '.join(m for m in mapped if m in rust_names)}"
                continue
        # 2. 限定名匹配
        qualified_hits = [q for q in rust_names if q.endswith(f"::{name}") or q == name]
        if qualified_hits:
            coverage[name] = True
            matched_by[name] = f"限定名匹配 → {qualified_hits[0]}"
            continue
        # 3. 简单名称匹配
        if name in rust_names:
            coverage[name] = True
            matched_by[name] = "名称匹配"
            continue

        coverage[name] = False
        matched_by[name] = "无匹配"

    return coverage, matched_by


# ===== SYMBOLS.md 生成 =====

def generate_symbols_md(module, c_symbols, rust_symbols, rust_qualified, doc_coverage, rust_coverage, matched_by, c_dir, doc_dir, rust_dir, semantic_map_path, doc_file=None):
    """生成 SYMBOLS.md 骨架。"""
    lines = []
    if doc_file:
        lines.append(f"# SYMBOLS.md — {module} / {doc_file} 覆盖率穷举清单")
    else:
        lines.append(f"# SYMBOLS.md — {module} 覆盖率穷举清单")
    lines.append("")
    lines.append(f"> 机器生成（coverage-extract.py），AI 补充语义判断。")
    lines.append(f"> C 源码: `{c_dir}` | 文档: `{doc_dir}` | Rust: `{rust_dir or 'N/A'}`")
    if doc_file:
        lines.append(f"> 限定文档: `{doc_file}`")
    if semantic_map_path:
        lines.append(f"> 语义映射表: `{semantic_map_path}`")
    lines.append("")
    lines.append("## 覆盖率统计")
    lines.append("")

    # 统计
    total = 0
    doc_covered = 0
    rust_covered = 0
    gaps = 0

    def is_in_target_doc(name):
        docs = doc_coverage.get(name, [])
        if doc_file:
            return doc_file in docs
        return len(docs) > 0

    for cat in ('functions', 'structs', 'macros', 'enums', 'typedefs'):
        for name, _, _ in c_symbols[cat]:
            total += 1
            in_doc = is_in_target_doc(name)
            in_rust = rust_coverage.get(name, False)
            if in_doc:
                doc_covered += 1
            if in_rust:
                rust_covered += 1
            if not in_doc and not in_rust:
                gaps += 1

    lines.append(f"| 指标 | 数值 |")
    lines.append(f"|------|------|")
    def pct(n, d):
        return f"{n * 100 / d:.1f}" if d else "0.0"

    lines.append(f"| C 符号总数 | {total} |")
    lines.append(f"| 文档覆盖 | {doc_covered} ({pct(doc_covered, total)}%) |")
    lines.append(f"| Rust 覆盖 | {rust_covered} ({pct(rust_covered, total)}%) |")
    lines.append(f"| 完全缺口 (无文档无Rust) | {gaps} |")
    lines.append(f"| 函数 | {len(c_symbols['functions'])} |")
    lines.append(f"| 结构体 | {len(c_symbols['structs']) + len(c_symbols['typedefs'])} |")
    lines.append(f"| 宏 | {len(c_symbols['macros'])} |")
    lines.append(f"| 枚举 | {len(c_symbols['enums'])} |")
    lines.append("")

    # 按类别输出
    cat_titles = {
        'functions': '函数',
        'structs': '结构体',
        'typedefs': 'Typedef 结构体',
        'enums': '枚举',
        'macros': '宏',
    }

    for cat, title in cat_titles.items():
        if not c_symbols[cat]:
            continue
        lines.append(f"## {title}")
        lines.append("")
        lines.append(f"| C 符号 | C 源码位置 | 文档覆盖 | Rust 实现 | 状态 | AI 语义判断 |")
        lines.append(f"|--------|-----------|---------|----------|------|-------------|")
        for name, file, line in c_symbols[cat]:
            in_doc_files = doc_coverage.get(name, [])
            doc_str = ", ".join(in_doc_files) if in_doc_files else "❌"
            in_doc = is_in_target_doc(name)
            in_rust = rust_coverage.get(name, False)
            match_info = matched_by.get(name, "无匹配")
            rust_str = f"✅({match_info})" if in_rust else "❌"

            if in_doc and in_rust:
                status = "✅ 覆盖"
            elif in_doc and not in_rust:
                status = "⚠️ 有文档无Rust"
            elif not in_doc and in_rust:
                status = "⚠️ 有Rust无文档"
            else:
                status = "❌ 缺口"

            # 如果限定 doc_file 且该符号不在目标文档中，可以跳过或淡化显示
            if doc_file and not in_doc:
                continue

            c_loc = f"{file}:{line}"
            lines.append(f"| `{name}` | {c_loc} | {doc_str} | {rust_str} | {status} | _待AI补充_ |")
        lines.append("")

    # Rust 符号清单（供 AI 反向核对）
    has_rust = any(rust_symbols[k] for k in rust_symbols) or rust_qualified
    if has_rust:
        lines.append("## Rust 实现符号（供反向核对）")
        lines.append("")
        lines.append("> AI 应核对：每个 Rust 符号是否都有对应的 C 符号？是否有 Rust 实现了 C 中不存在的功能？")
        lines.append("")
        all_fns = sorted(set(rust_symbols['functions'] + rust_qualified), key=lambda x: (x[1], x[2]))
        if all_fns:
            lines.append("### Rust 函数/方法")
            lines.append("")
            for name, file, line in all_fns[:80]:  # 增加限制到80个
                lines.append(f"- `{name}` — {file}:{line}")
            if len(all_fns) > 80:
                lines.append(f"- ... 共 {len(all_fns)} 个")
            lines.append("")
        if rust_symbols['structs'] or rust_symbols['enums']:
            lines.append("### Rust 结构体/枚举")
            lines.append("")
            for name, file, line in (rust_symbols['structs'] + rust_symbols['enums'])[:30]:
                lines.append(f"- `{name}` — {file}:{line}")
            lines.append("")

    # AI 补充指引
    lines.append("## AI 补充指引")
    lines.append("")
    lines.append("> 以下需要 AI 补充语义判断（机器无法自动判断）：")
    lines.append("")
    lines.append("1. **Rust 对应关系**：名称匹配不等于语义对应。AI 需确认每个 `✅(名称匹配)` 的 Rust 函数是否真的实现了对应 C 函数的语义。")
    lines.append("2. **架构演进标记**：某些 C 函数在 Rust 中不需要（如 IPC 协议演进、32→64 位变化）。AI 需标记为 `ARCH: 不需要，<理由>`。")
    lines.append("3. **语义归属**：某些符号可能属于其他文档的语义范围（如 `vm_mappages` 定义在 mmap.c 但语义属页表操作）。AI 需标注归属文档。")
    lines.append("4. **行为契约表**：对每个核心函数，补充行为契约（输入/输出/副作用/错误码/时序）。见 review-core-semantics.md。")
    lines.append("5. **测试覆盖**：补充每个函数对应的测试文件和测试函数。")
    lines.append("")
    lines.append("## 状态图例")
    lines.append("")
    lines.append("- ✅ 覆盖：文档+Rust 均有")
    lines.append("- ⚠️ 有文档无Rust：文档已分析但未实现（可能是后续阶段任务）")
    lines.append("- ⚠️ 有Rust无文档：Rust 实现了但文档未分析（P1：应补充文档）")
    lines.append("- ❌ 缺口：C 有但文档和 Rust 均无（P0：需判断是否在语义范围内）")
    lines.append("- ARCH: 架构演进，不需要（需 AI 标注理由）")

    return "\n".join(lines) + "\n"


# ===== 主函数 =====

MODULE_PATHS = {
    'vm': 'minix3/minix/servers/vm',
    'pm': 'minix3/minix/servers/pm',
    'vfs': 'minix3/minix/servers/vfs',
    'rs': 'minix3/minix/servers/rs',
    'ds': 'minix3/minix/servers/ds',
    'inet': 'minix3/minix/servers/inet',
    'kernel': 'minix3/minix/kernel',
    'drivers': 'minix3/minix/drivers',
    'include': 'minix3/minix/include',
}


def main():
    parser = argparse.ArgumentParser(description='Minix-RS 覆盖率穷举清单生成器')
    parser.add_argument('module', help='模块名 (vm/pm/vfs/kernel/...) 或自定义 C 源码路径')
    parser.add_argument('doc_dir', help='文档目录路径')
    parser.add_argument('--rust-dir', help='Rust 源码目录路径', default=None)
    parser.add_argument('--c-dir', help='自定义 C 源码目录（覆盖 module 默认路径）', default=None)
    parser.add_argument('--semantic-map', help='C→Rust 语义映射表 JSON 文件路径', default=None)
    parser.add_argument('--doc-file', help='只统计该文档文件（相对于 doc_dir 的路径，如 03-kmain-cstart.md）', default=None)
    parser.add_argument('--output', help='输出文件路径（默认 .review/{tool}/{module}/scans/SYMBOLS.md）', default=None)

    args = parser.parse_args()

    # 确定 C 源码目录
    if args.c_dir:
        c_dir = args.c_dir
    elif args.module in MODULE_PATHS:
        c_dir = MODULE_PATHS[args.module]
    else:
        c_dir = args.module  # 允许直接传路径

    # 确定 Rust 目录
    rust_dir = args.rust_dir
    if not rust_dir:
        # 默认推断
        if args.module == 'vm':
            rust_dir = 'os/servers/vm/src'
        elif args.module == 'kernel':
            rust_dir = 'os/kernel/src'
        elif args.module == 'pm':
            rust_dir = 'os/servers/pm/src'
        # 其他模块默认 None

    # 加载语义映射表
    semantic_map = load_semantic_map(args.semantic_map)
    if semantic_map:
        print(f"Loaded semantic map: {len(semantic_map)} entries", file=sys.stderr)

    print(f"Module: {args.module}", file=sys.stderr)
    print(f"C source: {c_dir}", file=sys.stderr)
    print(f"Docs: {args.doc_dir}", file=sys.stderr)
    print(f"Rust: {rust_dir or 'N/A'}", file=sys.stderr)
    print(file=sys.stderr)

    # 提取符号
    print("Extracting C symbols...", file=sys.stderr)
    c_symbols = extract_c_symbols(c_dir)
    c_total = sum(len(v) for v in c_symbols.values())
    print(f"  Found {c_total} C symbols ({len(c_symbols['functions'])} funcs, "
          f"{len(c_symbols['structs'])+len(c_symbols['typedefs'])} structs, "
          f"{len(c_symbols['macros'])} macros, {len(c_symbols['enums'])} enums)", file=sys.stderr)

    print("Extracting Rust symbols...", file=sys.stderr)
    rust_symbols = extract_rust_symbols(rust_dir)
    rust_qualified = extract_rust_qualified_names(rust_dir)
    r_total = sum(len(v) for v in rust_symbols.values()) + len(rust_qualified)
    print(f"  Found {r_total} Rust symbols ({len(rust_symbols['functions'])} top-level fn, "
          f"{len(rust_qualified)} qualified methods)", file=sys.stderr)

    print("Checking doc coverage...", file=sys.stderr)
    doc_coverage = check_doc_coverage(c_symbols, args.doc_dir)

    print("Checking Rust coverage...", file=sys.stderr)
    rust_coverage, matched_by = check_rust_coverage(c_symbols, rust_symbols, rust_qualified, semantic_map)

    # 生成 SYMBOLS.md
    print("Generating SYMBOLS.md...", file=sys.stderr)
    content = generate_symbols_md(
        args.module, c_symbols, rust_symbols, rust_qualified,
        doc_coverage, rust_coverage, matched_by,
        c_dir, args.doc_dir, rust_dir, args.semantic_map, args.doc_file
    )

    # 确定输出路径
    if args.output:
        output_path = args.output
        output_dir = os.path.dirname(output_path)
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
    else:
        output_dir = f".review/{args.module}"
        os.makedirs(output_dir, exist_ok=True)
        output_path = os.path.join(output_dir, "SYMBOLS.md")

    with open(output_path, 'w', encoding='utf-8') as f:
        f.write(content)

    print(file=sys.stderr)
    print(f"✅ Written: {output_path}", file=sys.stderr)
    print(file=sys.stderr)

    # stdout 输出统计摘要
    all_c_items = [(n, f, l) for cat in c_symbols.values() for n, f, l in cat]
    total = len(all_c_items)

    def in_target_doc(n):
        if args.doc_file:
            return args.doc_file in doc_coverage.get(n, [])
        return bool(doc_coverage.get(n))

    doc_covered = sum(1 for n, _, _ in all_c_items if in_target_doc(n))
    rust_covered = sum(1 for n, _, _ in all_c_items if in_target_doc(n) and rust_coverage.get(n))
    gaps = sum(1 for n, _, _ in all_c_items
               if in_target_doc(n) and not rust_coverage.get(n))

    def pct(n, d):
        return f"{n * 100 / d:.1f}" if d else "0.0"

    print(f"Coverage Summary for {args.module}{' / ' + args.doc_file if args.doc_file else ''}:")
    print(f"  Total C symbols: {total}")
    print(f"  Doc covered: {doc_covered} ({pct(doc_covered, total)}%)")
    print(f"  Rust covered: {rust_covered} ({pct(rust_covered, total)}%)")
    print(f"  Output: {output_path}")


if __name__ == '__main__':
    main()
