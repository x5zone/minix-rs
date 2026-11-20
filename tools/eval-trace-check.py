#!/usr/bin/env python3
"""
eval-trace-check.py — review-loop 轨迹完整性检查

检查对象：一篇循环的 run 目录，至少包含
    loop.md
    metrics.md
    defect-ledger.md
    panel/*.md

检查内容：
  1. loop.md 的九个固定二级标题是否齐全，且每节有内容；
  2. 每个 panel/*.md 里的 `## FINDING <ID>` 是否在 defect-ledger.md 有处置行；
  3. 台账是否有孤儿行（处置了但面板里没有）；
  4. 每行是否有处置类（①②④⑤⑥FP 或 fix/transfer/wording/insufficient/refactor/false-positive）；R25+ 拒绝已删除的 ③；
  5. 处置类为 ①/④/⑥ 的行，复验列是否非空且不是“未修/待修/未实施”；
  6. 处置类为 ⑤ 的行是否写清取证路径；FP 是否在 `证据` 列给出反证；② 是否有用户批准 + 日期；
  7. metrics.md 的必需键是否齐全；R25+ 必须有 `未闭环: 0`；
  8. metrics.md 的 `发现总数` 是否等于六项处置之和；
  9. defect-ledger.md 与 metrics.md 是否残留 TBD/待定 占位；
  10. 六步收口合同：loop.md 中编号 ≥ 13 的 `# Round` 小节必须含 `### L6` 与
      `### L6.5` 收口小节（R0–R12 为历史轮次，豁免；合同见
      runs/<stage>/<doc>/DECISIONS-PENDING.md 第 5 项裁决）。

用法：
    python3 tools/eval-trace-check.py <run-dir>
    python3 tools/eval-trace-check.py <run-dir> --json
    python3 tools/eval-trace-check.py --self-test

退出码：
    0 = 全部通过（允许 WARN）
    1 = 有 FAIL
    2 = 用法或文件缺失错误
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path

SECTION_TITLES = [
    "§0 基线",
    "§1 面板",
    "§2 发现",
    "§3 归因",
    "§4 修复",
    "§5 复验",
    "§6 指标",
    "§7 状态",
    "§8 给 meta-review 的不确定点",
]

FINDING_RE = re.compile(r"^#{2,3}\s+FINDING\s+([A-Za-z][A-Za-z0-9_-]*)", re.MULTILINE)
# 新文档（02 起）从 R1 就会使用 V2.5 状态机；不能靠绝对轮号 >=25 判定，
# 否则 02 的 R1 会落回旧 schema，重新允许 ③/无未闭环/无限全量轮。
SCHEMA_RE = re.compile(
    r"^\s*(?:[-*]\s*)?(?:schema|规则版本)\s*[:：]\s*V(\d+)\.(\d+)"
    r"(?:\s*from\s*R(\d+))?",
    re.MULTILINE | re.IGNORECASE,
)


def schema_start_round(text: str | None) -> int | None:
    """Return the first round governed by the V3+ schema markers.

    A layered document carries one marker per schema generation
    (`schema: V3.0 from R26`, then `schema: V3.3 from R31`). Each marker
    governs *from* its round, so the V3 base rules bind from the **minimum**
    `from R<N>` across V3+ markers — a V3.3 marker must not unscope the
    rounds the V3.0 marker already covered. A V2.5→V3 transition document
    keeps the historical newest-version-wins behavior for its V2.5 segment.
    """
    if not text:
        return None
    v3_starts = [
        int(m.group(3) or 1)
        for m in SCHEMA_RE.finditer(text)
        if (int(m.group(1)), int(m.group(2))) >= (3, 0)
    ]
    if v3_starts:
        return min(v3_starts)
    best: tuple[tuple[int, int], int] | None = None
    for m in SCHEMA_RE.finditer(text):
        version = (int(m.group(1)), int(m.group(2)))
        if version < (2, 5):
            continue
        start = int(m.group(3) or 1)
        if best is None or version > best[0] or (
            version == best[0] and start > best[1]
        ):
            best = (version, start)
    return best[1] if best else None


def schema_version(text: str | None) -> tuple[int, int] | None:
    if not text:
        return None
    best: tuple[int, int] | None = None
    for m in SCHEMA_RE.finditer(text):
        version = (int(m.group(1)), int(m.group(2)))
        if best is None or version > best:
            best = version
    return best


def is_v3_schema(text: str | None) -> bool:
    version = schema_version(text)
    return version is not None and version >= (3, 0)


def origin_round(origin: str) -> int | None:
    """Extract the first `R<N>` from an `origin` cell.

    V3 writes `origin: R<首见>` or `origin: R<首见>；last_seen: R<本轮>`.
    The first number is the identity round; the rest is history.
    """
    m = re.search(r"\bR(\d+)\b", origin or "")
    return int(m.group(1)) if m else None


V33_MARKER_RE = re.compile(
    r"schema\s*[:：]\s*V3\.3(?:\s+from\s+R(\d+))?", re.IGNORECASE
)


def v33_start_round(text: str | None, fallback: int | None = None) -> int | None:
    """First round governed by the V3.3 additions (four-state lifecycle,
    confirmation rounds, reader-P3 admission, origin/severity semantics).

    Parsed from a `schema: V3.3 from R<N>` marker; without `from R<N>` the
    V3.3 rules bind from the document's existing schema start.
    """
    if not text:
        return fallback
    best: int | None = None
    for m in V33_MARKER_RE.finditer(text):
        start = int(m.group(1)) if m.group(1) else (fallback or 1)
        if best is None or start < best:
            best = start
    return best if best is not None else fallback


ROUND_TYPE_V33 = {"full", "structural", "confirmation"}
# Round-tagged detailed-section style (fact.md R6 uses `### R6-04　…`): the
# round prefix is part of the ID namespace used in that round's ledger rows.
FINDING_ROUND_RE = re.compile(r"^#{2,3}\s+((?:R|D)\d+-\d+)\s*[　\s]", re.MULTILINE)
METRIC_TOTAL_RE = re.compile(r"^-\s*发现总数\s*[:：]\s*(\d+)", re.MULTILINE)
METRIC_COUNT_RE = re.compile(r"([①②④⑤⑥③]|FP|SUGG)\s*(\d+)")
def _strip_invisible(text: str) -> str:
    """V3.4-r26：白名单过滤器。"""
    import re as _re
    text = _re.sub(r"[\u2010-\u2015\u2212]", "-", text)
    return "".join(
        c
        for c in text
        if (
            0x20 <= ord(c) <= 0x7E
            or 0x4E00 <= ord(c) <= 0x9FFF
            or 0x3400 <= ord(c) <= 0x4DBF
            or 0x3000 <= ord(c) <= 0x303F
            or (0xFF01 <= ord(c) <= 0xFFEF and ord(c) != 0xFFA0)
            or c in '\n\t'
        )
    )


PLACEHOLDERS = {"tbd", "待定", "-", "—", "", "（待补）", "待补", "(待补)", "(待定)"}  # r16：全角归一后括号变半角，两形态都算占位
DISPOSITION_TOKENS = {
    "①": "①",
    "②": "②",
    "④": "④",
    "⑤": "⑤",
    "⑥": "⑥",
    "FP": "FP",
    "fix": "①",
    "transfer": "②",
    "wording": "④",
    "insufficient": "⑤",
    "refactor": "⑥",
    "false-positive": "FP",
    "fp": "FP",
    # Legacy-only token: accepted for rounds before 25, rejected for R25+.
    "③": "LEGACY",
    # V3.3 D-047: triage-out class — reader/design P3 findings routed to
    # suggestions.md. Visible in the ledger, never open, never in G1-G8.
    "SUGG": "SUGG",
    "sugg": "SUGG",
}


class Report:
    def __init__(self) -> None:
        self.items: list[dict] = []

    def add(self, level: str, check: str, message: str) -> None:
        self.items.append({"level": level, "check": check, "message": message})

    def fail(self, check: str, message: str) -> None:
        self.add("FAIL", check, message)

    def warn(self, check: str, message: str) -> None:
        self.add("WARN", check, message)

    def ok(self, check: str, message: str) -> None:
        self.add("PASS", check, message)

    @property
    def failed(self) -> bool:
        return any(item["level"] == "FAIL" for item in self.items)

    def as_dict(self) -> dict:
        return {
            "ok": not self.failed,
            "counts": {
                "pass": sum(1 for i in self.items if i["level"] == "PASS"),
                "warn": sum(1 for i in self.items if i["level"] == "WARN"),
                "fail": sum(1 for i in self.items if i["level"] == "FAIL"),
            },
            "items": self.items,
        }


def read_text(path: Path, report: Report, label: str) -> str | None:
    if not path.is_file():
        report.fail("file", f"缺少 {label}: {path}")
        return None
    return path.read_text(encoding="utf-8")


def check_loop_sections(text: str, report: Report, round_tag: str | None = None) -> None:
    # Round-scoped: a per-round panel must be answered by that round's loop
    # section. Whole-file search lets a deleted round section pass because the
    # same fixed titles exist in older rounds (B-31 residual).
    if round_tag:
        all_headings = [
            (m.start(), m.end(), m.group(0))
            for m in re.finditer(rf"^## .*{re.escape(round_tag)}.*$", text, re.MULTILINE)
        ]
        headings = [(a, b) for a, b, line in all_headings if "面板轮" in line] or [
            (a, b) for a, b, _line in all_headings
        ]
        if not headings:
            report.fail("loop-sections", f"loop.md 缺少 {round_tag} 面板轮小节")
            return
        if headings:
            start = headings[0][0]
            following = re.search(r"^## ", text[headings[0][1]:], re.MULTILINE)
            end = headings[0][1] + following.start() if following else len(text)
            text = text[start:end]
    positions: list[int] = []
    missing: list[str] = []
    for title in SECTION_TITLES:
        marker = f"§{title.lstrip('§')}"
        idx = text.find(marker)
        if idx < 0:
            missing.append(title)
        else:
            positions.append(idx)
    if missing:
        report.fail("loop-sections", "loop.md 缺少固定标题: " + "、".join(missing))
        return
    report.ok("loop-sections", "loop.md 九个固定标题齐全")
    ordered = sorted(positions)
    for i, idx in enumerate(ordered):
        end = ordered[i + 1] if i + 1 < len(ordered) else len(text)
        body = text[idx:end].split("\n", 1)[1] if "\n" in text[idx:end] else ""
        if not body.strip():
            report.fail(
                "loop-sections",
                f"loop.md 的 `{SECTION_TITLES[i]}` 节没有内容",
            )


RANGE_RE = re.compile(r"^([A-Z][A-Za-z0-9-]*?)-(\d+)\.\.(\d+)$")
SEP_RE = re.compile(r"^:?-{2,}:?$")


def expand_id_token(token: str) -> list[str]:
    """Expand `READER-A-01..20` into the individual IDs used by panels.

    Range rows keep multi-round ledgers readable; coverage still needs the
    concrete IDs, otherwise a whole reader family silently escapes the check.
    """
    m = RANGE_RE.match(token)
    if not m:
        return [token]
    prefix, start, end = m.group(1), int(m.group(2)), int(m.group(3))
    width = len(m.group(2))
    return [f"{prefix}-{i:0{width}d}" for i in range(start, end + 1)]


def _prefix_of(token: str) -> str | None:
    m = re.match(r"^([A-Z][A-Za-z0-9-]*?)-(\d+)$", token)
    return m.group(1) if m else None


def parse_ledger(
    text: str, report: Report, v3: bool = False, v3_from_round: int | None = None
) -> tuple[
    list[str],
    dict[str, dict],
    dict[str, set[str]],
    dict[str, dict[str, str]],
    dict[str, dict],
]:
    lines = text.splitlines()
    ids: list[str] = []
    rows: dict[str, dict] = {}
    section_ids: dict[str, set[str]] = {}
    section_classes: dict[str, dict[str, str]] = {}
    root_rows: dict[str, dict] = {}
    current: dict[str, int] | None = None
    section = ""
    table_seen: set[str] = set()
    range_covered: set[str] = set()

    for i, line in enumerate(lines):
        if line.startswith("## "):
            section = line.strip("# ").strip()
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        # Every markdown table header is followed by a separator row. Use that
        # to reset the column mapping at table boundaries; otherwise the RB
        # tables (4 columns) get parsed with the previous round's indices.
        next_is_sep = False
        if i + 1 < len(lines):
            nxt = lines[i + 1].strip()
            if nxt.startswith("|"):
                next_cells = [c.strip() for c in nxt.strip("|").split("|")]
                next_is_sep = bool(next_cells) and all(
                    SEP_RE.fullmatch(c) for c in next_cells
                )
        if next_is_sep:
            try:
                current = {
                    "id": cells.index("发现 ID"),
                    "cls": cells.index("处置类"),
                    "act": cells.index("处置"),
                    "ver": cells.index("复验"),
                    "ev": cells.index("证据") if "证据" in cells else -1,
                    "root": cells.index("根因 ID") if "根因 ID" in cells else -1,
                    "origin": cells.index("origin") if "origin" in cells else -1,
                    "sev": cells.index("严重度") if "严重度" in cells else -1,
                    "role": (
                        cells.index("来源角色")
                        if "来源角色" in cells
                        else (cells.index("角色") if "角色" in cells else -1)
                    ),
                }
                table_seen = set()
                range_covered = set()
                section_round = None
                section_match = re.match(r"^R(\d+)\b", section)
                if section_match:
                    section_round = int(section_match.group(1))
                # V3 ledger requirements start at schema_start; historical
                # R17–R25 tables (V2.5) do not have the new columns and must
                # not turn the transition into a permanent red gate.
                current["v3_scope"] = int(
                    v3
                    and (
                        v3_from_round is None
                        or (
                            section_round is not None
                            and section_round >= v3_from_round
                        )
                    )
                )
                if current["v3_scope"] and (current["root"] < 0 or current["origin"] < 0):
                    report.fail(
                        "ledger",
                        f"V3 台账缺少 `根因 ID` 或 `origin` 列（节：{section or '?'}；"
                        "§4.0c：一行一个根因，复发只更新 last_seen）",
                    )
            except ValueError:
                current = None
            continue
        if current is None:
            continue
        if len(cells) <= max(current.values()):
            report.fail("ledger", f"台账行列数不足: {line}")
            continue
        raw_ids = re.sub(r"CLAIM\s+(\d+)", lambda m: f"CLAIM-{int(m.group(1)):02d}", cells[current["id"]])
        row_ids: list[str] = []
        last_prefix: str | None = None
        # `DESIGN-D2-01、03、06` carries the prefix across comma-separated
        # bare numbers; `FACT-01、CLAIM 01` must NOT turn the claim number
        # into a second FACT-01, so only a chunk that is a bare number alone
        # inherits the previous prefix.
        for chunk in re.split(r"[、,，]", raw_ids):
            sub = [part for part in chunk.split() if part]
            if not sub:
                continue
            if len(sub) == 1 and re.fullmatch(r"\d{1,2}", sub[0]) and last_prefix:
                sub = [f"{last_prefix}-{int(sub[0]):02d}"]
            for token in sub:
                token = re.sub(r"^R\d+/", "", token)
                expanded = expand_id_token(token)
                if ".." in token:
                    range_covered.update(expanded)
                row_ids.extend(expanded)
                last_prefix = _prefix_of(token) or last_prefix
        if not row_ids:
            report.fail("ledger", f"台账行没有发现 ID: {line}")
            continue
        disposition_raw = cells[current["cls"]].strip()
        token = disposition_raw[:1] if disposition_raw else ""
        canonical = DISPOSITION_TOKENS.get(token) or DISPOSITION_TOKENS.get(
            disposition_raw.lower()
        )
        if canonical is None:
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 的处置类无法识别: `{disposition_raw}`",
            )
            continue
        action = cells[current["act"]]
        verify = cells[current["ver"]]
        evidence = cells[current["ev"]] if current["ev"] >= 0 else ""
        root_id = cells[current["root"]].strip() if current["root"] >= 0 else ""
        origin = cells[current["origin"]].strip() if current["origin"] >= 0 else ""
        severity = cells[current["sev"]].strip() if current["sev"] >= 0 else ""
        role = cells[current["role"]].strip() if current["role"] >= 0 else ""
        if current.get("v3_scope"):
            if not root_id:
                report.fail(
                    "ledger",
                    f"{'、'.join(row_ids)} 缺少 `根因 ID`（V3 一行一个根因）",
                )
            if not origin:
                report.fail(
                    "ledger",
                    f"{'、'.join(row_ids)} 缺少 `origin`（V3 首次出现轮次）",
                )
            else:
                row_round_match = re.match(r"^R(\d+)\b", section)
                row_round = int(row_round_match.group(1)) if row_round_match else None
                origin_n = origin_round(origin)
                if (
                    row_round is not None
                    and origin_n is not None
                    and origin_n > row_round
                ):
                    report.fail(
                        "ledger",
                        f"{root_id or row_ids[0]} 的 origin R{origin_n} 晚于首次出现节 "
                        f"R{row_round}；origin 必须是首见轮",
                    )
        if canonical in {"①", "④", "⑥"} and verify.lower() in PLACEHOLDERS:
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 处置类为 {canonical}，但复验列为空或占位"
                "（轮转闸：可修项必须有已修证据）",
            )
        if canonical == "FP" and (
            evidence.lower() in PLACEHOLDERS
            or len(evidence) < 20
            or re.search(r"源码|测试|文档原文|refute|cargo|grep|\.rs|§", evidence) is None
            or re.search(r"作者取舍|待裁决|未实施|无新证据", evidence) is not None
        ):
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 处置类为 FP，但 `证据` 列没有给出源码/测试/文档原文反证（或仍写作者取舍/待裁决）",
            )
        id_like_row = re.compile(r"^[A-Z]{2,}-[A-Za-z0-9-]+$")
        round_prefix_row = re.compile(r"^(?:R|D)\d+-\d+$")
        for row_id in row_ids:
            is_id_like = bool(id_like_row.match(row_id) or round_prefix_row.match(row_id))
            # A range row (`READER-B-01..20`) followed by a single-ID
            # refinement (`READER-B-15 | ③`) is intentional: the specific
            # disposition overrides the range. Only flag duplicates that are
            # not explained by such a refinement.
            refinement = row_id in range_covered and len(row_ids) == 1
            if is_id_like and row_id in table_seen and not refinement:
                report.fail("ledger", f"台账里发现 ID 重复: {row_id}（节：{section}）")
            table_seen.add(row_id)
            rows[row_id] = {
                "class": canonical,
                "action": action,
                "verify": verify,
                "evidence": evidence,
                "root": root_id,
                "origin": origin,
                "severity": severity,
                "role": role,
                "section": section,
            }
            section_ids.setdefault(section, set()).add(row_id)
            section_classes.setdefault(section, {})[row_id] = canonical
            ids.append(row_id)
        if root_id:
            if root_id in root_rows and root_rows[root_id]["section"] == section:
                report.fail("ledger", f"同一节重复根因 ID: {root_id}")
            root_rows[root_id] = {
                "class": canonical,
                "action": action,
                "verify": verify,
                "evidence": evidence,
                "root": root_id,
                "origin": origin,
                "role": role,
                "finding_ids": list(row_ids),
                "section": section,
            }
    report.ok("ledger", f"台账解析到 {len(ids)} 条处置")
    return ids, rows, section_ids, section_classes, root_rows


def _metric_blocks(text: str) -> list[tuple[str, str]]:
    """Split metrics.md into `##` sections so a multi-round file is checked
    per round instead of always reading the first (oldest) block."""
    blocks: list[tuple[str, str]] = []
    title = ""
    body: list[str] = []
    for line in text.splitlines():
        if line.startswith("## "):
            if title or body:
                blocks.append((title, "\n".join(body)))
            title = line.strip("# ").strip()
            body = [line]
        else:
            body.append(line)
    if title or body:
        blocks.append((title, "\n".join(body)))
    return blocks


# V3.6-r1-F1：减法表的占位首格——这些不是「提案」，不得计入提案数。
_SUBTRACTION_PLACEHOLDERS = {
    "无", "—", "–", "-", "", "N/A", "n/a", "none", "None", "（无）", "(无)", "无提案",
}


def _count_subtraction_rows(text: str) -> int:
    """Count data rows under `## SUBTRACTION-TABLE`（V3.6，C-34 的机械读数）。

    段不存在 → 0（D2 未给减法表）。数据行 = 以 `|` 开头的行，排除：
      - 分隔行（单元格全由 `-:` 与空格构成）；
      - 表头行（第一格是 加法提案 / 提案 / proposal / 加法）；
      - **占位行**（V3.6-r1-F1 判例：首格是 `无` / `—` / `–` / `-` / `N/A` /
        `none` 的行不是提案）。没有这条，D2 写「本轮无减法提案」并用 `无`
        占位会被数成 N 条提案——**诚实报告 FAIL、编造 PASS**，与 C-34 的
        反奖励回路目的完全相反。
      - 围栏代码块内的行（V3.6-r1-F6：D2 可能给「格式示例」表，示例不算实物）。
    以 `## ` **或 `### `** 标题停止（F6：`###` 子节也终止本段）。
    """
    m = re.search(r"^##\s+SUBTRACTION-TABLE\b", text, re.MULTILINE)
    if not m:
        return 0
    rows = 0
    in_fence = False
    for line in text[m.end():].splitlines():
        stripped = line.strip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if re.match(r"^#{2,3}\s", stripped):
            break
        if not stripped.startswith("|"):
            continue
        cells = [c.strip() for c in stripped.strip("|").split("|")]
        if not cells or all(set(c) <= set("-: ") for c in cells):
            continue
        if cells[0] in {"加法提案", "提案", "proposal", "Proposal", "加法"}:
            continue
        # r2（F1 变体）：`无提案（本轮无新增类型）` 这类**带括注的占位**也要认。
        if cells[0] in _SUBTRACTION_PLACEHOLDERS or re.match(
            r"^(?:无|—|–|N/A|none)(?:[（(]|$)", cells[0], re.IGNORECASE
        ):
            continue
        rows += 1
    return rows


def check_subtraction(
    d2_text: str,
    metrics_text: str,
    report: Report,
    round_tag: str | None,
) -> None:
    """V3.6 C-34：减法提案计数必须与 D2 实物一致。

    `metrics.md` 的 `- 减法: 提案 <N> / 采纳 <M>`：
      - 缺键 → FAIL（V3.6 轮必须有该键，否则奖励回路仍然只奖励加法）；
      - 提案数 ≠ `## SUBTRACTION-TABLE` 数据行数 → FAIL（自报数与实物不符）；
      - 提案数 = 0 → FAIL（D2 只列加法，报告不完整，§4.3 纪律）；
      - 采纳数 > 提案数 → FAIL（不可能）。
    """
    rows = _count_subtraction_rows(d2_text)
    body = _pick_metric_block(metrics_text, report, round_tag, strict=True)[1]
    sub = re.search(
        r"^-\s*减法\s*[:：]\s*提案\s*(\d+)\s*/\s*采纳\s*(\d+)",
        body,
        re.MULTILINE,
    )
    if sub is None:
        report.fail(
            "subtraction",
            "V3.6 metrics 缺少 `- 减法: 提案 <整数> / 采纳 <整数>` 键"
            "（C-34 反奖励回路）",
        )
        return
    said, adopted = int(sub.group(1)), int(sub.group(2))
    if said != rows:
        report.fail(
            "subtraction",
            f"metrics 声明减法提案 {said} 与 design-d2.md 的 "
            f"SUBTRACTION-TABLE 数据行 {rows} 不一致（C-34：提案数必须来自 D2 实物）",
        )
    elif rows == 0:
        # V3.6-r2（双路对抗共同判例）：0 行**不必然是缺陷**。C-34 要防的是
        # 「报告只列加法」，不是「本轮没有加法可列」。区分靠 D2 自己的声明：
        #   - 显式声明本轮无新增抽象提案 → 合法零（PASS）；
        #   - 有加法类 FINDING/提案却 0 行减法 → 只列加法（FAIL）。
        # 原实现无条件 FAIL，逼出「编造一行提案」——正是 F1 消灭的那个反向激励。
        _no_prop = re.search(
            r"本轮无(?:新增)?(?:类型|抽象|加法)?提案|无新增抽象|本轮无减法提案"
            r"|无需减法|无减法必要",
            d2_text,
        )
        _has_add = re.search(
            r"^(?:##+\s+FINDING|##\s+ARCH-PROPOSAL)", d2_text, re.MULTILINE
        ) is not None and re.search(
            r"新增(?:类型|字段|trait|抽象|守卫|token)|newtype|值对象|typestate",
            d2_text,
        ) is not None
        if _no_prop and not _has_add:
            report.ok(
                "subtraction", "D2 显式声明本轮无新增抽象提案（合法零）"
            )
        elif _has_add:
            report.fail(
                "subtraction",
                "design-d2.md 有新增抽象提案但 SUBTRACTION-TABLE 为 0 行："
                "报告只列加法（review-panel.md §4.3 纪律，报告不完整）",
            )
        else:
            report.ok(
                "subtraction",
                "D2 未提出新增抽象且无减法表（无对象可减，合法零）",
            )
    if adopted > said:
        report.fail(
            "subtraction",
            f"减法采纳数 {adopted} > 提案数 {said}（采纳不得多于提案）",
        )
    elif said == rows and rows > 0:
        report.ok(
            "subtraction", f"减法提案 {said} / 采纳 {adopted} 与 D2 实物一致"
        )


def _pick_metric_block(
    text: str, report: Report, round_tag: str | None, strict: bool = False
) -> tuple[str, str]:
    blocks = _metric_blocks(text)
    if round_tag:
        for title, body in blocks:
            if _round_key_matches(title, round_tag):
                return title, body
        # r8（对抗判例）：V3 的 `--round` 模式不得回退到最后一块——块缺失（含被
        # 写成 `# R<N>` 单级标题的旁路）必须由调用方判 FAIL；V2.5 历史夹具保留
        # 回退语义。
        if strict:
            return "", ""
        return blocks[-1] if blocks else ("", text)
    # No round heading: a single-block file is the normal case; otherwise the
    # last block is the newest round.
    return blocks[-1] if blocks else ("", text)


def _debt_row_closed(cells: list[str]) -> bool:
    """V3.6 §4.0f exit 1/2：这一行是否已销账（代码证据 / dup-of / 用户批准）。

    历史教训链（三轮都栽在同一处，判据必须同时满足三条）：
      - b3b：整行子串匹配 → 长散文句尾「已修」吃掉整条欠账；
      - b3c 修法：首格占位符 + 末格含完成词 → **又**把
        `| — | P2 | 待实施；同族项已在 R32 已修 |` 判成销账（题干「待实施」）；
      - V3.6-r1-F3：只看前缀 → `✅ 已登记，待实施（未修）` /
        `✅ 已修，但 R34 复验 FAIL` / `完成度 40%，未实施` 全放行。

    正确判据 = **闭合词出现在格首 AND 格内没有否定/未完成词**。否定词优先级高于
    完成词：一行同时说「已修」和「未实施」的，是未销账。
    """
    for cell in cells:
        if not _DONE_PREFIX_RE.match(cell):
            continue
        # 该格声称完成，但格内若出现否定/未完成语，按未销账处理。
        if _UNDONE_RE.search(cell):
            return False
        # V3.6-r2-F3（对抗判例）：**裸**状态词不算销账。§4.0f 的出口是
        # 「实施（附代码证据或 dup-of）」——一个只写「已实施」「已拒绝」的格
        # 没有任何证据，是状态声称而不是实施事实。阈值取 6 字符：去掉
        # `✅` / 标点后仅剩状态词本体才算「裸」；带任何补充说明的都放行
        # （D01 的 9 条长处置格各有测试计数/路径证据，不得误杀）。
        _bare = re.sub(r"[✅✔\s。．.；;，,]", "", cell)
        if len(_bare) <= 6:
            return False
        return True
    # 无 ID 的整行状态叙述（历史合法形态）：末格短、且**不含**否定语。
    if cells and cells[0] in {"—", "–", "-"}:
        last = cells[-1]
        if (
            len(last) <= 120
            and any(tok in last for tok in DEBT_DONE_TOKENS)
            and not _UNDONE_RE.search(last)
        ):
            return True
    # §4.0f exit 1 的 `dup-of`（F5；门自己的 FAIL 文案就写了这个出口）。
    # 它在**处置格**（第 3 格），不必在首格。
    if any(re.match(r"^\s*dup-of\b", cell, re.IGNORECASE) for cell in cells):
        return True
    return False


_DONE_PREFIX_RE = re.compile(
    # V3.6-r2：`完成(?!度)`——「完成度 40%」是百分比读数，不是完成声明。
    r"^\s*(?:✅|✔|完成(?!度)|已修|已实施|已闭合|已完成|已覆盖|已拒绝|superseded)"
    r"|^\s*（[^）]{0,24}）\s*已修"
)
# 否定/未完成语——出现在同一个格里就否决该格的完成声明。
_UNDONE_RE = re.compile(
    r"未修|未实施|未完成|未落地|待实施|待修|待补|待办|尚未|未开始|未做"
    r"|FAIL|REOPEN|延期|搁置|作废|回退|不适用"
    r"|完成度\s*\d*%?\s*(?:40|50|60|70|80|90)?\s*[,，]?\s*未"
)


def _panel_empty_ok(name: str, text: str, loop_text: str | None) -> bool:
    """V3.6-r2-F4：面板是否可判为「有内容」或「已声明无发现」。

    合法形态（`review-evals.md` 第 9 条）：
      - 面板文件本体含 `无发现`；
      - **或** `loop.md §1` 有具名声明 `无发现面板：<file>（理由）`——必须点名该文件。
    其余（0 条 FINDING/SUGG/CLAIM 且无声明）→ 空壳面板。
    """
    if "无发现" in text:
        return True
    if not loop_text:
        return False
    return bool(
        re.search(rf"无发现面板[^\n]{{0,200}}{re.escape(name)}", loop_text)
    )


def _has_lingering_severity(text: str, require_decl: bool = False) -> bool:
    """回执是否声明了**遗留**的 P0/P1。

    ⛔ 设计决定（V3.6-r1，F4/F10 判例）：**不再靠散文极性判断**。
    三轮对抗都把「P0 = 0 / 无 P0 / P0（0） / P0 无新增，P1 遗留 1 项 /
    P0/P1：0 / 2 / 本次无 P0/P1 遗留」逐个打补丁——这正是本项目 18 轮
    文本门循环的病根：散文的极性表达是开放集合，正则永远补不完。

    新形态（`require_decl=True`，V3.6+ 回执强制）：
      回执必须有一行**机器可读的计数声明**：
        `- 遗留 P0: <n> / P1: <n>`（或 `遗留 P0/P1: <n> / <n>`）
      门只读这两个数：全为 0 → 无遗留；任一非 0 → 有遗留；**缺失 → 视为不合规**
      （回执没有可判定读数，与「含否定结论」同等处理）。
    旧形态（`require_decl=False`，V3.5 及更早回执）：保留原启发式——它们
    已封存，不追溯，只需不误伤。
    """
    if require_decl:
        # V3.6-r2（双路对抗共同判例）：声明行有两种合法写法，且 P1 必须被解析：
        #   `- 遗留 P0: <n> / P1: <m>`
        #   `- 遗留 P0/P1: <n> / <m>`
        # 原实现只用一条组合正则，`P0: 0 / P1: 2` 的 P1 落在第一段捕获之外
        # → 返回 False。**一个诚实的 P1=2 回执因此能通过关轮**，与本门目的相反。
        decl = re.search(
            r"^-\s*遗留\s*(P0\s*/\s*P1|P0/P1|P0)\s*[:：]\s*([^\n]*)",
            text,
            re.MULTILINE,
        )
        if decl is None:
            return True   # 无可判定读数 = 不合规（fail-closed）
        head = decl.group(1).replace(" ", "")
        rest = decl.group(2)
        nums = [int(x) for x in re.findall(r"\d+", rest)]
        if head == "P0/P1":
            p0 = nums[0] if nums else 0
            p1 = nums[1] if len(nums) > 1 else 0
        else:
            p0 = nums[0] if nums else 0
            _p1 = re.search(r"P1\s*[:：]?\s*(\d+)", rest)
            p1 = int(_p1.group(1)) if _p1 else 0
        return p0 > 0 or p1 > 0
    # ---- 旧回执的启发式（不追溯）----
    for line in text.splitlines():
        if re.match(r"^-\s*遗留\s*(?:P0|P0/P1)", line.strip()):
            continue  # 显式声明行交给上面的分支判定，启发式不重复解读
        for m in re.finditer(r"(?<![a-zA-Z0-9])P[01]+(?![0-9])", line, re.IGNORECASE):
            if re.match(r"-[\dA-Za-z②③④⑤⑥]", line[m.end():]):
                continue
            head = re.split(r"[,，;；。:：|/]", line[max(0, m.start() - 10): m.start()])[-1]
            tail = re.split(r"[,，;；。]", line[m.end(): m.end() + 12])[0]
            if re.search(r"无|没有|未发现|零", head):
                continue
            if re.search(r"(?:[=:：]|\s)\s*[(（]?\s*(?:0|零)\s*[)）]?\b|0\s*条|零条", tail):
                continue
            if re.search(r"无|没有|未发现|均已?清|全部闭合", tail):
                continue
            return True
    return False


def check_metrics(
    text: str,
    report: Report,
    round_tag: str | None = None,
    expected: dict[str, int] | None = None,
    actual_open: int | None = None,
    schema_start: int | None = None,
    v3: bool = False,
    root_count: int | None = None,
    new_roots: int | None = None,
    v33_start: int | None = None,
    v34: bool = False,
) -> None:
    block_title, body = _pick_metric_block(text, report, round_tag, strict=v3)
    total_match = METRIC_TOTAL_RE.search(body)
    counts = {m.group(1): int(m.group(2)) for m in METRIC_COUNT_RE.finditer(body)}
    if round_tag:
        round_num = int(round_tag.lstrip("R"))
    else:
        # Whole-file mode: judge each block by its OWN round number, so the
        # V3.3 keys bind from v33_start and never leak onto the last
        # historical block (archival audit must not grow new reds).
        m = re.search(r"\bR(\d+)\b", block_title)
        round_num = int(m.group(1)) if m else None
    if v3:
        legacy = False
    else:
        new_schema = (
            (schema_start is not None and (round_num is None or round_num >= schema_start))
            or (round_num is not None and round_num >= 25)
            or ("未闭环" in body)
        )
        legacy = not new_schema
    if legacy:
        required = list("①②③④⑤")
    else:
        required = ["①", "②", "④", "⑤", "⑥", "FP"]
    # V3.3: triage-out findings (reader/design P3 → suggestions) are disposed
    # as SUGG rows — required in the disposition sum for bound rounds.
    if v33_start is not None and (round_num is None or round_num >= v33_start):
        required.append("SUGG")
    missing = [k for k in required if k not in counts]
    if missing:
        report.fail("metrics", "metrics.md 缺少处置计数: " + "、".join(missing))
    if not legacy and "③" in counts:
        report.fail("metrics", "R25+ 禁止使用已删除的 ③；误报写 FP 并附反证，否则写 ⑤ 未闭环")
    if v3 and total_match is None:
        # r10（P0-② 判例）：缺 `发现总数` 键会让和校验、不变式、分档对账、INDEX
        # 对账四个校验静默全跳——V3 轮必须有该键。
        report.fail("metrics", "V3 metrics 缺少 `- 发现总数: <整数>` 键")
    if total_match is not None and not missing:
        total = int(total_match.group(1))
        parts = [counts[k] for k in required]
        _new = re.search(r"^-\s*本轮新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if _new is not None and total < int(_new.group(1)):
            report.fail(
                "metrics",
                f"发现总数 {total} < 本轮新准入 {_new.group(1)}（r9 判例：三账必须同源）",
            )
        _uniq = re.search(r"^-\s*唯一根因数\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if _uniq is not None and total < int(_uniq.group(1)):
            report.fail(
                "metrics",
                f"发现总数 {total} < 唯一根因数 {_uniq.group(1)}（r9 判例）",
            )
        if total != sum(parts):
            report.fail(
                "metrics",
                f"发现总数 {total} 与处置之和 {sum(parts)} 不一致",
            )
        else:
            report.ok("metrics", f"发现总数与处置之和一致（{total}）")
        if expected is not None:
            for token in required:
                want = expected.get(token, 0)
                got = counts.get(token, 0)
                if want != got:
                    report.fail(
                        "metrics",
                        f"metrics 分档 {token}={got} 与台账本轮逐 ID 聚合 {token}={want} 不一致",
                    )
                    break
            else:
                report.ok("metrics", "metrics 各分档与台账本轮逐 ID 聚合一致")

    declared_open: int | None = None
    if not legacy:
        open_match = re.search(r"^-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if open_match is None:
            report.fail("metrics", "metrics.md 缺少 `- 未闭环: <整数>`（R25+ 强制）")
        else:
            declared_open = int(open_match.group(1))
            if actual_open is not None and declared_open != actual_open:
                report.fail(
                    "metrics",
                    f"metrics 写未闭环 {declared_open}，但台账本轮实际未闭环 {actual_open}；"
                    "禁止手填 0 掩盖 ⑤/未批准 ②/未实施 ⑥/复验写“未修”的行",
                )
            elif declared_open != 0:
                report.fail(
                    "metrics",
                    f"本轮仍有未闭环 {declared_open} 项；本轮不得关闭，禁止派下一轮面板",
                )
            else:
                report.ok("metrics", "未闭环 = 0，本轮可关闭")

    if v3:
        round_type = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        if round_type is None:
            report.fail("metrics", "V3 metrics 缺少 `- 轮型: full`")
        elif round_type.group(1) not in ROUND_TYPE_V33:
            report.fail(
                "metrics",
                f"V3 轮型只能是 full/structural/confirmation，实际 `{round_type.group(1)}`；"
                "discovery/verification 轮型已删除",
            )
        if re.search(r"^-\s*edition\s*[:：]\s*V\d+\.\d+", body, re.MULTILINE) is None:
            report.fail("metrics", "V3 metrics 缺少 `- edition: V3.x`")
        for key in ["唯一根因数", "本轮新准入", "certify 新准入", "重复合并", "自引入"]:
            if re.search(rf"^-\s*{re.escape(key)}\s*[:：]\s*(\d+)", body, re.MULTILINE) is None:
                report.fail("metrics", f"V3 metrics 缺少 `- {key}: <整数>`")
        certify = re.search(r"^-\s*certify 新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if certify is not None and declared_open == 0 and int(certify.group(1)) != 0:
            report.fail(
                "metrics",
                f"certify 新准入 = {certify.group(1)}；本轮不得关闭（必须在本轮内修完再认证）",
            )
        unique = re.search(r"^-\s*唯一根因数\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if root_count is not None and unique is not None and int(unique.group(1)) != root_count:
            report.fail(
                "metrics",
                f"唯一根因数 {unique.group(1)} 与台账根因 ID 数 {root_count} 不一致",
            )
        new_match = re.search(r"^-\s*本轮新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if new_roots is not None and new_match is not None and int(new_match.group(1)) != new_roots:
            report.fail(
                "metrics",
                f"本轮新准入 {new_match.group(1)} 与台账 origin=本轮 的根因数 {new_roots} 不一致",
            )
        # V3.3 four-state: every bound round declares its closing result.
        # `CONVERGED` here is cross-checked by check_v33_lifecycle (round type
        # + certify 新准入); the whitelist blocks free-form status prose.
        if v33_start is not None and (round_num is None or round_num >= v33_start):
            result_key = re.search(
                r"^-\s*轮结果\s*[:：]\s*(ROUND-CLOSED|CONVERGED|NOT CONVERGED"
                r"|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED)\b",
                body,
                re.MULTILINE,
            )
            if result_key is None:
                report.fail(
                    "metrics",
                    "V3.3 metrics 缺少 `- 轮结果: ROUND-CLOSED|CONVERGED|NOT CONVERGED"
                    "|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED`（§6A 四态模型）",
                )
            elif result_key.group(1) not in ROUND_RESULT_V33:
                report.fail(
                    "metrics",
                    f"- 轮结果 `{result_key.group(1)}` 不合法；允许：{sorted(ROUND_RESULT_V33)}",
                )
            # V3.3 D-047 治理仪表：代裁决授权与漂移可见性
            for key in ["代裁决", "ARCH 提案", "用户否决"]:
                if re.search(rf"^\-\s*{re.escape(key)}\s*[:：]\s*(\d+)", body, re.MULTILINE) is None:
                    report.fail("metrics", f"V3.3 metrics 缺少治理仪表 `- {key}: <整数>`")
            # V3.4 最小内核（P1-7 判例：D03/D04 的「certify 待跑」与 CONVERGED 同轮
            # 并存且门放行）：闭合声明不得与 certify 未完成标记同存。作用域限定
            # `schema: V3.4`——01–04 已 FREEZE 的存量记录不追溯（r5 回归审查 H1）。
            if (
                v34
                and result_key is not None
                and result_key.group(1) in {"CONVERGED", "ROUND-CLOSED"}
                and re.search(
                    r"certify[^\n]{0,16}(?:待跑|未跑|待补|待安排|未完成|未执行|没跑|延后|排期未定|待复跑|未复跑|尚未启动|回读未做|pending|not run|推迟)",
                    body,
                    re.IGNORECASE,
                )
            ):
                report.fail(
                    "metrics",
                    "轮结果为闭合态，但记录仍写「certify 待跑/未跑」；闭合声明缺闭合"
                    "证据——先跑完 certify 再改轮结果（P1-7）",
                )
    elif not legacy:
        round_type = re.search(r"^-\s*轮型\s*[:：]\s*(discovery|verification)", body, re.MULTILINE)
        if round_type is None:
            report.fail("metrics", "metrics.md 缺少 `- 轮型: discovery|verification`（R25+ 强制）")
        elif round_type.group(1) == "verification":
            new_find = re.search(r"^-\s*新 in-scope 真缺陷\s*[:：]\s*(\d+)", body, re.MULTILINE)
            if new_find is None:
                report.fail("metrics", "verification 轮缺少 `- 新 in-scope 真缺陷: <整数>`")
            elif int(new_find.group(1)) != 0:
                report.fail(
                    "metrics",
                    f"verification 轮仍有 {new_find.group(1)} 条新 in-scope 真缺陷；不得 CONVERGED",
                )
            else:
                report.ok("metrics", "verification 轮新 in-scope 真缺陷 = 0，可判 CONVERGED")

    required_keys = ["re-litigation", "adjacent 阻塞", "重构", "成本"]
    for key in required_keys:
        if not re.search(rf"^-\s*{re.escape(key)}\s*[:：]", text, re.MULTILINE):
            report.fail("metrics", f"metrics.md 缺少键 `{key}`")

    for line in text.splitlines():
        if line.strip().startswith("-") and re.search(r"TBD|待定", line, re.IGNORECASE):
            report.fail("metrics", f"metrics.md 残留占位: {line.strip()}")

def check_round_type_sequence(
    text: str, report: Report, schema_start: int | None = None, v3: bool = False
) -> None:
    """Enforce the V2.4 state machine on metrics blocks with explicit `## R<N>`
    headings: at most one discovery, and **no round after a verification**.

    Without this, clearing `未闭环` and then labeling the next round
    `discovery` silently reopens the full panel — the exact 25-round loop the
    state machine exists to stop. A verification round that finds new defects
    must fix them in the same round (max 2 deltas); it may not hand them to a
    later round.

    V3 removes both round types. In V3 every round must say `轮型: full`;
    there is no discovery/verification sequence to enforce.
    """
    if v3:
        for title, body in _metric_blocks(text):
            m = re.search(r"\bR(\d+)\b", title)
            if not m:
                continue
            n = int(m.group(1))
            if schema_start is not None and n < schema_start:
                continue
            rt = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
            if rt is None:
                report.fail("round-type", f"{title} 缺少 `- 轮型: full`（V3）")
            elif rt.group(1) not in ROUND_TYPE_V33:
                report.fail(
                    "round-type",
                    f"{title} 的轮型是 `{rt.group(1)}`；V3 只允许 full/structural/confirmation"
                    "（discovery/verification 已删除）",
                )
        return
    seq: list[tuple[int, str, str]] = []
    for title, body in _metric_blocks(text):
        m = re.search(r"\bR(\d+)\b", title)
        if not m:
            continue
        n = int(m.group(1))
        if schema_start is not None and n < schema_start:
            continue
        rt = re.search(r"^-\s*轮型\s*[:：]\s*(discovery|verification)", body, re.MULTILINE)
        if rt:
            seq.append((n, rt.group(1), title))
        elif schema_start is not None:
            report.fail(
                "round-type",
                f"{title} 缺少 `- 轮型: discovery|verification`；V2.5 schema 下从 R{schema_start} 起每一轮都必须声明轮型",
            )
    if not seq:
        return
    discoveries = [x for x in seq if x[1] == "discovery"]
    if len(discoveries) > 1:
        report.fail(
            "round-type",
            "metrics 出现多个 discovery 轮（"
            + "、".join(f"R{n}" for n, _, _ in discoveries)
            + "）；一篇文档只允许一次全量发现，后续必须是 verification",
        )
    for n, rtype, _title in seq:
        if rtype != "verification":
            continue
        later = sorted(x for x in seq if x[0] > n)
        if later:
            report.fail(
                "round-type",
                f"R{n} 是 verification，却出现 R{later[0][0]}；验证轮的新缺陷必须在同一轮 delta 内修完，"
                "不得开新轮（否则收敛又变成无限循环）",
            )
    if any(x[1] == "discovery" for x in seq) and seq[0][1] == "verification":
        report.fail("round-type", "metrics 在 discovery 之前出现了 verification；轮型顺序非法")


def check_verify_templates(rows: dict[str, dict], report: Report) -> None:
    """Warn when many rows reuse the same `复验` text (template evidence).

    The tool cannot judge whether a grep/test output is genuine, but a single
    sentence repeated across five or more fixes is almost always a template
    (R25's `doc 五门 PASS；宿主/QEMU 见 loop §5`). WARN, not FAIL: the meta
    audit and the per-root evidence requirement remain the authority.
    """
    from collections import Counter

    counts = Counter(row["verify"] for row in rows.values() if row.get("verify"))
    for text, count in counts.items():
        if count >= 5 and len(text) >= 20:
            report.warn(
                "verify-template",
                f"{count} 条记录复用同一复验文本（疑似模板）：{text[:48]}"
                "…；逐根因证据才算 100% 复验",
            )


ROW_LEVEL_TOKEN_RE = re.compile(
    r"os/[\w./-]+\.rs(?::|\s)|[\w./-]+\.(?:rs|md|sh|py):\d+|"
    r"tests?::[A-Za-z_]|cargo test|grep|旧串|commit\s+[0-9a-f]{7,}|git show|"
    r"冷读|回读|冷启动|CLOSED|STILL-BROKEN",
    re.IGNORECASE,
)
COLD_READ_TOKEN_RE = re.compile(r"冷读|回读|冷启动|CLOSED|STILL-BROKEN", re.IGNORECASE)
CONTRADICTION_RE = re.compile(r"矛盾|打架|对不上|不一致|自相矛盾")
B_CLASS_ROLE_RE = re.compile(r"^(reader-|design-)", re.IGNORECASE)


def _section_round(section: str) -> int | None:
    m = re.match(r"^R(\d+)\b", section or "")
    return int(m.group(1)) if m else None


def check_v3_evidence(
    ledger_rows: dict[str, dict],
    report: Report,
    round_tag: str | None = None,
    schema_start: int | None = None,
) -> None:
    """Enforce per-root evidence, B-class cold reads and no templates (V3).

    `复验` must carry a row-level token (path:line / test::name / grep / old
    string / commit / cold-read verdict). A round-level `五门 PASS` or
    `见 loop §N` is not evidence for an individual root. Reader/design rows
    are B-class and additionally require a cold-read `CLOSED`/`STILL-BROKEN`
    receipt; a FACT row whose evidence says "矛盾/打架" is treated as B-class.
    """
    from collections import Counter

    scope_round = int(round_tag.lstrip("R")) if round_tag else None
    scoped: list[tuple[str, dict]] = []
    for fid, row in ledger_rows.items():
        if row.get("class") not in {"①", "④", "⑥"}:
            continue
        section_round = _section_round(row.get("section", ""))
        if scope_round is not None:
            # A round-scoped check must not let a section without a round
            # number (e.g. a historical "RB13" or "R6 面板明细" table) leak
            # into R26 evidence checks.
            if section_round != scope_round:
                continue
        elif schema_start is not None and section_round is not None and section_round < schema_start:
            continue
        scoped.append((fid, row))

    for fid, row in scoped:
        verify = row.get("verify", "")
        role = row.get("role", "")
        if not ROW_LEVEL_TOKEN_RE.search(verify):
            report.fail(
                "v3-evidence",
                f"{fid} 复验没有行级证据（需 path:line / test::name / grep / 旧串 / "
                "commit / 冷读回执）；`五门 PASS`、`见 loop §N` 不算",
            )
        if B_CLASS_ROLE_RE.match(role) and not COLD_READ_TOKEN_RE.search(verify):
            report.fail(
                "v3-evidence",
                f"{fid} 来源 {role} 是 B 类（读者/设计），复验缺冷读 "
                "CLOSED/STILL-BROKEN 回执",
            )
        if (
            role.lower().startswith("fact-")
            and CONTRADICTION_RE.search(row.get("evidence", "") + row.get("action", ""))
            and not COLD_READ_TOKEN_RE.search(verify)
        ):
            report.fail(
                "v3-evidence",
                f"{fid} 是 FACT 的矛盾类（证据含矛盾/打架/对不上），复验缺冷读回执",
            )

    for key, label in (("action", "处置"), ("verify", "复验")):
        counts = Counter(row.get(key, "") for _, row in scoped if row.get(key))
        for text, count in counts.items():
            if count > 3 and len(text) >= 10:
                report.fail(
                    "v3-template",
                    f"V3 本轮 {label} 列同一文本出现 {count} 次（>3），疑似模板："
                    f"{text[:48]}",
                )


def audit_template_sections(ledger_text: str, report: Report) -> None:
    """Historical audit: flag template repetition in every ledger section.

    This is not part of the normal `--round` gate (R24/R25 are historical);
    run it explicitly with `--audit-templates` to see what the old rounds
    actually recorded.

    Parsed independently from `parse_ledger`: that function keys rows by
    finding ID, and R24/R25 reuse IDs like `FACT-A-01`, so a global dict
    would let R25 overwrite R24 and undercount the history (R24 114 rows
    collapsed to 5). The audit must keep every row per section.
    """
    from collections import Counter

    lines = ledger_text.splitlines()
    section = ""
    current: dict[str, int] | None = None
    by_section: dict[str, list[dict]] = {}
    for line in lines:
        if line.startswith("## "):
            section = line.strip("# ").strip()
            current = None
            continue
        stripped = line.strip()
        if not stripped.startswith("|"):
            current = None
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        if "发现 ID" in cells and "处置" in cells and "复验" in cells:
            current = {
                "id": cells.index("发现 ID"),
                "act": cells.index("处置"),
                "ver": cells.index("复验"),
            }
            continue
        if current is None or len(cells) <= max(current.values()):
            continue
        if not cells[current["id"]]:
            continue
        by_section.setdefault(section, []).append(
            {"act": cells[current["act"]], "ver": cells[current["ver"]]}
        )
    for section, rows in by_section.items():
        for key, label in (("act", "处置"), ("ver", "复验")):
            counts = Counter(row[key] for row in rows if row[key])
            for text, count in counts.items():
                if count > 3 and len(text) >= 10:
                    report.fail(
                        "template-audit",
                        f"{section} 的 {label} 列同一文本出现 {count} 次："
                        f"{text[:48]}",
                    )


def check_todo_ledger(path: Path, report: Report) -> None:
    """Reconcile a document-level todo ledger with the review disposition set.

    `01-todo.md` is outside the run directory and was invisible to the gate.
    It may legitimately carry user-owned or deferred items, but every open
    item needs a legal disposition: `②` (user approval + date) or `⑤`
    (evidence path / blocked). `允许开放` is not in the disposition schema and
    must not be used to keep a newly registered in-scope task out of the
    review ledger.
    """
    if not path.is_file():
        report.fail("todo-ledger", f"todo 台账不存在: {path}")
        return
    lines = path.read_text(encoding="utf-8").splitlines()
    current: dict[str, int] | None = None
    open_items = 0
    for line in lines:
        stripped = line.strip()
        if not stripped.startswith("|"):
            current = None
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        if "条目" in cells and "当前状态" in cells:
            current = {
                "id": cells.index("条目"),
                "status": cells.index("当前状态"),
            }
            continue
        if current is None or len(cells) <= max(current.values()):
            continue
        item = cells[current["id"]]
        status = cells[current["status"]]
        if not item:
            continue
        # `允许开放` is illegal regardless of other cells: a grouped row can
        # contain ✅ for most items and `允许开放` for one (TODO-05).
        if any("允许开放" in cell for cell in cells):
            open_items += 1
            report.fail(
                "todo-ledger",
                f"{item} 使用非法状态 `允许开放`；映射为 ②（用户批准 + 日期）"
                "或 ⑤（取证路径 / 阻塞），不得作为第七个处置桶",
            )
            continue
        # The overview table is free-form: some rows put `✅ 已实施` in the
        # type column and details in the status column. Treat the whole row
        # as done if any cell carries a completion token.
        if any(
            token in cell
            for cell in cells
            for token in ("✅", "已实施", "已修", "已闭合", "closed", "完成")
        ):
            continue
        open_items += 1
        if any(
            token in status
            for token in ("用户批准", "用户裁决", "用户侧", "DEFERRED", "取证", "阻塞")
        ):
            report.warn(
                "todo-ledger",
                f"{item} 开放（{status}）；确认批准日期或取证路径",
            )
        else:
            report.warn(
                "todo-ledger",
                f"{item} 未闭合且无合法处置（{status}）",
            )
    report.ok("todo-ledger", f"todo 台账开放项 {open_items} 条（逐条见上）")


def check_edition_trial(
    text: str, report: Report, schema_start: int | None = None
) -> None:
    """Warn when one edition has exhausted its 3-round trial on this doc.

    V3 §7.5: an edition gets at most 3 full rounds on the same document. The
    boundary is a process action, not a document defect, so this is a WARN;
    the main agent must run L7/EDITION-RETRO instead of opening a 4th round.
    """
    stats: dict[str, dict] = {}
    for title, body in _metric_blocks(text):
        m = re.search(r"\bR(\d+)\b", title)
        if not m:
            continue
        n = int(m.group(1))
        if schema_start is not None and n < schema_start:
            continue
        ed = re.search(r"^-\s*edition\s*[:：]\s*(V[\d.]+)", body, re.MULTILINE)
        rt = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        if not ed or not rt or rt.group(1) not in {"full", "structural"}:
            continue
        stat = stats.setdefault(ed.group(1), {"rounds": 0, "last_open": None, "last_n": 0})
        # V3.6-r1-F11：`last_open` 必须是**轮号最大**那一轮的读数，不是
        # 文件里最后出现的那个块（metrics 的块顺序可能倒挂，如 R6 写在
        # R4/R5 之前）。原实现按文件顺序覆盖，`R6(open=2),R4(0),R5(0)`
        # → `PASS 试验窗口 3/3`。
        if n >= stat["last_n"]:
            stat["last_n"] = n
            op = re.search(r"^-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
            stat["last_open"] = int(op.group(1)) if op else None
        stat["rounds"] += 1
    # V3.3: confirmation rounds are excluded by design (they are not
    # modification transactions), but structural rounds count like full.
    #
    # V3.6-b4（轨迹分析判例）：原条件 `rounds >= 3 and last_open > 0` 把
    # 「已超限但自报未闭环=0」判成 PASS（D01 的 V3.1 跑了 5 个修改轮，
    # 门读数 `试验窗口 5/3` PASS）。超限本身即须边界评估，与未闭环无关；
    # 未闭环 > 0 只是额外加重。改为：rounds > 3 → WARN（超限），
    # rounds == 3 and last_open > 0 → WARN（窗口用尽且未收口）。
    for edition, stat in stats.items():
        if stat["rounds"] > 3:
            report.warn(
                "edition-trial",
                f"edition {edition} 在本篇已跑 {stat['rounds']} 个修改轮（§7.5 上限 3）；"
                "超限必须先在 DECISIONS.md 落授权行（范围/理由/日期），否则该篇"
                "不得声明收敛（V3.6-b4：超限不再因「未闭环=0」而计 PASS）",
            )
        elif stat["rounds"] == 3 and (stat["last_open"] is None or stat["last_open"] > 0):
            report.warn(
                "edition-trial",
                f"edition {edition} 在本篇已跑 3 轮且最新一轮未闭环；"
                "按 §7.5 应执行 L7 边界评估或判 NOT CONVERGING，不得开第 4 轮",
            )
        else:
            report.ok(
                "edition-trial",
                f"edition {edition} 试验窗口 {stat['rounds']}/3",
            )


DEBT_DONE_TOKENS = (
    "✅",
    "已修",
    "已实施",
    "已闭合",
    "已完成",
    "已覆盖",
    "已拒绝",
    "superseded",
    "dup-of",
)
DEBT_DEADLINE_RE = re.compile(r"截止\s*R(\d+)")


def check_open_debt(path: Path, report: Report, round_tag: str | None) -> int:
    """V3.3 §4.0f: OPEN-DEBT rows are part of the single open count.

    A debt row has exactly two legal exits: implemented (completion token /
    `dup-of` pointing at the covering root) or a DECISIONS.md decision row
    (user adjudication). Anything else keeps the round open. A `截止 R<N>`
    whose N is below the closing round is overdue and said so by name.

    V3.6-b3（轨迹分析判例）：**表头不是入口条件**。原实现只在见过
    `| ID |` / `| 条目 |` 表头后才开始收集行，于是 D02/D06 那种「无表头、
    直接以数据行开头」的 OPEN-DEBT 整表被判「0 行未销账」——一笔真实欠账
    对门完全不可见（D06 的 D-077 悬空引用正是这样漏掉的）。现在：
    数据行无论是否有表头都收集；**有表格却无法识别任何数据行 →
    按「格式无法解析」FAIL**（用 fail-closed 取代静默放行）。

    Returns the open-row count so the caller folds it into `未闭环`.
    """
    if not path.is_file():
        return 0
    lines = path.read_text(encoding="utf-8").splitlines()
    open_rows: list[tuple[str, str]] = []
    closing_round = int(round_tag.lstrip("R")) if round_tag else None
    table_lines = 0          # 任意表格行（含表头/分隔）
    candidate_rows = 0       # 看起来是数据行（首格非表头词）
    header_seen = False
    for line in lines:
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        table_lines += 1
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        if any("ID" == cell or "条目" == cell for cell in cells):
            header_seen = True
            continue
        if not cells or not cells[0]:
            continue
        item = cells[0]
        if item.startswith("#"):
            continue
        candidate_rows += 1
        row_text = " | ".join(cells)
        # V3.6 销账判据（b3b/b3c/r1-F3/F5 四轮收敛的单一入口，见 _debt_row_closed）。
        if _debt_row_closed(cells):
            continue
        # §4.0f exit 2: a DECISIONS.md-recorded user adjudication — including
        # an explicitly approved deferral — resolves the row. Approval without
        # a date is not an adjudication (mirrors the ② gate).
        if "用户批准" in row_text and re.search(r"20\d{2}-\d{2}-\d{2}", row_text):
            report.warn(
                "open-debt",
                f"{item} 已批准延期；确认 DECISIONS.md 判决账行存在（含日期与去向）",
            )
            continue
        deadline = DEBT_DEADLINE_RE.search(row_text)
        deadline_note = ""
        if deadline and closing_round is not None and int(deadline.group(1)) < closing_round:
            deadline_note = f"；已逾期（截止 R{deadline.group(1)}）"
        open_rows.append((item, deadline_note))
    if table_lines >= 2 and candidate_rows == 0 and not header_seen:
        report.fail(
            "open-debt",
            "OPEN-DEBT 有表格但无可解析的数据行（格式无法解析不等于没有欠账；"
            "V3.6-b3 判例：静默放行会让整表欠账对门隐形）",
        )
    for item, note in open_rows:
        report.fail(
            "open-debt",
            f"{item} 未销账{note}；只有两个出口：实施（附代码证据或 dup-of）"
            "或 DECISIONS.md 判决账行——没有第三个状态（§4.0f）",
        )
    if open_rows:
        report.fail(
            "open-debt",
            f"OPEN-DEBT 有 {len(open_rows)} 行未销账；未闭环口径包含本表，"
            "本轮不得关闭、不得派下一轮面板（§4.0f 单一未闭环口径）",
        )
    else:
        report.ok("open-debt", "OPEN-DEBT 无未销账行")
    return len(open_rows)


ROUND_RESULT_V33 = {    "ROUND-CLOSED",
    "CONVERGED",
    "NOT CONVERGED",
    "NOT CONVERGING",
    "FREEZE-WITH-BACKLOG",
    "BLOCKED",
}


def _panel_severities(panel_dir: Path | None) -> dict[tuple[int, str], str]:
    """Map `(round, FINDING ID)` → `P0..P3` from panel headers (`（P0-fact）`).

    Keyed per round because short finding IDs repeat across rounds
    (READER-A-16 has seven lives in doc 01's history).
    """
    if panel_dir is None or not panel_dir.is_dir():
        return {}
    out: dict[tuple[int, str], str] = {}
    for path in sorted(panel_dir.rglob("*.md")):
        round_match = re.search(r"round(\d+)", str(path.parent))
        round_num = int(round_match.group(1)) if round_match else 0
        _FWS = str.maketrans({chr(0xFF01 + i): chr(0x21 + i) for i in range(94)})
        for m in re.finditer(
            r"^#{2,3}\s+FINDING\s+([A-Za-z][A-Za-z0-9-]*)\s*[（(]?\s*(P[0-3])",
            path.read_text(encoding="utf-8").translate(_FWS),
            re.MULTILINE,
        ):
            out.setdefault((round_num, m.group(1)), m.group(2))
    return out


def check_v33_admission(
    ledger_rows: dict[str, dict],
    panel_severities: dict[tuple[int, str], str],
    report: Report,
    round_tag: str | None,
    v33_start: int | None,
) -> None:
    """V3.3 admission rules, scoped to rounds >= v33_start.

    1. Reader-role P3 rows must not enter the ledger as blocking (①/④):
       they belong in suggestions.md unless they pass the "读者卡住无法
       继续" admission test (G3).
    2. Ledger severity must match the panel FINDING header (R30 recorded a
       P0 finding as P1/P2 rows — the columns diverged).
    """
    for fid, row in ledger_rows.items():
        section_round = _section_round(row.get("section", ""))
        if v33_start is None or section_round is None or section_round < v33_start:
            continue
        role = (row.get("role") or "").lower()
        severity = (row.get("severity") or "").strip()
        cls = row.get("class")
        if (
            (role.startswith("reader") or role.startswith("design"))
            and severity == "P3"
            and cls in {"①", "④", "⑥"}
        ):
            report.fail(
                "admission-v33",
                f"{fid} 是 {role} P3 却按 {cls} 准入；G3（V3.3）：改标 SUGG 并登记"
                " suggestions.md，只有通过「读者卡住无法继续」或「正确性缺陷」测试的"
                "才按 ①/④ 入台账",
            )
        if cls == "SUGG" and not (
            (role.startswith("reader") or role.startswith("design"))
            and severity == "P3"
        ):
            report.fail(
                "admission-v33",
                f"{fid} 按 SUGG 分流但来源/严重度为 {role or '?'}/{severity or '?'}；"
                "SUGG 仅限读者/设计的 P3——P0-P2 不得借 SUGG 逃出修复队列（D-047）",
            )
        panel_sev = panel_severities.get((section_round, fid))
        if panel_sev and severity and panel_sev != severity:
            report.fail(
                "severity-sync",
                f"{fid} 台账严重度 {severity} 与面板标题 {panel_sev} 不一致；"
                "severity 列必须从面板 FINDING 标题机械同步",
            )
        # V3.3 D-047: design decisions must declare their class. Unlabeled
        # implementations are unclassifiable and treated as ARCH (blocked) —
        # the burden of proof sits on the proxy side (C anchor required).
        if role.startswith("design") and cls == "⑥":
            row_text = " ".join(
                (row.get("action") or "", row.get("evidence") or "", row.get("verify") or "")
            )
            tag = re.search(r"\[(MODERN|DEL|ARCH)\]", row_text)
            if tag is None:
                report.fail(
                    "design-class",
                    f"{fid} 设计实施未分类（需 [MODERN]/[DEL]/[ARCH] 标记）；"
                    "未分类按 ARCH 阻断——举证责任在代裁侧：[MODERN]/[DEL] 必须能锚定 "
                    "Minix3 C 语义（D-047）",
                )
            elif tag.group(1) == "ARCH" and not (
                re.search(r"D-\d+", row_text)
                and "用户批准" in row_text
                and re.search(r"20\d{2}-\d{2}-\d{2}", row_text)
            ):
                report.fail(
                    "design-class",
                    f"{fid} 标记 [ARCH] 却已实施且无判决行引用（需 D-xxx + 用户批准 + 日期）；"
                    "ARCH 类必须先落 DECISIONS.md 判决账行（用户裁决）——即便口头说继续"
                    "也必须落行（D-047）",
                )


def _has_round_ledger(n: int, section_classes: dict[str, dict[str, str]]) -> bool:
    """r8（对抗判例）：`发现总数: 0` 两行伪造仍可满足 mod_closed——真实修改轮的
    台账必须有该轮的行。计入要件 = metrics 骨架 + 台账该轮节且非空。"""
    return any(
        _round_key_matches(sec, f"R{n}") and bool(mapping)
        for sec, mapping in section_classes.items()
    )


def check_v33_lifecycle(
    metrics_text: str,
    ledger_rows: dict[str, dict],
    section_classes: dict[str, dict[str, str]],
    report: Report,
    schema_start: int | None,
    v33_start: int | None,
) -> None:
    """V3.3 four-state lifecycle (§6A), anchored on the metrics `- 轮结果:` key
    (not on loop.md §7 prose — the prose legitimately mentions CONVERGED when
    narrating a previous round's overturned claim, which false-positives).

    - full/structural rounds close as ROUND-CLOSED at most; a CONVERGED
      result on a modification round is a false positive (R26/R30 判例).
    - CONVERGED requires a `confirmation` round with `certify 新准入: 0`;
      at most two confirmation rounds; none after a sealed one.
    - A confirmation round implements no ⑥: design items go to OPEN-DEBT
      for the next design batch, otherwise the baseline moves under the
      confirmation itself.
    """
    confirmations = 0
    sealed_at: int | None = None
    prev_edition: str | None = None
    # V3.4 最小内核（r6 R6-a 判例）：确认轮验证的是修改轮的收口——此前必须存在
    # 已关闭的修改轮（full/structural 且 ROUND-CLOSED 且未闭环=0），否则「确认」
    # 是伪造收敛。按文档计（不按 edition）：R34 型跨 edition 确认合法（D-057 口径）。
    mod_closed: list[int] = []
    for title, body in _metric_blocks(metrics_text):
        m = re.search(r"\bR(\d+)\b", title)
        if not m:
            continue
        n = int(m.group(1))
        # mod_closed 追踪在 edition/era 过滤之前：已关闭的修改轮按文档计，
        # 跨 edition（甚至 V3.3 前）的收口也算数——D-057 的 R34 型口径。
        _rt_early = re.search(r"^\s*-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        _res_early = re.search(
            r"^\s*-\s*轮结果\s*[:：]\s*(ROUND-CLOSED|CONVERGED|NOT CONVERGED"
            r"|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED)\b",
            body,
            re.MULTILINE,
        )
        _open_early = re.search(r"^\s*-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
        # r7（对抗判例）：两行伪造的 `full`+`ROUND-CLOSED` 空块也计入 mod_closed——
        # 计入要件加「块内有 `发现总数` 键」（真实轮必有；伪造须造全套 metrics，
        # 而全套伪造在 N0 双源与 coverage 门射程内）。
        _has_total = re.search(r"^\s*-\s*发现总数\s*[:：]", body, re.MULTILINE)
        if (
            _rt_early is not None
            and _rt_early.group(1) in {"full", "structural"}
            and _res_early is not None
            and _res_early.group(1) == "ROUND-CLOSED"
            and _has_total is not None
            and _has_round_ledger(n, section_classes)
            and (_open_early is None or int(_open_early.group(1)) == 0)
        ):
            mod_closed.append(n)
        if v33_start is not None and n < v33_start:
            continue
        if schema_start is not None and n < schema_start:
            continue
        # Per-CANDIDATE counters: a new edition (after NOT CONVERGING, or a
        # user-approved reopen of a sealed doc) starts a fresh confirmation
        # allowance and unseals. Without this, any document whose first
        # confirmation fails twice can never converge (K=3 editions would
        # exhaust the global counter).
        ed = re.search(r"^-\s*edition\s*[:：]\s*(V[\d.]+)", body, re.MULTILINE)
        edition = ed.group(1) if ed else None
        if edition is not None and prev_edition is not None and edition != prev_edition:
            confirmations = 0
            sealed_at = None
        if edition is not None:
            prev_edition = edition
        rt = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        rtype = rt.group(1) if rt else None
        result = re.search(
            r"^-\s*轮结果\s*[:：]\s*(ROUND-CLOSED|CONVERGED|NOT CONVERGED"
            r"|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED)\b",
            body,
            re.MULTILINE,
        )
        result_val = result.group(1) if result else None
        certify = re.search(r"^-\s*certify 新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        certify_new = int(certify.group(1)) if certify else None
        if rtype == "confirmation":
            confirmations += 1
            if confirmations > 2:
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 是第 {confirmations} 次确认轮；确认轮全局最多 2 次（§6A）",
                )
            if result_val == "CONVERGED" and not mod_closed:
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 确认轮宣告 CONVERGED，但本文档此前没有任何已关闭的修改轮"
                    "（full/structural 且 ROUND-CLOSED 且未闭环=0）；确认轮验证的是修改轮"
                    "的收口，无前置修改轮的「确认」是伪造收敛（r6 R6-a）",
                )
            if certify_new == 0:
                sealed_at = n
            for section, mapping in section_classes.items():
                if not _round_key_matches(section, f"R{n}"):
                    continue
                for fid, cls in mapping.items():
                    row = ledger_rows.get(fid)
                    if (
                        row is not None
                        and row.get("class") == "⑥"
                        and _section_round(row.get("section", "")) == n
                    ):
                        report.fail(
                            "lifecycle-v33",
                            f"{fid} 在确认轮实施 ⑥；确认轮只修新准入的 P0/P1/P2 事实与同步项，"
                            "设计项登记 OPEN-DEBT 到下一个设计批（§4.0d）",
                        )
        if result_val == "CONVERGED":
            if rtype != "confirmation":
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 是 {rtype or '?'} 轮却宣告 CONVERGED；修改轮最多 ROUND-CLOSED / "
                    "CONVERGENCE-CANDIDATE，收敛声明权只在确认轮（R26/R30 两次假阳性判例）",
                )
            elif certify_new != 0:
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 确认轮 certify 新准入 = {certify_new}；非 0 不得宣告 CONVERGED",
                )
        # V3.3 D-047: an ARCH-blocked round (⑤ rows awaiting user adjudication)
        # must declare `轮结果: BLOCKED` — the hard stop is a recorded state,
        # not a chat understanding. 口头"继续"不解锁：解除只认判决行。
        arch_blocked = [
            fid
            for section, mapping in section_classes.items()
            if _round_key_matches(section, f"R{n}")
            for fid, cls in mapping.items()
            if cls == "⑤"
            and "ARCH" in (
                (ledger_rows.get(fid, {}).get("action") or "")
                + (ledger_rows.get(fid, {}).get("verify") or "")
                + (ledger_rows.get(fid, {}).get("evidence") or "")
            )
            # V3.4-r8（修复回归 F6）：已批准延期（用户批准 + 日期 + D-xxx 判决行
            # 引用）不触发 ARCH 硬阻断——否则「延期」是死选项。
            and not (
                "用户批准" in (ledger_rows.get(fid, {}).get("action") or "")
                and re.search(r"20\d{2}-\d{2}-\d{2}", ledger_rows.get(fid, {}).get("action") or "")
                and re.search(r"\bD-\d{2,3}\b", ledger_rows.get(fid, {}).get("action") or "")
            )
        ]
        if arch_blocked and result_val not in (None, "BLOCKED"):
            report.fail(
                "lifecycle-v33",
                f"R{n} 存在等待用户 ARCH 裁决的 ⑤ 行（{', '.join(arch_blocked[:3])}…），"
                f"但轮结果为 `{result_val}`；必须 `BLOCKED`，且解除只认判决账行"
                "（D-047：口头继续不解除）",
            )
        if sealed_at is not None and n > sealed_at:
            report.fail(
                "lifecycle-v33",
                f"R{sealed_at} 确认通过（新准入=0）之后出现 R{n}；CONVERGED 后不得再开轮，"
                "新证据走重开决策（§6A）",
            )


def check_placeholders(text: str, report: Report, label: str) -> None:
    for line in text.splitlines():
        if re.search(r"\bTBD\b|待定", line, re.IGNORECASE):
            report.fail("placeholder", f"{label} 残留占位: {line.strip()}")


def collect_panel_findings(
    panel_dir: Path, report: Report, collect_sugg_ids: bool = False
) -> list[str]:
    if not panel_dir.is_dir():
        report.warn("panel", f"没有 panel 目录: {panel_dir}")
        return []
    ids: list[str] = []
    # Panel outputs live both flat (round 0) and in round subdirectories
    # (round1/, round2/, ...) — recurse so every round is checked.
    paths = [
        p
        for p in sorted(panel_dir.glob("*.md"))
        + sorted(p for p in panel_dir.glob("*/*.md") if p.is_file())
        if p.name != "INDEX.md"
    ]
    if not paths:
        report.warn("panel", f"面板目录没有任何 .md（含轮次子目录）: {panel_dir}")
        return []
    for path in paths:
        _FWC = str.maketrans({chr(0xFF01 + i): chr(0x21 + i) for i in range(94)})
        text = path.read_text(encoding="utf-8").translate(_FWC)
        found = FINDING_RE.findall(text) + FINDING_ROUND_RE.findall(text)
        # Non-`## FINDING` shapes still count as findings in a round's panel:
        # fact.md's secondary bullets and refute.md's numbered claims.
        found += [f"FACT-{n}" for n in re.findall(r"^-\s*\*\*FACT-(\d+)\*\*", text, re.MULTILINE)]
        found += [f"CLAIM-{int(n):02d}" for n in re.findall(r"^##\s+CLAIM\s+(\d+)", text, re.MULTILINE)]
        # r10（P0-① 判例）：带 ID 的 SUGGESTION 也是面板发现——SUGG 行入台账并
        # 计入 metrics 后，覆盖率门必须能看见它们，否则合规记账双 FAIL。
        # 仅对制品必需文档启用：历史冻结轮（V3.3 旧口径，SUGG 不入台账）不追溯。
        if collect_sugg_ids:
            found += re.findall(
                r"^#{2,3}\s+SUGGESTION\s+([A-Za-z][A-Za-z0-9_-]*)", text, re.MULTILINE
            )
        found += re.findall(r"^\*\*(DESIGN-D2-\d+)", text, re.MULTILINE)
        if not found:
            report.warn("panel", f"{path.relative_to(panel_dir)} 没有 `## FINDING <ID>` 行")
        ids.extend(found)
    seen: set[str] = set()
    ids = [i for i in ids if not (i in seen or seen.add(i))]
    report.ok("panel", f"面板报告解析到 {len(ids)} 条发现（含轮次子目录）")
    return ids


# 两种轮次标题：旧格式 `# Round 13`，以及 R14 起实际使用的
# `## R25 面板轮`。只认旧格式会让 R14–R25 整段逃过 L6/L6.5 收口合同
# （2026-11-10 回归检查实证：R15–R25 全部缺 L6/L6.5，旧正则零报警）。
ROUND_HEADER_RE = re.compile(
    r"^(?:#{1,2}\s+Round\s+(\d+)|##\s+R(\d+)\b)", re.MULTILINE
)


def _round_header_number(m: re.Match) -> int:
    return int(m.group(1) or m.group(2))


def check_round_closure(
    loop_text: str,
    report: Report,
    round_tag: str | None = None,
    v3: bool = False,
    v3_from_round: int | None = None,
) -> None:
    """六步收口合同（DECISIONS-PENDING 第 5 项裁决）：编号 >= 13 的
    轮次小节必须包含 `### L6`（meta 记录）与 `### L6.5`（三问收口
    记录）两个三级小节；R0–R12 为历史轮次，豁免。

    V3 没有历史豁免：每一轮都是交易，关闭时必须有 L6/L6.5 收口记录。
    `round_tag` 指定时只检查该轮：轮次关账只对本轮负责；全历史缺失
    是归档审计项，不能永远阻断当前轮（2026-11-10 用户裁决：流程必须
    能走向收敛，历史欠账不得成为当前轮的永久红灯）。"""
    matches = list(ROUND_HEADER_RE.finditer(loop_text))
    rounds = []
    want = int(round_tag.lstrip("R")) if round_tag else None
    for i, m in enumerate(matches):
        start = m.start()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(loop_text)
        num = _round_header_number(m)
        v3_round = v3 and (v3_from_round is None or num >= v3_from_round)
        if (num >= 13 or v3_round) and (want is None or num == want):
            rounds.append((str(num), start, end))
    if not rounds:
        report.ok("round-closure", "无 R13+ 轮次，收口检查跳过（合同自 R13 起生效）")
        return
    for label, start, end in rounds:
        body = loop_text[start:end]
        missing = [name for name in ("### L6", "### L6.5") if name not in body]
        if missing:
            report.fail(
                "round-closure",
                f"Round {label} 缺少收口小节: {'、'.join(missing)}"
                "（六步收口合同：L6 meta + L6.5 三问记录）",
            )
        else:
            report.ok("round-closure", f"Round {label} 收口小节齐全（### L6 / ### L6.5）")


def _round_open_items(
    ledger_rows: dict[str, dict],
    section_classes: dict[str, dict[str, str]],
    schema_start: int | None = None,
    v3: bool = False,
) -> dict[int, list[str]]:
    """Return open IDs per ledger round for the rounds under the new schema.

    V3 counts open **root causes**, not raw finding IDs: one root row may
    cover several panel findings, and the round closes only when every root
    is closed. V2.5 keeps the historical per-finding count.
    """
    open_rounds: dict[int, list[str]] = {}
    for section, mapping in section_classes.items():
        m = re.match(r"^R(\d+)\b", section)
        if not m:
            continue
        n = int(m.group(1))
        if schema_start is not None:
            if n < schema_start:
                continue
        elif n < 25:
            continue
        opened: set[str] = set()
        for fid, cls in mapping.items():
            row = ledger_rows.get(fid)
            if row is None:
                continue
            is_open = False
            if cls in {"LEGACY", "⑤"}:
                is_open = True
                # V3.3 D-047: a ⑤ row whose action/verify records an approved
                # deferral (用户批准 + date) is resolved-by-decision.
                if "用户批准" in (row["action"] + row["verify"]) and re.search(
                    r"20\d{2}-\d{2}-\d{2}", row["action"] + row["verify"]
                ):
                    is_open = False
            elif cls == "②":
                action = row["action"]
                if "用户批准" not in action or re.search(r"20\d{2}-\d{2}-\d{2}", action) is None:
                    is_open = True
            elif cls in {"①", "④", "⑥"} and re.search(
                r"未修|待修|未实施|TODO", row["verify"], re.IGNORECASE
            ):
                is_open = True
            if is_open:
                if v3 and row.get("root"):
                    opened.add(row["root"])
                else:
                    opened.add(fid)
        if opened:
            open_rounds[int(m.group(1))] = sorted(opened)
    return open_rounds


def _round_key_matches(key: str, target: str) -> bool:
    """`R1` 不得匹配 `R10`/`R12`（r7 前缀串扰判例：--round R1 把 R10+ 台账节并入
    作用域，产出假 FAIL；反向则污染分档聚合）。"""
    return re.match(rf"{re.escape(target.lower())}\b", key.lower()) is not None


def _loop_round_numbers(loop_text: str) -> set[int]:
    nums = {int(n) for n in re.findall(r"^##\s+R(\d+)\b", loop_text, re.MULTILINE)}
    nums |= {int(n) for n in re.findall(r"^#{1,2}\s+Round\s+(\d+)", loop_text, re.MULTILINE)}
    return nums


def check_round_monotonicity(
    loop_text: str,
    ledger_rows: dict[str, dict],
    section_classes: dict[str, dict[str, str]],
    report: Report,
    schema_start: int | None = None,
    v3: bool = False,
    metrics_text: str | None = None,
    round_tag: str | None = None,
) -> None:
    """A round with open items must not have a later round section."""
    open_rounds = _round_open_items(ledger_rows, section_classes, schema_start, v3)
    loop_rounds = _loop_round_numbers(loop_text)
    ledger_rounds = {
        int(m.group(1))
        for section in section_classes
        if (m := re.match(r"^R(\d+)\b", section))
    }
    # V3.4 最小内核（N0/R31 判例）：轮序门原只从台账行推 open 集——「不写台账的
    # 未关闭轮」结构性逃逸。双源补洞：metrics 块（须含真实账目键，防一行空标题
    # 绕过；区间标题按全部轮号登记）也算账。
    metrics_rounds: set[int] = set()
    metrics_open: set[int] = set()
    if metrics_text:
        for title, body in _metric_blocks(metrics_text):
            if not re.search(r"发现总数|轮结果|未闭环", body):
                continue
            # r16（P0-1 判例）：非锚定匹配让「标题提及他轮」即视为有账——只认
            # 块标题开头的轮号及显式区间。
            _mt_head = re.match(r"R(\d+)(?:\s*[–-]\s*R(\d+))?", title)
            ns = (
                [int(_mt_head.group(1))]
                + ([int(_mt_head.group(2))] if _mt_head.group(2) else [])
                if _mt_head
                else []
            )
            metrics_rounds.update(ns)
            # r7（对抗判例）：双源只看「有没有账」不看「账说什么」——metrics 自报
            # 未闭环>0 的轮同样不得静默越序。
            _mo = re.search(r"^\s*-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
            # r8（对抗判例）：缺键不得默认已关闭——无 `未闭环` 键的轮按开放计。
            if _mo is None or int(_mo.group(1)) > 0:
                metrics_open.update(ns)
    all_rounds = loop_rounds | ledger_rounds | metrics_rounds
    want = int(round_tag.lstrip("R")) if round_tag else None
    for r, ids in sorted(open_rounds.items()):
        later = sorted(x for x in all_rounds if x > r)
        if later:
            report.fail(
                "round-order",
                f"R{r} 有未闭环 {len(ids)} 项（{', '.join(ids[:5])}{'…' if len(ids) > 5 else ''}），"
                f"却已出现 R{later[0]}；禁止带病进入下一轮，必须在 R{r} 内修完",
            )
        else:
            report.ok("round-order", f"R{r} 未闭环项未泄漏到下一轮")
    # 双源断链检查：loop 有轮次小节但台账与 metrics 双缺 = 未闭合轮，不得静默
    # 越序；`--round R<N>` 只对本轮及更早负责——更晚的断链在该轮时点尚不存在，
    # 不得追溯判红（r5 回归审查 H5）。
    if schema_start is not None:
        for r in sorted(loop_rounds):
            if r < schema_start:
                continue
            if want is not None and r > want:
                continue
            if r in ledger_rounds or (r in metrics_rounds and r not in metrics_open):
                continue
            # r15（P0-1 判例）：无后继轮的注入（伪造最大轮号）同样必须 FAIL——
            # 「未闭合」本身即FAIL，不以「是否有后继轮」为条件（原实现让注入的
            # 最大轮静默逃逸）。
            later = sorted(x for x in all_rounds if x > r)
            why = (
                "metrics 自报未闭环>0" if r in metrics_open
                else "既无台账节也无 metrics 块（未闭合轮）"
            )
            if later:
                report.fail(
                    "round-order",
                    f"R{r} 在 loop.md 有轮次小节，但{why}；"
                    f"却已出现 R{later[0]}——未关闭轮不得静默越序（N0）",
                )
            elif round_tag is None or r <= int(round_tag.lstrip("R") or 0):
                report.fail(
                    "round-order",
                    f"R{r} 在 loop.md 有轮次小节，但{why}——无账轮次不得关闭（N0）",
                )


def enforce_adjacent_only_dispositions(
    ledger_rows: dict[str, dict],
    report: Report,
    schema_start: int | None,
) -> None:
    """V3.2 §4.1a: `②` is adjacent-scope only; `② 裁决` is illegal.

    A same-document pending-design item must be ⑥ implemented in-round (proxy
    adjudication) or the round goes BLOCKED — never transferred into the next
    round. Sections before `schema_start` are historical and exempt.

    A `②` row carrying `用户批准` + date is a historical adjudicated transfer
    (legal under V3.0, e.g. R26's D-043 batch): it is exempt here because its
    debt is separately enforced by the `open-debt` gate — double-flagging the
    same escape only buries the live signal.
    """
    for fid, row in ledger_rows.items():
        if row.get("class") != "②":
            continue
        section = row.get("section", "")
        section_round = _section_round(section)
        if section_round is None:
            # Tables under unparseable headings (处置索引 / Round N 明细) are
            # pre-V3 history: they cannot be dated, so they are never flagged.
            continue
        if (
            schema_start is not None
            and section_round < schema_start
        ):
            continue
        action = row.get("action", "")
        if "用户批准" in action and re.search(r"20\d{2}-\d{2}-\d{2}", action):
            continue
        if re.match(r"^\s*②\s*相邻", action) is None:
            preview = action[:36] + ("…" if len(action) > 36 else "")
            report.fail(
                "disposition-v32",
                f"{fid} 的 ② 不是相邻范围形态（`{preview}`）；V3.2 起同篇待裁决项"
                "只能 ⑥ 本轮实施或 BLOCKED，不得转出（§4.1a）",
            )


def check_run_dir(
    run_dir: Path,
    panel_dir: Path | None = None,
    round_tag_override: str | None = None,
    audit_templates: bool = False,
) -> Report:
    report = Report()
    if not run_dir.is_dir():
        report.fail("file", f"run 目录不存在: {run_dir}")
        return report

    loop_text = read_text(run_dir / "loop.md", report, "loop.md")
    metrics_text = read_text(run_dir / "metrics.md", report, "metrics.md")
    ledger_text = read_text(run_dir / "defect-ledger.md", report, "defect-ledger.md")
    # r16（P0-4 判例）：全角 Ｒ/ＮＯＴ 等载体绕过全部标题与词表匹配——三件套
    # 读入即做全角 ASCII 归一（面板文本在 collect/_panel_severities 内同样归一）。
    _FW_TBL = str.maketrans({chr(0xFF01 + i): chr(0x21 + i) for i in range(94)})
    if loop_text is not None:
        loop_text = loop_text.translate(_FW_TBL)
    if metrics_text is not None:
        metrics_text = metrics_text.translate(_FW_TBL)
    if ledger_text is not None:
        ledger_text = ledger_text.translate(_FW_TBL)
    schema_start = schema_start_round(loop_text)
    if schema_start is None and metrics_text is not None:
        schema_start = schema_start_round(metrics_text)
    # r14（P0-1 判例）：from 轮标记本身可注入——`schema: V3.4 from R99` 写进
    # loop 时 schema_start=99，V3 门对 <99 轮全体休眠且钳回锚点=注入值。
    # 自校验：起点轮不得晚于「已见最大轮号 + 1」，否则标记自报无效，钳回 1。
    _led_hdrs_early = set()
    _cur_hdr = None
    for _ln in (ledger_text or "").splitlines():
        if _ln.startswith("## "):
            _cur_hdr = _ln
        elif _ln.startswith("|") and not re.match(r"^\|[\s-]+\|", _ln) and _cur_hdr:
            _m_h = re.match(r"^##\s+R(\d+)\b", _cur_hdr)
            if _m_h:
                _led_hdrs_early.add(int(_m_h.group(1)))
    _max_seen_loop_early = max(
        (r for r in _loop_round_numbers(loop_text or "") if r in _led_hdrs_early),
        default=0,
    )
    if schema_start is not None and schema_start > _max_seen_loop_early:
        # r19（P0-4 判例）：+1 余量让 from R2+仅 R1 逃逸——收紧为 ≤ max_seen。
        schema_start = 1
    v3_doc = is_v3_schema(loop_text) or is_v3_schema(metrics_text)
    if round_tag_override and schema_start is None:
        report.fail(
            "schema",
            "缺少 `schema: V2.5|V3.0 from R<N>` 标记；轮次关账必须声明本文档从哪一轮起受"
            "状态机约束（01 写 `from R25`，02 起写 `from R1`，否则新文档会落回旧 schema"
            "无限全量轮）",
        )
    if panel_dir is None:
        panel_dir = run_dir / "panel"
        if (schema_version(loop_text) or (0, 0)) >= (3, 4):
            # V3.5 制品链：V3.4 起面板必须入库在 run 目录；tmp 只作工作副本。
            pass
        elif not panel_dir.is_dir():
            # 规范布局把原始面板输出放在 tmp/review-eval/<stage>/<doc>/panel；
            # 从 run 目录向上找第一个存在该相对路径的祖先（仓库根）。
            for parent in run_dir.resolve().parents:
                candidate = (
                    parent
                    / "tmp"
                    / "review-eval"
                    / run_dir.parent.name
                    / run_dir.name
                    / "panel"
                )
                if candidate.is_dir():
                    panel_dir = candidate
                    break
    if round_tag_override:
        round_tag = round_tag_override
        if panel_dir is not None:
            # `--round R25` 自动收窄到 panel/round25；只有显式存在该
            # 子目录时才切换，避免把单文件测试夹具改坏。
            scoped_panel = panel_dir / f"round{round_tag.lstrip('R').lstrip('0') or '0'}"
            if scoped_panel.is_dir():
                panel_dir = scoped_panel
    else:
        round_match = re.search(r"round(\d+)", panel_dir.name) if panel_dir else None
        round_tag = f"R{int(round_match.group(1))}" if round_match else None
    # A doc may transition from V2.5 to V3 at a specific round (01: R26).
    # `--round R25` on such a doc must still run the V2.5 checks.
    v3 = v3_doc and (
        round_tag is None
        or schema_start is None
        or int(round_tag.lstrip("R")) >= schema_start
    )
    _combined_ver = max(
        schema_version(loop_text) or (0, 0),
        schema_version(metrics_text) or (0, 0),
    )
    v33_doc_flag = _combined_ver >= (3, 3)
    v33_from = (
        v33_start_round(loop_text, fallback=schema_start) if v33_doc_flag else None
    )
    v34_doc_flag = _combined_ver >= (3, 4)
    # r9（考场判例）：制品链四项加固——
    # ① 适用面按组合 schema（loop ∪ metrics，防载体分脑）与 v34 标记起点轮
    #    （历史层回放不追溯）；② FREEZE 豁免要求文件非空且含 CONVERGED/
    #    FREEZE-WITH-BACKLOG（0 字节或自相矛盾的 FREEZE 不解锁）；③ 空制品不算
    #    证据（面板文件必须非空且含 FINDING/SUGGESTION/CLAIM 或「无发现」声明）；
    #    ④ INDEX 计数与文件内容、与 metrics `发现总数` 对账；--panel-dir 不得替代
    #    入库面板；certify 回执要求非空且无否定性结论（内容细审仍归 §4.7 人工）。
    _freeze_ok = False
    _freeze_path = run_dir / "FREEZE.md"
    if _freeze_path.is_file():
        # r13（P0-3/4 判例）：全角字符绕过极性扫描（ＮＯＴ/ＣＡＮＤＩＤＡＴＥ/全角？）、
        # 未定词残缺（暂判/拟）、双 H1 一否定一肯定时 any() 取一、正文 H1 深处
        # 解锁。修：全文全角归一；标题候选限**前三非空行**；带终态的标题行只要
        # 有一条被否定/未定标记污染即**毒化**整个 FREEZE（all 语义，不是 any）。
        _fw_tbl = str.maketrans(
            {chr(0xFF01 + i): chr(0x21 + i) for i in range(94)}
        )
        _ft = _freeze_path.read_text(encoding="utf-8").translate(_fw_tbl)
        # r17（P0-1 判例）：Unicode 连字符族（– — ‐ ‑ ‒ −）与数学字母（𝐑𝟏）对
        # 极性/标题扫描隐身——归一为 ASCII 等价物（连字符→-；数学字母→其基数字符）。
        _ft = re.sub(r"[\u2010-\u2015\u2212]", "-", _ft)
        _ft = "".join(
            chr(ord(c) - 0x1D400 + ord("A"))
            if 0x1D400 <= ord(c) <= 0x1D419  # 数学粗体大写 A–Z
            else chr(ord(c) - 0x1D7CE + ord("0"))
            if 0x1D7CE <= ord(c) <= 0x1D7D7  # 数学粗体数字 0–9
            else c
            for c in _ft
        )
        # r18（P0-① 判例）：零宽字符（ZWSP/ZWNJ/ZWJ/BOM）使极性扫描失明——剥离。
        _ft = _strip_invisible(_ft)
        _nonempty = [ln for ln in _ft.splitlines() if ln.strip()]
        _title_lines = [
            ln for ln in _nonempty[:3] if re.match(r"^#\s+.*FREEZE", ln)
        ]
        _TERMINAL = r"(?<![A-Za-z0-9_-])(CONVERGED|FREEZE-WITH-BACKLOG)\s*[）)]?\s*$"
        # r14（P0-2）：词表补 不/非/无/曾/将/可能/勿/REOPEN——「非 CONVERGED」
        # 「CONVERGED 未达成」类单行不再解锁；r14（P0-3）：毒化扫描**全部含
        # FREEZE 的标题行**（H1/H2，不带终态的否定行同样毒化），不再只看带终态
        # 后缀的行——「… CONVERGED 未达成」+「… CONVERGED」双行逃逸关闭。
        _NEG = (
            r"尚未|未|没|不|非|无|曾|将|可能|勿|NOT|CANDIDATE|REOPEN"
            r"|待|暂|拟|存疑|FAIL|解除|撤|吊销|\?"
        )
        _title_all = [ln for ln in _ft.splitlines() if re.match(r"^#{1,2}\s+.*FREEZE", ln)]
        # r15 裁定：正文毒化撤销（误伤合法历史叙述），FREEZE 状态以标题契约为准。
        _terminal_titles = [
            ln for ln in _title_lines if re.search(_TERMINAL, ln)
        ]
        _poison = any(re.search(_NEG, ln, re.IGNORECASE) for ln in _title_all)
        _freeze_ok = (
            len(_ft.strip()) > 0
            and bool(_terminal_titles)
            and not _poison
        )
    # r12（P1-1 判例）：V3.4 起点轮必须解析 V3.4 专属标记——V33_MARKER_RE 硬编码
    # V3.3，从不匹配 `schema: V3.4 from R<N>`，导致起点轮退回 schema_start、历史层
    # 回放被制品链追溯。载体 = V3.4 标记所在文件（loop 优先，metrics 兜底）。
    if (schema_version(loop_text) or (0, 0)) >= (3, 4):
        _carrier34 = loop_text
    elif _combined_ver >= (3, 4):
        _carrier34 = metrics_text or ""
    else:
        _carrier34 = None
    if _carrier34 is not None:
        _m34 = re.search(
            r"schema\s*[:：]\s*V3\.4(?:\s+from\s+R(\d+))?", _carrier34, re.IGNORECASE
        )
        _v34_from = (
            int(_m34.group(1))
            if _m34 and _m34.group(1)
            else 1  # r21：fallback 不复用可能被幽灵头撑大的 schema_start
        )
        # r13（P0-5 判例）：from 轮零校验——注入 `from R99` 可豁免全部轮的制品链。
        # 起点轮不得晚于「已见最大轮号 + 1」；越界即视为自报无效，钳回 schema_start。
        # r19（P0-1 判例）：幽灵轮头（只有标题无台账无 metrics）抬 max_seen 使
        # from 标记合法化——max_seen 只计有账轮次。
        _led_secs = {}
        for _ln in (ledger_text or "").splitlines():
            if _ln.startswith("|") and "|" in _ln[1:]:
                _led_secs[_ln] = True
        _led_hdrs = set()
        _cur_hdr = None
        for _ln in (ledger_text or "").splitlines():
            if _ln.startswith("## "):
                _cur_hdr = _ln
            elif _ln.startswith("|") and not re.match(r"^\|[\s-]+\|", _ln) and _cur_hdr:
                _m_h = re.match(r"^##\s+R(\d+)\b", _cur_hdr)
                if _m_h:
                    _led_hdrs.add(int(_m_h.group(1)))
        _met_hdrs = {
            int(n)
            for title, body in _metric_blocks(metrics_text or "")
            if re.search(r"发现总数|轮结果|未闭环", body)
            for n in re.findall(r"^R(\d+)", title)
        }
        _backed = set(_loop_round_numbers(loop_text)) & (_led_hdrs | _met_hdrs)
        _max_seen = max(_backed, default=0)
        # r18（P0-③ 判例）：max_seen+1 的余量正是注入向量（from R2 = max_seen+1
        # 的一行注入 + 轮型 discovery + 无制品面板 = 整门 exit 0）。收紧为
        # from ≤ max_seen——V3.4 标记声明的是历史事实，不是未来计划。
        if _v34_from > _max_seen:
            _v34_from = schema_start or 1
    _round_no = int(round_tag_override.lstrip("R")) if round_tag_override else None
    _artifact_required = False
    if round_tag_override:
        if _combined_ver >= (3, 4):
            _artifact_required = (
                _v34_from is None or _round_no is None or _round_no >= _v34_from
            )
        elif _combined_ver >= (3, 3):
            _artifact_required = not _freeze_ok
    if _artifact_required and round_tag_override:
        _rk = round_tag_override.lstrip("R").lstrip("0") or "0"
        _scoped = run_dir / "panel" / f"round{_rk}"
        if panel_dir is not None and run_dir.resolve() not in panel_dir.resolve().parents:
            report.fail(
                "panel-artifact",
                "面板目录位于 run 目录之外（tmp 工作副本或外部 --panel-dir）："
                "V3.5 制品链要求以入库面板 panel/round<N>/ 为准，外部目录不得替代",
            )
        if not (_scoped.is_dir() and any(_scoped.glob("*.md"))):
            report.fail(
                "panel-artifact",
                f"面板制品未入库：runs/<stage>/<doc>/panel/round{_rk}/ 缺失或为空"
                "（V3.5 制品链；tmp 只作工作副本）",
            )
        else:
            _idx = _scoped / "INDEX.md"
            if not _idx.is_file():
                report.fail(
                    "panel-artifact",
                    f"panel/round{_rk}/INDEX.md 缺失（python3 tools/panel-index.py 生成）",
                )
            else:
                import hashlib as _hashlib

                _rows: dict[str, dict] = {}
                for _line in _idx.read_text(encoding="utf-8").splitlines():
                    _m = re.match(
                        r"^\|\s*([^|]+?)\s*\|\s*([0-9a-f]{64})\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*([a-z-]+)\s*\|$",
                        _line,
                    )
                    if _m:
                        _rows[_m.group(1)] = {
                            "sha256": _m.group(2),
                            "findings": int(_m.group(3)),
                            "suggestions": int(_m.group(4)),
                            "claims": int(_m.group(5)),
                        }
                _files = {f.name: f for f in _scoped.glob("*.md") if f.name != "INDEX.md"}
                if not _rows:
                    report.fail("panel-artifact", f"panel/round{_rk}/INDEX.md 无有效行")
                _panel_total = 0
                for _name, _rec in _rows.items():
                    if _name not in _files:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 在索引中但文件不存在",
                        )
                        continue
                    _fpath = _files[_name]
                    _cur = _hashlib.sha256(_fpath.read_bytes()).hexdigest()
                    if _cur != _rec["sha256"]:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} sha256 与 INDEX.md 不一致"
                            "（面板被改动而未重生成索引）",
                        )
                    _text = _fpath.read_text(encoding="utf-8")
                    if len(_text.strip()) == 0:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 是空文件（空制品不算面板证据）",
                        )
                        continue
                    _nf = len(re.findall(r"^##\s+FINDING\b", _text, re.MULTILINE))
                    _nc = len(re.findall(r"^##\s+CLAIM\s+\d+", _text, re.MULTILINE))
                    _ns = len(
                        re.findall(
                            r"^#{2,3}\s+SUGGESTION\s+[A-Za-z][A-Za-z0-9_-]*",
                            _text,
                            re.MULTILINE,
                        )
                    )
                    # V3.6-r2-F4（axis-a 判例：契约与门不一致）：`review-evals.md`
                    # 第 9 条允许在 `loop.md §1` 写「无发现面板：<file>（理由）」来声明
                    # 一个没有 FINDING 的面板（如 design-d1 只有 DECISION、refute 只有
                    # CLAIM 走附加表）。原门只认面板文件本体里的「无发现」——照规则写的
                    # reviewer 会被判「空壳面板」。现在两处都认：本体声明 **或** loop §1
                    # 的具名声明（必须点名该文件，防止一句空话豁免所有面板）。
                    if _nf + _nc + _ns == 0 and not _panel_empty_ok(
                        _name, _text, loop_text
                    ):
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 既无 FINDING/SUGGESTION/CLAIM "
                            "也无「无发现」声明（空壳面板；loop §1 具名声明亦可）",
                        )
                    if (
                        _nf != _rec["findings"]
                        or _ns != _rec["suggestions"]
                        or _nc != _rec["claims"]
                    ):
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 计数与 INDEX 不一致"
                            f"（FINDING {_nf}≠{_rec['findings']} / SUGG {_ns}≠{_rec['suggestions']}"
                            f" / CLAIM {_nc}≠{_rec['claims']}；重跑 panel-index.py）",
                        )
                    _panel_total += _nf + _nc + _ns
                for _name in _files:
                    if _name not in _rows:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 存在但未入 INDEX.md",
                        )
                # INDEX 原始发现总数 vs metrics `发现总数`（r9 O5：不变式对账）
                if metrics_text is not None:
                    _mb = _pick_metric_block(metrics_text, report, round_tag_override, strict=True)[1]
                    _mt = re.search(r"^-\s*发现总数\s*[:：]\s*(\d+)", _mb, re.MULTILINE)
                    if _mt is not None and int(_mt.group(1)) != _panel_total:
                        report.fail(
                            "panel-artifact",
                            f"面板原始发现数 {_panel_total}（INDEX FINDING+SUGGESTION+CLAIM，均须带 ID）与 metrics "
                            f"发现总数 {_mt.group(1)} 不一致（r9 判例：三账必须同源）",
                        )

    if v3 and loop_text is not None:
        if re.search(
            r"^\s*(?:[-*]\s*)?edition\s*[:：]\s*V\d+\.\d+",
            loop_text,
            re.MULTILINE | re.IGNORECASE,
        ) is None:
            report.fail(
                "schema",
                "V3 loop.md §0 缺少 `edition: V3.x` 标记（轮次必须绑定到规则版本）",
            )
    if v3_doc and round_tag_override:
        # V3 轮关闭必须有非空面板目录（mkdir 空目录不算）——r8 判例；r9 复核。
        _empty_panel = panel_dir is None or not panel_dir.is_dir()
        if not _empty_panel:
            _empty_panel = not any(panel_dir.rglob("*.md"))
        if _empty_panel:
            report.fail(
                "panel",
                "V3 轮关闭要求面板证据：找不到 panel 目录或目录为空（至少一个角色报告；"
                "run-dir/panel 或 tmp/review-eval/<stage>/<doc>/panel）",
            )
    panel_ids = collect_panel_findings(
        panel_dir, report, collect_sugg_ids=_artifact_required
    )

    # V3.6（C-34）：减法提案计数门。`metrics.md` 的 `- 减法: 提案 <N> / 采纳 <M>`
    # 的提案数必须等于 design-d2.md 里 `## SUBTRACTION-TABLE` 的数据行数。
    # D2 报告在、但减法提案 0 行 → 报告不完整（review-panel.md §4.3 纪律）。
    # 只对 V3.6+ 生效（V3.5 存量文档不追溯）。逻辑在 check_subtraction 内，
    # 供自测直接调用。
    if _combined_ver >= (3, 6) and round_tag_override:
        _rk6 = round_tag_override.lstrip("R").lstrip("0") or "0"
        _d2_path = None
        for _cand in (
            run_dir / "panel" / f"round{_rk6}" / "design-d2.md",
            (panel_dir / "design-d2.md") if panel_dir else None,
        ):
            if _cand is not None and _cand.is_file():
                _d2_path = _cand
                break
        if metrics_text is not None:
            # V3.6-r1-F2（判例）：D2 缺失不得静默跳过。原实现在
            # `_d2_path is None` 时**整条减法门不执行**——metrics 可以自报
            # 「提案 3 / 采纳 3」而没有任何 D2 实物，门一个字都不说。
            # fail-closed：V3.6 轮没有 D2 报告 = 设计审查缺席，直接 FAIL；
            # metrics 已自报提案数 > 0 而 D2 缺失，另加一条更明确的 FAIL。
            if _d2_path is None:
                _claimed = re.search(
                    r"^-\s*减法\s*[:：]\s*提案\s*(\d+)",
                    metrics_text,
                    re.MULTILINE,
                )
                _n_claimed = int(_claimed.group(1)) if _claimed else 0
                report.fail(
                    "subtraction",
                    f"panel/round{_rk6}/design-d2.md 缺失：V3.6 轮必须有 D2 报告"
                    f"（其 `## SUBTRACTION-TABLE` 是减法提案的唯一实物；"
                    f"metrics 当前自报提案 {_n_claimed}）",
                )
            else:
                check_subtraction(
                    _d2_path.read_text(encoding="utf-8"),
                    metrics_text,
                    report,
                    round_tag_override,
                )

    if loop_text is not None:
        check_loop_sections(loop_text, report, round_tag)
        check_round_closure(
            loop_text, report, round_tag, v3=v3, v3_from_round=schema_start
        )
        check_placeholders(loop_text, report, "loop.md")
    expected_counts = None
    ledger_rows: dict[str, dict] = {}
    section_classes: dict[str, dict[str, str]] = {}
    root_count: int | None = None
    new_roots: int | None = None
    if ledger_text is not None:
        check_placeholders(ledger_text, report, "defect-ledger.md")
        (
            ledger_ids,
            ledger_rows,
            section_ids,
            section_classes,
            root_rows,
        ) = parse_ledger(ledger_text, report, v3=v3, v3_from_round=schema_start)
        if (schema_version(loop_text) or (0, 0)) >= (3, 2):
            enforce_adjacent_only_dispositions(ledger_rows, report, schema_start)
        # V3.3 additions: four-state lifecycle, confirmation rounds, reader-P3
        # admission, origin/severity semantics. Scoped from the V3.3 marker's
        # `from R<N>` so historical rounds stay exempt (01: R31 grandfathered).
        if v33_doc_flag:
            panel_severities = _panel_severities(panel_dir)
            check_v33_admission(
                ledger_rows, panel_severities, report, round_tag, v33_from
            )
            check_v33_lifecycle(
                metrics_text or "",
                ledger_rows,
                section_classes,
                report,
                schema_start,
                v33_from,
            )
        if v3:
            check_v3_evidence(ledger_rows, report, round_tag, schema_start)
        else:
            check_verify_templates(ledger_rows, report)
        if audit_templates:
            audit_template_sections(ledger_text, report)
        if v3:
            if round_tag:
                current_round = int(round_tag.lstrip("R"))
                scoped_roots = {
                    row["root"]
                    for row in ledger_rows.values()
                    if row.get("root")
                    and _round_key_matches(row.get("section", ""), round_tag)
                }
                root_count = len(scoped_roots)
                new_roots = sum(
                    1
                    for root in scoped_roots
                    if origin_round(root_rows.get(root, {}).get("origin", ""))
                    == current_round
                )
            else:
                root_count = len(root_rows)
        panel_set = set(panel_ids)
        ledger_set = set(ledger_ids)
        # Round-scoped coverage: a per-round --panel-dir (round18) must be
        # answered by that round's ledger section. Without this, deleting the
        # newest round's table still passes because short IDs repeat in older
        # rounds (B-51 whole-file loophole).
        if round_tag:
            scoped = {
                fid
                for section, fids in section_ids.items()
                if _round_key_matches(section, round_tag)
                for fid in fids
            }
            # No fallback to the whole ledger: if this round's section is
            # missing, every panel finding must be reported missing.
            coverage_ledger = scoped
            from collections import Counter
            expected_counts: dict[str, int] = {}
            counter: Counter[str] = Counter()
            for section, mapping in section_classes.items():
                if _round_key_matches(section, round_tag):
                    counter.update(
                        cls for fid, cls in mapping.items() if fid in panel_set
                    )
            expected_counts = dict(counter)
            round_n = int(round_tag.lstrip("R"))
            if (schema_start is not None and round_n >= schema_start) or round_n >= 25:
                for fid in sorted(scoped):
                    row = ledger_rows.get(fid)
                    if row is None:
                        continue
                    cls = row["class"]
                    if cls == "LEGACY":
                        report.fail(
                            "round-close",
                            f"{fid} 使用了已删除的 ③；R25+ 只能写 FP（附反证）或 ⑤（未闭环）",
                        )
                    elif cls == "⑤":
                        report.fail(
                            "round-close",
                            f"{fid} 处置类为 ⑤（未闭环）；本轮不得关闭",
                        )
                    elif cls == "②":
                        action = row["action"]
                        if "用户批准" not in action or re.search(r"20\d{2}-\d{2}-\d{2}", action) is None:
                            report.fail(
                                "round-close",
                                f"{fid} 处置类为 ②，但没有用户批准 + YYYY-MM-DD；按未闭环计",
                            )
                    elif cls in {"①", "④", "⑥"} and re.search(
                        r"未修|待修|未实施|TODO", row["verify"], re.IGNORECASE
                    ):
                        report.fail(
                            "round-close",
                            f"{fid} 处置类为 {cls}，但复验写“未修/待修/未实施”；本轮不得关闭",
                        )
        else:
            coverage_ledger = ledger_set
        # Orphan reference set: IDs that appear in ANY panel file (the
        # selected round plus its sibling rounds and the flat files at the
        # parent level). A per-round --panel-dir must not turn other rounds'
        # ledger rows into orphans.
        panel_text_ids = set()
        roots = [panel_dir]
        if panel_dir.parent != panel_dir and panel_dir.parent.is_dir():
            roots.append(panel_dir.parent)
        for root in roots:
            for f in root.rglob("*.md"):
                panel_text_ids.update(
                    re.findall(r"[A-Z]{2,}-[A-Za-z0-9-]+", f.read_text(encoding="utf-8"))
                )
        missing = sorted(panel_set - coverage_ledger)
        # Orphan candidates: only first-cells that look like finding IDs
        # ([A-Z]+-<digits> prefix). Aggregate ledger rows carry prose labels
        # ("断锚簇", "125→124/175→176") that split into non-ID fragments —
        # those are row labels, not orphan findings.
        id_like = re.compile(r"^[A-Z]{2,}-[A-Za-z0-9-]+$")
        orphan = sorted(x for x in (coverage_ledger - panel_set - panel_text_ids) if id_like.match(x))
        if missing:
            report.fail("coverage", "面板发现没有处置行: " + "、".join(missing))
        else:
            report.ok("coverage", "所有面板发现都有处置行")
        if orphan:
            report.fail("coverage", "台账有孤儿处置行: " + "、".join(orphan))
        else:
            report.ok("coverage", "台账没有孤儿行")

    actual_open = None
    if round_tag and ledger_rows:
        open_map = _round_open_items(
            ledger_rows, section_classes, schema_start, v3=v3
        )
        actual_open = len(open_map.get(int(round_tag.lstrip("R")), []))
    # V3.3 §4.0f: OPEN-DEBT is part of the single open count. An unclosed
    # same-document debt row blocks the round regardless of ledger state.
    open_debt_path = run_dir / "OPEN-DEBT.md"
    if open_debt_path.is_file():
        debt_open = check_open_debt(open_debt_path, report, round_tag)
        if actual_open is not None:
            actual_open += debt_open
        elif debt_open:
            report.fail(
                "open-debt",
                f"OPEN-DEBT 未销账 {debt_open} 行且未指定 --round；全档审计同样不得视为已收敛",
            )
    if metrics_text is not None:
        check_metrics(
            metrics_text,
            report,
            round_tag,
            expected_counts,
            actual_open,
            schema_start,
            v3=v3,
            root_count=root_count,
            new_roots=new_roots,
            v33_start=v33_from,
            v34=v34_doc_flag,
        )
        # The all-round checks follow the document's newest schema; the
        # per-round checks above use `v3` (the selected round's segment).
        check_round_type_sequence(metrics_text, report, schema_start, v3=v3_doc)
        if v3_doc:
            check_edition_trial(metrics_text, report, schema_start)
        # V3.4 最小内核（r5 回归审查 B4）：P1-7 同样覆盖 loop 轮次小节——D04 的
        # 「certify 待跑」写在 loop §5 而非 metrics，只扫 metrics 会漏。
        if _artifact_required and round_tag and loop_text is not None:
            _bt, _bb = _pick_metric_block(metrics_text, report, round_tag)
            _closes = re.search(
                r"^-\s*轮结果\s*[:：]\s*(CONVERGED|ROUND-CLOSED)\b",
                _bb,
                re.MULTILINE,
            )
            if _closes:
                _want = int(round_tag.lstrip("R"))
                _cert_dir = run_dir / "certify"
                _cert_files = (
                    list(_cert_dir.glob(f"round{_want}-*.md"))
                    if _cert_dir.is_dir()
                    else []
                )
                if not _cert_files:
                    report.fail(
                        "certify-artifact",
                        f"{round_tag} 为闭合态，certify 独立回执未入库"
                        f"（runs/<stage>/<doc>/certify/round{_want}-*.md；V3.5 制品链）",
                    )
                else:
                    _bad_certs = []
                    # 全角 ASCII 归一（r11 P0-2：ＮＯＴ　ＰＡＳＳ 绕过词表）
                    _fw = str.maketrans(
                        {chr(0xFF01 + i): chr(0x21 + i) for i in range(94)}
                    )
                    for _cert in _cert_files:
                        _ct = _cert.read_text(encoding="utf-8").translate(_fw)
                        # r10（P0-④ 判例）：字节数被纯标点绕过——按**有效字符**
                        # 计数。
                        _ct = _strip_invisible(_ct)
                        _neg_hit = re.search(
                            r"NOT\s*PASS|未通过|没通过"
                            r"|结论\s*[:：]\s*(FAIL|REOPEN)"
                            r"|^\s*REOPEN\b"
                            r"|^\s*[-*>]\s*.*\b(FAIL|REOPEN)\b"
                            r"|^\s*(?:\d+[.)]|#{1,6})\s*.*\b(FAIL|REOPEN)\b"
                            r"|\bFAIL\s*[:：\u2014\u2013\u2010\u2012\u2015\u2212（(]"
                            r"|\bREOPEN\b"
                            r"|(复验|验证|回读|确认)\s*失败"
                            r"|未(修|完成|实施|落地)(?!.*已补)",
                            _ct.translate(str.maketrans({chr(0xFF01 + i): chr(0x21 + i) for i in range(94)})),
                            re.IGNORECASE | re.MULTILINE,
                        ) or _has_lingering_severity(
                            _ct, require_decl=_combined_ver >= (3, 6)
                        )
                        _eff = len(re.sub(r"[^\w\u4e00-\u9fff]", "", _ct))
                        if _neg_hit:
                            _bad_certs.append(
                                f"{_cert.name} 含否定性结论"
                                "（NOT PASS/未通过/没通过/结论 FAIL|REOPEN/独行 REOPEN）"
                            )
                        elif _eff < 20:
                            _bad_certs.append(
                                f"{_cert.name} 有效字符不足（{_eff}）"
                            )
                        elif not any(
                            re.search(
                                r"(结论|判定|验证|确认)[^\n]{0,24}"
                                r"(\bPASS\b|\bCLOSED\b|通过|闭环)",
                                ln,
                                re.IGNORECASE,
                            )
                            and not re.search(
                                r"未|没|不|非|无|曾|将|勿|可能|REOPEN|NOT\s*PASS"
                                r"|FAIL|暂|拟|待|存疑|除|\?|？",
                                ln,
                                re.IGNORECASE,
                            )
                            for ln in _ct.splitlines()
                        ):
                            # r11（P0-2）：词表不可穷尽——改**肯定式结论要求**：
                            # 回执必须自带一行显式通过性结论（结论/判定/验证/确认
                            # + PASS/CLOSED/通过/闭环）。
                            _bad_certs.append(
                                f"{_cert.name} 缺少显式通过性结论行"
                                "（结论/判定/验证/确认 + PASS/CLOSED/通过/闭环）"
                            )
                    if _bad_certs:
                        report.fail(
                            "certify-artifact",
                            f"{round_tag} 的 certify 回执不合格："
                            + "；".join(_bad_certs)
                            + "（内容细审归 §4.7 人工，此处只拦空壳与否定结论）",
                        )
                _matches = list(ROUND_HEADER_RE.finditer(loop_text))
                for _i, _m in enumerate(_matches):
                    if _round_header_number(_m) != _want:
                        continue
                    _end = (
                        _matches[_i + 1].start()
                        if _i + 1 < len(_matches)
                        else len(loop_text)
                    )
                    if re.search(
                        r"certify[^\n]{0,16}(?:待跑|未跑|待补|待安排|未完成|未执行|没跑|延后|排期未定|待复跑|未复跑|尚未启动|回读未做|pending|not run|推迟|todo)",
                        loop_text[_m.start() : _end],
                        re.IGNORECASE,
                    ):
                        report.fail(
                            "round-closure",
                            f"{round_tag} 的 loop 记录写「certify 待跑/未跑」而 metrics "
                            "轮结果为闭合态；闭合声明缺闭合证据（P1-7，loop 侧）",
                        )

    if loop_text is not None:
        check_round_monotonicity(
            loop_text,
            ledger_rows,
            section_classes,
            report,
            schema_start,
            v3=v3,
            metrics_text=metrics_text,
            round_tag=round_tag,
        )

    return report


def print_report(report: Report, as_json: bool) -> None:
    if as_json:
        print(json.dumps(report.as_dict(), ensure_ascii=False, indent=2))
        return
    for item in report.items:
        print(f"[{item['level']:4}] {item['check']}: {item['message']}")
    counts = report.as_dict()["counts"]
    print(
        f"\n汇总: PASS {counts['pass']} / WARN {counts['warn']} / FAIL {counts['fail']}"
    )
    print("结论: " + ("FAIL" if report.failed else "PASS"))


GOOD_LOOP = "\n\n".join(f"## {title}\n本节的占位内容。" for title in SECTION_TITLES)
GOOD_PANEL = """# 面板报告

## FINDING FACT-01
- 位置：§1.1
- 问题：示例
- 影响：示例
- 证据或推理：E1
- 最小修复：示例
- 置信度：高
- 是否需源码核实：否
- 若我错了：示例

## FINDING FACT-02
- 位置：§1.2
- 问题：示例
- 影响：示例
- 证据或推理：E3
- 最小修复：示例
- 置信度：中
- 是否需源码核实：是
- 若我错了：示例
"""
GOOD_LEDGER = """# 缺陷台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修复 §1.1 | E1 | `grep -n x` 命中 |
| FACT-02 | P-FACT | P2 | 措辞 | FP | 反证见 §3 | 文档原文 §3 明确说 x 不是缺陷，且源码未出现该符号 | 不适用 |
| READER-A-01 | reader-a | P3 | 措辞 | ④ | 修 §1.2 | E1 | 旧串 grep 归零 |
"""
GOOD_METRICS = """# 指标

- 发现总数: 3
- 处置: ① 1 / ② 0 / ④ 1 / ⑤ 0 / ⑥ 0 / FP 1
- 未闭环: 0
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1000 / 耗时 1min
"""


def write_fixture(root: Path, loop: str, ledger: str, metrics: str, panel: str) -> Path:
    run_dir = root / "run"
    (run_dir / "panel").mkdir(parents=True)
    (run_dir / "loop.md").write_text(loop, encoding="utf-8")
    (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
    (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
    (run_dir / "panel" / "fact.md").write_text(panel, encoding="utf-8")
    return run_dir


def write_round_fixture(root: Path, ledger: str, metrics: str) -> tuple[Path, Path]:
    """R25+ fixture whose panel dir carries the round tag."""
    run_dir = root / "run"
    panel_dir = run_dir / "panel" / "round25"
    panel_dir.mkdir(parents=True)
    loop = (
        "## R25 面板轮\n\n"
        + GOOD_LOOP.replace("## ", "### ")
        + "\n\n### L6\nmeta 结论与 BLIND-SPOT-DELTA。\n"
        + "\n### L6.5\n三问三去处记录。\n"
    )
    (run_dir / "loop.md").write_text(loop, encoding="utf-8")
    (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
    (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
    (panel_dir / "fact.md").write_text(
        "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：示例\n",
        encoding="utf-8",
    )
    return run_dir, panel_dir


V3_LOOP = (
    "schema: V3.0 from R1\n\n"
    "edition: V3.0\n\n"
    "## R1 面板轮\n\n"
    + GOOD_LOOP.replace(
        "## §5 复验\n本节的占位内容。",
        "## §5 复验\n- certify 新准入: 0。",
    ).replace("## ", "### ")
    + "\n\n### L6\nmeta 结论与 BLIND-SPOT-DELTA。\n"
    + "\n### L6.5\n三问三去处记录。\n"
)

V3_LEDGER = """# 台账

## R1 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-01 | R1 | P-FACT | P1 | 事实 | ① | 修 §1 | E1 | 旧串 grep 归零 |
"""

V3_METRICS = """# 指标

## R1 面板轮

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""


def write_v3_fixture(
    root: Path,
    loop: str = V3_LOOP,
    ledger: str = V3_LEDGER,
    metrics: str = V3_METRICS,
    panel: str = "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：示例\n",
) -> tuple[Path, Path]:
    run_dir = root / "run"
    panel_dir = run_dir / "panel" / "round1"
    panel_dir.mkdir(parents=True)
    (run_dir / "loop.md").write_text(loop, encoding="utf-8")
    (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
    (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
    (panel_dir / "fact.md").write_text(panel, encoding="utf-8")
    return run_dir, panel_dir


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        good_dir = write_fixture(
            Path(tmp), GOOD_LOOP, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL
        )
        good = check_run_dir(good_dir)
        if good.failed:
            print("self-test FAIL: 正确样本被判 FAIL")
            print_report(good, False)
            return 1

    with tempfile.TemporaryDirectory() as tmp:
        bad_ledger = GOOD_LEDGER.replace(
            "| FACT-02 | P-FACT | P2 | 措辞 | FP | 反证见 §3 | 文档原文 §3 明确说 x 不是缺陷，且源码未出现该符号 | 不适用 |\n", ""
        )
        bad_metrics = GOOD_METRICS.replace("- 发现总数: 3", "- 发现总数: 4")
        bad_dir = write_fixture(
            Path(tmp), GOOD_LOOP, bad_ledger, bad_metrics, GOOD_PANEL
        )
        bad = check_run_dir(bad_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed:
            print("self-test FAIL: 缺陷样本没有被判 FAIL")
            return 1
        if "FACT-02" not in messages:
            print("self-test FAIL: 没有报出缺失的 FACT-02")
            print_report(bad, False)
            return 1
        if "不一致" not in messages:
            print("self-test FAIL: 没有报出总数与处置之和不一致")
            print_report(bad, False)
            return 1

    # R25+：已删除的 ③ 必须被拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_legacy = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ③ | 作者取舍 | 无 | 无 |
"""
        metrics_legacy = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 1
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_legacy, metrics_legacy)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "已删除的 ③" not in messages:
            print("self-test FAIL: R25+ 的 ③ 未被拦截")
            print_report(bad, False)
            return 1

    # R25+：FP 没有反证必须被拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_fp = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | FP | 误报 |  | 不适用 |
"""
        metrics_fp = metrics_legacy.replace("| ③ | 作者取舍 | 无 | 无 |", "").replace(
            "处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0",
            "处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 1",
        )
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_fp, metrics_fp)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "没有给出源码/测试/文档原文反证" not in messages:
            print("self-test FAIL: FP 缺反证未被拦截")
            print_report(bad, False)
            return 1

    # 验证轮：零新 in-scope 真缺陷 → 允许 CONVERGED
    with tempfile.TemporaryDirectory() as tmp:
        ledger_v = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修复 | E1 | 测试通过 |
"""
        metrics_v = """# 指标

- 发现总数: 1
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: verification
- 新 in-scope 真缺陷: 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_v, metrics_v)
        good = check_run_dir(run_dir, panel_dir)
        if good.failed:
            print("self-test FAIL: verification 零新增被判 FAIL")
            print_report(good, False)
            return 1

    # 验证轮：有新 in-scope 真缺陷 → 不得 CONVERGED
    with tempfile.TemporaryDirectory() as tmp:
        metrics_bad = metrics_v.replace("新 in-scope 真缺陷: 0", "新 in-scope 真缺陷: 1")
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_v, metrics_bad)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "不得 CONVERGED" not in messages:
            print("self-test FAIL: verification 有新发现未被拦截")
            print_report(bad, False)
            return 1

    # 上一轮未闭环时出现下一轮小节 → 必须拦截（禁止带病进下一轮）
    with tempfile.TemporaryDirectory() as tmp:
        ledger_open = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ⑤ | 缺源码核实路径：R26 取证 |  | 未闭环 |
"""
        metrics_open = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 1
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_open, metrics_open)
        loop = (run_dir / "loop.md").read_text()
        (run_dir / "loop.md").write_text(loop + "\n## R26 面板轮\n\n### §0 基线\n占位\n", encoding="utf-8")
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "禁止带病进入下一轮" not in messages:
            print("self-test FAIL: 上一轮未闭环却出现 R26 未被拦截")
            print_report(bad, False)
            return 1

    # 六步收口合同：R13+ 轮次含 ### L6/### L6.5 → 通过
    with tempfile.TemporaryDirectory() as tmp:
        loop_r13 = (
            GOOD_LOOP
            + "\n\n# Round 13（六步收口首轮）\n\n### L6\n"
            + "meta 结论与 BLIND-SPOT-DELTA。\n\n### L6.5\n三问三去处记录。\n"
        )
        ok_dir = write_fixture(Path(tmp), loop_r13, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL)
        ok = check_run_dir(ok_dir)
        if ok.failed:
            print("self-test FAIL: 带 L6/L6.5 收口小节的 R13 被判 FAIL")
            print_report(ok, False)
            return 1

    # 六步收口合同：R13+ 轮次缺 ### L6.5 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        loop_bad = GOOD_LOOP + "\n\n# Round 13（六步收口首轮）\n\n### L6\nmeta 结论。\n"
        bad_dir = write_fixture(Path(tmp), loop_bad, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL)
        bad = check_run_dir(bad_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "L6.5" not in messages:
            print("self-test FAIL: R13 缺 ### L6.5 未被拦截")
            print_report(bad, False)
            return 1

    # 六步收口合同：现代标题 `## R25 面板轮` 同样受约束（旧正则只认
    # `# Round`，R14–R25 曾整段逃检）
    with tempfile.TemporaryDirectory() as tmp:
        loop_modern = (
            "## R25 面板轮\n\n"
            + GOOD_LOOP.replace("## ", "### ")
            + "\n\n### L6\nmeta 结论。\n"
        )
        bad_dir = write_fixture(Path(tmp), loop_modern, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL)
        bad = check_run_dir(bad_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "Round 25" not in messages or "L6.5" not in messages:
            print("self-test FAIL: 现代 R25 标题缺 ### L6.5 未被拦截")
            print_report(bad, False)
            return 1

    # metrics 手填 未闭环: 0 掩盖台账 ⑤ → 必须按台账实际数拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_lie = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ⑤ | 还缺源码核实，取证路径 X | panel | 未闭环 |
"""
        metrics_lie = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_lie, metrics_lie)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "实际未闭环 1" not in messages:
            print("self-test FAIL: metrics 手填 0 未被台账实际数拦截")
            print_report(bad, False)
            return 1

    # 轮型状态机：verification 之后不得再出现新轮
    with tempfile.TemporaryDirectory() as tmp:
        ledger_one = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修复 | E1 | 测试通过 |
"""
        def metrics_block(round_no: int, rtype: str, extra: str = "") -> str:
            return f"""## R{round_no}

- 发现总数: 1
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: {rtype}
{extra}- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        metrics_seq = "# 指标\n\n" + metrics_block(25, "discovery") + metrics_block(
            26, "verification", "- 新 in-scope 真缺陷: 0\n"
        ) + metrics_block(27, "verification", "- 新 in-scope 真缺陷: 0\n")
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_one, metrics_seq)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "不得开新轮" not in messages:
            print("self-test FAIL: verification 之后出现 R27 未被拦截")
            print_report(bad, False)
            return 1

    # 轮型状态机：一篇文档只允许一次 discovery
    with tempfile.TemporaryDirectory() as tmp:
        metrics_two_discovery = "# 指标\n\n" + metrics_block(25, "discovery") + metrics_block(
            26, "discovery"
        )
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_one, metrics_two_discovery)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "多个 discovery" not in messages:
            print("self-test FAIL: 第二次 discovery 未被拦截")
            print_report(bad, False)
            return 1

    # 新文档（02 起）从 R1 就必须受 V2.5 状态机约束：不能靠轮号 >=25
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round1"
        panel_dir.mkdir(parents=True)
        loop_r1 = (
            "schema: V2.5 from R1\n\n## R1 面板轮\n\n"
            + GOOD_LOOP.replace("## ", "### ")
        )
        ledger_r1 = """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ⑤ | 还缺源码核实，取证路径 X | panel | 未闭环 |
"""
        metrics_r1 = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 1
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop_r1, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger_r1, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics_r1, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：x\n", encoding="utf-8"
        )
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "R1" not in messages or "未闭环" not in messages:
            print("self-test FAIL: 02 起 R1 的 V2.5 schema 未生效")
            print_report(bad, False)
            return 1

    # 新文档不写 schema 标记 → --round 必须拒绝，避免落回旧 schema
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round1"
        panel_dir.mkdir(parents=True)
        loop_no_schema = "## R1 面板轮\n\n" + GOOD_LOOP.replace("## ", "### ")
        (run_dir / "loop.md").write_text(loop_no_schema, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger_r1, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics_r1, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：x\n", encoding="utf-8"
        )
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "缺少 `schema: V2.5" not in messages:
            print("self-test FAIL: 缺 schema 标记的新文档未被拦截")
            print_report(bad, False)
            return 1

    with tempfile.TemporaryDirectory() as tmp:
        panel_range = "# 面板\n\n" + "\n\n".join(
            f"## FINDING READER-A-{i:02d}\n- 位置：§1\n- 问题：x\n" for i in (1, 2, 3)
        )
        ledger_range = """# 台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| READER-A-01..03 | reader-a | P2 | 结构 | ④ | 修 | E1 | grep |
"""
        metrics_range = """# 指标

- 发现总数: 3
- 处置: ① 0 / ② 0 / ④ 3 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir = write_fixture(Path(tmp), GOOD_LOOP, ledger_range, metrics_range, panel_range)
        result = check_run_dir(run_dir)
        if result.failed:
            print("self-test FAIL: 范围行 READER-A-01..03 未被展开覆盖")
            print_report(result, False)
            return 1

    # 轮次作用域：round2 的 FACT-01 不能由 R1 的台账行顶替
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        (run_dir / "loop.md").write_text(GOOD_LOOP, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：x\n", encoding="utf-8"
        )
        (run_dir / "metrics.md").write_text(
            GOOD_METRICS, encoding="utf-8"
        )
        (run_dir / "defect-ledger.md").write_text(
            """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | fact | P1 | 事实 | ① | 修 | E1 | grep |

