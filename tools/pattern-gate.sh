#!/usr/bin/env bash
# pattern-gate.sh — 历史教训模式回归检查（claim/C-61-zcode-glm）
#
# 每项检查对应的事故出处见：
#   notes/rewrite/fork-syscall-rewrite/PATTERN-SCAN-REPORT-20260923.md §4 检查映射表
# 实现范式照 tools/unsafe-audit.sh 与 tools/doc-style-lint.sh：
#   全量模式 = 存量对账（基线外发现即 FAIL）；--diff = 增量门（额外只看新增行）；
#   --update-baseline = 冻结当前 P7/P8/P12 存量；--self-test = 正反例判别矩阵。
#
# 用法:
#   tools/pattern-gate.sh                    # 全量检查（基线对账）
#   tools/pattern-gate.sh --diff [RANGE]     # 增量门（默认 RANGE=HEAD：工作树+暂存 vs HEAD）
#   tools/pattern-gate.sh --update-baseline  # 重写基线文件
#   tools/pattern-gate.sh --self-test        # 正反例判别矩阵（mktemp 夹具，跑完即删）
#   tools/pattern-gate.sh -v                 # 逐项明细
# 退出码: 0=通过（SKIP 不算失败） 1=有违规 2=用法错误
#
# 检查项（P 编号与目录文档 §4 一致）:
#   P1  gate   FIXLOG 完整性（标题行=1 + 无连续大段重复；.review 不入 git，缺席 SKIP）
#   P2  gate   防回归测试存在性清单（历史事故的防回归测试不得被静默删除）
#   P3  gate   权威常量派生（SELF/MIB_CALL_SYSCTL 禁字面量化石值 + 0x1800 pin 在位）
#   P4  gate   os/tests/*.rs ↔ Cargo.toml [[test]] 双向对账
#   P5  gate   冒烟契约 marker ↔ emit 点对账（T1 'entering scheduler' 漂移事故）
#   P6  gate   CI 跨架构 build 门存在性（必须 build 不能 check；防删门）
#   P7  gate+  退出码吞没（cargo/bash/sh ... || echo；基线外即 FAIL）
#   P8  gate+  跟踪文件 CRLF 入库（*.ld/*.sh/*.rs；基线外即 FAIL）
#   P9  gate   .gitignore 会话产物防线在位（.wt/、/tmp/*、!/tmp/nk4a/）
#   P10 report diff 新增 todo/new_todo 裸引 Fix #N（存量 500+ 不拦，只拦增量）
#   P11 diff   新增行 C 锚点 file.c:NNN 存在性与落窗抽查
#   P12 report 共享路径 asm/rdmsr 无架构门（M3.1 事故形态；基线对账）
#   P13 diff   新增行反引号全大写常量符号存在性抽查（虚构符号事故）
#   P14 gate+  RTS 裸 set/clear 绕过 rts_set/rts_unset（F10d 家族；基线对账）
#   P15 gate   vendored 依赖源码防线（.dockercargo 跟踪 0 + gitignore 条目；d6f176451 事故）
# 带 "+" 的检查基线豁免（tools/pattern-gate-baseline.txt，key=检查名|路径|cksum）。

set -u
export LC_ALL=C

BASELINE_REL="tools/pattern-gate-baseline.txt"
RANGE="HEAD"
DO_DIFF=0
DO_BASELINE=0
DO_SELFTEST=0
VERBOSE=0

usage() { sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --diff)
      DO_DIFF=1
      case "${2:-}" in
        ""|-*) ;;                       # 无 RANGE 或下一个是选项 → 默认 HEAD
        *) RANGE="$2"; shift ;;
      esac
      shift ;;
    --update-baseline) DO_BASELINE=1; shift ;;
    --self-test) DO_SELFTEST=1; shift ;;
    -v|--verbose) VERBOSE=1; shift ;;
    -h|--help) usage ;;
    *) echo "未知参数: $1" >&2; exit 2 ;;
  esac
done

PASS_CNT=0; FAIL_CNT=0; SKIP_CNT=0
NEW_BASELINE=""

if [ "$DO_SELFTEST" -eq 1 ]; then
  ROOT="$(mktemp -d "${TMPDIR:-/tmp}/pattern-gate-selftest.XXXXXX")"
  trap 'rm -rf "$ROOT"' EXIT
else
  ROOT="${PATTERN_GATE_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null)}"
  [ -n "$ROOT" ] || { echo "错误: 不在 git 仓库内且未设 PATTERN_GATE_ROOT" >&2; exit 2; }
fi

ok()   { printf '[%s] PASS  %s\n' "$1" "$2"; PASS_CNT=$((PASS_CNT+1)); }
skip() { printf '[%s] SKIP  %s\n' "$1" "$2"; SKIP_CNT=$((SKIP_CNT+1)); }
fail() { printf '[%s] FAIL  %s\n' "$1" "$2"; FAIL_CNT=$((FAIL_CNT+1)); }
detail() { [ "$VERBOSE" -eq 1 ] && printf '      %s\n' "$1" || true; }

cksum_of() { printf '%s' "$1" | cksum | cut -d' ' -f1; }
baseline_has() { [ -f "$ROOT/$BASELINE_REL" ] && grep -qxF "$1" "$ROOT/$BASELINE_REL" || return 1; }
baseline_add() { NEW_BASELINE+="$1"$'\n'; return 0; }

