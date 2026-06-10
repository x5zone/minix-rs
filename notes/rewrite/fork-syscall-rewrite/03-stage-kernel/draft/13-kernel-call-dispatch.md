# 13-kernel-call-dispatch - kernel_call_dispatch 函数

> 本文档分析 `minix3/minix/kernel/system.c` 第 141-190 行，讲解 kernel_call_dispatch 函数。

---

## 1. 概述

`kernel_call_dispatch` 负责将系统调用请求分发到对应的处理函数，包括调用号验证、权限检查和函数调用。

### 1.1 系统调用分发

系统调用分发采用向量表机制：

1. **调用号提取**：从消息的 `m_type` 字段提取调用号
2. **范围检查**：验证调用号在有效范围内
3. **权限检查**：验证调用者是否有权执行此系统调用
4. **函数调用**：通过函数指针向量表调用对应处理函数

### 1.2 与 fork 的关系

fork 系统调用的分发路径：

```
msg->m_type = SYS_FORK
call_nr = SYS_FORK - KERNEL_CALL
call_vec[call_nr] = do_fork
result = do_fork(caller, msg)
```

---

## 2. C 源码分析

本节详细分析 `kernel_call_dispatch` 函数的实现（`system.c` 第 97-131 行）。

### 2.1 函数签名

```c
static int kernel_call_dispatch(struct proc *caller, message *msg)
```

**参数**：

| 参数 | 类型 | 含义 |
|------|------|------|
| `caller` | `struct proc *` | 调用者进程指针 |
| `msg` | `message *` | 内核空间消息 |

**返回值**：`int`，系统调用结果码

#### 2.1.1 caller 参数

`caller` 用于权限检查和记账。

1. **权限检查**：`priv(caller)->s_k_call_mask` 获取调用者权限
2. **日志输出**：`caller->p_endpoint` 标识调用者
3. **传递给处理函数**：处理函数需要知道调用者信息

#### 2.1.2 msg 参数

`msg` 是内核空间的消息副本。

1. **调用号来源**：`msg->m_type` 包含系统调用号
2. **参数传递**：消息中包含系统调用参数
3. **结果返回**：处理函数可能修改消息内容

#### 2.1.3 返回值

返回值表示系统调用的执行结果：

| 值 | 含义 |
|----|------|
| `OK` (0) | 成功 |
| `EBADREQUEST` | 非法请求号 |
| `ECALLDENIED` | 权限不足 |
| 其他 | 处理函数返回的错误码 |

### 2.2 调试钩子

调试钩子用于 IPC 消息跟踪，仅在调试模式下启用。

```c
#if DEBUG_IPC_HOOK
    hook_ipc_msgkcall(msg, caller);
#endif
```

#### 2.2.1 DEBUG_IPC_HOOK

`DEBUG_IPC_HOOK` 是条件编译宏，控制 IPC 调试钩子。

- **启用时**：每次系统调用都会调用 `hook_ipc_msgkcall`
- **禁用时**：无额外开销
- **用途**：调试 IPC 消息流

#### 2.2.2 hook_ipc_msgkcall

`hook_ipc_msgkcall` 记录系统调用消息用于调试。

**作用**：

1. **消息记录**：记录进入内核调用的消息
2. **调用者记录**：记录发起调用的进程
3. **调试分析**：用于分析 IPC 消息流和系统调用序列

### 2.3 调用号提取

调用号提取从消息类型中计算出系统调用索引。

```c
call_nr = msg->m_type - KERNEL_CALL;
```

#### 2.3.1 call_nr 计算

`call_nr = msg->m_type - KERNEL_CALL` 将消息类型转换为调用号索引。

**转换原因**：

- `msg->m_type` 包含系统调用号（如 `SYS_FORK`）
- `KERNEL_CALL` 是系统调用号的起始偏移
- `call_nr` 是 `call_vec` 数组的索引

**示例**：

```
SYS_FORK = KERNEL_CALL + 0
call_nr = 0
call_vec[0] = do_fork
```

#### 2.3.2 KERNEL_CALL 偏移

`KERNEL_CALL` 是系统调用号的起始偏移量。

**作用**：

1. **编号基准**：所有内核系统调用号从 `KERNEL_CALL` 开始
2. **索引转换**：减去 `KERNEL_CALL` 得到 `call_vec` 索引
3. **编号空间**：与 IPC 消息类型共享编号空间

### 2.4 调用号检查

调用号检查确保请求号在有效范围内。

```c
if (call_nr < 0 || call_nr >= NR_SYS_CALLS) {
    result = EBADREQUEST;
}
```

#### 2.4.1 范围检查

范围检查防止数组越界访问 `call_vec`。

**检查条件**：

