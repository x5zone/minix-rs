# 99-ipc-global-concepts: 全局概念与常量收口

> **状态**: 已定稿（2026-09-16 改写，随实施轮 IPC-P1-1 的边界契约一并收口）
> **定位**: 99 全局概念——常量、错误码、标识符编码、以及各篇共享的边界行为契约
> **源码**: `minix3/sys/sys/ipc.h`、`minix3/sys/sys/sem.h`、`minix3/sys/sys/shm.h`、`minix3/minix/include/minix/com.h`、`minix3/sys/sys/errno.h`
> **Rust 模块**: `os/libs/minix-types/src/ipc/ipc_server.rs`（常量与调用面）、`os/servers/ipc-server/src/sem/mod.rs` + `shm/mod.rs`（错误枚举）、`os/servers/ipc-server/src/service.rs`（边界契约的承载处）

这一篇是查阅页，不是叙事页：五张表加一组跨篇契约，各篇正文引用到这里就不必重复数值。数值全部以 C 头文件为锚点；Rust 侧的对应物在括号里给名字。数值若与本篇冲突，以 C 头文件为准再改本篇。

## 1 常量与限制

### 1.1 信号量（sys/sem.h）

| 常量 | 值 | 行号 | 落点 |
|---|---|---|---|
| `SEMVMX` | 32767 | sem.h:93 | 任何信号量的最大值；超出回超范围（`try_ops` 正操作、`SETVAL`/`SETALL` 设值都查它） |
| `SEMAEM` | 16384 | sem.h:94 | 撤销上限（本服务不支持撤销，数值保留在 seminfo 输出里） |
| `SEM_UNDO` | 010000 | sem.h:76 | 撤销标志：**一票否决**——任一操作带上它，整个 semop 回参数错误（排除契约，06 篇 2.2） |
| `SEMMNI` | 10 | sem.h:164 | 集合表槽位数（`table.rs` 的数组长度） |
| `SEMMNS` | 60 | sem.h:167 | 头文件的系统级信号量数——注意 seminfo 报的不是它 |
| `SEMMSL` | `SEMMNS`（60） | sem.h:181 | 每集合信号量上限（`SemSet::sems` 数组长度、semget 数量检查） |
| `SEMOPM` | 100 | sem.h:184 | 单次 semop 操作数上限，超出回单次太多 |
| `SEMUME`/`SEMMNU`/`SEMMAP` | 10/30/30 | sem.h:170/:173/:178 | 撤销机制参数：本服务不支持撤销，seminfo 的 undo 三字段恒零（A-7 排除契约） |
| `SEM_STAT`/`SEM_INFO` | 18/19 | sem.h:215-216 | 按槽下标读取 / 模块明细（ipcs 用） |
| `SEM_ALLOC` | 01000 | sem.h:154 | 槽位占用状态位：权限位掩膜替换时它必须活下来（IPC_SET 的 `~ACCESSPERMS` 掩膜） |

一个容易踩的坑写在脚注里：`struct seminfo` 的 `semmns` 字段报的是 `SEMMNI × SEMMSL`（10 × 60 = 600，sem.c:437 附近的计算），不是头文件那个 60。Rust 侧 `fill_info` 用 `(SEMMNI * SEMMSL)` 同式计算，测试 `ctl_info_differs` 钉住。

### 1.2 共享内存（sys/shm.h 与 shm.c 私有位）

| 常量 | 值 | 行号 | 落点 |
|---|---|---|---|
| `SHMMNI` | 1024 | shm.h:218 | 段表槽位数（`ShmTable` 的数组长度） |
| `SHMSEG` | 32 | shm.h:219 | 每进程段上限（本服务未实施逐进程计数，数值仅在 shminfo 语境） |
| `SHM_ALLOC` | 0x0800 | shm.c:4 | 槽位占用位（C 定义在 shm.c 而非头文件——Rust 放在 minix-types） |
| `SHM_DEST` | 0x0400 | shm.h:222 | 删除标记：置位后等挂接数归零才真正销毁（08 篇的惰性清拍） |
| `SHM_RDONLY`/`SHM_RND` | 010000/010000 类 | sys/shm.h:76 附近 | 只读挂接 / 地址取整标志（04 篇挂接掩码、08 篇对齐） |

### 1.3 权限与命令（sys/ipc.h）

| 常量 | 值 | 行号 | 落点 |
|---|---|---|---|
| `IPC_R`/`IPC_W`/`IPC_M` | 0400/0200/010000 | sys/ipc.h:54-92 | 读、写、控制三位（04 篇掩码表的原料；`ACCESSPERMS` 掩膜保留九位权限） |
| `IPC_CREAT`/`IPC_EXCL`/`IPC_NOWAIT` | 01000/02000/04000 | sys/ipc.h:54-92 | 创建、排他、非阻塞 |
| `IPC_PRIVATE` | 0 | sys/ipc.h:54-92 | 私有键：semget 先分流、shmget 永不命中 |
| `IPC_RMID`/`IPC_SET`/`IPC_STAT` | 0/1/2 | sys/ipc.h:54-92 | 删除、改属性、读状态 |
| `IPC_INFO` | 500 | ipc.h:101 | 模块概要（ipcs 用） |
| `IXSEQ_TO_IPCID` | 宏 | ipc.h:110 | 低十六位槽号、高十六位序号拼标识符；`IPCID_TO_IX`/`IPCID_TO_SEQ` 反解（inc.h:41-42） |

