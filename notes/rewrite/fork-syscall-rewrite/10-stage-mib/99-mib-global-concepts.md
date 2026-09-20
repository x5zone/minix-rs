# 99-mib-global-concepts: 全局概念收口

> **状态**: reviewed（2026-09-15，随 todo.md P3-1 成文）
> **定位**: 全局概念（阶段 99）
> **源码**: `com.h`、`sys/sys/sysctl.h`、`minix/sysctl.h`
> **Rust 模块**: `minix-types`
> **draft 素材**: `draft/README.md`（占位）

本篇是查询手册：常量、错误码、跨服务引用在此各归一位，值全部以 minix-types 的钉值测试为准（每张表后给出权威位置），机制细节不在此复述。

## 1 服务坐标

| 事实 | 值 | C 依据 | Rust 位置 |
|---|---|---|---|
| MIB 端点号 | `MIB_PROC_NR = 7` | `com.h:66` | `minix-types/types/endpoint.rs`（`Endpoint::MIB`） |
| boot 登记位 | 紧跟 TTY、先于 VM | `kernel/table.c:60` | — |
| 特权要求 | 超级用户（要替调用方查其他服务的特权信息） | `plan.md §1.1` | `auth.rs`（`CallAuth`） |

注意一个曾经的真实偏差：kernel 侧 `grant.rs:293-295` 曾把 VFS/MIB 端点硬编码为 4/8（C 权威是 1/7），已登记 `../edge_todo.md` E-MIBGRANT——端点号的单一真值在 com.h 与 `Endpoint` 枚举，任何手抄数字都是缺陷候选。

## 2 号段与消息面

| 常量 | 值 | C 依据 | Rust 位置 |
|---|---|---|---|
| `MIB_BASE` | `0x1800` | `com.h:1022` | `types/com.rs:249` |
| `MIB_SYSCTL` / `MIB_REGISTER` / `MIB_DEREGISTER` | base+0/1/2 | `com.h:1026-1028` | `types/com.rs:252-256` |
| `NR_MIB_CALLS` | 3（三信无死号） | `com.h:1030` | `types/com.rs:258` |
| `COMMON_MIB_INFO` / `COMMON_MIB_CALL` | `0xE04` / `0xE05` | `com.h:613,616` | `types/com.rs:271-274` |
| `COMMON_MIB_REPLY` | `0xE81` | `com.h:622` | `types/com.rs:277` |
| 六种 56 字节线载荷 | `mess_lc_mib_sysctl` 等 | `ipc.h` 六处 | `ipc/message.rs:3188-3273`（`size_of==56` 全钉，`message.rs:4045`；其中结构①为 minix-rs 64 位交换面，见 02 §2.2 末 `[ARCH: MIB-SYSCTL-64LANE]` 注记） |

## 3 常量总表

全部常量的钉值与测试在 `minix-types/types/sysctl.rs`，此处只给索引：

- **尺寸族**：`CTL_MAXNAME=12`、`SYSCTL_NAMELEN=32`、`CTL_SHORTNAME=8`（消息内嵌名字分界）、`CREATE_BASE=1024`（动态 id 起点）、`SYSCTL_DEFSIZE=8`（`sysctl.rs:21-33`，对照 `sysctl.h:75-79`）。
- **顶层 id**：`CTL_UNSPEC=0` 到 `CTL_LAST`（`sysctl.h:169-183`；`sysctl.rs:37` 起），Minix 扩展 `CTL_MINIX` 及 `MINIX_*`/`TEST_*`（`minix/sysctl.h:17-59`）。
- **元标识符（负数族）**：`CTL_QUERY=-2`、`CTL_CREATE=-3`、`CTL_CREATESYM=-4`、`CTL_DESTROY=-5` 等（`sysctl.rs:704` 起，对照 `sysctl.h:158-164`）；只有末位负数合法，`CTL_CREATESYM`/`CTL_MMAP` 走 `EOPNOTSUPP`（A-9）。
- **类型族** `CTLTYPE_*` 与 **标志族** `CTLFLAG_*`（`sysctl.rs` 值表）；MIB 内部把三个 NetBSD 未用标志重赋值为 `PARENT`/`VERIFY`/`REMOTE`（`mib.h:72-74`，Rust 侧等式钉在 `tree/flag.rs:24-28`）。
- **版本**：`SYSCTL_VERSION = SYSCTL_VERS_1 = 0x0100_0000`（`sysctl.rs:687-689`，对照 `sysctl.h:130-134`）——树上说的永远是 VERS_1，`sysctlnode`/`sysctldesc` 交换格式是 NetBSD ABI（A-4，布局锚定条目 = todo.md P1-3）。

