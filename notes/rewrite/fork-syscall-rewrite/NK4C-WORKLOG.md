# NK4-C WORKLOG（Task C「清零者」追捕 → 三架构 + 命令面 + 测试上机）

> 本文件是**记忆与交接载体**（git-tracked）。长程任务：每完成一个逻辑单元就更新顶部"当前状态" + 追加一节 + commit。
> **顶部状态必须始终是最新的**——用户会在任意时刻让 agent 收尾，接手者只读它 + `git log --oneline -20` 就要能接续。
> 详细取证历史见 `.review/zcode/edge1/FIXLOG.md` 迭代 27-33（**本地文件、被 gitignore、换工作树会丢**——关键结论在本文件 §交接来源有副本）。

---

## 当前状态（每次 commit 前更新，一屏读完）

> **⚠ 最新前沿＝B38（exec 成功后进程上下文切换活锁）——B37 两个堆叠根因全修（§1.65），`/bin/sh` 首次真正装载并运行，rc 15 轮循环崩解为 1 轮，但两进程陷入紧密活锁、marker 仍未出（1.65，含代码·VFS 侧）**：承 §1.64（首块 FS 读返 Ok 零字节），本单元定位并修复 **B37a + B37b 两个堆叠缺陷**。**B37a（VFS-local magic grant 的 `who_from` 用错哨兵）**：`open_exec` 首块读与 stat 用 `grant_magic(who_from=Endpoint::SELF)` 给 VFS 自己的 `hdr_buf` 建权，但内核 `verify_grant`（`os/kernel/src/grant.rs:461`）把 `magic.who_from` 原样装为 `effective_granter` 去解析页表，而 `Endpoint::SELF` 是哨兵（`ENDPOINT_SLOT_TOP-3`、`is_valid()==false`）非真实端点 → 拷贝失败被 FS 侧吞（`let _ = ipc.copy_to`，`os/fs/fs-rt/src/transport.rs:272`）→ `hdr_buf` 全零、`req_read` 仍回 `Ok`（正合 §1.64 `after-read 00 00 00 00`）。修＝引入 `const VFS_LOCAL_WHO_FROM = Endpoint::VFS`（具体端点，对齐 C `map_header` 的 `VFS_PROC_NR`，exec.c:755）替两处 `Endpoint::SELF`；段装载读走 `target_e`（目标进程具体端点）本就正确未动；`sys_datacopy` 的 `SELF`（内核自解析 caller）是不同路径不受影响。**B37b（LP64 phdr 表界遗留 32 位假设）**：修好 B37a 后 `bn37p` 探针实锤首块字节已落入 `hdr0..8=7f454c46 02010100`（ELF 魔数），但 exec 仍回 ENOEXEC——真机探针（已回滚）区分出这是一条**更下游的独立腿**：`ElfHead::parse` 的 phdr 界用 `SECTOR_SIZE`(512) 校验（`e_phoff + phnum*56 <= 512`），而 C `elf_sane` 是 32 位时代（`__ELF_WORD_SIZE 32`、`Elf32_Phdr` 32B），LP64 下 `e_phentsize`=56、`/bin/sh` 的 10 项 phdr 表=624>512 被误拒。修＝界改为实际加载头缓冲长 `buf.len()`（`map_header` 本就加载 10 页=40960），删 `SECTOR_SIZE`；C `elf_unpack` 里被 `#if 0` 停用的 `phdr+phnum >= hdr_len` 检查正是本修法的设计本意。**两修叠加→真机 bn38/bn38b 双跑签名一致：`runcom` 15→**1**（循环崩解、`/bin/sh` 首次装载运行）、`ENOEXEC`=0（端到端消除）、`panic`=0、`oom`=0**。三件套绿（mock vfs lib **537·0fail** 基线+3 新测 / fmt WT0 / clippy 零新增）+ CodeReview **PASSED 0 MUST·0 SHOULD**。**新头号前沿＝B38**：`/bin/sh` 装载成功后，日志翻为 85 万行（bn38=858099/bn38b=849854，差异系活锁密度非签名漂移）**全是内核逐上下文切换诊断探针 spam**（`nk4a: pick/sa0/sa1/cr3/gs2/probe/pre-restore`，系 HEAD 既有诊断非本单元引入），两进程页表根 `0x2149000`/`0x2c26000` 各对敲 ~4.6 万次/150s = **紧密活锁**，`pre-restore rip=0x2016d6` 高重复，marker 未出。下单元＝判明哪两进程在敲（`0x2016d6` 属哪个 ELF/哪条 syscall 往返）、是 init↔PM↔VFS↔sh 的哪条 IPC 环，及是否 B31/B32 家族外部命令 exec（echo）腿的下游再现。rc marker 仍未达。详见 §1.64/§1.65。
>
> **⚠（1.63/1.64 历史·其头号前沿 B37 已于 §1.65 修复）——B33a 符号掩盖已修（pm_exec 边界单点折负，去 `exec_via` `Ok(_)→EIO` 吞没），真错误 **ENOEXEC(8)** 诚实浮现，但 `/bin/sh`（readelf 实测＝静态链接 ELF EXEC·无 PT_INTERP）仍装载失败、marker 仍未出（1.63，含代码·VFS 侧）**：§1.62 实锤 rc 阻塞真身＝B33a 符号掩盖（`pm_exec` 回正值 errno 被 init `exec_via` `Ok(_) => EIO` 吞掉）。本单元在 `pm_exec` 公共边界 `map_err(neg)` 单点折负（体内约 15 腿符号混合：IPC 腿已 `neg()`、`enoexec` 裸值与 `ExecError::*::to_errno()`/`ElfHead::parse` 正值腿泄漏），`neg()` 对已负腿幂等→不动既有正确腿；`ExecErrno`「负 errno 直通 PM」契约与 main_loop「失败为负 errno」注释自此真实成立。**真机 bn30p 探针（跑后回滚）捕获 `exec_via errno=8 name=Some("ENOEXEC")`×45——EIO 掩盖端到端消除**；bn29/bn29b 双跑签名一致（73081/73096 行·panic=0·oom=0·runcom=15·marker=0，boot 零回归）。三件套绿（mock vfs lib **534·0fail** 基线+1 / fmt WT0 / clippy 零新增）+ CodeReview **0 MUST·1 SHOULD 已采纳**（pm_exec_inner doc 自相矛盾→mixed-sign）。**新头号前沿＝B37（§1.64 四探针翻案）**：去掩盖后真错误 ENOEXEC(8) 浮现但 sh 仍 bail——初疑的 phdr 表超 512（readelf 实算 624>512）**已被探针推翻**：`parse-enter len=40960 b0..3=00 00 00 00`（hdr_buf 全零、parse 在魔数腿 L178 就 bail、早于 phdr 腿 L187）；`firstblock ino=8 vsize=131968 hdr_len=40960`（vnode size 正确、非 size=0 早退）；`after-read hdr[0..8]=00 00 00 00 无 req_read-ERR`。**真根因＝`open_exec` 首块读（`req_read(who_from=SELF)`→FS Endpoint(10)）返 Ok 却没把字节拷进 VFS-self grant 缓冲**。关键对照：init 能读 /etc/rc（FS→VFS 读路对小文件可用），缺口特定于 exec 首块**大块 40960（跨 10 页）grant 写回**或 `req_read` 大块分页读。下单元＝读 MFS Endpoint(10) REQ_READ 处理腿的 grant 写回机制 + `grant_magic(who_from=SELF)` 建权语义，对照 C `map_header`（exec.c:736-763），先确证「FS 是否真读到块、拷往哪个端点」再定修向。rc marker 仍未达。详见 §1.62/§1.63/§1.64。
>
> **⚠（1.61 历史·其头号前沿 B36 已于 §1.62 取证闭环＝真身 B33a 符号掩盖＋下层 EOPNOTSUPP）B35 已修（`supply_pages` 对 `page_count==1` 委托 `supply_page`，打通 freed 单页与 big-block 路径）→ 真机首次 **OOM-RT=0·panic=0**、历史最深推进 72495 行 → 翻出 B36 rc `Runcom` 15 轮循环（1.61，含代码·minix-rt）**：§1.60 把 B34 停在保守绑界 64 后，真机 bn23 OOM 换签 `size=001000 px=400/400 fp=39c/400`（十六进制＝恰好 1 页、bump 游标 1024/1024 耗尽、而 free-stack 搁浅 924 单页）。根因＝`FixedPoolSupplier::supply_pages`（run）只走 bump、**从不查 free-stack**，而 `alloc_big`→`supply_pages(1)` 对 1 页请求也走此路→游标耗尽后即使有可复用单页也返 null（与自身注释「free-stack reserved for single-page requests」意图相悖＝逻辑缺陷）。修＝`page_count==1` 委托 `supply_page`（1 页无邻接要求、与 `release_pages(p,1)` 逐页 push 对称、零化等价；多页 run 仍 bump-only；Linux/Redox order-0 从 free list 弹为同构做法）+ 新测。**重建镜像真机 `-m 512M -smp 1` 双跑 bn24/bn24b 签名一致**（72495/72494 行、pre-restore 7209/7209、**OOM-RT=0、panic=0（此前 6 次全消）、pagefault-in-VM=0**），boot 从 58507→**72495（历史最深）**。**新墙 B36**＝`Runcom=15`（每 ~3150 行重入 `init-state Runcom`）+`marker=0`+bogus=0——OOM 掩盖去除后 §1.55 的 rc 15 轮循环重现，现在不是 OOM、而是 B31/B32/B33b 修后的下游新失败因（runcom→exec→echo 腿哪步非 OOM 地失败）。三件套绿（mock minix-rt **59·0fail** 基线+1 / fmt WT0==HEAD0 / clippy Finished / 镜像重建+双跑一致）+ CodeReview **PASSED 0 MUST 0 SHOULD**。原则解 A/B（boot eager 物化 / VM-backed supplier）仍挂（多页 run 仍 bump-only，现因单页路径已足撑当前工作集未触发）。rc marker 仍未达。详见 §1.60/§1.61。
>
> **⚠（1.60 历史·其头号前沿 B35 已于 §1.61 修复）B34 已采「保守容量 round」落地（`MAX_BIG_BLOCKS` 32→64，消除 big=32/32 记录表 premature-OOM，boot 零回归，真机推进 30068→58507 行）（1.60，含代码·minix-rt）**：§1.59 把 B34 停在「待用户定 A/B/C」；本轮厘清——**A（boot eager 物化）/B（VM-backed heap supplier）触及 boot/VM 内存外部契约需 [ARCH]、确属架构裁决；C（纯分配器内部数组尺寸）与 §1.55 两个容量 round 同类、非裁决**，故自主采 C：`MAX_BIG_BLOCKS` 抬到覆盖观测峰值（big 峰值 47）的 64（每槽 `Option<BigBlock>` 实测 24 B，(64−32)×24=+768 B<1 页不跨未物化页，另加编译期不变量锁死「增长<1 页」）+ 回归测。**真机 `-m 512M -smp 1` 双跑 bn23/bn23b 签名一致**（58507/58511 行、`pagefault-in-VM=0`、EXIT=124、推进到 57715 才 OOM vs bn20 的 29010）。**新墙 B35**＝OOM-RT 换签 `big=2f/40 px=400/400 fp=39c/400`（表未满 47/64、bump 游标 1024/1024 耗尽、而 free-stack 搁浅 924 单页）——`alloc_big`→`supply_pages` 只从 bump 区发 run、1 页 big 请求也不查 `supply_page` 的单页 free-stack（§1.55 记录碎片的同族再现）。下一配方＝给 `alloc_big` 失败路径加「表满 vs 游标尽而 free-stack 有货」diag，再判修向（1 页 run 委托 `supply_page`？还是原则解 A/B）。三件套绿（mock minix-rt **58·0fail** 基线+1 / fmt WT0==HEAD0 / clippy exit0 / 双跑一致）+ CodeReview **0 MUST、1 SHOULD 采纳**（`Option<BigBlock>` 尺寸 16B→实测 24B、doc 数值全据实修正）、CONSIDER(diag) 留 B35。rc marker 仍未达。详见 §1.59/§1.60。
>
> **⚠（1.58/1.59 历史·其头号前沿 B34 已于 §1.60 采保守绑界 64 落地、翻出 B35）B33b 已修（exec 权限检查传低 3 位 X_BIT 不再传 9 位 owner-X 位 0o100，root 执行 0o100755 不再被误拒 EACCES，/bin/sh 首次获控 Runcom 15→1）→ 翻出 B34 运行期堆 OOM-RT（1.58，含代码·VFS 侧）**：§1.57 头号 **B33 残余 EIO** 拆为两层。**B33b（真阻塞，已修）**＝`exec_worker.rs:670` 调用 `forbidden_decision` 时传 `access: 0o100`（9 位模式 owner-可执行位），而 `forbidden_decision`（`protect.rs:229-261`，与 C `protect.c:238-287` 逐行一致）末判据 `(perm | access) != perm` 要求 access 与 perm 同在**低 3 位**三元组；root 的 `perm=0o7`，`0o7|0o100=0o107≠0o7`→恒 `EACCES`。两枚探针实锤（已回滚）：bn18 `nk4a: b33 E0000000d`＝Err 臂、正值 13(EACCES)；bn19 `nk4a: b33m 000081ed 0000 0000`＝mode `0o100755`（可执行位齐）、euid=0、file_uid=0——root 执行自己 owned 的可执行文件被误拒。修＝`access: crate::protect::X_BIT`（全仓其余 `forbidden_decision` 调用点传的均为 `crate::open::*` 低 3 位常量、无一用 9 位模式值，唯此腿错档），并把带常量的 `ForbidInput` 构造抽成纯函数 `exec_forbid_check` 令调用点进入单测射程 + 两条回归测。**B33a（符号掩盖，登记 follow-up 未修）**＝`ExecError::to_errno()` 返回正值经 INIT 成功车道吞成 EIO，真实 errno 被掩盖；修复方向＝归一化 `pm_exec` 的 `Err` 臂为负。真机无探针双跑 **bn20/bn20b 签名一致**：`Runcom` 15→**1**（`/bin/sh` 首次获控、无循环）、`bogus=0`、`eacces=0`、`marker=0`、`panic-enter=2`（30068/30066 行）。三件套绿（mock vfs lib **533·0fail** 基线+2 只增不减 / fmt exec_worker.rs+protect.rs WT0==HEAD0 / clippy exit 0 / 镜像重建+真机双跑 bn21/bn21b 与 bn20 同签名）。**CodeReview 0 MUST、2 SHOULD 全采纳**（① 修正失真注释“均传 open::X_BIT”→可核表述；② 测只钉被调方不钉调用点→抽 `exec_forbid_check`+调用点回归测）。**新头号前沿 B34**＝panic 现场 `nk4c: OOM-RT … big=20/20 px=094/400`（十六进制：`big=0x20/0x20`=32/32 记录表满、`px=0x94/0x400`=148/1024 空闲页仍在）。【§1.59 取证】根因实锤＝`minix-rt` 的 `big_blocks` 跟踪表硬编 32 先于池字节填满的 premature-OOM（与已修 `MAX_SLABS` 同类，非池字节耗尽）；但按 `MAX_SLABS` 纪律把表绑到 `GLOBAL_POOL_PAGES`（32→1024）后**真机确定性从 30090 行崩回 124 行**（对照 bn22ctl 实锤）——分配器静态实例变大平移 VM 的 .bss/用户栈基址，撞上 boot 未 eager 物化 VM 自身栈/.bss 尾页的缺口（PREALLOC demand-fill 分歧）。正解被反复 defer 的结构债（boot eager 物化 / VM-backed heap supplier）挡住，属**架构裁决级**：已回退未提交（改动存 `stash@{0}`），待用户定 A(boot eager 物化)/B(VM-backed 长堆)/C(保守绑界64+修自栈缺页)。rc marker 仍未达。详见 §1.58/§1.59。
>
> **⚠（1.57 历史·其头号前沿 B33 已于 §1.58 修 B33b、翻出 B34）B32 已修（exec_worker 同步 FS 腿补回事务打包 TRNS_ADD_ID/DEL_ID，伪指针 errno 归零翻为合法 EIO）→ 翻出 B33 残余 EIO（exec_via Ok→EIO 掩盖，VFS status 符号待取证）（1.57，含代码·VFS 侧）**：§1.56 头号 **B32（exec 回复携带伪 errno `-5114394`＝用户堆指针 0x4E0A1A）根因实锤并修复**。静态算术实锤：`FS_BASE=0xA00`、`REQ_LOOKUP=0xA1A=2586`，伪值 `0x4E0A1A`＝`(78 status << 16) | REQ_LOOKUP` 的事务打包形态；C 黄金参照 `minix3/minix/servers/vfs/comm.c:21` TRNS_ADD_ID / `main.c:88` TRNS_DEL_ID。根因＝`exec_worker.rs` 同步 `ipc.sendrec(fs_e)` 绕过 `fs_comm::send_fs` 异步路径的事务戳入/拆封，裸发 `m_type=REQ_xxx`（FS 按 `m_type>>16` 路由→call=0 无法路由）且把回复 `m_type` 原样 `neg()` 当 errno。修＝`neg()` 后新增 `fs_trans_stamp`/`fs_trans_status` 两助手，对 4 个同步 FS 腿（lookup/stat/req_read/ELF segment-read）接入；**VM 腿（vm_mmap/procctl_clear）与 PM 腿（pm_newexec）taskcall 风格不打包、保持不变**。运行时硬证（bn16 临时探针已回滚）：`b32 exec path=/bin/sh errno=5` 命中 44 次全干净 errno 5、伪指针归零；无探针双跑 bn15/bn15b 签名一致（79303/79319行、panic=0、bogus=0、Runcom=15、marker=0）。三件套绿（mock vfs lib 531·0fail 基线+1 新回归测 `test_fs_transaction_pack_roundtrip` 只增不减 / fmt exec_worker.rs WT0==HEAD0 零新增 / clippy exit 0 / 镜像重建+双跑一致）。**新头号前沿 B33**：exec_via 现返回**合法 errno 5（EIO）**而非伪指针，marker 仍未达——init `exec_via` 将任何正/零回复（Ok 车道）映射为 EIO（对位 C `execve.c:53-58`，成功路径永不返回），真问题＝VFS `pm_exec` 现返回正 status 被 Ok 车道吞成 EIO、或某下游腿（stat/read/mmap）失败被 EIO 掩盖；下一取证配方＝在 VFS pm_exec 各 bail 站点与 `queue_reply_msg` 终局打印 status 符号与来源腿。rc marker 仍未达。详见 §1.57。
>
> **⚠（1.56 历史·其头号前沿 B32 已于 §1.57 修复并翻出 B33）B31 已修（imgrd 补播 /bin/echo）+ B32 实锤（exec 回复携带伪 errno＝用户堆指针 0x4E0A1A，sh 从未获控）（1.56，含代码·xtask 装配面）**：§1.55 头号 B31（Runcom↔SingleUser 15 轮循环、rc marker 不出）第一层根因实锤＝**`/bin/echo` 从未播种进 imgrd**——`generate_etc_proto` bin 段只播 `sh`，而 rc 唯一外部命令是 echo，本重写 shell 只有 exit/cd 内建（C ash 有 echo 内建但镜像亦独立装 /bin/echo，播种对位）→ 一旦 exec 腿通 PATH 全 ENOENT → sh 退 127 → runcom 回落。修＝proto 签名列表化 `bin_entries` 播 `sh+echo`、构建步扩 `-p minix-fileops`（echo 属 fileops、不在 BOOT_MODULES 从不为 module target 构建）、测试同步，xtask 12/12。然播后 bn12 与 bn11 完全同形——探针三连（bn12p/q/s）实锤下一层 **B32**：rc 子进程 exit=5（`runcom.rs` exec 失败腿 `exit_process(5)`）、sh 内嵌探针零触发（main 从未进）、`exec_command` 六腿 A-E 全过腿 F `pm::exec_via` 失败、原始值探针坐实 **sendrec 往返 OK 但回复 `m_type=-5114394`（0x4E0A1A，落在用户堆 VA 区）＝指针被当 errno**——VFS `pm_exec` 错误车道全 `neg()` 透传是嫌疑面，下一单元从 VFS 终局 reply 原始值二分。B31 修复独立正确（B32 通后必撞 echo 缺件）；B32 既存非本单元引入。三件套绿（mock 817+3/57/418/243 不减·xtask 12/12 / fmt image.rs WT4==HEAD4 / 双跑 bn13==bn13b panic=0·Runcom=15·探针零残留），四探针文件全部回滚干净。下一前沿：① 头号 B32 伪 errno；② init stall 疑似不睡（15×30s 塞进 150s）；③ 结构债三件套；④ PREALLOC 裁决；⑤ SMP 竞态；⑥ §1.51 遗留。rc marker 仍未达。详见 §1.56。
>
> **⚠（1.55 历史·其头号前沿 B31 已于 §1.56 修复第一层并翻出 B32）最新前沿 = B29 VFS 堆 OOM + B30 内核页表池耗尽，双容量扰动落地后真机首次零 panic 跑到 7.7 万行（1.55，含代码·minix-rt/boot-shim 两参数）**：§1.54 头号前沿 **B29 定性实锤＝合法工作集超固定池、非泄漏**——三行取证：① 40960 失败分配主＝`exec_worker.rs:296 hdr_buf`（`HEADER_BUF_PAGES=10` 页，签名 `size=00a000` 逐位吻合）；② VFS 静态表足迹实测（临时 size_of 测试，跑毕回滚）`FProc`4368B×256＋`Filp`96B×1024＋`Vnode`120B×1024≈**327 页** vs 池 512 页，且 `FProc` 翻倍元凶＝`filps: [Option<usize>;255]` 16B/槽（C 指针 8B）；③ `supply_pages` 连续 run 只从 bump 区发、free 栈只养单页（alloc.rs L328 注释自陈）⇒ bump 耗尽后散页 170 无法拼 10 页 run=必炸。C 对位：表在 BSS、堆经 VM brk 无上限（`brk.c _syscall(VM_PROC_NR,VM_BRK)`；RS 默认就要 8MiB prealloc map `rs/const.h:83`）。**修（本单元＝容量扰动，结构修记 follow-up）**：`GLOBAL_POOL_BYTES` 512→1024 页。签名翻出 **B30**（bn9m）：`vmctl_set_addr_space … AllocationFailed`——B26 fix-B 每次新根 kerninfo 注入从 boot-shim 的 `boot_pt_alloc`（仅 128 页、纯 bump 无 free）烧 1-3 页，~40 次 bind 耗尽=确定性内核 panic；扩 `prepare_boot(128→1024)`（代价：`vm_handoff` 整段从 VM free list 扣除，每 boot 多占 4MiB）。附带物理预算钉桩：bn9（默认 128M）死 `exec_bootproc boot segment page allocation failed`＝bootproc 腿 eager materialize 随池扩容线性涨页，**测试台标准配置自此为 `-m 512M`**。真机 bn10/bn10b/bn11/bn11b 四轮 **0 panic**（77418/77996/75882/77757 行，签名一致），boot 历史首次推过全部已知崩溃点。**新前沿 B31 现场已刻画**：birth 停在第 8446 行（26 个），尾部 6.9 万行＝多服务事件循环 pick spam（slot 8/1/0/a/b 分布、非活锁死形），rc marker 未出。取证中还原两处事实错误：旧注释「池页按需物化不占物理」对 bootproc 腿不成立；本重写 VM 的 `MAP_PREALLOC` 只记位不预取帧（C `region.c:492` 真预取）＝**PREALLOC 语义缺口**新存量（demand-fill 使其外部行为近似等价，登记待裁决是否补实现）。三件套绿（mock kernel 820/rt 57·0fail / fmt 三文件零新增漂移 / 镜像重建+双跑一致×2）+ CodeReview **1 MUST-FIX 已修**（`nk4c_oom_tag` 探针分母写死 `/200`＝扩容后打 `px=400/200` 非法形态污染签名判据→改派生常量真值）+3 SHOULD 全采纳（PREALLOC 断言精确化/WORKLOG 登记（本节）/bump 扣除代价入注释）+2 NIT（hex 记法/PoolStorage 陈旧 doc）。**下一前沿（按序）**：① 头号＝**B31 rc marker 未达**——birth 停 26 后 INIT 命令面卡在哪一步（bn11 尾部 VFS/PM 活动正常，需定位 init runcom 的等待点）；② 结构债三件套：VM-backed heap supplier（C brk 保真）、boot bump 可回收/映射责任移交 VM、`MAX_BIG_BLOCKS=32` 同族；③ VM PREALLOC 语义缺口裁决；④ SMP 早期启动竞态（§1.52）；⑤ §1.51 遗留。rc marker 仍未达。详见 §1.55。
>
> **⚠（1.54 历史·其头号前沿「B29 VFS 运行时堆 OOM」已于 §1.55 容量定性+扰动修复，并翻出 B30 同族）最新前沿 = B26 kerninfo 双缺陷闭环 + B28 PM 位掩码→枚举语义坍缩修复，boot 推进 20 倍停在 B29 运行时 OOM（1.54，含代码·arch/kernel/PM 三侧）**：§1.53 头号前沿「B26 PM↔VFS 乒乓」根因反转两次收敛——**乒乓是下游后果，真凶＝kerninfo 交付链双缺陷**。缺陷 A（车道断链）：`set_secondary_ipc_return` 写 `ctx.rbx`，但 int33 立即返回臂只同步 RAX+R10（§1.40 迁移遗留）→ 页地址永达不到用户，用户 `ipc_trap` 从 R10 拿陷阱残留垃圾 → `new_image_stack_top` deref 崩（bn1 实锤 rip=0x204e61=`cmpq $0x10,(%rax)`、rax=1）；修=车道改 `gp_regs[GP_R10]`（C i386 %ebx 一条车道兼状态+secondary 两职、按调用互斥，`ipc_minix_kerninfo.S`+`proc.c:685-693` 对位）。缺陷 B（新根缺映射）：VM 建的根 `PDPT[2]==0`——kerninfo.rs 模块 doc 登记的「mapping responsibility 随 VM 接管转移」从未发生（C 经 VMCTL_KERN_PHYSMAP 把 `.usermapped` 映进每进程空间，i386 memory.c:746/847）；修=内核在 `vmctl_set_addr_space` 唯一提交点先查后装注入（`KERNINFO_PAGE_PHYS` 缓存 + read_only + boot_pt_alloc）。签名翻转链 bn4（A 修后翻为 noaddr cr2=0x200000040＝B 实锤）→ bn5（全消、init done/run enter、18 服务全诞）。bn5 尾暴露 **B28**：PM panic `assert(EXITING)`（slot 13 正常退出）——根因＝`Lifecycle` 互斥枚举把 C `mp_flags` OR 位语义压成替换语义：`zombify` 后（C `forkexit.c:619/621` 只 OR ZOMBIE、EXITING 位直到 cleanup 才清）VFS EXIT 回复到达时 `is_exiting()` 已 false。修=`is_exiting()` 覆盖全死亡窗口（Exiting|TraceZombie|Zombie|ToldParent），15 调用点逐一对位 C 位测试，两测试随 C 对齐（`process_ksig` 终止子现返 EDEADEPT＝signal.c:372-378 忠实形态，旧 ok 断言是缺陷镜像）。**真机 bn6-bn8**：notEXITING=0、活动 27998→47853 行（+71%），停在下一前沿 **B29＝VFS 运行时堆 OOM**（40KB 分配失败 `px=200/200 fp=0aa/200`），bn7==bn8 无探针双跑签名逐字确定。三件套绿（mock kernel **820**/pm lib **418**/arch **243**·0fail / fmt 九改动文件 HEAD==WT 零新增漂移 / 镜像重建+双跑一致）+ CodeReview **1 MUST-FIX 已修**（qemu-tests `test-user-trap` 手编 payload 仍读 RBX 车道→`mov rax,r10`/`mov rax,[r10]` 字节替换+sh/注释同步）+1 SHOULD（trap_dispatch 陈旧注释）。side 报告对账：诊断一＝§1.52 已落地；诊断二（B27 页表竞态/VMINHIBIT）与本单元实测无关（B26 真根因为 kerninfo 链）；「syscall_process.rs 未提交」不实。登记存量：minix-tests `pm_sched` 编译 E0046（HEAD 同红）、run_once_integration 2 失败（m_type errno 符号约定存量）。**下一前沿（按序）**：① 头号＝**B29 VFS OOM**（512 页 .bss 池 bump 耗尽＋170 空闲页不可复用＝big-block 连续性/碎片问题；对位已登记 follow-up＝VM-backed heap supplier（alloc.rs 模块 doc）+ `MAX_BIG_BLOCKS=32` 同族债）；② SMP 早期启动竞态本体（§1.52）；③ §1.51 遗留低频 bit47 / `stacktrace.rs:150`；④ latent `platform_sources`。rc marker 仍未达。详见 §1.54。
>
> **⚠（1.53 历史·其头号前沿「B26 乒乓」已于 §1.54 闭环＝kerninfo 双缺陷，乒乓为下游后果）最新前沿 = VM free-list 空真根因 = firmware-heap 腐化（非扣减过度），fix27c 同病第三发已修（1.53，含代码·内核侧）**：§1.52 登记的头号前沿「VM handoff free-list 布局鲁棒性」**根因反转**——带探针单核实跑（vh1）显示 `classify()` 时刻原始 memmap 快照**本身就是空的**（15 条 conventional 条目全部读回 length 0），根本不是「扣减把 usable 扣光」。真根因＝**fix27c 同病第三发**：`KernelInfo` 的 `memmap`/`boot_modules` 两个 `&'static` slice 指向 boot-shim 的 UEFI 池堆（`Box/Vec leak`），这些页在内核启动推进后被破坏——同一 run 内 Step 4 DM-coverage 还能经 `kernel_info.memmap()` 打出 16 条真数据（dm-mem 行），到 `vm_handoff::classify` 就全零，而 `KernelInfo` 结构体本身（同堆 `Box::leak`）其它字段仍完好＝**payload 页被复用/破坏、非结构损坏**。`reserved_regions` 早被 fix27c 用 `.bss` landing pad 保护过（当时就注明「固件池堆撑不过 boot」），`memmap`/`boot_modules` 是漏网同病。**修＝扩展 landing pad 范式到全部 payload**：`store_kernel_info` 在 arch_boot 入口（数据尚完好时）把 `memmap`（`ptr::copy` 到 `MEMMAP_REGION_STORE`，上限 `MAXMEMMAP=128` fail-fast）、`boot_modules`（逐条拷到 `BOOT_MODULE_STORE`，name 字节落 `BOOT_MODULE_NAME_STORE` 定长池、15 字节 strlcpy 式截断与 `vm_handoff::copy_name` 同界）深拷贝进 kernel `.bss` 并重指全局副本；re-store 幂等守卫与 fix27c 同款。`BootModule` 加 `#[derive(Debug,Clone,Copy)]`。**真机决定性**：修后单核 `classify` 时刻 memmap 16 条实数据、`vm_handoff free n=8`（原 0）、`boot.rs:159` panic 消失，boot 从「VM 崩于诞生前」推进到 **11 boot exec 全过 + runtime birth 链（slot 0xd–0x15）+ 空闲轮转**；残留形态＝INIT/runcom 后 PM(0)↔VFS(1) 轮转主导（pick 各 ≈4.5 万次）＝**B26 家族 livelock 现场复现**（§1.49-A 登记的 exec_command null deref 下游），成为新头号前沿。三件套绿（mock **820**·0fail 只增不减·+1 为 landing 回归测试 / fmt 三文件 HEAD==WT 零新增漂移（globals.rs 一处 16→17 已对齐）/ 镜像重建无探针两次单核签名一致 fm1==fm2 `kernel panic=0`·`no free memory`=0·exec=11）+ 探针版 lh1 机制验证后按纪律回滚 + CodeReview **PASSED 0 MUST/SHOULD-FIX**（确认：unsafe 裸指针生命周期/幂等/截断无消费者分叉/`param_buf` 两路径硬编 `&[]` 无风险/`platform_sources` 同病但零 post-boot 消费者＝登记隐患非本 unit）。**下一前沿（按序）**：① **头号＝B26 PM↔VFS 乒乓活锁**（本轮单核已把它重新推到主路径：用只打印寄存器探针抓 INIT(runcom) 在 `exec_command` 的 null+1 deref 指令现场，§1.49-A 已有半条证据链）；② SMP 早期启动竞态本体（§1.52）；③ §1.51 遗留低频 birth bit47 / `stacktrace.rs:150`；④ latent：`platform_sources` 未 landing（当前零消费者）。rc marker 仍未达。详见 §1.53。
>
> **⚠（1.52 历史·其「下一前沿 #1 VM free-list」根因已被 §1.53 反转为 firmware-heap 腐化而非扣减过度）最新前沿 = BKL 单原子化硬门 + §1.49 SMP 竞态实锤（1.52，含代码·内核侧）**：关闭 §1.50 登记的 SHOULD-FIX「BKL 两步写自死锁硬门」——`BKL_LOCKED: AtomicBool`+`BKL_OWNER: AtomicI32` 两分离原子的 `bkl_lock`(CAS locked 后 store owner)/`bkl_unlock`(store owner 后清 locked) 两步写在 `IF=1`（idle 唤醒调度循环）留「locked≠owner」窗口，同 CPU 中断落入即自旋等自己持有的锁=单核自死锁（boot 全程 `IF=0` 不触发）。**修=合并为单个 `static BKL: AtomicIsize`**（-1=空闲/≥0=持有者CPU id；acquire 单 CAS、release 单 store、inherit `load==me`），locked 与 owner 物理同字、窗口结构性消失，等价 C `spin_lock_irqsave`；改动全局部 `smp.rs` 六函数、调用方零改动（grep 确认两 static 仅内部引用）。**并做控制实验钉死 §1.49 悬案**：修后 SMP 同镜像两次签名发散（c66 15.9MB OOM / c67 13KB picknone），遂 stash 回 HEAD 原样重建对照——**控制组 HEAD 自身 `-smp 4` 亦 2-3 次 kernel panic**、而 `-smp 1` 四次全部 `kernel panic=0` 且结构签名一致（size 33185，含我的改动 bk==control，diff 仅 UEFI 一页基址抖动）⇒ **§1.49「早期 boot panic 是真 bug 还是 `-smp 4` 竞态」定性实锤=多核启动竞态，非确定性 bug**。方法论：单核 `-smp 1` 是可复现确定性测试台（`xtask qemu` 硬编 `-smp 4`，直呼 qemu 绕过）。三件套绿（mock **819**·0fail / fmt `smp.rs` HEAD=0==WT=0 / 真机改单核基线零回归坐实）+ CodeReview **PASSED 0 MUST/SHOULD-FIX**（采纳 CONSIDER#2 澄清 `bkl_try_lock` doc 旧「peer=继承」混淆态）。**下一前沿（按序，详见 §1.52）**：① **头号（单核测试台本轮立即掘出）= VM handoff free-list 布局鲁棒性**——单核 `-smp 1` 轨迹实锤 `kernel: vm_handoff free n=0x0 deducted=0x17` → VM 崩于 `servers/vm/src/boot.rs:159` `free_regions.is_empty()`（=`boot-shim` 装载点本页后移一页，内核 free-list 扣减把 15 条 usable 全扣光；S0 登记不修的布局鲁棒 bug 重现，**非 BKL 回归**，是挡 rc marker 的确定性头号前沿）；② SMP 早期启动竞态本体（`-smp 4`+只打印寄存器探针定位 BSP/AP bringup 时序，独立于 BKL 的另类竞态）；③ §1.51 遗留（低频 birth bit47 / `stacktrace.rs:150` 往返野地址 / `pagefault VM rip=0 cpu3`〔本轮证据属 SMP 竞态家族〕）。rc marker 仍未达。详见 §1.52。
>
> **⚠（1.51 历史·其「SMP 页表竞态」定性已被 §1.52 控制实验细化：现存 SMP panic 确为多核竞态、但独立于本 unit 关的 BKL 自死锁窗口）最新前沿 = B27 copy 层根因修复（1.51，含代码·内核侧）**：**证伪 §1.50 的「SMP 页表竞态 / VMINHIBIT 缺口」假设**——B27 里那个每次必现、卡死 boot 于 172 行的 `kdst copy pa=0x0000800005d90a30`（bit47 伪物理地址）真根因是**确定性 code bug**：`os/arch/src/arch/direct_map.rs` 的 `DirectMapArch::virt_to_phys` 只认两个 Direct Map 窗口（kernel DM `0xffff808000000000` / VM DM `0x80000000`），对**内核 image 高半区 VA**（内核栈/BSS/`IMAGE_HEAP` 堆，全在 `0xffff800000000000+`、两窗口之外）走 else 分支盲减 VM 基址 → 伪 PA。c56/c57 探针直取 `virt_to_phys(virt=0xffff8000003ffb30) -> 0xffff7fff803ffb30` 坐实。**修=18 个 boot-path copy 站点**统一从 `AddressRef::Physical(virt_to_phys(内核VA))` 改 `AddressRef::Process { endpoint, offset: VirBytes(内核VA) }`（进程 CR3 同时映射内核 higher-half，`resolve_physical` 走真页表得正确 PA；范式对位 `os/kernel/src/syscall.rs` dispatch_diagctl 的 boot-span 恒等式注释）：`syscall_process.rs`×2(exec name_buf/IPC filter)、`syscall_signal.rs`×3、`syscall_copy.rs`×4、`misc.rs`×7(getinfo/trace×4/sprof×2)、`syscall_device.rs`×3(vdevio/sdevio)、`kmess.rs`×1(kmess 快照堆)、`stacktrace.rs`×1(user 回栈 scratch)。**真机决定性**：原确定性 corrupt 归零，boot 从「每次卡死 172 行」前进到**全部 17-18 服务诞生 + 进入用户态 exec**；stacktrace.rs:249 修复另消除 c61 的 `rip=0xffff7fff…` 伪回栈野值（c62/c63 归零坐实）。**残留=新前沿（非本 bug）**：c60-c65 六次中仅 c64 出现 **1 次（1/6 低频、不阻塞 boot）** 的 birth 期 `copy pa=0x0000800005d90a30`——结构上是 bit47 落在 `ADDR_MASK` 内被当地址、**非 `virt_to_phys` 算术输出**（后者产 `0xffff7fff…` 形态），指向 birth 页表 walk 偶读到被 free/复用的中间表页（这才是 §1.50「页表生命周期」假设的**真实、低频、异机制**版本，side-conversation「诊断二」在此点上部分成立）。三件套绿（mock kernel **819**/·0fail 只增不减 / fmt 全 7 文件零新增漂移、syscall_copy 反而 44→0 / 真机 c64+c65 签名一致=17 服务 + `pagefault VM rip=0 cpu3`）+ CodeReview PASSED（无 MUST/SHOULD-FIX）。**下一前沿（按序）**：① 低频 birth 页表-walk bit47（用 walk 前后同址双采 PTE 探针定位是哪一级被并发 free）；② `pagefault for VM rip=0 cr2=0 err=0x15`（B26 家族用户态 null 取指）；③ `stacktrace.rs:150 kernel_direct_read_word` 的 `virt_to_phys→kernel_phys_to_virt` 往返对 image 高半区内核栈产野地址（c60 `recursive panic` 源，本 unit 未盲改）；④ BKL 两步写单原子硬门（§1.50 SHOULD-FIX）。rc marker 仍未达。详见 §1.51。
>
> **⚠（1.50 历史·部分结论已被 §1.51 证伪）B27 部分修（含代码·内核侧）**：坐实 B27 一类真根因=中断/陷阱入口用 `bkl_try_lock()`（单 AtomicBool CAS）无法区分「本 CPU 内核帧持有（继承）」与「别的 CPU 持有（须自旋获取）」，user/idle-origin 陷入误继承 peer 锁→两 CPU 并发 `&mut PROC_TABLE` 撕裂写。修=`smp.rs` 新增 `BKL_OWNER: AtomicI32` owner 跟踪 + `bkl_lock_or_inherit()`（owner==self 才继承、否则自旋获取），把 clock 0xF1 臂/PIT TIMER 臂/`save_irq_frame_to_context`/SCHED_IPI 臂/int33 IPC 入口/irq_manager dispatch·claim ×3/riscv·aarch 同型站点全换用之。**真机决定性**：原 B27 签名（`cr2=0xffff807f85d7b6a8` IpcEngine::send 读坏表指针）**从干净重建消失**，崩溃收敛为**确定性**（c54/c55 同 172 行、同一根腐蚀点）。~~**残留 B27**：birth 期页表 walk bit47 = VM 并发改页表竞态（候选 VMINHIBIT 缺口）~~ **【§1.51 纠正：该「每次必现、卡死 172 行」的 bit47 corrupt 实为确定性 `virt_to_phys(image VA)` code bug，非竞态，已修；仅 1/6 低频 birth 残留才是页表生命周期家族】**。三件套绿 + CodeReview PASSED（无 MUST-FIX）。**BKL 两步写硬门**（`bkl_unlock`/`bkl_lock` 中 `BKL_OWNER`+`BKL_LOCKED` 两步写在 IF=1 窗口可致同 CPU 自死锁，boot 全程 IF=0 不触发，根治=合并单 `AtomicIsize`）仍未做、排 §1.51 后续。详见 §1.50。
>
> （1.49 历史）B27 起因：为取 B26 崩溃 rip 加内核探针，意外发现回滚至干净 HEAD `ef07f760f` 全量重建 + 同镜像三连（c46/c46b/c46c）稳定复现早期内核 panic（`rip≈0x5c3f309 err=0 cr2=0xffff807f85d7b6a8 walk=NP` cs=0x8），即 §1.48「c43/c43b 真机签名一致」不可从干净重建复现、真机签名对构建产物敏感。前代「B26 RBX=0 根因」误判已作废（RBX=0 全进程普遍，含正常 0xc）。

