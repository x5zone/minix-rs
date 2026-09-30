//! NK4-C 续-88 甲案（riscv64 装机面）：kernel-image 自当引导体。
//!
//! 裁决源：`notes/rewrite/fork-syscall-rewrite/NK4C-OPENING-PROMPT.md` 阶段
//! 3.1——「kernel-image riscv64 接 a1 DTB → 解 memmap + 模块装载源 + .bss
//! 清零 → 调 arch_boot（过 validate 真门槛）」。本模块是那条裁决的 Rust 面
//! 落地（`.bss` 清零在 `main.rs` 的入口 asm；甲案三件缺件的另外两半都在
//! 这里）：
//!
//! 1. **memmap**：OpenSBI 递入 `a1` 的 DTB 物理址解析 `/memory` 节点的
//!    `reg`——不再抄 `opensbi_helpers::build_memmap` 的单区硬编码（那是把
//!    QEMU 拓扑写进产物的测试形状，NK4B M4.4 事实三已判「生产不可用」）。
//! 2. **模块装载源**：`BootFileTable`（magic `MNXBOOT1` 把关）。两个通道：
//!    U-Boot 腿的 `a2` 寄存器约定，与 DTB `/chosen` 的
//!    `opensbi,boot-file-table` 属性（后者让 OpenSBI `-kernel` 直载 +
//!    `-device loader` 注入也能全链自测，不必等 xtask 装机腿）。表项携
//!    U-Boot `fatload` 放好的物理址/长度，本面按契约序（`MODULE_NAMES` =
//!    C `table.c:44-64` 的 12 名；位置为主、表内名对期望字面量校验为辅）
//!    把模块字节搬进 bump 物理页。
//! 3. **arch_boot 真门槛**：组 `KernelInfo`（复用 boot-shim 库的
//!    `build_kernel_info` 车道：Sv39 `user_sp`、`bootstrap=(0,0)` no-op
//!    回收语义）后调 `minix_kernel::arch_boot`——走 `validate()` 与
//!    `jump_to_kmain`，这是三份仓内载体（hello-boot / test-rt-birth 等）
//!    都没碰过的那半（NK4B M4.4 事实三）。
//!
//! 执行视图不变量（M4.3 实测钉死）：riscv64 medany 下镜像内引用全部被
//! 松弛成 PC 相对，OpenSBI 跳的是**最低物理装载址**（`Domain0 Next
//! Address` = PT_LOAD 最低 `p_paddr`），所以本模块运行在「链接期 VMA −
//! 运行期 PA = 0xFFFFFFC000000000 − 0x80200000」的平移视图里：Rust 取到
//! 的一切地址值 = 物理址（satp=0 恒等可访），DTB、表、模块字节都按物理
//! 指针直读；`KernelInfo` 里的 `kern_virt_base` / `kern_stack_top` 用
//! `.ld` 常量（高半视图值），它们只在 `arch_boot` 建表并 `enable()` 之后
//! 才被解引用——顺序由 `arch_boot` 自身保证。
//!
//! 与 boot-shim `OpenSbiBootShim::prepare_boot` 的分界：那条腿是「独立
//! shim 装 kernel.elf 再跳」，会重新拷贝**本镜像自己的段**并清 NOBITS——
//! 对自引导镜像是自我破坏；本面只搬**模块**、绝不重拷自己。模块拷贝走
//! 物理 bump 暂存页而非堆 `Vec`（堆只有 `IMAGE_HEAP` 一格 1 MiB，而 U-Boot
//! 字节本就在 RAM——直读直拷少一层，也不受堆宽约束）。

use core::slice;

use minix_boot::{
    BootModule, KernelInfo, MemoryRegion, NR_BOOT_MODULES, PlatformDescSource, DTB,
};
use minix_types::{PhysBytes, VirBytes};

use boot_shim::opensbi_helpers::{
    BootFileTable, boot_file_table_magic, bump_alloc_pages, bump_end_bound, bump_pool_base,
    build_kernel_info,
};
use boot_shim::loader::MODULE_NAMES;

use crate::early_console;

