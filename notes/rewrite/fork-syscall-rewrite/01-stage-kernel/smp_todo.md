# SMP Bring-up 计划与设计（smp_todo.md）— 自包含版

> **═══ FROZEN — implementation starts at S-0 ═══**（2026-09-06，九轮外评 [smp_gpt.md/
> smp_gpt_v2/v3 + 四轮对话框直传] 收敛，P0: 0，待改设计的 P1: 0；v9 三 nit 已清：
> L2 解锁表述 / PLIC-IPI 术语边界 / consumer-side ordering barrier 更名）
> **自本标记起 review 规则变更**：不再重新讨论 SMP 总体架构；只 review 当前 S-x 的代码、
> C 对齐证据、体系结构 ABI 与该步骤的验收结果。发现的设计问题按 S-x 实施问题处理，
> 局部回退/修正，不重开总体设计。

> **这份文档是写给谁看的**：本文档自包含，供无法访问代码库的外部 reviewer 阅读（例如网页聊天式 AI）。
> 文中所有代码事实都以"路径 + 关键行为描述 + 行号锚点"的形式内嵌；C 语言源码片段直接引用原文。
> 读者只需具备操作系统与体系结构的通用知识（BSP/AP、页表、中断控制器等概念不另行科普），
> 项目特有的概念在 §0.3 术语表中给出定义。

> **目标**：让 Minix-RS 内核在 QEMU（`-smp 4`，四核配置）下真正跑满四个 CPU，三架构
> （x86_64 / aarch64 / riscv64）SMP 语义齐备，并补齐 QEMU 集成测试基建。
> 这份工作完成后，原 todo.md 中 "Edge Items" 表的全部 12 项即可逐项解锁。
>
> **计划日期**：2026-09-06。**执行方式**：单线程，一次一个 TODO，每步一个 git commit。
> **C 源码 ground truth**（本项目最高权威）：`minix3/minix/kernel/smp.c`（205 行，SMP 核心机制）、
> `minix3/minix/kernel/main.c:311`（`smp_init()` 在启动序列中的位置）、
> `minix3/minix/kernel/arch/i386/arch_smp.c`（x86 AP 启动的体系结构相关部分）。

---

## 0. 背景与术语

### 0.1 项目是什么

Minix3 是一个经典的教学微内核操作系统（Tanenbaum 等人，C 语言，2000 年代，x86 32 位为主）。
Minix-RS 是它的 Rust 语义重写：**保持外部可观察行为不变，内部用 Rust 类型系统重新表达**——
这不是逐行翻译（translate），而是重写（rewrite）。核心工程约束：

- `#![no_std]`：内核不依赖标准库，无堆分配（固定大小数组 + 侵入式链表）。
- 三架构统一：x86_64 / aarch64 / riscv64 共用一套内核代码，体系结构差异收敛在 `os/arch/` crate
  的 trait 抽象后面（例如 `SmpArch` trait 定义"如何启动一个 AP"的接口，三个架构各有一个实现）。
- 微内核：内核只保留调度 / IPC / 低级系统调用 / 中断分发；文件系统、进程管理、虚拟内存、
  根服务都跑在用户态服务器里。**本计划的所有工作都在内核与 arch 层内部，不依赖任何用户态服务器**。
- 并发模型：**BKL（Big Kernel Lock，大内核锁）+ 单核退化**。当前代码在单 CPU 上运行且全部
  内核临界区已被 BKL 串行化（详见 §3.6）；SMP bring-up 的本质是"让更多 CPU 进入这个已被
  BKL 保护的世界"，而不是重写并发模型。这与 C 版 Minix3 的设计完全一致（C 源码里就叫
  `big_kernel_lock`，smp.c:27）。

### 0.2 验收环境

QEMU 8.2.2（三架构模拟器均已安装），统一 `-smp 4`（四核）。启动链：
x86_64 走 UEFI（OVMF 固件），aarch64 走 UEFI（AAVMF 固件），riscv64 走 OpenSBI 固件
（`-bios default`）。测试判据：测试内核在串口输出 `### TEST_RESULT: PASS <名字> ###`，
运行脚本 grep 该标记判定通过。

### 0.3 术语表

| 术语 | 定义 |
|---|---|
| BSP / AP | Boot Processor（启动 CPU，上电后第一个跑固件的核）/ Application Processor（其余的核，由 BSP 唤醒） |
| BKL | Big Kernel Lock：一把全局自旋锁，内核所有临界区共用。本项目用 `AtomicBool` + CAS 实现，配 RAII guard |
| IPI | Inter-Processor Interrupt：一个 CPU 发给另一个 CPU 的中断，SMP 协调的基本手段 |
| per-CPU 数据 | 每个 CPU 私有、无需加锁的状态（当前栈、当前进程、统计）。Rust 侧容器叫 `CpuLocal` |
| SmpArch trait | arch 层接口：`boot_ap(hw_id, entry_pa, …)`（唤醒一个 AP——目标参数为硬件号，签名随 S-3/S-5 演进，见 §3.2/§3.4）、`send_sched_ipi(cpu)`（发调度 IPI）、`ack_ipi`（中断确认）、`halt_cpu`、`pause` |
| CpuTopology | 平台层解析固件表得到的三架构统一结构：`{ nr_cpus, bsp_id, cpus: [CpuInfo; 32] }`，每个 `CpuInfo` 含 `hw_id`（x86 的 APIC ID / ARM 的 MPIDR / RISC-V 的 hart ID）与 per-CPU 设备基址（GICR、mtimecmp） |
| MADT / DTB | x86 的 ACPI Multiple APIC Description 表（列出每个 LAPIC）/ 设备树（ARM 与 RISC-V 固件传递硬件描述的标准格式），二者的 Rust 解析都已存在 |
| PSCI | ARM Power State Coordination Interface：固件标准接口，`CPU_ON` 调用（功能号 0x84000003，经 `hvc #0` 指令陷入）可让固件启动一个副核，入口地址与上下文指针由调用者给定 |
| SBI HSM | RISC-V SBI 规范的 Hart State Management 扩展：`hart_start(hartid, start_addr, opaque)` 经 `ecall` 陷入 OpenSBI，语义同 PSCI CPU_ON |
| INIT-SIPI-SIPI | x86 AP 启动协议：BSP 向目标 LAPIC 写 ICR 寄存器发 INIT IPI（复位该核），随后发 Startup IPI（SIPI），AP 从**16 位实模式**、物理地址 `vector << 12`（必须 < 1 MiB 且页对齐）开始执行。这是 Intel SDM 规定的硬件契约，无法绕开 |
| higher-half kernel | 内核链接并运行在高虚拟地址（如 0xFFFF_xxxx_xxxx_xxxx），物理内存却加载在低位；分页把两者映射起来 |
| HigherHalf trait | 本项目已有的 BSP 侧跳转抽象：分页开启后从低地址恒等映射切栈并跳到高地址 kmain。**它与本计划的 AP 入口代码是两个不同的东西**（§3.2 详述） |
| global_asm! | Rust 官方的全局汇编机制：在普通 Rust 源文件内写整段顶层汇编（可用 `.section` 自定义段）。本计划的 AP early entry image 用它表达——它就是"一段机器码实体"，不是 Rust 函数，没有 ABI 假象；拷贝边界由链接器段符号界定 |

---

## 1. 目标与范围

### 1.1 要解锁的 12 项 Edge Items

todo.md 的 "Edge Items" 表（2026-09-06 扫描追加）列出了全部无法在单核 hosted 环境（普通
`cargo test`）下完成的残余项。每项一句话解释：

| # | 项 | 含义 |
|---|---|---|
| 1 | D-36 `smp_init` | SMP 启动编排：解析拓扑、逐个唤醒 AP、等待全员就位（§3.4） |
| 2 | D-37 `boot_lock` | AP 启动临界区的专用自旋锁（§3.4） |
| 3 | D-38① 异常入口 BKL | CPU 进入异常处理前先拿大内核锁（依赖 §3.7 的 asm 入口） |
| 4 | D-38④ kernel_call_resume BKL | **疑似已完成**（§2.3 勘误，S-1 核实） |
| 5 | D-39 x86_64 `init_ap` 占位 | AP 侧 per-CPU 初始化现在是 panic 占位（§3.3） |
| 6 | D-40 ptproc per-CPU | "当前页表宿主进程"跟踪从全局变量迁到 per-CPU（§3.5） |
| 7 | D-41 PLATFORM 并发化 | 平台描述全局的容器假设单线程，AP 并发前需换原子/锁容器（§3.5） |
| 8 | sched-1 per-CPU 调度访问 | 调度器队列操作的 per-CPU 语义（§3.5） |
| 9 | tick-1 per-CPU tick 统计 | 内核 tick 计数的 per-CPU 语义（§3.5） |
| 10 | hw-1 硬件半环 | x86 中断真正到达 CPU 的最后一环：asm 陷阱入口（§3.7） |
| 11 | hw-2 shutdown(0) | 优雅关机（§3.8） |
| 12 | trap-1 系统调用入口接线 | asm 层把系统调用入口接到 Rust 分发函数（§3.7） |

### 1.2 Stage 边界声明（为什么这些都不是 edge）

Minix-RS 按 Minix3 的服务边界划分 stage（kernel / VM / RS / VFS / PM 各一个 stage）。
微内核原则下各 stage 独立开发、只在联合调试时相遇。上述 12 项全部是**内核 stage 内部的
生产代码**：AP 启动、BKL、per-CPU、IPI、调度原语、关机——没有任何一项需要用户态服务器
存在才能实现或测试。真正的 edge 项只有两类（见 §7）：需要用户态服务器真实运行的联合调试
测试，以及改变了跨 stage 可见语义的接线。执行中若出现，追加到 `edge_todo.md` 单线程处理。

---

## 2. 现状盘点（2026-09-06 逐条核实）

### 2.1 已经存在的资产（不需要重做）

**拓扑发现（D-36 的上半，已完成）**
`os/libs/minix-platform/src/acpi.rs:261`（`find_madt`，在 XSDT/RSDT 中定位 MADT）与 `:318`
（`parse_madt`，遍历 MADT 条目产出 LAPIC 基址、I/O APIC、中断数、每 CPU 硬件 ID 列表、
CPU 总数、BSP 的硬件 ID）。aarch64 与 riscv64 走设备树：`os/libs/minix-platform/src/device_tree.rs:397`
解析 DTB 的 cpu 节点产出同一 `CpuTopology` 结构。内核已在消费它：
`os/kernel/src/lib.rs:565` 调 `platform_desc().cpu_topology()` 构建 `SmpState::with_ncpus(topo.nr_cpus, topo.bsp_id)`
——即"知道有几个核"这件事已经打通，缺的只是"让那些核真的跑起来"。

**LAPIC 已使能（x86 发 IPI 的前置条件）**
x86 的本地 APIC 复位后处于软件禁用态，必须向 Spurious Interrupt Vector 寄存器（偏移 0x0F0）
的 bit 8 写 1 才能收发中断。这一步已存在：`os/plat/src/x86_64/interrupt.rs:119` 在平台中断
控制器初始化时写 `SVR | LAPIC_SVR_ENABLE`。所以 §3.2 的 INIT/SIPI 发送端没有缺项。

**SmpArch trait 三架构真实现**（不是占位，逐一核实过）

| 架构 | `boot_ap` 实现 | 位置 |
|---|---|---|
| x86_64 | 完整 INIT-SIPI-SIPI 序列：ICR 高半寄存器写目标 APIC ID、低半写 INIT；再写 SIPI（vector = entry >> 12），200µs 后补发第二次 SIPI（Intel SDM §8.4 的可靠性建议）；入口地址有"页对齐且 < 1 MiB"断言 | `os/arch/src/x86_64/smp.rs:228` |
| aarch64 | PSCI `CPU_ON` 经 `hvc #0`——**v6 #2：现存代码用 32 位变体 0x84000003（`PSCI_CPU_ON_32`，arm64/smp.rs:59）**，SMC32 调用约定下 TF-A 把 x1-x3 截断为 32 位，entry/context ≥4GiB 即静默损坏（QEMU 默认布局 <4GiB 恰好掩盖）；conduit（hvc/smc）与位宽（FID bit 31）正交，原注释混为一谈。修复 = 换 **`PSCI_CPU_ON_AARCH64 = 0xC4000003`**（64 位参数，列入 S-3c）；注释保留的既有问题：线性 CPU 号当 MPIDR，真机需经拓扑表翻译（§3.2 改进点） | `os/arch/src/arm64/smp.rs:221` |
| riscv64 | SBI HSM `hart_start`：ecall，a0=hartid、a1=start_addr、**a2=opaque**（无特权级参数——hart 恒以 S-mode 启动，satp=0、SIE=0，AP 在 a0=hartid、a1=opaque 醒来）。**现存代码注释误标 a2 为 "priv"**（SBI v0.1 草案 API 的陈旧理解，ratified 规范无此参数；当前传 0 碰巧合法即 opaque=0，但 S-3 必须改传 bootstrap 指针并修正注释，见 §3.2/§3.9） | `os/arch/src/riscv64/smp.rs:155` |

**内核 SMP 机制全量就位**：`os/kernel/src/smp.rs`（1632 行，28 个单元测试）已实现
`schedule_sync`（同步 IPI：设标志→发 IPI→自旋等对方清零）、`schedule_stop_proc` /
`schedule_vminhibit` / `schedule_migrate_proc`、IPI 处理函数 `smp_sched_handler` /
`ipi_sched_handler` / `ipi_halt_handler`、`wait_for_aps`、BKL 全家（`AtomicBool` CAS、
`BklGuard` RAII 守卫、`BklSection` 类型见证、`*_with()` 持锁访问器——Rust 惯用法：
把"我持有锁"编码成类型，编译期强制）。`MAX_CPUS = 32`（对齐 C `CONFIG_MAX_CPUS`）。
系统调用路径的 BKL 已接线：入口 `os/kernel/src/syscall.rs:411/566` 获取，出口 `:2580/2605`
释放，恢复路径 `:2622` 重入。

**BSP higher-half 跳转（与 AP 入口无关，勿混淆）**
`os/kernel/src/arch/{x86_64,aarch64,riscv64}/higher_half.rs` + `os/kernel/src/boot/higher_half.rs`
实现了 `HigherHalf` trait：BSP 在分页开启后从低地址恒等映射切栈并远跳到高地址 kmain。
三个实现都是十几条内联汇编。设计文档 doc 02 §3.3 记录了为什么**不用**独立汇编文件
（asm! 寄存器控制足够精确 + trait 跨架构 + 类型安全三条理由），并有 `test-higher-half`
QEMU 测试。trait 契约明写 "called exactly once, from the boot CPU"——它只服务 BSP。

**x86 IDT 基建大部分就位**
`os/arch/src/x86_64/trap_entry.rs`（526 行）：IDT 门表按 C `protect.c:107-152` 的配置填充
（异常向量 0-14/16-19 的 DPL 与 IST 槽位、系统调用/IPC 向量 32-35 的 DPL=3、8259A PIC
向量 0x50-0x57/0x70-0x77）、`lidt` 加载（:303）、SYSCALL/SYSRET 的 MSR 配置
（STAR/LSTAR/SFMASK/EFER.SCE，:230-260）、门表存在性/DPL 的单元测试。启动时执行了
`init()`（表元数据 + SYSCALL MSR，`os/kernel/src/lib.rs` `init_protection`）。
**但是（v2 外评核实的精确状态）**：所有门的 handler 字段是 0 占位（`:186` 自注
"Handler addresses are placeholder 0 — must be set to actual trap handler entry points
before load() is called"），且 `init_protection` 自注 **"Do NOT call trap.load() here"**
——`lidt` 从未执行，真 IDT 属于"后续 boot 阶段"（装 handler → `set_handler()` →
`load()`），那正是 S-8 的工作。asm 陷阱入口本体缺失（§3.7）。当前环境零中断（时钟
中断被屏蔽、没有用户态发起系统调用、无 IDT 在位），所以
占位从未被触发。

**QEMU 测试基建**
`os/qemu-tests/run_qemu.sh`：单测试运行器。构建 FAT 磁盘镜像塞入 EFI 可执行文件 →
按架构选固件（OVMF / AAVMF / RISCV_VIRT，riscv64 对非 EFI 内核回落 `-bios default` 即
OpenSBI）→ QEMU 参数三架构统一含 `-smp 4`（脚本 :101/:125/:145/:156）→ 串口重定向到
日志文件 → **`timeout 30` 硬性杀进程**（:172，这是历史上 QEMU 卡死问题固化的解法）→
grep 串口日志中的 `### TEST_RESULT: PASS`（:186）判定。另有 `run_all.sh` 批量跑 7 个
x86_64 测试内核（hello-boot / test-memmap / test-paging-enable / test-kernel-map /
test-higher-half / test-protection / test-proc-init）及 aarch64、riscv64 变体。
2026-09-06 已实测验证：hello-boot x86_64 端到端 PASS。

### 2.2 真正缺的（本计划的工作量，三架构分列)

| 缺口 | x86_64 | aarch64 | riscv64 |
|---|---|---|---|
| **AP early entry**（§3.2） | **最重**：INIT-SIPI-SIPI 把 AP 放回 16 位实模式，需 16→32→64 位完整模式梯子 + <1MiB 低内存落点 + mailbox。全仓库搜索 `.code16` / BDA 0x467 均零命中——16 位代码一处都不存在 | 轻：PSCI 进入时 EL1 + MMU 关（a0=hartid、a1=opaque），stub 装 TTBR/栈/VBAR 后跳 `ap_early_entry` | 轻：SBI 进入时 S-mode + MMU 关（a0=hartid、a1=opaque，satp=0、SIE=0），stub 写 satp + sfence.vma + 栈后跳 `ap_early_entry` |
| **init_ap per-CPU 初始化**（§3.3） | per-CPU GDT（内嵌本 CPU TSS 描述符）+ ltr + 共享 IDT 的 lidt + WRMSR GS_BASE + 栈切换。API 骨架已在（`protection.rs:368` `init_ap(&self, cpu_id, kernel_stack_top)`，现为 panic 占位） | VBAR_EL1 指向共享异常向量 + TTBR/MAIR/TCR 复用 BSP 值 + GICR redistributor 唤醒 | stvec 指向共享向量 + satp 复用 BSP（SSIE 能力准备、置位推迟 S-10；PLIC context 移出 init_ap，归 S-8/S-10 的外部 IRQ 路径——v8 #3） |
| **smp_init 编排 + boot_lock**（§3.4） | 三架构共用同一编排（kernel 层遍历拓扑 + 调 `SmpArch::boot_ap` + `wait_for_aps`），架构差异全部压在 trait 后面 | 同左 | 同左 |
| **per-CPU 化四件套**（§3.5） | 三架构共用：`CURRENT_PTPROC_NR` 全局 → `CpuLocal`；`PLATFORM` 容器并发化；调度器 per-CPU 访问；tick 统计 per-CPU | 同左 | 同左 |
| **AP 主循环 + BKL 交接**（§3.6） | 三架构共用 | 同左 | 同左 |
| **asm 陷阱入口**（§3.7） | IDT 门表就绪，缺 stub 本体与分流 | VBAR/stvec 结构存在，stub 盘点留 S-8 开工（本轮未逐行核验，如实标注） | 同左 |
| **shutdown(0)**（§3.8） | 三架构共用 trait + QEMU 后端（isa-debug-exit 端口写） | 同左 | 同左 |

### 2.3 两处勘误（S-1 先核实再动笔）

