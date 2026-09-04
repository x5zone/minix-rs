# 01-devm-init-main：启动入口与主循环锚点

> **定位**：`main → run_vtreefs → sef_local_startup → fsdriver_task → VFS mount → init_hook`。本篇是整个 11-stage-devman 的时序锚点：回答"devman 进程如何活起来，并把自己挂到 VFS 上"。
> **源码**：`minix3/minix/servers/devman/main.c`（93 行，全部）+ `minix3/minix/lib/libvtreefs/vtreefs.c:sef_local_startup,run_vtreefs` + `mount.c:fs_mount` + `table.c` + `minix3/etc/system.conf:422-429`。
> **Rust 模块**：`os/servers/devman/src/hooks.rs` + `os/servers/devman/src/main.rs`。
> **前置依赖**：00 总览 + kernel 文档（RS 加载组概念）。
> **不覆盖（移交）**：VTreeFS 内部机制（02）、消息字段与分发细节（05）、各 handler 实现（07~09）、数据结构全字段（03）、树操作（04）、事件与缓冲（06）。

---

## 1. 概念：为什么设备管理需要一个"文件系统外形的服务器"

### 1.1 操作系统的老问题：设备从哪里被发现

每个操作系统都要回答同一个问题：用户程序怎么知道机器上有什么设备？

Linux 的答案是 sysfs：一组伪文件。你 `ls /sys/bus/usb/devices`，看到的就是设备；你 `cat` 某个属性文件，读到的就是设备描述。设备出现、消失，对应文件出现、消失；驱动绑定，对应某个属性文件的写操作。**设备管理问题被翻译成了文件问题**，而文件是所有 Unix 程序都会操作的东西。

Redox 的答案是 scheme：一切设备都是 URL。你打开 `usb:0/description`，背后是 USB 驱动在响应。思想相同：用统一的名字空间机制承载设备信息，消费者不需要学习第二套 API。

Minix3 的 devman 走了同一条路：它是一个**看起来像文件系统的服务器**。devmand 守护进程轮询的不是某个私有 IPC 通道，而是 `/sys/events` 这个文件；判断设备类型时读的不是某个结构体字段，而是 `<设备路径>/dev_type` 这个文件内容（13 会详述）。理解 devman 的第一步，是接受这个设定：**设备树就是文件树，设备事件就是文件内容**。

### 1.2 Minix3 的具体手段：VTreeFS 框架

"看起来像文件系统"在 Minix3 里有现成的实现，叫 VTreeFS（Virtual Tree File System，`minix3/minix/lib/libvtreefs/`，1642 行）。它是一个库，不是服务器：任何想把自己伪装成文件系统的服务都可以链接它，填三个钩子，然后调用一个函数进入主循环。

devman 的 `main`（main.c:70-91）就是这个模式的教科书实例，全函数 22 行，逻辑只有三步：

1. **填表**：声明一个 `struct fs_hooks`，把三个函数指针填进去；
2. **定根**：声明一个 `struct inode_stat`，描述文件树根节点的模样；
3. **开跑**：调用 `run_vtreefs(...)`，从此不再返回（直到卸载）。

读完本篇，你应该能不看代码说出这三步每一步在干什么、以及 `run_vtreefs` 之后控制权去了哪里。这是后面 12 篇全部内容的时序地基。

### 1.3 三个钩子的直觉：出生、说话、收信

`struct fs_hooks` 有 13 个槽（`minix3/minix/include/minix/vtreefs.h:24-44`，`grep -c "(\*"` 实数 13），devman 只用了 3 个。每个钩子回答一类"我不知道，自己决定"的事件，框架在事件发生时回调：

- `init_hook`（出生）：文件系统被挂载时调用一次。devman 用它创建设备树的根（`devman_init_devices()`，04 详述）。注意"挂载时"而非"启动时"——devman 进程先活起来进主循环，等 VFS 来挂载它时才初始化设备树。这个延迟是 §2.7 的核心。
- `read_hook`（说话）：VFS 替用户进程读某个文件时调用。devman 用它分发到每个文件的 `read_fn`（06 详述事件文件与静态信息文件的两种读法）。
- `message_hook`（收信）：收到**不是文件请求**的消息时调用（VTreeFS 术语叫 `fs_other`）。设备驱动的 ADD/DEL/BIND/UNBIND 请求都走这条通道（05 详述）。

