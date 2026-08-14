#!/usr/bin/env python3
"""
review-state-validate.py — Minix-RS Review STATE.md 预检与一致性校验

校验内容（P0-3 强制预检）:
  1. STATE.md 引用的所有文件路径是否存在（防"幻觉状态"）
  2. STATE.md Open P0/P1/P2 列表中的 issue 是否能在 scan.md 中找到对应条目（防"孤儿 issue"）
  3. STATE.md 必备段是否存在（Per-Doc Status / Session Status / Phase Completion Log）
  4. STATE.md 中自述的"已创建/已产出"文件是否真实存在

强制时机（见 improve-v2 §1.3.3）:
  - 每次 STATE.md 写入前（预检）
  - Trae 内多 AI bagging 合并前
  - review-session 启动时（Step 0）

用法:
    python3 tools/review-state-validate.py --state .review/trae/{module}/STATE.md
    python3 tools/review-state-validate.py --state .review/trae/{module}/STATE.md \
        --scan .review/trae/{module}/scans/{doc-stem}-{agent}-scan.md
    python3 tools/review-state-validate.py --state .review/claude/{module}/STATE.md --strict
    python3 tools/review-state-validate.py --state .review/codex/{module}/STATE.md --strict

退出码:
    0 = 全部通过
    1 = 有 ERROR（引用文件缺失 / 必备段缺失）
    2 = 仅有 WARNING（孤儿 issue / 自述产物未找到），--strict 时也返回 1

输出:
    stdout — 人类可读的校验报告
    机器可解析的关键字: ✅ PASS / ❌ FAIL / ⚠️ WARN
"""

import os
import re
import sys
import argparse
from pathlib import Path


# ===== 正则模式 =====

# 文件路径引用：匹配 `path/to/file.ext` 形式（含 .md/.py/.rs/.c/.h/.json/.sh 等）
# 排除 URL 和明显非路径的 token
PATH_REF_PATTERN = re.compile(
    r'(?<![\w/])('
    r'(?:\.review|notes|tools|prompt|minix3|os)/[A-Za-z0-9_./{}*?+-]+'
    r'\.(?:md|py|rs|c|h|json|sh|txt|toml)'
    r')',
    re.MULTILINE
)

ISSUE_ID = r'P[012]-[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*'

# Open issue 行：匹配 "- #P0-1 ..." / "- P0-DOC-1 ..." / "| P0-DOC-1 | ..." 等
OPEN_ISSUE_PATTERN = re.compile(
    rf'(?:^|\s)(#?{ISSUE_ID})\b',
    re.MULTILINE
)

# 自述"已创建/已产出"语句
CREATED_FILE_PATTERN = re.compile(
    r'(?:已创建|已产出|已生成|已写入|written to|created at|saved to)\s*[`:"]?\s*'
    r'((?:\.review|notes|tools)/[A-Za-z0-9_./{}*?+-]+\.(?:md|py|rs|c|h|json|sh|txt|toml))',
    re.IGNORECASE
)

# 必备段标题
REQUIRED_SECTIONS = [
    "Phase Completion Log",
    "Per-Doc Status",
    "Session Status",
    "Convergence Checklist",
]

# scan.md 中的 issue 行（更宽松，匹配表格或列表中的 P0/P1/P2 ID）
SCAN_ISSUE_PATTERN = re.compile(
    rf'\b({ISSUE_ID})\b',
    re.MULTILINE
)


# ===== 提取函数 =====

def extract_path_refs(text):
    """从文本中提取所有文件路径引用，返回去重后的列表。"""
    refs = PATH_REF_PATTERN.findall(text)
    # 去重并保持顺序
    seen = set()
    unique = []
    for r in refs:
        if r not in seen:
            seen.add(r)
            unique.append(r)
    return unique


def is_template_path(path):
    """Return True for documentation placeholders, not literal file paths."""
    return any(token in path for token in ("{", "}", "*", "..."))


