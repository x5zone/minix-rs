# P-DESIGN 第二遍（D2）· 05-clock-interrupt-init §3/§4 ↔ Rust 源码

- 审查者：独立设计审查（第二遍；D1 因 D-024 停跑，本遍以「文档主张 ↔ 源码事实」为基线独立取证）。
- 范围：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` §3、§4（§4.10 含 §5.1 计数表复核）。
- 证据源：仅 related-code.txt 清单内的 Rust 实现 + 文档本身；不读作者解释、历史评审、panel 其他文件、defect-ledger/loop/metrics、expected/。
- 方法：按七个维度（状态表达、身份、所有权、错误、并发、抽象边界、可测试性）批量核对类型与方法签名、调用者集合、常量与写入值、测试计数；每条结论给出 path:line。
- 纪律声明：单实现 trait 不判坏（先问是否测试接缝）；bool 不一律换 enum（判据是非法状态可否表示）。

## DIFF-TABLE

文档主张 ↔ 源码事实逐项对照。前 12 行为核对一致项（抽样列锚点），后 6 行为差异项（对应下方 FINDING / ARCH-PROPOSAL）。

| # | 文档位置 | 文档主张 | 源码事实 | 结论 |
|---|---|---|---|---|
| 1 | §3.1 ClockArch 签名引文 | 12 方法四组、`Sized + Send + Sync`、空体/零值默认 | os/arch/src/arch/clock.rs:76-240 逐方法一致 | 一致 |
| 2 | §3.1 分层依赖方向 | `os/arch/Cargo.toml` 无 `minix-kernel`；kernel 单向依赖 arch | os/arch/Cargo.toml:13-14（只依赖 plat/platform）；os/kernel/Cargo.toml:13 | 一致 |
| 3 | §3.2 DEFAULT_HZ 双份 100、with_hz 守卫 2..=50000 | 两份同值；构造器回退 | os/arch/src/arch/clock.rs:47；os/kernel/src/clock.rs:709、1143-1146 | 一致 |
| 4 | §4.2 ClockState 九字段与守卫 | 字段表、set_alarm_timer 断言、tick_bsp/tick_ap debug_assert 包装 | os/kernel/src/clock.rs:1083-1109、887-890、1260、1280 | 一致 |
| 5 | §4.3 三架构 init_timer 写值 | PIT divisor+`hz>=19` 断言；CNTP CVAL=当前计数+间隔、CTL=0x2、TICK_INTERVAL 原子存；SBI ecall | os/arch/src/x86_64/clock.rs:189-191；os/arch/src/arm64/clock.rs（init_timer 写 0x2）；os/arch/src/riscv64/clock.rs:124-146 | 一致 |
| 6 | §4.4 控制器初始化 | x86 LAPIC 断言+四 LVT 全屏蔽、IOAPIC 逐脚、isa_irq_to_pin 0↔2；arm64 GICD 读改写+GICR_SGI_FRAME+IGROUPR0；riscv 优先级 1/使能清零/阈值 0；mask_all 三种口径 | os/plat/src/x86_64/interrupt.rs:309-345、376-383、487-493；os/plat/src/arm64/interrupt.rs:175-215、65-74；os/plat/src/riscv64/interrupt.rs:124-134 | 一致（GICD_CTLR bit 31:30 与规范的出入文档已自登记，不再立新案） |
| 7 | §4.5 claim/complete 三答 | x86 无身份+EOI 照写；aarch64 IAR 低 24 位、1023→None；riscv claim_context/complete_context 按 context | os/plat/src/x86_64/interrupt.rs:568-585；os/plat/src/arm64/interrupt.rs:404-426；os/plat/src/riscv64/interrupt.rs:259-280 | 一致 |
| 8 | §4.6 三架构 ArchInit | x86 init 仅注释；aarch64 FPEN 0x0030_0000、PMU 由描述符门控（现全 false）、GICD base 播种；riscv 空 | os/arch/src/x86_64/arch_init.rs（init 仅注释）；os/arch/src/arm64/arch_init.rs（三块动作）；os/arch/src/riscv64/arch_init.rs | 一致 |
| 9 | §4.7 boot_init_timer 三段+pic_init | new→init_timer(DEFAULT_HZ, cpuid)→register_hook(五参)→enable_timer_irq→x86 pic_init；IRQ8_VECTOR=0x70 | os/kernel/src/lib.rs:3750-3802；os/plat/src/x86_64/interrupt.rs:79-100 | 一致 |
| 10 | §4.8 register_hook_at_slot 对账表 | 越界 panic、最低空闲位 id、挂链、actids 清位后 unmask | os/kernel/src/irq_manager.rs:520-580 | 一致 |
| 11 | §4.9 TIMER_IRQ 0/30/0 与 riscv 0 早退 | 三常量、mock 兜底、mask/unmask 对 0 提前返回、测试名 input_zero 残留 | os/plat/src/x86_64/interrupt.rs:45、os/plat/src/arm64/interrupt.rs:32、os/plat/src/riscv64/interrupt.rs:40、136-156；os/plat/src/lib.rs:90-114；测试名在 :529 | 一致 |
| 12 | §4.10 四全局/两套访问器/§5.1 计数 | SyncUnsafeCell 全局、boot_unchecked/with 两套、panic 文案、`#[test]` 计数表逐文件 | os/kernel/src/globals.rs:479-536；os/kernel/src/lib.rs:3135-3659；计数 grep 复核 12 文件全中（47/10/3/4/0/5/3/4/3/0/0，arch timer_irq_gate=4） | 一致 |
| 13 | §3.1/§4.3 trait 文档注释 | （文档正文）riscv 三架构都忽略 cpu_id、读走 time CSR、写走 SBI | trait 文档注释 os/arch/src/arch/clock.rs:82-84、110-115、188-191 仍描述 MMIO mtimecmp 按 cpu_id 寻址、x86 写 "LAPIC Timer" | 差异 → RD05-DESIGN-D2-01 |
| 14 | §4.1 主摘录与 §3.4 摘录 | `new` 紧跟 `intr.init()`，cfg 子步在 init 之后的省略号里 | os/kernel/src/lib.rs:2429-2498：两个 cfg 子步块在 `new` 与 `init` 之间，`init` 在 2498 | 差异 → RD05-DESIGN-D2-02 |
| 15 | §4.1 Step 2a 摘录 | x86 子步块只含一条 `assert!`（窗口页==0xFEE0_0000） | os/kernel/src/lib.rs:2447-2462 另有两条 `assert_eq!`（描述符 LAPIC/IOAPIC base vs 窗口页） | 差异 → RD05-DESIGN-D2-03 |
| 16 | §3.7 TimerIrqGate 两方法 | enable/disable 成对呈现，x86 双双空操作，文档未提 disable 的调用者 | `disable_timer_irq` 全仓零调用者（grep os/ 除 target，仅声明与三份实现）；enable 有唯一调用点 lib.rs:3779 | 差异 → RD05-DESIGN-D2-04 |
| 17 | §3.2 双份副本的代价 | 注释称 kernel 副本"为避免跨 crate 依赖"而独立存在 | os/kernel/Cargo.toml:13 无条件依赖 minix-arch；kernel lib.rs:2409/3750 已用 minix_arch 项；重导出路径存在 | 差异 → RD05-DESIGN-D2-05 |
| 18 | §4.3（未提及） | 文档未覆盖重挂路径的零间隔回退语义 | os/arch/src/arm64/clock.rs:41-56、os/arch/src/riscv64/clock.rs:191-199：`TICK_INTERVAL==0`（接线 bug）被静默替换为 DEFAULT_HZ 间隔 | 差异 → RD05-DESIGN-D2-06 |
| 19 | §4.1 Step 2a/2b 的归属 | cfg(target_arch) 块写在 kernel、调用 arch 命名自由函数（`minix_plat::x86_64_interrupt_set_lapic_va` 等）与 `minix_arch::arch::device_window` 内部 | os/kernel/src/lib.rs:2435-2497；os/plat/src/lib.rs:40、104（重导出） | 差异 → RD05-A01（ARCH-PROPOSAL） |

