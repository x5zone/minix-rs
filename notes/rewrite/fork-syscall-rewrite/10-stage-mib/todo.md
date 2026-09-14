# 10-stage-mib Rust 实现架构级 Review TODO（第一轮）

> **来源**：2026-09-15 首轮架构审查（用户指令：查漏补缺优先，再做架构深审；scan-only 轮，未改任何生产代码）。
> **范围**：一等对象 `os/servers/mib/src/`（35 文件 7187 行，107 个单元测试）；契约消费面 `minix-types`（`ipc/mib.rs` 405 行、`types/sysctl.rs` 949 行、`types/com.rs` MIB 号段、`ipc/message.rs` 六载荷）与 `minix-sys/src/rmib.rs`（259 行）；C ground truth `minix3/minix/servers/mib/`（8 个 .c 共 4990 行 + `mib.h` 390 行）与 `minix3/minix/lib/libsys/rmib.c`（1089 行）+ `rmib.h`（188 行）。libc sysctl 客户端为 A-10 外部契约，不在范围内。
> **方法**：三向对账（C ↔ 24 篇文档 ↔ Rust，四档判定：已实现 / 判定半 / 缺口 / 排除）→ 五层深审（L0 组合层 → L1 服务器内部 → L2 内核接缝 → L3 wire → L4 测试五维）→ 对照 Redox / NetBSD sysctl(9) / OS 理论 / Rust 社区（联网核验 2026-09-15，见 §6）。
> **定位**：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`，本文档 §5 只留双向指针。
> **状态（2026-09-15）**：首轮 scan-only 完成。**定性结论**：判定层（verdict）覆盖率高、质量好（Gate E 测试名对账 121/121 零缺失，12 篇逐符号对账无一处虚构执行者）；执行半整层缺席——IPC 壳、执行 walker、transport seam、内存竞技场四大件均未生成；wire 交换格式（A-4）整层未建模。**定量**：P1 × 5、P2 × 4、P3 × 2；edge 新登记 3 条、增补 5 处。

---

## 0. 审查结论速览

MIB 的现状是"语义库完备、服务器不存在"：22 篇文档声称的判定函数全部落地且逐条对得上 C（含静态树槽位 parity 逐槽相同），但 `main.rs:26` 是空转循环、`tree/dispatch.rs:6` 自述的执行 walker 不存在（`LevelVerdict`/`judge_level` 零生产调用方）、`io/copy.rs:7-9` 自述的 transport 半全仓无着落。与 DS（判定 + `server.rs` 装配双 trait）相比，MIB 缺的正是 DS 已完成的装配层。本轮把执行半拆为 P1-1/P1-2/P1-4/P1-5 四个 stage 内 campaign 项，把 A-4 交换格式裁决立为 P1-3，把两处 API 形状决策与两处 verdict 补齐立为 P2；kernel 侧端点常量偏差（grant.rs 用 4/8 而非 1/7）登记为 edge。

| 级别 | 条目 | 一句话 | 状态 |
|---|---|---|---|
| P1-1 | 主循环装配与传输 seam | MibServer + 双/三 trait + run_once，对齐 DS/SCHED 先例；含 UserSpaceTransport 上移再评估结论 | ⬜ |
| P1-2 | 树竞技场与执行 walker | 兑现 04/13/15 四篇文档的 arena 承诺（时点已到且说法矛盾，见 P3-1）；`LevelVerdict` 执行者 | ⬜ |
| P1-3 | A-4 交换格式布局裁决与锚定 | sysctlnode/sysctldesc/kinfo_lwp/kinfo_proc2 整层无结构无断言 | ✅ 2026-09-15（`minix-types::sysctl_abi`，4 结构 + 4 断言测试，196 passed） |
| P1-4 | 拷贝/授权执行半 | mib_oldp/mib_newp 类型化 + datacopy/grant 动词接线 | ⬜ |
| P1-5 | 进程表拉取执行半 | tables.rs 纯半 + getproctab/getsysinfo 接线（对端挂 E-MIBPROD） | ⬜ |
| P2-1 | verdict 层三处 C 判定缺口 | create 溢出门、create 版本门、query 拷入版本门 | ✅ 2026-09-15（`create_csize_ok` + `staged_vers_ok`，108 passed） |
| P2-2 | handler 结果与判定入参结构化 | `map_sysctl_reply` 平行参数通道 → 枚举；`judge_level` 九参 → 事实结构体 | ✅ 2026-09-15（`SysctlOutcome` + `LevelFacts`，`too_many_arguments` allow 已删） |
| P2-3 | 动态子节点容器选型 | C 排序链表 → BTreeMap / 排序 Vec 的裁决 | ⬜ |
| P2-4 | A-3 内存策略落地 | slab + 字节预算池（推荐）vs bumpalo vs 裸全局分配器 | ⬜ |
| P3-1 | 文档账目同步批次 | README/plan 失真行、02 篇行号漂移、arena 承诺时点四处统一、00/99 成文 | ✅ 2026-09-15（五项全闭环） |
| P3-2 | 死依赖与卫生指针 | mib crate 的 minix-sys 死依赖转正时机；rmib 卫生项归 edge | ⬜ |

**基线命令（2026-09-15 实测）**：

```bash
cargo test -p minix-mib --lib   # test result: ok. 107 passed; 0 failed
cargo clippy -p minix-mib       # minix-mib 本体 5 条告警（未用 import P_SINTR@proc2.rs:17 与 proc_args.rs:14、empty-line-after-doc、collapsible-if ×2；minix-sys 依赖的 5 条已登记 E-MINSYS-HYGIENE）→ 归 P3-2 清零
bash tools/design-coverage-check.sh fork-syscall-rewrite   # 10-stage-mib 无缺失行
python3 tools/coverage-extract/coverage-extract.py mib notes/rewrite/fork-syscall-rewrite/10-stage-mib \
  --c-dir minix3/minix/servers/mib --rust-dir os/servers/mib/src   # 见 §1 Gate A
