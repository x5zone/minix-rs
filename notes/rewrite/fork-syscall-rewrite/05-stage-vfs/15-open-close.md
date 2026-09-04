# 15 — open/close/lseek：路径到 fd 的绑定、创建、分派、定位与拆除

本文讲清打开调用如何在“访问意图的两比特编码→创建与打开的对偶入口→解析或创建的二选→权限与类型的六路分派→偏移定位→拆除序”的六段管线中，以 `oflags` 的意图位、`O_CREAT` 的有无、`v_mode & S_IFMT` 的类型位、`SEEK_*` 的原点位为四组开关，把一条路径名变成一个可读写的 fd，并在关闭时按“先摘索引、再递减计数、末引用才释放”的拆除序归还一切。

前置阅读：`13-path-lookup.md`（`eat_path/advance/last_dir` 的解析机）、`04-filp-table.md`（`filp_count` 的共享计数与空闲哨兵）、`14-filedes.md`（`get_fd` 的双表预留与 `Fd` 类型）。

> 本章不讲什么：
> - 路径解析的穿越细节（挂载点穿越、symlink 循环上限）—— `13-path-lookup.md`
> - fd 表条目管理（`get_fd` 的 `fp_filp` 侧扫描、`EMFILE/ENFILE` 分化）—— `14-filedes.md`
> - 字符/块/socket 驱动的打开实现（`cdev_open/bdev_open/sdev_*`）—— `19-device-map.md` / `20-bdev.md` / `21-cdev.md` / `22-sdev.md`
> - 权限判定（`forbidden` 的 uid/gid 比对）—— `29-protect.md`
> - 管道的阻塞语义全貌（`suspend/revive` 状态机）—— `17-pipe.md`
> - 记录锁的释放（`lock_revive`）—— `30-fcntl-lock.md`

---

## 1 概念

### 1.1 为什么名字与偏移必须分离

路径名回答“文件是谁”，fd 回答“这次打开怎么用”。同一个 `/etc/passwd` 可以被只读打开三次、读写打开一次——四次打开共享同一个 vnode，却各有自己的读写位置。如果解析（13）直接返回 fd，那么“共享位置还是独享位置”就无处安放：`fork` 要求父子共享偏移（`filp` 共享），而两次独立 `open` 要求偏移互不干扰（各占一个 `filp`）。

于是 VFS 把绑定拆成两跳：解析只给 vnode（“是谁”），打开再配 `filp`（“怎么用”）。`common_open` 的 `fp->fp_filp[fd] = filp; filp->filp_count = 1; filp->filp_vno = vp`（`open.c:133-135`）正是第二跳的落定——fd 索引私有、`filp` 计数共享、偏移独享，三条不变量各归其位。

### 1.2 创建与打开的对偶入口

`open` 与 `creat` 是同一管线的两个入口，区别只在 `O_CREAT` 的有无：`do_open` 见 `O_CREAT` 即 `EINVAL`（`open.c:46`），`do_creat` 不见 `O_CREAT` 即 `EINVAL`（`open.c:71`）。这种“互斥守门”看起来多余——为什么不直接让 `common_open` 按标志位办事？

原因在于调用约定的不对称：`open` 从 `copy_path` 取名（内核已拷贝好的路径），`creat` 从 `fetch_name` 取名（用户地址 + 长度，需自行拷贝）。入口分化的是“名字怎么来”，合流的是“名字来了怎么办”。`common_open(path, oflags, omode, for_exec)` 的四参数正是合流点：路径、标志、创建模式、执行标记。

### 1.3 访问意图的两比特编码

`oflags` 的最低两比特（`O_ACCMODE 003`）编码四种意图，但只有三种合法：`0→读、1→写、2→读写、3→非法`。C 用一张四槽表表达（`open.c:29` 的 `mode_map`），第四槽填 `0` 表示“无权限”，再以 `if (!bits) return EINVAL` 拒绝。

这张表的巧妙与脆弱同源：巧妙在意图到权限位的映射是纯查表，脆弱在“非法”与“无权限”共用 `0` 一个值——读者必须同时记住“`0` 既是第四槽内容又是拒绝条件”。Rust 改写把三值收敛为枚举（`AccessMode`），第四值在类型层面不可构造，拒绝发生在转换边而非使用点。

### 1.4 类型分派的六分支

