# 01-stage-kernel TODO 总清单（V13，2026-09-09 压缩归档版）

> 本文件是 01-stage-kernel stage 的 TODO 权威清单。**2026-09-09 压缩归档**：已完成条目
> 压缩为一行结论（条目号 + 一句话 + 日期），完整细节可从 git 历史找回——压缩前最后
> 全量版为 commit `893386cd8`（2897 行）；更早的原始 18 节历史见 `97df4e58d^`。
> 压缩只删"已闭环的论证过程"，不删任何 open 条目、不删防重查备忘（§21.3 / §7.6 / 非缺口表）。
>
> 历史结构（压缩后去向）：
> - §0.1 处理状态总览 → §0.1（压缩为批次表）
> - §0 审查基线 / §0.5 学习 backlog / §0.6 硬件抽象化 backlog → 原样保留
> - §1-§6 架构建议（A/B/C/D/E/F/G/M/R 系）→ §1 总览 + §1.1 一行解决记录
> - §7 未完成项汇总（D/W/I/T 系）→ §7（open 保留全文，resolved 压缩）
> - §H 历史归档（968 行原始 18 节 + 清空归档）→ 删，见 git `97df4e58d^`；转移清单结论并入 §7.7
> - §18.5 / §19 / §20（GPT 评论分析）→ §8（结论压缩，含 P0-1 未修核实）
> - Step 1.0f 漂移表 → 并入 §23（V13 复核仍未修）
> - OQ-15-1 / OQ-D49-1 / D19-1 → §9
> - Edge Items 表 → §10
> - §21 V12 轮 → §11（结果表 + 非缺口备忘保留）
> - §22 收敛执行计划 → §12（Phase 3 SMP 与 open 项保留，done 压缩）
> - §23 V13 轮（本轮新增，扫描进行中）

## 0.1 处理状态总览（压缩为批次表；行内日期 = 修复落地日）

| 批次 | 处理 | 一行结论 |
|------|------|---------|
| 2026-08-14 修复会话 | 5 项 | F1/F2（pt_alloc 论证模型 + register 同步）、D3（命名错误类型 6/7）、D-51（PrivUpdateRequest 字段序冻结）、D-53（cpu_identify + GET_CPUINFO 全记录） |
| 2026-09-05 三批 | 5 项 | D-6（dispatch_exit 全 cause_sig）+ D-11（cause_sig 致命 SELF 路径）+ D-57（proc_stacktrace 接入）+ D-13（sig_delay_done 双路接线）+ D-43/D-45（SC_TRACE→SIGTRAP、Segfault→SIGSEGV） |
| 2026-09-05 第四/五批 | 2 项 | D-49（set_sendto_bit 全链 + P0 顺带 p_priv 回链）+ D-58（boot 模板 fill_sendto_mask，自位恒清） |
| 2026-09-06 第六批 | 12 项 | D-8 kernel_call wrapper、D-14 no-op、D-15 CLOCK 通知、D-20 vm_enqueue_and_notify_vm、D-24/D-25 no-op、D-33 read_tsc 熵采样 [ARCH]、D-34 no-op、D-35 远程 IPI、D-44 vm_suspend(DeliverMsg)、D-46 软件半环、D-52 sched_proc 完全对齐 C、D-17/D-16/D-18（IPC 过滤三件套） |
| 2026-08-14 二次核实 | 18 项 | D-1~D-5/D-7/D-10/D-12/D-19/D-21/D-22/D-23/D-26~D-29/D-31/D-32 实为已实现（原记录过时，grep 证据在案） |
| 2026-09-08 Phase 4/5 | 8 项 | E1（Arch 关联类型聚合）、G1（ProcNr 上移 minix-types）、R1（核实闭合 CTOS）、C1（4 dispatcher 拆分）、M1（gen-test-kernel.sh）、U-1（alarm_timer 保全）、U-2（swap_memreq 真实现）、I-7/I-9/I-10（评估闭合维持现状）+ SYS_PADCONF（奇偶闭合） |
| 2026-09-08 Phase 6 | 7 项 | T-1（10/10）、T-6（10 测试）、T-7（5/5）、T-12（5/5 + dispatch_clear 幂等修正）、T-9（3 测试）、T-11（runqueues_ok 集成）、T-5（QEMU CI workflow） |
| 2026-09-07~08 §22 Phase 1/2 | 12 项 | V12-A1 trap_style（含 MINIX3 BUG 修复）、V12-B1 panic 审计、V12-A2/A3/A4、V12-B2（IoBatchBuf 对齐 UB 修复）、V12-B3（Message 72B 实测）、V12-B4、A1（assume_held + 24 处迁移）、A2（globals.rs）、B1（transfer API）、B-X 第一批、S-2b/S-3a/S-3b/S-3c |
| 2026-09-09 D1/D2 + Phase 7 | 4 项 | D1/D2（Errno newtype + ToErrno，架构建议清零）、I-14（B 层完成）、I-3（核实闭合） |
| 📌 保持 DEFERRED（依赖仍成立） | — | **I-1**（kernel ELF 构建无引导路径）、**I-5**（ACPI RSDP 无消费方）、**I-6**（bill_ptr+主循环，等 S-7/S-10）、**T-10**（U-Boot 工具链）、**profiling deferred 体**（等 S-8/S-9）；Edge 12 项全部阻塞于 SMP bring-up（§10） |

## 0. 审查基线 — 已确认的良好架构（无需改动）

| 维度 | 现状 | 评价 |
|------|------|------|
| 分层与依赖方向 | `kernel → arch → plat/platform → types/boot/elf`，依赖单向无环 | ✅ 干净 |
| no_std | 全链 `#![no_std]`（boot-shim 部分 cfg 后 std），无 std 泄漏 | ✅ |
| 全局状态安全 | `SyncUnsafeCell<T>` + sealed trait `BklProtected`（`bkl_protected_impls!` 宏）+ `BklSection` witness，声明集中于 `os/kernel/src/globals.rs` | ✅ typestate 水准（R-01/R-03/A2） |
| syscall 分派 | `enum Syscall` + `try_from` + match；`CurrentArchSyscall` ZST trait 化 | ✅ 优于 C call_vec[] |
| 硬件抽象 | 16+ 细粒度 trait（Paging/Protection/TrapEntry/Exception/Clock/Fpu/Smp/…）+ 每 arch Mock + `Arch` 关联类型聚合（E1） | ✅ |
| 跨空间拷贝 | `cross_space_copy<D: DirectMapArch>` 泛型核心 + `cross_space.rs` vmcheck 封装 | ✅ |
| 依赖注入 | syscall dispatch 显式传表；kernel_call_finish 已去全局化（A1） | ✅ 混合模式 |
| boot 契约 | `minix-boot::KernelInfo`（含 `validate()`）为 kernel/boot-shim 唯一契约 | ✅ |
| 占位符 | `todo!`/`unimplemented!` = 0；`unsafe impl Sync/Send` 全库 4 处真实实现均有论证 | ✅ 基线 |
| 平台发现 | `PlatformContext` write-once（AssumeSyncCell）+ `&'static dyn PlatformDesc` | ✅ |
| 错误通道 | `Errno(i32)` newtype + `ToErrno` trait 于 minix-types 单一权威（D1/D2，2026-09-09） | ✅ |

## 0.5 学习 / 概念理解 backlog（与代码改动分离）

### L1. `happens-before` 与内存序（C `volatile` 背景 → Rust 内存模型） [学习]

**盲点自述**（2026-08-31）：对 `Acquire`/`Release`/`Relaxed` 的形式化含义理解停留在现象层；
`06-proc-init-boot-proc.md` §3.3 的内存序论证只给了现象未给"如果去 BKL 会怎样"的完整推理。

**待深入**：① happens-before 的形式化定义（相对 C volatile 的差异）；② 单字段防撕裂
（所有 Ordering 都有）vs 跨字段次序（只有 Acquire/Release 建立）两层保证；③ Acquire 防的是
"load 之后的访问被重排到 load 之前"，不是 load 本身被重排；④ 读侧 Acquire 必须配写侧
Release 才形成 happens-before 链。

**递进路径**：06 §3.3"关键反推"段（已写）→ 16-smp BklSection 实现（`xchg` + 屏障）对照 →
11-scheduling-primitives 写作时推演"去 BKL 后 `p_nextready` 的 Relaxed 是否要升 Acquire"。

**触发条件**：启动 doc 11 或 doc 16 review；启动去 BKL 重构。是 backlog 不是 bug——
当前论证对已修目标足够，强行深挖阻塞主线。

## 0.6 硬件抽象化加固 backlog（B-X）

原则（CLAUDE.md）：`#[cfg(target_arch)]` 仅允许"定义 current* 类型 / trait 实现点 /
`naked_asm!`+`asm!` 指令字面量（语法硬约束）"；**禁止**在行为逻辑中用 cfg 选行为。

**✅ 第一批已落地（2026-09-07，§22 Phase 2 迭代 4）**：新增 `minix-platform::test_support`
（unit_test_irq_desc + ARCH_NAME/SP_LABEL/PC_LABEL/FP_LABEL/REACHED_BANNER 常量族）；
kernel 侧 13 处行为选择 cfg 消除（kmain_verify 9 常量块 + 1 打印行 + `new_test_interrupt_controller`
三变体合一）；kernel 侧行为选择 cfg 清零。验证：kernel 694 + platform 13 全绿，三架构
production-target check 全过。

**剩余 open（✅ 全部收口，2026-09-09 V13 终批）**：
- `os/kernel/src/lib.rs` DirectMap 三 cfg 分支 → 改用既有根别名
  `minix_arch::CurrentDirectMap::default()`（三分支 ZST 加 `#[derive(Default)]`；
  别名早已存在于 arch lib.rs 根，kernel 未用而已）。kernel 侧行为选择 cfg 归零复验通过。
