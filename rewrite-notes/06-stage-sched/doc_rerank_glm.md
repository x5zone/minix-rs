# 06-stage-sched 文档重建蓝图（glm）

```text
your_name(AI agent name) = glm
target_dir(关注的工作目录) = rewrite-notes/06-stage-sched
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = d6ecd22cae78345e746c82ad7cbe3c1612c151c2

任务 = R 相·重建蓝图：输出 06-stage-sched/doc_rerank_glm.md，不改任何正文。
约束 = 未引用 .design/ 与 tmp_design_and_todo/；未读取其它 AI 的 doc_rerank_* 产物；
       所有落盘产物仅本文件一个，带 _glm 后缀。
```

---

## 0. 元数据

- 执行者：glm；日期：2026-09-19；目标目录：`rewrite-notes/06-stage-sched/`。
- **结论先行**：本 stage 的 01~14 篇是 2026-09-03 按启动序主线全量重写、2026-09-04 通过 full-review 收敛（P0=0，见 plan.md §6.1/§7.1）的成稿，且本次对照 C 源码逐锚点抽查后事实面基本扎实。因此本蓝图的操作集是**"保编号、修事实、补两头"**：
  1. **补两头**——`00-sched-overview.md` 与 `99-global-concepts.md` 仍是 20 行占位骨架（plan §6.1 标 pending），按本蓝图契约全文重写；
  2. **修事实**——12 篇 §1.2 有一处 p_scheduler 生命周期断言与 C 源码不符（§3 缺口表 G-6）；三个已闭环 edge 条目（E-MINTYPES-SYS、E-SCHEDNICED、E-PREEMPTFLAG live 半）尚未回写正文（G-3/G-4/G-5）；全篇测试基线漂移（正文写 59/79，实测 81，G-7）；
  3. **保编号**——不重排、不重编号、不拆分合并：现有编号顺序已满足四条硬标准（§9 G3/G4 验证），而重编号将破坏约 213 处篇内互引、16 个外部文件引用与 3 处代码注释（§8 断链成本）。
- 审查范围：
  - **正文**：`[0-9][0-9]-*.md` 15 篇（2733 行）+ `99-global-concepts.md`，共 16 篇编号文档；
  - **参考材料**（不参与重建，只作证据与素材）：`plan.md`（383 行）、`todo.md`（60 行）、`draft/` 6 篇（938 行，含 `draft/00`、`draft/99` 两篇骨架的知识来源）、`archive/` 2 篇（todo 存档）；
  - **范围外**：`.design/`、`tmp_design_and_todo/`、其它 AI 的 `doc_rerank_*` 产物、`minix3/` 原树（只读不改）。

### 0.1 读取清单（步骤 0 四份清单）

**清单一：文档清单**（行数实测 `wc -l`）

| 编号 | 文件 | 行数 | 状态（plan §6.1） | 头部声明的边界摘要 |
|------|------|------|------|------|
| 00 | 00-sched-overview.md | 20 | **pending 骨架** | 总览+导航；"启动主线图"委托给 plan.md §1.2（骨架缺陷，见 G-1） |
| 01 | 01-sched-init-main.md | 209 | 已改写 2026-09-03 | 启动注册/机器信息/主循环骨架；不讲分发细节(02)、handler(06-08) |
| 02 | 02-sched-message-surface.md | 273 | 已改写 2026-09-03 | 五消息/分流/回复/拒收/主循环执行侧；不讲 handler 实现 |
| 03 | 03-schedproc-struct.md | 208 | 已改写 2026-09-03 | 七字段+死字段消除；不讲表管理(04)、取值语义(05) |
| 04 | 04-schedproc-table.md | 191 | 已改写 2026-09-03 | 槽位换算/两门禁/白名单/错误码；不讲字段细节(03) |
| 05 | 05-priority-timeslice-model.md | 218 | 已改写 2026-09-03 | 16 队列/上下限/毫秒/nice/系统进程判断；不讲 handler 用法 |
| 06 | 06-start-scheduling.md | 249 | 已改写 2026-09-03 | 登记全流程+fork 次主线；不讲下发(09)/选核(10)/客户端(13/14) |
| 07 | 07-stop-scheduling.md | 213 | 已改写 2026-09-03 | 释放流程+对称表；不讲客户端调用面(13/14) |
| 08 | 08-noquantum-nice.md | 208 | 已改写 2026-09-03 | 降级+改上限事务+信任不对称；不讲下发细节(09)/恢复(11) |
| 09 | 09-schedule-process.md | 220 | 已改写 2026-09-03 | 下发两层+内核三检查四写入；不讲注册(12)/选核(10) |
| 10 | 10-pick-cpu-smp.md | 197 | 已改写 2026-09-03 | 三规则/台账/S-5；不讲内核迁移(01-stage-kernel/16) |
| 11 | 11-balance-queues.md | 204 | **pending**（正文实为完整成稿） | 定时器/恢复/防振荡/S-9 |
| 12 | 12-kernel-interface.md | 214 | **pending**（正文实为完整成稿） | do_schedctl/notify_scheduler/proc_no_time 契约面；分工句声明内核原语归 01-stage-kernel/11 |
| 13 | 13-pm-interaction.md | 212 | **pending**（正文实为完整成稿） | libsys 三接口+PM 调用面；分工句声明 nice 用户面归 04-stage-pm/16 |
| 14 | 14-rs-interaction.md | 217 | **pending**（正文实为完整成稿） | RS 申请取消；分工句声明槽配置归 03-stage-rs/08 |
| 99 | 99-global-concepts.md | 20 | **pending 骨架** | 常量表/五表一致性/职责边界（正文缺失，见 G-2） |

> 注：plan §6.1 把 11/12/13/14 标 pending，但四篇正文实为完整成稿（与 01~10 同构的七节体例），plan 状态表本身过时——本蓝图在第 6 节变更表列出该对账项。

**清单二：C 源码清单**（本 stage 语义边界跨五个目录；逐文件确认，非仅凭 plan §5.1 映射表）

| C 文件 | 行数 | 角色 | 承载文档 |
|------|------|------|------|
| `minix3/minix/servers/sched/main.c` | 137 | 服务端 | 01/02 |
| `minix3/minix/servers/sched/schedule.c` | 369 | 服务端 | 05/06/07/08/09/10/11 |
| `minix3/minix/servers/sched/utility.c` | 74 | 服务端 | 02/04 |
| `minix3/minix/servers/sched/schedproc.h` | 40 | 服务端 | 03/04/10 |
| `minix3/minix/servers/sched/sched.h`、`proto.h` | 30/23 | 服务端 | 00/99（签名与全局声明） |
| `minix3/minix/lib/libsys/sched_start.c` | 98 | 客户端（PM/RS 共用） | 13 |
| `minix3/minix/lib/libsys/sched_stop.c` | 30 | 客户端 | 07/13/14 |
| `minix3/minix/servers/pm/schedule.c` | 112 | 客户端（PM 调用面） | 13 |
| `minix3/minix/servers/pm/utility.c`（nice_to_priority） | 91-101 | 客户端 | 05/13 |
| `minix3/minix/servers/pm/forkexit.c`（:102 默认主、:425 stop、:441 清主） | 3 处 | 客户端 | 13 |
| `minix3/minix/servers/pm/mproc.h`（:78 mp_scheduler） | 1 处 | 客户端 | 13 |
| `minix3/minix/servers/rs/utility.c`（sched_init_proc，:364-384） | 21 | 客户端（RS 调用面） | 14 |
| `minix3/minix/servers/rs/manager.c`（:461）、`request.c`（:342） | 2 处 | 客户端 | 14 |
| `minix3/minix/servers/rs/type.h`（:92-95 r_* 四字段） | 4 处 | 客户端 | 14 |
| `minix3/minix/kernel/system/do_schedctl.c` | 46 | 内核契约 | 12 |
| `minix3/minix/kernel/system/do_schedule.c` | 30 | 内核契约 | 09 |
| `minix3/minix/kernel/system.c`（sched_proc，:642-699） | 58 | 内核契约 | 09/12 |
| `minix3/minix/kernel/proc.c`（notify_scheduler :1860-1891、proc_no_time :1893-1910、表初始化 :134） | 3 段 | 内核契约 | 12 |
| `minix3/minix/kernel/table.c`（boot_image `image[]` :44、SCHED 行 :56） | 2 处 | 引导登记 | 00（新建） |
| `minix3/minix/include/minix/com.h`（:49,59-63,92,449,801-807,1151） | 常量 | 契约 | 02/05/13/99 |
| `minix3/minix/include/minix/config.h`（:66-77） | 常量 | 契约 | 05/99 |
| `minix3/minix/include/minix/ipc.h`（:261-273,1093-1113,1430-1445,1822-1828,1908-1913,2437-2612） | 结构 | 契约 | 02/09/12/13 |
| `minix3/minix/include/minix/type.h`（:122-131 machine） | 结构 | 契约 | 01/10 |
| `minix3/minix/include/minix/ipcconst.h`（:28 FROM_KERNEL） | 常量 | 契约 | 02 |
| `minix3/minix/include/minix/endpoint.h`（_ENDPOINT_P） | 宏 | 契约 | 04 |
| `minix3/minix/kernel/proc.h`（:34 p_scheduler、:47-56 accounting、:178-179 宏） | 3 段 | 内核契约 | 12 |
| `minix3/minix/kernel/priv.h`（:45 SRV_F、:49 USR_F）+ `const.h`（:143 PREEMPTIBLE） | 常量 | 内核契约 | 12 |
| `minix3/sys/sys/errno.h`（:64,211-213） | 常量 | 契约 | 04/06/99 |
| `minix3/minix/servers/sched/Makefile` | — | 构建 | WONTFIX（plan §5.4） |

**清单三：非 C 制品清单**（逐项检索结果；"无"均经检索确认）

| 制品类 | 实际情况 | 归属 |
|------|------|------|
| 链接脚本 | `servers/sched/` 无自定义链接脚本（目录仅 Makefile+3 .c+3 头） | 不在本 stage（§7 表 N-1） |
| 汇编入口与陷阱入口 | 无；用户态 trap 归 minix-sys/rt（edge E1 轨道），内核 trap 归 01-stage-kernel | 14-stage-runtime / 01-stage-kernel |
| 引导链与引导协议 | boot_image 登记一行（kernel/table.c:56）；加载协议归 01-stage-kernel/09-vm-boot-protocol | 00 一句话 + 引用 |
| 镜像布局 | sched 无自定义段 | 不在本 stage |
| 用户态启动与运行时装载 | SEF 生命周期（C 侧） | 01 |
| 跨模块接口与线格式 | 七个消息体 + endpoint/generation 语义 | 02/09/12/13/99 |
| 构建脚本与工具链 | `servers/sched/Makefile`（无语义） | WONTFIX |
| 测试基建与模拟器脚本 | `os/servers/sched/` 81 个内联测试；无 QEMU 脚本（真机测试归 kernel/pm stage） | 各篇 §5 |
| Rust 实现入口 | `os/servers/sched/src/` 20 文件 4525 行（server.rs 1133、transport.rs 463、start.rs 330、client.rs 293、priority.rs 288、schedule.rs 256、cpu.rs 222、schedctl.rs 217、stop.rs 168、dispatch.rs 165、nice.rs 161、schedproc.rs 158、table.rs 153、balancer.rs 138、sef.rs 93、noquantum.rs 99、valid.rs 69、main.rs 58、lib.rs 36、mod.rs 25） | 各篇 §3/§4 |

**清单四：引用关系清单**（对现有编号/文件名的全部引用；检索命令见 §0.2）

