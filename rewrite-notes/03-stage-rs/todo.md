# 03-stage-rs TODO 总清单（rs server Rust 实现深度 review + 改进点收集）

> **来源**：2026-08-15 Codex 深度 review 起始，历经 §10-§17 六轮修复、§18 查漏+架构轮、
> §18.11 收敛 campaign、§20/§21 扫描与解锁 campaign、§22 第二轮全面扫描（2026-09-09）。
> **证据基线（2026-09-09，§22 时点）**：`cargo test -p minix-rs` = **327 passed**；
> `cargo test -p minix-types` = **189 passed**；`cargo clippy -p minix-rs` 生产面 **0 告警**
> （依赖 crate 告警见 §22.4）；`cargo fmt --check` 干净；`tools/check-rs-unwired.sh`（T7 门禁）
> = PASS（唯一生产标记 `boot.rs:1060` 带 18 号 doc contract）。
> **重要前提**：生产运行路径仍 fail-closed（`minix-sys::receive` 为 `todo!()`、`KernelApi`
> 生产 impl 全 ENOSYS、`main()` 在接线前不可达运行期），因此无 P0 运行时缺陷；§22 新发现
> 以 P1（接线前必须修正的接线缺陷与缺失检查臂）为主。
> **清理记录（2026-09-09）**：按用户指示把已完成 Fix #1-#90 的逐条 Before/After 记录压缩为
> 索引表（§2/§5），细节沉淀在 git 历史（本文件 2026-09-09 之前的版本 + 各 Fix 的 commit）。
> 未完成项（§22 全部、§3.6 EDGE 清单）保留全文。

## 0. 审查基线 — 已确认的良好架构（无需改动）

| 维度 | 现状 | 评价 |
|------|------|------|
| 执行模型 | 单线程事件循环，状态全部经 `&mut self` 线程化传递，全 crate 无 `unsafe` | ✅ 符合 AGENTS.md 用户态服务器模型 |
| 错误失败策略 | 未接线路径全部 fail-closed（`UnimplementedKernelApi` 全 ENOSYS、`BootError` 类型化、T7 CI 门禁） | ✅ |
| C 语义编码 | bitflags 位值、errno 常量、消息号均有逐位测试断言；C 分支级对照测试 327 项 | ✅ 防漂移 |
| 类型化改进 | 五域 trait 面（`SysApi`/`SchedApi`/`PmApi`/`VmApi`/`IpcApi`，Fix #60）、`Errno` newtype（Fix #1）、`Effects` 聚合束（Fix #87/#88）、wire ABI 布局见证三件套（Fix #81/#85/#89） | ✅ Rust 类型安全红利兑现 |
| 内存安全 | `Arc<[u8]>` 共享 exec 镜像、`Option<SlotId>` 替代 C 四指针链、`assert_consistent` settled-state 不变式校验器（Fix #67/#79） | ✅ |
| 覆盖率 | 覆盖度工具 171 个 C 符号 / 文档覆盖 100% / Rust name-match 88.9%，⚠️ 项全部定谳（§6.1/§4.1）；函数级零真缺口 | ✅ |

## 1. 建议总览（第一轮 T/D/S/E 系列 — 全部闭合）

| ID | 主题 | 严重度 | 状态 |
|----|------|--------|------|
| T1-T7 | BootInit 状态 handover / KernelApi 拆面 / Errno newtype / 主循环骨架 / 注入统一 / boot 计数 / 占位门禁 | P1/P2 | ✅（Fix #1-#30、#60） |
| D1 | `ServiceSlot` god struct（45 字段） | P2 | 🔶 判定闭合：全拆不采纳（镜像保真是 A-3 契约；2026-09-07 用户裁决）；R33 深比较已修（Fix #68） |
| D2 | `SlotId` 无世代悬垂 + `get()` 越界 | P2 | 🔶 判定闭合：世代不采纳（C 行本就复用）+ `assert_consistent` 落地（Fix #67）；越界门随 Fix #29 |
| D3-D6 | totality / 常量双定义 / Default 对齐 / io-irq 双表 | P2 | ✅（Fix #3/#5/#6/#62） |
| S1-S4 | build_cmd_dep NUL / activate_boot_slot 字段 / sched NONE / sched 签名 | P1 | ✅（Fix #2/#4/#12） |
| E1-E3 | mock 收敛 / property 测试 / 集成面 | P2 | ✅（Fix #25/#64/#65；E3 的 mock 层集成面已落地，场景级集成归 E9 联调） |

> T/D/S/E 每项的原始"现状 + 问题分析 + 改进思考"全文与修复细节见 git 历史（本文件
> 2026-09-09 之前的版本）与 §2 索引表对应的 Fix commit。

## 2. 历史修复索引（Fix #1-#78，2026-08-15 — 2026-09-07）

> 逐条 Before/After 与方案对比沉淀在各 Fix 的 commit message 与本文件历史版本。

### 2.1 六轮 review 修复（Fix #1-#39，§10-§17 轮）

| Fix | 发现 | 主题 | Fix | 发现 | 主题 |
|-----|------|------|-----|------|------|
| #1 | T3 | Errno newtype 落 minix-types + 全 crate 迁移 | #21 | N8 | RupdateDescriptor 死代码删除 |
| #2 | S1 | build_cmd_dep 遇 NUL 停止 + 参数不截断 | #22 | N9 | IoRange/NR_SCHED_QUEUES 单一权威 |
| #3 | D3 | 越界计数 fail-closed | #23 | N11 | build_cmd_dep argv[0] 恒存在 |
| #4 | S3/S4 | sched NONE 跳过 + sched_init_proc 携全参 | #24 | T5 | `&mut dyn` 注入统一 monitor 模式 |
| #5 | D5 | RsStart::default 对齐 C 调用方默认 | #25 | E1 | 四份 mock 收敛 testutil |
| #6 | D4 | 常量收敛（RS_MAX_LABEL_LEN/MAX_BACKOFF） | #26 | R1 | effective_period LU 初始化超时 |
| #7 | T2 | KernelApi 删默认 impl + Unimplemented 全 ENOSYS | #27 | R2 | Privilege 计数域 u16→i32 fail-closed |
| #8 | T6 | Step2/3 计数修正 + SYNCH_BOOT fail-closed | #28 | R4 | do_init_ready pending 下溢 fail-fast |
| #9 | T1 | BootInit→ServerState 状态 handover | #29 | R5 | RProcTable::get 越界断言带上下文 |
| #10 | T4 | get_work 不伪造 IpcStatus | #30 | R6 | mock unimplemented → Err(ENOSYS) |
| #11 | T7 | 占位符 CI 门禁（check-rs-unwired.sh） | #31 | R11 | label UTF-8 契约 fail-closed |
| #12 | S2 | activate_boot_slot 补全 boot 字段 | #32 | R16 | compute_backoff 负 restarts clamp |
| #13 | 工具链 | clippy/fmt/重复注释清理 | #33 | R12 | endpoint_slot/set_endpoint_index 越界门 |
| #14 | N1+N10 | boot 表 flags 位值全错（P0）→ 单一权威 | #34 | R15 | rebuild_args argc 只计完整 token |
| #15 | N2 | ping 间隔用原始 r_period | #35 | R18 | free_slot 内建 SF_USE_COPY→free_exec |
| #16 | N3 | Machine 纳入 ServerState | #36 | R19 | pid.is_some_and(p>0) |
| #17 | N4 | Errno 统一 minix-types | #37 | R17 | boot 表 proc_nr↔endpoint 校验 |
| #18 | N5 | SefCallbacks fn 指针 → trait | #38 | R13/R14 | SlotMutations 决策变异载荷统一 |
| #19 | N6 | Label 相等语义 strcmp 化 | #39 | R14+ | RsStart 补 rss_nr_io/rss_io（rs.h:124） |
| #20 | N7 | CallMask/SysMap 移位越界 fail-closed | | | |

