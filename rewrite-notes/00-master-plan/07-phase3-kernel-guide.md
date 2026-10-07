# 阶段3：Kernel 层实现指南

> **状态**: ❌ 待实现  
> **对应源码**: `minix/kernel/system/do_fork.c`, `minix/include/minix/com.h`

---

## 1. 目标与范围

实现 Kernel 层的 `sys_fork()` 系统调用，完成 PCB 克隆、上下文伪造（ret_reg=0）、Endpoint 生成、RTS 标志管理等核心逻辑。

**硬件相关**: 寄存器读写、FPU 上下文保存/恢复（使用 Mock）  
**软件逻辑**: PCB 克隆、Endpoint 生成算法、RTS 标志管理（真实实现）

---

## 2. 任务清单

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 3.1 | 定义 `RtsFlags` 位标志 (完整 16 个标志) | 无 | `os/kernel/src/proc.rs` |
| 3.2 | 定义 `MiscFlags` 位标志 | 无 | `os/kernel/src/proc.rs` |
| 3.3 | 定义 `KProcess` 结构体 (对齐 struct proc 关键字段) | 3.1, 3.2 | `os/kernel/src/proc.rs` |
| 3.4 | 定义 `ProcTable` 结构体 (procs + generations) | 3.3 | `os/kernel/src/proc.rs` |
| 3.5 | 实现 `make_endpoint(generation, slot)` | 无 | `os/kernel/src/endpoint.rs` |
| 3.6 | 实现 `endpoint_generation(ep)` | 3.5 | `os/kernel/src/endpoint.rs` |
| 3.7 | 实现 `endpoint_slot(ep)` | 3.5 | `os/kernel/src/endpoint.rs` |
| 3.8 | 实现 `is_valid_endpoint()` — generation 验证 | 3.5, 3.4 | `os/kernel/src/endpoint.rs` |
| 3.9 | 实现 `KProcess::sys_fork()` — PCB 克隆 + 上下文伪造 | 3.3-3.8 | `os/kernel/src/system/do_fork.rs` |
| 3.10 | 实现 `ret_reg = 0` — 子进程 fork 返回 0 | 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.11 | 实现 generation 递增 + 回绕 (到 1 不是 0) | 3.5, 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.12 | 实现 RTS 标志管理 (NO_QUANTUM/VMINHIBIT/NO_PRIV) | 3.1, 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.13 | 实现 FPU 保存区修复逻辑 | 3.3, 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.14 | 实现特权进程降级 | 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.15 | 实现进程名 "*F" 追加 | 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.16 | 更新 `os/kernel/src/lib.rs` 模块导出 | 3.9 | `os/kernel/src/lib.rs` |
| 3.17 | 编写 Kernel 层单元测试 | 3.9 | `os/kernel/src/system/do_fork.rs` |

---

## 3. Proc 结构体设计

### 3.1 RTS 标志位定义

```rust
// os/kernel/src/proc.rs
bitflags! {
    pub struct RtsFlags: u32 {
        const VM_RUNNING = 0x001;      // 在VM中运行
        const VM_EXIT = 0x002;         // VM退出
        const SIG_PENDING = 0x004;     // 信号待处理
        const SIGNALED = 0x008;        // 已被信号中断
        const NO_PRIV = 0x010;         // 特权被撤销
        const NO_QUANTUM = 0x020;      // 无时间片
        const VMINHIBIT = 0x040;       // VM抑制
        const P_STOP = 0x080;          // 进程停止
        const RECEIVING = 0x100;       // 正在接收
        const SENDING = 0x200;         // 正在发送
        const SIGNALS_OFF = 0x400;     // 信号关闭
        const SYS_PROC = 0x800;        // 系统进程
        const DYING = 0x1000;          // 正在退出
        const OFF_Q = 0x2000;          // 不在队列中
        const LOCKED = 0x4000;         // 已锁定
        const UNREADY = 0x8000;        // 未就绪
    }
}
```

### 3.2 KProcess 结构体

```rust
pub struct KProcess {
    pub slot: usize,                    // 进程槽位索引
    pub endpoint: Endpoint,             // 内核端点
    pub rts_flags: RtsFlags,            // RTS 运行状态标志
    pub misc_flags: MiscFlags,          // 杂项标志
    pub priority: i32,                  // 优先级
    pub quantum_size_ms: i32,           // 时间片大小(毫秒)
    pub user_time: u64,                 // 用户态时间
    pub sys_time: u64,                  // 系统态时间
    pub virt_left: u64,                 // 虚拟定时器剩余
    pub prof_left: u64,                 // 性能分析定时器剩余
    pub name: [u8; 16],                 // 进程名
    pub pending_signals: u64,           // 待处理信号集
    pub is_system_proc: bool,           // 是否系统进程
    pub ret_reg: u64,                   // 返回寄存器 (rax/eax)
    pub cr3: u64,                       // 页表基址 (x86 CR3)
    pub fpu_state: Vec<u8>,             // FPU 保存区
    // ... 其他寄存器字段
}
```

