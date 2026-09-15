# 13-stage-ipc Rust 实现架构级 Review TODO

> **状态（2026-09-16）**：首轮架构审查完成，实施轮进行中——已完成 7 条（IPC-P2-3、IPC-P2-2、IPC-P1-6+T-4、IPC-P1-2、IPC-P1-3、IPC-P1-4、IPC-P1-5），余 8 条开口（P3-1 内 ShmTable 约定随 P1-1）。执行节奏：todo-fix 三段式，一次一条一提交。
> **来源**：13-stage-ipc 首轮代码扫描（查漏补缺 + 架构卓越度，2026-09-16）。入口：code-excellence（scope=dir）+ full-review 的 Gate A 覆盖穷举。
> **范围**：`os/servers/ipc-server/` 全部 17 个文件（4945 行），延伸核对 `os/libs/minix-types/src/ipc/ipc_server.rs`、`os/libs/minix-types/src/ipc/event.rs`、`os/libs/minix-types/src/message.rs` 的 IPC 消息面与 `os/libs/minix-sys/` 的 IPC wrapper 面。Ground truth：`minix3/minix/servers/ipc/`（main.c 284 行、sem.c 888 行、shm.c 469 行、utility.c 49 行）。
> **方法**：C 四文件逐函数清单 → `tools/coverage-extract/ipc-semantic-map.json` → Rust 实态，逐符号分类；约 20 项行为契约逐条对照 C 源（每条给出 C 行号与 Rust 行号）；分层架构审视（整体 → 模块 → trait → 函数），对照 Redox 的用户态化（userspaceification）路线、Linux `ipc/sem.c` 的现代设计（每集合锁、RCU、pending 队列、ipcperms 先行）与 OS 理论。
> **定位**：本清单只登记缺口与改进建议，不当场修正确性问题；发现的行为分歧全部登记（其中两处一旦接线即成正确性问题，已标注）。修复走后续 todo-fix 轮。
> **门裁剪声明**：本轮是代码扫描轮，不产 scan.md/STATE.md，Gate 0/STATE 简化为本声明（chapter 级先例，review-cmds.md §三）；Gate B/C/D-6/H 属修复轮。适用的门：锚点纪律（本文所有事实断言带 `file:line`）、文风门（style-bible）、translate 防线、Ground Truth 链、Gate A 覆盖穷举、Gate E 测试对账（§3）。Step 0 预检现状：`tools/design-coverage-check.sh fork-syscall-rewrite --stage 13-stage-ipc` 退出码 1——00/07/08/09/10/99 六篇缺 `.design/` 快照（01-06 六篇齐）。这是既知前置状态（plan.md §6.1：00/99 篇本身 pending；01-10 已于 2026-09-05 全部 reviewed），不构成本轮阻断，理由：本轮审代码不审文档版本沿革。见 IPC-D-4。
> **跨 stage 条目**：不在此展开，登记于 `../edge_todo.md`——本轮新增 E-IPCWIRE（生产面接线八缺）；既有 E-RMIBWIRE（MIB 客户端，代码半已闭环）、E5（联调测试包）。见 §5。

## 0. 审查结论速览

三条总体判断：

1. **判定层质量高，服务层整体缺失**。sem/shm/perms/events/lifecycle/mib_tree/dispatch/server 八个模块把 C 的语义判断做成了纯函数（效果以值返回：`TableEffect`/`Wakeup`/`SweepPlan`/`SyncAction`/`ShutdownVerdict`），83 个单元测试全过、clippy 零告警，锚点纪律好。但 `CallHandler` 的唯一实现是占位的 `StubHandler`（`server.rs:112-135`）——没有任何 struct 把各张表组装成服务器状态，七个调用没有一个真正的入口。这是最大的缺口，也是下一阶段的主战场（IPC-P1-1）。
2. **查漏补缺抓到两处"一旦接线即成 bug"的契约分歧**：do_semop 入口检查顺序（C 与设计文档都规定权限先于越界/撤销校验，代码分解把顺序弄丢了，IPC-P1-2）与共享内存引用计数清拍的 rc==0 环绕语义（C 的 u8 回绕保住段不被销毁，Rust 的饱和减法会销毁，IPC-P1-6）。另有 IPC_SET/RMID 应用逻辑、挂接/卸载落账三块"C 有、文档有、代码没有"的判定层缺口（IPC-P1-3/4/5）。
3. **文档落后于代码**：README 与 plan.md 仍称实现是"空壳 stub"（实际 4945 行、83 测试），A-1 风险已解除未回写，A-7 锚点漂移（IPC-D-1/2/3）。

本轮速览：

| 级别 | 条目 | 一句话 |
|---|---|---|
| P1 | IPC-P1-1 | 服务层组装根缺失：`CallHandler` 只有 `StubHandler`，七个调用无真入口；薄序列器方案 + 服务层必须回答的边界契约清单 |
| P1 | IPC-P1-2 | do_semop 检查顺序分歧：C（sem.c:693 注释、:704/:709/:731）与 doc 06 都规定权限先于越界/撤销，`validate_ops`/`authorize_ops` 的拆分把顺序弄反（**✅ 已完成** 2026-09-16，见 §1 修复记录） |
| P1 | IPC-P1-3 | IPC_SET 应用逻辑缺失（sem+shm 两处）：C 改 uid/gid/权限位+ctime（sem.c:550-559、shm.c:314-328），Rust 只有授权检查没有落账函数（**✅ 已完成** 2026-09-16，见 §1 修复记录） |
| P1 | IPC-P1-4 | shm 的 IPC_RMID 标记逻辑缺失：C 置 SHM_DEST 并立即尝试销毁（shm.c:334-336），Rust 的 sweep 只读标记不置标记（**✅ 已完成** 2026-09-16，见 §1 修复记录） |
| P1 | IPC-P1-5 | shmat/shmdt 落账缺失：C 刷新 atime/lpid（shm.c:164-165、:228-229），且 shmdt 更新的也是 atime（C 的怪癖，doc 08 已如实记录）——Rust 连函数都没有 |（**✅ 已完成** 2026-09-16，见 §1 修复记录）
| P1 | IPC-P1-6 | 引用计数 rc==0 环绕分歧：C 的 u8 回绕（rc-1=255）使段存活，Rust 饱和到 0 会销毁带 SHM_DEST 的段（shm.c:187 vs refcount.rs:91）（**✅ 已完成** 2026-09-16，见 §1 修复记录） |
| P2 | IPC-P2-1 | 判定/效果分离整体评估：维持，不学 Linux 的直改+锁，也不学 Redox scheme 的直改式 handler；理由与边界 |
| P2 | IPC-P2-2 | transport trait 双名冲突与 send 语义保真：ipc-server 本地 `IpcTransport`（2 方法）vs minix-sys 同名 trait（7 方法）；进程事件回信在 C 是 asynsend3(AMF_NOREPLY) 不是 ipc_sendnb，单一 `send` 表达不了（**✅ 已完成** 2026-09-16，见 §2 修复记录） |
| P2 | IPC-P2-3 | 死代码消除：10 个零使用常量再导出、恒真的 `needs_cancel()` 与 `write_all`/`write_value` 返回值、`ack_type`/`proc_event_reply_type` 双名（**✅ 已完成** 2026-09-16，见 §2 修复记录） |
| P3 | IPC-P3-1 | 小项集合：`RefCell<Box<T>>` 的 Box 冗余、`ShmTable::new()` 90KB 值拷贝、`migrate_block` 的两次克隆、`classify` 的 is_notify 冗余参数（**✅ 已完成** 2026-09-16，见 §2 修复记录；ShmTable Box 约定随 P1-1） |
| T | IPC-T-1 | `migrate_block`（挂起计数迁移，最易错的函数之一）零测试（**✅ 已完成** 2026-09-16） |
| T | IPC-T-2 | `next_seq` 0x7fff 环绕无测试（**✅ 已完成** 2026-09-16）；IPC-T-3 `tests/` 集成目录被注释声称但不存在（**✅ 已完成** 2026-09-16）；IPC-T-4 sweep rc==0 边界无测试（**✅ 已完成** 2026-09-16，随 IPC-P1-6）；IPC-T-5 `run()` 连续失败 panic 路径无测试（**✅ 已完成** 2026-09-16） |
| D | IPC-D-1 | README.md:6 与 plan.md:6/:167/:410 的"空壳 stub"表述过时 |
| D | IPC-D-2 | plan.md:17 A-1（minix-types 无 IPC 消息类型）已失效；A-5 应注记部分完成 |
| D | IPC-D-3 | plan.md:185 A-7 锚点漂移：sem.c:713-722 → 实际 :726-739 |
| D | IPC-D-4 | plan.md §6.1：00/99 篇 pending + 六篇缺 `.design/` 快照（Step 0 预检 FAIL 的既知状态） |
| D | IPC-D-5 | 03 篇 §2.5/§3 D4/§4.1 的 MountTable 失真（E-RMIBWIRE 已登记，回指） |