# --------------------------------------------------------------- P1 FIXLOG 完整性
# 事故：edge3 FIXLOG 被会话收尾时全量重复追加 32 遍膨胀到 68,552 行（new_edge4 §1 规则 5）。
check_p1() {
  local d="$1/.review/zcode" files f h mc bad=0 n=0
  [ -d "$d" ] || { skip P1 ".review/zcode 缺席（.review 不入 git，本检查仅主树有意义）"; return 0; }
  files=$(compgen -G "$d/edge*/FIXLOG.md" || true)
  [ -n "$files" ] || { skip P1 "无 edge*/FIXLOG.md"; return 0; }
  for f in $files; do
    n=$((n+1)); h=$(grep -c "^# " "$f")
    mc=$(uniq -c "$f" | awk '{print $1}' | sort -nr | head -1)
    if [ "$h" -ne 1 ]; then
      fail P1 "$(basename "$(dirname "$f")")/FIXLOG.md 标题行计数=$h（必须为 1；非 1 = 全量复制事故复发）"; bad=1
    elif [ "$mc" -gt 5 ]; then
      fail P1 "$(basename "$(dirname "$f")")/FIXLOG.md 最大连续重复行=$mc（>5 疑似整段重复追加）"; bad=1
    else
      detail "P1 $(basename "$(dirname "$f")") header=$h maxconsec=$mc"
    fi
  done
  [ "$bad" -eq 0 ] && ok P1 "$n 份 FIXLOG 标题唯一、无连续大段重复"
  return 0
}

# --------------------------------------------------------------- P2 防回归测试存在性
# 每个名字 = 一次历史事故的防回归测试（出处见目录文档 §4 P2 表）；被删即静默失守。
P2_TESTS=(
  test_sys_times_self_sentinel_authority_pin            # NK8  SELF=-2 化石值
  test_grantee_gate_any_authority_value                 # NS2  grant ANY 哨兵权威
  irq_entry_mirrors_user_frame_but_skips_kernel_origin  # NK4-A 迭代20 IRQ/tick 全量存帧
  test_pagefault_unknown_region_sigsegv_and_clears_park # NK4-A Task A noaddr 静默死锁收口
  test_vmctl_clear_page_fault_requeues_target           # NK2  RTS_UNSET 出队半缺失
  test_vm_mmap_stack_lanes_carry_above_4gib             # NS5-A u32 车道截断
  test_flags_for_priv_proc                              # C-28 srv_fork PRIV_PROC
  startup_nsh_bytes_are_the_frozen_template_per_loader_name  # M3.3 startup.nsh 字节门
  boot_module_order_matches_boot_shim_module_names      # NS8  装机清单三重锁
  test_step0_creates_rproctab_grant_over_wire_mirror    # NS2  rproctab 授权创建点
  test_cdev_ioctl_continuation_decodes_payload          # NS7-A `if status == 0` 恒假门
  test_lookup_label_hosted_is_none                      # NL4  driver-rt lookup_label
  test_feed_keyboard_byte_uses_full_table               # NL4  死缝销号
  test_sigsuspend_carries_mask_only                     # SIGACT C 化删死参
  test_sigaction_carries_sigreturn_stub                 # NS11 sigreturn 桩地址填真
  test_rs_init_birth_answered_with_ok                   # NS1  六服务器出生应答臂
  load_process_elf_two_images_same_va_no_collision      # NK1  per-process 根装载
  trampoline_address_is_nonzero                         # NL3  sigreturn trampoline
  test_dispatch_times_self_replacement                  # NK8  判别补强
  test_check_gic_madt_decision_table                    # C-38 GICR 空洞决策表
  test_phantom_seats_beyond_topology                    # NK4-C §1.118 幽灵 CPU（扫描界未钉 processors_count）
  test_open_request_wire_type_is_cdev_open_not_index    # NK4-C §1.105 消息号=枚举序数（第三发）
  test_stat_streams_struct_stat_through_copy_out        # NK4-C §1.106 stat 回复腿从不 copy_out
  test_do_fork_downgrades_parent_shared_pte             # NK4-C B48 fork 父侧 COW 写保护
  test_boot_params_validate_total_pages_mismatch        # NK4-C §1.111 位图容量 vs 记账双口径
  test_entity_fork_failure_reap_loop_exits_on_zero      # NK4-C §1.117 `.is_ok()` 误译 waitpid>0
  test_runetcrc_fork_failure_reap_loop_exits_on_zero    # NK4-C §1.117 同族（runcom 腿）
  classification_matches_the_expected_dfsc_table        # NK4-C §1.116 64 项 DFSC 期望表（空断言对分组写反照样过）
)
check_p2() {
  local root="$1" missing=0 t hits
  [ -d "$root/os" ] || { skip P2 "无 os/"; return 0; }
  for t in "${P2_TESTS[@]}"; do
    hits=$(grep -rn "fn $t" "$root/os" --include='*.rs' 2>/dev/null | wc -l)
    if [ "$hits" -eq 0 ]; then
      fail P2 "防回归测试被删/改名: fn $t（历史事故失去回归网；目录文档 §4 P2 表）"; missing=1
    fi
    detail "P2 fn $t -> $hits"
  done
  [ "$missing" -eq 0 ] && ok P2 "${#P2_TESTS[@]}/${#P2_TESTS[@]} 防回归测试全部在位"
  return 0
}