- **B14 已修（1.26 落地，含代码）**：VFS 同步挂载路径 `WireFsClient::send`（`request.rs`）两处缺陷根治——**A**：裸 `m_type=REQ_READSUPER(0xA1C)` 经 `sendrec` 直发，绕过 C `fs_sendrec` 的 `TRNS_ADD_ID`（把 request 号抬到高 16 位）→ MFS 侧 `TransactionId::decode`(call=raw>>16) 解出 call=0、index 下溢 → `Unserved`→回 ENOSYS，read_super 从未抵达 MFS 挂载门；修=`sendrec` 前 `msg.m_type = trns_add_id(REQ_READSUPER, 0)`（同步靠阻塞往返匹配、id=0 安全，CodeReview 独立确认 id=0 < `IS_VFS_FS_TRANSID` 的 0xB02 下界不误路由）。**B**：`decode_readsuper_reply` 旧无视回复状态字无条件返 Ok，把 ENOSYS 当挂载成功让 `do_init_root` 带病开门；修=sendrec 后 `trns_del_id(msg.m_type)!=0` 即 `Err(FsError::Io(status))` 上抛（恢复 C `req_readsuper` `if (r!=OK) return r;`，boot 应 panic 见 main.c:519-520）。**真机取证**：加探针（已删）实测 PFS readsuper 回 `0x00000000`(status0=OK 通过)、MFS 回 `0x001e0000`(status=30=**EROFS**) 正确上抛——**Fix B 无假阴性、Fix A 已让 MFS 真服务**。三件套绿（docker 813/242/528·0fail / fmt request.rs 17=17、main_loop.rs 1=1 零新增 / 两次真机 c1/c2 签名一致=23313 行、`main_loop.rs:7681 failed to initialize root: Io`）。CodeReview 无 MUST-FIX。**boot 现诚实停在下一步 B15。**⚠️ 本 bullet 前代 §1.25 把症状记为 **EBUSY(16)**：本轮实测本 build 根挂载失败真码是 **EROFS(30)**（EBUSY 是「已挂载再挂」旧时相的下游表现），见 §1.26。

- **B15 已修（1.27 落地，含代码）**：MFS 根挂载 dirty-mark 不再 EROFS——`fs/fs-rt/src/source.rs::ImgrdBlockSource` 加**有界 CoW 覆盖层**（`overlay: BTreeMap<u64, Vec<u8>>`）。修向：`read_block` 先查 overlay、miss 落 Static 基座（clip 短尾零填，保持 C `memory.c:442-443` 边界规则）；`write_block` 分 arm——`Owned` 就地写不 populate overlay（测试/future writeback 路径）、`Static` 不再返 EROFS 而是写落 overlay（部分越界存 surviving、整块越界 no-op）；`from_static` 构造点初始化空 overlay；`EROFS` import 删除。**dirty-mount 只写块 0 → overlay 1 条 ≈ block_size + 树节点，内存有界**（vs 8 MB base 无法 Owned 化、2 MB slab pool 装不下）。C 忠实性：memory 驱动自有缓冲的 RAM 盘可写；本 overlay 是 bdev 通道未接前的过渡形态，进程终止即丢=与 C 语义一致。宿主 fs-rt 27/27 pass（新增 4 测：overlay 遮蔽读写 / clip 短尾 / 整块越界 no-op / 多块独立），mfs 133/fs 178/vfs 530 全绿。三件套绿（docker 813/242/528·0fail / fmt source.rs HEAD=1 NEW=1 零新增 / 两次真机 c3/c4 签名一致=25213 行、`init-state Runcom` @24127 同位、无 panic/EROFS/Failed to init）。CodeReview PASSED 无 MUST-FIX（SHOULD-CONSIDER=Owned 热路径多做一次 overlay get 可加注释；NICE-TO-HAVE=image() doc 说明不含 overlay——均非破坏性，本轮保持 diff 最小不采纳）。**boot 现前进到 `init Runcom` 相位**。⚠️ 不选「改根挂载只读」缩窄目标路径（非 C 忠实、破坏单元 D/I 写需求）。

- **B16 部分修（1.30 落地，含代码；仅 Fork/SrvFork 两臂取负）**：候选根因确认——PM `Err(e) => ReplyIntent::Reply(PmError::from(e).to_errno())` 传正 errno、`reply()` 无取负写 `msg.m_type = 正` → init `perform_syscall` 判 `m_type ≥ 0` 走 Ok 分支、将 EAGAIN/ESRCH 当 child_pid → 假成功父无子、waitpid 卡（与 C `_syscall`/F10b `reply_wire()` 契约失配）。修复：`calls.rs` L234-247 的 `PmCall::Fork` 与 `PmCall::SrvFork` `Err(e) => ReplyIntent::Reply(-PmError::from(e).to_errno())`；测试 `test_dispatch_fork_parent_unknown_is_error_reply` 断言同步；`cargo test -p minix-pm --lib` **417/417 pass**。**真机 c8/c9**：两次签名一致（`init-state Runcom` 各 1、`nk4a: pm 0140b` 各 8）；c8 35863 行 vs c7 25013 = **活动 +43%（子/兄弟进程真在跑：`pfwd nr=1..9 out=B` 全 boot 模块页 fault 转发到 VM 服务）**——fork 链已通、系统进入下一停点 B17（rc marker 仍未出）。

- **B17 已修（1.34 落地，含代码·内核侧）**：根因坐实——内核 `syscall_process.rs::dispatch_fork`（SYS_FORK）把 **RECEIVING 校验、`fork_from` 拷贝源、`parent_is_sys_proc` priv 解析、应答 `msgaddr` 取值**四处全错用 `caller_nr`，而 minix-rs 里 **VM 代表父进程陷入 SYS_FORK**（`vm/fork.rs:343 gateway.sys_fork(parent.endpoint(),…)`），故 `caller_nr`=VM ≠ 被 fork 的父。C `do_fork.c` 四处一律用 `isokendpt` 从消息 `endpt` 解出的父 `rpp`（L41/44/51/63/105/112），从不用 `caller`（旧 Rust 注释「C guarantees rpp==caller」是假前提）。运行时 VM 不处于 RECEIVING → 返 `KcallResult::Ok(EINVAL)`（SYSCALL 腿经 `syscall_leg_wire` 取负成 `-EINVAL`，`minix-sys::sys_fork` 的 `reply<0` 正确判错，故**错误契约本就对、非 bug**）→ VM `VmForkError::KernelCall` → PM `dF:vmF` → fork 永久失败。修=`let parent_nr = proc_table.endpoint_to_nr(Endpoint(fork_req.endpt))`（isokendpt 等价、已排除 SLOT_FREE 故覆盖 C `isemptyp(rpp)`），四处改用 `parent_nr`；`caller_nr` 形参改名 `_caller_nr`（对齐 C 未用的 `caller`）。**证据链**：宿主 `endpoint_to_nr`/位比较语义核对 + 新增 2 测（`test_t12_fork_resolves_parent_from_endpt_not_caller` 旧码下必挂、`test_t12_fork_rejects_unresolvable_parent_endpt`）+ 修好被掏空的 `rejects_non_receiving_parent`（令其真达 RECEIVING 闸，CodeReview W1 采纳）。**真机（决定性）**：干净 HEAD 基线 c14 vs 修后 c16/c17——slot 1..0xa pick 计数**逐位相同**（slot8=763/763 等）=无早/boot 回归；仅 B17 活锁的 slot0↔slotb ping-pong 从 **907/665 → 311/63** 坍缩=**fork 不再 EINVAL、子真被创建**。三件套绿（docker kernel **815**/arch 242/vm 528·0fail / fmt syscall_process 54=54 零新增 / 两次真机 c16==c17 签名一致；c17 之后的改动全是 `#[cfg(test)]` 体与注释、release 内核二进制逐字节不变，签名沿用）。⚠️ **探针布局脆弱性登记仍有效**（§1.33）：本修刻意只动内核+minix-types 语义、不碰 PM，正是绕开该债。

- **B18 已修（1.36 落地，含代码·VFS 侧）**：根因坐实+C 交叉验证——**VFS 回执未 echo 目标进程端点**。C `service_pm`（minix3 `vfs/main.c`）每一路回复都 `proc_e=m_in.VFS_PM_ENDPT; m_out.VFS_PM_ENDPT=proc_e;` 把请求目标端点回填进回执 `VFS_PM_ENDPT`(=m7_i1)；PM `handle_vfs_reply`(main.c:315-321) 从 m7_i1 取端点 `pm_isokendpt` 解槽位再断言挂 `VFS_CALL`。但 minix-rs `VfsReply::encode()`(`minix-types/ipc/vfs.rs`) 对所有变体把 m7_i1 **恒置 0**（裸回复）从不 echo→PM 解 `Endpoint(0)`(=`Endpoint::PM`、PM 自身 mproc 槽、in_use 但无挂起 VfsCall)→`take_vfs_call(slot 0)` **panic「reply without request (slot 0)」**。修=新增 `VfsReply::encode_reply_for(target)`（`encode()` 上补写 `m7i1=target.get()`，不改 `encode()` 签名零 churn）+ `vfs/main_loop.rs` `Route::Pm` 两成功站点携带端点。`VfsCall::endpoint()` 对 Fork/SrvFork 回**子端点**、与 PM `do_fork` 挂 VFS_CALL 于子槽精确对齐。**三件套绿**（docker kernel 815/arch 242/vm 528 · minix-types 309(含新测 `test_vfs_reply_encode_reply_for_echoes_endpoint`✓)/minix-vfs 530 · 0fail / fmt vfs.rs 2=2・main_loop.rs 1=1 零新增 / 真机 c18==c19 结构签名一致(wc 同 24926)、**reply-without-request panic 彻底消除、ECALLDENIED/pmstall 归零**，boot 从 c16 的 24332 推进到 **24926 行**）。CodeReview **无 MUST-FIX**（union 写同一活跃臂 SAFETY 成立）。⚠️ **邻近边登记**（CodeReview SHOULD、不阻塞）：`Route::Pm` 两条 **Err 分支**（`queue_reply(SyscallResult::Error/Nosys)`）仍发裸 `-errno`（m7_i1=0、负 m_type 不命中 `is_vfs_pm_rs`）→ PM 误路由、原 VFS_CALL 悬挂；系**本修前既存**隐患（C 里那些 handler void 不回错），boot 关键路径 fork/exec/exit 均成功不命中（c18 实测 0 panic 佐证），本轮保持 diff 最小未纳入。**以下为 §1.35 历史侦察记录（终态机制推导当时未知根因）：**修后 boot 过 Runcom、fork 链通，但 rc marker 仍未出。**终态实锤=PM panic**：`servers/pm/src/ipc/vfs.rs:385: handle_vfs_reply: reply without request (slot 0)`（c16 第 24310 行，日志止于 24332；行首带 `file:line:` 前缀=标准 panic 格式，非 diagctl）。这是 C `main.c:324` `assert(p->P_flags & VFS_CALL)` 的忠实对位——**PM 收到一条 VFS 回执，其 `m_u.m_m7.m7i1` 端点经 `pm_isokendpt` 解析到 PM UserSlot 0（该槽 `endpoint()==回执端点` 且 `is_in_use()` 成立，但 `ipc_blocked` 非 `VfsCall`）→ `take_vfs_call` panic**。panic 后 PM 终止/自旋：紧随 `nk4a: ipcerr caller=0x0 err=ECALLDENIED`（caller 0=内核槽位 PM 自身，其 panic 后 IPC 被 `!GET_BIT(s_k_call_mask)` 拒）→ 内核 `pmstall2`（trap_dispatch.rs:814，检测内核 slot0=PM 连续 running>5000 tick=自旋）→ gtick 空转至超时、boot 死。`Endpoint::PM=Endpoint(0)`/`INIT=Endpoint(11)`（endpoint.rs:61/72），故 **slot 0 ≠ init，而是 PM 自身槽位**；且 fork 子端点 `slot()` 应为 12+（boot 0..11 已占），**故触发 panic 的回执并非 `VFS_PM_FORK_REPLY` 本身，而是另一条携带 slot-0 端点的 VFS 回执**（疑 PM 代表某进程发起、VFS 却以 PM 端点回投递，或代际/端点错配）。PM `do_fork`（fork.rs:23）已读通：`child_endpoint=vm_fork(...)`→`copy_mproc` 设子 PM 端点→`tell_vfs(UserSlot::new(child_slot), VfsCall::Fork{child,parent,child_pid})` 把 VFS_CALL 挂**子槽**、发 `VFS_PM_FORK`。⚠️ **取证阻断**：§1.33 登记的 PM 布局脆弱性阻断一切 PM 侧探针，无法取「panic 回执的 opcode/m7i1 具体值」现场。**下一入口（关键）**：VFS **服务端**（`fs/` 系，非 PM、**不受 PM 布局脆弱性阻断**）是 VFS_PM_FORK/EXEC 回执 `m7i1` 端点的构造方——下轮优先读 VFS `req_fork`/`req_exec`/投递回执的 encode 站点，查其 echo 的端点从请求哪个字段取（是 `child`、`m_source`、还是请求者 PM 端点？），坐实「哪条回执带 slot-0 端点、为何」。基线 c14 尾亦有 `ipcerr caller=4 err=EDEADSRCDST`，故 IPC 错误非本修独有、系 fork 真起子进程后才走到的既有下游浮现。rc marker 未达，frontier 仍 1.30。

- **B19 已修（1.37 落地，含代码·PM 侧）**：根因——`PmServices::sched_start_user`（`vfs.rs:439`）传给 `crate::sched::sched_start_user` 的 `ep` 参数 = `resources.scheduler`（=Endpoint::SCHED）而非子进程自身端点。C `schedule.c:55` `sched_start_user(scheduler, rmp)` 载荷 `m.SCHED_ENDPOINT=rmp->mp_endpoint`，本实现误将 scheduler 当 schedulee，导致 SCHED 收到 INHERIT 后对 SCHED 自身(slot4) 而非子进程(slot12+) 做 schedctl→**子进程 NO_QUANTUM 永不消除→永不调度**。修=传 `table.procs[slot].endpoint()`（子自身端点）。**证据**：c20/c21 确认 `schedctl caller=0x4 tgt=0x0c`（子槽）、`pick->0xc`（子被调度），post-Runcom 结构签名一致。三件套绿（docker 815/242/528·0fail / fmt 789=789 零新增 / PM 417测全绿）。**新前沿 B20**：子进程首次执行即触发 kernel GP fault（vector 13, rip=0x1dbf5f49, errcode=0x0）——内核上下文切换到子时崩（可能与 VM_FORK 后子的页表/CR3/寄存器状态设置有关）。rc marker 未达，frontier 仍 1.30。

- **B20 已修（1.38 落地，含代码·内核侧）**：根因坐实+C 交叉验证——内核 `KProcess::fork_from`（`proc.rs:1594`）把子的 `cpu_context` 塞 `CurrentCpuContext::default()`（**全零：rip=0/rsp=0**）、`trap_style` 塞 `TrapStyle::NoEntry`。调度器首次 pick 子后走 `finish_and_restore`/`restore_user_context`（`lib.rs:3357-3382`）：`NoEntry` 的 `return_sequence()` 返 None → `panic!("no entry trap style known")`（对位 C `arch_system.c:597-598`），即观察到的 kernel #GP(vector13, cs=0x8 内核态, "no entry trap style known")。C `minix3/minix/kernel/system/do_fork.c:63` `*rpc = *rpp` **整体拷贝父 proc 结构含 `p_reg` 全部寄存器与 `p_kern_trap_style`**，line 74 仅重置 `rpc->p_reg.retreg = 0`（子见 pid=0 走 child 分支）。修=`fork_from` 改 `cpu_context: parent.cpu_context` + `trap_style: parent.trap_style`（继承父帧，子在其上首次调度即从父陷入点返回用户态），并在函数返回前 `set_ipc_return_code(&mut child, 0)`（写 RAX=0，经 `write_user_register` offset 80=gp_regs[0]=RAX，对应 C retreg=0）。**证据（决定性）**：c22/c23 两次真机签名一致——**#GP/`no entry trap` 彻底消除**、子进程 `pick->0xc` 后**真实执行用户代码**（`pre-restore- rip=0x204b52/0x226d50/0x227c80` 等真实代码地址、VM 服务其 `vm-pf` 页故障），post-Runcom probe 序列同构（仅页帧地址 run-to-run 抖动）。三件套绿（docker kernel **816**(新增测 `test_fork_from_inherits_register_frame_and_trap_style`)/arch 242/vm 528·0fail / fmt proc.rs 43=43 零新增 / 两次真机 c22==c23 签名一致、panic 碎片逐字节相同）。CodeReview **PASSED 无 MUST/SHOULD-FIX**（确认继承语义=C `*rpc=*rpp`、`set_ipc_return_code` 位置不冲突、父 fork 时刻 trap_style 恒为 FullContext 不会继承到 NoEntry/Syscall）。NICE-TO-HAVE=断言子 RAX==0 未采纳（arch 无 `read_user_register`、`gp_regs` 系 pub(super) 跨 crate 不可读，强加将扩 diff）。**新前沿 B21**：见下条。

