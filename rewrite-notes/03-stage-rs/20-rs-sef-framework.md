# 20-rs-sef-framework: SEF 框架——服务的出生握手与收信拦截

> **分类**: 全局基建（链接进每个用户态服务进程的框架库，本 stage 作为该框架的唯一权威讲述点）
> **源码**: `minix3/minix/lib/libsys/sef.c`（`sef_startup`、`sef_receive_status`、`sef_self`、`sef_cancel`、`sef_exit`、`sef_munmap`、`sef_getrndseed`）、`minix3/minix/lib/libsys/sef_init.c`（`process_init`、`do_sef_rs_init`、`do_sef_init_request`、四个 init 注册器、`sef_cb_init_*` 预定义回调族、`sef_cb_init_response_rs_reply`、`sef_cb_init_response_rs_asyn_once`）、`minix3/minix/lib/libsys/sef_ping.c`（`do_sef_ping_request`、`sef_cb_ping_reply_pong`）、`minix3/minix/lib/libsys/sef_signal.c`（`do_sef_signal_request`、`process_sigmgr_signals`、`process_sigmgr_self_signals`）、`minix3/minix/lib/libsys/sef_liveupdate.c`（`do_sef_lu_request`、`do_sef_lu_before_receive`、`sef_lu_ready`、`sef_lu_state_change`）、`minix3/minix/lib/libsys/sef_st.c`（`do_sef_st_before_receive`、`sef_st_state_transfer`、`sef_copy_state_region`）、`minix3/minix/lib/libsys/sef_fi.c`（`do_sef_fi_request`）、`minix3/minix/lib/libsys/sef_gcov.c`（`do_sef_gcov_request`）、`minix3/minix/include/minix/sef.h`（`SEF_INIT_FRESH`、`SEF_LU_STATE_NULL`、`IS_SEF_INIT_REQUEST` 等全部约定）
> **Rust 实现**: `os/libs/minix-sef/src/lib.rs`, `os/servers/rs/src/sef.rs`, `os/servers/rs/src/lib.rs`, `os/servers/is/src/sef.rs`, `os/servers/devman/src/hooks.rs`, `os/servers/vm/src/vm_server.rs`, `os/servers/vfs/src/main_loop.rs`, `os/servers/mib/src/server.rs`, `os/servers/sched/src/kernel_api/transport.rs`, `os/servers/devman/src/ipc/minix.rs`, `os/servers/pm/src/init.rs`, `os/servers/ipc-server/src/server.rs`, `os/fs/fs-rt/src/ipc.rs`, `os/fs/fs-rt/src/transport.rs`, `os/libs/minix-driver-rt/src/transport.rs`, `os/net/lwip/src/server.rs`, `os/net/uds/src/server.rs`
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/00-rs-overview.md`（RS 是谁、boot 排第几）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md`（主循环的收信形状）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md`（握手协议的另一半：RS 侧）
> **说明**: 一个服务进程从"刚被装载"到"能接客"，中间隔着一条由框架库守住的门。本文档讲这条门：出生时它替服务跟 RS 说话，稳态时它替服务筛掉不该进业务代码的消息。

---

## 1. 概念：谁在替服务决定"什么时候可以开始干活"

### 1.0 目标读者与前置知识

面向已经读完 RS 总览（00）与主循环（06）、知道"服务是 RS 一个个生出来的"的读者。前置知识只需要两条：一个进程被装载不等于一个服务可用；微内核里进程之间只能靠消息说话。本文档第一次出现 `ipc_sendrec`、`asynsend3`、`sys_statectl` 这些名字时都会就地解释语义，不要求预习。

### 1.1 矛盾的起点：装载完成 ≠ 服务可用

RS 把一个服务的二进制装载进内存、给它建好地址空间、让它开始跑，这个进程此刻已经"活着"。但它还没有自己的数据结构：文件服务器还没建 vnode 表，进程管理器还没建进程槽位表，调度器还没建优先级队列。

如果此时别的进程给它发业务请求，会发生什么？这个进程的消息缓冲会被内核送到它的数据结构还没初始化的代码里——最幸运的情况是读到全零的表然后回一个错误码，最糟的情况是拿一个没初始化的指针当表头解引用。

于是系统必须回答三个问题，而这三个问题的答案就是 SEF 框架的全部内容：

1. **什么时候这个进程才可以接业务请求？** 答案是：它自己声明初始化完成之后。声明的方式是给 RS 回一条消息。这就是**出生握手**——框架库负责收发这条消息，服务只负责填一个结果码。
2. **在它自己声明之前，谁能给它发消息？** 答案是：只有 RS 和内核。因为 RS 可以给它装一道"只放行指定来源"的门（内核侧的 IPC 过滤），装到它回出生回报为止。
3. **它跑起来以后，控制类的消息（保活探针、热更新准备、信号）怎么和业务请求区分开？** 答案是：在每次收消息的门口做分类，控制类由框架就地处理或转成事件，业务请求才交给服务自己的分派表。这就是**收信拦截**。

> **本章不讲什么**（机制一律移交）：
> - RS 这一侧怎么发 `RS_INIT`、怎么等出生回报、回报失败后怎么判死 → `12-rs-init-run.md`
> - 热更新的状态机全貌（RS 视角的 prepare/update/init/end 编排）→ `16-rs-live-update.md`
> - 热更新要搬走的"记忆"长什么形状 → `17-rs-state-data.md`
> - RS 自己被重启、被更新时走的特殊路径 → `18-rs-self-lifecycle.md`
> - `SEF_INIT_*` / `SEF_LU_*` 这些常量在 Rust 侧的唯一定义位置 → `99-rs-global-concepts.md`
> - `sys_whoami` / `sys_getksig` / `vm_memctl` 这些调用本身的签名与线格式 → `19-rs-external-interfaces.md`
> - 某个具体服务注册了哪几个回调、每个回调里做了什么 → 各服务文档的首章（例如 `notes/rewrite/fork-syscall-rewrite/04-stage-pm/01-pm-init-main.md`）

### 1.2 它是什么：一个库，不是一个服务

SEF 的全称是 System Event Framework，展开写在头文件第一行的注释里（`minix3/minix/include/minix/sef.h:1`）。这一点要先钉住，因为把框架当成"某个服务"会推出错误的调用关系：

- 它没有自己的进程，没有自己的槽位，不出现在 boot 镜像表里。
- 它是一组 `.c` 文件，编译进 `libsys`，被每个服务的可执行文件链接进去。库本体 9 个文件共 2210 行（`sef.c` 400、`sef_init.c` 483、`sef_liveupdate.c` 566、`sef_llvm.c` 241、`sef_signal.c` 202、`sef_st.c` 194、`sef_ping.c` 63、`sef_fi.c` 31、`sef_gcov.c` 30，`wc -l` 实证），加上约定头 `sef.h` 387 行。
- 每个服务在自己源码里写一个 `sef_local_startup` 函数，登记自己关心的回调，然后调库的 `sef_startup` 把控制权交给框架。全 Minix3 树里有 52 个 `.c` 文件出现 `sef_local_startup`：`minix3/minix/drivers/` 28 个、`minix3/minix/servers/` 8 个、`minix3/minix/fs/` 5 个、`minix3/minix/lib/` 3 个（驱动与文件系统框架库替使用者代签）、`minix3/minix/tests/` 8 个（测试程序也按服务的规矩出生）。计数由 `grep -rl "sef_local_startup" minix3/minix --include=*.c` 得到。

换句话说：**每一个会跟 RS 打交道的用户态进程，都在自己进程里跑着一份 SEF 代码**。框架不是通信的一方，它是每一方内部的那层胶水。

### 1.3 三条时间轴

一个服务的一生里，SEF 的代码出现在三个不重叠的时间窗里。先建立这张图，后面每一节都是图上某个方块的放大：

```
时间 ──────────────────────────────────────────────────────────────→

【窗口一：出生】进程已开始跑，还没进主循环
   main() → sef_local_startup() 登记回调 → sef_startup()
                                                   │
                        ┌──────────────────────────┼─────────────────────┐
                        │ 我是 RS？                │ 我是 VM 且首次？     │ 其它服务
                        │ 自己造 init 参数         │ 跳过，稍后由 RS 补   │ 阻塞收 RS 的出生请求
                        │ （ROOT_SYS_PROC 分支）   │ （__vm_init_fresh）  │ ipc_receive(RS_PROC_NR)
                        └──────────────────────────┴─────────────────────┘
                                                   │
                                       process_init(type, info)
                                       清过滤 → 建约定授权 → 调服务的 init 回调
                                                   │
                                       回 RS 一条 RS_INIT + result  ← 门开了，可以接业务

【窗口二：稳态】主循环每一轮
   sef_receive_status(ANY, &m, &status)
        │ 收一条消息
        ├─ 是 RS 的保活探针？→ 回 pong，吞掉，回到收消息（业务看不见）
        ├─ 是内核的信号通知？→ 查信号表，调服务的信号回调，吞掉
        ├─ 是 RS 的热更新准备？→ 置状态，吞掉（之后走窗口三）
        ├─ 是覆盖统计 / 故障注入请求？→ 库内处理，吞掉
        └─ 都不是 → 返回给服务，进业务分派表

【窗口三：热更新进行中】（本重写项目未实装，见 §3.5 与 §3.6 TODO）
   每轮收消息之前先问服务"现在能安全交接吗"，能则保存状态、回 RS 就绪
