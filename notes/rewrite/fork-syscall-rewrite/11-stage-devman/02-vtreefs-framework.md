# 02-vtreefs-framework：VTreeFS 框架契约

> **定位**：`run_vtreefs` 之后的世界。本篇回答：主循环按什么表分发请求、inode 树如何存取、read 请求如何变成字节流、devman 的 3 个钩子在框架的哪 13 个槽里。01 给了调用点，这里给被调用方的全部语义。
> **源码**：`minix3/minix/lib/libvtreefs/`（10 个 .c + 4 个 .h，共 1642 行；devman 使用面：`vtreefs.c` / `table.c` / `inode.c` / `mount.c` / `file.c` / `path.c` / `stadir.c`）+ `minix3/minix/include/minix/vtreefs.h`（钩子表与常量）。
> **Rust 模块**：`os/servers/devman/src/vtreefs/`（`inode.rs` 树 + `mod.rs` 服务器/分发/传输）。
> **前置依赖**：01（hooks 注册、run_vtreefs 调用点、SEF 形状）。
> **不覆盖（移交）**：devman 业务结构全字段（03）、`devman_init_devices` 建树内容（04）、read_fn 两种实现（06）、消息字段与 fall-through 判定（05）。

---

## 1. 概念：为什么伪文件系统需要一个框架

### 1.1 重复的样板：每个伪文件系统都要回答同样的问题

devman 把设备树装进文件接口（01 §1.1），但"装进文件接口"这件事本身全是样板：维护一棵名字树、给每个节点编号、处理 mount、回答 lookup/read/getdents、把认不得的消息转交给业务。Minix3 里不止 devman 需要这套——procfs（进程信息伪文件系统）要的是一模一样的东西。于是样板被抽成库：VTreeFS。服务只填"我和别人不一样"的部分（钩子），剩下的由框架包办。

这就是 hook 表的由来：框架在每个"我不知道，自己决定"的点上留一个函数指针槽（共 13 个，`vtreefs.h:24-44`），服务填自己懂的槽，不管的槽保持 NULL 走框架默认。devman 填 3 个（init/read/message，01 §1.3），剩下 10 个的默认行为就是本篇的主题之一。

### 1.2 两类请求：文件请求走表，非文件请求走钩子

进主循环的消息只有两类。VFS 发来的文件请求（mount/lookup/read/write/getdents/…共 17 种）按 `vtreefs_table`（`table.c:6-24`）分发到框架自带的实现；框架实现内部再回调钩子（比如 `fs_read` 调 `read_hook`）。其余一切消息（设备驱动的 ADD/DEL/BIND/UNBIND）统称 `fs_other`，框架连看都不看，直接转给 `message_hook`（`vtreefs.c:66-80`，01 §2.5 已见调用点）。

关键洞察：**devman 只填 3 个钩子，但 VFS 照样能 `ls /sys`**——因为 lookup/getdents/stat 是框架自带实现，不经过任何钩子。钩子是"业务扩展点"，不是"请求入口"。混淆这两者是理解 VTreeFS 最常见的错误。

### 1.3 延迟初始化：树在 mount 时才出生

01 §2.7 已钉死触发链：`fs_mount` 调 `init_hook`，`init_hook` 建树。但框架侧还有另一半故事：`init_server`（`vtreefs.c:16-33`）在 SEF fresh 路径里先分配好 inode 池和 I/O 缓冲（失败就 `panic`，活不下去），`fs_mount` 只负责"引用根 + 调钩子"。也就是说框架分两步走：**先活（分配资源）**，**再营业（mount 后建业务树）**。Rust 侧这两步对应 `VTreeFs::new`（分配，失败 `Err(ENOMEM)`）与 `mount`（营业），[ARCH:A-7] 的第二次实例（第一次是 01 的 `SefHooks`）。

### 1.4 边界声明

本篇讲框架机制（表、树、mount、read 循环、编号）。以下只给调用点与一句话：