另记（不立案）：§4.7 摘录注释中的「s14n：」标记在源码注释中不存在（源码只写 "over 150 s the tick arm was called fewer than 1000 times"，lib.rs:3783-3784），属文档自注的取证编号；建议在该处标明其指向，避免被误读为源码原文。

## FINDING RD05-DESIGN-D2-01（P2）

- 位置：os/arch/src/arch/clock.rs:82-84（架构映射表）、110-115（`init_timer` 的 cpu_id 段）、188-191（`stop_local_timer` 的 cpu_id 段）。
- 严重度：P2（契约层注释与实现相悖，误导后来者；不改行为）。
- 分类：[DEL]。
- 问题：trait 的文档注释仍描述被放弃的上一代设计——「Only RISC-V uses it: the CLINT is a single MMIO block with one mtimecmp per hart at mtimecmp_base + cpu_id * mtimecmp_stride」（110-115 行），映射表把 x86 写成「8254 PIT divisor / LAPIC Timer」、把 riscv 的读写成「mtime (MMIO)」（82-84 行），`stop_local_timer` 注释称「RISC-V needs it to locate this hart's mtimecmp」（188-191 行）。实际实现：riscv64 的 `init_timer`/`stop_local_timer` 都忽略 `_cpu_id`，编程走 SBI TIME ecall、以调用 hart 为对象（os/arch/src/riscv64/clock.rs:124-146、161-169）；读计数走 `time` CSR，MMIO 读会取访问故障（os/arch/src/riscv64/clock.rs:148-159）。文档正文 §4.3/§3.8 描述的是现状，与实现一致；相悖的只在 trait 自身的契约注释这一层。
- 影响：trait 是三架构的公共契约，读者先读注释后读实现；注释里的 MMIO 寻址公式与「LAPIC Timer」会让按注释实现或审查的人走回头路，也削弱文档 §3.8 与代码的一致性论证。
- 证据或推理：os/arch/src/riscv64/clock.rs:124-146（`_cpu_id` 忽略、`sbi_set_timer`）、148-159（time CSR 与 scause=5 实测注释）；05 篇 §4.3 对照表（riscv「也忽略」）。
- 最小修复：改三处注释——映射表两行改为「PIT divisor（boot 源）」与「mtime，经 time CSR 读」；cpu_id 两段改为「三架构现均忽略；riscv 经 SBI 以调用 hart 为对象」。删除 `mtimecmp_stride` 公式句。
- 置信度：高（行为侧与注释侧均已读源码）。
- 是否需源码核实：否（已核）。
- 若我错了：若映射表的「LAPIC Timer」被解读为"将来 LAPIC 定时器成为启动源时的预告"，那也只是前瞻而 riscv 两处仍是当下的事实错误；修复仍只动注释，无行为风险。
- 裁决：接受。