# --------------------------------------------------------------- P3 权威常量派生
# 事故族：SELF=-2（真值 31742，-2 恰为 SYSTEM 端点）、MIB_CALL_SYSCTL=0x600（权威 0x1800）
# ——双方 canned 各用各的常量恒绿（edge2 NL9 / C-59）。
check_p3() {
  local root="$1" bad=0 hits
  grep -qE 'pub const SELF: i32 = Endpoint::SELF\.0' \
    "$root/os/libs/minix-sys/src/syscall.rs" 2>/dev/null \
    || { fail P3 "minix-sys SELF 未从 Endpoint::SELF 派生（化石值 -2 复发？）"; bad=1; }
  grep -qE 'pub const MIB_CALL_SYSCTL: i32 = minix_types::MIB_SYSCTL' \
    "$root/os/libs/minix-sys/src/misc.rs" 2>/dev/null \
    || { fail P3 "minix-sys MIB_CALL_SYSCTL 未从 minix_types 权威派生（0x600 复发？）"; bad=1; }
  hits=$(grep -rnE 'const (SELF|ANY|NONE): i32 = -[23];' "$root/os" --include='*.rs' 2>/dev/null | wc -l)
  if [ "$hits" -ne 0 ]; then
    fail P3 "发现 $hits 处端点哨兵字面量（= -2/-3，必须 Endpoint::* 派生）:"; bad=1
    grep -rnE 'const (SELF|ANY|NONE): i32 = -[23];' "$root/os" --include='*.rs' | sed 's/^/      /'
  fi
  hits=$(grep -rnE 'MIB_CALL_SYSCTL[^=]*=[^=]*0x600' "$root/os" --include='*.rs' 2>/dev/null | wc -l)
  [ "$hits" -eq 0 ] || { fail P3 "发现 $hits 处 MIB_CALL_SYSCTL=0x600 字面量（权威 0x1800）"; bad=1; }
  hits=$(grep -rn 'assert_eq!(MIB_CALL_SYSCTL, 0x1800)' "$root/os" --include='*.rs' 2>/dev/null | wc -l)
  [ "$hits" -ge 1 ] || { fail P3 "MIB_CALL_SYSCTL 权威绝对值 pin（0x1800）不在位"; bad=1; }
  [ "$bad" -eq 0 ] && ok P3 "端点哨兵/MIB 调用号权威派生 + 0x1800 pin 在位"
  return 0
}

# --------------------------------------------------------------- P4 os/tests 对账
# 新 .rs 未声明 [[test]] = cargo 静默不编译 = 假覆盖；夹具断链曾致 minix-tests 整 crate E0046
# 且阻断 clippy 多 crate 会话（edge3 Fix #142 同域）。
check_p4() {
  local root="$1" ct="$1/os/tests/Cargo.toml" declared files orphan missing
  [ -f "$ct" ] || { skip P4 "无 os/tests/Cargo.toml"; return 0; }
  declared=$(awk '/^\[\[test\]\]/{ing=1; next} ing && /^name/{sub(/^name[ ]*=[ ]*"/,""); sub(/".*/,""); print; ing=0}' "$ct" | sort)
  files=$(find "$root/os/tests" -maxdepth 1 -name '*.rs' -exec basename {} .rs \; 2>/dev/null | sort)
  orphan=$(comm -13 <(printf '%s\n' "$declared") <(printf '%s\n' "$files") | sed '/^$/d')
  missing=$(comm -23 <(printf '%s\n' "$declared") <(printf '%s\n' "$files") | sed '/^$/d')
  [ -n "$orphan" ] && fail P4 "os/tests 下有 .rs 未声明 [[test]]（cargo 不编译 = 假覆盖）: $(echo $orphan | tr '\n' ' ')"
  [ -n "$missing" ] && fail P4 "Cargo.toml 声明了不存在的 [[test]]: $(echo $missing | tr '\n' ' ')"
  if [ -z "$orphan" ] && [ -z "$missing" ]; then
    ok P4 "$(printf '%s\n' "$files" | grep -c .) 个测试文件与 [[test]] 声明一一对应"
  fi
  return 0
}

