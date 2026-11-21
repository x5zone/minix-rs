# R36 fact 第二轮独立回读（HEAD `1e68bfc1e`）

约定：`DOC` = `/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`。本轮只读；未改源码、文档、轨迹。直接读数包括 `anchor-resolve.sh`、`doc-citation-check.py`、`cargo test --list`、实际宿主测试与 `cargo check`；未跑 QEMU。

## 一、第一轮 certify 的五个残项

- **R1（§5.1 M14 首列不再写「x0 分道」）：CLOSED** — `DOC:3177` 的 M14 行机制列为「交接三形态（§3.7/§4.8）」，覆盖列只写「接收臂 `x0 == 0` 只诚实停机」；`grep -n 'x0=0 分道' DOC` 无命中；`DOC:1454`/`DOC:3106` 与 source `/home/xzhao/github/minix-rs/os/kernel-image/src/main.rs:387-435`（两个 cfg 臂）一致。
- **R2（build_memmap 2 直接 + 12 classify = 14 个可执行点，另 1 注释提及按文件 15）：CLOSED** — `DOC:3176`、`DOC:3270` 原文即此口径；source 直接调用仅 `/home/xzhao/github/minix-rs/os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap/src/main.rs:165` 与 `test-memmap-aarch64/src/main.rs:29`；`exit_boot_services_and_classify()` 恰有 12 个测试内核调用；`test-rt-birth-aarch64/src/main.rs:107` 只在注释提及。
- **R3（check_boot_postconditions 是 > u32::MAX 的 4 GiB 门，不是「各架构 higher half」）：STILL-BROKEN** — 断言本体已正确（`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:570` 为 `kern_virt_base.0 > u64::from(u32::MAX)`，`kernel_info.rs:966-975` 负控实际跑过 `ok`），但文档/注释仍有三处旧描述：
  - `DOC:3267`：「`check_boot_postconditions` 的 higher-half 断言（v10 后唯一剩余条件）」；
  - `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:536,545`：函数 docstring 仍称 “the same set of higher-half address-space relations” / “address-space separation relations”，同文件 `:968` 测试注释仍称 “the remaining assertion is the higher-half property”；
  - `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1852-1859`：kmain 注释仍称本层检查 “entry inside the image / user stack below the kernel base / kernel stack not landing in the user half” 三条关系；当前实现只剩一条 `> u32::MAX`。
  - 最小修复：把 `DOC:3267` 改为「`kern_virt_base > u32::MAX`（4 GiB 下限）断言」；把 `kernel_info.rs:527-549` 的 docstring、`:968` 与 `lib.rs:1852-1859` 改成「v10 删除了三条派生标量关系，唯一剩余检查是 `kern_virt_base > u32::MAX`」；`DOC:2738` 的标题「高半地址关系」建议同步改为「镜像地址下限」。
- **R4（静默跳过写成 strict 门按位置核名）：CLOSED** — `DOC:2608` 写「按位置绑定会静默少初始化几个进程，现场没有缺失的名字」，`DOC:3272` 写「最后由 strict 门按 `MODULE_NAMES` 的位置核名暴露」；source `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:441-449` 正是逐位置核名，`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:2769-2776` 的模块数断言在 boot 后段、静默跳过时 len 仍为 12；`grep '内核侧的数量断言' DOC` 无命中。
- **R5（§5.1 分工说明与 §5.2.3 的「M4 只引用」不再冲突）：CLOSED** — `DOC:3160` 明确本表给「需要证明什么 + 状态 + 证据摘要」，逐字展开在 §5.2/§5.3；`DOC:3307` 明确三层判别以 §5.2.3 为权威、§5.1 M4 只引用其结论；`DOC:3167` 的 M4 行确实只给摘要并回指 `§5.2.3`。

## 二、44 点逐条

