# 04-platform-discovery 深度 Review（正确性 + 卓越性）

> 审查对象：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`（2,587 行，sha256 `2e0f39714ec83aebd0ddd79f0a644993d5fa89e97871cc1db9647d5de5507349`）+ `tools/doc-code-map.sh` 关联的 49 个 Rust 文件。
> 审查立场：**只审不修**（用户裁决）。受审文档与 `os/` 代码零改动；本轮所有产物是报告、证据与重评快照。
> 状态：**修复前 NOT CONVERGED**（6 个 P0）；**修复后 CONVERGED_WITH_WARNING_BACKLOG**，见 §15。
> 真值优先序：Minix3 C 源码行为 > 设计契约 > Rust 实现 > 文档 > AI 分析。
> 审查模式：`full-review`（文档 + 关联代码，scope=doc，深度）；规则源与技能调用见 `.review/zcode/01-stage-kernel/04-platform-discovery/scan.md` 的 Skill Invocation Log。

> **关于计划标题**：本轮 `/goal` 的标题写的是「03-kmain-cstart 深度 Review 计划」，但 03 已于提交 `25d8f23ac` 完成并提交（报告 `03-review.md`、轨迹 `03-review-trajectory.md`）。本轮执行的是此前已批准的 **04-platform-discovery** 计划；03 不重复审查。

---

## 1. 结论先行

**04 篇的骨架、C 侧事实与大部分引用是可信的；问题集中在"发现的最后一公里"：x86 上，LAPIC/IOAPIC 的描述符地址被解析出来却没有进入 MMIO 消费者；aarch64 的 `DEVICE_TREE_GUID` 不是 UEFI 规范值，DTB 扫描在合规固件上永远不命中；`GET_MACHINE` 的 `apic_enabled`/`acpi_rsdp` 被宣布"淘汰/恒 0"，而仓库里的用户态 ACPI 驱动仍依赖它们；ACPI 解析器丢掉了 C 的校验和、根表签名与容量闸；CPU 拓扑的 `bsp_id` 语义与 C 不同；§4.9 的两段生产代码引用已经过期。**

具体：

| 级别 | 数量 | 代表 |
|---|---|---|
| P0 | 6 | `P0-fact-lapicConsumed`、`P0-design-machineFields`、`P0-code-acpiValidation`、`P0-design-bspId`、`P0-fact-staleSmpQuotes`、`P0-code-dtbGuid` |
| P1 | 18 | `P1-code-maskAllPins`、`P1-design-singleIoapic`、`P1-design-gicZeroCpu`、`P1-test-bspTautology`、`P1-fact-threeAsserts`、`P1-fact-translationTable`、`P1-fact-runAllLines` 等 |
| P2 | 19 | 分布表、注释算术、悬空 §4.2.2、`TODO-02-3` 伪造引文、ACLINT mtimecmp、aarch64 测试编译、`16-smp.md` 缺失等 |

**Layer 1（正确性）：FAIL。** 6 个 P0 均可复现、均有代码/C 真值/规范证据；按 `review.md` 的收敛规则，P0 未清零不得 CONVERGED。
**Layer 2（卓越性）：PARTIAL。** 结构骨架成立（Gate D-6 PASS 带 P1/P2），但"设计章不完整、LAPIC 主线停在解析、测试章两套视角"三处需要专门重写；不是文风修补能收口的。

---

## 2. 范围与方法

### 2.1 范围

- 文档全文 7 章、34 个 `##`/`###` 标题、2,587 行。
- 49 个关联 Rust 文件（`doc-code-map.sh --check`，存在性 49/49）：
  - `minix-boot`：`platform.rs`、`kernel_info.rs`、`memmap_source.rs`、`handoff_v1_write.rs`
  - `minix-platform`：`lib.rs`、`desc.rs`、`kind.rs`、`global.rs`、`acpi.rs`、`device_tree.rs`、`qemu_virt.rs`、`arch/{mod,x86_64,aarch64,riscv64}.rs`
  - `boot-shim`：`uefi_helpers.rs`、`opensbi_helpers.rs`
  - `kernel-image`：`bootface.rs`、`bootface_a64.rs`
  - `kernel`：`lib.rs`、`misc.rs`、`smp.rs`、`trap_dispatch.rs`
  - `minix-types`：`cell.rs`、`address.rs`
  - `plat`：`interrupt.rs`、`early_console.rs`、`{x86_64,arm64,riscv64}/interrupt.rs`
  - `arch`：`{arch,x86_64,arm64,riscv64}/clock.rs`、`arch_init.rs`、`x86_64/arch_init.rs`
  - 三个 `test-smp-topo` 变体、`sched/sef.rs`
- C 真值：`acpi.c`、`acpi.h`、`apic.c`、`apic.h`、`arch_system.c`、`arch_smp.c`、`memory.c`、`i8259.c`、`main.c`、`clock.c`、`glo.h`、`usermapped_data.c`、`do_getinfo.c`、`earm/*`、`board.h`、`type.h`。
- 跨文档：01/02/03/05/06 篇的相关节；`16-smp.md`（不存在，见 P2-doc-16smp）。

### 2.2 方法

1. **R0 预检**：`review-init`、4 条 `ls .design/04-*.v*.md`、`design-coverage-check`、`doc-code-map`、四道文档门、`coverage-extract`。
2. **R1 正确性**：Claims-Evidence 抽取；C 引用逐块 `diff`/目对；Rust 引用逐块与源码对；数据结构覆盖；调用关系；测试对账；代码维度 §1/2/4/8/12/13/14/15。
3. **R2 卓越性**：`structure.md` 12 节；文档卓越 §4.1–§4.5；M1–M9；代码卓越 §16–§21；文风与承载门。
4. **对抗 agent**：A1 代码事实核查（材料隔离，30 条：1 P0/10 P1/19 P2）、A2 闭卷学生（只读文档）、A3 冷读结构编辑（只读文档）；三份报告均已返回，发现已去重并入。
5. **R3 验证**：`verify-check.py` 机械门 + 人工反向验证；QEMU 三架构 `test-smp-topo`；宿主测试。
6. **R4/R5**：报告、轨迹、复盘。

### 2.3 本轮没做的事

- 没有修改受审文档、`os/` 代码、`prompt/`、`tools/`。
- 没有跑完整 `run_all.sh`（只跑 `test-smp-topo` 三架构；理由：04 的出口契约就是这三个变体，完整套件属回归轮）。
- 没有把 `.design`/`.review` 的中间产物写进正式正文。
- 没有对 03 篇重复审查（见开头说明）。

---

## 3. 执行摘要

### 3.1 六个 P0

#### P0-fact-lapicConsumed — "Rust 补上了 LAPIC 基址"在消费端不成立

文档 §2.5 的收束句是：

> Rust 侧把这一格补上了：解析器从 MADT 表头读出 `Local APIC Address` 存进描述符（§4.4）。

§4.8 的地址表进一步说 x86-64 "描述符里的地址 = 物理地址；实例字段里的地址 = 同值；不需要翻译"。

代码核对的结论相反：

```rust
// os/plat/src/x86_64/interrupt.rs:235-243 —— 逐字引用
unsafe fn lapic_read(base: usize, offset: usize) -> u32 { unsafe {
    let _ = base; // window base governs; the caller's PA base is legacy
    read_volatile((LAPIC_VA.load(Ordering::Relaxed) + offset) as *const u32)
}}
unsafe fn lapic_write(base: usize, offset: usize, value: u32) { unsafe {
    let _ = base; // window base governs; the caller's PA base is legacy
    write_volatile((LAPIC_VA.load(Ordering::Relaxed) + offset) as *mut u32, value)
}}
```

`ioapic_read_indirect`/`ioapic_write_indirect` 同样 `let _ = base;`，改用 `IOAPIC_VA`。两个全局 VA 在 `init_clock_and_interrupts` 里由设备窗口设置：

```rust
// os/kernel/src/lib.rs:2190-2215 —— 节选
assert!(lapic_region.phys_base == 0xFEE0_0000,
        "device-window LAPIC page drifted from 0xFEE00000");
let lapic_va = device_va_for(regions, 1, 0xFEE0_0000)...;
minix_plat::x86_64_interrupt_set_lapic_va(lapic_va);
...
let ioapic_va = device_va_for(regions, 0, regions[0].phys_base)...;
minix_plat::x86_64_interrupt_set_ioapic_va(ioapic_va);
```

`device_window_regions()` 的 x86 表是常量：

```rust
// os/arch/src/arch/device_window.rs:141-146 —— 逐字引用
pub const fn device_window_regions() -> &'static [DeviceWindowRegion] {
    &[
        DeviceWindowRegion { phys_base: 0x0FEC0_0000, size: 0x1000, slot: 0, bootstrap: true },
        DeviceWindowRegion { phys_base: 0x0FEE0_0000, size: 0x1000, slot: 1, bootstrap: true },
    ]
}
```

因此：

