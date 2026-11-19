# D05 R2 certify 独立回读回执

- 结论：PASS
- 核验条数：8
- CLOSED：8；需整改项：0

## 1. arch clock.rs 的 DEFAULT_HZ 注释尾段（Authoritative definition 段）与文档 §3.2 引文逐字一致

- 判定：CLOSED
- 证据：`sed -n 40,45p os/arch/src/arch/clock.rs` 读到：`**Authoritative definition**: this arch-level constant is the single / source of truth; the kernel crate re-exports it (os/kernel/src/clock.rs / pub use minix_arch::arch::clock::DEFAULT_HZ) instead of keeping its own / copy — the former "independent copy" rationale (avoid a cross-crate / dependency) no longer holds since the kernel unconditionally depends on / minix-arch.`——即「arch 层单一权威 + kernel re-export + independent copy 理由不再成立」。引文逐字比对：`sed -n 756,779p rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md > /tmp/d05_doc_quote.txt && sed -n 23,46p os/arch/src/arch/clock.rs > /tmp/d05_src_quote.txt && diff /tmp/d05_doc_quote.txt /tmp/d05_src_quote.txt` 输出为空（QUOTE-IDENTICAL），文档 §3.2 的「代码引用」块与源码 L23–L46 逐字节一致。

## 2. §3.8 K-3 行锚点指 arch 侧定义并注明 kernel re-export

- 判定：CLOSED
- 证据：文档 L1034（`grep -n 'ARCH: K-3' rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` 命中）读数：锚点写作 `[const DEFAULT_HZ](os/arch/src/arch/clock.rs:const DEFAULT_HZ) 的注释、[const DEFAULT_HZ](os/arch/src/arch/clock.rs:const DEFAULT_HZ)（kernel 侧为 re-export，os/kernel/src/clock.rs 的 pub use）`——两处锚点均指 `os/arch/src/arch/clock.rs:const DEFAULT_HZ`，括注写明 kernel 侧 re-export。re-export 本身存在：`grep -n "pub use minix_arch::arch::clock::DEFAULT_HZ" os/kernel/src/clock.rs` → L710。

## 3. L782 尾段旧口径退役 + with_hz(60) 构造，幽灵引用清零

- 判定：CLOSED
- 证据：文档 L782 读数：`以及权威位置（单一权威定义在 arch 层，kernel 侧经 pub use re-export——早先的「kernel 独立副本、先改 arch 再同步」口径随 RD05-DESIGN-D2-05 合并而退役）。需要与 C 完全对齐的调用可以用 with_hz(60) 构造（见下表第四行的保留项），不是改这个常量。`；`grep -c 'with_hz(60) 指定' rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` 读数 `0`。

## 4. §1.4 章末复述：幽灵引文改为如实转述现行 §2.6 限定段并声明退役

- 判定：CLOSED
- 证据：文档 L129 读数：`（注意：本篇早期版本在这里引过 §2.6 的「校准读进程上下文」一句，那句已被 §2.6 现行的限定段取代——现行说法是探针钩子只数 tick、不读进程上下文；真正的位置依据是源码里的调用链……）`。现行 §2.6 限定段原文（L612）：`校准窗口本身只需要"中断入口与控制器都已经就位"（探针钩子只数 tick，不读进程上下文）`——转述与现行说法一致。`grep -n '校准读进程上下文'` 全文仅 L129 一处命中，且处于退役声明语境。

## 5. §3.1 Send+Sync 释义改为实例不携带共享可变状态，PIT 单设备 + 每核私有的是比较器/门槛位

- 判定：CLOSED
- 证据：文档 L744（§3.1 范围内）读数：`因此这里的 Send + Sync 编码的是"实例不携带跨线程共享的可变状态、引用可以传"，而不是"这是一个并发数据结构"——硬件面的前提要说准：x86 的 PIT 是单一系统级设备（§4.3 引文的原话 "The PIT is a single system-wide device"），三架构真正「每核私有」的是比较器/门槛位（CNTP、sie.STIE、LVT），不是 PIT 本身。`；旧表述清零：`grep -c '每核的定时器寄存器物理上互不干扰'` 读数 0。

## 6. §3.7 推演 2 加形态限定（register_hook 开门形态；riscv64 空链 Spurious；x86/aarch64 连 Spurious 都没有）

- 判定：CLOSED
- 证据：文档 L1013（位于 §3.7，节区间 L969–L1025）读数：`只做第一段与第三段、跳过注册。这一条只在控制器侧的门会被 register_hook 打开的形态下成立（§3.7 的段定义：register_hook「同时完成控制器侧的放行」）——跳过注册后 x86/aarch64 的控制器侧门保持屏蔽，中断根本到不了处理器，连 Spurious 都不会有；riscv64 的控制器侧是 per-hart 门、由模块本地门直接承载，跳过注册才会出现「门开、分发器在 irq_handlers[TIMER_IRQ] 上找不到链、返回 IrqError::Spurious」的空链。`

## 7. §2.1 内存图读值句改为「模块区间字段（cut_memmap 的输入取自同一组 module_list 字段，不是读切完的图）」

- 判定：CLOSED
- 证据：文档 L185（§2.1 范围内）读数：`arch_init 会读这些动作的产物——它读 config_no_apic（……）与 kinfo 的模块区间字段（cut_memmap 的输入取自同一组 module_list 字段，不是读切完的图），也读 system_hz（init_clock 的产物）。`

## 8. §3.2 反事实段点名记账路径具名对象（misc.rs:1065 的 tsc_per_tick、clock.rs:1130 的构造默认）

- 判定：CLOSED
- 证据：文档 L807 读数：`具名对象是 os/kernel/src/misc.rs:1065 的 tsc_per_tick（拿 DEFAULT_HZ 折算每 tick 的 TSC 步数，GET_UPTIME 消费它）与 §3.2 推演里 clock.hz() 的默认世界（ClockState 用 DEFAULT_HZ 构造，clock.rs:1130）`。两处代码锚点均实证：`sed -n 1065p os/kernel/src/misc.rs` → `let tsc_per_tick = u64::from(crate::clock::DEFAULT_HZ)`；`sed -n 1130p os/kernel/src/clock.rs` → `hz: DEFAULT_HZ,`（`new_for_cpu` 构造体里的默认值行）。

结论：回读验证通过，8 条逐条落证。
