# 02-init-state-machine：状态机骨架与信号转换

> **定位**：状态机主循环 `transition`（`minix3/sbin/init/init.c:624-640`）与信号注册转换（`handle` 369-389、`delset` 394-405、`transition_handler` 1502-1522、`alrm_handler` 1649-1655），7 状态字符（`init.c:133-139`）。
> **Rust**：`os/commands/sbin/init/src/state_machine.rs`。
> **前置依赖**：01（入口调用点与 `InitialState` 子集）。
> **本篇不覆盖（移交）**：各状态函数体（见 04~11）、日志与 disaster（见 03）、utmpx runlevel 挂钩（见 13）、重启与关机挂钩（见 14）。

---

## 1. 概念：为何 init 是一台状态机，而非一堆 if

init 的一生有两个显著特征。第一，它要处理的事情很少，只有七件：单用户、跑启动脚本、读终端表、多用户、重读终端表、假死、关机。第二，事情之间的切换条件很稀疏：大部分时间它都在多用户状态里睡觉，只有收到特定信号或子进程退出时才需要换状态。

这种“状态少、转换稀、每次只做一件事”的程序，用一堆散落的 `if (got_sighup)` 来写会迅速腐烂：每个循环都要重复检查所有条件，新增状态要改所有分支。状态机的解法是把“现在处于什么状态”和“收到什么事件”显式建模：主循环只做一件事——运行当前状态函数，拿到下一个状态，再运行。信号处理函数不直接做事，只写一个“请求变量”，主循环在合适的时机认领它。异步世界（信号随时到来）与同步世界（一次只做一件事）之间只有一座桥，就是 `requested_transition` 这个变量。

理解了这座桥，C 代码的每个怪异之处都有了解释。为什么 `transition_handler` 里只有赋值没有动作？因为信号上下文里做大事不安全，记下来就走。为什么有个 `default: requested_transition = 0`？因为未知信号意味着“没有请求”，清零表示继续当前状态。为什么 `alrm_handler` 只置一个 `clang` 标志？因为闹钟只是关机倒计时的滴答声，真正的倒计时逻辑在 `death()` 里认领它。这些设计在 Rust 侧被保留，但用类型系统表达得更直白。

> **架构范围**：本章是 POSIX 风格状态机共性。`SIGABRT/SIGUSR1` 的 Minix 特有挂钩只列位置，语义见 14。

### 1.1 本章不讲什么

- 七个状态各自做什么（见 04 单用户、05 runcom、06 read_ttys、09 multi_user、10 clean_ttys、11 shutdown）。
- `disaster` 与日志通道（见 03）。
- `utmpx_set_runlevel` 的每次跳转挂钩（见 13）。

### 1.2 小结

记住桥的模型：信号只许写请求，不许直接开车；主循环只认请求，不直接收信号。下一章看 C 代码如何用七个字符与三个函数搭起这座桥。

---

## 2. C 源码分析

### 2.1 类型体操：返回函数指针的函数指针

```c
/* init.c:130-131 */
typedef long (*state_func_t)(void);
typedef state_func_t (*state_t)(void);
```

直译是“`state_t` 是返回 `state_func_t` 的函数指针”。C 之所以绕这么大弯，是因为 ISO C 不允许直接递归 typedef，只能用 `long` 的宽度兜底保证能装下函数指针。Rust 侧不需要这种体操：状态是枚举，状态函数是普通函数，映射由 `match` 完成，编译器保证穷尽。

### 2.2 七个状态字符

```c
/* init.c:133-139 */
#define DEATH       'd'
#define SINGLE_USER 's'
#define RUNCOM      'r'
#define READ_TTYS   't'
#define MULTI_USER  'm'
#define CLEAN_TTYS  'T'
#define CATATONIA   'c'
```

字符取值即文档通信协议的一部分：`get_runlevel()`（见 13）把它们翻译成 utmp runlevel 字符。大小写敏感（`t` 读表与 `T` 重读表是两个状态），Rust 侧 `as_char/from_char` 逐字对照，单测覆盖全部七个双向转换。

