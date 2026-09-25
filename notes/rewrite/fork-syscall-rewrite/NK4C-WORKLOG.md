# NK4-C WORKLOG（Task C「清零者」追捕 → 三架构 + 命令面 + 测试上机）

> 本文件是**记忆与交接载体**（git-tracked）。长程任务：每完成一个逻辑单元就更新顶部"当前状态" + 追加一节 + commit。
> **顶部状态必须始终是最新的**——用户会在任意时刻让 agent 收尾，接手者只读它 + `git log --oneline -20` 就要能接续。
> 详细取证历史见 `.review/zcode/edge1/FIXLOG.md` 迭代 27-33（**本地文件、被 gitignore、换工作树会丢**——关键结论在本文件 §交接来源有副本）。

---

## 当前状态（每次 commit 前更新，一屏读完）

- **B14 已修（1.26 落地，含代码）**：VFS 同步挂载路径 `WireFsClient::send`（`request.rs`）两处缺陷根治——**A**：裸 `m_type=REQ_READSUPER(0xA1C)` 经 `sendrec` 直发，绕过 C `fs_sendrec` 的 `TRNS_ADD_ID`（把 request 号抬到高 16 位）→ MFS 侧 `TransactionId::decode`(call=raw>>16) 解出 call=0、index 下溢 → `Unserved`→回 ENOSYS，read_super 从未抵达 MFS 挂载门；修=`sendrec` 前 `msg.m_type = trns_add_id(REQ_READSUPER, 0)`（同步靠阻塞往返匹配、id=0 安全，CodeReview 独立确认 id=0 < `IS_VFS_FS_TRANSID` 的 0xB02 下界不误路由）。**B**：`decode_readsuper_reply` 旧无视回复状态字无条件返 Ok，把 ENOSYS 当挂载成功让 `do_init_root` 带病开门；修=sendrec 后 `trns_del_id(msg.m_type)!=0` 即 `Err(FsError::Io(status))` 上抛（恢复 C `req_readsuper` `if (r!=OK) return r;`，boot 应 panic 见 main.c:519-520）。**真机取证**：加探针（已删）实测 PFS readsuper 回 `0x00000000`(status0=OK 通过)、MFS 回 `0x001e0000`(status=30=**EROFS**) 正确上抛——**Fix B 无假阴性、Fix A 已让 MFS 真服务**。三件套绿（docker 813/242/528·0fail / fmt request.rs 17=17、main_loop.rs 1=1 零新增 / 两次真机 c1/c2 签名一致=23313 行、`main_loop.rs:7681 failed to initialize root: Io`）。CodeReview 无 MUST-FIX。**boot 现诚实停在下一步 B15。**⚠️ 本 bullet 前代 §1.25 把症状记为 **EBUSY(16)**：本轮实测本 build 根挂载失败真码是 **EROFS(30)**（EBUSY 是「已挂载再挂」旧时相的下游表现），见 §1.26。

- **B15 已修（1.27 落地，含代码）**：MFS 根挂载 dirty-mark 不再 EROFS——`fs/fs-rt/src/source.rs::ImgrdBlockSource` 加**有界 CoW 覆盖层**（`overlay: BTreeMap<u64, Vec<u8>>`）。修向：`read_block` 先查 overlay、miss 落 Static 基座（clip 短尾零填，保持 C `memory.c:442-443` 边界规则）；`write_block` 分 arm——`Owned` 就地写不 populate overlay（测试/future writeback 路径）、`Static` 不再返 EROFS 而是写落 overlay（部分越界存 surviving、整块越界 no-op）；`from_static` 构造点初始化空 overlay；`EROFS` import 删除。**dirty-mount 只写块 0 → overlay 1 条 ≈ block_size + 树节点，内存有界**（vs 8 MB base 无法 Owned 化、2 MB slab pool 装不下）。C 忠实性：memory 驱动自有缓冲的 RAM 盘可写；本 overlay 是 bdev 通道未接前的过渡形态，进程终止即丢=与 C 语义一致。宿主 fs-rt 27/27 pass（新增 4 测：overlay 遮蔽读写 / clip 短尾 / 整块越界 no-op / 多块独立），mfs 133/fs 178/vfs 530 全绿。三件套绿（docker 813/242/528·0fail / fmt source.rs HEAD=1 NEW=1 零新增 / 两次真机 c3/c4 签名一致=25213 行、`init-state Runcom` @24127 同位、无 panic/EROFS/Failed to init）。CodeReview PASSED 无 MUST-FIX（SHOULD-CONSIDER=Owned 热路径多做一次 overlay get 可加注释；NICE-TO-HAVE=image() doc 说明不含 overlay——均非破坏性，本轮保持 diff 最小不采纳）。**boot 现前进到 `init Runcom` 相位**。⚠️ 不选「改根挂载只读」缩窄目标路径（非 C 忠实、破坏单元 D/I 写需求）。

- **B16 部分修（1.30 落地，含代码；仅 Fork/SrvFork 两臂取负）**：候选根因确认——PM `Err(e) => ReplyIntent::Reply(PmError::from(e).to_errno())` 传正 errno、`reply()` 无取负写 `msg.m_type = 正` → init `perform_syscall` 判 `m_type ≥ 0` 走 Ok 分支、将 EAGAIN/ESRCH 当 child_pid → 假成功父无子、waitpid 卡（与 C `_syscall`/F10b `reply_wire()` 契约失配）。修复：`calls.rs` L234-247 的 `PmCall::Fork` 与 `PmCall::SrvFork` `Err(e) => ReplyIntent::Reply(-PmError::from(e).to_errno())`；测试 `test_dispatch_fork_parent_unknown_is_error_reply` 断言同步；`cargo test -p minix-pm --lib` **417/417 pass**。**真机 c8/c9**：两次签名一致（`init-state Runcom` 各 1、`nk4a: pm 0140b` 各 8）；c8 35863 行 vs c7 25013 = **活动 +43%（子/兄弟进程真在跑：`pfwd nr=1..9 out=B` 全 boot 模块页 fault 转发到 VM 服务）**——fork 链已通、系统进入下一停点 B17（rc marker 仍未出）。

- **B17 已修（1.34 落地，含代码·内核侧）**：根因坐实——内核 `syscall_process.rs::dispatch_fork`（SYS_FORK）把 **RECEIVING 校验、`fork_from` 拷贝源、`parent_is_sys_proc` priv 解析、应答 `msgaddr` 取值**四处全错用 `caller_nr`，而 minix-rs 里 **VM 代表父进程陷入 SYS_FORK**（`vm/fork.rs:343 gateway.sys_fork(parent.endpoint(),…)`），故 `caller_nr`=VM ≠ 被 fork 的父。C `do_fork.c` 四处一律用 `isokendpt` 从消息 `endpt` 解出的父 `rpp`（L41/44/51/63/105/112），从不用 `caller`（旧 Rust 注释「C guarantees rpp==caller」是假前提）。运行时 VM 不处于 RECEIVING → 返 `KcallResult::Ok(EINVAL)`（SYSCALL 腿经 `syscall_leg_wire` 取负成 `-EINVAL`，`minix-sys::sys_fork` 的 `reply<0` 正确判错，故**错误契约本就对、非 bug**）→ VM `VmForkError::KernelCall` → PM `dF:vmF` → fork 永久失败。修=`let parent_nr = proc_table.endpoint_to_nr(Endpoint(fork_req.endpt))`（isokendpt 等价、已排除 SLOT_FREE 故覆盖 C `isemptyp(rpp)`），四处改用 `parent_nr`；`caller_nr` 形参改名 `_caller_nr`（对齐 C 未用的 `caller`）。**证据链**：宿主 `endpoint_to_nr`/位比较语义核对 + 新增 2 测（`test_t12_fork_resolves_parent_from_endpt_not_caller` 旧码下必挂、`test_t12_fork_rejects_unresolvable_parent_endpt`）+ 修好被掏空的 `rejects_non_receiving_parent`（令其真达 RECEIVING 闸，CodeReview W1 采纳）。**真机（决定性）**：干净 HEAD 基线 c14 vs 修后 c16/c17——slot 1..0xa pick 计数**逐位相同**（slot8=763/763 等）=无早/boot 回归；仅 B17 活锁的 slot0↔slotb ping-pong 从 **907/665 → 311/63** 坍缩=**fork 不再 EINVAL、子真被创建**。三件套绿（docker kernel **815**/arch 242/vm 528·0fail / fmt syscall_process 54=54 零新增 / 两次真机 c16==c17 签名一致；c17 之后的改动全是 `#[cfg(test)]` 体与注释、release 内核二进制逐字节不变，签名沿用）。⚠️ **探针布局脆弱性登记仍有效**（§1.33）：本修刻意只动内核+minix-types 语义、不碰 PM，正是绕开该债。

- **B18 已修（1.36 落地，含代码·VFS 侧）**：根因坐实+C 交叉验证——**VFS 回执未 echo 目标进程端点**。C `service_pm`（minix3 `vfs/main.c`）每一路回复都 `proc_e=m_in.VFS_PM_ENDPT; m_out.VFS_PM_ENDPT=proc_e;` 把请求目标端点回填进回执 `VFS_PM_ENDPT`(=m7_i1)；PM `handle_vfs_reply`(main.c:315-321) 从 m7_i1 取端点 `pm_isokendpt` 解槽位再断言挂 `VFS_CALL`。但 minix-rs `VfsReply::encode()`(`minix-types/ipc/vfs.rs`) 对所有变体把 m7_i1 **恒置 0**（裸回复）从不 echo→PM 解 `Endpoint(0)`(=`Endpoint::PM`、PM 自身 mproc 槽、in_use 但无挂起 VfsCall)→`take_vfs_call(slot 0)` **panic「reply without request (slot 0)」**。修=新增 `VfsReply::encode_reply_for(target)`（`encode()` 上补写 `m7i1=target.get()`，不改 `encode()` 签名零 churn）+ `vfs/main_loop.rs` `Route::Pm` 两成功站点携带端点。`VfsCall::endpoint()` 对 Fork/SrvFork 回**子端点**、与 PM `do_fork` 挂 VFS_CALL 于子槽精确对齐。**三件套绿**（docker kernel 815/arch 242/vm 528 · minix-types 309(含新测 `test_vfs_reply_encode_reply_for_echoes_endpoint`✓)/minix-vfs 530 · 0fail / fmt vfs.rs 2=2・main_loop.rs 1=1 零新增 / 真机 c18==c19 结构签名一致(wc 同 24926)、**reply-without-request panic 彻底消除、ECALLDENIED/pmstall 归零**，boot 从 c16 的 24332 推进到 **24926 行**）。CodeReview **无 MUST-FIX**（union 写同一活跃臂 SAFETY 成立）。⚠️ **邻近边登记**（CodeReview SHOULD、不阻塞）：`Route::Pm` 两条 **Err 分支**（`queue_reply(SyscallResult::Error/Nosys)`）仍发裸 `-errno`（m7_i1=0、负 m_type 不命中 `is_vfs_pm_rs`）→ PM 误路由、原 VFS_CALL 悬挂；系**本修前既存**隐患（C 里那些 handler void 不回错），boot 关键路径 fork/exec/exit 均成功不命中（c18 实测 0 panic 佐证），本轮保持 diff 最小未纳入。**以下为 §1.35 历史侦察记录（终态机制推导当时未知根因）：**修后 boot 过 Runcom、fork 链通，但 rc marker 仍未出。**终态实锤=PM panic**：`servers/pm/src/ipc/vfs.rs:385: handle_vfs_reply: reply without request (slot 0)`（c16 第 24310 行，日志止于 24332；行首带 `file:line:` 前缀=标准 panic 格式，非 diagctl）。这是 C `main.c:324` `assert(p->P_flags & VFS_CALL)` 的忠实对位——**PM 收到一条 VFS 回执，其 `m_u.m_m7.m7i1` 端点经 `pm_isokendpt` 解析到 PM UserSlot 0（该槽 `endpoint()==回执端点` 且 `is_in_use()` 成立，但 `ipc_blocked` 非 `VfsCall`）→ `take_vfs_call` panic**。panic 后 PM 终止/自旋：紧随 `nk4a: ipcerr caller=0x0 err=ECALLDENIED`（caller 0=内核槽位 PM 自身，其 panic 后 IPC 被 `!GET_BIT(s_k_call_mask)` 拒）→ 内核 `pmstall2`（trap_dispatch.rs:814，检测内核 slot0=PM 连续 running>5000 tick=自旋）→ gtick 空转至超时、boot 死。`Endpoint::PM=Endpoint(0)`/`INIT=Endpoint(11)`（endpoint.rs:61/72），故 **slot 0 ≠ init，而是 PM 自身槽位**；且 fork 子端点 `slot()` 应为 12+（boot 0..11 已占），**故触发 panic 的回执并非 `VFS_PM_FORK_REPLY` 本身，而是另一条携带 slot-0 端点的 VFS 回执**（疑 PM 代表某进程发起、VFS 却以 PM 端点回投递，或代际/端点错配）。PM `do_fork`（fork.rs:23）已读通：`child_endpoint=vm_fork(...)`→`copy_mproc` 设子 PM 端点→`tell_vfs(UserSlot::new(child_slot), VfsCall::Fork{child,parent,child_pid})` 把 VFS_CALL 挂**子槽**、发 `VFS_PM_FORK`。⚠️ **取证阻断**：§1.33 登记的 PM 布局脆弱性阻断一切 PM 侧探针，无法取「panic 回执的 opcode/m7i1 具体值」现场。**下一入口（关键）**：VFS **服务端**（`fs/` 系，非 PM、**不受 PM 布局脆弱性阻断**）是 VFS_PM_FORK/EXEC 回执 `m7i1` 端点的构造方——下轮优先读 VFS `req_fork`/`req_exec`/投递回执的 encode 站点，查其 echo 的端点从请求哪个字段取（是 `child`、`m_source`、还是请求者 PM 端点？），坐实「哪条回执带 slot-0 端点、为何」。基线 c14 尾亦有 `ipcerr caller=4 err=EDEADSRCDST`，故 IPC 错误非本修独有、系 fork 真起子进程后才走到的既有下游浮现。rc marker 未达，frontier 仍 1.30。

- **B19 前沿（B18 修后新停点：过 Runcom、无 panic，但 rc marker 仍未出）**：c18/c19 `nk4a: init-state Runcom` @24199 后 boot 持续进 gtick/pick 稳态调度（尾态 rip=0x202d68∈`KernelIpcTransport::receive`、pick->4=SCHED 空转）至 24926 行，**PM 不再 panic**、无 ECALLDENIED/pmstall——但 init fork+exec rc 脚本后 **`minix-rs rc` marker 始终未打印**、无后续 init/PM 状态标记。**下一入口**：①Runcom 后子进程（/bin/sh 跑 rc 脚本）是否真被 exec+跑起来（查子 birth/exec 标记、sh 镜像）；②若子真跑，其 `echo`/write(fd1) 到 console 链路是否通（回顾 §1.28 console_write 可观察性）；③若是新静默停点，优先内核/VFS 侧读码/宿主测（PM 侧探针仍受 §1.33 布局脆弱性阻断）。rc marker 未达，frontier 仍 1.30。

- **阶段**：**1.3 rc marker 链（当前 frontier = **1.24「B13 Bug2(EIO) 修复落地：每进程单一 grant 表——DsClient 去私有 `GrantTable` 改注入 `grants:&mut` 参数（恢复 C `sys_setgrant` 一次不变量）；真机 vp==gtab 地址一致、EIO→EBUSY、boot 达 SingleUser；三件套绿(docker 813/242/528·0fail / fmt 全 cur=0 / 两次复跑签名一致)；CodeReview 确认 RS 发布缝临时表为遗留→折叠 B14」；前代 1.23「B13 Bug2(EIO) 根因锁定：宿主复现证实 resolve_path 正确；真机已入仓 `nk4a: vg` 探针实锤 `sys_safecopyfrom(VFS)` 读到 VFS grant 表**陈旧快照**（内核读到 `fl=0x00` 不满足 USED|VALID → grant.rs:361 EPERM，且 seq/wto/len 整体错位 2+ 个生成代），VFS 当前活写（who_to=0x0a=MFS/len=13/seq=2/3/4/flags非零）内核看不到；失败发生在 MFS wire decode 的 `copy_from`（wire.rs:130 `.map_err(EIO)`），`transport.rs:221-234` 证实 decode 错即 `encode_reply(EIO)` 回执、**不经 resolve_path/load_dir_blocks**（db 探针 0 命中=真没到，非 cap 饥饿）→ §1.22 候选 B 确认；下一步：①对比 VFS `slots.as_ptr()` vs 内核 `priv(VFS).s_grant_table` 是否同值（realloc 后重注册是否生效），②若同值则查 VFS 堆 VA→PA 翻译」**；前代 1.22「宿主测试 PASS 证实 MFS resolve_path 正确，候选缩至传输层 A/B/C」；前代 1.21「B12 sendrec Path A delivery 清 REPLY_PEND」；1.20「B11 imgrd 零拷贝嵌入 MFS」；1.18「B10 getpid 线格式修复」；...）**
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

---

## 1.10x2 状态（s16d，2026-09-24）

- taskcall 无界 ELOCKED 重试版复跑：形态不变（1144 recvs、panic ×1、无 rc marker）——**重试未解互卡**：PM 重试自旋期间 sched 未取得推进条件（sched 的 receive 半未就绪的原因在更深处——sched 自身停在什么等待待查）。
- **下一轮配方（交接手）**：①probe sched 的 run_once 主循环推进位置（gtick 已证 timer 活；sched 的 receive 停在什么状态——sched 侧已有 sc-rv/srcv-err 打点零输出 = sched 的 run_once 在 receive Err 臂与 Reply 臂均未到达 ⇒ **sched 的 run_once 卡在 receive Ok 之后的处理分支**——读 sched 的 SchedMsg::from_raw(收到的 m_type) 分支处理定位）；②PM taskcall 的 ELOCKED 重试语义保留（无害），根因在 sched 侧推进条件；③修后两次复跑过 1.10 → rc marker（单元 B）。

---

## 1.11 根因收敛：init 的 getuid 被 PM 的 caller_q 过滤跳过（1.11，s16c 实证）

- 时间线（s16c）：7086 init rt-init → 7094 imain-1 getuid（此后 init 永久消失）→ 7494 sched receive-failure panic → 19000+ PM↔VFS 握手完成 → 19221+ PM taskcall ELOCKED → 19395 PM panic → 停摆。
- **根因定位**：init 的 getuid sendrec parks 在 PM 的 caller_q；PM 主循环 receive 的 **Phase 3 `caller_q_find_allowed` 的 D-16 过滤链（chain_allowed）将其跳过**（未 drain）⇒ init 的 getuid 永不完成 ⇒ init 卡死 ⇒ PM↔VFS/sched 后续交互全部异常（含 1.10 的互撞表象）。
- **对照 C**：C 的 receive-from-ANY 的 caller_q walk（proc.c:1077-1105）对**未配置过滤的进程默认放行**（filter 仅在显式配置后限制）；minix-rs 的 PM 若被 RS 配置了 whitelist（s_ipcf），init 的请求不在白名单 ⇒ 永久跳过。
- **下一轮配方**：①ipcerr 探针扩 caller=11（init）：看 init→PM 的 send 是否 ECALLDENIED/CallDenied（filter 拒绝证据）；②对照 C ipc.h filter 语义修 chain_allowed 对「无过滤/默认」的处理，或修 RS 对 PM 的 filter 配置；③修后 init 的 getuid 应完成 → init 进 runcom → exec /bin/sh /etc/rc → **rc marker（单元 B 完成）**。

---

## 1.11a 根因定案：RS priv 设置与服务器 main 启动的时序竞态（s16b，2026-09-24）

- panic location = `sched/src/server.rs:154` = `init_scheduling` 的 setalarm Err 臂（sched-alarm 探针区）——**sched 的 `sys_setalarm` 返回 EPERM**（dispatch_setalarm 仅 OK/EPERM；EPERM 臂 = `caller_has_sys_proc_with_table` 失败 = sched 的 priv 无 SYS_PROC 或 priv_id 未置）。
- **时序竞态**：RS 对服务器的 priv 设置（SetSys→SYS_PROC）与服务器 main 启动（init_scheduling 的 setalarm）交错——sched main 先行、priv 未就绪 → setalarm EPERM → panic → sched 死亡。
- **对照 C**：C 的 RS 在服务 exec 前完成 privilege 结构设置（do_exec 前置），服务器 main 运行时 SYS_PROC 必已就位；minix-rs 的 RS exec/ALLOW 流程缺该时序保证。

### 修复（下一轮，接手即做）

①审计 RS 的 service 启动序：privctl SetSys（SYS_PROC）→ ALLOW → 服务器 unblock 的次序，补齐「priv 未就绪不得 unblock」的时序（对照 C do_exec 前置 priv 语义）；②补判别测试（priv 未就绪时 setalarm 必须 EPERM、就绪后必须 OK）；③修后两次复跑：sched 不再 panic、init 进 runcom、/bin/sh exec → **rc marker（单元 B 完成）** → 单元 C-K。

---

## 1.11b 收口（s16d 时代，2026-09-24）

- sched:77 panic 的 errno 未现形：sched 侧 diagctl 打点静默失败（与 panic-handler Stage 2 同族）——sched→PM 的 SYS_DIAGCTL 调用本身异常（候选：PM 的 diagctl handler 对 sched 的请求处理缺失/返回错误）。**PM 的 diagctl（同族调用）在 PM 侧正常**（pmvi 打点可见）⇒ 差异在调用者身份或其 priv 的 diag 权限。
- **下一轮配方**：①kernel SYS_DIAGCTL handler 加 caller 打点（一轮即见 sched 的 diagctl 是否到达 kernel 及返回值）；②据 errno 修 sched 的 init_scheduling 首个失败调用（get_hz 或 setalarm）；③修后 sched 不再 panic → init 进 runcom → /bin/sh /etc/rc → **rc marker（单元 B 完成）** → 单元 C-K。

---

## 1.10x2 状态（s16c，2026-09-24）

- taskcall ELOCKED(208) 重试臂生效实锤：s16c elock 仅 1 发（此前预判的持续互卡未现）——**重试成功解开了那次瞬时互撞**；taskcall 最终返回非 208。
- 剩余停点（1153 recvs）：panic-enter ×2（7494 sched 早期、19395 PM 晚期 barrier）——**PM barrier m_type≠0 panic（init.rs:739）仍在**；sched 早期 panic 仍在。
- **下一轮**：①定位 PM barrier panic 的实际 m_type 值（init.rs:739 的 panic 消息经 1.10s 的 errno 打点应可见——grep s16c "final barrier"）；②sched 早期 panic 的根因（server.rs receive-failure 的连续失败——sc-rv/srcv-err 探针零输出待核：探针是否在 sched 的编译单元生效）；③修后两次复跑过 1.10 → rc marker（单元 B 完成）。

---

## 1.11c 收口（2026-09-24）：sched setalarm EPERM 根因收敛至 boot flags 传递链

- sched:77 panic（setalarm EPERM）的机制链闭合：`dispatch_setalarm` EPERM 臂 = `caller_has_sys_proc_with_table(sched)` 失败 ⇒ **sched 的 priv 无 SYS_PROC**。
- **疑点收敛至 boot flags 传递链**：RS 的 boot.rs:1010 SetSys 用 `Privilege::boot_priv(priv_.flags, ...)`——`priv_.flags` 来自 boot 表（BootImageStruct 的 flags 字段）；sched 条目的 flags 若缺 system 位 ⇒ SetSys 后仍无 SYS_PROC ⇒ sched 的 setalarm/一切 SYS 调用 EPERM ⇒ sched 出生即死。
- **旁证**：s15 系的 sched-alarm 探针（sched 侧 diagctl）静默失败与 sched 无 SYS_PROC（diagctl 权限拒）自洽——**sched 从出生起就无 SYS_PROC 权限**。
- 下一轮：①对照 C table.c image[] 表的 flags 列（sched 条目应带 SYSTEMIC 位）核对 minix-rs 的 boot 模块 flags 源（xtask image 装配 / boot-shim loader / 内核 boot info 的 flags 字段链）；②修 flags 传递缺位；③修后两次复跑：sched 存活、无 EPERM、boot 越过 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10y2 收口（s16d 时代，2026-09-24）

- **sched 侧一切 diagctl 打点静默失败的总根因候选**：RS 授予 sched 的 kcall mask（SRV_KC 模板）疑未含 SYS_DIAGCTL——sched 的 sys_diagctl_write 被内核拒绝（EPERM/CallDenied），`let _ =` 静默吞错 ⇒ sched 侧一切 errno 打点不可见。
- **下一轮**：①kernel SYS_DIAGCTL dispatch 加 caller 打点（一轮定位：sched 的 diagctl 是否到达 kernel、返回何错）；②据结果修 RS 的 kcall mask 模板（SRV_KC 补 SYS_DIAGCTL）或 kernel diag handler；③sched 侧诊断通道打通后，errno 现形 → 修 init_scheduling 首个失败调用 → 两次复跑过 1.10 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10y2-b 本轮收口总结（s16e，2026-09-24）

