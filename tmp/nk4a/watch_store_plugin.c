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
#define DM_HI 0x0000001400000000ULL   /* VM 直map窗：DM_LO + 16GiB */
#define KDM_LO 0xFFFFFFC040000000ULL
#define KDM_HI 0xFFFFFFC040000000ULL  /* 内核直map窗基址（direct_map.rs）; 下方按 +16GiB 算界 */
#define KDM_TOP 0x400000000ULL
#define RING 4096

static uint64_t g_base[MAX_TARGETS];
static int      g_n;
static bool     g_dmwin = true;
static unsigned long g_ev;             /* 累计事件数 */

/* 环形缓冲：只留最近 RING 条命中（避免逐条 fprintf 的 I/O 洪水与扰动）；
   崩溃后 harness 杀 qemu → atexit 一次性 flush，拿到崩溃前最后这段写者时间线。*/
struct ev { uint64_t vaddr, paddr, pc; uint32_t cpu : 8, store : 1, sz : 8, kind : 3; };
static struct ev g_ring[RING];
static unsigned  g_head, g_cnt;

static void rec(int kind, unsigned cpu, bool store, unsigned sz,
                uint64_t vaddr, uint64_t paddr, uint64_t pc)
{
    struct ev *e = &g_ring[g_head];
    e->vaddr = vaddr; e->paddr = paddr; e->pc = pc;
    e->cpu = cpu; e->store = store ? 1 : 0; e->sz = sz; e->kind = kind;
    g_head = (g_head + 1) % RING;
    if (g_cnt < RING) g_cnt++;
    g_ev++;
}

static void flush_cb(qemu_plugin_id_t id, void *ud)
{
    (void)id; (void)ud;
    fprintf(stderr,
        "\nwatch_store_plugin: FLUSH total_dmwin_stores_seen=%lu, last %u:\n",
        g_ev, g_cnt);
    unsigned start = (g_cnt < RING) ? 0 : g_head;   /* 最旧一条 */
    static const char *kname[4] = { "FRAME", "DMWIN", "FAULT", "QPC" };
    for (unsigned i = 0; i < g_cnt; i++) {
        struct ev *e = &g_ring[(start + i) % RING];
        fprintf(stderr, "PLG %s vcpu=%u %s sz=%u vaddr=0x%016"
            PRIx64 " paddr=0x%016" PRIx64 " slot=%ld pc=0x%08"
            PRIx64 "\n",
            kname[e->kind & 3], e->cpu, e->store ? "W" : "R", e->sz,
            e->vaddr, e->paddr, (long)((e->paddr >> 3) & 511), e->pc);
    }
    fflush(stderr);
}

static int in_target_frame(uint64_t pa)
{
    for (int i = 0; i < g_n; i++)
        if (pa >= g_base[i] && pa < g_base[i] + 4096) return i;
    return -1;
}

static bool in_a_win(uint64_t v)
{
    return (v >= DM_LO && v < DM_HI)
        || (v >= KDM_LO && v < KDM_LO + KDM_TOP);
}

/* 崩点函数 Riscv64Paging::query 的 guest PC 区间（当前 build 0x3ab58..0x3ac30）。
   记录该区间内每一次访存的精确 vaddr+pc，用于判：一条反汇证明只能产
   生 8 对齐地址的 query 槽读，其真实执行的 vaddr 到底对不对齐（定候选
   (i) TCG stval 语义 vs (ii) 真有非对齐访问）。 */
static bool in_query_pc(uint64_t pc)
{
    return pc >= 0x3ab58ULL && pc <= 0x3ac30ULL;
}

static void vcpu_mem(unsigned int cpu, qemu_plugin_meminfo_t info,
                     uint64_t vaddr, void *ud)
{
    bool store = qemu_plugin_mem_is_store(info);
    unsigned sz = 1u << qemu_plugin_mem_size_shift(info);
    uint64_t pc = (uint64_t)(uintptr_t)ud;

    struct qemu_plugin_hwaddr *hw = qemu_plugin_get_hwaddr(info, vaddr);
    if (!hw) {
        /* 未翻译成物理地址 = 该访存本身故障（缺页）。若虚拟地址落在任一直接映射窗
           （VM 或内核），这条就是崩点访问：记下 FAULT（含精确 pc+vaddr），
           用于判 0xB2C 偏移是走表索引还是调用者游标（侧对话点1）。*/
        if (in_a_win(vaddr) || in_query_pc(pc))
            rec(in_query_pc(pc) ? 3 : 2, cpu, store, sz, vaddr, 0, pc);
        return;
    }
    if (qemu_plugin_hwaddr_is_io(hw)) return;
    uint64_t pa = qemu_plugin_hwaddr_phys_addr(hw);

    /* query PC 区间内的每一次读/写（成功）：拿其精确 vaddr 定对齐性。 */
    if (in_query_pc(pc)) {
        rec(3, cpu, store, sz, vaddr, pa, pc);
        return;
    }

    if (in_target_frame(pa) >= 0) {                 /* 次过滤：显式帧（R+W）*/
        rec(0, cpu, store, sz, vaddr, pa, pc);
        return;
    }
    /* 主漏斗：任一直map窗（VM 或内核）内的 8 字节 store = 一枚 PTE 写。*/
    if (g_dmwin && store && sz == 8 && in_a_win(vaddr))
        rec(1, cpu, store, sz, vaddr, pa, pc);
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
    qemu_plugin_register_atexit_cb(id, flush_cb, NULL);

    fprintf(stderr, "watch_store_plugin: armed dmwin=%s frames=%d [",
            g_dmwin ? "on" : "off", g_n);
    for (int i = 0; i < g_n; i++)
        fprintf(stderr, "%s0x%" PRIx64, i ? "," : "", g_base[i]);
    fprintf(stderr, "]\n");
    fflush(stderr);
    return 0;
}