- `ApicDesc.lapic_base` 进入 `X86_64InterruptController::new` 后，`init_lapic` 会用 `IA32_APIC_BASE` MSR 覆盖它（`interrupt.rs:299-304`），但随后的 `lapic_write` 仍写固定的 `LAPIC_VA`——覆盖是"装饰性"的。
- `ApicDesc.ioapic_base` 从未与设备窗口比对；`ioapic_*` 只用 `IOAPIC_VA`。
- 文档用来立论的场景（固件把 LAPIC 窗口重定位）在 x86 上没有被"支持"，只是被"拒绝"：BSP 的 `init_lapic` 静默写 `0xFEE0_0000`；直到 `boot_init_timer` 的 `lapic_va_base()`（`os/arch/src/x86_64/clock.rs:47-77`）读 MSR 并 `assert!(base == 0xFEE0_0000, "...")` 才停机。AP 路径的 `os/arch/src/x86_64/smp.rs:95-115` 同样断言拒绝。
- 这不是"代码错、文档对"或"文档错、代码对"的二选一：代码选择了"固定窗口 + 拒绝重定位"的设计，文档却宣称"从 MADT 读基址并补上"。两者必须对齐——要么让消费者真的用 `ApicDesc` 的地址并动态建设备窗口，要么把文档改成"Rust 检测并拒绝重定位，不静默使用错地址"。

**定级理由**：这是 04 篇的叙事主轴（A3 独立判"全篇最好的一条线"），也是"平台发现"这个标题的出口契约；错在核心论点上，P0。

#### P0-design-machineFields — `GET_MACHINE` 的两个字段被"淘汰"，但消费者还在

文档 §4.10：

> `apic_enabled` …… **淘汰**：Rust 没有"用不用 APIC"这条分支（APIC/GIC/PLIC 是唯一路径），字段恒 0。
> `acpi_rsdp` …… **改型**：用户态这一份不再发布（`MachineStruct.acpi_rsdp` 恒 0）……

内核实现确实是"只填两个字段"：

```rust
// os/kernel/src/misc.rs:1086-1090 —— 逐字引用
let machine = MachineStruct {
    processors_count: smp.as_ref().map(|s| s.ncpus()).unwrap_or(1),
    bsp_id: smp.as_ref().map(|s| s.bsp_cpu_id().raw()).unwrap_or(0),
    ..Default::default()
};
```

但 Rust 树里的用户态 ACPI 驱动仍按 C 的语义读这两个字段：

```rust
// os/drivers/power/acpi/src/main.rs:40-50 —— 逐字引用
let apic_enabled = i32::from_le_bytes(buffer[12..16].try_into().unwrap()) != 0;
let acpi_rsdp = u64::from_le_bytes(buffer[16..24].try_into().unwrap());
Some(minix_driver_acpi::service::Machine { apic_enabled, acpi_rsdp })
```

```rust
// os/drivers/power/acpi/src/service.rs:91-93,134-136 —— 节选
if machine.acpi_rsdp == 0 {
    return;
}
...
fn scan_devices(&mut self, machine: Machine) {
    if !machine.apic_enabled {
        return;
    }
```

结果：`acpi_rsdp` 恒 0 → `init()` 第一道门直接返回，ACPI 驱动永不初始化；`apic_enabled` 恒 0 → 即使 `acpi_rsdp` 修好，PCI 桥扫描也不会跑。C 侧的同类消费者在 `minix3/minix/drivers/bus/pci/pci.c:917,1895,2104` 与 `drivers/power/acpi/acpi.c:82`，它们把 0 解释为"PIC 模式"。

此外，§4.1 自己写着 8259 路径存在（`pic_init` 在 `boot_init_timer` 末尾、每 tick 走 `pic_eoi`），§4.10 却说"APIC 唯一路径"；同一篇内自相矛盾。`pic_init`/`pic_eoi` 的活路径在 `os/kernel/src/lib.rs:3497-3518` 与 `os/kernel/src/trap_dispatch.rs:443-446`。

**定级理由**：`GET_MACHINE` 是内核与用户态的 ABI；把两个字段恒 0 而不改消费者，是外部行为回归。P0-design-deviation。

#### P0-code-acpiValidation — Rust 解析器丢掉了 C 的两道校验

C 的 RSDP 扫描有两道闸，`acpi_read_sdt_at` 还有签名/容量/校验和三道闸：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_rsdp_test —— 逐字引用 */
if (!platform_tbl_checksum_ok(buff, 20))
    return 0;
if (strncmp(rsdp->signature, "RSD PTR ", 8))
    return 0;
```

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_read_sdt_at —— 节选 */
if (acpi_check_signature(tb->signature, name)) { ... return -1; }
if (size < tb->length) { ... return -1; }
...
if (acpi_check_csum(tb, tb->length)) { ... return -1; }
```

Rust `AcpiDesc::parse` 只做签名比对：

```rust
// os/libs/minix-platform/src/acpi.rs:170-178 —— 节选
let sig_bytes = unsafe { core::slice::from_raw_parts(rsdp_phys as *const u8, 8) };
if sig_bytes != &RSDP_SIGNATURE[..] {
    return Err(AcpiParseError::BadRsdpSignature);
}
```

`find_madt` 从根表偏移 4 读 `length`，直接 `entry_count = (sdt_len - 36) / ptr_size`，没有 `MAX_RSDT` 式的容量上限，也没有 XSDT/RSDT 签名与校验和：

```rust
// os/libs/minix-platform/src/acpi.rs:329-350 —— 节选
let sdt_len = u32_le_from_slice(length_bytes) as usize;
let header_size = core::mem::size_of::<SdtHeader>();
if sdt_len < header_size { return None; }
let entry_count = (sdt_len - header_size) / ptr_size;
for i in 0..entry_count {
    ...
}
```

后果：畸形/损坏的 RSDP 或根表可以让 `entry_count` 变成数亿，循环按物理地址逐项读签名，表现为长时间扫描或越界读；C 在 176 字节处就拒绝。`parse` 是 `unsafe fn`，但它的调用方（`init_from_kinfo`）只保证"指针来自固件"，并不保证表内容良构——C 的校验正是为此存在。

文档 §1.6 把这条对照写成"C 的缺失——XSDT、扩展校验和、MADT 表头 LAPIC 字段、RSDP 的 20 字节校验口径"：前两项确实是 C 的缺失，后两项不是；20 字节校验是 **C 有、Rust 没有**。方向写反了。

**定级理由**：安全/健壮性代码缺陷，且文档的对照叙述与事实相反。P0-code-bug。

#### P0-design-bspId — `bsp_id` 是"第一条 enabled 记录"，不是实际 BSP

C 的 `discover_cpus` 先填 `apicid2cpuid[]`，`smp_init` 再用**自己的** APIC ID 取逻辑号：

```c
/* minix3/minix/kernel/arch/i386/arch_smp.c:smp_init —— 节选 */
bsp_cpu_id = apicid2cpuid[apicid()];
```

Rust 的解析器在 `nr_cpus == 0` 时把第一条 enabled 记录写成 `bsp_id`：

```rust
// os/libs/minix-platform/src/acpi.rs:505-520 —— 节选
if enabled && (nr_cpus as usize) < MAX_CPUS {
    let hw_id = apic_id as u64;
    if nr_cpus == 0 {
        bsp_id = apic_id;
    }
    ...
}
```

`device_tree.rs:471-473` 同样取第一个 CPU 节点。`CpuTopology.bsp_id` 的字段注释写的是 "Hardware ID of the BSP (boot CPU)"（`platform.rs:176`），但实现是"第一条记录"。ACPI 规范不保证 MADT 的记录顺序把 BSP 放在最前；一旦固件把 AP 排在前，`program_entry_base`（`lib.rs:2009-2014`）会把 GS 槽位编程到那颗 AP 的逻辑号，`SmpState` 的 BSP/READY 与 `GET_MACHINE.bsp_id` 也跟着错。

`init_smp_state` 在 riscv64/aarch64 用 `elected_bsp_hw_id`（election cell）纠偏；x86 的 `elected_bsp_hw_id` 直接返回 `topo.bsp_id`（`lib.rs:1722-1740`），没有读 `IA32_APIC_BASE`/`current_cpu_id()`。所以 x86 的纠偏不存在。

测试 `test-smp-topo` 的"BSP 在发现集合内"断言因此是结构恒真：`bsp_id` 由解析器从 `cpus[]` 里取，除非解析器被改成"凭空造 ID"，否则断言不可能失败。文档 §5.2 的表格说它排除"MADT 遍历跳过 BSP 记录"——恰恰排除不了。

**定级理由**：与 C 的语义偏差，影响 per-CPU 身份、GS、用户态 BSP 读数；QEMU 的 BSP 恰好第一条，测试不暴露。P0-design-deviation。

#### P0-fact-staleSmpQuotes — §4.9 的两段生产代码引用已过期

文档 §4.9 引用 `init_smp_state` 为：

```rust
// 文档 L2126-2135
*SMP_STATE.get() = Some(smp::SmpState::with_ncpus(
    topo.nr_cpus.max(1),
    crate::proc::CpuId::new_unchecked(topo.bsp_id),
));
```

当前生产代码（`os/kernel/src/lib.rs:1749-1768`）是：