- `read_fn` 的两种业务实现 → 06；
- `devman_init_devices` 建了哪几棵树 → 04；
- DEVMAN_* 消息长什么样、fall-through 怎么定性 → 05；
- wire 结构全字段 → 03。

---

## 2. C 源码分析

### 2.1 钩子全表：13 个槽，devman 用 3 个（vtreefs.h:24-44）

| 槽 | 签名行 | devman | 说明 |
|---|---|---|---|
| `init_hook` | :25 | ✅ 注册 | mount 时调一次（01 §2.2） |
| `cleanup_hook` | :26 | NULL | unmount 时调；devman 从不注册，框架的 `if != NULL` 对 devman 恒假 |
| `lookup_hook` | :27-28 | NULL | lookup 前的"刷新"回调；devman 无刷新语义 |
| `getdents_hook` | :29 | NULL | getdents 前的"刷新"回调；同上 |
| `read_hook` | :30-32 | ✅ 注册 | 文件内容唯一来源（§2.5） |
| `write_hook` | :33-34 | NULL | 未注册时 `fs_write` 直接 `EACCES`（file.c:123）——注意与 read 的不对称：读缺省装空文件（EOF），写缺省直接拒绝 |
| `trunc_hook` | :35 | NULL | 02 不实现（ENOSYS，§3.6） |
| `mknod_hook` | :36-37 | NULL | 同上（设备节点由消息通道创建，不走文件创建） |
| `unlink_hook` | :38 | NULL | 同上（删除走 DEL 消息，08） |
| `slink_hook` | :39-40 | NULL | 同上 |
| `rdlink_hook` | :41-42 | NULL | 同上 |
| `chstat_hook` | :43 | NULL | 同上 |
| `message_hook` | :44 | ✅ 注册 | `fs_other` 唯一出口（01 §2.4） |

计数核对：`grep -c "(\*" vtreefs.h` = 13（01 P0-1 的教训已固化）。读写缺省的不对称是 C 的刻意设计：读一个没注册的伪文件应该表现为什么？空（EOF）最无害；写呢？必须拒绝（EACCES），否则静默丢数据。Rust 侧保留这对不对称（`read` 无 hook → 空，`unsupported` → ENOSYS，§3.6 解释 EACCES→ENOSYS 的取舍）。

### 2.2 inode 池：固定数组 + 空闲表 + 两张哈希表（inode.c:31-99）

