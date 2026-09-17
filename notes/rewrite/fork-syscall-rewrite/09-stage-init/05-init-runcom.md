# 05-init-runcom：运行启动脚本

> **定位**：状态 `'r'`，`runcom`（`minix3/sbin/init/init.c:974-1014`）、`runetcrc`（879-969）。
> **Rust**：`os/commands/sbin/init/src/runcom.rs`。
> **前置依赖**：04（FASTBOOT 语义来源）、02（状态字符）、03（stall/warning/emergency）。
> **本篇不覆盖（移交）**：`shouldchroot` 与 chroot 机制（见 12）、`setctty`/`collect_child`（见 09）、utmp 记录（见 13）。

---

## 1. 概念：自动化与手工的分水岭

单用户是手工抢修，runcom 是自动化启动。`/etc/rc` 是一个 shell 脚本，负责挂载文件系统、启动守护进程、配置网络这些千篇一律的事。init 不关心脚本里写了什么，只关心它如何结束：退出码为零意味着自动化成功，下一站读终端表；非零或被信号杀死意味着自动化失败，退回单用户让人手工接管。这种“只看出口，不看过程”的契约让 rc 脚本可以任意演化而不必改 init。

`autoboot` 与 `fastboot` 是传给脚本的唯一参数。autoboot 做全量检查（慢而稳），fastboot 跳过文件系统检查（快而险）。04 的出口置 FASTBOOT 正是“刚抢修完别再全查一遍”的体贴。chroot 逻辑是 Minix 的第二遍启动：如果 `init.root` 指向非根目录，先在当前根下跑一遍 rc（装好新根所需驱动），再 chroot 进去跑真正的 rc。两次执行的退出码各自独立判断，任一次失败都回单用户。

### 1.1 小结

rc 脚本是黑盒，退出码是唯一的通信协议。下一章读终端表，把黑盒启动的系统变成可登录的系统。

---

## 2. C 源码分析

### 2.1 argv 组装

```c
/* init.c:897-900 */
argv[0] = "sh"; argv[1] = _PATH_RUNCOM;   /* /etc/rc */
argv[2] = (runcom_mode == AUTOBOOT ? "autoboot" : 0);
argv[3] = 0;
```

fastboot 时 `argv[2]` 为空指针，即只传两个参数。`_PATH_RUNCOM` 即 `/etc/rc`（`pathnames.h:40`）。

### 2.2 子进程六步

忽略 TSTP/HUP → `setctty(_PATH_CONSOLE)` → 组 argv → 解屏蔽 → 可选 chroot（失败 `_exit(4)`）→ `execv(INIT_BSHELL)`（失败 stall 后 `_exit(5)`）。`_exit(4/5)` 的非零码都导向 single_user，见 §2.3。

### 2.3 五归宿

| 条件 | 行号 | 归宿 |
|---|---|---|
| fork 失败 | 917-923 | emergency 后睡 30 秒，回 single_user |
| stop | 941-946 | SIGCONT 后继续等 |
| catatonia 请求加 SIGTERM | 949-957 | 静默 sigsuspend 等重启 |
| 非正常结束或非零退出 | 959-966 | 回 single_user |
| 零退出 | 968 | 进 read_ttys |

### 2.4 runcom 两次执行

`runetcrc(0)` 失败直接返回；成功且 `shouldchroot()` 为真则 `runetcrc(1)`，成功置 `did_multiuser_chroot=1`，否则 0（`init.c:990-998`）。无论是否 chroot，成功后 `runcom_mode` 重置为 AUTOBOOT（`init.c:1005`），utmp 的 reboot 记录点（`init.c:1007-1012`）移交 13。

---

## 3. Rust 设计决策

`rc_argv(mode)` 返回 `ParsedCommand`：exec 路径是 `/bin/sh`（C 的 `INIT_BSHELL`，init.c:105/913），argv[0] 是裸名 `sh`（init.c:899-900），fastboot 时截断第三个参数；`classify_rc_exit(WaitStatus, catatonia_requested)` 把子进程结局映射为 `RcOutcome`——静默重启需要 catatonia 请求与 SIGTERM 两个条件同时成立（init.c:949-957），这是文档里最容易读漏的一处；两次执行的顺序语义由 `runcom` 实体承接：`runetcrc` 是一次尝试，`runcom` 是「跑一次、也许 chroot 再跑一次、写台账」的策略层，chroot 判定复用 12 的 `should_chroot`。exec 回退与 stall 通道复用 03。与 Redox 对照：Redox 的 rc.d 风格是逐脚本执行，Minix 是单脚本加参数，我们保留单脚本语义不硬套。

---

## 4. 实现详解

模块 `runcom.rs`，实体分两层（P0-2b）。`runetcrc` 是一次尝试：子分支忽略 SIGHUP/SIGTSTP、占控制台、解屏蔽、可选 chroot（失败 `_exit(4)`）、exec（失败 stall 30 秒后 `_exit(5)`）；父循环收集每个收场的子进程，只认 rc 自己的 pid。`runcom` 是策略层：先跑一次，`should_chroot` 为真再在 chroot 里跑第二次，成功后写 reboot 台账（`deps.record_reboot`，接线到 13 的 utmp sink）并交出 `did_multiuser_chroot`。与 C 的差异：全局 `runcom_mode` 与 `did_multiuser_chroot` 变成参数与返回值；`_exit(4/5)` 留在子分支原样发散（剧本宿主记录退出码）；注意 fork 失败这条路 C 会先睡一个 STALL_TIMEOUT 再回 single_user——single_user 自己的 fork 失败没有这一睡，两处不可混淆。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_rc_argv_autoboot_has_third` | init.c:899 |
| `test_rc_argv_fastboot_truncated` | init.c:899 |
| `test_rc_exec_path_is_shell_binary_argv0_is_sh` | init.c:899-900/913 路径与 argv[0] 分离 |
| `test_runetcrc_child_exec_request_carries_autoboot` | init.c:884-910 子分支 + `_exit(5)` |
| `test_runetcrc_chroot_failure_exits_four` | init.c:903-906 `_exit(4)` |
| `test_runetcrc_chrooted_child_execs_after_chroot` | chroot 先于 exec、fastboot 无第三参 |
| `test_runetcrc_fork_failure_sleeps_then_single_user` | init.c:911-922（含睡 30 秒） |
| `test_runetcrc_stopped_shell_continues_then_succeeds` | init.c:943-946 SIGCONT |
| `test_runetcrc_sigterm_with_catatonia_awaits_reboot` | init.c:949-957 |
| `test_runetcrc_sigterm_without_catatonia_is_single_user` | 双条件反例 |
| `test_runcom_double_run_inside_chroot` | init.c:986-999 两次运行 + 单条台账 |
| `test_runcom_single_run_without_chroot` | init.c:1000-1002 |
| `test_runcom_rc_failure_propagates_without_ledger` | 失败不写台账 |

### 5.1 测试统计（截至 2026-09-18）

- `cargo test -p minix-init`：143 个通过（全 crate 口径），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/runcom.rs`。

---

## 6. 过渡

runcom 成功即进 read_ttys。下一站 06 解析 `/etc/ttys`，把“能跑的系统”变成“能登录的系统”。

---

## 7. 参见

- `04-init-single-user.md` — FASTBOOT 来源。
- `06-init-read-ttys.md` — read_ttys。
- `12-init-sysctl-interaction.md` — shouldchroot 机制。
- C 源码：`minix3/sbin/init/init.c:879-1014`。
