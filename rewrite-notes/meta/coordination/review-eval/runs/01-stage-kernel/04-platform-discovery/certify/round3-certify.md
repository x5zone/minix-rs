# D04 R3 certify 独立回读回执

- 结论：PASS
- 核验条数：12
- CLOSED：12；STILL-BROKEN：0
- 冷读方式：逐条定位文档句 + 对照 `minix3/`、`os/` 源码 grep/sed 实读；只读，未改动除本回执外的任何文件。

## 1. §1.4（L133）Rust 失败点位改为 init_protection 先于时钟消费
- 判定：CLOSED
- 证据：文档 L133 现文「Rust 侧同一错排的失败点不是时钟初始化自己——Phase A.5 的 `init_from_kinfo` 在 `init_clock_and_interrupts` **之前**就无条件调 `init_protection` 读 `platform_desc()`，断言在那里就触发（§4.1/§6.1 的排错记录）；时钟/中断的消费者反而永远到不了。」；`grep -n "第一行" 04-platform-discovery.md` 仅命中 L815 的「上表第一行」（指 §2.7 表格行，与 clock 函数无关），旧说法「时钟函数第一行」已不存在。源码实证调用序：`os/kernel/src/lib.rs` L1755-1759 `minix_platform::init_from_kinfo(kernel_info)`（Phase A.5）→ L1784 `init_protection(kernel_info);` → L1785 `init_clock_and_interrupts();`；`init_protection`（lib.rs:2217 起）在 L2249 无 cfg 门地执行 `let topo = minix_platform::platform_desc().cpu_topology();`——失败点确在时钟消费者之前、经 init_protection 的 platform_desc() 读触发。

## 2. 同句 C 侧改「约束放大」口径并声明未取证
- 判定：CLOSED
- 证据：文档 L133 现文「C 侧约束会放大：`acpi_phys2vir` 的 `vm_running` 检查让…必须整体赶在 VM 起来之前完成（§1.3）……至于是否真有消费者运行在 `vm_running` 置位之后，本篇未取证，成立的只是约束放大本身」。C 源实锚：`grep -n "vm_running" minix3/minix/kernel/arch/i386/acpi.c` → :61、:70 两处 `if(!vm_running)` 检查，与 §1.3 L64-74 的逐字引文一致。

## 3. §1.3 表格补 DTB 单块形态限定
- 判定：CLOSED
- 证据：文档 L83 现文「解析只依赖那份物理地址指向的字节，只要字节还在（这是 DTB 单块形态；ACPI 的表链不在这条保证内，见 §4.5 的 R04-T13）」。交叉引用可解析：`grep -n "R04-T13"` → L1905（§4.5 正文缺口段，登记原文）与 L2597（§5.4 TODO 表），两处均存在。

## 4. §3.7 区分 Panics 注释文本与运行时 assert 消息
- 判定：CLOSED
- 证据：文档 L1427「同一文件里 `get` 的 Panics **注释**写的是另一句措辞（注意区分：下面引的是注释文本，不是运行时断言文本；真正的 `assert!` 消息是 "Frozen::get before freeze"……）」+ L1429-1434 text 块引 Panics 注释 `"frozen before freeze"`。源码实证两处都在：`grep -n "before freeze" os/libs/minix-types/src/types/cell.rs` → :246 `/// Panics if \`freeze\` has not run yet (\`"frozen before freeze"\`).`（注释）、:250 `"Frozen::get before freeze",`（运行时断言消息）。

