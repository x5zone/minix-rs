### 2.9 资源访问字段

资源访问字段定义了进程可以访问的硬件资源和内核资源，包括 I/O 端口、内存范围、IRQ 线、grant 表和 state 表。这些字段实现了细粒度的资源访问控制，是 Minix3 安全模型的重要组成部分。

#### 2.9.1 I/O 端口范围

**字段定义**（`priv.h` 第 53-54 行）：

```c
int s_nr_io_range;		/* allowed I/O ports */
struct io_range s_io_tab[NR_IO_RANGE];
```

**作用说明**：

`s_nr_io_range` 字段存储该进程被允许的 **I/O 端口范围数量**，`s_io_tab` 数组存储具体的 I/O 端口范围定义。I/O 端口是 x86 架构中用于与硬件设备通信的机制（如通过 `in`/`out` 指令）。

**io_range 结构体**（通常定义在 `<minix/io_range.h>`）：

```c
struct io_range {
    unsigned short base;      /* I/O 端口基地址 */
    unsigned short len;       /* 端口范围长度 */
};
```

**使用场景**：

```c
// 初始化串口驱动程序的 I/O 权限
struct proc *com_driver = ...;
com_driver->p_priv->s_nr_io_range = 2;

// COM1: 0x3F8-0x3FF (8 bytes)
com_driver->p_priv->s_io_tab[0].base = 0x3F8;
com_driver->p_priv->s_io_tab[0].len = 8;

// COM2: 0x2F8-0x2FF (8 bytes)
com_driver->p_priv->s_io_tab[1].base = 0x2F8;
com_driver->p_priv->s_io_tab[1].len = 8;
```

#### 2.9.2 内存范围

**字段定义**（`priv.h` 第 56-57 行）：

```c
int s_nr_mem_range;		/* allowed memory ranges */
struct minix_mem_range s_mem_tab[NR_MEM_RANGE];
```

**作用说明**：

`s_nr_mem_range` 和 `s_mem_tab` 字段定义了进程可以访问的**物理内存范围**。这对于需要直接访问物理内存的设备驱动程序（如帧缓冲区、DMA 缓冲区等）非常重要。

#### 2.9.3 IRQ 线

**字段定义**（`priv.h` 第 59-60 行）：

```c
int s_nr_irq;			/* allowed IRQ lines */
int s_irq_tab[NR_IRQ];
```

**作用说明**：

`s_nr_irq` 和 `s_irq_tab` 字段定义了进程可以处理的**中断请求线**（IRQ lines）。设备驱动程序需要注册 IRQ 来处理硬件中断（如键盘输入、网络数据到达、磁盘 I/O 完成等）。

#### 2.9.4 grant 表

**字段定义**（`priv.h` 第 61-63 行）：

```c
vir_bytes s_grant_table;	/* grant table address of process, or 0 */
int s_grant_entries;		/* no. of entries, or 0 */
endpoint_t s_grant_endpoint;  /* the endpoint the grant table belongs to */
```

**作用说明**：

Grant 表是一种**安全共享内存机制**，允许进程授权其他进程访问自己的内存区域，而无需将物理内存地址暴露给对方。这是 Minix3 实现安全 IPC 和 DMA 的关键机制。

#### 2.9.5 state 表

**字段定义**（`priv.h` 第 64-66 行）：

```c
vir_bytes s_state_table;	/* state table address of process, or 0 */
int s_state_entries;		/* no. of entries, or 0 */
```

**作用说明**：

State 表用于**进程状态保存和恢复**，主要用于实现进程检查点（checkpointing）、实时迁移（live migration）或系统重启后状态恢复等功能。

---

## 3. Rust 设计决策

本节讨论如何用 Rust 实现特权结构体，包括类型安全设计、权限掩码实现和资源限制管理。

### 3.1 特权结构体设计

在 Rust 中实现特权结构体时，我们需要考虑以下几个方面：

1. **类型安全**：使用强类型系统区分不同的 ID 类型（进程号、特权 ID、端点）
2. **内存安全**：使用 Rust 的所有权系统管理特权结构体的生命周期
3. **并发安全**：使用原子类型和锁保护共享的特权表

### 3.2 权限掩码

权限掩码可以使用 Rust 的位标志（bitflags）或常量来实现：

```rust
bitflags! {
    struct PrivFlags: u16 {
        const SYS_PROC = 0x001;
        const BILLABLE = 0x002;
        const PREEMPTIBLE = 0x004;
        const KERNEL = 0x008;
        // ... 其他标志
    }
}
```

### 3.3 资源限制

资源限制（I/O 端口、内存范围、IRQ）可以使用固定大小的数组或动态分配的向量来实现，取决于性能和安全要求。

---

## 4. 实现

本节给出 Rust 实现代码，包括 Priv 结构体定义、特权标志定义和单元测试。