`init_inodes(nr_inodes, istat, nr_indexed_entries)` 做四件事：断言 `inodes > 0`；三次 `malloc`（节点数组 + 两张哈希表，失败 `ENOMEM` 且按逆序释放已分配的——三次分配、三处返回、逐级 `free`，无泄漏）；把 1~n-1 号槽挂上空闲表（跳过 0 号根）；初始化根节点（stat 拷贝、`i_index = NO_INDEX`、`i_cbdata = NULL）。

三点值得注意。第一，池大小就是 `main` 传的 1024（main.c:89），devman 的树永远超不过 1024 个节点——这是硬上限，不是调优参数。第二，失败路径是三段式 `ENOMEM`：C 在这里居然不 `panic`（与 `init_server` 的三 `panic` 对比：`init_server` 认为"池建不起来=活不下去"，而 `init_inodes` 把错误码还给调用者）。Rust 侧统一为 `Result`（§3.5）。第三，`CHECK_INODE` 宏（inode.c:19-25）是不变量的 C 写法：指针在数组界内、序号自洽、非根必有父（或已删）——Rust 用类型（`Ino` 非零 + `occupied` 检查）表达同一组约束。

### 2.3 加节点：7 个断言 + 双限名字 +  indexed  purge（inode.c:185-249）

`add_inode(parent, name, idx, istat, nr_indexed_entries, cbdata)` 先过 7 个断言（inode.c:193-200）：父合法、父是目录、父未删、名字 ≤ NAME_MAX、idx 合法、stat 非空、名字不重复。然后拿空闲槽（空了就 `purge_inode` 腾地方）；名字 ≤ 24（`PNAME_MAX`，vtreefs.h:14）用节点内静态缓冲，超长 `malloc`（失败返回 NULL——**全函数唯一的 NULL 返回**）。

名字有**两档限制**，容易混淆：`NAME_MAX 511`（`sys/sys/syslimits.h:57`，硬上限，断言守卫）与 `PNAME_MAX 24`（内联阈值，只决定名字存栈上还是堆上，不决定成败）。Rust 侧是统一的 `String`（堆），双限只剩下一条 `ENAMETOOLONG`（>511），24 的阈值在注释里交代去向（§3.3）。

`purge_inode`（inode.c:142-179）是本篇最重要的"devman 无关代码"证据：它只回收**有 index** 的节点（`node->i_index != NO_INDEX`，inode.c:169），轮询全池找"非父、零引用、无孩子"的 indexed 节点删掉腾地方。而 devman 的 4 处 `add_inode` 调用**全部传 NO_INDEX**（device.c:199/203/333/375，grep 实证）——purge 对 devman 永远找不到可回收节点。结论：devman 的池是**不可回收的硬上限**，1024 用完就 `assert` 炸（debug）/未定义（release）。Rust 把这翻译成诚实的 `Err(ENOMEM)`（§3.5，[ARCH:A-7]），并在单测里用小池子锁定（`pool_exhaustion_is_enomem`）。

### 2.4 删节点：两阶段 + 目录父链保留（inode.c:544-604）

`delete_inode` 分两步。第一步（未删过）：递归删孩子（**先于置标志**，inode.c:556-558——顺序是刻意的：孩子的删除逻辑依赖父链还在），从名字哈希摘除，有 index 才从 index 哈希摘除（devman 节点永远走不到这行），释放堆名字，置 `I_DELETED`；非目录立即与父断链（`unlink_inode`），**目录不断**——注释写得明白（inode.c:539-541）：目录要留着父链，否则 `cd ..` 出不来。第二步（已删或刚删）：引用归零且无孩子才真正回收进空闲表；否则继续挂着（`put_inode` 归零时再来一次，inode.c:496-497）。

引用计数三函数各司其职：`ref_inode` +1（inode.c:504-510）；`put_inode` -1，归零且已删则触发真删（inode.c:484-498）；`get_inode` 查找+1（inode.c:468-478）。`fs_putnode`（inode.c:610-626，VFS 放引用的入口）有个怪算式：`i_count -= count - 1; put_inode(node)`——净效果 -count（put 内再 -1 并检查删除），分两步写只是为了复用 put 的删除检查。Rust 侧 `release()` 一次做完（§4.2）。

根不能删（`assert(node != &inode[0])`，inode.c:549）→ Rust `EINVAL`。`is_inode_deleted` 就是读标志位（inode.c:599-604）。

### 2.5 读循环：5 步 + 部分结果规则（file.c:46-103）

`fs_read(ino_nr, data, bytes, pos)`：

1. `find_inode` 找不到 → `EINVAL`；
2. 非 regular 文件 → `EINVAL`（目录不能这样读，想列目录走 getdents）；
3. 已删或无 read_hook → 返回 0（装 EOF，file.c:62-64）；
4. 分块循环：`chunk = min(剩余, bufsize)`，调 `read_hook(node, buf, chunk, pos, cbdata)`，`len > 0` 就 `copyout`；
5. 错误处理（file.c:88-94）：出错但已产出 → 返回已产出数（部分结果）；一字节没产出就出错 → 返回该错误。`len < bufsize` 跳出（file.c:97-98，短读即 EOF 信号）。

第 5 步是全文最微妙的语义：**错误不一定是错误**——读了 100 字节、第 101 个出错，调用者拿到的是"成功读了 100"，而不是错误码。Rust 的 `Result<Vec<u8>, Errno>` 照搬这条：`Ok(partial)` vs `Err(e)` 的分界就是 `out.is_empty()`（单测 `read_error_rules` 双分支锁定）。

还有个 C 没说的安全假设：循环信任 `read_hook` 返回的 `len ≤ chunk`（超了就 `copyout` 越界读静态缓冲）。06 的实现会遵守，但框架不能替未来背书——Rust 在此设防：`len > chunk` 直接 `EIO`（§3.5 硬化说明，单测锁定）。

### 2.6 编号：ino = 槽位 + 1，0 永远无效（inode.c:369-375）

`get_inode_number` 返回 `i_num + 1`。于是外部看到的编号从 1 开始，0 天然无效（`find_inode(0)` 会读 `&inode[-1]`——C 靠调用者不传 0 保证）。Rust 用 `Ino(u32)` 新类型 + 构造期拒绝 0（`checked_sub`），把"调用者保证"变成"类型保证"。另有两个相关事实：`find_inode` 对已删节点照样返回（调用者自查 `is_inode_deleted`，read/lookup 都是这个模式）；`get_inode_cbdata` 就是读字段（inode.c:304-310），cbdata 是 01 §2.3 的"私房数据"指针，Rust 侧是 `usize`（§3.3 解释为什么不用引用）。

### 2.7 查名字：lookup 的完整形状（path.c:9-59）

`fs_lookup` 比 `get_inode_by_name` 多四样东西：`find_inode` 失败 → `EINVAL`；非目录 → **`ENOTDIR`**（注意不是 EINVAL，path.c:19-20——01 轮 P0-2 的姊妹教训：错误码必须逐个核对）；超长 → `ENAMETOOLONG`；`.` 停留、`..` 上移（根的 `..` → `ENOENT`，注释"should not be possible"，path.c:30——VFS 实际不会问根的 `..`，C 把不可能路径也给了明确错误码，Rust 原样保留）；以及 lookup_hook 刷新（devman 无，跳过）。成功则 `ref_inode` + 回填 `fsdriver_node`。

注意成功路径的 `ref_inode`：这是 VFS 打开文件的引用，生命周期归 VFS 侧。Rust 框架的 `lookup` 是纯查询（不碰引用计数），引用管理归传输层（VFS stage）——分层决策，§3.4 论证。

### 2.8 列目录：顺序是契约，编码是传输（file.c:195-295）

`fs_getdents` 的**顺序**是框架契约：pos 0 → `.`（自己），pos 1 → `..`（父，根则自己），之后先 indexed（`pos-2 < indexed`，devman 的 indexed 恒 0，此分支恒假），再按树序走非 indexed 孩子（跳过 indexed 的——对 devman 是空操作——再跳过已删的，file.c:236-262）。getdents_hook 刷新（devman 无）。**编码**（`fsdriver_dentry_*` 把条目塞进 VFS 缓冲）是传输细节，Rust 侧只交付 `Vec<Dirent>`（name/ino/is_dir），编码归传输层（§3.4 同款分层）。

C 有个反直觉但必须保留的行为：`fs_getdents` **不检查目录位**——对 regular 文件调用照样返回 `.`/`..` 两条。Rust 的 `readdir` 同样不设 `S_ISDIR` 门（单测用文件断言两条目）。删掉的目录照样遍历（只有未知编号才 `EINVAL`）——同理保留。

### 2.9 装载与卸载：mount.c 两函数 + 一个恒假分支（mount.c:10-56）

`fs_mount` 见过（01 §2.7）：拒 root（EINVAL）→ 取根引用 → 调 init_hook。`fs_unmount`（mount.c:44-56）：放掉根引用 → 有 cleanup_hook 才调。devman 的 hooks 表只有 3 个槽（main.c:78-80），`cleanup_hook` 永为 NULL——框架的 `if != NULL` 对 devman 是死分支，Rust 侧**不建模**这个槽（连 `None` 都不设：设了反而暗示"将来可能有人填"，而填了就会改变 unmount 语义——01 §3.1"只建模 3 槽"原则的延续）。

`fs_stat`（stadir.c:9）按 `i_stat` 回填 `struct stat`——Rust 侧 `stat()` 返回 `InodeStat`，`struct stat` 编码归传输层（§2.8 同款）。

### 2.10 非文件消息：拷一份再转交（vtreefs.c:66-80）

`fs_other` 把消息拷一份再调 `message_hook`——注释明示原因："不是所有用户都善待消息"（`Not all of vtreefs's users play nice with the message`）。Rust 侧 `other()` 是值语义调用，天然等价（§3.3）。

