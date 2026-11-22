# P-DESIGN 第二遍（确认轮）· D2 · 05-clock-interrupt-init

- 审查者：独立设计审查（D2），基线 = 文档主张 ↔ 当前 HEAD 源码事实
- 输入：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` §3/§4（708–1929 行）+ `related-code.txt` 清单内实现文件
- 说明：上一轮修改轮已实施 6 项修复（删 disable_timer_irq、DEFAULT_HZ 收 re-export、重挂回退加 debug_assert、arch/clock.rs 注释改 SBI 口径等）；本轮全部以当前 HEAD 独立复核，不复述上轮结论。
- 证据强度：全部为 L1（Read/grep 实际命令输出）。

## 已核验为一致的主张（抽样记录，不占 FINDING）

- §3.1 trait `ClockArch` 12 方法签名与 `os/arch/src/arch/clock.rs:87-246` 逐一对上（含 `init_local_timer` 默认体在 :141）。
- §3.1 依赖方向主张：`os/arch/Cargo.toml` 依赖表确无 `minix-kernel`（仅注释里提及）；`os/kernel/Cargo.toml:13` 无条件依赖 `minix-arch`。
- §3.3 两个 trait 签名与 `os/plat/src/interrupt.rs:143-174`（InterruptRouter 六方法）、`:186-244`（PerCpuInterruptUnit 五方法）一致；`IrqManager` 五字段与 `os/kernel/src/irq_manager.rs:453-458` 一致；泛型 bound `IC: InterruptRouter + PerCpuInterruptUnit` 在 :453。
- §3.4 两套访问器契约与 `os/kernel/src/lib.rs:3135/3153/3508/3525` 一致，expect 文案逐字一致。
- §4.1 `init_clock_and_interrupts`（lib.rs:2407-2530）：use 顺序、Step 1-4 次序、2a/2b/2c 三个 cfg 块、两条 assert_eq（:2447-2462）、aarch64 SGI 行（:2512-2513）均逐字对上。
- §4.7 `boot_init_timer`（lib.rs:3722-3803）：三段 + `pic_init` 尾调用、五参数 register_hook、`Endpoint::from_generation_slot(0, CLOCK.0)`、注释与源码一致。

## FINDING RD05R2-DESIGN-D2-01（P1）[DEL]

**主张**：§3.2 的「代码引用」块（文档 755–777 行）把 `os/arch/src/arch/clock.rs` 的 DEFAULT_HZ 文档注释结尾引作：

> `/// **Authoritative definition**: this is the arch-level constant. The kernel crate re-exports it (os/kernel/src/clock.rs 的 pub use)——早先 kernel 有一份独立副本，理由写的是 avoid a cross-crate dependency，副本只留漂移风险，已改为单一权威定义。`

**源码事实**：`os/arch/src/arch/clock.rs:40-42` 的实际注释是：

> `/// **Authoritative definition**: this is the arch-level constant. The kernel crate (os/kernel/src/clock.rs) has an independent copy to avoid a cross-crate dependency; modify here first, then sync.`

**判读**：文档标着「代码引用（文档注释与常量定义行）」，但引文与代码不符——这不是转述而是整段替换。且矛盾落在实体上：kernel 侧确实已是 re-export（`os/kernel/src/clock.rs:710` `pub use minix_arch::arch::clock::DEFAULT_HZ;`），arch 侧注释却仍宣称「kernel has an independent copy; modify here first, then sync」，注释描述的副本已不存在，注释自我失效。修复轮改了 kernel 侧与文档正文，漏了 arch 侧注释本体。上一轮修复（DEFAULT_HZ 收成 re-export）在代码与文档两个方向都做了，唯独这第三处（arch 注释）没同步，而文档反而按"理想态"重写了引文，形成双重失真：代码注释过时 + 文档引文虚构。

**修复方向**：把 `os/arch/src/arch/clock.rs:40-42` 注释改为与 re-export 事实一致（指向 kernel 侧 `pub use`）；文档引文恢复逐字引用改后的代码。