- 本轮仪器化全部落地并验证：elock 全量状态（PM rts/getfrom + sched rts/getfrom/sendto + PM 栈回溯）、dd2m（dst/cmt/xmt）、ipcerr（cap 64 + caller∈{0,4} 过滤）、diag-efault、imain-0..6、init-state ×7、pmvi k=0..b+barrier-done、srcv-err/sc-rv、rearmlen、gtick。
- **s16e 定案**：diag-efault 零命中 ⇒ sched 的 diagctl 写未 EFAULT——诊断静默失败机制在更深处（diagctl 返回路径/调用号戳/或 len==0 短路——注意 len==0 时 dispatch_diagctl 直接返回 Ok(0) 不打印：**若 sched 侧格式化产出了空串，diagctl 静默成功**——srcv-err/sc-rv 的格式化可能产出了空内容）。
- **当前停点不变**：PM↔sched 双向 SENDING 互撞（ELOCKED 单发）+ 全链健康 idle，1143-1151 recvs。

### 下一步（接手即做）

①核 srcv-err/sc-rv 打点的格式化是否产出空串（line[13..23] 的填充逻辑核对——v.to_be_bytes()[1..] 只填 6 字节但 line 有 24 字节、其余为零——**hex 编码位置偏移 bug**：应为 8 个 hex 字符，实际填了 6×2=12 字节越界一半——修正编码循环）；②复跑读 sched receive 失败 errno；③修根因 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.11b 状态固化（s16f，2026-09-24）

- sched:77 panic 复现确认（LN=0x4d=77 ✓）：init_scheduling 返回 Err（get_hz 或 setalarm 之一），sched-hz/sched-alarm 探针因 sched 侧 diagctl 断路未现形。
- **下一轮配方（接手即做，两步）**：①sched 侧诊断通道修复：grep sched 的 KernelIpcTransport 的 diag 通路（`sys_diagctl_write` 走 DirectKernelCallTransport → SYS_DIAGCTL kernel call——查 kernel dispatch_diagctl 对 caller=4 的返回（内核侧 ipcerr 式打点））；②按 errno 修 init_scheduling 首个失败调用；③修后 init 进 runcom → /bin/sh /etc/rc → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.10x3 状态固化（s15x2，2026-09-24）

- ipcerr 扩量+caller 过滤落地（s15x2）：`caller=0x0 err=ELOCKED`（PM taskcall send 半 ↔ sched EPERM 应答互撞）+ `caller=0x0 err=ECALLDENIED ×2`——**PM 的内核调用被 CallDenied**（kcall mask 缺口候选：SYS_SCHEDCTL/SYS_DIAGCTL 家族）。
- elock 全量状态探针入仓（0111b476e/ba0071680）：PM↔sched 双向 SENDING 互撞全现场。
- diag-efault 探针入仓（s16e 零命中=sched 的 diagctl 拷贝未 EFAULT——诊断静默失败机制在更深处）。
- WORKLOG 1.7-1.10y2 全链同步；kernel 811 全绿、xtask 12 全绿；`AI-chats/daily.todo.md` 与外来文件全程未触碰。

### 下一步（接手即做）

①CallDenied ×2 的调用号定位（ipcerr 补 call_nr 打点，一轮）——修 PM 的 kcall mask 或调用点；②ELOCKED 臂的 taskcall 重试/异步语义修复（重试需 yield 让 sched 推进）；③修后两次复跑过 1.10 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.11a sched 侧 EPERM 根因定位（2026-09-24）

- sched replying EPERM(1) 的产生点：sched 的 `do_start/do_stop/do_nice` 处理 PM 的 SCHEDULING taskcall 时，内部 `kernel.schedctl`（SYS_SCHEDCTL）返回 EPERM——kernel 的 `caller != p_scheduler` 检查拒绝（syscall_process.rs:780-830：仅 `p_scheduler == None` 时放行任意 caller）。
- **根因**：fork 子进程的 `p_sched.scheduler` 字段时序——sched 的 do_start 的 SYS_SCHEDCTL 要求 target 的 p_scheduler 已 = Some(sched)；该字段的设置链（PM fork 路径 / RS birth / SYS_SCHEDULE）未在 sched 的 do_start 前就位 → EPERM。
- **对照 C**：C 的 sched_do_start 前置 `sched_init_proc`（proc.nr 的 p_scheduler 预设）；minix-rs 的对应设置链缺失/时序错位。

### 修复配方（下一轮）

①PM fork 路径（fork.rs）在 sched_ctl 前补 `SYS_SCHEDCTL`（caller=sched）设置子进程 p_scheduler（或 RS birth 协议补）；②对照 C schedule.c 的 sched_init_proc 语义核对；③修后两次复跑：sched 不再 EPERM → init fork 链通 → /bin/sh /etc/rc → rc marker（单元 B 完成）→ 单元 C-K。

### 1.11a 补充（下一轮探针配方精确化）

elock 现场补两字段即可定案互撞的双方请求语义：①`c_rts`（PM 的 rts）与 `c_rpv`（PM 的 REPLY_PEND 位）——判定 PM 的 taskcall 是否被 1.10l 正确转入 receive 半；②`x_rpv`（sched 的 REPLY_PEND 位）——判定 sched 的 SENDING 是否为其 taskcall 应答（应为否，sched 不 taskcall PM）。字段齐后按 1.11a 修复配方实施。

---

## 1.10z 三根因连修：互卡 / 伪 reply / 权限剥旗（2026-09-24，serial_s17a…s17k）

> 本轮三个独立根因一起收敛，全部有真机探针链实证。修复后 boot 首次到达「12 服务全出生 + PM 主循环空闲」层；新停点见 1.11d 节。

### 根因① 1.10x：Phase 3 drain 的 SENDREC 停车 getfrom=ANY → 目的地端点

- C 对位 proc.c:1104-1107：SENDREC 发送者自身的 mini_receive 以 `src_e`（=sendrec 目的地）阻塞，不是 ANY。旧 1.10l 停车写 ANY → 任一第三方发往 PM 的 Path A 直投（is_willing_to_receive 只认 getfrom）冒充 reply 完成 PM 的 sendrec（s16g 的 PM↔sched 互 SENDING ELOCKED livelock + init 永停 imain-1 同根）。
- 修：`ipc.rs` drain 停车腿 `s.p_getfrom_e = caller（drainer）endpoint`。单测 `test_sendrec_parked_receive_half_getfrom_locks_to_destination`。

### 根因② RS WirePrivUpdate s_id 宽度错位 → 全服务 SYS_PROC 被剥 → sched setalarm EPERM

- 内核 `SysId = u16`（kpriv.rs:10），RS 镜像 `s_id: i32` → repr(C) 头部错位 4 字节：内核把 rs s_id 高半字读成 s_flags（小 id 恒 0），SET_SYS 覆盖后所有服务 flags=0。`s_ipc_to` 恰在 24 字节处重新对齐 → IPC 掩码正常、症状只在 flags/sig_mgr 腿（极迷惑）。
- 实锤：s17b `setsys tgt=4 req_fl=0x0 eff_fl=0x0`；修后 s17c `req_fl=0x12 eff_fl=0x12`（SRV_F=SYS_PROC|PREEMPTIBLE）。sched 的 `sys_setalarm`（init_scheduling，main.rs）EPERM panic 随之消失。
- 修：`servers/rs/src/trap_api.rs` s_id: u16 + `offset_of!` 三点守卫（s_flags@2 / s_init_flags@4 / s_ipc_to@24）+ size 守卫，漂移即编译失败。

### 根因③ sendrec 快路径 receive 半 ANY → 目的地（本轮主根因）

- `engine.sendrec()` 秒达分支 `self.receive(caller, ANY)`——C proc.c:569-583 SENDREC 落入 RECEIVE 臂用**同一 src_dst_e**。真机 s17j 铁证：PM sendrec(sched) 秒达后 receive(ANY) 把 init 排队在 PM caller_q 上的请求当 reply 消费（`p3park s=0xb gf=0x0` 在 `s4r` 后出现），sched 真 reply（`p4a src=4 gf=ANY rpv=n`）落进 PM 主循环成野请求 → PM 依协议回 ENOSYS(78) → sched no_sys 再回 ENOSYS → **ENOSYS ping-pong livelock**（s17e/s17f/s17g 反复 rv 004e×28）。
- 修：`self.receive(caller_nr, dst_endpoint)`。单测 `test_sendrec_fast_path_receive_half_scopes_to_destination`。
- 旁案：MinixSchedCtl::taskcall 的 1.10w ELOCKED 重试读 `rv == 208`（reply 语义），而内核 deadlock 检查以 **syscall 错误**返回 ELOCKED（走 `Err(_) => -EIO` 臂）——重试从未生效。本轮未修（taskcall 路径已被根因③疏通），登记 1.11e。

### 探针自身事故（教训入档）

- sched server.rs 的 sc-rv/srcv/rearm/mt 取证探针多处 `line[..N].copy_from_slice(b"...")` 长度不匹配——copy_from_slice 即 panic。**s17c/s17d 的 "server.rs:285/286 panic" 是探针自己**，非协议臂；且 >16B 的 diagctl 写被内核静默丢弃（s14r/w 已知），rearmlen 18B 的 errno 从未上串口。已全改 ≤16B 并逐一核对字面量长度。假 panic 消耗两轮真机，后续探针必须先本地核对 `line[..N]` 与字面量等长。

### 验证

- docker：kernel **813**（811 基线 + 2 新单测）/ arch 242 / vm 526 全绿；rustfmt nightly --check 六文件 hunk 数与 HEAD 持平（零新增漂移）。
- 真机 s17j / s17k 两轮：panic 清零、ENOSYS ping-pong 清零、elock 清零；`p4a src=4 gf=4 rpv=y` 实锤 PM↔sched sendrec 往返闭环。

## 1.11d 新停点（s17k）：init 的 getuid 被 PM 收下后无回执（下一轮主攻）

- 尾态（两轮快照一致）：12 服务 + PM 全部 RECEIVING 空闲、runnable=no、无 picknone 之前的任何 panic；init(0x10b) 停车 RECEIVING from=0（1.10x 停车腿 ✓）。
- 探针链：`p3park s=0xb gf=0x0`（PM 的 receive(ANY) 把 init 的 getuid drain 走，F14 同步拷贝应已落 PM 用户缓冲）→ **此后 PM 零输出**，无 reply（init 永停 receive 半）。PM 未 panic、未 ENOSYS——疑似 PM dispatcher 收到 m_type 后静默忽略（或同步拷贝落点/字节数不对，PM 收到 m_type=0 一类野值直接跳过）。
- 下一步（一轮探针定案）：PM dispatcher 入口打 `nk4a: pmrecv mt=<m_type> src=<m_source>`（≤16B，cap 8）——判定「PM 收到什么」与「GETUID 臂是否进入」。若 mt=0/野值 → 查 F14 同步拷贝的 p_delivermsg_vir 新鲜度（PM 该次 receive 的缓冲指针）；若 mt=GETUID(24) 正常进臂 → 查 reply 腿（sendnb 目的端点/reply 构造）。修后两次复跑 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.11d-fix m_source 盖章（2026-09-24，serial_s17m…s17p，commit 本轮）

- **根因**：Phase 3 drain 的 F14 同步拷贝直写用户缓冲时漏盖 `m_source`——C proc.c:1071-1075 对 p_delivermsg 盖 `m_source = sender->p_endpoint`，而发送者用户缓冲里 m_source 恒 0。PM 收到 init 的 GETUID 后 `pm 00600`（mt=6 src=0，init 实际端点 0xb）→ 回错槽位 → init 永停。
- **修**：ipc.rs drain 臂 `sender_msg.m_source = sender_ep` 后再 copy_msg_to_user（回退臂原有盖章不动）。docker kernel 813 全绿、fmt 88/88 零新增。
- **验证（s17o）**：`pm 0060b`（GETUID src=init ✓）→ `pm 0040b`（GETPID ✓）——init 与 PM 的调用链首次打通。

## 1.12 新停点（s17o/s17p 双签名）：Path A 唤醒丢入队（F10d 家族）+ VFS↔RS 请求链

- **s17o**：init getuid/getpid 过，随后的 VFS 请求使 init SENDING to=VFS(1)；VFS 停车 RECEIVING from=RS(2)（VFS 的 sendrec-to-RS receive 半）——RS 空闲 RECEIVING(ANY) 却未消费 VFS 的请求。
- **s17p（时序变体）**：PM 的 sched taskcall send 半 Path A 直投后 sched 被唤醒但 **runnable=yes queued=no**（F10d 同族：rts 原始 clear 与入队半脱节）→ sched 永不跑 → taskcall 无 reply → PM 停车 receive-from-sched → init 的 getuid 停在 PM 的 caller_q。
- **下一步（下一轮）**：①内核 `take_wake_target` 入队臂打点（`wake enq nr=X`，cap 16）+ Path A 直投臂打点，判定丢入队的站点（多 wake 覆盖？rts_unset 条件分支？）；②VFS 停车 from=RS 的 sendrec 语义核对（VFS 为何向 RS 发 sendrec、RS 为何不收）；③修后两次复跑 → rc marker（单元 B 完成）→ 单元 C-K。

---

## 1.12a 多唤醒互覆修复（2026-09-24，serial_s17q/s17r）

- **根因**：engine 的 `wake_target` 是单槽 `Option`——同一次 syscall 内第二次 `record_wake_target` 覆盖第一次（sendrec 的 send 腿唤醒目的地 + receive/drain 腿唤醒发送者；sendnb-reply+drain 同理），被覆盖者 rts 已清却永不入队 → `runnable=yes queued=no`（s17p 实锤 sched 卡死；F10d 家族新形态：不是绕过入队而是入队记录被覆写）。C 的 `RTS_UNSET` 在每个 clear 现场立即入队，无此窗口。
- **修**：ipc.rs `wake_targets: [Option<ProcNr>; 4]`（满丢最旧，防御容量）；`take_wake_target` 单槽 drain；syscall.rs/trap_dispatch.rs 两个消费点改循环全量入队（trap_dispatch 先收集后入队规避叠加借用）。docker kernel 813 全绿、fmt 三文件 hunk 总数与 HEAD 持平（228/228）。
- **验证**：s17q/s17r 两轮签名一致且 s17p 的 sched 饿死变体未再复现；init↔PM 的 GETUID/GETPID 打通稳定复现（`pm 0060b`/`pm 0040b`）。

## 1.12b 新停点固化（s17o/q/r 三轮一致）：init→VFS 阻塞在 VFS 的 from=RS 停车后面

- 尾态链：init(0xb) SENDING to=VFS(1)（getuid/getpid 后的首个 VFS 请求，疑似 open/stat 类）→ VFS(1) RECEIVING **from=RS(2)**（停车 receive 半或主循环限定源收 RS——`p3park s=1` 不在尾窗，非 drain 停车）→ RS(2) RECEIVING(ANY) 空闲。
- 疑点两分支：①VFS↔RS 的 RS_INIT/启动协议缺一条消息（RS 发完 INIT 即回 receive，VFS 等第二条；或 VFS 的请求 RS 已处理但 reply 臂缺失）；②VFS 的 sendrec-to-RS 请求 RS 收下后处理失败且无 reply。
- **下一轮探针配方（一轮定案）**：RS dispatcher 入口 `nk4a: rsm mt/src`（≤15B cap 16）+ VFS dispatcher 入口同款 `vfm mt/src`——看 RS 是否收到 VFS 的请求/收到什么 m_type；RS 的 reply 腿是否发出。随后按缺失臂修复 → 两次复跑 → rc marker（单元 B 完成）→ 单元 C-K（aarch64/riscv reply_wire → 命令面 → W^X → ABI 清单 → 三架构 marker → 测试上机 → 收尾）。

---

## 1.12c SENDA 实现尝试与回退（2026-09-24，serial_s17s/s17t，WIP 存 `tmp/nk4c-1.12-senda-wip.patch`）

- **定性升级**：`KernelUserCopy::read_senda_entry` 是**永久 stub**（恒 PageFault）——SENDA 从未在生产可用；RS 的 RS_INIT 经 `asynsend→senda` 全部静默丢失，VFS 的 boot `receive(RS)` 永等 → 1.12b 停点的直接根因。
- **WIP 内容**（已写好、未过真机，patch 在 tmp）：①trait 加 `root: PhysBytes`（C A_RETR 按发送者段读；deliver_async 跑在接收者陷入里必须显式传发送者 root）；②KernelUserCopy 真实现（`user_copy_range_mapped` + WireAsyncSlot repr(C) 镜像：flags@0/dst@4/result@8/msg@16，offset_of 守卫）；③minix-sys `AsyncSlot` 补 `#[repr(C)]`；④两消费点传 root；⑤RS/VFS dispatcher 探针（rsm/vfm）。
- **s17t 回退原因**：真机 GP fault（vector 13，trap_dispatch.rs:1004 dispatch_body，rip 0x5abd639 kernel text，紧随 `pdmv-set krn`、pfwd out=B 之后，boot 大幅提前死亡）——senda 真读/探针二者之一引入，根因未定位；248 行 patch 保留待查。
- **下一轮入口**：①patch 二分（先只上 rsm/vfm 探针不上 senda 真读，跑一轮定界）；②检查 `user_copy_range_mapped` 对非当前 root 的走表是否安全（deliver_async 场景）——GP 而非 PF 说明可能有内核态非法访问未走校验臂；③`AsyncSlot` 补 repr(C) 后 `size_of` 与 WireAsyncSlot 80 断言对齐核实。
- 验证基线不变：HEAD（839ecbd4b）= s17q/s17r 两轮稳定（init↔PM GETUID/GETPID 通、无 panic），docker 813/242/526 全绿。

---

## 1.12d SENDA 真读修复 + 探针链诊断（2026-09-24，serial_s18a…s18i，WIP 已入仓）

### 已完成（本 commit 入仓，未过 rc marker）

1. **senda 真读实现（根因修复方向正确）**：`KernelUserCopy::read_senda_entry`/`write_senda_result` 从永久 stub 改为真实现，**但必须走「发送者 root 翻译 → 物理地址 → Direct Map 窗口」**，不能直接解引用用户 VA——`deliver_async` 跑在接收者的 receive 陷入里，current CR3 是接收者的，同一 VA 落到错误地址空间（这正是 s17t 的 GP fault vector 13 根因，见 1.12c）。新增辅助 `copy_via_root_pages<D>`（kernel/src/ipc.rs）：
   - 逐页 `CurrentPteWalk::walk(root, va)` → `pa`（**注意 x86_64 `walk_translate` 返回的 pa 已含页内偏移**，`pte&ADDR_MASK | vaddr&0xFFF`；再叠一次 offset 会错位——首版踩过）
   - `D::kernel_phys_to_virt(pa)` → DM 窗口 VA → `copy_nonoverlapping`
   - 边界：`USER_ADDRESS_SPACE_LIMIT` + `checked_add`（防回绕）+ `USER_ACCESSIBLE` 标志位
2. **trait 扩展**：`UserCopy::read_senda_entry/write_senda_result` 增加 `root: PhysBytes` 参数（C A_RETR/A_INSRT 按发送者段读；senda 传 caller 自己的 root，deliver_async 传 sender 的 root）。全部实现点（KernelUserCopy 生产/测试双形态、proc_table 两个 stub、ipc.rs 测试 stub×4）已同步。
3. **`AsyncSlot` 补 `#[repr(C)]`**（libs/minix-sys/src/ipc.rs）——原缺 repr，布局不保证；配 `WireAsyncSlot` 镜像 + `offset_of!` 守卫（destination@4 / message@16 / size == 16+size_of::<Message>()）。
4. **验证**：docker kernel **813 全绿**；真机 s18d **零 GP fault**（对比 s17t 的 vector 13）——跨地址空间读的安全性修复**已被真机证实**。

### 诊断链（s18e…s18i，全部探针入仓，可直接复跑）

| 探针 | 位置 | 结果 |
|------|------|------|
| `saread` | read_senda_entry 内 | s18c：`root=0x35f8000 tbl=0x7fffffff5d20 v=y`（校验通过） |
| `apend` | take_pending_async（bitmap≠0 才打） | **零输出** → VFS 上 pending 位从未被设置 |
| `saent` | senda 逐条 dst 解析后 | **零输出** → 循环体从未执行到该点 |
| `sa-in` | senda 入口 | s18h/i：`c=0x2 n=0x1`（RS，count=1）——**入口到达** |
| `sa-out` | senda 四条早退门（e1-idx/e2-priv/e3-nosys/e4-clr） | **零输出** → 四条早退都没走 |
| `sa-readfail` | senda 循环内 `Err(_)` 臂 | **零输出** → 表读**没有失败** |

**结论（闭合）**：`senda` 被调用、入口门全过、表读**成功返回**，但 `flags` 解出 **0 = AMF_EMPTY** → `continue` → 循环结束 → `done` 仍 true → 返回 Delivered，**一条消息都没投**。即 **`copy_via_root_pages` 读回全零**（不是读失败，是读到了零内容）。

**下一步（下一手 agent 的第一件事）**：定位「读回全零」。
- 优先怀疑：①`self.procs[caller_idx].p_seg.phys_root` 不是活 root（对当前进程应等于 `current_root_phys()`）——加探针对拍两者；②DM 窗口 VA 是否真映射了该物理页（读回的零是「映射到零页」还是「窗口偏移错」）；③`walk` 返回的 pa 是否已含偏移（已确认 x86_64 含；需核对另两架构）。
- 手法：在 `copy_via_root_pages` 内加「walk 出的 pa / DM VA / 读回首 4 字节」三联探针，一轮即可定案。

### 环境备注

- `/tmp/nk4a` 会被清（WSL 重启）；复跑前需 `mkdir -p /tmp/nk4a && cp tmp/nk4a/vars.fd /tmp/nk4a/vars_run.fd`。
- 当前 frontier 基线：HEAD = 本 commit；docker 813/242/526 全绿。

---

## 1.12e SENDA 读方向互换修复（2026-09-24，serial_s19a/s19b）

### 现象

1.12d 探针链已把卡点收敛到「`copy_via_root_pages` 读回全零」。本轮未先上真机探针，改派 Debug 子代理做静态根因侦察，直接定位到方向互换（与 s18 全部探针签名逐条吻合），修复后真机两轮实锤：`saent` 首次出现且十连投（dst=0x1/3/4/5/6/7/8/9/a + none，mt=0x714=RS_INIT），`apend` 首次出现（VFS 的 s_asyn_pending 位被置起）。

### 根因

`copy_via_root_pages` 的 `to_kernel` 参数两臂内容与 doc 注释、与全部 4 个调用点的约定**恰好互换**：实现里 `to_kernel=true` 执行的是页→buf（读），`false` 执行 buf→页（写）；而 doc（ipc.rs:438-440）与调用点（`read_senda_entry` 传 false、`write_senda_result` 先 false 预读后 true 回写）约定 false=读。于是读腿拿到的 `out` 永远是初始化零 → flags=0=AMF_EMPTY → senda 一条不投；且**更破坏性**：写臂把清零的 80 字节写进用户栈槽位，把 RS 刚填好的 asynmsg 抹掉（用户槽 flags 真值应为 AMF_VALID|AMF_NO_REPLY=9）。C 锚点：`minix3/minix/kernel/proc.c:1244`（A_RETR 把表项拷进内核 tabent，方向 page→buf）与 `proc.c:1307`（A_INSRT 回写）。Rust 侧偏离：1.12d 首版跨地址空间改造时把两臂写反，且预置的真机探针只看到「读回全零」——因为全零是**自己写进去的**。

排他性佐证（Debug 子代理静态对拍，均不成立）：RS `p_seg.phys_root` 与切表后 CR3 一致（s18i `sa0-`/`sa1-after` 探针）；x86_64 walk 逐级查 PRESENT 位不会静默返零（paging.rs:265-337）；DM 窗口覆盖该物理页；用户侧表 VA/count/线格式（WireAsyncSlot offset_of 守卫）全部对位。

### 修复

`os/kernel/src/ipc.rs` 的 `copy_via_root_pages` 交换两臂（true → buf 写页，false → 页读进 buf），与 doc/调用点对齐；不改调用点（4 处 + trait doc 是多数方）。`write_senda_result` 同一函数两腿自动恢复。另登记不修：read_senda_entry 的 `read_volatile(out.as_ptr() as *const WireAsyncSlot)` 对非对齐字节缓冲转指针形式上 over-aligned，当前 codegen 良性，建议后续 `read_unaligned` 化（task1-close 候选）。

### 验证

