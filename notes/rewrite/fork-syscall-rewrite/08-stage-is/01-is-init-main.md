# 01-is-init-main：启动入口与主循环骨架

> **源码**：`minix3/minix/servers/is/main.c`（148 行，本文全覆盖）+
> `minix3/minix/servers/is/inc.h`（包含面）+ `proto.h`（函数归位）+
> `minix3/minix/lib/libsys/sef.c:208-214`（ping 拦截）+
> `minix3/minix/lib/libsys/sef_ping.c:21`（`do_sef_ping_request`）+
> `minix3/minix/include/minix/sef.h:121-122` + `com.h:64,90-93` +
> `minix3/sys/sys/signal.h:67`（SIGTERM=15）
> **Rust**：`os/servers/is/src/{lib,main,sef,state,dispatch}.rs`
> **draft 素材**：`draft/tmp_main.c.md`（逐行讲解可用作 §2 底料；
> 其 §Rust 实现对比块整体废弃，见 §3.0）
> **位置**：`sef_cb_init_fresh`（boot 锚点）+ 主循环 dispatch（plan §1.2 时序图）

---

## 1. 概念：为什么调试功能是一个独立的用户态服务

> **目标读者**：了解进程与系统调用基本概念、读过 plan §1.2 启动时序图的读者。
> 前置知识：kernel `12-ipc-core` 的 notify 语义（本章直接使用，不复述）。
> 本章只建心智模型，不讲任何函数实现（实现在 §2）。

### 1.1 问题：内核要不要自带调试台

一个操作系统内核在运行时会积累大量内部状态：进程表、特权表、中断路由、
内存区间、驱动状态。开发者调试时想看这些状态，最省事的做法是在内核里直接
`printf`。Minix3 早期确实这么干过，但它带来一个结构性问题：**查看状态的代码
和被查看的状态跑在同一个地址空间、同一个特权级**。调试代码的 bug 会直接踩坏
内核，调试输出的格式化开销会发生在关中断的临界区里。

Minix3 的答案是把"看状态"这件事整体搬出内核，做成一个普通的用户态服务：
Information Server（IS）。IS 的文件头注释把这个意图写得非常直白：

```c
/* System Information Service.
 * This service handles the various debugging dumps, such as the process
 * table, so that these no longer directly touch kernel memory. Instead, the
 * system task is asked to copy some table in local memory.
 */
```

（`minix3/minix/servers/is/main.c:1-8`）

注意第二句的措辞：调试转储"不再直接触碰内核内存"，取而代之的是"请系统任务把
表复制到本地内存"。这就是 IS 全部工作的本质——**调试转储聚合器**：平时什么
都不干，等用户按键，然后去各个服务那里把数据拷贝过来，格式化打印。

### 1.2 类比：火灾报警器，而不是前台接待处

理解 IS 主循环的关键，是先建立正确的心智模型。大多数服务器（如 VFS、PM）
是"前台接待处"：客户端发请求，服务器处理并回复，一问一答。IS 不是。IS 是
**火灾报警器**：它只听一种声音（功能键按下的通知），听到就拉响对应的警铃
（调用转储函数打印状态），听不到就静默。对其它一切声音——普通 IPC 请求、
来自非 TTY 的通知——它的反应是统一的：记一笔警告日志，然后**不回复**
（`EDONTREPLY`）。

这个"只听通知"的选择不是风格偏好，而是由触发链决定的：功能键按下是键盘
中断，键盘归 TTY 驱动管，TTY 通过**无载荷通知**（notify）告诉 IS"有键被按了，
自己来查是哪一个"。通知本身不带"哪个键"的信息——IS 收到通知后要再发一轮
`FKEY_EVENTS` 消息去 TTY 把位图拉回来（那是 02、03 的内容）。所以 IS 的主循环
天然是事件驱动的：等通知，分来源，干活或忽略。

```text
出生：rc.minix 条件启动 → RS 加载 → sef_startup → sef_cb_init_fresh
运行：get_work（等通知）→ 分类（TTY？）→ 干活 / 忽略（不回复）
退出：SIGTERM → 取消 TTY 登记 → exit
```

> **本章小结**：IS 是"只听 TTY 通知的火灾报警器"——用户态独立（§1.1）、
> 事件驱动（§1.2）。下一章 §2 把这三行骨架逐行对应到 `main.c` 的 148 行。

### 1.3 边界声明

**前置依赖**：plan §1.2 启动时序图；kernel `12-ipc-core`（notify 语义）；
`09-vm-boot-protocol`（服务启动协议的参照系）。