### 4.1 Priv 结构体定义

```rust
use core::sync::atomic::{AtomicU32, Ordering};

/// 特权结构体
#[repr(C)]
pub struct Priv {
    /// 关联进程号
    pub s_proc_nr: ProcNr,
    /// 特权表索引
    pub s_id: SysId,
    /// 特权标志
    pub s_flags: PrivFlags,
    /// 初始化标志
    pub s_init_flags: InitFlags,
    
    // 异步发送字段
    pub s_asyntab: VirBytes,
    pub s_asynsize: usize,
    pub s_asynendpoint: Endpoint,
    
    // 系统调用控制
    pub s_trap_mask: u16,
    pub s_ipc_to: SysMap,
    pub s_k_call_mask: [BitChunk; SYS_CALL_MASK_SIZE],
    
    // 信号管理
    pub s_sig_mgr: Endpoint,
    pub s_bak_sig_mgr: Endpoint,
    
    // 待处理事件
    pub s_notify_pending: AtomicSysMap,
    pub s_asyn_pending: AtomicSysMap,
    pub s_int_pending: AtomicU32,
    pub s_sig_pending: SigSet,
    
    // 其他字段
    pub s_ipcf: *mut IpcFilter,
    pub s_alarm_timer: Timer,
    pub s_stack_guard: *mut RegT,
    pub s_diag_sig: i8,
    
    // 资源访问
    pub s_nr_io_range: i32,
    pub s_io_tab: [IoRange; NR_IO_RANGE],
    pub s_nr_mem_range: i32,
    pub s_mem_tab: [MemRange; NR_MEM_RANGE],
    pub s_nr_irq: i32,
    pub s_irq_tab: [i32; NR_IRQ],
    pub s_grant_table: VirBytes,
    pub s_grant_entries: i32,
    pub s_grant_endpoint: Endpoint,
    pub s_state_table: VirBytes,
    pub s_state_entries: i32,
}
```

### 4.2 特权标志定义

```rust
use bitflags::bitflags;

bitflags! {
    /// 特权标志
    #[repr(transparent)]
    pub struct PrivFlags: u16 {
        /// 系统进程
        const SYS_PROC = 0x001;
        /// 可计费
        const BILLABLE = 0x002;
        /// 可抢占
        const PREEMPTIBLE = 0x004;
        /// 内核任务
        const KERNEL = 0x008;
        /// VM 标志
        const VM_F = 0x010;
        /// 允许 sendrec
        const SENDREC_F = 0x020;
        /// 驱动程序
        const DRIVER_F = 0x040;
        /// 服务器进程
        const SERVER_F = 0x080;
    }
}

bitflags! {
    /// 初始化标志
    #[repr(transparent)]
    pub struct InitFlags: u32 {
        const INTERCEPT_F = 0x001;
        const ORPHAN_F = 0x002;
        const SIGINT_F = 0x004;
        const SIGQUIT_F = 0x008;
        const SIGILL_F = 0x010;
        const SIGTRAP_F = 0x020;
        const SIGABRT_F = 0x040;
        const SIGBUS_F = 0x080;
        const SIGFPE_F = 0x100;
        const SIGUSR1_F = 0x200;
        const SIGSEGV_F = 0x400;
        const SIGUSR2_F = 0x800;
    }
}
```

### 4.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priv_flags() {
        let flags = PrivFlags::SYS_PROC | PrivFlags::BILLABLE;
        assert!(flags.contains(PrivFlags::SYS_PROC));
        assert!(flags.contains(PrivFlags::BILLABLE));
        assert!(!flags.contains(PrivFlags::KERNEL));
    }

    #[test]
    fn test_init_flags() {
        let flags = InitFlags::SIGINT_F | InitFlags::SIGQUIT_F;
        assert!(flags.contains(InitFlags::SIGINT_F));
        assert!(flags.contains(InitFlags::SIGQUIT_F));
    }

    #[test]
    fn test_priv_creation() {
        let priv = Priv {
            s_proc_nr: ProcNr(1),
            s_id: SysId(0),
            s_flags: PrivFlags::SYS_PROC,
            ..Default::default()
        };
        
        assert_eq!(priv.s_proc_nr.0, 1);
        assert!(priv.s_flags.contains(PrivFlags::SYS_PROC));
    }
}
```

---

## 5. 参见

- [08-proc-macros](08-proc-macros.md) - 进程访问宏
- [10-priv-macros](10-priv-macros.md) - 特权访问宏
- [18-do-fork-priv](18-do-fork-priv.md) - fork 特权处理
- [22-const](22-const.md) - 常量定义

---

**文档版本**：1.0  
**最后更新**：2024年  
**作者**：Minix-Rust 项目团队  
**许可证**：MIT
