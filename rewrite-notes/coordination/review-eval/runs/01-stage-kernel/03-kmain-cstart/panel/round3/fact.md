# P-FACT 独立事实审查 · 03-kmain-cstart.md（round3）

- 审查对象：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`（1396 行）
- 源码基线：`minix3/`（C 真值）与 `os/`（Rust 实现），清单 `related-code.txt` 21 文件全部在册
- 方法：逐符号 grep + 逐引文块比对。引文块 25 处（代码引用与节选）全部与现行源码逐字核对；常量/行号/计数约 90 项现算核对；测试名 13 个逐一确认存在；反事实前提 21 组逐一核对前提对象是否真实存在
- 证据分级：E1 = 直接读源码/grep 输出比对；E2 = 结构与调用图推断；E3 = 证据不足

## 核对通过的主项（抽样列举，非全部）

- 全部 25 处引文块逐字一致，省略处均有标注（`kmain`/`cstart`/`prot_init`/`tss_init`/`prot_load_selectors`/`idt_init`/`idt_copy_vectors`/`int_gate`/`pg_identity`/`pg_load`/两份 `vir2phys`/earm `prot_init`/`assert(prot_init_done)`/E1 注释/`X86_64Protection`/`GsArea`/Phase 清单/`init_protection` 节选/`GDT_USER_ANCHOR_INDEX`/`setup_tss_for_cpu`/`load_with_tss`/`init` PIC 行/`install_idt_handlers`/`configure_syscall`/arm64 `init`/`set_entry_stack_base`/test-protection FAIL 两行）——E1
- 常量与行号：`archconst.h` 11/19/20/21/31/149 行、`CONFIG_MAX_CPUS=1`（kernel.h:8,51）、访问字节 0x9A/0x92/0xFA/0xF2、粒度 0xA(L=1)/0xC(B=1)、`X86_64_STACK_TOP_RESERVED=16`、`TSS64_SIZE=104`、`GDT_ENTRIES=22`、`IDT_ENTRIES=256`（trap_entry.rs:114）、`SFMASK_CLEAR_IF=0x200`、`iobase=0x8000`、`IRQ0_VECTOR=0x50`/`IRQ8_VECTOR=0x70`、软中断 32–35、APIC 0xF0(timer)/0xF1(sched)/0xF2(halt)/0xFE/0xFF——E1
- 计数：C 39 门（16+23）、USER_PRIVILEGE 行 6、`tss_init` 恰 5 个调用点（earm/arch_system.c:113、i386/protect.c:338、arch_smp.h:18、arch_smp.c:272、arch_system.c:256）、`prot_init` 恰两份定义、i386 protect.c `#if`=0、`STUB_VECTORS`=41、`gate_table_ioapic` 64+2、`gate_table_smp` 2（90/92 算式成立）、9 文件测试 16/13/11/2/1/3/2/1/7、riscv64 trap_stub 注释命中剔除后 55、trap_entry 13=9+2+2——E1
- 结构体：`tss_s` 27 字段、`gate_table_s`/`gatedesc_s`/`Tss64`/`IdtEntry64` 七字段/`KernelInfo` 七 slice/`GsArea` 三字段、earm `tss_s` 仅 `sp0`——E1
- 文档点名的 13 个测试函数全部存在；`test_bss_region_is_zeroed_after_load`（os/boot-shim/src/loader.rs）、`riscv64_percpu_trap_stack_pin.rs` 的"禁止共享标量"断言在册——E1
- 反事实前提真实存在：`pg_utils.c:170` 断言、mpx.S 栈布局（k_boot_stack→k_stacks_start 两页式）、`cpuid` 宏取边界下一字、`int_gate` 固定写 `KERN_CS_SELECTOR`、E1 注释、`make_tss_desc64` 高半、`init_ap` 的 "Load order matters"/gs:0x10 IVT 记录、arm64 `fn init` 修正口径、`mask_all` 三架构实现、`write_syscall_msrs` 注释与 smp.rs:1628 AP 尾调用、aarch64/riscv64 载体判据（CurrentEL/VBAR/DAIF 全屏蔽/stvec Direct/sscratch/sstatus）逐条在源码——E1/E2
- 锚点抽查：文中 `path:symbol` 形态锚点全部解析到真实定义（含 `lib.rs` 1858-1863/3916-3919 两处行段引证）——E1

---