# --------------------------------------------------------------- P5 冒烟 marker ↔ emit
# 事故：T1 契约串 'entering scheduler' 全仓无 emit 点，smoke 契约与代码漂移
#（edge1 FIXLOG 迭代7 追记，NK4-A C-3 排查发现）。
# 配对表: <marker>|<引用方脚本>|<emit 点 grep 范围>
P5_PAIRS=(
  "entering scheduler|os/qemu-tests/test-cmd-smoke.sh|os/kernel/src"
  "rc: minimal boot script marker|os/qemu-tests/test-cmd-smoke.sh|os/etc"
)
check_p5() {
  local root="$1" pair marker ref emit nhits bad=0
  for pair in "${P5_PAIRS[@]}"; do
    marker="${pair%%|*}"; rest="${pair#*|}"; ref="${rest%%|*}"; emit="${rest#*|}"
    if [ ! -f "$root/$ref" ]; then
      fail P5 "配对表过期：$ref 不存在（marker '$marker' 引用方被删？更新 P5_PAIRS）"; bad=1; continue
    fi
    if ! grep -qF "$marker" "$root/$ref"; then
      fail P5 "配对表过期：$ref 不再引用 '$marker'（判据被删/改名？更新 P5_PAIRS）"; bad=1; continue
    fi
    nhits=$(grep -rFl "$marker" "$root/$emit" 2>/dev/null | wc -l)
    if [ "$nhits" -eq 0 ]; then
      fail P5 "冒烟契约漂移：'$marker' 被 $ref 等待但 $emit 下无 emit 点（T1 事故形态）"; bad=1
    else
      detail "P5 '$marker' emit: $(grep -rFl "$marker" "$root/$emit" 2>/dev/null | head -1)"
    fi
  done
  [ "$bad" -eq 0 ] && ok P5 "${#P5_PAIRS[@]} 个冒烟 marker 均有 emit 点"
  return 0
}

# --------------------------------------------------------------- P6 CI 跨架构 build 门
# 事故类：x86 专属 asm 无架构门两次打断跨架构载体编译（9e115387e / ab79b40ba）；OQ-1 变异实证
# cargo check 不做 codegen、inline asm 校验在汇编期——门必须 build。
check_p6() {
  local root="$1" wf="$1/.github/workflows/qemu-tests.yml" bad=0
  [ -f "$wf" ] || { skip P6 "无 .github/workflows/qemu-tests.yml"; return 0; }
  grep -q 'cross-arch-kernel-build:' "$wf" || { fail P6 "CI 跨架构 build 门被删（job cross-arch-kernel-build）"; bad=1; }
  grep -q 'cargo build -p minix-kernel --target aarch64' "$wf" || { fail P6 "CI 门缺 aarch64 生产形态 build 腿"; bad=1; }
  grep -q 'cargo build -p minix-kernel --target riscv64' "$wf" || { fail P6 "CI 门缺 riscv64 生产形态 build 腿"; bad=1; }
  if grep -q 'cargo check -p minix-kernel' "$wf"; then
    fail P6 "CI 门退化成 cargo check（check 不做 codegen，inline asm 错误静默放过）"; bad=1
  fi
  [ "$bad" -eq 0 ] && ok P6 "cross-arch-kernel-build 门在位且用 build 非 check"
  return 0
}

# --------------------------------------------------------------- P7 退出码吞没
# 事故：run_all.sh 构建循环 `|| echo "(build failed)"` 吞退出码，跨架构编译断裂被改判 skip（M3.1）。
collect_p7() {
  # 相对路径跑（key 可移植）；排除本脚本（注释/夹具里的模式引用不是真实吞没点）
  ( cd "$1" && grep -rnE '(cargo|bash|sh) [^|]*\|\| *echo' os tools .github 2>/dev/null \
    | grep -v '/target/' | grep -v '^tools/pattern-gate.sh:' | sort -u )
}
check_p7() {
  local root="$1" line path content key known=0 new=0
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    path="${line%%:*}"
    content=$(printf '%s' "$line" | sed 's/^[^:]*:[0-9]*://')
    key="P7|$path|$(cksum_of "$content")"
    if baseline_has "$key"; then known=$((known+1)); detail "P7 已知(基线): $line"
    else fail P7 "新增退出码吞没（基线外）: $line"; new=$((new+1)); fi
    baseline_add "$key"
  done < <(collect_p7 "$root")
  [ "$new" -eq 0 ] && ok P7 "退出码吞没对账通过（存量 ${known} 处在基线）"
  return 0
}

# --------------------------------------------------------------- P8 CRLF 入库
# 事故：aarch64.ld 以 CRLF 入库，链接器容忍 → 构建全绿肉眼看不出（NK4B M3.2 评审复核）。
collect_p8() {
  git -C "$1" ls-files --eol | awk -F'\t' '$1 ~ /^i\/crlf/ && $2 ~ /\.(ld|sh|rs)$/ {print $2}'
}
check_p8() {
  local root="$1" f key known=0 new=0
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    key="P8|$f"
    if baseline_has "$key"; then known=$((known+1)); detail "P8 已知(基线): $f"
    else fail P8 "跟踪文件 CRLF 入库（基线外）: $f"; new=$((new+1)); fi
    baseline_add "$key"
  done < <(collect_p8 "$root")
  [ "$new" -eq 0 ] && ok P8 "CRLF 对账通过（存量 ${known} 处在基线，建议顺手转 LF）"
  return 0
}

# --------------------------------------------------------------- P9 .gitignore 防线
# 事故：会话产物 543 文件 +35.7 万行误入库（f1041b3f1 落盘分流，edge1 FIXLOG Fix #9 修2）。
check_p9() {
  local root="$1" gi="$1/.gitignore" bad=0
  [ -f "$gi" ] || { skip P9 "无 .gitignore"; return 0; }
  grep -q '^\.wt/' "$gi"       || { fail P9 ".gitignore 缺 .wt/（claim 工作树防线）"; bad=1; }
  grep -q '^/tmp/\*' "$gi"     || { fail P9 ".gitignore 缺 /tmp/*（会话产物防线）"; bad=1; }
  grep -q '^!/tmp/nk4a/' "$gi" || { fail P9 ".gitignore 缺 !/tmp/nk4a/（内核注释取证锚点例外）"; bad=1; }
  [ "$bad" -eq 0 ] && ok P9 "会话产物防线三条规则在位"
  return 0
}

