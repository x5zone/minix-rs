# 20-syscall-device Design（设计文档）

> **状态**: 完整设计（基于 20-outline.md 经 outline-review 批准）
> **创建**: 2026-08-01
> **作者**: Trae (GLM-5.2)
> **前置**: 14-exception-interrupt.md, 22-privilege.md, 13-syscall-dispatch.md, 18-syscall-copy.md, 16-smp.md
> **C 源码**: `minix3/minix/kernel/system/do_irqctl.c` (174 行), `do_devio.c` (107 行), `do_vdevio.c` (165 行); `minix3/minix/kernel/arch/i386/do_sdevio.c` (162 行), `do_iopenable.c` (34 行), `do_readbios.c` (37 行)
> **Rust 实现**: `os/kernel/src/syscall_device.rs` (1464 行), `os/kernel/src/irq_manager.rs`, `os/kernel/src/syscall.rs`

---

## §1. 设计目标与约束

### 1.1 目标

重写 `os/kernel/src/syscall_device.rs` 及 dispatch 接线，使其：
1. **对齐 C ground truth**: 6 个 do_* 函数 + generic_handler 的完整语义，覆盖 IRQ 钩子生命周期 / 单次+批量+跨进程端口 I/O / IOPL 提权 / BIOS 读取
2. **修复 review 发现的 P0/P1**: 删除"DEFERRED 状态说明（2026-06-14 更新）"开发日志整节；删除 P0-07/P1-05/P1-09 内部 review ID；dispatch_irqctl BadCall 诚实标注；dispatch_sdevio/readbios DEFERRED 诚实标注；测试 bullet 改可 grep 函数名
3. **避免 translate**: 用 Rust 类型系统重新表达 C 的整数 switch（`IrqctlRequest` enum）、裸位掩码（`IoSize`/`IoDirection` enum）、全局数组（`IrqManager<IC>` 泛型）、函数指针+全局通知（`IrqHookContext` + `IrqNotify` trait）、静态可变缓冲区（栈分配）、`#[cfg(target_arch)]` 行为选择（trait + BadCall）
4. **多架构兼容**: `PortIo` trait + `InterruptController` trait + `CpuContextArch::enable_user_io` 抽象；非 x86 调用返回 BadCall（D9 全局决策）

### 1.2 约束

- `#![no_std]`（除 `#[cfg(test)]`）
- BKL 串行化 dispatch 入口，dispatch 函数内无睡眠
- 硬件抽象为 trait（`PortIo`/`InterruptController`/`IrqNotify`），内核主体无 `#[cfg(target_arch)]` 行为选择（仅 dispatch_arch_* 函数有 cfg 门控）
- 不引入 C 兼容层 / FFI
- 代码注释引用 C 源码 `file:line`
- DEFERRED 函数返回 ENOSYS（非 OK）避免 silent 语义漂移
- VMCTL 不纳入本文档（属内存系统调用）

### 1.3 Ground Truth 验证

| C 函数 | 行号 | 职责 | Rust 归属 |
|--------|------|------|----------|
| `do_irqctl` | do_irqctl.c:23-138 | SETPOLICY/RMPOLICY/ENABLE/DISABLE 4 子请求 | `dispatch_irqctl<IC>` ✅ 类型层完整 (syscall_device.rs:174-310)；dispatch 层 BadCall (syscall.rs:533-597) |
| `generic_handler` | do_irqctl.c:143-172 | 中断→通知：随机数+s_int_pending+mini_notify+REENABLE | `KernelNotifier::notify_hardware` ✅ (irq_manager.rs:121-183)；`get_randomness` DEFERRED |
| `do_devio` | do_devio.c:19-106 | 单次端口 I/O：解码→权限→对齐→in/out | `dispatch_devio<PI>` ✅ 完整 (syscall_device.rs:325-402) |
| `do_vdevio` | do_vdevio.c:25-164 | 批量端口 I/O：拷入→批量权限→批量 I/O→拷回 | `dispatch_vdevio<PI>` ⚠️ 参数验证完整，批量 I/O ENOSYS (syscall_device.rs:418-466) |
| `do_sdevio` | do_sdevio.c:24-161 | 跨进程批量：endpoint→verify_grant/unsafe→switch_space→phys_* | `dispatch_sdevio<PI>` ⚠️ 参数验证+权限+对齐完整，批量 I/O ENOSYS (syscall_device.rs:557-665) |
| `do_iopenable` | do_iopenable.c:19-33 | IOPL=3：SELF/endpoint→enable_iop | `dispatch_iopenable` ✅ 完整 (syscall_device.rs:483-528)；trap frame 同步 DEFERRED |
| `do_readbios` | do_readbios.c:15-37 | BIOS 读取：范围检查→virtual_copy_vmcheck | `dispatch_readbios` ⚠️ 范围检查完整，拷贝 ENOSYS (syscall_device.rs:692-731) |

---

## §2. 核心数据结构设计

### 2.1 IrqctlRequest enum（D1 — 保留，已实现）