---

## 3. Rust 设计决策

### 3.1 A-1 落地：框架住 devman  crate 内部（`src/vtreefs/`）

plan §7.3 的倾向是"devman 内部最小等价实现"，本篇结合新证据确认（plan 允许 02 做决策，§2"02（决策）"）：

1. **01 已收敛**：`FsHooks` 归 `hooks.rs` 所有（01 CONVERGED，doc+code+test 锁定）。框架若住共享 crate，要么重复定义钩子形状（事实不唯一），要么搬走 01 的类型（推倒已收敛篇）——两者都比"内部模块直接用"差。
2. **站内先例**：RS 的 SEF 形状住 `os/servers/rs/src/sef.rs`，共享的 `minix-sef` 保持 5 行 stub——"各 server 自带形状、共享 crate 占位"是本仓库既成模式（`os/servers/*/src/sef.rs` × 5 vs stub × 1）。VTreeFS 同理：`os/libs/minix-vtreefs` 保持 stub（有名字、无内容、workspace 已登记），devman 内部实现使用面。
3. **procfs 复用时再抽**：抽取重构的输入（devman 的 02 实现 + procfs 的需求）那时才齐，现在抽是 premature——YAGNI。

三处标注：本节 + `vtreefs/mod.rs` 模块头表 + 设计快照（scan Gate H 记录）。