## FINDING RD05-DESIGN-D2-02（P2）

- 位置：05 篇 §4.1 主摘录（文档 1068-1074 行）与 §3.4 摘录（文档 863-873 行）↔ os/kernel/src/lib.rs:2429-2498。
- 严重度：P2（摘录给出的执行次序与源码相反，且该次序承担功能）。
- 分类：[MODERN]。
- 问题：两处摘录都把 `intr.init();  // mask_all() called internally` 写在 `CurrentInterruptController::new(...)` 的紧后面，把「两个 cfg 注入块」放进 init 之后的省略号里；摘录头还声明省略号只覆盖「解释性注释、两个 cfg 注入块与 aarch64 SGI 调用」。源码的真实次序是：`new`（2429）→ x86 窗口校验与 VA 注入（2435-2473）→ riscv PLIC 校验与注入（2479-2497）→ `intr.init()`（2498）→ IRQ_MANAGER 写入（2504）→ aarch64 SGI 放行（2512-2513）。即两个 cfg 子步在 init **之前**。文档自己的 §4.1 步骤表第 2 行写的是「控制器 new + 2a/2b 注入 + init + 入全局」，与源码一致——文档内部自相矛盾，错的一侧是摘录。
- 影响：这不是纯排版。注入把 LAPIC/IOAPIC/PLIC 驱动的 MMIO 切到设备窗口（`set_x86_window_live` 在 init 之前执行），按摘录次序复现就会让控制器的 init MMIO 落在窗口未激活状态；对照表与摘录二选一会误导按文档走查或重写的人。
- 证据或推理：os/kernel/src/lib.rs:2429（new）、2435-2473（x86 块）、2479-2497（riscv 块）、2498（init）、2504（IRQ_MANAGER）、2512-2513（aarch64）。
- 最小修复：把摘录中 `intr.init()` 一行移到两个 cfg 块之后（或在摘录处加一行「原文中两个 cfg 块位于 new 与 init 之间」的说明），使摘录与步骤表、源码三方一致。
- 置信度：高。
- 是否需源码核实：否（已核）。
- 若我错了：若 panel 认定省略号语义本就"不保序"，剩余问题仍是摘录与本文档 §4.1 自己的表格互相矛盾，至少需在摘录处消除歧义。
- 裁决：接受。