- `call_nr < 0`：负数索引无效
- `call_nr >= NR_SYS_CALLS`：超出向量表范围

**安全意义**：不检查会导致 `call_vec[call_nr]` 越界访问，可能调用任意函数指针。

#### 2.4.2 EBADREQUEST 错误

`EBADREQUEST` 表示请求的系统调用号无效。

**触发条件**：

1. 调用号超出范围
2. 对应的处理函数未注册（`call_vec[call_nr]` 为 NULL）

**处理方式**：返回错误码，由 `kernel_call_finish` 通知调用者。

#### 2.4.3 错误日志

错误日志输出调用号和调用者信息：

```c
printf("SYSTEM: illegal request %d from %d.\n", call_nr, msg->m_source);
```

**日志内容**：

| 字段 | 含义 |
|------|------|
| `%d` (call_nr) | 非法调用号 |
| `%d` (m_source) | 调用者端点 |

### 2.5 权限检查

权限检查验证调用者是否有权执行请求的系统调用。

```c
else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
    result = ECALLDENIED;
}
```

#### 2.5.1 s_k_call_mask

`s_k_call_mask` 是特权结构中的系统调用权限位图。

**作用**：

1. **权限位图**：每一位对应一个系统调用的权限
2. **按位检查**：`GET_BIT` 宏检查指定位是否设置
3. **进程特定**：每个进程有自己的权限位图

**示例**：

```
s_k_call_mask bit 0 = 1  →  允许 SYS_FORK
s_k_call_mask bit 0 = 0  →  禁止 SYS_FORK
```

#### 2.5.2 GET_BIT 宏

`GET_BIT` 宏检查位图中指定位是否设置。

```c
#define GET_BIT(map, bit)    (map[(bit)/BITCHUNK_BITS] & (1 << ((bit) % BITCHUNK_BITS)))
```

**计算过程**：

1. `(bit)/BITCHUNK_BITS`：定位 chunk 索引
2. `(bit) % BITCHUNK_BITS`：计算位内偏移
3. `1 << offset`：生成位掩码
4. `& mask`：测试对应位

#### 2.5.3 ECALLDENIED 错误

`ECALLDENIED` 表示调用者没有执行此系统调用的权限。

**触发条件**：`s_k_call_mask` 中对应位未设置

**安全意义**：防止低权限进程执行高权限系统调用。

#### 2.5.4 fork 权限

fork 系统调用需要的权限：

1. **s_k_call_mask**：`SYS_FORK` 对应位必须设置
2. **PM 权限**：PM 进程有完整的系统调用权限
3. **用户进程**：用户进程不直接调用 `SYS_FORK`，而是通过 PM 间接调用

### 2.6 调用执行

调用执行通过函数指针向量表调用对应的处理函数。

```c
if (call_vec[call_nr])
    result = (*call_vec[call_nr])(caller, msg);
```

#### 2.6.1 call_vec 索引

`call_vec` 是系统调用处理函数的向量表。

**定义**：

```c
int (*call_vec[NR_SYS_CALLS])(struct proc *, message *);
```

**初始化**：

```c
map(SYS_FORK, do_fork);    // call_vec[0] = do_fork
map(SYS_EXEC, do_exec);    // call_vec[1] = do_exec
```

**索引**：`call_nr` 作为数组索引，直接定位到处理函数。

#### 2.6.2 函数指针调用

函数指针调用执行对应的系统调用处理函数。

**执行过程**：

1. **查找函数**：`call_vec[call_nr]` 获取函数指针
2. **调用函数**：`(*func)(caller, msg)` 执行处理函数
3. **获取结果**：返回值存储在 `result` 中

##### 2.6.2.1 fork 调用路径

fork 系统调用的调用路径：

```
call_nr = SYS_FORK - KERNEL_CALL = 0
call_vec[0] = do_fork  (在 system_init 中通过 map 宏设置)
result = do_fork(caller, msg)
```

##### 2.6.2.2 返回值传递

`do_fork` 的返回值通过 `result` 变量传递：

1. **do_fork 返回**：`OK` 表示成功，`EINVAL` 等表示失败
2. **result 存储**：`result = do_fork(caller, msg)`
3. **传递给 finish**：`kernel_call_finish(caller, &msg, result)`
4. **通知调用者**：`kernel_call_finish` 将结果复制回用户空间

#### 2.6.3 NULL 处理

当 `call_vec[call_nr]` 为 NULL 时，表示该系统调用未注册。

```c
else {
    printf("Unused kernel call %d from %d\n", call_nr, caller->p_endpoint);
    result = EBADREQUEST;
}
```

**原因**：

1. 系统调用未实现
2. 系统调用被禁用
3. 内核配置中未包含该功能

