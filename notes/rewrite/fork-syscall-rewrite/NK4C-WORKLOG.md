# NK4-C WORKLOG（Task C「清零者」追捕 → 三架构 + 命令面 + 测试上机）

> 本文件是**记忆与交接载体**（git-tracked）。长程任务：每完成一个逻辑单元就更新顶部"当前状态" + 追加一节 + commit。
> **顶部状态必须始终是最新的**——用户会在任意时刻让 agent 收尾，接手者只读它 + `git log --oneline -20` 就要能接续。
> 详细取证历史见 `.review/zcode/edge1/FIXLOG.md` 迭代 27-33（**本地文件、被 gitignore、换工作树会丢**——关键结论在本文件 §交接来源有副本）。

---

## 当前状态（每次 commit 前更新，一屏读完）

- **阶段**：**1.3 rc marker 链（F14+F15 已修，各两次独立真机验证 s14b-s14j；当前 frontier = 1.10「PM↔VM 缺页服务循环」——PM 在 0x209500/0x2200b0 反复 fault、VM 反复服务成功但不收敛，疑似 served PTE 不持久/缺 flush（对位 R2 登记 F2 + sync_slot_pte 无使用者），见 1.9d 节）**：历史——S3 Task C → F10b/c/d → F11 → F12 → F13 → **F14（Phase 3 drain 同步拷贝，1.7/1.8 节）** → **F15（队列唤醒完成码按 SENDING_FROM_KERNEL 门控，1.9 节）**：boot 从 449 livelock 推进至 birth 协议全过、PM↔VFS 同步、~19200 行无崩溃无 picknone
- **根因最终版（S3 定位修正 S2 第 4 点未收敛项）**：`kernel_call_finish` 的 eager 回执直写（errno 非零时把 80 字节回执写到进程表的 `p_delivermsg_vir`）在 C 里只存在于 `kernel_call()`/SYSCALL 腿（system.c:83），且该腿每次入口都先刷新 `p_delivermsg_vir`（system.c:141），目标结构性新鲜；C 的 int33 陷阱腿（proc.c `mini_*`）从不执行这条直写（状态经 h_errno/寄存器，真回执经 MF_DELIVERMSG 投递）。minix-rs 把 int33 腿（含 SENDA）统一接进同一 finish 机器而丢了这条**门纪律**：SENDA 入口按 C 对位故意不刷新 `p_delivermsg_vir`（trap_dispatch.rs 的 `!is_senda` 存储臂），于是 SENDA 窗内的同步 errno 回执落写上一次 SYSCALL 腿调用留下的陈旧地址（帧已弹出、区域已复用）→ self 槽被回执零字抹掉 → `endpoint_slot(0)` → SIGSEGV。完整证据链与修正说明见 S3 节
- **修复（方案甲，门纪律）**：新增 `kernel_call_finish_ipc_door`（int33 腿专用，跳 eager 直写，其余簿记不变）+ `VmSuspendContext.resume_skip_eager_reply` 门标记（IPC 腿挂起的调用被 stage 3a 补完成时同样不写）；详见 S3 节
- **新停点（1.10 处置，待定性）**：F15 修订后 s14i/s14j 两次复跑：0 崩溃 0 NoPerm、无 picknone、~19200 行持续推进，150s 时限未到 rc marker。死点活动 = PM(0)↔VM(8) 缺页循环（fa=0x209500/0x2200b0，VM bytes 显示真内容服务成功但不收敛）。入口：①VM 侧 dump 该 VA 的 PTE 与 region 槽状态（对照 F2「页粒度 remap 缺 INVL/flush」+ sync_slot_pte 无使用者）；②确认 PM 的 fault 是同 VA 重复（=PTE 丢失）还是相邻 VA 推进（=正常但慢）。历史「RS queued=no」在 s14h-j 未复现（F15 修订消除），降级观察。
- **新停点（登记，阶段 1.4 处置）**：~~修复后 boot 推进到 RS 阻塞在 int33 receive~~ → ~~F10c 后 s13e：8 服务器 runnable=yes queued=no~~ → **F10d 后 s13f 新停点 = VM 缺页解决循环**：服务器已跑起来，尾态 VM(0x100) VMREQUEST(0x800)、多数服务器 PAGEFAULT(0x400) 等 VM 解故障、RS(0x102/0x108) RECEIVING(0x8)、`picknone rs_flags=0x8`（RS 空转等消息）、`do-memory enter` 全程仅 1 次（VM 处理一次内存请求后自停）。需查 VM 与 PAGEFAULT 进程的故障解决握手是否闭环（VM 自身 VMREQUEST 谁驱动）
- **⚠️ 复现环境硬约束（S0 发现，仍有效）**：必须用仓库内 `tmp/nk4a/vars.fd`（累积过的 UEFI vars）的副本替换 §2.2 QEMU 命令里的 vars 槽（本文件 S0 节「做法」段已写好完整命令；QEMU 会写它，不要直接用仓库文件本体）；用全新 `OVMF_VARS_4M.fd` 会让 EFI 模块装载落点改变 → 内核 `vm_handoff free n=0` → VM 在 `boot.rs:157` assert panic → 全系统 livelock（比 Task C 更早的死法，签名完全不同；该 fresh-vars 布局鲁棒性 bug 已登记不修）；另 QEMU 命令照 S0 节模板原样跑，自行加 `-machine q35 -m 512` 会导致 QEMU 启动即退（实测）
- **已修复**（commit）：
  - `a470a8d9c`+`6be40748f` Task C 根因：int33 陷阱腿恢复 C 门纪律（详见 S3 节）
  - **F10b**（待 commit）：`KcallResult::Data(i32)` + `reply_wire()` — 数据码原样，错误码取负；`vmctl_memreq_get` ENOENT/VMPTYPE_CHECK 改 Data 路径，do_memory 服务成功
  - **F10c**（commit `fc66eb148`）：`copy_struct_from_user` + `dispatch_getmcontext/setmcontext` 改用 AddressRef::Process 走 caller CR3 页表解内核栈 PA，SetSys 不再 EFAULT，Allow 清 NO_PRIV 成功
  - **F10d**（commit `6e51b723a`）：`privctl_allow`/`privctl_yield`/`privctl_disallow` 改用调度器感知的 `proc_table::rts_unset`/`rts_set`，镜像 C `RTS_UNSET`/`RTS_SET` 宏的 enqueue/dequeue 半（proc.h:206-224），修复 NO_PRIV 清除后服务器 runnable 却永不入 run queue（runnable=yes queued=no）
  - **F12**（本轮）：**minix-rt slab 服务器 OOM 修复（跟踪表容量与池容量对齐）**——真机 `nk4c: OOM-RT slabs=64/64 px=65/512 big=0` 实锤：非物理内存不够、非真泄漏（池 87% 空闲、px≈slabs+fp 无页泄漏、big=0），而是 `MAX_SLABS=64` 固定记录表与 `GLOBAL_POOL_BYTES`（512 页）不一致——池在第12轮扰动实验撑到 512 页但记录表未跟着长，slab 对象有效上限被卡在 ~256KiB（512 页池只用 12.7%）。修复：`MAX_SLABS`/`free_pages` 栈改为绑定 `GLOBAL_POOL_BYTES/PAGE_BYTES`（新增 `GLOBAL_POOL_PAGES`）。真机 s13l：OOM 消失、`vm-pf recv` 324/325→**422**（越过原墙）。新停点见下条
  - **F11**（commit `d9fc1f649`）：**VM 请求链双入链死锁修复**——SYSCALL 快路径（`trap_dispatch.rs` VmSuspend 臂）与 `kernel_call_finish`（`syscall.rs` L3446）对同一挂起各调一次 `vm_enqueue_and_notify_vm`，头插两下使 `p_next[nr]` 自指成环 → `vm_memreq_get` 摘头后 head 落回自环节点永不归 None → `was_empty` 恒 false → 后续请求者入链不再唤醒 VM（稳定死锁）。移除 trap 腿冗余的 dequeue+enqueue+notify（C `vm_suspend` proc.c:241 只跑一次，唯一归属 finish），trap 臂只保留 SYSCALL 腿特有的上下文保存。详见 1.4b 节
  - **F13**（本轮）：**PM↔VFS 握手 Kernel(201) 修复——VFS_PM_INIT 用阻塞 ipc_send**——真机 s13l boot 停在 `PM: can't sync up with VFS (per-process send): Kernel(201)`（`os/servers/pm/src/init.rs:702`）。根因：`KernelIpcTransport::send` 把所有 send 映射到后端 `sendnb`（非阻塞），但 C `main.c:226` 的 VFS_PM_INIT 用的是**阻塞 `ipc_send`**——PM 先于 VFS 起时 VFS 尚未进 receive，sendnb 拿到 ENOTREADY(201) 直接 panic。穷举 PM 13 处 `.send()`：12 处是回复（C `ipc_sendnb`）或异步 tell_vfs（`asynsend3`），非阻塞正确；唯 init.rs:701 需阻塞。修复：trait 加 `send_blocking`（委托后端 `send`/SEND_NR），仅 VFS_PM_INIT 调用（其余 12 站点不动，低回归）；并纠正 `send` 的错误注释（原注释“C: ipc_send…非阻塞发送”正是 bug 源头）。真机 s13m：PM↔VFS panic 消失、无新 panic/OOM、全服务器 exec 完成、`vm-pf recv` 422→**449**（越过原墙）。新停点见下条
- **新停点（1.8 处置，F14 修复后的层）**：s14b/s14c 两次独立复跑逐字同形——`vm-pf recv`=280 封顶，尾态：**RS(0x102) flags=0x0、runnable=yes、queued=no**（picknone rs_flags=0x0，F10d 同族「清标志未入队」）；pm(0x100)+9 服务器 flags=**0x30（SIGNALED|SIG_PENDING）** 全部 to=VM、runnable=no；init(0x10b) SENDING(0x4) to=PM(0x0)；VM(0x108) 正常 RECEIVING。死点窗口：init 的 exec 缺页循环（fa=0x210b10/0x20fe60/0x219140 逐页服务成功）后 init 阻塞发 PM、PM 停 0x30。下一入口：①谁把 RS 的标志清光却没入队（对位 F10d：找 primitive clear SIGNALED/SIG_PENDING 的站点）；②0x30 的 SIGNALED|SIG_PENDING 是谁给 pm+9 服务器挂的（PM 信号机刚启动即发信号？sig_mgr 配置？）——两问大概率同一根：信号交付链的 rts 协议半缺失。
- **旧停点（阶段 1.7 处置，已修复 F14，保留取证结论）**：s13n/p boot 尾态——tail-dump 遍历**全部非 free 槽共 17 进程**（打印值为 `p_nr+256`，即 0xfb–0x10b = p_nr −5..+11，非”高 nr 窗口”）：**10 个 `0x400`(PAGEFAULT)、5 个 `0x2`(PROC_STOP)、2 个 `0x8`(RECEIVING)、1 个 `0x4`(SENDING)**，**全部 runnable=no queued=no**（调度器无任何可跑候选，`picknone rs_flags=0x8`）；两周期快照字节一致。**s13o 决定性裁决（同镜像 150s vs 60s）= 硬 livelock 非”慢”**：150s 与 60s 字节级一致（10291 行 / vm-pf recv **449** 封顶 / 最后一条 vm-pf 在 10242 行 / rc=0 / 无 panic），多给的 90s 只多出周期 tail-dump 输出（时钟中断仍活），**零新事件**——系统在第 449 轮缺页后完全停止推进。根因与修复见 1.7/1.8 节。
  - **根因已定位（1.7 插桩实锤，本轮完成）**：给 tail-dump 加进程名 + 阻塞边（`to=p_sendto_e`/`from=p_getfrom_e`）后，配合 `MAX_NR_TASKS=1023` 解出 **ANY=0x7c00、NONE=0x7bff**，尾态完整可读：
    - 那 **5 个 PROC_STOP = 内核 task（asyncm/idle/clock/system/kernel，p_nr −5..−1），本就该停（惰性调用）——开局首疑(a) “高 nr 用户进程被停”是 **误读**（nr+256 偏移），已排除。**
    - **10 个 PAGEFAULT 进程全部 `to=0x8` = Endpoint::VM(8)**——它们都卡在等 VM 解缺页；**VM（p_nr 8）处于 `RECEIVING`、`from=0x7c00` = ANY**（即 VM 正空等任意来源）。矛盾点：VM 在 receive(ANY)、却有 10 个缺页请求标着 to=VM 且永不 rendezvous。
    - **定位结论（不过度声称）**：死锁在**内核 PAGEFAULT→VM 投递/唤醒腿**——缺页进程置了 PAGEFAULT+`p_sendto_e=VM` 但未真正入 VM 的 caller 队列/未唤醒正处于 receive(ANY) 的 VM（注：vm-pf recv 能到 449 说明该腿早期可用，是**第 ~450 次之后某条件使其停止投递/唤醒**）。与 F11（VM 请求链唤醒）、已登记 **P1-ipc**（`clear_ipc_refs` 裸 clear 绕过 rts_unset 入队）同一家族。
    - **下一阶段入口（交接手 agent）**：（i）比对 PAGEFAULT 腿与 VMREQUEST(0x800) 腿的内核投递代码路径——缺页腿是否漏了 `vm_enqueue_and_notify_vm` 或等价的入队+唤醒？（ii）查 `vm-pf recv` 449 次后 VM 是否真回到 receive(ANY)（本例尾态显示在），若是则问题在“新缺页请求没入 VM 队列”；（iii）重点核 P1-ipc（syscall.rs L1283 `clear_ipc_refs`）与缺页往返是否重叠——WORKLOG 早标注“1.4 开工优先验证”。探针（tail-dump name/to/from）已入仓保留，接手可直接复跑。
  - `1d25f433e` AP 入口补 EFER.NXE（bit11）+ BSP `enable()` 显式置位 —— err=8 保留位风暴 20+ → 0
  - `967a903e7` 摘除金丝雀探针（它在污染生产上下文）
  - `85a0d7cd8` 四张检测网（全部零命中，见排除账）
- **已排除**（不要再重复排查）：DM 覆盖 / VM-内核树不一致 / 分配器双重分配·归还·底层重用 / EFER.NXE / gdb 硬件观察点路线 / **PTE 条目被抹写形态**（S2c 实证崩溃窗口内监视 VA 的页表条目全程完好、无 refault，那 48 次 lvl1=0 全是正常 lazy 缺页——『统一解释』的第 1 条骨架需按 S2 结论修正：损的是栈数据，不是页表）/ IPC 消息投递站点 `copy_msg_to_user`·viow（S1-S2 对账无直接命中，真凶是同构的 kernel_call_finish DM 直写，见 S2）
- **新登记（S0 顺带发现，暂不修）**：fresh-vars 布局下 `classify()` 产出 free n=0——与 §4.4 第 3 项「跨分配器双记账」候选直接相关，若后续修复涉及 memmap 扣减协议必须一并验证此场景
- **新登记（F10b/F10c 评审发现，阶段 2/3 前必须修）**：
  - **P1-arch**：AArch64 (`trap_dispatch.rs` aarch64 SYSCALL 臂) 和 RISC-V 的 SYSCALL 腿仍用 `reply_code()` 不取负，与 x86_64 负 errno ABI 不一致；阶段 2.1/3.1 开工时必须迁移到 `reply_wire()`
  - **P2-diag**：`dispatch_diagctl` 的内核栈→PA 走 `kern_phys_base + (va - kern_virt_base)` 方式，隐式假定栈在 image span 内；应收敛到 `AddressRef::Process` 统一方案（同形态：`grant.rs`/`syscall_device.rs`/`syscall_signal.rs`/`misc.rs` 等处），另开 todo
- **新登记（F10d 评审发现，同 rts 绕过形态）**：
  - **P1-ipc**：`clear_ipc_refs`（syscall.rs L1283）裸 `p_rts_flags.clear(SENDING|RECEIVING)` 绕过 C `clear_ipc`=RTS_UNSET 的入队半——被唤醒进程跃迁 runnable 却不入队。**与 1.4 VM 缺页循环强相关**（解故障往返若经 IPC clear 会重现 runnable=yes queued=no），1.4 开工优先验证
  - **P1-trace**：`do_trace`（misc.rs L1494-1531）对 PROC_STOP 裸 set/clear 绕过 rts_set/rts_unset（T_STOP 缺出队、T_DETACH/T_RESUME/T_STEP/T_SYSCALL 缺入队）；仅 trace 场景触发，不在 boot 关键路径，待专修
- **新登记（F11 评审发现，commit d9fc1f649，无 P0）**：
  - **P1-guard**：双入链在 release 配置零检测器——C `vm_suspend` 的 `assert(!RTS_VMREQUEST)`（proc.c:241）在 minix3 生产内核里是活的（不定义 NDEBUG），而 Rust 对应守卫只有 `suspend_for_vm` 的 `debug_assert!`（真机跑的 release 不编译），`VmRequestQueue::enqueue`（vm.rs:794）连 debug_assert 都无——同类故障复发仍会是静默死锁（本 bug 就是证据）。建议：在 `vm_enqueue_and_notify_vm` 入链前加与 C 同响度的运行期守卫（链极短，O(n) 遍历可忽略）防重复入链
  - **P2-smp**：SYSCALL 腿存帧发生在 `kernel_call`（含 finish 入链+notify）返回**之后**（与 C 入口存帧相反），单核 `-smp 1` 下靠“唯一 pick 点在 scheduler_loop”无争用，但 **SMP 打开后（schedule_migrate_proc 可改 `p_sched.cpu`）存在未存帧就被 AP 恢复的窗口**；修法=把 `save_frame_to_context`+`trap_style` 前移到 `kernel_call` 调用**之前**（与 int33 腿 trap_dispatch.rs:1413-1446 同形），窗口结构上消失；需一轮真机回归。不阻塞当前单核 bring-up
