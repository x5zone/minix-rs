# 15-do-fork-validate - do_fork 参数验证

> 本文档分析 `minix3/minix/kernel/system/do_fork.c` 第 1-50 行，讲解 do_fork 函数的参数验证部分。

---

## 1. 概述

`do_fork` 的参数验证确保 fork 请求的合法性，防止无效参数导致内核状态损坏。

### 1.1 SYS_FORK 系统调用

`SYS_FORK` 是内核系统调用，由 PM 进程调用，请求内核为子进程创建进程表条目。

**调用者**：PM（进程管理器）
**参数**：父进程端点、子进程槽位号、fork 标志

### 1.2 参数验证重要性

参数验证的重要性：

1. **安全边界**：内核不信任用户空间传入的参数
2. **状态一致性**：无效参数可能导致进程表损坏
3. **防御性编程**：防止 PM 错误导致内核崩溃

---

## 2. C 源码分析

本节详细分析 `do_fork` 函数的参数验证代码（`do_fork.c` 第 1-50 行）。

### 2.1 文件头部注释

文件头部注释说明了系统调用的参数：

```c
/* m_lsys_krn_sys_fork.endpt  - 父进程端点 */
/* m_lsys_krn_sys_fork.slot  - 子进程槽位号 */
/* m_lsys_krn_sys_fork.flags - fork 标志 */
/* m_krn_lsys_sys_fork.endpt - 子进程端点（输出） */
/* m_krn_lsys_sys_fork.msgaddr - 子进程新内存映射（输出） */
```

#### 2.1.1 系统调用说明

`SYS_FORK` 是内核提供的进程创建系统调用，由 PM 调用来请求内核为子进程创建进程表条目。

#### 2.1.2 参数说明

`m_lsys_krn_sys_fork` 是 PM → 内核方向的 fork 消息字段：

| 字段 | 类型 | 含义 |
|------|------|------|
| `endpt` | `endpoint_t` | 父进程端点 |
| `slot` | `int` | 子进程槽位号 |
| `flags` | `int` | fork 标志 |

##### 2.1.2.1 endpt 字段

`endpt` 字段指定被 fork 的父进程端点。

**来源**：PM 在处理用户 fork 请求时设置
**验证**：通过 `isokendpt` 验证有效性

##### 2.1.2.2 slot 字段

`slot` 字段指定预分配的子进程槽位号。

**来源**：PM 在进程表中找到空闲槽位后设置
**验证**：通过 `isemptyp(rpc)` 验证槽位确实空闲

##### 2.1.2.3 flags 字段

`flags` 字段包含 fork 的选项标志。

**可能的值**：

- 0：普通 fork
- 其他：特定平台的 fork 变体标志

#### 2.1.3 返回值说明

`m_krn_lsys_sys_fork` 是内核 → PM 方向的 fork 回复字段：

| 字段 | 类型 | 含义 |
|------|------|------|
| `endpt` | `endpoint_t` | 子进程端点 |
| `msgaddr` | `vir_bytes` | 子进程新内存映射地址 |

##### 2.1.3.1 endpt 字段

返回的 `endpt` 是子进程的端点号，由内核在 fork 完成后设置。

##### 2.1.3.2 msgaddr 字段

返回的 `msgaddr` 是子进程的新内存映射地址，用于 VM 设置子进程地址空间。

### 2.2 头文件包含

头文件包含提供了必要的类型定义和函数声明。

#### 2.2.1 kernel/system.h

`kernel/system.h` 包含内核系统调用处理的核心定义：

- `call_vec` 声明
- 系统调用处理函数原型
- 内核常量和宏定义

#### 2.2.2 kernel/vm.h

`kernel/vm.h` 包含 VM 相关的函数声明，用于 fork 中的内存操作。

#### 2.2.3 minix/endpoint.h

`minix/endpoint.h` 包含端点相关的宏定义：

- `_ENDPOINT_G`：提取 generation
- `_ENDPOINT_P`：提取 slot
- `isokendpt` 等验证宏

### 2.3 USE_FORK 条件编译

`USE_FORK` 控制是否编译 fork 系统调用处理代码。

```c
#if USE_FORK
int do_fork(struct proc * caller, message * m_ptr) { ... }
#endif
```

**作用**：允许在不需要 fork 的配置中排除此代码。

### 2.4 函数签名

```c
int do_fork(struct proc *caller, message *m_ptr)
```