其余 10 个槽（lookup、write、mknod……）devman 保持 NULL，使用 VTreeFS 的默认实现。02 会列出全表并说明每个槽 devman 为什么不需要。

### 1.4 边界声明

本篇讲"进程如何启动、钩子如何注册、主循环如何进入、挂载如何触发初始化"。以下内容本篇只给调用点与一句话，不展开：

- VTreeFS 的 inode 树、fsdriver 全表、read 循环 → 02；
- `devman_init_devices` 建了什么树 → 04；
- `message_hook` 里四个 handler 各自干什么 → 05/07~09；
- 事件格式与缓冲语义 → 06。

---

## 2. C 源码分析

### 2.1 `main`：填表、定根、开跑（main.c:70-91）

```c
int main (int argc, char* argv[])
{
	static struct fs_hooks hooks;
	static struct inode_stat root_stat;

	/* fill in the hooks */
	memset(&hooks, 0, sizeof(hooks));
	hooks.init_hook 	= init_hook;
	hooks.read_hook 	= read_hook;
	hooks.message_hook 	= message_hook;	/* handle the ds_update call */

	root_stat.mode 	= S_IFDIR | S_IRUSR | S_IRGRP | S_IROTH;
	root_stat.uid 	= 0;
	root_stat.gid 	= 0;
	root_stat.size 	= 0;
	root_stat.dev 	= NO_DEV;

	/* run VTreeFS */
	run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE);

	return 0;
}
```

逐行解读：

**`static struct fs_hooks hooks` + `memset` 清零**：`static` 让这张表在进程生命周期内一直有效（主循环全程通过全局指针 `vtreefs_hooks` 访问它，见 §2.5）。`memset` 把 13 个槽全部置 NULL，然后只填 3 个——这是 C 表达"其余用默认"的惯用法。Rust 侧不需要这一步（§3.1）：结构体字面量构造时未提及的字段必须显式写 `None`，类型系统替你做了清零的事。

**三个赋值**：注意注释 `/* handle the ds_update call */` 已经过时——`message_hook` 处理的是 DEVMAN_* 设备消息，不是 DS 更新。这是 C 注释漂移的实例，Rust 重写时不继承这条注释（review 模式 77 的反面教材）。

**`root_stat` 五行**：根节点是目录（`S_IFDIR`），权限 0444（只读：`S_IRUSR|S_IRGRP|S_IROTH`，连属主都不能写——设备树只通过消息通道变更，从不通过文件写通道变更，所以没有 `S_IWUSR`；这与 02 的 write 槽保持 NULL 是同一决策的两面），uid/gid 皆 0（系统所有），size 0（目录大小无意义），`dev = NO_DEV`（虚拟文件系统不 backend 任何块设备）。同样的五行（目录 + 0444 + uid/gid/size/dev）在 `device.c:18` 的 `default_dir_stat` 里又出现一次——C 侧的重复常量，Rust 侧收敛为 `RootStat::devman_root()` 唯一构造（§4.2）。

**`run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE)` 六参数**：

| 参数 | 值 | 含义 |
|---|---|---|
| `hooks` | `&hooks` | 上面填好的钩子表 |
| `nr_inodes` | `1024` | inode 池上限（02 详述分配器） |
| `inode_extra` | `0` | 每个 inode 的额外空间，devman 不用 |
| `istat` | `&root_stat` | 根节点属性 |
| `nr_indexed_entries` | `0` | 索引槽数量，devman 不用（只用名字树） |
| `bufsize` | `BUF_SIZE`（4097，devman.h:39） | I/O 缓冲大小 |

两个 `0` 值得注意：它们不是"随便填的"，而是"devman 不用这两项功能"的声明。02 会证明 devman 确实从不调用 indexed 槽 API。`BUF_SIZE 4097`（= 4096 + 1，多出的一字节是字符串终止符位，buf.c 详见 06）。

**`return 0` 永远执行不到**：`run_vtreefs` 内是 `fsdriver_task` 无限主循环，只在文件系统被卸载且收到 SIGTERM 时返回（vtreefs.c:108-109 的 `cleanup_buf/cleanup_inodes` 是退出路径）。C 写 `return 0` 只是为了让编译器闭嘴。

### 2.2 `init_hook`：一次性的出生证明（main.c:36-43）