## R2 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-02 | fact | P1 | 事实 | ① | 修 | E1 | grep |
""",
            encoding="utf-8",
        )
        result = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in result.items)
        if not result.failed or "FACT-01" not in messages:
            print("self-test FAIL: 轮次作用域未拦截 R1 台账顶替 R2 发现")
            print_report(result, False)
            return 1

    # 轮转闸（2026-11-09）：④ 行没有已修证据 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_4 = GOOD_LEDGER.replace(
            "| READER-A-01 | reader-a | P3 | 措辞 | ④ | 修 §1.2 | E1 | 旧串 grep 归零 |",
            "| READER-A-01 | reader-a | P3 | 措辞 | ④ | 修 §1.2 | E1 | （待补） |",
        )
        bad4_dir = write_fixture(Path(tmp), GOOD_LOOP, ledger_4, GOOD_METRICS, GOOD_PANEL)
        bad4 = check_run_dir(bad4_dir)
        messages4 = " ".join(item["message"] for item in bad4.items)
        if not bad4.failed or "轮转闸" not in messages4:
            print("self-test FAIL: ④ 无证据未被轮转闸拦截")
            print_report(bad4, False)
            return 1

    # V3：完整交易轮（full + root/origin + certify 0）→ 通过
    with tempfile.TemporaryDirectory() as tmp:
        run_dir, panel_dir = write_v3_fixture(Path(tmp))
        good = check_run_dir(run_dir, panel_dir, "R1")
        if good.failed:
            print("self-test FAIL: V3 完整交易轮被判 FAIL")
            print_report(good, False)
            return 1

    # V3：verification 轮型 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("轮型: full", "轮型: verification")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "V3 轮型只能是 full" not in messages:
            print("self-test FAIL: V3 verification 轮型未被拦截")
            print_report(bad, False)
            return 1

    # V3：缺少 certify 新准入 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("- certify 新准入: 0\n", "")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "certify 新准入" not in messages:
            print("self-test FAIL: V3 缺 certify 新准入未被拦截")
            print_report(bad, False)
            return 1

    # V3：台账缺 根因 ID/origin → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger = """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修 §1 | E1 | 旧串 grep 归零 |