// 甲案的字符串纪律（续-88 jiaf3–jiaf9 真机取证链定枢）：satp=0 物理视图下，
// 只有「调用点直接物化的字面量」（`auipc` 松弛）可解引用；任何经数据格
// 的 `&str`——静态胖指针、match 切换表（`.Lswitch.table` 携 baked VMA，
// jiaf5/jiaf6 反证实锤）——解引用即 Load access fault → OpenSBI 热重入。
// 名单因此以定长字节池（纯数据，无 baked 指针）+字面量校验形状存在；
// `BootModule.name` 的 backing 必须是物理名字池（内核 `store_kernel_info`
// 在物址腿读它，kernel/src/lib.rs:734）。
const _: () = assert!(NR_BOOT_MODULES == 12);

/// 12 名 × 16B 定长字节池（顺序 = C `table.c:44-64` 权威序 = `MODULE_NAMES`，
/// 字节池是纯数据（无 baked 指针），base 经调用点 `auipc` 物化，物理视图
/// 下按字节读安全；`[u8;16]` 而非 `&str` 正是为了不在数据格里存任何指针。
const NAME_POOL: [[u8; NAME_SLOT]; NR_BOOT_MODULES] = [
    *b"ds\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"rs\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"pm\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"sched\0\0\0\0\0\0\0\0\0\0\0",
    *b"vfs\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"memory\0\0\0\0\0\0\0\0\0\0",
    *b"tty\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"mib\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"vm\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"pfs\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"mfs\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"init\0\0\0\0\0\0\0\0\0\0\0\0",
];

/// 名字槽宽（= 内核 `BOOT_MODULE_NAME_LEN` 同阶 16B）。
const NAME_SLOT: usize = 16;

/// 字节池与契约名单的编译期逐字节对账（CodeReview SF6：虚构的「宿主测试」
/// 不存在——bootface 在宿主面根本不编译，名字序此前零护栏）。对账发生在
/// 真会坏的构建面（riscv 生产配方）：池槽前缀必须等于 `MODULE_NAMES[i]`
/// 字节、紧随零终止（与内核 `store_kernel_info` 的 min(15) 截断语义相容：
/// 最长名 `memory`=6 字节，截断永不发生）。
const _: () = {
    let mut i = 0;
    while i < NR_BOOT_MODULES {
        let want = MODULE_NAMES[i].as_bytes();
        let got = &NAME_POOL[i];
        assert!(want.len() < NAME_SLOT, "module name exceeds NAME_SLOT");
        let mut j = 0;
        while j < want.len() {
            assert!(got[j] == want[j], "NAME_POOL order drifts from MODULE_NAMES");
            j += 1;
        }
        assert!(got[want.len()] == 0, "NAME_POOL slot not NUL-terminated");
        i += 1;
    }
};

// ── 布局契约常量（与 riscv64.ld 同源；check-layout.sh L0/L9/A1 与真机
//    `Domain0 Next Address` 对账防漂移）──

/// QEMU virt DRAM 基址（DTB/表界闸的下界；与 `riscv64.ld` 头注、
/// `opensbi_helpers::DRAM_BASE` 同值，L0/A1 已护基址不漂）。
const DRAM_BASE: u64 = 0x8000_0000;

/// 镜像物理基址（`riscv64.ld` `KERNEL_PHYS_BASE`：装载器按 PT_LOAD 的
/// LMA 放置，跨距 2MiB 收口同脚本）。
const KERN_PHYS_BASE: u64 = 0x8020_0000;

/// 镜像链接高半基址（`riscv64.ld` `KERNEL_VIRT_BASE`）。
const KERN_VIRT_BASE: u64 = 0xFFFF_FFC0_0000_0000;

/// 镜像跨距（= 末段 2MiB 收口后的 LMA 尾差，当前工件 4 MiB）。CodeReview
/// BLOCKER-1：硬写 2MiB 副本时 `IMAGE_HEAP` 扩容已把真实跨距推到 4MiB——
/// 高半会少映半张镜像、`kern_stack_top` 落在 .bss 中段（静默腐坏）。本
/// 字面量是单一真值源，`check-layout.sh` L9 拿 ELF 实测跨距与之对账（堆
/// 宽变→跨距变→L9 当场红）；不另在 `.ld` 开符号节（曾试，多一个 PT_LOAD
/// 把跨距推成非 2MiB 整数倍，反破 L5/L7）。
const KERN_SIZE: u64 = 0x40_0000;

/// boot 暂存页预算已取消（续-89）：早期两段式拷贝的 1 MiB scratch 与模块
/// 尺寸上限均由「源直拷 + `in_memmap_range` 全宽闸」取代，见 `place_modules`。