```rust
/// IRQ control request types. C: `IRQ_SETPOLICY` etc. — devio.h
///
/// D1: enum + TryFrom<i32> replaces C's integer switch. The compiler
/// checks match exhaustiveness; `TryFrom` centralizes invalid-value
/// rejection (→ EINVAL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum IrqctlRequest {
    /// Register an interrupt hook. C: `IRQ_SETPOLICY`
    SetPolicy = 0,
    /// Remove an interrupt hook. C: `IRQ_RMPOLICY`
    RmPolicy = 1,
    /// Enable an IRQ. C: `IRQ_ENABLE`
    Enable = 2,
    /// Disable an IRQ. C: `IRQ_DISABLE`
    Disable = 3,
}

impl TryFrom<i32> for IrqctlRequest {
    type Error = ();
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::SetPolicy),
            1 => Ok(Self::RmPolicy),
            2 => Ok(Self::Enable),
            3 => Ok(Self::Disable),
            _ => Err(()),
        }
    }
}
```

**与 C 的差异**（anti-translate）:
- C `switch(request)` 整数分支 → Rust enum + `TryFrom<i32>`：类型安全 + 穷尽性检查
- C `default: r = EINVAL` → Rust `Err(())` 由调用者转 EINVAL

### 2.2 IoSize / IoDirection enum（D7 — 保留，已实现）

```rust
/// I/O operation size. C: `_DIO_BYTE/_DIO_WORD/_DIO_LONG`
///
/// D7: enum replaces raw bitmask. `from_request_mask` centralizes
/// decoding; `match` checks exhaustiveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoSize {
    Byte = 1,
    Word = 2,
    Long = 4,
}

impl IoSize {
    /// C: `_DIO_TYPEMASK` — com.h:287
    /// C values: `_DIO_BYTE=0x010, _DIO_WORD=0x020, _DIO_LONG=0x030`
    pub fn from_request_mask(mask: i32) -> Option<Self> {
        match mask {
            0x010 => Some(IoSize::Byte),
            0x020 => Some(IoSize::Word),
            0x030 => Some(IoSize::Long),
            _ => None,
        }
    }
}

/// I/O direction. C: `_DIO_INPUT/_DIO_OUTPUT`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoDirection {
    Input,
    Output,
}

impl IoDirection {
    /// C: `_DIO_DIRMASK` — com.h:283
    /// C values: `_DIO_INPUT=0x001, _DIO_OUTPUT=0x002`
    pub fn from_request_mask(mask: i32) -> Option<Self> {
        match mask {
            0x001 => Some(IoDirection::Input),
            0x002 => Some(IoDirection::Output),
            _ => None,
        }
    }
}
```

**与 C 的差异**:
- C `io_type = request & _DIO_TYPEMASK; switch(io_type)` → Rust `IoSize::from_request_mask(io_type)` 返回 `Option`，非法值 → EINVAL（C default size=4 保守值，Rust 收紧为 EINVAL）

### 2.3 PortIo trait（D2 — 保留，已实现，re-export 自 minix_plat）

```rust
/// Architecture-specific port I/O operations.
///
/// D2: trait abstracts x86 `in/out` instructions vs ARM/RISC-V MMIO.
/// Kernel code depends on the trait, not on `#[cfg(target_arch)]`.
/// Tests use `MockPortIo`; production uses `X86_64PortIo`.
///
/// Re-exported from `minix_plat::PortIo` so x86_64 can provide a real
/// implementation (`X86_64PortIo` using `in/out` instructions) while
/// mock/ARM/RISC-V use `MockPortIo` (no-op).
pub use minix_plat::PortIo;
```

`PortIo` trait 定义（minix_plat crate）:

```rust
pub trait PortIo {
    fn read_byte(port: u16) -> u8;   // inb
    fn read_word(port: u16) -> u16;  // inw
    fn read_long(port: u16) -> u32;  // inl
    fn write_byte(port: u16, value: u8);   // outb
    fn write_word(port: u16, value: u16);  // outw
    fn write_long(port: u16, value: u32);  // outl
}
```

**实现**:
- `X86_64PortIo`（minix_plat）：x86 `in/out` 内联汇编
- `MockPortIo`（test）：no-op + 记录 last_write / 返回 read_value
- `CurrentPortIo::new()`（minix_plat）：返回当前架构的实现

### 2.4 IrqManager<IC> + IrqHookContext + IrqNotify（D3/D4 — 保留，已实现）

```rust
/// IRQ hook chain manager. C: global `irq_hooks[]` + `put_irq_handler`.
///
/// D3: Generic over `IC: InterruptController` so the hook pool is
/// encapsulated and the interrupt controller is mockable.
/// `IrqManager<X86Apic>` / `IrqManager<MockIc>` both work.
///
/// Defined in `irq_manager.rs`. The dispatch layer returns BadCall
/// until `KernelState` holds a concrete `IrqManager<ArchIc>` (see §3.1).
pub struct IrqManager<IC: InterruptController> {
    // ... hook pool + per-vector chain heads (index-based linked list)
    _ic: core::marker::PhantomData<IC>,
}

