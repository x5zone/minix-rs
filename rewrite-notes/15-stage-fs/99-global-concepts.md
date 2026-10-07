# 99-global-concepts: FS 全局概念与常量

> **重写**: 2026-09-20（edge3 卡N/S41）
> **状态**: 正文
> **定位**: 全局概念（阶段 99：协议常量、TRNS 编码、errno 映射、测试资产总表）
> **源码**: `minix3/minix/include/minix/vfsif.h`、`fsdriver.h`、`minix3/minix/include/minix/com.h`
> **Rust 模块**: `minix-types/src/ipc/fs_driver.rs`、`minix-fs/src/protocol.rs`

## 1 概念：这些常量是 VFS 与 8 个 server 的合约文本

### 1.1 请求族——33 个 REQ，一个基址

全部文件系统请求住在 `FS_BASE 0xA00`（`vfsif.h:41` 一带）里，从 `REQ_GETNODE`（+1）到 `REQ_BPEEK`（+33），`NREQS = 34`。基址的教训值一提：绝对值是 wire 契约（VFS 与 FS 两侧按同一绝对值分派），历史上把 0x600 当基址用过——现在 Rust 侧单一事实源 `minix-types/src/ipc/fs_driver.rs` 的 `test_req_family_matches_c_absolute_values` 逐值钉死，再漂移会在测试里爆而不是在线上爆。

### 1.2 TRNS 编码——请求号与事务号怎么挤进一个字

`m_type` 一个 32 位字携带两样东西：高 16 位是请求号（回复时是结果码，负 errno 按 16 位有符号读回），低 16 位是事务标识。三个宏：`TRNS_GET_ID`（取事务号）、`TRNS_ADD_ID`（合成）、`TRNS_DEL_ID`（剥离）。Rust 对位 `fs_driver.rs` 的 `trns_add_id`/`trns_del_id` 与 `minix-fs/src/protocol.rs` 的 `TransactionId`。

### 1.3 errno 映射——FS 不发明新错误

FS 侧的拒绝一律返回既有 errno（`EINVAL`/`ENOSYS`/`EROFS`/`EIO` 等），三个特殊码是例外，走回复的 `m_type`：`EENTERMOUNT`（-301，进入挂载点）、`ELEAVEMOUNT`（-302，离开挂载点）、`ESYMLINK`（-303，命中符号链接）（`vfsif.h:26-28`）。这三个码是路径解析协议的一半——VFS 收到才重启解析。

## 2 测试资产总表（全 crate 单点计数）

各篇章 §5 只记"本篇 N 个"；**全 crate 计数以本表为准**（避免多份快照漂移——历史教训见蓝图 G3）。下表为本轮实测（`cargo test -p <crate>`）：

| crate | 测试数 | 覆盖篇章 |
|---|---|---|
| minix-fs（框架） | 133 | 01~05 |
| minix-fs-mfs | 131 | 07~17 |
| minix-fs-ext2 | 25 | 21 |
| minix-fs-procfs | 23 | 18 |
| minix-vtreefs | 15 | 18~20 框架 |
| minix-fs-pfs | 12 | 06 |
| minix-sffs | 12 | 24 |
| minix-fs-ptyfs | 11 | 19 |
| minix-fs-isofs | 9 | 22 |
| minix-fs-vbfs | 2 | 23 |
| minix-fs-hgfs | 2 | 24 |
| minix-fs-rt（运行时，卡D） | 13 | 01~05 的生产半 |

合计 388。计数随实现演进，刷新规则：改哪个 crate 就重测哪个 crate 并更新本行；跨 crate 的总量变化（新增 crate/删除 crate）必须改本表。

## 3 edge 锚点——子系统尚未接通的四条通道

| 锚点 | 内容 | 状态 |
|---|---|---|
| E-FSRUNTIME | 8 server bin 的启动握手与运行时（fs-rt 已落 mfs/pfs 两家） | 已接 mfs/pfs，余 6 server 随装配复用 |
| E-FSBDEV | minix-fs 块层与真实块驱动的接缝 | 登记（fs-rt 的 `PendingBlockSource` 在此之前 fail-closed） |
| E-FSVMCACHE | 二级缓存零拷贝页移交 | 已交付（vm 四 handler + minix-sys 四封装；edge3.md S38 ✅） |
| E-FSCMDS | fsck/mkfs 命令认领 | 待卡 G（等盘上结构层稳定） |

## 4 有意省略表（intentional omissions）

- **各 REQ 的载荷字段表**：33 个请求的逐域布局在 `minix-types/src/ipc/fs_driver.rs` 的偏移表（单一权威，VFS/FS 两侧共用），本篇不抄第二份；
- **VFS 侧的发起逻辑**：归 `../05-stage-vfs/`；
- **具体文件系统的磁盘布局**：归各 server 篇（07~17、21~24）。

## 5 参见

- 阶段内：01（协议常量的实现半）、12（请求包装器）、`../05-stage-vfs/12-request-wrappers.md`（对端）
- 线上状态：`../../edge3.md` S29/S30/S32/S38 行
