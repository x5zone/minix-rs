# 03-init-logging-failure：日志三件套与致命信号

> **定位**：`stall`（`minix3/sbin/init/init.c:440-450`）、`warning`（457-466）、`emergency`（472-481）、`disaster`（504-511）。明确排除 `print_console`（411-432，`#if 0` 死代码）与 `badsys`（490-498，非 Minix 分支）。
> **Rust**：`os/commands/sbin/init/src/log.rs`。
> **前置依赖**：02（`disaster` 的注册位置）。
> **本篇不覆盖（移交）**：状态转换（见 02）、重启与关机挂钩（见 14）。

---

## 1. 概念：给人留出读屏时间的日志

init 的日志有一个特殊读者：站在物理控制台前的值班管理员。当系统起不来、终端刷屏、错误一闪而过时，管理员需要的是“停下来让我看清”，而不是“记下来稍后查”。`stall()` 的名字很直白：记一条告警，然后睡 30 秒。这 30 秒（`STALL_TIMEOUT`，`init.c:95`）不是重试退避，不是限流，而是读屏时间。慢性故障下每次循环都停 30 秒，既让人看清，也避免日志淹没硬盘。

三级日志的分工按“需不需要停”划分。`stall` 是“记下来并停”，用于执行失败但还能继续的场景（比如某个 getty 起不来）。`warning` 是“记下来不停”，用于参数小毛病（未知 flag、多余参数）。`emergency` 是“记最重的级别”，用于内核安全级别改不动这类大事。级别上 `stall` 与 `warning` 都是 `LOG_ALERT`，`emergency` 是更高的 `LOG_EMERG`，`disaster` 是“记完就以死谢罪”：收到本不该来的致命信号，先记一条“fatal signal”，睡 30 秒让人看清，然后以信号号为退出码结束自己，触发重启。

源码里还有两处“诚实但未完成”的痕迹。三处函数注释都写着“NB: should send a message to the session logger to avoid blocking”，意思是日志应该发给会话记录器避免阻塞，但从未实现。`print_console` 整段被 `#if 0` 包起来，注释里坦言 syslog 在纯控制台场景下不好用。这些都说明 init 的日志是个够用就好的子系统，Rust 侧如实保留其语义，不擅自“补完” session logger。

### 1.1 本章不讲什么

- 信号如何变成状态请求（见 02）。
- 重启与关机的 Minix 特有挂钩（见 14）。

### 1.2 小结

记住“停 30 秒是给人看的”就行。下一章看 C 代码四段十行如何落实这一思想。

---

## 2. C 源码分析

### 2.1 常量

| 常量 | 值 | 位置 | 用途 |
|---|---|---|---|
| `STALL_TIMEOUT` | 30 | `init.c:95` | stall/disaster 睡眠秒数 |
| `LOG_ALERT` | syslog 级别 | `init.c:446,463` | stall/warning |
| `LOG_EMERG` | syslog 级别 | `init.c:478` | emergency/disaster |

### 2.2 stall：记加睡

```c
/* init.c:440-450 */
static void stall(const char *message, ...) {
    va_list ap;
    va_start(ap, message);
    vsyslog(LOG_ALERT, message, ap);
    va_end(ap);
    closelog();
    (void)sleep(STALL_TIMEOUT);
}
```

每次调用都 `openlog` 在前（`main` 中一次，`init.c:278`）、`closelog` 收尾。返回值全部忽略，日志失败不设失败路径。

### 2.3 warning 与 emergency：不睡的两个兄弟

`warning`（`init.c:457-466`）与 `stall` 仅差最后一行睡眠。`emergency`（`init.c:472-481`）把级别换成 `LOG_EMERG`。三者的注释各带一句 session logger 的 NB，语义相同，阅读时合并理解。

### 2.4 disaster：致命信号的遗言

