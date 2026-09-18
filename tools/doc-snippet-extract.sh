#!/usr/bin/env bash
# doc-snippet-extract.sh — 文档 Rust 代码块抽取（todo_plan.md H3 落地）
#
# 目的：把正式文档中的 ```rust / ```ignore / ```no_run 围栏代码块抽成清单
#   （{doc}:{start-end}:{lang}），供 review 逐块分类（逐字引用 / 教学简化 / 反面示例 / 签名示意）
#   与最佳实践判定。刻意保持简单：只做围栏抽取，不做解析判定——判定是 review 的事。
#
# 用法：
#   tools/doc-snippet-extract.sh DOC.md [DOC.md ...]   # 输出 {doc}:{start}-{end}:{lang} 清单
#   tools/doc-snippet-extract.sh --count DOC.md        # 只输出每文档块数
#   tools/doc-snippet-extract.sh --self-test
#
# 退出码：0 = 成功（无块也算成功）；2 = 用法/环境错误
# 输出格式：{doc}:{start}-{end}:{lang}（start/end 为文档行号，含围栏行）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

MODE="list"
TARGETS=()

usage() { echo "Usage: $0 [--count] DOC.md [...] | --self-test" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --count) MODE="count"; shift ;;
    --self-test) MODE="self-test"; shift ;;
    -h|--help) usage ;;
    -*) echo "Unknown option: $1" >&2; usage ;;
    *) TARGETS+=("$1"); shift ;;
  esac
done

do_self_test() {
  local ft="tools/.doc-snippet-selftest"
  rm -rf "$ft"; mkdir -p "$ft"
  trap 'rm -rf "$ft"' EXIT

  cat > "$ft/doc.md" <<'EOF'
前言正文。

```rust
fn main() { }
```

```ignore
fn broken() { }
```

```no_run
let x = 1;
```

```
plain fence 不算 rust 块
```

```text
text 围栏也不算
```

尾段正文。
EOF

  local out
  out="$("$0" "$ft/doc.md")"
  local n
  n=$(echo "$out" | grep -c ":rust$")
  local ni nn
  ni=$(echo "$out" | grep -c ":ignore$") || true
  nn=$(echo "$out" | grep -c ":no_run$") || true
  if [ "$n" -ne 1 ] || [ "$ni" -ne 1 ] || [ "$nn" -ne 1 ]; then
    echo "SELF-TEST FAIL: 期望 rust/ignore/no_run 各 1 块，实际 $n/$ni/$nn" >&2
    echo "$out" >&2
    exit 1
  fi
  if echo "$out" | grep -q ":$"; then
    echo "SELF-TEST FAIL: 无语言围栏不应输出" >&2
    exit 1
  fi
  echo "$out" | grep -qE "doc\.md:3-5:rust$" || { echo "SELF-TEST FAIL: 行号不符：$out" >&2; exit 1; }
  echo "SELF-TEST PASS（rust/ignore/no_run 各 1，plain/text 排除，行号含围栏）"
  trap - EXIT
}

if [ "$MODE" = "self-test" ]; then do_self_test; exit 0; fi
[ ${#TARGETS[@]} -gt 0 ] || usage

# 简化重写抽取逻辑（上面的 extract_doc 逻辑分支易错，改用清晰状态机）
extract_doc2() {
  local doc="$1"
  awk -v doc="$doc" '
    {
      line = $0
      rest = line
      sub(/^[[:space:]]+/, "", rest)
      if (rest ~ /^```/) {
        if (inside) {
          printf "%s:%d-%d:%s\n", doc, start, NR, lang
          inside = 0; lang = ""
        } else {
          lang = substr(rest, 4)
          sub(/[[:space:]]+$/, "", lang)
          if (lang == "rust" || lang == "ignore" || lang == "no_run") {
            inside = 1; start = NR
          } else {
            lang = ""
          }
        }
      }
    }
  ' "$doc"
}

for t in "${TARGETS[@]}"; do
  [ -f "$t" ] || { echo "Not a file: $t" >&2; exit 2; }
  if [ "$MODE" = "count" ]; then
    c="$(extract_doc2 "$t" | wc -l)"
    echo "$t blocks=$c"
  else
    extract_doc2 "$t"
  fi
done
exit 0