```rust
let bsp_hw = elected_bsp_hw_id(&topo);
let bsp_logical = (0..topo.nr_cpus as usize)
    .find(|&i| topo.cpus[i].hw_id == bsp_hw)
    .unwrap_or(topo.bsp_id as usize) as u32;
let mut state = smp::SmpState::with_ncpus(
    topo.nr_cpus.max(1), crate::proc::CpuId::new_unchecked(bsp_logical));
state.seed_bsp_masks(bsp_logical);
```

文档还写"函数余下 9 行是 BSP 的 ptproc 槽位安装"；当前函数约 76 行，含 boot set 重排与 `ptproc` 安装。

§4.9 的 `init_protection` 代码块同样过期：文档写 `#[cfg(target_arch = "x86_64")] { ... program_gs(...) }`，当前代码是：

```rust
// os/kernel/src/lib.rs:2009-2014
let topo = minix_platform::platform_desc().cpu_topology();
let bsp_logical = (0..topo.nr_cpus as usize)
    .find(|&i| topo.cpus[i].hw_id == topo.bsp_id as u64)
    .unwrap_or(0) as u32;
with_protection(|prot| prot.program_entry_base(bsp_logical, kernel_info.kern_stack_top));
```

这是 03 篇修复 `program_gs`/`program_entry_base` 时引入的变更；04 篇没有同步。按 Doc Code Sync 七项，函数名、签名、调用关系三项均不一致，P0-fact。

#### P0-code-dtbGuid — DTB 扫描用的不是 UEFI 规范 GUID

A1 的独立 P0，主会话复核后采纳。boot-shim 的常量是：

```rust
// os/boot-shim/src/uefi_helpers.rs:40-42 —— 逐字引用
/// Defined in the UEFI Specification as `EFI_DEVICE_TREE_GUID`.
const DEVICE_TREE_GUID: Guid = uefi::guid!("b1b621d2-f19c-41c5-8310-daa6f018a8d3");
```

edk2 的规范值是：

```c
/* MdePkg/Include/Guid/Fdt.h —— 逐字引用 */
#define FDT_TABLE_GUID \
  { 0xb1b621d5, 0xf19c, 0x41a5, { 0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0 } }
```

即 `b1b621d5-f19c-41a5-830b-d9152c69aae0`。二者第一段 `d2≠d5`、第三段 `41c5≠41a5`、末 8 字节也不同。测试载体注释里的"UEFI 规范 `EFI_DTB_TABLE_GUID` `b1b621d5-19f0-4008-b30d-b83e8133f39b`"又是第三个值，也不是规范值。

后果：`find_platform_sources` 的 DTB 扫描在任何符合规范的固件上都不可能命中；文档 §1.2/§5.2/§5.4 M1 把"从未命中"归因于"AAVMF 不提供 DTB 表项"，这是未被证据支持的因果结论——即使 AAVMF 安装了规范 FDT 表，代码也扫不到。§5.4 M1 的"DTB GUID 命中路径从未在真机上命中"因此是自证：常量错 → 扫不到 → 断言"固件没有"。

**定级理由**：这是可复现的代码缺陷（规范固件上 DTB 发现失效），且直接推翻文档的实测因果。P0-code-bug。

### 3.2 P1 摘要（完整表见 §9）

| ID | 一句话 |
|---|---|
| P1-fact-lapicAssignments | §2.5 "每一处赋值都是常量"被 `memory.c:954` 的 VA 回写证伪（物理结论仍成立） |
| P1-fact-noApicFn | `no_apic` 环境变量解析在 `cstart`，不在 `kmain` |
| P1-fact-panicSite | 挪后 `init_from_kinfo` 的首个 panic 点是 `init_protection`，不是 `init_clock_and_interrupts`；两种错排同栈 |
| P1-fact-runAllLines | `run_all.sh` 六行行号全部漂移 |
| P1-fact-uefiTestCount | `uefi_helpers.rs` 有 4 条测试，不是 1 条 |
| P1-test-bspTautology | "BSP 在集合内"断言结构恒真 |
| P1-fact-kernelInfoQuote | `KernelInfo.platform_sources` 节选已过期 |
| P1-quote-s2b | §3.2/§4.3 的"逐字引用"被加 `S-2b` |
| P1-fact-parseTimer | aarch64 `parse_timer` 不看 DTB，谈不上"确认存在" |
| P1-design-singleIoapic | C 支持多 IOAPIC；Rust 描述符只有一个 `ioapic_base`，文档未声明限制 |
| P1-code-maskAllPins | `mask_all` 写到 24 针 IOAPIC 的越界 RTE；`ioapic_version()` 死代码 |
| P1-design-gicZeroCpu | `check_gic_madt` 放行 `nr_cpus==0`，随后默认 1 CPU 带 0 GICR 进驱动 |
| P1-doc-apicContradiction | §4.1 有 8259 活路径 vs §4.10 "APIC 唯一路径" |
| P1-doc-styleProcessNumbers | `S-2b`/`S-6.1 (D-40)`/`I-22`/`NK4-C 1.114` 进正文/引用 |
| P1-test-hostCount | 22 是属性计数；x86 宿主实际运行 14 条 |
| P1-fact-threeAsserts | §4.8 说"两个零基址断言"，实际三个，且节选漏了 `gicr_stride` 字段 |
| P1-fact-translationTable | §4.8 说 x86/riscv "同值/否"，实际两者都走设备窗口 VA |
| P1-fact-todoQuote | §3.6 "按原文引用"的模块注释含源码没有的 `TODO-02-3` |

### 3.3 P2 摘要

`&[]` 分布表把 `vm_handoff.rs` 记成 1（实为 0，`handoff_v1_write.rs` 才是 1）；`nr_irqs: 64 // (16 ISA + 8 PCI)` 算术错；代码注释引用不存在的 §4.2.2；测试注释的 `EFI_DTB_TABLE_GUID` 值错、`mpdir` 笔误；`pins` 的消费者写成 `ioapic_enable_all`；"零 vtable"与 `&dyn PlatformDesc` 矛盾；`16-smp.md` 不存在；ACLINT 路径的 per-CPU `mtimecmp` 为 None；`parse_plic` 的 context 断言是常量自证；`acpi.rs` 两条测试在 aarch64 测试构建下编译失败；riscv `fn new` 节选漏 `plic_va_base`。

---

## 4. Gate 证据

| Gate | 结果 | 证据位置 |
|---|---|---|
| 0 制品完整性 | ✅ | `scan.md` 9 锚段；49/49 代码清单；Artifact Inventory |
| A 覆盖率穷举 | ⚠️ PARTIAL | 84/213=39.4%；`--check-filled` exit=0；人工射程分档见 §6 |
| B Top5 行为契约 | ✅ | `scan.md` `gate-evidence-B`；5 函数 × 8 字段 |
| C 细节精确性 | ⚠️ PARTIAL | 5 条元规则；3 条升级 P0 |
| D P0 必检 5 项 | ✅（带 P1） | `gate-evidence-D`；`ioapic_version` 等死字段 |
| D-6 structure 12 节 | ✅（带 P1/P2） | `structure.md`；§8/§10/§12 PARTIAL |
| E §5 测试验证 | ✅ | 测试名全量命中；宿主 14+65；QEMU 三架构 PASS |
| G 独立验证 | ✅ | `VERIFY-CHECK.md` 机械 PASS；人工 5/5=100%（同 agent 标注） |
| H design 门控 | ✅（以 v2/v4） | v1 过期；v2/v4 重评 |

### 4.1 Gate B：Top5 行为契约（8 字段）

| 函数 | 输入 | 输出 | 副作用 | 错误 | 时序 | 生命周期 | 权限 | 并发 |
|---|---|---|---|---|---|---|---|---|
| `init_from_kinfo` | `&KernelInfo` | 冻结的 `PlatformDescEnum` | 写 `static PLATFORM` | 开发构建兜底 / 发布 panic | Phase A.5，单线程 | 一次冻结 | `unsafe`，调用方保证来源有效 | 冻结后只读 |
| `parse_by_kind` | `PlatformDescSource` | `PlatformDescEnum` | 无 | `UnknownKind`/`DtbParse`/`AcpiParse` | 启动期 | 返回值进 PLATFORM | `unsafe`，物理地址有效 | 单线程 |
| `AcpiDesc::parse` | `rsdp_phys` | `AcpiDesc` | 读物理内存 | 签名/无根表/无 MADT/太短/GIC 三错误 | 启动期 | 值自包含 | `unsafe` | 单线程 |
| `DeviceTreeDesc::parse` | `dtb_phys` | `DeviceTreeDesc` | 读 DTB | `BadFdtPointer`/`UnsupportedArch`/节点缺失 | 启动期 | eager 提取，与 DTB 解耦 | `unsafe` | 单线程 |
| `InterruptRouter::new` | `&dyn InterruptControllerDesc` | 控制器实例 | 无（构造） | 类型不匹配 `expect`；aarch64 零基址 `assert` | `init_clock_and_interrupts` | 长期（`IRQ_MANAGER`） | 安全函数 | 启动期单线程 |

结论：契约字段齐全；但 x86 的地址字段没有进入 MMIO 消费者（`P0-fact-lapicConsumed`），"输出"字段在真实路径上是死的。