```

---

## 1. 覆盖矩阵（查漏补缺结论）

### 1.1 Gate E 测试名对账（全量，非抽样）

22 篇概念文档 §5 声称的测试函数逐名 `rg "fn {name}"` 对账：**22 篇合计声称 90 行，命中 90，缺失 0**；13~20 八篇进程信息文档合计 31 行，命中 31，缺失 0。两处落点差异（非缺失）：02 篇的 15 个测试全部落在 `os/libs/minix-types`（该篇基线自declared）；11 篇有 1 行复用 `sysctl.rs:745` 的既有测试（文档自注）。测试总数快照漂移：13/16/17 篇自报 94、14/15/18 篇自报 101、19/20 篇自报 107——均为写作时点快照且各篇已声明"随阶段推进增长"，当前实测 107。

### 1.2 三向矩阵：C ↔ 文档 ↔ Rust

逐符号对账（基准 = plan.md §5.3 的 82 函数 + 13 静态表 + 3 计数）的结论：

- **判定半为主体，且与 C 逐函数对得上**。12 篇核心文档（01~12）的每个 C 符号都有对应 verdict 或显式排除声明，无一处虚构执行者。典型：`mib_sysctl` 六道门 ↔ `dispatch.rs:126/153/175/197/217`；`mib_dispatch` 九判 ↔ `tree/dispatch.rs:95 judge_level`；`mib_scan` ↔ `tree/dynamic.rs:81 scan`。
- **静态树槽位 parity 逐槽相同，Rust 缺槽：无**。kern 45 填充 + 39 排除 = 84（`subtree/kern.rs:114-346` ↔ kern.c:332-498）；vm 4 + 9 = 13（`subtree/vm.rs:52-89` ↔ vm.c:120-144）；hw 10 + 6 = 16（`subtree/hw.rs:71-135` ↔ hw.c:98-130）。A-9 排除契约（KERN/VM/HW_UNIMPLEMENTED 清单）与 C 注释槽一一对应。
- **执行缺口集中于同一组声明**：arena（04/08/13/15 四篇承诺）、transport（A-12：sys_datacopy / cpf_grant_magic / getnuid / ds_retrieve_label_name / ipc_sendrec / sef_setcb）、IPC 主循环（main.rs:26 占位）。三个缺口互为前置，构成本 todo 的 P1 campaign。
- **判定级缺口三处**（12 篇对账中唯一"概念提及但符号表无行"的差异）→ P2-1：create 侧孩子数溢出门（tree.c:517-519）、create 侧 `SYSCTL_VERS != VERS_1` 门（tree.c:536-538；mount 侧 `check_head` 有、create 侧无）、query 拷入侧版本门（tree.c:194-195）。`newp == NULL` 门（tree.c:522-524）已由 01 配对层 `pair_newp` 代判，不缺。
- **有意并入（非缺口）**：`mib_copyin_aux` 无独立 verdict（06 篇 §4.4 自declared"断言即契约"）；kern.ipc 子表无独立数据表（`KernKind::IpcTable` 占位 + mock 判定，A-9 契约内）。

### 1.3 Gate A 覆盖枚举（coverage-extract 实测）

`coverage-extract.py mib`（--c-dir/--rust-dir 显式指定）：C 符号 63（38 函数 + 5 结构 + 20 宏），文档覆盖 55（87.3%），Rust name-match 15.9%。name-match 偏低是命名演进的预期结果（`mib_inrange` → `in_range` 等 verdict 重命名），语义对应以 §1.2 三向矩阵为准，name-match 数字不作为缺口依据。5 个"完全缺口"逐一判定：`_MINIX_MIB_MIB_H`（include guard，ARCH 不需要）；`MIB_FLAG_AUTH`/`MIB_FLAG_NOAUTH`（mib.h:49-50，内部 call_flags 位 → Rust `CallAuth` 枚举 ARCH 替代，语义等价）；`HASH_SLOTS`（proc.c:33 → `proc/tables.rs:29-31` 槽位数学已覆盖，命名不同）；`CONFIG_MAX_CPUS`（mib.h:14 → `subtree/hw.rs:80` 以 `HwKind::BuildInt("CONFIG_MAX_CPUS")` 命名保留）。**真实缺口：0。**

---

## 2. 新条目

### P1-1 主循环装配与传输 seam：把"判定库"变成"服务器"（stage 内）

**问题**：`main.rs:26` 非 test 构建只有 `#[allow(clippy::empty_loop)] loop {}`（自注"Spinning is intentional until the loop lands"）。`dispatch.rs` 已备齐 triage/`default_outcome`/`should_reply` 与 sysctl 解码判定，但没有任何装配方消费它们。对照 C `main.c:433-492`：`sef_receive_status` → notify 拒绝 → 三路 switch → 回信规则，循环本体整层缺席。transport seam 也未建：`io/copy.rs:7-9` 自述 moves 属 transport（A-12），`auth.rs` 的 getnuid PM 往返、`remote.rs` 的 DS label 查询、`io/relay.rs` 的 grant 创建全部停在判定半。Cargo.toml 声明的 `minix-sys` 依赖当前零引用（死依赖）。

**影响**：MIB 不能出生；107 个测试全部跑在纯函数上，生产语义零验证。这是 22 篇"已 review"文档与"服务器不存在"之间唯一的结构性缺口。

**建议**（对照仓库内三个先例，方案对比见 §4 L0）：
- **方案 A（推荐）**：沿 DS `server.rs` 先例（`os/servers/ds/src/server.rs:34-79`）——`MibServer` 结构持有树/远程表/认证缓存，双 trait `MibIpc`（receive_status + send_nb + send_rec）+ `MibKernel`（datacopy 双向 + cpf_grant_magic/revoke + getticks/hz）+ 第三 trait `MibServices`（getnuid/getsysinfo/ds label/vm_info），真实端委托 minix-sys（E1 通电即活），mock 端脚本化测试。MIB 与 DS 的形状差异要显式处理：`sef_receive_status` 的 notify 检测来自 status 而非 call number（`dispatch.rs:84-87` 注释已声明优势）；远程子树转发需要**嵌套 `ipc_sendrec`**（remote.c:341-355），DS 没有这个动词。
- **方案 B（否决）**：把 SCHED 的 `IpcTransport` + `KernelApi` 形状直接照搬。SCHED 没有嵌套 sendrec 与 grant 动词，照搬会在 P1-4 重新返工 trait 形状。
- UserSpaceTransport 上移评估（E-DSWIRE 字据的触发点复核）：MIB 是第 5 个 seam 消费者，但五个消费者的形状仍在发散（IS 要 sef+fkey、RS 五域、MIB 要 grant+sendrec+服务查询），**维持暂不上移判定**，触发条件不变（E1 trap 层落定后随执行轮再评估）。字据留在 `../edge_todo.md` E-DSWIRE。