```c
static void init_hook(void) {
	static int first = 1;

	if (first) {
		devman_init_devices();
		first = 0;
	}
}
```

`static int first` 是"只执行一次"守卫。为什么需要守卫？因为 `init_hook` 的调用方是 `fs_mount`（§2.7），而 mount 理论上可以发生多次（卸载后重挂）。devman 的设备树是进程级单例——重挂时树已经在了，重建会泄漏旧树。守卫保证 `devman_init_devices()`（04 详述：创建 `root_dev` + `devices/` + `events/`）只跑一次。

Rust 侧把这个隐式协议显式化为 `FirstGuard`（§3.2/§4.2）：状态机只有两个状态，`enter()` 至多返回一次 true。C 的 `int` 可取 2^32 个值但只用两个，Rust 用 `bool` 新类型收窄。

### 2.3 `read_hook`：一句话的分发器（main.c:60-67）

```c
static ssize_t
read_hook
(struct inode *inode, char *ptr, size_t len, off_t offset, cbdata_t cbdata)
{
	struct devman_inode *d_inode = (struct devman_inode *) cbdata;

	return d_inode->read_fn(ptr, len, offset, d_inode->data);
}
```

VTreeFS 的 inode 除了名字和属性，还挂一块"私房数据"（`cbdata`，`void*`）。devman 在每个 inode 的私房数据里放一个 `struct devman_inode`（03 详述），其中 `read_fn` 是函数指针，指向该文件的具体读法（事件文件 → `devman_event_read`，静态信息文件 → `devman_static_info_read`，06 详述）。`read_hook` 本身不做任何判断，只做一层间接跳转。

设计眼光看：这是**策略模式**的 C 写法——"哪个文件怎么读"这个知识分散在每个 inode 的 `read_fn` 里，而不是集中在 `read_hook` 的 switch 里。新增一种文件类型时不需要改分发器，只需要挂新的 `read_fn`。07（`devman_id` 文件）和 06 会展示这个扩展点被如何使用。Rust 侧保留这个结构（`read_fn` 字段），因为它是合理的设计，不是 C 的局限。

第一个参数 `inode` 在函数体内根本没用——C 保留它是因为钩子签名是框架定的（`vtreefs.h:29-30`）。Rust 侧直接省略该参数（`cbdata` 地址已足够分发到 `read_fn`，多传一个永不读取的指针没有意义）；保留但忽略的参数（如 `MessageHookFn` 的 `_ipc_status`，05 转发可能用）才用下划线前缀标记，这是 Clippy 对未使用参数的要求。

### 2.4 `message_hook`：四 case 无 break（main.c:46-58，现象记录）

```c
static void message_hook(message *m, int __unused ipc_status)
{
	switch (m->m_type) {
		case DEVMAN_ADD_DEV:
			do_add_device(m);
		case DEVMAN_DEL_DEV:
			do_del_device(m);
		case DEVMAN_BIND:
			do_bind_device(m);
		case DEVMAN_UNBIND:
			do_unbind_device(m);
	}
}
```

逐行走读：C switch 的每个 `case` 没有 `break`，意味着控制流**贯穿执行**。收到 `DEVMAN_ADD_DEV` 时，依次执行 `do_add_device` → `do_del_device` → `do_bind_device` → `do_unbind_device` 四个 handler；收到 `DEVMAN_DEL_DEV` 时执行后三个；收到 `DEVMAN_UNBIND` 时只执行最后一个。

这是什么后果？以 ADD 为例：`do_add_device` 刚把设备加入树并回复调用者，`do_del_device` 紧接着就用同一条消息把它删除（`_find_dev` 能找到刚加入的设备），然后两个 bind handler 因来源不是 RS 而回复 EPERM。一条 ADD 消息最终产生四次 handler 执行和多次 reply 写同一条消息——这是可观测的错误行为。

本篇**只记录现象**（switch 无 break、四 handler 贯穿、上面一段的执行序列），**不定性、不修复**。定性（是手误还是有意）与修复决策归属 05（A-3），那里有完整的证据链（libdevman 客户端契约证明"ADD 后设备必须可见"）和三处一致标注。这是 plan §3.4 分工的刻意安排：启动篇不抢协议篇的结论。