/// 页表 bump 区页数：1 MiB（256 页，= boot-shim 库 fallback 同宽）。Sv39
/// 建根+身份低半+高半+DM 两窗的中间表用量在 QEMU virt 单段 memmap 下
/// 实测未及此数；池枯竭走显式 halt 臂（CodeReview SF5：不留 `.expect`
/// ——panic 渲染腿携 baked-VMA fmt pieces，物址视图里正好踩本模块定枢）。
const PT_BUMP_PAGES: usize = 256;

/// memmap 条目上限（QEMU virt / 真板的多段 `/memory` 都远少于此）。
const MEMMAP_MAX: usize = 8;

// ── 入口 ────────────────────────────────────────────────────────

/// 甲案 Rust 面入口（`main.rs` riscv64 `_start` 清零 .bss、立栈后
/// `call` 进来；a0/a1/a2 依 RV64 ABI 落为三个入参，`call` 不动它们）。
pub extern "C" fn bootface(boot_hart: u64, dtb_pa: u64, table_pa_a2: u64) -> ! {
    // 触达锚点：运行路径上引用一次，保证内核启动图在归档成员粒度上
    // 也被拉入链接（链接脚本 KEEP 是第二道保险）。
    core::hint::black_box(crate::KERNEL_ENTRY_ANCHOR as usize);
    say("### minix-rs kernel image: riscv64 self-boot (NK4-C jia-an)\n");
    say("  boot hart 0x");
    say_hex(boot_hart);
    say(" dtb 0x");
    say_hex(dtb_pa);
    say("\n");

    // 1. DTB → memmap（OpenSBI 规范保证 a1 递 DTB；为 0 说明固件配置
    //    异常，诚实停机）。
    if dtb_pa == 0 {
        say("### minix-rs kernel image: OpenSBI passed no DTB in a1 — halting\n");
        crate::halt();
    }
    // SAFETY: `dtb_pa` 是 OpenSBI 放在 DRAM 高段的 FDT 物理址（规范
    // 承诺 + M4.3 固件串口实证 `Domain0 Next Arg1`）；satp=0 恒等视图
    // 可读。CodeReview SF2：不按 2MiB 假窗建切窗（窗尾会越出 DRAM 顶），
    // 先读 40B 定长头取 totalsize，`fdt_bounded` 证整块落在 memmap 内
    // 后才按真实宽度建切窗。
    // SAFETY: 40B 头部——`dtb_pa` 非零且 OpenSBI 保证 blob 在 DRAM 内，
    // 读前 40B 仅取魔数/长度字段；真正的使用范围由下面的区间闸卡死。
    let head = unsafe { slice::from_raw_parts(dtb_pa as *const u8, 40) };
    if u32::from_be_bytes(head[0..4].try_into().unwrap_or([0; 4])) != 0xd00d_feed {
        say("### minix-rs kernel image: DTB magic bad — halting\n");
        crate::halt();
    }
    let fdt_total = u32::from_be_bytes(head[4..8].try_into().unwrap_or([0; 4])) as usize;
    if fdt_total < 40 || !dtb_bounded(dtb_pa, fdt_total) {
        say("### minix-rs kernel image: DTB blob escapes DRAM bound — halting\n");
        crate::halt();
    }
    let fdt_bytes = unsafe { slice::from_raw_parts(dtb_pa as *const u8, fdt_total) };
    let fdt = match fdt::Fdt::new(fdt_bytes) {
        Ok(f) => f,
        Err(_) => {
            say("### minix-rs kernel image: DTB parse failed — halting\n");
            crate::halt();
        }
    };

    let n = fill_memmap(&fdt);
    if n == 0 {
        say("### minix-rs kernel image: no usable /memory reg in DTB — halting\n");
        crate::halt();
    }
    // SAFETY: `fill_memmap` 刚写了前 `n` 槽（n≥1）；boot 面单 hart，
    // 此后本模块对该格只读。
    let memmap: &'static [MemoryRegion] = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(MEMMAP_STORAGE).cast::<MemoryRegion>(), n)
    };
    say("  memmap regions 0x");
    say_hex(n as u64);
    say(" first 0x");
    say_hex(memmap[0].base.0);
    say("+0x");
    say_hex(memmap[0].len as u64);
    say("\n");

    // 2. BootFileTable：a2 约定通道与 DTB /chosen 通道，都要先过
    //    DRAM 界闸再过 magic 闸才解引用（hartid/垃圾指针直读 = Load
    //    fault；M4.3 与 U-Boot 取证的一贯纪律）。
    let chosen_pa = fdt_table_pa(&fdt);
    let table_pa = [Some(table_pa_a2), chosen_pa].into_iter().flatten().find(|pa| {
        in_memmap(memmap, *pa) && table_magic_ok(*pa)
    });
    let Some(table_pa) = table_pa else {
        say("### minix-rs kernel image: no BootFileTable (a2/chosen) — module channel not wired, halting\n");
        crate::halt();
    };
    say("  boot file table pa 0x");
    say_hex(table_pa);
    say("\n");
    // SAFETY: 上一步的 `table_magic_ok` 已在 `in_memmap` 界内核对
    // magic；表由 U-Boot/宿主注入工具放在启动期独占的物理 RAM，本面是
    // 它的首个也是唯一读者（单 hart，无并发写者）。
    let table = unsafe { &*(table_pa as *const BootFileTable) };

    // 3. bump 物理池（boot-shim 库 `MODULE_REGION_BASE` 车道：DRAM+32M
    //    起 32MiB，`BUMP_END <= boot_dm_admissible_end()` 编译期已证）：
    //    root 页 → 页表 bump 区 → 名字池，顺序无重叠；模块取页在其后。
    //    CodeReview SF5：全部走 Option+诚实停机，不留 `.expect`/panic 腿
    //    （panic 渲染携 baked-VMA fmt pieces，物址视图里正好踩定枢）。
    let pool_exhaust = "### minix-rs kernel image: bump pool exhausted — halting\n";
    let Some(cur_root) = bump_alloc_pages(1) else { say(pool_exhaust); crate::halt() };
    let root_page = PhysBytes(cur_root);
    let Some(pt_base) = bump_alloc_pages(PT_BUMP_PAGES) else { say(pool_exhaust); crate::halt() };
    let pt_end = pt_base + (PT_BUMP_PAGES as u64) * 4096;
    // 名字池：单页物址 bump 区，12×16B 槽（详 `place_modules` 视图论）。
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

    // 4. 12 模块：表定位（名对位校验）→ 源直拷进 bump 页（源已过与池
    //    不相交闸，`copy_nonoverlapping` 无重叠）。
    if place_modules(table, memmap, names_pool).is_none() {
        say("### minix-rs kernel image: module placement failed — halting\n");
        crate::halt();
    }
    // SAFETY: `place_modules` 刚按契约序写满 12 槽；此后只读。
    let modules: &'static [BootModule] = unsafe { &*core::ptr::addr_of!(MODULE_TABLE) };
    say("  boot modules placed 12\n");

    // 5. 平台描述符源：riscv 只有 DTB（boot-shim 库同款车道；槽位住
    //    .bss，避开堆 leak 面）。
    // SAFETY: boot 面单 hart，写一次后只读（下一步起）。
    unsafe {
        *core::ptr::addr_of_mut!(PLATFORM_SOURCE) =
            PlatformDescSource::new(DTB, PhysBytes(dtb_pa));
    }
    // SAFETY: 同上——写完之后的静态引用，无并发改写；数组单格与切片
    // 长度 1 同源。
    let platform_sources: &'static [PlatformDescSource] = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(PLATFORM_SOURCE), 1)
    };

    // 6. KernelInfo（boot-shim `build_kernel_info` 车道）+ arch_boot 真
    //    门槛（validate → 建根 → DM → jump_to_kmain，不再返回本面）。
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