### 4.2 Gate D：P0 必检 5 项

1. **测试存在性**：§5.1 的 5 个 `acpi.rs`、4 个 `device_tree.rs`、4/4/3 个 arch、1+1 global/kind，全部 `grep -n 'fn test_'` 命中。
2. **trait 实现**：`PlatformDesc` 4 个 impl；`InterruptControllerDesc`/`TimerDesc`/`ConsoleDesc` 各 3；`ArchInit`/`ClockArch`/`InterruptRouter` 各 3。
3. **函数位置**：`parse_by_kind`、`init_from_kinfo`、`AcpiDesc::parse`、`DeviceTreeDesc::parse`、`init_smp_state`、`init_protection`、`getinfo_machine` 全部命中；§4.9 两处引用过期单列 P0。
4. **stub/死代码**：`ioapic_version()` 无调用；`PitDesc.lapic_base`、`X86_64ClockArch.lapic_base` 存而不用；`ApicDesc.lapic_base`/`ioapic_base` 存而不用。
5. **签名**：`PlatformDesc`、三个子 trait、`ClockArch`、`ArchInit`、`Frozen`、`CpuTopology` 与文档一致。

---

## 5. Step 0 预检与基线

- 文档 sha256 `2e0f3971…`，2,587 行；基线提交 `d41d76ce…`；工作树仅 `AI-chats/daily.todo.md`。
- 快照：`04-outline.v1/v3`、`04-outline-review.v1/v3`、`04-design.v1`；无 `design-final`。
- `design-coverage-check` PASS；`doc-code-map` 49/49；`anchor-resolve` 69/69、zero-def 0；`doc-ref-check` exit 0；`doc-style-lint` error=0；`doc-code-density` 4 个可判章全过。
- 判定：快照齐全，但 `04-design.v1` 与现码明显脱节（`AssumeSyncCell`、`DtbDesc`/`IoApicDesc`、旧行号），本轮重评 v2/v4。

---

## 6. 覆盖率（Gate A）

```text
$ python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel \
    --rust-dir os --c-dir minix3/minix/kernel --doc-file 04-platform-discovery.md \
    --semantic-map tools/coverage-extract/kernel-semantic-map.json \
    --output .review/zcode/01-stage-kernel/04-platform-discovery/SYMBOLS.md
Coverage Summary: 84/213 (39.4%) semantic; 17/213 (8.0%) Rust; 有文档无Rust 67
```

人工分档（`MANUAL_FALLBACK`）：

| 档 | 判定 |
|---|---|
| 本篇核心（`acpi.c` 全函数、`acpi.h` MADT 结构、`acpi_get_ioapics`、`discover_cpus`、`machine` 六字段、earm 板级四函数） | 全部有节级锚点 + C 块；约 60 条 |
| 跨篇背景（`pg_identity`、`cut_memmap`、`tss_init`、`prot_init`、`kmain`） | 01/02/03 已覆盖，本篇只引用（正确） |
| 提取器噪声（`if`、`panic`、`TI`、`cpu`、`proc`、`irq`、`omap_timer`） | 非语义符号，已在 SYMBOLS 标注 |

结论：39.4% 是语义域分母过宽（它把"任意阶段文档/Rust 代码提到一次"的符号都算进来），不是本篇漏写率；Gate A 判 PARTIAL，不判 FAIL。SYMBOLS 的 84 行 AI 语义列已全部填充，`--check-filled` exit=0。

---

## 7. 正确性维度（R1）

### 7.1 Claims-Evidence

全文抽取 92 条事实断言（行为 41 / 数值 22 / 结构 18 / 调用关系 8 / 设计推断 3）。强证据 70、中证据 15、弱证据 7；弱且不可复现的 5 条升级为 P0/P1。代表性断言：

| 断言 | 强度 | 复现 |
|---|---|---|
| C 的 MADT 表头 `local_apic_address` 无读取点 | 强 | `rg -n 'local_apic' minix3/.../i386/` 仅 `acpi.h:99` |
| C 的 `acpi_madt_get_typed_item` 零长记录死循环 | 强 | 源码 `t += length` 无零检查 |
| `sizeof(struct acpi_rsdt) = 176` | 强 | 36 + 35×4 手算 + `sizeof` |
| `sdt_count` 的 `-1` 提升为 32 位无符号数 | 中 | i386 `size_t` 32 位；`(-1-36)/4=1073741814` |
| `run_all.sh` 六行行号 | 弱 | 实跑全部漂移（P1） |
| `uefi_helpers.rs` 只有 1 条测试 | 弱 | 实为 4 条（P1） |
| Rust 用 MADT 的 LAPIC 基址 | 弱 | 消费端不用（P0） |
| C 活路径 `lapic_addr` 都是常量 | 弱 | `memory.c:954` 反例（P1） |

### 7.2 C 引用逐块

所有"逐字引用"块与 C 源逐行对过（清单见 `scan.md` §Step 2）：`acpi_phys2vir`、`get_acpi_rsdp`、`acpi_rsdp_test`、`struct acpi_rsdp`、`acpi_init`、`acpi_madt_get_typed_item`、`acpi_get_lapic_next`、`discover_cpus`、`struct acpi_madt_hdr`、`LOCAL_APIC_DEF_ADDR`、`lapic_enable_in_msr`、`acpi_get_ioapics`、`struct machine`、`usermapped_data.c`、`cpu_identify`、`bsp_init`、`board.h` 位域、`omap_intr.c:intr_init` 均一致。节选块的省略位置与题注相符。

两处与 C 真值相关的错误不是引用错，而是结论错：
- §2.5 "每一处赋值都是常量"（P1-fact-lapicAssignments）。
- §2.7 "kmain 解析 no_apic"（P1-fact-noApicFn）。

### 7.3 Rust 引用与 Doc Code Sync 七项

| 项 | 结果 |
|---|---|
| 函数名 | `init_smp_state`/`init_protection` 引用过期（P0-fact-staleSmpQuotes） |
| 签名 | `PlatformDesc`/子 trait/`ClockArch`/`ArchInit` 一致 |
| 字段 | `MachineStruct` 一致；`KernelInfo` 注释过期（P1） |
| 常量 | `MAX_CPUS=64`、`NR_IRQ_VECTORS=64`、三份 `QemuVirtDesc` 值一致 |
| 路径 | `qemu_virt.rs`/`device_tree.rs` 的 §4.2.2 悬空（P2） |
| 测试名 | §5 列出的测试 fn 全部命中 |
| C 源引用 | 全部命中；`16-smp.md` 不存在（P2） |

### 7.4 数据结构覆盖

- C：`struct machine` 六字段（`type.h:122-131`）、`acpi_rsdp`（`acpi.h:8-15`）、`acpi_sdt_header`（36 字节）、`acpi_madt_hdr`、`acpi_madt_item_hdr`、`acpi_madt_lapic`、`acpi_madt_ioapic`、`struct io_apic`。
- Rust：`MachineStruct`（`misc.rs:301-314`）、`SdtHeader`/`MadtHeader`/`MadtEntryHeader`（`acpi.rs:96-123`）、`PlatformDescEnum`（`global.rs:109-118`）、`CpuTopology`/`CpuInfo`（`platform.rs:169-220`）、`ApicDesc`/`Gicv3Desc`/`PlicDesc`/`PitDesc`/`ClintDesc`/`ConsoleDesc`、`Frozen`。
- 覆盖完整；不足是 `CpuTopology.bsp_id` 的语义（P0-design-bspId）与 `ApicDesc` 单 IOAPIC（P1-design-singleIoapic）。

### 7.5 纵向链路

- §2.10 → §3 的三问/八决策对应成立。
- §4.1 的"首个 panic 点"错误（P1-fact-panicSite）。
- §4.9 的实现引用断裂（P0-fact-staleSmpQuotes）。
- §5.2 的 `run_all.sh` 行号断裂（P1-fact-runAllLines）。

### 7.6 跨文档

01/02/03/05/06 的引用全部可解析；`16-smp.md` 不存在（§7 已注"尚未重生成"，但 §1.5/§3.2 未标注）。`doc-ref-check` 只保证内部 §N.M 可解析，不保证跨文档同号引用语义正确；本篇未发现同号假通过案例。

### 7.7 测试对账（Gate E）

- 属性计数 22；x86_64 宿主实际运行 14（8 条 aarch64/riscv64 专用测试被 cfg 排除）。`cargo test -p minix-platform` = 14 passed；`cargo test -p minix-boot` = 65 passed。
- §5.1 的 5 个 ACPI 测试；§5.2 的三架构 `test-smp-topo`；§5.3 的三处缺口；§5.4 的 M1–M11 与 R04-T01..T08 全部能对应到源码/脚本。
- QEMU 三架构：`test-smp-topo` x86_64/aarch64/riscv64 全部 `### TEST_RESULT: PASS`，`nr_cpus = 0x4`、`bsp = 0x0`（日志见附录）。
- 测试问题：`bsp_id` 断言恒真（P1-test-bspTautology）；GUID 常量非规范值（P0-code-dtbGuid）；宿主运行条数未单列（P1-test-hostCount）。