### 2.2 §18 查漏+架构轮修复（Fix #40-#78，2026-09-06/07）

R20-R34/A1-A5 的发现全文见 §3.2/§3.3 判定表（保留）。

| Fix | 关单 | 主题 | Fix | 关单 | 主题 |
|-----|------|------|-----|------|------|
| #40 | R26 | signal_manager 签名对齐 sef.h:270 | #60 | E-2/R9 | KernelApi 拆五域 trait 面 |
| #41 | R25 | HeartbeatNotify 携带 timestamp | #61 | E-6/T3 | BootError 与 ENOSYS 解耦 |
| #42 | R24 | do_upd_ready 补 SlotMutations 载荷 | #62 | E-5/D6 | io/irq 双表收敛（单一权威+派生快照） |
| #43 | R30 | caller_can_control 补 in-use 复核 | #63 | A3 | SEF restart 回调重绑（RestartCb 枚举） |
| #44 | R21 | from_calls 加 base 参数（组合语义） | #64 | E-9/E2 | 解析面零依赖 property 测试 6 项 |
| #45 | R32 | 一致性杂项 7 小项 | #65 | E-10 | boot 失败传播族 + 壳层 fail-fast |
| #46-#49 | R20 | RsStart 全字段 + edit_slot/init_slot 全分支 | #66 | E-8/R31 | Errno::name/Display + error.rs 描述表 |
| #50 | R28 | clone_service 两分支决策原语 | #67 | E-3/D2/R8 | assert_consistent（世代不采纳闭合） |
| #51/#52 | R22/A4 | create_service 编排 + 控制域载荷模式 | #68 | E-4/D1/R33 | 删 derive(PartialEq, Eq)（god struct 闭合） |
| #53-#56 | R23/R27/A2 | UpdateState 单点 + LU 编排全链 | #69 | Fix69 | 06 主循环 notify 半环 + RS_SHUTDOWN 臂 |
| #57 | R34.21 | signal_handler 主体路由 | #70 | Fix70 | 12 号 init-ready 接线 + catch_boot_init_ready |
| #58 | R34.18 | get_work receive 经 KernelApi 注入 | #71 | Fix71 | 13 号 RS_DOWN 臂 + MessRsReq 解码 |
| #59 | R34 | SEF 回调接线（init_restart/init_lu） | #72 | I1/OQ-3 | 死代码裁决 + awaiting-wiring 标注机制 |
| | | | #73-#78 | I2-I6 | boot init_service / 14 号四臂 / 13 号 label 四臂 / 16 号 LU_PREPARE 臂 / signal_manager 七分支+terminate_service 执行体 |

## 3. §18 查漏 + 架构轮（2026-09-06）——判定表与证据（保留部分）

### 3.1 覆盖度提取证据（gate-evidence-A）

```
$ python3 tools/coverage-extract/coverage-extract.py rs …（semantic-map 全量扩展后）
  Found 171 C symbols (112 funcs, 7 structs, 52 macros, 0 enums)
  Doc covered: 163 (95.3%) → §20 回填后 171 (100%)
  Rust covered (name-match): 115 (67.3%) → §22 时点 152 (88.9%)
```

工具盲区：多行签名函数（exec.c 的 do_exec/read_seg/srv_execve）不被正则提取，人工确认
全缺并归属 19 号（E-11）。

### 3.2 R20-R34 判定表（全部关单，除 R31 残余）

| ID | 主题 | 状态 |
|----|------|------|
| R20 | edit_slot/init_slot 整体缺失 + RsStart 载体字段不全 | ✅ Fix #46/#48/#49 |
| R21 | from_calls 无法表达 is_init=false 组合语义 | ✅ Fix #44 |
| R22 | 服务生命周期编排层缺失（create_service 15 步） | ✅ Fix #51/#52/#56 |
| R23 | LU 中段编排缺失 + rupdate 全局碎片化 + r_upd 载体未建 | ✅ Fix #53/#54/#55/#56 |
| R24 | do_upd_ready 缺载荷（RS_PREPARE_DONE 无处落地） | ✅ Fix #42 |
| R25 | HeartbeatNotify 缺 timestamp | ✅ Fix #41 |
| R26 | signal_manager 签名偏差 | ✅ Fix #40 |
| R27 | rollback 心跳重发扫 + end_update 自毁短路 + abort 时序 | ✅ Fix #56 |
| R28 | clone_service 两分支漏标 DEFERRED | ✅ Fix #50 |
| R29 | TrapMask 宽度分歧（OQ-2 裁决 = u16 全宽忠实 C） | ✅ Fix #47 |
| R30 | caller_can_control 丢 IN_USE 复核 | ✅ Fix #43 |
| R31 | error.c 错误表 + 诊断字符串化 | 🔶 错误名面 ✅（Fix #66）；`srv_to_string_gen`/`srv_upd_to_string`/`print_services_status`/`print_update_status` 归 08-stage-is dump 面，`exec_restart` 归 19 |
| R32 | 一致性杂项 7 小项 | ✅ Fix #45 |
| R33 | ServiceSlot derive(PartialEq) 深比较风险 | ✅ Fix #68 |
| R34 | 测试盲区清单 24 条 | ✅ 全部闭合（1-13 各实现轮 / 14-15 Fix #75 / 16-17 Fix #71/#76/#69 / 18-21 Fix #65/#69 / 22 Fix #78 / 23 Fix #70 / 24 Fix #64） |

### 3.3 架构建议 A1-A5 终态

| ID | 建议 | 终态 |
|----|------|------|
| A1 | 编排层引入形态：忠实编排函数（a）/ 服务状态机（b）/ Effect 解释器（c） | ✅ 形态 a 采纳落地（Fix #51/#52/#55/#56） |
| A2 | rupdate 全局收敛为 `UpdateState` 挂 ServerState + per-slot `r_upd` | ✅ Fix #53 |
| A3 | SEF 回调重绑建模（RestartCb 枚举） | ✅ Fix #63 |
| A4 | 控制请求域统一决策载荷模式 | ✅（Fix #52 起，随接线完成） |
| A5 | 接线路线图 06→12→13→08→16→19 | ✅ 1-4/6 步按序执行；19 归 edge E9 |

