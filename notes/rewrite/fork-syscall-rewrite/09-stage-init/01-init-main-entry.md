# 01-init-main-entry：入口与进程身份

> **定位**：boot 链终点 → `main()`（`minix3/sbin/init/init.c:229-367`）入口全流程，02~14 的锚点。
> **源码**：`minix3/sbin/init/init.c`（`main` 229-367、`mfs_dev` 1703-1788）、`minix3/sbin/init/pathnames.h:39-40`、`minix3/include/paths.h:62,125`。
> **Rust**：`os/commands/sbin/init/src/entry.rs`、`os/commands/sbin/init/src/main.rs`。
> **前置依赖**：`../01-stage-kernel/06-proc-init-boot-proc.md`、`../01-stage-kernel/09-vm-boot-protocol.md`（init 如何被加载与首次调度）。
> **本篇不覆盖（移交）**：状态机细节（见 02）、日志与 disaster（见 03）、securelevel 机制（见 12）、信号 handler 语义（见 02/03/14）。

---

## 1. 概念：操作系统的第一个用户进程为何需要三重出生证明

操作系统的启动可以看作一场接力赛。内核先跑，依次点亮内存管理、进程表、系统服务，最后把接力棒交出去。接棒的人就是 init：它是内核调度运行的第一个用户态进程，也是此后所有用户进程在血缘上的祖先。

问题在于，内核凭什么相信“我是 init”？用户又凭什么相信“这个 init 是真的”？答案是 init 在出生的头几十行代码里主动出示三份证明，每一份都对应操作系统的一个基础概念。

第一份证明是身份。init 检查自己的用户 ID 必须为 0（root），进程 ID 必须为 1。如果 uid 不是 0，说明它不是以最高权限启动的，继续运行只会处处碰壁，不如立刻以权限错误退出。如果 pid 不是 1，说明系统里已经有一个 init 在跑，第二个 init 没有存在的理由，直接报错退出。这两行检查看起来霸道，实则是 PID 命名空间的锚点语义：PID 1 不是普通编号，它是孤儿进程的收养人，是所有会话的根。Minix3 的进程管理器（PM）也承认这一点：INIT 的父进程被强制设为自身（`minix3/minix/servers/pm/main.c:188-204`），RS 登记表中 init 被标为 `USR_F` 普通用户进程（`minix3/minix/servers/rs/table.c:28`），boot 镜像把它放在最后一项（`minix3/minix/kernel/table.c:64`）。换句话说，PID 1 是一个各方约定的位置，init 只是第一个坐上去并且敢验票的进程。

第二份证明是会话。init 调用 `setsid()` 创建一个新会话，把自己变成会话首进程。这一步的动机是解耦：init 从内核那里继承来的控制终端关系是未定义的，如果不主动切断，它可能被某个终端的挂断信号误伤。新建会话之后，init 就不再属于任何终端，后续每个登录会话（getty）再各自建立自己的控制终端，互不干扰。值得注意的是 C 代码对 `setsid()` 的失败非常宽容：失败只警告一声继续跑（`init.c:255-256`）。这不是疏忽，而是务实——会话只是卫生措施，不是生死线。

第三份证明是描述符卫生。init 关闭 0、1、2 三个标准描述符。这遵循经典的守护进程范式：刚出生的进程从父进程继承来的描述符表是不可信的，留着它们，后续 fork 出来的子进程会莫名其妙地继承奇怪的标准输入输出。与其逐个检查，不如全部关掉，等需要时再按需打开。这种“先清空再重建”的思路在整个 init 里反复出现，后面的会话链表重建、ttys 重读都是同一哲学。

理解了这三份证明，`main()` 的其余动作就顺理成章了：探测设备（`/dev/console` 在不在，不在就跑 MAKEDEV 现造）、解析参数（`-s` 单用户、`-f` 快速启动）、注册信号（只登记位置，具体语义后文再讲）、探测 securelevel（问问内核支不支持安全级别），最后调用 `transition()` 把控制权交给状态机。从这一刻起，init 的线性出生流程结束，循环的状态机人生开始。

> **架构范围**：本章讨论的是 POSIX init 共性语义。Minix 特有的 `mfs_dev`（无 console 时跑 MAKEDEV）与 `USR_F` 登记属于 Minix 移植层，下文明确标注。

