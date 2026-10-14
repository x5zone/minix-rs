#!/usr/bin/env bash
# atf-evidence.sh — 把 ATF 套件的**建立证据**钉成一份可比对的文件。
#
# 台账 `rewrite-notes/coordination/TODO-LEDGER-OPEN.md` §7 那行「上游测试套件的建立证据没有钉
# 校验和与构建提交，干净重建后引导腿会崩」的正主：`test-atf-riscv64.sh` 只校 `atf-plan.txt`
# 存在、非空、覆盖 18 个 prog，却不知道这些 ELF 是哪棵提交、哪套工具链建出来的。于是「一次
# 绿」这个读数不可判读——换工具链、`RS_ATF_SKIP_BUILD=1` 复用旧 ELF、或干净重建后少链一例，
# 串口上都可能长得一样，而引导腿其实已经崩。
#
# 钉四件事，每件都是可重跑的读数：
#   1. 构建提交（`git rev-parse HEAD`；工作树脏则记 `worktree=dirty`）；
#   2. 工具链身份（交叉 `gcc --version` 首行 + 共享输入 `libatf-c.a` 的校验和，
#      `build-atf-test.sh` 每一例都链它）；
#   3. 计划里每个 prog 的 ELF 校验和（逐 prog 一行，另给一个聚合值）；
#   4. prog 数本身（计划去重后的条目数；缺任一 ELF 当场判红，不等串口）。
#
# 用法：
#   tools/atf-evidence.sh write  <arch> <plan> <out>       # 生成证据
#   tools/atf-evidence.sh check  <arch> <plan> <evidence>   # 重算并比对，不一致退 1
#   tools/atf-evidence.sh self-test                        # 夹具正反对（不需要交叉工具链）
# 退出码：0 通过 / 1 比对或完整性失败 / 2 用法与前置缺失
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs

sha256_of() { sha256sum "$1" | cut -d' ' -f1; }

# 交叉编译器名按 arch 取（与 tools/build-atf-test.sh 同一张表）。
cc_for() {
    case "$1" in
        riscv64) echo riscv64-unknown-elf-gcc ;;
        aarch64) echo aarch64-linux-gnu-gcc ;;
        x86_64)  echo gcc ;;
        *) return 1 ;;
    esac
}

# `xtask atf-face` 写的 `atf-plan.txt` 每行是「<prog> <tc>」；一个 prog 一个 ELF，
# 多个 tc 共用它，所以证据按 prog 去重。
plan_progs() { awk 'NF>=2 {print $1}' "$1" | sort -u; }

elf_dir_for() { echo "$ROOT/os/target/atf/$1/tests"; }

# 完整性：计划里的每个 prog 都得有 ELF。缺例正是「干净重建后引导腿会崩」的形态，
# 在起 QEMU 之前判掉比从串口猜便宜得多。
require_elves() { # $1=plan $2=elfdir
    local plan="$1" dir="$2" missing=0 prog
    while IFS= read -r prog; do
        [ -n "$prog" ] || continue
        [ -f "$dir/$prog" ] || { echo "MISSING ELF: $dir/$prog（计划里有，构建产物里没有）" >&2; missing=$((missing + 1)); }
    done < <(plan_progs "$plan")
    [ "$missing" -eq 0 ]
}