OQ 清单终态：OQ-1（self_lifecycle 保持纯切片等 18 号接线，awaiting-wiring 标注）、
OQ-2（TrapMask u16 全宽）、OQ-3（share_exec 单一权威，clone_slot 路由过去）、
OQ-4（dispatch 死表逐臂转真）——全部关单。

### 3.4 §18.11 收敛 campaign（I1-I6，2026-09-07）

I1 死代码裁决（6 常量副本收敛 minix-types + set_endpoint_mapping 删除 + OQ-3 关单 +
awaiting-wiring 标注机制 8 处）→ I2 boot Step 2 init_service → I3a/b 14 号四臂 →
I4 13 号 label 四臂 → I5 16 号 LU_PREPARE 臂 → I6 signal_manager 七分支 +
terminate_service 执行体。基线 211→304 tests。

### 3.5 Rule Discovery 沉淀（§18/§20/§21 累计）

1. **CACG（Copy-struct ABI Gate，模式 84 候选）**：以"整结构拷贝"开场的控制臂，接线前先
   审计结构体内全部 typedef 的本树定义完整性，缺失即登记 edge、臂保持 fail-closed。
2. **LWT（Layout Witness Triple，模式 85 候选）**：跨边界字节 ABI 的 Rust pinning 固定形态
   = 偏移常量单点表 + `#[repr(C)]` 私有见证结构 + `offset_of!`/`size_of` 编译期断言，
   解码一律 `from_le_bytes` 安全读取（rs_start/rproc/rprocpub 三实例，后者当场抓住
   rs_pci_id 对齐误读与 3 处手推偏移错误）。
3. **mock 保真度随消费面升级**：mock 的 seam 语义在 handler 消费面变宽时必须同步升级到真实
   原语语义（罐装 blob 在多缓冲取数时代即成假绿）。
4. **语义映射表回填**：coverage-extract 的 name-match 假 ⚠️ 靠人工判定后应回写
   rs-semantic-map.json，后续复测直接归位。

### 3.6 EDGE 清单（§18.10——结构性重构 / 测试基建 / 生产接线边界）

**E-1 RS 自升级进程面（18 号接线轮）**：`self_lifecycle.rs` 13 个纯切片 + 决策面全就绪
（awaiting-wiring: 18）；缺口 = `srv_fork` 后 RS 自身继续运行同时驱动 `update_service
(RS_DONTSWAP)` + `init_service` + YIELD 链，每步压在 19 的进程/IPC 缝上。

**E-7 R10 调用顺序类型化（gated，13/06 接线轮）**：剩余项压在未落地编排上；E-7 的类型化锚
已以"序列即函数体 + 类型化缝"形态在 do_edit 臂闭合（Fix #83）。

**E-10 E3 集成测试剩余（场景级）**：`signal_manager` 分支的实排空已由 Fix #78 落地；
rollback→心跳重发扫、catch_boot_init_ready 三 panic 分支等场景级集成面已由走链测试覆盖，
剩余全链路集成归 E9 联调。

**E-11 生产接线面（19 号主线，恒为边界）**：`minix-sys::receive`/`send`/`asynsend`/
`sys_datacopy`/`cpf_revoke`/`cpf_grant_direct`/`sys_privctl`/`sys_getpriv`/
`sys_getmachine`/`sys_getinfo`/`sys_setalarm`/`sys_kill`/`sys_update`/
`sys_diagctl_stacktrace`/`vm_memctl`/`vm_set_priv`/`vm_prepare`/`getnpid`/`getnuid`/
`waitpid`/`setuid`/`read_exec`/`ds_publish_label`/`run_script` 的 fork+execle/
`env_parse` 配置面/`srv_execve` 真实 exec；A-3 的 diagctl 通用输出缝（rs_verbose 全家
的可观测性出口）补登于此。全部 ENOSYS fail-closed + doc contract；接线即
19-rs-external-interfaces.md 主线。

> E-2/E-3/E-4/E-5/E-6/E-8/E-9 已全部闭合（见 §2.2 索引），原全文见 git 历史。

## 4. §20 全面扫描轮（2026-09-07）——覆盖率判定表（standing 分类）

> 40 个 name-match ⚠️ + 1 有Rust无文档的逐项定谳结论（复测时直接引用，避免重复人工判定）：

- **A 类（名称不匹配，Rust 等价物已存在）21 项**：`copy_label`→`resolve_by_label`+
  `IpcApi::safecopy_from`；`kill/crash/detach_service_debug`→recovery.rs 同名（`_debug` 是
  C file/line 宏包装）；`rupdate_*` 链操作→`UpdateState`/`UpdateChain` 方法；`end_update_*`
  四相位→`UpdateState::end_update` 单体内联；`fi_service`→`do_fi`+`LsysFiCtl::encode_message`；
  `rs_asynsend`→`IpcApi::asynsend`；`BEG/END_RPROC_ADDR`→`iter_all`/`iter_in_use`；
  `RS_DONTREPLY`→`DispatchResult` 模型；`sef_local_startup`→`RsServer::init`+trait 分派；
  `RS_VM_DEFAULT_MAP_PREALLOC_LEN`→调用方参数注入；`copy_rs_start`/`do_edit` 曾刻意不回填
  （E-RSSTART 门）——**该门前已关单，回填待办见 §6.2 R39**。
- **B 类（ARCH 不需要，全 10 项显式列出——本表是覆盖率的文档引用源，压缩会掉 Doc
  covered）**：头文件守卫 `RS_CONST_H`（const.h:4）/`RS_GLO_H`（glo.h:4）/
  `RS_TYPE_H`（type.h:4）；机制宏 `EXTERN`（glo.h:8）/`_SYSTEM`（inc.h:7）/
  `_TABLE`（table.c:7）/`errentry`（error.c:9）；死配置 `RS_USE_PAGING`
  （const.h:84）；调试开关 `DEBUG`（const.h:10）/`PRIV_DEBUG`（const.h:14）。
- **C 类（归属他 stage）**：`exec_restart`（exec.c:121）→19；
  `srv_to_string_gen`（utility.c:142）/`srv_upd_to_string`（utility.c:189）/
  `print_services_status`（utility.c:485）/`print_update_status`（utility.c:516）
  →08-stage-is。
- **D 类（L5 可观测性吸收）**：`DEBUG_DEFAULT`（const.h:6）/`PRIV_DEBUG_DEFAULT`
  （const.h:7）→ rs_verbose 输出缝（E-11 diagctl）；`rs_strerror`（error.c:33）→
  `Errno::name`/`Display`+error.rs 描述表（Fix #66；R39 已回填 semantic-map）。

**§20 结论**：函数级零真缺口；调用图反查无 P0/P1；A-2（Effects 聚合）、A-4（slot.upd
镜像一致性）、A-1（lib.rs 拆分触发器）均已执行（Fix #79/#86/#87/#88）。A-5（夹具族）
维持现状（4 个具名夹具即文档）。

## 5. §21 E-RSSTART 解锁 campaign 终态（2026-09-08）+ §21 增量

