# 99-全局概念

> **重写**: 2026-09-20（edge3 卡N/S41；§0 命令面规模节为本轮补写，§1~§3 契约三节保留）
> **状态**: 正文
> **定位**: 全局概念
> **源码**: `minix3/minix/commands/DESCRIBE/（构建面）`、`minix3/bin/`、`sbin/`、`usr.bin/`、`usr.sbin/`、`etc/`
> **Rust 模块**: `os/commands/*`（24 域 crate）、`os/etc/`
> **draft 素材**: 无（新建）

## 0. 命令面规模与安装面

命令面的量级先立住：328 个命令程序、865 个 .c、425,130 行（`plan.md:14` 的实测口径），Rust 侧收敛为 24 个功能域 crate（域矩阵见 `00-commands-overview.md` §1.3，本篇不抄第二份）。安装面分四层目录，PATH 语义照 C 原版：`bin`（基本命令）、`sbin`（系统管理）、`usr.bin`（用户工具）、`usr.sbin`（用户态系统管理），加 `minix/commands` 与 `games`。`etc/` 的 49 项配置文件的逐项分配在 `plan.md` §5.3。

四条 [ARCH] 决策在命令层的效果：

- **A-1 用户态静态链接**：`ld.elf_so` 不实现，命令是静态二进制，`ldd` 的语义随之变化（报告"静态链接"而非列出共享库）；
- **A-2 不移植 libc**：命令的文本组织用 `core::fmt`/`alloc::format`，系统调用走 `minix-sys` 顶层（详见 §1 的 stdio 落点清单）；
- **A-4 命令参数框架**：自研 argparse，`--help`/usage 约定全域一致；
- **A-5 退出码约定**：`Result` + errno 映射，POSIX 退出码保持不变。

构建面的 DESCRIBE 目录（`minix3/minix/commands/DESCRIBE/`）是 C 侧的命令描述数据，Rust 侧的对应物是 00 篇 §1.3 的域矩阵与各命令文档的契约表。

## 核心契约（三节，依次为依赖分层、行为判定、契约表模板）

- §1 分层契约：命令依赖 `minix-rt` 与 `minix-sys` 顶层，不直接构造 IPC 消息
- §2 行为判定基准：POSIX 规定接口契约，Minix3 C 实现是真值
- §3 命令契约表模板：在 plan.md §3.6 的列上增补 Requires（依赖 API）列

---

## 1. 分层契约：一条命令依赖谁

命令不是凭空运行的：它需要有人装载、给它入口、提供输入输出通道。Minix3 的链条是命令 → C 库 → 消息层 → 用户态服务器 → 内核调用，命令本身不知道服务器的存在——`minix3/lib/libc/`（C 库）与 `minix3/minix/lib/libsys/`（消息层）两个目录就是这条链的两层。POSIX 规定的 `open`、`read`、`write`、`fork`、`exec`、`waitpid` 都落在 C 库这一层，它们内部是发消息还是陷入内核，对命令不可见。

minix-rs 沿用同一方向，C 库的位置由两样东西顶上：

- Rust 的 `core` 与 `alloc`：语言自带，替代 `printf`、`malloc` 这类纯用户态实现（[ARCH] A-2 已决定不移植 libc，见 `../14-stage-runtime/plan.md:157`）。
- `minix-sys` 的顶层函数：系统调用的调用封装。`os/libs/minix-sys/src/lib.rs:fn fork（L148，工具生成）` 是 `fork`、`exec`、`exit`、`waitpid`、`kill`；`:191-231` 是 `open`、`close`、`read`、`write`、`mmap`。

`minix-rt` 负责更早的环节：入口、参数栈交接、退出（`os/libs/minix-rt/src/crt0.rs`、`handoff.rs`）。

依赖方向是单向的，落成硬规则：

| 层 | 允许依赖 | 禁止 |
|----|---------|------|
| 命令（`os/commands/*`） | `minix-rt`、`minix-sys` 顶层 | `minix_sys::ipc`、`minix_types::ipc` 的消息构造；服务器协议细节 |
| `minix-sys` 顶层 | `minix-sys::ipc` 的 `send`/`receive`/`sendrec` | 假设服务器的内部实现 |
| `minix-sys::ipc` 与 `minix-types` | 内核调用入口与消息布局 | 业务语义 |

