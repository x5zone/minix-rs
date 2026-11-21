> **创建**: `2026-10-08`
> **本文是什么**: 进程出生链（新进程被首次调度到用户态时，体系结构为其建立的一切初始状态）的三架构 × 六维对账表。它是裁决 `PD-09`（采乙：按六个维度逐架构对账，静态读码可完成大半，对不上的立 `P-*` 条目）的实现产物，同时作为 `PD-01`（把 x86_64 补进目标③ 测试套件装配面）装配 x86 出生腿前的必读读物。
> **判据分级**: 本表全部结论来自读码（`L2`）。运行时读数（真机门）另注。凡标 `[待验证]` 的，静态读码无法定案，须真机确认。

# 出生链三架构对账

## 为什么是这六个维度

一个进程「出生」＝内核第一次把它恢复到用户态执行。此刻之前它没有任何寄存器状态；此刻之后直到它自己改写寄存器为止，它跑的每一步都依赖出生时种下的东西。因此出生链的正确性可以拆成六个互不重叠的问题：

1. 线程指针（每个线程私有的寻址基址）在出生时被种成什么；
2. 用户栈指针是否满足该体系结构调用约定的对齐要求；
3. 内核递给新生程序的那张参数描述表（`ps_strings`）里，槽位的字节宽度；
4. 程序的 `main` 返回后，退出码经哪条路回报给进程管理；
5. 用户态「第一条指令」看到的全部寄存器初始值（程序计数器、栈指针、承载 `ps_strings` 的那个寄存器、初始处理器状态字）；
6. 陷入内核时哪些寄存器必须进帧、哪些跨陷入保持、哪些是随手可毁的暂存器——这决定了第 5 维种下的寄存器能不能活到第一条指令。

前五个维度描述「出生时种什么」，第六维描述「种下的东西能不能被恢复机制如实带回」；两者必须自洽，否则第 5 维设的某个寄存器会在第一次陷入后凭空丢失。

## 权威位置（本文只列结论，逐维取码点在此）

| 维度 | Rust 真源 | C 真源 |
|---|---|---|
| 初始上下文构造 | `os/arch/src/{x86_64,arm64,riscv64}/boot.rs:fn build_cpu_context` | `minix3/minix/kernel/proto.h:arch_proc_reset` |
| 上下文落到陷入帧 | `os/arch/src/*/boot.rs:fn apply_to_trap_frame` | `minix3/minix/kernel/arch/i386/mpx.S` 恢复半（`switch_to_user`） |
| 用户态入口桩 | `os/libs/minix-rt/src/crt0.rs:fn _start`（各 `target_arch` 臂） | `minix3/lib/csu/arch/*/crt0.S:__start` |
| 参数描述表 | `os/libs/minix-rt/src/crt0.rs:struct PsStringsRaw` | `minix3/sys/sys/exec.h` |

## 逐维对账

### 维度一：初始线程指针

| 架构 | 出生时设什么 | 载体机制 | 结论 |
|---|---|---|---|
| x86_64 | 段选择子 `fs`/`gs` 置 `USER_DS_SELECTOR`；GS 基址（真正的线程指针）不在出生设 | 基址经 `os/arch/src/x86_64/trap_stub.rs` 对 `MSR_IA32_GS_BASE`/`MSR_IA32_KERNEL_GS_BASE` 的写维护，非出生路径 | 出生不种非零线程指针 |
| aarch64 | 上下文中无线程指针字段 | `os/arch/src/arm64/trap_return.rs` 读 `tpidr_el1` 是每核内核态基址，非每进程用户线程指针 | 出生不种非零线程指针 |
| riscv64 | `tp`（即 `x4`）落在通用寄存器保存区，出生随通用区清零 | `os/arch/src/riscv64/boot.rs` 通用区零初始化 | 出生 `tp`＝0 |

三架构在「出生不预设线程指针」这一点语义一致。差异在寻址手段（x86 段基址写模型特定寄存器、aarch64 系统寄存器、riscv 通用寄存器 `tp`），属同一层语义下的不同硬件承载，OS 语义层不可见。运行时 `main`/`rt_birth` 不依赖架构线程指针（线程局部存储由库层在 `main` 之后自行建立），故出生不种不影响当前用户镜像。`[待验证]` 一旦有用户态程序在 `_start`→`main` 之前依赖架构线程指针，需另立条目补出生设置臂。

### 维度二：初始栈对齐

