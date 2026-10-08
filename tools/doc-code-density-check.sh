#!/usr/bin/env bash
# doc-code-density-check.sh — 教学构件承载门 #9 机器读数（review-cmds §一 通用门 #9）
#
# 2026-10 二修（据 doc-commit-review bagging 的实测批评）：
#   ① 承载章按【内容】识别、不按标题词表：某 `## ` 章体内出现 C/Rust 符号锚点
#      （`...c:` / `...rs:`）即视为"该章分析/实现了 C 或 Rust 对象"→ 承载章。
#      改标题不再能躲门（词表法的老 bug）。
#   ② 语言感知围栏：只有 ```c / ```rust / ```cpp / ```asm / ```x86 算"贴码"；
#      ASCII 调用树（``` 无语言）、```text/```mermaid、表格都不算码（图≠码）。
#   ③ 降级键 = 该【章自身】行数 < 20（不是全文行数）。全文长短与"该不该给读者看码"无因果
#      ——doc-commit-review 共识：295 行干巴稿必须 FAIL。
#   ④ 阻断项只覆盖客观可判者：承载章 0 真码 = P1；诊断章（标题含 失败|诊断|定位|排障）
#      存在但 0 命令围栏 = P1。三架构/辨析/调用图 → 只出读数（交 review 判定/豁免），
#      不阻断（避免"严格文本无机器支持"→ 纯人判，doc-commit-review 冲突项的解）。
#   ⑤ 豁免：该章体内有 `<!-- doc-code-density: exempt reason=... -->` → 降为 report。
#   ⑥ 增量：默认 report 恒 exit 0（存量只报告，沿用 doc-style-lint/unsafe-audit 先例）；
#      --check 命中阻断项才 exit 1（供门/CI 用）。
#
# 用法：
#   tools/doc-code-density-check.sh [--check] <doc.md>
#   tools/doc-code-density-check.sh --self-test
set -euo pipefail

C_RUST_RE='^(c|rust|cpp|cc|cxx|asm|x86|s|arm|aarch64|riscv)\$\{0,1\}'  # info-string 前缀（awk 内用 index 判定）

# 按 `## ` 切章，逐章输出：标题 \t 章行数 \t 是否承载(1/0) \t C/Rust码围栏 \t 命令围栏 \t 任意围栏 \t 调用图(1/0) \t 三架构(1/0) \t 辨析带锚(数) \t 豁免(1/0) \t 是否诊断章
scan_chapters() {
  awk '
    function islang(l){ return (l=="c"||l=="rust"||l=="cpp"||l=="cc"||l=="cxx"||l=="asm"||l=="x86"||l=="arm"||l=="aarch64"||l=="riscv"||l=="s") }
    { sub(/\r$/,"") }   # CRLF 防御：剥行尾 \r，否则 ```c\r 会让 islang 失配
    function flush(){
      if (seen) printf "%s\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\n", title, lines, mech, codenf, bashnf, anyf, tree, triarch, bianxi, exempt, diag
    }
    /^## /{ flush(); title=substr($0,4); lines=0; mech=0; codenf=0; bashnf=0; anyf=0; tree=0; triarch=0; bianxi=0; exempt=0;
            diag = (title ~ /失败|诊断|定位|排障/) ? 1 : 0
            seen=1; infence=0; curbuf=""; next }
    { if(seen){
        lines++
        if ($0 ~ /(\.c:|\.rs:)/) mech=1
        if ($0 ~ /doc-code-density: exempt/) exempt=1
        if ($0 ~ /^```/){
          if (infence==0){
            infence=1; lang=$0; sub(/^```/,"",lang); sub(/[ \t].*/,"",lang); anyf++
            if (lang=="") curbuf="TREE?"                       # 无语言 → 可能是调用树/文本
            else if (lang=="bash"||lang=="sh") { bashnf++; curbuf="" }
            else if (islang(lang)) { codenf++; curbuf="" }
            else curbuf="OTHER"
            next
          } else {
            infence=0
            if (curbuf=="TREE?" && bufhastree) tree=1
            bufhastree=0
          }
          next
        }
        if (infence==1 && curbuf=="TREE?" && ($0 ~ /[├└→]/)) bufhastree=1
        # 三架构表：一行里同时出现 ≥3 个架构名 且像表格行
        if ($0 ~ /\|/) { a=0; if($0 ~ /x86/)a++; if($0 ~ /aarch64/)a++; if($0 ~ /riscv/)a++; if(a>=3) triarch=1 }
        # 辨析带源码锚点的 blockquote
        if ($0 ~ /^> / && $0 ~ /(\.c:|\.rs:)/) bianxi++
      } }
    END{ flush() }
  ' "$1"
}

