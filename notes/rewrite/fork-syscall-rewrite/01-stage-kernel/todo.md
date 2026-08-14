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
| ✅ 已修复（代码落地 + 测试通过） | **3 项** | §F1 pt_alloc 论证模型改写（SMP+BKL write-once）、§F2 register 防重复 + Release/Acquire、§D3 命名错误类型（6/7 处，vm.rs 按 R-18 保留） |
| ✅ 已实现（原记录过时，非缺口，2026-08-14 二次核实） | **18 项完整 + D-38 ②③ 部分** | §7.1 表 ✅ 行：D-1~D-5/D-7（dispatch_exec/clear/runctl/statectl 全部落地，含 process name 跨空间拷贝、release_address_space、clear_endpoint、SMP IPI）、D-10/D-12（cause_signal SELF 路径 mini_notify_core + 去重）、D-19（allow_ipc_filtered_memreq→dequeue_filtered）、D-21（kernel_call_resume + 2 测试）、D-22（data_copy_vmcheck VMSUSPEND 路径）、D-23（三架构 PTE walk）、D-26~D-29（GETINFO 10 分支）、D-31（sprof 数据拷贝）、D-32（clean_seen_flag）；D-38 ②③ BKL 接入（lib.rs:1956/:2167）；另有 I-4（doc 01/02 测试表去重，01 残留 TODO 标记已清理） |
| 🟢 设计 no-op（有 rationale，非缺口） | **1 项** | D-30 swap_memreq（misc.rs:1778 注释 + doc 25；与 W-6 ClearMapCache 同类，见 §7.2/§7.6） |
| 📌 保持 DEFERRED（已核实依赖仍成立） | **31 项** | 见 §7.1 表（SMP/VM/IPC/scheduler wiring 依赖）；其中 D-6 部分实现（cause_signal_abort 已接、mini_notify 待接线）、D-35 本地路径已实现（SMP IPI pending）、D-38 ①④ 仍 DEFERRED（SMP 异常路径）、D-14/D-15/I-2/I-6 已核实更新 |
| ☐ 未解决（架构建议，待专项） | **10 项** | A1/A2/B1/C1/D1/D2/E1/G1/M1/R1（均需设计决策或专项重构，见各自章节） |

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

### C1. dispatch 大函数按语义拆分（范围收窄后） [P2]

**精确测量**（按顶层 fn 边界，排除测试模块污染——首次粗筛 14 个"500-850 行"中
9 个是统计到文件尾/测试模块的误报，如 `kernel_mini_notify` 实为 10 行薄包装、`dispatch_readbios` 74 行）。

**真实存在的 250-550 行 dispatch 函数**（均为 C 大 switch 的忠实重写，内部已调 helper）：

| 函数 | 位置 | 行数 | 拆分方向 |
|------|------|------|---------|
| `dispatch_getinfo` | misc.rs:660 | ~550 | 按 info 类别拆（do_getinfo 同款大 switch） |
| `dispatch_privctl` | syscall.rs:1164 | ~418 | 按 privctl 子命令拆 |
| `dispatch_vmctl` | syscall.rs:1751 | ~348 | 按 vmctl 子命令拆 |
| `dispatch_trace` | misc.rs:1210 | ~332 | 按 trace 操作拆 |
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
    + FpuArch + SmpArch + ArchInit + ArchBoot + PostInitArch
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
比 fn 指针更可测试（mock 可注入）。长期可将 `pt_alloc::register(fn)` 演进为
`pt_alloc::register(impl FrameAllocator)`（trait object 或泛型）——fn 指针保持当前简单形态，
trait 化列为 VM 阶段（页表分配器接入）一并评估。

