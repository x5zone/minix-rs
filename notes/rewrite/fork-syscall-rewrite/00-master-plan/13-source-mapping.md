# Minix3 源码文件对应关系

> 本文档记录 Minix3 C 源码与 Rust 实现的文件对应关系。

---

## PM 服务源码

| C 文件 | 功能 | Rust 文件 | 状态 |
|--------|------|-----------|------|
| `servers/pm/mproc.h` | mproc 结构体定义 | `os/servers/pm/src/mproc/mproc.rs` | ✅ 已实现 |
| `servers/pm/mproc.h` | 生命周期状态机 | `os/servers/pm/src/mproc/lifecycle.rs` | ✅ 已实现 |
| `servers/pm/mproc.h` | 阻塞状态 | `os/servers/pm/src/mproc/block.rs` | ✅ 已实现 |
| `servers/pm/mproc.h` | 等待状态 | `os/servers/pm/src/mproc/wait.rs` | ✅ 已实现 |
| `servers/pm/mproc.h` | 监护关系 | `os/servers/pm/src/mproc/guardianship.rs` | ✅ 已实现 |
| `servers/pm/mproc.h` | 追踪状态 | `os/servers/pm/src/mproc/trace.rs` | ✅ 已实现 |
| `servers/pm/mproc.h` | 信号状态 | `os/servers/pm/src/mproc/signal.rs` | ✅ 已实现 |
| `servers/pm/mproc.h` | 权限凭证 | `os/servers/pm/src/mproc/credentials.rs` | ✅ 已实现 |
| `servers/pm/forkexit.c` | fork 实现 | `os/servers/pm/src/mproc/fork.rs` | 🔧 部分实现 |
| `servers/pm/forkexit.c` | PID 生成器 | `os/servers/pm/src/mproc/pid_gen.rs` | ✅ 已实现 |
| `servers/pm/forkexit.c` | 进程表管理 | `os/servers/pm/src/mproc/table.rs` | ✅ 已实现 |
| `servers/pm/forkexit.c` | PmContext | `os/servers/pm/src/mproc/context.rs` | 🔧 部分实现 |
| `servers/pm/utility.c` | PID 生成器辅助函数 | `os/servers/pm/src/mproc/pid_gen.rs` | ✅ 已实现 |
| `servers/pm/main.c` | PM 主循环、初始化 | `os/servers/pm/src/main.rs` | 📋 占位 |
| `servers/pm/exec.c` | exec 系统调用 | `os/servers/pm/src/exec.rs` | 📋 占位 |
| `servers/pm/const.h` | PM 常量定义 | `os/libs/minix-types/src/types/pid.rs` | 🔧 部分实现 |
| `servers/pm/pm.h` | PM 主头文件 | `os/servers/pm/src/lib.rs` | ✅ 已实现 |
| `servers/pm/glo.h` | PM 全局变量 | `os/servers/pm/src/mproc/table.rs` | ✅ 已实现 |

---

## VM 服务源码

| C 文件 | 功能 | Rust 文件 | 状态 |
|--------|------|-----------|------|
| `servers/vm/vmproc.h` | vmproc 结构体定义 | `os/servers/vm/src/vmproc.rs` | 📋 未开始 |
| `servers/vm/fork.c` | VM fork 实现 | `os/servers/vm/src/fork.rs` | 📋 未开始 |

---

## VFS 服务源码

| C 文件 | 功能 | Rust 文件 | 状态 |
|--------|------|-----------|------|
| `servers/vfs/fproc.h` | fproc 结构体定义 | `os/servers/vfs/src/fproc.rs` | 📋 未开始 |

---

## Kernel 源码

| C 文件 | 功能 | Rust 文件 | 状态 |
|--------|------|-----------|------|
| `kernel/proc.h` | proc 结构体定义 | `os/kernel/src/proc.rs` | 📋 占位 |
| `kernel/proc.c` | 进程管理 | — | 📋 未开始 |
| `include/minix/endpoint.h` | Endpoint 定义 | `os/libs/minix-types/src/types/pid.rs` | ✅ 已实现 |
| `include/minix/com.h` | 通信常量 | `os/libs/minix-types/src/types/pid.rs` | ✅ 已实现 |

---

## 公共库文件映射

| Minix3 C 文件 | Rust 文件 | 说明 |
|--------------|-----------|------|
| `minix/com.h` | `os/libs/minix-types/src/messages.rs` | IPC 消息定义 |

---

## 图例说明

| 符号 | 含义 |
|------|------|
| ✅ | 已实现 |
| 🔧 | 部分实现 |
| 📋 | 占位/未开始 |
| ❌ | 未定义 |
