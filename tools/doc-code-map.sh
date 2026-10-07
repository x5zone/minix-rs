#!/usr/bin/env bash
# doc-code-map.sh — 从一篇文档抽取"关联 Rust 代码"清单并校验存在性（todo_plan.md B2 落地）
#
# 关联代码定义（B1，写入 review-doc-checklist §1）：
#   1. 文档头部声明字段 `> **Rust 实现**:`（兼容别名 `> **Rust 模块**:`；半角/全角冒号都认）
#      无关联代码的文档写 `> **Rust 实现**: 无（{一句理由}）`
#   2. 文档 §3（设计）与 §4（实现）中出现的 `os/` 开头 .rs 路径（反引号内）
#   3. 文档 §5（测试）中声明的测试文件或测试模块
#
# 用法：
#   tools/doc-code-map.sh DOC.md [--check] [--format=md|plain]
#     --check        逐条 test -f，缺失汇总输出；有缺失 exit 1
#     --format=md    Markdown 表格（默认，可直接粘贴进 scan.md 的 Step 0 段）
#     --format=plain 每行一条路径
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

usage() {
  # 显式请求帮助（-h/--help）→ 打到标准输出、退出码 0；参数缺失或未知选项 →  stderr、退出码 2
  if [ "${1:-}" = "--print0" ]; then echo "Usage: $0 DOC.md [--check] [--format=md|plain]" >&2; return 2; fi
  cat <<'HELP'
用法：tools/doc-code-map.sh DOC.md [--check] [--format=md|plain]

  DOC.md      受检文档路径（规范形态 rewrite-notes/{stage}/{doc}.md）
  --check     额外校验清单里的路径是否真实存在；有缺失则以退出码 1 结束
  --format    md（默认，Markdown 表格）| plain（制表符分隔，便于管道）
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
    -*) echo "Unknown option: $1" >&2; usage ;;
    *) [ -z "$DOC" ] && DOC="$1" || { echo "多余的参数: $1" >&2; usage --print0; exit 2; }; shift ;;
  esac
done
[ -n "$DOC" ] && [ -f "$DOC" ] || { usage --print0; exit 2; }

# -------------------------------------------------- 抽取
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

TMP="$(mktemp)"
trap 'rm -f "$TMP" "${TMP}.src" "${TMP}.hdr" "${TMP}.body"' EXIT

# 1) 头部声明字段（前 30 行内；兼容 Rust 实现 / Rust 模块；半角/全角冒号）
head -30 "$DOC" | grep -E '^>[[:space:]]*\*\*Rust (实现|模块)\*\*' | head -1 \
  > "${TMP}.src" || true
header_line="$(cat "${TMP}.src" || true)"
header_declared="no"
header_exempt="no"
if [ -n "$header_line" ]; then
  header_declared="yes"
  # 无（理由） 豁免形态
  if echo "$header_line" | grep -qE '无（|无\('; then header_exempt="yes"; fi
  echo "$header_line" | grep -oE '`os/[A-Za-z0-9_/.{},-]+\.rs`' | tr -d '`' | expand_braces >> "${TMP}.hdr" || true
fi
touch "${TMP}.hdr"

# 2) §3/§4/§5 分节抽取 os/*.rs 反引号 token（无分节的文档退化为全文抽取）
awk '
  /^#{1,3} [0-9]+(\.[0-9]+)?[[:space:]]/ { section = substr($2, 1, 1) }
  { if (section == "" || section == "3" || section == "4" || section == "5") print }
\' "$DOC" | grep -oE '`os/[A-Za-z0-9_/.{},-]+\.rs`' | tr -d '`' | expand_braces >> "${TMP}.body" || true
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
  grep -qxF "$p" "${TMP}.hdr" && src="头部声明"
  emit_row "$p" "$src"
done < "$TMP"

if [ "$count" -eq 0 ]; then
  if [ "$header_exempt" = "yes" ]; then
    echo "无关联代码（头部声明豁免）：$header_line"
    exit 0
  fi
  echo "❌ 无关联代码：文档没有头部 \`Rust 实现\` 字段，正文也没有 os/*.rs 引用（需人工确认理由或补头部声明）"
  exit 1
fi

if [ "$header_declared" = "no" ]; then
  echo "⚠️ 文档缺头部 \`> **Rust 实现**:\` 字段（以上清单来自正文抽取；新文档缺失该字段判 P1）" >&2
fi

if [ "$CHECK" = "1" ] && [ "$missing" -gt 0 ]; then
  echo "存在性校验：$missing/$count 缺失" >&2
  exit 1
fi
[ "$FMT" = "md" ] && echo "（共 $count 个文件；存在性 $((count - missing))/$count 通过）"
exit 0
