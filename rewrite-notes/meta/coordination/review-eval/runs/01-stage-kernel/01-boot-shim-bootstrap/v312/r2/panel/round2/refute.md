DOC 01-boot-shim-bootstrap.md

# R2 反证报告：D01 · 01-boot-shim-bootstrap

核对 6 条主张：4 条反证失败（主张成立），1 条部分成立（门链成立、I4 全称措辞有 P2 反例），1 条跨节自洽主张有 2 处 P2 反例。未找到 P0/P1 阻断级反证。

## CLAIM 01
- 主张：三条 v1 腿的 wire 只写固定宽度标量与字节分区，接收侧在消费前把六个分区深拷进内核 `.bss`；wire 不携带发送侧虚拟地址或进程局部句柄（I1）。
- 反证尝试：
  1. 找接收侧在解码前解引用发送侧指针的路径：`arch_boot_resume_v1` 只从 arena 解 envelope 与六个分区，写入 `globals` 的定长 store；`KernelInfo` 的切片由 store 指针构造（`os/kernel/src/lib.rs:668-746`），`arena_pa` 不保留。未找到发送侧 VA。
  2. 找 wire 中的指针字段：`BootHandoffV1` 全是定宽标量；六段载荷由 `byte_len` 与固定偏移描述（`os/libs/minix-boot/src/handoff_v1.rs:356-392`）；`SourceRecord` 是 72 B 的 range/kind/origin，无线内指针。
  3. 找"校验前构造引用"的路径：`check_header` 先校验 `magic`/`version`/`size`/`arena_len`（`os/libs/minix-boot/src/handoff_v1_read.rs:100-117`），`element_count` 在构造引用前拒绝越界与不可整除（同文件 `:195-208`）。
  4. 找跨镜像借指针存活的路径：`land_slice` 的指针相等分支只跳过 v1 已落盘的同址重存，两个直核腿仍真拷贝（`os/kernel/src/lib.rs:1535-1548`、`store_boot_report` 在 `:1568-1573`）。
  5. 找 arena 生命周期反例：`arch_boot_resume_v1` 的文档注释把解码定为 arena 的最后一次读（`os/kernel/src/lib.rs:642-654`），VM 自由页在 `kmain` 之后才发布。
- 结果：反证失败（主张成立）。
- 证据：上列行号。`platform_sources` 携带的是物理地址（值），文档 §1.2 第 85 行已明确把物理地址排除在 I1 禁令之外；`SourceRecord` 的 `input_object`/`origin` 是值，不是句柄。
- 若我错了：若某腿把 arena 内的切片直接提升为 `'static` 并在跳转后读取，或把 `BootReport` 的借用切片写入全局而不经 `land_and_store`，即构成反例。仓内未找到这种路径；v1 接收侧的 `KernelInfo` 栈上临时值也只是 store 指针的搬运，最终唯一全局值由 `land_and_store` 写入。

## CLAIM 02
- 主张：§4.4.1 的逐腿门链成立——直核面 `strict → pre-table → postconditions → store_boot_report → boot_validate → kmain validate`；v1 接收面 `strict → postconditions → resume_boot_pt_alloc → boot_validate → … → store_kernel_info`，不经 `validate_before_tables`；v10 之后 `check_boot_postconditions` 只剩 `kern_virt_base > u32::MAX`。
- 反证尝试：
  1. 直核面：`bootface_common.rs:552-554` 在 `arch_boot` 之前显式 `validate_requiring_sources(DTB)`；`arch_boot_finish` 依次 `validate_before_tables`、`check_boot_postconditions`、`store_boot_report`、`arch_boot_impl`（`os/kernel/src/lib.rs:228-232`），`arch_boot_impl` 开头再 `boot_validate_and_prepare`（`:957`）。
  2. v1 接收面：`arch_boot_resume_v1` 在 `kinfo.validate_requiring_sources()` 之后、`boot_alloc::resume_boot_pt_alloc` 之前调用 `kinfo.check_boot_postconditions()`（`os/kernel/src/lib.rs:747-750`）；`resume_additive` 体内是 `boot_validate_and_prepare` → DM plan → `set_current_root_phys` → `store_kernel_info`（`:308-325`），没有 `validate_before_tables`。
  3. 后置条件：`check_boot_postconditions_view` 只有一条断言（`os/libs/minix-boot/src/kernel_info.rs:596-612`），`ValidationLevel` 的 strict 判定见同文件 `:157-179`。
  4. 找 I4 全称措辞的反例：v1 发送侧 `prepare_kernel_image_v1`（`os/kernel/src/lib.rs:437-528`）没有调用 `report.validate_before_tables()`；它用 `report.boot_modules`/`report.platform_sources` 组装窗口（`:479-487`），而 `push_platform_windows` 会对 RSDP 链做实际解引用（`:1230-1241`）。共享校验体里的平台源空址/kind/溢出检查（`os/libs/minix-boot/src/kernel_info.rs:301-317`）在 v1 腿上要等接收侧才第一次跑；发送侧的 `find_platform_sources_into` 自身已先读 RSDP/DTB 长度头（`os/boot-shim/src/uefi_helpers.rs:207-256`）。
