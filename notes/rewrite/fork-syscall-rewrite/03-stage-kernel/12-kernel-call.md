# 12-kernel-call - kernel_call 函数

> 本文档分析 `minix3/minix/kernel/system.c` 第 81-140 行，讲解 kernel_call 函数。

---

## 1. 概述

`kernel_call` 是内核系统调用的统一入口函数，负责将用户空间的系统调用请求分发到对应的处理函数。

**核心职责**：
- 保存用户空间消息地址
- 从用户空间复制消息
- 分发到具体处理函数
- 记账和完成处理

### 1.1 系统调用入口

`kernel_call` 是所有内核系统调用的唯一入口点。

**入口角色**：

1. **统一入口**：所有内核系统调用（如 SYS_FORK、SYS_EXEC 等）都通过此函数进入
2. **参数验证**：检查消息地址有效性
3. **分发调度**：根据消息类型调用对应处理函数
4. **结果返回**：将处理结果返回给调用者

### 1.2 与 fork 的关系

fork 系统调用通过以下路径进入内核：

```
用户进程调用 fork()
       │
       ▼
PM 进程发送 SYS_FORK 消息
       │
       ▼
kernel_call(m_user, caller)
       │
       ▼
kernel_call_dispatch(caller, &msg)
       │
       ▼
call_vec[SYS_FORK - KERNEL_CALL](caller, msg)
       │
       ▼
do_fork(caller, msg)
```

---

## 2. C 源码分析

本节详细分析 `kernel_call` 函数的实现（`system.c` 第 144-170 行），包括消息地址保存、消息复制、调度分发、记账和完成处理。

### 2.1 函数签名

`kernel_call` 函数签名：

```c
void kernel_call(message *m_user, struct proc *caller)
```

**参数**：

| 参数 | 类型 | 含义 |
|------|------|------|
| `m_user` | `message *` | 用户空间消息地址 |
| `caller` | `struct proc *` | 调用者进程指针 |

**返回值**：`void`，结果通过 `kernel_call_finish` 处理

#### 2.1.1 m_user 参数

`m_user` 是指向用户空间消息的指针。

**作用**：

1. **消息来源**：指向调用者进程用户空间中的消息缓冲区
2. **虚拟地址**：此地址是调用者进程的虚拟地址，需要通过 `copy_msg_from_user` 复制到内核空间
3. **保存地址**：此地址被保存到 `caller->p_delivermsg_vir`，用于后续将结果消息复制回用户空间

#### 2.1.2 caller 参数

`caller` 是调用者进程的进程结构指针。

**作用**：

1. **进程标识**：确定哪个进程发起了系统调用
2. **端点获取**：通过 `caller->p_endpoint` 获取调用者端点
3. **权限检查**：通过 `priv(caller)` 获取调用者特权结构
4. **地址空间**：确定消息复制的地址空间

### 2.2 消息地址保存

消息地址保存确保内核可以在后续处理中将结果消息复制回用户空间。

```c
caller->p_delivermsg_vir = (vir_bytes) m_user;
```

#### 2.2.1 p_delivermsg_vir 设置

`p_delivermsg_vir` 保存用户空间消息缓冲区的虚拟地址。

```c
caller->p_delivermsg_vir = (vir_bytes) m_user;
```

**设置时机**：在 `kernel_call` 入口处立即保存

**用途**：`kernel_call_finish` 中将结果消息复制回此地址

#### 2.2.2 用户空间消息地址

需要保存用户空间消息地址的原因：

1. **异步返回**：系统调用可能不会立即返回，内核需要在稍后将结果消息复制回去
2. **地址空间切换**：内核处理过程中可能切换到其他进程，需要记住原始地址
3. **中断处理**：系统调用可能被中断，恢复时需要知道用户空间缓冲区位置
4. **结果传递**：`kernel_call_finish` 使用此地址将结果返回给用户进程

### 2.3 消息复制

消息复制将用户空间消息安全地复制到内核空间。

```c
if (copy_msg_from_user(m_user, &msg) == 0) {
    msg.m_source = caller->p_endpoint;
    result = kernel_call_dispatch(caller, &msg);
} else {
    // 复制失败处理
}
```

#### 2.3.1 copy_msg_from_user

`copy_msg_from_user` 将消息从用户空间安全地复制到内核空间。

**函数原型**：

```c
int copy_msg_from_user(message *m_user, message *m_kernel);
```

**返回值**：

| 返回值 | 含义 |
|--------|------|
| 0 | 复制成功 |
| 非零 | 复制失败（地址无效） |

**安全性**：此函数使用调用者进程的页表进行地址翻译，确保不会越界访问。

#### 2.3.2 消息源设置

`msg.m_source = caller->p_endpoint` 设置消息的源端点。

**含义**：

1. **标识发送者**：消息的 `m_source` 字段标识消息的发送者
2. **内核设置**：由内核设置，而非用户空间，防止伪造
3. **端点值**：使用进程的端点值，而非槽位号

