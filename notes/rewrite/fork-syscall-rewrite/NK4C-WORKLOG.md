# NK4-C WORKLOG（Task C「清零者」追捕 → 三架构 + 命令面 + 测试上机）

> 本文件是**记忆与交接载体**（git-tracked）。长程任务：每完成一个逻辑单元就更新顶部"当前状态" + 追加一节 + commit。
> **顶部状态必须始终是最新的**——用户会在任意时刻让 agent 收尾，接手者只读它 + `git log --oneline -20` 就要能接续。
> 详细取证历史见 `.review/zcode/edge1/FIXLOG.md` 迭代 27-33（**本地文件、被 gitignore、换工作树会丢**——关键结论在本文件 §交接来源有副本）。

---

## 当前状态（每次 commit 前更新，一屏读完）

- **阶段**：**1.3 rc marker 链（449-livelock 已修复 F14，两次独立真机复跑确认；当前 frontier = 新停点「RS runnable=yes queued=no + pm/9 服务器 SIGNALED|SIG_PENDING(0x30)」）**：历史——S3 ✅ Task C 根因修复（`a470a8d9c`+`6be40748f`）→ F10b/c/d（SYSCALL 腿 errno/内核栈 VA/rts 协议）→ F11（VM 链双入链）→ F12（slab OOM）→ F13（PM↔VFS 握手）→ **F14（本轮，Phase 3 drain 同步拷贝，见 1.7/1.8 节）**：boot 从「449 轮缺页后硬 livelock、9 服务器永停出生」推进到「服务器全部跑起来、init exec 缺页循环、PM 信号机开始工作」，停在新层
- **根因最终版（S3 定位修正 S2 第 4 点未收敛项）**：`kernel_call_finish` 的 eager 回执直写（errno 非零时把 80 字节回执写到进程表的 `p_delivermsg_vir`）在 C 里只存在于 `kernel_call()`/SYSCALL 腿（system.c:83），且该腿每次入口都先刷新 `p_delivermsg_vir`（system.c:141），目标结构性新鲜；C 的 int33 陷阱腿（proc.c `mini_*`）从不执行这条直写（状态经 h_errno/寄存器，真回执经 MF_DELIVERMSG 投递）。minix-rs 把 int33 腿（含 SENDA）统一接进同一 finish 机器而丢了这条**门纪律**：SENDA 入口按 C 对位故意不刷新 `p_delivermsg_vir`（trap_dispatch.rs 的 `!is_senda` 存储臂），于是 SENDA 窗内的同步 errno 回执落写上一次 SYSCALL 腿调用留下的陈旧地址（帧已弹出、区域已复用）→ self 槽被回执零字抹掉 → `endpoint_slot(0)` → SIGSEGV。完整证据链与修正说明见 S3 节
- **修复（方案甲，门纪律）**：新增 `kernel_call_finish_ipc_door`（int33 腿专用，跳 eager 直写，其余簿记不变）+ `VmSuspendContext.resume_skip_eager_reply` 门标记（IPC 腿挂起的调用被 stage 3a 补完成时同样不写）；详见 S3 节
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
