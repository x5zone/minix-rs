#!/usr/bin/env bash
# doc-style-lint.sh — 正式文档"过程痕迹"检查（todo_plan.md A2 落地）
#
# 规则来源：prompt/review-rules/review-cmds.md §六 style-bible 第 7/8 条（A1.2）——
#   第 7 条：正式文档只面向读者，过程痕迹（review 编号/日期/修复史/写作策略元注释/流程工具术语）禁止入正文；
#   第 8 条：修复文档用假设性推理或直接正确表述重写，不追加"已修复（Px-y）"式补丁句。
# 存量策略（0.3 默认决策）：对存量只报告不阻断（exit 仍为 1 便于脚本感知，但 CI/门只对 --diff 阻断）；
#   对新增/修改行（--diff）阻断。
#
# 用法：
#   tools/doc-style-lint.sh FILE...            # 全量检查给定文件
#   tools/doc-style-lint.sh --diff [RANGE]     # 只检查新增/修改行；RANGE 传给 git diff（默认 HEAD）
#   tools/doc-style-lint.sh --dir DIR          # 检查目录下所有 .md（跳过 archive/draft/.design）
#   tools/doc-style-lint.sh --strict           # 额外启用 warning 级规则（默认只报 error 级）
#   tools/doc-style-lint.sh --self-test        # 内置正/反例自测，exit 0 = 自测通过
#
# 退出码：0 = 无 error 级命中；1 = 有 error 级命中；2 = 用法/环境错误
# 输出：{file}:{line}: [{ID}] {message} :: {匹配文本}；--diff 模式先输出被检范围摘要；末尾输出各 ID 计数汇总。
#
# 规则表（A2 任务卡）：
#   SL-1  error   V12-P1-2 式 review 批次编号
#   SL-2  error   FIX-20 式修复条目编号
#   SL-3  error   P1-1 / D-13 / R-05 / W-7 式 issue 编号（不含裸 P0/P1）
#   SL-4  error   正文日期 20XX-XX-XX（`> **创建**:` / `> **重写**:` 行豁免）
#   SL-5  error   修复史叙事（旧文档/旧版/已修复/修复前/原实现/旧实现/前版/原先；不含裸"最初/曾经"）
#   SL-6  error   写作策略元注释（本表不列/本文档不展开/不替…抢/论证归属/写作策略/文档维护者）
#   SL-7  error   review 工具术语（scan.md/STATE.md/VERIFY-CHECK/Pattern #N/模式 N/Gate X/反查维度）
#   SL-5b warning 最初/曾经/后来（合法的 C 历史叙述人工判断；仅 --strict 报告）
#   SL-8  warning 裸优先级词 P0/P1/P2（仅 --strict 报告）
#
# 实现约定与已知局限：
#   - fenced 代码块（``` / ~~~）内不检查（全量模式）；--diff 行级模式无围栏上下文，新增代码行
#     内出现类 token 可能误报——按报告人工确认
#   - 行内反引号包裹内容先剥离再匹配（`D-13` 作为代码标识符时合法）
#   - 词边界用 gawk 扩展 \< / \>（GNU Awk；POSIX awk 动态正则不支持 \b）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

STRICT=0
MODE="files"
RANGE="HEAD"
declare -A DIFF_LINES   # file -> "12 13 40"
TARGETS=()

usage() { echo "Usage: $0 [--strict] FILE... | --diff [RANGE] | --dir DIR | --self-test" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict) STRICT=1; shift ;;
    --diff) MODE="diff"; if [ $# -gt 1 ] && [[ "$2" != -* ]]; then RANGE="$2"; shift; fi; shift ;;
    --dir) MODE="dir"; shift ;;
    --self-test) MODE="self-test"; shift ;;
    -h|--help) usage ;;
    -*) echo "Unknown option: $1" >&2; usage ;;
    *) TARGETS+=("$1"); shift ;;
  esac
done

PROG="$(mktemp)"
trap 'rm -f "$PROG" "${PROG}.map" "${PROG}.body"' EXIT