验证基线（2026-09-16）：`cargo test -p minix-ipc-server` = **83 passed / 0 failed**；`cargo clippy -p minix-ipc-server` 本 crate 零告警（依赖 minix-types 余 1 条 large_enum_variant 告警，ipc-server 侧同型问题已 `#[allow]` 并写明理由，`sem/table.rs:87`）。

---

## 1. 查漏补缺（P1）

### IPC-P1-1 服务层组装根缺失：七个调用没有一个真入口

**是什么**：整个 crate 的判定层——`SemaphoreTable`/`SemSet`（`sem/table.rs`）、`WaiterTable`（`sem/waiter.rs`）、semctl 十三命令（`sem/ctl.rs`）、shm 段表与挂接（`shm/segment.rs`+`shm/attach.rs`）、清扫（`shm/refcount.rs`）、订阅开关（`events.rs`）、MIB 路由（`mib_tree.rs`）——都是等着被组装的零件，但组装它们的那个 struct 不存在。`CallHandler` 的唯一实现是 `StubHandler`（`server.rs:112-135`）：`handle_call` 一律回 ENOSYS，`handle_mib` 与 `on_cycle_end` 空体。C 侧的 `do_semget`/`do_semctl`/`do_semop`/`do_shmget`/`do_shmat`/`do_shmdt`/`do_shmctl` 七个入口（main.c:12-20 的 `call_vec` 表）在 Rust 侧没有对应物。`main.rs:20-28` 的二进制入口直接 panic，注释自认是"wiring list"。

**为何**：这不是偷懒，是分阶段的刻意安排（`server.rs:11-14`、`ipc-semantic-map.json` 头注都承认 handler 层未落地）。但它现在挡住了两件事：第一，判定层里相当一部分代码（`ctl.rs` 的命令分派、`waiter.rs` 的完成路径）没有任何端到端调用者，只有单元测试摸过；第二，服务层是本 stage 内**唯一**能把"判定 + 边界效果"串成外部可见行为的地方，它的形状决定后面每一条修复的落点，所以要先定形状再动手。

**方案对比**（三案）：

- **方案 A（推荐）：薄序列器**。一个 `IpcService` struct 持有全部状态（两张表、等待者表、订阅开关、MIB 静态描述），实现 `CallHandler` 的四个方法；每个方法体就是 C 对应函数的调用序列：解码消息 → 调判定函数 → 把返回的效果值经 transport/系统调用边界执行。逻辑只住在判定层，服务层不发明新判断，只负责"按 C 的顺序调用"。这保持了单线程事件循环下 `RefCell`/`&mut self` 的简单借用模型，也让"入口检查顺序"（IPC-P1-2）这类时序契约有唯一落点。Redox 的同类物是 scheme daemon：请求枚举进来，handler 直接驱动状态机——我们的判定/效果分离比它多一层，换来的是判定层 83 个无内核单测。
- **方案 B（否决）：七个 handler struct 各自持表**。把 sem 和 shm 拆成两个服务对象再组合。C 的四个 .c 文件确实语义独立，但它们共享三样东西：进程事件订阅开关（sem 创建/销毁驱动，main.c:176-189）、每轮收尾的引用计数清拍（shm 专用但挂在主循环，main.c:279）、MIB 信息装配（sem/shm 各半，main.c:27-51）。拆开之后这三处要么重复要么互相调用，比一个 struct 更纠缠。
- **方案 C（否决）：判定层直接持 transport 回调**。让判定函数自己发消息（C 就是这么写的，`complete_semop` 在 sem.c:242-243 内部 send）。这会把 83 个纯函数单测全部变成需要 mock transport 的测试，判定层的可测性优势归零——为了像 C 而牺牲重写后的最大收益，是 translate 思维。

**建议**：按方案 A 落地 `IpcService`，一次接线一个子系统（顺序建议：semget → shmget → semctl → shmctl → semop → shmat/shmdt → 进程事件/MIB/收尾钩子）。服务层落地时必须同时回答以下边界契约（每条都来自 C 的入口代码，判定层没有它们的位置，只能落在服务层）：

1. **操作数组拷贝**：do_semop 的 malloc 失败回 ENOMEM（sem.c:677-678）、sys_datacopy 失败原样返回（sem.c:680-682）。Rust 侧分配器模型不同（doc 06 §2.2 已声明记入 99），但"拷贝失败要回 EFAULT 系错误码而不是 EINVAL"必须显式定案。
2. **SETALL/GETALL 的缓冲长度**：C 拷入/拷出恰好 `sem_nsems` 个元素（sem.c:615-617/:584-588），短缓冲在 C 里表现为拷贝失败（EFAULT 系）；`write_all` 的短缓冲检查回 EINVAL（`ctl.rs:196-198`）是判定层的防御，服务层不得让它吞掉 EFAULT。
3. **pid/uid/gid 来源**：C 每个入口调 `getnpid`/`getnuid`/`getngid`（sem.c:720、utility.c:10-11）；`perms.rs:49-53` 已声明 Identity 由调用者查询——服务层接 `minix-sys/src/rs.rs:170` 的 `endpoint_identities_via`。
4. **时间来源**：所有 `now: u64` 是注入参数（`table.rs:219`、`segment.rs:102` 等），服务层接时钟（edge E-IPCWIRE 第 6 项）。
5. **挂起回信**：`Wakeup.code == NO_REPLY`（EDONTREPLY）时不发送（`table.rs:134-137` 的文档已写明由发送方检查——这个检查就是服务层的）。
6. **每轮收尾**：dispatch 路径末尾的 `on_cycle_end` 要发起全表 `vm_getrefcount` 轮询再喂给 `sweep`（`refcount.rs:66-73` 已写明查询在边界）。

### IPC-P1-2 do_semop 入口检查顺序：权限被排到了越界/撤销之后【✅ 已完成 2026-09-16】