/// 把 `/memory*` 节点的 `reg` 写进 `.bss` 存储格，返回条数（0 = 无可用
/// 条目）。不用 `Fdt::memory()`——它在缺 `/memory` 时 `expect` panic，
/// 甲案要的是诚实停机；这里走 `find_node` 的 Option 形态。
fn fill_memmap(fdt: &fdt::Fdt<'_>) -> usize {
    let mut n = 0;
    let mut cursor = Some(fdt.all_nodes());
    while let Some(nodes) = cursor.take() {
        for node in nodes {
            // `/memory` 或 `/memory@80000000`（unit-address 变体）。
            if node.name.split('@').next() != Some("memory") {
                continue;
            }
            let Some(regs) = node.reg() else { continue };
            for region in regs {
                let size = match region.size {
                    Some(s) if s != 0 => s,
                    _ => continue,
                };
                if n >= MEMMAP_MAX {
                    return n;
                }
                // SAFETY: n < MEMMAP_MAX = 存储格长度；boot 单 hart。
                unsafe {
                    MEMMAP_STORAGE[n] =
                        MemoryRegion { base: PhysBytes(region.starting_address as u64), len: size };
                }
                n += 1;
            }
        }
    }
    n
}

/// 读 `/chosen` 的 `opensbi,boot-file-table`（大端 cell；4/8 字节都认）。
fn fdt_table_pa(fdt: &fdt::Fdt<'_>) -> Option<u64> {
    let chosen = fdt.find_node("/chosen")?;
    let prop = chosen.property("opensbi,boot-file-table")?;
    be_cell(prop.value)
}