- `minix-platform/src/global.rs` 其余 cfg：逐处核实为"定义 current/trait 实现点"，
  合法（PlatformDescEnum 裁定沿用）。**B-X 关闭**。
- `os/libs/minix-platform/src/global.rs` 约 10 处 `#[cfg(target_arch = "x86_64")]`
  （global.rs:95-175 `PlatformDescEnum` 9 处经裁定为"枚举变体按架构存在性裁剪"= 合法；
  其余待逐处核实是否属行为选择）。

**触发条件**：doc 16 review 重启 / 新增 arch 平台时 cfg 重复暴增 / 再次发现行为选择 cfg。

## 1. 架构建议（原 §1-§6；全部闭环，保留总览与一行解决记录）

| ID | 层级 | 主题 | 结论（日期） |
|----|------|------|------|
| A1 | kernel | 全局访问器 witness 全覆盖 | ✅ `BklSection::assume_held()` 链根 + 调度/IRQ/boot 链 24 处迁移 + kernel_call_finish 去全局化；debug 断言当场揪出 8 个测试不持锁隐患（2026-09-07） |
| A2 | kernel | 全局静态收敛 | ✅ `os/kernel/src/globals.rs`（396 行：13 静态 + 包装器 + BklProtected 审批清单）；访问器族零改动（审计点单一化已足够，搬运无收益——记录防重查）（2026-09-07） |
| A3 | kernel | KProcess/KPriv Drop 防御 | ✅ 占用槽隐式销毁 panic fail-fast；测试豁免夹具三层；详见 `panic-in-drop.md`（2026-09-02） |
| B1 | kernel | BKL guard 跨函数传递 | ✅ `BklGuard::transfer(self)`（ManuallyDrop，全库唯一 `mem::forget` 收敛点）+ kernel_call_finish 入口 `debug_assert!(bkl_is_locked())`；11 处散点收敛（2026-09-07） |
| C1 | kernel | dispatch 大函数拆分 | ✅ 4 函数语义拆分：getinfo 468→35 / privctl 446→111 / vmctl 349→157 / trace 294→139（statectl 62 行不再 qualify）；verbatim 提取 + 46 处文档锚点同步（2026-09-08） |
| D1 | 全层 | errno newtype 单一来源 | ✅ `Errno(i32)` 于 `os/libs/minix-types/src/types/errno.rs`（唯一权威），kernel errno.rs 再导出（2026-09-09） |
| D2 | kernel | ToErrno 映射 trait | ✅ trait 于 errno.rs；impl：SchedProcError（sched.rs）/ PmError / KernelError（minix-types）；2 调用点迁移，手写映射删除；范围收窄说明：kernel 侧手写映射实测仅 1 处（2026-09-09） |
| D3 | kernel/arch | `Result<(), ()>` 清理 | ✅ ProfileClockError + WriteUserRegError（trait 签名级，三 arch + mock）；`vm::enqueue_and_notify` 按 R-18 保留（单一失败模式，全库仅剩 allow）（2026-08-14） |
| E1 | arch | 聚合 `Arch` trait | ✅ **落地形态与草图偏差（load-bearing 设计事实）**：supertrait 形式不可实现——Paging 是有状态句柄族（`from_active_root`），ZST 架构体无法伪造；改为全关联类型组合（17 关联类型覆盖 20 个 Current* 别名）+ `os/arch/src/arch/current.rs` 单点 cfg + MockArch 矩阵镜像（TypeId pin 测试）；既有别名不动、新代码优先 `CurrentArch`（OQ 并存裁决）（2026-09-08） |
| F1 | arch | pt_alloc 论证模型 | ✅ 改写为 SMP+BKL write-once-then-read-only 论证（os/arch/src/arch/pt_alloc.rs）（2026-08-14） |
| F2 | arch | register 同步 | ✅ debug_assert 防重复 + Release/Acquire；**trait 化演进已否决**——kernel 不做长期 PMM，FrameAllocator trait 无 owner（`os/arch/src/arch/frame.rs` 模块 doc "What this module does not own"），防 premature abstraction（2026-08-14） |
| G1 | 跨层 | ProcNr 双定义 | ✅ newtype 迁 `os/libs/minix-types/src/types/proc_nr.rs`（Redox ContextId 于 redox-syscall 同款）；arch 删别名改 `pub use`；kernel re-export ~600 使用点零 diff；边界拆包 3 处归零（2026-09-08） |
| M1 | qemu-tests | test-kernel 配置模板化 | ✅ `tools/gen-test-kernel.sh`（--new/--apply/--check）+ 23 文件规范化 + 死 profile 块删除；每架构聚合 crate 方案否决（feature unification 破坏 test-proc-init 无 alloc 约束）（2026-09-08） |
| R1 | arch | PTE 位权威位置 | ✅ 核实闭合（登记前提过时，Pattern #70）：`PageFlags` 语义位图（`os/arch/src/arch/paging.rs:28`）+ 三架构私有 raw 翻译层 + kernel 全消费点语义化早已达成；**关联常量形式被否决**——把 raw 位值以统一名暴露给共享层弱于现状封装（CLAUDE.md 硬件约束按更强者执行）（2026-09-08） |

### 1.1 Redox 对照结论（原 §6，2026-08-14 调研 + 后续落地）

同构且方向已正确：syscall 中心 match、usercopy 独立模块、共享 ABI crate 承载错误类型
（D1 落地）、全局锁 + 无 static mut（我们 sealed trait 更严格）、edition 2024 + panic=abort。
我们占优：细粒度 arch trait（Redox 用 cfg 换模块）、宿主单元测试密度（743 vs Redox ~19）。
Redox 领先且已对标：`CleanLockToken` 全链传递（我们 A1 assume_held 是雏形，S-8 trap entry
witness 根位补齐后可全链化）；rmm 独立内存管理 crate（F2 已论证不建——无 owner 抽象不建）。

## 7. 文档 01-30 未完成项汇总

### 7.1 DEFERRED 代码任务（D 系）

**✅ 已解决（44 项，一行备忘）**：D-1~D-13、D-15~D-35、D-42~D-58 全部闭环（落地批次见 §0.1；
逐条解决记录含 C 锚点与测试证据，见 git `893386cd8` 版 §7.1 表）。其中 🟢 设计 no-op 六项：
D-14（SIGKMESS 随 W-7 演进不可达）、D-24（VMSTYPE_MAP C 本身无此变体）、D-25（无部分进度
消费方）、D-30（swap 链恒空前提；**后被 U-2 推翻改真实现**，见 §12 Phase 5）、D-34
（mmap_size/mem_high_phys 无读者，write-only 状态不维护）、W-8 同 D-30。

**⬜ 仍 open（全部阻塞于 SMP，与 §10 Edge 表同源）**：

| # | 项 | 依赖 |
|---|-----|------|
| D-36 | `smp_init`（ACPI/MADT 表解析 + `SmpArch::boot_ap`） | S-4/S-5 |
| D-37 | `boot_lock`（smp.c:28） | 随 D-36 |
| D-38① | exception_dispatcher::handle 需 bkl_lock（②③④ 已闭合：lib.rs 两处 + 2026-09-07 S-1 核实 resume 路径已覆盖） | S-9 |
| D-39 | x86_64 `init_ap` panic 占位（protection.rs） | S-4（故意 panic：AP 无 TSS 即 triple-fault） |
| D-40 | ptproc per-CPU（现由 `CURRENT_PTPROC_NR` 全局单核承担） | S-6a |
| D-41 | `PLATFORM` AssumeSyncCell SMP 替换（Mutex/Atomic） | S-6b |

### 7.2 WONTFIX 设计排除清单（有 rationale，非缺口，防重查）

| # | 排除内容 | 理由 |
|---|---------|------|
| W-1 | NMI watchdog 全族（watchdog.c/arch_watchdog.c） | 架构覆盖不全 + 默认关闭 + 依赖 PMU；sprofile 替代（doc 26 §3.1） |
| W-2 | usermapped_data 全部 | 64 位无 `.usermapped` 段机制（doc 28 §4.2） |
| W-3 | IPC 消息跟踪/统计 hook 族（debug.c 12 函数 + 3 宏） | panic 路径风险；log crate + 外部工具替代（doc 29 §3.2-3.4） |
| W-4 | `nmi_sprofile_handler` | 无 NMI 子系统（doc 30 §4.4，同 W-1） |
| W-5 | SPROF `PROF_NMI` 路径 | 同上 |
| W-6 | `ClearMapCache` no-op | 64 位 Direct Map 无 cache table，匹配 C（doc 09 §4.5） |
| W-7 | kmess 缓冲体系（双缓冲/END_OF_KMESS/send_diag_sig/SIGKMESS） | EarlyConsole 直出 + log 后端替代（doc 27 §6.2，ARCH 演进） |
| W-8 | ~~swap_memreq no-op~~ → **已被 U-2 推翻并真实现**（2026-09-08，proc_table.rs `vm_swap_requestor`） | 见 §12 Phase 5 |

### 7.3 测试债（T 系）

**✅ 已解决**：T-1（10/10 行为矩阵）、T-6（10 测试）、T-7（5/5 队列）、T-9（IRQCTL 3 + 撤回项论证）、
T-11（runqueues_ok boot_integration）、T-12（5/5，附带修正 dispatch_clear 幂等对齐 C do_clear.c:38）、
T-5（QEMU CI：`.github/workflows/qemu-tests.yml`，23 测试 22 PASS，唯一 FAIL=S-3d WIP，CI 置
`QEMU_TESTS_SKIP_AP_ALIVE=1`）（2026-09-08~09）。

**⬜ 仍 open**：

