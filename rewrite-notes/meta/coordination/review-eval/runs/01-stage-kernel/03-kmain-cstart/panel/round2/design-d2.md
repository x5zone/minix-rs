# D03 R2 设计差异审查（P-DESIGN D2）

## 审查范围与方法

- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md` §3（834–1096 行）与 §4（1099–1413 行）。
- 代码：`os/arch/src/x86_64/protection.rs`、`trap_entry.rs`、`trap_stub.rs`；`os/arch/src/arch/protection.rs`、`trap_entry.rs`；`os/kernel/src/lib.rs` 的 `init_protection` 段（2371–2525 行）。
- 只读；未读作者解释、round1 panel 与历史证据。R1 归因因材料禁用而不作断言，以下按现文与代码逐条对账。
- 判定口径：文档的设计断言必须能被代码或架构手册复核；引文必须逐字存在；同一事实不得有两个版本。

## FINDINGS

### F1 [MODERN] P0-fact — `KernelInfo.syscall_entry` 字段在代码中不存在

- 位置：文档 §3.4 第 998 行，表格行 `` `KernelInfo.syscall_entry` | —— | 只作元数据；`configure_syscall` 不读它 | 降级为参考信息 ``。
- 现文断言：Rust 侧 `KernelInfo` 有一个 `syscall_entry` 字段，作为“只作元数据”的参考信息存在。
- 代码事实：`os/libs/minix-boot/src/kernel_info.rs:20` 的 `pub struct KernelInfo` 没有 `syscall_entry` 字段；同文件 540–542 行明确写 “`syscall_entry` / `kern_stack_top` are not wire fields”，562–563 行写 “The syscall-entry placement check has one authority: `syscall_entry_va()`”。`os/kernel/src/lib.rs:2426` 那句 “KernelInfo.syscall_entry is reference-only boot metadata” 是残留注释，紧接的 2434 行实际调用的是 `syscall_entry_va()`。
- 后果：读者按表格去 `KernelInfo` 找字段会找不到；该行把“不存在”写成“只作元数据”，属设计事实错误。正确写法是“`KernelInfo` 无此字段；`LSTAR` 的唯一来源是 `syscall_entry_va()`，`KernelInfo` 不携带入口地址”。
- 建议：改写该行，并同步清掉 `lib.rs:2426` 的残留注释（代码侧另记，不属本文档修复范围）。

### F2 [MODERN] P1（锚点保真）— `gate_present` 的“引文”在仓库中不存在，且与同节引用的代码自相矛盾

- 位置：文档 §3.7 第 1095 行，引号内文字 “the stub installer cross-checks its coverage against this before the table may go live via `load()`”。
- 代码事实：`os/arch/src/x86_64/trap_entry.rs:124–131` 的实际注释是 “`install_idt_handlers` uses this after writing the 41 gates to assert that gate presence equals stub coverage; `load()` itself does not call it (the installer is the single writer and runs first).” 全仓检索不到文档引用的那句话（`grep -rn --exclude-dir=target 'cross-checks\|may go live' os/arch/src/x86_64/trap_entry.rs` 无命中）。
- 第二重矛盾：文档 1095 行紧接着说“执行这条交叉核对的是宿主测试 `test_stub_coverage_matches_idt_gates`”，但文档自己在 1283–1318 行引用的 `install_idt_handlers` 函数体（1311–1317 行）就包含 `for vector in 0..=u8::MAX { assert_eq!(entry.gate_present(vector), has_stub(vector), ...) }`。运行时安装器本身就是核对者，宿主测试是第二道；现文把唯一执行者写成宿主测试，与同节引文冲突。
- 建议：删除伪造引文，改为陈述实际注释的两点（安装器写完后断言覆盖；`load()` 不调用 `gate_present`），并把“执行者”改为“安装器 + 宿主测试”。

### F3 [DEL] P1 — “x86 GDT 无 LDT 描述符”把 Rust 布局事实误写成架构事实

- 位置：文档 §3.1 第 888 行与 §4.7 第 1406 行，两处均写 “长模式不使用 LDT（x86 GDT 无 LDT 描述符，`load_with_tss` 不执行 `lldt`）”。
- 代码事实：`os/arch/src/x86_64/protection.rs:532–536` 的原注释限定得很清楚：“The Rust GDT has no LDT descriptor at all (slots 3/5 are the user-CS anchor/copy), so there is no lldt to issue here.” 主谓是 “The Rust GDT”。
- 架构事实：长模式仍支持 LDT 描述符（系统段 type 0010）与 `LLDT` 指令；Minix3/Rust 只是不使用它。写成 “x86 GDT 无 LDT 描述符” 是过度概括，且两处重复。
- 建议：改为“本实现的 GDT 布局不含 LDT 描述符；Minix 的 64 位路径不使用 LDT（长模式仍提供 LDT/`LLDT`），故 `load_with_tss` 不执行 `lldt`”。

### F4 [DEL] P1（出版标准）— §4.4 同一段落连续出现两遍

- 位置：文档 §4.4 第 1321 行与第 1322 行。
- 事实：两段几乎逐字相同，均以 “`set_handler` 写门时/自己把 IST 一起写” 开头，内容都是“向量 2 写 1、向量 8 写 2、其余写 0；硬件按 `TSS.ist[ist-1]` 取值；`STUB_VECTORS` 的布尔值只被宿主测试使用”。第二段多一句“仓内没有 `set_gate_ist`”。
- 判定：重复内容属出版标准缺陷，形态符合修复插入残留；因禁读 round1 不作归因。
- 建议：保留含“仓内没有 `set_gate_ist`”的那一段，删除另一段。

### F5 [MODERN] P2 — 非 x86 的 “不进入全局 / accessor 造哑实例” 与代码形状不完全对应

- 位置：文档 §3.3 第 953 行 “aarch64/riscv64 的保护状态是寄存器，不进入全局”；§4.7 第 1408 行 “aarch64/riscv64：无状态实现（accessor 造哑实例）”。
- 代码事实：`os/kernel/src/lib.rs:2517` 在非 x86 定义了 `static PROTECTION: CurrentProtection = CurrentProtection::new();`，`with_protection` 返回 `&PROTECTION`；而 `with_trap_entry`（2523–2524 行）确实是每次 `CurrentTrapEntry::init()` 造哑实例。`AArch64Protection`/`Riscv64Protection` 是 ZST（`os/arch/src/arm64/protection.rs:57`、`os/arch/src/riscv64/protection.rs:89`），`new()` 的注释也写明用途是给非 x86 accessor 返回一个 `'static` 实例。
- 判定：语义上“无状态”成立（ZST 不携带状态）；字面上“不进入全局”与真实存在的 static 不符，“accessor 造哑实例”只适用于 trap，不适用于 protection。
- 建议：改为“寄存器状态不进入全局；非 x86 只保留一个 ZST 静态占位（protection accessor 返回它），trap accessor 按调用造哑实例”。