**安全意义**：如果允许用户空间设置 `m_source`，进程可以伪造消息来源。

#### 2.3.3 复制失败处理

复制失败时，内核向调用者进程发送 SIGSEGV 信号。

```c
else {
    printf("WARNING wrong user pointer 0x%08x from process %s / %d\n",
                    m_user, caller->p_name, caller->p_endpoint);
    cause_sig(proc_nr(caller), SIGSEGV);
    return;
}
```

##### 2.3.3.1 错误日志

错误日志输出包含关键诊断信息：

```c
printf("WARNING wrong user pointer 0x%08x from process %s / %d\n",
                m_user, caller->p_name, caller->p_endpoint);
```

**日志内容**：

| 字段 | 含义 |
|------|------|
| `0x%08x` | 无效的用户空间地址 |
| `%s` | 进程名称 |
| `%d` | 进程端点 |

##### 2.3.3.2 cause_sig 调用

`cause_sig(proc_nr(caller), SIGSEGV)` 向调用者进程发送段错误信号。

**作用**：

1. **信号发送**：向进程发送 SIGSEGV（段错误）信号
2. **进程通知**：进程将在下次调度时收到此信号
3. **默认处理**：默认行为是终止进程

**为什么是 SIGSEGV**：传递无效的消息地址等同于访问无效内存，与段错误语义一致。

### 2.4 调度分发

调用分发将系统调用请求路由到对应的处理函数。

```c
result = kernel_call_dispatch(caller, &msg);
```

#### 2.4.1 kernel_call_dispatch 调用

`kernel_call_dispatch` 根据消息类型分发到对应的处理函数。

**函数逻辑**：

1. **提取调用号**：`call_nr = msg->m_type - KERNEL_CALL`
2. **范围检查**：`call_nr < 0 || call_nr >= NR_SYS_CALLS`
3. **权限检查**：`GET_BIT(priv(caller)->s_k_call_mask, call_nr)`
4. **调用处理**：`result = (*call_vec[call_nr])(caller, msg)`

**返回值**：

| 值 | 含义 |
|----|------|
| `OK` | 调用成功 |
| `EBADREQUEST` | 非法请求号 |
| `ECALLDENIED` | 权限不足 |
| 其他 | 处理函数返回的错误码 |

#### 2.4.2 返回值处理

`result` 变量存储系统调用的返回值。

```c
int result = OK;  // 初始化为 OK
```

**可能的值**：

| 值 | 含义 |
|----|------|
| `OK` (0) | 成功 |
| `EBADREQUEST` | 非法请求 |
| `ECALLDENIED` | 权限被拒 |
| `EINVAL` | 无效参数 |
| `ENOMEM` | 内存不足 |

### 2.5 记账

记账代码记录哪个进程发起了内核调用，用于 CPU 时间统计。

```c
kbill_kcall = caller;
```

#### 2.5.1 kbill_kcall 设置

`kbill_kcall = caller` 记录当前发起内核调用的进程。

**作用**：

1. **CPU 记账**：将内核调用处理时间计入调用者进程
2. **全局变量**：`kbill_kcall` 是全局指针，指向当前记账的进程
3. **时间统计**：时钟中断处理时使用此变量进行时间分配

#### 2.5.2 内核调用记账

内核调用记账的目的：

1. **公平计费**：内核代表进程执行操作的时间应计入该进程
2. **性能分析**：统计各进程的内核调用耗时
3. **资源监控**：监控进程对内核资源的使用
4. **调度依据**：CPU 时间统计影响进程优先级调整

### 2.6 完成处理

完成处理将结果消息复制回用户空间并恢复进程执行。

```c
kernel_call_finish(caller, &msg, result);
```

#### 2.6.1 kernel_call_finish 调用

`kernel_call_finish` 完成系统调用的后续处理。

**参数**：

| 参数 | 含义 |
|------|------|
| `caller` | 调用者进程指针 |
| `msg` | 处理后的消息 |
| `result` | 系统调用返回值 |

**主要工作**：

1. **设置返回值**：将 `result` 写入消息
2. **复制回用户**：将结果消息复制回 `caller->p_delivermsg_vir`
3. **恢复执行**：设置进程状态，使其可以继续执行

---

## 3. 调用流程图

本节绘制 `kernel_call` 的完整调用流程图。

### 3.1 正常流程

```
kernel_call(m_user, caller)
       │
       ▼
保存 p_delivermsg_vir = m_user
       │
       ▼
copy_msg_from_user(m_user, &msg)
       │
       ├── 成功
       │       │
       │       ▼
       │   msg.m_source = caller->p_endpoint
       │       │
       │       ▼
       │   kernel_call_dispatch(caller, &msg)
       │       │
       │       ▼
       │   kbill_kcall = caller
       │       │
       │       ▼
       │   kernel_call_finish(caller, &msg, result)
       │
       └── 失败
               │
               ▼
           cause_sig(proc_nr(caller), SIGSEGV)
               │
               ▼
           return
```