1. **D-38④ 疑似已完成**：doc 16 §4.13 的 BKL 接入表记 `kernel_call_resume` 为 "✅ 已接入
（syscall.rs:2622）"，而 todo.md Edge Items 表（同日 2026-09-06）仍标 DEFERRED。两处矛盾。
按 fix-guard 纪律（先 grep 现状再修），以代码为准勘误其中一方。
2. **hw-1 范围比 Edge 表写的窄**：Edge 表原文 "asm IRQ stub + IDT load + 入口分流"——
实际 IDT 门表、`lidt`、SYSCALL MSR 都已存在并接线（§2.1 末两条），真缺的只有 asm stub
本体 + 跳 Rust 的分流。本计划 S-8 按收窄后的范围执行。

### 2.4 CI 盲区（本次计划的方法论动机）

`os/kernel/Cargo.toml` 的默认 feature 是 `["mock"]`——hosted 环境（Linux 上跑
`cargo test`）用 mock 后端替代真实硬件，此时生产路径的 `kmain`（`lib.rs:369`，cfg 门控
`not(mock) + not(qemu_test)`）**根本不参与编译**。而 QEMU 测试内核以
`default-features = false` 依赖内核 crate，才会编译生产路径。

这个盲区已经咬过一口：2026-09-06 落地的 D-36/D-46 提交在生产路径里漏了两个 import
（`lib.rs:565` 的 `platform_desc`、`:2196` 的 `Endpoint`），hosted 全套 691 个测试绿灯
无感知，UEFI/QEMU 构建直接挂掉（已修复，两个 import 补在工作区待 S-0 提交）。
**方法论结论：凡触碰生产路径的 kernel 改动，验证清单必须包含 UEFI/QEMU 构建与对应
QEMU 测试**（§4 步骤 5 已固化此要求）。

---

## 3. 核心设计

### 3.1 三架构 AP 启动协议对照（硬件契约层）

| | x86_64 | aarch64 | riscv64 |
|---|---|---|---|
| 唤醒机制 | BSP 写 LAPIC ICR：INIT IPI 复位目标核 → SIPI 让它从 `vector<<12` 开始 | 固件调用 PSCI `CPU_ON`（`hvc #0`） | SBI `hart_start`（`ecall`） |
| AP 第一条指令的位置 | 物理 `vector << 12`，**必须 < 1 MiB 且页对齐**（硬件约束） | 固件指定的任意物理入口 | 固件指定的任意物理入口 |
| 进入时的模式 | **16 位实模式**（INIT 把核复位到上电态） | EL1，MMU 关 | S-mode，MMU 关 |
| BSP→AP 传值通道 | 无寄存器可用（SIPI 只给一个 vector 号）——**bootstrap 内嵌在 early-entry image 数据区固定偏移**（C 的 `__ap_id` 方案的 modern 等价物，v5 设计） | PSCI `context_id` → AP 的 x0，可传指针 | SBI `opaque` → AP 的 a1，可传指针 |
| 页表怎么来 | early-entry image 内嵌 bootstrap 的 `page_table_root_pa` 字段 → 装 CR3（BSP 页表含恒等映射，模式梯子期间可用） | stub 写 TTBR0/TTBR1 = BSP 值，置 SCTLR.M | stub 写 satp = BSP 值，sfence.vma |
| 目标核号 | ICR 目的域填 APIC ID → 用 `CpuInfo.hw_id` | PSCI x1 填 MPIDR → 用 `CpuInfo.hw_id` | hart_start a0 填 hartid → 用 `CpuInfo.hw_id` |

关键不对称：**只有 x86_64 需要 <1MiB 低内存**（实模式寻址上限）；ARM/RISC-V 的入口可以
就是内核镜像里的一段位置受限代码，无低内存问题。

### 3.2 AP early entry 设计（本计划最难的单件；2026-09-06 按 GPT 外评修订）

**命名与概念（修订）**：doc 01/02 里的 "trampoline" 指 BSP 的 higher-half 跳转（`HigherHalf`
trait，已实现已测试，§2.1）。x86 AP 被 INIT-SIPI-SIPI 放回 16 位实模式是硬件契约，绕不开
（C 版 Minix3 由 `arch_smp.c` 的 trampoline.S 承担：16 位代码 + `__ap_id` 邮箱，
`copy_trampoline()` 拷到 <1MiB，arch_smp.c:68-95）。但本项目**不为此新增叫 "trampoline"
的抽象层**——不建 Trampoline trait、不加 trampoline.S、不复制 HigherHalf 模式。那段代码的
真实身份是"**由 Rust 构建链携带的体系结构私有早期启动代码段**"（AP early entry image），
是 arch 内部实现细节，不进公共接口。工程原则一句话：**删掉"trampoline 文件"，保留"硬件
要求的那一小段机器码"**。三架构汇合点不是汇编，而是同一个 Rust 函数：

```rust
unsafe fn ap_early_entry(bootstrap_pa: usize) -> !   // 三架构共同的 Rust 入口；收物理地址
// 汇编边界只传 PA；Rust 侧内部经 DirectMapArch::kernel_phys_to_virt 转 VA 后构建 &ApBootstrap
```

（历史注：本项目早期曾试写过完整汇编 trampoline 后删除、未提交进 git，最终只保留
`HigherHalf`——本节设计是对那次演进结论的正式化。）

**方案候选（修订）**：

- **方案 A（倾向）**：`global_asm!` 在 arch crate 内定义 `.ap_early_entry`（代码，只读可执行）
  与 `.ap_early_entry_data`（邮箱等可写数据）两个段，构建期由内核拷贝到低内存。`global_asm!`
  是 Rust 官方的全局汇编机制，语义诚实——它就是"一段机器码实体"，**不是函数**。此前倾向的
  `#[naked] fn` 被否：naked function 的语义仍围绕"满足签名的函数 ABI"展开，而 AP 启动代码
  没有 ABI、没有栈、没有正常寄存器状态，CPU 甚至还在 16 位实模式——把它包装成 Rust 函数
  是假象。若 toolchain spike（见下）证明链路有问题，回退极小 .S 文件（唯一例外，16-smp 记录原因）。
- **方案 B（否决）**：手写独立 .S 文件照抄 C trampoline.S。违背 doc 02 §3.3 既有哲学
  （内联汇编足够精确 + trait 跨架构 + 类型安全），且 C 需要独立 .S 是其实现方式的产物，
  Rust 重写没有义务保留文件形式——要保留的只是硬件义务（INIT-SIPI → <1MiB → 16 位 →
  long mode → Rust），不是 trampoline.S 这个文件。

**硬约束与验收门（先于 L2）**：early entry image 必须 **position-independent、
self-contained、relocation-free**——它是"拷贝机器码到另一个地址执行"的 flat blob，链接期
地址（高 VMA）与运行期地址（低物理地址）毫无关系，任何绝对符号引用、外部符号、绝对地址
常量、非本地 relocation 都会让拷贝即爆炸。S-3 第一步是 **toolchain spike（10~30 行）**：
不写完整梯子，只证明"当前 Rust 工具链 + lld + UEFI 链路能产出一个可拷贝、无 relocation 的
`.ap_early_entry`"。验收命令：`readelf -S`（段存在且保留）、`readelf -r`（无未处理
relocation）、`objdump -d`（16/32/64 三阶段反汇编正确）、`objcopy`（blob 实际大小）。
spike 过不了立即换机制，不把工时押在错误抽象上。

**ApBootstrap 统一结构（值传递通道收敛）**：三个架构的汇编只负责"取出一个 opaque
bootstrap 指针，然后跳出去"，其余信息全部进结构体：

```rust
#[repr(C)]                      // 跨汇编 ABI（见下文"地址空间交接闭环"第 4 条）
pub struct ApBootstrap {
    logical_id: u32,            // 拓扑数组下标（CpuId）+ u32 padding
    hw_id: u64,                 // APIC ID / MPIDR / hartid
    page_table_root_pa: u64,    // BSP 页表物理根（MMU-off 读取，x86 需 <4GiB）
    kernel_stack_top_va: u64,   // per-AP 内核栈顶 VA（MMU on 后才解引用；x86 于 64 位段读）
    rust_entry_va: u64,         // ap_early_entry 链接期 VA（同上）
}
```

**bootstrap 内嵌 image 数据区（v4 #2 回应，地址限制问题就此消解）**：`ApBootstrap`
不独立分配——按固定偏移内嵌在 early entry image 的数据区（C 的 `__ap_id` 同款布局：
入口偏移 0，结构体在汇编期常量偏移 K）。x86 全部访问约束收敛到既有的
**image < 1MiB**（SIPI 硬性要求），不存在独立的 bootstrap <4GiB 追问；BSP 拷贝 image
到低内存后，经 direct map/恒等映射填写字段，串行复用（每 AP 重写 per-AP 字段，握手
保证上一个 AP 已消费——对齐 C `phys_copy __ap_id` 模式）；arm/riscv 的固件 context
寄存器退为可选（PC/CS 相对即可定位，保留传 bootstrap 偏移的能力以备用）。

**`ApEarlyEntryImage` 与 `ApBootstrap` 的对象边界（v5 P1 采纳；v7 #4 按架构拆分）**：
`ApBootstrap` 只是 `#[repr(C)]` 的 **ABI 布局类型**（编译期形状，无独立存储语义）。
运行期对象按架构分两种形态（v7 #4 定稿，消除"是否复制执行"的二义性）：

- **x86**：**`ApEarlyEntryImage`** = "机器码 + 内嵌 bootstrap 记录"的低地址复制实体
  （SIPI 落点 <1MiB 所必需；复制仅发生一次、发生在 BSP，无执行中改写 → 无 I-cache
  维护问题——x86 不需要 I-cache 同步）。
- **aarch64/riscv64**：**arch-local early stub**——静态内核镜像内的代码 + 静态内嵌
  `#[repr(C)]` bootstrap 记录（PA 链接期可算），**不复制**：AP 经 PSCI/SBI 直接进入
  内核镜像物理地址，镜像由固件装载且 ExitBootServices 已刷缓存，AP 冷 I-cache 首取即见
  已装载代码 → **D/I-cache maintenance 问题不存在**。BSP 串行重写静态记录字段（同一
  握手纪律）。**bootstrap 记录是 writable static storage（v8 #5）**——必须落在可写
  data 段（`static mut`/`SyncUnsafeCell` 语义，**不得** `static X: ApBootstrap = ...`
  落 rodata）；AP 经固件 context 或 PC 相对获取其 PA，BSP 在 AP 启动临界区内串行更新
  字段。

两形态共享同一 Rust 汇合点 `ap_early_entry(bootstrap_pa)`。AP 在 boot_ack 之后**不得
保留或使用指向 bootstrap 的任何引用**——旧 AP 使用的只有拷贝进寄存器/局部变量的字段值。

**image 生命周期不变量（v5 P0-1 采纳，方案 A）**：**"`boot_ack` 的 Release 发布 =
AP 已完成对当前可复用 image 的最后一次读取"**。落到 `ap_early_entry` 的执行序：asm 的
全部读取天然发生在 Rust 进入前（页表根→CR3/TTBR/satp，VA 值→寄存器）；Rust 侧把后续
所需字段（`logical_id`、`hw_id`、`kernel_stack_top_va`——后者仅传递用，栈已由 asm 装好）
**先拷贝到局部变量，再发布 boot_ack**，此后 `bootstrap_pa` 不再解引用、`init_ap` 只收
局部值作参数。BSP 观察到 ack 即可安全重写 image 给下一个 AP——单 image 串行复用的
happens-before 由此闭合。**ownership 语义（v6 #4 采纳）**：`boot_ack` 不只是事件通知，
还是 **mailbox ownership 的交接完成信号**——完整生命周期：`BSP owns image → populate →
hand off → AP reads → AP publishes boot_ack（= 最后一次读取）→ BSP regains ownership`。
禁止把 ack 误读为"AP 已经开始跑"（那是 boot_ack + 后续 ONLINE 的事）。方案 B（延迟 ack 到最后一次读取之后）不采纳：ack 越早，
per-AP 超时窗口越贴近 C 的握手语义。

**`page_table_root_pa < 4GiB` 不变量（v5 P0-2 采纳：方案 1，且源码已有保证链）**：
v4 的"消解"确实不成立——image <1MiB 管不到 `page_table_root_pa` 的值；且 Intel PAE
过渡阶段 32 位 `mov cr3` 只写 bits 31:0，根地址必须 <4GiB。源码证据链（v5 核实）：
`arch/src/lib.rs:202` `BOOT_IDENTITY_MAP_END = 0x1_0000_0000` →
`boot_dm_admissible_end()` = min(该值, VM DM window)（:215）→ boot-shim
`bootstrap_alloc_limit`（uefi_helpers.rs:266）→ `alloc_root_page()` 用 UEFI
`AllocateType::MaxAddress(上限)` 分配并 `.expect()` 强制（:276）→ riscv 侧更有编译期
断言 `BUMP_END <= boot_dm_admissible_end()`（opensbi_helpers.rs:375）。**AP 32 位 PAE
阶段第一次写 CR3 加载的就是这个 root**（`arch_boot_impl(root_page)` → `paging.enable`
→ `mov cr3`，arch/src/x86_64/paging.rs:431——与 C `bootstrap_pt->p_seg.p_cr3` 同源）。
S-3b 增加防御性 assert（BSP 填字段时校验 `page_table_root_pa < 4GiB`）。
**>4GiB 物理机的演进路径**（记录，不实施）：低内存 staging PAE root + long mode 后切
换最终 root（v5 方案 2）；其前提是抬高 `BOOT_IDENTITY_MAP_END`——那是 boot 设计常量
变更，不是 AP bring-up 局部补丁。

**两个掩码的写入点与地址形态（v4 #9 回应）**：`boot_ack_mask`/`online_mask` 的置位
**全部发生在 Rust（`ap_early_entry` 及 init_ap 尾部）**——AP 此时 MMU on、运行在内核
地址空间，直接引用 `SmpState` 全局（内核 .bss static，生命周期 = 内核终身；内核页表
以 cacheable normal memory 映射，x86 天然 coherent，arm/riscv 缓存一致性由正常
Release/Acquire 语义承担）——**asm 不写掩码、ApBootstrap 不携带掩码 PA**，PA 值与
Rust 原子访问合法性之间的隐含假设整个消失。

x86 邮箱只存**一个指针**（bootstrap 结构体指针，物理地址）：BSP 串行启动 AP（C 模型：
设 mailbox → SIPI → 等 AP 读走 → 下一个），单邮箱即可，与 C `__ap_id` 语义一致。ARM 走
PSCI `context_id`、RISC-V 走 SBI `opaque` 直接传同一结构体指针。三架构在 `ap_early_entry`
汇合。**物理地址/虚拟地址不变量（v3 外评 #5 修正版，两栏拆分）**：

| 类别 | 字段 | 可用时机 |
|---|---|---|
| MMU-off only（PA） | `bootstrap_pa`、`page_table_root_pa`、blob 内 boot 临时栈（x86，<1MiB） | asm 全程可解引用 |
| MMU-on only（VA） | `kernel_stack_top_va`、`rust_entry_va` | 仅 MMU 开启后解引用 |

内核是 higher-half，`&sym as usize` 这类链接期 VA 绝不能在 MMU off 时交给固件或被解引用。

**无栈叶子原则（v3 #6 提出，v4 #1 修正为按架构精确化）**：三架构 stub 全程关中断、
**零 push/call/pop（无栈）**——此点保留；但"MMU on 之后不触碰内存"只在 arm/riscv 成立，
x86 不成立：

- **arm/riscv（真·全预载）**：进入即 64 位寄存器，开 MMU 前把全部字段从 bootstrap
  （PA 读）预载寄存器（页表根 → TTBR/satp；`kernel_stack_top_va`/`rust_entry_va` 留
  寄存器）；MMU on 后数据访问为零，直接装 SP → 跳入口。
- **x86（64 位阶段读取）**：16/32 位阶段寄存器只有 32 位宽，**装不下 64 位 VA**——
  `kernel_stack_top_va`/`rust_entry_va` 的最终装载只能发生在 64 位代码段（分页已开）。
  真实时序：16 位 CS 相对读 bootstrap（内嵌 image，见下）→ 32 位以寄存器间接
  （地址 32 前缀零扩展）读 `page_table_root_pa`（<4GiB）装 CR3 → 开 PAE/LME/PG →
  远跳 64 位（恒等区内）→ **64 位段经恒等映射重读两个 VA** → `mov rsp` → `jmp`。
  准确表述：**"MMU on 后只经恒等映射读内存，且仍然无栈（无 push/call/pop）"**；
  恒等映射需覆盖 image 全体（代码 + 内嵌 bootstrap），不只是代码区。

`&ApBootstrap` 的重建仍在 Rust 侧经 direct map 完成（`ap_early_entry(bootstrap_pa)`）。

**image/section 模型（v3 #6 提出、v7 #5 精确化）**：x86 的 early-entry 运行期实体是
**一个连续 runtime image，由两个 input/output section 组成**——`.ap_early_entry`
（代码）+ `.ap_early_entry_data`（bootstrap 记录等可写数据）——拷贝单元 = 两 section 的
连续整体，运行期相对布局不变。三个概念严格区分：**single image**（拷贝单位）≠
**single ELF section**（有 .ap_early_entry 与 .ap_early_entry_data 两个）≠
**single output segment**。跨 section **无符号引用**：code/data 互址全部经**编译期
offset 注入**（`global_asm!` `const` 操作数 + `offset_of!`）；16 位部分 CS 相对
（入口偏移 0，bootstrap 偏移为汇编期常量），32/64 位部分 RIP 相对（链接期解析的 PC
相对量，整块拷贝后 delta 保持有效）。验收表述相应精确化：`readelf -r` 零
未解析 relocation **只是必要条件**——"ELF relocation-free ≠ PIC proof"，最终证据是
spike 的"同一机器码、不同运行期基址、执行成功"三要素（spike 验收已含）。

**linker 保留与边界符号（v3 外评 #8 回应，以实际配置为准）**：拷贝边界符号
（`ap_early_entry_start` / `ap_early_entry_end`）**在 asm 内部自定义**，不依赖链接器的
`__start_*` 生成机制。实际链接路径已核实：x86 UEFI 测试内核无自定义 link.ld（默认
lld 脚本）、riscv64 测试内核有项目自有 link.ld——是否启用 `--gc-sections` 由 spike 在
真实链接路径上实测；若启用则两种处置：传 link-arg 关闭对该段的 GC，或（I-1 接入 kernel
link.ld 时）加 `KEEP(*(.ap_early_entry))`——列入 S-3a spike 检查单。

**低内存物理来源（x86 独有前置）**：C 用 `alloc_lowest(&kinfo, tramp_size)`（arch_smp.c:76）
从 memmap 取最低区；Rust 侧 S-3 首查 `KernelInfo.memmap` 是否保留 <1MiB 传统低内存区
（UEFI 可能回收），无则 boot-shim memmap 增保留区或 boot_alloc 外设独立 low-memory 池。

**BDA 0x467 暖复位后备不移植**：82489DX 时代兼容（C arch_smp.c:127），现代 LAPIC 单轮
INIT+SIPI 可靠；与 D-53 "只支持现代硬件"原则（当时不移植 CPUID `max_leaf==0` 的 486
时代守卫）同一先例，16-smp 记录取舍。

**S-3 阶段边界（修订，避免小鸡生蛋）**：S-3 时 `init_ap` 还是 panic 占位，所以 S-3 的 AP
终点是 `ap_early_entry`——置 `boot_ack_mask` 握手位后 park（`hlt`/`wfi` 循环），
**不碰 online_mask、不进** `init_ap`（v3 #1 双状态拆分）；S-4 才把 `ap_early_entry`
扩展为调 `arch init_ap` → AP 主循环。阶段边界干净：S-3 测 early entry 本体，S-4 测扩展。

**地址空间交接闭环（2026-09-06 GPT v2 外评 P0 回应；原文跳过了 PA→VA 交接步）**：