- **下一步**：**1.5 内核 OOM hunt**（新 frontier）：1.4 双入链死锁已解（见下条「已修复 F11」+ 1.4b 节）。s13h/s13i 真机：VM 请求链恢复单入链→通知闭环，boot 大幅推进（VM 服务 324 次缺页 `vm-pf recv`、exec mfs/init 完成、PM privctl 往返、无 `picknone` 死锁）。**新停点 = 运行期堆分配失败（1536B，1.5a 已裁决归因见下条）**：`library/alloc/src/alloc.rs:566: memory allocation of 1536 bytes failed`（alloc crate 的 `handle_alloc_error`，非项目自有文件行号），发生在一次 `kdst copy pa=0x3ffc78 len=0x77`（rip=0x2258a0）之后，紧跟恰第 **324** 次 `vm-pf recv`。**旁路观察者的二分已验证（无需重建：s13f/s13g 即 pre-F11 基线，6e51b723a，F11 是唯一代码差）**：pre-F11 在 **171** 次缺页往返后死锁（picknone）无 OOM；post-F11 达 **324** 次往返、死锁消除、**然后** OOM。→ 排除“F11 在同一位置引入回退”（pre-F11 根本未到该处）；也非“固定堆池不够”静态墙（死锁冻在 171 < 耗尽的 324）。**主假设改为：`vm-pf` 服务路径每轮往返的累积堆增长/泄漏**（F11 只是解锁死锁让循环多跑 ~2× 把它暴露）。下一步（1.5）：在内核堆加 free-bytes 采样探针，看是否随缺页计数单调下降（定漏 vs 池小），并查每轮 vm-pf 往返里谁申请 1536B 且未释放。~~1.4 主假设“VM 自身页未映射→delivermsg 挂起 VM 自己”~~ → **1.4b `ven` 探针证伪**：挂起类型全是 KernelCall(st=1) 非 DeliverMsg(st=2)、全部 vm=0（VM 未被挂起、正常 RECEIVING）；真因是同一挂起被双入链（见 1.4b 节）。P1-ipc/P1-trace 同 rts 绕过形态仍待阶段 2/3 前专修（与本轮双入链不同机制）；`kernel_call_resume`（无调用方）登记 task1-close 死代码裁决
- **1.5a/1.5b 堆模型裁决（本轮完成，回答了旁路观察的"泄漏 vs 池小"歧义）**：运行期 `alloc` 落点有两块候选堆，都不是"物理内存不够"：
  - **内核 = `ImageBump`**（`os/kernel-image/src/main.rs:196-232`）：**128 KiB**（`IMAGE_HEAP_LEN`，注释自陈"只兜链接面与最小启动期，非运行面配额承诺"）+ **`dealloc` 空转纯 bump 从不回收**（注释："真回收面随 NK1 波次的堆设计落地"）。任何内核运行期 `alloc` 都单调耗它——属结构必然，非 F11 引入。内核运行期 alloc 站点极少（`kmess.rs:183` 仅读 /dev/klog 时 ~10KB；`vm_handoff.rs` 全在 boot）。
  - **VM 服务器（324 轮 `vm-pf` 的实际服务者）= minix-rt slab**（`os/libs/minix-rt/src/alloc.rs`）：2 MiB `.bss` 池，但**硬上限早于池字节触发**——`MAX_SLABS=64`（跨 9 个 size class 共享）、`free_pages` 栈仅 64 槽（满了静默丢页，`alloc.rs:315-324`）、`supply_pages` 连续大块只 bump 前进永不复用空闲栈（`alloc.rs:301-303`）、`free()` 只把 slot 压回本 slab 内部链、**空 slab 页从不回收归还 supplier**（`alloc.rs:560-580`）。1536B → class 2048（每页 2 slot），64 slab 摊到全类极易触顶。
  - **1536B ≠ `vm-pf bytes` 探针的 `format!`**（那些每次 ~40B 走 class 64）——是真实 collection 走 class 2048（候选：`page_cache.rs` 的 `Vec<LruNode>` / `evict: Vec` 增长等）。F11 解锁死锁让该服务路径跑到 324 轮，把"运行期在 slab 上增长的 collection + 64-slab 无回收上限"暴露为 OOM——完全落在旁路观察第一分支"新代码路径首次真正跑深"，所谓"泄漏"是 bump/无回收上限的固有属性。
  - **仍待 1.5c 运行时探针定夺**：精确分配站点、以及 OOM 落在内核 `ImageBump` 还是 VM 服务器 slab（现仅凭交织控制台无法 100% 归因）——探针须带**实体标签**打印 free-bytes/slab-in-use 随缺页计数曲线（单调下降=真增长；平台后突刺=碎片化触顶）。
- **阻塞/风险**：无阻塞；风险 = 探针采样饥饿（cap 被启动期重复事件吃光，见 prompt 铁律 2）与布局每轮漂移（禁止跨轮硬编码物理地址）

---

## 路线图（精简；完整版见 `NK4C-OPENING-PROMPT.md` §6）

| 阶段 | 内容 | 判据 |
|------|------|------|
| 1.2 | Task C：RS 越过 step2 → init_fresh → RS main | `-smp 1` 两次复跑都出现 `rs-epslot self=0x7fffffffc800`（活值） |
| **1.3** | **rc marker 链**（sh 域最小版进 imgrd，`init` exec `/bin/sh`） | 两次复跑出现 `minix-rs rc: minimal boot script marker` ← **x86_64 翻绿闸门** |
| 1.4 | F10 errno 全仓对账（P0-wire） | 每处判别测试 |
| 1.5 | P2 命令面（echo/ls/cat）+ smoke 扩展 | 两次复跑 |
| 1.6 | F3 W^X（boot-shim 段表 → 身份窗口 RX/RW 拆分） | 宿主测试 + 真机 |
| 1.7 | C 腿 ABI 对账清单（test12 前置） | 清单定稿 |
| 2.1-2.4 | aarch64：M3.4 B 案 → KernelUserCopy 丙案 → U-mode trap 腿 → **M3.6 aarch64 rc marker** | 同 1.3 判据（aarch64） |
| 3.1-3.4 | riscv64：甲案（kernel-image 接 DTB）→ SUM 丙案 → trap 腿 → **M4.5 riscv64 rc marker** | 同 1.3 判据（riscv64） |
| 4A-4D | 测试上机：C 腿基建（LP64 头/陷阱桩/crt0/clang 胶水）→ 领域梯子 W1-W13 → Rust 腿核心域 guest 化 | 每波 × 三架构脚本 |
| 5 | 收尾：E5 真机半点亮 / **task1-close 探针大裁决（删所有 `nk4a:` 探针）** / 全账本销账 | 终目标三条全绿 |

**保持登记不开工**：E5-SMP/NK6 X-8、E5(h)+E-DMWIRE+C-17、C-26、C-21/C-22、test82（外网）。

---

## 交接来源（上一个 agent，2026-09-23，起点 commit 56d6dec4c）

### 根因骨架（当前最强解释）

1. **PTE 在物理层消失**：内核侧 pf 层级 dump 显示故障时 `lvl1=0`（PTE 不存在），而 `lvl2`（PD 项）稳定——不是"没填过"，是**填过又被抹**。
   - ⚠️ dump 语义：`lvl2pa` 是 **PD 页**，`lvl1pa` 才是 **PT 页**（早前看错一级白跑一轮）。
2. **抹写窗口 = RS 停车→唤醒之间**（此时只有内核在跑，VM 在 receive 上睡着）。
3. **统一解释**：RS 的 asynsend 表就在 `self` 指针同一 VA（`0x7fffffffc800`，栈上，也是栈 PT 覆盖的最后一页）。**栈页被抹 → 从栈重装 self 得 0 → `endpoint_slot(self=0)` → 访问 VA 0 → SIGSEGV**。这把"PTE 消失"与"self=0"合成一条链。
4. 交付时序实证（`serial_c31c`，原文）：
   ```
   nk4a: pre-restore- rip=0x0000000000203bf0 rsp=0x00007fffffff9d88 r10s=0x0000000000007bff rbx=0x0000000000000000
   nk4a: i33-save rip=0x0000000000203c2d rbx=0x00007fffffff9d28 rsp=0x00007fffffff9d18
   nk4a: rs-epslot self=0x0 bep=0x0 slot=0
   nk4a: pf-exit noaddr cr2=0x0
   cause_sig: sig manager 2 gets lethal signal 11 for itself
   ```

### 布局观测（每轮漂移，仅供理解量级，**不可硬编码**）

| 量 | 观测值（多轮） |
|----|----------------|
| RS 页表根 | `0x35fd000` / `0x5e0f000`（两种形态交替） |
| RS 文本 PT 页 | `0x1c08000` / `0x1c07000`（-smp 1 内也漂） |
| RS 栈 PT 页 | `0x1c05000` / `0x1c04000` |
| self / asynsend 表 VA | `0x7fffffffc800`（用户 VA，跨轮稳定） |
| asynsend 首指令 | `0x203bf0`（用户 VA，跨轮稳定） |
| `endpoint_slot` | `0x216900`（用户 VA，跨轮稳定） |

**用户态 VA 跨轮稳定；物理地址每轮变**——对账必须用同一轮日志。

### 探针存量（都在 `#[cfg(not(feature = "mock"))]` 门下，task1-close 统一裁决删除）

| 探针 | 位置 | 作用 |
|------|------|------|
| `dm-cov` / `dm-mem` / `dm-bump` / `dm-mod` | `os/kernel/src/dm_coverage.rs` | VM DM 窗口三源候选 |
| `pf-save` / `pf#`（含 `cr3=` / `lvl4..lvl1` / `lvl1pa`） | `os/kernel/src/trap_dispatch.rs` | 故障现场 + 层级 dump |
| `i33-save` | `os/kernel/src/trap_dispatch.rs` | int-33 入口保存点 |
| `kdst` | `os/kernel/src/vm.rs` | cross_space 三个写核心的目标 PA |
| `msgw` | `os/kernel/src/ipc.rs` | `copy_msg_to_user` 的 (va, root, pa) |
| `sas-send` | `os/servers/vm/src/vm_server.rs` | SetAddrSpace 发送值 |
| `ptalloc-DUP` / `alloc-reuse-PT` / `ptfree-PT` + `PT_SEEN` 位图 | `os/servers/vm/src/alloc_page.rs` | 分配器三侧检测 |
| `vmpt2bf` / `pte-wb-FAIL` | `os/servers/vm/src/cow_exec_pf.rs` | VM 填页 + 写后回读 |
| `rs-step2` / `rs-epslot` / `rs-anom` | RS 自身（`os/servers/rs/src/`） | RS 侧 self/slot 值 |

### 顺带发现的功能缺口（**记录，不要现在修**）

`os/kernel/src/ipc.rs` 的 `impl UserCopy for KernelUserCopy` 中：
- `read_senda_entry` 恒返回 `Err(CopyError::PageFault)`（≈506 行）
- `write_senda_result` 是空操作（≈515 行）

即**生产路径下 SENDA（asynsend）不投递任何消息**。RS 的 asynsend 正是崩溃点的调用。若 Task C 修复后 rc marker 仍不通，这是首要功能缺口候选。

### 已证伪的旧假设（省得重走）

- 「PT 页不在 VM 分配清单」→ 探针 cap-64 采样伪影（每进程 `map_kernel` 消耗 ~640 PT 页）
- 「金丝雀证明保存后被内核改写」→ 金丝雀自己就是污染源，已摘除
- 「gdb 观察点能抓写入者」→ QEMU gdbstub 对目标 VA 触发不可靠

---

## 记录（按时间顺序追加；每节模板见下）

## S0 环境自检 + Task C 崩溃复现（2026-09-23，commit 3abed3cbf）

### 目标

构建 x86_64 可启动镜像，`-smp 1` 复现 Task C 崩溃（两轮独立复跑），记录本轮布局基准。

### 做法（可复制的命令）

```bash
# 构建（宿主，docker 缺 uefi target）
cd /home/xzhao/github/minix-rs/os && ulimit -v 3145728 && cargo run -q -p xtask -- image --arch x86_64 --release
# 复现（关键：vars.fd 用仓库累积副本，不用全新 OVMF VARS）
mkdir -p /tmp/nk4a
cp /home/xzhao/github/minix-rs/tmp/nk4a/vars.fd /tmp/nk4a/vars_run.fd
cd /home/xzhao/github/minix-rs/os && timeout 150 qemu-system-x86_64 -smp 1 \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=/tmp/nk4a/vars_run.fd \
  -drive file=target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:/tmp/nk4a/serial_<标签>.log -display none -no-reboot -device isa-debug-exit
```

环境自检全过：docker OK（minix-ci 可用）、cargo 1.94.1、QEMU 8.2.2、OVMF 4M 对在位。

### 原始数据

**第一轮坑（fresh vars，serial_s0a/s0b 两轮签名一致但不是 Task C）**：全新 `OVMF_VARS_4M.fd` 下 EFI 模块装载落点改变（reserved-big base=5eb5000/6d2e000/77ff000，conv=14），内核 `vm_handoff free n=0x0 deducted=0x17` → VM 在 `servers/vm/src/boot.rs:157: BootParams: no free memory regions` assert panic → RS 挂 `BOOTINHIBIT|VMINHIBIT`（picknone rs_flags=0x10200）→ 调度器 idle 循环 livelock（vs 采样器 400 tick 同一 kernel rip）。**这本身是一个布局敏感的鲁棒性 bug，已登记（见新登记）**。

**改用仓库 `tmp/nk4a/vars.fd` 副本后（serial_s0c / serial_s0d 两轮独立复跑，签名一致）**。下面贴 s0c 死亡窗口全序列（从 `rs-epslot` 活值到最后，仅省略与因果无关的 kdst/probe 行，行序保持日志原序）：

```
kernel: vm_handoff free n=0x7 deducted=0x16   （boot-shim: memmaps conv=13 reserved=121）
nk4a: rs-step2 pt=0x2237d8 len=12 tbl=0x7fffffffc800
nk4a: rs-epslot self=0x7fffffffc800 bep=0x7fffffffc800 slot=2   ← 活值（step2 期）
nk4a: rs-epslot self=0x7fffffffc800 bep=0x7fffffffc800 slot=8   ← 最后一次活值
── 抹写窗口（无任何 pick：RS 未让出 CPU，内核正替它办事）──
nk4a: rs-anom pf2 n=0x28 rbx=0x0000000000000000 rip=0x0000000000203bf0   ← PF 入口 ctx 的 rbx 已=0，栈页+文本页均已损
nk4a: pick->0x0000000000000008                                          ← 损伤确认后才轮到 VM 填页
nk4a: sa0-0x0000000000000008 root=0x0x0000000005e27000 cur=0x0x00000000035fd000
nk4a: vm-pf recv
nk4a: vmpt2bf off=0x2bf0 ptroot=0x35fd000                               ← VM 填 asynsend 页
nk4a: vm-pf bytes 000000000048c783  fa=0x203bf0 fa8=4883ec6889f04c8d
nk4a: pick->0x0000000000000002                                          ← 切回 RS
nk4a: pre-restore- rip=0x0000000000203bf0 rsp=0x00007fffffff9d88 r10s=0x0000000000007bff rbx=0x0000000000000000
nk4a: i33-save rip=0x0000000000203c2d rbx=0x00007fffffff9d28 rsp=0x00007fffffff9d18
nk4a: rs-step2 ep=0x0 slot=0                                            ← 栈上 self 已是 0
nk4a: rs-epslot self=0x0 bep=0x0 slot=0
nk4a: pf-exit noaddr cr2=0x0
cause_sig: sig manager 2 gets lethal signal 11 for itself
kernel panic: panicked at kernel/src/syscall_signal.rs:300:13
```

（s0d 同窗口行序一致，仅部分计数器值不同；全量日志在本地 `/tmp/nk4a/serial_s0c.log`、`serial_s0d.log`，仓库 `tmp/nk4a/*.log` 被 gitignore，关键内容以上述摘录为准。）

本轮布局基准（s0c=s0d 同构，两轮 cr3 一致）：RS root=`0x35fd000`（历史两形态之一）、VM root=`0x5e27000`、故障 lvl1pa（PT 页）=`0x1c08000`×42 + `0x1c05000`×3、lvl2pa（PD 页）=`0x35b6000`×44 + `0x1c06000`×3。用户 VA 全部跨轮稳定（`0x7fffffffc800` / `0x203bf0`）。

### 结论

- **Task C 按交接签名稳定复现**（需 repo vars.fd 环境，判据满足：两轮同签名）。
- 抹写窗口收窄（与交接描述不同，本轮实证）：`rs-epslot` 活值（slot=8）→ `rs-anom pf2`（PF 入口 ctx rbx=0，栈与 asynsend 文本页均已损）之间**没有任何进程切换**（无 pick 行）——抹写发生在 RS 持续持有 CPU 期间，即**替 RS 执行系统调用/页故障处理的内核代码**（或 VM 醒来服务本次 PF 的内核代执行段）。VM 的 `vmpt2bf` 填页发生在损伤确认之后，不是嫌疑窗口内动作。这把 §4.4 第 1 项（内核直写用户 VA 站点）的优先级再抬高一级，且提示新线索：**内核在 RS 上下文里的 PF/IPC 处理路径自身就是嫌疑人**。另注意更早的 `rs-anom rst n=27 rbx=0`（rip=0x2266e0 恢复点）说明同类损伤在窗口前已间歇出现，计数器 27→2b 连续，值得回溯 rs-anom 探针语义。
- fresh-vars livelock 是独立的可复现环境敏感性，佐证 §4.4 第 3 项方向（VM pool 扣减与 memmap 的交互在别的布局下会把 free 清单切光）。

### 下一步

S1：穷举内核直写用户 VA 站点（§4.4 第 1 项候选清单：syscall_signal.rs sigframe 写 / kerninfo / ps_strings / diagctl / syscall_copy.rs vumap 系列），逐站点加 §7.1 (va, root, pa) 探针（带去重）；S2 真机对账。

---

## S1 直写站点穷举 + 探针（2026-09-23，commit 见 git log）

### 目标

按 §4.4 第 1 项穷举 `os/kernel/src/` 里所有「内核拿用户 VA 直写」的站点，逐站点加 (va, root, pa) 三元组去重探针（`nk4a: w-<site>`，实现 `trap_dispatch::nk4a_user_write_probe`，CAP=96），另在 `kernel_call_finish` 直写循环前后加 finw/fina 观测；真机对账找抹写者。

### 站点清单（探针已挂）

- `syscall.rs kernel_call_finish`：errno 回执 DM 直写（finw/fina，本次主嫌，已实锤）
- `pte_walk.rs copy_to_user`：SYS_VDEVIO/SYS_SDEVIO 结果回写（viow）；VIRCOPY/SAFECOPYTO 走 `cross_space_copy`，由存量 kdst 探针覆盖
- `syscall_signal.rs`：sigframe 搭建写用户栈
- kerninfo / ps_strings / diagctl 写回点

### 收尾验证（均过）

- docker `cargo test -p minix-arch -p minix-kernel -p minix-vm`：242/809/526 全绿 0 failed（基线同前）
- rustfmt：零新增差异点判据（HEAD 本体不过，探针区按期望归位后净少 1 个差异点）
- clippy：生产形态（`--no-default-features --target x86_64-unknown-uefi`）告警集合与 HEAD 一致
- 顺带修了一个 HEAD 存量破损：`vm.rs` kdst 探针三个调用点的 cfg 门与定义门不一致（`not(test)` vs `not(feature="mock")`），宿主 mock 构建必炸 E0425，已统一为 `not(feature="mock")`

### 下一步

S2 真机复跑对账。

---

## S2 哨兵四代迭代 → 根因锁定（2026-09-23，serial_s2a…s2h）

### 目标

S2a/S2b：两轮独立复跑 + finw/viow 对账。无直接命中（写目标全在正常业务缓冲区）。转入哨兵路线：直接盯被抹的 `self` 存放槽。

### 哨兵演化链（每代被前一代的阴性结果重新定向）

1. **s2c 盯监视 VA 的 PTE 条目**：崩溃窗口内条目全程完好、无 refault → 「PTE 抹写」形态证伪（那 48 次 lvl1=0 全是正常 lazy 缺页），损伤 = **栈数据抹写**
2. **s2d 盯表首字**：`0x7fffffffc800` 是 asynsend 表本体（被 RS 活跃翻动），self 的存放槽在另一栈页
3. **s2e 按值扫描**（`nk4a_pte_watch` 重写版：对 RS 栈三页 `0x7fffffff9000/a000/c000` 扫「值==表地址」的 u64 槽，打包状态变化即打 `pw-<site>`）：**决定性**——`pw-x33 s9=0x1de0`（int33 入口 1 命中@0x9de0）→ `pw-fina s9=0x0fff`（0 命中），抹写锁定在**单次 int33 的内核代执行窗口**（用户栈不动，唯一写者=内核）
4. **s2f/s2g/s2h 现场打印**：fx（直写逐 chunk 打 va/pa/len/w0+尾字 t56/t64，cap 48→4096，48 条会在启动期耗尽——教训：判别窗口需无条件打印+足够 cap）、x33in（int33 入口打 call/r2/旧 p_delivermsg_vir/senda 标志）、pdmv-set（两个存值站点打新旧值）

### 原始数据（s2h 死亡窗口，行序保持；fx 行为节录——省略 `pa=` 字段并缩写十六进制，全量原文见本地 `/tmp/nk4a/serial_s2h.log`）