| 引用方 | 处数 | 引用形态 |
|------|------|------|
| 篇内互引（15 篇正文之间 `` `NN-*.md` `` 反引号引用实例） | ≈213 | 被引最多的篇：06（28）、02（21）、09（20）、10（19）、05（19）、13（16）、04（16） |
| `edge_todo.md` | 21 | E-SCHEDNICED/E-PREEMPTFLAG/E-SCHEDSMP/E-MINTYPES-SYS/E5(e)/E8 条目与对账注记 |
| `04-stage-pm/16-scheduling.md` | 4 | 分工句（:17 "SCHED 服务实现归 06-stage-sched"）与参见 |
| `00-master-plan/README.md` | 2 | 目录表 :24、旧→新映射 :69 |
| `edge1.md`/`edge3.md` | 2+2 | 状态注记 |
| `07-stage-ds/todo.md`、`08-stage-is/todo.md` | 1+2 | 对端登记 |
| `14-stage-runtime/plan.md` | 1 | E-MINTYPES-RUNTIME 关联 |
| `03-stage-rs/draft/README.md` | 1 | 旧素材引用 |
| 代码注释（`os/` 下 .rs） | 3 | `os/servers/sched/src/main.rs:4`→01 篇、`lib.rs:8`→01 篇、`main.rs:44`→11 篇 |

### 0.2 使用的命令与关键输出（证据摘录）

```bash
# 基线测试（81 passed，与 todo.md §0 记录一致）
cd os && cargo test -p minix-sched --lib
#   → test result: ok. 81 passed; 0 failed
# 各模块测试数（§2 K-081 的分布账）
grep -c 'fn test_' os/servers/sched/src/**/*.rs
#   → server.rs 22 / priority.rs 7 / start,schedule,schedctl,cpu,client,balancer 各 5 /
#     stop 4 / sef,nice,schedproc,dispatch 各 3 / valid,table,noquantum 各 2 （合计 81）
# p_scheduler 全部写入点（§3 G-6 的判定证据）
grep -rn 'p_scheduler =' minix3/minix/kernel/ minix3/minix/servers/
#   → kernel/proc.c:134（boot 清表）、kernel/system/do_schedctl.c:39,42、
#     kernel/system/do_update.c:252、pm 侧 5 处均为 mp_scheduler（另一张表）
#   → servers/sched/schedule.c 零命中：SCHED 侧 do_stop_scheduling 不写 p_scheduler
# 外部引用扫描
rg -c '06-stage-sched' rewrite-notes --glob '!06-stage-sched/**'
# 内部互引计数（每篇被引次数）
rg -c "\`NN-*.md\`" 06-stage-sched/*.md
# 错误码值
sed -n '211,213p' minix3/sys/sys/errno.h   # EDEADEPT(_SIGN 215)/EBADEPT(216)/EBADCPU(217)
```

---

## 1. C 真序

### 1.0 阶段类型判定

**服务事件循环型**（主形态）+ **契约对端集合型**（辅形态）。判定理由：`servers/sched/` 的本体是一个 SEF 启动、`while(TRUE)` 收消息分派的服务（main.c:22-96），完全符合 prompt 第九节"服务事件循环型"的形态；但 sched 的语义一半在服务端之外——`libsys` 三个客户端接口、PM/RS 两个调用面、内核 `do_schedctl`/`do_schedule`/`notify_scheduler` 两个对端，五目录合同构成"调度契约"。因此真序表分两段：服务端启动段+循环段（事件循环型的两段式），另加对端链（契约集合型的分组视图）。讲述顺序按第九节服务事件循环型的建议组织（为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 与邻接服务的协议），一次请求的生命周期作主线。

### 1.1 真序表

**A 启动段**（进程诞生 → 进入主循环前）

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| A1 | boot_image 登记 SCHED（`{SCHED_PROC_NR, "sched"}`），RS 按执行序放行 | `kernel/table.c:44`（`struct boot_image image[NR_BOOT_PROCS]`）、`:56`（SCHED 行）；`com.h:63`（SCHED_PROC_NR=4）；00-master-plan/README.md:40,53 | 登记序（ds→rs→pm→sched→vfs→…）与执行序（kernel→VM→RS→其余）是两层语义，SCHED 属"VM 解除后即可运行"的 boot_image 直登服务 |
| A2 | `main()` 入口：备消息缓冲，调 `sef_local_startup` | `servers/sched/main.c:22-32` | |
| A3 | SEF 注册：登记 fresh 回调 + 通用有状态 restart 回调，交权 `sef_startup` | `main.c:111-121`（`:114` fresh、`:115` `SEF_CB_INIT_RESTART_STATEFUL`、`:120`） | restart 本体在 SEF 通用库，SCHED 无自有代码 |
| A4 | fresh 初始化第一件事：`sys_getmachine(&machine)` 读机器信息，失败 panic | `main.c:126-131`；`type.h:122-131`（`processors_count`/`bsp_id`） | machine 是 main.c:17 的文件级全局 |
| A5 | fresh 初始化第二件事：`init_scheduling()` 设平衡闹钟 | `main.c:133` → `schedule.c:334-342`（`:338` `balance_timeout = BALANCE_TIMEOUT(5) × sys_hz()`；`:340` `sys_setalarm`；`:341` 失败 panic） | 先机器后定时器的顺序是硬约束 |
| A6 | 进入 `while (TRUE)` 主循环 | `main.c:35` | |

**B 循环段**（每一轮）

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| B1 | `sef_receive_status(ANY)` 收消息，失败 panic；取 `who_e`/`call_nr` | `main.c:39-42` | |
| B2 | 通知判定 `is_ipc_notify`：CLOCK → `balance_queues()`；其它通知忽略；通知一律不回复（continue） | `main.c:44-55`（`:45` 判定宏在 `com.h:92`） | 通知是告知不是请求 |
| B3 | `switch(call_nr)` 四路分发：INHERIT/START → `do_start_scheduling`；STOP → `do_stop_scheduling`；SET_NICE → `do_nice`；NO_QUANTUM → 校验 `IPC_FLG_MSG_FROM_KERNEL` 后 `do_noquantum` 且不回复，无标记则按伪造回 `EPERM`；default → `no_sys` | `main.c:57-87`；标记常量 `ipcconst.h:28` | 五个消息号 `com.h:801-807`（0xF01~0xF05） |
| B4 | 回复判定：`result != SUSPEND` 才回（`SUSPEND = -998`，`com.h:1151`）；`reply()` 用 `ipc_send`，失败仅打印 | `main.c:90-93,101-106` | |

**C handler 内部**（请求处理，按调用时机排列）

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| C1 | `do_start_scheduling`：类型断言 → 白名单 → 空槽检查 → 填三字段 → 上限 `< 16` 检查 → 三分支填值（init 临时值 / START 显式 / INHERIT 抄父）→ `sys_schedctl(0,…)` 接管 → 置 `IN_USE` → `pick_cpu` → `SCHEDULE_CHANGE_ALL` 下发 + EBADCPU 标死重试 → 回复 `scheduler=SCHED_PROC_NR` | `schedule.c:140-249`（检查 :146-166；init :171-187，含 :185 cpu_mask FIXME；START :191-197；INHERIT :199-213；接管 :218-223；选核重试 :226-237；回复 :246） | 唯一的双消息入口 |
| C2 | `do_stop_scheduling`：白名单 → 验占用 →（SMP）`cpu_proc[cpu]--` → `flags = 0` | `schedule.c:112-135` | 不触碰内核归属（见 G-6） |
| C3 | `do_noquantum`：`sched_isokendpt(m_source)` 验占用（无白名单）→ `priority < MIN_USER_Q` 才 `+= 1` → `schedule_process_local` | `schedule.c:87-107` | 来源信任在 B3 的内核标记，不在本函数 |
| C4 | `do_nice`：白名单 → 验占用 → `new_q >= 16` 拒绝 → 快照旧双值 → `max = priority = new` → 局部下发，失败回滚 | `schedule.c:254-292` | 事务性 |
| C5 | `schedule_process`：`pick_cpu` → 按掩码把未选中字段填 -1 → 算 `niced` → `sys_schedule` | `schedule.c:297-328`（掩码 :22-35；`pick_cpu` 在 :302 被再次调用） | 每次下发前无条件重选核，是 C 的意外（02 篇 D9 已述，Rust 不复制） |
| C6 | `pick_cpu`：单核 → bsp；系统进程 → bsp；否则选负载最低的可用非 bsp 核（平局取小号，全不优回退 bsp）；选中记账 `cpu_proc[cpu]++`；非 SMP 编译直接 `cpu = 0` | `schedule.c:48-81`（恒真宏 `cpu_is_available` :39） | |
| C7 | `balance_queues`：全表扫 IN_USE，`priority > max_priority` 才 `-1` 并局部下发（返回值不检查），再设闹钟 | `schedule.c:353-369` | 循环的续接点在 :367-368 |

**D 对端链**（契约对端，按触发时机）

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| D1 | fork 继承链：PM `do_fork` 置默认主 `mp_scheduler = SCHED_PROC_NR` → VFS 回复后 `sched_start_user`（nice→队列；PRIV_PROC 父 → INIT 特例）→ `sched_inherit` 发 INHERIT 消息 → C1 的 INHERIT 分支 | `pm/forkexit.c:102`；`pm/schedule.c:55-84`（:62 nice 换算、:71-76 继承来源）；`lib/libsys/sched_start.c:11-41` | 消息三字段，时间片不传 |
| D2 | 系统进程链：RS `sched_init_proc` 双断言（用户无主/系统有主）→ `sched_start` 六参直达（parent=RS_PROC_NR、r_priority/r_quantum/r_cpu）→ C1 的 START 分支 | `rs/utility.c:364-384`（:371-372 断言、:375-377 直达）；`rs/type.h:92-95` | |
| D3 | INIT 链：PM `sched_init` 扫表（IN_USE 且非 PRIV_PROC）双断言（只有 INIT、父=自己）→ `sched_start(SCHED, self, self, USER_Q, USER_QUANTUM, -1)`，失败仅告警 | `pm/schedule.c:20-50` | 全系统第一个被 SCHED 接管的进程 |
| D4 | 内核注册链：C1 内 `sys_schedctl(0, ep, 0, 0, 0)` → 内核 `do_schedctl`（未知位 EINVAL → isokendpt → 无 KERNEL 位则 `p_scheduler = caller`；带位则先 `sched_proc` 后 `p_scheduler = NULL`） | `schedule.c:218`；`kernel/system/do_schedctl.c:15-45` | `SCHEDCTL_FLAG_KERNEL = 1`（com.h:449） |
| D5 | 下发链：C5 → `sys_schedule`（五字段装消息，`_kernel_call(SYS_SCHEDULE)`）→ 内核 `do_schedule`（isokendpt → `caller != p_scheduler` 则 EPERM → 拆五值）→ `sched_proc`（范围检查 → SMP CPU 检查 EBADCPU → 置 `RTS_NO_QUANTUM` 出队 → 逐个应用非 -1 字段（毫秒入 `p_quantum_size_ms`）→ 按 niced 置/清 `MF_NICED` → 清位重入队） | `lib/libsys/sys_schedule.c`；`kernel/system/do_schedule.c:14-29`；`kernel/system.c:642-699` | 服务端 `>= 16` 拒、内核 `> 16` 才拒（system.c:645-646） |
| D6 | 时间片耗尽链：内核 `proc_no_time`（非内核调度且 PREEMPTIBLE → `notify_scheduler`；否则内核自行续量）→ `notify_scheduler` 出队、填 7 个记账字段、清账、`mini_send(…, FROM_KERNEL)` 代发 → NO_QUANTUM 消息 → B3 校验 → C3 | `kernel/proc.c:1893-1910`（PREEMPTIBLE 位：`priv.h:45` SRV_F、`:49` USR_F、`const.h:143` 值 0x002）；`proc.c:1860-1891` | 记账结构 `proc.h:47-56` |
| D7 | 退出链：PM exit → `sched_stop`（:425）失败仅告警，随后 `mp_scheduler = NONE`（:441）；RS 清理服务 → `manager.c:461` 失败继续；RS 改槽 → `request.c:342-345` 失败带码返回；`sched_stop` 对 KERNEL/NONE 短路直返 OK | `pm/forkexit.c:425,441`；`rs/manager.c:461`；`rs/request.c:342-345`；`lib/libsys/sched_stop.c:16-17` | |
| D8 | 时钟链：内核时钟中断到期 → CLOCK 通知 → B2 → C7 | 内核侧归 `01-stage-kernel/15-clock-timer.md`；本 stage 只从"收到通知"起 | |