## FINDING RD05R2-DESIGN-D2-02（P2）[DEL]

**主张**：§3.2（文档 780 行）称 kernel 侧注释里「额外写了一句『需与 C 完全一致时可在 boot 阶段用 `with_hz(60)` 指定』」。

**源码事实**：`grep -rn "with_hz(60)\|需与 C 完全一致" os --include="*.rs"`（排除 target/fmtchk）唯一命中是 `os/tests/pm_sched.rs:302` 的测试代码 `CapturedKernel::with_hz(60)`（另一个类型的测试辅助方法），不是任何注释。`os/kernel/src/clock.rs:702-709` 的 re-export 文档注释里没有这句话。

**判读**：文档对一条不存在的注释句子做了直接引语陈述，属于虚构引文。`with_hz(60)` 的「C 对齐用法」只在测试里出现过，且不是通过这句注释表达的。P2：单句虚构，不改变设计结论（with_hz 无生产调用者的主张本身已核实为真），但违反「事实断言必须带锚点、引文必须逐字」。

**修复方向**：删掉该句或改写为对测试用例（`os/tests/pm_sched.rs:302`）的真实引用；不得虚构注释内容。

## FINDING RD05R2-DESIGN-D2-03（P2）[DEL]

**主张**：§3.5/§4.6 陈述「riscv64 的 `init()` 是空的」且正文引用的函数体注释明言第 1 项（PMP）与第 2 项（sie 写入）都「Deliberately not done」。

**源码事实**：
- `os/arch/src/riscv64/arch_init.rs:1-4` 模块头注释：`performing PMP configuration and S-mode interrupt enablement`。
- `:19-26` 结构体文档注释：`1. Configure PMP (Physical Memory Protection) to allow all access 2. Enable S-mode interrupts (SIE register)`。
- `os/arch/src/arch/arch_init.rs:37` trait 的 Architecture mapping 表：`| init() | ACPI | PMU cycle counter + bsp_init | PMP + SIE |`。

**判读**：三处注释仍宣称 riscv64 的 `init()` 会配 PMP、开 SIE，而函数体（同文件 :37-57）明确这两件事都故意不做、且文档正文也按「空实现」陈述。注释与行为相反，正是文档 §4.6 说的「能做的那些写入不属于 S 模式」这一演进落定后遗留的旧描述。读者只看模块头或 trait 映射表会得出与函数体相反的结论。P2：注释失实，代码行为本身正确。

**修复方向**：改 `riscv64/arch_init.rs` 模块头与结构体注释为「deliberately empty；见函数体与 05 篇 §3.5」；`arch/arch_init.rs:37` 表格 riscv64 格改为 no-op（PMP/SIE 由固件或各自 trait 负责）。

## FINDING RD05R2-DESIGN-D2-04（P2）[DEL]

**主张**：§3.7 记录了删除决策：`TimerIrqGate` 只剩一个方法（[DEL] RD05-DESIGN-D2-04，原 `disable_timer_irq` 已删）。

**源码事实**：
- `os/arch/src/arch/timer_irq_gate.rs:3` 模块头：`Architecture-specific enable/disable of timer IRQ delivery`。
- `:62` trait 文档注释：`/// Open/close the timer IRQ delivery gate: the paired hardware operations.`。

**判读**：trait 与模块的自身注释仍按「enable/disable 成对操作」描述，与「单方法 trait」的现状相反——删除发生在代码，注释还停在删除前。这与上一轮删 `disable_timer_irq` 的修复同源：代码删了，注释头没有跟着改。文档引文块标着「文档注释省略」，所以文档没有虚构；但 trait 自述与代码签名矛盾会误导下一个读者去找 `disable` 方法。

**修复方向**：`timer_irq_gate.rs` 模块头与 trait 文档改为单动词（enable only），并注明关闭路径的归属（见 RD05R2-DESIGN-D2-05 的勘正）。

## FINDING RD05R2-DESIGN-D2-05（P2）[DEL]

