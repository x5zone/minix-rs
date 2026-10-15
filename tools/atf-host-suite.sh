#!/usr/bin/env bash
# atf-host-suite.sh — x86_64 宿主执行门：把上游 ATF C 套件在宿主**真跑一遍**并判终态。
#
# 这条门补的是「编译腿看不见的那一半」。台账 P-X86-01 的实测判例：从链接行摘掉
# `random.o` 时构建退码仍为 0、产物照样链成 ET_EXEC，只有把二进制跑起来才在
# `minix3/tests/lib/libc/string/t_memcpy.c:99` 现形——BSD 随机序这类**语义**缺口
# 对「18/18 构建成功」这种读数是瞎的。x86_64 腿链宿主 glibc，产物本身就是宿主可执行
# ELF（交叉两腿的产物在宿主跑不了），所以它是三腿里唯一能拿原生速度逐用例判定的腿。
#
# 它**不声明**出生腿能跑：x86_64 产物走 glibc 的运行时初始化与 Linux 系统调用，
# 上我方真机仍需台账 P-X86-01 的缺口④（freestanding x86_64 C 工具链，已登记待裁决）。
# 所以本门不替换 `os/qemu-tests/test-atf-{riscv64,aarch64}.sh`，也不动
# `ATF_BOOT_LEG_READY` 白名单——那条白名单断言的是「自定义出生腿能跑」，口径不混。
#
# 期望用例数从**测试源头**现算（每个 `ATF_TP_ADD_TC(tp, …)` 一行），不从产物 `-l`
# 输出反推：后者会把「某个 prog 压根没列出用例」这种漏测洗成绿灯。
#
# 用法：tools/atf-host-suite.sh [arch=x86_64]
# 退出码：0=PASS 1=FAIL 2=SKIP（前置件缺，按本仓门禁口径 SKIP 必须打 RESULT: SKIP，
#         免得「因为没装东西所以永远绿」）
set -uo pipefail

ARCH="${1:-x86_64}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SUITE="$ROOT/minix3/tests/lib/libc/string"
BUILD="$ROOT/os/target/atf/$ARCH/tests"
TIMEOUT_TC="${ATF_HOST_TIMEOUT:-120}"

[ "$ARCH" = "x86_64" ] || { echo "RESULT: SKIP — 本门只服务宿主腿（arch=$ARCH 的产物在宿主不可执行）"; exit 2; }
command -v cc >/dev/null 2>&1 || { echo "RESULT: SKIP — 缺宿主编译器 cc"; exit 2; }
[ -d "$SUITE" ] || { echo "FAIL: 测试源目录不存在 $SUITE"; exit 1; }

echo "== Stage 1: 逐例构建（$ARCH 宿主腿）=="
progs=()
build_fail=0
for src in "$SUITE"/*.c; do
    name="$(basename "$src" .c)"
    if bash "$ROOT/tools/build-atf-test.sh" "$src" "$ARCH" >/dev/null 2>&1; then
        progs+=("$name")
    else
        echo "  BUILD-FAIL: $name"
        build_fail=$((build_fail + 1))
    fi
done
[ "$build_fail" -eq 0 ] || { echo "FAIL: $build_fail 例构建失败（构建面的缺口不许当成未跑）"; exit 1; }
echo "  构建 ok=${#progs[@]} 例"

# 期望终态行数：测试源里每个 ATF_TP_ADD_TC 一行（现算，不抄表）
expected=0
for src in "$SUITE"/*.c; do
    n=$(grep -c '^[[:space:]]*ATF_TP_ADD_TC(tp,' "$src")
    expected=$((expected + n))
done
[ "$expected" -gt 0 ] || { echo "FAIL: 从测试源算出期望用例数为 0（源目录或宏形变了？本门判据失效，不许静默放行）"; exit 1; }
echo "== Stage 2: 逐用例真跑（期望终态 $expected 行，按 ATF_TP_ADD_TC 现算）=="

n_pass=0; n_skip=0; n_fail=0; n_broken=0; n_norst=0
for p in "${progs[@]}"; do
    bin="$BUILD/$p"
    [ -x "$bin" ] || { echo "  MISSING: $p 无可执行产物"; n_norst=$((n_norst + 1)); continue; }
    # 用例清单来自产物自己的元数据（`-l` 的 ident: 行）
    tcs=$("$bin" -l 2>/dev/null | sed -n 's/^ident: //p')
    if [ -z "$tcs" ]; then
        echo "  NO-TC-LIST: $p 列不出用例（源里有 ATF_TP_ADD_TC 而产物无终态可跑）"
        n_norst=$((n_norst + 1)); continue
    fi
    for tc in $tcs; do
        out=$(timeout "$TIMEOUT_TC" "$bin" "$tc" 2>&1 | grep -vE "WARNING|isolation" | tail -1)
        case "$out" in
            passed)  n_pass=$((n_pass + 1));;
            skipped*) n_skip=$((n_skip + 1));;
            broken*)  n_broken=$((n_broken + 1)); echo "  BROKEN: $p/$tc → $out";;
            failed*)  n_fail=$((n_fail + 1));   echo "  FAILED: $p/$tc → $out";;
            *)        n_norst=$((n_norst + 1));  echo "  NO-RESULT: $p/$tc → [${out:-（无输出或被超时杀掉）}]";;
        esac
    done
done

total=$((n_pass + n_skip + n_fail + n_broken + n_norst))
echo "RESULT: 终态 $total/$expected（$n_pass passed + $n_skip skipped + $n_fail failed + $n_broken broken + $n_norst 无终态）"

if [ "$total" -ne "$expected" ]; then
    echo "RESULT: FAIL — 终态行数与测试源算出的期望不符（少跑或多跑；期望 $expected 实得 $total）"
    exit 1
fi
if [ "$n_norst" -gt 0 ]; then
    echo "RESULT: FAIL — $n_norst 例未达终态（挂死、超时或崩溃无输出）"
    exit 1
fi
if [ "$n_fail" -gt 0 ] || [ "$n_broken" -gt 0 ]; then
    echo "RESULT: FAIL — $n_fail failed + $n_broken broken（skipped 不算失败，但逐条打印在此）"
    exit 1
fi
echo "RESULT: PASS — $expected/$expected 达终态，0 failed/broken（$n_skip 例为上游默认 skip）"
exit 0