```
nk4a: pdmv-set krn m_user=0x00007fffffff9da8 old=0x00007fffffff9d88   ← 最后一次存值：同步 kernel_call 存 0x9da8
nk4a: pick->0x0000000000000008 / pre-restore VM / vm-pf recv          ← 调用被 VM 缺页处理停住，RS 未 close
nk4a: pick->0x0000000000000002
nk4a: pre-restore- rip=0x0000000000203bf0 rsp=0x00007fffffff9d88 r10s=0x0000000000007bff rbx=0x0000000000000000   ← RS 被恢复回用户态（原调用帧死亡）
nk4a: pw-x33 n=0x30 s9=0x0000000000001de0                              ← RS 重进 int33：self@0x9de0 尚活
nk4a: x33in call=0x10 r2=0x00007fffffff9d28 old=0x00007fffffff9da8 senda=0x1   ← 本次是 SENDA（不更新 delivermsg，与 C 一致），旧值仍在表里
nk4a: fx va=0x00007fffffff9da8 len=0x50 w0=0xfffffffe t56=0x0 t64=0x7fffffffe138   ← kernel_call_finish 补完成旧调用，80B 直写死帧地址
nk4a: pw-fina n=0x31 s9=0x0000000000000fff                            ← 命中清零：self 被本次写的 +56 处零抹掉
nk4a: pdmv-set krn m_user=0x00007fffffffa258 old=0x00007fffffff9da8    ← 抹写后 RS 才发起下一次调用
rs-step2 ep=0x0 slot=0 → rs-epslot self=0x0 → SIGSEGV → panic syscall_signal.rs:300
```

### 结论（根因链）

1. **抹写者实锤**：`kernel_call_finish` 对陈旧 `p_delivermsg_vir`（0x9da8，存于已弹出的同步调用帧）的 80 字节 DM 直写；写入内容 = errno 回执（m_type=-2），其 +56 处零字正落在 self 槽（0x9de0 = buf+0x38）。
2. **与 C 的双重分叉**：①时机——C 的 VMSUSPEND 停车调用者阻塞在 RTS_VMREQUEST 不返回用户态（system.c:61-69），栈帧必活；minix-rs 停车后把 RS 恢复回用户态继续事件循环，帧死。②长度——C `copy_msg_to_user` 钉死 64B（klib.S:284）；Rust 写 `size_of::<Message>()`=80B（LP64 加宽，minix-types 测试 `test_message_total_size_pinned` 钉死）。两者叠加把陈旧指针的危害从「写回旧缓冲」放大成「踩死复用区」。
3. **SENDA 自身无罪**：`x33in senda=1` 实证窗口那次 int33 不碰 delivermsg（与 C mini_senda 一致），它只是把旧炸弹带进了完成时机。
4. 未收敛：停车后恢复 RS 的具体路径（pre-restore rip=0x203bf0 那条是哪条唤醒语义）与旧调用的补完成站点——S3 代码定位。
   > **S3 修正（2026-09-23）**：本节“补完成旧调用”的归因不准确。逐行对照 s2i 全量日志（见 S3 节）证实：崩溃窗口的 80 字节直写不是停住旧调用的延迟补完成（stage 3a），而是**当前 int33 SENDA 调用的同步完成**；s2h/s2i 窗口里那次“停车→恢复”是正常的需求分页服务回路。真正的分叉只有一条：门纪律（见 S3 节根因）。

### 下一步

S3：定位上述两处代码 → 甲/乙/丙多方案对比（对齐 C 阻塞语义 / 完成时失效陈旧 delivermsg / 写长边界）→ 修复单独 commit → 两次复跑判据（rs-epslot 活值 + 越过 step2）。

---

## S3 根因修复：int33 陷阱腿门纪律（2026-09-23）

### 目标

把 S2 的两个未收敛点在代码里定位到行，多方案对比后修复根因，真机两次复跑达判据。

### 定位过程（读码 + s2i/s3a 全量窗口逐行对照）

1. 读完调度循环 stage 3a（`os/kernel/src/lib.rs` L3716-3824：KCALL_RESUME 消费 → `vm::kernel_call_resume` → `kernel_call_dispatch_inner` → `kernel_call_finish_holding_bkl` → 完成臂 `set_ipc_return_code`+`clear_vm_suspend` 后正常 restore）与 `vm.rs` 的 `memreq_reply`（VM 回复后置 KCALL_RESUME、清 VMREQUEST）：停车调用者的恢复链路本身合规（停排期间不会被选回用户态，s2h 看到的“停车→恢复”是正常需求分页服务回路）。
2. 关键修正来自 s2i 全量日志的配对节奏：SYSCALL 腿的正常完成永远是「`pdmv-set` 紧接同址 `fx`」成对出现（L4201-4227 连续多对，地址轮转 0x9d88/0x9da8/0xa258/0x9bf0，全部新鲜）；而崩溃窗口（下列原文，`pa=` 字段省略）里抹写的 `fx` 紧跟在 `x33in` 之后、**没有任何同轮 `pdmv-set`**——写者不是停住调用的延迟补完成，而是**当前 int33 SENDA 调用的同步 finish**：
   ```
   nk4a: pdmv-set krn m_user=0x00007fffffff9da8 old=0x00007fffffff9d88   ← SYSCALL 腿调用存值
   nk4a: fx va=0x00007fffffff9da8 ... t64=0x000000000026c000                ← 该调用同步完成，回执写同一地址（合法，帧活）
   ……… RS 回用户态，帧弹出，0x9da8 区域被后续帧复用 ………
   nk4a: x33in call=0x0000000000000010 r2=0x00007fffffff9d28 old=0x00007fffffff9da8 senda=0x1   ← int33 SENDA（按 C 对位不刷新 delivermsg）
   nk4a: i33-save rip=0x0000000000203c2d rbx=0x00007fffffff9d28 rsp=0x00007fffffff9d18
   nk4a: fx va=0x00007fffffff9da8 ... t56=0x0000000000000000                ← SENDA 的同步 errno 回执落写陈旧地址
   nk4a: pw-fina s9=0x0000000000000fff                                     ← self@0x9de0 被 +56 处零字抹掉
   nk4a: rs-epslot self=0x0 ... → SIGSEGV
   ```
3. 代码坐实：`trap_dispatch.rs` int33 腿（`x86_ipc_dispatch_body`）对**所有** IPC 调用（含 SENDA）统一走 `dispatch_ipc_entry` + `kernel_call_finish`，后者的 errno 臂无条件 eager 直写 `p_delivermsg_vir`。对照 C：`copy_msg_to_user(p_delivermsg_vir)` 只在 `kernel_call()` 腿（system.c:83，入口 system.c:141 必刷新）；int33 陷阱腿（proc.c `mini_send`/`mini_receive`/`mini_senda`）状态经 h_errno/寄存器返回，从不写调用者消息缓冲。另注意：即使写长是 C 的 64B，抹写点 buf+56 仍在范围内——**长度不是本 bug 的本质，陈旧才是**（方案丙因此降级，见下）。

### 多方案对比

| 方案 | 内容 | 判定 |
|------|------|------|
| 甲（选） | 门纪律：int33 腿的 finish 不做 eager 回执直写；IPC 腿挂起的调用被 stage 3a 补完成时同样跳过（门标记随挂起上下文保存） | 精确对齐 C「copy 只在 system.c 腿」；errno 本就经 RAX 交付（int33 出口已有），直写是重复交付；封死所有 int33 陈旧可达形态（含 parked SENDA 恢复轮） |
| 乙 | finish 写前校验 delivermsg 新鲜度（代际计数），陈旧则按 C 的 WARNING+SIGSEGV 路径 | C 无此机制，是给错误复用机器打补丁；且新鲜场景在 int33 腿本就不该写，乙仍多写一次 |
| 丙 | 写长对齐 C 的 64B | 不解决陈旧（buf+56 在 64B 内照样被抹）；且 Rust 用户侧 Message 本就是 80B，截 64 会丢合法字段。否决 |

### 实施（改动点）

- `vm.rs`：`VmSuspendContext` 新增 `resume_skip_eager_reply: bool`（门标记，含 C 对位注释）；`proc.rs` 两个构造函数 + `proc_table.rs`/`misc.rs` 测试构造共 8 处同步补字段
- `syscall.rs`：`kernel_call_finish_holding_bkl` 新增参数 `eager_reply_copy: bool`；新增 `kernel_call_finish_ipc_door`（release_bkl=true、eager=false，带完整 C 对位文档）；VmSuspend 臂在 `!eager_reply_copy` 时粘性置位门标记；errno 回执块用 `result.reply_code().filter(...)` 门控（`eager_reply_copy && !ctx.resume_skip_eager_reply`）
- `trap_dispatch.rs`：int33 腿改调 `kernel_call_finish_ipc_door`
- `lib.rs`：stage 3a 的 `kernel_call_finish_holding_bkl` 调用点从挂起上下文读出 `resume_skip_eager_reply` 后以 `!door_skip` 驱动 eager 参数（评审修复，见下）
- 探针零新增（S1/S2 探针原样保留至 task1-close 裁决）

### 评审与修复（commit a470a8d9c 之后）

CodeReview 子代理评审结论：P0 无发现（门控覆盖全仓三类 finish 调用点，C 对位与借用/并发均成立；`syscall.rs::kernel_call_resume` 无调用方属存量死代码，已登记 task1-close 裁决）。P1 一条真实缺口：重派再次挂起时 `suspend_for_vm` 新建 ctx 不继承门标记，而 stage 3a 硬编码 `eager=true`，多段挂起的 IPC 腿调用会在最终完成时重新落写陈旧地址——修复为 stage 3a 从旧 ctx 读 `resume_skip_eager_reply` 驱动 `!door_skip`（重挂起时 finish 凭 eager=false 在新 ctx 上重新置位）。评审另提及「探针夹带进本 commit」为误判（探针属更早的 a528f0f40，本 commit 仅 135 行修复+文档）。修复后复验：mock 809 全绿、fmt 持平、真机 s3c 复跑判据不变。

### 判据（真机两次复跑，同一镜像）

- **s3a**：`rs-step2` 走完 ep=0x0..0xb 全部 12 个 endpoint（旧行为：slot=0 即崩）；`rs-epslot self=0x7fffffffc800` 全程活值；`grep -c "self=0x0 "` = 0；无 panic/SIGSEGV；QEMU 跑满 150s timeout（旧行为提前 panic 退出）
- **s3b**（不同 vars 副本，布局漂移下重复验证）：签名与 s3a 一致，同样全过
- 新停点：两轮最终都停在 RS 阻塞 int33 receive（`rbxw recv-clear` 后 `picknone rs_flags=0x8`）——旧崩溃点之后的新问题，登记待阶段 1.3 处置
- 验证链：docker `minix-arch/minix-kernel/minix-vm` 242/809/526 全绿；宿主 mock 809 全绿；rustfmt 差异数 syscall.rs=105、trap_dispatch.rs=31 均=HEAD；clippy 生产形态 collapsible_if 5 处全存量（arch/boot.rs、trap_dispatch.rs:1180、misc.rs:1036、lib.rs:2803、lib.rs:3797），too_many_arguments 3 处均存量（新参数未触顶：finish_holding_bkl 恰好 7 参数）；`tools/unsafe-audit.sh --diff` bare=0（本次零新 unsafe）

### 下一步

本单元 commit + code-review；然后阶段 1.3（rc marker 链），新停点（RS receive 阻塞、picknone 全停）的排查并入推进。

---

## 1.3 开局观察：rc marker 链现状盘点（2026-09-23，未 commit 独立代码）

1. **marker 链组件已在位**：`os/etc/rc:10` 就是 marker 打印体（`echo "minix-rs rc: minimal boot script marker"`）；sh 本体（含 pipe2/fork/dup2/waitpid 真执行器）NS12 批三已落（`minix_shell::exec_frame`，三架构 68 bin 总面含 sh/cat/ls）；init 的 runcom 状态机已接（`os/commands/sbin/init/src/driver.rs` L294-302 → `runcom::runcom`）。缺口：`xtask image` 的 imgrd 原型（`generate_etc_proto`，image.rs L426-450）只播 `/etc/{rc,ttys}` + `/dev/console`，**无 /bin/sh**；imgrd 的启动期消费通道归 NS6。
2. **s3c 串口的真实推进度**：RS 逐步 exec 完 12 服务器（`exec … ok` 全列，含 `exec init ok`）后 RS 进 main receive（`rs_flags=0x8`=RECEIVING）——但整轮 `pick->` 只出现过 slot 2（RS）与 8（VM）：**其余 10 个服务器出生后再未被调度**。init 出生时终态快照（bootinh-clear 打印）= `flags=0x8080`（NO_PRIV|NO_QUANTUM）`runnable=no queued=no`；NO_PRIV 的清除通道在 `syscall.rs` privctl 族（L1599-1667），RS 侧 `rs-pm post-privctl` 每 ep 都跑了——出生后期状态无现形探针，无法从旧日志判定 NO_PRIV 是否被清。
3. **1.3 排查入口（下一步）**：给 boot 尾段加一次性状态采样探针（每 slot 的 rts_flags/queued + privctl 调用结果，task1-close 同批删除），裁决「服务器卡 NO_PRIV」还是「已 runnable 但无人发消息后阻塞在 recv」；若是前者，对位 C 的 reincarnate 序（RS `request.c` fresh 完成后 SYS_NEWCAP 投递 + 内核 `priv_put` 清 NO_PRIV 时序）。

## 1.7 449-livelock 取证与根因（2026-09-23，serial_s13o/s14a）

### 目标

定性 449-livelock（PAGEFAULT→VM 投递/唤醒腿死锁）：prompt §4.3 三入口（P1-ipc clear_ipc_refs / 投递路径比对 / 队列入队核查）逐一裁决。

### 做法（可复制）

- 离线对账 s13o（150s 决定性轮）：pick 分布、pre-restore rip 分布、449 个 fa 相位、9 台服务器唯一 pick 上下文。
- 新增 4 探针（全部 cfg(not(feature="mock")) 门 + 过滤/cap，task1-close 裁决删除）：
  - `pfwd`（trap_dispatch.rs `forward_pagefault_to_vm`）：p_nr∈{1,3,4,5,6,7,9,10,11} 的缺页转发三分结果 D/B/E，cap 40；
  - `p3drain`（ipc.rs receive Phase 3）：drain 命中出生服务器的现场，cap 24；
  - `rcvblk`（ipc.rs receive Phase 4）：停车时 caller_q 非空（幽灵/全过滤证据），cap 16；
  - `cir`（syscall.rs `clear_ipc_refs` step 2）：裸清现场（target_ep/nr/清前 flags），cap 24。
- 真机 s14a（探针轮，150s）。

### 原始数据（关键摘录）

s13o pick 分布：`pick->0x0`=270（pm）、`0x2`=183（rs）、`0x8`=452（vm）、**p_nr 1/3/4/5/6/7/9/10/11 各恰 1 次**（出生首切片）。exec 映射：0=pm 1=vfs 2=rs 3=memory 4=sched 5=tty 6=ds 7=mib 8=vm 9=pfs 10=mfs 11=init。

s14a（birth burst，行号 4702-4802）：

```
pick->0x4 → pre-restore rip=0x205bc0（sched exec 入口）→ pfwd nr=4 out=B
pick->0x1 → pre-restore rip=0x23c6a0（vfs）           → pfwd nr=1 out=B
…（9 台同形：出生首指令取指即 fault，VM 忙 → Path B 入 VM caller_q）
pick->0x8 → vm-pf recv → vm-pf bytes fa=0x223de0（RS 的 fault，正常服务）
→ p3drain dst=0x8 snd=4/1/6/5/3/7/9/a/b mt=0xcff   ← 9 条背靠背，中间零 pick、零 dispatch
→ pick->0x0（pm）→ …此后 449-livelock 照旧
```

对账：449 recv = 449 bytes = 0 pf-exit = 0 vm-pf err（VM 收到的每条都成功服务）；449 个 fa 全部唯一、线性推进（RS/PM boot 期 fault + PM 堆逐页增长 0x230000→0x2b4000），**9 台服务器的入口 VA 在 fa 序列中零命中** = 它们的 fault 消息从未被 dispatch。`cir`=0（clear_ipc_refs 本轮未开火——P1-ipc 非本轮机制）；`rcvblk`=0（VM 每次停车时 caller_q 全空——队列被 drain 光）。

### 结论（根因链）

1. 9 台服务器出生首指令取指 fault → forward_pagefault_to_vm → VM 忙（正服务 RS/memreq）→ Path B：SENDING|PAGEFAULT 入 VM caller_q（`p_sendto_e=8` 即此刻写入）。
2. VM 的 receive 把 9 条消息**背靠背 drain 光**（Phase 3 每收一条 drain 一条；VM 的用户态循环拿不到新消息就立即再收，直到队列空、正常停车 RECEIVING(ANY)）。
3. **销毁机制**：Phase 3 旧实现只做内核侧 `p_delivermsg` 沉降 + MF_DELIVERMSG，把「消息→用户缓冲」拷贝推迟到接收者下次被挑中（process_misc_flags DELIVERMSG 臂）；而 receive 是接收者自己的陷入（完成后直接 restore、不经 pick），拷贝不会在返回前发生 → 9 次 drain 反复覆盖同一条 `p_delivermsg`，**除最后一条外全部销毁**。
4. 9 台服务器停纯 PAGEFAULT(0x400)+to=VM（SENDING 已被 drain 清、p_sendto_e 是 Path B 残留）——即 s13n/p 尾态矛盾形态的完整解释；PM 最终阻塞 SENDING→VFS(1)、VFS 停 0x400 → picknone 硬 livelock。
5. **C 对位**：C 的 Phase 3（proc.c:1071-1095）在 drain 时同步直拷用户缓冲（m_buff_usr 于 receive 入口存好，proc.c:983），一 drain 恰好交付一条；Path A（直投停车接收者）Rust 的「沉降+pick 时拷贝」成立是因为停车者恢复必经 pick。**只有 Phase 3 这条腿破坏了原子性。**
6. 三入口裁决：入口① P1-ipc `cir`=0 非本轮机制（仍是登记缺陷，另修）；入口② 投递路径比对命中（Phase 3 缺同步拷贝）；入口③ 队列入队正常（`rcvblk`=0 证明入队与 drain 都发生了，是 drain 后销毁）。

### 下一步

F14 修复（单独 commit）：Phase 3 drain 同步拷贝（C proc.c:1071-1095 对位），拷贝失败退回沉降+DELIVERMSG 兜底。

---

## 1.8 F14：Phase 3 drain 同步拷贝（2026-09-23，serial_s14b/s14c）

### 目标

按 1.7 节根因修复 449-livelock：恢复 C「一 drain 一交付」的原子性。

### 方案对比

| 方案 | 内容 | 判定 |
|------|------|------|
| 甲（选） | Phase 3 drain 命中后**同步** `user_copy.copy_msg_to_user(p_delivermsg_vir, sender_msg)`，成功不沉降；失败退回沉降+DELIVERMSG（delivermsg 臂兜底） | 精确对位 C proc.c:1071-1095（m_buff_usr 直拷）；地址空间正确性成立（Phase 3 只运行在接收者自身陷入，current root=接收者）；改动面最小 |
| 乙 | 保持沉降，但接收者已有未消费 DELIVERMSG 时拒绝 drain（返回 Blocked 留队列） | 引入 C 没有的「拒收」语义，队列语义复杂化；且 Phase 0 消费依赖 pick，自陷入路径仍绕过 |
| 丙 | 接收者用消息队列数组替代单条 p_delivermsg | 背离 C 的单缓冲模型（translate 防线/行为对位），改动面大 |

