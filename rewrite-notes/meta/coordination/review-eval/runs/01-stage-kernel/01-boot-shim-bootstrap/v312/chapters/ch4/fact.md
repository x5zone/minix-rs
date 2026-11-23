DOC: 01-boot-shim-bootstrap.md
CHAPTER: 4. 实现详解
VERDICT: PASS

核对基线：`tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch4-slice-candidate.md`
sha256 `6581f240d6886cef2fc9af4102da31f544c9a5ed21a71b1932fafaa2dc38cfda`，440 行；
设计合同 `rewrite-notes/meta/design/01-stage-kernel/01-design.v4.md`（43 行）。
下文切片行号均按该 sha；代码行号按本次读取时的工作区状态。只读了切片、设计合同与锚点指向的源码。

前置说明：本报告上一版（sha `7cbb027…`）的 2 个错误与 1 个 NOTE 已核销——§4.3 L231 的 `tty`/`mib`
反事实已改成 `place_modules` 运行期核对（与 `bootface_common.rs:227-243` 一致）；§4.10 L420-421
的预期输出已改成内核侧 16 行、镜像侧 6 行（与实跑一致）；§4.2 L80 已改成“入口不共享函数、取件共用
`FileLoader`”（与两个 impl 一致）。以下按新 sha 重新编号核对结果。

CF-01 入口与调用链（§4.1 L28-L32、L36-L47）
  断言：UEFI 入口是 `main`，只转交 `prepare_and_enter_kernel_image(BOOT_PT_BUMP_PAGES)`；该函数返回 `!`；直核家族收敛在 `arch_boot_finish`，跨镜像三腿经 `arch_boot_resume_v1` 汇入 `resume_additive`；`kmain` 复核 strict 与 postconditions。
  证据：`os/boot-shim/src/main.rs:33-71`（`#[entry] fn main() -> Status`，L70 调用）；`os/boot-shim/src/uefi_helpers.rs:1015`（`-> !`）；`os/kernel/src/lib.rs:215-248`（`arch_boot_finish`）、`310-360`（`resume_additive`）、`673-761`（`arch_boot_resume_v1`）、`1859-1861`（`kmain` strict + postconditions）。
  结论：OK。最小修法：无。

CF-02 页表页预算（§4.1 L72-L76）
  断言：`BOOT_PT_BUMP_PAGES = 1024`（4 MiB），两个直核面用 `PT_BUMP_PAGES = 256`；实际用量记 `kernel_allocated_dynamic`；未用部分不退回，按 `boot_alloc_region()` 整段扣除，UEFI 腿对 free list 是空操作。
  证据：`os/boot-shim/src/lib.rs:168`；`os/kernel-image/src/bootface_common.rs:33`；`os/kernel/src/vm_handoff.rs:668`（`kernel_allocated_dynamic: boot_alloc_used_bytes()`）；`os/kernel/src/vm_handoff.rs:497,504-513`（`boot_alloc_region()` 进同一扣除记录后逐段 cut）。
  结论：OK。最小修法：无。

CF-03 九动作与十时间拍（§4.2 L82-L89、L97-L108）
  断言：时间拍 1-10 为 读/预订 → root/bump → 拷贝/模块 → 平台源/参数 → arena → 信封 → EBS 分类 → 提交计数 → 报告复核 → 建根跳转。
  证据：`os/boot-shim/src/uefi_helpers.rs:1031-1057`（读 ELF、布局、assert entry、reserve/record）、`1058-1062`（root、bump、`init_boot_pt_alloc`）、`1065-1098`（段拷贝、模块、复查）、`1099-1114`（平台源、param）、`1116-1129`（arena、Serializer）、`1134-1146`（信封与四类非图载荷）、`1167-1228`（配对循环）、`1239-1249`（报告与三项 bootstrap 复核）、`1278-1283` + `os/kernel/src/lib.rs:447-557`（建根、补字段、跳转）。
  结论：OK。表内动作 ID 与真实时序的对应也正确（动作 2 拆 1/3 拍）。最小修法：无。