| # | 项 | 依赖 |
|---|-----|------|
| T-2 | 7 个 SMP 测试（schedule_sync/stop_proc/sched_handler/ipi_sched/ipi_halt/migrate_proc 等） | S-12 |
| T-3 | init/load 顺序约束测试（违序应 panic） | S-12 |
| T-4 | init_ap 路径验证 | S-12/S-4 |
| T-8 | E2E grant 剩余 QEMU 半 5 个（间接链 ELOOP/magic 重定向/跨进程拷贝/vm_lookup/vm_memset——宿主 PTE walk 必然 Suspended 无法到达断言） | T-5 QEMU CI 承接 |
| T-10 | riscv64 真实启动链（U-Boot fatload→bootelf）集成 | ⏸ 诚实 DEFERRED（2026-09-09）：需 sudo 装 u-boot-qemu/dosfstools + mkimage 工具链 + 串口监控脚本，预计半天；CI ubuntu-latest 具备全部条件可随 workflow 补齐 |

### 7.4 文档任务 / 改进方向（I 系）

**✅ 已解决/闭合**：I-3（riscv64 高半核，QEMU 全 PASS 证实）、I-4（测试表去重）、I-8（=D1）、
I-11（BIOS legacy 范围声明非缺口）、I-12（RAII 拒绝维持，B1 承接）、I-14（B 层完成：09/16 前置
声明对齐实时序，06/08/10 核销，C 层重编号不做——风险>收益，2026-09-09）、SYS_PADCONF
（奇偶闭合：C 仅 `__arm__` map，BadCall→EBADREQUEST 即正确应答）。

**⏸/⬜ 仍 open**：

| # | 项 | 状态与解除条件 |
|---|-----|------|
| I-1 | kernel 独立 ELF 构建接入（build.rs + link.ld → xtask） | ⏸ 诚实 DEFERRED（2026-09-09）：三架构 link.ld 就绪（`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`）但 artifact 无引导路径——`KernelLoadResult.entry_point`（`os/boot-shim/src/loader.rs:66`）零消费者，boot-shim→kernel.elf 终局跳转未设计（入口 ABI：e_entry 处接收什么参数/寄存器态无定义）。解除前置：(a) 入口交接 ABI 设计；(b) boot-shim 终局跳转（S-4/S-8 相邻）；(c) bin 目标 + build.rs（先例：hello-boot-riscv64/build.rs）。单独做 (c) = 不可引导 artifact（frame.rs 先例） |
| I-5 | ACPI RSDP 搜索与表解析 | ⏸ 诚实 DEFERRED（2026-09-09）：三家族 QEMU virt 均不依赖 ACPI（平台发现走 KernelInfo+DTB），内核无 RSDP 消费方；MADT 常量骨架在位（`os/arch/src/acpi.rs`）。解除条件：物理机 x86_64 支持（真实 MADT/HPET 消费方）时实现 RSDP 扫描（EBDA + 0xE0000-0xFFFFF） |
| I-6 | `bill_ptr` 完整联动 + 真实调度主循环 | ⬜ open：调度循环仍为 placeholder（`os/kernel/src/lib.rs` kmain Phase 末段 `loop { spin_loop() }` 一带，V13 复核见 §23）；bill_ptr 调度期联动未接线。安全窗口在 S-7/S-10 后 |
| I-13 | `InterruptController` trait 拆分（Router + per-CPU Ack） | ⬜ open，P2：ack/eoi 是 per-CPU 动作（LAPIC EOI/ICC_EOIR/PLIC complete，参数 `_irq` 三架构均忽略）与 mask/unmask 全局路由动作塞进同一 trait；`ack(irq)` 应为 `ack() -> IrqVector`（GIC IAR/PLIC claim 读语义）。依赖 per-CPU 基础设施（S-6）+ irq_manager + doc 05/14 同步。现状可工作（BKL 保护），心智模型债 |
| I-7 / I-9 / I-10 | MF_REPLY_PEND typestate / NonNull / PrivId newtype | ✅ 评估闭合（2026-09-08）：均维持现状——typestate 增益在持久化边界消失；`s_stack_guard` 是地址值无 deref 目标、filter 链本体未实现（虚构抽象不建）；PrivId newtype 需改 81 处构造点且无混用 bug 史例，**跟进顺序：D1 先例建立后批量评估**（若未来评估，从 I-10 重启） |
| I-2 | `release[]`/`version[]` + MINIX_KERNINFO | 🔀 移交 edge_todo.md `E-KERNINFO`（2026-09-07）：消费方在用户态，共享契约面在 minix-types/minix-sys |

### 7.5 文档条目 ↔ 架构建议关联（保留防重查）

13 §6.5=§D1；13 §6.2 BklGuard RAII 已拒绝→§B1 transfer 承接；16 §4.13 BKL 接入→§A1/§B1
（D-38①余）；22 D9 裸指针→I-9 维持论证；24 D-25 部分进度→无消费方不建。

### 7.6 已解决排除表（grep 验证后不入清单，防重查）

11 `notify_scheduler`（proc_table.rs 完整实现）；12 `copy_msg_from_user`（UserCopy trait）；
07 syscall_copy 6 stub 全实现；20 §4.8 DEFERRED 全实现；13 §6.1 kernel_call_resume 时机修复；
05 Timer queue 测试已覆盖；16 cache 对齐 FIXME=C 同款非缺口；23 IPC_STATUS_* 已实现；
06 `EntrySpec::DEFERRED`=设计语义；08 三处 C 步骤差异=ARCH 演进；27 kmess=W-7。

### 7.7 跨阶段转移清单（原 §H 结论，均已落地）

页表页分配器 VM 接入 + minix-vm clippy → 02-stage-vm；init/load 测试 + init_ap + QEMU GDB CI +
ptproc per-CPU → 16-smp（=本表 D-40/T-2~T-5）；C-D-1~5 → notes/TODO.md（其中 C-D-5 已随 D3 闭合）；
QEMU E2E → notes/TODO.md QEMU backlog。原始 18 节全文见 git `97df4e58d^`。

## 8. GPT 评论分析结论（原 §18.5/§19/§20，压缩为结论）

- **§18.5 cause_sig backlog**：BL-1/BL-2/BL-3 全部闭合（D-57/D-13 落地、D-14 no-op）。
- **§19 doc 05 时钟评论（GPT 8/10）**：P1 ack API 与 P2 mask_all 改名并入 I-13 待 S-6；
  P2-3 DEFAULT_HZ 并入 OQ-15-1；P3 ClockArch 拆三 trait 不做（C-Rust 1:1 对应优先）；
  **P0-1（`ClockArch::init_timer` 过早使能 timer IRQ）——V13 复核仍未修**（`os/arch/src/arm64/clock.rs:69`
  仍 `msr cntp_ctl_el0, 1` Enable=1/IMASK=0；`os/arch/src/riscv64/clock.rs:84` 仍 `csrs sie, 0x20`），
  C ground truth 是 `bsp_finish_booting`（main.c:324）才经 `boot_cpu_init_timer` 使能；Rust 在
  `init_clock_and_interrupts` 即 live 而 handler 链（D-46 硬件半环/S-8）未接 → **升格为 §23 正式条目 D-59**。
- **§20 doc 06 评论（GPT 8/10）**：P1 两项措辞修正已落（FIX-06-GPT-1/1b/2/2b/2c，"唯一组合"与
  "BSS"表述，三轮穷尽扫描）；5 项设计债保留如下（原编号 D-52~56 与 §7.1 D 系冲突，重编号 GD）：

| ID | 设计债 | 触发条件 |
|----|--------|---------|
| GD-1 | ProcKind 拆分（ProcessRole × ExecutionDomain × PrivilegePolicy） | KernelTask 下出现 interrupt-driven / kernel coroutine / schedulable kernel thread 三情形 |
| GD-2 | EntrySpec 演进为 state enum（Kernel/Deferred/Ready） | 出现 Option 表达不了的第三种 entry 状态 |
| GD-3 | enable_user_io 从 CpuContextArch 拆出（ArchPrivilege trait） | trait 方法 >8 或边界频繁调整（现 6 方法） |
| GD-4 | Process vs ExecutionContext 概念拆分 | 引入 coroutine/微线程等新内核执行流类型 |
| GD-5 | CapabilityTemplate 重命名 | 引入运行时 capability 机制 |

## 9. Open Questions 存档

### OQ-15-1 DEFAULT_HZ 三方不一致 [✅ 已裁决（2026-09-09 V13 终批）：维持 100]

C（ground truth）i386=60 / earm=1000（`minix3/minix/include/arch/i386/include/archconst.h:4`）；
Rust=100（`os/kernel/src/clock.rs` + `os/arch/src/arch/clock.rs`）；doc 15 §2.1/§3.7 D7 已标注。
影响：无 `hz` 覆盖参数时 tick 频率/时间片与 C i386 不同（行为差异）。已做：差异三处标注。
选项：A. 保留 100（现状，boot 时可 `with_hz(60)` 对齐）；B. 改 60（影响依赖 100 的测试与
时间片计算）。关联：跨 crate 双定义的"权威位置"问题（与 G1 同型，若合并建议随 minix-types
config 层收敛）。

**裁决：选 A（保留 100）**——C 自身即两值并存（i386=60 / earm=1000），不存在唯一
要对齐的 C 值；差异已按 [ARCH: K-3] 三层标注（doc 05 + design + code，V13 复核一致）；
切 60 将作废全部按 100 调优的测试与负载采样窗；需要 C i386 行为时可 boot 传 `with_hz(60)`。
跨 crate 权威位置：维持现状（`clock.rs` 常量 + doc 标注），不新增 config 层。

### OQ-D49-1（已关闭）/ D19-1（已实现）

