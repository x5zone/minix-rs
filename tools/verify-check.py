#!/usr/bin/env python3
"""
verify-check.py — Minix-RS Review 独立验证（Gate G）辅助脚本

辅助 VERIFY-CROSS 阶段（Step 5.6）执行机械化检查，生成 VERIFY-CHECK.md 骨架。
**不替代人工判定**——仅完成可自动化的部分（抽样、Gate 证据检查、Issue 提取），
人工 reviewer 需在生成的骨架上补充"反向验证"判定。

功能（对应 improve-v2 §1.4.2 VERIFY-CROSS 步骤）:
  1. 从 scan.md 提取 Issue List，随机抽样 20%
  2. 检查 scan.md 是否含 8 个 Gate 0 必备锚段
  3. 检查每个 Gate 的 gate-evidence-{X} 块是否存在且含关键字
  4. 从 SYMBOLS.md 抽样 20% 符号，供 reviewer 检查覆盖
  5. 生成 VERIFY-CHECK.md 骨架，含待人工填写的"反向验证"列

用法:
    python3 tools/verify-check.py \
        --scan .review/trae/{module}/scans/{doc-stem}-{agent}-scan.md \
        --state .review/trae/{module}/STATE.md \
        --symbols .review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md \
        --output .review/trae/{module}/VERIFY-CHECK.md

    # 仅自检模式（VERIFY-SELF，不生成文件，只输出检查结果）:
    python3 tools/verify-check.py --scan {scan.md} --self-check

退出码:
    0 = 机械化检查全过（仍需人工反向验证才能定 PASS）
    1 = 机械化检查失败（Gate 证据缺失 / 锚段缺失 → 直接判 CONCERN）
"""

import os
import re
import sys
import random
import argparse
from pathlib import Path
from datetime import datetime


# ===== Gate 0 必备锚段 =====
GATE0_ANCHORS = [
    "## Skill Invocation Log",
    "## Blocker Gates Status",
    "## Step 1: C Source Ground Truth Lookup",
    "## Step 1.5: Coverage Enumeration",
    "## Step 2: Diff Extraction",
    "## Step 3.5: Precision Check",
    "## Issue List",
    "## Artifact Inventory",
]

# ===== Gate 证据块关键字 =====
GATE_EVIDENCE_KEYWORDS = {
    "A": ["coverage-extract.py", "Coverage Summary"],
    "B": ["行为契约"],  # 8 字段 × 5 函数
    "C": [],  # Precision Check 表
    "D": [],  # 5 项 P0 必检 rg 命令
    "D-6": [],  # structure.md 评审表
    "E": [],  # 测试函数 grep
    "G": [],  # VERIFY-CHECK 路径
}

# ===== Issue 提取模式 =====
# 匹配 Issue List 表格行: | P1-1 | P1 | L459 | ... |
ISSUE_ROW_PATTERN = re.compile(
    r'^\|\s*(P[012]-\d+[a-z]?)\s*\|\s*(P[012])\s*\|',
    re.MULTILINE
)

# 匹配列表项: - P1-1 / ### P1-1
ISSUE_LIST_PATTERN = re.compile(
    r'(?:^|\n)(?:#+\s*)?(?:-\s*)?(P[012]-\d+[a-z]?)\b',
    re.MULTILINE
)


# ===== 提取函数 =====

def extract_issues_from_scan(scan_text):
    """从 scan.md 提取 issue 列表，返回 [(id, severity), ...]。"""
    issues = []
    seen = set()
    # 先匹配表格行
    for match in ISSUE_ROW_PATTERN.finditer(scan_text):
        issue_id = match.group(1)
        severity = match.group(2)
        if issue_id not in seen:
            seen.add(issue_id)
            issues.append((issue_id, severity))
    # 再匹配列表项（补充表格外的 issue）
    for match in ISSUE_LIST_PATTERN.finditer(scan_text):
        issue_id = match.group(1)
        severity = 'P' + issue_id[1]  # P0/P1/P2
        if issue_id not in seen:
            seen.add(issue_id)
            issues.append((issue_id, severity))
    return issues


def check_gate0_anchors(scan_text):
    """检查 scan.md 是否含 8 个 Gate 0 必备锚段。"""
    missing = []
    for anchor in GATE0_ANCHORS:
        if anchor not in scan_text:
            missing.append(anchor)
    return missing