- FACT-A-01：CLOSED — `grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs` → `:296 = 10`；`DOC:1714` 写「当前 10」，`DOC:1625` 写「9 → 10」历史；`handoff_v1_read.rs:107` 精确比较；`cargo test -p minix-boot -p minix-elf` 实跑 185/27 全过。`test-sysboot` 未跑，见局限。
- FACT-A-02：CLOSED — `awk` 对 `pub trait Paging` 体内活动声明计数为 `fn=23`、`const=1`；`DOC:1878` 写「24 个成员（1 个关联常量 + 23 个方法）」且名单逐项与 `/home/xzhao/github/minix-rs/os/arch/src/arch/paging.rs:212-606` 一致；`tools/anchor-resolve.sh --check DOC` 输出 `anchors=110 resolved=110 zero-def=0 multi-def=0`。
- FACT-A-03：CLOSED — `grep 'STARTUP_RECORD_STORE_LEN\|七个落地区' DOC` 无现役命中（`DOC:139`/`DOC:2436` 的 6b/startup 均标历史）；`DOC:1583`/`DOC:2763`/`DOC:2849` 写「六个落地区加一个名字池」；citation 工具 `checked=29 failed=0`。源码旧注释残留见「附加观察」。
- FACT-A-04：CLOSED — 实跑 `cargo test -p boot-shim --features test-all --lib` → `146 passed; 0 failed`；`cargo test -p minix-boot -p minix-elf` → `185 passed` / `27 passed`；`DOC:3198`、`DOC:3201-3205` 与逐文件表 `DOC:3223-3244` 逐项相符；`grep '六枚' DOC` 命中 `DOC:3171`、`DOC:3331`。
- FACT-A-05：CLOSED — `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs` 的 `#[test]` 共 24 条；其中 `fn test_validate_rejects_*` 15、`test_validate_accepts_*` 3、`test_validate_requiring_sources_*` 5、`test_boot_postconditions_reject_low_virtual_base` 1；`DOC:3267` 正是 15+3+5+1=24。
- FACT-A-06：CLOSED — `DOC:1655`/`DOC:1657`/`DOC:1659` 写「22 个字段」，表内 `DOC:1672` 有 `user_sp` 行；对 C `kinfo_t`（`minix3/minix/include/minix/param.h:14-47`）的 26 个字段去掉 Rust 保留的 `memmap`/`module_list`/`param_buf`/`vir_kern_start`，恰余 22 个。
- FACT-A-07：CLOSED — `grep 'commit_map_tables\|push_startup\|map_tables_len\|set_memmap_len\|set_reserved_len' DOC` 无命中；`DOC:1767`/`DOC:2365`/`DOC:2432` 使用真实的 `fill_map_tables` 闭包，source `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/boot_storage.rs:140` 签名与闭包返回 `MapCounts` 一致。
- FACT-A-08：CLOSED — `DOC:1770` 写「所有权是**一个**」「直核腿在信任边界上只有一个所有者」；`grep '两个所有者' DOC` 无命中；source 两个直核面各只有一个 `static STORAGE: BootStorageCell`（`bootface.rs:147`、`bootface_a64.rs:153`），DTB 收集走 `collect_owned()`（`bootface.rs:217`、`bootface_a64.rs:212`）。
- FACT-A-09：CLOSED — `grep -rn 'build_memmap()' os/qemu-tests` 的可执行调用恰 2 处（`test-memmap`、`test-memmap-aarch64`），另 1 处仅在 `test-rt-birth-aarch64` 注释；`DOC:3176`/`DOC:3270` 写「`build_memmap()` 被 2 个测试内核直接调用」「共 14 个可执行调用点（另 1 个文件只在注释里提及，按文件计 15）」。
- FACT-A-10：CLOSED — `grep 'x0=0 分道' DOC` 无命中；`DOC:3177`/`DOC:1454`/`DOC:3106` 统一为「D-061 编译期产物分叉；`x0 == 0` 只诚实停机」；`grep '诚实停机' DOC` 命中 3 处。源码 `bootface_a64.rs` 的旧 x0 分道注释见「附加观察」。
- FACT-A-11：CLOSED — `grep 'vm_handoff.rs:605' DOC` 无命中；anchor 工具 `110/110`、`multi-def=0`，手写行号已换为 `vm_handoff.rs:fn build_vm_handoff`/`fn boot_alloc_used_bytes` 等符号锚。
- FACT-A-12：CLOSED — `grep -c 'cfg(feature = "qemu-test-kernels")' os/boot-shim/src/uefi_helpers.rs` → `10`；`DOC:1528` 写「10 处」并列 `UefiPoolAllocator`、`TestMapTables`、`build_memmaps`/`build_memmap`、`exit_boot_services`、`classify_map_bytes_into_test_tables`、`exit_boot_services_and_classify`，六类符号均在 source 中存在。
- FACT-A-13：CLOSED — `grep -c 'land_slice(' os/kernel/src/lib.rs` → `4`（`:1601/:1613/:1660/:1681`，分别 reserved/memmap/platform/source）；`DOC:2768` 引文头写「保留 helper、四处调用、仍内联的模块名/param 两块」。
- FACT-A-14：CLOSED — `DOC:3305` 写「在最终 EBS 内存图之前经 `AllocatePages(...)` 分配」；source `test-kernel-map/src/main.rs:88` 分配探针、`:96` 才调 `exit_boot_services_and_classify()`；`grep 'build_memmap() 产出的 conventional' DOC` 无命中。
- FACT-A-15：CLOSED — `DOC:2867` 写「生产路径有两个 `store_kernel_info` 调用点」，为 `arch_boot_finish` 与 `resume_additive`；source 非测试调用点仅 `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:225` 与 `:347`；`grep 'let kernel_info = store_kernel_info' DOC` 无命中，kmain 在 `lib.rs:2207` 起只读 `kernel_info()`。
- FACT-B-01：CLOSED — 同 FACT-A-01：常量 `handoff_v1.rs:296 = 10`；`DOC:1714`「当前 10」；`handoff_v1_read.rs:107` 精确比较；185/27 实跑 0 失败。
- FACT-B-02：CLOSED — 同 FACT-A-07：幽灵 API 全无命中，`fill_map_tables` 在 `DOC:1767/2365/2432` 与 `boot_storage.rs:140` 一致。
- FACT-B-03：CLOSED — 同 FACT-A-03：无「七个落地区」/`STARTUP_RECORD_STORE_LEN` 现役命中；「六个落地区」在 `DOC:1583/2763/2849`；source 六个落地区见 `globals.rs:299/329/347/368/382/402`；citation 29/29。
- FACT-B-04：CLOSED — 同 FACT-A-15：`DOC:2867` 两个生产调用点，kmain 不重存，`lib.rs:1580-1590` 一次性守卫存在。
- FACT-B-05：STILL-BROKEN — 即 R3。`DOC:2738`/`DOC:2754` 已改为 `kern_virt_base > u32::MAX`（4 GiB 门），source `kernel_info.rs:570` 与测试 `:966` 相符且实跑 `ok`；但 `DOC:3267` 仍写「higher-half 断言」，`kernel_info.rs:527-549`/`:968` 与 `lib.rs:1852-1859` 仍按旧的 multiple higher-half relations 描述。最小修复见 R3。
- FACT-B-06：CLOSED — 实跑 146/185/27 全过；`DOC:3198`/`DOC:3201-3205` 与逐文件表相符。
- FACT-B-07：CLOSED — `kernel_info.rs` 24 条（15 rejects + 3 accepts + 5 requiring + 1 postconditions）；`DOC:3267` 同。
- FACT-B-08：CLOSED — `DOC:3171` 写 M8「六枚超容量负控」，`DOC:3331` 写 TODO-03「六枚 `should_panic`（reserved 257 / memmap 129 / boot modules 13 / platform 5 / param 1025 / source 297）」；source `lib.rs:6044/6060/6076/6093/6110/6123` 六条全在。
- FACT-B-09：CLOSED — 同 FACT-A-11：无 `vm_handoff.rs:605`，anchor 110/110。
- FACT-B-10：CLOSED — `DOC:2768` 引文头写「开头的一次性守卫 assert 与相邻接续行有省略」；source `lib.rs:1580-1590` 的 one-shot 断言确实存在。
- FACT-B-11：CLOSED — `DOC:3264` 表行写「`kern_stack_top` 由访问器派生；`user_sp`/`syscall_entry` 已不上线」；source 测试 `/home/xzhao/github/minix-rs/os/boot-shim/src/opensbi_helpers.rs:806-841` 只断言 memmap/kern_* 与派生 `kern_stack_top`，未断言已撤字段。
- FACT-B-12：CLOSED — `DOC:2570-2573` 引文已含 `elf_file_end`；source `loader.rs:227` 定义、`:517` 在 `load_boot_modules_into` 使用；`opensbi_helpers.rs:210-214` 注释说明 UbootFileLoader 不再跑 `segment_iter`；citation 工具 `checked=29 failed=0`。
- FACT-B-13：CLOSED — `DOC:3468` 写「Step 1/2 映射已过、Step 2b/2c 也已完成，卡在 Step 3 `enable` 或跳转」；source `lib.rs:1475-1519` 的 Step 2c 在 `step1+2 mappings ok` 之前，`:1522` 才 `enable()`。
- FACT-B-14：CLOSED — 同 FACT-A-10：无「x0=0 分道」，`诚实停机` 命中；source `main.rs:387-435` 已是编译期两臂。
- FACT-B-15：CLOSED — `DOC:2605` 写「只有余数 r 满足 `r > 0 && r < 4096 - A` 才会少一页」；`grep 'r > 4096 - A' DOC` 无命中；反例 `data.len()=4095, A=4094` 与原文算式一致。
- FACT-B-16：CLOSED — 同 FACT-A-08：`DOC:1770` 一个所有者；`两个所有者` 无命中；source 单 `BootStorageCell` + `collect_owned`。
- FACT-B-17：CLOSED — 同 FACT-A-14：`DOC:3305` 最终 EBS 图之前；source `test-kernel-map/src/main.rs:88/96` 顺序相符；旧 `build_memmap() 产出的 conventional` 无命中。
- FACT-B-18：CLOSED — 同 FACT-A-09：2 个直接调用 + 12 个 classify = 14；`DOC:3176/3270` 写「2 个测试内核」。
- FACT-B-19：CLOSED — `DOC:2608` 写「静默少初始化几个进程」，`DOC:3272` 写 strict 门按 `MODULE_NAMES` 位置核名；`grep '内核侧的数量断言' DOC` 无命中；source `kernel_info.rs:441-449` 位置核名、`lib.rs:2769-2776` 的 len 断言在 boot 后段且不会因静默跳过触发。
- DESIGN-D2-01：CLOSED — `handoff_v1.rs:296 = 10`；`DOC:1714`「当前 10」；`handoff_v1_read.rs:107` 精确比对；`handoff_v1.rs:428` `size_of::<BootHandoffV1>() == 128`；`WIRE_SOURCES_END == 29184` 与 `ARENA_CAP - WIRE_SOURCES_END == 3584` 编译期断言在 `handoff_v1.rs:450-455`。`test-sysboot` 未跑。
- DESIGN-D2-02：CLOSED — 同 FACT-A-03/B-03：doc 无现役 seven-region/startup store；source 六落地区；strict 门 source `kernel_info.rs:497-510` 检查内核跨度落在 `memmap` 或 `reserved_regions`，与 `DOC:2739` 一致。
- DESIGN-D2-04：CLOSED — `bootface_a64.rs:114-135` 的 `bump_alloc_pages` 用 `minix_boot::BumpRegion::resumed/alloc_pages/next`，游标为 `static BUMP_NEXT: AtomicU64`；`cargo check -p kernel-image --target aarch64-unknown-none --features fw-aarch64-none` 退出 0；`test-rt-birth-aarch64` 未跑 QEMU。
- DESIGN-D2-06：CLOSED — `paging.rs` 24 个成员（23 fn + 1 const）；`DOC:1878` 名单逐项一致；`DOC:2082` 补了 `new_from_page(root_page, channel: PteChannel)` 与 `PteChannel::Identity/KernelDm/VmDm`，source `paging.rs:197-210` 三态齐全。
- DESIGN-D2-07：CLOSED — `uboot_main.rs:293` `static mut SHIM_TABLE: BootFileTable`；`:301` `size_of::<BootFileTable>() == 16 + 16 * 80` 编译期断言；`build_shim_table` 按 typed 字段写入（`:382` 起），不再按裸字节偏移镜像；`test-riscv64-uboot-shim.sh:206-209,231` 的 6 条标记存在，未跑 QEMU。
- DESIGN-D2-08：CLOSED — 同 FACT-B-12：`elf_file_end` 在 `DOC:2572`，source `loader.rs:227/517`，UbootFileLoader 不再跑 `segment_iter`；citation 29/29。
- CLAIM-03：CLOSED — 同 FACT-A-01/D2-01：常量 10、文档「当前 10」、精确比较、128 B/29184 B/3584 B 断言。
- CLAIM-04：CLOSED — 同 FACT-A-03/B-03：source 六个落地区加名字池，生产 `store_kernel_info` 调用点 2 个（`lib.rs:225/347`），不存在「七个落地区 + 三个生产调用点」。
- CLAIM-05：CLOSED — `DOC:1925` 写「不得重复安装同一个叶」，`DOC:2939`/`DOC:2940` 写 2 MiB 分支对已存在的 1 GiB 叶或 2 MiB 叶返回 `AlreadyMapped`；source 三架构一致：x86_64 `paging.rs:1309-1326`、arm64 `paging.rs:1230-1245`、riscv64 `paging.rs:1221-1245`；`grep '重复安装同一个叶' DOC` 命中。
- CLAIM-06：CLOSED — 同 FACT-A-04/B-06：实跑 146/185/27 全 0 失败；`grep '六枚' DOC` 命中 M8/§5.3。