# --------------------------------------------------------------- P10 FIXLOG 序号裸引
# 教训：并发共享 gitignored FIXLOG 撞号（#117×3/#118×2/#119×2…）；tracked 文档引序号不可移植
#（edge3 Fix #126 教训）。存量 500+ 处为历史账本行只报告；--diff 拦新增。
diff_added_lines() {
  local root="$1" range="$2"
  { git -C "$root" diff -U0 "$range" -- os tools notes .github 2>/dev/null
    git -C "$root" diff -U0 --cached -- os tools notes .github 2>/dev/null; } \
    | awk '/^\+\+\+ b\//{f=substr($0,7)} /^@@/{next} /^\+/{if (f != "") print f "\t" substr($0,2)}'
}
check_p10_diff() {
  local added="$1" n
  n=$(printf '%s\n' "$added" | awk -F'\t' '$1 ~ /(^|\/)todo\.md$|new_todo/ && $2 ~ /Fix #[0-9]+/' | wc -l)
  [ "$n" -eq 0 ] && { ok P10 "增量无新增 Fix #N 裸引"; return 0; }
  fail P10 "新增 $n 处 Fix #N 裸引 todo/new_todo（tracked 文档只引 commit 哈希，edge3 Fix #126 教训）"
}
report_p10() {
  local root="$1" n=0 f
  while IFS= read -r f; do n=$((n + $(grep -cE 'Fix #[0-9]+' "$root/$f"))); done \
    < <(git -C "$root" ls-files | grep -E '(^|/)todo\.md$|new_todo')
  printf '[P10] REPORT tracked todo/new_todo 现有 %s 处 Fix #N 裸引（存量不拦；新增由 --diff 拦）\n' "$n"
  return 0
}

# --------------------------------------------------------------- P11 C 锚点（diff 门）
# 事故族：C 对位指向不存在的 kernel/memory.c、krandom.c，行号越界七组（NK4B M4.4 五轮评审）。
# 同名多命中（i386/earm 变体）任一文件行数覆盖即通过。
declare -A P11_CANDS=()
p11_candidates() { # basename → 换行分隔的 minix3/ 相对路径列表（缓存）
  local base="$1"
  if [ -z "${P11_CANDS[$base]:-}" ]; then
    P11_CANDS[$base]=$(find "$ROOT/minix3" -name "$base" -type f 2>/dev/null | sed "s|$ROOT/||")
  fi
  printf '%s\n' "${P11_CANDS[$base]}"
}
check_p11() {
  local root="$1" tokfile="$2" tok base ln cands p lines bad=0 n=0
  [ -f "$tokfile" ] || { skip P11 "无增量 token"; return 0; }
  while IFS= read -r tok; do
    [ -n "$tok" ] || continue
    base="${tok%%:*}"; ln="${tok##*:}"
    case "$ln" in ''|*[!0-9]*) continue ;; esac
    n=$((n+1))
    if baseline_has "P11|$base"; then detail "P11 已知(基线): $base"; continue; fi
    cands=$(p11_candidates "$base")
    if [ -z "$cands" ]; then
      fail P11 "C 锚点文件不存在: $base（虚构/改名路径——kernel/memory.c、krandom.c 同族；若属目录文档引例，加 P11|$base 进基线）"; bad=1; continue
    fi
    ok_line=0
    while IFS= read -r p; do
      [ -n "$p" ] || continue
      lines=$(wc -l < "$ROOT/$p")
      [ "$ln" -le "$lines" ] && ok_line=1
    done <<EOF
$cands
EOF
    if [ "$ok_line" -eq 0 ]; then
      fail P11 "C 锚点行号越界: $tok（各变体均不足 $ln 行）"; bad=1
    fi
  done < "$tokfile"
  [ "$bad" -eq 0 ] && ok P11 "增量 C 锚点存在性/落窗通过（检查 $n 个 token）"
  rm -f "$tokfile"
  return 0
}

