#!/usr/bin/env bash
# doc-code-density-check.sh — 教学构件承载门 #9 机器读数（review-cmds §一 通用门 #9）
#
# 承载章按【内容】识别（章体内有 C/Rust 符号锚点），但**按标题排除非承载章型**
# （测试/概念/参见/附录/概述/目录/术语/导航/小结/总览——这些章引用代码锚点不代表"该贴节选码"）。
# 语言感知围栏：只有 ```c/rust/... 算"贴码"；ASCII 图（无语言）、表不算。
# 降级键 = 该【章自身】 < 20 行（不用全文行数）。
# 阻断（客观可判）：非排除型承载章 0 真码 = P1；非排除型诊断章 0 命令 = P1。
# 读数（不阻断，交 review 判定/豁免）：三架构 / 调用图 / 源码级辨析。
# 分母守卫：未识别到任何承载章 → 不判 ✅，打"需人工确认"。豁免章数过半 → WARN 泛滥。
# 增量：report 恒 exit0（存量只报告）；--check 命中阻断项才 exit1。
# 用法： tools/doc-code-density-check.sh [--check] <doc.md> | --self-test
set -euo pipefail

scan_chapters() {
  "${AWK:-awk}" '
    function islang(l){ return (l=="c"||l=="rust"||l=="cpp"||l=="cc"||l=="cxx"||l=="asm"||l=="x86"||l=="arm"||l=="aarch64"||l=="riscv"||l=="s"||l=="ld"||l=="lds") }
    { sub(/\r$/,"") }
    function flush(){
      if (seen) printf "%s\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\n", title, lines, mech, codenf, bashnf, anyf, tree, triarch, bianxi, exempt, diag, nontitle, expout
    }
    /^## /{ flush(); title=substr($0,4); lines=0; mech=0; codenf=0; bashnf=0; anyf=0; tree=0; triarch=0; bianxi=0; exempt=0;
            diag = (title ~ /失败|诊断|定位|排障/) ? 1 : 0
            expout = 0
            nontitle = (title ~ /测试|参见|附录|概述|概念|目录|术语|导航|小结|导览|速查|总览|全景/) ? 1 : 0
            seen=1; infence=0; curbuf=""; next }
    { if(seen){
        lines++
        if ($0 ~ /(\.c:|\.rs:|\.h:|\.S:|\.lds?:)/) mech=1
        if ($0 ~ /doc-code-density: exempt/) exempt=1
        # 诊断命令"预期输出"读数：非围栏行里出现 预期/expected/PASS/输出 关键提示
        if (infence==0 && $0 ~ /预期|expected|PASS|关键行|输出/) expout=1
        if ($0 ~ /^```/){
          if (infence==0){
            infence=1; lang=$0; sub(/^```/,"",lang); sub(/[ \t].*/,"",lang); anyf++
            if (lang=="") curbuf="TREE?"
            else if (lang=="bash"||lang=="sh") { bashnf++; curbuf="" }
            else if (islang(lang)) { codenf++; curbuf="" }
            else curbuf="OTHER"
            next
          } else { infence=0; if (curbuf=="TREE?" && bufhastree) tree=1; bufhastree=0 }
          next
        }
        if (infence==1 && curbuf=="TREE?" && ($0 ~ /[├└→]/)) bufhastree=1
        if ($0 ~ /\|/) { a=0; if($0 ~ /x86/)a++; if($0 ~ /aarch64/)a++; if($0 ~ /riscv/)a++; if(a>=3) triarch=1 }
        if ($0 ~ /^> / && $0 ~ /(\.c:|\.rs:)/) bianxi++
      } }
    END{ flush() }
  ' "$1"
}