## SUBTRACTION-TABLE

| # | 位置 | 动作 | 理由 |
|---|---|---|---|
| 1 | 文档 §4.4 第 1322 行（或 1321 行） | 删除重复段之一，保留含“仓内没有 `set_gate_ist`”的版本 | 同一事实两个版本，出版标准禁止重复；F4 |
| 2 | 文档 §3.4 第 998 行 | 删除 `KernelInfo.syscall_entry` 行或改为“无此字段，来源为 `syscall_entry_va()`” | 字段不存在，F1 |
| 3 | 文档 §3.7 第 1095 行 | 删除伪造引文，改为实际注释语义 | 引文在仓库不存在，F2 |
| 4 | 文档 §3.1 第 888 行、§4.7 第 1406 行 | 把 “x86 GDT 无 LDT 描述符” 收窄为 “本实现的 GDT 布局” | 架构事实错误，F3 |

## CONSISTENCY CHECKS（任务点逐项）

| 检查项 | 结果 | 证据 |
|---|---|---|
| GDT 槽位与表长 | PASS | `protection.rs:52–79`：anchor=3、user DS=4、user CS=5、TSS 起始=6、每 CPU 2 槽；`GDT_ENTRIES = 6 + 32*2 = 70`（`MAX_CPUS=32`，`os/libs/minix-types/src/types/com.rs:51`），与 §3.1 表一致 |
| GDT 访问字节/粒度 | PASS | `protection.rs:99–102, 120–122`：0x9A/0x92/0xFA/0xF2；代码段 G=1|L=1，数据段 G=1|B=1，与 §4.3 一致 |
| TSS 布局与写入 | PASS | `protection.rs:124–153, 226–273`：104 字节、`ist[7]`、`iobase=0x8000`、`sp1/sp2` 零值；`sp0 = stack_top - 16`；描述符高低半都写进 `gdt[idx]/gdt[idx+1]`，与 §3.1/§4.3 一致 |
| TSS 描述符重建 | PASS | `refresh_tss_descriptors` 遍历全部 32 槽按搬移后地址重建（`protection.rs:276–287`）；`lib.rs:2404–2406` 在 `load` 前调用，与 §3.3/§4.2 一致 |
| IDT 单写者与 41 门 | PASS（引文问题见 F2） | `trap_entry.rs:270–287` 的 `set_handler` 写 handler/DPL/IST/type；`trap_stub.rs:168–216` 共 41 项；`install_idt_handlers` 写门后断言 DPL/IST/type 与覆盖（613–648 行），与 §3.7/§4.4 一致 |
| IST 1/2 | PASS | 门侧 2→1、8→2（`trap_entry.rs:281–285`）；TSS 侧只填 `ist[0]/ist[1]`（`protection.rs:258–259`）；`IST_STACKS[2][MAX_CPUS]` 每栈 8 KiB（`protection.rs:321–330`），与 §3.1/§4.4 一致 |
| ZST/static accessor | 实质一致，措辞见 F5 | 非 x86 protection 为 ZST + `static PROTECTION`（`lib.rs:2517`）；trap accessor 每次造哑实例（`lib.rs:2523–2524`）；x86 用 `Option` + `expect` panic 契约（`lib.rs:2473–2503`），与 §4.7 主体一致 |
| `init_protection` 顺序 | PASS | `lib.rs:2395–2440`：`init → store_protection → refresh_descriptors(x86) → load → program_entry_base → trap init → install_trap_stubs → configure_syscall → register_current_arch → store_trap_entry → trap load`，与 §4.2 节选及 §3.3/§3.7 的次序断言一致 |
| `configure_syscall` 四 MSR 次序 | PASS | `trap_entry.rs:220–235`：STAR → LSTAR → SFMASK → EFER.SCE，与 §3.4/§4.2 一致；STAR 位域（47:32 内核 CS、63:48 anchor）与 §4.5 一致 |
| GS 区与 `program_gs` | PASS | `trap_stub.rs:699–718` 写两个 GS_BASE MSR；`GsArea` 三字段与 §3.4 一致；SYSCALL 入口 `mov gs:{gs_user_rsp}, rsp` 在第 377 行（文档 §3.4 写 376 行，行号漂移一位，P2 附注，不单列 FINDING） |

## VERDICT

**NOT PASS**

- 阻断项：F1（P0-fact，虚构 `KernelInfo.syscall_entry`）与 F2（引文不存在且与同节引用的代码矛盾）必须修复；F3、F4 为 P1，必须修复。
- F5 为 P2 措辞，可与上述一并改，不单独阻断。
- 对“R1 修复是否引入设计事实错误”的回答：现文确实存在设计事实错误（F1）与锚点失真（F2），F4 具有修复插入残留的形态；但按本轮材料禁令未读 round1，无法确认哪一条由 R1 引入，只能确认它们存在于当前文本。