# --------------------------------------------------------------- P12 共享路径 asm/rdmsr 无架构门
# 事故：9e115387e / ab79b40ba——x86 rdmsr/asm 只带 mock 门不带 target_arch 门，aarch64 载体 17 错。
P12_FILES="os/kernel/src/trap_dispatch.rs os/kernel/src/lib.rs os/arch/src/arch/stacktrace.rs os/boot-shim/src/lib.rs"
collect_p12() {
  local root="$1" rel f ln content back
  for rel in $P12_FILES; do
    f="$root/$rel"
    [ -f "$f" ] || continue
    # 排除纯注释行（C-62 误报修正：doc 注释里的 naked_asm! 等词不是代码）；
    # 窗口 6→12 行（C-62 误报修正：块头 cfg 门可隔长注释块，如 lib.rs GS 采样探针）
    while IFS=: read -r ln content; do
      printf '%s' "$content" | grep -qE '^[[:space:]]*(//|/\*|\*)' && continue
      back=$(awk -v n="$ln" 'NR>=n-12 && NR<n' "$f" | grep -c 'target_arch' || true)
      [ "${back:-0}" -gt 0 ] && continue
      printf '%s:%s|%s\n' "$rel" "$ln" "$(printf '%s' "$content" | tr -s ' \t' ' ' | cksum | cut -d' ' -f1)"
    done < <(grep -nE 'asm!|rdmsr|wrmsr' "$f")
  done
}
check_p12() {
  local root="$1" row loc key known=0 new=0
  while IFS= read -r row; do
    [ -n "$row" ] || continue
    loc="${row%%|*}"
    key="P12|$loc|${row#*|}"
    if baseline_has "$key"; then known=$((known+1)); detail "P12 已知(基线): $loc"
    else fail P12 "共享路径 asm/rdmsr 无架构门（基线外）: $loc"; new=$((new+1)); fi
    baseline_add "$key"
  done < <(collect_p12 "$root")
  [ "$new" -eq 0 ] && ok P12 "共享路径 asm/rdmsr 架构门对账通过（存量 ${known} 处在基线）"
  return 0
}

# --------------------------------------------------------------- P13 反引号常量符号存在性（diff 门）
# 事故：设计文档与注释引用全仓不存在的 SRV_FORK_INHERIT_FLAGS（edge4 批A 附带发现 / Fix #124①）。
# PATTERN-SCAN-REPORT 自身豁免（目录文档合法引用事故引例）。
check_p13() {
  local root="$1" tokfile="$2" tok hits bad=0 n=0
  [ -f "$tokfile" ] || { skip P13 "无增量 token"; return 0; }
  while IFS= read -r tok; do
    [ -n "$tok" ] || continue
    n=$((n+1))
    baseline_has "P13|$tok" && continue
    hits=$(grep -rlF "$tok" "$ROOT/os" "$ROOT/minix3" 2>/dev/null | wc -l)
    if [ "$hits" -eq 0 ]; then
      fail P13 "新增行引用的全大写常量符号全仓不存在: $tok（虚构符号事故族；目录文档引例加 P13|$tok 进基线）"; bad=1
    fi
  done < "$tokfile"
  [ "$bad" -eq 0 ] && ok P13 "增量反引号常量符号存在性通过（检查 $n 个 token）"
  rm -f "$tokfile"
  return 0
}

# --------------------------------------------------------------- P14 RTS 裸操作（report+基线）
# 事故：F10d 家族三种变体——绕过 rts_set/rts_unset（含入队/出队半）裸 set/clear
# p_rts_flags → runnable 却不入调度队（privctl/clear_ipc_refs/do_trace，NK4C-RESUME-PROMPT §9.8）。
# collect 只抓 proc.rs（helper 定义/实现域）之外的 kernel 文件；存量对账基线，新增即抓人审。
collect_p14() {
  local root="$1"
  ( cd "$root" && grep -rnE 'p_rts_flags\.(set|clear|insert|remove)\(' os/kernel/src \
    --include='*.rs' 2>/dev/null | grep -v '^os/kernel/src/proc.rs:' | sort -u )
}
check_p14() {
  local root="$1" line path content key known=0 new=0
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    path="${line%%:*}"
    content=$(printf '%s' "$line" | sed 's/^[^:]*:[0-9]*://')
    key="P14|$path|$(cksum_of "$content")"
    if baseline_has "$key"; then known=$((known+1)); detail "P14 已知(基线): $line"
    else fail P14 "RTS 裸 set/clear 绕过 rts_set/rts_unset（基线外；核对入队/出队半，F10d 家族）: $line"; new=$((new+1)); fi
    baseline_add "$key"
  done < <(collect_p14 "$root")
  [ "$new" -eq 0 ] && ok P14 "RTS 裸操作对账通过（存量 ${known} 处在基线）"
  return 0
}

# --------------------------------------------------------------- P15 误提交防线（vendored/依赖源码）
# 事故：d6f176451 `git add -A os/` 误将 os/.dockercargo/registry/（158 文件 4.7MB crates.io
# 依赖源码）提交入库且 .gitignore 无条目（NK4C R3 评审 §一 唯一卫生违规）。
check_p15() {
  local root="$1" bad=0 n=0
  if git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
    n=$(git -C "$root" ls-files 'os/.dockercargo/' 2>/dev/null | wc -l)
    [ "$n" -eq 0 ] || { fail P15 "vendored 依赖源码被跟踪（git rm -r --cached os/.dockercargo + gitignore）：${n} 文件"; bad=1; }
  else
    skip P15 "非 git 仓库（self-test 沙箱），跳过 ls-files 半"
  fi
  if [ -f "$root/.gitignore" ]; then
    grep -q 'dockercargo' "$root/.gitignore" || { fail P15 ".gitignore 缺 os/.dockercargo/ 条目（git add -A 防线）"; bad=1; }
  fi
  [ "$bad" -eq 0 ] && ok P15 "vendored 目录防线在位（跟踪 0 + gitignore 条目）"
  return 0
}