## 5. §4.2 authority 过滤句改为可解析机制描述
- 判定：CLOSED
- 证据：文档 L1644 现文「`audit_source_coverage_for`（`memmap_source.rs:361`）在 UEFI 腿只以 `UefiMemoryMap` 这一类记录为**权威来源**（`authority` 参数只收它；DTB 记录被排除在「必须解释投影」的清单之外，机制见 `memmap_source.rs:354-360` 的注释）——所以它们跨了 ABI，但不承担覆盖证明的义务」。源码实证：`sed -n '354,366p' os/libs/minix-boot/src/memmap_source.rs` → :354-360 注释原文「Only the memory-map records are authoritative for the UEFI projections: a DTB `/memory` record must not stand in for a descriptor the firmware map did not report.」，:361 `pub fn audit_source_coverage_for(`；过滤体 :371-374 `authority.is_none_or(|a| r.input_object == a)`——非权威 input_object 的记录不承担覆盖义务。UEFI 腿权威实为 UefiMemoryMap：`kernel_info.rs:179-180`（"the UEFI legs pass InputObjectId::UefiMemoryMap"，直核腿 :728 传 DtbBootInput）、`uefi_helpers.rs:1060`、`ebs_pair.rs:425`。锚点落在声明的 354-366 区段内（注释段 354-360 + 函数头 361；362-366 仅参数表，声明区间内全部语义均有锚）。

## 6. §2.9 板号两步的主题句改为「失败停点不同」
- 判定：CLOSED
- 证据：文档 L1009 现文「板号在两步里落地，两步的失败停点不同：短名缺失停在第一步（`pre_init` 的 `svc #00`，源码注释 "there is no safe escape"），到不了后面的任何 panic；只有全名查表失败（板号 0）才会走到 `intr_init` 的 panic」；L1011-1013 两步展开与 §2.9 前文不再矛盾。C 源实证：`minix3/minix/kernel/arch/earm/pre_init.c` :351 `#define POORMANS_FAILURE_NOTIFICATION asm volatile("svc #00\n")`、:366 `machine.board_id = get_board_id_by_short_name(boardname);`、:369-370 `/* same thing as above there is no safe escape */ POORMANS_FAILURE_NOTIFICATION;`；`minix3/minix/kernel/main.c:133` 全名查表 `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));`；`minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:32` panic "Can not do the interrupt setup. machine (0x%08x) is unknown"。

## 7. §5.4 M1 行改为与 §5.3 同口径
- 判定：CLOSED
- 证据：文档 L2567 M1 现文「真机 10 个测试内核直接调用它（名单见 §5.3，与本行同一个计数）且全部走 ACPI 命中路径；**DTB GUID 命中路径从未在真机上命中**（……`test-smp-aps-aarch64` 走 `-kernel`+`-dtb` 直核 DTB 单源、不经此函数，见 M7）」。口径核对：§5.3 L2530 名单逐个计数 = test-smp-aps、test-smp-ipi、test-timer-irq、test-timer-irq-aarch64、test-smp-topo-aarch64、test-rt-birth、test-rt-birth-aarch64、test-user-trap、test-paging-faultloop、test-sysboot = 恰 10 个，且 `test-smp-aps-aarch64` 不在名单内（它由 M7/R04-T04 行接管，L2532 写明 `-kernel`＋`-dtb` 直核），`test-smp-topo`（x86_64）单独注明内联扫描不经此函数——M1 与 §5.3 两处计数、名单、例外完全一致。

## 8. §5.2 riscv64 topo 行改为 `new(DTB, PhysBytes(dtb_phys))` 并区分 with_optional_len
- 判定：CLOSED
- 证据：文档 L2472 现文「`PlatformDescSource::new(DTB, PhysBytes(dtb_phys))`（len=0，不带长度；与 boot-shim 生产腿的 `with_optional_len` 不同）」。源码实证：`os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-topo-riscv64/src/main.rs:110` = `let source = PlatformDescSource::new(DTB, PhysBytes(dtb_phys));`；`os/libs/minix-boot/src/platform.rs:134-140` `pub const fn new(...)` 置 `len: 0`；生产腿确实走 `with_optional_len`（文档 §4.2 L1597 x86-64、L1639 opensbi 腿）。

