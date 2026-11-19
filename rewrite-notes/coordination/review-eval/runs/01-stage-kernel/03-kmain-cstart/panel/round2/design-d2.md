# D03 R2 确认轮 · P-DESIGN-D2

- 基线：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`，1384 行，sha256 前缀 `093de231`（全量 `093de2311fb54746c3841dfbe7f17dbb97719a60cf86f1f304ca88b6c861faa7`）。
- 范围：文档第 3/4 章（§3.1–§3.7、§4.1–§4.7）对照 doc-code-map 的 21 个 Rust 文件；C 侧只复核文档自身给出的读数，未重读 `minix3/`。只读审查，未修改任何文件，未实施 ⑥。
- 标签：`[MODERN]` = Rust 设计/陈述与实现不符或可现代化；`[DEL]` = 死抽象、死数据、翻译残留删除项；`[ARCH]` = 架构演进（本轮无）。
- 严重度：P1 = 事实错误（必改）；P2 = 设计陈述与实现不符，或可达非法状态/验证缺口；P3 = SUGGESTION（默认不阻断，按 D-046 的读者 P3 分流）。

## 复核证据

下列读数全部在本轮重放；没有「通过」字样而无命令与输出的条目。

```text
$ sha256sum rewrite-notes/01-stage-kernel/03-kmain-cstart.md
093de2311fb54746c3841dfbe7f17dbb97719a60cf86f1f304ca88b6c861faa7  .../03-kmain-cstart.md

$ grep -c '#\[test\]' os/arch/src/x86_64/protection.rs os/arch/src/x86_64/trap_entry.rs \
    os/arch/src/x86_64/trap_stub.rs os/arch/src/arm64/protection.rs \
    os/arch/src/arm64/trap_entry.rs os/arch/src/arm64/trap_stub.rs \
    os/arch/src/riscv64/protection.rs os/arch/src/riscv64/trap_entry.rs \
    os/arch/src/riscv64/trap_stub.rs
16 / 13 / 11 / 2 / 1 / 3 / 2 / 1 / 7        # 合计 55，与 §5.1 一致

$ sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs \
    | grep -cE '^\s+\(0x|^\s+\([0-9]+,'
41

$ sed -n '/pub fn init_protection/,/^}/p' os/kernel/src/lib.rs \
    | grep -nE 'cr3|paging|pg_|PageTable|map_'
（无输出，exit 1）

