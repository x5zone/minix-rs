# R38 · 反证者（P-REFUTE）· 01-boot-shim-bootstrap

只读复核；源码行号取自 2026-11-22 工作树（rustc 1.94.1）。五条主张 + 一条表述主张，逐条造反例。

## REFUTE-01 五条生产腿与接线状态

- 主张：五条生产腿 = 两条 UEFI 链 + riscv64 U-Boot shim + riscv64 `bootface` + aarch64 `bootface_a64`；riscv64 UEFI shim 未接线是工具链事实。
- 反例检索：找第六个生产入口、找运行期形态判别残留、找 riscv64 UEFI 目标是否真的不存在。
- 证据：`os/boot-shim/Cargo.toml` 两个 `[[bin]]`（`boot-shim` 要 `fw-uefi-image`、`riscv64-uboot-shim` 要 `fw-riscv-uboot`）；`os/kernel-image/Cargo.toml` 的 `fw-aarch64-none`/`fw-aarch64-uefi-recv`/`fw-riscv64-none`/`fw-riscv64-uboot` 四特性；`os/kernel-image/src/main.rs:346-413` 的 cfg 分道（aarch64 接收臂 `x0==0` 时 halt，不再转自举，D-061 已落地）；`test-smp-aps-aarch64.sh:80` 确实用 `--features fw-aarch64-none`。`rustc --print target-list | grep uefi` → 只有 `aarch64/i686/x86_64-unknown-uefi`，无 riscv64。
- 分级：反证失败（主张成立）。
- 最小修复：无。

## REFUTE-02 wire v10 与 KernelInfo 10 字段

- 主张：version=10、信封 128 B（v9 160→128）、arena 八页已用 29184 B、余 3584；v10 删 `startup_objects`/`user_sp`/`kern_stack_top`/`syscall_entry`；`KernelInfo` 10 字段。
- 反例检索：找 v9 残留常量、找信封字段数不符、找仍以结构字段存在的三个派生值。
- 证据：`rg -n 'BOOT_HANDOFF_V1_VERSION|ARENA_CAP|WIRE_SOURCES_END' os/libs/minix-boot/src/handoff_v1.rs` → `:278 =10`、`:392 8*4096`、`:435 assert!(WIRE_SOURCES_END==29184)`、`:410 assert!(size_of::<BootHandoffV1>()==128)`、`:464 余 3584`；`WIRE_STARTUP` 全仓零命中。git 交叉核对：`git show 52110580f^:os/libs/minix-boot/src/handoff_v1.rs` 的结构体确有 `user_sp/kern_stack_top/syscall_entry/startup_byte_len` 四字段（4×8=32，160→128），`git show 10ecd8ef4:...` 的 v9 常量为 `WIRE_STARTUP_END`/30624。`KernelInfo` 实文 `os/libs/minix-boot/src/kernel_info.rs:20-80` 共 10 个 `pub` 字段；`kern_stack_top` 只剩派生访问器 `:193`。
- 分级：反证失败（主张成立）。
- 最小修复：无。

## REFUTE-03 memmap/reserved 来源三组与 UEFI 内核跨度归属

- 主张：UEFI 腿在 EBS 配对循环分类、内核跨度经 `AllocateAddress(LOADER_DATA)` 落 `ReservedMapped` 而非 free list；U-Boot/riscv 直核用 DTB（`fw_end=KERN_PHYS_BASE`）；aarch64 直核 `fw_end=0`。
- 反例检索：找 UEFI 腿绕过配对循环的分类路径、找内核跨度留在 conventional 的路径、找 `fw_end` 与 `KERN_PHYS_BASE` 不一致。
- 证据：`os/boot-shim/src/uefi_helpers.rs:69-92 reserve_kernel_span`（`AllocateType::Address` + `LOADER_DATA`，失败 `panic` 不降级）；`os/boot-shim/src/ebs_map.rs:638-643`（`LOADER_DATA`→`ReservedMapped`）；`os/boot-shim/src/opensbi_helpers.rs:81,291`（`KERN_PHYS_BASE=0x8020_0000`→`DtbMemoryMap::new`）；`os/kernel-image/src/bootface.rs:142`（`fw_end: KERN_PHYS_BASE`）、`bootface_a64.rs:148`（`fw_end: 0`）。反向路径存在但已被文档登记：无 `WRITE_BACK` 时 `LOADER_DATA`→`Side`（Step 5 先于类型表），内核跨度两表皆无 → strict 门 fail-closed；文档 §6.3 有对应失败行。
- 分级：反证失败（主张成立；失败分支已被 §3.2.1/§6.3 覆盖，不算省略）。
- 最小修复：无。

## REFUTE-04 I1–I4：I1/I2/I4 成立，I3 的判据句反例成立

