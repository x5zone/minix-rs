# Kernel do_fork 实现

> 本文档详细说明 Kernel 层的 `do_fork()` 实现，包括 PCB 克隆、上下文伪造和 Endpoint 生成。

## C 源码位置

| 文件 | 路径 | 说明 |
|------|------|------|
| [`do_fork.c`](../../../../minix3/minix/kernel/system/do_fork.c) | `minix3/minix/kernel/system/do_fork.c` | Kernel fork 主逻辑，`do_fork()` 函数（约 120 行） |
| [`proc.c`](../../../../minix3/minix/kernel/proc.c) | `minix3/minix/kernel/proc.c` | 进程管理，`proc_addr()`, `isokendpt()` |
| [`proc.h`](../../../../minix3/minix/kernel/proc.h) | `minix3/minix/kernel/proc.h` | `struct proc`, `struct stackframe_s` 定义 |
| [`endpoint.h`](../../../../minix3/include/minix/endpoint.h) | `minix3/include/minix/endpoint.h` | Endpoint 生成算法，`_ENDPOINT()` 宏 |

---

## 1. 函数签名

### 1.1 C 源码接口

```c
int do_fork(proc *caller, message *m_ptr);
```

**参数**:
- `caller`: 调用者进程（通常是 VM）
- `m_ptr`: 消息指针，包含 fork 参数

**消息字段**:
```c
// 输入
m_ptr->m_lsys_krn_sys_fork.endpt   // 父进程 endpoint
m_ptr->m_lsys_krn_sys_fork.slot    // 子进程槽号
m_ptr->m_lsys_krn_sys_fork.flags   // PFF_VMINHIBIT (0x01)

// 输出
m_ptr->m_krn_lsys_sys_fork.endpt   // 子进程的新 endpoint
m_ptr->m_krn_lsys_sys_fork.msgaddr // 子进程的消息缓冲区虚拟地址
```

### 1.2 Rust 接口设计

```rust
pub fn do_fork(
    kernel: &mut KernelState,
    caller: Endpoint,
    request: &SysForkRequest,
) -> Result<SysForkResponse, Error>;
```

---

## 2. PCB 克隆

### 2.1 核心数据结构

#### proc（PCB — 进程控制块）

```c
struct proc {
    stackframe_s p_reg;        // 保存的寄存器（含 retreg = eax/r0）
    segframe p_seg;            // 段描述符（含 fpu_state, p_cr3）
    proc_nr_t p_nr;            // 进程号
    priv *p_priv;              // 特权结构指针
    u32_t p_rts_flags;         // 运行时标志（==0 才可运行）
    u32_t p_misc_flags;        // 杂项标志
    char p_priority;           // 优先级
    u64_t p_cpu_time_left;     // 剩余 CPU 时间
    unsigned p_quantum_size_ms;// 时间片
    proc *p_scheduler;         // 调度器
    clock_t p_user_time;       // 用户态时间
    clock_t p_sys_time;        // 内核态时间
    clock_t p_virt_left;       // 虚拟定时器剩余
    clock_t p_prof_left;       // 剖析定时器剩余
    sigset_t p_pending;        // 待处理信号
    char p_name[PROC_NAME_LEN];// 进程名
    endpoint_t p_endpoint;     // endpoint（含 generation）
    message p_delivermsg;      // 待投递消息
    vir_bytes p_delivermsg_vir;// 消息缓冲区虚拟地址
};
```

#### stackframe_s（x86 寄存器保存区）

```c
struct stackframe_s {
    u16_t gs, fs, es, ds;
    reg_t di, si, fp, bx, dx, cx;
    reg_t retreg;    // = eax！fork 返回值寄存器
    reg_t pc;        // = eip
    reg_t cs;
    reg_t psw;       // = eflags
    reg_t sp;        // = esp
    reg_t ss;
};
```

### 2.2 do_fork() 逐行分析

```c
int do_fork(proc *caller, message *m_ptr) {
    // 1. 验证参数
    isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc);
    rpp = proc_addr(p_proc);   // 父进程
    rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);  // 子进程
    assert(!isemptyp(rpp) && isemptyp(rpc));
    assert(!(rpp->p_misc_flags & MF_DELIVERMSG));
    assert(RTS_ISSET(rpp, RTS_RECEIVING));  // 父进程必须阻塞在接收状态

    // 2. 保存 FPU 上下文
    save_fpu(rpp);
    gen = _ENDPOINT_G(rpc->p_endpoint);
    old_fpu_save_area_p = rpc->p_seg.fpu_state;  // 保存子槽的 FPU 缓冲区

    // 3. 整体复制 PCB
    *rpc = *rpp;  // C 结构体赋值 = memcpy

    // 4. 修复 FPU 缓冲区指针
    rpc->p_seg.fpu_state = old_fpu_save_area_p;
    if (proc_used_fpu(rpp))
        memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);

    // 5. 递增 generation，生成新 endpoint
    if (++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;
    rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;
    rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);

    // 6. 伪造子进程返回值：rax = 0
    rpc->p_reg.retreg = 0;

    // 7. 清零时间统计
    rpc->p_user_time = 0;
    rpc->p_sys_time = 0;

    // 8. 清除杂项标志
    rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_STEP);
    rpc->p_virt_left = 0;
    rpc->p_prof_left = 0;

    // 9. 追加进程名 "*F"
    if (strlen(rpc->p_name) + 2 < sizeof(rpc->p_name))
        strcat(rpc->p_name, "*F");

    // 10. 设置不可运行
    RTS_SET(rpc, RTS_NO_QUANTUM);
    reset_proc_accounting(rpc);
    rpc->p_cpu_time_left = 0;
    rpc->p_cycles = 0;
    rpc->p_kcall_cycles = 0;
    rpc->p_kipc_cycles = 0;
    rpc->p_tick_cycles = 0;
    cpuavg_init(&rpc->p_cpuavg);

    // 11. 特权进程降级
    if (priv(rpp)->s_flags & SYS_PROC) {
        rpc->p_priv = priv_addr(USER_PRIV_ID);
        rpc->p_rts_flags |= RTS_NO_PRIV;
    }

    // 12. 返回子进程 endpoint 和消息地址
    m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
    m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir;

    // 13. VM 抑制
    if (m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT)
        RTS_SET(rpc, RTS_VMINHIBIT);

    // 14. 清除信号和追踪
    RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP);
    sigemptyset(&rpc->p_pending);

    // 15. 清除页表基址
    rpc->p_seg.p_cr3 = 0;
    rpc->p_seg.p_cr3_v = NULL;

    return OK;
}
```