$ grep -rn "fn add_memmap\|fn cut_memmap" --include=*.rs os --exclude-dir=target
（无输出；全部 add_memmap 命中均为注释）
```

## DIFF-TABLE（七维）

| 维 | 文档陈述（行） | 代码事实（锚点） | 判定 |
|---|---|---|---|
| D1 事实/数值 | §3.1 GDT 表长：C=7、Rust=6+8×2=22；每 CPU 两个 TSS 槽 | `os/arch/src/x86_64/protection.rs:63-85`：`GDT_TSS_FIRST_INDEX=6`、`GDT_SLOTS_PER_TSS=2`、`GDT_ENTRIES=22` | 符 |
| D1 | §3.7 `IDT_ENTRIES=256`、`STUB_VECTORS=41`、`41=18+4+16+3` | `x86_64/trap_entry.rs:114`；`x86_64/trap_stub.rs:168-216`；命令读数 41 | 符 |
| D1 | §5.1 九文件 `#[test]` 计数 16/13/11/2/1/3/2/1/7（合计 55） | 命令读数逐项一致 | 符 |
| D1 | §4.1 `bootstrap_start/len` 仍为 `(0, 0)` | `os/kernel/src/lib.rs:1858-1863` 的 Phase F 注释：「the two fields were removed from `KernelInfo` (D01-F03), so there is nothing to reclaim and no field to validate.」 | 不符 → F01 |
| D1 | §4.7 `kernel_may_alloc` 闸门「无对应物」 | 函数/AtomicBool 确不存在；但 `lib.rs:3701` 仍写「5. Set kernel_may_alloc = false」、`:3706` 仍写「kernel_may_alloc uses AtomicBool」 | 部分符 → F03 |
| D1 | §4.3 四个段描述符访问字节 0x9A/0x92/0xFA/0xF2，代码段 L=1、数据段 B=1 | `x86_64/protection.rs:105-120` 常量逐字一致 | 符 |
| D2 类型/布局 | §3.1 Tss64：104 字节、`ist[7]`、`iobase=0x8000` | `x86_64/protection.rs:87,122-150`；`tss64_size_is_104_bytes`/`tss64_offsets_correct`（sp0@4、ist@36、iobase@102） | 符 |
| D2 | §4.4 `IdtEntry64` 七字段、`ist & 0x7`、`dpl & 0x3` | `x86_64/trap_entry.rs:69-105`（16 字节布局） | 符 |
| D2 | §3.4/§4.5 `GsArea` 三字段（kernel_stack_top/user_rsp/cpu_id） | `x86_64/trap_stub.rs:641-649`；`test_gs_area_layout_frozen` 把偏移钉在 0/8/16 | 符 |
| D2 | §3.3 结构体骨架 `gdt/tss/cpu_count/boot_cpu_id` | `x86_64/protection.rs:183-190` 字段一致；`cpu_count` 生产无读者，仅测试读取 | 符，但存在死字段 → F05 |
| D3 次序/不变量 | §4.2 序列：store → refresh → load → TrapEntry::init → install → configure_syscall → register → store → trap.load | `os/kernel/src/lib.rs:2230-2282` 逐步一致 | 符 |
| D3 | §4.3 `load_with_tss` 五步；`program_gs` 必须在它之后 | `x86_64/protection.rs:479-550`；`init_ap` 的 `Load order matters` 注释在 `:464-469` | 符 |
| D3 | §3.7 `set_handler` 写 IST=0，安装器随后恢复 2/8 | `x86_64/trap_stub.rs:613-633`；`x86_64/trap_entry.rs:138-146,333-341` | 符 |
| D3 | §4.2/§3.2 「BSP」在 `init` 与 `program_entry_base` 中是同一个 CPU | `lib.rs:2230` 传字面量 `0`；`lib.rs:2249-2254` 传 `bsp_logical`；`smp.rs:1150-1163` 以 `bsp_logical` 跳过 BSP 并回写其余槽 | 不符 → F04 |
| D4 失败模式 | §3.3 E1：栈上实例被搬走后描述符基址失效，首次 CPL3→CPL0 `sp0=0`，压栈写 VA −8 缺页 | `lib.rs:2232-2236` 注释原文逐字一致 | 符 |
| D4 | §4.5 GS 先于 `load_with_tss` → 第 4 步 `mov gs,0` 清 GS BASE，`gs:0x10` 读回 IVT 字节 | `x86_64/protection.rs:464-469` 注释原文一致 | 符 |
| D4 | §4.2 EFER 先于 LSTAR → 首次 `syscall` 跳地址 0；`configure_syscall` 次序 STAR→LSTAR→SFMASK→EFER | `x86_64/trap_entry.rs:283-298` 次序一致 | 符 |
| D4 | §4.6 `SP_EL1` 在 EL1 可访问（HCR_EL2.TSP=0） | `qemu-tests/.../test-protection-aarch64/src/main.rs:9-12` 写「UEFI sets HCR_EL2.TSP=1, which traps all SP_EL1 accesses」；同文件 `:368` 又写「TSP=0 confirmed」；探针只打印不判定（`:289-306`） | 证据冲突 → F09 |
| D5 跨架构 | §4.6 aarch64：`init` 写 `TPIDR_EL1`、`set_kernel_stack` 文档化 no-op、`load` 只发 `isb` | `arm64/protection.rs:102-118,140-203`；`arm64/trap_entry.rs:93-111` | 符 |
| D5 | §4.6 riscv64：`sscratch`、用户腿第一条无条件交换、SPP 只在返回重锚 `stvec` 时读、`prepare_user_return` 重写本 hart | `riscv64/protection.rs:70-77`；`riscv64/trap_stub.rs:534-664`；`arch/tests/riscv64_percpu_trap_stack_pin.rs:24-68`（pin `prepare_user_return` 与 per-hart sscratch） | 符 |
| D5 | §3.2 `Privilege` 仅测试/接口预留 | 全仓无运行期消费者（仅 impl/测试/一个 arch 测试的文本 pin）；文档已披露 | 符，死抽象 → F05 |
| D6 证据/测试 | §3.7 门元数据生产被 `set_handler` 覆盖；§5.1/§5.4 M1 称 13 条 trap 测试「已覆盖」 | 13 条 trap 测试里 9 条 `idt_init_*` 断言 init 写下的门表，2 条 `set_handler_*` 只测单个向量的 DPL/地址写入，2 条是 `IdtEntry64`/`IdtPtr` 布局测试；没有 post-install 41 门断言，也没有 `user_accessible`/`set_gate_ist` 的断言 | 内部张力 → F06 |
| D6 | §5.2 x86 载体不调 `install_trap_stubs`，门内入口保持占位 0 | `test-protection/src/main.rs:74-79` 只调 `init`/`load`/`configure_syscall`/`load` | 符 |
| D7 命名/翻译 | §4.1 「用 `cut_memmap` 扣除」与「没有 `add_memmap` 调用」 | `grep -rn "fn cut_memmap\|fn add_memmap" os --include=*.rs --exclude-dir=target` 无输出；`lib.rs:2827` 注释仍称 VM 模块物理内存「reclaimed via `add_memmap` (undoes the `cut_memmap` from Phase A.2)」 | 不符 → F07 |
| D7 | §3.7 「Rust 侧相应增加」（承接 C 的 64 条 IOAPIC 门）；§4.7 「APIC 路径另加 IOAPIC 门」 | Rust 无 0x58–0x8F 门：`STUB_VECTORS` 的 41 项里只有 16 条 PIC 向量，且注释明写该范围「reused as the IOAPIC delivery range」（`trap_stub.rs:294-306`）；`X86_64TrapEntry::init` 也只覆盖同样 16 条 PIC 向量（没有扩展 IOAPIC 区） | 不符 → F02 |
| D7 | §4.1 节选代码 vs `lib.rs:1687-1688` | 源码是 `let kernel_info: &KernelInfo =` 换行后接 `crate::kernel_info()...`；文档把 `crate::kernel_info()` 提到第一行 | 行重排（D-045 只许省略）→ F08 |

