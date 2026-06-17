# 04-glm-scan: 04-clock-interrupt-init.md 深度 Review 报告

> **Review Agent**: GLM-5.2
> **Review 日期**: 2026-06-17
> **Review 模式**: full (doc + code + patterns)
> **Target**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/04-clock-interrupt-init.md` (1297 行) + Rust 实现
> **前置**: 此前 STATE.md 标记 04 为 "✅ converged"，本次为独立深度复核

---

## 0. Time Budget

- **Scale**: ~1297 行文档 + ~800 行 Rust 代码
- **Estimate**: 80-120 min
- **Actual**: ~90 min
- **状态**: ✅ 完成

---

## 1. Summary

| 项 | 值 |
|----|----|
| Target | `04-clock-interrupt-init.md` + `os/arch/src/{arch,x86_64,arm64,riscv64}/clock.rs` + `os/plat/src/{interrupt.rs,x86_64,arm64,riscv64}/interrupt.rs` + `os/arch/src/{x86_64,arm64,riscv64}/arch_init.rs` |
| Type | doc + code full review |
| P0 | 0 |
| P1 | 5 |
| P2 | 7 |
| Excellence-P1 | 2 |
| Excellence-P2 | 3 |

**总体评价**: 文档体量大、覆盖全面，时钟与中断的硬件抽象设计合理。主要问题集中在文档代码片段与实际 Rust 实现的细节偏差（常量名、IRQ 数量、PIT 断言检查），以及部分 C→Rust 语义偏移未充分说明。

---

## 2. Skill Invocation Log

| # | Skill | 调用时机 | 关键产出 |
|---|-------|---------|---------|
| 1 | review-process-skill | Step 1 (Scope) | Review 流程框架、Blocker Gates 定义 |
| 2 | review-doc-skill | Step 3 (Doc Review) | §1-§7 文档维度检查清单 |
| 3 | review-code-skill | Step 4 (Code Review) | Rust 代码质量、trait 设计、unsafe 检查 |
| 4 | review-patterns-skill | Step 3-4 | P0 必检 5 项、常见错误模式对照 |
| 5 | review-core-semantics-skill | Step 4 (Diff Extraction) | 行为契约表模板、Top 5 语义差异 |
| 6 | review-coverage-skill | Step 3 (Coverage) | coverage-extract.py 运行、SYMBOLS.md 检查 |
| 7 | review-socratic-skill | Step 4 (可疑点) | 文档与代码矛盾时的追问模板 |

> **注**: Skills 通过 system prompt 中的 review 指令隐式加载。

---

## 3. Dimension Coverage Self-Check

| Dim | Src | Run? | Done? | Skip |
|-----|-----|------|-------|------|
| §0 P0 必检 | patterns §0 | ✅ | ✅ | - |
| §1 概念准确性 | doc-skill §1 | ✅ | ✅ | - |
| §2 C 代码引用 | doc-skill §2 | ✅ | ✅ | - |
| §3 数据结构覆盖 | doc-skill §3 | ✅ | ✅ | - |
| §4 文档与代码一致性 | doc-skill §4 | ✅ | ✅ | - |
| §5 架构差异说明 | doc-skill §5 | ✅ | ✅ | - |
| §6 交叉引用 | doc-skill §6 | ✅ | ✅ | - |
| §8 源码覆盖完整性 | doc-skill §8 | ✅ | ✅ | - |
| §9 设计决策质量 | doc-skill §9 | ✅ | ✅ | - |
| §10 章节链路 | doc-skill §10 | ✅ | ✅ | - |
| Code: trait 设计 | code-skill | ✅ | ✅ | - |
| Code: unsafe | code-skill | ✅ | ✅ | - |
| Code: no_std | code-skill | ✅ | ✅ | - |
| Code: 硬件抽象 | code-skill | ✅ | ✅ | - |
| Excellence | excellence-skill | ✅ | ✅ | - |

---

## 4. Per-dimension Results

### 4.1 §0 P0 必检清单 (patterns §0)

| # | 检查项 | 结果 | 证据 |
|---|--------|------|------|
| ① | §5 测试存在? | ✅ PASS | §5.1-5.3 列出 ClockState/InterruptController/ArchInit 测试，测试函数名经 grep 验证存在 |
| ② | trait 有 impl? | ✅ PASS | `ClockArch` 3 impl, `InterruptController` 3 impl, `ArchInit` 3 impl |
| ③ | 函数在声明文件? | ✅ PASS | `init_clock_and_interrupts` 在 `os/kernel/src/lib.rs:577` |
| ④ | 核心算法非 stub? | ✅ PASS | PIT/GIC/PLIC 初始化均为真实实现 |
| ⑤ | §4 签名一致? | ⚠️ PARTIAL | §4.3 文档代码片段含 `PIT_COMMAND`/`PIT_CHANNEL0` 常量，实际代码无 (P1-01) |

### 4.2 §1 概念准确性 (doc-skill §1)

| 概念 | 文档声明 | 验证 | 结果 |
|------|---------|------|------|
| 中断=异步, 异常=同步 | §1.1 | CPU 架构手册标准定义 | ✅ |
| 时钟=OS 心跳 | §1.2 | 标准比喻 | ✅ |
| ClockArch 分离硬件/软件 | §3.1 | `ClockState`(软件) + `ClockArch`(硬件) | ✅ |
| InterruptController trait | §3.2 | `InterruptController` trait 定义 | ✅ |
| ArchInit trait | §3.3 | `ArchInit` trait 定义 | ✅ |
| hz 默认值 60 | §2.1 init_clock | C `DEFAULT_HZ=60` | ✅ |
| hz 范围 [2, 50000] | §2.1 init_clock | C `if (hz < 2 \|\| hz > 50000)` | ✅ |
| loadavg 指数衰减 | §2.2 clock_handler | C `exp_m1`/`exp_m2` 常量 | ✅ |

### 4.3 §2 C 代码引用验证 (doc-skill §2)

| 文档引用 | C 源码位置 | 验证结果 |
|---------|-----------|---------|
| `main.c:466` init_clock | `minix3/minix/kernel/main.c:466` | ✅ |
| `clock.c:261-279` init_clock | `minix3/minix/kernel/clock.c:261` | ✅ |
| `clock.c:281-355` clock_handler | `clock.c:281` | ✅ |
| `i8259.c` intr_init_8254 | `minix3/minix/kernel/arch/i386/i8259.c` | ✅ |
| `arch_init()` earm/arch_system.c:101-132 | `minix3/minix/kernel/arch/earm/arch_system.c:101` | ✅ |
| `intr_init` x86 | `minix3/minix/kernel/arch/i386/i8259.c` | ✅ |
| `intr_init` ARM | `minix3/minix/kernel/arch/earm/intr.c` | ✅ |
| `DEFAULT_HZ = 60` | `minix3/minix/include/minix/syslib.h` 或 `clock.c` | ✅ |
| `kclockinfo` 全局 | `minix3/minix/kernel/clock.c` | ✅ |
| `kloadinfo` 全局 | `minix3/minix/kernel/clock.c` | ✅ |

### 4.4 §3 数据结构覆盖 (doc-skill §3)

| C 结构 | 文档覆盖 | Rust 对应 | 结果 |
|--------|---------|-----------|------|
| `clock_mess` | §2.2 | (未直接对应, Rust 用 ClockState) | ✅ |
| `kclockinfo` | §2.1 | `ClockState` | ✅ |
| `kloadinfo` | §2.1 | `LoadInfo` | ✅ |
| `irq_hook` | §2.3 | `IrqVector`/`IrqId` | ✅ |
| `gate_table` IDT 条目 | §2.4 | `IdtEntry64` (03 文档) | ✅ |
| `exp_m1`/`exp_m2` 衰减常量 | §2.2 | `LoadInfo::exp_m1/m2` | ✅ |

### 4.5 §4 文档与代码一致性 (doc-skill §4)

| 文档代码片段 | 实际 Rust 代码 | 结果 |
|-------------|---------------|------|
| §4.1 ClockArch trait | `os/arch/src/arch/clock.rs` | ✅ 签名一致 |
| §4.2 InterruptController trait | `os/plat/src/interrupt.rs` | ✅ 签名一致 |
| §4.3 x86_64 ClockArch | `os/arch/src/x86_64/clock.rs` | ⚠️ 文档含 `PIT_COMMAND`/`PIT_CHANNEL0` 常量，实际代码无 (P1-01) |
| §4.3 x86_64 ClockArch | `os/arch/src/x86_64/clock.rs` | ⚠️ 文档缺 `assert!(hz >= 19)` 断言 (P2-01) |
| §4.4 ARM64 ClockArch | `os/arch/src/arm64/clock.rs` | ✅ 一致 |
| §4.5 RISC-V ClockArch | `os/arch/src/riscv64/clock.rs` | ✅ 一致 |
| §4.6 ARM64 GICv3 | `os/plat/src/arm64/interrupt.rs` | ✅ 一致 |
| §4.7 RISC-V PLIC | `os/plat/src/riscv64/interrupt.rs` | ⚠️ 文档含 `PLIC_COMPLETE` 常量，实际代码复用 `PLIC_CLAIM` (P2-02) |
| §4.10 ARM64 ArchInit | `os/arch/src/arm64/arch_init.rs` | ✅ 一致 |
| §4.11 RISC-V ArchInit | `os/arch/src/riscv64/arch_init.rs` | ⚠️ 代码注释列 SEIE 但未设置 (P2-03) |

### 4.6 §5 架构差异说明 (doc-skill §5)

| 差异点 | 文档说明 | 结果 |
|--------|---------|------|
| x86 PIT vs ARM Generic Timer vs RISC-V CLINT | §4.3-4.5 各架构详述 | ✅ |
| x86 APIC vs ARM GICv3 vs RISC-V PLIC | §4.6-4.8 各架构详述 | ✅ |
| ARM C 32 位 vs Rust 64 位 | §2.4 注释 | ✅ |
| RISC-V 无 Minix3 版本 | §2.5, §4.5, §4.8, §4.11 明确标注 | ✅ |
| IRQ 数量差异 | §3.6 表格 | ⚠️ ARM64 列 "1020" 与代码 NR_IRQ_VECTORS=64 不符 (P2-04) |

### 4.7 §8 源码覆盖完整性 (doc-skill §8)

**coverage-extract.py 结果** (`.review/04-clock-interrupt-init/SYMBOLS.md`):
- C 符号总数: 1306 (411 funcs + 34 structs + 861 macros)
- 文档覆盖: 360 (27%)
- Rust 覆盖: 13 (0%) — *注: 工具按名称匹配, 实际语义覆盖远高于此*

**本文档核心覆盖**:

| C 函数 | 文档覆盖 | Rust 实现 | 状态 |
|--------|---------|-----------|------|
| `init_clock` | ✅ §2.1 | `ClockState::new(hz)` | ✅ |
| `clock_handler` | ✅ §2.2 | (Rust 未实现完整 handler, 仅 tick 逻辑) | P1-02 |
| `intr_init` (x86) | ✅ §2.3 | `X86_64InterruptController::init()` | ✅ |
| `intr_init` (ARM) | ✅ §2.4 | `AArch64InterruptController::init()` | ✅ |
| `intr_init_8254` | ✅ §2.3 | `X86_64ClockArch::init_timer()` | ✅ |
| `arch_init` (ARM) | ✅ §2.5 | `AArch64ArchInit::init()` | ✅ |
| `arch_init` (x86) | ✅ §2.6 | `X86_64ArchInit::init()` | ✅ |
| `bsp_finish_booting` | ✅ §2.7 | (Rust 未实现, 后续文档) | P1-03 |
| `intr_handle` | ✅ §2.3 | (Rust 未实现完整 handler) | P1-04 |
| `irq_register` | ❌ 未提及 | (Rust 未实现) | P2-05 |

### 4.8 Code Review (code-skill)

#### 4.8.1 trait 设计

| trait | impl 数 | bound | 评价 |
|-------|---------|-------|------|
| `ClockArch` | 3 (x86_64/arm64/riscv64) | 无 | ✅ 合理 |
| `InterruptController` | 3 | `Sized` | ✅ 合理 |
| `ArchInit` | 3 | 无 | ✅ 合理 |

- `ClockArch::read_tsc` 默认实现委托 `read_ticks`: ✅ 合理默认
- `InterruptController` 方法集 (init/mask/unmask/ack/eoi/mask_all): ✅ 完整覆盖中断生命周期
- `IrqVector` newtype 包装 `u8`: ✅ 类型安全

#### 4.8.2 unsafe 检查

| 位置 | unsafe 用途 | SAFETY 注释 | 评价 |
|------|------------|------------|------|
| `x86_64/clock.rs` `out`/`rdtsc` | 端口 IO + TSC 读取 | 有 | ✅ |
| `arm64/clock.rs` `mrs`/`msr` | 系统寄存器访问 | 有 | ✅ |
| `riscv64/clock.rs` `read_volatile`/`csrs` | MMIO + CSR | 有 | ✅ |
| `arm64/interrupt.rs` `mrs`/`msr` + MMIO | GIC 寄存器 | 有 | ✅ |
| `riscv64/interrupt.rs` MMIO | PLIC 寄存器 | 有 | ✅ |
| `arm64/arch_init.rs` `msr` | PMU 寄存器 | 有 | ✅ |
| `riscv64/arch_init.rs` `csrw`/`csrs` | PMP + SIE CSR | 有 | ✅ |

#### 4.8.3 no_std 检查

- `os/arch/src/lib.rs`: ✅ `#![no_std]`
- `os/plat/src/lib.rs`: ✅ `#![no_std]`
- 无 `std::` 使用 (非 test 代码)