- docker：kernel **813** / arch **242** / vm **526**（基线持平，0 failed）
- rustfmt：kernel/src/ipc.rs hunk 数 HEAD=99 NEW=99（零新增漂移）
- 真机：serial_s19a / serial_s19b 两轮签名一致——`saent`×10、`apend`×8、无 vector 13；rc marker 未出现（新的更深层停点，见下）
- commit 后 CodeReview：无 P0；P1×1（新 unsafe 块同时构造 `buf.as_ptr()`/`as_mut_ptr()`，Stacked Borrows 下读腿指针在写腿借用后使用属别名违规）→ 已改为两臂共用单次可变再借用 `base`，同镜像再跑 s19c/s19d 两轮签名一致（saent×10/apend×8/panic×1/v13×0）

### 新停点（1.12e-newstall）

1. **RS 侧 1 次 panic-enter**（s19a:4880，紧随 `pdmv-set krn m_user=0x7fffffff5b58` + fx/kdst 证据，panic-msg-nonstr）——疑似 Task C/S3 家族的 eager 直写腿在新投递量下重现，需定性。
2. **多进程同一栈页缺页循环**：init/VFS 等（snd=0x1/3/4/5/6/7/9/a）反复对 fa=0x7fffffffea68（用户栈页）缺页，VM 服务但 `fa8` 读回全零、回复 bytes=0；尾态全服务器 RECEIVING(ANY) 饿死。下一手第一动作：对 fa=0x7fffffffea68 这个 VA 打 VM 侧 region 槽状态 + 该次 fault 的 vm 应答内容探针（对照 1.10「PM↔VM 缺页循环」节的入口配方：dump 该 VA 的 PTE 与 region 槽，查同 VA 重复 = PTE 丢失/映射被拒），并先定性 panic-enter 是否与循环同根。

---

## 1.13 RS_INIT rproctab grant 读 EPERM（Direct Map 窗口外内核栈别名）修复（2026-09-24，serial_b4d/b4e → b4f/b4g）

### 现象

1.12e 修好 SENDA 方向后，boot 停点仍在 RS 侧一次 panic-enter。s20b 取证推翻 1.12e-newstall 的「多进程同一栈页缺页死循环」假设：那 20 条 `pf#` 的 rip 逐条推进、中间层页表逐级建出、只有叶子 PTE(lvl1)=0，是**正常按需分页**、不是同页死循环——该假设为幻影。加 `rs-initfail` 探针（boot.rs:1252 前打 `m_source`+result，绕开 `{m:?}` 的 `panic-msg-nonstr`）实锤：真停点 = RS `catch_boot_init_ready`（boot.rs:1234-1275）收到 `src=8 res=3`——VM(src=8) 的 RS_INIT 回了 ESRCH(3) → boot.rs:1254 fail-closed panic。VM 侧 `vm-rswire` 探针显示 `sys_safecopyfrom(Endpoint::RS, gid=0, buf, len=680)`（读 rproctab）返回内核 EPERM(-1)，真 errno 被 VM 的 `map_err` 吞成 ESRCH 上报。内核 `grant.rs` 的 `nk4a_vcopy_code` 把该 EPERM 落到具体腿：`vg readfail gr=0x2 code=2` → code=2 = **DstPageFault**，位置 = `verify_grant` 读 granter grant 表项。

### 根因

`verify_grant` 读 granter 用户空间的 grant 表项到内核栈局部 `grant_entry` 时，把**内核栈变量的 VA 经 `CurrentDirectMap::virt_to_phys` 转成 `AddressRef::Physical`** 当作 copy 目标，再走 `cross_space_copy` 的 Direct-Map 窗口守卫。内核栈/数据 VA（PML4[256]，`0xFFFF_8000_...`）落在 Direct Map 窗口（`KERNEL_DIRECT_MAP_BASE = 0xFFFF_8080_...`）之外，`virt_to_phys` 对高半区镜像 VA 产出一个无意义的「物理地址」，其 DM 别名未被映射 → 守卫判 `DstPageFault` → `verify_grant` 返回 EPERM（do_safecopy.c:126「隐藏 granter 设了非法 grant 表项」的错误路径）→ VM 的 rproctab safecopyfrom 失败 → 回 ESRCH → RS fail-closed panic。

C 锚点：`minix3/minix/kernel/system/do_safecopy.c:116-128` 用 `data_copy(granter, s_grant_table + sizeof(g)*idx, KERNEL, (vir_bytes)&g, sizeof(g))`——`KERNEL` 侧的 `&g` 是内核直接用**自身 VA** 访问的栈变量，不经 DM 别名（`memory.c` 的 `virtual_copy`/`lin_lin_copy` 对 KERNEL 端就是本机 memcpy）。Rust 早期实现误把内核栈 VA 伪装成物理地址走 DM，是对 C `KERNEL` 语义的偏离。

### 修复

补上读侧的 mirror（此前只有写侧 `cross_space_write`/`write_to_process_vmcheck`）：

1. `os/kernel/src/vm.rs::cross_space_read<D>` —— src 是进程地址（PTE 解析 + DM 别名 + 窗口守卫），dst 是内核局部 `&mut [u8]`（**直接用自身 VA 写**，不加 DM 守卫）；**按物理连续段分片**（`lookup_range_in_table` 逐段解析），忠实对位 C 的 `lin_lin_copy`；`AddressRef::Physical` 源走单段直拷快路。
2. `os/kernel/src/cross_space.rs::read_from_process_vmcheck` —— 封装 `cross_space_read`，`Suspended(Src)` 时按 `check_params{start, length=dst.len(), write_flag:false}` 调 `suspend_for_vm_with_copy`，逐字段镜像 `write_to_process_vmcheck`。
3. `os/kernel/src/grant.rs::verify_grant` —— 改用 `read_from_process_vmcheck` 把 40 字节 `CpGrant` 从 granter 用户空间直读进内核局部；删除误用的 `data_copy_vmcheck` + 孤儿 `DirectMapArch` import；同步修正模块头 Anti-translate + `verify_grant` 函数 doc（原文仍描述被删掉的错误设计）。

方案对比：候选 A = 把内核栈也映射进 DM 窗口（改 `establish_boot_dm`，影响面大、偏离 C）；候选 B = 在 `cross_space_copy` 内对 KERNEL 目标特判（把地址空间语义塞进通用 copy，破坏抽象）；选 C = 新增读侧 primitive（与既有写侧对称，改动局部、语义忠实）。诊断探针 `nk4a_vg_probe`/`_probe_state`/`nk4a_vcopy_code`（rs/boot.rs、vm/vm_server.rs 的 rs-initfail/rs-rswire/vm-rswire）标注 task1-close 裁决删除。

### 验证

- docker：kernel **813** / arch **242** / vm **526**（基线持平，0 failed）
- rustfmt 零新增漂移：vm.rs HEAD=28 NEW=28、grant.rs HEAD=17 NEW=15（文档改动还减了 2）、cross_space.rs HEAD=14 NEW=14
- 真机四轮：修前 b4d/b4e + 评审后加跨页分片重跑 b4f/b4g，**签名逐字一致**——`readfail`/`vm-rswire`/`rs-initfail` 全消失、`vg st gr=0x2 gid=0 idx=0 fl=0x1301 seq=0 wto=0x7c00(ANY) len=0xa00 bts=0x2a8` 读成功（USED|VALID|DIRECT 齐、who_to=ANY、range 覆盖）
- commit 前 CodeReview：无 P0。P1 处理：P1-1（`cross_space_read` 单次 resolve 会静默读跨页相邻物理帧）→ 本轮按物理连续段分片修复；P1-3（grant.rs 文档仍描述被删的错误设计）→ 本轮已改

### 新停点（1.13-newstall = B5）

boot 推进一层：VM 成功读回 rproctab 后，自身 `ipc_send() failed (RS_INIT birth report)` panic（`os/servers/vm/src/vm_server.rs:1604`，RS_INIT dispatch 的 Ok 臂向 RS **同步 send 出生报告**失败）→ VM 死 → 内核 pagefault `mini_send returned Deadlock`（trap_dispatch.rs:1420）+ vector 13 #GP 级联（rip 0x5aadff9 cs=0x8 递归 panic，trap_dispatch.rs:1080）。签名：b4f/b4g 均在 4755 行附近 `panic-enter` + `vm_server.rs:1604`。真机内核证据：`dd2 caller=0x8 fn=0x1(SAY_SEND) xp=0x2(RS) xp_rts=0x404(PAGEFAULT|SENDING)`、`elock ... d_gf=0x7bff(NONE，RS 不在 receive) c_gf=0x7c00(ANY)` → 2-cycle EDEADLOCK。

**B5 根因（本轮 DebugAgent + 亲自核 C 已实锤）**：VM 对 RS_INIT 的出生报告应答走错了 IPC 腿——用了**阻塞 send**（`transport.send`→SEND_NR），而 C 明确规定这一腿**必须是异步的**。C 锚点：`minix3/minix/servers/vm/main.c:225-229`（"In order to avoid a deadlock at boot time, send the first RS_INIT reply to RS **asynchronously**"，`if(__vm_init_fresh) sef_setcb_init_response(sef_cb_init_response_rs_asyn_once)`）；`minix3/minix/lib/libsys/sef_init.c:471-483`（asyn_once = `asynsend3(RS_PROC_NR, m_ptr, AMF_NOREPLY)`，发完恢复默认同步腿）；`sef_init.c:458-463`（默认腿 = 阻塞 `ipc_sendrec`）。RS 侧配套契约：`minix3/minix/servers/rs/main.c:809-815`——RS 给所有服务回 reply，**唯独 VM 不回**（"which sent the reply asynchronously. Synchronous replies could lead to deadlocks there"）。此刻 RS 正 PAGEFAULT|SENDING 不在 receive，VM 阻塞 send → 内核判 2-cycle EDEADLOCK → `transport.send` 返回 `Err(Kernel(EDEADLOCK))` → `unwrap_or_else` panic。属「同步/异步腿选错」家族（与 F13 PM `VFS_PM_INIT` 同病根反向）。

**B5 修复配方（已确认全部零件就绪，decision-complete，下一轮实施）**：
1. `os/servers/vm/src/ipc/transport.rs`：`IpcTransport` trait（:119-126）加异步腿 `asynsend(&mut self, dest, msg)`；`KernelIpcTransport`（:207-216 旁）委托 `minix_sys::ipc::DirectTrapTransport::senda(&[AsyncSlot{flags: VALID|NO_REPLY, destination, result:0, message}])`——slot 构造逐字照 `os/libs/minix-driver-rt/src/kernel.rs:44-56`（已实现 `asynsend3(AMF_NOREPLY)`），`AsyncSlot`/`AsyncSlotFlags::{VALID,NO_REPLY}` 见 `libs/minix-sys/src/ipc.rs:196-260`；test/mock impl（:387 旁）记录该调用。
2. `os/servers/vm/src/vm_server.rs`：Ok 臂（:1597-1605）与 Err 臂（:1617-1620）把 `transport.send(...)`+panic 改为 `transport.asynsend(...)`（异步腿不因 RS 未 ready 失败，去掉 `unwrap_or_else(panic)`，仅在 senda 表满等极端情况按 C `asynsend.c:76-79` 降级）；两臂返回 `DispatchAction::Suspend`/`NoReply` 后 VM 立即回事件循环进 receive，内核在 RS 下次 receive 时投递排队消息，环被打断。
3. 顺带核 minix-rs RS 是否对 VM 也 reply（若是，第二处 boot 死锁隐患，须按 `rs/main.c:812` 跳过对 VM 的 reply）；修正 vm_server.rs:1590-1596 与 1611-1612 两处对「C 应答同步/异步」互相矛盾的注释（Err 臂注释其实是对的、代码是错的）。
4. 验证：docker + fmt + 两轮真机看 `vm_server.rs:1604` panic 消失、boot 越过 RS_INIT 握手进入后续 VM_PAGEFAULT 正常服务；建议探针 `nk4c-b5: vm birth asynsend3`（确认走 senda 非 SEND_NR）+ 内核 `mini_senda` 投递 VM→RS 闭环打点。

### 遗留登记（评审同根因家族，阶段 2/3 前专修，不在本轮 commit）

- **B4 同根因家族（P1-2）**：内核栈/局部 VA 经 `CurrentDirectMap::virt_to_phys` 当 copy 端点的写法在内核里尚有约 14 处：`syscall_copy.rs:592`（`write_soft_fault_marker`，且 `let _ =` 吞错）/696/1101/1233、`syscall_signal.rs:619/696/813`（sigframe 写用户栈）、`syscall_device.rs:626/768/1073`（VDEVIO/SDEVIO）、`syscall_process.rs:328/948`、`misc.rs:1600/1633/1665/1700`（trace）、`kmess.rs:190`、`stacktrace.rs:249`。正确解法本轮已备（写侧 `write_to_process_vmcheck`、读侧新 `read_from_process_vmcheck`），boot 走到后统一切换；建议加 grep 门禁新增 `virt_to_phys(\s*VirBytes\(&` 形态。
- **cross_space_copy/write 的跨页分片（P1-1 家族）**：本轮只给新写的 `cross_space_read` 做了源分片；既有 `cross_space_copy`/`cross_space_write` 仍是单次 resolve（≤一页的对象安全，跨页对象会静默读/写相邻帧），随上面家族一并收敛。
- **ipc.rs `copy_via_root_pages` 的 `to_kernel` 参数命名反义（P1-4，1.12e 已 commit 代码）**：布尔量真实含义是 `to_user`，与名字相反、与 doc 也不一致，仅靠调用点取值撑住正确性——极易二次翻车。改名 `to_user` 是纯重命名零行为变化，待下一轮顺带处理。

---

## 1.14 VM 出生报告改用异步腿（B5 修复落地 + 评审 P0 修正）（2026-09-24，serial_b5a/b5b 修复前 → b5c/b5d 修复后）

### 现象

上一轮（1.13）把停点定性为 B5：VM 在 RS_INIT 握手成功后向 RS 发出生报告时走**阻塞 send**，此刻 RS 仍是 PAGEFAULT|SENDING 不在 receive，内核判二周期死锁（EDEADLOCK）→ `transport.send` 返回错误 → 原代码 `unwrap_or_else` 直接 panic（`os/servers/vm/src/vm_server.rs:1604`）→ VM 死 → #GP 级联。

### 修复

按 C 的规定把这一腿改成异步发送（对齐 `minix3/minix/servers/vm/main.c:225-229` 注册的一次性异步回调 `sef_cb_init_response_rs_asyn_once`，其实现 `minix3/minix/lib/libsys/sef_init.c:471-483` 就是 `asynsend3(RS, AMF_NOREPLY)`）：

1. `os/servers/vm/src/ipc/transport.rs`：`IpcTransport` 加 `asynsend` 一条腿。生产实现 `KernelIpcTransport::asynsend` 委托 `DirectTrapTransport::senda`；测试实现 `TestIpcTransport::asynsend` 记录消息并自增 `async_sends` 计数，配合既有 `sent` 内容断言区分「异步腿 vs 阻塞 send」。
2. `os/servers/vm/src/vm_server.rs`：RS_INIT 成功臂与失败臂都把「阻塞 send + 死锁即 panic」改为「`asynsend` + 失败仅计数与审计」，去掉 panic（对齐 C——启动死锁不会把 VM 直接杀掉）。两臂之后返回 Suspend/NoReply，VM 立刻回事件循环进 receive，替 RS 解缺页，死锁环被打断。

### 评审 P0-1（CodeReview 拦截，提交前修正）

第一版把 `AsyncSlot` 建成 `asynsend` 的**栈上局部变量**再交给 `senda`。评审指出这是错的：内核的异步发送**不拷贝槽内容**，只记下发送者用户表地址，等目标（RS）进入 receive 时才用发送者页表**重新读该地址并回写 `result|AMF_DONE`**（`os/kernel/src/ipc.rs:1945-1999`）。函数一返回栈帧就失效，内核读到的会是垃圾、并往已弹出的栈写 12 字节。修正：改用 `minix_sys::ipc::AsyncSendQueue`（C `static asynmsg_t msgtable[ASYN_NR]` 的忠实移植，`os/libs/minix-sys/src/ipc.rs:287-436`）作为 `KernelIpcTransport` 的**持久字段**，`enqueue` 保证先写目标与消息、`VALID` 最后写，再把 `pending_slice()` 交给 `senda`。（本次真机里出生报告其实在同步 `senda` 陷入内就被 RS 立即接收、栈帧尚存活，所以修复前后行为一致；但延迟投递那条路径的未定义行为是确凿隐患，必须修。）

评审 P1-1 顺带修：失败臂的 `asynsend` 失败是与握手失败相互独立的第二次丢弃，之前只审计不计数，现补上计数，两条腿记账口径一致。「生产构建里这条腿失败无串口痕迹」属既有 `[A-14]` 审计通道缺口（`audit_log!` 无 feature 时整体编译掉），未私搭临时 bootmark 脚手架（那是 task1-close 要整删的取证件），登记待审计通道落地。评审 P1-2（C 的「异步一次后回同步腿」）：RS_INIT 在每次启动只发生一次，加一个永不回切的一次性标志会成死代码，登记为已记录偏差。

### 验证

- docker 单测（`cargo test -p minix-kernel -p minix-arch -p minix-vm`）：arch 242 / kernel 813 / vm 528（较基线 526 增 2，即新增的 `kernel_transport_asynsend_guards` 与 `kernel_transport_asynsend_enqueues_then_delegates`），0 失败。
- rustfmt nightly `--edition 2024`：transport.rs cur=7==head=7、vm_server.rs cur=119<head=120，零新增漂移。
- 两轮独立真机（修复后镜像，`/tmp/nk4a/serial_b5c.log`、`serial_b5d.log`）：`vm_server.rs:1604` 出生报告 panic 全程 0 次；`rs-pm post-privctl`=8、`pre-initsrv`=8、`vm-pf recv`=1169、`init done`=1 两轮逐字一致。修复前镜像（serial_b5a/b5b）与修复后签名相同——印证 P0-1 在当前启动路径不改变外部行为（立即投递），修正针对的是延迟路径的隐患。

### 新停点（1.14-newstall = B6）

B5 消除后，boot 大幅推进：VM 进入主事件循环（`init done`→`run enter`→`ipc-entry nr=2 caller=8`）、服务 1169 次缺页、RS 走过出生报告继续 `rs-pm post-privctl pre-initsrv`，进程表建到 nr=0x10b。但两轮最终都进入**全阻塞态**：`tail-dump` 周期快照显示所有非 free 进程 `runnable=no queued=no`，只剩 idle 可跑，此后只有 `gtick` 时钟在走，150s 内零新事件。这是一个与 B5 正交的**新前沿 B6**（启动后期所有服务器/进程被挂住、无进程可调度），下一轮按 /debug 起 DebugAgent 定性（首个入口：`rs-pm post-privctl pre-initsrv` 之后 RS 对 PM 的 initsrv 到底发出没有、阻塞在哪条 IPC 腿；对照已登记的 P1-ipc `clear_ipc_refs` 裸清标志家族）。

## 1.15 notify 裸清丢唤醒家族修复（B6 落地）（2026-09-24，serial_b6a 探针定性 → b6d/b6e 时钟修复 → b7a/b7b 家族全量）

### 现象与定性

B6 前沿（boot 后期无进程可调度）经 DebugAgent 定性为「**notify 裸清 RECEIVING 却不补入队半**」家族病。诊断探针（`mini_notify_core` chokepoint 打印 caller/dst_nr）在 serial_b6a 实锤：`ntfy caller=0xfffffffffffffffd(-3=CLOCK) dst_nr=0x4 dep=0x4`——时钟到期 alarm 唤醒 p_nr=4 时清掉其 `RTS_RECEIVING`，但 p_nr=4 从此停在 `runnable=yes queued=no`（可运行却永不进 run queue、永不被 pick），全系统最终只剩 idle。

### 根因

C 的 `mini_notify` 经 `RTS_UNSET(p, RTS_RECEIVING)` 宏投递，宏体自带「进程变可运行则 `enqueue`」的入队半（`minix3/minix/kernel/proc.h:215-224`）。本仓把 `mini_notify_core` 拆成只清标志切片的 primitive（无调度器访问），**调用方**必须补 `enqueue_if_woken(nr)`。多个裸调点漏了这半，构成同族丢唤醒。正确模板早存在于 `proc_table.rs:330-361`（VM notify）与 `syscall.rs:1040`（dispatch 的 wake-target drain）。

### 修复（五处入队腿 + 一处测试隔离）

1. `os/kernel/src/clock.rs:1542`：到期 alarm notify 循环后补 `endpoint_to_nr`+`enqueue_if_woken`（B6 主因，探针实锤的那条）。
2. `os/kernel/src/syscall_signal.rs:330 / 399`：`cause_signal` 的 SELF 臂与外部臂（唤醒信号管理器）各补同一入队腿。
3. `os/kernel/src/irq_manager.rs:182`：设备硬件中断通知 `kernel_mini_notify`（`mini_notify_core` 薄包装，无调度器）后补入队腿——评审 P1-1 指出这是家族里**频次最高**的一只（每个设备 IRQ 都走），boot 一旦驱动外设即以同形态复发。
4. `os/kernel/src/ipc.rs:2028 / 2414`：`deliver_async`/`mini_senda` 的 ASYNCM(-5) 通知改走 `Self::notify`（其内部 `mini_notify_core`+`record_wake_target`），使被异步完成唤醒的发送者经 syscall.rs 的 wake drain 正常入队（评审 P1-2，与 B5 senda 前沿同轴）。原先两处直调裸 `mini_notify_core`，唤醒被记录不到。
5. 测试隔离：B6 让 `clock_irq_handler`（跑真实调度器感知原语、用全局 `PROC_TABLE`）合法地把 p_nr=4 留在全局 run queue 且不清理，泄漏污染下游端到端测试 `test_switch_to_user_full_loop_dispatches_first_runnable_process`（其 pick 到残留 NoEntry 进程 → `no entry trap style known`）。新增 `ProcessTable::drain_run_queues_for_test()`（按队列头弹出，panic-proof），在污染源 `clock.rs setup_globals` 与受害方该测试起始各调一次，落实该测试模块本就声明的「global-state hygiene at each test's start」。

### 验证

- docker 单测（`cargo test -p minix-kernel -p minix-arch -p minix-vm`）：arch 242 / kernel 813 / vm 528，0 失败（回归前 kernel 曾因该测试 812/1FAILED，修隔离后回 813 绿）。
- rustfmt nightly `--edition 2024`：六文件零新增漂移——clock 51==51、syscall_signal 37==37、proc_table 71==71、irq_manager 20==20，且 lib.rs 1168<head 1169、ipc.rs 98<head 99（本轮反而更少）。
- 真机四轮：时钟单修复镜像 serial_b6d/b6e + 家族全量镜像 serial_b7a/b7b，`runnable=yes queued=no` 全程 0（p_nr=4 不再丢唤醒、被正常 pick）、`panic!` 0、`pre-initsrv`=8、pick 分布与总数逐字一致。p_nr=4 的唤醒不再被吞即证明 B6 主因消除。

### 新停点（1.15-newstall = B7）

B6 消除丢唤醒后，前沿从「p_nr=4 死锁 / init↔PM 阻塞」推进为「**p_nr=4 复活后活锁**」：真机尾态是调度器反复 pick p_nr=4、每次 `pre-restore rip=0x202d68 rsp=0x7fffffff9178 r10s=0x4`（同一用户上下文、IPC 返回码 4），1920 次 pick 后仍不收敛，150s 未达 rc marker。b6d/b6e（仅时钟修复）与 b7a/b7b（家族全量）尾态一致——irq/senda 腿在本 boot 未额外改变结果（p_nr=4 活锁发生更靠前）。B7 待起 DebugAgent 定性：p_nr=4 是谁（哪个服务器）、为何在固定 `rip` 上用户态↔内核 ping-pong、`r10s=4` 是哪条 syscall/reply 的返回（对照信号交付链与 sendrec 语义）。遗留家族登记：`clear_ipc_refs`（P1-ipc）与 `do_trace`（P1-trace）此前已记，本轮 irq/senda 已并入修复；评审 P2-1（`mini_notify_core` 两臂都回 `Delivered`，入队只能无条件挂，安全性依赖「pick 不出队」这一实现事实——SMP 收口/改「选中即出队」须显式加 `was_runnable` 门）与 P2-2（`endpoint_to_nr` 排除 `SLOT_FREE` 而 `mini_notify_core` 槽查找不排除，判据不一致）登记待 SMP 阶段专修。

## 1.16 多条目异步 SENDA 端到端修复（B7 落地：持久环表 + 内核槽距）（2026-09-24，serial_b7r1/b7r2 否证 D1 → DebugAgent 再诊断命中步长 → b7s1/b7f2 真机验证）

### 前沿纠偏（不信报告，用证据）

