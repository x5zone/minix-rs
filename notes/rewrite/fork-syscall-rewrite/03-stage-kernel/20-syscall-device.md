# 20-syscall-device: 设备 I/O 系统调用

> **分类**: 系统调用服务
> **源码**: `minix3/minix/kernel/system/do_irqctl.c`, `do_devio.c`, `do_vdevio.c`
> **前置**: 13（异常/中断——IRQ hook 机制）, 21（权限——CHECK_IO_PORT/CHECK_IRQ）
> **C 总行数**: ~450 行

---

## Ch1: 概念

**核心问题**: 内核如何让用户态驱动程序安全地访问硬件？

设备 I/O 系统调用分为三类：

1. **IRQ 控制**（IRQCTL）：注册/删除/启用/禁用中断钩子
2. **端口 I/O**（DEVIO/VDEVIO/SDEVIO）：读写 I/O 端口
3. **特殊**（IOPENABLE/READBIOS）：x86 特定的 I/O 权限和 BIOS 读取

### 1.1 IRQ 控制

| 子请求 | 语义 | C 常量 |
|--------|------|--------|
| IRQ_SETPOLICY | 注册中断钩子 | `IRQ_SETPOLICY` |
| IRQ_RMPOLICY | 删除中断钩子 | `IRQ_RMPOLICY` |
| IRQ_ENABLE | 启用中断 | `IRQ_ENABLE` |
| IRQ_DISABLE | 禁用中断 | `IRQ_DISABLE` |

**IRQ_SETPOLICY 流程**：
1. 验证 IRQ 向量号在范围内
2. 检查 `CHECK_IRQ` 权限（`s_irq_tab[]`）
3. 查找现有钩子（同 endpoint + notify_id）或空闲钩子
4. 安装钩子：设置 proc_nr_e、notify_id、policy
5. 调用 `put_irq_handler()` 注册到中断控制器

**中断发生时的 generic_handler**：
1. 采集随机数（/dev/random）
2. 设置 `s_int_pending |= (1 << notify_id)`
3. `mini_notify(HARDWARE, proc_nr_e)` 唤醒驱动
4. 如果 policy 含 `IRQ_REENABLE`，返回时重新启用

### 1.2 端口 I/O

| 系统调用 | 语义 | 架构 |
|---------|------|------|
| SYS_DEVIO | 单个端口读写 | x86 |
| SYS_VDEVIO | 批量端口读写 | x86 |
| SYS_SDEVIO | 安全端口读写（带 grant） | x86 |
| SYS_IOPENABLE | 启用用户态 I/O 权限 | x86 |
| SYS_READBIOS | 读取 BIOS 数据 | x86 |

**DEVIO 流程**：
1. 解析请求类型（byte/word/long）和方向（input/output）
2. 检查 `CHECK_IO_PORT` 权限（`s_io_tab[]`）
3. 对齐检查
4. 执行 `inb/inw/inl` 或 `outb/outw/outl`

**VDEVIO 流程**：
1. 从用户空间拷贝 (port, value) 向量
2. 批量权限检查
3. 批量执行 I/O
4. 输入模式下拷贝结果回用户空间

### 1.3 架构相关性

DEVIO/VDEVIO/SDEVIO/IOPENABLE/READBIOS 都是 x86 特有的（I/O 端口是 x86 概念）。其他架构（ARM/RISC-V）使用内存映射 I/O，不需要这些调用。

设计决策 D9（08-system-init）：这些调用在非 x86 架构上返回 `KcallResult::BadCall`。

---

## Ch2: C 源码分析

### do_irqctl.c (174 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-174 | `do_irqctl()` | switch(request): SETPOLICY/RMPOLICY/ENABLE/DISABLE |
| 176-200 | `generic_handler()` | 中断发生时的通用处理 |

### do_devio.c (107 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 18-107 | `do_devio()` | 解析类型/方向 → 权限检查 → 执行 in/out |

