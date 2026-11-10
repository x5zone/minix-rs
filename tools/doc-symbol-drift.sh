#!/usr/bin/env bash
# doc-symbol-drift.sh — 文档旧符号漂移检查（V3）
#
# 代码改动触及 pub 签名/结构体字段/常量时，文档里可能还留着旧符号名。
# 这个门把“旧串命中”变成可机械检查的 commit 门：
#
#   tools/doc-symbol-drift.sh <doc.md> --symbol map_kernel --symbol old_field
#   tools/doc-symbol-drift.sh <doc.md> --from-diff HEAD~1 --strict
#   tools/doc-symbol-drift.sh <doc.md> --stale-lines HEAD~1 --strict
#
# 退出码：0 = 文档里没有旧符号/陈旧行；1 = 有命中；2 = 用法/环境错误。
# `--from-diff` 从 `git diff <rev>..HEAD -- '*.rs'` 的删除行提取标识符，
# 过滤 Rust 关键字后逐个 grep 文档；默认只报告，`--strict` 才作为失败门。
# `--stale-lines`（V3.2，FACT-B-01 判例）抓**行为级改动**：`let _ =` 换成
# `match` 这类改动不删任何符号，标识符粒度扫不到。它提取 diff 删除行里
# 足够具体的整行，先确认该行已不在当前源码树（排除“只是移动”），再查文档
# 是否仍在逐字引用这行——命中即文档引用了源码里已不存在的代码。

set -euo pipefail

SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"

usage() {
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
}

doc=""
declare -a symbols=()
from_diff=""
stale_lines_rev=""
strict=0
self_test=0

while [ $# -gt 0 ]; do
  case "$1" in
    --symbol)
      [ $# -ge 2 ] || { echo "缺少 --symbol 参数" >&2; exit 2; }
      symbols+=("$2")
      shift 2
      ;;
    --from-diff)
      [ $# -ge 2 ] || { echo "缺少 --from-diff 参数" >&2; exit 2; }
      from_diff="$2"
      shift 2
      ;;
    --stale-lines)
      [ $# -ge 2 ] || { echo "缺少 --stale-lines 参数" >&2; exit 2; }
      stale_lines_rev="$2"
      shift 2
      ;;
    --strict)
      strict=1
      shift
      ;;
    --self-test)
      self_test=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      echo "未知参数: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [ -z "$doc" ]; then
        doc="$1"
      else
        echo "多余参数: $1" >&2
        exit 2
      fi
      shift
      ;;
  esac
done

run_self_test() {
  local tmp
  tmp="$(mktemp -d)"
  cat >"$tmp/hit.md" <<'EOF'
# 示例

旧接口 `map_kernel` 仍在这里。
EOF
  cat >"$tmp/miss.md" <<'EOF'
# 示例

新接口 `map_kernel_with_layout` 已替换旧接口。
EOF
  local rc=0
  if bash "$0" "$tmp/hit.md" --symbol map_kernel >/dev/null 2>&1; then
    echo "self-test FAIL: 旧符号命中未被拦截"
    rc=1
  elif ! bash "$0" "$tmp/miss.md" --symbol map_kernel >/dev/null 2>&1; then
    echo "self-test FAIL: 无旧符号被误报"
    rc=1
  else
    echo "self-test PASS: 旧符号命中被拦截，无命中通过"
  fi

  # --stale-lines 自测：临时 git 仓库，v1 含特征行、v2 删除它；
  # 引用该行的文档必须被拦，未引用的通过。
  local srepo
  srepo="$tmp/repo"
  mkdir -p "$srepo"
  git -C "$srepo" init -q
  git -C "$srepo" -c user.email=t@t -c user.name=t commit --allow-empty -q -m init 2>/dev/null || true
  printf 'fn keep() {}\nfn stale_demo_line() {\n    let distinctive_payload = compute(7);\n}\n' > "$srepo/lib.rs"
  git -C "$srepo" add lib.rs
  git -C "$srepo" -c user.email=t@t -c user.name=t commit -q -m v1
  printf 'fn keep() {}\nfn replacement_line() {\n    let another_payload = compute(9);\n}\n' > "$srepo/lib.rs"
  git -C "$srepo" -c user.email=t@t -c user.name=t commit -q -am v2
  cat >"$tmp/stale-hit.md" <<'EOF'
# 文档

    let distinctive_payload = compute(7);
EOF
  cat >"$tmp/stale-miss.md" <<'EOF'
# 文档

    let another_payload = compute(9);
EOF
  if ( cd "$srepo" && bash "$SELF" "$tmp/stale-hit.md" --stale-lines HEAD~1 --strict ) >/dev/null 2>&1; then
    echo "self-test FAIL: --stale-lines 陈旧行命中未被拦截"
    rc=1
  elif ! ( cd "$srepo" && bash "$SELF" "$tmp/stale-miss.md" --stale-lines HEAD~1 --strict ) >/dev/null 2>&1; then
    echo "self-test FAIL: --stale-lines 无命中被误报"
    rc=1
  else
    echo "self-test PASS: --stale-lines 陈旧行命中被拦截，无命中通过"
  fi
  rm -rf "$tmp"
  return "$rc"
}