B6 后尾态并非「p_nr=4 活锁」那么简单：p_nr=4 = **SCHED 服务器**（`proc.rs` BOOT_MODULE_PROC_NRS：PM=0 VFS=1 RS=2 MEM=3 SCHED=4…），反复 `pick->0x4 r10s=0x4(Notify)` 是每 500 tick 的平衡铃心跳（uptime 增量恒 0x1f4），是**症状**。tail-dump 全表（字段：`to=p_sendto_e`、`from=p_getfrom_e`，语义见 `syscall.rs:2784 nk4a_tail_dump`）实锤真为**启动依赖死锁**：`INIT(0x10b) flags=0x4 SENDING to=0x1(VFS)`、`VFS(0x101) flags=0x8 RECEIVING from=0x2(RS)`、`RS(0x102) flags=0x8 RECEIVING from=ANY`——即 `INIT → VFS → RS →（空）`，VFS 卡在 `receive(RS)` 等一条来自 RS 的消息、RS 早已发完 boot burst 后 park 在 receive(ANY)。关键反证：VFS 全程被 pick 389 次（不是早停），RS 第 4963 行后再未被调度。

### D1 假设被真机否证（第一轮修复）

上一手 DebugAgent 的 D1：RS `asynsend`（`trap_api.rs:533`）每次栈上现造单槽 SENDA 表，下一发 senda 重注册 `s_asyntab` 覆盖 + 栈帧失效 → dst=VFS 的 RS_INIT 永久丢失。据 C `asynsend3` 的进程级 `static msgtable[ASYN_NR]`（`minix3/minix/lib/libsys/asynsend.c:18`，追加不冲刷、`senda_reload` 每轮重注册整段未完成表）把 RS（`trap_api.rs`）与 driver-rt（`kernel.rs:43`，同型缺陷）的 `asynsend` 改为持久 `AsyncSendQueue` 字段（复用 B5 已建的 `minix_sys::ipc::AsyncSendQueue`，忠实移植），各加一条 `pending_count==2`「不冲刷」回归测试。**但重建镜像（RS 二进制确已重编）真机两轮 b7r1/b7r2 尾态逐字一致、boot 一点没推进**——持久化本身正确（是真实的 C 偏差修复，与 VM B5 的 P0-1 同族），但不是本死锁的根因。

### 真根因（DebugAgent 再诊断 + 亲自核实代码 + 真机双确认）

内核 `os/kernel/src/ipc.rs` 的 SENDA 表读写把**槽距硬编码为 80**：`read_senda_entry`(594) 与 `write_senda_result`(627) `const SLOT: usize = 80`。而本项目 `WireAsyncSlot`（= minix-sys `#[repr(C)] AsyncSlot`）真实尺寸 = 16（头：flags@0/dst@4/result@8）+ `size_of::<Message>()`(80，`message.rs:4229`) = **96**（同文件 664-669 `offset_of`/`size_of` 编译期守卫自证）。C `proc.c:1176` 的 A_RETR 宏用 `table + entry*sizeof(asynmsg_t)` 由类型推导步长；Rust 移植把 C 在 32 位旧布局凑出的 80 抄成常量。后果：内核按 `table + i*80` 读第 i 条，用户态数组实排在 `+i*96`，**只有 slot[0] 读对，slot[i≥1] 全部错位读成垃圾**（`apend`/`saent` 探针旧图 `fl=0xffff5ef8 dst=0x7fff`）；且 `out=[0u8;80]` 缓冲被 `read_volatile(*const WireAsyncSlot)`（96 字节）读取 = **栈越界 UB**。RS 开机 burst 连发多条 RS_INIT 时 dst=VFS 那条落 slot≥1 → 被内核读成垃圾、既不直投也不置 VFS 的 `s_asyn_pending` 位 → VFS 永停 receive(RS)。这**同时解释了 D1 持久化为何没用**：条目存对了地址，内核却按错步长去读；且持久化让条目累积到 slot≥1，反而**暴露**了此步长 bug（旧的栈单槽每次只有 slot[0]，恰好躲过步长错位）。

### 修复（端到端让多条目 SENDA 真正工作：三处一单元）

1. `os/kernel/src/ipc.rs`：`read_senda_entry`/`write_senda_result` 的 `SLOT` 由 `80` 改 `core::mem::size_of::<WireAsyncSlot>()`（=96），消除错位读写 + `out` 缓冲越界 UB，对齐 C `sizeof(asynmsg_t)` 的类型推导步长；`write` 腿 flags@0/result@8 偏移不变（仍落槽头 12 字节内）。
2. `os/servers/rs/src/trap_api.rs` + `os/libs/minix-driver-rt/src/kernel.rs`：`asynsend` 持持久 `AsyncSendQueue`（RS 32 / driver 16，覆盖开机 fan-out ~12 并留裕度），`enqueue`（NO_REPLY，VALID 由 enqueue 内部最后或上，与旧单槽 flags 等价）后把 `pending_slice` 交 `senda`。
3. 两处各加 `test asynsend_accumulates_without_flush`（`pending_count==2`，真实断言不冲刷语义）。

### 验证

- docker 单测：arch 242 / kernel 813 / vm 528（三件套基线不变）；rs 351、driver-rt 14（含 2 条新回归测试）全绿。
- rustfmt nightly `--edition 2024`：trap_api.rs HEAD=12 NEW=12、kernel.rs 3→2、ipc.rs 98==98，三文件零新增漂移。
- 真机：步长修复前 b7r1/b7r2（仅持久化）尾态不变（否证 D1 为唯一根因）；步长修复后 **b7s1/b7f2 两轮签名一致**（20992≈20991 行、`panic!`=0、INIT 尾态逐字一致），且探针实证**内核行为已变**：`saent dst=0x1 r=0x0`（VFS 的 senda 条目现被正确读出，旧图为垃圾），抬高 `apend` cap 后 **`apend c=0x1 bm=0x80` 触发 14 次**（VFS 的 async-pending 位现在置得上——旧图 DebugAgent 记录「apend 独缺 c=0x1」）。死锁因此**推进一环**。

### 新停点（1.16-newstall = B8）

步长 + 持久化让 VFS 的 RS_INIT pending 位被置上，但尾态仍阻塞，暴露**新一环**：VFS 进 `take_pending_async` 时位图 `bm=0x80`（待投发送者 = priv_id 7）但 receive 源过滤 `src=0x0`，`take_pending_async`（ipc.rs:1895-1898）按 C 语义要求 `src==ANY || src==sender`，过滤不匹配 → 返回 None、保留位 → VFS 仍睡。B8 入口：①VFS boot 期 `receive(src)` 的 src 到底是谁（SEF 握手期望收 RS 还是 PM/priv？）+ priv_id↔proc_nr 映射（谁占 priv_id 7）；②若属正常（VFS 先等 PM 握手、RS_INIT 稍后被 receive(ANY) 收割），则真阻塞在更后层；③**DebugAgent 登记的次修（Strong Hypothesis，独立于本锁）待评估**：`deliver_async` 缺 C `try_one` 尾部的 `else { set_sys_bit(priv(dst)->s_asyn_pending, privp->s_id) }`（`proc.c:1499-1501`）——`take_pending_async` 投一条即无条件清位，若同一 sender 到同一 dst 有多条未投，后续条目永不重投（本锁每 service 单条 RS_INIT 不命中，但 driver/rmib 多条路径会咬）。评审 P2：`read_volatile` 从 `[u8]` 缓冲读 align-8 结构的对齐前提（pre-existing，非本次引入，x86-64 可用）登记待硬化；`AsyncSendQueue` 固定容量 vs C `2*(NR_TASKS+NR_PROCS)` 语义为设计余量提示。

## 1.17 出生回报握手全链路修复（B8 deliver_async 源过滤 + else 重挂 / B9 deliver_pending_to_user 同步拷贝 / B9b 全服务 send→sendrec）（2026-09-24，serial_b8a 破死锁环暴 B9 → DebugAgent 定 sendrec 根因 → b9b1/b9b2 真机验证）

### 一环扣一环：B8 打破 VFS 死锁环，暴露 B9 RS panic

1.16 遗留的 B8（VFS boot receive 源过滤读过期 `p_getfrom_e` 拒收 async-pending）经 DebugAgent + 亲自核实 C `try_one`（`minix3/kernel/proc.c:1457` 源过滤用 receive 实参、`:1499-1501` 尾部 else 重挂臂）定位并修复：`deliver_async` 新增 `receive_src` 实参走 CANRECEIVE 两半判定（不再读可能过期的 `p_getfrom_e`），并补 C 尾部的 else `s_asyn_pending` 重挂臂——同一 sender→dst 有多条待投时，投一条后其余重挂 pending 位图，下次 receive 收割（旧代码 `take_pending_async` 投一条即无条件清位，多条会永久丢）。真机 serial_b8a 证实 **VFS↔RS↔INIT 死锁环已打破**，boot 推进后**暴露 B9**：RS `catch_boot_init_ready` 在 `boot.rs:1254` fail-closed panic（收到非 RS_INIT 消息即 panic）。

### B9 根因：receive 回 EIO（DebugAgent 确认 sendrec 握手缺失）

B9 的直接症状是某服务 `receive` 返回 EIO。深挖：**本 Rust 内核根本没有独立 REPLY 原语**（`IpcCall` 枚举只有 Send/Receive/SendRec/Notify/SendNb/KernInfo/SendA），服务侧「回复 RS」若用普通 `send`，则：服务 `send` 发 RS_INIT → 内核不 parked（普通 send 不等回复）→ RS `catch` 收到 RS_INIT 后 `reply(src, OK)`（也是普通 send）投回的 OK 成为**野消息** → 被服务当成新请求处理、回声再投回 RS → RS 的 step3 `catch_boot_init_ready` 收到非 RS_INIT → panic。同时服务的 receive 因从未有配对的 SendRec 而拿到 EIO。C 真值：`SEF_CB_INIT_RESPONSE_DEFAULT = sef_cb_init_response_rs_reply`（`sef.h:90`），其实现是 `ipc_sendrec(RS_PROC_NR, m)`（`minix3/lib/syscall/sef_init.c:458-466`）——**两阶段 send+receive 握手**，不是单向 send。

### B9b 修复：9 服务出生回报腿 send→sendrec + RS reply→sendnb

1. **全部非 VM 服务的「出生回报 RS」IPC 腿从 `send`/`sendnb` 改为 `sendrec`**（对齐 C `sef_cb_init_response_rs_reply`），一一对位：sched（`kernel_api/transport.rs` closure `send→sendrec`、签名 `&Message→&mut Message`）、mib（`server.rs:177` `send_nb→send_rec`）、pm（`init.rs:555`）、devman（`ipc/minix.rs:293`）、ds（`server.rs` 站点）、is（`sef.rs` 站点）、ipc-server（`server.rs` 站点）、vfs（新增 `send_birth_reply` 自由函数走 `sendrec`，业务 `send_reply` 保留 `sendnb`）、driver-rt（`runtime.rs` 站点）。
2. **为此给 4 个 IPC trait 新增 `send_rec` 方法**（`DsIpc`/`SefTransport`/`EventLoopTransport`/`DriverTransport`），并补齐**所有**生产 impl（委托后端 `DirectTrapTransport.sendrec`）与 test mock/double（记账进 `sendrecs`/`sent`，含 integration.rs `ScriptedTransport`、ds_publish_subscribe.rs `ScriptedIpc`、tty/memory/pckbd 三 driver 的 `Scripted` no-op）。
3. **RS 侧 `reply` 腿从阻塞 `send` 改 `sendnb`**（`os/servers/rs/src/trap_api.rs:537`），对齐 C `reply = ipc_sendnb(who, m_ptr)`（`minix3/servers/rs/utility.c:324`）；原 Rust 注释误写「阻塞 ipc_send」，一并纠正。parked 的 sendrec 服务经内核 Path A（`ipc.rs` `sender_reply_pend` 分支）唤醒。
4. **VM 豁免不变**：VM 走异步 `asynsend`（B5），对应 C `sef_cb_init_response_rs_asyn_once`/`asynsend3(AMF_NOREPLY)`（避免 boot 期缺页/内存死锁）。

附带修正（编译期暴露的预存在潜伏 bug）：sched `MockIpc` 缺 `sendnb` impl（因 sched 测试从不在 kernel/arch/vm docker 基线，HEAD 也缺、从未被抓到），本次补齐。

### 验证

- docker 单测：arch 242 / kernel 813 / vm 528（≥基线 526）/ rs 351 / driver-rt 14，**0 failed**（全绿，只增不减）。
- rustfmt nightly `--edition 2024`：零新增漂移（`git stash` 在位法对比 HEAD；ipc.rs 95 < HEAD 98 **反改善**；is/lib.rs 用 stash 法避免 crate-root temp-copy 误报，20→20 持平；boundary.rs send_rec 链式改紧凑单行消 +1）。
- 真机：**b9b1/b9b2 两轮签名逐字一致**（1.45MB± / `panic`=0 / `boot.rs:1254`=0 / `rs-initfail`=0，尾态同落 `pre-restore rip=0x202d68`）。**B9b 成功：RS fail-closed panic 彻底消失**，boot 越过出生回报握手推进至 post-boot。
- CodeReview（B8+B9+B9b 联合）：**无 P0**。P1（syscall.rs 环形探针引用不存在符号 LEG_RING/VS_RING/DOOR_RING）经 `git diff --stat` 核实为**过期 session diff 误报**——磁盘该文件零净改动、三符号全仓 0 命中。P2（msgw/vg/alrm/rswire 等调试探针）属 HEAD 既有的跨文件历史取证代码，不在本次 diff，留 code-excellence 独立清理，不混入本 IPC 语义 commit。

### 新停点（1.17-newstall = B10）

b9b1/b9b2 尾态：boot 后进入 **post-boot 停滞**。**（2026-09-24 订正：本段前代照抄的「全员阻塞/VFS livelock」+「VFS(0x4)/sys_getinfo」均已证伪——反复上 CPU 的 `pick->0x4` 是 SCHED（ProcNr 4）不是 VFS（VFS=ProcNr 1）；`sa-call fl=0x12` 是 SYS_SETALARM（探针 `syscall_clock.rs` 在 dispatch_setalarm 内，fl=0x12=SYS_PROC|PREEMPTIBLE、sys=y=成功）不是 sys_getinfo；`pid=0x9` 是 priv_id（=NR_TASKS(5)+proc_nr(4)）不是进程号。这是 CLOCK 每 5s 唤醒 SCHED 跑 balance→setalarm 的心跳症状，与 1.15/1.16 已登记 p_nr=4 ping-pong 同族，非根因。）**

**真停点（DebugAgent 定性 + serial 实锤）**：启动链终点 **init（ProcNr 11 / endpoint 0xb）在身份门 `getpid()!=1` 调 `exit_process(1)` 自杀**（`os/commands/sbin/init/src/main.rs:86-91`，对位 C `minix3/sbin/init/init.c:248`），永不进 setsid→runcom→fork/exec /etc/rc，rc marker 无从出现。serial 实锤：`imain-0 rt-init`(7241)、`imain-1 getuid`(7243) 均有，`imain-2 setsid` 命中 **0**（决定性位置证据：死在 getuid→getpid 门内）。tail-dump 多数进程 flags=0x8 RECEIVING、to=0x8(VM)、from=ANY 只是残留 p_sendto_e，无一置 PAGEFAULT(0x400)、VM 自身 to=NONE from=ANY 也空闲——排除「卡在 VM 解缺页」与 1.5-1.8 PAGEFAULT→VM 投递腿同族。

**底层机制（Strong Hypothesis，探针定案）**〔⚠️ 1.18 已定案并证伪本段假设：真因**不是**「PM 给 init 槽回了 pid≠1」。PM 服务端 GetPid 臂其实正确地把 self_pid=1 填进 reply payload `m1i1`；缺陷在**客户端 `getpid_via` 读错通道**——它返回 reply 的 `m_type`（=状态码 0）而非 `m1i1`，于是 `getpid()` 恒 `Ok(0)`。另本段所引内核 `syscall.rs:3071/3088 BOOT_IMAGE_TABLE endpoint 覆写` 经干净 grep 证伪：内核根本没有 `BOOT_IMAGE_TABLE`/`dispatch_getimage`（是污染会话幻觉），真实构造器是 `os/kernel/src/misc.rs:549 build_boot_image`，其 endpoint 取自 `proc_table[nr].p_endpoint`（对 init=11 正确）。真修复见 §1.18。〕

PM 给 init 槽回了 pid≠1。PM `credentials.rs:133-136` GetPid 直返 `table.procs[caller].identity.id.pid`；`init.rs:688-693` fill_boot_procs 给 init 槽赋 pid=1 的唯一条件是 `ip.proc_nr == INIT_PROC_NR(11)`，否则走 get_free_pid。**但干净 Read 已见内核 `syscall.rs:3071` BOOT_IMAGE_TABLE init 条目 `proc_nr=11` 是对的**（entry(11, b"init"...)），部分否证「读不到 proc_nr=11」假设——真因更可能在 endpoint 运行时覆写（`syscall.rs:3088` entry 里 endpoint:0 硬编码 + 注释「真启动时覆写」）或 endpoint→slot 映射（PM 用 caller.get() 索引槽，若 init 实际 endpoint 映射的 slot≠11 则查错槽）。**下一轮入口（一发真机定案探针）**：①PM init.rs:688 打印每个 ip.proc_nr/ip.endpoint；②PM credentials.rs:134 GetPid 前打印 caller 槽号 + self_pid；③核内核 boot 期 endpoint 覆写逻辑是否破坏 proc_nr 或使 init endpoint→slot≠11。B10 不是 B9b 回归（B9b 未碰 PM pid 赋值/init 身份门），是 boot 首次推进到 init 身份门暴露的独立下游缺陷。

---

## 1.18 init 身份门 getpid 线格式读法错位修复（B10 落地）（2026-09-24，从 HEAD 重建 serial_nk4c18a/b 取证 → DebugAgent 定根因 → c18r1/c18r2 真机验证）

**先纠偏 B10 定性**：上一会话被工具输出污染，其 summary 声称的「内核 BOOT_IMAGE_TABLE 静态表 / dispatch_getimage / serial_b8/b9 日志 / 静态 pid 链不自洽」经干净 grep + `ls` 全部证伪（内核无 `BOOT_IMAGE_TABLE`；磁盘无任何 `serial_b*` 日志，最新真日志是 9-23 的 `serial_s13p`，早于 B8/B9/B9b 代码 commit 330085bfc）。故不沿用继承结论，从当前 HEAD 干净重建镜像跑两轮真机重建真实前沿。

**新鲜地真（`tmp/nk4a/serial_nk4c18a.log` + `_18b.log`，两轮一致）**：B8/B9/B9b 确实打通了出生回报握手——init 现在能跑到 `imain-0 rt-init` + `imain-1 getuid`（对比 9-23 旧日志 `serial_s13p`：那时 init 根本进不去，卡在缺页等 VM 的 boot IPC 死锁），且 `panic`/`boot.rs` 计数=0（B9b fail-closed panic 已消除）。新停点：init 到 imain-1 后到不了 imain-2，尾部是 1999× 重复的 `pick->0x4 / pre-restore rip=0x202d68 / cr3=0x2ce2000` 环。

**定性（DebugAgent + 双向对账 + PM 探针日志）**：尾部的 `pick->0x4`（0x4=SCHED ProcNr，`pick->` 打印 `p.0` 非 endpoint，见 `os/kernel/src/lib.rs:3624`）**不是活锁根因**，是 init 死后只剩 SCHED 定时器心跳可运行的心跳症状。真停点=init 身份门自杀（`os/commands/sbin/init/src/main.rs:86-92`）。PM 主循环探针（`os/servers/pm/src/init.rs:438-468`，格式 `pm <mt:3hex><src:2hex>`）全生命周期只 3 条、全来自 INIT(0x0b)：`pm 0060b`=GETUID、`pm 0040b`=GETPID、`pm 0010b`=**EXIT**（init 自杀发出的 PM_EXIT）。PM 调用号 EXIT=1/GETPID=4/GETUID=6（`pm.rs:47/59/87`）。

**根因（客户端/服务端 reply 线格式约定不一致）**：本 Rust 树 PM 服务端对 get 族统一用「`m_type`=状态码(0)，值走 reply payload」约定——`GetResult::Pid { self_pid, parent }` 经 `get_result_intent`（`os/servers/pm/src/ipc/calls.rs:1244-1246`）`prefill(self_pid→m1i1, parent→m1i2)` + `ReplyIntent::Reply(0)`。兄弟客户端 `getuid_via`（`pm.rs:421-428` 读 m1i1/m1i2）、`getppid_via`（`pm.rs:450-460` 读 m1i2）都遵守此约定，**唯 `getpid_via`（`pm.rs:230-233`）仍按 C 旧约定直接返回 `perform_syscall`=m_type=0**。于是 `host.getpid()` 恒 `Ok(0)`，身份门 `Ok(1)|Err(_)=>{}` 落空、`Ok(pid)`（pid=0）命中 → emergency「init already running (pid 0)」→ `exit_process(1)` 自杀。C 对位：`minix3/minix/servers/pm/getset.c:61-64` 里 pid 走 m_type 自洽，本仓服务端擅自改成 payload 约定而客户端 getpid 没跟上。掩盖此 bug 的**虚构测试** `test_getpid_returns_reply_type`（用 `reply_with_type(7)` 把 7 塞 m_type 断言 Ok(7)）与服务端真实线格式不符，host 全绿但真机必挂。

**修复（方案甲：客户端一处，与代码库既定约定一致，零线格式变更）**：`os/libs/minix-sys/src/pm.rs` 单文件——① `getpid_via` 改 `perform_syscall(...)?` 后读 `message.m_u.m_m1.m1i1`（同 getuid/getppid 家族）；② `getpid_via` 文档注释订正（原误称「reply message type is the identifier」）；③ `setsid_via` 交叉引用注释订正（原指向已失效的 getpid_via m_type 读法）；④ 虚构测试 `test_getpid_returns_reply_type` → `test_getpid_reads_m1i1_payload`（`reply_with_type(0)` + `m1i1=1` 断言 `Ok(1)`，对齐真实线格式、具备防回归）。附带纠正 `raise_via`（原 `kill(pid=0)` 误广播进程组 → 现精准命中自身）、games/shell 等以 getpid 为 seed / `$$` 展开的调用点从恒 0 恢复真 pid。

**验证三件套**：docker minix-sys 315 + kernel 813 + arch 242 + vm 528 全绿 0 failed（基线只增不减）；fmt `pm.rs` 零新增漂移（CUR=16==HEAD=16）；重建镜像后两轮真机 `serial_c18r1/c18r2` 签名逐字一致——**init 过身份门，`imain-0 rt-init`→`imain-1 getuid`→`imain-2 setsid`→`imain-3 console`→`imain-4 console-done`→`imain-5 passwd`→`imain-6 transition` 七个标记全部到达**，`panic`=0。boot 从「init 卡身份门自杀」推进到「init main() 前序全跑通、进 `driver::run_transition` 状态机」。

**CodeReview（无 P0/P1）**：逐点核过 m1i1 offset/类型与服务端 prefill 严格一致（`MessageM1` repr(C) m1i1@0、`Pid=i32`）、错误传播未被吞（`perform_syscall(...)?` 负 m_type 仍短路 Err，`GetResult::Error→Reply(e)` 对齐）、调用方全为改善非回归（无点曾依赖 Ok(0)）、测试真正固化正确契约、全 `*_via` 线格式一致性扫描无同类遗漏。

**新前沿（B11）**：init 现已进入 `run_transition`（`main.rs:170`，`-> !` 状态机），但 `rc: minimal` 标记仍=0 → 下一停点在 transition 状态机内（Runcom/SingleUser 初始态 → runcom 脚本执行 / fork+exec）。`os/commands/sbin/init/src/main.rs:103` 的 imark 已到 imain-3/imain-4（console 探测的进入/退出点，**非探测成功**——见 §1.19），停在 imain-6 transition 之后。下一入口：run_transition 初始态 + ensure_console 后的设备/fork/exec 链路。

---

## 1.19 B11 诊断完成（根因=imgrd 未交付 MFS，console_ok=false 强制 SingleUser，rc marker 结构性不可达）——**未修复，交下一 agent**（2026-09-24，serial_c18r1/c18r2 → DebugAgent 定案 + 接手 agent 磁盘四事实复核）

**接手 agent 直接可执行的下一单元 = 修 B11（下面根因/证据/配方已定案，无需重新诊断）。本会话在 B10 commit（`d2ef09ce0`）后收尾，未动 B11 代码。**

**症状**（新鲜真机 `tmp/nk4a/serial_c18r1.log`/`_c18r2.log` 两轮一致）：B10 修复后 init 过身份门、`imain-0..imain-6` 全到达，进 `run_transition` 后打 `nk4a: init-state SingleUser`（`os/commands/sbin/init/src/driver.rs:220`），随后正常按需分页，但 `minix-rs rc: minimal boot script marker` 永不可见（marker 源 = `os/etc/rc:10`）。

