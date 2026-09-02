# Plan: IRQ 通知链路完成 + 01-14 TODO 审计 + doc 14 回归 Review

> **三任务顺序执行**：Task 1 → Task 2 → Task 3
> 用户明确要求：不选简易方案；多方案优中选优；彻底修复不留坑。

---

## Summary

- **Task 1**：完成 `generic_notify_handler` TODO 的完整方案——包括 IRQ→IPC 通知链路设计、IrqManager 全局化、`build_notify_message` HARDWARE case、trap 入口接入 dispatch。涉及 6+ 文件改动。
- **Task 2**：审计 01-14 文档全部 TODO/stub，完成所有不依赖缺失子系统的项。预期可完成项：`MessNotify` 结构体补充、doc-only TODO 修复、`generic_notify_handler` 关联项。
- **Task 3**：对 doc 14 + 关联 Rust 实现做深度回归 review（含 Step 0-7 全流程）。

---

## Current State Analysis

### Task 1 现状

| 组件 | 现状 | 问题 |
|------|------|------|
| `generic_notify_handler` (irq_manager.rs:460-472) | 返回 `Completed`，无通知逻辑 | 签名 `fn(IrqVector, IrqId) -> IrqAction` 无法访问 slot 的 `proc_endpoint`/`notify_id`，无法触达 IPC |
| `mini_notify` (ipc.rs) | **不存在**为独立函数 | 只有 `IpcEngine::notify`（per-syscall，短生命周期），IRQ dispatch 路径无法使用 |
| `IrqManager` 全局实例 | **不存在**（lib.rs:1247-1291 标 DEFERRED） | 仅测试中实例化；trap 入口无法调用 `dispatch` |
| `build_notify_message` (ipc.rs:998-1007) | HARDWARE source case TODO | `MessNotify` 变体不在 `MessageUnion` 中，`s_int_pending` 无法填入消息 |
| `PROC_TABLE`/`PRIV_TABLE` | 全局 `static mut`（lib.rs:1022/1027） | 可通过 `unsafe fn proc_table()`/`priv_table()` 访问（需持 BKL） |
| `InterruptController` trait | `Sized + Send + Sync`（plat/interrupt.rs:129） | 非 object-safe（`fn new(desc) -> Self`），无法 `dyn` |

### Task 2 现状（01-14 TODO 审计结果）

| Doc | TODO | 分类 | 可完成？ |
|-----|------|------|---------|
| 01 | boot module 内存回收 | BLOCKED（需 boot module 加载子系统） | ❌ |
| 01 | QEMU+OpenSBI+U-Boot 集成测试 | BLOCKED（需工具链） | ❌ |
| 03 | 内核内存管理 | BLOCKED（需 arch） | ❌ |
| 05 | boot loading 覆盖 | DOC-ONLY | ✅ 文档修复 |
| 08 | `add_memmap` kinfo 字段 | DEFERRED 到 09 | ❌ |
| 08 | `krandom_init` | BLOCKED（需随机数源） | ❌ |
| 10 | IrqManager 与早期 AP 上下文切换 | 关联 Task 1 | ✅ 随 Task 1 完成 |
| 11 | 文档不完整需验证 | NEEDS VERIFICATION | 待审计 |
| 12 | 检查 task 发送失败描述缺失 | DOC-ONLY | ✅ 文档修复 |
| 13 | arch-specific code TODO | DOC-ONLY | ✅ 文档修复 |
| 14 | `generic_notify_handler` | Task 1 主体 | ✅ |
| 14 | `build_notify_message` HARDWARE case | 关联 Task 1 | ✅ |
| 14 | `MessNotify` 变体缺失 | 关联 Task 1 | ✅ |

### Task 3 现状

doc 14 已于 2026-07-31 完成重写（651→591 行），关联代码已修复（IRQ unmask 逻辑、重复实现删除、注释更新）。本次是重写后的**回归 review**，验证重写质量。

---

## Task 1: 完成 generic_notify_handler TODO（完整方案）

