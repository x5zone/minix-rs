# D01 开放欠账（open-debt）

> 本表只收**跨 crate / 需要独立项目**的 ② 项（负责人 + 截止轮次），按 `prompt/review-rules/review-evals.md` §4.0f（V3.3 单一未闭环口径）与 G9 管理。
> `adjacent` 项不进本表：它们在 `follow-ups.md` 登记目标轮次。
> **本表的未销账行计入未闭环（V3.3 §4.0f）**：有未销账行 = 本轮不得关闭、不得派下一轮面板。
> 只有两个出口：实施（附代码证据或 `dup-of` 指向覆盖根因）或 `DECISIONS.md` 判决账行；没有第三个状态。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| F-R17-02 | P2 | [CLOSED] ✅ 已修：UEFI 测试辅助三件套 + `UefiPoolAllocator` + 四个 static 收进 `qemu-test-kernels` feature；16 个测试内核 manifest 同步；生产 `fw-*-uefi` 不再链接；证据见 01-todo.md 与对应测试门 |
| F-R17-04 | P2 | [CLOSED] ✅ 已修（R23 前置）：`KernelInfo` 增第 14 个字段 `authority: InputObjectId`，`validate_requiring_sources()` 改无参读字段；wire 解码复制 `ram_authority`，直核腿传 `DtbBootInput`、UEFI 腿传 `UefiMemoryMap`；文档 13→14 与绑定说明同步；`minix-boot` 192 通过；证据见 01-todo.md 与对应测试门 |
| F-R17-01 | P2 | [CLOSED] ✅ 已修（R23 前置）：删除单实现 `BootShim` trait，U-Boot 腿改普通函数 `opensbi_helpers::prepare_boot`；§3.2 按事实重写；`impl BootShim` 与 `dyn BootShim` 零命中；三 QEMU 门 PASS；证据见 01-todo.md 与对应测试门 |
| F-R17-03 | P2 | [CLOSED] ✅ 已修：`static mut PHASE` → `PhaseCell(UnsafeCell<Phase>)`，`Sync` 的安全论证集中一处；无 `static mut`；证据见 01-todo.md 与对应测试门 |
| F-R20-01 | P2 | [CLOSED] ✅ 已修：`BootPrepareResult.bump: PhysRange`、`arena_pa/root_page: PhysBytes`；`enter_kernel_image_v1` 收 `PhysBytes`；证据见 01-todo.md 与对应测试门 |
| F-R20-02 | P2 | [CLOSED] ✅ 已修：`PhysRange`（构造拒绝反向）统一 `FreeRangeSet::cut`，调用点不再传 `(start,size)`；证据见 01-todo.md 与对应测试门 |
| F-R20-03 | P2 | [CLOSED] ✅ 已修：四个 `static mut` 表 → write-once `TestMapTables`（AtomicBool 单写 + Release 发布，初始化后只读）；`qemu-test-kernels` 门控，生产不链接；`'static` 由真实 static 生命周期支撑；证据见 01-todo.md 与对应测试门 |
| F-R20-04 | P2 | [CLOSED] ✅ 已修：补 reserved / from_memmap / dtb 覆盖的溢出/反向负控；minix-boot 192、kernel 862+8；证据见 01-todo.md 与对应测试门 |
| F-R23-03 | P2 | [CLOSED] ✅ 已修（R23 后）：删除恒空 `kern_ident`/`kern_ident_count` 与 `VM_BOOT_HANDOFF_MAX_IDENT`；`VM_BOOT_HANDOFF_VERSION` 8→9；VM server 与测试同步；证据见 01-todo.md 与对应测试门 |
| R26-design-batch | P1 | [CLOSED] 已实施：D2-01/02/03/04/05 在 R27 code batch 落码或被后续轮取代；D2-07..12 核心项（BootStorage 统一、BootStorageCell、store_kernel_info 返回值）已在 R29/R30 落地；残余项已由 R34/R35/R36 批逐条关闭。证据见 `01-todo.md` 与 `cargo test -p minix-boot --lib` 187/0。 |
| R34-design-batch | P2 | [CLOSED] 已按 `D-107` 全部落码：D2-02/03/04/05 分别由 `01-todo.md` I-34/I-31 与 `store_kernel_info` 一次性落地覆盖；D2-04 wire 标量随 v10 删除。证据：`cargo test -p minix-boot --lib` 187/0、`cargo test -p minix-kernel --features mock --lib` 866/0/8。 |
| F-R34-01 | P2 | [CLOSED] 已实施（按 `D-107`）：`check-layout.sh` 的 L9 按架构选 direct face——x86_64/riscv64 读 `src/bootface.rs`、aarch64 读 `src/bootface_a64.rs`，三架构均做 ELF 跨距与 `KERN_SIZE` 等值对账。验收：`bash os/kernel-image/check-layout.sh all` → x86_64/aarch64/riscv64 三条 L9 全 PASS（各 0x400000 = 0x400000），总 PASS。 |
| R35-design-batch | P2 | [CLOSED] 已按 `D-107` 全部落码：D2-07 的 `PreparedJump` 见 `os/kernel/src/lib.rs`；D2-08 字段更名 `source_records`（提交 `e27dc513b`）；D2-10 新增 `RamAuthority`（提交 `aa0d5393c`）；D2-11 `free_pt_page` 未注册改为 fail-closed（提交 `88f3cb96a`）；D2-12 `write_envelope` 收为测试/载体专用；其余项随 v10 或既有提交关闭。 |
| I-44（= R35 `DESIGN-D2-11` 的十轮重启） | P3→设计裁决 | [CLOSED] **D-078（用户 2026-11-20）**：概念设计冻结通过、代码实施现在开始；两项独立实施门 R1（共享源结构稳定性，`lib.rs:1707`/`:2928` 是否在继承者存活期间替换共享子树）与 R2（`test-sysboot:242` 快照未扣除 `:254` 的消耗，真实 VM 接入前必修）。设计文档 `rewrite-notes/01-stage-kernel/01-review-request-page-table-free.md` 第十版；实施台账见 `01-todo.md` §3 的 I-44；用户批准 2026-11-20（DECISIONS D-078） |
| I-40（= I-39 mini review 的两处 ghost 注释） | P2 | [CLOSED] **D-079（用户 2026-11-20）采 A**：改写措辞指回真实载体（`KernelInfo::boot_modules()` → `KernelInfo::boot_modules` 字段；`KernelInfo::memmap()` → `KernelInfo::memmap` field）。跨 crate（`minix-boot` + `minix-kernel`）；纯注释零行为变化。第三处 `KernelInfo::kern_stack_top()`（`boot-shim/src/lib.rs:103`）经核实不是 ghost（`kernel_info.rs:200` 有该方法），已排除；用户批准 2026-11-20（DECISIONS D-079） |
| I-42（= I-34 mini review 的 `push_*` 死子系统） | P2 | [CLOSED] **D-080（用户 2026-11-20）采 A**：射程核实后只剩 `BootStorageFull` 空结构体（`push_module` 与 `push_platform_source` 在 I-34 落地 `fill_*` 时已随之删除）。删 `boot_storage.rs:291-293` 与 `lib.rs:31` 的 re-export；纯删除零行为变化；用户批准 2026-11-20（DECISIONS D-080） |
| I-43（= I-31 补漏批二的空槽 `BOOT_MODULES`） | P1 | [CLOSED] dup-of：后续 D-087（同一 I-43 家族）已把 12 个载体全部改为 `PLACEHOLDER_SPAN`，并把三层残留拆为 I-52/I-53；见本表后一行与 `01-todo.md` 的 I-43/D-087。 |
| I-36（= R35 `DESIGN-D2-05` 的 ELF 修剪） | P2 | [CLOSED] **D-082（用户 2026-11-20）采「删重复 + 修剪上移」**，不引入新 trait：① 删 `UbootFileLoader::read` 里冗余的 `segment_iter`（实测表项 `len` 已是修剪值）② 修剪上移到 `load_boot_modules_into` ③ 复用 `minix_elf::segment_iter`。否决「修剪 trait + 空实现」（无哪个腿不需要修剪）。② 落地前须验证 12 个模块 PT_LOAD 覆盖真实内容；用户批准 2026-11-20（DECISIONS D-082） |
| RD3-D2-01（= D02 面板对同一缺陷的第二次发现） | P1 | [CLOSED] **D-083（用户 2026-11-20）`dup-of: D-071`**：`store_kernel_info` 是安全 fn 却返回 `&'static` 并在内部无条件改写同一槽（`lib.rs:1676`），`kmain:1858` 二次 store 构成别名漏洞。与 D-071（I-35）同一处代码，不另立方案；随 I-35 一并实施。复核修正五处漂移行号并补 C 侧 `pre_init.c` 拷贝一次的对照；用户批准 2026-11-20（DECISIONS D-083） |
| RD3-D2-02（= `SegmentIter` 畸形 ELF 静默跳过） | P1 | [CLOSED] **D-084（用户 2026-11-20）梭哈 A**：`Item` 直接改 `Result<LoadSegment, ElfError>`，不走 B 过渡（无外部消费者、保留旧函数等于给缺陷留后门）；`filesz > memsz` 检查收进迭代器（从 1/7 覆盖提升到 7/7）；`load_segments_into_buffer` 返回类型同批改。台账见 `01-todo.md` 的 I-45；用户批准 2026-11-20（DECISIONS D-084） |
| I-41（= `BootReport` 三零读者派生字段） | P2 | [CLOSED] **D-085（用户 2026-11-21）采 A**：删 `boot_storage.rs:314-316` 三字段、`:278-280` 三行赋值、`:385-387` 三条断言。否决 B（三个都是可派生值：常量 / 专门函数 / shim 内无语义）。裁决前实测生产读者零、`from_report` 只读三个 `kern_*`、wire 编码器不读它们。台账见 `01-todo.md` 的 I-41；用户批准 2026-11-21（DECISIONS D-085） |
| I-33（= DTB 静态表去重，原 D-065 被实施受阻推翻） | P2 | [CLOSED] **D-086（用户 2026-11-21）拆三项**：丙（新，最高优先）给 `DtbMapOwned` 加 `panorama` + `resv_claims::capture` 改走 `collect_owned`——**修复真机 EBS 重试 attempt 2 的 panic**（`DTB_COLLECT_TAKEN` 与 UEFI 标准重试冲突，且被 `cfg(not(test))` + 测试形态掩盖）；甲（两直核腿改 owned）；乙（删四张静态表 + 62 处测试）。依赖 丙 → 甲 → 乙；用户批准 2026-11-21（DECISIONS D-086） |
| I-43（= 测试内核空槽 BOOT_MODULES 家族） | P1 | [CLOSED] **D-087（用户 2026-11-21）采 C 关闭**：一层（12 载体全 `PLACEHOLDER_SPAN`）与二层（`enable()` 后静默停双根因：载体装载地址改用 `shim_image_window()` + `arch_boot_impl` 补压调用者栈窗口）均已达成原始验收（x86 13/14、aarch64 5/5、riscv hello-boot PASS）。三层残留拆出：三条地址假设失配 → **I-52**；故障臂路由 → **I-53**；`test-rt-birth-riscv64` 的 `bump(256)` 为明示遗留；用户批准 2026-11-21（DECISIONS D-087） |
| RD3-D2-03（= `load_segments_into_phys_memory` Safety 前提） | P2 | [CLOSED] **D-088（用户 2026-11-21）采「只改 SAFETY 文档」**：I-48 落地（`a25305c66`）后复核，本条只剩一份过期文档——迭代器已自查 `filesz > memsz`，而 `loader.rs:275-279` 仍写「本函数不拒绝」。② 面板称的文档失真实测不存在 → 销账；③ entry 断言两份拷贝不合并（守卫时机不同）。面板 `ValidatedKernelImage` 与改 wrapper 形状两案不采纳（I-48 后净收益为零）；用户批准 2026-11-21（DECISIONS D-088） |
| RD3-D2-04（= `load_segments_into_buffer` 计数与语义） | P2 | [CLOSED] **D-089（用户 2026-11-21）采乙**，照 `minix3/minix/lib/libexec/exec_elf.c` 的 `libexec_load_elf` 两循环结构重排：校验前置（不通过不写任何字节）+ 装不下返回 `Err(TargetBufferTooSmall{..})`（C 用 `ENOEXEC`）+ 文档改「测试投影」。不补段间重叠检测（C 也未在装载循环里做，其防线在 mmap/copymem 实现侧）。射程由「修 count 语义」扩大为结构重排；用户批准 2026-11-21（DECISIONS D-089） |
| I-50（= x86 模块 `.lbss` 无 PT_LOAD 覆盖） | P3 | [CLOSED] ✅ **D-090（用户 2026-11-21）销账·前提不成立**：对 115 个 x86_64 ELF 逐文件对账，112 个带 `.lbss` 的**全部完整落在某 PT_LOAD 的 `memsz` 内**（`minix-fs-mfs` 的 4 MiB `.lbss` 上界与最后 PT_LOAD 的 memsz 结束地址完全相等）。`filesz` 之外是 NOBITS 正常语义；装载路径「拷 `[0,filesz)` → 清零 `[filesz,memsz)`」已覆盖。证据边界：静态检查+代码语义，非运行时实证 |
| I-51（= 测试面并行隔离失败） | P2 | [CLOSED] **D-091（用户 2026-11-21）判「待复核后销账」**：症状 1（vm 10 条共享 `PAGE_ALLOC_PTR`）实测消失（连跑 3 次 548/0），成因已定位——16 处测试自己 `unregister`/`register` 配对、各用栈上 allocator，并行时互相抹指针；**须复核「为什么现在全绿」（甲已加隔离/乙测试被改/丙偶然），判丙则须补 `test_sync` 式全局锁**。症状 2（`memmap_source` 族漂移）当前无法验证：`minix-boot` 测试面编译失败，因 D-086 丙批量替换误把 4 处 `Iterator::collect()` 写成 `collect_owned()`（未提交工作树改动，归该条收尾；本条不插手）；用户批准 2026-11-21（DECISIONS D-098） |
| I-52（= I-43 三层残留的深跑地址失配） | P2 | [CLOSED] **D-092（用户 2026-11-21）：不预分三条、保持一条，下一步 `-d int`**。地址形态实测推翻「同根」：riscv `stval=0xffffffc0cffff000` 是 Sv39 规范高半、偏移 `0xcffff000`≈3.3GiB；aarch64 `far=0xffff80804ffff000` 是 VA48 规范（主 agent 曾误用 Sv39 判非规范）、偏移 `0x804ffff000`=128×4GiB+0x50000000（疑位域污染）；x86 `cr2=0x7ba7000` 是规范低地址≈123MiB、疑与高半假设无关。**不预分理由**：拆分前提是性质不同，而当前只是地址形态推测、未到根因；用户批准 2026-11-21（DECISIONS D-094） |
| I-53（= 故障臂路由，原定性作废） | P2 | [CLOSED] **D-093（用户 2026-11-21）：原定性作废，拆 I-57/I-58**。审查证实 `is_user_mode=(cs&3)==3` 与 `is_nested=!is_user` 均正确（C 双入口 `mpx.S:368/382` 语义已正确映射），wrapper 判据与注册顺序也对。真实病因两条均在载体侧：**I-57**（faultloop 的 `carrier_pagefault` 在 CPL0 执行，`walk_leaf:314` 的 `alloc_pt_page().expect` / `:437-440` 的 VM 槽 expect）、**I-58**（user-trap 触发 `trap_dispatch:465-472` 的 `proc_ptr=None` 断言——该断言正确，问题在载体的 CPL3 载荷时序）。两条均待 `-d int`/串口取证；用户批准 2026-11-21（DECISIONS D-098） |
| I-59（= `Paging` 写通道双轨，三条载体共同根因） | P1 | [CLOSED] **D-094（用户 2026-11-21）扩为三条共同根因；I-52 关闭并入**。三条故障地址减去各架构 `KERNEL_DIRECT_MAP_BASE` 后都是页对齐的正常 PA（aarch64 `0x4ffff000` / riscv `0x8ffff000` / x86 裸恒等），同属「写通道与实际映射不匹配」。修法采**修正版 A'**：`new_from_page` 加 `channel: PteChannel` 参数（与 I-44 已加的 `from_active_root(root, channel)` 对称），bootstrap 传恒等通道、`load_process_elf` 传 `KernelDm`。副产物 **I-60**（双通道并存无文档，P2）；用户批准 2026-11-21（DECISIONS D-095） |
| I-60（= 双通道并存，D-094 副产物） | P2 | [CLOSED] ✅ **D-095（用户 2026-11-21）销账·伪缺陷**：双通道是按阶段分工而非并存竞争——裸恒等用于 DM 未建（`map_huge` 5 处生产调用全在 `build_bootstrap_root_and_enable` 内 / `install_leaf` 是 DM 安装器本身）、KDM 用于 DM 建立后；`split_huge:997` 本就走 channel。唯一真错的 `new_from_page` 在 `load_process_elf` 处已含在 I-59。同时收窄 I-59 的 A' 范围：仅 `new_from_page` 1 处 × 3 架构 + 2 调用点，`map_huge`/`split_huge`/`install_leaf` 不用动 |
| E-VMTLB（= D02 登记的 kernel 侧 TLB 刷新机制） | 跨阶段 | [CLOSED] ✅ **D-096（用户 2026-11-21）销账**：九项组件全部实现——`FLUSH_TLB` 标志、`mark_flush_tlb` 无条件置位、`needs_tlb_refresh` 双条件判定（在 `switch_address_space` 之前）、`consume_flush_tlb_flag`、`flush_all` 与 C `refresh_tlb` 逐指令等价；「同根跳过优化」已改成读活 CR3 并修了 mirror 漂移 bug。**保留**：依据是代码+C 语义对应，非 SMP 真机实证。D02 `:187` 已加更正注记 |
| I-57 / I-58（= I-53 拆出的两条载体缺陷） | P2 | [CLOSED] ✅ **D-097（用户 2026-11-21）双双销账**：实跑取证 `test-paging-faultloop.sh` PASS（kbox `fault count==1` 无 re-fault + `0xFEED` legs OK）、`test-user-trap.sh` PASS（E1 slice 5 + E-KERNINFO）。**根因重定位**：fault 在更早的「zero mailbox + copy payload」阶段——载体注释「identity map makes the first 4 GiB kernel-writable」是 BOOT-SCOPE 2a 之前的假设，CPL3 配套物理页无人声明恒等窗口。修法：`ds` 槽 span 改指真实数据页块 `[0x400_0000, 0x402_0000)`（零内核改动）。D-093 时对两处的怀疑均被证伪 |
| I-51 / I-53（= 测试面并行隔离 + 故障臂路由） | P2 | [CLOSED] ✅ **D-098（用户 2026-11-21）双双销账**：**I-51** 成因是第四种（cargo config 按 cwd 发现，从仓库根跑 `--manifest-path os/Cargo.toml` 时 `os/.cargo/config.toml` 的 `RUST_TEST_THREADS=1`/`RUST_MIN_STACK` 钉未被发现 → 并发 → 16 处栈上 allocator 配对竞态），修复为仓库根 `.cargo/config.toml` 加 `include` 桥（`c1f17a449`），验收 vm 548×10 / boot 184×5 / kernel 864/0 全绿；**不需全局锁**（非调度偶然）。**I-53** 两条子项均结清（D-097 门链 PASS），原定性作废的结论保留为记录；用户批准 2026-11-21（DECISIONS D-098） |
| WIRE-v10-batch | P2 | [CLOSED] **用户批准 2026-11-20（用户裁决 D-062）批准实施、可立即开工**：三条 wire 字段删除一次性升版（`BOOT_HANDOFF_V1_VERSION` 9→10）：① 删 `KernelInfo.syscall_entry`（零读者、值错误、C 无此字段）；② 删 `user_sp`/`kern_stack_top`（两端都能算出的常量，C 侧 `user_sp` 是编译期常量 `USR_STACKTOP`）；③ 删 `startup_objects` 分区（20 槽 1440 B，唯一读者一条 `any()` 断言）。前提已实跑验证：五条腿内核跨度全部落在 `memmap` 或 `reserved_regions` 内。实施细节与验收见 `01-todo.md` 的 I-31 |
| ARCH-01-aarch64-link-split | P2 | [CLOSED] **用户批准 2026-11-19（用户裁决 D-061）批准实施、可立即开工**：D01 的 ARCH-PROPOSAL-01——aarch64 内核镜像的形态判别从运行期 `x0 == 0` 哨兵改为**链接期分叉**，与 riscv64 `fw-riscv64-uboot` 同构（直核 `-kernel` 变体 + UEFI v1 接收变体，形态由构建产物决定）。**已实施**：链接期分叉已落码（`01-todo.md` 的 I-30；`fw-aarch64-none` 与 `fw-aarch64-uefi-recv` 两个构建产物，`main.rs` 按 feature 选入口）。不做会坏在哪：`x0` 零值非规范承诺，任何在 `x0` 放非零值的加载器会把直核启动送进 v1 解码，症状是 `kernel-image: handoff v1 envelope invalid` panic 且发生在 `early_console` 之后 |
| BOOTSTORAGE-fill-api | P2 | [CLOSED] **用户批准 2026-11-20（用户裁决 D-067）批准实施、可立即开工**：`BootStorage` 半提交防护改造——删 `map_dirty`（borrow checker 已禁掉「边写边读」，该字段防错了对象）；五族改用 `fill_*` 闭包（闭包返回实际写入量并当场提交，可内置断言）；内存图两族改用 `fill_map_tables` + 新增 `MapCounts { memmap, reserved }` 值对象；`ParamStats` 的 pub 字段不动。详见 `01-todo.md` 的 I-34 |
| I35-write-once | P2 | [CLOSED] 已实施：`store_kernel_info` 的 one-shot 断言与 `.bss` 落地见 `os/kernel/src/lib.rs:fn store_kernel_info`；`Frozen::freeze` 的 `debug_assert!` 已并入 I-32（D-064）；`IPC_FILTER_POOL` 注释已改事实。证据：`cargo test -p minix-kernel --features mock --lib` 866/0/8。 |
| — | — | [CLOSED] F-R18-01 已实施；R14/READER-A-16 为 adjacent，见 `follow-ups.md` |