### 1.2 序差表（运行时序 vs 教学序）

| # | 运行时事实（锚点） | 教学序安排 | 理由 | 回指补偿 |
|---|---|---|---|---|
| 序-1 | 客户端先发消息（D1/D2/D3），服务端 handler 后执行 | 13/14 排在 06 之后 | 本 stage 的主体是服务端；读懂客户端契约需要 02 的消息词汇与 04 的门禁词汇 | 06§1.7 预置 fork 路径图；13/14 篇首回指 06 的入口 |
| 序-2 | `sys_schedctl` 注册（D4）发生在 C1 内部 | 12 排在 09 之后 | 12 的通知半需要 09 的 `sys_schedule` 对称面与 05 的参数语义作前置 | 06§1.4 声明"交接语义细节归 12" |
| 序-3 | `pick_cpu`/`schedule_process` 是 C1 的内部调用 | 09/10 排在 06-08 之后 | 下发与选核是四个 handler 共享的汇聚机制，按"汇聚点+触发时机"后置（prompt §5.2） | 06 每个调用点标注"细节归 09/10" |
| 序-4 | 机器信息读取（A4）先于一切 handler | 01 即第一篇机制文 | 教学序=启动序，无差 | — |
| 序-5 | 平衡闹钟在 A5 设定、触发在循环内（D8） | 11 排在 10 之后；02 只讲触发门 | 值与触发分离：02 讲"铃从哪来"，11 讲"铃响干什么" | 02§1.3 前指声明 + 11§2.5 回指 |
| 序-6 | `nice_to_priority` 在 PM 进程里执行（D1），先于 INHERIT 消息存在 | 05 集中给出公式（含死钳位），13 只引用 | 公式只有一份（05§1.1 的立规） | 05§2.4 锚 pm/utility.c；13§2.4 引用 |

---

## 2. 知识点全集

> 池的构成：存量 = 15 篇正文 + draft/00 + draft/99 去重后的知识点；新增 = 本次对照 C 源码、非 C 制品与 edge 边界材料发现的、现有正文未承载的知识点。编号 stage 内唯一，B 相以"名称+锚点"对齐。**去向**列给出新目录中的位置（编号不变，去向即"原篇保留/修正"或"迁入 00/99"）。

### 2.1 知识点池总表

**域 A：服务骨架与消息面（00/01/02）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-001 | 双层调度模型（内核执行面 vs 用户策略面） | 概念 | 存量 | 00 骨架、01§1.1、99 骨架 | `schedule.c:1-9` 文件头；`do_schedctl.c:40-43`（注册）；`do_schedule.c:20-21`（权限） | 回答"为什么调度器是用户态进程" | **主讲述点迁 00**；01§1.1 压缩为引用 |
| K-002 | SCHED 在系统启动链中的位置（boot_image 登记、RS 放行、SCHED_PROC_NR=4） | 概念 | **新增** | 无（00 骨架未讲） | `kernel/table.c:44,56`；`com.h:63`；00-master-plan/README.md:40,53 | 回答"sched 是谁、何时被启动" | 00 新建承接 |
| K-003 | SEF 启动注册（fresh + STATEFUL restart） | 机制 | 存量 | 01§1.2/§2.7 | `main.c:111-121`；`sef.h:SEF_CB_INIT_RESTART_STATEFUL` | 回答"服务怎么向 SEF 报到" | 01 保留 |
| K-004 | 机器信息读取（sys_getmachine/machine 两字段） | 机制 | 存量 | 01§1.3/§2.8 | `main.c:130-131`；`type.h:122-131` | 回答"CPU 拓扑从哪来" | 01 保留 |
| K-005 | fresh 初始化顺序（先机器后定时器；读失败 panic） | 约束 | 存量 | 01§1.3/§4.3 | `main.c:126-136` | 回答"启动顺序能否换" | 01 保留 |
| K-006 | 主循环骨架（收/分/回三步） | 机制 | 存量 | 01§1.4、02 全篇 | `main.c:35-96` | 回答"服务的一轮怎么转" | **主讲述点在 02**；01 只留骨架 |
| K-007 | 五种消息与编号（0xF01~0xF05） | 接口与协议 | 存量 | 02§1.2/§2.3 | `com.h:801-807`；`main.c:57-87` | 回答"服务听哪几种号" | 02 主；99 引用表 |
| K-008 | 通知/调用三态分流（is_ipc_notify；通知不回复） | 机制 | 存量 | 02§1.3/§2.2 | `main.c:44-55`；`com.h:92` | 回答"告知与请求怎么分" | 02 保留 |
| K-009 | NO_QUANTUM 来源校验（FROM_KERNEL；伪造 EPERM） | 约束 | 存量 | 02§1.4/§2.4、08§1.2 | `main.c:68-84`；`ipcconst.h:28` | 回答"为什么这条消息最危险" | 02 主（门）；08 主（信任模型阐述） |
| K-010 | SUSPEND 不回复契约 | 约束 | 存量 | 02§1.5/§3 D4 | `main.c:90-93`；`com.h:1151`（-998） | 回答"何时可以不回话" | 02 保留 |
| K-011 | no_sys 固定拒收 ENOSYS | 机制 | 存量 | 02§2.6 | `utility.c:18-23` | 回答"野编号的下场" | 02 保留 |
| K-012 | reply 失败仅告警不致命 | 机制 | 存量 | 01§2.6、02§5 | `main.c:101-106` | 回答"回件失败服务死不死" | 01/02 保留 |
| K-013 | 七个消息体布局（56 字节定长） | 接口与协议 | 存量 | 02§2.7 | `ipc.h:261-273,1093-1113,1430-1445,1822-1828,1908-1913,2437-2612` | 回答"线上字节长什么样" | 02 保留（位置清单）；字段语义归 06/09/12/13 |
| K-014 | Rust 主循环一轮一步（run_once/Step）+ 64 次接收失败上限 | 工具与工程 | 存量 | 02§3 D7 | `server.rs:55,138,161` | 回答"无限循环怎么测" | 02 保留 |
| K-015 | Rust SchedServer 单一所有者（四个 C 全局折进一个结构，S-11） | 架构演进 | 存量 | 02§3 D8 | `server.rs:79`；`schedule.c:16,46`；`main.c:17` | 回答"C 四个全局怎么收" | 02 保留 |
| K-016 | Rust 双 trait 接缝（IpcTransport/KernelApi） | 架构演进 | 存量 | 02§3 D9 | `kernel_api/transport.rs` | 回答"消息线与内核线怎么分" | 02 保留 |
| K-017 | 常量权威已迁 minix-types/minix-sys（SUSPEND 本地定义删除、SYS_* 消费 kernel_call 权威） | 架构演进 | **新增**（E-MINTYPES-SYS 闭环产物，正文未回写） | 02§3 D4/D5 仍写"本地定义" | `dispatch.rs:13`（import SUSPEND）；`transport.rs:33`（import SYS_*）；`minix-types/src/ipc/kernel_call.rs`；edge_todo E-MINTYPES-SYS ✅ 2026-09-15 | 回答"常量的唯一真源在哪" | 02 修正 D4/D5 锚点 |

**域 B：数据与门禁（03/04）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-018 | schedproc 七字段（身份 2+标记 1+优先级 2+时间/CPU 2） | 数据结构 | 存量 | 03 全篇 | `schedproc.h:23-36` | 回答"调度器眼里的进程长什么样" | 03 保留 |
| K-019 | IN_USE 单标记语义 | 数据结构 | 存量 | 03§1.3/§2.3 | `schedproc.h:39` | 回答"槽位空占怎么判" | 03 保留 |
| K-020 | cpu_mask 死字段结构消除（S-3：无来源/无读者/无作者） | 架构演进 | 存量 | 03§1.6/§3 D5 | `schedproc.h:33-35`；`schedule.c:185` FIXME | 回答"为什么 Rust 少一个字段" | 03 保留 |
| K-021 | 静态表与 _MAIN 具化（分配一处声明多处） | 数据结构 | 存量 | 03§2.1/§2.7 | `schedproc.h:8-12,36` | 回答"表在哪里分配" | 03 保留 |
| K-022 | 槽位换算 _ENDPOINT_P（剥 generation） | 机制 | 存量 | 04§1.2/§2 | `endpoint.h:_ENDPOINT_P`；`utility.c:31,48` | 回答"端点号怎么变槽位号" | 04 保留 |
| K-023 | sched_isokendpt 四判断有序（任务→越界→名实→占用；后两道同码 EDEADEPT） | 机制 | 存量 | 04§1.3/§2.1 | `utility.c:29-41` | 回答"查表前过哪几道门" | 04 保留 |
| K-024 | sched_isemtyendpt 镜像门（末道反转、空槽不验名） | 机制 | 存量 | 04§1.4/§2.2 | `utility.c:46-56` | 回答"查空与查占为何两个函数" | 04 保留 |
| K-025 | accept_message 白名单（PM/RS 两家，恒定名单） | 约束 | 存量 | 04§1.5/§2.3 | `utility.c:61-74`；`com.h:59,61` | 回答"谁有资格发请求" | 04 主；06/07/08 引用 |
| K-026 | 错误码语义表（EINVAL 22/EDEADEPT 215/EBADEPT 216/EBADCPU 217/EPERM 1/ENOSYS，_SIGN 用户态为正） | 约束 | 存量 | 04§2.4、06 D6、99 骨架 | `sys/sys/errno.h:64,211-213`；Rust `errno.rs:132-137` | 回答"每个拒绝码什么意思" | 99 建总表；04/06 保留各场景 |

**域 C：参数模型（05）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-027 | 优先级常量体系（NR_SCHED_QUEUES=16/TASK_Q=0/MAX_USER_Q=0/USER_Q 派生 7/MIN_USER_Q=15） | 数据结构 | 存量 | 05§1.2/§2.1 | `config.h:66-77` | 回答"队列怎么编号" | 05 主；99 引用表 |
| K-028 | max_priority（约束）与 priority（状态）的分工 | 概念 | 存量 | 05§1.3 | `schedproc.h:29-30` | 回答"两个优先级数是什么关系" | 05 保留 |
| K-029 | 时间片毫秒约定（S-6；内核 `p_quantum_size_ms`） | 约束 | 存量 | 05§1.4/§2.5 | `system.c:683-685`；`config.h:74` | 回答"时间片单位是什么"（修正旧 draft 的 ticks 错误） | 05 保留 |
| K-030 | 同值双名（USER_QUANTUM 配额默认 vs DEFAULT_USER_TIME_SLICE 出生初值，均 200 但来源不同） | 约束 | 存量 | 05§1.4/§3 D1 | `config.h:74` vs `schedule.c:41` | 回答"两个 200 能否合并" | 05 保留 |
| K-031 | nice 换算公式（41 级→16 队列；nice 0 → 7 是算出来的；末尾钳位是死代码） | 机制 | 存量 | 05§1.5/§2.4 | `pm/utility.c:91-101`；`sys/resource.h:43-44` | 回答"nice 怎么变队列" | 05 主；13 引用 |
| K-032 | is_system_proc（parent==RS_PROC_NR；只看一代） | 机制 | 存量 | 05§1.6/§2.3 | `schedule.c:44`；`com.h:61` | 回答"谁是系统进程" | 05 主；10/14 引用 |
| K-033 | niced 标记（max > USER_Q；随每条下发消息带出，空掩码也带） | 机制 | 存量 | 05§3 D6、09§1.4 | `schedule.c:319`；`system.c:692-694` | 回答"备注位跟谁走" | 05 主（公式）；09 主（携带） |
| K-034 | USER_DEFAULT_CPU=-1 哨兵（默认或保持的双义，Rust 以 CpuChoice 枚举承接） | 约束 | 存量 | 05§3 D5 | `config.h:77`；`system.c:652-655` | 回答"-1 是什么意思" | 05 保留 |