CF-04 DTB preflight 与 authorize_and_copy 的时序（§4.2 L91）
  断言：“配对循环体内另有一处 DTB 头 preflight，发生在首次 `GetMemoryMap` 之后、`ExitBootServices` 之前；`authorize_and_copy` 也在最后一次 `GetMemoryMap` 之后、`ExitBootServices` 之前执行。”
  证据：循环体顺序是 `preflight_dtb`（`os/boot-shim/src/uefi_helpers.rs:1176`）在 `pair_and_classify`（L1193-1204）之前；`preflight_dtb` 自己先取一次 `GetMemoryMap` 快照再读 DTB 头并分配副本（`os/boot-shim/src/ebs_pair.rs:266-304`）；`authorize_and_copy` 在最终快照分类之后、EBS 之前（`ebs_pair.rs:195-207`）。若“首次 GetMemoryMap”指 `probe_map_buffer_size` 的空缓冲探测（`uefi_helpers.rs:1159`），首句字面成立；若按配对循环内的首次读取理解，先后顺序相反——而“副本分配改变内存图、所以配对循环的那次读取才是最终快照”正是该 preflight 的设计理由（`ebs_pair.rs:9-13`）。
  结论：NOTE（时序表述有两种读法，自然读法为反序）。最小修法：改为“preflight 自己先取一次快照，再读 DTB 头并分配副本；该副本改变内存图，配对循环随后的 `GetMemoryMap` 才是最终快照”。

CF-05 配对循环与提交（§4.2 L105-L106、L120、L126）
  断言：循环内 GetMemoryMap → 分类 → 写 source → 同一 MapKey 调 raw EBS；两表与计数同一步提交；重试前清旧 wire 条目；`BUFFER_TOO_SMALL` 仅首次 EBS 前扩容，`EFI_INVALID_PARAMETER` 重分类一次，其余立即失败。
  证据：`os/boot-shim/src/ebs_pair.rs:142-233`（L150-159 取图、L172 `reset_map_partitions`、L175-184 parse/classify、L195-203 DTB copy/claims、L207 EBS 用 `map_key`、L225-230 重试）；`EBS_ATTEMPT_LIMIT = 2`（L59）；`BufferReuseInsufficient`（L160-165）；`os/libs/minix-boot/src/boot_storage.rs:147-166`（`fill_map_tables` 一次提交两表与计数）。
  结论：OK。最小修法：无。

CF-06 动作 2 的跨度预订与两道防线（§4.2 L128-L132）
  断言：`reserve_kernel_span` 用 `AllocateAddress` 把跨度订为 `LOADER_DATA`（失败即 halt），`record_kernel_span` 在拷贝前记跨度，模块分配与 root/bump/模块复查是第二道防线。
  证据：`os/boot-shim/src/uefi_helpers.rs:69-92`（`AllocateType::Address` + `LOADER_DATA`，失败 panic）、`44-50,1056-1067`（record 在拷贝前）、`1072-1085`（root/bump 复查）、`1371-1396`（`alloc_module_pages` 拒绝内核重叠页）、`107-115,1094-1098`（`assert_modules_clear_of_kernel`）。
  结论：OK。最小修法：无。

CF-07 分类五步与两道内联闸（§4.2 L138-L150）
  断言：判定步 1 类型/属性/ISA；步 2 `RefuseToBoot`；`checked_shape`；步 3 `Excluded`/`DevicePath`；`checked_len`；步 4 `TYPE_INDEPENDENT`；步 5 `WRITE_BACK` 与类型表；整图级拒绝含 `OverlapsDescriptor` 与四类容量。
  证据：`os/boot-shim/src/ebs_map.rs:587-646`（`adjudicate`）；`544-567`（`checked_shape`）、`571-578`（`checked_len`）；`133-155`（类型，`LAST_KNOWN = UNACCEPTED = 15`）、`164-235`（`AMBIGUOUS`/`PROFILE_ONLY`/`TYPE_INDEPENDENT`）；`690-745`（`classify` 的重量与重叠检查、L724-743 四类容量）。
  结论：OK；仅 `RUNTIME` 能到步 4 的判断也成立。最小修法：无。