```
firmware / SIPI（入口 = image 基址，struct 内嵌于固定偏移）
  ↓ x86：CS 相对读内嵌 struct；arm/riscv：PC 相对或固件 context
MMU-off asm：读 page_table_root_pa → 装 CR3/TTBR/satp
  ├─ arm/riscv：预载两个 VA 到寄存器 → 开 MMU → 装 SP → 跳入口（零内存访问）
  └─ x86：开 PAE/LME/PG → 远跳 64 位（恒等区内）→ 经恒等映射读两个 VA
                                          → mov rsp → jmp
  ↓
跳转进入 ap_early_entry(bootstrap_pa: usize)  ← 无 call（无栈）；ABI 参数寄存器设置后 jmp/branch；Rust 侧内部经 DirectMapArch::kernel_phys_to_virt
                                            （PA + KERNEL_DIRECT_MAP_BASE）转 VA 构建 &ApBootstrap
```

四个决定：

1. **Rust 边界收 PA 而非引用**：`unsafe fn ap_early_entry(bootstrap_pa: usize) -> !`——
   跨 asm 边界的值本质是物理地址，Rust 侧用 `kernel_phys_to_virt` 转换
   （x86 基址 `0xFFFF_8080_0000_0000`、riscv `0xFFFF_FFC0_4000_0000`，
   `os/arch/src/arch/direct_map.rs:81/:115/:146/:169`），转换后才建 `&ApBootstrap`。
2. **`ap_early_entry` 的地址由 bootstrap 字段 `rust_entry_va` 传入**——relocation-free 的
   blob 不可能直接引用 Rust 符号；`rust_entry_va`/`kernel_stack_top_va` 是 BSP 填好的
   **数据值**（链接期 VA），只被 MMU-on 之后的代码解引用，与"PA 字段先于 MMU 使用"的
   不变量不冲突。blob 的段名随之更名：`.ap_early_entry` / `.ap_early_entry_data`。
3. **MMU 开启瞬间 PC/数据仍可访问的依据**：三架构 BSP 页表含恒等映射（HigherHalf 契约
   "identity and kernel high mappings"，doc 02 §3.4；x86 实现为 PML4[0] 恒等 [0,4GiB)
   且在 DM coverage 建立后保留——paging.rs:727-795 的 split/降位注释即证）。x86 的 blob
   与 bootstrap 落在恒等区内（<1MiB），MMU on 后 PC 与 PA 读继续有效；arm/riscv 的 stub
   落点必须同样落在恒等映射覆盖区内——S-3c 首项核验"三架构恒等映射覆盖范围 vs stub
   落点"，不覆盖则挪落点或扩恒等映射。x86 附加不变量：**BSP 页表根 PA < 4GiB**（32 位
   保护模式下 `mov cr3` 只装得下 32 位，Linux head_64.S 同款约束；QEMU 默认内存布局恒
   成立，S-3b 加 assert）。
4. **`ApBootstrap` 是跨汇编 ABI（v2 外评 P0 回应）**：`#[repr(C)]` + 全部固定宽度字段
   （u64 为主，`logical_id: u32` + 显式 padding）——项目先例：`CpuInfoEntry` repr(C)
   16 字节（D-53）、`PrivUpdateRequest` repr(C)（D-51）。汇编访问字段的 offset **不由
   手写常量**，经 `global_asm!` 的 `const` 操作数注入，值来自 `core::mem::offset_of!`
   （编译期生成，Rust 加删字段立即编译失败）；另配 `const _: () = assert!(size_of::<
   ApBootstrap>() == N)` 钉总尺寸。静默漂移不可能。

**握手与上线双状态分离（v3 外评 #1 裁定为 P0 并采纳；v2 曾把两者错并进一个 online_mask）**：
C 有两个刻意分开的信号——`ap_cpu_ready`（AP 到达早期入口即写，arch_smp.c:219，在
`ap_finish_booting` 的**第一条语句**，先于一切 per-CPU 初始化）与 `ap_cpus_booted`
（AP 完成 boot 序列后自报，smp.c:51）。v2 曾让 early entry 直接置 `online_mask` 位，
导致"AP 置位 ≠ init_ap 完成"却会被 wait_for_aps 当成上线——这是真实的 bring-up
逻辑 bug（ parked AP 也会让 BSP 误判就绪）。Rust 恢复双状态结构，两个位图：

```
boot_ack_mask : AtomicU32   // "AP 已消费 bootstrap、到达 early entry"
    AP 在 ap_early_entry 第一动作：fetch_or(1 << logical_id, Release)
    ← C 对应物：ap_cpu_ready = cpu（arch_smp.c:219）
    用途：内嵌 struct 串行复用的握手（BSP 观察到 ack 才重写字段启动下一个 AP）+ per-AP 启动超时判定
         + S-3d 的 AP-alive 测试（L2 断言 boot_ack 位，不碰 online）

online_mask : AtomicU32     // "AP 完成 init_ap，可参与 SMP"
    AP 在 init_ap 尾部自报：fetch_or(1 << logical_id, Release)
    ← C 对应物：ap_boot_finished / ap_cpus_booted（smp.c:51）
    用途：wait_for_aps 完成条件（online_mask == boot_ack_mask，即 C 的
         ap_cpus_booted == n-1——n 为已握手数）+ L3 断言
```

`CpuFlags.READY` 仍由 **BSP 代置**（观察到 boot_ack 位后，对齐 C arch_smp.c:137 的
`cpu_set_flag(cpu, CPU_IS_READY)` 代置语义）。fetch_or 按位幂等——**重复置位不会膨胀计数**（数量/身份/缺席由最终 mask 判定；
若需检测重复本身，可 debug_assert `old & bit == 0`，不增加正式状态；掩码由 ap_early_entry 直接引用 SmpState 全局，不进 bootstrap ABI——见下文"两个掩码的写入点"）。时间轴：

```
early entry ──→ boot_ack |= bit ──→ init_ap（per-CPU GDT/TSS/GS_BASE/MSR…）──→ online |= bit ──→ 调度循环
     │                                   │
     BSP：观察到 ack → 代置 READY        BSP：wait_for_aps 等 online == boot_ack
```

**mailbox 握手（v2 外评 #5 回应；源码证实 C 有完整握手 + 超时，原文转述不完整）**：
C `smp_start_aps` 每 AP 流程（arch_smp.c:124-141）——置 `ap_cpu_ready = -1`（握手变量）
→ 写 `__ap_id` mailbox → `mfence()` → INIT/SIPI → **LAPIC one-shot 5 秒计时** → 自旋等
`ap_cpu_ready == cpu`（AP 消费 mailbox 后写回自己的 cpu 号）→ 观察到则 BSP 代置
`CPU_IS_READY`、break；**5 秒超时则 WARNING "CPU didn't boot" 并跳过**。单邮箱安全的
原因正是这个串行握手：BSP 等 AP 消费完才启动下一个。Rust 逐条对齐：握手变量 =
online 位（AP 置位即消费证明），超时用 TSC/PIT 读数（语义 = C 的 5 秒，S-5 实施时定
常数），超时 WARNING 后继续下一个 AP。

### 3.3 init_ap：AP 侧 per-CPU 初始化（三架构分列）

API 骨架已在：`os/arch/src/x86_64/protection.rs:368` 的
`init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes)`，现为 panic 占位，模块注释自述
职责 "Per-CPU TSS/GDT entry, load selectors (no global GDT sync)"（每 CPU 自己的 TSS/GDT，
加载选择子，不与全局 GDT 同步）。三架构落地内容：

- **x86_64**（最重）：构建 per-CPU GDT——x86 硬件要求 TSS 描述符必须挂在 GDT 里，且每个
  CPU 需要自己的 TSS（RSP0 内核栈、IST 槽位），所以每 CPU 一张 GDT（C 同构做法，
  `protection.rs` 模块注释 "no global GDT sync" 即指此）；`lgdt` + `ltr` + 数据段选择子；
  `lidt` 指向**共享 IDT**（IDT 可跨 CPU 共享，门表里存的是同一批入口）；`WRMSR GS_BASE`
  写入本 CPU 的 per-CPU 结构指针（后续 `CpuLocal` 访问的根基）；换栈后跳 Rust。per-AP
  内核栈来源：C 用每 CPU 静态栈，Rust 用固定数组 `[u8; N]; MAX_CPUS` 或 boot_alloc 分配
  ——S-4 实施时定，倾向前者（no_std 下更可预测）。
  **per-CPU MSR 重编程（GPT 外评补入，防"四核起来了但只有 CPU0 能跑系统调用"）**：
  `IA32_STAR / IA32_LSTAR / IA32_SFMASK / IA32_EFER.SCE` 都是 per-CPU MSR——BSP 在
  `configure_syscall`（trap_entry.rs:230-260）写过的值，**每个 AP 必须原样再写一遍**，否则
  AP 进用户态后第一条 `syscall` 指令即跑飞。`IA32_GS_BASE` 与 `IA32_KERNEL_GS_BASE`
  两个变体都要处理（用户态/内核态 swapgs 切换用）。S-4 验收清单必须逐项列出 BSP-only
  与 per-CPU 的 MSR/寄存器归属。
  **时钟基线核对（v2 外评 #6 回应，源码修正原文）**：x86 `ClockArch::init_timer`
  配置的是 **8254 PIT——单一系统级设备**，`cpu_id` 参数被忽略（clock.rs:57-63 自注
  "The PIT is a single system-wide device... cpu_id is not needed here"）；per-CPU 的
  LAPIC timer 属"后续 + stop_local_timer"范畴。**结论：x86 的 S-4 无 per-AP 时钟工作**
  （PIT 已由 BSP 配置）；arm/riscv 的 init_timer 是 per-CPU 语义（比较器按 CPU 配置），
  S-4 可做。**IRQ 送达链四级，缺一不可**：①init_timer 配置源 → ②控制器 unmask
  （`ArchInit::init`，lib.rs 注释明示"enables the timer interrupt"）→ ③CPU 允许中断
  （x86 IF / arm DAIF / riscv `sstatus.SIE` + `sie.SSIE`，两者都需要）→ ④trap 入口就位
  （**现状：IDT 的 `load()` 从未被调用**——lib.rs `init_protection` 自注
  "Do NOT call trap.load() here — handler addresses are still 0"，真正的 load 属 S-8）。
  所以 S-4/S-7 期间 AP 不会收到任何可屏蔽 IRQ，前提是**保持③全屏蔽**：S-7 前 AP 的
  pause 不得置 IF（或依赖与 BSP 现状相同的不变量"无未屏蔽 IRQ 源"，S-7 验收时二选一
  并写明）。"AP 可接收可屏蔽中断"的最终解锁点 = S-8 的④ + 显式的①②③序列。
- **aarch64**：`VBAR_EL1` 指向共享异常向量表；MAIR/TCR 复用 BSP 相同值；TTBR0/1 = BSP
  页表物理根（bootstrap 结构体传入）；`dsb sy` + `isb` + `SCTLR.M = 1` + `isb` 开 MMU；
  `sp` 先指 per-AP 物理栈顶、MMU 开启后换高地址栈；分支到 `ap_early_entry` → `init_ap`。
  GICv3 redistributor 唤醒与使能（每 CPU 必须各自做一次，`CpuInfo.gicr_base` 已备好基址）
  归属"收 IPI 前"（S-8 阶段）。
- **riscv64**：`satp = BSP satp`（Sv39 编码，bootstrap 传入）+ `sfence.vma`；`sp` 指 per-AP
  栈；`stvec` 指共享向量表。**收 IPI 的前置是 `SIE.SSIE`（监督态软件中断使能）**。
  **PLIC 与 IPI 路径完全独立（v9 nit 2 最终边界）**：IPI = SBI send_ipi → CLINT MSIP →
  supervisor **software** interrupt，全程不经 PLIC；PLIC 只服务外部 IRQ，其 context 使能
  归 S-8/S-10 的 IRQ 使能阶段，与 IPI 无任何前置关系。
  IPI 经 SBI 送达——**v6 #1 发现现存代码是真 bug**：`riscv64/smp.rs:43/47` 实际写
  `a7 = 0x0, a6 = 0x3`，但 ratified SBI 规范的 legacy 扩展（EID 0x00-0x08）每个 v0.1
  函数有**独立 EID 且 FID 被忽略**——EID 0x0 是 legacy **SetTimer**，Send IPI 是
  **EID 0x4**。`a7=0/a6=3` 会被 OpenSBI 当成 set_timer 把 `1 << cpu` 写成计时值
  （IPI 根本没发、BSP 自己的 timer 被污染——S-10 才会暴露，故至今未触发）。修复 =
  改用标准 SBI IPI 扩展：**`a7 = 0x735049`（"sPI"）、`a6 = 0`、`a0 = hart_mask`、
  `a1 = hart_mask_base`**（语义不变），legacy EID 0x4 为备选；列入 S-3c 修复清单。
  OpenSBI 收到后写目标 hart 的 CLINT MSIP 位 → 触发**软件中断**，**与 PLIC 无关**（PLIC 只管外部中断，归 S-8 的 IRQ 路径；
  2026-09-06 GPT 外评修正，原文"PLIC 使能是收 IPI 前置"系错误）。

### 3.4 smp_init 编排、boot_lock 与 wait_for_APs 的 BKL 舞蹈

C 语义精确转述（实施时按此对齐，这是 rewrite 原则——外部行为保持）：

```
smp_start_aps()                          arch_smp.c:100
  spinlock_lock(&boot_lock)              :227  ← AP 启动临界区开始
  copy_trampoline()                      :120  ← 拷 16 位代码到 <1MiB
  __ap_id mailbox 地址写 BDA 0x467       :123-127
  for 每个 CPU（跳过 BSP）：                    ← 逐核发 INIT/SIPI
  spinlock_unlock(&boot_lock)            :247  ← 临界区结束

wait_for_APs_to_finish_booting()         smp.c:30
  数 cpu_test_flag(i, CPU_IS_READY)            ← 实际活了几核
  若 n != ncpus → printf WARNING              ← 少核不 panic，降级继续
  BKL_UNLOCK()                                ← 关键：先放锁，让 AP 能进内核
  while (ap_cpus_booted != n-1) arch_pause()  ← 等 AP 全部报到
  BKL_LOCK()                                  ← 等完再拿回来继续 BSP 流程

AP 侧：early entry image → init_ap → ap_boot_finished(cpu)（smp.c:51，递增 ap_cpus_booted）→ 调度循环（C 用独立 trampoline.S，Rust 对应物为 early-entry image，无文件抽象）
```

Rust 侧：kernel 层编排（拓扑遍历 + `SmpArch::boot_ap` + `wait_for_aps`——后者已实现）+
`boot_lock` 用既有 spinlock 基建。**要点**：Rust `wait_for_aps` 现无 BKL 交接语义，编排时
必须复刻 C 的 UNLOCK→等待→LOCK 舞蹈，否则 BSP 持锁自旋、AP 抢不到锁，死锁。
对照：Linux 的 `smp_init()` → `cpu_up()` 走 CPU 热插拔状态机逐核注册（复杂度服务于运行期
热插拔）；Minix3 与本项目选择"全部拉起再等满"（简单、够用，重写原则：外部行为保持）。
Redox 的 `arch::start_aps` 与本设计同构（early-entry 代码 + bootstrap mailbox）。

**CPU 身份模型（GPT 外评补入，防"CPU #2 ≠ APIC ID 2 ≠ MPIDR 2 ≠ hartid 2"类 bug）**：
两个 ID 空间严格分离——**逻辑号**（`CpuId` newtype，kernel 已有，= `CpuTopology.cpus[]`
数组下标）与**硬件号**（`CpuInfo.hw_id`，x86 APIC ID / ARM MPIDR / RISC-V hartid）。
`boot_ap` 的目标参数从本计划起一律传 `hw_id`（现有 arm64/riscv64 实现用线性号是
QEMU 单簇下的巧合，arm64/smp.rs:221 注释自认简化）。类型层面：`CpuId` 已是 newtype，
`hw_id` 侧 S-5 引入 `HwCpuId` 同类包装（或以 `u64` + 命名约定，实施时定），杜绝互换。

**CPU 状态机（GPT 外评补入；v2 回应修正——C 的超时语义经源码核实后归位）**：C 用两个
不同信号（`CPU_IS_READY` per-CPU 标志 + `ap_cpus_booted` 计数器），语义本就不是一个
flag，Rust 不许压扁成一个 `AtomicBool`：

```
DISCOVERED（拓扑表列出，未唤醒）
  → BOOTING（BSP 发 INIT/SIPI 或 PSCI/SBI 调用）
  → READY（BSP 观察到 AP 握手信号后代置 CpuFlags.READY——对齐 C arch_smp.c:137
           cpu_set_flag(cpu, CPU_IS_READY) 的"BSP 代置"语义，不是 AP 自置）
  → ONLINE（AP 置 online_mask 位——对齐 C smp.c:51 ap_boot_finished 的"AP 自报"语义）
  → 进调度循环
失败路径：BOOTING → FAILED——**C 忠实，非行为偏离**：C 的 smp_start_aps 对每个 AP 有
  5 秒 LAPIC one-shot 超时（arch_smp.c:131-141），超时 WARNING "CPU didn't boot" 并跳过；
  但 wait_for_APs 本身无超时（等"所有 READY 的 CPU 报到"，smp.c:38-45）——两级语义
  Rust 全部保留，超时常数取同量级（实施时定）。
```

Rust 落地（v3 外评 #1 修正为**双位图**，见 §3.2——v2 曾错并成单 online_mask）：`CpuFlags.READY`
（已有，BSP 代置）承担 READY；`boot_ack_mask` 位由 AP 在 early entry 置位（对应 C
`ap_cpu_ready`）；`online_mask` 位由 AP 在 **init_ap 完成后**自报（对应 C
`ap_cpus_booted`，现有 `ap_cpus_booted` 计数器 smp.rs:443 退役）——计数器的"3"可能来自
"CPU1 报两次 + CPU3 没报"，位图则数量/身份/重复/缺席一次断言全齐，天然契合
`MAX_CPUS = 32`。wait_for_aps 完成条件 = **`online_mask == boot_ack_mask`**（全部已握手
的 AP 完成 init_ap，即 C 的 `ap_cpus_booted == n-1`）——**这是生产语义，允许少核降级继续（C 忠实）**。测试判据（v4 #3 修正，防
"全部 AP 失败仍相等"的假通过）必须另加 expected 等式：L3 断言
**`boot_ack_mask == expected_mask && online_mask == expected_mask`**——生产逻辑与
测试健康性两个层次分开，测试失败不代表生产路径行为改变。原 L3"断言用之"的表述废止。
**掩码细则与命名（v2 #8 回应 + v5 P1-4 统一）**：位下标 = 逻辑号（`CpuId` = 拓扑数组
下标），不是 hw_id；BSP 自己的两位（boot_ack 与 online 中的 BSP 位）由编排层在启动任何
AP 之前设置；BSP 的逻辑号 = 拓扑中 `cpus[i].hw_id == bsp_id` 的下标 i（**MADT/DTB
解析顺序不保证 BSP 排第一，必须 match 求出，不得假设 0**）。四个 mask 的定义固定：

```
expected_cpu_mask = BSP 位 + 全部拓扑 AP 位（"应上线的全员"）
expected_ap_mask  = 仅 AP 位
boot_ack_mask     = BSP 位 + 已到 early entry 的 AP 位
online_mask       = BSP 位 + 已完成 init_ap 的 AP 位
```

L2 断言 `boot_ack_mask == expected_cpu_mask`；L3 断言
`boot_ack_mask == expected_cpu_mask && online_mask == expected_cpu_mask`
（FAILED AP → expected 不满足 → 测试失败，而生产 wait_for_aps 仍走 C 忠实的
`online_mask == boot_ack_mask` 降级语义）。**`expected_cpu_mask` 不是生产状态**——它是
测试环境根据已发现拓扑计算的健康性期望，证明"本次测试确实启动了全部预期 CPU"；
"少核启动仍可继续"（生产）与"测试要求四核全部起来"（健康性）不矛盾，两层并存。