- **B21 取证（根因收敛前的真机实锤，1.39 纯侦察 doc）**：c22/c23 确定性签名——子进程运行、VM 服务页故障若干轮后，PM 主循环探针（init.rs:451，`receive()` Ok 臂内、is_notify 判前）打印 `nk4a: pm 0030b`（WAIT4 from INIT=0xb）紧接 `nk4a: pm 000ff`（**m_type=0x000、`m_source.0 & 0xff`=0xff**）→ `panic-enter` → PM 死 → `ipcerr caller=0x0 err=ECALLDENIED` → `pmstall2` 自旋至 timeout（EXIT=124，boot 停 ~24402/24405 行）。**探针解码坐实**：`Endpoint::KERNEL=Endpoint(-1)`（0xFFFFFFFF & 0xff=0xff），故 source=**Endpoint::KERNEL/HARDWARE(-1)**（纠正初判「255/NONE」——`Endpoint::NONE=Endpoint(SLOT_TOP-2)≈32766`&0xff=0xFE 不符）。**⚠️ 修正初判「m_type=0/非 notify」（1.39 真机取证 c29/c30）**：PM 探针（init.rs:452 `m_type as u16`、只打低 12 位）对 `NOTIFY_MESSAGE=0x1000`（bit12，com.h:90）恰好打「000」——与真 m_type=0 **不可区分**，初判「m_type=0/非 notify」系探针截断假象。加**内核侧**（避 §1.33 PM 阻断）取证探针 dump 完整 m_type：c30 唯一一次 Phase-1 notify 到 PM 打印 `b21n mt=0x1000 src=0x7bff ns=-2(SYSTEM)`——**实锤 notify 就是 mt=0x1000**（探针会打成 000），且 `build_notify_message`（ipc.rs:2925）**不设 m_source**（留 `Message::default`=0x7bff→PM 打「bf」，即 `pm 000bf`，非本次崩的 `000ff`）。而 Path-A(b21a)/Phase-3(b21q)/Phase-2(b21s) **均无 src=KERNEL 命中**→崩前那条 `000ff`（src=0xffffffff、mt 低12位0）**必来自未覆盖的直投 notify 臂 `mini_notify_core`（ipc.rs:3019-3033，`kernel_mini_notify` IRQ 入口 caller_nr=proc_nr::KERNEL→`NotifySource::Hardware`、m_source=HARDWARE=-1、m_type=0x1000）**——该臂 line 3031 明确 `ipc_status_add_call(dst, IpcCall::Notify)`。**故定性**：这是一条**硬件中断通知（HARDWARE notify）**，C `main.c:64` 里 PM 本就会收到并靠 `is_ipc_notify(ipc_status)` 无条件 `continue` 跳过（C PM main.c 明确对 CLOCK 外的 notify 静默 continue），**唯 `is_notify()` 误判 false 才会下探到 `pm_isokendpt(KERNEL)` 失败 panic**。**根因缩到两点之一**：①`mini_notify_core` 直投臂给 blocked-in-receive 的 PM 加 Notify status 位（写 `cpu_context` 的 ipc_status 寄存器），但 PM 被唤醒后经调度器 restore 回用户态时该寄存器**未 sync 到返回帧**（对比 Phase-1 接收者自发 receive 陷阱腿 trap_dispatch.rs:1810 `sync_status_register_to_frame` 是同陷阱内完成）；②`ipc_status_add_call`（proc.rs:1805）在 `MF_REPLY_PEND` 置位时跳过加 Notify 位。**下一入口（精确）**：给 `mini_notify_core` 直投臂（dst==PM 时）加临时探针 dump `ipc_status_register(&procs[dst].cpu_context)`（add_call 之后）+ PM 当时 `p_misc_flags`（是否 REPLY_PEND），一次真机即可二分手①/②；若①=修 restore/sync 路径让直投唤醒也带 status；若②=对齐 C（notify 与 reply_pending 的 status 门纪律）；亦须评估 `pm_isokendpt` 是否该对 HARDWARE/KERNEL 保留源 `continue`（C 靠 is_ipc_notify 已滤，若 status 修好则无需）。§1.33 PM 布局脆弱性阻断 PM 侧新探针——取证走内核侧。rc marker 未达（`minix-rs rc: minimal boot script marker` 仍未打印），frontier 仍 1.30。**本轮 c29/c30 临时内核探针已全部 `git checkout` 回滚**（tracked diff 空），纯侦察 doc commit、无生产码、无三件套。

- **B21 已修（1.40 落地，含代码·arch 侧）**：根因坐实——`sync_status_register_to_frame`（`os/arch/src/x86_64/trap_stub.rs`）是 IPC 状态车道 **RBX→R10 迁移的陈旧漏改**：状态位现由 `or_ipc_status_reg`/`clear_ipc_status_reg`/`ipc_status_register` 读写 `gp_regs[GP_R10=8]`、trap 出口 asm `pop r10`、userspace `ipc_trap`（arch_trap.rs:84 `mov status,r10`）从 R10 读 status，但该 sync 仍做 `frame.rbx = ctx.rbx`。在接收进程**自身** int-33 receive 陷阱返回臂（内核 `trap_dispatch.rs:1810` reply-code 路径，唯一调用点）上，内核已把投递的 `IpcCall::Notify` 位 OR 进 `gp_regs[GP_R10]`，却只把 RBX 拉回帧 → `frame.r10` 停在 RECEIVE 序言清零的旧值 → userspace `is_ipc_notify` 读到 0 误判非通知 → 那条 `mini_notify_core` 直投的 HARDWARE notify（m_type=0x1000、src=KERNEL/HARDWARE=-1、被 init.rs:452 探针低12位截断打成 `pm 000ff`）下探到 `pm_isokendpt(KERNEL)` 失败 → init.rs:567 panic。修=改 `frame.r10 = ctx.gp_regs[crate::x86_64::signal::GP_R10]`（同步用户真正读的状态车道）+ 诚实化函数注释 + 回归守卫单测 `sync_status_register_copies_r10_lane_not_rbx`（`ctx.rbx` 干扰值双向锁死「R10 收到状态、RBX 不被污染」+ 复刻 `is_ipc_notify` 的 6-bit 掩码断言）。**真机 c31/c32 决定性**：`panic-enter`=0（PM 不再崩）、那条 `nk4a: pm 000ff` 命中 1 次后被正确识别跳过、`pre-restore- r10s=0x0000000000000004`（Notify status 正确交付进 R10）出现 115 次，两次行数 25284/25285 同构=签名一致。三件套绿（docker arch **243**(head 242 +1 新测)/kernel 816/vm 528·0fail / fmt trap_stub.rs cur=17=head=17 零新增 / 真机 c31==c32 签名一致）。CodeReview **PASSED**（无 W/S；N1 陈旧 RBX 注释多在 pre-existing 内核侧、不阻塞，本轮已采纳修自身新增 rustdoc 的 broken intra-doc link）。⚠️ 修复只动 arch crate、不碰 PM（规避 §1.33 PM 布局脆弱性债）。**新前沿 B22**：见下条。

- **B22 前沿（⚠ 根因再纠正=INIT fork 子空指针 SIGSEGV、作废前「入队违例」方向、详见 §1.43）**：B21 修复后 PM 存活、boot 推进至 ~25285 行 timeout（EXIT=124），末态全系统静态死锁（§1.42）。§1.42 猜「child queued=no=入队违例」**已作废**：核对 C 黄金参照 `minix3/minix/kernel/proc.h:169 rts_f_is_runnable(flg)==((flg)==0)`——SIGNALED 置位即非 runnable 是 **C 忠实正确**（信号交 PM 经 GETKSIG 的 `rts_unset(SIGNALED)` 入队半唤醒子），child `flags=0x30 queued=no` 无违例。**真根因坐实（§1.43）**：c32 全 boot 唯一一条 `csig tgt=0xc sig=0xb`，`sig 11=SIGSEGV`；现场上文 `pf-exit noaddr cr2=0x0`→**INIT fork 子(12) fork 后解引用空指针**、VM 判 noaddr→`cause_signal(SIGSEGV)`→子永停 0x30。addr2line `0x21b3e4`=`minix_sys::fork`、objdump 该处=`int $0x21` 后 `mov %r10,%rsi`（**子从 fork sendrec 正常返回、fork 链已通**），SIGSEGV 崩在子返回后 INIT 用户代码下游。exec 探针覆盖 nr 0x5/7/9/a/b 但**无 0xc**→子从未 exec /bin/sh→rc marker 不可达（与「无 sh/rc exec 痕迹」闭环）。次生：PM 空闲未 `sys_getksig` 清理该 SIGSEGV，但对 marker 已次要（子已崩）。**下一入口（精确）**：`pfc` 崩溃-rip 探针过滤器加 12 +build+跑 c33→**实锤 child 崩于 rip=0x21b3e4 cr2=0x0 err=0x4**（详 §1.44）。addr2line+objdump：0x21b3e4=`minix_sys::fork` 内 `int $0x21`（sendrec-to-PM）**返回后首条** `mov %r10,%rsi`（本身不访存）——即子从 fork sendrec 的 trap-return 臂未正常完成，内核把 PM fork REPLY 拷回子消息缓冲时子的 reply-buffer/描述符指针（应=rsp+8 栈址）=**0**→读地址0 fault（延续 B20 `fork_from` 子帧继承链：`cpu_context:parent.cpu_context` 整体拷贝未带 reply-buffer 指针）。**下轮取证**：dump child 崩溃时完整用户帧（rdx/rsp/r10）与父同点对比坐实。rc marker `minix-rs rc: minimal boot script marker` 仍未打印，frontier 仍 1.30。【→ 本行已于 §1.45 闭环：c34 帧 dump 实锤 **child(12) 切进时 `p_seg.phys_root=0`（无任何自有页表）**→以空 cr3 跑首个用户访存即 SIGSEGV（确定性、非竞态）。根因=VM 腿 `fork.rs:344 sys_fork(parent, child.slot())` **不传 PFF_VMINHIBIT 且未为子发 SetAddrSpace 绑页表**，而未实现 C 的「VMINHIBIT 持子至绑页表后清」配套纪律。修复方向（下轮、需真机验 rc marker）：VM sys_fork 传 PFF_VMINHIBIT + 为子绑页表后再 VmInhibitClear（或 fork_from 继父 phys_root 共享，与 C `*rpc=*rpp` 一致）。】

- **B22 已修（1.46 落地，含代码·VM 侧）**：根因坐实（§1.45 铁证 c34）——INIT fork 子(12) 被调度时 `p_seg.phys_root=0`（无自有页表）、无 VMINHIBIT 持制→以空 cr3 跑首个用户访存即 SIGSEGV、从未 exec /bin/sh→rc marker 不可达。根因=VM fork 腿未落实 C 的「VMINHIBIT 持子 + 绑页表后清」配套纪律：C `minix3/minix/servers/vm/fork.c:89-95` = `sys_fork(..., PFF_VMINHIBIT, ...)` **紧跟** `pt_bind(&vmc->vm_pt, vmc)`（尾部=`sys_vmctl_set_addrspace`，内核 `vmctl_set_addr_space` syscall.rs:2925 存 phys_root + step5 清 VMINHIBIT）；而 minix-rs `fork.rs:344 gateway.sys_fork(parent.endpoint(), child.slot())` **只传 2 参（flags 硬编 0→内核不置 VMINHIBIT）且从不调 set_addrspace（子 phys_root 永为 0）**。修（忠实移植 C 两腿）：①`kernel_gateway.rs` trait `KernelGateway::sys_fork` 加 `flags: u32` 参（真实 impl 转发给 `minix_sys::syscall::sys_fork` 第 4 参、wire `m_lsys_krn_sys_fork.flags`→内核 `complete_fork_setup` 按 `flags & fork_flags::VMINHIBIT(0x01)` 置 VMINHIBIT）；②`fork.rs` 新增 `const PFF_VMINHIBIT: u32 = 0x01`（对位 com.h:360）并在 `sys_fork` 后、`handle_memory_once` 前插入 set_addrspace 腿（`child_root_phys = <PageTable as Paging>::root_paddr(child.page_table_mut()).0`→`gateway.sys_vmctl_set_addrspace(child_endpoint, child_root_phys, 0)`，Direct Map 下 virt alias 传 0、与 boot 路径 vm_server.rs:750-770 同款）；修正原 fork.rs:324-327「A1 adoption、sys_fork 内完成地址登记、无独立 bind 步」的错误注释（真机证伪）。MockGateway.last_fork 扩为 `(Endpoint,UserSlot,u32)` 记录 flags + 新单测 `test_do_fork_holds_child_then_bounds_addrspace`（断言 sys_fork 收 PFF_VMINHIBIT、set_addrspace 恰 1 次且目标=child/root≠0/virt=0）。**真机验证（决定性）**：c35/c36 两次签名一致——**SIGSEGV(`csig`)=0（B22 消除）**、panic-enter=0、vector13=0、行数 **26296=26296**（boot 从 ~25285 前进 ~1000 行，所有系统服务 + init 均正常 exec、子不再崩溃；新出现 `vm-pf … fa=0x21b3e4`＝VM 正常服务子 fork 返回页故障而非致命）。三件套绿（docker kernel 816/arch 243/vm **529**(+1 新测)·0fail / fmt 3 文件零新增漂移 fork 30=30 gateway 21=21 vm_server 119=119 / 两次真机签名一致）。CodeReview **PASSED（MUST-FIX=0）**，采纳 SHOULD-FIX（修 trait 头部注释残留「`m1i3 = flags (0)`」与新 flags 参矛盾）；NICE-TO-HAVE（set_addrspace 失败诊断锚点）本轮未采纳以保持 diff 最小、fail-closed 与相邻腿一致（另：评审提及的 vm_server.rs B5/B4 额外改动经核为**已入 HEAD 的历史提交**、非本工作树 diff，属误报）。⚠ 修复只动 VM crate（fork.rs/kernel_gateway.rs/vm_server.rs test mock）、不碰内核/PM（规避 §1.33 PM 布局脆弱性债）。**新前沿 B22→B23**：见下条。

- **B23+B24+B25 已修（1.48 落地，含代码·三根因联治）**：B22→B23 前沿暴露三个 IPC/VM/PM 链条缺陷，全部忠实移植 C 语义修复。
  - **B23**（vm_server.rs）：`sys_vmctl_memreq_reply` WHO 参数 `req.target`→`req.requestor`（C pagefaults.c:206 `sys_vmctl(state->requestor,...)`）——跨进程拷贝挂起时 target（故障页空间）≠requestor（挂起者），回包砸错槽→EINVAL→VM drain return→真请求者永卡 VMREQUEST→全系统死锁。
  - **B24**（ipc.rs）：`receive` 入口加 SENDING 门（C proc.c:999 `if(!RTS_ISSET(caller, RTS_SENDING))`）——SENDREC receive 半带 SENDING 进来应跳过扫描直接停车 `getfrom=src_e+RECEIVING`；同时删除 drain 腿的“代停车 sender+设RECEIVING”旧 workaround（该 workaround 不存 pdmv 导致回包投递砸 0x0）。真机 c35/c37 实锤连锁：receive 半不在此停车→drain 不存 pdmv→DeliverMsg 挂起 start=0x0→VM 拒绝→崩。
  - **B25**（exit.rs PM）：`tell_parent` 加 `if addr.0 != 0` 守卫（C forkexit.c:692 `if (addr) { sys_datacopy... }`）——INIT 的 `waitpid(pid, NULL, 0)` 使 rusage_addr==0 合法，旧实现无条件 copy_to_user(144B, INIT, 0x0) → SYS_VIRCOPY → 内核挂 PM 问 VM → VM 拒 → errno 回投 INIT pdmv=0 → SIGSEGV → panic（c41 susp-krn 探针实锤 `nr=0x0 mt=0xf tgt=0xb st=0x0 ln=0x90`）。
  三件套绿：docker kernel 816/arch 243/vm 529/pm 418·0fail✓ / fmt 6文件零新增漂移✓ / 两次真机 c43/c43b 签名一致（0 panic / 2 csig on slot 0xd / 3 pf-exit / PM↔VFS spin）✓。CodeReview PASSED。

- **B26 侦察纠正（1.49 纯侦察·无生产码，作废前代「RBX=0 根因」误判）**：前代 compacted 会话记「slot 0xd cpu_context RBX=0 是根因」——**本轮 c43 探针输出直接证伪**：`rbxw int33-save/irq-save` 探针显示 **RBX=0 对全系统每个进程普遍成立**（ep=0x1/0x2/0x5/0xa/0xb 均 rbx=0，连**正常工作的 slot 0xc** 也 rbx=0），而紧邻的 `pre-restore-` 行（读自待恢复的 cpu_context）里 0xd 的 rbx=0x7fffffffa540（真实栈/消息缓冲地址）。⇒ `rbxw` 探针读的是 int33 陷阱帧入口的 RBX（syscall 序言前），非用户态消息缓冲指针，RBX=0 与崩溃无因果。**真崩溃链**（c43 line 26806-26819）：0xd `int33-save/ret rip=0x204e3f`（syscall 门，与 0xc 同址）→ VM 服务其腿 `vm-pf recv` → `pf-exit noaddr cr2=0x1` → `csig tgt=0xd sig=0xb(SIGSEGV)` ×2。addr2line(init ELF) 0xd 崩溃前后 rip：0x20c510=`RawVec::grow_one`、0x204e61/0x2014c1=`execve::exec_command`、0x2245c0=`Vec::from_iter`、0x2067a0=`fmt::format_inner`——即 0xd（INIT 第二个 fork 子，runcom 的 `runetcrc(true)` chroot 分支）在 **`exec_command` 路径 deref 近零指针（addr 0x1）**。cr2=0x1 = null+1（结构体偏移 1 处读空指针）。**未定案**：现有 `pfc` 探针过滤 `cur_nr∈{0,1,3..11}&&vaddr==0`，不含 12/13 且不含 vaddr=0x1，故 c43 未捕获 0xd 的确切 faulting rip；本轮试图扩探针取证——**见 B27（该取证尝试反而暴露更硬的阻塞）**。

- **B27 新阻塞（1.49 实锤，⚠ 取代 B26 成为头号前沿·比 B26 更早）**：为取 0xd 崩溃 rip，加了两版内核探针（先扩 `pfc` 过滤器、再改独立安全探针），**均导致早期 boot 内核 panic**（rip≈0x5c34005/0x5c51309，cs=0x8，`walk=NP`，多 CPU panic 处理竞态致 rip/计数漂移）。一度疑为探针引入，**但 `git checkout` 回滚至干净 HEAD `ef07f760f` 后重跑 `xtask image --release` + QEMU（c46/c46b/c46c 同镜像三连）复现同样早期 panic**（panic 计数 2/3/4、行数量 208/202/211 微抖=正常，故障本身确定）：**决定性根因签名** = 内核数据读 `rip=0x5c3f309 err=0x0 cr2=0xffff807f85d7b6a8 walk=NP`（Direct Map 基址 0xffff808000000000 之下≈2GB 处一个未映射高半区指针，cs=0x8 内核态读，`pfm1-gate..pfm5-dump` 崩于 pagefault 处理中）。**⇒ 重大流程发现**：§1.48「c43/c43b 两次真机签名一致」的三件套验证**不可从干净重建复现**——真机签名对**构建产物敏感**（增量构建陈旧 artifact vs 全量重建、或工具链/registry 漂移，令同一 commit 的 boot 相位从「推进至 Runcom+0xd」退化到「早期内核 panic」）。**这意味着 ef07f760f 名义上「三件套绿」实则未固化一个稳定可复现的无 panic 基线。**
  - **下一入口（B27 优先，须先于 B26）**：①先重建**可复现无 panic 基线**——回滚到 §1.48 之前那个能跑到 Runcom 的镜像来源，比对 `xtask image` 全量 vs 增量、锁定 `cr2=0xffff807f85d7b6a8`（≈DirectMap 基址下 2GB 处）是哪块内核数据结构的指针（页表 walk 目标 / boot info memmap / slab 元数据）；②判明是**真 bug** 还是 `-smp 4` 早期启动竞态（可试 `CARGO_TARGET_DIR` 干净重建 + 单/多核对照）；③在稳定基线之上再回到 B26（0xd exec_command cr2=0x1 取证 + PM↔VFS 乒乓活锁）。**取证探针纪律重申**：内核缺页探针内**严禁页表 walk/DirectMap 读内存**（c44 证伪：会在非预期 fault 上下文递归 panic），只可打印寄存器。
  rc marker 仍未出现（`minix-rs rc: minimal boot script marker`），且现退化至早期 boot panic，frontier 名义仍 1.30 但**实际可复现停点早于 B22 之前的 Runcom**。本轮无生产码改动（工作树干净），纯侦察 doc，免三件套。


【→ §1.47 c35 全量 tail-dump 已导致 B23+B24+B25 三修实施，见上条。】


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

## §1.38　B20 修复：内核 fork_from 塞 default cpu_context + NoEntry trap_style→子首次调度即 #GP

**受控读法接续**：本 turn 从 §1.37（B19 已修，commit 967b613c9）后的 B20 前沿下一入口起步（读内核 schedule 后 context switch + VM do_fork 是否设子 p_cr3）。HEAD=967b613c9，工作树 clean 起步。

**addr2line + 日志钉死终态**：c22 尾 `no entry trap style known` + `trap: vector 0xd err 0x0 rip 0x1dbf5f49 cs 0x8`（cs=8=内核代码段、rsp=0xffff8000003ff8b0=内核栈）→ 非用户态崩，是内核 `restore_user_context` 路径 panic（`lib.rs:3382`，PF 碎片化的 `no entry trap style known` 先于外层 `trap_dispatch.rs:1004` 的 vector13 报告）。`ELF` 加载基址与链接地址 0xffff8000002xxxxx 不同（物理低位 0x1dcxxxxx），addr2line 直接解需 slide，但 panic 串已自证落点，未纠缠。

**根因（读码坐实 + C 交叉验证）**：
- `KProcess::fork_from`（`proc.rs:1594`）构造子时 line 1662-1663 塞 `cpu_context: CurrentCpuContext::default()`（x86_64 default = 全零 gp_regs + rip=0/rsp=0）+ `trap_style: TrapStyle::NoEntry`；
- `dispatch_fork`（`syscall_process.rs:205-234`）在 fork_from 之后**无任何**补设子 cpu_context/trap_style 的站点（line 237 注释谎称「retreg=0 set by fork_from via p_reg initialization」——default 根本不拷父帧）；
- 调度器 pick 子 → `finish_and_restore`（`lib.rs:3353-3383`）读 `p.trap_style`，`NoEntry.return_sequence()` 返 `None` → `panic!("no entry trap style known")`（对位 C `arch_system.c:597-598`）；
- **C `do_fork.c:63` `*rpc = *rpp` 整体拷贝父 proc 结构**（含 `p_reg` 全部寄存器 + `p_kern_trap_style`），仅 `do_fork.c:74 rpc->p_reg.retreg = 0`。子因此从父陷入点返回用户态、retreg=0 走 child 分支。Rust 版把这两处丢成了 default/NoEntry。

**修复**（`os/kernel/src/proc.rs::fork_from`）：
1. `cpu_context: parent.cpu_context` + `trap_style: parent.trap_style`（继承父帧，忠实 `*rpc = *rpp`）；
2. FPU 继承块之后、函数返回前调 `set_ipc_return_code(&mut child, 0)`（现成 API，经 `write_user_register` offset 80=gp_regs[0]=RAX，对位 C retreg=0）；
3. 新单测 `test_fork_from_inherits_register_frame_and_trap_style`：给父设 `trap_style=FullContext` + 写 rip/rsp，fork 后断言子 `trap_style==FullContext` 且 `apply_to_trap_frame` 后 frame.rip/rsp 继承父值（守护 B20 回归）。

**CodeReview 确认（PASSED 无 MUST/SHOULD）**：`set_ipc_return_code` 位置安全（x86_64 `inherit_fpu_state` 仅拷 fpu_policy 不碰 gp_regs；`complete_fork_setup` 仅改 flags/name；dispatch_fork 尾部仅 move）；C `do_fork.c` `*rpc=*rpp` 后显式重置的字段（p_nr/endpoint/时间/misc mask/NO_QUANTUM/cycles/cpuavg/信号标志/p_pending/页表/privilege）在 fork_from/dispatch_fork 均有对位处理，cpu_context/trap_style 系本轮补齐的两个遗漏；父进 dispatch_fork 前已验证 RECEIVING→经 IPC 入口→`trap_dispatch.rs:133` 恒置 FullContext，且生产无路径置 Syscall（S-8 deferred），故子继承到的必为 FullContext（可派发），不会继承 NoEntry/Syscall。

**三件套取证**：
- 镜像 build：`xtask image x86_64 release` EXIT=0 ✅；
- Docker：kernel **816**（+1 新测 ✓）/ arch **242** / vm **528** = **0 fail**（基线 815/242/528 只增不减 ✓）；
- fmt 零新增漂移：proc.rs cur=43=head=43；
- 真机 c22/c23 两次：EXIT=124、24402/24405 行、**`vector 13`/`no entry trap`=0**（GP 彻底消除）、子进程 `pick->0xc` 后真实执行（`pre-restore- rip=0x204b52/0x226d50/0x227c80` 真实代码地址 + VM 服务 `vm-pf`）、post-Runcom probe 序列同构、panic 碎片（`servers/pm/src/i`+`nit.rs`+len=0x1a+`237`+len=0x1）**逐字节相同**（确定性）。

**新前沿 B21**：子真跑后 PM 主循环对 `pm 000ff`（m_type=0x000、source=0xff=255 非法端点）panic（init.rs run-loop L567 invalid-endpoint 或 L594 handle_vfs_reply，loc 串 26 字符），PM 死→ECALLDENIED→pmstall2 自旋至 timeout。非 B20 回归（此前子跑不到此）。rc marker `minix-rs rc: minimal boot script marker` 仍未打印，frontier 仍 1.30。

### §1.38 本 turn 行量
- 生产代码 1 文件 2 处（`kernel/src/proc.rs::fork_from` 继承父帧 + retreg=0）+ 1 新单测；
- WORKLOG 顶部 B20→已修 bullet + B21 前沿 bullet + 本节 §1.38；
- 三件套全绿 + CodeReview PASSED；**含代码修改的 fix commit**。

---

## §1.40　B21 修复：`sync_status_register_to_frame` RBX→R10 迁移陈旧漏改→自身 receive 陷阱返回臂丢 notify status→PM panic

**受控读法接续**：本 turn 从 §1.39（B21 取证，commit 987b82c63）后的 B21 根因二分手（假设① sync/restore 丢 status vs 假设② REPLY_PEND 跳过加位）起步。HEAD=987b82c63，工作树 clean 起步。

**纯读码坐实机制（未再加探针，避开 §1.33 PM 阻断）**：
- IPC 状态车道在 x86_64 已从 RBX 迁到 **R10 = `gp_regs[GP_R10=8]`**（caller-saved，避免摧毁用户 callee-saved RBX）：`or_ipc_status_reg`（boot.rs:241）/`clear_ipc_status_reg`(:253)/`ipc_status_register`（trap_stub.rs:587）全读写 `gp_regs[GP_R10]`；`save_frame_to_context`:537 `ctx.gp_regs[8]=frame.r10`；trap 出口 asm `pop r10`（trap_stub.rs:356）；userspace `ipc_trap` 从 R10 读 status（arch_trap.rs:84 `mov status,r10`）；调度器 restore（trap_return.rs:123 `mov r10,[gp+8*8]`）正确、`apply_to_trap_frame` 只搬 rflags/cs/ss/rip/rsp（不搬 gp_regs）。
- **唯一漏改点 = `sync_status_register_to_frame`（trap_stub.rs:553）仍 `frame.rbx = ctx.rbx`**。其唯一调用点是 `trap_dispatch.rs:1810`（接收进程**自身** int-33 receive 陷阱、`result.reply_code()=Some` 返回臂）。内核在该臂前已把投递的 `IpcCall::Notify` 位 OR 进 `gp_regs[GP_R10]`，但 sync 只回填 RBX → `frame.r10` 停在 RECEIVE 序言被 `clear_ipc_status_reg` 清零的旧值 → 出口 `pop r10` 交付 0 → userspace `is_ipc_notify(0)` 判 false → 那条通知下探到 `pm_isokendpt(KERNEL)` 失败 → init.rs:567 panic。
- **假设裁决**：假设①成立（sync 拷错寄存器），假设②（REPLY_PEND 跳过加位）不需要——`mini_notify_core` 直投臂 line 3031 已无条件 `ipc_status_add_call(Notify)`，位写对了，纯粹是**回传帧时寄存器选错**。`ipc_status_call_to(Notify=4)=(4&0x3F)<<0=4` 编码正确。