### §1.1 设计决策 D1：handler 上下文传递方式（多方案）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 改 handler 签名为 `fn(&mut IrqHookContext) -> IrqAction`** | handler 接收完整上下文（含 proc_endpoint/notify_id/notifier 引用），handler 内自行通知 | 与 C `generic_handler(irq_hook_t *)` 语义对齐；handler 有全部信息；通知逻辑在 handler 内（非 dispatch 内）；可测试 | 改变 handler 类型签名（破坏性）；需迁移现有 handler |
| **B. dispatch 后置通知（trait 注入）** | handler 签名不变；`dispatch` 额外接收 `&mut dyn IrqNotify`；handler 返回后 dispatch 调用 `notifier.notify_hardware(slot.proc_endpoint, slot.notify_id)` | handler 签名不变；通知逻辑集中 | **语义偏移**：C 中通知是 handler 的职责（`generic_handler` 内调 `mini_notify`），非 `irq_handle` 的职责；所有 hook 都会被通知（即使非 generic） |
| **C. dispatch 后置通知 + handler 标志位** | 同 B 但 `IrqHookSlot` 加 `notify: bool` 字段，只有 `generic_notify_handler` 注册的 hook 标 `true` | 比 B 精确 | 仍然语义偏移；额外字段；`bool` 标志不如类型系统表达 |
| **D. `generic_notify_handler` 直接用全局表** | handler 内 `unsafe { proc_table() }` + `priv_table()` 直接操作 | 最简单 | `unsafe` 散布；handler 不可测试（依赖全局态）；违反"硬件全 trait、OS 策略不 unsafe"原则 |

**选定 A**：改 handler 签名。理由：
1. **语义正确**：C 的 `generic_handler(irq_hook_t *hook)` 接收 hook 指针，通知是 handler 的职责。方案 A 与此对齐。
2. **类型安全**：`IrqHookContext` 把 slot 信息以类型化方式传入，无 `unsafe`。
3. **可测试**：`IrqNotify` trait 可 mock；handler 接收 `&mut IrqHookContext` 可构造测试上下文。
4. **不 translate**：C 用 `irq_hook_t *` 裸指针 + 全局 `mini_notify`；Rust 用 `&mut IrqHookContext` + `&mut dyn IrqNotify` trait 注入——这是 Rust 类型系统的重新表达，非 1:1 翻译。

### §1.2 设计决策 D2：IrqManager 全局化方式（多方案）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. arch 专属 static（每 arch 定义具体 IC 类型）** | `static mut IRQ_MANAGER: IrqManager<X86_64Apic>`（x86_64）/ `IrqManager<GicV3>`（aarch64） | 类型安全；无 dyn 开销；每 arch 独立 | 需每 arch 模块定义；访问函数需 arch 条件编译或 trait |
| **B. `OnceCell<IrqManager<ConcreteIC>>`** | boot 时 `OnceCell::set(IrqManager::new(ic))` | 延迟初始化 | `ConcreteIC` 类型仍需 arch 确定；OnceCell 在 no_std 需自实现或依赖 |
| **C. trait object `&'static dyn IrqManagerTrait`** | IrqManager 的方法提取为 trait，用 `dyn` 擦除 IC 类型 | 单一全局变量 | `InterruptController` 非 object-safe（`fn new -> Self`）；需额外 trait 包装层；dyn 开销 |
| **D. 存入 arch boot 结构** | IrqManager 作为 `CurrentArchBoot` 的字段 | 与现有 arch 抽象对齐 | `CurrentArchBoot` 可能不是 `static`；生命周期复杂 |

**选定 A**：arch 专属 static。理由：
1. **类型安全**：保留 `IrqManager<IC>` 的泛型类型，IC 方法静态分发。
2. **与现有模式一致**：`PROC_TABLE`/`PRIV_TABLE` 已是 `static mut` + `unsafe fn` 访问模式，IrqManager 同理。
3. **BKL 保护**：与 `proc_table()`/`priv_table()` 相同的 safety contract（caller must hold BKL）。
4. **arch 分离**：每 arch 模块定义自己的 `IRQ_MANAGER: IrqManager<ConcreteIC>` + `unsafe fn irq_manager() -> &'static mut IrqManager<ConcreteIC>`。