### 3.2 错误流程

```
kernel_call_dispatch(caller, &msg)
       │
       ▼
call_nr = msg->m_type - KERNEL_CALL
       │
       ├── call_nr < 0 || call_nr >= NR_SYS_CALLS
       │       │
       │       ▼
       │   result = EBADREQUEST
       │
       ├── !GET_BIT(s_k_call_mask, call_nr)
       │       │
       │       ▼
       │   result = ECALLDENIED
       │
       └── 合法请求
               │
               ▼
           call_vec[call_nr](caller, msg)
               │
               ├── 处理函数存在
               │       │
               │       ▼
               │   result = 处理函数返回值
               └── 处理函数为 NULL
                       │
                       ▼
                   result = EBADREQUEST
```

---

## 4. Rust 设计决策

本节讨论如何用 Rust 实现 `kernel_call`，重点关注消息复制安全性和错误处理。

### 4.1 用户空间消息复制

Rust 中用户空间消息复制可以通过类型系统保证安全性。

**C 的问题**：

```c
// C: 无类型检查，可能越界
if (copy_msg_from_user(m_user, &msg) == 0) { ... }
```

**Rust 改进**：

```rust
// Rust: 使用 Result 类型处理错误
fn copy_msg_from_user(m_user: *const Message, msg: &mut Message) -> Result<(), CopyError> {
    // 安全检查 + 复制
}
```

**优势**：

1. **编译期检查**：`Result` 类型强制处理错误
2. **类型安全**：消息类型由结构体定义
3. **边界检查**：复制操作有长度验证

### 4.2 错误处理

Rust 使用 `Result` 类型替代 C 的错误码。

**C 的方式**：

```c
if (copy_msg_from_user(m_user, &msg) == 0) {
    // 成功
} else {
    cause_sig(proc_nr(caller), SIGSEGV);
    return;
}
```

**Rust 的方式**：

```rust
match copy_msg_from_user(m_user, &mut msg) {
    Ok(()) => {
        msg.m_source = caller.endpoint();
        result = kernel_call_dispatch(caller, &mut msg);
    }
    Err(_) => {
        cause_sig(caller.proc_nr(), Signal::SIGSEGV);
        return;
    }
}
```

**优势**：错误路径在编译期被强制处理，不会遗漏。

### 4.3 记账机制

Rust 中记账机制可以通过 RAII 模式实现。

**C 的方式**：

```c
kbill_kcall = caller;  // 手动设置
```

**Rust 的方式**：

```rust
struct BillingGuard'a {
    bill: &'a mut Option<ProcRef>,
}

impl'a Drop for BillingGuard'a {
    fn drop(&mut self) {
        // 自动清理
    }
}
```

**优势**：RAII 确保记账在作用域结束时自动清理。

---

## 5. 实现

本节给出 `kernel_call` 的 Rust 实现代码。

### 5.1 kernel_call 函数

```rust
pub fn kernel_call(m_user: *const Message, caller: &mut Proc) {
    caller.delivermsg_vir = m_user as usize;

    let mut msg = match copy_msg_from_user(m_user, caller) {
        Ok(m) => m,
        Err(_) => {
            log::warn!(
                "wrong user pointer {:?} from process {} / {}",
                m_user, caller.name(), caller.endpoint()
            );
            cause_sig(caller.proc_nr(), Signal::SIGSEGV);
            return;
        }
    };

    msg.m_source = caller.endpoint();
    let result = kernel_call_dispatch(caller, &mut msg);

    kbill_kcall = Some(caller.as_ref());
    kernel_call_finish(caller, &msg, result);
}
```

### 5.2 消息复制函数

```rust
pub fn copy_msg_from_user(
    m_user: *const Message,
    caller: &Proc
) -> Result<Message, CopyError> {
    if m_user.is_null() {
        return Err(CopyError::NullPointer);
    }
    let user_addr = m_user as usize;
    if user_addr >= USR_DATATOP {
        return Err(CopyError::InvalidAddress(user_addr));
    }
    let msg = unsafe { arch::copy_from_user(caller, m_user) }?;
    Ok(msg)
}
```

### 5.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_call_null_pointer() {
        let mut caller = Proc::new_for_test();
        let result = copy_msg_from_user(core::ptr::null(), &caller);
        assert!(result.is_err());
    }

    #[test]
    fn test_kernel_call_invalid_address() {
        let mut caller = Proc::new_for_test();
        let addr = USR_DATATOP as *const Message;
        let result = copy_msg_from_user(addr, &caller);
        assert!(result.is_err());
    }
}
```

---

## 6. 参见

- [11-system-init](11-system-init.md) - 系统调用初始化
- [13-kernel-call-dispatch](13-kernel-call-dispatch.md) - kernel_call_dispatch
- [14-kernel-call-finish](14-kernel-call-finish.md) - kernel_call_finish
