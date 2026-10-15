#!/usr/bin/env bash
# doc-code-map.sh — 从一篇文档抽取"关联 Rust 代码"清单并校验存在性（todo_plan.md B2 落地）
#
# 关联代码定义（2026-10-15 W-10 重定：清单收拢到 §7，抽取改为四级优先序合并去重）：
#   抽取四级（全部收集后 union 去重，不是首个命中即停）：
#     ① §7 语义小标题声明块  `### 关联 Rust 源码清单` / `### 关联 C 源码清单`（新主形态）
#     ② 头部 `> **Rust 实现**` / `> **Rust 模块**`（存量兼容，仍要求行首 `>`，避开正文段落小标题撞车）
#     ③ 正文 §3–§5 的 `os/*.rs` 引用
#     ④ 全文回退（③ 为空时；保留现状兜底）
#   外加：正文/§7 里的 Markdown 链接目标 `](os/**.rs)`（W-11 链接化后纯路径不得从清单蒸发）
#   token 规约：整段反引号取 token → 花括号展开 → 剥 `:符号` 尾 → 只留 `^os/.*\.rs$`。
#   非 `.rs`（`.sh`/`.ld`/目录簇）不进门的能力范围（判例：W-10 §93；与三基线口径一致，只增不减）。
#
# 用法：
#   tools/doc-code-map.sh DOC.md [--check] [--format=md|plain]
#     --check        逐条 test -f，缺失汇总输出；有缺失 exit 1
#     --format=md    Markdown 表格（默认，可直接粘贴进 scan.md 的 Step 0 段）
#     --format=plain 每行一条路径
#   tools/doc-code-map.sh --self-test   内置正/反例自测，exit 0 = 通过
#
# 退出码：0 = 清单产出且（--check 时）存在性全通过；1 = 无关联代码且未声明豁免，或 --check 有缺失；
#         2 = 用法/环境错误

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

DOC=""
CHECK=0
FMT="md"
MODE="map"

usage() {
  # 显式请求帮助（-h/--help）→ 打到标准输出、退出码 0；参数缺失或未知选项 →  stderr、退出码 2
  if [ "${1:-}" = "--print0" ]; then echo "Usage: $0 DOC.md [--check] [--format=md|plain] [--self-test]" >&2; return 2; fi
  cat <<'HELP'
用法：tools/doc-code-map.sh DOC.md [--check] [--format=md|plain] [--self-test]

  DOC.md      受检文档路径（规范形态 rewrite-notes/{stage}/{doc}.md）
  --check     额外校验清单里的路径是否真实存在；有缺失则以退出码 1 结束
  --format    md（默认，Markdown 表格）| plain（制表符分隔，便于管道）
  --self-test 跑内置正/反例自测
  -h, --help  显示本帮助，退出码 0

退出码：0 正常 ｜ 1 --check 发现不存在的路径 ｜ 2 用法错误
HELP
  return 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) CHECK=1; shift ;;
    --format=md) FMT="md"; shift ;;
    --format=plain) FMT="plain"; shift ;;
    -h|--help) usage; exit 0 ;;
    --self-test) MODE="self-test"; shift ;;
    -*) echo "Unknown option: $1" >&2; usage --print0; exit 2 ;;
    *) [ -z "$DOC" ] && DOC="$1" || { echo "多余的参数: $1" >&2; usage --print0; exit 2; }; shift ;;
  esac
done

