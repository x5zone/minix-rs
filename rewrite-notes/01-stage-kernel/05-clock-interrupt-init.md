# 05 · 让时间流动、让中断可达：时钟与中断初始化

> **C 真值源**: `minix3/minix/kernel/clock.c:init_clock`、`minix3/minix/kernel/arch/i386/protect.c:idt_init`、`minix3/minix/kernel/arch/i386/arch_clock.c`
> **Rust 实现**: `os/kernel/src/lib.rs:fn init_clock_and_interrupts`、`os/kernel/src/irq_manager.rs:fn enable_irq`、`os/arch/src/{arm64,riscv64}/clock.rs`

## 第 1 章　概念：03 交了空表，05 往里填内容并让它跑起来

### 1.1 问题：一张能跳、却还没接上真实硬件的表

03 文档把保护结构建了起来——x86 的 IDT（中断描述符表）、TSS、每 CPU 的入口骨架就位了，`idt_init` 也把每个向量填上了一段 stub 入口。但那只是"CPU 知道异常来了能跳进一段 stub"，还没接到**真实的硬件中断源**上：时钟 tick 从哪个计数器来、外设 IRQ 怎么映到向量、哪个中断控制器（APIC / GICv3 / PLIC）负责分发、stub 之后交给哪个内核 handler。05 要做的，是把这些"活的信号源"接上。

### 1.2 为什么这一步必须排在 04 之后

时钟频率、中断控制器基址、有几个 hart——这些参数**不该再由代码写死**（那是 04 批判过的做法）。05 的正确姿势是：**先有 04 发现的 `PlatformDesc`，再从它取参数来初始化中断控制器与架构层**。所以 05 在启动链上排在 04 之后——它第一个动作就是拿 04 冻结好的那份只读描述。这不是编排随意，是数据依赖：没有描述，就不知道该拿哪个基址去构造中断控制器实例、该配哪个 arch-misc。

> **一个必须说清的细分（否则会把依赖讲错）**：这里要区分两个“频率”。**软件 tick 率**（内核把时间切成多少格）是编译期常量 `DEFAULT_HZ`（`os/kernel/src/clock.rs:DEFAULT_HZ`，值 100），**不**从 `pd` 来；而**硬件 timer/中断控制器的基址与拓扑**（拿谁产生周期中断、有多少 hart）才从 `pd` 取。05 对 04 的真实依赖是后者，不是“从 pd 读 tick 频率”。把两者混为一谈会得到“硬件参数全来自发现”这个不真的结论。

```
01 boot-shim 交接 → 03 建保护结构(IDT 填 stub、TSS 就位) → 04 发现 PlatformDesc(只读冻结)
                                                          │
                                        05 从这里取参数：──┤
                                          硬件 timer 参数 / IC 基址 / CPU 拓扑
                                                          ▼
                                init_clock_and_interrupts：填表、使能、接上 tick
```

> **认知减负**：本篇讲"初始化编排 + 为什么这个顺序 + 一次真实使能路径"，不逐个讲每种中断控制器的寄存器编程细节（APIC 的 LVT、GIC 的 ICDGICR、PLIC 的 priority/threshold）——那些属中断/驱动各篇的"编程"侧。边界同 04：本篇把"发现到的参数"接进"能收中断的状态"，具体寄存器时序留后。

---

## 第 2 章　C 源码：向量表怎么填、时钟从哪来

### 2.1 idt_init：把处理入口填进向量表

03 的 `prot_init` 里就调了 `idt_init()`（`minix3/minix/kernel/arch/i386/protect.c:idt_init`，节选）：

```c
void idt_init(void)
{
	idt_copy_vectors_pic();               // PIC 模式下各向量指向对应 stub
	idt_copy_vectors(gate_table_exceptions);  // 异常/中断门按向量号填入口地址
}
```

它填的是"向量号 → 处理入口地址"这张表，入口指向的是一段 **stub**（陷入汇编存上下文的小口）。此刻只保证"中断来了 CPU 能跳进一段 stub"，还没决定"外设 IRQx 用哪个向量、由哪个中断控制器分发、stub 之后又交给哪个内核 handler"——**stub 到 handler 这一跳、以及 IRQ→向量的映射**，才是下面中断控制器初始化要接上的。

