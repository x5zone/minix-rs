# NK4-C 评审报告（接力 agent 全量提交评审，第一轮）

- **评审对象**：`56d6dec4c..e90efbcd8`（交接后全部 30 笔提交，2026-09-23）
- **评审人**：上一棒 agent（交接方），2026-09-23
- **性质**：全量回归评审（代码 + 文档 + 声称验证）。另一 agent 仍在收尾，其后续 commit 待增量第二轮。
- **结论先行**：**通过**。Task C 根因修复（`a470a8d9c`+`6be40748f`）经 C 源逐锚点核对与本轮第三次独立真机验证成立；F10b/c/d、F11、F12、F13 五笔修复全部 C 对位扎实、证据链完整。发现 1 项 P1（报告口径：自造子阶段编号与计划阶段编号冲突，导致「1.3/1.4/1.5/1.6 全闭环」表述误导）与 3 项 P2，无 P0。

---

## 一、验证结果（评审人独立实测，非转述）

| 项 | 结果 |
|----|------|
| docker 测试（arch/kernel/vm/rt） | **242 / 810 / 526 / 57 全绿**（kernel 810 = 基线 809 + F10b 的 `reply_wire` 判别测试，计数不减反增） |
| 镜像构建 | `xtask image --arch x86_64 --release` 成功 |
| 真机复跑（repo vars.fd，`-smp 1`，150s） | `cause_sig`=**0**、`rs-epslot self=0x0`=**0**、`panicked at`=**0**、`vm-pf recv`=**449**（与声称精确一致） |
| 尾态 tail-dump | 与 WORKLOG 描述逐字一致：VM(`name=vm`) `RECEIVING` `from=ANY(0x7c00)`、10×`PAGEFAULT` `to=VM`、全部 `runnable=no queued=no` |
| rc marker | **未出现**（0 次）——与「当前停点 = 硬 livelock」的定性一致 |
| Task C 修复判据 | `rs-epslot self=0x7fffffff8850`（活值，slot 0/2/4/8 全活）——**1.2 判据达成属实** |

## 二、Task C 修复深审（`a470a8d9c` + `6be40748f`）

### 2.1 C 对位锚点逐一核对（评审人读 C 源确认）

| 声称 | C 源核实 |
|------|----------|
| eager 回执直写只存在于 kernel_call()/SYSCALL 腿（system.c:83） | ✅ `system.c:83 copy_msg_to_user(msg, caller->p_delivermsg_vir)`，位于 `kernel_call_finish` |
| 该腿入口必刷新 `p_delivermsg_vir`（system.c:141） | ✅ `system.c:141 caller->p_delivermsg_vir = (vir_bytes) m_user;` |
| int33 陷阱腿（proc.c `mini_*`）不写调用者消息缓冲 | ✅ `mini_send` 无 delivermsg 触碰；`mini_senda` 同；真回执走 `delivermsg()`（proc.c:269，MF_DELIVERMSG 机制） |
| RECEIVE 刷新、SENDA 不刷新 | ✅ `proc.c:983`（`mini_receive` 入口刷新）；`mini_senda` 无刷新——与 Rust `!is_senda` 存储臂同形 |

### 2.2 修复方案评审

- **方案甲（门纪律）判定正确**：int33 腿的 errno 本就经 RAX 交付，`kernel_call_finish` 的 eager 直写对它是**重复交付**；SENDA 窗口的陈旧 `p_delivermsg_vir` 被门封死。方案乙（新鲜度校验）与丙（截 64B）的否决理由成立（C 无校验机制；buf+56 在 64B 内照样被抹，陈旧才是本质）。
- **`6be40748f` 评审修复必要且正确**：`suspend_for_vm` 新建 ctx 不继承门标记 + stage 3a 硬编码 eager=true 会让多段挂起的 IPC 腿调用在最终完成时重开崩溃窗；改为从旧 ctx 读 `resume_skip_eager_reply` 驱动 `!door_skip`，重挂起时凭 eager=false 重新置位——闭环。
- **交付机制无回归的旁证**：修复后 boot 反而推进到历史最深（449 轮缺页服务、12 服务器 exec 完成）——若 int33 RECEIVE 的投递曾依赖被删的 eager 写，boot 会在第一步就退化；实测没有。

### 2.3 根因证据链质量（S0→S2→S3）