### §1.3 设计决策 D3：`build_notify_message` HARDWARE case（多方案）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 添加 `MessNotify` 到 `MessageUnion`** | 新建 `MessNotify { timestamp: u64, interrupts: u32, sigset: u64 }`，加入 `MessageUnion` 变体；`build_notify_message` 按 source 填充 | 与 C `BuildNotifyMessage` 宏对齐；消息格式完整 | 改 `minix-types` crate（跨 crate）；`MessageUnion` 是 `union`，需 `unsafe` 访问 |
| **B. 侧信道投递** | `s_int_pending` 不通过消息体传递；接收方通过额外 syscall 查询 | 不改 MessageUnion | 语义偏移（C 的 `m_notify.interrupts` 是消息体一部分）；接收方需额外查询 |
| **C. 延迟到 MessNotify 已有时** | 不在本任务做，等 minix-types 添加 MessNotify | 改动最小 | 仍然 TODO；Task 1 不完整 |

**选定 A**：添加 `MessNotify`。理由：
1. **语义完整**：C 的 `BuildNotifyMessage` 把 `s_int_pending` 填入 `m_notify.interrupts`，接收方一次 RECEIVE 即拿到全部信息。
2. **不拖延**：`MessNotify` 是 IPC 协议的必要部分，不是可选增强。
3. **改动可控**：`MessNotify` 是简单的 POD 结构，`MessageUnion` 是 `#[repr(C)] union`，添加变体是安全的 additive 改动。

### §1.4 设计决策 D4：`mini_notify` 独立函数 vs IpcEngine 方法（多方案）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 提取 `mini_notify_core` 为自由函数** | `pub fn mini_notify_core(procs: &mut [KProcess], priv_table: &mut PrivTable, caller_ep: Endpoint, dst_ep: Endpoint) -> IpcOutcome`；`IpcEngine::notify` 委托给它；IRQ 路径用全局表调用 | 逻辑单一真相；syscall 路径和 IRQ 路径共用 | 自由函数参数多 |
| **B. IRQ 路径构造临时 IpcEngine** | `let mut engine = IpcEngine::new(unsafe { proc_table() }.procs_mut(), unsafe { priv_table() }, &KernelUserCopy); engine.notify(HARDWARE, dst)` | 复用 IpcEngine | `IpcEngine::new` 借用全局表 `&mut`——与 `proc_table()` 返回 `&'static mut` 冲突（多次借用）；`KernelUserCopy` 在 IRQ 路径无意义 |
| **C. 独立 `kernel_mini_notify` 函数** | 新建 `pub fn kernel_mini_notify(caller_ep: Endpoint, dst_ep: Endpoint)` 用全局表，逻辑与 `IpcEngine::notify` 重复 | 独立 | 逻辑重复；两份代码需同步维护 |

**选定 A**：提取 `mini_notify_core` 自由函数。理由：
1. **单一真相**：通知逻辑只在一处，syscall 和 IRQ 路径共用。
2. **可测试**：`mini_notify_core` 接收 `&mut [KProcess]` + `&mut PrivTable`，测试可传入 mock 表。
3. **无重复**：`IpcEngine::notify` 委托给 `mini_notify_core`，无第二份代码。

### §1.5 实施步骤

#### Step 1.5.1: 添加 `MessNotify` 到 `minix-types`

**文件**: `os/libs/minix-types/src/ipc/message.rs` + `os/libs/minix-types/src/ipc/notify.rs`

- 在 `notify.rs` 新建 `MessNotify` 结构体（对齐 C `mess_notify`）：
  ```rust
  /// Notification message payload.
  /// C: `mess_notify` — minix3/minix/kernel/ipc.h
  #[derive(Clone, Copy, Default)]
  #[repr(C)]
  pub struct MessNotify {
      pub timestamp: u64,    // get_monotonic()
      pub interrupts: u32,   // s_int_pending bitmap (HARDWARE source)
      pub sigset: u64,       // s_sig_pending (SYSTEM source)
  }
  ```
- 在 `message.rs` 的 `MessageUnion` 添加 `pub m_notify: MessNotify` 变体
- 更新 `MessageUnion::zeroed()` / `Default` 实现

#### Step 1.5.2: 提取 `mini_notify_core` 自由函数

**文件**: `os/kernel/src/ipc.rs`

