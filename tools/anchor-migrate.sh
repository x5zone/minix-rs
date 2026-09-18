#!/usr/bin/env bash
# anchor-migrate.sh — 一次性行号锚点 → 符号锚点迁移（todo_plan.md D5 改写落地）
#
# 目标：把正式文档中的旧 `path:line` / `path:line-line` 锚点重写为符号锚点
#   （`path:fn NAME`、`path:struct Name`、C `path:func`），消灭行号漂移这类机械噪声。
#   行号只允许以工具派生后缀 `（Lnnn，工具生成）` 的形式保留在锚点内部。
#
# 迁移规则（与 anchor-resolve.sh 的解析语法一致）：
#   - 目标行本身就是定义行 → `path:kind NAME`
#   - 目标行在函数/impl 内部 → 就近向上找最近定义行，改写为 `path:kind NAME（L原行号，工具生成）`
#   - 找不到定义 / 文件不存在 → 列入"无法解析清单"（stderr），人工过一遍（工具不猜）
#   - fenced 代码块（``` / ~~~）内的行号是代码内容，一律不迁移
#
# 用法：
#   tools/anchor-migrate.sh [--write] DOC.md|DIR ...    # 默认 dry-run，逐条预览，不改写
#   tools/anchor-migrate.sh --stats-only DIR ...        # 只统计（不产出预览）
#   tools/anchor-migrate.sh --self-test
#
# 退出码：恒为 0（迁移与不可解析都以报告呈现；未解析清单需人工处理）
# 建议流程：--stats-only 拿全仓统计 → 单文档 dry-run 人工抽查 → --write 落地（git 兜底）。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

WRITE=0
STATS_ONLY=0
MODE=""
REPORT_FILE=""
TARGETS=()

usage() { echo "Usage: $0 [--write] [--stats-only] [--report FILE] DOC.md|DIR ... | --self-test" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --write) WRITE=1; shift ;;
    --stats-only) STATS_ONLY=1; shift ;;
    --report) REPORT_FILE="$2"; shift 2 ;;
    --self-test) MODE="self-test"; shift ;;
    -h|--help) usage ;;
    -*) echo "Unknown option: $1" >&2; usage ;;
    *) TARGETS+=("$1"); shift ;;
  esac
done

# ---------------------------------------------------------- self-test

