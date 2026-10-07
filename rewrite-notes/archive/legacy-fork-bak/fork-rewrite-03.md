# 阶段 3：Kernel — PCB 克隆与上下文伪造实现
> **状态**: ❌ 待实现
> **硬件依赖**: 寄存器读写Mock、FPU上下文Mock
> **Mock说明**: 所有寄存器硬件访问、FPU状态保存/恢复全部使用Mock，仅实现上下文伪造(ret_reg=0)、RTS标志管理、Endpoint生成逻辑
> **对应源码**: `minix/kernel/system/do_fork.c`, `minix/include/minix/com.h`

---

## 一、任务清单
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

## 二、核心实现模板
### 2.1 Endpoint 生成算法
```rust
pub const ENDPOINT_GENERATION_SHIFT: u32 = 15;
pub const ENDPOINT_MAX_GENERATION: i32 = 65535;

pub fn make_endpoint(generation: i32, slot: usize) -> Endpoint {
    Endpoint::new((generation << ENDPOINT_GENERATION_SHIFT as i32) | (slot as i32))
}

pub fn endpoint_generation(ep: Endpoint) -> i32 {
    (ep.get() >> ENDPOINT_GENERATION_SHIFT as i32) & 0x7FFF
}

pub fn endpoint_slot(ep: Endpoint) -> usize {
    (ep.get() & 0x7FFF) as usize
}
```

### 2.2 `KProcess::sys_fork()` PCB 克隆
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

## 三、必须通过的单元测试
```
test_make_endpoint()
test_endpoint_generation()
test_endpoint_slot()
test_endpoint_roundtrip()
test_sys_fork_ret_reg_zero()
test_sys_fork_generation_increment()
test_sys_fork_generation_wraparound()
test_sys_fork_rts_no_quantum()
test_sys_fork_rts_vminhibit()
test_sys_fork_privileged_demotion()
test_sys_fork_signal_cleared()
test_sys_fork_name_appended()
test_sys_fork_fpu_preserved()
test_sys_fork_cr3_cleared()
```

---

## 四、检查清单
| # | 逻辑点 | 状态 |
|---|--------|------|
| K-01 | `KProcess` 结构体对齐 struct proc | ❌ |
| K-02 | `RtsFlags` 完整位标志定义 | ❌ |
| K-03 | `MiscFlags` 完整位标志定义 | ❌ |
| K-04 | `ProcTable` 包含 generations 数组 | ❌ |
| K-05 | `*rpc = *rpp` PCB 整体复制 | ❌ |
| K-06 | `ret_reg = 0` 子进程返回值伪造 | ❌ |
| K-07 | 寄存器映射对齐架构 (eax/rax/r0) | ❌ |
| K-08 | 上下文恢复逻辑对齐 | ❌ |
| K-09 | `make_endpoint()` 精确实现 Minix3 算法 | ❌ |
| K-10 | `endpoint_generation()` 正确提取 generation | ❌ |
| K-11 | `endpoint_slot()` 正确提取槽位 | ❌ |
| K-12 | generation 每次 fork 递增 | ❌ |
| K-13 | generation 溢出回绕到 1 而非 0 | ❌ |
| K-14 | `is_valid_endpoint()` generation 验证 | ❌ |
| K-15 | `RTS_NO_QUANTUM` 初始设置 | ❌ |
| K-16 | `RTS_VMINHIBIT` 条件设置 | ❌ |
| K-17 | `RTS_NO_PRIV` 特权降级设置 | ❌ |
| K-18 | 信号相关 RTS 标志清除 | ❌ |
| K-19 | 进程可运行判定 (rts_flags == 0) | ❌ |
| K-20 | FPU 保存区修复，保留子槽缓冲区 | ❌ |
| K-21 | FPU 内容复制逻辑 | ❌ |
| K-22 | 特权进程子进程降级逻辑 | ❌ |
| K-23 | `p_pending` 信号集清空 | ❌ |
| K-24 | `cr3` 页表基址清零 | ❌ |
| K-25 | 进程名追加 "*F" | ❌ |
| K-26 | 记账统计全部归零 | ❌ |
| K-27 | MiscFlags 无关标志清除 | ❌ |