D-58 方案 B 落地后关闭（2026-09-06）；D19-1（cause_signal 致命 SELF 子路径）随 D-11 落地（2026-09-05）。

## 10. Edge Items：SMP/硬件阻塞残余（2026-09-06 全面扫描，V13 复核仍成立）

> 全部需要 SMP bring-up 或裸机/QEMU 基建，hosted CI 无法诚实验证。步骤编号见 smp_todo.md §5。

| ID | 项 | 阻塞 |
|---|---|---|
| D-36 | smp_init（ACPI/MADT + boot_ap） | S-4/S-5 |
| D-37 | boot_lock | 随 D-36 |
| D-38① | exception_dispatcher BKL | S-9 |
| D-39 | x86_64 init_ap panic 占位 | S-4 |
| D-40 | ptproc per-CPU（CpuLocal） | S-6a |
| D-41 | PLATFORM SMP 容器语义 | S-6b |
| sched-1 | per-CPU scheduler running 指针 | S-6c |
| tick-1 | per-CPU kernel-tick 统计 | S-6d |
| hw-1 | ~~D-46 硬件半环（asm IRQ stub + IDT load + 入口分流）~~ ✅ S-8（2026-09-14，test-timer-irq PASS 全链实证） | S-8 ✅ |
| hw-2 | D-48 shutdown(0)（现 halt-loop 即终态） | S-11 |
| trap-1 | ~~trap 入口接线 kernel_call_dispatch~~ ✅ S-8（B 体 asm→kernel_call 已接；disjoint-API 重构登记为 S-6 阻塞项） | S-8 ✅ |

非 SMP 的代码级 TODO 残余：ipc.rs NOTIFY 权限路径已改设计决策注释；lib.rs TODO-01-1 为
boot-shim 历史项已有 doc。

## 11. V12 轮 Rust 全量扫描结果（2026-09-07；8/8 条目全部闭环）

| ID | 严重度 | 结论 |
|----|--------|------|
| V12-A1 | P1 | ✅ `TrapStyle` 枚举（`os/arch/src/arch/trap_style.rs`，保留 C KTS_* 编号）+ `KProcess.trap_style` 字段 + sigsend 前置 EINVAL 校验 + **MINIX3 BUG 修复**（C arch_system.c:597 用户可控 trap_style 可 panic，Rust 校验先行）+ finish_and_restore 返回闸门分流；KernTrapStyle 并入（2026-09-07） |
| V12-A2 | P2 | ✅ `caller_q_find` 泛型化（accept 闭包插在 C CANRECEIVE 位）+ `el_match` 归一——单一真相源，测试测的就是生产路径（2026-09-07） |
| V12-A3 | P2 | ✅ senda notify 同分支合并为单布尔表达式（C proc.c:1301-1305 锚点）（2026-09-07） |
| V12-A4 | P2 | ✅ boot 回收重验：add_memmap 启动链接线本已落地（lib.rs bootstrap 守卫段），旧表 5 行"❌ 缺失"断言全部改写（Pattern #70 实例）（2026-09-07） |
| V12-B1 | P1 | ✅ proc_table.rs 17 处 panic 审计：16 处内部不变量维持 panic（C assert parity，14 处已有 R-15 注释）+ 2 处补契约注释 + 1 处分类纠偏（SYSTEM→VM 唤醒吞错改 C-parity panic）；无用户可达 panic（2026-09-07） |
| V12-B2 | P2 | ✅ unsafe 审计抓到真 UB：`do_vdevio`/`do_sdevio` 批量 I/O 缓冲对齐 1 → `slice::from_raw_parts` 转型对齐 4/2 失配（12 处 cast）——`IoBatchBuf`（`#[repr(C, align(8))]`）构造性修复 + paging.rs 两处形态修复（arch clippy 2→0）+ misc.rs union 读取补论证（2026-09-07） |
| V12-B3 | P3 | ✅ 实测关闭：`Message` = 72 字节（union 64 + 头 8，pin 进测试），每 syscall 至多 ~144B 瞬时栈占用，距 512B 阈值余量 3.5 倍（2026-09-07） |
| V12-B4 | P3 | ✅ clippy kernel 16→2（10 处机械修复；余 2 处为已登记不修：push_exclusion 特性门控误报、too_many_arguments=C-D-2）；arch 2→0；MADT 跨架构误报记录防重开（2026-09-07） |

**§21.3 非缺口备忘（防重查，保留）**：① `announce`/`prepare_shutdown`/`is_fpu`/`env_get("hz")`/
`cut_memmap`/`add_memmap` 均有实现（接线仅 V12-A4 一条且已闭合）；② debug.c 打印族已实现
（runqueues_ok_cpu/rtsflagstr/miscflagstr/print_proc），IPC 统计 hook 属 W-3；③ profile.c 经
dispatch_profile 覆盖（nmi 臂 W-4），watchdog.c 全族 W-1，usermapped_data.c W-2；④ 跨 stage
判定：V12 轮未发现新的跨 stage 生产代码缺口（微内核边界下 stage 内消费稳定契约即可独立实现）。

## 12. 全量收敛执行计划（§22 压缩版）

> 逐迭代串行：讲明白（grep 证据）→ 多方案对比 → 实施（代码+文档+测试）→ 测试全绿 →
> 8 公共门回归 → 打勾 + commit。EDGE 判定裁定（2026-09-07）：仅三类进 edge_todo.md——
> ①共享契约层的契约性变更（minix-types wire ABI / minix-sys trap 层 / os/arch 跨 stage 基建）、
> ②其他 stage 目录内生产代码、③多进程联调集成测试；共享库中纯增量、仅内核消费的类型改进
> （D1/G1）不属 edge。

### 12.0 环境基线（2026-09-07 实测；V13 待复测）

`cargo test -p minix-kernel` 691→729+3（含迭代增量）；QEMU 四模拟器齐备；三 production targets
就绪；rustc 1.94.1；Phase 边界回归 = 全 workspace cargo test + `os/qemu-tests/run_all.sh`。

### 12.1 Phase 0/1/2/4/6（✅ 全部完成，压缩为 §0.1 批次表 + §1/§11 条目行）

跨会话教训（保留）：并行会话共享工作区——提交前 `git status` 核查暂存区；在途编译错误阻塞
依赖链验证；minix-types Message union 64→72B 增长被 V12-B3 pin 测试当场拦截（pin 随实测更新）；
`RUST_MIN_STACK=8388608` 已入 `.cargo/config.toml`（深帧链击穿 2MiB 默认栈）。

### 12.2 Phase 3 — SMP 主线（smp_todo.md §5 为权威，此处只留状态索引）

S-0/S-1/S-2/S-2b/S-3a ✅；**S-3b ✅（2026-09-08，commit e41929cf2——本表原 ⬜ 为陈旧状态，
smp_todo.md §5 为准）**；S-3c ✅（PSCI 0xC4000003 / riscv IPI EID 0x735049 / x86 mfence 三真
bug 修复）；**S-3d ✅（2026-09-14 第四会话收官：test-smp-ap-alive-mb PASS、-d int 0 异常——
真因 lgdt 16 位寻址 modrm 0x15=[DI] 读 IVT 垃圾，修复为 0x16=disp16，另修阶梯尾部三处
记录字段绝对读字面量笔误；见 smp_todo.md 第十~十二轮记录 + 本文件 §25 终局段）。**

| 步骤 | 内容 | 依赖 |
|------|------|------|
| S-8 | ✅ asm trap stub + SYSCALL 入口（2026-09-14 收官——A/B 路径 + Win64 ABI/ISA override/LVT 屏蔽三真 bug + test-timer-irq PASS；见 smp_todo.md §21 完成记录） | S-3d ✅ |
| S-4 | init_ap 真实现（per-CPU GDT/TSS/GS_BASE/lidt + MSR 重编程 + LAPIC local timer） | S-8（D-39） |
| S-5 | smp_init 编排 + boot_lock（D-36 下半 + D-37） | S-4 |
| S-6a~d | per-CPU ptproc / PLATFORM 冻结语义 / 调度 running 指针 / tick 统计（D-40/D-41/sched-1/tick-1） | S-5 |
| S-7 | AP 主循环（与 BSP 同 BKL 所有权前置） | S-6 |
| S-9 | 异常入口 BKL（D-38①） | S-8 |
| S-10 | IPI 往返验证（riscv SSIE 路径） | S-7 |
| S-11 | shutdown(0)（hw-2；语义层 + 三架构 QEMU 后端） | S-10 |
| S-12 | 测试债（T-2/T-3/T-4）+ CI 化 | S-7+ |
| S-13 | 收尾 sweep（Edge 表迁移、doc 16 刷新、smp_todo 封存） | 全部 |

### 12.3 Phase 5/7 — 仍 open 的主线项

- ⬜ **I-6** bill_ptr + 真实调度主循环（替换 placeholder；安全窗口 S-7/S-10 后）
- ⬜ **I-13** InterruptController trait 拆分（依赖 S-6）
- 🔄 **profiling/sprofiling deferred 体**（misc.rs dispatch_sprofile 已全实现；剩余 = trap 入口接线
  传 PC，依赖 S-8/S-9；NMI 臂 ENOSYS 与 idle 轮询变体两处 documented 豁免）
- ⏸ I-1 / I-5 / T-10（解除条件见 §7.3/§7.4）
- ✅ SYS_PADCONF / syscall.rs:1013 / dispatch_clear 缺口盘点全部闭合（详见 git 893386cd8 版 Phase 5）

## 23. V13 轮：查漏补缺 + 架构深审（2026-09-09 扫描完成）

