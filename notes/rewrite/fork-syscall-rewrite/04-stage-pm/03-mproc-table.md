# 03-mproc-table: 进程表——槽位、endpoint 与 PID 的身份管理

> **状态**: 完整版（2026-08-17 首版）
> **定位**: 阶段 1 启动与进程模型——`sef_cb_init_fresh` 第 5 步（`minix3/minix/servers/pm/main.c:177-229` boot image 填充）之后、主循环（04）使用的基础设施；03 是"PM 对进程集合的索引视图"
> **源码**: `minix3/minix/servers/pm/{glo.h, utility.c, const.h, forkexit.c}` + `minix3/minix/kernel/system/do_fork.c`（generation 归属）+ `minix3/minix/include/minix/endpoint.h`
> **Rust 模块**: `os/servers/pm/src/mproc/{table.rs, pid_gen.rs, constants.rs, context.rs}`
> **前置依赖**: 01-pm-init-main（启动链）、02-mproc-struct（Process 四层）
> **不覆盖（移交）**: 槽位使用方（07-fork/09-exit/10-wait）、endpoint 的**生成**（内核 do_fork + VM fork 回复）、PID 之外的进程身份字段（02）

---

## 1. 概念：进程表——PM 如何索引"活着的进程"

### 1.0 章节引言

02 回答了"PM 进程表的**每一行**长什么样"（`Process` 四层模型）。本档回答
**表本身**的问题：PM 如何在 256 个槽位中管理"谁活着、谁死了、谁是谁"。

进程表的索引问题有三个层次，每个层次给出一个不同的身份：

1. **槽位（slot）**：`[0, NR_PROCS)` 的物理索引，是内核/VM/VFS/PM 四张
   进程表共享的坐标；
2. **endpoint**：`generation << 15 + slot` 的跨服务 IPC 身份，携带"代数"
   防陈旧引用；
3. **PID**：POSIX 用户态可见身份，进程组 ID 也从同一空间借用。

本档回答三个问题：

1. **为什么**需要三层身份？它们各自解决什么问题、生命周期有什么不同？
2. **为什么** PID 分配要"单调递增 + 冲突扫描"，而不是简单的计数器或位图？
3. **为什么** endpoint 要带 generation？**generation 的所有权在谁手里**？

### 1.1 三层身份：槽位、endpoint、PID

Minix3 的微内核把进程状态拆成四张表（02 §1.1），四张表靠**同一个槽位编号**
对齐。但槽位编号只在表内部有意义——跨服务通信时，必须有一个不随
"槽位复用"失效的身份，这就是 endpoint；用户程序看到的身份又是另一个东西，
这就是 PID。

| 身份 | 值域 | 谁分配 | 谁消费 | 复用规则 | 陈旧防护 |
|------|------|--------|--------|---------|---------|
| 槽位 slot | `[0, NR_PROCS)` | PM 表管理（03） | PM/VM/VFS/内核四表 | 空槽可复用 | 无（表内坐标） |
| endpoint | `generation<<15 + slot` | **内核**（fork 时 bump）+ 启动映像 | 跨服务 IPC | generation 每次复用 +1 | 代数不匹配 → EDEADEPT |
| pid | `[2, NR_PIDS]` | PM（`get_free_pid`） | 用户程序（POSIX） | 单调递增 + 冲突扫描 | 对活进程唯一 |

三层身份的**生命周期不同**，是理解本档的主线：

- 槽位随进程创建/退出在 PM 内分配、释放；
- endpoint 是"内核视角的槽位 + 代数"——PM 只**存储和验证**它，从不生成它；
- PID 是 PM 的私有命名空间，只在 PM 表内保证唯一。

> **与 Monolithic 的对比**：Linux 把"槽位"和"endpoint"合并成一个
> `struct pid`/`task_struct` 引用体系（进程 ID 本身就是内核句柄），没有
> 独立的 generation 维度——它用 RCU + 引用计数保证陈旧引用安全。Minix3
> 的 generation 是**跨服务消息**的轻量防伪机制（详见 §3.8）。

### 1.2 表的两类操作：索引与分配

进程表对外的操作分两类，本档各占一半篇幅：

- **索引**：把"外部身份"翻译成"表内坐标"——
  - endpoint → 槽位：`pm_isokendpt`（utility.c:108），三层检查（范围/代数/存活）；
  - pid → 槽位：`find_proc`（utility.c:76），线性扫描活进程。
- **分配**：管理"谁占据哪个槽位"——
  - 找空槽：`next_child` 轮转扫描（forkexit.c:68-74）；
  - 分配 PID：`get_free_pid` 单调递增 + 冲突扫描（utility.c:34-52）；
  - 释放：`cleanup` 清计数与身份（forkexit.c:795-806）。

两类操作共享一个不变量：**槽位编号是四表对齐的唯一坐标**。索引操作把外部
身份翻译成这个坐标，分配操作决定哪个坐标被占用——两者必须一致，否则
"PM 说进程在槽 5，VFS 却以为在槽 6"。

### 1.3 PID 为什么稀缺：NR_PIDS、INIT_PID 保留、进程组冲突