### 7.8 代码维度

| 维度 | 结论 |
|---|---|
| §1 Rewrite | 发现语义保持；x86 消费端偏离（P0） |
| §2 硬件抽象/trait | trait 边界清晰；`as_any` 必需方法的理由正确 |
| §4 SMP/BKL | `Frozen` 协议正确；`bsp_id` 语义偏差（P0） |
| §8 命名 | 品牌名限制在 `arch/*.rs`；`mpdir` 笔误（P2） |
| §12 no_std | `minix-platform` `#![no_std]`；`MAX_CPUS` 定长数组 |
| §13 设计-代码一致 | §4.9 stale（P0）；§4.2.2 悬空（P2） |
| §14 C-Rust 叶函数 | `acpi_phys2vir`/`acpi_check_*` 无同名 Rust；校验缺失（P0） |
| §15 精度 | 见 §7.1/§7.2；行号/计数问题单列 |

---

## 8. 卓越性维度（R2）

### 8.1 structure 12 节

`structure.md` 全量填写。判定：

- 主题思想 ✅（§1 提前发放结论）
- 目标读者 ✅（`PlatformSource`/`ArchMiscDesc` 晚定义）
- 叙事主语 ✅（Ch1 首句书目化）
- 驱动方向 ✅（§4.10 偏离启动序）
- 大纲 ✅（设计章不完整，七项决策藏在 §4）
- 核心概念 ✅（两处缺定义）
- 跨架构统一抽象 ✅（x86 消费端未统一；PLIC context 两套约定）
- 双向闭环 ⚠️（六段未并排；LAPIC 消费环缺失）
- 起承转合 ✅（合部分裂为两套覆盖视角）
- 元注释 ⚠️（口径说明密度偏高）
- 裸概念复述 ✅（A2 11/12 可答）
- 纵向链路 ⚠️（§4.1 panic 点、§4.9 引用、§5.2 行号）

Gate D-6：PASS（带 P1/P2）。

### 8.2 文档卓越

- **教学资产**：C 章每节有真实代码块 + 角色框定 + 逐条件讲解；§3 每项有方案/否决/代价；§5 有夹具口径与反转表；§6 有症状→命令。承载门 4/4 通过。
- **反事实推理**：全篇约 35 处"把 X 会坏在哪 / 去掉 X 会坏在哪"，是本篇的主要教学装置；多数落到具体地址/位/线/次序，符合 style-bible 第 9 条。
- **重复与越界**：§1.1 提前给出 §2.5/§6.1 的结论；§2.4/§2.5 中段插入 Rust 对照；§5.3 夹带 TODO 与分流列；§6.2 的 8 条命令是正文复制件。A3 的"若只做五件事"可作修订清单。
- **设计视角**：§3 的"方案 → 否掉理由 → 代价"结构好；但只覆盖 8 项，eager parsing/per-arch 切分/const fn 分工/零基址断言/地址三语义/伪向量 0/`check_gic_madt` 七项在 §4 首次出现，没有备选对比。

### 8.3 教学 M1–M9 与闭卷

A2 闭卷 12 问：11 可答、1 需一步推断（riscv64 不翻译的理由）、0 不能答；节级三态 17 ① / 1 ② / 0 ③。唯一 ② 是 §2.1 的"`apic_single_cpu_init` 自己调 `acpi_init`"——结论有，引文没有。子项级 ② 12 / ③ 13，集中在被点名但未展示的辅助函数（`platform_tbl_ptr`、`acpi_check_signature`/`csum`、`acpi_get_table_base`、`ioapic_read`、`lapic_enable`、`Frozen::get`）。

A2 的"认知跃迁丢失"清单按 M1–M9 记：M4（多来源顺序/兜底）与 M8（`Frozen` 二次冻结）的失败后果能答，但 release 行为与断言存续需要推断；M9（aarch64 ACPI-first 的失败后果）被显式推迟到 `16-smp.md`，本篇不可答。这两处是教学缺口，不是事实错误。

### 8.4 代码卓越

- **抽象**：`PlatformDesc` + 三个子 trait + `Any` downcast 的分层正确；`as_any` 必需方法的类型理由（`Self: Sized`）准确。
- **非法态**：aarch64 `new()` 的零基址断言 + `mmio_translate` 的窗口断言是正面样本；x86 没有零断言，且描述符地址根本不进 MMIO——"非法态封堵"在 x86 上是空的（P0）。
- **死字段/死函数**：`ioapic_version()`、`PitDesc.lapic_base`、`X86_64ClockArch.lapic_base`、`ApicDesc.lapic_base`/`ioapic_base`（P1/P2）。
- **跨架构判定**：三份 `QemuVirtDesc` per-arch 文件方案正确；`#[cfg]` 分布表（§4.6）与代码一致；但 `nr_irqs=64` 在 x86/aarch64 都是"容量常量"而非硬件线数，文档没有区分（P1-design-singleIoapic / P1-code-maskAllPins）。
- **测试质量**：解析器单测覆盖端到端与错误变体；缺口（MADT 畸形、RSDT 回落、多来源、Frozen 二次冻结）文档已自列 R04-T01..T08；`bsp_id` 断言恒真是新发现。

### 8.5 文风与承载

- `doc-style-lint` error=0；`doc-code-density` 4/4 通过。
- 过程编号（`S-2b`、`S-6.1 (D-40)`、`I-22`、`NK4-C 1.114`）未被 SL-11 覆盖（P1-doc-styleProcessNumbers）；其中 `S-2b` 出现在"逐字引用"块，属引文增字。
- 元话语密度偏高（`structure.md` §10），但多数有消歧价值；建议把纯流程说明移入 `.review`/trajectory。

---

## 9. Issue List（43 条）

> 与 `.review/zcode/01-stage-kernel/04-platform-discovery/scan.md` 同一清单。每条给"锚点 + 证据 + 修复方向"。

### 9.1 P0（6）

| ID | 位置 | 问题 | 证据 | 修复方向 |
|---|---|---|---|---|
| P0-fact-lapicConsumed | §2.5 L728；§4.8 L2087-2093；§6.1 L2468 | Rust 未在消费端使用 `ApicDesc.lapic_base`/`ioapic_base`；MMIO 走固定设备窗口；重定位 LAPIC 被"静默写错 + 稍后断言拒绝"，不是"补上" | `interrupt.rs:235-243,260-270,299-304`；`lib.rs:2190-2215`；`device_window.rs:141-146`；`clock.rs:47-77` | 二选一：① 让 `init_clock_and_interrupts` 用描述符/ MSR 基址动态建设备窗口，并让 accessor 用实例基址；② 把文档改为"Rust 检测并拒绝重定位（check-and-reject），不静默使用错地址"，并删掉"补上"结论 |
| P0-design-machineFields | §4.10 L2229-2232；§4.1 L1469 | `apic_enabled`/`acpi_rsdp` 恒 0，但 Rust ACPI 驱动与 C PCI 驱动仍读；ACPI 驱动永不初始化、PCI 桥扫描不跑 | `misc.rs:1086-1090`；`drivers/power/acpi/src/main.rs:40-50`、`service.rs:91-93,134-136`；C `pci.c:917,1895,2104` | 在 `getinfo_machine` 填 `apic_enabled=1`（x86 APIC 路径）与 `acpi_rsdp`（从描述符/ArchMiscDesc 取）；或明确删除/改写用户态消费者并在文档登记 |
| P0-code-acpiValidation | §4.4 L1637-1692；§1.6 L164 | Rust 丢 RSDP 20 字节校验、SDT 校验和、根表签名、根表容量闸；`find_madt` 无上限遍历 | C `acpi.c:45-52,90-130,192-202`；Rust `acpi.rs:170-210,329-350` | 补 RSDP/RSDT/XSDT/MADT 校验和与 `MAX_RSDT` 式容量上限；补负向测试（R04-T02 扩展） |
| P0-design-bspId | §4.9 L2126-2158；§4.10 L2228；§5.2 L2361 | `bsp_id` = 第一条 enabled 记录，不是实际 BSP；x86 无 election/ MSR 纠偏；测试断言恒真 | `acpi.rs:505-520,545-560`；`device_tree.rs:471-473`；C `arch_smp.c:302`；`lib.rs:1722-1740` | x86 用 `IA32_APIC_BASE`/`current_cpu_id()` 解析实际 BSP；或把字段改名为 `first_cpu_id` 并新增 `boot_cpu_id`；测试改为断言"实际执行 CPU 的身份" |
| P0-fact-staleSmpQuotes | §4.9 L2125-2155 | 两段生产代码引用过期；"余下 9 行"不成立 | `lib.rs:1749-1824,1977-2044` | 按现码重写 §4.9；补 Doc Code Sync 回归检查 |
| P0-code-dtbGuid | §1.2 L56；§5.2 L2364；§5.4 M1 L2426 | `DEVICE_TREE_GUID` 是 legacy `b1b621d2-…`，不是 UEFI/edk2 `b1b621d5-f19c-41a5-830b-d9152c69aae0`；测试注释的"spec 值"又是第三个；DTB 扫描在合规固件上永不命中 | `uefi_helpers.rs:40-42,121`；`test-smp-topo-aarch64:6-8`；edk2 `MdePkg/Include/Guid/Fdt.h` | 改用规范 GUID；修正测试注释；在 AAVMF 上实跑确认 DTB 项是否存在后再写成因 |