> 范围：`os/kernel` + `os/arch` + `os/plat` + `os/boot-shim` + 共享层内核消费面，方法与
> 定位：① Gate A 机器覆盖（SYMBOLS.md 已落 `.review/claude/fork-syscall-rewrite/stage01-rescan-20260909/`）
> ＋ 2026-08-12 后 91 个增量提交的语义对账；② 分层架构深审（共享层 → HAL → kernel 核心 → boot 链），
> 联网对照 Redox/Linux/OS 理论；③ 设计深度检查（translate 残留、BKL 纪律、借用结构）。
> 本轮只登记不改代码。详细证据链见同目录 scan.md。

### 23.1 覆盖结论（先说查漏结果）

- **38/38 个 `do_*.c` 全部有语义对应实现**（syscall.rs 分派 46 项覆盖 C call_vec 全部条目，
  11 个空洞与 C 同为 WONTFIX），本轮增量（U-1/U-2/do_sprofile）均对齐；**无疑似缺口**。
- 顶层内核文件（proc/system/clock/main/interrupt/table/smp/debug/utility）函数级对账：
  仅 2 条轻微项（入 D-65）；send_diag_sig/get_value 等落在 W-7/设计演进范围。
- include 契约抽查 12 项（端点常量/SYS 调用号/VMSUSPEND/NR_BOOT_PROCS/VmBootHandoff）：
  **零值漂移**。
- 标注体系反向覆盖：MINIX3 BUG 全库 7 处（6 处锚点精确，1 处 C 行号 ~5 行漂移→D-60）；
  `[ARCH]` 6 处三层一致（krandom D-33 的 design 快照层滞后一行→D-65）。
- **无新增 edge 条目**：本轮未发现跨 stage 生产代码缺口，V12 §21.3④ 结论维持
  （微内核边界下 stage 内消费稳定契约即可独立实现）。

### 23.2 新发现 P1（两项均为 S-8 前置必修的休眠缺口——现状无实害是因为中断从未真正开启）

#### D-59 timer IRQ 链：过早使能 + 使能抽象零调用 + 路由错线（复合） [P1] — ✅ 已修复（2026-09-09，V13 首个 todo-fix）

**问题**（五点，均实测锚点）：
1. **配置与使能合一步且提前**：arm64 `os/arch/src/arm64/clock.rs:69` 在 `init_timer` 内直接
   `msr cntp_ctl_el0, 1`（Enable=1/IMASK=0）；riscv64 `os/arch/src/riscv64/clock.rs:84` 直接
   `csrs sie, 0x20`——均在 Phase B `init_clock_and_interrupts`（`os/kernel/src/lib.rs:808` 调用点）
   即 armed；x86_64 `os/arch/src/x86_64/clock.rs:57-81` 是纯配置（只编程 PIT，LVT 保持 masked）。
   C ground truth：`minix3/minix/kernel/clock.c:48-66` init_clock 纯软件，硬件编程+handler 注册
   相邻且在 boot 最后一刻（`main.c:73` bsp_finish_booting 末段 → clock.c:294-304）。
2. **使能抽象已建但零调用**：`TimerIrqGate` trait（`os/arch/src/arch/timer_irq_gate.rs:62-78`）
   三架构 enable/disable 实现齐备，但生产 kernel **零调用**（`os/kernel/src/lib.rs:1902-1917`
   仅注释提及）——"配置→使能分离"的正确抽象落地后被旧通道（init_timer 内联使能）绕过。
3. **"no-op safety net" 名不副实**：`os/kernel/src/lib.rs:1875-1882` 在 bsp_finish_booting 里
   再调一次 init_timer，注释称 no-op；实际 arm64/riscv64 语义是重置比较器推后首个 deadline
   （重 armed，非 no-op）。
4. **riscv64 双重使能**：`os/arch/src/riscv64/arch_init.rs:54` 再写 `sie=0x22`，STIE 被置两次。
5. **IrqVector(0) 路由三架构不同**：`os/kernel/src/lib.rs:1893` 的 `register_hook(IrqVector(0))`
   + unmask 在 x86_64 正确（IOAPIC IRQ0=PIT）；arm64 上 unmask(0) 使能的是 SGI 0 而非 timer
   PPI 29（`os/plat/src/arm64/interrupt.rs:162-179` 写 GICR_ISENABLER0 bit 0）；riscv64 上
   unmask(0) 直接 return（`os/plat/src/riscv64/interrupt.rs:113-116`，PLIC 无 0 号源）——
   即 arm64/riscv64 的 timer IRQ 投递路径根本没接对。

**影响**：当前休眠（三架构 CPU 级中断掩码全程关闭，无 `sti`/daifclr/sstatus.SIE 生产路径，
S-8 前不会触发）。S-8 trap 入口落地、首次开中断的瞬间：Phase B 埋下的 deadline 早已过期
（首个 unmask 即收到积压 tick）；arm64/riscv64 则连投递线都没接对（D-46 时钟中断在这两个
架构上静默失效）。

**修法方向（多方案对比）**：
- 方案 A（推荐，对齐 C 时序）：init_timer 全架构改 configure-only（arm64 写 CNTP_CTL=2
  即 Enable=0/IMASK=1；riscv 写 mtimecmp 不写 sie.STIE；x86_64 维持现状）；TimerIrqGate
  成为唯一使能入口；bsp_finish_booting 的 re-call 删除或改语义化命名；hook 注册改为
  按架构正确的 IRQ 号（x86 IRQ0 / arm64 PPI 29 / riscv 将 timer 直连 sie 不经 PLIC——
  需 irq hook 层支持"非控制器路由"的中断源）。
- 方案 B（最小改动）：仅修 arm64/riscv64 使能时点后移到 bsp_finish_booting，IrqVector(0)
  路由修对；TimerIrqGate 维持零调用并删除（避免双通道）。
- 方案 C（保守）：全部推迟到 S-8 批次一并做（与 asm trap 入口同批验收）。
- 对照：Linux clockevent 把 clockevent device 的 set_state_* 与 irq enable 分离，
  enable 在 late_time_init 阶段（`start_kernel` 末尾）——与方案 A 同型；C Minix3 的
  "编程+注册相邻且最后" 是同一原则。
**裁决建议**：随 S-8 批次实施（方案 A 为目标形态）；S-8 前任何"开中断"代码不得合入。

**✅ 已修复（2026-09-09，方案 A 落地）**：
- **Phase B 纯软件化**：`init_clock_and_interrupts` 删除 `init_timer` 调用（C init_clock clock.c:48-66 纯软件 parity）；"Order matters" 注释重写（删除"arch_init 使能 LVT"虚构声称）。
- **Step 6 重建为 C boot_cpu_init_timer 完整序列**（lib.rs）：`init_timer` 只编程（arm64 `CNTP_CTL=0x2` 保持门闭；riscv64 不碰 `sie.STIE`）→ `register_hook(minix_plat::TIMER_IRQ, ...)`（首 handler 自动 unmask = C interrupt.c:65 parity）→ `<CurrentTimerIrqGate>::enable_timer_irq()` 开本地门；"no-op safety net" 重复调用与误导性 Behavior-change 块删除。
- **`minix_plat::TIMER_IRQ`**（新常量，per-arch 定义 + 根 cfg 选择）：x86_64=0（PIT→IOAPIC 0）、aarch64=30（CNTP NS PPI，CNTPNSIRQ）、riscv64=0（伪向量，PLIC mask/unmask 对 0 恒 no-op 即"无控制器线"的语义化）；三架构各带硬件事实 pin 测试。
- **x86_64 gate 语义修正 `[ARCH: gate-semantics]`**：enable/disable 改文档化 no-op（PIT 无本地门；旧实现清未编程 LVT = 打开无 handler 中断源的陷阱），LVT 门随未来 LAPIC 时钟源批次回归；trait 模块文档同步（调用时序 = 晚于编程与注册）。
- **riscv64 arch_init** 删除提前 `csrs sie, 0x22`（STIE 归门控、SSIE 归 S-7/S-10 IPI）。
- 文档同步：doc 05 §3.7（行为变更声明表标注已修复 + D-59 解决记录 5 条 + 调用时序约束改写 + 调用顺序段改写）、doc 08（Step 6 流程行/不变量/代码块三处）。
- 验证：kernel 729 passed / 0 failed / 8 ignored；arch 223 passed；plat 3 passed（含 3 个新 pin 测试）；三架构 production-target check exit 0；clippy 零新增。QEMU 回归见 commit 前记录。
- **附带发现（不属本条顺手修，登记为 T-13）**：`arch/src/arch/dm_coverage.rs` driver 测试族共享全局 mock 注册表无同步，并行调度下确定性失败（单线程 10/10 全过）——预存在的测试隔离缺陷，与本次改动无关。

#### T-13 [P2] dm_coverage driver 测试族共享全局 mock 注册表无同步，并行运行确定性失败

`arch/src/arch/dm_coverage.rs` 的 `tests::driver` 子模块各测试共享 mock 全局注册表（`mock_dm_clear`/`mock_dm_leaves`），无锁。默认并行调度下 `test_establish_window_pa_limit` 与 `test_establish_skips_hole_and_offsets_va` 稳定失败（断言读到他人测试的叶项：期望 `min_pa=0x3000_0000` 实得 `0x1C000`）；`--test-threads=1` 下 10/10 全过。hosted 测试通常经 `os/.cargo/config.toml` 的 `RUST_TEST_THREADS=1` 单线程运行故未暴露（D-59 验证时从仓库根目录调用 cargo 漏读 config 才现形——调用目录教训已录 §12.1）。修法：mock 注册表加 `BKL_TEST_LOCK` 同型互斥（kernel misc.rs SPROF_TEST_LOCK 先例），或 driver 各测试用互不重叠的窗口基址。

#### D-61 InterruptController：生产 dispatch 从不 ack，arm64/riscv64 eoi 将以 0 值完成中断 [P1] — ✅ 已修复（2026-09-09，V13 第二个 todo-fix）

