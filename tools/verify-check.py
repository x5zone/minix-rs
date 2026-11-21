#!/usr/bin/env python3
"""
verify-check.py — Minix-RS Review 独立验证（Gate G）辅助脚本

辅助 VERIFY-CROSS 阶段（Step 5.6）执行机械化检查，生成 VERIFY-CHECK.md 骨架。
**不替代人工判定**——仅完成可自动化的部分（抽样、Gate 证据检查、Issue 提取），
人工 reviewer 需在生成的骨架上补充"反向验证"判定。

功能（对应 improve-v2 §1.4.2 VERIFY-CROSS 步骤）:
  1. 从 scan.md 提取 Issue List，随机抽样 20%
  2. 检查 scan.md 是否含 9 个 Gate 0 必备锚段
  3. 检查每个 Gate 的 gate-evidence-{X} 块是否存在且含关键字
  4. 从 SYMBOLS.md 抽样 20% 符号，供 reviewer 检查覆盖
  5. 生成 VERIFY-CHECK.md 骨架，含待人工填写的"反向验证"列

用法:
    python3 tools/verify-check.py \
        --scan .review/trae/{stage}/scans/{doc-stem}-{agent}-scan.md \
        --state .review/trae/{stage}/STATE.md \
        --symbols .review/trae/{stage}/scans/{doc-stem}-{agent}-SYMBOLS.md \
        --output .review/trae/{stage}/VERIFY-CHECK.md

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
import subprocess
from pathlib import Path
from datetime import datetime


# ===== Gate 0 必备锚段 =====
GATE0_ANCHORS = [
    "## Skill Invocation Log",
    "## Blocker Gates Status",
    "## Step 0: 预检结果",
    "## Step 1: C Source Ground Truth Lookup",
    "## Step 1.5: Coverage Enumeration",
    "## Step 2: Diff Extraction",
    "## Step 3.5: Precision Check",
    "## Issue List",
    "## Artifact Inventory",
]

# ===== Gate 证据分级 =====
# L1 = 工具输出；L2 = 手工 grep；L3 = 推断。注册表给出每门的最低级别；
# A/D/E/G10–G13 必须有 L1；L3 仅 B/C/D-6/H 允许，且必须写 MANUAL_FALLBACK 理由。
GATE_MIN_LEVEL = {
    "A": "L1",
    "B": "L2",
    "C": "L2",
    "D": "L1",
    "D-6": "L2",
    "E": "L1",
    "G": "L2",
    "H": "L2",
    "G10": "L1",
    "G11": "L1",
    "G12": "L1",
    "G13": "L1",
}
GATE_L3_ALLOWED = {"B", "C", "D-6", "H"}
LEVEL_RANK = {"L1": 3, "L2": 2, "L3": 1}

# ===== Gate 证据块关键字 =====
GATE_EVIDENCE_KEYWORDS = {
    "A": ["coverage-extract.py", "Coverage Summary"],
    "B": ["行为契约"],  # 8 字段 × 5 函数
    "C": [],  # Precision Check 表；另由结构判据要求 meta-rule/L459
    "D": [],  # 5 项 P0 必检 rg 命令；另由结构判据要求 >=5 条
    "D-6": [],  # structure.md 评审表；另由结构判据要求 structure.md
    "E": [],  # 测试函数 grep；另由结构判据要求 fn test_* 或 N/A+理由
    "G": [],  # VERIFY-CHECK 路径；另由结构判据解析 consistency >=90%
    "H": [],  # H.2–H.5 语义矩阵
    "G10": ["doc-publication-check.py"],
    "G11": ["code-publication-check.py"],
    "G12": ["todo-ledger.py"],
    "G13": ["tutorial-rubric-check.py"],
}


def gate_block_extra_missing(gate: str, content: str) -> list[str]:
    """Gate 证据块的结构判据：空围栏/warning 态不能冒充通过。"""
    extra: list[str] = []
    if gate == "B":
        if content.count("|") < 10 or "行为契约" not in content:
            extra.append("行为契约表（8 字段 × 5 函数）")
    elif gate == "C":
        if not re.search(r"meta-rule|L459|Precision", content, re.I):
            extra.append("Precision/meta-rule 判据")
        if content.count("|") < 3:
            extra.append("Precision 检查表（表格行不足）")
    elif gate == "D":
        cmds = [ln for ln in content.splitlines()
                if re.match(r"\s*(?:\$\s*)?(rg|grep)\s", ln)]
        if len(cmds) < 5:
            extra.append(f"至少 5 条 rg/grep 命令（当前 {len(cmds)}）")
    elif gate == "D-6":
        if "structure.md" not in content:
            extra.append("structure.md 评审表")
        if content.count("|") < 10:
            extra.append("structure.md 12 节评审表（表格行不足）")
    elif gate == "E":
        has_fn = re.search(r"\bfn\s+test_\w+", content) is not None
        has_na = "N/A" in content and re.search(r"[：:]\s*\S", content) is not None
        if not (has_fn or has_na):
            extra.append("fn test_* 命中或 N/A+理由")
    elif gate == "G":
        m = re.search(r"consistency\D{0,12}(\d+(?:\.\d+)?)\s*%", content, re.I)
        if not m:
            extra.append("consistency NN%")
        elif float(m.group(1)) < 90:
            extra.append(f"consistency {m.group(1)}% < 90%")
    elif gate == "H":
        if not all(x in content for x in ("H.2", "H.3", "H.4", "H.5")):
            extra.append("H.2–H.5 语义矩阵")
    elif gate == "G10":
        if not re.search(r"0 处|无新增违规|exit\s*0", content):
            extra.append("doc-publication 的 0 命中/exit 0 输出")
    elif gate == "G11":
        if not re.search(r"0 处|无新增违规|exit\s*0", content):
            extra.append("code-publication 的 0 命中/exit 0 输出")
    elif gate == "G12":
        if not re.search(r"open\s*=\s*0|todo-ledger check: OK|exit\s*0", content):
            extra.append("todo-ledger 的 open=0/OK 输出")
    elif gate == "G13":
        if not re.search(r"0 章缺项|0 缺项|exit\s*0", content):
            extra.append("tutorial-rubric 的 0 缺项/exit 0 输出")
    return extra

# ===== Issue 提取模式 =====
# 匹配 Issue List 表格行: | P1-1 | P1 | L459 | ... |
ISSUE_ROW_PATTERN = re.compile(
    r'^\|\s*`?(P[012]-[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*)`?\s*\|\s*`?(P[012])`?\s*\|',
    re.MULTILINE
)

# 匹配列表项: - P1-1 / ### P1-1
ISSUE_LIST_PATTERN = re.compile(
    r'(?:^|\n)(?:#+\s*)?(?:-\s*)?`?(P[012]-[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*)`?\b',
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
    """检查 scan.md 是否含 9 个 Gate 0 必备锚段。"""
    missing = []
    for anchor in GATE0_ANCHORS:
        if anchor not in scan_text:
            missing.append(anchor)
    return missing


KNOWN_SKILLS = {
    "review-agent-ide",
    "review-agent-trigger",
    "review-code-skill",
    "review-core-semantics-skill",
    "review-coverage-skill",
    "review-doc-skill",
    "review-excellence-skill",
    "review-implementation-skill",
    "review-loop",
    "review-patterns-skill",
    "review-process-skill",
    "review-scan",
    "review-socratic-skill",
    "full-review",
    "style-fix",
    "code-excellence",
    "test-audit",
    "todo-fix",
    "style-bible",
}


def check_skill_invocation_log(scan_text):
    """校验 Skill Invocation Log 的表体，而不只是标题存在。

    这是形状检查：表格列、至少一行、Skill 名在已注册技能集合内、且没有
    “规则已加载 / 计划调用”这类未实际调用的措辞。它不能证明运行时的确调用了
    Skill 工具（那需要运行时事件），但能挡住空表和自造技能名。
    """
    if re.search(r"(?m)^## Skill Invocation Log\b", scan_text) and not re.search(
        r"(?m)^## Skill Invocation Log\s*$", scan_text
    ):
        return ["Skill Invocation Log 标题后不得加后缀（必须是 `## Skill Invocation Log`）"]
    m = re.search(
        r"(?ms)^## Skill Invocation Log\s*$(.*?)(?=^## |\Z)", scan_text
    )
    if not m:
        return []  # 锚段缺失由 check_gate0_anchors 报
    body = m.group(1)
    errors = []
    if re.search(r"规则已加载|计划调用|待调用|拟调用", body):
        errors.append("Skill Invocation Log 含未实际调用的措辞（规则已加载/计划调用/待调用）")
    lines = [ln.strip() for ln in body.splitlines() if ln.strip().startswith("|")]
    if len(lines) < 2:
        errors.append("Skill Invocation Log 没有表头/分隔行")
        return errors
    sep = [c.strip() for c in lines[1].strip("|").split("|")]
    if not all(re.fullmatch(r":?-{2,}:?", c) for c in sep if c):
        errors.append("Skill Invocation Log 表头后缺分隔行")
        return errors
    if len(lines) < 3:
        errors.append("Skill Invocation Log 没有数据行")
        return errors
    header = [c.strip().strip("`") for c in lines[0].strip("|").split("|")]
    for required in ("Skill", "调用时机", "关键产出"):
        if required not in header:
            errors.append(f"Skill Invocation Log 表头缺 `{required}` 列")
    if len(header) < 3:
        return errors
    skill_idx = header.index("Skill") if "Skill" in header else -1
    if skill_idx < 0:
        return errors
    for line in lines[2:]:
        cells = [c.strip().strip("`") for c in line.strip("|").split("|")]
        if len(cells) <= skill_idx:
            errors.append(f"Skill Invocation Log 行列数不足: {line}")
            continue
        name = cells[skill_idx]
        if not name:
            errors.append("Skill Invocation Log 有空的 Skill 名")
        elif name not in KNOWN_SKILLS:
            errors.append(f"Skill Invocation Log 里的技能名未注册: {name}")
    return errors


def check_gate_evidence_blocks(scan_text):
    """检查每个 Gate 的 gate-evidence-{X} 块是否存在且含关键字。"""
    results = {}
    for gate, keywords in GATE_EVIDENCE_KEYWORDS.items():
        block_pattern = re.compile(
            rf'```gate-evidence-{re.escape(gate)}\b(.*?)```',
            re.DOTALL
        )
        # R7-D4：scan.md 正文可能「提到」gate-evidence-X（裸围栏/示例），旧实现取首个匹配，
        # 提到即造空块 → 误判 FAIL。改为取**内容最长**的那个块（真证据块总是最长的）。
        matches = list(block_pattern.finditer(scan_text))
        match = max(matches, key=lambda m: len(m.group(1))) if matches else None
        if not match:
            results[gate] = {"exists": False, "keywords_ok": False, "missing_kw": keywords}
            continue
        block_content = match.group(1)
        missing_kw = [kw for kw in keywords if kw not in block_content]
        missing_kw += gate_block_extra_missing(gate, block_content)
        first_line = next(
            (ln.strip() for ln in block_content.splitlines() if ln.strip()), ""
        )
        level_match = re.match(r"level:\s*(L[123])\b", first_line)
        if not level_match:
            missing_kw.append("level: L1/L2/L3（必须是证据块首行）")
        else:
            level = level_match.group(1)
            minimum = GATE_MIN_LEVEL.get(gate, "L2")
            if level == "L3":
                if gate not in GATE_L3_ALLOWED:
                    missing_kw.append(f"Gate {gate} 不允许 L3 推断证据")
                elif not re.search(
                    r"MANUAL_FALLBACK\s*[:：]\s*\S+", block_content
                ):
                    missing_kw.append("L3 需 `MANUAL_FALLBACK: <理由>`")
            elif LEVEL_RANK[level] < LEVEL_RANK[minimum]:
                missing_kw.append(f"证据级别 {level} 低于 Gate {gate} 的最低级别 {minimum}")
            if level == "L1":
                if not re.search(
                    r"(?m)^\s*[\$>]?\s*(?:rg|grep|cargo|python3\s+tools/|bash\s+tools/|\./tools/|[\w./-]+\.(?:py|sh))",
                    block_content,
                ):
                    missing_kw.append("L1 需命令+输出（块内找不到工具命令）")
                if not re.search(r"exit\s*\d|stdout|输出|PASS|FAIL|命中|\d+\s*(?:处|个|行)", block_content):
                    missing_kw.append("L1 需输出/exit code（块内只有命令或关键字）")
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
    # 符号行的真实形态是「| `sym` | file.c:NN | ...」（coverage-extract.py 的输出带反引号）。
    # 旧写法不接受反引号，于是整张符号清单一条都抽不到，只从汇总表里捞到一个统计行标题
    # （如 Gaps），Gate G 的符号覆盖抽样于是静默空转 —— 报告照样写「已抽样验证」。
    symbol_pattern = re.compile(
        r'^\|\s*`?([A-Za-z_][A-Za-z0-9_]*)`?\s*\|\s*([^|]+)\|',
        re.MULTILINE
    )
    HEADER_WORDS = {'Symbol', '符号', 'Name', 'Function', 'C 符号', '指标', '数值', '状态', 'Gaps', 'Rust', '路径'}
    symbols = []
    seen = set()
    for match in symbol_pattern.finditer(symbols_text):
        sym, second = match.group(1), match.group(2).strip()
        if sym in seen or sym in HEADER_WORDS:
            continue
        # 第二列必须是位置引用（含 : 行号或文件后缀），否则是汇总/统计行
        if not re.search(r'[.:]', second):
            continue
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
    issue_ratio=0.2,
    symbol_ratio=0.2,
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
    lines.append(f"- **抽样比例**: {issue_ratio} issues, {symbol_ratio} symbols（与命令行参数一致）")
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
        lines.append(f"✅ **PASS** — 9 个必备锚段齐全")
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
    lines.append(f"- [ ] Blocker Gates 0/A/B/C/D/D-6/E/G/H 均真实通过（非自报）")
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

ROOT = Path(__file__).resolve().parents[1]


def _latest_snapshot(hits: list[Path]) -> Path | None:
    if not hits:
        return None
    def key(p: Path) -> int:
        m = re.search(r"\.v(\d+)\.md$", p.name)
        return int(m.group(1)) if m else -1
    return sorted(hits, key=key)[-1]


def _outside_fences(text: str) -> str:
    out: list[str] = []
    fenced = False
    for line in text.splitlines():
        if line.strip().startswith("```"):
            fenced = not fenced
            continue
        if not fenced:
            out.append(line)
    return "\n".join(out)


def _section_body(text: str, heading: str) -> str:
    m = re.search(rf"(?ms)^{re.escape(heading)}\s*$(.*?)(?=^### |^## |\Z)", text)
    return m.group(1).strip() if m else ""


def _validate_resume_pointers(section: str, scan_path: Path | None) -> list[str]:
    """C4：`### 指针` 里的 `path:line` 必须指向真实文件和真实行号。"""
    errors: list[str] = []
    refs = []
    for m in re.finditer(
        r"`?([A-Za-z0-9_./-]+\.(?:md|rs|py|sh|ya?ml)):([^`\s|)]+)`?", section
    ):
        raw, line_text = m.group(1), m.group(2)
        if not re.fullmatch(r"\d+", line_text):
            errors.append(f"指针行号格式非法: {raw}:{line_text}")
            continue
        refs.append((raw, line_text))
    if not refs:
        return errors + ["指针段没有 `path:line` 形式的可核验指针"]
    for raw, line_text in refs:
        line_no = int(line_text)
        candidates = []
        p = Path(raw)
        if p.is_absolute():
            candidates.append(p)
        else:
            if scan_path is not None:
                candidates.append(scan_path.parent / raw)
            candidates.append(ROOT / raw)
        found = next((c for c in candidates if c.is_file()), None)
        if found is None:
            errors.append(f"指针文件不存在: {raw}")
            continue
        try:
            count = len(found.read_text(encoding="utf-8", errors="replace").splitlines())
        except OSError as exc:
            errors.append(f"指针文件不可读: {raw}（{exc}）")
            continue
        if line_no < 1 or line_no > count:
            errors.append(f"指针行号越界: {raw}:{line_no}（文件共 {count} 行）")
    return errors


def resume_point_missing(text: str, scan_path: Path | None = None) -> list[str]:
    clean = _outside_fences(text)
    if not re.search(r"(?m)^## Resume Point\b", clean):
        return []
    required = ("### 当前断点", "### 下一 session 启动协议", "### 指针", "### 必跑命令")
    missing: list[str] = []
    for sec in required:
        if sec not in clean or len(_section_body(clean, sec)) < 20:
            missing.append(sec)
    for cmd in ("--preflight", "eval-trace-check.py", "todo-ledger.py"):
        if cmd not in clean:
            missing.append(cmd)
    missing += _validate_resume_pointers(_section_body(clean, "### 指针"), scan_path)
    return missing


def cmd_preflight(tool: str, stage: str, doc_stem: str, agent: str) -> int:
    """review 启动前的 5 组产物存在性检查（原 review-gate-check.sh 的合并入口）。"""
    nn_match = re.match(r"(\d+)", doc_stem)
    nn = nn_match.group(1) if nn_match else doc_stem
    base = ROOT / ".review" / tool / stage
    docdir = base / doc_stem
    if tool == "trae":
        scans = base / "scans"
        def scan_artifact(kind: str) -> Path | None:
            direct = scans / f"{doc_stem}-{agent}-{kind}.md"
            if direct.is_file():
                return direct
            hits = sorted(scans.glob(f"{doc_stem}-*-{agent}-{kind}.md"))
            return hits[0] if hits else None
        scan = scan_artifact("scan")
        structure = scan_artifact("structure")
        symbols = scan_artifact("SYMBOLS")
    else:
        scan = docdir / "scan.md"
        structure = docdir / "structure.md"
        symbols = docdir / "SYMBOLS.md"
    verify = next((p for p in (docdir / "VERIFY-CHECK.md", base / "VERIFY-CHECK.md",
                               base / f"VERIFY-CHECK-{nn}.md") if p.is_file()), None)
    groups = [
        ("STATE.md", [base / "STATE.md"] if (base / "STATE.md").is_file() else []),
        ("scan.md", [scan] if scan and scan.is_file() else []),
        ("structure.md", [structure] if structure and structure.is_file() else []),
        ("SYMBOLS.md", [symbols] if symbols and symbols.is_file() else []),
        ("Gate G VERIFY-CHECK.md", [verify] if verify else []),
    ]
    print(f"=== Review Preflight ===\ntool={tool} stage={stage} doc-stem={doc_stem} agent={agent}")
    failures = 0
    for name, hits in groups:
        if hits:
            shown = _latest_snapshot(hits) or hits[0]
            try:
                shown = shown.relative_to(ROOT)
            except ValueError:
                pass
            print(f"[PASS] {name}: {shown}")
        else:
            failures += 1
            print(f"[FAIL] {name}: 缺失")
    scan_path = scan if scan and scan.is_file() else None
    state_path = base / "STATE.md"
    if state_path.is_file():
        cmd = [
            sys.executable,
            str(ROOT / "tools" / "review-state-validate.py"),
            "--state",
            str(state_path),
            "--strict",
        ]
        if scan_path:
            cmd += ["--scan", str(scan_path)]
        state_check = subprocess.run(cmd, capture_output=True, text=True)
        if state_check.returncode != 0:
            failures += 1
            print("[FAIL] STATE 预检: review-state-validate.py 未通过")
            print((state_check.stdout + state_check.stderr).strip()[:800])
        else:
            print("[PASS] STATE 预检: review-state-validate.py 通过")
    if scan_path:
        text = scan_path.read_text(encoding="utf-8", errors="replace")
        skill_errors = check_skill_invocation_log(text)
        if skill_errors:
            failures += 1
            print("[FAIL] Skill Invocation Log: " + "; ".join(skill_errors))
        else:
            print("[PASS] Skill Invocation Log: 表体与技能名可解析")
        if "关联代码清单" not in text:
            failures += 1
            print("[FAIL] B4.1: scan.md §Step 0 预检结果缺「关联代码清单」")
        else:
            print("[PASS] B4.1: 关联代码清单存在")
        if "gate-evidence-code" not in text:
            failures += 1
            print("[FAIL] B4.1: scan.md 缺 gate-evidence-code 块（或 N/A + 理由）")
        else:
            print("[PASS] B4.1: gate-evidence-code 块存在")
        missing_artifacts = []
        for m in re.finditer(r"^artifact:\s*(\S+)", text, re.MULTILINE):
            art = m.group(1).strip().strip("（）()")
            if not art or art == "none":
                continue
            ap = Path(art)
            if not ap.is_absolute():
                ap = ROOT / ap
            if not ap.exists():
                missing_artifacts.append(art)
        if missing_artifacts:
            failures += 1
            print(f"[FAIL] D4: gate-evidence artifact 不存在：{', '.join(missing_artifacts)}")
        else:
            print("[PASS] D4: gate-evidence artifact 路径全部存在（或 none）")
        missing_resume = resume_point_missing(text, scan_path)
        if missing_resume:
            failures += 1
            print(f"[FAIL] Resume Point 缺/空：{', '.join(missing_resume)}")
        elif re.search(r"(?m)^## Resume Point\b", _outside_fences(text)):
            print("[PASS] Resume Point 四段 + 必跑命令齐全")
    if failures:
        print(f"preflight: FAIL（{failures} 项）")
        return 1
    print("preflight: PASS（5 组产物 + scan 内容级检查）")
    return 0


def cmd_finalize(path: Path) -> int:
    """从已填的 §2.1 Issue 反向验证表计算 Gate G 一致性；拒绝残留（待填）。"""
    if not path.is_file():
        print(f"finalize: 文件不存在：{path}", file=sys.stderr)
        return 2
    text = path.read_text(encoding="utf-8")
    m = re.search(r"### 2\.1 Issue 反向验证表(.*?)(?=\n### |\n## |\Z)", text, re.S)
    if not m:
        print("finalize: 找不到 §2.1 Issue 反向验证表", file=sys.stderr)
        return 1
    rows: list[list[str]] = []
    for line in m.group(1).splitlines():
        if not line.strip().startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 5:
            continue
        if cells[0] in ("Issue ID", "---------") or set(cells[0]) <= {"-", ":"}:
            continue
        rows.append(cells)
    if not rows:
        print("finalize: §2.1 没有可判定的 issue 行", file=sys.stderr)
        return 1
    for i, cells in enumerate(rows, 1):
        if any("待填" in c for c in cells):
            print(f"finalize: §2.1 第 {i} 行仍有（待填）", file=sys.stderr)
            return 1
    ok_values = {"是", "✅", "一致", "yes", "Y", "ok", "OK"}
    matches = sum(1 for cells in rows if cells[3] in ok_values)
    total = len(rows)
    pct = matches * 100.0 / total
    text = re.sub(r"^- 一致 issue 数: .*$", f"- 一致 issue 数: {matches}", text, flags=re.M)
    text = re.sub(r"^- 一致性: .*$", f"- 一致性: {pct:.1f}%", text, flags=re.M)
    path.write_text(text, encoding="utf-8")
    print(f"finalize: 一致 {matches}/{total} = {pct:.1f}%")
    return 0 if pct >= 90.0 else 1


def main():
    parser = argparse.ArgumentParser(
        description='Minix-RS Review 独立验证（Gate G）辅助脚本'
    )
    parser.add_argument(
        '--scan',
        help='scan.md 文件路径（--preflight/--finalize 时不需要）'
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
        '--units',
        help='单元状态 YAML（V3.9：要求 scan.md 含 ## Unit Ledger）'
    )
    parser.add_argument(
        '--self-check',
        action='store_true',
        help='VERIFY-SELF 模式：仅输出机械化检查结果，不生成 VERIFY-CHECK.md'
    )
    parser.add_argument(
        '--preflight',
        action='store_true',
        help='检查 5 组 review 产物是否存在（原 review-gate-check.sh）'
    )
    parser.add_argument('--tool', default='zcode', help='运行时：trae/claude/codex/zcode/qoder')
    parser.add_argument('--stage', help='stage 目录名，如 01-stage-kernel')
    parser.add_argument('--doc-stem', help='文档 stem，如 01-boot-shim-bootstrap')
    parser.add_argument('--agent', default='glm', help='Trae 扁平布局的 agent 名')
    parser.add_argument(
        '--finalize',
        metavar='VERIFY_CHECK_MD',
        help='从已填的 §2.1 表计算一致性并写回 §4.1'
    )
    args = parser.parse_args()

    if args.preflight:
        if not args.stage or not args.doc_stem:
            parser.error("--preflight 需要 --stage 与 --doc-stem")
        return cmd_preflight(args.tool, args.stage, args.doc_stem, args.agent)
    if args.finalize:
        return cmd_finalize(Path(args.finalize))
    if not args.scan:
        parser.error("--scan 必填（除非使用 --preflight/--finalize）")

    scan_path = Path(args.scan).resolve()
    if not scan_path.exists():
        print(f"❌ scan.md 不存在: {scan_path}", file=sys.stderr)
        return 1

    scan_text = scan_path.read_text(encoding='utf-8')

    # 1. Gate 0 锚段检查
    gate0_missing = check_gate0_anchors(scan_text)
    gate0_missing += check_skill_invocation_log(scan_text)

    # 1b. G14 单元账本锚段（仅在传入 --units 时要求）
    units_missing = False
    if args.units:
        up = Path(args.units).resolve()
        if not up.exists():
            print(f"❌ 单元状态不存在: {up}", file=sys.stderr)
            units_missing = True
        elif "## Unit Ledger" not in scan_text:
            print("❌ scan.md 缺 `## Unit Ledger`（V3.9 G14）", file=sys.stderr)
            units_missing = True

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
            print("✅ 9 个锚段齐全")
        print()
        print("--- Unit Ledger ---")
        print("✅ 含 `## Unit Ledger`" if args.units and not units_missing else ("❌ 缺失" if args.units else "ℹ️ 未启用"))
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
        if gate0_missing or units_missing or any(not r["exists"] or not r["keywords_ok"] for r in gate_evidence_results.values()):
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
        issue_ratio=args.sample_ratio,
        symbol_ratio=args.sample_ratio,
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