### 3.2 树：Vec 池 + 空闲栈 + 线性扫描（BTreeMap-free 是刻意的）

C 用数组 + 空闲表 + **两张哈希表**。Rust 侧保留数组（`Vec` 池，槽位即编号-1）与空闲表（`Vec<usize>` 栈），删掉两张哈希表，改线性扫孩子。理由：devman 的树以十计节点、上限 1024——哈希省不下可测的时间，却要多维护一套索引一致性（删节点时两处摘除，C 的 `LIST_REMOVE` × 2 就是代价）。"为 1024 个节点的树维护哈希表"是典型的 C 惯性（内存紧张时代的遗产），不是语义。查找 O(孩子数)，分配 O(1)——单测覆盖，性能主张到此为止（不做 bench，树太小，bench 是噪音）。

### 3.3 类型映射表（C 惯用法 → Rust）

| C | Rust | 理由 |
|---|---|---|
| `struct inode` 56 字节全字段 | `Inode`（parent/children/name/stat/refcount/deleted/cbdata） | 去 `i_num`（槽位即编号）、`i_index`/`i_indexed`（NO_INDEX-only，类型级省略）、哈希链（§3.2）、`i_namebuf`（统一下面） |
| `i_namebuf[25]` + 超长 malloc | 统一 `String` | 双限只剩 `NAME_MAX 511` 一条检查；24 阈值是分配策略不是语义 |
| `cbdata_t`（void*） | `usize`（地址/整数两用，注释引 inode.h:19-23 的 cixfer 说明） | 不用引用：框架不拥有指向物，所有权归业务（04/06）；`usize` 是"不透明句柄"的诚实表达 |
| `ino_t`（int，0 非法靠约定） | `Ino(u32)` + 0 拒绝 | 类型保证代替调用者保证 |
| `struct inode_stat` | `InodeStat`（5 字段）+ `From<RootStat>` | C 只有一张 stat 结构；Rust 分 root/通用两名同形，转换显式 |
| `fsdriver_dentry` 流 | `Vec<Dirent>` | 顺序是契约（§2.8），编码是传输 |
| `struct stat` 回填 | `stat()` 返 `InodeStat` | 同上 |