# --------------------------------------------------------------- 自测（正反例判别矩阵）
ST_FAIL=0
st_expect() { # 用例名 期望子串（固定串匹配） 实际输出
  local name="$1" want="$2" got="$3"
  if printf '%s' "$got" | grep -qF "$want"; then
    printf '  [case] %-22s PASS（含「%s」）\n' "$name" "$want"
  else
    printf '  [case] %-22s FAIL 期望含「%s」实得:\n%s\n' "$name" "$want" "$got"; ST_FAIL=$((ST_FAIL+1))
  fi
}
run_selftest() {
  local F="$ROOT"
  echo "== pattern-gate --self-test（夹具根 $F）=="

  # --- P1 ---
  mkdir -p "$F/.review/zcode/edge9"
  printf '# t 线修复日志\n\n## Fix #1 — a\n- x\n' > "$F/.review/zcode/edge9/FIXLOG.md"
  out=$(check_p1 "$F")
  st_expect "P1-healthy"   "[P1] PASS" "$out"
  printf '# t\n# t\nbody\n' >> "$F/.review/zcode/edge9/FIXLOG.md"
  out=$(check_p1 "$F")
  st_expect "P1-dup-header" "[P1] FAIL" "$out"
  printf '# t only\n' > "$F/.review/zcode/edge9/FIXLOG.md"
  for i in 1 2 3 4 5 6 7 8; do echo "same line" >> "$F/.review/zcode/edge9/FIXLOG.md"; done
  out=$(check_p1 "$F")
  st_expect "P1-consec-repeat" "[P1] FAIL" "$out"
  rm -rf "$F/.review"

  # --- P4 ---
  mkdir -p "$F/os/tests"
  : > "$F/os/tests/a.rs"
  printf '[[test]]\nname = "a"\npath = "a.rs"\n' > "$F/os/tests/Cargo.toml"
  out=$(check_p4 "$F"); st_expect "P4-ok" "[P4] PASS" "$out"
  : > "$F/os/tests/b.rs"
  out=$(check_p4 "$F"); st_expect "P4-orphan" "未声明 [[test]]" "$out"
  rm -f "$F/os/tests/b.rs"
  printf '[[test]]\nname = "a"\npath = "a.rs"\n[[test]]\nname = "ghost"\npath = "ghost.rs"\n' > "$F/os/tests/Cargo.toml"
  out=$(check_p4 "$F"); st_expect "P4-ghost" "不存在的 [[test]]" "$out"
  printf '[[test]]\nname = "a"\npath = "a.rs"\n' > "$F/os/tests/Cargo.toml"

  # --- P5 ---
  mkdir -p "$F/os/qemu-tests" "$F/os/kernel/src"
  printf 'T4_MARKER="${T4_MARKER:-rc: minimal boot script marker}"\ngrep -q "entering scheduler"\n' > "$F/os/qemu-tests/test-cmd-smoke.sh"
  printf 'boot_stage!("entering scheduler\\n");\n' > "$F/os/kernel/src/lib.rs"
  mkdir -p "$F/os/etc"; printf 'echo "minix-rs rc: minimal boot script marker"\n' > "$F/os/etc/rc"
  out=$(check_p5 "$F"); st_expect "P5-ok" "[P5] PASS" "$out"
  rm "$F/os/etc/rc"
  out=$(check_p5 "$F"); st_expect "P5-drift" "[P5] FAIL" "$out"

  # --- P6 ---
  mkdir -p "$F/.github/workflows"
  cat > "$F/.github/workflows/qemu-tests.yml" <<'YAML'
  cross-arch-kernel-build:
    - run: cargo build -p minix-kernel --target aarch64-unknown-none --no-default-features
    - run: cargo build -p minix-kernel --target riscv64gc-unknown-none-elf --no-default-features
YAML
  out=$(check_p6 "$F"); st_expect "P6-ok" "[P6] PASS" "$out"
  sed -i 's/cargo build -p minix-kernel --target riscv64[^ ]*/cargo check -p minix-kernel --target riscv64gc-unknown-none-elf/' "$F/.github/workflows/qemu-tests.yml"
  out=$(check_p6 "$F"); st_expect "P6-check-degrade" "[P6] FAIL" "$out"

  # --- P7 ---
  mkdir -p "$F/os/qemu-tests"
  printf 'cargo build -p x --release 2>&1 || echo "(build failed)"\n' > "$F/os/qemu-tests/run_all.sh"
  out=$(collect_p7 "$F")
  st_expect "P7-detect" '|| echo "(build failed)"' "$out"

  # --- P9 ---
  printf '.wt/\n/tmp/*\n!/tmp/nk4a/\n' > "$F/.gitignore"
  out=$(check_p9 "$F"); st_expect "P9-ok" "[P9] PASS" "$out"
  sed -i '/^!.\/tmp\/nk4a/d;/^!\/tmp\/nk4a/d' "$F/.gitignore"
  out=$(check_p9 "$F"); st_expect "P9-missing-rule" "[P9] FAIL" "$out"

  # --- P11 ---
  mkdir -p "$F/minix3/kernel/arch/i386"
  printf 'l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n' > "$F/minix3/kernel/arch/i386/memory.c"
  printf 'memory.c:5\nmemory.c:99\nghost.c:1\n' > "$F/.p11_tokens"
  out=$(check_p11 "$F" "$F/.p11_tokens")
  st_expect "P11-missing-file" "锚点文件不存在: ghost.c" "$out"
  st_expect "P11-line-overflow" "memory.c:99" "$out"
  printf 'memory.c:5\n' > "$F/.p11_tokens"
  out=$(check_p11 "$F" "$F/.p11_tokens"); st_expect "P11-ok" "[P11] PASS" "$out"

  # --- P13 ---
  mkdir -p "$F/os/drivers/x"
  printf 'static let A_REAL_SYMBOL = 1;\n' > "$F/os/drivers/x/lib.rs"
  printf 'A_REAL_SYMBOL\nA_PHANTOM_SYMBOL\n' > "$F/.p13_tokens"
  out=$(check_p13 "$F" "$F/.p13_tokens")
  st_expect "P13-phantom" "全仓不存在: A_PHANTOM_SYMBOL" "$out"
  printf 'A_REAL_SYMBOL\n' > "$F/.p13_tokens"
  out=$(check_p13 "$F" "$F/.p13_tokens"); st_expect "P13-ok" "[P13] PASS" "$out"

  # --- P14 ---
  mkdir -p "$F/os/kernel/src"
  printf 'slot.p_rts_flags.set(RtsFlagsBits::RECEIVING);\n' > "$F/os/kernel/src/clock.rs"
  printf 'fn rts_set() {}\np.p_rts_flags.clear(SLOT_FREE);\n' > "$F/os/kernel/src/proc.rs"
  out=$(collect_p14 "$F")
  st_expect "P14-detect"  "clock.rs" "$out"
  if printf '%s' "$out" | grep -q "proc.rs"; then
    printf '  [case] %-22s FAIL 定义文件 proc.rs 不应被抓\n' "P14-scope"; ST_FAIL=$((ST_FAIL+1))
  else
    printf '  [case] %-22s PASS（定义文件豁免）\n' "P14-scope"
  fi

  # --- P15（gitignore 半；ls-files 半在真树变异验证）---
  printf '.wt/\n/tmp/*\n!/tmp/nk4a/\nos/.dockercargo/\n' > "$F/.gitignore"
  out=$(check_p15 "$F"); st_expect "P15-ok" "[P15] PASS" "$out"
  printf '.wt/\n/tmp/*\n' > "$F/.gitignore"
  out=$(check_p15 "$F"); st_expect "P15-missing" "[P15] FAIL" "$out"

  rm -f "$F/.p11_tokens" "$F/.p13_tokens"
  echo "----"
  if [ "$ST_FAIL" -gt 0 ]; then
    echo "self-test FAIL（$ST_FAIL 例）"; exit 1
  fi
  echo "self-test 全绿"
  exit 0
}