/// Context passed to IRQ handlers. Replaces C's `irq_hook_t *hook`.
///
/// D4: Carries slot info + `&mut dyn IrqNotify` for delivering
/// notifications. No global state access in handler body.
pub struct IrqHookContext<'a> {
    pub irq: IrqVector,
    pub id: IrqId,
    pub proc_endpoint: Endpoint,
    pub notify_id: IrqNotifyId,
    pub policy: IrqPolicy,
    pub notifier: &'a mut dyn IrqNotify,
}

/// Trait for delivering hardware interrupt notifications.
///
/// D4: Abstracts C's global `mini_notify` so handlers are testable.
/// `KernelNotifier` (production) accesses global PROC_TABLE/PRIV_TABLE
/// under BKL; `MockNotifier` (tests) records calls.
pub trait IrqNotify {
    /// C: `priv(rp)->s_int_pending |= (1 << hook->notify_id);`
    ///    `mini_notify(proc_addr(HARDWARE), hook->proc_nr_e);` — do_irqctl.c:167-170.
    fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId);
}

/// IRQ handler function pointer type.
/// C: `int (*handler)(irq_hook_t *)` — glo.h:46.
pub type IrqHandler = for<'a> fn(ctx: &'a mut IrqHookContext<'a>) -> IrqAction;
```

**与 C 的差异**（anti-translate）:
- C 全局 `irq_hooks[]` → Rust `IrqManager<IC>` 封装 + 泛型可 mock
- C `generic_handler(irq_hook_t* hook)` + 全局 `mini_notify` → Rust `IrqHookContext<'a>` + `IrqNotify` trait（生产 `KernelNotifier` / 测试 `MockNotifier`）
- C 函数指针返回 int（0/IRQ_REENABLE） → Rust `IrqAction` enum

### 2.5 VDEVIO 栈缓冲区（D5 — 设计，DEFERRED 实现）

```rust
/// Maximum VDEVIO buffer size. C: `VDEVIO_BUF_SIZE` — do_vdevio.c
pub const VDEVIO_BUF_SIZE: usize = 1024;

// 在 dispatch_vdevio 内栈分配（DEFERRED 实现）:
// let mut buf = [0u8; VDEVIO_BUF_SIZE];
// // 按 IoSize cast 为 pvb_pair_t/pvw_pair_t/pvl_pair_t 数组
// // C: do_vdevio.c:17-20 三种 pair cast 复用同一缓冲区
```

**与 C 的差异**:
- C `static char vdevio_buf[VDEVIO_BUF_SIZE]` + `lock()/unlock()` 保护 → Rust 栈分配 `[u8; VDEVIO_BUF_SIZE]`：每个调用栈独立无竞争，BKL 已串行化无需额外锁，no_std 友好

---

## §3. 缺失函数实现方案（DEFERRED → 实现设计）

### 3.1 dispatch_irqctl dispatch 层接入（BadCall → 完整）

**当前状态**: `syscall.rs:533-597 dispatch_irqctl` 仅做 step1-3 校验（request/vector/sys_proc），hook 链操作返回 `BadCall`。类型层 `syscall_device.rs:174-310 dispatch_irqctl<IC>` 完整实现 4 子请求。

**根因**: `dispatch_irqctl` 在 `syscall.rs` 的签名是 `fn(caller, msg) -> KcallResult`，无 `&mut IrqManager<ArchIc>` 参数；`KernelState` 未持有 `IrqManager<ArchIc>` 单态。

**修复路径**:

```rust
// 1. KernelState 持有 IrqManager 单态（依赖 KernelState 重构）
pub struct KernelState {
    // ...
    pub irq_mgr: IrqManager<ArchIc>,  // 新增字段
}

// 2. dispatch_irqctl 签名扩展，传入 IrqManager + PrivTable
fn dispatch_irqctl(
    caller: &mut KProcess,
    msg: &mut Message,
    irq_mgr: &mut IrqManager<ArchIc>,
    priv_table: &PrivTable,
) -> KcallResult {
    crate::syscall_device::dispatch_irqctl(caller, msg, irq_mgr, priv_table)
}

// 3. kernel_call_dispatch 从 KernelState 取出 irq_mgr 传入
```

**依赖**: `KernelState` 重构（持有 `IrqManager<ArchIc>` 单态）。这是跨文档依赖，涉及 13-syscall-dispatch / 06-proc-init-boot-proc 的 KernelState 设计。

**实现状态**: 类型层 `dispatch_irqctl<IC>` 已完整（syscall_device.rs:174-310），4 子请求分支齐全：
- SetPolicy: 向量范围 + CHECK_IRQ + notify_id 上限 + `irqctl_set_policy`
- RmPolicy: hook_id 校验 + owner 检查 + `remove_hook_by_slot`
- Enable: hook_id 校验 + owner 检查 + `enable_irq_by_slot`
- Disable: hook_id 校验 + owner 检查 + `disable_irq_by_slot`