# -------------------------------------------------- 花括号展开
# 花括号展开：`os/servers/pm/src/mproc/{wait,lifecycle,table}.rs` → 逐文件路径。
# 不展开会让关联代码清单漏掉花括号后的全部文件（2026-10-08 干跑取证：09-pm-exit/11-rs-publish 均受影响）。
expand_braces() {
  local line
  while IFS= read -r line; do
    if [[ "$line" == *"{"*"}"* ]]; then
      # local 同一句里的后续赋值看不到前面刚声明的变量（bash 先置空），必须分句，
      # 否则 set -u 下 ${rest} 报 unbound。
      local pre rest body post alt
      pre="${line%%\{*}"
      rest="${line#*\{}"
      body="${rest%%\}*}"
      post="${rest#*\}}"
      local IFS=','
      for alt in $body; do
        local joined="${pre}${alt}${post}"
        if [[ "$joined" == *"{"*"}"* ]]; then printf '%s\n' "$joined" | expand_braces
        else printf '%s\n' "$joined"; fi
      done
    else
      printf '%s\n' "$line"
    fi
  done
}

# -------------------------------------------------- token 抽取原语
# stdin：一段文本；stdout：逐个 os/**.rs 路径（已花括号展开、剥 `:符号`、只留 `.rs`）。
# 覆盖两种承载：反引号 token（可含簇）与 Markdown 链接目标 `](os/**.rs)`。
extract_os_tokens() {
  # 先把 stdin 缓存一份：`{ grep-A; grep-B; }` 两路都读同一管道时，A 会吃光输入、B 落空。
  local txt; txt="$(cat)"
  {
    # 反引号 span：内容必须以 os/ 开头才当路径 token（不抓散文里夹带的 os/ 片段，保持旧门语义）
    printf '%s\n' "$txt" | grep -oE '`[^`]+`' | tr -d '`' | grep -E '^os/' || true
    # Markdown 链接目标 ](os/**.rs)
    printf '%s\n' "$txt" | grep -oE '\]\(os/[^)]+\)' | sed -E 's/^\]\(//; s/\)$//' || true
  } | grep -oE '^os/[A-Za-z0-9_/.{},-]+' | expand_braces | sed -E 's/:.*$//' | grep -E '^os/.*\.rs$' || true
}

# 取 `### … <keyword> …` 小标题之下、下一 `##`/`###`/`####` 之前的区段。
extract_semantic_block() { # DOC KEYWORD
  awk -v kw="$2" '
    /^#{2,4}[[:space:]]/ { if (inb) exit; if (index($0, kw)) { inb = 1 }; next }
    inb { print }
  ' "$1"
}

if [ "$MODE" = "self-test" ]; then
  # ---------------------------------------------- 内置自测
  ft="tools/.doc-code-map-selftest"; rm -rf "$ft"; mkdir -p "$ft"
  # 夹具全部指向真实 os/ 文件，--check 才有存在性意义。
  cat > "$ft/clean.md" <<'EOF'
# fixture clean

> **Rust 实现**: `os/kernel/src/lib.rs`, `os/kernel/src/globals.rs`

## 3. 设计
`os/arch/src/x86_64/paging.rs`

## 4. 实现
`os/kernel-image/{check-layout.sh,src/main.rs}` 与 [`x`](os/boot-shim/src/main.rs)
EOF
  cat > "$ft/nocluster.md" <<'EOF'
# fixture nocluster

## 3. 设计
`os/kernel/src/lib.rs`
EOF
  cat > "$ft/exempt.md" <<'EOF'
> **Rust 实现**: 无（boot-shim 独立测试）
EOF
  cat > "$ft/bare.md" <<'EOF'
# fixture bare

正文段落小标题（无 `>` 前缀，不得当声明）：

**Rust 实现**

## 3. 设计
`os/kernel/src/lib.rs`
EOF
  cat > "$ft/ghost.md" <<'EOF'
