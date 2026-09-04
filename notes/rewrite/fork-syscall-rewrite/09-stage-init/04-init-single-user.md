# 04-init-single-user：单用户抢修态

> **定位**：状态 `'s'`，`single_user`（`minix3/sbin/init/init.c:694-877`）。
> **Rust**：`os/commands/sbin/init/src/single_user.rs`。
> **前置依赖**：02（状态机骨架）、03（stall/warning/emergency 通道）。
> **本篇不覆盖（移交）**：`getsecuritylevel/setsecuritylevel` 机制（见 12）、`setctty` 机制（见 09）、`collect_child` 机制（见 09）、`/etc/rc`（见 05）。

---

## 1. 概念：带口令门的最小 shell

单用户模式是操作系统的抢修通道。正常启动走 `/etc/rc` 自动执行几十个步骤，任何一步出错都可能把系统卡死。单用户模式跳过一切自动化，直接给管理员一个 root shell，修好再手动进入多用户。这就是为什么它的退出语义是“退出即前进”：shell 正常退出意味着抢修结束，下一站是 runcom（并置 FASTBOOT，因为刚修过的系统不宜再做全量检查）。

口令门是单用户最有争议的设计。物理控制台前的人默认可信吗？Minix 的回答是条件信任：只有当 console 条目标为非 secure，或内核此前处于较高安全级别，且 root 设有口令时，才要求输入口令。空输入（Ctrl-D）的语义不是“跳过口令”，而是“放弃抢修，直接进多用户”——这对应 `_exit(0)` 的正常退出路径。ALTSHELL 则是另一个务实主义：允许管理员临时指定 shell 路径，默认回退到 `/bin/sh`，exec 失败记 emergency 后重试。

父进程的等待循环体现了“看住 shell”的职责：shell 被暂停就唤醒它，被杀就安静等待重启（`/sbin/reboot` 走 SIGKILL），崩溃就重启单用户，收到外部状态请求就交出控制权。五种结局各有归宿，没有一处是“崩了就 panic”。

### 1.1 本章不讲什么

- runcom 如何执行 `/etc/rc`（见 05）。
- 会话与控制终端的底层机制（见 07、09）。

### 1.2 小结

单用户等于“口令门加 shell 加看护循环”。记住退出即前进，下一章进 runcom。

---

## 2. C 源码分析

### 2.1 父进程三段

| 段 | 行号 | 行为 |
|---|---|---|
| 准备 | 715-731 | 清 chroot 标记；安全级别降级；忽略 TSTP/HUP 并保存旧动作 |
| fork | 732-823 | 子进程建会话开 shell；父进程 fork 失败记 emergency 后返回 single_user 重试 |
| 看护 | 825-873 | waitpid 循环五结局（见 §2.4） |

### 2.2 安全级别降级

```c
/* init.c:723-725 */
from_securitylevel = getsecuritylevel();
if (from_securitylevel > 0) setsecuritylevel(0);
```

抢修态必须可写一切，故先降到 0。查询与设置机制见 12，本篇只确认调用顺序。

### 2.3 子进程：口令门与 shell 选择

SECURE 口令门（`init.c:747-763`）：`console` 条目非 secure 或此前级别≥2，且 root 有口令时才提问；空输入 `_exit(0)`；错一次记 warning 后重试。ALTSHELL（`init.c:768-783`）：提示输入路径，空行取默认。exec 顺序：altshell（如有）→ `INIT_BSHELL`（`init.c:803-808`），失败记 emergency、睡 30 秒、`_exit(3)`。

### 2.4 看护循环五结局

| 条件 | 行号 | 归宿 |
|---|---|---|
| shell 被暂停 | 836-840 | SIGCONT 后继续等 |
| 外部状态请求 | 843-847 | 恢复信号动作后跳转 |
| SIGKILL 杀死 | 849-856 | 静默 sigsuspend 等重启 |
| 其他信号杀死 | 857-863 | 重启 single_user |
| 正常退出 | 866-870 | 置 FASTBOOT，进 runcom |

`WUNTRACED` 让暂停也可见；`EINTR` 重试；其他 wait 错误记 warning 后重启 single_user（`init.c:829-835`）。

---

## 3. Rust 设计决策

### 3.1 口令门纯决策

`password_gate_required(console_secure, from_level, root_has_password)` 实现 `typ && (level>=2 || !secure) && pp && 有口令` 的布尔化；`classify_attempt` 把空输入映射为退出、匹配映射为成功、不匹配映射为重试。crypt 与内存清零由平台层实现，不在本篇伪造。

### 3.2 shell 选择纯函数

`choose_shell(input, default)` 去空白后空则取默认。与 Redox 的 shell 回退链思路一致，但路径常量取 Minix 的 `INIT_BSHELL`。

### 3.3 等待结局枚举

`WaitOutcome::{Continue, Transition, RestartSingleUser, RebootQuiet, ProceedRuncom}` 加 `classify_wait(stopped, requested, signaled, termsig, exited)` 纯函数，五结局单测全覆盖。`ProcessOps` trait 收敛 fork/exec/wait/kill，Live 待 A-9，Fake 剧本驱动，双实现满足 trait 规则。

---

## 4. 实现详解

模块 `single_user.rs`；与 C 差异：全局写改为返回值；`_exit` 改为动作枚举；信号保存恢复语义保留为注释。`runcom_mode=FASTBOOT` 的副作用显式为 `ProceedRuncom { fastboot: true }` 数据。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_gate_requires_password_matrix` | init.c:749-750 |
| `test_empty_input_exits` | init.c:755-756 |
| `test_choose_shell_default_and_alt` | init.c:781-782 |
| `test_wait_stop_continues` | init.c:836-840 |
| `test_wait_requested_transitions` | init.c:843-847 |
| `test_wait_sigkill_quiets` | init.c:849-856 |
| `test_wait_normal_proceeds_runcom_fastboot` | init.c:866-870 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：32 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/single_user.rs`。

---

## 6. 过渡

单用户出口通向 runcom。下一站 05 看 `/etc/rc` 如何执行，以及 FASTBOOT 跳过什么。

---

## 7. 参见

- `02-init-state-machine.md` — 状态字符与主循环。
- `05-init-runcom.md` — runcom 与 FASTBOOT 语义。
- `09-init-multi-user.md` — setctty 与 collect_child 机制。
- `12-init-sysctl-interaction.md` — 安全级别机制。
- C 源码：`minix3/sbin/init/init.c:694-877`。