**修复**（`os/arch/src/x86_64/trap_stub.rs::sync_status_register_to_frame`）：`frame.rbx = ctx.rbx` → `frame.r10 = ctx.gp_regs[crate::x86_64::signal::GP_R10]`；重写函数 doc 交代 RBX→R10 迁移历史 + B21 触发链；采纳 CodeReview N1 中本轮自撰部分——把新注释里 `or_ipc_status_reg` 的 broken intra-doc 链接（该符号是 boot.rs 的 trait 方法、本模块不可解析）降为普通文本。新增回归守卫单测 `sync_status_register_copies_r10_lane_not_rbx`：置 `ctx.gp_regs[GP_R10]=4` + `ctx.rbx=0xdead_beef` 干扰值，断言 `frame.r10==4`（状态车道同步到位）、`frame.rbx==0`（RBX 不被污染）、`frame.r10 & 0x3F == 4`（复刻 is_ipc_notify 判定）。

**CodeReview PASSED**：确认状态车道确应取 `gp_regs[GP_R10]`（入口 `push r10`→save gp[8]→出口 `pop r10` 三段同源）、不再触碰 RBX 正确（`save_frame_to_context` 已 `ctx.rbx=frame.rbx`、旧 RBX→RBX 实为恒等空操作、RBX 现系用户 callee-saved 内核写它反成污染）、唯一调用点语义一致无遗漏。N1 非阻塞=pre-existing 内核注释仍写 RBX（`trap_dispatch.rs:1798/636`、`arch/lib.rs:348`），不在本轮生产 diff、留待探针口径统一时处理。

**三件套取证**：
- 镜像 build：`xtask image x86_64 release` EXIT=0 ✅；
- Docker：arch **243**（head 242 +1 新回归测 ✓）/ kernel **816** / vm **528** = **0 fail**（基线只增不减 ✓）；
- fmt 零新增漂移：trap_stub.rs cur=17=head=17（新单测 TrapFrame 字面量按 rustfmt 逐字段展开形态书写以对齐基线）；
- 真机 c31/c32 两次：EXIT=124、25284/25285 行、**`panic-enter`=0**（PM 不再崩）、`nk4a: pm 000ff` 各命中 1 次后被正确识别跳过（其后无 panic）、`pre-restore- r10s=0x0000000000000004`（Notify status 正确交付进 R10）各 115 次、两次签名同构。

**新前沿 B22**：PM 存活后 boot 推进至 ~25285 行 timeout，尾态反复 `pre-restore- rip=0x202d68 rsp=0x7fffffff9178 r10s=0x4`（rip 落在 `KernelIpcTransport::receive`、r10s=4=Notify）——某进程（疑 SCHED proc4）在 receive 中被反复交付 Notify、返回即再 receive、通知不收敛，rc 脚本 exec 链疑被饿死。非本轮 r10 修复回归（修前 boot 在 PM panic 处即死、从未走到此；r10 系 caller-saved、status 本应在斯）。下一入口见顶部 B22 bullet。rc marker 未达，frontier 仍 1.30。

### §1.40 本 turn 行量
- 生产代码 1 文件 1 处（`arch/src/x86_64/trap_stub.rs::sync_status_register_to_frame` RBX→R10）+ 1 新回归守卫单测 + doc 注释；
- WORKLOG 顶部 B21→取证/已修双 bullet + B22 前沿 bullet + 本节 §1.40；
- 三件套全绿 + CodeReview PASSED；**含代码修改的 fix commit**。

---

## §1.41　B22 侦察（读码 + c32 日志裁决）：终态 SCHED(proc4) 独占调度饿死 rc 脚本 exec 链

**受控读法接续**：本 turn 从 §1.40（B21 修复，commit 13bdba063）后的 B22 前沿起步。HEAD=13bdba063。**纯侦察、无生产代码改动**（§1.25/§1.28 doc commit 先例，无三件套）。

**c32 日志裁决推翻「notify 风暴」初判**：
- `nk4a: rearm`=0、`nk4a: srcv`=0、`panic-enter`=0 → SCHED 的 CLOCK 臂 `balance_queues`/re-arm **未失败**、receive **无 Err**、无任何 panic；
- 全文件 `r10s=0x...04`（Notify 交付）仅 **115** 次/150s → **低频**，非紧风暴；
- 全文件 pick 目标 **各槽 0..12 均被调度**（PM(0)=313、VFS(1)=437、SCHED(4)=137、VM(8)=765、INIT(11)=64、子(12)=**1**）→ 系统真活着、远超 SingleUser、fork 链已起子（slot12 被 pick 过）。
- **决定性末态**：tail 300 行 pick 目标 **只有 `pick->0x4`**（37 次），无其它槽 → SCHED 终态**独占调度器选取**。

**根因假设（待下轮真机/读码坐实）**：B21 修复后 SCHED 现能正确识别投递的 Notify（run_once：`is_notify` 且 sender≠CLOCK → 静默 return Handled；sender==CLOCK → balance_queues+rearm 后 return Handled，**永不对通知回信**，server.rs:226-253）。SCHED 处理完通知后回到 `ipc.receive()`——若 receive **立即返回**（内核侧有一项 persistent/never-cleared 的 pending notify 状态，或 `clear_ipc_status_reg` 未在 SCHED receive 重入时清除 gp_regs[GP_R10] 的 Notify 位），则 SCHED 永不真正阻塞、恒 runnable，调度器每轮都选中它→**饿死正等 SCHED 推进量子/调度的 INIT(11)/子(12) 的 exec 链**→rc 脚本 marker 不可达。子 slot12 全程仅被 pick 1 次=exec 起步即停。

**下一入口（精确）**：①先确认末态 `pick->0x4` 时 SCHED receive 收到的 notify **源**（CLOCK vs 其它）——若 CLOCK 但 re-arm 未失败而仍不停→查 sys_setalarm 重挂是否真产生下一次到期（否则就是同一通知反复投）；②查内核 receive 重入路径（Phase-1 自发 receive 陷阱序言 `clear_ipc_status_reg`）是否清掉了 gp_regs[GP_R10] 的 Notify 位——若未清，`is_notify` 会粘滞为真→SCHED 假“收到通知”死循环（与 B21 同一状态车道、新形态）；③对齐 C sched `main.c:35-96`：处理完通知后 receive 应真阻塞等下一事件。取证优先读码/内核侧（§1.33 PM 阻断不影响 SCHED/内核）。

### §1.41 本 turn 行量
- 无生产代码（纯侦察）；
- WORKLOG 本节 §1.41（精化顶部 B22 前沿的根因假设与下一入口）；
- 纯侦察 doc commit、无三件套；rc marker 未达，frontier 仍 1.30。

---

## §1.42　B22 侦察纠正：推翻 §1.41「SCHED 风暴/活锁」——c32 tail-dump 实锤=静态全系统死锁

**受控读法接续**：本 turn 从 §1.41（B22 侦察，commit 055fd91b9）后的 B22 下一入口起步。**纯侦察、无生产代码**（§1.25/§1.28/§1.41 doc commit 先例、无三件套）。

**读码排除 §1.41 假设②（R10 粘滞）**：plain-RECEIVE 序言（ipc.rs:2730-2761）对每次 fresh receive **确实** `clear_ipc_status_reg`（清 gp_regs[GP_R10]）+ 清 REPLY_PEND；`pick_allowed_notify`（ipc.rs:1818-1862）扫 `s_notify_pending` 位→命中即 `&= !(1<<bit)` **消费即清位**（除非 CANRECEIVE 过滤则保留）。→ 状态车道无粘滞。

**c32 tail-dump 实锤推翻「活锁/风暴」（决定性）**：
- `picknone`=1、`tail-dump begin`=2，且**两次 tail-dump 逐字节相同**（=冻结、非慢）→ **静态全系统死锁**，非 §1.41 猜的 SCHED 风暴；
- 尾部反复 `pick->0x4` 只是**时钟 tick 短暂唤醒 SCHED 服务 Clock-notify**（SCHED 是唯一被唤者，其余全 runnable=no queued=no），与早期 WORKLOG「449-livelock/picknone 同族」尾态一致。

**末态 13 进程 wait-for 图（flags 解码 RECEIVING=0x8 SIGNALED=0x10 SIG_PENDING=0x20）**：
- 大多数服务（PM0/VFS1/RS2/MEM3/SCHED4/TTY5/DS6/MIB7/VM8/9/10）均 `flags=0x8` RECEIVING = **空闲等信**；VM(8) from=ANY(to=NONE) 纯 receive。
- **INIT(11)/0x10b**：`flags=0x8` RECEIVING、to=VFS(1)、from=PM(0)、runnable=no queued=no。
- **child(12)/0x10c**（`*F` forked）：`flags=0x30` = SIGNALED|SIG_PENDING、to=VFS(1)、from=PM(0)、runnable=no queued=no。

**最锐利线索（待下轮坐实）**：child(12) 只有 SIGNALED|SIG_PENDING、**不含任何阻塞 IPC 位**（无 SENDING/RECEIVING/PAGEFAULT）却 runnable=no queued=no——一个仅被标信号的进程本应 runnable+入队（SIGNALED 非阻塞位）。与 WORKLOG 已登记的 **P1-ipc**（`clear_ipc_refs` syscall.rs L1283 裸清标志绕过 `clear_ipc`=RTS_UNSET 入队半）/ **F10d**同族=置/清标志丢入队。**下一入口（优先级）**：①定位 fork 后给 child 置 SIGNALED|SIG_PENDING 的腿（信号投递 / PM `mprocs_run` 唤醒 INIT 子），查其是否漏 `make_runnable`+enqueue；②若 child 本应可跑，INIT 为何阻塞等 from=PM（PM 空闲 receive ANY）——查 PM 对 INIT 的 WAIT4/回信腿丢唤醒。取证优先内核/读码侧（§1.33 PM 阻断不影响内核）。

### §1.42 本 turn 行量
- 无生产代码（纯侦察）；
- WORKLOG 顶部 B22 bullet 纠正定性 + 本节 §1.42（tail-dump 实锤全系统静态死锁、推翻 §1.41 风暴假设）；
- 纯侦察 doc commit、无三件套；rc marker 未达，frontier 仍 1.30。

---

## §1.43 B22 侦察纠正——推翻「child queued=no=入队违例」定性，坐实真根因=INIT fork 子空指针 SIGSEGV（纯侦察 doc）

本轮沿 §1.42「最锐利线索」深入，**推翻其自身假设并坐实真根因**。

**决定性纠正①：`is_runnable()=load()==0` 是 C 忠实、child queued=no 非违例**。§1.42 猜「SIGNALED 非阻塞位、child 本应 runnable+入队」。核对 vendored C 黄金参照 `minix3/minix/kernel/proc.h:169-170`：
```c
#define rts_f_is_runnable(flg)	((flg) == 0)
#define proc_is_runnable(p)	(rts_f_is_runnable((p)->p_rts_flags))
```
C 里 `is_runnable` 就是 **flags 全 0**——SIGNALED(0x10) 置位即非 runnable，进程被移出就绪队列是**设计如此**（信号不靠运行被信号进程本身投递，而是交其 sig_mgr=PM 处理，PM 调 SYS_GETKSIG 经 `rts_unset(SIGNALED)` 的入队半唤醒子）。Rust `RtsFlags::is_runnable`(proc.rs:1245 `load()==0`) 与之逐字对齐，**无违例**。child(12) `flags=0x30 queued=no` 是 C 正确终态。§1.42 的「入队丢半/P1-ipc 同族」方向作废。

**决定性纠正②：真根因=child(12) fork 后空指针 SIGSEGV**。c32 全日志仅**一条** `nk4a: csig tgt=0xc sig=0xb`（cause_signal 探针，CSIG_N<24 未封顶，全 boot 唯一信号）。`sig=0xb=11=SIGSEGV`（`syscall_signal.rs:66 SIGSEGV=11`、`minix-types/signal.rs:47 SIGNAL_SEGMENT_VIOLATION=11`）。csig 现场上文：
```
nk4a: pick->0xc
nk4a: pre-restore- rip=0x21b3e4 rsp=0x7fffffffa608   ← 子从 fork sendrec 正常返回(见下)
... 若干调度 ...
nk4a: vm-pf recv
nk4a: pf-exit noaddr cr2=0x0                          ← 子触发 cr2=0 空指针缺页，VM 判 noaddr
nk4a: csig tgt=0xc sig=0xb                            ← 内核 cause_signal(SIGSEGV) 给子
```
即 child 走 `pf-exit noaddr cr2=0x0` → VM 拒 noaddr → `cause_signal(SIGSEGV)` → 子被挂 SIGNALED|SIG_PENDING 停 0x30。

**fork 链本身已通（旁证 B18/B19/B20 修复有效）**：addr2line `0x21b3e4`→`minix_sys::fork`（init ELF）；objdump 该地址=`int $0x21` 后第一条 `mov %r10,%rsi`（寄存器移动、不可能 fault）——即**子从 `fork()` sendrec-to-PM 正常返回**（rax=pid/r10=status），`fork()` 成功。SIGSEGV 崩在子返回后的 **INIT 用户代码下游**（子 exec /bin/sh 前的某段 init 代码 deref 了地址 0）。

**为何从未 exec /bin/sh**：exec 探针 `nk4a: exec-store` 覆盖到 nr 0x5/0x7/0x9/0xa/0xb（各 boot 服务 + init），**无 0xc**——子 fork 后即 SIGSEGV，从未走到 exec(/bin/sh) → rc 脚本从未运行 → marker 不可达。这与「无 /bin/sh、/etc/rc exec 痕迹」日志观察闭环。

**PM 侧次生死锁（对 marker 而言已次要）**：PM 空闲在 receive(ANY)、最后活动止于 `pm 000ff`（硬中断 notify，B21 已正确 is_notify 跳过），**从未调 `sys_getksig`**（exit.rs:257 的 reap/sig-check 腿）清理这条 SIGSEGV——故 child 永停 0x30。但即便 PM 清了 child 信号，子已崩、exec 未发生，marker 仍不可达；主因是子的 SIGSEGV，非 PM 未 reap。

**下一入口（精确·优先级）**：捕获 child(12) 的**崩溃 rip**（现 `pfc` 探针过滤器 `matches!(cur_nr.0, 0|1|3|4|5|6|7|9|10|11)` **不含 12**，未记）。做法：把 12 加入 pfc 过滤器（或另设子专用 fault-rip 探针），build+一次真机即得崩溃指令地址 → addr2line 定位 INIT 子代码哪一行 deref null。假设备选：①子从 fork 返回后读到的某出参指针（如 `&mut pid`/env 表/sigaction 数组）被子上下文置 0（延续 B20 子帧继承链的残留字段）；②init 程序 fork-child 分支里 `exec` 前置的空 arg/env 指针。取证走内核侧（§1.33 PM 布局脆弱性、且根因在用户态 init 非 PM）。

### §1.43 本 turn 行量
- 无生产代码（纯侦察 + 读码交叉验证 C `proc.h:169`）；
- WORKLOG 顶部 B22 bullet 再次纠正（作废「入队违例」方向、指向子 SIGSEGV）+ 本节 §1.43；
- 纯侦察 doc commit、免三件套（§1.25/28/41/42 先例）；rc marker 未达，frontier 仍 1.30。

---

## §1.44 B22 崩溃 rip 实锤（真机 c33 取证·纯侦察 doc）

按 §1.43 登记的下一入口，临时将内核 `pfc` 崩溃-rip 探针过滤器 `matches!(cur_nr.0, 0|1|3|4|5|6|7|9|10|11)` 加 `| 12`（内核侧、§1.33 不阻），build+跑一次 c33。**取证探针已 `git checkout` 回滚（tracked os diff 空）**。

**决定性数据（c33）**：
```
nk4a: pfc ep=0xc rip=0x21b3e4 cr2=0x0 err=0x4 tex0x0=unmapped
nk4a: csig tgt=0xc sig=0xb
```
即 child(12) 崩溃现场 = **rip 0x21b3e4、cr2=0x0、err=0x4**（bit2=US ⇒ 用户态读、present=0 ⇒ 页不存在；与 c32 一致，可复现、非随机）。

**rip 0x21b3e4 定位到确切指令（objdump init ELF）**：属 `minix_sys::fork`（`os/libs/minix-sys/src/pm.rs:165 fork_via → perform_syscall(DirectTrapTransport, PM_CALL_FORK)`），子进程从 `int $0x21`（sendrec-to-PM）**返回后的首条**：
```
0x21b380 fork:  sub $0x58,%rsp
0x21b384..b3ba  将 [rsp+0x10..0x48] 56B 消息区清 0
0x21b3c3  movabs $0x200000000,%rax ; 0x21b3cd mov %rax,0x8(%rsp)  ← 消息描述符(栈上 rsp+8)，低 32 位=0
0x21b3d2  lea 0x8(%rsp),%rdx        ; rdx = &描述符
0x21b3dc  xor %eax,%eax; 0x21b3e2 int $0x21   ; ← SENDREC 陷入
0x21b3e4  mov %r10,%rsi             ; ← 崩溃 rip（寄存器移动、本身不访存）
0x21b3e7  pop %rbx; 0x21b3e8 mov %rax,%rdx; ...
0x21b3f9  mov 0xc(%rsp),%eax        ; 读回复 m_type
```

**推理（待下轮坐实）**：`mov %r10,%rsi` 本身不访问内存却带 cr2=0 报 fault，只有两种解释：①**子从 fork sendrec 的 trap-return 臂未正常完成**——内核把 PM 的 fork REPLY 拷回子消息缓冲时，子的 msg-指针/描述符寄存器（应=rsp+8 栈地址）被子上下文置为 **0**，拷贝读地址 0 fault，rip 记为子即将恢复的 0x21b3e4；②子的 ipc_trap ABI 中某出参寄存器（rsi/rdx）未随父正确继承。两者都指向 **子上下文/回复投递的寄存器继承缺陷（延续 B20 `fork_from` 子帧继承链：只 `cpu_context:parent.cpu_context` 整体拷贝是否足够？reply-buffer 指针存在哪？）**。

**关键区分证据（下轮取证入口）**：需 dump child(12) 崩溃时的**完整用户帧寄存器组**（尤其 rdx/rsp/r10）与父同点位对比——若子的 rsp 正常但某 reply 指针=0 则定性①。现有 pfc 只打 rip/cr2/errcode，不够。候选探针位：(a) trap_dispatch.rs 子 fault 入口 dump `frame.regs`全集（仅 ep=0xc）；(b) 内核投递 PM→子 reply 的 copy 站点（ipc.rs reply deliver）dump 子的 dst 指针。**预期根因家族**：`fork_from` 拷贝的 `cpu_context` 未包含/未正确带 reply-buffer 指针（该指针可能存于 `p_delivermsg_dist`/单独字段而非 `cpu_context`）→子 resume receive 时无目标缓冲→地址0。

### §1.44 本 turn 行量
- 无生产代码（真机取证 c33：临时内核探针已回滚、tracked diff 空）；
- WORKLOG 顶部 B22 bullet 崩溃 rip 更新 + 本节 §1.44；
- 纯侦察 doc commit、免三件套（§1.25/28/41/42/43 先例）；rc marker 未达，frontier 仍 1.30。

---

## §1.45 B22 根因彻底闭环（真机 c34 帧 dump + 读码交叉验证 C·纯侦察 doc）

沿 §1.44 登记的下一入口，临时在 `trap_dispatch.rs` pfc 块后加 child(12) 完整帧 dump 探针（内核侧、§1.33 不阻），build+跑 c34。**取证探针已 `git checkout` 回滚（tracked diff 空）**。

**c34 铁证一（调度器切子时页表根=0）**：
```
nk4a: pick->0xc
nk4a: sa0-0xc root=0x0 cur=0x3355000   ← 子 p_seg.phys_root = 0（NULL页表！）
nk4a: sa1-after cr3=0x3355000          ← 内核见 root=0 未切换、沿用陈旧 cr3
nk4a: pre-restore- rip=0x21b3e4 rsp=0x7fffffffa608
```
**c34 铁证二（子崩溃帧）**：`c33fr rip=0x21b3e4 rax=0x0 rdx=0x7fffffffa618 rsi=0x10 rdi=0x7fffffffa9a0 rsp=0x7fffffffa608 rbp=0x7fffffffa7f4 r10=0x22a7e5 rbx=0x7fffffffa618`。与 pfc `tex=unmapped` 合看：**子根本没有任何自己的页表（phys_root=0）**，以空/陈旧 cr3 跑首个用户访存即 fault（rax=0、cr2=0）。非竞态（c32/c33/c34 确定性同崩）。

**读码交叉验证坐实根因链（C vs minix-rs）**：
1. C `do_fork.c:63 *rpc=*rpp` 拷全 proc（含 p_seg），但 line 126 `rpc->p_seg.p_cr3=0` 又显式清零，**同时 line 115-116 `if(flags & PFF_VMINHIBIT) RTS_SET(rpc, RTS_VMINHIBIT)` 把子挡在调度外**，直到 VM 给子绑新页表（SYS_VIRGIN/pt_bind）后发 VmInhibitClear 清 VMINHIBIT。**两件事配套**：清零 cr3 安全，因为有 VMINHIBIT 持住。
2. minix-rs `fork_from`(proc.rs:1617) 置 `p_seg: default()`→phys_root=0（与 C 一致）；`complete_fork_setup`(proc.rs:1750) **确实**按 `flags & fork_flags::VMINHIBIT(0x01)` 置 VMINHIBIT。
3. **但 VM 腿 `os/servers/vm/src/fork.rs:344 gateway.sys_fork(parent.endpoint(), child.slot())` 只传 2 参、不传任何 PFF flags** → dispatch_fork 收到的 `fork_req.flags`=0 → `complete_fork_setup` **从不置 VMINHIBIT** → 子 fork 完即叮 runnable。子能 `pick->0xc`（c34 实锤）正因无 VMINHIBIT。
4. 且子 phys_root 始终=0 说明 VM fork 腿**从未为子发 SetAddrSpace VMCTL**（`vmctl_vminhibit_clear`/设 phys_root 的 SetAddrSpace 腿在 syscall.rs:2831/lib.rs:1722 存在，但未对 fork 子调用）。

**根因定性**：B22 = **VM/内核 fork 腿未落实 C 的「VMINHIBIT 持子 + 绑页表后清」配套纪律**：VM sys_fork 不传 PFF_VMINHIBIT 且不为子绑页表（phys_root 永为 0）→ INIT fork 子一被调度即以空页表 SIGSEGV→从未 exec /bin/sh→rc marker 不可达。同 B16/B17 VM-fork 家族延续。

**修复方向（下轮实施·需真机验证 rc marker、不投机）**：使 minix-rs 对齐 C——① VM `fork.rs` 的 sys_fork 路径传 PFF_VMINHIBIT（使 `complete_fork_setup` 给子置 VMINHIBIT），且为子发 SetAddrSpace 绑真页表（填 phys_root），再 VmInhibitClear；或②若本设计采用“A1 采纳语义、sys_fork 内就完成地址空间登记”（fork.rs:325-327 注释），则修正 `fork_from` 不置 phys_root=0而继承父 phys_root（共享页表、COW 由 VM 管）与 C `*rpc=*rpp` 语义一致。两方案均需：先定位 VM 对子到底有无 SetAddrSpace 腿（读 vm fork.rs:300-320 `write_page_table_mappings` 是否真把子页表回写内核），再选最小忠实方案 + 三件套（docker 只增不减 + fmt 零新增 + 两次真机签名且子真 exec sh 达 rc marker）+ CodeReview。

### §1.45 本 turn 行量
- 无生产代码（真机取证 c34：临时内核帧-dump 探针已回滚、tracked diff 空）；
- WORKLOG 顶部 B22 bullet 根因闭环 + 本节 §1.45；
- 纯侦察 doc commit、免三件套（§1.25/28/41/42/43/44 先例）；rc marker 未达，frontier 仍 1.30。

---

## §1.46 B22 修复落地（含代码·VM 侧·PFF_VMINHIBIT + SetAddrSpace 双腿）

承接 §1.45 登记的修复方向，本轮忠实移植 C `minix3/minix/servers/vm/fork.c:89-95` 的两条配套腿，消除 INIT fork 子空页表 SIGSEGV。

### 根因回顾（c34 铁证）
子(12) 被 pick 时 `sa0-0xc root=0x0`（p_seg.phys_root=0、无自有页表），内核见 root=0 未切 cr3、沿用陈旧，子以空/陈旧 cr3 跑首访存即 SIGSEGV（rip=0x21b3e4、rax=0、cr2=0、err=0x4、tex=unmapped，c32/c33/c34 确定性同崩）。根因=VM `fork.rs` 只传 2 参（flags=0→内核不置 VMINHIBIT）且从不为子发 SetAddrSpace。

### C 两腿 vs minix-rs 缺失
- **腿1（持子）**：C `sys_fork(vmp->vm_endpoint, childproc, &vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr)`——PFF_VMINHIBIT 经 wire `m_lsys_krn_sys_fork.flags` 进内核 `dispatch_fork`→`complete_fork_setup`（proc.rs:1750）按 `flags & fork_flags::VMINHIBIT(0x01)` 给子 RTS_SET(VMINHIBIT)。minix-rs 原 gateway 硬编 0。
- **腿2（绑页表+清 VMINHIBIT）**：C `pt_bind(&vmc->vm_pt, vmc)` 尾部=`sys_vmctl_set_addrspace(endpoint, pt_dir_phys, pdes)`，内核 `vmctl_set_addr_space`（syscall.rs:2925 存 phys_root；step5 line 2966 `rts_unset(VMINHIBIT)` 带入队半）。minix-rs 原无此腿。

### 本修复
1. `kernel_gateway.rs`：trait `KernelGateway::sys_fork` +`flags: u32`；`TrapKernelGateway` 转发 flags；`MockGateway.last_fork`→`(Endpoint,UserSlot,u32)`。
2. `vm_server.rs`：两处 test-only mock `sys_fork` +flags 参（pass-through / EIO 占位）。
3. `fork.rs`：`const PFF_VMINHIBIT: u32 = 0x01`；do_fork 传该 flag；`sys_fork` 后、`handle_memory_once` 前插 `sys_vmctl_set_addrspace(child_endpoint, <PageTable as Paging>::root_paddr(child.page_table_mut()).0, 0)`；修正 fork.rs:324-327 错误注释。新单测 `test_do_fork_holds_child_then_bounds_addrspace`。

### 真机验证（三件套）
- **docker**：kernel 816 / arch 243 / vm **529**（+1 新测）·0 fail，只增不减✓。
- **fmt**：fork 30=30 / kernel_gateway 21=21 / vm_server 119=119，零新增漂移✓。
- **两次真机 c35/c36 签名一致**：`csig`(SIGSEGV)=0、panic-enter=0、vector13=0、行数 26296=26296（较 §1.45 前 ~25285 前进 ~1000 行）✓。新出现 `vm-pf fa=0x21b3e4`（VM 正常服务子 fork 返回页故障而非致命）。
- CodeReview PASSED（MUST-FIX=0，采纳 SHOULD-FIX 修残留 `flags (0)` 注释）。

### §1.46 本 turn 行量
- 生产代码：fork.rs / kernel_gateway.rs / vm_server.rs(test mock)；
- 含码 commit、走三件套（docker 529/243/816 ✓ + fmt 零新 ✓ + 两次真机签名一致 ✓）+ CodeReview✓；
- **B22 已修→新前沿 B23**（SCHED slot-4 receive 自旋、child 未推进至 exec sh）；rc marker 仍未打印，frontier 仍 1.30。

---

## §1.48 B23+B24+B25 三根因联治（含代码·VM/内核/PM 三层）

**日期**：2026-09-25，serial_c43/c43b

### 现象

B22 修复后 c35/c36 真机 boot 停在“全系统 RECEIVING 死锁”（§1.47 tail-dump 实锤）：所有服务 flags=0x8(RECEIVING)、VM 空闲等 ANY、INIT to=VFS、child to=VFS，无任何进程持有待发消息。随后经 c37-c41 探针链发现三个独立缺陷：

1. VM `do_memreq_drain` 回包 WHO 用了 `req.target`（故障页空间）而不是 `req.requestor`（挂起者）；
2. `mini_receive` 入口缺少 C `RTS_ISSET(caller, RTS_SENDING)` 门，SENDREC receive 半不在此停车而是落到 drain 腿“代停车” workaround（不存 pdmv）；
3. PM `tell_parent` 缺少 C `if (addr)` 守卫，INIT 的 `waitpid(pid, NULL, 0)` 导致对地址 0x0 发 144B VIRCOPY→崩溃。

### 根因与 C 锚点