### 9.2 P1（18）

| ID | 位置 | 问题 | 证据 |
|---|---|---|---|
| P1-fact-lapicAssignments | §2.5 L722 | "C 活路径每处赋值都是常量"被 `memory.c:954` 证伪 | `memory.c:940-958` |
| P1-fact-noApicFn | §2.7 L781 | `no_apic` 解析在 `cstart`，不在 `kmain` | `main.c:304,442-453` |
| P1-fact-panicSite | §4.1 L1510 | 首个 panic 是 `init_protection`；两种错排同栈 | `lib.rs:2009` |
| P1-fact-runAllLines | §5.2 L2370-2375 | 六行行号漂移 | `rg -n 'test-smp-topo' run_all.sh` |
| P1-fact-uefiTestCount | §5.3 L2390 | 4 条测试，不是 1 条 | `grep -c '#[test]' uefi_helpers.rs` |
| P1-test-bspTautology | §5.2 L2361 | "BSP 在集合内"恒真 | 同 P0-design-bspId |
| P1-fact-kernelInfoQuote | §3.1 L1037-1048 | 注释节选过期 | `kernel_info.rs:112-134` |
| P1-quote-s2b | §3.2 L1083；§4.3 L1590 | 逐字引用被加 `S-2b` | `global.rs:113`；`kind.rs:61` |
| P1-fact-parseTimer | §4.5 L1839 | aarch64 `parse_timer` 不看 DTB | `device_tree.rs:300-305` |
| P1-design-singleIoapic | §2.6 L730-776；§3.3 L1149 | 单 IOAPIC 限制未声明 | `arch/x86_64.rs:21-27`；C `apic.c:919-945,1203-1225` |
| P1-code-maskAllPins | §2.6 L772；§4.1 L1502 | `mask_all` 写 24 针设备的越界 RTE；`ioapic_version` 死代码 | `interrupt.rs:207,317-321,359-376,423,483` |
| P1-design-gicZeroCpu | §4.4 L1753-1762 | `nr_cpus==0` 放行后默认 1 CPU 带 0 GICR | `acpi.rs:590-610,690-725` |
| P1-doc-apicContradiction | §4.1 L1469 vs §4.10 L2230 | 8259 活路径 vs "APIC 唯一路径" | `lib.rs:3497-3518`；`trap_dispatch.rs:443-446` |
| P1-doc-styleProcessNumbers | §3.2/§4.3/§4.8/§4.9 | `S-2b`/`S-6.1 (D-40)`/`I-22`/`NK4-C 1.114` 进正文 | 原文 |
| P1-test-hostCount | §5.1 L2304-2320 | 22 属性 vs 14 宿主运行 | `cargo test -p minix-platform` |
| P1-fact-threeAsserts | §4.8 L2095 | 称"两个零基址断言"；实际三个（gicd/gicr/gicr_stride），节选漏 `gicr_stride` 断言与字段 | `arm64/interrupt.rs:226-266` |
| P1-fact-translationTable | §4.8 L2089-2093 | 表说 x86/riscv"同值/否"；两者现在都经设备窗口 VA（`LAPIC_VA`/`IOAPIC_VA`；`plic_va_base`） | `interrupt.rs:219-243`；`riscv64/interrupt.rs:97-121` |
| P1-fact-todoQuote | §3.6 L1324-1335 | "按原文引用"的模块注释含源码没有的 `TODO-02-3`；原文是 "Once the kernel becomes an independent ELF," | `platform.rs:18-28`；`rg TODO-02-3` 无代码命中 |

### 9.3 P2（19）

| ID | 位置 | 问题 |
|---|---|---|
| P2-fact-sliceDistribution | §5.3 L2408 | `vm_handoff.rs` 记 1（实 0）；`handoff_v1_write.rs` 才是 1 |
| P2-code-ioapicComment | `acpi.rs:617` | `64` 与注释 `(16 ISA + 8 PCI)=24` 不符 |
| P2-code-sectionRef | `qemu_virt.rs:15`；`device_tree.rs:159` | §4.2.2 不存在 |
| P2-code-testGuidComment | `test-smp-topo-aarch64:6-8` | GUID 值错；`mpdir` 笔误 |
| P2-fact-pinsConsumer | §2.6 L776 | `pins` 实际消费者是 `ioapic_set_irq`，不是 `ioapic_enable_all` |
| P2-doc-zeroVtable | §3.2 L1108-1110 | `platform_desc()` 返回 `&dyn`，外层仍有 vtable |
| P2-doc-16smp | §1.5/§3.2/§7 | `16-smp.md` 不存在 |
| P2-fact-topoComment | §4.5 L1857-1863 | "源码注释"是中文翻译；题注未说明 |
| P2-code-lineRef | §4.5 L1818-1823 | 代码注释 `lib.rs:27-28` 不是 `pg_identity` 语义 |
| P2-fact-picEoiComment | §4.1 L1469 | 称 `pic_eoi` 注释标了 C `irq_8259_eoi`；实际没有 |
| P2-doc-crossref35 | §3.5 L1247 | `detect_ioapics` 在 §2.1，不在 §2.6 |
| P2-fact-consumerCount | §4.9 L2122 | 称三处消费者；`rg` 至少 7 处 |
| P2-fact-sswiPsci | §3.2 L1073 | `sswi_setip_base()` 还决定 `sys_schedctl` 亲和钳制 |
| P2-fact-twoExits | §1.4 L116；§4.1 L1482 | 漏掉 `acpi_init_poweroff`（FACP） |
| P2-test-plicContext | §5.1 L2298 | `plic.context == 1` 断言对硬编码值自证 |
| P2-fact-aclintMtimecmp | §4.5 L1866 | ACLINT 路径下 `CpuInfo.mtimecmp_addr` 为 None |
| P2-fact-giccUid | §4.4 L1705-1723 | ACPI 用 GICC UID 当 GICR 帧索引，假设未记录 |
| P2-code-aarch64TestsCompile | §5.1 L2322 | `acpi.rs` 两条测试在 aarch64 测试构建下编译失败 |
| P2-fact-riscvExcerpt | §3.3 L1163-1173 | riscv `fn new` 节选漏 `plic_va_base` 字段 |

---

## 10. 对抗 agent 记录

| Agent | 输入隔离 | 结果 | 与本报告的关系 |
|---|---|---|---|
| A2 闭卷学生 | 只读 `04-platform-discovery.md`，不读源码/规则/其他文档 | 12 问：11 可答、1 需推断；节级 17 ① / 1 ② / 0 ③；指出 §4.1 与 §4.9 的首 panic 点冲突、`Frozen` 断言文本、riscv 不翻译理由缺失 | 独立命中 P1-fact-panicSite、P2-doc-zeroVtable 的相邻问题；其"首 panic 点"与主报告一致 |
| A3 冷读结构编辑 | 只读文档；不读源码/规则/diff/自评 | 骨架成立、弧线中段断开、§3 设计章不完整、§1 提前发结论、§4.10 偏离主线、LAPIC 线停在解析 | 直接支撑 structure.md 与 R2；A3 的"LAPIC 缺消费环"被代码核对升级为 P0-fact-lapicConsumed |
| A1 代码事实核查 | 只给文档 + 文件清单；不给主会话发现 | 30 条：1 P0 / 10 P1 / 19 P2（报告 664 行，附命令与输出） | P0（GUID）被采纳为 P0-code-dtbGuid；P1 的 lapicAssignments/threeAsserts/translationTable/todoQuote 等已并入；其余与本报告重叠或降为 P2 |

**分级差异**：A1 把 §4.9 两处引用与 ACPI 校验缺失定为 P1；本报告定为 P0（Doc Code Sync 断裂 + 核心语义/安全）。差异按证据强度裁决：两处引用是"生产代码已变、文档仍在教旧行为"，ACPI 校验是"C 有、Rust 无"的外部行为回归；不由票数决定。

**同模型局限**：A1/A2/A3 与主会话同模型；用材料隔离 + 独立重放补偿，但不宣称异构。A1 的代码事实读数与主会话在 GUID、LAPIC 消费、ACPI 校验、§4.9 引用、`&[]` 分布五处独立重合；冲突项按证据强度裁决（不以票数）。

---

## 11. Rule Discovery（Step 5.7）

本轮发现两个候选模式：

1. **"描述符死字段"模式（新）**：文档把"解析出字段"当成"消费端用上"，但消费端用全局常量/固定窗口。判据：对每个描述符字段做 `rg '<field>'`，若只在 `new()`/测试/`Debug` 出现，就是死字段；文档声称"补上/统一/消费"时必须给消费点。与本轮 P0-fact-lapicConsumed 对应。
2. **"淘汰字段未查消费者"模式（新）**：C 的发布结构字段被标"淘汰/恒 0"时，必须检索仓库内所有读者（含用户态服务器/驱动），否则 ABI 语义变化会静默切断下游。与本轮 P0-design-machineFields 对应。