| 参数 | 类型 | 含义 |
|------|------|------|
| `caller` | `struct proc *` | 调用者（PM）进程指针 |
| `m_ptr` | `message *` | 系统调用消息 |

**返回值**：`int`，OK 表示成功，错误码表示失败

#### 2.4.1 caller 参数

`caller` 是发起 SYS_FORK 的进程（通常是 PM）。

**注意**：`caller` 不是被 fork 的父进程，而是 PM 进程。父进程通过消息中的端点参数指定。

#### 2.4.2 m_ptr 参数

`m_ptr` 包含 fork 系统调用的参数：

1. `m_lsys_krn_sys_fork.endpt`：被 fork 的父进程端点
2. `m_lsys_krn_sys_fork.slot`：预分配的子进程槽位号
3. `m_lsys_krn_sys_fork.flags`：fork 标志

#### 2.4.3 返回值

返回值表示 fork 操作的结果：

| 值 | 含义 |
|----|------|
| `OK` | fork 成功 |
| `EINVAL` | 参数无效 |

### 2.5 局部变量声明

局部变量声明：

```c
char *old_fpu_save_area_p;  // 旧 FPU 保存区指针
register struct proc *rpc;  // 子进程指针（寄存器变量）
struct proc *rpp;           // 父进程指针
int gen;                    // generation 号
int p_proc;                 // 父进程槽位号
int namelen;                // 名称长度
```

#### 2.5.1 rpc 变量

`rpc` 是子进程的进程结构指针，使用 `register` 关键字优化访问速度。

#### 2.5.2 rpp 变量

`rpp` 是父进程的进程结构指针。

#### 2.5.3 gen 变量

`gen` 保存子进程端点的 generation 号，用于构造新的端点值。

#### 2.5.4 p_proc 变量

`p_proc` 保存从父进程端点提取的槽位号。

### 2.6 端点验证

端点验证确保父进程端点有效：

```c
if(!isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc))
    return EINVAL;
```

#### 2.6.1 isokendpt 函数

`isokendpt` 验证父进程端点并提取槽位号到 `p_proc`。

#### 2.6.2 EINVAL 错误

`EINVAL` 表示参数无效，是 fork 验证失败时最常用的错误码。

### 2.7 进程指针获取

进程指针获取通过槽位号定位进程结构：

```c
rpp = proc_addr(p_proc);
rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);
```

#### 2.7.1 proc_addr 宏

`proc_addr(p_proc)` 通过槽位号获取进程结构指针：`&proc[NR_TASKS + p_proc]`。

#### 2.7.2 rpp 赋值

`rpp` 获取父进程指针，后续用于读取父进程状态和复制到子进程。

### 2.8 子进程槽验证

子进程槽验证确保父进程存在且子进程槽位空闲：

```c
if (isemptyp(rpp) || !isemptyp(rpc)) return(EINVAL);
```

#### 2.8.1 rpc 赋值

`rpc` 获取子进程指针，PM 已预分配此槽位。

#### 2.8.2 isemptyp 检查

`isemptyp(rpc)` 检查子进程槽位是否空闲（`RTS_SLOT_FREE`）。

##### 2.8.2.1 父进程检查

`isemptyp(rpp)` 检查父进程槽位是否空闲。如果空闲，说明端点指向不存在的进程。

##### 2.8.2.2 子进程槽空闲检查

`!isemptyp(rpc)` 检查子进程槽位是否已被占用。如果已占用，PM 分配了错误的槽位。

#### 2.8.3 EINVAL 错误

验证失败返回 `EINVAL`，PM 收到此错误后会向用户进程返回 fork 失败。

### 2.9 消息投递检查

消息投递检查确保父进程没有待传递的消息：

```c
assert(!(rpp->p_misc_flags & MF_DELIVERMSG));
```

#### 2.9.1 MF_DELIVERMSG 检查

此断言确保父进程没有待传递的消息。

**含义**：

- `MF_DELIVERMSG` 表示有消息等待传递
- fork 时父进程不应有未处理的消息
- 如果断言失败，说明内核状态不一致

#### 2.9.2 断言含义

父进程不能有待投递消息的原因：

1. **状态一致**：待投递消息表示有异步消息未处理
2. **复制安全**：fork 复制进程结构时，待投递消息状态可能导致不一致
3. **协议保证**：PM 同步调用 SYS_FORK 时，父进程应处于干净状态

### 2.10 接收状态检查

接收状态检查确保父进程正在等待接收：

```c
if(!RTS_ISSET(rpp, RTS_RECEIVING)) {
    printf("kernel: fork not done synchronously?\n");
    return EINVAL;
}
```