---

## 4. Endpoint 生成算法

```rust
// os/kernel/src/endpoint.rs

pub const ENDPOINT_GENERATION_SHIFT: u32 = 15;
pub const ENDPOINT_MAX_GENERATION: i32 = 65535;

/// 从 generation 和 slot 构造 endpoint
pub fn make_endpoint(generation: i32, slot: usize) -> Endpoint {
    Endpoint::new((generation << ENDPOINT_GENERATION_SHIFT as i32) | (slot as i32))
}

/// 从 endpoint 提取 generation
pub fn endpoint_generation(ep: Endpoint) -> i32 {
    (ep.get() >> ENDPOINT_GENERATION_SHIFT as i32) & 0x7FFF
}

/// 从 endpoint 提取 slot
pub fn endpoint_slot(ep: Endpoint) -> usize {
    (ep.get() & 0x7FFF) as usize
}

/// 验证 endpoint 的 generation 是否有效
pub fn is_valid_endpoint(ep: Endpoint, generations: &[i32; NR_PROCS]) -> bool {
    let slot = endpoint_slot(ep);
    let gen = endpoint_generation(ep);
    slot < NR_PROCS && gen == generations[slot]
}
```

---

## 5. PCB 克隆实现

### 5.1 sys_fork 核心逻辑

```rust
impl KProcess {
    pub fn sys_fork(
        parent: &KProcess,
        child_slot: usize,
        flags: u32,
        generations: &mut [i32; NR_PROCS],
    ) -> Result<(Self, Endpoint), SysForkError> {
        // Generation 递增，回绕到1而非0
        let gen = generations[child_slot] + 1;
        generations[child_slot] = if gen >= ENDPOINT_MAX_GENERATION { 1 } else { gen };
        let child_endpoint = make_endpoint(generations[child_slot], child_slot);

        // 整体复制父进程PCB
        let mut child = KProcess {
            slot: child_slot,
            endpoint: child_endpoint,
            rts_flags: parent.rts_flags,
            misc_flags: parent.misc_flags
                & !(MiscFlags::VIRT_TIMER | MiscFlags::PROF_TIMER
                    | MiscFlags::SC_TRACE | MiscFlags::STEP),
            priority: parent.priority,
            quantum_size_ms: parent.quantum_size_ms,
            user_time: 0,
            sys_time: 0,
            virt_left: 0,
            prof_left: 0,
            name: parent.name,
            pending_signals: 0,
            is_system_proc: parent.is_system_proc,
            ret_reg: 0, // 核心：伪造子进程返回值为0
            cr3: 0, // 页表基址清零，后续由VM设置
            fpu_state: parent.fpu_state.clone(),
        };

        // 追加进程名标识
        let namelen = child.name.iter().position(|&c| c == 0).unwrap_or(16);
        if namelen + 2 < 16 {
            child.name[namelen] = b'*';
            child.name[namelen + 1] = b'F';
        }

        // 子进程初始无时间片
        child.rts_flags |= RtsFlags::NO_QUANTUM;

        // 特权进程子进程降级为普通用户
        if parent.is_system_proc {
            child.is_system_proc = false;
            child.rts_flags |= RtsFlags::NO_PRIV;
        }

        // VM 抑制标志，等待VM绑定页表后清除
        if flags & PFF_VMINHIBIT != 0 {
            child.rts_flags |= RtsFlags::VMINHIBIT;
        }

        // 清除继承的信号状态
        child.rts_flags &= !(RtsFlags::SIGNALED | RtsFlags::SIG_PENDING | RtsFlags::P_STOP);
        child.pending_signals = 0;

        Ok((child, child_endpoint))
    }
}
```

---

## 6. RTS 标志管理

| 标志 | 设置时机 | 清除时机 | 含义 |
|------|---------|---------|------|
| `NO_QUANTUM` | fork时 | 调度器分配时间片 | 子进程初始无时间片 |
| `VMINHIBIT` | fork时(VM请求) | VM调用pt_bind后 | 等待VM完成页表绑定 |
| `NO_PRIV` | fork时(特权父进程) | 永不 | 子进程特权降级 |
| `SIGNALED` | 收到信号时 | fork时清除 | 信号已送达 |
| `SIG_PENDING` | 信号排队时 | fork时清除 | 信号待处理 |

---

## 7. 上下文伪造

### 7.1 ret_reg = 0 机制

这是 fork 系统调用最核心的 trick：