**是什么**：C 的 do_semop 入口顺序是：找集合（sem.c:667-668）→ 个数零/超上限（:670-673）→ 拷贝数组（:676-682）→ **权限**（掩码组装 :695-702，`r = EACCES` 检查 :704）→ **序号越界**（`r = EFBIG` :709-717）→ **撤销标志**（:729-739）→ 试执行（:742）。顺序不是偶然的——sem.c:690-693 的注释写明："perform the permission check **before** checking on the validity of semaphore numbers, since obtaining the semaphore set size itself requires read permission"。Linux 同哲学：`ipcperms()` 在触碰信号量值或入队之前跑，挂起恢复后 `sem_revalidate()` 重查。本 stage 的设计文档也是对的：doc 06 §2.2 的七步表按 C 序排列（权限掩码第 4 步、越界第 5 步、撤销第 6 步），doc 06 §4（:169）开的处方就是 `validate(count, ops, perm, identity)`——权限在 validate 里面。

**分歧在哪**：Rust 把入口拆成了 `validate_ops(ops, set_count)`（`op.rs:85-105`：个数 → 越界 → 撤销 → 顺带算掩码）和 `authorize_ops(perm, identity, need)`（`op.rs:242-257`），而模块头注释把调用序定为"validate → authorize → try"（`op.rs:239-241`）。这个拆分一旦照注释接线，一个"既无权限又有越界序号"的请求会回 EFBIG（validate 先碰序号），而 C 回 EACCES；"既无权限又带 SEM_UNDO"同理回 EINVAL 而非 EACCES。errno 优先级是外部可见行为，属于 Rewrite 边界内。

**方案对比**：

- **方案 A（推荐）：把 perm+identity 收回 validate_ops**，签名改成 doc 06 §4 处方的 `validate_ops(ops, set_count, perm, caller)`：内部顺序改为 个数 → 扫掩码 → check_perm（失败 `Access`）→ 越界（`BadNumber`）→ 撤销（`Invalid`），返回 `Result<OpNeed, SemError>`。`authorize_ops` 删除。与 C 的三段扫描（掩码/越界/撤销）相比多一两次遍历，但 `SEMOPM ≤ 100`、数组在栈/堆上各一份，开销可忽略；换来的是顺序由类型签名钉死，调用方想接错都难。
- **方案 B（否决）：保持拆分，改服务层调用序为"先算 need 再授权再 validate"**。需要把掩码计算从 validate_ops 里再拆出来（`compute_need(ops) -> OpNeed`），服务层按 `compute_need → authorize → validate → try` 接线。顺序对了，但正确性悬在服务层的一行调用序上，没有任何机制防止后来者接错——本轮的分歧恰恰就是这么来的。
- doc 06 需要的同步：若采 A，§4 的 D5 处方与现状重新一致，只需把测试表里 `validate_rejects_bad_index`/`validate_rejects_undo` 的调用样例加上 perm/identity 参数；若采 B，doc 06 §2.2 的七步表要加一段"拆分保序"说明。A 的文档代价更小。

**建议**：方案 A，并补一个"无权限 + 越界序号 → EACCES"的顺序回归测试守住优先级（现在没有测试覆盖这个优先级，与 IPC-T-1 同批）。**注意**：当前无调用者，故记 P1；服务层若按现状注释接线，本条即升级正确性问题（P0）。

**修复记录（2026-09-16，方案 A 落地）**：`validate_ops` 签名改为 `(ops, set_count, perm, caller)`（doc 06 §4:169 的处方原形），内部按 C 序三段扫描——掩码（sem.c:697-706）→ check_perm（:704，sem.c:690-693 注释的缘由写进函数文档）→ 越界（:709-717）→ 撤销（:729-739）；`authorize_ops` 与 `need_mask` 删除（授权已并入 validate，零外部调用者）。测试：原两个 validate 测试补权限参数，新增 `validate_perm_precedes_num_and_undo`（无权限+越界 → EACCES、无权限+SEM_UNDO → EACCES、root 过门后后置检查仍生效）。doc 06 §4.2 函数清单与 §5.1 测试表同步。验证：`cargo test -p minix-ipc-server` = **84 passed / 0 failed**（83+1）。

### IPC-P1-3 IPC_SET 的应用逻辑缺失（sem 与 shm 两处）【✅ 已完成 2026-09-16】

**是什么**：semctl(IPC_SET) 在 C 里做四件事：所有者身份检查（sem.c:523-529）→ 拷回描述符草稿（:551-553）→ 改 uid/gid、权限位按 `~ACCESSPERMS` 掩膜替换、刷新 ctime（:554-559）。shmctl(IPC_SET) 同构（shm.c:314-328）。Rust 侧：授权检查有（`SemctlAccess::CheckOwner`，`perms.rs:204`；`ctl.rs:126-131` 执行），命令解码有（`ctl.rs:68`、`attach.rs:96`），但**改字段的函数不存在**——`ctl.rs` 只有 `write_value`/`write_all` 两个设值函数，`attach.rs` 连设值函数都没有。grep 全 crate 确认生产代码没有任何地方给 `perm.uid`/`perm.gid`/`perm.mode` 做过 IPC_SET 式赋值（唯一例外是 sweep 测试里手工塞 `SHM_DEST` 标记，`refcount.rs:139`/`:228`）。doc 05:119 与 doc 08:101 都已写明这四步——文档对，代码缺。

**为何是缺口而不是"留给服务层"**：本 crate 的分工原则是"判定在模块、字节搬运在边界"（`ctl.rs:7-9`）。IPC_SET 的字段替换是纯状态变更，跟 `write_value` 同类；若留给服务层直接改 `perm` 字段，就破坏了自己的分层原则，且掩膜替换（`mode &= ~ACCESSPERMS; mode |= tmp.mode & ACCESSPERMS`——保留 SEM_ALLOC/SHM_DEST 状态位、只换权限九位）是个容易丢的细节。

**方案对比**：

- **方案 A（推荐）：判定层补 `apply_set` 函数**。`ctl.rs` 加 `apply_set(set: &mut SemSet, new: SetFields, now: u64)`，`attach.rs` 加同型；`SetFields { uid, gid, mode }` 从拷入草稿解码而来。掩膜替换逻辑收在一处，可单测"状态位保留、权限位替换、ctime 刷新"三个断言。
- **方案 B（否决）：服务层直接改 `set.perm` 字段**。字段公开可改（`pub perm`），技术上可行，但同样的掩膜替换要写两遍（sem/shm），且绕开了"效果值返回"以外的状态变更都进判定层的惯例。

**建议**：方案 A；与 IPC-P1-4 同批实现（同是 shmctl/semctl 的变更命令），测试各补"IPC_SET 后 cuid 不变、SEM_ALLOC 保留、ctime 刷新"。

**修复记录（2026-09-16，方案 A 落地，草稿类型上移共享）**：新增 `perms::SetOptions{uid, gid, mode}`（sem/shm 权限面同构，共享一个草稿类型胜过两个一模一样的本地类型；字节拷贝留边界的注记写入类型文档，wire 布局仍属 E-IPCWIRE 第 8 项）；`ctl.rs::apply_set` 与 `attach.rs::apply_set` 各自落账——uid/gid outright 替换、mode 在 `ACCESSPERMS` 掩膜下替换（`SEM_ALLOC`/`SHM_DEST` 存活）、ctime 刷新、创建者字段不碰（C sem.c:554-559、shm.c:322-327）。测试 `apply_set_keeps_status_bits` / `apply_set_keeps_alloc_bit` 各验四断言（属主换、创建者留、状态位活、草稿噪声位 0o1000 丢弃）。lib.rs 再导出 `SetOptions`；doc 05 §4.2/§5.1、doc 08 §4.2/§5.1 同步。验证：`cargo test -p minix-ipc-server` = **86 passed / 0 failed**（84+2）。

