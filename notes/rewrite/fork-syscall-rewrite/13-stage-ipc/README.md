# 13-stage-ipc — IPC 文档目录

> **状态**: 骨架就绪（plan.md 定稿 2026-08-16；各 doc 为最小骨架，待按 plan.md 改写）
> **主线**: IPC server 启动顺序（RS 加载 → SEF init → kern.ipc MIB 子树注册 → 主循环 dispatch）；SysV IPC 调用旅程为次主线
> **Ground truth**: `minix3/minix/servers/ipc/`（4 个 .c，1690 行）
> **Rust**: `os/servers/ipc-server/`（当前为空壳 stub）

## 概念边界

**IPC server ≠ kernel IPC 机制**：本 stage 是 SysV 信号量（semget/semctl/semop）与共享内存（shmget/shmat/shmdt/shmctl）的**用户态对象管理服务**；kernel 内 send/receive/notify 原语见 `01-stage-kernel/12-ipc-core.md` 与 `notes/rewrite/ipc-sendrec.md`（不属本 stage）。

## 文档清单（12 篇）

| 编号 | 文档 | 语义模块 |
|------|------|---------|
| 00 | `00-ipc-overview.md` | 总览：IPC server 是什么、kernel IPC 边界、启动主线图、SysV 调用旅程次主线、导航 |
| 01 | `01-ipc-init-main.md` | main()/SEF 注册、主循环消息分类、call_vec 分发、SUSPEND 回复、notify 忽略 |
| 02 | `02-ipc-message-contract.md` | 协议面：IPC_BASE 0xD00、7 call numbers、7 种消息结构、PROC_EVENT、ipc.conf 特权面（A-1） |
| 03 | `03-ipc-mib-registration.md` | kern.ipc 远程 MIB 子树：注册/分派/rmib 客户端契约（A-6） |
| 04 | `04-ipc-permissions.md` | check_perm/prepare_mib_perm：SysV 权限模型与掩码调用点 |
| 05 | `05-ipc-sem-table.md` | 信号量集合表与生命周期：semget/semctl/remove_set/fill_seminfo/get_sem_mib_info |
| 06 | `06-ipc-semop.md` | semop 原子性与等待队列：try_semop/check_set/iproc/sem_process_event（A-2） |
| 07 | `07-ipc-shm-segment.md` | 共享内存段创建：shmget/vm_getphys（A-9） |
| 08 | `08-ipc-shm-attach.md` | 挂接与引用计数：shmat/shmdt/shmctl/update_refcount_and_destroy（A-3 决策点） |
| 09 | `09-ipc-proc-events.md` | PM 进程事件：订阅掩码/got_proc_event/PROC_EVENT_REPLY |
| 10 | `10-ipc-lifecycle.md` | 生命周期：SIGTERM 清理/干净退出判定/重启语义 |
| 99 | `99-ipc-global-concepts.md` | 常量/errno 特殊语义/IPCID 编码/跨服务引用收口 |

## 关键文件

- `plan.md` — 文档重组计划（定稿，含覆盖契约 §5 + ARCH 清单 §4 + 双轮 review 记录 §7）
- `draft/` — 旧占位 README（素材）
- `checklist.md` — 函数级基线（实现期创建，参照 `02-stage-vm/checklist.md` 模式）

## 启动链路位置

```
kernel → VM → RS → PM/SCHED/VFS/DS/MIB → IS/DEVMAN/INPUT/IPC → INIT（boot 终点）
```

IPC 不在 boot_image（`kernel/table.c` 无 ipc 条目），由 RS 运行时加载（ipc.conf 定义特权面），与 IS/devman/input 同属 RS 加载组。