---

## 3. 返回值分析

本节分析 `kernel_call_dispatch` 的所有可能返回值。

### 3.1 OK

`OK` (0) 表示系统调用成功执行。

**含义**：

1. 调用号合法
2. 权限检查通过
3. 处理函数返回成功

### 3.2 EBADREQUEST

`EBADREQUEST` 表示请求号无效。

**触发条件**：

1. `call_nr` 超出范围
2. `call_vec[call_nr]` 为 NULL

### 3.3 ECALLDENIED

`ECALLDENIED` 表示权限不足。

**触发条件**：`s_k_call_mask` 中对应位未设置

**安全意义**：这是内核安全的关键防线。

### 3.4 系统调用返回值

系统调用处理函数的返回值直接传递给 `kernel_call_finish`。

**常见返回值**：

| 值 | 含义 |
|----|------|
| `OK` | 成功 |
| `EINVAL` | 无效参数 |
| `ENOMEM` | 内存不足 |
| `EAGAIN` | 资源暂时不可用 |
| `EPERM` | 操作不允许 |

---

## 4. Rust 设计决策

本节讨论如何用 Rust 实现系统调用分发，重点关注类型安全和权限检查。

### 4.1 调用向量

Rust 中调用向量可以使用枚举和 match 实现，替代 C 的函数指针数组。

**C 的方式**：

```c
int (*call_vec[NR_SYS_CALLS])(struct proc *, message *);
result = (*call_vec[call_nr])(caller, msg);
```

**Rust 的方式**：

```rust
enum KernelCall {
    Fork,
    Exec,
    // ...
}

match call_nr {
    KernelCall::Fork => do_fork(caller, msg),
    KernelCall::Exec => do_exec(caller, msg),
    // ...
}
```

**优势**：编译期检查完整性，不会遗漏分支。

### 4.2 权限检查

Rust 中权限检查可以通过类型系统增强安全性。

**改进**：

1. **位图类型**：使用 `SysMap` 类型而非原始数组
2. **编译期检查**：权限检查函数返回 `Result`
3. **不可伪造**：权限对象由内核创建，用户空间无法伪造

### 4.3 错误处理

Rust 使用 `Result` 类型替代错误码。

```rust
fn dispatch(caller: &Proc, msg: &Message) -> Result<i32, KcallError> {
    let call_nr = msg.m_type - KERNEL_CALL;
    if call_nr >= NR_SYS_CALLS {
        return Err(KcallError::BadRequest);
    }
    if !caller.has_kcall_permission(call_nr) {
        return Err(KcallError::CallDenied);
    }
    call_vec[call_nr](caller, msg)
}
```

---

## 5. 实现

本节给出系统调用分发的 Rust 实现代码。

### 5.1 dispatch 函数

```rust
pub fn kernel_call_dispatch(caller: &mut Proc, msg: &mut Message) -> i32 {
    let call_nr = (msg.m_type - KERNEL_CALL) as usize;

    if call_nr >= NR_SYS_CALLS {
        log::warn!("illegal request {} from {}", call_nr, msg.m_source);
        return EBADREQUEST;
    }

    if !caller.has_kcall_permission(call_nr) {
        log::warn!("denied request {} from {}", call_nr, msg.m_source);
        return ECALLDENIED;
    }

    match CALL_VEC[call_nr] {
        Some(handler) => handler(caller, msg),
        None => {
            log::warn!("unused kernel call {} from {}", call_nr, caller.endpoint());
            EBADREQUEST
        }
    }
}
```

### 5.2 权限检查实现

```rust
impl Proc {
    pub fn has_kcall_permission(&self, call_nr: usize) -> bool {
        self.privilege().k_call_mask.get_bit(call_nr)
    }
}
```

### 5.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_bad_request() {
        let mut caller = Proc::new_for_test();
        let mut msg = Message::default();
        msg.m_type = KERNEL_CALL + NR_SYS_CALLS as i32 + 1;
        let result = kernel_call_dispatch(&mut caller, &mut msg);
        assert_eq!(result, EBADREQUEST);
    }

    #[test]
    fn test_dispatch_call_denied() {
        let mut caller = Proc::new_for_test_no_kcall();
        let mut msg = Message::default();
        msg.m_type = KERNEL_CALL;  // SYS_FORK
        let result = kernel_call_dispatch(&mut caller, &mut msg);
        assert_eq!(result, ECALLDENIED);
    }
}
```

---

## 6. 参见

- [12-kernel-call](12-kernel-call.md) - kernel_call 函数
- [14-kernel-call-finish](14-kernel-call-finish.md) - kernel_call_finish
- [09-priv-struct](09-priv-struct.md) - 特权结构体