**验证**：`cargo test -p minix-mib` 基线 107 → 新增 run_once 装配测试（notify 臂、三信臂、default 双态、回信规则各 ≥1）；`MibCall::from_raw`/`triage`/`should_reply` 从测试消费转为生产消费（grep 调用方非零）。

### P1-2 树竞技场与执行 walker：兑现 04/13/15 的 arena 承诺（stage 内）

**问题**：`tree/dispatch.rs:6` 自述"the walker (10's follow-up with the arena) executes the verdicts"，但 walker 不存在：`LevelVerdict`/`judge_level`/`resolve_shape`/`judge_remote_result` 生产调用方为零（grep 仅定义处与 `tree/mod.rs:22` 再导出）。全 crate 7 处 follow-up 标记（query.rs:5、tree/lookup.rs:4、tree/init.rs:89、tree/dispatch.rs:6、tree/mod.rs:7、data/readwrite.rs:7、tree/dynamic.rs:7）。竞技场本体同样缺席：`tree/node.rs:5-7` 声明"real nodes land in 04/05/08"，实际 `RootSpec`（static_tree.rs:116）只是规格、`TreeCounts` 只是纯累加器、文档承诺的兑现时点在 04/08/13/15 四篇里说法不一（04:82 与 08:82 说"13 首表落地时建"、13:112 说"15 统一建"、15:136-138 说"15 之后的独立后续任务"——15 篇同时宣布"可以统一建了"）。`remote.rs:22-28` 的 `EndptSlot` 缺 C 的 `nodes` 链头字段（remote.c:31-35），挂载点链表无建模。

**影响**：MIB 的全部核心语义（解析、读写、枚举、挂载）停在"逐层判定"，没有一条请求能真正走完树。P1-1 的装配层没有 walker 就只能装配出空壳。

**建议**：竞技场选型决策（P2-4）先行；walker 按判定/执行切分消费 `tree/dispatch.rs` 现有 verdict——每轮 `lookup`（`tree/lookup.rs:81 find`）→ 可见性（`auth.rs:73 can_see`）→ `judge_level` → 执行动作（descend/remote call/handler/readwrite），终态用 `terminal_code`/`EISDIR_EMPTY` 查表。`mib_find` 的生产消费方同步从 0 转正。挂载链表补 `EndptSlot.nodes` 头字段与摘链/头插执行（remote.c:189-191/:257-275）。`TreeCounts` 从纯累加器转为 server 持有的真实计数器（minix.mib.* 统计子树的数据源）。

**验证**：walker 集成测试走通 C tree.c 的全部终态（ENOENT/ENOTDIR/EISDIR/EPERM/CallFunc/Readwrite/Remote 三去向）；`cargo test -p minix-mib` 新增 ≥8 个 walker 用例。

### P1-3 A-4 交换格式布局裁决与锚定：wire 契约整层空白（stage 内决策 + 契约面落地）

**问题**：用户态直接解释的交换格式整层未建模——`sysctlnode`/`sysctldesc`（C sys/sys/sysctl.h:1382-1450，NetBSD `SYSCTL_VERSION=VERS_1`）、`kinfo_lwp`（:647-673，26 字段）、`kinfo_proc2`（:383-495）、`clockinfo`/`loadavg`/`uvmexp_sysctl` 在 `os/` 全树**无结构体定义、无 size_of/offset_of 断言**（minix-types `types/sysctl.rs` 949 行只有 ID/标志值表；`offset_of` 在该文件零出现，而同库 `ipc/rproc.rs:95` 有现成先例）。唯一的布局常量是 `describe.rs:16-22` 手写的 `DESC_HEADER=12`/`DESC_ALIGN=4`。半锚定项：`MinixProcList`/`MinixProcData`（sysctl.rs:177-220）有 repr(C) + size_of（16/72 字节）但无 offset 断言。文档立场是 A-4"延后"（17/18 篇 §1.2/§3），但 P1-2 的 walker 执行 `mib_copyout_node`/`mib_describe` 时必须逐字段产出字节——没有裁决就没有执行体可写。

**影响**：11/17/18/19 四篇的执行半全部被阻塞；ProcFS（15-stage-fs）未来直接消费 `MinixProcList/Data`（A-5），布局错位是行为级事故（同 E-ISPROD 对 IS 的警告）。

**建议**：
- **方案 A（推荐）**：minix-types 建 `types/kinfo.rs`（或 sysctl.rs 扩展）：`#[repr(C)]` 全结构 + `size_of` + 逐字段 `offset_of!` 断言（风格模板 `ipc/rproc.rs` 的 witness 派生），`SYSCTL_VERSION` 常量钉 VERS_1。交换面保真（Rewrite 边界），内部树不受影响。与 E-ISPROD 的快照权威裁决协同做一次（producer/consumer 共用同一批 repr(C) 结构）。
- **方案 B（否决）**：02 篇现立场"不预定结构、11 篇落地时逐字段序列化"。逐字段手排等于把 offset 断言散进执行代码，错位不可编译期发现；且 17/18 篇的 kinfo 布局迟早要 pin，晚 pin 的代价是执行体返工。
- 附带裁决：A-5 的 `MinixProcList/Data` 补 offset 断言升级为全锚定。

**验证**：minix-types 新增布局断言测试（手排十六进制锚定，风格同 `test_mib_wire_layouts` message.rs:3784）；`MinixProcList`/`Data` 的 offset 断言对照 C minix/sysctl.h:61-86。