### 实施

`os/kernel/src/ipc.rs` `receive()` Phase 3：drain 后先同步拷贝，`is_err()` 才沉降 `p_delivermsg`+`DELIVERMSG`（C「copy 失败进 delivermsg」同形，proc.c:278-282）。判别测试 `test_receive_caller_q_drain_copies_to_user_buffer_not_deposit`（SyncCopyOk/SyncCopyFault 双 mock 钉死两形态）。

### 验证（两次独立复跑，同镜像不同 vars 副本）

- **s14b / s14c 逐字同形**：`vm-pf recv` 449 → **280** 封顶且停点完全不同；旧死点（9 服务器永停出生 0x400）**消失**——9 服务器跑起来完成 init IPC、进入 SIGNALED|SIG_PENDING(0x30)；init 的 exec 缺页循环逐页服务成功（fa=0x210b10/0x20fe60/0x219140）；无 panic/无 OOM/无 rc marker。
- 修复前 s14a（探针轮）：p3drain×9 后零 dispatch；修复后 p3drain×24 全部伴随正常 dispatch 消费。
- 质量门：docker minix-arch/minix-kernel/minix-vm = **242/811/526** 全绿（kernel 811 = 810 基线 + F14 判别测试）；rustfmt 漂移 ipc.rs=83/trap_dispatch.rs=30/syscall.rs=103 与 HEAD 持平；clippy（mock 形态）告警类别与 HEAD 一致，改动区零新增。

### 结论

449-livelock 闭环。新停点 = RS runnable=yes queued=no + pm/9 服务器 SIGNALED|SIG_PENDING(0x30)（见顶部「新停点（1.8 处置）」），信号交付链的 rts 协议半缺失是首要嫌疑。

### 下一步

1.9：定位「谁 primitive 清 RS 的 SIGNALED/SIG_PENDING 未入队」+「谁给 pm+9 服务器挂 0x30」——对位 C cause_signal/ delivermsg 的 rts_set/rts_unset 协议半。

---

---

## 1.3-F10b-F10c：SYSCALL 腿数据码 + 内核栈地址修复（2026-09-23）

### 目标

修复 s13c 新停点：boot 推进到 RS `do_memory` 循环后，RS 的 VmSuspend 无法被 VM 服务，全系统卡在 NO_PRIV。分析根因并修复。

### 做法

1. **s13c 取证**：tail-dump 探针在 picknone 第 2/4 次空手时触发，打出全表 flags/runnable/queued 快照。观察到：VM RECEIVING=0x8、RS VMREQUEST=0x800，其余 10 服务器全 NO_PRIV|NO_QUANTUM=0x8080。串口日志没有 `do-memory enter` 行 → VM `do_memory` 从未进入 `memreq_get` 循环。

2. **F10b 根因（syscall_leg_wire 取负破坏数据码）**：VM 在 `do_memory` 中调用 `sys_vmctl_memreq_get`（SYSCALL 腿）。内核空队列时返回 `KcallResult::Ok(ENOENT=2)`。F10 取负：`syscall_leg_wire(2)` = -2，用户态 `reply < 0` 拦截 → `Err(-2)` → `do_memory` 立即退出，永远不服务 RS 请求。同理 VMPTYPE_CHECK=1 也被取负。

3. **F10b 修复**：`KcallResult` 新增 `Data(i32)` 变体（专用于正值数据码交付），`reply_wire()` 方法统一三条交付点逻辑：`Ok(code)` → `-code`（错误码取负），`Data(v)` → `v`（原样）。`vmctl_memreq_get`、`GetPdbr` 改为 `Data(...)` 路径。`trap_dispatch.rs`、`lib.rs` 的 frame.rax/set_ipc_return_code 改用 `reply_wire()`。

4. **s13d（F10b 后）**：`do-memory enter` → `memreq target=2 ok=1` → `do-memory done`——VM 服务成功！boot 推进到 privctl SetSys 阶段。但 `pctl req=3 tgt=0 r=0xe(EFAULT)`——SetSys 失败。

5. **F10c 根因（`copy_struct_from_user` 内核栈 virt_to_phys bug）**：`privctl_set_sys` 调用 `copy_struct_from_user(caller, arg_ptr, &mut req, size)` 把 RS 用户空间的 `PrivUpdateRequest` 结构读到内核栈。函数内部 `dst_phys = CurrentDirectMap::virt_to_phys(&req)`。`&req` VA ≈ `0xFFFF8000003FF890`（内核 higher-half 段），但 `virt_to_phys` 只认 `KERNEL_DM_BASE(0xFFFF808000000000)` 以上的 DM 地址，低于此的落入 VM_DM 分支做 `va - 0x80000000` 得垃圾 PA `0xffff7fff803ff890` → 页表访问失败 → EFAULT。

6. **F10c 修复**：`copy_struct_from_user`、`dispatch_getmcontext`、`dispatch_setmcontext` 三处把 `AddressRef::Physical(virt_to_phys(kernel_buf))` 改为 `AddressRef::Process { endpoint: caller_endpt, offset: VirBytes(kernel_buf as u64) }`——用 caller（RS）的 CR3 走真实页表解内核栈 PA（RS 的 PML4[511] 包含内核 higher-half 映射），`cross_space_copy` 得到正确 PA 后通过 `kernel_phys_to_virt` 经 DM 写入同一物理内存。语义等价 C `vircopyf(VMIO_READ, user_ptr, size, &priv)`。

### 原始数据（s13e 串口关键段）

```
nk4a: do-memory enter
nk4a: memreq target=2 start=0x224008 len=0x11 ok=1
nk4a: do-memory done
...
nk4a: pctl req=0x0000000000000003 tgt=0x0000000000000000 r=0x0000000000000000 tf=0x0x0000000000008080   ← SetSys OK, tgt=0 (DS)
nk4a: pctl req=0x0000000000000003 tgt=0x0000000000000004 r=0x0000000000000000 tf=0x0x0000000000008080   ← SetSys OK, tgt=4
...
nk4a: pctl req=0x0000000000000001 tgt=0x0000000000000000 r=0x0000000000000000 tf=0x0x0000000000000000   ← Allow OK, NO_PRIV 已清
nk4a: pctl req=0x0000000000000001 tgt=0x0000000000000004 r=0x0000000000000000 tf=0x0x0000000000000000   ← Allow OK
...
nk4a: picknone rs_flags=0x0x0000000000000008   ← RS RECEIVING
nk4a: tail nr=0x100 flags=0x0 runnable=yes queued=no   ← DS 已 runnable
nk4a: tail nr=0x103 flags=0x0 runnable=yes queued=no   ← sched 已 runnable
nk4a: tail nr=0x108 flags=0x8 runnable=no queued=no    ← VM RECEIVING
nk4a: tail nr=0x10b flags=0x0 runnable=yes queued=no   ← init 已 runnable
```

### 结论

F10b+F10c 彻底修复了 NO_PRIV 停点。s13e 全 4572 行日志：无 panic、无 SIGSEGV，12 服务器全 exec，SetSys 全成功，Allow 全成功 tf=0x0（NO_PRIV 清除）。新停点 = 服务器 runnable=yes queued=no（调度器未捞起），登记待下一步处理。

### 下一步

1. 调查 queued=no：`pick` 函数如何从 `proc_base` 链表捞 runnable 进程，新启动进程是否被加入 `run_qh` 队列；或 `runnable=yes` 是否只是无阻塞标志但没有在 per-CPU queue 中。
2. commit F10b+F10c，对 commit 做 code-review。

---

## 1.3 F10d：privctl 三函数绕过 RTS 宏的 enqueue/dequeue 协议 → runnable=yes queued=no（2026-09-23，serial_s13f）

### 现象（承 s13e）

F10c 后 Allow 全成功（tf=0x0，NO_PRIV 已清），但 s13e 尾态 8 服务器 `runnable=yes queued=no`——无阻塞标志却不被调度器捞起，系统空转。

### 根因

C 的 `RTS_UNSET`/`RTS_SET` 宏（`minix3/minix/kernel/proc.h:206-224`）**不是单纯的标志位读写**，而是携带调度器簿记：

```c
/* Clear flag and enqueue if the process was not runnable but is now. */
#define RTS_UNSET(rp, f) do {                     \
    int rts = (rp)->p_rts_flags;                  \
    (rp)->p_rts_flags &= ~(f);                     \
    if(!rts_f_is_runnable(rts) && proc_is_runnable(rp)) \
        enqueue(rp);   /* ← 最后一道阻塞标志清除时入队 */ \
} while(0)
```

`do_privctl.c` 的 ALLOW（L63）/YIELD（L71-72）/DISALLOW（L78）全部走这两个宏。而 minix-rs 的 `privctl_allow`/`privctl_yield`/`privctl_disallow` 用裸的 `p.p_rts_flags.clear(NO_PRIV)` / `.set(NO_PRIV)`，**丢了这个 enqueue/dequeue 半**：Allow 清掉 NO_PRIV 后进程结构性 runnable，却从未被加入 per-CPU run queue → 调度器 `pick_proc` 扫 `run_q_head` 永远看不到它。

### 已有正确构件（无需新写）

`proc_table.rs` 早已存在 `rts_set`（L436，clear→非 runnable 时 dequeue）与 `rts_unset`（L502，clear→runnable 时 `sched_enqueue`），是 C 两个宏的完整镜像；`vmctl_clear_page_fault`（syscall.rs L2522）就是用 `rts_unset` 关闭缺页环的工作参照，注释 L2504-2505 明确记录了同一 bug 形态："The primitive flag clear alone left the process dequeued forever"。

### 修复

三函数改为：预检用不可变借用（`get`）读标志 + priv 状态，实际清/置标志改调 `proc_table.rts_unset(target_nr, NO_PRIV)` / `rts_set(...)`，让调度器簿记随之发生。`privctl_yield` 的 caller 侧改 `rts_set`（dequeue），target 侧改 `rts_unset`（enqueue），与 C L71-72 逐行对位。

### 真机数据（s13f，4717 行，timeout 150s，无 panic/SIGSEGV）

```
nr=0x100 flags=0x800 (VMREQUEST)  runnable=no queued=no
nr=0x101 flags=0x400 (PAGEFAULT)  runnable=no queued=no
nr=0x102 flags=0x8   (RECEIVING)  runnable=no queued=no   ← RS 空转等消息
nr=0x103..0x107 flags=0x400 (PAGEFAULT)                   ← 服务器已跑起来后停缺页
nr=0x108 flags=0x8   (RECEIVING)
picknone rs_flags=0x8   ← RS 在 RECEIVING 正常等待
do-memory enter 全程 1 次
```

关键：NO_PRIV(0x80) **全部消失**，服务器不再停 `runnable=yes queued=no`，而是运行后进入合法阻塞态（PAGEFAULT/VMREQUEST/RECEIVING）。调度器现在能捞起被 Allow 的服务器 = F10d 修复奏效，boot 实质前进到下一环。

### 结论

F10d 修复 runnable=yes queued=no 停点。新停点（阶段 1.4）= VM 缺页解决循环：多数服务器停 PAGEFAULT 等 VM 解故障，VM 自身停 VMREQUEST，`do-memory` 仅 1 次 → VM 处理一次内存请求后自停，握手未闭环。

### 下一步（1.4）

1. 查 VM 处理一次内存请求后为何自停（VMREQUEST 0x800 是谁给它置的、ClearPageFault 驱动路径）。
2. 查 PAGEFAULT 进程 → VM 解故障 → 回清 PAGEFAULT 的完整往返是否闭合（同 F10d 形态的其他 rts 绕过点？）。

---

## 1.4 开局分析：稳定死锁——VM 自身 VMREQUEST + PAGEFAULT 孤儿（2026-09-23）

### 死锁形态（s13f 尾态，两次 dump 完全一致 = 静滞非 livelock）

```
0xfb-0xff flags=0x2   PROC_STOP   内核任务（正常停）
0x100     flags=0x800 VMREQUEST   ← VM 自身被 VmSuspend，等 VM 服务自己
0x101/0x103-0x107/0x109-0x10b  0x400 PAGEFAULT  ← 9 进程等 VM 解故障
0x102/0x108 flags=0x8 RECEIVING  ← RS 等消息
全程：memreq-notify 仅 1 次、do-memory enter 仅 1 次
```

### 机制与假设（待下一轮真机探针裁定）

0. **两条独立机制（本轮静态厘清）**：缺页腿 `forward_pagefault_to_vm`（trap_dispatch.rs L1320）=`rts_set(PAGEFAULT)` + `mini_send(VM_PAGEFAULT → VM)`（C `pagefault()` exception.c:112-129），走 VM **主 receive 循环**；而内核调用 VmSuspend（拷贝等）走 `vm_enqueue_and_notify_vm` + SIGKMEM → `do_memory`（MEMREQ_GET）。9 进程 flags=0x400（**纯 PAGEFAULT无 SENDING 位**）= 它们的 VM_PAGEFAULT 消息已**投递给 VM**（非阻塞在发送队列），即 VM 确实收到并开始逐个处理。
1. **notify 守卫正确**：`vm_enqueue_and_notify_vm`（proc_table.rs L321）仅 `was_empty`（链空→非空）时 notify VM，忠实 C proc.c:253。`vm_memreq_get`（L223-229）取走时确实从链上摘除。→ 协议本身无丢失。
2. **主疑：VM 解缺页时自suspend（看门人看自己）**：VM 收到 VM_PAGEFAULT 开解时，某内核操作（映射/拷贝）触发一个需 VM 解的 VmSuspend → VM 自己被置 VMREQUEST 入链。一旦 VMREQUEST 挂在 VM（唯一 receive+do_memory 服务者）身上，无人能清它 → 剩余 VM_PAGEFAULT 消息无人再收、已入链的 memreq 无人再服务 → 全系统孤儿。**C 里 VM 解缺页的内核调用不应 VmSuspend 回自身**，需 grep `minix3/servers/vm`（do_pagefault / 映射路径）对位 Rust 是否多绕了一次会自挂起的内核调用。
3. **P1-ipc 关联**：`clear_ipc_refs`（syscall.rs L1283）裸 clear(SENDING|RECEIVING) 绕过 RTS_UNSET 入队半（F10d 同型）。若解故障往返经 IPC clear 唤醒会重现 runnable=yes queued=no。

### 决定性线索（本轮 grep C 实锤）

**C `delivermsg`（proc.c:270-284）**：给进程投递消息时若 `copy_msg_to_user(&p_delivermsg, p_delivermsg_vir)` 失败（目标页未映射），C **不重试而直接** `vm_suspend(rp, rp, rp->p_delivermsg_vir, sizeof(message), VMSTYPE_DELIVERMSG, 1)` —— 把**接收者自己**挂起（VMREQUEST），让 VM 去映射它的消息缓冲再恢复。

→ **C 的隐式不变式：VM 自身的页（尤其 `p_delivermsg_vir` 区/内核栈/用户栈）必须始终全映射**——VM 由内核建地址空间时特殊化、从不按需缺页。否则任何人给 VM 发消息（各进程的 VM_PAGEFAULT、SYSTEM 的 memreq-notify）都在 `delivermsg` 处挂起 VM → VMREQUEST 挂在唯一服务者身上 → 无人能清 = **本死锁的精确形态**。

→ **Rust 侧假设**：VM 的地址空间有页未初始映射（delivermsg 缓冲 / 栈 growth / imgrd 映射未覆盖），导致给 VM 投递消息时走 `vm_suspend` 把 VM 自挂起。下一轮探针：在 VM(0x100) 被置 VMREQUEST 处打印 suspend type（DELIVERMSG/COPY/CHECK）+ 故障地址，并比对 VM 初始页表覆盖 vs C。

### 下一步探针计划（未实施）

1. 在 `vm_enqueue_and_notify_vm` 入口无条件打 nr + was_empty + 链长，看 9 个 PAGEFAULT 是否真的进了链、was_empty 何时假。
2. 在 VM(0x100) 被置 VMREQUEST 的点（kernel_call VmSuspend 臂）打当前 rip/调用栈，定位 VM 哪个内核调用自挂起。
3. 对位 C `servers/vm` 的 do_memory / 自身页表处理，确认 Rust 是否漏了 VM 自服务豁免。

---

## 1.4b F11：VM 请求链双入链自环 → was_empty 恒 false → 稳定死锁（2026-09-23，serial_s13g→s13i）

### 取证：`ven` 探针裁定 1.4 主假设被证伪

在 `vm_enqueue_and_notify_vm` 入口打 nr/endpoint/suspend_type/是否 VM（cap 40），跑 s13g。结果与 1.4 假设不符：

- 挂起类型**全是 KernelCall(st=1)，无一 DeliverMsg(st=2)**——VM 不是被 `delivermsg` 挂起的；
- **全部 vm=0**——VM(ProcNr8) 从未被挂起入链，它正常停在 RECEIVING 等消息；
- tail-dump 编号 = `ProcNr+256`（syscall.rs L2701），故尾态 VMREQUEST(0x800) 的 0x100 = **ProcNr0/endpoint0**（不是 VM=0x108）。

→ “VM 自身页未映射→自挂起”假设推翻。真凶在 ProcNr0 入链却无人服务。

### 根因：同一挂起被双入链，头插两下形自环

s13g 时间线实锤（同一 ProcNr 连续两次 `ven`）：

```
335 kc0x1 caller=0x2         ← ProcNr2 发起内核调用
336 ven nr=0x2 st=1          ← 入链 #1（kernel_call_finish，syscall.rs:3446）
337 memreq-notify o=1        ← was_empty=true（链空→非空），唤醒 VM ✓
339 sys-susp vmreq=y pending=y
340 ven nr=0x2 st=1          ← 入链 #2（trap 快路径，trap_dispatch.rs:1749）——无 notify
...
4675 ven nr=0x0 st=1         ← ProcNr0 入链
4676 sys-susp
4677 ven nr=0x0 st=1         ← 又双入链
4678 picknone                ← 死锁（ProcNr0 入链但全程无 memreq-notify）
```

机制：`VmRequestQueue::enqueue`（vm.rs:794）是头插：`p_next[nr]=head; head=nr`。同一节点连插两次 → `p_next[nr]` 自指成环。`vm_memreq_get`（proc_table.rs:224-228）摘头时 `prev_idx=None` 分支做 `set_head(next)`，而 `next` 正是那个自环节点 → head 落回自环、**永不归 None** → `was_empty` 恒 false → 后续请求者（ProcNr0）入链时 `if was_empty` 不成立→**不再唤醒 VM**，VM 停在 RECEIVING 空等 = 稳定死锁。

### C 对位（唯一归属）

C `vm_suspend`（proc.c:234-258）开头 `assert(!RTS_ISSET(caller, RTS_VMREQUEST))`——同一挂起**只跑一次**（flag+params+入链+notify 一体）。Rust 把它拆为 `suspend_for_vm`（置标志+建 ctx，跑一次）+ `vm_enqueue_and_notify_vm`（入链+出队+notify）。后者的唯一归属是 `kernel_call_finish`（syscall.rs:3446，D-20 引入），而 `kernel_call`（syscall.rs:600）对 VmSuspend **必调 finish**。trap 快路径的那次（迭代 7 `50ee0e470` 在“快路径不走 finish”旧假设下补）是冗余重复；而 `data_copy_vmcheck`（cross_space.rs:122-187）只建挂起上下文（`suspend_for_vm_with_copy`）不入链，入链由调用方完成（参 vm.rs:416-419 `cross_space_copy` 的文档注释“caller 应调 vm_suspend 入链”）→ finish 是本腿设计上的唯一入链点。