def check_gate_evidence_blocks(scan_text):
    """检查每个 Gate 的 gate-evidence-{X} 块是否存在且含关键字。"""
    results = {}
    for gate, keywords in GATE_EVIDENCE_KEYWORDS.items():
        block_pattern = re.compile(
            rf'```gate-evidence-{re.escape(gate)}\b(.*?)```',
            re.DOTALL
        )
        match = block_pattern.search(scan_text)
        if not match:
            results[gate] = {"exists": False, "keywords_ok": False, "missing_kw": keywords}
            continue
        block_content = match.group(1)
        missing_kw = [kw for kw in keywords if kw not in block_content]
        results[gate] = {
            "exists": True,
            "keywords_ok": len(missing_kw) == 0,
            "missing_kw": missing_kw,
        }
    return results


def extract_symbols_from_symbols_md(symbols_text):
    """从 SYMBOLS.md 提取符号名列表（简化版，提取 C 符号名）。"""
    # SYMBOLS.md 通常含表格，符号名在第一列或 | symbol | 形式
    # 这里用简化模式：匹配 C 函数名/结构体名
    symbol_pattern = re.compile(
        r'^\|\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\|',
        re.MULTILINE
    )
    symbols = []
    seen = set()
    for match in symbol_pattern.finditer(symbols_text):
        sym = match.group(1)
        if sym not in seen and sym not in ('Symbol', '符号', 'Name', 'Function', '---'):
            seen.add(sym)
            symbols.append(sym)
    return symbols


def sample_issues(issues, sample_ratio=0.2, seed=None):
    """随机抽样 issue，至少 1 个（若非空）。"""
    if not issues:
        return []
    n = max(1, int(len(issues) * sample_ratio))
    if seed is not None:
        random.seed(seed)
    return random.sample(issues, min(n, len(issues)))


def sample_symbols(symbols, sample_ratio=0.2, seed=None):
    """随机抽样符号，至少 5 个（若非空）。"""
    if not symbols:
        return []
    n = max(5, int(len(symbols) * sample_ratio))
    if seed is not None:
        random.seed(seed)
    return random.sample(symbols, min(n, len(symbols)))


# ===== VERIFY-CHECK.md 生成 =====