cat > "$PROG" <<'AWK'
function strip_inline_code(line,   out, i, n, c, incode) {
  out = ""; incode = 0; n = length(line)
  for (i = 1; i <= n; i++) {
    c = substr(line, i, 1)
    if (c == "`") { incode = !incode; out = out " "; continue }
    out = out (incode ? " " : c)
  }
  return out
}
function match_one(clean, raw, pat, id, msg) {
  if (match(clean, pat)) {
    printf "%s:%d: [%s] %s :: %s\n", file, linenr, id, msg, substr(raw, RSTART, RLENGTH)
    hits[id]++
    if (id != "SL-5b" && id != "SL-8") err_hits++
  }
}
BEGIN {
  nfence = 0
  if (map != "") {
    n = split(map, pairs, " ")
    for (i = 1; i <= n; i++) {
      if (pairs[i] == "") continue          # map 前导空格产生的空段
      want[pairs[i] + 0] = 1
    }
  }
}
{
  raw = $0
  if (diffmode == 1) {
    if (!(NR in want)) next
    linenr = NR
  } else {
    linenr = NR
    if (nfence == 0 && raw ~ /^[[:space:]]*(```|~~~)/) { nfence = 1; next }
    else if (nfence == 1) { if (raw ~ /^[[:space:]]*(```|~~~)/) nfence = 0; next }
  }
  is_header_date = (raw ~ /^>[[:space:]]*\*\*(创建|重写)\*\*/)
  clean = strip_inline_code(raw)
  # awk 的 /regex/ 作实参会退化为布尔——必须用字符串动态正则（反斜杠双写）
  match_one(clean, raw, "\\<V[0-9]+-(P[0-9]+|A[0-9]+|T[0-9]+)(-[0-9]+)?\\>", "SL-1", "review 批次编号")
  match_one(clean, raw, "\\<FIX-[0-9]+\\>", "SL-2", "修复条目编号")
  match_one(clean, raw, "\\<P[0-9]+-[0-9]+\\>", "SL-3", "issue 编号")
  match_one(clean, raw, "\\bD-[0-9]+\\b", "SL-3", "issue 编号")
  match_one(clean, raw, "\\bR-[0-9]+\\b", "SL-3", "issue 编号")
  match_one(clean, raw, "\\bW-[0-9]+\\b", "SL-3", "issue 编号")
  if (!is_header_date) match_one(clean, raw, "20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]", "SL-4", "正文日期（头部创建/重写行豁免）")
  match_one(clean, raw, "旧文档|旧版|已修复|修复前|原实现|旧实现|前版|原先", "SL-5", "修复史叙事")
  match_one(clean, raw, "本表不列|本文档不展开|不替.{0,6}抢|论证归属|写作策略|文档维护者", "SL-6", "写作策略元注释")
  match_one(clean, raw, "scan\\.md|STATE\\.md|VERIFY-CHECK|Pattern #[0-9]+|模式 [0-9]+|Gate [0-9A-Z]|反查维度", "SL-7", "review 工具术语")
  if (strict == 1) {
    match_one(clean, raw, "最初|曾经|后来", "SL-5b", "可能的设计历史（人工判断）")
    match_one(clean, raw, "\\<P[0-9]\\>", "SL-8", "裸优先级词")
  }
}
END {
  for (k in hits) printf "SUMMARY\t%s\t%d\n", k, hits[k]
  if (err_hits > 0) printf "SUMMARY\tERROR_TOTAL\t%d\n", err_hits
}
AWK

run_lint() { # FILE [MAP] — MAP 形如 "1:12 2:13"（第 n 行输入 : 原文件行号）
  local file="$1" map="${2:-}"
  if [ -n "$map" ]; then
    awk -f "$PROG" -v file="$file" -v strict="$STRICT" -v diffmode=1 -v map="$map" "$file"
  else
    awk -f "$PROG" -v file="$file" -v strict="$STRICT" -v diffmode=0 "$file"
  fi
}

# -------------------------------------------------- self-test
do_self_test() {
  local ft="tools/.doc-style-selftest"
  rm -rf "$ft"; mkdir -p "$ft"
  trap 'rm -rf "$ft"' EXIT

  cat > "$ft/bad.md" <<'EOF'
r2 回归 review 中实施了 4 项卓越性改进（V12-P1-2）。
**A-3 v2（2026-08-16，todo P1-1）**：v1 用纯 bump，FIX-20 改为 free-list。
（**注意：旧文档误标为 0**，原实现将 X 写成 Y，2026-08-15 review 已修复，见 D-13。）
**本表不列的存储决策**……本文档不展开评估上述方向的工程细节。
产物见 scan.md 与 STATE.md，按 Pattern #66 与 Gate H 处理。
EOF

  cat > "$ft/clean.md" <<'EOF'
> **创建**: 2026-08-15
> **重写**: 2026-09-01
保护模式下的地址翻译依赖 `CR3` 寄存器（见 os/kernel/src/clock.rs:fn clock_init）。
`[ARCH: New, 三架构统一抽象]` 标注按项目约定三处一致。
交叉引用见 §3.2 与 15-clock-timer.md。
EOF

  cat > "$ft/history.md" <<'EOF'
Minix3 最初把进程表放在 BSS 段，后来 FS 也复用同一张表。
EOF

  local out rc=0
  out="$("$0" "$ft/bad.md" "$ft/clean.md" "$ft/history.md" 2>&1)" && rc=0 || rc=$?
  if [ "$rc" -ne 1 ]; then
    echo "SELF-TEST FAIL: 含违规样例应 exit 1，实际 $rc" >&2
    echo "$out" >&2
    exit 1
  fi
  if echo "$out" | grep -q "clean.md:"; then
    echo "SELF-TEST FAIL: 干净样例被误报：" >&2
    echo "$out" | grep "clean.md:" >&2
    exit 1
  fi
  if echo "$out" | grep "history.md:" | grep -q "SL-5b\|SL-5\]"; then
    echo "SELF-TEST FAIL: 合法历史叙述被默认级误报" >&2
    echo "$out" | grep "history.md:" >&2
    exit 1
  fi
  for id in SL-1 SL-2 SL-3 SL-4 SL-5 SL-6 SL-7; do
    echo "$out" | grep -q "\[$id\]" || { echo "SELF-TEST FAIL: $id 未命中" >&2; echo "$out" >&2; exit 1; }
  done

  local sout
  sout="$("$0" --strict "$ft/history.md" 2>&1 || true)"
  if ! echo "$sout" | grep -q "SL-5b"; then
    echo "SELF-TEST FAIL: --strict 下 SL-5b 未命中" >&2
    echo "$sout" >&2
    exit 1
  fi

  echo "SELF-TEST PASS（组1 全类命中；组2 零误报；组3 合法历史默认零命中、--strict 命中 SL-5b）"
  trap - EXIT
  rm -rf "$ft"
}