```

窗口二是这张图里最容易被误读的一格：主循环调用点看起来是"收消息"，但收消息这个动作里被塞进了五到六个分类判定。服务作者在主循环里看见的 `m_type`，永远是**已经过了筛**的值——这是为什么很多服务的主循环开头只判 `is_ipc_notify(ipc_status)` 就直接分派业务，而不需要防着"万一这条是 RS 探针"。

### 1.4 行为规则（后文全部论证的收敛点）

1. **被框架吞掉的消息永不回到调用方**。`sef_receive_status` 判定为有效 ping 之后 `continue`，直接在下一轮重新收消息；服务永远看不到这个 `m_type`。这是"框架处理掉"与"框架转成事件"的唯一区别（信号与出生请求属于后者，见 §2.3.6）。
2. **出生回报决定服务生死**。回报里的 result 字段由 RS 判读，非 OK 会触发 RS 的重启/判死编排（机制归 12/15，本处只钉住"result 是框架替你递给 RS 的那一位"）。
3. **框架不猜服务意图，只做查表分派**。C 用四张静态函数指针表（init、ping、signal、live update），Rust 用 trait 方法——两者共同点是：注册什么就调什么，框架不含服务业务知识。
4. **判定条件全是三元组**：消息类型 + 来源端点 + 投递方式（是普通请求还是通知）。少一个维度都会误吞业务消息，见 §2.3.7 的判据表。

### 1.5 本章小结

SEF 回答的是三个问题：什么时候能接客（出生握手）、接客时谁先过筛（收信拦截）、控制类消息谁接（库内回调表）。窗口一与窗口二在 C 里是两团互不相干的代码，但它们共享同一份约定头 `sef.h` 和同一张"消息类型 → 处理者"的表。第 2 章按源码逐个拆开，第 3 章讲 Rust 重写为什么把窗口一的库壳拆掉、把窗口二的判定留在库里，第 4 章给逐条实现对照与真实消费点清单。

---

## 2. C 源码分析

### 2.1 相关定义：六类拦截的三件套

框架的每一个拦截臂都由三个宏定义成一组，`sef.h` 里逐条写着"拦什么"（`INTERCEPT_SEF_*_REQUESTS`）、"按什么类型进 switch"（`SEF_*_REQUEST_TYPE`）、"怎么确认真的是这类"（`IS_SEF_*`）。把六臂列全，是因为只数出"四类"会漏掉两个真实存在的臂：

| 拦截臂 | 开关（定义处） | switch 用的类型 | 确认判据 | 编译条件 |
|---|---|---|---|---|
| 出生 | `sef.h:INTERCEPT_SEF_INIT_REQUESTS` | `RS_INIT`（`sef.h:SEF_INIT_REQUEST_TYPE`） | `m_type == RS_INIT && m_source == RS_PROC_NR`（`sef.h:IS_SEF_INIT_REQUEST`） | 恒开 |
| 保活探针 | `sef.h:INTERCEPT_SEF_PING_REQUESTS` | `NOTIFY_MESSAGE` | 是通知 且 `m_source == RS_PROC_NR`（`sef.h:IS_SEF_PING_REQUEST`） | 恒开 |
| 热更新准备 | `sef.h:INTERCEPT_SEF_LU_REQUESTS` | `RS_LU_PREPARE` | `m_type == RS_LU_PREPARE && m_source == RS_PROC_NR` | `USE_LIVEUPDATE` 关闭时被 `sef.h:381-384` 撤销 |
| 信号 | `sef.h:INTERCEPT_SEF_SIGNAL_REQUESTS` | `SIGS_SIGNAL_RECEIVED` | `m_type == SIGS_SIGNAL_RECEIVED && m_source < INIT_PROC_NR`，或"是通知且 `m_source == SYSTEM`"（两条臂） | 恒开 |
| 覆盖统计 | `sef.h:INTERCEPT_SEF_GCOV_REQUESTS` | `COMMON_REQ_GCOV_DATA` | `m_type == COMMON_REQ_GCOV_DATA && m_source == VFS_PROC_NR` | `sef.c` 里要求 `USE_COVERAGE` 同时成立 |
| 故障注入 | `sef.h:INTERCEPT_SEF_FI_REQUESTS` | `COMMON_REQ_FI_CTL` | `m_type == COMMON_REQ_FI_CTL`（`sef.h:IS_SEF_FI_REQUEST` 只判类型） | 恒开 |

消息类型本身的取值（都是协议头的常量，不是框架自造的）：

| 常量 | 值 | 定义处 |
|---|---|---|
| `NOTIFY_MESSAGE` | `0x1000` | `minix3/minix/include/minix/com.h:90` |
| `RS_INIT` | `RS_RQ_BASE + 20` | `com.h:478` |
| `RS_LU_PREPARE` | `RS_RQ_BASE + 21` | `com.h:479` |
| `SIGS_SIGNAL_RECEIVED` | `COMMON_RQ_BASE + 0` | `com.h:601` |
| `SYSTEM` | `-2` | `com.h:50` |
| `RS_PROC_NR` | `2` | `com.h:61` |
| `VM_PROC_NR` | `8` | `com.h:67` |
| `IDLE` | `-4` | `com.h:48` |
| `INIT_PROC_NR` | `11` | `com.h:72`（取值来自 `LAST_SPECIAL_PROC_NR`，`com.h:70`） |

初始化类型与标志（同一份头文件里两套编号，容易混，故列表区分）：

- 类型三值：`SEF_INIT_FRESH = 0`（全新启动）、`SEF_INIT_LU = 1`（热更新后初始化）、`SEF_INIT_RESTART = 2`（崩溃重启后初始化），定义在 `sef.h:SEF_INIT_FRESH`。
- 调试标志：`SEF_INIT_CRASH`/`SEF_INIT_FAIL`/`SEF_INIT_TIMEOUT`/`SEF_INIT_DEFCB`/`SEF_INIT_SCRIPT_RESTART`/`SEF_INIT_ST`，定义在 `sef.h:SEF_INIT_CRASH`。前三个会让框架**跳过服务真正的初始化回调**（见 §2.3.3 的第五步），`SEF_INIT_DEFCB` 则强制走默认回调而不是服务注册的那个。
- 预定义回调别名族 `SEF_CB_*`（`sef.h:SEF_CB_INIT_FRESH_DEFAULT`、`sef.h:SEF_CB_INIT_RESTART_STATEFUL`、`sef.h:SEF_CB_INIT_RESPONSE_DEFAULT` 等）：服务注册时可以直接填这些宏，等于"我要库写的现成行为"。

### 2.2 核心数据结构

#### 2.2.1 四张回调表

框架不保存任何服务状态，只保存"该调谁"。四张表都是文件内静态的匿名结构体变量，字段就是函数指针：

| 表 | 定义位置 | 字段（即回调槽） | 初值 |
|---|---|---|---|
| `sef_init_cbs` | `minix3/minix/lib/libsys/sef_init.c`（L11，工具生成） | `sef_cb_init_fresh`、`sef_cb_init_lu`、`sef_cb_init_restart`、`sef_cb_init_response` | `SEF_CB_INIT_FRESH_DEFAULT`（= `sef_cb_init_null`）、`SEF_CB_INIT_LU_DEFAULT`（= `sef_cb_init_lu_generic`）、`SEF_CB_INIT_RESTART_DEFAULT`（= `sef_cb_init_reset`）、`SEF_CB_INIT_RESPONSE_DEFAULT`（= `sef_cb_init_response_rs_reply`） |
| `sef_ping_cbs` | `minix3/minix/lib/libsys/sef_ping.c`（L6，工具生成） | `sef_cb_ping_reply` | `SEF_CB_PING_REPLY_DEFAULT`（= `sef_cb_ping_reply_pong`，见 §2.3.7） |
| `sef_signal_cbs` | `minix3/minix/lib/libsys/sef_signal.c`（L7，工具生成） | `sef_cb_signal_handler`、`sef_cb_signal_manager` | 两个 `*_NULL`（收到即什么都不做） |
| `sef_lu_cbs` | `minix3/minix/lib/libsys/sef_liveupdate.c`（L13，工具生成） | 六个：`lu_prepare`、`lu_state_isvalid`、`lu_state_changed`、`lu_state_dump`、`lu_state_save`、`lu_response` | 各自的 `SEF_CB_LU_*_DEFAULT`（`sef.h:SEF_CB_LU_PREPARE_DEFAULT`） |

值得停下来看一眼的是默认值的取向差异：**init 臂的默认值就是"什么都不做然后回 OK"**（`sef_cb_init_null`），而**response 臂的默认值是"真的给 RS 发一条 sendrec"**。这意味着一个只写了 `sef_setcb_init_fresh`、忘了注册 response 的服务，仍然会正确地向 RS 报到；反过来，如果 response 默认值是空操作，服务就会在出生时静默卡死（RS 等不到回报，超时判死）。默认值的这一侧选择，是框架"宁可让你报错、不可让你静默"的取舍。

注册器（`sef_setcb_*`）的函数体只有两行：`assert(cb != NULL)` 与一次赋值（例如 `sef_init.c:sef_setcb_init_fresh`）。断言把"注册了个空指针"变成启动期崩溃而不是运行期解引用——这条差异在第 3 章变成类型系统的性质。

#### 2.2.2 `sef_init_info_t`：出生请求的附件

服务的初始化回调签名是 `int cb(int type, sef_init_info_t *info)`，第二个参数就是 RS 随 `RS_INIT` 一起递来的上下文（`minix3/minix/include/minix/sef.h` 的 `sef_init_info_t`，L42-L53，工具生成；11 个字段）：

| 字段 | 类型 | 含义 | 由谁填 |
|---|---|---|---|
| `flags` | `int` | 上面那套 `SEF_INIT_*` / `SEF_LU_*` 标志 | RS（`minix3/minix/servers/rs/utility.c:init_service`） |
| `rproctab_gid` | `cp_grant_id_t`（= `int32_t`，`minix3/minix/include/minix/type.h:23`） | 指向 RS 公开进程表的授权号，服务可只读地拷走自己的那一行 | RS |
| `endpoint` / `old_endpoint` | `endpoint_t`（= `int`，`type.h:22`） | 本世 / 上一世的端点 | 框架自填 `sef_self_endpoint`；`old_endpoint` 来自消息 |
| `restarts` | `int` | 这是第几次重生 | RS |
| `init_buff_start` / `init_buff_cleanup_start` | `void *` | RS 预先分配的初始化缓冲及其待清理起点 | 消息，或 `do_sef_rs_init` 向 VM 索取 |
| `init_buff_len` | `size_t` | 缓冲长度 | 同上 |
| `copy_flags` | `int` | 状态搬移的寻址方式（`SEF_COPY_DEST_OFFSET` 等） | 框架在状态搬移中回填 |
| `prepare_state` | `int` | 热更新准备阶段状态 | RS |

按 LP64（x86-64）对齐推导这个结构体的尺寸：5 个 4 字节字段占 0–19，指针要求 8 字节对齐故插入 4 字节填充，三个 8 字节量（两个指针 + 一个 `size_t`）占 24–47，最后 2 个 `int` 占 48–55，**总计 56 字节**。同一份定义在 32 位下是 40 字节（无填充、指针 4 字节）。这就是本重写项目里"结构体按 64 位重排"的判例之一（同类判断见 `notes/rewrite/fork-syscall-rewrite/03-stage-rs/17-rs-state-data.md`）。

这些字段全部住在**栈**上：`do_sef_init_request` 在函数体开头声明 `sef_init_info_t info` 并 `memset` 清零，然后把消息里的值逐个搬进去，再把 `&info` 交给 `process_init`（`sef_init.c:do_sef_init_request`）。函数返回后这块栈就失效——所以初始化回调如果想留住 `info` 里的任何指针（例如 `init_buff_start`），必须在回调内用完。

### 2.3 关键函数分析

#### 2.3.1 `sef_startup`：先问内核"我是谁"，再决定走哪条出生路

入口在 `sef.c:sef_startup`。它的开头一次系统调用拿走四样东西：

```c
  r = sys_whoami(&sef_self_endpoint, sef_self_name, SEF_SELF_NAME_MAXLEN,
      &priv_flags, &init_flags);                                    /* sef.c:78 */
