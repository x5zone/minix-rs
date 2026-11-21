# 反证（确认轮）— 05-clock-interrupt-init

反证者立场：对文档核心主张逐条构造反例，全部核对针对 C 源（`minix3/`）与 Rust 源（`os/`）的现行代码，不采信文档自述。每条给出核对的命令与关键输出行。

## CLAIM RD05R2-CLAIM-01（次序主张：三行调用中唯一不能交换的是 arch_init）

**文档主张**（§1.1 表格、§2.1 结论段、§2.7 归纳 1）：`init_clock` 与 `intr_init` 无共享状态可交换；`arch_init` 必须在两者之后，依据有二——它读 `system_hz`（APIC 校准算式），且它重写 IDT 门。

**反证尝试与结果**：

1. 正向锚点全部核实成立：`system_hz` 别名宏（`grep -n 'define system_hz' minix3/minix/kernel/glo.h` → 行 38）；`init_clock` 是 `kclockinfo.hz` 唯一赋值处（`grep -rn 'kclockinfo.hz ='` → 仅 clock.c:58/60）；APIC 校准四式（apic.c:169 `PROBE_TICKS (system_hz/10)`、:491 `init_8253A_timer(system_hz)`、:517/:520 两式分母 `(PROBE_TICKS - 1)`）。
2. **反证命中一**：该依赖是**形态条件**的，而文档写成无条件。`minix3/minix/kernel/main.c` USE_APIC 块（实测 `sed -n '415,475p'`）：`value = env_get("no_apic"); if(value) config_no_apic = atoi(value); else config_no_apic = 1;` ——**缺省（不传 no_apic）时 config_no_apic = 1，APIC 关闭、走 legacy PIC**。legacy 形态下 `arch_init` 的六件事（k_stacks/tss_init/ser_init/acpi_init/cut_memmap×2，arch_system.c:246-279 逐行核）没有一件读 `system_hz`，也没有 IDT 重写（`apic_single_cpu_init` 被跳过）。即 C 缺省 boot 上"arch_init 不能前置"的两条依据均不生效；`-DUSE_APIC`（Makefile.inc:94-96）只保证代码编入，不保证分支走到。文档在 §2.1 知道 `config_no_apic` 开关存在，但未披露"缺省即关"的语义，§1.1 表格"必须在前两者之后：它读 system_hz，且自己重写 IDT 的门"是无条件表述。
3. **反证命中二**：§1.1 的三处污染清单在"交换次序"场景下不全部落地。交换后 `init_clock` 仍在 cstart 内、tick 源未编程、中断屏蔽——`load_update`（清单第 1 项）在该窗口内无任何执行路径，system_hz 到它执行时已是合法值；第 2 项 `init_local_timer` 的除零实际经由被污染后**持久化**的 `cpu_freq`（apic_calibrate_clocks 内 `cpu_set_freq`）间接落地，文档未写这条链；只有第 3 项（校准链）当场落地。清单把"hazard 世界"（hz 全零贯穿全程）与"交换场景"（hz 迟早有值）混在一张表里。

**分级**：非阻断。APIC 形态下的依赖真实存在且锚点齐全，主命题不被推翻；但"唯一不能交换的次序"缺少形态条件限定，污染清单的落地范围表述过宽。

## CLAIM RD05R2-CLAIM-02（intr_init 十次写与全屏蔽结束态）

**文档主张**（§2.3）：实际执行 10 次 `outb`（源码 12 行）；结束态主片 `0xFB`（留级联 IRQ2）、从片 `0xFF`；ICW1 三常量位差、ICW3 位图/编号不对称、ICW4 主从差 bit 2 注释相同；空链分发打印伪中断并保持屏蔽。

**反证尝试与结果**：逐项对源做差均失败，主张站住：

- `sed -n '1,60p' minix3/minix/kernel/arch/i386/i8259.c`：函数体 `outb(` 恰 12 行，两处 `if (auto_eoi)` 各带一条替代臂，执行路径 5+5=10；常量表与文档引文逐字一致（`ICW1_AT 0x11`、`ICW4_AT_MASTER 0x05`/`ICW4_AT_SLAVE 0x01` 且尾注同为 `not SFNM, not buffered, normal EOI, 8086`）。
- `interrupt.h:9-12,17-18,39,41`：`INT_CTL 0x20`/`INT_CTLMASK 0x21`/`INT2_CTL 0xA0`/`INT2_CTLMASK 0xA1`、`IRQ0_VECTOR 0x50`、`IRQ8_VECTOR 0x70`、`CLOCK_IRQ 0`、`CASCADE_IRQ 2`，`INT_CTLMASK` 注释确为 `setting bits in this port disables ints`。
- §6.2 命令 3 的"9 行"预期复核成立（ICW1_AT 2 行 + IRQ0_VECTOR 1 + CASCADE_IRQ 3 + ICW4_AT_MASTER/SLAVE 各 1 + ~0 1）。
- `kernel/interrupt.c` `irq_handle`：入口 `hw_intr_mask(irq)`，空链分支 `printf("irq_handle: spurious irq %d (count: %d); keeping masked\n")` 后直接 `return`——"打印并保持屏蔽"成立。