- 从 `IpcEngine::notify`（ipc.rs:1019-1060）提取核心逻辑为：
  ```rust
  pub fn mini_notify_core(
      procs: &mut [KProcess],
      priv_table: &mut PrivTable,
      caller_endpoint: Endpoint,
      dst_endpoint: Endpoint,
  ) -> IpcOutcome
  ```
- `IpcEngine::notify` 改为委托：`mini_notify_core(self.procs, self.priv_table, caller_ep, dst_ep)`
- 新增 `pub fn kernel_mini_notify(caller_ep: Endpoint, dst_ep: Endpoint) -> IpcOutcome`：
  - `let procs = unsafe { crate::proc_table() }.procs_mut();`
  - `let priv_table = unsafe { crate::priv_table() };`
  - `mini_notify_core(procs, priv_table, caller_ep, dst_ep)`

#### Step 1.5.3: 完成 `build_notify_message` HARDWARE case

**文件**: `os/kernel/src/ipc.rs`（build_notify_message, ~L998-1007）

- 添加 `NotifySource` enum：`pub enum NotifySource { Process(Endpoint), Hardware, System }`
- `build_notify_message` 接收 `NotifySource` 参数
- HARDWARE case：`m_notify.interrupts = priv(dst)->s_int_pending; priv(dst)->s_int_pending = 0;`
- SYSTEM case：`m_notify.sigset = priv(dst)->s_sig_pending; priv(dst)->s_sig_pending = 0;`
- `mini_notify_core` 传递 `NotifySource::Hardware` 当 `caller_endpoint == HARDWARE_ENDPOINT`

#### Step 1.5.4: 改 handler 签名 + IrqHookContext

**文件**: `os/kernel/src/irq_manager.rs`

- 新增 trait + context：
  ```rust
  pub trait IrqNotify {
      fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId);
  }

  pub struct IrqHookContext<'a> {
      pub irq: IrqVector,
      pub id: IrqId,
      pub proc_endpoint: Endpoint,
      pub notify_id: IrqNotifyId,
      pub policy: IrqPolicy,
      pub notifier: &'a mut dyn IrqNotify,
  }
  ```
- handler 类型改为 `pub type IrqHandler = fn(ctx: &mut IrqHookContext) -> IrqAction;`
- `IrqHookSlot.handler` 字段类型同步更新
- `dispatch` 方法改为 `dispatch(&mut self, irq: IrqVector, notifier: &mut dyn IrqNotify)`
- `generic_notify_handler` 改为：
  ```rust
  fn generic_notify_handler(ctx: &mut IrqHookContext) -> IrqAction {
      ctx.notifier.notify_hardware(ctx.proc_endpoint, ctx.notify_id);
      if ctx.policy.contains(IrqPolicy::REENABLE) {
          IrqAction::Completed
      } else {
          IrqAction::NotCompleted
      }
  }
  ```
- 删除 TODO 注释

#### Step 1.5.5: IrqManager 全局化

**文件**: `os/kernel/src/lib.rs` + arch 模块

- 在 `lib.rs` 添加（先以 x86_64 为例，其他 arch 同理）：
  ```rust
  static mut IRQ_MANAGER: Option<IrqManager<ConcreteIC>> = None;
  pub unsafe fn irq_manager() -> &'static mut IrqManager<ConcreteIC> {
      // SAFETY: caller must hold BKL
      IRQ_MANAGER.as_mut().expect("IRQ_MANAGER not initialized")
  }
  ```
  注：`ConcreteIC` 由 arch 模块确定。如果 arch 模块暂无具体 IC 类型，先留 `Option` + boot 时 `set`。
- 在 `bsp_finish_booting`（lib.rs:~1245）初始化 IrqManager：替换现有的 `dummy_timer_handler` 注释为真实注册
- 删除 lib.rs:1247-1291 的 DEFERRED 注释

#### Step 1.5.6: KernelNotifier 实现 IrqNotify

**文件**: `os/kernel/src/irq_manager.rs`（或新 `irq_notifier.rs`）

  ```rust
  pub struct KernelNotifier;
  impl IrqNotify for KernelNotifier {
      fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId) {
          // 1. Set s_int_pending bit
          let priv_table = unsafe { crate::priv_table() };
          if let Some(idx) = /* find dst in proc_table */ {
              if let Some(priv_id) = proc_table[idx].priv_id {
                  if let Some(priv) = priv_table.get_mut(priv_id) {
                      priv.s_int_pending |= 1u32 << notify_id.get();
                  }
              }
          }
          // 2. mini_notify(HARDWARE, dst)
          crate::ipc::kernel_mini_notify(HARDWARE_ENDPOINT, dst);
      }
  }
  ```