### IPC-P1-4 shm 的 IPC_RMID 标记逻辑缺失【✅ 已完成 2026-09-16】

**是什么**：C 的 shmctl(IPC_RMID)：所有者检查（shm.c:330-333）→ 置 `SHM_DEST`（:334）→ 立即调 `update_refcount_and_destroy()` 尽早销毁（:335-336）。Rust 侧：`ShmctlCommand::Remove` 能解码能授权（`attach.rs:95/:129-134`），但置 `SHM_DEST` 的代码不存在——`refcount.rs` 的 `sweep` 只**读**标记（`refcount.rs:92`），测试里标记是手工 `perm.mode |= SHM_DEST` 塞进去的（`refcount.rs:139`、`:228`）。sem 的 RMID 链路是全的（`drain_set` → `table.remove`，`waiter.rs:164-174` + `table.rs:288-306`），shm 侧断在中间。

**方案对比**：

- **方案 A（推荐）：判定层补 `mark_destroy(index) -> MarkEffect` 之类的判定函数**，返回值告诉服务层"已尽力销毁/仍挂接"，服务层据此决定是否立即跑一轮 sweep（对应 C 的 :335-336 立即销毁尝试）。标记只置不 清（C 永不清 SHM_DEST，销毁是连槽一起释放——`refcount.rs:75-77` 的文档已写明）。
- **方案 B（否决）：服务层直接 `perm.mode |= SHM_DEST`**。字段公开所以能写，但"RMID 之后必须跟一次销毁尝试"这个 C 契约（:335-336）就没有类型层面的表达，容易漏。

**建议**：方案 A，与 IPC-P1-3 同批。测试：RMID 后未挂接的段在下一轮 sweep 前就被销毁（立即性）；已挂接的段标记就位、nattch 归零后销毁（延迟性——`sweep_destroys_at_zero` 已覆盖后半，`refcount.rs:145-164`）。

**修复记录（2026-09-16，方案 A 落地，返回值简化）**：`attach.rs::mark_destroy(table, index) -> Result<(), ShmError>` 置 `SHM_DEST`（C shm.c:334）。相对方案 A 原文的 `MarkEffect` 返回值做了有意简化：C 在 :335-336 是**无条件**立即清拍，不存在"看情况跳过"的分支，区分无信息量——"调用方必须紧跟一轮 sweep"的立即性契约改写在函数文档里，与 `apply_set` 的 `Result<()>` 风格一致。测试两枚：`mark_destroy_sets_pending_bit`（标记就位、槽位存活）与 `mark_then_sweep_destroys_unattached`（标记+清拍链：未挂接段当场销毁；延迟半由既有 `sweep_destroys_at_zero` 覆盖）。doc 08 §4.2/§5.1 同步。验证：`cargo test -p minix-ipc-server` = **88 passed / 0 failed**（86+2）。

### IPC-P1-5 shmat/shmdt 的落账函数缺失（含 C 的 atime 怪癖，必须保留）【✅ 已完成 2026-09-16】

**是什么**：C 的 do_shmat 在 vm_remap 成功后刷新 `shm_atime` 与 `shm_lpid`（shm.c:164-165，注释 :166 写明 nattch 惰性）；do_shmdt 命中后**也刷新 `shm_atime`** 与 lpid（shm.c:228-229）——按 POSIX 语义这里该是 `shm_dtime`，Minix3 写成了 atime，这是 ground truth 的既有怪癖。doc 08 如实记录了这两处（doc 08:64"记时间与进程"、:87"刷新访问时间与最后进程"），没有替 C 改错。Rust 侧：`ShmSegment` 有 `attach_time`/`detach_time`/`last_pid` 三个字段（`segment.rs:58-67`），但全 crate 没有任何函数给它们赋值——shmat/shmdt 的落账既没实现也没声明归属。地址对齐（`align_addr`，`attach.rs:35-43`）、物理查找（`find_by_phys`，`attach.rs:54-63`）、权限掩码（`attach_mask`）都齐了，断的正是"成功后的记录"这一步。

**为何要专门一条**：服务层接线时（IPC-P1-1）最自然的写法是"服务层直接改 `seg.attach_time`/`seg.detach_time`"，而 shmdt 那处**必须**写成 `attach_time`——不看 C 源（或 doc 08:87 的转述）几乎必然写成 detach_time，把怪癖"修好"，外部行为就变了（`ipcs -p` 的 ATIME 列会变成 DTIME）。这正是 style-bible 与 translate 防线要防的事：ground truth 的怪癖是契约。

**方案对比**：

- **方案 A（推荐）：判定层补 `record_attach`/`record_detach` 两个小函数**，各两行（刷 atime + lpid；detach 复用 atime 字段并加注释引 shm.c:228-229 说明这不是笔误）。落账语义进判定层，怪癖的解释钉在代码旁边，服务层无从写错。
- **方案 B（否决）：服务层直接改字段**。三个字段都是 pub，能写；但"detach 也刷 atime"这个反直觉写法散落在服务层接线代码里，没有注释锚点，下一个人一定会"修"它。

**建议**：方案 A。测试：shmat 后 atime/lpid 刷新且 nattch 不动（惰性）；shmdt 后刷新的也是 atime 字段（这条回归测试守住 C 的现行为，注释引 shm.c:228-229）。

**修复记录（2026-09-16，方案 A 落地）**：`attach.rs` 新增 `record_attach`（shm.c:164-165：刷 atime+lpid，挂接数惰性）与 `record_detach`（shm.c:228-229：刷的也是 **atime**）。后者函数文档用整段写明怪癖的来龙去脉与"不许修"的裁决（改 dtime 会让 `ipcs -p` 的 ATIME 列变成 DTIME——translate 式顺手纠正，本代码库拒绝）；detach_time 字段保留但生产路径不写（测试钉住恒零）。测试 `record_attach_stamps_atime_and_pid`、`record_detach_refreshes_atime_not_dtime`。doc 08 §4.2 补两函数条目（含怪癖告诫）、§5.1 补两行。验证：`cargo test -p minix-ipc-server` = **90 passed / 0 failed**（88+2）。

### IPC-P1-6 引用计数清拍的 rc==0 环绕分歧：C 保段、Rust 毁段【✅ 已完成 2026-09-16】

**是什么**：C 的 `update_refcount_and_destroy` 用 `u8_t rc` 接 `vm_getrefcount`，`nattch = rc - 1` 是 u8 运算（shm.c:187）。rc==0 时 `0 - 1` 回绕成 255——nattch 巨大，段绝不会被销毁。rc==255（即 `(u8_t)-1`）才是错误哨兵，走"找不到物理区，跳过"分支（shm.c:183-186）。Rust 的 `sweep` 里 `count` 是 `Option<u8>`（`refcount.rs:30`，None 对应哨兵，映射正确），但换算用 `count.saturating_sub(1)`（`refcount.rs:91`）：rc==0 时得到 0——一个带 SHM_DEST 的段会被当场销毁并 unmap。两种实现对外部分歧：C 把 rc==0 当"还有引用"处理，Rust 当"无人引用"处理。

