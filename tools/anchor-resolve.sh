#!/usr/bin/env bash
# anchor-resolve.sh — 解析/校验文档中的符号锚点（todo_plan.md D5 改写落地）
#
# 锚点约定（2026-09-18，取代行号锚点；规则源 prompt/review-rules/review-process.md §Step 1.0）：
#   Rust : path:fn NAME | path:struct Name | path:enum Name | path:trait Name
#          | path:const NAME | path:static NAME | path:type Alias
#          同名方法歧义加 impl 限定：path:impl Type::method（迁移产物也接受 path:fn Type::method）
#          impl 块锚点：path:impl <签名子串>
#   C    : path.c:func | path.h:struct name | path:NAME（函数/结构体/宏，启发式）
#   行号 : 只允许工具派生后缀 `（Lnnn，工具生成）`，禁止手工维护
#
# 判定（与规则同步）：
#   0 定义 → ZERO-DEF（按 P0-fact 处理：符号消失/改名）
#   多定义 → MULTI-DEF（要求文档补 impl Type::method 限定，工具不加特例）
#
# 用法：
#   tools/anchor-resolve.sh --check DOC.md [DOC.md ...]   # 校验符号锚点（默认模式）
#   tools/anchor-resolve.sh --extract DOC.md              # 仅列出识别到的锚点（调试用）
#   tools/anchor-resolve.sh --self-test                   # 内置正/反例自测，exit 0 = 通过
#
# 退出码：0 = 全部锚点解析通过；1 = 存在 ZERO-DEF/MULTI-DEF；2 = 用法/环境错误
#
# 局限（有意为之，见 D5 执行注意）：
#   - C 侧用启发式（非缩进、非 ";" 结尾的 NAME( 行）；歧义一律报 MULTI-DEF，不加特例。
#   - 宏生成的 Rust 项可能 0 定义误报；按规则登记为"待补限定/待豁免"，不静默放过。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

MODE="check"
DOCS=()

usage() { echo "Usage: $0 --check DOC.md [...] | --extract DOC.md | --self-test" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)     MODE="check"; shift ;;
    --extract)   MODE="extract"; shift ;;
    --self-test) MODE="self-test"; shift ;;
    -h|--help)   usage ;;
    -*) echo "Unknown option: $1" >&2; usage ;;
    *) DOCS+=("$1"); shift ;;
  esac
done

# 转义正则元字符（名字限定为标识符+少量结构字符，其余按字面匹配）
esc_re() { printf '%s' "$1" | sed -e 's/[][\.*^$()+?{|}\\]/\\&/g'; }

# ---------------------------------------------------------- resolution 原语

# OK:<line> | ZERO | MULTI:<l1,l2,...>
verdict() { # $1 = 空格分隔的命中行号列表
  local hits="$1"
  hits="${hits% }"
  if [ -z "$hits" ]; then echo "ZERO"; return; fi
  local n
  n=$(printf '%s' "$hits" | wc -w)
  if [ "$n" -gt 1 ]; then echo "MULTI:$(printf '%s' "$hits" | tr ' ' ',')"; else echo "OK:$hits"; fi
}

resolve_rust_item() { # FILE KIND NAME
  local file="$1" kind="$2" name="$3"
  local n; n="$(esc_re "$name")"
  local pat
  case "$kind" in
    fn)     pat='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(default[[:space:]]+)?(const[[:space:]]+)?(unsafe[[:space:]]+)?(async[[:space:]]+)?(extern[[:space:]]+("[^"]*")?[[:space:]]+)?fn[[:space:]]+'"${n%%::*}"'($|[(<;[:space:]])' ;;
    struct) pat='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?struct[[:space:]]+'"${n%%::*}"'($|[(;{[:space:]:])' ;;
    enum)   pat='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?enum[[:space:]]+'"${n%%::*}"'($|[(;{[:space:]:])' ;;
    trait)  pat='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(unsafe[[:space:]]+)?(auto[[:space:]]+)?trait[[:space:]]+'"${n%%::*}"'($|[(;{[:space:]:])' ;;
    const)  pat='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?const[[:space:]]+'"$(esc_re "${n%%::*}")"'($|[:;=[:space:]])' ;;
    static) pat='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?static[[:space:]]+(mut[[:space:]]+)?'"$(esc_re "${n%%::*}")"'($|[:;=[:space:]])' ;;
    type)   pat='^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?type[[:space:]]+'"$n"'($|[=;[:space:]])' ;;
    *) echo "ZERO"; return ;;
  esac
  verdict "$(grep -nE "$pat" "$file" 2>/dev/null | cut -d: -f1 | tr '\n' ' ' || true)"
}

