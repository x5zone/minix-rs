# 99-global-concepts: 全局概念

> **状态**: 正文 v1 落稿（2026-09-17，E-MINTYPES-RUNTIME 第①步；.design outline/design v1 快照同行入库）
> **定位**: 全局概念（所有文档共享）
> **源码**: `minix3/minix/include/minix/type.h`、`ipc.h`、`com.h`、`endpoint.h`、`const.h`、`config.h`
> **Rust 模块**: `os/libs/minix-types`
> **draft 素材**: 无（新建）

## 核心点

- endpoint/generation 语义（type.h/endpoint.h）
- message 布局与 56 字节负载约束（ipc.h，_ASSERT_MSG_SIZE）
- 服务号常量（com.h：PM=0/VFS=1/RS=2/VM=8/MIB=7）、minix_ipcvecs 结构
- 全局状态表（_minix_kerninfo/_minix_ipcvecs 等）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制（见 00~13）

---

## Ch1: 概念——四件事，一份契约

全局概念篇回答一个问题：**所有服务共享的数字与布局，由谁说了算**。Minix3 的答案是
一组头文件（type.h/ipc.h/com.h/endpoint.h）；minix-rs 的答案是 `minix-types` 这个
crate。REDOX 把同样的东西收进一个 `syscalls.toml` 单源——三家一致的方向：这些数字
只能有一份权威。

### 1.1 endpoint 与 generation

endpoint 是进程的对外地址。Minix3 把它编码为「槽位 + 代数」：低位的槽号永远不变，
高位累加代数防止旧消息打到新进程。Rust 侧 `Endpoint(pub i32)` + `get()`；常量表
（endpoint.rs）给出 PM=0/VFS=1/RS=2/…的系统槽与 `ANY`/`NONE` 哨兵。
**本篇不裁决定时器/调度相关的 endpoint 派生**——那是内核篇的事。

### 1.2 message：56 字节的负载契约

`Message` = `m_source` + `m_type` + 56 字节 union 载荷。C 用 `_ASSERT_MSG_SIZE`
钉住每个 `mess_*` 结构；Rust 侧等价物 = 每个带 `#[repr(C)]` 的 wire 结构配一组
`size_of`/`offset_of` 见证（`MprocWire` 464B、`KmessagesSnap`、`IdSet` 等）。
LP64 判例（E7/A-E 批确立）：**指针域 4→8、padding 等比收缩、总长保持 56**。
新增 wire 结构必须走同一判例并带见证，否则 E-ISPROD 警告过的「按猜测布局解释
字节」事故会重演。

### 1.3 调用号：绝对值与相对偏移的归一

服务号常量分两个家族：**进程槽号**（PM=0/VFS=1/RS=2/VM=8/MIB=7，
`proc_nr.rs`）与**内核调用号**（`com.h:205` `KERNEL_CALL = 0x600` 基 + 偏移，
`kernel_call.rs` 单点权威）。T2 真机发现并修复的体例分裂发生在这里：用户库发
绝对值（0x600+44），内核 `Syscall::try_from` 只认相对偏移（44）——归一点在
`kernel_call_dispatch_inner`（减基直通，C mpx.S `subl $KERNEL_CALL` 同型）。
**裁决**：libc/服务侧发绝对值；内核入口归一一次；宿主测试可直接喂相对值。

### 1.4 全局状态表

`_minix_kerninfo`/`_minix_ipcvecs` 两个 C 全局在此篇记账：kerninfo 页已按 C
行为实装（E-KERNINFO/E-MIBPROD，`types/kerninfo.rs` 布局 + `kerninfo::init`）；
ipcvecs 维持不保留（64 位 syscall 直入内核，28 篇 D5）。字节布局契约 =
`minix_types::types::kerninfo` 与 `mproc.rs`（D-29 镜像，464B）。

---

## Ch2: 与 04/08/09 篇悬空项的收口关系

- 09 篇 open 路径 40 字节内联裁决：归本篇 §1.2 的 LP64 判例下再裁
- V1-P2-2 wire 单点权威：本篇 §1.3 裁决 + E-MINTYPES-RUNTIME 执行序
- 08 篇消息体布局集中归档：本篇 §1.2 的见证纪律承接
- E7/E-MIBPROD 的 MprocWire/kerninfo 镜像：§1.4 的既成事实