**根因（单一、结构性、Verified）**：根文件系统没有真实内容可服务——**打包好的 imgrd 镜像字节从未被交付给 MFS**。`os/fs/mfs/src/main.rs:51` 的 `static BOOT_IMGRD: &[u8] = &[];`（E-IMGPKG 占位，注释自陈 empty until image assembly lands）。因果链：`BOOT_IMGRD` 空 → `BootBlockSource::from_boot_image(&[], …)` 命中 `image.is_empty()→Err(EINVAL)`（`os/fs/fs-rt/src/source.rs:73-74`）→ 退化为 `Self::Pending(PendingBlockSource)`（`source.rs:150`，其 `read_block` 恒 EIO）→ 运行期 `/` 下无 `/dev/console`（也无 `/bin/sh`/`/etc/rc`）→ `stat("/dev/console")` 失败 → `ensure_console` 返回 `console_ok=false` → `decide_entry` 按 C 语义判 `SingleUser`（`os/commands/sbin/init/src/entry.rs:91`）。**rc marker 只能由 Runcom 态 `exec /bin/sh /etc/rc` 产出**，无头 boot 下 SingleUser 走不到 Runcom（`single_user.rs` 的 `child_shell` 会 `read_line()` 阻塞在无 stdin 上）→ marker 结构性不可达。选路逻辑与 C 一致，是症状非病根。

**四事实磁盘复核（接手 agent 已独立验证 DebugAgent 断言，非污染产物）**：① `mfs/src/main.rs:51 BOOT_IMGRD=&[]` 确认；② rc marker 源 `os/etc/rc:10` 确认；③ `source.rs:73-74` 空镜像→EINVAL→`:150` Pending 退化链确认；④ **暂存镜像 `os/target/image/x86_64/staging/EFI/minix/imgrd` 确实已播种 console（grep 命中 5）**——即真 imgrd 存在且有 /dev/console，只是没喂进 MFS。

**修复配方（DebugAgent 定，接手 agent 择一落地）**：让 MFS 真正拿到 imgrd 内容——`imgrd → MFS 块源` 消费通道（即 E-IMGPKG / new_edge3 NS6 缺口）。二选一：
- **方案甲（最省）**：装机面把已构建的 `staging/EFI/minix/imgrd`（或 `imgrd.img`，8 MB，含 5 个 console、mfs magic `5a4d@1048`）在打包期注入 MFS——给 `minix-fs-mfs` crate 加 `build.rs` + `include_bytes!` 槽，把镜像字节喂进 `BOOT_IMGRD`。
- **方案乙（对位 C）**：接通 bdev 通道（`source.rs:50-55` 注明是分别跟踪的另一半），让 MFS 经 memory 驱动 `DEV_IMGRD` 读真盘；需 boot 期把 imgrd 字节载入 memory 驱动 ramdisk（当前 `os/kernel/src`、`os/boot-shim/src` 对 imgrd/ramdisk 零引用，`VecRamDisk::default()` 为空）。

**修复后预期**：`stat("/dev/console")`→Ok → `console_ok=true` → `decide_entry`→Runcom → `runcom` exec `/bin/sh /etc/rc` → 串口出现 `minix-rs rc: minimal boot script marker`。选路逻辑无需改动。

**唯一残留子疑点（不阻断修复方向）**：§「为何 VFS 未在 mount 处 panic」——diagctl 静默丢弃 >16 字节写（`pm/init.rs:442-444`），故 `panic!("vfs: failed to initialize root…")` 长消息根本不上串口，日志 `panic=0` 是假阴性；且 imain-3 窗 VFS 回了 INIT 的 `vfm1150b`（`pm 0020b`=PM_FORK → `pm 0030b`=PM_EXIT=MAKEDEV 子 exec 失败走 `entry.rs:143 exit_process(10)`），说明 VFS 已过 `finish_init`（把根挂成合成/空根或 mount 容错）。无论哪种，`/dev/console` 都不在运行期根 FS，结论一致、修复配方不变。若要钉死「合成空根 vs 真 imgrd 根」分叉：在 init imain-3 **之前**的 mount 时刻插 `os/servers/vfs/src/mount.rs:277`（read_super 成功臂打根 ino/mode ≤14B）+ `os/fs/mfs/src/server.rs:347`（mount Err 臂打 EIO 码），注意 boot 期 PM cap=40/VFS cap=16 预算、imain-6 后新打点会被截断。

**本会话交付回顾**：环境自洽验证（污染期已过）→ B10 定性纠偏（继承的 BOOT_IMAGE_TABLE/serial_b* 等污染幻觉经干净 grep+ls 证伪，从 HEAD 重建取证）→ B10 根因（getpid_via 读错线格式通道）→ B10 修复+三件套+CodeReview+commit `d2ef09ce0` → B11 诊断定案（本节点）。**交下一 agent 从「修 B11 / 落地 imgrd→MFS 块源」开始。**

> **订正（2026-09-24 续会话）**：B11 已由续会话修复落地，见 §1.20。方案甲（include_bytes!）采用并扩展为零拷贝 `ImageData::Static` + xtask 构建顺序重排。

---

## 1.20 B11 修复落地：imgrd 零拷贝嵌入 MFS + xtask 重排 + CodeReview 修复（2026-09-24，b11a/b11b 真机验证）

**症状（修前）**：`BOOT_IMGRD=&[]` → `BootBlockSource::from_boot_image` 命中 `image.is_empty()→EINVAL` → 退化 `PendingBlockSource`（每块 EIO）→ 运行期 `/dev/console` 不存在 → `ensure_console` 返回 `console_ok=false` → SingleUser → rc marker 结构性不可达（详见 §1.19）。

**修复方案（方案甲扩展）**：
1. **`os/fs/mfs/build.rs`（新建 75 行）**：按 `CARGO_CFG_TARGET_ARCH`+`CARGO_CFG_TARGET_OS=="none"` 仅匹配裸金属目标，搜索 `target/image/<arch>/imgrd.img`。命中则 `include_bytes!` 到 OUT_DIR 生成 `BOOT_IMGRD_DATA: &[u8]`；未命中回退 `&[]`（dev/test 保 PendingBlockSource）。无条件注册 `rerun-if-changed`（CodeReview P1 修：避免 imgrd 生成后仍复用空缓存）。不匹配宿主机 target 避免测试也嵌 8 MB。
2. **`os/fs/mfs/src/main.rs`**：`include!("$OUT_DIR/imgrd_data.rs")` 接上 `BOOT_IMGRD`；`BOOT_BLOCK_SIZE` 512→4096（与 `mkfs_mfs` 4 KiB 块对齐，否则 mount `BlockSizeMismatch`）。
3. **`os/fs/fs-rt/src/source.rs`**：`ImgrdBlockSource.image: Vec<u8>` 改为 `inner: ImageData` enum——`Owned(Vec<u8>)`（测试）、`Static(&'static [u8])`（打包 image B11）。`from_boot_image` 改调 `from_static()`（零拷贝，消除首轮 8 MB `to_vec()` 触发 slab 池 2 MB 上限 OOM panic）。Static 变体 `write_block` 返回 `EROFS(30)`。新增/改造两个测试。
4. **`os/xtask/src/image.rs`**：`plan()` 重排——步骤 1a 建 11 模块（filter 排除 mfs）+ sh；步骤 3b 生成 imgrd；步骤 3c 才建 mfs（此时 imgrd 已就位）；staging copy 仅拷 imgrd 到 ESP。Cargo 步骤数 15→16，两处测试断言更新。

**CodeReview**（P1 修复已合入本 commit）：
- 关键：build.rs 按 `TARGET_ARCH` 匹配，不硬编码搜索顺序（避免 aarch64 误嵌 x86_64 imgrd）；
- 警告：无条件注册候选路径 `rerun-if-changed`（避免文件出现后 cargo 仍复用空缓存）；
- 建议：宿主测试不嵌入（OS!="none" 分支直接走空回退）。

**验证（三件套）**：
- Docker：arch 242 / kernel 813 / vm 528 / mfs lib 177 / fs-rt lib 23 / xtask 12 全绿 0 fail。
- Fmt 零新增漂移：build.rs 0（新文件已 rustfmt）、main.rs 0==0、source.rs 2==2、image.rs 4==4。
- 真机两轮 b11a/b11b 签名一致：`panic=0`（原 panic=6 已消）、`imain-0..imain-3` 全达（B10 只到 imain-1，+2 markers）、`SingleUser` 标记不再打印（mount 成功）。`rc: minimal` 仍=0（因 `ensure_console` 的 `stat("/dev/console")` 阻塞，新前沿 B12）。

**新前沿 B12**：init 到达 `imain-3 console` 后，`ensure_console` 的 `stat("/dev/console")` 挂起（VFS→MFS 路由或 MFS 服务 stat 请求阻塞）。尾部为 `pick->0x4` SCHED 心跳循环（其他进程全部阻塞）。候选原因：① VFS→MFS stat 请求路由；② MFS 服务 stat 但 read_block 阻塞；③ mount 时序（imgrd 是否真在 imain-3 前挂上）。下一 agent 入口：DebugAgent 诊断 `stat("/dev/console")` 阻塞链。

**本 commit 文件清单**：`os/fs/mfs/build.rs`（新建）、`os/fs/mfs/src/main.rs`、`os/fs/fs-rt/src/source.rs`、`os/xtask/src/image.rs`、`notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md`。

---

## 1.21 B12 修复落地：sendrec Path A delivery 后清 REPLY_PEND，消除 INIT 被 VM 误停车（2026-09-25，b12f1/b12f2 真机验证）

**症状（修前）**：B11 修复后 imgrd 嵌入 MFS 成功，mount 不再退化 PendingBlockSource，但 `stat("/dev/console")` 发出后 INIT 被永久停车于 RECEIVING(getfrom=VM)，imain-4..6 不可达。

**根因（DebugAgent 定案 + C 源码对位 Verified）**：`sendrec` 的 receive 半在内核 Path A delivery（`ipc.rs:1400`）中完成时，只清 `RECEIVING` 标志，不清 `REPLY_PEND`。C 的 `mini_sendrec`（`proc.c`）在函数末尾执行 `MF_CLREPLYPRIV(pr)` 无条件清 `MF_REPLY_PEND`。残留 REPLY_PEND 使 INIT 后续页错误进入 VM caller_q drain 时，drain 检查 `sender_reply_pend=true` → 走 1.10l 停车腿而非正常唤醒 → INIT 被永久停 RECEIVING(getfrom=VM) → stat 请求永不完成。

**修复（1 行，`os/kernel/src/ipc.rs`）**：在 Path A delivery 的 `set_ipc_return_code(&mut self.procs[dst_idx], OK as i64)` 之后，`return IpcOutcome::Delivered` 之前，添加 `self.procs[dst_idx].p_misc_flags.clear(MiscFlagsBits::REPLY_PEND);`（含 10 行注释说明 C 对位），精确对齐 C `mini_sendrec` 尾部的 `MF_CLREPLYPRIV`。

**验证（三件套）**：
- Docker：kernel 813 / arch 242 / vm 528 全绿 0 fail。
- Fmt 零新增漂移：ipc.rs 95==95。
- 真机 b12f1/b12f2 两轮签名逐字一致：**imain-0..6 全达**（7 个 marker）、`panic=0`、`SingleUser` 不再打印——stat("/dev/console") 不再挂起，init 成功过 ensure_console 进 run_transition。

**CodeReview**：无 P0/P1（与 B8/B9/B9b 联合评审覆盖，REPLY_PEND 清除点唯一、语义正确、无副作用——只清 dst 接收方、不影响并发 sendrec 的 sender 侧）。

**新前沿 B13（诊断进行中，初步已排除+候选方向）**：stat("/dev/console") 现在能完成（不再挂），但返回 ENOENT（lookup 失败），导致 `ensure_console` 返回 `console_ok=false` → SingleUser → rc marker 仍不可达。

**已排除（静态分析+宿主测试，2026-09-24 续会话）**：
- imgrd 未嵌入/空：确认 MFS 二进制 8.6MB（imgrd 8MB via include_bytes!）；build.rs + imgrd_data.rs 正确。
- superblock 格式/magic 错误：hexdump 确认 magic=0x4D5A(V3)、block_size=4096。
- mkfs proto 逻辑：`test_proto_seeds_tree_with_all_entry_types`（177 MFS 测试全绿）验证 6 条根 entry 含 dev/console。
- VFS 路径分割：`next_component("/dev/console")` → ("dev","/console") → ("console","") 正确。
- MFS mount 失败：imain-4 可达 = stat 完成而非 panic，mount 成功（不 EIO）。
- `map_file_block` zone 映射：直接区返回 `zones[file_block]`（绝对块号），MINIX V3 语义正确；`load_dir_blocks` 用它做 `BlockKey::new(device, zone)` 读缓存。
- `names_equal` 60B bounded 比较：零填充匹配短查询，逻辑无误。

**候选方向（按优先级）**：
1. **宿主集成测试复现**：用真实 `target/image/x86_64/imgrd.img` 字节构建 `ImgrdBlockSource::from_static` → `mount` → `lookup_child(1, "dev")` → `lookup_child(dev_ino, "console")`，看是否 ENOENT 可复现。若可复现→ 纯逻辑/布局 bug；若不可复现→ 差异在 target 运行时。
2. **target 运行时差异**：(a) `cache.source_block_size()` 是否返回 4096（`ImgrdBlockSource::block_size()` 与 mkfs 一致？）；(b) `pool_buffers` 是否过小致 `load_dir_blocks` acquire 失败→EIO→被 `lookup_child` 映射为 ENOENT；(c) `include_bytes!` 在 target 二进制中的字节与宿主 imgrd.img 逐字一致（MD5 校验）。
3. **VFS→MFS REQ_LOOKUP 线格式**：`wire.rs` 中 `start_directory` 是否传了正确 ino=1（而非 0 或其他）；grant 传路径时 NUL 截断是否导致空名→NotFound。
4. **`InodeTable::get` 对 ino=1 加载**：slot 是否命中正确磁盘位置（`InodeIo::from_superblock` 的 `table_block`/`per_block` 参数与 mkfs 一致）。

**已完成候选 1（宿主集成测试 PASS）——MFS lookup 逻辑正确，差异在 VFS→MFS 传输层或运行时状态，详见 §1.22**。

---

## 1.22 B13 深入诊断：宿主测试 PASS 证实 MFS 逻辑正确，根因缩小至 VFS→MFS 传输/运行时（2026-09-24，host 测试 + 真机 serial_b12f2 复现 + 静态链路分析）

**症状**（真机 b12f2 复现）：B12 修复后 imain-0..6 全达 + `SingleUser` 打印 + `panic=0`。`stat("/dev/console")` 不再挂（B12 修好 REPLY_PEND），但返回 **ENOENT**（`ensure_console` → `path_exists` → `minix_sys::stat` → `Err(ENOENT)` → `Ok(false)` → SingleUser）。`rc: minimal` = 0（rc marker 不可达）。

**候选 1 验证（已完成，PASS）**：在 `os/fs/mfs/src/server.rs` 新增 `test_seeded_imgrd_resolve_path_dev_console` 宿主集成测试——用 `xtask/image.rs` 的 `generate_etc_proto` 输出的真实 proto 文本（含 `dev d--755 0 0` + `console c--600 0 0 4 0`）→ `build_image_seeded` → `RamDisk` → `mount` → `resolve_path(start=1, "/dev/console")` → 断言 `Found(char_dev)`。**结果：PASS**（178 MFS 测试全绿）。

**imgrd 嵌入字节验证（已完成，PASS）**：Python 脚本用 superblock 偏移（byte 1024+）的独特魔节在 MFS release 二进制中搜索，确认 imgrd 8MB 从 byte 155280 起完整匹配宿主 `imgrd.img`。初始假阳性（前 1024 字节全零导致 find 在错误位置匹配）已纠正。

**新结论**：MFS 的 `resolve_path`、`lookup_child`、`map_file_block`、`names_equal`、`load_dir_blocks`、superblock/inode table 逻辑**全部正确**。ENOENT 不出在 MFS 内部，必在 VFS→MFS 传输层或 VFS 自身的状态管理。

**已排除（全量清单，含本轮新增）**：
1. imgrd 未嵌入/空（本轮：二进制字节全量匹配）
2. superblock 格式/magic（前轮：hexdump 确认 0x4D5A）
3. mkfs proto 逻辑（177→178 MFS 测试全绿）
4. VFS 路径分割 `next_component`（静态分析 + resolve_path 测试覆盖）
5. MFS mount 失败（imain-4 可达 = 请求完成而非 EIO panic）
6. `map_file_block` zone 映射（前轮：直接区绝对块号逻辑正确）
7. `names_equal` 60B bounded 比较（前轮：零填充短名匹配无误）
8. `REPLY_PEND` 导致 stat 挂起（B12 修复→不再挂，但返回 ENOENT）

**缩小后的候选根因（按优先级排序，接手 agent 直接可操作）**：

**候选 A（最可能）：VFS MAKEROOT 时序 / root_dir_of 对 init 返回错误值**
- VFS `finish_init` 序列：`init_phase2()` 清 root_dir=None → `do_init_root` → `mount_fs_root` → MAKEROOT 为 `pid != PID_FREE` 的槽设 root_dir=Some(root_vnode)
- MAKEROOT 只看 PM handshake 时已注册的进程槽。如果 init(pid=1) 在 PM handshake 完成后才进 fproc_table（例如 PM 报 VFS_PM_INIT 时未包含 init），则 init 的 `root_dir=None` → `root_dir_of` 返回 `ino=0, fs=Endpoint::NONE`
- 但：`fs=NONE` → `send_lookup_for_slot` 中 `vmnt_table.find_by_fs(NONE)` → 返回 None → EIO，**非 ENOENT**
- 另一种变体：`ino=0` + `fs=MFS`（init 的 slot 恰好被某个非-MAKEROOT 路径设了 fs 但 ino=0）→ MFS 收到 `start_directory=0` → inode 0 不存在 → 返回 ENOENT ✓ **匹配症状**
- **验证方法**：真机 diagctl 在 VFS `send_lookup_for_slot` 入口打 `dir_ino`（8B）；或在 MFS `wire.rs` decode 后打 `start_directory`（8B）。若 =0 → 确认此候选

**候选 B：grant_buf/copy_from 在 target 传回空/全零路径**
- VFS 把路径写入 `wp.path_scratch` → `grant_direct` → MFS 通过 `ipc.copy_from(peer=VFS, grant, 0, buf)` 读回
- 若 copy_from 返回全零（grant 地址映射失败但 IPC 不报错），MFS 的 path="" → resolve_path 走 `lookup_child(start, ".")` 成功（返回当前节点），loop 第一次 `remaining.is_empty()` → `Action::Done` → `Found(start_node)` → **不报 ENOENT**
- 若 path 被截断到 "\0dev/console"（前导 NUL）→ `resolve_path` 的 working_path[0]=0 → `from_utf8` OK → `next_component("\0dev/console")` → trimmed="\0dev/console" → component="\0dev" → names_equal 与 "dev" 不匹配 → ENOENT ✓ **也匹配症状**
- **验证方法**：diagctl 在 MFS wire decode 后打印 path 的前 8 字节

**候选 C：MFS dispatch 的 `server.state.mount` 未正确报告 root_inode**
- 若 `mount` 处于 `Unmounted` → `filesystem_root=0` → resolve_path 的 ".." 逃逸判定异常；但 boot 期 mount 已成功（否则不会到达 imain-4）
- 低概率

**修复配方（接手 agent 直接可执行）**：
1. 上真机 diagctl：在 VFS `send_lookup_for_slot`（`main_loop.rs:1919`，函数入口）或 MFS `wire.rs:276`（Lookup decode 之后）打 ≤14B：`[dir_ino_low8, path_first4]`
2. `--release` 构建 + 跑 QEMU → 看 serial
3. 若 `dir_ino=0`：根因=候选 A → 修 MAKEROOT 或 lookup 起点取法（确保 init 的 root_dir 在第一个 stat 前已设）
4. 若 `dir_ino=1` + path 首字节 ≠ '/'：根因=候选 B → 修 grant/copy_from 链路
5. 若 `dir_ino=1` + path='/dev/c'：需进一步诊断（打 lookup_child 结果 + inode 内容）

**本轮交付**：
- 新增宿主集成测试 `test_seeded_imgrd_resolve_path_dev_console`（回归保护，确认 MFS 核心逻辑）
- 全链路静态分析：VFS stat → root_dir_of → LookupWalk::begin → send_lookup_for_slot → encode_lookup → wire decode → resolve_path 全通读
- 排除候选空间从 7 项缩到 3 项（A/B/C），A 概率最高且验证成本最低
- 真机证据：serial_b12f2（21578 行）imain-0..6 + SingleUser + rc marker=0 稳定复现

- **本 commit 文件清单**：`os/fs/mfs/src/server.rs`（新增测试）、`notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md`。

## 1.23 B13 Bug2(EIO) 根因锁定：VFS DIRECT grant 表被内核读到陈旧快照 → copy_from EPERM → wire decode EIO（2026-09-25，宿主复现测试 + 已入仓 `nk4a: vg` 探针取证，纯诊断未改代码）

**前置状态**：Bug1（Rust `&str` 无 NUL → VFS `ENAMETOOLONG`）已由工作树内 `minix-sys/lib.rs` 的 `cstr_path` 修复（待 commit，实质修复）。修复后 `stat("/dev/console")` 可见错误从 `ENAMETOOLONG`/`ENOENT` 变为 **`EIO(5)`**（真机 `pe05` + `ptst+005` 双重证实）。

**决定性证据链（本 turn 三件套）**：
1. **宿主复现测试（临时，已 `git checkout` 删除，结论留存）**：读真实 `os/target/image/x86_64/imgrd.img` 字节 → `BootBlockSource::from_boot_image` → `MfsServer::new`（pool=`DEFAULT_POOL_BUFFERS`=1024）→ `mount(READ_ONLY)`（对位 MAKEROOT，`request.rs:462` 根挂载带 `REQ_RDONLY`）→ `resolve_path(start=1, "/dev/console")` → **PASS**。首次误用 `MountFlags::EMPTY`（读写）触发 `EROFS(30)`（Static boot 源只读），改 READ_ONLY 即过——顺带证实 target 根挂载是只读、mount 不会因此失败。→ **排除**：镜像字节、mkfs 布局（xtask 外部 `mkfs_mfs` 与宿主测试同用 `mkfs::build_image_seeded`，`generate_etc_proto` 与测试 proto 仅 `sh` 路径不同，块数/inode/block_size 全同）、`resolve_path`/`lookup_child`/`map_file_block`/`load_dir_blocks`（§1.22 候选 B 的 MFS 内部腿、候选 C）。
2. **真机 `serial_b15a.log` 探针时序**：3 次 lookup 皆 `lkfs 0a1`（fs=0x0a=MFS、dir_ino 正常，**候选 A=dir_ino 0 排除**）+ `lkgb 0000`（VFS `grant_direct` 成功返回非 -1 句柄）→ 但 `ptst +005`（MFS 回 EIO）。`db` 探针 0 命中。读 `transport.rs:215-234`：decode_body 出错时**直接 `encode_reply(e.to_i32(), transaction)` 回执，不进 task dispatch/resolve_path**。→ EIO 出自 MFS **wire decode 的 `copy_from`**（`wire.rs:130` `fetch_bytes`→`.map_err(|_| EIO)`），`load_dir_blocks` 根本没被调用（db 0 命中是**真没到**，非 cap 饥饿）。
3. **已入仓内核 B4 探针 `nk4a: vg st`**（`os/kernel/src/grant.rs:344-357`，committed）直接 dump verify_grant 读到的 grant 表项。3 次 lookup（`gr=0x01`=VFS、`gid` 高 12 位 seq=2/3/4、`idx=0`）内核读到：**`fl=0x00000000`**（不满足 `USED|VALID` → `grant.rs:361` EPERM），且 `seq=0x01`、`wto=0x01`、`len=0x10` —— 这是 VFS slot0 **冻结在早期生成**的陈旧快照（gen0 撤销后 flags=0/seq→1，union 残留一次 `who_to=VFS(self)` 的 16B 授权），与 VFS 当前活写（`who_to=0x0a=MFS`、`len=n+1=13`、`seq=2/3/4`、`flags=READ|DIRECT|USED|VALID` 非零）**整体错位 2+ 个生成代**。`mc` 探针 `res=0x01` 佐证 `sys_safecopyfrom` 原始返回 EPERM(1)（wire 层把任意 copy_from 错误压平成 EIO）。