> 阻塞前提（"bitchunk_t/uid_t 本树无 typedef"）经核实为误判（minix3/sys/sys/types.h:124/:221）。
> 数据模型决策：wire 偏移按 x86-64 LP64 pinning。用户三项裁决：E-RSSTART/E-RSWIRE RS 侧半
> 并入本 campaign；A-2 执行；D1 god-struct 维持闭合。

| 轮 | 内容 | Fix | 轮 | 内容 | Fix |
|----|------|-----|----|------|-----|
| R1 | slot.upd 镜像一致性校验 + clear_upds 镜像清理 | #79 | R7 | do_getsysinfo 拷出半（SI_PROCPUB_TAB live） | #85 |
| R2 | semantic-map 回填（假 ⚠️ 归零） | #80 | R8 | A-1 lib.rs 拆分（3913→2261 行） | #86 |
| R3 | E-RSSTART 改判 + rs_start_t wire 面（920 字节 LP64） | #81 | R9a/b | Effects 聚合（Create/Restart/Terminate + Lu 族三束） | #87/#88 |
| R4 | RS_UP 臂（do_up，request.c:15-106） | #82 | R12 | struct rproc 内部表 pinning（SI_PROC_TAB/SI_PROCALL_TAB live，sizeof=3752） | #89 |
| R5 | RS_EDIT 臂（do_edit + E-7 收口） | #83 | R13 | init_state_data 组合 + do_update 状态数据段（17 号域闭环） | #90 |
| R6 | RS_UPDATE 臂（do_update 全流程，调度死表清零） | #84 | | | |

**终态**：dispatch 死表仅剩未知号 default 臂；`rs_start_t`/`rprocpub`/`rproc` 三个字节 ABI
pinning 落地 minix-types；测试 304→327（minix-rs）、175→189（minix-types）。
**§21 后增量（未入 §21 记录）**：`5a09366af`（init_state_data 源参数打包 `&RsStateData` +
`StateFetchFn` 别名——clippy 收敛）、`33d0e887c`（G1 `ProcNr` newtype 上移 minix-types
消双定义——arch 别名删除、边界拆包归零，minix-types 测试 184→189）。

## 6. §22 全面扫描第二轮（2026-09-09 —— cmd-04 + cmd-20 合体：查漏补缺 + 架构级审视，scan-only）

> **定位**：§21 收官后独立扫描轮，增量价值：①对 Fix #80 之后的 ~2000 行新接线代码做
> fresh-eyes 复审（review-core 同 Agent 局限缓解）；②函数级覆盖度复测 + 判定表复用；
> ③Redox 2025 对照增量。**scan-only**：全部发现为待修条目（R35-R42），修复走后续 todo-fix。
> **硬阻断预检**：`.design/` 快照 63 个全齐（21 文档 × outline/outline-review/design），
> `tools/design-coverage-check.sh` 全 PASS。**scan 期间产品代码零改动**。

### 6.0 预检与基线（gate 证据）

```
$ ls notes/rewrite/fork-syscall-rewrite/03-stage-rs/.design/ | grep -c "outline.v"     # 21
$ ls notes/rewrite/fork-syscall-rewrite/03-stage-rs/.design/ | grep -c "outline-review.v" # 21
$ ls notes/rewrite/fork-syscall-rewrite/03-stage-rs/.design/ | grep -c "design.v"      # 21
$ bash tools/design-coverage-check.sh fork-syscall-rewrite/03-stage-rs                 # ALL DOCS COMPLETE
$ cargo test -p minix-rs      # 327 passed / 0 failed
$ cargo test -p minix-types   # 189 passed / 0 failed
$ bash tools/check-rs-unwired.sh   # PASS（boot.rs:1060 唯一标记带 18 号契约）
$ cargo clippy -p minix-rs    # rs crate 0 告警（依赖 crate 10 条见 §6.4）
$ cargo fmt -p minix-rs -- --check   # 干净
```

### 6.1 覆盖率复测判定（21 个 ⚠️ 逐项定谳）

```
$ python3 tools/coverage-extract/coverage-extract.py rs … --output /tmp/SYMBOLS-r22.md
  Total C symbols: 171 / Doc covered: 171 (100%) / Rust name-match: 152 (88.9%) / ⚠️ 21
```

逐项对照 §4 standing 分类：20 项与 §20.1 判定表一致（B 类 ARCH 8 + C 类归属他 stage 5 +
D 类可观测性 5 + 既有 2）；唯一新判定 = `rs_strerror`（D 类吸收成立，Fix #66 的
`Errno::name`/`Display` + error.rs 描述表即其对应物，映射回填可随 R39 一并做）。
**函数级覆盖维持零真缺口结论**。

### 6.2 新发现（R35-R42，全部 ☐ 待修）

- **R35（P1-code-bug）— `do_upd_ready_shell` 在 gate 判定前无条件走链，gate 失败路径污染
  LU 链状态** ✅ 已修（2026-09-09，见 §22.7 Fix #91）
  - C 证据：request.c:890-938 的顺序是 ①gate（`!rpupd || rp != rpupd->rp ||
    RUPDATE_IS_INITIALIZING()`）不过 → 直接 `return EINVAL`（零状态变更）；②
    `rp->r_flags |= RS_PREPARE_DONE`；③`result != OK` → `end_update(result, RS_REPLY)`
    后返回（**不走链**）；④`start_update_prepare_next()` 才发生（update.c:467 起：置
    `RUPDATE_UPDATING`（update.c:510）、推进 `curr_rpupd`、发 prepare 请求）。
  - Rust 现状：`do_upd_ready_shell`（shell_update.rs:41-52）为给纯决策函数
    `ready::do_upd_ready(result, gate_ok, has_next)` 喂 `has_next`，把
    `start_update_prepare_next` 提到了决策**之前**无条件执行——而它是真实状态突变：
    置 `RupdateFlags::UPDATING`（live_update.rs:1296）、推进 `chain.curr`（:1301）、
    触发 `request_prepare`/`vm_prepare` effects（:1291/:1307，现为 noop、19 后为真消息）。
  - 后果分路径：**gate 失败（Unexpected → EINVAL）路径**——C 零状态变更，Rust 已把链推进
    到下一个服务并武装 UPDATING 且无任何清理回滚；多服务批量更新时，一条"迟到的
    RS_LU_PREPARE"（gate 存在的目的正是防它）会把期望报告者易主，后续合法消息反被拒，
    更新协议锁死。**PrepareFailed 路径**——C 不走链直接 end_update；Rust 先走链（向下一
    个服务误发 prepare）再 end_update，19 接线后成为对无 bip 服务误发消息。
  - 佐证：现有测试 `test_do_upd_ready_shell_gates_and_updates`（lib.rs:2166）只断言
    EINVAL/EDONTREPLY 回复值，未断言 gate 失败后的链状态——单条链测试中 walk 恰好无目标
    （`curr.next == None`）而表现良性，多服务链即暴露。
  - 改进思考：方案 a——给 `UpdateChain` 提取纯窥视 `fn peek_next(&self) -> Option<usize>`
    （即 live_update.rs:1268-1272 的取址逻辑，无 flags 写无 effects），`do_upd_ready_shell`
    用 peek 值喂决策，mutating 的 `start_update_prepare_next` 只在 NextPrepare/StartUpdate
    两臂执行（推荐——peek 与 walk 共享取址逻辑，单一真相）；方案 b——决策函数改签名收
    `&UpdateChain` 自己窥视（破坏 ready.rs 纯函数面）。补测试：gate 失败后
    `chain.curr`/`flags` 不变的负断言、双服务链的 PrepareFailed 不向第二服务发 prepare。
  - 归属：可立即 todo-fix（16 号域接线缺陷）。