#### 2.10.1 RTS_RECEIVING 检查

`RTS_ISSET(rpp, RTS_RECEIVING)` 检查父进程是否在接收状态。

**含义**：

1. **同步要求**：fork 必须在 PM 同步调用时执行
2. **消息缓冲区**：接收状态保证父进程有消息缓冲区
3. **安全保证**：确保 fork 期间父进程不会执行其他操作

#### 2.10.2 同步 fork 要求

fork 必须同步进行：PM 通过 sendrec 调用 SYS_FORK，此时父进程处于 RTS_RECEIVING 状态。

#### 2.10.3 错误日志

错误日志输出 "kernel: fork not done synchronously?"，提示 fork 未同步执行。

#### 2.10.4 EINVAL 错误

验证失败返回 `EINVAL`，PM 收到此错误后会向用户进程返回 fork 失败。

---

## 3. 验证流程图

本节绘制 `do_fork` 参数验证的完整流程图。

### 3.1 验证步骤

```
isokendpt(endpt, &p_proc)
       ├── 失败 → EINVAL
       ▼
proc_addr(p_proc) → rpp
proc_addr(slot) → rpc
       ▼
isemptyp(rpp)? → EINVAL
!isemptyp(rpc)? → EINVAL
assert(!MF_DELIVERMSG)
RTS_ISSET(rpp, RTS_RECEIVING)? → EINVAL
save_fpu(rpp)
       ▼
验证通过
```

### 3.2 错误路径

```
isokendpt 失败 → EINVAL (端点无效)
isemptyp(rpp) → EINVAL (父进程不存在)
!isemptyp(rpc) → EINVAL (子进程槽位已占用)
!RTS_RECEIVING → EINVAL (非同步 fork)
```

---

## 4. Rust 设计决策

本节讨论如何用 Rust 实现 `do_fork` 参数验证。

### 4.1 参数类型安全

Rust 通过强类型确保参数安全：

1. **Endpoint 类型**：替代原始 `int`
2. **ProcRef 类型**：替代原始指针
3. **Result 返回**：替代错误码

### 4.2 验证函数

Rust 验证函数返回 `Result` 类型，强制处理错误：

```rust
fn validate_fork_params(msg: &Message) -> Result<(ProcRef, ProcRef), ForkError>
```

### 4.3 错误处理

Rust 使用 `Result` 和错误枚举替代错误码：

```rust
enum ForkError {
    InvalidEndpoint,
    ParentNotFound,
    ChildSlotOccupied,
    NotSynchronous,
    DeliverMsgPending,
}
```

---

## 5. 实现

本节给出 `do_fork` 参数验证的 Rust 实现代码。

### 5.1 do_fork 函数签名

```rust
pub fn do_fork(caller: &mut Proc, msg: &Message) -> Result<i32, ForkError>
```

### 5.2 验证逻辑实现

```rust
fn validate_fork_params(msg: &Message) -> Result<(ProcRef, ProcRef), ForkError> {
    let p_proc = isokendpt(msg.fork_endpt()).ok_or(ForkError::InvalidEndpoint)?;
    let rpp = proc_addr(p_proc);
    let rpc = proc_addr(msg.fork_slot());
    if rpp.is_empty() { return Err(ForkError::ParentNotFound); }
    if !rpc.is_empty() { return Err(ForkError::ChildSlotOccupied); }
    if !rpp.rts_flags().contains(RtsFlags::RECEIVING) {
        return Err(ForkError::NotSynchronous);
    }
    Ok((rpp, rpc))
}
```

### 5.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_invalid_endpoint() {
        let caller = Proc::new_for_test();
        let msg = Message::new_fork(Endpoint::NONE, 0);
        let result = do_fork_validate(&caller, &msg);
        assert_eq!(result, Err(ForkError::InvalidEndpoint));
    }

    #[test]
    fn test_validate_parent_not_found() {
        let caller = Proc::new_for_test();
        let msg = Message::new_fork(Endpoint::new(9999), 0);
        let result = do_fork_validate(&caller, &msg);
        assert!(matches!(result, Err(ForkError::ParentNotFound) | Err(ForkError::InvalidEndpoint)));
    }
}
```

---

## 6. 参见

- [06-proc-rts-flags](06-proc-rts-flags.md) - RTS 标志位
- [08-proc-macros](08-proc-macros.md) - 进程访问宏
- [16-do-fork-copy](16-do-fork-copy.md) - 进程结构复制