### 2.2 init_clock：运行期 env 定的 tick，与 Rust 把它改成编译期常量

`clock.c:init_clock` 初始化内核时钟状态。**C 版里 tick 频率是运行期从环境变量读的**：`env_get("hz")` 拿到就用，缺失或越界（`<2` 或 `>50000`）才回落到 `DEFAULT_HZ`。x86 走 APIC timer（`arch_clock.c` 注释"Using APIC, it is configured in apic_idt_init()"）、旧 PIC 机型走 8253/8254。APIC 与 PIC 的 idt 是两套（`arch_smp.c` 里 `apic_idt_init(0)` 与 `apic_idt_init(1)` 分别装 APIC 模式与 PIC 回落模式的中断表）。

> **辨析（一处真实的 Rewrite 偏离）：** Rust 侧把这条 `env_get("hz")` 运行期路径**主动去掉了**，让**软件 tick 率**固定为编译期常量 `DEFAULT_HZ`（`os/kernel/src/clock.rs:DEFAULT_HZ`，值 100）。所以"tick 率是编译期常量"只对 **Rust 侧成立**、对 C 不对——别把 §1.2 那句"Rust 的编译期 DEFAULT_HZ"误当成 C 的行为。而硬件 timer 的定标仍从 04 发现的 `pd` 取（见 §1.2 两频率细分）。

### 2.3 使能一个外设中断的 C 路径

驱动申请一个 IRQ，内核要：把该 IRQ 挂进 `irq_hook`、经中断控制器 `enable_irq` 解屏蔽。C 的 `enable_irq`（`interrupt.c`）走的是直接操作 APIC/ICU 寄存器；这一路径在 Rust 侧对应 `os/kernel/src/irq_manager.rs:fn enable_irq`（见 §4）。

---

## 第 3 章　Rust 设计：实例化 + 单一真相源

### 3.1 从描述符"实例化"中断控制器，而非写死

C 用一堆 `#if` / 板级常量决定用哪个中断控制器。Rust 把 04 的 `PlatformDesc` 当**唯一真相源**：从中断控制器描述符**构造出控制器实例**，再对实例调 `init`。这样 OS 层不写"我假设有 APIC"，而是"平台告诉我是什么、我就初始化什么"。

| 方案 | 做法 | 取舍 |
|---|---|---|
| **实例化（选用）** | 从 `PlatformDesc.interrupt_controller()` 拿描述符 → 构造 IC 实例 → `instance.init()` | 与 04 的"发现"对齐，加一种控制器不改内核编排序 |
| 全局单例 + `#if` | 编译期选一个 IC、写死基址 | ❌ 回到 04 批判的硬编码 |

### 3.2 时钟/中断的"编排"与"寄存器编程"分家

`init_clock_and_interrupts` 只做**编排**（拿描述、按依赖顺序初始化各子系统）；具体寄存器时序在架构实现（`os/arch/src/arm64/clock.rs`、`riscv64/clock.rs`、x86 的 APIC 模块）里。OS 语义层看不见 LVT/ICCGIR/threshold 这些位——延续 01~04 的"抽象语义、差异关进实现"。

---

## 第 4 章　实现 + 实地导览：打开代码，这一步真实长什么样

`init_clock_and_interrupts` 是编排的落点（`os/kernel/src/lib.rs:fn init_clock_and_interrupts`，节选真实代码）：

```rust
pub fn init_clock_and_interrupts() {
    use minix_arch::{ArchInit, CurrentArchInit};
    use minix_platform::{platform_desc, PlatformDesc};
    use minix_plat::{InterruptRouter, CurrentInterruptController};
    use crate::clock::ClockState;

    // 取 04 冻结好的那份只读描述——硬件参数的来源
    let pd = platform_desc();

    // Step 1: 软件层时钟状态（对位 C: init_clock, clock.c:48）
    let mut clock = ClockState::new();   // 软件 tick 率是 DEFAULT_HZ（见 §1.2）

    // Step 2: 从中断控制器描述符**构造实例**并 init（对位 C: intr_init(0)）
    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());  // ← 消费 pd
    intr.init();                                   // 内部 mask_all()
    unsafe { *IRQ_MANAGER.get() = Some(crate::irq_manager::IrqManager::new(intr)); }

    // Step 3: 从 arch-misc 描述构造架构初始化器并 init（对位 C: arch_init()）
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());                    // ← 又消费 pd
    arch_init.init();
    // …时钟句柄存入全局，供 clock::clock_irq_handler 在 dispatch 时读…
}
```