**本篇职责**：`main.c` 全部 6 个函数 + 4 个静态全局 + SEF 注册 + SEF ping
透明性 + IS 启动条件证据。回答"IS 如何出生、如何进入主循环、主循环如何
分类消息"。

**不覆盖（移交）**：`map_unmap_fkeys` 的注册实现细节 → 02；
`do_fkey_pressed` 的分派机制 → 03；SEF 框架本体（`sef_startup`/`sef_receive`
内部状态机）→ `minix-sef` crate，本篇只消费其 API（§3 D2）；
各 dump 函数体 → 05~10；`glo.h` 的死 extern（`diag_buf`/`sys_panic`/
`dont_reply`）→ 99（plan §5.4：全树无定义无使用）。

---

## 2. C 源码分析

> 本章是 ground truth。所有行号均为 `sed`/`grep` 实证，非凭记忆。
> `inc.h` 是纯包含面（32 行，`signal.h` 等 12 个头 + `proto.h` + `glo.h`），
> 无独立语义，不单独立节。`proto.h` 的函数归位：`main` 归本篇；
> `map_unmap_fkeys`/`do_fkey_pressed`/`mapping_dmp` 归 02/03；
> `vm_dmp` 等 8 个转储函数归 05~10。

### 2.1 四个静态全局：服务器的全部可变状态

```c
/* Allocate space for the global variables. */
static message m_in;		/* the input message itself */
static message m_out;		/* the output message used for reply */
static endpoint_t who_e;	/* caller's proc number */
static int callnr;		/* system call number */
```

（`main.c:13-17`）

IS 的全部运行时可变状态就是这四个变量：一个收件箱（`m_in`，64 字节消息，
布局见 `minix-types` 的 `Message`：`m_source` + `m_type` + 56 字节载荷）、
一个发件箱（`m_out`，只用 `m_type` 字段装回复码）、发送者端点、消息类型。
没有进程表，没有缓存，没有计数器——转储用的游标（`prev_i` 等）是各
`dmp_*.c` 文件私有的静态变量，不归 `main.c` 管。

这四个变量刻画了 C 实现的一个基本事实：**IS 是单线程的**。`get_work` 写它们，
主循环读它们，不存在并发写入者。Rust 重写必须保留这个事实，但要用类型系统
把它显式化（§3 D1），而不是照抄四个 `static mut`。

### 2.2 `main`：三段式主循环

```c
int main(int argc, char **argv)
{
  int result;

  /* SEF local startup. */
  env_setargs(argc, argv);
  sef_local_startup();

  /* Main loop - get work and do it, forever. */
  while (TRUE) {
      /* Wait for incoming message, sets 'callnr' and 'who'. */
      get_work();

      if (is_notify(callnr)) {
	      switch (_ENDPOINT_P(who_e)) {
		      case TTY_PROC_NR:
			      result = do_fkey_pressed(&m_in);
			      break;
		      default:
			      /* FIXME: error message. */
			      result = EDONTREPLY;
			      break;
	      }
      }
      else {
          printf("IS: warning, got illegal request %d from %d\n",
          	callnr, m_in.m_source);
          result = EDONTREPLY;
      }

      /* Finally send reply message, unless disabled. */
      if (result != EDONTREPLY) {
	  reply(who_e, result);
      }
  }
  return(OK);				/* shouldn't come here */
}
```

（`main.c:31-71`）

三段结构泾渭分明：`env_setargs` + `sef_local_startup`（启动，§2.6），然后
`while (TRUE)` 无限循环里的"取活→分类→回复"。注意三个细节：

1. `result` 是栈上局部量，每轮重写——分类结果不跨轮残留。
2. `return(OK)` 在循环之后，注释直说 `shouldn't come here`：正常执行流永远
   到不了，函数以发散（diverge）语义运行。Rust 侧这对应 `run()` 返回 `!`
   还是 `loop {}` 的选型（§3 D5 的一部分）。
3. 回复门 `if (result != EDONTREPLY)`： suppressed 的消息**连 `reply` 都不调**，
   不是"回一个空消息"。`EDONTREPLY` 不是错误码，是回复抑制哨兵（值为 203，
   见 §2.5）。

### 2.3 notify 分支：两层分类与一个 FIXME

```c
      if (is_notify(callnr)) {
	      switch (_ENDPOINT_P(who_e)) {
		      case TTY_PROC_NR:
			      result = do_fkey_pressed(&m_in);
			      break;
		      default:
			      /* FIXME: error message. */
			      result = EDONTREPLY;
			      break;
	      }
      }
```