`int __unused ipc_status`：GCC 扩展标记"参数故意不用"。`fs_other`（vtreefs.c:66-80）确实传了 `ipc_status` 进来，但 devman 的四个 handler 都不需要它（权限判断用 `m->m_source`，见 05）。Rust 侧对应 `_ipc_status`。

### 2.5 `run_vtreefs`：六个全局变量的中转站（vtreefs.c:88-110）

```c
void
run_vtreefs(struct fs_hooks * hooks, unsigned int nr_inodes,
	size_t inode_extra, struct inode_stat * istat,
	index_t nr_indexed_entries, size_t bufsize)
{
	/*
	 * Use global variables to work around the inability to pass parameters
	 * through SEF to the initialization function..
	 */
	vtreefs_hooks = hooks;
	inodes = nr_inodes;
	extra_size = inode_extra;
	root_stat = istat;
	root_entries = nr_indexed_entries;
	buf_size = bufsize;

	sef_local_startup();

	fsdriver_task(&vtreefs_table);

	cleanup_buf();
	cleanup_inodes();
}
```

注释说得很坦白：SEF 的初始化回调签名是固定的（`init_server(int type, sef_init_info_t *info)`，vtreefs.c:16），`main` 的六个参数穿不过去，只能暂存全局变量，等 `init_server` 被 SEF 回调时再读出来（`init_inodes(inodes, root_stat, ...)`，vtreefs.c:21）。这是框架迁就库接口限制的典型妥协。

调用序列三步：存参数 → `sef_local_startup()`（§2.6，SEF 握手）→ `fsdriver_task(&vtreefs_table)`（进主循环，02 详述分发表）。最后两行 cleanup（`cleanup_buf/cleanup_inodes`，vtreefs.c:108-109）只在主循环退出时执行。

Rust 侧没有 SEF 签名限制（`minix-sef` 是我们自己的 crate，将来可以定任何签名），所以不需要全局中转——`ServerConfig` 显式传参（§3.4）。这是 [ARCH:A-1-相关]（本篇 §3 + `hooks.rs` 注释 + 设计快照三处一致，设计侧一致性见本篇 review 的 scan.md Gate H）。

### 2.6 `sef_local_startup`：三行注册一行启动（vtreefs.c:52-60）

```c
static void
sef_local_startup(void)
{
	sef_setcb_init_fresh(init_server);
	sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL);
	sef_setcb_signal_handler(got_signal);

	sef_startup();
}
```

SEF（System Event Framework）是 Minix3 服务的"出生证"机制：新服务启动时要向 RS 报到（`sef_startup` 内部做 RS_INIT 握手），并声明"如果我崩溃重启，之前的状态还要不要"（`SEF_CB_INIT_RESTART_STATEFUL` = 要，重启时走 restart 路径而非 fresh 路径）与"收到信号怎么办"（`got_signal`：只有 SIGTERM 才退出，vtreefs.c:38-46）。

`init_server`（vtreefs.c:16-33）是 fresh 路径的真正工作：`init_inodes(1024, root_stat, 0)` 建 inode 池 → `init_extra` → `init_buf(4097)`。注意失败处理是 `panic`——VTreeFS 认为这三项分配失败属于"活不下去"，直接 abort。这与 A-7（Rust 用 `Result` 显式传播）的对照点，02 会展开。

### 2.7 `fs_mount → init_hook`：挂载触发初始化（mount.c:10-38）

```c
int
fs_mount(dev_t __unused dev, unsigned int flags,
	struct fsdriver_node * root_node, unsigned int * res_flags)
{
	struct inode *root;

	/* VTreeFS must not be mounted as a root file system. */
	if (flags & REQ_ISROOT)
		return EINVAL;

	/* Get the root inode and increase its reference count. */
	root = get_root_inode();
	ref_inode(root);

	/* The system is now mounted.  Call the initialization hook. */
	if (vtreefs_hooks->init_hook != NULL)
		vtreefs_hooks->init_hook();

	/* Return the root inode's properties. */
	root_node->fn_ino_nr = get_inode_number(root);
	...
	return OK;
}
```

这就是 §1.3 说的"延迟"：devman 进程启动（`main` → 主循环）与设备树初始化（`devman_init_devices`）之间隔着一次 VFS mount。VFS 决定挂载 `/sys` 时发 mount 请求，`fsdriver_task` 按 `vtreefs_table`（table.c:6-24，17 个槽，本篇只关心 `.fdr_mount = fs_mount` 与 `.fdr_other = fs_other`）分发到 `fs_mount`，它才调用 `init_hook`。

