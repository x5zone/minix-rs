# 05 · 让时间流动、让中断可达：时钟与中断初始化

> **C 真值源**: `minix3/minix/kernel/clock.c:init_clock`、`minix3/minix/kernel/arch/i386/protect.c:idt_init`、`minix3/minix/kernel/arch/i386/arch_clock.c`
> **Rust 实现**: `os/kernel/src/lib.rs:fn init_clock_and_interrupts`、`os/kernel/src/irq_manager.rs:fn enable_irq`、`os/arch/src/{arm64,riscv64}/clock.rs`

## 第 1 章　概念：03 交了空表，05 往里填内容并让它跑起来

### 1.1 问题：一张能跳、却还没接上真实硬件的表

03 文档把保护结构建了起来——x86 的 IDT（中断描述符表）、TSS、每 CPU 的入口骨架就位了。但那只是"CPU 知道异常来了该走哪张表"，表里的每一项还没接到**真实的硬件中断源**上：时钟 tick 从哪个计数器来、外设 IRQ 怎么映到向量、哪个中断控制器（APIC / GICv3 / PLIC）负责分发。05 要做的，是把这些"活的信号源"接上：初始化时钟、初始化中断控制器、把 handler 填进向量表、按驱动请求逐个开中断。

### 1.2 为什么这一步必须排在 04 之后

时钟频率、中断控制器基址、有几个 hart——这些参数**不该再由代码写死**（那是 04 批判过的做法）。05 的正确姿势是：**先有 04 发现的 `PlatformDesc`，再从它取参数来初始化时钟与中断**。所以 05 在启动链上排在 04 之后——它第一个动作就是拿 04 冻结好的那份只读描述。这不是编排随意，是数据依赖：没有描述，`init_clock` 不知道该把 APIC timer 配成多快。

```
01 boot-shim 交接 → 03 建保护结构(IDT/TSS 空表) → 04 发现 PlatformDesc(只读冻结)
                                                          │
                                        05 从这里取参数：──┤
                                          时钟源频率 / IC 基址 / CPU 拓扑
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
    idt_copy_vectors_pic();   // 把每个向量指向对应的 stub 入口
    /* 中断门/陷阱门按向量号逐个填 access/dselector/offset */
}
```

它填的是"向量号 → 处理入口地址"这张表。此刻只保证"中断来了 CPU 能跳进一段 stub"，还没决定"外设 IRQx 用哪个向量、由谁分发"——那是下面中断控制器初始化的事。

### 2.2 init_clock：一个编译期常量撑起的 tick

`clock.c:init_clock` 初始化内核时钟状态。C 版里 tick 频率是编译期常量（`DEFAULT_HZ`），x86 走 APIC timer（`arch_clock.c` 注释"Using APIC, it is configured in apic_idt_init()"）、旧 PIC 机型走 8253/8254。APIC 与 PIC 的 idt 是两套（`arch_smp.c` 里 `apic_idt_init(0)` 与 `apic_idt_init(1)` 分别装 APIC 模式与 PIC 回落模式的中断表）——**同一个"收时钟 tick"的语义，在 APIC/PIC 两种承载下写法不同**。

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
    use minix_platform::{platform_desc, PlatformDesc};
    use crate::clock::ClockState;
    // 取 04 冻结好的那份只读描述——所有硬件参数的唯一来源
    let pd = platform_desc();

    // Step 1: 软件层时钟状态初始化（对位 C: init_clock, clock.c:48）
    //         hz 是编译期 DEFAULT_HZ，不再 env_get
    let mut clock = ClockState::new();

    // Step 2: 中断控制器初始化（对位 C: intr_init(0), i8259.c / omap_intr.c）
    //         实例化设计：从中断控制器描述符构造实例，再对实例调 init
    // ...（用 pd 拿 IC 描述、构造并 init 控制器实例、装载 handler 到向量表）
}
```

**顺着读这条链**（它把 04→05 真正接起来）：

```
kmain Phase（03 篇的骨架）
  └─ init_clock_and_interrupts()                       ← 本篇入口
       ├─ let pd = platform_desc()                     ← 取 04 冻结的 PlatformDesc（唯一真相源）
       ├─ Step1 ClockState::new()                       时钟软件状态
       ├─ Step2 从 pd.interrupt_controller() 描述符构造 IC 实例并 init   ← 用发现结果，非写死
       └─ 装载 handler 到向量表（填实 03 建的 IDT/stvec/VBAR 骨架）
            ↓ 之后驱动使能某条 IRQ 时
       irq_manager.enable_irq(hook_id, irq)             (os/kernel/src/irq_manager.rs:fn enable_irq)
```

把这条链在仓库里点一遍就能看到：`platform_desc()` 返回的是 04 在 T2.5 `PLATFORM.freeze(...)` 存进去的那份只读描述——**05 的一切硬件参数都从 04 的发现里取**，一条数据线把两篇缝上；`enable_irq` 则是"驱动申请→解屏蔽"这条可观察行为的落点（对位 C `interrupt.c:enable_irq`）。这里刻意没写"APIC 的 LVT Timer 寄存器值是多少"，因为那是中断/驱动各篇的编程细节，本篇只到"用发现到的描述把中断系统初始化到能收 tick"为止。

---

## 第 5 章　测试要点

- 时钟：`ClockState` 的 tick/换算单测；三架构下 `init_clock_and_interrupts` 后端到端能进调度（`os/qemu-tests/.../test-clock-timer` 一类，验证 tick 到点触发）。
- 中断：`enable_irq`/mask 路径单测（用 mock 中断控制器断屏蔽位随调用翻转）；真机上外设中断能进 handler。
- 依赖：断"取 `platform_desc()` 前，04 的冻结必须已完成"——顺序错则拿到空/未初始化描述。**测试数现算**（`rg -c '#\[test\]' os/kernel/src/clock.rs os/kernel/src/irq_manager.rs`），不抄表。

> 认知减负：真机门需 QEMU；宿主 `cargo test` 用 mock IC 覆盖编排与使能逻辑，寄存器级生效只在真机。

---

## 第 6 章　诊断：时钟不走 / 中断收不到 怎么定位

两类典型故障：

- **tick 不来**（调度不动、超时全卡）：多半是时钟源没按发现的参数配好。先确认 `platform_desc()` 给的 timer 频率是否合理（回到 04：是不是又走了 QemuVirt 兜底、频率是预置值而非真硬件）。
- **外设中断收不到**（驱动等待永不返回）：`enable_irq` 是否真解了屏蔽、handler 是否填进了正确向量。x86 上还要分 APIC 与 PIC 回落两套 idt（C 的 `apic_idt_init(0/1)` 就是这个分叉）——选错那套，向量表指向的入口不对。

可复制定位命令：

```bash
# 看 05 入口的编排步序（是否先取 platform_desc 再初始化）
rg -n "platform_desc|ClockState::new|interrupt_controller|\.init\(" os/kernel/src/lib.rs

# 跟一次中断使能路径：hook 登记 → 解屏蔽
rg -n "fn enable_irq|fn register_irq_hook|mask|unmask" os/kernel/src/irq_manager.rs
```

预期：正常启动日志里时钟/中断初始化各有一行里程碑、且出现在 04 的平台发现之后；若 `enable_irq` 后仍收不到中断，沿 §4 那条链核对"向量表填的是哪套 idt（APIC vs PIC）"。