#### Step 1.5.7: trap 入口接入 dispatch

**文件**: `os/arch/src/arch/exception_dispatcher.rs` 或 trap_entry.rs

- 在 IRQ 处理路径（非异常）调用：
  ```rust
  let mut notifier = KernelNotifier;
  unsafe { crate::irq_manager() }.dispatch(irq, &mut notifier);
  ```
- 更新 lib.rs:1278-1291 的 timer handler 注册为真实 `register_hook` 调用

#### Step 1.5.8: 更新测试

- `irq_manager.rs` 测试：更新 handler 签名 + 添加 `MockNotifier`
- 新增测试：`generic_notify_handler_sends_notification`——验证 notify_hardware 被调用
- 新增测试：`build_notify_message_hardware_source`——验证 `s_int_pending` 填入消息

#### Step 1.5.9: 更新文档

- `14-exception-interrupt.md` §4.4：删除 TODO P1 标注，描述新设计
- `14-design.v1.md`：添加 D9/D10 设计决策（handler context + IrqManager 全局化）
- `design/14-design.v2.md`：生成 v2 快照记录本轮设计演进

### §1.6 验证

```bash
cd os
cargo test -p minix-kernel --lib irq_manager
cargo test -p minix-kernel --lib ipc::tests
cargo test -p minix-arch exception
cargo build -p minix-kernel
cargo build -p minix-types
cargo clippy -p minix-kernel -- -D warnings
```

---

## Task 2: 审计 01-14 TODO 并完成可完成项

### §2.1 审计步骤

1. 逐文档 grep `TODO|FIXME|stub|todo!|unimplemented!` 并分类
2. 对每个 TODO 判定：COMPLETABLE / BLOCKED / DOC-ONLY
3. COMPLETABLE 项立即完成（含多方案设计）
4. BLOCKED 项在 `todo.md` 更新状态 + 阻塞原因
5. DOC-ONLY 项修复文档

### §2.2 预期可完成项（除 Task 1 已覆盖的）

| Doc | TODO | 行动 |
|-----|------|------|
| 05 | boot loading 覆盖不足 | 添加边界说明指向 01 |
| 11 | 文档不完整 | 审计后补全或标注 |
| 12 | task 发送失败描述缺失 | 补充描述 |
| 13 | arch-specific code TODO | 审计后修复或标注 |

### §2.3 更新 `todo.md`

在 `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/todo.md` 中：
- 标记 Task 1 完成的项为 ✅
- 更新 BLOCKED 项的阻塞原因
- 新增 Task 2 发现的新 TODO（如有）

### §2.4 验证

- 文档改动后 `rg "TODO" notes/rewrite/fork-syscall-rewrite/03-stage-kernel/0[1-9]*.md notes/rewrite/fork-syscall-rewrite/03-stage-kernel/1[0-4]*.md` 确认仅剩 BLOCKED 项
- 代码改动后 `cargo test` 全量通过

---

## Task 3: doc 14 + 关联 Rust 实现深度回归 Review

### §3.1 Review 模式

**深度模式（Deep）**，2 rounds（591 行属 500-1500 区间）：
- R1: 正确性（Step 0-4 + Gate 0/A/B/C/D/D-6/E）
- R2: 卓越性（Step 5 + Gate G + patterns/excellence）

### §3.2 Review 流程

按 `review-process-skill` 执行 Step 0-7：