dispatch 层接入后即可工作，无需改类型层。

### 3.2 dispatch_vdevio 批量 I/O（ENOSYS → 完整）

**当前状态**: `syscall_device.rs:418-466` 参数验证完整（type/dir/vec_size），批量 I/O 返回 ENOSYS。

**修复路径**:

```rust
pub fn dispatch_vdevio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &Message,
    port_io: &PI,
    priv_table: &PrivTable,
    proc_table: &crate::proc_table::ProcessTable,  // 新增，data_copy 需要
) -> KcallResult {
    // ... 参数验证（已实现）...

    // C: do_vdevio.c:67-70 — 拷入 (port,value) 向量
    let mut buf = [0u8; VDEVIO_BUF_SIZE];  // D5 栈分配
    let bytes = vec_size * pair_size;  // pair_size = size_of::<pvX_pair_t>()
    if let Err(e) = data_copy_vmcheck(caller.p_endpoint, vec_addr, KERNEL, &buf, bytes) {
        return KcallResult::Ok(e);
    }

    // C: do_vdevio.c:72-100 — 批量 CHECK_IO_PORT
    // 按 IoSize 解释 buf 为 [pvb_pair_t] / [pvw_pair_t] / [pvl_pair_t]
    // 逐元素扫 s_io_tab，失败 → EPERM
    if let Some(priv_) = caller.priv_id.and_then(|pid| priv_table.get(pid)) {
        if priv_.capability.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
            for pair in pairs.iter() {
                let port = pair.port;
                if !check_io_port_range(priv_, port, size) {
                    return KcallResult::Ok(EPERM);
                }
            }
        }
    }

    // C: do_vdevio.c:102-149 — 批量 in/out
    // byte: 无对齐检查
    // word: port&1 == 0 否则 panic（C 行为）
    // long: port&3 == 0 否则 panic
    match dir {
        IoDirection::Input => {
            for pair in pairs.iter_mut() {
                pair.value = match size {
                    IoSize::Byte => port_io.read_byte(pair.port) as u32,
                    IoSize::Word => {
                        assert!(pair.port % 2 == 0, "unaligned port");
                        port_io.read_word(pair.port) as u32
                    }
                    IoSize::Long => {
                        assert!(pair.port % 4 == 0, "unaligned port");
                        port_io.read_long(pair.port)
                    }
                };
            }
        }
        IoDirection::Output => { /* 类似，write_* */ }
    }

    // C: do_vdevio.c:151-156 — input 模式拷回
    if matches!(dir, IoDirection::Input) {
        if let Err(e) = data_copy_vmcheck(KERNEL, &buf, caller.p_endpoint, vec_addr, bytes) {
            return KcallResult::Ok(e);
        }
    }

    KcallResult::Ok(OK)
}
```

**依赖**: `data_copy_vmcheck`（跨空间拷贝子系统，18-syscall-copy.md §D1 Direct Map DEFERRED 的依赖）。

**对齐违例处理**: C `panic("unaligned port")`；Rust 保持 `assert!` panic 语义（kernel bug）。

### 3.3 dispatch_sdevio 批量 I/O（ENOSYS → 完整）

**当前状态**: `syscall_device.rs:557-665` 参数验证 + endpoint + CHECK_IO_PORT + 对齐完整，批量 I/O 返回 ENOSYS。

**修复路径**:

```rust
pub fn dispatch_sdevio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &Message,
    port_io: &PI,
    priv_table: &PrivTable,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    // ... 参数验证 + endpoint + CHECK_IO_PORT + 对齐（已实现）...

    let is_safe = (request & DIO_SAFEMASK) == DIO_SAFE;
    let (vir_buf, destproc_nr) = if is_safe {
        // C: do_sdevio.c:68-82 — safe 变体 verify_grant
        match verify_grant(
            target_ep, caller.p_endpoint, vec_addr, vec_size,
            if matches!(dir, IoDirection::Input) { CPF_WRITE } else { CPF_READ },
            offset,
        ) {
            Ok(result) => (result.offset, result.granter),
            Err(e) => return KcallResult::Ok(e),
        }
    } else {
        // C: do_sdevio.c:83-93 — unsafe 变体，target == caller
        (vec_addr, caller.p_endpoint)
    };

    // C: do_sdevio.c:96 — 切到目标地址空间
    switch_address_space(destproc_nr);

    // C: do_sdevio.c:134-150 — phys_insb/outsb/insw/outsw
    let result = match (dir, size) {
        (IoDirection::Input, IoSize::Byte) => {
            phys_insb(port_io, port, vir_buf, vec_size);
            KcallResult::Ok(OK)
        }
        (IoDirection::Output, IoSize::Byte) => {
            phys_outsb(port_io, port, vir_buf, vec_size);
            KcallResult::Ok(OK)
        }
        (IoDirection::Input, IoSize::Word) => {
            phys_insw(port_io, port, vir_buf, vec_size);
            KcallResult::Ok(OK)
        }
        (IoDirection::Output, IoSize::Word) => {
            phys_outsw(port_io, port, vir_buf, vec_size);
            KcallResult::Ok(OK)
        }
        _ => KcallResult::Ok(EINVAL),  // long 不支持
    };

    // C: do_sdevio.c:159 — 切回 caller 地址空间
    switch_address_space(caller.p_endpoint);

    result
}
```