### 1.1 本章不讲什么

- 状态机如何流转（见 02）。
- 日志三件套与致命信号路径（见 03）。
- securelevel 查询与设置的内核交互（见 12）。
- `/etc/rc` 与 `/etc/ttys` 的具体格式（见 05、06）。

### 1.2 小结

记住一句话就行：init 的入口做且只做三件事——证明自己是 PID 1、与旧终端世界断绝关系、把文件描述符表打扫干净，然后带着参数决策跳进状态机。下一章我们看 C 代码是如何一字不差地落实这三件事的。

---

## 2. C 源码分析

### 2.1 入口全景：八个步骤

`main()`（`init.c:229-367`）按执行顺序可分为八步。下表给出每步的源码位置与行为摘要，便于与 Rust 实现逐项对照。

| 步骤 | 源码 | 行为 | 失败策略 |
|---|---|---|---|
| S1 身份校验 | `init.c:242-249` | `getuid()!=0` 置 `EPERM` 退出；`getpid()!=1` 报 `already running` 退出 | 直接退出 |
| S2 新建会话 | `init.c:255-256` | `setsid()` 建初始会话 | 仅 `warn`，继续 |
| S3 非 Minix 登录名 | `init.c:262-265` | `#if !defined(__minix)` 时 `setlogin("root")` | Minix 构建跳过 |
| S4 设备探测 | `init.c:268-271` | `MFS_DEV_IF_NO_CONSOLE` 下 `mfs_dev()==-1` 则首状态改 single_user | 改写 `requested_transition` |
| S5 日志打开 | `init.c:278` | `openlog("init", LOG_CONS, LOG_AUTH)` | 无 |
| S6 参数解析 | `init.c:287-303` | `getopt "sf"`：`-s` 置 single_user，`-f` 置 FASTBOOT；多余参数警告 | 警告并继续 |
| S7 信号注册 | `init.c:310-334` | 注册 8 类信号映射并屏蔽、忽略 SIGTTIN/SIGTTOU | 全忽略返回值 |
| S8 收尾与跳转 | `init.c:339-358` | `close(0/1/2)`；建 `init.root` 节点（CHROOT）；`has_securelevel()`；`transition(requested_transition)` | 跳转后不返回 |

默认首状态是 `runcom`（`init.c:195`），`LETS_GET_SMALL` 构建下默认 `single_user`（`init.c:217`）。`runcom_mode` 默认 `AUTOBOOT`（`init.c:151`）。

### 2.2 S1 身份校验：两行拒绝

```c
/* init.c:242-249 */
if (getuid() != 0) {
    errno = EPERM;
    err(1, NULL);
}
if (getpid() != 1)
    errx(1, "already running");
```

`err(1, NULL)` 打印带 `strerror(errno)` 的消息并以状态 1 退出；`errx` 不带 errno 后缀。两者的共同点是都不返回。Rust 侧把“校验”与“退出”分离（见 §3.2），但错误语义保持一致：非 root 对应 `EPERM`，重复运行对应明确的错误变体。

### 2.3 S6 参数解析：两个字母的全部语义

```c
/* init.c:287-303 */
while ((c = getopt(argc, argv, "sf")) != -1)
    switch (c) {
    case 's':
        requested_transition = single_user;
        break;
    case 'f':
        runcom_mode = FASTBOOT;
        break;
    default:
        warning("unrecognized flag `%c'", c);
        break;
    }
if (optind != argc)
    warning("ignoring excess arguments");
```

语义要点有三。第一，`-s` 改变的是首状态，不是运行模式开关；一旦置位，后续 `transition()` 直接进入单用户 shell（见 04）。第二，`-f` 改变的是 `/etc/rc` 的执行方式（fastboot 跳过文件系统检查，见 05），不改变状态序列本身。第三，未知参数与多余参数都不致命，只记一条警告。这种宽容是有意的：真实 boot 路径下 VM 传给 init 的参数是固定的 `{"init", NULL}`（`minix3/minix/servers/vm/main.c:345`），`-s`/`-f` 在正常启动中根本不会出现；它们是运维手动干预的后门，不是常规输入。

### 2.4 S4 设备探测：mfs_dev

```c
/* init.c:268-271 */
#ifdef MFS_DEV_IF_NO_CONSOLE
if (mfs_dev() == -1)
    requested_transition = single_user;