**复核 ✅（2026-09-15，本条目闭环）**：核心四结构落 `minix-types/src/types/sysctl_abi.rs`——`SysctlNode`（96B，union `SysctlNodeUn` + child/data 两臂，`__sysc_pad` 的跨模型稳定性在模块文档显式论证：ILP32/LP64 逐偏移一致是 C 头自己的设计目标）、`SysctlDesc`（16B，头 12B）、`KinfoLwp`（128B）、`KinfoProc2`（680B，growth-only 尾部契约记录在案）+ `KI_*` 常量与 `KiSigset`；4 个断言测试把逐偏移布局钉死（手算表与 `offset_of!` 编译器实测一致）。附带：(a) `MinixProcList`/`MinixProcData` 升级 offset 级锚定（A-5 收口）；(b) `describe.rs` 的 `DESC_HEADER`/`DESC_ALIGN` 从手写常量改为从 `SysctlDesc` 派生（消灭双真相源）；(c) 02 篇 D-否决行显式改判（前提不成立：`__sysc_pad` 已保证模型稳定），17/18 篇补 P1-3 指针；plan.md A-4 行 →"已兑现（核心），尾随结构（clockinfo/loadavg/uvmexp_sysctl/kinfo_drivers/ps_strings）随各自 handler 首消费时钉"。minix-types 196 passed（+4），minix-mib 108 passed。

### P1-4 拷贝/授权执行半：io 层类型化 + 内核动词接线（stage 内）

**问题**：`io/copy.rs` 9 个拷贝原语全部停在"what would move"：`copyout_span` 不执行 `sys_datacopy`（main.c:136-138）、`check_copyin` 不执行拷入（:183-184）、`io/relay.rs:63-89` 的 grant 以 `Some(0)` 占位（`cpf_grant_magic` 创建 + 失败 EINVAL 回路 + 逆序 revoke 全无，remote.c:396-413/:441-446）。`mib_oldp`/`mib_newp` 在 C 是带游标的不透明体（main.c:70 起），Rust 侧退化为 `Option<u64>` 长度参数，walker 届时被迫在自持状态里并行维护 endpt/addr/len/cursor 四元组。授权半：`auth.rs:47-65` 的 `CallAuth` 缺 getnuid PM 查询执行（main.c:265-268）。

**影响**：P1-2 walker 的 Readwrite/RemoteCall 两类终态没有执行载体；relay grant 占位使远程子树转发在通电后必然错误。

**建议**：把 `Oldp`/`Newp` 建为 io 层类型（`endpt + base + len + cursor`，方法化 `in_range/copyout_span/check_copyin/relay_*`，现有自由函数改为方法体），grant 槽位作为字段持有真身——这是 C 不透明体的 Rust 等价物而非 translate（C 用全局 struct + 指针，Rust 用值语义 + 所有权，外部行为不变）。kernel 动词 trait 归 P1-1 的 `MibKernel`。auth 执行半：`CallAuth::resolve` 接 `MibServices::getnuid`，缓存语义保持（每 call 一次）。

**验证**：mock transport 上的 copyout/copyin 往返测试；relay 的三 grant 创建/逆撤顺序断言（对照 remote.c:396-413 的后授先撤）；错误传输码上浮路径。

### P1-5 进程表拉取执行半：tables 纯半接线（stage 内，对端挂 E-MIBPROD）

**问题**：`proc/tables.rs` 有完整的拉取纪律判定（`PullVerdict`/`judge_pull` 节流+失败闩锁、双魔数 `PMAGIC`/`MP_MAGIC`、PID 哈希、`ticks_to_timeval`、wmesg 映射），但 C proc.c:46-217 的执行半整缺：`getticks()`（:66）、`sys_getproctab`（:75）、`getsysinfo(PM, SI_PROC_TAB)`（:90）、`getsysinfo(VFS, SI_PROCLIGHT_TAB)`（:106）、逐行魔数扫描、哈希表实建、`tabs_valid`/`tabs_updated` 静态存储。`getsysinfo`/`getproctab` 在 mib crate 只出现在注释里（tables.rs:5,77-79）。

**影响**：16~20 五篇的快照消费没有数据源；拉取纪律判定无真实输入。

**建议**：拉取动词归 P1-1 的 `MibServices` trait（getproctab 走 kernel、getsysinfo 走 PM/VFS）；快照结构依赖 P1-3 的布局裁决（kernel 侧 `ProcInfoStruct` 已实现 GET_PROCTAB chunked 拷贝，os/kernel/src/misc.rs:894——布局对齐与 PM/VFS producer 缺位登记 `../edge_todo.md` E-MIBPROD）。闩锁状态（tabs_valid）作为 `MibServer` 字段（C 是静态变量，Rust 归 server 所有——单线程事件循环下等价）。

**验证**：mock services 上的 judge_pull 三态测试已有（tables.rs:196）；补"拉取失败→闩锁→不再重试"的执行级测试。

### P2-1 verdict 层三处 C 判定缺口补齐（stage 内，小）

**问题**：§1.2 识别的三处判定级差异：(a) create 侧孩子数溢出门——C `node_csize == INT_MAX → EINVAL`（tree.c:517-519），Rust `dynamic.rs` 无对应；(b) create 请求的 `SYSCTL_VERS != SYSCTL_VERSION → EINVAL`（tree.c:536-538）——mount 侧 `tree/mount.rs check_head` 有版本门、create 侧没有；(c) query 拷入侧版本门（tree.c:194-195）——`query.rs:38` 只有 `query_ver_ok` 一半。

**影响**：walker 落地后这三条路径会静默放行 C 拒绝的请求（行为偏离）；(b) 还会让错误版本号的 create 挂到树上。

**建议**：`dynamic.rs` 补 `create_csize_ok(parent_csize: u32) -> bool`（对照 :517-519 的 INT_MAX 上界，Rust 侧用 `u32::MAX` 并加 ARCH 注）与 create 版本门（复用 `query.rs` 的版本常量）；`query.rs` 补拷入侧门。三处各配一枚测试，锚 C 行号。

**验证**：`rg "INT_MAX|SYSCTL_VERSION" os/servers/mib/src/tree/dynamic.rs os/servers/mib/src/query.rs` 出现门实现；新增 3 测试通过。

### P2-2 handler 结果与判定入参的结构化（stage 内，walker 前置形状决策）