### 修复

删除 `trap_dispatch.rs` VmSuspend 臂里冗余的 `dequeue_if_blocked` + `vm_enqueue_and_notify_vm`（出队/入链/notify 已由 finish 承担），trap 臂只保留 SYSCALL 腿特有的上下文保存（`save_frame_to_context` / `trap_style` / `saved_m_user`）。

**存帧顺序说明（不是 C 对位，是执行模型事实）**：本臂的存帧发生在 `kernel_call`（含 finish 入链+notify）返回**之后**，而 C 在 `kernel_call_entry_um/orig` **入口**就 `SAVE_PROCESS_CTX`（mpx.S:306-312）早于 `call kernel_call`——Rust 此腿的存帧顺序实为 C 的**反面**。安全性不来自 C 对位，而来自“全内核唯一 pick 点在 `scheduler_loop`（lib.rs:3566 唯一调 `pick_and_bill`）”：VM 在 finish 里被 `enqueue_if_woken` 置备后，不可能在 trap 腿 `scheduler_loop`（trap_dispatch.rs:1756）之前被调起读该进程上下文（且队列按 `p_sched.cpu` 定主，单核 bring-up 无跨 CPU）——故 `-smp 1` 下 notify 早于存帧无争用。⚠ **SMP 打开后此存帧窗口是真实隐患**（见 P2-3）。

### 真机验证（s13h 带探针 → 确认后删探针 s13i）

```
s13h（带 ven）：ProcNr2 单入链+notify → 服务；ProcNr0 入链→**memreq-notify o=1**→do-memory→memreq target=0 服务 ✓
s13i（删 ven）：ven=0、picknone=0（死锁消除）、vm-pf recv=324（VM 服务 324 次缺页）、do-memory=2、memreq-notify=2
```

boot 从“开局即死锁”推进到“exec mfs/init 完成 + PM privctl 往返 + 324 次缺页服务”，新停点 = 内核堆 `alloc.rs:566 memory allocation of 1536 bytes failed`（阶段 1.5）。

### 质量门

cargo test 810 passed/0 failed；release image 构建通过；fmt 无新增漂移（proc_table.rs 删探针后与 HEAD 字节一致，trap_dispatch.rs 改动区 clean）；clippy 改动区无新警告（trap_dispatch 仅存预存 E0133/dead-fn，均在 531/640/1180/1882，远离改动区）。`ven` 探针已删（task1-close 裁决）。

---

## 1.9 新停点取证：9×SIGSEGV 空指针崩溃 + RS queued=no 未解（2026-09-23，serial_s14d）

### 目标

裁决 F14 后新停点的两个疑点：谁给 pm/9 服务器挂 SIGNALED|SIG_PENDING(0x30)；RS flags=0x0 为何 queued=no。

### 做法

新增 3 探针（cfg 门+cap，task1-close 裁决删除）：`csig`（syscall_signal.rs cause_signal：tgt+sig，cap 24）、`rtsrs`（proc_table.rs rts_set/rts_unset：RS 全轨迹+enq 判定+cpu，cap 各 32）、`schedctl`（syscall.rs dispatch_schedule：caller/tgt/cpu/prio，cap 16）。真机 s14d 一轮。

### 原始数据

```
csig ×9：tgt=0xa,0x9,0x7,0x3,0x5,0x6,0x1,0x4,0x0  全部 sig=0xb（SIGSEGV）
         （= mfs,pfs,mib,memory,tty,ds,vfs,sched,pm —— 与尾态 9 个 0x30 完全吻合）
pf-exit noaddr cr2=0x0 ×8（cap 触顶；VM dispatch_pagefault 的 noaddr 臂）
vm-pf err=0；schedctl=0（全程无 SYS_SCHEDULE）
rtsrs（RS PAGEFAULT 循环）：set 0x400 → unset 0x400 → now=0x0 enq=y cpu=0x0 反复正常，
         末尾 4 次 set 后 unset 被 cap 截断未采
死点前共同形态：9 崩溃者被恢复时 rsp=0x7fffffffaa40、rbx=0x62c00007bff（互不相同 rip
         =0x203a01/0x207461/0x234131/0x202f31/0x219fb1 等，均为各自 stub/代码路径）
birth 进度：birth s3 runtime ok、birth s5 -> main（birth 协议已运行）
```

### 结论

1. **0x30 之谜破案**：SIGNALED|SIG_PENDING 是 VM 对「不可服务缺页」的**正确** SIGSEGV 交付（C pagefaults.c:89-105 对位：noaddr → pf_fail_segv + CLEAR_PAGEFAULT）。内核/VM 无罪。
2. **真正的 bug 上移一层**：pm+8 台服务器在 birth 运行期**空指针解引用**（cr2=0x0 无所属 region）。RS 发的 birth 消息内容、服务器侧 birth 处理、或其共享环境存在零指针源。
3. RS 自身 rts 协议健康（enq=y cpu=0），schedctl=0 排除幽灵 CPU；RS queued=no 的最后跃迁被 cap 截断，未定案。
4. init 阻塞 SENDING→PM、VM RECEIVING 空等均为下游症状。

### 下一步（交接手）

①pf-exit 探针补打崩溃 ep + 该进程保存上下文 rip → 符号化崩溃点；②审 RS birth 消息构造与服务器 birth 处理的空指针源（对照 C RS request.c birth 协议）；③rtsrs unset cap→64 复采 RS 死点前最后跃迁，定案 queued=no。

---

## 1.9b 崩溃点锁定：minix_rt::crt0::rt_birth +0x101（2026-09-23，serial_s14e/s14f）

### 取证

- s14e：pfc 探针 cap 48 被启动期常规 fault 吃光（铁律 2 复发）→ 改过滤 `cr2==0`（fe383c627）。
- s14f：9 个致命现场全部捕获，**全部 err=0x4（用户态读，页不存在）、cr2=0x0**：

| 进程 | 崩溃 rip | 符号 |
|------|---------|------|
| mfs(0xa) | 0x221d71 | minix_rt::crt0::rt_birth |
| pfs(0x9) | 0x205cc1 | 同 |
| mib(0x7) | 0x20a7a1 | 同 |
| memory(0x3) | 0x2033e1 | 同 |
| tty(0x5) | 0x203a01 | 同 |
| ds(0x6) | 0x207461 | 同 |
| vfs(0x1) | 0x234131 | 同 |
| sched(0x4) | 0x202f31 | 同 |
| pm(0x0) | 0x219fb1 | 同 |

- nm 证实：各二进制内崩溃 rip 与 `rt_birth` 起始的偏移**恒为 +0x101**（mfs 0x221c70+0x101、pm 0x219eb0+0x101）——同一指令。

### 结论

**单一根因**：minix-rt 公共出生链 `rt_birth`（os/libs/minix-rt/src/crt0.rs:303）早期（Stage 1 `read_process_strings` / Stage 2 `crate::init` / Stage 3 `initialize_runtime` 内联区，位于 `birth s3 runtime ok` mark 之前）存在一次空指针读。9 进程共用该代码，故一损俱损。VM 的 SIGSEGV 交付与内核信号协议均无罪。

### 下一步（交接手，直接可做）

1. `objdump -d --start-address=<rip-0x40> --stop-address=<rip+0x10>` 任一二进制（如 mfs 0x221d40），看 +0x101 处读的哪个寄存器间接为 0（rdi=ps_strings? 内联的 kerninfo 表指针?）。
2. 读 `crt0.rs` rt_birth 前段 + `read_process_strings` + `crate::init`（`os/libs/minix-rt/src/init.rs`），对照「birth enter 后、s3 mark 前」的路径找 NULL 源。注意：exec 时 ps_str=0x7fffffffefe0 非零（exec-store 探针），但进程真正运行是 F14 之后（出生即 parked、被 drain 后才首跑）——**怀疑恢复出的上下文/寄存器（尤其 rdi）在长期 parked→drain→clear→resume 往返中丢失或被 Phase 3 的 `set_ipc_return_code(sender,OK)` 写 RAX 破坏**（该调用把 OK 写进的是 PF 陷阱帧的 RAX 位）。
3. 修复方向预告：Phase 3 对「发送者是 PAGEFAULT-parked 进程」的 set_ipc_return_code 本就不该写（它的非 IPC 陷阱帧里 RAX 是用户代码现场）——对位 C：`RTS_UNSET(sender, RTS_SENDING)` 不写 retreg（只有 clear_ipc_refs 的 EDEADSRCDST 路径写）。核实并修。

探针存量：pfc（cr2==0 过滤）、csig、rtsrs、schedctl、pfwd、p3drain、rcvblk、cir 全部在仓，task1-close 统一裁决删除。

### 1.9b 补充（objdump 实锤，同 commit）

mfs 0x221d71（=rt_birth+0x101）反汇编：

```
221d5e: mov (%rdx),%r13
221d61: mov 0x10(%rdx),%r12
221d65: movabs $0x233638,%rax      ← 锁地址（进程数据段，各二进制同偏移）
221d6f: mov $0x1,%cl
221d71: xchg %cl,(%rax)            ← 崩溃指令：test-and-set 自旋锁获取
```

= minix-rt `crate::init()`（Stage 2 分配器一次性初始化）的**初始化自旋锁**位于未映射数据页。两个矛盾待下一轮裁：①pfc 打的 cr2=0x0 与 xchg 目标 0x233638 不符（内核 pf.vaddr 来源疑似陈旧——cr2 读取链路本身是候选 bug）；②err=0x4（读）与 xchg RMW（应为写 0x6）不符。**修复候选方向**：exec/loader 未把服务器镜像的 .data/.bss 段登记进 VM region 表（缺页服务应映射数据段而 VM 无 region → noaddr）或 rt 的锁页应预映射。下一步：查 RS exec 的 region 登记范围（text+stack+? 对照 loader 契约）+ 内核 PF 腿 cr2 读取时点。

---

## 总计划（2026-09-24 制定，覆盖至终目标； roadmap 原件见 NK4C-OPENING-PROMPT.md §6）

> 完成判据 = §0.1 三条：①三架构 OS 各出 rc marker；②18-stage 命令在 OS 上可跑（echo/ls/cat 核心）；③minix3 tests/ 已迁移项上机。执行纪律 = code-excellence（每修 ≥2 方案对比 + C 对位锚点 + 非法态封堵清单）+ prompt §5.3 验证纪律 + §9 停止条件。

| 单元 | 内容 | 判据 | 依赖 |
|------|------|------|------|
| **A（1.9c，当前）** | rt_birth 数据页崩溃修复：A1 读 exec region 登记范围（RS exec + kernel dispatch_exec + VM region 表，对照 C exec 段表契约）；A2 裁 cr2 陈旧矛盾（PF 腿 cr2 读取时点）；A3 修+判别测试；A4 两次复跑无 noaddr/9 服务器过 birth | 服务器全部过 rt_birth 进 main；boot 推进 | — |
| **B（1.3 收尾）** | rc marker 闸门：逐停点推进（定性→定位→修→两次复跑）。已知缺口按序：①SENDA stub（read_senda_entry 恒 PageFault/write_senda_result 空操作，C A_RETR/A_INSRT 对位）；②imgrd 缺 /bin/sh（xtask image.rs generate_etc_proto 补播）；③其余现场发现 | 两次复跑出现 `minix-rs rc: minimal boot script marker`（**x86_64 翻绿闸门**） | A |
| **C（1.4）** | aarch64/riscv SYSCALL 腿迁移 reply_wire()（P1-arch），每处判别测试 | 三架构 build 门绿（minix-ci:1.94-arch）+ 测试 | B 过闸门后 |
| **D（债务清偿，随关联阶段）** | P1-ipc clear_ipc_refs→clear_ipc（C system.c:601-605）；P1-guard vm_enqueue_and_notify_vm 查重守卫；P1-trace do_trace rts 半；P2-diag kern_phys_base 收敛；P2-smp 存帧前移（SMP 前） | 各自判别测试 + 复跑不回退 | 就近搭车 |
| **E（1.5）** | 命令面 echo/ls/cat smoke | 两次复跑 | B |
| **F（1.6）** | W^X（boot-shim 段表→身份窗口 RX/RW，[ARCH] 评估） | 宿主测试+真机 | B |
| **G（1.7）** | C 腿 ABI 对账清单定稿（rax/r10/rbx/rcx、kerninfo、crt0 handoff） | 清单定稿 | B |
| **H（阶段 2）** | aarch64：M3.4 B 案（设计先行→§9 问用户）→KernelUserCopy 丙案（[ARCH]）→U-mode trap→M3.6 rc marker | aarch64 rc marker ×2 | C/G |
| **I（阶段 3）** | riscv64：甲案 DTB→SUM 丙案→trap→M4.5 rc marker | riscv64 rc marker ×2 | C/G |
| **J（阶段 4）** | 4A C 腿基建→4B W1-W13（每波三架构脚本）→4C Rust 腿 guest 化→4D | 每波 × 三架构判据 | H/I |
| **K（阶段 5）** | E5 半点亮、task1-close 探针大裁决（含本轮 8 探针）、全账本销账、终版评审 | §0.1 三条全绿 | J |

每次过闸门：更新 WORKLOG 顶部 + edge_todo.md §A 对应行 + commit（不碰 AI-chats/daily.todo.md）。

---

## 1.9c 根因收敛：VM 双帧分配器记账分裂（跨分配器双记账实锤）（2026-09-24）

### 证据链

1. mfs readelf 段表：锁地址 0x233638 落在末段 RW [0x22e000,0x434120) 的 **filesz 内**——exec_bootproc（vm_server.rs:772-852）本应遍历全部 PT_LOAD、急切 alloc_pfn+map+拷贝文件字节。**段登记缺失假设证伪**。
2. 故障形态重释：cr2=0、err=4、rip=真函数内 ⇒ **受害页内容被抹成零**，执行零字节 `add [rax],al`（rax 恰 0）⇒ fault at 0。即「页还在 PTE 上、帧内容被别人零填充」= **帧别名/复用**。
3. 崩溃顺序 0xa→0x9→0x7→0x3→0x5→0x6→0x1→0x4→0x0 ≈ **exec 逆序**：PF 路径分配游标自低向高推进，先撞后 exec 模块（高帧号）的急切映射帧；RS（次早 exec，低帧号）按序将最后中招——与 RS=0x0 待崩状态吻合。
4. PA-ALIAS 探针 0 命中 = cap 96 早被 PM 增长的 400+ 帧吃光（铁律 2 再发），非阴性。
5. 对账：**= §4.4 登记未审的「跨分配器双记账」候选**（FIXLOG 迭代 25「PT 页双重分配」理论的正确形态）。

### 根因陈述

VM 内存管理存在**两条分配路径共享同一物理内存但记账不互通**：boot 期 exec_bootproc 用 `ctx.page_alloc`（游标）急切映射 12 模块全段（~7000+ 帧），运行期 handle_pagefault 用 `page_frames` 位图另账分配零页——后者把前者已用帧再发出去并零填充，抹掉受害进程已映射页内容。

### 修复方向（下一轮 A1 收尾）

①审计 `PageFrames` 位图初始化与 boot `page_alloc` 游标的初始占用互认（boot 急切映射后把 [base,cursor) 标记 used，或两条路径合一）；②回归测试：「eager-mapped 帧不得再分配」不变量（对位 FIXLOG 迭代 25 遗留方案）；③真机判据：9 进程不再 SIGSEGV、birth 协议走完、`vm-pf recv` 形态正常。

### 探针修正待办

PA-ALIAS 的 SEEN_PA cap 96 → 扩容或按 (ep,va) 过滤；pfc/rtsrs 已够用。

### 1.9c 补充审计（同日，A1 读码结论）

- `PageFrames`（region/page_state.rs:231）只是 per-PFN 状态表（refcount/IN_CACHE），**非分配器**；PF 路径帧分配实际也走 `VmPageAllocator::alloc_phys`（经 PfnAllocator trait）。双路径共用 ctx.page_alloc 单实例 ✓。
- 剩余双账本候选收窄为：①`crate::global::page_alloc_mut()`（vm_pt_alloc/free 的 PT 页路径，alloc_page.rs:83）是否与 ctx.page_alloc 为**两个 PhysAlloc 实例**盖同一物理内存；②`PhysAlloc` 空闲链初始化（memmap candidates，F6 的 RESERVED_REGION_STORE 耦合处）是否把 boot 已占用帧（12 模块 blob、内核、VM 自身镜像）划进可分配范围。
- **判别探针（s14g 配方）**：pfc 对 cr2==0 事件补打「经进程页表 walk 该 rip 处 8 字节」（照抄 vm-pf bytes 的 CurrentPteWalk 读法，门 ep 集+cap）——若读到 00 00 00 00（`add [rax],al`）⇒ 帧被抹零（走 ①② 分配器审计）；若读到 xchg 真指令 ⇒ 上下文/寄存器腐坏（回 1.9b 的 set_ipc_return_code 对 PF 帧 RAX 污染假设）。

---

## 1.9d F15 修复：队列唤醒完成码按 FROM_KERNEL 门控（2026-09-24，serial_s14h/s14i）

### 根因链（1.9b/c 收口）

rt_birth+0x101 崩溃九进程的机制闭环：服务器因缺页腿 FROM_KERNEL 伪发送 park 在 VM caller_q，Phase 3 drain 的 `set_ipc_return_code(sender, OK)` 把 RAX=0 写进其**用户态陷阱现场**；VM 服务缺页恢复后重试 fault 指令（xchg，锁地址寄存器=rax）→ 读 [0] → cr2=0 → VM noaddr → SIGSEGV。s14g tex 读回 rip 处真指令（帧未抹零，分配器假设排除）+ rax 被清零 ⇒ 定案。

### 修复（两步演进）

1. F15 首版（5c654c959）：全删该写入 → s14h 实测引入回归：PM↔VFS 握手 `NoPerm`（int33 合法阻塞发送者的完成码在 Rust door「Blocked=leave RAX untouched」语义下**必须**由 drain 补写；C 等价性 = C 阻塞 send 在停车臂即写 retreg=OK，proc.c:960）。
2. F15 修订（42676e165）：按 `SENDING_FROM_KERNEL` 门控——真实 IPC 陷入发送者补写 OK；kernel 内部伪发送者（缺页帧）不写。kernel 811 全绿。

### 验证（s14h 首版 + s14i 修订版，各 150s）

- 崩溃全清：pfc/pf-exit noaddr/csig 全 0（修复前 9 进程全灭）。
- boot 大幅推进：s14h/s14i 日志 ~19200 行（前轮 6200），birth 协议多服务器完成（s3 runtime ok / s5 -> main 反复出现），PM↔VFS 同步恢复，无 picknone、无 panic。
- **s14i 复跑 #1 修订版**：0 崩溃 + 0 NoPerm ✓；复跑 #2 待跑（纪律要求两次）。
- **新停点（1.10 待定性）**：PM(0)↔VM(8) 缺页服务循环——PM 在 0x209500/0x2200b0 反复 fault、VM 反复服务（bytes 显示真内容），疑似 served PTE 不持久/TLB flush 缺失（对位 R2 登记 F2「页粒度 unmap/remap 缺 INVL/flush」与 sync_slot_pte 无使用者）。

### 下一步

①s14i 复跑 #2 确认；②定性 PM↔VM fault 循环（VM 侧 dump 该 VA 的 PTE 与 region 槽状态，对照 F2）；③过门后按总计划单元 B 推进 rc marker。

---

## 1.10 定性：PM sendrec 错误报告路径 Debug::fmt 死循环（2026-09-24，serial_s14k）

### 判别过程