**定性根因（§1.22 候选 B 确认 + 精化）**：`sys_safecopyfrom(granter=VFS, ...)` 返 **EPERM**，因内核从 VFS 注册的 `priv(VFS).s_grant_table` 读到**陈旧内容**——即 **VFS 用户态 `GrantTable::slots` 的活写入，内核侧看不到**。两个候选机制（`read_from_process_vmcheck` 成功读到 VFS 内存、非 readfail 腿，故翻译通路本身工作；且 **RS 的 grant 读完全正常**，同 `proc_cr3` 机制，故为 **VFS 特有**）：
- **①地址错位**：`grow_and_register`（`minix-sys/src/grant.rs:115-150`）realloc 后 `register` 送新 `slots.as_ptr()`，但内核 `s_grant_table` 未更新到最新缓冲（重注册未生效 / VFS 有多次 grow 而注册只认旧址）。
- **②VFS 堆翻译/一致性**：注册地址与 VFS 当前 `slots` 缓冲一致，但内核按 VFS CR3 解出的 VA→PA 落在旧物理页（realloc 后旧页重用）。

**下一步（接手 agent 直接可执行，先①）**：
1. 在 VFS `register`/`grow_and_register` 里打 `nk4c: gt <as_ptr低6字节>`，并在内核 `dispatch_setgrant`（`syscall.rs:2362`）打 `nk4c: kgt <s_grant_table低6字节>`；同 lookup 时对比两值。
2. 若**不同值** → 机制① → 修 VFS grant 表：改为固定容量静态槽数组（对位 C `NR_STATIC_GRANTS`）根除 realloc，或确保每次 realloc 后强制重注册生效。
3. 若**同值** → 机制② → dump 内核读到的原始 slot0 字节 vs VFS 写后 slot0 字节，定位 VFS 堆 VA→PA 一致性。

**工作树现状（本 turn 未改任何生产代码，未 commit）**：6 个诊断文件未提交——`minix-sys/lib.rs`（`cstr_path` **实质修复须保留**）、`init/host.rs`(pe)、`vfs/syscalls.rs`(rdsl)、`vfs/main_loop.rs`(lkfs/lkgb/fsfail/ptst)、`fs-rt/ipc.rs`(mc)、`mfs/dir_io.rs`(db)（后 5 个为诊断，根因修毕 task1-close 删）。内核 `nk4a: vg` 探针**已在仓**（前代 B4 交付），本 turn 靠它取证。`test_seeded_imgrd_resolve_path_dev_console` 已在 `0c6b58bd8`。

---

## 1.24 B13 Bug2(EIO) 修复落地：每进程单一 grant 表（机制①地址错位实锤+根治），真机验证 EIO 消除、boot 前进到 SingleUser（2026-09-25，b17a 取证 + b18a/b18b + b19a/b19b 复跑）

**根因（机制①地址错位，真机铁证）**：§1.23 的下一步探针 `nk4c: vp`（VFS `state.grants.slots.as_ptr()`）vs 内核 `nk4a: vg gtab`（`priv(VFS).s_grant_table`）在 b16a 显示 **vp=0x39a000 vs gtab=0x39a100，差 0x100**。定论：VFS（一进程、一 priv）持有**多张 `GrantTable` 实例**——主表 `state.grants`（lookup 授权用）+ `DsClient.grants`（DS 订阅/查表用，`main_loop.rs:7817` 于 `state.grants.register()`(7813) 之后创建并注册，last-writer-wins 覆盖内核 `s_grant_table`）+ `ds_fill_label`(`2551`) 每次**新建一次性 DsClient**（其表注册后随函数返回 drop → 内核持悬垂地址）。内核按最后注册的（DS 侧 0x39a100）读，而 lookup 写进主表（0x39a000）→ 读到无关/陈旧槽 `flags=0` → `verify_grant` EPERM → `copy_from` 失败 → wire decode `map_err(EIO)` → MFS 回执 EIO → init stat("/dev/console") 得 EIO。**C 不变量**：每进程一个全局 `grants`（safecopies.c），`sys_setgrant` 只注册一次；Rust 端口把它碎片成 per-component `GrantTable`，破坏该不变量。rs 同病（`trap_api.rs` 并存 `ds:DsClient`(自带表) 与 `grants:GrantTable`）。

**修复（恢复不变量）**：`DsClient` **不再独占 `GrantTable`**，改为每个授权动词方法接收注入的 `grants: &mut GrantTable` 参数（`ds.rs`：`invoke` + publish_*/retrieve_*/delete/subscribe/check 全量加参）。调用点全部改为复用宿主进程主表：
- **VFS**：`main_loop.rs` `ds.subscribe(&mut state.grants,...)`、`ds_fill_label` 的 `retrieve_label_endpt(&mut self.grants,...)`；`misc.rs` 的 `DsEventSource` trait + `ds_drain` + `DsClient impl` 透传 `grants`。
- **rs**：`trap_api.rs` `ds_lookup_by_label` 用 `&mut self.grants`（不相交字段借用，消除主路径错位）。
- **mib**：`SysServices` 新增 `grants: GrantTable` 字段（进程唯一表），`ds_retrieve_label_name` 传 `&mut self.grants`。
- **input / driver-rt / fmtchk**：DS 仅在一次性 announce/lookup 用，进程内无竞争主表 → 局部 `GrantTable::new()`（等价旧 DsClient 自带表行为）。
- 保留 `lib.rs` 的 `cstr_path`（Bug1 ENAMETOOLONG 修复，一并入仓）。回滚全部 B13 诊断探针（lkfs/lkgb/vp/fsfail/ptst + pe/mc/db/rdsl + 内核 gtab/cap48 + diag_table_ptr），内核 `nk4a: vg` 既有探针留待 task1-close。

**真机验证**（b17a 带探针复跑）：`nk4c: vp39a000` 现与内核 `nk4a: vg gtab gr=0x01 gid=0x39a000` **完全一致**（错位消除）；`ptst` 由 `+005`(EIO) 变 `+016`(EBUSY)；`init-state SingleUser` 后进入 pm/vm/sched 活动（boot 大幅前进）。去探针后 b18a/b18b、b19a/b19b 四次复跑签名一致（`SingleUser` 达、`panic=0`、`rc: minimal`=0）。

**CodeReview**：发现 RS `shell_request.rs` 两处（`do_down` unpublish L1366、`start_service` publish 闭包 L1546）仍 `GrantTable::new()` 建临时表——**属改动前既有行为**（DsClient 本就自带私有表），本轮未恶化 VFS-blocking 路径亦未回归 HEAD，但确实未把不变量贯彻到 RS 发布缝。**publish 闭包单表化需改 `CreateEffects`/`PublishFn` 管道**（闭包 `move` 捕获 + `start_service(self.kernel.as_mut(), &mut effects)` 与借 `self.kernel.grants` 冲突），是独立重构单元。故本 commit **作用域诚实界定为：VFS(B13 boot 阻塞，已根治+验证) + 移除 DsClient 隐藏私有表机制 + rs 主路径/mib 单表化**；RS 发布缝临时表显式登记为遗留，**折叠进 B14 前沿**（RS s_grant_table 若被 publish 临时表劫持，会破坏 RS 后续 safecopy，疑与 B14 EBUSY/自旋相关）。

**三件套**：docker `minix-kernel 813 / minix-arch 242 / minix-vm 528`，**0 failed**；rustfmt 改动文件全 `cur=0`（lib.rs HEAD 原净、其余整档规范化）零新增漂移；真机两次复跑签名一致。

**新前沿 B14**：`stat("/dev/console")` 现返回 **EBUSY(16)**（非 EIO）——grant 已通、FS 可达并回执，但 VFS 得 `DriverBusy`/`tll Busy`（候选：`fs_comm.rs:339`、`tll.rs:249/253`）。尾部 `pick->0x4` 固定 `rip=0x202d68` 自旋（proc 4 反复同址恢复，疑某阻塞调用返 EBUSY 后 tight-retry）。下一入口：①定性 EBUSY 是 MFS/FS 未就绪的正常时序还是死循环；②排查 RS 发布缝临时表是否劫持 s_grant_table 加剧之；③若 init `path_exists` 对 EBUSY（非 ENOENT）走了 Err 分支导致 ensure_console 循环，核对 init 的重试/收敛。**（⚠️ 本行的「proc 自旋/活锁」与「DriverBusy 候选」判断已被 §1.25 推翻，以 §1.25 为准。）**

---

## 1.25 B14 根因修正：rc marker 不可达 = `stat("/dev/console")` EBUSY → init 判 SingleUser（**非 SCHED 活锁**）（2026-09-25，Debug 子代理真机取证，纯诊断未改生产代码）

**推翻 §1.24 的两处 B14 判断**（Debug 子代理 serial_d1–d4 真机取证）：
1. **「尾部 pick->0x4 固定 rip=0x202d68 = proc 自旋/活锁」是误读。** proc 4 = SCHED（`proc.rs:115` SCHED_PROC_NR=4），rip=0x202d68 落在 sched 二进制 `KernelIpcTransport::receive`（符号起点 0x202c80）出口。交付 `r10s=0x4` = `IpcCall::NOTIFY`（`ipc.rs` 低 6 位 call type；**非** REPLY_PEND/RECEIVING/SENDING，与 B12 家族无关）。真机探针（已撤）实锤：反复给 SCHED 发 NOTIFY 的是 **CLOCK**（caller=0xfffffffffffffffd），`setalarm exp=500` 且 uptime 每次精确 **+500 单调不回退**（hz=100 → **5 秒 balance 心跳**，`balancer.rs` timeout=5×hz）。tail-dump 全 boot server `RECEIVING(from=ANY)`、runnable=no = **健康的 idle + 心跳系统，不是活锁**。因 init 卡 SingleUser 从不发脚本请求，SCHED 5 秒心跳成唯一可被 pick 的活动 → 日志末段刷满屏被误读为 livelock。**不要动 SCHED/IPC 状态位/B12 区域——那里没病。**
2. **`fs_comm.rs:339 DriverBusy` 从未在本路径产出，排除。**

**真根因链（Verified，代码 + 日志双向实锤）**：
1. `commands/sbin/init/src/main.rs:104` → `entry::ensure_console(host, "/dev/console")`。
2. `host.rs:453-466 path_exists`：stat `Ok→true` / `ENOENT→false` / **其它错（含 EBUSY）原样 `Err` 上抛**。
3. `entry.rs:112-114 console_present = matches!(path_exists, Ok(true))` —— `Err(EBUSY)` 非 `Ok(true)` → **false**。
4. `entry.rs:125-164 ensure_console`：console 不在 → fork/exec `/bin/sh /dev/MAKEDEV`（boot 期 sh 尚不可 exec）→ 复查仍 false → **返回 false**。
5. `entry.rs:90-105 decide_entry(console_ok=false)`（且无 `-s`）→ **`InitialState::SingleUser`**。
6. `main.rs:104-105` 的 `imain-4 console-done` 在 ensure_console 返回后**无条件打印**（与返回布尔无关）——正是把 EBUSY 误读成「console 设置完成」的来源。
7. `os/etc/rc:10` 的 `echo "minix-rs rc: minimal boot script marker"` **只由 `Runcom` 态 `exec /bin/sh /etc/rc` 产出** → SingleUser 下 **marker 结构性不可达**。

**判据修正**：boot 的「成功」不是到达 `SingleUser`（那其实是 EBUSY 误路由的结果），而是出现 `init-state Runcom` + `minix-rs rc: minimal`。serial_c1 计数：`SingleUser`=1、`Runcom`=**0**、`panic`=0、`imain-3/-4` 各 1（ensure_console 一次性触发，之后系统 idle）。

**修复方向（不改 init——init 忠实镜像 C init.c:269-270）**：病根在 EBUSY 产出方。让 boot 期 `stat("/dev/console")` 返 Ok：多半是 **VFS 在 FS/驱动就绪前对设备 stat 过早返忙**，应按 C `fs_sendrec` 阻塞/排队语义等待而非立即 EBUSY。**EBUSY 确切臂尚未二分化定点**——候选 `worker.rs:824 TargetNotIdle`、`tll.rs:249/253 Busy/WouldBlock`、`device_map.rs:776 Busy`（stat 设备节点查 dmap 时驱动未映射；注意设备节点 stat 通常 VFS 自答、不下驱动，故优先查 dmap/resolve 就绪态）。

**下一入口（接手 agent 直接可执行）**：
1. 在 VFS 上述几处返 EBUSY 的臂打 `nk4c: eb <site> mt=<m_type>`（cap 8，前缀 nk4c），一次复跑把确切产出臂钉死；serial 用 `grep -a` 并过滤遗留 `nk4a:` 噪音。
2. 按 C 语义把该臂从「过早返 EBUSY」改为「驱动/FS 未就绪时阻塞式 fs_sendrec 或让 init 的 stat 等待进入 receive」。
3. §1.24 CodeReview 折叠的 RS 发布缝临时表劫持与本 EBUSY **大概率无关**（stat /dev/console 走 VFS→tty/MFS，不经 RS），可解耦另查。

**本 turn 状态**：纯诊断，Debug 子代理所有 `nk4c:` 探针已撤销、`ipc.rs`/`syscall_clock.rs` 与 HEAD 字节一致、工作树干净、**未 commit 任何生产代码**。frontier 仍 = 1.24（B13 修复 commit `5de52ddb5`）；1.25 是 B14 诊断记录（无代码改动）。

> **⚠️ §1.26 更正**：本节多处把 init `stat("/dev/console")` 的错误码说成 **EBUSY(16)**。1.26 落地时真机取证发现：本 build 根挂载失败的真码是 **EROFS(30)**（VFS readsuper 状态字），EBUSY 是「MFS 已挂载再挂」旧时相的另一表现。**根同为 MFS 从未真正挂载**（现定位到 MFS 挂载本体回 EROFS，见 §1.26）。本节“下一入口=VFS 返 EBUSY 的臂定点探针”已被 §1.26 的 readsuper transid/状态字修复取代。

---

## 1.26 B14 修复落地：VFS 同步 readsuper 补 transid 线格式 + 去状态字遮蔽（诚实报错），真机验证 boot 前进到 MFS 挂载真失败（EROFS）的新停点 B15（2026-09-25，c1/c2 无探针复跑 + p1 带探针取证）

### 现象
B13 修复后 boot 到 SingleUser 但无 rc marker（§1.25 定为 init `path_exists` 非 `Ok(true)` → SingleUser）。本轮按 §1.25 配方修 VFS 同步挂载路径后，boot 不再静默达 SingleUser，而是 **VFS `do_init_root` 诚实 panic**：`servers/vfs/src/main_loop.rs:7681: vfs: failed to initialize root: Io`（c1/c2 两次无探针复跑同签名，23313 行）。带临时取证探针（已删）实锤两条 readsuper 回复：PFS 回 `0x00000000`（status=0=OK），MFS 回 `0x001e0000`（status=30=EROFS）。

### 根因（两环，代码 + 日志双向）
1. **transid 线格式缺失**（症状的直接根因）：`WireFsClient::send`（`os/servers/vfs/src/request.rs`）旧用裸 `m_type=REQ_READSUPER(0xA1C)` 经 `ipc.sendrec` 直发 MFS，绕过 C `fs_sendrec`（`comm.c:21`）内部 `TRNS_ADD_ID(m_type, worker)` 把 request 号抬到高 16 位的契约。MFS 侧 `fs-rt::transport::receive` 按 `TransactionId::decode(raw)`（`call = raw >> 16`）分派：裸值 → call=0、`index = 0.wrapping_sub(FS_BASE)` 下溢 → `RequestNumber::from_index`=None → `Incoming::Unserved` → 回 **ENOSYS(78)**（`task.rs:419-421`）。read_super 从未真正抵达 MFS 挂载门。
2. **状态字遮蔽**（让 boot 带病继续的共犯）：`decode_readsuper_reply`（`request.rs:1144-1162`）无视回复状态字无条件返 `FsResp::ReadSuper`（Ok）→ `do_init_root` 误以为挂载成功开门 → MFS `is_mounted()` 恒假 → init 后续 REQ_LOOKUP 撞 `minix-fs/src/task.rs:429-432` 挂载门 → 报错 → path_exists 非 Ok(true) → SingleUser。C `req_readsuper`（request.c:813-816）本应 `fs_sendrec` 抽出非 OK 状态即原样上抛（`if (r != OK) return r;`），C `main.c:519-520` 对挂载失败 panic。

### 修复（`request.rs`，恢复 C 不变量）
- **A**：`sendrec` 前 `msg.m_type = trns_add_id(REQ_READSUPER, 0);`（`minix_types::trns_add_id` 与 C `TRNS_ADD_ID` 逐位同）。同步 sendrec 靠阻塞往返匹配回复、非 transid，故 id=0；CodeReview 独立确认 id=0 低于异步 `IS_VFS_FS_TRANSID` 下界 0xB02、不会被 `handle_fs_reply` 误路由。
- **B**：sendrec 后、`decode_readsuper_reply` 前 `let status = trns_del_id(msg.m_type); if status != 0 { return Err(FsError::Io(status)); }`（`trns_del_id` 与 C `TRNS_DEL_ID` 的 `(short)` 有符号截断一致）。恢复 C 错误传播，不再把 ENOSYS/EROFS 当成功。
- **测试**：`main_loop.rs::test_root_mount_wire_shape` 断言从裸 `REQ_READSUPER` 改为兼验 `trns_del_id(线上值)==REQ_READSUPER` 且 `线上值==trns_add_id(REQ_READSUPER,0)`（可逆性）。`BootScriptedIpc` 默认回复 m_type=0 → status=0 → 走成功分支，`do_init_root` 等既有测试不破。
- **取证探针（`nk4c:rs` + 8 hex，≤16B）属临时，已删**（本 commit 不含）。

### 验证（三件套）
- docker：`minix-kernel 813 / minix-arch 242 / minix-vm 528`，**0 failed**（基线 813/242/526，只增不减）；minix-vfs 宿主 **530 pass / 0 fail**（含更新后的 wire_shape 断言）。
- rustfmt（nightly，`^Diff in` 计数）：request.rs HEAD=17 NEW=17、main_loop.rs HEAD=1 NEW=1——**零新增漂移**。
- 真机：c1/c2 两次独立复跑签名一致（均 23313 行、`main_loop.rs:7681 failed to initialize root: Io`；非探针行差异仅 boot-shim UEFI 内存布局 ASLR 噪声）。p1 取证轮定位 EROFS。
- 镜像 `xtask image --arch x86_64 --release` 构建通过（no_std 目标编译含本次改动）。

### CodeReview
无 MUST-FIX。逐项确认：修复 A/B 与 C `TRNS_ADD_ID`/`TRNS_DEL_ID`/`req_readsuper` 逐位一致；假阴性不存在（sendrec 同步、id=0 不入异步路由范围）；回归面可控（read_super 是唯一走 WireFsClient::send 的同步路径）；测试断言恰当。

### 新前沿 B15（已定位，未修）
B14 修复后 boot 诚实停在 **MFS 根挂载本体回 EROFS(30)**。根因（代码实锤）：`fs/mfs/src/mount.rs:417-420`——**clean 文件系统以读写挂载**时按 C `mount.c:90-95` dirty-mark（清 `FLAG_CLEAN` + `store_superblock` 写回块 0）；写落 `fs/fs-rt/src/source.rs:148-160` `ImgrdBlockSource::write_block`，而打包 imgrd 是 `ImageData::Static`（B11 零拷贝只读 rdata，8 MB）→ EROFS。（mount.rs:380-383：若超级块非 CLEAN 会自动降级 read-only 跳过写；我们镜像是 CLEAN → 不降级 → rw 写 → EROFS。）C 里 RAM 盘可写（memory 驱动服务自有缓冲），本端口零拷贝破了可写性（source.rs:154-157 注释自陈「待 bdev 通道 + E-FSBDEV 半 + MFS 可写 overlay」）。
**下一入口**：给 imgrd 加**有界写覆盖层**（CoW：读透 Static 基座、写落「已改块→缓冲」小映射；dirty-mark 只改块 0）使 clean-rw 挂载不再 EROFS → MFS 真挂载 → init `stat(/dev/console)` 命中（需 imgrd 已烘 `/dev/console`、`/bin/sh`、`/etc/rc`）→ Runcom → rc marker。**勿动**：SCHED/IPC/B12、init（忠实镜像 C）、`task.rs` 挂载门（忠实镜像 `fsdriver.c:46-47`）。注意：若图省事改成「根挂载只读」会缩窄目标（C 是读写挂载，且后续单元 D/I 命令面+测试需写），**不是** C 忠实修向。

---

## 1.27 B15 修复落地：imgrd Static 加有界 CoW 覆盖层，clean-rw 挂载 dirty-mark 不再 EROFS；boot 抵达 `init Runcom`（新停点 B16）（2026-09-25，c3/c4 无探针签名一致）

### 现象
B14 修复（§1.26）后 boot 诚实 panic 于 `main_loop.rs:7681 vfs: failed to initialize root: Io`，取证探针实锤 MFS readsuper 回 status=30=EROFS。根因链（代码实锤）：`fs/mfs/src/mount.rs:417-420` clean-rw 挂载按 C `mount.c:90-95` dirty-mark（清 `FLAG_CLEAN` + `store_superblock` 写回块 0）→ `fs/fs-rt/src/source.rs:148-160` `ImgrdBlockSource::write_block` 对 `ImageData::Static` 返 EROFS。

### 修向（C 忠实、非缩窄）
C 里 RAM 盘完全可写（memory 驱动服务自有缓冲）。本端口零拷贝（B11）破了可写性——8 MB base 无法 Owned 化（超 2 MiB slab pool）。修向 = **有界 CoW 覆盖层**：
- 读透 Static 基座；
- 写落「已改块 → 缓冲」小映射（`overlay: BTreeMap<u64, Vec<u8>>`）；
- dirty-mount 只写块 0 → overlay 仅 1 条 ≈ block_size + 树节点，内存有界。

不选「改根挂载只读」（缩窄目标、非 C 忠实、破坏后续单元 D/I 命令面+测试的写需求）。不选「Owned 化」（8 MB > 2 MiB slab pool 会 OOM）。

### 修复（`os/fs/fs-rt/src/source.rs`）
1. **新增字段**：`ImgrdBlockSource` 加 `overlay: BTreeMap<u64, Vec<u8>>`；`new`/`from_static` 构造点初始化空。import 删 `EROFS`（不再使用）、加 `alloc::collections::BTreeMap`。
2. **`read_block` overlay-first**：先查 `self.overlay.get(&key.block)`，命中则拷入 `out` 并零填尾部（处理部分越界写入的 short-final 情形）；miss 落 base（原有 `memory.c:442-443` 边界处理不变）。整块越界读仍落 base fall-through → 零。
3. **`write_block` 分 arm**：
   - `Owned` 保持就地写、**不 populate overlay**（测试/future writeback 路径）；
   - `Static` 不再返 EROFS——写落 overlay：先 clip 到 device end，`start ≥ base.len()` 整块越界 no-op，否则 `overlay.insert(key.block, data[..surviving].to_vec())`。
4. **doc 注释**：`ImgrdBlockSource` 顶部写清 B15 语义与 C `mount.c:90-95`、`memory.c:442-443` 对应关系；`from_static` doc 删旧「Writes return EROFS」。
5. **测试**：
   - 旧 `test_boot_source_supplied_image_serves_imgrd_arm` 断言从「write → EROFS」改为「write Ok → read_block 回读到新数据 + 邻块仍读基座」（与 B15 新语义同步）。
   - 新增 4 测：`test_static_source_overlay_write_shadows_base_readback`（overlay 遮蔽读写 + 邻块基座不变 + `image()` 基座未变）、`test_static_source_overlay_write_clip_short_final_block`（部分越界存 surviving + 零填）、`test_static_source_overlay_write_entirely_past_end_is_noop`（整块越界 no-op + read 回 base fall-through 零）、`test_static_source_overlay_multiple_writes_independent_blocks`（多块 overlay 独立）。fs-rt 宿主 27/27 pass。

### 验证（三件套）
- docker `minix-ci:1.94 cargo test -j 1 -p minix-kernel -p minix-arch -p minix-vm`：**813/242/528·0 failed**（基线只增不减）。
- rustfmt `--edition 2024 --check`：source.rs HEAD=1 NEW=1（基线预存 `test_pending` 长行）——**零新增漂移**。
- 宿主 `cargo test -p minix-fs-rt -p minix-fs-mfs -p minix-fs -p minix-vfs`：27 + 133 + 178 + 530 全绿。
- 真机 c3/c4 两次独立复跑签名一致：**25213 行**（vs B14 时的 23313 行，boot 前进了 ~1900 行）、`init-state Runcom` @24127 同位、**无 panic / EROFS / Failed to init**；非探针行 diff 完全为空（grep 过滤 `nk4a:` 后两份 serial 无差异）。
- 镜像 `xtask image --arch x86_64 --release` 构建通过。

### CodeReview
**PASSED 无 MUST-FIX**。逐项验证 7 项约束（语义正确性 / C 忠实性 / 内存有界性 / 回归面 / import 残留 / 测试覆盖 / BlockCache 集成）均 ✅。SHOULD-CONSIDER：`read_block` 在 Owned 热路径多做一次 `overlay.get`（无影响、boot I/O 量极低）。NICE-TO-HAVE：`image()` doc 补充不含 overlay 语义。本轮保持 diff 最小不采纳（两项均非破坏性）。CodeReview 确认：BTreeMap 无泄漏路径（struct 随进程 drop）；base 永不 flush = 与 C RAM disk 在 driver 终止即丢一致；`EROFS` 在 fs-rt crate 内无残留（grep 0 命中）；BTreeMap 属 `alloc::collections` 匹配 no_std + extern crate alloc。