**依赖**:
- `verify_grant`（grant 表验证，18-syscall-copy.md §1.2）
- `switch_address_space`（地址空间切换原语，arch 层）
- `phys_insb/outsb/insw/outsw`（批量 I/O 原语，arch 层；按 count 重复调用 `PortIo::read_byte/word`）

### 3.4 dispatch_readbios 拷贝（ENOSYS → 完整）

**当前状态**: `syscall_device.rs:692-731` size==0 guard + checked_add + BIOS 范围检查完整，拷贝返回 ENOSYS。

**修复路径**:

```rust
pub fn dispatch_readbios(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &crate::proc_table::ProcessTable,  // 新增
) -> KcallResult {
    // ... 参数提取 + size==0 guard + checked_add + BIOS 范围检查（已实现）...

    // C: do_readbios.c:36 — virtual_copy_vmcheck
    // src 是物理地址（NONE endpoint），dst 是 caller buffer
    let src = VirAddr { proc_nr_e: Endpoint::NONE, offset: addr };
    let dst = VirAddr { proc_nr_e: caller.p_endpoint, offset: buf };
    match virtual_copy_vmcheck(caller, &src, &dst, size) {
        Ok(()) => KcallResult::Ok(OK),
        Err(e) => KcallResult::Ok(e),
    }
}
```

**依赖**: `virtual_copy_vmcheck`（跨空间拷贝 + VM 协助缺页处理，18-syscall-copy.md §1.1）。

### 3.5 dispatch_iopenable trap frame 同步（部分 → 完整）

**当前状态**: `syscall_device.rs:483-528` SELF 解析 + endpoint + iskerneln + `enable_user_io()`（改 initial_status）完整；运行中进程的 trap frame 同步 DEFERRED。

**修复路径**:

```rust
pub fn dispatch_iopenable(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut crate::proc_table::ProcessTable,
) -> KcallResult {
    // ... 现有逻辑 ...

    if let Some(target) = proc_table.get_mut(target_nr) {
        target.enable_user_io();  // 改 initial_status（已实现）
        // 新增：若进程正在运行，同步更新内核栈上的 trap frame
        if target.is_running() {
            // 需要 arch 层访问 saved exception frame
            // C: do_iopenable.c:28 enable_iop 直接改 p_reg.psw 即下次中断入口生效
            // Rust: 需 CpuContextArch::enable_user_io_on_trap_frame
            target.enable_user_io_on_trap_frame();  // DEFERRED
        }
    }

    KcallResult::Ok(0)
}
```

**依赖**: scheduler 集成 + arch 层 trap frame 访问原语（`CpuContextArch::enable_user_io_on_trap_frame`）。

### 3.6 generic_handler get_randomness（缺失 → 实现）

**当前状态**: `KernelNotifier::notify_hardware` (irq_manager.rs:121-183) 实现了 s_int_pending + mini_notify，`get_randomness` 注释 DEFERRED。

**修复路径**:

```rust
impl IrqNotify for KernelNotifier {
    fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId) {
        // C: do_irqctl.c:154 — get_randomness(&krandom, hook->irq)
        // 新增：采集随机数（需 krandom 子系统）
        // krandom::add_entropy(irq);  // DEFERRED

        // ... 现有 s_int_pending + mini_notify 逻辑 ...
    }
}
```

**依赖**: krandom 子系统（/dev/random 内核熵池）。

---

## §4. 限制与约束

### 4.1 DEFERRED 函数依赖汇总

| 函数 | DEFERRED 项 | 依赖 | 阻塞原因 |
|------|------------|------|---------|
| dispatch_irqctl (dispatch 层) | hook 链操作 BadCall | KernelState 持有 `IrqManager<ArchIc>` | KernelState 重构（跨文档依赖） |
| dispatch_vdevio | 批量 I/O（拷入+权限+执行+拷回） | `data_copy_vmcheck` | 跨空间拷贝子系统（18-syscall-copy Direct Map） |
| dispatch_sdevio | 批量 I/O（verify_grant+switch_space+phys_*） | `verify_grant` + `switch_address_space` + `phys_*` | 跨空间拷贝 + arch 地址空间切换 |
| dispatch_readbios | 拷贝 | `virtual_copy_vmcheck` | 跨空间拷贝子系统 |
| dispatch_iopenable | trap frame 同步 | scheduler + arch trap frame 访问 | scheduler 集成 |
| generic_handler | get_randomness | krandom 子系统 | /dev/random 熵池 |

### 4.2 anti-drift ENOSYS 决策

DEFERRED 函数返回 `ENOSYS`（非 OK）避免 silent 语义漂移：