| Bug | 一句话根因 | C 锚点 |
|-----|----------|----------|
| B23 | `sys_vmctl_memreq_reply` WHO=req.target，跨进程挂起时 target≠requestor，内核 EINVAL | `minix3/minix/servers/vm/pagefaults.c:206` |
| B24 | receive 无 SENDING 门，SENDREC receive 半走错路径（drain代停车不存 pdmv → DeliverMsg 挂起 start=0x0 → VM 拒 → 崩） | `minix3/minix/kernel/proc.c:999` |
| B25 | tell_parent 无条件 copy_to_user(addr=0) → INIT 空间 VIRCOPY 失败 → SIGSEGV 链 | `minix3/minix/servers/pm/forkexit.c:692` |

### 修复

1. **vm_server.rs**（B23）：`req.target`→`req.requestor`，回归测试改 requestor≠target 并断言 REPLY WHO 跟随 requestor。
2. **ipc.rs**（B24）：
   - receive 入口加 SENDING 门（+14 行）：若 caller 带 SENDING，跳过所有扫描直接 `getfrom=src_e + RECEIVING + return Blocked`；
   - 删除 drain 腿旧 `if sender_reply_pend { ... }` 代停车块（-34 行）——该逻辑已由上面的 receive 入口门接管；
   - 探针：INIT send/receive 腿现场打印（c39 取证用，task1-close 裁决删除）。
3. **exit.rs**（B25）：包裹 `copy_to_user` 在 `if addr.0 != 0 { ... }` 内，addr==0 时直接跳过拷贝、正常回复 pid。新测试 `test_tell_parent_null_rusage_addr_skips_datacopy`。
4. **探针代码**（syscall.rs/proc_table.rs/trap_dispatch.rs）：诊断用，`#[cfg(not(feature = "mock"))]` 保护，不影响测试。c40/c41 取证后确认根因，将来 task1-close 统一删除。

### 验证

- **docker**：kernel 816 / arch 243 / vm 529 / pm-lib 418·**0 fail**，只增不减✓（基线 813/242/526/417，+1 PM B25 回归测试）。
- **rustfmt**：6 文件全部 CUR ≤ HEAD（ipc -1, proc_table =, syscall =, trap_dispatch =, exit -1, vm_server =），零新增漂移✓。
- **两次真机 c43/c43b**：0 panic / 2 csig(slot 0xd) / 3 pf-exit / PM↔VFS spin，行为签名一致✓。与 c42（修复前最后一轮）一致（确认可重现）。INIT 崩溃链（memreq start=0x0 → SIGSEGV → panic）完全消失，证明 B25 有效。
- **CodeReview**：PASSED，MUST-FIX=0。

### 新停点 B26

slot 0xd（INIT 第二个 fork 子，gen=1 slot=13）正常缺页服务后二次缺页 `pf-exit noaddr cr2=0x1`（近零指针）→ SIGSEGV×2 → 子被杀 → PM↔VFS pick 交替自旋（128万行/150s 探针 spam）。rc marker 仍未达。下一入口：分析 slot 0xd cr2=0x1 根因（子 deref null / fork 参数差异）以及 PM↔VFS spin 的成因。

---


## §1.49 B26 误判作废 + B27 新阻塞（纯侦察·无生产码）

**模式**：纯侦察 doc，工作树全程干净（探针两次编辑均 `git checkout` 回滚），免三件套（§1.25/28/41-46 先例）。

**取证产物**（`/tmp/nk4a/`，gitignore，换树会丢——关键结论已入上文状态板）：
- `c43.txt` / `serial_c43.log`：committed HEAD 的「good」签名（推进至 Runcom、0xd SIGSEGV、PM↔VFS 125万行 spin）。
- `serial_c44.log` / `c44b.log`：扩 `pfc` 过滤器（含页表 walk 读）→ 早期内核递归 panic（页表 walk/DirectMap 读在非预期 fault 上下文递归崩溃，**探针纪律教训**）。
- `serial_c45.log`：改独立安全探针（仅打印寄存器）→ 仍早期 panic（b26p 从未触发，证明与探针无关）。
- `serial_c46.log` / `c46b.log` / `c46c.log`：**回滚至干净 HEAD `ef07f760f` 全量重建 + 同镜像三连**——稳定复现早期内核 panic（panic 计数 2/3/4、行 208/202/211 微抖，故障确定）。

**两条硬结论**：
1. **B26「RBX=0 根因」作废**：`rbxw` 探针读 int33 陷阱帧入口 RBX（syscall 序言前），全系统每进程普遍 =0（含正常工作的 slot 0xc），与 0xd 崩溃无因果。真崩溃 = 0xd（INIT 第二 fork 子，`runetcrc(true)` chroot 分支）在 `execve::exec_command` 路径 deref `cr2=0x1`（null+1）→ VM `pf-exit noaddr` → SIGSEGV×2 → 子死 → PM(0)↔VFS(1) 乒乓活锁。**确切 faulting rip 未捕获**（`pfc` 过滤器不含 slot 12/13 与 vaddr=0x1）。
2. **B27 取代 B26 成头号前沿**：committed HEAD 全量重建后**不再复现 c43 无 panic 签名**，退化为早期内核数据读缺页 `rip≈0x5c3f309 cr2=0xffff807f85d7b6a8 walk=NP`（cs=0x8，Direct Map 基址下≈2GB 处未映射指针）。真机三件套验证**对构建产物敏感**——§1.48「c43/c43b 签名一致」实为增量构建陈旧 artifact 侥幸，未固化稳定无 panic 基线。

**接手者下一步（严格按序）**：
1. **B27 优先**：定位 `cr2=0xffff807f85d7b6a8`（≈phys 0x85d7b6a8）指向的内核数据结构（页表 walk 目标 / boot info memmap / slab 元数据），判明是真 bug 还是 `-smp 4` 早期竞态（干净 `CARGO_TARGET_DIR` 重建 + 单核/多核对照）；恢复一个**可复现无 panic** 的真机基线。
2. **B26 次之**：在稳定基线上，用一个**只打印寄存器、绝不做内存读**的内核缺页探针捕获 slot 12/13 且 vaddr≤0x1000 的 faulting rip，坐实 0xd 在 exec_command 的 null+1 deref 具体指令；并分析 PM↔VFS 乒乓活锁（子死后 INIT waitpid 不可达，PM/VFS 互等）。
3. rc marker（`minix-rs rc: minimal boot script marker`）仍未达，三架构均未启动打印。frontier 名义 1.30，实际可复现停点已早于 B22 之前的 Runcom。

---

## §1.50 B27 部分修：BKL owner 跟踪消除 PROC_TABLE 别名 + 残留页表撕裂前沿（含代码·内核侧）

**模式**：含生产码修复的逻辑单元，走三件套 + CodeReview。

### 根因坐实（一类真 bug，非探针假象、非架构裁决级）

B27 早期内核 panic 的一类根因 = **中断/陷阱入口的 BKL 获取语义错误**。原代码在 clock/IRQ/IPI/IPC 陷阱入口用 `crate::smp::bkl_try_lock()`（对单个 `static BKL_LOCKED: AtomicBool` 做 CAS）：
- CAS 成功 → 本 CPU 新获取（欠 release）；
- CAS 失败 → 代码当作「继承被中断上下文已持有的锁」（不欠 release）。

但 CAS 失败**无法区分**两种截然不同的状态：(a) **本 CPU 的内核帧**正持锁（打断发生在内核态 → 应继承，重获取会在非重入 spinlock 上自死锁）；(b) **另一个 CPU** 正持锁（打断发生在 user/idle 态，本 CPU 根本不持锁 → 必须自旋获取后才能碰共享表）。把 (b) 误当 (a) 处理，就让**两个 CPU 同时对全局 `PROC_TABLE` 取 `&mut`** → 撕裂写坏某进程的表内指针 → 内核态数据读缺页 panic（原签名 `cr2=0xffff807f85d7b6a8` = 读一个被写坏的伪指针）。单核（`-smp 1`）无对手，故 c51s1 零 panic —— 与「真机签名对 `-smp 4` 敏感」一致。

**C 黄金参照**：`minix3/minix/kernel/arch/i386/arch_clock.c:226-263` clock handler 明确分支——`p == proc_addr(KERNEL)` 时继承（不取锁、`must_bkl_unlock=1`），否则 `BKL_LOCK()`；`mpx.S` 用 `TEST_INT_IN_KERNEL`（CPL）判源。C 的 BKL 是非重入单 spinlock（`smp.c:27` `SPINLOCK_DEFINE(big_kernel_lock)`）。本 Rust 端的**异常臂**（trap_dispatch.rs:906-910）早已按 `is_user → bkl_lock_section() vs assume_held()` 正确分源，唯独中断/IPC 入口用 try_lock 歧义原语。

### 修复（owner-CPU 跟踪）

1. `smp.rs`：新增 `static BKL_OWNER: AtomicI32 = -1`（`#[cfg(not(mock))]`）；`bkl_lock`/`bkl_try_lock` 获取成功后 `store(current_cpu_id().raw())`；`bkl_unlock` 释放前 `store(-1)`。审计确认 `BKL_LOCKED` 的全部 store(true) 都在这三个获取函数内（smp.rs:1603 为 `#[cfg(test)]` test-only），无绕道置位导致 owner 漏设。
2. `smp.rs` 新增 `pub fn bkl_lock_or_inherit() -> bool`：`BKL_OWNER==self` → 返 false（继承本 CPU 内核帧的锁，不欠 release）；否则自旋 CAS 获取直到本 CPU 拥有 → 返 true（欠 release）。**即使 peer 持有也自旋等待，绝不继承 peer 的锁**。mock 下回退 `bkl_try_lock()`（单线程宿主无 owner 语义需求）。
3. 把所有中断/陷阱入口的共享表变更点从 `bkl_try_lock()`/裸 `assume_held()` 换成 `bkl_lock_or_inherit()` + 出口/早退点按 `acquired` 配对 release：
   - `trap_dispatch.rs`：SCHED_IPI 臂、LAPIC clock 0xF1 臂的 quantum 段、PIT TIMER 臂的 quantum 段、`save_irq_frame_to_context`（推翻旧「user-origin IRQ 运行在被中断上下文锁下」错误前提，改为自锁自放）、int33 `x86_ipc_dispatch_body` 入口（三处早退 EBADCALL/EFAULT/正常 dispatch 前均补 release + `debug_assert!(ipc_bkl)` 守护「int33 恒从 DPL=3 到达、必新获取」前提）、riscv64/aarch64 两处同型 quantum 站点；
   - `irq_manager.rs`：`dispatch_hardware_irq`/`claim_hardware_irq`/`dispatch_claimed_hardware_irq` ×3。
4. int33 IPC 入口获取锁后，在调用 `dispatch_ipc_entry`（其内部重新获取）之前先 release（BKL 非重入，跨该调用持有会自死锁），两释放点之间不碰表故互斥不破。

### 真机证据（决定性）

- **原 B27 别名签名消失**：干净 HEAD 全量重建 + 本修复后跑 smp4，崩溃不再是 `IpcEngine::send` 读 `cr2=0xffff807f85d7b6a8`（表指针被撕裂）——该签名从复现中**根除**。
- **崩溃收敛为确定性**：c51/c52/c53（前 session，含未完整修复态）签名各异、非确定性；本修复后 **c54/c55 两次同 172 行、同一根腐蚀点**（下游最终 panic 表现 c54=pagefault-for-VM、c55=直接 GPF 略抖）。
- **残留根腐蚀点定位**：崩溃前串口打 `nk4a: birth enter` → `nk4a: kdst copy pa=0x0000800005d90a30`（正常应 `0x0000000005d90a30`，此处 **bit47=0x800000000000 被 OR 进物理地址字段**）→ `birth s3 runtime ok` → 崩。`kdst copy` 探针（vm.rs:447 `nk4a_kdst_probe("copy", dst_phys.0, ...)`）打印的是 `cross_space_copy`→`resolve_physical` 页表 walk 解出的目标物理地址——**带 bit47 脏位 = 页表项地址字段被撕裂读**。即进程 birth 期间 **VM 并发改页表 vs 内核 `cross_space_copy` walk 同一页表**的竞态（bit47 落在 12–51 地址位内、非标准 PTE flag，故为 corrupted mapping）。这**不是纯 BKL 问题**：候选 = C 的 VMINHIBIT「持进程页表禁 VM 改动」协议在 birth/copy 路径的缺口（与 B22 的 fork VMINHIBIT 家族相邻，但此处是 copy 期的 map/unmap 竞态）。

### 三件套 + CodeReview

- docker：`cargo test -p minix-kernel --features mock` = **819 passed / 0 failed**（基线 816，只增不减）。
- fmt：`cargo +nightly fmt --check` 逐文件漂移数 HEAD==工作树完全相同（smp 24=24 / trap_dispatch 35=35 / irq_manager 20=20，全局 1196=1196）= **零新增漂移**。
- 非 mock 镜像重建：`cargo run -p xtask -- image --arch x86_64 --release` BUILD_EXIT=0；两次真机 c54/c55 确定性签名一致（见上）。单核无回归（前 session c51s1 578 行健康 tick）。
- **CodeReview：PASSED，无 MUST-FIX**。采纳其 CONSIDER（IPC 入口 `debug_assert!(ipc_bkl)`）。**SHOULD-FIX 未在本轮关闭、记为下一硬门**：`bkl_unlock`（先 owner=-1 后 locked=false）/`bkl_lock`（先 locked=true 后 owner=me）的两步写在 **IF=1 窗口**（idle 唤醒后的调度循环，lib.rs `idle()`→`idle_halt()` sti;hlt 之后）若恰在两 store 之间被同 CPU 外部中断打断，handler 的 `bkl_lock_or_inherit` 读到 owner≠self 且 locked=true → 自旋等一个被冻结、永不置 owner 的持锁者 → 同 CPU 自死锁。**boot 全程经 interrupt gate（IF=0）进入 handler，本轮修复场景不受影响**（故不阻塞合入），但**须先于任何 idle-wake / 多核压力稳定性验证关闭**。根治（CodeReview 方案 1，推荐）=合并为单个 `AtomicIsize`（-1=空闲、≥0=持有者 CPU id），acquire=`compare_exchange(-1,me)`、release=`store(-1)`、inherit 判 `load()==me`，消除两步不一致窗口，语义等价 C `spin_lock_irqsave`。因预算与本 session 已在确定性真机证据上收敛，不为塞入未经验证的大重构，留作下一单元。

### 新前沿（严格按序）

1. **B27 残留（下一步）**：`cross_space_copy`/`resolve_physical` 页表 walk 与 VM 改页表的 birth 期竞态（bit47 撕裂）。方向：查 C 如何在 safecopy 期用 VMINHIBIT/其他机制阻止 VM 改动被拷贝进程的页表；对照本 Rust 端 birth→copy 路径缺哪道持制。用只打印寄存器/PA 的安全探针（严禁在缺页 handler 内做内存读，§1.49 教训）在 walk 前后各采一次同址 PTE 坐实是哪一级页表项被并发改。目标：恢复可复现**无 panic** 真机基线。
2. **idle-wake 前的 BKL 单原子硬门**（上述 SHOULD-FIX）。
3. B27 稳后回 B26（slot 0xd exec_command `cr2=0x1` null+1 deref + PM↔VFS 乒乓）。rc marker（`minix-rs rc: minimal boot script marker`）仍未达。

**取证产物**（`/tmp/nk4a/`，gitignore）：`serial_c54.log`/`serial_c55.log`（确定性 172 行签名 + bit47 kdst copy 证据）。本工作树**无新增探针**（3 文件均为生产锁修复；既有 nk4a 探针在 HEAD，task1-close 统一裁决删除）。

---

## §1.51 B27 copy 层根因修复：`virt_to_phys(内核 image 高半区 VA)` 确定性伪 PA → 18 站点改 `AddressRef::Process`（含代码·内核侧）

### 根因（证伪 §1.50「SMP 竞态 / VMINHIBIT 缺口」假设）

§1.50 把「每次必现、卡死 boot 于 172 行」的 `kdst copy pa=0x0000800005d90a30`（bit47 伪物理地址）猜成「VM 并发改页表 vs 内核 walk 的竞态」。**错**。c56/c57 取证探针直接坐实这是**确定性 code bug**：

- `os/arch/src/arch/direct_map.rs::DirectMapArch::virt_to_phys` 只识别两个 Direct Map 窗口：kernel DM `KERNEL_DIRECT_MAP_BASE=0xffff_8080_0000_0000`、VM DM `VM_DIRECT_MAP_BASE=0x8000_0000`。
- 内核 **image 高半区 VA**（内核栈、`.bss`、`IMAGE_HEAP` 静态堆——`os/kernel-image/src/main.rs:193` `static IMAGE_HEAP: [u8; …]` 落 `.bss`，故所有 `alloc::vec` 指针也在 `0xffff_8000_0000_0000+`）位于 **两个窗口之外**，走 else 分支 `virt - 0x8000_0000` → 伪 PA。
- c57 铁证：`virt_to_phys(virt=0xffff8000003ffb30) -> phys=0xffff7fff803ffb30`（一个内核栈 VA 被减成非规范野值）。
- c56 铁证：`kdst dst=phys sep=0x8 sva=0x23be48`——corrupt 值从 caller **直接以 `AddressRef::Physical` 传入** `resolve_physical`（原样返回 `Ok(*paddr)`），**不经页表 walk**，故与「并发改页表」无因果。

### 修复范式

沿用 `os/kernel/src/syscall.rs::dispatch_diagctl` 早先承认同类问题的 boot-span 恒等式思路（L3072-3077 注释明确写「DirectMap `virt_to_phys` 转换分支不适用于 higher-half kernel-image VA——那里会产垃圾地址」），但 copy 站点统一改走**更通用的 `AddressRef::Process`**：内核栈/BSS/堆 VA 用 `AddressRef::Process { endpoint, offset: VirBytes(内核VA) }` 替换 `AddressRef::Physical(virt_to_phys(内核VA))`。因每个进程的 CR3 都映射内核 higher-half（SYSCALL 不切页表），`resolve_physical` 对 Process 走真页表 walk 即得正确 PA。

### 18 个 boot-path copy 站点（本 unit + 上一 session 的 16 + 本轮补 2）

| 文件 | 站点 | 缓冲区性质 |
|---|---|---|
| `syscall_process.rs` | ×2 | exec `name_buf`（栈局部）、IPC filter pool |
| `syscall_signal.rs` | ×3 | `smsg`/`frame`/`sctx`（栈局部） |
| `syscall_copy.rs` | ×4 | soft-fault marker / vsafecopy vec / vvec / vumap pvec |
| `misc.rs` | ×7 | getinfo `cpu_context` / trace ×4 / sprof ×2 |
| `syscall_device.rs` | ×3 | vdevio output+input / sdevio（CodeReview 指出 `IoBatchBuf` 栈局部同类） |
| `kmess.rs` | ×1（本轮）| `snap = alloc::vec![0u8; …]` → `IMAGE_HEAP`(`.bss`) → image 高半区（CodeReview 指出） |
| `stacktrace.rs` | ×1（本轮）| user 回栈 `scratch_ptr`（`proc_stacktrace` 帧栈局部） |

### 真机决定性（c60–c65）

- **c60/c61**（kmess+stacktrace 修复前后各一次）：`kdst copy` 全为干净低位 PA（`pa=0x00000000003ffc60`），原 `0x0000800005d90a30` 确定性 corrupt 归零；**全部 18 服务名出现**（kernel/clock/system/idle/memory/sched/vm/pm/rs/ds/tty/mib/init/pfs/mfs/vfs/asyncm）。
- **c61 → c62/c63**（stacktrace.rs:249 修复）：c61 有 `rip=0xffff7fff…` 伪回栈野值（伪 dst 经 `cross_space_copy` 写坏放大崩溃级联），c62/c63 该野值**归零**——坐实 stacktrace.rs:249 是其来源，修复有效。
- **c64/c65**（死引用清理后重建，最终树）：签名一致=17 服务 + `pagefault for VM rip=0 cr2=0 err=0x15`（新前沿）。
- **c64 残留（新前沿）**：六次运行中**仅 c64 一次（1/6）**出现 `kdst copy pa=0x0000800005d90a30`（birth s5→main，len=0x11）。低频、**不阻塞 boot**（仍达 17 服务 + 用户 exec）。结构上 bit47 落在 `ADDR_MASK=0x000F_FFFF_FFFF_F000` 内被当地址，**非 `virt_to_phys` 算术输出**（后者产 `0xffff7fff…` 形态）——指向 birth 页表 walk 偶读到被 free/复用的**中间表页**（这才是 §1.50「页表生命周期」假设的真实、低频、异机制版本）。

### 三件套 + CodeReview

- mock：kernel 816 lib + 3 integration = **819** 通过，0 fail（基线只增不减）。
- fmt：全 7 编辑文件**零新增漂移**（nightly rustfmt，方法=同文件 HEAD hunk 数 == WORKTREE hunk 数）；`syscall_copy.rs` 反而 44→0（编辑顺带整理进 rustfmt 风格），`kmess.rs`/`stacktrace.rs` 各 3/8=3/8。稳定版 rustfmt 连未改文件都大量 flag（项目基线用 nightly）。
- 真机：c64 + c65 两次签名一致。
- CodeReview：**PASSED 无 MUST/SHOULD-FIX**；确认 stacktrace panic 路径用 target CR3 walk 严格更安全（最坏返回 None 打占位符，绝不比旧 `Physical(伪PA)` 更易递归 panic）。CONSIDER=清理 `syscall_device.rs` L609/L1015 + `syscall_copy.rs` L38 移除 `virt_to_phys` 后的死 import（**本 unit 已采纳清理**）。

### 本 unit 未做（诚实登记，转下轮）

1. **低频 birth 页表-walk bit47（c64）**：需专探针——在 `resolve_physical` walk 前后对同一 leaf 槽 `read_pte_raw` 双采 + 打印每级中间指针原值；两次不同=值撕裂，两次相同但带 bit47=中间表页被复用。据此再裁决修向（补 birth 期 VMINHIBIT vs 中间表页引用计数）。
2. **`stacktrace.rs:150 kernel_direct_read_word`**：`virt_to_phys→kernel_phys_to_virt` 往返对 image 高半区内核栈产野地址（c60 `recursive panic` 源）。正确修法是「直接读已映射的内核栈 VA」而非往返 DM，属 panic 回栈诊断码的重设计，mock 测 `#[ignore]` 无法覆盖，本 unit 不盲改。line 332 是 `#[ignore]` 真机测且喂 DM 区地址（本就正确）。
3. **`pagefault for VM rip=0 cr2=0 err=0x15`**：本 unit 侦察定位=**VM server 自身（ring 3）rip=0 取指 fault**，走 `os/kernel/src/trap_dispatch.rs:1039-1057` 的 `ExceptionOutcome::VmPageFault` 臂 → `panic!("pagefault in VM")`（**忠实对位 C `exception.c:101-118`**：VM 无法服务自己页表的缺页 → 必 panic，非未处理的用户 fault）。err=0x15=present+user+IFETCH、cr2=0、发生在 cpu 3。下一步 = 查 VM server 控制流何故跳到 null（被损坏的函数指针/返回地址），与 §1.49「exec_command null+1 deref」家族相关。属新独立前沿，非 copy 层。
4. **BKL 两步写单原子硬门**：§1.50 SHOULD-FIX（`BKL_OWNER`+`BKL_LOCKED` 合并为单 `AtomicIsize`），IF=1 idle-wake 前必关。

**取证产物**（`os/target/nk4a-runs/`，gitignore）：`serial_c60.log`–`serial_c65.log`。工作树探针状态：仅改生产 copy 站点（+ 少量既有 `nk4a_kdst_probe` 在 HEAD，task1-close 统一裁决删除）。

---

## §1.52 BKL 单原子化硬门落地（含代码·内核侧）+ §1.49 早期 boot panic 定性实锤 = SMP 竞态（控制实验）

### 本 unit 做了什么

关闭 §1.50 登记、§1.51 顺延的 SHOULD-FIX「BKL 两步写自死锁硬门」，并用一次控制实验把 §1.49 悬而未决的「早期 boot panic 是真 bug 还是 `-smp 4` 竞态」钉死为后者。

### 一、BKL 两步写自死锁硬门修复（`os/kernel/src/smp.rs`）

**根因（side-conversation 诊断一 + §1.50 独立登记，两处一致）**：`BKL_LOCKED: AtomicBool` 与 `BKL_OWNER: AtomicI32` 是**两个分离原子**。`bkl_lock` 先 CAS `locked` false→true、**后** `store owner=me`；`bkl_unlock` 先 `store owner=-1`、**后** `store locked=false`。两步写之间有「locked 与 owner 不一致」窗口。在 `IF=1`（idle 唤醒后的调度循环）窗口，若本 CPU 恰在 `bkl_lock` 的两条 store 之间被中断，中断处理程序进 `bkl_lock_or_inherit` 读到 `owner != me`（仍是 -1 或旧 peer）→ 判为**非继承**→ 去 CAS `BKL_LOCKED`→ 而 locked 已被被打断的本 CPU 内核帧置 true → CAS 恒失败 → **同 CPU 自旋等一个自己持有的锁 = 单核自死锁**。boot 全程经 `IF=0` 的 interrupt gate 进 handler 故不触发（这也是它不阻塞 §1.50/§1.51 合入的原因），但它是任何 idle-wake / SMP 压力稳定性验证之前必须关的硬门。

**修法（CodeReview 方案 1，已核等价）**：合并为**单个 `static BKL: AtomicIsize`**（`-1` = 空闲 / `>= 0` = 持有者 CPU id）。acquire = 单条 `compare_exchange(BKL_FREE, me, Acquire, Relaxed)`；release = 单条 `store(BKL_FREE, Release)`；inherit 判定 = `load(Acquire) == me`。「locked 位」与「owner」**物理上同一字**，不一致窗口结构性消失，语义等价 C `spin_lock_irqsave`。改动全局部于 `smp.rs` 六函数（`bkl_try_lock`/`bkl_lock`/`bkl_lock_or_inherit`/`bkl_unlock`/`bkl_is_locked`/`bkl_lock_reset_for_test`）+ `BklGuard`/测试 setup/teardown；`BKL_LOCKED`/`BKL_OWNER` 经 grep 确认仅 `smp.rs` 内部引用，**调用方零改动**。mock（单 CPU，不调 `current_cpu_id()`）用哨兵 `BKL_MOCK_HOLDER=-2` 标记持锁（`!= BKL_FREE` 即 locked）。

**CodeReview 采纳**：CONSIDER#2——`bkl_try_lock` doc 原写「peer 持有 = INHERITS」是旧 bool-only 模型混淆态；补 NOTE 澄清「合并后 false 仅表示『未获取』（可能本 CPU 帧或 peer），中断/陷阱入口须用 `bkl_lock_or_inherit`」。CONSIDER#1（`bkl_unlock` debug_assert 顺带验所有权）示例用了 `debug_assert!` 内嵌 `#[cfg]` 不合法 Rust，未采纳（结构性目标「消除不一致窗口」已由单字达成）。

### 二、§1.49 悬案定性实锤：早期 boot panic = `-smp 4` 启动竞态（控制实验·无生产码改动）

修完 BKL 后建镜像上真机，本想做「两次签名一致」三件套，结果 c66（15.9 MB、birth 后 OOM 刷屏）与 c67（13 KB、卡在 picknone）**同一镜像两次签名天差地别**。据此做了严格控制实验（`git stash` 掉 BKL 改动 → 用 HEAD dade6883f 原样重建 → 同法跑）：

| 运行 | 构建 | 核数 | 结果 |
|------|------|------|------|
| c68ctl / c69ctl | **HEAD（无我的改动）** | `-smp 4` | 均 `picknone`+`tail-dump`+**2–3 次 kernel panic**（11 KB / 29 KB 互不相同）|
| c66 / c67 | 含 BKL 改动 | `-smp 4` | 15.9 MB OOM 刷屏 / 13 KB picknone（互不相同）|
| s1a / s1b | HEAD（无改动） | **`-smp 1`** | **结构确定**：size 33185==33185，**0 kernel panic**，birth=1，idle gtick 计数 |
| bk1 / bk2 | 含 BKL 改动 | `-smp 1` | size 33185==33185，**0 kernel panic**，与 control 仅差 UEFI 一页基址抖动（`0x4920000`→`0x4921000`）|

**结论（钉死 §1.49）**：早期内核 panic 是 **`-smp 4` 多核启动竞态，不是确定性 boot 逻辑 bug**。证据链：(a) **控制组 HEAD 自身**在 SMP 下也复现 kernel panic（证明与我的改动无关）；(b) 同一镜像 SMP 两次结果发散（证明运行时非确定性 = 时序竞态）；(c) 单核四次全部 `kernel panic=0`、结构签名一致（证明去掉 SMP 即去掉竞态）。§1.49「判明是真 bug 还是 `-smp 4` 竞态」的取证配方（干净重建 + 单/多核对照）本轮执行完毕。

**方法论收获**：单核 `-smp 1`（`xtask qemu` 里硬编 `-smp 4`，绕过 xtask 直呼 `qemu-system-x86_64 -smp 1` + 同 drive 参数）是一个**确定性可复现的测试台**——SMP 噪声消除后，任何 boot 逻辑改动都能用「单核 size 33185 + 0 panic 基线」做无歧义 A/B。本 unit 的 BKL 改动经此台验证为**零回归**（bk==control）。

### 三、三件套