**域 D：服务臂（06/07/08）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-035 | 双消息合流单入口（全系统 sole scheduler 注释） | 概念 | 存量 | 06§1.1/§2.1 | `main.c:58-61`；`schedule.c:146-147,168-170` | 回答"为什么两种消息一扇门" | 06 保留 |
| K-036 | 登记检查序（类型→白名单→空槽→上限；顺序即诊断） | 约束 | 存量 | 06§1.2/§3 D2 | `schedule.c:146-166` | 回答"检查为什么不能换序" | 06 主；07/08 同构引用 |
| K-037 | 三种填值与 init 覆盖净等价（临时值必被 START 分支与无条件 pick_cpu 覆盖；C 注释 :177-183 已过时，以 :226 为准） | 机制 | 存量 | 06§1.3/§2.3 | `schedule.c:171-211,226` | 回答"init 的临时值去哪了" | 06 保留 |
| K-038 | 先交接后置位（标记是交接的结果；失败残留是契约） | 约束 | 存量 | 06§1.4、02§4.3 | `schedule.c:218-223` | 回答"IN_USE 何时算数" | 06 保留 |
| K-039 | EBADCPU 标死重试环（终止性证明；SMP 启动失败核心） | 机制 | 存量 | 06§1.5/§3 D6 | `schedule.c:227-231,229` | 回答"目标核挂了怎么办" | 06 主；10 引用 |
| K-040 | scheduler 回复字段与转交形状（微内核多调度器伏笔） | 机制 | 存量 | 06§1.6/§2.8 | `schedule.c:238-248` | 回答"调度权归谁怎么告诉 PM" | 06 保留 |
| K-041 | fork 次主线路径图（PM→nice 换算→继承来源→sched_inherit→服务端分支） | 概念 | 存量 | 06§1.7 | `pm/schedule.c:55-84`；`sched_start.c:11-41`；`pm/main.c:369-373` | 回答"fork 的儿子怎么被接管" | 06 主；13 引用 |
| K-042 | do_stop_scheduling（两检查两释放；SMP 才减账；释放一定成功） | 机制 | 存量 | 07 全篇 | `schedule.c:112-135` | 回答"进程退出怎么销账" | 07 保留 |
| K-043 | 释放四"没有"与登记-释放对称表 | 概念 | 存量 | 07§1.4/§1.5 | `schedule.c:112-135` vs `140-252` | 回答"释放比登记少什么" | 07 保留 |
| K-044 | do_noquantum 降级（降一级、MIN_USER_Q 到底停、只动当前位置） | 机制 | 存量 | 08§1.3/§2.1 | `schedule.c:87-107` | 回答"时间用完的后果" | 08 保留 |
| K-045 | 信任不对称（四 handler 三验名单、耗尽只验内核标记；权力越大检查越严） | 概念 | 存量 | 08§1.2/§3 D1 | `schedule.c:92`（无 accept_message）vs `main.c:70-71` | 回答"为什么这条臂没有白名单" | 08 保留 |
| K-046 | do_nice 快照-改值-回滚事务 | 机制 | 存量 | 08§1.4/§2.2-2.3 | `schedule.c:254-292`（快照 :278-279、双写 :282、回滚 :287-288） | 回答"改一半失败怎么办" | 08 保留 |
| K-047 | 局部下发掩码（LOCAL=PRIO\|QUANTUM；MIGRATE=CPU；降/改/平衡共用） | 接口与协议 | 存量 | 08§1.5、09§1.2、11§1.4 | `schedule.c:32-35` | 回答"动数字不动核" | 09 主（掩码全集）；08/11 引用 |

**域 E：下发与机制（09/10/11）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-048 | schedule_process 参数聚合（掩码→取值或 -1；niced 收尾） | 机制 | 存量 | 09§2.2/§3 D1 | `schedule.c:297-328` | 回答"一次下发打包了什么" | 09 保留 |
| K-049 | -1 保持语义（两侧同一哨兵；与 USER_DEFAULT_CPU 的 -1 同形不同义） | 约束 | 存量 | 09§1.3 | `schedule.c:307,312,317`；`system.c:680-688` | 回答"没带的字段怎么办" | 09 保留 |
| K-050 | sys_schedule 消息五字段（endpoint/quantum/priority/cpu/niced） | 接口与协议 | 存量 | 09§2.3 | `lib/libsys/sys_schedule.c`；`ipc.h:1104-1113` | 回答"两层之间的分界面" | 09 保留 |
| K-051 | do_schedule 权限检查（endpoint→调用者必须是 p_scheduler→取值） | 机制 | 存量 | 09§1.5/§2.4 | `do_schedule.c:14-27`（niced `!!` 压缩 :27） | 回答"内核凭什么听 SCHED 的" | 09 保留 |
| K-052 | sched_proc 检查与写入序列（三检查四步写；RTS_NO_QUANTUM 出入队；MF_NICED） | 机制 | 存量 | 09§2.5 | `system.c:642-699` | 回答"内核怎么落盘参数" | 09 主；12 引用 |
| K-053 | 服务端严内核宽（16 在服务端被拒、内核 `> NR_SCHED_QUEUES` 才拒；内核是末道闸） | 约束 | 存量 | 09§2.5 | `system.c:645-646` vs `schedule.c:164` | 回答"两边边界为何不完全一致" | 09 保留 |
| K-054 | schedule_process/sched_proc 命名对照（同形异层） | 概念 | 存量 | 09§1.6/§2.6 | `schedule.c:297` vs `system.c:642` | 回答"两个像名字的函数谁是谁" | 09 保留 |
| K-055 | pick_cpu 三规则（单核短路→系统进程守 BSP→负载最低；平局取小；回退即初值） | 机制 | 存量 | 10§1.2/§2.2 | `schedule.c:48-81` | 回答"进程放哪个核" | 10 保留 |
| K-056 | cpu_proc 台账三动作（选中加 :77/释放减 07:130/标死 :229；收支相符） | 数据结构 | 存量 | 10§1.3/§2.1 | `schedule.c:37-46,77,229`；`schedule.c:130`（07 篇域） | 回答"负载账怎么记" | 10 主；07 引用 |
| K-057 | cpu_is_available 恒真宏（无符号比较永真；真过滤在负载比较） | 事实 | 存量 | 10§1.4/§2.1 | `schedule.c:39,67-75` | 回答"死亡的两道门哪道真管用" | 10 保留 |
| K-058 | SMP 编译期开关→运行时数字（S-5；CONFIG_MAX_CPUS 缺省 1；非 SMP 返 0 与单核返 bsp 语义不同） | 架构演进 | 存量 | 10§1.5/§2.3/§3 D4 | `schedproc.h:14-16`；`schedule.c:50,78-80`；`type.h:123-124` | 回答"编译开关怎么退化成数字" | 10 保留 |
| K-059 | init_scheduling 定时器设定（5 秒×sys_hz；失败 panic；一次性闹钟） | 机制 | 存量 | 11§1.3/§2.2 | `schedule.c:334-342` | 回答"恢复的节奏怎么定" | 11 保留 |
| K-060 | balance_queues 逐级恢复（全表扫 IN_USE；`priority > max_priority` 才 -1；再设闹钟续环） | 机制 | 存量 | 11§1.3/§2.3 | `schedule.c:353-369` | 回答"沉底的怎么升回来" | 11 保留 |
| K-061 | 降快升慢防振荡（事件触发 vs 时间触发；单步恢复） | 概念 | 存量 | 11§1.1-1.2 | `schedule.c:99-101` vs `360-362` | 回答"为什么不对称" | 11 保留 |
| K-062 | 平衡下发返回值不检查（尽力而为周期纠偏；与 103/284 检查返回值对照） | 事实 | 存量 | 11§2.3/§3 D3 | `schedule.c:362` vs `103,284` | 回答"恢复失败会怎样" | 11 保留 |
| K-063 | 平衡策略可换注释（"This default policy will soon be changed"；S-9 默认结构+替换位） | 架构演进 | 存量 | 11§1.5/§3 D4 | `schedule.c:348-352` | 回答"策略往哪换" | 11 保留 |

**域 F：内核契约（12）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-064 | do_schedctl 注册两分支（未知位 EINVAL→isokendpt→KERNEL 带参先落参数后改归属/否则只改归属） | 机制 | 存量 | 12§1.3/§2.1-2.2 | `do_schedctl.c:15-45` | 回答"调度权怎么交接" | 12 保留 |
| K-065 | p_scheduler 归属语义与生命周期 | 数据结构 | 存量（含一处错误，见 G-6） | 12§1.2/§2.6 | `proc.h:34,178-179`；写入点仅 `proc.c:134`（boot 清表）、`do_schedctl.c:39,42`、`do_update.c:252`（Live Update 拷贝） | 回答"内核记着谁是调度器" | 12 修正后保留 |
| K-066 | notify_scheduler 通知构造（出队→填来源/类型→7 记账字段→清账→FROM_KERNEL 代发，失败 panic） | 机制 | 存量 | 12§1.4/§2.3-2.4 | `proc.c:1860-1891`；`proc.h:47-56` | 回答"耗尽通知怎么造出来" | 12 保留 |
| K-067 | proc_no_time 双分支（非内核调度 + PREEMPTIBLE → 通知；否则内核续量） | 机制 | 存量 | 12§1.6/§2.5 | `proc.c:1893-1910`；`priv.h:45,49`；`const.h:143` | 回答"谁的时间用完会惊动 SCHED" | 12 保留 |
| K-068 | 内核 Rust 侧接线现状（PREEMPTIBLE 已入特权位、niced wire 已闭环、enqueue 抢占门仍挂） | 架构演进 | **新增**（两个 edge 闭环未回写正文） | 无 | E-PREEMPTFLAG（🔄 live 半 2026-09-15：`kpriv.rs:is_preemptible`、`capability.rs:69-70` 位 0x0000_0002；enqueue Phase 3 余项登记）；E-SCHEDNICED（✅ 2026-09-15：`dispatch_schedule` 读线上 niced）；edge_todo.md:439-505 | 回答"内核侧现在的真实行为" | 12 新增"内核侧现状"小节 |

**域 G：客户端契约（13/14）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-069 | libsys 三接口与三路路由（NONE 完了/KERNEL 直达 sys_schedctl/用户态发消息；sched_stop 双条件短路） | 接口与协议 | 存量 | 13§1.2/§2.1-2.2 | `sched_start.c:46-98`；`sched_stop.c:9-30` | 回答"申请为什么有时不发消息" | 13 保留 |
| K-070 | START 4 字段 vs INHERIT 3 字段（同体异载；继承 quantum 置零且服务端不读） | 约束 | 存量 | 13§1.3/§2.3 | `sched_start.c:25-27,80-84`；`ipc.h:1430-1438` | 回答"两种申请差在哪" | 13 保留 |
| K-071 | 回复照收转发语义（"might have forwarded"；回复是谁主就是谁） | 概念 | 存量 | 13§1.4/§2.3 | `sched_start.c:33-39,90-96` | 回答"回复的调度器为何可能不是问的那个" | 13 保留 |
| K-072 | PM sched_init INIT 特例（扫表双断言；USER_Q/USER_QUANTUM/-1；失败仅告警） | 机制 | 存量 | 13§1.6/§2.4 | `pm/schedule.c:20-50` | 回答"第一个用户进程怎么上调度" | 13 保留 |
| K-073 | PM sched_start_user（nice 换算；PRIV_PROC 父→INIT；发 INHERIT） | 机制 | 存量 | 13§2.4 | `pm/schedule.c:55-84`（断言 :72） | 回答"fork 后谁去申请" | 13 保留 |
| K-074 | PM sched_nice（KERNEL/NONE 拒绝 EINVAL；换算后发 SET_NICE 两字段） | 机制 | 存量 | 13§2.4 | `pm/schedule.c:89-112` | 回答"改 nice 的申请长什么样" | 13 保留 |
| K-075 | mp_scheduler 生死记录（fork 默认主→回复定主→退出清空；申请失败拆台） | 概念 | 存量 | 13§1.5/§2.5-2.6 | `mproc.h:78`；`pm/forkexit.c:102,425,441`；`pm/main.c:199,213,372-380` | 回答"PM 怎么记住谁在管" | 13 保留（只描述不建模） |
| K-076 | RS sched_init_proc（双断言分域；6 参直达；父恒 RS） | 机制 | 存量 | 14§1.2-1.3/§2.1-2.2 | `rs/utility.c:364-384` | 回答"系统进程的申请怎么走" | 14 保留 |
| K-077 | RS 取消两位置两处理（清理继续/改槽带码返回；同样的错不同的处理） | 机制 | 存量 | 14§1.4/§2.4-2.5 | `rs/manager.c:461`；`rs/request.c:342-345` | 回答"取消失败为什么两种下场" | 14 保留 |
| K-078 | r_scheduler/r_priority/r_quantum/r_cpu 槽值（负 priority 保留义） | 接口与协议 | 存量 | 14§2.3 | `rs/type.h:92-95` | 回答"四个值从哪来" | 14 保留（来源归 03-stage-rs/08） |