```rust
// 父进程路径：从系统调用返回，ret_reg 保持原值
// 子进程路径：从调度恢复，ret_reg = 0

// 在 x86_64 上，ret_reg 对应 rax 寄存器
// 在 x86 上，ret_reg 对应 eax 寄存器
// 在 ARM 上，ret_reg 对应 r0 寄存器

child.ret_reg = 0; // 子进程从 fork 返回 0
```

### 7.2 父子进程区分

```rust
// 父进程：系统调用正常返回，返回值 = 子进程 PID
// 子进程：调度器首次调度，从 ret_reg = 0 恢复执行

match who_am_i() {
    Parent => return child_pid,  // 父进程返回子进程PID
    Child => return 0,           // 子进程返回0（通过ret_reg伪造）
}
```

---

## 8. 测试用例

### 8.1 Endpoint 测试

```rust
#[test]
fn test_make_endpoint() {
    let ep = make_endpoint(1, 5);
    assert_eq!(endpoint_generation(ep), 1);
    assert_eq!(endpoint_slot(ep), 5);
}

#[test]
fn test_endpoint_generation_wraparound() {
    let mut gen = ENDPOINT_MAX_GENERATION;
    gen = if gen + 1 >= ENDPOINT_MAX_GENERATION { 1 } else { gen + 1 };
    assert_eq!(gen, 1); // 回绕到1而非0
}
```

### 8.2 sys_fork 测试

```rust
#[test]
fn test_sys_fork_ret_reg_zero() {
    let (child, _) = KProcess::sys_fork(&parent, 5, 0, &mut generations).unwrap();
    assert_eq!(child.ret_reg, 0);
}

#[test]
fn test_sys_fork_rts_no_quantum() {
    let (child, _) = KProcess::sys_fork(&parent, 5, 0, &mut generations).unwrap();
    assert!(child.rts_flags.contains(RtsFlags::NO_QUANTUM));
}

#[test]
fn test_sys_fork_privileged_demotion() {
    let mut parent = create_privileged_process();
    let (child, _) = KProcess::sys_fork(&parent, 5, 0, &mut generations).unwrap();
    assert!(!child.is_system_proc);
    assert!(child.rts_flags.contains(RtsFlags::NO_PRIV));
}

#[test]
fn test_sys_fork_name_appended() {
    let mut parent = KProcess::default();
    parent.name = *b"init\0\0\0\0\0\0\0\0\0\0\0\0";
    let (child, _) = KProcess::sys_fork(&parent, 5, 0, &mut generations).unwrap();
    assert_eq!(&child.name[0..6], b"init*F");
}
```

---

## 9. 检查清单

| # | 逻辑点 | 状态 |
|---|--------|------|
| K-01 | `KProcess` 结构体对齐 struct proc | ⬜ |
| K-02 | `RtsFlags` 完整位标志定义 | ⬜ |
| K-03 | `MiscFlags` 完整位标志定义 | ⬜ |
| K-04 | `ProcTable` 包含 generations 数组 | ⬜ |
| K-05 | `*rpc = *rpp` PCB 整体复制 | ⬜ |
| K-06 | `ret_reg = 0` 子进程返回值伪造 | ⬜ |
| K-07 | 寄存器映射对齐架构 (eax/rax/r0) | ⬜ |
| K-08 | 上下文恢复逻辑对齐 | ⬜ |
| K-09 | `make_endpoint()` 精确实现 Minix3 算法 | ⬜ |
| K-10 | `endpoint_generation()` 正确提取 generation | ⬜ |
| K-11 | `endpoint_slot()` 正确提取槽位 | ⬜ |
| K-12 | generation 每次 fork 递增 | ⬜ |
| K-13 | generation 溢出回绕到 1 而非 0 | ⬜ |
| K-14 | `is_valid_endpoint()` generation 验证 | ⬜ |
| K-15 | `RTS_NO_QUANTUM` 初始设置 | ⬜ |
| K-16 | `RTS_VMINHIBIT` 条件设置 | ⬜ |
| K-17 | `RTS_NO_PRIV` 特权降级设置 | ⬜ |
| K-18 | 信号相关 RTS 标志清除 | ⬜ |
| K-19 | 进程可运行判定 (rts_flags == 0) | ⬜ |
| K-20 | FPU 保存区修复，保留子槽缓冲区 | ⬜ |
| K-21 | FPU 内容复制逻辑 | ⬜ |
| K-22 | 特权进程子进程降级逻辑 | ⬜ |
| K-23 | `p_pending` 信号集清空 | ⬜ |
| K-24 | `cr3` 页表基址清零 | ⬜ |
| K-25 | 进程名追加 "*F" | ⬜ |
| K-26 | 记账统计全部归零 | ⬜ |
| K-27 | MiscFlags 无关标志清除 | ⬜ |