- **mock**：kernel 816 lib + 3 integration = **819** 通过，0 fail（只增不减）。`cargo check -p minix-kernel --features mock` exit 0（仅 `warning[E0133]` 既有裸指针告警，非本改动）。
- **fmt**：`smp.rs` nightly rustfmt **HEAD=0 == WT=0**（零新增漂移）。
- **真机**：SMP 非确定性（控制组亦崩）故无法对本语义中性改动做 A/B 门；改用**单核确定性基线**验证——bk1/bk2 size 33185 == control s1a/s1b、`kernel panic=0`、diff 仅 UEFI 一页基址抖动 → 零回归坐实。
- **CodeReview**：**PASSED，0 MUST-FIX / 0 SHOULD-FIX**（2 CONSIDER：#2 已采纳，#1 示例不合法未采纳）。

### 四、下一前沿（本 unit 未做，按序转下轮）

> **本 unit 用单核测试台立即掘出确定性根因**：单核 `-smp 1`（bk1/bk2 与 control s1a/s1b 同一签名）轨迹实锤——`vm enter`（bk1 line 106）后紧接 `kernel: vm_handoff free n=0x0 deducted=0x17`（line 79）→ VM server `panic-enter`（line 109）→ 崩于 `os/servers/vm/src/boot.rs:159` `assert!(!self.free_regions.is_empty(), "BootParams: no free memory regions (C: mmap_size > 0)")`。此前记的「birth=1 后 picknone、gtick 空转」是该 panic 的**下游**（VM 死→无内存服务→INIT 无从调度→idle）。**非 BKL 回归**（control 亦同签名），即 S0 登记的「vm_handoff free n=0 → boot.rs assert → livelock」**内存交接布局鲁棒性 bug**（当时归因 fresh-vars，现本 build 亦触发：本次重建令模块装载点整体后移一页 `0x4920000→0x4921000`，把 free-list 裁到 0）。

1. **头号前沿（挡 rc marker，且现可确定性复现）= VM handoff free-list 布局鲁棒性**：内核 `vm_handoff.rs` 的 `build_*`/free-list 扣减（log `deducted=0x17`=23）在特定 memmap 布局下把 15 条 usable 全扣光→`free n=0`→VM `boot.rs:159` 必崩。修法方向（下轮设计）：扣减只应剔除「内核 image / boot modules / 页表 / VM 自alloc / MMIO hole」，**模块上方到 `vm_pa_limit`(0x40000000=1GB) 的空闲 RAM 必须至少保留一条**；核对 `boot-shim: memmaps conv=15 reserved=121` 与 23 条扣减记录的来源，找过度扣减项。有单核 size 33185 + `free n=` 日志作为无噪 A/B 基线。
2. **SMP 早期启动竞态本体**（`-smp 4` 独有、与 #1 正交）：§1.52 证实现存 SMP panic 是**另一类**早期竞态（HEAD 亦崩、同镜像两次发散）。需 `-smp 4` + 只打印寄存器探针（守探针纪律）比对 BSP/AP bringup 与首轮 `bkl_lock_or_inherit` 时序。BKL 单原子化关的是「同 CPU 自死锁窗口」这一类，但非本竞态本体。待 #1 打通单核 rc marker 后再攻。
3. §1.51 遗留：低频 birth 页表-walk bit47（c64 1/6）、`stacktrace.rs:150` 往返野地址、`pagefault VM rip=0 cpu3`（本轮证据：SMP 单发，属竞态家族，待 #2 收敛后复看）。

**取证产物**（gitignore）：`serial_c66.log`–`c69ctl.log`、`serial_s1a/s1b/bk1/bk2.log`。工作树：仅 `smp.rs` 一处生产改动（BKL 单原子化），控制实验的 stash 已 pop 回、无残留。

---

## §1.53 VM free-list 空的真根因 = firmware-heap 腐化（含代码·内核侧）：fix27c landing pad 范式扩展到 memmap + boot_modules

### 根因反转：不是扣减过度，是快照本身是空的

§1.52 把「`vm_handoff free n=0`」登记为「free-list 扣减把 15 条 usable 全扣光」的布局鲁棒 bug。本轮在 `classify()` 切割循环前插临时探针（dump 原始 mmap 快照 + 每条 deduction，`#[cfg(not(mock))]`、nk4a: 前缀）单核实跑（`serial_vh1.log`），结果反转：**`vh raw mmap begin` 后一条 `raw[` 都没有**——`kernel_info.memmap()` 在 classify 时刻返回的 15 条条目全部 `len==0`；而 23 条 deduction（kernel image / root page / bump / 12 模块 blob）数值全部正常。即：扣减逻辑无误，输入已被挖空。

### 病灶：boot-shim 的 UEFI 池堆撑不过 boot（fix27c 早就说过）

`os/boot-shim/src/uefi_helpers.rs::build_memmaps()` 用 `Vec` + leak 在 UEFI 池堆上构造 `memmap`/`reserved_regions` 两个 slice，`boot_modules` 同源。fix27c 已为 `reserved_regions` 撞过同一堵墙并在 `globals.rs::RESERVED_REGION_STORE` 注明：「firmware-pool-backed 的 `&'static` slice，内核页表不重放那段映射，高半区跳转后同 VA 读回零」。但 `memmap`/`boot_modules` 是漏网之鱼。同一 run 内的时间线证据：Step 4 DM-coverage 的 `dm-mem` 探针（读 `kernel_info.memmap()`）还能打 16 条真数据 → 到 `vm_handoff::classify` 全零；`KernelInfo` 结构体其它字段（含同堆 `boot_modules` 内容）仍可读 → 腐化发生在 payload 页级（被复用/破坏），非结构级，且随 boot 推进渐进发生（VM ELF 加载 + boot-identity 驱逐期间）。

### 修：landing pad 范式覆盖全部 payload

- `os/kernel/src/globals.rs`：新增 `MEMMAP_REGION_STORE: [MemoryRegion; MAXMEMMAP=128]`、`BOOT_MODULE_STORE: [BootModule; NR_BOOT_MODULES=12]`、`BOOT_MODULE_NAME_STORE: [[u8; 16]; 12]`（名字定长字节池，15 字节 strlcpy 式截断与 `vm_handoff::copy_name` 的 `proc_name: [u8;16]` 同界，对现有消费者零行为分叉）；`BootModule` 入 `BklProtected` allowlist（boot 期写一次、其后只读，同 `MemoryRegion` 契约）。
- `os/kernel/src/lib.rs::store_kernel_info`：在 arch_boot 入口（数据尚完好时）`ptr::copy` memmap、逐条重建 boot_modules（name 经裸指针从 `.bss` 字节池派生 `&'static str`），重指全局副本三个 slice；容量超限 fail-fast assert 不截断；re-store（kmain 拿全局副本再存）幂等守卫与 fix27c 同款。`platform_sources` 同病但 grep 确认零 post-boot 消费者，`param_buf` 两路径硬编 `&[]`——均登记不盲改。
- `os/libs/minix-boot/src/kernel_info.rs`：`BootModule` 加 `#[derive(Debug, Clone, Copy)]`。
- 新增 mock 回归测试 `test_store_kernel_info_lands_firmware_slices`：断言重指（ptr 不等于源）、内容完整、re-store 幂等。

### 真机验证（单核确定性测试台，§1.52 方法论）

| run | 镜像 | 结果 |
|-----|------|------|
| vh1 | 探针版+修 | `raw[0..15]` 16 条实数据；`vm_handoff free n=8`（原 0）；无 `boot.rs:159` panic；11 boot exec + runtime birth 0xd–0x15 |
| lh1 | 探针版+修（长跑） | 同上，尾部 PM(0)↔VFS(1) 轮转各 ≈4.5 万 pick＝B26 livelock 形态复现 |
| fm1/fm2 | 回滚探针后重建 | 两次结构签名一致：`kernel panic=0`、`no free memory`=0、exec=11 |

三件套：mock **820**·0fail（+1＝新回归测试，只增不减）；nightly rustfmt 三改动文件 HEAD==WT 零新增漂移（globals.rs 一处 16→17 已对齐回 16）；镜像重建 + 真机双跑签名一致。临时探针已按纪律回滚（`git checkout os/kernel/src/vm_handoff.rs`）。CodeReview 子代理 **PASSED 0 MUST/SHOULD-FIX**（逐条确认 unsafe 生命周期/别名/幂等/截断分叉/遗漏 slice 风险）。

### 下一前沿入口（按序）

1. **头号＝B26 PM↔VFS 乒乓活锁**（本轮把它重新推回主路径）：§1.49-A 半条证据链在手（INIT 第二 fork 子 runcom `runetcrc(true)` chroot 分支 → `exec_command` deref `cr2=0x1` → VM pf-exit noaddr → SIGSEGV×2 → 子死 → PM↔VFS 乒乓）。配方：单核 + 只打印寄存器探针（守探针纪律，缺页 handler 内禁页表 walk）抓 faulting rip 定位 null+1 的具体指令，再对位 minix3 `exec.c`/`main.c` 查语义缺项。
2. SMP 早期启动竞态本体（§1.52 遗留，`-smp 4` 独有）。
3. §1.51 遗留：低频 birth bit47 / `stacktrace.rs:150`。
4. latent：`platform_sources` 未 landing（当前零消费者，接入 DTB/RSDP 解析前必须补）。

**取证产物**（gitignore）：`serial_vh1.log`、`serial_lh1.log`、`serial_fm1/fm2.log`。工作树：本单元 3 文件生产改动（`lib.rs`/`globals.rs`/`kernel_info.rs`），探针零残留。

---

## §1.54 B26 kerninfo 双缺陷 + B28 EXITING 位语义坍缩（含代码·arch/kernel/PM 三侧）

### B26：两条独立腿，签名逐层翻转实锤

§1.53 头号前沿「INIT runcom 子 `exec_command` deref cr2=0x1」按 §1.49-A 配方（单核 + 只打印寄存器探针）开抓。取证链：

1. **bn1**（b26 探针，trap_dispatch ForwardToVm 臂转发前，门控 `cur_nr==13 && cr2<0x1000`）：确定性崩溃 `rip=0x204e61`。addr2line=`exec_command` 内联的 `new_image_stack_top`；objdump：`int $0x21(ecx=6)` 后 `mov %r10,%r8` → `mov 0x40(%r8),%rax`（读「页」内 kuserinfo 内部指针=1）→ `cmpq $0x10,(%rax)` deref 0x1。
2. **ki13 探针**（syscall 上下文，KernInfo 臂，合规 walk）：内核静态页 `+0x40` 实读 `kword=0x200000800`（正确），但调用者页表 `PML4E 有效 → PDPT[2]=0`（kerninfo VA 在该根无映射）而用户读却「成功」——矛盾消解＝**用户根本没读 0x2_0000_0040，r10 是陷阱残留垃圾另指他处**。

**缺陷 A（secondary 车道断链）**：`set_secondary_ipc_return` 写 `ctx.rbx`（Task C 状态车道 RBX→R10 迁移时有意保留），但 int33 立即返回臂只同步 `frame.rax` + `sync_status_register_to_frame`(R10)——ctx.rbx 永远达不到用户。C 真值：i386 `%ebx` 一条车道兼状态+secondary 两职（`ipc_minix_kerninfo.S` 返回后 `movl %ebx,(%ecx)`；`proc.c:685-693`），两职按调用互斥。**修＝x86-64 secondary 车道改 `gp_regs[GP_R10]`**（`arch/src/x86_64/boot.rs`，测试 `set_secondary_ipc_return_assigns_status_lane` 锁死 R10 收到值 + RBX 不被污染；trap_dispatch L645/minix-sys ipc.rs/arch_trap.rs/arch boot.rs trait doc 同步）。

**缺陷 B（新地址空间缺 kerninfo 映射）**：kerninfo.rs 模块 doc 登记的「replicating this mapping into each new root moves to VM」从未发生；C 经 VMCTL_KERN_PHYSMAP 协议（`do_vmctl.c:112`→i386 `memory.c:746/847 arch_phys_map`）由内核把 `.usermapped` 段映进每进程空间。**修＝内核在 `vmctl_set_addr_space`（唯一提交点）注入，不新增外部契约**：`kerninfo.rs` 在 `map_and_publish` 时缓存 `KERNINFO_PAGE_PHYS: AtomicU64`（帧永不移动）+ `kerninfo_page_phys()` accessor；`syscall.rs` 注入块先查后装（重绑同一根幂等、virtual_copy 继承过则跳过），`read_only` 标志、中间表页 `boot_pt_alloc`（其区域已从 VM free list 扣减，§1.53 记账），至多 2-3 页/根有界泄漏登记。mock 下 `KERNINFO_PAGE_PHYS=0`→跳过，零影响。

签名翻转：bn4（修 A 后）b26=0 但翻为 `pf-exit noaddr cr2=0x200000040`＝缺陷 B 独立实锤 → bn5（修 A+B）b26+noaddr 全消，boot 推进到「init done/run enter」18 服务全诞、无崩溃。§1.49-A 的「乒乓活锁」＝本崩溃的下游后果，崩溃消除后未再出现（bn6 无 4.5 万级 pick 轮转）。

### B28：Lifecycle 互斥枚举压掉 C 的 OR 位语义

bn5 终态：PM panic `servers/pm/src/ipc/vfs.rs:473 handle_vfs_reply: EXIT/CORE reply but process is not EXITING (slot 13)` → PM 死 → `ipcerr ECALLDENIED` → pmstall2 静默停滞。C 同样 assert（main.c:362）→ fail-fast 合规，真 bug 在上游状态：

- C `zombify`（`forkexit.c:619/621`）= `mp_flags |= ZOMBIE/TRACE_ZOMBIE`——**EXITING 位保留**，直到 `cleanup()` 释放槽位才清；故 VFS EXIT 回复（晚于 `exit_proc` 内同步 zombify 到达）时进程是 `EXITING|ZOMBIE`，assert 通过。
- Rust `Lifecycle` 互斥枚举 zombify 把 `Exiting{..}` 整个替换为 `Zombie{..}` → `is_exiting()` false → assert 必炸。**每个正常退出的进程都会炸 PM**（bn5 是 slot 13 第一个撞上的）。

**修＝`is_exiting()` 改 C EXITING 位语义**：覆盖 Exiting|TraceZombie|Zombie|ToldParent 全死亡窗口（`mproc/lifecycle.rs`）。15 个调用点逐一对位 C 位测试（`alarm.c:330`、`signal.c:279/372/693`、`forkexit.c:646/775`、`main.c:81/362/422`、`event.c:86/265/330`、`trace.c:64-141`）——全部是 `& EXITING` / `(IN_USE|EXITING) != IN_USE` 位读，无一需要「仅 Exiting 变体」；变体严格判断处（如 `exit_restart` 的 `matches!(lc, Exiting)` zombify 门、`check_parent` 的 TraceZombie 分支）本就是显式 `matches!`，不受影响。附带修正两处旧测试期望（它们断言的是缺陷镜像）：`test_told_parent`（TOLD_PARENT 仍持 EXITING 位，forkexit.c:717 只 OR）翻为 `assert!(is_exiting())`；`process_ksig` SIGSNDELAY 测试——check_pending 终止进程后 C 尾部（signal.c:372-378）返 **EDEADEPT**，改断言 `Err(InvalidEndpoint)`。

### 真机验证与三件套

| run | 镜像 | 结果 |
|-----|------|------|
| bn1-bn3 | 探针版 | 取证：崩溃指令/ki13 双缺陷现场坐实 |
| bn4 | 修 A | b26=0，翻转 noaddr cr2=0x200000040（缺陷 B 独立实锤） |
| bn5 | 修 A+B | 崩溃全消，init done/run enter，尾部暴露 B28 panic → 停滞 |
| bn6 | A+B+修 B28（带探针） | notEXITING=0，47862 行（+71%），停 B29 OOM |
| bn7/bn8 | 探针回滚重建 | 双跑签名逐字一致：`OOM-RT size=00a000 px=200/200 fp=0aa/200`、终态 `rip=0x202d68 r10s=4` |

三件套：mock kernel **817+3=820**·0fail（基线不减）、pm lib **418**·0fail、arch **243**·0fail；nightly rustfmt 九个改动文件 HEAD==WT 零新增漂移；镜像重建 + 无探针双跑一致。临时探针 3 处全部回滚（trap_dispatch b26 块、syscall ki13 块、kerninfo `kerninfo_page_kernel_va`）。CodeReview 子代理：**MUST-FIX×1**（`qemu-tests/test-user-trap` 手编 CPL3 payload 仍 `mov rax,rbx`/`mov rax,[rbx]` 读旧车道→改 `4C 89 D0`/`49 8B 02` + sh/注释 R10 同步）已修；**SHOULD×1**（trap_dispatch:1737 陈旧 RBX 注释）已修。

### side 报告对账（另线程只读诊断）

诊断一（BKL 两步写自死锁）＝§1.52 已落地（单 AtomicIsize），方案一致；诊断二（B27 birth 页表 TOCTOU/VMINHIBIT 缺口）＝§1.51 已证伪，本单元进一步实测 B26 真根因（kerninfo 车道+映射）与页表竞态无关；「syscall_process.rs 未提交」提醒不实（该文件在 dade6883f 内，工作树当时干净）。报告中唯一被吸收的方法＝两次同址采样取证纪律（ki13 探针按此设计）。

### 存量登记（本单元发现的既有红）

1. `minix-tests --test pm_sched` 编译失败 E0046（`sendnb`/`send_blocking` trait 漂移）——**HEAD 上同样红**，非本单元引入。
2. `minix-pm --test run_once_integration` 2 失败（`ECHILD`/`ENOSYS` 的 m_type 符号约定：测试期望正值、代码回 `-errno`）——HEAD 同红；**注意这可能是真约定分歧**（C reply m_type 携带正 errno），归 rc 打通后单独裁决。

### 下一前沿入口（按序）

1. **头号＝B29 VFS 运行时堆 OOM**：bn7/bn8 确定性签名——VFS（slot 1）`alloc` 40KB 失败，diag `slabs=9/200 big=6/20 px=512/512 fp=170/512`：bump 区耗尽而 free-page 栈尚有 170 页 ⇒ **big-block 连续 run 无法从散页拼出**（碎片）或 free 页语义另有账（先读 `alloc.rs` grow/big-block 分配路径三行判定家族）；已知 follow-up＝VM-backed heap supplier（alloc.rs 模块 doc「C servers grow a real heap through VM brk」）+ `MAX_BIG_BLOCKS=32` 注释自认同族债。OOM 下游＝SIGKILL VFS → RS 连锁 panic（`servers/rs/src/b...`）→ SCHED(4) 空转终态。
2. SMP 早期启动竞态本体（§1.52 遗留，`-smp 4` 独有）。
3. §1.51 遗留：低频 birth bit47 / `stacktrace.rs:150`。
4. latent：`platform_sources` 未 landing（零消费者）。

**取证产物**（gitignore）：`serial_bn1..bn8.log` + `bn1..bn8.txt`。工作树：本单元生产改动 = arch 2 文件 + kernel 2 文件 + minix-sys 2 文件（注释）+ PM 2 文件 + qemu-tests 2 文件（payload 字节 + 注释），探针零残留。

---

## §1.55 B29 VFS 堆 OOM + B30 boot 页表池耗尽：双容量扰动，真机首次零 panic（含代码·minix-rt/boot-shim）

### 一句话

B29 定性＝合法工作集超固定池（非泄漏），`GLOBAL_POOL_BYTES` 512→1024 页翻面；翻面后暴露同族 B30（boot 页表 bump 池 128 页无 free，kerninfo 注入烧尽），`prepare_boot(128→1024)` 再翻面；四轮真机 0 panic，boot 历史首次推过全部已知崩溃点，新前沿 B31＝rc marker 前的命令面卡点。

### B29 取证（三行定性）

1. **分配主**：`size=00a000`（40960）＝`servers/vfs/src/exec_worker.rs:296` 的 `hdr_buf = Box::new([0u8; HEADER_BUF_PAGES(10) * 4096])`——每次 exec 的 ELF 头读缓冲，10 页 big-block，与 OOM 签名逐位吻合。
2. **足迹量化**（临时 `size_of` 测试实测后回滚）：`FProc` 4368B × `NR_PROCS` 256 ＝ 273 页；`Filp` 96B × 1024 ＝ 24 页；`Vnode` 120B × 1024 ＝ 30 页（含对齐）→ 三表 ≈ **327 页**，占 512 页池的 64%。`FProc` 大的元凶是 `filps: [Option<usize>; 255]`——`Option<usize>` 无 niche 每槽 16B，C 的 `file_desc *` 仅 8B，同一张表 Rust 天然翻倍。
3. **碎片机制**（`alloc.rs` `supply_pages` L328 注释自陈）：连续 run 只从 bump 区发，free-page 栈只收单页。bn7/bn8 签名 `px=200/200 fp=0aa/200`＝bump 512 页全耗尽、170 散页躺 free 栈、10 页连续请求无从拼起 → premature OOM 必现。

**C 对位**：C 的 fproc/filp/vnode 表在 VFS 的 BSS（exec 时 mmap），malloc 堆经 `brk()`→`_syscall(VM_PROC_NR, VM_BRK)` 无上限生长（`minix3/minix/lib/libc/sys/brk.c:24-30`）；minix-rs 把它们全塞进 2MiB 固定池＝容量墙是重写引入的。RS 在 C 默认即申请 8MiB 预映射（`rs/const.h:83 RS_VM_DEFAULT_MAP_PREALLOC_LEN (1024*1024*8)`）。VM 服务端 `VM_BRK` 臂已接线完整（`servers/vm/src/brk.rs` 435 行 + dispatcher:688），缺的是**客户端 heap supplier 腿**（allocator 里发 IPC 有再入约束，独立单元）。

**修（扰动实验）**：`GLOBAL_POOL_BYTES` 512→1024 页（`MAX_SLABS`/free 栈/记录表全部经 `GLOBAL_POOL_BYTES` 派生自动跟随，无表满旧债）。物理代价见 B30 侧账。

### B30 签名翻转与同族扰动

池 1024 后 bn9（测试台默认内存 128M）死 `vm_server.rs:647 exec_bootproc: pfs failed: boot segment page allocation failed`——**bootproc 腿 eager materialize**（`vm_server.rs` boot segment 循环逐页 `alloc_pfn`+拷贝，含 .bss 全段）随池扩容线性涨页，128M 预算翻越不过去。对照跑 `-m 512M`（bn9m）：boot 服务 11 exec 全过、panic 移位为

```
vmctl_set_addr_space: failed to map the kerninfo page into the new root: AllocationFailed   (syscall.rs:2990)
```

即 B26 fix-B 的 kerninfo 注入腿从 `boot_pt_alloc` 拿中间表页——该池 boot-shim 只给 **128 页**（`prepare_boot(128)`），boot 期 identity/high/DM 建立已耗大头，此后**每次新根绑定点 1-3 页、纯 bump 无任何 free 路径**（旧根页永不回收），~40 次 bind 后确定性 `.expect` panic。本单元同样以扰动续命：`prepare_boot(128→1024)`，注释钉死三件事：选值算式、`vm_handoff` 把整段从 VM free list 扣除的代价（每 boot 多占 4MiB）、「这只是推迟确定性 panic」。结构性修法（页表页可回收 / 把 kerninfo 映射责任移交 VM——`kerninfo.rs` 模块 doc 登记的 responsibility 转移）列 B30 follow-up。

**测试台标准配置自此变更：`-m 512M -smp 1`**（§1.52 单核台 + 本轮内存钉桩），命令见本节末。

### 取证中还原的两处事实错误（存量登记）

1. 旧 `GLOBAL_POOL_BYTES` 注释「池页按需物化、名义大小不占物理内存」——对 **bootproc 腿不成立**（eager materialize 全段）；bn9 死因即为此。alloc.rs 新注释已按 bootproc eager / 常规 exec 两条腿分开表述。
2. **本重写 VM 未实现 PREALLOC 语义**：`MmapFlags::PREALLOC` 只翻译成 `VrFlags::PREALLOC_MAP` 记账位（`mmap.rs:110`），`map_region` 不取帧，anon 段 memtype `AnonymousMemory` 无 `ev_new`（`memtype.rs:228` doc 自陈按需分页）；C 的 `region.c:492-499` 收 `MF_PREALLOC` 即 `map_handle_memory` 预取全部帧。二者行为差＝页物化时机（首次触碰 vs mmap 时），外部可观察语义近似等价，故暂按 Refactor 级登记，是否补实现待后续裁决（若做「exec 时预占防 later-OOM」的 C 语义时需要）。

### 真机验证（bn9-bn11b）

| 轮 | 配置 | 结果 |
|---|---|---|
| bn9 | 池1024, 128M | 669 行，`exec_bootproc … page allocation failed`（物理预算） |
| bn9m | 池1024, 512M | 54002 行，VFS OOM 消失；死 B30 `AllocationFailed`（kerninfo 注入） |
| bn10/10b | 池1024+bump1024, 512M | **0 panic**，77418/77996 行（+43% vs bn9m） |
| bn11/11b | 同上（含 CodeReview MUST 修后重建） | **0 panic**，75882/77757 行，签名一致 |

**B31 新前沿现场**：bn11 里 birth 事件停在第 8446 行（累计 26 个），其余 ≈6.9 万行是多服务事件循环 pick spam（尾部 pick 分布 slot8=2370/slot1=1404/slot0=807/slot a·b 数百，rip 多样=非双进程活锁）；rc marker（`minix-rs rc: minimal boot script marker`）仍未打印——INIT runcom 命令面停在某等待点，定位是下一单元第一动作。

### 三件套与 CodeReview

- mock：`minix-kernel` 817+3=**820** 0 fail（基线不降）；`minix-rt` **57** 0 fail；clippy 改动前后同为 122 存量告警（stash 对照）。
- fmt：`alloc.rs`/`lib.rs`/`boot-shim/main.rs` nightly `--check` HEAD==WT 零新增漂移。
- 镜像重建 + 真机双跑×2（bn10 系与 bn11 系）零 panic、签名一致。
- CodeReview（本单元）：**MUST-FIX 1 已修**——`nk4c_oom_tag` 探针把分母写死 `/200`，池扩容后会打 `px=400/200` 非法形态污染逐字签名判据，改为 `MAX_SLABS`/`MAX_BIG_BLOCKS`/`total_pages`/`GLOBAL_POOL_PAGES` 派生真值（`lib.rs`）。SHOULD 3 全采纳（PREALLOC 断言精确化、WORKLOG §1.55 登记、bump 扣除代价与算式入注释）；NIT 2 采纳（diag 引原始 hex、`PoolStorage` doc「sixteen pages」更新）。

### 命令（测试台标准配置更新）

```bash
# 建镜像
cd os && cargo run -q -p xtask -- image --arch x86_64 --release
# 真机（单核 + 512M，EXIT=124 正常）
timeout 150 qemu-system-x86_64 -m 512M -smp 1 \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=target/image/x86_64/fw_vars.fd \
  -drive file=target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:target/nk4a-runs/serial_bnN.log -display none -no-reboot -device isa-debug-exit
# 去噪
tr -cd '\11\12\15\40-\176' < serial_bnN.log > bnN.txt
```

### 下一前沿入口（按序）

1. **头号 B31**：bn11 尾部定位 INIT 命令面卡点（runcom 哪一步、等谁的回复；对照 `notes` 的 18-stage 期望），rc marker 三连是终目标①。
2. 结构债三件套：VM-backed heap supplier（allocator→VM_BRK 客户端腿，注意 alloc 再入）；boot bump 可回收或 kerninfo 映射责任移交 VM；`MAX_BIG_BLOCKS=32` 同族。
3. VM PREALLOC 语义缺口裁决（存量 2）。
4. SMP 早期启动竞态本体（§1.52 遗留）。
5. §1.51 遗留：低频 birth bit47 / `stacktrace.rs:150`；`platform_sources` latent。

**取证产物**（gitignore）：`serial_bn9*.log`/`serial_bn1*` + 对应 `.txt`。工作树生产改动＝`alloc.rs`（池常量+注释）、`lib.rs`（探针分母）、`boot-shim/main.rs`（bump 页数+注释）三文件，无探针残留。

---

## 1.56 B31 imgrd 缺 /bin/echo 播种（已修）+ B32 exec 回复携带伪 errno（新头号前沿）（含代码·xtask 装配面）

§1.55 登记的 **B31（init-state Runcom↔SingleUser 无限循环）根因实锤并修复第一层**，同时翻出更深一层的 **B32**。

