#!/usr/bin/env python3
"""todo-ledger.py — TODO 单一账本（V3.7，2026-11-21 用户裁决）

背景：此前 TODO 散在 50+ 个 markdown（`01-todo.md`、各 stage `todo.md`、OPEN-DEBT、
suggestions、follow-ups），彼此不引用、无门读取。收敛判据只读 OPEN-DEBT，于是出现
「CONVERGED 但 01-todo.md 还有 24 条待实施」的假收敛。

用户裁决：
  - 规范源 = `rewrite-notes/coordination/todo-ledger/<stage>.yaml`（每 stage 一个，入库）。
  - 旧 todo 文件冻结为 archive，只迁 **open** 项；closed 历史留在旧文件。
  - 零欠账：CONVERGED 时当前篇 `status: open` 必须为 0。

用法：
  python3 tools/todo-ledger.py check [--doc DOC] [--fail-on-open]
  python3 tools/todo-ledger.py list  [--doc DOC]
  python3 tools/todo-ledger.py import-md <todo.md> --stage S --doc D --out YAML
  python3 tools/todo-ledger.py import-suggestions <suggestions.md> --stage S --doc D --out YAML
  python3 tools/todo-ledger.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

try:
    import yaml
except Exception:  # pragma: no cover
    print("需要 PyYAML（python3 -m pip install pyyaml）", file=sys.stderr)
    raise SystemExit(2)

ROOT = Path(__file__).resolve().parents[1]
LEDGER_DIR = ROOT / "rewrite-notes/coordination/todo-ledger"
STATUSES = {"open", "closed", "rejected", "defer-approved"}
SEVERITIES = {"P0", "P1", "P2", "P3"}
KINDS = {"code", "doc", "design", "arch"}
REQUIRED = ("id", "stage", "doc", "title", "status", "severity", "kind")
OPEN_MARKERS = re.compile(r"待实施|未实施|登记不实施|不实施|待裁决|待用户批准|开放调查|未修|待补|待办")
CLOSED_MARKERS = re.compile(r"✅|已实施|已关闭|已修|完成")
ID_RE = re.compile(r"\b((?:I|T|N|D)-\d+[a-z]?|D-\d{3})\b")
SEV_RE = re.compile(r"\bP([0-3])\b")
ORIGIN_RE = re.compile(r"来源\s*[:：]\s*([^\n；;]{0,120})")


def load(path: Path) -> list[dict]:
    data = yaml.safe_load(path.read_text(encoding="utf-8")) or []
    if not isinstance(data, list):
        raise SystemExit(f"{path}: 顶层必须是列表")
    return data


def save(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    header = (
        "# TODO 单一账本（V3.7）——规范源；旧 todo markdown 仅为归档。\n"
        "# 字段：id/stage/doc/title/status/severity/kind/origin/evidence/defer\n"
        "# status: open | closed | rejected | defer-approved\n"
        "# defer-approved 必须带 decision/approved/until/consequence 四项。\n"
    )
    body = yaml.safe_dump(rows, allow_unicode=True, sort_keys=False, width=120)
    path.write_text(header + body, encoding="utf-8")


def check_rows(rows: list[dict], doc: str | None) -> tuple[list[str], list[str], int]:
    errors: list[str] = []
    warnings: list[str] = []
    seen: set[str] = set()
    open_count = 0
    for i, r in enumerate(rows, 1):
        where = f"#{i} {r.get('id', '<no id>')}"
        for k in REQUIRED:
            if k not in r or r[k] in (None, ""):
                errors.append(f"{where}: 缺字段 {k}")
        rid = str(r.get("id", ""))
        if rid in seen:
            errors.append(f"{where}: ID 重复")
        seen.add(rid)
        if r.get("status") not in STATUSES:
            errors.append(f"{where}: status 非法 `{r.get('status')}`")
        if r.get("severity") not in SEVERITIES:
            errors.append(f"{where}: severity 非法 `{r.get('severity')}`")
        if r.get("kind") not in KINDS:
            errors.append(f"{where}: kind 非法 `{r.get('kind')}`")
        if r.get("status") == "closed" and not str(r.get("evidence", "")).strip():
            errors.append(f"{where}: closed 缺 evidence（commit / path:line / test:: / dup-of）")
        if r.get("status") == "rejected":
            ev = str(r.get("evidence", ""))
            if not re.search(r"D-\d{3}", ev):
                errors.append(f"{where}: rejected 必须引用用户裁决 D-xxx")
        if r.get("status") == "defer-approved":
            d = r.get("defer") or {}
            for k in ("decision", "approved", "until", "consequence"):
                if not d.get(k):
                    errors.append(f"{where}: defer-approved 缺 defer.{k}")
            if d.get("decision") and not re.search(r"D-\d{3}", str(d["decision"])):
                errors.append(f"{where}: defer.decision 必须是 D-xxx")
        if r.get("status") == "open" and (doc is None or r.get("doc") == doc):
            open_count += 1
    return errors, warnings, open_count


def cmd_check(args) -> int:
    files = sorted(LEDGER_DIR.glob("*.yaml"))
    if not files:
        print(f"todo-ledger: {LEDGER_DIR} 下没有 .yaml")
        return 2
    all_errors: list[str] = []
    all_open = 0
    for f in files:
        rows = load(f)
        errors, _, open_count = check_rows(rows, args.doc)
        all_errors += [f"{f.name}: {e}" for e in errors]
        all_open += open_count
        print(f"[账本] {f.name}: {len(rows)} 条，open={open_count}"
              + (f"（doc={args.doc}）" if args.doc else ""))
    for e in all_errors:
        print(f"[FAIL] {e}")
    if all_errors:
        print(f"todo-ledger check: {len(all_errors)} 处错误")
        return 1
    if args.fail_on_open and all_open:
        print(f"todo-ledger check: 当前范围仍有 {all_open} 条 open（零欠账要求 0）")
        return 1
    print("todo-ledger check: OK")
    return 0


def cmd_list(args) -> int:
    for f in sorted(LEDGER_DIR.glob("*.yaml")):
        for r in load(f):
            if args.doc and r.get("doc") != args.doc:
                continue
            print(f"{r['id']:16} {r['status']:14} {r['severity']:3} {r['kind']:7} {r['title'][:70]}")
    return 0


def _section_open(title: str, body: str) -> bool:
    head = title + "\n" + body[:400]
    return bool(OPEN_MARKERS.search(head)) and not (
        CLOSED_MARKERS.search(title) and not OPEN_MARKERS.search(title)
    )


def _merge(out: Path, new_rows: list[dict]) -> list[dict]:
    rows = load(out) if out.is_file() else []
    by_id = {r["id"]: r for r in rows}
    for r in new_rows:
        by_id.setdefault(r["id"], r)
    return sorted(by_id.values(), key=lambda r: (r["stage"], r["doc"], r["id"]))


def cmd_import_md(args) -> int:
    text = Path(args.file).read_text(encoding="utf-8")
    parts = re.split(r"^###\s+", text, flags=re.MULTILINE)[1:]
    new_rows: list[dict] = []
    for part in parts:
        title, _, body = part.partition("\n")
        title = title.strip()
        if not _section_open(title, body):
            continue
        m = ID_RE.search(title)
        rid = m.group(1) if m else re.sub(r"[^A-Za-z0-9]+", "-", title)[:40]
        sev = SEV_RE.search(body) or SEV_RE.search(title)
        origin = ORIGIN_RE.search(body)
        new_rows.append({
            "id": f"{args.prefix}{rid}" if args.prefix else rid,
            "stage": args.stage,
            "doc": args.doc,
            "title": re.sub(r"（[^）]*）|\([^)]*\)", "", title).strip()[:160],
            "status": "open",
            "severity": f"P{sev.group(1)}" if sev else "P2",
            "kind": "design" if "设计" in body[:300] else "code",
            "origin": (origin.group(1).strip() if origin else ""),
            "evidence": "",
        })
    out = Path(args.out)
    rows = _merge(out, new_rows)
    save(out, rows)
    print(f"import-md: 从 {args.file} 迁入 {len(new_rows)} 条 open；账本共 {len(rows)} 条 -> {out}")
    return 0


def cmd_import_suggestions(args) -> int:
    text = Path(args.file).read_text(encoding="utf-8")
    new_rows: list[dict] = []
    for line in text.splitlines():
        if not line.strip().startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 2 or "不实施" not in " ".join(cells):
            continue
        rid = cells[0].strip("`* ")
        if not re.match(r"[A-Za-z]", rid):
            continue
        new_rows.append({
            "id": f"{args.prefix}{rid}" if args.prefix else rid,
            "stage": args.stage,
            "doc": args.doc,
            "title": f"面板建议：{rid}",
            "status": "open",
            "severity": "P3",
            "kind": "design",
            "origin": args.file,
            "evidence": "",
        })
    out = Path(args.out)
    rows = _merge(out, new_rows)
    save(out, rows)
    print(f"import-suggestions: 迁入 {len(new_rows)} 条；账本共 {len(rows)} 条 -> {out}")
    return 0


def self_test() -> int:
    import tempfile

    good = [
        {"id": "D01-I-31", "stage": "01-stage-kernel", "doc": "01", "title": "t",
         "status": "closed", "severity": "P1", "kind": "code",
         "evidence": "abc1234 path:1"},
        {"id": "D01-I-32", "stage": "01-stage-kernel", "doc": "01", "title": "t2",
         "status": "defer-approved", "severity": "P2", "kind": "code",
         "defer": {"decision": "D-107", "approved": "2026-11-21",
                   "until": "出现第三调用者", "consequence": "…"}},
    ]
    errors, _, _ = check_rows(good, None)
    if errors:
        print(f"self-test FAIL: 合规账本被判错 {errors}")
        return 1
    bad = [
        {"id": "X-1", "stage": "s", "doc": "d", "title": "t", "status": "closed",
         "severity": "P1", "kind": "code"},
        {"id": "X-1", "stage": "s", "doc": "d", "title": "dup", "status": "open",
         "severity": "P9", "kind": "unknown"},
        {"id": "X-2", "stage": "s", "doc": "d", "title": "t", "status": "defer-approved",
         "severity": "P2", "kind": "code", "defer": {"decision": "D-1"}},
    ]
    errors, _, open_count = check_rows(bad, "d")
    need = ["缺 evidence", "ID 重复", "severity 非法", "kind 非法", "defer-approved 缺"]
    miss = [n for n in need if not any(n in e for e in errors)]
    if miss:
        print(f"self-test FAIL: 漏检 {miss}")
        return 1
    if open_count != 1:
        print(f"self-test FAIL: open 计数错（{open_count}，应 1）")
        return 1
    print("self-test PASS: 字段/重复/closed 证据/rejected D 行/defer 四要素/open 计数 均正确")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="TODO 单一账本（V3.7）")
    sub = ap.add_subparsers(dest="cmd", required=False)
    c = sub.add_parser("check")
    c.add_argument("--doc")
    c.add_argument("--fail-on-open", action="store_true")
    l = sub.add_parser("list")
    l.add_argument("--doc")
    im = sub.add_parser("import-md")
    im.add_argument("file")
    im.add_argument("--stage", required=True)
    im.add_argument("--doc", required=True)
    im.add_argument("--out", required=True)
    im.add_argument("--prefix", default="")
    isg = sub.add_parser("import-suggestions")
    isg.add_argument("file")
    isg.add_argument("--stage", required=True)
    isg.add_argument("--doc", required=True)
    isg.add_argument("--out", required=True)
    isg.add_argument("--prefix", default="")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.cmd:
        ap.print_help()
        return 2
    if args.cmd == "check":
        return cmd_check(args)
    if args.cmd == "list":
        return cmd_list(args)
    if args.cmd == "import-md":
        return cmd_import_md(args)
    if args.cmd == "import-suggestions":
        return cmd_import_suggestions(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