- s14j/s14i 尾部 fa 全部唯一（0x22f9b0/0x21a000/0x222d1c/0x21fe00/0x2252a0/0x2200b0/0x209500 各一次）——**同 VA 重复 fault 假设证伪**。
- s14k 420s 长跑：19214 行 vs 150s 轮 19191 行——**270s 零新事件，非慢，真卡死**；无 picknone = 有进程烧 CPU（livelock 非 deadlock）。
- 两轮日志逐字节比对：分歧仅在 boot 布局噪声，最终事件完全一致：
  ```
  vm-pf recv fa=0x209500（服务成功，bytes=真指令）→ pick->0x0 → PM 恢复 rip=0x209500
  → w-finw n=0x21 → 静默 270s（无 tick 打断、无重 pick）
  ```
- 符号化：pm 0x209500 = `<&T as core::fmt::Debug>::fmt`，紧邻前一符号 `KernelIpcTransport::sendrec`（0x2094c0）——**PM sendrec 错误路径在格式化 Debug 值时死循环**（纯计算自旋，无串口输出；疑似腐坏长度 slice / 循环结构的 `{:?}`），其试图上报的 sendrec 真实错误被遮蔽。

### 定性

非缺页问题（1.10 初猜证伪）；F2 flush 缺失假设暂无证据。真停点 = **PM 在 sendrec 错误处理里格式化腐坏 Debug 载荷死循环**。这同时意味着 sendrec 确实出错了——错误本身才是下一个根因。

### 下一步（交接手）

①grep `os/servers/pm/src/ipc/transport.rs` sendrec 错误臂的 format!/`{:?}` 站点，定位被格式化的值与错误来源；②该错误发生在 boot 第 ~1100 次缺页后（PM↔init/fork 交互期），对照 s14j 5990-6010 行上下文找触发 syscall；③修复错误本身后，fmt 死循环作为健壮性问题单独评估（审计 Debug 载荷边界）。

### 1.10 补充（fmt 死循环站点定位）

fmt 自旋 = `init.rs:710-711` VFS 屏障 `sendrec(...).expect("PM: can't sync up with VFS (final barrier)")` 的 Err 分支——`Result::expect` 用 `{:?}` 格式化 `IpcTransportError`（符号 `<&T as Debug>::fmt` 实锤）。两层问题：①**真实错误被遮蔽**（屏障 sendrec 出错的具体 errno 未知；候选 = VFS 未进 receive 时 sendrec 立即失败而非阻塞等待——对照 F13 的 send_blocking 教训）；②expect/panic 的 Debug 格式化在腐坏/未知 errno 载荷上自旋（健壮性债：错误类型 Debug 对未知值应封闭）。**下一轮**：临时把 expect 消息改为只打 errno 数值（或先 `map_err` 取 `trap.0` 打整数再 panic）复跑一轮拿真实错误码，再修 barrier 语义（对照 C main.c:231-236 阻塞屏障）。

### 1.10 补充二（s14l：init.rs:711 假设证伪）

errno 暴露版（init.rs 屏障 expect 改 panic! 打 errno 数值）复跑：仍停 1141 recvs、新 panic 行未出现 ⇒ **自旋的 Debug::fmt 不在 711 站点**，PM 在进入 711 之前已在某 `{:?}` 格式化中自旋（候选：panic handler 对 PanicInfo 的格式化、audit_log! 的 Debug 参数、其他错误臂）。init.rs 的 errno 暴露加固保留（封闭格式化面仍有价值）。**下一轮**：①kernel 侧对「PM 恢复后 N 秒静默」打 PM 保存上下文完整栈回溯（stacktrace 探针已有基建）拿到 fmt 的调用链 rip；②沿调用链找具体 `{:?}` 站点与被格式化的腐坏值。

---

## 1.10c 覆盖性根因：timer 中断早期死亡（2026-09-24，serial_s14m）

### 判别

pmstall 探针（tick 臂内 PMST_TICK 计数 + tick>5000 且 current==PM 时打栈回溯，cap 2）420s 全程**零输出**。结合 rbxw irq-save 仅在早期出现（s14m 13 次，最后一条在 ~5021 行，ep=0x8 rip=0x22d24d=VM receive 空闲点）：**timer IRQ 在 boot ~5000 行（birth-burst 相位）后彻底停止**；其后 ~14000 行全部无 tick 运行。

### 定性

- PM 的 fmt 自旋是**受害者**：tick 死后无任何中断能打断用户态自旋（无抢占、无 picknone、无 tail-dump——全部自洽）。
- 1.10 的 PM↔VM fault 循环、fmt 死循环、乃至更早的多个「静默死锁」形态，都可能只是这一覆盖性根因的不同投影：**任何 CPU 忙循环（哪怕几十毫秒）在 tick 死后都变成永久垄断**。
- 候选死因：①tick 臂 hook 链（clock task 唤醒路径）某分支 mask 了 IRQ 行未恢复 / EOI 缺失 → PIC 锁死；②`save_irq_frame_to_context` 在特定 interrupted 状态（kernel-origin tick）下破坏现场后异常返回；③PIT 编程被某处重编程关闭。tick 死亡的精确时刻可探：PMST_TICK 每 1000 次打 tick 计数+current ep（cap 20），死亡点前后事件对齐。

### 下一步（交接手）

①s14n：tick 臂加每-1000-tick 打点（tick 计数+current ep），定位 tick 停止的精确事件；②审计 tick 臂 hook 链（irq_manager dispatch → clock hooks）的 mask/EOI 配对与错误路径；③对照 C clock.c 的 tick 处理（任务唤醒 vs 内联记账）查 minix-rs 懒任务模型的 tick 消费缺口。

### 1.10c 补充二（s14n 定案）：timer 从未正常工作

`tick k=` 每-1000-打点零输出 ⇒ 150s 全程 tick 臂（clock stub → local_tick 臂）调用 **< 1000 次**（大概率 ≈0）。rbxw irq-save 的 13-21 次事件走的是**通用 IRQ 臂**（line 722 save_irq_frame_to_context），非 clock 臂。⇒ **PIT/clock 线从未接到 clock 臂（或 PIT 未编程/被 mask）**——整个系统历来靠 trap 驱动调度（fault+IPC）运行；任何用户态自旋即刻永久垄断。这是覆盖历史全部「静默死锁/picknone」形态的底层 bring-up 缺口，优先级高于 1.10 表象本身。

**下一轮（最高优先）**：审计 x86 timer bring-up——①PIT(i8254) channel 0 编程点是否存在/频率正确；②PIC IRQ0 unmask 与 vector 路由（clock stub 绑定）；③`irq_manager` TIMER_IRQ hook 链注册；④对照 C i8254.c/clock.c 与 minix-rs boot-shim/kernel 的初始化分工。修好后：tick 驱动抢占恢复 → PM fmt 自旋会被打断（或至少可 dump 栈）→ 按 1.10 原路继续。

### 1.10c 补充三（A1 静态审计完成）：IRQ0 落在通用臂，quantum 抢占被孤儿化

- IDT 门齐全：trap_entry.rs:241-247 注册 0x50-0x57/0x70-0x77（PIC 全线）；0xF0/0xF1 IPI/LAPIC timer 亦有。
- 但 dispatch 侧：PIT(IRQ0→vector 0x50) 落**通用 IRQ 臂**（trap_dispatch.rs:722 `irq_of_vector` → `dispatch_hardware_irq(TIMER_IRQ)` → `clock_irq_handler` 软件时钟推进）——该臂**不做 `local_tick`/`check_quantum`**；带 `local_tick`+quantum 抢占的 **clock 臂（702）只由专用 clock stub 进入，实际几乎不被调用**（s14n：<1000 次/150s）。
- ⇒ tick 到达但抢占逻辑被孤儿化：quantum 永不过期 → 单进程用户态自旋即永久垄断 → 全部历史静默死锁形态的总根因。
- **修复（下一轮，最小面）**：通用 IRQ 臂的 TIMER_IRQ 分支补齐 clock 臂语义（`local_tick` + `check_quantum`），或把 0x50 门改绑 clock stub（后者需动 asm 绑定表，面大）。修后判据：tick k= 打点恢复节奏、PM fmt 自旋被打断/可 dump、boot 越过 1.10。

### 1.10c 补充四（s14o/s14q）：PIC bring-up 首轮修复未通，交付链逐环待测

- 已落地（128df8a5b + 本轮）：①通用 IRQ 臂 TIMER 分支补 `local_tick`+`check_quantum`；②plat 层 `pic_init`（ICW 重映射 0x50/0x70+IMR 只放行 IRQ0/级联，boot_init_timer 尾调用）+ IRQ 臂 `pic_eoi`；③`lapic_eoi()` 全局助手接入 TIMER 分支（edge 交付必需回执）。kernel 811 全绿、fmt 零新增。
- **s14p/s14q 实测：`tick k=` 仍零、1141 recvs 不变** ⇒ 交付链 PIT→8259→IOAPIC pin2→LAPIC→0x50 仍有断环。已证：IDT 门在、IOAPIC RTE pin2 已编程 vector 0x50（mask 态，unmask 经 isa_irq_to_pin(0)→pin2 映射正确）、LAPIC SVR 使能。
- **下一轮逐环仪器化**（每环一行探针）：①确认 `pic_init`/`boot_init_timer` 真被调用（入口打点）；②pic_init 后读 IMR 回显；③register_hook 后读 pin2 RTE 回显（mask 位应已清）；④init_ioapic 的 `mask_all`（lib.rs:395 init() 尾）**是否在 register_hook unmask 之后又把 pin2 重新 mask**（初始化次序竞态——init() 与 register_hook 的调用顺序待核！）；⑤PIT 端口写是否真达设备（QEMU 追踪或回读）。

---

## 1.10d 里程碑：timer 交付链修通（2026-09-24，serial_s14r/s14t）

### 逐环仪器化结果（s14r）

- `bit-enter` ×2 / `pit-programmed` ✓——**`boot_init_timer` 此前根本不在活 boot 路径**（唯一调用点在 `#[allow(dead_code)]` 的 `bsp_finish_booting` 分歧路径）；活路径 `kmain→init_clock_and_interrupts`（lib.rs:673）只做 route+mask_all，**D-59 三段序列（PIT 编程/hook 注册/unmask）从未执行**。
- **修复（f2c1b4183 前置 + 本次）**：活路径 `init_clock_and_interrupts()` 后补调 `boot_init_timer()`（lib.rs:675）。
- 回读全对：`rte pin2=0x50`（mask 位已清 ✓）、`pic-imr m=0xfa`（IRQ0+级联放行 ✓）、`pit-programmed` ✓。

### s14t 实锤

- 通用臂 gtick：20 打点（cap）= **≥20000 ticks/150s ≈ 133Hz** —— PIT→8259→IOAPIC pin2→LAPIC→0x50→通用臂 TIMER 分支**全线打通**（128df8a5b+e0fc5f268+lapic_eoi）。
- PM 仍停 1141 recvs：PM 是**唯一可跑者**，tick 抢占后仍只有 PM 可选——自旋继续但不再永久垄断（tick 打断成立）。
- pmstall（clock 臂）零输出 = 仪器化缺口：tick 现走通用臂。

### 下一步（交接手，单步即达）

把 `proc_stacktrace(PM)` 从 clock 臂 pmstall 移入通用臂 gtick 分支（`g>5000 && nr==0 && cap2`，table/tick_section 已在作用域）→ 复跑抓 PM fmt 自旋调用链 → 沿链修 `{:?}` 站点的腐坏值 → 过 1.10 → 单元 B rc marker 冲刺。

---

## 1.10e 终定位：PM panic → exit_via 自旋，panic 原因被吞（2026-09-24，serial_s14u）

### 定案

pmstall2 栈回溯（gtick 分支）：PM 自旋 PC = **0x21c652 = `minix_sys::pm::exit_via`**（两次采样同 PC，5000+ tick 不动；回溯 pa=0xffff7fff803ffcc8 为 P2-diag 坏换算，仅 PC 可用）。exit_via 结构：发 PM_CALL_EXIT 给 `pm_endpoint()`（= PM 自己）后 `loop { spin_loop() }` 永久自旋（minix-sys/src/pm.rs 设计如此——普通进程语义正确；PM 自杀即自卡）。

⇒ **完整因果链**：PM 在 boot 期某处 **panic** →（早前的 Debug::fmt 自旋 = panic 消息格式化，init.rs 修复后已过）→ exit_via 自旋。**真根因 = PM 的 panic 原因**，被「panic handler 不打印直接 exit 自旋」吞掉。

### 下一步（交接手，两步）

1. **审 minix-rt panic handler**（os/libs/minix-rt/）：确认它在 exit_via 之前把 PanicInfo 的 message+location 打到串口（diagctl）；不打就修——PM 一 panic 消息即现形。
2. PM panic 消息现形后修根因（大概率是 PM↔VFS/VM 协议返回值的语义错配——对照 s14i 5990-6010 行触发上下文），过 1.10 → 单元 B rc marker 冲刺。

注：timer 修复（1.10c/d）是本发现的前置——没有 tick 抢占，pmstall2 栈回溯无法采样。

### 1.10f panic 原因落点：PM receive 连续失败 fail-fast（2026-09-24）

panic 文本虽被双重遮蔽（Stage1 格式化早前自旋 + Stage2 diagctl 未达串口），但 PM run 循环（init.rs:398-418）唯一 fail-fast panic 即答案：**「IPC transport permanently broken: N consecutive receive failures」——PM 的 int33 RECEIVE 连续返回 Err**（非阻塞失败；C 语义下 receive 只有传输损坏才 Err）。

**下一轮配方（两步，仪器化量小）**：①`init.rs` ReceiveFailed 臂加一次性 diagctl 打 `transport` 最后 Err 的 errno 数值（IpcTransportError 已有 `errno()` 方法）+ 计数——一次复跑即得真实错误码；②按错误码对位：EPERM/CallDenied ⇒ PM trap mask/ipc 权限被某路径（recovery DISALLOW？）破坏；EDEADSRCDST ⇒ endpoint 解析；EFAULT ⇒ int33 入口 copy_msg_from_user（PM 的 receive 缓冲页缺失——注意 PM 的 p_delivermsg_vir 语义与 F14 同步拷贝的交互）。**关联疑点**：tick 修通后 CLOCK notify 开始到达 PM（`is_notify` → expire_timers 路径首次真实运行）——失败可能与 notify 处理的交互有关（对照 C main.c:65-71）。

### 1.10g 收口（s14v）：receive-failure 假设证伪；PM panic 消息被静默吞没

- `pm-recv-err` 零输出 + 无 fail-fast panic ⇒ **PM 的 receive 连续失败假设证伪**（run 循环 fail-fast 从未触发）。
- exit_via 自旋 = **panic handler Stage 3**（或 main 返回，但 run()->! 排除）。Stage 2 的 `sys_diagctl_write`（lib.rs:380-383）**静默失败**（`let _ =` 吞错）⇒ panic 消息从未上串口。
- **下一轮（交接手，小改即可）**：①minix-rt panic handler Stage 2 的 diagctl 返回值改为打印失败标记+重试/换通道（port IO 或降级文本）；②在 panic handler 入口打 `info.location()` 的 file:line 原始字节（不经 fmt，直接逐字节 Console::write——fmt 已证不可靠）——一次复跑即得 PM panic 的位置与消息；③按 panic 原因修复后向 rc marker 推进（单元 B）。
- 旁证：PM panic 的上游触发在 s14j log 5990-6010 行上下文（PM↔init/fork 交互期）；tick 修复（1.10c/d）后 PM 的死法从 Debug::fmt 自旋变为 exit_via 自旋——panic 处理在推进，根因临近。

---

## 1.10h 定案与下一步（s14r-t-x）：panic location 被 release 构建剥除

- 分块写（16B chunks）后 `panic-enter` 仍 ×2，但 file/message 行依旧空 ⇒ **release（无 debuginfo）下 `Location::file()` 返回空串、line 无意义**——location 路线在 release 是死路（诊断基建本身工作正常：12B 短串可达，>16B 需分块，均已入仓）。
- 已确认事实：**每轮恰 2 次 panic**（行 7343 与 18550 附近）；panic 后 PM 走 exit_via 自旋；第二次 panic 点附近伴随 PM/进程栈页零填充 fault 群（anon 栈首触，正常）与调度乱序。
- **下一轮（单步）**：`os/Cargo.toml`（或 workspace profile）给 `[profile.release]` 加 `debug = 1`（仅行表，体积代价小）→ 复跑：`panic-enter` 后的 file:line 即真实定位 → 修 PM panic 根因 → 单元 B rc marker。
- 若 panic 消息仍为 nonstr（`{:?}` 载荷），改用 PM 侧 panic 站点清单二分（PM init/fork/exit 路径的唯一 expect/assert 各已核）。

---

## 1.10i 两个 panic 站点锁定（2026-09-24，serial_s14z2）

panic 入口仪器化（零 fmt 依赖：file 原串 16B 分块 + `nk4a: PF ` 前缀 + line 手工十六进制）复跑即得：

| # | 站点 | panic 内容（代码即得） |
|---|------|----------------------|
| 1（早） | `servers/sched/src/main.rs:77`（hex 0x4d） | `sys_setalarm failed: {errno}`——sched 出生期 `init_scheduling` 武装 5s 告警被内核拒绝（C schedule.c:340-341 同样 panic） |
| 2（晚） | `servers/pm/src/init.rs:739`（hex 0x2e3） | VFS 屏障 `assert_eq!(barrier.m_type, 0)`——VFS 对末条 VFS_PM_INIT（endpoint=NONE 标记项）的回复非 OK |

两 panic 后各自 exit_via 自旋（发 PM_EXIT 给自己），连累依赖方停摆。

### 下一步（交接手）

①**sched**：查内核 `dispatch_setalarm`/alarm 臂为何 Err（EPERM 权限？CLOCK notify 链未接？）——对照 C schedule.c:340 与 kernel alarm 基建；注意 tick 修通后 alarm→CLOCK notify→sched 的链路首次真实运行。②**PM/VFS**：VFS 侧 VFS_PM_INIT 处理对末条 NONE 标记项的回复 m_type（对照 C main.c:231-236 与 vfs init 握手）——barrier 回复应为 OK。两修后两次复跑 → rc marker 冲刺（单元 B 余段）。

### 1.10i 收口补充（s15a）

String downcast 未命中（panic=abort 下格式化载荷非 String）；Stage 2 diagctl 连格式化消息也静默失败 ⇒ **诊断通道在 panic 上下文不可用**（独立 bug 登记：diagctl 需查明失败原因并加失败重试/降级）。位置+站点已锁定（1.10i 表），足够开工修复：

1. **sched/src/main.rs:77**：`sys_setalarm` Err。查 kernel `dispatch_setalarm`（syscall_clock.rs:187）EPERM 臂（`caller_has_sys_proc_with_table`——sched 的 priv/SYS_PROC 在 setalarm 时点是否已置）与 alarm 基建（set_alarm_timer→CLOCK notify 链，tick 修通后首跑）。
2. **pm/src/init.rs:739**：VFS 屏障回复 `m_type != 0`。查 VFS 对末条 VFS_PM_INIT（endpoint=NONE）的处理与回复（对照 C main.c:231-236）。

两根因修复 + 两次复跑（判据：两 panic 消失、boot 越过 PM↔VFS/后段）→ rc marker（单元 B）。

### 1.10i 补充（sched 侧定案方向）