**锁序证据（v1 #17 遗留，v3 期间源码补证）**：AP 侧顺序确认为 **boot_lock → BKL**——
C `ap_finish_booting`（arch_smp.c:222-224）先 `spinlock_lock(&boot_lock)` 再
`BKL_LOCK()`，注释并说明原因（lapic timer 校准的嵌套中断使 BKL 不够，必须 boot_lock）。
BSP 侧（`smp_start_aps` 进入时是否持 BKL）仍留 S-5 读 main.c 调用链裁定；两侧取并集后出锁序表。
**表述定性（v6 #5 采纳）**：boot_lock 与 BKL 的双向嵌套**不是全局 lock-order 反转炸弹**，
而是**带阶段性交接约束的启动协议**——BSP 在释放 `boot_lock`（arch_smp.c:247）之后 AP
才可能获得它；BSP 在 `wait_for_APs` 开头先 `BKL_UNLOCK`（smp.c:38-45）才解除 AP 对 BKL
的等待。两条阶段约束使双向嵌套在 bring-up 窗口内无死锁。锁序表必须写明这些**发生条件**，
不得把 boot_lock 抽象成普通全局锁序规则。

**boot_lock 保留立场（v4 #4 修正，撤销 v1 的"无对手可删"假设）**：AP 自己就是竞争者
（arch_smp.c:222-224 源码证据），"只有 BSP 调用所以无竞争"的删除理由**作废**。C 中
boot_lock 的保护对象 = AP 完成启动序列期间的 per-CPU 初始化临界区（timer 校准会解锁/
重获 BKL，单独 BKL 不够——arch_smp.c:223 原注释）。**默认保留** boot_lock（Rust 对齐
C 语义）；"能否用更小原语（bootstrap 握手 + 原子发布）替代"列为 SMP 跑通后的优化评估
项，删除必须给出完整 happens-before 论证——不为 Rust 漂亮而提前拆安全机制。

**锁序分析（S-5 开工项，GPT 外评提出、顺序待源码裁定）**：涉及 boot_lock、BKL、调度/IPI
状态三类锁，实施时必须产出一张"谁在持有时可获取谁"的表。注意 C 的实际持锁顺序与直觉
可能相反：`smp_start_aps` 拿 boot_lock 时 BSP 很可能已持有 BKL（即 BKL → boot_lock），
与常见"外围锁在内"的直觉相反——**以 C 源码实测为准，不预设结论**。同时按 rewrite
（v4 已定稿：boot_lock 默认保留，AP 即竞争者；删除属 SMP 跑通后的独立优化评估，需完整 happens-before 论证——"单 BSP 调用 smp_init"**不能**推出无竞争（arch_smp.c:222-224 证明 AP 亦获取该锁），v7 #7 残留清理）。

### 3.5 per-CPU 化四件套（每件独立迭代、独立 commit）

1. **D-40 ptproc per-CPU**：现状是全局 `CURRENT_PTPROC_NR`（单核下由 `init_post_and_memory`
   设一次）。迁移到 `CpuLocal` 字段：AP 进入后各自设置。影响面：所有读它的调用点改
   per-CPU 访问（grep 盘点后逐点迁移）。
2. **D-41 PLATFORM 容器并发化**：现状 `SyncUnsafeCell` + 单线程假设（boot 期由
   `BklProtected` 审批列表背书）。AP 并发读前需换 `Atomic`/`Mutex` 容器或论证"只读 +
   boot 期冻结后不可变"。倾向后者：平台描述在 BSP boot 后即冻结，AP 只读——用类型系统
   表达"冻结"（如 `Once`/const 语义）比加锁更符合项目风格。C 对照：C 全局结构体天然
   "boot 后只读"，无显式机制——Rust 显式化是重写收益。
3. **sched-1 per-CPU 调度访问**：`os/kernel/src/proc_table.rs:487/496` 的 `TODO (SMP)`
   注释——调度队列操作的 per-CPU 语义（每 CPU 一个 runqueue 还是共享 + BKL？C 的答案：
   共享 + BKL，`big_kernel_lock` 包一切）。**对齐 C：保持共享 runqueue + BKL**，per-CPU
   化的只是"当前正在运行的进程指针"（`CpuLocal.set_running` 已有）。这也是 doc 16 §4.3
   D2 决策的延伸：Linux 的 per-CPU runqueue 是负载均衡优化，Minix3 语义就是全局队列。
4. **tick-1 per-CPU tick 统计**：`lib.rs:2630` 的 `TODO(P2)`——每 CPU 的 tick 计入自己
   `CpuLocal` 的统计字段（`note_context_switch` 已有 TSC 记账先例）。

### 3.6 AP 主循环与 BKL 交接

C AP 终态：`init_ap` 完成 → `ap_boot_finished` → 进调度循环（idle → pick_proc →
switch_to_user），与 BSP 同一条循环代码。Rust 现状：BSP 侧 `switch_to_user` 的 BKL
获取/释放已接线（D-38②③，`lib.rs:1956/2167`），缺 AP 侧入口。设计（措辞按 GPT 外评
修正，防止误读为"AP 全程持锁"）：**AP 首次进入调度循环时满足与 BSP 相同的 BKL 所有权
前置条件**——AP 从 init_ap 出来时处在"内核持锁"状态，与 BSP 从 kmain 进入时一致；此后
的获取、释放与让出完全复用现有调度路径（BKL 不是死循环里的一次性 acquire）。主循环体
直接复用 BSP 的既有循环。C 在 AP 初始化代码里由 `boot_lock` 保护共享资源，进循环前持有
BKL——Rust 同构。

### 3.7 asm 陷阱入口与硬件半环（hw-1 / trap-1 / D-38① / D-46 硬件半环）

x86_64 已核实的状态：IDT 门表 + `lidt` + SYSCALL MSR 就绪（§2.1），handler 全是 0 占位
（trap_entry.rs:186）。**两条硬件入口路径必须分开建模（GPT 外评修正，原文混为一谈）**：

- **A. IDT/trap gate 路径**：异常（#DE/#PF/#GP…）、外部 IRQ（PIC/LAPIC）、IPI、软件 INT
  门都经 IDT 向量。本步写 asm stub 本体：按向量有无错误码归一（CPU 只对部分异常压错误
  码，入口必须对齐栈布局）、保存通用寄存器到 trapframe、调 Rust 分流——异常 →
  `exception_dispatcher`（进 handler 前 `bkl_lock()`，即 D-38①）；IRQ →
  `IrqManager::dispatch`（D-46 硬件半环，时钟中断从此真实到达）。
- **B. SYSCALL/SYSRET 路径**：**不是 IDT 向量**，由 MSR LSTAR 指向的独立入口负责——
  SWAPGS 切换内核 GS、保存用户 RSP 到 TSS/每 CPU 槽、保存寄存器、组装 syscall
  trapframe → `kernel_call_dispatch`（trap-1）。`configure_syscall`（MSR 配置）已存在，
  缺的是 LSTAR 指向的 asm 入口本体。

返回路径走已实现的 `TrapReturnArch`（iretq）。aarch64（VBAR 向量表）/ riscv64（stvec）的
stub 盘点在 S-8 开工时做——本轮未逐行核验，不预设结论。**风险联动**：AP 只有在 IDT 就位
后才能安全收 IPI（无 IDT 的 AP 收到 IPI 即三重故障），所以 S-7 前 AP 不订阅任何 IPI，
IPI 订阅推迟到本步之后（§8 风险表同款）。

### 3.8 shutdown(0)（hw-2，最后做；2026-09-06 按 GPT 外评修订为三架构后端）

分层两个概念：**真正的 shutdown 语义**（C `minix_shutdown`，minix3/minix/kernel/utility.c）
与**QEMU 测试退出机制**是两层，不许混同。QEMU 退出机制本身按架构分后端（三架构没有
统一的 isa-debug-exit——那是 x86 的 ISA 设备）：

| 架构 | QEMU 测试后端 | 真实硬件后端（后续） |
|---|---|---|
| x86_64 | `isa-debug-exit`（I/O 端口写 → QEMU 带退出码结束） | ACPI S5 睡眠 |
| aarch64 | semihosting `SYS_EXIT` | PSCI `SYSTEM_OFF` |
| riscv64 | `sifive_test` 设备（QEMU virt 测试结束器） | SBI SRST 扩展 |

Rust 形态：`minix_shutdown` 语义层一个入口，向下分 QEMU 测试后端（arch 各自实现）与
硬件后端（arch 各自实现）——测试内核走前者，生产内核走后者。hw-2 从"halt-loop 无验收"
变成可测：QEMU 进程按预期退出码结束即 PASS。

### 3.9 SMP 内存序契约（2026-09-06 GPT 外评新增；x86 上"测试全绿"不等于 ARM/RISC-V 正确）

本计划出现大量跨 CPU 发布/消费（bootstrap 结构体、online mask、IPI 请求/应答、PLATFORM
冻结），x86 的强内存序会掩盖问题，ARM/RISC-V 的弱序会暴露它们。全计划统一遵守以下
publication 规则（各 per-CPU 迁移项不再各自决定）：

| 场景 | 规则 |
|---|---|
| bootstrap 数据发布（BSP 填 ApBootstrap → **AP early asm 读**） | **v6 #3 重写：第一次消费者是汇编，Rust 层 Acquire 管不到它**——publisher 侧 fence 属于 `SmpArch::boot_ap`（BSP 填完字段、调固件之前）：x86 `mfence`（C arch_smp.c:130 先例，现存代码缺失→S-3b 补）；arm `dsb ishst`；riscv `fence w,w`。consumer 侧（v7 #1 采纳方案 A；v9 nit 3 更名：**consumer-side ordering barrier**，不称 acquire fence——本交换不构成标准语言内存模型的 release/acquire 同步对，双方是裸汇编与普通内存，barrier 的作用是排序与完成，非同步对象配对）：AP early entry 在首次读
  bootstrap 前执行架构 fence——arm `dsb ish`、riscv `fence rw,rw`（x86 为 TSO，发布侧
  `mfence` 已足够，消费侧无需）。论证链如实记录四层：①BSP 发布 fence 推到一致性点
  （w 侧）；②固件时序交接（temporal：CPU_ON/hart_start 在 hvc/ecall 之后才上电目标
  核）；③冷缓存论证（AP 从未触碰过 bootstrap 区域 → 无陈旧行 → 首读 miss → 必从
  一致性点取到已发布值）——外评正确指出"冷缓存"单独不是 ordering 保证，但 ①+②+③
  合成 coherence 语义下的完整论证；④显式 consumer fence 作为廉价保险使正确性**局部化**
  于 AP 入口（未来若引入 CPU_OFF/ON 复用路径亦不破坏），一条指令成本，采纳。**Rust 层的 Release/Acquire 只管掩码（Rust↔Rust，经 SmpState），不管 bootstrap 交接** |
| CPU 握手发布（AP 到 early entry → BSP 观察 boot_ack）与上线发布（AP 完成 init_ap → BSP/等持者观察 online） | 两个位图同规则：AP 在 Rust 侧（MMU on、内核地址空间）`fetch_or(mask, Release)`，直接引用 SmpState 全局——cacheable normal memory、x86 天然 coherent、arm/riscv 由正常 Release/Acquire 承担；BSP 等待侧 `load(Acquire)`。握手位与上线位是两个独立生命周期（v3 #1），不许合并；asm 不写掩码（v4 #9） |
| IPI 请求（BSP 设标志 → 发 IPI → AP 处理） | 标志 store 用 `Release`，**先写标志再发 IPI**。**职责分离（v2 外评 #10 回应）**：IPI 是通知机制，Release/Acquire 是数据发布机制——正确性**只建立在原子序上**，不依赖"IPI 是否自带屏障"这种架构/实现细节。源码核实：现有 `SchedIpiData` 已按此实现（smp.rs:288 Acquire load / :293 AcqRel fetch_or / :298 Release 清零 / :537/:585 发送前后 SeqCst fence）——S-10 的设计点仅剩"SeqCst fence 是否降级为 Release/Acquire"（记录后定，现有更强序保留亦可） |
| IPI 完成应答（AP 清标志 → BSP 自旋返回） | AP 清标志 `Release`；BSP `load(Acquire)` 观察到清零才返回（`schedule_sync` 既有自旋语义的对齐点） |
| 一次性冻结数据（PLATFORM 等冻结后只读的全局） | freeze 完成即一次性 `Release` 发布；AP 首次访问 `Acquire`。此后只读，无需屏障（对应 D-41 的"冻结"语义） |

例外说明：汇编 early entry 内部（CR3/TTBR/satp 装载与 MMU 开启）用架构规定的屏障
（`dsb+isb` / `sfence.vma`），不走上表；x86 模式梯子（far jump / lgdt）自带串行化。
上表管的是**Rust 侧跨 CPU 数据**的原子序。

---

## 4. 执行规程（每个 TODO 一律走此循环）

1. **选步**：按 §5 顺序取一个 S-x，不改下一个。
2. **讲明白**：这项是什么、为什么。给出 C 锚点与 Rust 现状锚点。
3. **讲怎么修**：≥2 个设计候选，对比 Linux / Redox / OS 理论，优中选优；架构演进项
   `[ARCH: ...]` 三处一致标注（doc + design + code）。
4. **实施**：代码 + 文档 + 测试一并改，不漏文档。测试按五维自审：完备性 / 测试自身正确
   （防"测试代码就是错的"）/ 冗余 / 无效 / 虚构（文档声称的测试与代码双向对账）。
5. **验证**（全部通过才算完成）：
   - `cargo test -p minix-kernel`（hosted mock 路径无回归）
   - `cargo build --target <production-target> -p <受影响测试内核>`（生产路径编译门，§2.4；target 按 S-0 三架构矩阵取——x86/arm: `*-unknown-uefi`，riscv: `riscv64gc-unknown-none-elf`，v4 小点修正原 x86-only 写法）
   - 本步对应 QEMU 测试 PASS：`os/qemu-tests/run_qemu.sh <arch> <efi>`
     （脚本自带 `timeout 30` 防卡死，外层再套 `timeout 120`）
6. **回归 review**：重读本次全部 diff（代码 + 文档 + 测试），对照代码/文档检查清单；
   确认文档-代码一致、无 translate 味。
7. **标注 + 提交**：本文件 §5 表与 todo.md 相关行标注（日期 + 证据锚点），一个 TODO 一个
   commit，commit message 说明设计与验证证据。

**文风约束**：全程技术博客文风，禁黑话/缩写/文言文/压缩简写，事实断言必须带锚点——
文档和中间结果都适用。

---

## 5. 步骤表（单线程顺序执行；每步一个 commit）

| # | 项（对应 TODO） | 内容 | 交付物 | 验收 | 文档同步 |
|---|---|---|---|---|---|
| S-0 | ✅ **已完成（2026-09-06）**——见 §18 基线记录 | 提交 `kernel/src/lib.rs` 两个缺失 import 修复（工作区已有，§2.4）；**三架构固定基线矩阵**（GPT 外评 #11）：每架构 = hosted 测试 + **production-target 构建**（x86/arm: `*-unknown-uefi`；riscv: `riscv64gc` 目标 + OpenSBI 链路——v2 外评措辞修正，非三架构都是 UEFI）+ QEMU smoke（hello-boot），哪怕 arm/riscv 起步只有 hello-boot | 绿色基线记录（矩阵表）+ commit | ~~L0：`run_all.sh` x86_64 7/7 PASS + 三架构矩阵全 ✓~~ **L0 达成**（x86_64 7/7 + arm/riscv hello-boot；基线额外修复 arm/riscv trap_return asm 操作数语法错误，见 §18） | 本文件 §2.4 结论视需要补 doc 01 §5 |
| S-1 | ✅ **已完成（2026-09-07）**——见 §19 勘误记录 | grep 核实 `kernel_call_resume` BKL（原记 syscall.rs:2622 已漂移）；以代码为准勘误 todo.md Edge Items **和** doc 16（两者记录均有误，详见 §19） | 勘误 commit | grep 证据入 §19 | todo.md Edge Items / §7.1 / doc 16 §4.13 |
| S-2 | ✅ **已完成（2026-09-07）**——x86_64/riscv64 PASS，aarch64 SKIP（发现 AAVMF 缺口，S-2b 登记）——见 §20 | 新测试内核 `test-smp-topo` ×3：`-smp 4` 下断言 `nr_cpus==4`、`hw_id` 互异、BSP `hw_id` ∈ 发现集；QEMU 默认拓扑值仅打印为机器特定观察；解析走 `parse_by_kind` 正式交接路径 | 测试内核 ×3 + run_all.sh 接入（SKIP 单列计数） | L1：x86_64 PASS（MADT）/ riscv64 PASS（DTB via a1）/ aarch64 SKIP→S-2b | 16-smp §5.1 加行；doc 04 消费说明随 S-2b 一并 |
| S-2b | ✅ **已完成（2026-09-07）**——见 §20.1 完成记录 | 实测裁决：boot-shim 的 aarch64 ACPI fallback **本已存在**（扫描 ACPI2/ACPI GUID → RSDP source），真缺口是 kernel 侧三处门/偏移：①`kind.rs` RSDP 臂 x86 独占（→ 放开 aarch64）；②`global.rs`/`lib.rs` 的 `PlatformDescEnum::Acpi` 变体与 acpi 模块声明 x86 独占（→ 放开）；③`acpi.rs` 解析偏移两处真 bug（字节级实证：**GICD base 在 +8 而非 +12**——原读恒 0 触发 GicdNotFound；**GICC hw_id 的 MPIDR@+56 在 QEMU GICv2 恒 0**——改用 ACPI Processor UID@+8，恰与 DTB MPIDR Aff0 一致）。另修 B1/A1 连带缺口：`bkl_is_locked` 解除 `cfg(any(test, debug_assertions))` 门（release 测试内核的 debug_assert 载荷必须可命名） | minix-platform kind.rs/global.rs/lib.rs/acpi.rs + test 内核转正 | **aarch64 `test-smp-topo-aarch64` 转 PASS**（nr_cpus=4、hw_id {0,1,2,3}、BSP=0） | doc 04 拓扑消费说明；16-smp §5.1 已加行 |
| S-3a | AP bootstrap ABI + 内存序契约 | 定义 `ApBootstrap`（§3.2）+ `ap_early_entry` 签名 + §3.9 契约落为代码注释/断言约定；**toolchain spike**（10~30 行）：证明 `global_asm!` + lld + UEFI 能产出可拷贝的 `.ap_early_entry` 单段 image（ELF 中间产物 `readelf -r` 零未解析 reloc + 三阶段反汇编 + **实际链接路径 gc-sections 行为实测** + 边界符号 `ap_early_entry_start/end` 自定义 + 同一机器码不同基址拷贝执行成功 + **image 邻接硬验收（v8 #6）**：`[image_start, image_end)` 内 `.ap_early_entry` 必须紧邻 `.ap_early_entry_data`、中间不得插入其它可分配 section——记录 image_start/image_end/image_size/section ordering；拷贝单位是 [start,end) 区间而非"两个 section 各自存在"——v3 #7/#8：relocation-free ≠ PIC proof，执行才是最终证据；**v4 小点：UEFI 最终产物是 PE/COFF，`readelf` 只适用于 ELF 中间产物，最终 .efi 需 PE 感知工具（`objdump -h -r` PE 模式 / llvm-objdump）检查 base relocation 块**） | ABI 定义 + spike 报告 | spike 全绿；失败则立即换机制（.S），不写完整梯子 | 16-smp 新 §；spike 结论入 commit | → ✅ **已完成（2026-09-07）**：`ApBootstrap` 40 字节 ABI 冻结（offset 断言钉住）；spike 全绿——见 §5.1 spike 报告 |
| S-3b | ✅ **已完成（2026-09-08）——代码经并行会话 commit e41929cf2 入库（提交归属错乱，内容完整）**——见 16-smp 新 §（AP early entry 梯子）与本行尾记录 | 梯子（16 实模式 cli/ds=ss/lgdt → PE → PAE+LME+PG 恒等续跑 → 64 位低位尾读记录 → 高位 Rust 汇合点）+ 数据区（MAGIC/记录/GDT 描述符+表，装配期常量寻址；entry→data 页隙以 DATA_GAP 常量化并由 hosted 测试钉住）+ BSP API（`install_at`/`fill_bootstrap`，含 <4GiB 断言）+ Rust 汇合点 `ap_early_entry(bootstrap_pa)`（DM 转 VA、字段入局部、§3.2 生命周期文档）| x86_64 early entry 模块 | 静态验收全过（hosted 布局契约测试 ×3 + 三目标 check；`test-smp-spike` 随梯子落地退役——16 位入口形态不可 64 位直调，L2 验证由 S-3d test-smp-ap-alive 接管；恒等覆盖由 PML4[0]=[0,4GiB) 构造性确认） | 16-smp 新 §（设计 + HigherHalf 分工 + 0x467 舍弃）；doc 02 交叉引用随 S-3d 一并 |
| S-3c | ✅ **已完成（2026-09-08）** aarch64/riscv64 early stub | 同一 `ap_early_entry(bootstrap_pa)` 汇合：arm PSCI context 传参 + MMU-off stub；riscv SBI opaque 传参（**修正 a2 误标 priv 的注释，改传 bootstrap 指针**，§2.1）+ satp
  stub；**本步打包三处固件 ABI 修复（v6 #1/#2，全部为现存代码真 bug）**：①riscv
  send_ipi EID 改 0x735049/FID 0（现 a7=0/a6=3 实为 SetTimer，见 §3.3）；②arm PSCI
  CPU_ON 换 0xC4000003（现 0x84000003 为 SMC32 截断位宽，见 §3.1）；③x86 `boot_ap`
  补 ICR 前置 `mfence`（C arch_smp.c:130 先例，现缺失——bootstrap 发布 fence，见 §3.9）；**首项核验：三架构恒等映射覆盖范围 vs stub/bootstrap 落点**（§3.2 闭环第 3 条） | 两架构 stub 模块 | hosted 无回归 + 构建门 + 恒等覆盖核验记录 | 16-smp §补两架构入口 | → **完成记录（2026-09-08）**：三修复全落地（①riscv legacy EID 0/FID 3 → IPI 扩展 0x735049/0——legacy 形态被现代 OpenSBI 废弃，ecall 静默 NOT_SUPPORTED；②arm PSCI CPU_ON SMC32→SMC64 0xC4000003，防 >4GiB entry 截断，context_id 改传 bootstrap 指针；③x86 boot_ap ICR 前补 mfence）；a2 语义勘误（opaque cookie 非 priv）；两架构 stub 骨架模块（义务清单+契约，实体随 S-4）；恒等覆盖核验静态完成（stub/记录在内核镜像内，BSP 根双映射）；arch 217 全绿 + 三目标构建门全过（16-smp §10）
