# 用户层 fork 系统调用接口

> 本文档描述用户态 libc/libsys 中 fork 系统调用的接口层实现，以及集成阶段如何将其纳入完整的端到端路径。

---

## 1. 概述

### 1.1 lib 层的角色

- TODO: 说明 fork 系统调用在用户空间的入口——libc 封装
- TODO: 说明 lib 层逻辑极简：构造消息 + ipc_sendrec → PM
- TODO: 说明集成阶段需要搭建 lib 接口架子，使整个 fork 路径从用户态到内核态完整闭环

### 1.2 涉及的源码

| C 源文件 | 路径 | 职责 |
|---------|------|------|
| `fork.c` | `minix3/minix/lib/libc/sys/fork.c` | fork() 用户态入口 |
| `syscall.c` | `minix3/minix/lib/libc/sys/syscall.c` | _syscall() 通用 IPC 封装 |
| `ipc.h` | `minix3/minix/include/minix/ipc.h` | ipc_sendrec 内联实现 |

---

## 2. fork() 用户态实现

### 2.1 C 源码

- TODO: 分析 `fork()` 函数的完整实现：
  ```c
  pid_t fork(void) {
      message m;
      memset(&m, 0, sizeof(m));
      return(_syscall(PM_PROC_NR, PM_FORK, &m));
  }
  ```

### 2.2 _syscall() 封装

- TODO: 分析 `_syscall()` 函数的实现：
  ```c
  int _syscall(endpoint_t who, int syscallnr, message *msgptr) {
      msgptr->m_type = syscallnr;
      status = ipc_sendrec(who, msgptr);
      // ... 错误处理与返回值转换 ...
  }
  ```

### 2.3 ipc_sendrec() 内核原语

- TODO: 分析 `ipc_sendrec()` 的实现——触发 trap 指令进入内核
- TODO: 说明内核路由 IPC 消息到 PM 的过程
- TODO: 说明这是内核提供的 IPC 原语，不属于 lib 逻辑

---

## 3. Rust 实现设计

### 3.1 sys_fork() 接口

- TODO: 设计 Rust 版 fork 系统调用入口：
  ```rust
  pub fn sys_fork() -> Result<Pid, SyscallError> {
      let mut msg = Message::zeroed();
      msg.m_type = PmCall::Fork as u32;
      ipc_sendrec(PM_PROC_NR, &mut msg)?;
      // 转换返回值
  }
  ```

### 3.2 _syscall 通用封装

- TODO: 设计 Rust 版通用系统调用封装：
  ```rust
  pub fn syscall(who: Endpoint, callnr: u32, msg: &mut Message) -> Result<i32, IpcError>
  ```

### 3.3 Message 类型

- TODO: 说明 `minix-types` crate 中已有的 Message 类型定义
- TODO: 说明 fork 消息使用空消息体（m_type 之外无额外字段）

---

## 4. 集成阶段的工作

### 4.1 接口架子

- TODO: 说明集成阶段需要搭建的 lib 接口：
  - `sys_fork()` 函数——用户态入口
  - `ipc_sendrec()` mock——模拟内核 IPC 路由
  - 返回值处理——PM 回复消息 → pid_t 转换

### 4.2 端到端路径

- TODO: 说明集成后完整的端到端路径：
  ```
  User: fork()
    → sys_fork()
    → ipc_sendrec(PM_PROC_NR, PM_FORK)
    → Kernel IPC 路由
    → PM: do_fork()
    → VM → Kernel → VFS → SCHED
    → PM 回复
    → User: pid (parent) / 0 (child)
  ```

### 4.3 Mock 策略

- TODO: 说明 lib 层的 Mock 策略：
  - `ipc_sendrec()` mock——不真正 trap，直接调用 PM do_fork()
  - 或者通过集成测试框架的消息传递机制
  - 用户态不需要 Mock 其他组件——IPC 路由由集成框架处理

---

## 5. fork 返回值处理

### 5.1 父进程返回值

- TODO: 分析 PM 回复父进程时设置的消息字段
- TODO: 说明 `m_pm_lc_fork.pid` 字段包含子进程 PID
- TODO: 说明 libc 将 PID 转换为 `pid_t` 返回

### 5.2 子进程返回值

- TODO: 分析内核如何设置子进程的返回值为 0
- TODO: 说明 `p_reg.retreg = 0` 的内核伪造机制
- TODO: 说明在 Mock 环境下如何模拟子进程返回 0

---

## 6. 与其他系统调用的关系

### 6.1 fork + exec 模式

- TODO: 说明 fork 后通常紧跟 exec 的 Unix 惯例
- TODO: 说明 lib 层 fork 接口为未来 exec 实现预留的设计空间

### 6.2 vfork (如需要)

- TODO: 说明 Minix3 是否支持 vfork
- TODO: 说明 vfork 与 fork 在 lib 层的差异

---

## 7. 参见

- [cross-service-msg.md](cross-service-msg.md) - 跨服务消息协议
- [integration-tests.md](integration-tests.md) - 集成测试
- [suspend-wakeup.md](suspend-wakeup.md) - SUSPEND 与唤醒机制