同一次打开，按 vnode 类型走完全不同的路：普通文件可能被截断，目录拒绝写打开，字符/块设备转交驱动，命名管道要等对端，socket 直接拒绝。`switch (vp->v_mode & S_IFMT)`（`open.c:148`）的六分支不是“功能罗列”，而是 VFS 对后端的路由表——每个分支的下一站都属于另一篇文档（驱动 19~22、管道 17）。

路由发生在权限检查之后（`open.c:146` 的 `forbidden` 先行）：先问“配不配开”，再问“开去哪”。`for_exec` 的 `X_BIT` 替换（同一行）是唯一的例外——为解释器取可执行 fd 时，读权限让位于执行权限。

### 1.5 偏移的抽象：原点、当前位置与溢出

`lseek` 的全部语义是三选一的原点（`SEEK_SET=0/SEEK_CUR=1/SEEK_END=2`）加一个加法：`newpos = base + offset`。真正的知识在两处守卫：管道拒绝一切定位（`ESPIPE`，偏移对流无意义），加法必须防回绕（`EOVERFLOW`）。

C 的溢出守卫是手写的双向比较（`open.c:632-635`）：正偏移上溢、负偏移下溢各一条。正确但脆弱——任何改动都要同时改对两条。`checked_add` 把“回绕即错”交还给整数语义，一条表达式替代四行比较。

### 1.6 关闭的拆除序：先摘索引，再递减计数

关闭的危险在于并发：两个线程同时 `close` 同一个 fd，若都看到“计数为 1”，都会执行末引用的释放。`close_fd` 的第一步 `rfp->fp_filp[fd_nr] = NULL`（`open.c:706`，注释写明“先让后来的一切 `get_filp2` 失败”）正是拆除序的起点——先摘掉索引，后来者连对象都找不到，自然无法重复释放；再递减共享计数，归零才释放 vnode。

拆除序的完整形态是三段：摘索引（防重入）→ 分流（特殊类型各走各的善后）→ 递减归零才释放（`close_filp` 尾）。普通文件的关闭看起来“什么都没做”，恰恰是因为三段中只有第三段对它有意义。

### 1.7 与其他 OS 的打开对照

- **Linux** 以 `open/openat` 的 `O_*` 标志 + `struct file { f_path; f_pos; f_count }` 的三段式（路径引用、独享偏移、原子共享计数）与 `filp` 同型；`O_TRUNC` 的“写检查后截断”与 `open.c:151-156` 同序；`lseek` 的 `ESPIPE/EOVERFLOW` 错误码一致。
- **Redox** 的 `Scheme::open(path, flags, mode)` 以 trait 对象隔离各 scheme（`open.rs: OpenResult`），VFS 的 `FsNodeFactory` trait 正是同一隔离思想——文件服务（MFS/PFS）是打开机的对端，不在本阶段实现，trait 使创建机可脱离真实 FS 单测。
- **seL4** 无打开原语，`open` 的“名字→能力”绑定由用户态文件服务模拟；VFS 的 `fd→filp→vnode` 二跳在 seL4 中对应“fd 表→cap 槽”的用户态复刻，但能力推导（`mint/copy`）的权限收紧语义与 `forbidden` 先行同序。

### 1.8 小结

打开是六段管线（意图解码→双表预留→解析或创建→权限→类型分派→提交或回滚），关闭是三段拆除（摘索引→分流→递减归零才释放），定位是一次带守卫的加法。三者的开关分别是 `oflags` 意图位、`O_CREAT` 有无位、`S_IFMT` 类型位、`SEEK_*` 原点位——记住四组开关，就记住了本篇全部。

---

## 2 C 源码分析

### 2.1 `mode_map` 意图编码（`open.c:29,97-99`）

`static char mode_map[] = {R_BIT, W_BIT, R_BIT|W_BIT, 0}` 以 `O_ACCMODE` 为下标：`bits = mode_map[oflags & O_ACCMODE]`，`0` 值即拒绝（`if (!bits) return EINVAL`）。`R_BIT 004/W_BIT 002/X_BIT 001` 定义于 `minix3/minix/include/minix/const.h:117-119`；`O_RDONLY 0/O_WRONLY 1/O_RDWR 2/O_ACCMODE 003` 定义于 `minix3/sys/sys/fcntl.h:64-67`。

### 2.2 `do_open/do_creat` 对偶守门（`open.c:38-78`）