需要的能力如果在 `minix-sys` 顶层没有对应函数，正确做法是在运行时阶段补一个封装，而不是在命令里直接发消息。echo 就是这种情况的正面样本：它的二进制（`os/commands/bin/fileops/src/bin/echo.rs`）只调用 `write`（`os/libs/minix-sys/src/lib.rs:fn write（L217，工具生成）`）与 exit，不需要知道 VFS 的消息编号。

plan.md 中多处用 "stdio" 指代命令的输入输出通道（`plan.md:206`、`:394`、`:405`、`:407`）。由于 [ARCH] A-2 已决定不移植 libc 的 stdio（`../14-stage-runtime/plan.md:157`、`:273`），这些句子需要落到具体函数上，否则读者会等待一个不会出现的库。准确的含义是：

- 输出：`core::fmt` 与 `alloc::format` 组织文本，再用 `minix-sys` 的 `write` 写出；
- 输入：用 `minix-sys` 的 `read` 读入，或命令自己按行解析；
- 终端属性：常量按需从 `minix3/sys/sys/termios.h` 搬运（`../14-stage-runtime/13-constants-abi.md:48`），行为面走 `ioctl` 封装（`../14-stage-runtime/09-vfs-syscalls.md`）；
- 网络：`minix-sys` 的 socket 封装，协议实现归 `../17-stage-net/`。

---

## 2. 行为判定基准：POSIX 与 Minix3 C 各自的角色

POSIX 规定的是一组接口契约：函数的签名与语义（`open`、`read`、`write`、`fork`、`exec`、`waitpid` 等），以及工具本身（`echo`、`ls` 就在 POSIX 里）。它不规定某个接口是系统调用还是库函数，也不规定实现走消息传递还是陷入内核。Minix3 的实际结构是：内核只提供少量内核调用，进程管理、文件系统、服务管理都是用户态服务器，C 库把请求打包成消息再发出去。

因此命令层的行为判定沿用项目既有的 ground truth 链：**Minix3 C 源码 > 阶段设计文档 > Rust 现有实现**，并补充两条命令层规则：

1. 行为契约以 POSIX 为准绳，以 Minix3 C 实现为真值。C 实现偏离 POSIX 时按 C 实现写，并在命令契约表里标注偏离点。
2. 每个命令的选项、退出码、错误输出面逐项对照 C 源，不允许"典型命令详述、其余略过"（`plan.md:205` 已有覆盖要求，此处给出判定依据）。

---

## 3. 命令契约表模板：增补 Requires 列

plan.md §3.6 的契约表当前列为：命令 / C 源 / 职责 / 关键选项 / 输入输出 / 退出码 / 错误面 / Rust 模块（`plan.md:201`）。这张表回答不了"这个命令现在能不能动手"：`06-file-ops.md:6` 只能写"文件读写待系统调用"，读者无从知道缺的是哪个函数。增补一列 **Requires**——本命令需要的最小 API 清单。清单中的每一项都应能在 `minix-sys` 顶层找到；找不到的，登记到运行时阶段的缺口清单（`../14-stage-runtime/todo.md:60` 已登记 stat/getdents/ioctl 一族）。

| 命令 | Requires（最小 API） | 当前状态 |
|------|---------------------|---------|
| echo | `write`、`exit`、argv 交接（`minix-rt`） | 已实现（`os/commands/bin/fileops/src/bin/echo.rs`；argv 接缝现走 std，no_std 构建换 minix-rt） |
| cat | `open`、`read`、`write`、`close`、`exit` | API 齐备 |
| ls | `open`、`getdents`、`stat`、`write` | `getdents`、`stat` 缺封装（`../14-stage-runtime/todo.md:60`） |
| sh（执行器） | `fork`、`exec`、`waitpid`、`dup2`、`pipe`、`kill`、信号面 | `dup2`、`pipe`、信号面缺封装 |

用法：各命令文档按此模板逐命令填 Requires；跨文档统计同一 API 的消费者数量，作为运行时阶段补齐顺序的依据。

---

## 边界

- **前置依赖**: 全部
- **不覆盖（移交）**: 一切机制细节（01~24）

## 参见

- `plan.md`——§3.3 边界表、§3.6 命令契约表、§5.2 命令归属表、§5.4 排除表
- `todo.md`——本阶段实施规格缺口登记（依赖契约、Requires 列、POSIX 基准、线程模型各轮的登记与销账记录）
- `../14-stage-runtime/plan.md`——运行时库分层与 [ARCH] A-2（no_std 替代 libc）、A-4（TLS）
- `../14-stage-runtime/todo.md:60`——命令层硬依赖 API 的缺口登记
- `../17-stage-net/plan.md`——socket 封装的归属