CF-08 三个手算例与诊断串（§4.2 L156-L160）
  断言：例 1 `Excluded` 先于属性；例 2 `Side`（属性独立先于类型表）；例 3 `OutOfPhysRange` 整图作废；`build_memmaps` 输出 `boot-shim: memmaps conv=… reserved=… unmapped=…`，空表打印 `(empty map)`。
  证据：`os/boot-shim/src/ebs_map.rs:619-624`、`629-631`、`571-577`；`os/boot-shim/src/uefi_helpers.rs:431-433`。
  结论：OK。最小修法：无。

CF-09 `pa_limit` 与 `BOOT_IDENTITY_MAP_END`（§4.2 L162-L171）
  断言：界是 `min(BOOT_IDENTITY_MAP_END, VM DM 窗口物理上界)`，`BOOT_IDENTITY_MAP_END = 0x1_0000_0000`；`MaxAddress` 传 `bound - 1`；引用代码与 `bootstrap_alloc_limit` 一致。
  证据：`os/arch/src/lib.rs:222`（`BOOT_IDENTITY_MAP_END = 0x1_0000_0000`）、`236-243`（`boot_dm_admissible_end`）；`os/boot-shim/src/uefi_helpers.rs:632-650`（语义注释与函数体，逐行匹配切片引文）；`590-596`（48/52 位页表宽度）。
  结论：OK。最小修法：无。

CF-10 名字表与位置契约（§4.3 L186-L198）
  断言：`BOOT_MODULE_IDENTITIES` 12 项，顺序 `ds, rs, pm, sched, vfs, memory, tty, mib, vm, pfs, mfs, init`；`ds` 在位置 0、进程号 6；`MODULE_NAMES`/`BOOT_MODULE_PROC_NRS` 是派生视图。
  证据：`os/libs/minix-boot/src/kernel_info.rs:686`（`NR_BOOT_MODULES = 12`）、`704-751`（身份表与两个派生视图）。
  结论：OK。最小修法：无。

CF-11 装载循环四条事实（§4.3 L204-L216）
  断言：页数向上取整；尾部余量清零；`BootModule.len` 是修剪后的文件尾；缺件 panic 且消息带名字/路径/总数。
  证据：`os/boot-shim/src/loader.rs:601-653`（L625 `elf_file_end`、L628 `(len+4095)/4096`、L641-642 清零、L650 `len: data.len()`、L603-611 panic 文案含 name/path/len）。
  结论：OK。最小修法：无。

CF-12 U-Boot 取件与表构建（§4.3 L218-L229）
  断言：`entry_path_eq` 要求路径 NUL 结尾；表在 `.bss` 内构建、两条 assert 拦路径超界；`valid_entries` 夹到容量；坏路径导致 `find` 返回 None、`read_required` 报找不到内核。
  证据：`os/boot-shim/src/opensbi_helpers.rs:176-182`（等式）、`165-168`（夹取）；`os/boot-shim/src/uboot_main.rs:287-292`（`.bss` 表）、`373-428`（路径写入与两条 assert）；`os/boot-shim/src/loader.rs:152-157`（`read_required` panic）。
  结论：OK。最小修法：无。