def generate_verify_check_md(
    scan_path,
    state_path,
    symbols_path,
    issues,
    sampled_issues,
    sampled_symbols,
    gate0_missing,
    gate_evidence_results,
    sample_seed,
):
    """生成 VERIFY-CHECK.md 骨架。"""
    now = datetime.now().strftime("%Y-%m-%d %H:%M")
    lines = []
    lines.append(f"# VERIFY-CHECK — 独立验证报告")
    lines.append(f"")
    lines.append(f"- **生成时间**: {now}")
    lines.append(f"- **scan.md**: `{scan_path}`")
    lines.append(f"- **STATE.md**: `{state_path}`")
    lines.append(f"- **SYMBOLS.md**: `{symbols_path or '(未提供)'}`")
    lines.append(f"- **抽样种子**: {sample_seed}（可复现）")
    lines.append(f"- **抽样比例**: 20% issues, 20% symbols")
    lines.append(f"")
    lines.append(f"---")
    lines.append(f"")
    lines.append(f"## 1. 机械化检查")
    lines.append(f"")
    lines.append(f"### 1.1 Gate 0 锚段检查")
    if gate0_missing:
        lines.append(f"❌ **FAIL** — 缺失锚段:")
        for anchor in gate0_missing:
            lines.append(f"   - `{anchor}`")
    else:
        lines.append(f"✅ **PASS** — 8 个必备锚段齐全")
    lines.append(f"")
    lines.append(f"### 1.2 Gate 证据块检查")
    lines.append(f"")
    lines.append(f"| Gate | 证据块存在 | 关键字齐全 | 缺失关键字 | 判定 |")
    lines.append(f"|------|-----------|-----------|-----------|------|")
    all_gates_ok = True
    for gate in sorted(gate_evidence_results.keys()):
        r = gate_evidence_results[gate]
        exists = "✅" if r["exists"] else "❌"
        kw_ok = "✅" if r["keywords_ok"] else "❌"
        missing = ", ".join(r["missing_kw"]) if r["missing_kw"] else "—"
        if not r["exists"] or not r["keywords_ok"]:
            verdict = "❌ FAIL"
            all_gates_ok = False
        else:
            verdict = "✅ PASS"
        lines.append(f"| {gate} | {exists} | {kw_ok} | {missing} | {verdict} |")
    lines.append(f"")
    lines.append(f"### 1.3 Issue 统计")
    lines.append(f"")
    lines.append(f"- scan.md 提取到 **{len(issues)}** 个 issue")
    lines.append(f"- 抽样 **{len(sampled_issues)}** 个 issue 供反向验证")
    lines.append(f"")
    lines.append(f"---")
    lines.append(f"")
    lines.append(f"## 2. 反向验证（人工填写）")
    lines.append(f"")
    lines.append(f"> **说明**：对每个抽样 issue，独立重新判定。source evidence 是否充分？判定等级是否合理？")
    lines.append(f"")
    lines.append(f"### 2.1 Issue 反向验证表")
    lines.append(f"")
    lines.append(f"| Issue ID | scan.md 判定 | 独立重新判定 | 一致? | 备注 |")
    lines.append(f"|---------|-------------|------------|-------|------|")
    for issue_id, severity in sampled_issues:
        lines.append(f"| {issue_id} | {severity} | （待填） | （待填） | （待填） |")
    lines.append(f"")
    lines.append(f"### 2.2 符号覆盖抽样验证")
    lines.append(f"")
    if symbols_path:
        lines.append(f"从 SYMBOLS.md 抽样 **{len(sampled_symbols)}** 个符号，验证是否在文档/检查中覆盖:")
        lines.append(f"")
        lines.append(f"| 符号 | 在 scan.md 中提及? | 在文档中覆盖? | 在代码中实现? | 备注 |")
        lines.append(f"|------|-------------------|-------------|-------------|------|")
        for sym in sampled_symbols:
            lines.append(f"| {sym} | （待填） | （待填） | （待填） | （待填） |")
    else:
        lines.append(f"⚠️ 未提供 SYMBOLS.md，跳过符号覆盖抽样")
    lines.append(f"")
    lines.append(f"---")
    lines.append(f"")
    lines.append(f"## 3. 收敛验证（人工填写）")
    lines.append(f"")
    lines.append(f"- [ ] STATE.md Convergence Checklist 中标记 COMPLETE 的维度均真实完成")
    lines.append(f"- [ ] STATE.md Open P0/P1/P2 列表与 scan.md Issue List 一致")
    lines.append(f"- [ ] Blocker Gates 0/A/B/C/D/D-6/E/G 均真实通过（非自报）")
    lines.append(f"")
    lines.append(f"---")
    lines.append(f"")
    lines.append(f"## 4. 判定")
    lines.append(f"")
    lines.append(f"### 4.1 抽样一致性")
    lines.append(f"")
    lines.append(f"- 抽样 issue 数: {len(sampled_issues)}")
    lines.append(f"- 一致 issue 数: （待填）")
    lines.append(f"- 一致性: （待填）%")
    lines.append(f"")
    lines.append(f"### 4.2 最终判定")
    lines.append(f"")
    lines.append(f"- [ ] **PASS** — 一致性 ≥ 90%，无遗漏 key symbols，收敛状态可信，Gates 真实通过")
    lines.append(f"- [ ] **CONCERN** — 一致性 70-90%，特定维度需重新审查")
    lines.append(f"- [ ] **FAIL** — 一致性 < 70% 或发现关键遗漏")
    lines.append(f"")
    lines.append(f"> **注意**：CONCERN/FAIL 不得标记 STATE.md 为 CONVERGED。")
    lines.append(f"")
    lines.append(f"---")
    lines.append(f"")
    lines.append(f"## 5. 机械化检查预判")
    lines.append(f"")
    if gate0_missing or not all_gates_ok:
        lines.append(f"⚠️ 机械化检查未全过（Gate 0 锚段缺失或 Gate 证据块缺失），建议直接判 **CONCERN**。")
        lines.append(f"")
        lines.append(f"缺失项:")
        if gate0_missing:
            lines.append(f"- Gate 0 锚段: {', '.join(gate0_missing)}")
        for gate, r in gate_evidence_results.items():
            if not r["exists"] or not r["keywords_ok"]:
                lines.append(f"- Gate {gate} 证据块: exists={r['exists']}, missing_kw={r['missing_kw']}")
    else:
        lines.append(f"✅ 机械化检查全过，可进入人工反向验证判定 PASS/CONCERN/FAIL。")
    lines.append(f"")
    return "\n".join(lines)


# ===== 主入口 =====