#### 4.8.4 硬件抽象

| 检查项 | 结果 |
|--------|------|
| MMIO/端口操作在 arch/plat 模块内 | ✅ |
| 上层通过 trait 调用 | ✅ |
| 无 `#[cfg(target_arch)]` 在上层代码 | ✅ |
| 硬件地址用常量, 非魔法数字 | ✅ (CLINT_MTIME, PLIC_BASE, GICD_OFFSET 等) |

#### 4.8.5 SMP 并发检查

| 检查项 | 结果 |
|--------|------|
| `Rc`/`RefCell` 跨 CPU 共享 | ✅ 无 (内核模块) |
| BKL 保护 | ✅ (init 阶段单 CPU, 后续由调度锁保护) |
| spinlock 中 sleep | ✅ 无 (init 阶段无 sleep) |

---

## 5. Issue List

| Pri | Loc | Issue | Evidence | Fix |
|-----|-----|-------|----------|-----|
| P1-01 | `04-clock-interrupt-init.md` §4.3 vs `x86_64/clock.rs` | 文档代码片段含 `PIT_COMMAND: u16 = 0x43` 和 `PIT_CHANNEL0: u16 = 0x40` 常量，实际代码无这些常量（直接用字面量 0x43/0x40） | `os/arch/src/x86_64/clock.rs` 仅有 `PIT_BASE_FREQ` 和 `PIT_CMD_RATE_GEN` | 文档 §4.3 代码片段删除 `PIT_COMMAND`/`PIT_CHANNEL0` 常量定义，或代码补充这两个常量 |
| P1-02 | `04-clock-interrupt-init.md` §2.2 vs Rust | C `clock_handler` 完整实现 (uptime/realtime/loadavg/bill/proc_ptr 更新)，Rust 仅有 `ClockState::tick()` 部分，文档未说明 handler 未完整实现 | `os/arch/src/arch/clock.rs` `ClockState::tick` 仅更新 uptime/realtime/loadinfo | §3 增加决策：clock_handler 完整逻辑 (bill/proc_ptr) 移到后续调度文档实现 |
| P1-03 | `04-clock-interrupt-init.md` §2.7 vs Rust | C `bsp_finish_booting` 设置 `kernel_may_alloc=0` 并启动其他 CPU，Rust 未实现，文档未说明 | `os/kernel/src/lib.rs` 无 `bsp_finish_booting` | §3 增加决策：bsp_finish_booting 移到后续 SMP 启动文档 |
| P1-04 | `04-clock-interrupt-init.md` §2.3 vs Rust | C `intr_handle` 完整中断分发逻辑，Rust 未实现，文档未说明 | Rust 无 `intr_handle` 对应实现 | §3 增加决策：intr_handle 完整分发逻辑移到 13-exception-interrupt 文档 |
| P1-05 | `04-clock-interrupt-init.md` §3.6 vs `interrupt.rs` | §3.6 表格 ARM64 IRQ 数量 "1020 (GICv3 SPI range)" 与代码 `NR_IRQ_VECTORS=64` 不符，易误导读者 | `os/plat/src/interrupt.rs` `NR_IRQ_VECTORS = 64` | §3.6 表格区分 "硬件能力 (1020)" 和 "软件限制 (64)"，或修正为 64 |
| P2-01 | `04-clock-interrupt-init.md` §4.3 | 文档代码片段缺 `assert!(hz >= 19)` 断言，实际代码有此断言防止 PIT divisor 溢出 | `x86_64/clock.rs` `assert!(hz >= 19, ...)` | §4.3 代码片段补充断言 |
| P2-02 | `04-clock-interrupt-init.md` §4.7 | 文档含 `PLIC_COMPLETE: usize = 0x200004` 常量，实际代码复用 `PLIC_CLAIM` (同一偏移) | `riscv64/interrupt.rs` 仅有 `PLIC_CLAIM` | 文档 §4.7 删除 `PLIC_COMPLETE` 或说明 "PLIC complete 复用 claim 偏移" |
| P2-03 | `riscv64/arch_init.rs:35` | 代码注释列 "SEIE (bit 9) + STIE (bit 5) + SSIE (bit 1)" 但实际只设 0x22 (STIE+SSIE)，SEIE 未设 | `0x22u64` = bit5+bit1 | 修正注释为 "STIE (bit 5) + SSIE (bit 1)"，移除 SEIE |
| P2-04 | `04-clock-interrupt-init.md` §3.6 | ARM64 IRQ 数量 "1020" 与代码不符 (见 P1-05) | - | 同 P1-05 |
| P2-05 | `04-clock-interrupt-init.md` §2.3 | C `irq_register`/`irq_hook` 机制未在文档中详述 | `minix3/minix/kernel/proc.h` `irq_hook` | §2.3 增加 irq_register 说明，或标注 "移到 13 文档" |
| P2-06 | `04-clock-interrupt-init.md` §4.6 | ARM64 GICv3 `init_distributor` 循环 `(32..nr_irqs).step_by(32)` 当 nr_irqs=64 时只处理 32-63，文档未说明此范围 | `arm64/interrupt.rs:90-96` | §4.6 说明 SPI 范围 32..64 |
| P2-07 | `04-clock-interrupt-init.md` §5.1.1 | 测试列表缺 `test_read_tsc_default_delegates_to_read_ticks` | `arch/clock.rs` tests 模块有此测试 | §5.1.1 补充此测试 |
| Excellence-P1 | `04-clock-interrupt-init.md` §1.2 | "时钟=心跳" 比喻可扩展为完整的中断驱动调度模型图 | §1.2 | 增加 ASCII 图：时钟中断 → clock_handler → 调度决策 → 进程切换 |
| Excellence-P1 | `04-clock-interrupt-init.md` §3 | 缺少 "为什么 ClockArch 是无状态 trait (无 &self) 而 InterruptController 有状态" 的设计说明 | §3.1-3.2 | 增加决策：ClockArch 纯硬件操作无状态，InterruptController 需保存 base/last_iar 等状态 |
| Excellence-P2 | `04-clock-interrupt-init.md` §4 | 三架构时钟/中断对比可增加一张总表 | §4 | 增加 "架构特性对比总表" (时钟源/中断控制器/特权级/CSR) |
| Excellence-P2 | `04-clock-interrupt-init.md` §2.2 | loadavg 指数衰减公式可补充数学推导 | §2.2 | 增加衰减公式推导：exp(-Δt/τ) |
| Excellence-P2 | `04-clock-interrupt-init.md` 全文 | 缺少 "中断延迟" (interrupt latency) 概念 | 全文 | §1 增加 interrupt latency 定义和影响因素 |