---

## 3. 上下文伪造

### 3.1 retreg = 0 如何使子进程从 fork 返回 0

1. 父进程调用 `sys_fork()`，处于 `RTS_RECEIVING` 状态
2. `*rpc = *rpp` 复制父进程的完整寄存器状态（包括 eax）
3. `rpc->p_reg.retreg = 0` 将子进程的 eax 强制设为 0
4. 子进程被调度运行时，`restore_user_context()` 从 `p_reg` 恢复所有寄存器
5. 子进程用户态看到 eax = 0，即 `fork()` 返回 0

### 3.2 FPU 上下文处理

FPU 状态保存在每个进程槽的专用缓冲区中。fork 时需要：
1. 保存父进程的 FPU 上下文
2. 保留子进程槽的 FPU 缓冲区指针
3. 如果父进程使用过 FPU，复制 FPU 状态到子进程

---

## 4. RTS 设置

### 4.1 RTS 标志位

| 标志 | 值 | fork 中的操作 |
|------|-----|-------------|
| `RTS_NO_QUANTUM` | 0x8000 | **设置** — 子进程无时间片 |
| `RTS_VMINHIBIT` | 0x200 | **条件设置** — 等 VM 设置页表 |
| `RTS_NO_PRIV` | 0x80 | **条件设置** — 特权进程子进程降级 |
| `RTS_SIGNALED` | 0x10 | **清除** — 不继承信号 |
| `RTS_SIG_PENDING` | 0x20 | **清除** — 不继承信号处理 |
| `RTS_P_STOP` | 0x40 | **清除** — 不继承追踪 |

**核心规则**: `p_rts_flags == 0` 时进程才可运行。

### 4.2 调度准备

子进程初始状态：
- `RTS_NO_QUANTUM`：无时间片，需要调度器分配
- `p_cpu_time_left = 0`：无剩余 CPU 时间
- 所有统计计数器清零

---

## 5. Endpoint Generation

### 5.1 算法

```c
#define _ENDPOINT_GENERATION_SHIFT  15
#define _ENDPOINT_MAX_GENERATION    65535

endpoint = (generation << 15) | slot
generation = (endpoint + MAX_NR_TASKS) >> 15
slot = ((endpoint + MAX_NR_TASKS) & 0x7FFF) - MAX_NR_TASKS
```

fork 时：取子槽当前 generation，+1，若溢出则回绕到 1（不是 0）。

### 5.2 目的

- 防止过时的 endpoint 引用被误用
- 每个 slot 可以重用，但 generation 递增保证唯一性

---

## 6. Rust 结构定义

```rust
pub struct KProcess {
    pub slot: usize,
    pub endpoint: Endpoint,
    pub rts_flags: RtsFlags,
    pub misc_flags: MiscFlags,
    pub priority: i8,
    pub quantum_size_ms: u32,
    pub user_time: Clock,
    pub sys_time: Clock,
    pub virt_left: Clock,
    pub prof_left: Clock,
    pub name: [u8; 16],
    pub pending_signals: u64,
    pub is_system_proc: bool,
    pub ret_reg: i32,           // fork 返回值寄存器
    pub cr3: u64,               // 页表基址（Mock）
    pub fpu_state: Vec<u8>,     // FPU 保存区（Mock）
}

bitflags::bitflags! {
    pub struct RtsFlags: u32 {
        const SLOT_FREE = 0x01;
        const PROC_STOP = 0x02;
        const SENDING = 0x04;
        const RECEIVING = 0x08;
        const SIGNALED = 0x10;
        const SIG_PENDING = 0x20;
        const P_STOP = 0x40;
        const NO_PRIV = 0x80;
        const NO_ENDPOINT = 0x100;
        const VMINHIBIT = 0x200;
        const NO_QUANTUM = 0x8000;
    }
}

pub struct ProcTable {
    pub procs: [Option<KProcess>; NR_PROCS],
    pub generations: [i32; NR_PROCS],
}
```

---

## 7. 测试验证

### 7.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_do_fork_copies_pcb() {
        // 验证 PCB 被正确复制
    }

    #[test]
    fn test_do_fork_retreg_zero() {
        // 验证子进程 retreg = 0
    }

    #[test]
    fn test_do_fork_endpoint_generation() {
        // 验证 endpoint generation 递增
    }

    #[test]
    fn test_do_fork_rts_flags() {
        // 验证 RTS 标志正确设置
    }
}
```

### 7.2 集成测试

```rust
#[test]
fn test_do_fork_end_to_end() {
    // 完整 Kernel fork 流程测试
}
```