## FINDING RD3B-DESIGN-D2-01（P1，[MODERN]）

**文档位置**：§4.1，文档第 965 行末句：

> 「`bootstrap_start/len` 仍为 `(0, 0)`。」

**代码事实**：

- `os/kernel/src/lib.rs:1858-1863` 的 Phase F 注释逐字写着：「the two fields were removed from `KernelInfo` (D01-F03), so there is nothing to reclaim and no field to validate.」这句话直接否定了「仍为 `(0, 0)`」：字段已被移除，Phase F 也没有可做的回收校验。
- `os/kernel/src/lib.rs:3916-3919` 的 Step 8 注释同样只谈 `kernel_may_alloc` 的淘汰，不再出现任何 `bootstrap` 字段。

**差异与影响**：这不是措辞偏差，而是陈述了不存在的字段。读者会据此去 `KernelInfo` 里找 `bootstrap_start/len`，或以为 Phase F 仍要做一次 `(0,0)` 的回收校验；实际两件事都已随 D01-F03 消失。该句与同段前文「Phase A.2 … 故意为空」以及 `add_memmap` 已无调用自相矛盾。

**建议**：删掉该句，改为「`bootstrap_start/len` 已随 D01-F03 移出 `KernelInfo` 与 handoff envelope（协议版本 8），Phase F 不做回收」。修复为一句删除/替换，不改设计骨架。

## FINDING RD3B-DESIGN-D2-02（P2，[MODERN]）

**文档位置**：

- §3.7，文档第 947 行末句：「…39 − 16 + 64 + 3/5 = 90/92（3 与 5 的差来自 `CONFIG_SMP` 的 0xF1/0xF2）；Rust 侧相应增加。」
- §4.7，文档第 1198 行「差异原因」列：「长模式保留 9/15、增加 LAPIC/调度向量（PIC 对照；APIC 路径另加 IOAPIC 门）」。

**代码事实**：

- `os/arch/src/x86_64/trap_stub.rs:168-216` 的 `STUB_VECTORS` 固定 41 项；PIC/IOAPIC 交付范围只有 `0x50-0x57` 与 `0x70-0x77` 共 16 项，没有 `0x58–0x8F`。
- 同文件 `:294-306` 的注释明写：「8259A-style PIC vector range, **reused as the IOAPIC delivery range**」。
- `X86_64TrapEntry::init`（`x86_64/trap_entry.rs:197-261`）同样只覆盖 16 条 PIC 门（没有 `0x58–0x8F` 的扩展 IOAPIC 区）。