`REQ_ISROOT` 拒绝：VTreeFS 不能当根文件系统（根需要真正的块设备后端，VTreeFS 是内存树）。devman 若收到 root mount 请求返回 `EINVAL`——这是本篇触及的第一个 errno，99 会收口全部错误码。

NULL 检查（`if (vtreefs_hooks->init_hook != NULL)`）：框架允许服务不注册 init 钩子。Rust 侧这是 `Option::is_some`，语义相同（§3.1）。

### 2.8 启动来源：RS 加载组（system.conf:422-429）

```
service devman
{
	uid 0;
	vm
		SETCACHEPAGE
		CLEARCACHE
	;
};
```

devman **不在 boot_image**（`minix3/minix/kernel/table.c:44-64` 无 devman 条目，grep 空结果为证）——内核启动时不知道它的存在。RS（Reincarnation Server）读 `system.conf`，以 uid 0 启动 devman 进程，并授予两项 VM 特权（`SETCACHEPAGE`/`CLEARCACHE`，设备内存映射所需，12 详述为什么是这两项）。

这意味着 devman 的"第零步"在 RS：RS 先 `fork+exec` 出 devman 进程，devman 的 `main` 才开始跑。本篇从 `main` 讲起，RS 那半边是 12 的职责（`publish_service` 的 bind 握手也在 12）。

---

## 3. Rust 设计决策

### 3.1 钩子表：13 个 NULL 槽 → 3 个 `Option` 字段

C 的 `struct fs_hooks` 有 13 个函数指针槽，devman 填 3 个、其余靠 `memset` 置 NULL。Rust 的 `FsHooks`（`hooks.rs`）只建模 devman 用的 3 个：

```rust
pub struct FsHooks {
    pub init_hook: Option<fn(&mut InitCtx)>,
    pub read_hook: Option<ReadHookFn>,
    pub message_hook: Option<MessageHookFn>,
}
```

三个取舍：

1. **为什么是 `Option` 而不是 NULL**：`None` 在类型层面就是"未注册"，调用点 `if let Some(f) = hooks.init_hook` 与 mount.c:24 的 NULL 检查语义相同，但编译器强制你处理 `None` 分支——C 忘记检查就野指针，Rust 忘记处理就编译不过。
2. **为什么是 `fn` 而不是 `Fn` trait 对象**：C 函数指针无捕获，`fn` 类型与之对等；单线程事件循环不需要 `Send`。06 若需要带状态的 read 闭包再评估 `Box<dyn Fn>`，本篇不预支复杂度（YAGNI）。
3. **为什么只有 3 个字段而不是 13 个**：plan §3.4 边界——其余 10 个槽是 02 的职责，在 `vtreefs` 框架模块内补全。本篇建 13 字段的结构体会造成两个"全表"定义，违反事实唯一性。

对照 Redox：`redox_scheme::Scheme` trait 对未实现的方法返回 `ENOSYS` 默认实现；此处 `None` 的默认行为是 init 无操作 / read 返回 EOF / message 忽略——"缺失=安全默认值"的思想一致，只是机制不同（trait 默认方法 vs Option 分支）。

对照 VM（`os/servers/vm/src/lib.rs:13-22` 单线程文档）：本 crate 同样假设单线程，`FsHooks` 是 `!Sync` 也没关系——它只活在 devman 主线程里。

### 3.2 `FirstGuard`：`static int` → 显式状态机

C 的 `static int first` 藏在函数体内，测试无法观察、重置无法表达。Rust 的 `FirstGuard(bool)` 把它变成可构造、可测试的值：

```rust
pub struct FirstGuard(bool);
```

单线程所以 `bool` 足够——不需要 `AtomicBool`（那是给 SMP kernel 准备的，devman 是用户态单线程，CLAUDE.md 执行模型节）。VM 的 `global.rs` 对 BSS 单线程状态也是同样论证。

### 3.3 `RootStat`：五行赋值 → 一次构造

```rust
impl RootStat {
    pub const fn devman_root() -> Self;
}
```