**解决记录（2026-08-14）**：
- `register()` 增加 `debug_assert!(!PT_REGISTERED.load(Ordering::Relaxed))`——同域双注册即 boot bug
- store 改 `Ordering::Release`、`alloc_pt_page()` load 改 `Ordering::Acquire`——fn 指针写入对观察者可见（pt_alloc.rs:74-86/:101-105）
- "boot 域 + user-space VM 域"双注册契约已文档化
- trait 化演进方向不变（VM 阶段评估）；172 arch tests 通过

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
| **内存管理下沉独立 crate rmm** + `FrameAllocator` trait + `PageMapper`（软件遍历） | pt_alloc fn 指针注册 + Paging trait + pte_walk | ⚠️ **fn 指针 vs trait 注入**——trait 化演进见 §F2；VM 阶段可参考 rmm 分层（页表核心独立 + allocator 注入） |
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
| D-6 | 进程 | `dispatch_exit` mini_notify(sig_mgr)（do_exit.c:21） | 17 §4.3/4.8 | **部分实现**：`cause_signal_abort` 已接线（syscall_process.rs:339+），`mini_notify(sig_mgr)` 待 `SignalContext` trait + IPC 接线 |
| D-7 | 进程 | `dispatch_statectl` ClearIpcRefs（do_statectl.c:21-25） | 17 §4.7/4.8 | **✅ 已解决（2026-08-14 核实）**：ClearIpcRefs 分支 + clear_endpoint（syscall_process.rs:58/:732，do_statectl.c:21-25 语义） |
| D-8 | syscall | `kernel_call()` wrapper 未实现（trap 入口 wrapper：copy_msg_from_user + p_delivermsg_vir + dispatch + finish） | 13 §6.3 [P1] | 待 trap 入口文档化（14），含 TOCTOU 防护的 copy_msg_from_user |
| D-9 | syscall | `kbill_kcall` 内核计费标记（性能分析用） | 13 §6.4 [P2] | — |
| D-10 | 信号 | cause_sig **SELF 路径**（system.c:416-437 自管理进程自通知） | 19 §4.7 | **✅ 已解决（2026-08-14 核实）**：写自身 `s_sig_pending` + `mini_notify_core` 唤醒（syscall_signal.rs:195-196；SIGKSIGSM=73 仅写入无内核读者的 s_sig_pending，无需常量） |
| D-11 | 信号 | cause_sig **致命信号 panic**（system.c:417-432） | 19 §4.7 | `SIGS_IS_LETHAL` + backup 切换（见尾部 D19-1 段） |
| D-12 | 信号 | cause_sig **去重检查**（system.c:439-448） | 19 §4.7 | **✅ 已解决（2026-08-14 核实）**：`was_signaled`（RTS_SIGNALED 判定）+ 无条件 `p_pending.add` 去重（syscall_signal.rs:209-220） |
| D-13 | 信号 | `sig_delay_done`（system.c:454-464） | 19 §4.7 | PM 通知接口 + `SIGSNDELAY` 常量 |
| D-14 | 信号 | DIAGCTL `send_sig(PM_PROC_NR, SIGKMESS)`（do_diagctl.c:49-56） | 19 §4.7 | 保持 DEFERRED。已核实（2026-08-14）：借用冲突注释现存于 syscall.rs:2286（原引 :1010-1037 行号漂移，已更正）；PM getksig 轮询主用途已实现 |
| D-15 | 时钟 | `mini_notify(CLOCK, endpoint)` 到期通知分发（TimerAction::NotifyAlarm 已定义未接线） | 21 附录 B | 保持 DEFERRED。已核实（2026-08-14）：NotifyAlarm 仅 enum 定义（clock.rs:522）+ 测试构造（:1363/:1378），tick 无生产调用方 |
| D-16 | IPC 过滤 | `allow_ipc_filtered_msg`（system.c:803-874，L2 receive 路径偏好过滤） | 23 §4.5/Ch6 [P1] | 12-ipc-core RECEIVE 路径（当前 skeleton）后才消费方；当前 s_ipcf 字段存在但无消费方 |
| D-17 | IPC 过滤 | `may_asynsend_to` self-send 不对称（priv.h:87） | 23 §4.5/Ch6 [P1] | 异步 IPC 路径完整接入（当前用 may_send_to 替代） |
| D-18 | IPC 过滤 | `IPCF_EL_MATCH` 宏链（ipc_filter.h:19-41） | 23 §4.5/Ch6 [P2] | `allow_ipc_filtered_msg` 子逻辑 |
| D-19 | IPC 过滤 | `allow_ipc_filtered_memreq`（system.c:879+，VM 页错误请求过滤） | 23 §4.5/Ch6 [P2] | **✅ 已解决（2026-08-14 核实）**：语义对应 `VmRequestQueue::dequeue_filtered`（vm.rs:588-620，MEMREQ_GET 遍历时按过滤器跳过请求） |
| D-20 | 跨空间 | `VmRequestQueue` 全局链表 + `send_sig(SIGKMEM)` | 24 §4.6 [P1] | 随 SIGSEND 实现落地；当前 suspend_for_vm_with_copy 已设 RTS_VMREQUEST + p_vm_suspend 但未通知 VM |
| D-21 | 跨空间 | `kernel_call_resume()` 恢复路径 | 24 §4.6 [P1] | **✅ 已解决（2026-08-14 核实）**：kernel_call_resume 完整实现（vm.rs:896）+ 2 测试（vm.rs:1303/:1316） |
| D-22 | 跨空间 | `dispatch_vircopy` 集成 `data_copy_vmcheck`（当前走 virtual_copy_vmcheck 未走 VMSUSPEND 路径） | 24 §4.6 [P1] | **✅ 已解决（2026-08-14 核实）**：dispatch_vircopy 走 `cross_space::data_copy_vmcheck`（syscall_copy.rs:220/:344-345，VMSUSPEND 路径）；D-20 的 SIGKMEM 通知接线仍 DEFERRED |
| D-23 | 跨空间 | aarch64/riscv64 PTE walk | 24 §4.6 [P2] | **✅ 已解决（2026-08-14 核实）**：三架构 `PteWalkArch` 完整实现——x86_64/paging.rs、arm64/paging.rs:310（4 级）、riscv64/paging.rs:334（Sv39 3 级） |
| D-24 | 跨空间 | `VmSuspendContext` Map 变体未使用（当前仅 KernelCall + DeliverMsg） | 24 §4.6 [P2] | 随 IPC/message deliver 推进 |
| D-25 | 跨空间 | 部分拷贝进度报告（`CrossSpaceResult::Suspended(fault, usize)` 扩展方向） | 24 §4.6 [P2] | 未来可扩展 |
| D-26 | GETINFO | `GET_PROC`/`GET_PROCTAB`（do_getinfo.c） | 25 附录 | **✅ 已解决（2026-08-14）**：已实现为 `GetInfoRequest::Proc`/`ProcTab`（misc.rs:732/:754），非 C 布局而是 KProcess Rust 语义（rewrite 目标） |
| D-27 | GETINFO | `GET_PRIV`/`GET_PRIVTAB` | 25 附录 | **✅ 已解决（2026-08-14）**：`Priv`/`PrivTab`（misc.rs:841/:791） |
| D-28 | GETINFO | `GET_REGS` | 25 附录 | **✅ 已解决（2026-08-14）**：`Regs`（misc.rs:863） |
| D-29 | GETINFO | 其余：`GET_IMAGE`/`MONPARAMS`/`IRQHOOKS`/`IRQACTIDS`/`IDLETSC` | 25 附录 + §2.1 | **✅ 已解决（2026-08-14）**：`Image`/`MonParams`/`IrqHooks`/`IrqActids`/`IdleTsc`（misc.rs:1083/:1121/:1051/:958/:988） |
| D-30 | UPDATE | `swap_memreq`（do_update.c:313-337） | 25 附录 | 🟢 **设计 no-op（2026-08-14 核实）**：swap 时两个进程均不可运行 → swap 无操作（misc.rs:1778 注释 + doc 25；与 W-6 ClearMapCache 同类，见 §7.6） |
| D-31 | SPROF | 数据拷贝（sprof_info + 采样缓冲区 → 用户态） | 25 §4.5 | **✅ 已解决（2026-08-14 核实）**：info + buffer 双重 `data_copy_vmcheck`（misc.rs:2067-2070，do_sprofile.c:117-120 语义） |
| D-32 | SPROF | `clean_seen_flag`（do_sprofile.c:25-31） | 25 附录 | **✅ 已解决（2026-08-14）**：`clean_seen_flag` helper（misc.rs:1951）+ PROF_START（:2017）/ PROF_STOP（:2149）调用（do_sprofile.c:91/:122 语义）+ 测试 test_sprof_start_clears_seen_flags |
| D-33 | 随机数 | x86 RDRAND 熵采集（当前 kernel 侧 no-op stub 匹配 C i386/earm） | 25 §4.7 D3 | 应在 os/arch/src/x86_64/ 实现（当前 deferred）；实际熵采集由用户态 random 驱动 |
| D-34 | VM | `kinfo.mmap_size` + `mem_high_phys` 更新（08 委派，C add_memmap 更新，Rust KernelInfo immutable） | 09 §6.1 + 08 §4 | VM direct map 实际大小 + 最高物理地址在 boot 阶段确定后，dispatch_vmctl 补充分支或独立 SYS_GETINFO 路径 |
| D-35 | VM | `VmInhibitSet` SMP IPI（dispatch_vmctl，RTS_SET(VMINHIBIT)） | 09 §4 | **部分实现（2026-08-14 核实）**：本地路径已实现（syscall.rs:1883-1903 RTS_SET(VMINHIBIT)）；SMP IPI 远程路径待实现 |
| D-36 | SMP | `smp_init`（ACPI/MADT 表解析 + `SmpArch::boot_ap`） | 16 §4.14 | 16 文档自身 DEFERRED |
| D-37 | SMP | `boot_lock`（smp.c:28） | 16 §4.14 | 随 smp_init（D-36）一并实现 |
| D-38 | SMP | **BKL 接入 4 处**：exception_dispatcher::handle 需 bkl_lock() / kmain switch_to_user 前获取 / switch_to_user 调度循环前 bkl_unlock / kernel_call_resume | 16 §4.13 | ②③ **✅ 已解决（2026-08-14）**：lib.rs:1956 bkl_lock（switch_to_user 前）+ lib.rs:2167 bkl_unlock（调度循环前）；①④ 仍 DEFERRED（SMP 异常路径） |
| D-39 | SMP | x86_64 `init_ap` panic 占位（protection.rs:368-370） | 16 §5.3 [P1] | AP 启动路径；占位触发后无测试（→ 见 T-2） |
| D-40 | SMP | ptproc per-CPU 语义（PostInitArch::set_ptproc 三架构占位） | 16 §5.3 [P1] | SMP 落地后 per-CPU ptproc 跟踪 |
| D-41 | 平台 | `PLATFORM` AssumeSyncCell SMP 替换（AP_STARTUP 并发访问时替换为 Mutex/Atomic） | 04 §3.3 TODO [P1] | SMP 就绪后失效；由 16-smp 在 AP bring-up 前完成 |
| D-42 | return-path | `TrapReturnArch` trait 定义 + asm impl（iretq/eret/sret + GP 寄存器恢复，trap_return.rs 未创建） | 10 §4.1 + 14 §4.8 + 03 §4.3 | 待 doc 10 switch_to_user 完整调度循环落地；拟议签名 `trait TrapReturnArch: ExceptionArch { type RegisterFile; unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> ! }` |
| D-43 | 调度 | `SC_TRACE`/`SC_ACTIVE` misc flags 处理 | 10 §4.2 | future phase（依赖 signal module） |
| D-44 | 调度 | `vm_suspend(VMS_PAGEFAULT)` 路由（switch_to_user 内 TODO） | 10 §4.2 | future phase |
| D-45 | 调度 | `cause_sig(SIGSEGV)` 路由（switch_to_user DeliverResult::Segfault 分支） | 10 §4.2 | future phase |
| D-46 | 中断 | Timer IRQ 链注册 dummy handler（lib.rs:1901-1921） | 08 §4 | 待 trap entry 连接 `IrqManager::dispatch`（Step 1.5.7） |
| D-47 | 调试 | `util_stacktrace()` 不展开栈 | 27 §6.3 | 待 arch stack walker |
| D-48 | panic | panic handler 增强：消息输出 / CPU 号 / `minix_shutdown(0)` / 栈展开 | 27 §6.3 | 待 shutdown 协议 + arch stack walker + `minix_sys::write(STDERR)` |
| D-49 | 特权 | `set_sendto_bit`（system.c:307-330 运行时设置 s_ipc_to 位） | 22 §4.6 | boot 阶段由 grant_capability 的 ipc_mask 替代；运行时设置路径未实现 |
| D-50 | 架构清理 | `set_ptproc`/`PostInitArch`/`MemoryInitArch`/`FreePdeSlots` 待废弃（createpde 已被 Direct Map 取代） | 07 §1.4 TODO-07-1 | 修复 lib.rs:905-972 旧逻辑后自然废弃（MemoryInitArch 对接 DirectMapArch） |

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
| W-8 | 25 | `swap_memreq`（do_update.c:313-337，D-30） | 🟢 设计 no-op（2026-08-14 核实）：当前 vmrequest 链无入队路径（D-20 DEFERRED）→ 链恒空 → swap 无操作（misc.rs:1778 注释 + doc 25，与 W-6 同类）；**D-20 落地时需重审** |

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