if [ "$MODE" = "self-test" ]; then do_self_test; exit 0; fi

# -------------------------------------------------- 目标收集
if [ "$MODE" = "dir" ]; then
  [ ${#TARGETS[@]} -eq 1 ] || usage
  mapfile -t DIRFILES < <(find "${TARGETS[0]}" -name '*.md' -type f \
    | grep -v '/\.design/' | grep -v '/archive' | grep -v '/draft' | sort)
  TARGETS=("${DIRFILES[@]}")
elif [ "$MODE" = "diff" ]; then
  while IFS=$'\t' read -r file lines; do
    DIFF_LINES["$file"]="${DIFF_LINES[$file]:-} $lines"
  done < <( { git diff -U0 "$RANGE" -- notes/rewrite; git diff -U0 --cached -- notes/rewrite; } \
    | awk '
        /^\+\+\+ b\// { file = substr($2, 3); next }
        /^@@ -[0-9]+(,[0-9]+)? \+([0-9]+)(,([0-9]+))? @@/ {
          hunk = $3
          start = substr(hunk, 2) + 0
          split(hunk, hparts, ",")
          cnt = (hparts[2] == "" ? 1 : hparts[2] + 0)
          if (cnt > 0) { out = ""; for (i = 0; i < cnt; i++) out = out " " (start + i); print file "\t" out }
        }
      ' )
  # 空关联数组在 set -u 下 ${#arr[@]} / ${!arr[@]} 都可能 unbound（bash<4.4）——统一用 ${arr[@]+…} 探测
  TARGETS=()
  if [ -n "${DIFF_LINES[*]+x}" ]; then TARGETS=("${!DIFF_LINES[@]}"); fi
  total=0
  if [ -n "${DIFF_LINES[*]+x}" ]; then
    for f in "${!DIFF_LINES[@]}"; do
      for l in ${DIFF_LINES[$f]}; do total=$((total + 1)); done
    done
  fi
  echo "diff 范围：${#TARGETS[@]} 个文件 / $total 个新增行（RANGE=$RANGE，限 notes/rewrite；规则文件不在增量门范围）" >&2
fi

if [ ${#TARGETS[@]} -eq 0 ]; then
  if [ "$MODE" = "diff" ]; then
    echo "无新增/修改的正式文档行，增量门通过（exit 0）" >&2
    exit 0
  fi
  echo "无检查目标" >&2
  exit 2
fi

# -------------------------------------------------- 执行（单次遍历，收集输出与汇总）
ALL_OUT="$(mktemp)"
err_total=0
for f in "${TARGETS[@]}"; do
  [ -f "$f" ] || { echo "Not a file: $f" >&2; exit 2; }
  if [ "$MODE" = "diff" ] && [ -n "${DIFF_LINES[$f]+x}" ]; then
    map="${DIFF_LINES[$f]}"
    run_lint "$f" "$map" >> "$ALL_OUT" || true
  else
    run_lint "$f" >> "$ALL_OUT" || true
  fi
done

grep -v "^SUMMARY" "$ALL_OUT" || true
awk '/^SUMMARY/ && $2 != "ERROR_TOTAL" { s[$2] += $3 }
     /^SUMMARY/ && $2 == "ERROR_TOTAL"  { e += $3 }
     END { for (k in s) printf "%s=%d ", k, s[k]; printf "| error级=%d\n", e+0 }' "$ALL_OUT" | sed 's/^/命中汇总：/'
awk '/^SUMMARY/ && $2 == "ERROR_TOTAL" { e += $3 } END { exit (e > 0) ? 1 : 0 }' "$ALL_OUT" || err_total=1

rm -f "$ALL_OUT"
[ "$err_total" = 0 ] && exit 0
exit 1