**问题**：
1. **ack 全仓库无生产调用方**：`os/kernel/src/irq_manager.rs:442-507` 的 dispatch 只调
   mask(:452)/handler/unmask(:501)/eoi(:504)；arm64 的 eoi 依赖 ack 捕获的 `last_iar`
   （`os/plat/src/arm64/interrupt.rs:182-194`）、riscv64 依赖 `last_claimed`
   （`os/plat/src/riscv64/interrupt.rs:127-141`）——真中断到来时这两个字段是初值 0，
   eoi 将以 0 写 ICC_EOIR1_EL1 / PLIC complete。GIC/PLIC 语义：ack（读 IAR/claim）取得
   INTID → 处理 → 用**同一 INTID** 写 EOIR/complete；以 0 完成不是合法完成
   （Redox aarch64 / Linux generic IRQ 层 / GIC 规范同此协议，2026-09-09 联网核实）——
   后果是活动中断永不关闭 → 中断风暴。x86_64 的 ack/eoi 忽略 `_irq` 直接 `lapic_eoi()`
   （`os/plat/src/x86_64/interrupt.rs:218-224`）语义正确。
2. **arm64 mask_all/init 只盖 SPI**：`os/plat/src/arm64/interrupt.rs:196-203` 循环
   `32..nr_irqs`，SGI/PPI（<32）从未被 disable（init :83-90 同）；"init 后全屏蔽"不变量
   对 SGI/PPI 不成立（timer PPI 29 保留 firmware 遗留状态）。trait 文档表
   `os/plat/src/interrupt.rs:126` 声称 "GICD_ICENABLER=all" 与实现不符。
3. 关联但独立于 I-13：I-13（trait 拆分 Router/Ack）是抽象债；本条是**正确性缺口**——
   S-8/S-9 接通中断入口时若沿用即引暴。

**修法方向**：dispatch 路径补 ack（向量分发前读 IAR/claim 取 INTID，再按 INTID 派发
hook——这同时修正 mask/unmask 的 IRQ 号来源）；或 eoi 改为携带 handler 层回传的 INTID。
arm64 init/mask_all 补 GICR_ICENABLER0 处理（SGI/PPI 全屏蔽 + 按需 unmask PPI）。
与 I-13 的 trait 拆分合并设计（ack 归 per-CPU Ack 面、mask/unmask 归 Router 面），
**但修复不等待 trait 重构**——S-8 批次内先以现 trait 修正确性。

**✅ 已修复（2026-09-09，claim/complete 语义方案落地）**：
- **dispatch 接入 claim**（`os/kernel/src/irq_manager.rs`）：控制器调用序列定型为
  **ack（claim）→ mask → handler 链 → unmask → eoi（complete）**——claim 在最前，因为
  eoi 写回的 INTID 必须来自本次 claim 的捕获；C parity：claim 在 C i386 位于 asm 入口
  （`irq_handle` 之前），EOI 在 asm 尾部。
- **x86 ack 修正为文档化 no-op**：旧实现写 LAPIC EOI——把"完成"塞进"取号"，dispatch 头部
  调用即提前放行同级中断；x86 无 claim 寄存器，EOI 是完整握手（trait 文档表与 trait 方法
  doc 同步，`os/plat/src/interrupt.rs`）。
- **arm64 mask_all 补 SGI/PPI**（`os/plat/src/arm64/interrupt.rs`）：GICD 只达 SPI（≥32），
  追加 banked `GICR_ICENABLER0 = all`；`init_redistributor` 在 WAKER 唤醒后同样屏蔽
  INTID 0-31（firmware 遗留的 timer PPI 使能态被清除，timer 由 D-59 链的 register_hook
  unmask 按需打开）——"init 后全屏蔽"不变量对 SGI/PPI 成立。
- **顺序钉住测试**：`dispatch_claims_before_mask_and_completes_last`（mock 增加跨方法
  `call_log`，钉住完整序列 unmask[register]→ack→mask→unmask→eoi；mock 的 ack 不再
  混入 eoi_log）。
- 文档同步：doc 14 §2.6（C `hw_intr_ack=*_eoi` 是 i386 事实；Rust ack=claim 语义差异注）、
  doc 05（ack/eoi 握手精确语义 + 分发序列）。
- 验证：kernel 730 passed / 0 failed / 8 ignored；plat 3 passed；clippy 零新增；QEMU 全量
  回归（aarch64 GICR init 写经实测）随 commit 记录。
- **I-13 关系**：trait 拆分（Router + per-CPU Ack）仍待 S-6 per-CPU 基建；本条只修正确性，
  现 trait 内完成，I-13 维持 open。

### 23.3 新发现 P2

#### D-60 注释准确性批（4 处漂移 + 1 处虚构 + 1 处失真） [P2] — ✅ 已修复/核实闭合（2026-09-09）

`os/kernel/src/lib.rs:1807`（`see smp.rs:200`）、`os/kernel/src/lib.rs:1823`
（`see proc_table.rs:276`）、`os/kernel/src/syscall.rs:2858`（`see vm.rs:323`——
`os/servers/vm/src/` 无 vm.rs，文件级失效，应指向实际文件或删除）；Step 1.0f 原始
5 项清单见 git 893386cd8 版。另有：`os/kernel/src/lib.rs:777-780` 声称 "x86-64 LAPIC
LVT timer entry 在 X86_64ArchInit::init() 使能"——实际 `os/arch/src/x86_64/arch_init.rs:46-62`
是空壳（虚构使能点）；`os/kernel/src/syscall_copy.rs:561` 注释 "Failure is logged"
实际只是丢弃返回值（C 同样不检查，注释失真非行为缺口）；`os/arch/src/x86_64/cpu_identity.rs:57`
MINIX3 BUG 标注引 `arch_system.c:239`，实际合并语句在 :243-245（~5 行漂移）。
修法：fix-guard 逐处 grep 目标符号现址后重写。

**✅ 解决记录（2026-09-09，fix-guard 逐处 grep 复核后执行）**：
- **已修 3 处**（行内行号引用改为符号引用——`CpuLocal::set_running` / `ProcessTable::rts_unset`
  / `ipc::delivermsg`，消灭行号腐烂这一类）：lib.rs `set_running(IDLE)` 注释（:200→符号）、
  lib.rs `rts_unset auto-enqueues` 注释（:276→符号；现址 418）、syscall.rs `copy_msg_to_user`
  doc（vm.rs:323 文件级失效→`ipc::delivermsg` 消费点，grep 核实在 ipc.rs:402）。
- **已随 D-59 闭合**：lib.rs "LVT 使能点" 虚构声称（该注释块在 D-59 Step 6 重写中删除）。
- **复核剔除 2 条（防重查）**：① cpu_identity.rs 的 C 锚点 `:239` **正确**——C 源
  `arch_system.c:239-240` 正是 model 合语句，V13 扫描代理报的 ":243-245 漂移" 为误报；
  ② "Failure is logged" 全库 grep 零命中（已被此前批次修复或扫描误读），无需动作。
- 验证：kernel 730/0/8 全绿（注释改动无行为面）。

#### D-62 A2 收尾：globals.rs 之外 4 组静态漏网 [P2] — ✅ 已修复（2026-09-09，③ 契约显式化 + 收敛路径登记）

`os/kernel/src/globals.rs:1-6` 的"单一审计点"声明对核心状态成立，但：①
`os/kernel/src/krandom.rs:172/:177`（`KRANDOM` SyncUnsafeCell + `KRANDOM_INIT` AtomicBool）；
② `os/kernel/src/smp.rs:837`（`CPU_INFO` SyncUnsafeCell）——类型在审批清单、静态本体
不在 globals.rs；③ `os/kernel/src/clock.rs:58-79` 四个 AtomicU64 镜像
（CLOCK_UPTIME/REALTIME/BOOTTIME/TSC_PER_MS，与 CLOCK_STATE 字段并存，clock.rs:50 注释
自辩"免穿引用读"——属 [ARCH] 级双真相源，需裁决：镜像豁免登记 or 收敛到单一来源）；
④ **最尖锐**：`os/kernel/src/misc.rs:2227/:2242` 两个 `static mut`（SPROF_INFO +
256KB SPROF_SAMPLE_BUFFER）连 SyncUnsafeCell 模式都绕过（仅 addr_of_mut! 纪律，
misc.rs:2219-2222 自述）。修法：迁 globals.rs 或按 A2 反例条款论证豁免并登记；
`static mut` 至少升 SyncUnsafeCell + BklProtected 审批。

**✅ 解决记录（2026-09-09，11 个静态全部收编 globals.rs）**：
- **①②④ 迁移**：`KRANDOM`/`KRANDOM_INIT`、`CPU_INFO`、`SPROF_*` 六件 + `SPROF_INFO`/
  `SPROF_SAMPLE_BUFFER` 两个 `static mut` 升级 `SyncUnsafeCell`（`get()` 取代
  `addr_of_mut!`，19 个访问点机械替换；misc.rs 声明块删除改 import）；`SprofInfo` 与
  `u8`（`[u8;N]` 复合 impl 的元素类型）加入 `bkl_protected_impls!` 审批清单（即 A2
  设计的"审批摩擦点"动作）；globals.rs 头部清单 13 → 24。
- **③ 裁决：双源契约显式化，完全收敛登记为独立路径**。穷举消费方（sched/proc_table/
  ipc/syscall_clock 共 6 点）证实**全部持 BKL**——镜像注释"免穿引用读"的存在理由已
  失效；但完全收敛（删镜像、读侧走 `clock_state_with`）需给 `IpcEngine::
  build_notify_message` 等引擎内部函数穿透 `&BklSection` 签名，属 IPC 引擎级重构，
  不属"A2 收尾"边界。本批落地：4 个 Atomic 迁 globals.rs + 写侧 Release 双写/读侧
  Acquire/两源差 ≤1 tick 的契约写入 clock.rs 与 doc 15（C 单源 `kclockinfo` 差异如实
  记录）；收敛重构作为独立条目跟进（见下）。
