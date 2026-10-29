//! aarch64 自引导面（§续-406：QEMU `-kernel` ELF 直核引导的 Rust 面，§续-88
//! 甲案 riscv `bootface.rs` 的 aarch64 镜像）。
//!
//! `[ARCH: FW_PAYLOAD 直核形态（Minix3 无对应）]`：本面是 aarch64 的辅助
//! 启动路径，覆盖测试为主（QEMU `-kernel`）；真板启动一般仍经 UEFI/U-Boot。
//! 主线是独立 boot-shim + 独立内核镜像，落位矩阵见 01 篇 §3.7。
//!
//! # 引导模型（与 riscv 半的差异全部源自固件，不是设计分叉）
//!
//! QEMU mach-virt 对非 Linux ELF `-kernel` 走 `do_cpu_reset` 的
//! `!info->is_linux` 路径（hw/arm/boot.c）。**次级核不进 ELF 入口**：只有
//! first_cpu 被 `cpu_set_pc` 送进入口，其余保持复位悬挂，由内核经 QEMU
//! 内建 PSCI CPU_ON（conduit 按 DTB `/psci/method`，直核链＝hvc）交付
//! ——门读数为准：`test-smp-aps-aarch64` 在 `AP_GO` 写入为 0 的前提下
//! `ap-arrived=1 / ap-timeout=0`（`2026-10-18` 基线，交付通道＝PSCI 的
//! 实证）。因此：
//!
//! - BSP 由 `_start` 的早期选举格记录（MPIDR+1 存 [`BOOT_HART_ELECTED`]，
//!   与 riscv 半同格同名）；落选者本地 wfi 停车（SD-37 乙案：`AP_GO`
//!   停车邮箱已删——它以「全部 CPU 进入口」为前提，该前提被上读数
//!   否证；见本仓库 `PARALLEL-20261018-C-KERN-QEMU.md` C2 行）。
//! - DTB 由 QEMU `arm_load_dtb` 放在 RAM 基址（`info->dtb_start =
//!   loader_start`，hw/arm/boot.c ELF 臂源码锚定）＝[`DTB_PA`] 定值。
//! - `x0` 在直核入口为 0（QEMU 不设参）；boot-shim UEFI 链则以 `x0` 递
//!   `BootHandoff` 物理指针——`rust_image_main` 以 `x0==0` 分道两形。
//!
//! # 与 riscv bootface 的镜像关系
//!
//! 流程逐段同构（DTB→memmap→BootFileTable→bump 池→12 模块→KernelInfo→
//! `minix_kernel::arch_boot`）。本文件自包含（bootface.rs 被 riscv64 cfg
//! 门住，aarch64 构建不编译它）；两门全绿后的去重收敛留给独立清理增量，
//! 届时以两份串口输出逐行对账。
//!
//! # 字符串纪律（承 riscv 半 续-88 jiaf 链）
//!
//! MMU-off 物理视图下只有调用点直接物化的字面量可解引用；本面全部输出走
//! 定长字节缓冲＋`early_console::write_str`，不留 baked 指针腿。

use core::slice;

use minix_boot::{
    BootModule, DTB, KernelInfo, MemoryMapSource as _, MemoryRegion, NR_BOOT_MODULES,
    PlatformDescSource,
};
use minix_types::{PhysBytes, VirBytes};

use boot_shim::opensbi_helpers::{BootFileTable, build_kernel_info};

use minix_plat::arm64::early_console;

// ── 布局契约常量（与 aarch64.ld 同源：KERNEL_VIRT_BASE=0xFFFF800000000000、
//    KERNEL_PHYS_BASE=0x40200000；QEMU virt DRAM 基 0x40000000）──
//
// 本组是架构真值，与 riscv 半不同源，各留一份互指；两份共用的名字池与
// 纯函数闸在 `bootface_common`（N-4 处置）。

/// QEMU virt（aarch64）DRAM 基址。
const DRAM_BASE: u64 = 0x4000_0000;

/// 镜像物理基址（`aarch64.ld` `KERNEL_PHYS_BASE`：DRAM+2MiB，2MiB 对齐）。
const KERN_PHYS_BASE: u64 = 0x4020_0000;

/// 镜像链接高半基址（`aarch64.ld` `KERNEL_VIRT_BASE`）。
const KERN_VIRT_BASE: u64 = 0xFFFF_8000_0000_0000;

/// 镜像跨距（末段收口后的 LMA 尾差；riscv 半同值 4 MiB）。
const KERN_SIZE: u64 = 0x40_0000;

/// QEMU `arm_load_dtb` 对 ELF `-kernel` 的 DTB 落点＝RAM 基址
/// （`info->dtb_start = info->loader_start`，hw/arm/boot.c ELF 臂）。
const DTB_PA: u64 = DRAM_BASE;