def extract_open_issues(text):
    """从 STATE.md 的 Open P0/P1/P2 段落提取 issue ID。

    策略：找到 "Open P0 issues" / "Open P1 issues" / "Open P2 issues" 行，
    从该行及其后续括号内提取 P0-N / P0-DOC-1 / P1-CODE-2 形式的 ID。
    """
    issues = set()
    # 匹配 "Open P0 issues: 3 (#1, #2, #3)" 或 "Open P0 issues: #P0-1, #P0-2"
    open_line_pattern = re.compile(
        r'Open\s+P([012])\s+issues\s*\**\s*:\s*([^\n]*)',
        re.IGNORECASE
    )
    for match in open_line_pattern.finditer(text):
        severity = 'P' + match.group(1)
        rest = match.group(2)
        # 提取所有 #N 或 P{X}-N 形式
        # 形式 1: "#1, #2, #3" → 转为 P0-1, P0-2, P0-3
        hash_nums = re.findall(r'#(\d+)', rest)
        for n in hash_nums:
            issues.add(f"{severity}-{n}")
        # 形式 2: "P0-1, P0-DOC-2" 直接匹配
        explicit_ids = re.findall(
            rf'({severity}-[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*)', rest
        )
        issues.update(explicit_ids)

    # 也扫描 "## Open Issues" 段落下的列表项
    open_section_pattern = re.compile(
        r'##\s*Open\s+(?:P0|P1|P2|Issues)[^\n]*\n([\s\S]*?)(?=\n##|\Z)',
        re.IGNORECASE
    )
    for section_match in open_section_pattern.finditer(text):
        section_text = section_match.group(1)
        for issue_match in OPEN_ISSUE_PATTERN.finditer(section_text):
            issues.add(issue_match.group(1).lstrip('#'))

    return sorted(issues)


def extract_created_files(text):
    """提取 STATE.md 中自述"已创建/已产出"的文件路径。"""
    files = set()
    for match in CREATED_FILE_PATTERN.finditer(text):
        files.add(match.group(1))
    # 也匹配 Markdown 链接中的路径：[text](path/to/file.md)
    link_pattern = re.compile(
        r'\[[^\]]*\]\(((?:\.review|notes|tools)/[A-Za-z0-9_./{}*?+-]+\.(?:md|py|rs|c|h|json|sh|txt|toml))\)'
    )
    for match in link_pattern.finditer(text):
        files.add(match.group(1))
    return sorted(files)


def check_required_sections(text):
    """检查必备段是否存在。"""
    missing = []
    for section in REQUIRED_SECTIONS:
        if section not in text:
            missing.append(section)
    return missing


def extract_scan_issues(scan_text):
    """从 scan.md 中提取所有 issue ID。"""
    return sorted(set(SCAN_ISSUE_PATTERN.findall(scan_text)))


# ===== 校验函数 =====