"""
        run_dir, panel_dir = write_v3_fixture(Path(tmp), ledger=ledger)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "V3 台账缺少" not in messages:
            print("self-test FAIL: V3 台账缺根因列未被拦截")
            print_report(bad, False)
            return 1

    # V3：唯一根因数与台账不一致 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("- 唯一根因数: 1", "- 唯一根因数: 2")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "唯一根因数 2 与台账根因 ID 数 1 不一致" not in messages:
            print("self-test FAIL: V3 唯一根因数不一致未被拦截")
            print_report(bad, False)
            return 1

    # V3：certify 新准入 >0 且未闭环=0 → 不得关闭
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("- certify 新准入: 0", "- certify 新准入: 1")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "不得关闭" not in messages:
            print("self-test FAIL: V3 certify 新准入>0 未被拦截")
            print_report(bad, False)
            return 1

    # V3 过渡：schema: V3.0 from R2 时，历史 R1 台账没有根因列不能被判红；
    # 只检查 R2 起的新表。
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        sections = GOOD_LOOP.replace(
            "## §5 复验\n本节的占位内容。",
            "## §5 复验\n- certify 新准入: 0。",
        ).replace("## ", "### ")
        sections_r1 = GOOD_LOOP.replace("## ", "### ")
        loop = (
            "schema: V3.0 from R2\n\n"
            "edition: V3.0\n\n"
            "## R1 面板轮\n\n"
            + sections_r1
            + "\n\n## R2 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n"
        )
        ledger = """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |

