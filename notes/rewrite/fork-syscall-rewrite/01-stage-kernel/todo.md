# 01-stage-kernel TODO 总清单（架构优化建议 + 文档 01-30 未完成项收集）

> 本文件由"历史归档"转型为 **TODO 总清单**，两个来源：
> 1. **§1-§6**：01-stage-kernel 相关全部 Rust 代码的**整体架构审查**建议（2026-08-14）——
>    `os/kernel`（35.5k 行 / 36 文件）+ `os/arch`（16.2k 行 / 61 文件）+ `os/plat` +
>    `os/boot-shim` + 共享层（minix-types / minix-boot / minix-elf / minix-platform），
>    视角从整体到分层（共享层 → arch 层 → kernel 层 → boot 链），
>    结合 Redox OS 实现、OS 方向与 Rust 社区最佳实践对照。
> 2. **§7**：01-30 全部文档中**未完成 / DEFERRED / TODO / WONTFIX / 已知缺口**的系统收集
>    （2026-08-14），每条带文档出处，跨文档去重。
> 原历史归档（18 节完整内容）完整保留于 §H。
> 2026-08-14 修复会话：Step 0.7.1 全量 grep 验证后，18 项 DEFERRED 确认为已实现（原记录过时，
> 附 grep 证据），1 项（D-30）重新分类为设计 no-op（W-8），D-6/D-35 标注部分实现；
> 同步更新 doc 01/23/24 过时描述。剩余 DEFERRED 依赖 SMP/VM/IPC/scheduler wiring（见 §7.1）。

## 0.1 处理状态总览（2026-08-14 修复会话）

| 分类 | 处理 | 明细 |
|------|------|------|
| ✅ 已修复（代码落地 + 测试通过） | **5 项** | §F1 pt_alloc 论证模型改写（SMP+BKL write-once）、§F2 register 防重复 + Release/Acquire、§D3 命名错误类型（6/7 处，vm.rs 按 R-18 保留）、D-51 PrivUpdateRequest 字段序重排（2026-09-03，KPriv 子结构镜像 + RS 侧同序，见 §7.1）、D-53 cpu_identify + GET_CPUINFO 全记录（2026-09-04，arch 探测 + CPU_INFO 表 + C ABI 对齐，见 §7.1） |
| ✅ 已修复（代码落地 + 测试通过，2026-09-05） | **2 项** | **D-6** dispatch_exit 全 cause_sig 接线（syscall_process.rs:351-364，SIGABRT 自杀经管理器通知/自管理致命路径；原 cause_signal_abort 删除）+ **D-11** cause_sig 致命 SELF 路径（syscall_signal.rs:210-256，is_lethal 101-103 + s_bak_sig_mgr 提升 + RTS_UNSET(NO_PRIV) + 递归重投 + 无 backup panic；19-design.v2；doc 17 §4.3 / doc 19 §4.3/§4.7；新增 6 测试（syscall_signal.rs：is_lethal 1 + cause_signal 5），见 §7.1 行） |
| ✅ 已修复（代码落地 + 测试通过，2026-09-05，第二批） | **1 项** | **D-57** `proc_stacktrace` 接入 `cause_signal` 致命 SELF panic 路径。新增 `os/kernel/src/stacktrace.rs` 模块（`proc_stacktrace` + `make_read_word` 助手，`Cell<*mut u8>` 承载栈 scratch 缓冲保持 `Fn` 闭包语义）；`syscall_signal.rs` 致命无 backup 分支 panic 前调用；`syscall.rs` DIAGCTL STACKTRACE 路径去重改用同一助手；新增 8 测试（4 运行 + 4 `#[ignore]` 需真硬件）；doc 19 §4.3/§4.7 同步（移除"省略 proc_stacktrace"标注）；`#[cfg(not(test))]` 守门避免 hosted Linux test SIGSEGV（无 `iopl`）。详 §7.1 D-57 行 |
| ✅ 已修复（代码落地 + 测试通过，2026-09-05，第三批） | **1 项** | **D-13** `sig_delay_done` 停止延迟结束通知（system.c:454-464）。`ProcessTable::sig_delay_done`（清 MF_SIG_DELAY + cause_signal(SIGSNDELAY)）双路接线：process_misc_flags SC_DEFER 分支（proc.c:379-381）+ receive 投递记录（IpcEngine sig_delay_sender → dispatch_ipc 统一收尾，proc.c:1082-1083）；dispatch_ipc 改持 &mut ProcessTable；新增 6 测试 + 端到端。详 §7.1 D-13 行（已知限制：SIGSNDELAY=70 超出 SigSet(u64) 位宽） |
| ✅ 已实现（原记录过时，非缺口，2026-08-14 二次核实） | **18 项完整 + D-38 ②③ 部分** | §7.1 表 ✅ 行：D-1~D-5/D-7（dispatch_exec/clear/runctl/statectl 全部落地，含 process name 跨空间拷贝、release_address_space、clear_endpoint、SMP IPI）、D-10/D-12（cause_signal SELF 路径 mini_notify_core + 去重）、D-19（allow_ipc_filtered_memreq→dequeue_filtered）、D-21（kernel_call_resume + 2 测试）、D-22（data_copy_vmcheck VMSUSPEND 路径）、D-23（三架构 PTE walk）、D-26~D-29（GETINFO 10 分支）、D-31（sprof 数据拷贝）、D-32（clean_seen_flag）；D-38 ②③ BKL 接入（lib.rs:1956/:2167）；另有 I-4（doc 01/02 测试表去重，01 残留 TODO 标记已清理） |
| 🟢 设计 no-op（有 rationale，非缺口） | **1 项** | D-30 swap_memreq（misc.rs:1807 注释 + doc 25；与 W-6 ClearMapCache 同类，见 §7.2/§7.6） |
| 📌 保持 DEFERRED（已核实依赖仍成立） | **27 项** | 见 §7.1 表（SMP/VM/IPC/scheduler wiring 依赖 + RS 联调）；**D-58（2026-09-05 新增）：boot 能力模板 sendto 掩码自位残留（OQ-D49-1 裁决为独立 TODO 暂时 defer）** + §18.5 cause_sig 收尾 backlog；其中 D-35 本地路径已实现（SMP IPI pending）、D-38 ①④ 仍 DEFERRED（SMP 异常路径）、~~D-43/D-45 已于 2026-09-05 实施完成，移入上方第五批行~~、D-14/D-15/I-2/I-6 已核实更新、**D-52（2026-08-31 新增）：sched_proc 裸 set/clear 对齐 C RTS_SET/RTS_UNSET 语义决策（deferred 到 11-scheduling-primitives review）**（~~D-57 已于 2026-09-05 实施完成，移入上方第二批"已修复"行~~；~~D-53 已于 2026-09-04 实施完成，移入"已修复"行~~；~~D-6/D-11 已于 2026-09-05 实施完成，移入上方 2026-09-05 行~~；~~D-13 已于 2026-09-05 实施完成，移入上方第三批"已修复"行~~；~~D-49 已于 2026-09-05 实施完成，移入上方第四批"已修复"行~~） |
| ☐ 未解决（架构建议，待专项） | **10 项** | A1/A2/B1/C1/D/D2/E1/G1/M1/R1（均需设计决策或专项重构，见各自章节；~~A3 已于 2026-09-02 实施完成，见 §2 A3~~） |
| ✅ 已修复（doc 准确性问题，2026-08-18 GPT 评论评审） | **5 项** | §20.6 FIX-06-GPT-1/1b（§3.9 + §3.1 "唯一组合"措辞降级）+ FIX-06-GPT-2/2b（§3.8 + §3.1 "BSS"表述修正）+ FIX-06-GPT-2c（§3.15 决策表"BSS 段零运行时开销"修正） |
| ✅ 已修复（代码落地 + 测试通过，2026-09-05，第五批） | **2 项** | **D-43 + D-45** `process_misc_flags` 信号路由收尾（signal module 依赖已由 2026-09-05 前几批解除）：`SC_TRACE` → `cause_signal(SIGTRAP)`（proc.c:392-398）+ delivermsg `Segfault` → `cause_signal(SIGSEGV)`（proc.c:271-278，路由落消费侧、`delivermsg` 保持 FIX-20 无表访问契约；WARNING printf 以 `#[cfg(not(test))]` EarlyConsole 保留）；两处均不 `break`、落入可运行性复查 = C proc.c:413 语义。设计决策三方案对比 + Linux/Redox 对照见 doc 10 §3.3（新增）；doc 10 §4.5 差异表 row 11/12 更新 + row 13（D-44 vm_suspend，保持 DEFERRED，阻塞 D-20）；doc 12 §4.6 对齐表更新；doc 19 头部状态句修正 + §4.7 表加 2 行。新增 5 测试（proc_table.rs），`cargo test -p minix-kernel` → **669 passed, 0 failed, 6 ignored**，clippy 对 proc_table.rs 零告警。详 §7.1 D-43/D-45 行 |
| ✅ 已修复（代码落地 + 测试通过，2026-09-05，第四批） | **1 项 + 1 P0 顺带** | **D-49** `set_sendto_bit` 运行时路径全链落地（`PrivTable::set_sendto_bit`/`unset_sendto_bit`/`fill_sendto_mask`/`update_priv` + `IpcMask::set_bit/unset_bit/has_bit` + `TrapMask::allows_more_than_receive`；接线 SET_SYS 默认掩码 / UPDATE_SYS / `dispatch_update` 掩码继承三处；`update_from_request` 拆分为私有 `apply_fields_from_request` + 命名错误 `PrivUpdateError`，D3 方向）；**顺带 P0**：SET_SYS 成功后 `p.priv_id` 未回链（C `get_priv` 的 `rc->p_priv = sp`，system.c:298——新 dispatch 测试 `test_dispatch_privctl_set_sys_fills_guarded_default_mask` 首跑即暴露）。新增 8 测试（kpriv 7 + dispatch 1）；doc 22 §4.6/§4.6.1/§4.7 + doc 25 §4 同步；boot 模板路径残留差异（自位；预授权经回归论证不可观察）记录于 doc 22 §4.6.1 + 已裁决为独立 TODO **D-58** 暂时 defer（2026-09-05，原始记录见文末 OQ-D49-1）。`cargo test -p minix-kernel --lib` → 664 passed, 0 failed, 6 ignored。详 §7.1 D-49 行 |

## 0. 审查基线 — 已确认的良好架构（无需改动）

| 维度 | 现状 | 评价 |
|------|------|------|
| 分层与依赖方向 | `kernel → arch → plat/platform → types/boot/elf`，依赖单向无环 | ✅ 干净 |
| no_std | 全链 `#![no_std]`（boot-shim 部分 cfg 后 std），无 std 泄漏 | ✅ |
| 全局状态安全 | `SyncUnsafeCell<T>` + **sealed trait `BklProtected`**（`bkl_protected_impls!` 宏收编 9 类类型）+ `BklSection` witness | ✅ 达到 typestate 水准（R-01/R-03） |
| syscall 分派 | `enum Syscall` + `try_from` + match；`CurrentArchSyscall` ZST trait 化避免 cfg 扩散（D6） | ✅ 优于 C 的 call_vec[] |
| 硬件抽象 | 16 个细粒度 trait（Paging/Protection/TrapEntry/Exception/Clock/Fpu/Smp/…）+ 每 arch Mock | ✅ 聚合收尾见 §E1 |
| 跨空间拷贝 | `cross_space_copy<D: DirectMapArch>` 泛型核心（vm.rs）+ `cross_space.rs` vmcheck 封装，职责分层 | ✅ |
| 依赖注入 | syscall dispatch 显式传 `&mut ProcessTable` / `&PrivTable`；全局访问 33 裸 + 35 witness | ✅ 混合模式见 §A1 |
| boot 契约 | `minix-boot::KernelInfo`（含 `validate()`）为 kernel/boot-shim 唯一契约 | ✅ |
| 占位符 | `todo!`/`unimplemented!` = **0**；`unsafe impl Sync/Send` 全库仅 4 处真实实现（pt_alloc:46 / lib.rs:1155 / protection.rs:175-176，均有论证） | ✅ 基线（F1 已解决） |
| 平台发现 | `PlatformContext` write-once（AssumeSyncCell）+ `&'static dyn PlatformDesc` | ✅ 论证完整 |

## 0.5 学习 / 概念理解 backlog（与代码改动分离）

本段记录**需要透彻理解但暂不立即动手**的概念 / 知识类任务。与 §1-§5 的"代码改动类建议"区分——这些是给自己未来阅读 §16/§11 时回头查找的索引，不进入 §0.1 的"未解决"或"DEFERRED"统计（它们不是 bug）。

### L1. `happens-before` 与内存序（C 时代背景 → Rust 内存模型） [学习]

