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

### 3.3 等待结局枚举与状态实体

`classify_wait(WaitStatus, requested)` 纯函数吃 `wait.rs` 的 `WaitStatus`（C 宏族的 Rust 和类型），五个结局与 C 等待循环逐一对照；stopped/请求/信号阶梯/零退出的先后次序就是 init.c:827-870 的次序，其中"零退出"不看退出码——C 只测 WIFEXITED，^D 的 0 和 shell 的 1 走同一条 FASTBOOT 之路。

状态本体（P0-2a）补齐 C 的三幕结构：父幕降级安全级、SIG_IGN 窗口、fork 与等待收集；子幕在控制终端上要口令、问备用 shell、exec 双次兜底。剧本宿主把 `fork_outcomes` 排成队，`Ok(0)` 把测试送进子分支——C 的"fork 返回两次"在测试里是"脚本回放两次"。子进程的 `_exit` 无法在测试里真实发生，剧本宿主记录退出码后以 panic 收场，测试用 catch_unwind 包住调用再断言 `exits` 队列——发散语义诚实，断言不缺席。

---

## 4. 实现详解

模块 `single_user.rs`。状态函数本体是 `single_user(host, deps) -> SingleUserOutcome`，fork 语义原样保留（`Ok(0)` 走子分支）；与 C 的差异：`requested_transition` 全局变成 `deps.requested` 询问闭包，`collect_child` 变成 `deps.collect` 回调（每个收到的子进程都喂给它，包括 shell 自己的 pid——会话表会忽略陌生 pid），口令匹配是 `deps.verify_password` 注入的判定闭包（crypt 后端见 12/ARCH A-12）。`SingleUserOutcome::AwaitReboot` 对应 C 的 `sigfillset` + `for(;;) sigsuspend`——那个状态函数从不返回，驱动层对它停泊即可。子分支的 `exec` 兜底是双次的：第一次带 altshell 或 `-sh`，失败后重置 `argv[0]` 为 `-sh` 重试 `INIT_BSHELL`——即使第一次已经是它（init.c:803-808 的原样语义），最后裸 `sleep(30)` 加 `_exit(3)`。

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
| `test_wait_other_signal_restarts_single_user` | init.c:857-863 |
| `test_wait_normal_proceeds_runcom_fastboot` | init.c:866-870（退出码不参与） |
| `test_entity_happy_path_runs_shell_then_fastboot` | wait 循环主干 |
| `test_entity_downgrades_securitylevel_before_fork` | init.c:711-713 |
| `test_entity_fork_failure_retries` | init.c:814-821 |
| `test_entity_ignores_foreign_children_until_shell_exits` | init.c:825-874 的 `wpid != pid` 续等 |
| `test_entity_sigkill_death_awaits_reboot` | init.c:849-856 |
| `test_entity_requested_transition_wins_after_shell_exit` | init.c:846-848 |
| `test_entity_child_branch_gates_then_execs_default_shell` | init.c:721-813 子分支 + PATH |
| `test_entity_child_branch_matching_password_reaches_altshell_prompt` | init.c:731-789 + 双 exec 兜底 |
| `test_entity_child_branch_wrong_password_reprompts` | init.c:764-770 |
| `test_entity_child_branch_eof_on_password_exits_zero` | init.c:758-760 的 ^D `_exit(0)` |
| `test_ignore_then_restore_shapes_are_data` | SIG_IGN 窗口与 satstp/sahup 恢复 |

### 5.1 测试统计（截至 2026-09-18）

- `cargo test -p minix-init`：122 个通过（累计），0 失败。
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