`do_open` 取 `job_m_in.m_lc_vfs_path.flags`，见 `O_CREAT` 即 `EINVAL`（46），经 `copy_path`（49）进 `common_open(..., 0, FALSE)`（52）。`do_creat` 取 `m_lc_vfs_creat` 的名址/名长/标志/模式（66-69），缺 `O_CREAT` 即 `EINVAL`（71），经 `fetch_name`（74）进 `common_open(..., create_mode, FALSE)`（77）。`for_exec` 在两处皆 `FALSE`——`TRUE` 只出现在 `exec` 为解释器取 fd 的路径（25）。

### 2.3 `common_open` 七步管线（`open.c:83-293`）

1. 意图解码（97-99）：见 §2.1。
2. 双表预留（102）：`get_fd(fp, start=0, bits, &fd, &filp)` 同时占住 fd 槽与 filp 槽（语义见 14，`filedes.c:110`）。
3. 解析或创建（105-130）：`O_CREAT` 置位走 `new_node`（110，见 §2.4）；否则 `eat_path` 取 vnode（124，`VMNT_READ/VNODE_OPCL` 锁），`vmp` 非空即解（129）。
4. 认领（133-138）：`fp_filp[fd]=filp/count=1/vno=vp/flags=oflags`，`O_CLOEXEC` 置位进 `fp_cloexec_set`。
5. 权限（146）：`forbidden(fp, vp, for_exec ? X_BIT : bits)` 先行。
6. 类型分派（148-274）：REG（149，`O_TRUNC` 时先补 `W_BIT` 检查再 `upgrade` + `truncate_vnode`（定义见 `link.c:366`，27 篇管辖），151-156）/ DIR（158，写意图即 `EISDIR`，160）/ CHR（162，`cdev_open`，166）/ BLK（170，`bdev_open` + 驱动存活检查 + 挂载扫描 + 根 FS 标签投递，172-220）/ FIFO（222，`map_vnode` + 首引用截断 + `O_APPEND` 强制 + `pipe_open` + 共用者合并，225-264）/ SOCK（266，`EOPNOTSUPP`，267）/ 未知（269，`EIO` + 打印，270-273）。
7. 提交或回滚（278-292）：`unlock_filp` 后非 `OK` 即摘 fd、清计数、断 vnode、`put_vnode`——但 `SUSPEND` 豁免（282，“稍后由 revive 路径回复”，见 09/17）。

### 2.4 `new_node` 创建机（`open.c:299-477`）

`last_dir` 取父目录（324，`VMNT_WRITE/VNODE_WRITE`）→ `advance` 取末分量（330，`O_TRUNC` 时 `VNODE_WRITE` 否则 `VNODE_OPCL`）→ 绝对路径叠 symlink 的整体重解（338-347，递归）→ `ENOENT` 时 `get_free_vnode` 取空槽（351）→ `forbidden(W|X)` + `req_create`（362-364）→ 悬垂 symlink 的 `EEXIST` 重解递归（372-427）→ 七字段落定（445-455：`fs_e/inode_nr/mode/size/uid/gid/sdev` + 挂载继承 + `fs_count=1/ref_count=1`）。末分量已存在即 `EEXIST`（459），其他失败原样透传（462）。`dirp==vp` 时不重复解锁（469-471，“锁一次即持有一次”）。

### 2.5 `pipe_open` 配对机（`open.c:483-508`）

读写同开一 fd 即 `ENXIO`（491）→ 无对端且 `O_NONBLOCK` 时写方 `ENXIO`、读方直接开（496-497）→ 无对端且阻塞则 `fp_popen.fd=fd + suspend(FP_BLOCKED_ON_POPEN)` 返回 `SUSPEND`（500-502）→ 有对端且 `susp_count>0` 则 `release` 唤醒（504-505）。

### 2.6 `do_mknod/do_mkdir` 节点机（`open.c:514-598`）

`do_mknod` 以 `PATH_RET_SYMLINK` 初始化（533， symlink 本身即 `EEXIST`，532）→ 非超管创非 FIFO 即 `EPERM`（538-539）→ 类型位 + 权限位 `& umask`（541）→ `last_dir` 取父（545）→ 父非目录即 `ENOTDIR`（548）→ `forbidden(W|X)` 通过后 `req_mknod`（550-553）。`do_mkdir` 同型：`I_DIRECTORY | dirmode & RWX & umask`（583）→ `last_dir`（584）→ `req_mkdir`（590-591）。

### 2.7 `actual_lseek/do_lseek` 定位机（`open.c:603-669`）