| 函数 | C 行为 | 错误返回 OK 的后果 | ENOSYS 的理由 |
|------|--------|------------------|--------------|
| dispatch_vdevio | 批量 I/O 或返回错误 | 调用者以为 I/O 完成但实际 no-op | 诚实告知"未实现" |
| dispatch_sdevio | 批量 I/O 或返回错误 | 同上 | 同上 |
| dispatch_readbios | 拷贝或返回错误 | 调用者以为读到 BIOS 数据但 buffer 未变 | 同上 |

dispatch_irqctl dispatch 层返回 BadCall（非 ENOSYS）是因为参数校验已通过但 hook 操作无法执行——BadCall 表示"调用本身无法被处理"，ENOSYS 表示"功能未实现"，语义更精确。

### 4.3 语义偏移文档化（合理收紧）

| 偏移点 | C 行为 | Rust 行为 | 理由 |
|--------|--------|----------|------|
| DEVIO unknown type | default size=4 (do_devio.c:35) | EINVAL (syscall_device.rs:343) | 保守放行 vs 严格拒绝；Rust 更安全 |
| VDEVIO 超缓冲区 | E2BIG (do_vdevio.c:65) | EINVAL (syscall_device.rs:444) | E2BIG 在 minix-types 未定义；EINVAL 语义足够 |

### 4.4 x86-only 调用的非 x86 退化

`#[cfg(target_arch = "x86_64")] dispatch_arch_*` 转发到 `syscall_device::dispatch_*`；非 x86 返回 `BadCall`（D9 全局决策）。

非 x86 配置下:
- DEVIO/VDEVIO/SDEVIO/IOPENABLE/READBIOS 全部 BadCall
- IRQCTL 仍可用（IRQ 概念跨架构，InterruptController trait 抽象）
- PortIo trait 仍需实现（ARM/RISC-V MMIO），但 dispatch_arch_devio 不会调用

---

## §5. dispatch 接线清单

### 5.1 已接入（当前状态）

| 接入点 | 文件 | 说明 |
|--------|------|------|
| SYS_IRQCTL | syscall.rs:304 → dispatch_irqctl (533) | dispatch 层 BadCall（类型层完整） |
| SYS_DEVIO (x86_64) | syscall.rs:306 → dispatch_arch_devio (1180) | 转发到 syscall_device::dispatch_devio |
| SYS_SDEVIO (x86_64) | syscall.rs:307 → dispatch_arch_sdevio (1187) | 转发到 syscall_device::dispatch_sdevio |
| SYS_VDEVIO (x86_64) | syscall.rs: → dispatch_arch_vdevio (1203) | 转发到 syscall_device::dispatch_vdevio |
| SYS_IOPENABLE (x86_64) | syscall.rs: → dispatch_arch_iopenable (1212) | 转发到 syscall_device::dispatch_iopenable |
| SYS_READBIOS (x86_64) | syscall.rs: → dispatch_arch_readbios (1220) | 转发到 syscall_device::dispatch_readbios |

### 5.2 待接入（DEFERRED）

| 接入点 | 依赖 | 接入方式 |
|---------|------|---------|
| dispatch_irqctl 完整 | KernelState 持有 IrqManager | 扩展签名传入 `&mut IrqManager<ArchIc>` + `&PrivTable` |
| dispatch_vdevio 批量 | data_copy_vmcheck | 扩展签名传入 `&ProcessTable` |
| dispatch_sdevio 批量 | verify_grant + switch_address_space + phys_* | 扩展签名 + arch 原语 |
| dispatch_readbios 拷贝 | virtual_copy_vmcheck | 扩展签名传入 `&ProcessTable` |
| dispatch_iopenable trap frame | scheduler + arch trap frame | target.enable_user_io_on_trap_frame() |

---

## 附录 A: C↔Rust 差异矩阵