### 2.3 注册：handle 与 delset

`handle(handler, ...)`（`init.c:369-389`）用可变参数批量注册：对每个信号填 `sa_mask` 为全集（handler 执行期间屏蔽一切），`sa_flags` 仅对 `SIGCHLD` 置 `SA_NOCLDSTOP`（子进程暂停不停机不通知，避免调试暂停惊扰 init），然后逐个 `sigaction`。源码留有一句诚实的注释 `/* XXX SA_RESTART? */`（`init.c:384`），意思是作者不确定是否该让慢系统调用被信号打断后自动重启。这个疑问在 Minix 上游从未解决，Rust 侧如实保留为注释，不擅自“修复”。

`delset(maskp, ...)`（`init.c:394-405`）做相反的事：从全集掩码里删掉需要放行的信号。两者配合完成 01 所述 S7：先全屏蔽，再放行已注册的八类，外加忽略 `SIGTTIN/SIGTTOU`。

### 2.4 主循环：transition

```c
/* init.c:624-640 */
static void
transition(state_t s)
{
    if (s == NULL)
        return;
    for (;;) {
        utmpx_set_runlevel(get_runlevel(current_state), get_runlevel(s));
        current_state = s;
        s = (state_t)(*s)();
    }
}
```

空指针直接返回（防御性编程，正常路径永不触发）。循环体内先记 runlevel 再推进，`current_state` 初始为 `death`（`init.c:201`，`SUPPORT_UTMPX` 构建）。`LETS_GET_SMALL` 构建下循环体退化为纯推进（无 utmp 挂钩）。Rust 侧把推进语义提炼为 `TransitionDriver`，utmp 挂钩移交 13。

### 2.5 转换：transition_handler 与 alrm_handler

```c
/* init.c:1502-1522 */
static void
transition_handler(int sig)
{
    switch (sig) {
    case SIGHUP:  requested_transition = clean_ttys; break;
    case SIGTERM: requested_transition = death;      break;
    case SIGTSTP: requested_transition = catatonia;  break;
    default:      requested_transition = 0;          break;
    }
}
/* init.c:1649-1655 */
static void
alrm_handler(int sig) { clang = 1; }
```

三映射加一个清零，`clang`（`init.c:173`）是关机倒计时的滴答标志。注意 `transition_handler` 不处理 `SIGABRT/SIGUSR1/SIGALRM/致命信号`，它们各有专属 handler（见 03/14）。这种“按信号分流到不同 handler，再汇总为状态请求”的两级结构，正是 Rust 侧 `Signal → Option<StateKind>` 纯函数加 `HandlerKind` 分类的由来。

---

## 3. Rust 设计决策

### 3.1 总览：把异步请求变成可测试的纯函数

C 用全局变量桥接异步与同步，Rust 用返回值桥接。信号到状态的映射是纯函数，可单测；主循环是 trait，可用假驱动单测步数；注册表是 trait，Live 与 Fake 双实现分别对应生产与测试。与 Redox 的对照：Redox 的状态驱动服务同样避免在信号上下文做实事，我们借鉴其“记录加认领”模式，但状态载体用枚举而非 Redox 式的字符串配置，因为 Minix 的七字符是编译期常量，枚举的穷尽检查更有价值。

### 3.2 决策一：StateKind 七状态枚举

```rust
pub enum StateKind { Death, SingleUser, Runcom, ReadTtys, MultiUser, CleanTtys, Catatonia }
```

`as_char()` 返回 C 字符，`from_char()` 做逆向，非法字符返回 `None`。01 的 `InitialState` 是它的子集视图，02 落地后 01 的决策可无损升级（见 §4.3 差异说明）。

### 3.3 决策二：signal_to_state 纯函数

```rust
pub fn signal_to_state(sig: Signal) -> Option<StateKind>
```