**差异与影响**：C 的 APIC 路径确实把 16 条 PIC 门换成 64 条 IOAPIC 门（0x50–0x8F），所以 90/92 的算术对 C 成立；但 Rust 侧没有这个增量，IOAPIC 复用 16 条 PIC 向量。文档把 C 的对照结论顺势写成 Rust 也「相应增加」，会让读者以为 Rust 的 IDT 有 64 条 IOAPIC 门，并在诊断时去查不存在的向量。§4.7 的「APIC 路径另加 IOAPIC 门」至少也是歧义句。

**建议**：把 §3.7 末句改为「Rust 侧不照搬这条 APIC 增量：IOAPIC 复用 `0x50–0x57 / 0x70–0x77` 这 16 条 PIC 向量（`trap_stub.rs` 的 `STUB_VECTORS` 注释与 `ioapic_route_and_mask` 是锚点），门总数保持 41」；§4.7 的括注同步改写。若未来真的要扩到 64 条 IOAPIC 门，需单列设计决策，不能以「相应增加」带过。

## FINDING RD3B-DESIGN-D2-03（P2，[DEL]）

**文档位置**：§4.7，文档第 1189 行：

> 「`kernel_may_alloc` 闸门 | 无对应物（D01-F08 删除唯一读者） | 淘汰 | 引导期写权限由调用图与 FreeRangeSet 保证（`lib.rs:3916-3919`）」

**代码事实**：

- 函数与静态量都不存在：`grep -rn "kernel_may_alloc" os/kernel/src` 只命中三处注释（`lib.rs:3701`、`:3706`、`:3916`）；没有同名的 `AtomicBool`（`grep -rn "MAY_ALLOC" os/kernel/src` 无输出）。
- `os/kernel/src/lib.rs:3701` 的 `bsp_finish_booting` 文档注释仍列「5. Set kernel_may_alloc = false」；`:3706` 仍写「Design decision D8 (08 §3): kernel_may_alloc uses AtomicBool.」。
- 同一函数的 Step 8 注释（`:3916-3919`）则写「C 的 kernel_may_alloc = 0 在 Rust 侧没有对应物——该开关唯一的读者是已删除的 add_memmap 断言」。

**差异与影响**：文档说「无对应物」，函数上方的文档注释却仍把「置 false」列为第 5 步、并宣称用 `AtomicBool` 实现。这是典型的翻译残留：C 的步骤清单被原样留在 Rust 函数的 doc comment 里，而实现早已删掉。读者按注释找 `AtomicBool` 会落空；Doc Code Sync 也应把这条算作代码注释与设计陈述不同步。

**建议**：删除 `lib.rs:3701` 与 `:3706` 两行（或改写为「C 的 kernel_may_alloc 已随 D01-F08 淘汰，本函数不再有第 5 步」）。这是纯注释删除，无行为变化。

## FINDING RD3B-DESIGN-D2-04（P2，[MODERN]）

**文档位置**：

- §4.2，文档第 977 行节选首行：`let prot = CurrentProtection::init(0, kernel_info.kern_stack_top);`
- §3.2 映射表，文档第 801 行：「GS 区/入口基址编程（BSP） | `ProtectionArch::program_entry_base`」。

**代码事实**：

- `os/kernel/src/lib.rs:2230` 传的是字面量 `0`。
- 同一函数在 `:2249-2254` 又从平台拓扑算出 `bsp_logical`，并用它调 `program_entry_base`：

  ```rust
  let topo = minix_platform::platform_desc().cpu_topology();
  let bsp_hw = elected_bsp_hw_id(&topo);
  let bsp_logical = (0..topo.nr_cpus as usize)
      .find(|&i| topo.cpus[i].hw_id == bsp_hw)
      .unwrap_or(0) as u32;
  with_protection(|prot| prot.program_entry_base(bsp_logical, kernel_info.kern_stack_top));
  ```

- `os/arch/src/x86_64/protection.rs:187-189` 的字段注释自己写着：`load()` 用 `boot_cpu_id`「instead of hardcoding BSP (cpu 0)」。调用点恰好把它硬编码成 0。
- `os/kernel/src/smp.rs:1150-1163` 的 AP 循环按 `bsp_logical` 跳过 BSP、对**其余所有** logical id 调 `set_kernel_stack(logical, ap_stack)`。`set_kernel_stack` 会写该槽的 `sp0` 与 CPU-id 戳记（`protection.rs:388-394`）。

**可达的非法状态**：若「实际执行引导的 CPU」在拓扑里的 logical id 不是 0（`elected_bsp_hw_id` 的注释明说固件的 `bsp_id` 只是第一条启用的 MADT 记录，ACPI 不要求它等于引导 CPU；x86 用 CPUID APIC ID 时也可能命中非 0 下标），则：