**B31 取证链**（全部真机+静态双向坐实）：
1. 循环形态：bn11/bn12 均 15 轮 Runcom→SingleUser→Runcom，rc 子进程每次都干净退出（无 panic、无信号）。
2. 盘面对账：`mcopy` 抽 imgrd 实测——`generate_etc_proto`（`os/xtask/src/image.rs`）的 bin 段只播 `sh` 一行；`/bin/echo` **从未播种**，而 `os/etc/rc` 唯一外部命令恰是 `echo "minix-rs rc: … marker"`。
3. 语义对位：本重写 shell 目前只有 exit/cd 两内建（`os/commands/bin/shell/src/bin/sh.rs` 的 eval 臂 `Some("exit")`/`Some("cd")`），rc 的 `echo` 走外部命令腿——注意 **C ash 有 echo 内建**（`minix3/bin/sh/builtins.def` 的 `echocmd`，无 TINY/SMALL 门，CodeReview 揪出的初稿虚言已改），但 C 镜像同样独立装 `/bin/echo`（`minix3/bin/echo/`），播种即对位，内建化留待 shell 扩内建面裁决。假定 exec 腿可通：`child_exec` PATH 搜 `/bin:/usr/bin` 全 ENOENT → `terminate(127)` → rc 非零 → runcom 的 `_ => Attempt::SingleUser` 腿。
4. **修**：`generate_etc_proto` 签名改 `bin_entries: &[(&str,&str)]` 列表化（18-stage 命令面后续按需上收），生产传 `sh+echo`；构建步 `-p minix-shell` 扩为 `-p minix-shell -p minix-fileops`（fileops 含 echo 但不在 BOOT_MODULES，此前从不为 module target 构建——target 目录里 9-21 的旧产物纯属侥幸存在）；测试同步（plant echo、新断言、16 步 cargo 计数不变）。xtask 12/12 绿。

**但是**：播种 echo 后真机 bn12 与 bn11 **完全同形**（75874 vs 75882 行、15 轮循环）——marker 仍未出。探针版 bn12p/bn12q/bn12s 三连（init 侧 rc-status/exec-fail/execve 腿标签 + shell 侧三探针）实锤下一层：

**B32（新头号前沿）**：init 的 runcom 子进程**从未 exec 成功**（bn12p 探针实锤；即 B31 的 127 链是下游假设而非当前近因）——
- rc 子进程退出码 = 5 = `runcom.rs` 的 `exit_process(5)` 腿（`fn runcom` 子分支）（`host.exec` 失败回返 → stall → exit 5）；
- shell 二进制内嵌探针一次未触发 → sh 的 main 从未获得控制权；
- `exec_command` 六腿探针定位：A-E 全过（帧装配正常），**腿 F = `pm::exec_via` 失败**；
- 原始值探针实锤：**sendrec 往返成功、回复 `m_type = -5114394`**（0x4E0A1A）。这不是任何合法 errno（≤130 量级），且数值落在用户态堆 VA 区（同 run vm-pf 轨迹 0x224610–0x551000 之间）——**某个指针被当 errno 写进了 exec 回复**。
- 嫌疑链（VFS `exec_worker::pm_exec` 的错误车道全是 `neg(msg.m_type)`/`neg(t.0)` 透传：`open_exec` lookup 闭包、`req_stat`、`req_read`、`grant_magic().map_err(neg)`；PM `do_exec`→`forward_exec` ReplyLater 后由 VFS 回 PM、PM 再回 INIT）——下一单元从「谁往回复 m_type 写了堆地址」入手：先 VFS 终局 `queue_reply_msg` 处打 result 原始值，再二分 helper。
- **B31 修复仍然必要且独立正确**（echo 缺件是真实盘面缺陷，B32 修好后立刻会撞上它）；B32 是既存问题非本单元引入（rc marker 从 NK4-C 开案起从未达成过）。

**探针回滚**：runcom.rs/execve.rs/sh.rs/pm.rs 四文件纯探针全部 `git checkout` 回滚，工作树仅剩 `image.rs` 真实修复；bn13/bn13b 双跑 `probe_residue=0` 坐实。

**三件套**：mock kernel 817+3/rt 57/pm 418/arch 243·0fail（基线不减）；xtask 12/12；fmt `image.rs` 漂移 WT=4==HEAD=4 零新增（857 处新增漂移已按 rustfmt 建议收单行）；镜像重建+真机双跑 bn13/bn13b 签名一致（panic=0、Runcom=15、marker=0、无残留）。

**下一前沿（按序）**：① **头号＝B32 伪 errno 定位与修复**（`-5114394`≈用户堆指针；修通后 rc marker 有望首达——echo 已就位）；② init 侧 stall 的 sleep 腿疑似不睡（15 轮×STALL_TIMEOUT 30s 塞进 150s 内，独立小案）；③ 结构债三件套（VM-backed heap supplier/boot bump 可回收/`MAX_BIG_BLOCKS`）；④ VM PREALLOC 语义缺口裁决；⑤ SMP 早期竞态（§1.52）；⑥ §1.51 遗留。

---

## 1.57 B32 exec_worker 同步 FS 腿丢事务打包（已修）→ 翻出 B33 残余 EIO（新头号前沿）（含代码·VFS 侧）

§1.56 头号前沿 **B32（exec 回复携带伪 errno `-5114394`＝用户堆指针 0x4E0A1A）根因实锤并修复**。修后伪指针归零、exec_via 返回**合法 errno 5（EIO）**，翻出下一层 **B33**。

**B32 根因（静态算术 + C 对位双向实锤）**：VFS↔FS 协议每一条请求的 `m_type` 都必须做**事务打包**——① FS（fs-rt）按 `TransactionId::decode(msg.m_type)`＝`call = m_type >> 16` 路由（`os/fs/fs-rt/src/transport.rs`）；② FS 回复 `TransactionId::encode_reply(status, transaction) = (status << 16) | transaction`，VFS 须按 `TRNS_DEL_ID = (short)(m_type >> 16)` 取真实 status。**关键算术**：`FS_BASE=0xA00`、`REQ_LOOKUP=FS_BASE+26=0xA1A=2586`；伪值 `0x4E0A1A` 拆成 `(高16=0x4E=78 status) | (低16=0x0A1A=REQ_LOOKUP 回显)`——正是事务打包形态。C 黄金参照：`minix3/minix/servers/vfs/comm.c:21` `wp->w_sendrec->m_type = TRNS_ADD_ID(...)`；`minix3/minix/servers/vfs/main.c:88` `m_in.m_type = TRNS_DEL_ID(m_in.m_type)`。

`exec_worker.rs` 的同步 `ipc.sendrec(fs_e, …)` **绕过了 `fs_comm::send_fs` 异步路径（`os/servers/vfs/src/fs_comm.rs` 有 `TransId::add`）的戳入/拆封**：裸发 `m_type = REQ_xxx`（高 16 为 0 → FS 解出 call=0 无法路由），且把回复 `msg.m_type` 原样 `neg()` 当 errno → 旧代码产生 `neg(0x4E0A1A) = -5114394` 伪 errno。

**修（仅 FS 腿）**：`neg()` 后新增两助手 `fs_trans_stamp`（出向 `minix_types::trns_add_id(m_type, 0)`）/`fs_trans_status`（入向 `minix_types::trns_del_id(m_type)`）；对 4 个同步 FS 腿接入——`open_exec` lookup 腿、`req_stat` 腿、`req_read` 腿、ELF segment-read 腿：发送前 `fs_trans_stamp(&mut msg)`、读回复用 `fs_trans_status(&msg)` 取代裸 `msg.m_type`。**VM 腿（`vm_mmap`/`vm_procctl_clear`）与 PM 腿（`pm_newexec`）不打包事务、保持不变**——它们是 taskcall 风格服务器，`m_type` 直接就是结果（对位 C，只有 VFS↔FS 往返过 TRNS 打包）。transid 取 0（同步腿靠阻塞往返自匹配、不占 worker 号空间，见下 CodeReview）。（助手初稿用 `fs_comm::TransId::add(m_type, 0)`、status 自带位运算式，经 CodeReview 收敛到 `minix_types` 权威件并修正低 16 撞号，见下段。）

**运行时硬证（bn16 临时探针，已回滚）**：在 init `exec_via` 返回处打印 path+errno，实测 `nk4a: b32 exec path=/bin/sh errno=5` 命中 44 次、全为干净 errno 5；伪指针 `5114394`/`0x4E0A1A` 归零。无探针双跑 bn15/bn15b 签名一致（79303/79319 行、panic=0、bogus=0、Runcom=15、marker=0）。探针文件 `execve.rs` 已 `git checkout` 回滚，工作树仅剩 `exec_worker.rs` 真实修复。

**B33（新头号前沿）**：B32 修后 exec_via 返回**合法 errno 5（EIO）**而非伪指针，但 rc marker 仍未达（Runcom=15 循环依旧）。init `exec_via` 逻辑 `match perform_syscall(...) { Ok(_) => Errno::EIO, Err(e) => e }`——任何正/零回复（Ok 车道）都被映射为 EIO（对位 C `execve.c:53-58`，因成功路径永不返回）。真问题＝VFS `pm_exec` 现返回**正 status**（被 Ok 车道吞成 EIO）或某下游腿失败被 EIO 掩盖。下一取证配方：在 VFS `pm_exec` 各 bail 站点与 `queue_reply_msg` 终局打印 status 符号与来源腿，定位为何返回正值（疑 `ExecError::NoExec.to_errno()` 正 errno，或 lookup 成功后 stat/read/mmap 某腿）。

**三件套**：mock vfs lib 531·0fail（基线 530+1 新回归测 `test_fs_transaction_pack_roundtrip`，只增不减；pin stamp 把 REQ 移高 16、status 从 `0x4E0A1A` 提 `0x4E`、`neg→-0x4E`）；改测 `exec_worker` 出向断言期望打包值 `TransId::add(REQ_READ,0)`（裸发回归即测红）；fmt `exec_worker.rs` 漂移 WT=0==HEAD=0 零新增；clippy exit 0；镜像重建+真机双跑 bn15/bn15b 签名一致 + bn16 探针实锤伪指针归零。

**CodeReview（本单元，target=exec_worker.rs 未提交改动）**：**2 SHOULD-FIX 全采纳**——① 原 `fs_trans_stamp` 用 `TransId::add(m_type, 0)` 低 16 落 `encode(0)=0xB01`（＝异步 worker slot 0 的合法 transid，与主循环 `handle_fs_reply` 按低 16 反解 worker 槽相冲、错投可污染 worker 0）；改为 `minix_types::trns_add_id(m_type, 0)`（低 16＝0，与姊妹同步腿 `request.rs` REQ_READSUPER 同一裁决，错投被 `SpuriousTransid` 丢弃）；② 原 `fs_trans_status` 函数体与 `minix_types::trns_del_id` 逐字符相同，委托权威件（vfsif.h 三行宏的权威逐行落地）。新测补一条 `trns_get_id(req)==0` 断言 pin 住不撞 worker 号空间；出向期望值改用真机线形状常量 `trns_add_id(REQ_READ,0)`（不复用生产助手自我印证）。**重构后重新三件套**：mock 531·0fail / fmt WT0==HEAD0 / clippy exit 0；镜像重建+真机双跑 bn17/bn17b 签名一致（81398/81401行、bogus=0、panic=0、Runcom=15、marker=0），确认低 16 从 0xB01→改 0 无回归。**2 CONSIDER 登记为同族 follow-up**（不阻塞本单元）：③ C `fs_sendrec` 的 `ERESTART→EIO` 折叠腿四同步腿未建模（`comm.c:164-167`，全仓 `os/fs/**` 暂无 ERESTART 发出点，异步侧 `main_loop.rs:4756` 已建模）；④ 跨文件同族——`main_loop.rs` 的 `flush_pending_puts` 仍裸发 REQ_PUTNODE（无戳包、回复不剥 id），exec 失败/终局 `release_vnode` 灌进此队列，同一 B32 类待收口。

**下一前沿（按序）**：① **头号＝B33 残余 EIO**（VFS pm_exec 正 status/下游腿取证，定位 init exec_via Ok→EIO 掩盖的真实失败点）；② init 侧 stall 的 sleep 腿疑似不睡；③ 结构债三件套（VM-backed heap supplier/boot bump 可回收/`MAX_BIG_BLOCKS`）；④ VM PREALLOC 语义缺口裁决；⑤ SMP 早期竞态（§1.52）；⑥ §1.51 遗留。

---

## 1.58 B33 exec 权限检查传错权限位（已修）→ 翻出 B34 运行期堆 OOM（新头号前沿）（含代码·VFS 侧）

### 一句话

§1.57 头号 **B33** 的残余 EIO 拆开是两层：表层 **B33b**＝`exec_worker.rs` 调用 `forbidden_decision` 时把「想要的访问位」传成了 9 位模式里的 owner-可执行位（`0o100`），而权限判决把它和进程能力三元组（低 3 位）同档比对，导致连 root 执行自己的 `0o100755` 普通文件都被误拒 `EACCES(13)`；这条是真阻塞（`/bin/sh` 从未获控）。深层 **B33a**＝即便拒了，错误码经 `ExecError::to_errno()` 以**正值**返回、被 init 的 `exec_via` 走「成功车道」吞成 `EIO`，把真实 `EACCES` 掩盖成 `EIO`（诊断性、非阻塞，登记 follow-up）。修 B33b 后真机 `/bin/sh` 首次获控、`Runcom` 从历史 15 轮循环塌成 1 次，暴露 **B34**＝运行期内核堆 `OOM-RT`（与 §1.55 的 B29 同签名，属已登记的结构债家族）。

### B33 取证：两枚临时探针锁死「正值 status + 被误拒的权限」

静态链路：init `pm.rs:574 exec_via` 对 `perform_syscall` 的 `Ok(_)` 臂一律映射为 `Errno::EIO`；`syscall.rs:94 perform_syscall` 约定 `m_type<0 → Err(-m_type)`、否则 `Ok(m_type)`。所以只要 VFS 的 exec 回复 `m_type` 是**正值**，INIT 就把它当成功、吞成 EIO——这解释了 §1.57 里 marker 迟迟不出却只看到 `errno=5`。为确认 VFS 侧到底回了什么，插两枚探针（单元收尾已 `git checkout` 回滚）：

- **探针 1**（`main_loop.rs` exec 分支 `Err(errno) => VfsReply::Exec{status:errno}` 之后，打印 `nk4a: b33 <O/E><8位hex>`）：真机 bn18 命中 `nk4a: b33 E0000000d`——走 **Err 臂**、值 `0x0D`＝**13（正）**。即 VFS `pm_exec` 确实失败并原样透传了一个**正的** `EACCES`，正值经 INIT 成功车道被吞成 EIO（B33a 掩盖机制实锤）。
- **探针 2**（`exec_worker.rs:655` 取到 `v.mode/v.uid` 之后，打印 `nk4a: b33m <mode><euid><fuid>`）：真机 bn19 命中 `nk4a: b33m 000081ed 0000 0000`——`mode` 低 16 位 `0x81ed`＝八进制 `0o100755`（S_IFREG + `rwxr-xr-x`，可执行位齐全）、`eff_uid`＝`0`（root）、文件属主 uid＝`0`。一个 root 执行自己-owned、`0o755`、可执行位齐备的普通文件，却被判 `EACCES`——权限判决本身有 bug。

### B33b 根因：`forbidden_decision` 的 access 必须是低 3 位，不是 9 位模式位

`forbidden_decision`（`protect.rs:229-261`）与 C `forbidden`（`minix3/minix/servers/vfs/protect.c:238-287`）逐行一致：先按身份算出 `perm`（root 为 `R|W|X` 或目录/任意可执行文件时取 `RWX_BITS`；否则 `(mode >> shift) & RWX_BITS` 取三元组的一档），末判据 `if (perm | access) != perm { Err(Acces) }`——它要求 `access` 是 `perm` 的**子集**，两者同在低 3 位（`R_BIT=0o4`/`W_BIT=0o2`/`X_BIT=0o1`）。而 `exec_worker.rs:670` 的调用点传的是 `access: 0o100`（=64，那是 9 位模式里 owner 段的可执行位、属于「文件模式位」域，不是「想要的访问」域）。root 的 `perm=0o7`，`0o7 | 0o100 = 0o107 ≠ 0o7` → 恒判失败 → 恒 `EACCES`。C 侧对应物：`forbidden` 的 `access_desired` 用的就是 `X_BIT` 低位（`protect.c:238-287`）。全仓另外 12 个 `forbidden_decision` 调用点（`main_loop.rs` 等）都正确传 `crate::open::X_BIT`，唯独 exec 开卷腿用错档位。既有 mock 测试 `test_pm_exec_open_phase_wires_and_bail`（path_len=0 → 空路径提前 `EINVAL` bail）从未到达此调用点，故缺陷未被捕获。

### 修复

`exec_worker.rs:670` 的 `access: 0o100` → `access: crate::protect::X_BIT`。为让「access 档位」这一 B33b 缺陷点直接进入单测射程（CodeReview #2：`access` 常量藏在内联 `ForbidInput` 构造里、既有单测均绕过三检门直调 `load_elf_segments`，改回 `0o100` 无测试变红），把这段带常量的构造与判决抽成纯函数 `exec_forbid_check(file_mode, file_uid, file_gid, real_uid, real_gid, eff_uid, eff_gid)`，`open_exec` 改调它。补两条回归测：`protect.rs::test_exec_access_bit_is_low_three_not_nine_bit`（钉被调方契约：同一 root 执行 `0o755`，`X_BIT` 放行、`0o100` 误拒）+ `exec_worker.rs::test_exec_forbid_check_uses_low_three_bit_access`（钉调用点：跨 root/owner 执行 0o755 放行、执行 0o644 拒绝，内部 `access` 改回 `0o100` 即测红）。

### B33a（符号掩盖，登记 follow-up，本单元不修）

`ExecError::to_errno()`（`exec.rs:586-594`）返回**正** errno，而错误车道各 `neg()` 站点（FS 腿）返回**负**；`main_loop.rs:1162` 把 `Err(errno)` 原样透传成正 status，INIT 成功车道吞成 EIO。真实 errno（本例 EACCES）被掩盖，只表现为 EIO，诊断困难。修复方向＝归一化 `pm_exec` 的 `Err` 臂为负（与 `perform_syscall` 契约 `m_type<0→Err` 对齐）。因它不是 marker 阻塞（真 blocker 是 B33b 的误拒本身），且改符号约定涉及面较宽、需全量核对调用方，本单元保持 diff 最小、登记为同族 follow-up。

### 真机验证（单核确定性测试台，§1.52 方法论）

无探针双跑 bn20/bn20b 签名一致：`Runcom` 从历史 15 轮循环塌成 **1 次**（exec 不再被 EACCES 拒绝、`/bin/sh` 首次真正获控），`bogus=0`（B32 伪指针未回归）、`eacces=0`（误拒消除）、`marker=0`、`panic-enter=2`。30068/30066 行。

### 三件套

mock vfs lib **533·0fail**（基线 531 + B33b 两条新回归测 `test_exec_access_bit_is_low_three_not_nine_bit` + `test_exec_forbid_check_uses_low_three_bit_access`，只增不减）；fmt `exec_worker.rs`、`protect.rs` 漂移 WT=0==HEAD=0 零新增；clippy exit 0；镜像重建 + 真机双跑 **bn21/bn21b 签名一致**（与 bn20/bn20b 同：Runcom=1、bogus=0、eacces=0、marker=0、panic=2、OOM-RT=1，行 30083/30086）——证实提取函数重构行为等价。两枚取证探针已按纪律 `git checkout` 回滚，工作树仅余 B33b 修复本体。

### CodeReview（本单元，target=exec_worker.rs+protect.rs 未提交改动）

**0 MUST-FIX、2 SHOULD-FIX 全采纳**：① 原修复注释「全仓其它 `forbidden_decision` 调用点均传 `crate::open::X_BIT`」为失真事实断言（grep 实为 main_loop.rs 11 处传 `crate::open::*` 的 W_BIT/X_BIT/W_BIT|X_BIT 混合，仅 1 处 X_BIT）——改为可核表述；② 原新增测只钉被调方 `forbidden_decision`、不钉调用点（改回 `access:0o100` 全套仍绿）——采纳其方案 a 变体：抽 `exec_forbid_check` 纯函数 + 调用点回归测（已上方「修复」段落实）。**1 CONSIDER 登记 follow-up**：`X_BIT` 在 `protect.rs`/`open.rs` 两份定义无交叉锁定（现同值 0o1），建议补 `assert_eq!(protect::X_BIT, open::X_BIT as u8)` 或将 `ForbidInput.access` 类型化为位集使 `0o100` 编译期不可构造（结构性改进，本 diff 不做）。CodeReview 附核：修复单值不携 R_BIT 与 C `exec.c:128 forbidden(...,X_BIT)` 忠实对齐（不扩大）；全仓 12 个 `ForbidInput{access}` 初始值经穷举无其它 9 位模式值残留。

### 下一前沿（按序）

① **头号＝B34 运行期堆 `OOM-RT`**：panic 现场 `nk4c: OOM-RT size=001000 slabs=008/400 big=20/20 px=094/400 fp=03f/400`——`/bin/sh` 获控后执行 rc 的真实工作集撑爆固定内存池（`big=20/20` 满），与 §1.55 的 B29（VFS 堆 OOM）同签名，属已登记结构债「VM-backed heap supplier / `MAX_BIG_BLOCKS`」家族复现。这是执行深度推进（B33b 让 sh 真跑起来）必然暴露的下游容量问题，非本单元权限改动引入。② B33a 符号归一化；③ init sleep 腿疑似不睡；④ VM PREALLOC 语义缺口裁决；⑤ SMP 早期竞态（§1.52）；⑥ §1.51 遗留。

---

## 1.59 B34 根因实锤（big-block 记录表 premature-OOM，非池耗尽）+ 朴素修法回归 boot（已回退、未提交，待结构裁决）（取证单元，含对照实验）

### 一句话

B34 的 OOM-RT 打印值是**十六进制**：`big=20/20`＝`0x20/0x20`＝**32/32**（`MAX_BIG_BLOCKS` 记录表满），而 `px=094/400`＝`0x94/0x400`＝**148/1024 空闲页仍在**——即不是池字节耗尽，而是 `minix-rt` 分配器的 `big_blocks` 跟踪表（硬编 32）先于池字节填满，造成 premature-OOM。这正是 `alloc.rs` 早年登记的「big-block-heavy server 到来时 revisit」同型缺陷（与已修的 `MAX_SLABS` 一类）。把它按 `MAX_SLABS` 纪律绑到池页数（`32→GLOBAL_POOL_PAGES`）后，**真机确定性地从 30090 行崩回 124 行**（VM 服务器早期 `pagefault in VM`）——因分配器静态实例变大平移了 VM 的 .bss/用户栈基址，撞上 boot **未 eager 物化初始栈/.bss 尾页**的缺口（§1.55 已登记的 PREALLOC 只记位不 `ev_new`、demand-fill-on-first-touch 分歧）。故 B34 的正解被反复 defer 的结构债（boot eager 物化 / VM-backed heap supplier）挡住，属**架构裁决级**，本轮不提交回归、保留取证，待方向确认。

### 根因链（静态实锤，`os/libs/minix-rt/src/alloc.rs`）

- `alloc_big(size)`（L523-546）：先 `supplier.supply_pages(pages)` 从池拿页（**成功**，bn20 现场 px=148/1024 空闲页充裕），再在 `big_blocks[0..MAX_BIG_BLOCKS]` 里找空记录槽；找不到就 `release_pages` 归还刚拿的页并返 null → 上层 OOM。**约束在记录表，不在池字节**。
- `MAX_BIG_BLOCKS: usize = 32`（旧值）；`4 KiB` 单页分配走 `alloc_big`（`class_for(4096)` 因 `MAX_SLAB_OBJECT_BYTES=2048` 落 None）。sh 运行期并发 big block 超 32 → 表满 premature-OOM。
- 与 `MAX_SLABS`（L65，已＝`GLOBAL_POOL_BYTES/PAGE_BYTES`）严格同类：每个 big block ≥1 页，故并发上限＝池总页数，表应绑到 `GLOBAL_POOL_PAGES`。

### 已试修法（存于 `stash@{0}`，未提交）与真机回归

改动本体（2 行 + 文档 + 一测）：
- `alloc.rs:75` `MAX_BIG_BLOCKS: usize = 32` → `= GLOBAL_POOL_PAGES`；
- `lib.rs:161,163` OOM 打印 `big` 分子/分母宽度 `2`→`3`（否则 `0x400` 被截成“00”，污染签名对账，同 L154 注释对 slabs 的警告）；
- 新增回归测 `test_big_block_table_tracks_pool_pages_not_capped_at_32`（48 页池连分 40 个单页 big block，旧码第 33 个即 null）。静态三件套绿（minix-rt mock 58·0fail / fmt alloc.rs+lib.rs WT0==HEAD0 / clippy exit 0）。

**真机确定性回归**（同镜像连跑）：
- bn22：`lines=126`、Runcom=0、panic=2、OOM-RT=0——**没到 Runcom 就死**。现场：`pf rip=0x2346db err=0x6 cr2=0x7ffffffeef88 walk=NP`（紧邻 rsp、非 present+write+user）→ `trap_dispatch.rs:1059 panic!("pagefault in VM")` → vector 13 GP @`rip=0x1dc73e79` 递归 panic。
- bn22b：`lines=124` 同签名（确定性）。
- **对照实验 bn22ctl**（`git stash` 掉本改动、重建、同镜像）：`lines=30090`、Runcom=1、panic=2、OOM-RT=1——**与 bn20/bn21 一致**，坐实回归由本改动引入。

### 机制定性

`GlobalAllocator` 是每服务器的 `static`（`lib.rs:239`，内含 `SlabAllocator` 的 `slabs[1024]`+`big_blocks[N]`+`FixedPoolSupplier.free_pages[1024]`）。`big_blocks` 从 32→1024 使该静态约 +16KB，平移了 VM 镜像的段/用户栈布局；而 boot 对 VM 自身镜像是 demand-fill-on-first-touch（未 eager 物化），VM 在启动初期用到那个新落入未映射页的栈/.bss 地址时缺页，而 VM 自己是缺页服务者→无法服务自身→递归 panic（C `exception.c:101-118` 忠实行为）。即：**不是容量不够，是 boot 未把 VM 自身的栈/.bss 尾页预先映好**，一有尺寸扰动就暴露。

### 为何停下（架构裁决级）

三个修法方向各有权衷，且都触及 boot/VM 内存契约（需 [ARCH] 三处一致），反复被 defer：
- **A（boot eager 物化）**：给 boot/VM 的初始镜像段与初始栈 eager 预映（补 `ev_new`/PREALLOC 真预分配，对位 C `region.c:492-499`），让尺寸扰动不再触发自缺页。直接、且是 §1.55 登记分歧的根治，但改 boot 映页路径。
- **B（VM-backed heap supplier）**：真正实现 C `brk`/`VM_BRK` 按需长堆，堆不再受固定 `.bss` 池限（最大单元，之前定性为“客户端 supplier=大单元”）。
- **C（保守绑界）**：`MAX_BIG_BLOCKS` 只抬到不跨未物化页的值（如 64）+ 修 VM 自栈缺页；能清当前 32/32 但仍留脆性、非原则解。
本轮采“不提交回归 + 保留取证”；改动存 `stash@{0}`（可 `git stash pop` 恢复），待用户定 A/B/C 方向。rc marker 仍未达（bn20 系列 30090 行、OOM-RT 非唯一卡点，深度推进后另有下游）。

---

## 1.60 B34 采 C 方案落地（`MAX_BIG_BLOCKS` 保守绑界 64）→ 翻出 B35 `supply_pages` 碎片化墙（含代码·minix-rt）

### 为何 C 不算架构裁决、可直接提交

§1.59 的三方向里，**A（boot eager 物化）改 boot 映页路径、B（VM-backed heap supplier）改 VM `brk`/长堆契约**——两者都触及 boot/VM 内存外部契约，需 [ARCH] 三处一致，确属架构裁决级停点。但 **C（把 `MAX_BIG_BLOCKS` 抬到不跨未物化页的保守值）是纯分配器内部静态数组尺寸 + 一条回归测**，不改任何外部行为，与 §1.55 的 `GLOBAL_POOL_BYTES` 512→1024、`prepare_boot` 128→1024 **同属“容量 round”先例**（那两个 round 都是直接提交的）。故本轮自主采 C 落地，A/B 作为「原则解仍待裁决」继续挂着（真正逼出 A/B 的是下面的 B35 与未来更高 big-block 峰值）。

### 落地：`MAX_BIG_BLOCKS` 32→64（非 1024）

`git stash pop` 恢复 §1.59 改动后，把常量从试验性的 `GLOBAL_POOL_PAGES`（1024）改为 **64**。64 的依据：真机 §1.58/本轮观测 big 峰值＝47（`0x2f`），64 覆盖之且留余量；且每个 `Option<BigBlock>` 槽在 x86_64 上是 **24 B**（`*mut u8` 无 niche→`Option` 需独立 discriminant，实测 `size_of` 确认），(64−32)×24 B＝**+768 B < 一页**，不跨未物化页。同步加一条编译期不变量 `const _: () = assert!((MAX_BIG_BLOCKS-32)*size_of::<Option<BigBlock>>() < PAGE_BYTES)`，把「增长 < 一页」锁进构建——未来谁把该常量调大到跨页会直接编译失败，而非退化为 §1.59 的启动 panic。回归测 `test_big_block_table_not_capped_at_32`（48 页池连分 40 个单页 big block，旧 32 上限在第 33 个返 null）钉住「>32 不 premature-OOM」。

### 真机双跑：premature-OOM 消除、boot 零回归、推进近 2 倍

