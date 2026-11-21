#!/usr/bin/env python3
"""todo-migrate.py — 旧 TODO 文档 → YAML 账本的 dry-run 提取器（V3.8-r2）

只读：不写 YAML，只生成 `manifest.tsv`（source:line → 原 ID/状态/拟定状态）。
真正的状态判定在核实阶段（子 agent + 主 agent），本工具只做**候选抽取与分类**。

用法：
  python3 tools/todo-migrate.py --all --out tmp/todo-migration/manifest.tsv
  python3 tools/todo-migrate.py --file <path> --stage S --doc D --out ...
  python3 tools/todo-migrate.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

ID_RE = re.compile(
    r"\b(?:"
    r"(?:I|T|N|D|W|S|K|M|OQ|P|PD|SD|E|C|G|F|RD|D2|V\d+|R\d*[A-Z]?)"
    r"-\d+[a-z]?(?:-[A-Za-z0-9]+)*"
    r"|(?:SD|PD|P|E|C|OQ)\d+"
    r")\b"
)
SEV_RE = re.compile(r"\bP([0-3])\b")
HASH_RE = re.compile(r"\b[0-9a-f]{7,40}\b")
PATH_RE = re.compile(r"[\w./-]+\.(?:rs|md|sh|py|toml):\d+")
TEST_RE = re.compile(r"\b(?:test_[a-z0-9_]+|[a-z0-9_]+::[a-z0-9_]+)\b")
OPEN_RE = re.compile(
    r"待实施|未实施|待裁决|待确认|待核实|待补|待办|开放调查|未修|未完成|\bTODO\b|\bFIXME\b|"
    r"DEFERRED|⏳|☐|\[ \]|\bopen\b|partially fixed|stopgap landed|部分实施|部分修复"
)
CLOSED_RE = re.compile(r"✅|已实施|已关闭|已修|已完成|closed|CLOSED|完成")
DROPPED_RE = re.compile(r"dropped-from-ledgers|已销账|WONTFIX|不实施|登记不实施")
ADJ_RE = re.compile(r"adjudicated|已裁决|已定案")
ARCHIVE_TITLE_RE = re.compile(r"archive|历史|旧版|V\d+\s*archive", re.IGNORECASE)

PROFILES: list[tuple[str, str, str | None, str, str]] = [
    # (path, stage, default_doc, mode, note)
    ("rewrite-notes/01-stage-kernel/01-todo.md", "01-stage-kernel", "01-boot-shim-bootstrap", "mixed", ""),
    ("rewrite-notes/01-stage-kernel/todo.md", "01-stage-kernel", "01-stage-kernel", "mixed", "stage"),
    ("rewrite-notes/01-stage-kernel/08-todo.md", "01-stage-kernel", "01-stage-kernel", "mixed", "stage"),
    ("rewrite-notes/01-stage-kernel/smp_todo.md", "01-stage-kernel", "01-stage-kernel", "mixed", "stage"),
    ("rewrite-notes/01-stage-kernel/endpoint_todo.md", "01-stage-kernel", "01-stage-kernel", "prose", "stage"),
    ("rewrite-notes/01-stage-kernel/draft/06-todo.md", "01-stage-kernel", "01-stage-kernel", "prose", "stage"),
    ("rewrite-notes/02-stage-vm/todo.md", "02-stage-vm", "02-stage-vm", "mixed", "stage"),
    ("rewrite-notes/02-stage-vm/draft/TODO.md", "02-stage-vm", "02-stage-vm", "prose", "stage"),
    ("rewrite-notes/02-stage-vm/archive/todo-V11-archive-2026-09-08.md", "02-stage-vm", "02-stage-vm", "archive", "stage"),
    ("rewrite-notes/02-stage-vm/archive/todo-V12-archive-2026-09-09.md", "02-stage-vm", "02-stage-vm", "archive", "stage"),
    ("rewrite-notes/meta/coordination/TODO-LEDGER-OPEN.md", "coordination", "coordination", "ledger", "stage"),
    ("rewrite-notes/meta/coordination/TODO-LEDGER-DONE.md", "coordination", "coordination", "done", "stage"),
    ("rewrite-notes/meta/coordination/TODO-LEDGER-INDEX.md", "coordination", "coordination", "index", "stage"),
]
RUN_FILES = [
    ("01-boot-shim-bootstrap", "OPEN-DEBT.md", "debt"),
    ("01-boot-shim-bootstrap", "suggestions.md", "suggestions"),
    ("01-boot-shim-bootstrap", "follow-ups.md", "followups"),
    ("02-higher-half-kernel", "OPEN-DEBT.md", "debt"),
    ("02-higher-half-kernel", "suggestions.md", "suggestions"),
    ("02-higher-half-kernel", "follow-ups.md", "followups"),
]


@dataclass
class Item:
    source: str
    line: int
    rid: str
    title: str
    legacy_status: str
    proposed: str
    severity: str
    kind: str
    evidence: str = ""
    doc: str = ""
    stage: str = ""
    scope: str = "doc"
    note: str = ""


def classify(text: str, mode: str) -> tuple[str, str]:
    """返回 (proposed, legacy_status)。proposed ∈ open/open-partial/adjudicated/
    closed-skip/historical-archive/blocked/review。"""
    if mode == "archive":
        return "historical-archive", "archive"
    if mode == "suggestions":
        if DROPPED_RE.search(text) or OPEN_RE.search(text):
            return "open", "不实施（旧建议桶）"
        if CLOSED_RE.search(text):
            return "closed-skip", "已实施"
        return "review", ""
    dropped = DROPPED_RE.search(text)
    adjudicated = ADJ_RE.search(text)
    closed = CLOSED_RE.search(text)
    opened = OPEN_RE.search(text)
    # 先 closed 后 dropped：`✅ 全部实施完毕` 行里可能同时出现「不实施」的说明文字。
    if closed and not opened:
        return "closed-skip", closed.group(0)
    if dropped and not closed:
        return "historical", dropped.group(0)
    if adjudicated and not opened:
        return "adjudicated", adjudicated.group(0)
    if opened:
        legacy = opened.group(0)
        if re.search(r"partially fixed|stopgap landed|部分实施|部分修复", text):
            return "open-partial", legacy
        return "open", legacy
    if closed:
        return "closed-skip", closed.group(0)
    return "review", ""


STATUS_CELL_RE = re.compile(
    r"✅|已实施|已关闭|已修|已完成|待实施|未实施|待裁决|待确认|待核实|待补|待办|"
    r"开放调查|未修|未完成|partially fixed|stopgap landed|部分实施|部分修复|"
    r"adjudicated|已裁决|已定案|dropped-from-ledgers|已销账|WONTFIX|不实施|"
    r"登记不实施|closed|CLOSED|DEFERRED|⏳|☐|\bopen\b|\bdone\b|DONE|已落地|"
    r"已落码|已闭合|已冻结|待送审|已批准待实施|待用户裁决|已关闭|已修"
)


def status_cell(cells: list[str]) -> str:
    """在表格行里找**状态格**，避免把类型/编号列里的 TODO 当待办。"""
    for c in cells[1:]:
        if STATUS_CELL_RE.search(c):
            return c
    return ""


def extract_id(text: str) -> str:
    """优先带连字符/已知前缀的 ID；`P0` 这类优先级不作为 ID。"""
    cands = ID_RE.findall(text)
    if not cands:
        return ""
    def score(x: str) -> tuple[int, int]:
        s = 10 if "-" in x else (5 if re.match(r"^(?:I|T|N|D|W|S|K|M|OQ|P|PD|SD|E|C|G|F|RD|D2)\d", x) else 1)
        return (s, len(x))
    best = max(cands, key=score)
    if "-" not in best and re.fullmatch(r"P[0-3]", best):
        return ""
    return best


def extract_sev(text: str) -> str:
    m = SEV_RE.search(text)
    return f"P{m.group(1)}" if m else "P2"


def extract_kind(text: str) -> str:
    return "design" if "设计" in text[:400] else "code"


def extract_evidence(text: str) -> str:
    parts = []
    for rx in (HASH_RE, PATH_RE, TEST_RE):
        for m in rx.finditer(text):
            parts.append(m.group(0))
            if len(parts) >= 3:
                break
        if len(parts) >= 3:
            break
    return "; ".join(dict.fromkeys(parts))


def _rel(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def table_cells(line: str) -> list[str]:
    return [c.strip() for c in line.strip().strip("|").split("|")]


def is_sep(cells: list[str]) -> bool:
    return bool(cells) and all(set(c) <= set("-: ") for c in cells)


def scan_mixed(path: Path, stage: str, default_doc: str, scope: str) -> list[Item]:
    lines = path.read_text(encoding="utf-8").splitlines()
    items: list[Item] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        # --- 标题式条目：标题里带 ID ---
        if re.match(r"^#{2,4}\s+", stripped):
            rid = extract_id(stripped)
            if rid:
                j = i + 1
                body: list[str] = []
                while j < len(lines) and not re.match(r"^#{2,4}\s+", lines[j]):
                    body.append(lines[j])
                    j += 1
                text = stripped + "\n" + "\n".join(body[:30])
                proposed, legacy = classify(text, "mixed")
                if proposed != "closed-skip":
                    title = re.sub(r"^#{2,4}\s+", "", stripped)
                    title = re.sub(r"（[^）]*）|\([^)]*\)", "", title).strip()[:160]
                    items.append(Item(
                        source=_rel(path), line=i + 1, rid=rid,
                        title=title, legacy_status=legacy, proposed=proposed,
                        severity=extract_sev(text), kind=extract_kind(text),
                        evidence=extract_evidence(text), doc=default_doc,
                        stage=stage, scope=scope,
                    ))
                i = j
                continue
        # --- 表格行：首格或前两格带 ID ---
        if stripped.startswith("|"):
            cells = table_cells(stripped)
            if not is_sep(cells) and cells:
                rid = extract_id(" | ".join(cells[:3]))
                if rid and not re.match(r"^(ID|条目|编号|优先级)$", cells[0]):
                    st = status_cell(cells)
                    text = (st or "") + " " + " ".join(cells[1:3])
                    proposed, legacy = classify(text, "mixed")
                    if proposed != "closed-skip":
                        title = next(
                            (c for c in cells[1:] if c and not SEV_RE.fullmatch(c) and c != "—"),
                            "",
                        )[:160]
                        items.append(Item(
                            source=_rel(path), line=i + 1, rid=rid,
                            title=title or cells[0][:160], legacy_status=legacy,
                            proposed=proposed, severity=extract_sev(text),
                            kind=extract_kind(text), evidence=extract_evidence(text),
                            doc=default_doc, stage=stage, scope=scope,
                        ))
        # --- 复选框/项目符号 ---
        elif re.match(r"^[-*]\s*\[[ xX]\]", stripped) or (
            stripped.startswith("- ") and extract_id(stripped)
        ):
            rid = extract_id(stripped)
            if rid and OPEN_RE.search(stripped):
                items.append(Item(
                    source=_rel(path), line=i + 1, rid=rid,
                    title=stripped.lstrip("-* ")[:160], legacy_status="",
                    proposed="open", severity=extract_sev(stripped),
                    kind=extract_kind(stripped), evidence=extract_evidence(stripped),
                    doc=default_doc, stage=stage, scope=scope,
                ))
        i += 1
    return items


def scan_ledger(path: Path, stage: str, default_doc: str, scope: str) -> list[Item]:
    lines = path.read_text(encoding="utf-8").splitlines()
    items: list[Item] = []
    for i, line in enumerate(lines, 1):
        if not line.strip().startswith("|"):
            continue
        cells = table_cells(line)
        if is_sep(cells) or not cells:
            continue
        rid = extract_id(" | ".join(cells[:4]))
        if not rid or re.match(r"^(ID|条目|编号)$", cells[0]):
            continue
        st = status_cell(cells)
        text = ((st or " | ".join(cells)) + (" " + cells[1] if len(cells) > 1 else ""))
        proposed, legacy = classify(text, "ledger")
        if proposed == "closed-skip":
            continue
        items.append(Item(
            source=_rel(path), line=i, rid=rid,
            title=next((c for c in cells[1:] if len(c) > 4 and c != "—"), rid)[:160],
            legacy_status=legacy, proposed=proposed, severity=extract_sev(text),
            kind=extract_kind(text), evidence=extract_evidence(text),
            doc=default_doc, stage=stage, scope=scope,
        ))
    return items


def scan_archive(path: Path, stage: str, default_doc: str) -> list[Item]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    n_open = len(OPEN_RE.findall(text))
    n_done = len(CLOSED_RE.findall(text))
    return [Item(
        source=_rel(path), line=1,
        rid=f"ARCH-{path.stem}", title=f"archive: {path.name}（{len(lines)} 行）",
        legacy_status=f"archive: open={n_open} done={n_done}", proposed="historical-archive",
        severity="P3", kind="doc", evidence=f"{len(lines)} lines",
        doc=default_doc, stage=stage, scope="stage",
    )]


def scan_prose(path: Path, stage: str, default_doc: str) -> list[Item]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    items = [Item(
        source=_rel(path), line=1, rid=f"PROSE-{path.stem}",
        title=f"prose todo file: {path.name}（{len(lines)} 行）",
        legacy_status="prose", proposed="historical-archive", severity="P3",
        kind="doc", evidence=f"{len(lines)} lines", doc=default_doc,
        stage=stage, scope="stage",
    )]
    parts = re.split(r"^(#{2,4}\s+.*)$", text, flags=re.MULTILINE)
    line_no = 1
    for i in range(1, len(parts), 2):
        head, body = parts[i], parts[i + 1]
        line_no += head.count("\n") + body.count("\n") + 1
        if not OPEN_RE.search(head + "\n" + body[:600]):
            continue
        slug = re.sub(r"[^A-Za-z0-9]+", "-", head.strip("# "))[:32].strip("-") or "sec"
        items.append(Item(
            source=_rel(path), line=line_no, rid=f"PROSE-{path.stem}-{slug}",
            title=head.strip("# ")[:160], legacy_status="prose-open",
            proposed="review", severity=extract_sev(body), kind=extract_kind(body),
            evidence=extract_evidence(body), doc=default_doc, stage=stage,
            scope="stage",
        ))
    return items


def scan_index(path: Path, stage: str, default_doc: str) -> list[Item]:
    return [Item(
        source=_rel(path), line=1,
        rid=f"INDEX-{path.stem}", title=f"alias index: {path.name}",
        legacy_status="alias-index", proposed="historical-archive",
        severity="P3", kind="doc", evidence="aliases", doc=default_doc,
        stage=stage, scope="stage",
    )]


def scan_run(path: Path, doc: str, mode: str) -> list[Item]:
    lines = path.read_text(encoding="utf-8").splitlines()
    items: list[Item] = []
    for i, line in enumerate(lines, 1):
        if not line.strip().startswith("|"):
            continue
        cells = table_cells(line)
        if is_sep(cells) or not cells:
            continue
        rid = extract_id(" | ".join(cells[:3]))
        if not rid:
            continue
        text = " | ".join(cells)
        if mode == "debt":
            if "[CLOSED]" in text:
                continue
            proposed = "blocked" if "用户批准" in text and not "[OPEN]" in text else "open"
            legacy = "OPEN-DEBT"
        elif mode == "suggestions":
            st = status_cell(cells)
            proposed, legacy = classify(st or text, "suggestions")
            if proposed == "closed-skip":
                continue
        else:  # follow-ups
            if re.search(r"CLOSED|closed|已关闭|已修", text) and not OPEN_RE.search(text):
                continue
            st = status_cell(cells)
            proposed, legacy = classify(st or text, "mixed")
            if proposed == "closed-skip":
                continue
        items.append(Item(
            source=_rel(path), line=i, rid=rid,
            title=next((c for c in cells[1:] if len(c) > 4 and c != "—"), rid)[:160],
            legacy_status=legacy, proposed=proposed, severity=extract_sev(text),
            kind=extract_kind(text), evidence=extract_evidence(text),
            doc=doc, stage=doc.split("-")[0] if "-" in doc else doc, scope="doc",
        ))
    return items


def dedup(items: list[Item]) -> list[Item]:
    by_id: dict[str, Item] = {}
    out: list[Item] = []
    for it in items:
        if not it.rid:
            out.append(it)
            continue
        if it.rid in by_id:
            first = by_id[it.rid]
            if it.proposed != first.proposed:
                first.note = (first.note + f"; dup at {it.source}:{it.line}"
                              f" proposed={it.proposed}").strip("; ")
            continue
        by_id[it.rid] = it
        out.append(it)
    return out


def run_all(out_path: Path) -> int:
    items: list[Item] = []
    for rel, stage, default_doc, mode, scope in PROFILES:
        p = ROOT / rel
        if not p.is_file():
            print(f"[SKIP] 不存在: {rel}", file=sys.stderr)
            continue
        if mode in ("archive", "done"):
            items += scan_archive(p, stage, default_doc)
        elif mode == "prose":
            items += scan_prose(p, stage, default_doc)
        elif mode == "index":
            items += scan_index(p, stage, default_doc)
        elif mode == "ledger":
            items += scan_ledger(p, stage, default_doc, scope)
        else:
            items += scan_mixed(p, stage, default_doc, scope)
    for doc, name, mode in RUN_FILES:
        p = ROOT / "rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel" / doc / name
        if not p.is_file():
            print(f"[SKIP] 不存在: {p}", file=sys.stderr)
            continue
        items += scan_run(p, doc, mode)
    items = dedup(items)
    items.sort(key=lambda x: (x.source, x.line, x.rid))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w", encoding="utf-8") as f:
        f.write("source\tline\tid\ttitle\tlegacy_status\tproposed\tseverity\tkind\t"
                "doc\tstage\tscope\tevidence\tnote\n")
        for it in items:
            f.write("\t".join([
                it.source, str(it.line), it.rid, it.title.replace("\t", " "),
                it.legacy_status.replace("\t", " "), it.proposed, it.severity,
                it.kind, it.doc, it.stage, it.scope, it.evidence, it.note,
            ]) + "\n")
    from collections import Counter
    c = Counter(it.proposed for it in items)
    print(f"todo-migrate: {len(items)} 条候选 -> {out_path}")
    print("  " + "  ".join(f"{k}={v}" for k, v in sorted(c.items())))
    return 0


def self_test() -> int:
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        f = root / "x.md"
        f.write_text(
            "# x\n\n"
            "## 1. 总览\n\n"
            "| 条目 | 一句话 | 状态 |\n|---|---|---|\n"
            "| **I-31** | wire v10 | ✅ 已实施 |\n"
            "| **I-35** | write-once | 待实施 |\n"
            "| **I-40** | ghost | 部分实施 |\n"
            "\n### I-50（✅ 已关闭）trait\n\n正文\n\n### I-51（待裁决）\n\n正文\n",
            encoding="utf-8",
        )
        items = scan_mixed(f, "s", "d", "doc")
        ids = {i.rid: i for i in items}
        if "I-31" in ids:
            print("self-test FAIL: closed 行未被跳过")
            return 1
        if ids.get("I-35", None) is None or ids["I-35"].proposed != "open":
            print("self-test FAIL: open 行未识别")
            return 1
        if ids.get("I-40", None) is None or ids["I-40"].proposed != "open-partial":
            print("self-test FAIL: 部分实施未识别为 open-partial")
            return 1
        if ids.get("I-51", None) is None or ids["I-51"].proposed != "open":
            print("self-test FAIL: 待裁决标题未识别")
            return 1
        # archive / ledger / run
        a = root / "a.md"
        a.write_text("# archive\n" + "x\n" * 30, encoding="utf-8")
        if scan_archive(a, "s", "d")[0].proposed != "historical-archive":
            print("self-test FAIL: archive 分类错")
            return 1
        led = root / "l.md"
        led.write_text(
            "| ID | 状态 | 说明 |\n|---|---|---|\n"
            "| I-9 | open | x |\n| I-10 | partially fixed | y |\n| I-11 | closed | z |\n",
            encoding="utf-8",
        )
        li = {i.rid: i.proposed for i in scan_ledger(led, "s", "d", "stage")}
        if li != {"I-9": "open", "I-10": "open-partial"}:
            print(f"self-test FAIL: ledger 分类错 {li}")
            return 1
        run = root / "r.md"
        run.write_text(
            "| ID | 严重度 | 处置 |\n|---|---|---|\n"
            "| F-1 | P1 | [OPEN] 待下一批 |\n| F-2 | P2 | [CLOSED] 已修 a.rs:1 |\n",
            encoding="utf-8",
        )
        ri = scan_run(run, "01-x", "debt")
        if len(ri) != 1 or ri[0].rid != "F-1" or ri[0].proposed != "open":
            print("self-test FAIL: debt 分类错")
            return 1
    print("self-test PASS: mixed/ledger/archive/debt 分类正确")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="旧 TODO → YAML dry-run 提取器")
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--file")
    ap.add_argument("--stage", default="01-stage-kernel")
    ap.add_argument("--doc", default="")
    ap.add_argument("--out", default="tmp/todo-migration/manifest.tsv")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if args.file:
        p = Path(args.file)
        items = scan_mixed(p, args.stage, args.doc or args.stage, "stage")
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open("w", encoding="utf-8") as f:
            f.write("source\tline\tid\ttitle\tlegacy_status\tproposed\tseverity\tkind\t"
                    "doc\tstage\tscope\tevidence\tnote\n")
            for it in items:
                f.write("\t".join([
                    it.source, str(it.line), it.rid, it.title, it.legacy_status,
                    it.proposed, it.severity, it.kind, it.doc, it.stage, it.scope,
                    it.evidence, it.note,
                ]) + "\n")
        print(f"todo-migrate: {len(items)} 条 -> {out}")
        return 0
    return run_all(Path(args.out))


if __name__ == "__main__":
    sys.exit(main())