**问题**：两处 API 形状有误用风险。(a) `map_sysctl_reply(handler_result: i64, oldaddr, oldlen, error_reslen)`（dispatch.rs:217-233）把 A-11 的"错误 + 附带 oldlen"拆成两个平行参数——walker 调 handler 时必须自己维护 i64 与 error_reslen 的配对不变量，配错编译期无感。plan.md A-11 自己预设的形态是 `Result<usize, (Errno, Option<usize>)>`。(b) `judge_level` 九个裸参数（tree/dispatch.rs:94-105，`#[allow(clippy::too_many_arguments)]` 自证）——相邻两个 bool 交换位置照样编译。

**影响**：walker 是这些 API 的第一个真实调用方，形状决策拖到 walker 落地时就是返工。

**建议**：handler 结果类型化 `enum SysctlOutcome { Done(u64), ErrWithResLen(i32, u64) }`（`map_sysctl_reply` 保留为最终 wire 映射器，入参改收枚举）；`judge_level` 入参聚合为 `struct LevelFacts { leaf, remote, can_restart, func, verify, remaining, has_new, flags, auth }`。两案都让非法状态不可表达（review-code-excellence §16.1）。

**验证**：签名迁移 + 既有测试改造 + `too_many_arguments` allow 删除。

### P2-3 动态子节点容器选型（stage 内，P1-2 前置）

**问题**：C 的动态子节点是按 id 排序的侵入式链表（tree.c:34-88 查找可提前终止；:465-467 插入 O(n) 扫描定位）。Rust 竞技场（P1-2）必须选容器：`tree/dynamic.rs:81 scan` 已假设"有序切片 + insert_at"接口，plan.md A-2 预案写了"链表→BTreeMap/Vec+索引"但未裁决。

**建议**：**方案 A（推荐）**`BTreeMap<i32, NodeId>`——有序迭代免费（CTL_QUERY 按 id 序输出，11 篇执行半需要）、查找/插入/删除 O(log n)、与 `scan()` 的有序假设天然对齐（scan 保留纯判定，walker 用 range 查询适配）。**方案 B**排序 `Vec<(i32, NodeId)>` + `insert_at`——缓存友好、省每节点分配，但 create 的 O(n) 搬移与删除的 O(n) 压缩在节点数大时劣化，且 `insert_at` 索引在并发无虞的单线程下并无优势。**方案 C（否决）**直译链表——translate 防线（模式 16）：C 选链表是为了在静态数组上省分配，Rust 有 alloc，直译只继承缺点。节点规模依据：`MAX_REMOTE_CHILDREN=4096`（node.rs:76）量级下 BTreeMap 足够。

**验证**：选型写进 `tree/mod.rs` 模块文档 + [ARCH] 三处一致标注（对照点 mib.h 链表字段、design 03/08 篇、代码注释）。

### P2-4 A-3 内存策略落地（stage 内，P1-2 前置）

**问题**：crate 当前 alloc-free（`lib.rs:1` 仅 `no_std`，无 `extern crate alloc`）。C 的分配语义有两条硬约束：分配失败**报 EINVAL 不报 ENOMEM**（tree.c:589,1043,1207——仅临时挂载点例外报 ENOMEM，:1741-1744），以及 SEF 重启丢全部动态状态（main.c:415-431 注释）。A-3 一直悬置（04:82"arena 缺口→13"、08:82"arena→后续"）。

**建议**：**方案 A（推荐）**`extern crate alloc` + slab `Vec<Node>`（`NodeId(u32)` 索引）+ **字节预算池**（data/desc 缓冲，DS `DsPool` 先例 `os/servers/ds/src/heap.rs`）——预算耗尽即 C 的"分配失败报 EINVAL"，语义逐条对得上，服务自持上限不依赖全局分配器行为。**方案 B**bumpalo——重启整体复位确实优雅贴合"重启丢动态"，但树内交错增删（create/destroy 长期共存）与 bump 只增不减的模型冲突，只适合阶段作用域，否决为主竞技场方案。**方案 C**裸全局分配器无上限——无法兑现"失败报 EINVAL"的预算语义，否决。OWNDATA/OWNDESC 所有权用 Rust 所有权建模（drop 释放 + `TreeCounts::object_removed` 接线），`RemoveDelta`（dynamic.rs:274-294）已备差量语义。

**验证**：`extern crate alloc` + 预算池单元测试（耗尽→EINVAL 路径、释放→预算回收）；重启路径（`MibInitKind::RestartLossy`，sef.rs:24-32）清空竞技场的测试。

### P3-1 文档账目同步批次（doc，机械修复）

**问题**（全部已核实原文）：
1. `README.md:3`"骨架就绪（各 doc 为最小骨架，待按 plan.md 改写）"与 plan.md §6.1 的 01-22 全 reviewed 矛盾（README 未随写作推进更新）。
2. `plan.md:6` 与 `plan.md:467`"Rust 实现（当前为 stub）"失真（实际 7187 行、107 测试）；`plan.md:207` §3.5"无任何测试"基线失真；`plan.md:110` A-1 行"缺 mib.rs 消息类型"过期（`ipc/mib.rs` 405 行已在）。
3. 02 篇行号漂移：引用的 message.rs 结构行号 2595-2760/198-208 实际为 3014-3196/218-228；mib.rs 两处 211,222 实为 218,226（模式 77 同款，结构存在仅行号过时）。
4. arena 承诺时点四处矛盾（04:82/08:82 说 13 落地时、13:112 说 15 统一建、15:136-138 说 15 之后独立任务）——P1-2 动工时必须先统一为一句。
5. `00-mib-overview.md`（21 行）与 `99-mib-global-concepts.md`（20 行）仍是最小骨架，plan.md §6.1 两行 pending——是 24 篇中仅有的未成文两篇。

**建议**：P1-2 开工前先做 1-4（半小时级，模式 83 锚点纪律）；00/99 成文排执行轮末（00 补导航时把 §1.2 启动图对齐现状，99 的 `MIB_PROC_NR=7` 已写对——与 kernel grant.rs 的 8 相比它是正确的一方）。

**验证**：`sed -n '3p' README.md` 状态行更新；`rg -n "2595|2618|2655" 02-mib-message-contract.md` 归零。

