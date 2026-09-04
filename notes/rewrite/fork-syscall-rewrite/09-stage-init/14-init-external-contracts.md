# 14-init-external-contracts：对外契约

> **定位**：`minixreboot`（`minix3/sbin/init/init.c:517-525`）、`minixpowerdown`（530-538）与跨服务契约（plan §5.3 全 13 项）。
> **Rust**：`os/commands/sbin/init/src/contracts.rs`。
> **前置依赖**：01（boot argv）、02（信号注册）、09/11（PM 交互）。
> **本篇不覆盖（移交）**：各对端服务的实现（见 `../01-stage-kernel/`、`../04-stage-pm/`）。

---

## 1. 概念：不是服务，而是众所周知的用户进程

init 最容易被误解的身份是“系统服务”。它不是。RS 登记表把它标为 `USR_F`（`minix3/minix/servers/rs/table.c:28`），procfs 的 `service_active` 对它返回假（`minix3/minix/fs/procfs/service.c:195-207`），它没有 IPC 主循环，没有 SEF，没有 CALLMAP。它只是一个 pid 为 1 的普通用户进程，恰好被所有人认识：内核把它放在 boot 镜像最后（`table.c:64`），VM 用固定参数 `{"init", NULL}` 启动它（`vm/main.c:345`），PM 把孤儿都过继给它（`forkexit.c:396`），键盘驱动在 Ctrl-Alt-Del 时给它发 SIGABRT（`keyboard.c:300`），电源驱动在低电时给它发 SIGUSR1（`tps65217.c:226`）。认识它的人越多，它的契约越不能变。

两个 Minix 特有挂钩是这种“众所周知”的体现。SIGABRT 不再是崩溃，而是“用户按了 Ctrl-Alt-Del，请重启”；SIGUSR1 不再是自定义信号，而是“电池快没电了，请关机”。两者都只是 fork 一个 `/sbin/shutdown` 子进程，参数分别是 `-r` 与 `-p`，自己立刻返回。这种“信号转进程”的设计避免在信号上下文里做实事，与 02 的桥模型一致。

### 1.1 小结

全系列终点：init 是终点不是中心，契约稳定压倒功能膨胀。01~14 闭环。

---

## 2. C 源码分析

### 2.1 两个挂钩

```c
/* init.c:517-538 */
minixreboot: fork==0 → execl("/sbin/shutdown","shutdown","-r","now","CTRL-ALT_DEL") → _exit(1)
minixpowerdown: fork==0 → execl(...,"-p",...) → _exit(1)
```

父进程（init 本人）不等待，直接返回继续主循环。`_exit(1)` 只影响 exec 失败的子进程。

### 2.2 跨服务契约（13 项，grep 实证）

| 契约 | 证据 |
|---|---|
| boot 镜像最后 | `kernel/table.c:64` |
| USR_F 非服务 | `rs/table.c:28` |
| PM 父即自身/INIT_PID/调度 | `pm/main.c:188-204` |
| 孤儿收养 | `pm/forkexit.c:396` |
| INIT 死只栈回溯 | `pm/forkexit.c:336-341` |
| reboot 停 init | `pm/misc.c:224` |
| RS 父亦 INIT | `pm/main.c:203` |
| 调度继承 | `pm/schedule.c:34,73` |
| procfs 非服务 | `procfs/service.c:195-207` |
| boot argv 固定 | `vm/main.c:345` |
| ELF 由 VM 加载 | `vm/main.c:498-514` |
| Ctrl-Alt-Del 发 SIGABRT | `keyboard.c:300` |
| 低电发 SIGUSR1 | `tps65217.c:226` |

---

## 3. Rust 设计决策

信号到关机请求的映射是纯函数，shutdown 参数组装是纯函数，契约表是静态数据。fork/exec 由调用方执行。与 Redox 的电源管理对照：同样信号转进程，但参数取 Minix 的 shutdown 方言。

---

## 4. 实现详解

模块 `contracts.rs`；差异：execl 改为 argv 数据；父不等待语义保留为注释。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_sigabrt_requests_reboot` | init.c:517-525 |
| `test_sigusr1_requests_powerdown` | init.c:530-538 |
| `test_other_signal_none` | transition_handler 分流 |
| `test_reboot_argv` | shutdown -r now |
| `test_powerdown_argv` | shutdown -p now |
| `test_boot_argv_contract` | vm/main.c:345 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：87 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/contracts.rs`。

---

## 6. 过渡（全系列闭环）

01 发射，02 铺轨，03 给嗓门，04~06 启动三站，07/08 建会话与索引，09 稳态，10 重读，11 关停，12/13 交互与账本，14 契约。状态机主线与会话次主线在此汇合，01~14 全部完成。

---

## 7. 参见

- `01-init-main-entry.md` — boot argv。
- `02-init-state-machine.md` — 信号分流。
- C 源码：`minix3/sbin/init/init.c:517-538` 与 §2.2 表。
