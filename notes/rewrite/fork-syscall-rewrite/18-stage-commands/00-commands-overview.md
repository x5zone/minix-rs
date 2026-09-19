# 00-总览：命令面全图与交付链路

> **重写**: 2026-09-20（edge3 卡N/S41；crate 口径按蓝图 G1 从 35 修正为 24 域）
> **状态**: 正文
> **定位**: 总览：命令面全图与交付链路
> **源码**: `minix3/bin/`、`sbin/`、`usr.bin/`、`usr.sbin/`、`minix/commands/`、`minix/usr.bin/`、`games/`、`etc/`、`libexec/getty/`
> **Rust 模块**: `os/commands/*`（24 域 crate）、`os/etc/`
> **draft 素材**: 无（新建，按 18-stage plan §5 的全量映射）

## 1 概念：命令面是什么，为什么按"交付链"而不是"按服务器"组织

### 1.1 一个非 server 主线

18-stage 与其它 stage 最大的不同：它没有单一的服务器进程。它覆盖的是 Minix3 用户可见的**命令/工具层与系统配置层**——328 个命令程序（30 bin + 19 sbin + 141 usr.bin + 25 usr.sbin + 81 minix/commands + 8 minix/usr.bin + 24 games）、`/etc` 的 49 项配置、getty/login 登录链路，合计 865 个 .c / 425,130 行（`18-stage-commands/plan.md:14` 的实测口径）。因此本阶段的文档主线不是"某个 server 的生命周期"，而是**用户系统交付因果链**：boot → init（01）→ rc → 服务（02）→ 登录（03）→ 设备/数据库（04）→ shell（05）→ 命令使用（06~24）。编号即阅读顺序，与实施顺序（plan §6 的批次）解耦。

### 1.2 交付链双轨：进程从哪来，命令靠什么跑

每个命令的一生有两条轨：

- **装载轨**：boot → init（01 篇的 rc 状态机）→ getty/login（03 篇）→ shell（05 篇）→ 用户敲入命令 → fork+exec。装载轨决定"命令怎么开始运行"。
- **依赖轨**：命令 → Rust `core`/`alloc` 与 `minix-rt`（入口/参数/退出）→ `minix-sys` 顶层封装（系统调用）→ 用户态服务器（VFS/PM/…）→ 内核。依赖轨决定"命令能调到什么"。99 篇 §1 的分层契约表把这条轨落成硬规则：命令 crate 只许依赖 `minix-rt` 与 `minix-sys` 顶层，禁止直碰 `minix_sys::ipc`。

### 1.3 功能域矩阵：24 域 crate 与 26 篇文档

Rust 侧按**功能域**收敛成 24 个域 crate（不是一命令一 crate）：

| 域 | 篇 | crate（os/commands/） |
|---|---|---|
| init/rc | 01 | sbin/init |
| 服务调度 | 02 | usr-sbin/svcsched |
| 登录 | 03 | usr-bin/login |
| 设备/数据库 | 04 | sbin/devdb、sbin/mountinfo |
| shell | 05 | bin/shell |
| 文件（06）/文本（07） | 06/07 | bin/fileops、usr-bin/textfilter |
| 正则（08）/编辑（09）/文档（10） | 08~10 | usr-bin/regex、bin/editor、usr-bin/doctools |
| 归档压缩/进程（11/12） | 11/12 | usr-bin/compress、bin/proctools |
| 终端（13） | 13 | bin/termctl |
| 存储（14~17） | 14~17 | sbin/diskfmt（fsck/mkfs 归此，等卡 G） |
| 网络（18/19） | 18/19 | usr-sbin/netconfig、usr-sbin/netservices |
| 系统（20/21） | 20/21 | bin/sysinfo、sbin/maint |
| 游戏（22~24） | 22~24 | games/{text-games, stdio-games, term-games} |

蓝图的 G1 口径修正记录在案：早期"35 crate"的口径按域合并实测为 24（`find os/commands -name Cargo.toml` 实测）。

### 1.4 设计原则（全阶段文档共守）

- **位置可回答性** / **禁止前向引用** / **每篇一个语义单元** / **ARCH 三处一致**：同全仓四原则
- **非 server 主线重定义**：本线的"主线"是交付链而非服务器生命周期（参照 14/15/16/17 先例）
- **变体 diff 原则**：同类命令只写域内共性一次，命令个体只写差异

## 2 C 源码分析：规模与分组

全量映射的权威表在 `18-stage-commands/plan.md` §5（865 .c / 425,130 行逐目录统计、328 命令归属、`etc/` 49 项分配，§6 覆盖结论"0 遗漏"）。本文档不抄第二份，只给分组骨架：bin（30）/sbin（19）/usr.bin（141）/usr.sbin（25）/minix/commands（81）/minix/usr.bin（8）/games（24）+ `libexec/getty` 与 `usr.bin/login` 登录链 + `etc/` 49 项配置。

## 3 文档导航：26 篇

01 init/rc、02 服务调度、03 登录、04 设备/数据库、05 shell、06 文件、07 文本、08 正则、09 编辑、10 文档/man、11 归档、12 进程、13 终端、14~17 存储、18/19 网络、20/21 系统、22~24 游戏、99 全局概念（分层契约/行为判定基准/契约表模板）。已接线批次与剩余长尾见 todo §6.1（C-1 批次已完成，05 sh/23/24 与 Requires 回填在册）。

## 4 边界

- **前置依赖**: 09-stage-init（01 篇的 rc 语义）、14-stage-runtime（minix-rt/minix-sys 面）
- **不覆盖（移交）**: 服务器协议语义（各 server stage）；libc 的完整移植面（[ARCH] A-2 决策记录在 14-stage plan）；fsck/mkfs 的盘上结构（15-stage / 卡 G）

## 7 参见

- 交付链的运行时半：99 篇 §1 分层契约
- 批次进度：todo §6.1；S35/S36 行状态见 `../../edge3.md`