```

`sys_whoami` 是向内核（伪端点 `SYSTEM`，值 `-2`）发起的调用，返回本进程的端点号、名字（缓冲区 `SEF_SELF_NAME_MAXLEN` 即 20 字节，`sef.c:SEF_SELF_NAME_MAXLEN`）、权限标志、初始化标志。这四个出参全部是**栈地址**：`priv_flags`、`init_flags` 各 4 字节 `int`，`sef_self_endpoint` 是 4 字节 `endpoint_t` 但住在静态存储段（`minix3/minix/lib/libsys/sef.c` 的 `sef_self_endpoint`，L15，工具生成，初值 `NONE`）——因为后面其它文件里的函数要靠 `EXTERN` 声明共享它（`minix3/minix/lib/libsys/sef_init.c` 的同名外部声明，L31，工具生成）。

拿到身份之后分三条路，这个分岔是整个框架里最容易读错的一段：

1. **我是 RS**（`priv_flags & ROOT_SYS_PROC`）：走 `do_sef_rs_init`（`sef.c:109-114`）。理由很直白——所有服务都要等 RS 的消息才能初始化，而 RS 等谁？只能自己造参数。
2. **我是 VM 且首次启动**（`sef_self_endpoint == VM_PROC_NR && __vm_init_fresh`）：**什么都不做，直接返回**（`sef.c:115-116`）。这个空分支是关键设计，不是遗漏：VM 一旦开始处理自己的初始化，就可能触发缺页，而缺页要靠 VM 自己服务——此时 VM 还没初始化完。所以 VM 的首次初始化推迟到 RS 主动给它发出生请求时再做（`__vm_init_fresh` 由 VM 的 `main` 在调 `sef_local_startup` 前置 1、返回后清 0）。注意 `__vm_init_fresh` 是**弱符号**（`minix3/minix/lib/libsys/sef.c` L25，工具生成，声明带 `__attribute__((weak))`）：不是 VM 的进程根本没这个变量，弱链接保证库对所有服务都能链接通过。
3. **其它服务**：阻塞在 `ipc_receive(RS_PROC_NR, &m, &status)` 上，并且用 `do { … } while(!IS_SEF_INIT_REQUEST(&m, status))` 把"不是出生请求的消息"**丢弃**后继续等（`sef.c:126-131`）。

第 3 条路里的丢弃循环值得多看一眼，源码注释说得很明白（`sef.c:121-125`）：崩溃重启时，本进程可能收到原本要投递给"上一世自己"的杂消息（例如 RS 发给旧实例的更新请求）。如果直接把它当出生请求处理，回调拿到的 `info` 里 `type` 是垃圾值、`init_buff_start` 是野指针，服务会带病出生。丢弃 + 继续阻塞，是这个框架对"重启"这一现实的最小防御。

最后是热更新场景下的一处端点修正：如果本进程是根系统进程但端点号不等于 `RS_PROC_NR`，说明这是 RS 热更新后的新实例，先用 `vm_update(RS_PROC_NR, sef_self_endpoint, …)` 把 VM 侧记录的 RS 端点改正过来，再把 `sef_self_endpoint` 掰回 `RS_PROC_NR`（`sef.c:94-105`）。这段在重写侧尚未实装（见 §3.5）。

#### 2.3.2 `do_sef_rs_init`：RS 的出生参数怎么"自己造"

`sef_init.c:do_sef_rs_init` 是 §2.3.1 第 1 条路的实现。它做的事情是：既然没人给我发 `RS_INIT`，我就自己拼一份 `sef_init_info_t`。

| 字段 | RS 给自己填的值 | 依据 |
|---|---|---|
| `type` | 默认 `SEF_INIT_FRESH`；若权限标志含 `LU_SYS_PROC` 则 `SEF_INIT_LU`，含 `RST_SYS_PROC` 则 `SEF_INIT_RESTART` | 从内核给的权限标志反推自己的出生方式（`sef_init.c:do_sef_rs_init`） |
| `rproctab_gid` | `GRANT_INVALID` | RS 自己没有进程表可拷——它就是表的拥有者 |
| `endpoint` / `old_endpoint` | 自己的端点 / §2.3.1 里修正前的端点 | — |
| `init_buff_start` 等 | `SEF_INIT_FRESH` 时为 `NULL`/0；否则向 VM 用 `vm_memctl(RS_PROC_NR, VM_RS_MEM_GET_PREALLOC_MAP, …)` 索取预分配缓冲 | 非首次出生需要上一世留下的缓冲 |

拼完就直接调 `process_init(type, &info)`，与别的服务走的是同一个函数——这一点很重要：**RS 的出生和其它服务的出生共享全部初始化动作**，差别只在参数来源。

#### 2.3.3 `process_init`：六段固定动作

`sef_init.c:process_init` 是出生握手的心脏。无论是 RS 自造参数（§2.3.2）、还是主循环前收到的出生请求（§2.3.1 第 3 条路），最终都汇到这一个函数。它的动作顺序是硬编码的，每一段都有"必须在此时"的理由。

**第一段：撤除 IPC 过滤。** 内核侧这一请求的处理是 `clear_ipc_filters(caller)`（`minix3/minix/kernel/system/do_statectl.c` 的 `SYS_STATE_CLEAR_IPC_FILTERS` 分支，L42，工具生成；请求号定义在 `minix3/minix/include/minix/com.h:SYS_STATE_CLEAR_IPC_FILTERS`）。过滤器的语义是"只放行匹配项，其余收不到"，挂着它就收不到全量业务消息。谁会给一个进程装过滤器？当前源码里加白名单的调用方有 VM（`minix3/minix/servers/vm/main.c:666`，在自己的初始化里加）、RS（`minix3/minix/servers/rs/utility.c:266`，带超时的接收，注释自述"假设 RS 此前没挂过过滤器"）以及热更新时的过滤规则搬移（`sef_liveupdate.c:336`）。于是这一步的确定作用是：**从上一世（重启或热更新回滚）继承下来的过滤规则，在新世出生时被无条件清干净**。"新进程从创建到出生之间是否已被装了过滤器"此处不下结论——需核对 RS 与 VM 的装载路径（归 `notes/rewrite/fork-syscall-rewrite/03-stage-rs/05-rs-ipc-sendmask.md`）[待验证]。

**第二段：建一个覆盖全地址空间的只读授权，并且要求它的编号必须是 0。** `cpf_grant_direct(sef_self_endpoint, 0, ULONG_MAX, CPF_READ)` 把"我自己的整个地址空间、允许别人只读"登记成一个授权项（`sef_init.c:process_init`）。要求编号等于 `SEF_STATE_TRANSFER_GID`（值 0，`sef.h:SEF_STATE_TRANSFER_GID`）的原因在授权表的分配策略里：这张表是**每个进程自己内存里的一条数组**（实现 `safecopies.c:cpf_grant_direct`），空闲槽串成按编号升序的链表，源码注释把这条约定直接写了出来——"升序加序号清零，是为了让第一个被分配的授权项编号为 0"（`safecopies.c:cpf_prealloc`）。所这个断言实际在检查一件更脆的事：**我是本进程里第一个创建授权项的人**。若某个服务的构造函数（比 `sef_local_startup` 更早跑）已经建过授权项，0 号被占，服务会在出生时直接 panic。这是一条真实存在的隐式契约，重写侧若要用同一约定必须显式建模（见 §3.6）。

**第三段：调试标志短路。** 如果 `info->flags` 含 `SEF_INIT_CRASH`/`SEF_INIT_FAIL`/`SEF_INIT_TIMEOUT`，框架就**不调服务的初始化回调**，而是调库自带的模拟函数（`sef_init.c:process_init` 里的 `debug_result_found` 分支），分别对应 `sef_cb_init_crash`（当场 panic）、`sef_cb_init_fail`（回 `ENOSYS`）、`sef_cb_init_timeout`（去 `ipc_receive(IDLE, …)` 死等，从而让 RS 的初始化超时逻辑被触发）。这三个函数存在的意义是让"服务在初始化时崩溃/失败/超时"这三种最难验证的恢复路径可以被人为制造出来。

**第四段：按类型分派到服务的初始化回调。** `switch (type)` 三个分支，每个分支都再问一句 `info->flags & SEF_INIT_DEFCB`：置位就用默认回调，否则用服务注册的（`sef_init.c:process_init`）。也就是说 RS 有权强制某一次出生"不走你的自定义初始化，走库的通用初始化"。类型不认识则 `result = EINVAL`。

**第五段：造回报、送出去。** 框架清零一个消息、填 `m_source = 自己的端点`、`m_type = RS_INIT`、`m_rs_init.result = 回调返回值`，然后**把这个消息交给 response 回调**去发（`sef_init.c:process_init`）。分工很干净：框架负责"该回报什么"，服务（或默认实现）负责"怎么送"——于是同一个 result 可以走阻塞 sendrec，也可以走异步 send（§2.3.5）。

**第六段：解除映射 + 三次内核登记。** 回报之后，若 `info->init_buff_cleanup_start` 非空则 `sef_munmap` 掉 RS 预分配缓冲里用剩的部分（失败只打印，不致命），再依次 `cpf_reload()`（把授权表位置与大小告诉内核）、`senda_reload()`（把异步发送表告诉内核，失败只打印）、`sys_statectl(SYS_STATE_SET_STATE_TABLE, sef_llvm_state_table_addr(), 0)`（把状态表告诉内核；内核侧只是写进本进程的 `priv` 字段，见 `minix3/minix/kernel/system/do_statectl.c` 的 `SYS_STATE_SET_STATE_TABLE` 分支，L27，工具生成；请求号本身定在 `minix3/minix/include/minix/com.h:SYS_STATE_SET_STATE_TABLE`）。这三步的共同点是：**把本进程私有内存里几张表的地址交给内核**，内核从此能直接读它们。跳过任一步的后果不是"初始化失败"这种可控错误，而是运行期才暴露的拷贝失败或异步消息丢失——这也是本框架最难在 Rust 侧"顺手省掉"的一段（见 §3.6）。

#### 2.3.4 预定义 init 回调族：库写好的几种标准出生行为

服务不必都自己写初始化。`sef_init.c` 提供了一组可直接登记的实现，理解它们的返回值就是理解 RS 会怎么对待你：

| 回调 | 返回 | RS 侧后果（机制归 12/15） | 用途 |
|---|---|---|---|
| `sef_cb_init_null` | `OK` | 放行 | 无状态服务（不需要初始化） |
| `sef_cb_init_fail` | `ENOSYS` | 判功能不支持 | 人为制造失败 |
| `sef_cb_init_reset` | `ERESTART` | 让自己重生（不带旧资源、换新端点） | 默认的重启回调："我不知道怎么恢复，重来吧" |
| `sef_cb_init_crash` | 不返回（panic） | 崩溃恢复路径 | 人为制造崩溃 |
| `sef_cb_init_timeout` | 不返回（死等 `IDLE`） | 初始化超时路径 | 人为制造超时 |
| `sef_cb_init_restart_generic`（宏 `SEF_CB_INIT_RESTART_STATEFUL`） | 见说明 | — | "有状态的重启"：若是自身热更新（`SEF_INIT_LU` 且 `SEF_LU_SELF`）转去做同一性状态搬移；若类型不是 `SEF_INIT_RESTART` 则 `ENOSYS`；否则调 `sef_llvm_ltckpt_restart`（编译期插桩支持的检查点重启） |
| `sef_cb_init_identity_state_transfer` | `OK` 或错误码 | — | 把旧世的数据段与堆搬进本世：先存栈引用、按 `_etext` 到 `_brksize` 拷数据段、若堆变大则先 `brk` 再拷新增部分；最后检查 `sef_controlled_crash`——若不是受控崩溃就回 `EGENERIC` |
| `sef_cb_init_lu_generic` / `sef_cb_init_lu_identity_as_restart` | 转调 | — | 热更新后的初始化：自身更新按重启处理，其余走状态搬移框架 |

其中同一性搬移里那句 `sef_controlled_crash` 检查（`sef_init.c:sef_cb_init_identity_state_transfer`）是容易被忽略的诚实设计：只有框架自己制造的崩溃（见 §2.3.10 的故障注入臂会把它置真）才允许被接续；一个真崩溃的进程内存状态不可信，接过来只会把污染带进新世。

#### 2.3.5 出生回报的三条腿

回报怎么送，是 response 回调的自由，库里给了三个现成答案：

```c
int sef_cb_init_response_rs_reply(message *m_ptr) {
  return ipc_sendrec(RS_PROC_NR, m_ptr);        /* sef_init.c:458-466，默认腿 */
}
int sef_cb_init_response_rs_asyn_once(message *m_ptr) {
  int r = asynsend3(RS_PROC_NR, m_ptr, AMF_NOREPLY);
  sef_setcb_init_response(SEF_CB_INIT_RESPONSE_DEFAULT);  /* 只异步一次 */
  return r;
}
int sef_cb_init_response_null(message *UNUSED(m_ptr)) { return ENOSYS; }
```

`ipc_sendrec` 是"发送并等回复"的合成调用：本进程发出后停在回复相位，直到 RS 处理完并回应。这正是 §1.1 那个矛盾的另一面——**出生回报是一次请求-应答，不是一条通知**。如果这里退化成单向 `ipc_send`，RS 的回应就成了没人认领的消息，会被当成新请求处理，进而产生"回声投回 RS"的错乱；本重写项目实现这条腿时踩过的具体坑与取证记录见 `notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md` 的 B9 / B9b 两节；C 真值是确定的：默认腿必须是 sendrec。

第二条腿是给 VM 的：`sef_cb_init_response_rs_asyn_once` 首条回报异步发送、之后自动改回默认腿。VM 在 `sef_local_startup` 里按 `__vm_init_fresh` 条件登记它（`minix3/minix/servers/vm/main.c:219-240`）。

#### 2.3.6 `sef_receive_status`：稳态门口的筛子

主循环只调用这一个函数，但它内部是一个 `while(TRUE)` 的"收一条、判一次、必要时重收"循环（`sef.c:sef_receive_status`）。骨架如下：

```c
  sef_self_receiving = TRUE;
  while(TRUE) {
      if (!sef_self_receiving) return EINTR;                 /* sef.c:161 */
      if(sef_lu_state != SEF_LU_STATE_NULL) do_sef_lu_before_receive();
      if(__sef_st_before_receive_enabled)   do_sef_st_before_receive();
      r = ipc_receive(src, m_ptr, &status);
      if(r != OK) return r;                                  /* sef.c:179 */
      m_type = m_ptr->m_type;
      if (is_ipc_notify(status)) {
          switch (m_ptr->m_source) {                         /* sef.c:184-193 */
              case SYSTEM:     m_type = SEF_SIGNAL_REQUEST_TYPE; break;
              case RS_PROC_NR: m_type = SEF_PING_REQUEST_TYPE;   break;
          }
      }
      switch(m_type) { /* 六个拦截臂 */ }
      break;  /* 不是有效的 SEF 请求，交给调用方 */
  }