## FINDING RD05-DESIGN-D2-03（P3）

- 位置：05 篇 §4.1 Step 2a 摘录（文档 1092-1109 行）与 2a 的散文段（文档 1135 行）↔ os/kernel/src/lib.rs:2447-2462。
- 严重度：P3。
- 分类：[MODERN]。
- 问题：Step 2a 的代码摘录只保留一条 `assert!`（窗口 LAPIC 页==0xFEE0_0000），静默略去了源码里紧随其后的两条 `assert_eq!`（`intr.lapic_base()` 与 `intr.ioapic_base()` 必须等于窗口页，否则拒绝启动）；摘录头只声明「错误文案与诊断打印按原文」，未声明断言被删。2a 的散文段也只讲了一条检查。
- 影响：这两条守卫是 PD-22 check-and-reject 家族的主体（描述符是契约、窗口只映射那一页， relocated 控制器直接拒绝）；省略后读者对「启动期挡哪几类错误」的理解从三条缩为一条，且被删的是断言而非注释，与本篇其他摘录的保真口径不一致。
- 证据或推理：os/kernel/src/lib.rs:2447-2454（LAPIC base 对窗口）、2455-2462（IOAPIC base 对窗口）。
- 最小修复：摘录补回两条 `assert_eq!`，或在摘录头显式标注「省略两条描述符-窗口一致性断言（lib.rs:2447-2462）」，散文段补一句。
- 置信度：高。
- 是否需源码核实：否（已核）。
- 若我错了：若这两条断言已由 §4.4 的 `init_lapic` 内 `assert_eq!`（os/plat/src/x86_64/interrupt.rs:315-323）完全冗余覆盖，仍存在摘录保真问题；修复不涉及代码。
- 裁决：接受。

## FINDING RD05-DESIGN-D2-04（P2）

- 位置：os/arch/src/arch/timer_irq_gate.rs:89（`fn disable_timer_irq();`）与三份实现（os/arch/src/x86_64/timer_irq_gate.rs:53、os/arch/src/arm64/timer_irq_gate.rs:48、os/arch/src/riscv64/timer_irq_gate.rs:35）；文档 §3.7（trait 签名引文）与 §4.7 表。
- 严重度：P2。
- 分类：[DEL]。
- 问题：`TimerIrqGate::disable_timer_irq` 在全仓没有任何调用者（grep os/ 排除 target 后，命中仅为 trait 声明与三份实现体；`enable_timer_irq` 有唯一生产调用点 os/kernel/src/lib.rs:3779）。它是为对称而预建的接口面：C 侧也没有同名动作（C 的关断路径属于 `stop_local_timer`/irq_disable 语义，不在"模块本地门"名下）。文档 §3.7 把两方法成对呈现、§4.7 表把 x86 的 disable 列为空操作，均未提示 disable 半边今天无人消费——这与文档 §4.7.2 自己立下的判据「差异出现之前不预建抽象」直接冲突（那里删掉的 `register_timer_handler` 空占位与这里是同一形状）。
- 影响：死 trait 方法不是零成本：三份实现各养一个空体或真实写寄存器的死路径（arm64/riscv 的 disable 会真写寄存器，却没有任何时刻会执行到），审查者每次都要为它证明"为什么留着"。
- 证据或推理：调用者集合检索（enable：lib.rs:3779 一处；disable：零处）；§4.7.2 判据原文。
- 最小修复：二选一并留痕——(a) 从 trait 删除 `disable_timer_irq`，连同三份实现与映射表行，将来出现消费者再加回；(b) 若 panel 判定保留（如将来 idle/关键区要用），在 trait 文档注释与 05 篇 §3.7 显式写「当前无调用者，保留理由为 X」。
- 置信度：高（零调用者是穷举检索结论）。
- 是否需源码核实：否（已核）。
- 若我错了：若我漏检了宏或字符串拼接式调用，则退为"文档未披露调用者位置"的 P3；修复(b)仍成立。
- 裁决：接受（删或注，由维护者按修复(a)/(b)择一）。

