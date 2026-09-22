#!/bin/bash
# check-layout.sh — 生产内核镜像的布局断言（NK4-B P3 M3.2，宿主可测）
#
# 判据来源：NK4B-TODO §3 M3.2「验证=宿主可测：产出 ELF + readelf 断言
# （entry/段布局）」。真机装载要等 M3.3 的 boot-shim 接线，本脚本只在宿主
# 校验镜像的静态布局——它是 boot-shim `compute_kernel_layout` /
# `load_segments_into_phys_memory`（os/boot-shim/src/loader.rs）读的全部字段。
#
# 断言清单（每项都是一条会被违反的真实契约，不是装饰）：
#   L1 ELF 类型 = EXEC（非 static-pie：内核 ELF 装载面无重定位处理器）
#   L2 机器类型与预期三元组相符（防"拿 x86 工件当 aarch64 工件"）
#   L3 首段 PT_LOAD 的 VMA/LMA = 该架构预期的基址
#      （x86_64 惯例 vaddr = 高半基址 + paddr ⇒ 首段 VMA 含物理基址；
#       aarch64 惯例 vaddr = 高半基址 + (paddr - 物理基址) ⇒ 首段 VMA 就是
#       高半基址。两条都满足 boot-shim 的 min(vaddr)/min(paddr) 契约，
#       差别写在各自 .ld 头部）
#   L4 所有 PT_LOAD 的 (vaddr - paddr) 同一个平移量
#      —— 高半映射「一个 kern_virt_base ↔ kern_phys_base 平移」的前提
#   L5 镜像跨距（末段 VMA+memsz 减首段 VMA）2MiB 对齐
#      —— boot_validate_and_prepare 的大页阶梯（NK4-A 首亮实断）
#   L6 入口 = 链接脚本 ENTRY(_start) = 符号 `_start` 的值，且落在首段内
#   L7 引导栈符号存在、自顶向下 64 KiB、整体落在末段（PT_LOAD memsz 内）
#   L8 内核启动图未被裁掉：`minix_kernel::arch_boot` 符号在镜像里
#      （KERNEL_ENTRY_ANCHOR + 链接脚本 KEEP 的双重保险的可观测证据）
#
# 用法：bash os/kernel-image/check-layout.sh [x86_64|aarch64|all]
#       SKIP_BUILD=1 复用已有工件（默认每个架构都重新构建）
# 退出码：0 = 全部断言通过；1 = 任一断言失败；2 = 用法/环境错误
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OS_ROOT="$(dirname "$SCRIPT_DIR")"
WHICH="${1:-all}"
case "$WHICH" in
    x86_64|aarch64|all) ;;
    *) echo "usage: $0 [x86_64|aarch64|all]" >&2; exit 2 ;;
esac
command -v readelf >/dev/null || { echo "缺 readelf" >&2; exit 2; }
command -v nm >/dev/null || { echo "缺 nm" >&2; exit 2; }