## 三、汇总

- **44 点：43 CLOSED / 1 STILL-BROKEN（FACT-B-05，即 R3）。**
- **五个残项：4 CLOSED / 1 STILL-BROKEN（R3）。**
- 直接读数：anchor `110/110`、`multi-def=0`；citation `checked=29 failed=0`；`cargo test --list` 与实跑均为 146/185/27、0 failed；`test_boot_postconditions_reject_low_virtual_base` 实跑 `ok`；`cargo check` aarch64 退出 0。
- 未跑 QEMU，因此 `test-sysboot`、`test-riscv64-uboot-shim` 6/6、`test-rt-birth-aarch64` 到 `entering scheduler` 只做了脚本/source 静态核对，不作为独立运行证据。

## 四、局限与清单外附加观察

以下四处不属于 44 点清单的期望复核点，但同属本轮「v10 删除传播 / D-061 形态判据 / 探针时序」的源码注释残留；未计入上面的 43/1。若按「文档-代码同步」的更严口径，建议作为新发现处理：

1. `/home/xzhao/github/minix-rs/os/kernel-image/src/bootface_a64.rs:31-35` 仍写「`rust_image_main` splits the two forms on `x0==0`」，`:147-148` 仍写「`_start` … branches on `x0==0`」。HEAD 的 `main.rs:387-435` 是两个 cfg 臂，直核 `_start`（`main.rs:175-227`）不读 x0。最小修复：改为 D-061 编译期产物分叉；UEFI 接收臂的 `x0==0` 只作诚实停机。
2. `/home/xzhao/github/minix-rs/os/boot-shim/src/uefi_helpers.rs:1229` 仍写「The classifier wrote the source/startup records directly into the wire」。v10 已无 startup 记录。最小修复：删 `/startup` 或改述为「source records（startup 记录已随 D-062 删除）」。
3. `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1_read.rs:761-765` 有三段悬空注释仍引用已不存在的 `WIRE_STARTUP_CAP`/startup-object partition。最小修复：删除这三段（`grep 'WIRE_STARTUP_CAP' os/` 现仅此处命中）。
4. `/home/xzhao/github/minix-rs/os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map/src/main.rs:9`、`:73` 仍写探针页在「memmap snapshot」之后分配；实际是 `:88` 在 `:96` 的最终 `exit_boot_services_and_classify()` 之前分配，`DOC:3305` 的「最终 EBS 内存图之前」才对。最小修复：把两处注释改成「before the final ExitBootServices memory map」。

- 历史回执：本 pass 的结论以正文汇总为准（pass1 38 CLOSED/6 STILL-BROKEN、pass2 43/1）；本轮最终结论 CLOSED/PASS（44/44，见 `round36-fact.md`）。