**为何**：C 的回绕大概率是无意为之（作者没想过 rc==0），但 ground truth 链的裁决是行为对齐——`vm_getrefcount` 返回 0 意味着"连我们自己的映射都不算了"，此时销毁与否是外部可见差异（段消失 vs 段残留）。Rewrite 的边界是保持外部行为，怪癖同 IPC-P1-5 一样要保留，除非将来走三处一致的 [ARCH] 演进。

**方案对比**：

- **方案 A（推荐）：对齐 C——`count.wrapping_sub(1)`**，一行改动，u8 语义与 C 完全一致（rc==0 → 255 → 存活）。旁边加注释引 shm.c:187，写明回绕是契约不是缺陷。
- **方案 B（演进候选，暂不用）：`count == 0` 显式归入 skipped**，语义注释"无法判定引用状态，保守跳过"。行为与 A 相同（段存活），但偏离了 C 的机制形状；若哪天做 [ARCH] 演进（比如换成显式计数，plan A-3 的候选），B 的语义更干净——现在不做，只在 todo 里留个名字。

**建议**：方案 A；同批补测试 `sweep_zero_refcount_keeps_marked`（rc==0 + SHM_DEST → 存活，attached==255；见 IPC-T-4）。

**修复记录（2026-09-16，方案 A 落地）**：`refcount.rs` 的换算改 `count.wrapping_sub(1)`（原 saturating），u8 回绕语义与 C shm.c:187 完全一致，契约缘由（零答案回绕 255 → 带标记段存活）钉在换算处与 `RefQuery` 文档两处；doc 08 §4.2 的 `RefQuery`/`sweep` 两条目补环绕契约与"顺手改饱和即分叉"的告诫，§5.1 测试表补新行。测试落地名 `sweep_zero_refcount_wraps_alive`（rc==0 + SHM_DEST → 存活、attached==255、槽位保留）。验证：`cargo test -p minix-ipc-server` = **83 passed / 0 failed**（82+1）。**IPC-T-4 随本条闭合**。

---

## 2. 架构与设计（P2/P3）

### IPC-P2-1 判定/效果分离的整体评估：维持，边界已划对，别摇摆

**是什么**：本轮按"如果今天重写会怎么设计"逐层过了一遍，整体架构的结论是**维持现状**，把理由写下来，防止未来轮次在缺少论证的情况下反复推翻已经定过的架构决策。当前形状：单线程事件循环（`server.rs:5-9`）+ 判定纯函数化（效果以值返回）+ 两个 seam trait（transport/handler）注入。

**对照系**：

- **Linux `ipc/sem.c`**：每集合自旋锁 + 全局锁分层（`sem_lock`）、RCU 保护 idr 查找、双 pending 队列（per-array + per-semaphore，为单操作 FIFO）——这些全是为 SMP 扩展性服务的机制。Minix3 的 IPC server 是单线程用户态进程，没有并发穿插（doc 06:26 把这点讲透了：原子性不用锁，难点是乐观执行与回滚）。把 Linux 的锁结构搬过来是典型的 translate 陷阱；我们没搬，正确。
- **Redox**：Redox 把 POSIX 关注点持续推向用户态（"userspaceification"，见 redox-os.org/news/kernel-11/），SysV IPC 这类设施按其哲学就是用户态 scheme daemon——与本项目的"IPC server 是用户态服务"定位同构。Redox scheme handler 的风格是请求枚举 + handler 直改状态；我们的判定/效果分离比它多一层间接，多出的那层正是无内核单测的来源（83 个测试不需要起内核或 mock 全套系统调用）。这层间接的成本是效果值管道（`TableEffect`/`Wakeup`/`SweepPlan` 要有人执行）——执行者就是缺位的服务层（IPC-P1-1 的方案 A），缺口补上后成本即摊平。
- **OS 理论**：command-decision 分离（决策与效果解耦）是可测试性设计的标准手法；本 crate 是教科书式落地。唯一要警惕的后续风险：服务层落地时把新逻辑塞进判定层或把判定搬进服务层，层间边界逐渐糊掉。防的办法是把"服务层不发明判断"写进 doc 05-08 的接线章节（修复轮顺带）。

**建议**：架构维持，不立修复条目；本条本身就是 IPC-P1-1 方案 A 的设计依据。唯一配套动作：IPC-P1-1 落地时在 `server.rs` 模块头把"判定在模块、效果在服务层"的分工升格为显式注释。

### IPC-P2-2 transport trait 的双名冲突与 send 语义保真【✅ 已完成 2026-09-16】

**是什么**：两个问题。其一，workspace 里有两个同名 `IpcTransport`：`os/servers/ipc-server/src/server.rs:74`（2 方法：`receive`/`send`，事件循环的 seam）与 `os/libs/minix-sys/src/ipc.rs:496-515`（7 方法：send/receive/sendrec/notify/sendnb/senda/query，对应 C `_ipc.S` 全家）。同名不同形，读代码的人（和 AI）很容易混淆哪个是哪个。其二，保真度：C 主循环的调用回信走 `ipc_sendnb`（main.c:273），进程事件回信走 `asynsend3(AMF_NOREPLY)`（main.c:207-208）——**两个不同的内核调用**。Rust 的单一 `send` 方法（文档写"C: ipc_sendnb"，`server.rs:77`）表达不了这个差别；将来 DirectTrapTransport 接线时，进程事件路径会被错接成 sendnb。

**方案对比**：

- **方案 A（推荐）：本地 trait 改名 + 双方法**。`server.rs` 的 trait 改名为 `EventLoopTransport`（或 `LoopTransport`），并把 `send` 拆成 `send_reply`（sendnb 语义）与 `send_async`（asynsend3 语义）两个方法；测试 transport 两个方法同实现，生产行为各自映射内核调用。改名连带 `server.rs:35` 的 re-export 与测试。成本小，两个问题一次解决。
- **方案 B（否决）：删本地 trait，直接用 minix-sys 的**。测试就得拖 minix-sys 的 `CannedTransport` 形状（7 个方法都要实现），server.rs 的 seam 反而被无关方法污染；且 minix-sys 的 trait 是 wrapper 层的 seam，不是事件循环的 seam，层次不同。
- **方案 C（否决）：维持单 send，差异丢给实现注释**。保真度问题原样留给接线轮，正是本条要消掉的雷。

**建议**：方案 A，在服务层接线前做（晚做一天，测试 transport 就多一天接错形状的风险）。进程事件路径（`server.rs:280-299`）改调 `send_async`。

**修复记录（2026-09-16，方案 A 落地）**：`server.rs` 的本地 trait 改名 `EventLoopTransport`（trait 文档写明改名缘由：minix-sys 七方法 `IpcTransport` 与 VM 服务器私有同名 trait 已占用该名字）；`send` 拆为 `send_reply`（ipc_sendnb，main.c:273，dispatch/unknown 回信路径）与 `send_async`（asynsend3(AMF_NOREPLY)，main.c:207-208，进程事件回执路径），两个内核调用不可互换的映射义务写进 trait 文档；测试替身 `TestTransport` 两动词同实现（共享 `push_outbound`），语义差异由生产实现承担。lib.rs 再导出与 doc 01（D6 决策行 + §4.2 函数清单）同步——D6 行顺带修正了旧文"Redox 方案服务同款"的类比措辞为虚拟内存服务先例。验证：`cargo test -p minix-ipc-server` = 82 passed / 0 failed；clippy 本 crate 零告警；旧名残留仅 trait 文档中解释改名缘由的一处。