## FINDING RD05-DESIGN-D2-05（P2）

- 位置：os/kernel/src/clock.rs:701-709（kernel 副本与注释）、os/arch/src/arch/clock.rs:41-47（arch 副本的权威声明）；文档 §3.2。
- 严重度：P2。
- 分类：[MODERN]。
- 问题：`DEFAULT_HZ` 在两个 crate 各存一份 100，arch 副本注释给出的理由是「The kernel crate (os/kernel/src/clock.rs) has an independent copy to avoid a cross-crate dependency; modify here first, then sync」。该理由与依赖图不符：os/kernel/Cargo.toml:13 无条件依赖 minix-arch（`minix-arch = { path = "../arch", default-features = false }`），且 os/kernel/src/lib.rs:2409、3750 已经直接使用 `minix_arch::{ArchInit, ClockArch, ...}`——"避免跨 crate 依赖"这个前提不存在。C 侧原本是一个定义（archconst.h）经 include 链可见；Rust 侧反而制造了需要人肉同步的第二副本，文档 §3.2 把同步负担当作已接受的代价记录，但没有检验这个代价是否必要。
- 影响：两处常量可静默漂移（只改一处时 tick 编程与软件记账的比例失真，文档 §3.2 自己推演过 60:100 的后果），而防漂移手段（重导出）伸手可及。
- 证据或推理：os/kernel/Cargo.toml:13；os/kernel/src/lib.rs:2409（`use minix_arch::{ArchInit, CurrentArchInit}`）；os/arch/src/arch/clock.rs:47 与 os/kernel/src/clock.rs:709 同值。
- 最小修复：kernel 侧 `pub use minix_arch::arch::clock::DEFAULT_HZ;`（或经 minix-arch 的公共出口转发），删除本地副本；两份注释里的「modify here first, then sync」与「独立副本」理由段随之删除。若 mock/feature 组合确有障碍，先在注释里写清真实障碍再保留副本。
- 置信度：高（依赖边与既有 use 语句均已读）。
- 是否需源码核实：需要一次 `cargo check -p minix-kernel --features mock` 级别的编译确认重导出在全部形态下可用（本遍未跑编译）。
- 若我错了：若存在我没看到的 feature 门控使 arch::clock 模块在 kernel 的某形态下不可用，那么修复降级为"把理由注释改写成真实原因"；发现本身（理由与依赖图不符）仍然成立。
- 裁决：接受。

## FINDING RD05-DESIGN-D2-06（P3）

- 位置：os/arch/src/arm64/clock.rs:41-56（`rearm_next_tick`）、os/arch/src/riscv64/clock.rs:191-199（`local_timer_eoi`）；文档 §4.3 未提及。
- 严重度：P3。
- 分类：[MODERN]。
- 问题：重挂路径在 `TICK_INTERVAL` 为 0（即 `init_timer` 尚未执行）时，静默回退到「freq / DEFAULT_HZ」的间隔，两处注释都承认「before any init_timer would be a wiring bug」——一个被自我声明的接线 bug，在 release 构建里被换成 100 Hz 的正常观感继续跑，不留任何痕迹。按"非法状态可否表示"的判据：`TICK_INTERVAL==0` 是可表示、可到达（顺序回归即可触发）、且被吸收的状态。
- 影响：三段次序（§3.7）若将来被回归破坏，故障表现是"定时器仍在走但按错误频率"，而不是一声显式的断言；诊断章（§6.1）的频率类症状排查会多绕一圈。
- 证据或推理：os/arch/src/arm64/clock.rs:43-48（注释原文「wiring bug」）、53（`freq / crate::clock::DEFAULT_HZ`）；os/arch/src/riscv64/clock.rs:192-199 同构。
- 最小修复：在回退分支前加 `debug_assert!(TICK_INTERVAL.load(...) != 0, "rearm before init_timer")`——debug 构建立刻响，release 行为不变（保留回退作为中断上下文的最后防线）；并在 §4.3 补一句两架构重挂路径的回退语义。
- 置信度：高。
- 是否需源码核实：否（已核）。
- 若我错了：若 panel 认为"中断上下文内绝不 panic、debug 断言也不该在中断路径"是本仓的红线，则退为仅在 §4.3 补文档；发现的事实部分不受影响。
- 裁决：接受。