**主张**：§3.7（文档 989 行）为删除 `disable_timer_irq` 给出的理由是「关闭动作由控制器侧的门承担」。

**源码事实**：
- `os/arch/src/arm64/clock.rs:149-159` `stop_local_timer`：写 `CNTP_CTL_EL0 = 0x2`（Enable=0、IMASK=1）——这是**模块本地门**的关闭，不是控制器侧；
- `os/arch/src/riscv64/clock.rs:161-173` `stop_local_timer`：`sbi_set_timer(u64::MAX)` + `csrc sie, 0x20`（清 STIE）——sie.STIE 正是文档 §3.7 表格里定义的 riscv64「模块本地门」，且 riscv64 **没有控制器侧线**（§3.7 表格自述「无——定时器是核本地中断」）；
- 全仓对 `sie.STIE` 的清零只有这一处。

**判读**：删除方法本身正确（零调用者成立），但文档给出的替代路径是错的：模块本地门的关闭动作由 `ClockArch::stop_local_timer` 承担（arm64 写 CNTP_CTL、riscv64 清 sie.STIE），riscv64 上根本不存在控制器侧的门可承担。文档 §3.7 自己的表格与代码都反对这句话。P2：决策节的理由句与代码机制矛盾，会误导后续维护者以为「关定时器 = 走 IrqManager 的 mask」——对 riscv64 那是一次约定内的空操作。

**修复方向**：该句改为「关闭动作由 `ClockArch::stop_local_timer` 承担（arm64/riscv64 关模块本地门）；控制器侧的线由 IrqManager 的 mask 负责，riscv64 上无控制器线」。

## FINDING RD05R2-DESIGN-D2-06（P2）[DEL]

**主张**：§4.7 的 `pic_init` 引文块头标明「函数体；**注释按原文**」，引文内注释以中文呈现（`// master ICW2：向量基 0x50`、`// IMR：全 mask（IRQ0/IRQ2 由 unmask_irq 按需打开）`、`// 级联线` 等）。

**源码事实**：`os/plat/src/x86_64/interrupt.rs:85-102` 的实际注释是英文：`// master ICW2: vector base 0x50`、`// master ICW3: IRQ2 wired to the slave`、`// slave ICW3: cascade identity = 2`、`// IMR: mask everything (IRQ0/IRQ2 are opened on demand by unmask_irq)`、`// cascade line`。

**判读**：块头承诺「注释按原文」，实际是中文转述。同节的 boot_init_timer 引文块（文档 1736 行）明确标注了「注释为中文转述、非源码逐字，代码逐字」——同一篇文档内两种标注口径，pic_init 块的标注与内容不符。P2：引文保真度问题，数值（0x50/0x70/0x04/0x02/0xFF）本身全部核实一致。

**修复方向**：把 pic_init 块头改为「注释为中文转述、非源码逐字」，或恢复英文原文。

## FINDING RD05R2-DESIGN-D2-07（P3）[DEL]

**主张**：§3.8 演进表 `[ARCH: K-1]` 行的依据锚点是「[`interrupt.rs`](os/plat/src/arm64/interrupt.rs)、[`clock.rs`](os/arch/src/arm64/clock.rs) 的模块注释」。

**源码事实**：全仓 `[ARCH: K-1]` 标注（排除 target/fmtchk）只有一处：`os/arch/src/arm64/timer_irq_gate.rs:12`。`os/plat/src/arm64/interrupt.rs` 与 `os/arch/src/arm64/clock.rs` 的模块注释均不含 `[ARCH: K-1]`（grep 零命中）。

**判读**：文档声称的「依据锚点」在所指文件里不存在该标注；K-1 的代码侧标注落在文档未指名的第三个文件。按三档术语的「三处一致标注」要求，读者按锚点去找会落空。P3：锚点指错文件，标注本身存在。

**修复方向**：改 §3.8 K-1 行锚点为 `arm64/timer_irq_gate.rs`（或把 `[ARCH: K-1]` 补进 arm64 clock.rs / arm64 interrupt.rs 的模块注释，与文档锚点对齐）。