`NR_PIDS = 30000`（const.h:3），但可用范围只有 `[2, 30000]`：

- `NO_PID = 0` 保留（const.h:8，"pid value indicating no process"）；
- `INIT_PID = 1` 保留给 INIT（const.h:9），且永不重新分配。

为什么偏偏是 30000？const.h:4-5 的注释给出了理由：

> magic constant: some old applications use a 'short' instead of pid_t.

`short` 的最大值是 32767，30000 保证 PID 被截断成 `short` 后仍可表示。
这是**二进制兼容**约束——不是性能或审美选择。

PID 空间还有一个隐形的借用者：**进程组**。`mp_procgrp` 存的是进程组
组长的 PID，因此 `get_free_pid` 的冲突扫描必须同时检查 `mp_pid` 和
`mp_procgrp`（utility.c:45）——否则新 PID 可能与某个活着的进程组 ID 撞车，
信号广播（`killpg`）就会打到错误的进程。

### 1.4 endpoint generation：防陈旧引用的代数机制

endpoint 的格式（endpoint.h）：

```text
endpoint = (generation << 15) + slot
```

`generation` 的语义（endpoint.h 注释）：

> The generation number is a per-slot number that gets increased by one every
> time a slot is reused for a new process. The generation number minimizes
> the chance that the endpoint of a dead process can (accidentially) be used
> to communicate with a different, live process.

也就是说：槽位 7 的第一代进程 endpoint 是 `(0<<15)+7`；它退出后槽位 7 被
新进程复用，新进程的 endpoint 变成 `(1<<15)+7`。任何还在用旧 endpoint 的
消息都会在 `pm_isokendpt` 的代数检查处失败——**死的进程不能借尸还魂**。

**关键事实：generation 的递增发生在内核的 `do_fork`**（`kernel/system/do_fork.c:69-72`）：

```c
gen = _ENDPOINT_G(rpc->p_endpoint);
if (++gen >= _ENDPOINT_MAX_GENERATION)	/* increase generation */
    gen = 1;				/* generation number wraparound */
rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);	/* new endpoint of slot */
```

新 endpoint 经 VM 的 `vm_fork` 回复传回 PM，PM 只做
`rmc->mp_endpoint = child_ep`（forkexit.c:111）——**PM 不拥有 generation**。
这个"所有权"问题正是 Rust 重写时最容易犯的错误（§3.3 修正了现有实现）。

### 1.5 容量纪律：procs_in_use + LAST_FEW

`procs_in_use`（glo.h:9）是活进程计数。它有两个用途：

1. 容量检查（forkexit.c:60-62）：表满（`== NR_PROCS`）直接 EAGAIN；
2. 保留区纪律：计数达到 `NR_PROCS - LAST_FEW` 后，**只有 effective uid 为
   0 的进程还能 fork**。

`LAST_FEW = 2`（`forkexit.c:32` 本地定义——`#define LAST_FEW 2`，PM 私有
常量，非 include 头文件；`minix-types` com 模块同值）是给 root 预留的应急槽位——普通用户耗尽进程槽后，root 仍需能创建
进程来恢复系统。注意 C 的检查用的是 **`mp_effuid`**（effective uid），
不是 real uid——setuid 程序提升权限后应能使用保留区（§3.6 修正了 Rust 现状）。

### 1.6 本章小结

- 三层身份（slot/endpoint/pid）生命周期不同：槽位随创建/退出、endpoint
  由内核换代、pid 由 PM 分配；
- 表的两类操作（索引/分配）共享"槽位 = 四表对齐坐标"不变量；
- PID 空间稀缺来自 `short` 兼容 + 进程组借用，冲突扫描必须含 `mp_procgrp`；
- **generation 的所有权在内核/VM，PM 只验证不生成**——本档的心智模型；
- 容量纪律用 `procs_in_use` + `LAST_FEW` + **effective uid** 实现。

---

## 2. C 源码分析

### 2.1 glo.h：全局表与调用上下文

`glo.h`（7-18 行）声明 PM 的文件级全局：

| 全局 | 行号 | 语义 |
|------|------|------|
| `mp` | :8 | 当前调用者的槽指针（`mp = &mproc[who_p]`，main.c:77） |
| `mproc[]` | mproc.h:83 | 进程表本体（glo.h 只声明 `mp` 指针，数组在 mproc.h） |
| `procs_in_use` | :9 | 活进程计数 |
| `m_in`/`who_p`/`who_e`/`call_nr` | :16-18 | 当前消息与调用上下文 |
| `monitor_params` | :10 | 启动参数缓冲（01） |

`mp` 宏（glo.h:8）是 C 的隐式上下文——Rust 侧由 `PmContext`（context.rs）
显式化（ARCH A-3，03 §3.1）。`procs_in_use` 是本档的核心全局。

### 2.2 pm_isokendpt：三层检查的 errno 契约（utility.c:108-121）

```c
int pm_isokendpt(int endpoint, int *proc)
{
	*proc = _ENDPOINT_P(endpoint);
	if (*proc < 0 || *proc >= NR_PROCS)
		return EINVAL;
	if (endpoint != mproc[*proc].mp_endpoint)
		return EDEADEPT;
	if (!(mproc[*proc].mp_flags & IN_USE))
		return EDEADEPT;
	return OK;
}
```

