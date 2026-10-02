/*
 * NK4-C (A) 瞬态页表写者抓取插件（QEMU 8.2 plugin API，mem-write 观察）
 *
 * 背景（见 notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md §续-241~252）：
 * 缺陷 (A) 的坏页表项【只在被读的那一瞬间存在、停机即净】，本机 QEMU riscv TCG 的
 * 硬件写观察点不触发、-S/加读探针都会把崩溃点漂移。唯一还能【零暂停、不改 guest 码】
 * 抓到「谁在往这个物理地址写」的办法，是 QEMU 插件系统：在每条访存指令上挂回调，
 * 命中目标物理地址的【写】就打印发起方 vCPU 的 guest PC —— 不暂停被观测系统。
 *
 * 目标物理地址（--arg 传入，缺省 (A) 子根槽）：0x9dc377f8
 *
 * 用法（待 qemu-plugin.h 就绪后编译，见同目录 watch_store_plugin.build.sh）：
 *   qemu-system-riscv64 -object memory-backend-* ... \
 *     -plugin file=watch_store_plugin.so,arg=0x9dc377f8 ...
 *   命中时串口/日志打印 "WATCH ep=<pc> vaddr=<v> paddr=<p> len=<n>"，
 *   再 objdump minix-vm 反汇 <pc> 定写者函数（预期落 exec 装填 / sync_slot_pte 腿）。
 *
 * ⚠ 本文件【未在本机编译验证】（缺 qemu-plugin.h）。装好头文件后先 `--cflags`
 *    编一遍；若 QEMU 8.2 的 API 名/签名与此处有出入（尤其 qemu_plugin_get_hwaddr /
 *    qemu_plugin_hwaddr_phys_addr / qemu_plugin_get_vcpu_pc），按头文件实际原型微调。
 */
#include <qemu-plugin.h>
#include <stdio.h>
#include <stdlib.h>
#include <inttypes.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;

static guint64 g_target;       /* 目标 guest 物理地址（页表帧内某 8 字节槽） */
static unsigned g_hits;
static const unsigned g_hit_cap = 16;

/* 访存回调：只关心【写】命中目标物理地址。info 带 rw 元数据，vaddr 是客户机虚拟地址。*/
static void vcpu_mem(unsigned int cpu, qemu_plugin_meminfo_t info,
                     uint64_t vaddr, void *ud)
{
    (void)ud;
    if (!qemu_plugin_mem_is_store(info)) {
        return;
    }
    qemu_plugin_hwaddr *hw = qemu_plugin_get_hwaddr(info, vaddr);
    if (!hw || qemu_plugin_hwaddr_is_io(hw)) {
        return;
    }
    uint64_t paddr = qemu_plugin_hwaddr_phys_addr(hw);
    /* 允许命中整个 4K 帧或精确到 8 字节槽：这里按精确目标地址 + 相邻 8 字节。*/
    if (paddr >= g_target && paddr < g_target + 8) {
        if (g_hits < g_hit_cap) {
            /* guest PC 定位写者指令；若该 API 在本版本取不到，退化为只报 vaddr/paddr。*/
            uint64_t pc = qemu_plugin_get_vcpu_pc(cpu);
            g_printf("WATCHSTORE cpu=%u pc=0x%" PRIx64 " vaddr=0x%" PRIx64
                     " paddr=0x%" PRIx64 " len=%u\n",
                     cpu, pc, vaddr, paddr,
                     (unsigned)qemu_plugin_mem_rw_get_length(info));
            g_hits++;
        }
    }
}

static void qemu_plugin_outs_noop(void) { /* g_printf 由 plugin 输出通道承接 */ }

QEMU_PLUGIN_EXPORT
void qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info,
                         int argc, char **argv)
{
    (void)id; (void)info; (void)qemu_plugin_outs_noop;
    g_target = 0x9dc377f8;               /* 缺省：(A) 子根 i2=255 槽 */
    if (argc >= 1) {
        g_target = g_ascii_strtoull(argv[0], NULL, 0);
    }
    /* QEMU_PLUGIN_MEM_W = 只订阅写；对全 vCPU、全代码块注册。*/
    qemu_plugin_register_vcpu_mem_cb(id, vcpu_mem, QEMU_PLUGIN_MEM_W, NULL);
    g_printf("watch_store_plugin: armed on paddr 0x%" PRIx64 "\n", g_target);
}