- 结果：门链主张反证失败（成立）；I4 的全称措辞非阻断（P2），见反证尝试第 4 条。
- 证据：上列行号；文档 I4 原文在第 83 行，§5.1 把 I4 的顺序保证写成"由 §4.4.1 的调用序与代码走查保证"（第 2613 行），但 §4.4.1 只列了接收侧链。
- 若我错了：若"消费"严格取 §1.2 的三个动词（建表/映射/分配），发送侧的窗口组装与 RSDP 解引用不算消费，则 I4 没有字面反例，只是 §5.1 的"代码走查保证"仍未覆盖发送侧；这仍是措辞收口问题，不是代码缺陷。发送侧是生产者，首个固件头的读取不可能先于校验，属不可消除的先读；文档应把 I4 限定为接收侧消费，或明写"发送侧首读豁免"。

## CLAIM 03
- 主张：两条 UEFI 腿的内核跨度在 A+D 的 A 步被 `AllocateAddress` 预订为 `LOADER_DATA`，因此不在 `memmap`，分类落 `ReservedMapped` 后进 `reserved_regions`；U-Boot shim 与两个直核腿的内核跨度在 `memmap` 内，由扣除表实际扣除；bump 区整段扣除；扣除记录含 VM 自己的 blob，交接模块清单排除 VM。
- 反证尝试：
  1. 找 UEFI 内核跨度进 free list 的路径：`reserve_kernel_span` 用 `AllocateAddress` + `MemoryType::LOADER_DATA`（`os/boot-shim/src/uefi_helpers.rs:69-81`）；root/bump/arena 也全是 `LOADER_DATA`（同文件 `:656-714`），模块页同样是 `LOADER_DATA`（`:1371-1385`）。分类器对 `LOADER_DATA + WRITE_BACK` 返回 `ReservedMapped`（`os/boot-shim/src/ebs_map.rs:637-644`），所以 free list 不含它，内核行扣除在 UEFI 腿是空操作。
  2. 找 UEFI 无 `WRITE_BACK` 的相反分支：同类 `LOADER_DATA` 无 WB 时落 `Side`（`ebs_map.rs:634-636`），strict 的内核跨度覆盖检查会 fail-closed（`os/libs/minix-boot/src/kernel_info.rs:535-553`）；文档 §6.3 已把"固件不设 WRITE_BACK"列为失败模式，不是被省略的"不会发生"。
  3. 找 bump 只扣已用部分的路径：`for_each_live_bootstrap` 对 `boot_bump` 整段 `f(base, end-base)`（`os/kernel/src/vm_handoff.rs:756-758`），不是 `boot_alloc_used_bytes`；与 §3.4 的反向推演一致。
  4. 找模块扣除漏项或误排 VM 的路径：同函数对每个 `modules` 条目 push（`:764-766`），含 VM；交接清单排除 VM 是 `build_vm_handoff` 侧的选择，VM 消费侧再校验 `LiveBootstrap ∩ VM-free = ∅`（`os/servers/vm/src/boot.rs:469-470`）。
- 结果：反证失败（主张成立）。
- 证据：上列行号；扣除表的表头责任说明在 `os/kernel/src/vm_handoff.rs:7-20`；VM 侧"empty LiveBootstrap record" fail-closed 在 `boot.rs:436`。
- 若我错了：若某 UEFI 固件对 shim 自己 `AllocateAddress` 的页报非 WB 属性，则第 1 条会改走 `Side`，内核跨度不进两张表，strict 在接收侧停机。文档 §3.2.1 与 §6.3 已承认并把它设计成 fail-closed，故不构成"失败路径被省略"。另一个潜在反例是某腿的内核跨度既不在 memmap 也不在 reserved（例如 U-Boot shim 的 `fw_end` 裁剪恰好把内核切掉），仓内 `fw_end = KERN_PHYS_BASE` 的取值使内核落在 free 侧，未找到反例。