| S-3d | AP alive 验证 | S-3 终点 = `ap_early_entry` 执行 `boot_ack_mask.fetch_or(1<<logical_id, Release)` 后 park（**只置握手位，不碰 online**——v3 #1 双状态拆分；`boot_ack_mask` 字段随本步引入 SmpState） | 测试内核 `test-smp-ap-alive` | L2：单 AP 跨完模式梯子，BSP 观察到 boot_ack 位已置（x86 为主件；arm/riscv 同测） | 16-smp §5.1 加行 | → **🔶 进行中（2026-09-08，WIP commit）**：SmpState.boot_ack_mask + publish/observe 方法与单元测试已落地；test-smp-ap-alive 内核已建（LAPIC 使能/镜像安装/记录填充/boot_ap INIT-SIPI/有界观测）。**当前症状**：INIT 与 SIPI 均已发出（ICR 状态 idle、ESR=0 无错误），但 AP 未到达 Rust 入口（marker=0）且无三重故障复位——嫌疑集中在梯子 16 位段内 lgdt 的 modrm 编码（raw 字节缺 0x67 地址尺寸前缀，16 位模式 rm=101 为 disp16）与 16→32 far-jump 的操作数宽度；下一步 = QEMU gdb 会话（-s -S 单步 AP 真实执行路径）。；**⏸ 暂停（2026-09-08）：AP bring-up 需 QEMU gdb 专注调试，切先做 Phase 4/5 可并行项，回归时从此处续** |
**第二轮进展（2026-09-08）**：①16 位段 far-jump 操作数宽度修正（EA 无 66 前缀时读 off16+sel16，原 .long 注入使 selector=0 → null selector #GP——旧日志的 v=0d@0x19 即此）；②lgdt disp 形式修正；③**blob 布局手工冻结**（code16@0 / GDT 描述符@0x30 / code32@0x40 / code64@0xC0 / GDT 表@0x110 / MAGIC@0x140 / 记录@0x148——全部 .org 边界 + Rust 常量镜像 + hosted 测试钉住）；④新增 hosted 测试（镜像 cli 首字节/GDT 描述符字节/记录布局镜像）。**第三轮进展（同日）**：梯子加入分段 trace 诊断（每级写线性 0x8200+i，BSP 超时 dump）后出现两种互斥症状——轮次 A：trace=[1,2,3,4,0…]（AP 走完 16 位与 far jump，未达 32 位入口——far jump 目标 0x8040 与 ap_protected 实际位置漂移，已用 .org 0x40/0x0C0 冻结修复）；轮次 B（.org 修复后）：trace 全零 + AP 无声（INIT/SIPI 交付层本身待证——引出 QEMU gdb 单步需求）。**下一步 = QEMU gdb（-s -S）单步真实执行路径**
**第四轮进展（2026-09-08）**：alive 测试内核简化重写（去除 trace 诊断、用固定 0x9000 scratch + `install_at(0x8000)` + `ap_entry` 发布 boot_ack）。**当前状态**：build ✓ 但 QEMU 运行仍超时（AP 未发布 ack）——需要 QEMU gdb（-s -S）在 AP 的 16 位入口 0x8000 设断点，确认 INIT-SIPI 后 AP 是否开始取指，以及 16→32→64 每级是否走通。候选根因：①INIT-SIPI 未送达目标 APIC；②16 位代码段的 lgdt 或 far-jump 编码仍然有误；③UEFI identity map 的 NX 位阻止了低位执行。：①16 位段 far-jump 操作数宽度修正（EA 后应读 off16+sel16，原 .long 注入使 sel=0 → null selector #GP——旧日志的 v=0d@0x19 即此）；②lgdt disp16 形式修正；③**blob 布局手工冻结**（C trampoline.S 同款：code16@0、GDT 描述符@0x30、code32@0x40、code64@0xC0、GDT 表@0x110、MAGIC@0x140、记录@0x148——全部数字常量 + `test_ladder_offsets_frozen`/`test_image_starts_with_cli_and_layout_markers` 钉住）；④新增 hosted 测试（镜像 cli 首字节/GDT 描述符字节/记录布局镜像）。**当前 QEMU 状态**：SIPI 已发出（ICR 状态 idle、ESR=0）但 AP 仍未达 Rust 入口（marker=0）且无 v= 异常日志——下一步 QEMU gdb（-s -S）单步真实执行路径
| S-4 | init_ap 真实现（D-39） | §3.3：x86 per-CPU GDT/TSS/ltr/GS_BASE/lidt + **per-CPU MSR 重编程**（STAR/LSTAR/SFMASK/EFER.SCE 每核必写 + GS_BASE 两变体，防"四核起来只有 CPU0 能 syscall"）+ **时钟基线核对**（x86 PIT 全局无 per-AP 工作，arm/riscv per-CPU 比较器可配，§3.3）；arm VBAR/TTBR/GICR；riscv satp（**SSIE 能力准备、置位推迟到 S-10**——v7 #2：若 S-4
置 SSIE 而 stvec 未装，IPI 到达即入无向量路径）；**lidt/VBAR/stvec = per-CPU attach 到 S-8 产出的完整共享表**（v8 #1 定位：S-8 是
BSP 公共陷阱基建、固定在本步之前——AP 侧只做 per-CPU load，一执行即得完整 trap 路径，
对齐 C `ap_finish_booting` 在 IDT 完整后才执行的事实）；**新增
子项：AP LAPIC local timer**（C `app_cpu_init_timer` → `init_local_timer` 对应物，
arch_clock.c:131-139：`tsc_per_ms/tsc_per_tick[cpu]` 校准 + `lapic_set_timer_one_shot`，
失败 panic——C 的 AP 真实接收本地 timer 中断；Rust 现仅有 PIT，此为真缺口，LVT 编程 +
one-shot re-arm + TSC 校准为本步新 infra，依赖 S-8 IDT 就位——S-8 现固定在 S-4 之前，时序成立；LVT 编程就绪但**投递仍受
IF=0 门控，S-10 才开启**（阶段不变量：S-8 后不存在空 vector/空 handler 状态，S-8~S-10
只差投递开关）；若阻塞则 AP 暂无本地 tick
并登记限制：AP 上 quantum 递减停摆，L5 idle 测试不受影响）；`ap_early_entry` 扩展为
init_ap → 主循环，**尾部自报 `online_mask` 位**（v3 #1 双状态拆分，§3.2）。**GS_BASE/swapgs 不变量（v3 #11 回应，源码核实）**：当前代码零 `swapgs` 使用者（kernel + x86 arch 全 grep 无命中）——首个 swapgs 是 S-8 的入口 stub；S-4 在任何入口路径存在之前写好 `IA32_GS_BASE`/`IA32_KERNEL_GS_BASE` 两 MSR，依赖顺序天然成立，S-8 设计时复核"CPL 判定后才 swapgs" | arch 三架构 init_ap + per-AP 栈来源定案 + **BSP-only vs per-CPU 归属清单**（含 TSS/栈/MSR 全量盘点，交付于本步） | L2 扩展：AP 内执行 Rust fn、读回 per-CPU 标识 + **online 位观察（init_ap 后）** | 16-smp §4.14 行更新；protection 模块文档 |
| S-5 | smp_init 编排 + boot_lock（D-36 下半 + D-37） | §3.4：kernel 层编排（拓扑 `hw_id` 传参 + boot_ap + wait_for_aps 复刻 BKL 舞蹈）；**per-AP 串行握手 + bounded 超时**（对齐 C arch_smp.c:124-141：AP 置 boot_ack 位 = 消费证明，BSP 观察后代置 CpuFlags.READY；超时语义 = bounded per-AP startup timeout，**常数按架构归 SmpArch**——x86 5 秒是 C i386 的 LAPIC one-shot 语义，arm/riscv 无 C 对应物（Minix3 SMP 仅 i386），取同量级设计值，计时机制 arch 自备）；**锁序表**（boot_lock/BKL/调度锁，以 C 源码实测为准，含"boot_lock 是否可删"论证）；CPU 状态机落地（§3.4，含 BSP 位 match `hw_id==bsp_id` 求逻辑号） | kernel `smp_init` + boot_lock（或其删除论证）+ 握手超时 | L4：`test-smp-aps` 三架构 `boot_ack_mask == expected_cpu_mask && online_mask == expected_cpu_mask` PASS（生产 wait 仍为 `online == boot_ack`——v7 #8 与 §3.4 对齐） | 16-smp §4.14 两行转 ✅；doc 08 boot 序列补 smp_init 位次（main.c:311） |
| S-6 | per-CPU 化四件套（D-40/D-41/sched-1/tick-1） | §3.5，**每件独立迭代独立 commit**；D-41 冻结语义按 §3.9 一次性发布规则 | 4 个 commit 逐个验证 | 每件：hosted 测试 + UEFI 构建 + L3 不回归 | 16-smp §D8；doc 04 §3.3（D-41）；doc 11（sched-1） |
| S-7 | AP 主循环 | §3.6：**AP 首次进调度循环时满足与 BSP 相同的 BKL 所有权前置条件**，此后获取/释放/让出复用既有路径；循环体复用 BSP 代码。**安全性声明（v2 外评 #7 回应 + v3 #10 三架构化，源码证据）**：当前代码不可能进 user mode——调度循环是 placeholder（todo I-6：`lib.rs` `loop { spin_loop() }`，bill_ptr 联动未接线）+ boot image 只有内核任务 ASYNCM/IDLE/CLOCK/SYSTEM/KERNEL 无用户进程（lib.rs:922）+ **三架构均无可达异常/IRQ 路径**（v8 #1 最终阶段不变量：**向量基址有效 + handler 完整（S-8 基建 + S-4 per-CPU attach）+ 中断投递关闭**——x86 IDT 有效 + IF=0；arm VBAR 有效 + DAIF 屏蔽；riscv stvec 有效 + `sstatus.SIE`/`sie`/SSIE 关闭。S-8 后任何架构都不再存在空 vector/空 handler 阶段，S-8~S-10 只差投递开关；无未屏蔽 IRQ 源前提不变——IO APIC RTE 初始化即 mask（plat interrupt.rs:124），LAPIC LVT timer 无 unmask 代码（arch_init.rs））——故 S-7 是"bring-up 测试安全"；**生产安全的 user entry/syscall 依赖 S-8**，16-smp 写明 | AP loop 接线 | L5：`test-smp-sched`——AP 走调度循环、串口可见 per-CPU 活动标记（测试内核维持无用户进程） | 16-smp §4.13 接入点表更新；doc 10（switch_to_user AP 侧衔接） |
| S-8（**v8 #1 定位：BSP 公共陷阱基建，固定在 S-4 之前**） | asm trap stub + 入口分流（hw-1/trap-1/D-46 硬件半环） | §3.7：范围已收窄（§2.3-2）——**IDT 路径与 SYSCALL 路径分开建模**：A. IDT stub（按向量归一错误码、保存上下文）→ 异常/IRQ 分流；B. LSTAR 指向的 SYSCALL 入口本体（SWAPGS + 用户 RSP 保存 + trapframe）→ `kernel_call_dispatch`。**向量基址与真 handler 本步一起安装**（x86 `set_handler` + `load()` BSP 侧执行；arm/riscv 同构 BSP `load`）。**职责边界
  （v8 #1 定稿，替换 v7 的"前移"表述）**：S-8 = **BSP 侧公共陷阱基建**——asm stub 本体
  + 共享 handler 表填满 + BSP load，产出"每个 CPU 的 per-CPU load（`lidt`/VBAR/stvec）
  一执行即得完整 trap 路径"的公共资产；**AP 投递在本步不开启也无须开启**（AP 尚不存在
  ——本步纯 BSP 工作，无 S-3/S-4/S-5/S-6/S-7 依赖）。此前 v7 的排序（S-4→S-8）存在
  步骤依赖倒置：S-4 的 `lidt` 引用"S-8 已填满的表"而 S-8 排在其后。C 顺序佐证：
  `protect_init`/`idt_init`（带真 handler）在 AP 启动前完成——S-8 即该位置的 Rust 对应 | x86_64 asm trap stub 模块（A+B）；arm/riscv stub 盘点结论 | L3：`test-timer-irq`——tick 推进 uptime，串口打印 | doc 14 + doc 13 §6.3（kernel_call 接线）+ doc 08（D-46 硬件半环行） |
| S-9 | 异常入口 BKL（D-38①） | 异常 dispatcher 进 handle 前 `bkl_lock()`（依赖 S-8 asm 入口） | D-38① 落地 | L3 回归 + hosted mock 单元测试 | 16-smp §4.13 行 ✅；todo.md Edge D-38① 划掉 |
| S-10 | IPI 往返验证 | AP 有 IDT 后：BSP↔AP `schedule_sync`/`ipi_sched_handler` 真实往返（§2.1 的 SMP 机制第一次在真多核上跑；riscv 侧按 §3.3 走 SSIE 软件中断路径） | L6：`test-smp-ipi` | BSP 发 IPI → AP 处理 → ack → 标志清零，串口判定 | 16-smp §5.2 相应行；§2.6 调用图验证注记 |
| S-11 | shutdown(0)（hw-2） | §3.8：`minix_shutdown` 语义层 + **三架构 QEMU 测试后端**（x86 isa-debug-exit / arm semihosting / riscv sifive_test），硬件后端（ACPI S5/PSCI SYSTEM_OFF/SBI SRST）留接口 | shutdown 分层接口 + 三架构测试后端 | L7：`test-smp-shutdown`——QEMU 按预期退出码结束（三架构） | doc 27（utility）shutdown 语义；Edge hw-2 划掉 |
| S-12 | 测试债 + CI 化 | doc 16 §5.2/§5.3 的待补测试（T-2 七个 SMP 行为测试、T-3 init/load 顺序约束、T-4 init_ap 路径验证、T-5 QEMU 脚本进 CI） | 测试 commit ×N | `cargo test` 全绿 + run_all.sh 一键全量 | 16-smp §5.2/§5.3 清账；todo.md §7.3 对应行 |
| S-13 | 收尾 sweep | todo.md Edge Items 表整体迁移（逐项标 ✅/日期/证据）；doc 16 状态列刷新；本文件封存 | 收尾 commit | §6 全阶梯重跑一次全绿 | todo.md、16-smp.md、本文件 |

**顺序依赖（v8 #1 定稿）**：S-0→S-1→S-2 必须最先；S-3a→S-3b→S-3c→S-3d→**S-8**→
S-4→S-5→S-6→S-7→S-9→S-10→S-11→S-12→S-13。**S-8 = BSP 公共陷阱基建（asm stub + 共享
handler 表 + BSP load），固定在 S-4 之前**：S-4 的 per-CPU attach（`lidt`/VBAR/stvec）
依赖 S-8 产出，v7 的 S-4→S-8 排序是步骤依赖倒置（本 P0 的根因）；重排后消灭跨 CPU
`lidt` 鸡生蛋，且 S-8 后全系统不存在空 vector/空 handler 阶段。S-8 纯 BSP 工作无
S-3~S-7 依赖，前移无成本。S-6 仍在 S-7 前（AP 主循环依赖 per-CPU scheduler）；S-11
可在 S-8 后任意时点插入；S-12/S-13 收尾。spike 失败换机制不影响后续步骤定义。

---

## 6. QEMU 测试阶梯（一步一测，龟速前进）

> 每级只加一个测试内核，绿了才爬下一级。全部走 `run_qemu.sh`
> （内建 `timeout 30` + 外层 `timeout 120`），PASS 判据 = 串口出现
> `### TEST_RESULT: PASS <name> ###`。

| 级 | 测试内核 | 断言 | 解锁 |
|---|---|---|---|
| L0 | 现存 7 个 x86_64 测试 | 全绿基线 | S-0 |
| L1 | test-smp-topo | `nr_cpus==4`、每 CPU `hw_id` 合法、per-arch 来源（MADT/DTB） | S-2 |
| L2 | test-smp-ap-alive | AP 完成模式梯子（x86：16→32→64）、置 **boot_ack_mask** 自身位、执行 Rust fn；**断言 `boot_ack_mask == expected_cpu_mask`**（early entry 全员可达，v5 P1-4 统一命名） | S-3a~S-3d（S-4 在本测试内核上追加 per-CPU/online 断言——v9 nit 1，S-3d 时 init_ap 未实现，L2 本体不验证 S-4） |
| L3 | test-timer-irq | 时钟中断真实到 IDT→handler→uptime 推进（**v7 #2：S-8 前移后本级提前**） | S-8、S-9 |
| L4 | test-smp-aps | **`online_mask == expected_cpu_mask` 且 `boot_ack_mask == expected_cpu_mask`**（v4 #3 + v5 P1-4：生产条件 + 测试健康性双等式，杜绝全 AP 失败的假通过）、`wait_for_aps` 返回（含 BKL 舞蹈不死锁） | S-5 |
| L5 | test-smp-sched | AP 以与 BSP 相同的 BKL 前置条件进调度循环，per-CPU 状态就位 | S-6、S-7 |
| L6 | test-smp-ipi | BSP→AP 调度 IPI 真实往返（发→处理→ack→清零） | S-10 |
| L7 | test-smp-shutdown | shutdown(0) → QEMU 按预期退出码结束 | S-11 |