1. `init(0, ...)` 把 TSS 槽 0 当 BSP 槽，`load()` 的 `ltr` 指向槽 0（`protection.rs:404-406` 用 `boot_cpu_id=0`）；
2. `program_entry_base(bsp_logical, ...)` 却给另一个 logical id 写 GS 区；
3. AP 循环跳过 `bsp_logical`，因此 `logical=0` 被当作 AP，`set_kernel_stack(0, ap_stack)` 覆盖槽 0 的 `sp0` 与 CPU-id 戳记；
4. BSP 的第一次 ring3→ring0 仍按 TR 指向的槽 0 取 `sp0`——此时它已经是某颗 AP 的栈。GS 区的 CPU 身份（`gs:0x10`）与 TSS 槽的身份从此不一致。

这不是纯理论：`smp.rs` 的循环与 `elected_bsp_hw_id` 的存在本身就承认 `bsp_logical != 0` 是合法输入；只是 QEMU/CI 的 MADT 顺序通常让 BSP 落在 0，所以测试面看不见。

**建议**：让 BSP 身份只有一个来源。最直接的改法是在 `init_protection` 开头先算 `bsp_logical`（把 `:2249-2253` 的选举块上移），把 `CurrentProtection::init(bsp_logical, ...)` 传进去；`X86_64Protection::load()` 已经用 `boot_cpu_id`，无需改语义。aarch64/riscv64 的 `init` 只把 `cpu_id` 用于 `cpu_count`（生产无读者），传 `bsp_logical` 无行为副作用。文档 §4.2 的节选同步改为 `init(bsp_logical, ...)`，并把「BSP 的 logical id 来自选举」写进正文，而不是只留在代码注释里。

## FINDING RD3B-DESIGN-D2-05（P3，[DEL]，SUGGESTION）

**文档位置**：

- §3.2，文档第 787 行：「当前 OS 层没有特权级的运行期消费者，`Privilege` 类型仅测试/接口预留」。
- §3.3，文档第 838 行结构体骨架里的 `cpu_count: u32`。

**代码事实**：

- `Privilege`、`PrivilegeLevel` 关联类型、`KERNEL_PRIVILEGE`/`USER_PRIVILEGE`、`to_privilege`/`from_privilege`：全仓无运行期消费者。除三份架构 impl 与 `runtime-window` 的 `MockProtection` 外，只有各 `protection.rs` 的 `privilege_level_roundtrip` 测试与一个 arch 测试的文本 pin 使用它们。
- `X86_64Protection::cpu_count`（`protection.rs:186`）只在 `init` 里写（`:383`），唯一读点是 `init_sets_cpu_count` 测试（`:729-731`）；`AArch64Protection`/`Riscv64Protection` 的同名字段同样只被 `protection_has_cpu_count` 测试直接构造后读。

**差异与影响**：两处都是文档已经选中的形状，但生产路径没有读者。`ProtectionArch` 的 trait 文档仍把「Privilege level representation」列为「What is abstracted (the 5 OS-level concerns)」的第一条，读者会以为 OS 层真的在用 `Privilege`；文档 §3.2 的披露只在本篇可见，trait 自身没有同等说明。`cpu_count` 则连这层披露都没有。

**建议**（P3，可延后）：二选一。要么删除 `cpu_count` 与 `PrivilegeLevel` 关联类型/常量/转换函数，把「kernel vs user」留给将来真正需要跨架构特权级的调用点；要么保留但在 trait 文档里标成「test-only interface reservation」并加一条 arch 测试式的存在性 pin。当前「文档说预留、trait 文档说这是五大抽象之一」的两种口径应统一。

## FINDING RD3B-DESIGN-D2-06（P2，[DEL]）

**文档位置**：

- §3.7，文档第 930 行：「`TrapEntryArch::init()` 写的元数据在生产路径上被这次重写覆盖；……当前实现存在两个 DPL 来源（init 的表与 `user_accessible`），以 `set_handler` 为准，元数据是待清理的死数据。」
- §5.1，文档第 1225 行：「13 个 trap 测试覆盖门表：异常区存在性、保留向量非存在、软中断 DPL=3、PIC 区 DPL=0、NMI/#DF 的 IST。」
- §5.4 M1，文档第 1267 行：门字段与分布「已覆盖」。

**代码事实**：