### 7.5 与架构建议（§1-§6）交叉引用

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
| 25 swap_memreq（D-30） | 🟢 设计 no-op | misc.rs:1778 注释 + doc 25（2026-08-14）：vmrequest 链无入队路径 → 链恒空 → swap 无操作；与 W-6 同类，D-20 落地时重审 |



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

| §5 子节 | 测试函数 | 位置 | 状态 |
|---------|---------|------|------|
| 5.1 | `test_set_ptproc_accepts_valid_vm_page_table_info` | `arch/src/{x86_64,arm64,riscv64}/post_init.rs` | ✅ |
| 5.2 | `test_memory_init_arch_allocates_two_consecutive_pdes` | `arch/src/arch/post_init.rs` | ✅ |
| 5.2 | `test_memory_init_arch_advances_free_upper_idx_by_two` | `kernel/src/lib.rs` (`test_advance_free_upper_idx`) | ✅ 已有 |
| 5.2 | `test_allocate_free_pdes_panics_on_overflow` | `arch/src/{x86_64,arm64,riscv64}/post_init.rs` (per-arch) | ✅ |
| 5.2 | `test_free_pde_slots_new_is_empty` | `arch/src/arch/post_init.rs` | ✅ |
| 5.2 | `test_free_pde_slots_push_*` | `arch/src/arch/post_init.rs` | ✅ |
| 5.3 | `test_init_post_and_memory_phase_d_dependencies` | `kernel/src/lib.rs` | ✅ |
| 5.3 | `test_free_pde_slots_global_persists_after_init` | `kernel/src/lib.rs` (`test_free_upper_idx_starts_at_zero`) | ✅ 已有 |
| 5.3 | `test_createpde_does_not_reallocate_slots` | `kernel/src/lib.rs` | ✅ |
| 5.1/5.3 | ptproc 未初始化 / virt_root=None / mem_clear_mapcache | — | DEFERRED（见 §12.2） |