# 把证据打到 stdout（调用方重定向）。同一棵提交、同一批 ELF 重算两次必须逐字相同，
# 否则这份证据本身就不可比。
emit_evidence() { # $1=arch $2=plan $3=elfdir
    local arch="$1" plan="$2" dir="$3" cc line lib
    cc="$(cc_for "$arch")" || return 2
    echo "arch=$arch"
    echo "commit=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)"
    if [ -n "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ]; then
        echo "worktree=dirty"
    else
        echo "worktree=clean"
    fi
    if command -v "$cc" >/dev/null 2>&1; then
        line="$("$cc" --version 2>/dev/null | head -1)"
    else
        line="absent"
    fi
    echo "toolchain=$line"
    lib="$ROOT/os/target/atf/$arch/libatf-c.a"
    if [ -f "$lib" ]; then
        echo "libatf_c_sha256=$(sha256_of "$lib")"
    else
        echo "libatf_c_sha256=absent"
    fi
    local n=0 sum="" prog hash
    while IFS= read -r prog; do
        [ -n "$prog" ] || continue
        if [ -f "$dir/$prog" ]; then hash="$(sha256_of "$dir/$prog")"; else hash=missing; fi
        printf 'elf %s %s\n' "$prog" "$hash"
        sum="$sum$prog$hash"
        n=$((n + 1))
    done < <(plan_progs "$plan")
    echo "progs=$n"
    echo "suite_sha256=$(printf '%s' "$sum" | sha256sum | cut -d' ' -f1)"
}

case "${1:-}" in
    write)
        arch="${2:?write 需要 arch}"; plan="${3:?write 需要 plan 路径}"; out="${4:?write 需要 out 路径}"
        dir="$(elf_dir_for "$arch")"
        [ -s "$plan" ] || { echo "plan 缺失或为空：$plan" >&2; exit 2; }
        require_elves "$plan" "$dir" || exit 1
        emit_evidence "$arch" "$plan" "$dir" >"$out" || exit 2
        echo "OK: 证据已写到 $out（$(grep -c '^elf ' "$out") 个 prog，commit $(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)）"
        ;;
    check)
        arch="${2:?check 需要 arch}"; plan="${3:?check 需要 plan 路径}"; ev="${4:?check 需要 evidence 路径}"
        dir="$(elf_dir_for "$arch")"
        [ -s "$ev" ] || { echo "证据文件缺失：$ev（先跑 tools/atf-evidence.sh write）" >&2; exit 2; }
        require_elves "$plan" "$dir" || exit 1
        fresh="$(emit_evidence "$arch" "$plan" "$dir")" || exit 2
        if [ "$fresh" != "$(cat "$ev")" ]; then
            printf '%s\n' "$fresh" >"$ev.current"
            echo "FAIL: 建立证据与当前构建不一致（$ev）" >&2
            diff -u "$ev" "$ev.current" | sed 's/^/  /' >&2 || true
            rm -f "$ev.current"
            echo "  ↑ 「-」为已记录、「+」为当前重算；换工具链、复用旧 ELF、少链一例都会走到这里" >&2
            exit 1
        fi
        echo "OK: 建立证据一致（$(grep -c '^elf ' "$ev") 个 prog，suite $(grep '^suite_sha256=' "$ev" | cut -d= -f2 | cut -c1-12)…）"
        ;;
    self-test)
        tmp="$(mktemp -d)" || { echo "mktemp 失败" >&2; exit 2; }
        trap 'rm -rf "$tmp"' EXIT
        mkdir -p "$tmp/tests"
        printf '%s  %s\n' tc_string first tc_mem second >"$tmp/plan.txt"
        printf 'ELF-A' >"$tmp/tests/tc_string"
        printf 'ELF-B' >"$tmp/tests/tc_mem"
        fails=0
        say() { printf '  [case] %-24s %s\n' "$1" "$2"; [ "$2" = PASS ] || fails=$((fails + 1)); }
        echo "== atf-evidence self-test（夹具根 $tmp）=="
        a="$(emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests")"
        b="$(emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests")"
        if [ "$a" = "$b" ]; then say "两次重算逐字相同" PASS; else say "两次重算逐字相同" FAIL; fi
        if [ "$(printf '%s\n' "$a" | grep -c '^elf ')" = "2" ]; then
            say "逐 prog 一行校验和" PASS
        else
            say "逐 prog 一行校验和" FAIL
        fi
        rm -f "$tmp/tests/tc_mem"
        if require_elves "$tmp/plan.txt" "$tmp/tests"; then say "缺 ELF 时判红" FAIL; else say "缺 ELF 时判红" PASS; fi
        printf 'ELF-B' >"$tmp/tests/tc_mem"
        before="$(emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests" | grep '^suite_sha256=')"
        printf 'ELF-B-tampered' >"$tmp/tests/tc_mem"
        after="$(emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests" | grep '^suite_sha256=')"
        if [ "$before" != "$after" ]; then say "篡改 ELF 聚合值即变" PASS; else say "篡改 ELF 聚合值即变" FAIL; fi
        printf '%s  %s\n' tc_new third >>"$tmp/plan.txt"
        printf 'ELF-C' >"$tmp/tests/tc_new"
        if [ "$(emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests" | grep '^progs=' | cut -d= -f2)" = "3" ]; then
            say "计划条目数进证据" PASS
        else
            say "计划条目数进证据" FAIL
        fi
        # 多 tc 共用一个 prog 时不重复计数（证据按 prog 去重）
        printf '%s  %s\n' tc_mem third >>"$tmp/plan.txt"
        if [ "$(emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests" | grep '^progs=' | cut -d= -f2)" = "3" ]; then
            say "同 prog 多 tc 不重复计数" PASS
        else
            say "同 prog 多 tc 不重复计数" FAIL
        fi
        # check 的正反两路：一致必须 0，换掉一个 ELF 必须 1
        emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests" >"$tmp/ev.txt"
        if check_out="$(
                ROOT_OVERRIDE=1
                emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests" | diff -q - "$tmp/ev.txt" >/dev/null
            )"; then say "写后重算无漂移" PASS; else say "写后重算无漂移" FAIL; fi
        printf 'ELF-DIFFERENT' >"$tmp/tests/tc_new"
        if emit_evidence x86_64 "$tmp/plan.txt" "$tmp/tests" | diff -q - "$tmp/ev.txt" >/dev/null; then
            say "换 ELF 后重算必不等" FAIL
        else
            say "换 ELF 后重算必不等" PASS
        fi
        echo "----"
        if [ "$fails" -eq 0 ]; then echo "self-test 全绿"; exit 0; else echo "self-test 有 $fails 项判红"; exit 1; fi
        ;;
    *)
        sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
        exit 2
        ;;
esac