**附记（不构成反证）**：§2.2 称 init_clock"函数体只有三个动作"而逐行读列出 4 条（env_get 与范围回退分列），同章前后口径有小出入；不影响本条与该节事实。

**分级**：反证失败。

## CLAIM RD05R2-CLAIM-03（持久 tick 源不在 cstart：C/Rust 同位与三段）

**文档主张**（§2.6、§3.7、§4.7）：C 把持久 tick 源放在 `bsp_finish_booting` → `boot_cpu_init_timer`（编程/注册/放行三段），Rust 的 `boot_init_timer` 在 `bsp_finish_booting` Step 6（Phase F，进程表建立之后）同位承接。

**反证尝试与结果**：主张站住：

- C 侧：`clock.c:294-304` `boot_cpu_init_timer` 两个调用、失败即返 -1；`main.c:71-78` `cycles_accounting_init()` → `boot_cpu_init_timer(system_hz)`（panic 文案逐字一致）→ `fpu_init()`；`main.c:316/324` `bsp_finish_booting()` 在 kmain 末端。`bsp_register_timer_handler`（omap_timer.c）`put_irq_handler` 后 `bsp_irq_unmask` 且注释 `only unmask interrupts after registering` 逐字一致。
- Rust 侧：`os/kernel/src/lib.rs:3722` `boot_init_timer`，三段 `(a) init_timer(DEFAULT_HZ, cpu)` → `(b) register_hook(TIMER_IRQ, clock_irq_handler, …)` → `(c) enable_timer_irq()`，x86_64 尾部 `minix_plat::pic_init()`（:3802）；调用点 :3890 在 Phase F 的 `bsp_finish_booting` 内，而 `init_proc_and_boot`（:1821，Phase C）与 `init_post_and_memory`（:1848）先行——"进程表建立之后"成立；函数头注释自证 `C: boot_cpu_init_timer(system_hz) — clock.c:294`。
- 载体 `test-timer-irq/src/main.rs` 装配顺序与 kmain 同（init_clock_and_interrupts → init_proc_and_boot → init_smp_state → boot_init_timer → sti），与 §5.2 引文一致。

**失败路径省略（查得，非推翻）**：riscv64 `sbi_set_timer` 的 `assert!(error == 0, "sbi_set_timer failed: SBI error {error}")`（os/arch/src/riscv64/clock.rs:55）这条启动期 panic 路径在 §6.1 症状表与 §5.3 M 表均无行；同表也未收 aarch64 `rearm_next_tick` 的 `TICK_INTERVAL is 0 (wiring bug)` panic 与 x86_64 设备窗口 `device-window LAPIC page drifted` 断言（§4.1 引文里出现过但诊断表未列）。

**分级**：反证失败（失败路径登记为附注缺口）。

## CLAIM RD05R2-CLAIM-04（x86 `pic_init`：唯一 8259A 初始化点与 ICW4 差异登记）

**文档主张**（§4.7）：`pic_init` 重映射 master→0x50/slave→0x70、IMR 全屏蔽后 `unmask_irq(0)`+`unmask_irq(2)`；Rust 两片同写 `ICW4_8086 = 0x01`，与 C 主片 `0x05` 的差异"按现状登记"；从片 ICW2 用 0x70 不是 0x58。

**反证尝试与结果**：机制主张全部核实：`os/plat/src/x86_64/interrupt.rs` `pub fn pic_init` 函数体与文档代码逐字一致，`ICW4_8086: u8 = 0x01`（:64）、`IRQ8_VECTOR: u8 = 0x70`（:35）、测试钉 `assert!(pic_init_body.contains("outb(PIC2_DATA, IRQ8_VECTOR)"), "slave ICW2 must use IRQ8_VECTOR (0x70), not IRQ0_VECTOR+8")`；`boot_init_timer` 内 `#[cfg(target_arch = "x86_64")] minix_plat::pic_init();` 是唯一调用点。