## R2 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-02 | R2 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |
"""
        metrics = """# 指标

## R2

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-02\n- 位置：§1\n- 问题：示例\n",
            encoding="utf-8",
        )
        good = check_run_dir(run_dir, panel_dir, "R2")
        if good.failed:
            print("self-test FAIL: V3 过渡轮（R1 历史表无根因列）被判 FAIL")
            print_report(good, False)
            return 1

    # V3：同一 edition 第 3 轮仍未闭环 → WARN 要求走 L7/EDITION-RETRO
    metrics_trial = "# 指标\n\n" + "\n\n".join(
        f"## R{i}\n\n- 未闭环: 1\n- 轮型: full\n- edition: V3.0"
        for i in (1, 2, 3)
    )
    trial_report = Report()
    check_edition_trial(metrics_trial, trial_report, 1)
    if not any(
        item["level"] == "WARN" and "第 4 轮" in item["message"]
        for item in trial_report.items
    ):
        print("self-test FAIL: 同一 edition 第 3 轮未闭环未触发 L7 WARN")
        return 1

    # V3：R1 的根因未闭环（⑤）却出现 R2 → 必须拦截（按根因，不按原始 ID）
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        sections = GOOD_LOOP.replace(
            "## §5 复验\n本节的占位内容。",
            "## §5 复验\n- certify 新准入: 0。",
        ).replace("## ", "### ")
        loop = (
            "schema: V3.0 from R1\n\n"
            "edition: V3.0\n\n"
            "## R1 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n\n"
            "## R2 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n"
        )
        ledger = """# 台账

## R1 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-01 | R1 | P-FACT | P1 | 事实 | ⑤ | 还缺源码核实，取证路径 X | panel | 未闭环 |

## R2 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-002 | FACT-02 | R2 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |
"""
        metrics = """# 指标

## R1

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 1
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min

## R2

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-02\n- 位置：§1\n- 问题：示例\n",
            encoding="utf-8",
        )
        bad = check_run_dir(run_dir, panel_dir, "R2")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "禁止带病进入下一轮" not in messages:
            print("self-test FAIL: V3 R1 根因未闭环却出现 R2 未被拦截")
            print_report(bad, False)
            return 1

    # V3：同一根因跨轮复发（origin=R1）不计“本轮新准入”，R2 可关闭
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        sections = GOOD_LOOP.replace(
            "## §5 复验\n本节的占位内容。",
            "## §5 复验\n- certify 新准入: 0。",
        ).replace("## ", "### ")
        loop = (
            "schema: V3.0 from R1\n\n"
            "edition: V3.0\n\n"
            "## R1 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n\n"
            "## R2 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n"
        )
        ledger = """# 台账

