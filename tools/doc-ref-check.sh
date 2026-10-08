#!/usr/bin/env bash
# doc-ref-check.sh — 文档内部章节引用对账（doc-commit-review bagging：longcat 3.3 / Muse F-9）
#
# 查内部引用 `第 N 章` / `第 N.M 节` / `§N.M`（数字编号）是否指向本文存在的标题
# （`## 第 N 章`、`### N.M`、`#### N.M.K`）。
# 跳过：同一行该引用前紧跟 `.md`（外部文档引用，如 `review-core-semantics.md §1.6`）；
#        CJK 编号（§一/§六 等指命令规范）。
# 判据（保守）：目标节号在本文标题中不存在 → 列为 suspect。report 仅报告；--check 命中 exit1。
# 用法： tools/doc-ref-check.sh [--check] <doc.md> | --self-test
set -uo pipefail   # 不用 -e：逐行对账里大量 grep 无命中是正常路径

run_check() {
  local doc="$1" mode="${2:-report}"
  local out
  out=$(awk '
    # 第一遍：收集标题里的章号/节号
    NR==FNR {
      if ($0 ~ /^#{2,4} /) {
        h=$0; sub(/^#+ /,"",h)
        if (match(h, /^第 [0-9]+ 章/))            { n=h; sub(/^第 /,"",n); sub(/ 章.*/,"",n); head[n]=1 }
        else if (match(h, /^[0-9]+(\.[0-9]+)+/))  { split(h,a," "); head[a[1]]=1 }   # N.M 或 N.M.K
      }
      next
    }
    # 第二遍：扫引用
    {
      line=$0
      # 该整行若含 .md 引用（外部），对行内每个 §N.M 前若有 .md 则跳过——简化为：含 .md 的行仍逐 token 判，
      # 但仅当 token 前无 .md。此处按 token 前缀判定。
      # 抽 §N.M
      s=line
      while (match(s, /§[0-9]+\.[0-9]+/)) {
        tok=substr(s, RSTART+1, RLENGTH-1)       # 去 §
        before=substr(s, 1, RSTART-1)
        if (before !~ /\.md/) { if (!(tok in head)) bad[++k]=tok }
        s=substr(s, RSTART+RLENGTH)
      }
      # 抽 第 N(.M)? 章/节
      s=line
      while (match(s, /第 [0-9]+(\.[0-9]+)? ?[章节]/)) {
        t=substr(s, RSTART, RLENGTH)
        num=t; sub(/^第 /,"",num); sub(/[  ]?[章节].*$/,"",num)
        before=substr(line,1,RSTART-1)
        if (before !~ /\.md/) { if (!(num in head)) bad2[++k2]=num }
        # 消费掉本次匹配
        s=substr(s, RSTART+RLENGTH)
      }
    }
    END {
      for (i=1;i<=k;i++)  print "§" bad[i]
      for (i=1;i<=k2;i++) print "第 " bad2[i] " 章/节"
    }
  ' "$doc" "$doc")
  local n
  n=$(printf '%s' "$out" | grep -c . || true)
  if [ "${n:-0}" -gt 0 ]; then
    echo "⛔ doc-ref-check: $doc 有 $n 处内部引用疑指向不存在章节（人工核对：改对 / 删 / 或标外部）："
    sed 's/^/  · /' <<< "$out"
    [ "$mode" = check ] && return 1 || return 0
  fi
  echo "✅ doc-ref-check: $doc 内部章节引用均可解析"
  return 0
}

self_test() {
  local tmp; tmp=$(mktemp -d); local fail=0
  printf '# t\n## 第 1 章 概念\n### 1.1 小节\n见 §1.1 与 第 1 章 正常；坏引用见 §9.9 与 第 7 章。\n' > "$tmp/bad.md"
  printf '# t\n## 第 1 章 概念\n### 1.1 小节\n见 §1.1 与 第 1 章；外部 `review.md §1.6` 与 `§六` 应跳过。\n' > "$tmp/good.md"
  if run_check "$tmp/bad.md" check >/dev/null; then echo "  FAIL: bad 未被拦"; fail=1; else echo "  OK: bad 被拦"; fi
  if run_check "$tmp/good.md" check >/dev/null; then echo "  OK: good 通过"; else echo "  FAIL: good 误拦"; fail=1; fi
  rm -rf "$tmp"
  [ "$fail" = 0 ] && { echo "SELF-TEST PASS"; return 0; } || { echo "SELF-TEST FAIL"; return 1; }
}

case "${1:-}" in
  --self-test) self_test; exit $? ;;
  --check) shift; run_check "${1:?用法: doc-ref-check.sh [--check] <doc.md>}" check; exit $? ;;
  ""|-h|--help) echo "用法: $0 [--check] <doc.md> | --self-test"; exit 0 ;;
  *) run_check "$1" report; exit $? ;;
esac
