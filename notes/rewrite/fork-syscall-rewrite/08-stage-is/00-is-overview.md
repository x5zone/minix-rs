# 00-is-overview: IS 整体架构概览

> **状态**: reviewed（2026-09-15 V1 执行轮成文，取代 pending 骨架）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/is/`（8 个 .c，1151 行）+ 启动证据（`kernel/table.c:44-64`、`etc/rc.minix:117`、`etc/system.conf:271-277`）
> **Rust 模块**: `os/servers/is/` 全部
> **draft 素材**: `draft/README.md`（占位）
> **前置依赖**: 无 ｜ **不覆盖（移交）**: 一切机制细节（01~10、99）

## 1. IS 是什么：调试转储聚合器

Information Server 平时什么都不做，等用户按下功能键，再到 kernel/PM/VFS/RS/DS/VM
那里把表拷过来，格式化打印。它存在的理由是微内核的审计纪律：查看状态的代码
不与被查看的状态同地址空间（01 §1.1）。16 个转储按 hooks 表分派（03），
数据经五条通道取得（04）。

## 2. 启动主线（plan §1.2）

`rc.minix:117` 仅当 `sysenv debug_fkeys != 0` 时 `up -n is -period 5HZ` →
RS 运行时加载并动态分配 endpoint → `sef_cb_init_fresh` 向 TTY 注册 F1~SF12
观察者（A-10/A-11）→ 主循环 `get_work → classify → 干活或忽略`。
minix-rs 侧 endpoint 由 transport 层运行时注入，全 crate 无 `IS_PROC_NR`
常量（A-9，01 §2.11/§4.2）。

## 3. 无 boot_image 登记语义

`kernel/table.c:44-64` 的 17 项 boot_image 中无 `is`：IS 不是核心启动因果链
的成员，而是条件性 debug 服务。死亡也不影响系统——SIGTERM 时先取消 TTY
观察者登记再退出（先 unmap 后 exit 的顺序不变量，01 §2.8）。

## 4. 执行模型

单线程事件循环（user-space server 模型）：`!Send`/`!Sync` 合理，无
`Rc`/`RefCell` 需求，全部状态归 `IsServer` 单一所有者。与 Kernel 的
SMP+BKL 模型互斥（99 收口声明）。

## 5. 文档导航

| 编号 | 主题 | 一句话 |
|------|------|--------|
| 01 | 启动入口与主循环 | SEF 注册/分类器/回复门/EDONTREPLY |
| 02 | 功能键观察者协议 | FKEY_MAP/UNMAP/EVENTS 三命令 + TTY 契约 |
| 03 | 转储分派 | hooks 表 + `do_fkey_pressed` + 次主线路径图 |
| 04 | 数据获取五通道 | sys_getinfo/diagctl/kerninfo/getsysinfo/vm_info |
| 05~10 | 六个转储域 | kernel 8 席 + PM/VFS/RS/DS/VM 各域布局 ABI |
| 99 | 全局概念收口 | 常量表/错误码/执行模型/跨服务引用 |

交叉引用：kernel 侧 IPC 原语见 `../01-stage-kernel/`；对端服务语义见
`../03-stage-rs/`、`../04-stage-pm/`、`../05-stage-vfs/`、`../07-stage-ds/`、
`../02-stage-vm/`。

## 6. 过渡

机制从 01（出生与主循环）开始；本篇只需回答"IS 是什么、何时存在、文档
怎么读"。

## 7. 参见

- `plan.md` §1.2（启动时序图）/§4（ARCH 总表）/§5（覆盖契约）
- `todo.md` — 本 stage 的审查与修复台账