CF-13 `tty`/`mib` 互换反事实（§4.3 L231；对应上一版错误，已修复）
  断言（新文）：“`place_modules` 在装载时按 `NAME_POOL` 逐槽核对 `BootFileTable` 条目的 basename，互换会当场打印 “path basename mismatches contract order” 并停机；编译期常量只核对池自身（截断/NUL），拦不住顺序互换。反事实只对三条 v1 腿成立。”
  证据：`os/kernel-image/src/bootface_common.rs:227-243`（逐槽取 `NAME_POOL[i]`、取表项 basename、不匹配打印 L241 的原文并 `return None`）；`os/kernel-image/src/bootface_common.rs:486-490`（`run_self_boot` 收到 None 后 halting）；编译期断言只查 `want.len() < NAME_SLOT` 与 NUL（`70-84`），`NAME_POOL` 由同一列表生成（`54-64`），确实拦不住顺序互换；v1 腿的静默错绑描述与 `load_boot_modules_into` 按 `MODULE_NAMES` 顺序装载、内核按 `BOOT_MODULE_PROC_NRS` 位置绑定一致（`os/boot-shim/src/loader.rs:601-611`、`os/kernel/src/misc.rs:551,577`）。
  结论：OK（上一版错误已核销）。最小修法：无。

CF-14 发送视图收敛与 `KernelInfoView`（§4.4 L235-L248）
  断言：五腿发送侧只构造 `BootReport<'_>`；接收侧统一 `land_and_store`（直核 `store_boot_report`，v1 `store_kernel_info`）；`KernelInfoView` 九个访问器且 `KernelInfo` 与 `BootReport` 都实现。
  证据：`os/kernel/src/lib.rs:1546-1559`（`land_slice`）、`1573-1584`（两个 store 入口）、`1586-1717`（`land_and_store`）；`os/libs/minix-boot/src/kernel_info.rs:188-221`（trait 与两个 impl）。
  结论：OK。最小修法：无。

CF-15 字段来源表（§4.4 L252-L260）
  断言：`memmap`（UEFI conventional / U-Boot `argv[0]` / riscv64 `a1` / aarch64 `DTB_PA`）；`reserved_regions`（ReservedMapped / DTB 以 `KERN_PHYS_BASE` 裁剪 / aarch64 `fw_end=0`）；跨度三字段由 PT_LOAD 求 min/max 且断言 `e_entry == min(vaddr)`；`source_records` 按腿；`authority` 直核 DTB、UEFI wire 解码为 UEFI；`platform_sources`；`param_buf`；v10 派生字段。
  证据：`os/kernel-image/src/bootface.rs:135-143`（a1、`fw_end: KERN_PHYS_BASE`）；`os/kernel-image/src/bootface_a64.rs:84,144-148`（`DTB_PA`、`fw_end: 0`）；`os/boot-shim/src/uboot_main.rs:439-448`（argv[0] DTB）；`os/boot-shim/src/loader.rs:249-296`（跨度计算）；`os/boot-shim/src/uefi_helpers.rs:1043-1046`（entry 断言）、`305-313`（LoadOptions）、`207-266`（Configuration Table）；`os/libs/minix-boot/src/handoff_v1_read.rs:160-164`（tag 0/1 → DTB/UEFI）、`os/libs/minix-boot/src/memmap_source.rs:574-596`；`os/libs/minix-types/src/types/boot.rs:151`（`USER_STACK_TOP`）；`os/arch/src/lib.rs:380-392`（`syscall_entry_va`）。
  结论：OK。最小修法：无。

CF-16 `kern_stack_top` 三条语义（§4.4 L262）
  断言：镜像尾是 `.bss` 末尾 `ALIGN(0x200000)` 收口；`kernel_boot_stack_top` 是 64 KiB 栈顶，位于收口前，两者相差 0 到 2 MiB−16；`jump_to_kmain` 把 sp 置到镜像尾；直核 `_start` 只清到 `__bss_end`。
  证据：`os/libs/minix-boot/src/kernel_info.rs:253-260`（`kern_virt_base + kern_size`）；`os/kernel-image/x86_64.ld:86-99`（`kernel_boot_stack_bottom/top`、0x10000、`ALIGN(0x200000)`、`__image_end`）；`os/kernel/src/arch/aarch64/higher_half.rs:29`（及 x86_64 同名实现）取 `kinfo.kern_stack_top()` 作 sp；`os/kernel-image/src/main.rs:149-160`（清到 `__bss_end`）。
  结论：OK；0 到 2 MiB−16 的差值范围与 16 字节对齐推导一致。最小修法：无。