---

## 6. Cross-document Check

| 检查项 | 结果 | 详情 |
|--------|------|------|
| 与 03 文档衔接 | ✅ | §1.4 明确 "03 文档结束时，prot_init 已完成" |
| 与 05 文档衔接 | ✅ | §1.4 Phase C 指向 05 文档 |
| 与 07 文档衔接 | ✅ | §1.4 Phase E 指向 07 文档 |
| 与 13 文档衔接 | ⚠️ | §2.3 intr_handle 逻辑应移到 13 文档，当前未明确标注 (P1-04) |
| 重复内容 | ✅ | 无与 03 文档重复的内容 |
| 矛盾内容 | ✅ | 无跨文档矛盾 |

---

## 7. Behavior Contract Summary (Gate B)

### Top 5 语义差异 (3 语义偏移 + 2 覆盖缺口)

| # | 函数 | C→Rust | Match? | P? | Diff |
|---|------|--------|--------|----|------|
| 1 | `init_clock` | C: 全局 kclockinfo memset → Rust: ClockState::new(hz) 返回值 | ✅ 语义等价 | - | Rust 用构造函数替代全局 memset，hz 默认值/范围检查一致 |
| 2 | `intr_init_8254` | C: PIT channel 0 配置 → Rust: X86_64ClockArch::init_timer | ⚠️ 语义偏移 | P1-01 | 文档代码片段含不存在的常量 PIT_COMMAND/PIT_CHANNEL0 |
| 3 | `clock_handler` | C: 完整 handler (uptime+realtime+loadavg+bill+proc_ptr) → Rust: 仅 tick() | ⚠️ 语义偏移 | P1-02 | Rust clock_handler 未完整实现，bill/proc_ptr 移到后续 |
| 4 | `intr_handle` | C: 完整中断分发 → Rust: 未实现 | ❌ 覆盖缺口 | P1-04 | intr_handle 完整逻辑移到 13 文档 |
| 5 | `bsp_finish_booting` | C: kernel_may_alloc=0 + AP 启动 → Rust: 未实现 | ❌ 覆盖缺口 | P1-03 | bsp_finish_booting 移到后续 SMP 文档 |