`get_filp2(rfp, seekfd, VNODE_READ)` 取 filp（611）→ FIFO 即 `ESPIPE`（616-619）→ `SEEK_SET/CUR/END` 取基（623-625，非法 `whence` 即 `EINVAL`，626）→ `newpos = pos + offset`（629）→ 双向回绕即 `EOVERFLOW`（632-635）→ 位置不变跳过 `req_inhibread`（639-645）→ 解锁返回。`do_lseek` 只负责把新位置写回出参消息（667）。

### 2.8 `do_close/close_fd/close_filp` 拆除机（`open.c:674-727` + `filedes.c:411-523`）

`do_close` 取 `fd/nblock` 后直调 `close_fd(fp, fd, !nblock)`（682）。`close_fd` 以 `VNODE_OPCL` 取 filp（699，`FILP_CLOSED` 亦可取——关闭是“只关语义”的特权，见 14）→ `fp_filp[fd]=NULL` 先行（706）→ `close_filp(rfilp, may_suspend)`（708）→ `FD_CLR`（710）→ 记录锁全清 + `lock_revive`（713-724，见 30）。

`close_filp`（`filedes.c:414`）断言已持 `filp_lock` 与 vnode 锁→ 末引用且非常关时按类型分流：BLK（刷缓存 + `bdev_close`，442-453，错误忽略）/ CHR（`cdev_close`，455）/ SOCK（`may_suspend` 守门 + `O_NONBLOCK` 清除 + 非 `SUSPEND` 归 `OK`，464-490，“关闭不应让调用者困惑”）→ 末引用标记 `FILP_CLOSED`（492）→ FIFO 唤醒等待者（496-500）→ `count--` 归零则 `truncate` 尾同步 + `put_vnode` + 断链清零（501-512），负数即 `panic`（513-514）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `open.c` 的七步直线代码，而是吸收 Linux/Redox 的打开模型后做取舍。以下决策对应 `.design/15-design.v1.md` D1-D7。

### D1 意图类型化

- **C**：`mode_map[4]` 查表 + `!bits→EINVAL`（`open.c:29,98-99`）。
- **Rust**：`AccessMode::{ReadOnly, WriteOnly, ReadWrite}`（`TryFrom<u32>` 拒绝第四值）+ `AccessBits::{R,W,X}` 位集 + `bits()` 纯函数（`os/servers/vfs/src/open.rs:56`）。
- **为什么**：查表的第四槽 `0` 把“非法”与“无权限”混为一谈；枚举使第四值不可构造。替代方案（保留表 + 注释）被否决：表是“数据即逻辑”，枚举是“类型即逻辑”，后者在调用点无需重复 `if (!bits)`。

### D2 标志位集与对偶守门

- **C**：`int oflags` 裸整数 + 两入口各写一次 `O_CREAT` 检查（`open.c:46,71`）。
- **Rust**：`OpenFlags` bitflags + `OpenArgs::validate_for_open/validate_for_creat` 对偶方法（`os/servers/vfs/src/open.rs:114,145`）。
- **为什么**：两次检查是同一校验的对偶（有/无），收敛为一对方法使“开错入口”在参数层可测。`filp_flags` 仍存原始位（C `open.c:136` 同义），未建模的标志位原样透传——位集只解释本篇关心的七位。

### D3 创建外包

- **C**：`new_node` 内嵌 `req_create` FS 往返（`open.c:363`）。
- **Rust**：`trait FsNodeFactory::create` + `MemFs`（常成功）vs `ReadOnlyFs`（常 `EACCES`）双实现 + `CreationOutcome::{Created, Exists, Absent}`（`os/servers/vfs/src/open.rs:327,340`）。
- **为什么**：FS 对端（MFS/PFS）不在本阶段实现；trait 隔离使创建机可单测。替代方案（`enum FsKind` 分发）被否决：工厂行为随.addAttribute 而扩展时，枚举的 `match` 必须处处补臂，trait 的新实现零改旧代码。

### D4 管道纯决策

- **C**：`pipe_open` 内嵌 `find_filp` 全局扫描与 `suspend()` 副作用（`open.c:494-502`）。
- **Rust**：`decide_pipe_open(bits, peer, nonblock, suspended) -> PipeOpenDecision` 纯函数（`os/servers/vfs/src/open.rs:262`）。
- **为什么**：配对矩阵（意图×对端×阻塞×等待数）是纯判定知识，与 worker 挂起机制（08）正交；纯函数使矩阵全覆盖可测，挂起执行留给调用点。