### IPC-P2-3 死代码消除：常量再导出、恒真函数、双名常量【✅ 已完成 2026-09-16】

**是什么**：三组。其一，**10 个零使用的常量再导出**（grep 全 crate 验证，定义处之外零引用）：`sem/table.rs:330`（`DENIED`）、`:333`（`SUPPRESSED`）、`sem/op.rs:260`（`AGAIN`）、`sem/waiter.rs:233`（`SUPPRESSED_WAKE`）、`shm/attach.rs:303`（`WRITE_MASK`）、`:305`（`READ_MASK`）、`:307`（`WRITE_BIT`）、`perms.rs:263`（`READ_BIT`）、`:265`（`WRITE_BIT`）、`:267`（`CONTROL_BIT`）、`:269`（`READ_WRITE`）——它们把 minix-types 的值换个名字再导出，注释都写"for mask-table readers"，而那些 reader 从未出现。服务层要用时从 minix-types 拿即可（crate 内其它代码正是这么做的）。其二，**恒真的函数与返回值**：`ProcEvent::needs_cancel()` 恒返回 true（`events.rs:141-143`，C 的门在订阅掩码上，main.c:203-204，而"未订阅时必无等待者"的不变量使恒真在行为上等价——但恒真谓词没有信息量）；`write_all`/`write_value` 的 `Result<bool>` 恒返回 `Ok(true)`（`ctl.rs:189-234`，"是否需要重试队列"在 C 里是无条件 check_set，sem.c:627/:641，bool 是残留）。其三，**双名常量**：`ack_type()`（`events.rs:151`）与 `proc_event_reply_type()`（`dispatch.rs:96`）返回同一个 `PROC_EVENT_REPLY`，两个名字各有一组调用者。

**为何**：code-excellence 的显式子目标。死代码不是中性的：恒真的 `needs_cancel` 与恒真的 bool 会让读者去找"什么时候是 false"，找不到就开始怀疑自己的理解；双名常量迟早分叉。

**方案对比**：

- **方案 A（推荐）：删**。10 个常量直接删（影响：无——零引用）；`needs_cancel` 删（调用者只有测试，`events.rs:198`）；`write_all`/`write_value` 返回 `Result<(), SemError>`（"成功即需重试"写进文档一句话）；`ack_type` 删、统一用 `proc_event_reply_type`（后者有 6 处使用，是保留方）。
- **方案 B（否决）：标 `#[allow(dead_code)]` 留着**。"服务层可能用到"是猜测；真用到时从 minix-types 拿一个常量的成本是零。留着只会让 grep 结果持续膨胀。

**建议**：方案 A，一个独立小提交（与行为无关的纯删除 + 受影响测试同步），在服务层接线前做完，避免接线轮的 diff 混入删除噪音。

**修复记录（2026-09-16，方案 A 落地）**：删除 11 处零使用常量定义（DENIED/SUPPRESSED/AGAIN/SUPPRESSED_WAKE/WRITE_MASK/READ_MASK/WRITE_BIT 两处/READ_BIT/CONTROL_BIT/READ_WRITE）及同族死常量 SIGNAL_BIT（events.rs，登记时未枚举、同类别一并删）；删恒真 `needs_cancel()` 与 `ack_type()`（回执统一走 `dispatch::proc_event_reply_type`，冗余测试 `ack_is_reply_type` 一并删除——其断言已被 dispatch 的 `should_reply_suspend_suppresses` 覆盖）；`write_all`/`write_value` 返回 `Result<bool>` → `Result<()>`，"成功即需重试"写进函数文档。顺带清理 6 个文件的未用导入（EAGAIN/EACCES/NO_REPLY/IPC_M/IPC_W/PROC_EVENT_REPLY/PROC_EVENT_SIGNAL）。文档同步：doc 09 D4 决策行改指 dispatch、§4.1 模块注记更新、§4.2 删两函数并新增"为何删"段落（防补回）。验证：`cargo test -p minix-ipc-server` = **82 passed / 0 failed**（83−1，删冗余测试）；clippy 本 crate 零告警（依赖 minix-types 余 1 条既有告警，与本条无关）；死代码 grep 零残留。

### IPC-P3-1 小项集合（不阻塞接线，顺手轮处理）【✅ 已完成 2026-09-16】

四项，各两行以内：

1. **`RefCell<Box<T>>` 的 Box 冗余**（`server.rs:165-166`）：`T`/`H` 已是泛型参数，`RefCell<T>` 即可；Box 多一次堆分配与一层间接。若是为了缩小 `IpcServer` 的尺寸，等真实尺寸成为问题再说——现在 `main.rs` 根本还没构造它。
2. **`ShmTable::new()` 的 90KB 值拷贝**（`segment.rs:110-123`）：`[ShmSlot; 1024]` 内联在结构体里，`new()` 按值返回要走栈/拷贝；服务层应 `Box::new(ShmTable::new())` 持有（C 的 `shm_list` 是静态全局，等价物是堆上长期驻留而非栈上临时）。放服务层接线约定里，不必改表本身。
3. **`migrate_block` 的两次克隆**（`waiter.rs:148-156`）：为了绕借用先 `clone()` 整个 waiter 再改计数。`Waiter` 含 `Vec<SemOp>`，克隆是堆分配；可以只提取 `(num, op)` 两个标量再改。频率低（每次重试的卡点迁移），记为净化项不阻塞。
4. **`classify` 的 is_notify 冗余参数**（`server.rs:273`）：调用点传字面 `false`（通知在 :269-271 已提前返回），参数只为纯函数测试存在。`server.rs:274-279` 的注释已解释，属已知取舍——若 IPC-P2-2 方案 A 落地，顺手把该分支的"Unreachable"注释更新为指向新的枚举臂。

**修复记录（2026-09-16）**：第 1 项——`IpcServer.transport` 去 Box（`RefCell<T>` 直持有，`new()` 不再堆分配；生产类型 DirectTrapTransport 与测试类型皆小，Box 只添间接）；第 3 项——`migrate_block` 去两次整 waiter 克隆：`bump_count` 改收 `SemOp`（Copy 小值），park/complete_slot/migrate_block 三个调用点同步，迁移处补 `debug_assert_eq!(waiter.blocked_on, from)` 钉住前置；第 4 项——核实 `Incoming::Notify` 分支注释在 IPC-P2-2 后仍准确（该分支与枚举未因改名变化），无需改动。第 2 项（`ShmTable` 由服务层 Box 持有）属服务层接线约定，随 IPC-P1-1 落地（见其修复记录）。验证：`cargo test -p minix-ipc-server` = **90 passed / 0 failed**；clippy 本 crate 零告警。

---

## 3. 测试缺口（Gate E 对账）

83 个现有测试与行为契约清单做了双向对账（契约 → 测试、测试 → 契约）。整体质量好：每个测试锚定 C 行号、断言行为而非实现细节；全部测试在逐文件深读时过目，断言值与 C 语义重点核对过 try_ops/retry/sweep/perms/classify 五个模块（逐条对），没有发现"测试代码自身错误"。缺口如下：

### IPC-T-1 `migrate_block` 零测试【✅ 已完成 2026-09-16】