**域 H：全局（99/00）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-079 | 五进程表一致性（Kernel proc/PM mproc/VM vmproc/VFS fproc/SCHED schedproc 各记一事） | 概念 | 存量（draft/00§2.3 有初稿） | 99 骨架、draft/00 | `schedproc.h:1-3` 注释；draft/00-sched-overview.md§2.3 | 回答"为什么每个服务都有一张进程表" | **99 新建承接**（draft/00 为素材） |
| K-080 | SCHED 与 Kernel/PM/RS 职责边界 | 概念 | 存量 | 99 骨架、散见各篇 | `schedule.c:1-9`；`pm/schedule.c:20-50`；`rs/utility.c:364-384`；`do_schedctl.c` | 回答"四家各管什么" | 99 新建承接 |
| K-081 | 测试基线与分布（81 个：server 22/priority 7/start/schedule/schedctl/cpu/client/balancer 各 5/stop 4/sef/nice/schedproc/dispatch 各 3/valid/table/noquantum 各 2） | 测试性质 | **新增**（正文基线漂移） | 各篇 §5.1 写 59 或 79 | `cargo test -p minix-sched --lib`（81 passed）；todo.md §0 | 回答"这套实现被多少测试钉着" | 各篇 §5.1 基线统一修正；99 记总账 |

### 2.2 统计摘要

- 总条数 **81**：概念 14、机制 27、数据结构 9、接口与协议 7、约束与不变量 13、架构演进 8、工具与工程 1、事实 2（K-057、K-062）、测试性质 1（K-081）——按主类型计，跨类型条目以首个类型计。
- 来源类型：**存量 75**（其中 K-079/K-080 的实质素材在 draft/00、draft/99，正式正文未承载）；**新增 6**（K-002、K-017、K-068、K-081，另 K-065 含一处存量错误事实的替换、K-013 补 56 字节定长断言锚）。
- 按现有文档分布：01→5、02→13、03→4、04→5、05→8、06→7、07→2、08→4、09→7、10→4、11→5、12→5、13→7、14→3、99→2、新增/跨篇→若干（重复条目按主讲述点计一次）。
- 主讲述点重复标记见 §3.3。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路来源：① C 符号全集（服务端 16 函数 + 对端 14 函数/调用点 + 7 消息体 + 常量族，与 plan §7.2 的 grep 证据一致，本次逐条复核成立）；② 操作系统通用概念（双层调度、MLFQ 降级/回升、SMP 负载均衡、nice 语义、事务回滚）；③ 非 C 制品主题（§0.1 清单三）；④ 阶段边界契约（master-plan 启动链定位、edge_todo 六条相关条目、三个主权文档分工句）。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据 | 建议 | 处理 |
|---|---|---|---|---|
| G-1 | **00 总览正文缺失**：现 20 行骨架，启动主线图委托给 plan.md §1.2（正式文档不得依赖参考材料承载核心内容） | 00-sched-overview.md:13 | 新建 00 正文（契约见 §5） | 采纳 |
| G-2 | **99 全局概念正文缺失**：常量表/错误码表/五表一致性/职责边界均无正文 | 99-global-concepts.md:9-15 仅列条目名 | 新建 99 正文（契约见 §5） | 采纳 |
| G-3 | **E-MINTYPES-SYS 闭环未回写**：02 篇 D4/D5 仍写 SUSPEND/SYS_* "本地定义/本地镜像"，实际已删改 import | `dispatch.rs:13`、`transport.rs:33`；edge_todo E-MINTYPES-SYS ✅ 2026-09-15 | 02 修正 | 采纳 |
| G-4 | **E-SCHEDNICED 闭环未回写**：kernel 侧 `dispatch_schedule` 已读线上 niced（C: do_schedule.c:27 直译），12 篇无此现状 | edge_todo E-SCHEDNICED ✅ 2026-09-15 | 12 补"内核侧现状" | 采纳 |
| G-5 | **E-PREEMPTFLAG live 半闭环未回写**：kernel 的 proc_no_time 已读特权位 PREEMPTIBLE（近似删除），enqueue 抢占门余项仍挂；12 篇 Rust 侧描述停留在旧状 | edge_todo E-PREEMPTFLAG 🔄 2026-09-15；`capability.rs:69-70`、`kpriv.rs:164-173` | 12 补现状并挂 edge 状态指针 | 采纳 |
| G-6 | **12 篇 §1.2 事实错误**："停止调度后也回到内核调度（见 07）"不成立——SCHED 的 `do_stop_scheduling` 不写 p_scheduler（schedule.c:112-135 无此语句），内核侧写入点仅 boot 清表/do_schedctl/do_update 三处；进程退出后归属指针滞留，由槽位下一次 do_schedctl 覆盖（do_fork.c:89 子进程不可运行直至被调度，故滞留值不可观测） | grep 证据见 §0.2 | 12 修正该句，改为真实生命周期描述 | 采纳（本蓝图唯一发现的存量事实错误） |
| G-7 | **测试基线漂移**：01/03/04/05/06/07/08/09/10/12/13/14 写"59 passed"，02/11 写"79 passed"，实测 **81**（2026-09-09 起，todo.md §0 已记录 81 但正文未跟） | §0.2 命令输出 | 全篇 §5.1 基线行机械修正 + 各篇归属测试数对账（模块级计数目前准确，见 K-081） | 采纳 |
| G-8 | **锚点卫生**：部分锚点为"（LNN，工具生成）"形态且符号名错误——如 03 篇通篇 `schedproc.h:CONFIG_MAX_CPUS（L24）`（:24 实为 endpoint 字段）、01§7 `type.h:kclockinfo（L118）`（机器信息结构实为 :122-131 的 struct machine）、10§2.3 `schedproc.h:EXTERN（L14）`（:14 实为 `#ifndef CONFIG_SMP`）、13§1.5 `mproc.h:sigaction（L78）`（:78 实为 mp_scheduler）、14§2.3 `type.h:ARGV_ELEMENTS（L92）`（:92-95 实为 r_scheduler 等） | 逐条 sed 核对（§0.2） | B 相按"文件:起-止行 + 真实符号"重写这些锚点 | 采纳 |
| G-9 | **boot_image 登记事实无承载**：`kernel/table.c:56` 的 SCHED 登记行没有任何一篇提到 | 全文检索 | 00 新建吸收（K-002） | 采纳 |
| G-10 | **edge 挂起条目的正文状态指针缺失**：E-SCHEDSMP/E5(e)/E8 三条 open 条目在 06/10/12 篇有语义描述但无状态指针，读者无法知道"哪半已实现" | edge_todo.md:186,258,487 | 06/10/12 各加一行状态指针（不展开内容） | 采纳 |
| G-11 | plan §6.1 状态表过时：11/12/13/14 标 pending 但正文已是完整成稿 | 对照表与正文 | plan.md 属参考材料，不改；在 B 相交付说明中记录对账结论 | 记录不改 |

### 3.3 重复主题表（主讲述点裁决）

| 主题 | 出现位置 | 主讲述点 | 其余位置处理 |
|---|---|---|---|
| 双层调度模型 | 00 骨架、01§1.1、99 骨架 | **00**（重写后） | 01§1.1 压缩为一段动机叙述+指针；99 只留一句定义 |
| 五消息表 | 02、99 骨架 | 02 | 99 放编号-名称查询表，语义引用 02 |
| 优先级常量 | 05、99 骨架 | 05 | 99 放常量-值查询表，语义引用 05 |
| nice 换算 | 05、13 | 05 | 13 只调用 |
| accept_message | 04、06/07/08 | 04 | 三臂只写"过了 04 的门" |
| EBADCPU 重试 | 06、10 | 06 | 10 只讲标死动作本体 |
| schedule_process | 09、06/08/11 | 09 | 三臂只讲调用时机 |
| SUSPEND | 02、01 | 02 | 01 只留条件判断一句 |
| 检查顺序范式 | 06、07/08 | 06 | 07/08 声明"同序同构" |
| is_system_proc | 05、10/14 | 05 | 10/14 引用 |
| fork 路径图 | 06§1.7、13 | 06 | 13 引用不重绘 |
| -1 保持语义 | 09、05 | 09 | 05 只讲 USER_DEFAULT_CPU 的 -1 并声明差异 |

### 3.4 越界主题表

| 越界描述 | 所在 | 裁决 |
|---|---|---|
| 12 篇引内核 Rust 实现（proc.rs Option 归属、syscall_process.rs 分派） | 12§2.6/§4.1 | 合规：已有分工句（篇首"分工声明"），内核原语主权归 01-stage-kernel/11；保持"只引用不展开" |
| 13 篇讲 PM 表生死细节（mp_scheduler 三处写点） | 13§1.5/§2.5-2.6 | 合规边界：D4"只描述、不建模"，主权归 04-stage-pm/16；保持 |
| 14 篇引 RS recovery/detach 与 Live Update | 14§2.4/分工句 | 合规：D3 只描述；主权归 03-stage-rs；保持 |
| 09 篇讲内核 sched_proc 写入序列（与 01-stage-kernel/11§3.8 相邻） | 09§2.5 | 合规边界：09 讲"规则与顺序"（契约视角），11 讲"实现"（内核视角），分工句已存在；保持 |
| 11 篇 §1.6 引 Redox DWRR/EEVDF 演进（外部项目动态） | 11§1.6 | 合规：作为 S-9 的路标论证；建议 B 相复核两枚链接仍有效 |

### 3.5 非 C 主题逐项回答

| 主题 | 在哪讲 / 为什么不在本 stage |
|---|---|
| 链接与加载 | 不在本 stage：`servers/sched/` 无自定义链接脚本（目录清单 §0.1）；ELF 装载协议归 01-stage-kernel/09；99 用一句话声明 |
| 镜像与内存布局 | 不在本 stage：sched 无自定义段；`struct machine` 的内存布局属内核信息面（01-stage-kernel）；SCHED 只消费两字段（01/10） |
| 汇编入口与陷阱进入 | 不在本 stage：用户态 trap 归 14-stage-runtime（edge E1 轨道，transport.rs 真实端目前诚实返回 EIO，02 篇 D9 已声明）；内核 trap 归 01-stage-kernel |
| 启动装配 | 在本 stage：C 侧 SEF 链（01）；Rust 侧 main.rs 装配线（01 D4） |
| 构建与工具链 | WONTFIX：`servers/sched/Makefile` 无语义（plan §5.4 既定） |
| 跨模块接口与线格式 | 在本 stage：七个消息体与字段序（02/09/12/13）、endpoint/generation 语义（99） |
| 错误路径 | 在本 stage：各臂错误路径在各篇；跨场景错误码总表在 99（新建） |
| 关闭与退出 | 在本 stage：服务端释放（07）+ 客户端三路（13/14）；内核侧进程回收归 01-stage-kernel |
| 并发与同步 | 在本 stage（仅服务侧）：SCHED 是单线程事件循环，无共享内存并发；S-11 单一所有者（02 D8）即并发答案。内核 BKL/SMP 并发归 01-stage-kernel（16 篇） |
| 测试基建 | 在本 stage：81 个内联测试与逐篇 §5；QEMU 真机联调归 edge E5(e)/E8（不属本 stage 正文） |