## FINDING RD05R2-DESIGN-D2-08（P3）[DEL]

**主张**：§3.8 K-1 行陈述 C 现状为「32 位 ARM 用 OMAP INTC + BSP 私有定时器（omap_timer.c）」，即 aarch64 的 Generic Timer 在 C 里无对应物。

**源码事实**：`os/arch/src/arm64/clock.rs:13`（模块注释）与 `:27`（结构体注释）两处 C 对位写的是 `C: earm/arch_system.c PMU init (cycle counter for user mode)`。

**判读**：arm64 时钟实现自身的 C 对位注释指向 PMU 初始化——PMU 是性能计数器使能，不是 C 侧定时器编程的对位物（C 的 ARM-32 定时器路径是 `bsp_register_timer_handler`，omap_timer.c）；这与文档 §3.8 对 K-1 的陈述相矛盾。读者按 clock.rs 注释追 C 源会追到错误的位置。P3：注释错位，不影响行为。

**修复方向**：把 arm64/clock.rs 两处 C 对位注释改为 `C: 无对应（32 位 ARM 用 omap_timer.c 的 BSP 私有定时器；ARMv8-A Generic Timer 是 [ARCH: K-1] 演进）`。

## FINDING RD05R2-DESIGN-D2-09（P3）[DEL]

**主张**：§4.6 自陈两处超出 C 行为的 aarch64 补充——①FPEN 写（「C 的 32 位 ARM 端口没有这个寄存器写，这一行是 64 位直核链的补充」）；②PMU 门控差异（「C 无条件执行，Rust 由 ArchMiscDesc 的 pmu_cycle_counter 决定，当前所有平台描述符都是 false，实际不执行」）。

**源码事实**：`os/arch/src/arm64/arch_init.rs:41-57`（FPEN 写）、`:59-76`（PMU 门控块）注释均无 `[ARCH: ...]` 标注；全仓 ARM64 侧唯一的 `[ARCH: K-1]` 在 timer_irq_gate.rs:12（见 RD05R2-DESIGN-D2-07）。文档 05 篇内这两处也未标。

**判读**：按三档术语，这两处是行为层面的 Architectural Evolution（C 无此写/行为不同），应文档 + 设计 + 代码三处一致标注。文档对事实的转述本身准确（FPEN 理由、门控差异、false 取值均已核实：`os/libs/minix-platform/src/arch/{aarch64,riscv64,x86_64}.rs` 与 `acpi.rs` 的描述符全部 `pmu_cycle_counter: false`）。P3：术语标注缺失，若标注已在 03/04 篇落位则降级为交叉引用缺失。

**修复方向**：给这两处补 `[ARCH: ...]` 标注（文档 + 代码注释同步），或在 §4.6 交叉引用已有标注位置。

## DIFF-TABLE