```

四个细节决定了这层筛子的行为边界：

1. **`sef_self_receiving` 是重入逃生门。** 信号回调可能让服务想结束主循环（例如收到 `SIGTERM` 后走关停流程），而回调是在这个循环**内部**被调用的——服务无法从外面打断它。于是库给了 `sef_cancel`（`sef.c:sef_cancel`）：把标志置假，下一次循环顶部的检查就返回 `EINTR`。注意它只在"库已经把控制权交给框架代码"的窗口内有效，这是异步信号安全的经典手法：不中断执行流，只让执行流自己在一个明确点退出。
2. **通知的类型改写是"分类"而不是"拦截"。** 内核来的通知本身没有可读的业务 `m_type`，框架按**来源**改写它：`SYSTEM` 改写成信号请求类型、`RS_PROC_NR` 改写成探针类型。改写后仍要走下面那个 `switch`——所以"是不是通知"只决定候选类别，真正确认还要看 `IS_SEF_*` 判据（§2.1 的表）。
3. **出生请求臂是"吞掉"而不是"上浮"。** `IS_SEF_INIT_REQUEST` 成立时，除非"类型是 `SEF_INIT_FRESH` 且自己正是 VM"，一律 `continue`（`sef.c:196-206`）。这条臂与 §2.3.1 的启动期等待正好配对：出生请求只应该在启动期被处理一次，稳态时再收到一条（多半是重启杂消息或 RS 的重发）不该让业务分派表看到。VM 的例外是给 §2.3.1 第 2 条路补的那次迟到初始化。
4. **`do_sef_*_request` 返回 `OK` 就 `continue`，返回非 `OK` 就落到 `break`。** 于是"库处理掉了"与"这不归库管"用同一个返回值区分。例如探针处理函数返回 `OK` 表示"我已经回了 pong"（`sef_ping.c:do_sef_ping_request` 的注释明写这点），故障注入对不认识的操作回 `ENOSYS`，消息就落到服务手里。

#### 2.3.7 保活探针：一次通知换一次通知

RS 需要知道一个服务是不是还活着。探针由 RS 发出（通知，来源 `RS_PROC_NR`，`m_type == NOTIFY_MESSAGE`），库里两处代码完成应答：

```c
int do_sef_ping_request(message *m_ptr) {
  sef_ping_cbs.sef_cb_ping_reply(m_ptr->m_source);   /* sef_ping.c:21-38 */
  return OK;                                          /* 吞掉 */
}
void sef_cb_ping_reply_pong(endpoint_t source) {
  ipc_notify(source);                                 /* sef_ping.c:sef_cb_ping_reply_pong */
}
```

默认回调就是"回一条通知给发探针的人"，另一个现成答案 `sef_cb_ping_reply_null` 什么都不做——登记它等于自愿让 RS 以为自己死了。这一对是框架里少见的"默认即正确、显式登记才是选择"的臂。RS 多久探一次、探不到怎么处置，归 `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md`。

判据汇总表（§1.4 第 4 条的展开）：

| 类别 | 消息类型条件 | 来源条件 | 投递方式条件 | 处理结果 |
|---|---|---|---|---|
| 出生请求 | `== RS_INIT` | `== RS_PROC_NR` | 不要求 | 启动期：处理并回报；稳态：吞掉（VM 首次除外） |
| 探针 | `== NOTIFY_MESSAGE` | `== RS_PROC_NR` | **必须**是通知 | 回 pong 并吞掉 |
| 热更新准备 | `== RS_LU_PREPARE` | `== RS_PROC_NR` | 不要求 | 置库内状态并吞掉 |
| 信号 | `== SIGS_SIGNAL_RECEIVED` | `< INIT_PROC_NR`，或通知时 `== SYSTEM` | 两条臂分别判 | 调信号回调后吞掉 |
| 覆盖统计 | `== COMMON_REQ_GCOV_DATA` | `== VFS_PROC_NR` | 不要求 | 库内回数据后吞掉 |
| 故障注入 | `== COMMON_REQ_FI_CTL` | 不判 | 不要求 | 认识则处理，不认识落给服务 |

前三类都要求"来源是 RS"。若判据里去掉来源，任何进程都能伪装 RS 让服务吞掉自己的消息（探针伪装会让服务误以为已被保活；出生请求伪装的后果见 §2.3.1 丢弃循环的讨论）。

#### 2.3.8 信号：两条臂与一个代理循环

`sef_signal.c:do_sef_signal_request` 按来源分两条完全不同的路：

- **来源是 `SYSTEM`（内核通知）**：从 `m_ptr->m_notify.sigset` 取信号集，遍历 `SIGK_FIRST` 到 `SIGK_LAST`（即从 `SIGKMEM` 到 `SIGKSIG`，`minix3/sys/sys/signal.h:SIGK_FIRST`），逐个调注册的信号处理回调。集合里若有 `SIGKSIG`（值 74，`minix3/sys/sys/signal.h:SIGKSIG`），说明"有别人的信号挂在我这个信号管理器上"，转 `process_sigmgr_signals`；若有 `SIGKSIGSM`（值 73，`minix3/sys/sys/signal.h:SIGKSIGSM`），说明"有信号是给我自己的"，转 `process_sigmgr_self_signals`。
- **来源不是内核**：这是信号管理器投递过来的普通系统信号，直接读 `m_ptr->m_pm_lsys_sigs_signal.num` 拿信号号，调处理回调。

代理循环 `sef_signal.c:process_sigmgr_signals` 的结构值得单独看，因为它是"一个进程替别的进程管理信号"的实现骨架：

```c
  while (TRUE) {
      if((r=sys_getksig(&target, &set)) != OK) panic(…);   /* sef_signal.c:36 */
      if (target == NONE) break;                            /* 没有待处理信号了 */
      for (signo = SIGS_FIRST; signo <= SIGS_LAST; signo++) {
          if(sigismember(&set, signo)) {
              r = sef_signal_cbs.sef_cb_signal_manager(target, signo);
              if(r == EDEADEPT) break;                      /* 目标已死，停手 */
          }
      }
      if(r == OK) { if((r=sys_endksig(target)) != OK) panic(…); }  /* sef_signal.c:60 */
  }