`const fn` 保证编译期可求值，不存在"填了一半"的中间态。字段值与 main.c:82-86 逐字段锁定（单测 `root_stat_matches_c_main`，含 `dev = 0` 字面量断言）。常量来源标注：`S_IFDIR = 0o040000`（POSIX）、`NO_DEV = 0`（Minix `const.h:132 #define NO_DEV ((dev_t) 0)`；注意不是 -1——驱动层的 `NO_DEVICE -1` 是另一个宏，不可混淆）。99 收口进 `minix-types` 时本篇的本地定义是上游，路径写在注释里避免将来双定义（模式 76 的预防）。

### 3.4 `ServerConfig`：全局中转 → 显式传参 [ARCH:A-1-相关]

C 不得不用六个全局变量（vtreefs.c:93-96 注释原文为证），因为 SEF 回调签名穿不过参数。Rust 的 `minix-sef` 是我们自己的 crate，没有这个限制，所以：

```rust
pub struct ServerConfig {
    pub nr_inodes: u32,   // 1024（main.c:89）
    pub root_stat: RootStat,
    pub buf_size: usize,  // BUF_SIZE 4097（devman.h:39）
}
```

`inode_extra=0` / `nr_indexed_entries=0` 在类型层面省略——devman 从不使用这两项功能（02 会给出 grep 证据），省略不是丢弃语义，而是把"不用"从"传零"升级为"不可表达"。差异说明表见 §4.3。

三处一致：本节 + `hooks.rs` 的 `// [ARCH:A-1-相关]` 注释 + 设计快照（一致性见本篇 review 的 scan.md Gate H）。

### 3.5 SEF 生命周期：三回调 → 枚举 + trait

C 的三行 `sef_setcb_*` 注册在 Rust 侧表达为启动序列契约（`SefLifecycle` 枚举）与 `SefHooks` trait（`init_server` 返回 `Result<(), Errno>` 而非 panic，[ARCH:A-7] 的本篇实例：分配失败显式传播，调用方决定 fail-fast）。trait 合规要求 ≥2 个行为不同的实现（Gate D-2）：`hooks.rs` 的测试模块以两个 double 落地——`OkSef`（记录 Fresh→Signal 序列，返回 Ok）与 `FailingSef`（`init_server` 直接 `Err(ENOMEM)`），分别锁定正常序列与 A-7 错误上抛。

`minix-sef` 当前是 5 行 stub——本篇**只定义 devman 侧需要的最小形状**，不虚构 `minix-sef` 的 API（Step 1.0g forward reference 合规：不存在的文件/API 必须显式标注"待落地"，不得假装存在）。

### 3.6 message 分发：本篇只做"忽略"桩

`message_hook` 的完整分发是 05 的职责。本篇的 fail-closed 默认就是 `devman_message_hook` 的空实现（忽略并返回）加上 `fire_message` 的 `None` 分支（忽略），注释指向 05/A-3。这不是最终行为，是防止 01 的桩与 05 的实现冲突的占位——05 落地时替换 `devman_message_hook` 的函数体为单 handler 分派（`fire_message` 签名不变）。注意不另设第二忽略入口：曾经的 `dispatch_message_default()` 因与空实现语义重复且 0 调用已删除，避免死代码。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/
  lib.rs    — #![no_std] + 模块声明 + 单线程模型文档（仿 vm/src/lib.rs:1-29）
  hooks.rs  — FsHooks / RootStat / ServerConfig / FirstGuard / SefLifecycle / SefHooks（本篇全部）
  main.rs   — 二进制入口：组装默认 hooks + root_stat + config，调用 run 桩
```

`main.rs` 的组装顺序即 C `main` 的三步（填表→定根→开跑），代码注释逐行标注 C 行号（`// C: main.c:78 ...`），行号漂移时按模式 77 修注释。主循环本体归 02（`vtreefs::run`）：在 02 落地前 `main.rs` 以 park 循环保持进程存活（RS 要求 live endpoint，直接退出会触发重启风暴；fail-safe 缺省）。该 park 是 02-owned 前向缺口（P1 跟踪），不是本篇核心算法——本篇核心（hooks/config/guard）已全部实现。

### 4.2 关键不变量

1. `run` 之前 hooks 已全部注册（`main` 的构造顺序保证，无"先跑后填"的中间态）。
2. `RootStat::devman_root()` 与 main.c:82-86 逐字段相等（单测锁定，改 C 值必改单测）。
3. `FirstGuard::enter()` 至多返回一次 `true`（单测循环 3 次断言）。
4. `ServerConfig::devman_default()` 的 `nr_inodes = 1024`、`buf_size = 4097` 与 main.c:89、devman.h:39 相等（单测锁定）。