`Signal` 是 `Sighup/Sigterm/Sigstp/Other(i32)` 的枚举，避免裸 int。映射表与 C 三分支逐行对照，`default` 对应 `None`。调用方（driver）负责把 `Some` 写进请求槽、`None` 理解为清零，语义与 C 一致但数据流显式。

### 3.4 决策三：TransitionDriver 可步进主循环

C 的 `for(;;)` 无法单测，Rust 的 `run(max_steps)` 允许测试跑固定步数后停下断言轨迹。`step()` 返回 `None` 表示“当前状态函数要求停机”（对应 C 的返回空指针），测试可覆盖这条罕见路径。

### 3.5 决策四：SignalRegistry 双实现

`FakeSignalRegistry` 记录注册表供断言；`LiveSignalRegistry` 预留真实 `sigaction` 接线，当前返回 defer 错误（A-8 缺口显式化）。`SA_NOCLDSTOP` 语义写进 Live 注释，`SA_RESTART` 疑问如实转述，不虚构结论。

---

## 4. 实现详解

### 4.1 模块结构

```text
os/commands/sbin/init/src/
  entry.rs          — 01（本篇复用 InitialState→StateKind 升级见 §4.3）
  state_machine.rs  — 本篇：StateKind / Signal / signal_to_state / TransitionDriver / SignalRegistry
```

设计决策引用：文件头标 `design §1.1~§1.4`；每个映射分支注释 C 行号。

### 4.2 与 C 步骤的差异说明

| C | Rust | 分类 |
|---|---|---|
| `state_t` 函数指针（init.c:130-131） | `StateKind` 枚举加 `match` 分发 | 类型安全演进 |
| 全局 `requested_transition` 写 | `signal_to_state` 返回 `Option`，写动作上移 | 设计决策（可测性） |
| `transition` 内嵌 utmp 更新（init.c:633-635） | driver 不含 utmp，13 提供挂钩 | 职责分离（调用点 vs 机制） |
| `XXX SA_RESTART?` 未决（init.c:384） | 注释如实转述，不擅改 | 诚实表达不确定性 |

### 4.3 01 兼容：InitialState 到 StateKind

01 的 `InitialState::{Runcom, SingleUser}` 分别等于 `StateKind::{Runcom, SingleUser}`。`entry::decide_entry` 保持不动，02 新增 `EntryDecision::to_state_kind()` 桥接，避免修改已 CONVERGED 的 01 代码。

---

## 5. 测试要点

| 测试函数 | 覆盖 | C 对照 |
|---|---|---|
| `test_state_chars_roundtrip` | 七字符双向 | init.c:133-139 |
| `test_signal_to_state_maps` | 三映射 | init.c:1508-1516 |
| `test_signal_to_state_default_none` | default 清零 | init.c:1518-1520 |
| `test_fake_registry_records` | 注册记录 | init.c:369-389 |
| `test_driver_runs_fixed_steps` | 主循环步进 | init.c:624-640 |
| `test_driver_stops_on_none` | 空指针返回路径 | init.c:628-629 |
| `test_alarm_flag_set_and_clear` | clang 语义 | init.c:1649-1655 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：20 个通过（01 的 13 个加本篇 7 个），0 失败。
- 完整清单：`rg "fn test_" os/commands/sbin/init/src/state_machine.rs`。

---

## 6. 过渡

骨架已立：01 给出发射台，02 给出轨道与道岔。从 04 起逐个填充车站——单用户、runcom、读表、多用户、重读、假死、关机。读 04 之前记住桥的规则：任何状态函数想换轨，只能写请求，不能直接跳。

---

## 7. 参见

- `01-init-main-entry.md` — S7 调用点与发射顺序。
- `04-init-single-user.md`、`05-init-runcom.md`、`06-init-read-ttys.md` — 启动序列三站。
- `09-init-multi-user.md`、`10-init-clean-ttys.md`、`11-init-shutdown.md` — 稳态与关停。
- `13-init-utmp.md` — runlevel 挂钩机制。
- C 源码：`minix3/sbin/init/init.c:130-139,369-405,624-640,1502-1522,1649-1655`。