- **R36（P1-design-missing）— `update_period` 检查臂缺失：prepare 阶段超时永不触发回滚**
  ✅ 已修（2026-09-09，见 §22.7 Fix #92）
  - C 证据：do_period 头部（request.c:950-954）——`RUPDATE_IS_UPDATING() &&
    !RUPDATE_IS_INITIALIZING()` 时每个时钟 tick 先调 `update_period(m_ptr)`（且扫表
    **继续**，非 return）；`update_period`（update.c:371-396）：取 `curr_rpupd`，按
    `prepare_maxtime > 0 && now - prepare_tm > prepare_maxtime` 判超时，超时即
    `end_update(EINTR, RS_CANCEL)`——旧版本恢复运行并收到取消通知。这是 live update
    prepare 阶段挂死（新实例永不报告）时的唯一自动回滚路径。
  - Rust 现状：`do_period`（lib.rs:356-358）只有一条注释："routes the tick into
    `update_period` first (request.c:952-954) — the LU mid-state checker; **deferred with
    the 16 wiring**"。但 16 号接线已落地（Fix #84/#88/#90：do_update、start_update 编排、
    prepare_state_data 全链 live）——deferral 标记指向的工期已经过去，检查臂本身成了漏接。
    组件就绪且**生产消费点缺失**：`has_update_timed_out`（monitor.rs:245，语义与 C 逐分支
    一致含 maxtime=0 不超时）全 crate 仅有 re-export（lib.rs:71）与自身测试两个引用，
    零生产调用；`do_period` 全函数无 `RupdateFlags::UPDATING` 门。`RS_CANCEL`
    （live_update.rs:120）已有 do_sysctl UPD_STOP 臂消费（shell_request.rs:1279），不缺
    载体——缺的正是 do_period 这条周期触发路径。
  - 后果：prepare 阶段挂死的批量更新永远不回滚——`RS_UPDATING` 持续、旧实例滞留 updating
    门、`effective_period` 按 LU 初始化超时喂 ping，系统进入无超时的等待。C 语义下
    2*RS_DELTA_T 内必然 abort。
  - 改进思考：do_period 头部补 `updating && !initializing` 门 → `has_update_timed_out`
    （读 `chain.curr()` entry 的 prepare_tm/prepare_maxtime）→ 超时走真
    `EndEffects` 的 `end_update(EINTR, RS_CANCEL)`（C 的 printf 归 19 diag 缝）。**不
    return**——门后继续原扫表（C 语义同 tick 双动作）。补测试：超时触发回滚（flags 复位 +
    RS_CANCEL reply_flag 路径）、未超时不动、maxtime=0 永不超时。
  - 归属：可立即 todo-fix；顺带把 lib.rs:356-358 的陈旧 deferral 注释删除。

- **R37（P2-健壮性）— `get_ticks().unwrap_or(0)` ×17 处：缝失败把时间戳静默归零**
  ✅ 已修（2026-09-09，见 §22.7 Fix #93）
  - C 证据：`getticks()`（sysutil.h:57）经 `getuptime`（libsys/getuptime.c:9-23）读
    `minix_kerninfo` 共享页，**恒返回 OK**——C 语义下时间戳不可失败。
  - Rust 现状：`kernel.get_ticks().unwrap_or(0)` 共 17 处（boot.rs:876/:1009、
    shell_update.rs:66/:137/:158、shell_request.rs:610/:981/:1031/:1066/:1281 等）。缝失败
    （ENOSYS 或 19 传输错误）时 prepare_tm/alive_tm/check_tm/prepare 基准归 0：
    `prepare_tm=0` 使 `now - 0 > maxtime` 立即为真（**更新被立即误判超时回滚**）；
    `alive_tm=0` 使心跳判定误报服务死亡。C 无此失败模式，Rust 把"不可失败"翻译成了
    "失败时用 0 继续"——fail-open 劣化而非 fail-closed。
  - 改进思考：方案 a（推荐）——handler 内 `?` 传播（这些 handler 本就返回
    `Result<i32, Errno>`，get_ticks 失败按内核调用失败处理，与 privctl/getpriv 同待遇）；
    方案 b——boot.rs 两处 C 不可败假设点用 `expect`（对齐 C"不可能"语义，失败即程序错误）。
    19 接线前定型，避免接线时 17 处逐一决断。
  - 归属：可立即 todo-fix（一次一个模块，5 个文件）。

- **R38（P2-doc-sync）— doc 19 §3.3 codec 决策声明被实际路线超越** ✅ 已修（2026-09-09，见 §22.7 Fix #94）
  - 现状：19-rs-external-interfaces.md:143 写"Rust 侧以语义层 struct 建模（无
    `#[repr(C)]` 字节布局），`DecodeFromM1`/`EncodeToM1` 实现 DEFERRED——wire-up 时按
    64 字节协议定稿传输层，再补 codec"。
  - 实际演进：①minix-types 已落三个 `#[repr(C)]` 布局见证 + LWT 解码面（`rs_start`/
    `rprocpub`/`rproc`，Fix #81/#85/#89）；②消息槽的解码实际采用了**目标字段读取器**形态
    （`Message::rs_init_result()`/`rs_req_payload()`/`RsUpdate::decode_message`），不是
    泛型 codec；③message 尺寸问题以实测 pinning 关闭（72 字节，commit `deec0c347`）。
  - 改进思考：doc 19 §3.3 补"落地后记"——三决策（LWT 代替泛型 codec、字段读取器代替
    整槽 DecodeFromM1、72 字节实测 pinning），并同步 §4.1。属于"决策文本与代码不同步"
    的 P2（模式 73 同族），不影响正确性。
  - 归属：文档修复，1 轮。

- **R39（P2-工具配置）— semantic-map 回填 `copy_rs_start`（E-RSSTART 关单后的过期缺口）** ✅ 已修（2026-09-09，见 §22.7 Fix #94）
  - 现状：Fix #80 刻意不回填 `copy_rs_start`/`do_edit`（当时是 E-RSSTART 门上的真缺口，
    保持 ⚠️ 可见）。E-RSSTART 已关单且 do_up/do_edit/do_update 三臂 live（Fix #82-#84），
    `do_edit` 已回填而 `copy_rs_start` 遗漏——它是 E-RSSTART 时代留下的唯一过期 ⚠️。
  - 改进思考：映射 `copy_rs_start` → `RsStartWire::decode`（minix-types::ipc::rs_start）+
    `fetch_rs_start`（shell_request.rs:352，两段式取数半）；顺带补 `rs_strerror` →
    `error.rs`（§6.1 新判定）。回填后 name-match 预期 152→154、⚠️ 21→19（= §4 的 B/C/D
    standing 分类数）。
  - 归属：工具配置，随任意轮捎带。