### D5 定位 checked

- **C**：手写双向溢出比较（`open.c:632-635`）。
- **Rust**：`Whence::{Set, Cur, End}` + `base.checked_add(offset)`（`os/servers/vfs/src/open.rs:408,434`），`None→EOVERFLOW`。
- **为什么**：手写比较正确但脆弱（两条必须同时对）；`checked_add` 由整数语义保证。`newpos==pos` 跳过 `inhibread` 的短路保留并以返回值第二元显式（调用点决定是否发 `req_inhibread`，见 12）。

### D6 类型分派纯判定

- **C**：六分支内嵌驱动调用与 `printf`（`open.c:148-274`）。
- **Rust**：`FileType` 六值 + `dispatch_open -> OpenOutcome::{Proceed, NeedTruncate, Reject, Suspend, Delegate}`（`os/servers/vfs/src/open.rs:163,205`）。
- **为什么**：分派判定（纯知识）与驱动执行（19~22 的管辖）分离；`Delegate(Char/Block)` 明确标出移交边界。`SOCK→EOPNOTSUPP` 与未知类型 `→EIO` 以 `Reject` 显式，避免调用点遗漏错误分支。

### D7 拆除分流表

- **C**：`close_filp` 的类型分流散在 60 行中（`filedes.c:435-508`）。
- **Rust**：`special_close(ft, last, may_suspend, nonblock) -> CloseOutcome::{Closed, ClosedLast, Suspend}`（`os/servers/vfs/src/open.rs:491,506`）；拆除的“先 NULL”序由 `filedes.rs:close_fd` 承载，本模块只给分流判定。
- **为什么**：拆除序（时序知识，归 14）与分流（类型知识，归本篇）是两种知识；混写则任一演进都要重读 60 行。`panic("invalid filp count")` 收敛为调用点 `debug_assert`（不可达即 abort 语义等价）。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | `Suspend` 只作 verdict，挂起执行留给 08 worker | `open.rs:245,491` + 本文档 D4/D7 + 15 正文 §1.6 |
| A-8 64 位类型映射 | `Mode=u32` 位集、`off_t→i64` 偏移 | `open.rs:12,434` + 本文档 D1/D5 + 15 正文 §2.1/§2.7 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── open.rs               — 本篇：意图/标志/分派/创建/定位/拆除判定
├── filedes.rs            — Fd/get_fd/close_fd（含拆除序，14）
├── filp.rs               — FilpTable/count（04）
└── path.rs               — Lookup/resolve（13）
```

> 设计决策：§3 D1（意图类型化）/ D3（创建外包）/ D6（类型分派）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `mode_map` | `open.c:29` | `open.rs:56 AccessMode + 79 bits()` | 三值枚举，第四值不可构造 |
| `R/W/X_BIT` | `const.h:117-119` | `open.rs:19/21/23 + 91 AccessBits` | `R=004 W=002 X=001` |
| `O_*` 标志 | `fcntl.h:64-136` | `open.rs:114 OpenFlags` | 七位建模，其余透传 |
| `do_open/do_creat` 守门 | `open.c:46,71` | `open.rs:145,153 validate_for_open/creat` | 对偶方法 |
| `S_IFMT` 六分支 | `open.c:148` | `open.rs:163 FileType + 219 dispatch_open` | 纯分派 verdict |
| `pipe_open` | `open.c:483` | `open.rs:245,262 decide_pipe_open` | 配对矩阵纯函数 |
| `new_node` | `open.c:299` | `open.rs:299,327,381 resolve_or_create` | 工厂外包 + 三值归来 |
| `actual_lseek` | `open.c:603` | `open.rs:408,434 seek_pos` | `checked_add` + 不变短路 |
| `do_mknod` 门 | `open.c:538` | `open.rs:458,476 check_mknod_perm` | FIFO 放行 / 特权守门 |
| `close_filp` 分流 | `filedes.c:435` | `open.rs:491,506 special_close` | 末引用分流表 |
| 错误族 | `open.c` 全文件 | `open.rs:534,565 OpenError::to_errno` | 13 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 意图非空 | `AccessMode::try_from` | 第四值拒绝 | `open.c:99` |
| 开创对偶 | `validate_for_open/creat` | 有无 `CREAT` 互斥 | `open.c:46,71` |
| 拆除先摘索引 | `filedes::close_fd` | `NULL` 先于递减 | `open.c:706` |
| 偏移守恒 | `seek_pos` | 不变不 `inhibread` | `open.c:639` |
| 创建落定 | `NodeDetails` | 七字段一次构造 | `open.c:445-455` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **203 passed / 0 failed**（既有 192 + 本篇新增 11；`minix-types` 独立）。
> 本章直接影响 11 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_access_mode_map_four_values` | `open.c:29,98-99` | 四值映射 + 第四值 `EINVAL` + 高位掩码 | `open.rs:593` |
| `test_open_creat_dual_gate` | `open.c:46,71` | 对偶守门互斥 | `open.rs:605` |
| `test_dispatch_six_branches` | `open.c:148-273` | 六分支 verdict 全覆盖 | `open.rs:618` |
| `test_file_type_from_mode` | `open.c:148` | `S_IFMT` 六值解析 | `open.rs:659` |
| `test_pipe_pairing_matrix` | `open.c:491-506` | 配对矩阵 7 样本 | `open.rs:669` |
| `test_create_or_find` | `open.c:349-462` | 创建/已存/透传三路 | `open.rs:695` |
| `test_factories_differ` | design D3 | 双工厂行为分化 + trait 多态 | `open.rs:726` |
| `test_seek_origins_and_overflow` | `open.c:616-635` | 三原点 + 双向溢出 + `ESPIPE` + 非法 whence | `open.rs:740` |
| `test_mknod_privilege_gate` | `open.c:538-539` | FIFO 放行 / 特权守门 | `open.rs:772` |
| `test_close_shunt` | `filedes.c:435-508` | 分流表 7 样本 | `open.rs:783` |
| `test_errno_map_covers_open_c` | `open.c` 全文件 | 13 变体→errno 全映射 | `open.rs:818` |

