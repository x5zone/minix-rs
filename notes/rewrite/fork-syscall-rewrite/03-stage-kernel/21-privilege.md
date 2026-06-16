# 21-privilege: 权限管理

> **分类**: 运行时基础设施
> **源码**: `minix3/minix/kernel/priv.h`, `minix3/minix/kernel/system.c:274-540`
> **前置**: 10（调度——进程状态）, 16（进程管理——fork/privctl）
> **C 总行数**: ~400 行

---

## Ch1: 概念

**核心问题**: 内核如何控制进程的权限——谁能发 IPC 给谁、谁能调用哪些系统调用、谁能访问哪些 I/O 端口？

Minix3 的权限模型基于 `struct priv`：

- **系统进程**（VM/PM/VFS/RS 等）：每个有独立的 `priv` 结构，定义了完整的权限集
- **用户进程**：共享 `USER_PRIV`，权限最小化

### 1.1 struct priv 关键字段

| 字段 | 语义 | 类型 |
|------|------|------|
| `s_flags` | 权限标志 | `PREEMPTIBLE / BILLABLE / SYS_PROC / DYN_PRIV_ID / CHECK_IO_PORT / CHECK_IRQ / CHECK_MEM / ROOT_SYS_PROC / VM_SYS_PROC / LU_SYS_PROC / RST_SYS_PROC` |
| `s_trap_mask` | 允许的 IPC 原语掩码 | 位图 |
| `s_ipc_to` | 允许发送的目标位图 | `sys_map_t` (64-bit) |
| `s_k_call_mask` | 允许的内核调用位图 | `sys_map_t[NR_SYS_CALLS/64]` |
| `s_notify_pending` | 挂起通知位图 | `sys_map_t` |
| `s_asyn_pending` | 挂起异步消息位图 | `sys_map_t` |
| `s_int_pending` | 挂起中断位图 | `u32` |
| `s_sig_mgr` | 信号管理器 endpoint | `endpoint_t` |
| `s_bak_sig_mgr` | 备份信号管理器 | `endpoint_t` |
| `s_alarm_timer` | 同步闹钟定时器 | `minix_timer_t` |
| `s_grant_table` | Grant 表虚拟地址 | `vir_bytes` |
| `s_grant_entries` | Grant 表项数 | `int` |
| `s_grant_endpoint` | Grant 表所属 endpoint | `endpoint_t` |
| `s_io_tab` | I/O 端口范围表 | `io_range[]` |
| `s_nr_io_range` | I/O 端口范围数 | `int` |
| `s_irq_tab` | IRQ 向量表 | `int[]` |
| `s_nr_irq` | IRQ 数 | `int` |

### 1.2 s_flags 权限标志

> C 源码: `minix/include/minix/const.h:143-154`

| 标志 | 值 | 含义 |
|------|-----|------|
| `PREEMPTIBLE` | 0x002 | 进程可被抢占 |
| `BILLABLE` | 0x004 | 进程的时间可被记账 |
| `DYN_PRIV_ID` | 0x008 | 动态分配的特权 ID |
| `SYS_PROC` | 0x010 | 系统进程（有独立 priv 结构） |
| `CHECK_IO_PORT` | 0x020 | 启用 I/O 端口访问检查 |
| `CHECK_IRQ` | 0x040 | 启用 IRQ 访问检查 |
| `CHECK_MEM` | 0x080 | 启用内存映射权限检查 |
| `ROOT_SYS_PROC` | 0x100 | 根系统进程 |
| `VM_SYS_PROC` | 0x200 | VM 系统进程 |
| `LU_SYS_PROC` | 0x400 | Live Update 系统进程 |
| `RST_SYS_PROC` | 0x800 | 重启系统进程 |

> **注意**: Minix3 C 源码中**不存在** `CHECK_IPC` 标志。IPC 目标过滤通过 `s_ipc_to` 位图**无条件**执行——`may_send_to()` 宏（priv.h:86）始终检查 `s_ipc_to`，无需标志位启用。这与 `CHECK_IO_PORT`/`CHECK_IRQ`/`CHECK_MEM` 的模式不同——后者是可选检查，需要标志位启用。

### 1.3 权限操作

| 操作 | 函数 | 说明 |
|------|------|------|
| 获取 priv | `get_priv(rp, flags)` | 分配或返回 USER_PRIV |
| 设置发送目标 | `set_sendto_bit(rp, bit)` | 设置 s_ipc_to 的某一位 |
| 填充发送掩码 | `fill_sendto_mask(rp, mask)` | 批量设置 s_ipc_to |
| IPC 权限检查 | `ipc_filter_check` | 检查 s_ipc_to 和 s_k_call_mask |

### 1.4 priv 表结构

```
priv[NR_SYS_PROCS]  — 系统进程的 priv 结构数组
USER_PRIV           — 用户进程共享的单一 priv 结构
```

每个系统进程的 priv 通过 `p_priv → &priv[id]` 引用。用户进程的 `p_priv` 指向 `USER_PRIV`。