## 9. §4.2 `cross_validate_config_dtb` 句的锚点组改写
- 判定：CLOSED
- 证据：文档 L1644 现文「定义在 `uefi_helpers.rs:855-895`，函数体内 4 条 panic 停机路径——:866/874/880/885；`:1188` 是调用点，紧随其后的 :899/902 属下一函数 `load_dumpdtb_blob`。……（`push_dtb_sources` 定义在 `ebs_pair.rs:237`……）」。源码实证（`grep -n` 全部命中）：`os/boot-shim/src/uefi_helpers.rs` :855 `fn cross_validate_config_dtb(...)`；:866 panic（owned copy 缺失）、:874 panic（unparseable）、:880 `.expect(... "DTB memmap rejected — halting")`（expect 即停机路径）、:885 panic（conventional 越界）；:1188 `cross_validate_config_dtb(report.platform_sources, report.memmap);`；:896 `fn load_dumpdtb_blob(...)`，其内 panic 在 :899、:902——归属正确。`os/boot-shim/src/ebs_pair.rs:237` `fn push_dtb_sources(...)`。附注：函数体实际止于 :889（:890-895 是下一函数的文档注释与 `#[cfg]`），855-895 的区间写法按声明原样兑现，四个停点锚全部落在真实函数体内，无误导。

## 10. 三处引文标注（§3.1 / §4.9 / §4.8）
- 判定：CLOSED
- 证据：三处标注均在。§3.1 L1065「句序相对源注释有重排」——实证为真：`kernel_info.rs` 源注释把 `len = 0` 从句放在段落末尾（"...ELF binaries. No function pointers cross the boundary. \`len = 0\` means..."），文档引文把它移入段中（"...phys_addr)\` — \`len = 0\` means... piece — safe to pass..."）。§4.9 L2227「块内注释为中文转述式压缩，非源码逐字——源注释首句 not the truth: OpenSBI may hand control… 被压缩」——非逐字披露实证为真：源注释（`os/kernel/src/lib.rs` init_smp_state 内）为 7 行英文、含「OpenSBI may hand control to any hart (QEMU -smp 4 ...) and the entry election cell records the real one. Resolve it here, ...」全句，文档摘录压缩为 2 行且恰被压缩掉 OpenSBI 分句，与标注所述一致（观察：压缩后的摘录注释以英文呈现，「中文转述式」是压缩方式描述；实质披露「非源码逐字 + 被压缩内容」准确可查）。§4.8 L2148「注释为中文转述、非源码逐字，行首过程编号未保留；代码逐字」——实证：源注释为英文（"Check for zero before translating..."、"Route to the kernel higher-half (TTBR1) MMIO alias..."，`os/plat/src/arm64/interrupt.rs:226` 起），文档摘录注释为中文转述；代码逐字比对一致（downcast 链、三条 assert 消息原文、`mmio_translate` 调用与 `Self{}` 字段全部逐字）。

## 11. §3.2 区分「源注释原话」与「本文推论」
- 判定：CLOSED
- 证据：文档 L1154 现文「enum 分派零开销、不经 vtable——这两条是源注释的原话，「省掉每描述符一次堆分配」是本文的推论，不是源注释原文」。源码实证：`os/libs/minix-platform/src/global.rs:99-103` 注释原文「`no_std` — avoid allocator dependency at T2.5 (early boot).」与「Enum dispatch is zero-cost (no vtable indirection).」——「零开销、不经 vtable」确为源注释原话；源注释无「每描述符一次堆分配」表述，文档归为自身推论，区分成立。

## 12. §4.2 两个裸锚点补全路径
- 判定：CLOSED
- 证据：文档 L1644 现文「（`os/libs/minix-boot/src/memmap_source.rs:1152`、`os/boot-shim/src/resv_claims.rs:97`）」——两处均为全路径+行号。行内容实读：`memmap_source.rs:1152` = `for (entry_index, entry) in self.fdt.memory_reservations().enumerate() {`（collect_arrays 的 MRB 收集循环）；`resv_claims.rs:97` = `SourceKind::ReservedNoMap | SourceKind::MrbEntry => ExpectedEfiType::Reserved,`（MRB 映射 EFI Reserved）——两者均支撑该句「解 `/memory`、`/reserved-memory` 与 MRB，映射 `EfiReservedMemoryType`」的表述。

---

结论：回读验证通过，12 条逐条落证。
