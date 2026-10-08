#!/usr/bin/env bash
# doc-ref-check.sh — 文档内部章节引用对账（doc-commit-review bagging：longcat 3.3 / Muse F-9）
#
# 查内部引用 `第 N 章` / `第 N.M`(节，节/章字可省) / `§N.M`（数字编号）是否指向本文存在的标题。
# 跳过：同一行该引用左侧邻近出现 `.md`（外部引用）；CJK 编号（§一/§六）。
# 字节安全：不假设 `§` 占 1 字节（mawk 按字节走），用 gsub 剔除非数字字符取编号。
# report 恒 exit0（存量 134/369 有误报，未基线前只作人工分诊读数）；--check 命中 exit1（供已基线文档用）。
# 用法： tools/doc-ref-check.sh [--check] <doc.md> | --self-test
#
# 依赖：需 GNU awk（多字节正则）——mawk 不保证多字节字面量匹配，外部 .md 抑制会失真。
#       CI 中以 `AWK=gawk` pin；`awk` 非 GNU Awk 时下方会自提示。
set -uo pipefail

# awk 引擎守卫：--self-test/--check 需多字节正则，非 gawk 时给出明确提示（不静默误报）
_awk="${AWK:-awk}"
if ! "$_awk" --version 2>/dev/null | grep -q 'GNU Awk'; then
  echo "ℹ doc-ref-check：当前 awk ('$_awk') 非 GNU Awk，多字节正则可能失真；请用 AWK=gawk。" >&2
fi

run_check() {
  local doc="$1" mode="${2:-report}"
  local out
  out=$("${AWK:-awk}" '
    # 第一遍：登记标题里的章号(整数)与节号(N.M / N.M.K)
    NR==FNR {
      gsub(/\r$/,"")
      if ($0 ~ /^#{2,4} /) {
        h=$0; sub(/^#+ /,"",h)
        if (match(h, /^第 [0-9]+ 章/)) { n=h; sub(/^第 /,"",n); sub(/[^0-9].*/,"",n); head[n]=1 }
        else if (match(h, /^Ch[0-9]+/)) { n=h; sub(/^Ch/,"",n); sub(/[^0-9].*/,"",n); head[n]=1 }
        else if (match(h, /^[0-9]+(\.[0-9]+)+/)) { t=h; sub(/[^0-9.].*$/,"",t); head[t]=1; split(t,p,"."); head[p[1]]=1 }
        else if (match(h, /^[0-9]+\.?$/)) { t=h; sub(/[^0-9].*$/,"",t); head[t]=1 }
      }
      next
    }
    {
      line=$0; gsub(/\r$/,"",line)
      # §N.M（可 §N.M.K）：按数字串取，字节安全
      s=line
      while (match(s, /§[0-9]+(\.[0-9]+)+/)) {
        seg=substr(s, RSTART, RLENGTH); num=seg; gsub(/[^0-9.]/,"",num)
        before=substr(line, 1, (s==line?RSTART:(length(line)-length(s)+RSTART))-1)
        if (before !~ /\.md/) { if (!(num in head)) bad[++k]=num }
        s=substr(s, RSTART+RLENGTH)
      }
      # 第 N 章 / 第 N.M（节，"节/章"字可省）
      s=line
      while (match(s, /第 [0-9]+(\.[0-9]+)?[  ]*[章节]?/)) {
        seg=substr(s, RSTART, RLENGTH); num=seg; gsub(/[^0-9.]/,"",num)
        if (num=="") { s=substr(s,RSTART+RLENGTH); continue }
        off = (s==line?RSTART:(length(line)-length(s)+RSTART))
        before=substr(line, 1, off-1)
        if (before !~ /\.md/) { if (!(num in head)) bad2[++k2]=num }
        s=substr(s, RSTART+RLENGTH)
      }
    }
    END { for(i=1;i<=k;i++)print "§"bad[i]; for(i=1;i<=k2;i++)print "第 "bad2[i]" 章/节" }
  ' "$doc" "$doc")
  local n
  n=$(printf '%s' "$out" | grep -c . || true)
  if [ "${n:-0}" -gt 0 ]; then
    if [ "$mode" = check ]; then
      echo "⛔ doc-ref-check: $doc 有 $n 处内部引用疑指向不存在章节（人工核对：改对/删/标外部）："
    else
      echo "⚠ doc-ref-check(report): $doc 有 $n 处内部引用待人工分诊（--check 才阻断）："
    fi
    sed 's/^/  · /' <<< "$out"
    [ "$mode" = check ] && return 1 || return 0
  fi
  echo "✅ doc-ref-check: $doc 内部章节引用均可解析"
  return 0
}

self_test() {
  local tmp; tmp=$(mktemp -d); local fail=0
  printf '# t\n## 第 1 章 概念\n### 1.1 小节\n见 §1.1 与 第 1 章 正常；坏引用见 §9.9 与 第 7 章、见第 8.8 那套。\n' > "$tmp/bad.md"
  printf '# t\n## 第 1 章 概念\n### 1.1 小节\n见 §1.1 与 第 1 章；外部 `review.md §1.6` 与 `§六` 应跳过。\n' > "$tmp/good.md"
  run_check "$tmp/bad.md" check >/dev/null 2>&1 && { echo "  FAIL: bad 未被拦"; fail=1; } || echo "  OK: bad 被拦(§9.9/第7章/第8.8)"
  run_check "$tmp/good.md" check >/dev/null 2>&1 && echo "  OK: good 通过" || { echo "  FAIL: good 误拦"; fail=1; }
  rm -rf "$tmp"
  [ "$fail" = 0 ] && { echo "SELF-TEST PASS"; return 0; } || { echo "SELF-TEST FAIL"; return 1; }
}

case "${1:-}" in
  --self-test) self_test; exit $? ;;
  --check) shift; run_check "${1:?用法: doc-ref-check.sh [--check] <doc.md>}" check; exit $? ;;
  ""|-h|--help) echo "用法: $0 [--check] <doc.md> | --self-test"; exit 0 ;;
  *) run_check "$1" report; exit $? ;;
esac
