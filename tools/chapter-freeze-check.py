#!/usr/bin/env python3
"""chapter-freeze-check.py — 章冻结对账（V3.13 / §Phase 1.2）

对账四件产物，fail-closed：
  CHAPTER-FREEZE.md ↔ 章切片快照 ↔ 六视角 manifest ↔ events.csv。

约定：
- `CHAPTER-FREEZE.md` 每个 `## ChN` 节写 `sha256: <64hex>`、`章切片：<path>`、`摘要：<path>`；
- `v312/chapter-slices/chN-*.md` 是切片快照，sha 必须与 freeze 记录一致；
- `v312/chapters/chN-manifest.md` 是六视角 manifest：
    VERDICT: PASS
    REPORTS:
    - beginner-me.md <sha256>
    ...
- events.csv 中必须有 completed 的 `chapter-review`/`summary-faithful`/`chapter-freeze`
  事件，unit_ids 覆盖该章；
- `summary-faithful` 报告必须含 `VERDICT: FAITHFUL`；
- `chapter-freeze` 报告（CHAPTER-FREEZE.md）必须含 `VERDICT: FROZEN`；
- `chapters/chN/` 下除 `superseded/` 与 `INTERFACE-SUMMARY.md` 外的每份报告必须在 manifest 里。

用法：
  python3 tools/chapter-freeze-check.py <doc> --run-dir <v312> [--chapter N] [--events PATH]
  python3 tools/chapter-freeze-check.py --self-test
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import re
import sys
import tempfile
from pathlib import Path

SHA_RE = re.compile(r"\b([0-9a-f]{64})\b")
CH_RE = re.compile(r"^##\s+Ch(\d+)\b", re.MULTILINE)
MANIFEST_RE = re.compile(r"^\s*-\s+(\S+\.md)\s+([0-9a-f]{64})\s*$", re.MULTILINE)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def freeze_sections(text: str) -> dict[int, str]:
    out: dict[int, str] = {}
    marks = list(CH_RE.finditer(text))
    for i, m in enumerate(marks):
        end = marks[i + 1].start() if i + 1 < len(marks) else len(text)
        out[int(m.group(1))] = text[m.start():end]
    return out


def event_rows(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    with path.open(newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def unit_has(row: dict, n: int) -> bool:
    ids = row.get("unit_ids", "")
    return str(n) in re.split(r"[\s,;]+", ids.strip()) if ids else False


def find_event(rows: list[dict], n: int, typ: str) -> dict | None:
    """取该类型/单元的最新 completed 事件（按 seq，其次按行序），避免行序敏感。"""
    hits = [
        (i, r) for i, r in enumerate(rows)
        if (r.get("review_type") == typ and r.get("status", "").startswith("completed")
            and unit_has(r, n))
    ]
    if not hits:
        return None
    def key(item: tuple[int, dict]) -> tuple[int, int]:
        i, r = item
        try:
            seq = int(str(r.get("seq") or "").strip())
        except ValueError:
            seq = -1
        return seq, i
    return max(hits, key=key)[1]


def resolve_path(raw: str, run_dir: Path) -> Path:
    """解析 freeze 里写的路径：绝对路径，或相对 cwd / run 目录 / run 的父目录。"""
    p = Path(raw)
    if p.is_absolute():
        return p
    for base in (Path.cwd(), run_dir, run_dir.parent):
        cand = base / p
        if cand.is_file():
            return cand
    return Path.cwd() / p


def check_event_report(row: dict, run_dir: Path, label: str, errs: list[str]) -> Path | None:
    raw = row.get("report_path", "")
    rp = resolve_path(raw, run_dir)
    if not rp.is_file():
        errs.append(f"{label} 报告不存在：{raw}")
        return None
    want = str(row.get("report_sha") or "").strip()
    status = str(row.get("status") or "")
    if not status.startswith("completed-pass"):
        errs.append(f"{label} 最新事件不是 completed-pass：{status}")
    if not want:
        errs.append(f"{label} 事件缺 report_sha")
    elif sha(rp) != want:
        errs.append(f"{label} 事件 report_sha 与当前文件不符：{raw}")
    return rp


def check(doc: Path, run_dir: Path, events: Path, only: int | None) -> list[str]:
    errs: list[str] = []
    freeze = run_dir / "CHAPTER-FREEZE.md"
    if not freeze.is_file():
        return [f"缺 {freeze}"]
    text = freeze.read_text(encoding="utf-8", errors="replace")
    if "VERDICT: FROZEN" not in text:
        errs.append("CHAPTER-FREEZE.md 缺 VERDICT: FROZEN")
    secs = freeze_sections(text)
    rows = event_rows(events)
    if not rows:
        errs.append(f"events.csv 为空或不存在：{events}")
    chapters = [only] if only else sorted(secs)
    for n in chapters:
        if n not in secs:
            errs.append(f"CHAPTER-FREEZE.md 缺 Ch{n} 节")
            continue
        sec = secs[n]
        m = SHA_RE.search(sec)
        if not m:
            errs.append(f"Ch{n} 节缺 64 位 sha256")
            continue
        want = m.group(1)
        slices = sorted((run_dir / "chapter-slices").glob(f"ch{n}-*.md"))
        if not slices:
            errs.append(f"Ch{n} 缺切片快照 v312/chapter-slices/ch{n}-*.md")
        else:
            got = {sha(p) for p in slices}
            if want not in got:
                errs.append(f"Ch{n} 切片快照 sha 与 freeze 记录不一致：{sorted(got)} vs {want}")
        # manifest
        man = run_dir / "chapters" / f"ch{n}-manifest.md"
        if not man.is_file():
            errs.append(f"Ch{n} 缺六视角 manifest：{man}")
        else:
            mt = man.read_text(encoding="utf-8", errors="replace")
            if "VERDICT: PASS" not in mt or "REPORTS:" not in mt:
                errs.append(f"Ch{n} manifest 缺 VERDICT: PASS 或 REPORTS:")
            listed = MANIFEST_RE.findall(mt)
            if len(listed) < 6:
                errs.append(f"Ch{n} manifest 只列 {len(listed)} 份报告；至少 6")
            for name, h in listed:
                rp = man.parent / f"ch{n}" / name
                if not rp.is_file():
                    errs.append(f"Ch{n} manifest 指向不存在的报告：{name}")
                elif sha(rp) != h:
                    errs.append(f"Ch{n} manifest 报告 sha 不符：{name}")
            # fail-closed: no unreferenced reports
            listed_names = {x[0] for x in listed}
            d = man.parent / f"ch{n}"
            if d.is_dir():
                for p in d.glob("*.md"):
                    if p.name in ("INTERFACE-SUMMARY.md",) or p.name.startswith("ch") and "manifest" in p.name:
                        continue
                    if p.name not in listed_names:
                        errs.append(f"Ch{n} 报告未登记到 manifest：{p.name}")
        # summary / slice path declared in freeze
        sm = re.search(r"摘要：\s*`([^`]+)`", sec)
        if not sm:
            errs.append(f"Ch{n} 节缺 `摘要：` 路径")
        else:
            sp = resolve_path(sm.group(1), run_dir)
            if not sp.is_file():
                errs.append(f"Ch{n} 摘要文件不存在：{sm.group(1)}")
        sl = re.search(r"章切片：\s*`([^`]+)`", sec)
        if not sl:
            errs.append(f"Ch{n} 节缺 `章切片：` 路径")
        else:
            sp = resolve_path(sl.group(1), run_dir)
            if not sp.is_file():
                errs.append(f"Ch{n} 章切片文件不存在：{sl.group(1)}")
        # events
        cr = find_event(rows, n, "chapter-review")
        if not cr:
            errs.append(f"Ch{n} 缺 completed chapter-review 事件")
        else:
            rp = check_event_report(cr, run_dir, f"Ch{n} chapter-review", errs)
            if rp and man.is_file() and rp.resolve() != man.resolve():
                errs.append(f"Ch{n} chapter-review 事件未指向 manifest：{cr.get('report_path')}")
        sf = find_event(rows, n, "summary-faithful")
        if not sf:
            errs.append(f"Ch{n} 缺 completed summary-faithful 事件")
        else:
            rp = check_event_report(sf, run_dir, f"Ch{n} summary-faithful", errs)
            if rp and "VERDICT: FAITHFUL" not in rp.read_text(encoding="utf-8", errors="replace"):
                errs.append(f"Ch{n} summary-faithful 报告缺 VERDICT: FAITHFUL")
        cf = find_event(rows, n, "chapter-freeze")
        if not cf:
            errs.append(f"Ch{n} 缺 completed chapter-freeze 事件")
        else:
            rp = check_event_report(cf, run_dir, f"Ch{n} chapter-freeze", errs)
            if rp and rp.resolve() != freeze.resolve():
                errs.append(f"Ch{n} chapter-freeze 事件未指向 CHAPTER-FREEZE.md")
            if rp and "VERDICT: FROZEN" not in rp.read_text(encoding="utf-8", errors="replace"):
                errs.append(f"Ch{n} chapter-freeze 报告缺 VERDICT: FROZEN")
    return errs


def self_test() -> int:
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        run = root / "run"
        (run / "chapter-slices").mkdir(parents=True)
        (run / "chapters" / "ch1").mkdir(parents=True)
        doc = root / "doc.md"
        doc.write_text("# doc\n\n## 1. 概述\n\n正文\n", encoding="utf-8")
        sl = run / "chapter-slices" / "ch1-abc.md"
        sl.write_text("## 1. 概述\n\n正文\n", encoding="utf-8")
        h = sha(sl)
        reports = []
        for name in ("beginner-me.md", "g18.md", "editor.md", "navigator.md", "reference.md", "instructional.md"):
            p = run / "chapters" / "ch1" / name
            p.write_text(f"DOC: doc.md\nVERDICT: READABLE\n", encoding="utf-8")
            reports.append(f"- {name} {sha(p)}")
        man = run / "chapters" / "ch1-manifest.md"
        man.write_text("DOC: doc.md\nVERDICT: PASS\nREPORTS:\n" + "\n".join(reports) + "\n", encoding="utf-8")
        (run / "summaries").mkdir(parents=True, exist_ok=True)
        summ = run / "summaries" / "ch1-summary-verification.md"
        summ.write_text("DOC: doc.md\nVERDICT: FAITHFUL\n", encoding="utf-8")
        freeze = run / "CHAPTER-FREEZE.md"
        freeze.write_text(f"# freeze\n\nVERDICT: FROZEN\n\n## Ch1 概述\n- sha256: {h}\n- 章切片：`{sl}`\n- 摘要：`{summ}`\n",
                          encoding="utf-8")
        ev = root / "events.csv"
        with ev.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=["review_id", "doc_id", "edition", "round", "review_type",
                                              "unit_ids", "reviewer", "target_commit", "doc_sha",
                                              "report_path", "report_sha", "status", "seq", "unit_sha"])
            w.writeheader()
            w.writerow({"review_id": "CR-1", "doc_id": "doc.md", "review_type": "chapter-review",
                        "unit_ids": "1", "report_path": str(man), "report_sha": sha(man),
                        "status": "completed-pass"})
            w.writerow({"review_id": "SF-1", "doc_id": "doc.md", "review_type": "summary-faithful",
                        "unit_ids": "1", "report_path": str(summ), "report_sha": sha(summ),
                        "status": "completed-pass"})
            w.writerow({"review_id": "CF-1", "doc_id": "doc.md", "review_type": "chapter-freeze",
                        "unit_ids": "1", "report_path": str(freeze), "report_sha": sha(freeze),
                        "status": "completed-pass"})
        if check(doc, run, ev, 1):
            print("self-test FAIL: 合法冻结被判红：" + "; ".join(check(doc, run, ev, 1)[:2]), file=sys.stderr)
            return 1
        # negative: missing summary path
        good_freeze = freeze.read_text(encoding="utf-8")
        freeze.write_text(good_freeze.replace(f"- 摘要：`{summ}`", "- 摘要：`/nonexistent/summary.md`"),
                          encoding="utf-8")
        if not any("摘要文件不存在" in e for e in check(doc, run, ev, 1)):
            print("self-test FAIL: 摘要路径缺失未被拦截", file=sys.stderr); return 1
        freeze.write_text(good_freeze, encoding="utf-8")
        # negative: extra unregistered report
        extra = run / "chapters" / "ch1" / "extra.md"
        extra.write_text("x", encoding="utf-8")
        if not any("未登记" in e for e in check(doc, run, ev, 1)):
            print("self-test FAIL: 未登记报告未被拦截", file=sys.stderr); return 1
        extra.unlink()
        # negative: stale report_sha
        rows = event_rows(ev)
        for r in rows:
            if r["review_type"] == "summary-faithful":
                r["report_sha"] = "0" * 64
        with ev.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=list(rows[0].keys())); w.writeheader(); w.writerows(rows)
        if not any("report_sha" in e for e in check(doc, run, ev, 1)):
            print("self-test FAIL: 事件 report_sha 失配未被拦截", file=sys.stderr); return 1
        for r in rows:
            if r["review_type"] == "summary-faithful":
                r["report_sha"] = sha(summ)
        with ev.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=list(rows[0].keys())); w.writeheader(); w.writerows(rows)
        # negative: latest event is completed-fail
        for r in rows:
            if r["review_type"] == "summary-faithful":
                r["status"] = "completed-fail"
        with ev.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=list(rows[0].keys())); w.writeheader(); w.writerows(rows)
        if not any("不是 completed-pass" in e for e in check(doc, run, ev, 1)):
            print("self-test FAIL: 最新 completed-fail 未被拦截", file=sys.stderr); return 1
        for r in rows:
            if r["review_type"] == "summary-faithful":
                r["status"] = "completed-pass"
        with ev.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=list(rows[0].keys())); w.writeheader(); w.writerows(rows)
        # negative: missing event
        rows = [r for r in event_rows(ev) if r["review_type"] != "summary-faithful"]
        with ev.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=list(rows[0].keys())); w.writeheader(); w.writerows(rows)
        if not any("summary-faithful" in e for e in check(doc, run, ev, 1)):
            print("self-test FAIL: 缺 summary-faithful 未被拦截", file=sys.stderr); return 1
    print("self-test PASS: 冻结对账、摘要/章切片路径、report_sha、latest-fail、未登记报告、缺事件均按预期")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="章冻结对账")
    ap.add_argument("doc", nargs="?")
    ap.add_argument("--run-dir", type=Path)
    ap.add_argument("--events", type=Path, default=Path("rewrite-notes/meta/review/events.csv"))
    ap.add_argument("--chapter", type=int)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.doc or not args.run_dir:
        ap.print_help(); return 2
    errs = check(Path(args.doc), args.run_dir, args.events, args.chapter)
    for e in errs:
        print(f"[FAIL] {e}")
    if errs:
        print("chapter-freeze-check: FAIL")
        return 1
    print("chapter-freeze-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