aarch64（PSCI）与 riscv64（SBI）的 L1/L3/L4 同步验证——无模式梯子，路径更短；
riscv64 非 EFI 测试内核按 run_qemu.sh 既有 fallback 走 OpenSBI `-bios default`。

---

## 7. Edge 判定规则与 edge_todo.md 约定

本项目按 Minix3 微内核服务边界分 stage；SMP bring-up 的全部生产代码（boot/BKL/per-CPU/
IPI/调度原语/shutdown）都在 kernel stage 内部，不依赖任何用户态服务器——本计划所有 S-x
均为 stage 内工作。判为 edge（追加 `edge_todo.md`，单线程后续执行）的只有两类：

1. **联合调试测试**：需要用户态服务器（VM/PM/RS/VFS）真实运行才能验证的 SMP 行为——
   例：多核下 VM 页错误挂起/恢复全链、RS 经系统调用触发的跨核 stop/migrate 端到端。
2. **跨 stage 接线**：本 stage 改了协议/语义且对端在别的 stage——例：per-CPU 迁移若改变
   VM 可见的全局符号语义，需 02-stage-vm 同步。

执行中发现疑似 edge 项：先按上述两类判定；判 edge 则追加到 `edge_todo.md`（首次创建，
含"日期 / 来源 S-x / 阻塞依赖 / 建议执行 stage"四列），当前 S-x 继续推进不阻塞。

---

## 8. 风险与已知盲区

| 风险 | 缓解 |
|---|---|
| hosted CI 盲区（默认 mock 不编译生产路径，§2.4） | §4 验证清单强制 UEFI 构建门；S-0 起三架构基线矩阵固化 |
| early entry image 的 relocation 陷阱（拷到低地址即炸，§3.2） | S-3a toolchain spike 先行（`readelf -r` 零 reloc + 拷贝执行验证）；PIC/relocation-free 写成硬约束 |
| AP 系统调用炸弹（per-CPU MSR 漏写：四核起来了但只有 CPU0 能 syscall，§3.3） | S-4 交付物含 BSP-only vs per-CPU 归属清单；L4 测试覆盖 AP 侧路径 |
| ARM/RISC-V 弱内存序暴露 x86 上不可见的问题（§3.9） | §3.9 统一 Acquire/Release 契约，各迁移项不再各自决定 |
| x86_64 early entry 时序敏感（INIT/SIPI 延时、低内存落点） | L2 单独成级；SIPI 重发对齐 Intel SDM §8.4（现有 boot_ap 已含 200µs 二次 SIPI） |
| x86 低内存 <1MiB 可用性 | UEFI 可能回收传统低内存且 boot-shim memmap 未必保留——S-3b 首查 `KernelInfo.memmap`；无保留区则扩 boot-shim 或独立 low-memory 池（C 对应 `alloc_lowest`，arch_smp.c:76） |
| per-CPU 迁移波及面（D-40/D-41 触及多处调用点） | 每件独立迭代独立 commit，L3 回归兜底 |
| AP 收到 IPI 时 trap 路径不可用（旧模型风险，v8 #4 已消除） | 阶段不变量：S-8 起全系统无空 vector/空 handler；AP 自 S-4 起拥有完整 trap 路径，仅投递关闭；IPI 投递在 S-10 开启 |
| wait_for_APs 的 BKL 舞蹈漏接 → 死锁 | §3.4 明示 C 语义；L3 测试断言包含"BKL 舞蹈不死锁" |
| 握手/上线状态被合并（v2 曾犯，v3 外评捕获） | 双位图纪律：boot_ack_mask（early entry）与 online_mask（init_ap 尾）生命周期分离，§3.9 内存序表同步约束；L2 只断言 boot_ack |
| ~~x86 NMI 在无 IDT 窗口不可屏蔽~~（v8 #4 删除：该窗口已不存在——BSP 自 S-8、AP 自 S-4 均持有效 IDT，NMI 落到真 handler 可观察） | — |
| AP ack 后继续读复用 image（v5 P0-1 竞争） | image 生命周期不变量：boot_ack Release = 最后一次读取；字段先拷局部后 ack（§3.2） |
| 锁序错配（boot_lock × BKL × 调度锁，§3.4） | S-5 产出锁序表，以 C 源码实测为准；boot_lock 无真实对手时允许论证删除 |
| shutdown 方案影响物理机语义 | §3.8 分层隔离：`minix_shutdown` 语义层 / QEMU 测试后端（三架构各异）/ 硬件后端（后续）三层分离 |
| arm/riscv trap stub 现状未逐行核验 | S-8 开工首项盘点，不预设结论（§3.7 如实标注） |

---

## 9. GPT 外评采纳记录·第一轮（2026-09-06，smp_gpt.md 18 条逐条裁定）

> 外评意见仅是意见：采纳与否一律以源码/规范核实为准。核实证据标注在裁定栏。

| # | 外评要点 | 裁定 | 依据 |
|---|---|---|---|
| 1 | RISC-V `hart_start` 第三参数是 `opaque` 不是"特权级"；AP 醒来 a0=hartid、a1=opaque、satp=0、SIE=0 | **采纳（P0）** | ratified SBI HSM 规范无特权参数（"priv" 是 SBI v0.1 草案 API）；Rust `riscv64/smp.rs:155` 注释确实误标，当前传 0 碰巧合法（opaque=0）。S-3c 修注释并改传 bootstrap 指针 |
| 2 | `#[naked] fn` 模型不诚实；改 `global_asm!` flat blob；加 PIC/relocation-free 硬约束 + toolchain spike | **采纳（P0）** | naked function 语义仍绑定函数 ABI，而 AP 启动代码无 ABI 无栈无寄存器状态；spike 作为 S-3a 第一验收门 |
| 3 | mailbox 拆代码/数据段；bootstrap 收敛为单 opaque 指针 + `ApBootstrap` 结构 | **采纳** | 段权限分离（可执行 vs 可写）+ 三架构在 `ap_early_entry` 汇合；x86 保持 C 的串行单邮箱模型 |
| 4 | IDT 入口与 SYSCALL 入口是两条硬件路径；per-CPU MSR（LSTAR/STAR/SFMASK）每个 AP 必须重写 | **采纳（P0/P1）** | SYSCALL 经 MSR LSTAR 不经 IDT 向量（§3.7 拆分 A/B）；S-4 增 per-CPU 归属清单 |
| 5 | shutdown 三架构 QEMU 退出后端各异（isa-debug-exit 仅 x86） | **采纳** | §3.8 重构为语义层/QEMU 后端/硬件后端三层，arm semihosting、riscv sifive_test |
| 6 | CPU 身份模型：logical_id 与 hw_id 分离、类型化 | **部分采纳** | kernel 已有 `CpuId` newtype；`CpuTopology` 数组下标即逻辑号（不重复建字段），`boot_ap` 改传 `hw_id`；`HwCpuId` 包装 S-5 实施时定 |
| 7 | CPU 状态机（READY/booted/online 不许压成一个 flag） | **采纳** | C 本就是双信号（`CPU_IS_READY` + `ap_cpus_booted`，smp.c:30-51）；Rust `CpuFlags.READY` + 状态机映射 |
| 8 | `ap_cpus_booted` 计数器升级 bitmask | **采纳** | 计数器的 3 可能来自错误组合；位图一次断言数量/身份/重复/缺席；`MAX_CPUS=32` 天然契合 |
| 9 | 内存序契约缺失 | **采纳** | 新增 §3.9（bootstrap/online/IPI/冻结四类 Release→Acquire 规则） |
| 10 | S-3 与 L2 小鸡生蛋（init_ap 尚是 panic） | **采纳** | S-3 终点改为 `ap_early_entry` 写 magic 后 park；S-3 拆 a/b/c/d 四步 |
| 11 | ARM/RISC-V "无低内存问题"要收紧为物理地址不变量 | **采纳** | §3.2 物理地址不变量四条（entry/bootstrap/页表根/栈顶） |
| 12 | PLIC 不是 IPI 前置（软件中断走 SBI→CLINT MSIP） | **采纳（P1 事实修正）** | Rust `riscv64/smp.rs:67` 实走 legacy SBI send_ipi（EID 0）；收 IPI 前置是 `SIE.SSIE`；PLIC 归 S-8 外部中断路径 |
| 13 | x86 per-CPU 本地时钟归属 init_ap | **采纳** | `ClockArch::init_timer(hz, cpuid)` 基础设施已存在；S-4 增"CPU-local 时钟基线" |
| 14 | S-2 不把 APIC ID {0,1,2,3} 当 parser 语义契约 | **采纳** | 断言改 hw_id 互异 + BSP ∈ 发现集；机器特定期望值单列 |
| 15 | S-0 三架构固定基线矩阵 | **采纳** | 三架构 = hosted + 生产构建 + QEMU smoke，制度化 |
| 16 | S-7 "AP 持 BKL 进主循环"措辞误导 | **采纳** | 改为"首次进循环满足与 BSP 相同的 BKL 前置条件，此后复用既有路径" |
| 17 | boot_lock 锁序表 | **部分采纳** | 锁序分析列为 S-5 交付；但外评建议的"boot_lock → BKL、禁止 BKL → boot_lock"顺序**不预设**——C 的 `smp_start_aps` 拿 boot_lock 时 BSP 很可能已持 BKL，以 C 源码实测为准；boot_lock 无真实对手时允许论证删除 |
| 18 | 不要建 Trampoline trait / 不要第二个 boot 抽象体系 | **采纳** | §3.2 改名 AP early entry；汇合点是 Rust 函数 `ap_early_entry`，汇编为 arch 私有实现细节 |

**未采纳/存疑项**：无。其中 #17 的锁序方向与外评建议相反的可能性已在 §3.4 标注，留 S-5 用源码裁定。

---

## 10. GPT 外评·第二轮回应（2026-09-06，smp_gpt_v2.md 十问逐条源码级回答）

> v2 外评定性为"查证题"——以下每问以源码/规范作答，答案已回写进正文对应节。

**Q1（P0）`ApBootstrap` → `ap_early_entry` 的地址空间交接**
答：交接四步闭环已补入 §3.2——bootstrap 一律 PA 传递（x86 经 mailbox，arm/riscv 经固件
context 寄存器）；MMU-off asm 只解引用 PA 字段（页表根）；MMU on 后 PC 仍处恒等映射区
（x86 PML4[0] 恒等 [0,4GiB) 在 DM coverage 建立后保留，paging.rs:727-795），此后才读
`rust_entry_va`/`kernel_stack_top_va` 两个 VA 数据字段；Rust 边界收 PA：
`unsafe fn ap_early_entry(bootstrap_pa: usize) -> !`，内部经 `DirectMapArch::kernel_phys_to_virt`
转 VA（三架构基址 `os/arch/src/arch/direct_map.rs:81/:115/:146/:169`）。blob 引用
`ap_early_entry` 符号的问题不存在——它从 bootstrap 数据字段读入口 VA，自身保持
relocation-free。

**Q2（P0）`ApBootstrap` 跨汇编 ABI**
答：`#[repr(C)]` + 固定宽度字段（§3.2 结构体已改）；汇编 offset 不手写——经
`global_asm!` 的 `const` 操作数注入，值来自 `core::mem::offset_of!`，Rust 侧加删字段
即编译失败；另加 `const _: () = assert!(size_of::<ApBootstrap>() == N)` 钉总尺寸。
项目先例：`CpuInfoEntry` repr(C)（D-53）、`PrivUpdateRequest` repr(C)（D-51）。

**Q3（P0）MMU 切换瞬间可访问性**
答：依据 = 三架构 BSP 页表含恒等映射（HigherHalf 契约 doc 02 §3.4；x86 为 PML4[0]
[0,4GiB)，见 §3.2 闭环第 3 条）。x86：blob/bootstrap 落 <1MiB 恒等区，MMU on 后 PC 与
PA 读继续有效；final kernel stack 以 VA 形式由 bootstrap 携带、MMU on 后由
`ap_early_entry` 切换（切换点 = Rust 函数第一条指令前的 `mov rsp`）。arm/riscv：stub
落点必须落在恒等映射覆盖区内——**S-3c 首项核验三架构恒等映射范围 vs 落点**，不覆盖则
挪落点或扩恒等映射。x86 附加不变量：BSP 页表根 PA <4GiB（32 位模式 `mov cr3` 约束），
S-3b 加 assert。

**Q4（P0）`ready_ptr` 语义冲突**
答：冲突成立，解法比外评建议更干净——**废弃 magic**：early entry 直接执行生产动作
`online_mask.fetch_or(1 << logical_id, Release)`，S-3d 的"活着"证据 = 位已置；无测试
专用字段，正式 ABI 不被污染；`fetch_or` 按位幂等兼防重复报到。`ready_ptr` 更名
`online_mask_pa`（§3.2）。

**Q5（P1）单 mailbox 是否真有握手**
答：**C 有完整握手 + 超时**（外评与原文此前都转述不完整）——arch_smp.c:124-141：BSP 置
`ap_cpu_ready = -1` → 写 `__ap_id` mailbox → `mfence()` → INIT/SIPI → LAPIC one-shot
**5 秒**计时 → 自旋等 `ap_cpu_ready == cpu` → 观察到则 BSP 代置 `CPU_IS_READY` 并 break；
超时 WARNING "CPU didn't boot" 跳过。单邮箱安全的根基 = 该串行握手。Rust 逐条对齐
（S-5 交付物已更新）。

**Q6（P1）S-4 timer 与 S-8 trap 时序**
答：`init_timer` **只配置不使能**——x86 配的是全局 8254 PIT（cpu_id 被忽略，
clock.rs:57-63 自注），per-CPU LAPIC timer 属后续；arm/riscv 比较器为 per-CPU 语义、
S-4 可配。IRQ 送达链四级：①源配置 → ②控制器 unmask（`ArchInit::init`）→ ③CPU 允许
（x86 IF / arm DAIF / riscv `sstatus.SIE` **且** `sie.SSIE`）→ ④trap 入口。④现状不存在：
**IDT 的 `load()` 从未执行**（lib.rs `init_protection` 自注 "Do NOT call trap.load()
here"）——真正的 load 属 S-8。故 S-4/S-7 无 IRQ 可达，前提是保持③屏蔽；S-7 的
pause(sti;hlt) 依赖"无未屏蔽 IRQ 源"不变量（与 BSP 现状一致），S-7 验收写明取舍。
"AP 可接收可屏蔽中断"的最终解锁点 = S-8 的④ + 显式①②③序列（§3.3 已改写）。

**Q7（P1）S-7 是否可能进 user mode**
答：**当前代码不可能**，三重证据：①调度循环是 placeholder（todo I-6：lib.rs
`loop { spin_loop() }`，bill_ptr 联动未接线）；②boot image 只有内核任务
ASYNCM/IDLE/CLOCK/SYSTEM/KERNEL，无用户进程（lib.rs:922）；③无 IDT 在位（Q6）。
故 S-7 定性"bring-up 测试安全"而非"生产路径安全"，user entry/syscall 依赖 S-8
（S-7 行已加安全性声明，16-smp 将写明）。顺序不调整。

**Q8（P1）`online_mask` 的 BSP 位**
答：位下标 = 逻辑号（拓扑数组下标），非 hw_id；BSP 位由编排层在启动任何 AP 前设置；
BSP 逻辑号 = 拓扑中 `cpus[i].hw_id == bsp_id` 的 match 结果（**解析顺序不保证 BSP 第一，
禁止假设 0**）；`expected_mask` = 拓扑 0..nr_cpus 全位；`fetch_or` 幂等防重复。
READY 与 online 的设置者分离对齐 C：AP 置 online 位（= C `ap_cpus_booted` 自报），
BSP 观察后代置 `CpuFlags.READY`（= C `cpu_set_flag` 代置，arch_smp.c:137）。

**Q9（P1）FAILED/timeout 是否改变 C 语义**
答：**不改变——C 本就有超时**。外评的前提（"C 可能无限等待"）不完整：smp_start_aps
的 per-AP 5 秒 LAPIC one-shot 超时（arch_smp.c:131-141）超时即 WARNING 跳过；仅
wait_for_APs 一级无超时（等"所有 READY 的 CPU 报到"）。Rust 状态机两级全保留：
per-AP 超时（FAILED，C 忠实）+ wait_for_APs 无超时。不需要引入任何新行为。
（此问的价值在于暴露了原文对 C 超时语义的遗漏，而非纠正偏离。）

**Q10（P1）IPI 与 Acquire/Release 的职责边界**
答：现有代码**已经**把正确性建立在原子序上：`SchedIpiData` 的
Acquire load / AcqRel fetch_or / Release 清零（smp.rs:288/293/298）+ IPI 发送前后
SeqCst fence（:537/:585）。§3.9 已按外评建议改写为职责分离表述："IPI 是通知机制，
Release/Acquire 是数据发布机制"；删除"IPI 自带屏障语义"的表述。遗留设计点仅一个：
SeqCst fence 是否降级为 Release/Acquire（更强序保留亦可，S-10 记录后定）。

**纯文档五处（外评 §十）**：术语表 `boot_ap` 签名更新 ✓；段名 `.ap_trampoline` →
`.ap_early_entry` ✓；Redox 措辞 ✓；S-0 "UEFI 构建" → production-target 构建（riscv 走
OpenSBI）✓；§9 标题计数修正 ✓。

**结论**：十问全部给出源码级答案；其中 Q1-Q4（P0）的答案已直接改写 §3.2/§3.3/§3.4
设计正文，Q5/Q9 修正了对 C 语义的转述（C 有握手有超时），Q6/Q7 以"当前不可能 + 前提
不变量"定性。设计进入可冻结状态。

---

## 11. GPT 外评·第三轮回应（2026-09-06，smp_gpt_v3.md 七问逐条源码级回答）

**Q1（P0）boot 握手与 online 状态分离**
答：**指控成立，采纳**。v2 曾让 early entry 置 `online_mask` 位，使"已消费 bootstrap"
与"完成 init_ap"被合并——parked AP 也会让 wait_for_aps 误判就绪，是真实 bring-up bug。
源码同时证实 C 的握手时机比原文转述的还早：`ap_cpu_ready = cpu` 是 `ap_finish_booting`
的**第一条语句**（arch_smp.c:219，先于 boot_lock/BKL/cpu_identify 一切初始化）——
所以"握手位 ≈ 到达 early entry"是 C 原义。修复：`boot_ack_mask`（early entry 置位，
对应 `ap_cpu_ready`）与 `online_mask`（init_ap 尾部自报，对应 `ap_cpus_booted`
smp.c:51）两个独立位图；wait_for_aps 条件 = `online_mask == boot_ack_mask`（即 C 的
`ap_cpus_booted == n-1`）；`CpuFlags.READY` 保持 BSP 观察到 boot_ack 后代置（arch_smp.c:137）。
S-3d/L2 只断言 boot_ack 位，不碰 online。§3.2/§3.4/§5/L3 全部改写。

**Q2 签名不一致**
答：成立，已统一为 `unsafe fn ap_early_entry(bootstrap_pa: usize) -> !`（§3.2 汇合点
行 + Rust 边界两处），汇编边界永远传 PA，`bootstrap_pa → direct-map VA → &ApBootstrap`
的转换发生在 Rust 函数内部（位置与机制见 §3.2 闭环）。