- **R40（P3-组织）— shell 模块测试归属与拆分初衷错位** 🔶 12/16 号域已随迁（2026-09-09，
  §22.7 Fix #95：6 个测试 + 夹具族上移 testutil）；13/14 号域测试待其 handler 触碰时随迁
  （testutil 夹具已就绪，迁移路径已铺平）
  - 现状：Fix #86 拆分时"结构体与全部测试"留在 lib.rs（2378 行，其中测试约 1300 行）——
    shell_request.rs（1789 行）与 shell_update.rs 的 handler 测试全部在 lib.rs
    （`grep -c "#\[test\]" shell_request.rs` = 0）。模块边界=文档域的拆分初衷在测试侧
    未对齐。
  - 改进思考：方案 a——后续接线轮触碰某 handler 时把对应测试随迁（渐进，零专项成本）；
    方案 b——专项迁移轮（机械但扰动大）。推荐 a；本条记录预期，防"测试在哪"的查找成本。
  - 归属：观察项，随 R35/R36 修复轮顺带迁移对应测试。

- **R41（P3-观察，维持记录不动作）— `mem::take(&mut state.update)` 别名墙舞蹈**
  - 现状：do_update 的 prepare 段（shell_request.rs:827-881）为绕开"abort/end 闭包无法
    捕获 state.update 与 table"的借用墙，`mem::take` 取出 UpdateState 再放回。功能正确、
    有注释（A-2 别名墙），但 take/restore 是易漏配对的手工模式（漏 restore 即状态丢失）。
  - 改进思考：低优先。若 19 落地时 Effects 需要真实捕获，考虑 `UpdateState::take`/恢复
    封装为单一出口（或 start_update_prepare 的 abort/end 改为返回值由调用方执行——
    monitor 模式先例 Fix #24）。记录不改。
  - 归属：19 接线期再评估。

- **R42（P3-防误修记录，无动作）— `is_idle` 调用方计算 + 空表恒真已核实忠实**
  - 现状：do_update 备 prepare 前 `state.table.iter_in_use().all(|(_, s)| s.flags.is_idle())`
    （shell_request.rs:826）作为 `start_update_prepare` 的入参——C 的 `rs_is_idle()`
    （utility.c:424-438）是逐 IN_USE 行查 `RS_SRV_IS_IDLE`；Rust 把扫描移到调用方是
    functional-core 既定翻译（T5/A1 方向）。`is_idle` 的位语义（service_slot.rs:100-104）
    与 C 宏一致；空表 `all()` 恒真不可达（RS 自身恒在表内）。本条为防将来被当 bug"修复"。
  - 归属：无动作。

### 6.3 架构对照增量（Redox 2025 / Rust 社区）

- **Redox 服务监督趋同（§7 对照的 2026-09-09 增量）**：Redox 官方 init（redox-os/init）2025
  年重写，显式目标加入"service management / enable-disable / restart failed services"；
  2026-04 月报演示微内核检测并重启 crash/hang 服务（Phoronix 报道）。方向与 RS 的
  monitor（period 决策）→ recovery（terminate/backoff/reincarnate）链一致——业界无监督
  用户的 init 正在向 Minix3 RS 的 1990 年代设计收敛，16/17 号文档设计差异节可引用此对照。
  对本 stage 无架构动作（外部行为契约仍以 C 为准）。[来源：redox-os.org news 250430、
  github.com/redox-os/init；通识性论断，锚点为外部链接]
- **Rust 社区对照（本轮无新增强制项）**：五域 trait 面 + supertrait 组合、Effects 聚合束
  （Box+Default / `&mut dyn` 转发分型——Fix #87 的"`&mut dyn` 字段上 Default 音响性不
  成立"发现）、LWT 布局见证均为社区常规形态的忠实应用；R35 的 peek/walk 分离亦是
  "命令-查询分离"（CQS）原则的直接应用，无需引入新依赖或新抽象。

### 6.4 依赖 crate 卫生（非 rs stage 内，登记 edge）

`cargo clippy` 全链 10 条告警全部来自依赖 crate（rs crate 本体 0）：minix-sys 5 条
（misc.rs:219/:220 与 rmib.rs:85 collapsible-if、syscall.rs:308 doc 后空行、
rmib.rs:127 missing Default）、minix-types 1 条（ipc/vm.rs:667 VmReply 大小差异，既有
known）、workspace profile 声明 4 条（kernel/boot-shim 的非根 profile）。minix-sys 的
5 条卫生项随 E1/E2 的 wrapper 工作顺带清理即可，不单独立项（已登记 edge_todo.md
E-MINSYS-HYGIENE 一句话条目）。

### 6.5 修复优先级路线（供 todo-fix 排队）

1. ~~**R35**（P1，1 轮）：peek/walk 分离 + gate 失败链状态负断言测试~~ ✅ Fix #91；
2. ~~**R36**（P1，1 轮）：do_period 补 update_period 臂 + 超时回滚测试 + 删陈旧注释~~ ✅ Fix #92；
3. ~~**R37**（P2，1-2 轮）：get_ticks 失败策略定型（? 传播 / expect 分点）+ 测试~~ ✅ Fix #93；
4. ~~**R38 + R39**（P2，可并 1 轮）：doc 19 §3.3 后记 + semantic-map 双回填~~ ✅ Fix #94；
5. R40 随 1/2 顺带；R41/R42 记录不动作。
   修复遵循 fix-guard.md（读目标行 ±5、grep 确认、单条修复、修后验证 + 本节标注）；
   每轮完成后 `cargo test -p minix-rs -p minix-types` + clippy/fmt + T7 门。

### 6.7 §22 修复记录（2026-09-09 起，迭代协议同 §18.9）

> 每轮一项：讲明白 → ≥2 方案对比（Linux/Redox/OS 理论/Rust 社区）→ 测试先行实施 →
> 文档同步 → 全门验证（test/clippy/fmt/T7）→ 回归 review（git diff 全量走查）→ 本节
> 标注 → commit。

### ✅ Fix #91 — R35（P1）：`do_upd_ready_shell` peek/walk 分离，gate 失败不再污染链状态

- **File**：`os/servers/rs/src/live_update.rs`（新增私有 `next_prepare_target`——
  walk 与 peek 共享的目标选择规则，update.c:470-477；新增纯 `peek_next`——
  无突变地重放派发循环；`start_update_prepare_next` 头部改用共享助手，`None`
  早退语义不变）、`os/servers/rs/src/shell_update.rs`（`do_upd_ready_shell`
  重排：peek 喂决策 → mutations.apply → 四分支按 C 顺序执行效果；NextPrepare
  臂 `debug_assert!(walked.is_some())` 编码"peek ⇒ walk 必派发"）、
  `16-rs-live-update.md`（接线注记补 R35 设计说明）；todo.md 翻态