三层检查的顺序就是 errno 契约：

| 层 | 检查 | 失败 errno | 含义 |
|----|------|-----------|------|
| 1 | 槽位范围（`_ENDPOINT_P` 提取 slot，`< 0 || >= NR_PROCS`） | `EINVAL` | 编码非法：内核 task 负槽位、ANY/NONE/SELF 特殊值、越界槽 |
| 2 | 代数匹配（`endpoint != mproc[slot].mp_endpoint`） | `EDEADEPT` | 陈旧引用：槽位被复用，generation 对不上 |
| 3 | 存活（`!(mp_flags & IN_USE)`） | `EDEADEPT` | 槽位存在且代数匹配，但进程已退出 |

调用点（全部 grep 实证）：

- **主循环**（main.c:75）：`if (pm_isokendpt(who_e, &who_p) != OK) panic(...)`
  ——主循环收到无效 endpoint 是协议/内核错误，直接 panic；
- **服务 handler**：exec.c:74/140、signal.c:300、event.c:241、misc.c:176、
  alarm.c:323——返回 errno 给调用者（Rust 侧经 `EndpointError::to_errno`）。

第 2 层与第 3 层都返回 EDEADEPT，但**检查对象不同**：第 2 层防"槽位被
新进程复用后的陈旧引用"，第 3 层防"槽位已释放但代数恰好没变"（见 §2.5）。

### 2.3 find_proc：PID → 槽位（utility.c:76-85）

```c
struct mproc *find_proc(lpid)
pid_t lpid;
{
  for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
	if ((rmp->mp_flags & IN_USE) && rmp->mp_pid == lpid)
		return(rmp);
  return(NULL);
}
```

- 线性扫描全部槽位，匹配 `IN_USE && mp_pid == lpid` 的**第一个**；
- 返回 NULL 时调用方映射为 `ESRCH`（trace.c:63 等）；
- 调用点：trace.c:63/103/115/140（ptrace 目标查找）、misc.c:159/257
  （`getprocnr`/`reboot`）。

注意 `IN_USE` 检查：释放槽的陈旧 `mp_pid`（cleanup 只清零，见 §2.5）不参与
匹配——**PID 查找只对活进程有效**。

### 2.4 get_free_pid：单调递增 + 冲突扫描（utility.c:34-52）

```c
static pid_t next_pid = INIT_PID + 1;		/* next pid to be assigned */

do {
	t = 0;
	next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
	for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
		if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
			t = 1;
			break;
		}
} while (t);					/* 't' = 0 means pid free */
```

四个要点：

1. **单调递增**：`next_pid` 是 `static`，跨调用持续；每次先 +1，到
   `NR_PIDS` 后环绕回 `INIT_PID + 1`（=2）——`NO_PID`/`INIT_PID` 永不分配；
2. **冲突扫描含 procgrp**：`mp_pid == next_pid || mp_procgrp == next_pid`
   （§1.3 的进程组借用）；
3. **期望 O(1)**：冲突概率 ≈ `NR_PROCS/NR_PIDS ≈ 256/30000 ≈ 0.85%`，
   99% 以上的情况一次扫描即命中；最坏 O(N)（冲突密集的极端场景）；
4. **扫描全部槽位**（含释放槽）：释放槽的陈旧 `mp_procgrp` 会造成
   **额外跳过**——这是 C 的保守行为，Rust 只扫活进程（§3.4 ARCH 差异）。

调用点：main.c:209（boot 系统进程）、forkexit.c:119/219（fork/srv_fork）。

### 2.5 slot 分配/释放：forkexit.c 的容量纪律与 cleanup

**分配**（do_fork，forkexit.c:60-74）：

```c
if ((procs_in_use == NR_PROCS) ||
    (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)) {
	printf("PM: warning, process table is full!\n");
	return(EAGAIN);
}
do {
    next_child = (next_child+1) % NR_PROCS;
    n++;
} while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
if(n > NR_PROCS)
    panic("do_fork can't find child slot");
```

- 容量检查在前（§1.5），槽位扫描在后；`next_child` 是 `do_fork` 内的
  `static`——**先递增再检查**，保证轮转；
- `n > NR_PROCS` 的 panic 是防御路径：容量检查已保证存在空槽，扫描不应失败
  （Rust 用 `Option` 表达，`None` = 表满，无 panic）；
- do_srv_fork（forkexit.c:165-180）同型（RS 专用，08 展开）。

**释放**（cleanup，forkexit.c:795-806）：

```c
static void
cleanup(register struct mproc *rmp)
{
  rmp->mp_pid = 0;
  rmp->mp_flags = 0;
  rmp->mp_child_utime = 0;
  rmp->mp_child_stime = 0;
  procs_in_use--;
}
```

**关键事实**：cleanup **不清理 `mp_endpoint`、不 bump generation**。
释放槽保留陈旧 endpoint，靠第 3 层检查（`!IN_USE` → EDEADEPT）挡住陈旧引用。
新 endpoint 只在内核 `do_fork` 复用槽位时生成（§2.6）。