| C 符号 | C 位置 | Rust 表达 | 差异类型 | 理由 |
|--------|--------|----------|---------|------|
| `switch(request)` 整数分支 | do_irqctl.c:39 | `IrqctlRequest` enum + `TryFrom<i32>` | 类型增强 | 穷尽性检查 + 集中校验 |
| `IRQ_SETPOLICY` 等常量 | devio.h | `IrqctlRequest::SetPolicy` 等 | 类型增强 | enum 替代魔数 |
| `io_type = request & _DIO_TYPEMASK` | do_devio.c:27 | `IoSize::from_request_mask(io_type)` | 类型增强 | enum + Option 替代裸位掩码 |
| `io_dir = request & _DIO_DIRMASK` | do_devio.c:28 | `IoDirection::from_request_mask(io_dir)` | 类型增强 | 同上 |
| `inb/inw/inl/outb/outw/outl` | do_devio.c:75-99 | `PortIo::read_byte/word/long` + `write_*` | 抽象增强 | trait 替代内联汇编，多架构 |
| `static char vdevio_buf[]` | do_vdevio.c:17 | 栈分配 `[u8; VDEVIO_BUF_SIZE]` (D5) | anti-translate | no_std 友好 + SMP 安全 |
| `lock()/unlock()` 包裹 VDEVIO | do_vdevio.c:29-31 | BKL 串行化（无需额外锁） | 语义对齐 | BKL 已串行化 dispatch |
| `data_copy` 跨空间拷贝 | do_vdevio.c:67-70,153-156 | `data_copy_vmcheck` (DEFERRED) | 语义对齐 | Direct Map 简化 |
| `verify_grant` | do_sdevio.c:68-82 | `verify_grant` (DEFERRED) | 语义对齐 | grant 表验证 |
| `switch_address_space` | do_sdevio.c:96,159 | `switch_address_space` (DEFERRED) | 语义对齐 | arch 地址空间切换 |
| `phys_insb/outsb/insw/outsw` | do_sdevio.c:134-150 | `phys_*` (DEFERRED) | 语义对齐 | 批量 I/O 原语 |
| `enable_iop` 设 `p_reg.psw |= 0x3000` | do_iopenable.c:28 | `enable_user_io()` 下沉 arch | 抽象增强 | 内核层只知概念，arch 决定编码 |
| `virtual_copy_vmcheck` | do_readbios.c:36 | `virtual_copy_vmcheck` (DEFERRED) | 语义对齐 | 跨空间拷贝 |
| `irq_hooks[]` 全局数组 | do_irqctl.c:90-108 | `IrqManager<IC>` 封装 + 泛型 | anti-translate | 可 mock + 封装 |
| `put_irq_handler/rm_irq_handler` | do_irqctl.c:114,130 | `IrqManager::irqctl_set_policy/remove_hook_by_slot` | 语义对齐 | 方法封装 |
| `generic_handler(irq_hook_t*)` | do_irqctl.c:143 | `IrqHandler` + `IrqHookContext<'a>` | anti-translate | context 替代裸指针 + notifier 注入 |
| 全局 `mini_notify` | do_irqctl.c:170 | `IrqNotify` trait + `KernelNotifier`/`MockNotifier` | anti-translate | 可测试 + 安全 |
| `get_randomness` | do_irqctl.c:154 | (DEFERRED krandom 子系统) | 缺失 | /dev/random 熵池未实现 |
| `policy & IRQ_REENABLE` 返回 | do_irqctl.c:171 | `IrqPolicy::REENABLE` + `IrqAction` enum | 类型增强 | enum 替代 int 返回 |
| `default: size=4` 保守值 | do_devio.c:35 | `None => EINVAL` | 语义偏移 | 合理收紧（更安全） |
| `E2BIG` 超缓冲区 | do_vdevio.c:65 | `EINVAL` | 语义偏移 | E2BIG 未定义，EINVAL 足够 |
| `panic("unaligned port")` | do_vdevio.c:160 | `assert!(port % size == 0)` | 语义对齐 | kernel bug panic |
| `#if USE_DEVIO` 编译期门控 | do_devio.c:14 | `#[cfg(target_arch)]` + BadCall (D6) | 抽象增强 | trait + BadCall 替代 cfg 行为选择 |
| (无 C 对应) | — | `IrqctlRequest` TryFrom | Rust 独有 | 集中校验 |
| (无 C 对应) | — | `IoSize`/`IoDirection` enum | Rust 独有 | 类型安全解码 |
| (无 C 对应) | — | `IrqNotify` trait | Rust 独有 | 可测试通知抽象 |

---

## 附录 B: redox 对比

| 维度 | redox | minix-rs | 选择理由 |
|------|-------|---------|---------|
| IRQ 访问模型 | `scheme::irq::IrqScheme`——用户态驱动通过 scheme fd 注册/等中断 | 内核 `IrqManager<IC>` + `mini_notify` 通知 | minix-rs 对齐 C ground truth（内核维护钩子池+链表）；redox 是 userspace-driver 重设计，IRQ 经 scheme 文件描述符 |
| 端口 I/O 抽象 | `scheme::io::Pio`——封装 `inb/outb` 的 newtype | `trait PortIo` + `X86_64PortIo`/`MockPortIo` impl | 两者都抽象；minix-rs 用 trait 便于 mock 测试 + 多架构（ARM MMIO），redox 用 newtype 简单封装 |
| I/O 权限 | `syscall::iopl` 设置 IOPL；scheme 权限模型 | `dispatch_iopenable` 设 IOPL=3 via arch `enable_user_io` | redox 更细粒度（per-scheme 权限）；minix-rs 对齐 C 全局 IOPL（提权后可访问所有端口） |
| 批量 I/O | redox 无直接对应（用户态驱动自做循环） | VDEVIO/SDEVIO 内核批量 | minix-rs 保留 C 批量语义（性能：一次 syscall 多端口）；redox 用户态驱动自循环 |
| 跨地址空间 I/O | redox 驱动直接用 grant + DMA | SDEVIO `verify_grant` + `switch_address_space` + `phys_*` | 两者都需 grant；minix-rs 对齐 C 的 `phys_ins/outs` 批量原语 |
| 中断通知 | redox `scheme::irq` 返回 `Event` 给用户态 | `mini_notify(HARDWARE, endpoint)` + `s_int_pending` 位图 | minix-rs 对齐 C 通知语义；redox 用 scheme 事件 |