def validate_state(state_path, scan_path=None, project_root=None):
    """主校验函数。返回 (errors, warnings, report_lines)。"""
    errors = []
    warnings = []
    report = []

    state_path = Path(state_path).resolve()
    if not state_path.exists():
        errors.append(f"STATE.md 文件不存在: {state_path}")
        return errors, warnings, report

    if project_root is None:
        project_root = Path.cwd()
    else:
        project_root = Path(project_root).resolve()

    text = state_path.read_text(encoding='utf-8')
    report.append(f"=== STATE.md 预检报告 ===")
    report.append(f"文件: {state_path}")
    report.append(f"大小: {len(text)} 字节, {text.count(chr(10)) + 1} 行")
    report.append("")

    # 1. 必备段检查
    report.append("--- 1. 必备段检查 ---")
    missing_sections = check_required_sections(text)
    if missing_sections:
        errors.append(f"STATE.md 缺失必备段: {missing_sections}")
        report.append(f"❌ 缺失段: {', '.join(missing_sections)}")
    else:
        report.append(f"✅ 必备段齐全: {', '.join(REQUIRED_SECTIONS)}")
    report.append("")

    # 2. 路径引用存在性检查
    report.append("--- 2. 路径引用存在性检查 ---")
    path_refs = extract_path_refs(text)
    report.append(f"提取到 {len(path_refs)} 个路径引用")
    missing_refs = []
    existing_refs = []
    template_refs = []
    for ref in path_refs:
        if is_template_path(ref):
            template_refs.append(ref)
            continue
        # 尝试相对于 project_root 解析
        ref_path = (project_root / ref).resolve()
        if ref_path.exists():
            existing_refs.append(ref)
        else:
            # 也尝试相对于 STATE.md 所在目录
            alt_path = (state_path.parent / ref).resolve()
            if alt_path.exists():
                existing_refs.append(ref)
            else:
                missing_refs.append(ref)
    if missing_refs:
        errors.append(f"STATE.md 引用不存在的文件: {missing_refs}")
        report.append(f"❌ 缺失引用 ({len(missing_refs)} 个):")
        for ref in missing_refs:
            report.append(f"   - {ref}")
    else:
        report.append(f"✅ 所有 {len(path_refs) - len(template_refs)} 个字面路径引用均存在")
    if template_refs:
        report.append(f"ℹ️ 跳过 {len(template_refs)} 个模板路径引用")
    report.append("")

    # 3. 自述"已创建"文件存在性检查
    report.append("--- 3. 自述已创建文件检查 ---")
    created_files = extract_created_files(text)
    report.append(f"提取到 {len(created_files)} 个自述已创建文件")
    missing_created = []
    for cf in created_files:
        if is_template_path(cf):
            continue
        cf_path = (project_root / cf).resolve()
        if not cf_path.exists():
            alt_path = (state_path.parent / cf).resolve()
            if not alt_path.exists():
                missing_created.append(cf)
    if missing_created:
        warnings.append(f"STATE.md 自述已创建但实际不存在: {missing_created}")
        report.append(f"⚠️ 自述已创建但缺失 ({len(missing_created)} 个):")
        for cf in missing_created:
            report.append(f"   - {cf}")
    else:
        if created_files:
            report.append(f"✅ 所有 {len(created_files)} 个自述已创建文件均存在")
        else:
            report.append(f"ℹ️ 未检测到自述已创建文件语句")
    report.append("")

    # 4. Open issue 与 scan.md 一致性检查
    report.append("--- 4. Open Issue 与 scan.md 一致性检查 ---")
    open_issues = extract_open_issues(text)
    report.append(f"STATE.md Open 列表提取到 {len(open_issues)} 个 issue: {open_issues}")

    if not scan_path:
        # 尝试自动查找 scan.md
        scan_path = _find_scan_md(state_path, project_root)

    if scan_path:
        scan_path = Path(scan_path).resolve()
        report.append(f"scan.md 路径: {scan_path}")
        if scan_path.exists():
            scan_text = scan_path.read_text(encoding='utf-8')
            scan_issues = extract_scan_issues(scan_text)
            report.append(f"scan.md 提取到 {len(scan_issues)} 个 issue")

            # 找孤儿 issue（在 STATE Open 但不在 scan）
            orphan_issues = [i for i in open_issues if i not in scan_issues]
            if orphan_issues:
                warnings.append(
                    f"STATE.md Open 列表含未在 scan.md 中找到的 issue: {orphan_issues}"
                )
                report.append(f"⚠️ 孤儿 issue ({len(orphan_issues)} 个，在 STATE Open 但 scan.md 未找到):")
                for oi in orphan_issues:
                    report.append(f"   - {oi}")
            else:
                if open_issues:
                    report.append(f"✅ 所有 {len(open_issues)} 个 Open issue 均在 scan.md 中找到对应条目")
                else:
                    report.append(f"ℹ️ STATE.md Open 列表为空，跳过孤儿检查")
        else:
            warnings.append(f"指定的 scan.md 不存在: {scan_path}")
            report.append(f"⚠️ scan.md 不存在: {scan_path}（跳过孤儿检查）")
    else:
        warnings.append("未提供 --scan 且无法自动定位 scan.md，跳过孤儿 issue 检查")
        report.append(f"⚠️ 未提供 --scan 且无法自动定位 scan.md（跳过孤儿检查）")
    report.append("")

    # 5. Per-Doc Status 表中引用的 scan 文件存在性
    report.append("--- 5. Per-Doc Status 引用文件检查 ---")
    per_doc_files = _extract_per_doc_scan_files(text)
    if per_doc_files:
        report.append(f"Per-Doc Status 表引用 {len(per_doc_files)} 个 scan 文件")
        missing_per_doc = []
        for pf in per_doc_files:
            pf_path = (project_root / pf).resolve()
            if not pf_path.exists():
                missing_per_doc.append(pf)
        if missing_per_doc:
            warnings.append(f"Per-Doc Status 引用的 scan 文件不存在: {missing_per_doc}")
            report.append(f"⚠️ 缺失 ({len(missing_per_doc)} 个):")
            for pf in missing_per_doc:
                report.append(f"   - {pf}")
        else:
            report.append(f"✅ 所有 {len(per_doc_files)} 个 Per-Doc Status 引用文件均存在")
    else:
        report.append(f"ℹ️ Per-Doc Status 表未引用具体 scan 文件（或表为空）")
    report.append("")

    return errors, warnings, report