```

三点性质：① 只有信号管理器进程会调用 `sys_getksig`，别的进程没有代理权；② `sys_endksig(target)` 是"投递完成"的确认，若回调中途发现目标死了（`EDEADEPT`）就**不发这个确认**，信号留在内核等下次；③ 外层 `while(TRUE)` 会一直抽干内核里的待处理信号才回到收信循环——所以一次通知可能引发多轮代理。谁充当信号管理器、PM 在这条链上的分工归 `notes/rewrite/fork-syscall-rewrite/04-stage-pm/13-signal-flow.md`；本框架只负责"抽干并回调"。

库给的现成处理回调有三个极端：`sef_cb_signal_handler_null`（什么都不做）、`sef_cb_signal_handler_term`（只认 `SIGTERM`，收到就 `sef_exit(1)`，其它忽略）、`sef_cb_signal_handler_posix_default`（把可忽略的 `SIGCHLD`/`SIGWINCH`/`SIGCONT`/`SIGTSTP`/`SIGTTIN`/`SIGTTOU` 放过、其余一律终止，但**内核信号不放**，判据 `IS_SIGK`，`minix3/sys/sys/signal.h:IS_SIGK`）——因为内核信号（如 `SIGKMEM`）不是要杀你，而是通知你有事发生（VM 就靠它触发内存回收，见 `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`）。

#### 2.3.9 热更新的库侧半：一问一答之外还有"每轮自问"

热更新臂与其它臂的结构性区别是：其它臂在一条消息上完成，它跳很多轮收信。三个函数接力：

1. `sef_liveupdate.c:do_sef_lu_request` 收到 `RS_LU_PREPARE`，先处理三种早退：状态为 `SEF_LU_STATE_NULL` 表示取消，直接归零；库里已有状态表示忙，回 `EBUSY`；状态不被服务认可则按"服务没注册合法性回调 → `ENOSYS`，注册了但判非法 → `EINVAL`"区分回码——这个区分让 RS 能知道是"你不支持热更新"还是"这个更新方式不行"。合法才处理 RS 随消息给的状态数据（`sef_lu_handle_state_data`），然后进入新状态。
2. `sef_liveupdate.c:do_sef_lu_before_receive` 在**每一轮收信之前**被 §2.3.6 的循环顶部调用（仅当已有状态）。它按状态问"现在是安全交接点吗"：`SEF_LU_STATE_WORK_FREE` 直接算就绪；`SEF_LU_STATE_UNREACHABLE` 永不就绪；`SEF_LU_STATE_EVAL` 去求值一个表达式（调试用，表达式最长 512 字节；`minix3/minix/include/minix/sef.h:SEF_LU_STATE_EVAL_MAX_LEN` 这个宏在同一头文件里被写了两遍，L232 与 L334，工具生成，两处取值相同，属无害重复）；其余交给服务注册的 `lu_prepare` 回调。返回 `ENOTREADY` 就继续干活，否则进入第 3 步。
3. `sef_liveupdate.c:sef_lu_ready` 依次做清理（`sef_llvm_state_cleanup`）、保存要带走的状态（服务的 `lu_state_save` 回调）、造 `RS_LU_PREPARE` + result 交给 `lu_response` 回调送出（默认实现同样是 sendrec 给 RS，`sef_liveupdate.c:sef_cb_lu_response_rs_reply`）。**送完之后无条件把状态归零并 `senda_reload()`**——归零的含义是"更新没成（或被取消），我继续原样活"；而 `senda_reload()` 的原因在源码里有一段很长的注释（`sef_liveupdate.c:sef_lu_ready`）：异步发送表在更新期间不是原子交接的，若新实例没能走到 `senda_reload()` 就回滚，旧实例在内核侧的"有异步消息待发"标志可能已被清掉，不重做一次就会有消息永远投不出去。

状态回到 `SEF_LU_STATE_NULL` 时 `sef_lu_state_change` 还会再清一次 IPC 过滤（`sef_liveupdate.c:sef_lu_state_change`），并在状态真的变化时通知服务的 `lu_state_changed` 回调——这是给服务"我进/出了热更新模式"的唯一钩子。状态机的 RS 侧编排归 `16-rs-live-update.md`。

#### 2.3.10 状态搬移、覆盖统计与故障注入

- **状态搬移**（`sef_st.c`）：`sef_st.c:do_sef_st_before_receive` 在当前源码里是**空函数**（函数体是一对空花括号）——§2.3.6 循环顶部那个开关 `__sef_st_before_receive_enabled` 是接好的，动作待插。真正的搬移在 `sef_st.c:sef_st_state_transfer` 与 `sef_st.c:sef_copy_state_region`，配合 `sef_llvm.c` 的插桩辅助（栈引用保存与恢复、`sef_llvm_real_brk`、状态表地址）以及 DS 标签初始化 `sef_llvm_ds_st_init`（DS 在自己的 `sef_local_startup` 里登记它，`minix3/minix/servers/ds/main.c:93-104`）。
- **覆盖统计**（`sef_gcov.c:do_sef_gcov_request`）：VFS 会向各服务要代码覆盖率数据（判据要求来源 `VFS_PROC_NR`），库把请求转给注册的 `sef_cb_gcov`，默认实现是宏 `SEF_CB_GCOV_FLUSH_DEFAULT`。整个臂在 `sef.c` 里还额外要求 `USE_COVERAGE` 编译开关。
- **故障注入**（`sef_fi.c:do_sef_fi_request`）：若子类型是 `RS_FI_CRASH`，先把 `sef_controlled_crash = TRUE` 再 `panic("Crash!")`——这正是 §2.3.4 里"受控崩溃才允许接续状态"那个标志的唯一置真处；其余请求在有 EDFI 注入器时转给它，否则回 `ENOSYS`（消息落回服务手里）。

#### 2.3.11 其余入口

| 入口 | 定义 | 语义与钉住的细节 |
|---|---|---|
| `sef_self` | `sef.c:sef_self` | 返回自己的端点；若还没初始化（值为 `NONE`）直接 panic——把"用错时序"变成崩溃而不是返回 0 |
| `sef_cancel` | `sef.c:sef_cancel` | 置 `sef_self_receiving = FALSE`，配合 §2.3.6 第 1 点返回 `EINTR` |
| `sef_exit` | `sef.c:sef_exit` | 服务的 `exit` 被**弱别名**接到它（`__weak_alias(_exit, sef_exit)`），实现是 `sys_exit()`；若内核不接手就打印一句然后死循环——服务不该有自己的退出路径 |
| `sef_munmap` | `sef.c:sef_munmap` | 给 VM 的解映射：若自己就是 VM，改用 `asynsend3(SELF, …, AMF_NOREPLY)` 给自己异步发，否则普通 `_syscall(VM_PROC_NR, …)`。理由与 §2.3.1 第 2 条路同源：VM 不能同步等自己 |
| `sef_getrndseed` | `sef.c:sef_getrndseed` | 用 `getticks()` 的返回值当随机种子（熵来自时间戳，不是密码学强度） |
| 调试头 | `sef.c:sef_debug_header` | 只在任一 `*_DEBUG` 开启时编译进去：首次调用向内核取 `boottime` 与 `GET_HZ`，之后每条调试输出前缀"进程名: 时间 = 秒 + 微秒" |

### 2.4 调用关系与消费面

两个方向看这张网：谁调库（服务侧），库调谁（对端）。

```
服务 main()
  └─ sef_local_startup()                 ← 服务自己写的，不在库里
       ├─ sef_setcb_*() × N              ← 登记（§2.2.1 的四张表）
       └─ sef_startup()                  ← 库：窗口一
            ├─ sys_whoami() → 内核
            ├─ do_sef_rs_init()                      ┐
            ├─ （VM 首次：空转直接返回）              ├→ process_init() → 服务的 init 回调
            └─ ipc_receive(RS) + do_sef_init_request() ┘   → response 回调 → ipc_sendrec(RS)
  └─ while(TRUE)
       └─ sef_receive_status()             ← 库：窗口二
            ├─ do_sef_ping_request()    → 服务的探针应答回调 → ipc_notify(RS)
            ├─ do_sef_signal_request()  → 服务的信号回调（→ sys_getksig / sys_endksig）
            ├─ do_sef_lu_request()      → 置库内状态（→ 窗口三）
            └─ 其余 → 返回给服务的业务分派表