### 2.6 endpoint generation 的归属：kernel do_fork.c

generation 的完整生命周期（grep 实证）：

| 步骤 | 位置 | 行为 |
|------|------|------|
| 初始 | kernel/proc.c:133 | `p_endpoint = _ENDPOINT(0, p_nr)`（generation 0） |
| 槽位复用 | kernel/system/do_fork.c:69-72 | `gen = _ENDPOINT_G(rpc->p_endpoint); if(++gen >= MAX) gen = 1; p_endpoint = _ENDPOINT(gen, p_nr)` |
| 传给 PM | forkexit.c:111 | `rmc->mp_endpoint = child_ep`（child_ep 来自 `vm_fork` 回复，forkexit.c:78） |
| PM 启动 | main.c:218 | `rmp->mp_endpoint = ip->endpoint`（boot image，generation 0） |
| PM 释放 | forkexit.c:795-806 | **不碰 endpoint** |

结论：**PM 侧不存在任何 generation 运算**。PM 对 endpoint 只有三个动作——
启动时存 boot image 的值、fork 时存 VM 回传的值、验证时比较。任何在 PM
侧 bump generation 的实现都与 C 语义不符（§3.3 修正）。

### 2.7 本档覆盖的符号清单

| C 符号 | 位置 | 文档归属 |
|--------|------|---------|
| `mproc[NR_PROCS]` / `procs_in_use` | mproc.h:83、glo.h:8-9 | §2.1/§2.5 |
| `pm_isokendpt` | utility.c:108-121 | §2.2 |
| `find_proc` | utility.c:76-85 | §2.3 |
| `get_free_pid` | utility.c:34-52 | §2.4 |
| `next_child` 轮转 / 容量检查 / cleanup | forkexit.c:68-74/165-180/795-806 | §2.5 |
| generation 递增 | kernel/system/do_fork.c:69-72 | §2.6 |
| `NR_PIDS`/`INIT_PID`/`NO_PID` | const.h:3-9 | §1.3 |
| `LAST_FEW`/`NR_PROCS` | forkexit.c:32、config.h:31 | §1.5 |
| `_ENDPOINT_P`/`_ENDPOINT_G`/`_ENDPOINT` | include/minix/endpoint.h | §2.2/§2.6 |

---

## 3. Rust 设计决策

### 3.1 D1：`ProcTable` 聚合表 + 计数 + 轮转指针 + PID 生成器（ARCH A-3）

- **C**：`mproc[NR_PROCS]` + 三个文件级全局——`procs_in_use`（glo.h:9）、
  `do_fork` 内 `static next_child`（forkexit.c:51）、`get_free_pid` 内
  `static next_pid`（utility.c:36）；
- **Rust**：

```rust
pub struct ProcTable {
    pub procs: [Process; NR_PROCS],
    pub procs_in_use: Cell<usize>,
    pub next_child: Cell<usize>,
    pub pid_generator: PidGenerator,
}
```

- **理由**：三个分散全局聚合为单一结构，是 ARCH A-3（全局变量 →
  `PmContext`/`ProcTable`）在"表"这一侧的落地；"计数必须与数组一致"的
  不变量由类型封装，消除"哪个调用点忘记增减 `procs_in_use`"的整类错误；
- **行为契约**：`new()` = 全空槽 + 计数 0 + 轮转 0 + `next_pid = INIT_PID+1`；
  `count()`/`is_full()` 读计数；`iter_active()` 只产生活进程。

### 3.2 D2：`pm_isokendpt → Result<UserSlot, EndpointError>`（errno 精确映射）

- **C**：出参 `int *proc` + 返回 OK/EINVAL/EDEADEPT；
- **Rust**：

```rust
pub fn pm_isokendpt(&self, endpoint: Endpoint) -> Result<UserSlot, EndpointError>
pub enum EndpointError { InvalidSlot, DeadEndpoint }
impl EndpointError {
    pub const fn to_errno(self) -> Errno { /* InvalidSlot→EINVAL, DeadEndpoint→EDEADEPT */ }
}
```

- **理由**：C 的错误码是 PM 对调用者的契约（handler 原样返回），Rust 用
  `Result` 精确表达；区分 `InvalidSlot`（编码非法）与 `DeadEndpoint`
  （陈旧引用）对调试有价值；与 VM 的 `vm_isokendpt`/`EndpointError`
  （`vmproc/table.rs:284`）同型，跨服务一致；
- **行为契约**：三层检查顺序与 C 完全一致（范围 → 代数 → 存活）；
  `EndpointError::to_errno()` 映射 `Errno::EINVAL`（22）/`Errno::EDEADEPT`（215）。

### 3.3 D3：`release_slot` 不 bump generation——修正现有实现

**这是本档最重要的语义修正。**

- **C**：cleanup（forkexit.c:795-806）只清 pid/flags/child 时间 + 计数减一，
  **不 bump generation**；新 endpoint 由内核 `do_fork` 生成（§2.6）；