**Q3 段内互引如何 relocation-free**
答：**单段方案**——`.ap_early_entry` 一个段内固定布局 [16 位代码][32 位代码][64 位
代码][mailbox 数据]，整体拷贝后段内相对距离不变：16 位用 CS 相对（mailbox 偏移为汇编
期常量）、32/64 位用 RIP 相对（链接期解析，拷贝后 delta 有效）、**禁止跨段符号引用**。
`readelf -r` 零未解析 reloc 是必要条件而非充分条件；最终证据 = spike 的"同一机器码、
不同基址、执行成功"（§3.2 已改写验收表述）。**（v7 #5 已修正本条："单段"精化为"单
runtime image = 两 section 连续布局"，现行设计见 §3.2 image/section 模型。）**

**Q4 linker 是否 GC 掉 early-entry 段**
答：以实际配置核实——x86 UEFI 测试内核**无自定义 link.ld**（默认 lld 脚本），riscv64
测试内核有项目自有 link.ld，workspace .cargo/config.toml 无 gc-sections 相关 link-arg。
处置：边界符号 `ap_early_entry_start/end` 在 asm 内部自定义（不依赖链接器生成）；
gc-sections 行为由 spike 在真实链接路径实测；若 GC 开启则关 link-arg 或（I-1 接入
kernel link.ld 时）加 `KEEP(*(.ap_early_entry))`。列入 S-3a 检查单。

**Q5 PA/VA 不变量最终版**
答：两栏拆分（§3.2 表）：MMU-off only = `bootstrap_pa`/`page_table_root_pa`/boot
临时栈 PA；MMU-on only = `kernel_stack_top_va`/`rust_entry_va`。SP 时间线三架构统一：
**stub 是无栈叶子**（全程关中断、零压栈，MMU-off 阶段不需要栈）；所需值在开 MMU 前全部
预载寄存器；MMU on 后 asm 只做"装最终 SP（寄存器值）→ 跳 rust_entry_va（寄存器值）"
两条指令，**不再触碰任何内存**；`kernel_stack_top_va` 的切换点 = blob 尾部这两条指令，
`ap_early_entry` 看到的已是最终栈。bootstrap 数据 MMU on 后由 Rust 侧 direct map 转换，
asm 不再读它。恒等映射只需覆盖 stub 代码区（取指）。**（v4 #1 已修正本条：x86 因 32 位寄存器宽度的现实，两个 VA 的读取发生在 64 位阶段、经恒等映射，"MMU on 后零内存访问"只对 arm/riscv 成立——现行设计见 §3.2"无栈叶子原则"。）**

**Q6 trap 不可达证明三架构化**
答：采纳。S-7 安全声明已改写为统一抽象"三架构均无可达异常/IRQ 路径"，分架构证据：
x86 = `lidt` 未执行（lib.rs init_protection 自注 "Do NOT call trap.load() here"）+ IF=0；
arm = VBAR_EL1 未安装（trap_entry.rs:62 自注 load() 才写 VBAR）+ DAIF 屏蔽；riscv =
stvec 未安装（trap_entry.rs:63 同构）+ `sstatus.SIE`/`sie` 关闭；公共前提 = 无未屏蔽
IRQ 源（x86 arch_init.rs 现无 LVT unmask 代码，PIC 状态 S-7 验收时核对）。

**Q7 GS_BASE/swapgs 顺序**
答：源码核实——当前 kernel + x86 arch **零 swapgs/GS_BASE 使用者**（全 grep 无命中）：
首个 swapgs 就是 S-8 要写的入口 stub，而 S-4 在任何入口路径存在之前写好双 GS MSR，
依赖顺序天然成立、无缺口。S-4 的 BSP-only vs per-CPU 归属清单扩充为全量盘点（GDT/TSS/
栈/STAR/LSTAR/SFMASK/EFER/GS_BASE 双变体），交付于 S-4；S-8 设计时复核"CPL 判定后才
swapgs"（kernel 态进入不得 swapgs 翻转）。

**附：v3 期间的额外源码发现（超出七问）**：`ap_finish_booting` 的锁顺序为
**boot_lock → BKL**（arch_smp.c:222-224，注释说明原因：LAPIC timer 校准的嵌套中断使
BKL 单独不够）——v1 #17 遗留的锁序方向问题获得 AP 侧源码证据，BSP 侧留 S-5 读
main.c 调用链裁定（§3.4 已补记）。

**结论**：七问全部源码级回答；#1 为真实 bug 并已修复（双状态拆分贯穿 §3.2/§3.4/§5/L3）；
#2-#4、#6-#7 为表述/验收精确化；#5 为不变量收严（无栈叶子 + 寄存器预载）。外评确认
"global_asm! 方案无原则性异议"。设计达到可冻结状态。

---

## 12. GPT 外评·第四轮回应（2026-09-06，对话框直传意见，三 P0 + 三 P1 + 小点）

**Q1（P0）"无栈叶子"与 x86 PA→VA 读取的矛盾**
答：**成立，采纳修正**。矛盾根源：16/32 位阶段寄存器只有 32 位宽，装不下 64 位 VA，
"开 MMU 前全预载"在 x86 上不可行。按架构拆分（§3.2）：arm/riscv 保留"真·全预载 +
MMU on 后零数据访问"；x86 改为"64 位阶段（分页已开）经**恒等映射**读取
`kernel_stack_top_va`/`rust_entry_va` → `mov rsp` → `jmp`"，统一表述收敛为
**"无栈（零 push/call/pop）+ MMU on 后只经恒等映射读内存"**——外评建议的原句采纳。
恒等映射覆盖要求随之从"代码区"扩为"image 全体"。

**Q2（P0）`bootstrap_pa` 地址限制**
答：**用更强的设计消解而非加不变量**——`ApBootstrap` 内嵌 early entry image 数据区
（固定偏移，C `__ap_id` 同款布局），BSP 拷贝后经 direct map 填字段、串行复用。x86 的
全部地址约束收敛到既有的 image < 1MiB（SIPI 硬性要求）；arm/riscv 经 PC 相对定位，
固件 context 退为备用。"real mode 能否解引用 64 位 PA"的追问不复存在：x86 16 位阶段
只做 CS 相对访问，从不解引用完整 64 位地址。

**Q3（P0）L3 假通过**
答：**成立，采纳**。生产语义与测试健康性拆为两层：生产 wait_for_aps 保持
`online_mask == boot_ack_mask`（C 忠实，少核降级继续）；测试断言必须为
`boot_ack_mask == expected_mask && online_mask == expected_mask`——"三 AP 全挂、两掩码
都只剩 BSP 位"的相等性不再能骗过测试。L2/L3 行与 §3.4 已改写（L2 同时升格为
`boot_ack_mask == expected_ap_mask`，early entry 全部可达性独立成级，v4 #10 一并采纳）。

**Q4（P1）boot_lock 删除论证**
答：**采纳，立场反转**。"单 BSP 串行调用故无竞争"的理由作废——AP 自己就是竞争者
（arch_smp.c:222-224）。默认**保留** boot_lock；C 保护对象已钉住（timer 校准解锁/重获
BKL，单独 BKL 不够，arch_smp.c:223 原注释）；"更小原语替代"降级为 SMP 跑通后的优化
评估项，删除需完整 happens-before 论证。

**Q5（P1）S-4 装向量 vs S-7"未安装"证明的矛盾**
答：**成立，采纳统一时序**：S-4 安装向量基址（x86 `lidt` / arm `load_ap`→VBAR_EL1 /
riscv `load_ap`→stvec——per-CPU load_ap 分工在 trap_entry 模块文档已有），投递保持
关闭；S-7 安全证明改为"向量基址已装但投递关闭"（IF=0 / DAIF / `sstatus.SIE`+`sie`，
加"无未屏蔽 IRQ 源"公共前提——IO APIC RTE 初始化即 mask，plat interrupt.rs:124）；
S-8 = 填真 handler + 开放投递。三段式"装 base → 关投递 → 开投递"阶段模型成立。

**Q6（P1）5 秒超时是 x86 外推**
答：**成立，采纳**。arm/riscv 无 C 对应物（Minix3 SMP 仅 i386 实现）。统一语义 =
bounded per-AP startup timeout，**常数与计时机制归 SmpArch**（x86：5 秒，C 忠实；
arm/riscv：同量级设计值，机制 arch 自备）。S-5 行已改写。

**Q7（P1）SBI legacy send_ipi 调用形式**
答：已原样列出（§3.3）：`a7 = 0x0`（legacy 扩展 EID）、`a6 = 0x3`（FID send_ipi）、
`a0 = 1 << hartid`（hart mask）、`a1 = 0`（hart_mask_base）；OpenSBI 写目标 hart 的
CLINT MSIP → 软件中断。"legacy"指 ratified 规范把 v0.1 函数收纳为 EID 0x0 扩展，
非独立 ABI。

**Q8（P1）"防重复"表述**
答：采纳收窄：fetch_or 幂等 = **重复置位不膨胀计数**；数量/身份/缺席由最终 mask 判定；
检测重复本身可选 `debug_assert!(old & bit == 0)`，不增加正式状态。

**Q9（P1）掩码 PA 的生命周期**
答：**用更简单的设计消解**——掩码置位全部发生在 Rust（MMU on、内核地址空间），
直接引用 SmpState 全局（.bss static、内核终身、cacheable normal、coherent 语义由
Release/Acquire 承担）；**asm 不写掩码、ApBootstrap 不携带掩码 PA**——两字段删除，
"PA 正确 ≠ 原子访问合法"的隐含假设整个消失。

**Q10（L2/L3 职责再分）**：采纳——L1 topology → L2 AP entry（boot_ack == expected）
→ L3 AP init（双等式）→ L4 scheduler → L5 IPI → L6 timer/trap → L7 shutdown，
阶梯语义完整闭环。

**文档小点两处**：S-3a 增加"UEFI 产物是 PE/COFF，readelf 只适用 ELF 中间产物，最终
.efi 用 PE 感知工具检查 base relocation 块"；§4 验证命令从 x86-only 泛化为
production-target（与 S-0 三架构矩阵一致）。

**结论**：三 P0 全部成立并修复（其中 #2/#9 以更简单设计消解而非加补丁），三 P1 全部
采纳，两处文档小点修正。外评"查完可冻结"的前提已满足——设计冻结，S-0 可开工。

---

## 13. GPT 外评·第五轮回应（2026-09-06，对话框直传意见，两 P0 + 三 P1）

**Q1（P0）boot_ack 发布后 image 复用的生命周期竞争**
答：**成立，采纳方案 A**。竞争真实存在：`ap_early_entry(bootstrap_pa)` 在 ack 之后仍需
构建 `&ApBootstrap`（读 logical_id 等），而 BSP 观察到 ack 即重写 image 给下一个 AP——
除非把"最后一次读取"钉在 ack 之前。定稿不变量（§3.2 已入正文）：
**"`boot_ack` 的 Release 发布 = AP 已完成对当前可复用 image 的最后一次读取"**。执行序 =
方案 A：asm 的读取天然在 Rust 进入前（页表根→CR3、VA→寄存器）；Rust 侧把后续所需字段
（`logical_id`/`hw_id`/`kernel_stack_top_va`）先拷局部变量再发 ack，此后 `bootstrap_pa`
不再解引用、`init_ap` 只收局部值。方案 B（延迟 ack）不采纳：ack 越早越贴近 C 的握手
语义（`ap_cpu_ready` 也是 AP 早期动作）。

**Q2（P0）`page_table_root_pa < 4GiB` 未被 v4 真正消解**
答：**外评对 v4 的否定成立，但结论比外评预期的更好——方案 1 的保证链已在源码里**。
核实链（v5 新增调查）：`arch/src/lib.rs:202` `BOOT_IDENTITY_MAP_END = 0x1_0000_0000`
→ `boot_dm_admissible_end()` = min(4GiB, VM DM window)（:215）→ boot-shim
`bootstrap_alloc_limit`（uefi_helpers.rs:266）→ `alloc_root_page()` 以 UEFI
`AllocateType::MaxAddress(上限)` 分配并 `.expect()` 强制（:276）；riscv 侧编译期断言
`BUMP_END <= boot_dm_admissible_end()`（opensbi_helpers.rs:375）。AP 32 位 PAE 阶段
第一次写 CR3 加载的正是这个 root（`arch_boot_impl(root_page)` → `paging.enable` →
`mov cr3`，paging.rs:431——与 C `bootstrap_pt->p_seg.p_cr3` 同源）。处置：方案 1 定稿
——不变量**不是新增**而是既有 boot 设计常量（恒等映射上界 4GiB）的推论；S-3b 加防御性
assert；方案 2（低内存 staging root）登记为 >4GiB 物理机的演进路径，其前提是抬高
`BOOT_IDENTITY_MAP_END`——属 boot 设计常量变更，非 AP bring-up 局部补丁。

**Q3（P1）S-4 装 IDT 与 NMI 的缺口**
答：**成立，时序再修正（撤销 v4 #5 的"S-4 装 base"）**。IF=0 不屏蔽 NMI（Intel 明文），
S-4 装 base 会把"NMI → vector 2 → handler 0"变成可达路径；arm/riscv 的空 handler 表
同理不提供保护。定稿三层模型：**S-4 不装向量；S-7 未安装 + 投递关闭（双层防护）；
S-8 base 与真 handler 一起安装并开投递**。x86 NMI 在 S-4~S-7 无 IDT 窗口 = 三重故障，
登记为显式残余风险（QEMU 启动期无 NMI 源；物理机硬化路径 = 前置 early-NMI stub，
记录不实施）。外评对 v4 "PIC 已 mask 只覆盖可屏蔽 IRQ、不管 NMI"的补充确认无误。

**Q4（P1）expected mask 命名统一**
答：**采纳外评偏好的方案**：`expected_cpu_mask`（BSP + 全员）/`expected_ap_mask`
（仅 AP）/`boot_ack_mask`/`online_mask` 四名固定；L2 断言
`boot_ack_mask == expected_cpu_mask`，L3 断言双等式
`boot_ack == expected_cpu && online == expected_cpu`；生产 wait_for_aps 仍为
`online == boot_ack`（C 忠实）。FAILED AP 使 expected 不成立 → 测试失败，生产语义不变。

**Q5（P1）ApBootstrap 对象边界**
答：**采纳并概念化**：`ApBootstrap` = `#[repr(C)]` ABI 布局类型（编译期形状）；
`ApEarlyEntryImage` = 运行期对象（机器码 + 一条可变 bootstrap 记录，BSP 拷贝到低地址、
串行复用）。配套规则入正文：**AP 在 boot_ack 后不得保留或使用指向 image 的任何引用**
（与 Q1 不变量同一条纪律的两面）。

**结论**：两 P0 一并闭环（Q1 方案 A 不变量、Q2 源码保证链 + 防御 assert）；三 P1 采纳
（时序第三版定稿、四名 mask、概念边界）；NMI 登记为显式残余风险。外评"查完即可冻结"
的判定条件已全部满足——**设计冻结**，S-0 开工。

---

## 14. GPT 外评·第六轮回应（2026-09-06，对话框直传意见，两 P0 级固件 ABI + 一 P0 内存序 + 三 P1）

**Q1（P0）RISC-V send_ipi ABI——确认为现存代码真 bug**
答：**外评正确，且比措辞问题严重**。源码核实：`riscv64/smp.rs:43/47` 实际常量
`SBI_LEGACY_EID = 0`、`SBI_LEGACY_SEND_IPI_FID = 3`——ratified SBI 规范的 legacy 扩展
（EID 0x00-0x08）每个 v0.1 函数有**独立 EID 且 FID 被忽略**：EID 0x0 = legacy
**SetTimer**、EID **0x4** 才是 Send IPI。`a7=0/a6=3` 的真实效果 = OpenSBI 按
SetTimer 处理，把 `1 << cpu` 写成计时值——IPI 未发出、BSP 自己 timer 被污染。外评的
判断"尚未走到 S-10 所以没暴露"正确。修复（S-3c 打包）：改用标准 SBI IPI 扩展
**EID 0x735049 / FID 0**（a0=hart_mask、a1=hart_mask_base，语义不变），legacy EID 0x4
为备选记录。

**Q2（P0/P1）AArch64 PSCI CPU_ON 位宽**
答：**核实为真**：`arm64/smp.rs:59` `PSCI_CPU_ON_32 = 0x8400_0003`（SMC32 变体），
TF-A 对 SMC32 调用把 x1-x3 截断为 uint32_t——entry_point ≥4GiB 即静默损坏（QEMU
默认 DRAM 布局 <4GiB 恰好掩盖）。原注释"SMC64 变体用于 smc conduit"混淆了两件正交的
事：conduit（hvc/smc）与调用位宽（FID bit 31）。修复（S-3c 打包）：换
**`PSCI_CPU_ON_AARCH64 = 0xC4000003`**（64 位参数）；现传 `context_id = 0`，截断风险
当前仅及 entry_point。SMC32→SMC64 后 <4GiB 限制对 entry/context 同时解除。

**Q3（P0）bootstrap publication 的真实 happens-before**
答：**采纳重写 §3.9 该行**。承认原文"BSP Release → AP Acquire"是 Rust 层表述，未落到
真实控制流——bootstrap 的第一次消费者是汇编。定稿模型（按架构）：
- **publisher 侧 fence 在 `SmpArch::boot_ap`**（BSP 填完字段、调固件之前）：x86
  `mfence`——**C 有此先例（arch_smp.c:130）而现存 Rust 代码缺失**（grep 零命中，S-3b
  补）；arm `dsb ishst`；riscv `fence w,w`。
- **consumer 侧**：AP 为刚上电/固件停放的冷缓存核（无陈旧缓存行）+ 固件自身交接屏障
  （TF-A CPU_ON 路径含 cache flush + dsb；OpenSBI hart_start 同构）+ BSP 发布已推到
  一致性点——首读无需附加 fence，依据写入文档。
- **Rust 层 Release/Acquire 只管掩码（Rust↔Rust 经 SmpState），不管 bootstrap 交接**——
  §3.9 表两行职责现已分明。

**Q4（P1）单 image ownership invariant**
答：采纳，正式化（§3.2）：`BSP owns image → populate → hand off → AP reads → AP
publishes boot_ack（= 最后一次读取）→ BSP regains ownership`。`boot_ack` = mailbox
ownership 交接完成信号，禁止误读为"AP 已开始跑"。

**Q5（P1）锁序 → 阶段性交接协议**
答：采纳定性（§3.4）：boot_lock × BKL 的双向嵌套**不是全局 lock-order 反转**，而是
**带阶段性交接约束的启动协议**——两条发生条件（BSP 先释放 boot_lock，AP 才能获得；
BSP 在 wait_for_APs 先 BKL_UNLOCK，才解除 AP 等待）使 bring-up 窗口内无死锁。S-5
锁序表必须写明发生条件，不得抽象成普通全局锁序。

**Q6（文档残留）trampoline 措辞**
答：全部清理——§3.1 表"mailbox 内嵌 trampoline blob"→"bootstrap 内嵌 early-entry
image 数据区"；§3.4 C 流程转述标注"C 用独立 trampoline.S，Rust 对应物为 early-entry
image"。全文剩余 trampoline 提及仅限历史记录与 C 源描述（copy_trampoline 等）。

**Q7（补充）expected_cpu_mask 非生产状态**
答：采纳，§3.4 加说明句——`expected_cpu_mask` 是测试健康性期望（证明本次测试启动了
全部预期 CPU），与生产的"少核 WARNING 降级"并存不矛盾。

**结论**：四问全部闭环，其中 **Q1/Q2 是现存代码真 bug**（都已潜伏至今未被暴露，恰好
证明外评"以代码为准"的价值），修复打包进 S-3c；Q3 修正了 §3.9 的模型归属；Q4-Q7 为
表述/协议精化。外评"查掉 4 个问题后基本赞成真正冻结"的条件满足——**设计冻结**。

---