run_doc() {
  local doc="$1" mode="${2:-report}" blocks=0 capable=0 total=0 exemptn=0
  echo "== doc-code-density-check: $doc  (模式：$mode)"
  printf '%-30s %4s %3s %3s %3s %3s %3s %4s %4s %8s\n' 章 承载 码 令 图 架 辨 短 豁免 判定
  while IFS=$'\t' read -r title lines mech codenf bashnf anyf tree triarch bianxi exempt diag nontitle expout; do
    [ -z "${title:-}" ] && continue
    total=$((total+1))
    local short=0; (( lines < 20 )) && short=1
    local t="${title}"; [ ${#t} -gt 30 ] && t="${t:0:30}…"
    local verdict="-"
    [ "$exempt" = 1 ] && exemptn=$((exemptn+1))
    if [ "$nontitle" = 1 ]; then
      verdict="非承载章型(报告)"
    elif [ "$short" = 1 ]; then
      [ "$mech" = 1 ] || [ "$diag" = 1 ] && verdict="承载/诊断(短章,报告)"
    elif [ "$exempt" = 1 ]; then
      verdict="豁免(报告)"
    else
      capable=$((capable+1))
      if [ "$diag" = 1 ]; then
        if [ "$bashnf" -eq 0 ]; then verdict="P1 诊断缺命令"; blocks=$((blocks+1)); else verdict="OK 命令($bashnf)"; fi
        [ "$bashnf" -gt 0 ] && [ "$expout" = 0 ] && verdict="$verdict ⚠无预期输出"
      elif [ "$mech" = 1 ]; then
        if [ "$codenf" -eq 0 ]; then verdict="P1 承载缺码"; blocks=$((blocks+1)); else verdict="OK 码($codenf)"; fi
      fi
    fi
    printf '%-30s %4s %3s %3s %3s %3s %3s %4s %4s %8s\n' "$t" "$mech" "$codenf" "$bashnf" "$tree" "$triarch" "$bianxi" "$short" "$exempt" "$verdict"
  done < <(scan_chapters "$doc")
  echo "---"
  echo "分母：承载/诊断章(可判) $capable / 全部章 $total；豁免 $exemptn。"
  [ "$exemptn" -gt 0 ] && [ $((exemptn*2)) -ge "$total" ] && echo "⚠ 豁免泛滥：$exemptn/$total 章标豁免——门形同虚设，请人工复核豁免理由。"
  echo "读数：三架构/调用图/源码辨析 = 读数项，缺失由 review 逐条判定或登记豁免（非自动阻断）。"
  if [ "$capable" -eq 0 ]; then
    echo "⚠ 未识别到可判承载/诊断章（标题均被排除或无内容锚点）→ 不判 ✅，请人工确认承载章型排除是否过宽。"
    return 0
  fi
  if [ "$blocks" -gt 0 ]; then
    if [ "$mode" = check ]; then echo "⛔ 承载门未过：$blocks 个阻断项"; return 1
    else echo "⚠ 发现 $blocks 个阻断候选（report 模式，仅报告；--check 才阻断）"; return 0; fi
  fi
  echo "✅ 承载门阻断项通过（$capable 个可判章均有码/命令）"
  return 0
}

self_test() {
  local tmp; tmp=$(mktemp -d); local fail=0
  { printf '# t\n## C 源码分析\n本节复刻 minix3/minix/kernel/arch/i386/pre_init.c:pre_init\n'
    for i in $(seq 1 292); do echo "填充 $i"; done; } > "$tmp/dry295.md"                       # 应拦
  { printf '# t\n## 第 5 章 测试要点\n覆盖 os/libs/minix-boot/src/kernel_info.rs:KernelInfo 与 boot_shim.rs:BootShim 的测试\n'
    for i in $(seq 1 40); do echo "测试点 $i"; done; } > "$tmp/testch.md"                       # 测试章型→应放行
  { printf '# t\n## C 源码分析\n锚 pg_utils.c:pg_identity\n```\nhead.S\n  └─ pre_init\n```\n'
    for i in $(seq 1 30); do echo "行 $i"; done; } > "$tmp/treeonly.md"                          # 图不算码→应拦
  { printf '# t\n## C 源码分析\n锚 pre_init.c:pre_init\n```c\nint x(){return 0;}\n```\n'
    for i in $(seq 1 25); do echo "行 $i"; done; } > "$tmp/good.md"                              # 真码→应过
  DESC="dry295 应拦"; run_doc "$tmp/dry295.md" check >/dev/null 2>&1 && { echo "  FAIL: $DESC"; fail=1; } || echo "  OK: $DESC"
  DESC="testch 测试章不误伤(放行)"; run_doc "$tmp/testch.md" check >/dev/null 2>&1 && echo "  OK: $DESC" || { echo "  FAIL: $DESC"; fail=1; }
  DESC="treeonly 图不算码应拦"; run_doc "$tmp/treeonly.md" check >/dev/null 2>&1 && { echo "  FAIL: $DESC"; fail=1; } || echo "  OK: $DESC"
  DESC="good 真码应过"; run_doc "$tmp/good.md" check >/dev/null 2>&1 && echo "  OK: $DESC" || { echo "  FAIL: $DESC"; fail=1; }
  DESC="report 模式恒 exit0"; run_doc "$tmp/dry295.md" report >/dev/null 2>&1 && echo "  OK: $DESC" || { echo "  FAIL: $DESC"; fail=1; }
  rm -rf "$tmp"
  [ "$fail" = 0 ] && { echo "SELF-TEST PASS"; return 0; } || { echo "SELF-TEST FAIL"; return 1; }
}

case "${1:-}" in
  --self-test) self_test; exit $? ;;
  --check) shift; run_doc "${1:?用法: doc-code-density-check.sh [--check] <doc.md>}" check; exit $? ;;
  ""|-h|--help) echo "用法: $0 [--check] <doc.md> | --self-test"; exit 0 ;;
  *) run_doc "$1" report; exit $? ;;
esac