- **Rust 现状（修正前）**：`release_slot` 调 `increment_endpoint_generation`
  把代数 +1——把 generation 所有权错误地放在 PM。若 PM 与内核都 bump，
  会出现代数偏移：内核下次复用槽位时从自己的陈旧值 +1，与 PM 表内值
  不一致，`pm_isokendpt` 的代数检查会拒绝**合法的新进程**；
- **修正**：`release_slot` 将槽位重置为 `Process::default()`（endpoint →
  `Endpoint::NONE`）+ 计数减一；删除 `increment_endpoint_generation`/
  `calculate_endpoint`/`endpoint_to_index`/`endpoint_to_generation`/
  `validate_endpoint`（endpoint 编解码属 `minix-types` 的 `Endpoint` API，
  不属 PM 表逻辑）；
- **errno 契约不变论证**：

| 场景 | C 行为 | Rust 行为 | errno |
|------|--------|----------|-------|
| 陈旧引用（释放后，槽未复用） | 代数匹配（陈旧 endpoint 仍在槽中）→ `!IN_USE` 失败 | endpoint = NONE ≠ 陈旧 → 代数失败 | EDEADEPT（两边相同） |
| 陈旧引用（槽已复用） | 代数不匹配（内核已 bump） | 代数不匹配 | EDEADEPT（两边相同） |
| ANY/NONE/SELF 或越界 | 槽位越界 | 槽位越界 | EINVAL（两边相同） |

  内部表达不同（C 留陈旧 endpoint、Rust 清为 NONE），但**外部 errno 契约
  完全一致**；且 Rust 版本对代数环绕（`_ENDPOINT_MAX_GENERATION` 回 1）免疫
  ——释放槽永远是 NONE，不可能与任何代数的陈旧引用匹配；
- **三处一致**：table.rs 注释 + design.v1 §D3 + 本档 §3.3（标注"修正现有
  实现"）。

### 3.4 D4：`PidGenerator` 单一事实源（表即状态，无位图）

- **C**：`static next_pid` + do-while 全表冲突扫描；
- **Rust**：

```rust
pub struct PidGenerator { next_pid: Cell<Pid> }
pub fn get_free_pid(&self, table: &ProcTable) -> Pid {
    loop {
        let candidate = self.next_pid.get();
        let next = if candidate < NR_PIDS { candidate + 1 } else { INIT_PID + 1 };
        self.next_pid.set(next);
        if !self.any_conflict(candidate, table) { return candidate; }
    }
}
fn any_conflict(&self, candidate: Pid, table: &ProcTable) -> bool {
    table.iter_active().any(|p| p.pid() == candidate || p.procgrp() == candidate)
}
```

- **理由**：无位图 → 不存在"位图说空、表说占用"的同步问题（单一事实源）；
  `NR_PIDS >> NR_PROCS`（30000 vs 256）保证期望 O(1)；
- **ARCH 行为差异（显式声明）**：C 扫描**全部**槽位，释放槽的陈旧
  `mp_procgrp` 会造成额外跳过；Rust 只扫活进程（`iter_active`）。对活进程
  的 PID 唯一性契约两边一致；具体选中哪个 PID 在环绕边界可能有差异，不构成
  外部语义变化（PID 分配无确定性契约，POSIX 只要求对活进程唯一）；
- **行为契约**：首分配 = 2；环绕 = `NR_PIDS → INIT_PID+1`；冲突 = 任一活
  进程的 pid 或 procgrp。

### 3.5 D5：`find_proc → Option<UserSlot>`（表保持索引权威）

- **C**：返回 `struct mproc *` 或 NULL；调用方（trace/misc）拿到指针后
  读写字段；
- **Rust**：`find_proc(&self, pid: Pid) -> Option<UserSlot>`——返回**槽号**
  而非 `&Process`；
- **理由**：调用方（如 do_trace）需要可变访问，返回槽号让调用方按需
  `get()/get_mut()`；与 `pm_isokendpt` 返回 `UserSlot` 对称——表保持
  "索引权威"，不泄漏内部引用；避免 `&self` 返回引用与后续 `&mut` 请求的
  借用冲突；
- **行为契约**：扫描活进程（`is_in_use`）中 pid 匹配的第一个；无匹配 →
  `None`（调用方映射 ESRCH）。

### 3.6 D6：`PmContext::is_root` 用 effective uid（代码修正）

- **C**：LAST_FEW 容量检查用 `rmp->mp_effuid != 0`（forkexit.c:61）；
- **Rust 现状（修正前）**：`PmContext::is_root` 检查 `creds.user.real == 0`
  （context.rs）——real 与 effective 混淆：setuid 程序（real uid 非 0、
  effective uid 为 0）在 C 中能用保留区，Rust 现状会错误拒绝；
- **修正**：

```rust
pub fn is_root(&self) -> bool {
    match &self.current_proc().resources.privilege {
        Privilege::User(creds) => creds.is_superuser(),  // effective == 0
        Privilege::Kernel => true,                        // 系统进程特权
    }
}
```

- **行为契约**：`can_alloc()` 在计数 ≥ `NR_PROCS - LAST_FEW` 时仅 root
  （effuid == 0 或系统进程）可通过。