1. **Step 0**：范围声明 + 时间预算 + STATE 读取 + design/outline 预检（4 条 `ls` 命令）
2. **Step 0.5**：structure.md 生成 + 12 节骨架评审 → Gate D-6
3. **Step 1**：C 源码 Ground Truth 验证（exception.c / interrupt.c / do_irqctl.c）
4. **Step 1.5**：coverage-extract.py 运行 → Gate A
5. **Step 2**：Diff Extraction（Top 5 行为契约表）→ Gate B
6. **Step 3.5**：Precision Check（5 元规则表）→ Gate C
7. **Step 3.5a**：纵向链路检查（14 ↔ 03/05/10/11/12/13/15/16）
8. **Step 3.5b**：因果链抽样验证
9. **Step 4.5**：测试验证（§5 每个 test fn grep）→ Gate E
10. **Step 5**：patterns 对照（模式 1-57）+ excellence 检查
11. **Step 5.6**：VERIFY-CHECK.md 产出 → Gate G

### §3.3 Review 关注点

- Task 1/2 改动后，doc 14 §4.4 是否与新代码一致
- handler 签名变更后，§4.4 的 `generic_notify_handler` 描述是否更新
- `MessNotify` 添加后，§4.6 页错误转发路径描述是否需更新
- 新增设计决策（D9/D10）是否在 design v2 中记录
- 跨文档引用（14→12 mini_notify, 14→10 switch_to_user, 14→16 BKL）是否准确

### §3.4 Review 产物

- `.review/trae/fork-syscall-rewrite/scans/14-exception-interrupt-glm-scan.md`
- `.review/trae/fork-syscall-rewrite/scans/14-exception-interrupt-glm-structure.md`
- `.review/trae/fork-syscall-rewrite/scans/14-exception-interrupt-glm-SYMBOLS.md`
- `.review/trae/fork-syscall-rewrite/VERIFY-CHECK.md`
- 更新 `.review/trae/fork-syscall-rewrite/STATE.md`

---

## Assumptions & Decisions

1. **HARDWARE_ENDPOINT**：C 的 `HARDWARE` 是 `proc_addr(HARDWARE)` 即 `-1`（proc.rs:80）。Rust 已有 `("kernel", -1)` 映射。`kernel_mini_notify` 用此 endpoint 作 caller。
2. **BKL 假设**：IRQ dispatch 在 trap 入口时已持 BKL（x86-64 汇编 trap entry 在 `exception_dispatcher` 前获取 BKL）。`KernelNotifier` 的 `unsafe` 块安全。
3. **IrqManager 初始化时机**：在 `bsp_finish_booting` Step 6（boot_cpu_init_timer）之后、Step 9（switch_to_user）之前。替换现有 `dummy_timer_handler`。
4. **不改 IpcEngine 生命周期模型**：`IpcEngine<'a>` 仍 per-syscall；`mini_notify_core` 是自由函数，不依赖 IpcEngine。
5. **MessNotify 布局**：对齐 C `mess_notify`，`timestamp: u64` + `interrupts: u32` + `sigset: u64`。padding 由 `#[repr(C)]` 保证。
6. **arch 专属 IC 类型**：x86_64 用 `Apic`（已存在于 plat crate）；aarch64/riscv64 同理但本轮只验证 x86_64 编译。
7. **不接入真实硬件中断**：trap 入口汇编可能仍用 dummy——本轮只完成 IrqManager 全局化 + dispatch 可调用性，真实 IRQ 线由 arch 任务接入。

---

## Verification Steps

### Task 1 验证
```bash
cd os
cargo build -p minix-types                    # MessNotify 编译
cargo build -p minix-kernel                   # IrqManager 全局 + KernelNotifier 编译
cargo test -p minix-kernel --lib irq_manager  # IRQ 测试
cargo test -p minix-kernel --lib ipc::tests   # IPC notify 测试
cargo test -p minix-arch exception            # Exception 测试
cargo clippy -p minix-kernel -- -D warnings   # 无 warning
```

### Task 2 验证
```bash
# 确认 01-14 仅剩 BLOCKED TODO
rg "TODO|FIXME" notes/rewrite/fork-syscall-rewrite/03-stage-kernel/0[1-9]*.md \
                   notes/rewrite/fork-syscall-rewrite/03-stage-kernel/1[0-4]*.md \
  --type md -n | grep -v "BLOCKED\|DEFERRED\|已修复"
```

### Task 3 验证
- scan.md 含 9 个 Gate 锚段
- Gate 0/A/B/C/D/D-6/E/G 全部通过
- VERIFY-CHECK.md 判定 PASS（一致性 ≥90%）
- STATE.md 更新为 CONVERGED