## CLAIM 04
- 主张：`alloc_lowest` 把长度传给 `cut_memmap` 的结束地址参数，是真实参数错位；在四个已核对前提下真实调用序落"第三行常态"（`len ≤ lowest`，切除为空操作），跳板页仍挂空闲账；四前提不是结构保证，证据只支持"本仓观测布局下不立刻发作"。
- 反证尝试：
  1. 读签名与调用：`cut_memmap(kinfo_t*, phys_bytes start, phys_bytes end)`（`minix3/minix/kernel/arch/i386/pg_utils.c:32`）与 `alloc_lowest` 里的 `cut_memmap(cbi, lowest, len)`（同文件 `:81`）；第三个实参确实是长度。
  2. 攻击前提一（调用序）：`kmain` 在 `cstart`（`minix3/minix/kernel/main.c:147`）之后、`smp_init`（`:311`）之前；`cstart` 内含 `arch_init`，两条 `cut_memmap` 在 `arch_system.c:277-278`。
  3. 攻击前提二（最低基址非零且页对齐）：两条 cut 的 end 向上取整到 `0x1000`/`0x100000`（`pg_utils.c:37-40`），`add_memmap` 把起点向上对齐（`:99`）；BIOS 常量见 `memory.h:29`、`:33`、`:118-119`。
  4. 攻击前提三（`roundup(tramp_size) ≤ lowest`）：跳板尺寸约 2212 B，`roundup` 为 `0x1000`；`GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS) = 6 + CONFIG_MAX_CPUS`（`archconst.h:19-21`），当前为 7×8 B。最低基址至少 `0x1000`，故 `len ≤ lowest` 成立。
  5. 攻击前提四（唯一调用点）：`rg -n "alloc_lowest" minix3/minix/kernel/arch/i386/` 只有定义、原型与 `arch_smp.c:76` 的调用。
  6. 找"切除确实发生"的调用序：只有 `lowest == 0` 或 `lowest < len` 才会切；前提二、三把它排到第三行。若前提改变，第三行常态失效，但文档已把四前提写成可复查条件并拒绝称其为结构保证。
- 结果：反证失败（主张成立）。
- 证据：上列行号；`pg_identity` 的 `assert(cbi->mem_high_phys)`（`pg_utils.c:170`）也支撑 D1 的"反排会当场失败"陈述。
- 若我错了：若未来出现第二个 `alloc_lowest` 调用点，或最低可用条目的基址小于跳板 `roundup` 后的长度，则第三行常态不成立；文档已声明此点，不越权。另有一个不影响内存净账的边角：退化请求恰好页对齐时切不切都为空操作，文档已写明。

## CLAIM 05
- 主张：ARM32 的 `overlaps` 是闭区间判定（上界取 `thismod->mod_end`，第二项没有 `-1`），模块—模块因 8 MiB 槽距且 `mod_end` 少一页而不会相切；内核项无减页，只有内核长到 30 MiB 才会与 0 号模块相切并误报。
- 反证尝试：
  1. 读宏：`earm/pre_init.c:167-169` 的 `INRANGE` 上界是 `thismod->mod_end`，`OVERLAP` 第二项是 `mod2->mod_end`，确实是无 `-1` 的闭区间。
  2. 找模块相切：`setup_mbi` 把 `mod_end` 设为 `start + MB_MODS_ALIGN - ARM_PAGE_SIZE`（`:199-202`），下一条 `start` 加 `0x800000`，两者间隔 `0x1000`，闭区间不相接。
  3. 找内核相切：内核项 `mod_end = kernbase + kernsize`（`:322-324`），无减页；`MB_MODS_BASE = 0x82000000`（`:182`）减 `_kern_phys_base = 0x80200000`（`earm/kernel.lds:4`）等于 `0x1E00000`，即 30 MiB。
  4. 找尺寸保证：`earm/kernel.lds` 没有内核尺寸断言，因此 30 MiB 是构建事实，不是结构保证；文档正是这样写的。
- 结果：反证失败（主张成立）。
- 证据：上列行号。模块—模块方向还有一条更强的理由：即使 `mod_end` 减页，闭区间只会在 `mod_end == 下一条 start` 时误报，而洞里留了整页。
- 若我错了：若有板级头文件覆盖 `MB_MODS_ALIGN`/`MB_MODS_BASE`，或有其他 ARM 代码在 `setup_mbi` 之后改写 `module_list`，结论需重推。仓内 `rg` 未找到这两类改写。