FAIL=0
hex2dec() { echo $((16#$1)); }   # bash 自带 64 位整数运算，不依赖 gawk

# check <编号> <说明> <条件 0/1>
check() {
    local id="$1" desc="$2" ok="$3"
    if [ "$ok" = "1" ]; then
        echo "  [$id] PASS  $desc"
    else
        echo "  [$id] FAIL  $desc" >&2
        FAIL=1
    fi
}

# 每架构一行：triple|feature|readelf Machine 字样|预期首段 VMA|预期首段 LMA|预期入口
# （数值按十进制比较，故这里写十六进制字面量不受 readelf 补零宽度影响）
arch_expect() {
    case "$1" in
        x86_64)  echo "x86_64-unknown-none|fw-x86-none|Advanced Micro Devices X86-64|ffff800000200000|200000|ffff800000200000" ;;
        aarch64) echo "aarch64-unknown-none|fw-aarch64-none|AArch64|ffff800000000000|40200000|ffff800000000000" ;;
    esac
}

check_arch() {
    local arch="$1" triple feature machine want_vma want_lma entry_want
    IFS='|' read -r triple feature machine want_vma want_lma entry_want < <(arch_expect "$arch")
    local elf="$OS_ROOT/target/$triple/release/kernel"

    echo "== $arch：$elf"
    if [ "${SKIP_BUILD:-0}" != "1" ]; then
        # 与 xtask 装机同一条构建命令（换 target/feature）；ulimit 兜住
        # 宿主内存上限（大链接在 CI 容器里会 OOM，见 NK4A-TODO §7）。
        # 构建日志只在失败时回显，否则几百行既有警告会淹掉断言结果。
        local blog
        blog="$(mktemp /tmp/check-layout-build.XXXXXX.log)"
        ( cd "$OS_ROOT" && (ulimit -v 3145728; \
            cargo build -q -p kernel-image --target "$triple" \
                --features "$feature" --release) ) > "$blog" 2>&1 || {
            echo "  构建失败（$arch）：$blog" >&2; tail -40 "$blog" >&2; FAIL=1; return; }
        rm -f "$blog"
    fi
    [ -f "$elf" ] || { echo "  工件缺失：$elf" >&2; FAIL=1; return; }

    # L1 / L2
    local hdr etype amachine
    hdr="$(readelf -h "$elf")"
    etype="$(echo "$hdr" | sed -n 's/^ *Type:  *\(.*\)$/\1/p' | sed 's/ .*//')"
    amachine="$(echo "$hdr" | sed -n 's/^ *Machine:  *\(.*\)$/\1/p')"
    check L1 "ELF 类型 = EXEC（实得 $etype）" "$([ "$etype" = "EXEC" ] && echo 1 || echo 0)"
    check L2 "机器类型含 \"$machine\"（实得 $amachine）" \
        "$(echo "$amachine" | grep -qi -- "$machine" && echo 1 || echo 0)"

    # PT_LOAD 表：vaddr paddr filesz memsz（readelf -lW 的第 3/4/5/6 列）
    local loads
    loads="$(readelf -lW "$elf" | awk '$1=="LOAD"{print $3,$4,$5,$6}')"
    local nload
    nload="$(echo "$loads" | grep -c . )"
    check "L3a" "PT_LOAD 段数 >= 1（实得 $nload）" "$([ "$nload" -ge 1 ] && echo 1 || echo 0)"

    local first_vaddr first_paddr first_memsz
    first_vaddr="$(echo "$loads" | head -1 | awk '{print $1}')"
    first_paddr="$(echo "$loads" | head -1 | awk '{print $2}')"
    first_memsz="$(echo "$loads" | head -1 | awk '{print $4}')"
    first_vaddr="${first_vaddr#0x}"; first_paddr="${first_paddr#0x}"; first_memsz="${first_memsz#0x}"
    check "L3b" "首段 VMA = 0x$want_vma（实得 0x$first_vaddr）" \
        "$([ "$(hex2dec "$first_vaddr")" = "$(hex2dec "$want_vma")" ] && echo 1 || echo 0)"
    check "L3c" "首段 LMA = 0x$want_lma（实得 0x$first_paddr）" \
        "$([ "$(hex2dec "$first_paddr")" = "$(hex2dec "$want_lma")" ] && echo 1 || echo 0)"

    # L4：所有段的 vaddr-paddr 平移量与首段一致
    local delta_want bad=0 line v p d
    delta_want=$(( $(hex2dec "$first_vaddr") - $(hex2dec "$first_paddr") ))
    while read -r v p _; do
        [ -z "${v:-}" ] && continue
        v="${v#0x}"; p="${p#0x}"
        d=$(( $(hex2dec "$v") - $(hex2dec "$p") ))
        [ "$d" -ne "$delta_want" ] && bad=$((bad + 1))
    done <<< "$loads"
    check L4 "所有 PT_LOAD 的 vaddr-paddr 平移量一致（= $(printf 0x%x "$delta_want")，违例 $bad 段）" \
        "$([ "$bad" = 0 ] && echo 1 || echo 0)"

    # L5：跨距 2MiB 对齐（末段 VMA+memsz 减首段 VMA）
    local last_vaddr last_memsz span
    last_vaddr="$(echo "$loads" | tail -1 | awk '{print $1}')"; last_vaddr="${last_vaddr#0x}"
    last_memsz="$(echo "$loads" | tail -1 | awk '{print $4}')"; last_memsz="${last_memsz#0x}"
    span=$(( $(hex2dec "$last_vaddr") + $(hex2dec "$last_memsz") - $(hex2dec "$first_vaddr") ))
    check L5 "镜像跨距 $((span / 1024 / 1024)) MiB 且 2MiB 对齐（余 $((span % 0x200000)) 字节）" \
        "$([ $((span % 0x200000)) = 0 ] && echo 1 || echo 0)"

    # L6：入口 = ENTRY(_start) = _start 符号，且落在首段内
    local entry got_start
    entry="$(echo "$hdr" | sed -n 's/^ *Entry point address:  *0x\([0-9a-fA-F]*\)$/\1/p')"
    got_start="$(nm "$elf" | awk '$3=="_start"{print $1}')"
    check "L6a" "入口 = 0x$entry_want（实得 0x$entry）" \
        "$([ "$(hex2dec "$entry")" = "$(hex2dec "$entry_want")" ] && echo 1 || echo 0)"
    check "L6b" "符号 _start = 入口（实得 ${got_start:-无}）" \
        "$([ "${got_start,,}" = "$entry" ] && echo 1 || echo 0)"
    local e_dec fv_dec fs_dec
    e_dec=$(hex2dec "$entry"); fv_dec=$(hex2dec "$first_vaddr"); fs_dec=$(hex2dec "$first_memsz")
    check "L6c" "入口 $(printf 0x%x "$e_dec") 落在首段 [$(printf 0x%x "$fv_dec"), +$(printf 0x%x "$fs_dec")) 内" \
        "$([ "$e_dec" -ge "$fv_dec" ] && [ "$e_dec" -lt "$((fv_dec + fs_dec))" ] && echo 1 || echo 0)"

    # L7：引导栈符号（64 KiB，落在末段）
    local sb st
    sb="$(nm "$elf" | awk '$3=="kernel_boot_stack_bottom"{print $1}')"
    st="$(nm "$elf" | awk '$3=="kernel_boot_stack_top"{print $1}')"
    if [ -z "$sb" ] || [ -z "$st" ]; then
        check L7 "引导栈符号存在（bottom=${sb:-无} top=${st:-无}）" 0
    else
        local span_want last_lo last_hi ok7
        span_want=$(( $(hex2dec "$st") - $(hex2dec "$sb") ))
        last_lo=$(hex2dec "$last_vaddr"); last_hi=$((last_lo + $(hex2dec "$last_memsz")))
        ok7=0
        [ "$span_want" = "$((0x10000))" ] \
            && [ $(hex2dec "$sb") -ge "$last_lo" ] && [ $(hex2dec "$st") -le "$last_hi" ] && ok7=1
        check L7 "引导栈 64 KiB（实得 $span_want 字节）且落在末段 [$(printf 0x%x "$last_lo"), +$(printf 0x%x "$last_hi"))（bottom=$(printf 0x%x "$(( $(hex2dec "$sb") ))")）" "$ok7"
    fi

    # L8：内核启动图在镜像里
    local n_anchor
    n_anchor="$(nm "$elf" | grep -c "minix_kernel9arch_boot" || true)"
    check L8 "minix_kernel::arch_boot 在镜像里（命中 $n_anchor 个符号）" \
        "$([ "$n_anchor" -ge 1 ] && echo 1 || echo 0)"
}

case "$WHICH" in
    x86_64)  check_arch x86_64 ;;
    aarch64) check_arch aarch64 ;;
    all)     check_arch x86_64; check_arch aarch64 ;;
esac

if [ "$FAIL" = 0 ]; then
    echo "CHECK-LAYOUT=PASS（$WHICH）"
else
    echo "CHECK-LAYOUT=FAIL（$WHICH）" >&2
fi
exit "$FAIL"