（`main.c:48-58`）

第一层 `is_notify(callnr)` 的定义在 `com.h:93`：

```c
#define NOTIFY_MESSAGE		  0x1000                                    /* com.h:90 */
#define is_notify(a)		  ((unsigned) ((a) - NOTIFY_MESSAGE) < 0x100) /* com.h:93 */
```

即"消息类型落在 `[0x1000, 0x1100)` 区间即通知"。紧贴它上面有一行 FIXME
（`com.h:91`）：

```c
/* FIXME the old is_notify(a) should be replaced by is_ipc_notify(status). */
```

新形式 `is_ipc_notify(status)` 检查的是 `sef_receive` 带回的**状态字**
（`IPC_STATUS_CALL(status) == NOTIFY`，`com.h:92`），而不是消息类型。IS 沿用
旧形式是**有意的**：改新形式需要把 `get_work` 的签名改成带回状态字，属于
行为变更；保守重写保留旧形式（§3 D3 记录此决策，不标 ARCH——保留现状不是
架构演进）。

第二层 `_ENDPOINT_P(who_e)` 从端点提取槽号（slot），只认 `TTY_PROC_NR`
（值为 5，`com.h:64`）。这里必须用槽号比较而**不能**用端点裸值比较：端点
的高位是 generation（槽位复用时递增），同一个 TTY 在重启前后裸端点不同，
槽号不变。`default` 分支的 FIXME（"error message"）是 C 作者承认的缺口：
非 TTY 通知被静默吞掉，连警告日志都没有——与 §2.4 非 notify 分支的
`printf` 告警不对称。Rust 侧保留此不对称（行为兼容），但文档在此处记一笔，
避免后人"好心"补日志造成行为漂移。

### 2.4 非 notify 分支：警告日志 + 不回复

```c
      else {
          printf("IS: warning, got illegal request %d from %d\n",
          	callnr, m_in.m_source);
          result = EDONTREPLY;
      }
```

（`main.c:59-63`）

IS 不接受任何请求-响应式消息：非常见的"返回 EINVAL"，而是打印一行警告后
`EDONTREPLY`。格式串里有两个字段：`callnr`（消息类型）和 `m_in.m_source`
（发送者**裸端点**，注意不是槽号——日志原文如此，保留）。`printf` 经 libc
stdio 最终走到 log 驱动（输出通道问题见 §3 D4，A-6）。

### 2.5 回复门与 `reply`

```c
      /* Finally send reply message, unless disabled. */
      if (result != EDONTREPLY) {
	  reply(who_e, result);
      }
```

（`main.c:65-68`）

```c
static void
reply(
	int who,                           	/* destination */
	int result                           	/* report result to replyee */
)
{
    int send_status;
    m_out.m_type = result;  		/* build reply message */
    send_status = ipc_send(who, &m_out);    /* send the message */
    if (OK != send_status)
        panic("unable to send reply!: %d", send_status);
}
```

（`main.c:134-146`）

回复消息只装 `m_type`（结果码），载荷不动。`ipc_send` 失败 → `panic`：
回复路径被视为不可恢复——发不出回复的 IS 已经无法履行服务契约，继续循环
只会堆积不一致。`EDONTREPLY` 的值是 203（`os/libs/minix-types/src/types/
errno.rs:101`，注释引 `sys/errno.h:199`）；它是"伪码"（不进 `Errno` 错误
语义，errno.rs:94 明写"don't send a reply"），分类器必须把它当哨兵处理，
不能当错误码传播。

### 2.6 `sef_local_startup`：四个注册

```c
static void
sef_local_startup(void)
{
  /* Register init callbacks. */
  sef_setcb_init_fresh(sef_cb_init_fresh);
  sef_setcb_init_lu(sef_cb_init_fresh);
  sef_setcb_init_restart(sef_cb_init_fresh);

  /* Register signal callbacks. */
  sef_setcb_signal_handler(sef_cb_signal_handler);

  /* Let SEF perform startup. */
  sef_startup();
}
```

（`main.c:75-89`）

三个 init 回调（fresh / live-update / restart）注册的是**同一个函数**。
这不是偷懒，而是 STATELESS 声明：IS 没有需要跨重启恢复的状态（fkey 映射
是向 TTY 登记的观察者关系，重启后重建即可），所以三种出生方式走同一条
初始化路径（A-10，§3 D5）。第四个注册是 SIGTERM 处理器（§2.8）。
`sef_startup()` 把控制权交给 SEF 框架：它按当前出生类型调对应的 init 回调，
之后 `main` 的 `while` 循环才开始——所以 `sef_cb_init_fresh` 是事实上的
boot 锚点。