## CLAIM 06
- 主张：文档的跨节所有权与载荷族陈述自洽——§1.4 把 `k_initial_stktop`/`kernel_boot_stack_top`/`kern_stack_top` 的判据归 02，§3.7 说 `fill_source_records` 只在 U-Boot shim 腿使用。
- 反证尝试：
  1. 栈顶名字：§1.4 第 115 行把三个名字的判据全部判给 02；但 §3.2.1 第 2326 行与 §4.4 第 2334-2338 行在 01 里展开了"两个对象 / 收口填充 / 清零语义"，§7.5 第 2937、2951 行再次复述。§4.4 的三条展开本身与源码一致（`os/kernel-image/x86_64.ld:86-100`、`os/kernel-image/riscv64.ld:92-108`、`os/libs/minix-boot/src/kernel_info.rs:236-243`、`os/kernel/src/arch/riscv64/higher_half.rs:28-40`），所以不是事实错误，而是 §1.4 的边界句被本篇正文直接证伪：读者被指向 02，判据其实在 01 的 §4.4。
  2. 载荷族：§3.7 第 1847 行写"`fill_source_records` 只在 U-Boot shim 腿使用"；`os/kernel-image/src/bootface_common.rs:539` 在两个直核腿都调用它，`os/boot-shim/src/opensbi_helpers.rs:317` 是 U-Boot shim；§4.4 第 2329 行自己写"直核腿与 U-Boot shim 放进 `BootReport`"。全称句为假，正确表述是"UEFI 腿不用；U-Boot shim 与两个直核面都用"。
  3. 相关精度：§2.4.2.5 第 777 行写 `kernel_allocated_bytes` 是 `_kern_size` 减去 `bootstrap_len`；`pre_init.c:160-166` 实际先减未取整的 `bootstrap_len`，随后才把字段 `rounddown` 到页。差值小于 4 KiB，且文档已声明残差多因、不单归因，故只作观察，不单列。
- 结果：非阻断（P2）。两条都是边界/作用域措辞问题，不改变 CLAIM 01–05 的事实结论；但第一条会让读者按 §1.4 去 02 找 `kern_stack_top` 的判据，而 01 已经给了；第二条与同篇 §4.4 的表述互斥。
- 证据：上列文档行号与源码行号。栈顶三条展开的源码核对结果支持 §4.4 的内容，只否定 §1.4 的"归 02"全称句；`fill_source_records` 的两处调用点否定 §3.7 的全称句。
- 若我错了：若"归 02"指"权威推导与证据归 02，01 只在 v10 字段删除的语境下复述区别"，则该句应改成这个可核形式；若 §3.7 的"只在 U-Boot shim 腿"只指 v1 两族（UEFI 对 U-Boot），则应写明"在 v1 两族内"，因为两个直核腿同样拥有 `BootStorage` 与 source 切片。就现文面，两处都是可被本篇正文或仓内源码直接证伪的全称句。

## 未覆盖
- 宿主测试读数（188/0、136/0、866/0/8、27/0）与 QEMU 五门未执行；本轮只做只读源码核对，没有运行 cargo 或 qemu，因此 §5.1/§5.2/§5.4 的门链判据只按文档引用的符号核对，未重放脚本。
- §3.2.4 的真机零化读数（`res=127 zeroed=127`、15 条 conventional 条目）无法复现；文档自己标注为未归档观察，本轮不把它们当事实基线。
- 02 篇的布局判据（`k_initial_stktop`、L0–L10、`arch_boot_resume_v1` 之后的加性阶段）不在本次允许输入内，未交叉核对；只核了 01 内部对这些边界的引用是否与 01 自身内容冲突（CLAIM 06）。
- §2.3 的 `rootdevname` 用户态读者链（PM 的 GET_MONPARAMS 缓存、`mib` 的 `mib_kern_root_device`、`sysenv`）只抽查了 Rust 侧 `KinfoStruct.user_sp` 与 C 侧 `do_getinfo`/`param_buf` 的存在，未逐点重放 PM/mib 读链。
- §2.4.2.5 的"还 VM 镜像"只确认 C 侧唯一的 `add_memmap` 模块回收点（`i386/protect.c:450`），未审计 VM 启动后 `free_mem` 的每条路径。
- 结论：本轮没有 P0/P1 阻断反证；上列 P2 不改变 I1–I4 在接收侧的可观察结论。若面板需要更高置信度，优先补跑宿主测试与两条 UEFI/U-Boot shim 门链，并核 02 篇的栈顶判据是否与 01 §4.4 重复。