### 3.4 两条分层线：引用计数与编码归传输层

`lookup` 成功不 `ref`（C 会 `ref_inode`）。论证：C 的 ref 是"VFS 打开了文件"的生命周期事件，属于 VFS 协议（打开/关闭配对）而非树查询；框架只答"名字在哪"，传输层决定"打开意味着什么"。同理 `readdir`/`stat` 只给形状，编码归传输。两条线的好处：框架 100% 可单测（无 VFS 依赖），传输层独立演进。代价：与 C 的调用点不对齐处必须在 §4.3 逐条列出（CSSCM 合规），不能悄悄抹掉。

### 3.5 assert → Err 映射表（[ARCH:A-7] 第二次实例）

| C 断言/行为 | Rust | 单测 |
|---|---|---|
| `assert(inodes > 0)` | `EINVAL`（0 池） | `init_zero_capacity_is_einval` |
| 三 `malloc` 失败 → ENOMEM | `try_reserve` → `ENOMEM` | （小池 exhaust 间接覆盖；reserve 失败路径类型同） |
| 池空（purge 对 devman 无效）→ assert | `ENOMEM`（硬上限诚实化） | `pool_exhaustion_is_enomem` |
| 名字重复/非目录父/长名 | `EEXIST`/`ENOTDIR`/`ENAMETOOLONG` | 三单测 |
| `put` 下溢/删根/ino 0 | `EINVAL` | 三单测 |
| `panic("init_inodes failed")` 等三 panic | `VTreeFs::new` 返回 `ENOMEM` | 类型级（Result）+ exhaust |
| hook 返回超长（C 信任） | `EIO`（硬化，§2.5） | `read_overlong_hook_result_is_eio` |
| 已释放槽可解析（C  stale） | `None`/`EINVAL`（硬化） | readdir 部分（reaped → EINVAL） |

### 3.6 未实现槽：显式 ENOSYS（与 C 不对称默认的取舍）

devman 未注册的变更槽（write/trunc/mknod/…）在 Rust 侧是 `unsupported() → ENOSYS`。C 的 `fs_write` 无 hook 时是 `EACCES`（file.c:123）——为什么不照搬？因为 EACCES 在 VFS 语义里是"权限不够"（调用者可能换个身份重试），而真相是"这个服务器根本没这功能"，ENOSYS（"没实现"）才是诚实回答，调用者行为也不同（换身份 vs 放弃）。read 的 EOF 默认保留（§2.5：空文件是最无害的谎言，且 06 的事件语义依赖它）。这条取舍记在这里，99 收口错误码时复核。

### 3.7 传输注入：`run` 的循环逻辑现在就绪，生产传输待定

`VTreeFs::run(&mut impl Transport)` 是 `fsdriver_task` 分发形状的现在完成时：Mount/Lookup/Read/Readdir/Other/Unmount 六分支 + 错误→哨兵回复。`Transport` trait 把"下一条消息从哪来"反转出去——测试用 `VecTransport`（脚本进、回复出），生产用内核 IPC（`minix-sys::receive` 现为 `todo!()`，调了就 panic）。于是 main 的 park 循环性质变了：01 时它是"整个事件循环缺席"，现在它是"循环逻辑就绪，只缺传输"。P1-6 收窄为传输接线（§4.4），doc 与注释同步改写。