/// DTB 区块界闸（CodeReview SF2）：totalsize 自描述，但必须验证整块
/// 落在可寻 DRAM 形内——`[DRAM_BASE, 0x1_0000_0000)` 是本平台（QEMU virt
/// 基址 + 4GiB 下界）的保守物理内存上界，更大 blob 或高位址一律拒收，
/// 不给 `Fdt::new` 递越界切窗。memmap 真解析在其后（鸡生蛋：先有
/// 头部才能解树），所以这里用物理形不用车道值。
fn dtb_bounded(dtb_pa: u64, total: usize) -> bool {
    dtb_pa >= DRAM_BASE && dtb_pa + total as u64 <= 0x1_0000_0000
}

/// 大端 cell 取地址值：4 字节 u32、8 字节 u64。
fn be_cell(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        4 => Some(u32::from_be_bytes(bytes.try_into().ok()?) as u64),
        8 => Some(u64::from_be_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}

/// 地址是否落在 memmap 任一区间内（防炸闸：hartid/垃圾值不当指针用）。
fn in_memmap(memmap: &[MemoryRegion], pa: u64) -> bool {
    memmap.iter().any(|r| pa >= r.base.0 && pa < r.base.0 + r.len as u64)
}

/// 地址区间是否整体落在 memmap 任一区间内（防炸闸：hartid/垃圾值不当
/// 指针用；区间形而非单点——CodeReview SF3 要求消费前验全宽）。用
/// `checked_add`：`pa + len` 对任意（含现实不可达的超大）`len` 都不回绕，
/// 溢出即视为越界拒收（CodeReview 续-89 N1：让全宽闸与输入无关地成立）。
fn in_memmap_range(memmap: &[MemoryRegion], pa: u64, len: u64) -> bool {
    let Some(end) = pa.checked_add(len) else {
        return false;
    };
    memmap
        .iter()
        .any(|r| pa >= r.base.0 && end <= r.base.0 + r.len as u64)
}

/// magic 闸：在 `in_memmap` 界内解引用一个 u64 核对表签名。
fn table_magic_ok(pa: u64) -> bool {
    // SAFETY: 调用方已保证 `in_memmap(pa)`——地址在 DTB 报告的 DRAM 内，
    // 可读；只读一个对齐 u64；boot 面单 hart 无并发。
    unsafe { (pa as *const u64).read_volatile() == boot_file_table_magic() }
}

// ── BootFileTable 消费（kernel-image 形状：源直拷入物理 bump 页、零堆）──

/// 12 模块的落地描述表（`.bss`，装配后只读）。
static mut MODULE_TABLE: [BootModule; NR_BOOT_MODULES] = [const {
    BootModule { name: "", start: PhysBytes(0), len: 0 }
}; NR_BOOT_MODULES];

/// 按契约位置消费表前 `NR_BOOT_MODULES` 项、源直拷进 bump 页；条目不足/
/// 名不对位/源越界/源与池相交/池枯竭都点名并返回 `None`（对位 loader.rs
/// `load_boot_modules_with_loader` 的 fail-fast 语义——装机错误在这条腿
/// 上暴露，不给内核递半套模块）。
///
/// 为什么源直拷而不经暂存中转（续-89 真机定案）：早期设计留了一格 1 MiB
/// 物理 scratch 做两段式拷贝，但 mfs 嵌 8 MiB imgrd 后单模块达 ~8.9 MiB，
/// 超死 scratch 预算——而中转本仅为「拷前核长度」，`in_memmap_range` 已把
/// 源全宽验在 memmap 内、不相交闸已排除与 bump 池重叠，`copy_nonoverlapping`
/// 直拷已充分（无重叠 UB）；去掉中转既省内存又消除尺寸上限。
///
/// 位置为主、名为辅（CodeReview SF4）：表格式本身不承诺顺序（槽宽 16
/// 可含 kernel 槽），纯位置消费会把错位的字节流安静地当成某个模块。
/// 校验用「表内 path 的最后一段（`/` 分界，对位 loader.rs 的 ESP 语义）
/// 逐字节对期望名字面量」——表 RAM 字节与调用点字面量都是视图安全形，
/// 不引入任何 baked 指针解引用。
///
/// 源区间三道闸（CodeReview SF3）：len 非零；`phys_addr..+len` 整体落在
/// memmap 内（越界读 = fault→热重入）；不与 bump 池相交（直拷重叠即 UB）。
///
/// `names_pool` 是物理 bump 页基址：`BootModule.name` 的指针字段必须也
/// 是物理址——内核 `store_kernel_info` 在 `arch_boot` 入口（satp 写入后、
/// enable 前的物址腿）就 `module.name.as_bytes()` 读一次（kernel/src/
/// lib.rs:734），baked VMA 递进去同样当场炸。字节池槽（纯数据，调用点
/// `auipc` base）拷进物理名字池后，胖指针现场拼装——两份视图都可达。
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
        // 名对位闸：表内 path 基名 == 字节池第 i 槽前缀。
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
        if entry.phys_addr < bump_end_bound() && entry.phys_addr + entry.len >= bump_pool_base() {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" source overlaps bump pool\n");
            return None;
        }
        let pages = len.div_ceil(4096);
        let dest = bump_alloc_pages(pages)?;
        // SAFETY: 源区间刚过 memmap 全宽闸且与 bump 池（dest 所在）不相交；
        // dest = 独占 bump 页（游标单调互不重叠），拷贝长度恒 ≤ 两端区间长。
        unsafe {
            core::ptr::copy_nonoverlapping(entry.phys_addr as *const u8, dest as *mut u8, len);
            core::ptr::write_bytes((dest as *mut u8).add(len), 0, pages * 4096 - len);
        }
        // 名字入池：字节池第 i 槽（16B，含零填充）拷进物理名字池，胖
        // 指针指向池槽——纯数据读，不踩 baked 指针。
        let slot = names_pool + (i as u64) * NAME_SLOT as u64;
        // SAFETY: 源 = `NAME_POOL` 字节格（base 经调用点 `auipc` 物化，
        // 非 baked 指针；读 16B 不越池尾）；目的 = 独占名字池内第 i 槽
        // （i < 12 ⇒ 偏移 < 192 ≤ 一页）。长度取池中第一条非零边界前
        // 的字节数：零填充不入 `&str`（内核 strlcpy 语义只需前缀）。
        unsafe {
            core::ptr::copy_nonoverlapping(
                NAME_POOL[i].as_ptr(),
                slot as *mut u8,
                NAME_SLOT,
            );
        }
        let n = NAME_POOL[i].iter().position(|&b| b == 0).unwrap_or(NAME_SLOT);
        let landed: &'static str = unsafe {
            // SAFETY: 字节来自 ASCII 字面量池（零截断前全 < 0x80）；
            // backing 是独占物理页，与 `MODULE_TABLE` 同寿命。
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

// ── 串口面（无堆：定长字符串 + 手写 hex）──────────────────────

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
    // hex 字面量恒 ASCII。
    early_console::write_str(core::str::from_utf8(&buf).unwrap_or(""));
}

// ── .bss 装配格（boot 面独占写、随后只读）──────────────────────

/// memmap 条目存储格（`fill_memmap` 写前 `n` 槽）。
static mut MEMMAP_STORAGE: [MemoryRegion; MEMMAP_MAX] =
    [MemoryRegion { base: PhysBytes(0), len: 0 }; MEMMAP_MAX];

/// 平台描述符源格（DTB 单源；先于 `arch_boot` 写一次）。
static mut PLATFORM_SOURCE: PlatformDescSource = PlatformDescSource::new(DTB, PhysBytes(0));