测试策略：意图以 `mode_map` 四值全枚举覆盖；分派以六分支 verdict 全覆盖；配对以“读写同开/无对端阻塞/无对端非阻塞读写/有对端/有等待”7 样本覆盖矩阵；定位以三原点 + 双向溢出 + 管道拒绝 + 非法 whence 覆盖；拆除以“共享/末引用/三特殊/socket 三条件”7 样本覆盖；错误以 13 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**203 passed / 0 failed**
- 本节列出与本模块直接相关的 11 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/open.rs`

---

## 6 过渡

本篇在 `13` 路径解析之后、`16` 读写之前，是“名字”与“偏移”之间的绑定层：没有本篇的 fd+filp 落定，`read/write` 的 `filp_pos` 无处安放；没有本篇的类型分派，驱动的 `cdev/bdev` 打不开。

```
13-path-lookup: eat_path/advance/last_dir 给 vnode
   │
   └─► 本篇：意图解码 → get_fd 预留(14) → 解析或创建 → forbidden(29) → 六路分派 → 提交/回滚
          │                                              │
          ├─► 16-read-write：filp_pos 的读写推进（REG 的下一站）
          ├─► 17-pipe：SUSPEND 配对的等待与唤醒（FIFO 的下一站）
          └─► 19~22：cdev/bdev/sdev 的驱动打开（CHR/BLK/SOCK 的下一站）
```

阅读顺序提示：若关心“打开后怎么读写”，下一站 `16-read-write.md`（`filp_pos` 的推进与 `bsf` 锁）；若关心“管道的等待是谁执行的”，下一站 `17-pipe.md`（`suspend/revive` 的完整状态机）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/open.c:1-727`（`do_open/do_creat/common_open/new_node/pipe_open/do_mknod/do_mkdir/actual_lseek/do_lseek/do_close/close_fd`）、`minix3/minix/servers/vfs/filedes.c:411-523`（`close_filp`）、`minix3/minix/include/minix/const.h:117-119`（`R/W/X_BIT`）、`minix3/sys/sys/fcntl.h:64-136`（`O_*` 标志）
- 阶段文档：`13-path-lookup.md`（解析机）、`04-filp-table.md`（共享计数）、`14-filedes.md`（fd 表与拆除序）、`09-main-loop.md`（`SUSPEND` 回复路由）、`29-protect.md`（`forbidden`）、`17-pipe.md`（配对执行）、`19-device-map.md`（驱动移交）
- Rust 实现：`os/servers/vfs/src/open.rs:1`（本篇判定层）、`os/servers/vfs/src/filedes.rs:1`（拆除序）、`os/servers/vfs/src/filp.rs:1`（共享计数）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（`copy_path/fetch_name` 的拷贝语义）