### 3.7 ARCH 标注汇总

| ARCH 项 | Minix3 现状 | minix-rs 演进 | 本档位置 |
|---------|------------|--------------|---------|
| A-3 | 文件级全局（procs_in_use/next_child/next_pid/mp） | `ProcTable`/`PmContext` 聚合 | §3.1/D1 |
| A-11 | `pid_t`/`endpoint_t` 平台相关 | `Pid`/`Endpoint`/`UserSlot` 类型化 | 全文 |
| 新增（行为差异） | C cleanup 留陈旧 endpoint | Rust 重置为 NONE（errno 契约不变） | §3.3/D3 |
| 新增（行为差异） | C 扫全表（含陈旧 procgrp） | Rust 只扫活进程 | §3.4/D4 |
| 新增（代码修正） | C 用 mp_effuid | Rust `is_superuser()`（修正 real 误用） | §3.6/D6 |

### 3.8 对照：Redox / Linux 的进程表与 PID 分配

| 维度 | Minix3 | Linux | Redox |
|------|--------|-------|-------|
| 表归属 | 四表分布式（PM/VM/VFS/内核） | 单 `task_struct` 树 | 内核 `Context` 集中 |
| 进程身份 | slot + generation + pid 三层 | `pid`/`pidfd` 文件句柄 | pid 单一 |
| 陈旧引用防护 | endpoint generation（代数不匹配 → EDEADEPT） | RCU + 引用计数（`get_pid_task` 后指针存活） | 内核单地址空间 + 所有权 |
| PID 分配 | 单调递增 + 冲突扫描（O(1) 期望） | 位图 + 环形（`alloc_pid`，常数时间 + 每 PID 一位空间） | 递增分配 |
| 保留区 | LAST_FEW 给 root | `pid_max` 上限 + `pid_max` 可调 | 无 |

**最佳实践结论**：

1. **generation 是微内核跨服务消息的轻量防伪**——Monolithic 内核不需要
   它，因为引用不跨信任边界。Minix3 把代数编码进 endpoint，让消息接收方
   零成本验证；Rust 侧保留此机制（`Endpoint` 类型），但**所有权必须留在
   生成侧（内核/VM）**——这是 §3.3 修正的根因。
2. **PID 分配的"位图 vs 递增扫描"权衡**：位图（Linux）常数时间但每 PID
   一位空间（30000 PID ≈ 3.75 KB）；递增扫描（Minix3）零额外空间、期望
   常数时间。对 PM 这种 256 槽的小表，扫描的成本可忽略——Rust 保持
   C 的算法（D4），不引入位图。
3. **"表即状态"优于"分配器独立状态"**：Linux 的 pid 位图需要与
   `task_struct` 同步（`alloc_pid`/`free_pid` 成对）；Minix3/Rust 让表本身
   成为唯一事实源（冲突检测直接读表），消除同步类错误——Redox 同样把
   分配状态放在内核表内。

---

## 4. 实现详解

### 4.1 模块结构

| 文件 | 内容 | 对应 C |
|------|------|--------|
| `mproc/table.rs` | `ProcTable` + `pm_isokendpt` + `find_proc` + `alloc/release` + `EndpointError` | glo.h、utility.c:34-121、forkexit.c:60-74/795-806 |
| `mproc/pid_gen.rs` | `PidGenerator`（单调递增 + 冲突扫描） | utility.c:34-52 |
| `mproc/constants.rs` | `NR_PIDS`/`INIT_PID`/`NO_PID`/`NO_TRACER_INDEX` | const.h |
| `mproc/context.rs` | `PmContext`（当前进程访问 + `is_root`） | glo.h:8（mp 宏） |

### 4.2 ProcTable 布局与初始化

```rust
pub struct ProcTable {
    pub procs: [Process; NR_PROCS],      // 256 槽，约 120 KB（size_of 实测 480 B/槽）
    pub procs_in_use: Cell<usize>,       // 活进程计数
    pub next_child: Cell<usize>,         // 轮转分配指针
    pub pid_generator: PidGenerator,     // next_pid
}
```

- `[Process; NR_PROCS]` 静态数组保证槽位地址稳定（与 C 的
  `mproc[NR_PROCS]` 同构）；
- `Cell<usize>` 是单线程事件循环下的合法内部可变性（PM 用户态服务器，
  与执行模型约束一致）；未来多线程需改 `AtomicUsize`；
- `new()` 对应 main.c:146-152 的"表初始化 + 全零"（01 已展开）。

### 4.3 `pm_isokendpt` 实现（table.rs）

```rust
pub fn pm_isokendpt(&self, endpoint: Endpoint) -> Result<UserSlot, EndpointError> {
    let slot = endpoint.slot();
    if slot < 0 || slot as usize >= NR_PROCS {
        return Err(EndpointError::InvalidSlot);   // 层 1：范围
    }
    let idx = slot as usize;
    if self.procs[idx].endpoint() != endpoint {
        return Err(EndpointError::DeadEndpoint);  // 层 2：代数
    }
    if !self.procs[idx].is_in_use() {
        return Err(EndpointError::DeadEndpoint);  // 层 3：存活
    }
    Ok(UserSlot::new(idx))
}
```