/// bump 物理池基址（DRAM+64MiB；避开镜像 4MiB 与 DTB 页，32MiB 收口
/// 与 riscv 半同宽——root/页表 bump/名字池/12 模块取页都在其内）。
const BUMP_BASE: u64 = DRAM_BASE + 0x0400_0000;

/// bump 池宽 32MiB（riscv 半同宽）。
const BUMP_SIZE: u64 = 0x0200_0000;

/// 页表 bump 区页数（1 MiB，256 页；riscv 半同宽——VMSv9 四级与 Sv39
/// 的中间表用量同阶，池枯竭走显式 halt 臂）。
const PT_BUMP_PAGES: usize = 256;

// 名字池/槽宽与纯函数闸（`be_cell`/`in_memmap*`/`table_magic_ok`/
// `fdt_table_pa`）由两个自引导面共用，住在 `bootface_common`（N-4）。
use crate::bootface_common::{
    NAME_POOL, NAME_SLOT, fdt_table_pa, in_memmap, in_memmap_range, table_magic_ok,
};

// ── bump 池（aarch64 自有游标；opensbi_helpers 的池基是 riscv DRAM）──

/// 单调游标（.bss；与 opensbi_helpers 同纪律：池基/池尾/游标只有这一份）。
static mut BUMP_PTR: u64 = BUMP_BASE;

/// 取 `num_pages` 页；0 页请求拒绝（防重复地址破坏不重叠不变量）。
fn bump_alloc_pages(num_pages: usize) -> Option<u64> {
    assert!(num_pages > 0, "bump_alloc: num_pages must be > 0");
    // SAFETY: 单线程 boot 面（次级核经 PSCI CPU_ON 交付后停内核汇聚点，
    // 不进本面；落选者即使出现也本地 wfi 停车）。
    unsafe {
        let need = (num_pages as u64) * 4096;
        let ptr = core::ptr::addr_of_mut!(BUMP_PTR);
        if *ptr + need > BUMP_BASE + BUMP_SIZE {
            return None;
        }
        let addr = *ptr;
        *ptr += need;
        Some(addr)
    }
}

// ── .bss 装配格（boot 面独占写、随后只读）──────────────────────

/// 平台描述符源格（DTB 单源）。
static mut PLATFORM_SOURCE: PlatformDescSource = PlatformDescSource::new(DTB, PhysBytes(0));

/// 12 模块落地描述表。
static mut MODULE_TABLE: [BootModule; NR_BOOT_MODULES] = [const {
    BootModule { name: "", start: PhysBytes(0), len: 0 }
}; NR_BOOT_MODULES];

// ── 入口 ────────────────────────────────────────────────────────