#endif
```

`mfs_dev()`（`init.c:1703-1788`）的逻辑分三层。先看 `/dev/console` 是否存在，存在则直接返回 0，什么都不做，这是快路径。中间一大段 `#if 0` 包裹的调试代码（`init.c:1716-1756`）是死代码，构建不生效，阅读时跳过。真正的慢路径是 fork 一个子进程跑 MAKEDEV 脚本（`init.c:1759-1785`）：子进程里先把标准错误复制到标准输出，然后 `chdir("/dev")`，执行 `sh ./MAKEDEV -MM init`（或回退到 `/etc/MAKEDEV`）。父进程等待子进程结束，若 `/dev/console` 被造出来就返回 0，否则 `_exit(11/12)`。注意父进程分支的 `_exit` 语义：`mfs_dev` 运行在 init 自己的进程上下文里，失败意味着设备子系统出了大问题，调用方把它降级为单用户模式而非直接崩溃，给管理员留一条抢修通道。

路径常量方面，`_PATH_CONSOLE` 定义为 `/dev/console`（`minix3/include/paths.h:62`），`INIT_BSHELL` 在 Minix 构建下取 `_PATH_BSHELL` 即 `/bin/sh`（`init.c:105`、`paths.h:125`）。`pathnames.h` 里的 `_PATH_SLOGGER`（`pathnames.h:39`）在 `init.c` 中无使用者，属于历史遗留死常量，阅读时忽略。

### 2.5 S7 信号注册调用点（本篇只列位置）

```c
/* init.c:314-334，Minix 分支 */
handle(minixreboot, SIGABRT, 0);          /* Ctrl-Alt-Del 重启挂钩 */
handle(minixpowerdown, SIGUSR1, 0);       /* 低电关机挂钩 */
handle(disaster, SIGFPE, SIGILL, SIGSEGV, SIGBUS, 0);
handle(transition_handler, SIGHUP, SIGTERM, SIGTSTP, 0);
handle(alrm_handler, SIGALRM, 0);
/* 屏蔽除上述外的几乎所有信号；忽略 SIGTTIN/SIGTTOU */
```

`handle` 与 `delset` 的实现（`init.c:369-409`）归 02，`disaster` 归 03，`minixreboot`/`minixpowerdown` 归 14。本篇只确认调用点存在且顺序如上，不展开 handler 语义。

### 2.6 S8 收尾：描述符、sysctl 节点、securelevel 探测

`close(0/1/2)`（`init.c:339-341`）全部忽略返回值，符合“卫生措施不设失败路径”的惯例。`createsysctlnode()`（CHROOT 构建）归 12。`has_securelevel()`（`init.c:544-563`）用 `sysctl(KERN_SECURELVL)` 试探内核能力，不存在返回 0，存在返回 1，结果存入 `securelevel_present`（`init.c:353`）。查询与设置的完整语义见 12。

---

## 3. Rust 设计决策

### 3.1 总览：把八步线性流程变成可测试的纯函数加薄接线

C 的 `main()` 是典型的“边解析边行动”风格：读到 `-s` 就地改全局，探测失败就地改全局，校验失败就地退出。这种风格在单体 C 程序里很高效，但有两个代价：逻辑不可单测（必须 fork 真进程才能验证），意图不可见（全局变量在十几个函数之间隐式传递）。

Rust 侧的设计原则是：凡是不需要系统调用就能算出来的，一律做成纯函数；凡是需要系统调用的，一律收敛到 trait 边界后面。于是八步流程被切成三层：参数解析层（纯）、决策层（纯）、执行层（薄）。`main()` 本人只剩十几行接线代码，读起来像目录，测起来靠单测覆盖纯层，集成靠 trait 的假实现。

与 Redox 的对照值得一提。Redox 的 init 风格服务同样把“解析启动配置”与“执行启动动作”分开，配置结构体可以独立构造与断言。我们借鉴了这种分离，但没有照搬 Redox 的 scheme 抽象，因为 Minix3 的 init 语义（状态机字符、ttys 格式、runcom 模式）与 Redox 差异太大，硬套只会增加翻译腔。

### 3.2 决策一：BootArgs 只管解析，不管执行

```rust
pub struct BootArgs { pub single_user: bool, pub fastboot: bool }
pub fn parse_boot_args(argv: &[String]) -> (BootArgs, Vec<String>);
```