### 4.3 与 C 步骤的差异说明

| C 步骤 | Rust 对应 | 分类 |
|---|---|---|
| `memset` 清零 13 槽 | 结构体字面量 + `None` | 设计决策（类型系统消除整块清零） |
| 六全局变量中转参数 | `ServerConfig` 显式传参 | 架构演进 A-1-相关 |
| `static int first` | `FirstGuard` 显式状态 | 设计决策（可测试性） |
| `panic("init_inodes failed")` | `Result<(), Errno>` 传播 | 架构演进 A-7（显式错误） |
| message 四 handler 贯穿 | 本篇桩"忽略"，05 实现单分派 | 已知缺口→05（注释指向 A-3） |
| 10 个未用 hooks 槽 NULL | 本篇不建模，02 补全 | 分工（非缺口） |

---

## 5. 测试要点

`hooks.rs` 内 `#[cfg(test)]` 模块（`cargo test -p minix-devman`）：

| 测试 | 断言 | 对应不变量 |
|---|---|---|
| `first_guard_fires_once` | 3 次 `enter()` 只有首次 true | §4.2-3 |
| `root_stat_matches_c_main` | mode/uid/gid/size/dev 五字段等于 C 值（`dev` 字面量 `0`，`const.h:132`，非自引用常量） | §4.2-2 |
| `server_config_defaults_match_c` | nr_inodes=1024，buf_size=4097 | §4.2-4 |
| `hooks_none_is_safe_default` | 全 `None` 时 init/read/message 默认行为安全（无操作/EOF/忽略） | §3.1 默认语义 |
| `init_hook_wires_to_first_guard` | 注册的 init_hook 经 `FirstGuard` 只触发一次底层 init | §2.2 守卫语义 |
| `sef_ok_records_lifecycle_sequence` | `OkSef` 记录 Fresh→Signal 序列，非 TERM 信号忽略（对偶 C `got_signal`） | §3.5 序列契约 |
| `sef_failing_returns_enomem` | `FailingSef::init_server` 返回 `Err(ENOMEM)` 且不 panic | §3.5 [ARCH:A-7] |

截至 2026-09-04：`cargo test -p minix-devman` **7 passed / 0 failed**（本篇子集；完整统计每篇末段累积更新，review-doc-skill §2.4j）。`cargo clippy -p minix-devman --all-targets` 0 警告（`FirstGuard: Default` 已补，workspace profile 提示除外）。

---

## 6. 过渡

进程已经活起来了：hooks 注册完毕，`run_vtreefs` 的参数备好，下一步是进入主循环。但"主循环里有什么"——`fsdriver_task` 如何按 `vtreefs_table` 分发 17 种请求、inode 树如何分配与查找、`fs_read` 如何循环调用 `read_hook` 把文件内容送出去——是 02（VTreeFS 框架契约）的内容。01 给了调用点，02 给被调用方的全部语义。

设备生命周期的第一步（`devman_init_devices` 建树）在 04，但它的**触发时刻**（mount 时、且仅首次）已经在本篇 §2.2/§2.7 钉死。读 04 时若忘记"谁、何时调用了它"，回看本篇 §2.7 的 mount 序列图。

---

## 7. 参见

- `02-vtreefs-framework.md` — `run_vtreefs` 之后的世界（fsdriver 全表、inode 树、read 循环、A-1 决策正文）
- `04-device-tree.md` — `devman_init_devices()` 建了什么（本篇 §2.2 的调用目标）
- `05-devm-message-contract.md` — `message_hook` fall-through 定性与 A-3 修复（本篇 §2.4 的现象 → 那里的判定）
- `06-event-buf.md` — `read_fn` 的两种实现（本篇 §2.3 的分发目标）
- `12-rs-integration.md` — RS 如何按 `system.conf` 启动本进程（本篇 §2.8 的前半段）
- `99-devm-global-concepts.md` — `BUF_SIZE`、`NO_DEV`、errno 收口
- C 源：`minix3/minix/servers/devman/main.c`、`minix3/minix/lib/libvtreefs/vtreefs.c:52-60,88-110`、`mount.c:10-38`、`table.c:6-24`、`minix3/etc/system.conf:422-429`