def _find_scan_md(state_path, project_root):
    """尝试从 STATE.md 路径推断 scan.md 位置。

    推断规则：
      .review/trae/{module}/STATE.md
        → .review/trae/{module}/scans/ 下最新的 *-scan.md
      .review/claude/{module}/STATE.md
        → .review/claude/{module}/{doc-stem}/scan.md（取第一个存在的）
      .review/codex/{module}/STATE.md
        → .review/codex/{module}/{doc-stem}/scan.md（取第一个存在的）
    """
    state_path = Path(state_path)
    parts = state_path.parts
    if '.review' not in parts:
        return None

    review_idx = parts.index('.review')
    if review_idx + 2 >= len(parts):
        return None

    tool = parts[review_idx + 1]  # trae, claude, or codex
    module = parts[review_idx + 2]

    if tool == 'trae':
        scans_dir = project_root / '.review' / 'trae' / module / 'scans'
        if scans_dir.exists():
            scan_files = sorted(scans_dir.glob('*-scan.md'), key=lambda p: p.stat().st_mtime, reverse=True)
            if scan_files:
                return scan_files[0]
    elif tool in {'claude', 'codex'}:
        tool_module_dir = project_root / '.review' / tool / module
        if tool_module_dir.exists():
            for doc_dir in sorted(tool_module_dir.iterdir()):
                if doc_dir.is_dir():
                    scan_file = doc_dir / 'scan.md'
                    if scan_file.exists():
                        return scan_file
    return None


def _extract_per_doc_scan_files(text):
    """从 Per-Doc Status 表中提取 scan 文件名，并尝试解析为路径。

    表格格式:
    | Doc | Last scan | Agent | P0 | P1 | P2 | VERIFY? |
    |-----|-----------|-------|----|----|----|---------|
    | 03-kmain-cstart | 2026-06-19 | glm | 0 | 1 | 3 | ❌ |

    "产出" 列在 Session Status 表中，这里我们扫描 Per-Doc Status 附近的
    {doc-stem}-{agent}-scan.md 引用。
    """
    files = set()
    # 直接扫描文本中的 *-scan.md 引用
    scan_ref_pattern = re.compile(
        r'((?:\.review/)?(?:trae|claude|codex)/[^\s`"\'<>\]\|]+-scan\.md|'
        r'[a-z0-9_-]+-[a-z]+-scan\.md)',
        re.IGNORECASE
    )
    for match in scan_ref_pattern.finditer(text):
        files.add(match.group(1))
    return sorted(files)


# ===== 主入口 =====

def main():
    parser = argparse.ArgumentParser(
        description='Minix-RS Review STATE.md 预检与一致性校验'
    )
    parser.add_argument(
        '--state',
        required=True,
        help='STATE.md 文件路径'
    )
    parser.add_argument(
        '--scan',
        help='对应的 scan.md 路径（不提供则自动推断）'
    )
    parser.add_argument(
        '--project-root',
        help='项目根目录（默认当前工作目录）'
    )
    parser.add_argument(
        '--strict',
        action='store_true',
        help='严格模式：WARNING 也视为失败（退出码 1）'
    )
    args = parser.parse_args()

    project_root = Path(args.project_root).resolve() if args.project_root else Path.cwd()

    errors, warnings, report = validate_state(
        state_path=args.state,
        scan_path=args.scan,
        project_root=project_root
    )

    # 输出报告
    print('\n'.join(report))

    # 总结
    print("=== 总结 ===")
    if errors:
        print(f"❌ FAIL: {len(errors)} 个 ERROR")
        for e in errors:
            print(f"   ERROR: {e}")
    else:
        print("✅ PASS: 无 ERROR")

    if warnings:
        print(f"⚠️ WARN: {len(warnings)} 个 WARNING")
        for w in warnings:
            print(f"   WARN: {w}")
    else:
        print("✅ 无 WARNING")

    # 退出码
    if errors:
        return 1
    if warnings and args.strict:
        return 1
    if warnings:
        return 2
    return 0


if __name__ == '__main__':
    sys.exit(main())
