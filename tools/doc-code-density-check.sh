#!/usr/bin/env bash
# doc-code-density-check.sh — 教学构件承载门的机器读数（review-cmds §一 通用门 #9）
#
# 判据（中等尺度）：机制类文档的 **C 源码分析章 / 实现章** 不得为纯散文——
#   该类章节 0 个代码围栏块 → P1-docStyle（全文 < 300 行时降为 P2）。
#   其它章（概念/测试/参见等）只报数，不判级。
#   非机制章缺码不算违规（避免"为凑码而贴码"）。
#
# 用法：
#   tools/doc-code-density-check.sh <doc.md>            # 出读数 + 判级
#   tools/doc-code-density-check.sh --check <doc.md>    # 同上，命中 P1 时退出码 1（供 CI/门用）
#   tools/doc-code-density-check.sh --self-test         # 夹具自检
#
# 章节识别：按 `## ` 二级标题切块；"分析/实现"类关键词命中即视为承载对象。
# 代码块识别：以 ``` 起始的围栏（含 ```c / ```rust / 无语言）；配对计数。
set -euo pipefail

KEYWORDS='源码分析|C 源|关键函数|实现|落地|详解|Rust 设计|代码分析'

analyze_chapters() {
  # $1 = doc path; 输出每章：章节标题 | 代码块数 | 是否承载章
  local doc="$1"
  awk -v kw="$KEYWORDS" '
    BEGIN { n=0; title="(preamble)"; blocks=0; infence=0 }
    function flush(){ if (seen) printf "%s\t%d\n", title, blocks }
    /^## /{
      flush();
      title=substr($0,4); blocks=0; seen=1;
      ismech = (title ~ kw) ? 1 : 0
      next
    }
    /^```/{
      if (infence==0){ infence=1; blocks++ } else { infence=0 }
      next
    }
    END{ flush() }
  ' "$doc"
}

grade_doc() {
  local doc="$1" mode="${2:-report}"
  local total; total=$(wc -l < "$doc")
  local shortdoc=0; (( total < 300 )) && shortdoc=1
  local rc=0
  echo "== doc-code-density-check: $doc （全文 ${total} 行$([ $shortdoc = 1 ] && echo '，短文档 → 承载缺码降 P2')）"
  printf '%-40s %-10s %-10s\n' "章节" "代码块" "判定"
  while IFS=$'\t' read -r title blocks; do
    [ -z "${title:-}" ] && continue
    local ismech=0
    echo "$title" | grep -Eq "$KEYWORDS" && ismech=1
    local verdict="-"
    if [ "$ismech" = 1 ]; then
      if [ "$blocks" -eq 0 ]; then
        if [ "$shortdoc" = 1 ]; then
          verdict="P2 承载缺码(短文档降级)";
        else
          verdict="P1-docStyle 承载缺码"; rc=1
        fi
      else
        verdict="OK 有码(${blocks})"
      fi
    else
      [ "$blocks" -gt 0 ] && verdict="非承载章(${blocks})"
    fi
    local t="${title}"; [ ${#t} -gt 38 ] && t="${t:0:38}…"
    printf '%-40s %-10s %-10s\n' "$t" "$blocks" "$verdict"
  done < <(analyze_chapters "$doc")
  echo "---"
  if [ "$rc" = 0 ]; then
    echo "✅ 承载门通过（机制章均贴码）"
  else
    echo "⛔ 承载门未过：存在 0 代码块的机制章（见上 P1 行）"
  fi
  return $rc
}

self_test() {
  local tmp; tmp=$(mktemp -d)
  # 夹具 1：机制章有码 → 应通过
  cat > "$tmp/good.md" <<'EOF'
# 标题
## 第 2 章 C 源码分析
```c
int f(void){return 1;}
```
EOF
  # 夹具 2：机制章零码、长文（>300 行）→ 应报 P1
  { echo "# 标题"; echo "## 第 2 章 C 源码分析"; for i in $(seq 1 320); do echo "填充行 $i 讲机制不讲代码"; done; } > "$tmp/bad.md"
  echo "[self-test] good 应过："; grade_doc "$tmp/good.md" || { echo "FAIL: good 被判不过"; return 1; }
  echo "[self-test] bad 应不过：";
  if grade_doc "$tmp/bad.md"; then echo "FAIL: bad 未被拦下"; return 1; else echo "OK: bad 被拦下"; fi
  rm -rf "$tmp"
  echo "SELF-TEST PASS"
}

case "${1:-}" in
  --self-test) self_test; exit $? ;;
  --check) shift; grade_doc "${1:?用法: doc-code-density-check.sh [--check] <doc.md>}" check; exit $? ;;
  ""|-h|--help) echo "用法: $0 [--check] <doc.md> | --self-test"; exit 0 ;;
  *) grade_doc "$1" report; exit $? ;;
esac