> **Rust 实现**: `os/kernel/src/lib.rs`, `os/kernel/src/__ghost__.rs`
EOF
  fail=0
  # 1) 干净文档：exit 0 且抽取集合精确（5 条：头部 2 + §3 1 + 簇修复 main.rs + 链接目标 boot-shim main.rs）
  rc=0; out="$("$0" --check "$ft/clean.md" --format=plain 2>&1)" || rc=$?
  [ "$rc" -eq 0 ] || { echo "SELFTEST FAIL: clean 期望 exit 0，实际 $rc"; fail=1; }
  set_i="$(printf '%s\n' "$out" | grep -c '^os/' || true)"
  [ "$set_i" -eq 5 ] || { echo "SELFTEST FAIL: clean 期望 5 条，实际 $set_i"; printf '%s\n' "$out" | grep '^os/' >&2; fail=1; }
  for want in os/kernel/src/lib.rs os/kernel/src/globals.rs os/arch/src/x86_64/paging.rs os/kernel-image/src/main.rs os/boot-shim/src/main.rs; do
    printf '%s\n' "$out" | grep -qxF "$want" || { echo "SELFTEST FAIL: 缺 $want"; fail=1; }
  done
  # 簇修复必须把 main.rs 拉进来；.sh 不得混入（rs-only 门）
  if printf '%s\n' "$out" | grep -q 'check-layout.sh'; then echo "SELFTEST FAIL: .sh 不应进清单"; fail=1; fi
  # 2) 无簇回退：只 lib.rs
  rc2=0; out2="$("$0" "$ft/nocluster.md" --format=plain 2>&1)" || rc2=$?
  [ "$rc2" -eq 0 ] || { echo "SELFTEST FAIL: nocluster 期望 exit 0"; fail=1; }
  [ "$(printf '%s\n' "$out2" | grep -c '^os/' || true)" -eq 1 ] || { echo "SELFTEST FAIL: nocluster 期望 1 条"; fail=1; }
  # 3) 豁免：无（理由）→ exit 0 且不打缺字段告警
  rc3=0; out3="$("$0" "$ft/exempt.md" 2>&1)" || rc3=$?
  [ "$rc3" -eq 0 ] || { echo "SELFTEST FAIL: exempt 期望 exit 0，实际 $rc3"; fail=1; }
  if printf '%s\n' "$out3" | grep -q '缺'; then echo "SELFTEST FAIL: exempt 不该报缺字段"; fail=1; fi
  # 4) 正文段落小标题（无 `>`）不当声明：抽取正常但缺字段告警应触发
  rc4=0; out4="$("$0" "$ft/bare.md" --format=plain 2>&1)" || rc4=$?
  [ "$rc4" -eq 0 ] || { echo "SELFTEST FAIL: bare 期望 exit 0"; fail=1; }
  printf '%s\n' "$out4" | grep -qF 'os/kernel/src/lib.rs' || { echo "SELFTEST FAIL: bare 正文应抽到 lib.rs"; fail=1; }
  # 5) ghost：--check 期望 exit 1（剥过头/存在性门仍须咬住不存在的路径）
  if "$0" "$ft/ghost.md" --check --format=plain >/dev/null 2>&1; then echo "SELFTEST FAIL: ghost 期望 exit 1"; fail=1; fi
  # 6) 未知参数：期望 exit 2 且不挂起（S0 死循环回归保护）
  if timeout 5 "$0" --bogus >/dev/null 2>&1; then echo "SELFTEST FAIL: --bogus 不该成功"; fail=1; else rc6=$?; [ "$rc6" -eq 2 ] || { echo "SELFTEST FAIL: --bogus 期望 exit 2，实际 $rc6"; fail=1; }; fi
  rm -rf "$ft"
  if [ "$fail" -ne 0 ]; then echo "SELF-TEST FAIL doc-code-map"; exit 1; fi
  echo "SELF-TEST PASS doc-code-map（四级抽取/簇修复/链接目标/:符号剥离/豁免/正文小标题负例/ghost/未知参数）"
  exit 0
fi

[ -n "$DOC" ] && [ -f "$DOC" ] || { usage --print0; exit 2; }

# -------------------------------------------------- 抽取
TMP="$(mktemp)"
trap 'rm -f "$TMP" "${TMP}.hdr" "${TMP}.body" "${TMP}.sem"' EXIT

