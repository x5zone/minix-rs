#!/usr/bin/env bash
# unsafe-audit.sh — unsafe / SAFETY 审计（todo_plan.md D2 落地）
#
# 目标：让每个 unsafe 构造都有安全辩护，新增裸 unsafe 被增量门拦截；存量冻结进基线，只登记不修。
#
# 判定规则（启发式，已知局限见下）：
#   - 枚举 `unsafe {` / `unsafe fn` / `unsafe impl` / `unsafe extern`
#   - 辩护判定：同一行或前 5 行内出现 `// SAFETY:`（兼容 `// Safety:`）→ 有辩护；
#     否则记为**裸 unsafe**
#   - #[cfg(test)] / tests/ 目录 / mock 内的 unsafe 计入统计但不进裸清单默认输出（--all 输出全部）
#
# 已知局限（有意的，基线收纳误报）：
#   - 多行注释、宏展开内的 unsafe、字符串字面量里的 "unsafe" 均可能误报
#   - 不追求零误报；误报通过基线收纳，不特判
#   - --diff 的辩护判定需要前 5 行上下文：命中行会按工作树补读 CTX（因此对已提交的
#     RANGE 跑 --diff 时，若那几行后来被改过，辩护可能读到新内容——与 --report 同一局限）
#
# 用法：
#   tools/unsafe-audit.sh --report                     # 全量统计 + 裸 unsafe 清单
#   tools/unsafe-audit.sh --diff [RANGE]               # 只查新增/修改行（默认 HEAD；增量门）
#   tools/unsafe-audit.sh --baseline FILE --update-baseline   # 产出/更新基线
#   tools/unsafe-audit.sh --baseline FILE --report     # 报告 + 与基线差值
#   tools/unsafe-audit.sh --self-test
#
# 退出码：--report 0=有输出（统计命令恒成功，除非环境错误 2）；--diff 0=无新增裸 unsafe，1=有；2=用法错误

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

MODE=""
RANGE="HEAD"
BASELINE=""
UPDATE_BASELINE=0
SHOW_ALL=0

usage() { echo "Usage: $0 --report [--all] | --diff [RANGE] | --baseline FILE [--update-baseline] | --self-test" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --report) MODE="report"; shift ;;
    --diff) MODE="diff"; if [ $# -gt 1 ] && [[ "$2" != -* ]]; then RANGE="$2"; shift; fi; shift ;;
    --baseline) BASELINE="$2"; MODE="report"; shift 2 ;;
    --update-baseline) UPDATE_BASELINE=1; MODE="report"; shift ;;
    --all) SHOW_ALL=1; shift ;;
    --self-test) MODE="self-test"; shift ;;
    -h|--help) usage ;;
    *) echo "Unknown arg: $1" >&2; usage ;;
  esac
done

PROG="$(mktemp)"
trap 'rm -f "$PROG"' EXIT