两个模式都建议进 `prompt/review-rules/review-patterns.md`；本轮不擅自改规则，交用户裁决。

---

## 12. 轨迹复盘（R5）

### 12.1 计划维度 vs 实际执行

| 计划 | 执行 | 偏差 |
|---|---|---|
| R0 预检 | ✅ | 无 |
| R1 Claims/C/Rust/覆盖/链路/跨文档/测试/代码 | ✅（C 引用逐块、Rust 引用逐块、49 文件代码维度） | 数据结构覆盖以"关键字段"为主，没有逐字段展开 27 字段级清单（本篇对象不是 TSS 类结构） |
| R2 structure/文档/教学/代码/文风 | ✅（structure.md、A2/A3、code-excellence） | M1–M9 的"认知跃迁丢失清单"按 A2 的节级三态替代，未逐条 M# 打表 |
| 对抗 A1/A2/A3 | 三份均返回；A1 30 条、A2 12 问、A3 结构报告 | A1 的 P0 与主会话 P0 重合；A1 的 19 条 P2 已去重并入 |
| R3 Gate G + QEMU | ✅（机械 PASS；5/5 人工；三架构 PASS） | QEMU 只跑 `test-smp-topo`，未跑完整套件 |
| R4 报告/轨迹 | ✅ | 报告 43 条（A1/A2/A3 去重后），P0 密度高于 03 |
| R5 复盘 | ✅ | 见下 |

### 12.2 每条发现首次出现在哪

- P0-fact-lapicConsumed：主会话读 `os/plat/src/x86_64/interrupt.rs` 的 `let _ = base;` 时首次出现；A3 独立从结构上判"LAPIC 停在解析"，但没有代码证据。二轮由 `device_window.rs` + `lib.rs` 的固定窗口确证。
- P0-design-machineFields：主会话 `rg 'apic_enabled' os/drivers` 时首次出现；A2 未读源码，未命中。
- P0-code-acpiValidation：主会话对照 C `acpi_read_sdt_at` 与 Rust `find_madt` 时首次出现。
- P0-design-bspId：主会话读 C `smp_init` 的 `bsp_cpu_id = apicid2cpuid[apicid()]` 时首次出现；A2 的 Q10 指出"test 断言与解析器同源"的相邻问题。
- P0-fact-staleSmpQuotes：主会话逐块 diff §4.9 时首次出现；这是 03 修复后的代码漂移，不是文档原始错误。

### 12.3 零发现反证探针

随机抽 3 条"看起来已证实"的断言重跑：

1. `uefi_helpers.rs` 测试数：文档说 1，实跑 4 → 不是零发现，已列 P1。
2. `run_all.sh` 行号：文档说 82/87/92/115/195/230，实跑 96/102/108/131/211/246 → 已列 P1。
3. §4.5 `from_fdt` 逐字引用：与 `device_tree.rs:161-187` 对齐 → 零发现，确认引用可信。

### 12.4 最弱项自检 5 问

1. **C 覆盖**：本篇 C 面是 `acpi.c`/`apic.c`/`arch_smp.c`/`memory.c`/`main.c`/earm；`acpi.c` 全函数覆盖，`apic.c` 只覆盖发现/引脚相关函数（`ioapic_set_irq` 等消费函数只引用），这是篇界而非漏写。
2. **链路**：§2.10→§3、§3→§4 的链路成立；§4.9 的引用断裂已列 P0。
3. **跨文档**：01/02/03/05/06 可解析；`16-smp.md` 缺失已列 P2。
4. **错误路径**：ACPI 解析器的错误路径覆盖了签名/无根表/无 MADT/太短/GIC 三错误；缺校验和/容量上限（P0）。
5. **design 对齐**：v1 过期，v2/v4 重评；`bsp_id`/`machine` 字段/单 IOAPIC 三处偏差已列 P0/P1。

### 12.5 如果我错了，最可能错在哪

- `P0-fact-lapicConsumed`：如果 `LAPIC_VA` 在某条路径会被设置为描述符基址（例如 boot-shim 或 arch 初始化），则"不使用描述符"不成立。已检索 `set_lapic_va` 的全部调用点：只有 `lib.rs:2206`，参数来自固定窗口；没有描述符输入。仍可能错在"设备窗口是否由描述符动态建设"这一设计意图上——若设计意图就是"固定窗口 + 拒绝重定位"，那 P0 应降为 P1（文档措辞），而不是代码 bug。
- `P0-design-bspId`：如果 ACPI 规范/固件约定保证 BSP 第一条，则实际风险降低；但 C 的实现不依赖该约定，且 QEMU 以外的固件不保证。仍应按"语义偏差"处理。
- `P0-code-acpiValidation`：如果项目的安全模型把固件表视为"完全可信、校验是 C 的历史包袱"，则可降为 P1；但 C 的校验是外部行为的一部分，且 `parse` 的 `unsafe` 契约只保证指针有效、不保证内容良构。

### 12.6 本次 review 自身的缺陷与改进

1. **同模型盲点**：A1/A2/A3 与主会话同模型；已用材料隔离 + 独立重放补偿。改进：下一轮优先换模型家族或至少换代际。
2. **QEMU 范围窄**：只跑 `test-smp-topo` 三架构，没有跑 `test-timer-irq`/`test-smp-aps`。改进：回归轮补完整套件。
3. **覆盖率分母**：39.4% 是工具分母过宽；改进：给 semantic-map 加"本篇射程"过滤，或让工具按文档作用域计算。
4. **快照过期**：`04-design.v1` 与现码脱节；本轮 v2/v4 重评。改进：每轮 review 必须重评，不把旧快照当 Gate H 依据。
5. **报告长度**：43 条 issue 的完整表 + R2 展开，阅读成本高；但用户要求深度全面，保留。改进：下一轮按章拆 scan，主报告只留结论与证据索引。

### 12.7 规则缺口与是否升级为模式

- "描述符死字段"与"淘汰字段未查消费者"两个候选模式（§11）建议进模式库；本轮不改 `prompt/`，交用户裁决。
- `doc-ref-check` 不覆盖跨文档同号引用；`doc-style-lint` 不覆盖 `S-2b` 类过程编号；`verify-check.py` 不检查 issue 证据强度。三条工具局限已在本轮人工补位，建议后续工具化。

---

## 13. 确认清单

- [x] 受审文档与 `os/` 代码零改动；`git status` 仅 `AI-chats/daily.todo.md`（用户原有）。
- [x] `scan.md` 9 锚段 + 8 Gate 证据块；`verify-check.py --self-check` exit=0。
- [x] Issue List 43 条与主报告 §9 一致（6 P0 / 18 P1 / 19 P2）。
- [x] 每条 P0 有可重放证据；每条 P1 有锚点与修复方向；P2 有位置。
- [x] 覆盖率 `--check-filled` exit=0；Gate A 判 PARTIAL 并给 `MANUAL_FALLBACK` 理由。
- [x] QEMU 三架构 `test-smp-topo` PASS，读数为 `nr_cpus=4`、`bsp=0`。
- [x] 宿主 `minix-platform` 14 passed、`minix-boot` 65 passed。
- [x] 对抗 agent 的发现已归因（A1 30 条、A2/A3 全量；去重后并入 43 条）。
- [x] 轨迹文件含失败与返工；复盘回答 7 问。
- [x] 状态：**NOT CONVERGED**（P0=6 open）；Layer 2 PARTIAL。

---

## 14. 附录

### 14.1 QEMU 三架构输出（`tmp/review04/qemu-topo.log`）

```text
### test_smp_topo: discovering topology (RSDP → MADT)...
  nr_cpus = 0x0000000000000004, bsp hw_id = 0x0000000000000000
  cpu[0x0000000000000000] hw_id = 0x0000000000000000 (BSP)
  cpu[0x0000000000000001] hw_id = 0x0000000000000001
  cpu[0x0000000000000002] hw_id = 0x0000000000000002
  cpu[0x0000000000000003] hw_id = 0x0000000000000003
### TEST_RESULT: PASS test-smp-topo ###

### test_smp_topo (aarch64): topology discovery pin (ACPI via S-2b)
  nr_cpus = 0x0000000000000004, bsp mpdir/hw_id = 0x0000000000000000
### TEST_RESULT: PASS test-smp-topo-aarch64 ###

### test_smp_topo (riscv64): discovering topology (DTB via a1)...
  nr_cpus = 0x0000000000000004, bsp hw_id = 0x0000000000000000
  cpu[0x0000000000000000] hw_id = 0x0000000000000000 (BSP)
  cpu[0x0000000000000001] hw_id = 0x0000000000000001
  cpu[0x0000000000000002] hw_id = 0x0000000000000002
  cpu[0x0000000000000003] hw_id = 0x0000000000000003
### TEST_RESULT: PASS test-smp-topo-riscv64 ###
```

### 14.2 宿主测试

```text
$ cargo test -p minix-platform
test result: ok. 14 passed; 0 failed; 0 ignored
$ cargo test -p minix-boot
test result: ok. 65 passed; 0 failed; 0 ignored
```

### 14.3 复现命令（本轮关键证据）