| 架构 | C 的入口桩 | Rust 的入口桩 | 是否一致 |
|---|---|---|---|
| x86_64 | `minix3/lib/csu/arch/x86_64/crt0.S` `andq $~15,%rsp`（16 字节） | `crt0.rs` x86 臂 `and rsp, -16` | 一致，且满足 System V |
| aarch64 | `minix3/lib/csu/arch/aarch64/crt0.S` 裸 `b ___start`，不对齐 | aarch64 臂裸 `bl rt_birth`，不对齐 | Rust 忠实于 C |
| riscv64 | `minix3/lib/csu/arch/riscv/crt0.S` 裸 `j ___start`，不对齐 | riscv64 臂裸 `call rt_birth`，不对齐 | Rust 忠实于 C |

这条推翻了「aarch64/riscv64 缺对齐是重写对上游的偏离」的读法：三架构的 Rust 入口桩与 Minix3 各自的 `crt0.S` 逐一对齐（x86 都对齐、aarch64/riscv64 都不对齐）。真正的残余问题是「是否应偏离 Minix3 以满足各架构调用规范」——AAPCS64 要求 `sp` 在任意时刻 16 字节对齐、RISC-V psABI 要求在调用点 16 字节对齐，而 Minix3 的 aarch64/riscv 端口本身未强制。因此这是「与上游共有的规范完备性问题」，不是重写引入的语义分歧。

处置：`SD-21`/`P-A64RV-03` 据本表重新定性——若要闭合，属 `[ARCH]` 级偏离决定（要么在 aarch64/riscv64 入口桩补对齐、偏离 Minix3；要么由装载器保证交付的初始 `sp` 已 16 字节对齐，从而入口桩无需再动，且这才是「谁负责对齐」的正解归属）。它不是 P0-fact（对 C 无分歧）。本表不自行偏离 C，仅登记该定性，交裁决。

### 维度三：参数描述表的槽位字节宽度

`ps_strings` 描述表的字段偏移与 C 逐字对齐（`crt0.rs` 载 `ps_argvstr`@0、`ps_nargvstr`@8、`ps_envstr`@16、`ps_nenvstr`@24，对齐 `minix3/sys/sys/exec.h:struct ps_strings`；argv/envp 指针槽按 8 字节寻址——`crt0.rs:fn string_at` 以 `(list + index*8)` 取 `u64`）。计数为 32 位字段、槽位指针为 64 位——这与 64 位下 `size_t`/指针宽度一致。`SD-17`（曾疑「装载腿 4 字节 vs 执行腿 8 字节」）已闭合，三架构共用同一份 `crt0.rs` 解析逻辑，无按架构分叉。结论：语义三架构一致、与 C 一致。

### 维度四：退出回报

`rt_birth` 末段（`crt0.rs:fn rt_birth` 第 6 阶段）统一经 `minix_sys::exit(exit_code)` 把 `main` 的 `i32` 返回值交给进程管理，三架构同一路径。入口桩调用 `rt_birth` 之后的「永不返回」兜底各用本架构的非法指令陷入：x86_64 `ud2`、riscv64 `unimp`、aarch64 `brk #0`。语义同一（出生函数不应返回，返回即致命），承载各异。`crt0.rs` 模块文档已钉「`main` 必须返回 `i32`」的消费者契约：符号解析不校验签名，若用户镜像的 `main` 返回单元类型，链接照过、退出码是寄存器残值——这是三架构共有的契约风险，非架构差异。结论：语义一致。

### 维度五：第一条用户指令看到的寄存器

| 要素 | x86_64 | aarch64 | riscv64 | C/ABI 依据 |
|---|---|---|---|---|
| 程序计数器 | `RIP ← entry.pc` | `ELR_EL1 ← entry.pc` | `sepc ← entry.pc` | 各架构恢复约定 |
| 栈指针 | `RSP ← entry.sp` | `SP_EL0 ← entry.sp` | `x2 ← entry.sp` | 同 |
| 承载 `ps_strings` | `RBX` | `X0` | `X10`(a0) | x86-64 由 `crt0` 把 `RBX` 转 `RDI`；aarch64/riscv64 直接是首参寄存器 |
| 其余通用寄存器 | 清零 | 清零 | 清零 | 出生不给垃圾 |
| 初始状态字 | `RFLAGS ← INIT_PSW` | `SPSR_EL1 ← INIT_PSR` | `sstatus ← INIT_USER_SSTATUS`（`FS=Off`） | 决定首条指令的处理器模式与浮点门控 |
| 浮点策略 | 懒初始化（首条 FP 指令经 #NM 陷阱取得归属） | 懒（FPEN 门控 + EC=0x07 陷阱） | 懒（`FS=Off` → 首条 FP 指令非法码陷阱轮转） | 三架构 `FpuArch` 各持自家状态形态，OS 层只见「该进程有浮点上下文」 |