**反证命中（引用保真）**：该段引文的标注写着"**注释按原文**"，但源注释是英文——`// master ICW2: vector base 0x50`、`// master ICW3: IRQ2 wired to the slave`、`// IMR: mask everything (IRQ0/IRQ2 are opened on demand by unmask_irq).`、`// cascade line`；文档全部写成中文（"master ICW2：向量基 0x50"、"IMR：全 mask（IRQ0/IRQ2 由 unmask_irq 按需打开）。"、"级联线"）。同篇对 `boot_init_timer` 的另一段引文明确标注"注释为中文转述、非源码逐字"——同一节内两种标注并存，"按原文"那份为假标注。

**分级**：非阻断（语义无失真、代码逐字成立；"注释按原文"的引用保真声明不实）。

## CLAIM RD05R2-CLAIM-05（控制器细节：0↔2 交换、TIMER_IRQ 0/30/0、注册即放行对位）

**文档主张**（§4.4、§4.8、§4.9）：`isa_irq_to_pin` 0↔2 互换且所有 RTE 访问必经之；`TIMER_IRQ` = 0/30/0；riscv64 对 0 提前返回；`set_irq_priority` 仅 riscv64 覆写；`register_hook_at_slot` 的 actids 清零 + 条件 unmask 与 C `put_irq_handler` 逐项对上；三架构 `mask_all` 口径不同（x86 为 `min(nr_irq_vectors, 16)`）。

**反证尝试与结果**：全部反证失败，主张站住：

- `os/plat/src/x86_64/interrupt.rs:376-381`：`0 => 2, 2 => 0` 精确成立；`ioapic_route_and_mask` 反演表 `0 => Some(2), 2 => Some(0)`，向量算式 `IRQ0_VECTOR + i` → 引脚 0 编程 0x52、引脚 2 编程 0x50，与 §4.4 推演 1、§6.1 行 9 一致。
- `TIMER_IRQ`：x86_64 `IrqVector::new(0)`（:45）、arm64 `new(30)`（:32）、riscv64 `new(0)`（:40）＋ lib.rs 三条 cfg 转发与 mock 兜底，共 7 行（§6.2 命令 8 的"七行"预期成立）。
- riscv64 `mask`/`unmask` 开头 `if irq_num == 0 { return; }` 逐字成立；`set_irq_priority` 仅 riscv64/interrupt.rs:178 有覆写。
- `register_hook_at_slot`（irq_manager.rs）尾部 `self.actids[irq_idx] &= !id; if self.actids[irq_idx] == 0 { self.controller.unmask(irq); }` 与 C `put_irq_handler` 的 `(irq_actids[hook->irq] &= ~hook->id) == 0 → hw_intr_used + hw_intr_unmask` 对位成立；id 分配循环（`while id != 0 && (bitmap & id) != 0 { id <<= 1; }`）与 C `for (id = 1; id != 0; id <<= 1) if (!(bitmap & id)) break;` 等价。
- `IrqManager` 五字段与 §3.4 引文逐字一致；x86 `mask_all` 为 `0..(self.nr_irq_vectors.min(16))` 且注释解释了为何不是 64。
- x86 `init_lapic` 的 SVR 写（`LAPIC_SPURIOUS_VECTOR | LAPIC_SVR_ENABLE`）与四项 LVT 全 mask 循环、aarch64 `GICR_SGI_FRAME = 0x1_0000` 及 `IGROUPR0/ISENABLER0/ICENABLER0` 偏移均与文档一致。

**分级**：反证失败。

## CLAIM RD05R2-CLAIM-06（DEFAULT_HZ 统一 100 与测试读数）

**文档主张**（§3.2、§5.1）：arch 层 `DEFAULT_HZ: u32 = 100` 单一权威、kernel re-export；`with_hz` 保留 `2..=50000` 守卫且无生产调用者；15 个文件的 `#[test]` 计数表与"46 跑 + 1 忽略"口径。

**反证尝试与结果**：

1. 主张主体站住：`os/arch/src/arch/clock.rs:43 pub const DEFAULT_HZ: u32 = 100`、`os/kernel/src/clock.rs:710 pub use minix_arch::arch::clock::DEFAULT_HZ`；`os/arch/Cargo.toml` 依赖表确无 `minix-kernel`（kernel :13 反向依赖 arch）——"分层方向"论据成立；`with_hz` 守卫 `!(2..=50000).contains(&hz)` 成立，调用点仅 clock.rs 测试三例（:1859/:1865/:1868），无生产调用者。
2. 测试读数：15 个文件 `grep -c '#\[test\]'` 与 §5.1 表**逐格相符**（47/10/3/4/0/5/3/4/3/0/0/3/4/3/5）；实跑核验：`cargo test -p minix-kernel --features mock --lib -- --list | grep -c '^clock'` = 47、`grep -c '^irq_manager'` = 10；实跑 `clock::` 前缀 = **67 passed + 1 ignored**，忽略项即 `clock::clock_irq_handler_tests::test_clock_hook_registered_in_manager`——"46 跑 + 1 忽略"精确成立。全套 `--lib` = 866 passed / 0 failed / 8 ignored（套件规模较 AGENTS.md 2026-10-12 的 831 有增长，但本文档未引用 831，无冲突）。
3. **反证命中（文档-代码漂移）**：§3.2 称 kernel 侧 re-export 的注释里"额外写了一句「需与 C 完全一致时可在 boot 阶段用 `with_hz(60)` 指定」"。现行 `os/kernel/src/clock.rs:703-710` 的注释是"single authoritative definition / former independent copy carried a 'avoid a cross-crate dependency' rationale"一段，**没有**该句；全仓检索该句只命中过期构建副本 `os/target/fmtchk/kernel_src_clock.rs:684`（目录时间戳 9 月 25 日）。文档引用了一句已不在现行源码里的注释。另：该句所在处的锚点写的是 `os/arch/src/arch/clock.rs:const DEFAULT_HZ`（arch 层文件）来指"kernel 层的副本"，锚点错位（括号内文字已纠正，但锚点仍指向错误文件）。