---

## Ch2: C 源码分析

### priv.h (~200 行)

| 行号 | 内容 | 说明 |
|------|------|------|
| 1-50 | `struct priv` | 完整的 priv 结构定义 |
| 51-80 | 权限标志宏 | `PREEMPTIBLE` 等 |
| 81-120 | `get_priv()` | 分配 priv 结构 |
| 121-150 | `set_sendto_bit()` | 设置 IPC 目标位 |
| 151-180 | `fill_sendto_mask()` | 批量设置 IPC 目标 |

### system.c:274-540 (~270 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 274-340 | `do_privctl()` | SYS_PRIVCTL: 设置权限、grant 表、信号管理器 |
| 341-400 | priv 初始化 | boot 阶段分配 priv |
| 401-540 | 权限检查辅助 | `isokendpt()`, `okendpt()` 等 |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | s_flags | 裸整数 vs bitflags | **`PrivFlags` bitflags** | Rust 惯用法 |
| D2 | s_ipc_to | `sys_map_t` vs `u64` | **`u64`** | 64 个 endpoint 足够 |
| D3 | s_k_call_mask | `sys_map_t[]` vs `[u64; N]` | **`[u64; 1]`** | 58 个 syscall，1 个 u64 足够 |
| D4 | priv 表 | 全局数组 vs `PrivTable` struct | **`PrivTable` struct** | 已在 kpriv.rs 中定义 |
| D5 | USER_PRIV | 全局 vs PrivTable 字段 | **PrivTable 字段** | 封装 |
| D6 | get_priv | 返回指针 vs 返回索引 | **返回 `PrivId`** | 避免裸指针 |
| D7 | s_alarm_timer | `minix_timer_t` vs `Option<TimerEntry>` | **`Option<TimerEntry>`** | 与 14-clock-timer 一致 |

---

## Ch4: 实现要点

### 4.1 PrivFlags bitflags

> Rust 实现已与 C 源码对齐: `os/kernel/src/kpriv.rs:48-59`

```rust
bitflags::bitflags! {
    /// C: minix/include/minix/const.h:143-154
    pub struct PrivFlagsBits: u16 {
        const PREEMPTIBLE     = 0x002;  // const.h:143
        const BILLABLE        = 0x004;  // const.h:144
        const DYN_PRIV_ID     = 0x008;  // const.h:145
        const SYS_PROC        = 0x010;  // const.h:147
        const CHECK_IO_PORT   = 0x020;  // const.h:148
        const CHECK_IRQ       = 0x040;  // const.h:149
        const CHECK_MEM       = 0x080;  // const.h:150
        const ROOT_SYS_PROC   = 0x100;  // const.h:151
        const VM_SYS_PROC     = 0x200;  // const.h:152
        const LU_SYS_PROC     = 0x400;  // const.h:153
        const RST_SYS_PROC    = 0x800;  // const.h:154
    }
}
```

> **注意**: C 源码中不存在 `CHECK_IPC` 标志。IPC 目标过滤通过 `s_ipc_to` 位图无条件执行（`may_send_to()` 宏），无需标志位启用。详见 §1.2 注释。

### 4.2 KPriv 结构体更新

在现有 `kpriv.rs` 的 `KPriv` 基础上增加：

- `s_ipc_to: u64`
- `s_k_call_mask: u64`
- `s_notify_pending: u64`
- `s_asyn_pending: u64`
- `s_int_pending: u32`
- `s_sig_mgr: Endpoint`
- `s_bak_sig_mgr: Endpoint`
- `s_alarm_timer: Option<TimerEntry>`（2026-06-13 修复 P1-13：原 `u64` 已改为 `Option<TimerEntry>`，与 Ch3 D7 承诺一致；默认 `None` 对应 C `system.c:180` `tmr_inittimer`；测试 `test_kpriv_alarm_timer_default_none` / `test_kpriv_alarm_timer_some_carries_action` 覆盖）
- `s_grant_table: u64`
- `s_grant_entries: u32`
- `s_grant_endpoint: Endpoint`
- `s_io_tab: [IoRange; MAX_IO_RANGES]`
- `s_irq_tab: [i32; MAX_IRQS]`

### 4.3 IoRange

```rust
pub struct IoRange {
    pub base: u16,
    pub limit: u16,
}
```

---

## 测试

- 单元：PrivFlags 位操作
- 单元：set_sendto_bit / fill_sendto_mask
- 单元：get_priv 分配和 USER_PRIV 共享
- 单元：s_k_call_mask 权限检查

---

## 补充：特权结构详细分析

> 来源：tmp-11-privilege.md, tmp_09-priv-struct.md

### 特权表布局

