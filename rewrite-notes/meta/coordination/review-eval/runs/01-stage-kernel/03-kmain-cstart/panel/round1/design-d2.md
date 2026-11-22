# 设计差异审查 D2 — 03-kmain-cstart（P-DESIGN）

只读范围：文档 §3/§4、`os/arch/src/x86_64/{protection,trap_entry,trap_stub}.rs`、`os/arch/src/arch/{protection,trap_entry}.rs`、`os/kernel/src/lib.rs` 的 `init_protection` 段、`AGENTS.md` Key Constraints。未改仓库。

## DIFF-TABLE

| # | 维度 | 文档说法 | 代码事实（位置） | 判定 |
|---|---|---|---|---|
| A | GDT 单写者与槽位所有权 | §3.1:907-916 槽表（0 空/1 KCS/2 KDS/3 anchor/4 UDS/5 UCS/6 起每 CPU 两槽，长 70） | `protection.rs:38-63` 常量逐槽对上；`fill_flat_segments`（:190-224）只写 0–5，`setup_tss_for_cpu`（:267-269）只写本 CPU 的 `6+i*2` 两槽，`refresh_tss_descriptors`（:277-285）重写同一批 TSS 槽——写者按槽号分区，无重叠 | 一致 |
| B | TSS 内容写者 / 描述符刷新 | §3.3:953-975「搬进全局后重建描述符再 `ltr`」；§4.3:1231 保留区 16 字节 | `init`（`protection.rs:333-358`）循环建全部槽的描述符（此时实例是栈局部），`set_kernel_stack`（:360-366）只改 `sp0`+CPU 戳记，`refresh_tss_descriptors`（:277-285）按当前地址重建全部槽；`lib.rs:2395-2406` 顺序 store→refresh→load | 一致（搬迁-重建契约双端到位） |
| C | IDT 占位者 / 单写者 | §3.7:1076、§4.4:1259-1275「`init` 只建 256 条不存在占位项，`set_handler` 唯一写者」 | `init`（`trap_entry.rs:188-198`）建 `[IdtEntry64::empty(); 256]`；唯一 IDT 写点是私有 `set_gate`（:120-122），只被 `set_handler`（:286）调用；`gate_present`/`gate_metadata`（:131-145）只读 | 一致（全仓 `idt[` 写点仅 :121） |
| D | IST1/IST2 映射与落栈 | §4.4:1321「2→IST1、8→IST2，其余 0；硬件按 `TSS.ist[ist-1]`；`setup_tss_for_cpu` 只填两格」 | `set_handler`（`trap_entry.rs:281-285`）match 与文档同；`setup_tss_for_cpu`（`protection.rs:255-256`）只填 `ist[0]`/`ist[1]`；`IST_STACKS[2][MAX_CPUS]` 每栈 8 KiB（:320-330） | 一致 |
| E | 三架构状态载体 | §3.3:953「只有 x86-64 把表装进 static」；§4.6/§4.7:1408「aarch64/riscv64 无状态、accessor 造哑实例」 | x86：`X86_64Protection{gdt,tss,boot_cpu_id}` + `static PROTECTION: SyncUnsafeCell<Option<..>>`（`lib.rs:2445-2448`）；arm64/riscv64 四个类型全是 ZST（`arm64/protection.rs:56`、`arm64/trap_entry.rs:75`、`riscv64/protection.rs:89`、`riscv64/trap_entry.rs:81`），非 x86 `store_protection` 丢弃实例、`with_protection` 用静态哑实例（`lib.rs:2505-2520`） | 一致（无非法状态） |
| F | `init_protection` 顺序不变量 | §4.2:1187 五条次序；§3.6:1072 保护结构先于控制器 | `lib.rs:2395-2412` init→store→refresh（cfg x86）→load→program_entry_base；`:2421-2440` trap init→install stubs→configure_syscall→register→store→load；`os/kernel/tests/init_protection_order_pin.rs:58-67` 钉住 | 一致 |
| G | 抽象边界（trait 契约文字） | §3.2:947 引「init+load 先于 TrapEntryArch::init」；§3.7 单写者 | `arch/protection.rs:79-82` 命中；但 `arch/trap_entry.rs:115-118` 仍写 `init` “Fills the table with handler addresses … C: idt_init()”，`x86_64/trap_entry.rs:39` 模块表仍写 `init` 填 `gate_table_exceptions[]`/`gate_table_pic[]` | 不一致（FINDING 2） |

## FINDING

1. **[DEL] P0-fact：文档引用了代码里已不存在的注释原文。** §3.1:888 与 §4.7:1406 两处逐字写“`load_with_tss` 尾注的 "LDT descriptor is filled in the GDT" 是旧方案残留”。`grep -rn "filled in the GDT" --include=*.rs os/` 无输出；当前尾注（`os/arch/src/x86_64/protection.rs:530-534`）说的是相反内容：“C loads LDTR via lldt(LDT_SELECTOR), but 64-bit mode does not use LDT. The Rust GDT has no LDT descriptor at all (slots 3/5 are the user-CS anchor/copy), so there is no lldt to issue here.” 文档把一条已被改掉的旧注释当成现存证据继续引用，锚点失效。改法：删掉该括注，或改写为“尾注已说明不执行 `lldt`”。