cat > "$PROG" <<'AWK'
# 输入：rg -n 输出（file:line:content）
{
  raw = $0
  if (raw == "" || raw == "\n") next
  if (raw ~ /^CTX\t/) {
    sub(/^CTX\t/, "", raw)
    pos = index(raw, ":"); f2 = substr(raw, 1, pos - 1); rest2 = substr(raw, pos + 1)
    pos2 = index(rest2, "\t"); l2 = substr(rest2, 1, pos2 - 1) + 0
    lines[f2, l2] = substr(rest2, pos2 + 1)
    next
  }
  # 拆 file:line:content（Windows 路径不存在，冒号分裂安全：file 无冒号）
  pos = index(raw, ":")
  file = substr(raw, 1, pos - 1)
  rest = substr(raw, pos + 1)
  pos2 = index(rest, ":")
  line = substr(rest, 1, pos2 - 1) + 0
  content = substr(rest, pos2 + 1)
  key = file ":" line
  if (seen[key]++) next

  total++
  # test/mock 归类
  is_test = (file ~ /\/tests?\// || content ~ /#\[cfg\(test\)\]/ || file ~ /\/mock/)
  # 辩护判定：本行或前 5 行（同文件）是否有 SAFETY
  has_safety = 0
  if (content ~ /\/\/[[:space:]]*[Ss][Aa][Ff][Ee][Tt][Yy]:/) has_safety = 1
  else {
    for (d = 1; d <= 5; d++) {
      pk = file ":" (line - d)
      if ((file, line - d) in lines && lines[file, line - d] ~ /\/\/[[:space:]]*[Ss][Aa][Ff][Ee][Tt][Yy]:/) { has_safety = 1; break }
    }
  }
  lines[file, line] = content
  if (has_safety) { with_safety++; next }
  bare++
  if (is_test && show_all == 0) { bare_test++; next }
  printf "BARE\t%s:%d\t%s\n", file, line, content
}
END {
  printf "STATS\ttotal=%d\twith_safety=%d\tbare=%d\tbare_in_test=%d\n", total, with_safety, bare, bare_test
}
AWK

# report 模式：unsafe 行 + 其前 5 行上下文一起输出；上下文行加 CTX 前缀由 awk 识别（不计数、只参与辩护）
collect_with_context() {
  local tmp_all; tmp_all="$(mktemp)"
  rg -n "unsafe[[:space:]]*(\{|fn|impl|extern)" os --glob '*.rs' 2>/dev/null > "$tmp_all" || true
  while IFS= read -r row; do
    [ -n "$row" ] || continue
    f="${row%%:*}"; rest="${row#*:}"; l="${rest%%:*}"
    start=$(( l > 5 ? l - 5 : 1 ))
    end=$(( l - 1 ))
    if [ "$end" -ge "$start" ]; then
      awk -v f="$f" -v s="$start" -v e="$end" 'NR >= s && NR <= e { printf "CTX\t%s:%d\t%s\n", f, NR, $0 }' "$f" 2>/dev/null
    fi
    printf '%s\n' "$row"
  done < "$tmp_all"
  rm -f "$tmp_all"
}

collect() { # 输出 rg -n 风格流
  if [ "$MODE" = "diff" ]; then
    # 只检查新增/修改行：git diff -U0 的 + 行，保留原行号
    local tmp_hits; tmp_hits="$(mktemp)"
    { git diff -U0 "$RANGE" -- 'os/**/*.rs'; git diff -U0 --cached -- 'os/**/*.rs'; } \
    | awk '
        /^\+\+\+ b\// { file = substr($2, 3); next }
        /^@@ -[0-9]+(,[0-9]+)? \+([0-9]+)/ {
          h = $3; sub(/^\+/, "", h); split(h, hp, ","); ln = hp[1] + 0; next }
        /^\+/ && !/^\+\+\+/ {
          content = substr($0, 2)
          if (content ~ /unsafe[[:space:]]*(\{|fn|impl|extern)/) print file ":" ln ":" content
          ln++
          next
        }
        /^[^-+ ]/ { next }
        /^ / { ln++ }
      ' > "$tmp_hits"
    # 为每条命中补前 5 行上下文（与 report 模式同一判定；不补则「辩护在上一行」的
    # 合法 union 读必被判裸——集中化访问器一类改动会被增量门假红）
    while IFS= read -r row; do
      [ -n "$row" ] || continue
      f="${row%%:*}"; rest="${row#*:}"; l="${rest%%:*}"
      [ -f "$f" ] || continue
      start=$(( l > 5 ? l - 5 : 1 ))
      end=$(( l - 1 ))
      if [ "$end" -ge "$start" ]; then
        awk -v f="$f" -v s="$start" -v e="$end" \
          'NR >= s && NR <= e { printf "CTX\t%s:%d\t%s\n", f, NR, $0 }' "$f"
      fi
    done < "$tmp_hits"
    cat "$tmp_hits"
    rm -f "$tmp_hits"
  else
    # 收集 unsafe 行 + 其前 5 行上下文（辩护判定需要上下文行，但上下文行不计入 unsafe 统计）
    collect_with_context
  fi
}

do_self_test() {
  local ft="tools/.unsafe-selftest"
  rm -rf "$ft"; mkdir -p "$ft/tests"
  trap 'rm -rf "$ft"' EXIT

  # 有辩护（同 5 行内）+ 无辩护 + test 内
  cat > "$ft/a.rs" <<'EOF'
// SAFETY: ptr is valid for the lifetime of the struct.
static A: i32 = unsafe { 1 };

static B: i32 = unsafe { 2 };

fn f() {
    // SAFETY:
    // argued elsewhere
    let _ = unsafe { 3 };
    let _ = unsafe { 4 };
}
EOF
  printf '#[cfg(test)]\nmod t {\n    #[test]\n    fn x() { let _ = unsafe { 5 }; }\n}\n' > "$ft/tests/b.rs"

  # 用 --diff 对临时文件不可行（diff 模式走 git）——直接测 collect 管道：模拟 rg 流
  local out
  out="$(printf '%s\n' \
    "$ft/a.rs:1:// SAFETY: ptr is valid for the lifetime of the struct." \
    "$ft/a.rs:2:static A: i32 = unsafe { 1 };" \
    "$ft/a.rs:5:static B: i32 = unsafe { 2 };" \
    "$ft/a.rs:8:    // SAFETY:" \
    "$ft/a.rs:9:    // argued elsewhere" \
    "$ft/a.rs:10:    let _ = unsafe { 3 };" \
    "$ft/a.rs:11:    let _ = unsafe { 4 };" \
    "$ft/tests/b.rs:4:    fn x() { let _ = unsafe { 5 }; }" \
    | awk -f "$PROG" -v show_all=0)"
  local bare_n stats
  bare_n=$(echo "$out" | grep -c "^BARE" || true)
  stats=$(echo "$out" | grep "^STATS")
  # a.rs:5 距 SAFETY(1) 为 4 行 → 有辩护；a.rs:10 距 8 为 2 行 → 有辩护；a.rs:11 距 8 为 3 → 有辩护；b.rs:4 test 内裸
  if [ "$bare_n" -ne 0 ]; then
    echo "SELF-TEST FAIL: 全部样本应有辩护或归 test，实际 BARE=$bare_n" >&2
    echo "$out" >&2
    exit 1
  fi
  echo "$stats" | grep -q "bare_in_test=1" || { echo "SELF-TEST FAIL: test 内裸计数不符：$stats" >&2; exit 1; }
  echo "SELF-TEST PASS（前 5 行辩护判定正确；test 内裸归类 bare_in_test）"
  trap - EXIT
  rm -rf "$ft"
}

if [ "$MODE" = "self-test" ]; then do_self_test; exit 0; fi
[ -n "$MODE" ] || usage

# -------------------------------------------------- 主流程
RAW="$(collect)"
if [ -n "$RAW" ]; then
  OUT="$(printf '%s\n' "$RAW" | awk -f "$PROG" -v show_all="$SHOW_ALL")"
else
  OUT="STATS\ttotal=0\twith_safety=0\tbare=0\tbare_in_test=0"
fi

echo "$OUT" | grep "^BARE" || true
echo "$OUT" | grep "^STATS" | sed 's/^STATS\t/统计：/'

stats_bare="$(echo "$OUT" | grep "^STATS" | sed 's/.*bare=\([0-9]*\).*/\1/')"

if [ "$MODE" = "diff" ]; then
  [ "${stats_bare:-0}" -eq 0 ] && exit 0
  exit 1
fi

# 基线
if [ -n "$BASELINE" ] && [ "$UPDATE_BASELINE" = "1" ]; then
  {
    echo "# unsafe-baseline — 由 tools/unsafe-audit.sh --update-baseline 生成（D2 存量冻结；只登记不修）"
    echo "# 格式：BARE<TAB>file:line<TAB>content"
    echo "$OUT" | grep "^BARE" || true
    echo "$OUT" | grep "^STATS"
  } > "$BASELINE"
  echo "基线已写入：$BASELINE（裸 unsafe 存量 $( [ "$SHOW_ALL" = 1 ] && echo "$stats_bare" || echo "$(echo "$OUT" | grep '^STATS' | sed 's/.*bare=\([0-9]*\)\tbare_in_test.*/\1/')" ) 处，冻结登记）"
fi

exit 0