/// aarch64 自引导面入口（`main.rs` aarch64 `_start` 清零 .bss、立栈、
/// `x0==0` 判道后 `bl` 进来）。
pub extern "C" fn bootface_a64() -> ! {
    core::hint::black_box(crate::KERNEL_ENTRY_ANCHOR as usize);
    say("### minix-rs kernel image: aarch64 self-boot (NK4-C sec-406)\n");

    // 1. DTB → memmap。QEMU `arm_load_dtb` 把 DTB 放在 RAM 基址（源码
    //    锚定，见模块头注）；40B 定长头取 totalsize，`dtb_bounded` 证整
    //    块落在 DRAM 形内后才建切窗（riscv 半 SF2 同款防越界）。
    // SAFETY: DTB_PA 由 QEMU 装载器写入且在本面之前稳定；读 40B 只取
    // 魔数/长度，真实使用范围由 `dtb_bounded` 界闸卡死。
    let head = unsafe { slice::from_raw_parts(DTB_PA as *const u8, 40) };
    if u32::from_be_bytes(head[0..4].try_into().unwrap_or([0; 4])) != 0xd00d_feed {
        say("### minix-rs kernel image: DTB magic bad — halting\n");
        crate::halt();
    }
    let fdt_total = u32::from_be_bytes(head[4..8].try_into().unwrap_or([0; 4])) as usize;
    if fdt_total < 40 || !dtb_bounded(DTB_PA, fdt_total) {
        say("### minix-rs kernel image: DTB blob escapes DRAM bound — halting\n");
        crate::halt();
    }
    // SAFETY: `dtb_bounded` 已证整块落在 DRAM 形内；MMU-off 恒等视图可读。
    let fdt_bytes = unsafe { slice::from_raw_parts(DTB_PA as *const u8, fdt_total) };
    let fdt = match fdt::Fdt::new(fdt_bytes) {
        Ok(f) => f,
        Err(_) => {
            say("### minix-rs kernel image: DTB parse failed — halting\n");
            crate::halt();
        }
    };

    // I-22 丁案 / D-68：memmap 源统一到 minix-boot 的 `DtbMemoryMap`
    // （与 opensbi 腿同一 impl，取件车道的第四链收口）。`fw_end = 0` 是
    // 本形态前提：aarch64 直核链的 DRAM 里没有固件常驻区（QEMU 只放 DTB
    // 与镜像），因此不削界；DTB 自身由内核 Phase A.2 按真实 totalsize
    // 永久扣除——两条机制不替岗：本处只产出语义列表，扣除是内核记账。
    let (memmap, _reserved) = minix_boot::DtbMemoryMap::new(&fdt, 0).collect();
    if memmap.is_empty() {
        say("### minix-rs kernel image: no usable /memory reg in DTB — halting\n");
        crate::halt();
    }
    say("  memmap regions 0x");
    say_hex(memmap.len() as u64);
    say(" first 0x");
    say_hex(memmap[0].base.0);
    say("+0x");
    say_hex(memmap[0].len as u64);
    say("\n");

    // 2. BootFileTable：DTB /chosen 通道，先过 DRAM 界闸再过 magic 闸
    //    才解引用（riscv 半 M4.3 纪律同款）。
    let table_pa = fdt_table_pa(&fdt).filter(|pa| in_memmap(memmap, *pa) && table_magic_ok(*pa));
    let Some(table_pa) = table_pa else {
        say("### minix-rs kernel image: no BootFileTable (chosen) — halting\n");
        crate::halt();
    };
    say("  boot file table pa 0x");
    say_hex(table_pa);
    say("\n");
    // SAFETY: `table_magic_ok` 已在 memmap 界内核对 magic；表由门装配工
    // 具放在启动期独占的物理 RAM，本面是首个也是唯一读者。
    let table = unsafe { &*(table_pa as *const BootFileTable) };

    // 3. bump 物理池：root 页 → 页表 bump 区 → 名字池（顺序无重叠；
    //    Option+诚实停机，不留 expect/panic 腿——riscv 半 SF5 同款）。
    let pool_exhaust = "### minix-rs kernel image: bump pool exhausted — halting\n";
    let Some(cur_root) = bump_alloc_pages(1) else { say(pool_exhaust); crate::halt() };
    let root_page = PhysBytes(cur_root);
    let Some(pt_base) = bump_alloc_pages(PT_BUMP_PAGES) else { say(pool_exhaust); crate::halt() };
    let pt_end = pt_base + (PT_BUMP_PAGES as u64) * 4096;
    let Some(names_pool) = bump_alloc_pages(1) else { say(pool_exhaust); crate::halt() };
    minix_kernel::boot_alloc::init_boot_pt_alloc(pt_base, pt_end);
    minix_arch::pt_alloc::register(minix_kernel::boot_alloc::boot_pt_alloc);
    say("  root 0x");
    say_hex(root_page.0);
    say(" pt bump 0x");
    say_hex(pt_base);
    say("..0x");
    say_hex(pt_end);
    say("\n");

    // 4. 12 模块：表定位（名对位校验）→ 源直拷进 bump 页。
    if place_modules(table, memmap, names_pool).is_none() {
        say("### minix-rs kernel image: module placement failed — halting\n");
        crate::halt();
    }
    // SAFETY: `place_modules` 刚按契约序写满 12 槽；此后只读。
    let modules: &'static [BootModule] = unsafe { &*core::ptr::addr_of!(MODULE_TABLE) };
    say("  boot modules placed 12\n");

    // 5. 平台描述符源：DTB 单源（槽位住 .bss）。
    // SAFETY: boot 面单 hart，写一次后只读。
    unsafe {
        *core::ptr::addr_of_mut!(PLATFORM_SOURCE) =
            PlatformDescSource::new(DTB, PhysBytes(DTB_PA));
    }
    // SAFETY: 同上——写完之后的静态引用，数组单格与切片长度 1 同源。
    let platform_sources: &'static [PlatformDescSource] = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(PLATFORM_SOURCE), 1)
    };

    // 6. KernelInfo + arch_boot 真门槛（validate → 建根 → DM →
    //    jump_to_kmain，不再返回本面）。
    let kernel_info: KernelInfo = build_kernel_info(
        memmap,
        VirBytes(KERN_VIRT_BASE),
        PhysBytes(KERN_PHYS_BASE),
        KERN_SIZE,
        modules,
        PhysBytes(0),
        0,
        platform_sources,
    );
    say("  calling arch_boot (validate gate)\n");
    minix_kernel::arch_boot(&kernel_info, root_page)
}

// ── DTB 面 ──────────────────────────────────────────────────────