### 2.7 `sef_cb_init_fresh`：boot 锚点，一行

```c
static int sef_cb_init_fresh(int UNUSED(type), sef_init_info_t *UNUSED(info))
{
/* Initialize the information server. */

  /* Set key mappings. */
  map_unmap_fkeys(TRUE /*map*/);

  return(OK);
}
```

（`main.c:91-102`）

整个初始化就是一行：`map_unmap_fkeys(TRUE)`——向 TTY 登记 F1~SF12 观察者。
参数和 init 信息都标记 `UNUSED`：IS 的出生不需要任何外部信息（对比 RS 的
init 要读 boot image 表——IS 无表可读）。这一行是 01 与 02 的铆接点：
机制归 02，本篇只记录"锚点在此，调用发生在此"。

### 2.8 `sef_cb_signal_handler`：只认 SIGTERM

```c
static void sef_cb_signal_handler(int signo)
{
  /* Only check for termination signal, ignore anything else. */
  if (signo != SIGTERM) return;

  /* Shutting down. Unset key mappings, and quit. */
  map_unmap_fkeys(FALSE /*map*/);

  exit(0);
}
```

（`main.c:104-116`）

非 SIGTERM 信号直接返回（忽略）。SIGTERM（值 15，
`minix3/sys/sys/signal.h:67`）→ 取消 fkey 映射（告诉 TTY 别再通知一个将死
之人，否则 TTY 会往不存在的端点发通知）→ `exit(0)`。注意清理顺序：
**先 unmap 后 exit**，反过来会泄漏 TTY 侧的观察者登记。这是"灾难预演"级
的不变量，Rust 侧用单测锁死（§5）。

### 2.9 `get_work`：阻塞收 + 双写回 + panic

```c
static void
get_work(void)
{
    int status = 0;
    status = sef_receive(ANY, &m_in);   /* this blocks until message arrives */
    if (OK != status)
        panic("sef_receive failed!: %d", status);
    who_e = m_in.m_source;        /* message arrived! set sender */
    callnr = m_in.m_type;       /* set function call number */
}
```

（`main.c:118-130`）

`sef_receive(ANY, ...)` 阻塞到任意来源的消息到达。`ANY` 在这里是字面意义的
"谁都行"——来源过滤发生在分类器（§2.3），不在接收点。失败 → `panic`
（接收路径不可恢复：收不到消息的事件循环已死）。成功后把发送者和类型写回
两个静态全局（§2.1）——这正是 Rust 侧要消掉的共享可变（§3 D1）。

一个关键的隐形语义：`sef_receive` 不是裸 `receive`。SEF ping 存活检查就
藏在这里（§2.11）。

### 2.10 SEF ping 透明拦截：主循环永远看不到 ping

RS 用 `-period 5HZ` 参数启动 IS 后，会周期性发 ping 检查 IS 是否还活着。
ping 是 `NOTIFY_MESSAGE` 类型（`sef.h:122`：`SEF_PING_REQUEST_TYPE` 即
`NOTIFY_MESSAGE`），按 §2.3 的分类器它会被判为 notify——但**主循环永远看
不到它**，因为 `sef_receive` 内部先拦截了：

```c
#if INTERCEPT_SEF_PING_REQUESTS
      case SEF_PING_REQUEST_TYPE:
          /* Intercept SEF Ping requests. */
          if(IS_SEF_PING_REQUEST(m_ptr, status)) {
              if(do_sef_ping_request(m_ptr) == OK) {
                  continue;
              }
          }
      break;
#endif
```

（`minix3/minix/lib/libsys/sef.c:208-214`，开关 `sef.h:121` 恒为 1）

`do_sef_ping_request`（`sef_ping.c:21`）调默认回调应答后返回 OK，
`continue` 让 `sef_receive` 继续等下一条消息。这意味着：ping 的应答发生在
`get_work` 返回**之前**，分类器无感。这是"透明拦截"四个字的确切含义，
也是 Rust 侧 transport trait 必须复现的不变量（§3 D2，单测覆盖）。

### 2.11 启动条件：IS 何时存在

IS 不是常驻服务，而是**条件性 debug 服务**，三条证据链：

1. 无 `boot_image` 登记：`minix3/minix/kernel/table.c:44-64` 的 17 项
  （asyncm…init）中无 `is`（`grep '"is"'` 零命中，实证）。