Reply 的错误传递：`Read` 失败时回哨兵 `err_marker`（`0xFF` + 4 字节 errno）——因为空 `Vec` 是合法 EOF，错误必须有别于空。真正的 fsdriver 回复带状态字，这里是测试传输层的最小可辨别编码（§4.3 注记，传输落地时替换）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/vtreefs/
  inode.rs  — Ino / InodeStat / Inode / InodeTree（池、树、引用计数、遍历）
  mod.rs    — VTreeFs（mount/unmount/lookup/read/readdir/stat/other/run）
              + Request / Reply / Transport / VecTransport（测试）
```

`lib.rs` 加 `pub mod vtreefs` 一行。`main.rs` 的 park 注释改写（§3.7 收窄语义）。

### 4.2 关键不变量

1. 槽位 == 编号 - 1（`Ino(i+1)`，构造/查找唯一换算点 `idx()`）。
2. 根永在槽 0、永不可删、引用可增减（mount/unmount 配对）。
3. `read` 的 hook 调用次数 == 循环轮数；`pos` 严格递增 `got` 之和。
4. `delete` 先递归孩子再置标志；文件立即断链、目录留链到回收（C §2.4 原样）。
5. `run` 每个 `next()` 必恰一次 `reply()`（循环体无 `continue` 裸奔路径）。

### 4.3 与 C 步骤的差异说明

| C 步骤 | Rust 对应 | 分类 |
|---|---|---|
| 两张哈希表 | 线性扫孩子 | 设计决策（§3.2，小树） |
| `i_namebuf` + 超长 malloc | 统一 `String` | 设计决策（分配策略非语义） |
| `purge_inode` 回收 | 无（NO_INDEX-only，pool 硬上限 + ENOMEM） | 架构演进 A-7（C 的 assert→诚实错误） |
| `assert` 7+3+… | EINVAL/ENOTDIR/ENAMETOOLONG/EEXIST/ENOMEM（§3.5 表） | 架构演进 A-7 |
| hook 超长信任 | `EIO` | 安全硬化（§2.5，单测锁定） |
| 已释放槽可解析 | `EINVAL` | 安全硬化（单测锁定） |
| 负 errno 返回 | 正 `Errno`（符号翻转） | 约定对齐（minix-rs user-space positive convention，`Errno::to_i32` 文档） |
| `lookup` 成功 `ref` | 不 ref（传输层负责） | 分层决策（§3.4） |
| dirent/stat 编码 | `Dirent`/`InodeStat` 形状 | 分层决策（§3.4） |
| `fs_write` 无 hook → EACCES | `unsupported` → ENOSYS | 设计取舍（§3.6） |
| `fs_other` 拷贝消息 | 值语义调用（天然等价） | 设计决策 |
| cleanup_hook 槽 | 不建模（devman 永不注册） | 分工（01 原则延续，有 main.c:78-80 证据） |
| `fs_putnode` 怪算式 | `release()` 一次完成 | 设计决策（等价化简，净效果同为 -count+删除检查） |

### 4.4 P1-6 收窄记录

01 的 P1-6（"main park，事件循环缺席"，02-owned）本篇后收窄为"传输接线"：`run` + `Transport` + `VecTransport` 端到端已测（`run_dispatches_script`），`main.rs` 注释已改写。STATE 的 P1-6 更新 owner 为"传输（minix-sys IPC 落地）"，不再阻塞 03~13（框架数据平面 100% 可用，04/06 直接调 `tree_mut`/`read`）。

---

## 5. 测试要点

`vtreefs/` 内 `#[cfg(test)]`（`cargo test -p minix-devman`）：