## ARCH-PROPOSAL RD05-A01

- 位置：os/kernel/src/lib.rs:2435-2497（kernel 内两个 `#[cfg(target_arch)]` 块）；os/plat/src/lib.rs:40、104（arch 命名自由函数重导出 `riscv64_early_console_write_str`、`x86_64_interrupt_set_lapic_va`/`_set_ioapic_va`）；os/arch/src/arch/device_window.rs:141-228。
- 主张：设备窗口的 check-and-reject 与 VA 注入今天由 kernel 的启动编排层用 cfg 按架构分派，直接调用 arch 命名的自由函数与 `minix_arch::arch::device_window` 的内部函数；「驱动用哪个 VA 访问 LAPIC/IOAPIC/PLIC」属于机制，而非 OS 语义层应当直说的内容，与本项目第一原则（抽象语义，不描述机制；arch trait 之外不向语义层暴露硬件承载细节）存在张力。现有接缝其实已经存在：`InterruptRouter`（kernel 已依赖其 `init/mask/unmask` 面）或 `CurrentInterruptController` 可以拥有「校验窗口一致性并注入 VA」这一步。
- 提议：把 Step 2a/2b 收进控制器接口——例如 `InterruptRouter::init` 前增加一个 `fn attach_device_window(&mut self, va: usize)`（或 init 内部经 `device_window` 自解析），PLIC 的 round-trip 诊断打印随注入一起移入驱动；aarch64 的 SGI 放行（2c）本就经 `irq_manager::enable_sched_sgi_for_cpu` 走接口，保持不动。kernel 侧的两个 cfg 块与 arch 命名重导出函数随之消失，`init_clock_and_interrupts` 对架构只说 trait 语言。
- 影响面：改动 §4.1 文档记载的 Step 2a/2b 形态（步骤从 kernel 编排项变为控制器接口项）、plat 两个重导出、PD-22 的注入路径归属；不改任何寄存器写值与次序约束（注入仍须先于 `init`，该次序挪进接口内部后由实现保证）。
- 判为 ARCH 的理由：这改变「中断初始化是什么」的记载形态（§4.1 的步骤表与三个子步的归属），按三档术语需要文档/设计/代码三处 `[ARCH: ...]` 标注，不作为 FINDING 提出。
- 代价与风险：触面是三个架构的控制器实现加 kernel 一处调用点；收益是 kernel 去掉 cfg 分派与 arch 名泄漏，设备窗口成为控制器自描述的能力。若 panel 认为启动编排允许谈机制（boot 编排层本来就是机制层），则本提案降级为记录性说明即可。
- 置信度：事实面高（调用关系已核）；规范性判断中。
- 是否需源码核实：需要——`plat::x86_64::interrupt::set_lapic_va` 系列与 `set_plic_va_base` 的全部调用者清点后再定挪动边界。
- 裁决：需用户裁决。

## VERDICT

- 总体：文档 §3/§4 的事实准确性高——trait/结构体签名、三架构寄存器写值、调用链次序（除 D2-02）、测试计数表（逐文件 grep 全中）与源码一致；未发现虚构主张，未发现 P0。设计决策层（ClockState/ClockArch 分层、IRQ_MANAGER 归宿、三段分离、TimerIrqGate 两大门的归属、claim/complete 配对协议）在源码中均有清晰落点且有测试或形状钉支撑。
- 发现面：P2×4（trait 注释漂移、摘录次序错误、死 trait 方法、DEFAULT_HZ 双副本假理由）、P3×2（摘录缺断言、零间隔回退静默吸收）、ARCH-PROPOSAL×1（设备窗口注入的归属，需用户裁决）。
- 系统性观察：四处 P2 里三处（D2-01、D2-02、D2-04）属于「契约/文档层与实现层的缓慢漂移」——行为正确、叙述过时；建议 review 之外增加一次"trait 文档注释与实现签名/机制的一致性抽查"，能在不跑测试的情况下拦住这一族问题。
- 本遍未跑编译与宿主测试（D2 为静态设计对照）；D2-05 的重导出可行性与 D2-A01 的调用者清点列为需源码核实的后续项。

- 本次扫描：6 条 FINDING，1 条 ARCH-PROPOSAL。