挂起计数迁移（C check_set 的 :408-420，`waiter.rs:148-156`）是 semop 语义里最容易错的函数——它要在两个信号量的 `semncnt`/`semzcnt` 之间搬计数，搬错会让 GETNCNT/GETZCNT 返回错值、且影响后续重试的判定。`retry_wakes_fifo`（`op.rs:371-411`）没有走到迁移分支（卡点没变过）。补法：构造一个两信号量集合，等待者卡在信号量 0，另一操作抬高信号量 0 后其卡点落到信号量 1（或同集合不同下标的等价场景），断言 `raise_waiters`/`zero_waiters` 一减一增。

**修复记录（2026-09-16）**：测试 `migrate_block_moves_suspension_count` 落地——两信号量集合，等待者携带双操作数组卡在操作 0（等增长），直接调 `migrate_block(…, 0, 1)` 模拟重试中卡点后移，四断言：旧点 `raise_waiters` 归零、新点 `zero_waiters` 记一、等待者仍在队、（隐含）调用前旧点计数为一。doc 06 §5.1 补行。验证：`cargo test -p minix-ipc-server` = **91 passed / 0 failed**（90+1）。

### IPC-T-2 `next_seq` 环绕无测试【✅ 已完成 2026-09-16】

`next_seq(0x7fff)` 应回 0（C：`(seq+1) & 0x7fff`，sem.c:135）。现有测试只覆盖了"换新 seq 拒旧 id"（`find_id_rejects_stale_seq`，`table.rs:414-425`），环绕本身没人碰。一行断言的事，但它守的是标识符老化的正确性根基。

**修复记录（2026-09-16）**：`table.rs` 补 `next_seq_wraps_at_fifteen_bits`（0→1、0x7ffe→0x7fff、0x7fff→0 三点）；`segment.rs` 的同型函数是独立定义，补单点环绕断言（注释说明为何重复）。验证：`cargo test -p minix-ipc-server` = **93 passed / 0 failed**（91+2）。

### IPC-T-3 `tests/` 集成目录被声称但不存在【✅ 已完成 2026-09-16】

`server.rs:251-253` 说 `run_once` 是 pub"so integration tests in `tests/` can drive whole scenarios"，`RunStep` 的文档（:142-144）同款说法——但 `os/servers/ipc-server/tests/` 目录不存在。要么补一个最小集成测试（scripted transport 驱动 `run_once` 走 semget→semop→进程事件全链，StubHandler 换 RecordingHandler），要么改注释。推荐前者：服务层接线（IPC-P1-1）恰好需要这个驱动面。


**修复记录（2026-09-16，前者落地）**：新建 `tests/integration.rs`（4 个场景，全走公共 API：semget 请求往返含 send_reply 动词断言、SUSPEND 无出站、进程事件回执走 send_async 动词——IPC-P2-2 的双动词区分在公共面钉住、传输耗尽计 ReceiveFailed）。配套三件：`IpcServer::into_parts` 公共访问器（消费归还 transport/handler 供测试检查记录流量）、lib.rs 补 `RunStep` 再导出、Cargo.toml 补 `[dev-dependencies] minix-types`（集成测试是独立 crate，只能用包 lib + dev-deps）。**顺带移除零引用死依赖 minix-sys**（E-RMIBWIRE 早已登记其死；并行线程 E1 在途改动令其暂时不可编译，本 crate 因此恢复独立可建；P1-1 接 MIB 客户端时恢复）。验证：单元 93 + 集成 4 = **97 passed / 0 failed**。
### IPC-T-4 sweep 的 rc==0 边界无测试【✅ 已完成 2026-09-16，随 IPC-P1-6 闭合】

与 IPC-P1-6 联动：修复时必须同时补 `sweep` 在 `count: Some(0)` + SHM_DEST 下存活的回归测试（现测试只覆盖 1/4/None 三种输入，`refcount.rs:144-206`）。

### IPC-T-5 `run()` 的连续失败 panic 路径无测试【✅ 已完成 2026-09-16】

`run` 在 32 次连续接收失败后 panic（`server.rs:157`、`:232-247`），是事件循环唯一的自保机制，无测试。补法：`fail_next_receives` 设 32 后 `catch_unwind` 驱动 `run`（`run_requires_init` 已有同款 catch_unwind 先例，`server.rs:612-615`）。若觉得在测试里数 32 次太脆，把 `MAX_CONSECUTIVE_RECV_FAILURES` 提为可在测试模块引用的常量即可（已是 `const`，直接用）。

**修复记录（2026-09-16）**：测试 `run_panics_after_sustained_receive_failures` 落地——`fail_next_receives` 预置满额（直接引用常量 `MAX_CONSECUTIVE_RECV_FAILURES`，无数 magic），`catch_unwind` 断言 `run` 终止且丢弃计数恰等于上限。验证：`cargo test -p minix-ipc-server` = 单元 **94** + 集成 **4** = 98 passed / 0 failed。

---

## 4. 文档同步（IPC-D-x）

### IPC-D-1 "空壳 stub" 表述过时

`README.md:6`、`plan.md:6`、`plan.md:167`（§3.5）、`plan.md:410` 四处仍称 `os/servers/ipc-server/` 为"空壳 stub（lib.rs 仅 pub fn init() {}）"。现状：17 文件 4945 行、83 测试全过（基线见 §0）。修法：各处改为当前事实并注日期；`plan.md:167` 的"测试基线待建立"改为本轮数字。注意保留"handler 层未落地"的准确表述——过时的是"空壳"，不是"未完成"。

### IPC-D-2 plan.md 风险表 A-1/A-5 状态失效

`plan.md:17` 说"minix-types 尚无 IPC 消息类型"——七个 `MessLcIpc*` 结构在 `message.rs:3467-3711`、语义层在 `ipc/ipc_server.rs:44-566`，齐全。风险表 `A-5`（plan.md:185）状态"待实施"——minix-sys 侧 `sys_vircopy`（syscall.rs:481）、VM 三 wrapper（vm.rs:171/:207/:335/:356）、`endpoint_identities_via`（rs.rs:170）已落地，余项（trap 桥/SEF/proceventmask/clock 等）挂 E-IPCWIRE。修法：风险表加状态注记列或行内回指，不删历史（风险表是决策记录）。

### IPC-D-3 A-7 锚点漂移

`plan.md:185` 引"sem.c:713-722"作 SEM_UNDO 契约锚点，实际该检查在 :729-739（`r = EINVAL` 在 :737，警告打印在 :733-735），C 文件本轮未变——是 plan 定稿时的行号就偏了。修法：改 :729-739，顺手把 doc 05/06 里同引号段的地方 grep 一遍（本轮 grep doc 06 用的是 :730-739，正确，不需动）。

### IPC-D-4 00/99 篇 pending 与 .design 快照缺口

plan.md §6.1（:347-360）：01-10 全 reviewed（2026-09-05），00/99 pending；`tools/design-coverage-check.sh` 显示 00/07/08/09/10/99 六篇缺 `.design/` 三件套。这不是本轮新发现，登记为待办防漂移：00（总览）与 99（全局概念/常量收口）两篇按其余各篇流程补齐；99 篇尤其该在服务层接线前完成——IPC-P1-1 的边界契约清单（拷贝失败错误码、分配器模型）doc 06:106 已声明"记入 99"，99 不落地这些契约就悬空。

### IPC-D-5 03 篇 MountTable 失真

