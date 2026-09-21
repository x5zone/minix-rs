# 其他服务

> **学习目标**: 理解其他系统服务的实现，服务间如何协作。

---

## 7.1 RS（重启服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/rs/inc.h` | ⏳ 待读 | RS 包含头文件 | - |
| 2 | `servers/rs/proto.h` | ⏳ 待读 | RS 函数原型 | - |
| 3 | `servers/rs/glo.h` | ⏳ 待读 | RS 全局变量 | - |
| 4 | `servers/rs/const.h` | ⏳ 待读 | RS 常量 | - |
| 5 | `servers/rs/type.h` | ⏳ 待读 | RS 类型定义 | - |
| 6 | `servers/rs/main.c` | ⏳ 待读 | RS 主循环 | - |
| 7 | `servers/rs/manager.c` | ⏳ 待读 | 服务管理 | - |
| 8 | `servers/rs/exec.c` | ⏳ 待读 | 服务执行 | - |
| 9 | `servers/rs/error.c` | ⏳ 待读 | 错误处理 | - |
| 10 | `servers/rs/utility.c` | ⏳ 待读 | 工具函数 | - |
| 11 | `servers/rs/update.c` | ⏳ 待读 | 热更新 | - |
| 12 | `servers/rs/request.c` | ⏳ 待读 | 请求处理 | - |
| 13 | `servers/rs/table.c` | ⏳ 待读 | 表管理 | - |

---

## 7.2 Sched（调度服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/sched/sched.h` | ⏳ 待读 | 调度器头文件 | - |
| 2 | `servers/sched/schedproc.h` | ⏳ 待读 | 调度进程结构 | - |
| 3 | `servers/sched/proto.h` | ⏳ 待读 | 调度函数原型 | - |
| 4 | `servers/sched/main.c` | ⏳ 待读 | 调度器主循环 | - |
| 5 | `servers/sched/schedule.c` | ⏳ 待读 | 调度实现 | - |

---

## 7.3 DS（数据存储服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/ds/inc.h` | ⏳ 待读 | DS 包含头文件 | - |
| 2 | `servers/ds/proto.h` | ⏳ 待读 | DS 函数原型 | - |
| 3 | `servers/ds/store.h` | ⏳ 待读 | 存储头文件 | - |
| 4 | `servers/ds/main.c` | ⏳ 待读 | DS 主循环 | - |
| 5 | `servers/ds/store.c` | ⏳ 待读 | 数据存储 | - |

---

## 7.4 IPC 服务器（System V IPC）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/ipc/inc.h` | ⏳ 待读 | IPC 服务器包含头文件 | - |
| 2 | `servers/ipc/main.c` | ⏳ 待读 | IPC 服务器主循环 | - |
| 3 | `servers/ipc/shm.c` | ⏳ 待读 | 共享内存 | - |
| 4 | `servers/ipc/sem.c` | ⏳ 待读 | 信号量 | - |
| 5 | `servers/ipc/utility.c` | ⏳ 待读 | 工具函数 | - |

---

## 7.5 IS（信息服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/is/inc.h` | ⏳ 待读 | IS 包含头文件 | - |
| 2 | `servers/is/proto.h` | ⏳ 待读 | IS 函数原型 | - |
| 3 | `servers/is/glo.h` | ⏳ 待读 | IS 全局变量 | - |
| 4 | `servers/is/main.c` | ⏳ 待读 | IS 主循环 | - |
| 5 | `servers/is/dmp.c` | ⏳ 待读 | 转储实现 | - |
| 6 | `servers/is/dmp_pm.c` | ⏳ 待读 | PM 转储 | - |
| 7 | `servers/is/dmp_kernel.c` | ⏳ 待读 | 内核转储 | - |
| 8 | `servers/is/dmp_fs.c` | ⏳ 待读 | FS 转储 | - |
| 9 | `servers/is/dmp_vm.c` | ⏳ 待读 | VM 转储 | - |
| 10 | `servers/is/dmp_rs.c` | ⏳ 待读 | RS 转储 | - |
| 11 | `servers/is/dmp_ds.c` | ⏳ 待读 | DS 转储 | - |

---

## 7.6 MIB（管理信息库）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/mib/mib.h` | ⏳ 待读 | MIB 头文件 | - |
| 2 | `servers/mib/main.c` | ⏳ 待读 | MIB 主循环 | - |
| 3 | `servers/mib/proc.c` | ⏳ 待读 | 进程相关 | - |
| 4 | `servers/mib/minix.c` | ⏳ 待读 | Minix 相关 | - |
| 5 | `servers/mib/hw.c` | ⏳ 待读 | 硬件相关 | - |
| 6 | `servers/mib/remote.c` | ⏳ 待读 | 远程相关 | - |
| 7 | `servers/mib/kern.c` | ⏳ 待读 | 内核相关 | - |
| 8 | `servers/mib/tree.c` | ⏳ 待读 | 树管理 | - |
| 9 | `servers/mib/vm.c` | ⏳ 待读 | VM 相关 | - |

---

## 7.7 Devman（设备管理服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/devman/proto.h` | ⏳ 待读 | Devman 函数原型 | - |
| 2 | `servers/devman/devman.h` | ⏳ 待读 | Devman 头文件 | - |
| 3 | `servers/devman/devinfo.h` | ⏳ 待读 | 设备信息头文件 | - |
| 4 | `servers/devman/main.c` | ⏳ 待读 | Devman 主循环 | - |
| 5 | `servers/devman/device.c` | ⏳ 待读 | 设备管理 | - |
| 6 | `servers/devman/bind.c` | ⏳ 待读 | 绑定实现 | - |
| 7 | `servers/devman/buf.c` | ⏳ 待读 | 缓冲区管理 | - |

---

## 7.8 Input（输入服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/input/input.h` | ⏳ 待读 | 输入头文件 | - |
| 2 | `servers/input/input.c` | ⏳ 待读 | 输入实现 | - |

---

## 核心概念总结

### 服务分类
- **RS**：服务管理、热更新、重启
- **Sched**：用户态调度器
- **DS**：数据存储、发布订阅
- **IPC**：System V IPC（共享内存、信号量）
- **IS**：系统信息转储
- **MIB**：管理信息库（sysctl）
- **Devman**：设备管理
- **Input**：输入设备管理

---

## 进度统计

| 服务 | 已读 | 待读 | 覆盖率 |
|------|------|------|--------|
| RS | 0 | 13 | 0% |
| Sched | 0 | 5 | 0% |
| DS | 0 | 5 | 0% |
| IPC | 0 | 5 | 0% |
| IS | 0 | 11 | 0% |
| MIB | 0 | 9 | 0% |
| Devman | 0 | 7 | 0% |
| Input | 0 | 2 | 0% |
| **总计** | **0** | **57** | **0%** |
