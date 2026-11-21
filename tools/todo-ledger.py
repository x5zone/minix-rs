#!/usr/bin/env python3
"""todo-ledger.py — TODO 单一账本（V3.7 / V3.8-r2 迁移扩展）

背景：此前 TODO 散在 50+ 个 markdown（旧 stage `todo.md`（已迁移删除）、OPEN-DEBT、
suggestions、follow-ups），彼此不引用、无门读取。收敛判据只读 OPEN-DEBT，于是出现
「CONVERGED 但旧 todo 文档还有 24 条待实施」的假收敛。

用户裁决：
  - 规范源 = `rewrite-notes/coordination/todo-ledger/<stage>.yaml`（每 stage 一个，入库）。
  - 旧 todo 文件在迁移后删除；closed 历史靠 git 历史 + origin_commit 追溯。
  - 零欠账：CONVERGED 时当前篇 `status: open` 必须为 0。
  - 待裁项必须 `blocked: true` + `blocked_reason`，阻断收敛。

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
STATUSES = {"open", "closed", "rejected", "defer-approved", "historical"}
SEVERITIES = {"P0", "P1", "P2", "P3"}
KINDS = {"code", "doc", "design", "arch"}
SCOPES = {"doc", "stage"}
REQUIRED = ("id", "stage", "doc", "title", "status", "severity", "kind")
OPEN_MARKERS = re.compile(r"待实施|未实施|登记不实施|不实施|待裁决|待用户批准|开放调查|未修|待补|待办")
CLOSED_MARKERS = re.compile(r"✅|已实施|已关闭|已修|完成")
ID_RE = re.compile(r"\b((?:I|T|N|D)-\d+[a-z]?|D-\d{3})\b")
SEV_RE = re.compile(r"\bP([0-3])\b")
ORIGIN_RE = re.compile(r"来源\s*[:：]\s*([^\n；;]{0,120})")
DATE_RE = re.compile(r"^20\d{2}-\d{2}-\d{2}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{7,40}$")


def canon_doc(value: str | None) -> str | None:
    """`--doc` 归一：去掉目录前缀与 `.md`，只比 basename。"""
    if not value:
        return None
    name = Path(str(value)).name
    return name[:-3] if name.endswith(".md") else name


def doc_stage(doc: str | None) -> str | None:
    """从正式篇目路径反查 stage 目录名（用于 scope: stage 的行）。"""
    if not doc:
        return None
    name = canon_doc(doc)
    for p in ROOT.glob("rewrite-notes/*-stage-*/**/*.md"):
        if p.stem == name:
            for part in p.parts:
                if part.endswith("-stage-"):
                    return part
                if re.match(r"^\d+-stage-", part):
                    return part
    return None


def load(path: Path) -> list[dict]:
    data = yaml.safe_load(path.read_text(encoding="utf-8")) or []
    if not isinstance(data, list):
        raise SystemExit(f"{path}: 顶层必须是列表")
    return data


def save(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    header = (
        "# TODO 单一账本（V3.7 / V3.8-r2）——规范源；旧 todo markdown 已迁移后删除。\n"
        "# 字段：id/stage/doc/title/status/severity/kind/origin/evidence/defer/\n"
        "#       merged_into/legacy_status/origin_file/origin_line/origin_commit/\n"
        "#       verified_by/verified_at/aliases/scope/blocked/blocked_reason\n"
        "# status: open | closed | rejected | defer-approved | historical\n"
        "# scope: doc（默认）| stage；blocked=true 必须带 blocked_reason。\n"
        "# defer-approved 必须带 decision/approved/until/consequence 四项。\n"
    )
    body = yaml.safe_dump(rows, allow_unicode=True, sort_keys=False, width=120)
    path.write_text(header + body, encoding="utf-8")


def _row_matches_doc(r: dict, doc: str | None) -> bool:
    if doc is None:
        return True
    if canon_doc(r.get("doc")) == canon_doc(doc):
        return True
    if r.get("scope") == "stage" and doc_stage(doc) is not None:
        return r.get("stage") == doc_stage(doc)
    return False


def check_rows(
    rows: list[dict],
    doc: str | None,
    all_ids: set[str] | None = None,
    global_seen: set[str] | None = None,
) -> tuple[list[str], list[str], int, int]:
    errors: list[str] = []
    warnings: list[str] = []
    seen: set[str] = global_seen if global_seen is not None else set()
    open_count = 0
    blocked_count = 0
    for i, r in enumerate(rows, 1):
        where = f"#{i} {r.get('id', '<no id>')}"
        for k in REQUIRED:
            if k not in r or r[k] in (None, ""):
                errors.append(f"{where}: 缺字段 {k}")
        rid = str(r.get("id", ""))
        if rid in seen:
            errors.append(f"{where}: ID 重复（跨文件也计）")
        seen.add(rid)
        if r.get("status") not in STATUSES:
            errors.append(f"{where}: status 非法 `{r.get('status')}`")
        if r.get("severity") not in SEVERITIES:
            errors.append(f"{where}: severity 非法 `{r.get('severity')}`")
        if r.get("kind") not in KINDS:
            errors.append(f"{where}: kind 非法 `{r.get('kind')}`")
        if r.get("scope", "doc") not in SCOPES:
            errors.append(f"{where}: scope 非法 `{r.get('scope')}`")
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
        # V3.7-r2：待裁项必须显式 blocked，且必须有理由。
        if r.get("blocked"):
            if not str(r.get("blocked_reason", "")).strip():
                errors.append(f"{where}: blocked=true 缺 blocked_reason")
            if r.get("status") not in ("open", "historical"):
                errors.append(f"{where}: blocked=true 的 status 必须是 open 或 historical")
            if _row_matches_doc(r, doc):
                blocked_count += 1
        # ---- V3.8-r2 迁移扩展字段（全部可选）----
        merged = r.get("merged_into")
        if merged is not None:
            if not str(merged).strip():
                errors.append(f"{where}: merged_into 为空")
            elif str(merged) == rid:
                errors.append(f"{where}: merged_into 不能指向自己")
            elif all_ids is not None and str(merged) not in all_ids:
                errors.append(f"{where}: merged_into 目标 `{merged}` 不存在")
            if r.get("status") not in ("closed", "historical"):
                errors.append(f"{where}: merged_into 的 status 必须是 closed 或 historical")
        if "legacy_status" in r and not str(r.get("legacy_status", "")).strip():
            errors.append(f"{where}: legacy_status 为空")
        of = r.get("origin_file")
        ol = r.get("origin_line")
        oc = r.get("origin_commit")
        if of is not None:
            if not str(of).strip():
                errors.append(f"{where}: origin_file 为空")
            elif not oc and not (ROOT / str(of)).is_file():
                errors.append(f"{where}: origin_file 不存在且无 origin_commit: {of}")
        if ol is not None:
            if not of:
                errors.append(f"{where}: origin_line 需要 origin_file")
            elif not isinstance(ol, int) or ol < 1:
                errors.append(f"{where}: origin_line 必须是 >=1 的整数")
            elif of and (ROOT / str(of)).is_file():
                n = len((ROOT / str(of)).read_text(encoding="utf-8").splitlines())
                if ol > n:
                    warnings.append(f"{where}: origin_line {ol} 超出文件行数 {n}")
        if oc is not None and not COMMIT_RE.match(str(oc)):
            errors.append(f"{where}: origin_commit 不是 7–40 位十六进制")
        vb, va = r.get("verified_by"), r.get("verified_at")
        if (vb is None) != (va is None):
            errors.append(f"{where}: verified_by/verified_at 必须成对出现")
        if va is not None and not DATE_RE.match(str(va)):
            errors.append(f"{where}: verified_at 必须是 YYYY-MM-DD")
        if va is not None and not str(r.get("evidence", "")).strip():
            errors.append(f"{where}: 有 verified_at 时 evidence 不能为空")
        aliases = r.get("aliases")
        if aliases is not None:
            if not isinstance(aliases, list):
                errors.append(f"{where}: aliases 必须是列表")
            else:
                if len(aliases) != len(set(map(str, aliases))):
                    errors.append(f"{where}: aliases 有重复")
                if rid in {str(a) for a in aliases}:
                    errors.append(f"{where}: aliases 不能包含自身 ID")
                for a in aliases:
                    if not str(a).strip():
                        errors.append(f"{where}: aliases 含空值")
        if r.get("status") == "open" and _row_matches_doc(r, doc):
            open_count += 1
    return errors, warnings, open_count, blocked_count


def cmd_check(args) -> int:
    files = sorted(LEDGER_DIR.glob("*.yaml"))
    if not files:
        print(f"todo-ledger: {LEDGER_DIR} 下没有 .yaml")
        return 2
    all_rows: list[tuple[Path, list[dict]]] = [(f, load(f)) for f in files]
    all_ids = {str(r.get("id")) for _, rows in all_rows for r in rows}
    global_seen: set[str] = set()
    all_errors: list[str] = []
    all_warnings: list[str] = []
    all_open = 0
    all_blocked = 0
    matched = 0
    for f, rows in all_rows:
        errors, warnings, open_count, blocked_count = check_rows(
            rows, args.doc, all_ids, global_seen
        )
        all_errors += [f"{f.name}: {e}" for e in errors]
        all_warnings += [f"{f.name}: {w}" for w in warnings]
        all_open += open_count
        all_blocked += blocked_count
        matched += sum(1 for r in rows if _row_matches_doc(r, args.doc))
        print(f"[账本] {f.name}: {len(rows)} 条，open={open_count}，blocked={blocked_count}"
              + (f"（doc={args.doc}）" if args.doc else ""))
    for w in all_warnings:
        print(f"[WARN] {w}")
    for e in all_errors:
        print(f"[FAIL] {e}")
    if args.doc and matched == 0:
        print(f"todo-ledger check: --doc `{args.doc}` 匹配 0 行（doc 名或路径写错？）")
        return 1
    if all_errors:
        print(f"todo-ledger check: {len(all_errors)} 处错误")
        return 1
    if args.fail_on_open and all_blocked:
        print(f"todo-ledger check: 当前范围有 {all_blocked} 条 blocked 待裁项——"
              "绝不允许收敛（即使代裁授权也不得自行决定）")
        return 1
    if args.fail_on_open and all_open:
        print(f"todo-ledger check: 当前范围仍有 {all_open} 条 open（零欠账要求 0）")
        return 1
    print("todo-ledger check: OK")
    return 0


def cmd_list(args) -> int:
    for f in sorted(LEDGER_DIR.glob("*.yaml")):
        for r in load(f):
            if not _row_matches_doc(r, args.doc):
                continue
            print(f"{r['id']:22} {r['status']:14} {r['severity']:3} {r['kind']:7} {r['title'][:70]}")
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
        {"id": "D01-ARCH-1", "stage": "01-stage-kernel", "doc": "01-todo",
         "title": "archive", "status": "historical", "severity": "P3", "kind": "doc",
         "legacy_status": "V11 archive", "origin_file": "rewrite-notes/MIGRATION.md",
         "origin_line": 1, "aliases": ["V11-1", "P0-1"]},
        {"id": "D01-DUP-1", "stage": "01-stage-kernel", "doc": "01", "title": "dup",
         "status": "closed", "severity": "P3", "kind": "doc",
         "evidence": "dup-of D01-I-31", "merged_into": "D01-I-31"},
    ]
    ids = {r["id"] for r in good}
    errors, _, _, _ = check_rows(good, None, ids)
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
        {"id": "X-3", "stage": "s", "doc": "d", "title": "m", "status": "open",
         "severity": "P2", "kind": "code", "merged_into": "NOPE"},
        {"id": "X-4", "stage": "s", "doc": "d", "title": "v", "status": "open",
         "severity": "P2", "kind": "code", "verified_by": "a"},
        {"id": "X-5", "stage": "s", "doc": "d", "title": "a", "status": "open",
         "severity": "P2", "kind": "code", "aliases": ["X-5"]},
        {"id": "X-6", "stage": "s", "doc": "d", "title": "o", "status": "open",
         "severity": "P2", "kind": "code", "origin_file": "no/such/file.md"},
    ]
    errors, _, open_count, _bc = check_rows(bad, "d", {r["id"] for r in bad})
    need = ["缺 evidence", "ID 重复", "severity 非法", "kind 非法", "defer-approved 缺",
            "merged_into 目标", "verified_by/verified_at 必须成对", "aliases 不能包含自身",
            "origin_file 不存在"]
    miss = [n for n in need if not any(n in e for e in errors)]
    if miss:
        print(f"self-test FAIL: 漏检 {miss}")
        return 1
    if open_count != 5:
        print(f"self-test FAIL: open 计数错（{open_count}，应 5）")
        return 1
    blocked = [{"id": "B-1", "stage": "s", "doc": "d", "title": "待裁",
                "status": "open", "severity": "P1", "kind": "arch",
                "blocked": True, "blocked_reason": "用户未定"}]
    e2, _, o2, b2 = check_rows(blocked, "d", {"B-1"})
    if e2 or o2 != 1 or b2 != 1:
        print(f"self-test FAIL: blocked 计数错（errors={e2} open={o2} blocked={b2}）")
        return 1
    e3, _, _, _ = check_rows(
        [{"id": "B-2", "stage": "s", "doc": "d", "title": "x", "status": "open",
          "severity": "P1", "kind": "arch", "blocked": True}], "d", {"B-2"})
    if not any("blocked_reason" in e for e in e3):
        print("self-test FAIL: blocked 缺理由未拦截")
        return 1
    # --doc 归一：路径/`.md` 与 basename 等价
    if canon_doc("rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md") != "01-boot-shim-bootstrap":
        print("self-test FAIL: canon_doc 未归一")
        return 1
    print("self-test PASS: 字段/跨文件重复/closed 证据/rejected D 行/defer 四要素/"
          "blocked/merged_into/verified 成对/aliases/origin/doc 归一 均正确")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="TODO 单一账本（V3.7 / V3.8-r2）")
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