- 槽位提取复用 `minix-types` 的 `Endpoint::slot()`（对应 `_ENDPOINT_P`）；
- 特殊 endpoint（ANY/NONE/SELF）的 slot 值都在 `[31742, 31744]`，远超
  `NR_PROCS` → 层 1 EINVAL，与 C 一致；
- 负槽位（内核 task）→ 层 1 EINVAL（内核 task 不在 mproc 表，C 同）。

### 4.4 `find_proc` 实现（table.rs）

```rust
pub fn find_proc(&self, pid: Pid) -> Option<UserSlot> {
    self.procs
        .iter()
        .enumerate()
        .find(|(_, p)| p.is_in_use() && p.pid() == pid)
        .map(|(idx, _)| UserSlot::new(idx))
}
```

`is_in_use()` 对应 C 的 `mp_flags & IN_USE`（02 §4.3 的不变量：释放槽
`Lifecycle::Unused`）。释放槽的陈旧 pid 不参与匹配（§2.3）。

### 4.5 alloc/release 生命周期

```rust
pub fn find_free_slot(&self) -> Option<usize> {
    for _ in 0..NR_PROCS {
        let next = (self.next_child.get() + 1) % NR_PROCS;  // 先递增，同 C
        self.next_child.set(next);
        if !self.procs[next].is_in_use() {
            return Some(next);
        }
    }
    None
}

pub fn alloc_slot(&self) -> Option<usize> {
    let slot = self.find_free_slot()?;
    self.procs_in_use.set(self.procs_in_use.get() + 1);
    Some(slot)
}

pub fn release_slot(&mut self, index: usize) {
    if index >= NR_PROCS || self.procs_in_use.get() == 0 {
        return;
    }
    self.procs[index] = Process::default();   // 清身份/状态/资源（endpoint → NONE）
    self.procs_in_use.set(self.procs_in_use.get() - 1);
}
```

| C 行为 | Rust 行为 |
|--------|----------|
| 容量检查（forkexit.c:60-62） | `can_alloc_for_user(is_root)`（调用方先查） |
| 轮转扫描（forkexit.c:68-74） | `find_free_slot`（先递增再检查，`next_child` 保持"最后检查的槽位"） |
| `procs_in_use++`（forkexit.c:86） | `alloc_slot` 内计数加一 |
| cleanup 清 pid/flags/child 时间（forkexit.c:801-804） | `release_slot` 重置整槽为 default |
| **不 bump generation**（forkexit.c 无此操作） | **不 bump**（§3.3 修正；新 endpoint 来自内核/VM） |

> **DEFERRED（07-pm-fork）**：`mproc/fork.rs` 的 `do_fork_prepare` 目前把
> `child_endpoint` 预填为释放槽的（默认 NONE）值——真实 endpoint 必须来自
> VM `vm_fork` 回复（forkexit.c:78-111）。03 只保证"释放槽不提供 endpoint"
> 的语义；07 落地 fork 全流程时修正该预填。

### 4.6 PidGenerator 实现与复杂度（pid_gen.rs）

```rust
pub fn get_free_pid(&self, table: &ProcTable) -> Pid {
    loop {
        let candidate = self.next_pid.get();
        let next = if candidate < NR_PIDS { candidate + 1 } else { INIT_PID + 1 };
        self.next_pid.set(next);
        if !self.any_conflict(candidate, table) {
            return candidate;
        }
    }
}
```

- **期望 O(1)**：冲突概率 ≈ 0.85%（§2.4），循环几乎一次退出；
- **最坏 O(N)**：表满且冲突密集时扫描整个表；与 C 相同；
- `Cell<Pid>` 的单线程安全性：PM 单线程事件循环；注释已声明未来多线程
  需改 `AtomicI32`；
- 唯一性证明依赖 `iter_active()` 的活进程过滤 + 表内 `mp_pid` 写路径
  （fork/init）都经过 `get_free_pid`——启动路径（main.c:209）与 fork 路径
  （forkexit.c:119）共用同一生成器，保证全局唯一。

### 4.7 不变量清单

| # | 不变量 | 依据 | 维护者 |
|---|--------|------|--------|
| 1 | `procs_in_use == count(is_in_use())` | glo.h:9 语义 | `alloc_slot`/`release_slot`/init |
| 2 | 活进程 pid 全局唯一（含 procgrp 冲突） | utility.c:45 | `PidGenerator` |
| 3 | 释放槽 endpoint == `Endpoint::NONE` | §3.3（D3） | `release_slot` |
| 4 | 释放槽不参与 pid/procgrp 冲突与 find_proc 匹配 | §3.4/§3.5 | `iter_active` |
| 5 | 槽位 = 四表共享坐标（endpoint 的 slot 部分） | endpoint.h | 内核/VM/PM 协议 |
| 6 | PM 不生成 endpoint（只存储 boot image/VM 回传值） | §2.6 | fork/init 路径（07/01） |

---

## 5. 测试要点