C 把 `getopt` 循环与全局改写揉在一起，Rust 把它拆成输入 `argv` 切片、输出 `(BootArgs, warnings)`。未知参数与多余参数进入 `warnings` 向量，由调用方决定记日志还是忽略。`argv[0]`（程序名）自动跳过，与 C 的 `getopt` 行为一致。空参数、重复 `-s`、`-sf` 合写、`--` 终止符等边界都在单测里覆盖。

### 3.3 决策二：身份校验返回 Result，退出上移

```rust
pub enum EntryError { NotRoot, AlreadyRunning }
pub fn check_identity(uid: u32, pid: i32) -> Result<(), EntryError>;
```

`NotRoot` 对应 C 的 `EPERM`，`AlreadyRunning` 对应 `already running`。函数本身不退出、不打印，调用方（`main`）再把错误翻译成退出码与日志。这样单测可以直接断言 `check_identity(1000, 1) == Err(NotRoot)`，不需要 fork 子进程看退出码。这是从 C 到 Rust 最典型的“库与策略分离”，也是避免 translate 味道的关键一步：C 的 `err()` 是库与策略的混合体，Rust 把它拆开。

错误码映射上，`EntryError::NotRoot` 到 `minix_types::Errno::EPERM` 的转换是显式的（`to_errno()` 方法），不自创错误码，符合全仓约束。

### 3.4 决策三：首状态决策显式化为 EntryDecision

```rust
pub enum RuncomMode { Autoboot, Fastboot }
pub enum InitialState { Runcom, SingleUser }
pub struct EntryDecision { pub initial: InitialState, pub runcom_mode: RuncomMode }
pub fn decide_entry(args: &BootArgs, console_ok: bool) -> EntryDecision;
```

C 用两个全局变量（`requested_transition`、`runcom_mode`）隐式传递决策，Rust 用一个返回值显式传递。`console_ok=false`（即 `mfs_dev` 失败）强制 `SingleUser`，优先级高于 `-s` 参数，两者一致时自然合并。这张真值表只有四行，单测全覆盖，比读 C 的三处赋值点更不容易漏。

状态本身用枚举而非函数指针。C 的 `state_t` 是返回函数指针的函数指针（`init.c:130-131`），类型体操复杂且无法穷尽匹配。Rust 的 `InitialState` 只有两个变体，`match` 必须处理完备，新增状态时编译器会逼着补全。完整七状态枚举在 02 定义，本篇只用其子集并做显式注释，避免前向引用。

### 3.5 决策四：mfs_dev 收敛为 DeviceProbe trait

```rust
pub trait DeviceProbe {
    fn console_present(&self) -> bool;
    fn ensure_devices(&self) -> DeviceEnsureOutcome;
}
```

真实实现检查 `/dev/console` 是否存在，缺失则执行 MAKEDEV 流程；测试用内存假实现直接返回预设值。`DeviceEnsureOutcome::{Ok, FellBackToSingleUser, Failed}` 对应 C 的三种结局。`#if 0` 死代码段在 Rust 侧直接丢弃，并在注释中说明丢弃依据（C 行号），不做无意义的逐行翻译。

构建变体宏（`LETS_GET_SMALL`、`MFS_DEV_IF_NO_CONSOLE` 等 Makefile 开关）属于 **[ARCH A-7]**：Rust 侧取 Minix 默认全开语义（支持 `-s`/`-f`、支持 MAKEDEV 回退），暂不提供 feature 开关。如未来需要裁剪体积，再引入 Cargo feature，此处预留注释说明。

### 3.6 本篇有意不做的事

信号 handler、日志通道、securelevel 查询、sysctl 节点创建在本篇只保留调用点占位，具体 trait 定义与实现分别在 02、03、12 落地。本篇的 `main()` 接线代码用清晰的注释标出这些移交点，避免读者误以为本篇漏了实现。

---

## 4. 实现详解

### 4.1 模块结构

```text
os/commands/sbin/init/src/
  main.rs    — 接线：init() → 校验 → 会话 → 设备 → 参数 → 信号占位 → transition 占位
  entry.rs   — 本篇：BootArgs / Identity / EntryDecision / DeviceProbe
```