- `X86_64TrapEntry::init`（`os/arch/src/x86_64/trap_entry.rs:197-261`）写 41 条门的 DPL、IST 与 `is_trap`。
- `install_idt_handlers`（`os/arch/src/x86_64/trap_stub.rs:613-633`）对 `STUB_VECTORS` 的**同一 41 个向量**逐条调 `set_handler`；`set_handler`（`trap_entry.rs:333-341`）把门整体重写为 `dpl = user_accessible(vector)`、`ist = 0`、`is_trap = false`，随后只对向量 2/8 用 `set_gate_ist` 恢复 IST。
- 13 条 trap 测试的分布是：9 条 `idt_init_*` 直接调 `X86_64TrapEntry::init()` 后断言 init 写下的门表；2 条 `set_handler_*` 只验证 `set_handler` 对单个向量的 DPL/地址写入；2 条 `idt_entry64_size_is_16_bytes`/`idt_ptr_size_is_10_bytes` 是布局测试。没有一条在 `install_idt_handlers` 之后断言最终 41 条门。唯一覆盖安装器的测试 `test_stub_coverage_matches_idt_gates`（`trap_stub.rs:766-778`）只比较 present 与 `has_stub`，不比较 DPL/IST/门类型。
- `user_accessible`（`trap_stub.rs:240-242`）的向量集与 `set_gate_ist` 的恢复结果因此没有任何自动断言；若 `user_accessible` 漂移，或 `set_gate_ist` 漏恢复一条，现有宿主测试不会红。

**差异与影响**：文档同时说了两句不能同时为真的覆盖结论。若 §3.7 为真（init 元数据是死数据），则 §5.1/§5.4 M1 的「已覆盖」覆盖的是死数据，真正生效的 `user_accessible` + IST 恢复路径是未覆盖的；若 §5.1 为真（13 条测试是门表的验收），则 init 元数据不是死数据，生产路径不应整条重写它。两种读法指向不同的修法，读者无法从文档判断该删还是该留。

**建议**：先裁决「init 元数据是死还是活」，裁决要连带载体一起看。若选「死」（符合 §3.7 的现状）：删掉 `init` 里的 DPL/IST/`is_trap` 表，但必须同时处理两个依赖——`gate_present` 不能再从 init 的门派生（可改由 `STUB_VECTORS` 派生，代价是 `test_stub_coverage_matches_idt_gates` 变成同义反复，需要换成"安装后 present"的断言）；x86 `test-protection` 载体不调 `install_trap_stubs`，删表后它装载的是非 present 门，载体注释要写明这一状态。若选「活」：让生产路径不再用 `set_handler` 覆盖元数据，或者把 init 的元数据表变成 `set_handler` 的单一来源，并补一条 post-install 断言（向量 2/8 的 IST=1/2、3/4/32-35 的 DPL=3、其余 DPL=0）。无论哪种，都应把「post-install 门元数据断言」补进 §5.3（可挂在 R03-T05 下），并在 §5.4 M1 的「已覆盖」后注明覆盖的是 init 侧还是 install 侧。

## FINDING RD3B-DESIGN-D2-07（P3，[DEL]，SUGGESTION）

**文档位置**：§4.1，文档第 965 行：

> 「内核镜像的保留由 `vm_handoff::classify` 在构造 VM 交接记录时用 `cut_memmap` 扣除并记录，`init_proc_and_boot` 与 Phase F 都没有 `add_memmap` 调用。」

**代码事实**：

- 文档说 `vm_handoff::classify` 负责扣除；`grep -rn "fn cut_memmap" --include=*.rs os --exclude-dir=target` 无输出，说明没有名为 `cut_memmap` 的函数。扣除的实际写法是 `FreeRangeSet` 的 `cut`（`vm_handoff::classify` 的 free-list 路径），不是 C 的同名函数。
- `grep -rn "fn add_memmap" --include=*.rs os --exclude-dir=target` 无输出；`grep -rn "add_memmap"` 的命中全部是注释，没有函数定义或调用。
- 但 `os/kernel/src/lib.rs:2827`（`init_proc_and_boot` 内）仍写着：「After the ELF is loaded, the VM module's physical memory is reclaimed via `add_memmap` (undoes the `cut_memmap` from Phase A.2), matching C: protect.c:450-451.」——这与同一个文件 `:1741-1751` 的「Phase A.2 is intentionally empty」以及 `:1858-1863` 的「nothing to reclaim」直接冲突。