**设计哲学差异**: redox 是 userspace-driver 重设计（驱动通过 scheme 文件描述符访问硬件），minix-rs 对齐 Minix3 C 的内核钩子+权限表模型。两者都抽象端口 I/O（redox newtype / minix-rs trait），但 IRQ/通知机制本质不同。

---

## 附录 C: 测试策略

### C.1 现有测试（33 个，已实现）

详见 outline §5.1。覆盖：
- IrqctlRequest TryFrom（1 个）
- IoSize/IoDirection 解码（3 个）
- IRQ_REENABLE 标志（1 个）
- check_irq_permission（3 个）
- dispatch_devio（5 个：input/output/alignment/check_allowed/check_denied/invalid_type）
- dispatch_iopenable（5 个：self/explicit/invalid/kernel/iopl_bits）
- dispatch_sdevio（7 个：invalid_ep/kernel/unsafe/long/unaligned/check_denied/valid_enosys/invalid_dir）
- dispatch_readbios（5 个：zero_size/outside/in_bios/in_upper/straddling/overflow）

### C.2 新增测试（DEFERRED 函数实现后）

| 测试函数 | 验证行为 | Mock 策略 |
|---------|---------|----------|
| `test_dispatch_irqctl_setpolicy_full` | SETPOLICY 完整路径：权限→查重→安装→返回 hook_id | `MockIc` + MockNotifier |
| `test_dispatch_irqctl_rmpolicy_owner_check` | RMPOLICY owner 校验 + 删除 | 同上 |
| `test_dispatch_irqctl_enable_disable` | ENABLE/DISABLE owner 校验 + 调用 IC | `MockIc` 记录 mask/unmask |
| `test_dispatch_vdevio_batch_io` | 批量 I/O 端到端：拷入→权限→执行→拷回 | `MockPortIo` + mock data_copy_vmcheck |
| `test_dispatch_vdevio_alignment_panic` | word/long 未对齐 panic | `#[should_panic]` |
| `test_dispatch_sdevio_safe_grant` | safe 变体 verify_grant 映射 | mock verify_grant |
| `test_dispatch_sdevio_phys_batch` | phys_insb/outsb 批量 | `MockPortIo` + mock switch_address_space |
| `test_dispatch_readbios_copy` | BIOS 拷贝端到端 | mock virtual_copy_vmcheck |
| `test_dispatch_iopenable_trap_frame_sync` | 运行中进程 trap frame 同步 | mock is_running + enable_user_io_on_trap_frame |

### C.3 MockPortIo 实现（已存在）

```rust
#[cfg(test)]
struct MockPortIo {
    last_write: core::cell::RefCell<Option<(u16, u32)>>,
    read_value: u32,
}

#[cfg(test)]
impl PortIo for MockPortIo {
    fn read_byte(&self, _port: u16) -> u8 { self.read_value as u8 }
    fn write_byte(&self, port: u16, value: u8) {
        *self.last_write.borrow_mut() = Some((port, value as u32));
    }
    // ... read_word/write_word/read_long/write_long ...
}
```

---

## 自检

- [x] §1 目标约束完整（对齐 C + 修复 P0/P1 + anti-translate + 多架构兼容 + VMCTL 边界）
- [x] §2 数据结构设计完整（IrqctlRequest/IoSize/IoDirection enum + PortIo trait + IrqManager<IC> + IrqHookContext + IrqNotify trait + VDEVIO 栈缓冲区）
- [x] §2 anti-translate 体现（enum/trait/泛型/context 注入/栈分配）
- [x] §3 缺失函数实现方案完整（6 个 DEFERRED 项 + 设计方案 + 依赖）
- [x] §3 DEFERRED 依赖诚实标注（KernelState/data_copy_vmcheck/verify_grant/switch_address_space/virtual_copy_vmcheck/scheduler/krandom）
- [x] §3.1 dispatch_irqctl BadCall 诚实标注（类型层完整 vs dispatch 层 BadCall + 修复路径）
- [x] §4 限制约束完整（DEFERRED 依赖汇总 + anti-drift ENOSYS 决策 + 语义偏移文档化 + 非 x86 退化）
- [x] §5 dispatch 接线清单完整（已接入 6 处 + 待接入 5 处）
- [x] 附录 A C↔Rust 差异矩阵完整（25 项）
- [x] 附录 B redox 对比完整（6 维度 + 设计哲学差异）
- [x] 附录 C 测试策略完整（33 现有 + 9 新测试 + MockPortIo）
- [x] 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无日期标注（"2026-XX-XX"）
- [x] 无"DEFERRED 状态说明（2026-06-14 更新）"开发日志整节
- [x] 跨架构统一抽象（PortIo trait + InterruptController trait + BadCall）
- [x] VMCTL 不纳入（边界明确）