### do_vdevio.c (165 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 24-165 | `do_vdevio()` | 拷入向量 → 批量权限检查 → 批量 I/O → 拷出结果 |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | IRQ 子请求 | 整数 vs enum | **`IrqctlRequest` enum** | 类型安全 |
| D2 | I/O 端口操作 | 直接 in/out vs trait | **`trait PortIo`** | 多架构支持，ARM/RISC-V 用 MMIO |
| D3 | IRQ hook 池 | 全局数组 vs IrqManager | **IrqManager (DEFERRED @ dispatch)** | 类型已在 `irq_manager.rs` 定义，但 `syscall.rs:dispatch_irqctl` 暂返回 `BadCall`（依赖 P0-07 `KernelState` 重构） |
| D4 | generic_handler | 函数指针 vs 闭包 | **IrqHook 结构体** | 已有设计 |
| D5 | VDEVIO 缓冲区 | 静态数组 vs 栈分配 | **栈分配 `[u8; VDEVIO_BUF_SIZE]`** | no_std，避免静态可变 |
| D6 | x86-only 调用 | `#[cfg(target_arch)]` vs trait + BadCall | **trait + BadCall** | D9 全局决策 |

> **⚠️ D3 当前状态：DEFERRED @ dispatch**
>
> D3 表格行易被误读为"已实现"。完整状态：
> - **类型层**：`irq_manager.rs` 中的 `IrqManager<IC: InterruptController>` 已定义，4 个 sub-request（`SetPolicy/RmPolicy/Enable/Disable`）实现层完整。
> - **dispatch 层**：`syscall.rs:dispatch_irqctl` 当前 `BadCall`，因为 dispatch 函数尚未持有 `&mut IrqManager<ArchIc>`——`KernelState` 单态未确定。
>
> 完整 DEFERRED 状态、根因、优先级路径见本文末尾 **DEFERRED 状态说明** 小节。
```
---

## Ch4: 实现要点

### 4.1 IrqctlRequest enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum IrqctlRequest {
    SetPolicy = 0,
    RmPolicy = 1,
    Enable = 2,
    Disable = 3,
}
```

### 4.2 trait PortIo

```rust
/// Architecture-specific port I/O operations.
pub trait PortIo {
    fn read_byte(port: u16) -> u8;
    fn read_word(port: u16) -> u16;
    fn read_long(port: u16) -> u32;
    fn write_byte(port: u16, value: u8);
    fn write_word(port: u16, value: u16);
    fn write_long(port: u16, value: u32);
}
```