```bash
# 描述符地址未进入 MMIO
rg -n 'let _ = base;' os/plat/src/x86_64/interrupt.rs
rg -n 'LAPIC_VA|IOAPIC_VA' os/plat/src/x86_64/interrupt.rs
rg -n 'device_window_regions' -A 6 os/arch/src/arch/device_window.rs

# machine 字段消费者
rg -n 'apic_enabled|acpi_rsdp' os/drivers/power/acpi/src os/kernel/src/misc.rs

# ACPI 校验
rg -n 'checksum|check_csum|entry_count' os/libs/minix-platform/src/acpi.rs
sed -n '45,52p;90,130p;192,202p' minix3/minix/kernel/arch/i386/acpi.c

# bsp_id 语义
sed -n '285,330p' minix3/minix/kernel/arch/i386/arch_smp.c
sed -n '505,520p' os/libs/minix-platform/src/acpi.rs

# §4.9 漂移
sed -n '1749,1824p;1977,2044p' os/kernel/src/lib.rs
```

---

**报告状态**：NOT CONVERGED；P0=6 open；Layer 2 PARTIAL。下一轮若用户要求修复，按 `fix-guard.md` 一次一条，先修 P0-code-dtbGuid / P0-fact-lapicConsumed / P0-design-machineFields / P0-code-acpiValidation 四条代码侧，再同步 §4.9 引用与测试断言。

---

## 15. 修复轮与 R2 回归 full review（2026-11-03）

> 本节是修复后的追加结果。§1–§14 保留为修复前基线（NOT CONVERGED）；本节给出关闭状态、回归证据与剩余风险。
> 完整逐条关闭表见 `.review/zcode/01-stage-kernel/04-platform-discovery/VERIFY-CHECK-R2.md`。

### 15.1 修复范围与原则

- 按 `fix-guard.md`：每条先读目标行 ±5、`rg` 确认现状、一次一修、修后复验；P0 先修代码再同步文档。
- 受审文档与 `os/` 代码均修改；未改 `prompt/`、`tools/`、`minix3/`。
- 43 条 Issue List 全部处置：6 P0 关闭（其中 2 条以"代码校验 + 显式限制"关闭），18 P1 关闭，19 P2 关闭。

### 15.2 P0 关闭表

| ID | 修复 | 回归证据 |
|---|---|---|
| P0-code-dtbGuid | `uefi_helpers.rs` 的 `DEVICE_TREE_GUID` 改为 UEFI 2.4+ 规范值 `b1b621d5-f19c-41a5-830b-d9152c69aae0`；新增 `LEGACY_DEVICE_TREE_GUID`（`b1b621d2-…`）并同时匹配；测试注释与 `mpidr` 标签修正 | aarch64 QEMU `test-smp-topo-aarch64` PASS，`platform sources=1`（无 FDT，走 ACPI）；`rg` 三处常量一致 |
| P0-fact-lapicConsumed | `X86_64InterruptController::init_lapic` 断言 `IA32_APIC_BASE == 描述符基址`；`init_clock_and_interrupts` 断言 LAPIC/IOAPIC 描述符基址 == 设备窗口页；文档 §2.5/§4.8/§6.1 改为"check-and-reject，不静默使用错窗口" | x86 `test-smp-topo` / `test-timer-irq` / `test-protection` PASS；重定位支持登记 R04-T09 |
| P0-design-machineFields | `getinfo_machine` 填 `apic_enabled`（x86=1，其他=0）与 `acpi_rsdp`（`ArchMiscDesc.acpi_tables`）；抽出 `machine_record` 并加宿主测试 | `test_machine_record_publishes_apic_mode_and_acpi_rsdp` PASS；`cargo test -p minix-kernel --features mock` 857 passed |
| P0-code-acpiValidation | RSDP 20 字节校验；根表签名/35 项容量/校验和；MADT 签名/64 KiB/校验和；新增 4 条负向测试 | `cargo test -p minix-platform` 18 passed；QEMU 三架构 topo PASS |
| P0-design-bspId | `elected_bsp_hw_id` x86 用 CPUID leaf 1 APIC ID，aarch64/riscv64 用 election cell；`init_smp_state`/`init_protection` 使用 election；三个 topo 载体断言执行 CPU 身份 | QEMU `test-smp-topo` 三架构 PASS；`nr_cpus=4`、`bsp=0` |
| P0-fact-staleSmpQuotes | §4.9 按当前 `init_smp_state`/`init_protection` 重写（`elected_bsp_hw_id`、`seed_bsp_masks`、`program_entry_base`） | 文档门全绿；`rg` 引文与 `lib.rs:1749-1824,1977-2044` 对齐 |

### 15.3 P1/P2 关闭摘要

- **P1 18/18**：`lapicAssignments`（§2.5 物理基址/VA 区分）、`noApicFn`（cstart）、`panicSite`（init_protection 首读）、`runAllLines`（96/102/108/131/211/246）、`uefiTestCount`（4 条）、`bspTautology`（执行 CPU 身份断言）、`kernelInfoQuote`（现注释）、`quote-s2b`（移除增字）、`parseTimer`（aarch64 定时器节点检查）、`singleIoapic`（显式限制 + R04-T10）、`maskAllPins`（只遍历 0..16，删除 `ioapic_version`）、`gicZeroCpu`（无 GICD 直接拒绝）、`apicContradiction`（8259 初始化但投递走 APIC）、`styleProcessNumbers`（全部移出正文）、`hostCount`（26 属性 / 18 宿主）、`threeAsserts`（三个断言 + `gicr_stride`）、`translationTable`（PA/实例/MMIO 三列）、`todoQuote`（恢复源码原文）。
- **P2 19/19**：分布表（`handoff_v1_write`）、`nr_irqs` 注释、§4.2.2→§4.6、测试 GUID/mpdir、`pins` 消费者、零 vtable、`16-smp` 标注、topo 注释译文、`lib.rs:27-28` 锚点、`pic_eoi` 注释、§3.5 交叉引用、消费者计数、`sswi` 的 sched 消费者、`acpi_init_poweroff` 出口、PLIC context 自证、ACLINT mtimecmp（`parse_timer` 回落）、GICC UID 假设、aarch64 测试 cfg、riscv `new` 字段。

### 15.4 回归 full review 结果

| 项 | 结果 |
|---|---|
| 文档门 | anchor 74/74 zero-def 0；doc-ref exit 0；style error=0；density 4/4 |
| 宿主 | `minix-platform` 18 passed；`minix-boot` 65 passed；`minix-kernel --features mock` 857 passed / 8 ignored |
| QEMU topo | x86_64 / aarch64 / riscv64 全部 PASS，`nr_cpus=4`、`bsp=0` |
| QEMU 关键回归 | `test-timer-irq` x86_64 PASS；`test-protection` x86_64 PASS；`test-smp-ipi-riscv64`（aclint=on）PASS；`test-smp-aps-aarch64`（-dtb）PASS |
| QEMU 基线失败 | `test-smp-aps` x86_64 在 `protection.rs:451` stamp 断言失败；**用 `git stash` 去掉本轮 04 代码改动后同样失败**，故不是本轮引入；建议另开修复轮 |
| 结构回归 | 新增 §3.9（实现层设计决定索引）与 §4.11（三架构六段表）；§4.10 标为对账附录；`PlatformSource`/`ArchMiscDesc` 补定义 |
| 新发现 | 无新增 P0/P1；`MAX_TABLE_LEN` 注释"one page"改为"64 KiB"（修复轮内自纠 P2）；`test-smp-aps` 基线失败单列 |

### 15.5 剩余风险与 backlog

- **R04-T09**：LAPIC/IOAPIC 重定位仍不支持（当前 check-and-reject）。
- **R04-T10**：多 IOAPIC 仍不支持（单 `ApicDesc`）。
- **R04-T11**：aarch64 ACPI 的 GICC UID → GICR 帧索引假设（非 QEMU 固件不保证）。
- **R04-T01..T08**：测试缺口仍在（RSDT 回落、MADT 畸形记录、ACLINT 宿主夹具、aarch64 DTB 宿主夹具、`find_platform_sources` 单测、多来源兜底、`Frozen` 二次 freeze、`parse_by_kind` 分支）。
- **结构性 backlog**：§1 仍提前给出部分 §2.5/§6.1 结论；§5.3/§5.4 仍是两套覆盖视角。Gate D-6 仍 PASS（带 P1/P2），未把这两项升级为 P0/P1。
- **同 agent Gate G**：8/8 抽样一致，但不宣称异构独立；A1 的 P0（GUID）与主会话独立重合。

### 15.6 最终状态

- **Issue List**：43/43 关闭（6 P0 / 18 P1 / 19 P2）。
- **Layer 1**：PASS（P0 全关；回归门禁通过；`test-smp-aps` 基线失败单列）。
- **Layer 2**：PASS（带 warning；结构性 backlog 见 §15.5）。
- **最终判定**：**CONVERGED_WITH_WARNING_BACKLOG**。
- **STATE.md**：04 节更新为 CONVERGED_WITH_WARNING_BACKLOG；`test-smp-aps` 基线失败与 R04-T09..T11 记为 warning/backlog。