- 🆕 **I-15（本条派生，open）**：时钟镜像收敛单源——删除 CLOCK_UPTIME/REALTIME/
  BOOTTIME 镜像，`get_*` 改 `&BklSection` 签名读 `ClockState`，穿透 `IpcEngine`
  `build_notify_message`/`mini_notify` 链。触发时机：IPC 引擎重构窗口（S-8 trap 入口
  改造会重塑 IPC 入口，届时一并做）。
- 验证：kernel 730/0/8 全绿；clippy 零新增；doc 15/25 声称同步（SPROF 访问描述、
  镜像契约）。

#### D-63 A1 约定破例两处（witness 通道） [P2] — ✅ 已修复（2026-09-09，两处同批）

**✅ 解决记录**：① `clock_irq_handler`（`os/kernel/src/clock.rs`）迁移到 `_with` 访问器族 +
`BklSection::assume_held()` 链根取证（`IrqHandler` fn 指针签名无法携带 witness，与
`KernelNotifier::notify_hardware` 同constraint；"boot context" 误导注释删除）——"运行期 IRQ
路径用 boot_unchecked"的契约违反消除；② `ipc_filter_pool` 裸访问器消灭：`notify_scheduler`
（proc_table.rs，`section` 参数本就在作用域，纯疏漏）、`kernel_call_dispatch_inner` 的
Statectl 臂（syscall.rs，同上）、`dispatch_ipc`（syscall.rs，assume_held 链根 + kernel_call
dispatch 契约注释）三处全部迁 `_with`，`lib.rs` 删除裸访问器与 `#[allow(dead_code)]`。
测试侧：`clock_irq_handler` 直调测试与 4 个触及 assume_held 链根的测试补 RAII 取锁
（`bkl_lock_reset_for_test` + `bkl_lock()` 不 transfer，作用域尾自动解锁——首版用 transfer
遗留锁定态导致字母序后续测试自旋挂死，回归 review 拦截后修正）。验证：kernel 730/0/8 全绿，
clippy 零新增。

① `os/kernel/src/clock.rs:1271-1275` `clock_irq_handler`（D-46 新代码）在**运行期定时器
IRQ 路径**用 `*_boot_unchecked` 访问器族——"boot context" 注释名不副实，`_with` 访问器
可用而未用；② `os/kernel/src/syscall.rs:781-783` `dispatch_ipc` 用裸 `ipc_filter_pool()`
（`os/kernel/src/lib.rs:1740`，无 witness 参数），而带 witness 的 `ipc_filter_pool_with`
（lib.rs:1748）挂着 `#[allow(dead_code)]`——同文件新代码走旧通道。修法：迁移到 `_with`/
`assume_held` 链根取证（同 A1 已迁 24 处的形态）。

#### D-64 boot 链窄项三件 [P2] — ✅ 已修复（2026-09-09，三件同批）

① **OpenSBI 路径 FREE_MEMMAP 含内核镜像**：`os/boot-shim/src/opensbi_helpers.rs:396-403`
把整段 DRAM 报成单一 CONVENTIONAL 区（含内核/模块/shim）；kernel Phase A.2 只 cut 模块
（`os/kernel/src/lib.rs:439-445`），内核镜像只在 VmBootRegion 排除与 handoff 快照中扣除
——GET_MEMINFO 出口与 C `pre_init.c:196-199`"内核按额外模块 cut"语义在 OpenSBI/U-Boot
路径不一致（UEFI 路径因 LOADER_DATA 天然无害）。修法：kernel 侧 Phase A.2 补 cut 内核
镜像区间（stage 内闭合，非 edge）。② `os/boot-shim/src/uefi_helpers.rs:245-255`
`assert_bootstrap_outside_memmap` 定义后零调用——shim 侧防线是死代码，wire 或删除
（删除需说明"为何死"：kernel 侧 vm_handoff 扣除已兜底）。③ Phase E 注释壳
（`os/kernel/src/lib.rs:491-496` 无可执行语句，system_init 已被 enum match 取代）——
kmain 头注/流程注释与实际不符，注释级同步。

**✅ 解决记录（2026-09-09，三件同批）**：
- **① 内核镜像自切**：Phase A.2 新增 Step 3——`cut_memmap(kern_phys_base, kern_size)`
  （C parity：`pre_init.c:196-216` 把内核当额外 module `kern_mod` 一并 cut）。UEFI 路径
  no-op（内核 LOADER_DATA 不在 conventional memmap，cut 返 Err 被忽略=预期）；OpenSBI
  路径修复 GET_MEMINFO 把内核镜像报成可用内存的偏差。
- **② 死防护代码接线**（选择 wire 而非删除——它是真实的 fail-fast 防线）：
  `prepare_boot` 在 root page/bump region 两笔 LOADER_DATA 分配后调用
  `assert_bootstrap_outside_memmap`，固件若把 conventional 页分给引导分配即刻 panic
  （此时 boot-shim 尚能打印诊断）。
- **③ Phase E 注释**：kmain 头注 Phase E 行改为"无 boot 期动作"（C system_init 已被
  enum Syscall 分派吸收）+ Phase B/F 行补 D-59 后状态；Phase E 体注改写为准确叙述。
- 文档同步：doc 01 回收逻辑分布表补第 4 行（内核镜像自切 + 两路径差异）。
- 验证：kernel 730/0/8 全绿（memmap 18/18）；boot-shim x86_64-unknown-uefi check 通过。

#### I-6 改判：bill_ptr 链路已闭环，主循环真实可达（原"保持 DEFERRED"降级为三个小余项）

**V13 实测推翻旧登记**：五阶段调度循环（`os/kernel/src/lib.rs:2583` switch_to_user，
真 `loop` :2606-2664）生产可达（真 kmain lib.rs:382 → bsp_finish_booting :1773 →
:1959）；"placeholder 占位循环"只存在于 qemu_test/mock 的 cfg 分支（lib.rs:714-726/:191-196），
生产构建没有占位循环。bill_ptr 完整链路已在：写点 set_bill_to_idle
（proc_table.rs:520）/pick_and_bill（lib.rs:2148-2153，含 is_billable 判定）/idle
（lib.rs:2266-2270），读点 clock_irq_handler→tick_with(billp)（clock.rs:1277-1327，
对齐 C clock.c:114-152 的 billp 三用途）。**生产单跳限制的根因是 S-8 缺席**（首个
restore_to_user 交出 CPU 后无向量回内核），非调度器缺陷。
**剩余三个小余项**（并入本条跟踪）：① idle 的 kernel_ticks 未接线；②
`CpuLocal::set_running` 无 BILLABLE 判定；③ S-8 落地后循环多轮行为端到端验证。

**✅ 余项解决记录（2026-09-09，①② 落地；③ 维持 S-8 依赖）**：
- **① kernel_ticks 裁决为 no-op（C write-only 实证）**：全 C 树 grep 显示
  `kernel_ticks[CONFIG_MAX_CPUS]`（glo.h:86）只有声明与 arch_clock.c:231 的累加、
  **无任何读方**（do_getinfo/debug 均不触）——接线即 write-only 状态（D-34 标准）。
  idle Step 4 的缺口 TODO 注释改写为该裁决 + 证据。随带补上 **p_cycles 的 KERNEL
  分支累加**（C arch_clock.c:232，`p_cycles` 经 GET_PROC 可观察，是真缺口）：
  `idle` 中 `kernel.p_cycles.add_cycles(tsc_delta)`。
- **② set_running 收紧为 proc_ptr 原语**：bill_ptr 写是调用点策略（C 的三个写点
  两个 BILLABLE 门控、boot 点目标是自带 BILLABLE 的 idle；Rust 门控路径 =
  set_bill_to_idle / pick_and_bill / idle）。原语不再无条件写 bill_ptr，
  新增反例测试 `test_cpu_local_set_running_does_not_bill`（原测试断言的正是
  要消除的行为，已同步改写）。
- 🆕 **I-16（本条派生，open，P2）**：**p_cycles.total 全库无累加点**——
  `CyclesStats::add_cycles`（proc.rs:733）在生产代码零调用方，GET_PROC/SCHEDCTL
  导出的 p_cycles 恒 0；C 的 context_stop 一般分支（arch_clock.c:245-250）在每次
  上下文切换/时钟路径累积 `p->p_cycles += tmp`。修法需在 Rust 的切换链
  （finish_and_restore / pick 循环）找 C context_stop 调用点全集一一对应，
  属记账面专项（建议与 I-15 时钟镜像收敛同窗口做）。
- **③ 维持**：S-8 落地后端到端验证（原依赖不变）。

### 23.4 新发现 P3（D-65 轻微项批） — ✅ 已处置（2026-09-09：①③ 落地，② 维持登记，④ 误报纠正）

① idle 命名：C `proc.c:66-90` set_idle_name 产出 "idle<n>"，Rust 硬编码 "IDLE"
（`os/kernel/src/proc_table.rs:85-88`，测试 :1298 钉住）——诊断输出漂移，取舍登记
（对齐 C 或保留类型化名，二选一即可）；② `get_value`/env_get boot 旋钮通道缺失
（C `main.c:481-508`；Rust 结构化 handoff 未留字符串通道——设计演进合理，将来需要
"boot 参数改内核策略"时再立项）；③ krandom D-33 的 `[ARCH: deviation]` doc+code 两层
一致、design 快照层（25-design.v1.md）未回写一行；④ `os/kernel/src/debug.rs:302/:313`
两个 `#[ignore]` 实为测试基建缺口（mock logger 未初始化）而非硬件依赖，可初始化后转
常规测试。