---

## 4. 新目录

### 4.1 新篇章总表（编号不变，16 篇）

> **裁决**：现有编号顺序已满足四条硬标准（G3/G4 验证见 §9），且 213 处篇内互引 + 16 个外部文件 + 3 处代码注释以编号/文件名为锚。**不重排、不重编号、不拆合**。重建工作 = 00/99 两篇全文新建 + 14 篇定向修正。

| 编号 | 标题（不变） | 一句话定位 | 分组 | 操作 |
|---|---|---|---|---|
| 00 | SCHED 整体架构概览 | 回答"这个服务是什么、为何存在、全目录怎么读" | 阶段 0 总览 | **全文新建** |
| 01 | SCHED 启动入口与主循环 | 启动注册、机器信息、主循环骨架 | 阶段 1 启动与消息面 | 保持+修正 |
| 02 | SCHED 消息面与分发 | 五消息、分流、回复、拒收、主循环执行侧 | 阶段 1 | 保持+修正 |
| 03 | schedproc 进程记录结构 | 调度记录的七个字段与一个被消除的死字段 | 阶段 2 数据与门禁 | 保持+修正 |
| 04 | schedproc 表管理与槽位校验 | 槽位换算、两道门禁、白名单 | 阶段 2 | 保持+修正 |
| 05 | 优先级与时间片模型 | 全 stage 公共参数的唯一定义处 | 阶段 3 模型 | 保持+修正 |
| 06 | do_start_scheduling 调度接管 | 登记全流程 + fork 次主线 | 阶段 4 服务臂 | 保持+修正 |
| 07 | do_stop_scheduling 停止调度 | 登记的逆操作 | 阶段 4 | 保持+修正 |
| 08 | do_noquantum 与 do_nice | 在册进程的两种优先级变化 | 阶段 4 | 保持+修正 |
| 09 | schedule_process 参数下发与内核对接 | 策略面到执行面的桥 | 阶段 5 机制 | 保持+修正 |
| 10 | pick_cpu 与 SMP CPU 选择 | 三条选核规则与负载台账 | 阶段 5 | 保持+修正 |
| 11 | init_scheduling 与 balance_queues | 定时恢复与防振荡 | 阶段 5 | 保持+修正 |
| 12 | SCHED 与内核的双向契约 | 注册与通知的契约面 + 内核侧现状 | 阶段 6 内核契约 | 保持+修正（G-4/G-5/G-6/G-8） |
| 13 | SCHED 与 PM 的交互 | 用户进程的申请取消 | 阶段 7 客户端契约 | 保持+修正 |
| 14 | SCHED 与 RS 的交互 | 系统进程的申请取消 | 阶段 7 | 保持+修正 |
| 99 | 全局概念 | 常量/错误码/消息号查询表 + 五表一致性 + 职责边界 | 全局查询 | **全文新建** |

### 4.2 阅读路径

- **主线（启动序，全读）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14。
- **可跳读/支线**：
  - 只关心"一个请求的生命周期"：00 → 02 → 06 → 09（回 02 收尾），机制细节后补；
  - 只关心契约对端：05 → 09 → 12 → 13 → 14（契约面自洽）；
  - 查询型阅读：99 随时跳入（常量/错误码/消息号三张表）。
- **并行体的组织声明**：四个 handler（06/07/08）是"按场景分组的请求处理"，组内以 06 为代表成员（最复杂、含检查序范式与 fork 次主线），07/08 以差异表与对称表收束——现状已是此形态，保持。三种触发来源（PM fork、RS 启动、内核耗尽）在 02 的消息面汇聚，符合"汇聚点+触发时机"规则。

---

## 5. 每篇契约

> B 相执行说明：标注【新建】的两篇按契约全文撰写；标注【保持+修正】的 14 篇**不重写正文**，只执行各自的"修正清单"（每条修完跑 fix-guard 验证 + 对应 Gate）。知识点列引用 §2 的 K 编号；"事实底线"是修正时必须复核的锚点集。

### 00-sched-overview【新建】

- 一句话定位：回答"SCHED 是什么、为什么调度策略在用户态、它在系统启动链的哪一环、这 16 篇按什么顺序读"。
- 讲什么：K-001（双层调度模型，主讲述点自本篇起）、K-002（boot_image 登记与启动链位置）、K-079（五进程表一致性，概念级）、K-080（四家职责边界，概念级）、启动主线图（§1.1 真序表 A 段的图形化）、16 篇导航表与阅读路径（§4.2）。
- 不讲什么：一切机制细节（01~14）；SEF 细节（01）；消息号语义（02）；内核启动协议（`../01-stage-kernel/09-vm-boot-protocol.md`）。
- 前置：无（全目录入口）。
- 后置：01~14 与 99 均引用本篇的导航与主线图。
- 事实底线：`kernel/table.c:44,56`；`com.h:63`；`servers/sched/main.c:22,111-133`；`schedule.c:1-9`（双层模型的 C 侧自述）；`schedproc.h:1-3`。
- 知识点清单：K-001、K-002、K-079、K-080 + 主线图 + 导航表。
- 验收标准：① 启动主线图自带（A1~A6 全锚点），不再出现"参见 plan.md §1.2"式的委托（对照 05-stage-vfs/00-vfs-overview.md 的成稿标准）；② 双层模型的"是什么/为什么/代价"三问可独立回答；③ 导航表 16 行齐全且与 §4.1 一致；④ draft/00 的五表对照表素材被吸收改写（不照抄，去 fork 视角）。

### 01-sched-init-main【保持+修正】

- 一句话定位（不变）：SCHED 怎么启动并进入主循环。
- 讲什么：K-003~K-005、K-012、K-006 的骨架半；Rust 侧 K-014 的装配半（main.rs）、SEF 枚举。
- 不讲什么（不变）：分发细节 02；handler 06-08；定时器定义 11；机器信息用途 10。
- 前置：00。后置：02、10、11。
- 事实底线：`main.c:22-136`；`type.h:122-131`；`com.h:801-807`；`os/servers/sched/src/main.rs`、`sef.rs`。
- 修正清单：
  1. §1.1 双层模型段压缩为动机叙述+指向 00（K-001 主讲述点迁移的回指补偿）；
  2. §7 参见的 `type.h:kclockinfo（L118，工具生成）` 改为 `type.h:122-131（struct machine）`（G-8）；
  3. §5.1 基线 "59 passed" → "81 passed（2026-09-19 实测）"，本篇相关测试 4 个（sef.rs 3 + types 1）复核（G-7）。
- 验收标准：修正后三条全部有锚点复核记录；主线图引用指向 00 而非 plan.md。

### 02-sched-message-surface【保持+修正】

- 一句话定位（不变）：主循环消息处理规则的用户手册。
- 讲什么：K-006~K-014、K-015、K-016、K-017、K-013。
- 不讲什么（不变）：handler 实现（06-08）；字段语义（09/12/13）。
- 前置：01。后置：06、03、11（触发门）。
- 事实底线：`main.c:35-106`；`utility.c:18-23`；`com.h:92,801-807,1151`；`ipcconst.h:28`；`ipc.h` 七结构；`server.rs`、`dispatch.rs`、`transport.rs`。
- 修正清单：
  1. D4 的"SUSPEND 常量（dispatch.rs 本地定义）"改为"消费 minix-types 权威（`dispatch.rs:13` import；本地定义已随 E-MINTYPES-SYS 删除）"，并把 SUSPEND=-998 的锚点补 `com.h:1151` + minix-types 权威路径（G-3）；
  2. D9 中"SYS_* 本地镜像"同步改为消费 `minix-sys`/`minix-types kernel_call.rs` 权威（`transport.rs:33`）（G-3）；
  3. §4.1 模块结构中 `os/libs/minix-types/src/ipc/message.rs` 路径复核（现树为 `src/ipc/message.rs`，确认后更新）；
  4. §5.1 基线 "79" → "81"；"本篇直接相关 23 个（dispatch 3 + server 19 + types 1）"与 server.rs 实有 22 个对账（11 篇认领 2 个后差 1 个，找出归属并写明）（G-7）。
- 验收标准：D4/D5/D9 三处 Rust 锚点与 `dispatch.rs`/`transport.rs` 当前文件逐行一致；测试对账表三列闭合。

### 03-schedproc-struct【保持+修正】

- 讲什么：K-018~K-021。前置：00、02。后置：04、05。
- 事实底线：`schedproc.h:1-40`；`schedule.c:185`。
- 修正清单：
  1. §2 全篇"（LNN，工具生成）"锚点重写为真实符号+行段：§2.2 → `schedproc.h:24-25`（endpoint/parent）、§2.3 → `:26,39`（flags/IN_USE）、§2.4 → `:29-30`、§2.5 → `:31-32`、§2.6 → `:33-35`、§2.7 → `:36`（G-8）；
  2. §5.1 基线 59 → 81（G-7）。
- 验收标准：§2 每个锚点 `sed -n` 可复核；§5 测试表三行与 schedproc.rs 实有 3 个测试一致。

### 04-schedproc-table【保持+修正】

- 讲什么：K-022~K-026。前置：02、03。后置：05、06。
- 事实底线：`utility.c:18-74`；`endpoint.h`；`sys/sys/errno.h:64,211-212`。
- 修正清单：
  1. §2.4 错误码段的 errno.h 路径统一为 `minix3/sys/sys/errno.h`（现文写 `errno.h:64,211-212` 无全路径，补全）；EBADEPT=216/EDEADEPT=215 值与 Rust `errno.rs:132-135` 互证（G-8 卫生级）；
  2. §5.1 基线 59 → 81（G-7）。
- 验收标准：三个错误码值在 C 与 Rust 两侧各有一个可 grep 锚点。

### 05-priority-timeslice-model【保持+修正】

- 讲什么：K-027~K-034。前置：03、04。后置：06/08/09/10/11。
- 事实底线：`config.h:66-77`；`schedule.c:41,44,319`；`pm/utility.c:91-101`；`system.c:645-650,652-655,683-686`；`sys/resource.h:43-44`。
- 修正清单：
  1. §5.1 基线 59 → 81（G-7）；
  2. §3 D3 的 Rust 锚点笔误复核：`priority.rs` 中 `is_system_proc` 谓词与 `is_niced` 是两个函数，正文 D3 代码位置写成了 `fn is_niced`——按 `rg "fn is_system_proc" os/servers/sched/src/priority.rs` 实况修正（G-8）。
- 验收标准：`USER_Q` 派生式（(15-0)/2+0=7）、nice 0→7、死钳位三个断言各有锚点；D3 锚点与代码一致。

### 06-start-scheduling【保持+修正】

- 讲什么：K-035~K-041。前置：02/04/05。后置：07/09/10、13/14（路径图）。
- 事实底线：`schedule.c:140-249`；`main.c:57-61`；`ipc.h:1430-1438,1908-1913`；`sys/sys/errno.h:213`。
- 修正清单：
  1. §5.1 基线 59 → 81（G-7）；
  2. 增补一行 edge 状态指针：EBADCPU 重试环在 Rust 服务侧已按最终形态实现并有测试（`test_start_retries_after_dead_cpu`），内核侧 EBADCPU 校验桩未接、挂 `../edge_todo.md` E-SCHEDSMP（G-10）。
- 验收标准：§1.3 的 init 覆盖净等价论证保持（:226 无条件 pick_cpu 是其锚）；状态指针一行落地。

### 07-stop-scheduling【保持+修正】