do_self_test() {
  local ft="tools/.anchor-migrate-selftest"
  rm -rf "$ft"; mkdir -p "$ft"
  trap 'rm -rf "$ft"' EXIT

  cat > "$ft/lib.rs" <<'EOF'
pub const MAX_TICKS: u64 = 100;

pub struct Clock {
    pub ticks: u64,
}

pub fn clock_init() {
    // 内部第 1 行
    // 内部第 2 行
    let _ = MAX_TICKS;
}

pub fn clock_stop() {
}
EOF

  cat > "$ft/clock.c" <<'EOF'
#include <minix.h>

#define CLOCK_FREQ 60

void clock_init(void)
{
    int x = CLOCK_FREQ;
}

void clock_stop(void)
{
}
EOF

  cat > "$ft/doc.md" <<'EOF'
指向定义行：`ft/lib.rs:7` 应转为符号锚点（无后缀）。
指向函数内部：`ft/lib.rs:9` 应带（L9，工具生成）后缀。
C 定义行：`ft/clock.c:10` 应转为 C 符号锚点。
C 函数内部：`ft/clock.c:7` 应带（L7，工具生成）后缀。
fenced 块内的行号是代码内容，不迁移：
```
let x = ft/lib.rs:9;
```
文件缺失：`ft/vanished.rs:12` 期望进无法解析清单。
无定义可寻：`ft/lib.rs:99` 期望进无法解析清单（向上无定义）。
边界保真：（ft/vanished.h:28 extern）与 `ft/vanished.c:9` 的括号与反引号必须原样保留。
EOF
  sed -i "s|ft/|tools/.anchor-migrate-selftest/|g" "$ft/doc.md"

  local out
  out="$("$0" "$ft/doc.md" 2>/dev/null || true)"
  local conv unres
  conv=$(echo "$out" | grep -c "→ " || true)
  unres=$(echo "$out" | grep -oE "unresolved=[0-9]+" | head -1 | cut -d= -f2)
  if [ "$conv" -ne 4 ] || [ "$unres" -ne 4 ]; then
    echo "SELF-TEST FAIL: 期望 4 个迁移候选 + 4 个不可解析，实际 $conv / $unres" >&2
    echo "$out" >&2
    exit 1
  fi

  "$0" --write "$ft/doc.md" > /dev/null 2>&1
  local ok=1
  grep -q "tools/.anchor-migrate-selftest/lib.rs:fn clock_init\` 应转为" "$ft/doc.md" || ok=0
  grep -q "tools/.anchor-migrate-selftest/lib.rs:fn clock_init（L9，工具生成）\` 应带" "$ft/doc.md" || ok=0
  grep -q "tools/.anchor-migrate-selftest/clock.c:clock_stop\` 应转为" "$ft/doc.md" || ok=0
  grep -q "tools/.anchor-migrate-selftest/clock.c:clock_init（L7，工具生成）\` 应带" "$ft/doc.md" || ok=0
  grep -q "let x = tools/.anchor-migrate-selftest/lib.rs:9;" "$ft/doc.md" || ok=0
  grep -q "lib.rs:7" "$ft/doc.md" && ok=0
  # 边界保真：全角括号与空格、反引号闭合都不能被吞
  grep -q "（tools/.anchor-migrate-selftest/vanished.h:28 extern）" "$ft/doc.md" || ok=0
  grep -q "\`tools/.anchor-migrate-selftest/vanished.c:9\` 的括号" "$ft/doc.md" || ok=0
  if [ "$ok" -ne 1 ]; then
    echo "SELF-TEST FAIL: 写模式替换结果不符合预期：" >&2
    cat "$ft/doc.md" >&2
    exit 1
  fi

  echo "SELF-TEST PASS（dry-run 4 候选 + 4 不可解析；写模式替换正确、fenced 未动、边界字符保真）"
  trap - EXIT
}