### 5.1 本模块测试（`os/servers/pm/src/mproc/`）

| 测试 | 验证点 | 对应 C |
|------|--------|--------|
| `table::tests::test_proc_table_new` | 空表：计数 0、槽 0 Unused | glo.h:9 |
| `table::tests::test_find_free_slot_round_robin` | 轮转从槽 1 开始（先递增）、跳过占用槽 | forkexit.c:68-74 |
| `table::tests::test_find_free_slot_full` / `test_alloc_slot_full_returns_none` | 满表 None（C panic 的不可达路径） | forkexit.c:68-74 |
| `table::tests::test_alloc_slot` | 计数 +1 | forkexit.c:86 |
| `table::tests::test_release_slot` | 重置为 default、endpoint=NONE、**不 bump generation** | forkexit.c:795-806 |
| `table::tests::test_release_slot_keeps_count` | 空表释放不产生负计数 | forkexit.c:805 |
| `table::tests::test_can_alloc_for_user` | 保留区 root 语义（LAST_FEW） | forkexit.c:60-62 |
| `table::tests::test_pm_isokendpt_valid` / `_slot_out_of_range` / `_generation_mismatch` / `_not_in_use` / `_released_slot`（5 个） | 三层检查 + 三类 errno 场景 | utility.c:108-121 |
| `table::tests::test_find_proc` / `test_find_proc_skips_released_slot` | 活进程 pid 匹配、释放槽不匹配 | utility.c:76-85 |
| `table::tests::test_endpoint_error_to_errno` | EINVAL/EDEADEPT 值对齐 | sys/errno.h:64/211 |
| `pid_gen::tests`（8 个） | 首分配 2、唯一性、环绕、pid/procgrp 冲突、范围、释放槽陈旧 procgrp 不冲突 | utility.c:34-52 |
| `context::tests::test_is_root_effective_uid` + `mproc::fork::tests::test_fork_reserved_for_root` | is_root（effuid，含 setuid 提权场景）+ 保留区拒绝非 root | forkexit.c:61 |

### 5.2 基线

- `cargo test -p minix-pm --lib`：**101 passed / 0 failed**（2026-08-17 实测；
  上一基线 91：table.rs 净增 8（新增 11：pm_isokendpt 5 + find_proc 2 +
  轮转/满表/计数边界 4；替换删除 generation-bump 测试 3）、pid_gen.rs +1、
  context.rs +1（is_root effuid，R1 补））。
- `cargo build -p minix-pm`：通过（本档改动的 table.rs/pid_gen.rs/context.rs
  无新增警告；workspace 其余 crate 的存量警告与 qemu-tests 编译错误均为
  历史问题，与 PM 无关）。
- 定向模块：`mproc::table` 16、`mproc::pid_gen` 8、`mproc::context` 3。

---

## 6. 过渡

本档回答了"PM 如何索引活着的进程"：`ProcTable` 聚合了表、计数、轮转指针
与 PID 生成器；`pm_isokendpt`/`find_proc` 提供外部身份 → 槽位的翻译；
`alloc_slot`/`release_slot` 维护容量纪律；并修正了两处 C 语义偏离
（generation 所有权、effuid）。

在启动时序中，01 完成了 boot image 填充（写入了 INIT/系统进程的 endpoint
与 PID）；本档定义这些写入所依赖的**表基础设施**；下一步：

- **04-ipc-dispatch.md** 承接：主循环用 `pm_isokendpt(who_e)` 验证每个
  调用者（main.c:75）——本档的层 1/2/3 检查是分发前的第一道闸门；
- **07-pm-fork.md** 承接：`can_alloc_for_user` + `find_free_slot` +
  `get_free_pid` 构成 fork 的槽位/PID 分配链（并修正 `do_fork_prepare` 的
  endpoint 预填，§4.5 DEFERRED）；
- **09-pm-exit.md / 10-pm-wait.md** 承接：`release_slot` 是 cleanup 的落点；
- **18-trace.md / 20-misc-queries.md** 承接：`find_proc` 的 ptrace/查询消费。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/plan.md` §2/§3.4/§4（A-3/A-11）/§5.3/§7.3
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/01-pm-init-main.md` §2.4 第 5 步（boot image 填充）与 §3.8（Redox/Linux 对照风格）
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/02-mproc-struct.md` §4.2（Process 四层映射）与 §4.3（不变量）
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/04-ipc-dispatch.md`（主循环 pm_isokendpt 消费，后续）
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/07-pm-fork.md`（槽位/PID 分配链，后续）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/03-vmproc-table.md` §3.1（VmProcTable 同型对照）
- `minix3/minix/servers/pm/{glo.h, utility.c, const.h, forkexit.c}`（ground truth）
- `minix3/minix/kernel/system/do_fork.c:69-72`（generation 归属）
- `minix3/minix/include/minix/endpoint.h`（endpoint 格式与 generation 语义）
- `os/libs/minix-types/src/types/endpoint.rs`（`Endpoint`/`UserSlot` API）
- `os/servers/vm/src/vmproc/table.rs:284`（`vm_isokendpt` 同型实现）
