# edge3 — 服务器 · FS · net · 命令线（三线并行之三）

> **定位**：临时分工索引之三。条目权威描述在原 todo 文件（每条附链接）。归档规则见 [edge4.md](edge4.md) §8。
>
> **所有权（本线可独占修改）**：`os/servers/`（vm、rs、pm、vfs、ds、is、mib、devman、input、ipc-server、sched 全部）、`os/fs/`、`os/net/`、`os/commands/`、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/` ~ `13-stage-*`、`15-stage-fs/`、`17-stage-net/`、`18-stage-commands/`。
>
> **并发规则**：见 [edge4.md](edge4.md) §1。本线对 edge1（kernel/arch）与 edge2（minix-types/sys/rt/sef、框架库）是**消费方**：需要它们产出时先读对方文件状态，未 ✅ 则先做无前置条目。E1 trap 层 / E2 wrapper 已于 2026-09-16/17 闭环通电，因此本线大量"通电"类条目**现在即可开工**。

状态图例：☐ 未开工 ｜ 🔄 进行中 ｜ ⏸ 等待（注明等谁）｜ ✅ 完成（日期+commit）｜ 🚫 维持登记不排期

---

## PM 簇（04-stage-pm）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| S1 | P1-3：KernelIpcTransport 生产实现 | [04-stage-pm/todo.md P1-3](04-stage-pm/todo.md) | `ipc/transport.rs:86-96` 三方法 `unimplemented!()` → 接 minix-sys 真实传输（E1 已通电） | 无 | ✅ 2026-09-19 09f6039e7（三方法委托 DirectTrapTransport；错误类型换 PM 本地 IpcTransportError 保真携带原始 errno——minix_types::IpcError 无原始载荷违反 V3-P2-6） |
| S2 | 接线批次 A：凭证 13 调用 | [04-stage-pm/todo.md §11.1.1](04-stage-pm/todo.md) | wire（E7 A 批已落 minix-types）+ `CopyGroups` 生产实现（sys_datacopy）+ do_getepinfo groups 拷出（D-30(b)） | S1 | ✅ 2026-09-19 e56618e20（13 臂全接：decode::setid/groups/getsid raw 解码、SysCopyGroups 网关适配、SysVfsForward→tell_vfs 三段[VfsForwarder 契约改形携表]、Get 族双值 reply 预填；391+11 passed） |
| S3 | 接线批次 B 余：sigaction 族 5 调用接线（= E-INITSYS 的 PM dispatch 臂） | [04-stage-pm/todo.md](04-stage-pm/todo.md) ｜ [edge_todo.md](edge_todo.md) E-INITSYS | `dispatch_pm_call` 补信号/uid 族 match 臂——handle_sigaction/sigprocmask 逻辑已在但未接消息解码；wire（MessLcPmSig/MessLcPmSigset）与 sys_sigreturn wrapper 均已备 | S1；解锁 edge3 S36 与 edge4 E-INITSYS 收口 | ✅ 2026-09-19（五臂全接 bde80b05: sigaction copy 缝+sigpending reply 载荷+sigsuspend+sigprocmask+sigreturn;余件收口本批:SIGPROCMASK 按 needs_check 门控/SIGRETURN 无条件的内联 check_pending 经 PmEventServices[RestartServices 生产适配器]接上;3 接缝测试[pending 位消费/Block 保留/无条件重投];400+11 passed。12-signal-handlers.md §8 同步） |
| S4 | 接线批次 C：时间 6 调用 | [04-stage-pm/todo.md](04-stage-pm/todo.md) | ClockSource 生产实现；GETUPTIME 的 C 对账裁决（com.h:315-345 GET_* 家族无此号）登记 [edge4.md](edge4.md) §6 OQ | S1 | ✅ 2026-09-19（六臂全接:Stime/GetTimeOfDay/ClockGetRes/ClockGetTime/ClockSetTime[6b948e2ae]+GetRUsage 36 收尾——decode::rusage[ipc.h:510-515]+SysTimesVmCtl 生产实现[SYS_TIMES 直连+VM_GETRUSAGE taskcall]+128B struct rusage 组装经 CopyToUser 拷出;VM 侧 VM_GETRUSAGE 按值三槽已全链[query.rs/dispatcher.rs:912-920/encode.rs],值通道替换 C 的 datacopy 往返、用户可见字节逐位一致,20-misc-queries.md D6 同步） |
| S5 | 接线批次 D：itimer | [04-stage-pm/todo.md](04-stage-pm/todo.md) | TimerCtl/VTimerCtl 生产实现 + sys_datacopy 拷贝 | S1 | ✅ 2026-09-19 ee1145c0c（SysTimerCtl 本地簿记+内核时钟[C 定谳:PM 无 sys_setalarm,纯本地队列]、SysVTimerCtl 直委托、TimerFaces 穿分发表、17 号臂+decode::itimer、389+11 passed） |
| S6 | 接线批次 E：exec 3 调用 | [04-stage-pm/todo.md](04-stage-pm/todo.md) | sys_exec wrapper 已备（E2）；D-16 core dump 路径契约裁决（VFS 契约重设计 + minix-types wire 成员）随批 | S1；D-16 契约裁决（edge4 §6 OQ） | ✅ 2026-09-19 06936df2d（三臂全接：Exec[SysVfsExec 五域转发]/ExecNew[exec_info 192B 拷入+reply suid,libexec.h:21-58 布局]/ExecRestart[ExecServices 装配]；VfsExec 契约改形携表；391+11 passed） |
| S7 | 接线批次 F：调度 2 调用 | [04-stage-pm/todo.md](04-stage-pm/todo.md) | get/setpriority；SCHED 客户端 D-05/D-12（sched 服务器参战即可真） | S1、S27 | ☐ |
| S8 | 接线批次 G 余 + D-31/D-32 | [04-stage-pm/todo.md](04-stage-pm/todo.md) | mcontext/sprof wrapper 消费、sysgetenv 结构拷入与 EFAULT 透传、D-31 诊断输出接 sys_diagctl（E2 已落）、D-32 calls_stats 计数 | S1 | ✅ 2026-09-19（八臂全接：mcontext 18/19[2696568b7]/GetProcNr 46/GetEpInfo 45+D-30(b) 拷出[7bc7f0c21]/SysUname 25/SProf 39 全载荷[d6ecd22ca+cb0666690]/SvrCtl 38 sysgetenv 机制[1dfdbd4ac]/GetSysInfo 47[D-32 计数+SI_CALL_STATS 暴露 6965e6885+10d27ca9a]；D-31 diag_write 缝[A-6]） |
| S9 | D-02：BootParams 占位（GETMONPARAMS/GETIMAGE 双侧新建） | [04-stage-pm/todo.md D-02](04-stage-pm/todo.md) | 三方联合条目：kernel 臂（**跨界改点 C-3**，由 edge1 认领实现）+ minix-sys wrapper（edge2 认领）+ PM 消费（本线） | edge1/edge2 认领排期 | ⏸ |
| S10 | D-05/D-12/D-17：SCHED 客户端真实化 | [04-stage-pm/todo.md](04-stage-pm/todo.md) | sched_start_user 非 KERNEL/NONE 臂、minix_sched 假 endpoint、sched_stop 假成功——SCHED 服务器通电后逐一转真 | S27 | ☐ |
| S11 | PM 卫生批 | [04-stage-pm/todo.md §12.3](04-stage-pm/todo.md) | `ipc/decode.rs` unsafe 单点化过渡（6 处内联解码）、44 条 field_reassign lint、plan.md 四列对照表、V3-P3-6 文档锚点校准（01-pm-init-main.md） | 无 | ✅ 2026-09-19 d18966fb7（decode 单点化 7 处（实测含 Ptrace 新增 1 处）；44 lint 清零；对照表刷新 4 行（原判 3 行，实证多 1 行 datacopy）；锚点校准由 D5 迁移结构性完成 14/14） |

## VFS 簇（05-stage-vfs）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| S12 | W1：内核 IPC 原语真实传输激活 + 56 臂接入 | [05-stage-vfs/todo.md P1-2](05-stage-vfs/todo.md) ｜ plan.md §8 矩阵 | trap 桥已通 → fs_comm 窗口接真实 IpcTransport/sys_safecopy/sys_datacopy；dispatch 64 臂中 56 个对话臂从 Nosys 逐臂接入 | 无 | 🔄 2026-09-19（W1 两片:f746ea022 传输底座[IpcFsTransport 生产传输+handle_fs_reply 补 do_reply 全语义+GlobalComm 入 VfsState+run_once FsReply 臂]+本片对话原语[VfsState::fs_sendrec 六步进入半+EDEADLK/双重挂接守门+flush_send_queue 补发半];余件=56 臂逐臂接入[S13/S14 依 W6/W7 次序]） |
| S13 | W4 + W5：SEF LU/restart 接线；dmap/smap 初始化 + DS 订阅排空 | [05-stage-vfs/todo.md](05-stage-vfs/todo.md) | C-1 SEF RS 侧行、C-9 真实 DS 订阅（DsClient 已备） | S12 | ✅ 2026-09-19（W5 已落:VfsState 用 DmapTable::init 接 C init_dmap 的 CTTY 槽[main_loop]+ds_drain 排空循环[check→classify→retrieve_u32→DsUpTarget sink,misc.rs]+DsClient::check key 回拷[edge4 C-13]+订阅动词 subscribe 已备,由启动段执行;W4 已落:run() 换 sef_receive_status 驱动[VfsIpc 适配+握手 sef_receive(PM) do-while+Signal 默认忽略+Init≡init_fresh 映射+DS 订阅启动段执行];LU prepare/rollback 决策已备,RS 推进面挂通电） |
| S14 | W6 + W9：根挂载 REQ_READSUPER 往返 + mfs 25 项 Pending | [05-stage-vfs/todo.md](05-stage-vfs/todo.md) ｜ [15-stage-fs/todo.md](15-stage-fs/todo.md) | C-7 readsuper 确认往返；mfs 侧 fs_lookup 等 25 项（15-stage 域内，同线） | S12；mfs 侧随 S28 | ✅ 2026-09-19（req_readsuper 往返落地:encode_readsuper[ipc.h:2111-2119 布局+REQ_RDONLY/ISROOT vfsif.h:8-9]/decode_readsuper_reply[ipc.h:198-211:node_details+fs_flags+con_reqs]/FsSuperblock 生产读者[FsClient::send_with_retry]+SuperInfo.max_reqs[RES_THREADED?NR_WTHREADS:1,mount.c:307-312];mfs 25 项 Pending 已随 S28 清零[plan §0 调查];do_init_root 注记刷新。12-request-wrappers.md §9 同步） |
| S15 | W8：驱动死亡级联 fproc 扫描编排 | [05-stage-vfs/todo.md](05-stage-vfs/todo.md) | 驱动死亡 → fproc 扫描 → 关闭句柄 | S12 | ✅ 2026-09-19（两级联编排:device_map::driver_death_cascade[身份扫描 dmap major/smap 行+字符/socket 失效执行+classify_vanish 家族分类,块走 bdev_up notice]+pipe::driver_vanish_plan[fproc 扫描:Cdev 端点匹配→ReviveEio/Sdev 经 stop_matches→StopSdev;select 第三面归 select 模块];三面决策族[Fix #5/#6/#11]由此合编。19-device-map.md §9 同步） |
| S16 | filedes.rs:207 偏离注释残留收口 | [05-stage-vfs/todo.md L61](05-stage-vfs/todo.md) | 6 处清点余 1 处（"simplified" 注释 + 行为核实） | 无 | ✅ 2026-09-19 56cc03eb7（实测余 2 处同簇一并收口：may_suspend 注释纠错 + 末次关闭扇出事实化，368 passed） |

## RS 簇（03-stage-rs）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| S17 | E-11 / E9 余：RS 侧 KernelApi 换装 + 19 号主线 handler 生产接线 | [03-stage-rs/todo.md E-11](03-stage-rs/todo.md) ｜ [edge_todo.md](edge_todo.md) E9 | minix-sys wrapper 五域面已全就绪（cbb7b9571/d18be19e1/d0abe5143）——`UnimplementedKernelApi` 换真实现 + 各 handler 接线（19-rs-external-interfaces.md 主线）。**RS 是全体服务器启动的枢纽，优先级最高** | 无（wrapper 已备） | 🔄 2026-09-19 0e33c276c+3b1c82eb4（换装半+DS 缝已落：TrapKernelApi 27/27 方法含 ds_lookup_by_label[DsClient 生产委托]、do_update 的 ds_lookup 真缝、getsysinfo 拷出注记清账；exec.rs srv_execve 组装[✅ 五步已落 2b27fce0a+25a188e2d]、三个 cpf_grant READ 授权[✅ 97458bf13]、live_update 两效果[✅ 789b1f6c4]、DS 三缝[✅ 667d87e7c+98d9739a5]——余 unpublish 的 mapdriver 半[归 S12 传输批]与 PCI 半[A-10 fail-closed,恒 false 非可接线] |
| S18 | E-1：RS 自升级进程面 | [03-stage-rs/todo.md E-1](03-stage-rs/todo.md) | self_lifecycle 13 切片就绪（awaiting-wiring: 18）——srv_fork 后 update_service(RS_DONTSWAP) + init_service + YIELD 链 | S17 | ✅ 2026-09-19（self_update 从 unimplemented 落地:BootInit::self_update 按 srv_fork 返回值经 self_upgrade_role 双分支——新实例腿[update_service(RS_SWAP)+cpf_reload+cleanup_service+vm_memctl(PIN)]与旧实例腿[privctl(SetSys)+sched_init_proc+privctl(Yield)];SysApi 新增 cpf_reload[trap=GrantTable::register,锚 safecopies.c:373-381];Mock 脚本 fork_pid 两腿测试[调用序+互斥];feature live-update 双配置 335 全绿。18 篇增接线小节） |
| S19 | R40 余半 + A-6 diagctl 输出缝裁决 | [03-stage-rs/todo.md](03-stage-rs/todo.md) | 13/14 号域测试随迁 testutil；A-6 通用诊断输出缝裁决（rs_verbose 全家出口）——**S22 的前置** | 无 | ✅ 2026-09-19 1f72d8ffe（A-6 裁决落地：sys_diagctl code1 单汇点 + SysApi::diag_write + minix-sys 共享 helper，三方同构 IS/PM；R40 余半按 R40 权威文本方案 a 随 S17 触碰 handler 渐进随迁） |

## VM 簇（02-stage-vm）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| S20 | V11-P1-1 通电宿主半 + V13-P3-1之2 + V14-P2-1 | [02-stage-vm/todo.md](02-stage-vm/todo.md) | VM 传输逻辑完备，宿主侧激活；VfsRequestQueue 补按 owner 取消（死进程兜底，wire 已通现在可做）；V14-P2-1 VM SEF 面收编 minix-sef 第四消费方裁决 + E1 信号投递通道裁决 | 无 | ✅ 2026-09-19（058fdbe22 V13 purge+墓碑；0b0f050bc V14 方案 A 收编 minix-sef + 信号抵达裁决按 kernel 唤醒半契约；V11-P1-1 宿主半核验闭环，真机半挂 E5(d)） |
| S21 维持 | V12-P3-1/P3-2 优化登记 | [02-stage-vm/todo.md](02-stage-vm/todo.md) | 零帧批量预映射/find_free 双索引/bitmap lastscan——维持登记，独立设计决策 | 🚫 | 🚫 |

## 小服务器簇（ds / is / mib / devman / input / ipc-server / sched）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| S22 | E-DSWIRE→DS main.rs 通电 | [07-stage-ds/todo.md](07-stage-ds/todo.md) ｜ [edge_todo.md](edge_todo.md) E-DSWIRE | stage 内 seam/mock 已完备、minix-sys ds.rs 18 API 已落——main.rs 空转 loop → `Server::run` 真实装配（get_key_name safecopy / getsysinfo datacopy / notify 发送） | 无 ✅ 2026-09-19（登记复核:main.rs 真装配在库内已就位[DsServer::new+SysIpc(DirectTrapTransport)+SysKernel+run],余件=SysKernel 三拷贝动词真装——safecopy_from/safecopy_to/data_copy_to 接 minix-sys sys_safecopyfrom/to+sys_datacopy[注释'minix-sys 无 wrapper'已过时];data_copy_to 签名补 where_[m_lsys_getsysinfo.where,store.c:672]+调用点透传;测试:where_ 透传见证+hosted EIO 契约;112 全绿） |
| S23 | E-ISWIRE(3)：IS main.rs 生产替换 | [08-stage-is/todo.md](08-stage-is/todo.md) ｜ [edge_todo.md](edge_todo.md) E-ISWIRE | `UnimplementedTransport/UnimplementedFkeyCtl` → 生产 impl + main 装配（IS 现在运行即 panic）；LU/ST 拦截路径登记不实装 | S19（A-6 裁决）；E-ISKMESS 链路随之激活 | ☐ |
| S24 | MIB 卫生 + NR_VNODES 裁决 + mib_get_label 接线 + main 装配 | [10-stage-mib/todo.md P3-2/P1-5](10-stage-mib/todo.md) ｜ [edge_todo.md](edge_todo.md) E-DSWIRE/E-ISWIRE 增补 | 3 条 clippy 清零；NR_VNODES 语义裁决；mib_get_label 接 DsClient（ds.rs 已备）；MibIpc main 装配（sef_receive_status 已接） | 无（DS 装配后 label 查询真） | ✅ 2026-09-19 bd5c3d008（NR_VNODES=1024 按 C 锚 vfs/const.h:8 + mib/kern.c:341；SysServices 持 DsClient 换真 label+getnuid 两动词，其余 fail-closed 归 S33；clippy 3 条清零；minix-sys 补 retrieve_label_name[C-9]） |
| S25 | E-DMWIRE：devman 生产接线四缺 | [11-stage-devman/todo.md](11-stage-devman/todo.md) ｜ [edge_todo.md](edge_todo.md) E-DMWIRE | server 生产 Transport + 请求分类器（消费 E-REQWIRE fs_driver.rs 权威，已立）+ main.rs 装配替换停车 + ClientTransport/RsTransport 生产 impl + RS publish.rs devman 臂接生产端 | 无（wrapper/wire 全备） | ☐ |
| S26 | ipc-server 生产装配 | [13-stage-ipc/todo.md](13-stage-ipc/todo.md) ｜ [edge_todo.md](edge_todo.md) E-IPCWIRE | IpcBoundary 生产 impl（thin minix-sys 包装，E-IPCWIRE 八缺已闭合）+ main.rs 去 panic + 恢复 minix-sys 依赖（T-3 移除过）+ mib_tree.rs 消费 E-RMIBWIRE + IPC-D-4 .design 快照。**在制避让**：tests/integration.rs 已落地（fa668079d） | 无 | ✅ 2026-09-19 0508c0213（主体已落：SysBoundary 18 动词[凭证整臂/copy 族/E-IPCWIRE 布局/VM 消息族] + SysEventLoopTransport + main 四步去 panic；余件全清:C-12 RMIB 协议走查器已落[minix-sys rmib_call 函数节点回调+name 指针视图对齐+rmib_info;ipc-server handle_mib 编排 INFO/CALL 分发+REPLY 两分+grant safecopy 四动词+build_kern_ipc_tree 挂载;INFO 明细拷出挂 E-IPCWIRE §8 fail-closed];IPC-D-4 .design 快照已落[00/99 三件套]） |
| S27 | E8 余项：sched 生产二进制真实通电 | [06-stage-sched/todo.md](06-stage-sched/todo.md) ｜ [edge_todo.md](edge_todo.md) E8 | minix-sched 进 run() 全链——传输底座已真机验收（test-user-trap PASS），剩服务器参战场景化验证 | 无 | ☐ |
| S28 | input IN-P3-1 保留项复核 | [12-stage-input/todo.md L133/L137](12-stage-input/todo.md) | 循环壳 serve.rs 已落（593c2daa6）——Cargo.toml minix-sys 依赖、connect/produce/init 出口、占位循环在 E-INWIRE 落地后复核（真机联调归 edge4 E5） | 无 | ✅ 2026-09-19（复核通过：minix-sys 依赖已真实消费、main.rs 停车循环已被 serve() 替换（main.rs:39）、connect/produce/init 出口即通电主路径；IN-P3-1 三类处置全部消解；self_ep=NONE 待 RS 分配归 E5 真机联调） |

## FS / net 簇（15 / 17-stage）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| S29 | E-FSRUNTIME：8 个 fs server bin 启动握手 | [15-stage-fs/todo.md](15-stage-fs/todo.md) ｜ [edge_todo.md](edge_todo.md) E-FSRUNTIME | mfs 第一个接（V1-P0-1 装配产物直接可用），pfs 第二个（boot 链需要）：SEF 回调注册 + RS_INIT 握手 + 参数解析 + 信号循环，沿 E-ISBOOT/E-INWIRE 先例的通用 startup 面 | 无 | ☐ |
| S30 | V1-P2-10：ext2/isofs/sffs/vbfs/hgfs 覆盖面 + procfs/ptyfs 批 6/7 | [15-stage-fs/todo.md](15-stage-fs/todo.md) | DEFERRED 大件：先补 21~24 篇缺失章节，再随 mfs 装配模式逐 server 复用；procfs/ptyfs 消费 TreeServer 走批 6/7 | S29 | ☐ |
| S31 | E-NETSTART：lwip/uds 双 server 真实 main | [17-stage-net/todo.md](17-stage-net/todo.md) ｜ [edge_todo.md](edge_todo.md) E-NETSTART | 跟随 S29 落地的通用 SEF/RS start 框架接入，startup.rs 8 态粗粒度沿用 | ⏸ S29（通用框架） | ☐ |
| S32 | E-FSCMDS：fsck/mkfs 命令认领 | [15-stage-fs/todo.md](15-stage-fs/todo.md) ｜ [edge_todo.md](edge_todo.md) E-FSCMDS | 等盘上结构层稳定（V1-P2-10 排期）后由命令轨道认领，复用 minix-fs-mfs/ext2 解析写入函数。真机制作根镜像（装机面）依赖此条 | ⏸ S30 | ☐ |
| S33 | E-MIBPROD / E-ISPROD 余项：五 producer 对账 | [edge_todo.md](edge_todo.md) E-MIBPROD/E-ISPROD | kernel 半已闭（ProcInfoStruct 单点权威）；余 = DMAP_TAB producer（VFS）、RS/DS/VM producer 对账、IS dump_pm/dump_vfs/dump_rs/dump_ds/dump_vm 五处 TODO(P1) 消费对账 + run_dump 真实装配 | 无（布局权威已立） | ☐ |
| S34 | E-ISBOOT 的 IS/RS 侧 | [edge_todo.md](edge_todo.md) E-ISBOOT | RS 动态加载 IS 的注册面（S17 之后）；TTY 侧 fkey 真实观察者（TTY crate staging 与 edge2 L6 协调）；rc/system.conf 等价物载体裁决归 [edge4.md](edge4.md) §6 | S17、S23 | ☐ |
| S37 | E-DMABUF vm 侧：连续页分配 trait 实现 | [edge_todo.md](edge_todo.md) E-DMABUF | 按 edge2 L9 定稿的 trait 契约，VM 侧实现 + 文档；解锁 16-stage 存储与 USB 驱动服务层 | ~~⏸ edge2 L9~~ L9 已闭环（757398407） | ✅ 2026-09-19 7ed891a9b（VmDmaMemory：alloc_contiguous + Direct Map 线性平移 + 在役簿记；7 测试；消费方挂 16-stage） |
| S38 | E-FSVMCACHE vm 传输面：页移交/逐出回报 | [edge_todo.md](edge_todo.md) E-FSVMCACHE | vm_map_cacheblock/vm_set_cacheblock 等价传输面；落地后解锁 edge2 L15 | 无 | ☐ |

## 命令与 init 簇（18 / 09-stage）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| S35 | 18-stage C-1 长尾：24 域逐批接线 | [18-stage-commands/todo.md §6.1](18-stage-commands/todo.md) | 剩余批次：05 sh（**前置 13 篇 [ARCH] A-2 决策**）、09 shell 批（ed/mined）、10→11→12、13→23 终端批、14~17 存储批（fsck/mkfs 受 S32 约束）、18~19 网络批（17-stage）、20~21、init 域 reboot/shutdown/rcorder。批内执行面余项（open 路径类命令 ⏸ edge2 L10；seq 浮点、pr 多栏、sort 外部归并等逐项批内处理） | 各批前置不同（见要点）；L1/L2 落地后宿主冒烟即可信 | ☐ |
| S36 | 18-stage P1-1/P1-2：Requires 回填 + POSIX 基准引用 | [18-stage-commands/todo.md §2/§3](18-stage-commands/todo.md) | 随 S35 各批同步：契约表 Requires 列逐命令回填（06/07/08/22 已做，余各篇）；各命令文档补 POSIX 准绳引用 | 随 S35 | ☐ |
| S39 | init P1-2 ①② + P0-8 libcrypt + E-INITSYS init 侧跟进 + P2-6 残余接线 | [09-stage-init/todo.md](09-stage-init/todo.md) ｜ [edge_todo.md](edge_todo.md) E-INITSYS | ①HashMap→BTreeMap 决策（动 A-1 闭单决策，谨慎）②Arc→alloc Arc；P0-8 新建 minix-crypt crate（新成员 → edge4 认领 os/Cargo.toml）；E-INITSYS ①②闭单后 init 侧小批次（信号 trampoline + setsid/ctty + utmp 台账）逐条消解 P2-6 八组等待态 | S3（PM dispatch 臂）、edge2 L11/L12 | ☐ |
| S40 | E-THREAD-MODEL 立项指针 | [18-stage-commands/todo.md §4](18-stage-commands/todo.md) | 线程模型与 futex 无归属——推动 14-stage-runtime 或 04-stage-pm 立项（归属裁决登记 edge4 §6）；18 侧只改 05-shell 依赖链指向 | edge4 §6 裁决 | ✅ 2026-09-19 c78a1a062（OQ-4 裁决=归 14-stage-runtime；05-shell-family.md 三处指针改指线程模型条目） |
| S41 | 各 stage 00/99 正文改写与按篇 review | 各 stage todo（如 [06-stage-sched/todo.md](06-stage-sched/todo.md) §3、[07-stage-ds/todo.md](07-stage-ds/todo.md)） | 06/07 等 stage 的 00/99 篇仍是 pending 骨架；13-stage 的 .design 快照（IPC-D-4）已并入 S26。按 plan.md §6.1 排期，低优先 | 无 | ☐ |

---

## 已闭单勿领（以 edge_todo.md 最新进度注记为准，stage todo 头部状态可能滞后）

- 02-stage-vm：V13-P2-2（call_mask u64）/ V13-P2-5（RS_SET_PRIV 真掩码）/ V13-P2-6（五消息 wire struct）**均已在 commit 403e47022 闭环**（stage todo 未刷新）；V10/V12 错误枚举收敛（Fix #75）；E-VFSWIRE（d136491ee 三步全闭环）。
- 04-stage-pm：P2-6（00/99 文档 Fix #60）、V3-P2-5（Fix #59）、V2-P2-7/8（Fix #49）、D-29 getsysinfo 数据路径（fbd4afa9b）。
- 05-stage-vfs：R2 轮 10 条 + 首轮 26 条全部闭合；E-REQWIRE。
- 07-stage-ds：stage 内 11 条全闭；E-MINTYPES-SYS 的 DS 段。
- 08-stage-is：V1 轮 10 条全处置；E-ISKMESS 主体（832c20945 + IS 侧取数通道）。
- 11/12-stage：stage 内条目全部闭合。
- 13-stage：R1 轮 19/20 闭合；E-IPCWIRE 八缺（bbac3ba9d，第 3-7 项 wrapper + 第 8 项布局）。
- 14-stage：P0/P1/P2/P3 编号条目闭合；V1-P0-2 vm_info 三函数与 V1-P1-3 通用 select 为"待消费方"DEFERRED（维持，不排期）。
- 15-stage：V1 全部编号条目闭合（338 passed 终验）。
- 16-stage：§1 正确性族 + G1-G5/G8/G9 + A1/A3/A5/A6/A8/A9/A11 闭合。
- 17-stage：14/14 条目全部闭环。
- 18-stage：C-2 18 侧、C-4、C-5、C-7 闭合。

## 本线在最终验收阶梯中的位置（全文见 [edge4.md](edge4.md) §7）

T2 服务器通电链（S17 枢纽 → S22/S27/S1 → S12 → 其余）、T3 init 真实运行（S39）、T4 命令执行（S35 + S32 装机面）。
