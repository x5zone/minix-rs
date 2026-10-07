# 00-runtime-overview: 用户态运行时整体概览

> **状态**: 正文 v1 落稿（2026-09-17，与 99 篇同批——V1-P1-5 的文档半）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/lib/csu/`、`minix3/minix/lib/libc/`、`minix3/minix/lib/libminc/`、`minix3/minix/lib/libsys/`（共享部分）、`minix3/minix/include/`（用户态 ABI）
> **Rust 模块**: `os/libs/minix-rt`、`os/libs/minix-sys`、`os/libs/minix-types`
> **draft 素材**: `draft/README.md`（素材）

## 核心点

- runtime 是什么：一切 userland（server/fs/driver/命令）共享的运行时库层，不是 server（无主循环/IPC 分发）
- 生命周期主线图：内核交付 → crt0 → 运行时初始化 → syscall 服务 → 终局（plan §1.1）
- 文档导航：6 阶段 15 篇，新编号交叉引用规则（plan §3.2）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- ARCH A-2：no_std + Rust core/alloc 替代 libc（libminc/Makefile 组成为排除基线）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~13、99）

---

## Ch1: runtime 是什么——一切 userland 共享的运行时库层

用户态服务器、文件系统、驱动、命令，四类程序在 Minix3 里共享同一套启动、消息、
内存与诊断机制。minix-rs 把这套机制收进两个库：`minix-types`（契约：endpoint、
消息布局、服务号常量、错误号）与 `minix-sys`（机制：系统调用的打包与传输），
`minix-rt` 则补充进程出生与生存期的脚手架（crt0、运行时初始化、分配器绑定、
恐慌输出）。runtime **不是 server**：没有主循环、没有 IPC 分发——那些是每个
服务器自己的事；runtime 只回答"一个进程如何出生、如何调用、如何离开"。

**ARCH A-2**：no_std + Rust core/alloc 替代 libc。C 的 libminc/Makefile 组成
（哪些 .o 进哪些变体）就是这份排除基线的 C 侧对应物。

## Ch2: 生命周期主线

一条主线贯穿 01~13：**内核交付 → crt0 → 运行时初始化 → syscall 服务 → 终局**
（plan §1.1）。

- **交付**（01）：内核把进程映像装入地址空间，ps_strings 指针放进 RBX，
  栈顶放进 RSP——进程在 `_start` 的第一条指令前就带着两样行李。
- **crt0**（02）：汇编桩整编寄存器，读出描述符，把控制权交给 Rust 的
  诞生函数。
- **初始化**（03）：查内核信息页（MINIX_KERNINFO）、发布全局、装表。
- **服务**（04~12）：分配器（06）、诊断（07）、PM/VFS/VM/RS/misc 各族
  系统调用包装（08~12）。
- **终局**（13 + exit）：返回值交给 `minix_sys::exit`，PM_EXIT 消息出。

2026-09-17 起，这条主线**在真机上完整执行过一次**（rt-birth，edge E1 切片 5
验收 + edge E-MIBPROD 交付）：rt-birth 用户镜像经生产 `load_vm_elf` 装入 VM
boot 槽，从 `_start` 走到 forced panic 的 hook 渲染，五断言 PASS
（`qemu-tests/test-rt-birth.sh`）。

## Ch3: 文档导航

6 阶段 15 篇，编号即阅读顺序（plan §3.2 的交叉引用规则：引用编号不引用
文件名，编号不变文件名可换）：

| 阶段 | 篇 | 一句话 |
|---|---|---|
| 0（本篇） | 00 | 整体概览与导航 |
| 出生 | 01/02/03 | 交付、crt0、初始化 |
| 机制 | 04/05 | IPC 原语、syscall 机制 |
| 资源 | 06/07 | 分配器、诊断输出 |
| 服务族 | 08~12 | PM/VFS/VM/RS 查询/misc 各族 |
| 契约 | 13/99 | 常量 ABI、全局概念权威 |

## Ch4: 设计原则

- **位置可回答性**：每个机制问题都有唯一的篇与节可回答（找不到了就立
  交叉引用，不复制内容）。
- **禁止前向引用**：本篇只讲"有什么"，不讲"怎么实现"——实现归 01~13。
- **每篇一个语义单元**：一篇讲透一件事；两件事相关靠"参见"，不靠合并。
- **ARCH 三处一致**：架构演进必须 doc/design/code 三处同标注
  （例：crt0 入口 ABI、调用号体例归一）。