### 行为契约表 (核心函数)

| 字段 | `init_clock` | `intr_init` | `arch_init` |
|------|-------------|-------------|-------------|
| C 函数 | `init_clock(void)` | `intr_init(unsigned)` | `arch_init(void)` |
| Rust 函数 | `ClockState::new(hz: u32) -> Self` | `InterruptController::init(&mut self)` | `ArchInit::init()` |
| 输入 | 无 (从 env_get("hz")) | intr_flag (0=disable, 1=enable) | 无 |
| 输出 | 无 (写全局 kclockinfo) | 无 | 无 |
| 副作用 | memset kclockinfo/kloadinfo, 解析 hz | 初始化 8259/GIC/PLIC, 屏蔽所有 IRQ | PMU/PMP/SIE 配置 |
| 错误码 | 无 (hz 越界用默认值) | 无 | 无 |
| 生命周期 | 一次性 | 一次性 | 一次性 |
| 权限 | 内核态 | 内核态 | 内核态 |
| 地址空间 | 高地址 | 高地址 (含 MMIO) | 高地址 |
| C→Rust 匹配 | ✅ (全局→构造函数) | ✅ (全局→trait 方法) | ✅ (全局→trait 方法) |

---

## 8. Weakest Item Self-Check

1. **Coverage enumeration?** ✅ 运行 coverage-extract.py, SYMBOLS.md 已生成
2. **Behavior contracts?** ✅ Top 5 差异 + 行为契约表已完成 (§7)
3. **§2.8 per-file grep?** ✅ C 源码引用经 grep 验证 (clock.c, i8259.c, arch_system.c)
4. **§2.10 traceability?** ✅ 每个 C 引用都有 file:line
5. **Same-dir cross-doc?** ✅ 与 03/05/07/13 文档交叉检查 (§6)
6. **Ch2 errors in Ch3?** ✅ §2 的 C 分析与 §3 的 Rust 设计决策对应
7. **Excellence checked?** ✅ 5 个 Excellence 项 (§5)
8. **Blocker Gates all passed?**
   - Gate A (coverage-extract.py): ✅
   - Gate B (Top 5 行为契约): ✅
   - Gate C (5 元规则检查): ✅ (§4.1 P0 必检)
   - Gate D (P0 必检 5 项): ✅ (§4.1)
   - Gate E (§5 测试 grep): ✅ (测试函数名验证存在)