```

**库调用的对端全景**（本框架一共有四个对话者）：

| 对端 | 腿 | 锚点 |
|---|---|---|
| RS | 启动期等出生请求 | `sef.c:sef_startup` 的 `ipc_receive(RS_PROC_NR, …)` |
| RS | 出生完成回报 | `sef_init.c:sef_cb_init_response_rs_reply`（sendrec）、`sef_init.c:sef_cb_init_response_rs_asyn_once`（异步） |
| RS | 探针应答（pong） | `sef_ping.c:sef_cb_ping_reply_pong` |
| RS | 热更新就绪回报 | `sef_liveupdate.c:sef_cb_lu_response_rs_reply` |
| 内核 | 自身份 | `sys_whoami`（`sef.c:sef_startup`） |
| 内核 | 信号代理与状态登记 | `sys_getksig`/`sys_endksig`（`sef_signal.c:process_sigmgr_signals`）、`sys_statectl`（`sef_init.c:process_init`） |
| VM | 预分配缓冲、解映射、端点修正 | `vm_memctl`（`sef_init.c:do_sef_rs_init`）、`sef.c:sef_munmap`、`vm_update`（`sef.c:sef_startup`） |
| VFS | 代码覆盖数据 | `sef.h:IS_SEF_GCOV_REQUEST`（来源限定 `VFS_PROC_NR`） |
| DS | 状态搬移标签（仅 DS 自己） | `sef_llvm_ds_st_init`（`minix3/minix/servers/ds/main.c:93-104`） |

对端分布坐实了本 stage 承担这个权威讲述点的理由：**init / 热更新 / 探针三条腿的对端全部是 RS**，而库内部还为 RS 开了专属分支（§2.3.1 第 1 条路）。信号腿的对端是内核，PM 只在"普通信号投递"那一臂出现（`sef_signal.c:do_sef_signal_request` 读 `m_pm_lsys_sigs_signal.num`）。

**各服务的注册差异（实测）**。同一个 `sef_local_startup` 模式，每个服务只登记自己需要的子集——这张表既是"框架不属于任何单个服务"的直接证据，也是各服务首章应当只写自己那一行的原因：

| 服务 / 驱动 | 登记的回调 | 锚点 |
|---|---|---|
| RS | init_fresh + init_restart + init_lu + init_response + lu_response + signal_handler + signal_manager（**全 7 个**） | `minix3/minix/servers/rs/main.c:136-152` |
| PM | init_fresh + init_restart（宏 `SEF_CB_INIT_RESTART_STATEFUL`）+ signal_manager | `minix3/minix/servers/pm/main.c:115-127` |
| VFS | init_fresh + init_restart（同宏）+ init_lu + lu_prepare + lu_state_changed + lu_state_isvalid | `minix3/minix/servers/vfs/main.c:374-389` |
| VM | init_fresh + init_lu + init_restart + **条件改 init_response** + lu_state_changed + signal_handler | `minix3/minix/servers/vm/main.c:219-240` |
| SCHED | init_fresh + init_restart（同宏） | `minix3/minix/servers/sched/main.c:111-121` |
| DS | init_fresh + init_restart（同宏）+ `sef_llvm_ds_st_init` | `minix3/minix/servers/ds/main.c:93-104` |
| IS | init_fresh + init_lu + init_restart（同一回调）+ signal_handler | `minix3/minix/servers/is/main.c:77-89` |
| IPC | init_fresh + init_restart + signal_handler | `minix3/minix/servers/ipc/main.c:124-136` |
| MIB | init_fresh + init_restart（没有独立的 `sef_local_startup` 函数，写在启动路径里） | `minix3/minix/servers/mib/main.c:419-427` |
| MFS / ISO9660 | init_fresh + init_restart（同宏）+ signal_handler | `minix3/minix/fs/mfs/main.c:31-42`、`minix3/minix/fs/isofs/main.c:46-53` |
| EXT2 | init_fresh + signal_handler（**不登记重启**） | `minix3/minix/fs/ext2/main.c:39-49` |
| 字符设备驱动（TTY） | init_fresh + init_restart（同宏）+ signal_handler | `minix3/minix/drivers/tty/tty/tty.c` 的 `sef_local_startup` |
| 传感器类小驱动（BMP085 等） | init_fresh + init_lu + init_restart（同一回调）+ lu_state_save | `minix3/minix/drivers/sensors/bmp085/bmp085.c:511-527` |
| 驱动 / 文件系统框架库（代签） | 由 `chardriver_task` / `blockdriver_task` / `fsdriver_task` 一类库函数代写 | `minix3/minix/lib/` 下 3 个 `.c` 命中（§1.2 计数） |

两处容易被当成写错的地方：① IS 把 fresh/lu/restart 三个槽都指到**同一个** `sef_cb_init_fresh`——它无状态，三种出生都一样做；② 宏 `SEF_CB_INIT_RESTART_STATEFUL` 不是服务写的函数，而是库的 `sef_cb_init_restart_generic`（§2.3.4）——服务只在"我要自己恢复状态"与"我放弃状态重来"之间选一个。

### 2.5 设计要点

1. **三层分离：开关（编译期）/ 判据（运行期）/ 回调（服务侧）。** 六条腿各自回答"拦不拦"与"拦下来交给谁"，服务只能影响后者。若把判据下放给服务，每个服务都要自己防探针与出生杂消息（§2.3.6 第 3 点就是这个防线的所在）。
2. **出生期与稳态用两套代码处理同一判据。** 启动期是同步阻塞且不允许失败（处理不了就 panic），稳态是尽力过滤、失败可降级（吞不下的落给服务）。把两者合为一处，会得到一个"启动期能 panic、稳态不能 panic"的矛盾函数。
3. **默认回调的取值反映了安全侧的选边。** init 默认无副作用（回 OK），response 默认真发消息（不报到的服务会被 RS 判死）。两者合起来保证"什么都不注册的服务 = 一个干净的空服务"，同时"忘了回报"不会被静默吞掉。
4. **返回值是库与服务之间唯一的协议。** `do_sef_*_request` 用 OK / 非 OK 说"我处理完了 / 没处理"，init 回调用 errno 值说"我初始化成了 / 该怎么对我"。框架没有自造任何新的错误码种类。
5. **隐式契约集。** 三处必须钉住：状态搬移授权的编号必须为 0（§2.3.3 第二段）、`__vm_init_fresh` 必须存在且初值为 0（靠弱符号兜底，§2.3.1 第 2 条路）、`sef_self_endpoint` 必须在 `sef_startup` 之后才可用（§2.3.11 的 panic 就是这个顺序的保护）。这类契约在 C 里靠注释与 panic 维持，在 Rust 里应当变成类型约束或启动顺序断言。

---

## 3. Rust 设计决策

> 本章只回答"为什么这样设"，怎么做的部分在第 4 章。每条决策都可追溯到第 1、2 章的机制；凡 Rust 侧与 C 真值不一致、或整块语义尚未实现的地方，均在本章末尾列成待办清单（§3.6），交给后续实现轮处理。

### 3.1 D1：全局回调表 → 服务器实现的 trait

C 的四张表（§2.2.1）能工作，前提有两个：回调体可以碰全局变量，以及函数指针可以随时替换。重写侧去掉了全局变量（服务器状态是结构体字段，单线程事件循环里由 `&mut self` 唯一持有），第一个前提就消失了：Rust 的 `fn` 指针不能捕获环境，把 `fn(&mut ServerState) -> i32` 存进表里，调用时谁来传这个 `&mut`？库不知道状态在哪。

选定方案：回调集 = 服务器实现的 trait 方法，"注册"这个动作本身消失（编译期静态分派）。落点与槽位对照：RS 侧 `os/servers/rs/src/sef.rs:SefCallbacks` 的七个方法对应 §2.4 表里 RS 的 7 次登记；IS 侧另有一份 `os/servers/is/src/sef.rs:SefCallbacks`；devman 用的是 `os/servers/devman/src/hooks.rs:SefHooks`。

被否的两个方案与理由：

- 保留全局可变状态（`static` + `UnsafeCell`）：与"状态入结构"相逆，而且在 `no_std` 下仍然需要自己保证不重入，风险回到 C 那一侧。
- `Box<dyn FnMut(&mut …)>` 闭包表：能捕获，但在单线程场景下与 trait 方法等价，却多一次堆分配与一次动态调度；更实际的问题是覆盖率穷举时"谁实现了哪个回调"从 grep 可定变成运行期才知道。

留下的真实代价：**同一个 C 语义现在有三套 trait 并存**（RS / IS / devman 各自声明）。这层重复不在本节修补范围（属代码卓越度与死代码消除任务），登记在 §3.6 第 T5 条。

### 3.2 D2：出生握手不建库壳，把出生请求上浮

C 把"等出生请求 → 调初始化回调 → 回报 RS"三段都塞在库里（§2.3.1 第 3 条路 + §2.3.3）。重写侧拆成两处：

1. **服务侧**：`sef_startup()` 的等价面就是一个显式初始化函数（PM 的 `os/servers/pm/src/init.rs`、VM 的 `os/servers/vm/src/vm_server.rs:rs_handshake`、VFS 的 `os/servers/vfs/src/main_loop.rs:run`），启动参数不再来自 RS 的 `sef_init_info_t`，而来自本进程已有的 boot 上下文。
2. **库侧**：分类器把 RS 的出生请求作为 `SefEvent::Init(init_type)` **上浮**给服务器（`os/libs/minix-sef/src/lib.rs:sef_receive_status`），服务器跑完自己的初始化后发回报，回报消息由 `os/libs/minix-sef/src/lib.rs:sef_init_reply` 构造。

为什么不上浮就不成立：如果库照 C 那样在内部处理掉出生请求，它必须能回调服务器的初始化代码——那又回到 D1 里被否掉的函数指针方案。Rust 侧把初始化做成了带状态的方法，库拿不到那个状态，所以上浮是 D1 的必然后果而不是偷懒。

与 C 的**行为差异**必须说清：C 稳态收到出生请求一律吞掉（§2.3.6 第 3 点），Rust 一律上浮。差异的合理之处是 Rust 侧没有"启动期同步等待"这个窗口，出生只能发生在主循环里；风险是服务必须自己接住这个事件，否则出生请求会变成一条被当成业务请求处理的消息（后果同样是 RS 等不到回报）。这个风险由 §3.6 第 T2 条的回归测试点兑现。

### 3.3 D3：分类器落库，但只保留"判定 + 吞探针"

库里能站住的部分是一条判据，而不是一整套业务：判定逻辑（通知的低位字段、来源等于 RS、类型等于 `RS_INIT`）是 13 个 crate 共享的（`grep -rn minix-sef os/**/Cargo.toml` 实测消费方），而"怎么处理"是每个服务的私有语义。于是 `os/libs/minix-sef/src/lib.rs:SefEvent` 只有四个变体：普通请求（`Call`）、信号事件（`Signal`）、出生事件（`Init`）、无效探针（`PingInvalid`）；其中只有有效探针是真正被吞掉的（就地回 pong 后重新收消息，对应 `sef_ping.c:do_sef_ping_request` 的返回 OK 分支）。

IPC 动词靠注入而非直调：`os/libs/minix-sef/src/lib.rs:SefIpc` 只要 `receive` 与 `notify` 两个方法，生产实现转 `minix-sys` 的传输层，测试实现用 `os/libs/minix-sef/src/lib.rs:CannedSefIpc` 脚本化回放（C 没有对应物，它是"把一条时间序列塞进收信口"的测试夹具）。

### 3.4 D4：四类拦截不建模（热更新 / 状态搬移 / 覆盖统计 / 故障注入）

C 的六条腿里，重写侧只做了 init / ping / signal 三条。理由不是工作量，而是依赖：

- 热更新与状态搬移臂（§2.3.9、§2.3.10）依赖本项目的 `live-update` 战役（RS 侧开关已建模为 cargo feature，归 `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md`），库内提前实现会得到一堆永不被调用的分支（死代码）。
- 覆盖统计臂依赖内核的 `USE_COVERAGE` 编译配置，本重写项目的测试策略不走代码覆盖率采集。
- 故障注入臂依赖 RS 侧的 `RS_FI_CRASH` 发起方与内核的受控崩溃语义（§2.3.4 的 `sef_controlled_crash`）。

代价不是"行为不同"而是"行为缺失"：今天这套代码无法被热更新，也无法在初始化时被人为注入崩溃/失败/超时（§2.3.3 第三段那三种测试能力同步缺失）。缺失面已在 §3.6 第 T4 条登记，与 16 篇的热更新实装同批解锁。

### 3.5 D5：信号面的两条臂与一条唤醒兜底

C 的 `sef_signal.c:do_sef_signal_request` 对内核通知要遍历 `SIGK_FIRST..SIGK_LAST` 并区分 `SIGKSIG`/`SIGKSIGSM`，后者还要靠 `sys_getksig`/`sys_endksig` 循环代理他人信号。重写侧的两条臂现在都在 `os/libs/minix-sef/src/lib.rs:sef_receive_status` 里：内核通知形按位升序展开，每一位命中调一次 `on_signal(signo)`；信号管理器形（普通消息携 `m_pm_lsys_sigs_signal.num`）把那个号码交给回调后本身被吞掉，与 C 的 `continue` 同形。服务拿得到的就是 C 的信号号本身，不再是统一的请求类型常量。

还留着的两项差异要说明白：

- **逐位展开在实机上跑不到**。C 的 `sigset_t` 是 128 位（`minix3/sys/sys/sigtypes.h:60-61`），内核信号 71..=74 落在第 3 个字；内核自己的待决信号集仍是 64 位（`os/kernel/src/proc.rs:SigSet` 与其 `os/kernel/src/syscall_signal.rs:88-94` 写明的限制），装不下那四位，所以今天送达的 `SYSTEM` 通知位图恒空。通知载荷已经按 C 的 16 字节宽建好形状（`os/libs/minix-types/src/ipc/notify.rs:SigSetBits`），差的只是生产者那一腿。
- **因此需要一条唤醒兜底**：位图空时回调仍跑一次，参数是 `SEF_SIGNAL_REQUEST_TYPE`。C 在位图为空时一次回调都不跑，这是重写侧有意保留的偏离——服务器的信号处理目前靠“收到通知”这个事件驱动（`os/servers/vm/src/vm_server.rs` 的 `pending_signo.is_some()` 裁决与 `os/net/uds`、`os/net/lwip` 的终止闩都是这种形状），去掉兜底就会静默断掉这些路径。待位宽那一腿补齐，真信号号就能同一条回调腿流过来，兜底自然不再触发。

### 3.6 待办清单（框架侧尚未实现的语义）

| 编号 | 发现 | 证据（C 真值 ↔ 现状） | 建议与优先级 |
|---|---|---|---|
| T1 | 信号集未解析，服务不知道具体信号号 | C：`sef_signal.c:do_sef_signal_request` 两条臂 + `SIGKSIG` 代理循环；现状：两条臂都已落地（notify 形逐位展开、管理器形拦截后吞掉），但实机上通知位图恒空（内核 `SigSet` 仍是 64 位），空位图走一次 `SEF_SIGNAL_REQUEST_TYPE` 唤醒兜底 | 已收（解析上提到库已完成，载荷已按 C 的 16 字节 `sigset_t` 建形）。余项：内核 `SigSet` 拓宽到 128 位（跨层 wire 变更）、`SIGKSIG`/`SIGKSIGSM` 的代理循环建模、各服务接住信号号的语义验证（即 T2 与 IS / VM / PM 解锁面） |
| T2 | 出生事件上浮后，各服务接住它的深度不一 | 已收（`NK4C-WORKLOG.md` §续-423..425）。动手前重验 C 注册面：注册 `sef_setcb_signal_handler` 的重写内服务共六族——IS（`is/main.c:85`）、VM（`vm/main.c:235`）、uds 与 netdriver（`uds.c:1374`、`netdriver.c:976`）、ipc（`ipc/main.c:132`，实体 :101-118：非 SIGTERM 忽略；SIGTERM 且 sem/shm 表双空则 `rmib_deregister`＋`sef_exit(0)`，否则打印告警留守）、RS（`rs/main.c:148`，实体 :631-641：SIGCHLD 排空子进程、SIGTERM 关机、其余忽略）——六族全部按各自 C 实体接线并配宿主测试；mfs/pfs/ptyfs/procfs 经 fs-rt 家族收敛（§续-422，见上一版记录）；sched/mib/vfs/pm/devman 在 C 未注册（全目录 grep 零命中），库默认值即空函数 `sef_cb_signal_handler_null`（sef.h:287 ＋ sef_signal.c:156-158），空闭包即忠实，五站注释已带锚明证（§续-423；其中 PM 的 SYSTEM 通知上浮并驱动拉取循环属既有 [ARCH] 登记，与本表 T1 的位宽限制同源）。接线形状按服务分层：ipc 走 `CallHandler::handle_signal` 三态 `SignalStep{Ignored, WarnedDirty, ExitClean}`，退出归主循环（`RunStep::ExitClean` → `minix_sys::exit(0)`），注销消息与注册消息同挂未通电发送面（§续-424）；RS 的 `get_work` 经 `SefKernelBridge` 走库腿，notify 形信号帧在接收缝吞掉（C `sef.c:231-235` 该帧永不回主循环），判定在帧交还前应用（§续-425）。ipc 与 RS 的传输夹具都与生产走同一条 `sef_receive_status` 腿。devman 的信号面已接线（§续-429）：动手核真源发现「C devman 未注册 handler」是假陈述——devman 的 C main 走 `run_vtreefs`（main.c:88），libvtreefs 的 `sef_local_startup` 注册了 `got_signal`（vtreefs.c:59/:39-46，SIGTERM→`fsdriver_terminate`），与 procfs 同族（注册在链接的库里，目录级 grep 查不到）；PD-26 的「删除」决议前提被真源证伪，按用户实施口径改执行忠实行——`SefHooks::on_signal` 死方法仍删（它的链从未接通），信号语义落在传输层锁存（`MinixTransport::terminate`＝C `running` 全局的对位物）＋PD-27 逃生门 | 余项：管理器形发方链未通电（PM 卡 asynsend3 重试表、RS `signal_manager` 无生产调用者），接线服务的可观察效果今日仍在宿主面；RS 的 boot 期收信腿（C `main.c:37` 与 :795 同走 SEF）本树保持裸收、已在现场注明；`SIGKSIG`/`SIGKSIGSM` 的代理循环仍未建模；库层 `sef_cancel` 等价物已定计并落地（PD-27 甲：返 `EINTR`，`SefCancel` 令牌，§续-428） |
| T3 | `process_init` 的六段动作（§2.3.3）无统一落点 | 已落地（PD-34 定稿后，`NK4C-WORKLOG.md` §续-431）。**丙**＝`minix-sys::syscall::sys_statectl` 线形包装（C sys_statectl.c:3-11 形状，载荷三件进 `m_lsys_krn_sys_statectl`，附钉形测试）；**甲**＝`minix_sef::process_init` 编排面——按 C 六段顺序统辖：段一清 IPC 过滤经消费方供给的 `statectl` 动词发出（C `assert(r == OK)`＝失败即出生失败上抛）、按类型分派归消费方回调、回报消息由库构造（`sef_init_reply`）交消费方回报腿发送；未建模槽位（state-transfer grant／`SEF_STATE_TRANSFER_GID`、调试标志短路、munmap/`cpf_reload`/`senda_reload`/`SYS_STATE_SET_STATE_TABLE`）按决议登记槽位注释、不顺手发明语义（PD-24 同批正交）。首消费方＝mib 出生臂（`MibKernel::statectl` 缝：生产臂 trap 直连真接线、mock 默认 OK 模拟接受型内核）；其余出生臂的迁移为渐进欠账。库测 19＋3（段一恰一次/LU 拒绝透传/段一失败分派不达）＋mib 176 全绿 | 余项＝其余服务出生臂逐个迁移到编排面（渐进）；真机段一的内核往返在门里随 mib 请求路真跑 |
| T4 | 热更新 / 状态搬移 / 覆盖统计 / 故障注入四类拦截未建模 | 见 §3.4；已定案（PD-24 冻结决议＝乙：本轮不建模，登记 `[ARCH]` 演进项）——任一类启用前先补回归哨兵测试（§5.2 第 6 项）；另见 §5.3 同构表的热更行与 PD-34 的槽位注释（调试标志短路已随 `process_init` 槽位登记） | 优先级中（与 `live-update` 战役同批）；人为注入崩溃/超时能力缺失会影响 15/16 篇的恢复路径验证 |
| T5 | 同一个 C 语义对应三套 trait（`SefCallbacks` ×2 + `SefHooks`） | `os/servers/rs/src/sef.rs:SefCallbacks`、`os/servers/is/src/sef.rs:SefCallbacks`、`os/servers/devman/src/hooks.rs:SefHooks` | 交代码卓越度裁决（合并或分层）；本文档只记录形状差异 |
| T6 | `sef_init_reply` 不填 `m_source` | C：`sef_init.c:process_init` 明确 `m.m_source = sef_self_endpoint`；现状：`os/libs/minix-sef/src/lib.rs:sef_init_reply` 只填 `m_type` 与 result | [待验证]——若本项目的传输层会由内核回填来源，则不构成缺陷；若不回填，出生应答在 RS 侧会被归错来源。需对照 `os/libs/minix-sys` 的 `sendrec` 实现核实 |
| T7 | 出生回报腿的 IPC 原语选型已修但易复发 | C：默认腿 `ipc_sendrec`（§2.3.5）；现状：`os/servers/vfs/src/main_loop.rs:send_birth_reply` 与各服务 transport 的 `send_rec` 方法已对位 | 属回归风险；测试点见 §5 第 4 项 |

---

## 4. 实现详解

### 4.1 分类器：C 的每一段在 Rust 里的对应位置

`os/libs/minix-sef/src/lib.rs:sef_receive_status` 是 §2.3.6 那张骨架的直接对位。逐段比对（"行为"列写不同之处，相同就写同）：

| C 片段 | Rust 片段 | 行为 |
|---|---|---|
| `sef.c:sef_receive_status` 的 `while(TRUE)` | `lib.rs:sef_receive_status` 的 `loop` | 同 |
| `sef.c:161` 的 `sef_self_receiving` 逃生门（`sef.c:sef_cancel`） | 无 | 不同：重写侧没有 `sef_cancel` 等价物，从信号回调里退出主循环目前只能靠服务自己的状态位 + 外层循环判断（§5 第 5 项） |
| `sef.c:177` 的 `ipc_receive(src, m_ptr, &status)` 失败即返 | `lib.rs:sef_receive_status` 的 `let status = ipc.receive(src, msg)?` | 同（错误直接上抛，不重试） |
| `sef.c:184-193` 按来源改写 `m_type` | `lib.rs:sef_receive_status` 的 `is_ipc_notify(status)` 分支 | 同（`os/libs/minix-sef/src/lib.rs:is_ipc_notify` 就是取 status 低 16 位与 `CALL_NOTIFY` 比较） |
| `sef.c:196-206` 出生臂：**吞掉**（除 VM fresh） | `lib.rs:sef_receive_status` 的 `SefEvent::Init`：**上浮** | 不同，见 §3.2 |
| `sef.c:208-216` 探针臂：回 pong 后 `continue` | `lib.rs:pong_via_ipc` + `continue` | 同（判定条件也同：`lib.rs:is_sef_ping_request` 要求通知 + 来源 RS + 类型 `NOTIFY_MESSAGE`） |
| `sef.c:183-193` 改写后的 `SEF_SIGNAL_REQUEST_TYPE` 进入信号臂（`sef.c:230-238`）：进 `do_sef_signal_request` 解析信号集，返回 OK 即 `continue` 吞掉 | `lib.rs:sef_receive_status` 的两条臂：notify 形逐位展开后调 `on_signal(signo)` 并上浮 `SefEvent::Signal`；管理器形（普通消息携 `num`）调完回调就 `continue` | 半同：两形都解析了（§3.5）；不同：内核通知那一形重写侧上浮为事件（C 吞），位图为空时多一次带着请求类型的唤醒兜底 |
| `sef.c:219-228` 热更新臂 / `sef.c:241-261` 覆盖统计与故障注入臂 | 无 | 缺失，见 §3.4 |
| `sef.c:263-270` 落到 `default` 后 `break` 返回 | `lib.rs:sef_receive_status` 末尾的 `SefEvent::Call(m_type)` | 同 |
| `sef_init.c:process_init` 尾部的回报构造 | `lib.rs:sef_init_reply` | 部分：构造了 `m_type` 与 result，未构造 `m_source`（§3.6 T6） |

还有一个形状上的选择值得注意：返回值类型 `os/libs/minix-sef/src/lib.rs:SefReceive` 把"来源 + 消息 + 状态字 + 分类结论"打包返回，而 C 把来源与状态字写回调用方给的指针。后者的代价是调用方必须自己记住去看 `status`，前者把"分类结论"这个信息并入了类型——服务写 `match recv.event` 而不是 `if is_ipc_notify(status) && m_source == RS`。这不是风格差异：前者让"漏看状态字"这类错误在编译期就不可能出现（本项目的 VM 服务器曾因误把通知当业务请求处理踩过同类问题，归 `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md`）。

### 4.2 真实消费点清单

以下站点均经 `grep -rn "minix_sef::sef_receive_status" os/ --include=*.rs` 实测命中（只取生产代码，不含构建产物）：

| 消费方 | 函数 | 拿到的事件形状 |
|---|---|---|
| `os/servers/vm/src/vm_server.rs` | `VmServer::run_once` | 四变体全用（VM 是第四个消费方，出生臂带自己的异步豁免） |
| `os/servers/vfs/src/main_loop.rs` | `run` | 启动阶段的 PM 握手段用 `Call`，出生回报走 `send_birth_reply` |
| `os/servers/mib/src/server.rs` | `Server::run_once`（泛型实现 `impl<K: MibKernel, S: MibServices, I: MibIpc + minix_sef::SefIpc> Server<K, S, I>`） | `Call` + `Signal`/`Init` 分派 |
| `os/servers/sched/src/kernel_api/transport.rs` | `sef_filtered_receive` | 包一层给调度器的接收口 |
| `os/servers/is/src/sef.rs` | `SefTransport for …::receive` | 返回 (来源, 状态字) 元组 |
| `os/servers/devman/src/ipc/minix.rs` | `receive_message` | 同上形状，返 `Option<i32>` |
| `os/fs/fs-rt/src/ipc.rs` | `SeIpcAdapter::receive` | 文件系统服务共用的接收口（八个 fs 二进制均经它） |
| `os/net/lwip/src/server.rs` | `wait_for_init` 与主循环两处 | 网络服务：先等出生再进循环 |
| `os/net/uds/src/server.rs` | `run` | 同上 |

另有三处在注释里声明了"将来切到分类器"但尚未切：`os/libs/minix-driver-rt/src/transport.rs`、`os/libs/minix-driver-rt/src/kernel.rs`、`os/servers/input/src/serve.rs`。它们是驱动与输入服务的事件循环，目前直写 `receive`。

### 4.3 服务侧回调集的形状差异

同一个 C 语义在三个服务里有三种声明（差异已登记 §3.6 T5，此处只记录形状，不做判定的取舍）：

| | RS | IS | devman |
|---|---|---|---|
| trait | `os/servers/rs/src/sef.rs:SefCallbacks` | `os/servers/is/src/sef.rs:SefCallbacks` | `os/servers/devman/src/hooks.rs:SefHooks` |
| 槽位数 | 7（fresh / restart / lu / init_response / lu_response / signal_handler / signal_manager） | 4（fresh / restart / lu / signal_handler） | 2（`init_server`、`on_signal`） |
| 与 §2.4 表的对应 | 逐一对位 RS 的 7 次 `sef_setcb_*` | 对位 IS 的 4 次登记 | 对位 devman 自己的启动路径（未拆出出生/重启/热更新三型） |
| 初始化类型 | `os/libs/minix-sef/src/lib.rs:SefInitType`（Fresh/Lu/Restart） | 同 | 同 |

为什么 RS 要写全 7 个：它就是出生协议的另一端，其它服务只处理与自己有关的那几类（例如从未被热更新过的驱动不登记 `lu_state_save` 以外的腿）。这也是本权威讲述点定在 03-stage-rs 的理由：只有一个服务能在一篇里把六条腿都碰到。

### 4.4 与 RS 侧半的接缝

本档只管库侧半，接缝处一句话对位（机制在 12 篇）：

| 库侧 | RS 侧 | 消息 |
|---|---|---|
| 启动期 `ipc_receive(RS_PROC_NR)`（§2.3.1 第 3 条路） | `minix3/minix/servers/rs/utility.c:init_service` 组装 `m_rs_init.*` 后 `rs_asynsend` | `RS_INIT`（异步送达） |
| 回报腿 `sef_cb_init_response_rs_reply`（§2.3.5） | RS 的 ready 编排与合法性判读 | `RS_INIT` + result |
| 探针臂 `sef_ping.c:do_sef_ping_request` | RS 的周期探活（归 `07-rs-period-heartbeat.md`） | 通知 → 通知 |
| 热更新臂 `sef_liveupdate.c:do_sef_lu_request` | RS 的 prepare/update 编排（归 `16-rs-live-update.md`） | `RS_LU_PREPARE` + result |

一句话把两半合上：**RS 只能"发消息、等回报、做仲裁"，能不能初始化、初始化成没成，全在服务自己的回调里**——这正是 §1.1 那个矛盾的两侧。

### 4.5 读这段代码的最小形状

下面这段不是任何现存文件的逐字引用，而是把 §3.2 / §3.3 / §4.1 拼回一个服务主循环的教学简化形状，用于自检"上浮事件到底怎么用"：

```rust
// 教学简化：一个服务主循环里，分类器与业务分派表的分工
loop {
    let recv = minix_sef::sef_receive_status(&mut ipc, Endpoint::ANY, &mut msg, &mut on_signal)?;
    match recv.event {
        // 出生事件：自己跑初始化，再用共享助手回报（阻塞 sendrec 语义）
        minix_sef::SefEvent::Init(_init_type) => {
            let result = server.init_fresh();
            ipc.send_rec(&Endpoint::RS, &mut minix_sef::sef_init_reply(result))?;
        }
        // 信号事件：解析具体信号号是服务器的责任（现状差异见 §3.5）
        minix_sef::SefEvent::Signal(_kind) => server.handle_signals(&recv.message),
        // 普通请求：进业务分派表
        minix_sef::SefEvent::Call(m_type) => server.dispatch(recv.source, m_type, &mut msg)?,
        // 无效探针：RS 发来了通知但不是 ping，当普通消息处理
        minix_sef::SefEvent::PingInvalid => server.deliver(recv.message)?,
    }
}
```

这段里三个细节分别来自 §2.3.6 第 4 点（只有 `Call` 进分派表）、§3.2（出生事件自己应答）、§3.5（`Signal` 背后可拿到的已经是真信号号：逐位展开与管理器形拦截都在库里跑，服务侧只需决定拿到号后做什么——那就是 T2）。

---

## 5. 测试要点

> 计数口径：下列现有测试逐条经 `grep -n "fn test_" os/libs/minix-sef/src/lib.rs` 命中（共 13 个），本表是静态口径；运行时计数由 `cargo test -p minix-sef` 复核，两者不一致时以运行结果为准。

### 5.1 库内现有十三个单测各自钉住了哪条分支

| 测试 | 钉住的行为 | 对应源码位置 |
|---|---|---|
| `test_ping_intercepted_and_swallowed` | 两条 RS 通知被吞掉并各回一次 pong，第三条普通消息原样上浮 | §2.3.7、§4.1 的探针行 |
| `test_system_notification_surfaces_signal` | `SYSTEM` 通知上浮为 `SefEvent::Signal` 且回调被叫 | §2.3.8（识别那一层；解析面见下面四行） |
| `test_rs_non_notification_is_plain_call` | 来向是 RS 但不是通知 → 不进探针分类 | §2.1 判据的"投递方式"维度 |
| `test_receive_error_passthrough` | 收信失败直上抛错误码，不重试 | §2.3.6 的 `if(r != OK) return r` |
| `test_rs_init_request_surfaces_init_event` | 非通知的异步投递也能识别为出生，并带上 `init_type` | §2.3.1、§3.2 |
| `test_rs_init_wins_over_notify_band` | 判定按类型+来源、与投递方式无关，不误入探针分支 | §3.2（C 的出生判据也无通知条件） |
| `test_rs_non_ping_notification_is_invalid` | RS 通知但类型不是 `NOTIFY_MESSAGE` → `PingInvalid` 且不回 pong | §2.3.7 判据三元组的第三维 |
| `test_sef_init_reply_carries_result` | 回报消息携带 result，成功与 `ENOSYS` 两种都能正确装载 | §2.3.5（构造面；发送面属 T6）|
| `test_notify_sigset_walks_kernel_signals_ascending` | 通知位图里 `SIGKMEM`+`SIGKSIG` 两位 → 回调按升序收到 `[71, 74]`，事件形状不随位集变 | §3.5第一条臂（C `sef_signal.c:97-113`）|
| `test_notify_low_word_bits_are_outside_the_kernel_window` | 低 64 位全置位（用户/系统信号 1..=64）不展开，只走一次兜底 | §3.5 窗口边界（C 只走 `SIGK_FIRST..SIGK_LAST`）|
| `test_notify_empty_sigset_wakes_the_handler_once` | 空位图 → 恰好一次带着 `SEF_SIGNAL_REQUEST_TYPE` 的回调（删它就断掉 VM/`uds`/`lwip` 的唤醒路径）| §3.5 的唤醒兜底段 |
| `test_manager_signal_request_is_swallowed_after_the_callback` | 管理器形的号码交给回调后消息被吞，服务看到的是下一条普通消息且不回 pong | §4.1 信号臂的“C 吞”那半（`sef.c:233-236`）|
| `test_signal_request_from_init_slot_or_above_is_a_plain_call` | 同类型同载荷但发方是 init 槽或用户槽 → 不得拦截，原样上浮为 `Call` | §3.6 T1 的判据方向（C `sef.h:265` 的 `<`）|

### 5.2 需要补的测试点（由第 3、4 章的决策与缺口推导）

1. **出生应答的发送原语形状**（推自 §2.3.5 与 §3.6 T7）：断言出生回报走的是"发送并等回复"而不是单向发；反面例子是只返 `Ok(())` 而不校验消息体与目标端点，那种断言永真，测不出退化。
2. **信号集解析**（推自 §3.5 与 T1）：已补——给定一个带 `SIGKMEM` 与 `SIGKSIG` 的通知，服务能分别得到两个信号号（`test_notify_sigset_walks_kernel_signals_ascending` 钉升序与号码本身），低字位不算内核信号、空位图只走一次兜底也各有测试；管理器形的号码直通与“不拦截 init 及以上端点”同样已钉。剩余缺口：`SIGKSIG` 触发的代理循环（`sys_getksig`/`sys_endksig` 三段循环）仍无对位物，以及 §3.5 第一条差异——实机通知位图因 `SigSet` 仍为 64 位而恒空，展开分支目下只在宿主可驱动。
3. **`Init` 与 `Call` 不串台**（推自 §3.2 的上浮决策）：同一条 `RS_INIT` 不应同时被当成业务请求处理；需要一个跨服务回归（选一个真实消费方，如 `os/servers/mib/src/server.rs:run_once`）而不是只在库里测。
4. **异步腿的一次性语义**（推自 §2.3.5 第二条腿与 VM）：首条回报异步、后续回到阻塞；重写侧目前只 VM 带这个豁免，回归面应限定在 VM（归 `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`）。
5. **从信号回调里退出主循环**（推自 §2.3.6 第 1 点的 `sef_cancel`）：重写侧仍无 `sef_cancel` 等价物（库层没有「本轮不想再收」的入口），但服务侧的形状已有实例：IS 把回调里的决定当成 `LifecycleAction` 一步步传上主循环——回调置位、`step()` 提前返回、`run()` 才真的 `exit(0)`（`os/servers/is/src/lib.rs:step`/`run`，归 `notes/rewrite/fork-syscall-rewrite/08-stage-is/01-is-init-main.md` §2.8）。这带着一个必须记下的偏离：C 是在 `sef_receive_status` 内部直接 `exit(0)`，那一帧永不返回调用方；本树的 `receive` 已经返回了那一帧（携带信号的 `SYSTEM` 通知），服务必须在分派**之前**先看退出位——IS 的测试钉住了这一点（那一帧不产生回复也不产生非法请求告警）。该未定计已由 PD-27 定案并落地（§续-428）：库层返 `EINTR`（`SefCancel` 令牌，循环顶检查），uds/lwip/fs-rt/ipc/IS 五服务已接「收到中断→检查退出标志→退出」的同拍形状。ipc 与 RS 已按同族形状落地（ipc：回调报三态 `SignalStep`、主循环持退出权，§续-424；RS：`get_work` 吞信号帧后在交还前应用判定，§续-425），三实例形状一致——决定不能走返回值，只能由回调把状态带上主循环。
6. **未建模四类拦截的回归哨兵**（推自 §3.4）：当 `live-update` 战役开工时，至少要先补一个"收到 `RS_LU_PREPARE` 当前会落到 `Call` 分派表"的当前行为断言，否则将来改造时说不清"之前是怎么表现"的（这类断言只锁当前行为，不为缺失功能开证）。

### 5.3 三套 trait 的逐方法同构表（PD-25 的交付物）

C 的回调注册面是 `sef.h` 的 `sef_cb_*` 函数指针族；重写侧它分散在 rs 的 `SefCallbacks`（全回调集）、devman 的 `SefHooks`（本审计后仅 init 一轴）、fs-rt 的 `ServerHooks`（信号三态决策＋init）与各服务器收信点的 `on_signal` 闭包里。逐方法对表（对表本身即 PD-25 要求的前置物；「同构才合并」的裁定按轴进行）：

| C 回调 | rs `SefCallbacks` | devman `SefHooks` | fs-rt `ServerHooks`/闭包 | 同构判定 |
|---|---|---|---|---|
| `sef_cb_init_fresh_t` | `init_fresh` | `init_server`（fresh-only，restart 走 STATEFUL 空体） | `init` 回调（`InitKind::Fresh`） | 语义同构（fresh 出生），参数与错误模型各异——不合并类型 |
| `sef_cb_init_restart_t` | `init_restart` | （STATEFUL 空体，vtreefs.c:58） | `InitKind::Restart` → 诚实拒 `ENOSYS` | 语义分歧（rs 真 restart，其余拒）——不同构 |
| `sef_cb_init_lu_t` 及热更五件套 | `init_lu`/`lu_*` 族 | 无 | 无 | 仅 rs 有消费者——留在 rs 子集视图（随 PD-24 延后建模） |
| `sef_cb_init_response_t` / `sef_cb_lu_response_t` | `init_response`/`lu_response` | 无（出生回报在各传输层） | 无（`sef_init_reply` 库根半＋服务发送半） | 语义同构（回报腿）但调用者形状各异——不合并 |
| `sef_cb_signal_handler_t` | `signal_handler`（SIGCHLD/SIGTERM） | 已删（§续-429；语义在传输层锁存） | `SignalDecision: FnMut(i32) -> SignalAction` | **语义同构、参数形状不同构**（裸 i32 分派 vs 三态词表）——按冻结决议统一的是「`on_signal` 契约名＋`SignalAction` 三态词汇表（现居 `minix_sef`），不是 trait 类型 |
| `sef_cb_signal_manager_t` | `signal_manager`（代内核逐目标转发） | 无 | 无 | 仅 rs——RS 专属族保持边界 |
| `sef_cb_signal_manager_null` 等预置空实现 | （库默认即空） | （同） | （同） | 库默认面，无 Rust trait 对应 |

裁定记录：三套 trait **不**合并为一个超级 trait——方法集互为子集、信号轴参数形状不同（i32 分派与三态词表各有消费者）；统一的是语义层：`on_signal` 是信号轴的统一契约名，`SignalAction{Ignore, Terminate, SyncThenTerminate}` 是该轴的唯一三态词表（定义在 `minix_sef`，fs-rt re-export，§续-430），每个回调的 C 锚点与语义规范散见各实现文档。这条「语义统一≠类型统一」即 PD-25 冻结文本与 PD-33 公理的共同结论。

---

## 6. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/00-rs-overview.md` —— RS 全局心智模型与机制归属导航
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md` —— RS 自己的 `sef_local_startup` 与四步 boot（本档的上游调用点）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md` —— 主循环收信形状（理解"门口拦截"的前置）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md` —— 探针与心跳的发起侧
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md` —— 出生握手的 RS 侧半
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md`、`…/17-rs-state-data.md`、`…/18-rs-self-lifecycle.md` —— 热更新的状态机、数据形状、RS 自身路径
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` —— libsys 动词签名面
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/99-rs-global-concepts.md` —— `SEF_*` 常量的权威定义位置
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` —— VM 的异步豁免与 `SIGKMEM` 使用方
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/01-pm-init-main.md` —— PM 视角的注册表与 `process_init` 路径
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/13-signal-flow.md` —— 信号在 PM/内核/信号管理器之间的完整流向
- `notes/rewrite/fork-syscall-rewrite/15-stage-fs/` 与 `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/` —— 八个文件系统二进制与驱动框架的事件循环

---

## 附录：本文档的代码块清单与分类

由 `tools/doc-snippet-extract.sh` 抽取（只统计 `rust`/`ignore`/`no_run` 围栏块），共 1 个：

| 位置 | 语言 | 分类 | 说明 |
|---|---|---|---|
| L570-L588（§4.5） | rust | 教学简化 | 不要求可编译；变量 `ipc`/`msg`/`server`/`on_signal` 为叙述引入的抽象名，方法名 `send_rec`/`init_fresh`/`dispatch` 与现存实现同族但不限定某一个 crate |

C 代码围栏块（§2.3.1、§2.3.3、§2.3.5、§2.3.6、§2.3.7、§2.3.8 六处）均为**逐字引用**自 `minix3/minix/lib/libsys/` 下的对应函数体（个别行做了省略，省略处用 `…` 或注释行标注），不参与上述四分类统计。