if [ "$MODE" = "self-test" ]; then do_self_test; exit 0; fi
[ ${#TARGETS[@]} -gt 0 ] || usage

# ---------------------------------------------------------- 收集目标文档

DOCS=()
for t in "${TARGETS[@]}"; do
  if [ -f "$t" ]; then
    DOCS+=("$t")
  elif [ -d "$t" ]; then
    while IFS= read -r f; do DOCS+=("$f"); done < <(find "$t" -name '[0-9][0-9]-*.md' -type f \
      | grep -v '/\.design/' | grep -v '/archive' | sort)
  else
    echo "Not a file or dir: $t" >&2
    exit 2
  fi
done
[ ${#DOCS[@]} -gt 0 ] || { echo "No docs matched（目录模式只迁移 {NN}-*.md 编号文档）。" >&2; exit 2; }

# ---------------------------------------------------------- 迁移引擎

# basename 索引：文档常写裸文件名（clock.c:48），按 minix3/ + os/ 全树建立映射。
# ⛔ 唯一性守卫：重名裸文件名（如 kernel/priv.h vs include/minix/priv.h）不自动解析——
#    自动选第一个命中会迁到错误文件/错误符号（比不迁移更糟）；重名一律进未解析清单人工处理。
report="$(mktemp)"
IDXFILE="$(mktemp)"
trap 'rm -f "$report" "$IDXFILE"' EXIT
find minix3 os -type f \( -name '*.rs' -o -name '*.c' -o -name '*.h' -o -name '*.S' -o -name '*.asm' -o -name '*.ld' \) 2>/dev/null \
  | awk -F/ '{ bz=$NF; cnt[bz]++; if (cnt[bz] == 1) first[bz] = $0 } END { for (bz in cnt) if (cnt[bz] == 1) print bz "\t" first[bz] }' > "$IDXFILE"

migrate_doc() {
  local doc="$1"
  awk -v doc="$doc" -v write="$WRITE" -v stats_only="$STATS_ONLY" -v idxfile="$IDXFILE" -v rfile="$REPORT_FILE" '
    BEGIN {
      while ((getline il < idxfile) > 0) { split(il, iv, "\t"); IDX[iv[1]] = iv[2] }
      close(idxfile)
    }
    function isdef_rust(line) {
      if (line ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(default[[:space:]]+)?(const[[:space:]]+)?(unsafe[[:space:]]+)?(async[[:space:]]+)?(extern[[:space:]]+("[^"]*")?[[:space:]]+)?fn[[:space:]]+[A-Za-z0-9_]/) return "fn"
      if (line ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?struct[[:space:]]+[A-Za-z0-9_]/) return "struct"
      if (line ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?enum[[:space:]]+[A-Za-z0-9_]/) return "enum"
      if (line ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(unsafe[[:space:]]+)?(auto[[:space:]]+)?trait[[:space:]]+[A-Za-z0-9_]/) return "trait"
      if (line ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?const[[:space:]]+[A-Za-z0-9_]/) return "const"
      if (line ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?static[[:space:]]+(mut[[:space:]]+)?[A-Za-z0-9_]/) return "static"
      if (line ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?type[[:space:]]+[A-Za-z0-9_]/) return "type"
      if (line ~ /^impl([^[:alnum:]_]|$)/) return "impl"
      return ""
    }
    function isdef_c(line) {
      if (line ~ /^#[[:space:]]*define[[:space:]]+[A-Za-z_]/) return "def"
      if (line ~ /^(typedef[[:space:]]+)?(struct|enum|union)([^[:alnum:]_]|$)/) return "def"
      if (line ~ /^[A-Za-z_].*\(/ && line !~ /;[[:space:]]*$/) return "def"
      if (line ~ /^static[[:space:]].*\(/ && line !~ /;[[:space:]]*$/) return "def"
      return ""
    }
    # 从 src 文件 from 行找最近定义；输出 "kind\tname\tdefline" 或空。
    # 查找顺序：from 行本身 → 属性/注释/空行前缀的小步下探（±1~3 行漂移高发，模式 75 实证）→ 向上回溯。
    function find_def(src, from,   i, j, line, jline, k, n, before) {
      for (i = from; i >= 1; i--) {
        if (!((src, i) in L)) break
        line = L[src, i]
        # 前缀行下探：from 行本身是属性/文档注释/空行时，真定义通常在其下方数行内
        if (i == from && (line ~ /^[[:space:]]*$/ || line ~ /^[[:space:]]*#\[/ || line ~ /^[[:space:]]*\/\/[/!]?/)) {
          j = from
          while (j <= from + 3) {
            if (!((src, j) in L)) break
            jline = L[src, j]
            if (jline ~ /^[[:space:]]*$/ || jline ~ /^[[:space:]]*#\[/ || jline ~ /^[[:space:]]*\/\/[/!]?/) { j++; continue }
            break
          }
          if (j > from && j <= from + 3 && (src, j) in L) {
            k = (src ~ /\.rs$/) ? isdef_rust(L[src, j]) : ((isdef_c(L[src, j]) != "") ? "c" : "")
            if (k != "") { from = j; line = L[src, j]; i = j }
          }
        }
        if (src ~ /\.rs$/) {
          k = isdef_rust(line)
          if (k != "") {
            if (k == "impl") {
              n = line
              sub(/^[[:space:]]*impl[[:space:]]+/, "", n)
              sub(/[[:space:]]*\{[[:space:]]*$/, "", n)
              return "impl\t" n "\t" i
            }
            n = line
            before = n
            sub("^.*[[:space:]]" k "[[:space:]]+", "", n)
            if (n == before) sub("^[[:space:]]*" k "[[:space:]]+", "", n)  # 关键字顶格（无前导空白）时贪婪式失配
            if (k == "static") sub(/^mut[[:space:]]+/, "", n)
            if (k == "fn") { sub(/\(.*/, "", n); sub(/<.*/, "", n) } else { sub(/[({;=:[:space:]\/\*].*/, "", n) }
            return k "\t" n "\t" i
          }
        } else {
          if (isdef_c(line) != "") {
            n = line
            if (n ~ /^#[[:space:]]*define[[:space:]]+/) {
              sub(/^#[[:space:]]*define[[:space:]]+/, "", n); sub(/[^A-Za-z0-9_].*$/, "", n)
            } else if (n ~ /^}/) {
              sub(/^}[[:space:]]*/, "", n); sub(/;.*/, "", n)
            } else if (n ~ /^(typedef[[:space:]]+)?(struct|enum|union)([^[:alnum:]_]|$)/) {
              sub(/^(typedef[[:space:]]+)?(struct|enum|union)[[:space:]]+/, "", n); sub(/[^A-Za-z0-9_].*$/, "", n)
            } else {
              if (match(n, /[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\(/)) {
                n = substr(n, RSTART, RLENGTH)
                sub(/[[:space:]]*\(.*/, "", n)
              } else {
                n = ""
              }
            }
            if (n == "" ) return ""
            return "c\t" n "\t" i
          }
        }
      }
      return ""
    }
    function load_src(path) {
      if (loaded[path]) return
      loaded[path] = 1
      local_n = 0
      while ((getline srcline < path) > 0) { local_n++; L[path, local_n] = srcline }
      close(path)
      Lcount[path] = local_n
    }
    function rewrite_line(line, docline,   out, s, tok, path, ln, hi, key, cached, kind, name, defline, m, newanchor, n, dres, dparts, mstart, mlen, rpath, une_reason, bnd, rs1, rs2, r1s, r1l, r2s, r2l) {
      out = ""
      s = line
      while (1) {
        bnd = ""
        # 最左优先、同位取长：RANGE 与 SINGLE 各自 match 后取起点更早者（同起点 RANGE 更长）
        rs1 = match(s, /[A-Za-z0-9_\/.-]+\.(rs|c|h|S|asm|ld):[0-9]+-[0-9]+/)
        if (rs1) { r1s = RSTART; r1l = RLENGTH }
        rs2 = match(s, /[A-Za-z0-9_\/.-]+\.(rs|c|h|S|asm|ld):[0-9]+([^0-9]|$)/)
        if (rs2) { r2s = RSTART; r2l = RLENGTH }
        if (!rs1 && !rs2) break
        if (rs1 && (!rs2 || r1s < r2s || (r1s == r2s))) {
          tok = substr(s, r1s, r1l)
          mstart = r1s; mlen = r1l
        } else {
          # 正则含 1 字符前瞻边界（无 lookahead 语法）——显式摘出，重建时原样回填
          tok = substr(s, r2s, r2l)
          if (tok !~ /[0-9]$/) {
            bnd = substr(tok, length(tok), 1)
            tok = substr(tok, 1, length(tok) - 1)
          }
          mstart = r2s; mlen = r2l
        }
        # match()/sub() 会覆盖 RSTART/RLENGTH——上面已保存 mstart/mlen，后续 substr 一律用保存值
        path = tok; sub(/:[0-9]+(-[0-9]+)?$/, "", path)
        ln = tok; sub(/^[^:]*:/, "", ln)
        hi = ln; sub(/-[0-9]+$/, "", hi)
        key = path ":" hi
        # 裸文件名回退：minix3/ + os/ 全树 basename 索引（唯一命中才采用，重名保持未解析）
        rpath = path
        if (Lcount[path] == 0 && (path in IDX)) rpath = IDX[path]
        if (!(key in defcache)) {
          load_src(rpath)
          if (Lcount[rpath] == 0) {
            defcache[key] = "\x00MISSING"
          } else {
            dres = find_def(rpath, hi + 0)
            defcache[key] = (dres == "") ? "\x00NODEF" : dres
          }
        }
        cached = defcache[key]
        if (cached == "\x00MISSING" || cached == "\x00NODEF") {
          une_reason = (cached == "\x00MISSING" ? "文件不存在" : "向上无定义行")
          printf "%s:%d: UNRESOLVED %s（%s）\n", doc, docline, tok, une_reason > "/dev/stderr"
          if (rfile != "") printf "%s:%d: UNRESOLVED %s（%s）\n", doc, docline, tok, une_reason >> rfile
          unresolved[tok] = 1
          out = out substr(s, 1, mstart - 1) tok bnd
          s = substr(s, mstart + mlen)
          continue
        }
        split(cached, dparts, "\t")
        kind = dparts[1]; name = dparts[2]; defline = dparts[3] + 0
        if (kind == "impl")      newanchor = rpath ":impl " name
        else if (kind == "c")    newanchor = rpath ":" name
        else                     newanchor = rpath ":" kind " " name
        if (defline != hi + 0)   newanchor = newanchor "（L" hi "，工具生成）"
        converted[tok] = newanchor
        if (write == 0 && stats_only == 0) {
          printf "%s:%d: %s → %s\n", doc, docline, tok, newanchor
        }
        m = substr(s, 1, mstart - 1)
        if (write == 1) { m = m newanchor bnd; s = substr(s, mstart + mlen) }
        else            { m = m tok bnd;       s = substr(s, mstart + mlen) }
        out = out m
      }
      return out s
    }
    BEGIN { infence = 0; total = 0 }
    /^[[:space:]]*(```|~~~)/ { infence = !infence; if (write == 1) print; next }
    infence { if (write == 1) print; next }
    {
      n = 0; tmp = $0
      while (match(tmp, /[A-Za-z0-9_\/.-]+\.(rs|c|h|S|asm|ld):[0-9]+(-[0-9]+)?/)) { n++; tmp = substr(tmp, RSTART + RLENGTH) }
      total += n
      if (write == 1) print rewrite_line($0, NR)
      else            rewrite_line($0, NR)
    }
    END {
      cn = 0; for (k in converted) cn++
      un = 0; for (k in unresolved) un++
      printf "%s\ttokens=%d\tconvertible=%d\tunresolved=%d\n", doc, total, cn, un > "/dev/stderr"
    }
  ' "$doc"
}

for doc in "${DOCS[@]}"; do
  if [ "$WRITE" -eq 1 ]; then
    tmpout="$(mktemp)"
    migrate_doc "$doc" 2> >(grep -E "UNRESOLVED|tokens=" >> "$report" || true) > "$tmpout"
    if [ "$STATS_ONLY" -eq 0 ]; then mv "$tmpout" "$doc"; else rm -f "$tmpout"; fi
  else
    migrate_doc "$doc" 2> >(grep -E "UNRESOLVED|tokens=" >> "$report" || true)
  fi
done

echo "----"
tokens=$(grep -oE "tokens=[0-9]+" "$report" | cut -d= -f2 | awk '{s+=$1} END {print s+0}')
conv=$(grep -oE "convertible=[0-9]+" "$report" | cut -d= -f2 | awk '{s+=$1} END {print s+0}')
unres=$(grep -oE "unresolved=[0-9]+" "$report" | cut -d= -f2 | awk '{s+=$1} END {print s+0}')
echo "docs=${#DOCS[@]} tokens=$tokens convertible=$conv unresolved=$unres mode=$( [ "$WRITE" -eq 1 ] && echo WRITE || echo DRY-RUN )"
exit 0
