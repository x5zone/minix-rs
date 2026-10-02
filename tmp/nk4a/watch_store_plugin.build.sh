#!/usr/bin/env bash
# NK4-C (A) 瞬态写者插件：编译 + 挂到 riscv boot 的辅助脚本（待 qemu-plugin.h 就绪）。
#
# 前置（用户安装后满足其一）：
#   - 有 `pkg-config --cflags --libs qemu-plugin`（Ubuntu: apt install qemu-system-common
#     + qemu 开发头，使 /usr/include/qemu-plugin.h 存在），或
#   - 手工拿到 qemu-plugin.h 放到本目录（脚本会 -I 本目录）。
#
# 本脚本先【探测头文件】；缺则明确 SKIP 并提示安装，不静默失败。
set -u
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SO="$DIR/watch_store_plugin.so"
C="$DIR/watch_store_plugin.c"
TARGET="${1:-0x9dc377f8}"      # 目标物理地址（(A) 子根 i2=255 槽）

echo "== 探测 qemu-plugin.h =="
HDR=""
for c in $(pkg-config --cflags qemu-plugin 2>/dev/null) /usr/include /usr/include/qemu "$DIR"; do
    d="${c#/usr/include}"
    [ -f "/usr/include/qemu-plugin.h" ] && { HDR="/usr/include/qemu-plugin.h"; break; }
    [ -f "$c/qemu-plugin.h" ] && { HDR="$c/qemu-plugin.h"; break; }
done
if [ -z "$HDR" ] && [ ! -f "$DIR/qemu-plugin.h" ]; then
    echo "SKIP: qemu-plugin.h 未找到。安装其一后重试："
    echo "      sudo apt install qemu-system-common   # 或含 dev 头的 qemu 包"
    echo "      或把与本机 qemu-system-riscv64 同版本的 qemu-plugin.h 放到 $DIR/"
    exit 2
fi

echo "== 编译 watch_store_plugin.so =="
CFLAGS_PLUG="$(pkg-config --cflags qemu-plugin 2>/dev/null || echo "-I$DIR")"
LIBS_PLUG="$(pkg-config --libs qemu-plugin 2>/dev/null || echo '')"
cc -O2 -fPIC -shared $CFLAGS_PLUG "$C" -o "$SO" $LIBS_PLUG || {
    echo "FAIL: 编译失败——多半是本插件按 QEMU 8.2 API 写的个别符号名与本机头文件有出入"
    echo "      （尤其 qemu_plugin_get_vcpu_pc / qemu_plugin_get_hwaddr /"
    echo "        qemu_plugin_hwaddr_phys_addr / qemu_plugin_mem_rw_get_length / g_printf）；"
    echo "      对照 $HDR 实际原型微调 $C 后重编。"
    exit 1
}
echo "built: $SO"

echo
echo "== 挂进 riscv boot（把下面这行加进 tmp/nk4a 里 riscv 启动 qemu 参数，端口/日志沿用既有 harness）=="
echo "  -plugin file=$SO,arg=$TARGET"
echo "然后正常 boot；命中瞬态写者时插件打印："
echo "  WATCHSTORE cpu=.. pc=0x.. vaddr=0x.. paddr=$TARGET len=.."
echo "拿 pc 反汇：riscv64-unknown-elf-objdump -d --start-address=<pc-0x40> \\"
echo "   --stop-address=<pc+0x10> $DIR/../../os/target/riscv64gc-unknown-none-elf/release/minix-vm"
echo "（目标帧 halt 时净，插件是唯一能抓读瞬间写者的零暂停手段，见 WORKLOG §续-241~252。）"