## R1 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-01 | R1 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |

## R2 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-02 | 复发：origin R1；last_seen R2 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |
"""
        metrics = """# 指标

## R1

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min

## R2

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 0
- certify 新准入: 0
- 重复合并: 1
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-02\n- 位置：§1\n- 问题：示例\n",
            encoding="utf-8",
        )
        good = check_run_dir(run_dir, panel_dir, "R2")
        if good.failed:
            print("self-test FAIL: 同一根因跨轮复发被误判为新准入")
            print_report(good, False)
            return 1

    # V3：B 类行只写“五门 PASS” → 行级证据与冷读双 FAIL
    evidence_report = Report()
    check_v3_evidence(
        {
            "READER-A-01": {
                "class": "④",
                "action": "修",
                "verify": "doc 五门 PASS",
                "role": "reader-a",
                "section": "R1 面板轮",
                "evidence": "§1 读者卡住",
            }
        },
        evidence_report,
        "R1",
        1,
    )
    evidence_messages = " ".join(item["message"] for item in evidence_report.items)
    if "行级证据" not in evidence_messages or "B 类" not in evidence_messages:
        print("self-test FAIL: V3 B 类行只写五门 PASS 未被拦截")
        print_report(evidence_report, False)
        return 1

    # V3：同一处置/复验文本 >3 次 → 模板串 FAIL
    template_report = Report()
    check_v3_evidence(
        {
            f"FACT-0{i}": {
                "class": "①",
                "action": "按面板最小修复落地",
                "verify": "doc 五门 PASS；见 loop §5",
                "role": "P-FACT",
                "section": "R1 面板轮",
                "evidence": "",
            }
            for i in range(5)
        },
        template_report,
        "R1",
        1,
    )
    template_messages = " ".join(item["message"] for item in template_report.items)
    if "疑似模板" not in template_messages:
        print("self-test FAIL: V3 模板串重复未被拦截")
        print_report(template_report, False)
        return 1

    # todo 台账：`允许开放` 是非法第七桶，必须被拦截
    with tempfile.TemporaryDirectory() as tmp:
        todo = Path(tmp) / "todo.md"
        todo.write_text(
            "# todo\n\n"
            "| 条目 | 一句话 | 类型 | 当前状态 | 去向 |\n"
            "|---|---|---|---|---|\n"
            "| I-30 | 新登记教学任务 | 文档教学 | 允许开放（刚登记） | §5 |\n"
            "| D-1 | 已完成项 | 代码 | ✅ 已实施 | §2 |\n",
            encoding="utf-8",
        )
        todo_report = Report()
        check_todo_ledger(todo, todo_report)
        todo_messages = " ".join(item["message"] for item in todo_report.items)
        if "非法状态" not in todo_messages:
            print("self-test FAIL: todo 台账的 `允许开放` 未被拦截")
            print_report(todo_report, False)
            return 1

    # V3.3：OPEN-DEBT 未销账行 → FAIL 并计入未闭环；用户批准延期 → WARN 放行。
    # V3.6-r2 修约：`✅ 已修：已完成` 这种**裸状态声称**（无代码证据/测试读数/
    # commit/path:line）不再算销账——§4.0f 的出口是「实施（附代码证据或 dup-of）」。
    # 因此本夹具现在期望 open_count == 2（R26-design-batch + F-R17-02）。
    with tempfile.TemporaryDirectory() as tmp:
        debt = Path(tmp) / "OPEN-DEBT.md"
        debt.write_text(
            "# open-debt\n\n"
            "| ID | 严重度 | 处置/档期 |\n"
            "|---|---|---|\n"
            "| R26-design-batch | P1 | 待实施（R27）；截止 R27 边界 |\n"
            "| F-X-99 | P2 | ② 用户批准 2026-11-15 延期至 02 篇（D-047） |\n"
            "| F-R17-02 | P2 | ✅ 已修：已完成 |\n"
            "| F-R17-03 | P2 | ✅ 已修：`os/kernel/src/lib.rs:88` 的 `PhaseCell` 已落地；测试 `cel"
            "l::phase` 通过 |\n",
            encoding="utf-8",
        )
        debt_report = Report()
        open_count = check_open_debt(debt, debt_report, "R31")
        debt_messages = " ".join(item["message"] for item in debt_report.items)
        if open_count != 2 or "未销账" not in debt_messages or "已逾期" not in debt_messages:
            print(
                "self-test FAIL: OPEN-DEBT 未销账行未被拦截或逾期未点名"
                f"（open={open_count}，期望 2：裸状态词不计销账）"
            )
            print_report(debt_report, False)
            return 1
        if "已批准延期" not in debt_messages:
            print("self-test FAIL: 用户批准的延期出口未被识别")
            print_report(debt_report, False)
            return 1
        if "F-R17-03" in debt_messages:
            print("self-test FAIL: 带证据的完成行被误判未销账（F-R17-03 应销账）")
            print_report(debt_report, False)
            return 1

    # V3.3：读者/设计 P3 按 ①/④/⑥ 准入 → FAIL；severity 与面板不一致 → FAIL；
    # 设计 ⑥ 未分类 → FAIL；[ARCH] 无判决行 → FAIL
    admission_report = Report()
    check_v33_admission(
        {
            "READER-A-01": {
                "class": "④",
                "severity": "P3",
                "role": "reader-a",
                "section": "R31 面板轮",
            },
            "DESIGN-D2-01": {
                "class": "⑥",
                "severity": "P3",
                "role": "design-d2",
                "section": "R31 面板轮",
                "action": "实施 [MODERN] BootStorage 内部提交断言（外部行为不变）",
                "evidence": "cargo test",
            },
            "DESIGN-D2-02": {
                "class": "⑥",
                "severity": "P2",
                "role": "design-d2",
                "section": "R31 面板轮",
                "action": "实施某项改进",
                "evidence": "cargo test",
            },
            "DESIGN-D2-03": {
                "class": "⑥",
                "severity": "P1",
                "role": "design-d2",
                "section": "R31 面板轮",
                "action": "实施 [ARCH] 借槽语义改 directMap",
                "evidence": "cargo test",
            },
            "READER-A-02": {
                "class": "SUGG",
                "severity": "P3",
                "role": "reader-a",
                "section": "R31 面板轮",
            },
            "FACT-B-02": {
                "class": "SUGG",
                "severity": "P2",
                "role": "P-FACT-B",
                "section": "R31 面板轮",
            },
            "FACT-B-01": {
                "class": "①",
                "severity": "P1",
                "role": "P-FACT-B",
                "section": "R31 面板轮",
            },
        },
        {(31, "FACT-B-01"): "P0"},
        admission_report,
        "R31",
        31,
    )
    admission_messages = " ".join(item["message"] for item in admission_report.items)
    if (
        "suggestions" not in admission_messages
        or "面板标题" not in admission_messages
        or "design-d2" not in admission_messages
    ):
        print("self-test FAIL: V3.3 读者/设计 P3 准入 / severity 同步未被拦截")
        print_report(admission_report, False)
        return 1
    class_messages = " ".join(
        item["message"]
        for item in admission_report.items
        if item["check"] == "design-class"
    )
    if "DESIGN-D2-02" not in class_messages or "未分类" not in class_messages:
        print("self-test FAIL: 设计 ⑥ 未分类未被拦截")
        print_report(admission_report, False)
        return 1
    if "DESIGN-D2-03" in class_messages and "判决行" not in class_messages:
        print("self-test FAIL: [ARCH] 无判决行未被拦截")
        print_report(admission_report, False)
        return 1
    if "DESIGN-D2-01" in class_messages:
        print("self-test FAIL: 已分类的设计行被误判")
        print_report(admission_report, False)
        return 1
    sugg_escape = any(
        item["check"] == "admission-v33" and "FACT-B-02" in item["message"]
        for item in admission_report.items
    )
    if not sugg_escape:
        print("self-test FAIL: P2 借 SUGG 逃逸分流未被拦截")
        print_report(admission_report, False)
        return 1
    reader_sugg_ok = not any(
        item["check"] == "admission-v33" and "READER-A-02" in item["message"]
        for item in admission_report.items
    )
    if not reader_sugg_ok:
        print("self-test FAIL: 合法 SUGG 分流被误判")
        print_report(admission_report, False)
        return 1

    # V3.3：修改轮自判 CONVERGED → FAIL（收敛声明权只在确认轮）
    lifecycle_loop = (
        "## R31 完整 survey\n\n### R31 §7 状态\n\n`CERTIFY PASS / CONVERGED（按代理口径 A）`\n"
    )
    lifecycle_metrics = """# 指标

## R31 完整 survey

- 发现总数: 1
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- certify 新准入: 0
- 轮型: full
- 轮结果: CONVERGED
- edition: V3.1
"""
    lifecycle_report = Report()
    check_v33_lifecycle(
        lifecycle_metrics,
        {},
        {},
        lifecycle_report,
        26,
        31,
    )
    lifecycle_messages = " ".join(item["message"] for item in lifecycle_report.items)
    if "收敛声明权" not in lifecycle_messages:
        print("self-test FAIL: 修改轮自判 CONVERGED 未被拦截")
        print_report(lifecycle_report, False)
        return 1

    # V3.3：确认轮通过（新准入=0）合法；确认轮实施 ⑥ → FAIL
    # （r6 R6-a：确认轮前置须有已关闭修改轮——按文档计，故补一个 R30 收口轮）
    _closed_mod = (
        "## R30 修改轮\n\n- 发现总数: 1\n- 未闭环: 0\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n\n"
    )
    confirm_metrics = lifecycle_metrics.replace(
        "- 轮型: full", "- 轮型: confirmation"
    ).replace("# 指标\n\n", "# 指标\n\n" + _closed_mod, 1)
    confirm_ledger_rows = {
        "RD-R31-01": {
            "class": "⑥",
            "action": "实施",
            "verify": "cargo test",
            "root": "RD-R31-01",
            "origin": "R31",
            "severity": "P2",
            "role": "design-d2",
            "section": "R31 完整 survey",
        }
    }
    confirm_sections = {
        "R31 完整 survey": {"RD-R31-01": "⑥"},
        "R30 修改轮": {"RD-30": "①"},
    }
    _closed_rows = {
        "RD-30": {"class": "①", "action": "修", "verify": "grep x", "root": "RD-30",
                  "origin": "R30", "severity": "P2", "role": "fact",
                  "section": "R30 修改轮"},
    }
    confirm_report = Report()
    check_v33_lifecycle(
        confirm_metrics,
        {**confirm_ledger_rows, **_closed_rows},
        confirm_sections,
        confirm_report,
        26,
        31,
    )
    confirm_messages = " ".join(item["message"] for item in confirm_report.items)
    if "确认轮实施 ⑥" not in confirm_messages:
        print("self-test FAIL: 确认轮实施 ⑥ 未被拦截")
        print_report(confirm_report, False)
        return 1

    # V3.3：确认轮计数按 CANDIDATE（edition）重置——跨 edition 的第 3 次确认
    # 不再触发"最多 2 次"；CONVERGED 后的新 edition（批准重开）也不再 FAIL
    cross_edition_metrics = (
        lifecycle_metrics.replace(
            "- 轮型: full", "- 轮型: confirmation"
        ).replace("# 指标\n\n", "# 指标\n\n" + _closed_mod, 1)
        + "\n## R32 确认轮\n\n"
        + "- 发现总数: 0\n"
        + "- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0\n"
        + "- 未闭环: 0\n"
        + "- certify 新准入: 0\n"
        + "- 轮型: confirmation\n"
        + "- 轮结果: CONVERGED\n"
        + "- edition: V3.3\n"
    )
    cross_report = Report()
    cross_sections = {"R30 修改轮": {"RD-30": "①"}}
    check_v33_lifecycle(
        cross_edition_metrics,
        _closed_rows,
        cross_sections,
        cross_report,
        26,
        31,
    )
    cross_messages = " ".join(item["message"] for item in cross_report.items)
    if cross_report.failed:
        print("self-test FAIL: 跨 edition 的确认轮被误判（计数应按 CANDIDATE 重置）")
        print_report(cross_report, False)
        return 1
    if "lifecycle-v33" in cross_messages:
        print("self-test FAIL: 跨 edition 确认轮误报 lifecycle")
        return 1

    # ===== V3.4 最小内核夹具（r6 重实施） =====
    # R6-a：确认轮前置修改轮。
    _r6a_bad = (
        "# 指标\n\n## R1 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- certify 新准入: 0\n"
    )
    rep_r6a = Report()
    check_v33_lifecycle(_r6a_bad, {}, {}, rep_r6a, 1, 1)
    if not any("R6-a" in i["message"] for i in rep_r6a.items if i["level"] == "FAIL"):
        print("self-test FAIL: 无前置修改轮的确认轮 CONVERGED 未被拦截")
        return 1
    _r6a_good = (
        "# 指标\n\n## R1 修改轮\n\n- 发现总数: 1\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 未闭环: 0\n\n"
        "## R2 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n- certify 新准入: 0\n"
    )
    rep_r6a2 = Report()
    _r6a_rows = {
        "RD-1": {"class": "①", "action": "修", "verify": "grep x", "root": "RD-1",
                 "origin": "R1", "severity": "P2", "role": "fact",
                 "section": "R1 修改轮"},
    }
    _r6a_sections = {"R1 修改轮": {"RD-1": "①"}}
    check_v33_lifecycle(_r6a_good, _r6a_rows, _r6a_sections, rep_r6a2, 1, 1)
    if any("R6-a" in i["message"] for i in rep_r6a2.items if i["level"] == "FAIL"):
        print("self-test FAIL: 有前置修改轮的确认轮被误拦")
        return 1

    # P1-7：certify 待跑 × 闭合态（限 schema V3.4）。
    _p17_metrics = (
        "## R1 确认轮\n"
        "- 发现总数: 1\n- 唯一根因数: 1\n- 本轮新准入: 1\n- certify 新准入: 0\n"
        "- 重复合并: 0\n- 自引入: 0\n"
        "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
        "- 轮型: confirmation\n- 轮结果: CONVERGED\n- 代裁决: 0\n- ARCH 提案: 0\n"
        "- 用户否决: 0\n- edition: V3.4\n- re-litigation: 0\n- adjacent 阻塞: 0\n"
        "- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0\n- 成本: agent 1 / token 1 / 耗时 1min\n"
        "- 回读与回归: certify 待跑\n"
    )
    rep_p17 = Report()
    check_metrics(_p17_metrics, rep_p17, "R1", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=True)
    if not any("P1-7" in i["message"] for i in rep_p17.items if i["level"] == "FAIL"):
        print("self-test FAIL: V3.4 下「certify 待跑」并存未被拦截")
        return 1
    rep_p17b = Report()
    check_metrics(_p17_metrics, rep_p17b, "R1", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=False)
    if any("P1-7" in i["message"] for i in rep_p17b.items if i["level"] == "FAIL"):
        print("self-test FAIL: V3.3 schema 被 P1-7 追溯判红")
        return 1

    # N0：双源断链 + --round 作用域。
    _loop_n0 = (
        "schema: V3.4 from R1\n\nedition: V3.4\n\n"
        "## R1 面板轮\n\n### §7 状态\nCONVERGED\n\n"
        "## R2 面板轮\n\n### §7 状态\nCONVERGED\n\n"
        "## R3 面板轮\n\n### §7 状态\nCONVERGED\n"
    )
    _metrics_n0 = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n\n"
        "## R3 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n"
    )
    rep_n0 = Report()
    check_round_monotonicity(_loop_n0, {}, {}, rep_n0, schema_start=1, v3=True,
                             metrics_text=_metrics_n0)
    if not any("N0" in i["message"] for i in rep_n0.items if i["level"] == "FAIL"):
        print("self-test FAIL: 无台账无 metrics 的未闭合轮越序未被拦截")
        return 1
    rep_scope = Report()
    check_round_monotonicity(_loop_n0, {}, {}, rep_scope, schema_start=1, v3=True,
                             metrics_text=_metrics_n0, round_tag="R1")
    if any("N0" in i["message"] for i in rep_scope.items if i["level"] == "FAIL"):
        print("self-test FAIL: --round R1 追溯判了 R2 断链（作用域过宽）")
        return 1
    rep_scope2 = Report()
    check_round_monotonicity(_loop_n0, {}, {}, rep_scope2, schema_start=1, v3=True,
                             metrics_text=_metrics_n0, round_tag="R3")
    if not any("N0" in i["message"] for i in rep_scope2.items if i["level"] == "FAIL"):
        print("self-test FAIL: --round R3 未抓到 R2 断链")
        return 1

    # r7：R6-a 实质要件——无发现总数键的两行伪造不得计入 mod_closed。
    _forge = (
        "# 指标\n\n## R1 修改轮\n\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n\n"
        "## R2 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- certify 新准入: 0\n"
    )
    _fr = Report()
    check_v33_lifecycle(_forge, {}, {}, _fr, 1, 1)
    if not any("R6-a" in i["message"] for i in _fr.items if i["level"] == "FAIL"):
        print("self-test FAIL: 无发现总数键的伪造修改轮仍计入 mod_closed")
        return 1

    # r7：N0 读 metrics 未闭环值——自报 open 的轮不得静默越序。
    _mo_loop = (
        "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n### §7 状态\nNOT CONVERGED\n\n"
        "## R2 面板轮\n\n### §7 状态\nCONVERGED\n"
    )
    _mo_metrics = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n- 未闭环: 1\n\n"
        "## R2 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n"
    )
    _mr = Report()
    check_round_monotonicity(_mo_loop, {}, {}, _mr, schema_start=1, v3=True,
                             metrics_text=_mo_metrics)
    if not any("N0" in i["message"] for i in _mr.items if i["level"] == "FAIL"):
        print("self-test FAIL: metrics 自报未闭环>0 的轮静默越序")
        return 1

    # r7：前缀串扰——词边界助手双向判定。
    if _round_key_matches("R10 面板轮", "R1"):
        print("self-test FAIL: R1 仍匹配 R10（词边界失效）")
        return 1
    if not _round_key_matches("R1 面板轮", "R1"):
        print("self-test FAIL: R1 不再匹配自身")
        return 1
    if _round_key_matches("R12 面板轮", "R1"):
        print("self-test FAIL: R1 仍匹配 R12")
        return 1

    # ===== V3.4-r8 判别夹具（删除对应新逻辑必须变红）=====
    # (1) mod_closed 台账要件：有发现总数但无台账节的伪造轮不得计入。
    _forge3 = (
        "# 指标\n\n## R1 修改轮\n\n- 发现总数: 0\n- 轮型: full\n"
        "- 轮结果: ROUND-CLOSED\n- 未闭环: 0\n\n"
        "## R2 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- certify 新准入: 0\n"
    )
    _f3 = Report()
    check_v33_lifecycle(_forge3, {}, {}, _f3, 1, 1)
    if not any("R6-a" in i["message"] for i in _f3.items if i["level"] == "FAIL"):
        print("self-test FAIL: 有发现总数但无台账节的伪造轮仍计入 mod_closed")
        return 1

    # (2) N0 缺 `未闭环` 键按开放计。
    _nk_loop = (
        "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n### §7 状态\nNOT CONVERGED\n\n"
        "## R2 面板轮\n\n### §7 状态\nCONVERGED\n"
    )
    _nk_metrics = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n\n"
        "## R2 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n"
    )
    _nkr = Report()
    check_round_monotonicity(_nk_loop, {}, {}, _nkr, schema_start=1, v3=True,
                             metrics_text=_nk_metrics)
    if not any("N0" in i["message"] for i in _nkr.items if i["level"] == "FAIL"):
        print("self-test FAIL: metrics 缺未闭环键的轮未按开放计")
        return 1

    # (3) P1-7 词表扩充生效。
    _p17b = (
        "## R1 确认轮\n"
        "- 发现总数: 1\n- 唯一根因数: 1\n- 本轮新准入: 1\n- certify 新准入: 0\n"
        "- 重复合并: 0\n- 自引入: 0\n"
        "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
        "- 轮型: confirmation\n- 轮结果: CONVERGED\n- 代裁决: 0\n- ARCH 提案: 0\n"
        "- 用户否决: 0\n- edition: V3.4\n- re-litigation: 0\n- adjacent 阻塞: 0\n"
        "- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0\n- 成本: agent 1\n"
        "- 回读与回归: certify 待复跑\n"
    )
    _p17r = Report()
    check_metrics(_p17b, _p17r, "R1", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=True)
    if not any("P1-7" in i["message"] for i in _p17r.items if i["level"] == "FAIL"):
        print("self-test FAIL: P1-7 词表扩充未生效（certify 待复跑）")
        return 1

    # (4) strict 不回退：块标题写成一/三级 `# R2` 时 --round 必须 FAIL。
    _strictr = Report()
    check_metrics("# 指标\n\n# R2 面板轮\n\n- 发现总数: 0\n", _strictr, "R2",
                  None, 0, 1, v3=True, root_count=None, new_roots=None,
                  v33_start=1, v34=True)
    if not _strictr.failed:
        print("self-test FAIL: 轮块标题旁路（# R2）未被 strict 模式拦住")
        return 1

    # (5) 零/空面板关轮必须 FAIL。
    with tempfile.TemporaryDirectory() as t8:
        r8d = Path(t8) / "run"
        r8d.mkdir()
        (r8d / "loop.md").write_text(
            "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (r8d / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (r8d / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        _pr = check_run_dir(r8d, None, "R1")
        if not any("面板证据" in i["message"] for i in _pr.items if i["level"] == "FAIL"):
            print("self-test FAIL: 零面板关轮未被拦截")
            return 1

    # (6) ARCH ⑤ 行：批准延期（用户批准+日期+D 行）不触发 BLOCKED；未批准触发。
    _arch_metrics = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
        "- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 1\n"
        "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
        "- 用户否决: 0\n- edition: V3.4\n"
    )
    _arch_secs = {"R1 面板轮": {"F-9": "⑤"}}
    _arch_pending = {"F-9": {"class": "⑤", "action": "[ARCH] 待用户裁决",
                             "verify": "-", "root": "F-9", "origin": "R1",
                             "severity": "P1", "role": "design-d2",
                             "section": "R1 面板轮"}}
    _ar = Report()
    check_v33_lifecycle(_arch_metrics, _arch_pending, _arch_secs, _ar, 1, 1)
    if not any("ARCH" in i["message"] for i in _ar.items if i["level"] == "FAIL"):
        print("self-test FAIL: 未批准的 ARCH ⑤ 行未触发 BLOCKED")
        return 1
    _arch_ok = {"F-9": {"class": "⑤", "action": "[ARCH] 待用户裁决；用户批准 2026-11-18；D-056",
                        "verify": "-", "root": "F-9", "origin": "R1",
                        "severity": "P1", "role": "design-d2",
                        "section": "R1 面板轮"}}
    _ar2 = Report()
    check_v33_lifecycle(_arch_metrics, _arch_ok, _arch_secs, _ar2, 1, 1)
    if any("ARCH" in i["message"] for i in _ar2.items if i["level"] == "FAIL"):
        print("self-test FAIL: 已批准延期的 ARCH 行仍被硬阻断")
        return 1

    # ===== V3.5 制品链夹具 =====
    import hashlib as _hl

    def _write_artifact_run(root: Path) -> Path:
        d = root / "run"
        d.mkdir()
        (d / "loop.md").write_text(
            "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (d / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (d / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        return d

    with tempfile.TemporaryDirectory() as tv:
        rv = _write_artifact_run(Path(tv))
        rp = check_run_dir(rv, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rp.items if i["level"] == "FAIL"):
            print("self-test FAIL: V3.5 面板制品缺失未拦截")
            return 1
    with tempfile.TemporaryDirectory() as tv:
        rv = _write_artifact_run(Path(tv))
        pd = rv / "panel" / "round1"
        pd.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引（panel-index，生成物——勿手改）\n\n- 文件数: 1\n\n"
            "| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        rp = check_run_dir(rv, None, "R1")
        msgs = [i["message"] for i in rp.items if i["level"] == "FAIL"]
        if any("面板制品未入库" in x or "INDEX.md" in x for x in msgs):
            print("self-test FAIL: 合法制品链被误拦")
            return 1
        if not any("certify 独立回执未入库" in x for x in msgs):
            print("self-test FAIL: 闭合轮 certify 回执缺失未拦截")
            return 1
        cd = rv / "certify"
        cd.mkdir()
        (cd / "round1-1.md").write_text(
            "# 独立冷读回执\n\n逐条回读 R1 全部修复 hunk；结论：CLOSED。\n" * 2,
            encoding="utf-8")
        rp2 = check_run_dir(rv, None, "R1")
        if any("certify 独立回执未入库" in i["message"] for i in rp2.items if i["level"] == "FAIL"):
            print("self-test FAIL: 已入库的 certify 回执仍被拦")
            return 1

    # ===== r9 考场判别夹具 =====
    # (a) strict：完整合法 metrics 写成单级 `# R2` 标题 → 必须 FAIL。
    _strict_complete = (
        "# 指标\n\n# R2 确认轮\n\n- 发现总数: 0\n- 唯一根因数: 0\n"
        "- 本轮新准入: 0\n- certify 新准入: 0\n- 重复合并: 0\n- 自引入: 0\n"
        "- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
        "- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- 代裁决: 0\n- ARCH 提案: 0\n- 用户否决: 0\n- edition: V3.4\n"
        "- re-litigation: 0\n- adjacent 阻塞: 0\n- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0\n"
        "- 成本: agent 1\n"
    )
    _scr = Report()
    check_metrics(_strict_complete, _scr, "R2", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=True)
    if not _scr.failed:
        print("self-test FAIL: strict 未拦住单级标题的完整块")
        return 1

    # (b) 空面板目录（存在但为空）→ 必须 FAIL。
    with tempfile.TemporaryDirectory() as te:
        re_ = _write_artifact_run(Path(te))
        (re_ / "panel" / "round1").mkdir(parents=True)
        rpe = check_run_dir(re_, None, "R1")
        if not any("目录为空" in i["message"] for i in rpe.items if i["level"] == "FAIL"):
            print("self-test FAIL: 空面板目录未被拦截")
            return 1

    # (c) INDEX 计数漂移 → 必须 FAIL。
    with tempfile.TemporaryDirectory() as tc:
        rc = _write_artifact_run(Path(tc))
        pd = rc / "panel" / "round1"
        pd.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 2 | 0 | 0 | finding |\n", encoding="utf-8")
        rpc = check_run_dir(rc, None, "R1")
        if not any("计数与 INDEX 不一致" in i["message"] for i in rpc.items if i["level"] == "FAIL"):
            print("self-test FAIL: INDEX 计数漂移未被拦截")
            return 1

    # (d) 缺 INDEX → 必须 FAIL。
    with tempfile.TemporaryDirectory() as td:
        rd = _write_artifact_run(Path(td))
        pd = rd / "panel" / "round1"
        pd.mkdir(parents=True)
        (pd / "fact.md").write_text("## FINDING F-1\n", encoding="utf-8")
        rpd = check_run_dir(rd, None, "R1")
        if not any("INDEX.md 缺失" in i["message"] for i in rpd.items if i["level"] == "FAIL"):
            print("self-test FAIL: 缺 INDEX.md 未被拦截")
            return 1

    # (e) certify 回执含否定结论 → 必须 FAIL。
    with tempfile.TemporaryDirectory() as tf:
        rf = _write_artifact_run(Path(tf))
        pd = rf / "panel" / "round1"
        pd.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cd = rf / "certify"
        cd.mkdir()
        (cd / "round1-1.md").write_text(
            "# 独立冷读回执\n\n逐条回读完成。\n\n结论：NOT PASS——REOPEN 两处未落地。\n" * 2,
            encoding="utf-8")
        rpf = check_run_dir(rf, None, "R1")
        if not any("否定性结论" in i["message"] for i in rpf.items if i["level"] == "FAIL"):
            print("self-test FAIL: 含否定结论的回执未被拦截")
            return 1

    # (f) 空 FREEZE 不解锁制品豁免（V3.3 未冻结文档）。
    with tempfile.TemporaryDirectory() as tg:
        rg = Path(tg) / "run"
        rg.mkdir()
        (rg / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rg / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (rg / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n", encoding="utf-8")
        (rg / "FREEZE.md").write_text("", encoding="utf-8")
        rpg = check_run_dir(rg, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rpg.items if i["level"] == "FAIL"):
            print("self-test FAIL: 空 FREEZE 解锁了制品豁免")
            return 1
        (rg / "FREEZE.md").write_text("# 测试 FREEZE —— 夹具（CONVERGED）\n", encoding="utf-8")
        rpg2 = check_run_dir(rg, None, "R1")
        if any("面板制品未入库" in i["message"] for i in rpg2.items if i["level"] == "FAIL"):
            print("self-test FAIL: 有效 FREEZE 未豁免制品链")
            return 1

    # ===== r10 判别夹具（四个 P0）=====
    # (1) SUGG 账目：合规记账（面板 FINDING+SUGG、台账两行、metrics 总数 2）不得双 FAIL。
    with tempfile.TemporaryDirectory() as ts:
        rs = _write_artifact_run(Path(ts))
        pd = rs / "panel" / "round1"
        pd.mkdir(parents=True)
        body = (
            "# fact\n\n## FINDING F-1\n- 位置：§1\n\n"
            "## SUGGESTION RD-S-01\n- 建议：措辞\n"
        )
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 1 | 0 | finding |\n", encoding="utf-8")
        lt = rs / "defect-ledger.md"
        lt.write_text(
            "## R1 面板轮\n\n| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|---|---|\n"
            "| RD-1 | F-1 | R1 | fact | P2 | 事实 | ① | 修 | e | grep x |\n"
            "| RD-2 | RD-S-01 | R1 | fact | P3 | 建议 | SUGG | 登记 | e | 无 |\n",
            encoding="utf-8")
        (rs / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 2\n- 唯一根因数: 2\n"
            "- 本轮新准入: 2\n- certify 新准入: 0\n- 重复合并: 0\n- 自引入: 0\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 1\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        rps = check_run_dir(rs, None, "R1")
        _msgs = [i["message"] for i in rps.items if i["level"] == "FAIL"]
        if any("面板原始发现数" in x for x in _msgs):
            print("self-test FAIL: 合规 SUGG 记账仍触发三账对账 FAIL（r10 P0-①）")
            return 1
        if any("分档 SUGG" in x or "SUGG=0" in x for x in _msgs):
            print("self-test FAIL: 分档聚合仍看不见 SUGG 行（r10 P0-①）")
            return 1

    # (2) 缺 `发现总数` 键必须 FAIL。
    _mt = Report()
    check_metrics(
        "## R1 面板轮\n- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n"
        "- 未闭环: 0\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n"
        "- ARCH 提案: 0\n- 用户否决: 0\n- edition: V3.4\n",
        _mt, "R1", None, 0, 1, v3=True, root_count=None, new_roots=None,
        v33_start=1, v34=True,
    )
    if not any("发现总数" in i["message"] for i in _mt.items if i["level"] == "FAIL"):
        print("self-test FAIL: 缺发现总数键未被判 FAIL（r10 P0-②）")
        return 1

    # (3) 否定措辞的 FREEZE 不解锁豁免。
    with tempfile.TemporaryDirectory() as tn:
        rn = _write_artifact_run(Path(tn))
        # 改成 V3.3 未冻结文档（制品要求来自 V3.3+未FREEZE 分支）
        (rn / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rn / "FREEZE.md").write_text("状态：NOT CONVERGED——尚有 53 项未清点。\n", encoding="utf-8")
        rpn = check_run_dir(rn, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rpn.items if i["level"] == "FAIL"):
            print("self-test FAIL: 否定措辞 FREEZE 仍解锁豁免（r10 P0-③）")
            return 1

    # (4) 纯标点回执与 结论：REOPEN 型回执必须 FAIL。
    with tempfile.TemporaryDirectory() as tp:
        rp4 = _write_artifact_run(Path(tp))
        pd4 = rp4 / "panel" / "round1"
        pd4.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd4 / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd4 / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cd4 = rp4 / "certify"
        cd4.mkdir()
        (cd4 / "round1-1.md").write_text("。" * 50, encoding="utf-8")
        rp4a = check_run_dir(rp4, None, "R1")
        if not any("有效字符不足" in i["message"] for i in rp4a.items if i["level"] == "FAIL"):
            print("self-test FAIL: 纯标点回执未被拦（r10 P0-④）")
            return 1
        (cd4 / "round1-2.md").write_text(
            "# 独立冷读回执\n\n" + "逐条回读完成。" * 5 + "\n\n结论：REOPEN——两处未落地。\n",
            encoding="utf-8")
        rp4b = check_run_dir(rp4, None, "R1")
        if not any("否定性结论" in i["message"] for i in rp4b.items if i["level"] == "FAIL"):
            print("self-test FAIL: 结论 REOPEN 型回执未被拦（r10 P0-④/F-r10-1）")
            return 1

    # (5) 空文件面板 + 外部 --panel-dir（r10 补充判别）。
    with tempfile.TemporaryDirectory() as tq:
        rq = _write_artifact_run(Path(tq))
        pdq = rq / "panel" / "round1"
        pdq.mkdir(parents=True)
        (pdq / "empty.md").write_text("", encoding="utf-8")
        hq = _hl.sha256(b"").hexdigest()
        (pdq / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| empty.md | {hq} | 0 | 0 | 0 | no-finding |\n", encoding="utf-8")
        rpq = check_run_dir(rq, None, "R1")
        if not any("是空文件" in i["message"] for i in rpq.items if i["level"] == "FAIL"):
            print("self-test FAIL: 空文件面板未被拦（r9-③/r10 复核）")
            return 1
    with tempfile.TemporaryDirectory() as tr:
        rr = _write_artifact_run(Path(tr))
        external = Path(tr) / "external-panel"
        external.mkdir()
        (external / "fact.md").write_text("## FINDING F-1\n", encoding="utf-8")
        rpr = check_run_dir(rr, external, "R1")
        if not any("run 目录之外" in i["message"] for i in rpr.items if i["level"] == "FAIL"):
            print("self-test FAIL: 外部 --panel-dir 未被拦（r10 复核）")
            return 1

    # ===== r11 判别夹具 =====
    # (a) FREEZE 肯定式：否定措辞的 FREEZE 不豁免（黑名单绕过四型中抽两型）。
    with tempfile.TemporaryDirectory() as th:
        rh = _write_artifact_run(Path(th))
        (rh / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rh / "FREEZE.md").write_text("还没 CONVERGED。\n", encoding="utf-8")
        rph = check_run_dir(rh, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rph.items if i["level"] == "FAIL"):
            print("self-test FAIL: 「还没 CONVERGED」FREEZE 仍解锁（r11 P0-1）")
            return 1
        (rh / "FREEZE.md").write_text("目标：达成 CONVERGED 后冻结。\n", encoding="utf-8")
        rph2 = check_run_dir(rh, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rph2.items if i["level"] == "FAIL"):
            print("self-test FAIL: 「目标达成 CONVERGED」FREEZE 仍解锁（r11 P0-1）")
            return 1

    # (b) SUGG 形状契约：### 层级带 ID 的 SUGGESTION 被三处同口径收集。
    with tempfile.TemporaryDirectory() as tj:
        rj = _write_artifact_run(Path(tj))
        pd = rj / "panel" / "round1"
        pd.mkdir(parents=True)
        body = (
            "# fact\n\n## FINDING F-1\n- 位置：§1\n\n"
            "### SUGGESTION RD-S-02\n- 建议：结构\n\n"
            "## SUGGESTION\n- 无 ID 聚合段：不计入\n"
        )
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 1 | 0 | finding |\n", encoding="utf-8")
        (rj / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|---|---|\n"
            "| RD-1 | F-1 | R1 | fact | P2 | 事实 | ① | 修 | e | grep x |\n"
            "| RD-2 | RD-S-02 | R1 | fact | P3 | 建议 | SUGG | 登记 | e | 无 |\n",
            encoding="utf-8")
        (rj / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 2\n- 唯一根因数: 2\n"
            "- 本轮新准入: 2\n- certify 新准入: 0\n- 重复合并: 0\n- 自引入: 0\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 1\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        rpj = check_run_dir(rj, None, "R1")
        _mj = [i["message"] for i in rpj.items if i["level"] == "FAIL"]
        if any("计数与 INDEX 不一致" in x for x in _mj):
            print("self-test FAIL: ### 层级 SUGG 与 INDEX 计数口径不一致（r11 P0-3）")
            return 1
        if any("没有处置行" in x and "RD-S-02" in x for x in _mj):
            print("self-test FAIL: ### 层级 SUGG 未被覆盖率收集（r11 P0-3）")
            return 1

    # (c) certify 肯定式结论：缺结论行 / 各种否定形态必须 FAIL；显式结论通过。
    with tempfile.TemporaryDirectory() as tk:
        rk = _write_artifact_run(Path(tk))
        pd = rk / "panel" / "round1"
        pd.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cd = rk / "certify"
        cd.mkdir()
        (cd / "round1-1.md").write_text(
            "# 独立冷读回执\n\n逐条回读完成。\n" + "内容详述。" * 8, encoding="utf-8")
        rpk = check_run_dir(rk, None, "R1")
        if not any("缺少显式通过性结论" in i["message"] for i in rpk.items if i["level"] == "FAIL"):
            print("self-test FAIL: 无结论行回执未被拦（r11 P0-2 肯定式）")
            return 1
        (cd / "round1-2.md").write_text(
            "# 回执\n\n" + "内容。" * 10 + "\n\n没通过。\n", encoding="utf-8")
        (cd / "round1-3.md").write_text(
            "# 回执\n\n" + "内容。" * 10 + "\n\nＮＯＴ　ＰＡＳＳ。\n", encoding="utf-8")
        rpk2 = check_run_dir(rk, None, "R1")
        _neg = sum(
            1 for i in rpk2.items if i["level"] == "FAIL" and "否定性结论" in i["message"]
        )
        if _neg < 1:
            print("self-test FAIL: 没通过/全角 NOT PASS 未被拦（r11 P0-2）")
            return 1
        (cd / "round1-2.md").unlink()
        (cd / "round1-3.md").unlink()
        (cd / "round1-4.md").write_text(
            "# 回执\n\n" + "内容。" * 10 + "\n\n结论：通过，全部闭环。\n", encoding="utf-8")
        (cd / "round1-1.md").write_text(
            "# 独立冷读回执\n\n逐条回读完成。\n\n结论：CLOSED。\n" + "详述。" * 6,
            encoding="utf-8")
        rpk3 = check_run_dir(rk, None, "R1")
        if any(
            ("缺少显式通过性结论" in i["message"] or "否定性结论" in i["message"])
            for i in rpk3.items if i["level"] == "FAIL"
        ):
            print("self-test FAIL: 显式结论回执被误拦（r11 P0-2）")
            return 1

    # ===== r12 判别夹具 =====
    # (a) FREEZE 标题位否定后缀三型 → 不豁免。
    with tempfile.TemporaryDirectory() as tu:
        ru = _write_artifact_run(Path(tu))
        (ru / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        for title in (
            "# FREEZE 计划（尚未 CONVERGED）\n",
            "# 01 FREEZE——CONVERGED-CANDIDATE（待确认）\n",
            "# 01 FREEZE——NOT-CONVERGED\n",
        ):
            (ru / "FREEZE.md").write_text(title, encoding="utf-8")
            rpu = check_run_dir(ru, None, "R1")
            if not any(
                "面板制品未入库" in i["message"] for i in rpu.items if i["level"] == "FAIL"
            ):
                print(f"self-test FAIL: 标题位否定变体解锁了豁免（r12 P0-1）: {title!r}")
                return 1

    # (b) certify 结论行极性：未闭环/不可能/疑问句不算通过性结论。
    with tempfile.TemporaryDirectory() as tv2:
        rv2 = _write_artifact_run(Path(tv2))
        pd2 = rv2 / "panel" / "round1"
        pd2.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd2 / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd2 / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cd2 = rv2 / "certify"
        cd2.mkdir()
        (cd2 / "round1-1.md").write_text(
            "# 回执\n\n" + "内容。" * 10 + "\n\n判定：两项未闭环。\n", encoding="utf-8")
        rp2a = check_run_dir(rv2, None, "R1")
        if not any("缺少显式通过性结论" in i["message"] for i in rp2a.items if i["level"] == "FAIL"):
            print("self-test FAIL: 「判定：两项未闭环」被当通过性结论（r12 P0-2）")
            return 1
        import shutil as _sh
        _mk = lambda: (  # noqa: E731
            _write_artifact_run(Path(tempfile.mkdtemp()))
        )
        _r2b = _mk()
        _pd2b = _r2b / "panel" / "round1"
        _pd2b.mkdir(parents=True)
        (_pd2b / "fact.md").write_text(body, encoding="utf-8")
        _h2b = _hl.sha256(body.encode()).hexdigest()
        (_pd2b / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {_h2b} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        (_r2b / "certify").mkdir()
        (_r2b / "certify" / "round1-2.md").write_text(
            "# 回执\n\n" + "内容。" * 10 + "\n\n验证确认了不可能 PASS。\n", encoding="utf-8")
        rp2b = check_run_dir(_r2b, None, "R1")
        if not any("缺少显式通过性结论" in i["message"] for i in rp2b.items if i["level"] == "FAIL"):
            print("self-test FAIL: 「不可能 PASS」被当通过性结论（r12 P0-2，隔离复验）")
            return 1
        _r2c = _mk()
        _pd2c = _r2c / "panel" / "round1"
        _pd2c.mkdir(parents=True)
        (_pd2c / "fact.md").write_text(body, encoding="utf-8")
        (_pd2c / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {_h2b} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        (_r2c / "certify").mkdir()
        (_r2c / "certify" / "round1-3.md").write_text(
            "# 回执\n\n" + "内容。" * 10 + "\n\n结论：应该通过吗？\n", encoding="utf-8")
        rp2c = check_run_dir(_r2c, None, "R1")
        if not any("缺少显式通过性结论" in i["message"] for i in rp2c.items if i["level"] == "FAIL"):
            print("self-test FAIL: 疑问句结论被当通过（r12 P0-2，隔离复验）")
            return 1
        _sh  # noqa

    # (c) ### SUGG 经官方生成器与门同口径。
    import importlib as _imp

    _pi_spec = _imp.util.spec_from_file_location(
        "panel_index", Path(__file__).resolve().parent / "panel-index.py"
    )
    _pi = _imp.util.module_from_spec(_pi_spec)
    _pi_spec.loader.exec_module(_pi)
    with tempfile.TemporaryDirectory() as tw:
        rw = _write_artifact_run(Path(tw))
        pdw = rw / "panel" / "round1"
        pdw.mkdir(parents=True)
        (pdw / "fact.md").write_text(
            "# fact\n\n## FINDING F-1\n- 位置：§1\n\n"
            "### SUGGESTION RD-S-02\n- 建议：结构\n", encoding="utf-8")
        if _pi.write_index(pdw) != 0:
            print("self-test FAIL: 官方生成器失败")
            return 1
        rp2d = check_run_dir(rw, None, "R1")
        _m12 = [i["message"] for i in rp2d.items if i["level"] == "FAIL"]
        if any("计数与 INDEX 不一致" in x for x in _m12):
            print("self-test FAIL: 官方生成器与门计数分叉（r12 F-r12-2）")
            return 1

    # (d) metrics 载体携带 V3.4 from R5：历史层 R2 不要求制品。
    with tempfile.TemporaryDirectory() as tx:
        rx = Path(tx) / "run"
        rx.mkdir()
        (rx / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R2 确认轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rx / "defect-ledger.md").write_text(
            "## R2 确认轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (rx / "metrics.md").write_text(
            "# 指标\n\nschema: V3.4 from R2\n\n## R2 确认轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: confirmation\n- 轮结果: CONVERGED\n- certify 新准入: 0\n"
            "- 代裁决: 0\n- ARCH 提案: 0\n- 用户否决: 0\n- edition: V3.3\n",
            encoding="utf-8")
        (rx / "certify").mkdir()
        (rx / "certify" / "round2-1.md").write_text(
            "# 回执\n\n" + "内容。" * 10 + "\n\n结论：CLOSED。\n", encoding="utf-8")
        # r18 收紧后：from R2 ≤ max_seen 2 → V3.4 从 R2 起约束 → R2 要求制品。
        # 夹具断言反转：标记被正确解析（未入库 → FAIL 证明 V3.4 from R2 生效）。
        rpx = check_run_dir(rx, None, "R2")
        if not any("面板制品未入库" in i["message"] for i in rpx.items if i["level"] == "FAIL"):
            print("self-test FAIL: V3.4 from R2 标记未被解析（r18 收紧后应要求制品）")
            return 1

    # (g) from 轮封顶（r13 P0-5）：from R99 注入被钳回 schema_start。
    with tempfile.TemporaryDirectory() as ty:
        ry = Path(ty) / "run"
        ry.mkdir()
        (ry / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (ry / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (ry / "metrics.md").write_text(
            "# 指标\n\nschema: V3.4 from R99\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n", encoding="utf-8")
        rpy = check_run_dir(ry, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rpy.items if i["level"] == "FAIL"):
            print("self-test FAIL: from R99 注入仍豁免制品链（r13 P0-5）")
            return 1

    # ===== r14 判别夹具 =====
    # (a) from 轮封顶锚点注入（loop 携带 from R99 + 伪造 R99 标题）→ 必须 FAIL。
    with tempfile.TemporaryDirectory() as tz:
        rz = Path(tz) / "run"
        rz.mkdir()
        (rz / "loop.md").write_text(
            "schema: V3.4 from R99\n\nedition: V3.4\n\n## R99 面板轮\n\n"
            "### L6\na\n\n### L6.5\nb\n\n## R1 面板轮\n\n"
            "### §7\nx\n", encoding="utf-8")
        (rz / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (rz / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        rpz = check_run_dir(rz, None, "R1")
        if not any(
            ("面板制品未入库" in i["message"] or "面板证据" in i["message"])
            for i in rpz.items if i["level"] == "FAIL"
        ):
            print("self-test FAIL: from 标记注入（R99 标题+标记）仍豁免制品链（r14 P0-1）")
            return 1

    # (b) FREEZE 词表补全：非/无/曾/将/勿/可能 六型 → 不豁免。
    with tempfile.TemporaryDirectory() as tu2:
        ru2 = _write_artifact_run(Path(tu2))
        (ru2 / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        for title in (
            "# 01 FREEZE——非 CONVERGED\n",
            "# 01 FREEZE——CONVERGED 未达成\n",
        ):
            (ru2 / "FREEZE.md").write_text(title, encoding="utf-8")
            rpu2 = check_run_dir(ru2, None, "R1")
            if not any(
                "面板制品未入库" in i["message"] for i in rpu2.items if i["level"] == "FAIL"
            ):
                print(f"self-test FAIL: 词表缺口标题解锁豁免（r14 P0-2）: {title!r}")
                return 1

    # (c) 毒化逃逸：否定行（无终态）+ 肯定行双 H1 → 不豁免。
    with tempfile.TemporaryDirectory() as tv3:
        rv3 = _write_artifact_run(Path(tv3))
        (rv3 / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rv3 / "FREEZE.md").write_text(
            "# 01 FREEZE——CONVERGED 未达成\n# 01 FREEZE——CONVERGED\n", encoding="utf-8")
        rpv3 = check_run_dir(rv3, None, "R1")
        if not any(
            "面板制品未入库" in i["message"] for i in rpv3.items if i["level"] == "FAIL"
        ):
            print("self-test FAIL: 否定行+肯定行双 H1 毒化逃逸（r14 P0-3）")
            return 1

    # ===== r16 判别夹具（4 P0）=====
    # (a) 全角载体：Ｒ１ 标题的伪造确认轮必须被生命周期看见（R6-a）。
    with tempfile.TemporaryDirectory() as tfw:
        rfw = Path(tfw) / "run"
        rfw.mkdir()
        (rfw / "loop.md").write_text(
            "schema: V3.4 from R1\n\nedition: V3.4\n\n## Ｒ１ 确认轮\n\n"
            "### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rfw / "defect-ledger.md").write_text(
            "## Ｒ１ 确认轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (rfw / "metrics.md").write_text(
            "# 指标\n\n## Ｒ１ 确认轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: confirmation\n- 轮结果: CONVERGED\n- certify 新准入: 0\n"
            "- 代裁决: 0\n- ARCH 提案: 0\n- 用户否决: 0\n- edition: V3.4\n",
            encoding="utf-8")
        rpfw = check_run_dir(rfw, None, "R1")
        if not any("R6-a" in i["message"] for i in rpfw.items if i["level"] == "FAIL"):
            print("self-test FAIL: 全角 Ｒ１ 载体对生命周期隐身（r16 P0-4）")
            return 1

    # (b) metrics 标题提及他轮 ≠ 有账（r16 P0-1）。
    with tempfile.TemporaryDirectory() as tg2:
        rg2 = Path(tg2) / "run"
        rg2.mkdir()
        (rg2 / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rg2 / "defect-ledger.md").write_text(
            "## R2 确认轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (rg2 / "metrics.md").write_text(
            "# 指标\n\n## R2 确认轮（R1 遗留已清）\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n", encoding="utf-8")
        rpg2 = check_run_dir(rg2, None, "R2")
        if not any("N0" in i["message"] for i in rpg2.items if i["level"] == "FAIL"):
            print("self-test FAIL: 标题提及他轮被当作有账（r16 P0-1）")
            return 1

    # (c) FREEZE 词表/左边界：FAIL 解除型 + UN-CONVERGED → 不豁免。
    with tempfile.TemporaryDirectory() as th2:
        rh2 = _write_artifact_run(Path(th2))
        (rh2 / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        for title in (
            "# 01 FREEZE——FAIL 解除后 CONVERGED\n",
            "# 01 FREEZE——UN-CONVERGED\n",
        ):
            (rh2 / "FREEZE.md").write_text(title, encoding="utf-8")
            rph2 = check_run_dir(rh2, None, "R1")
            if not any(
                "面板制品未入库" in i["message"] for i in rph2.items if i["level"] == "FAIL"
            ):
                print(f"self-test FAIL: FREEZE 词表/左边界绕过（r16 P0-2）: {title!r}")
                return 1

    # (d) certify bullet-FAIL 行（r16 P0-3）。
    with tempfile.TemporaryDirectory() as tk2:
        rk2 = _write_artifact_run(Path(tk2))
        pdk = rk2 / "panel" / "round1"
        pdk.mkdir(parents=True)
        (pdk / "fact.md").write_text("## FINDING F-1\n", encoding="utf-8")
        hk = _hl.sha256(b"## FINDING F-1\n").hexdigest()
        (pdk / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {hk} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cdk = rk2 / "certify"
        cdk.mkdir()
        (cdk / "round1-1.md").write_text(
            "# 回执\n\n结论：通过。\n\n- FAIL（两处未收口）\n", encoding="utf-8")
        rpk2 = check_run_dir(rk2, None, "R1")
        if not any("否定性结论" in i["message"] for i in rpk2.items if i["level"] == "FAIL"):
            print("self-test FAIL: bullet-FAIL 行放行（r16 P0-3）")
            return 1

    # ===== r17 判别夹具 =====
    # (h) FREEZE 连字符族（UN–CONVERGED / 0CONVERGED）→ 不豁免。
    with tempfile.TemporaryDirectory() as t20:
        r20 = _write_artifact_run(Path(t20))
        (r20 / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        for title in (
            "# 01 FREEZE——UN\u2013CONVERGED\n",
            "# 01 FREEZE——0CONVERGED\n",
        ):
            (r20 / "FREEZE.md").write_text(title, encoding="utf-8")
            rp20 = check_run_dir(r20, None, "R1")
            if not any(
                "面板制品未入库" in i["message"] for i in rp20.items if i["level"] == "FAIL"
            ):
                print(f"self-test FAIL: 连字符族/前缀数字绕过终态左边界（r17 P0-1）: {title!r}")
                return 1

    # (i) certify 编号列表 FAIL（r17 P0-2）。
    with tempfile.TemporaryDirectory() as t21:
        r21 = _write_artifact_run(Path(t21))
        pd21 = r21 / "panel" / "round1"
        pd21.mkdir(parents=True)
        (pd21 / "fact.md").write_text("## FINDING F-1\n", encoding="utf-8")
        h21 = _hl.sha256(b"## FINDING F-1\n").hexdigest()
        (pd21 / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h21} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cd21 = r21 / "certify"
        cd21.mkdir()
        (cd21 / "round1-1.md").write_text(
            "# 回执\n\n结论：通过。\n\n1. FAIL——两处未收口。\n", encoding="utf-8")
        rp21 = check_run_dir(r21, None, "R1")
        if not any("否定性结论" in i["message"] for i in rp21.items if i["level"] == "FAIL"):
            print("self-test FAIL: 编号列表 FAIL 放行（r17 P0-2）")
            return 1

    # ===== V3.6 C-34 判别夹具（直接测 check_subtraction 单元）=====
    # (j) 减法表 0 行 → FAIL（只列加法的 D2 报告不完整）。
    _d2_none = "# D2\n\n## DIFF-TABLE\n\n| 维度 | 判断 |\n|---|---|\n| a | b |\n\n## VERDICT\nok\n"
    _d2_two = (
        "## SUBTRACTION-TABLE\n\n"
        "| 加法提案 | 减法替代 | 拦不住的理由 | 保障对比 | 新增概念数 |\n"
        "|---|---|---|---|---|\n"
        "| 新类型 A | 闭包 | 闭包不能跨调用保存 | 等效 | 1→0 |\n"
        "| 守卫字段 B | 断言 | 断言在 release 下消失 | 略弱 | +1→0 |\n\n"
        "## VERDICT\nok\n"
    )
    _m = "## R1 面板轮\n\n- 减法: 提案 {s} / 采纳 {a}\n"
    # V3.6-r2 修约：0 行不再无条件 FAIL——区分「诚实零」与「只列加法」。
    _d2_honest_zero = "## SUBTRACTION-TABLE\n\n本轮无新增抽象提案。\n\n## VERDICT\nok\n"
    _d2_add_only = (
        "## FINDING DESIGN-D2-01（P1）\n- 问题：应新增 newtype 承载这个标量\n\n"
        "## VERDICT\nok\n"
    )
    for label, d2, said, adopted, want_fail, want_sub in (
        ("0 行 + 有加法无表", _d2_add_only, 0, 0, "只列加法", None),
        ("0 行 + 显式无提案", _d2_honest_zero, 0, 0, None, "显式声明本轮无新增抽象提案"),
        ("0 行 + 无加法无表", _d2_none, 0, 0, None, "无对象可减"),
        ("0 行声明 1", _d2_none, 1, 0, "不一致", None),
        ("2 行声明 2/1", _d2_two, 2, 1, None, "减法提案 2 / 采纳 1"),
        ("2 行声明 2/3", _d2_two, 2, 3, "采纳数 3 > 提案数 2", None),
        ("2 行声明 1", _d2_two, 1, 0, "不一致", None),
    ):
        rep34 = Report()
        check_subtraction(d2, _m.format(s=said, a=adopted), rep34, "R1")
        if want_fail is not None:
            if not any(i["level"] == "FAIL" and want_fail in i["message"] for i in rep34.items):
                print(f"self-test FAIL: C-34 夹具「{label}」未按预期拦红")
                return 1
        else:
            if any(i["level"] == "FAIL" for i in rep34.items):
                print(f"self-test FAIL: C-34 正控「{label}」在合规样本上变红")
                return 1
            if not any(want_sub in i["message"] for i in rep34.items):
                print(f"self-test FAIL: C-34 正控「{label}」未产生读数")
                return 1
    # (j2) 缺键 → FAIL（V3.6 轮必须有该键）。
    rep34b = Report()
    check_subtraction(_d2_two, "## R1 面板轮\n\n- 发现总数: 1\n", rep34b, "R1")
    if not any("缺少 `- 减法" in i["message"] for i in rep34b.items if i["level"] == "FAIL"):
        print("self-test FAIL: V3.6 轮缺减法键未拦截（C-34）")
        return 1

    # ===== V3.6 b3/b3b/b3c: OPEN-DEBT 可见性判别夹具 =====
    def _debt_rows(text: str, rnd: str = "R3") -> tuple[int, list[str]]:
        rep = Report()
        with tempfile.TemporaryDirectory() as td:
            p = Path(td) / "OPEN-DEBT.md"
            p.write_text(text, encoding="utf-8")
            n = check_open_debt(p, rep, rnd)
        return n, [i["message"] for i in rep.items if i["level"] == "FAIL"]

    # (l) 无表头、直接数据行 → 行必须被看见（原实现静默 0 行）。
    n, fails = _debt_rows("| R3-design-batch | P2 | 待评估（无批准） |\n")
    if n != 1:
        print("self-test FAIL: 无表头 OPEN-DEBT 的行未被看见（V3.6-b3）")
        return 1
    # (m) 有表头 + 已批准延期 → 不判未销账（WARN 级）。
    n, fails = _debt_rows(
        "| ID | 严重度 | 处置/档期 |\n|---|---|---|\n"
        "| R3-batch | P2 | 用户批准 2026-11-20 延期 |\n"
    )
    if n != 0:
        print("self-test FAIL: 已批准延期的 debt 行被判未销账（V3.6-b3 过度收紧）")
        return 1
    # (n) 长散文句尾含「已修」→ 仍判未销账（b3b 判例：词尾不得吃掉欠账）。
    n, fails = _debt_rows(
        "| R1-design-batch | P1 | 四项设计 P1 待实施；文档半边的偏离登记已修 |\n"
    )
    if n != 1:
        print("self-test FAIL: 句中「已修」把整条欠账判成已销账（V3.6-b3b）")
        return 1
    # (o) 格首状态声明 → 已销账，不受长度影响（b3b 的修法不得误杀 D01 形态）。
    n, fails = _debt_rows(
        "| F-R20-01 | P2 | ✅ 已修：`BootPrepareResult.bump: PhysRange`、"
        "`arena_pa/root_page: PhysBytes`；`enter_kernel_image` 同步；附完整证据链与测试读数 |\n"
    )
    if n != 0:
        print("self-test FAIL: 格首 `✅ 已修` 的长处置格被误判未销账（V3.6-b3b 反向）")
        return 1
    # (p) 无 ID 的整行状态叙述（历史合法形态）→ 放行。
    n, fails = _debt_rows("| — | — | F-R18-01 已实施；R14/READER-A-16 为 adjacent，见 follow-ups.md |\n")
    if n != 0:
        print("self-test FAIL: 无 ID 的整行状态叙述被误判未销账（V3.6-b3c）")
        return 1
    # (q) 有表格但无可解析数据行（表头+分隔线、无数据）→ 不 fail-closed
    #     （空表 = 真的没有欠账）；**有数据行却解析不出 ID** 的形态才 fail-closed。
    n, fails = _debt_rows("| ID | 严重度 | 处置/档期 |\n|---|---|---|\n")
    if n != 0 or fails:
        print("self-test FAIL: 空表头的 OPEN-DEBT 被误判（应为 0 行、无 FAIL）")
        return 1
    n, fails = _debt_rows("|  |  |  |\n|---|---|---|\n|  |  |  |\n")
    if not fails or not any("格式无法解析" in f for f in fails):
        print("self-test FAIL: 解析不出 ID 的 OPEN-DEBT 表格未 fail-closed（V3.6-b3）")
        return 1

    # (r) edition-trial：>3 个修改轮 → WARN 超限，不因「未闭环=0」判 PASS（V3.6-b4）。
    _ed = "# 指标\n\n"
    for _i in range(1, 5):
        _ed += (
            f"## R{_i} 完整 survey\n\n"
            f"- 发现总数: 0\n- 唯一根因数: 0\n- 本轮新准入: 0\n- certify 新准入: 0\n"
            f"- 重复合并: 0\n- 自引入: 0\n"
            f"- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n"
            f"- 未闭环: 0\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n- edition: V3.1\n\n"
        )
    _ed_rep = Report()
    check_edition_trial(_ed, _ed_rep, None)
    if not any("超限" in i["message"] for i in _ed_rep.items if i["level"] == "WARN"):
        print("self-test FAIL: 4 个修改轮未报超限（V3.6-b4：超限被算 PASS）")
        return 1
    # 3 轮且未闭环=0 → PASS（不误报）。
    _ed3 = _ed.split("## R4")[0]
    _ed3_rep = Report()
    check_edition_trial(_ed3, _ed3_rep, None)
    if any(i["level"] == "WARN" for i in _ed3_rep.items):
        print("self-test FAIL: 恰好 3 轮且已收口被误报超限（V3.6-b4 反向）")
        return 1

    # (s) 回执遗留计数（V3.6，F4/F10 判例：不再靠散文极性猜）。
    _decl_ok = ["- 遗留 P0: 0 / P1: 0", "- 遗留 P0/P1: 0 / 0"]
    _decl_bad = ["- 遗留 P0: 1 / P1: 2", "- 遗留 P0/P1: 0 / 3"]
    _decl_missing = ["结论：通过（P0 = 0）", "无 P0"]   # 无可判定读数 → fail-closed
    for _s in _decl_ok:
        if _has_lingering_severity(_s, require_decl=True):
            print(f"self-test FAIL: 零遗留声明被判有遗留（V3.6-decl）: {_s!r}")
            return 1
    for _s in _decl_bad:
        if not _has_lingering_severity(_s, require_decl=True):
            print(f"self-test FAIL: 非零遗留未被抓（V3.6-decl）: {_s!r}")
            return 1
    for _s in _decl_missing:
        if not _has_lingering_severity(_s, require_decl=True):
            print(f"self-test FAIL: 无声明未 fail-closed（V3.6-decl）: {_s!r}")
            return 1
    # 旧回执不追溯：无声明时走启发式，六篇已封存回执不得被误伤。
    for _s in ("P0 = 0", "无 P0", "P0 事项 0 条", "P0：无", "P0/P1 无残留"):
        if _has_lingering_severity(_s, require_decl=False):
            print(f"self-test FAIL: 旧回执零化表述被误伤（V3.6-b5）: {_s!r}")
            return 1
    for _s in ("遗留 P0 一项", "P0 共 2 条", "P1 尚有 1"):
        if not _has_lingering_severity(_s, require_decl=False):
            print(f"self-test FAIL: 旧回执遗留表述未被抓（V3.6-b5）: {_s!r}")
            return 1

    # (t) 销账判据（b3b/b3c/F3/F5 四轮收敛后的单一入口）——正负各 5 例。
    for _row, _want_open in (
        ("✅ 已修：附完整证据链与测试读数", 0),
        ("（R23 前置）已修：删单实现 BootShim", 0),
        ("dup-of F-R17-02", 0),
        ("用户批准 2026-11-20 延期至下一 batch", 0),
        ("✅ 已实施（commit e74db99e4）", 0),
        ("✅ 已登记，待实施（未修）", 1),
        ("✅ 已修，但 R34 复验 FAIL", 1),
        ("完成度 40%，未实施", 1),
        ("待实施；同族项已在 R32 已修", 1),
        ("无批准，待评估", 1),
    ):
        _with_id, _no_id = True, _want_open
        for _pfx in ("| X-1 | P1 | ", "| — | P2 | "):
            _rep_t = Report()
            with tempfile.TemporaryDirectory() as _td:
                _pp = Path(_td) / "OPEN-DEBT.md"
                _pp.write_text(
                    "| ID | 严重度 | 处置 |\n|---|---|---|\n" + _pfx + _row + " |\n",
                    encoding="utf-8",
                )
                _n_t = check_open_debt(_pp, _rep_t, "R3")
            if _n_t != _want_open:
                print(
                    f"self-test FAIL: 销账判据错（V3.6-F3/F5）: {_pfx}{_row!r} "
                    f"open={_n_t} want={_want_open}"
                )
                return 1

    # (u) 减法表占位行不得计为提案（F1：诚实报告 FAIL、编造 PASS 是本末倒置）。
    _d2_placeholder = (
        "## SUBTRACTION-TABLE\n\n"
        "| 加法提案 | 减法替代 | 拦不住的理由 | 保障对比 | 新增概念数 |\n"
        "|---|---|---|---|---|\n"
        "| 无 | — | — | — | — |\n"
        "| 无 | — | — | — | — |\n\n## VERDICT\nok\n"
    )
    if _count_subtraction_rows(_d2_placeholder) != 0:
        print("self-test FAIL: 减法表占位行被计为提案（V3.6-F1）")
        return 1
    # 围栏内的「格式示例」表不得计为提案（F6）。
    _d2_fenced = (
        "## SUBTRACTION-TABLE\n\n本轮无减法提案。\n\n```markdown\n"
        "| 加法提案 | 减法替代 |\n|---|---|\n| A | 闭包 |\n```\n\n## VERDICT\nok\n"
    )
    if _count_subtraction_rows(_d2_fenced) != 0:
        print("self-test FAIL: 围栏内示例表被计为提案（V3.6-F6）")
        return 1
    # `###` 子节必须终止本段（F6）。
    _d2_sub = (
        "## SUBTRACTION-TABLE\n\n| A | 闭包 |\n\n### 备注\n\n| B | 断言 |\n"
    )
    if _count_subtraction_rows(_d2_sub) != 1:
        print("self-test FAIL: `###` 子节未终止减法段（V3.6-F6）")
        return 1

    # (v) 无发现面板的两种合法声明（V3.6-r2-F4：规则第 9 条 vs 门的一致性）。
    if not _panel_empty_ok("fact.md", "# FACT\n\n无发现。\n", None):
        print("self-test FAIL: 面板本体「无发现」声明未被认（V3.6-r2-F4）")
        return 1
    if not _panel_empty_ok(
        "design-d1.md",
        "# D1\n\n## DECISION 01\n说明\n",
        "- 无发现面板：`design-d1.md`（独立设计提案，只作参考）\n",
    ):
        print("self-test FAIL: loop §1 具名「无发现面板」声明未被认（V3.6-r2-F4）")
        return 1
    if _panel_empty_ok(
        "design-d1.md",
        "# D1\n\n## DECISION 01\n说明\n",
        "- 无发现面板：`refute.md`（点名的是另一个文件）\n",
    ):
        print("self-test FAIL: loop §1 声明点名了别的文件却也豁免（V3.6-r2-F4）")
        return 1
    if _panel_empty_ok("fact.md", "# FACT\n\n## DECISION 01\n", None):
        print("self-test FAIL: 真空壳面板（无声明无发现）被放行（V3.6-r2-F4）")
        return 1

    print("self-test PASS: 正确样本通过，缺陷样本被拦截")
    return 0

def main() -> int:
    parser = argparse.ArgumentParser(
        description="review-loop 轨迹完整性检查",
    )
    parser.add_argument("run_dir", nargs="?", help="run 目录")
    parser.add_argument(
        "--panel-dir",
        help="面板原始输出目录（默认 <run-dir>/panel；规范布局在 tmp/review-eval/ 下）",
    )
    parser.add_argument(
        "--round",
        dest="round_tag",
        metavar="R25",
        help="只检查指定轮次（覆盖范围、收口、未闭环、round-order）；轮次关账用这个",
    )
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument(
        "--audit-templates",
        action="store_true",
        help="审计历史轮次的处置/复验模板串重复（R24/R25 的隐式带病）",
    )
    parser.add_argument(
        "--todo-file",
        help="同时核对该文档的 todo 台账（如 rewrite-notes/01-stage-kernel/01-todo.md）",
    )
    parser.add_argument("--self-test", action="store_true", help="运行内置自测")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.run_dir:
        parser.print_help()
        return 2

    panel_dir = Path(args.panel_dir) if args.panel_dir else None
    report = check_run_dir(
        Path(args.run_dir), panel_dir, args.round_tag, args.audit_templates
    )
    if args.todo_file:
        check_todo_ledger(Path(args.todo_file), report)
    print_report(report, args.json)
    return 1 if report.failed else 0


if __name__ == "__main__":
    sys.exit(main())