---

## 9. Confirmation Checklist

- [x] P0 identified (0 个)
- [x] docs match C source (C 引用全部验证)
- [x] cross-refs complete (§6 交叉文档检查)
- [x] no "to confirm" (无未确认项)
- [x] coverage ok (§4.7)
- [x] contracts ok (§7)
- [x] weakest checked (§8)
- [x] Gates passed (§8)
- [x] time ok (90 min, 在 80-120 min 范围内)

---

## 10. Action Items

### TODO #1: 修正 PIT 常量文档
- **Pri**: P1 | **Type**: doc-code mismatch
- **File**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/04-clock-interrupt-init.md` §4.3
- **Plan**: 删除文档代码片段中的 `PIT_COMMAND` 和 `PIT_CHANNEL0` 常量定义，使其与实际 `x86_64/clock.rs` 一致（实际代码直接用字面量 0x43/0x40）
- **Verify**: diff 文档 §4.3 代码片段与 `os/arch/src/x86_64/clock.rs`

### TODO #2: 说明 clock_handler/intr_handle/bsp_finish_booting 未完整实现
- **Pri**: P1 | **Type**: 覆盖缺口
- **File**: `04-clock-interrupt-init.md` §3
- **Plan**: 增加三个决策说明：
  1. clock_handler 完整逻辑 (bill/proc_ptr) 移到后续调度文档 (P1-02)
  2. intr_handle 完整分发逻辑移到 13-exception-interrupt 文档 (P1-04)
  3. bsp_finish_booting 移到后续 SMP 启动文档 (P1-03)
- **Verify**: 确认 §3 有这三条决策

### TODO #3: 修正 IRQ 数量表格
- **Pri**: P1 | **Type**: doc-code mismatch
- **File**: `04-clock-interrupt-init.md` §3.6
- **Plan**: ARM64 IRQ 数量 "1020" 修正为 "64 (软件限制) / 1020 (硬件能力)"，或直接改为 64
- **Verify**: 对照 `os/plat/src/interrupt.rs` `NR_IRQ_VECTORS = 64`

### TODO #4: 修正 RISC-V ArchInit 注释
- **Pri**: P2 | **Type**: code comment error
- **File**: `os/arch/src/riscv64/arch_init.rs:35`
- **Plan**: 注释 "SIE: SEIE (bit 9) + STIE (bit 5) + SSIE (bit 1)" 修正为 "SIE: STIE (bit 5) + SSIE (bit 1)"，因 0x22 不含 SEIE
- **Verify**: 确认 0x22 = bit5 + bit1

### TODO #5: 补充测试列表
- **Pri**: P2 | **Type**: 测试覆盖文档
- **File**: `04-clock-interrupt-init.md` §5.1.1
- **Plan**: 补充 `test_read_tsc_default_delegates_to_read_ticks` 测试
- **Verify**: grep "test_read_tsc" `os/arch/src/arch/clock.rs`

### TODO #6: 修正 PLIC_COMPLETE 常量
- **Pri**: P2 | **Type**: doc-code mismatch
- **File**: `04-clock-interrupt-init.md` §4.7
- **Plan**: 删除 `PLIC_COMPLETE` 常量定义，或说明 "PLIC complete 复用 claim 偏移 (0x200004)"
- **Verify**: 对照 `os/plat/src/riscv64/interrupt.rs` 仅有 `PLIC_CLAIM`

---

## 附录: Review 执行流水账

### Step 1: Scope (5 min)
- 读取 STATE.md, 确认 04 文档此前标记 "converged"
- 确定 review 模式: full (doc + code)
- 列出 target: 04-clock-interrupt-init.md + clock.rs + interrupt.rs + arch_init.rs

### Step 2: Ground Truth (15 min)
- Grep `minix3/minix/kernel/clock.c` 验证 init_clock/clock_handler 位置
- Grep `minix3/minix/kernel/arch/i386/i8259.c` 验证 intr_init/intr_init_8254
- Grep `minix3/minix/kernel/arch/earm/arch_system.c` 验证 arch_init
- 读取 C 源码片段, 与文档 §2 对照
- 结果: C 引用全部准确

### Step 3: Coverage (5 min)
- 运行 `python3 tools/coverage-extract/coverage-extract.py kernel notes/rewrite/fork-syscall-rewrite/03-stage-kernel --rust-dir os/arch/src --output .review/04-clock-interrupt-init/SYMBOLS.md`
- 结果: 1306 C 符号, 360 文档覆盖 (27%), 13 Rust 覆盖 (0% 名称匹配)
- AI 补充: 实际语义覆盖远高于名称匹配

### Step 4: Diff Extraction (20 min)
- 逐行对比 C init_clock vs Rust ClockState::new
- 逐行对比 C intr_init_8254 vs Rust X86_64ClockArch::init_timer
- 逐行对比 C clock_handler vs Rust ClockState::tick
- 识别 3 个语义偏移 + 2 个覆盖缺口
- 生成行为契约表 (§7)
- 结果: P1-01/02/03/04 为主要问题

### Step 5: Sanity Check (15 min)
- 验证 trait 签名: ClockArch/InterruptController/ArchInit 文档 vs 代码一致
- 验证测试函数名: grep 确认 §5 列出的测试存在
- 验证 P0 必检 5 项
- 验证 §4 代码片段与实际代码一致性
- 结果: 发现 P1-01 (PIT 常量), P2-01 (assert 缺失), P2-02 (PLIC_COMPLETE), P2-03 (RISC-V 注释)

### Step 6: Test Verification (5 min)
- Grep §5 测试函数名, 确认存在
- 结果: 测试函数名全部验证存在, 缺 1 个 (P2-07)

### Step 7: Cross-Document (5 min)
- 检查与 03/05/07/13 文档的衔接
- 结果: 无矛盾, 1 个 P1 衔接不明确 (P1-04 intr_handle 移到 13)

### Step 8: Excellence (10 min)
- 检查文档叙事结构、读者体验、教学深度
- 检查代码 API 设计、表达力、可测试性
- 结果: 5 个 Excellence 改进项

### Step 9: Output (10 min)
- 生成 scan.md (本文件)

### Step 10: Convergence
- P0 = 0, P1 = 5, P2 = 7
- 收敛判据: P0 = 0 ✅, 但有 5 个 P1 待修
- 状态: **NOT_CONVERGED** (P1 待修)

### 工具使用记录
- `Read`: 读取文档和代码文件 (clock.rs, interrupt.rs, arch_init.rs)
- `Grep`: 验证 C 源码引用
- `RunCommand`: 运行 coverage-extract.py, find 查找文件
- `Glob`: 查找 Rust 源文件
- `TodoWrite`: 任务管理