**复核 ✅（2026-09-15，本条目闭环）**：五项全落地——(1) README.md:3 状态行更新为"22 篇 reviewed + 判定层 7187 行/107 tests"；(2) plan.md 四处失真修正（对照行、A-1 行 →"已兑现：ipc/mib.rs + message.rs 六载荷"、§3.5 基线更新为 107 passed、参见行）；(3) 02 篇 3 处行号修正（message.rs 六结构 →3014/3037/3074/3098/3144/3179、union 臂 →218-228、mib.rs →218/226，替换值均当场 grep 核实）；(4) arena 承诺统一——15 §4.4 为单一真值（兑现点 todo.md P1-2），04:82/:121、08:82、13:112 全部改指该处；(5) 00/99 成文（00：导航+启动主线+双次主线+设计原则；99：常量/错误码/跨服务引用收口，全部锚点当场核实），plan.md §6.1 两行 pending → reviewed。

### P3-2 死依赖与卫生指针（stage 内，随 P1-1 转正）

**问题**：`os/servers/mib/Cargo.toml:14` 声明 `minix-sys` 依赖但全 crate 零引用（DS 同款先例：声明→E-DSWIRE 落地时转正）。`sef.rs:13-14` 声称 SEF transport 留给 minix-sef 但 Cargo.toml 未声明该依赖（minix-sef 现为 5 行占位 crate，实装已登记 E-ISWIRE）。`os/servers/ipc-server` 同样死依赖 minix-sys 且其文档 03 篇三处声称"复用 minix-sys 的 MountTable"但代码零引用（`os/servers/ipc-server/src/mib_tree.rs:16` 注释自述复用、:18 实际 import 只有 minix-types）——该失真属 13-stage-ipc 域，指针留在 edge E-RMIBWIRE，本 stage 不代修。

**建议**：mib 的 minix-sys 死依赖在 P1-1 落 `SysIpc` 真实端时自然转正（DS 同款），不单独修；sef 依赖随 E-ISWIRE。附带卫生批：crate 本体 5 条 clippy 告警随执行轮清零（`cargo clippy -p minix-mib` 实测：未用 import `P_SINTR`@proc2.rs:17 与 proc_args.rs:14 一处、empty-line-after-doc、collapsible-if ×2）。本轮仅登记。

**验证**：`cargo clippy -p minix-mib` 本体告警归零；转正后 `rg "minix_sys" os/servers/mib/src/` 非零。

---

## 3. 存量复核：plan.md ARCH 表 A-1~A-12 staleness

| # | plan.md 原状态 | 本轮复核（锚点实测） | 处置 |
|---|---|---|---|
| A-1 MIB 消息类型 | "缺口：minix-types 新增" | **已兑现**——`ipc/mib.rs` 405 行 + message.rs 六载荷 + 56 字节断言（message.rs:3784）；plan.md:110 行过期 | §3 存档；plan.md:110 归 P3-1 |
| A-2 节点建模 | "待设计" | 判定半已落（flag/node/lookup/dynamic 四组 verdict + A-2 声明的 NodeKind/ChildMap 方向一致）；竞技场本体未建 | 归 P1-2/P2-3 |
| A-3 动态内存 | "待设计" | 悬置未决，crate 仍 alloc-free | 归 P2-4 |
| A-4 交换格式 ABI | "待决策" | 未建模（§1.3/P1-3 证据） | 归 P1-3 |
| A-5 minix_proc 布局 | "待决策" | 半决：MinixProcList/Data repr(C)+size_of（sysctl.rs:868-869），offset 未钉 | 并入 P1-3 |
| A-6 进程表快照 | "待设计（跨 stage）" | 判定半齐（tables.rs），执行半与对端双缺 | stage 内归 P1-5；对端归 E-MIBPROD |
| A-7 SEF | "参照先例" | 语义半已落（sef.rs 两种 init 名），minix-sef 依赖未声明、实装占位 | 挂 E-ISWIRE；接线归 P1-1 |
| A-8 RMIB 客户端归属 | "待实施" | minix-sys/rmib.rs 簿记半（259 行 vs C 1089），协议半整缺 | 归 E-RMIBWIRE |
| A-9 死/未实现排除 | "排除（grep 实证）" | **已兑现**——45+39/4+9/10+6 三表逐槽核对一致 | §3 存档 |
| A-10 libc 客户端 | "外部契约（不实现）" | 维持（Rust 侧无 libc crate，minix-sys/misc.rs:239 只有裁决助手） | §3 存档 |
| A-11 错误码特殊语义 | "已具备（类型建模待写）" | 半兑现：`map_sysctl_reply` 的 error_reslen 通道已建，结构化建模未做 | 归 P2-2 |
| A-12 外部依赖原语 | "待实施（依赖链）" | 全部停在判定半；trait 归属已设计（P1-1 三 seam），通电挂 E1/E2 | 归 P1-1/P1-4 + edge |

---

## 4. 分层深审记录（code-excellence 五层）

### L0 组合层

- **判定/执行切分本身是对的**（对比三先例：DS 判定+装配双半齐、SCHED 装配已接线、IS acquire/render+run_dump 已填实）。切分的**收口件**（装配层）是 MIB 独有的缺口——三个先例没有谁缺这一整层。判 P1-1/P1-2。
- **UserSpaceTransport 上移再评估**（E-DSWIRE 字据触发点）：五消费者形状仍发散（IS 的 sef+fkey、RS 五域、MIB 的 grant+嵌套 sendrec+三服务查询），上移会在 E1 前冻结错误形状。**判定闭合：维持暂不上移，触发条件不变**（E1 落定后随执行轮评估）；MIB seam 命名与 trait 拆分方式对齐 SCHED/DS 以降低未来抽取成本。
- `MibServer` 形状：DS 先例"created once and never moved"（server.rs:82-90，池内指针约束）对 MIB **不需要**——MIB 没有长期借出指针（C 的 rno_node 裸指针在 Rust 里是 NodeId 索引），竞技场值语义即可整体移动。设计时不要照抄 DS 的不可移动约束。

### L1 服务器内部