2. **[DEL] P1-design-deviation：trait 契约文字仍描述被淘汰的“init 填门”模型。** `os/arch/src/arch/trap_entry.rs:115-118` 的方法文档写 `init` “Fills the table with handler addresses for CPU exceptions, hardware interrupts, and system call vectors”，`os/arch/src/x86_64/trap_entry.rs:39` 的模块映射表写 `init` 填 `gate_table_exceptions[]`/`gate_table_pic[]`。这与 §3.7:1076/§4.4:1259 的“`init` 只建空占位、`set_handler` 唯一写者”以及实际实现（`trap_entry.rs:188-198`、`:120-122`、`:286`）相反。§3.2 把两个 trait 的立论建立在“顺序依赖写成接口契约”上，契约正文与实现相反，等于把 C 的 `idt_init` 语义残留在抽象层。改法：把这两处改为“init 只建不存在占位，handler 由 `set_handler` 安装”。

3. **[DEL] P1：§4.4 段落重复。** 文档 :1321 与 :1322 是几乎逐字相同的两段（同述“2→IST1、8→IST2、硬件取 `ist[ist-1]`、`setup_tss_for_cpu` 只填两格、错误码布尔只被 `test_errcode_classification_matches_sdm` 使用”）；第二段多出“仓内没有 `set_gate_ist`”。删除其一，保留含 `set_gate_ist` 否定陈述的那段（它承担单写者论证）。

## ARCH-PROPOSAL

1. **[ARCH] `refresh_descriptors` 的 OS 层 `#[cfg(target_arch = "x86_64")]` 让 trait 默认实现成为死抽象。** `os/kernel/src/lib.rs:2404-2405` 用 `#[cfg]` 包住唯一的 `refresh_descriptors()` 调用，而 `os/arch/src/arch/protection.rs:119-125` 为 aarch64/riscv64 提供 no-op 默认；非 x86 的 `store_protection` 直接丢弃实例（`lib.rs:2508-2511`），因此这两个实现永远不会被调用。对照同一函数里 `program_entry_base`（`lib.rs:2412`）是**不带 cfg** 走 trait 调用的——同一个“搬迁后收尾”生命周期在 OS 层用了两种写法，正是 `AGENTS.md`“抽象语义、不描述机制”要消除的形态。提案：把 `refresh_descriptors` 收敛为一个 trait 收尾方法（如 `fn commit(&mut self)`，语义为“实例已就位，按最终地址定稿”），OS 层无条件调用；x86 重建 TSS 描述符，aarch64/riscv64 默认 no-op。这样删掉 OS 层的 arch 名字判断，默认实现重新可达，`[ARCH: ...]` 三处标注只需落在 trait 与文档（IST 落栈点的 `[ARCH: IST replaces i386 same-stack nesting]` 已在 `protection.rs:250-251` 与 `trap_stub.rs:620` 就位，不需再补）。

## SUBTRACTION-TABLE

| 可减项 | 位置 | 减掉后行为 | 风险 |
|---|---|---|---|
| `X86_64Protection::refresh_tss_descriptors` 的 pub 固有方法与 `ProtectionArch::refresh_descriptors` 的一行转发 | `os/arch/src/x86_64/protection.rs:277-285`、`:368-370` | 函数体并入 trait 实现；仓内唯一调用者是 `lib.rs:2405`，无其它消费者（`os/target/fmtchk/...` 是构建产物不是源码） | 低：x86 私有面缩小，trait 形状不变；与上条 ARCH 提案合并做更彻底 |
| §4.4:1321/:1322 的重复段 | 文档 | 删一段，信息量不减 | 无 |

## VERDICT

**NOT PASS。** 重点六问里 A（GDT 槽位分区）、B（TSS 描述符搬迁后刷新）、C（IDT 占位者 + 单写者）、D（IST1/IST2）、E（三架构 ZST/静态 accessor）、F（`init_protection` 顺序）与代码逐条对上，单写者声明经全仓写点核对成立（IDT 只有 `trap_entry.rs:121` 一个写点）。阻断项是文档与抽象层三处：1 条 P0-fact（引用不存在的 `"LDT descriptor is filled in the GDT"`，§3.1:888/§4.7:1406）+ 1 条 P1 契约文字与实现相反（`arch/trap_entry.rs:115-118`、`x86_64/trap_entry.rs:39`）+ 1 条 P1 重复段（:1321/:1322）；`refresh_descriptors` 的 cfg 泄漏列为 [ARCH] 提案，不阻断。修掉 1–3 后可复审。