/// 读 `/chosen` 的 `opensbi,boot-file-table`（大端 cell；4/8 字节都认；
/// 属性名沿用 riscv 门装配工具的同一约定，门脚本 fdtput 同形）。
///
/// 查找走 `all_nodes` 而非 `find_node`（§续-406 定谳：fdt 0.1.5 的
/// `find_node` 对 QEMU `-kernel` 直核链重建出的 blob 返回 None——
/// all_nodes 与宿主 python 解析都能看到 /chosen，find_node 看不到；
/// 名对位用「去 @ 后缀」比对，与 find_node 的宽松语义一致）。
/// DTB 区块界闸：totalsize 自描述，但整块必须落在 DRAM 形内。
fn dtb_bounded(dtb_pa: u64, total: usize) -> bool {
    dtb_pa >= DRAM_BASE && dtb_pa + total as u64 <= 0x1_0000_0000
}

/// 12 模块的落地描述表（`.bss`，装配后只读）。
///
/// 位置为主、名为辅／源区间三道闸／名字入池——语义与 riscv 半
/// `place_modules` 逐条同形（续-89 直拷定案、SF3/SF4 全宽与对位闸），
/// 不在此复述；差异仅 bump 池基（aarch64 形）。
fn place_modules(
    table: &BootFileTable,
    memmap: &[MemoryRegion],
    names_pool: u64,
) -> Option<()> {
    let entries = table.valid_entries();
    if entries.len() < NR_BOOT_MODULES {
        say("  table holds 0x");
        say_hex(entries.len() as u64);
        say(" entries, kernel contract needs 0x");
        say_hex(NR_BOOT_MODULES as u64);
        say("\n");
        return None;
    }
    for i in 0..NR_BOOT_MODULES {
        let entry = &entries[i];
        let expect = &NAME_POOL[i];
        let want_n = expect.iter().position(|&b| b == 0).unwrap_or(NAME_SLOT);
        let base = entry
            .path
            .iter()
            .rposition(|&b| b == b'/')
            .map_or(0, |p| p + 1);
        let got = &entry.path[base..];
        let got_n = got.iter().position(|&b| b == 0).unwrap_or(got.len());
        if got_n != want_n || got[..want_n] != expect[..want_n] {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" path basename mismatches contract order\n");
            return None;
        }
        let len = entry.len as usize;
        if len == 0 {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" length 0\n");
            return None;
        }
        if !in_memmap_range(memmap, entry.phys_addr, entry.len) {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" source 0x");
            say_hex(entry.phys_addr);
            say("+0x");
            say_hex(entry.len);
            say(" escapes memmap\n");
            return None;
        }
        if entry.phys_addr < BUMP_BASE + BUMP_SIZE && entry.phys_addr + entry.len >= BUMP_BASE {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" source overlaps bump pool\n");
            return None;
        }
        let pages = len.div_ceil(4096);
        let dest = bump_alloc_pages(pages)?;
        // SAFETY: 源刚过 memmap 全宽闸且与 bump 池不相交；dest = 独占
        // bump 页（游标单调），拷贝长度 ≤ 两端区间长。
        unsafe {
            core::ptr::copy_nonoverlapping(entry.phys_addr as *const u8, dest as *mut u8, len);
            core::ptr::write_bytes((dest as *mut u8).add(len), 0, pages * 4096 - len);
        }
        let slot = names_pool + (i as u64) * NAME_SLOT as u64;
        // SAFETY: 源 = NAME_POOL 字节格（base 调用点物化）；目的 = 名字池
        // 第 i 槽（i < 12 ⇒ 偏移 < 一页）。
        unsafe {
            core::ptr::copy_nonoverlapping(NAME_POOL[i].as_ptr(), slot as *mut u8, NAME_SLOT);
        }
        let n = NAME_POOL[i].iter().position(|&b| b == 0).unwrap_or(NAME_SLOT);
        let landed: &'static str = unsafe {
            // SAFETY: 字节来自 ASCII 字面量池；backing 是独占物理页。
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(slot as *const u8, n))
        };
        // SAFETY: 单 hart boot 面逐槽写一次后只读。
        unsafe {
            MODULE_TABLE[i] = BootModule {
                name: landed,
                start: PhysBytes(dest),
                len,
            };
        }
    }
    Some(())
}

// ── 串口面（无堆：定长字符串 + 手写 hex；PL011 恒等 PA 路径）──

fn say(msg: &str) {
    early_console::write_str(msg);
}

fn say_hex(v: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        buf[i] = HEX[((v >> ((15 - i) * 4)) & 0xf) as usize];
        i += 1;
    }
    early_console::write_str(core::str::from_utf8(&buf).unwrap_or(""));
}