- `judge_level` 九参（tree/dispatch.rs:94）与 `map_sysctl_reply` 平行参数（dispatch.rs:217）是仅有的两处误用风险点 → P2-2。
- 动态子节点容器未裁决 → P2-3（推荐 BTreeMap，否决链表直译）。
- 内存策略未裁决 → P2-4（推荐 slab+预算池；bumpalo 否决理由：create/destroy 长期交错与 bump 模型冲突）。
- `ChildWindow`/`RemotePack`（node.rs:21-101）把 C 位域 union 的不变量建模为拒错构造器（clen>csize 拒、越界 lane 拒）——卓越级，无改进点。
- `scan()`（dynamic.rs:81-133）的第二趟越位重扫（:126-131，对应 C :345-354"提前终止会漏同名"）有 `debug_assert!(did > id)` 兜底——正确且自证。

### L2 内核接缝

- 远程转发的**嵌套 sendrec**（remote.c:341-355）是 MIB seam 独有动词：单线程循环内 sendrec 到远程服务会阻塞自身直到对方应答——与 C 行为一致（C 同样阻塞），**判定闭合：非偏离，不设计超时**；但 `MibIpc` trait 必须显式含 `send_rec`，并在注释声明"阻塞语义 = C parity"。
- relay 三 grant 的后授先撤顺序（remote.c:396-413）在 `RelayRegion` 判定半只有方向 flag（relay.rs:37-43），执行半 P1-4 落地时以测试钉顺序。
- getnuid 缓存：C 把结果写 `call_flags`（mib.h:49-50 位标志），Rust 用 `CallAuth` 枚举——ARCH 替代，行为等价（每 call 一次查询），Gate A 判定记录在案。
- tables 闩锁状态归 `MibServer` 字段（C 静态变量 → 单线程 server 所有权等价）——P1-5。

### L3 wire

- 六载荷 56 字节断言在位（message.rs:3784-3791 + mib.rs:352），union 六臂齐（message.rs:218-228），MIB 号段与 COMMON_MIB_* 值全部钉死（com.rs:323-337）。**六载荷层无缺口。**
- 交换格式层（sysctlnode/sysctldesc/kinfo 族）整层空白 → P1-3（本轮最大 wire 缺口）。
- `RegisterView` 不携带 flags/csize/clen 三 lane（mib.rs:155-162）——挂载窗口判定以函数参数收值，wire→判定的传递通道在 P1-2 挂载执行时补，届时不得复用 `SysctlRequest` 通道（防 lane 错位）。

### L4 测试五维

- 完备性：verdict 层分支覆盖好（每篇 §5 对账全命中）；**执行半零覆盖是结构性预期**（代码不存在），P1 各条已带新增测试要求，不另立缺陷项。
- 自身正确性：抽验 `judge_level`/`map_sysctl_reply`/`scan` 断言体与 C 行号锚一致；全量 5 维审计按 cmds 单一目标原则留给独立 `test-audit` 轮（发现 → 记录不执行）。
- 冗余/无效：107 个测试无 `#[ignore]`、无 assert 永真模式（抽验）；`dupe` 风险集中在即将改造的 `map_sysctl_reply`/`judge_level` 测试——P2-2 迁移时同步合并。
- 虚构对账：Gate E 121/121 零虚构；文档 §5.1 的总数快照漂移（94/101/107）已声明时点，非虚构。
- 行为契约缺口：rmibtest.c 的 8 个远端拒绝场景 + 注册顺序契约（rmibtest.c:126-177,183-201）目前无 Rust 对应测试——归 E-RMIBWIRE 的联调验收面（E5(g)），不属 stage 内单测。

---

## 5. 边界条目双向指针

| edge 条目 | 来源 | 一句话 | mib 侧关联 |
|---|---|---|---|
| E-RMIBWIRE（新登记） | 本轮 §2/P3-2、A-8 | minix-sys rmib 客户端协议半整缺（259 行簿记 vs C 1089 行），消费方 ipc-server 文档声称复用但零代码引用 | 22 篇执行半的对端 |
| E-MIBPROD（新登记） | 本轮 P1-5、A-6 | MIB 快照消费 vs kernel/PM/VFS producer 布局对账（kernel GET_PROCTAB 已实现；PM/VFS getsysinfo 数据路径 fail-closed） | P1-5 的对端 |
| E-MIBGRANT（新登记） | 本轮 §4/L2 采证 | kernel grant.rs:293/295 端点常量 VFS=4/MIB=8 与 C com.h（1/7）单点偏差，magic grant 门行为错误 | P1-4 relay 通电的正确性前提 |
| E-DSWIRE（增补） | 本轮 P1-1 | mib_get_label 为 ds_retrieve_label_name 消费方 | remote.rs DS 查询执行半 |
| E-ISWIRE（增补） | 本轮 P3-2 | mib 为 minix-sef 第二消费方（sef.rs:13-14 声称、Cargo 未声明） | P1-1 的 SEF 半 |
| E5（增补 (g)） | 本轮 L4 | MIB 联调验收面：MIB_SYSCTL 往返 + rmibtest 注册/转发契约 + ERESTART 续走 | P1-1/P1-2 通电后 |
| E-MINTYPES-SYS（增补） | 本轮 Phase 0 | DS 段 (a) 现状更新：SI_DATA_STORE 已上移 minix-types（sysinfo.rs:137），ds 侧切换未做；MIB 无新增常量需求（Endpoint::MIB 足够，grant.rs 修复应改用之） | — |
| E-MINSYS-HYGIENE（增补） | 本轮 Phase 0 | rmib.rs:85/:127 锚点复核仍有效（5 条 clippy 告警仍在） | — |

---

## 6. 对照参考（联网核验 2026-09-15）