- 讲什么：K-042、K-043。前置：02/04/06。后置：08、10、13/14（来路）。
- 事实底线：`schedule.c:112-135`；`ipc.h:1440-1445`；`sched_stop.c`。
- 修正清单：
  1. §5.1 基线 59 → 81（G-7）；
  2. §1.2/§2 与 G-6 联动核对：本篇不声称"释放会改变内核归属"（现文已无此声称，复核保持）。
- 验收标准：对称表十行与两侧 C 行号一致；无归属变更断言。

### 08-noquantum-nice【保持+修正】

- 讲什么：K-044~K-047。前置：02/04/05。后置：09、11、12（记账字段对端）。
- 事实底线：`schedule.c:32-35,87-107,254-292`；`main.c:68-84`；`ipc.h:1822-1828`。
- 修正清单：
  1. §5.1 基线 59 → 81（G-7）；
  2. §1.2 信任不对称段补一句内核侧现状：Rust 内核的耗尽门已由"priority!=0 近似"改为读 PREEMPTIBLE 特权位（E-PREEMPTFLAG live 半，2026-09-15），enqueue 抢占门余项挂 edge——C 侧行为本篇描述不变（G-5）。
- 验收标准：降级/回滚断言锚点不变；现状句带 edge 指针。

### 09-schedule-process【保持+修正】

- 讲什么：K-047~K-054。前置：05/06。后置：10、12。
- 事实底线：`schedule.c:22-35,297-328`；`sys_schedule.c`；`ipc.h:1104-1113`；`do_schedule.c`；`system.c:642-699`。
- 修正清单：
  1. §5.1 基线 59 → 81（G-7）；
  2. §2.3 的"失败提示里写着 PM:"历史字符串注记复核（`schedule.c:323` 仍是 "PM:"，保持）。
- 验收标准：三检查四写入序列的行段锚点复核；掩码四形状与 `kernel_api/schedule.rs` 测试对账。

### 10-pick-cpu-smp【保持+修正】

- 讲什么：K-055~K-058。前置：01/05。后置：11、12。
- 事实底线：`schedule.c:37-81,226-231`；`schedproc.h:14-16`；`type.h:122-131`。
- 修正清单：
  1. §5.1 基线 59 → 81（G-7）；
  2. §2.3 锚点 `schedproc.h:EXTERN（L14，工具生成）` → `schedproc.h:14-16（#ifndef CONFIG_SMP 缺省 1）`（G-8）；
  3. 增补一行 edge 状态指针：pick/台账的 Rust 实现已就绪，内核侧每核队列/EBADCPU/迁移三环挂 E-SCHEDSMP（G-10）。
- 验收标准：三规则顺序测试与 `cpu.rs` 5 个测试对账；两条增改落地。

### 11-balance-queues【保持+修正】

- 讲什么：K-059~K-063。前置：02/05。后置：12、13（降级源头回指）。
- 事实底线：`schedule.c:16-18,32-35,334-369`；`sysutil.h:60`；`main.c:44-55`。
- 修正清单：
  1. §5.1 基线 "79" → "81"；§5 表中属 server.rs 的 2 个测试（`test_init_scheduling_arms_bell`、`test_clock_rebalances_multiple_slots_in_order`）与 02 篇的归属对账保持一致（G-7）；
  2. §1.6 的两个外部链接（redox-os.org 两枚）点击复核，失效则改引存档页（G-8 卫生级）。
- 验收标准：恢复判断 `>` 边界、单步、再设闹钟三断言锚点不变；链接有效。

### 12-kernel-interface【保持+修正】（修正量最大的一篇）

- 讲什么：K-064~K-068。前置：05/09。后置：13/14。
- 分工声明（保持）：内核原语主权在 `../01-stage-kernel/11-scheduling-primitives.md`。
- 事实底线：`do_schedctl.c`；`proc.c:1860-1910`；`proc.h:34,47-56,178-179`；`priv.h:45,49`；`const.h:143`；`com.h:449`；`kernel/ipc.h:FROM_KERNEL`。
- 修正清单：
  1. **G-6 事实修正**：§1.2 "状态的变化有三条路"列举的第四条"停止调度后也回到内核调度（见 07）"删除，替换为真实生命周期：内核写入点仅三处——boot 清表（`proc.c:134`）、`do_schedctl` 两分支（`:39,42`）、Live Update 拷贝（`do_update.c:252`）；进程退出不清 `p_scheduler`，SCHED 侧 `do_stop_scheduling` 只清本服务表（`schedule.c:132`）；滞留值由槽位下一次 `do_schedctl` 覆盖，不可观测（`do_fork.c:89` 子进程不可运行直至被调度）。同时修"三条路"实列四条的计数语病；
  2. **G-4**：补"内核侧现状（Rust）"小节——`dispatch_schedule` 已读线上 niced（E-SCHEDNICED ✅ 2026-09-15；C 侧锚 `do_schedule.c:27`），MF_NICED 端到端测试钉住；
  3. **G-5**：同节写明耗尽门已读 PREEMPTIBLE 特权位（`capability.rs:69-70` 位定义、`kpriv.rs` `is_preemptible`、`sched_proc_no_time` 已切换；enqueue Phase 3 抢占门余项仍挂 E-PREEMPTFLAG 🔄）；
  4. §4.1 Rust 锚点对账：`syscall_process.rs` 的 `dispatch_schedctl` 现于 :651（正文写 :630），按现树修正；`proc.rs` 的 scheduler 字段 :530-534 复核（G-8）；
  5. §5.1 基线 59 → 81（G-7）。
- 验收标准：G-6 修正句有 grep 证据（§0.2 命令复跑）；§1.2 不再存在无锚点的归属变迁断言；现状小节两个 edge 状态与 edge_todo.md 逐字一致；五字段 wire 顺序测试与 `schedctl.rs` 5 测试对账。

### 13-pm-interaction【保持+修正】

- 讲什么：K-069~K-075。前置：02/04、05（公式）、06（对端）。后置：14、99。
- 分工声明（保持）：nice 用户面主权在 `../04-stage-pm/16-scheduling.md`。
- 事实底线：`sched_start.c`；`sched_stop.c`；`pm/schedule.c`；`pm/forkexit.c:102,425,441`；`pm/main.c:199,213,372-380`；`mproc.h:78`。
- 修正清单：
  1. §1.5 锚点 `mproc.h:sigaction（L78，工具生成）` → `mproc.h:78（mp_scheduler 字段）`（G-8）；
  2. 全篇"（LNN，工具生成）"锚点按"函数名+行段"规范化（如 `sched_start.c:58-59`）（G-8）；
  3. §5.1 基线 59 → 81（G-7）。
- 验收标准：三路路由/双包/照收三决策与 `client.rs`（`SchedulerSel:31`、`route_start:77`、`route_stop:106`、`read_scheduler:199`）对账；5 测试对账。

### 14-rs-interaction【保持+修正】

- 讲什么：K-076~K-078。前置：02/13、05（出身判断）、10（守土）。后置：99。
- 分工声明（保持）：槽配置归 `../03-stage-rs/08-rs-slot-config.md`，Live Update 归 `../03-stage-rs/16-rs-live-update.md`。
- 事实底线：`rs/utility.c:364-384`；`rs/manager.c:461`；`rs/request.c:342-345`；`rs/type.h:92-95`；`com.h:61`。
- 修正清单：
  1. §2.3 锚点 `type.h:ARGV_ELEMENTS（L92，工具生成）` → `rs/type.h:92-95（r_scheduler/r_priority/r_quantum/r_cpu）`（G-8）；
  2. §4.1 实现归属三层声明的文件路径复核（`os/servers/rs/src/sched.rs` 的 `StopSite`/`on_stop_result` 行号漂移按 rg 实况修正）（G-8）；
  3. §5.1 基线 59 → 81（G-7）。
- 验收标准：六参直达的六个实参逐个有锚；取消两位置处理与 `sched.rs` 实况一致。

### 99-global-concepts【新建】

- 一句话定位：全 stage 的查询面——三张表（消息号、优先级常量、错误码）+ 三个全局概念（双层模型一句话、五表一致性、职责边界）+ 测试总账。
- 讲什么：K-007/K-026/K-027 的**查询表形态**（语义引用 02/05）；K-079、K-080（承接 draft/99 与 draft/00 素材改写）；K-081 测试总账。
- 不讲什么：一切机制（各篇）；实现细节（各篇 §3/§4）。
- 前置：无（可随时跳入；概念条目指向 00）。
- 后置：被全部各篇引用（从 99 反向引用各篇主权小节）。
- 事实底线：`com.h:59-63,449,801-807,1151`；`config.h:66-77`；`schedule.c:16-46`；`sys/sys/errno.h:64,211-213`；`schedproc.h:1-3`；`kernel/table.c:56`。
- 知识点清单：K-007 表、K-026 表、K-027 表、K-079、K-080、K-081。
- 验收标准：① 消息号表 5 行（含方向与处理函数列）；② 优先级常量表 7 行（含派生式）；③ 错误码表 ≥6 行（EINVAL/EPERM/ENOSYS/EDEADEPT/EBADEPT/EBADCPU，C 值与 Rust `errno.rs` 值双列）；④ 五表对照 5 行（表名/标识字段/记录内容）；⑤ 职责边界一段+一张四家分工表；⑥ draft/99 的"待迁移"项全部落实或显式声明不迁移及理由。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|---|---|---|---|---|---|
| N-1 | 新建（全文） | 00 骨架 20 行 | 00 全文 | G-1：总览是入口篇，骨架不可交付；主线图不得委托 plan.md | K-001/002/079/080 | 存量：draft/00（五表表、消息表）；新增：K-002（table.c:56）、真序表 A 段 |
| N-2 | 新建（全文） | 99 骨架 20 行 | 99 全文 | G-2：查询表与全局概念无正文 | K-007 表、K-026 表、K-027 表、K-079/080/081 | 存量：draft/99（双层模型问题清单）；05/02/04 的表转为引用 |
| N-3 | 修正 | 01§1.1、§7、§5.1 | 原位 | K-001 主讲述点迁 00 的回指补偿；G-7/G-8 | K-001、K-004 | 存量原位保留 |
| N-4 | 修正 | 02§3 D4/D5/D9、§4.1、§5.1 | 原位 | G-3（E-MINTYPES-SYS 回写）、G-7 | K-010、K-017 | 新增事实源：`dispatch.rs:13`、`transport.rs:33` |
| N-5 | 修正 | 03§2 全部锚点、§5.1 | 原位 | G-8（符号名错误锚点）、G-7 | K-018~K-021 | 存量原位保留 |
| N-6 | 修正 | 04§2.4、§5.1 | 原位 | G-8 路径补全、G-7 | K-026 | 存量原位保留 |
| N-7 | 修正 | 05§3 D3、§5.1 | 原位 | G-8 锚点笔误、G-7 | K-031~K-034 | 存量原位保留 |
| N-8 | 修正 | 06§5.1+状态指针 | 原位 | G-7、G-10 | K-035~K-041 | 新增指针：edge E-SCHEDSMP |
| N-9 | 修正 | 07§5.1 | 原位 | G-7；G-6 联动复核 | K-042/043 | 存量原位保留 |
| N-10 | 修正 | 08§1.2、§5.1 | 原位 | G-5（E-PREEMPTFLAG live 半回写）、G-7 | K-044/045 | 新增事实源：edge_todo E-PREEMPTFLAG |
| N-11 | 修正 | 09§5.1 | 原位 | G-7 | K-047~K-054 | 存量原位保留 |
| N-12 | 修正 | 10§2.3、§5.1+状态指针 | 原位 | G-8、G-7、G-10 | K-055~K-058 | 新增指针：edge E-SCHEDSMP |
| N-13 | 修正 | 11§5.1、§1.6 链接 | 原位 | G-7、G-8 链接复核 | K-059~K-063 | 存量原位保留 |
| N-14 | 修正 | 12§1.2、新增现状节、§4.1、§5.1 | 原位 | **G-6 事实错误**、G-4、G-5、G-8、G-7 | K-064~K-068 | 新增事实源：§0.2 grep、edge 两闭单一 open |
| N-15 | 修正 | 13§1.5、全篇锚点、§5.1 | 原位 | G-8、G-7 | K-069~K-075 | 存量原位保留 |
| N-16 | 修正 | 14§2.3、§4.1、§5.1 | 原位 | G-8、G-7 | K-076~K-078 | 存量原位保留 |
| N-17 | 对账（不改） | plan.md §6.1 状态表 | — | 11-14 实为成稿，状态表过时；plan 属参考材料不改 | — | 记录于 B 相交付说明（G-11） |