2. 由 `rc.minix` 条件启动：仅当 `sysenv debug_fkeys` 非零时执行
   `up -n is -period 5HZ`（`minix3/etc/rc.minix:115-118`，`up` 行在 117；
   `-period 5HZ` 即 RS 每 5 秒 ping 存活检查，A-11）。
3. 权限面极窄：`minix3/etc/system.conf:271` 起 `service is { vm INFO; uid 0; }`——
   只允许 `VM_INFO` 系统调用 + 只能以 root 运行（A-9 的权限侧）。

全 minix3 无 `IS_PROC_NR` 定义（`rg IS_PROC_NR` 零命中，plan §5.4 实证）：
IS 的 endpoint 由 RS 在加载时动态分配。这就是 A-9 的全部内容——本篇不定义
任何对应常量（§3 D5），02~10 的消息目标端点一律运行时注入。

---

## 3. Rust 设计决策

> **先说 draft 旧块为什么整体废弃**（§3.0）：`draft/tmp_main.c.md` 的 Rust
> 对比块（425-520 行）犯了三个错：① `static mut M_IN/M_OUT/WHO_E/CALLNR`
> 直译全局可变（Rust 2024 已弃用 `static mut`，pattern #73）；② 虚构
> `IS_PROC_NR` 常量（全树无定义，A-9）；③ 引用 `minix_rs::ipc` 等不存在的
> 旧路径。其逐行讲解部分可用，代码块不可用。本章是重写后的决策记录。

### 3.1 D1：四个静态全局 → `IsServerState` 单例（state.rs）

C 用四个文件静态量在 `get_work` 和主循环之间传话（§2.1、§2.9）。Rust 侧把
它们收敛为一个结构体，归 `IsServer` 所有：

```rust
pub struct IsServerState {
    pub inbox: Message,
    pub reply_buf: Message,
    pub caller: Endpoint,
    pub call_nr: i32,
}
```

`get_work`/`reply` 变成 `&mut self` 方法。选择的理由：IS 单线程，无共享，
`Rc/RefCell` 是多余的运行时成本，`static mut` 是不可证的 unsafe——plain
struct + 可变借用是零成本且编译器检查的。这与 RS `RsServer` 同款（plain struct，
`os/servers/rs/src/lib.rs:117` 起），跨服务一致。

### 3.2 D2：SEF 回调 → trait，传输 → trait 缝（sef.rs）

C 的 `sef_setcb_*` 注册的是裸函数指针（§2.6）。裸 `fn` 指针不能捕获环境，
回调体一旦需要 server 状态就只能回头碰全局量——这正是 C 用四个静态量的
结构性原因。RS 已经趟过这条路：`os/servers/rs/src/sef.rs:60-91` 把回调集
建模为 `trait SefCallbacks`，由 `RsServer` 实现。本篇照抄该论证（先例引用，
非重复发明）：

```rust
pub trait SefCallbacks {
    fn init_fresh(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;
    fn init_restart(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;
    fn init_lu(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;
    fn signal_handler(&mut self, signo: i32);
}
```

三 init 缺省转调同一 `init_fresh` 体——STATELESS（A-10）在类型签名层面可见：
缺省方法体就是"三种出生同一条路"的机器可读声明。`[ARCH: A-10]` 标注在
`sef.rs` 缺省方法注释 + 本节 + design D5 三处。

传输侧（`sef_receive`/`ipc_send`/`sef_startup`）本体属 `minix-sef`/`minix-sys`，
当前均为 stub（sef 仅 5 行占位，sys 全 `todo!()`），本篇**只消费 API**：

```rust
pub trait SefTransport {
    fn receive(&mut self, inbox: &mut Message) -> Result<(Endpoint, i32), i32>;
    fn send(&mut self, dest: Endpoint, reply: &Message) -> Result<(), i32>;
    fn startup(&mut self);
}
```

`receive` 返回 `(caller, call_nr)` 而非写全局量——D1 的直接推论。生产实现是
forward reference（文件不存在时显式标注，Step 1.0g 合规），测试用 fake
transport。对照 Redox：Redox 的 scheme server 同样把内核接线收敛到 trait
边界之后，服务逻辑只对 trait 编程，单测永远不进内核。本篇是同一原则在
Minix-SEF 语义下的实例化。

### 3.3 D3：分类器是纯函数（dispatch.rs），02/03 留桩

`main.c:48-63` 的两层分支提炼为无副作用的纯函数（RS `dispatch::classify`
同款，`os/servers/rs/src/dispatch.rs:70-82`）：