**差异与影响**：`cut_memmap`/`add_memmap` 是 C 的函数名，在 Rust 里只剩 `FreeRangeSet::cut` 与扣除记录。文档用「用 `cut_memmap` 扣除」会把读者引向一个不存在的函数；`lib.rs:2827` 的「reclaimed via `add_memmap`」更是把一条已经退役的 C 回收路径写成了现行行为，且它声称「undoes the `cut_memmap` from Phase A.2」，而 Phase A.2 根本没有 cut。

**建议**（P3）：文档改为「`classify` 用 `FreeRangeSet::cut` 把内核镜像等 live-bootstrap 区间从 free list 扣除，并由 `DeductionCollector` 记录」；`lib.rs:2827` 的注释改成「VM 模块的内存由 `vm_handoff::classify` 的扣除记录排除，不在 free list 里，因此没有对应的 add 回调」。这是命名与注释清理，不改行为。

## FINDING RD3B-DESIGN-D2-08（P3，[MODERN]，SUGGESTION）

**文档位置**：§4.1，文档第 960-963 行：

```rust
/* 节选（保留入口取值；诊断输出块与说明注释省略），os/kernel/src/lib.rs:fn kmain。 */
    let kernel_info: &KernelInfo = crate::kernel_info()
        .expect("kmain: KERNEL_INFO must be stored by arch_boot before jump");
```

**代码事实**：`os/kernel/src/lib.rs:1687-1688` 的源码是：

```rust
    let kernel_info: &KernelInfo =
        crate::kernel_info().expect("kmain: KERNEL_INFO must be stored by arch_boot before jump");
```

**差异与影响**：词句完全相同，但文档把源码的换行位置改了（`crate::kernel_info()` 从第二行提到第一行）。D-045 的冻结裁决是「节选引文只许省略不许改写」。这类行重排不会改变语义，但会让「逐字引用」的承诺失真；本篇第 3/4 章其余节选（E1 注释、GsArea、Phase 清单、GDT anchor、install_idt_handlers、configure_syscall）经逐字比对都没有这个问题，只有这一处。

**建议**（P3）：恢复源码的两行形态，或把该块改成「示意」并去掉「代码引用」的口径。修复成本一行。

## FINDING RD3B-DESIGN-D2-09（P3，[MODERN]，SUGGESTION）

**文档位置**：§4.6，文档第 1172 行：

> 「**aarch64 的 `init` 直接 `msr SP_EL1, kernel_stack_top` 会怎样**：`SP_EL1` 在 EL1 可访问（HCR_EL2.TSP=0）；真正的问题是它把当前活动栈指针换掉……」

**代码事实**（同一载体文件内部就矛盾）：

- `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs:9-12` 的文件头写着：「NOTE: UEFI firmware sets HCR_EL2.TSP=1, which traps all SP_EL1 accesses (MRS/MSR) from EL1 to EL2. This means we cannot directly set or read SP_EL1 in the UEFI test environment.」
- 同文件 `:368` 又写：「NOTE: SP_EL1 is fully accessible from EL1 (HCR_EL2.TSP=0 confirmed).」
- 探针 `test_sp_el1_access`（`:141-162`）在 `SPSel=0` 窗口里只做一条 `mrs x0, SP_EL1`；调用点（`:289-306`）把 `sp_readback` 与 `trap_flag` **只打印、不判定**。即使 trap_flag=1，测试仍然 PASS。

**差异与影响**：文档把「TSP=0」当成事实写进反事实的前提，但所引用的载体自己给不出这个事实：文件头说 TSP=1 全 trap，后文说 TSP=0 confirmed，探针则对两种结果都放行。文档的反事实结论（真正的问题是换掉活动 SP，而不是 UNDEFINED）有 `protection.rs` 的独立注释支持，不依赖 TSP=0；出问题的是「可访问」这个前提被写成了已验证的事实。

**建议**（P3）：把「`SP_EL1` 在 EL1 可访问（HCR_EL2.TSP=0）」降级为「在 QEMU/UEFI 载体上 `SP_EL1` 的访问是否被 EL2 陷获取决于固件；`test-protection-aarch64` 的探针是 trap-tolerant 的，只打印 trap_flag，不作为判据」，并在载体侧二选一：修掉文件头的 TSP=1 陈述，或给探针加断言（例如 `trap_flag == 0` 且 readback 等于预期）后再声称 TSP=0。若两者都做不到，§5.2/§5.4 M7 不应把「SP_EL1 只读」列为已覆盖的寄存器判据。