`dispatch_setalarm` 仅两种返回：OK / EPERM（两臂：无 SYS_PROC priv、priv_id None）。sched 的 `sys_setalarm failed: {errno}` ⇒ **errno=EPERM(1)**。sched 的 SYS_PROC 在 birth 前已 SetSys（F10 轮 `pctl req=3 tgt=4` 成功）——EPERM 说明**时点问题**：sched 的 setalarm 到达时其 priv 表现与预期不符，或 panic 站点并非首次 setalarm。下一轮：①sched `init_scheduling` 的 Err 臂改 diagctl 先打 errno 数值再 panic（PM pm-recv-err 同款配方）；②若确认 EPERM，对照 C schedule.c:340 的 `sys_setalarm` 权限链查 RS 对 sched 的 priv 时序。

---

## 1.10j 状态板（s15e，本轮终点）

- **SCHED panic 重定位**：站点已移至 `servers/sched/src/server.rs`（run 循环的「IPC transport broken after N consecutive receive failures」）——与 PM 同族的 receive 连续失败 fail-fast；init_scheduling 的 hz/alarm 探针零输出（该二臂非本轮死点）。**「receive 连续失败」成为 SCHED+PM 共同的主根因形态**：int33 RECEIVE 对系统服务器返回 Err。
- **PM/VFS 屏障**：阻塞 send 修复已落地（ad1d3b9a6），s15e 中 PM init.rs panic 仍在（分块 `servers/pm/src/i`+`nit.rs`）——阻塞 send 的回复内容/时序仍未达 PM barrier 缓冲（候选：PM 的 sendrec send 半被 drain 后 receive 半的恢复时序、或 VFS 阻塞 send 与 PM 未进 receive 的互等——单核下 VFS 停则 PM 不跑的互等需按 C sef_receive 时序重核）。
- 1145 recvs、两 panic、无 rc marker（s15e）。所有探针在仓（pfc/pmstall/gtick/csig/rtsrs/schedctl/pfwd/p3drain/rcvblk/cir + sched/pm errno 打点）。

### 下一轮入口（最高优先）

①**收敛「receive 连续失败」**：sched+pm 同族——在 kernel `do_ipc` receive 的错误返回点（ELOCKED/EDEADSRCDST/EPERM/EFAULT 各臂）加 errno+caller 打点，一轮复跑即定位错误类别；②按错误类别修内核接收路径；③VFS 阻塞 send 屏障的互等分析（对照 C main.c:435-436 与 PM vfs_init_sync 的 receive 半时序）。修通后：boot 越过 PM↔VFS/sched 全链 → rc marker 冲刺（单元 B 余段）→ C-K。

---

## 1.10k 错误类别定案：PM receive = ECALLDENIED（权限层拒绝）（2026-09-24，serial_s15f）

- 内核 `dispatch_ipc` Error 返回点打点（ipcerr）实锤：**`caller=0x0(pm) err=ECALLDENIED`**（PM 的 int33 RECEIVE 被权限层拒绝，run 循环连续失败即 fail-fast panic 根源）；另 `caller=0x6(ds) err=EDEADSRCDST ×3`（line ~10693，独立小项）。
- 修通 timer 后 CLOCK notify 首次真实到达 → PM 的 receive 被权限检查拒绝 ⇒ 1.10 表象（PM 死→全链停）的直接机制。

### 下一步（交接手，单点修复）

①读 kernel `do_ipc`/`IpcEngine` 对 RECEIVE 的权限预检（trap mask / `s_ipc_to` / call mask 三选哪个拒绝 PM）——对照 C：receive 不受 `s_ipc_to` 限制（只 SEND 受目的端检查），trap mask 需含 RECEIVE 位；②查 RS 对 PM 的 priv 配置（SetSys 时 `s_ipc_to`/trap mask 是否含 RECEIVE 位或 ANY）——minix-rs 若把 receive 也对 `s_ipc_to` 做了 AND 检查则对位偏差；③修复 + 两次复跑 → 1.10 消除 → rc marker（单元 B）。

### 1.10k 补充（s15f 精读）

- ipcerr 全程仅 4 条：ds EDEADSRCDST×3（line 10693，独立小项）+ **PM ECALLDENIED ×1（19277，恰在 PM panic 前）**。PM 连续失败需 16 次 kernel Err，但 kernel 错误仅 1 条 ⇒ 其余失败是**传输层/入口层**（trap ret<0 但非 dispatch_ipc 错误映射，或 copy_msg_from_user 阶段）。
- PM 第二 panic 站点确认 = `servers/pm/src/init.rs`（barrier assert）；SCHED panic = `servers/sched/src/server.rs`（receive-failure fail-fast）。
- 收敛判断：**两服务器（sched/pm）的 int33 RECEIVE 在特定时点返回 Err 的共同机制** = 权限层（CallDenied 仅 PM 1 次）+ 可能的入口层失败并存；与 tick 修通后 CLOCK notify 首次到达的时序强相关。

### 下一步（不变，聚焦权限层）

①`check_permission`（ipc.rs:2046-2115）Layer 3 trap mask：dump PM 的 `s_trap_mask`/`priv_id` 在 ECALLDENIED 瞬间的值（ipcerr 探针扩展）；②对照 RS SetSys 对 PM 的 mask 配置（是否含 RECEIVE 位）；③C proc.c:552 的 trap mask 语义对位。修后两次复跑 → rc marker。

---

## 1.10l 根因定案：SENDREC 原子性缺口（PM barrier panic 机制）（2026-09-24）

### 机制

PM 的 VFS 屏障 = `sendrec`（单陷阱 send+receive 原子）。minix-rs `engine.sendrec` Blocked 臂：SEND 停车 + `MF_REPLY_PEND`。但 **Phase 3 drain 把 PM 摘下时直接完成其 syscall**（`set_ipc_return_code(PM, OK)` + `record_wake_target(PM)` 唤醒）——sendrec 的 **receive 半被跳过**：PM 提前带 OK 返回、barrier 缓冲仍是旧内容 → `assert_eq!(m_type, 0)` panic（1.10i 站点 2 = pm/src/init.rs:739）→ exit_via 自旋。

C 对位：blocked sendrec 的 send 半被取走时（proc.c:1084-1093），发送者在**自身** mini_sendrec 内 `goto receive` 重新阻塞等 REPLY——syscall 直到回复到达才完成。Rust 的 drain-wake 把两段性打破。

### 修复方向（下一轮首务）

Phase 3 drain 中，对 `REPLY_PEND` 的被摘发送者：**不完成 syscall**——改为转入 receive 半（置 RECEIVING、getfrom=ANY，保持停车、不写 retreg、不唤醒）；VFS 随后的 OK 阻塞 send（1.10j 已改）经 Path A 直投 PM 的 parked receive → PM 带真回复醒来。普通 SEND（无 REPLY_PEND）维持现状（send 成功即完成）。同型审查：`sendrec` 的 Path A 直投臂（1728 行 send Delivered → receive(ANY)）语义不变。

### 状态

sched panic（server.rs receive-failure fail-fast）与 PM panic（SENDREC 缺口）同源于「tick 修通后 CLOCK notify 首次真实到达」触发的协议深水区。本轮仪器化与探针全部在仓；1.10i/j/k/l 全链可接手。

---

## 1.10m 新前沿：SEND 死锁检测误报 ELOCKED（s15g/s15f，2026-09-24）

- ipcerr（s15f）：`PM err=ELOCKED`（PM 的 send 被死锁检测拒绝）+ `ds err=EDEADSRCDST ×3`（早期，独立）。
- PM 的 send→VFS 被 `detect_deadlock(SEND, PM, VFS)` 判死锁——PM↔VFS 互等（VFS receive 等 PM 消息 + PM send 等 VFS receive）**本应互补解析**（C deadlock 对「dst 正 receive 自 src」不是死锁，send 恰是其解），Rust 判定误报 ⇒ PM 的 send 被拒 → 后续链停。
- **下一轮首务**：①对照 C proc.c deadlock() 的 walk（dst 在 RECEIVE 且 getfrom==src ⇒ 非死锁，链在此终止）修 `detect_deadlock` SEND 分支的终止条件；②修后 PM↔VFS 屏障/sched 链应继续推进 → rc marker（单元 B）。
- 附：sched panic（7483）与 PM panic（19310）在 s15g 仍各 1 次；ipcerr 探针（cap 24）已覆盖全部 IPC 错误类别输出。

### 1.10m 补充（静态复核 + 下一轮探针设计）

静态复核：Rust 2-cycle XOR 测试与 C 逐位等价（`(xp_rts ^ (fn<<2)) & SENDING`）；ANY 终止 ✓；chain-end（blocked_on None）✓。**ELOCKED 的实际触发需运行时数据**：下一轮在 `detect_deadlock` 的 2-cycle 分支加一次性 dump（xp_rts/function/group_size/caller，cap 4），即可见 PM send 被拒时 VFS 的实际 rts 与判定路径；同时 ipcerr 全量（cap 提到 64）看 PM ELOCKED 前后的完整 IPC 错误序列。

---

## 1.10n 定案：PM↔sched SEND-SEND 真死锁（协议层）（2026-09-24，serial_s15i）

- dd2 实锤：`caller=0(PM) fn=SEND xp=4(sched) xp_rts=0x4`——PM send→sched 被 ELOCKED，因 **sched 同时 blocked SENDING（向 PM）**：PM↔sched 互发 = SEND-SEND 真死锁。**检测器正确**（C XOR 判定同样报死锁；2-cycle 互补仅覆盖 SEND↔RECEIVE）。1.10m「误报」假设证伪。
- 真问题 = **boot 协议层的 PM↔sched 消息序**：sched 出生期 setalarm 成功后其 balancer 定时（tick 已活）→ sched 发消息给 PM？同时 PM 在向 sched 发？两侧同步 send 互撞。此前 tick 死亡时该死锁不可达（sched 的定时器根本不触发）——**timer 修复暴露了这一协议缺陷**。

### 下一步（交接手）

①dd2 探针扩展：打印该 send 的 m_type/目标消息（kernel 侧 send 时 msg 可得）+ sched 侧被 ELOCKED 后的重试行为；②读 sched 的 balancer 到期处理（server.rs run_once 的 tick 分支）与 PM run_once 中向 sched 发消息的站点，对位 C（sched 的 balancer 消息在 C 是 notify PM？还是 PM 主动向 sched？）；③修协议序（一侧改 async/sendnb 或加 receive 窗口），两次复跑 → rc marker（单元 B）。

### 1.10n 补充（s15i，含 1.10l 修复后）

dd2 实锤（含 1.10l 后仍复现）：`caller=0(PM) fn=SEND(1) xp=4(sched) xp_rts=0x4`——PM 的**普通 SEND**（fn=1 非 SENDREC）与 sched 的 SEND 互撞。即：PM 存在**非 sendrec 的对 sched 阻塞 SEND** 站点（候选：PM 的 SCHEDULING 通知/或 send_blocking 变体），与 sched 的 taskcall 回复 send 互撞 → 双向 SEND 真死锁。

**下一轮（交接手，单点）**：①grep PM 向 SCHED endpoint 的全部 send 站点定位该普通 SEND（候选：sched_ctl taskcall 的实现若是 send 而非 sendrec、或 PM 的 noquantum/sched 通知）；②修一侧为 sendnb/notify（对照 C：sched 的 reply 用 ipc_send 阻塞、PM 的 taskcall 用 ipc_sendrec 阻塞——时序上 PM 的 #N+1 只能在 #N reply 后发出，若 PM 提前发出即站点错误）；③修后两次复跑 → rc marker。

### 1.10n 补充二（s15i 精读 + 站点排查）

- PM 向 sched 的三个站点（sched_start/sched_stop/taskcall）**全部是 sendrec**——无裸 SEND 站点；fn=0x1 的普通 SEND 来源待下一轮 dd2 扩展（打印 send 的 m_type 与 msg 首字）确认（候选：PM 的 sched 相关 notify 经 SEND 语义、或某 reply 路径）。
- s15i：dd2 仅 1 条（PM→sched，sched xp_rts=SENDING 双向互撞）、崩溃/NoPerm 零、1145 recvs——1.10l 修复后系统整体仍稳定推进。
- **下一轮**：dd2 扩展（m_type+msg 首字）→ 定位 PM 侧站点与 sched 侧停发语义 → 按协议修一侧（对照 C：sched reply 用 ipc_send 阻塞、PM taskcall 用 sendrec——PM 不应在 sched reply 在途时发新请求；若发现 minix-rs 侧顺序颠倒即修）。

---

## 1.10o 交付链定案与下一轮配方（s15j，2026-09-24）

- dd2m 实锤：PM 的 send m_type=**0x900（VFS_PM_INIT）**，闭环节点 xp=**sched(4)**，xp_rts=SENDING。即：**PM 的 VFS_PM_INIT 屏障 sendrec 的 walk 经 sched 闭合** —— PM 侧该 send 的 dst 疑似 = sched 的端点！
- **头号嫌疑（下一轮单点核查）**：PM 的 `vfs_endpoint` 参数值。若 params 把 vfs_endpoint 配成了 SCHED 的端点（4），则 PM 的全部 VFS_PM_INIT/屏障流量错投 sched：sched 的 receive(ANY) 吸收这些消息后按 SchedMsg 解码失败/自旋，PM 的 barrier reply 永不到达 ⇒ assert(m_type==0) panic ✓ 与全部观测吻合（VFS 侧从未见过 0x900、PM 反复 panic、exit_via 自旋）。
- **核查点**：①`os/servers/pm/src/init.rs` 的 `self.params.vfs_endpoint` 来源（boot 参数/RS 传入）与实际值；②对照 `exec endpt=` 序列（vfs=1）与 RS boot 镜像里的 endpoint 分配；③若确认错配 → 修 vfs_endpoint 来源（RS boot 参数/PM params 构造）。
- 修后判据不变：两次复跑无 `pm/init.rs:739` panic、boot 越过 PM↔VFS → rc marker（单元 B）→ 单元 C-K。

---

## 1.10p 根因闭合：PM 的 vfs_endpoint=4（sched）——启动参数错配（2026-09-24，serial_s15l）

### dd2m 终版实锤

`dd2m caller=0x0 dst=0x4 cmt=0x900(VFS_PM_INIT) xp=0x4 xmt=0x1` —— **PM 把 VFS_PM_INIT 发给了 endpoint 4（sched）**，walk 闭环节点=sched 自洽。全部历史矛盾（PM↔sched SEND-SEND 死锁、PM barrier panic、VFS 侧 0x900 消息缺失、sched 收到无法解码的消息后 receive-failure fail-fast）由此一条错配全部解释。

### 根因链

PM `real_main` → `BootParams::acquire_from(&DirectKernelCallTransport)`（内核 GETINFO 启动参数）→ 运行时 `vfs_endpoint=4`（sched 的端点，正确应为 VFS=1）⇒ PM 全部 VFS_PM_INIT/屏障流量错投 sched：sched receive(ANY) 吸收无法解码的消息 → 协议断裂 → receive 连续失败 fail-fast panic → exit_via 自旋；PM 屏障 reply 永不到达 → m_type≠0 panic；连累 PM↔VFS 全链与依赖方停摆。

### 下一轮修复配方（交接手）

①审计 `BootParams::acquire_from` 的端点解析（os/libs/minix-rt 或 pm 侧 boot params 结构）：vfs_endpoint 字段的来源行/偏移——对照内核 GETINFO 侧填充（kernel image table 的 endpoint 字段序）；②重点核 GET_HZ 同表的字段错位族（一次错位往往连坏多字段）；③修后判据：两次复跑 PM 不 panic、PM↔VFS 屏障过、`pm-recv-err`/`dd2m` 零输出、boot 越过 1145 → rc marker（单元 B）→ 单元 C-K。

### 1.10p 补充（矛盾点登记）

静态核查：`acquire_from`（init.rs:218）硬编码 `vfs_endpoint: Endpoint::VFS(1)` ✓；`Endpoint::VFS=1`、`Endpoint::SCHED=4` 常量正确。但 dd2m 实测 dst=4 ⇒ **运行时存在第二条到 ep4 的 0x900 发送路径，或 params 在 new/init 中被覆写**。候选：①`PmServer::new/init` 内重设 vfs_endpoint；②send_blocking 之外的某 send 站点复用了 0x900 消息但目标变量为 4（如 sched_ctl/taskcall 的目标变量被 0x900 消息误传——即 vfs_init_sync 与 sched_ctl 的消息/端点参数交错）；③初始化次序：vfs_init_sync 的 panic 与 sched_ctl 的时序交错。

---

## 1.10p 定案：PM↔sched 协议级阻塞发送交错（ELOCKED）（2026-09-24，serial_s15n）

- elock 实锤：`PM → sched, m_type=0x3` 的阻塞 send 被死锁检测拒绝（sched 同时 SENDING→PM）。
- **机制**：PM 的 m_type=3 消息（回复/通知类）与 sched 的反向 send 交错时，两侧阻塞 send 互等——C minix3 的 PM↔sched 协议用 kernel notify/异步避免此类交错；minix-rs 侧两侧均用阻塞 send。
- **修复方向**：①PM 的 m_type=3 消息发送改 sendnb/异步（或 sched 侧）；②对照 C pm 的 m_type=3 消息语义（回复 or 通知）定协议归属；③修后两次复跑过 1.10 → rc marker（单元 B）。
- 状态：探针族完备（elock/dd2m/dd2/ipcerr/gtick/pmstall2）；1.10 系全部取证在仓。

### 1.10p 补充（elock 定性精确化）

ELOCKED 现场 = **PM 的 `reply(slot, code=3)`**（ipc/vfs.rs:312 `m_type=code` + 阻塞 `transport.send`）→ sched，与 sched 的反向 send 互卡。PM 的 reply 是请求-应答协议的应答半：应答阻塞本应被请求方的 receive 半吸收——互卡说明 **sched 在未收到 #N 应答时已发出下一请求**（应答丢失/时序错位）或 **PM 的应答目标/时序错位**。下一轮：①grep sched 请求 PM 的站点（其应答 m_type=3 的请求语义）与 PM dispatcher result=3 的来源；②对照 C reply 协议（C 同为阻塞 ipc_send 但时序由 sef_receive 互锁保证）定错位侧。

---

## 1.10q 收口状态（s15q，2026-09-24）

- sched sendnb 修复已生效入仓（trait+实现+settle 改非阻塞）；s15q 复跑：形态不变（2 panic-enter、elock PM→sched mt=3、1151 recvs、无 rc marker）。
- dd2m 数据精读修正：`cmt=0x900` 为 PM 的**陈旧 p_sendmsg 缓存**（曾 blocked 的 VFS_PM_INIT Path-B 残留）——提示 **PM 的某条 VFS_PM_INIT send_blocking 曾长期 park 未被 VFS drain**（VFS 握手 receive 与 PM send_blocking 的时序错位）；`xmt=0x1` = sched 在发的消息 m_type=1（其 EPERM 类拒绝回复）。
- **下一轮（交接手，聚焦三点）**：①PM `vfs_init_sync` 的 send_blocking 与 VFS 握手 receive 的逐条对账（哪条 INIT 未被吸收——probe：PM send_blocking 前/后打序号，VFS 侧 receive 计数）；②定位「PM 发 m_type=3 到 sched」的站点（PM 侧 grep 0x3 发送或 elock 探针补 m_source 链）；③修复后两次复跑 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10r 里程碑：系统完整 boot 进入正常 idle（2026-09-24，serial_s15r）

