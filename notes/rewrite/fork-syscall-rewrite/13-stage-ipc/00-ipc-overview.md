# 00-ipc-overview: IPC server 总览

> **状态**: 已定稿（2026-09-16 改写，随实施轮收口）
> **定位**: 阶段 0 总览（导航）
> **源码**: `minix3/minix/servers/ipc/`（4 个 .c，1690 行）
> **Rust 模块**: `os/servers/ipc-server/`（判定层 + 服务层已落地，17+1 个源文件；生产传输接线挂 edge E-IPCWIRE）
> **draft 素材**: `draft/README.md`（占位）

## 1 这个服务是什么

IPC server 是 System V 信号量（semget/semctl/semop）与共享内存（shmget/shmat/shmdt/shmctl）的**用户态对象管理服务**：集合表、段表、权限、等待队列、引用计数都住在它的地址空间里，内核不掺和任何一份 SysV 语义。应用程序的 semop(2) 一路走 libc 与消息传输入口，最终变成一条发到本服务的消息；本服务查表、算权限、改状态，再决定"立即回信"还是"挂起等条件"。这个定位与 Redox 把 POSIX 设施推向用户态守护进程的路线同构——对象管理与策略在用户态，内核只保留收发原语。

**与内核 IPC 机制的边界**：send/receive/notify 这些原语本身是 `../01-stage-kernel/12-ipc-core.md` 与 `notes/rewrite/ipc-sendrec.md` 的话题，本 stage 只做原语之上的对象管理。crate 名叫 `ipc-server`，正是为了不和内核 IPC 撞名。

**在系统里的位置**：不在 boot_image（`kernel/table.c` 没有 ipc 条目），由 RS 运行时按 `ipc.conf` 声明的特权面加载——它能 UMAP/VIRCOPY，能收若干端点消息，VM 给了它 REMAP/GETPHYS/GETREF 四样访存动词。

## 2 两条主线

**启动主线**（plan §1.2）：RS 加载进程 → `main` 里 `sef_local_startup` 注册三路 SEF 回调 → `sef_cb_init_fresh` 挂载 kern.ipc 远程 MIB 子树 → 进入主循环。主循环每轮五件事：内核通知忽略、PM 的进程事件转给信号量等待者、MIB 请求进子树分发、七个 SysV 调用查 `call_vec` 分发、回信与收尾清拍。SIGTERM 到来时看两张表是否全空——空则注销子树退出，不空则警告留下。这条线的细节在 01（循环与分派）、03（MIB 子树）、10（生命周期）。

**调用旅程次主线**（plan §1.3）：libc 打包 `mess_lc_ipc_*` 消息（02 篇协议面）→ 本服务 dispatch → 信号量语义（05 表、06 操作）或共享内存语义（07 段、08 挂接与清拍）→ 每一步先过 04 篇的权限模型 → 需要内存动作找 VM、进程生死找 PM、管理面查询出 kern.ipc 子树。99 篇是这趟旅程的常量与错误码字典。

## 3 文档地图

| 篇 | 内容 | 主线位置 |
|---|---|---|
| 01 | SEF 启动、主循环五分支、传输缝 | 启动主线 |
| 02 | IPC_BASE 协议面：七个调用号、消息结构、ipc.conf 特权 | 两线地基 |
| 03 | kern.ipc 远程 MIB 子树 | 主循环 MIB 分支 |
| 04 | check_perm 权限模型与各调用点掩码 | 每个调用的门 |
| 05 | 信号量集合表：semget/semctl/删除/seminfo | 调用旅程 |
| 06 | semop 原子性：乐观试做、挂起、三种叫醒 | 调用旅程 |
| 07 | shmget 与段创建 | 调用旅程 |
| 08 | shmat/shmdt/shmctl 与惰性引用计数 | 调用旅程 |
| 09 | PM 进程事件：订阅边沿、分诊、回执 | 主循环事件分支 |
| 10 | SIGTERM、干净退出、重启语义 | 收口 |
| 99 | 常量、错误码、标识符编码、边界契约 | 字典 |

## 4 Rust 侧现状（2026-09-16）

判定与编排分两层。判定层是纯函数与状态容器：`sem/`（表、ctl、op、waiter）、`shm/`（segment、attach、refcount）、`perms.rs`、`events.rs`、`lifecycle.rs`、`mib_tree.rs`、`dispatch.rs`——所有语义判断都在这里，可无内核单测。服务层 `service.rs` 的 `IpcService` 按篇序把判定模块串成七个调用的完整入口，跨服务效应（时钟、凭证、拷贝、订阅、唤醒、VM 动词）经 `IpcBoundary` 动词出边界，测试用 `TestBoundary` 替身驱动。事件循环 `server.rs` 只管路由与回信判决。当前测试规模：单元 100 + 集成 4，全过；`main.rs` 的生产接线（真实传输与边界实现）挂 edge E-IPCWIRE，接线清单写在它的注释里。

## 5 边界

- **前置依赖**: 无（导航页）。
- **不覆盖（移交）**: 一切机制细节（01~10、99）；内核 IPC 原语见上文边界说明。