设计决策引用：`entry.rs` 顶部注释标 `design §1.1~§1.4`；每个公有项注释其 C 对照行号。

### 4.2 关键流程（与 C 八步的对照）

| C 步骤 | Rust 对应 | 差异说明 |
|---|---|---|
| S1 身份校验 | `check_identity(uid, pid)` 返回 Result | 退出上移 main；语义等价 |
| S2 setsid | trait 占位（02 落地完整信号与会话抽象） | 本篇只记调用点，缺口显式标注 |
| S4 mfs_dev | `DeviceProbe::ensure_devices()` | 死代码段丢弃；三种结局枚举化 |
| S6 getopt | `parse_boot_args(argv)` 纯函数 | 副作用后移 decide_entry |
| S8 close/securelevel/transition | main 接线顺序保留 | securelevel 机制移交 12 |

未知参数示例：`parse_boot_args(["init","-z"])` 返回 `(BootArgs{false,false}, ["unrecognized flag `z'"])`，与 C 的 `warning("unrecognized flag ...")` 语义对应，日志动作由调用方完成。

### 4.3 不变量

- `decide_entry` 永不失败，总能给出一个合法首状态（默认 Runcom）。
- `check_identity` 无副作用，可任意次调用。
- `parse_boot_args` 不触碰全局状态与文件系统，同一输入恒定同一输出。

---

## 5. 测试要点

测试文件位于 `entry.rs` 内 `#[cfg(test)]` 模块，运行 `cargo test -p minix-init`。

| 测试函数 | 覆盖的设计 | C 对照 |
|---|---|---|
| `test_parse_no_args_defaults` | 默认 Autoboot、无警告 | `init.c:287-303` 空参数路径 |
| `test_parse_single_user_flag` | `-s` 置单用户 | `init.c:289-291` |
| `test_parse_fastboot_flag` | `-f` 置 FASTBOOT | `init.c:292-294` |
| `test_parse_combined_flags` | `-sf` 合写 | 同上 |
| `test_parse_unknown_flag_warns` | 未知参数进 warnings | `init.c:295-297` |
| `test_parse_excess_args_warn` | 多余位置参数警告 | `init.c:300-301` |
| `test_identity_root_and_pid1_ok` | 合法身份通过 | `init.c:242-249` |
| `test_identity_non_root_fails` | 非 root 报 NotRoot | `init.c:242-245` |
| `test_identity_wrong_pid_fails` | pid 非 1 报 AlreadyRunning | `init.c:248-249` |
| `test_decide_defaults_to_runcom` | 默认首状态 runcom | `init.c:195` |
| `test_decide_single_user_flag` | `-s` 改 single_user | `init.c:290` |
| `test_decide_console_failure_forces_single_user` | 设备失败降级 | `init.c:269-270` |
| `test_entry_error_maps_to_errno` | NotRoot 对应 EPERM | errno 对齐约束 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：13 个通过，0 个失败。
- 本节列出与本模块直接相关的 13 个（全集）。
- 完整清单：`rg "fn test_" os/commands/sbin/init/src/entry.rs`。

---

## 6. 过渡：在状态机中的位置

本篇是状态机的“发射台”：做完八步之后调用 `transition(requested_transition)`，把控制权交给 02 定义的状态机主循环。`-s` 的读者下一站是 04（单用户 shell），默认的读者下一站是 05（runcom），`-f` 的语义在 05 展开，设备缺失被降级的读者同样先读 04。信号注册的八个名字在本篇只是名单，它们的简历在 02（转换表）、03（disaster）、14（重启与关机挂钩）。

---

## 7. 参见

- `02-init-state-machine.md` — `transition` 主循环与信号→状态转换表（本篇 S7 的语义归属）。
- `03-init-logging-failure.md` — `warning`/`emergency`/`disaster`（本篇 S1/S6 警告的通道）。
- `05-init-runcom.md` — `runcom_mode` AUTOBOOT/FASTBOOT 的完整语义。
- `12-init-sysctl-interaction.md` — `has_securelevel` 探测与 `init.root` 节点。
- `14-init-external-contracts.md` — boot argv 契约（VM 固定 `{"init",NULL}`）与 USR_F 身份。
- C 源码：`minix3/sbin/init/init.c:229-367`、`minix3/sbin/init/init.c:1703-1788`、`minix3/include/paths.h:62,125`。