**顺着读这条链**（它把 04→05 真正接起来）：

```
kmain Phase（03 篇的骨架）
  └─ init_clock_and_interrupts()                       ← 本篇入口
       ├─ let pd = platform_desc()                     ← 取 04 冻结的 PlatformDesc
       ├─ Step1 ClockState::new()                       时钟软件状态（tick 率= DEFAULT_HZ）
       ├─ Step2 CurrentInterruptController::new(pd.interrupt_controller()) 并 init ← 消费 pd
       ├─ Step3 CurrentArchInit::new(&pd.arch_misc()) 并 init                        ← 又消费 pd
       └─ 把 intr 存进全局 IRQ_MANAGER；handler 接上 03 建的向量表
            ↓ 之后驱动使能某条 IRQ 时
       irq_manager.enable_irq(hook_id, irq)             (os/kernel/src/irq_manager.rs:fn enable_irq)
```

把这条链在仓库里点一遍就能看到：`platform_desc()` 返回的是 04 在 T2.5 `PLATFORM.freeze(...)` 存进去的那份只读描述——**05 的一切硬件参数都从 04 的发现里取**，一条数据线把两篇缝上；`enable_irq` 则是"驱动申请→解屏蔽"这条可观察行为的落点（对位 C `interrupt.c:enable_irq`）。这里刻意没写"APIC 的 LVT Timer 寄存器值是多少"，因为那是中断/驱动各篇的编程细节，本篇只到"用发现到的描述把中断系统初始化到能收 tick"为止。

---

## 第 5 章　测试要点

- 时钟：`ClockState` 的 tick/换算单测；三架构下 timer 中断端到端能触发（`os/qemu-tests/test-timer-irq.sh` 及其 `-aarch64`/`-riscv64` 变体，引导内核在 `os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq*`，验证 tick 到点触发）。
- 中断：`enable_irq`/mask 路径单测（用 mock 中断控制器断屏蔽位随调用翻转）；真机上外设中断能进 handler。
- 依赖：断"取 `platform_desc()` 前，04 的冻结必须已完成"——顺序错则拿到空/未初始化描述。**测试数现算**（`rg -c '#\[test\]' os/kernel/src/clock.rs os/kernel/src/irq_manager.rs`），不抄表。

> 认知减负：真机门需 QEMU；宿主 `cargo test` 用 mock IC 覆盖编排与使能逻辑，寄存器级生效只在真机。

---

## 第 6 章　诊断：时钟不走 / 中断收不到 怎么定位

两类典型故障：

- **tick 不来**（调度不动、超时全卡）：硬件 timer 没按发现的参数定标好。先确认 04 发现的那份描述里 timer/中断部分是否合理（若又走了 QemuVirt 兜底——即 04 里"没拿到真 DTB/ACPI 时的硬编码默认描述"，频率与基址都是预置值而非真硬件）。注意软件 tick 率本身是 `DEFAULT_HZ` 常量，要查的是硬件 timer 的定标而非这个常量。
- **外设中断收不到**（驱动等待永不返回）：`enable_irq` 是否真解了屏蔽、handler 是否填进了正确向量。x86 上还要分 APIC 与 PIC 回落两套 idt（C 的 `apic_idt_init(0/1)` 就是这个分叉）——选错那套，向量表指向的入口不对。

可复制定位命令：

```bash
# 看 05 入口的编排步序（是否先取 platform_desc 再初始化）
rg -n "platform_desc|ClockState::new|interrupt_controller|\.init\(" os/kernel/src/lib.rs

# 跟一次中断使能路径：hook 登记 → 解屏蔽
rg -n "fn enable_irq|fn register_hook|mask|unmask" os/kernel/src/irq_manager.rs
```

预期：正常启动日志里时钟/中断初始化各有一行里程碑、且出现在 04 的平台发现之后；若 `enable_irq` 后仍收不到中断，沿 §4 那条链核对"向量表填的是哪套 idt（APIC vs PIC）"。