## FINDING RD03-FACT-01（P3）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1235`（§5.1：「arch crate 全量 `src/**/*.rs` 是 373 个测试/54 文件，`os/arch/tests/*.rs` 另有 87 个/21 文件」）
- 严重度：P3
- 问题：全量两对数字只在「属性行计数」（剔除注释中 `#[test]` 字样）口径下成立；文档未对该口径作任何说明，而 §5.1 同段与 §6.2 给出的复算命令恰是 `grep -c '#\[test\]'`，按此原样复算会得到不同数字
- 影响：读者按文档自己的命令复算全量会得到 374（src）与 95（os/arch/tests），与文中 373/87 对不上 +1/+8，误判文档计数为错
- 证据或推理：E1。os/arch/src 原样子串命中 374，剔除 `os/arch/src/riscv64/trap_stub.rs:906` 一处注释提及后为 373，含测试的文件 54 个；os/arch/tests 原样子串命中 95，属性行 87（差值 8 处均为注释提及，如 `os/arch/tests/arch_gated_audit.rs:1,7,208,210`、`os/arch/tests/riscv64_return_leg_pin.rs:138`、两份 trap_leg_shape 头注），文件 21 个。9 文件子集一处（55 = 56−1）文档明说了剔除口径，全量两对数字未说；同段「按锚定 #[test] 去重后合计 55」的"去重"实为"剔除注释命中"，无去重动作
- 最小修复：在 §5.1 该句后补一个口径短语（如「口径：属性行计数，剔除注释中的 `#[test]` 字样；`grep -c` 原样复算 src=374、tests=95」），并把「去重」改为「剔除注释命中」
- 置信度：高
- 是否需源码核实：否（已核）
- 若我错了：若该口径在本文档之外的机制说明（如 review-evals 或前轮 scan.md）已定义且文档按约定省略，则本条降级为措辞建议，数字本身仍然正确

## FINDING RD03-FACT-02（P3）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:988`（§4.2 节选标题：「两处省略号分别为 `program_entry_base` 的 BSP 调用与 `register_current_arch` 的注册细节，段间说明注释按节选略去」）
- 严重度：P3
- 问题：两处省略号的标注与 `os/kernel/src/lib.rs:2217-2249` 实际省略内容不完全对应。第一处（`prot.load()` 与 `CurrentTrapEntry::init()` 之间）除 `program_entry_base` 调用外还省略了 4 行实代码（`platform_desc().cpu_topology()`、`elected_bsp_hw_id`、`bsp_logical` 查找），标注未提；第二处（`configure_syscall` 与 `trap_dispatch::register_current_arch();` 之间）省略的只是说明注释——注册调用行本身在省略号之后原样保留，称其为「注册细节」会让读者以为省掉了注册相关代码
- 影响：按节选复原函数实际内容时，第一处会缺 4 行拓扑计算；第二处会被误读为省略了注册调用
- 证据或推理：E1。`os/kernel/src/lib.rs:2239-2247` 为 topo/bsp_hw/bsp_logical/program_entry_base 四段；`lib.rs:2249-2256` 为 `trap.configure_syscall` 与 `register_current_arch` 之间的注释块（无代码行）
- 最小修复：标题改为「两处省略号分别为 program_entry_base 的 BSP 调用（含其 BSP 拓扑实参计算）与 register_current_arch 调用点前的说明注释」
- 置信度：高
- 是否需源码核实：否（已核）
- 若我错了：若「注册细节」被公认读作"注释所述的注册细节"，则只剩第一处的 4 行代码未点名，降级为措辞建议

---

## SUGGESTION RD03-SUG-01

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:483`、`:500`（§2.4 正文与反事实）
- 建议：两处 inline 代码写作 `*((reg_t *)(t->sp0 + sizeof(reg_t))) = cpu` / 「CPU 号写在 `sp0 + sizeof(reg_t)`」，源码为 `t->sp0 + 1 * sizeof(reg_t)`（`minix3/minix/kernel/arch/i386/protect.c:181`）。语义等价（`1 *` 化简），但本文档其余 inline 代码均逐字照抄，此处宜照抄或注明化简
- 证据：E1
- 置信度：高（差异存在）；是否需源码核实：否

## SUGGESTION RD03-SUG-02

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:792`（§3.2：「切分轴是入口机制 vs 访问控制（与 `trap_entry.rs` 文件头的 trait 自述一致（`trap_entry.rs:7-21`））」）
- 建议：`os/arch/src/x86_64/trap_entry.rs:7-21` 的文件头只自述「IDT + SYSCALL MSR 两类入口机制对 OS 隐藏」，未提「访问控制」（DPL）这一半；现锚点只支撑切分轴的入口机制半边。建议锚点改到能同时体现两半的行段，或在正文注明该佐证仅覆盖入口机制一侧
- 证据：E1（`trap_entry.rs:7-14` 无 DPL/访问控制表述）
- 置信度：中；是否需源码核实：否

---

- 本次扫描：2 条 FINDING，2 条 SUGGESTION。