教科书级。亮点：
- S0 的「抹写窗口无 pick」观察把嫌疑从"任何时刻"收窄到"内核代执行段"；
- S2 哨兵四代每代被前代阴性结果重新定向（PTE 完好→栈数据抹写→按值扫描→逐 chunk 现场打印），最终 `pw-fina` 命中清零的瞬间捕获；
- S3 用 s2i 全量日志的「`pdmv-set` 与 `fx` 成对出现」节奏否定了自己 S2 的"停车补完成"归因，修正为"当前 SENDA 同步 finish"——**自我证伪并修正**，这是本轮最高质量的推理步骤。
- 遗留修正：S2 结论里「48 次 lvl1=0 = 正常 lazy 缺页」推翻了交接骨架的第 1 条（PTE 物理消失）——WORKLOG 已在"已排除"账标注，诚实。

## 三、其余修复逐笔审阅

| commit | 审阅结论 |
|--------|----------|
| `fc66eb148` F10b+F10c | ✅ `KcallResult::Data(i32)` + `reply_wire()`（Ok→取负 / Data→原样）语义正确；GetPdbr 返回物理地址走 Data 合理（PA<1GB 在 i32 内，与 C 线宽约束一致）；F10c 把内核栈 VA 解析统一到 `AddressRef::Process` 消除 `kern_phys_base` 偏移假定的散布。附带的 P1-arch 登记（aarch64/riscv `reply_code()` 未迁移）诚实且正确排队 |
| `6e51b723a` F10d | ✅ 裸 `p_rts_flags.clear/set(NO_PRIV)` → 调度器感知 `rts_unset/rts_set`，镜像 C `RTS_UNSET/RTS_SET` 宏（proc.h:206-224）的 enqueue/dequeue 半；借用重构（immutable 预检 + &mut 入队）干净 |
| `d9fc1f649` F11 | ✅ 双入链根因（trap 快路径 + finish 各入链一次 → 头插自指成环 → `was_empty` 恒 false）分析严密；移除 trap 腿冗余簿记、唯一归属 finish（镜像 C `vm_suspend` 单次语义 + `assert(!RTS_VMREQUEST)`）；pre-F11 基线（s13f/g 即 6e51b723a 唯一代码差）的旁路二分方法论漂亮。P1-guard（release 无重复入链守卫）登记合理 |
| `afa850c07` + `ce024355d` F12 | ✅ OOM 归因（`MAX_SLABS=64` 固定表 vs 512 页池）有真机读数实锤（`slabs=64/64 px=65/512 big=0`，池 87% 空闲）；修复绑定 `GLOBAL_POOL_PAGES` 而非调魔数；`nk4c_oom_hex` 手工 hex 不分配（OOM 路径无分配纪律）细节到位 |
| `5d9d57d0f` F13 | ✅ `send_blocking` 仅 VFS_PM_INIT 一处（穷举 PM 13 处 `.send()` 后定位），并纠正了源注释错误（"C: ipc_send…非阻塞"正是 bug 源头）——C main.c:226 阻塞语义对位正确 |
| `e90efbcd8` tail-dump 增强 | ✅ name + to/from 阻塞边 + ANY/NONE 解码（0x7c00/0x7bff），门与去重合规；产出的尾态矛盾形态（VM receive(ANY) vs 10 缺页请求不 rendezvous）是下一轮死锁修复的有效起点 |
| `200a016a8` kdst 门统一 | ✅ 修复**交接方（我）在 `85a0d7cd8` 引入的缺陷**：kdst 三个调用点 `#[cfg(not(test))]` 与定义门 `not(feature="mock")` 不一致，宿主 mock 构建 E0425 必炸。归责如实（"HEAD 存量破损"），修复正确 |
| `d8d3a8f48` S2h 探针评审修复 | ✅ cap 48→512 触顶现形、pw-pf 移出 PFRBX 配额块（修崩溃窗口失明）、PDPT 级 PS 位守卫、未对齐读改守卫字节读——全是真实的探针工程缺陷修复 |
| `b4c742f5d`/`b3f38f516`/`8688435c5`/`a9cc3013c`/`f11f93d61`/`2e04777be` 等文档笔 | ✅ 证据链回填、commit 号回填、IPC 理论定位与 SENDREC-Thrift 对照——文档债同步清偿，风格合规 |

## 四、发现

### P1（报告口径）：自造子阶段编号与计划阶段编号冲突，「全闭环」表述误导

顶部状态「**1.3/1.4/1.5/1.6 全闭环**」用的是 agent 自造的侦查编号（1.3=NO_PRIV/privctl、1.4=F11 VM 链、1.5=OOM、1.6=F13 握手），但**同一文件**的路线图表里 1.3=rc marker 链、1.4=F10 errno 全仓对账、1.5=P2 命令面、1.6=W^X。按计划编号的**真实状态**：