```rust
pub const NOTIFY_MESSAGE: i32 = 0x1000;  // com.h:90
pub const TTY_PROC_SLOT: i32 = 5;        // com.h:64

pub fn is_notify_call(call_nr: i32) -> bool {
    ((call_nr - NOTIFY_MESSAGE) as u32) < 0x100  // com.h:93
}

pub enum DispatchAction { HandleFkey, Suppress }

pub fn classify(call_nr: i32, sender: Endpoint) -> DispatchAction {
    if is_notify_call(call_nr) && sender.slot() == TTY_PROC_SLOT {
        DispatchAction::HandleFkey
    } else {
        DispatchAction::Suppress
    }
}

pub fn is_reply_suppressed(result: i32) -> bool {
    result == EDONTREPLY  // 203, errno.rs:101
}
```

三处有意的设计点：① 用 `sender.slot()`（即 `_ENDPOINT_P`，endpoint.rs 的
`slot()`）而不用端点裸值相等——generation 位会使裸相等在 TTY 重启后失效，
C 用 `_ENDPOINT_P` 正是此因；② `is_notify` 保留旧形式（com.h:93），FIXME
（com.h:91）已知悉但不跟进——改新形式要改 `get_work` 签名带回状态字，属
行为变更，保守取向拒绝，§2.3 已讲清，不标 ARCH（保留≠演进）；③ `default`
分支的静默不对称（§2.3）原样保留，注释引 C 行号，防后人"补日志"漂移。

02 的 `map_unmap_fkeys` 在本篇以 `IsServer` 的桩方法 `request_fkey_map` 声明，
初始缺省 `Err(ENOSYS)` fail-closed（RS `dispatch_request` 全臂 ENOSYS 同款，
`dispatch.rs:106-118`）；02 落地时 `request_fkey_map` 经
`tty_fkey::map_unmap_keys` 填实（见 `02-is-fkey-contract.md` §4），签名不变。
> **03 更新**：`handle_fkey_pressed` 亦已填实（EVENTS 拉取 + 表分派 + 恒
> `EDONTREPLY`，见 `03-is-dump-dispatch.md` §4）；本段的"03 仍 ENOSYS"已过时。

### 3.4 D4：告警走 log 抽象（A-6 保守）

C 的 `printf("IS: warning, ...")`（§2.4）经 libc stdio 到 log 驱动。minix-rs
`no_std` 下没有 libc：告警经 `log`-形抽象（由诊断输出通道提供，A-6 待设计），
行为契约保留——**非法请求必留一条可观测告警**，不静默。`[ARCH: A-6]` 标在
本节 + design D4 + 代码注释三处；通道落地前 transport fake 把告警记入内存
`Vec` 供单测断言（"告警发生过"可测，不依赖真实驱动）。

### 3.5 D5：生命周期语义与错误面

- **STATELESS 三合一**（A-10）：见 D2 缺省方法。重启重建 fkey 映射，无状态
  恢复分支——`init_restart`/`init_lu` 体内无 `if`，这是可审查的"无分支即无
  状态"证据。
- **signal 只认 SIGTERM=15**（`sys/sys/signal.h:67`）：非 TERM 直接返回；
  TERM → 经 02 桩 unmap → 返回 `Shutdown` 动作由 `run()` 边界处理（库内不
  `exit`——C 的 `exit(0)` 是进程级发散，库函数发散会杀死测试宿主，不可测，
  故库返回动作枚举，二进制执行发散）。
- **`main` 的不可达返回**：C `return(OK); /* shouldn't come here */`（§2.2）
  对应 `IsServer::run()` 发散签名（`-> !`），`main.rs` 照 RS
 （`os/servers/rs/src/main.rs:12-45`：test 下 allocator + 空转门）同款接线。
- **错误面**：transport `receive`/`send` 失败 → panic（C §2.9/§2.5 同语义，
  启动与回复路径不可恢复）；分类 suppressed → 不调 `send`（§2.5 回复门）。
  panic 只出现在 transport 失败路径——dump 侧失败告警不 panic 是 04 的契约，
  本篇不碰。

---

## 4. 实现详解

### 4.1 模块树