def main():
    parser = argparse.ArgumentParser(
        description='Minix-RS Review 独立验证（Gate G）辅助脚本'
    )
    parser.add_argument(
        '--scan',
        required=True,
        help='scan.md 文件路径'
    )
    parser.add_argument(
        '--state',
        help='STATE.md 文件路径（用于收敛验证）'
    )
    parser.add_argument(
        '--symbols',
        help='SYMBOLS.md 文件路径（用于符号覆盖抽样）'
    )
    parser.add_argument(
        '--output',
        help='输出 VERIFY-CHECK.md 路径（不提供则输出到 stdout）'
    )
    parser.add_argument(
        '--sample-ratio',
        type=float,
        default=0.2,
        help='抽样比例（默认 0.2）'
    )
    parser.add_argument(
        '--seed',
        type=int,
        default=None,
        help='随机抽样种子（可复现）'
    )
    parser.add_argument(
        '--self-check',
        action='store_true',
        help='VERIFY-SELF 模式：仅输出机械化检查结果，不生成 VERIFY-CHECK.md'
    )
    args = parser.parse_args()

    scan_path = Path(args.scan).resolve()
    if not scan_path.exists():
        print(f"❌ scan.md 不存在: {scan_path}", file=sys.stderr)
        return 1

    scan_text = scan_path.read_text(encoding='utf-8')

    # 1. Gate 0 锚段检查
    gate0_missing = check_gate0_anchors(scan_text)

    # 2. Gate 证据块检查
    gate_evidence_results = check_gate_evidence_blocks(scan_text)

    # 3. 提取 issue
    issues = extract_issues_from_scan(scan_text)
    sampled_issues = sample_issues(issues, args.sample_ratio, args.seed)

    # 4. 提取符号
    sampled_symbols = []
    symbols_text = ""
    if args.symbols:
        symbols_path = Path(args.symbols).resolve()
        if symbols_path.exists():
            symbols_text = symbols_path.read_text(encoding='utf-8')
            symbols = extract_symbols_from_symbols_md(symbols_text)
            sampled_symbols = sample_symbols(symbols, args.sample_ratio, args.seed)
        else:
            print(f"⚠️ SYMBOLS.md 不存在: {symbols_path}", file=sys.stderr)

    # VERIFY-SELF 模式：仅输出机械化检查结果
    if args.self_check:
        print("=== VERIFY-SELF 机械化检查 ===")
        print(f"scan.md: {scan_path}")
        print()
        print("--- Gate 0 锚段 ---")
        if gate0_missing:
            print(f"❌ 缺失 {len(gate0_missing)} 个锚段:")
            for a in gate0_missing:
                print(f"   - {a}")
        else:
            print("✅ 8 个锚段齐全")
        print()
        print("--- Gate 证据块 ---")
        for gate in sorted(gate_evidence_results.keys()):
            r = gate_evidence_results[gate]
            status = "✅" if (r["exists"] and r["keywords_ok"]) else "❌"
            print(f"   Gate {gate}: {status} (exists={r['exists']}, missing_kw={r['missing_kw']})")
        print()
        print(f"--- Issue 统计 ---")
        print(f"总 issue 数: {len(issues)}")
        print(f"抽样数 (20%): {len(sampled_issues)}")
        if sampled_issues:
            print(f"抽样 ID: {[i[0] for i in sampled_issues]}")
        print()
        # 自检判定
        if gate0_missing or any(not r["exists"] or not r["keywords_ok"] for r in gate_evidence_results.values()):
            print("❌ VERIFY-SELF FAIL — 机械化检查未过，需修复 Gate 证据")
            return 1
        print("✅ VERIFY-SELF PASS — 机械化检查全过，可进入 VERIFY-CROSS")
        return 0

    # VERIFY-CROSS 模式：生成 VERIFY-CHECK.md
    state_path = Path(args.state).resolve() if args.state else None
    symbols_path = Path(args.symbols).resolve() if args.symbols else None

    verify_check_md = generate_verify_check_md(
        scan_path=scan_path,
        state_path=state_path,
        symbols_path=symbols_path,
        issues=issues,
        sampled_issues=sampled_issues,
        sampled_symbols=sampled_symbols,
        gate0_missing=gate0_missing,
        gate_evidence_results=gate_evidence_results,
        sample_seed=args.seed,
    )

    if args.output:
        output_path = Path(args.output).resolve()
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(verify_check_md, encoding='utf-8')
        print(f"✅ VERIFY-CHECK.md 已生成: {output_path}")
        print(f"   大小: {len(verify_check_md)} 字节")
    else:
        print(verify_check_md)

    # 机械化检查预判
    if gate0_missing or any(not r["exists"] or not r["keywords_ok"] for r in gate_evidence_results.values()):
        print("\n⚠️ 机械化检查未全过，建议直接判 CONCERN", file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