| # | 文档主张（§3/§4） | 源码事实（当前 HEAD） | 判定 |
|---|---|---|---|
| 1 | §3.1 ClockArch 12 方法四分组、默认体位置 :141 | arch/clock.rs:87-246 逐一对上 | 一致 |
| 2 | §3.1/§3.2 kernel 侧 DEFAULT_HZ 为 re-export、单一权威 | kernel/clock.rs:710 `pub use minix_arch::arch::clock::DEFAULT_HZ` | 一致（但 arch 侧注释仍写 independent copy，见 D2-01） |
| 3 | §3.2 arch 侧 DEFAULT_HZ 注释引文 | arch/clock.rs:40-42 与引文不符（引文为改写文本） | **D2-01（P1）** |
| 4 | §3.2 kernel 注释含「with_hz(60) 指定」句 | 全仓无此句 | **D2-02（P2）** |
| 5 | §3.2 with_hz 守卫 2..=50000、无生产调用者 | clock.rs:1144-1153；调用者仅测试 | 一致 |
| 6 | §3.2 misc.rs:1065 tsc_per_tick 读 DEFAULT_HZ | misc.rs:1065 命中 | 一致 |
| 7 | §3.1 依赖方向：arch 不依赖 kernel | arch/Cargo.toml 无 minix-kernel 条目；kernel/Cargo.toml:13 依赖 minix-arch | 一致 |
| 8 | §3.3 两 trait 签名与 IrqManager 泛型 bound | plat/interrupt.rs:143-244；irq_manager.rs:453 | 一致 |
| 9 | §3.3 set_irq_priority 仅 riscv64 覆写；enable_sgi 仅 aarch64 覆写 | riscv64/interrupt.rs:178；arm64/interrupt.rs:428 | 一致 |
| 10 | §3.3 claim/complete 共用同一偏移（PLIC） | riscv64/interrupt.rs:28 `PLIC_COMPLETE = PLIC_CLAIM` | 一致 |
| 11 | §3.4 两套访问器契约与报错文案 | lib.rs:3135/3153/3508/3525 逐字 | 一致 |
| 12 | §3.5 riscv64 init() 为空、两个位各有归属 | riscv64/arch_init.rs:37-57 函数体逐字一致 | 一致（但模块/结构体/trait 表注释仍写 PMP+SIE，见 D2-03） |
| 13 | §3.6 Phase A.2 intentionally empty、无活 cut_memmap 调用 | lib.rs:1741-1742；grep 全仓无调用 | 一致 |
| 14 | §3.7 TimerIrqGate 单方法；三架构门语义表 | arch/timer_irq_gate.rs:70-86；三实现（no-op / CNTP=1+isb / csrs sie,0x20）逐字 | 一致（但 trait/模块注释仍写 enable/disable 成对，见 D2-04） |
| 15 | §3.7 删除 disable_timer_irq 的理由「关闭动作由控制器侧的门承担」 | arm64/riscv64 stop_local_timer 关的是模块本地门；riscv64 无控制器线 | **D2-05（P2）** |
| 16 | §4.1 init_clock_and_interrupts 全序列（含 2a/2b/2c、两条 assert_eq、use 顺序） | lib.rs:2407-2530 逐字/逐序 | 一致 |
| 17 | §4.2 ClockState 字段表、set_alarm_timer 断言、reset 无守卫、tick_bsp/ap debug_assert、tick_with 第 5 步 BSP-only | clock.rs:1112-1153/888-891/855/1261/1281/1399-1404 | 一致 |
| 18 | §4.3 x86 init_timer 全函数、常量、hz>=19 断言、PitDesc 1_193_182 | x86/clock.rs:179-203、:21-25、:189；platform x86_64.rs:45/99 | 一致 |
| 19 | §4.3 arm64/riscv64 init_timer 引文（0x2 门关、SBI ecall、TICK_INTERVAL） | arm64/clock.rs:94-139；riscv64/clock.rs:124-146 | 一致 |
| 20 | §4.4 x86 init_lapic/init_ioapic/isa_irq_to_pin、mask_all 口径、SVR/LVT/EN 常量 | x86/interrupt.rs:309-338/376-382/487-496/187/192/198/208 | 一致 |
| 21 | §4.4 arm64 三步初始化引文、GICR_SGI_FRAME、GICD_CTLR 读改写、mask_all 两半 | arm64/interrupt.rs:175-218/65-74/189/319-336 | 一致（ARE 31:30 注释错误由文档自行登记，仍待源码侧修复） |
| 22 | §4.4 riscv64 PLIC init 引文与 mask_all 口径 | riscv64/interrupt.rs:124-134/167-173 | 一致 |
| 23 | §4.5 三架构 claim/complete 引文与协议（None/1023/0、EOI 0x0B0） | x86:568-586；arm64:404-426；riscv64:259-301；LAPIC_REG_EOI=0x0B0（:198） | 一致 |
| 24 | §4.5 dispatch_claimed 先 mask 再走链、Spurious 分支 | irq_manager.rs:730（mask）/738-747（Spurious） | 一致 |
| 25 | §4.6 三架构 ArchInit 引文（x86 空、arm64 FPEN+PMU+GICD 播种、riscv 空） | 三个 arch_init.rs 逐字 | 一致（FPEN/PMU 缺 [ARCH] 标注，见 D2-09） |
| 26 | §4.6 pmu_cycle_counter 当前全 false | platform 四个构造点均 false | 一致 |
| 27 | §4.7 boot_init_timer 三段 + pic_init、五参数、Endpoint 构造、x86 尾调用 | lib.rs:3722-3803 | 一致 |
| 28 | §4.7 pic_init 引文（端口/ICW3/ICW4/IMR/unmask 数值） | x86/interrupt.rs:79-103 数值全对 | 一致（但块头称「注释按原文」而注释为中文转述，见 D2-06） |
| 29 | §4.7 启动路径无 sti；IF_MASK 在 trap_return | kernel lib.rs/trap_return.rs 无 sti；trap_return.rs:43 `IF_MASK: u64 = 1 << 9` | 一致 |
| 30 | §4.8 register_hook_at_slot 四行对照（panic/find id/挂链/unmask） | irq_manager.rs:520-586 | 一致 |
| 31 | §4.9 三架构 TIMER_IRQ 取值与理由、mock 兜底、riscv64 mask/unmask 对 0 早退 | x86:45/arm64:30/riscv64:40（值 0/30/0）；plat/lib.rs:90/110/112/114；riscv64:136-164 | 一致（含「测试名 input_zero 残留」的陈述：测试仍在 :529） |
| 32 | §4.10 四全局写入点与访问器（含 try_smp_state、SMP_STATE 函数注释自称 Phase D 而调用点在 Phase F） | lib.rs:1752-1758/2504/2528/1884/1955/3653 | 一致 |
| 33 | §3.8 K-1/K-2/K-3 代码侧 [ARCH] 标注锚点 | K-2/K-3 锚点命中；K-1 的两个锚点文件无标注 | **D2-07（P3）** |