```text
os/servers/is/src/
├── lib.rs       — crate 根：no_std 门 + IsServer 编排 + SefCallbacks 实现
├── state.rs     — IsServerState（D1 单例）
├── dispatch.rs  — is_notify_call / classify / is_reply_suppressed（D3 纯函数）
├── sef.rs       — SefInitType/SefInitInfo/SefCallbacks/SefTransport/SIGTERM（D2）
├── tty_fkey.rs  — FkeyId/位组装/transport/编排（02 起，见 02-is-fkey-contract.md §4）
└── main.rs      — 二进制接线（RS 同款 test 门）
```

`lib.rs` 头部与 RS 对齐（`#![cfg_attr(not(test), no_std)]` +
`extern crate alloc`，`os/servers/rs/src/lib.rs:1-29` 同款），模块文档写清
"单线程事件循环，`!Send` 合理，无跨 CPU 共享"（执行模型声明，review-core
强制）。

### 4.2 关键签名（与 §3 设计决策一致，Gate D-5 依据）

```rust
// state.rs
pub struct IsServerState { pub inbox: Message, pub reply_buf: Message, pub caller: Endpoint, pub call_nr: i32 }
impl IsServerState { pub fn new() -> Self; }

// dispatch.rs
pub const NOTIFY_MESSAGE: i32 = 0x1000;
pub const TTY_PROC_SLOT: i32 = 5;
pub fn is_notify_call(call_nr: i32) -> bool;
pub enum DispatchAction { HandleFkey, Suppress }
pub fn classify(call_nr: i32, sender: Endpoint) -> DispatchAction;
pub fn is_reply_suppressed(result: i32) -> bool;

// sef.rs
pub const SIGTERM: i32 = 15;
pub enum SefInitType { Fresh, Lu, Restart }
pub struct SefInitInfo { pub endpoint: i32, pub old_endpoint: i32 }
pub trait SefCallbacks { fn init_fresh(...) -> ...; /* 后三缺省 */ }
pub trait SefTransport { fn receive(...); fn send(...); fn startup(...); fn warn_illegal(...); /* 03 起 + warn_fkey_events(...)，见 03 §4 */ }
pub enum LifecycleAction { Continue, Shutdown }

// lib.rs（02 起双泛型：F 为 FKEY 传输，见 02-is-fkey-contract.md §4）
pub struct IsServer<T: SefTransport, F: FkeyCtlTransport> { state: IsServerState, transport: T, fkey: F, /* 03 桩状态 */ }
impl<T: SefTransport, F: FkeyCtlTransport> IsServer<T, F> {
    pub fn new(transport: T, fkey: F) -> Self;
    pub fn startup(&mut self) -> Result<i32, Errno>;   // sef_startup + init_fresh（boot 锚点）
    pub fn step(&mut self) -> LifecycleAction;          // get_work + classify + reply 门
    pub fn run(&mut self) -> !;                          // loop { step }（C while(TRUE)）
}
```

`IsServer::new` **不取 endpoint 参数**（A-9：endpoint 运行时由 RS 注入，
transport 层持有，库内无 `IS_PROC_NR` 常量——全树零定义，§2.11）。

常量权威位置（§2.4g：以下常量在 minix-rs 内均单一定义，无跨 crate 副本，
修改时无需同步；若未来有副本，必须先改此处）：`NOTIFY_MESSAGE`/`TTY_PROC_SLOT`
唯一定义于 `dispatch.rs`（C 源 `com.h:90/64`）；`SIGTERM` 唯一定义于 `sef.rs`
（C 源 `sys/sys/signal.h:67`）；`EDONTREPLY` 唯一定义于 `minix-types`
（`errno.rs:101`），本 crate 只引用不重定义。

### 4.3 关键不变量（实现必须维持，review 抽查点）

1. `classify` 无副作用、可重入（纯函数，§5 穷举）。
2. `Suppress` 路径永不调 `transport.send`（回复门，§2.5）。
3. transport 失败必 panic，不降级为告警（§2.9/§2.5）。
4. `run()` 发散；`step()` 是单轮可测单元（RS `server.run()` 同款拆分，
   `main.rs:44`）。
5. signal 非 TERM 无状态变化（§2.8，单测断言 transport 零调用）。

---

## 5. 测试要点

> 基线：`minix-is` 此前 0 测试（plan §3.5）。本篇新增全部测试，
> 以 `cargo test -p minix-is` 实际通过数为准（文末总数声明，§2.4j 格式）。

### 5.1 分类器真值表（dispatch.rs 单测）