`ps_strings` 的承载寄存器按架构 ABI 不同（`RBX`/`X0`/`a0`），这是 `EntrySpec`（`os/arch/src/arch/boot.rs:struct EntrySpec` 文档）明载的合法差异，不是不一致。关键不变式——「程序计数器、栈指针、参数表地址、其余清零、状态字进用户模式、浮点懒开」——三架构语义齐平。

### 维度六：陷入保存/恢复契约

| 架构 | 通用保存集 | 单独存放（不在通用集内） | 出生上下文→帧 的落点（`apply_to_trap_frame`） |
|---|---|---|---|
| x86_64 | 14 个：`RAX,RCX,RDX,RSI,RDI,RBP,R8–R15` | `RBX`、`RIP/RSP/RFLAGS`、`CS/DS/SS/ES/FS/GS`（后五个为 CPU 压栈帧字段） | 写 `rflags/cs/ss/rip/rsp` 入帧；`RBX`/`ES/FS/GS` 随 `CpuContext` 旁挂由恢复臂带回 |
| aarch64 | 30 个：`X1–X30` | `X0`（参数表）、`SP_EL0`、`ELR_EL1`、`SPSR_EL1` | 写 `spsr_el1/elr_el1/sp_el0/regs[0]` |
| riscv64 | 30 个：`x1,x3–x9,x11–x31` | `x2`(sp)、`x10`(a0)、`sepc`、`sstatus` | 写 `sstatus/sepc/regs[2]/regs[10]` |

自洽性检查（第 5 维种的能否活到第一条指令）：三架构的 `build_cpu_context` 所写的每一个要素，都能在 `apply_to_trap_frame` 找到对应的帧落点，且该落点正是本架构陷入臂保存/恢复的同一槽位——x86_64 的 `RBX`（第 5 维参数表载体）不在 CPU 压栈帧里，但由 `CpuContext` 旁挂、恢复路径带回；riscv64 把 `sp`/`a0` 直接写进帧的 `regs[2]`/`regs[10]`，与 `Riscv64ExceptionFrame::set_return_value` 同一索引约定。x86_64 曾把 IPC 状态寄存器暂存于 `RBX`，后迁移至调用者保存的 `R10`（`SD-14` 已闭，见 `os/arch/src/x86_64/trap_stub.rs:fn ipc_status_register`），迁移后 `RBX` 归还参数表用途，与第 5 维一致。结论：出生种寄存器集 ⊆ 陷入恢复带回集，三架构自洽、语义齐平。

## 对不上的地方（本表的产出）

1. **维度二**：aarch64/riscv64 初始栈未按各自调用规范强制 16 字节对齐——但与 Minix3 逐架构一致，故属「与上游共有的规范完备性」问题而非重写分歧。定性并挂回 `SD-21`/`P-A64RV-03`，闭合属 `[ARCH]` 偏离决定或装载器负责对齐，二者需其一被裁决。
2. **维度一**：`[待验证]` 出生不预设线程指针对当前用户镜像无碍；若未来出现 `_start` 前即依赖架构线程指针的用户态，需立条补臂。
3. **维度五/六**：`ps_strings` 承载寄存器与帧保存集的跨架构差异，经核对全部是合法 ABI 差异（各有 `EntrySpec`/`CpuContext`/`trap_stub` 锚点支撑），非缺陷。

## 喂给 PD-01

`PD-01` 把 x86_64 补进目标③ 装配白名单（`os/xtask/src/image.rs:const ATF_BOOT_LEG_READY`）前，须与本表「维度五（x86_64 用 `RBX` 承载 `ps_strings`，非 `RDI`——`RDI` 是入口桩转的）」与「维度六（x86_64 的 `RBX`/段寄存器不在 CPU 压栈帧内、靠 `CpuContext` 旁挂恢复）」两列逐项互检：x86 出生腿与 aarch64/riscv64 已在维度五/六齐平（语义同一、承载各异），装配面可静态铺，终态判据（ATF 用例真机跑绿）另立执行。本表即为 PD-09 钉的「装配前必须有」的出生链对账读物。