resolve_rust_method() { # FILE TYPE METHOD —— 在 impl Type 块内找 fn method
  local file="$1" typ="$2" method="$3"
  awk -v typ="$typ" -v method="$method" '
    {
      if ($0 ~ /^[[:space:]]*impl([^[:alnum:]_]|$)/) { in_impl = ($0 ~ typ) ? 1 : 0; depth = 0 }
      else if (in_impl) {
        if ($0 ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(const[[:space:]]+)?(unsafe[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]/) {
          line = $0
          sub(/^[[:space:]]*/, "", line)
          n = split(line, words, /[[:space:]]+/)
          for (i = 1; i <= n; i++) if (words[i] == "fn") { fname = words[i+1]; break }
          sub(/\(.*/, "", fname)
          if (fname == method) print NR
        }
      }
    }
  ' "$file" | tr '\n' ' '
}

resolve_rust_impl() { # FILE SIGNATURE-SUBSTRING
  local file="$1" sig="$2"
  grep -nE "^impl\b.*$(esc_re "$sig")" "$file" 2>/dev/null | cut -d: -f1 | tr '\n' ' ' || true
}

resolve_c() { # FILE NAME —— C 启发式（限定定义形态，排除注释行/宏调用/.S 宏使用）：
  #  1) #define NAME          2) struct|enum|union 行内含 NAME
  #  3) "返回类型 NAME(" 且不以 ";" 结尾；或跨行返回类型风格：NAME( 顶格且下一行以 "{" 开头
  #  .S/.asm 只认 "NAME:" 标签。NAME(...) 顶格且下一行非 "{" 视为宏调用，不算定义。
  local file="$1" name="$2"
  awk -v name="$name" '
    NR == FNR { lines[FNR] = $0; total = FNR; next }
    {
      line = $0
      if (line !~ ("(^|[^A-Za-z0-9_])" name "([^A-Za-z0-9_]|$)")) next
      if (line ~ /^[[:space:]]*(\/\*|\*)/) next
      if (line ~ ("^#[[:space:]]*define[[:space:]]+" name "([^A-Za-z0-9_]|$)")) { print FNR; next }
      if (FILENAME ~ /\.(S|asm)$/) {
        if (line ~ ("^" name ":")) { print FNR }
        next
      }
      if (line ~ ("^(typedef[[:space:]]+)?(struct|enum|union)([^A-Za-z0-9_]|$)")) { print FNR; next }
      if (line ~ ("^[^[:space:]].*[^A-Za-z0-9_]" name "[[:space:]]*[(]") && line !~ /;[[:space:]]*$/) { print FNR; next }
      if (line ~ ("^" name "[[:space:]]*[(]") && line !~ /;[[:space:]]*$/ && FNR < total && lines[FNR + 1] ~ /^[[:space:]]*[{]/) { print FNR; next }
    }
  ' "$file" "$file" | tr '
' ' '
}

# ---------------------------------------------------------- extraction

# 输出 "DOC\tLINE\tANCHOR"；fenced 代码块（``` / ~~~）内不识别（是代码内容，不是引用）
extract_anchors() {
  local doc="$1"
  awk -v doc="$doc" '
    BEGIN { infence = 0 }
    /^[[:space:]]*(```|~~~)/ { infence = !infence; next }
    infence { next }
    {
      line = $0
      gsub(/https?:\/\/[^ )`]+/, "", line)
      # 关键字形态：path:(fn|struct|enum|trait|const|static|type|impl) 后跟非空白
      s = line
      while (match(s, /[A-Za-z0-9_./-]+\.(rs|c|h|S|asm|ld):(fn|struct|enum|trait|const|static|type|impl)[[:space:]]+[^`（）()]+/)) {
        a = substr(s, RSTART, RLENGTH)
        sub(/[[:space:]]+$/, "", a)
        print doc "\t" NR "\t" a
        s = substr(s, RSTART + RLENGTH)
      }
      # C 裸名形态：path.c:NAME / path.h:NAME / path.S:NAME
      s = line
      while (match(s, /[A-Za-z0-9_./-]+\.(c|h|S):[A-Za-z_][A-Za-z0-9_]*/)) {
        a = substr(s, RSTART, RLENGTH)
        after = substr(s, RSTART + RLENGTH, 30)
        if (after !~ /^[[:space:]]*(fn|struct|enum|trait|const|static|type|impl)[[:space:]]/) {
          print doc "\t" NR "\t" a
        }
        s = substr(s, RSTART + RLENGTH)
      }
    }
  ' "$doc"
}

# ---------------------------------------------------------- self-test

do_self_test() {
  local ft="tools/.anchor-resolve-selftest"
  rm -rf "$ft"; mkdir -p "$ft"
  trap 'rm -rf "$ft"' EXIT

  cat > "$ft/lib.rs" <<'EOF'
pub const MAX_TICKS: u64 = 100;

pub struct Clock {
    pub ticks: u64,
}

impl Clock {
    pub fn reset(&mut self) {
        self.ticks = 0;
    }
}

pub struct Timer;

impl Timer {
    pub fn reset(&mut self) {
    }
}

pub fn clock_init() {
    let _ = MAX_TICKS;
}
EOF

  cat > "$ft/clock.c" <<'EOF'
#include <minix.h>

struct clock_state {
    int running;
};

#define CLOCK_FREQ 60

void clock_init(void)
{
    int x = CLOCK_FREQ;
}
EOF

  cat > "$ft/doc.md" <<'EOF'
正文引用 `ft/lib.rs:fn clock_init` 应解析。
带工具派生后缀 `ft/lib.rs:struct Clock（L6，工具生成）` 应解析。
impl 限定方法 `ft/lib.rs:impl Clock::reset` 应解析。
impl 块锚点 `ft/lib.rs:impl Clock` 应解析。
C 函数 `ft/clock.c:clock_init` 应解析。
fenced 块内的伪锚点不会被抓取：
```
ft/lib.rs:fn vanishing_in_fence
```
坏符号 `ft/lib.rs:fn vanishing_fn` 期望 ZERO-DEF。
裸同名方法 `ft/lib.rs:fn reset` 期望 MULTI-DEF（要求补 impl 限定）。
EOF

  # 关键：fixture 的路径前缀是 ft/，校验时从仓库根看是 tools/.anchor-resolve-selftest/…
  # 因此 --check 跑在临时 chdir 不可行；直接把 doc 里的 ft/ 前缀写成相对 PROJECT_ROOT 的路径再校验。
  sed -i "s|ft/|tools/.anchor-resolve-selftest/|g" "$ft/doc.md"

  local out rc=0
  out="$("$0" --check "$ft/doc.md" 2>&1)" && rc=0 || rc=$?

  if [ "$rc" -ne 1 ]; then
    echo "SELF-TEST FAIL: 期望 exit 1（存在 ZERO-DEF/MULTI-DEF），实际 exit $rc" >&2
    echo "$out" >&2
    exit 1
  fi
  local zero multi resolved fenced
  zero=$(echo "$out" | grep -c "ZERO-DEF" || true)
  multi=$(echo "$out" | grep -c "MULTI-DEF" || true)
  resolved=$(echo "$out" | grep -c "→ resolved" || true)
  fenced=$(echo "$out" | grep -c "vanishing_in_fence" || true)
  if [ "$zero" -ne 1 ] || [ "$multi" -ne 1 ] || [ "$resolved" -ne 5 ] || [ "$fenced" -ne 0 ]; then
    echo "SELF-TEST FAIL: resolved=$resolved zero=$zero multi=$multi fenced=$fenced（期望 5/1/1/0）" >&2
    echo "$out" >&2
    exit 1
  fi

  printf '无锚点的干净正文，应 exit 0。\n' > "$ft/clean.md"
  if ! "$0" --check "$ft/clean.md" >/dev/null 2>&1; then
    echo "SELF-TEST FAIL: 无锚点文档应 exit 0" >&2
    exit 1
  fi

  echo "SELF-TEST PASS（resolved=5 含 fenced 排除, ZERO-DEF=1, MULTI-DEF=1, 无锚点=exit0）"
  trap - EXIT
}

if [ "$MODE" = "self-test" ]; then do_self_test; exit 0; fi
[ ${#DOCS[@]} -gt 0 ] || usage

# ---------------------------------------------------------- main

# basename 索引（唯一名 → 全路径；重名不收录——与 anchor-migrate 同一唯一性守卫）
IDXFILE="$(mktemp)"
find minix3 os -type f \( -name '*.rs' -o -name '*.c' -o -name '*.h' -o -name '*.S' -o -name '*.asm' -o -name '*.ld' \) 2>/dev/null \
  | awk -F/ '{ bz=$NF; cnt[bz]++; if (cnt[bz] == 1) first[bz] = $0 } END { for (bz in cnt) if (cnt[bz] == 1) print bz "\t" first[bz] }' > "$IDXFILE"

grand_total=0; grand_resolved=0; grand_zero=0; grand_multi=0
tmp_all="$(mktemp)"
trap 'rm -f "$tmp_all" "$IDXFILE"' EXIT

for doc in "${DOCS[@]}"; do
  [ -f "$doc" ] || { echo "Not a file: $doc" >&2; exit 2; }
  extract_anchors "$doc" >> "$tmp_all"
done

if [ "$MODE" = "extract" ]; then cat "$tmp_all"; exit 0; fi

while IFS=$'\t' read -r doc dline anchor; do
  [ -n "$anchor" ] || continue
  grand_total=$((grand_total + 1))
  clean="$(printf '%s' "$anchor" | sed -e 's/（L[0-9]+，工具生成）.*$//' -e 's/`//g' -e 's/[[:space:]]*$//')"
  path="${clean%%:*}"
  rest="${clean#*:}"
  # 名字清洗：剥尾部非标识符字符（正则前瞻吃进的 , | ] 等），并裁到首个合法标识符
  case "$rest" in
    fn\ *|struct\ *|enum\ *|trait\ *|const\ *|static\ *|type\ *|impl\ *)
      kind="${rest%% *}"; nm="${rest#* }"
      ident="$(printf '%s' "$nm" | grep -oE '^[A-Za-z_][A-Za-z0-9_]*' || true)"
      if [ -z "$ident" ] || [ "$ident" = "$kind" ]; then
        echo "$doc:$dline: $anchor → SKIP（锚点名不是合法标识符：$nm）"
        grand_total=$((grand_total - 1))
        continue
      fi
      rest="$kind $ident"
      ;;
  esac

  file=""
  [ -f "$path" ] && file="$path"
  if [ -z "$file" ]; then
    # 裸文件名回退：minix3/ + os/ 全树唯一 basename 索引
    alt="$(awk -F'\t' -v b="$(printf '%s' "$path" | sed 's|.*/||')" '$1 == b { print $2 }' "$IDXFILE")"
    if [ -n "$alt" ]; then
      file="$alt"
      # 后续定位输出用解析后的真实路径
      clean="$(printf '%s' "$clean" | sed "s|^$path|$alt|")"
      path="$alt"
      rest="${clean#*:}"
    else
      echo "$doc:$dline: $anchor → ZERO-DEF（文件不存在：$path）→ 按 P0-fact 处理"
      grand_zero=$((grand_zero + 1))
      continue
    fi
  fi

  result=""; label=""
  case "$rest" in
    impl\ *)
      sig="${rest#impl }"
      if [[ "$sig" == *"::"* ]]; then
        hits="$(resolve_rust_method "$file" "${sig%%::*}" "${sig#*::}")"
        label="impl ${sig%%::*}::${sig#*::}"
      else
        hits="$(resolve_rust_impl "$file" "$sig")"
        label="impl $sig"
      fi
      [ -n "$hits" ] && result="$(verdict "$hits")" || result="ZERO"
      ;;
    fn\ *|struct\ *|enum\ *|trait\ *|const\ *|static\ *|type\ *)
      kind="${rest%% *}"; name="${rest#* }"
      if [[ "$file" == *.rs ]]; then
        if [[ "$name" == *"::"* ]]; then
          hits="$(resolve_rust_method "$file" "${name%%::*}" "${name#*::}")"
          [ -n "$hits" ] && result="$(verdict "$hits")" || result="ZERO"
        else
          result="$(resolve_rust_item "$file" "$kind" "$name")"
        fi
      else
        result="$(verdict "$(resolve_c "$file" "$name")")"
      fi
      label="$kind $name"
      ;;
    *)
      if [[ "$file" == *.rs ]]; then
        echo "$doc:$dline: $anchor → SKIP（Rust 裸名锚点不在校验范围，改用 fn/struct/... 前缀）"
        grand_total=$((grand_total - 1))
        continue
      fi
      result="$(verdict "$(resolve_c "$file" "$rest")")"
      label="$rest (C)"
      ;;
  esac

  case "$result" in
    OK:*)
      grand_resolved=$((grand_resolved + 1))
      echo "$doc:$dline: $anchor → resolved @ $path:${result#OK:}（$label）"
      ;;
    ZERO)
      grand_zero=$((grand_zero + 1))
      echo "$doc:$dline: $anchor → ZERO-DEF（符号不存在/已改名：$label in $path）→ 按 P0-fact 处理"
      ;;
    MULTI:*)
      grand_multi=$((grand_multi + 1))
      echo "$doc:$dline: $anchor → MULTI-DEF（${path} 行 ${result#MULTI:}）→ 补 impl Type::method 限定（$label）"
      ;;
  esac
done < "$tmp_all"

echo "----"
echo "anchors=$grand_total resolved=$grand_resolved zero-def=$grand_zero multi-def=$grand_multi"
[ "$grand_zero" -eq 0 ] && [ "$grand_multi" -eq 0 ] && exit 0
exit 1