03 篇 §2.5/§3 D4/§4.1 三处声称复用 minix-sys 的 `MountTable` 而 crate 实际零引用（`mib_tree.rs:16` 注释声称、`:18` 实际 import 仅 minix-types；Cargo.toml 的 minix-sys 依赖因此是死依赖）。E-RMIBWIRE（edge_todo.md）已登记并给出修正方案，本条只回指不重复；服务层接 MIB 时（IPC-P1-1 的最后一步）一并处理——届时 minix-sys 依赖从死变活，失真自动消除。

---

## 5. 跨 stage 条目指针（登记于 `../edge_todo.md`）

- **E-IPCWIRE（本轮新增）**：ipc-server 生产面接线八缺，全部落在共享基础设施（minix-sys wrapper 层 + minix-types 布局面），按 edge 判定第①类登记：DirectTrapTransport 真实 trap 接线（阻塞于 E1 内核 trap 桥，`minix-sys/src/ipc.rs:519-527` 注释自认）、SEF 层（`sef_startup`/`sef_receive_status` 全仓库无实现，ipc-server 的 `init()` 只设 bool，`server.rs:200-202`）、`sys_datacopy` 命名 wrapper（`SELF`+`sys_vircopy` 已备，`syscall.rs:254`/:481）、`proceventmask` 客户端 wrapper（minix-sys 零命中）、`VM_CALL_SHARED_UNMAP` wrapper（常量在 `vm.rs:62` 无 wrapper，mmap_via/munmap_via 先例可循）、时钟客户端（全 crate `now: u64` 注入无来源）、`getnuid`/`getngid`/`getnpid` 命名 helper（`rs.rs:167-169` 注释明说未提供）、minix-types 补 `struct semid_ds`/`struct shmid_ds` 二进制布局（IPC_STAT 拷出/IPC_SET 拷入的 wire 契约，minix-types grep 零命中——E-REQWIRE/E-INWIRE 同款 wire 缺口模式）。
- **E-RMIBWIRE（既有，2026-09-16 状态）**：MIB 客户端的代码半已在 minix-sys 闭环（rmib_call 遍历 + register/deregister/reregister，c7d2ea150），仅余 E1 通电后的端到端验收。**对 13-stage 的含义**：ipc-server 的 MIB 接线（`MountTable::claim` 消费、`handle_mib` 调 `rmib_process`、`assemble_mib_info` 接到 `route_info_query` 的两臂）已在库层面解锁，只差 E1 的发送半——IPC-P1-1 接线序里 MIB 排最后是对的。
- **E5（联调测试包）**：sem/shm 的端到端冒烟（双进程 semop 握手、shmat 写读）将来作为新验收面字母挂入 E5，不新开条目。

## 6. 本轮明确未发现（防止误以为漏查）

以下各项核对过、无问题，列出以圈定审查范围：

- **SEM_UNDO 排除契约**：未实现且显式拒绝（`op.rs:97-99` 回 `Invalid`/EINVAL），与 C 一致（sem.c:731-739）；测试集魔数（`SHRT_MAX` 只跳过警告不改变拒绝）也注释在案（`op.rs:83-84`）。plan A-7 的"契约非演进"定性正确。
- **seminfo 值**：`fill_info` 十字段与 sem.c:430-463 逐项一致（semmni=10、semmns=600、semmsl=60、semopm=100、semvmx=32767、undo 三字段 IPC_INFO 时全零、SEM_INFO 时 semusz=live_count、semaem=Σsem_nsems——`ctl.rs:272-290` + `ctl_info_differs` 测试）。
- **do_semget 逐行核对一致**：既有分支 EXCL→perm→nsems（sem.c:104-111 vs `table.rs:222-235`）；含"负 nsems 在既有分支放行"的 C 怪癖（`nsems > sem_nsems` 对负数恒假——Rust 同式同果）；首集合订阅边沿（sem.c:143-148 vs `table.rs:271-278`）。
- **do_shmget 的 perm-first 顺序**（与 semget 相反）：`segment.rs:193-212` 与 shm.c:64-71 一致，doc 07 §2.3 有意记录过这个不对称。
- **SHM_INFO 的 shm_tot 用原 size 整除页**（不是 rounded）：shm.c:357-359 vs `attach.rs:216`，一致。
- **kern_ipc_info 无 root 检查**（NetBSD 语义）：`route_info_query` 签名不带端点（`mib_tree.rs:150-165`），用类型表达了"没有权限参数"；四个子节点、MSG=0、保留槽 5..9 的双重语义（子节点缺席 vs 查询值 5/6 有效）在 `mib_tree.rs:47-62` 讲清了。
- **try_ops 与 try_semop 的等价性**：scratch 拷贝 + 全成才 commit 替代 C 的乐观修改+逆运算回滚，数组顺序语义、负操作下溢放行、SEMVMX 只查正操作——抽验逐条一致（sem.c:314-373 vs `op.rs:132-173`）；`retry` 的 FIFO 与连锁进展同 check_set（sem.c:381-423 vs `op.rs:186-237`）。
- **等待者单挂起不变量**：C `assert(ip->ip_sem == NULL)`（sem.c:755）→ Rust park 断言（`waiter.rs:98-101`）；世代端点同槽不同占有的断言（sem.c:880 → `waiter.rs:187-191`）都有。
- **`run` 的 32 次失败 panic**：与 C 首败即 panic（main.c:229）的偏离已带 `[ARCH: IPC-01-01]` 标注（`server.rs:223-227`），文档 01 §3 D7 在案——偏离已申报，维持。
- **PROC_EVENT_REPLY 常量单一真值**：minix-types 仅一处定义（`ipc/event.rs:43`，= COMMON_RS_BASE，com.h:619），ipc-server 无本地复制定义（E7 担心的双址在 ipc-server 侧不存在）。
- **端点常量**：`Endpoint::PM = 0`/`Endpoint::MIB = 7` 与 com.h:59/:66 一致；`classify` 的分支顺序（通知 → 进程事件 → MIB → 查表）与 main.c:234-261 一致且有乱序反例测试（`dispatch.rs:105-114`）。
- **encode_id/find_id 双校验**：低 16 位索引 + 高 16 位序号 + 占位检查（sys/ipc.h:110 vs `table.rs:179-190`/`segment.rs:150-161`），stale-seq 拒绝有测试。

## 7. 规则发现（Step 5.7）

1. **纯函数化会吃掉检查顺序**（候选新模式，代码族）：把一个 C 入口函数拆成多个纯函数时，C 源里隐式的"错误码优先级"没有任何类型或测试承载，拆分者的自然顺序（先验形状再验权限）往往与 C 相反。本轮实例：IPC-P1-2。建议 review-patterns 收一条"errno 优先级是分解的隐式契约，拆分前先抄 C 的检查顺序清单，并为每个优先级对写回归测试"。
2. **C 的整数环绕是契约不是缺陷**（候选新模式，代码族）：小整数类型的回绕语义（本例 `u8_t rc - 1`，shm.c:187）在 Rust 侧用饱和/Option"修好"时会悄悄改变外部行为。与已知的"translate 防线"互补：translate 防的是照搬实现，这条防的是过度修正。实例：IPC-P1-6。
3. **"文档-代码一致"不等于"代码-ground truth 一致"**（候选新模式，流程族）：本轮三个缺口（IPC-P1-3/4/5）都是文档写对了、代码没跟上——若查漏只做文档↔代码对账，这三条会漏网。coverage 穷举必须以 C 源为锚点直查代码，文档只在两侧都过完后用来解释分歧归属（文档错 / 代码错 / 未实现）。