**分级**：非阻断（计数与守卫全部成立；注释引用漂移与锚点错位是同步缺陷）。

## 跨节复述与失败路径总检

- 复述一致性：§1.4 章末复述、§2.6、§4.7、§5.2 四处对"三段在 Phase F Step 6、Phase C 先行、唯一维护点"的表述互不冲突，且与 `lib.rs:1821/1848/1858/3890` 的实际次序一致；§2.6 限定段对"APIC 形态 cstart 内有 PIT 校准窗口"的修正与 apic.c:491-512、:541-542（`lapic_write(LAPIC_LVTTR, …)` 不带 mask 位）相符。
- 口径小冲突：§2.2"三个动作"vs 四条逐行读（并入 CLAIM-02 附记）；§3.2 锚点错位（并入 CLAIM-06）。
- 失败路径省略：`sbi_set_timer` 断言 panic、`rearm_next_tick` wiring-bug panic、x86 设备窗口 drift 断言三处启动期 panic 未入 §6.1/§5.3（并入 CLAIM-03 附注）；`register_hook` 的 NoFreeSlots expect 在 §4.7 引文中有、诊断表无行。
- 引用保真：§4.7 `pic_init` 引文标注"注释按原文"但注释为译文（CLAIM-04）；其余抽查引文（init_clock、intr_init、arch_init×2、omap 两函数、boot_cpu_init_timer、estimate_cpu_freq 节选、put_irq_handler 节选、三架构 init_timer、IrqManager 结构、riscv64 arch_init、载体节选）与源码逐字或按其自报口径一致。

## CLAIM 21

- 结果：非阻断——次序主张的 APIC 依赖成立（apic.c:169/491/517/520），但 C 缺省 boot `config_no_apic=1`（main.c USE_APIC 块）使该依赖成为形态条件而文档写成无条件；§1.1 污染清单在交换场景下仅校准一项当场落地（对应上文 RD05R2-CLAIM-01）。

## CLAIM 22

- 结果：反证失败——intr_init 十次写/12 行源码/0xFB+0xFF 结束态/三条常量结论/空链分发行为全部与 i8259.c、interrupt.h、interrupt.c 现源相符（对应上文 RD05R2-CLAIM-02）。

## CLAIM 23

- 结果：反证失败——持久 tick 源的 C/Rust 同位与三段结构全链核实（clock.c:294、main.c:73、lib.rs:3722/3890、Phase C :1821 先行）；附注三条启动期 panic 路径未入诊断表（对应上文 RD05R2-CLAIM-03）。

## CLAIM 24

- 结果：非阻断——pic_init 机制与 ICW4 差异登记成立，但引文标注"注释按原文"而注释实为中文译文，引用保真声明不实（对应上文 RD05R2-CLAIM-04）。

## CLAIM 25

- 结果：反证失败——isa_irq_to_pin 0↔2、TIMER_IRQ 0/30/0、riscv64 零号提前返回、set_irq_priority 单架构覆写、register_hook 放行对位、三架构 mask_all 口径全部与现源一致（对应上文 RD05R2-CLAIM-05）。

## CLAIM 26

- 结果：非阻断——DEFAULT_HZ=100/守卫/无生产调用者/15 文件测试计数（含 47=46 跑+1 忽略实跑复核）全部成立；但 §3.2 引用的一句 with_hz(60) 注释已不在现行 kernel/src/clock.rs（仅存于过期 target/fmtchk 副本），且"kernel 层副本"锚点错指 arch 层文件（对应上文 RD05R2-CLAIM-06）。

- 本次反证：6 条 CLAIM（阻断 0 / 非阻断 3 / 反证失败 3）。