if [ "$self_test" -eq 1 ]; then
  run_self_test
  exit $?
fi

if [ -z "$doc" ] || [ ! -f "$doc" ]; then
  echo "文档不存在: ${doc:-<未给出>}" >&2
  exit 2
fi

status=0

check_symbol() {
  local sym="$1"
  local hits
  if hits="$(grep -nwF -- "$sym" "$doc" 2>/dev/null)"; then
    echo "[FAIL] $doc 仍含旧符号 \`$sym\`:"
    printf '%s\n' "$hits" | sed 's/^/    /'
    status=1
  else
    echo "[PASS] $doc 不含旧符号 \`$sym\`"
  fi
}

for sym in "${symbols[@]}"; do
  check_symbol "$sym"
done

if [ -n "$from_diff" ]; then
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "--from-diff 需要 git 仓库" >&2
    exit 2
  fi
  declare -a removed=()
  while IFS= read -r sym; do
    case "$sym" in
      self|Self|super|crate|let|mut|pub|fn|struct|enum|impl|trait|const|static|type|use|mod|return|if|else|match|for|while|loop|in|as|where|unsafe|async|await|move|ref|dyn|true|false|None|Some|Ok|Err|usize|u8|u16|u32|u64|i32|i64|bool|str|String|Vec|Option|Result|Box|Arc|Rc|assert|debug_assert|panic|unwrap|expect|clone|into|from|new|default|main) continue ;;
    esac
    removed+=("$sym")
  done < <(
    git diff --unified=0 "$from_diff"..HEAD -- '*.rs' 2>/dev/null \
      | awk '/^-/ && !/^---/ {print}' \
      | grep -oE '\b[A-Za-z_][A-Za-z0-9_]{2,}\b' \
      | sort -u || true
  )
  if [ "${#removed[@]}" -eq 0 ]; then
    echo "[PASS] --from-diff 没有提取到旧标识符"
  else
    for sym in "${removed[@]}"; do
      if grep -qwF -- "$sym" "$doc" 2>/dev/null; then
        echo "[WARN] $doc 仍提到 diff 删除的标识符 \`$sym\`"
        if [ "$strict" -eq 1 ]; then
          status=1
        fi
      fi
    done
  fi
fi

if [ -n "$stale_lines_rev" ]; then
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "--stale-lines 需要 git 仓库" >&2
    exit 2
  fi
  # 提取删除行 → 归一化空白 → 过滤注释/括号/过短行；先确认该行已不在当前
  # 源码树（排除“只是移动”），再查文档是否仍在逐字引用。短而泛化的行
  # （如 `let x = 1;`）不在射程内——那是 doc-citation-check 逐行门的职责；
  # 这个网只抓足够具体、文档逐字照抄的已删行。
  stale_hits=0
  checked_lines=0
  while IFS= read -r raw; do
    norm="$(printf '%s' "$raw" | tr -s ' \t' ' ' | sed 's/^ //; s/ $//')"
    [ "${#norm}" -lt 18 ] && continue
    case "$norm" in
      "//"* | "/*"* | "*"*) continue ;;
    esac
    if git grep -qF -- "$norm" -- '*.rs' 2>/dev/null; then
      continue
    fi
    checked_lines=$((checked_lines + 1))
    if tr -s ' \t' ' ' < "$doc" | grep -qF -- "$norm"; then
      echo "[FAIL] $doc 仍逐字引用源码已删除的代码行:"
      printf '    %s\n' "$norm"
      stale_hits=$((stale_hits + 1))
    fi
  done < <(
    git diff --unified=0 "$stale_lines_rev" -- '*.rs' 2>/dev/null \
      | awk '/^-/ && !/^---/ { print substr($0, 2) }' \
      | sort -u
  )
  if [ "$stale_hits" -eq 0 ]; then
    echo "[PASS] --stale-lines 没有发现文档仍引用的已删代码行（候选 $checked_lines 行）"
  elif [ "$strict" -eq 1 ]; then
    status=1
  fi
fi

exit "$status"