| 测试 | 断言 | 对应 |
|---|---|---|
| `init_root_number_is_one` | 根 Ino(1)，live 1 | §2.6 编号 |
| `init_zero_capacity_is_einval` | 0 池 EINVAL | §3.5 |
| `add_lookup_roundtrip` | 加/查/名/cbdata/ENOENT | §2.3/§2.7 |
| `add_duplicate_is_eexist` | 重名 EEXIST | §2.3 断言 |
| `add_under_file_is_einval` | 文件下加 EINVAL | §2.3 断言 |
| `add_long_name_is_enametoolong` | 512 拒/511 收 | §2.3 双限 |
| `pool_exhaustion_is_enomem` | 小池满 ENOMEM | §2.3 purge 无效 |
| `delete_recursive_and_reuses_slot` | 级联删 + 槽复用 | §2.4 |
| `delete_referenced_is_lazy` | 有引用延迟删，release 回收 | §2.4 两阶段 |
| `release_underflow_is_einval` | 0 引用放 EINVAL | §2.4 断言 |
| `root_delete_and_ino_zero_are_einval` | 删根/ino 0 EINVAL | §2.4/§2.6 |
| `lookup_dots_and_notdir` | ./.. / 根.. ENOENT / 文件下 ENOTDIR | §2.7 |
| `mount_rejects_root` | root mount EINVAL | §2.9 |
| `mount_fires_init_hook` | 每次 mount 都调（框架忠实，guard 归 01） | §2.9 |
| `read_bad_ino_and_dir_are_einval` | 未知/目录 EINVAL | §2.5 步骤 1-2 |
| `read_no_hook_and_deleted_are_eof` | 无 hook/已删 → 空 | §2.5 步骤 3 |
| `read_full_single_chunk` | 8 字节全对 | §2.5 循环 |
| `read_multichunk_until_short` | 短块即停 | §2.5 步骤 5 |
| `read_error_rules` | 先错 Err / 后错部分 Ok | §2.5 部分规则 |
| `read_overlong_hook_result_is_eio` | 超长 EIO | §3.5 硬化 |
| `readdir_dot_dot_children` | 顺序 + 游标 + 删跳过 + 文件两条 + reaped 拒 | §2.8 |
| `other_forwards_to_message_hook` | 转交可观测 | §2.10 |
| `run_dispatches_script` | transport 端到端 5 请求 | §3.7 |
| `unsupported_is_enosys` | 未实现槽 ENOSYS | §3.6 |

截至 2026-09-04：`cargo test -p minix-devman` **31 passed / 0 failed**（01 的 7 + 本篇 24）。`cargo clippy -p minix-devman --all-targets` devman 部分 0 警告（minix-types 遗留 2 处与本篇无关：event.rs 空行、vm.rs 枚举体量；本次为 02 新增 `Errno::ENOENT/EIO/EEXIST/ENAMETOOLONG/ENOTDIR` 5 个 assoc 常量，纯加法）。

---

## 6. 过渡

框架就绪：树能建、请求能分发、字节能流出来。但树里**节点的模样**（`devman_device` 17 个字段是什么、UNBOUND/BOUND/ZOMBIE 怎么转、wire 上设备长什么字节）是 03 的；**树长什么样**（`devices/` 下挂什么、`events/` 哪来、路径怎么拼、dev_id 谁发）是 04 的。04/06 会直接调用本篇的 `tree_mut().add` 与 `read`——如果忘了"节点存哪"，回看 §4.1；如果疑惑"为什么 lookup 不 ref"，答案在 §3.4。

---

## 7. 参见

- `01-devm-init-main.md` — hooks 形状与 run 调用点（本篇的输入）
- `03-devm-structs.md` — 节点荷载全字段（本篇 `cbdata` 背后的结构）
- `04-device-tree.md` — 建树内容（本篇 `add` 的调用方）
- `05-devm-message-contract.md` — `other()` 转交后的世界
- `06-event-buf.md` — `read_hook` 的两种实现（本篇 `read` 的调用目标）
- C 源：`minix3/minix/lib/libvtreefs/{vtreefs.c:16-110,table.c:6-24,inode.c:31-626,mount.c:10-56,file.c:46-295,path.c:9-59,stadir.c:9}`、`minix3/minix/include/minix/vtreefs.h:14,24-44`、`minix3/sys/sys/syslimits.h:57`