CF-17 内核侧门与窗口（§4.4 L267-L283）
  断言：`ks>0`、两基址页对齐、栈顶 16 对齐、bump 已注册、页大小能力；`pt_alloc` 按 `is_registered()` 守卫注册；物理上限由 `BootWindows::check` 逐窗比较，建根只收 `CheckedWindows`；`push`/`PhysRange::new` 的负控有宿主测试。
  证据：`os/kernel/src/lib.rs:902-946`（断言、`require_boot_region`、`is_registered` 守卫、`detect_boot_page_support`）、`1307-1323`（`CheckedWindows` 与 `check`）、`1325-1329`（建根签名）；`os/kernel/src/lib.rs:5924-5935`（empty/reversed 负控）、`os/kernel/src/vm_handoff.rs:1339`（`PhysRange::new` 反向返回 None）。
  结论：OK；引文 `lib.rs:902-931` 与切片一致。最小修法：无。

CF-18 逐腿门链（§4.4.1 L287-L298）
  断言：直核面 strict → 建表前门 → postconditions → `store_boot_report` → `boot_validate_and_prepare` → kmain 的 strict 与 postconditions 复核；v1 面 strict → postconditions → `resume_boot_pt_alloc` → `boot_validate_and_prepare` → … → `store_kernel_info` → kmain 复核，且不经建表前门；v1 发送侧只跑 `boot_validate_and_prepare`。
  证据：`os/kernel-image/src/bootface_common.rs:554-556`（strict 后 `arch_boot`）；`os/kernel/src/lib.rs:228-247`（pre-table、postconditions、store、arch_boot_impl、jump）、`967-971`（arch_boot_impl 开头 boot_validate）、`757-760`（v1 的 strict、postconditions、resume_boot_pt_alloc、resume_after_v1_decode）、`318-359`（resume_additive 内 boot_validate → store_kernel_info → establish → jump）、`1859-1861`（kmain strict + postconditions）；`447-470`（v1 发送侧只跑 boot_validate）。
  结论：OK；kmain 的“strict 与 postconditions 复核”与代码一致。最小修法：无。

CF-19 门表两处范围/措辞（§4.4.1 L295、L309）
  断言（L295）：v1 接收面第 3 拍是“解码后、建表前的 postconditions”。（L309）“共享校验体（三档都跑）”包含“按 `MODULE_NAMES` 位置核对模块名”。
  证据：v1 接收面不重建根，`resume_additive` 只在活动根上追加 DM 覆盖（`os/kernel/src/lib.rs:337-350`）；“建表前”只对直核面成立。模块名位置检查在 `if level.is_strict()` 内（`os/libs/minix-boot/src/kernel_info.rs:510-517`），`BeforeTables`/`Standard` 不走这段；共享体其余项（两表升序/不相交、内核↔模块、模块↔模块）确为三档共用（L346-367、L400-435）。
  结论：NOTE。最小修法：（a）v1 行改为“解码后、DM 建立前”；（b）把“按 `MODULE_NAMES` 位置核对模块名”移到 strict 行或标注“仅 strict”。

CF-20 深拷路径与引文（§4.5 L314-L321）
  断言：`arch_boot_finish` 经 `store_boot_report` → `land_and_store` 深拷；v1 `resume_additive` 仍走 `store_kernel_info`；引文三行取自 `land_and_store` 的 reserved 段。
  证据：`os/kernel/src/lib.rs:240`（`store_boot_report` 调用）、`1579-1584`、`335`（`store_kernel_info`）、`1613-1615`（引文原行）。
  结论：OK。最小修法：无。