`-m 512M -smp 1` 无探针双跑 **bn23/bn23b 签名一致**（58507/58511 行、pre-restore 5681/5682）：**`pagefault-in-VM=0`**（关键：+768B 未跨未物化页，§1.59 的 1024 版崩于 124 行的形态彻底消失）、EXIT=124（boot 自然跑到超时、非早期 debug-exit）、`Runcom` 与 init/rc 路径与 bn20 同位（25546）后**继续推进到 57715 才 OOM**（bn20 是 29010 行即 OOM）。**big 记录表 32/32 premature-OOM 已根除**。

### 新头号前沿 B35：`supply_pages` 只走 bump 游标、1 页 big 请求不查 free-stack

OOM-RT 换签为 `big=2f/40 px=400/400 fp=39c/400`（十六进制：`big=0x2f/0x40`＝47/64 **表未满**、`px=0x400/0x400`＝pages_consumed 1024/1024 **bump 游标耗尽**、`fp=0x39c/0x400`＝free_pages **924 单页搁浅**）。即：一次 `size=0x1000`（1 页）big 请求失败，**不是因为没页**（free-stack 有 924），而是 `alloc_big`→`supply_pages` 只从 bump 区发连续 run（`next_page+pages<=total`，L360-376），**从不查 `supply_page` 的单页 free-stack**。这正是 §1.55 记录的「free 栈只养单页、run 只从 bump 区发」碎片的同族再现——是 B29 家族的真实结构墙（非表容量）。下一取证配方：CodeReview 提的 `alloc_big` 失败路径 diag（区分「表满」vs「游标尽而 free-stack 有货」），再判修向（1 页 run 委托 `supply_page`？还是原则解 A/B）。

### 三件套 + CodeReview

静态三件套绿（mock minix-rt **58·0fail** 基线+1 只增不减 / fmt alloc.rs WT0==HEAD0 零新增漂移 / clippy exit 0 无新增警告 / `MAX_BIG_BLOCKS` 值与 bn23/bn23b 双跑一致——本轮在双跑后仅改注释+零运行时影响的编译期断言，二进制行为不变，另重建镜像确认断言在 `x86_64-unknown-none` 目标编译通过）。**CodeReview 0 MUST、1 SHOULD 采纳**：`Option<BigBlock>` 每槽尺寸断言 16B 有误→实测 24B，doc 数值 `+512B→+768B`、`~16KB→~24KB` 全部据实修正并锁进编译期不变量；1 CONSIDER（`alloc_big` 加表满/池空区分 diag）留作 B35 取证面。rc marker 仍未达。详见 §1.59/§1.60。

---

## 1.61 B35 `supply_pages` 不查 free-stack 碎片修复（单页 big block 委托 `supply_page`）→ 真机首次 OOM-RT=0·panic=0·推进 72495 行 → 翻出 B36 rc 15 轮循环（含代码·minix-rt）

### 根因（静态+签名双坐实，无需探针）

§1.60 把 B34 停在保守绑界 64 后，真机 bn23 OOM 换签为 `size=001000 … px=400/400 fp=39c/400`（十六进制：`size=0x1000`＝**恰好 1 页**、`px=0x400/0x400`＝bump 游标 1024/1024 **耗尽**、`fp=0x39c/0x400`＝free-stack **924 单页搁浅**）。根因＝`FixedPoolSupplier` 两供货接口分工失衡：`supply_page`（单页）**先弹 free-stack 再推 bump**，`supply_pages`（run）**只走 bump、从不查 free-stack**（其注释自陈 free-stack「reserved for single-page requests」）。而 `add_slab`→`supply_page`，`alloc_big`→`supply_pages(size.div_ceil(PAGE_BYTES))`——于是一个 4 KiB（1 页）的 big-block 请求走 `supply_pages(1)`，游标耗尽后即使 free-stack 有 924 可复用单页也返 null。即「freed 单页永不被 big-block 路径复用」，与自身注释意图相悖＝逻辑缺陷，非池字节耗尽。

### 修：`supply_pages` 对 `page_count==1` 委托 `supply_page`

在 `supply_pages` 开头加 `if page_count == 1 { return self.supply_page(); }`。1 页 run 无邻接要求（trivially 连续的 1 页），契约正确；且 `free()` 归还大块走 `release_pages(p,1)`（默认逐页 push 回 free-stack），与委托路径对称；零化（`supply_page` 两分支均 `write_bytes`）与旧 `supply_pages(1)` 等价。多页 run 仍只走 bump（散页无法保证邻接）；coalesced free list / VM-backed heap 是延后的结构修（已在注释登记）。对照：Linux `__alloc_pages` order-0 / Redox buddy order-0 / Minix3 per-page cache 均优先从 free list 弹单页，**邻接约束仅对多页 run 存在**——本修法是最简实现。新测 `test_single_page_big_block_reuses_free_stack`（4 页池填满 bump→释放 1 页→再取 1 页 big block 必成功；旧码此时 `4+1<=4` 必返 null）。

### 真机双跑：OOM-RT 彻底归零、panic 6→0、历史最深推进

重建镜像后 `-m 512M -smp 1` 双跑 **bn24/bn24b 签名一致**（72495/72494 行、pre-restore 7209/7209）：**`OOM-RT=0`（上轮还有 1）、`panic=0`（此前 6 次全消）、`pagefault-in-VM=0`、EXIT=124**。boot 从 bn23 的 58507 推进到 **72495 行（历史最深）**。free-stack 与 big-block 路径打通后，之前因一次 1 页拷贝失败引发的 OOM→panic 链彻底消失。

### 新头号前沿 B36：rc `Runcom` 15 轮循环、marker 仍未出

OOM 掩盖去除后暴露 `Runcom=15`（每 ~3150 行重入 `nk4a: init-state Runcom`，与 bn20 的 Runcom=1 不同）、`marker=0`——即 §1.55 描述的 **rc 15 轮循环**形态重现：init runcom 反复重入因外部命令（marker echo）仍未跑完，**但现在不是 OOM、而是 B31/B32/B33b 修后的下游新失败因**。需下一单元探针取证（runcom→exec→echo 腿现有哪步非 OOM 地失败）。bogus=0（B32 伪指针未再现）。

### 三件套 + CodeReview

静态三件套绿（mock minix-rt **59·0fail** 基线+1 只增不减 / fmt alloc.rs WT0==HEAD0 / clippy Finished 无新错 / 镜像重建+真机双跑 bn24==bn24b 签名一致）。**CodeReview PASSED：0 MUST、0 SHOULD**（逐项核过邻接契约/supply-release 对称/零化语义/游标无双重计数/multi-page与 slab 零回归/测钉不变量/hex 解码/与 Linux-Redox order-0 对照）。**注**：本修法只解 1 页 big 请求；多页 run 仍 bump-only（碎片未治），真正原则解仍是 A(boot eager 物化)/B(VM-backed supplier)，现因单页路径已足撑当前工作集而未触发。rc marker 仍未达。详见 §1.60/§1.61。

---

## §1.62 B36 取证闭环——rc marker 阻塞真身：exec 回复携带正值 errno（+45＝EOPNOTSUPP）被 exec_via 成功车道吞成 EIO（B33a 实锤·纯取证 doc·探针已回滚）

§1.61 修掉 B35 后，真机首次 **OOM-RT=0、panic=0**、推进到 72495 行（历史最深），随即翻出 **B36**：init 状态机 `Runcom` 重入 15 轮、`marker=0`。本轮通过三枚真机探针把 B36 逐层收窄到一条确定性结论——**rc 起的 `/bin/sh` 从未 exec 成功，其 exec 系统调用被客户端车道把真实错误码掩盖成了 EIO**。这是 §1.58 登记的 **B33a 符号掩盖** follow-up 的实锤闭环。本节为纯取证，未改任何生产码，探针全部回滚、工作树干净、HEAD 仍 `1d78948a4`（模式同 §1.59 `bad22e56c`）。

### 探针链与真机证据

1. **退出码探针（bn25p）**：在 `runcom.rs` 判定 rc 子进程退出的地方打印 `WaitStatus`。捕获 **15 个 rc 子进程全部 `Exited { code: 5 }`**。code 5 来自 `runcom.rs:147-152` 的 exec 失败腿——子进程 `host.exec(&cmd)` 返回后无条件 `stall(...)` + `exit_process(5)`，即 **exec 这一步失败**，脚本（连同其中打印 marker 的 `echo`）从未跑起来。
2. **errno 探针（bn26p）**：在 exec 失败腿把 `host.exec` 的返回码打出来。捕获 `nk4a: b36 exec-fail err=5 disp=EIO argv=["sh","/etc/rc","autoboot"] path=/bin/sh` ×15——`host.exec` 恒返 **EIO(5)**。
3. **区分探针（bn27p）**：EIO 有多个可能来源，须定位是哪一条腿。`exec_command`（`execve.rs:269-313`）按序走 `new_image_stack_top`（kerninfo 查询，失败则 L195-215 四站点返 EIO）→ `prepare_exec` → `exec_via`。给 `new_image_stack_top` 失败腿和 `exec_via` 的 `perform_syscall` 两条臂（`pm.rs:593-596`）各插一枚打点。真机捕获 **`nk4a: b33a exec_via-OK mtype=45` ×45，零 `stack_top-fail`、零 `exec_via-ERR`**。

### 结论：掩盖点坐实 + 真错误码浮现

- **kerninfo 腿清白**：`new_image_stack_top` 全程成功（无 `stack_top-fail`）——§1.54 修好的 kerninfo 交付链在当前 boot 正常，EIO 不来自它。
- **掩盖点＝`exec_via` 成功车道**：`perform_syscall` 返回 `Ok(45)`——因为消息 `m_type` 等于 **45（非负）**，命中 `pm.rs:594` 的 `Ok(_) => Errno::EIO`，把真实值 **丢弃、换成 EIO**。这正是 B33a「成功车道吞掉错误」的机制（`perform_syscall` 契约：`syscall.rs:107` `m_type < 0 → Err(errno)`，否则视为成功 `Ok`）。
- **真错误码＝45＝EOPNOTSUPP**（`errno.rs:59`）：**exec 链给客户端回了正值 45**（Operation not supported），而不是负 errno。按 C 黄金参照，`_syscall`（`execve.c` 走的就是它）同样把正 `m_type` 当成功——所以 **根子在服务端回了错误码却忘了取负**，客户端的 `Ok→EIO` 只是二次掩盖。真错误本身（为何对 `/bin/sh` 的 exec 报 EOPNOTSUPP）是下一层。

### 定性：B36＝B33a（符号掩盖）＋其下的 EOPNOTSUPP，两层

- **Layer 1（B33a·已实锤）**：exec 的真实 errno（此处 45）以正值回流，被 `exec_via` 的 `Ok` 车道吞成 EIO。修复方向＝归一化 exec 回复符号，让真实 errno 从 `Err` 车道浮出（对齐 C 的「失败回 `-errno`」约定），而非继续用 EIO 掩盖。
- **Layer 2（真错误·待追）**：exec `/bin/sh` 实际返 **EOPNOTSUPP**。VFS 侧 `NotSup` 是 dispatch 的保留拒绝位（`open.rs:558`、`read_write.rs:454`），常规文件不该命中；需下一单元顺 PM exec 回复组装点（`exec.rs` 的 `svc.reply(.., result)` 之 `result` 来源）与 VFS exec 各 bail 腿二分，定位是谁把 EOPNOTSUPP 填进了回复。

### Runcom 15 轮循环非回归

`Runcom=15` 不是新缺陷，而是 OOM 崩溃截断消失后 **rc 循环首次完整可见**：每轮 `exec /bin/sh` 失败 → `exit(5)` → 父判非零退出 → `Attempt::SingleUser` → 状态机 `ProceedRuncomFastboot` 回 `Runcom`（`driver.rs` 状态迁移），如此 15 次直至超时。此前 §1.55 的 15 轮由 OOM 掩盖、§1.58 的 `Runcom=1` 因 B33b 让 `/bin/sh` 短暂获控但随即被 B34 OOM 崩溃截断——B35 清掉 OOM 后，命令面这条 exec 腿的真实失败（EOPNOTSUPP 被掩盖成 EIO）成了挡 marker 的头号前沿。

### 下一前沿与单元收尾

**新头号前沿＝B33a 修复单元**：先追 Layer 2 定位 EOPNOTSUPP 产生腿（决定是纯客户端符号归一化即可、还是服务端 reply 也需改），修后 rc marker 才能浮出真错误乃至成功。触及 IPC 符号约定，须对照 C `execve.c`/`_syscall` 回复约定 + Ground Truth 优先链 + CodeReview + 三件套 + 真机双跑。本取证单元：三探针（runcom.rs×2、pm.rs、execve.rs）全部 `git checkout` 回滚、工作树干净、HEAD 未动，无生产码改动故测基线不变。rc marker 仍未达。详见 §1.61。

---

## §1.63 B33a 修复——pm_exec 边界单点折负，去 exec_via EIO 符号掩盖（含代码·VFS 侧）

承接 §1.62 取证，本单元落 **Layer 1（B33a 符号掩盖）的修复**。Layer 2（真 errno 本身，见下修正）作为新前沿续追。

### 根因精确化（bn28p/bn30p 修正 §1.62 的 45 读数）

§1.62 依 bn27p 记「真错误码＝45＝EOPNOTSUPP」。后续 bn28p 双探针（VFS `main_loop` pm_exec Err 臂 + PM `exec_restart` 入口）与修后 bn30p init 侧 `exec_via` 返回码探针一致读数 **errno=8＝ENOEXEC**（非 45）——45 系前一轮另一 exec 目标或探针 cap 截断下的异读，本轮多点交叉以 **ENOEXEC** 为准。**掩盖机制不变且与真值无关**：`pm_exec` 回**正值** errno → PM `exec_restart` 原样写进回复 `m_type`（正）→ init `perform_syscall`（`syscall.rs:107` `m_type<0→Err`）判非负走 `Ok` → `exec_via`（`pm.rs:594`）`Ok(_) => EIO` 吞成 EIO。无论内层是 8 还是 45，只要以正值回流就被吞成 EIO、真值永不浮现。

### Ground Truth 对照

C `minix3/minix/servers/vfs/exec.c:401` `pm_exec` **正返回** `ENOEXEC`；C `minix3/minix/servers/pm/exec.c:156-170` `exec_restart` 原样 `reply(rmp-mproc, result)`。即 C 的 pm_exec 本身回正值，符号折叠发生在 C libc 边界。minix-rs 未沿用该形态，而是确立「**负 errno 入 `m_type`**」为全项目 IPC 约定（`perform_syscall` `m_type<0→Err`；§B16 既往已在 PM fork 回复点 `-PmError::to_errno()` 取负）。本修沿用该约定：exec 失败须以负 errno 出 `pm_exec`。

### 修复：公共边界单点折负

`exec_worker.rs` 体内约 15 条静态失败腿符号**混合**——IPC 往返腿（`vm_mmap`/`vm_procctl_clear`/`load_elf_segments`/`pm_newexec`/`req_read`）已用私有 `neg()`（L88）折负，但 `enoexec` 裸值（`.ok_or` L273/275/277、`bail!` L351/360/372）与 `ExecError::*::to_errno()`/`ElfHead::parse` 正值腿（L169/178/182/188 等）泄漏正号。逐腿补 `neg()` churn 高且易漏。采**公共边界单点折负**：原 `pub fn pm_exec` 更名 `fn pm_exec_inner`，新增同名 `pub fn pm_exec = pm_exec_inner(...).map_err(neg)`。`neg()` 对已负腿幂等（`v<0` 原样返），故不动既有正确腿；正值静态腿统一折负。`ExecErrno` 契约（L151「负 errno 直通 PM」）与 `main_loop.rs:1153` 注释（「失败为负 errno」）自此真实成立，`main_loop` 侧无需改动、原样透传即可。

### 三件套 + CodeReview

- **mock**：`cargo test -p minix-vfs --lib` **534 passed/0 fail**（基线 533 **+1**：既有两测 `test_pm_exec_frame_gate_enomem`/fetch 门已断言 `Err(neg(..))`，本单元把固化正泄漏的 `test_pm_exec_rejects_dead_target` 改为断言 `Err(-ENOEXEC)`，新增 `test_pm_exec_boundary_folds_positive_errno_negative` 显式 pin 边界折负不变量）。
- **fmt**：nightly rustfmt `--check` exec_worker.rs **0 Diff**（WT 全格式化）。
- **clippy**：改动区间零新增 warning（存量 23 条系他文件）。
- **真机**：重建镜像 `-m 512M -smp 1` 双跑 **bn29/bn29b 签名一致**（73081/73096 行、panic=0/oom=0/runcom=15/marker=0，boot 零回归、比 bn24 的 72495 略深）；**bn30p 临时探针**（init `exec_via` 返回码，跑后 `git checkout` 回滚）捕获 **`exec_via errno=8 name=Some("ENOEXEC")`×45**——EIO 掩盖端到端消除、真 errno 浮现的决定性证据（45＝15 rc 子×每子 cap 3，fork 拷贝 .data 使计数器逐子归零）。
- **CodeReview 子代理**：**0 MUST-FIX / 1 SHOULD-FIX 已采纳**（`pm_exec_inner` doc 原「失败返回负 errno」与新增「静态腿返回正值」自相矛盾 → 改为「失败返回 mixed-sign errno（正值静态腿由 `pm_exec` 边界统一折负）」）。

### 新前沿＝B37（Layer 2：ENOEXEC 根因）

B33a 去掩盖后，真错误 **ENOEXEC(8)** 诚实浮现，但 `/bin/sh` 仍装载失败、marker 仍未出。readelf 实测 `/bin/sh` 为**静态链接 ELF EXEC、无 PT_INTERP**（非脚本、非 dyn），故 ENOEXEC 来自 `pm_exec` 更深的装载 bail 腿而非脚本/dyn 后置分支（L351/360）。下一单元定位具体 bail 腿：`ElfHead::parse` 校验（魔数/e_type/phoff 界 L167-189）、全 phdr 预检 `p_offset+p_filesz > v_size`（L371，最可疑——imgrd 播种的 sh 首块/段数据与实际镜像读边界）、或 `open_exec` 的 stat/首块读腿。探针/读码二分定位后修向方能让 `/bin/sh` 真 exec、marker 浮出。rc marker 仍未达，frontier＝§1.63。

> 【§1.64 翻案】本节初拟的「首疑 phdr 预检 / phdr 表超 512」**已被四探针推翻**——真根因更上游：exec 首块 FS 读根本没把字节读进 hdr_buf。详见 §1.64。

---

## §1.64 B37 取证翻案——真根因＝exec 首块 FS 读（REQ_READ→VFS-self grant）返 Ok 却零字节落入，非 phdr 界（纯取证 doc·探针已回滚）

§1.63 尾把 B37 头号嫌疑定在 `ElfHead::parse` 的 phdr-界校验（`e_phoff+e_phnum*56 > SECTOR_SIZE`，readelf 实测 /bin/sh＝10 phdr→624>512）。本单元四枚真机探针**推翻该假设**、把根因前移到更上游的**首块数据读取**：缓冲区拿到的是全零，`ElfHead::parse` 在**魔数腿**（L178，早于 phdr 腿 L187）就 bail ENOEXEC——所以我的 phdr 探针根本不触发。取证链（均 `nk4a:` 前缀·`#[cfg(not(feature="mock"))]`·AtomicUsize cap·跑后全回滚）：

1. **bn31p**（在 L187 phdr 界打点）：**无** `b37 phdr-bound` 输出（grep 命中的 `b37` 是字节串巧合）→ phdr 界腿未触发。
2. **bn32p**（在 `ElfHead::parse` 入口打 `buf.len()`+首 4 字节）：捕获 **`parse-enter len=40960 b0..3=00 00 00 00`**——hdr_buf 全零（非 `7f 45 4c 46`），坐实魔数检查失败才是 bail 腿。
3. **bn33p**（在 open_exec 首块读前打 `v.size`/`hdr_len`）：捕获 **`firstblock ino=8 vsize=131968 hdr_len=40960 fs=Endpoint(10)`**——vnode size **正确**（＝/bin/sh 真实字节数 131968，证明 imgrd 播种含文件内容、非「size=0 早退」）、hdr_len 非零、FS 端点 10。
4. **bn34p**（在首块 `req_read` 之后打 hdr_buf 内容 + Err 分支）：捕获 **`after-read hdr[0..8]=00 00 00 00 00 00 00 00 hdr_len=40960`** 且**无** `req_read-ERR`——即 req_read 向 FS 发 REQ_READ 返回 **Ok**（`fs_trans_status==OK`）、无错误，但读后 hdr_buf 仍**全零**。

### 定性结论（不越位声称修法）

exec `/bin/sh` 报 ENOEXEC 的**真根因**＝`open_exec` 的首块读（`req_read(who_from=Endpoint::SELF)`：对 VFS 自身 hdr_buf 建 magic grant→发 REQ_READ 到 FS Endpoint(10)→revoke）**返回成功却没有把文件字节拷进 grant 目标**——FS 侧要么未把数据 virto-copy 回 VFS-self 缓冲、要么大块（40960 跨 10 页）grant 拷贝路径有缺口。这**不是** ELF 布局/phdr 计数问题（那些在数据落地后才相关）。

**关键对照**：init runcom 能成功读 `/etc/rc`（否则解析不出 `echo` 命令、走不到 exec）→ FS→VFS 读路对**小文件**可用；故缺口特定于 exec 首块的**大块 grant（40960）写回 VFS-self 地址空间**这条路径，或 VFS→FS `req_read` 大块分页读的实现。下一单元顺此定位：读 FS（MFS Endpoint(10)）`REQ_READ` 处理腿的 grant 写回机制 + `grant_magic(who_from=SELF)` 建权语义，对照 C `map_header`（exec.c:736-763）用 `VFS_PROC_NR` 自读语义；先确证「FS 是否真收到 REQ_READ 并读到块、拷往哪个端点」再定修向（不预设结论）。本取证单元：四探针全 `git checkout` 回滚、工作树干净、HEAD 未动（`9714fe40d`），无生产码改动故测基线不变。rc marker 仍未达。

---

## §1.65 B37 修复——exec 首块读的两个堆叠缺陷（B37a grant `who_from` 哨兵 + B37b LP64 phdr 界）（含代码·VFS 侧）

承 §1.64 取证，本单元把「exec 首块 FS 读返 Ok 零字节」一路追到底，发现是**两个堆叠缺陷**（修掉前一个才会暴露后一个），修好后 `/bin/sh` 首次真正装载运行、rc 15 轮循环崩解为 1 轮。

### B37a：VFS-local magic grant 的 `who_from` 用错哨兵（真根因，证实 §1.64 假设）

静态追 FS REQ_READ 写回链坐实机制：FS 侧 `copy_out`（`os/fs/fs-rt/src/transport.rs:268-273`）经 `ipc.copy_to(self.peer, self.grant, offset, bytes)` → `sys_safecopyto`；内核 `verify_grant` 对 magic grant **把 `magic.who_from` 原样装为 `effective_granter`**（`os/kernel/src/grant.rs:461`），随后数据拷贝用该端点的页表解析 `start`。而 `open_exec` 首块读（`req_read(..., who_from=Endpoint::SELF)`）与 stat 用 `grant_magic(who_from=Endpoint::SELF.get(), ...)`。`Endpoint::SELF` = `ENDPOINT_SLOT_TOP-3` 是哨兵（`is_valid()==false`），非任何真实进程 → 内核拿它解析页表必失败 → 而 FS 侧 `copy_out` 吞掉拷贝错误（`let _ =`）→ `hdr_buf` 一字节未落、`req_read` 仍回 `Ok`。**这正是 §1.64 `after-read hdr[0..8]=00 00 00 00` 的机制**。关键对照亦闭合：正常 `/etc/rc` 读走 `grant_user_buffer(user_e=具体进程)`（`main_loop.rs:7291`），who_from 是具体端点故可用；段装载读 `load_elf_segments` 用 `target_e`（目标进程具体端点，exec.c:709-711）本就正确。**唯 `Endpoint::SELF` 两处错**（首块读致命、stat 因只消费成败位被掩盖）。

修＝引入 `const VFS_LOCAL_WHO_FROM: Endpoint = Endpoint::VFS`（`Endpoint(1)`，合法具体端点，对齐 C `map_header` 的 `VFS_PROC_NR`，exec.c:755），替换 stat（L759 区）与首块读（L800 区）两处 `Endpoint::SELF`。`sys_datacopy` 里的 `Endpoint::SELF`（内核自解析 caller，syscall_clock.rs 有 SELF 替换）是不同路径，不受影响、未动。

### B37b：LP64 phdr 表界遗留 32 位假设（修好 B37a 后暴露的下游独立腿）

B37a 修复后临时探针 `bn37p`（`open_exec` 首块读后打 `hdr_buf[0..8]`）实锤 `b37b hdr0..8=7f454c46 02010100`＝**ELF 魔数已落入**（证明 B37a 修对了），但 `bn36p` init 侧探针仍捕获 `exec_via errno=8 ENOEXEC`×45——同一 ENOEXEC 来自**更下游的一条腿**（§1.64 曾推翻的 phdr 假设，在字节能落入后**复活为真**）。定位＝`ElfHead::parse` phdr 界 `e_phoff + e_phnum*56 > SECTOR_SIZE(512)` → ENOEXEC。C `elf_sane`（exec_elf.c:34-38）用 `SECTOR_SIZE` 是因 C 编译于 32 位时代（`__ELF_WORD_SIZE 32`、`check_header` 用 `sizeof(Elf32_Phdr)`=32B），32 位 phdr 表天然 ≤512；minix-rs 是 LP64（`e_phentsize`=56），readelf 实测 `/bin/sh` e_phoff=64、e_phnum=10 → 表 624>512 被误拒，但 `map_header` 本就加载 10 页（40960）缓冲，表完全装得下。C `elf_unpack`（exec_elf.c）里一条被 `#if 0` 停用的 `phdr + phnum >= hdr_len` 检查，正是「按加载缓冲长校验」的设计本意（32 位下 SECTOR_SIZE 够用故被注释）。

修＝phdr 界从 `SECTOR_SIZE`(512) 改为实际加载头缓冲长 `buf.len()`，删 `SECTOR_SIZE` 常量（仅注释保留历史解释）。越界读安全性：parse 守 `e_phoff + phnum*56 ≤ buf.len()`，`phdr(i)` 最大索引 `e_phoff + phnum*56 - 9 ≤ buf.len() - 9`，无越界（CodeReview 已核）。此为 LP64 适配性 bug 修复，不改外部 IPC 契约、 preserves「装载合法 LP64 ELF」的外部行为，非 [ARCH] 裁决（与 NS5-A MessMmap 64 位车道同族）。

### 三件套 + CodeReview

- **mock**：`cargo test -p minix-vfs --lib` **537 passed/0 fail**（基线 534 **+3**：`test_vfs_local_who_from_is_concrete_endpoint`（常量具体性 + `is_valid()` 不变量）、`test_req_read_first_block_wires_with_concrete_who_from`（驱动 `req_read` 全腿到 IPC 线形状）、`test_elfhead_parse_lp64_phdr_table_beyond_sector_size`（624B phdr 表合法通过 + 真越界仍拒））。既有 `test_elfhead_parse_accepts_static_and_rejects_broken` 的 `e_phoff=600` 拒绝腿仍成立（120B 测试缓冲 < 600），注释按新语义修正。
- **fmt**：nightly rustfmt `--check` exec_worker.rs **0 Diff**。
- **clippy**：改动区间零新增 warning（存量 23 条系他文件）。
- **真机**：`-m 512M -smp 1` 无探针双跑 **bn38/bn38b 签名一致**——`runcom` 15→**1**（rc 循环崩解、`/bin/sh` 首次装载运行）、`ENOEXEC`=0（端到端消除）、`panic`=0、`oom`=0；LINES 858099/849854 差异系活锁诊断 spam 密度非签名漂移。取证期临时探针（init execve.rs `exec_via` 返回码 bn36p、exec_worker.rs 首块 `hdr[0..8]` bn37p）跑后全部 `git checkout`/整段删除回滚，工作树仅 `exec_worker.rs` 生产改动。
- **CodeReview 子代理**：**PASSED 0 MUST-FIX / 0 SHOULD-FIX**（逐项核：who_from 全仓无遗漏 SELF、phdr 界改 buf.len() 无 OOB、无其它 Elf32 遗留、6 测真正钉住不变量）。

### 新前沿＝B38（exec 成功后上下文切换活锁）

`/bin/sh` 装载成功（runcom 15→1）后，日志翻为 85 万行，**全是内核逐上下文切换诊断探针 spam**（`nk4a: pick/sa0/sa1/cr3/gs2/probe/pre-restore`，系 HEAD 既有诊断，非本单元引入），两进程页表根 `0x2149000`/`0x2c26000` 各对敲 ~4.6 万次/150s = **紧密活锁**，`pre-restore rip=0x2016d6` 高重复，marker 仍未出。下一单元＝判明 `0x2016d6` 属哪个 ELF（addr2line 用户态 staging/init 或 sh）、哪两进程在敲（init↔PM↔VFS↔sh 的哪条 IPC 环）、是否 B31/B32 家族外部命令 exec（echo）腿的下游再现。rc marker 仍未达，frontier＝§1.65。