# --------------------------------------------------------------- 主流程
if [ "$DO_SELFTEST" -eq 1 ]; then
  run_selftest
fi

echo "== pattern-gate 全量检查 root=$ROOT $([ "$DO_DIFF" -eq 1 ] && echo "（附增量门 RANGE=$RANGE）") =="

check_p1 "$ROOT"
check_p2 "$ROOT"
check_p3 "$ROOT"
check_p4 "$ROOT"
check_p5 "$ROOT"
check_p6 "$ROOT"
check_p7 "$ROOT"
check_p8 "$ROOT"
check_p9 "$ROOT"
report_p10 "$ROOT"
check_p12 "$ROOT"
check_p14 "$ROOT"
check_p15 "$ROOT"

if [ "$DO_DIFF" -eq 1 ]; then
  added=$(diff_added_lines "$ROOT" "$RANGE")
  check_p10_diff "$added"
  TOKBASE="${TMPDIR:-/tmp}/pattern-gate-tokens.$$"
  # P11/P13 的 token 提取排除检查脚本自身与目录文档（两者引用锚点/符号属合法自指）；
  # 先剥 shell 字面量 \n，防“nfoo.c:1”型假 token
  printf '%s\n' "$added" | grep -v 'tools/pattern-gate.sh' | grep -v 'PATTERN-SCAN-REPORT' \
    | cut -f2- | sed 's/\\n/ /g' | grep -oE '[A-Za-z0-9_.$-]+\.c:[0-9]+' | sort -u > "$TOKBASE.p11"
  printf '%s\n' "$added" | grep -v 'tools/pattern-gate.sh' | grep -v 'PATTERN-SCAN-REPORT' \
    | cut -f2- | grep -oE '`[A-Z][A-Z0-9_]{3,}`' | tr -d '`' | sort -u > "$TOKBASE.p13"
  check_p11 "$ROOT" "$TOKBASE.p11"
  check_p13 "$ROOT" "$TOKBASE.p13"
fi

if [ "$DO_BASELINE" -eq 1 ]; then
  mkdir -p "$ROOT/$(dirname "$BASELINE_REL")"
  printf '%s' "$NEW_BASELINE" | sort -u > "$ROOT/$BASELINE_REL"
  echo "== 基线已重写: $BASELINE_REL（$(grep -c . "$ROOT/$BASELINE_REL") keys）=="
fi

echo "----"
echo "PASS=$PASS_CNT FAIL=$FAIL_CNT SKIP=$SKIP_CNT"
if [ "$FAIL_CNT" -gt 0 ]; then
  echo "违规 ${FAIL_CNT} 项。存量豁免见 $BASELINE_REL（--update-baseline 重写）。"
  exit 1
fi
exit 0