CF-21 六个落地区与容量（§4.5 L327-L336）
  断言：256 / 128（C MAXMEMMAP=40 提额）/ 12 / 4 / 1024 / 296×72；名字池每槽 16；`KernelInfo.memmap` 是切片视图；`VmBootHandoff` 64 / 47。
  证据：`os/kernel/src/globals.rs:297,327,343,355,366,380,400`；`os/libs/minix-boot/src/handoff_v1.rs:356-369`（128/256/4/1024/296/72）；`os/libs/minix-types/src/types/boot.rs:69,84`（64/47）；`os/kernel/src/vm_handoff.rs:235-236`（47 ≥ 3+32+12 编译期断言）、`269`、`527-530`（容量断言 panic）。
  结论：OK。最小修法：无。

CF-22 名字池截断与 UTF-8（§4.5 L338-L340）
  断言：`store_kernel_info` 取 `min(len, 15)`；v1 接收路径写到 16、不 `min(15)`；截断后用 `from_utf8`，15 字节切断多字节序列时取 `valid_up_to()` 最长合法前缀。
  证据：`os/kernel/src/lib.rs:1638`（`min(BOOT_MODULE_NAME_LEN - 1)`）、`704-710`（v1 的 `position(0)`/`name_len` 可到 16）、`1658-1661`（`from_utf8` + `valid_up_to`）；但 v1 路径用的是 `core::str::from_utf8(bytes).expect(...)`（`lib.rs:717-718`），非法 UTF-8 直接 panic，不取最长前缀。
  结论：NOTE（`valid_up_to` 只描述 `store_kernel_info` 路径，v1 路径行为不同且正文未交代）。最小修法：补一句“v1 接收路径对非法 UTF-8 直接 panic（`expect`）”。

CF-23 落地区守卫（§4.5 L340）
  断言：同址跳过是真实现；两个生产 store 调用点；`kmain` 不再 store；一次性守卫 check-then-write、非 CAS；`ptr::copy` 为部分重叠保留。
  证据：`os/kernel/src/lib.rs:1551`（指针相等跳过）、`1554-1557`（`ptr::copy`）、`1594-1597`（`KERNEL_INFO.is_none()` 断言）、`1840-1877`（kmain 读全局槽、不 re-store）。
  结论：OK。最小修法：无。

CF-24 `BootAlloc` 两种进入方式（§4.6 L352-L366）
  断言：`init` 置回 base；直核 `run_self_boot` 只初始化 bump 区间、注册在 `boot_validate_and_prepare`；`resume` 续游标，v1 在解码信封后、委托壳前调用；三架构 cfg 壳经 `resume_after_v1_decode` 分派。
  证据：`os/kernel/src/boot_alloc.rs:60-68`（`init`）、`84-92`（`resume`）、`183-217`（全局入口）；`os/kernel-image/src/bootface_common.rs:460`（`init` 于 `run_self_boot`）；`os/kernel/src/lib.rs:933-935`（注册）、`757-760`（v1 次序）、`766-803`（三个 cfg 壳）。
  结论：OK。最小修法：无。

CF-25 两个 bump 实例（§4.6 L370）
  断言：U-Boot 固定池按“根页 → 1024 页页表子区间 → 模块页 → arena”发放；内核只续用页表子区间；两实例不共享游标；共享 `BumpRegion`；四条性质的编译期断言。
  证据：`os/boot-shim/src/opensbi_helpers.rs:336-357`（root 后紧接 PT 子区间及运行期断言）、`420`（arena 在模块之后）、`553-570`（顺序注释与四条性质的编译期断言：基址/大小 4 KiB 对齐、`KERNEL_MAX_LMA_END <= MODULE_REGION_BASE` 在 L94-96、容量和、`BUMP_END <= boot_dm_admissible_end` 在 L550-551；另有 `BUMP_PAGE_SIZE` 同除数的一条冗余断言）、`540-541`（`BUMP_NEXT`/`BUMP_END`）；`os/boot-shim/src/uboot_main.rs:470-476`（只把 `result.bump` 交给 `init_boot_pt_alloc`）；`os/kernel/src/boot_alloc.rs:166`（独立的 `BOOT_ALLOC`）；`os/libs/minix-boot/src/bump.rs:51-92`（`resumed`/`used_bytes`/`is_consistent`/`alloc_pages`）。
  结论：OK；“合并会让 `resume` 看到 `next > end`”的机制与 `BumpRegion` 的一致性谓词一致。最小修法：无。