```c
/* init.c:504-511 */
static void disaster(int sig) {
    emergency("fatal signal: %s", strsignal(sig));
    (void)sleep(STALL_TIMEOUT);
    _exit(sig);
}
```

Minix 分支下它处理 `SIGFPE/SIGILL/SIGSEGV/SIGBUS`（见 01 S7）。`_exit(sig)` 的退出码即信号号，PM 侧将其理解为重启请求。注意它用 `_exit` 而非 `exit`，跳过 stdio 冲刷，符合信号上下文的谨慎原则。

### 2.5 明确排除

| 项 | 依据 | 处理 |
|---|---|---|
| `print_console`（`init.c:411-432`） | `#if 0` 包裹的死代码 | 丢弃，不建模 |
| `badsys`（`init.c:490-498`） | `#if !defined(__minix)` 非 Minix 分支 | 丢弃，不建模 |

---

## 3. Rust 设计决策

### 3.1 总览

日志是 init 的基础设施，但 Rust 侧不引入 syslog 客户端（minix-rs 暂无 syslog 服务，**[ARCH A-3]** 缺口 defer）。设计收敛为三样东西：严重度枚举、日志接收 trait、时钟 trait。时间可注入让 `stall` 的 30 秒在单测里不真睡，这是与 Redox 日志抽象一致的做法（Redox 同样把 sleep 注入以便测试），但我们没照搬 Redox 的整套 slog 门面，因为 init 只需要 alert/emerg 两级。

### 3.2 决策一：Severity 两级

```rust
pub enum Severity { Alert, Emerg }
```

`stall/warning→Alert`，`emergency/disaster→Emerg`，与 C 的 syslog 级别逐行对照。未来 syslog 服务落地时再做 `Severity→syslog::Priority` 映射，当前 Console 实现直写标准错误。

### 3.3 决策二：LogSink 双实现

`FakeLogSink` 内存记录 `(Severity, String)` 供断言；`ConsoleLogSink` 预留直写通道。C 的 NB（session logger）转述为注释，不实现。

### 3.4 决策三：睡眠注入与永不返回的诚实表达

`stall()` 签名取 `clock: &dyn Clock`，Fake 时钟只记录秒数。`disaster()` 返回 `DisasterAction::ExitWith(i32)` 数据而非直接 `_exit`，退出动作上移调用方，单测可断言遗言内容与退出码。

---

## 4. 实现详解

### 4.1 模块结构

```text
os/commands/sbin/init/src/log.rs — Severity / LogSink / Fake+Console / Clock / stall/warning/emergency/disaster
```

### 4.2 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| 全局 vsyslog 直调 | LogSink trait 注入 | 可测性演进 |
| sleep(30) 硬编码等待 | Clock trait，Fake 不真睡 | 测试性演进 |
| disaster 直接 _exit | 返回 ExitWith 数据 | 库策略分离 |
| session logger NB | 注释转述不实现 | 诚实缺口 |

---

## 5. 测试要点

| 测试 | 覆盖 | C 对照 |
|---|---|---|
| `test_stall_logs_alert_and_sleeps_30` | 级别加睡眠 | init.c:440-450 |
| `test_warning_logs_alert_without_sleep` | 不睡 | init.c:457-466 |
| `test_emergency_logs_emerg` | 最高级 | init.c:472-481 |
| `test_disaster_records_and_requests_exit` | 遗言加退出码 | init.c:504-511 |
| `test_console_sink_never_panics_on_empty` | 空消息稳健 | 稳健性 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：25 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/log.rs`。

---

## 6. 过渡

基础设施就绪：01 给身份，02 给轨道，03 给嗓门。从 04 起进入启动序列三站，stall 将在每站的失败路径里反复出场。

---

## 7. 参见

- `02-init-state-machine.md` — disaster 的注册位置。
- `04-init-single-user.md`、`05-init-runcom.md`、`09-init-multi-user.md` — stall 调用点。
- C 源码：`minix3/sbin/init/init.c:440-511`。
