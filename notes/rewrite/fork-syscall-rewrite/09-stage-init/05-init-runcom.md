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

`rc_argv(mode)` 纯组装 fastboot 时截断第三个参数；`classify_rc_exit` 把退出状态映射为 `RcOutcome`；`RuncomFlow` 记录两次执行的顺序语义，chroot 判定依赖 12 的 trait。exec 回退与 stall 通道复用 03。与 Redox 对照：Redox 的 rc.d 风格是逐脚本执行，Minix 是单脚本加参数，我们保留单脚本语义不硬套。

---

## 4. 实现详解

模块 `runcom.rs`；差异：全局 `runcom_mode` 改为参数传递；`_exit(4/5)` 合并为 `SingleUser` 数据；chroot 副作用由调用方执行。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_rc_argv_autoboot_has_third` | init.c:899 |
| `test_rc_argv_fastboot_truncated` | init.c:899 |
| `test_zero_exit_goes_read_ttys` | init.c:968 |
| `test_nonzero_goes_single_user` | init.c:965-966 |
| `test_abnormal_goes_single_user` | init.c:959-963 |
| `test_catatonia_sigterm_quiets` | init.c:949-957 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：38 个通过（累计），0 失败。
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