CF-26 arena 生命周期（§4.6 L372）
  断言：解码是末次读取，六载荷进 store，`arena_pa` 不保留；VM 自由清单在 `kmain → build_vm_handoff` 才发布；测试 `decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 复现 poison/重发/内核扣除；“VM 在消费侧校验 `LiveBootstrap ∩ VM-free = ∅`”。
  证据：`os/kernel/src/lib.rs:652-761`（生命周期文档与六个 decode）、`3144`（`build_vm_handoff` 由 `init_proc_and_boot` 调用，处于 `kmain` Phase C）；`os/kernel/src/vm_handoff.rs:575-668`、`1088-1195`（测试内容与描述一致）。
  结论：NOTE（前半 OK；末句“VM 消费侧校验 `LiveBootstrap ∩ VM-free = ∅`”在本次定向源码内无锚点，无法证实）。最小修法：给该句补 `os/servers/vm/...` 锚点，或删去。

CF-27 `pt_alloc::register` 反事实（§4.6 L374）
  断言：`register` 用比较交换抢唯一写权，失败 panic，两种构建档都生效，无 release 静默保留。
  证据：`os/arch/src/arch/pt_alloc.rs:55-71`（`compare_exchange` + 双档 panic）；`85-95`（`alloc_pt_page` 的 “called before register” 断言，与 §4.9 一致）。
  结论：OK。最小修法：无。

CF-28 C 函数落点表（§4.7 L382-L399）
  断言：表中每个锚点存在，行为描述与代码相符；`enable` 含 EFER.NXE 与 `mov cr3`、CR0/CR4，且无 PSE；`alloc_lowest` 当前 64 位路径不经过“从记录最低处分配”。
  证据：`os/boot-shim/src/main.rs:33-71`；`os/libs/minix-boot/src/boot_params.rs:105`；`os/arch/src/x86_64/ap_early_entry.rs:244-258`；`os/arch/src/x86_64/paging.rs:628-650`（清零）、`725-800`（NXE/CR3/CR0/CR4.PGE，无 PSE）、`870`（`map`）；`os/kernel/src/lib.rs:1325`（建根，Step 1/2）；`os/arch/src/arch/pt_alloc.rs:85`；`os/kernel/src/smp.rs:1117`（`smp_init`）；`os/kernel/src/boot_policy.rs:98`（`plan_kernel_mapping`）。
  结论：OK。附注：`ap_early_entry.rs:238` 的注释仍说 `smp_init` “scans the free map, mirroring C alloc_lowest”，但当前代码用固定 `AP_STARTUP_PA = 0x5000`（`ap_early_entry.rs:53`、`smp.rs:1157`），切片按代码而非过期注释描述，正确。最小修法：无。

CF-29 Direct Map 链（§4.8 L405-L407）
  断言：直核 `store_boot_report` → `arch_boot_impl`（含建根与 DM）→ jump；v1 `boot_validate_and_prepare` → `store_kernel_info` → `establish_boot_dm` → jump，不调用 `arch_boot_impl`；`establish_boot_dm` 只追加。
  证据：`os/kernel/src/lib.rs:1053`（建根）、`1062`（`establish_boot_dm`）、`318-359`（v1 链）；`os/kernel/src/dm_coverage.rs:166-179`（文档）。
  结论：OK。最小修法：无。

CF-30 最小示例的预期输出（§4.10 L417-L424；对应上一版错误，已修复）
  断言（新文）：内核侧命中 16 行 `boot_stage!`（含 Step 1/2、Step 4 与 kmain 各 Phase）；镜像侧命中 6 行 `### minix-rs kernel-image`。
  证据：实跑同一条命令：`os/kernel/src/lib.rs` 16 行 `boot_stage!`；`os/kernel-image/src/main.rs` 6 行 `### minix-rs kernel-image`（L366/371/401/410/444/450）；`bootface*.rs` 无命中（其串是 “### minix-rs kernel image”，空格）。L424 的“覆盖 Step 1/2、Step 4 与 kmain 各 Phase”与 16 行内容一致。
  结论：OK（上一版错误已核销）。最小修法：无。