# ① §7 语义小标题声明块（Rust / C 两块，C 块只含 minix3 → 天然被 .rs+os/ 过滤掉）
extract_semantic_block "$DOC" "关联 Rust 源码清单" | extract_os_tokens > "${TMP}.sem" || true
extract_semantic_block "$DOC" "关联 C 源码清单"   | extract_os_tokens >> "${TMP}.sem" || true
[ -s "${TMP}.sem" ] && sem_declared="yes" || sem_declared="no"
cat "${TMP}.sem" >> "${TMP}.hdr"

# ② 头部声明字段（存量兼容；去掉 head -30/head -1，仍要求行首 `>`；兼容全/半角冒号）
header_line="$(grep -m1 -E '^>[[:space:]]*\*\*Rust (实现|模块)\*\*' "$DOC" || true)"
header_declared="no"
header_exempt="no"
if [ -n "$header_line" ]; then
  header_declared="yes"
  if echo "$header_line" | grep -qE '无（|无\('; then header_exempt="yes"; fi
  printf '%s\n' "$header_line" | extract_os_tokens >> "${TMP}.hdr" || true
fi
touch "${TMP}.hdr"

# ③ 正文 §3–§5 抽取；④ 为空时全文回退（保留现状兜底）
awk '
  /^#{1,3} [0-9]+(\.[0-9]+)?[[:space:]]/ { section = substr($2, 1, 1) }
  { if (section == "" || section == "3" || section == "4" || section == "5") print }
' "$DOC" | extract_os_tokens >> "${TMP}.body" || true
if [ ! -s "${TMP}.body" ]; then
  extract_os_tokens < "$DOC" >> "${TMP}.body" || true
fi
touch "${TMP}.body"

cat "${TMP}.hdr" "${TMP}.body" | sort -u > "$TMP"

# -------------------------------------------------- 输出
emit_row() { # path 来源
  local p="$1" src="$2" exists="是" note=""
  if [ ! -f "$p" ]; then exists="**缺失**"; note="test -f 失败"; fi
  if [ "$FMT" = "md" ]; then
    printf '| %s | %s | 实现 | %s | %s |\n' "$p" "$src" "$exists" "$note"
  else
    printf '%s\n' "$p"
  fi
}

if [ "$FMT" = "md" ]; then
  echo "### 关联代码清单（tools/doc-code-map.sh）"
  echo
  echo "| 路径 | 来源 | 类型 | 存在 | 备注 |"
  echo "|---|---|---|---|---|"
fi

missing=0
count=0
while IFS= read -r p; do
  [ -n "$p" ] || continue
  count=$((count + 1))
  if [ ! -f "$p" ]; then missing=$((missing + 1)); fi
  src="正文引用"
  grep -qxF "$p" "${TMP}.hdr" && src="声明字段"
  emit_row "$p" "$src"
done < "$TMP"

if [ "$count" -eq 0 ]; then
  if [ "$header_exempt" = "yes" ]; then
    echo "无关联代码（声明字段豁免）：$header_line"
    exit 0
  fi
  echo "❌ 无关联代码：文档既无头部 \`Rust 实现\` 字段、无 §7 关联源码清单，正文也没有 os/*.rs 引用（需人工确认理由或补声明）"
  exit 1
fi

if [ "$header_declared" = "no" ] && [ "$sem_declared" = "no" ]; then
  echo "⚠️ 文档缺 \`> **Rust 实现**\` 头部字段、也无 §7 \`### 关联 Rust 源码清单\` 声明块（以上清单来自正文抽取；新文档两者皆缺判 P1）" >&2
fi

if [ "$CHECK" = "1" ] && [ "$missing" -gt 0 ]; then
  echo "存在性校验：$missing/$count 缺失" >&2
  exit 1
fi
[ "$FMT" = "md" ] && echo "（共 $count 个文件；存在性 $((count - missing))/$count 通过）"
exit 0