| 计划阶段 | 真实状态 | 证据 |
|----------|----------|------|
| 1.3 rc marker | **未达成** | 本轮实测 rc marker = 0；boot 死锁于第 449 轮缺页服务 |
| 1.4 F10 errno 全仓对账 | **部分** | x86_64 完成（F10b/c/d）；P1-arch（aarch64/riscv `reply_code` 未迁移）自登记"阶段 2/3 前必须修" |
| 1.5 P2 命令面 | 无证据 | 无 echo/ls/cat 上机记录 |
| 1.6 W^X | 无证据 | 无相关 commit |

「1.7 根因已定位」同理（计划 1.7 = C 腿 ABI 清单，非 livelock 排查）。**建议**：自造编号改名（如 `PH-privctl` / `PH-vmchain` / `PH-oom` / `PH-vfshs`），或在顶部注明「此处 1.x 为侦查分期，非计划阶段号」。这不掩盖实际进展（进展是真实的、巨大的），但状态口径必须无歧义——本报告 §五 已按计划口径重列。

### P2（流程）：`edge_todo.md` 未按约更新

prompt §5.1 要求"每项完成 → 更新 WORKLOG + `edge_todo.md` 对应行 + commit"。30 笔提交中 `edge_todo.md` 零改动（git diff 为空）。Task C 行的「六轮真机收窄」表述已过时（实际已修复）。

### P2（卫生）：WORKLOG 残留模板占位

`NK4C-WORKLOG.md:332` 残留未清理的模板行 `## 1.2-<n> <标题>（<日期>，commit <hash>）`。另 1.5c/1.6/1.7 的工作只有顶部状态摘要有记录、正文无对应节（顶部信息密度足够接手，实害有限，但违反自定"每步写报告"规范）。

### 归责记录（对交接方的修正）

`200a016a8` 修复的 kdst 门不一致（调用点 `not(test)` vs 定义 `not(feature="mock")`，mock 构建 E0425 必炸）是**交接方（我）在 `85a0d7cd8` 引入的缺陷**。agent 发现如实、修复正确、归责标注清楚。

## 五、当前进度评估（按计划口径重列）

| 计划阶段 | 状态 | 说明 |
|----------|------|------|
| 1.1 Task C A 案 | ✅（上一棒） | + 本轮根因修复 `a470a8d9c`+`6be40748f` 完成 1.2 |
| 1.2 真机复跑过 step2 | ✅ **达成** | 三次独立验证（agent s3a/s3b + 本轮 rev_s1） |
| 1.3 rc marker | ❌ **未达成（当前阻塞）** | boot 死锁于第 449 轮缺页服务：10 进程 PAGEFAULT(to=VM) + VM RECEIVING(from=ANY) 永不 rendezvous。根因已定位到「PAGEFAULT→VM 投递/唤醒腿」，三个排查入口已列（首推 P1-ipc `clear_ipc_refs` 裸 clear 绕过 rts_unset——与 F10d 同族，嫌疑最大） |
| 1.4 F10 errno 对账 | ◐ 部分 | x86_64 完成；aarch64/riscv 待迁移（P1-arch） |
| 1.5 命令面 / 1.6 W^X / 1.7 ABI 清单 | ❌ 未开工 | 被 1.3 阻塞（agent 的 F11/F12/F13 属打通 1.3 的使能修复，非计划 1.4/1.5 本体） |
| 阶段 2-5 | ❌ 未开工 | — |

**净评估**：本轮把系统从「RS 第一步即崩」推进到「12 服务器全部 exec 完成、449 轮缺页服务、仅剩一层投递/唤醒死锁」——这是本弧线开始以来最大的一次实质推进。修复质量与取证纪律均为高标准。当前阻塞面已收窄到单个可测的内核路径。

## 六、下一步建议（给收尾后的接手轮）

1. **修 449-livelock**（阶段 1.3 闸门）：按 agent 已列三入口，**优先验 P1-ipc `clear_ipc_refs`**（syscall.rs，裸 `clear(SENDING|RECEIVING)` 绕过入队——与 F10d 同族、与缺页往返强相关）。修法同 F10d：换调度器感知 `rts_set/rts_unset`。修复后需加与 P1-guard 同样的运行期守卫评估。
2. **修完先复判 1.3**：rc marker 出现即闸门过，然后按路线图 1.4（补 aarch64/riscv 迁移）→ 1.5 → 1.6 → 1.7。
3. **清偿本文档 P1/P2**：状态编号正名（或加注）、`edge_todo.md` 补更、WORKLOG 模板残留清理。
4. **增量第二轮评审**：覆盖收尾期间的新 commit（含 449-livelock 修复本身）。