**未执行的操作类型**：重排（0 处）、拆分（0 处）、合并（0 处）、归档（0 处——draft/ 与 archive/ 维持现状，draft/00 与 draft/99 在 N-1/N-2 吸收后仍按"B 相归档不删"原则保留原文件）。理由：现有目录满足四条硬标准，且任何编号变动触发 §8 的断链成本。

---

## 7. 缺漏新篇（非 C 主题逐项落实）

> §3.5 已逐项回答"在哪里讲/为什么不在"。本节收口为裁决表，无"待定"项。

| 主题 | 裁决 | 承载位置 | 验收 |
|---|---|---|---|
| 链接与加载 | 不在本 stage | 99 一句声明 + 引 01-stage-kernel/09 | 99 含该声明句 |
| 镜像与内存布局 | 不在本 stage | 99 一句声明 | 同上 |
| 汇编入口与陷阱进入 | 不在本 stage（edge E1 轨道） | 02 D9 已声明接缝形态；99 汇总声明 | 02/99 各一句 |
| 引导链与引导协议 | 登记事实入 00（K-002），协议引用 01-stage-kernel/09 | 00 主线图 A1 行 | 00 含 table.c:56 锚 |
| 启动装配 | 在本 stage | 01（C SEF + Rust main.rs） | 已有 |
| 构建与工具链 | WONTFIX | plan §5.4 既定 | 不新增正文 |
| 跨模块接口与线格式 | 在本 stage | 02/09/12/13/99 | 已有 + 99 表 |
| 错误路径 | 在本 stage | 各臂 + 99 错误码表 | 99 表 ≥6 行 |
| 关闭与退出 | 在本 stage | 07 + 13/14 | 已有 |
| 并发与同步 | 服务侧在本 stage（单线程+S-11）；内核侧不在 | 02 D8 + 99 一句边界声明 | 99 含声明句 |
| 测试基建 | 在本 stage（内联测试）；真机联调不在（E5(e)/E8） | 各篇 §5 + 99 总账 + 06/10/12 状态指针 | 99 含 81 总账 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

> 编号不变 ⇒ 篇间锚点零迁移。发生变化的只有：00/99（全文重建，旧节为骨架无实质小节）与 14 篇的原位修正。逐节列出：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|---|---|---|---|---|
| 00 骨架 §核心点 全部 5 条 | 条目名列表 | 00 全文对应章节 | 改写（骨架→正文） | 无外部引用指向 00 骨架小节 |
| 00 骨架 "启动主线图…参见 plan.md §1.2" | 委托式主线图 | 00 自带主线图（真序表 A 段） | 改写+去委托 | 无（该委托句本身是要消除的缺陷） |
| 99 骨架 §核心点 5 条 | 条目名列表 | 99 全文对应表/节 | 改写（骨架→正文） | 无 |
| draft/00 §2.3 五进程表 | fork 视角的表对照 | 99 五表一致性节 | 改写吸收（去 fork 视角） | draft 属素材，正式文档不引用（既有规范） |
| draft/99 §1-3 TODO 清单 | 双层模型/继承/五表问题清单 | 99 对应节 | 改写吸收 | 同上 |
| 01§1.1 双层模型段 | 主讲述（约 3 段） | 压缩为引用段，主讲述点迁 00 | 改写 | 01 被引 8 次均为篇级引用，无节级引用，安全 |
| 12§1.2 第四条归属变迁 | 错误断言 | 12§1.2 修正句 | 改写（事实修正） | 12 被引 11 次均为篇级，无节级引用，安全 |
| 02§3 D4/D5/D9 的本地常量表述 | 过时 Rust 锚点 | 原位改写为权威路径 | 改写 | 无节级外部引用 |
| 各篇 §5.1 基线行 | 59/79 | 81 | 原样替换 | 无 |
| 各篇"（LNN，工具生成）"锚点 | 符号名错误 | 真实符号+行段 | 原样替换 | 无（这些锚点本就解析失败） |

### 8.2 引用迁移表

| 引用方 | 旧引用 | 新目标 | 验证方式 |
|---|---|---|---|
| `edge_todo.md`（21 处） | `06-stage-sched/todo.md §…`、篇名 | 不变 | `rg -c '06-stage-sched' edge_todo.md` 前后一致 |
| `04-stage-pm/16-scheduling.md`（4 处） | `06-stage-sched`（目录级）与 `sched_stop` 等函数名 | 不变 | `rg -n '06-stage-sched' ../04-stage-pm/16-scheduling.md` 逐条回访 |
| `00-master-plan/README.md`（2 处） | `06-stage-sched/` 目录名 | 不变 | 目录名不动 |
| `07-stage-ds/todo.md`、`08-stage-is/todo.md`、`edge1/3.md`、`14-stage-runtime/plan.md` | 目录/篇名 | 不变 | 同上 |
| `os/servers/sched/src/main.rs:4`、`lib.rs:8` | `06-stage-sched/01-sched-init-main.md` | 不变 | 文件名不动；`rg -n '06-stage-sched' os/servers/sched/src/` 应仍 3 命中 |
| `os/servers/sched/src/main.rs:44` | `11-balance-queues.md` | 不变 | 同上 |
| 01-14 篇间互引 ≈213 处 | `NN-*.md` | 不变 | 重建后 `rg -c` 复跑，计数不减 |

### 8.3 断链成本摘要

- **本蓝图的方案（保编号）**：受影响外部引用 **0**；受影响代码注释 **0**；受影响篇间互引 **0**。
- **反事实（若重编号/重排）**：篇间互引 ≈213 处 + 外部 9 文件 36 处 + 代码注释 3 处，共 **≈252 处**需批量改写；热点为 06（28 次被引）、02（21）、09（20）、10（19）、05（19）。以 01-stage-kernel/todo.md I-14 的历史裁决（"重编号断链风险大于收益"）为同一结论的先例。
- **建议的批量修改方式**（仅针对本蓝图实际需要的修正）：G-7 基线行可用一条 sed 按篇批量替换后逐篇人工复核；G-8 锚点必须逐条手工改（每条需 sed 复核目标行），禁止盲替。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：逐篇核对 §5 契约的"前置"字段——00 无前置；01→00；02→01；03→00,02；04→02,03；05→03,04；06→02,04,05；07→02,04,06；08→02,04,05；09→05,06；10→01,05；11→02,05；12→05,09；13→02,04,05,06；14→02,05,10,13。全部指向更早编号，**通过**。（正文内部"不讲什么/细节归 NN"式前指是去向下放声明，属项目既定导航约定，非概念使用，已逐篇确认不产生阅读依赖。）
2. **依赖关系图检查**：由上述前置边构图，为链式偏序（00→01→02→{03,04}→05→{06,07,08}→09→10→11→12→13→14），无环，**通过**。
3. **覆盖率检查**：§2 池中 81 条知识点全部在"去向"列有落点（76 条原篇保留/修正、5 条迁入或新建于 00/99）；删除项 0 条；新增条目 4 条（K-002/017/068/081）均有 C/Rust/edge 锚点，**通过**。
4. **断链成本统计**：§8.3——方案内 0 断链；反事实 ≈252 处已列热点，**通过**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|---|---|---|
| G1 C 真序逐条可核对 | **通过** | 抽 10 条复核：A1（table.c:56 sed 确认 SCHED 行）、A4（main.c:130）、B2（main.c:45-54）、B3（main.c:57-87）、C1 检查序（schedule.c:146-166 通读）、C4 回滚（:287-288）、C6 平局取小（:71 严格小于）、D4（do_schedctl.c:39,42）、D5 的 `> NR_SCHED_QUEUES`（system.c:645-646）、D7 短路（sched_stop.c:16-17）——全部与源码一致 |
| G2 知识点池完整 | **通过** | C 侧：服务端 16 函数 + 对端 14 函数/调用点 + 7 消息体 + 常量族（com/config/errno/type/priv/const/ipcconst/endpoint）全部入池或显式 WONTFIX（Makefile，plan §5.4 既定）；非 C 制品 10 类逐项有归属（§3.5/§7） |
| G3 前向引用为零 | **通过** | §9.1 第 1 条 |
| G4 依赖图无环 | **通过** | §9.1 第 2 条 |
| G5 覆盖率 100% | **通过** | §9.1 第 3 条；明确删除项：无（本 stage 无被弃知识点；draft 中已废弃的"fork 设置 cpu_mask"错误断言不配得编号，其修正在 03 篇 K-020 以 S-3 消除决策承载） |
| G6 拆合去向/新建来源 | **通过** | 本蓝图拆分 0、合并 0；新建 2 篇（N-1/N-2）的存量来源（draft/00、draft/99）与新增来源（K-002 等）均已列；抽查超 10 处修正操作（N-3~N-16）均写明原位保留与事实源 |
| G7 契约七要素齐全 | **通过** | 16 份契约均有：定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收标准（保持型契约以"修正清单+验收标准"承载验收要素） |
| G8 迁移表覆盖 | **通过** | §8.1 覆盖全部变化文档的每一变化节；§8.2 覆盖文档间与代码注释引用（全部 0 迁移，逐类列验证命令） |
| G9 事实断言有锚点 | **通过** | 抽 10 条：p_scheduler 写入点三处（grep 全树）、errno 三值（sed :211-213）、USER_Q 派生式（config.h:69-70）、PREEMPTIBLE 位（priv.h:45,49+const.h:143）、FROM_KERNEL（ipcconst.h:28）、SUSPEND=-998（com.h:1151）、boot 登记（table.c:56）、niced 公式（pm/utility.c:95-96）、81 测试（cargo 实跑）、E-MINTYPES-SYS 闭环（dispatch.rs:13 import 实况）。推测项：无（唯二标注"待 B 相复核"的是 11§1.6 两条外链的有效性与 02 篇 §4.1 的 minix-types 路径写法，均已写入对应修正清单） |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图已完成。可交付给 B 相执行：2 篇全文新建（00、99）+ 14 篇定向修正（合计 32 条修正项，全部带锚点与验收标准）+ 0 篇重写 + 0 处重排。

**待用户裁决**：

1. **"保编号、修事实、补两头"方案是否采纳**（glm 建议采纳）：替代方案是对 01-14 全量重写。不建议——这 14 篇经过 2026-09-03/04 的重写与收敛审查，本次 40+ 锚点抽查仅发现 1 处事实错误与若干卫生问题，全量重写将以高成本重造已验证内容；但若汇总轮裁定"文风/结构仍需统一重写"，本蓝图的知识点池与各篇契约同样可直接作为重写任务书使用。
2. **G-6（12 篇 p_scheduler 事实错误）的登记级别**：glm 建议 P1-fact（误导读者对归属生命周期的理解，但不导致代码行为误判），随 B 相 N-14 一并修复并在该篇 scan 记录。
3. **plan.md §6.1 状态表过时（G-11）**：glm 建议不改 plan.md（参考材料），由 B 相交付说明记录对账；若用户要求 plan 也同步，属一次小改。
4. **两篇骨架（00/99）的执行时机**：plan §6.1 本就排有"00/99 正文改写"待办；glm 建议直接按本蓝图 §5 的两份契约执行，不必再走一轮设计。

---

*（蓝图完。执行者 glm，2026-09-19，基线 commit d6ecd22ca。本文件是 06-stage-sched 目录内唯一的 `_glm` 产物。）*