## FINDING RD3B-DESIGN-D2-10（P3，[MODERN]，SUGGESTION）

**文档位置**：§4.7 末段，文档第 1200 行：

> 「本篇做完之后，系统处在这样一个状态：三张表都已装载生效、门里有真入口（§3.7），而**尚未放行任何中断源**（IF=0 一直保持；`intr.init()` 已编程 LAPIC/IOAPIC，时钟门在 Phase F 放行）；进程与用户态都还没有。」

**代码事实**：`os/kernel/src/lib.rs:1784-1785` 的 kmain 顺序是：

```rust
    init_protection(kernel_info); // prot_init equivalent
    init_clock_and_interrupts(); // clock + intr + arch_init (covered in 05)
```

`intr.init()` 在 `init_clock_and_interrupts` 内部（`:2498`），而本篇 §3.6 的 Phase 表明确把「B（时钟与中断控制器）」划给 `05-clock-interrupt-init.md`、本篇「不展开」。因此在 `init_protection` 返回的时刻，`intr.init()` **尚未执行**；「已编程 LAPIC/IOAPIC」是 Phase B 后半段完成后的状态。

**差异与影响**：这句话把「本篇实现的终点」与「整个 Phase B 的终点」混在一起。读者若按本篇的代码顺序在 `init_protection` 之后读寄存器，会发现控制器还没编程；若按句子去等「已编程」，又要跨到 05 篇。状态描述应把两个终点分开。

**建议**（P3）：改成「本篇的 `init_protection` 结束时：三张表已装载、门里有真入口、IF=0、中断控制器尚未编程；Phase B 的 `init_clock_and_interrupts` 完成 `intr.init()` 后全部中断线仍是屏蔽的，时钟门要到 Phase F 才放行」。

## 决策对账

- 不重开 D-001（四章教学骨架不是优化对象）：本轮 F01–F10 全部落在陈述、注释、验证或局部身份来源，没有一条要求重组四章骨架。
- 不重开 D-032/D-033/D-040/D-042（BootWindows/CheckedWindows 系列）：这些落点不在第 3/4 章的 21 文件映射内，本轮未触碰。
- D-045（节选只许省略不许改写）被 F08 引用；这是本轮唯一一处引文行重排，且不涉及语义。
- D-046 的「读者 P3 分流」用于 F05、F07–F10：它们都标 P3/SUGGESTION，不进入阻断集。
- D-047（`[ARCH]` 硬阻断只认判决账行）：本轮无 ARCH 提案，F04 的身份收敛按普通 P2 修复处理，不冒充架构演进。
- D-048/D-049/D-050–D-052 与 D03 第 3/4 章的设计陈述无交集，不需要在本轮销账或延期。

## ARCH-PROPOSAL

本轮无 `[ARCH]` 级提案。四条 P2 都是当前架构内的陈述修正、注释删除或身份来源收敛，不改变 Minix3 外部语义，也不满足 D-047 的「判决账行」条件；不需要三处一致标注。若用户希望把「BSP 身份单一来源」（F04）升级为契约变更，可另开 `[ARCH]` 议题，但那属于代码修复的边界，不是本轮设计陈述的一部分。

## VERDICT

**NOT PASS**：1×P1 + 4×P2 + 5×P3。

- 阻断项：F01（`bootstrap_start/len` 已不存在却写成 `(0,0)`）、F02（Rust 侧没有 IOAPIC 门增量）、F03（`kernel_may_alloc` 的 doc comment 与设计陈述相反）、F04（BSP 身份 0 与 `bsp_logical` 双源，可达非法状态）、F06（「死元数据」与「已覆盖」不能同时成立）。
- 非阻断建议：F05（`cpu_count`/`Privilege` 死抽象）、F07（`cut_memmap`/`add_memmap` 命名残留）、F08（D-045 节选行重排）、F09（TSP=0 证据冲突）、F10（篇末状态跨了 Phase B 边界）。
- 设计骨架本身成立：§3 的 GDT/TSS/IDT 去留、两 trait 拆分、静态地址契约、SYSCALL 路径替换与 §4 的 `init_protection` 次序，与 21 个映射文件的实现逐项对得上；问题集中在事实句、C→Rust 增量句、一处死注释与一处身份双源。修完 F01–F04、F06 后建议只对改动段落做 R3 复验，不需要重跑整篇。