- **Before**：`has_next` 由带突变的 `start_update_prepare_next` 急切求值——gate 失败
  （EINVAL）与 prepare 失败两条 C 零突变的路径都会推进 `chain.curr`、武装
  `RS_UPDATING`、（19 后）向下一服务误发 prepare。
- **After**：方案对比——a) 纯 peek 与 walk 共享目标规则（选定：`next_prepare_target`
  单一真相，peek 重放 preparing-only 跳过循环保证返回值与 walk 严格一致）vs
  b) 决策函数收 `&UpdateChain` 自行窥视（破坏 ready.rs 纯函数面，拒绝）vs c) 壳层
  内联 C 式分支（放弃 R13 决策-载荷模式，与 A4 判定冲突，拒绝）。C 侧等价性依据：
  `start_update_prepare_next` 仅在目标选择早退（update.c:470-472）返回 NULL，彼时
  零突变；peek-None ⇔ walk-None-零突变，StartUpdate 臂直接 `start_update` 与 C 的
  可观察序列一致。
- **Tests**（+4，327→331）：`test_peek_next_matches_walk_targets`（空链 None 不武装
  UPDATING / 新链 peek 首 / updating peek next / 尽头 None）、
  `test_peek_next_skips_prepare_only_entries`（peek 返回最后被派发的槽——跳过
  preparing-only）、`test_do_upd_ready_gate_failure_leaves_chain_untouched`（错误
  sender + INITIALIZING 两路 gate 拒绝后 `curr`/flags 不变——旧实现此测试失败）、
  `test_do_upd_ready_walks_two_entry_chain_to_start_update`（双链：首报告走链推进
  curr=1、PREPARE_DONE 归报告者；次报告 peek None → start_update）。
- **Verified**：`cargo test -p minix-rs` = **331 passed**（+4）；clippy rs crate 零告警；
  fmt 干净；`tools/check-rs-unwired.sh` PASS。回归 review（git diff 走查）确认：
  walk 体其余逻辑（VM-multi 预阶段、UPDATING 写点、派发循环）零改动；`end_update`
  的 ESRCH/`start_update_prepare` 路径（do_update 的 `mem::take` 段）未触碰。

### ✅ Fix #92 — R36（P1）：`do_period` 补 `update_period` 检查臂——prepare 超时回滚 live

- **File**：`os/servers/rs/src/lib.rs`（`do_period` 头部：updating && !initializing 门 →
  读 `chain.curr()` 条目 → `monitor::has_update_timed_out` → 超时
  `UpdateState::end_update(EINTR, RS_CANCEL, now)` → SelfTerminate 上抛；扫表同 tick
  继续——C request.c:950-954 的忠实形状；删除"deferred with the 16 wiring"陈旧注释；
  doc 注释同步）、`07-rs-period-heartbeat.md`（§2.1 接线落地注记）、todo.md 翻态
- **Before**：检查臂缺失，`has_update_timed_out` 零生产消费点——prepare 阶段挂死的
  批量更新永不回滚（`RS_UPDATING` 滞留、旧实例困于 updating 门、`effective_period`
  按 LU 初始化超时误喂 ping）。
- **After**：方案对比——a) 壳层组合已有纯决策（选定：纯决策 `has_update_timed_out`
  已存在且语义锁定，壳层只补门 + 读 curr + 执行效果）vs b) 在 UpdateState 上加编排
  方法 `update_period(...)`（C 的 update_period 本就是读 + end_update 的薄组合，多一层
  方法只是搬运）vs c) 塞进 period_decision（污染 monitor 决策面——超时检查属于
  update 域不属于服务心跳域）。`now` 直接用 tick 自带的 CLOCK 时戳（= C end_update
  内部 `getticks()` 同钟值），避免一次多余 seam 调用；`SelfTerminate`（R27b：
  result≠0 且 RS INIT_DONE）按 do_period 既有 crash 形状上抛 `Err(EGENERIC)`
  （C `exit(1)`）。
- **Tests**（+1，331→332）：`test_do_period_update_timeout_rolls_back` 四例——逾期
  tick 回滚（UPDATING 清 + 链清空）、未到期不动、maxtime=0 永不超时（update.c:386
  的 `prepare_maxtime > 0` 门）、initializing 相位豁免（request.c:951）。
- **Verified**：`cargo test -p minix-rs` = **332 passed**（+1）；clippy rs crate 零告警；
  fmt 干净；T7 PASS。回归 review：`end_update` 的 UPDATING debug_assert 在本臂
  恒满足（门先行）；init 豁免分支不触达 end_update；`chain.curr()` 为 None（空链但
  flags 残留的非法态）时跳过检查而非 panic——与 C 的 NULL 解引用相比是防御性收敛，
  不改变合法态语义。

### 6.6 Step 5.7 Rule Discovery

1. **「deferred 标记指向已收工的工期」模式（新）**：R36 的 deferral 注释写于 16 号未落地
   时，16 号接线完成后标记未复核，检查臂成为孤儿——与 R28"漏标"、R21"注释声称的调用方
   路径不存在"同族但方向相反（不是漏标，是**过期标**）。建议 review-patterns 增补：
   DEFERRED 注释的宿主文档落地时，必须反向 grep 该文档域的全部 deferral 标记逐一复核
   （可挂 check-rs-unwired.sh 的 doc-contract 校验扩展）。本轮已按此模式排查全 crate
   deferral 标记（区别于描述 C LATEREPLY/EDONTREPLY 回复延迟语义的英文散文）：**行为类
   过期标记唯 R36 一处**（lib.rs:358，宿主 16 号已落地）；一处准过期（ready.rs:133
   "payload struct deferred to 12's call site"——12 号已落地而载荷收敛未做，属签名
   ergonomics 非行为，§21.1 已带理由记录，维持 allow）；其余宿主（boot.rs×9、exec.rs、
   main.rs、publish.rs、service_create.rs×2 归 19/17/18；sched.rs:9 生产传输归 19/E9；
   slot.rs:181 A-10 PCI 特权模型归 19）经逐条核对确未落地，标记有效。
2. **「纯决策函数的输入急切求值」陷阱（新，R35 根因归纳）**：functional-core 拆分后，
   决策函数需要的输入（如 has_next）若只能由带副作用的编排函数产生，就会出现"为纯度
   而提前执行副作用"的顺序倒置。判定规则：决策输入的计算必须与决策消费解耦——副作用
   版本（walk）与纯版本（peek）应同时提供，壳层按决策结果执行副作用版本。同型风险点
   已排查：do_update 的 prepare 段（决策后执行 ✅）、do_init_ready（决策输入全为纯读 ✅）、
   signal_handler（无决策函数 ✅）——全 crate 仅 R35 一处。

### ✅ Fix #93 — R37（P2）：`get_ticks` 失败策略定型——17 处 `unwrap_or(0)` 全部改为 `?` 传播

