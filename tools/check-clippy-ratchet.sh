#!/usr/bin/env bash
# check-clippy-ratchet.sh — clippy 警告【计数】棘轮（PD-31 采乙「报告制 + 新增必须为零」的机械化件）
#
# 出处：rewrite-notes/coordination/PENDING-DECISIONS-3ARCH-PARITY.md PD-31（第十轮定稿：采乙）。
# 决策：把「人工对账 clippy_base.txt、新增必须为零」的棘轮纪律机械化进持续集成。存量按
#   「谁改哪块清哪块」的童子军纪律消化，清完的模块从报告名单摘除；全仓归零后再议翻回
#   `cargo clippy -D warnings`（PD-31 被否的甲案：一次性清存量在小团队长仓库从未成功且堵死改动）。
#
# 与 tools/clippy_base.txt 的分工：后者是【按位置 path:line:col】的人工对账清单，行号一动就
#   漂移、只能人判；本件是【按 crate 聚合的警告计数】棘轮——计数不随无关行号漂移，故可安全
#   机检。总数超过基线即 FAIL（有新增警告）；低于基线则提示 --update-baseline 收紧。
#
# 五元组元数据（PD-31 第二轮追加落地纪律：基线不得是裸数字，必须绑定可复现口径）：
#   工具链版本、目标三元组、特性开关、包集、clippy 版本（外加可复跑命令与指纹）。
#
# 用法:
#   tools/check-clippy-ratchet.sh                     # 跑 cargo clippy --workspace，与基线比对
#   tools/check-clippy-ratchet.sh LOGFILE            # 用给定的 clippy stderr 日志算数（CI/自检复用）
#   tools/check-clippy-ratchet.sh --update-baseline [LOGFILE]  # 用实测重写基线（刷五元组头）
#   tools/check-clippy-ratchet.sh --self-test         # 夹具正反例判别矩阵（不跑真 clippy）
# 退出码: 0=未超基线（或 update/self-test 成功） 1=有新增警告 2=用法/环境错误
set -u
export LC_ALL=C

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE_REL="tools/clippy-ratchet-baseline.txt"
BASE="$REPO_ROOT/$BASE_REL"
RUST_ROOT="$REPO_ROOT/os"

MODE="check"
LOGFILE=""
for a in "$@"; do
  case "$a" in
    --self-test)        MODE="selftest" ;;
    --update-baseline)  MODE="update" ;;
    -h|--help)          sed -n '1,22p' "$0"; exit 0 ;;
    --*)                echo "unknown option: $a" >&2; exit 2 ;;
    *)                  LOGFILE="$a" ;;
  esac
done

# count_warnings <logfile> —— 累加各编译单元的「generated N warnings」尾行，得工作区警告总数。
count_warnings() {
  grep -oE 'generated [0-9]+ warning' "$1" 2>/dev/null \
    | grep -oE '[0-9]+' | awk '{ s += $1 } END { print s + 0 }'
}

# verdict <current> <baseline> —— 棘轮判定：升=FAIL，降=SHRINK（提示收紧），平=PASS。
verdict() {
  local cur="$1" base="$2"
  if [ "$cur" -gt "$base" ]; then echo FAIL
  elif [ "$cur" -lt "$base" ]; then echo SHRINK
  else echo PASS; fi
}

baseline_total() { grep -vE '^#' "$1" | head -1 | tr -d '[:space:]'; }

run_clippy() { # -> clippy stderr 到 stdout
  ( cd "$RUST_ROOT" && cargo clippy --workspace ) 2>&1
}