**编译验证**：
- `cargo test -p minix-arch --features mock --lib -- post_init::tests` / `x86_64::post_init::tests` → OK
- `cargo test -p minix-kernel --features mock --lib` → OK (test_createpde + test_phase_d_dependencies)

###### 12.2 [P1] ptproc per-CPU 变量未实现（架构 TODO）

**问题**：`PostInitArch::set_ptproc()` 当前在三个架构实现中都只是占位（`let _ = vm_page_table`），未实际存储 ptproc 指针。这意味着 `createpde()` 等后续依赖 ptproc 的功能无法工作。

**位置**：
- `os/arch/src/x86_64/post_init.rs:46-58`
- `os/arch/src/arm64/post_init.rs:38-48`
- `os/arch/src/riscv64/post_init.rs:42-52`

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
| 阶段 C（`arch_boot_proc(VM)`） | `pg_map(PG_ALLOCATEME, ...)` 添加 VM 映射 | 同 C |

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
| §4 | 页表页分配器 VM 阶段接入 | 🔀 转移 | `02-stage-vm/00-vm-overview.md` §8.4 |
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
| `02-stage-vm/00-vm-overview.md` | §8.4 | 页表页分配器 VM 阶段接入（原 §4）+ minix-vm 116 clippy warnings（原 §11.3）+ **minix-vm 15 测试失败（本次发现，pre-existing）** + AcpiDesc 最小化扩展（原 §7） |
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