| # | 输入（call_nr / sender） | 期望 | C 依据 |
|---|---|---|---|
| T1 | notify + TTY slot（generation=0） | HandleFkey | main.c:50-52 |
| T2 | notify + TTY slot（generation≠0，裸值≠5） | HandleFkey | `_ENDPOINT_P` 语义（slot 比较的必要性） |
| T3 | notify + 非 TTY（如 RS slot 2） | Suppress | main.c:53-56 |
| T4 | 非 notify + 任意 | Suppress | main.c:59-63 |
| T5 | 边界：0x0FFF→false / 0x1000→true / 0x10FF→true / 0x1100→false | — | com.h:93 区间语义 |
| T6 | `is_reply_suppressed(EDONTREPLY)`→true；ENOSYS/OK→false | — | main.c:66 |

### 5.2 生命周期（sef.rs + lib.rs，fake transport）

| # | 场景 | 期望 |
|---|---|---|
| T7 | `init_restart`/`init_lu` 缺省体 | 与 `init_fresh` 同结果（STATELESS 无分支） |
| T8 | `signal_handler(TERM≠15)` | 无 transport 调用、无状态变化 |
| T9 | `signal_handler(15)` | 返回 Shutdown + 恰一次 unmap 请求（顺序：先 unmap 后停，§2.8） |
| T10 | `step` 遇 HandleFkey | 拉取 EVENTS → 表分派执行 → `EDONTREPLY` 抑制无 send（03 起；初版 ENOSYS 回复已更新） |
| T11a | `step` 遇 Suppress（非 notify） | send 零调用 + 告警恰一次（D4 可观测性） |
| T11b | `step` 遇 Suppress（非 TTY notify） | send 零调用 + 告警零次（C default 分支静默，§2.3） |
| T12 | transport receive 错误 | `#[should_panic]`（§2.9 不可恢复语义；send 侧 panic 防御性保留但经 step 不可达——两臂恒抑制，03 起单测锁定） |
| T13 | `startup` | transport.startup 恰一次 + init_fresh OK（boot 锚点可达） |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**41 passed, 0 failed**（`state` 1 + `dispatch` 15 +
  `sef` 4 + `lib` 10 + `tty_fkey` 11；其中 1 个 `#[should_panic]` 锁定 receive
  失败路径；`dispatch` 8 个与 `tty_fkey` 11 个归 02/03 §5）。
- `cargo check -p minix-is` / `cargo clippy -p minix-is`：本 crate 零警告
 （`minix-sys` stub 的 28 个预先存在警告与本篇无关）。
- 本节列出与本模块直接相关的 21 个行为断言（T1~T13，T5 含 4 个边界值，
  T11 拆 a/b 两路）；
  完整清单：`rg "#\[test\]" os/servers/is/src/`。

---

## 6. 过渡：在启动时序中的位置与下一篇入口

本篇覆盖 plan §1.2 时序图的两处：**`sef_cb_init_fresh` boot 锚点**
（`map_unmap_fkeys(TRUE)` 调用点，§2.7）与**主循环 dispatch**
（`get_work→classify→reply`，§2.2）。读完本篇，读者知道 IS 如何出生、如何
等活、如何把 TTY 通知挑出来——但还不知道三件事：① 那行 `map_unmap_fkeys`
到底往 TTY 登记了什么（② 收到通知后 `do_fkey_pressed` 如何知道是哪个键
（③ 数据从哪来。① 是下一篇 02（`02-is-fkey-contract.md`）的入口：FKEY_
MAP/UNMAP/EVENTS 三命令 + `TTY_FKEY_CONTROL` 消息格式 + TTY 侧
`do_fkey_ctl`/`func_key` 契约 + `map_unmap_fkeys` 本体。

---

## 7. 参见

- `02-is-fkey-contract.md`（待写）：`map_unmap_fkeys` 本体 + FKEY 协议面（A-1/A-2）
- `03-is-dump-dispatch.md`（待写）：`do_fkey_pressed` + hooks 表 + 次主线路径图
- `../01-stage-kernel/12-ipc-core.md`：notify/IPC 原语语义
- `../01-stage-kernel/09-vm-boot-protocol.md`：服务启动协议参照
- `../03-stage-rs/01-rs-boot-init.md` + `os/servers/rs/src/sef.rs:60-91`：
  SEF trait 建模先例（D2 直接复用其论证）
- `../03-stage-rs/06-rs-main-loop.md`：RS 主循环对照（RS 用状态字分类，
  IS 用消息类型分类——§2.3 FIXME 差异点）
- `draft/tmp_main.c.md`：逐行素材（§2 底料；其 Rust 对比块已废弃，§3.0）
- `plan.md` §1.2/§4(A-9/A-10/A-11)/§5.3(01 条目)/§5.4(排除项)