CF-31 小结与术语（§4.11-§4.12 L428-L440）
  断言：入口一句转交；九动作三段；接收端按 `rbx`/`x0`/`a0` 取 arena 后汇入 `resume_additive`；五腿发送侧只构造 `BootReport`；六落地区深拷；门链按腿不同；术语五条。
  证据：`os/kernel/src/lib.rs:571-613`（三个跳转寄存器约定）、`1831-1861`；`os/libs/minix-boot/src/boot_storage.rs:292`（`BootReport`）；其余同 CF-01/14/18/20。
  结论：OK。最小修法：无。

CF-32 “两条入口不共享函数、取件共用 FileLoader”（§4.2 L80；对应上一版 NOTE，已修复）
  断言（新文）：两条入口不共享函数；取件共用 `FileLoader` trait，两个实现分别走 `SimpleFileSystem` 与 `BootFileTable`。
  证据：`os/boot-shim/src/uefi_helpers.rs:1348-1352`（`UefiFileLoader`，`read` 走 `boot::get_image_file_system`）；`os/boot-shim/src/opensbi_helpers.rs:199-201`（`UbootFileLoader`，`read` 走 `self.table.find`）；`os/boot-shim/src/loader.rs:146-158`（trait 定义）。
  结论：OK（上一版 NOTE 已核销）。最小修法：无。

CF-33 §4.3 导语的残余暗示（§4.3 L175）
  断言：“三条 v1 发送腿……的模块顺序没有编译期检查……两个直核腿另有 `NAME_POOL` 与 `MODULE_NAMES` 的编译期逐字节对账。”
  证据：该编译期断言只比较 `NAME_POOL` 与其生成源 `MODULE_NAMES`（`os/kernel-image/src/bootface_common.rs:54-84`），不构成顺序守卫；直核腿的顺序守卫是 L231 已改正的 `place_modules` 运行期核对（`227-243`）。字面“有对账”为真，但在“v1 没有编译期检查”的对照下容易被读成“直核腿有顺序的编译期检查”。
  结论：NOTE（L231 已澄清，导语单独看仍有暗示）。最小修法：导语改为“……两个直核腿的 `NAME_POOL` 与 `MODULE_NAMES` 有编译期对账（只核池自身），顺序由 `place_modules` 在装载时核对”。

CF-34 “每个 cfg 产品一行”的归属（§4.10 L421）
  断言：“镜像侧命中 6 行 `### minix-rs kernel-image`（每个 cfg 产品一行）。”
  证据：6 行分布在三个 v1 接收产品上，每个产品 2 行（halt 分支与 resume 横幅）：x86_64 L366/371、aarch64 接收 L401/410、riscv64 U-Boot 接收 L444/450；aarch64 直核产品无此行（`os/kernel-image/src/main.rs:358-453`）。行数 6 正确，归属说明不准确。
  结论：NOTE。最小修法：改为“3 个 v1 接收产品各 2 行（halt 与 resume）”。

SUMMARY: errors=0 notes=6