## 2 错误码的特殊语义

普通 errno（`EINVAL`/`EACCES`/`EPERM`/`EEXIST`/`ENOENT`/`ENOSPC`/`ERANGE`/`E2BIG`/`EFBIG`/`ENOMEM`/`EAGAIN`/`EOPNOTSUPP`=45）沿用各自的标准含义，`SemError`/`ShmError` 的 `to_errno` 一一映射（sem/mod.rs、shm/mod.rs），这里不重复。三个要单独讲：

| 标记 | 值 | 行号 | 语义 |
|---|---|---|---|
| `SUSPEND` | -998 | com.h:1151 | "调用者留在原地，以后再回信"。主循环见到它不回信（dispatch 篇的 `should_reply`）；semop 挂起即此值。它不是错误，是协议里的一次约定 |
| `EDONTREPLY` | 203（伪码） | errno.h:199 | "调用者已经不在了，什么都别发"。进程退出取消等待时用它抑制回信；Rust 侧是 `NO_REPLY`（sem/mod.rs），边界 `send_wakeup` 见到即吞掉 |
| `EIDRM` | 82 | errno.h | "标识符已被删除"。集合删除时排空等待队列，一人一封；与 `EINTR`（信号打断，sem.c:887 的另一半）成对 |

排序契约再记一遍（06 篇 IPC-P1-2 的教训）：do_semop 入口的多重失败只暴露**最先**触发的那一个——权限（EACCES）先于序号（EFBIG）先于撤销（EINVAL），顺序由 `validate_ops` 的签名钉死，不是约定俗成。

## 3 标识符编码与端点

- **标识符 = 序号 << 16 | 槽号**（`IXSEQ_TO_IPCID`，ipc.h:110）。槽号复用、序号递增一位（`& 0x7fff`，sem.c:135/shm.c:109），十五位绕回永不枯竭——老标识符靠序号失配自然失效（`find_id` 双校验）。
- **端点**：PM 是 0、MIB 是 7（com.h:59/:66；`Endpoint::PM`/`Endpoint::MIB`）。等待者槽位取端点低位的进程槽号（C `_ENDPOINT_P`；Rust `Endpoint::slot`），同槽两代端点靠占有校验区分（sem.c:880 的断言）。
- **m_source 是唯一身份凭证的来源**：每个请求的 caller 都从 `m_source` 解码（minix-types 的 decode 类型带 `caller` 字段），伪造端点过不了内核这一关。

## 4 跨篇边界行为契约（IPC-P1-1 收口）

以下是服务层落地时定案的契约，各篇正文引用此处：

1. **拷贝失败的错误码**：C 的 `sys_datacopy` 失败原样返回（负的内核错误码，`EFAULT` 家族）；分配失败回 `ENOMEM`（sem.c:677-678）。Rust 的边界动词（`IpcBoundary::copy_in_*`/`copy_out_*`）返回 `Err(i32)` 承载同一语义——判定层里 `write_all` 的短缓冲检查回 `EINVAL` 是防御，不得吞掉边界层的 `EFAULT`。
2. **分配器模型**：C 的 `malloc`/`free`（semop 操作数组）在 Rust 侧是全局分配器 + `alloc::vec`；上限 `SEMOPM` 保证请求内存有界。C 的"分配失败回 ENOMEM"在测试替身里以缓冲不足表达。
3. **时间**：一律内核时钟（`clock_time(NULL)`），注入参数 `now: u64`，由边界 `now()` 供给；ctime 在创建与 IPC_SET/SETVAL/SETALL 刷新，otime 只在成功 semop 刷新，atime 在挂接**与**脱离时刷新（shm.c:228 的怪癖，08 篇有专段）。
4. **传输动词两分**：调用回信走 `ipc_sendnb`（main.c:273），进程事件回执走 `asynsend3(AMF_NOREPLY)`（main.c:207-208）——`EventLoopTransport` 的 `send_reply`/`send_async` 一一对应，生产实现不得混用。
5. **wire 布局**：`struct semid_ds`/`shmid_ds` 的二进制布局（IPC_STAT 拷出、IPC_SET 拷入的载体）登记在 edge E-IPCWIRE 第 8 项（minix-types 落位）；本 crate 的 `IpcBoundary` 结构化拷贝动词是它落地前的缝。
6. **排除项全家福**：SEM_UNDO（EINVAL+警告）、seminfo 的 undo 三字段（IPC_INFO 恒零）、`DEBUG_SEM`/`verbose` 打印、`list_shm_ds` 死代码、kern.ipc 的五个保留槽——全部"未实现且显式声明"，清单见 plan A-7/A-8。

## 5 边界

- **前置依赖**: 无（查阅页）。
- **不覆盖（移交）**: 各机制细节在 01~10；内核侧 IPC 原语在 `01-stage-kernel/12`；MIB 客户端协议在 `10-stage-mib` 22 篇与 edge E-RMIBWIRE。