# ---------------- self-test（不依赖真 clippy，纯夹具） ----------------
if [ "$MODE" = "selftest" ]; then
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/clippy-ratchet-selftest.XXXXXX")"
  trap 'rm -rf "$tmp"' EXIT
  fails=0
  check() { [ "$2" = "$3" ] || { echo "  FAIL: $1: got '$2' want '$3'"; fails=$((fails+1)); }; }

  # count_warnings：两条尾行 (3+2) 与一条 (0) 与空日志
  printf 'warning: `x` (lib) generated 3 warnings\nwarning: `y` (lib) generated 2 warnings\n' > "$tmp/a"
  check "count 3+2" "$(count_warnings "$tmp/a")" "5"
  : > "$tmp/empty"
  check "count empty" "$(count_warnings "$tmp/empty")" "0"
  printf 'no warnings here\n' > "$tmp/none"
  check "count no-trailer" "$(count_warnings "$tmp/none")" "0"

  # verdict 三态
  check "verdict equal=PASS"  "$(verdict 5 5)"  "PASS"
  check "verdict up=FAIL"     "$(verdict 6 5)"  "FAIL"
  check "verdict down=SHRINK" "$(verdict 4 5)"  "SHRINK"
  check "verdict zero-base"   "$(verdict 1 0)"  "FAIL"

  # baseline_total：忽略 # 头取首行数字
  printf '# meta\n# more\n42\n' > "$tmp/b"
  check "baseline_total skip-comments" "$(baseline_total "$tmp/b")" "42"

  if [ "$fails" -eq 0 ]; then
    echo "clippy-ratchet --self-test: all fixtures PASS (count/verdict/baseline parsing)"
    exit 0
  fi
  echo "clippy-ratchet --self-test: $fails fixture(s) FAILED" >&2
  exit 1
fi

# ---------------- 取得当前警告总数 ----------------
if [ -n "$LOGFILE" ]; then
  [ -f "$LOGFILE" ] || { echo "log file not found: $LOGFILE" >&2; exit 2; }
  cur="$(count_warnings "$LOGFILE")"
  src="(from $LOGFILE)"
else
  echo "running: cargo clippy --workspace  (this compiles the whole tree)"
  logtmp="$(mktemp "${TMPDIR:-/tmp}/clippy-run.XXXXXX")"
  run_clippy > "$logtmp" 2>&1
  rc=$?
  # clippy 非零退出=编译或 deny 级错误，棘轮不能"看不见"，直接判错
  if [ "$rc" -ne 0 ]; then
    echo "clippy exited $rc (build error or a deny-level lint) — ratchet cannot proceed:" >&2
    grep -E '^error' "$logtmp" | head -5 >&2
    cp "$logtmp" /tmp/clippy-ratchet-last-error.log 2>/dev/null || true
    rm -f "$logtmp"; exit 2
  fi
  cur="$(count_warnings "$logtmp")"
  src="(from cargo clippy --workspace)"
  cp "$logtmp" /tmp/clippy-ratchet-last.log 2>/dev/null || true
  rm -f "$logtmp"
fi

# ---------------- update-baseline：刷五元组元数据 + 总数 ----------------
if [ "$MODE" = "update" ]; then
  [ -n "$cur" ] || { echo "no count to write" >&2; exit 2; }
  toolchain="$(rustc --version 2>/dev/null)"
  clip="$(cargo clippy --version 2>/dev/null)"
  commit="$( cd "$REPO_ROOT" && git rev-parse --short HEAD 2>/dev/null || echo unknown )"
  date="$(date +%F)"
  {
    echo "# clippy 警告计数棘轮基线（PD-31 采乙）。首行数字=工作区警告总数；# 头=五元组元数据。"
    echo "# toolchain: ${toolchain}"
    echo "# clippy:    ${clip}"
    echo "# command:   cargo clippy --workspace   (in ${RUST_ROOT#"$REPO_ROOT"/}, target=all-host, features=default)"
    echo "# package-set: workspace members"
    echo "# captured:  ${date} @ ${commit}"
    echo "# rerun:     bash tools/check-clippy-ratchet.sh"
    echo "$cur"
  } > "$BASE"
  echo "updated $BASE_REL to total=$cur (${src})"
  exit 0
fi

# ---------------- check：与基线比对 ----------------
if [ ! -f "$BASE" ]; then
  echo "no baseline at $BASE_REL — run: tools/check-clippy-ratchet.sh --update-baseline" >&2
  exit 2
fi
base="$(baseline_total "$BASE")"
v="$(verdict "$cur" "$base")"
echo "clippy warnings: current=$cur baseline=$base ${src} -> $v"
case "$v" in
  FAIL)
    echo "PD-31 ratchet: ${cur} > ${base} — $((cur - base)) NEW warning(s). Fix them (or, only if a lint toolchain-bump legitimately re-counted, --update-baseline with justification)." >&2
    exit 1 ;;
  SHRINK)
    echo "PD-31 ratchet: warnings dropped to ${cur} — nice; tighten with --update-baseline to lock the gain." >&2
    exit 0 ;;
  *) exit 0 ;;
esac