- PM↔VFS 握手**完全通过**：`pmvi k=0..0xb` 12 条 INIT 全部流动 + **`pmvi-barrier-done`**（屏障 OK）。
- 尾态（健康 idle）：**RS/VM/全部服务器 flags=0x8 RECEIVING 正常停车**（对比 1.9 时代 9 台 0x30 崩溃态）；`gtick k=0x3e8`（1000 ticks，timer 稳定）；1143 recvs；无 elock 风暴。
- sched 仍有 1 次 panic-enter（setalarm EPERM 路径，独立问题登记不阻塞 idle）。
- **定性：x86_64 内核+服务器栈已完整 boot 到多用户空闲态**——1.10 系（timer/SENDREC/协议序）修复全部生效。

## rc marker 最后缺口（单元 B 余段，下一轮单点）

**imgrd 缺 /bin/sh**（1.3 开局盘点已知）：`os/xtask/src/image.rs:425 generate_etc_proto` 的 proto 只播 `etc/{rc,ttys}`+`dev/console`。init 的 runcom 状态机已接线（driver.rs:294-302 → runcom::runcom）、rc 脚本（os/etc/rc:10）含 marker echo——**只差把 sh 二进制（os/commands/sh 构建产物）+ bin 目录播种进 proto**：
```
bin d--755 0 0
sh ---755 0 0 <staging>/sh
```
rc 内容补 marker echo（或确认已有）。修后两次复跑判据 `minix-rs rc: minimal boot script marker` —— **单元 B 完成**。

---

## 1.10s /bin/sh 已播种；下一环 = init 侧 rc 触发（s15s，2026-09-24）

- /bin/sh 已入 imgrd（proto 增 bin/sh，xtask 12 测试全绿含新断言）；s15s 形态同前（1143 recvs、1 panic=sched setalarm EPERM 独立项、系统 idle）。
- **下一环定性**：init(11) 停在 receive(ANY) 等 PM——rc 执行链 = init fork/exec /bin/sh /etc/rc 需经 PM fork+exec；init 未主动发起 ⇒ 需审 init 的 runcom 启动臂（driver.rs:294-302 → runcom::runcom）在等待什么（PM 的 spawn 通知？RS_INIT 后的启动事件？）。
- **对照 C**：init.c 主循环读 /etc/ttytab 前 `exec /etc/rc`（init 自己 fork+exec 经 PM）——init 侧应有主动 fork/exec 动作；minix-rs init 的该臂状态待核。

### 下一轮配方

①grep os/commands/sbin/init 的 runcom/start 臂触发条件（等什么消息/状态）；②若 init 等 PM 的 spawn 通道而 PM 侧未发 → 补 PM 侧 init-startup 通知；③/bin/sh 在 imgrd 已就位、PM fork/exec 基建已通（sched taskcall 链已活）——rc marker 只差这一环。

---

## 1.10t 收口与下一步（s15q，2026-09-24）

- s15q（sched sendnb 生效版）：形态不变（2 panic-enter、elock PM mt=3↔sched、1151 recvs、无 rc marker、picknone rs_flags=0x8）。
- **关键未解点（1.10t 收口）**：init(11) 停在 receive(ANY)——其 driver 状态机（Runcom 臂含 runetcrc→fork/exec /bin/sh /etc/rc）**从未到达 Runcom 臂或卡在其内部**。elock 的 PM reply(m_type=3→sched) 与 sched 的反向 send 互卡是该停顿的表象。
- **下一轮配方（单点仪器化，一次复跑）**：在 init 的 `run_transition`/`step` 各 StateKind 入口打 diagctl 打点（`nk4a: init-state <kind>`，每次迁移 cap 全打）——一次复跑即见 init 停在哪个状态、runetcrc 内部卡在哪（open /etc/rc？fork？wait？）。对照 C init.c:986-1012 的 runetcrc 时序修根因。
- 附带登记：sched setalarm EPERM panic（1 panic-enter 之一）独立不阻塞 idle，但 sched 死亡会影响后续调度服务，随根因一并修。

---

## 1.10t-b 精确定位：init 未进入状态机，卡在 main 前段（s15m2，2026-09-24）

- `init-state` 打点零输出 ⇒ **init 的 `run_transition` 从未运行**——init 卡在 main.rs 前段（BootParams::acquire_from / console 探针 / register_handlers / securitylevel / read_file("/etc/passwd") 等早期步骤之一，其 receive(ANY) 停车即在此）。
- 前段每个步骤都可能经 VFS/PM 的阻塞 IPC——任一环节的响应缺失即卡死。对照 C init.c:49-96 的启动步序逐环打点即可定位。
- **下一轮配方**：main.rs 前段每步加 `nk4a: imain <step>` diagctl 打点（BootParams/console_ok/register_handlers/close_std_fds/securitylevel/read_file）→ 一次复跑定位卡死步骤 → 修根因 → Runcom 臂即可达（/bin/sh 已在 imgrd、PM↔VFS 已通）→ rc marker（单元 B 完成）。
- 探针存量：init-state、pmvi、elock、dd2m、ipcerr 等全部在仓；kernel 811 全绿、xtask 12 全绿。

---

## 1.10q 最终定案（s15w，2026-09-24）

elock 补 src 后完整图景：
- `elock caller=0(PM) dst=4(sched) mt=3 src=0` + `dd2m xmt=0x1`。
- **xmt=0x1 = PM_CALL_EXIT(1)**：sched 的 setalarm EPERM panic → panic handler → exit(1) → `exit_via`：sendrec(PM, PM_CALL_EXIT)——sched 的 EXIT send 在途。
- **PM 的 mt=3 阻塞 send 发往 sched 与之互撞** → 双向 SENDING 死锁（检测器正确）。PM 的 m_type=3 发送站点仍未定位（PM 全量 grep 无 m_type=3 构造——值 3 来自运行时变量，候选=dispatcher result/errno 透传）。

### 修复配方（下一轮，交接手）

①PM 侧定位 m_type=3 send：在 PM 的 reply/send 站点加「dst==4 时打 m_type+调用点标记」的编译期探针（PM 侧 grep `reply(` 的全部 code 实参，值 3 者）；②按协议归属改异步/sendnb（PM 应答本应 sendnb——发现运行时仍有阻塞 send 路径即修）；③sched setalarm EPERM 根因（init_scheduling 的 get_hz/setalarm 之一）随探针继续。

### 1.10q 精化（elock mt=3 语义勘定）

elock 的 `mt` 打的是 `msg.m_type`——int33 陷阱入口把**调用号**盖进 m_type：3=SENDREC。⇒ elock 的死锁 send = **PM 的 taskcall（sendrec→sched）的 send 半**，与 sched 在途的上一应答 send 互撞。修复点收敛为：**PM taskcall 的 send 半被 ELOCKED 后，PM 的 init_scheduling/后续流程对该错误的处理路径**（当前：init_scheduling Err → panic → exit 自旋连累全链）。对照 C：sched 的应答用 ipc_sendnb（非阻塞）即不存在互撞——修复=sched settle 应答 sendnb 已落地（687a5bf51），若 ELOCKED 仍现说明互撞对偶的另一侧（PM 的 send 半）时序仍需核（PM 单线程下 taskcall #N+1 只能在 #N reply 后发出——需核 reply 的 Path A 交付时序）。

---

## 1.10t-b2 里程碑（s15w3，2026-09-24）：PM↔sched 互卡全现场捕获

- elock 全量状态版（s15w3）：`elock caller=0(PM) dst=4(sched) mt=3 src=0 d_rts=0x4 d_gf=0x7bff(NONE) d_sto=0x0 c_gf=0x7c00(ANY)`——**PM 与 sched 双向阻塞 SENDING 互撞的全现场**：sched 的反向 send 目标=PM(0)、PM 的 getfrom=ANY（在等任意来源的请求/消息）。
- 系统形态：boot 全链健康（无崩溃、无 0x30 群、全服务器 RECEIVING 停车），仅 PM↔sched 的消息序互卡点未通。
- **下一轮配方（接手即做）**：①dd2m/elock 上下文（前后 ~40 行）取 PM 该 send 的 m_type=3 与 sched 在途 send 的 m_type=1 语义对照（sched 侧 grep reply/code=1 的构造——EPERM 应答 or exit 残留）；②按 C sched 协议修一侧（sched 应答改 sendnb 已落地；若互卡仍在，查 PM 侧对 sched 的阻塞 send 站点——grep PM 全部 `send_blocking`/sendrec 至 ep4 的调用）；③修后两次复跑 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10u 定案：PM↔sched taskcall send 半 ELOCKED（协议互撞完整闭环）（s15x，2026-09-24）

- elock 补 PM 栈回溯实锤：**ELOCKED 现场 = `PmServer::init` 内的 taskcall（sendrec→sched，SCHEDULING 家族）send 半**——PM 处理 init fork 链时对 sched 的 SCHEDULING taskcall，与 sched 在途的上一 taskcall 应答 send 互撞（双向 SENDING，dd2m：cmt=0x900 残留缓存/xmt=0x1=sched 应答 EPERM）。
- elock 的 mt=3 勘定 = **陷阱层盖的调用号**（3=SENDREC），非协议消息号——mt 打点消歧完成。
- **协议缺陷本质**：PM 的 taskcall#N+1 send 半在 sched 应答 #N 仍在途时即发出并被 ELOCKED——应答/请求时序错位一拍。C 同形时序下不发生：C taskcall 应答由 sef_receive 互锁保证（应答 sendnb 或时序互锁）。

### 下一轮修复配方（交接手，单点）

①sched `settle` 的应答 sendnb 已落地（687a5bf51）但 PM taskcall 的 send 半仍 ELOCKED——核 Phase 3 drain 对 REPLY_PEND 发送者的 1.10l 转入 receive 半路径是否在 PM 的 taskcall 线上生效（probe：Phase 3 转入时打 PM 的 rts）；②若确认已生效而 ELOCKED 仍现，则 PM 的 taskcall send 半被拒发生在「sched 应答已 Path-A 交付但 PM 尚未消费」的窗口——修法=PM 的 taskcall ELOCKED 臂改为重试而非 fail-fast panic（对照 C schedule.c:340-341 的行为差异）；③两次复跑过 1.10 → rc marker（单元 B 完成）。

---

## 1.10w 收口（s15w3，2026-09-24）：互卡全现场已捕获，协议修复点明确

- elock 全量状态（s15w3）：`caller=0(PM) dst=4(sched) mt=3 src=0 d_rts=SENDING(0x4) d_gf=NONE d_sto=0(PM) c_gf=ANY`——**PM 与 sched 双向阻塞 SENDING 互撞的全现场**。
- PM 侧该 send 的 m_type=3：**PM 的 WAIT4 协议号（PM_CALL_WAIT4=3）**——PM 在向 sched 发送 WAIT4 语义的消息（候选：PM 的 sig_delay/wait 处理转发），而 sched 同时 SENDING 其应答（m_type=1=EPERM 拒绝应答）。
- **定性**：PM↔sched 的消息序在「PM 发 WAIT4 语义消息 ↔ sched 发 EPERM 应答」处互撞——两侧阻塞 send 即死锁（检测器正确）。

### 下一轮配方（交接手，聚焦三点）

①grep PM 侧全部「向 ep4 发送 m_type=3」的站点：`grep -rn "PM_CALL_WAIT4\|m_type.*=.*3" os/servers/pm/src/` 定位该消息构造点（候选：PM 的 wait.rs 转发、sig_delay_done 应答、或 timer.rs 的 alarm 应答）；②按 C 协议修一侧异步（该类应答改 sendnb，或改 kernel notify）；③修后两次复跑过 1.10 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10w 收口（s15w，2026-09-24）

- elock 补 m_source 字段（src=0x0=PM 自身 ✓ 一致）；elock/dd2m 探针族入仓（687a5bf51）。
- s15w 复跑形态同前（elock caller=PM dst=4 mt=3 单发、1151 recvs、无 rc marker、picknone rs_flags=0x8）。
- **定性保持**：ELOCKED = PM 的 taskcall（SENDREC→sched）send 半，与 sched 在途应答 send 互撞（双向 SENDING）——协议级真死锁，检测器正确。
- **下一轮配方（接手即做，单点）**：①PM 的 taskcall 被 ELOCKED 后 init_scheduling 的 fail-fast 行为对照 C schedule.c:340-341 改重试/延迟语义；②sched 侧应答 sendnb 已落地，核 PM taskcall 的 sendrec 原子性修复（1.10l）是否覆盖 PM taskcall 场景（REPLY_PEND 门控路径）；③修后两次复跑过 1.10 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10w2 状态固化（s15x，2026-09-24）

- elock 补 PM 栈回溯实锤：ELOCKED 现场 PM 侧调用点 = **`PmServer::init` 内的 taskcall（sendrec→sched，SCHEDULING 家族）**；对侧 sched 的 SENDING 为其上一 taskcall 应答在途。
- sched 侧独立 panic：`servers/sched/src/server.rs` 的 receive-failure fail-fast（连续 receive 失败上限）——sched 的 receive(ANY) 连续 Err 的错误类别待一轮 ipcerr 扩展（现 ipcerr cap=24 只打印 4 条即停，需放宽 cap 至 64 并补 caller=6 过滤）。
- PM 栈回溯的坏 DM 换算（pa=0xffff7fff803ff668）为 P2-diag 登记的 kern_phys_base 偏移问题，回溯深度受限不影响 PC 定位。

### 下一轮配方（交接手）

①ipcerr cap 24→64 + 过滤 caller∈{0,4}：一轮定位 PM taskcall ELOCKED 与 sched receive 失败的错误类别；②按类别修（ELOCKED→taskcall 重试臂；EPERM→priv 时序；CallDenied→trap mask）；③修后两次复跑过 1.10 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10y 状态固化（s15w3，2026-09-24）

- elock 全量状态探针落地：`caller=0(PM) dst=4(sched) mt=3 src=0 d_rts=SENDING d_gf=NONE d_sto=PM c_gf=ANY`。
- 1.10l（SENDREC 原子性：REPLY_PEND 发送者 drain 后转 receive 半停车）已生效入仓；VFS 屏障阻塞 send 修复已落地。
- **下一轮配方（交接手，单点）**：elock 瞬间 PM↔sched 互卡的全状态已捕获——PM 的 mt=3 阻塞 send 与 sched 的反向 SENDING。下一轮：①dd2m 探针补 PM 侧该 send 的 m_type 来源（PM 的 p_sendmsg 0x900 残留已核为陈旧缓存）；②定位 PM 侧向 ep4 的阻塞 send 站点（grep PM 全部 send_blocking/sendrec 至 ep4 的调用，重点 sched_ctl taskcall 与 vfs barrier 的参数变量）；③按 C sched 协议修一侧异步/sendnb；④修后两次复跑过 1.10 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10x 定位完成（s15x2，2026-09-24）

- ipcerr 扩量+caller 过滤（cap 64、caller∈{0,4}）一轮即中：**`ipcerr caller=0x0 err=ELOCKED`**（19221 行，仅 1 发）——PM 的 taskcall（SENDREC→sched）send 半被死锁检测拒绝，此时 sched 正 SENDING 其 EPERM 应答（m_type=1，1.10i 定性的 EPERM reply）。
- **互撞对偶完整**：PM taskcall send 半 ↔ sched EPERM 应答 send——双向 SENDING 真死锁（检测器正确）。
- **根因两层**：①sched 的 do_start/do_stop 处理返回 EPERM（应答 m_type=1）——sched 侧拒绝原因待查（`accept`/`sender_from` 校验或内核调用失败）；②EPERM 应答 send 与 PM 的下一 taskcall send 互撞的时序（两侧阻塞 send 交错）。

### 下一轮配方（交接手）

①定位 sched 侧 EPERM 的产生点：sched 的 do_start/do_stop/do_nice 的 `accept`/kernel 调用错误臂加打点（一轮复跑即得具体拒绝原因）；②按拒绝原因修 sched 侧（校验过严或内核调用时序）；③PM taskcall ELOCKED 臂对照 C schedule.c:340-341 改重试（sched 应答 sendnb 已修，重试窗口应极短）；④修后两次复跑过 1.10 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10z 定案：sched balance_queues re-arm setalarm 失败 panic（s15z，2026-09-24）

- PF chunks 定位：sched 的 panic 在 `server.rs` 的 **balance_queues re-arm**（`.expect("sys_setalarm failed (schedule.c:367-368)")`）——**初始武装成功（alarm 触发、balance round 跑了），re-arm 失败** → sched 死亡。
- srcv-err 零输出 ⇒ sched 的 receive 从未 Err——**sched 的 receive-failure fail-fast 假设证伪**；sched 死因单一 = balance re-arm setalarm 失败。
- sched 死后：系统 idle（全服务器 RECEIVING、gtick 稳定）——其余链路健康。

### 下一步（交接手，单点）

①balance_queues 的 re-arm `kernel.setalarm` 失败 errno 定位（Err 臂打 errno 数值，一轮）；②按 errno 修：EPERM ⇒ dispatch_setalarm 的 caller_has_sys_proc_with_table 在 re-arm 时点的 priv 状态；EINVAL ⇒ timeout_ticks 参数；③对照 C schedule.c:367-368（re-arm 失败即 panic——C 同语义，根因在 setalarm 本身失败的原因）。

### 1.10w3 补充（s16a）

- sched 侧用户态 diagctl 打点（srcv-err/sc-rv）**静默失败**（零输出）——sched 的诊断通道不可用（diagctl 失败被 `let _ =` 吞），与 panic-handler Stage 2 同族。⇒ sched 侧 errno 定位必须走**内核侧** ipcerr（已在仓，caller∈{0,4} 过滤 + cap 64）。
- s16a 复跑：`rearmlen` 零输出 ⇒ balance_queues 未返回 Err（balance re-arm 非死因——1.10z 假设修正）；panic-enter ×1 仍在（sched 死因 = server.rs:182 receive-failure fail-fast 或其他 server.rs panic——PF chunks 指向 server.rs）。

### 1.10w4 补充（s16b）

- panic-entry 补 location 行号十六进制打点（`nk4a: LN <8hex>`，lib.rs panic-entry）。s16b 实测：**panic location = sched/src/server.rs:154（0x9a）**——现行源码该行 = sched-alarm Err 探针的 diagctl 块内（1.10 轮加的探针区）。sched 的 panic 发生在探针 diagctl 打点路径上（sys_diagctl_write 自身或其返回处理）——**PM taskcall ELOCKED → init_scheduling 的 Err 臂 → sched-alarm 探针 diagctl → 该路径 panic**。
- 语义链完整：PM taskcall ELOCKED（互撞真死锁）→ sched-alarm Err 臂 → panic ——**双重效应**：ELOCKED 本身（PM↔sched 互撞）是根因，Err 臂的探针/panic 处理放大为 sched 死亡。
- **下一轮**：①查 diagctl 在该上下文 panic 的机制（DirectKernelCallTransport 的 diagctl 错误路径）；②PM taskcall ELOCKED 臂对照 C 改重试/异步消除互撞；③两次复跑过 1.10 → rc marker → C-K。

---

## 1.10x 状态（s16c，2026-09-24）

- taskcall ELOCKED 重试臂已实现并验证轮 s16c：**elock 仍 1 发、1153 recvs、无 rc marker**——重试空转不 yield，sched 不运行 → 互卡持续（livelock）。
- **下一轮配方（交接手）**：①重试间加 yield（内核 yield 系统调用或 door 让出），让 sched 的 receive 半就绪后互卡自解；或②按 C 协议修 sched 应答侧（应答改异步 notify）；③sched setalarm EPERM（sched 死因）与 VFS barrier m_type 仍待修（探针已备）。

### 状态板

系统 boot 全链健康（全服务器 birth 完成、PM↔VFS 屏障过、timer 133Hz）；仅 PM↔sched taskcall ELOCKED 一点未通。探针族 16 种在仓；kernel 811/xtask 12 全绿；daily.todo.md 未触碰。