- 主张：I1 值跨镜像、I2 每字节可追溯、I3 `BootStorage` 单所有者且「第二次 fill panic」、I4 校验先于消费。
- 反例检索：I1 找 wire 指针；I2 找无来源的 free/reserved 字节；I3 逐个 `fill_*` 找无 one-shot 位的家族；I4 找先建表后校验的分支。
- 证据：I1 `WireVirtAddr/WirePhysAddr` 为 `u64`（`handoff_v1.rs:30-35`，`:406-407` 断言 8 B），信封无指针。I2 `audit_source_coverage_for`（`memmap_source.rs:352-397`）逐 span 要求 `Memory` 类 / reservation 类记录。I4 `os/kernel/src/lib.rs:214-227`（先 `validate_before_tables`+postconditions+store 再 `arch_boot_impl`）、`:653-742`（strict→postconditions→resume_pt_alloc→additive）。**I3 反例**：`os/libs/minix-boot/src/boot_storage.rs:99-105` 只有 `modules/platform/param/sources` 四个 committed 位，`fill_map_tables`（`:139-157`）只断容量、无 one-shot 断言——第二次调用会静默覆盖两表计数；唯一二次调用负控是 `fill_modules_twice_is_refused`（`:395-400`），无 map 版本。文档 `01-boot-shim-bootstrap.md:70` 的可观测判据、`:1837` 的「第二次 fill 会 panic」与 `:1841` 的「半提交 → 第二次调用 panic」因此对 5 个 `fill_*` 之一不成立。
- 分级：**阻断（P1）**——I3 本体（单所有者、半提交不可表示）仍成立，但作为判据的绝对句为假。
- 最小修复：`:70` 改为「四族 `fill_*` 二次调用 panic；`fill_map_tables` 由 `MapCounts` 双计数一步提交」；`:1837`/`:1841` 同句加限定「走 committed 位的那四族」。

## REFUTE-05 接收侧只做 additive、不重建活动根

- 主张：三条 v1 接收腿只走 validate→DM→jump，不得 `new_from_page` 清活动根。
- 反例检索：接收链上找 `new_from_page`/`build_bootstrap_root_and_enable`/清根写点，找 aarch64 分支绕过。
- 证据：`rg -n 'fn resume_additive' -A 45 os/kernel/src/lib.rs` → `:299-344` 体内仅 validate、DM 计划、`set_current_root_phys`、store、panic hook、`establish_boot_dm`（注释标 additive）、`jump_to_kmain`；`:653-742` 与 `:746/:754/:773` 的三个 `resume_after_v1_decode` 壳全落 `resume_additive`；`rg -n new_from_page os/kernel/src/lib.rs` 非测试命中只有 `:1344`（`build_bootstrap_root_and_enable` 内）。
- 分级：反证失败（主张成立）。
- 最小修复：无。

## REFUTE-06 落地区/名字池/诊断清单的表述完备性

- 主张：六个落地区 + 名字池的容量与溢出行为按 §4.5 表执行；失败路径都有可读信号（§6.1/§6.3）。
- 反例（均非阻断）：① §4.5 表 `BOOT_MODULE_NAME_STORE` 行与正文说「复制时截到 15 字节」，但那是 `store_kernel_info` 的写法（`lib.rs:1629`）；v1 接收路径由 `arch_boot_resume_v1` 直接写池，`name_len` 取到 16、不做 `min(15)`（`lib.rs:684-690`），16 字节名不会被截断。② 文档 `:1058` 的术语「六类 `fill_*`」与实际 5 个 `fill_*` 函数（文档 `:1837` 自己列的也是 5 个）冲突。③ 生产打印 `boot-shim: memory map left N range(s) out of both ordinary tables`（`ebs_pair.rs:216-219`，EBS 成功后唯一说明 `Side` 去向的行）未进 §6.1 里程碑表、§6.3 也无对应行。
- 证据：`rg -n 'BOOT_MODULE_NAME_LEN' os/kernel/src/globals.rs` → `:344 =16`；`rg -n 'BOOT_MODULE_NAME_LEN - 1' os/kernel/src/lib.rs` → `:1629`；`rg -n 'name_len = wire' 同文件` → `:684`；上述行号均已在工作树复核。
- 分级：非阻断（P2/观察）。
- 最小修复：① 表行限定为「`store_kernel_info` 复制时截到 15 B；v1 解码路径按 wire 16 B 原样落」；② 改「五类 `fill_*`」；③ 在 §6.1 表补该行并指向 §6.3 的 `Side`/WB 失败行。

## SUMMARY

- 主张成立（反证失败）：REFUTE-01、02、03、05；REFUTE-04 的 I1/I2/I4 三支成立。
- 阻断：1 条（REFUTE-04 的 I3 判据句，P1）。
- 非阻断：3 项（均在 REFUTE-06：「六类 fill_*」计数、名字池截断、§6.1 缺 `Side` 诊断行）。
- 未发现跨节复述冲突：五条腿计数、29184/3584/64/47/128/256/4/296 等数字在 §2/§3/§4/§5 间一致；git 历史亦与 v9→v10 的 160→128、30624→29184 对得上。

## ZERO-FINDINGS（无发现）

本报告没有 FINDING；反证条目是 REFUTE-01–06，阻断项为 REFUTE-04 的 I3。