**当前认知盲点**（2026-08-31）：仅停留在 C 时代 `volatile` 知识水平，对 Rust `Acquire`/`Release`/`Relaxed` 的形式化含义理解不够深。§3.3 并发协议（[06-proc-init-boot-proc.md:1067-1087](file:///os/notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md#L1067-L1087)）的修复论证正确但深度不够——论证"为什么 `p_rts_flags` 用 `Acquire`、`p_nextready` 用 `Relaxed`"时只给了现象，未给"如果去 BKL 会怎样"的完整推理。

**待深入**：
1. **`happens-before` 是什么**——Rust 借用 C++ 内存模型的形式化定义（与 C `volatile + 程序员约定` 的差异）
2. **单字段次序 vs 跨字段次序**两层保证——`Atomic<T>` 在所有 Ordering 下都防撕裂读 + 防本字段读写乱序（这是 C `volatile` 做不到的），但**只有** `Acquire`/`Release` 才建立跨字段 happens-before
3. **`Acquire` 防止的是 `load` 之后被重排到 `load` 之前**——而非 `load` 本身被重排（这是常见误解）
4. **`Acquire`/`Release` 配对原则**——读侧 `Acquire` 必须配写侧 `Release` 才形成 happens-before 链

**学习入口（按深度递进）**：

| 阶段 | 文档 | 期望收获 |
|---|---|---|
| 第一遍（基础） | [06-proc-init-boot-proc.md §3.3](file:///os/notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md#L1050) 末段"关键反推" | 当前已写——能答"为什么这里用 Relaxed/Acquire" |
| 第二遍（形式化） | [16-smp.md](file:///os/notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md) §BKL 实现 + `BklSection` witness | BKL 内部 `xchg` + `Acquire`/`Release` 屏障的具体实现，对照 §3.3 验证 |
| 第三遍（推演） | [11-scheduling-primitives.md](file:///os/notes/rewrite/fork-syscall-rewrite/01-stage-kernel/11-scheduling-primitives.md) 写作时（**D-52** deferred） | 推演"如果去 BKL、改用 RCU/per-CPU lock，`p_nextready` 的 Relaxed 是否要升 `Acquire`——反推 §3.3 关键反推段的判断标准" |

**为什么是 backlog 而非立即任务**：
- 当前 §3.3 论证对修复 CSK_T 修复 + 调度联动修复的目标已足够
- §16-smp 是当前未充分写的文档，D-52 决策也 deferred——强行深挖会阻塞主线
- 真正需要 happens-before 形式化的地方是 §11 写作 + 未来去 BKL 的重构，本轮只是"埋锚"

**触发条件**（任一出现即开始第二/三遍）：
- 启动 [11-scheduling-primitives.md](file:///os/notes/rewrite/fork-syscall-rewrite/01-stage-kernel/11-scheduling-primitives.md) 写作
- 启动 [16-smp.md](file:///os/notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md) review
- 实际去 BKL 的重构启动

## 0.6 硬件抽象化加固 backlog（CLAUDE.md §"硬件抽象为 trait"）

本段记录**违反 CLAUDE.md 原则 "使用 `#[cfg(target_arch)]` 选行为 ❌ 应 trait 静态分派"**的位置。与 §0.5（学习类）和 §1-§5（代码改动类）区分——这些是**架构级重构**，范围跨多个 crate，需要专门的 review 任务。

CLAUDE.md 原则：
- ✅ **可以用 `#[cfg(target_arch)]` 的位置**：仅在**定义 current* 类型**（如 `qemu_virt.rs`、`minix-platform/src/arch/mod.rs` 的 module 路径选择）
- ❌ **不可以用 `#[cfg(target_arch)]` 的位置**：在代码逻辑中"用 cfg 选行为"——这应当 trait 化
- ⚠️ **字面量汇编硬约束**：`naked_asm!` 内的寄存器名 / `asm!("hlt"/"wfi")` 内的指令字面量——这些**必须**用 cfg 选（因为 Rust 的宏展开要求编译期已知符号），不是"行为选择"是"语法约束"

### B-X. 全项目 `#[cfg(target_arch)]` 行为选择审计与 trait 化重构 [Backlog]

**当前规模**（全 `os/` grep）：**50+ 处** `#[cfg(target_arch)]` 使用。

**两步分类**：

#### 步骤一：剔除合法使用（保留）

| 位置 | 用途 | 分类 |
|---|---|---|
| `libs/minix-platform/src/arch/mod.rs:18-23` | module 路径选择 | ✅ 合法（定义 current） |
| `plat/src/lib.rs:30-71` | `CurrentXxx` 类型选择 | ✅ 合法（定义 current） |
| `libs/minix-platform/src/kind.rs:61` 等 | `Kind` trait 实现选择 | ✅ 合法（trait 实现点） |
| `kernel/src/lib.rs:534/544/552` | `naked_asm!` 内寄存器名（`rbp/rsp/rcx/rdx` / `x0-x29` / `a0-a7`/`s0`） | ⚠️ 字面量汇编硬约束 |
| `kernel/src/lib.rs:636/641` | `asm!("hlt")` / `asm!("wfi")` 指令字面量 | ⚠️ 字面量汇编硬约束 |
| `kernel/src/smp.rs:512` | 注释 | ✅ 合法 |
| `kernel/src/syscall.rs:368/370/372` | `CurrentSyscallArch` 选 trait 实现 + aarch64 fallback 到 `DefaultSyscall` | ✅ 合法（定义 current）**+ 注意**：`L370 target_arch = "arm"` 是有意区分 32-bit ARM（`ArmSyscall`）vs aarch64（fallback 到 `DefaultSyscall`——aarch64 syscall 实现尚未落地）。未来加 aarch64 syscall 时**必须**在 L370 之前插入 `#[cfg(target_arch = "aarch64")] pub type CurrentArchSyscall = Aarch64Syscall;`，否则 fallback 仍生效（什么也不做） |
| `libs/minix-platform/src/device_tree.rs:51-77` 等 | `device_tree` crate 内 arch trait 实现选择 | ✅ 合法（trait 实现点） |

#### 步骤二：列出违规使用（需要重构）

| 位置 | 现状 | 应当改 |
|---|---|---|
| `kernel/src/lib.rs:582-600` | 9 个 `#[cfg(target_arch)]` 给 `ARCH_NAME`/`SP_LABEL`/`PC_LABEL`/`FP_LABEL` 字符串常量赋值 | 抽 `ArchNames` trait 到 `minix_plat`，调用 `CurrentArchNames::arch_name()` 等 |
| `kernel/src/lib.rs:1537/1549/1563` | 3 个 `new_test_interrupt_controller` fn，每个架构一个，用 cfg 选 | 抽 `MockInterruptController` trait 或注册到 `minix_plat::CurrentInterruptController` 同位置（与 `qemu_virt.rs` 对齐） |
| `kernel/src/lib.rs:576` | riscv64 独有"reached!"提示（用 cfg 守门） | 抽到 `ArchNames::boot_reached_msg()` 等 |

#### 待发现位置（建议下一步全 grep）

- `libs/minix-platform/src/global.rs` 有 10+ 处 `#[cfg(target_arch = "x86_64")]`（仅 x86，其他架构）—— 是否合法需进一步核实
- `kernel/src/cross_space.rs` / `kernel/src/vm.rs` 等注释中有提到 `no #[cfg(target_arch)]` 的声明——这些是**已经完成 trait 化**的好引用

**为什么是 backlog 而非立即任务**：
1. 范围跨多个 crate（kernel + arch + libs + boot-shim），需要专门的"硬件抽象化加固"review 任务
2. `naked_asm!` / `asm!` 内的 cfg 是真硬约束，**不能动**——分类后才知道哪些能改
3. 重构期间需要逐个设计 trait 边界（`ArchNames` / `MockInterruptController` 等），不能批量 sed

**触发条件**（任一出现即开始）：
- 启动 [16-smp.md](file:///os/notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md) review（BklSection 已在 §3.3 出现，但完整 trait 实现细节在 16）
- 用户在 review 中再次发现 `#[cfg(target_arch)]` 行为选择
- 添加新 arch 平台时遇到大量 cfg 重复（说明 trait 化迫切）

**当前已埋的 TODO 锚点**（便于重启任务时找到）：
- [kernel/src/lib.rs:582](file:///os/kernel/src/lib.rs#L582) `ARCH_NAME` 字符串常量上方（"Architecture-specific labels for register output"）
- [kernel/src/lib.rs:1539](file:///os/kernel/src/lib.rs#L1539) `new_test_interrupt_controller` 注释下方

## 1. 建议总览

| ID | 层级 | 主题 | 严重度 | 状态 |
|----|------|------|--------|------|
| A1 | kernel | 全局访问器 witness 全覆盖（消除 `unsafe fn` 双轨） | P1 | ☐ |
| A2 | kernel | 全局静态收敛到 `globals.rs`（9 SyncUnsafeCell + 5 Atomic 集中管理） | P2 | ☐ |
| B1 | kernel | BKL guard `mem::forget` 跨函数传递（R-05）→ 显式 transfer API | P1 | ☐ |
| C1 | kernel | 5 个 250-550 行 dispatch 大函数按语义拆分（精确测量） | P2 | ☐ |
| D1 | kernel/全层 | errno newtype 化（消除裸 i32 常量 + 三套错误并存） | P1 | ☐ |
| D2 | kernel | 14 个模块级错误枚举统一 errno 映射 trait | P2 | ☐ |
| D3 | kernel/arch | `Result<(), ()>` 7 处清理（C-D-5 扩展） | P2 | ✅ 已解决（2026-08-14，见 §2 D3 节） |
| E1 | arch | 聚合 trait `Arch`（16 个细粒度 trait → 单点 `CurrentArch`） | P1 | ☐ |
| F1 | arch | `pt_alloc` unsafe 论证模型错位（single-threaded → SMP+BKL） | P1 | ✅ 已解决（2026-08-14，见 §4 F1 节） |
| F2 | arch | `pt_alloc::register` 防重复注册 + Release/Acquire 同步 | P2 | ✅ 已解决（2026-08-14，见 §4 F2 节） |
| G1 | 跨层 | `ProcNr` 双定义（arch `i32` alias vs kernel newtype）→ 上移 minix-types | P1 | ☐ |
| M1 | qemu-tests | 19 个 test-kernel 重复配置 → workspace 依赖 + 模板生成 | P2 | ☐ |
| R1 | arch | PTE 位权威位置（Paging trait 关联常量，Redox rmm `ENTRY_FLAG_*` 同款） | P2 | ☐ |

## 2. Kernel 层建议

### A1. 全局访问器 witness 全覆盖 [P1]

**现状**：同一全局存在**双轨访问器**——`pub unsafe fn proc_table()`（lib.rs:1414，调用者自行保证 BKL）与
`pub fn proc_table_with(&BklSection)`（lib.rs:1426，编译期 witness 证明）。`priv_table`/`irq_manager` 同模式。
lib.rs 共 69 处 unsafe，其中约 1/3 是全局静态访问（boot 期除外）。

**建议**：逐步淘汰 `unsafe fn xxx()` 裸访问器，全部调用点迁移到 `xxx_with(&BklSection)`；
boot 期单线程阶段用专门的 `boot_unchecked` 系列并限定 `#[cfg(any(debug_assertions, feature = "qemu_test"))]` 或以显式 `BootPhase` witness 传递。
最终形态：**"BKL 持有"在类型系统内强制**，消除"忘了持锁就 unsafe"的漏洞面。

**依据**：项目 `notes/rewrite/concepts/capability.md` 的能力理论已部分落地（R-03），本项是收官。
对照 Redox（§6）：其 `CleanLockToken` 由 kmain 一路线程化贯穿所有 syscall（syscall 全链传 token）——
**我们的 `BklSection` 与它同构**，witness 全覆盖正是向 Redox 完全版收敛（Redox 已做到"全链 token"，我们 33/68 调用点仍走裸 unsafe）。

**范围**：裸访问器调用点 **33 处**（`rg -n "proc_table\(\)|priv_table\(\)|irq_manager\(\)"`，排除 `_with`/`_boot_unchecked`；
其中测试路径约 1/4，测试内可用 mock 或显式 `unsafe` + 注释），witness 版已有 35 处调用——双轨接近 1:1，正是收敛时机。

### A2. 全局静态收敛 `globals.rs` [P2]

**现状**：9 个 `SyncUnsafeCell` 静态 + 5 个 `Atomic*`（KERNEL_MAY_ALLOC / FREE_UPPER_IDX / VM_RUNNING / CURRENT_PTPROC_NR / CURRENT_ROOT_PHYS）
散落 lib.rs 的 1104-2101 行区段（约 1000 行），与 kmain 引导序列混在同一文件。

**建议**：抽 `globals.rs` 模块，集中声明 + 全部访问器（`xxx_with` / `boot_unchecked`）+ BklProtected 实现清单；
lib.rs 瘦身为入口 + 引导序列 + re-export。收益：全局状态审计点单一（新增 static 必须过 `bkl_protected_impls!` 的摩擦已有，但可见性集中后更可审计）。

**范围**：纯移动 + 引用修正，`cargo test` 回归（kernel 609 测试）。

### B1. BKL guard `mem::forget` 跨函数传递 → 显式 transfer [P1]

**现状**：smp.rs:577/600 `R-05: forget guard — BKL stays held, released later by caller`——
guard 跨函数用 `mem::forget` 转移，drop 语义失效（RAII 断链）。release 时机完全依赖调用者手写。

**建议**：引入显式转移 API：`impl BklGuard { pub fn transfer(self) -> BklGuard`（内部 `ManuallyDrop`）或
改为传递 `&mut BklSection`（witness 已有）。目标：guard 生命周期的**所有权转移在类型层可见**，
杜绝"forget 了但没人 release"路径。

**依据**：这是 C 的 `bkl_lock`/`bkl_unlock` 手动配对语义的 Rust 化收尾——C 语义保真（D6 注释）是
历史正确决定，但 `mem::forget` 是过渡手段，架构上应收敛为 RAII/所有权转移。
注意：`bkl_is_locked()`（smp.rs:1005）可作为 debug 断言锚点，建议在 `kernel_call_finish` 路径加
`debug_assert!(bkl_is_locked())`。

### A3. KProcess / KPriv `Drop + panic` slot ownership 防御 [Done: 2026-09-02]

**已实施**。`KProcess`（proc.rs）与 `KPriv`（kpriv.rs）均加防御性 `Drop`——占用槽（`SLOT_FREE` 清除 /
`s_proc_nr = Some`）被隐式销毁时 `panic!` fail-fast，同时 `impl Drop` 自动禁止 `Copy`。

**测试落地**：三层豁免夹具（`os/kernel/src/test_helpers.rs`：`TestProcTable` / `TestPrivTable` /
`TestProcArray` / `TestKProc`）承载所有测试局部表/局部进程；`panic = "abort"`（dev/release 双 profile）
下 panic 路径不靠 `#[should_panic]`（abort 无法捕获），由"未豁免占用槽 drop 即 abort"在 CI 承担，
判定谓词 `slot_is_occupied` 有专门单测钉死。验证：全 workspace `cargo test` 绿（kernel 615 单测 + 2 集成）。

**完整设计来源**：见 [`panic-in-drop.md`](./panic-in-drop.md)（三分类法 + 不做之事 + 实施门槛与决策）。
原「现状/建议/触发条件/Backlog」内容已被实施记录取代。

### C1. dispatch 大函数按语义拆分（范围收窄后） [P2]

**精确测量**（按顶层 fn 边界，排除测试模块污染——首次粗筛 14 个"500-850 行"中
9 个是统计到文件尾/测试模块的误报，如 `kernel_mini_notify` 实为 10 行薄包装、`dispatch_readbios` 74 行）。

**真实存在的 250-550 行 dispatch 函数**（均为 C 大 switch 的忠实重写，内部已调 helper）：

| 函数 | 位置 | 行数 | 拆分方向 |
|------|------|------|---------|
| `dispatch_getinfo` | misc.rs:721 | ~550 | 按 info 类别拆（do_getinfo 同款大 switch） |
| `dispatch_privctl` | syscall.rs:1164 | ~418 | 按 privctl 子命令拆 |
| `dispatch_vmctl` | syscall.rs:1751 | ~348 | 按 vmctl 子命令拆 |
| `dispatch_trace` | misc.rs:1239 | ~332 | 按 trace 操作拆 |
| `dispatch_statectl` | syscall_process.rs:732 | ~258 | 按 state 子命令拆 |

**原则**：按 C 函数边界/子步骤拆 helper，**不改行为**；拆后跑 Gate B 行为契约验证。
这是工程优化不是正确性修复——进入本轮 backlog，按文档（doc 29/30/32 对应章节）同步更新。
**方法学教训**：大函数统计必须按顶层 fn 边界精测（本审查首次粗筛 14 项仅 5 项属实）。

## 3. 错误处理统一建议

### D1. errno newtype 化 [P1, 设计决策]

**现状三套并存**：
1. `kernel/src/errno.rs` 裸 `pub const EXXX: i32`（**114 个常量**）
2. `KcallResult`（syscall.rs:198）——好的 enum 但 `Ok(i32)` 内部仍是裸 i32
3. 14 个模块级错误枚举（VmCopyError/CrossSpaceResult/IpcError/CopyError/SchedProcError/MemMapError/CapabilityError/IrqError/UserCopyError/DeliverResult/VmCheckResult/VmCtlResult/KcallResult…）+ 手写映射函数（`sched_proc_error_to_errno` sched.rs:428 等）；直接 `Err(EXXX)` 裸常量写法 26 处

**⚠️ errno 双源实证**（Redox 调研发现，Proposal #12"权威位置"问题第二个实例，同 DEFAULT_HZ）：
- `os/kernel/src/errno.rs:105` `pub const EAGAIN: i32 = 35`
- `os/libs/minix-types/src/types/errno.rs:44` `pub const EAGAIN: i32 = 35` —— **两处独立定义，数值一致但无共享**
- minix-types 另有 `KernelError` enum（ipc/kernel.rs:36）+ `to_errno()` 映射（EAGAIN/ESRCH/EIO/ENOSYS）
- 漂移风险：kernel 侧改 errno 数值，minix-types 不感知（C 源码 errno.h 是唯一 ground truth，但 Rust 侧无强制同步）

**建议**（Redox redox_syscall 同款——共享 ABI crate 承载 `Error{errno}`）：
- `Errno(i32)` newtype 定义在 **minix-types**（`types/errno.rs` 成为唯一权威位置），kernel errno.rs 改为 `pub use minix_types::*` 再导出（兼容现有调用点）
- 各模块错误枚举实现 `impl From<XxxError> for Errno`（或 `fn to_errno(&self) -> Errno` trait）
- `KcallResult::Ok(i32)` 维持（系统调用返回值是 i32 语义），但内部错误路径统一 `Result<T, Errno>`
- 可参考 Redox `mux/demux`（`Ok(v)=>v, Err(e)=>-e.errno as usize`）编码——我们 `to_n()` 已有雏形
- **保留约束**：errno 数值与 Minix3 完全一致（CLAUDE.md 强制）

**对照**：Redox `syscall::Error` enum + `errno()` 映射（`Result<usize, Error>` 贯穿 syscall 层）——
本项目已有 14 个局部枚举，比 Redox 的全局 Error 更贴近 C 语义，只需统一"到 errno 的映射通道"。

**范围**：全 kernel 26 处直接 `Err(EXXX)` + 14 个枚举的映射通道；属设计决策，先 OQ 确认再做。

### D2. errno 映射 trait 统一 [P2]

在 D1 基础上，定义 `trait ToErrno { fn to_errno(&self) -> Errno; }`（或直接 `impl From<X> for Errno`），
消灭手写映射函数（`sched_proc_error_to_errno` sched.rs:428 等 14 行级小映射仍属可接受，但一致性应统一）——
映射逻辑归属错误类型自身。

### D3. `Result<(), ()>` 7 处清理 [P2, C-D-5 扩展] — ✅ 已解决（2026-08-14）

原 C-D-5 记录 4 处，全库实际 **7 处**：
- `kernel/clock.rs:293` `init_profile_clock`
- `kernel/vm.rs:650` + `:651` `enqueue_and_notify`（函数 + send_sig 闭包参数，2 处）
- `arch/arch/boot.rs:211-218` `write_user_register`（trait 方法，:211 已标注 TODO(C-D-5)）
- `arch/arm64/boot.rs:139`、`arch/arm64/clock.rs:89`、`arch/riscv64/boot.rs:124`

建议：`write_user_register` 定义为 `Result<(), Errno>`（trait 签名级修改，三 arch + mock 同步）；
`init_profile_clock`/`enqueue_and_notify` 返回模块级错误枚举。

**解决记录（2026-08-14）**：6/7 处落地**命名错误类型**——
- `ProfileClockError`（`os/arch/src/arch/clock.rs` trait 签名 + x86_64/arm64/riscv64/mock 同步，`Err(ProfileClockError::Unsupported)`）
- `WriteUserRegError`（`os/arch/src/arch/boot.rs` trait 签名 + 三 arch + mock 同步，`BadAddress`/`Protected` 两变体）
- `init_profile_clock` 返回 `Result<(), ProfileClockError>`（`os/kernel/src/clock.rs`）
- `enqueue_and_notify`（`os/kernel/src/vm.rs:645`）按 **R-18 决策**保留 `Result<(), ()>`——单一失败模式 API，理由已文档化，为全库仅剩的 result_unit_err allow
- 验证：172 arch + 610 kernel tests 通过，clippy 干净，`rg "TODO(C-D-5)"` = 0

## 4. Arch 层建议

### E1. 聚合 trait `Arch` [P1, 设计决策]

**现状**：16 个细粒度 trait（`minix_arch::arch::*`），每个 3 arch 实现 + 1 mock。
`minix_arch` lib.rs 318 行中约 2/3 是 re-export + mock cfg 开关。
调用方以 `minix_arch::xxx` 直呼 trait 方法——"当前架构"概念分散。

**建议**：定义聚合 trait：
```rust
pub trait Arch: ProtectionArch + TrapEntryArch + ExceptionArch + ClockArch
    + FpuArch + SmpArch + ArchInit + TimerIrqGate + PostInitArch
    + StacktraceArch + TlbArch + PteWalkArch + DirectMapArch
    + CpuContextArch + SignalContext + Paging {}
pub type CurrentArch = X86_64Arch;  // 单点 cfg 选择（lib.rs 已有模式）
```
调用点 `use minix_arch::{CurrentArch, Arch}` + `<CurrentArch as ClockArch>::xxx`。
收益：架构实例概念显式化（Redox `Arch` trait 同款）；`CurrentArch` 为将来 cfg 切换唯一锚点；
trait 细粒度保留（mock 组合不变）。

**代价**：调用点迁移 ~50-100 处；**先 OQ 确认**（细粒度直呼 vs 聚合——两者可长期并存）。

### F1. `pt_alloc` unsafe 论证模型错位 [P1] — ✅ 已解决（2026-08-14）

**现状**：`arch/pt_alloc.rs:35-40` 用 **"single-threaded event loop model"** 论证
`unsafe impl Sync for PtAllocSlot`——但 arch crate 服务的对象是 **kernel（SMP + BKL）**，
user-space 服务器模型（CLAUDE.md 执行模型 A）不是 arch 层的执行模型。
代码本身安全（fn 指针 write-once at boot / read-only after），**论证错误**——这正是 Pattern #77 类注释漂移的架构级版本。

**建议**：注释改为正确论证：
```
SAFETY: 写入仅发生在 boot 单线程期（kernel lib.rs:184 注册）或 user-space VM 初始化期；
        boot 后为只读。register() 必须发生在任何并发访问之前。
```
同时 `register()` 与首次 `alloc_pt_page()` 之间的可见性用 Release/Acquire 保证（见 F2）。

**解决记录（2026-08-14）**：模块文档 + `PtAllocSlot` SAFETY 注释已改写为 **SMP+BKL write-once-then-read-only**
论证（`os/arch/src/arch/pt_alloc.rs:24-35` 模块文档 + `:45-52` SAFETY），并显式声明"这是 kernel（SMP+BKL）
执行模型，不是单线程服务器模型"；Release/Acquire 可见性见 F2 解决记录。172 arch tests 通过。

### F2. `pt_alloc::register` 防重复注册 + 同步 [P2] — ✅ 已解决（2026-08-14）

**现状**：`register()` 无条件覆写 `PT_ALLOC`（`core::ptr::write` + `PT_REGISTERED.store(true, Relaxed)`）；
`alloc_pt_page()` 用 Relaxed 读。测试间共享该全局（与 minix-vm `PAGE_ALLOC_PTR` 全局状态测试问题同模式）。

**建议**：`debug_assert!(!PT_REGISTERED.load(Relaxed))`（boot 期重复注册即 bug）；
`register()` 的 store 用 Release、`alloc_pt_page()` 的 load 用 Acquire（fn 指针可见性保证）；
文档化"boot 域 + user-space VM 域"双注册契约。

**演进方向（Redox rmm 对照，§6）**：Redox 用 `FrameAllocator` trait + `TheFrameAllocator` 别名注入，
比 fn 指针更可测试（mock 可注入）。~~长期可将 `pt_alloc::register(fn)` 演进为
`pt_alloc::register(impl FrameAllocator)`~~——**已否决（见 `frame.rs` 模块 doc-comment "What this module does not own"）**：
在"kernel 不做长期 PMM"前提下，通用 `FrameAllocator` trait 无长期 owner（VM PMM 在 VM 进程内、`VmBootAllocator` 是一次性），属于 premature abstraction，**明确不引入**。
`pt_alloc` 保持 fn 指针形态；runtime VM 页表页 provider 待 VM 阶段。

**解决记录（2026-08-14）**：
- `register()` 增加 `debug_assert!(!PT_REGISTERED.load(Ordering::Relaxed))`——同域双注册即 boot bug
- store 改 `Ordering::Release`、`alloc_pt_page()` load 改 `Ordering::Acquire`——fn 指针写入对观察者可见（pt_alloc.rs:74-86/:101-105）
- "boot 域 + user-space VM 域"双注册契约已文档化
- trait 化演进方向**已否决**（见 `frame.rs` 模块 doc-comment "What this module does not own"）；172 arch tests 通过

## 5. 跨层建议

### G1. `ProcNr` 双定义统一 → minix-types [P1, 设计决策]

**现状**：`arch/boot.rs:45 pub type ProcNr = i32` vs `kernel/proc.rs:30 pub struct ProcNr(pub i32)`——
arch trait 签名用裸 i32（`build_cpu_context` 等），kernel 调用点被迫 `nr.0` 拆包（lib.rs:823/975
已注释"arch trait takes a plain i32 (arch::boot::ProcNr = i32 alias)"；另有 lib.rs:1842/2074 多处 `.0`）。

**建议**：`ProcNr` newtype 上移 `minix-types`（共享协议层，同 Endpoint/Pid 家族）；
arch trait 签名改用共享 `ProcNr`；消除 `nr.0` 与双定义。符合 CLAUDE.md"共享类型放 minix-types"设计原则
（minix-types lib.rs 已阐明 Microkernel 最小知识原则）。
**注意**：minix-types 是"协议类型"（服务间通信），ProcNr 是内核内部索引——需 OQ 确认归属
（备选：新 `minix-kernel-types` crate，或 arch 依赖 kernel 的 `proc` 类型不可行——循环依赖）。

### M1. test-kernel 配置模板化 [P2]

**现状**：19 个 bootstrap test-kernels 各自维护 Cargo.toml；
2026-08-14 曾批量修复 19+12 个文件的 `[[bin]] test = false` + uefi feature 问题——
**重复配置是结构性病灶**（每次配置演化都要改 19 个文件）。

**建议**：依赖统一收敛到 `[workspace.dependencies]`（已就位）+ 生成脚本
（`tools/gen-test-kernel.sh`：按 target 生成 bin 配置）+ 文档声明"新增 test-kernel 的模板步骤"。
验收：新增一个 test-kernel 只需 1 个新目录 + 1 次脚本调用。

## 6. Redox OS 对照（2026-08-14 调研完成）

> 调研对象：redox-os/kernel master（rmm 0.6 重写后），重点对照与建议的关联。

| Redox 做法 | 我们现状 | 对照结论 |
|-----------|---------|---------|
| **syscall 中心 match + 分类模块**（`syscall/mod.rs` 27 臂 → fs/process/time/futex/debug/usercopy 六文件） | `enum Syscall` + `dispatch_*` → 6 个 syscall_*.rs | ✅ **同构**——方向已正确，无需改动 |
| **usercopy 独立模块**（`UserSlice::ro/wo` 统一用户内存校验） | `UserCopy` trait（ipc.rs:266）+ pte_walk + cross_space | ✅ 同构；`UserSlice` 的"统一校验入口"思想可对照我们 `syscall_copy.rs` 的 copy_struct_from_user 收口 |
| **无内核级 Arch trait**（`#[cfg(target_arch)]` 整体模块切换） | **16 个细粒度 trait + mock** | ✅ **trait 化优于 cfg 换模块**（可测、可文档化）——保留；聚合收尾见 §E1 |
| **rmm crate 的 `Arch` trait 关联常量**（PAGE_SHIFT/PAGE_LEVELS/**ENTRY_FLAG_\*（PTE 位）**/PHYS_OFFSET） | Paging trait 有 `PAGE_SIZE` 常量；PTE 位各 arch 私有 const（x86_64/pte.rs:19 `PTE_PRESENT` 等） | ⚠️ **PTE 位无权威位置**——各 arch 各自定义 → 新增建议 **R1**（见下） |
| **内存管理下沉独立 crate rmm** + `FrameAllocator` trait + `PageMapper`（软件遍历） | pt_alloc fn 指针注册 + Paging trait + pte_walk | ⚠️ **fn 指针 vs trait 注入**——`FrameAllocator` trait 方案**已否决**（见 `frame.rs` 模块 doc-comment "What this module does not own"：无 owner 的抽象不建）；`pt_alloc` 保持 fn 指针；VM 阶段可参考 rmm 的页表核心独立分层 |
| **共享 ABI crate redox_syscall 承载 `Error{errno:i32}`** + `mux/demux`（`Ok(v)=>v, Err(e)=>-e.errno`）+ `Result<T, E=Error>` | kernel errno.rs 114 常量；**minix-types 已部分 import errno**（ipc/kernel.rs `EAGAIN/EIO/ENOSYS/ESRCH` + `to_n()`） | ⚠️ **双源风险实证**——kernel errno.rs 与 minix-types 各自有 errno → 强化 §D1：**Errno newtype 放 minix-types**（Redox 同款共享 ABI 定位），mux/demux 的 `-errno` 编码可参考 |
| **锁序类型 L0-L5 + `CleanLockToken`**（syscall 全链传 token，乱序编译失败） | `BklSection` witness（已实现）+ 33 处裸 unsafe 访问器未收敛 | ⚠️ **我们已有 CleanLockToken 的雏形**——witness 全覆盖（§A1）正是向 Redox 完全版收敛 |
| **全局自旋锁 `CONTEXT_SWITCH_LOCK: AtomicBool`**（每 arch 一份）+ 无 static mut | BKL `AtomicBool` + `SyncUnsafeCell` + sealed `BklProtected` | ✅ 同构且我们的 sealed trait 更严格 |
| **宿主单元测试极少**（19 个），QEMU boot 集成测试为主 + 三交叉构建 CI | **610 单元测试**（mock 三层）+ qemu_test feature | ✅ 单元测试是我们的优势；可补 QEMU boot 冒烟（已在 QEMU backlog） |
| 内核/用户态共享 ABI crate；edition 2024；panic=abort | minix-types 同定位；2024；panic=abort | ✅ 一致 |

### R1. PTE 位权威位置（Paging trait 关联常量） [P2]

**现状**：PTE 位（PRESENT/WRITABLE/USER/…）各 arch 私有 `const` 定义（x86_64/pte.rs:19 起、arm64/riscv64 paging.rs 各自定义），
Paging trait 仅有 `PAGE_SIZE` 常量——跨 arch 无法在统一代码中引用 PTE 位。

**建议**（Redox rmm `Arch::ENTRY_FLAG_*` 同款）：Paging trait 增加关联常量（`const PTE_PRESENT: PageFlags;` 或
const 集合），各 arch 实现时给出本架构位值；PageFlags 位语义（kernel/user/writable/nx）统一枚举化。
收益：pte_walk.rs / cross_space 等跨 arch 代码可统一引用，消除各 arch 重复定义。
**注意**：与 CLAUDE.md"硬件语义不泄漏到 OS 层"一致——PTE 位属 arch 层，在 trait 常量中定义正是抽象化而非泄漏。

### 对照总结

Redox 与我们最大分歧在 Arch 抽象（cfg 换模块 vs trait）与锁（锁序类型系统 vs 单 BKL）——
**两个分歧点我们都占优或同构**；最大共识"共享 ABI crate 承载错误类型"正是 minix-types 定位，
但 errno 双源（kernel errno.rs + minix-types 部分导入）需通过 §D1 统一。

## 7. 文档 01-30 未完成项汇总（2026-08-14 系统收集）

> **收集方法**：对 01-30 全部文档 grep `TODO|FIXME|deferred|WONTFIX|backlog|待实现|未实现|待落地|待补|已知缺口/限制/缺陷|IN_DESIGN` 等标记（~260 处候选）→ 逐条读上下文 → **对关键条目反查 Rust 代码当前状态**（避免把已实现的写成未实现）→ 跨文档去重。
> **排除原则**：已解决的历史记录（如 20 §4.8 全 ✅）、设计语义（如 06 `EntrySpec::DEFERRED` 延后加载）、与 C 现状对齐（如 16 cache 对齐 FIXME）、ARCH 演进差异（如 08 三处 C 步骤差异）不入清单，见 §7.6 排除表。
> **严重度**：沿用文档自标（[P1]/[P2]）；文档未标级的按依赖链重要性定级。

### 7.1 DEFERRED 代码任务（真实未实现，等待依赖落地）

按子系统分组，实现路径 = 解除依赖后按 C 源码补齐。

| # | 子系统 | 条目 | 文档出处 | 依赖 / 阻塞原因 |
|---|--------|------|---------|----------------|
| D-1 | 进程 | `dispatch_exec` cross-space 进程名拷贝（do_exec.c:37） | 17 §4.2/4.8 | **✅ 已解决（2026-08-14 核实）**：`data_copy_vmcheck` 跨空间拷贝 + 截断（syscall_process.rs:250-285，do_exec.c:37-43 语义） |
| D-2 | 进程 | `dispatch_exec` arch_proc_init 设新 IP/SP（do_exec.c:45） | 17 §4.2/4.8 | **✅ 已解决（2026-08-14 核实）**：arch_proc_init 设 IP/SP + 清零部分寄存器（syscall_process.rs:293-300，do_exec.c:45-48 语义） |
| D-3 | 进程 | `dispatch_clear` release_address_space（do_clear.c:35） | 17 §4.4/4.8 | **✅ 已解决（2026-08-14 核实）**：release_address_space（syscall.rs:1075，对应 C memory.c:986-989 单行 `p_cr3_v = NULL`） |
| D-4 | 进程 | `dispatch_clear` clear_endpoint（do_clear.c:49） | 17 §4.4/4.8 | **✅ 已解决（2026-08-14 核实）**：clear_endpoint 完整序列（syscall.rs:1096 + syscall_process.rs:457-458，2026-08-13 Phase 8） |
| D-5 | 进程 | `dispatch_runctl` SMP IPI 路径（do_runctl.c:55-58） | 17 §4.6/4.8 | **✅ 已解决（2026-08-14 核实）**：RC_STOP 远程路径 `smp_state.schedule_stop_proc::<CurrentSmpArch>`（syscall_process.rs:570-575，do_runctl.c:55-58 语义） |
| D-6 | 进程 | `dispatch_exit` mini_notify(sig_mgr)（do_exit.c:21） | 17 §4.3/4.8 | **✅ 已解决（2026-09-05）**：`dispatch_exit` 转调完整 `cause_signal(caller.p_nr, SIGABRT)`（syscall_process.rs:351-364，签名接 proc_table/priv_table，dispatch 表 syscall.rs:482/707 同步）；原 `cause_signal_abort` 删除；管理器通知（SYSTEM 源 mini_notify_core）落地；测试 `test_dispatch_exit_sets_sigabrt_and_notifies_manager`（管理器收到 DELIVERMSG）+ `test_dispatch_exit_returns_no_reply` |
| D-7 | 进程 | `dispatch_statectl` ClearIpcRefs（do_statectl.c:21-25） | 17 §4.7/4.8 | **✅ 已解决（2026-08-14 核实）**：ClearIpcRefs 分支 + clear_endpoint（syscall_process.rs:58/:732，do_statectl.c:21-25 语义） |
| D-8 | syscall | `kernel_call()` wrapper 未实现（trap 入口 wrapper：copy_msg_from_user + p_delivermsg_vir + dispatch + finish） | 13 §6.3 [P1] | 待 trap 入口文档化（14），含 TOCTOU 防护的 copy_msg_from_user |
| D-9 | syscall | `kbill_kcall` 内核计费标记（性能分析用） | 13 §6.4 [P2] | **✅ 已解决（2026-09-06）**：`KBILL_KCALL` 全局（`SyncUnsafeCell<Option<ProcNr>>`，BklProtected 收编 `ProcNr`）+ 两钩子——置位：`kernel_call_dispatch` dispatch 返回后无条件（C system.c:160，拒绝调用也计费；resume 不重置）；消费：context_stop 等价点（`finish_and_restore` 步骤 2 / `idle` 步骤 4）以同一 `tsc_delta`（`decrement_quantum_in_with_delta` 新增返回，kernel 豁免路径也返回 delta——C 消费块是 context_stop 公共尾部）计入 `p_cycles.kcall` 后清位。Rust 在 `bkl_unlock` 前消费（关闭 C :226 先解锁的窗口，单 CPU 语义同）。新增 2 测试（dispatch 置位含拒绝场景 / 消费归属+清位+无标记 no-op）。`cargo test -p minix-kernel` → 673 passed。SMP 迁移方向：CpuLocal（同 D-40 先例），已注释标注 |
| D-10 | 信号 | cause_sig **SELF 路径**（system.c:416-437 自管理进程自通知） | 19 §4.7 | **✅ 已解决（2026-08-14 核实）**：写自身 `s_sig_pending` + `mini_notify_core` 唤醒（syscall_signal.rs:258-278，行号 2026-09-05 漂移修正；SIGKSIGSM=73 超出 SigSet(u64) 位宽，唤醒即 C 语义全部内核动作，无需常量） |
| D-11 | 信号 | cause_sig **致命信号 panic**（system.c:417-432） | 19 §4.7 | **✅ 已解决（2026-09-05）**：`is_lethal`（syscall_signal.rs:101-103，SIGS_IS_LETHAL 六信号）+ SELF 致命子路径（syscall_signal.rs:210-256）——s_bak_sig_mgr 提升（s_sig_mgr←backup、s_bak_sig_mgr←NONE、RTS_UNSET(NO_PRIV)）+ 递归 cause_signal 走外部路径 + 无 backup panic（消息同 C）；差异：省略 C 的 proc_stacktrace（syscall.rs:2295 DIAGCTL 栈路径可复用，见 doc 19 §4.7）；测试 `test_cause_signal_self_lethal_promotes_backup` / `test_cause_signal_self_lethal_no_backup_panics` / `test_is_lethal`（另见 .design/19-design.v2.md） |
| D-12 | 信号 | cause_sig **去重检查**（system.c:439-448） | 19 §4.7 | **✅ 已解决（2026-08-14 核实）**：`was_signaled`（RTS_SIGNALED 判定）+ 无条件 `p_pending.add` 去重（syscall_signal.rs:285-286，行号 2026-09-05 漂移修正） |
| D-13 | 信号 | `sig_delay_done`（system.c:454-464） | 19 §4.7 | **✅ 已解决（2026-09-05）**：`ProcessTable::sig_delay_done`（proc_table.rs，清 `MF_SIG_DELAY` + `cause_signal(SIGSNDELAY)`）实现并双路接线——(a) `process_misc_flags` SC_DEFER 分支（proc.c:379-381 语义：deferred syscall 完成且未阻塞 SEND → 立即结束延迟）；(b) receive 投递路径（proc.c:1082-1083 语义：IpcEngine receive Phase 3 记录 MF_SIG_DELAY sender → `dispatch_ipc` 改持 `&mut ProcessTable` 后统一 `sig_delay_done`）。设计要点：cause_signal 经 `rts_set` 带调度器 dequeue 副作用，故延迟结束通知必须在 ProcessTable 级而非 slice 级。已知限制：SIGSNDELAY=70 超出 `SigSet(u64)` 位宽，信号编号不随 GETKSIG map 送达 PM（与 SIGKSIG 同类），交付的内核行为是唤醒协议（RTS_SIGNALED + mini_notify）；扩宽 SigSet 到 C 的 128-bit 是独立跨切面任务。新增 6 测试（ipc.rs 2 + proc_table.rs 3 + syscall.rs 端到端 1）。doc 19 §1.5/§2.1/§2.3/§4.7/§4.8/§5、doc 12 §4.4/§5.1、doc 13 §1.3/§4.8、doc 17 §4.6 同步。`cargo test -p minix-kernel --lib` → 656 passed, 0 failed, 6 ignored。 |
| D-14 | 信号 | DIAGCTL `send_sig(PM_PROC_NR, SIGKMESS)`（do_diagctl.c:49-56） | 19 §4.7 | 保持 DEFERRED。已核实（2026-08-14）：借用冲突注释现存于 syscall.rs:2286（原引 :1010-1037 行号漂移，已更正）；PM getksig 轮询主用途已实现 |
| **D-57** | 信号 | `proc_stacktrace` 接入 `cause_sig` 致命 SELF panic 路径（system.c:429 差异对齐） | 19 §4.7 / [32-stack-tracing.md](32-stack-tracing.md) | ✅ **已解决（2026-09-05，第二批）**：新增 `os/kernel/src/stacktrace.rs` 模块（`proc_stacktrace(rp: &KProcess)` + `make_read_word` 助手），`is_kernel_task` 分流（Direct Map alias `read_volatile` vs `cross_space_copy`）；syscall_signal.rs 致命无 backup 分支（:251-260）`#[cfg(not(test))]` 守门后 panic 前调用；syscall.rs DIAGCTL STACKTRACE 路径去重改用同一助手（消除原 ~75 行重复代码）；`Cell<*mut u8>` 承载 stack-allocated scratch 缓冲，保持 `Fn` 闭包语义（`StacktraceArch::walk_frames` bound）。**8 测试**：4 运行 + 4 `#[ignore]` 需真硬件（EarlyConsole → COM1 UART → hosted Linux 无 `iopl` SIGSEGV，与 `test_dispatch_diagctl_stacktrace_valid_endpoint_returns_ok` 同类）。doc 19 §4.3/§4.7 同步（移除"省略 proc_stacktrace"标注，差异表更新为"完整已实现"）。`cargo test -p minix-kernel --lib` → 650 passed, 0 failed, 6 ignored。 |
| D-15 | 时钟 | `mini_notify(CLOCK, endpoint)` 到期通知分发（TimerAction::NotifyAlarm 已定义未接线） | 21 附录 B | 保持 DEFERRED。已核实（2026-08-14）：NotifyAlarm 仅 enum 定义（clock.rs:522）+ 测试构造（:1363/:1378），tick 无生产调用方 |
| D-16 | IPC 过滤 | `allow_ipc_filtered_msg`（system.c:803-874，L2 receive 路径偏好过滤） | 23 §4.5/Ch6 [P1] | 12-ipc-core RECEIVE 路径（当前 skeleton）后才消费方；当前 s_ipcf 字段存在但无消费方 |
| D-17 | IPC 过滤 | `may_asynsend_to` self-send 不对称（priv.h:87） | 23 §4.5/Ch6 [P1] | **✅ 已解决（2026-09-06，核实轮）**：语义早已于 2026-09-03 随 D-51 提交落地（ipc.rs:1456 `\|\| p_nr == caller_nr`，与 C priv.h:87 一致；send 路径无此例外保持 priv.h:86 不对称），todo 记录未随更新。本轮补齐不对称测试 ×2（`test_senda_self_target_allowed_without_mask_bit`：空掩码 SELF 表项走 pending 路径 s_asyn_pending 置位 + 表指针保留；`test_senda_other_without_mask_bit_denied`：同空掩码他者表项 per-entry 结果 ECALLDENIED=210 写回）+ doc 23 三处状态更新。`cargo test -p minix-kernel` → 671 passed |
| D-18 | IPC 过滤 | `IPCF_EL_MATCH` 宏链（ipc_filter.h:19-41） | 23 §4.5/Ch6 [P2] | `allow_ipc_filtered_msg` 子逻辑 |
| D-19 | IPC 过滤 | `allow_ipc_filtered_memreq`（system.c:879+，VM 页错误请求过滤） | 23 §4.5/Ch6 [P2] | **✅ 已解决（2026-08-14 核实）**：语义对应 `VmRequestQueue::dequeue_filtered`（vm.rs:588-620，MEMREQ_GET 遍历时按过滤器跳过请求） |
| D-20 | 跨空间 | `VmRequestQueue` 全局链表 + `send_sig(SIGKMEM)` | 24 §4.6 [P1] | 随 SIGSEND 实现落地；当前 suspend_for_vm_with_copy 已设 RTS_VMREQUEST + p_vm_suspend 但未通知 VM |
| D-21 | 跨空间 | `kernel_call_resume()` 恢复路径 | 24 §4.6 [P1] | **✅ 已解决（2026-08-14 核实）**：kernel_call_resume 完整实现（vm.rs:896）+ 2 测试（vm.rs:1303/:1316） |
| D-22 | 跨空间 | `dispatch_vircopy` 集成 `data_copy_vmcheck`（当前走 virtual_copy_vmcheck 未走 VMSUSPEND 路径） | 24 §4.6 [P1] | **✅ 已解决（2026-08-14 核实）**：dispatch_vircopy 走 `cross_space::data_copy_vmcheck`（syscall_copy.rs:220/:344-345，VMSUSPEND 路径）；D-20 的 SIGKMEM 通知接线仍 DEFERRED |
| D-23 | 跨空间 | aarch64/riscv64 PTE walk | 24 §4.6 [P2] | **✅ 已解决（2026-08-14 核实）**：三架构 `PteWalkArch` 完整实现——x86_64/paging.rs、arm64/paging.rs:310（4 级）、riscv64/paging.rs:334（Sv39 3 级） |
| D-24 | 跨空间 | `VmSuspendContext` Map 变体未使用（当前仅 KernelCall + DeliverMsg） | 24 §4.6 [P2] | 随 IPC/message deliver 推进 |
| D-25 | 跨空间 | 部分拷贝进度报告（`CrossSpaceResult::Suspended(fault, usize)` 扩展方向） | 24 §4.6 [P2] | 未来可扩展 |
| D-26 | GETINFO | `GET_PROC`/`GET_PROCTAB`（do_getinfo.c） | 25 附录 | **✅ 已解决（2026-08-14）**：已实现为 `GetInfoRequest::Proc`/`ProcTab`（misc.rs:784/:754），非 C 布局而是 KProcess Rust 语义（rewrite 目标） |
| D-27 | GETINFO | `GET_PRIV`/`GET_PRIVTAB` | 25 附录 | **✅ 已解决（2026-08-14）**：`Priv`/`PrivTab`（misc.rs:893/:791） |
| D-28 | GETINFO | `GET_REGS` | 25 附录 | **✅ 已解决（2026-08-14）**：`Regs`（misc.rs:915） |
| D-29 | GETINFO | 其余：`GET_IMAGE`/`MONPARAMS`/`IRQHOOKS`/`IRQACTIDS`/`IDLETSC` | 25 附录 + §2.1 | **✅ 已解决（2026-08-14）**：`Image`/`MonParams`/`IrqHooks`/`IrqActids`/`IdleTsc`（misc.rs:1135/:1150/:1103/:1010/:1040） |
| D-30 | UPDATE | `swap_memreq`（do_update.c:313-337） | 25 附录 | 🟢 **设计 no-op（2026-08-14 核实）**：swap 时两个进程均不可运行 → swap 无操作（misc.rs:1807 注释 + doc 25；与 W-6 ClearMapCache 同类，见 §7.6） |
| D-31 | SPROF | 数据拷贝（sprof_info + 采样缓冲区 → 用户态） | 25 §4.5 | **✅ 已解决（2026-08-14 核实）**：info + buffer 双重 `data_copy_vmcheck`（misc.rs:2096-2099，do_sprofile.c:117-120 语义） |
| D-32 | SPROF | `clean_seen_flag`（do_sprofile.c:25-31） | 25 附录 | **✅ 已解决（2026-08-14）**：`clean_seen_flag` helper（misc.rs:1980）+ PROF_START（:1993）/ PROF_STOP（:2080）调用（do_sprofile.c:91/:122 语义）+ 测试 test_sprof_start_clears_seen_flags |
| D-33 | 随机数 | x86 RDRAND 熵采集（当前 kernel 侧 no-op stub 匹配 C i386/earm） | 25 §4.7 D3 | 应在 os/arch/src/x86_64/ 实现（当前 deferred）；实际熵采集由用户态 random 驱动 |
| D-34 | VM | `kinfo.mmap_size` + `mem_high_phys` 更新（08 委派，C add_memmap 更新，Rust KernelInfo immutable） | 09 §6.1 + 08 §4 | VM direct map 实际大小 + 最高物理地址在 boot 阶段确定后，dispatch_vmctl 补充分支或独立 SYS_GETINFO 路径 |
| D-35 | VM | `VmInhibitSet` SMP IPI（dispatch_vmctl，RTS_SET(VMINHIBIT)） | 09 §4 | **部分实现（2026-08-14 核实）**：本地路径已实现（syscall.rs:1883-1903 RTS_SET(VMINHIBIT)）；SMP IPI 远程路径待实现 |
| D-36 | SMP | `smp_init`（ACPI/MADT 表解析 + `SmpArch::boot_ap`） | 16 §4.14 | 16 文档自身 DEFERRED |
| D-37 | SMP | `boot_lock`（smp.c:28） | 16 §4.14 | 随 smp_init（D-36）一并实现 |
| D-38 | SMP | **BKL 接入 4 处**：exception_dispatcher::handle 需 bkl_lock() / kmain switch_to_user 前获取 / switch_to_user 调度循环前 bkl_unlock / kernel_call_resume | 16 §4.13 | ②③ **✅ 已解决（2026-08-14）**：lib.rs:1956 bkl_lock（switch_to_user 前）+ lib.rs:2167 bkl_unlock（调度循环前）；①④ 仍 DEFERRED（SMP 异常路径） |
| D-39 | SMP | x86_64 `init_ap` panic 占位（protection.rs:368-370） | 16 §5.3 [P1] | AP 启动路径；占位触发后无测试（→ 见 T-2） |
| D-40 | SMP | ptproc per-CPU 语义（arch 层 `PostInitArch` 已删除；kernel 级 `CURRENT_PTPROC_NR` 全局当前承担，SMP 落地时迁移 `CpuLocal::ptproc`） | 16 §5.3 [P1] | SMP 落地后 per-CPU ptproc 跟踪（见 16 §D8） |
| D-41 | 平台 | `PLATFORM` AssumeSyncCell SMP 替换（AP_STARTUP 并发访问时替换为 Mutex/Atomic） | 04 §3.3 TODO [P1] | SMP 就绪后失效；由 16-smp 在 AP bring-up 前完成 |
| D-42 | return-path | `TrapReturnArch` trait 定义 + asm impl（iretq/eret/sret + GP 寄存器恢复，trap_return.rs 未创建） | 10 §4.1 + 14 §4.8 + 03 §4.3 | 待 doc 10 switch_to_user 完整调度循环落地；拟议签名 `trait TrapReturnArch: ExceptionArch { type RegisterFile; unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> ! }` |
| D-43 | 调度 | `SC_TRACE` → `cause_sig(SIGTRAP)`（proc.c:392-398，调度循环追踪信号） | 10 §4.2 | **✅ 已解决（2026-09-05，第五批）**：`process_misc_flags` `SC_TRACE` 分支清 `MF_SC_TRACE\|MF_SC_ACTIVE` 后就地 `cause_signal(SIGTRAP)`（proc_table.rs，借用结构同 `sig_delay_done` 先例）；不 `break`、落入循环尾可运行性复查——外部管理 → `RTS_SIGNALED` → 返回 `false` 重调度（C proc.c:413 `goto not_runnable_pick_new`），自管理（SELF 非致命路径）仅唤醒、标志已清循环自然退出，两种管理形态控制流均与 C 吻合。新增 2 测试（`sc_trace_causes_sigtrap` / `sc_trace_without_active_no_signal`，后者钉 C 的 break 分支）。设计决策（三个候选落点对比 + Linux `force_sig_fault` / Redox EFAULT-vs-SIGSEGV / 微内核机制-策略对照）见 doc 10 §3.3；doc 19 §4.7 表已加行 |
| D-44 | 调度 | `vm_suspend(VMS_PAGEFAULT)` 路由（delivermsg 首错分支，proc.c:281-282） | 10 §4.2 | **保持 DEFERRED（2026-09-05 依赖重核）**：阻塞于 D-20 的 VM 通知链（`SIGKMEM` 接线）——只挂起不通知会让进程永睡 `RTS_VMREQUEST`，半截挂起（死锁）比现状（进程继续跑、二次失败吃 SIGSEGV，D-45 已落地）更糟；边界由 `test_process_misc_flags_delivermsg_first_fault_no_signal` 钉住 |
| D-45 | 调度 | `cause_sig(SIGSEGV)` 路由（delivermsg 第二次拷贝失败/越界，proc.c:271-278） | 10 §4.2 | **✅ 已解决（2026-09-05，第五批）**：`process_misc_flags` 的 `DeliverResult::Segfault` 分支就地 `cause_signal(SIGSEGV)`（外部路径置 `RTS_SIGNALED\|SIG_PENDING` → 复查返回 `false`，与 C proc.c:413 等价）；信号路由在消费侧而非 `ipc::delivermsg` 内——后者被 FIX-20 刻意约束为 `&mut KProcess` + `&dyn UserCopy` 无表访问，塞回表访问是架构倒退（doc 10 §3.3 方案三否决论证）；C 的 WARNING printf（proc.c:273-277）以 `#[cfg(not(test))]` + EarlyConsole 保留（hosted test 无 `iopl`，同 `proc_stacktrace` gate 理由）。新增 2 测试（`delivermsg_segfault_causes_sigsegv`：pending 位 + SIGNALED + 管理器 notify + 返回 false；`delivermsg_first_fault_no_signal`：钉住 D-44 边界——首错不升信号）。**`PageFault` 分支（D-44）保持 DEFERRED**：阻塞于 D-20 VM 通知链，半截挂起（置 `RTS_VMREQUEST` 无人唤醒）比现状更糟，已论证记录 |
| D-46 | 中断 | Timer IRQ 链注册 dummy handler（lib.rs:1901-1921） | 08 §4 | 待 trap entry 连接 `IrqManager::dispatch`（Step 1.5.7） |
| D-47 | 调试 | `util_stacktrace()` 不展开栈 | 27 §6.3 | **✅ 已解决（2026-09-06）**：`os/kernel/src/stacktrace.rs::util_stacktrace`——内核自回溯（无进程上下文，C libsys/stacktrace.c:17-37 `get_bp()` 语义）。三层落地：(a) `StacktraceArch::current_frame_pointer() -> Option<u64>`（默认 None；x86_64 `asm!("mov {}, rbp")` 覆写；aarch64/riscv64 默认，`[ARCH: scope]` 标注——C 同为 i386 only）；(b) `walk_frames_from(read_word, emit, fp, already_emitted)`：帧链循环自 `walk_frames` 抽出共享，`already_emitted` 保持 `MAX_STACK_FRAMES` 总发射上限契约（caps 测试钉住）；(c) 内核 Direct Map 直读 helper `kernel_direct_read_word` 自 `make_read_word` 抽取共享 + `util_stacktrace` 组装。panic 路径接线属 D-48（轮 4）。测试：arch `test_current_frame_pointer_plausible`/`test_walk_frames_from_shared_loop`（合成地址链，hosted 安全）+ kernel `util_stacktrace_walks_current_stack_without_panic`（#[ignore] 需真硬件，同 proc_stacktrace gate）。`cargo test -p minix-arch` → 207 passed；`-p minix-kernel` → 673 passed + 7 ignored |
| D-48 | panic | panic handler 增强：消息输出 / CPU 号 / `minix_shutdown(0)` / 栈展开 | 27 §6.3 | **部分解决（2026-09-06，消息+CPU 号+栈展开三项落地）**：minix-rt 的 `#[panic_handler]` 支持诊断 hook 委托（A-8 step 2）——hook 槽位于 `minix-types::diagnostic`（`PanicDiagnosticHook = fn(&str)` + set/run/get；依赖方向 kernel → minix-types ← minix-rt，kernel 不依赖 minix-rt：其 `_start`/crt0 与 kernel 测试链接冲突）；kernel 注册 `kernel_panic_diagnostic` 渲染器（bsp_finish_booting Step -1 `register_panic_diagnostic`）：`kernel panic: ` + 消息 + `kernel on CPU %d: `（BSP 常量）+ `util_stacktrace()`（D-47），C utility.c:30-39 对齐；未注册回退 stage-1 SpinSink。minix-rt 加 opt-in `panic-handler` feature（原 handler 无消费者，行为保持；E0152 经 hello-boot 构建验证）。**`minix_shutdown(0)` 保持 DEFERRED**（零 Rust 基础，halt-loop 即终态）。测试：minix-rt `hook_set_run_clear_round_trip`、kernel `test_register_panic_diagnostic_sets_hook`（注册后清理防 hosted UART SIGSEGV）。`cargo test` → kernel 674 + rt 59 + types 168 passed |
| D-49 | 特权 | `set_sendto_bit`（system.c:307-330 运行时设置 s_ipc_to 位） | 22 §4.6 | **✅ 已解决（2026-09-05，第四批）**：`PrivTable::set_sendto_bit` / `unset_sendto_bit` / `fill_sendto_mask` 三原语 + `PrivTable::update_priv` 组合入口（kpriv.rs）——守卫（未关联 slot / 自身 → 降级为清己位）+ 回执对称（RECEIVE-only 目标跳过，`TrapMask::allows_more_than_receive`）+ fill 撤销修复（清位同步清别家回执位）。`IpcMask` 补 `set_bit`/`unset_bit`/`has_bit`。三处接线：SET_SYS 默认掩码（`fill_sendto_mask(ALL)` 替代裸 `IpcMask::ALL`）、UPDATE_SYS（`update_priv` 替代 `update_from_request` 裸拷贝）、`dispatch_update` 掩码继承（逐位 `set_sendto_bit` + 每轮迭代重读 src 掩码，对齐 C do_update.c:107-112 活读取；原 union 合并）。`update_from_request` 拆分为私有 `apply_fields_from_request` + 命名错误 `PrivUpdateError`（BadIrqCount/BadIoRange/BadMemRange/NoSuchSlot，D3 方向）。**顺带 P0 修复**：SET_SYS 成功后 `p.priv_id` 未回链（C `get_priv` 的 `rc->p_priv = sp`，system.c:298；Rust `PrivTable::get_priv` 只达 priv 侧），新 dispatch 测试首跑暴露。新增 8 测试（kpriv 7：守卫/对称/撤销修复/guard-case 往返位不动/update_priv Err 路径/wire decode 重构 + dispatch 1：SET_SYS 端到端默认掩码），`cargo test -p minix-kernel --lib` → 664 passed, 0 failed, 6 ignored。doc 22 §4.6 重命名 + §4.6.1 新增（设计讨论：不变量/落点选择/seL4·Redox·Linux 对照/boot 模板残留差异）+ §4.7 行同步；doc 25 §4 两处掩码继承描述同步。**Boot 残留差异**（有意不动，已裁决为独立 TODO **D-58**（§7.1）暂时 defer（2026-09-05）；原始 Open Question 记录保留于文末 **OQ-D49-1**）：CapabilityTemplate 的 `IpcMask::ALL` 含自位——boot 服务 SEND 自身 endpoint 时 C 在掩码层拒绝（`ECALLDENIED`，proc.c:536-541）而 Rust 放行；候选差异"预授权未绑定 slot"经回归论证不可观察（详见 OQ-D49-1 + doc 22 §4.6.1）。 |
| D-50 | 架构清理 | `set_ptproc`/`PostInitArch`/`MemoryInitArch`/`FreePdeSlots` 待废弃（createpde 已被 Direct Map 取代） | 07 §1.4 TODO-07-1 | ✅ **已修复（2026-08-17，Action Item #1）**：`init_post_and_memory` 重构为确认就绪 + 相关类型链/statics/测试全部删除 |
| D-51 | IPC 协议 | `PrivUpdateRequest` 字段顺序全局重构（按读写时机 / 锁粒度 / 协议语义切，让 14 字段贴近 KPriv 8 子结构分组语义） | 06 §3.10 | **✅ 已解决（2026-09-03）**：字段序冻结为 KPriv 8 子结构镜像（去 `PrivRuntime`）：`s_id / s_flags / s_init_flags / s_sig_mgr / s_bak_sig_mgr / s_trap_mask / s_ipc_to / s_k_call_mask / s_nr_io_range / s_io_tab / s_nr_irq / s_irq_tab / s_nr_mem_range / s_mem_tab`——s_id 归位 Identity 首；I/O 组在 IRQ 组前（对齐 `PrivIo` 与 C priv.h 惯例）；资源组计数在表前；signal manager 在 Signals 位、init 贴近 s_flags。三处同步落地：(a) `kpriv.rs` `PrivUpdateRequest` + `new()`（`TODO(backlog)` 锚点移除，改为冻结序说明）；(b) RS `privilege.rs` `Privilege`/`vacant()`/`boot_priv()` 同序镜像；(c) `data_copy` 大小由内核侧 `size_of::<PrivUpdateRequest>()` 统一计算（repr(C) 布局随新序自动一致，RS 无独立 size 源）。文档同步：06 §3.10、22 §4.2（协议结构字段序注 + KPriv 片段 flags/init 顺序对齐代码）、03-rs §3.2/§4.2。`cargo test -p minix-kernel -p minix-rs` 全过。 |
| D-52 | 调度 | `sched_proc` 裸 `set/clear(NO_QUANTUM)` 是否对齐 C 的 `RTS_SET/RTS_UNSET` dequeue/enqueue 语义（二选一？） | 06 §3.3 / 11（待写） | **deferred 到 11-scheduling-primitives review（2026-08-31 标记，用户决策）**：当前 `sched_proc`（sched.rs:321-411）拿 `&mut KProcess` 只能裸改 RTS 位（[sched.rs:366/408](file:///os/kernel/src/sched.rs#L366)），绕过了 `rts_set/rts_unset` 的 enqueue/dequeue 封装（proc_table.rs:282-316）——依赖 syscall 出口统一 pick_proc 重调度。C 的 `RTS_SET/RTS_UNSET(RTS_NO_QUANTUM)`（system.c:674/697）宏会立即 dequeue/enqueue。**决策待定**：完全对齐 C（改 `sched_proc` 签名为持有 `&mut ProcessTable` 并调用封装）vs 在更高层对齐（保留出口重调度模型，文档标注差异）。改 priority 后 runqueue 位置不立即重排是当前模型与 C 的实际行为差。已同步在 `06-proc-init-boot-proc.md §3.3` 写侧代码证据中标注该差异与 deferred 决策。 |
| D-53 | boot / 用户态链路 | `cpu_identify()`（main.c:45 → i386 arch_system.c:212，CPUID 填 `cpu_info[CONFIG_MAX_CPUS]`：vendor/family/model/stepping/freq/flags，archtypes.h:39-46）未移植；Rust `GET_CPUINFO` 分支（misc.rs:1001-1015）返回缩减 `CpuInfoEntry`（仅 cpu_id 实值），布局与 C `struct cpu_info` 不一致，注释引用 `type.h:146-159` 为错误出处（该处实为 boot_image；真实类型为各 arch archtypes.h 的 `struct cpu_info`） | 08 §4 / 06 §4.8 | **✅ 已解决（2026-09-04）**：boot 期 per-CPU 身份探测 + `CpuInfoEntry` 布局对齐 + misc.rs 注释出处修正全部落地。三层实现：(a) **arch 探测** `os/arch/src/arch/cpu_identity.rs`——`CpuIdentity` enum（X86/Arm/Riscv 每 ISA 一个变体，C 各 arch 异形 struct 的类型化）+ `CpuIdentityArch` trait + `CurrentCpuIdentity` alias（x86 CPUID leaves 0/1；aarch64 `MIDR_EL1`；riscv64 SBI `mvendorid/marchid/mimpid`）。x86 含 **MINIX3 BUG 修复**：C arch_system.c:239 把 ext-model 合并条件误写在 base model 上，对 2007 后 family-6 CPU（Penryn 起）截断 model（Nehalem 0x106E0→0xE 而非 0x1E）；Rust 按 SDM 以 family∈{0xF,0x6} 为条件（`// MINIX3 BUG:` 标注 + `decode_signature` 4 单测）；C 的 `max_leaf==0` 古董守卫（486 时代）按"只支持现代硬件"原则不移植；(b) **kernel 表**——`CPU_INFO: SyncUnsafeCell<CpuInfoTable>`（smp.rs，`[Option<CpuIdentity>; MAX_CPUS]`，纳入 `BklProtected` 审批列表），`smp::cpu_identify()` 挂 `bsp_finish_booting` Step 0（与 C 同位同序 main.c:45；AP 路径随 16-smp.md SMP bring-up 落地）；(c) **GET_CPUINFO 全记录**——misc.rs `CpuInfoEntry` 重排为 C i386 `struct cpu_info` repr(C) 16 字节（vendor=INTEL 0/AMD 2/UNKNOWN 0xff，archconst.h:134-136；freq=0——TSC 校准回填无 Rust 读者，有意保留缺口；非 x86 身份 wire 上为 UNKNOWN+全零，类型化身份留 `smp::cpu_identity`），整体拷出对齐 do_getinfo.c:76-80，注释出处修正为 archtypes.h:39-46。设计说明见 08 §4.6 差异表；新增 8 测试（smp.rs 1 + misc.rs 3 + arch decode_signature 4），`cargo test -p minix-kernel` 629 + `cargo test -p minix-arch` 201 通过，x86_64/aarch64/riscv64 三目标 `cargo check --no-default-features` 通过。 |
| D-58 | 特权 / boot | boot 能力模板 sendto 掩码残留差异：`CapabilityTemplate::Vm/RootService` 的 `IpcMask::ALL` 含自位——boot 服务 SEND 自身 endpoint 时 C 在掩码层拒绝（`ECALLDENIED`，system.c:316-319 + main.c:244 + proc.c:536-541 经 `may_send_to`）而 Rust 放行；"预授权未绑定 slot"候选差异经回归论证不可观察 | 22 §4.6.1 / OQ-D49-1 | **⏸ deferred（2026-09-05 用户决策"暂时 defer"，无触发条件）**：选项 A 保留（模板 correct-by-construction 最简，自 SEND 病态场景由既有死锁语义暴露）/ B `grant_capability` 后跑一次 `fill_sendto_mask(priv_id, 模板掩码)`（与 C boot fill 终态完全一致，约 1 行 + boot 能力测试更新）；AI 倾向 B（"自位恒清"应为全域不变量，模板路径不应是唯一例外），但触及 doc 06 §3.2 / doc 22 §3 D3 文档化 boot 设计。原始论证全文见文末 OQ-D49-1；runtime 路径已由 D-49 完全对齐 C，本项仅为 boot 模板窄残留 |

### 7.2 WONTFIX 设计排除清单（有 rationale，非缺口）

| # | 文档 | 排除内容 | 理由（文档出处） |
|---|------|---------|----------------|
| W-1 | 26 | NMI watchdog 全部（lockup_check / nmi_watchdog_* / arch_watchdog_* / watchdog_local_timer_ticks / struct arch_watchdog，9 函数 + 3 arch 函数 + 1 struct） | 架构覆盖不全 + 默认关闭 + 依赖硬件 PMU + sprofile 已替代（26 §3.1） |
| W-2 | 28 | usermapped_data 全部 | 64-bit 架构无此机制（28 §4.2） |
| W-3 | 29 | IPC 消息跟踪 + IPC 统计 + 相关 hook（12 函数 + 3 宏：hook_ipc_* / printproc / printparam / printstats / sortstats / statmsg / namematch / print_proc_depends / print_proc_recursive / IPCPROCS / KERNELIPC / PRINTSLOTS） | D2（panic 路径风险）/ D3 / D4：由 log crate + 外部工具替代（29 §3.2-3.4） |
| W-4 | 30 | `nmi_sprofile_handler`（profile.c:128） | 64-bit 无 NMI 子系统（30 §4.4，同 26） |
| W-5 | 25 | SPROF `PROF_NMI` 路径 | NMI 子系统超范围（25 附录，同 26） |
| W-6 | 09 | `ClearMapCache` → Ok(0) no-op | 64-bit Direct Map 无 cache table，匹配 C 无 cache table 架构的 no-op（09 §4.5） |
| W-7 | 27 | kmess 缓冲体系（kmess_buf / km_buf 双缓冲 / END_OF_KMESS / send_diag_sig / SIGKMESS / DIAGCTL_CODE_REGISTER） | EarlyConsole 直接输出 + log 后端替代（27 §6.2，ARCH 演进） |
| W-8 | 25 | `swap_memreq`（do_update.c:313-337，D-30） | 🟢 设计 no-op（2026-08-14 核实）：当前 vmrequest 链无入队路径（D-20 DEFERRED）→ 链恒空 → swap 无操作（misc.rs:1807 注释 + doc 25，与 W-6 同类）；**D-20 落地时需重审** |

### 7.3 待补充测试清单

| # | 文档 | 测试项 | 依赖 |
|---|------|--------|------|
| T-1 | 19 §5.2 | 10 个行为测试（cause_signal sets_pending / dedup / self_path / lethal_panic / getksig / endksig ×2 / sigsend vmsuspend / sigreturn vmsuspend / is_lethal） | D-10..D-13 落地 + KProcess::cause_signal 重构 |
| T-2 | 16 §5.2 | 7 个 SMP 测试（schedule_sync / schedule_stop_proc ×2 / sched_handler_full_save_ctx / ipi_sched_handler / ipi_halt / migrate_proc） | 多 CPU 模拟环境 |
| T-3 | 16 §5.3 | init/load 顺序约束测试（init_proc_and_boot → init_post_and_memory 顺序违反应 panic）[P1] | 原 todo §6.3 转移 |
| T-4 | 16 §5.3 | init_ap 路径验证（x86_64 panic 占位 protection.rs:370 触发后无测试）[P1] | 原 todo §6.4 转移 |
| T-5 | 16 §5.3 | QEMU GDB 脚本集成 CI [P2] | 原 todo §6.8 转移（当前手动脚本） |
| T-6 | 21 §5.2 | 8 个行为测试（STIME/SETTIME/SETALARM/VTIMER 非 EPERM 路径） | D5 参数传递使 mock 可注入 |
| T-7 | 24 §5.2 | ✅ 队列测试已落地 6 个（`vm_request_queue_new_is_empty` :1100 / `enqueue_returns_was_empty` :1106 / `dequeue_filtered_empty` :1118 / `dequeue_filtered_returns_first_match` :1126 / `remove` :1136 / `memreq_get_dequeues_and_sets_fetched` :1368，D-21 同批）；剩余 5 个（data_copy_vmcheck ×3 / zero_byte_copy / self_replacement） | 剩余项依赖 D-20 SIGKMEM 接线 |
| T-8 | 18 §5.2 | 8 个 E2E（verify_grant / vm_lookup / vm_memset / 跨进程 PTE walk 等，原 DEFERRED 已实现，转端到端目标） | QEMU 或 mock grant 表构造真实跨进程场景 |
| T-9 | 20 §5.2 | IRQCTL 3 个（setpolicy_full / rmpolicy_owner / enable_disable）+ vdevio 对齐 panic + readbios copy（后两者需 QEMU pte_walk/copy_to_user） | IrqManager 接入 KernelState |
| T-10 | 01 §5 | QEMU + OpenSBI + U-Boot riscv64 真实启动链集成测试（fatload kernel ELF → OpenSBI 跳转 → 串口捕获 TEST_RESULT）[P2] | QEMU 镜像 + U-Boot 工具链 + 串口监控脚本 |
| T-11 | 29 §5.3 | runqueues_ok_cpu 集成测试（构造 SmpState + ProcessTable mock） | boot_integration |
| T-12 | 17 §5.2 | §5.2 待补测试（DEFERRED 函数实现后） | D-1..D-7 落地 |

### 7.4 文档任务 / 改进方向

| # | 条目 | 文档出处 | 说明 |
|---|------|---------|------|
| I-1 | kernel 独立 ELF 构建（build.rs + link.ld 链接为独立 ELF binary） | 02 §4 + 01 §5.2 | 三架构 link.ld 已就绪，构建系统未接入；当前 rlib 测试路径是合理简化，生产路径后续工作 |
| I-2 | `release[]`/`version[]` 未实现 | 01 §2 | 保持 DEFERRED。已核实（2026-08-14）：C 侧仅 main.c:432-433 赋值且无消费方（banner 直打 OS_RELEASE，main.c:344）；Rust 侧无消费方（KernelInfo 无此字段；banner 硬编码 lib.rs:1831；MINIX_KERNINFO IPC 未实现 ipc.rs:1335）→ 待 MINIX_KERNINFO 落地时实现 |
| I-3 | riscv64 QEMU `-kernel` 场景未完成 ELF 装载 + 高半核切换（临时妥协） | 01 §5 + §4.5 TODO | 生产路径三架构统一高半核；测试场景暴露的临时妥协 |
| I-4 | 01 §5 ↔ 02 §5.1 测试对照表跨文档去重 | 01 §5 TODO [P2] | **✅ 已解决（2026-08-14）**：02 §5.1 测试归属声明已落地（L901，"hello-boot 等四类归 01 §5.2，本节仅 test-higher-half，合计 15/15"）+ 01 交叉引用（L1689）；01 残留 TODO 标记已清理为已解决注 |
| I-5 | ACPI RSDP 搜索与表解析 | 05 §3 | QEMU virt 暂不依赖；支持物理机时需实现 |
| I-6 | `bill_ptr` 完整联动 | 11 §4 [已知缺口] | 保持 DEFERRED。已复核（2026-08-14）：调度循环仍为 placeholder（lib.rs:2187-2189 `loop { spin_loop() }`），bill_ptr 调度期联动确未接线 |
| I-7 | `MF_REPLY_PEND` typestate 演进评估 | 12 §3.11 TODO | 当前保留标志位（决策已定），未来评估 typestate（`SendRec<Sending> → SendRec<Receiving>`） |
| I-8 | errno newtype（模式 16 裸整数） | 13 §6.5 [P2] | **与 §D1 同项**（架构建议），doc 13 独立提出 |
| I-9 | D9: `s_ipcf`/`s_stack_guard` 裸 usize → `NonNull<T>` | 22 §3 [P2] | 当前对齐 C 裸指针语义（Option<usize>），redesign 阶段引入 |
| I-10 | D10: `PrivId`/`SysId` type alias → newtype | 22 §3 [P2] | 改 newtype 需更新所有 callsite，影响面大 |
| I-11 | BIOS 启动 / 实模式 / Multiboot/GRUB legacy 路径未覆盖 | 04 §2 + 01 §3.1 | 范围声明（当前仅 UEFI x86-64/aarch64 + OpenSBI+U-Boot riscv64），非缺口 |
| I-12 | BKL guard RAII 重构建议 | 13 §6.2 | **已被 D6 明确拒绝**（BKL 语义需显式 release/reacquire 围绕阻塞操作）；§B1 的显式 transfer API 是不同方案，待 OQ |
| I-13 | `InterruptController` trait C-Rust 不对称分析 + `Send + Sync` 真实动机（doc 05 §3.3 缺文） | 05 §3.3 | 见下方 §7.4.1 详细背景与建议 |
| I-14 | 启动主线文档 06/09/10/16 范围声明 vs 代码时序系统性错位（architecture-wide 调整的预登记） | 06/08/09/10/16 | 见下方 §7.4.2 详细背景与建议 |

### 7.4.1 [I-14] 启动主线文档 06/08/09/10/16 范围声明 vs 代码时序系统性错位（2026-09-04）

> **状态：📌 P1 [doc] [architecture] 结构性错位 — 留作大调整专项预登记（用户裁决：追加记入，本 session 不动手）**

**背景**：本次回归 review（2026-09-04，fix #9 见 [08-system-init-boot-finish.md:33](08-system-init-boot-finish.md) 作者自注 "TODO，看起来 06,07,08 的时序并不正确"）触发的子。代码实读 `os/kernel/src/lib.rs` 实际 kmain 阶段顺序：

```
T0 init_protection              (lib.rs:744)
T1 init_clock_and_interrupts    (lib.rs:780)
T2 SMP_STATE = SmpState::new_single_cpu()   (lib.rs:559, 早于一切 init)
T3 init_proc_and_boot           (lib.rs:879)
T4 init_post_and_memory         (lib.rs:1271)
T5 bsp_finish_booting           (lib.rs:1948, 含 smp_state 参数)
T6 switch_to_user               (lib.rs:2757, 完整五阶段调度循环；
                                 早期草案的 apply_boot_cpu_contexts 钩子已删除——
                                 trap frame 改为每次分派前从 cpu_context 重建，
                                 见 10-switch-to-user.md §3.2)
```

**2026-09-04 更新（10 号文档完整实现落地后的前提核销）**：上表三项前提已失效——

| 原条目 | 失效原因 | 当前证据 |
|--------|---------|---------|
| [10 范围声明](10-switch-to-user.md) "自承 stub"（原 L177/L251） | 10 已完整重写：五阶段调度循环 + idle + 地址空间切换 + TrapReturnArch 终局分派全部落地，文档不再含"占位 stub"表述 | `rg "占位 stub" 10-switch-to-user.md` → 0 hits；`rg "fn switch_to_user" os/kernel/src/lib.rs` → lib.rs:2757 |
| [06 §3.5/§3.12](06-proc-init-boot-proc.md) 引用 `apply_boot_cpu_contexts` | 该钩子已从代码删除；doc 06 §3.12 伪代码与叙述同步修订（trap frame 分派时重建，lib.rs:2689） | `rg "apply_boot_cpu_contexts" os/ notes/` → 0 hits |
| [08 §1.1/§1.2](08-system-init-boot-finish.md) T6 "占位 stub" 五处 | doc 08 同步更新：T6 状态改为"已实现"，§3.4 的类型/行为一致性说明同步修订 | `rg "占位 stub" 08-system-init-boot-finish.md` → 0 hits |

行号快照（lib.rs，2026-09-04）：`switch_to_user` = L2757；阶段函数 `set_active_root_tracked` = L2324、`pick_and_bill` = L2346、`requeue_if_preempted` = L2392、`idle` = L2453、`restart_local_timer` = L2526、`switch_address_space` = L2558、`finish_and_restore` = L2624。

**问题清单**（不属本 session 改动引入——`06-todo.md:9` 作者执行记录已自带注解）。
> **阅读提示（2026-09-04）**：下表是写于 10 号文档完整实现**之前**的审计记录，行号与结论均已过时（其中 06/08/10 三行已由上表核销，11-15 行随本次实现落地，16 行未动）。保留原文仅作审计追溯，请勿据其定位当前代码。

| 文档 | 偏差类型 | 具体错位 | 证据 |
|------|---------|---------|------|
| [06 §3.5/§3.12](06-proc-init-boot-proc.md) | 范围溢出 | 11 处引用 `apply_boot_cpu_contexts`/`apply_to_trap_frame`/"首次调度"——这些实现在 10 范围（lib.rs:2348 `apply_boot_cpu_contexts`），非阶段 C 范畴 | `rg "apply_boot_cpu_contexts\|apply_to_trap_frame\|首次调度" 06-proc-init-boot-proc.md \| wc -l = 11` |
| [08 §1.2](08-system-init-boot-finish.md) | 文档虚构 | T8 `system_init` / T9 `add_memmap` grep 0 命中实现；T10 `bsp_finish_booting` 真存在 | `rg "fn system_init\|fn add_memmap" os/kernel/src/lib.rs` → 0 hits |
| [09 范围声明](09-vm-boot-protocol.md) | 前置链错位 | 自称"前置 08"，但实际在 T6 之后才发生（VM 启动需 switch_to_user 先调度） | 09 L5 vs 实际调度循环在 lib.rs:2302 |
| [10 范围声明](10-switch-to-user.md) | 自承 stub | L177/L251 自承"占位 stub"，调度循环依赖 11/13/14 未落地 | rg L177 "占位 stub"、L251 "占位 stub" |
| [11/12/13/14/15](11-scheduling-primitives.md) 等 | 计划状态 | 调度/IPC/异常/时钟/系统调用分发表全部为计划文档，代码 0 实现 | — |
| [16 范围声明](16-smp.md) | 反向依赖 | 自称"前置 11/14/15"，但 smp 实现早于 init（lib.rs:559 T2），早于多数被列前置的文档 | 16 L7 vs lib.rs:559 |

**已影响**：本 session Phase 6 的回归 review 已撞见一次（[07-cross-space-init.md §3.4](07-cross-space-init.md) 之前存在隐含范围溢出，已在本 session 修复）；未来 11/13/14 落地时若继续按现前置链会引入系统性偏差。

**修复路径（用户裁决推迟，本 session 不动手）**：

1. **最小动作（A 层，本 session 内可完成）**：修范围声明 + 前置依赖指向，对齐 T0-T6 实读顺序；不动章节内容/序号/细节
2. **中度动作（B 层）**：上一项 + 把 06 §3.5/§3.12 中属于阶段 F 的内容显式 "see 10 §X" + 08 把 T8/T9 改写为 "规划中 — 当前仅 T10 实现"
3. **大重排（C 层，按 03 重构级别）**：按真实时序重编号 06~16 + 改写各章节内部叙述。涉及 11+ 篇文档，预计单 session 完不成且易引入新错位

**关联**：
- 用户裁决：2026-09-04 选择 "先讨论"（在 `.trae/documents/07-paging-init-d8-implementation-plan.md` 三路径问询中），本条目作为 "讨论结论：留作大调整专项" 的占位记录
- 真实 bug 现场：08-system-init-boot-finish.md:33 作者已自注 "TODO，看起来 06,07,08 的时序并不正确，可能需要稍作调整"
- 间接证据：fix-guard 严格修 06/07/08 时序，会引出 §3.12/§3.5 范围溢出连锁修，建议按 B 层走

**保留**：
- 本 session 完成的 D8 实施（dm_coverage + VmBootHandoff + 测试完备性）不依赖此结构调整——本次回归 review 已对 07-cross-space-init.md 单独完成行号/隐藏文件夹/§4.1 签名补全，**与 I-14 解耦**
- 本文件（todo.md）的 §0.1 计数需待 I-14 修复会话完成后回写（届时本条目从"📌"升"✅"或新拆条目）

### 7.4.2 [I-13] InterruptController trait：C-Rust 不对称 + `Send + Sync` 真实动机

> 来源：doc 05 §3.3 review 顺带发现（2026-08-15，user-question driven）——C 源行为细节揭示 Rust trait 把两类**语义不同**的动作塞进了同一抽象。

**问题陈述**

`InterruptController` trait 的 6 个方法，按硬件动作的 CPU 局部性可分为两类语义：

| 方法 | CPU 局部性 | 说明 | 实际硬件动作 |
|------|----------|------|------------|
| `init()` | BSP 独占 | 整个中断控制器初始化只跑一次（BSP 引导期间） | LAPIC + IOAPIC / GICD + GICR + CPU IF / PLIC + CLINT 一次性 setup |
| `mask_all()` | BSP 独占 | 同样初始化阶段一次性动作 | IOAPIC mask all / GICD_ICENABLER=all / PLIC threshold=max |
| `mask()` / `unmask()` | **全局** | 修改中断控制器内部的"路由表"（IOAPIC redirection entry, GIC GICD/GICR enable, PLIC ENABLE bit）——**一旦改，全部 CPU 看到一致结果** | x86 IOAPIC mask bit / GICD_ICENABLER / PLIC enable=0 |
| `ack()` / `eoi()` | **per-CPU**（隐藏） | 写的是该核的 **LAPIC EOI** / **ICC_EOIR1_EL1** / **PLIC complete per-context** 寄存器——这些是 per-CPU/priv 寄存器，不能跨核写 | 印证：`x86_64::ack(_irq)` 与 `arm64::eoi(_irq)` 中 `_irq` 参数**完全被忽略**，因为这些动作不依赖具体 IRQ 号，只依赖"当前核" |

**现有 trait 的设计混淆点**

- `mask(IrqVector)` 与 `ack(IrqVector)` 看似同层接口（同一 trait、同形参数），但实际位于**不同语义层**——前者改全局路由表，后者写当前核私有寄存器
- 读者若按"对称接口"理解，会错过硬件真相：以为 `Send + Sync` 表达"该类型实例可被多 CPU 同时持有"，但 `ack()` 实际写的是 per-CPU LAPIC，"多 CPU 同时调" 在硬件层不可能发生
- doc §3.3 当前列了 "5 个问题（init/mask_all/mask/unmask/ack/eoi）"——但忽略了"per-CPU 与全局动作"的区分

**`Send + Sync` 的真实动机**

回答 user 问题"为什么 `InterruptController: Send + Sync`？"——三个理由叠加：

1. **mask/unmask 全局可见**：否则一个 CPU 写 IOAPIC mask，另一 CPU 还以为 IRQ 是 unblocked，会读 IOAPIC 的 stale 状态。要求 trait 实例引用可安全跨线程传递，`Send + Sync` 是 Rust 表达"可跨线程共享引用"的最自然方式
2. **ack/eoi per-CPU + trait 又要 `Send + Sync`**：这是矛盾点，但能并存因为 `Send + Sync` 让 trait 实例能**跨线程引用**，"实例字段代表什么"与"实例能否跨线程"是两个独立问题
3. **真实并发安全不来自 trait 自身**：而是 **`BKL (Big Kernel Lock)` 在外面**保证同一时刻只有一个核在调 `InterruptController` 方法。看 `os/kernel/src/arch/.../irq_manager.rs` 调用点可确认

**建议（方向 A，文档层强化）**

**不动 trait**，但**在 doc 05 §3.3 末尾追加一段**：

```markdown
## 关于 Send + Sync 与 per-CPU 真相

`InterruptController: Send + Sync` 看似只是 trait bound，实际表达的是
"实例可被多个 CPU 持有引用"——而该 trait 的 6 个方法分属两类语义：

| 方法层 | 实例字段意义 | 调用时机 |
|--------|------------|---------|
| init / mask_all | BSP 独占操作 | 仅 BSP 启动时 |
| mask / unmask | 全局路由表（IOAPIC redirection entry 等） | BSP 启动时 + 任何 CPU 在 ISR 中 |
| ack / eoi | 当前核私有寄存器（LAPIC EOI / ICC EOIR / PLIC context） | 任何 CPU 在 ISR 中；参数 `_irq` 被忽略 |

**核心洞察**：mask 是"全局动作"（改路由表全部 CPU 看见）；
ack/eoi 是"per-CPU 动作"（写当前核私有寄存器）。
两套动作塞进同一 trait 是**简化抽象**而非**对称抽象**——
实际并发安全由外面 `BKL`（`os/kernel/src/arch/.../irq_manager.rs`）保证，
trait 的 `Send + Sync` 是引用层面的类型证明，不蕴含实例字段可多 CPU 同时变更。
```

**未来的方向 B（**不动手，仅记入 backlog**）**：把当前 trait 拆成两个：
- `InterruptRouter: Send + Sync`（init/mask_all/mask/unmask，全局语义）
- `InterruptAck: Send`（ack/eoi，per-CPU 语义，每核持有一个实例）
- 上层用 `Router` + per-CPU `Ack` 组合

但这是 trait 重构工作（影响 `os/plat/src/{x86_64,arm64,riscv64}/interrupt.rs` 三个文件 + `os/kernel/src/irq_manager.rs` + doc 05/14），**不在 05-clock-interrupt-init.md 阶段范围内**。记入 backlog，作为 SMP 阶段（16-smp 之后）的可重构目标。

**前置依赖**：需要先实现 `PerCpu<Ack>` 分派基础设施（每 CPU 持有一个 `InterruptAck` 实例）+ BSP/AP 启动钩子序列化。这依赖 16-smp.md 的 SMP 完整实现。

**关联**

- 与 §D1（errno newtype）同类——属于"trait 抽象看似对称但语义不对称"的认知陷阱
- 与 §A1（全局访问器 witness 全覆盖）有交叉：ack/eoi 的 per-CPU 局部性可以合并到 witness 类型系统
- doc 14 §4.8（中断交付）也提到 `irq_handle()` 中 mask→handler→unmask 的同款全局动作，需要在 §14 中同步说明

**优先级**：P2（文档改进，非代码缺陷）。当前 trait 工作正常（BKL 保护），但读者心智模型建立需要明确说明。



| 文档条目 | 对应架构建议 | 关系 |
|---------|-------------|------|
| 13 §6.5 errno newtype | §D1 | 同项（doc 13 独立提出，相互印证） |
| 13 §6.2 BklGuard RAII（已拒绝） | §B1 | RAII 已拒绝；§B1 的显式 transfer API 是替代方案，未定案 |
| 16 §4.13 BKL 接入 4 处 DEFERRED | §A1 / §B1 | BKL 接入是 witness 全覆盖的前提之一 |
| 24 D-25 部分拷贝进度报告 | cross_space_copy（§0 基线） | 演进方向一致 |
| 22 D9（裸 usize 指针） | — | 独立 P2 清理项 |

### 7.6 已解决排除表（grep 验证后不入清单）

| 条目 | 判定 | 证据 |
|------|------|------|
| 11 `notify_scheduler` TODO | ✅ 已实现 | proc_table.rs:643 完整实现（mini_send SCHEDULING_NO_QUANTUM，R-15 不变量注释，2026-08-12） |
| 12 `copy_msg_from_user` trait TODO | ✅ 已落地 | `UserCopy` trait（ipc.rs:266-271）含 copy_msg_from_user |
| 07 syscall_copy 6 处 stub | ✅ 已全部实现 | doc 18 §4.8（2026-08-01，verify_grant / vm_lookup / vm_memset / PTE walk） |
| 20 §4.8 DEFERRED 全部 | ✅ 已解决 | dispatch_irqctl/vdevio/sdevio/readbios 全实现；iopenable trap frame 已解决（C p_reg = Rust cpu_context） |
| 13 §6.1 kernel_call_resume 清除时机 | ✅ 已修复 | P0-3，syscall.rs:1342-1378 |
| 05 Timer queue 测试未覆盖 | ✅ 已覆盖 | doc 15 §5.1/5.2 有 12+ 个 timer queue 测试 |
| 16 cache 对齐 FIXME | ✅ 非缺口 | 对齐 C 现状（C 源码同样 FIXME 未实现） |
| 23 IPC_STATUS_* | ✅ 已实现（P9-2） | `CpuContextArch::or_ipc_status_reg` + ipc.rs 4 路径 wire |
| 06 `EntrySpec::DEFERRED` 非 VM 进程 | ✅ 设计语义 | boot 协议（RS 运行时加载），非缺口 |
| 08 三处 C 步骤差异（cpu_identify / krandom_init / CPU_IS_READY） | ✅ ARCH 演进 | 08 §4 差异说明表，非实现遗漏 |
| 27 kmess 缓冲 | ✅ 演进替代 | §W-7（log crate + EarlyConsole） |
| 25 swap_memreq（D-30） | 🟢 设计 no-op | misc.rs:1807 注释 + doc 25（2026-08-14）：vmrequest 链无入队路径 → 链恒空 → swap 无操作；与 W-6 同类，D-20 落地时重审 |



> 原 18 节 todo 全部处理完毕（✅ 已解决 / 🔀 已转移 / 📋 完成记录），跨阶段未完成项
> 已实体转移到 `16-smp.md §5.3` / `00-vm-overview.md §8.4` / `notes/TODO.md`。
> 本次（2026-08-14）文件主体转为架构优化建议 + 文档 01-30 未完成项收集（§7），
> **原 18 节完整内容（968 行）按追加语义保留于下方 §H，未删除**。

## H. 历史归档（完整保留，2026-08-14 追加恢复）

### H.1 原始 18 节版（清空前，968 行，完整保留）

> 原始 todo.md 全文（2026-08-14 清空前），按追加语义从 git 历史（97df4e58d^）完整恢复。
> 标题层级降两级（原 H1/H2/H3 → H4/H5/H6），与本文档主体及 §H 标题层级错开。

#### 原文档标题：03-stage-kernel 全局 TODO

> 本文件汇总 03-stage-kernel 中各文档尚未完成、待后续实现的 Rust 开发任务。
> 每个条目标注来源文档。

---

##### 1. Boot module 内存回收（Rust 实现）

> 来源：`01-boot-shim-bootstrap.md` / `01-todo.md` C 项（C 源码讲解已完成）

**背景**：Minix3 的 boot module 物理内存生命周期是：`cut_memmap()` 临时切掉 → `protect.c` 解析 ELF 复制到进程空间 → `add_memmap()` 回收。C 源码已在 01 文档 §2.5 讲解完毕。

**Rust 实现待办**：

| 环节 | Minix3 C | Rust minix-rs | 状态 |
|------|----------|---------------|------|
| boot module 加载 | GRUB 传入 `module_list[]` | `boot_modules: &[]` 始终为空 | ❌ 缺失 |
| 从 memmap 切掉 module 内存 | `cut_memmap()` (pre_init.c:211) | 无等价实现 | ❌ 缺失 |
| ELF 解析 + 复制到进程空间 | `protect.c` 解析 ELF | `kmain()` 是 `loop {}` | ❌ 缺失 |
| 回收 module 物理内存 | `add_memmap()` (protect.c:450) | 无等价实现 | ❌ 缺失 |
| 回收 bootstrap 代码内存 | `add_memmap()` (main.c:301) | 无等价实现 | ❌ 缺失 |

`BootModule` 结构体已定义（`minix-types/src/kernel_info.rs`）但从未被填充使用。不仅在回收逻辑缺失，整个 boot module 生命周期（加载→ELF解析→复制→回收）都尚未实现。

---

##### 2. `ExitBootServices` 后内存映射回收 + `Box::leak` 生命周期

> 来源：`01-boot-shim-bootstrap.md` §4 实现详解、`uefi_helpers.rs`

**问题**：
- `uefi_helpers.rs` 中 `build_memmap()` 在 `ExitBootServices()` **之前**调用，只收集 `CONVENTIONAL` 类型区域
- `exit_boot_services()` 返回的最终内存映射被丢弃（`_mmap`），kernel 永远看不到 `ExitBootServices` 后的页面重新分类
- `Box::leak()` 泄漏的 `MemoryRegion` 切片（`'static`）只是症状——根本问题是 kernel 无法知道 boot-shim 占用的内存在退出后被固件标记为什么类型（可用/保留/已用）

**与 Minix3 C 的差距**：
- C 版本：`pre_init()` 记录 `bootstrap_start`/`bootstrap_len` → `kmain()` 调用 `add_memmap(&kinfo, bootstrap_start, bootstrap_len)` **显式归还**
- Rust 版本：boot-shim 是独立 `.efi` 程序，kernel 不知道其内存布局；`ExitBootServices` 最终映射被丢弃后，boot-shim 内存对 kernel 不可见

**修复方向**：
- [ ] 在 `ExitBootServices` **之后**获取最终内存映射，替代 `build_memmap()` 的退出前快照
- [ ] 或：boot-shim 在 `BootPrepareResult` 中传递 `boot_shim_start`/`boot_shim_len`，让 kernel 在初始化后显式归还（对标 Minix3 C 的 `add_memmap(bootstrap)`）

---

##### 3. qemu-tests 后续扩展

> 来源：`01-todo.md` D 项（目录结构重组已完成）

**待办**：

- [ ] 按文档语义添加后续测试目录：

```
test-kernels/
├── kernel/
│   ├── bootstrap/      # 01-boot-shim-bootstrap.md ✅ 已创建
│   ├── pagetable/      # 02-page-table-kernel.md
│   ├── exception/      # 05-exception-interrupt.md
│   └── ipc/            # 09-sync-ipc.md
└── vm/                 # 02-stage-vm/
    └── ...
```

- [ ] 代码去重：test-kernel 中的 UEFI 入口/串口/memmap 代码应复用 `boot-shim` + `kernel` 的正式代码，而非各自复制
- [ ] 全系统 QEMU 启动规划：每个阶段用同一内核 + 不同 boot modules

---

##### 4. 页表页分配器：VM 阶段接入

> 来源：`01-todo.md` E 项（boot 阶段已完成）

- [ ] 设计 memmap 中切除 bump 范围的机制（确保 VM 不踩踏）
- [ ] VM 接入时实现 `vm_pt_alloc` 并注册

---

##### 5. code-review 未修复问题（来自 02-code.md）

> 来源：`02-code.md` 修复记录中的"未修复（留待后续）"部分（审查日期 2026-06-09）
> 12 项 code-review 问题中 7 项已修复，本节列出剩余 5 项。
> 已修复的 7 项详见 02-code.md §修复记录；01-bug.md 同步更新了 §3.2 和 §5。

###### 4.1 [P1] KernelInfo 字段 pub 封装 — ✅ 已完成（R-07 / FIX-12，2026-08-12）

- **类型**: 设计 / 命名 / 封装
- **位置**: `os/libs/minix-boot/src/kernel_info.rs`
- **问题**: `KernelInfo` 所有字段都是 `pub`，外部代码可以读/写所有内部状态，缺少封装边界。`free_upper_idx` 始终为 0 但没有"未初始化"语义，外部无法判断有效性。
- **已完成修复 (2026-06-16)**:
  - `free_upper_idx` 类型从 `usize` 改为 `Option<usize>`，用 `None` 表示"boot-shim 尚未计算"
  - 添加 `pub fn free_upper_idx(&self) -> Option<usize>` getter 方法
  - boot-shim 构造时填 `None`（C 中由 `pg_mapkernel()` 返回值设置，Rust boot-shim 尚未实现此计算）
  - kernel `init_post_and_memory` 中使用 `.expect()` 取值
  - `misc.rs` 诊断输出使用 `.unwrap_or(0)`
  - QEMU 测试中有意义的值填 `Some(N)`，无意义的填 `None`
- **R-07 完成修复 (2026-08-12, FIX-12)**:
  - 新增 `KernelInfo::validate()` 方法，强制 5 项不变量（bootstrap_len=0、kern_size>0、栈 16 字节对齐、memmap/boot_modules 非空）
  - `kmain` 入口调用 `validate()` fail-fast
  - 新增 12 个 `#[inline]` getter 方法作为首选 API
  - kernel 内部所有直接字段访问迁移到 getter（`lib.rs` 11 处 + `arch/boot.rs` 1 处）
  - 字段保持 `pub` 以兼容 boot-shim 构造和 test 断言（设计决策：`KernelInfo` 是 `Copy` POD，跨 crate 构造需求）
  - 7 个单元测试覆盖 validate() + getter
- **验证**: `cargo test -p minix-boot --lib kernel_info` → 7 passed + `cargo test -p minix-kernel --lib` → 561 passed + `cargo test -p minix-kernel --test boot_integration` → 2 passed

###### 4.2 [P1] HigherHalf trait 单方法评估 — ✅ 已完成

- **类型**: trait 设计
- **位置**: `os/kernel/src/boot/higher_half.rs`
- **问题**: `HigherHalf` trait 只有 1 个方法 `jump_to_kmain`，3 个实现行为模式相同（切栈 + 对齐 + 清零 FP + 跳转），只是汇编指令不同。
- **当前评估**:
  - ✅ 3 个实现行为不同（x86_64 `mov rsp + call`，aarch64 `mov sp + br`，riscv64 `mv sp + jalr`）→ 多态合理
  - ✅ 被 `arch_boot()` 通过具体类型调用，但测试代码中已验证泛型 bound 能力（`fn accept_higher_half<H: boot::HigherHalf>()`）
  - ✅ 只有 1 个方法但语义完整
- **修复方案**: 采用方案 A，保持现状
  - `higher_half.rs` 文档注释已完整说明设计意图（架构隔离、统一调用点、可测试性三点理由）
  - `02-higher-half-kernel.md` §3.4 已详细展开 "为什么需要 trait 而不是 `#[cfg(target_arch)]`"
  - 无需新增方法或改为枚举分发
- **风险**: 如果未来 `HigherHalf` 增加方法，需要重写所有实现（当前单方法语义完整，无扩展计划）
- **验证**:
  - `cargo check -p minix-kernel --all-targets --all-features` 通过
  - 文档与代码一致：§3.4 的 trait 定义与 `higher_half.rs` 源码匹配
  - 测试代码验证泛型 bound：`kernel/src/lib.rs:718-719` `accept_higher_half::<MockHigherHalf>()` 编译通过
- **工作量**: 小（1-2 小时，主要是文档）

###### 4.3 [P1] kmain_verify 三架构重复代码 — ✅ 已完成

- **类型**: 重构 / 抽象
- **位置**: `os/kernel/src/lib.rs:318-432`（约 100 行）
- **问题**: 三架构的打印和断言逻辑几乎完全相同，只是 `early_console` 模块路径不同。
- **重复模式**: 打印 `### test-higher-half ###` + 架构名 + kern_virt_base + kern_phys_base + kern_size；验证 KERN_VIRT_BASE 在 `0xFFFF_8000_0000_0000` 范围；验证 kern_phys_base < kern_virt_base；验证 kern_size 是 HUGE_PAGE_SIZE 倍数；打印 PASS/FAIL
- **修复方案**: 采用方案 A，抽象 `EarlyConsole` trait
  1. 在 `os/arch/src/early_console.rs` 定义 `EarlyConsole` trait，含 `write_byte` 抽象方法和 `write_str`/`write_hex` 默认方法（统一处理 `\n` → `\r\n` 和十六进制格式）（**落地位置更新（2026-08-13）**：trait 最终定义于 `os/plat/src/early_console.rs:14`——plat crate 持有，见 27-kernel-utility.md）
  2. 三架构 `early_console.rs` 各添加 ZST 类型（`X86_64EarlyConsole`/`AArch64EarlyConsole`/`Riscv64EarlyConsole`）实现 `EarlyConsole`
  3. `arch/src/lib.rs` 添加 `CurrentEarlyConsole` type alias，统一导出
  4. `kernel/src/lib.rs` 的 `kmain_verify` 使用 `CurrentEarlyConsole` 统一输出，仅保留 `#[cfg]` 区分寄存器标签（`RSP`/`SP` 等）和架构名，消除全部重复代码块
- **风险**: 无。各架构原有的 `early_console::write_str`/`write_hex` 自由函数保留，测试内核代码无需改动。
- **验证**: `cargo check -p minix-arch -p minix-kernel --all-targets --all-features` 通过；`kmain_verify` 输出格式与重构前完全一致
- **工作量**: 中（4-6 小时）

###### 4.4 [P2] PTE_HUGE_FLAGS 语义不明 — ✅ 已完成

- **类型**: 命名
- **位置**: `os/arch/src/arch/paging_ext.rs:96`
- **问题**: `PTE_HUGE_FLAGS: u64 = 0`（riscv64/arm64）或 `1 << 7`（x86_64），命名暗示"大页的 PTE 标志"，实际含义是"需要在 PTE 中额外设置的大页标识位"。
- **修复**: 采用方案 A，重命名为 `PTE_HUGE_IDENTIFIER_BIT`，并补充文档注释说明各架构取值（x86_64: PS bit `1 << 7`; ARM64/RISC-V: 0）。同时更新了所有文档引用。
- **验证**: `cargo check -p minix-arch --all-targets --all-features` 通过；`grep PTE_HUGE_FLAGS` 确认代码和文档中无遗漏（仅剩 x86_64/pte.rs 内部常量 `PTE_HUGE`，非 trait 接口）

###### 4.5 [P2] 调试输出无注释 — ✅ 已完成

- **类型**: 注释 / 工程规范
- **位置**: `os/kernel/src/lib.rs:82-85,177,208,246,251`
- **状态**: 2026-06-09 已彻底移除（推荐方案 A：完全移除）
- **备注**: 生产内核不应包含调试输出，feature gate 增加了配置复杂度

---

###### 4.x 处理优先级建议

| 子项 | 优先级 | 工作量 | 建议处理时间 |
|------|--------|--------|--------------|
| 4.1 KernelInfo pub 封装 | P1 | 中 | ✅ 已完成（R-07 / FIX-12） |
| 4.3 kmain_verify 重复 | P1 | 中 | 重构窗口期统一处理 |
| 4.2 HigherHalf 单方法 | P1 | 小 | 文档完善即可（推荐保持现状） |
| 4.4 PTE_HUGE_FLAGS 命名 | P2 | 小 | ✅ 已完成 |
| 4.5 调试输出 | P2 | — | ✅ 已完成 |

**建议**: 4.1 和 4.3 在下一次大规模重构时统一处理（涉及 trait 设计和 API 变更）；4.2 通过文档注释解决。

---

##### 6. 测试缺口

> 来源：`04-tests.md`（分析日期 2026-06-11）+ `07-cross-space-init.md` review（2026-06-21）
> 分析覆盖：03-kmain-cstart.md + 04-clock-interrupt-init.md + 07-cross-space-init.md
> 总测试数：126 单元测试 + 6 QEMU 测试 + 本轮新增 13 测试（arch 9 + per-arch 9 + kernel 2 → 去重后 ~20）全部通过。

###### 6.1 [P1] 异常端到端测试（L4）：handler 地址为 0，异常交付未验证

> 当前 IDT handler 地址全为 0（`os/arch/src/x86_64/trap_entry.rs:151`），无法验证异常交付链路。

- [ ] x86_64: 触发除零异常（vector 0）→ 验证 handler 执行
- [ ] x86_64: 触发缺页异常（vector 14）→ 验证 handler 执行
- [ ] aarch64: 触发 SVC → 验证 VBAR_EL1 跳转到 handler
- [ ] riscv64: 触发 ecall → 验证 stvec 跳转到 handler

**范围**：此缺口属于后续"异常处理"文档的阶段。当前 03/04 文档只需证明寄存器配置正确。

###### 6.2 [P1] 中断端到端测试（L5）：中断交付链路未验证

> QEMU GDB 脚本验证了中断控制器寄存器初始化配置，但未验证实际中断到达 → handler 执行 → EOI 完整链路。

- [ ] 时钟中断到达 CPU → handler 执行 → ClockState::tick() 被调用
- [ ] 中断完整链路：设备 → GIC/APIC/PLIC → CPU → handler → EOI
- [ ] 中断 mask/unmask 端到端行为验证

**范围**：此缺口属于后续"中断处理"文档的阶段。当前只需证明中断控制器和时钟硬件寄存器配置正确。

###### 6.3 [P1] init/load 顺序约束缺少测试

> `ProtectionArch::load()` 必须在 `TrapEntryArch::load()` 之前的约束仅在文档中声明（03 文档 §4.1），无测试验证违反顺序的后果。

**范围**：依赖 SMP 多核支持，将在 SMP 阶段补充。

###### 6.4 [P1] init_ap 路径验证缺失

> AP 启动路径完全未测试（`init_ap` 函数未被任何测试覆盖）。

**范围**：依赖 SMP 多核支持，将在 SMP 阶段补充。

###### 6.5 [P2] aarch64 GICv3 PPI unmask 未实现 — ✅ 已完成

> **位置**: `os/plat/src/arm64/interrupt.rs` `unmask()` / `mask()` 方法
> PPI (IRQ < 32) 的 unmask 已实现，通过 Redistributor 的 `GICR_ISENABLER0`/`GICR_ICENABLER0` 寄存器控制。
> 同时更新了 `InterruptController` trait 文档中的 ARM64 列，反映 PPI 路径。
> 文档 `04-clock-interrupt-init.md` §4.6 状态说明已同步更新 (2026-06-16)。

###### 6.6 [P2] riscv64 PLIC base 硬编码 — ✅ 已完成

> **位置**: `os/plat/src/riscv64/interrupt.rs`
> PLIC_BASE = 0x0C00_0000 硬编码为 QEMU virt 默认地址，未从 device tree 自动发现。有 `set_base()` 方法可手动覆盖。

**2026-06-20 更新**：`DeviceTreeDesc` 已实现 RISC-V PLIC 节点解析（`os/libs/minix-platform/src/device_tree.rs`）。当 boot-shim 传入 DTB 指针时，`init_from_kinfo()` 走 DTB 解析路径，不再使用硬编码 `PLIC_BASE`；无 DTB 时才回退到 `QemuVirtDesc`。

###### 6.7 [P2] riscv64 PMP 仅配置 entry 0

> **位置**: `os/arch/src/riscv64/arch_init.rs`
> PMP 仅配置 entry 0 为 Allow All (NAPOT + R+W+X)，未设置区域隔离。

当前简单场景足够，暂无安全隔离需求。后续需完整 PMP 配置。

###### 6.8 [P2] QEMU GDB 脚本未集成到 CI

> **位置**: `os/arch/tests/qemu_test_{x86_64,aarch64,riscv64}.sh`
> 三个 QEMU GDB 自动化脚本当前为手动运行，未集成到 CI pipeline。

建议后续集成到 CI，每个 PR 自动验证 QEMU 寄存器初始化。

---

##### 7. PlatformDesc / 硬件发现

> 来源：设备树 / ACPI 硬件发现 TODO

**已完成（2026-06-20）**：

| 任务 | 状态 | 实际文件 |
|------|------|----------|
| 设计 `PlatformDesc` trait | ✅ | `os/libs/minix-platform/src/desc.rs` |
| 实现 `QemuVirtDesc` 兜底 | ✅ | `os/libs/minix-platform/src/qemu_virt.rs` |
| 实现 `DeviceTreeDesc`（FDT/DTB 解析） | ✅ | `os/libs/minix-platform/src/device_tree.rs` |
| 实现 `AcpiDesc`（最小 ACPI 解析） | ✅ | `os/libs/minix-platform/src/acpi.rs` |
| 实现 `PlatformContext` 全局 + `init_from_kinfo()` | ✅ | `os/libs/minix-platform/src/global.rs` |
| UEFI boot-shim 定位 RSDP/DTB | ✅ | `os/boot-shim/src/uefi_helpers.rs` |
| OpenSBI boot-shim 保存 a1 DTB 指针 | ✅ | `os/boot-shim/src/opensbi_helpers.rs` |
| `KernelInfo` 扩展 `platform_descriptor` 字段 | ✅ | `os/libs/minix-boot/src/kernel_info.rs` |

**验证**：
- `cargo test -p minix-platform`：19/19 通过（含 DTB/ACPI 合成表解析测试）。
- `cargo check -p minix-platform -p minix-kernel`：通过。
- `cargo check -p boot-shim --features test-all`：通过。

**剩余偏差 / 后续扩展**：

- `AcpiDesc` 当前为最小化实现：仅支持 RSDP → XSDT/RSDT → MADT，提取 LAPIC/IOAPIC base 和 CPU 拓扑。HPET、x2APIC 中断投递、Interrupt Source Override、多 IOAPIC 等尚未实现（QEMU `virt` x86_64 当前够用）。
- 文件名 `device_tree.rs` 与设计稿 `fdt.rs` 不一致，属命名偏差，功能等价。

**收益**：支持真实硬件移植时，不需要为每块板子单独修改 Rust 源码；ARM/RISC-V 换 DTB，x86 换 ACPI 表即可。

---

##### 8. 平台发现阶段预存在问题（与本次改动无关）

> 来源：Phase 2~4 实施过程中通过 `cargo build --workspace` 发现，与本次 `minix-platform` 新增代码无关。

###### 8.1 `test-memmap-riscv64` 编译失败

- **位置**：`os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs`
- **错误**：`use minix_plat::riscv64::early_console` → `could not find riscv64 in minix_plat`
- **含义**：测试内核引用了一个不存在的模块路径 `minix_plat::riscv64`。可能是 `os/plat/src/riscv64` 子模块尚未创建，或测试内核的导入路径已过时。
- **影响**：仅影响该单个测试二进制；`minix-platform` 自身、UEFI/OpenSBI boot-shim、`minix-kernel` 均不受影响。
- **修复方向**：
  1. 确认 `os/plat/src/riscv64/mod.rs` 是否存在；若不存在则创建。
  2. 若模块已存在但路径不同，更新测试内核的 `use` 语句。
  3. 若该测试已废弃，考虑移除或重命名。

###### 8.2 `boot-shim` 默认 target 的 `panic_impl` lang item 冲突

- **位置**：`os/boot-shim`
- **错误**：
  - `error[E0152]: found duplicate lang item panic_impl`
  - `error[E0425]: cannot find function arch_boot in crate minix_kernel`
- **含义**：
  - `boot-shim` 在默认 target 下既自己实现了 `panic_handler`，又链接了同样实现 `panic_handler` 的 crate（如 `minix-kernel` 或测试框架），导致 Rust lang item 重复。
  - 同时 `arch_boot` 函数在当前 cfg/target 组合下不可见。
- **影响**：
  - `cargo check -p boot-shim`（不带 feature）失败。
  - `cargo check -p boot-shim --features test-all` 通过，说明 `test-all` feature 的依赖/link 配置是正确的。
- **修复方向**：
  1. 检查 `boot-shim/Cargo.toml` 的默认 feature 是否错误地依赖了 `minix-kernel` 或测试 crate。
2. 检查 `boot-shim/src/main.rs` 的 `panic_handler` 是否在非测试 target 下被错误启用。
3. 检查 `arch_boot` 的可见性：是否只在某个 feature 或 target_arch 下暴露，而默认 target 没有。
4. 考虑把 `boot-shim` 的默认 target 也改为与 `--features test-all` 一致的配置，或明确区分"真实固件目标"与"测试目标"的 panic_handler 归属。

**备注**：这两个问题在本次 Phase 2~4 实施前已存在，不应由本次 `minix-platform` 改动负责。后续优先处理 8.2，因为它影响 `boot-shim` 的常规构建体验。

---

##### 9. Rust 2024 edition 迁移后暴露的残留编译错误

> 来源：2026-06-21 将全部 `os/` crate edition 由 2021 升级到 2024 时，`cargo check --workspace --all-targets` 暴露的、与 edition 无关的预先存在问题。
> 2024 edition 本身只引入 4 处真实语法问题（已就地修复，详见本文件 §9.0），其余错误均为更早阶段遗留。

###### 9.0 2024 edition 自身引入并修复的 4 处问题 ✅

| 文件 | 行号 | 修改 |
|------|------|------|
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs` | 19 | `#[link_section = ".bss"]` → `#[unsafe(link_section = ".bss")]` |
| 同上 | 79 | `#[no_mangle]` → `#[unsafe(no_mangle)]` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs` | 140 | `extern "C" { ... }` → `unsafe extern "C" { ... }` |
| 同上 | 307 | `extern "C" { ... }` → `unsafe extern "C" { ... }` |

###### 9.1 [P1] `E0152 duplicate lang item panic_impl` — test-kernel 与 boot-shim

**根因**：测试二进制是 `#![no_std]` 的 `bin`，自带 `fn panic(_: &PanicInfo) -> !` 作为 `panic_handler`。但它们依赖的 `minix-kernel`（默认 feature = mock，链接到 `std` 用于测试）以及 `boot-shim` 的 lib/test 目标都会拉入 `std`，而 `std` 已经定义了一个 `panic_impl` lang item，导致冲突。

**触发位置**：

| 文件 | 备注 |
|------|------|
| `os/boot-shim/src/main.rs:57` | `boot-shim` 默认 target 的 panic_handler |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:70` | test-higher-half bin |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-proc-init/src/main.rs:263` | test-proc-init bin |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-riscv64/src/main.rs` | 同类 |

**影响范围**：所有依赖 `minix-kernel` 或 `boot-shim` 的 `#[no_std]` 测试 bin 在 `cargo check --tests` 时全部失败。

**修复方向**：
1. **方案 A（推荐）**：让 `minix-kernel` 和 `boot-shim` 的"生产代码 target"也通过 cfg 与 "测试 target"分离 panic_handler——生产 target 依赖 `core::panic::PanicInfo`，测试 target 走 std 的 panic_handler。
2. **方案 B**：让 test-kernel bin 不通过 `minix-kernel` 转而直接使用 `minix-arch` + `minix-platform` 这类底层 crate，跳过 mock feature 带来的 std 依赖。
3. **方案 C**：把 `minix-kernel` 的 mock feature 与 std 依赖解耦（`mock` 不应拉 `std` 进生产二进制）。

###### 9.2 [P1] `E0425 cannot find function arch_boot / init_proc_and_boot / init_post_and_memory` — cfg gating 不匹配

**根因**：`os/kernel/src/lib.rs` 中这些函数都标注 `#[cfg(not(feature = "mock"))]`，而 `os/qemu-tests/test-kernels/kernel/bootstrap/*` 编译时通过 workspace 传递 `minix-kernel` 的默认 feature（= `mock`），导致函数被 cfg 掉。

**触发位置**：

| 函数 | kernel/src/lib.rs 行号 | 引用方 |
|------|----------------------|--------|
| `arch_boot` | 64 / 70 / 82 / 94（按 target_arch 三份 + 一份 mock） | `boot-shim`、`hello-boot`、`test-paging-enable-*`、`test-protection-aarch64` |
| `init_proc_and_boot` | 693 | `test-proc-init/src/main.rs:168` |
| `init_post_and_memory` | 910 | `test-proc-init/src/main.rs:250` |

**影响范围**：依赖 `minix-kernel` 但又要走真实 arch 路径的 test-kernel bin 全部失败。

**修复方向**：
1. 让 test-kernel 在自己的 `Cargo.toml` 中显式 `default-features = false, features = ["real"]`（需新增对应 feature）或
2. 让 `minix-kernel` 拆分为 `minix-kernel-mock` 与 `minix-kernel-real` 两个 crate，test-kernel 引用 real 版本；或
3. 把 `arch_boot` 等从 cfg gating 中放出来，改为 `#[cfg(any(not(feature = "mock"), feature = "test-allow-real"))]` 这种"测试场景下也编译"的形式。

###### 9.3 [P1] `E0432/E0433 unresolved import minix_plat::arm64 / riscv64` — 路径不存在

**根因**：`os/plat/src/` 当前没有 `arm64/mod.rs` 和 `riscv64/mod.rs` 子模块，但部分 test-kernel bin 通过 `use minix_plat::arm64::...` / `minix_plat::riscv64::...` 引用。

**触发位置**：

| 文件 | 行号 |
|------|------|
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-riscv64/src/main.rs` | `use minix_plat::riscv64` |

**修复方向**：
1. 在 `os/plat/src/arm64/mod.rs` 与 `os/plat/src/riscv64/mod.rs` 创建空模块（最低修复成本），或
2. 重构 `os/plat/` 把 aarch64/riscv64 早期的 `early_console` 等能力下沉到 `os/arch/src/arm64/` / `os/arch/src/riscv64/`（已经在那里），让 test-kernel 改 import 路径——参考 `04-platform-discovery.md §3.5` 已迁移的设计。

###### 9.4 [P1] `the #[global_allocator] in this crate conflicts with global allocator in: uefi` — uefi crate 已自带 allocator

**根因**：UEFI 测试 bin 通过 `uefi` crate 间接拉入其内置的 `#[global_allocator]`（用于 `Vec` 等），而 test-kernel 自带的 `BootAllocator` 也声明 `#[global_allocator]`，两个 allocator 冲突。

**触发位置**：`test-paging-enable-aarch64/src/main.rs` 与 `test-paging-enable-riscv64/src/main.rs` 之类的带 `BootAllocator` 的 UEFI 测试 bin。

**修复方向**：把 `BootAllocator` 移除，改用 `uefi` 自带的 allocator；或在 feature flag 控制下二选一。

###### 9.5 修复优先级建议

| 子项 | 根因类别 | 影响 crate 数 | 建议处理时机 |
|------|---------|--------------|------------|
| 9.1 panic_impl lang item | `no_std` bin 与 `std` 依赖混杂 | 17 个 test-kernel + boot-shim | 重构 `minix-kernel` 拆分 crate 时一并修 |
| 9.2 cfg gating 不匹配 | mock feature 默认开启 | 17 个 test-kernel | 同上 |
| 9.3 模块路径缺失 | `os/plat` 拆分未完成 | 12 个 test-kernel | 同 9.1/9.2 一次处理 |
| 9.4 global_allocator 冲突 | uefi crate 自带 allocator | 2 个 test-kernel | 最小修复，可单 PR 处理 |

**共性**：四个子项都源于 `os/qemu-tests/test-kernels/*` 这批早期脚手架代码未跟随 `minix-kernel` / `minix-platform` / `os/plat` 的重构而同步更新。**修复它们的最佳窗口是下一次涉及这三个 crate 的大规模重构**——单独修只能解决症状，治本需要重新设计 test-kernel 与生产 crate 的依赖边界。

**临时缓解**：日常开发可只跑 `cargo check -p minix-arch -p minix-platform -p minix-boot -p minix-elf -p minix-rt -p minix-sys -p minix-types -p minix-plat`（这 8 个核心 crate 已 2024 edition 干净通过），CI 可加白名单忽略 test-kernel bin 直到 §9.x 解决。

---

##### 10. boot-shim 与 kernel 的 `static mut` 收编到 `AssumeSyncCell`

> 来源：本次 `04-platform-discovery.md §3.5` 重构时把 `SyncPlatformCell` 替换为 `minix-types::AssumeSyncCell`，审计其他同类位置时发现 `boot-shim` 和 `kernel` 还在用 `static mut`——属于同一模式但更大范围的重构。

###### 10.1 现状：项目内仍使用 `static mut` 的位置

| 文件 | 行号 | 标识符 | 用途推断（未验证） |
|------|------|--------|---------------------|
| `os/boot-shim/src/opensbi_helpers.rs` | 252 | `BOOT_FILE_TABLE_PTR` | OpenSBI 启动文件表指针 |
| `os/boot-shim/src/opensbi_helpers.rs` | 300 | `DTB_PTR` | 设备树指针（OpenSBI 阶段） |
| `os/boot-shim/src/opensbi_helpers.rs` | 335 | `BUMP_PTR` | bump allocator 游标 |
| `os/kernel/src/lib.rs` | 999 | `FREE_MEMMAP` | 物理内存 map（待函数过滤） |
| `os/kernel/src/lib.rs` | 1011 | `KERNEL_INFO` | `Option<KernelInfo>`，单写多读 |
| `os/kernel/src/lib.rs` | 1032 | `FREE_PDE_SLOTS` | 页表项空闲槽位 |
| `os/kernel/src/lib.rs` | 1090 | `IPC_FILTER_POOL` | IPC 过滤器池 |

###### 10.2 已完成的同模式重构（参考样板）

**参考实现**：`os/libs/minix-platform/src/global.rs` 当前形态（2026-06-21 改后）。

```rust
// 之前：自己定义 wrapper
struct SyncPlatformCell(UnsafeCell<Option<PlatformContext>>);
unsafe impl Sync for SyncPlatformCell {}
static PLATFORM: SyncPlatformCell = SyncPlatformCell(UnsafeCell::new(None));

// 之后：复用项目统一原语
use minix_types::AssumeSyncCell;
static PLATFORM: AssumeSyncCell<Option<PlatformContext>> = AssumeSyncCell::new(None);
// init:  *PLATFORM.get() = Some(...);
// read:  (*PLATFORM.get()).as_ref().expect(...)
```

**收益**：消除重复 wrapper，集中安全契约（"调用方保证单线程独占访问"），与 VM server / heap arena / vmproc table 保持一致。

###### 10.3 重构范围与顺序

| 阶段 | 范围 | 难度 | 前置条件 |
|------|------|------|----------|
| 10.3.1 | `KERNEL_INFO: Option<KernelInfo>`（kernel/src/lib.rs:1011） | **低**——已是 `Option<T>`，模式与 PLATFORM 完全一致 | 验证 `KernelInfo: Send + Sync`（大概率已具备，因字段是 usize/u64/array） |
| 10.3.2 | `FREE_MEMMAP`、`FREE_PDE_SLOTS`、`IPC_FILTER_POOL`（kernel/src/lib.rs:999, 1032, 1090） | **中**——类型/大小需确认是 Copy 还是含数组；含大数组时 `AssumeSyncCell<T>` 仍然合适，但需要验证运行时拷贝开销 | 验证这些类型已经是 `Copy`/`Clone` 或可由 `AssumeSyncCell<T>` 直接持有 |
| 10.3.3 | `BOOT_FILE_TABLE_PTR`、`DTB_PTR`、`BUMP_PTR`（boot-shim/src/opensbi_helpers.rs:252, 300, 335） | **中-高**——boot 阶段初始化顺序敏感；OpenSBI 阶段可能比 `minix-types` 更早，需要 `cfg`-gate 或 `boot-shim` 内复制一份等价 wrapper | 确认 boot-shim 与 minix-types 的依赖顺序，必要时在 boot-shim 内定义等价 wrapper 后再统一收编 |

###### 10.4 触发条件

> **触发条件**：`os/kernel/` 完成重大重构（特别是内存管理、IPC 重构）后，**重新扫描项目内所有 `static mut`**——一次性扫描+收编，避免每次单独 PR。

**重新扫描命令**：
```bash
rg "^static mut " os/ --type rust -n
```

**执行 PR 检查清单**：
1. 确认目标类型是 `Send + Sync`（或本就是单线程使用）
2. 确认无跨线程共享访问（通过 BKL 或单线程上下文保证）
3. 替换为 `static X: AssumeSyncCell<T> = AssumeSyncCell::new(...);`
4. 所有读 `*X` 改为 `unsafe { *X.get() }` 或 `unsafe { X.as_ptr() }`
5. 所有写 `X = ...` 改为 `unsafe { *X.get() = ... }`
6. 验证 `cargo check --workspace` 通过、`cargo test -p kernel -p boot-shim` 通过

###### 10.5 不迁移 `static mut` 的反例（应保留）

- **`OnceLock` 等已被更优类型替代的**：本任务前必须先确认目标类型没有被更好的并发原语替代（如 `AtomicU64`、`OnceLock`）
- **类型内部已带同步原语的**：如果 `static mut` 持有 `Mutex`/`RwLock`，迁移到 `AssumeSyncCell` 是退化（去掉了一层 sync），应跳过
- **真正跨 CPU 共享的可变状态**：必须用原子类型或显式 lock，`AssumeSyncCell` 不适用（它的安全契约就是"单线程"）

###### 10.6 为什么不在本次一起做

- **boot 阶段 `unsafe` 审计成本高**：OpenSBI 阶段、kernel 早期 init 的内存安全论据需要逐条审查，独立 PR 更安全
- **kernel 还在重构**：本目录 §9 列出的 `minix-kernel` 拆分 panic_impl、cfg gating 等尚未解决，等 kernel crate 稳定后再统一处理
- **风险局部化**：本次 `minix-platform` 重构是已知安全的（只替换 wrapper，行为完全等价），kernel/boot-shim 的 `static mut` 涉及多线程契约，需要单独的设计评审

---

##### 11. QEMU 测试与生产路径不一致：`QemuVirtDesc` 替代了 DTB/ACPI parser

> 来源：`04-platform-discovery.md §3.6` 重写时发现——`QemuVirtDesc` 不是设计缺陷，但**当前测试路径刻意走它**而非 DTB/ACPI parser，这违背了"test what you fly"原则。

###### 11.1 问题陈述

**事实链**：

1. QEMU `virt` 机器**本身就提供 DTB/ACPI**——aarch64/riscv64 提供 DTB（GICv3/PLIC 基地址、virtio-mmio 设备），x86-64 提供 ACPI 表（RSDP→XSDT→MADT）。QEMU 在启动时把这些嵌入固件接口，boot-shim 完全有条件拿到。
2. 当前 `boot-shim` **在某些测试路径显式构造 `platform_descriptor: None`**（见 `os/boot-shim/src/opensbi_helpers.rs:535`、`os/boot-shim/src/uefi_helpers.rs:79` 注释 "use the QEMU fallback"）。
3. `platform::init_from_kinfo` 看到 `None` → 走 `QemuVirtDesc` 兜底路径 → 用硬编码常量填充硬件参数。
4. 结果：**DTB parser（aarch64/riscv64）和 ACPI parser（x86-64）在测试中根本不跑**。

**后果**：

| 后果 | 严重度 |
|------|-------|
| DTB/ACPI parser 有 bug 也发现不了（测试绿但生产挂） | **P0**——隐性故障源 |
| QEMU 升级后 virt 机器布局漂移（例如 PLIC 基地址变了），硬编码常量过时，测试还过——真实硬件走 parser 拿到的是新值，跟测试常量不一致 | **P1**——版本漂移 |
| 测试覆盖率统计失真（parser 主路径 0% 覆盖，但报告里看不出来） | **P1**——决策失据 |
| `04-platform-discovery.md §3.6` 原表述 "QEMU 测试不需要 DTB/ACPI 解析器" 是**因果倒置**——不是"不需要"，是"故意不用"，掩盖了上面的问题 | **P0**——文档误导 |

###### 11.2 触发条件

**QEMU 测试路径故意走 `QemuVirtDesc` 的代码位置**：

> ⚠️ **RCPD 过时标注（2026-08-13, Task 4 回归）**：下表为 2026-07 分析时的位置清单，行号与文件已过时——`os/arch/src/{x86_64,riscv64,arm64}/proc_arch.rs` 已重构删除（`platform_descriptor` 已不在 arch crate）；opensbi/uefi_helpers 行号已漂移。历史记录保留，不再作为有效引用。

| 文件 | 行号 | 上下文 |
|------|------|--------|
| `os/boot-shim/src/opensbi_helpers.rs` | 535 | `None, // platform_descriptor` 测试用 `KernelInfo` 构造 |
| `os/boot-shim/src/opensbi_helpers.rs` | 305 | `"Passing 0 is equivalent to 'no DTB available' (kernel uses QEMU fallback)"` |
| `os/boot-shim/src/uefi_helpers.rs` | 79 | `"use the QEMU fallback"` 注释 |
| `os/arch/src/x86_64/proc_arch.rs` | 350 | mock 路径，`platform_descriptor: None` |
| `os/arch/src/riscv64/proc_arch.rs` | 252 | mock 路径 |
| `os/arch/src/arm64/proc_arch.rs` | 279 | mock 路径 |

###### 11.3 修复方向

**目标**：让 QEMU 测试走完整的 boot-shim → kernel → `platform::init_from_kinfo` → DTB/ACPI parser 主路径，跟真实硬件完全一致。

**步骤**：

1. **boot-shim 改造**：在 `find_platform_descriptor()` 中确认 QEMU 提供的 DTB/RSDP 一定可拿到（即便在测试固件中），而不是仅在某些路径返回 `Some(...)`，另一些路径返回 `None`。
2. **替换 `None` 为 `Some`**：
   - `os/boot-shim/src/opensbi_helpers.rs:535` 测试用 `KernelInfo` 改为传 `Some(PlatformDescriptorPtr::Dtb(dtb_phys))`
   - 三个 `proc_arch.rs:350/252/279` 的 mock 路径改为传真实 QEMU 提供的 DTB/RSDP
3. **DTB/ACPI parser 验证**：跑通一次完整 QEMU 测试，验证 parser 正确解析 QEMU 提供的 DTB/ACPI——这一步可能暴露 parser 的既有 bug（这正是目的）。
4. **修复 parser bug**：如果在 11.3.3 暴露 parser bug，修复并加单测覆盖。
5. **`QemuVirtDesc` 角色回归**：修复后 `QemuVirtDesc` 应该**仅在 boot-shim 完全失败时**作为最后兜底（panic 前还能输出一点诊断）。如果新路径下 `QemuVirtDesc` 永远不会被触发，那是好事——说明 boot-shim 总是能正常工作。
6. **更新 §3.6**：完成后删除 §3.6 的"已知缺陷"标注，因为问题已解决。

###### 11.4 优先级

**P0**——这是隐性故障源，会让 parser 的 bug 偷偷溜过去。修复工作量不大（主要是 boot-shim 调整 + parser 验证），但需要先把 §9.3（`os/plat` 拆分未完成）一并处理，否则 test-kernel 无法改 import 路径。

###### 11.5 风险与缓解

| 风险 | 缓解 |
|------|------|
| 修复后测试大规模失败（parser bug 暴露） | 这是预期收益，不是风险；记录并修复 |
| QEMU 不同版本提供的 DTB/ACPI 字段有差异 | 锁版本（CI 用固定 QEMU 版本），并在 parser 中容忍未知字段 |
| boot-shim 在某些 firmware 配置下确实找不到 DTB/RSDP | `QemuVirtDesc` 兜底保留——这是它的合法用途 |
| 修改波及 17 个 test-kernel（todo.md §9.3） | 与 §9.3 同步处理，合并 PR |

###### 11.6 与其他章节的关系

- **§9.3**（`os/plat` 拆分未完成）：本次修改需要 test-kernel 改 import 路径（从 `minix_plat::arm64` 等迁移到 `minix_arch::arch::*`），应在 §9.3 解决时同步做。
- **§11.3.6** 完成后，**§3.6 的"已知缺陷"标注**应同步删除——避免文档与代码现实脱节。

---

##### 12. 07-cross-space-init 文档 review 待办（来自 2026-06-21 深度 review）

> 来源：`07-cross-space-init.md` 深度 review（2026-06-21）
> 文档语义范围：Phase D — `arch_post_init` + `memory_init`，设置 ptproc、分配 freepdes 槽位。

###### 12.1 [P0] §5 测试缺口 — ✅ **已修复（2026-06-21）**

**问题**：`07-cross-space-init.md §5` 列出了 13 项测试，此前仅有 2 项已实现，其余 11 项为 TODO。

**修复结果**：13 项中 11 项已实现（✅），1 项已有覆盖（✅），1 项 DEFERRED。

**已补测试分布**：

> **2026-08-17 变更（Action Item #1）**：下表测试对应的机制（`PostInitArch`/`MemoryInitArch`/`FreePdeSlots`/`FREE_UPPER_IDX`）已被 Direct Map 取代并删除，下表中的 5.1/5.2/5.3 测试随类型删除一并移除（`test_set_ptproc_*`/`test_allocate_free_pdes_*`/`test_free_pde_slots_*`/`test_advance_free_upper_idx`/`test_createpde_does_not_reallocate_slots`/`test_init_post_and_memory_phase_d_dependencies`）。07 文档 §5.2 以"类型不存在"的编译期断言替代（见 [07-cross-space-init.md §5.2](07-cross-space-init.md)）。

| §5 子节 | 测试函数 | 位置 | 状态 |
|---------|---------|------|------|
| 5.1 | `test_set_ptproc_accepts_valid_vm_page_table_info` | `arch/src/{x86_64,arm64,riscv64}/post_init.rs` | ✅ → 2026-08-17 删除（类型废弃） |
| 5.2 | `test_memory_init_arch_allocates_two_consecutive_pdes` | `arch/src/arch/post_init.rs` | ✅ → 2026-08-17 删除（类型废弃） |
| 5.2 | `test_memory_init_arch_advances_free_upper_idx_by_two` | `kernel/src/lib.rs` (`test_advance_free_upper_idx`) | ✅ 已有 → 2026-08-17 删除 |
| 5.2 | `test_allocate_free_pdes_panics_on_overflow` | `arch/src/{x86_64,arm64,riscv64}/post_init.rs` (per-arch) | ✅ → 2026-08-17 删除（类型废弃） |
| 5.2 | `test_free_pde_slots_new_is_empty` | `arch/src/arch/post_init.rs` | ✅ → 2026-08-17 删除（类型废弃） |
| 5.2 | `test_free_pde_slots_push_*` | `arch/src/arch/post_init.rs` | ✅ → 2026-08-17 删除（类型废弃） |
| 5.3 | `test_init_post_and_memory_phase_d_dependencies` | `kernel/src/lib.rs` | ✅ → 2026-08-17 删除（机制废弃） |
| 5.3 | `test_free_pde_slots_global_persists_after_init` | `kernel/src/lib.rs` (`test_free_upper_idx_starts_at_zero`) | ✅ 已有 → 2026-08-17 删除 |
| 5.3 | `test_createpde_does_not_reallocate_slots` | `kernel/src/lib.rs` | ✅ → 2026-08-17 删除（机制废弃） |
| 5.1/5.3 | ptproc 未初始化 / virt_root=None / mem_clear_mapcache | — | DEFERRED（见 §12.2） |

**编译验证**（2026-06-21，删除前）：
- `cargo test -p minix-arch --features mock --lib -- post_init::tests` / `x86_64::post_init::tests` → OK
- `cargo test -p minix-kernel --features mock --lib` → OK (test_createpde + test_phase_d_dependencies)

**编译验证**（2026-08-17，删除后）：
- `cargo test -p minix-arch -p minix-kernel` → OK（610 + 2 通过，0 失败）
- `rg "PostInitArch|MemoryInitArch|FreePdeSlots|VmPageTableInfo" os/ -t rust` → 0 matches

###### 12.2 [P1] ptproc per-CPU 变量未实现（架构 TODO）

**问题**：`PostInitArch::set_ptproc()` 当前在三个架构实现中都只是占位（`let _ = vm_page_table`），未实际存储 ptproc 指针。这意味着 `createpde()` 等后续依赖 ptproc 的功能无法工作。

> **2026-08-17 更新（Action Item #1）**：arch 层 `PostInitArch` 及三架构占位实现已删除——`createpde` 已被 Direct Map 取代，arch 级 ptproc 不再需要。kernel 级 ptproc 跟踪由 `CURRENT_PTPROC_NR` 全局（`set_current_ptproc_nr(VM_PROC_NR)`，[07 §1.4](07-cross-space-init.md)）承担；SMP 落地时迁移到 `CpuLocal::ptproc`（见 [16-smp.md §D8](16-smp.md)）。

**位置**（已删除，2026-08-17）：
- ~~`os/arch/src/x86_64/post_init.rs:46-58`~~
- ~~`os/arch/src/arm64/post_init.rs:38-48`~~
- ~~`os/arch/src/riscv64/post_init.rs:42-52`~~

**修复方向**：
1. 在 arch 层添加 `pub fn ptproc() -> Option<*const KProcess>` 和 `pub fn set_ptproc_value(...)` 自由函数（封装 per-CPU 访问的 `AtomicPtr` 或 `[UnsafeCell<Option<usize>>; MAX_CPUS]`）。
2. `PostInitArch::set_ptproc(&vm_page_table)` 改为：`minix_kernel::set_ptproc_value(vm_page_table.phys_root);` 把页表物理地址存储为全局变量，供 `createpde()` 读取。
3. 加测试验证 set/get 配对。
4. 与 §10 `static mut` 收编到 `AssumeSyncCell` 协同处理：ptproc 单写多读场景很适合 `AssumeSyncCell<Option<usize>>`。

###### 12.3 [P1] §3.5 BKL 安全论证与实际实现不一致 — ⏸ **短期已修，长期 DEPENDS §12.2**

**问题**：§3.5 论证"ptproc 是 per-CPU 变量"但 ptproc 当前未实现。

**短期修复**：已在 §3.5 末尾追加"实现状态"段落（含 freepdes 已实现 / ptproc per-CPU 未实现的标注、BKL 论证、当前局限性、差异追踪表）。参见 `07-cross-space-init.md §3.5` 末尾。

**长期**：§12.2 完成后重写 §3.5 让 Rust 实现与论证对齐。

###### 12.4 [P2] §4.5 "KernelState 聚合类型" 与实现不一致（已修）

**问题**：原 §4.5 显示 `static mut KERNEL_STATE: Option<KernelState>` 聚合类型。实际代码是分散的 `static mut FREE_PDE_SLOTS` / `static FREE_UPPER_IDX: AtomicUsize`。

**修复**：已在本轮 review 重写 §4.5，使用与代码一致的分散 static 模式 + 解释为何不聚合。

###### 12.5 [P2] §4.4 `init_post_and_memory` 示例代码陈旧（已修）

**问题**：原示例代码使用 `PhysBytes(0)/VirBytes(0)` 硬编码 + `_free_pde_slots` 被丢弃。实际代码已演进为：从 `proc_table.get(VM_PROC_NR).p_seg` 读取 + 存储到 `FREE_PDE_SLOTS` 全局。

**修复**：已在本轮 review 用实际 `lib.rs:910-980` 代码替换示例。

###### 12.6 [P2] §2.3 IPCNAME 宏位置错误（已修）

**问题**：原文"IPCNAME 宏（定义在 `kernel/ipc.h`）" — 实际定义在 `minix3/minix/kernel/main.c:277`，调用方在 `main.c:285-290`。同时代码块中 IPCNAME 调用使用了两参数形式（`IPCNAME(SEND, 0)`），实际是一参数（`IPCNAME(SEND)`），且 6 个调用而非 "约 13 个"。

**修复**：已在本轮 review 改为正确的 IPCNAME 宏定义 + 调用代码块 + 注明实际 6 个调用。

###### 12.7 [P2] §4.1 FreePdeSlots 方法签名陈旧（已修）

**问题**：原示例显示 `push()` 返回 `Result<(), &'static str>`、`get()` 之外无其他方法。实际是 `push() -> Result<(), usize>`（Err 返回 pde_index）、并有 `len()/is_empty()/iter()`。

**修复**：已在本轮 review 用实际 `post_init.rs:109-160` 代码替换示例。

###### 12.8 [P2] cpulocals.h 行号微偏（已修）

**问题**：原文 `cpulocals.h:55`，实际 `ptproc` 字段在第 56 行。

**修复**：已在 §2.1 与 §6 参见章节更新为 `:56`。

---

##### 13. Rust 阶段 B 页表重建决策 — 需再次 review

> 来源：`06-proc-init-boot-proc.md` §1.5（2026-06-22 review 提出）

**背景**：

Minix3 C 版的页表生命周期：

| 阶段 | C 行为 | Rust 行为 |
|------|--------|-----------|
| pre_init | `pg_identity()` + `pg_mapkernel()` + `pg_load()` 建立初始页表 | 同 C |
| 阶段 B（`prot_init()`） | `pg_clear()` + `pg_identity()` + `pg_mapkernel()` + `pg_load()` **重建**页表 | **不重建**，直接复用 pre_init 页表 |
| 阶段 C（`arch_boot_proc(VM)`） | `pg_map(PG_ALLOCATEME, ...)` 分配器选帧添加 VM 映射 | **架构演进（2026-09-01，见 [`frame.rs`](file:///os/arch/src/arch/frame.rs)）**：`VmBootAllocator` 分配物理帧 + `Paging::map(vm_va → pa)`；与 C 语义目标一致（PA≠VA），实现位置不同 |

C 版重建的动机（`protect.c:357-358` 注释）："Set up a new post-relocate bootstrap pagetable so that we can map in VM, and we no longer rely on pre-relocated data."

**当前 Rust 决策**：阶段 B 不重建页表，直接复用 pre_init 页表。

**待决问题**：

- [ ] **决策是否合理？** 需要核对以下上下文后再次 review：
  1. Rust pre_init 阶段是否也存在"pre-relocated data 依赖"问题？如果存在，不重建会导致相同问题
  2. Rust pre_init 阶段是否已经做了"post-relocate"（重定位后再建页表）？如果是，不重建是合理的
  3. Rust 阶段 B 的代码路径（`prot_init` 或 `cstart`）是否有重定位发生？如有，是否需要在阶段 B 重建以匹配 C 行为
  4. 不重建是否有性能/正确性优势？或仅是实现简化？

- [ ] **决策依据缺失**：当前文档中"Rust 版不重建"的描述仅一句话（§1.5 注释），缺乏：
  - 为什么 Rust 不需要重建的论证
  - 是否有测试验证不重建后阶段 C 添加 VM 映射正常工作
  - 与 C 行为的语义等价性论证

**下一步**：

- 核对 Rust `pre_init()` 实现，确认是否已 post-relocate
- 核对 Rust `cstart()` / `prot_init()` 是否涉及重定位
- 补充决策依据到 `06-proc-init-boot-proc.md` §1.5 或 `02-higher-half-kernel.md`
- 若决策确为合理，更新文档明确说明；若不合理，需实现页表重建逻辑

---

##### 14. Task 1: C 源码覆盖扫描 + 28-30 新文档补足（2026-08-12 完成）

> 来源：用户任务 1 — "03-stage-kernel 目录下，01~27 文档关联的 rust 实现，对比 minix3 的 c 实现，是否有任何遗漏的地方？若有，则新建文档 28,29 等等，补足遗漏。"

**执行结果**：

| 阶段 | 内容 | 状态 |
|------|------|------|
| Phase A | 运行 coverage-extract.py 扫描模块级 C 符号覆盖 | ✅ 完成 |
| Phase A | 识别 3 个完全缺口源文件（usermapped_data.c / debug.c / profile.c） | ✅ 完成 |
| Phase A | 生成 NEW-DOCS-CANDIDATES.md 候选清单 | ✅ 完成 |
| Phase B.1 | 新建 28-usermapped-data.md（含 design/outline/scan/SYMBOLS/structure/VERIFY-CHECK） | ✅ CONVERGED |
| Phase B.2 | 新建 29-kernel-debug.md（含 design/outline/scan/SYMBOLS/structure/VERIFY-CHECK） | ✅ CONVERGED |
| Phase B.3 | 新建 30-kernel-profile.md（含 design/outline/scan/SYMBOLS/structure/VERIFY-CHECK） | ✅ CONVERGED |
| Phase B.4 | 31+ arch 文档决策：不新建（ARCH-stub 已分散在各 doc 内） | ✅ 完成 |
| Phase C | 重新跑 coverage 验证：31.2% → 33.8%（+33 symbols） | ✅ 完成 |
| Phase C | 三文档 semantic gaps 全部 = 0 | ✅ 完成 |
| Phase C | 更新 checklist.md / 00-kernel-overview.md / STATE.md / todo.md | ✅ 完成 |

**Blocker Gates**：28/29/30 三文档全部 0/A/B/C/D/D-6/E/G/H PASS

##### 15. Task 1 复扫（2026-08-13）：FPU 子系统缺口 → doc 31 + FpuTrap 实现

> 来源：用户任务 1 重新发起——"01~30 文档关联的 rust 实现 vs minix3 C 实现，是否有遗漏？若有，新建 31,32 等补足"。
> 2026-08-12 的 31+ 决策为"不新建"（ARCH-stub 已分散），本轮按用户要求**重新扫描**（含 28-30 引入后的复扫）。

**执行结果**：

| 项 | 内容 | 状态 |
|----|------|------|
| 复扫 | coverage-extract 重跑：1306 C 符号，doc 覆盖 446 (34.2%)，semantic gaps 0 | ✅ |
| 缺口判定 | 唯一真实 OS 知识点缺口 = **FPU 子系统**（arch_system.c fpu 函数族 / proc.c copr_not_available_handler / exception.c enable+disable_fpu_exception / do_sigsend.c save_fpu 路径在 01-30 全部无文档）；arch/earm + arch/i386 legacy = 范围外（2026-08-12 决策）；prepare_shutdown/debug 工具 = 已记录缺口/琐碎跳过 | ✅ |
| 新建 doc 31 | `31-fpu-context-switching.md`（Ch1 概念 6 节 / Ch2 C 全析 9 节 / Ch3 设计 6 决策 / Ch4 实现 5 节 / Ch5 测试 / Ch6 参见） | ✅ CONVERGED |
| 快照 | `31-outline.v1.md` + `31-outline-review.v1.md` + `31-design.v1.md`（Step 0.3 嵌入生成） | ✅ |
| 代码实现 | **FIX-31-1**：`ExceptionOutcome::FpuTrap` + vector 7 && is_user 特判 + 2 新测试（exception_dispatcher.rs）→ 167/167 tests pass | ✅ |
| 文档同步 | doc 14 §4.2 分发片段 + §4.5 注释同步 vector 7 特判；dispatcher 过时 doc 引用修复（Pattern #76）；fpu_arch.rs 注释行号修复（Pattern #77，lib.rs:1382→:1805） | ✅ |
| 显式缺口 | lazy-restore 主体（待异常交付路径接线）+ 信号路径 FPU 保存（D6，Task 3 候选）——31 doc §4.5 标注 | ✅（forward reference 合规） |
| Blocker Gates | 31 文档 0/A/B/C/D/D-6/E/G/H 全 PASS；VERIFY-CHECK consistency 100% | ✅ |

**遗留债务（本轮记录，不阻塞）**：docs 15/17-27/28/29/30 的 `.design/` 快照缺失
（前 session 协议未走；28-30 的 review 产物齐全）。已登记批量豁免，Proposal #18
（Step 0.3.3 批量补齐）待用户确认后统一执行。不可泛化为其他文档免快照（模式 71 DOG）。

**新文档决策摘要**：

| Doc | 决策 | 理由 |
|-----|------|------|
| 28-usermapped-data | WONTFIX | 64-bit 重写不保留 `.usermapped` 段；数据通过 `sys_getinfo` 获取（D1） |
| 29-kernel-debug | Partial+ | **✅ runqueues_ok / print_proc / rtsflagstr 已实现 (Phase 8, 2026-08-13)**；BKL timing 用 `debug_assert!` + `log!` 替代；非生产路径 |
| 30-kernel-profile | Partial+ | profile clock interface 保留；**✅ sample collection 已实现 (Phase 8, 2026-08-13)**；NMI profiling WONTFIX（D2/D3/D4） |

**剩余缺口（不新建文档的理由）**：

- `arch/earm/bsp/ti/omap_*.h` BSP 寄存器定义 (286+) — 硬件特定，OUT OF SCOPE
- `arch/i386/` 历史架构代码 (210+) — x86_64 有独立实现，ARCH-stub 化
- 编译时配置宏 (DEBUG_*/VF_*) — 已在 29-kernel-debug.md 语义覆盖
- DEFERRED 函数 — 已在现有文档中标注

**Task 1 状态**：✅ **CONVERGED** — 进入 Task 2（Rust 代码改进扫描）

---

##### 16. Task 2 反向覆盖（2026-08-13）：StacktraceArch 零文档缺口 → doc 32

> 来源：用户任务 3——"kernel 相关 Rust 实现（涵盖 OS 知识点）未被 01~30 覆盖？未覆盖则补足"。
> 方法：`find os/ -name *.rs` × 文档文件名引用交叉（systematic reverse scan）。

**执行结果**：

| 项 | 内容 | 状态 |
|----|------|------|
| 反向扫描 | 13 个未覆盖 .rs 文件分类：真实知识点缺口 1（stacktrace.rs）/ 概念已覆盖 4（errno.rs、ipc/notify.rs、types/address.rs、types/com.rs）/ 超出内核范围 4（ipc/kernel.rs 仅 PM 用、ipc/pm.rs、ipc/vfs.rs、types/{id,pid,bitmap}.rs）| ✅ |
| 缺口判定 | **stacktrace.rs（StacktraceArch，113 行 trait + 2 impl + 默认 walk_frames 实现）有 Rust 实现零文档**——01-30 文件名 0 命中；doc 27 "util_stacktrace 未实现" 仅指内核栈 panic 版本（进程栈 proc_stacktrace 语义未覆盖）。OS 知识点 = 栈回溯机制，非 trivial | ✅ |
| 新建 doc 32 | `32-stack-tracing.md`（409 行，自包含：Ch1 概念 6 节 / Ch2 C 5 节 / Ch3 设计 D1-D7 / Ch4 实现 5 节 / Ch5 测试 / Ch6 参见）| ✅ CONVERGED |
| 快照 | `32-outline.v1.md` + `32-outline-review.v1.md` + `32-design.v1.md`（Step 0.3 嵌入生成）| ✅ |
| 代码实现 | **FIX-32-1**：x86_64/boot.rs 新增 stacktrace_tests 模块 5 测试（链遍历/循环检测/读失败/上限 32/GP_RBP 索引）→ 172/172 tests pass | ✅ |
| 文档同步 | FIX-32-2 4 处修正（trait span :41-113 / 172 passed 总数 / §5.1 缺 caps_at_max 行 / 测试数表述）| ✅ |
| Blocker Gates | doc 32 0/A/B/C/D/D-6/E/G/H 全 PASS；VERIFY-CHECK consistency 100%（14 项 grep 重放）| ✅ |
| 遗留 | ~~riscv64 StacktraceArch impl 缺失~~ → **FIX-32-2 已实现（2026-08-13，boot.rs:182-198，Task 3）**；DIAGCTL STACKTRACE 未接线（syscall.rs:670 ENOSYS，doc §4.5 forward reference）| ⏸ 接线仍遗留 |

**Task 2 状态**：✅ **CONVERGED** — 进入 Task 3（卓越性 2nd-pass）

---

##### 17. Task 3 卓越性 2nd-pass（2026-08-13）：clippy 清零 + Pattern #76 全量复扫

> 来源：用户任务 2——"对照 Redox 与 Rust 社区最佳实践，修复所有改进点；修复一项标注一项"。
> 原则：每个修复标注 ID + file:line + 反查维度；关联文档同步。

**执行结果**：

| 改进项 | 修复内容 | 状态 |
|--------|---------|------|
| **FIX-T3-1** clippy E0133 批量 | `minix-plat` 43 处 `unsafe_op_in_unsafe_fn`（Rust 2024）→ `cargo clippy --fix` 包 unsafe block（plat/src/x86_64/early_console.rs ×9 + interrupt.rs ×21 + 其余） | ✅ |
| **FIX-T3-2** missing_safety_doc | `plat/src/x86_64/interrupt.rs:141-160` `lapic_id`/`ioapic_version` 补 `# Safety` 段（MMIO 地址有效性 + BKL 论证） | ✅ |
| **FIX-T3-3** unnecessary_parens | `arch/src/x86_64/paging.rs:218,323` `((vaddr >> 12) & 0x1FF)` → `(vaddr >> 12) & 0x1FF` | ✅ |
| **FIX-T3-4** Default impl | `arch/src/arch/post_init.rs:109-113` `FreePdeSlots` 补 `impl Default`（委托 `Self::new()`，与 FaultContextTracker 模式一致） | ✅ |
| **FIX-T3-5** doc indent | `arch/src/x86_64/trap_entry.rs:12,14` doc list item 缩进（markdown 续行 4 空格） | ✅ |
| **FIX-T3-6** same-type cast | `arch/src/x86_64/fpu.rs:102,120` `as *mut u8`/`as *const u8` 移除（as_mut_ptr/as_ptr 已返回目标类型） | ✅ |
| **FIX-T3-7** field_reassign | `arch/src/x86_64/signal.rs:221-259` `build_sigcontext` 30 字段连续赋值 → struct literal 全字段显式赋值（clippy struct update no effect 证明全覆盖，语义 = 原零初始化 + 赋值） | ✅ |
| **FIX-T3-8** result_unit_err | `arch/src/arch/clock.rs:142` `init_profile_clock` + `arch/src/arch/boot.rs:211` `write_user_register` → `#[allow(clippy::result_unit_err)]` + TODO(C-D-5) 标注（trait 契约，设计级改动不在此轮） | ✅（allow 标注） |
| **FIX-T3-9** Pattern #76 复扫 | **90 处 doc 引用失效修复**：`06-design-final.md`×42 → `06-design.v1.md`（章节号验证 D1-D8/§2.x/§3.x/§4.x，5 处不存在章节 §5/§12.5/§12.9/§15.5 → D7/§3.3/§3.10 语义修正）；`15-design.md`×9 → `15-clock-timer.md`；编号漂移×12 类（04-clock→05、19-syscall-device→20、18-syscall-signal→19、18-syscall-device→20、17-syscall-copy→18、16-syscall-process→17、07-scheduling→11、05-exception→14、06-arch-post-init→08、05-proc-init→06、08-vm-boot→09、02-page-table→02-higher-half）；语义映射×3（`03-vm-request`→`24-cross-space-runtime` §2.7/§2.8/§4.3、`08-proc-macros`→`06-proc-init-boot-proc` §3.1/§5.2、`01-bug`→`02-higher-half-kernel` 附录 A） | ✅ |
| **FIX-T3-10** SAFETY 注释审计 | krandom.rs try_krandom/krandom（BKL 论证 + addr_of_mut!）、misc.rs SPROF statics（BKL 论证）、arm64/smp.rs GICD_BASE（单线程早期启动论证）、boot-shim 3 处 static mut（exactly-once + # Safety）→ 全部论证齐全 | ✅ |
| G3 信号路径 FPU 保存 | 31 doc §4.5 显式缺口（do_sigsend.c:86 save_fpu 到 sigcontext）——涉及 sigcontext 布局变化 + 传递路径接线 = 设计级新功能 | ⏸ 保持 forward reference（31 doc 已诚实标注，入 backlog） |

**验证（2026-08-13）**：
- `cargo clippy -p minix-kernel -p minix-arch -p minix-plat -p minix-types -p minix-platform -p minix-boot` → **0 warnings**（除 workspace profiles 配置层）
- `cargo test` kernel 链全绿：609 kernel + 172 arch + 62 types + 15/17/19/2 其余
- 全 os/ doc 引用 0 MISSING（文件存在性验证）；riscv64 qemu-tests 编译失败 = 预存在问题（stash 验证，backlog）

**Task 3 状态**：✅ **COMPLETED** — 进入 Task 4（收尾回归 review 01~30）

---

##### 10. Task 2-4 完成状态（2026-08-13）

###### Task 2: 12-ipc-core P0 修复（5 P0 + 6 P1）

| P0 ID | 描述 | 状态 |
|-------|------|------|
| P0-12-1 | 14 个 P0 测试路径补齐 | ✅ 已修复 |
| P0-12-2 | receive Phase 2 async 完整实现 | ✅ 已修复 |
| P0-12-3 | do_ipc SENDA table 完整实现 | ✅ 已修复 |
| P0-12-4 | IpcEngine<'a> + SenderQueue(VecDeque) 对齐 design | ✅ 已修复 |
| P0-12-5 | REPLY_PEND 正确跳过 notify 但不跳过 async/caller_q | ✅ 已修复 |

**12-ipc-core 状态**：✅ **CONVERGED**（Session #24, 2026-08-13）— VERIFY-CHECK-12.md PASS

###### Task 3: krandom 文档同步（反向覆盖）

| 文档 | 修复内容 | 状态 |
|------|----------|------|
| 25-misc-unported.md | §4.7 krandom 子系统接入描述 | ✅ 已修复 |
| 08-system-init-boot-finish.md | krandom_init() 已实现标注 | ✅ 已修复 |
| 14-exception-interrupt.md | get_randomness no-op stub 标注 | ✅ 已修复 |
| 20-syscall-device.md | generic_handler get_randomness 标注 | ✅ 已修复 |
| checklist.md | G-026/D-16 状态更新 | ✅ 已修复 |

###### Task 4: 卓越性全量 2nd-pass（clippy 清理）

| 改进项 | 修复前 | 修复后 | 状态 |
|--------|--------|--------|------|
| clippy --fix 自动修复 | 272 warnings | 75 warnings | ✅ |
| unnecessary unsafe block (Rust 2024 union write) | 96 | 0 | ✅ |
| field_reassign_with_default (test code) | 109 | 0 (crate-level cfg(test) allow) | ✅ |
| dead_code (ipc_filter_check + IpcFilterElFlags) | 2 | 0 (#[allow] + reason) | ✅ |
| result_unit_err (clock/vm) | 2 | 0 (#[allow] + reason) | ✅ |
| too_many_arguments (grant/kpriv C-mirrored) | 2 | 0 (#[allow] + reason) | ✅ |
| should_implement_trait (proc from_str) | 1 | 0 (#[allow] + reason) | ✅ |
| if_same_then_else (syscall_copy) | 1 | 0 (#[allow] + reason) | ✅ |
| needless_range_loop (memmap) | 1 | 0 (iter_mut().enumerate()) | ✅ |
| assertions_on_constants (lib.rs) | 1 | 0 (const { assert! }) | ✅ |
| unusual_byte_groupings (memmap hex) | 1 | 0 (regrouped) | ✅ |
| map→if let (proc_table) | 1 | 0 | ✅ |

**kernel crate clippy**：✅ **0 warnings**（arch/plat/types 的 78 warnings 是 Rust 2024 unsafe_op_in_unsafe_fn，独立迁移任务）

**测试验证**：✅ **608/608 tests pass**，0 failures

###### Phase 5: 收尾回归

- 12-ipc-core per-doc gates 升 CONVERGED ✅
- 15/26/27 文档存在且内容完整（26=WONTFIX）✅
- Phase 4 代码改动仅影响测试代码 + #[allow] 属性，无生产签名变化 ✅
- CONVERGED 文档不受影响 ✅

**整体状态**：✅ **Phase 1-5 全部完成**

---

##### 11. 遗留项（Clippy Round 2 — Mechanical Cleanup，未执行）

> **来源**：`~/.trae/documents/clippy-round2-mechanical-cleanup-plan.md`（2026-08-13 制定，未实施）
> **背景**：Round 1 完成后 `minix-kernel` 仍有 47 个 clippy warnings。Round 2 计划范围限定为**纯机械修复**（不改语义/签名/类型），其余需设计决策的项已排除。
> **状态**：⏸️ **DEFERRED**（用户决定不修，登记后续处理）

###### 11.1 Round 2 已排除项（需设计决策，**排除**）

| 警告 | 位置 | 排除原因 |
|------|------|----------|
| `too_many_arguments` (10/7) | `os/kernel/src/grant.rs:348` `verify_grant` | 需引入参数 struct → 签名变化 |
| `too_many_arguments` (8/7) | `os/kernel/src/kpriv.rs:793` `configure_boot_priv` | 需引入参数 struct → 签名变化 |
| `should_implement_trait` | `os/kernel/src/proc.rs:1034` `from_str` | 应实现 `std::str::FromStr` trait → API 重设计 |
| `if_same_then_else` | `os/kernel/src/syscall_copy.rs:829-832` | 两分支均 `return EINVAL` → 可能是 bug 或有意为之（C-ref: `do_umap_remote.c:57-66`） |
| `result_unit_err` | `os/kernel/src/vm.rs:642` `enqueue_and_notify` | `Result<(), ()>` → 自定义错误类型 → API 变化 |
| `result_unit_err` | `os/kernel/src/clock.rs:289` `init_profile_clock` | 同上 |

###### 11.2 Round 2 计划执行的机械修复（已随整体 Phase 4 阶段完成，故跳过）

| 任务 | 范围 | Round 2 计划 | 实际状态 |
|------|------|-------------|----------|
| Task A | `no_effect` (1 处) | 移除 `();` 空语句 | ✅ 已完成（Phase 4 `syscall_copy.rs`） |
| Task B | `field_reassign_with_default` (3 处) | 转 struct literal | ✅ 已完成（Phase 4 通过 crate-level `cfg(test)` allow 抑制） |
| Task C | `needless_range_loop` (5 安全子集) | 转 `iter().take()` | ✅ 已完成（Phase 4 `memmap.rs` + 其他） |
| Task D1 | 真正死代码移除 (~6 处) | grep 验证后删除 | ✅ 已完成（Phase 4 已审查） |
| Task D2/D3/D4 | `#[allow(dead_code)]` 或 `#[cfg(test)]` (~15 处) | 加属性 + 注释 | ✅ 已完成（Phase 4 `ipc_filter_check`/`IpcFilterElFlags`/`proc_table::FREE_*` 等） |

**说明**：Round 2 计划是 Phase 4 工作的**子集**。Phase 4 在更广范围完成 clippy 清理（kernel crate 272→0 warnings），因此 Round 2 列出的所有机械项都被一并解决，无需独立执行。

###### 11.3 剩余 clippy 警告分布（2026-08-13 状态 → 2026-08-13 晚 Task 3 清理后）

| Crate | 警告数 | 范围 | 处理建议 |
|-------|--------|------|----------|
| `minix-kernel` | **0** | ✅ 已清理 | — |
| `minix-arch` | **0** | ✅ 已清理（Task 3：10 处，含 E0133 批量 + 括号/Default/doc-indent/cast/signal struct literal） | — |
| `minix-plat` | **0** | ✅ 已清理（Task 3：43 处 E0133 unsafe block 批量 + 2 处 # Safety doc） | — |
| `minix-types` | **0** | ✅ 已清理（Task 2） | — |
| `minix-platform` | **0** | ✅ 已清理（Task 2） | — |
| `minix-boot` | **0** | ✅ 已清理（Task 2） | — |
| `minix-vm` | 116 | VM 服务器 crate（01-stage-kernel 范围外） | backlog（VM stage） |

**验证（2026-08-13）**：`cargo clippy -p minix-kernel -p minix-arch -p minix-plat` → **0 warnings**（除 workspace profiles 配置层警告）。`cargo test` kernel 链全绿（609 kernel + 172 arch + 62 types + 其余）。

###### 11.4 待启动的设计级清理（未来 Round）

**触发时机**：arch 重构完成后启动

| 编号 | 任务 | 描述 | 依赖 |
|------|------|------|------|
| C-D-1 | `verify_grant` 参数 struct 化 | 10 个参数→ 4 个小组，签名变化 | Round 3 完成 |
| C-D-2 | `configure_boot_priv` 参数 struct 化 | 8 个参数 → 2 个小组 | Round 3 完成 |
| C-D-3 | `proc::from_str` → `FromStr` trait | 实现标准 trait，类型变化 | 无 |
| C-D-4 | `syscall_copy` if_same_then_else 调查 | 确认 C 行为，合并 or 显式 `#[allow]` | 无 |
| C-D-5 | `Result<(), ()>` → 自定义错误类型 | `vm::enqueue_and_notify` / `clock::init_profile_clock` / **`arch::clock::init_profile_clock` / `arch::boot::write_user_register`（Task 3 已 `#[allow(clippy::result_unit_err)]` + TODO 标注）** | 无 |

**Task 3 补充记录（2026-08-13，kernel 链 clippy 清零）**：
- 修复项：`plat` E0133×43（--fix 批量包 unsafe block）+ `plat` missing_safety_doc×2（interrupt.rs lapic_id/ioapic_version 加 # Safety）+ `arch` 括号×2（paging.rs:218/323）+ `arch` FreePdeSlots Default（post_init.rs）+ `arch` doc-indent×2（trap_entry.rs）+ `arch` same-type cast×2（fpu.rs:102/120）+ `arch` field_reassign→struct literal（signal.rs build_sigcontext，含移除无效 `..Default::default()`）
- 语义验证：signal.rs struct literal 全字段显式赋值 = 原 memset 零初始化 + 赋值（clippy struct update no effect 证明字段全覆盖）；所有修复 `cargo test` kernel 链全绿
- 未修（设计级，已 allow 标注）：`Result<(), ()>`×2（arch clock/boot trait 契约，C-D-5）

###### 11.5 重启 Round 2 的方式

如需重启：
1. 阅读本章节确认 Round 2 范围
2. 阅读 `~/.trae/documents/clippy-round2-mechanical-cleanup-plan.md` 获取详细 plan
3. 优先修复 Round 3-6（arch/types/platform/boot）后再回到本任务
4. 执行后删除本章节（任务结束）

**Action Item**：本节保留至所有 Round 完成

##### 18. Task 4 收尾回归（2026-08-13）：17 项 doc 引用修复 + 全量回归验证

**范围**：回归 review 01-30 文档 + 关联 Rust 实现的收尾。发现并修复 17 项 P2 doc 引用漂移/状态过时（无 P0/P1 新发现）。

**修复清单**（ID + file:line + 反查维度）：

| # | ID | 文件:行 | 修复 | 反查维度 |
|---|-----|--------|------|---------|
| 1 | FIX-T4-1 | 04-platform-discovery.md:397/401 | `clock.rs:182` → `:72`（ClockArch trait 实际定义位置）| 维度1 outline↔doc |
| 2 | FIX-T4-2 | 04-platform-discovery.md:401 | `interrupt.rs:128` → `:129`（InterruptController trait）| 维度1 |
| 3 | FIX-T4-3 | 04-platform-discovery.md:432 | `boot.rs:386` → `:443`（platform_sources fixture）| 维度5 doc↔code |
| 4 | FIX-T4-4 | 04-platform-discovery.md:432 | lib.rs fixture 行号 8 处 → :2265/2329/2411/2506/2537/2778/2802/2826 | 维度5 |
| 5 | FIX-T4-5 | 04-platform-discovery.md:432 | helpers 329/569 描述修正：生产构造函数参数（uefi:228/opensbi:432），非测试 fixture | 维度5（事实纠正）|
| 6 | FIX-T4-6 | 09-vm-boot-protocol.md:503 | `boot.rs:287-323` → `:290-326`（identity mapping，本 session +3 引入）| 维度6 元层 |
| 7 | FIX-T4-7 | 25-misc-unported.md:708 | `boot.rs:211` → `:214`（write_user_register，本 session +3）| 维度6 |
| 8 | FIX-T4-8 | 02-higher-half-kernel.md:1290 | `os/arch/src/paging.rs` → `os/arch/src/arch/paging.rs` | 维度5 |
| 9 | FIX-T4-9 | 02-higher-half-kernel.md:1291 | `direct_map.rs` 补 `arch/` 前缀 | 维度5 |
| 10 | FIX-T4-10 | 18-syscall-copy.md:607 | 同上 | 维度5 |
| 11 | FIX-T4-11 | 20-syscall-device.md:119 | `os/arch/src/x86_64/port_io.rs` → `os/plat/src/x86_64/port_io.rs`（crate 错位）| 维度5 |
| 12 | FIX-T4-12 | 01-boot-shim-bootstrap.md:1727 | `os/kernel/src/main.rs` 虚构路径 → panic=abort（Cargo.toml:32/37）+ EarlyConsole（plat）准确描述 | 维度5（虚构位置）|
| 13 | FIX-T4-13 | todo.md:145 | `paging_ext.rs` 补 `arch/` 前缀 | 维度5 |
| 14 | FIX-T4-14 | todo.md §4.3 | EarlyConsole 落地位置标注（方案 arch → 实际 plat）| 维度6 |
| 15 | FIX-T4-15 | todo.md §11.2 | RCPD 过时标注（proc_arch.rs ×3 已删 + helpers 行号漂移，Pattern #66）| 维度6 |
| 16 | FIX-T4-16 | checklist.md | syscall_signal.rs 10 处行号 → :107/266/348/437/615（perl 负向前瞻保护 P1-07/P1-08 历史记录）| 维度5 |
| 17 | FIX-T4-17 | checklist.md F-20/F-21 | ⚠️ Partial → ✅ Complete（2026-08-01 已落地 data_copy_vmcheck + sigframe）| 维度5（状态过时）|

**最终回归验证**（2026-08-13）：
- `cargo build`：✅（minix-vm 116 warnings = 已知 backlog，VM stage 范围外）
- `cargo clippy` kernel 链 6 crates：✅ 0 warnings（仅 workspace profiles 配置噪音）
- `cargo test` kernel 链：**894 passed, 0 failed**（kernel 609 + arch 172 + plat 19 + types 62 + platform 15 + boot 17）
- RCPD 复扫（Pattern #66）：100 unique `os/` 路径，5 missing 全部处置（trap_return.rs forward reference 合规 ×1 + todo.md §11.2 RCPD 标注 ×4）
- 收敛成本评估：触发 Step 7.1 停止规则 4（zero-bias——本回归 0 新 P0/P1，仅 P2 引用修复）

**backlog**（移出 Task 4 范围）：minix-vm 116 clippy warnings + riscv64 qemu-tests 编译失败（预先存在 ~250 errors）+ docs 15/17-27/28/29/30 `.design/` 快照缺失（Proposal #18 待用户确认）+ G3 信号路径 FPU 保存（doc 31 §4.5 forward reference）+ DIAGCTL STACKTRACE 接线（syscall.rs:670 ENOSYS）+ C-D-1~5 design-level cleanups（§11.4）。

### H.2 清空归档版（2026-08-14，67 行，完整保留）

> 97df4e58d commit 写入的清空归档（处理摘要表 + 修复记录 + 转移清单），
> 架构审查重写时被替换为 §0-§6，现按"不丢历史内容"要求完整恢复。

### 清空归档版：01-stage-kernel 全局 TODO — 已全部清空（2026-08-14）

> 本文件为历史归档。原列出的全部 todo 均已处理（✅ 已解决 / 🔀 已转移 / 📋 完成记录），
> **无剩余未处理项**。跨阶段未完成项已实体转移到目标文档（见 §3 转移清单）。

#### 1. 处理摘要

| 原章节 | 原内容 | 处置 | 去向 / 证据 |
|--------|--------|------|------------|
| §1 | Boot module 内存回收（Rust 实现） | ✅ 已解决 | boot-shim 加载路径已实现，todo 过时 |
| §2 | EBS 后映射回收 + `Box::leak` 生命周期 | ✅ 已解决 | 01-boot-shim-bootstrap §3.5.1 有意的 0/0 设计决策 |
| §3 | qemu-tests 后续扩展 | ✅ 已解决 | 目录已演进为 22 个按功能命名 test-kernels，代码复用 + run_all.sh 就位 |
| §4 | 页表页分配器 VM 阶段接入 | 🔀 转移 | `02-stage-vm/draft/00-vm-overview.md` §8.4 |
| §5 | code-review 未修复问题（4.1-4.5） | ✅ 已解决 | 全部完成（KernelInfo pub 封装 / HigherHalf trait / kmain_verify / PTE_HUGE_FLAGS / 调试输出） |
| §6.1/6.2 | 异常 / 中断端到端测试（QEMU L4/L5） | 🔀 转移 | 单元层已实现（trap_entry 完整 IDT + 5 测试 / exception_dispatcher 分发测试）；QEMU E2E → `notes/TODO.md` QEMU backlog |
| §6.3/6.4 | init/load 顺序测试 + init_ap 路径 | 🔀 转移 | `16-smp.md` §5.3（x86_64 init_ap 仍为 panic! 占位，protection.rs:370） |
| §6.5/6.6 | aarch64 GICv3 PPI unmask / riscv64 PLIC base | ✅ 已解决 | 已完成 |
| §6.7 | riscv64 PMP 仅配置 entry 0 | 🔀 转移 | 当前行为符合文档描述（allow-all），增强 → `notes/TODO.md` QEMU backlog |
| §6.8 | QEMU GDB 脚本未集成 CI | 🔀 转移 | `16-smp.md` §5.3 |
| §7 | PlatformDesc / 硬件发现 | ✅ 已解决 | 全部完成 + 验证通过（19/19 platform 测试）；AcpiDesc 最小化扩展 → `00-vm-overview.md` §8.4 |
| §8.1/8.2 | 平台发现阶段预存在编译失败 | ✅ 已解决（2026-08-14） | **19/19 test-kernels 三 target 编译通过**，见 §2 |
| §9.0-9.4 | Rust 2024 迁移后残留编译错误 | ✅ 已解决（2026-08-14） | 修复方式见 §2（9.0 原已修，9.1-9.4 本次修复） |
| §10 | `static mut` 收编 AssumeSyncCell | ✅ 已解决 | kernel 4 个 lib.rs 目标已迁移 `.get()`；6 处生产位置 SAFETY 论证齐全（FIX-T3-10） |
| §11 | QEMU 测试与生产路径不一致（QemuVirtDesc P0） | ✅ 已解决 | PlatformDescSource 架构（UEFI RSDP/DTB + OpenSBI `install_dtb_ptr`/`dtb_ptr`）+ doc 04 重写（2026-07-16） |
| §12.1-12.8 | 07-cross-space-init review 待办 | ✅ 已解决 | 12.1-12.8 全部完成；12.2 ptproc / 12.3 BKL 长期 → `16-smp.md` §5.3 |
| §13 | 阶段 B 页表重建决策 | ✅ 已解决 | 02-higher-half-kernel.md:117-119 论证（Rust 内核第一指令即在最终虚拟地址，无 pre-relocated data） |
| §14 | Task 1 C 源码覆盖扫描 + doc 28-30 | 📋 完成记录 | 28/29/30 三文档 CONVERGED |
| §15 | Task 1 复扫：FPU 缺口 → doc 31 | 📋 完成记录 | 31-fpu-context-switching CONVERGED；G3 信号路径 FPU 保存已在 31 §4.5 forward reference |
| §16 | Task 2 反向覆盖 → doc 32 | 📋 完成记录 | 32-stack-tracing CONVERGED；DIAGCTL STACKTRACE **已接线**（syscall.rs:2167 `2 =>` 分支） |
| §17 | Task 3 卓越性 2nd-pass | 📋 完成记录 | kernel 链 clippy 0 warnings；Pattern #76 90 处 doc 引用修复 |
| §10(二) | Task 2-4 完成状态 | 📋 完成记录 | 12-ipc-core CONVERGED（5 P0 + 6 P1 修复）+ krandom 文档同步 + Phase 4 收尾 |
| §11(二) | Clippy Round 2 遗留 | ✅ 已解决 | 全部机械项随 Phase 4 完成；排除项 C-D-1~5 → `notes/TODO.md` kernel 设计级 backlog |
| §18 | Task 4 收尾回归 | 📋 完成记录 | 17 项 doc 引用修复 + 894 passed 回归 |

#### 2. 2026-08-14 修复记录（§8/§9 编译问题）

todo.md §8.1/§8.2/§9.1-9.4 记录的 qemu-tests 编译失败（19 个 test-kernels 三 target），
根因是早期脚手架未跟随 kernel/plat 重构同步。本次修复：

| 问题 | 根因 | 修复 |
|------|------|------|
| §9.1 `E0152 duplicate panic_impl` | 裸机 bin 在 host `cargo test` 下编译 test harness，链接 std（std 已定义 panic_impl） | **19 个 test-kernel Cargo.toml 添加 `[[bin]] test = false`**——裸机固件不编译 host test harness |
| §9.2 `E0425 arch_boot/init_proc_and_boot` cfg 不匹配 | 早期默认 mock feature 拉 std；现 test-kernels 已 `default-features = false` | 裸机 target 编译验证通过（cfg 匹配已正确） |
| §9.3 `E0432 minix_plat::riscv64` 路径不存在 | host 下 `target_arch` 不匹配，riscv64 模块被 cfg 掉 | `test = false` 后 host 不再编译 test harness；裸机 target 下模块存在 ✅ |
| §9.4 `#[global_allocator]` 与 uefi 冲突 | 12 个 test-kernels 声明 `uefi features = ["global_allocator"]`，feature unification 传播到 boot-shim lib test | 移除该 feature（test-kernels 自带 HybridAllocator，改用 `features = ["alloc"]`） |

**验证（2026-08-14）**：
- 19/19 test-kernels 三 target 编译 0 errors（x86_64-unknown-uefi ×7 / aarch64-unknown-uefi ×6 / riscv64gc-unknown-none-elf ×6）
- 全 workspace `cargo test`：kernel 609 + arch 172 + plat 19 + types 62 + platform 15 + boot-shim 26+13 + 其余全绿
  （**唯一例外**：minix-vm 15 failed = **pre-existing**，stash 验证与本次改动无关 → `00-vm-overview.md` §8.4）
- 全 workspace `cargo clippy`：无 error

#### 3. 转移清单（跨阶段未完成项）

| 目标文档 | 追加位置 | 转移项 |
|---------|---------|--------|
| `02-stage-vm/draft/00-vm-overview.md` | §8.4 | 页表页分配器 VM 阶段接入（原 §4）+ minix-vm 116 clippy warnings（原 §11.3）+ **minix-vm 15 测试失败（本次发现，pre-existing）** + AcpiDesc 最小化扩展（原 §7） |
| `01-stage-kernel/16-smp.md` | §5.3 | init/load 顺序测试（原 §6.3）+ init_ap 路径验证（原 §6.4）+ QEMU GDB CI（原 §6.8）+ ptproc per-CPU 语义跟踪（原 §12.2，单核占位 OK） |
| `notes/TODO.md` | kernel 设计级 backlog 段 | C-D-1~5 设计级清理（原 §11.4：verify_grant / configure_boot_priv / FromStr / if_same_then_else / Result<(),()>） |
| `notes/TODO.md` | QEMU 集成测试 backlog 段 | 异常 E2E（原 §6.1）+ 中断 E2E（原 §6.2）+ riscv64 PMP 多 entry 增强（原 §6.7） |

#### 4. 最终验证（2026-08-14）

- `cargo test`（os/ workspace）：全部通过（除 minix-vm pre-existing 15 failed，见上）
- `cargo clippy`（全 workspace）：无 error
- test-kernels 三 target 编译：19/19 × 0 errors
- `git status`：本次改动全部为 tracked 文件修改（`git add -u` 模式提交）

---

## Step 1.0f 发现：代码注释行号漂移（2026-08-14，doc 04 review 顺带发现，Pattern #77）

> 属于 kernel/arch 模块代码注释（非 doc 04 platform 范围），待对应 doc review（16-smp / 08 / 17-19）或独立修复时处理。

| # | 位置 | 注释声称 | 实际 | 漂移 | 修复建议 |
|---|------|---------|------|------|---------|
| 1 | `os/kernel/src/lib.rs:1821` | `CpuLocal::set_running(IDLE) — see smp.rs:200` | set_running 在 `smp.rs:202` | +2（边界）| 200 → 202 |
| 2 | `os/kernel/src/lib.rs:1837` | `ProcessTable::rts_unset auto-enqueues ... (see proc_table.rs:276)` | rts_unset 在 `proc_table.rs:304` | **+28** | 276 → 304 |
| 3 | `os/kernel/src/lib.rs:1931` | `CpuLocal::fpu_presence ... (see smp.rs:134)` | fpu_presence 在 `smp.rs:161-162` | **+27** | 134 → 161 |
| 4 | `os/kernel/src/proc_table.rs:357` | `set_running(IDLE) ... (see smp.rs:127)` | set_running 在 `smp.rs:202` | **+75** | 127 → 202 |
| 5 | `os/kernel/src/syscall.rs:2542` | `message delivery mechanism (see vm.rs:323)` | `os/servers/vm/src/` 下**无 vm.rs**（主文件为 lib.rs/main.rs，lib.rs:323 为空行）| 文件不存在 | 改为指向实际文件或删除引用 |

✅ 准确引用（无需修）：`os/arch/src/x86_64/boot.rs:357` `(see signal.rs:38)` → `os/arch/src/x86_64/signal.rs:38` 正是 `pub(super) const GP_RBP: usize = 5;`

**根因**：与 Pattern #77（doc 08 先例）相同——代码增量后注释未同步。建议批量修复脚本：`rg "see [a-z_/0-9]+\.rs:[0-9]+" os/ -t rust` + sed 逐处验证。

---

## §18.5 Cause_sig 全语义实施的遗留 backlog（2026-09-05 cause_sig-D-6/D-11 收尾）

> 本节为 cause_sig 全语义实现完成（见 §0.1 "2026-09-05"行）后明确登记的**不阻塞收敛** backlog，
> 与 §7.1 表格共同构成完整追踪。三项均为非缺口但仍未落地的实现项，遵循 §0.1 的 "📌 保持 DEFERRED" 范畴。

| 序号 | 关联条目 | 项 | 关联文档 / 代码 | 实施要点 / 备注 |
|------|---------|-----|----------------|-----------------|
| BL-1 | **D-57** | `proc_stacktrace` 接入 `cause_sig` 致命 SELF panic 路径 | doc 19 §4.7 / syscall_signal.rs:249-255 / syscall.rs:2295 / doc 32 | C system.c:429 在 panic 前调 `proc_stacktrace(rp)` 打印进程用户栈；Rust 当前省略，差异已在 doc 19 §4.7 标注。**复用路径**：DIAGCTL_CODE_STACKTRACE 已实现 `cross_space_copy` + `StacktraceArch::walk_frames` 闭包；新助手函数 `proc_stacktrace(&KProcess, &ProcessTable)` 即可 |
| BL-2 | D-13 | `sig_delay_done`（system.c:454-464） | doc 19 §4.7 / T-1 行 | PM 通知接口 + `SIGSNDELAY` 常量；依赖 PM getksig 主路径已就绪（D-21/D-22 同批已落） |
| BL-3 | D-14 | DIAGCTL `send_sig(PM_PROC_NR, SIGKMESS)`（do_diagctl.c:49-56） | doc 19 §4.7 / syscall.rs:2271-2315 / W-7 kmess 体系 ARCH 演进 | 借用冲突注释现存于 syscall.rs:2286（dispatch 已持 caller/priv_table 借用，与查 PM_PROC_NR 全局 proc_table 访问冲突）；PM 下次 getksig 轮询可观察 → 主用途已实现，PM 通知是次要副作用 |

**统一触发条件**（任一出现即启动对应 backlog）：
- 启动 doc 11-scheduling-primitives review（BL-2 因 T-1 链路依赖）
- 启动 doc 16-smp review（BL-3 因 PM 调用主路径 SM 重构）
- 用户态服务进程首次出现"自管理 + 致命信号"路径实证（BL-1）
- 启动 doc 32 卓越性 2nd-pass（BL-1 因依赖 `StacktraceArch` 完整闭环）

---

## 19. GPT 评论批判性分析（2026-08-16，doc 05 review 顺带）

> **来源**：`~/AI-chats/comments.md:1-1214`（GPT 对 doc 05 时钟中断初始化设计的整体评论）。
> **方法学**：逐条对照 C 源码（ground truth）+ 当前 Rust 实现 + doc 05 既有决策，标注"接受 / 部分接受 / 反对"，避免直接照搬 GPT 结论。
> **状态**：批判性 review 完成，结论写入 todo；不立即改动代码（需 OQ 确认哪些进入 P0/P1/P2 改造）。

### 19.1 评分与采纳结论

GPT 整体给 8/10。逐项判定：

| # | GPT 论点 | 准确性 | 评审结论 | 处理 |
|---|---------|--------|---------|------|
| 1 | **ClockArch / TimerIrqGate 边界冲突** | ✅ 完全正确（已 grep 验证 `arm64/clock.rs:69` 写 `msr cntp_ctl_el0, 1`（Enable=1,IMASK=0）+ `riscv64/clock.rs:84` 写 `csrs sie, 0x20`） | 与 C 时序偏差：C `bsp_finish_booting`（`main.c:324`）才使能 timer IRQ；Rust 在 `init_clock_and_interrupts` 就直接 Enable → 违反 C 时序 → boot 期间 timer IRQ 已 live 但尚无 handler（与 doc §3.7 V3 P0-1 已承认的差异一致） | **P0-1 接受**：把 `ClockArch::init_timer` 改为 "configure + IRQ-gated" 两阶段（GPT 建议的方案）；`TimerIrqGate::enable_timer_irq` 成为唯一 timer IRQ live 入口 |
| 2 | **InterruptController global/per-CPU 不对称** | ✅ 完全正确（已 grep 验证 `arm64/interrupt.rs:1569 ack(_irq)` / `x86_64/interrupt.rs` 同款 / `riscv64/interrupt.rs:1776 ack(_irq)` 全忽略 `_irq`） | doc §3.3 末尾已延展 + §7.4.1 I-13 P2；GPT 进一步建议 `InterruptRouter` + per-CPU `InterruptAck` 两个 trait，与现有 P2 一致 | **P1 接受**：backlog 写入 §7.4.1 I-13；当前不动代码（依赖 16-smp 的 per-CPU 基础设施） |
| 3 | **`ack(irq)` API 应改为 `ack() -> IrqVector`** | ✅ 完全正确（硬语义错误） | GIC/PLIC 硬件语义确实是"读控制器当前最高优先级 pending IRQ"，不是"ack expected IRQ"；x86 LAPIC EOI 写动作对应 eoi 不对应 ack | **P1 接受**：trait 修改工作（影响三架构 + irq_manager），doc §3.3 + §4.6/§4.8 同步修正；与 §7.4.1 I-13 合并 |
| 4 | **ClockArch 职责膨胀** | ⚠️ 部分对 | 8 个方法（`new/init_timer/read_ticks/read_tsc/stop_local_timer/init_profile_clock/stop_profile_clock/ack_profile_clock`）确实多；但 `init_profile_clock`/`stop_profile_clock`/`ack_profile_clock` 在 C `profile.c:123` 也存在（不是 Rust 引入）；`read_tsc` 默认委托 `read_ticks`（arch/clock.rs:1255） | **P2 接受**：可演进为 `PeriodicTimer` + `ClockCounter` + `ProfileTimer` 三个 trait；当前保持不动（与方法数膨胀但语义清晰不冲突） |
| 5 | **GIC `mask_all()` 只处理 SPI** | ✅ 完全正确（已 grep 验证 `arm64/interrupt.rs:1582` 循环 `32..self.nr_irqs`） | trait 命名 `mask_all()` 暗示"所有 INTID disabled"实则"仅 SPI disabled" | **P2 接受**：trait 方法改名为 `mask_all_global()` + 加 `mask_all_local()`（GPT 建议）；doc §3.3 同步注释明确语义 |
| 6 | **`ClockState` BSP/AP 模型** | ✅ 完全正确 | doc §4.1.5 L1011 `set_timer` AP 上 panic；GPT 建议 `GlobalClock + PerCpuClock` 拆分 | **P2 接受**：留待 SMP 阶段拆分（doc §3.1 D8 已说"先运行时分支"）；当前实现可工作 |
| 7 | **TimerQueue 双索引 + ClockState/硬件分层** | ✅ GPT 评价正确 | doc §4.1.2 D2 + §3.1 D1 决策；GPT 认可是正确的 | **记录**：无修改需要 |
| 8 | **`ArchInit` 垃圾箱风险** | ✅ 完全正确（doc §3.5 已自承） | doc §3.5 末段已写硬边界规则；GPT 强调强化与现有决策一致 | **P2 接受**：保持现有边界规则文字；如未来加 PMP/ACPI/PMU 仍走 ArchInit，需先论证无跨架构抽象空间 |
| 9 | **`DEFAULT_HZ` 双份定义** | ✅ 完全正确 | doc §3.2 已标注；os/arch + os/kernel 两处定义 | **P2 接受**：同 D1（`Errno newtype` 共享 minix-types）——把 `TickRate` / `ClockConfig` 放到 platform/common config 层；当前依赖方向不允许反向依赖，需 OQ 决定 crate 拆分 |
| 10 | **x86-64 文档描述 vs 实现脱节** | ✅ 完全正确 | doc §3.3 表写 `x86-64: 8254 PIT / LAPIC Timer`，但 `ClockArch::init_timer` 只配 PIT，LAPIC 由 `InterruptController::init` 负责 | **P2 接受**：doc §3.3 表加 boot/runtime 阶段明确说明（GPT 建议的画法） |
| 11 | **EarlyConsole 别动** | ✅ GPT 评价正确 | doc §3.4 简洁 + 与 doc §3.5 ArchInit 边界一致；无需加 ConsoleManager | **记录**：无修改需要 |
| 12 | **不应过度强调 `Send + Sync`** | ⚠️ **GPT 错了一半** | doc §3.1（ClockArch）+ §3.3（InterruptController）已给出**完整解释**：per-CPU 物理隔离 + 引用可传递的语义编码，不是并发安全声明；GPT 没看到这两段长注释就说"为有问题的 instance model 辩护"——实际 doc 已说明 `ClockArch` 是 transient 当前不需要持久化 | **反驳 GPT**：当前 `Send + Sync` bound **没有运行时开销**，对未来 per-CPU 持久化也无坏处；doc §3.1 大段解释恰恰是设计透明性而非"解释为什么它没问题"——读者能据此判断何时可收紧 bound |
| 13 | **建议的总体架构图** | ⚠️ 方向对但时机错 | GPT 提出的 `PlatformDesc → Clock Hardware / Interrupt Router / Arch Boot Glue → TimerIrqGate → CPU Trap Entry → IrqManager → ClockState::tick()` 链路与 doc §4.13 `init_clock_and_interrupts()` 流程图基本一致；GPT 多出 `InterruptRouter` 拆分（与 §7.4.1 I-13 重合） | **记录**：架构方向确认，无新决策 |

### 19.2 关键 P0 接受项详细记录（待 OQ 确认是否本轮改造）

#### 19.2.1 P0-1：ClockArch / TimerIrqGate 边界冲突修复

**问题陈述（与 doc §3.7 V3 P0-1 一致）**：

当前 `ClockArch::init_timer` 在三架构直接 Enable timer IRQ：

```rust
// os/arch/src/arm64/clock.rs:69
core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);  // Enable=1, IMASK=0

// os/arch/src/riscv64/clock.rs:84
core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64);  // STIE=1

// os/arch/src/x86_64/clock.rs（已 grep 验证 init_timer 只配 PIT）
// init_timer 不写 LAPIC LVT Timer mask；但 LAPIC LVT mask 由 init_lapic 设=1
```

而 `TimerIrqGate::enable_timer_irq` 又写**完全相同**的硬件位：

```rust
// os/arch/src/arm64/timer_irq_gate.rs:31-44
core::arch::asm!("msr CNTP_CTL_EL0, {ctrl}", ctrl = in(reg) 1u64);

// os/arch/src/riscv64/timer_irq_gate.rs:23-32
core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64);
```

→ aarch64/riscv64 在 `init_clock_and_interrupts` 阶段 `ClockArch::init_timer` 一调用，timer IRQ 立刻 live，但 handler 尚未注册。

**C 时序（ground truth）**：C 版 `init_clock()`（clock.c:48）只初始化软件状态（`kclockinfo`/`kloadinfo`/频率），**不碰硬件**。硬件定时器使能发生在更晚的 `bsp_finish_booting()`（main.c:324）调用 `boot_cpu_init_timer()`（clock.c:293）→ `init_local_timer()` + `register_local_timer_handler()`。

**GPT 建议**：把 `ClockArch::init_timer` 改为 "configure timer + leave IRQ gated"，由 `TimerIrqGate::enable_timer_irq` 成为唯一 timer IRQ live 入口。

**具体实现方向**：

| 架构 | 当前 `ClockArch::init_timer` | 期望行为（GPT 建议） |
|------|------------------------------|----------------------|
| x86-64 | 配 PIT | 配 PIT + LAPIC LVT mask=1（保持 masked） |
| aarch64 | `CNTP_CTL_EL0 = 1`（Enable=1,IMASK=0）| `CNTP_CTL_EL0 = 2`（Enable=0,IMASK=1）— 配置 compare 值但不 live |
| riscv64 | `csrs sie, 0x20`（STIE=1）| 写 mtimecmp 但**不**写 `sie.STIE`；`TimerIrqGate::enable_timer_irq` 才设 STIE |

**影响范围**：
- `os/arch/src/{x86_64,arm64,riscv64}/clock.rs` `init_timer` 实现
- `os/arch/src/{x86_64,arm64,riscv64}/timer_irq_gate.rs`（当前实现可能要从"实现 enable"改为"实现 enable 的硬件位 mask"——语义不变只是迁移）
- `os/kernel/src/lib.rs` `bsp_finish_booting` Step 6（V3 P0-1 已记录此步需调用 `enable_timer_irq`）
- doc 05 §3.7 + §4.2/§4.4/§4.5/§4.7 + §3.8 行为变更声明表
- 测试：x86 LAPIC timer 启用后的 QEMU 集成测试覆盖（当前 §5.2 列为未覆盖）

**风险**：
- boot 期间 timer IRQ 不 live → 所有依赖 boot 期间 tick 的代码（罕见）会延后到 `bsp_finish_booting` 之后；与 C 时序对齐（ground truth priority）
- 与 doc §3.7 V3 P0-1 的差异声明表做反向修正：当前是"x86 LAPIC LVT mask 0→1"，改为"aarch64/riscv64 从 live 改为 gated"

**建议**：本轮修复（与 V3 P0-1 合并）；保持 doc §3.7 的 P0-1 标记。

### 19.3 P1 接受项详细记录

#### 19.3.1 P1-1：`InterruptController::ack` API 改为 `ack() -> IrqVector`

**问题陈述（GPT 与 §7.4.1 I-13 重合）**：当前 `ack(irq: IrqVector)` 把参数 `_irq` 完全忽略（x86_64/arm64/riscv64 三架构 grep 已验证），但签名要求调用方传一个 IRQ 号——这是把 x86 LAPIC EOI 思维（写动作不需要 IRQ 参数）强行投影到 GIC/PLIC。

**GIC 真实语义**（`arm/interrupt.c`，Minix3 未移植）：读 `ICC_IAR1_EL1` 返回 INTID。
**PLIC 真实语义**（RISC-V PLIC spec §4）：读 claim 寄存器返回 INTID。
**LAPIC 真实语义**（x86）：EOI 寄存器**只写不读**——但 LAPIC 也有"读 IRR（Interrupt Request Register）/ISR（In-Service Register）"获得 INTID 的能力；Minix3 x86 用 IOAPIC RTE（Redirection Table Entry）+ `irq_handlers[]` 表查 IRQ 号，不靠读 LAPIC 寄存器。

→ 三架构硬件语义都是"读 → 得到 INTID"，而非"传 INTID 进去 ack"。

**GPT 建议 API**：

```rust
trait InterruptCpuInterface {
    fn ack(&mut self) -> IrqVector;  // 返回 ack 到的 IRQ 号
    fn eoi(&mut self, irq: IrqVector);  // eoi 仍需 IRQ 号（GIC 写 ICC_EOIR1_EL1 + INTID）
}
```

**影响范围**：与 §7.4.1 I-13 的 `InterruptRouter + InterruptAck` trait 拆分合并处理；当前 `InterruptController::ack(irq)` 应作为过渡，doc §3.3 + §3.8 表 + §4.6/§4.8 同步修正。

**风险**：拆 trait 依赖 per-CPU 基础设施（`CpuLocal<Ack>`），与 16-smp 同批次；本轮先做文档修正（§3.3 + §3.8 + §4.6 + §4.8 中所有 `ack(irq)` 调用语义说明）。

### 19.4 反驳/部分反对项

#### 19.4.1 对 GPT "不应过度强调 Send + Sync" 论点的反驳

GPT 原文："**Send + Sync** 解释得很努力，但模型本身不够干净"——GPT 没看 doc §3.1（ClockArch 完整解释）+ §3.3（InterruptController 完整解释）就下结论。

**反驳证据**：

1. doc §3.1 L360-370 用 80+ 行解释 `ClockArch: Send + Sync` 的真实动机——per-CPU 物理隔离 + 引用可传递的语义编码，不是"并发数据结构"的声明；
2. doc §3.3 末尾延展（DEFERRED 段）已说明 `Send + Sync` 是引用层面的类型证明不蕴含实例字段可多 CPU 同时变更；
3. 当前 `ClockArch` 确实是 transient（每次调用从 desc 现构造），但 `Send + Sync` bound 没有运行时开销，且对未来 per-CPU 持久化无坏处；
4. 项目整体（kernel/arch 已 0 clippy warnings）的 Sync 设计有完整论证（sealed trait `BklProtected` + BKL witness）——`Send + Sync` 不是"解释为什么没问题"，而是"设计透明性"的体现。

**评审结论**：`Send + Sync` bound 当前**正确且必要**；GPT 的"抽象泄漏约束"推测与代码事实不符。**不修**。

#### 19.4.2 对 GPT "ClockArch 拆分三个 trait" 的部分接受

GPT 原文建议拆 `PeriodicTimer` + `ClockCounter` + `ProfileTimer` 三个 trait。

**部分接受**：`PeriodicTimer`（`init/stop`）+ `ClockCounter`（`read`）的语义区分**真实**——`read_ticks/read_tsc` 当前默认委托确实是把不同语义揉在一起（C 版亦如此，`read_tsc` 是 x86 标准做法；Rust 用默认方法委托保持 API 一致）。

**部分反对**：`init_profile_clock/stop_profile_clock/ack_profile_clock` 在 C `profile.c:123` 也存在——这不是 Rust 引入的"膨胀"，而是 C 既有 API 的忠实重写。拆 `ProfileTimer` trait 会失去 C-Rust 1:1 对应（CLAUDE.md 强调 Rewrite 保持外部行为）。

**评审结论**：当前 8 方法 trait 可保留；doc §3.1 增补一句"ClockArch 当前承担三个职责（periodic + counter + profile），C 端同款"——明确这是 C 既有语义的保留，而非 Rust 引入的过度设计。**P3 记录**，不修。

### 19.5 P2/P3 项汇总（备查）

| 项 | 描述 | 触发时机 |
|----|------|---------|
| P2-1 | `InterruptController::mask_all()` 改名为 `mask_all_global()` + 加 `mask_all_local()`（GPT §14） | 与 §7.4.1 I-13 合并重构时 |
| P2-2 | doc §3.3 表加 boot/runtime 阶段明确说明（GPT §13） | 本轮 doc 修正可做 |
| P2-3 | `DEFAULT_HZ` 跨 crate 共享（放到 platform config 层，GPT §16） | OQ 决定 crate 拆分（与 D1 同源） |
| P2-4 | `ArchInit` 硬边界规则文字强化（GPT §12） | doc §3.5 末段已有，复核即可 |
| P3-1 | ClockArch 拆 `PeriodicTimer` + `ClockCounter` + `ProfileTimer` 三 trait | 不推荐（C-Rust 1:1 对应丢失） |
| P3-2 | `Send + Sync` bound 收紧（仅在需要持久化场景保留） | 不推荐（当前 bound 无运行时开销） |

### 19.6 评审整体结论

- **GPT 评价 8/10 合理**；
- **核心 P0（ClockArch / TimerIrqGate 边界）**与 doc §3.7 V3 P0-1 已记录项重合，**本轮修复时合并处理**；
- **P1 ack API 修正**与 §7.4.1 I-13 合并，**SMP 阶段统一处理**；
- **P2/P3 项**已识别但工作量小、本轮不强制；
- **Send+Sync 等论述 doc 已透明**，**无需修改**；
- 整体 doc 05 设计方向**保留**，小修具体实现即可。

## 20. GPT 对 doc 06 评论的批判性分析（2026-08-18，task: `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/todo.md`）

> **来源**：`~/AI-chats/comments.md:1-1137`（GPT 对 doc 06 进程表初始化与 boot 进程加载的整体评论）
> **方法学**：逐条对照 C 源码（ground truth）+ 当前 Rust 实现 + doc 06 既有决策，标注"接受 / 部分接受 / 反对 / 已记录"。
> **状态**：批判性 review 完成；可优化项追加写入本节（不立即改动代码——遵循 doc 06 §3.0 设计原则 "先做完整个 kernel，统一 review"）。

### 20.1 GPT 论点逐项判定

GPT 整体评价 8/10。逐项判定：

| # | GPT 论点 | 准确性 | 评审结论 | 处理 |
|---|---------|--------|---------|------|
| 1 | **固定数组 ProcessTable / ProcNr/index / CapabilityTemplate / RtsFlags + rts_set/rts_unset** 评价"很好/正确/非常好" | ✅ 完全正确（已 grep 验证：proc_table.rs:75 const fn new + proc.rs:112 KERNEL_TASKS + capability.rs:131 enum CapabilityTemplate + RtsFlags bitflags） | 与 doc 06 §3.1/§3.3 决策完全一致 | **记录**：无需修改 |
| 2 | **ProcKind 方向正确但职责开始变宽** | ⚠️ 部分正确 | 当前 `ProcKind` 5 变体（KernelTask/Vm/RootService/UserService/UserProcess）确实决定 PSW/段选择子/FPU/特权等多维度。但拆分 `ExecutionDomain` × `ProcessRole` × `PrivilegePolicy` 三轴短期内是 over-engineering——当前 Minix3 角色确实简单 | **P2 设计债记录**：未来如果 KernelTask 下出现 interrupt-driven / kernel coroutine / schedulable kernel thread 三种情形，再拆分；当前不修 |
| 3 | **EntrySpec 是"第一阶段 Rust refinement"** | ✅ 完全正确 | doc 06 §3.4 已自承 "类型层局限（仍依赖 `ProcKind` 区分）：KERNEL_TASK 与 DEFERRED 都是全 None 的 EntrySpec"——已明确为 known debt | **P2 设计债记录**：未来演进为 `enum ProcessStart { KernelEventDriven, UserDeferred, UserReady(UserEntry) }`；当前不修 |
| 4 | **CpuContextArch trait 正在变胖（6 方法）** | ✅ 完全正确 | 已 grep 验证 boot.rs:165/173/184/197/226/251 实有 6 方法：build_cpu_context/apply_to_trap_frame/enable_user_io/inherit_fpu_state/write_user_register/or_ipc_status_reg（doc 06 §5.4 也记录 21 处引用、6 类方法） | **P2 设计债记录**：未来 `enable_user_io` 可能拆出到 `ArchPrivilege` / `UserIoArch` trait；当前不修 |
| 5 | **enable_user_io() 是不属于 CpuContextArch 的"语义偏移"** | ✅ 部分正确 | x86-64 `enable_user_io` 设 IOPL=3，aarch64/riscv64 no-op——确实是"privilege / device access policy"而非 CPU context。但 doc 06 §3.5 已明确论证"trait 用于真多态"——`enable_user_io` 是真多态（x86 需实现，aarch64/riscv64 不需要），三架构语义不同 | **P2 设计债记录**：与 #4 合并 |
| 6 | **FPU 下沉 arch（FpuArch trait + CurrentFpuState + FPU 策略）方向正确** | ✅ 完全正确 | doc 06 §3.6 已论证 rust 不翻译 fnsave/fxrstor 而是 FXSAVE/CPACR_EL1.FPEN/sstatus.FS 三架构统一抽象；实测 x86_64/boot.rs:1657 `fpu_policy` 枚举 + aarch64/riscv64 同款 trait 方法 | **记录**：无需修改 |
| 7 | **KPriv 8 子结构是健康的重构** | ✅ 完全正确 | doc 06 §4.6 + kpriv.rs 实际有 8 子结构（PrivIdentity/PrivInit/PrivFlags/PrivSignals/PrivIpc/PrivIo/PrivMem/PrivRuntime，§4.6 表精确）；PrivFlagsBits 物理位布局保留的决策正确（拆位会破坏 IPC） | **记录**：无需修改 |
| 8 | **BKL + atomic 是 SMP 安全"唯一组合" — 说得太满** | ✅ 完全正确（重要修正） | doc 06 §3.9 原文："BKL + AtomicI32 是 SMP 安全的唯一组合"——这个表述确实过强。实际存在 per-CPU lock / RCU / sequence lock / MCS lock / lock-free queue 等多种方案。但 doc 06 §3.9 的"在当前 Minix3 BKL + idle-steal 模型下，我们选择的组合"是准确的 | **P1 文档修正**：doc 06 §3.9 把 "唯一组合" 降级为 "在当前 Minix3 BKL + idle-steal 模型下，我们选择的组合"——这是 doc 准确性修正，无代码改动 |
| 9 | **BSS 表述需要小心（const fn ≠ BSS）** | ✅ 完全正确（重要修正） | doc 06 §3.8 原文："编译期完成初始化（BSS 段零运行时开销）" + "结果固化在 .bss 段"。实际 const fn 是 static compile-time initialization，最终落到 `.bss / .data / .rodata` 由链接器决定——若 `KProcess::new_zeroed()` 含非零初值（如 Atomic/Option/Endpoint），不一定纯 zero-init | **P1 文档修正**：doc 06 §3.8 "BSS" 措辞改为 "static compile-time initialization, avoiding runtime construction"——避免 BSS 表述不准 |
| 10 | **load_vm_elf() free fn（非 trait 方法）很好** | ✅ 完全正确 | doc 06 §3.7 + arch/boot.rs:271 实测——三架构实现相同是假多态，free function 正确 | **记录**：无需修改 |
| 11 | **boot 主流程保留 orchestrator 形态（未拆 builder）** | ✅ 完全正确 | doc 06 §3.0 + lib.rs:767 init_proc_and_boot 实测是单 function 编排，无 ProcessBuilder/PrivilegeBuilder/BootImageBuilder/ContextBuilder 拆分 | **记录**：无需修改 |
| 12 | **bsp_finish_booting() 结构清晰** | ✅ 完全正确 | lib.rs:1806 + doc 06 §4.8 步骤分解准确；Step 8.5 `mem::forget(bkl_lock())` 是 BKL 跨 switch_to_user 的关键设计（lib.rs:1843） | **记录**：无需修改 |
| 13 | **"Process = execution flow"概念仍有过拟合（Process vs ExecutionContext 应拆分）** | ⚠️ 部分正确 | GPT 指出 IDLE 几乎无状态（"反证执行流与状态是正交维度"），CLOCK/SYSTEM 的入口是事件驱动而非独立执行流——这是真实的概念债务。但 doc 06 §1.1.4 已诚实标注 "Kernel task 的'运行'是事件驱动的，不是独立执行流" + "没有独立地址空间（共享 kernel image），且 kernel task 从未被 `arch_proc_init()` 设置执行入口"，已建立了 Process 与 ExecutionContext 的分离意识 | **P2 设计债记录**：与 #2/#3 合并——待整个 kernel 完成后统一 review |
| 14 | **GPT 的 17 条最终设计债（5 类：Proc vs ExecutionContext / ProcKind 拆分 / EntrySpec state enum / CpuContextArch 膨胀 / BKL 论证强度）** | ✅ 部分正确 | 5 类设计债中：① Process vs ExecutionContext = 与 #13 同项；② ProcKind 拆分 = 与 #2 同项；③ EntrySpec state enum = 与 #3 同项；④ CpuContextArch 膨胀 = 与 #4/#5 同项；⑤ BKL 论证强度 = 与 #8 同项（已修正） | **P2 设计债记录**：5 项统一纳入最终 review checklist |
| 15 | **GPT 的"反转收益"指标（每个 abstraction 检查反方向收益）** | ⚠️ 方法论正确，但 doc 06 已部分体现 | doc 06 §3.0 "强制设计原则"（P1-P7）+ §3.5 CpuContextArch 论证 + §3.6 FPU 演进 + §3.7 load_vm_elf 都给出了明确的"反方向收益"分析（不是更抽象，是更具体）。但 doc 没有系统化用"反方向收益"做清单 | **P2 文档改进（可选）**：可在 doc 06 §3.15 决策汇总表加一列 "反方向收益（GPT §15 方法论）" |
| 16 | **GPT 提到 "capability.rs 的 `CapabilityTemplate` 与 `ProcKind` 不合并"** | ✅ 完全正确 | doc 06 §3.4 已论证 + capability.rs:131-145 enum 实测——两者关注点不同（ProcKind 给 arch 层看，CapabilityTemplate 给 kernel 层看），合并会丢失 IDLE = KernelTask + Idle 的组合自由度 | **记录**：无需修改 |
| 17 | **CapabilityTemplate 名字"有点危险"（template 太窄）** | ✅ 部分正确 | 当前 CapabilityTemplate 表达 "boot-time privilege policy"——未来若引入运行时 capability 机制，"template" 概念会变窄。但目前是 5 变体枚举，命名匹配 | **P3 命名**：未来 capability 复杂化时再考虑改名 |

### 20.2 关键 P1 文档修正项详细记录

#### 20.2.1 [P1] BKL + AtomicI32 "唯一组合" 措辞降级

**问题陈述**：doc 06 §3.9 原文：

> **假设性推理**：如果用 `Rc<RefCell<KProcess>>` 跨 CPU 共享，`RefCell` 的运行时借用检查不是原子操作，两个 CPU 可能同时获得 `&mut`，导致 UB。**BKL**（全局串行化调度决策）+ **`AtomicI32`**（BKL 外的 idle steal 读路径）组合是 SMP 安全的**唯一组合**——`AtomicI32` 不是 BKL 的"替代"，而是 BKL 之外的**少锁读路径**（CPU 闲置从其他队列偷进程时，BKL 不被持有，需要原子读 `p_nextready`）。

**GPT 准确判定**：

> 后半句"唯一组合"就说得太满了。
> 因为从系统设计角度：BKL + atomics 只是"你当前这个 Minix-style scheduler 选择的安全方案"，不是一般意义上的"唯一 SMP 安全组合"。可以存在：per-CPU locks / RCU / sequence lock / MCS lock / lock-free queue / …

**修复建议（doc 06 §3.9 末尾段）**：

```diff
- **BKL**（全局串行化调度决策）+ **`AtomicI32`**（BKL 外的 idle steal 读路径）组合是 SMP 安全的唯一组合
+ **BKL**（全局串行化调度决策）+ **`AtomicI32`**（BKL 外的 idle steal 读路径）组合是 **在当前 Minix3 BKL + idle-steal 模型下我们选择的方案**——
+ 存在其他 SMP 安全组合（per-CPU locks / RCU / sequence lock / MCS lock / lock-free queue 等），
+ 但本项目沿用 Minix3 的简化模型，故未引入
```

**关联**：todo §7.1 D-38（BKL 接入 4 处）的 §4.13 BKL 论证强度 + 16-smp.md §4.13 同款问题。

**优先级**：P1（文档准确性，非代码缺陷）

#### 20.2.2 [P1] "BSS 段" 表述改为 "static compile-time initialization"

**问题陈述**：doc 06 §3.8 原文：

> **`const fn` 实际实现**（[proc_table.rs:75](file:///os/kernel/src/proc_table.rs)）：`[const { KProcess::new_zeroed() }; PROC_TABLE_SIZE]` 在编译期构建完整数组，调用点 `static PROC_TABLE` 触发 const 上下文求值，结果**固化在 `.bss` 段**，运行期零开销。

**GPT 准确判定**：

> 严格来说：`const fn` 意味着 **静态初始化可以在编译期完成**；但是否最终进 `.bss / .data / .rodata` 是由对象的实际初始值和链接器决定的。尤其 `KProcess::new_zeroed()` 如果其中存在非零初始值（Atomic/Option/Endpoint/ProcNr...），不一定全部是纯 zero-init。因此更准确的设计表述应该是：**static compile-time initialization, avoiding runtime construction** 而不是 **BSS = compile-time initialization**

**修复建议（doc 06 §3.8 末段）**：

```diff
- `[const { KProcess::new_zeroed() }; PROC_TABLE_SIZE]` 在编译期构建完整数组，调用点 `static PROC_TABLE` 触发 const 上下文求值，结果固化在 `.bss` 段，运行期零开销。
+ `[const { KProcess::new_zeroed() }; PROC_TABLE_SIZE]` 在编译期构建完整数组，调用点 `static PROC_TABLE` 触发 const 上下文求值，运行期零开销。
+ 注：`KProcess::new_zeroed()` 中含 `Atomic*` / `Option<ProcNr>` / `Endpoint` 等非零初始字段，
+ 实际链接段由编译器/链接器决定（可能是 `.data` 而非 `.bss`）——准确表述是 "static compile-time initialization" 而非 "BSS = compile-time initialization"。
```

**关联**：CLAUDE.md "Hidden Folder Convention" — `tmp_design_and_todo/` 视为中间产物，正式 doc 引用应使用绝对路径到 doc、代码、C 源。当前 doc 06 §3.8 的 BSS 表述是设计准确性而非路径问题。

**优先级**：P1（文档准确性）

### 20.3 P2 设计债汇总（5 项，纳入最终 review checklist）

合并 GPT 论点 #2/#3/#4/#5/#13/#14 的设计债，作为最终 kernel architecture review 的固定检查项（doc 06 §3.0 "待整个 kernel 完成后统一 review"）：

| ID | 设计债 | 触发条件 | 当前状态 |
|----|--------|---------|---------|
| **D-52** | ProcKind 拆分（ProcessRole × ExecutionDomain × PrivilegePolicy） | KernelTask 下出现 interrupt-driven / kernel coroutine / schedulable kernel thread 三种情形时 | ⏸ 设计债 |
| **D-53** | EntrySpec 从 Option fields 演进为 state enum（`Kernel / Deferred / Ready(UserEntry)`） | 出现第三种 "Option 表达不了" 的 entry 状态时 | ⏸ 设计债 |
| **D-54** | enable_user_io 从 CpuContextArch 拆出（成立 `ArchPrivilege` / `UserIoArch` trait） | CpuContextArch trait 方法数 > 8 或 trait 边界频繁调整时 | ⏸ 设计债 |
| **D-55** | Process vs ExecutionContext 概念拆分（拆 `ProcessIdentity/State` 与 `ExecutionContext`） | 引入更多类型 kernel entity（coroutine / 微线程 / 异构执行流）时 | ⏸ 设计债 |
| **D-56** | CapabilityTemplate 重命名（template → policy / 引入运行时 capability 时） | 引入运行时 capability 机制时 | ⏸ 设计债 |

**集成位置**：写入 doc 06 §3.0 "强制设计原则" 之后的 "设计债跟踪" 段 + STATE.md（如果后续启动 Claude Code review 流程）。

**验证**：

```bash
# 1. doc 06 §3.9 "唯一组合" 措辞降级
rg "唯一组合" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 1 match（需降级措辞）

# 2. doc 06 §3.8 "BSS" 表述
rg "固化在 .bss 段" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 1 match（需改为 "static compile-time initialization"）

# 3. CpuContextArch trait 方法数
rg "fn " os/arch/src/arch/boot.rs -n | rg "build_cpu_context|apply_to_trap_frame|enable_user_io|inherit_fpu_state|write_user_register|or_ipc_status_reg"
# → 6 matches（当前 6 方法，触发 D-54 拆出的临界点）
```

### 20.4 反驳/部分反对项

#### 20.4.1 对 GPT "CpuContextArch trait 正在变胖" 的部分反对

**GPT 论点**：21 处引用、6 类方法——trait 已从 "CPU context abstraction" 演变为 "ArchEverything"。

**反驳/补充**：

1. **6 方法数量仍在合理区间**（Linux `arch_thread_struct` 同款 6+ 字段、Redox `Arch::Context` 5 方法）
2. **每个方法都是真多态**：x86_64 vs aarch64 vs riscv64 实现真的不同（不是假多态）——`enable_user_io` 在 x86-64 写 PSW.IOPL，aarch64/riscv64 no-op；`inherit_fpu_state` 在 x86-64 复制 `fpu_policy`，aarch64 复制 `fpu_enable_el0`，riscv64 复制 sstatus
3. **doc 06 §3.5 已给出"为什么 trait 而非 cfg-alias"的论证**：3 个架构的 `CpuContext` 字段布局完全不同 + 提供 mock 测试能力

**结论**：6 方法是当前复杂度下的合理设计；GPT 警告的"抽象膨胀"风险成立，但触发点应该是 8+ 方法或 trait 边界频繁调整时。当前不修，纳入 D-54 设计债跟踪。

#### 20.4.2 对 GPT "CapabilityTemplate 名字有点危险" 的反驳

**GPT 论点**：名字 "template" 太窄——其实表达的是 "boot-time privilege policy"。

**反驳**：

1. **当前 5 变体枚举完美匹配 boot 期 5 类角色**（Idle/KernelTask/Vm/RootService/Deferred）
2. **"template" 在 Rust 生态是 idiomatic 命名**（Diesel、Askama、handlebars 都用 "Template" 表达"角色/模板"含义）
3. **未来若引入运行时 capability，**可重命名为 `CapabilityPolicy` 或 `BootCapability`，当前命名不阻塞

**结论**：当前命名合理；GPT 担忧的"未来过窄"风险成立，但当前不修（D-56 设计债跟踪）。

### 20.5 评审整体结论

- **GPT 评价 8/10 合理**；
- **核心 P1 文档修正 2 项**（§3.9 "唯一组合" + §3.8 "BSS"）——**✅ 已修复（2026-08-18）**：见 §20.6 修复记录；
- **P2 设计债 5 项**已识别但工作量小、本轮不强制（纳入 D-52~D-56 设计债跟踪，最终 kernel 完成后统一 review）；
- **P3 命名 1 项**（CapabilityTemplate 重命名）暂不修；
- **核心架构决策（ProcessTable/ProcKind/CpuContextArch/EntrySpec/FPU/KPriv/BKL/boot 主流程）保留**，GPT 评价与 doc 06 既有决策一致；
- **整体 doc 06 设计方向保留**，小修具体表述即可。

**关联文档**：
- doc 06 §3.0 设计原则（"先做完整个 kernel，统一 review"）→ 本节 5 项设计债纳入最终 review checklist
- doc 06 §3.8 §3.9 文字表述 → **✅ 已修复**（§20.6）
- doc 06 §3.15 设计决策汇总表 → 可选加 "反方向收益" 列（GPT §15 方法论）

### 20.6 P1 修复记录（2026-08-18）

| ID | 修复内容 | doc 文件:行 | diff 摘要 | 验证 |
|----|---------|------------|---------|------|
| **FIX-06-GPT-1** | §3.9 "BKL + AtomicI32 唯一组合" 措辞降级 | `06-proc-init-boot-proc.md:1411` | 把"唯一组合"改为"在当前 Minix3 BKL + idle-steal 模型下我们选择的方案" + 新增"架构范围说明"段落（列出其他 SMP 安全组合） | `rg "唯一组合" notes/rewrite/.../06-proc-init-boot-proc.md` = 0 match |
| **FIX-06-GPT-1b** | §3.1 "零堆 + SMP 安全唯一组合" 措辞降级（第二轮扫描发现 L1144 第二处出现） | `06-proc-init-boot-proc.md:1144` | 把"零堆 + SMP 安全的唯一组合"改为"零堆 + SMP 安全在当前 Minix3 BKL + idle-steal 模型下我们选择的方案" | 同上 |
| **FIX-06-GPT-2** | §3.8 "BSS 段" 表述修正 | `06-proc-init-boot-proc.md:1374` | 把"结果固化在 .bss 段"改为"static compile-time initialization, avoiding runtime construction" + 新增"术语说明"段（解释 const fn 与 BSS 的区别） | `rg "固化在 .bss 段" notes/rewrite/.../06-proc-init-boot-proc.md` = 0 match |
| **FIX-06-GPT-2b** | §3.1 "`.kernel.bss`" 表述修正（L1146 第二处出现） | `06-proc-init-boot-proc.md:1146` | 把"放在 `.kernel.bss` 段"改为"放在编译期静态段（实际链接段由 link.ld 与 `ProcessTable` 字段初值决定，可能落在 `.bss` 或 `.data`）" | `rg "\.kernel\.bss" notes/rewrite/.../06-proc-init-boot-proc.md` = 0 match |
| **FIX-06-GPT-2c** | §3.15 决策汇总表 "BSS 段零运行时开销" 表述修正（第三轮扫描发现 L1557 第三处出现） | `06-proc-init-boot-proc.md:1557` | 把"BSS 段零运行时开销"改为"static compile-time initialization，运行期零构造开销" | `rg "BSS 段零运行时" notes/rewrite/.../06-proc-init-boot-proc.md` = 0 match |

**关联修改**：
- doc 06 §3.9 新增段落引用其他 SMP 安全组合（per-CPU locks / RCU / sequence lock / MCS lock / lock-free queue），明确这些方案未引入的原因（本项目沿用 Minix3 简化模型）
- doc 06 §3.8 新增段落说明 `KProcess::new_zeroed()` 含非零字段（Atomic/Option/Endpoint）→ 实际链接段可能是 `.data` 而非 `.bss`
- doc 06 §3.1 第二处 BSS/BKL 表述也修正（不仅 §3.8/§3.9，§3.1 也有同类表述）
- doc 06 §3.0 设计原则 P1 列表内 "BSS 段零运行时开销" 修正
- doc 06 §3.15 设计决策汇总表 "BSS 段零运行时开销" 修正

**三轮扫描覆盖**：第一轮扫描覆盖 §3.8/§3.9 主要位置；第二轮发现 §3.1 也有两处同类表述；第三轮发现 §3.0 原则表 + §3.15 决策表两处遗漏。修复采用"全文档穷尽扫描"模式确保不留死角。

**验证命令**：
```bash
# 验证 "唯一组合" 已清除（全文档）
rg "唯一组合" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 0 match（FIX-06-GPT-1 + FIX-06-GPT-1b 生效）

# 验证 "固化在 .bss 段" 已清除
rg "固化在 .bss 段" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 0 match（FIX-06-GPT-2 生效）

# 验证 ".kernel.bss" 已清除
rg "\.kernel\.bss" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 0 match（FIX-06-GPT-2b 生效）

# 验证 "BSS 段零运行时" 已清除
rg "BSS 段零运行时" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 0 match（FIX-06-GPT-2c 生效）

# 验证新增段落存在
rg "static compile-time initialization" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 3 match（§3.0 + §3.8 + §3.15 三处提及）
rg "架构范围说明" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md
# → 1 match（FIX-06-GPT-1 新增段落）
```

## doc 15 review 发现：DEFAULT_HZ 设计差异（2026-08-14，OQ-15-1）

> **状态：🟡 Open Question（待用户决定）**。doc 15 review 发现 `DEFAULT_HZ` 三方不一致：

| 侧 | 值 | 位置 |
|----|-----|------|
| C（ground truth）| **60** (i386) / 1000 (earm) | `minix3/minix/include/arch/i386/include/archconst.h:4` |
| Rust | 100 | `os/kernel/src/clock.rs:472` + `os/arch/src/arch/clock.rs:43` |
| doc | 100（标注 C 对照）| §2.1 表 + §3.7 D7 注 |

**影响**：无 `hz` 覆盖参数时，Rust 内核以 100Hz 运行而 C 内核以 60Hz 运行——tick 频率、调度时间片、时钟精度随之不同（**行为差异**，违反 Rewrite 严格等价）。

**已做**：差异已标注（doc §3.7 D7 注 + 两处代码注释），保留 100 为设计选择。

**选项**：
- A. 保留 100（现状）——Rust 已按 100 调优；boot 时可用 `with_hz(60)` 与 C 对齐
- B. 改为 60——与 C i386 完全一致，但影响所有依赖 100 的测试/时间片计算

**关联**：Proposal #12（DEFAULT_HZ 跨 crate 重复定义的"权威位置"问题：os/arch + os/kernel 双定义，os/arch 注释已列 C 值）。

---

## D19-1: cause_signal SELF 路径致命信号子路径（SIGS_IS_LETHAL + backup 切换）DEFERRED（2026-08-14）

**来源**：doc 19-syscall-signal 深度 full-review（2026-08-14，marathon round 19）

**背景**：Rust `cause_signal`（syscall_signal.rs:164-251）已按 C 双路径重构：
- **SELF 路径**（C: system.c:416）：`rp->p_endpoint == sig_mgr` 时写自身 `s_sig_pending` + `mini_notify_core` 唤醒自身（C 的 SIGKSIGSM=73 数值仅写入无内核读者的 `s_sig_pending`，无需常量）
- **外部路径**（C: system.c:439-445）：写 `p_pending` + `RTS_SIGNALED|RTS_SIG_PENDING` + SM 的 `s_sig_pending` 置 SIGKSIG（74 超出 u64 位宽 → no-op，写记录无读者，行为保持）+ `mini_notify_core` 唤醒 SM 自身（源 = SYSTEM）

**待办**：SELF 路径内 `SIGS_IS_LETHAL`（signal.h:283，SIGILL/SIGBUS/SIGFPE/SIGSEGV/SIGEMT/SIGABRT）子路径 DEFERRED（syscall_signal.rs:180-182 注释标注）：

| 侧 | 行为 | 位置 |
|----|------|------|
| C（ground truth）| 自管理进程收到致命信号 → 有 backup：切换 `s_sig_mgr = s_bak_sig_mgr`、清 `s_bak_sig_mgr`、`RTS_UNSET(sig_mgr_rp, RTS_NO_PRIV)`、递归 `cause_sig` 重试；无 backup：`proc_stacktrace` + `panic` | system.c:417-432 |
| Rust | 当前跳过（`当前阶段无用户态进程，自管理进程（VM/RS）收到致命信号的路径不可达`） | syscall_signal.rs:180-182 |

**依赖**：`s_bak_sig_mgr` 字段接入 + `RTS_NO_PRIV` 清除 + panic 集成（设计级：需决定 Rust 内核 panic 策略）。

**建议**：待用户态进程落地、信号路径可测后再实现；当前不可达路径无需占用设计决策。

**状态（2026-09-05）**：✅ 已实现——`is_lethal`（syscall_signal.rs:101-103）+ SELF 致命子路径（syscall_signal.rs:210-256：backup 提升 + `RTS_UNSET(NO_PRIV)` + 递归 + 无 backup panic）落地（见 §7.1 D-11 行 + `.design/19-design.v2.md`）。本段为历史归档，保留原始 deferral 记录。

---

## D-49 实施发现：boot 能力模板 sendto 掩码残留差异（2026-09-05，OQ-D49-1）

> **状态：⏸ 已裁决（2026-09-05 用户决策）：作为独立 TODO（§7.1 **D-58**）暂时 defer**。
> 以下保留原始 Open Question 记录（两侧行为、选项与 AI 倾向论证供 D-58 重审时直接使用）。
> D-49（`set_sendto_bit` 运行时路径）落地时确认：runtime 路径（SET_SYS / UPDATE_SYS /
> do_update）已完全对齐 C 的 `fill_sendto_mask` 守卫/对称语义；boot 路径走文档化的
> `CapabilityTemplate` 设计（doc 22 §3 D3），与 C boot fill（main.c:244）存在一处
> **可观察**残留差异。

| 侧 | 行为 | 位置 |
|----|------|------|
| C（ground truth） | fill 的 self 守卫使 `s_ipc_to` 自位**恒为 0**——boot 服务 SEND 自身 endpoint 在掩码层被拒（`ECALLDENIED`） | system.c:316-319 + main.c:244；proc.c:536-541 经 `may_send_to` |
| Rust | `CapabilityTemplate::Vm` / `RootService` 的 `ipc_mask() = IpcMask::ALL`（含自位）——boot 服务 SEND 自身 endpoint 通过掩码层，进入阻塞路径 | os/kernel/src/capability.rs 模板；doc 22 §4.6.1 |

**影响范围**：仅 boot 模板持有者（VM/RS）SEND 自身 endpoint 这一病态场景。另一候选差异"预授权未绑定 slot"经回归论证**不可观察**：slot 未绑定时没有 endpoint，IPC 层解析不出目标；绑定时 C 由绑定方自己的 SET_SYS fill 回执补全，终态一致（论证见 doc 22 §4.6.1 "boot 路径的差异"段）。

**选项**：
- A. 保留现状（模板 `ALL` 含自位）——模板 correct-by-construction 最简；自 SEND 属病态场景，进入阻塞路径后由既有死锁语义暴露
- B. 模板掩码 + 守卫 fill——`grant_capability` 落模板掩码后对全表跑一次 `fill_sendto_mask(priv_id, 模板掩码)`，自位/未绑定 slot 被守卫清除；与 C boot fill 终态完全一致，代价约 1 行 + boot 能力测试更新

**AI 倾向**：B——"自位恒清"是 C 模型的全域不变量，模板路径成为唯一例外不够干净；但触及 boot 能力授予的文档化设计（doc 06 §3.2 / doc 22 §3 D3），故上交而非擅动。

**关联**：doc 22 §4.6.1（详细论证）、§7.1 D-49 行、§0.1 第四批行。