## ARCH-PROPOSAL

本轮无 `[ARCH]` 级新提案（RD05R2-A 系列 0 条）。核对过程中唯一接近架构层的观察是：模块本地门目前「开」在 `TimerIrqGate::enable_timer_irq`、「关」在 `ClockArch::stop_local_timer`（arm64 写 CNTP_CTL=0x2、riscv64 `csrc sie`），一个语义分属两个 trait——但这正是 RD05R2-DESIGN-D2-05 要求文档写准的事实，不是要求改契约；在出现第二个真实消费者之前按「差异出现之前不预建抽象」不动代码。

## VERDICT

**PASS（设计实质），遗留注释/引文同步缺陷 9 条。**

- 设计层（§3 的 8 条决策、§4 的 10 小节）与当前 HEAD 的代码逐条核对：33 项 DIFF-TABLE 断言中 30 项一致，无一 P0；上一轮 6 项修复的 4 项可在 HEAD 独立复验（DEFAULT_HZ re-export 在 kernel/clock.rs:710、arm64/riscv64 重挂回退 debug_assert 在 clock.rs:44/194、riscv64 init_timer 的 SBI 口径注释在 clock.rs:27-36、TimerIrqGate 单方法）。
- 发现全部集中在「修复轮的尾巴」：代码注释未随行为/决策同步（D2-01/03/04/08）、文档引文失实或虚构（D2-01/02/06）、删除理由句与机制矛盾（D2-05）、术语标注与锚点缺失（D2-07/09）。1 条 P1（D2-01）+ 5 条 P2 + 3 条 P3。
- 对 D2-01 的特别提示：arch/clock.rs 的过时注释与 kernel 侧 re-export 直接矛盾（"modify here first, then sync" 指向一条已不存在的同步路径），这是漂移隐患而不仅是文风问题，建议随本条修复一并落。

- 本次扫描：9 条 FINDING，0 条 ARCH-PROPOSAL。