## 本次核实状态（D-107 终态）

- 本表所有数据行均以 `[CLOSED]` 开头，附代码/测试/commit 证据或 `dup-of` 指针；不再有未销账行。
- F-R34-01 已按 D-107 实施：`check-layout.sh` 的 L9 按架构选 `bootface.rs`/`bootface_a64.rs`，三架构等值对账 PASS。
- I-29-16、I-29-9、D-59 的物理机证据按 D-107 以现有 QEMU/OVMF 读数收口，标 `external-evidence-waived`：`evidence/I-5-acpi-ovmf.txt`（RSDP @ 0x777e000、nr_cpus=4、PASS）、`evidence/T-10-uboot-shim-6of6.txt`（shim 6/6 markers）。
- C1 / I-29-8 十项按 D-107 全部批准，已由 `01-todo.md` 的对应代码项落码。

## 重写基线（2026-11-21）

- 本节只登记“重写未替旧债销账”：上方 39 行仍全部 `[CLOSED]`，本表不新增欠账。
- 01/02 两篇重写后的新 review 基线见 `FREEZE.md` 的“重写基线”节；旧 R36 的 CONVERGED 已作废。

## R37 重写后状态（2026-11-21）

- 本表上方数据行仍全部 `[CLOSED]`；重写未替旧债销账。
- 本轮未闭环 0、certify 新准入 0；不新增代码欠账。
- 未达 CONVERGED 的三项是门级/外部项，已登记 `follow-ups.md` 的 F-R37-01..03：G11 code-publication 126 处存量注释、G13 tutorial-rubric 6/6 缺项、`D01-EXTERNAL-D107` 外部哨兵。