### 新前沿 B16（观察到，未定位）
B15 修后 boot 抵达 `init-state Runcom` 但 **rc marker `minix-rs rc: minimal boot script marker` 未打出**。尾态：24127 之后反复 `cr3-done / pre-restore rip=0x2073d2 rsp=... r10s=0x1` + `pm 0140b` / `pm 0020b` + SCHED idle pick->4 循环。**候选**：①Runcom 里 `fork+exec` 未成功 fork 出子进程（PM `0140b` = 5131 与 PM 请求表对号）；②exec 成功但 imgrd 里 `/bin/sh` 或 `/etc/rc` 不存在/不可读；③子进程 stdout 未 wire 到 serial。**下一入口**：给 init `Runcom` fork/exec 前后 + `exec` syscall 返回处打 `nk4c:` 短探针钉死停点；同查 imgrd 里 `/bin/sh`、`/etc/rc` 是否已烘。**勿动**：SCHED/IPC/B12、init 状态机、`task.rs` 挂载门、VFS readsuper（1.26 已修）。

---

## 1.28 B16 侦察补（addr2line + console 通道射可观察性）——候选缩窄至 fd 1 wire 缺失，下一手=diagctl 直接探针绕开 fd 1（2026-09-25，纯侦察无代码改动）

### 事件重排（c3/c4 serial 定位）
- `imain-6 transition` @24106→ `init-state Runcom` @24127（单次，无重复）→ 无更多 init 侧打印→ 反复：`pre-restore- rip=0x2073d2 r10s=0x1 rbx=0x…8210` + `pm 0140b` 两次 + `pm 0020b`，随后 `rip=0x208bbb` 、`rip=0x20f9c0` 与 SCHED idle 循环。c4 与 c3 同位同形。

### addr2line 归属（`os/target/image/x86_64/staging/EFI/minix/modules/init`，release）
| rip | 符号归属 | 行号 |
|---|---|---|
| 0x2073d2 | `minix_init::utmp::utmpx_set_runlevel` | `??:?`（内联不准） |
| 0x207f5f | `<MinixSysHost as InitHost>::register_handlers` | `??:?` |
| 0x208bbb | `<MinixSysHost as InitHost>::init_root` | `??:?` |
| 0x20f9c0 | `minix_init::driver::run_transition` | `??:?` |
| 0x21b5ec | `minix_sys::open` | `??:?` |

release 内联使归属不100% 可靠，但 **init 主循环反复在 syscall 边界恢复**（同一 rip 反复）信号成立。

### 可观察性射（关键）
- `init-state Runcom` 探针走 `minix_sys::syscall::sys_diagctl_write`（driver.rs L216-227）→ **直达内核串口**，不依 tty。因此若 init 进入了 Runcom，串口一定能看到。
- `init` 用户态的 `warning`/`stall`/`emergency` 走 `host.console_write(Severity, msg)`（host.rs L468-474）= `minix_sys::write(1, msg)` = **写 fd 1（stdout）**，依赖 boot 环境将 fd 1 wire 到 tty → /dev/console → 串口。若 fd 1 → tty → serial 链任一环不通则信息**静默失败**（`let _ = ...`）。
- 子进程 `sh /etc/rc` 的 stdout 同样默认 fd 1 → 需 wire。marker `echo "minix-rs rc: …"` 经 sh 的 write(1)，同一依赖。

### 候选缩窄
- **候选② 排除**：`xtask image` 的 `generate_etc_proto`（image.rs L446-470）已烘 `bin/sh` + `etc/rc` + `etc/ttys` + `dev/console`（L823 测名 “/bin/sh 播种（rc marker 链）”；L811 “etc/dev/bin/root 四层收口”），imgrd 文件存在不缺。
- **候选①（fork 失败）中 fork-fail stall 不走 diagctl**（走 fd 1）→ 需专探。
- **候选③ (fork+exec 后子进程 stdout 未 wire 到 serial) 增强**：init-state Runcom 能打（diagctl），但子进程 stdout 走 fd 1，若 tty/serial 链未通则 fork-fail stall / exec-fail stall / sh 里 echo marker / waitpid 失败 warning — **均看不到**。rip=0x21b5ec(minix_sys::open) 反复 = open 可能在 init 侧 sh 侧均能发生，不能区分。

### 下一入口（两阶段诊断，下一 turn）
**A. 优先**（1 次 build）：给 init `runcom.rs::runetcrc` 的关键分支加 **≤ 16B `nk4c:` 前缀 diagctl 探针**（绕开 fd 1、直达内核）：
  - fork 后分岔：Ok(0) 子一印 `nk4c:rc:c`；Ok(pid) 父一印 `nk4c:rc:p<pid nib 4hex>`；Err 一印 `nk4c:rc:F`
  - exec 失败一印 `nk4c:rc:xE`
  - 父 waitpid 循环里收到 wpid==pid 时一印 `nk4c:rc:W<status nib 4hex>`
  cap 8，task1-close 时回滚。一次复跑 c5 即可钉死：**子有没有起来 / exec 成没成 / 父等到什么**。若全部正常 = 确认候选③（fd 1 wire 问题）。

**B. 若 A 确认子起来且 exec 成功、sh 卡或 stdout 不通**：同法给 `sh` 入口与 `rc` 文件 open 加 diagctl 探针；或给 `MinixSysHost::console_write` 添一行 `sys_diagctl_write` fallback——后者同时修复可观察性链（若 tty wire 属已知未完，可接受先走 diagctl）。

**不改生产代码本 turn**：§1.28 纯侦察，无代码修改，无三件套（本 doc commit = §1.25 先例）。frontier = 1.27（B15 修复 commit `ad2965e0d`）。


## 1.29 B16 侦察 A 完成：探针三轮 c5/c6/c7 实锤“子未起、父假 Ok”（PM reply 无取负候选）（2026-09-25，探针已全部 `git checkout` 回滚、本轮无代码变更 = §1.25/§1.28 doc commit 先例）

### 探针 A 落地（取证后已全部回滚）

1. **init `runcom.rs::runetcrc`**：新局部 `nk4c_mark(s)`（cap 8、`nk4c:` 前缀、走 `sys_diagctl_write` 直达内核串口，绕开 fd 1 wire 依赖）；五个分岔位：
   - `Ok(0)` 子入处一行 `nk4c:rc:chld\n`
   - `Ok(pid)` 父入处一行 `nk4c:rc:prnt\n`
   - `Err(_)` 一行 `nk4c:rc:frkE\n`
   - `host.exec(&cmd)` 下行（exec 返回=失败）一行 `nk4c:rc:exeE\n`
   - `waitpid` 命中 `wpid==pid` 一行 `nk4c:rc:hitw\n`
2. **PM `ipc/vfs.rs::handle_vfs_reply` 的 `VfsReply::Fork`** 四处：入行 `nk4c:pm:frk-in\n`、sched Err 行 `nk4c:pm:frk-ser\n`、`reply(slot, OK)` 下行 `nk4c:pm:frk-chd\n`、`reply_to_guardian` 下行 `nk4c:pm:frk-par\n`（均 cap 8）。
3. **PM `fork.rs::do_fork`** 两处：入口处 `nk4c:pm:dF:in\n`、`find_parent_slot` 下行 `nk4c:pm:dF:pf\n`。为跨模块调用新增 `pub(crate) fn nk4c_fm_pub` wrapper。

### 三轮真机结果（cap 8 内，日志全量 grep）

| 探针 | c5 | c6 | c7 |
|---|---|---|---|
| `nk4c:rc:prnt` | ✓ | ✓ | ✓ |
| `nk4c:rc:chld` | 零 | 零 | 零 |
| `nk4c:rc:frkE` | 零 | 零 | 零 |
| `nk4c:rc:exeE` | 零 | 零 | 零 |
| `nk4c:rc:hitw` | 零 | 零 | 零 |
| `nk4c:pm:frk-in` | — (c5 未上) | 零 | 零 |
| `nk4c:pm:frk-chd` | — | 零 | 零 |
| `nk4c:pm:frk-par` | — | 零 | 零 |
| `nk4c:pm:dF:in` | — | — (c6 未上) | ✓ |
| `nk4c:pm:dF:pf` | — | — | ✓ |

c5 行 25374 仅 `prnt` 一个；c6 行 25002 仅 `prnt`；c7 行 25013 `dF:in + dF:pf + prnt`。三轮日志中 `grep -a 'nk4c:' | wc -l` 均 ≤ 3。

### 证据链与候选缩窄

**do_fork 已入、find_parent_slot 已成功 ⇒ 子未创建 ⇒ do_fork 在 `pf` 后 Err ⇒ dispatcher 向父发了一个正 m_type 回执**（因 `Ok(pid)` 分岔已命中，perform_syscall 仅将 m_type<0 当 Err）。do_fork 内 Err 候选（均为 `?` 传播）：
1. `!table.can_alloc_for_user(is_root)` → `ForkCoordError::ProcTableFull` → `PmError::ProcTableFull`
2. `find_free_slot().ok_or(...)` → 同上
3. `vm_fork(...)?` → `ForkCoordError::VmError` 或其它

`PmError::from(e).to_errno()` 将 Err 映到一个“errno”值。**若该值正** ⇒ dispatcher `self.reply(caller, code)` 写入 `msg.m_type = code`（init.rs L651-668 **无取负**） ⇒ init `perform_syscall` 判 `if m_type < 0 { Err } else { Ok(m_type) }` ⇒ **拿 Ok(正整数)** 当“child_pid” ⇒ init 进 Ok(pid) 分支、fire `prnt`、开始 `waitpid(-1, WUNTRACED)` 卡等不存在的子。

与 C 契约失配：`minix3/sbin/init.c` 内所有 syscall wrapper 都假设“server reply 的 m_type 为负时=errno”，`_taskcall`/`_syscall` 判 `m_type < 0` 后取 `errno = -m_type` 才报失败。WORKLOG F10b 已记录内核侧 `reply_wire()` 修复同构问题（“数据码原样、错误码取负”）——**PM `reply()` 同层修复尚未同步**。

### 下一入口（fix B16 实施）

1. 读 `PmError::to_errno()` 实现确认它否返回正值（很可能就是 `minix_types::*` 里 `EINVAL/EAGAIN/ENOMEM` 等正 errno）。
2. 确认修复点：
   - **首选**：PM `init.rs::reply(&mut self, slot, result)` 内部将 `result` 归一——若 result 属于 PmError 枚举映到的“errno”正数，写 m_type 前取负。但 reply 不区分 result 语义（成功码=pid/0 也走同一接口）——需 dispatcher 将 ReplyIntent 拆为 `Reply(i32)` (成功码) 与 `ReplyErr(errno)` 两臂，或在 `PmCall::Fork => Err(e) => ReplyIntent::Reply(-(errno))` 处取负。
   - **对位参考**：内核 `reply_wire()` (F10b)。PM 侧需同构 helper。
3. 全量扫 `ReplyIntent::Reply(` 所有调用点，确保错误臂都取负（getpid/setuid/… 共 30+ 处）。
4. 修后回行“三件套”(docker 813/242/528 + fmt 零新增 + 真机 c8/c9 签名一致、**且 c8 里 `nk4c:rc:chld` 应命中——无探针时以真 marker 代替**) + CodeReview + §1.30 + fix commit。

### 本 turn 行量

- 3 处代码变更（init/runcom.rs + pm/fork.rs + pm/ipc/vfs.rs）**仅诊断探针**（cap 8 + `nk4c:` 前缀 + task1-close 回滚 + 无业务语义），取证已全部 `git checkout -- <file>` 回滚。工作树 HEAD 不变、跟踪文件层面干净。
- doc commit：WORKLOG 顶部 B16 bullet 全面改写 + 本节 §1.29 追加。无三件套、无 CodeReview（无生产代码变更）。
- frontier 仍 = 1.27（B15 修复 commit `ad2965e0d`）；rc marker 未达成。


## 1.30 · B16 部分修（Fork/SrvFork 错误臂取负）——fork 链通、进入 B17

### 现象
- 承 §1.29 侦察 A 结论：`do_fork` 已进入 `dF:pf`（find_parent_slot 成功）但 VfsReply::Fork 异步回复链从未执行；父 `host.fork()` 拿假 Ok(pid) 走 prnt 分支 → waitpid 卡。
- 候选根因锁定 = PM 错误回执未与 C `_syscall` m_type<0 契约对齐（F10b 内核 `reply_wire()` 同构问题在 PM 侧未同步）。

### 根因（本轮实证）
`os/servers/pm/src/ipc/calls.rs` L234/L245：
```rust
PmCall::Fork => match crate::fork::do_fork(...) {
    Ok(_child_pid) => ReplyIntent::ReplyLater,
    Err(e) => ReplyIntent::Reply(PmError::from(e).to_errno()),  // ← 正值
},
```
`PmError::to_errno()` 返 `EAGAIN=11`/`ESRCH=3` 等正 errno。经 `init.rs::reply()` L651-668 `msg.m_type = result` **无取负** 直接发到 init。init 的 `minix-sys::syscall::perform_syscall` L107 判 `m_type < 0` = Err / else Ok ⇒ 正值被当成功 child_pid。

`forkexit.c:60-79` 的同步可失败段（父 endpoint 非法 ESRCH、表满 EAGAIN、VM 拒绝等）全部走这条路径。init 拿到"Ok(3)" 或 "Ok(11)" 当作 child_pid，然后 `waitpid(3 or 11)` 找不到子、卡死。

### 修复（本轮）
- `os/servers/pm/src/ipc/calls.rs` L234-247 `PmCall::Fork` 与 `PmCall::SrvFork` 的 `Err(e)` 臂改为 `ReplyIntent::Reply(-PmError::from(e).to_errno())`——与 F10b `reply_wire()` 语义同构（错误码取负、数据码原样）；
- `os/servers/pm/src/ipc/dispatcher.rs` L110/L114 `ReplyIntent::Reply(ENOSYS)` → `Reply(-ENOSYS)`（CodeReview S1 建议）；同步 3 处单测断言；
- `os/servers/pm/src/event.rs` L409 `Reply(minix_types::ENOSYS)` → `Reply(-ENOSYS)`（与 dispatcher 同路径）；同步 1 处单测断言；
- `os/servers/pm/src/wait.rs` L149 `Reply(ECHILD)` → `Reply(-ECHILD)`（init waitpid 循环必 hit 分支，CodeReview S1）；同步 1 处单测断言；
- `os/servers/pm/src/init.rs` L1264 `test_run_once_replies_enosys_to_unimplemented_call` 断言同步 `-ENOSYS`；
- `os/servers/pm/src/ipc/calls.rs` L1449 `test_dispatch_fork_parent_unknown_is_error_reply` 断言同步。

**未扫的兄弟站点**（登记在 B17 前沿里，架构修法候选）：
- `calls.rs` L264 Get/SetPriority、L303 Kill、L317 SrvKill、L336 Ptrace、L429/L478/L505/L514/L520/L542-613/L633/L672/L688/L707/L716/L734/L742/L756/L765/L780/L791/L799/L816/L843/L859/L880/L897 `Err(e) => Reply(e.to_errno())` 共 ~25 处；
- `calls.rs` 12 处 `Reply(positive_errno(e))`——`positive_errno` 只归一不取负；
- `trace.rs` L646 EPERM 等未扫。
本轮采 CodeReview S1 建议先修高优先级 4 臂（Fork/SrvFork/ENOSYS×3 站点/ECHILD）；其余 ~37 站点下一轮采 N3 架构修法统一（拆 `ReplyIntent::Reply(i32)` 为 `Reply`/`ReplyErr` 两臂、`reply()` 内分派）。

### 三件套
1. **宿主测试**：`cargo test -p minix-pm --lib` **417/417 pass**（含 5 处断言同步 + Fork/ENOSYS/ECHILD 多臂盖到）。
2. **rustfmt 零新增**：5 个文件 `HEAD=NEW`（calls.rs 685=685、dispatcher.rs 240=240、wait.rs 415=415、event.rs 743=743、init.rs 298=298）。
3. **真机三次签名一致**（c8/c9/c10）：
   - c8（仅 Fork/SrvFork 取负）：35863 行、`init-state Runcom` 1 次、`nk4a: pm 0140b` 8 次；
   - c9（同 c8 build 另一次跑）：35281 行、`init-state Runcom` 1 次、`nk4a: pm 0140b` 8 次；
   - c10（采 CodeReview S1 补修 dispatcher/event/wait ENOSYS·ECHILD）：35267 行、`init-state Runcom` 1 次、`nk4a: pm 0140b` 8 次；
   - **对比 c5/c6/c7 基线（~25000 行）= +43% 活动**——`pfwd nr=1..9 out=B`、`p3drain dst=8 snd=0x1..0x7`——fork 链已通、init 子与兄弟服务器在跑；
   - rc marker 未出现 → B17 前沿（下轮定位 exec 半链或 marker echo 链）。

### CodeReview
Subagent CodeReview 报 **PASSED、无 MUST-FIX**，逐项结论：
- Item 1–2–6（取负语义、C 契约对位、From 转换）：均 PASSED，确认 `PmError::to_errno()` 恒正、`-to_errno()` 恒负。
- Item 3（同族站点未修）：SHOULD-CONSIDER——本轮已采纳建议先修高优先级 4 臂（dispatcher ENOSYS×2 + event ENOSYS + wait ECHILD）；
- Item 4（测试覆盖）：SHOULD-CONSIDER——未补 SrvFork Err 腿单测，下一轮补（本轮 5 处断言已同步，不属回归）；
- Item 5（回归风险）：PASSED——c8 +43% 活动量 = 解除旧阻塞而非引入新错误（`pfwd` 递增推进、新下游活动）；
- Item 7（注释质量）：N1 typo已修（旧木→旧实现），N2 “同构”措辞已按评审精确化为“与 C 客户端 `_syscall`/`_taskcall` 契约对齐”（C 服务端 PM 同处 latent bug，本 fix 同时修之）。
无回归，无 MUST-FIX 遗漏。

### 本 turn 行量
- 5 处生产代码变更：`calls.rs` Fork/SrvFork 两臂 + `dispatcher.rs` ENOSYS×2 + `event.rs` ENOSYS×1 + `wait.rs` ECHILD×1 + `init.rs` 1 断言；
- 6 处测试断言同步（calls 1 + dispatcher 3 + event 1 + wait 1）；
- WORKLOG 顶部 B16 bullet 改写 + B17 前沿新增 + 本节 §1.30 追加；
- 探针：全部已在 §1.29 之后 `git checkout` 回滚；本轮工作树无诊断探针；
- docker 三件套 813/242/528 全绿（本次仅改 PM，不影响 kernel/arch/vm 三件套，结果仅一验）；
- **frontier 推进至 1.30**（B16 部分修 = fork 链通，rc marker 未达，进入 B17）。

---

## 1.31 · B17 侦察（探针确认 do_fork 现真 Err，非假 Ok）

### 现象
承 §1.30 修复落地后（HEAD `71ae0dc8b`），fork 假 Ok 已消除——init 不再把 EAGAIN/ESRCH 当 child_pid 卡 waitpid。但 rc marker 仍缺，boot 停在 `init-state Runcom`。候选缩窄：
- ① do_fork 返 Err 被 init 正确消费 → init 走 `Attempt::SingleUser` → 再 fork 再 Err 循环；
- ② 或 do_fork 返 Ok 但 VfsReply::Fork 异步回复链仍未起。

### 探针 B（本轮，5 点 runcom.rs）
`nk4c_mark("rc:chld" | "rc:before-exec" | "rc:exec-ret" | "rc:frkE" | "rc:prnt" | "rc:hitw")`——同 §1.29 模式，走内核 diagctl 串口直写不依 tty/fd 1 wire，cap 8。

### 真机 c11 命中矩阵
| 探针 | c5/c6/c7（修前）| **c11（修后）** |
|---|---|---|
| rc:prnt（父 Ok 分支）| 3 轮全命中 | **0 命中** |
| rc:frkE（Err 分支）| 0 | **1 命中** |
| rc:chld（子 Ok(0)）| 0 | 0 |
| rc:before-exec / rc:exec-ret | 未测 | 0 |
| rc:hitw（waitpid 命中）| 未测 | 0 |
| init-state Runcom | 命中 | 命中 |

**关键翻转**：`prnt` 归零 + `frkE` 首次命中 = **§1.30 取负修复实锤生效**：init 现收到 `Err(-errno)` → 走 `Attempt::SingleUser` 分支 → emergency + `waitpid(-1, WNOHANG).is_ok() {}` 排空 zombie → sleep(STALL_TIMEOUT) → 回 SingleUser 状态。

### 结论
- **B16 修复完全正确**：错误回执不再被误当合法 child_pid；init 状态机按 C `init.c:911-922` "can't fork → reap + sleep + single_user" 走通。
- **B17 真前沿**：do_fork 现**实际**返回 Err（前只是 Err 被误当 Ok），但**具体哪个 Err 臂**仍未知——候选 `can_alloc_for_user`（VM/proc 表内存）、`find_free_slot`（proc 表满）、`vm_fork`（VM sendrec 失败）——都需 PM 侧下一层探针。init 侧 fork 已正确失败。

### 下一入口（fix B17）
1. PM 侧 `fork.rs` 里 `do_fork` 在 `find_parent_slot` 之后的每个 Err 站点加 `nk4c:` 探针（`dF:canA`/`dF:ffs`/`dF:vmF`/`dF:tell`）——一次真机跑即可钉死停点；
2. 依数据裁决修复：若 `vm_fork` 失败 → 查 VM 侧 VmFork 语义（可能与 §1.24 grant table 单点性相关）；若 `find_free_slot` → 查 proc 表容量与 boot 模块数量匹配；若 `can_alloc_for_user` → 查 VM 内存配额；
3. 修后三件套 + CodeReview + §1.32 + fix commit；期望 c12 见 `rc:chld` 命中且 `rc:before-exec` 后**不**再命中 `rc:exec-ret`（=exec 成功、marker 由 sh 打印）。

### 本 turn 行量
- 1 处代码变更（`runcom.rs` 探针 + `nk4c_mark` 辅助）→ **本 turn 末已 `git checkout --` 回滚**，工作树与 HEAD 相同；
- WORKLOG 顶部不动（B17 前沿 bullet 已存在，仅本节 §1.31 追加）；
- **纯侦察 doc commit**（同 §1.25/§1.28/§1.29 先例）——无生产代码变更、无三件套、无 CodeReview。
- frontier 仍在 1.30；rc marker 未达。

---

## 1.32 · B17 侦察 C（PM 侧 do_fork Err 臂定位探针）——vm_fork 是唯一命中臂

### 现象
§1.31 探针 B 实锤 init 现真收 `Err(-errno)` 走 `rc:frkE` 分支，do_fork 实际 Err——但具体 Err 臂未定。候选：`can_alloc_for_user` / `find_free_slot` / `vm_fork`。

### 探针 C（本轮，PM fork.rs 3 点）
`nk4c_fm("dF:canA")`（`can_alloc_for_user` 失败）+ `nk4c_fm("dF:ffs")`（`find_free_slot` 返 None）+ `nk4c_fm("dF:vmF")`（`vm_fork` sendrec 或 m_type!=OK）——cap 8，走内核 diagctl 串口直写。

### 真机 c12 命中矩阵（35607 行）
| 探针 | c12 |
|---|---|
| dF:canA（表满容量）| **0** |
| dF:ffs（无空闲槽）| **0** |
| **dF:vmF（VM 拒绝）**| **1 命中** |

### 结论
**根因缩窄至 `vm_fork` 单点**——proc 表容量与非空闲槽都正常（`procs_in_use < LAST_FEW`、`find_free_slot` 返 Some），失败发生在 `dispatcher.rs::vm_fork` 内的 `transport.sendrec(Endpoint::VM, &mut msg)` 或 `msg.m_type != OK` 分支。

### 下一入口（fix B17 完整链）
候选：
1. **VM 服务未接线 VM_FORK**：`Endpoint::VM` receive 循环对 `VM_FORK = ?` 未实现 → 回 ENOSYS 或 ESIGN 使 `msg.m_type != OK` 走 VmError；
2. **VM 未进 receive**：`transport.sendrec` 阻塞拿到 ENOTREADY(201)（与 F13 PM↔VFS 同形态但 VM 侧）；
3. **VM 分配子地址空间失败**：真语义 ENOMEM（VM 侧 region 池不足，与 §F12 slab 上限相关）。

诊断：VM 侧 `vm_fork` 处理入口加"vmF-in / vmF-ok / vmF-err"三点短探针 + dump reply m_type；一轮即可裁决候选 1/2/3。