## 4 错误码汇总（A-11）

普通 errno 按各自篇章的语义出现（`EPERM` 权限、`EINVAL` 形状、`ESRCH` 无此进程、`ENOENT`/`ENOTDIR`/`EISDIR` 解析三态、`EEXIST` create 撞名、`EBUSY`/`ENOTEMPTY` destroy 守卫、`ENAMETOOLONG` label 界、`EOPNOTSUPP` 排除契约、`ENOSYS` 野号码）。三个有特殊语义的常客值得单列：

| 常量 | 在 sysctl 里的特殊含义 | 对照 |
|---|---|---|
| `ENOMEM` | 不是纯失败：部分拷贝 + 报完整长度，调用方凭此重试；且分配失败**禁止**用 ENOMEM（报 EINVAL） | `main.c:341-356`、`tree.c:589` |
| `EDONTREPLY` | 主循环的静默哨兵：非 SENDREC 调用方无回信槽，回信即误投 | `main.c:474-487` |
| `ERESTART` | 远程子树内部信号：服务死亡或请辞时，本地能续走就续走 | `tree.c:1410-1416`、`remote.c:455-459` |

Rust 建模：`dispatch.rs:105/116` 的 `default_outcome`/`should_reply` 钉住后两者；`map_sysctl_reply` 的 `error_reslen` 通道承载第一条（结构化裁决见 todo.md P2-2）。

## 5 跨服务引用（A-12 依赖链）

MIB 自己几乎不产生数据，它对外部服务的引用决定了执行半的接缝形状：

| 对端 | 消费内容 | 交付条目 |
|---|---|---|
| kernel | `sys_datacopy`、`cpf_grant_*`/revoke、`sys_getproctab`、`getticks`/`sys_hz`/`sys_getcputicks` | stage 内（todo.md P1-1/P1-4），通电挂 E1/E2 |
| PM | `getnuid`（认证缓存）、`getsysinfo(SI_PROC_TAB)`、`svrctl(PMGETPARAM)` | `../edge_todo.md` E-MIBPROD |
| VFS | `getsysinfo(SI_PROCLIGHT_TAB)`/`SI_DMAP_TAB` | 同上 |
| VM | `vm_info_stats`/`vm_info_usage`（CTL_VM 子树） | 同上 |
| DS | `ds_retrieve_label_name`（远程注册第一步） | `../edge_todo.md` E-DSWIRE |
| ProcFS | `MinixProcList`/`MinixProcData` 布局（A-5，15-stage 消费） | `minix-types/types/sysctl.rs:177-220` |
| IPC/LWIP/UDS | RMIB 客户端（注册/转发应答） | `../edge_todo.md` E-RMIBWIRE |
| libc | sysctl(3) 消息面（A-10，不实现） | 21 篇外部契约 |

## 6 执行模型声明

MIB 是**单线程事件循环**服务：`Rc`/`RefCell` 类内部可变性在单线程前提下合法，判定函数全部无共享状态；这与 kernel 的 SMP + BKL 模型（共享数据必须 `Arc`+`Mutex`/`Atomic`，临界区内不得睡眠）是两套纪律，review 时不允许混用判据（`review-core.md` 执行模型条款）。远程转发时对远端服务的 `ipc_sendrec` 是阻塞的——与 C 行为一致，是"单线程服务间同步调用"的固有属性，不设计超时。

## 7 边界

- **前置依赖**: 全部
- **不覆盖（移交）**: 各机制细节（01~22）

## 8 参见

- `00-mib-overview.md` — 总览与导航
- `minix-types/types/com.rs`、`types/sysctl.rs`、`types/endpoint.rs` — 本篇全部常量的权威落点（钉值测试同文件）
- `../edge_todo.md` — 跨 stage 条目唯一入口