run_doc() {
  local doc="$1" mode="${2:-report}" blocks=0
  echo "== doc-code-density-check: $doc  (模式：$mode)"
  printf '%-30s %5s %5s %5s %5s %5s %5s %5s %6s %6s %7s\n' 章 承载 码 令 树 架 辨 短 豁免 判定
  while IFS=$'\t' read -r title lines mech codenf bashnf anyf tree triarch bianxi exempt diag; do
    [ -z "${title:-}" ] && continue
    local short=0; (( lines < 20 )) && short=1
    local verdict="-"
    local t="${title}"; [ ${#t} -gt 30 ] && t="${t:0:30}…"
    if [ "$diag" = 1 ] && [ "$short" = 0 ] && [ "$exempt" = 0 ]; then
      # 诊断章优先按"命令围栏"判（它该给可复制命令，不一定有 C/Rust 码）
      if [ "$bashnf" -eq 0 ]; then verdict="P1 诊断缺命令"; blocks=$((blocks+1)); else verdict="OK 命令($bashnf)"; fi
    elif [ "$mech" = 1 ] && [ "$short" = 0 ] && [ "$exempt" = 0 ]; then
      if [ "$codenf" -eq 0 ]; then verdict="P1 承载缺码"; blocks=$((blocks+1)); else verdict="OK 码($codenf)"; fi
    elif [ "$mech" = 1 ] || [ "$diag" = 1 ]; then
      verdict="承载/诊断(短章或豁免,报告)"
    fi
    printf '%-30s %5s %5s %5s %5s %5s %5s %5s %6s %7s\n' "$t" "$mech" "$codenf" "$bashnf" "$tree" "$triarch" "$bianxi" "$short" "$exempt" "$verdict"
  done < <(scan_chapters "$doc")
  echo "---"
  # 读数小结（②③④ 交 review 判定，不阻断）
  echo "读数说明：承载章无 C/Rust 码、诊断章无命令 = 阻断(P1)；三架构/调用图/辨析 = 读数，缺失由 review 判定或登记豁免。"
  if [ "$blocks" -gt 0 ]; then
    echo "⛔ 承载门未过：$blocks 个阻断项"
    [ "$mode" = check ] && return 1 || return 0
  else
    echo "✅ 承载门阻断项通过（无零码承载章）"
    return 0
  fi
}

self_test() {
  local tmp; tmp=$(mktemp -d); local fail=0
  # 夹具 1：295 行干巴稿——承载章有 C 锚点、零码 → 必须 FAIL（旧版此处误判通过）
  { printf '# t\n## C 源码分析\n本节复刻 minix3/minix/kernel/arch/i386/pre_init.c:pre_init 的五步\n'
    for i in $(seq 1 292); do echo "填充讲解第 $i 行，不贴任何代码。"; done; } > "$tmp/dry295.md"
  # 夹具 2：承载章只有一张 ASCII 调用树（``` 无语言）→ 图不算码 → FAIL
  { printf '# t\n## C 源码分析\n见 minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity\n```\nhead.S\n  └─ pre_init → get_parameters\n```\n'
    for i in $(seq 1 30); do echo "行 $i"; done; } > "$tmp/treeonly.md"
  # 夹具 3：承载章有 ```c 真码 → PASS
  { printf '# t\n## C 源码分析\n锚 pre_init.c:pre_init\n```c\nint pre_init(){return 0;}\n```\n'
    for i in $(seq 1 25); do echo "行 $i"; done; } > "$tmp/good.md"
  check(){ if "$@"; then echo "  OK: $DESC"; else echo "  FAIL: $DESC"; fail=1; fi; }
  DESC="dry295 应被拦(exit1)"; { ! run_doc "$tmp/dry295.md" check >/dev/null; } && echo "  OK: $DESC" || { echo "  FAIL: $DESC"; fail=1; }
  DESC="treeonly 图不算码应被拦"; { ! run_doc "$tmp/treeonly.md" check >/dev/null; } && echo "  OK: $DESC" || { echo "  FAIL: $DESC"; fail=1; }
  DESC="good 有真码应过(exit0)"; run_doc "$tmp/good.md" check >/dev/null && echo "  OK: $DESC" || { echo "  FAIL: $DESC"; fail=1; }
  DESC="report 模式恒 exit0"; run_doc "$tmp/dry295.md" report >/dev/null && echo "  OK: $DESC" || { echo "  FAIL: $DESC"; fail=1; }
  rm -rf "$tmp"
  [ "$fail" = 0 ] && { echo "SELF-TEST PASS"; return 0; } || { echo "SELF-TEST FAIL"; return 1; }
}

case "${1:-}" in
  --self-test) self_test; exit $? ;;
  --check) shift; run_doc "${1:?用法: doc-code-density-check.sh [--check] <doc.md>}" check; exit $? ;;
  ""|-h|--help) echo "用法: $0 [--check] <doc.md> | --self-test"; exit 0 ;;
  *) run_doc "$1" report; exit $? ;;
esac
