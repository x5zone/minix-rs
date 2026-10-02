/*
 * NK4-C (A) 页表写瞬态观察插件 —— 按【本机真实 qemu-plugin.h(QEMU 8.2.2)】API 写。
 *
 * 模型：translation cb → 枚举 TB insn → 对每条 insn 注册 per-insn memory cb，把该
 * insn 的 guest PC(qemu_plugin_insn_vaddr) 经 userdata 传入 mem cb。零暂停、不改 guest 码。
 *
 * 主过滤（宽口径漏斗，采纳侧对话③）：任何【store 且访问虚拟地址落在 VM 直接映射窗
 * [0x1000000000, 0x1000000000+0x4000000000)】的事件——用户态进程写【任何】页表项都
 * 必经这扇窗（channel_to_ptr→vm_phys_to_virt）。这样即便还没枚举到真正的 L0/L1 帧也漏不掉。
 *   窗基址/容量：os/arch/src/arch/direct_map.rs Riscv64DirectMap（VM_DIRECT_MAP_BASE=0x10_0000_0000，
 *   VM_DIRECT_MAP_SIZE=0x4_0000_0000=16GiB）。8 字节 store = 一枚 PTE 宽。
 * 次过滤（精确）：显式帧列表（argv 逗号；或默认若干帧）按【物理 paddr】命中也记。
 *
 * 输出走 fprintf(stderr)（侧对话①：qemu_plugin_outs 需 -d plugin 才可见，stderr 不受此限）。
 * 用法：qemu-system-riscv64 ... -plugin file=watch_store_plugin.so[,arg=0x9DC37000,...] ...
 *   不带 arg 即只用 DM 窗漏斗 + 内置默认帧。
 */
#include <qemu-plugin.h>
#include <stdint.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;

#define MAX_TARGETS 24
#define DM_LO 0x0000001000000000ULL
#define DM_HI 0x0000001400000000ULL   /* DM_LO + 16GiB */

static uint64_t g_base[MAX_TARGETS];
static int      g_n;
static bool     g_dmwin = true;
static unsigned long g_ev;
static const unsigned long EVENT_CAP = 200000;

static void emit(const char *kind, unsigned int cpu, bool store, unsigned sz,
                 uint64_t vaddr, uint64_t paddr, uint64_t pc)
{
    if (g_ev++ >= EVENT_CAP) return;
    char buf[256];
    snprintf(buf, sizeof buf,
        "PLG %s ev=%lu vcpu=%u %s sz=%u vaddr=0x%016" PRIx64
        " paddr=0x%016" PRIx64 " slot=%ld pc=0x%08" PRIx64 "\n",
        kind, g_ev, cpu, store ? "W" : "R", sz, vaddr, paddr,
        (long)((paddr >> 3) & 511), pc);
    fprintf(stderr, "%s", buf);
    fflush(stderr);
}

static int in_target_frame(uint64_t pa)
{
    for (int i = 0; i < g_n; i++)
        if (pa >= g_base[i] && pa < g_base[i] + 4096) return i;
    return -1;
}

static void vcpu_mem(unsigned int cpu, qemu_plugin_meminfo_t info,
                     uint64_t vaddr, void *ud)
{
    bool store = qemu_plugin_mem_is_store(info);
    unsigned sz = 1u << qemu_plugin_mem_size_shift(info);
    uint64_t pc = (uint64_t)(uintptr_t)ud;

    struct qemu_plugin_hwaddr *hw = qemu_plugin_get_hwaddr(info, vaddr);
    if (!hw || qemu_plugin_hwaddr_is_io(hw)) return;
    uint64_t pa = qemu_plugin_hwaddr_phys_addr(hw);

    /* 次过滤：显式帧（按物理地址）——精确，读+写都记。 */
    int fi = in_target_frame(pa);
    if (fi >= 0) {
        emit("FRAME", cpu, store, sz, vaddr, pa, pc);
        return;
    }
    /* 主漏斗：DM 窗内的 8 字节 store = 一枚 PTE 写（不依赖枚举帧）。 */
    if (g_dmwin && store && sz == 8 && vaddr >= DM_LO && vaddr < DM_HI)
        emit("DMWIN", cpu, store, sz, vaddr, pa, pc);
}

static void vcpu_tb_trans(qemu_plugin_id_t id, struct qemu_plugin_tb *tb)
{
    (void)id;
    size_t n = qemu_plugin_tb_n_insns(tb);
    for (size_t j = 0; j < n; j++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, j);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        qemu_plugin_register_vcpu_mem_cb(insn, vcpu_mem, QEMU_PLUGIN_CB_NO_REGS,
                                         QEMU_PLUGIN_MEM_RW, (void *)(uintptr_t)pc);
    }
}

/* 解析逗号分隔 hex 帧列表；容忍 QEMU loader 改写的 "0x...=on" 形状（侧对话②）。 */
static void parse_targets(const char *line)
{
    for (const char *p = line; *p && g_n < MAX_TARGETS; ) {
        while (*p == ',' || *p == ' ') p++;
        if (*p != '0' || (p[1] != 'x' && p[1] != 'X')) break;
        char *end = NULL;
        uint64_t v = strtoull(p, &end, 16);
        if (end == p) break;
        g_base[g_n++] = v & ~0xFFFULL;
        p = end;
    }
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,
                                           const qemu_info_t *info,
                                           int argc, char **argv)
{
    (void)info;
    for (int a = 0; a < argc; a++)
        if (argv[a] && argv[a][0]) parse_targets(argv[a]);

    if (g_n == 0) {
        /* 默认帧：栈缺页 walk 链（T2）——子根 / L1 / L0 + bogus 叶子基址帧。*/
        g_base[0]=0x9DC37000ULL; g_base[1]=0x9d2ad000ULL;
        g_base[2]=0x9d2ac000ULL; g_base[3]=0x9d28c000ULL; g_n=4;
    }
    qemu_plugin_register_vcpu_tb_trans_cb(id, vcpu_tb_trans);

    fprintf(stderr, "watch_store_plugin: armed dmwin=%s frames=%d [",
            g_dmwin ? "on" : "off", g_n);
    for (int i = 0; i < g_n; i++)
        fprintf(stderr, "%s0x%" PRIx64, i ? "," : "", g_base[i]);
    fprintf(stderr, "]\n");
    fflush(stderr);
    return 0;
}