- **Redox**：无 sysctl——系统信息走 `sys:` scheme（内核实现的文件接口，"一切皆文件"，Plan 9 影响；[Redox Book](https://doc.redox-os.org/book/)、[redox-os/kernel](https://github.com/redox-os/kernel)）。对照结论：MIB 的远程子树转发（COMMON_MIB_CALL + grant relay）在思想上就是 Redox scheme 转发的 Minix3 形态——请求转发给数据的拥有者；minix-rs 保持 C 外部行为（Rewrite 边界），Redox 的启发只用于内部 handler 归属的理解，不构成改动理由。
- **NetBSD sysctl(9)**：Minix3 MIB 的直系原型（SYSCTL_VERSION/sysctlnode/sysctldesc 即 NetBSD ABI）。NetBSD 用 `sysctl_createv()` 动态建树 + proplib 属性字典扩展（[sysctl(9)](https://man.netbsd.org/sysctl.9)、[proplib(3)](https://man.netbsd.org/proplib.3)）；Minix3 选择了静态表 + 用户态 CTL_CREATE 动态节点。Rust 侧静态表方向（`TOP_SLOTS`/`KERN_ENTRIES` 常量表 + verdict）与 Minix3 取舍一致，**不引入 createv 式动态注册**（无此 C 语义）。
- **Linux**：sysctl(2) 自 2.6 起弃用、接口移往 /proc/sys——教训是"名字数组 + 二进制节点结构"的稳定 ABI 维护成本高。对 minix-rs 的含义：A-4 布局一旦 pin 死就是对外契约（P1-3 用 offset 断言而不是松散序列化的理由），通电后再改即破坏二进制兼容（同 E-VMMCPWIRE 先例逻辑）。
- **Rust 社区**：typed-index arena（slotmap/generational-arena 一族）是图/树结构的成熟形态；`no_std + alloc` 下 `Vec` slab + u32 索引是 Redox 内核（context table 等）与 tokio（slab crate）的通行做法。P2-4 方案 A 即此惯例的服务器特化（加字节预算以兑现 C 的 EINVAL 分配语义）。typestate/枚举替代位标志（`CallAuth` 替代 MIB_FLAG_* 位）是既有 ARCH 方向的延续。

---

## 7. Rule Discovery 与 Gate 证据

### §7.1 Rule Discovery

**发现 ✅**，两个候选（同型 ≥2 例，可提案）：

1. **承诺时点漂移（Promised-Landing Drift）候选**：同一承诺在多篇文档里的兑现时点各自演化且互相矛盾（arena：04:82/08:82 说"13 落地时"、13:112 说"15 统一建"、15:136-138 说"15 之后独立任务"）。与模式 70 CTOS（跨轮状态陈旧）同族但是**文档间**漂移。检查命令：`rg -n "arena.*(13|15)" notes/rewrite/fork-syscall-rewrite/10-stage-mib/0{4,8}-*.md 1{3,5}-*.md`。建议规则：承诺兑现时点必须单一真值（一处声明 + 他处引用），跨文档承诺在接收方文档落地时回写来源方。
2. **消费声称失真（Claimed-Reuse Drift）候选**：文档声称"复用 X 模块"但代码零引用（13-stage-ipc/03 篇三处声称复用 `minix-sys::rmib::MountTable`，`os/servers/ipc-server/src/mib_tree.rs:16` 注释声称、:18 实际 import 无）。与模式 76（跨文档归属漂移）同族但方向相反（声称了不存在的依赖）。检查命令：对文档每处"复用 `X`"grep `use X` 于声称的 crate。

### §7.2 Gate 证据

| Gate | 结果 | 证据 |
|---|---|---|
| Step 0 预检 | ✅ | 4 条 ls：`.design/` 三族快照各 30 份全齐（outline/outline-review/design × v1）；`design-coverage-check.sh fork-syscall-rewrite` 输出无 10-stage-mib 缺失行（仅 14~18 stage 的 00/99 缺失，与本轮无关） |
| Gate A | ✅（带 AI 判定） | `coverage-extract.py mib --c-dir minix3/minix/servers/mib --rust-dir os/servers/mib/src`：63 C 符号 / doc 87.3% / name-match 15.9%；5 个机器缺口逐一判定为 include-guard/MARKER-ARCH-替代/命名演进，真实缺口 0（§1.3） |
| Gate B/C | 不适用 | 本轮 scan-only 无 8 字段行为契约表需求（无修复）；差异提取以 §1.2 三向矩阵承载 |
| Gate D | 不适用（记录理由） | 5 项强制清单针对 doc review 的 §5 测试/trait impl 声称；本轮对象是代码架构审查，等价检查已由 Gate E + §1.2 承载。trait ≥2 impl 规则在本 crate 的映射：crate 无 trait（判定库形态），seam trait 引入时（P1-1）每 trait 必须带 mock + 真实双 impl——已写入 P1-1 验证 |
| Gate E | ✅ | 22 篇 90/90 + 8 篇 31/31 测试名全命中（§1.1） |
| 锚点纪律（模式 83） | ✅ | 本 todo 全部 file:line 经采证 agent grep/Read 实证；关键锚点（main.rs:26、tree/dispatch.rs:6、grant.rs:293-295、com.h:66、hw.rs:80、ds server.rs traits）由主代理 sed/Read 复验 |
| 收敛评估 | 首轮 P1 ×5 非零发现，无漏检自检触发；成本 ~1 session，下一轮（执行轮）按 §8 顺序 | |

---

## 8. 建议推进顺序（执行轮）

1. **P3-1（1~4 项）**——文档账目清账，半小时级，消除 arena 承诺矛盾后再动工。
2. **P2-1**——三处 verdict 补齐（小，独立，先行防 walker 放行错误请求）。
3. **P2-2 + P2-3 + P2-4**——三个形状决策（结果枚举、容器、内存策略），walker 的前置契约。
4. **P1-1**——主循环 + 三 seam（真实端 minix-sys 委托，通电挂 E1；死依赖随本轮转正，P3-2 闭单）。
5. **P1-2**——竞技场 + walker（消费 2/3 步的全部决策；`mib_find`/`judge_level` 生产转正）。
6. **P1-4**——拷贝/授权执行半（依赖 P1-1 的 MibKernel/MibServices）。
7. **P1-5**——表拉取执行半（依赖 P1-3 布局裁决 + E-MIBPROD 对端）。
8. **P1-3**——A-4 裁决与布局锚定（可与 4~6 并行，与 E-ISPROD 裁决协同一次做）。
9. **P3-1 第 5 项**——00/99 两篇成文，plan.md §6.1 两行 pending 清账。

每步完成回写本文件状态列（✅ + 日期 + Fix #N），跨 stage 项按 `../edge_todo.md` 执行约定单线程推进。