## 15. GPT 外评·第七轮回应（2026-09-06，对话框直传意见，冻结前最后五问）

**Q1（P0）bootstrap consumer-side memory ordering**
答：**采纳方案 A**——AP early entry 在首次读 bootstrap 前执行显式 acquire fence
（arm `dsb ish` / riscv `fence rw,rw`；x86 TSO + 发布侧 mfence 已足够）。对外评论点
的精确回应与让步一并写入：**"冷缓存"单独确实不是 ordering 保证**；但 ①BSP 发布 fence
推到一致性点 ②固件时序交接 ③冷缓存（无陈旧行 → 首读 miss → 从一致性点取）三者合成
coherence 语义下的完整论证——consumer fence（④）作为一条指令的廉价保险使正确性**局部
化**于 AP 入口，并防未来 CPU_OFF/ON 复用路径破坏论证前提。§3.9 该行已按"publisher
fence → firmware start → consumer fence → bootstrap load"四段闭环重写。

**Q2（P0）S-4/S-7/S-8 向量与 SSIE 时序矛盾**
答：**成立，且调查中发现一个五处正文都没覆盖的更深问题**——若 S-8 排在 S-7 后，
S-5 起已在运行的 AP **永远执行不到 `lidt`/`load_ap`**（跨 CPU 通知机制本身需要 IDT，
发 IPI 即鸡生蛋）。定稿：**S-8 前移至 S-5 之前**（无 S-5/S-6/S-7 依赖，前移无成本；
C 顺序佐证：IDT 带 handler 在 AP 启动前装好）。五处统一结果：S-4 恢复
lidt/VBAR/stvec（指向 S-8 已填满的共享表，执行时即完整）；S-7 安全证明第三版 =
"向量已装 + handler 已满 + 投递关闭"（单层但坚实：意外中断可观察可诊断）；S-8 = 装
base + 填 handler + 开投递（不变）；riscv **SSIE 置位推迟到 S-10**（S-4 仅准备能力）。
阶梯随之重排：L3=timer-irq、L4=aps、L5=sched、L6=ipi、L7=shutdown；v5 的"NMI × 零
handler 窗口"整体消失。

**Q3（P1）x86 AP timer 的真实 C 语义**
答：**查实——C 的 AP 真实接收本地 LAPIC timer 中断**：`app_cpu_init_timer` →
`init_local_timer`（arch_clock.c:131-139）在有 LAPIC 时做 per-CPU
`tsc_per_ms/tsc_per_tick[cpu]` 校准 + `lapic_set_timer_one_shot(1000000/system_hz)`
（one-shot 由 handler 重装），失败即 panic（"cannot continue without any clock
source"）。**Rust 缺口为真**（现仅 PIT 全局）。处置：S-4 新增子项"AP LAPIC local
timer"（LVT 编程 + one-shot re-arm + TSC 校准，依赖 S-8 IDT 就位——前移后时序成立）；
若实施阻塞则 AP 暂无本地 tick 并登记限制（AP 上 quantum 递减停摆，L5 idle 测试不受
影响）。

**Q4（P1）ARM/RISC-V 是否复制执行**
答：**采纳外评推荐的设计**（v7 #4 定稿）：x86 独占"复制到低地址的
`ApEarlyEntryImage`"（SIPI 落点所必需）；**aarch64/riscv64 = 内核镜像内 arch-local
early stub + 静态内嵌 bootstrap 记录，不复制**——AP 经 PSCI/SBI 直接进入内核镜像物理
地址，镜像由固件装载且 ExitBootServices 已刷缓存，AP 冷 I-cache 首取即见已装载代码，
**D/I-cache maintenance 问题不存在**。三架构仍共享 `ap_early_entry(bootstrap_pa)`。
arm/riscv 的 bootstrap 记录 = 内核镜像内静态 `#[repr(C)]` 变量（PA 链接期可算），BSP
串行重写字段、同一握手纪律。

**Q5（P1）image/section 模型措辞**
答：采纳精确化（§3.2）：**single runtime image（拷贝单位）由两个 section 组成**
（`.ap_early_entry` 代码 + `.ap_early_entry_data` 数据），≠ single ELF section ≠
single output segment；跨 section 无符号引用，code/data 地址经编译期 offset 注入；
控制转移为**"设置 ABI 参数寄存器后 jmp/branch 进入 `ap_early_entry`"**（无 call，
与无栈一致——§3.2 闭环图已改）。boot_lock 删除残留句已清（v7 #7）。

**结论**：五问全部闭环；Q2 的调查额外发现并修复了"跨 CPU lidt 鸡生蛋"排序缺陷（S-8
前移）；Q3 查实 AP LAPIC timer 缺口并排期（S-4 子项）。外评的冻结门链条
（firmware ABI → publication → AP first load → MMU → Rust entry → boot_ack →
init_ap → online → scheduler）各环节均已闭合——**设计冻结，S-0 开工**。

---

## 16. GPT 外评·第八轮回应（2026-09-06，对话框直传意见；一 P0 + 四 P1，冻结前最后一轮）

**Q1（P0）S-4 ↔ S-8 循环依赖**
答：**成立——v7 的排序（S-4→S-8）是步骤依赖倒置**：S-4 的 `lidt` 引用"S-8 已填满的
表"，而 S-8 排在其后（运行期靠 S-8 先于 S-5 巧合成立，作为步骤定义不成立；且 S-4 自己
的 L2 扩展验证时表仍为空）。**采纳外评方案，S-8 重定义为"BSP 公共陷阱基建"并永久固定
在 S-4 之前**：S-8 = asm stub 本体 + 共享 handler 表填满 + BSP load（纯 BSP 工作无
S-3~S-7 依赖）；S-4 = per-CPU attach（GDT/TSS/GS_BASE/MSR/lidt/local timer——把
S-8 的公共资产接到每个 AP）。**阶段不变量（取代 v5 的 NMI 残余风险模型）**：S-8 之后
任何架构都不存在空 vector/空 handler 状态，S-8~S-10 只差投递开关；x86 NMI 风险随之
整体消失（BSP 自 S-8、AP 自 S-4 均持有效 IDT）。顺序定稿：
S-3a→S-3b→S-3c→S-3d→**S-8**→S-4→S-5→S-6→S-7→S-9→S-10→S-11→S-12→S-13。

**Q2（P1）§2.2 riscv PLIC 残留**
答：采纳——"PLIC context 使能"移出 init_ap，归 S-8/S-10 的外部 IRQ 路径（§2.2 已改）。

**Q3（P1）§8 风险表两条旧模型残留**
答：均清——"AP 无 IDT 时收到 IPI"行改写为阶段不变量表述（S-8 起无空 vector 状态，
IPI 投递 S-10 开启）；"x86 NMI 无 IDT 窗口"行**删除**（窗口已不存在：BSP 自 S-8、AP
自 S-4 均持有效 IDT，NMI 落真 handler 可观察）。

**Q4（P1）ARM/RISC-V bootstrap writable static**
答：采纳补句（§3.2）：bootstrap 记录是 **writable static storage**——落可写 data 段
（`static mut`/`SyncUnsafeCell` 语义，不得 `static X: ApBootstrap = ...` 落 rodata）；
AP 经固件 context 或 PC 相对取 PA，BSP 在启动临界区内串行更新。

**Q5（P1）image adjacency 硬验收**
答：采纳——S-3a 验收增加：`[image_start, image_end)` 内 `.ap_early_entry` 紧邻
`.ap_early_entry_data`、中间不得插入其它可分配 section；记录 image_start/end/size/
section ordering；拷贝单位是区间而非"两 section 各自存在"。与"readelf -r 只是必要
条件"同一防自验原则。

**结论**：一 P0（依赖倒置）+ 四 P1 全部闭环；外评 20 项判定表全 ✅（NMI 项由新阶段
模型消除）。**设计冻结（外评明确"结构上已无值得继续设计迭代之处"）**——后续问题进入
S-x 实施阶段的代码级 review，不再修改 SMP 总体设计。S-0 开工。

---

## 17. GPT 外评·第九轮（2026-09-06，冻结判定轮）：P0 = 0，P1 = 0，三个文档 nit 即清

外评正式给出冻结判定：整体进入"实施时发现具体代码问题、局部回退修正"阶段。三个 nit：
1. **L2 解锁表述**——L2 本体（S-3d 时 init_ap 未实现）只验证 early entry + boot_ack +
   park，不验证 S-4；S-4 用同一测试内核扩展 per-CPU/online 断言（阶梯行已改）。
2. **RISC-V PLIC-IPI 术语边界**——IPI = SBI send_ipi → CLINT MSIP → software interrupt，
   全程不经 PLIC；PLIC 只服务外部 IRQ（§3.3 已收紧为最终边界声明）。
3. **"consumer-side acquire fence" 更名 "consumer-side ordering barrier"**——本交换不
   构成标准语言内存模型的 release/acquire 同步对（第一消费者是裸汇编、交换的是普通内存
   数据，非 Rust atomic 同步对象）；barrier 的作用是排序与完成，非同步配对。指令不变
   （arm `dsb ish` / riscv `fence rw,rw`），§3.9 已更名。

**FROZEN — implementation starts at S-0。**

---

## 18. S-0 完成记录（2026-09-06）——三架构基线矩阵

| 维度 | x86_64 | aarch64 | riscv64 |
|---|---|---|---|
| hosted 测试 | `cargo test -p minix-kernel --lib` → **691 passed**（arch 207 passed） | 同左（共享 hosted 编译） | 同左 |
| production-target 构建 | 7/7 测试内核 `x86_64-unknown-uefi` release 全过 | `hello-boot-aarch64` 过（**修复后**） | `hello-boot-riscv64` 过（**修复后**） |
| QEMU smoke | **7/7 PASS**（hello-boot / memmap / paging-enable / kernel-map / higher-half / protection / proc-init） | hello-boot **PASS**（UEFI/AAVMF 链） | hello-boot **PASS**（OpenSBI `-bios default` 链） |

**基线额外发现并修复（第 5 个真 bug，同为"hosted CI 盲区"产物）**：
`arm64/trap_return.rs:115-116` 与 `riscv64/trap_return.rs:113-114` 的 `TrapReturnArch`
asm 使用**具名操作数绑定显式寄存器**（`frame = in("x0") …`）——Rust 规则禁止（E: explicit
register arguments cannot have names），aarch64/riscv64 的 production-target 构建从未编译
通过过。修复 = 去操作数名（模板本就使用字面寄存器 x0/x1、a0/a1，无占位符引用）。
回归：`cargo test -p minix-arch` 207 passed、`-p minix-kernel --lib` 691 passed 无回归；
aarch64/riscv64 QEMU smoke PASS 证明修复后 trap-return 路径（`jump_to_kmain` 下游）真实可用。
x86_64 全部 7 测试在我修复 import 后（本 session 早前验证）已经过同一 QEMU 门。

**遗留观察（记录，不阻塞）**：`run_all.sh` 的 riscv64 分支对非 EFI 内核走 OpenSBI
fallback 正常；aarch64 仅 6 个测试包（无 proc-init 变体）——与脚本既有清单一致。


---

## 19. S-1 完成记录（2026-09-07）——D-38④ 勘误：两个记录都错了

**核实结论（以代码为准）**：resume 路径的 BKL **已覆盖**，但原两份记录各自错了一半。

1. **todo.md Edge 表错在标 DEFERRED**：D-38④ 的诉求（resume 需 BKL）实际已满足——
   VmSuspend 在 `kernel_call_finish` 释放 BKL（syscall.rs:2705，含注释"kernel_call_resume()
   will re-acquire BKL when VM replies"）；生产 resume 路径（proc_table.rs:882 调
   `vm::kernel_call_resume` 简单版）在调度循环锁内运行——`switch_to_user` 入口契约注释
   （lib.rs:2921 "BKL is held on entry"）+ Stage 3 明注（:2979 "Runs under the BKL"）。
2. **doc 16 §4.13 错在归属混淆**：原记录"✅ 已接入（syscall.rs:2622）……生产调用方
   proc_table.rs:859"把两个同名函数混为一谈——proc_table.rs:882 实际调
   `vm::kernel_call_resume`（简单版，无自身加锁逻辑）；带 BKL 重入的
   `syscall::kernel_call_resume`（:2747，重入 dispatch :493 → finish :2730）**无生产
   调用方**——这是 doc 10 §4.2 记录的 Rust 借用拆分偏差（process_misc_flags 持
   `&mut self` 无法再传 `self` 给完整重派发版），属于 resume 语义维度，**不是 BKL 缺口**，
   不应由 D-38④ 跟踪。
3. **行号全面刷新**：dispatch :493/:655、finish :2705/:2730（原记 411/566/2580/2605/2622
   均已漂移——行号锚点必须随勘误刷新，否则下次核实还会踩坑）。

**修正落点**：todo.md Edge 表 D-38④ 行转 ✅、DEFERRED 汇总行划掉 ④、§7.1 D-38 行补 ④
结论；doc 16 §4.13 三行（入口/完成行号刷新 + resume 行重写为生产/完整双版本说明）。
D-38① 维持 DEFERRED（SMP 异常路径，smp_todo S-9）。


---

## 20. S-2 完成记录（2026-09-07）——拓扑发现回归钉（L1）

**交付**：三个测试内核 `test-smp-topo{,-aarch64,-riscv64}` + run_all.sh/run_qemu.sh 接入
（SKIP 单列计数，`⏭️` 标记，不与 FAIL 混淆）。

**结果**：

| 架构 | 来源链 | 结果 | 发现 |
|---|---|---|---|
| x86_64 | UEFI config table → RSDP → `AcpiDesc::parse`（MADT） | **PASS** | nr_cpus=4、APIC ID {0,1,2,3}、BSP=0——QEMU 默认拓扑与解析器预期一致（仅打印观察，未入契约） |
| riscv64 | OpenSBI a1 → DTB → `DeviceTreeDesc::parse` | **PASS** | nr_cpus=4、hart {0,1,2,3}、BSP=0——DTB 解析器真固件下钉住 |
| aarch64 | config table 无 FDT；acpi 模块 x86_64 门控；fw_cfg MMIO UEFI 阶段未映射 | **SKIP → S-2b** | 三条拓扑来源全断（详见测试头注释三个 blocker）；DTB 解析器本身由 riscv64 覆盖（同一份 device_tree.rs） |

**设计决策（cmd-08 候选对比）**：解析器直钉（选）vs 进内核集成钉（qemu_test kmain，重、
失败面大）vs 纯 mock（不验真固件，否）。解析器是 D-36 上半的全部内容；`init_from_kinfo`
的集成消费由后续 L4+（AP 启动经 `platform_desc()`）覆盖。

**发现的实施缺口（S-2b 登记进 §5）**：aarch64 生产链同病——AAVMF 下 `init_from_kinfo`
发现不到任何 source。修复方向两个候选（RSDP arm 扩 aarch64 + GICC 解析 / DTB 直通交接），
S-2b 开工时按 cmd-08 对比定夺。**这不是架构变更**（FROZEN 规则内：实施发现的局部缺口）。

**测试自审（cmd-23 五维）**：完备（3 断言 + 观察）✓；自身正确（断言 vs 打印严格分离，
QEMU 拓扑值不进契约）✓；冗余（无——三架构各测各的来源链）✓；无效（aarch64 SKIP 带三
blocker 证据 + FDT 复现 tripwire，非空转）✓；虚构（测试名/断言/串口输出一一对应）✓。

---

## 20.1 S-2b 完成记录（2026-09-07）——aarch64 发现链走 ACPI（非 DTB）

**实验裁决（测试内核字节级诊断）**：AAVMF（Ubuntu 2024.02）config table 8 表项中，FDT 的两个候选 GUID（boot-shim 的 legacy 串与 UEFI 规范 `EFI_DTB_TABLE_GUID` b1b621d5-…）**均不存在**，ACPI 2.0 RSDP GUID **存在**——候选 (a)（RSDP arm 扩 aarch64）胜出，候选 (b)（DTB 直通）前提不成立。

**真缺口三层（非登记时以为的单点）**：boot-shim aarch64 ACPI fallback 早已存在；真缺口 = kernel 侧 ①kind.rs RSDP 臂门、②global.rs/lib.rs 的 Acpi 枚举与模块门、③acpi.rs 两处解析偏移真 bug（GICD base@+8 非 +12；GICC 的 MPIDR@+56 在 QEMU GICv2 恒 0 → hw_id 改用 Processor UID@+8，与 DTB MPIDR Aff0 天然一致）。另发现并修复 B1/A1 连带缺口：`bkl_is_locked` 的 `cfg(any(test, debug_assertions))` 门在 release 测试内核下使 `debug_assert!` 载荷无法命名该函数（`debug_assert!` 逐字检查不因 release 跳过）——解除门控。

**验收**：`test-smp-topo-aarch64` SKIP → **PASS**（nr_cpus=4、hw_id {0,1,2,3} 互异、BSP=0）。MADT 原始记录字节级诊断（GICD 24 字节 hex dump）留档于本节与 16-smp §5.1 行。

**测试自审（cmd-23 五维）**：断言三连（来源非空 / nr_cpus==4 / hw_id 互异 + BSP∈集合）钉 D-36 上半的 aarch64 全链（boot-shim 扫描 → parse_by_kind → AcpiDesc::parse → CpuTopology），与 x86/riscv 变体各测各来源链、无冗余 ✓。

---

## 5.1 S-3a spike 报告（2026-09-07）——global_asm! + lld + UEFI 链路成立

**载体**：`minix-arch::x86_64::ap_early_entry`（`.ap_early_entry` AX 段 + `.ap_early_entry_data` AW 段 + `ap_early_entry_start`/`ap_early_entry`/`ap_early_entry_data_start`/`ap_early_entry_end` 边界符号）+ `test-smp-spike` 测试内核（x86_64，已入 run_all.sh）。

**PE/COFF 验收（objdump pei-x86-64，v4 要求的 PE 感知检查）**：

| 项 | 结果 |
|---|---|
| 段表 | `.ap_early_entry` 0x24=36B CODE @0x140005000；`.ap_early_entry_data` 0x40=64B DATA（可写）@0x140006000 |
| 段名 | COFF 8 字符截断，两者均显示 `.ap_earl`（字符串表区分） |
| 反汇编 | 5 条指令与设计逐条一致（rcx 捕获 / movabs ACK / rip-rel 写 / rip-rel 读 / 穿参回写 + ret） |
| base relocation | `.reloc` 共 0x20 字节，**零条目落在 0x140005000/0x140006000 两页**——blob 零 base reloc |
| 拷贝执行 | 拷贝 [start,end) 至堆缓冲后调偏移 0：三证明 PASS（ACK 写入 = 执行 + rip-rel 写；echo = 寄存器绝对；读回 = rip-rel 读） |

**工具链现实（spike 的核心产出——四条，全为实证）**：
1. **x86_64-unknown-uefi = Microsoft x64 ABI**：`extern "C"` 首参在 RCX。SysV 式 RDI 读参被 #GP 实证否决（非 canonical 地址写即 GP）——S-3b 梯子定义自己的入口寄存器时不得假设 SysV。
2. **64 位立即数不可直写内存**：`mov qword ptr [mem], imm64` 非法——必须 `movabs reg` + 寄存器存储。
3. **lld-link 将新段放下一 4KiB 页边界**：`.ap_early_entry_data` 在 +0x1000（blob 4160 字节 = 代码 36 + 页隙 + 数据 64）。[start,end) 自包含性不受影响（隙内无外来 section），代价是每 AP 多占一页低内存——记录不修。
4. **COFF 段名 8 字符截断**（`.ap_earl`）——段身份靠字符串表，无功能影响。

**结论**：方案 A（`global_asm!`）成立，**无需回退 .S**。内存序契约（§3.9 boot_ack Release 语义）已随 `ApBootstrap` 文档落入 ABI 层，S-3b/S-3d 按此实现。