**✅ 处置记录（2026-09-09）**：
- **① idle 命名对齐 C**：`"IDLE"` → `"idle0"`（C `set_idle_name(name, cpu)` =
  "idle"+十进制 CPU 号，proc.c:66-90；GET_PROC 外部可观察，Ground Truth 链强制）。
  const 声明与 pin 测试同步（`[..5] == b"idle0"`）。
- **② env_get 旋钮通道**：维持登记（设计演进方向非缺口，需 boot 参数改内核策略时立项）。
- **③ krandom D-33 design 层回写**：`.design/25-design.v1.md` 追加 D33 补记，
  [ARCH: deviation] 三层一致达成（.design 不入库，本地生效）。
- **④ 纠正（V13 扫描代理误报）**：debug.rs 两个 `#[ignore]` 是**真硬件依赖**——
  hosted x86_64 经 plat cfg 选真 COM1 控制台，无 iopl 的 port I/O 即 SIGSEGV
  （实测确认，与 T-11/stacktrace 同类），并非"mock logger 未初始化"；已恢复 ignore
  并改写为准确理由。教训：ignore 理由注释的错误比 ignore 本身更误导。
- 验证：kernel 731/0/8 全绿。

### 23.5 Rule Discovery（Step 5.7）

✅ **Pattern #84 候选："抽象落地未收口"（Built-but-not-wired）**——正确的新抽象/API 落地
后，生产调用路径未迁移、继续走旧通道，新旧并存且旧通道往往是错的那个。本轮三实例：
`TimerIrqGate` 三架构实现齐备但生产零调用（D-59）；`ipc_filter_pool_with` 挂
`#[allow(dead_code)]` 而新代码走裸访问器（D-63）；`InterruptController::ack` 有实现无
生产调用方、dispatch 直接 eoi（D-61）。检测命令：`rg "fn xxx" + 生产调用点 grep == 0`
或 `#[allow(dead_code)]` 清单复核。**待用户确认后落地**到 review-patterns（同 #78 先例，
本 scan 先登记）。

### 23.6 处置建议（供 todo-fix 排序）

1. **S-8 批次前置**：D-59 + D-61（+ D-63① 因同一 IRQ 路径）——两项 P1 在中断真正开启
   （S-8 trap 入口）时从休眠转实害，建议作为 S-8 的显式前置子步骤或同批验收项；
   smp_todo.md FROZEN 不重开，但 S-8 的验收标准应包含"timer IRQ 链三架构端到端"。
2. **独立可修**（不依赖 SMP）：D-60、D-62、D-63②、D-64、D-65、I-6 余项①②。
3. **需裁决**：D-62③（clock.rs 四 AtomicU64 镜像的双真相源：豁免登记 vs 收敛）；
   D-65①（idle 命名取舍）。
4. 收敛评估：本轮新发现 2 P1 + 4 P2 + 1 P3 批 + 1 改判，占增量窗口（91 提交）的可
   发现面比例合理；对已收敛面（30 轮 doc review + V12）零重复发现，符合增量策略预期。

## 24. V13 执行 campaign 终局对账（2026-09-09）

**已修复（本 campaign，一 TODO 一 commit）**：D-59（1297da86c）、D-61（15e76355e）、
D-63①②（9cf8788bb）、D-60（8cf00a60e）、D-62（ae951c71a）、D-64（81d873818）、
I-6 余项①②（a22ae791a）、D-65（54d5403ea）、B-X 收口 + OQ-15-1 裁决（终批）。
派生新条目：T-13（dm_coverage 并行 flake）、I-15（时钟镜像收敛）、I-16（p_cycles 累加）。

**诚实 DEFERRED 维持（依赖未解除，逐条复核）**：
- **SMP 链**（S-3d 起全部步骤、Edge 表 12 项、T-2/T-3/T-4、I-13 trait 拆分、profiling
  deferred 体、trap-1/hw-1/hw-2）：依赖 = QEMU gdb 硬件调试 AP bring-up（smp_todo.md
  S-3d WIP ⏸，此前 4 轮尝试后主动暂停）——本会话未解除，维持 FROZEN 设计 + 暂停态。
- **T-10**：sudo/工具链不可得（解除条件已具体化），维持 DEFERRED。
- **T-8 QEMU 半**：归 T-5 CI 承接（需新 QEMU 测试内核工作，独立批次）。
- **I-1**：入口 ABI 设计 + boot-shim 终局跳转未做（单独构建 = 不可引导 artifact），
  维持 DEFERRED；**I-5**：无 RSDP 消费方，维持 DEFERRED。
- **I-15/I-16**（本 campaign 新登记）：触发窗口 = S-8 IPC 入口重构 / 记账专项，open。

**stage 状态总结**：本 campaign 后，01-stage-kernel 全部"可诚实完成"的 stub/deferred/TODO
清零；剩余 open 项的依赖全部在 SMP bring-up（S-3d 起的硬件调试链）或外部环境（sudo），
无一项可在当前环境下进一步推进。

## 25. S-3d 续诊（2026-09-09 第二会话：三修复 + 环境级根因 + 下一步）

**真实修复（已入库，见 smp_todo.md S-3d V13 续诊记录）**：
1. **FJ16/FJ32 远跳偏移脱钩修复**：手写 0x8040/0x80C0 与实际布局脱节（AP 落进 FJ32
   操作数字节执行垃圾——S-3d 卡死的直接原因之一），改汇编期 label 差值 + AP_BASE
   推导，hosted 签名契约测试（`test_far_jump_targets_hit_real_instruction_boundaries`）
   钉住"目标落点=下一阶段首指令"。
2. **SDM 强制延时补齐**：INIT→SIPI 10ms / SIPI 间 200µs（`tsc_delay`，C arch_smp.c
   udelay parity）——消除 0x0000/0xA2 随机（SIPI 在 INIT 未完成时被丢弃）。
3. **级标诊断设施**：梯子逐级写 AP_STAGE_MARK + #UD 收集器（IVT[6]→handler 记录
   故障 CS:IP）+ 串口级标——本轮全部定位证据的手段即来源于此，S-4/S-5 调试沿用。

**环境级根因（monitor `xp` 实证，修正此前"lgdt 0x67 编码"旧猜测）**：
- 本 QEMU+OVMF(TCG) 组合的 MpInitLib 将 AP 停放跳表/循环置于 **PA 0x8000-0x91xx**
  （每页头 3 字节页号痕迹 + `jmp 82:xx` 跳表 + CS=0x82 停放循环），与 vector 0x08
  选页（PA 0x8000）及 scratch（0x9000）全面冲突；
- 存在 **2/3 号 #UD 游走核**，持续执行/清零低内存（blob 安装后被清零实证）——
  低内存 trampoline 在该引导路径下不可靠。
- 已搬迁 blob/scratch/marks 至 vector 0x05（PA 0x5000/0x6000/0x6F00）并复核
  GDT/record 内容正确落盘，但游走核踩踏仍在——**属 OVMF+TCG 环境级问题**。

**下一步（S-3d 解除的明确路径）**：测试内核改 **multiboot 直启**（QEMU `-kernel`，
绕开 OVMF：AP 天然 wait-for-SIPI、无固件 AP 干扰、无 UEFI 依赖），multiboot 入口
桩 + 复用 kernel 库，随后 S-3d 在干净环境一次跑通，顺路成为 S-8（asm trap 入口）
的裸机测试基建。

**终局（2026-09-14 第四会话，第十~十二轮，详见 smp_todo.md 同名记录）**：mb 直启
内核在第十轮以"先反汇编产物、再运行"的方法排除构建变量后，九轮"fill base 字节
写入未生效"改判为 **WSL 崩溃留下的陈旧 rlib**（干净重建后 fill 序列逐条正确，
desc 回读 27 00 10 61 ✓）；十轮谜团真凶由 `-d int` 故障转储首次给出的 AP 视角
GDTR={0xFF53F000, 0xFF53}=**IVT[0] 字节（F000:FF53）**钉死——手写 LGDT 的
modrm 0x15 沿用 32 位寻址形态（rm=101=disp），16 位寻址下 rm=101=[DI]，lgdt 实际
从 [DI=0] 装载；修复为 0x16（rm=110 mod=00=[disp16]）。连带修复阶梯尾部三处记录
字段绝对读的重复"60"字面量（含 stack 读 +4 与 offset_of! 钉死偏移矛盾——AP 从未
到达尾部故未引爆），并新增
`test_ladder_absolute_reads_match_record_layout` 钉死全部绝对读字节契约。修复后
**`TEST_RESULT: PASS test-smp-ap-alive-mb`、中断日志 0 异常**（AP 一次爬完
16→PE→32→CR3/PAE/LME/PG→64→`ap_early_entry`，BOOT_ACK 观察 ✓），minix-arch
225 hosted 测试全绿。修复 commit aa73eeb5f。**SMP 链自 S-8 起解锁**（S-8 asm
trap stub + SYSCALL 入口 → S-4→S-13 与 Edge 表 12 项顺次解冻）。

**全量回归（同日）**：run_all.sh 600s 截断统计 **19 PASS / 0 FAIL**——x86 9 项
全绿，其中 **canonical OVMF 路线 `test-smp-ap-alive` 亦首次 PASS**（此前"游走核
踩踏低内存"应为本测试三重故障后被 QEMU 复位的 AP 的次生现象，非固件环境问题；
smp_todo §25 环境级根因段相应降级为历史假说）；aarch64 全绿；riscv64 前 3 项绿
（截断段与本修复无关——x86 阶梯不参与 arm/riscv 构建）。CI 计数的 23 测试至此
应全绿，T-5 记录待 CI 复跑刷新。