```
priv[NR_SYS_PROCS] 的布局：

索引: 0          ... NR_BOOT_PROCS-1   ... NR_SYS_PROCS-1
     ┌─────────────────────────────┬───────────────────────┐
     │     静态特权区               │    动态特权区          │
     │  BEG_STATIC_PRIV_ADDR       │  BEG_DYN_PRIV_ADDR    │
     │  预定义系统进程特权           │  运行时分配            │
     │  (boot image 进程)          │  (动态创建的系统进程)    │
     └─────────────────────────────┴───────────────────────┘

USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR) = NR_TASKS + RS_PROC_NR = 6
```

| 常量 | 值 | 含义 |
|------|-----|------|
| `NR_STATIC_PRIV_IDS` | `NR_BOOT_PROCS` (= 17) | 静态特权数量 |
| `USER_PRIV_ID` | `NR_TASKS + RS_PROC_NR` (= 6) | 用户进程共享的特权 ID |
| `NR_SYS_PROCS` | 64 | 特权表总大小 |

### 预定义特权组合

`minix/include/minix/priv.h:36-49` 定义了进程类型的默认特权组合：

| 组合 | 值 | 适用 |
|------|-----|------|
| `IDL_F` | `SYS_PROC \| BILLABLE` | IDLE 任务（不可抢占） |
| `TSK_F` | `SYS_PROC` | 其他内核任务 |
| `SRV_F` | `SYS_PROC \| PREEMPTIBLE` | 系统服务（PM、VFS 等） |
| `DSRV_F` | `SRV_F \| DYN_PRIV_ID` | 动态系统服务 |
| `RSYS_F` | `SRV_F \| ROOT_SYS_PROC` | 根系统进程（RS） |
| `VM_F` | `SYS_PROC \| VM_SYS_PROC` | VM 进程 |
| `USR_F` | `BILLABLE \| PREEMPTIBLE` | 用户进程 |

### s_flags 完整标志位

| 标志 | 值 | 含义 |
|------|-----|------|
| `PREEMPTIBLE` | 0x002 | 进程可被抢占 |
| `BILLABLE` | 0x004 | 进程可被计费 |
| `DYN_PRIV_ID` | 0x008 | 动态分配的特权 ID |
| `SYS_PROC` | 0x010 | 系统进程 |
| `CHECK_IO_PORT` | 0x020 | 检查 I/O 端口权限 |
| `CHECK_IRQ` | 0x040 | 检查 IRQ 权限 |
| `CHECK_MEM` | 0x080 | 检查内存映射权限 |
| `ROOT_SYS_PROC` | 0x100 | 根系统进程 |
| `VM_SYS_PROC` | 0x200 | VM 系统进程 |
| `LU_SYS_PROC` | 0x400 | 热更新系统进程 |
| `RST_SYS_PROC` | 0x800 | 重启系统进程 |

### fork 中的特权降级

```c
// do_fork.c:104-107
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);   // 降级为用户特权
    rpc->p_rts_flags |= RTS_NO_PRIV;          // 禁止运行
}
```

- 用户进程 fork → 子进程继承 `USER_PRIV_ID`，无特权变化
- 系统进程 fork → 子进程降级为用户进程，设置 `RTS_NO_PRIV`，必须经 RS 授权才能运行

### 特权访问宏

```c
#define BEG_PRIV_ADDR              (&priv[0])
#define END_PRIV_ADDR              (&priv[NR_SYS_PROCS])
#define BEG_DYN_PRIV_ADDR          END_STATIC_PRIV_ADDR
#define priv_addr(i)      (ppriv_addr)[(i)]     // 通过 ID 获取特权指针
#define priv_id(rp)       ((rp)->p_priv->s_id)  // 获取进程的特权 ID
#define priv(rp)          ((rp)->p_priv)         // 获取进程的特权指针
#define id_to_nr(id)      priv_addr(id)->s_proc_nr
#define nr_to_id(nr)      priv(proc_addr(nr))->s_id
#define may_send_to(rp, nr) (get_sys_set(priv(rp)->s_ipc_to, nr_to_id(nr)))
```

### 特权操作行为规则

1. **系统进程独立特权**：每个系统进程有独立的 `struct priv`，互不影响
2. **用户进程共享特权**：所有用户进程共享 `USER_PRIV_ID` 指向的同一个 `struct priv`
3. **IPC 权限双向检查**：发送方检查 `s_ipc_to`（允许发给谁），接收方检查 IPC 过滤器
4. **内核调用掩码精确控制**：`s_k_call_mask` 位图中每一位对应一个内核调用号
5. **I/O 端口 / IRQ / 内存范围**：系统进程通过 `priv_add_io/irq/mem` 逐条添加权限，有上限
6. **特权 ID 回收**：动态服务进程退出时释放特权 ID，重启时重新分配

---

## 参见

- [10-scheduling-primitives.md](10-scheduling-primitives.md) — PREEMPTIBLE/BILLABLE 与调度
- [16-syscall-process.md](16-syscall-process.md) — fork 分配 priv
- [17-syscall-copy.md](17-syscall-copy.md) — grant 表用于 safecopy
- [19-syscall-device.md](19-syscall-device.md) — CHECK_IO_PORT/CHECK_IRQ