### 4.3 IoSize enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoSize {
    Byte = 1,
    Word = 2,
    Long = 4,
}
```

---

## 测试

- 单元：IrqctlRequest TryFrom<i32>
- 单元：IoSize from request mask
- 单元：权限检查逻辑（CHECK_IO_PORT / CHECK_IRQ）
- 单元：VDEVIO 对齐检查

---

## DEFERRED 状态说明（2026-06-14 更新）

### 范围

本节记录 `dispatch_irqctl` / `dispatch_devio` / `dispatch_vdevio` 在 dispatch 层的当前实现状态与未来路径。

### 当前 dispatch 层（`os/kernel/src/syscall.rs`）

```rust
// syscall.rs:352-364
fn dispatch_irqctl(caller: &mut KProcess, msg: &mut Message) -> KcallResult {
    // DEFERRED: dispatch_irqctl now requires &mut IrqManager<IC> + &PrivTable.
    // The IrqManager is generic over InterruptController and not yet available
    // as a global. When KernelState is refactored to hold IrqManager, this
    // dispatch will be updated to pass it through.
    // For now, the function exists in syscall_device.rs with the correct
    // signature but cannot be called from this dispatch layer.
    let _ = (caller, msg);
    KcallResult::BadCall
}
```

`dispatch_devio` / `dispatch_vdevio` 同理：当目标架构为 x86_64 时转发到 `syscall_device::dispatch_devio`；其他架构返回 `BadCall`（设计决策 D9）。

### 实现层（`os/kernel/src/syscall_device.rs`）

| 函数 | 行号 | 状态 |
|------|------|------|
| `dispatch_irqctl` | 174-323 | **部分实现**：所有 4 个子请求（SETPOLICY/RMPOLICY/ENABLE/DISABLE）已分支，但 handler 注册路径（`put_irq_handler`）依赖 `IrqManager<IC>`，当前 `IrqManager` 尚未作为全局状态暴露 |
| `dispatch_devio` | 325-417 | **已实现 (2026-06-16)**：PortIo trait 迁移至 minix_plat, x86_64 X8664PortIo 实现, dispatch_arch_devio 接线, CHECK_IO_PORT + 对齐检查 + I/O 执行 + 结果写回 |
| `dispatch_vdevio` | 419-475 | **部分实现**：参数提取 + 类型/方向解析 + vec_size 校验已实现; **语义漂移已修复 (2026-06-16): OK→ENOSYS**; dispatch_arch_vdevio x86_64 已接线; 批量 I/O 执行 DEFERRED (需 data_copy_vmcheck) |
| `dispatch_iopenable` | 491-542 | **已实现 (2026-06-16)**：SELF endpoint 解析 (caller.p_endpoint) + endpoint 验证 + iskerneln 检查 + IOPL=3 修改 initial_status (C: enable_iop pp->p_reg.psw |= 0x3000); &mut ProcessTable 接入; 5 个单元测试; trap frame 更新 DEFERRED (需 scheduler 集成) |
| `dispatch_sdevio` | 同上 | **DEFERRED** (需 MessLsysKrnSysSdevio 消息类型) |
| `dispatch_readbios` | arch 层 | **DEFERRED** (需 virtual_copy_vmcheck) |

### DEFERRED 根因

1. **`IrqManager<IC>` 不可全局访问** — 该类型在 `irq_manager.rs` 中以 `GenericIrqManager<IC: InterruptController>` 形式存在，是泛型且非单态。`KernelState` 尚未持有具体单态（如 `IrqManager<X86Apic>`）。修复路径：在 `KernelState` 添加 `irq_mgr: IrqManager<ArchIc>` 字段（**P0-07 依赖**）。

2. **`data_copy_vmcheck` 缺失** — `dispatch_devio` 需要从用户空间拷贝 `IoVecSap` 描述符到内核栈（`do_devio.c:36-43`），当前 `dispatch_devio` 直接 panic 在 `unimplemented!()` 上（如果跳过检查）。修复路径：实现 `data_copy_vmcheck` + `data_copy_normal`（**P1-05 / P0-07 依赖**）。

3. **架构单态未确定** — `dispatch_arch_devio` 在 `#[cfg(not(target_arch = "x86_64"))]` 分支返回 `BadCall`；在 x86_64 分支转发到 `dispatch_devio`。这与 D9 决策一致，不需要重新设计。

### 优先级与依赖图

```
P0-01 BKL cross-CPU wiring
   └── P0-07 KernelState 单态
          ├── P1-09 dispatch_irqctl 完整 (IrqManager<ArchIc> 接入)
          └── P1-05 dispatch_copy Direct Map
                 └── dispatch_devio / dispatch_vdevio 完整
```

### 测试覆盖现状

| 测试 | 状态 |
|------|------|
| `IrqctlRequest::try_from` 全分支覆盖 | ✅ |
| `IoSize` 解码（mask → enum） | ✅ |
| `check_irq_permission` 边界 | ✅ |
| `dispatch_irqctl` 子请求 → `IrqManager.put_irq_handler` | ❌（DEFERRED） |
| `dispatch_devio` 对齐检查 / 权限检查 | ✅ (2026-06-16) |
| `dispatch_vdevio` 批量路径 | ⚠️ Partial (参数验证+ENOSYS; 批量I/O DEFERRED) |

### 关联 TODO

- `todo.md §2` — `data_copy` 子系统回收
- `todo.md §5` — code-review 04-5 项（部分与 P1-09 重叠）
- `STATE.md §4` — P0-07 剩余项 `AddIpcBlFilter` / `AddIpcWlFilter` 是 P1-09 派生的剩余 work

---

## 参见

- [14-exception-interrupt.md](14-exception-interrupt.md) — IRQ hook 注册/分发
- [22-privilege.md](22-privilege.md) — CHECK_IO_PORT / CHECK_IRQ 权限