- **File**：`boot.rs`（3）/`shell_update.rs`（3）/`shell_request.rs`（8）/`lib.rs`（4）
  共 17 个调用点 + `lib.rs` 夹具参数化（`booted_vfs_kernel`，注入 seam 失败用）+
  `19-rs-external-interfaces.md` §2.1（时钟读取失败语义声明）；todo.md 翻态
- **Before**：C 的 `getticks()` 读 kerninfo 共享页恒成功（getuptime.c:9-23），Rust 缝
  可失败时全部 17 处静默降级为 tick 0——`prepare_tm=0` 使超时判定立即为真（误回滚）、
  `alive_tm=0`/`check_tm=0` 使幸存服务被误判 PingTimeoutCrash（fail-open 劣化）。
- **After**：方案对比——a) 全点 `?` 传播（选定）vs b) `expect`/panic（时钟缝失败是
  传输故障不是程序错误；panic = RS 死亡 = 整机不可用，拒绝）vs c) 缓存上次 tick 回退
  （为 C 不可达路径引入隐藏陈旧状态机，拒绝）。a 的关键论证：**回滚路径上的传播也有
  收敛兜底**——Fix #92 的 do_period 看门狗臂读的 `prepare_tm`/`prepare_maxtime` 未被
  污染，传播后更新保持 armed，下个 CLOCK tick 内照常回滚；传播 = 与同 handler 内
  privctl/getpriv 等内核调用失败同一待遇（一致性）。
- **Tests**（+1，332→333）：`test_clock_seam_failure_propagates_instead_of_poisoning`
  ——`fail_calls` 注入 GetTicks 失败，PrepareFailed 臂返回 Err 且链条保持 armed
  （2 条目不动），锁定"传播而非毒化"策略。
- **Verified**：`cargo test -p minix-rs` = **333 passed**（+1）；clippy rs crate 零告警；
  fmt 干净；T7 PASS。回归 review：18 处替换逐点核对（含 live_update.rs 既有 1 处 `?`
  形态一致）；`booted_vfs_labeled` 重构为委托 `booted_vfs_kernel`（既有调用点零改动，
  A-5 夹具参数化而非新增变体）；无测试依赖旧 fail-open 行为（333 全绿即证）。

### ✅ Fix #94 — R38+R39（P2 合并轮）：doc 19 §3.3 落地后记 + semantic-map 双回填

- **File**：`19-rs-external-interfaces.md`（§3.3 落地后记：三条实际路线取代预案文本——
  ①控制结构字节 ABI pinning（LWT，Fix #81/#85/#89）②消息槽目标字段读取器
  （`rs_init_result()`/`rs_req_payload()`/`RsUpdate::decode_message`）③Message 72 字节
  实测 pinning，commit `deec0c347`；§3.1/§3.2 字段语义表维持有效）、
  `tools/coverage-extract/rs-semantic-map.json`（`copy_rs_start`→`fetch_rs_start`+
  `decode_rs_start`、`rs_strerror`→`init_strerror`+`lu_strerror`，`_note` 记录来源）、
  `03-stage-rs/todo.md`（§4 standing 分类符号名显式化——见下）
- **R39 附带发现（测试自查抓出）**：§22 清理 todo.md 时把 §4 分类表的部分符号名压缩
  掉了，而那 5 个符号（`RS_CONST_H`/`RS_GLO_H`/`RS_TYPE_H`/`DEBUG_DEFAULT`/
  `PRIV_DEBUG_DEFAULT`）的唯一文档引用就是 todo.md——Doc covered 从 100% 掉到
  97.1%（SYMBOLS 复测当场暴露）。修复 = §4 B/C/D 分类逐符号显式列出（含 file:line
  锚点），并加注"本表是覆盖率的文档引用源，压缩会掉 Doc covered"。
- **Verified**：coverage-extract 复测 = **171 C 符号 / Doc covered 171（100%）/
  name-match 154（90.1%）**；剩余 17 个 ⚠️ 逐名核对全部落在 §4 B/C/D standing
  分类（C 类 5 + B/D 类 12），零假阳性。`cargo test -p minix-rs -p minix-types`
  333/189 passed（本轮零代码改动）；clippy/fmt/T7 全绿。

### ✅ Fix #95 — R40（P3）12/16 号域测试随迁 + 夹具族上移 testutil

- **File**：`os/servers/rs/src/testutil.rs`（+148：`booted`/`booted_with`/
  `booted_vfs_labeled`/`booted_vfs_kernel`/`two_entry_chain`/`rs_init_envelope`
  六件夹具以 `pub(crate)` 上移共享）、`os/servers/rs/src/shell_update.rs`
  （新增 `#[cfg(test)] mod tests`：do_init_ready ×2 + do_upd_ready_shell ×4
  随 handler 同模块——R35/R36/R37 三轮触碰的正是这些 handler，R40 方案 a 的
  渐进随迁）、`os/servers/rs/src/lib.rs`（测试模块改经 testutil import 使用
  夹具；2257 行）
- **After**：`grep -c "#\[test\]" shell_update.rs` = 6、lib.rs 残留 0；13/14 号域
  测试（shell_request.rs 的 handler 群）维持 lib.rs 现状，待其 handler 触碰轮
  按同路径随迁（夹具已在 testutil，零前置）。既有测试侧告警（state_data.rs
  hex 分组 / lib.rs `let mut img` / process_table.rs if-let，`--tests` 模式下
  可见）非本轮触碰引入，记录为既有。
- **Verified**：`cargo test -p minix-rs` = **333 passed**（数量零变化——纯迁移）；
  clippy（含 `--tests`）触碰文件零告警；fmt 干净；T7 PASS。

## §22 收敛终态（2026-09-09 快照）

- **stage 内可实施 TODO 全部消化**：R35 ✅（Fix #91）/ R36 ✅（Fix #92）/
  R37 ✅（Fix #93）/ R38+R39 ✅（Fix #94）/ R40 🔶（12/16 域随迁完成，
  13/14 域按方案 a 渐进）/ R41、R42 维持记录（观察项，非可实施 TODO，
  触发条件见 §6.2 条目）。
- **测试基线**：327 → **333 passed**（minix-rs，净增 6）；minix-types 189；
  clippy（lib + tests 触碰文件）/ fmt / T7 全绿；覆盖率 Doc 100% /
  name-match 90.1%，17 个 ⚠️ 全部落在 standing 分类。
- **遗留（全部有主，非本 stage 单独可做）**：E-1（RS 自升级，18 号接线轮）、
  E-11/E9（生产传输 + trap 层，19 号主线，见 edge_todo.md）、A-3/R31 残余
  （诊断面，08-stage-is）、13/14 号域测试渐进随迁（R40 余半）。
- **Rule Discovery 追记（Fix #91-95 实战确认）**：①「deferred 标记指向已收工的
  工期」——R36 修复即例证（16 号落地后注释未复核）；②「纯决策函数的输入急切
  求值」——R35 修复采用 peek/walk 共享目标规则，两轮修复后全 crate 无第三处。