### 本 turn 行量
- 1 处代码变更（`fork.rs` `nk4c_fm` 辅助 + 3 处 Err 臂探针）→ **本 turn 末已 `git checkout --` 回滚**；
- WORKLOG 顶部不动（B17 前沿 bullet 已在 §1.30 落地），仅本节 §1.32 追加；
- **纯侦察 doc commit**（同 §1.25/§1.28/§1.29/§1.31 先例）——无生产代码、无三件套、无 CodeReview；
- frontier 仍在 1.30；rc marker 未达，B17 完整链下一轮修复。

---

## 1.33 · B17 侦察 D（PM 侧 dump vm_fork 回复 m_type）——ACL 候选排除 + 新登记 PM 布局脆弱性（探针回滚）

### 目的
承 §1.32（c12 实锤 vm_fork 是唯一 Err 臂）。拟在 PM `vm_fork` dump VM 回复的真实 `m_type`（正 errno 直接映射 VM 侧 `VmError` 变体 → `fork::do_fork` 失败点），一轮裁决候选 1/2/3。

### 探针实现（两版）
1. **首版**：`ForkCoordError::VmError` 改为携带 i32（`VmError(msg.m_type)`，transport 失败用 `-1` 哨兵）；`do_fork`/`srv_fork` 在 `Err(VmError(code))` 臂用 `kern.diag_write(&alloc::format!("nk4c:vmF={}", code))` 打点。
2. **lean 版**（发现回归后改）：`nk4c_probe(kern, prefix, code)` 手写定长栈缓冲 itoa，**不引 `core::fmt`**（与既有 `nk4a: pmvi` 探针 init.rs:755 同形）。

### 关键发现 A：boot 确定性回归（探针被阻断）
**两版探针都令 boot 不达 init Runcom**，且回归与“向 PM 加代码”强相关：
- 干净 HEAD（`git stash` 同 session 重建）c14/c14b 两次均 **Runcom=1、~35200 行、无 rip=0、SCHEDstall=28 一致**；
- 探针 build 6/6 失败：c13（停 SCHED receive、picknone 0x8）、c13b（rip=0 err=0x10 用户态取指崩溃 rsp 递减自环、exit=0）、c13c/c15/c15b/c15c（exit=124、SCHED 活锁）；
- 停点定位：c15c 里 PM 发完 12 条 `nk4a: pmvi k=0..0xb`（`init.rs:753` VFS_PM_INIT per-process send）后，卡在**末条 barrier `sendrec(VFS)`**（`init.rs:778`）——**无 `nk4a: pmvi-barrier-done`** → PM 挂死 → init 拿不到进程表同步 → 永不 Runcom；
- 体量线索：lean 版 PM 模块仅 277840B(68 页)→278864B(69 页)，跨 278528=68×4096 边界即触发。+1KB 就破坏 boot → 非单纯“大爆炸”、而是**按页/地址粒度敏感的脆弱性**。

**定性**：这是 WORKLOG 已登记“fresh-vars 布局鲁棒性 bug”的**新形态**——原仅限全新 vars；本轮实锤**累积 vars 下 PM 代码体积/地址布局同样能触发服务加载/IPC 投递异常**。候选根因：sendrec 半与 VFS rendezvous 疑依赖固定地址假设（与 1.10z “sendrec 快路径 receive 半 ANY→目的地”、B13 栏陈旧快照、rip=0x202d68∈KernelIpcTransport::receive 同族），或模块加载有 68 页硬边界。**属 bring-up 稳定性债，本轮未修**（它是下一轮真正的阻断项：不修则一切 PM 侧探测/开发都被卡）。探针已全部 `git checkout` 回滚，工作树=HEAD。

### 关键发现 B：读码排除候选 1（ACL 拒/未接线）
无需探针即可裁决：
- VM 收到任意 VM_* 调用先过 `vm_server.rs:1684` `callnr` 定序 + `1694` `acl_check`；被拒则回 `VmError::NotImplemented`→ENOSYS(38)。
- 但 `rs/boot.rs:2989` “SRV_VC = ALL_C → full vm_call_mask”（对位 C main.c:317-319/priv.h:78-80），**boot 服务槽拿 `CallMask::all()`**，含 VM_FORK 位（`acl.rs:65` DEFAULT 也含 VM_FORK）→ PM 必过 acl_check。
- ∴ **候选 1 排除**：VM 确进 `dispatch_fork`→`fork::do_fork`，失败在**其内部返 Err**。候选缩至：`InvalidSlot`/`SlotInUse`(EINVAL=22)、`PageTableInitFailed`/`PageTableMapFailed`(EIO)、`fork_regions` 错、`sys_fork → VmForkError::KernelCall`(gateway 失败)、`handle_memory_once`(msgaddr 预故障 EFAULT/CowAllocFailed)。其中依赖内核 `sys_fork` 输出 `fork_msgaddr` 的后两者与 F11/F13 内核 IPC 腿同族、**最可疑**。

### 下一入口（fix B17，不再动 PM）
1. **优先 (A)：攻 PM 布局脆弱性**——定位 sendrec/barrier 路径的固定地址假设或模块加载页上限；不修则 PM 一切后续开发被卡（不只本探针）。
2. **(B)：改从 VM 侧探测 `fork::do_fork` 的 `VmError` 变体**（用无 fmt 定长栈缓冲）——先验 VM 代码膨胀是否同样脆弱；若 VM 也脆弱，退化到内核 `sys_fork` 腿观测。
3. **(C)：纯读码推进候选**——优先查 VM `gateway.sys_fork`（`kernel_gateway.rs`）与内核侧 fork 实现是否接线/返回正确 `(child_endpoint, fork_msgaddr)`（与 E-FORKMSG/do_fork.c:112 对位）。
   - **本轮 (C) 读码已得精确候选（未真机/宿主测验证，下轮首要）**：内核 `syscall_process.rs::dispatch_fork` 两处早期校验失败都返 **`KcallResult::Ok(EINVAL)`（正值 22、非负错误码）**——L162-167「caller(VM) 必须 RECEIVING」与 L171-173「child 槽必须空」。而 VM `minix-sys::sys_fork`（syscall.rs:597-603）只在 `reply < 0` 判错→ **正值 EINVAL 被当成功**、去读**从未写入的应答臂** `m_krn_lsys_sys_fork.{endpt,msgaddr}`（garbage）→ VM fork::do_fork 带垃圾 msgaddr 继续→ `handle_memory_once` 失败→ VM 回 Err → PM `dF:vmF`。其中 L162 RECEIVING 校验最可疑：VM 经 SYSCALL/int33 腿（非 IPC receive）调 sys_fork 时多半不在 RECEIVING 态（与 S3「int33 vs IPC 腿门纪律」/F10b「`KcallResult::Data` vs 错误取负」同族）。**候选修法**：校验失败应返真正错误语义（`KcallResult::Err(-EINVAL)` 或使 `kernel_call_finish` 取负），而非 `Ok(EINVAL)` 正值；需先核 C `do_fork.c:46/51` 的返回是走 `errno`（正值、_syscall 自返 OK 后查 errnoc）还是 m_type——并确认 VM sys_fork 是否真处 RECEIVING。**下一步：先写一个宿主/真机最小实验坐实 dispatch_fork 到底命中哪条 Ok(EINVAL)（或根本不命中的话 vm_fork 失败另有其因），再定修法。**（注：该修法若动 PM 侧代码会撞本轮登记的 PM 布局脆弱性；但 dispatch_fork 在内核、sys_fork wrapper 在 minix-sys，不属 PM，可先改这两处验证。）

### §1.33 本 turn 行量
- 2 处代码变更（`fork.rs` VmError(i32)+nk4c_probe+两 Err 臂打点、`dispatcher.rs` vm_fork 携带 m_type+两测断言）→ **本 turn 末全部 `git checkout --` 回滚**，工作树=HEAD；
- WORKLOG 顶部 B17 前沿 bullet 重写（vm_fork 内部失败+布局脆弱性阻断+ACL 排除）+ 本节 §1.33 追加；
- **纯侦察 doc commit**（同 §1.25/§1.28/§1.29/§1.31/§1.32 先例）——无生产代码、无三件套、无 CodeReview；
- frontier 仍在 1.30；rc marker 未达。

---

## §1.34　B17 修复落地（内核 `dispatch_fork` 用错 caller 而非 parent）+ B18 前沿

**受控读法接续**：本 turn 从 §1.33 下一入口 (C) 精确候选起步（HEAD=c46ba35a7，工作树 clean）。

**修正 §1.33 的一处误判**：§1.33 疑「内核返 `KcallResult::Ok(EINVAL)` 正值 22、minix-sys::sys_fork 仅 `reply<0` 判错→正值被当成功」。**实测证伪**：`KcallResult::Ok(code)` 在 SYSCALL 腿经 `syscall.rs:270 syscall_leg_wire(code)=code.wrapping_neg()` **取负**后交付（`reply_wire()` L246），故 `Ok(EINVAL)` 上线即 `-EINVAL`，`sys_fork` 的 `reply<0` **正确判错**；成功 `Ok(0)`→`-0`=0=OK。**错误契约无 bug、minix-sys 不需改。**

**真根因（读码坐实，纯逻辑非契约）**：内核 `syscall_process.rs::dispatch_fork` 四处——RECEIVING 校验(L162)、`KProcess::fork_from` 拷贝源(L193)、`parent_is_sys_proc` 的 priv 解析(L203)、应答 `msgaddr` 读取(L242)——全用 `caller_nr`。但 minix-rs 里 **VM 代表父进程陷入 SYS_FORK**（`vm/src/fork.rs:343 gateway.sys_fork(parent.endpoint(), child.slot())`，`kernel_gateway.rs:232` 下传 `m_lsys_krn_sys_fork.endpt=父endpoint`），故内核 `caller_nr`=VM ≠ 被 fork 的父。C `do_fork.c` 这四行（L41/44 isokendpt 解 rpp、L51 RECEIVING、L63 `*rpc=*rpp`、L105 priv、L112 msgaddr）一律基于 **rpp（父）**、从不碰 `caller`（旧 Rust 注释「C guarantees rpp==caller」是臆断的假前提）。运行时 VM 不在 RECEIVING（正主动 dispatch VM_FORK）→ L162 校验失败 → 返 EINVAL（经腿取负为 -EINVAL）→ VM `VmForkError::KernelCall` → PM `dF:vmF` → fork 永久失败（=B17 停点）。

**修法（只动内核、不碰 PM，避开 §1.33 布局脆弱性）**：新增 `let parent_nr = proc_table.endpoint_to_nr(Endpoint(fork_req.endpt))`（`proc_table.rs:788`，匹配 `p_endpoint==ep && p_rts_flags!=SLOT_FREE`，即 C `isokendpt` 含 `!isemptyn` 的等价；解析失败→`Ok(EINVAL)` 覆盖 C L41/L46 isemptyp(rpp)），四处改用 `parent_nr`；`caller_nr` 形参→`_caller_nr`（对齐 C 未用的 `caller`，与同文件 `dispatch_exec`/`dispatch_clear` 已用 `endpoint_to_nr` 解 target 的先例一致）。子端点代际仍取自 child_slot 旧端点（C L59 亦如此、与拷贝源无关，不改）。

**测试（新增/修正 3 项）**：
- 新 `test_t12_fork_resolves_parent_from_endpt_not_caller`：父在 slot2（端点 500、occupied、RECEIVING、delivermsg=0xBEEF0000）、caller=VM(ProcNr(8)) 非 receiving（delivermsg=0xDEAD）；旧码查 VM 非 receiving→EINVAL→断言 `Ok(0)` 挂；新码解析父→成功、msgaddr=字面 `0xBEEF_0000`（非 caller 0xDEAD）。真捕获 B17 回归。
- 新 `test_t12_fork_rejects_unresolvable_parent_endpt`：端点 999 无占用槽→`endpoint_to_nr` None→EINVAL（覆盖 C L41 isokendpt 分支持续）。
- 修被本改动**摊空**的 `test_t12_fork_rejects_non_receiving_parent`（CodeReview **W1** 采纳）：旧版只设 endpt=100 不占用→新代码先倒在 `endpoint_to_nr`、永远走不到 RECEIVING 闸（同值不同因、丢覆盖）；现改为令父槽 occupied+端点匹配但不 RECEIVING，控制流真达 L162。
- CodeReview 其余采纳：S2 修正函数 doc/行内关于 msgaddr 的错误叙述（msgaddr=父的交付缓冲、作参回交 VM 做父子两侧 eager CoW；内核自身回执写进 caller(VM) 自己的 deliver 缓冲，非父的）+ 旧测注释；S3 新测父槽补 `clear(SLOT_FREE)`、caller 用真实 VM_PROC_NR=8。

**三件套**：
- docker：kernel **815**（基线 813→+2 新测）/ arch 242 / vm 528，**0 fail**（只增不减✓）。
- fmt：`syscall_process.rs` worktree=54 == HEAD=54，零新增漂移✓（本文件存量漂移多、判据是不新增）。
- 真机两次签名一致：c16==c17（均 timeout 90）——`init-state Runcom`=1、`reply without request(slot 0)`=1、`ECALLDENIED caller=0`=1、`pmstall2`=2、slot0=311/slotb=63。**与基线 c14 对比（决定性）**：slot 1..0xa pick 计数逐位相同（slot8=763/763、slot1=437/436 …）=无早/boot 回归；唯一变量=B17 活锁的 slot0↔slotb ping-pong 从 c14 的 **907/665** 坍缩到 c16/c17 的 **311/63**——即 fork 不再 EINVAL、子真被创建。c17 之后的改动全为 `#[cfg(test)]` 体与注释，release 内核二进制逐字节不变，c16/c17 签名即本 commit 签名。

**结论**：**B17 已修**（fork 内核侧 caller/parent 混淆根治、真机 ping-pong 坍缩为直接证据）；新停点 **B18**（fork 后下游：PM `reply without request(slot 0)` + `ECALLDENIED caller=0` + PM 停滞，详见顶部 B18 bullet）。rc marker 仍未出，frontier 仍 1.30。

### §1.34 本 turn 行量
- 生产代码 1 处（`syscall_process.rs::dispatch_fork` 四处 caller_nr→parent_nr + 新增 endpoint_to_nr 解析 + `_caller_nr` 改名）+ 新增/修正 3 测 + 若干注释修正；
- WORKLOG 顶部 B17→已修 bullet + B18 前沿 bullet + 本节 §1.34；
- 三件套全绿（docker 815/242/528·0fail / fmt 54=54 / 真机 c16==c17）+ CodeReview W1/S2/S3 已采纳；**含代码修改的 fix commit**。

---

## §1.35　B18 侦察：PM panic `reply without request (slot 0)` 终态机制钉死

**受控读法接续**：本 turn 从 §1.34 B18 前沿三条下一入口起步（HEAD=83ca265e5，工作树 clean）。纯读码 + 关联既有真机日志（c16/c17），无生产代码改动。

**终态钉死（c16 日志 24154→24310→24332）**：
- `24154: nk4a: init-state Runcom`（init 进入 Runcom，fork+exec rc 脚本）；
- 中间 ~156 行为页故障/调度活动（fork 链已通、子/兄弟进程真在跑，与 §1.30「活动 +43%」一致）；
- `24310: servers/pm/src/ipc/vfs.rs:385: handle_vfs_reply: reply without request (slot 0)`——**行首带 `file:line:` 前缀 = Rust `panic!` 标准输出格式**（非 `nk4a: PF` 分块 diagctl，说明 panic 走 `format_panic_report` 路径成功落盘），日志随即止于 24332（仅余 gtick 空转 + `pmstall2` ×2）。

**PM panic 的语义（C 忠实对位）**：`take_vfs_call`（`pm/src/ipc/vfs.rs:380-390`）对应 C `main.c:324` `assert(p->P_flags & VFS_CALL)`。触发条件（读 `pm_isokendpt`，`table.rs:203`）：`handle_vfs_reply` 从回执 `msg.m_u.m_m7.m7i1` 取端点→`endpoint.slot()` 得 slot→要求 `procs[slot].endpoint()==端点`（**含代际精确相等**）且 `procs[slot].is_in_use()` 成立、`slot_of_endpoint` 才返回 `Some(slot)`；随即 `take_vfs_call(slot)` 发现该槽 `ipc_blocked` 非 `IpcBlockReason::VfsCall{..}` → panic。即 **PM 收到一条 VFS 主动回执，声称关于「PM UserSlot 0」这个进程，但该进程此刻没有挂起的 VFS 请求**。

**slot 0 身份澄清（关键）**：`Endpoint::PM=Endpoint(0)`、`INIT=Endpoint(11)`（`endpoint.rs:61/72`）；PM UserSlot 按 `endpoint.slot()` 索引，故 **panic 的 slot 0 = 端点 slot 位为 0 的进程 = PM 自身在 mproc 表的槽位，绝非 init**（纠正 §1.34 前沿 bullet 里「slot0=init」的口头误记）。而 fork 子进程 `child_slot` 取自 `find_free_slot`/`next_child`，boot 槽 0..11 已占 → 首个子 slot ≥12 → `child_endpoint.slot()`≥12，**回执若关于子进程会解析到 ≥12 槽、不会 panic 在 slot 0**。故**触发 panic 的回执并非 `VFS_PM_FORK_REPLY` 本身**，而是另一条携带 slot-0（PM 端点）的 VFS 回执。

**PM do_fork 链已读通**（`pm/src/fork.rs:23`）：`child_endpoint=vm_fork(transport,parent_endpoint,UserSlot::new(child_slot))`（sendrec VM_FORK，回复子端点在 m1i3）→ `debug_assert child_endpoint.slot()==child_slot` → `copy_mproc(table,parent_slot,child_slot,0,child_endpoint)` 设子 PM 端点 → `tell_vfs(table, UserSlot::new(child_slot), VfsCall::Fork{child:child_endpoint,parent:parent_endpoint,child_pid}, transport)`——**VFS_CALL 挂在子槽、发 `VFS_PM_FORK`**。VFS 回 `VFS_PM_FORK_REPLY` 携 `m7i1`→PM `handle_vfs_reply` 解析。VFS 回执里 echo 的端点究竟取请求哪个字段（`child`？`m_source`=PM？），决定回投递落点。

**取证阻断（诚实登记）**：§1.33 的 PM 布局脆弱性阻断一切 PM 侧探针，无法在 `handle_vfs_reply` 入口 dump「panic 回执的 opcode + `m7i1` 具体值」以一眼定死。**因此本轮不臆造未验证的修复**（违反诚实证据纪律），仅钉死终态机制 + 缩窄触发面。

**下一入口（决定性、且绕开 PM 脆弱性）**：**VFS 服务端**（`fs/` 系 crate——独立于 PM，**不受 §1.33 布局脆弱性阻断**）是 `VFS_PM_FORK`/`VFS_PM_EXEC`/`VFS_PM_EXIT` 等回执 `m7i1` 端点的**构造方**。下轮优先：
1. 读 VFS 侧处理 `VFS_PM_FORK`/`VFS_PM_EXEC` 并 encode 回执的站点（grep `VFS_PM_FORK_REPLY`/`m7i1`/`m_m7` 的写入方），查其 echo 的端点从请求的哪个字段取；
2. 核对是否存在「PM 代表子/他进程发起 VFS 请求，但 VFS 用请求的 `m_source`（=PM 端点 slot 0）而非载荷端点回执」的失配，或代际/端点编码错配；
3. 若 VFS 侧读码仍不确定，可在 VFS（非 PM）加 ≤16B nk4c 探针 dump 回执 opcode+m7i1（VFS 不受 PM 布局脆弱性影响，但需先验 VFS 自身是否同类脆弱——若 build/boot 回归则退化到纯读码裁决）。

**结论**：B18 终态机制 = **PM 因一条带 slot-0（PM 端点）却无挂起 VFS 请求的回执而 panic（C 对位 `assert(VFS_CALL)`），panic 后 ECALLDENIED caller=0 + pmstall2 自旋、boot 死**；rc marker 未达，frontier 仍 1.30。修复入口缩窄至「VFS 回执端点 echo 语义」，下轮从 VFS 服务端读码/探针起步（不受 PM 阻断）。

### §1.35 本 turn 行量
- 纯侦察：读 c16/c17 真机日志尾 + `pm/src/ipc/vfs.rs`(handle_vfs_reply/take_vfs_call) + `pm/src/mproc/table.rs`(pm_isokendpt) + `minix-types/endpoint.rs` + `pm/src/fork.rs`(do_fork) + 内核 `syscall.rs`(ipcerr 探针)/`trap_dispatch.rs`(pmstall2 探针)；
- **无生产代码改动、无探针增删**（既有 `nk4a:` 取证探针系前代 commit 已入仓，非本轮新增）；
- WORKLOG 顶部 B18 bullet 精确化（钉死终态 + 澄清 slot0=PM 非 init）+ 本节 §1.35；
- **纯侦察 doc commit**（同 §1.25/§1.28/§1.29/§1.31/§1.32/§1.33 先例）——无三件套、无 CodeReview；frontier 仍在 1.30；rc marker 未达。

---

## §1.36　B18 修复：VFS service_pm 回执 m7i1 未 echo 目标端点→PM panic slot 0 根治

**受控读法接续**：本 turn 从 §1.35 下一入口（VFS 服务端读码，不受 PM 布局脆弱性阻断）起步（HEAD=1b81386ce，工作树 clean）。

**根因精确定位**：
- VFS `Route::Pm`（`main_loop.rs:1115`）处理 PM→VFS 服务消息（fork/exec/exit/setsid 等），成功路径调 `reply.encode()` 构造回执再 `queue_reply_msg`；
- `VfsReply::encode()`（`minix-types/ipc/vfs.rs:705`）对所有变体设 `m7i1=0`（从不回填目标端点）；
- C `service_pm`（minix3 `vfs/main.c`）每一路 `proc_e=m_in.VFS_PM_ENDPT; m_out.VFS_PM_ENDPT=proc_e;`——把请求携带的目标进程端点原样 echo 进回执 m7i1；
- PM `handle_vfs_reply`（`pm/src/ipc/vfs.rs:189`）从 m7i1 取端点→`pm_isokendpt(endpoint)` 解槽位→`take_vfs_call(slot)` 要求该槽有挂起 VfsCall；
- m7i1=0 被解为 `Endpoint(0)`=`Endpoint::PM`，slot 0 = PM 自身 mproc 槽（`in_use` 但无 `ipc_blocked:VfsCall`）→ **panic**。

**修复**：
1. **`os/libs/minix-types/src/ipc/vfs.rs`**：新增 `VfsReply::encode_reply_for(&self, target: Endpoint) -> Message`（line 766），在 `encode()` 产出的 Message 上补写 `unsafe { msg.m_u.m_m7.m7i1 = target.get() }`。不改 `encode()` 签名/行为，零 churn。
2. **`os/servers/vfs/src/main_loop.rs`**：Route::Pm 两处成功回执改用 `encode_reply_for`：
   - exec 分支：`reply.encode_reply_for(endpoint)`（`endpoint` 从 `VfsCall::Exec{endpoint,..}` 解构）；
   - `Ok(other)` 分支：先 `let target = other.endpoint();`（`VfsCall::endpoint()` 对 Fork/SrvFork 返回子端点，与 PM 挂 VFS_CALL 于子槽精确对齐），再 `reply.encode_reply_for(target.unwrap_or(Endpoint::NONE))`。
3. **新测** `test_vfs_reply_encode_reply_for_echoes_endpoint`：验证 bare encode 保持 0、encode_reply_for 回显指定端点、Exec 回执其他字段不受影响。

**三件套取证**：
- 镜像 build：`xtask image x86_64 release` EXIT=0；
- Docker：kernel **815** / arch **242** / vm **528** / minix-types **309**（含新测 ✓）/ minix-vfs **530** = **0 fail**；
- fmt 零新增漂移：vfs.rs cur=2=head=2, main_loop.rs cur=1=head=1；
- 真机 c18/c19 两次：timeout 124、**24926 行**（vs c16 的 24332=+594 行活动），`reply without request`=**0**、`ECALLDENIED`=**0**、`pmstall`=**0**；结构签名（去掉计时行 `vs<CNT>` 后）逐字节一致。

**CodeReview**：无 MUST-FIX。SAFETY 注释成立（union 刚由 encode 以 m_m7 构造、同臂写有效）。SHOULD-FIX 邻近边（Err 分支未 echo，本修前既存、boot 关键路径不命中，登记不扩面）。

**新前沿 B19**：过 Runcom、无 panic、无 IPC 错误——但 rc marker 仍未打印。子进程 exec/console 链路为下一追查方向。

### §1.36 本 turn 行量
- 生产代码 2 文件（`minix-types/ipc/vfs.rs` 新增方法+测 + `vfs/main_loop.rs` 两站点改用）；
- WORKLOG 顶部 B18→已修 bullet + B19 前沿 bullet + 本节 §1.36；
- 三件套全绿 + CodeReview PASSED；**含代码修改的 fix commit**。

---
