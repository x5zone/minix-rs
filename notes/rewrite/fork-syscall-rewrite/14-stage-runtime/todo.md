# 14-stage-runtime Rust 实现架构级 Review TODO

> 来源：2026-09-16 首轮架构级代码审查（V1 轮）。查漏补缺优先，其次整体/分层架构深审（code-excellence 口径：整体 → crate → 模块 → trait）。
> 范围：`os/libs/minix-rt/src/`（3003 行，59 测试）+ `os/libs/minix-sys/src/` 域内文件（ipc/syscall/pm/vfs/vm/misc/rs/stack/grant/arch_trap + lib.rs）+ `os/libs/minix-types/src/` runtime 域（errno/kerninfo/com + ipc/{message,pm,vfs,vm,rs,kernel_call,notify}）。minix-sys 中归属其它 stage 的六个客户端模块（ds/devman_client/inputdriver/rmib/socket/usb_model，约 3108 行）只做 crate 级一致性扫描，细节归 07/10/11/12/16/17 各 stage 轨道。
> 方法：调用号级全量对账（对照 `minix3/minix/include/minix/callnr.h`、`com.h` 与 plan.md §5.1/§5.2 覆盖契约）+ C 行为级对账（crt0/init/brk/kputc/panic 逐函数）+ Redox 对照（relibc 启动链、redox-syscall crate 组织，来源见 §7）+ 死代码扫描。
> 定位：本文档是架构改进建议与缺口登记清单，**不同于** plan.md（阶段计划）。本轮未修改任何生产代码。
> 状态（2026-09-16，V1 轮）：新发现 **3 项 P0 + 5 项 P1 + 2 项 P2 + 2 项 P3 + 1 项死代码批**，跨 stage 两条挂 edge（E-MINTYPES-RUNTIME 新登记 + E1 增补），另有 1 条 minix-sys 组织问题登记 E-MINSYS-SCOPE。既有登记缺口（09 篇 open 路径、07 篇诊断通道、06 篇 VM 页供给等）维持原归属，见 §3.3 防重复扫描清单。

---

## 0. 审查结论速览

### 0.1 本轮条目速览

| 级别 | 条目 | 一句话 |
|------|------|--------|
| **P0** | **V1-P0-1** | ~~panic 诊断 hook 注册表分裂：kernel 写 `minix_types` 注册表，minix-rt 的 panic handler 读的是自己 crate 内的重复注册表——hook 永不命中~~（**✅ 已修复** 2026-09-16，Fix #1，见 §3.1 修复记录） |
| **P0** | **V1-P0-2** | ~~10 篇对 plan.md §5.1 契约漏 VM 客户端库四文件族，Rust 零实现且无登记~~（**✅ 已修复** 2026-09-16，Fix #7/#8/#9：§2.5 清单 + 七个 wrapper；vm_info 暂缓已论证，见 §3.1 修复记录） |
| **P0** | **V1-P0-3** | ~~`STACK_MINIMUM_BYTES=372` 与 C `STACK_MIN_SZ`（约 1364 字节级）矛盾且整段死代码——生产的栈布局在 minix-sys（`STACK_MIN_SZ=1400`）~~（**✅ 已修复** 2026-09-16，Fix #3，见 §3.1 修复记录） |
| P1 | V1-P1-1 | 🔄 诞生链整体缺口（**第一步已完成** 2026-09-16，Fix #4：start.rs 整模块删除 + 02/03 篇同步；**第二步挂 E1 切片 5 通电**）：真实 `_start` 只做分配器初始化→main→exit，argv/environ/progname/ps_strings/fini_array/IPC 向量安装在真机路径上都不发生（§3.2） |
| P1 | V1-P1-2 | ~~诊断双轨：DiagBuffer/PanicStage 模型层与 lib.rs 内联 BufferWriter 生产层互不相连，`PanicPlan` 是不存在的类型名~~（**✅ 已修复** 2026-09-16，Fix #2，见 §3.2 修复记录） |
| P1 | V1-P1-3 | ~~misc 发送半缺口：nanosleep 的 select 组装归 09 篇但 09 篇未声明，svrctl 只有分派没有发送~~（**✅ 已修复** 2026-09-16，Fix #10，见 §3.2 修复记录） |
| P1 | V1-P1-4 | kerninfo MAGIC 失配行为分歧：C 静默降级继续运行，Rust 返回 ENOEXEC 拒绝——`initialize_runtime` 接线前必须裁决（§3.2） |
| P1 | V1-P1-5 | 99 篇（全局概念/布局权威）pending 是多个已登记缺口的共同解锁前置（09 open 路径布局、VM_REMAP 布局对齐都在等它）（§3.2） |
| P2 | V1-P2-1 | ~~查询族死 `None` 签名：四个 wrapper 的 `Option` 永远是 `Some`，docstring 承诺的失败语义实际走 `Err`~~（**✅ 已修复** 2026-09-16，Fix #6，见 §3.2 修复记录） |
| P2 | V1-P2-2 | wire 打包双体系：minix-types 的类型化布局与 minix-sys 的本地裸字节打包并存，`cleared_message`/`write_payload` 在三个文件各复制一份；VM_REMAP 两处状态标注不一致（§3.2，跨层部分挂 edge E-MINTYPES-RUNTIME） |
| P2 | V1-P2-3 | ~~注释锚点失实批：10 处~~（**✅ 全部处置** 2026-09-16，Fix #1/#2/#4/#12，见 §3.2 修复记录） |
| P3 | V1-P3-1 | 测试浅化批：大块分配测试未验证页边界、panic 阶梯只验长度不验中间序（§3.2） |
| P3 | V1-P3-2 | GlobalAllocator 手写简化版 Once + 注释中英混用（§3.2） |
| edge | E-MINTYPES-RUNTIME（新） | minix-types 布局单点权威收敛（Redox syscalls.toml 先例），99 篇定稿驱动 |
| edge | E-MINSYS-SCOPE（新） | minix-sys 内六个域外 stage 客户端模块的 crate 内聚性处置 |
| edge | E1 增补 | 通电族增加"首个 no_std minix-rt 二进制"验证面 |

验证命令（2026-09-16 两时点实测；本轮未改任何生产代码，差异全部来自并行线程当日提交——E7 批次与 E-BOOTFRAME 的 minix-sys/minix-types 落地）：
- `cargo test -p minix-rt -p minix-sys -p minix-types`：扫描起点 **59 / 161 / 246 passed, 0 failed**；收尾复测 **59 / 167 / 248 passed, 0 failed**（minix-sys +6、minix-types +2 为并行线程新增测试），两时点全绿
- `cargo clippy -p minix-rt -p minix-sys -p minix-types --lib`：扫描起点 minix-rt/minix-sys 零警告、minix-types 1 条（large size difference between variants，`VmReply` 枚举臂尺寸差，含行为权衡非纯清理项）；收尾复测 minix-sys 新增 4 条 doc 注释警告（empty line after doc comment + doc list item without indentation ×3，来自并行线程当日提交），归该工作线清理，非本轮引入

---

## 1. Step 0 预检与 Gate 状态

- **Step 0（模式 69）**：`tools/design-coverage-check.sh fork-syscall-rewrite --stage 14-stage-runtime` → 01~13 全 PASS；00/99 缺 `.design/` 快照（两篇正文同为 pending 骨架）→ CRITICAL 按 Step 0.3 规则登记不阻断（本轮以代码为主；00/99 改写列为 V1-P1-5）。
- **Gate A（覆盖穷举）**：`tools/coverage-extract/` 现无 runtime-semantic-map.json（现有 12 份 map 无 runtime）。本轮以 plan.md §5.1/§5.2 函数级契约为底册人工对账完成（§2 矩阵）。**建议后续动作**：补建 `tools/coverage-extract/runtime-semantic-map.json`（C 符号 → minix-rt/minix-sys 符号映射），使 runtime 可进入标准 Gate A 工具链。
- **Gate D 抽查（5 项 grep 清单）**：文档引用的测试 fn 全部 grep 命中（08 篇 13 个、09 篇 15 个、10 篇 13 个、11 篇 12 个、12 篇 9 个，与文档自称计数一致）；核心算法非 stub（transport 双形态：Canned 回放 + DirectTrap 真体收进 `real-trap` feature，`os/libs/minix-sys/src/arch_trap.rs:61-107`）。
- **Gate E（测试名对账）**：文档 §5 测试名均为中文描述标签而非 Rust 标识符，逐一对到代码后计数吻合（见 §4）。两处口径差已在条目正文标注（10 篇"七个封装"对 8 个测试面、12 篇"三个封装"对 4 个测试面——实际代码 fn 数裁决）。
- **测试卫生**：三 crate 474 个测试，`#[ignore]`/`assert!(true)`/`todo!`/`unimplemented!`/`FIXME`/`TODO` grep 全部零命中；无空函数体测试。

---

## 2. 覆盖对账矩阵（查漏主体）

> 判定基准三层：C 真值（minix3 头文件与 libsys/libc 源码）> plan.md §5.1/§5.2 契约 > Rust 实现。缺口分三态：**已登记**（文档或 edge 有归属，不重复立项）、**未登记缺口**（本轮立项）、**有意排除**（plan.md §5.3 排除表）。

### 2.1 PM 组（C `callnr.h:14-60` 共 47 个调用）

Rust 实现 8 个调用号（`os/libs/minix-sys/src/pm.rs:40-68`：EXIT/FORK/WAIT4/GETPID/KILL/EXEC/SRV_FORK/SRV_KILL）+ raise（kill 组合，`pm.rs:236`）= 9 个 wrapper。08 篇显式声明子集策略（"其余同形调用……清单见阶段计划，逐个铺开占用篇幅"，`08-pm-syscalls.md:60`）。与 plan.md §5.2-08"PM 调用全清单"契约存在**契约-文档-实现三角差**：plan 声明全清单，文档声明机制代表 + 清单收录，实现是子集。信号族（PM_BASE+20~24）与凭证/时间/itimer 各批的 **wire 半已于本轮扫描当日（2026-09-16）由 edge E7 落入 minix-types**（`MessLcPmSig`/`MessLcPmSigset`/`MessLcPmTime`/`MessLcPmSetid` 等布局 + 绝对值 pin，见 edge_todo.md E7 各进度块），minix-sys wrapper 半与消费接线归 04-stage-pm 批次表（04-stage-pm/todo.md §11.1.1）——已登记，不重复立项。仍开口的部分：ARCH A-9 声明"08 篇建模信号语义"但 08 篇只有 kill/raise，信号递送/掩码的**运行时语义面**（mcontext PM_BASE+18/19 亦未见落批次 [待验证]）与 08 篇文档覆盖仍缺；uid/gid/time/itimer/reboot 等同形面维持 08 篇收录策略。

### 2.2 VFS 组（C `callnr.h:72-135` 共 64 个调用）

Rust 实现 6 个调用（read/write/close/lseek/open + 散布校验 `validate_scatter_gather`，`os/libs/minix-sys/src/vfs.rs`）。已登记：open 路径布局路 ENOSYS（64 位下 40 字节内联缓冲放不进 56 字节载荷，待 99 篇裁决，`09-vfs-syscalls.md:72/:123`）。未登记缺口：**stat/fstat/lstat、ioctl、fcntl、getdents 等命令层硬依赖族零 wrapper 且无排期登记**（09 篇仅"同形清单见阶段计划"）；dup 组合面文档分析了 C 但 Rust 无对应且无"有意不做"声明（对照 12 篇窄 helper 的显式不做声明——同样决策未写）。select 组装缺失连带 nanosleep（见 V1-P1-3）。

### 2.3 VM 组（C `com.h:627-780` 共 49 个调用 + libsys 客户端库 11 文件）

Rust 实现 9 个 wrapper（`os/libs/minix-sys/src/vm.rs:171-399`：mmap/munmap/break/fork/exit/remap/getphys/getref/map_phys）。**未登记缺口（V1-P0-2）**：plan.md §5.1 将 `vm_info.c`/`vm_procctl.c`/`vm_cache.c`/`vm_getrusage.c` 四文件映射到 10 篇，10 篇对四者 grep 零提及；minix-types 已有 wire 半（`MessLsysVmInfo` `minix-types/src/ipc/vm.rs:1673`、`MessLsysVmRusage` :1693、`VmProcctlIn` :512、`VM_GETRUSAGE` 常量 :132），minix-sys 有常量无 wrapper（`VM_CALL_INFO`/`VM_CALL_PROCESS_CONTROL`/`VM_CALL_WILL_EXIT`/`VM_CALL_UNMAP_PHYS`，`vm.rs:49-71`），cache 族（VM_MAPCACHEPAGE+26/27/28/29）连常量都没有。有意排除：vm_set_priv/vm_update/vm_memctl/vm_prepare 归集成 stage（`10-vm-syscalls.md:48` 声明）。

### 2.4 misc 与 RS

misc：nanosleep/svrctl 只有纯函数判定半，发送半缺失（V1-P1-3）；kerninfo 直读四函数（tick/uptime 三元组/挂钟/tsc 拼接）完整且带 C 公式逐条对应（`os/libs/minix-sys/src/misc.rs:168-241`）；sysctl 只有内联分界验证，发送半归 E-RMIBWIRE/10-stage-mib 轨道（`misc.rs:19-21` 边界声明）。RS：lookup/getepinfo/procnr/getsysinfo 四面齐（`rs.rs:113-264`），窄 helper 有意不做已登记（`12-rs-query.md:60`）。

### 2.5 csu/crt0 与 libsys 共享子集

csu 链缺口整体见 V1-P1-1（13 项清单：ps_strings/environ/progname 全局、atexit/fini_array、`_libc_init`、argv 入 main、入口寄存器、IPC 向量整表安装等）。libsys 共享子集对账：kernel_call ✓（`syscall.rs` 三协议精确）；asynsend → SENDA 队列 ✓（`ipc.rs`，`&mut self` 替代 C `inside` 标志是合理的 Rust 化）；kputc/diagctl 缓冲与操作码 ✓ 发射 ✗（V1-P1-2）；panic/assert 阶梯形状 ✓ 执行器 ✗（同上）；getticks/getuptime/clock_time ✓；getsysinfo/getepinfo/getprocnr ✓；srv_fork/srv_kill ✓；vm_* 客户端库 3/7 文件（见 2.3）。

### 2.6 常量 ABI

errno：115 = 115 与 C `sys/sys/errno.h` 一比一对齐（`minix-types/src/types/errno.rs`，含对账测试 `test_errno_values_match_c` :551）。调用号：六个 `test_call_numbers_match_*` pin 测试家族覆盖 PM/VFS/VM/RS/kernel-call。termios/signal 大族按需移植策略已在 13 篇登记。布局 pin 纪律集中且扎实：minix-types 约 313 条 size/offset 断言（20 文件 42 测试）；minix-sys 与 minix-rt 合计仅 8 条——**建议** minix-sys 的本地 `#[repr(C)]` payload（`MapPayload`/`ExecPayload`/`ServiceForkPayload` 等）补齐 56 字节与字段偏移断言（`vm.rs:463` 的 `test_map_payload_matches_c_field_order` 是好样板，但只覆盖 VM 一族）。

---

## 3. 条目正文

### 3.1 P0（正确性与契约级）

#### V1-P0-1（P0-code-bug）panic 诊断 hook 注册表分裂——kernel 注册的 hook，minix-rt 的 panic handler 永远读不到——✅ 已修复 2026-09-16（Fix #1）

**Rust 现状**：minix-rt 在 `os/libs/minix-rt/src/lib.rs:381-382` 定义了自己的 `PANIC_DIAGNOSTIC_HOOK` 静态元胞，配套 `set_panic_diagnostic_hook`/`panic_diagnostic_hook`/`run_panic_diagnostic_hook`（lib.rs:389-412）。同一套东西在 `os/libs/minix-types/src/types/diagnostic.rs:26-51` 已有权威版本（连文档注释都同文："minix_kernel::register_panic_diagnostic"）。kernel 侧注册写的是 **minix_types** 的注册表：`os/kernel/src/lib.rs:1638` `minix_types::set_panic_diagnostic_hook(...)`，启动路径 :2009 调用。而 minix-rt 的 `#[panic_handler]` 检查的是**自己 crate 内**的注册表：`os/libs/minix-rt/src/lib.rs:333` `if !run_panic_diagnostic_hook(message)`。kernel 不依赖 minix-rt（`os/kernel/Cargo.toml` grep 零命中），lib.rs:330-331 注释声称的依赖方向"kernel → minix-rt"与现实相反（真实方向：kernel → minix-types ← minix-rt）。

**后果与可达性**：真实 no_std 二进制（启用 `panic-handler` feature）panic 时，lib.rs:333 的检查必然为 false（minix-rt 自己的注册表无人写入），一切 panic 落入 `SpinSink` 在 emit 内自旋（`os/libs/minix-rt/src/diag.rs:248-252`）——内核辛苦注册的 C 风格 panic 渲染（`kernel_panic_diagnostic`，kernel/src/lib.rs:1613）永不生效。当前不可达仅因仓内无二进制启用该 feature（grep `panic-handler` 于 os/commands 与 os/servers 的 Cargo.toml 零命中）——**通电即踩**，归 E1 切片 5 的验证面。

**修改方案**：
- 方案 A（选定建议）：删除 minix-rt 内的重复注册表（lib.rs:376-412 中 `PanicDiagnosticHook` 类型、静态元胞与三个函数），panic handler 改读 `minix_types::run_panic_diagnostic_hook`（minix-rt 已依赖 minix-types，零新边）；同轮修 lib.rs:330-331 依赖方向注释。
- 方案 B（否决）：kernel 改注册到 minix-rt。否决理由：制造 kernel → minix-rt 反向依赖，与"运行时库被内核复用"的分层方向相反；且 minix-types 的注册表已有 kernel 测试锚定（kernel/src/lib.rs:2987-2994）。

**验证**：宿主测试断言 `minix_rt::run_panic_diagnostic_hook` 与 `minix_types::panic_diagnostic_hook` 联动（注册后 minix-rt 侧可见）；`cargo test -p minix-rt` 全绿。
**边界**：勿动 kernel/src/lib.rs:1638 的注册点（它是对的）；与 V1-P2-3 的注释修正同函数但不重叠。

**修复记录（Fix #1，2026-09-16）**：按方案 A 落地——
- **方案对比**：A 删除 minix-rt 本地注册表、panic handler 一行委托 `minix_types::run_panic_diagnostic_hook`（选定；共享契约放公共叶子层，Linux panic_notifier_list 单注册点同构、Redox relibc→syscall crate 单向依赖同构）；B kernel 反向依赖 minix-rt（否决，分层倒置）；C 双表加同步步（否决，双真相源必再漂移）。
- **Files**：`os/libs/minix-rt/src/lib.rs`（panic handler stage-2 注释重写依赖方向描述 + 调用改 `minix_types::run_panic_diagnostic_hook`；删除 `PanicDiagnosticHook` 类型别名、本地静态元胞、set/get/run 三函数，约 40 行；原 `hook_set_run_clear_round_trip` 测试改为消费侧契约 pin `panic_hook_contract_through_shared_registry`，锁"共享 API 注册对共享 run 可见"这一 panic 路径依赖的语义；无钩子回退路径按其自旋契约不做进程内测试，注释说明）。
- **测试**：59 passed / 0 failed（计数不变：一删一换）；clippy minix-rt 本体零警告（工作区可见 7 条警告全部属于依赖 crate：minix-types 1 条既有 + minix-sys 4 条 doc 警告为并行线程 E7 当日落地，pm.rs:368-371，归该工作线）。
- **Verified**：`grep -rn "PANIC_DIAGNOSTIC_HOOK\|minix_rt::set_panic_diagnostic" os/libs/minix-rt/ os/commands/ os/servers/` 零命中；kernel 注册点（kernel/src/lib.rs:1638）未动。
- **Docs**：07 篇从未记载 hook 注册表（D-48 晚于其定稿），无需同步；lib.rs 代码注释已在本次重写。

#### V1-P0-2（P0-design-missing）10 篇对 plan.md §5.1 契约漏 VM 客户端库四文件族——✅ 已修复 2026-09-16（Fix #7/#8/#9：文档清单 + 七个 wrapper 落地；vm_info 暂缓已论证）

**契约**：plan.md §5.1（plan.md:229）将 `vm_info.c`、`vm_procctl.c`、`vm_cache.c`、`vm_getrusage.c` 明确映射到 10 篇（"VM 客户端库·用户态 ABI 子集"）。**文档现状**：`notes/rewrite/fork-syscall-rewrite/14-stage-runtime/10-vm-syscalls.md` 对 cache/info/procctl/rusage/willexit/unmap_phys 的 grep 为**零命中**（2026-09-16 实测）——四文件族连同 `VM_VFS_MMAP`、`VM_WILLEXIT` 均未进入文档清单，也无"有意排除"声明。**实现现状**：wire 半在 minix-types 已备（`VmProcctlIn` `minix-types/src/ipc/vm.rs:512`、`MessLsysVmInfo` :1673、`MessLsysVmRusage` :1693、`VM_GETRUSAGE` :132——VM 服务器分发端已在消费部分结构）；minix-sys 侧有常量无 wrapper（`vm.rs:49/:55/:67/:71`），cache 族（`com.h:682-691` 的 +26/27/28/29）连常量都无。

**后果**：消费方（FS 缓存客户端、getrusage 命令面、RS 的 vm_info 查询）无承载；且"文档已收敛"（CONVERGED）与"契约未覆盖"并存，后续按文档排期会永远跳过这四族。

**修改方案**：
- 方案 A（选定建议）：10 篇补一节"客户端库余量清单"（同 08 篇"清单收录"体例），逐文件列 C 函数与 C 锚点，标注归属（cache 族→FS 联调前置、rusage→命令面前置）；minix-sys 补 wrapper 的排期写入该节，优先级随消费方 stage 推进。
- 方案 B（否决）：plan.md 改契约把四文件族移出 10 篇。否决理由：无正当语义归属变更——它们就是用户态 VM 客户端库，移走是改契约掩盖缺口。

**验证**：10 篇补节后 `grep -c "vm_cache\|vm_getrusage\|vm_info\|vm_procctl" 10-vm-syscalls.md` ≥ 4；wrapper 落地时各带 CannedTransport 回放测试（E2 先例）。
**边界**：cache 族与 E-IPCWIRE 第 5 项（VM_CALL_SHARED_UNMAP wrapper）同在 vm.rs 落地，执行时同批勿分两次打开。

**修复记录（Fix #7，2026-09-16）**：文档半落地——
- **Files**：10 篇新增 §2.5"客户端库余量清单（阶段计划契约对账）"：七行清单表（vm_info 三函数 / vm_procctl 两公开 / vm_cache 四公开 / vm_getrusage / vm_willexit / vm_unmap_phys / minix_vfs_mmap），每行带 C 定义行锚点、调用号偏移、消费方与排期归属；三层现状盘点（wire 半备好的 info/procctl/rusage 三族、连常量都无的 cache 族与 willexit、封装半 9/N）；排除项重申（三件归集成、SHM_UNMAP 归 E-IPCWIRE、REMAP_RO 待 E-MINTYPES-RUNTIME）。
- **验证**：`grep -c` 实测 5 行命中（判定门槛 ≥ 4，达标；初记 18 为误写，特此勘误）；全部 C 锚点逐文件 grep 实证（vm_info.c:10/:24/:39、vm_procctl.c:10/:28/:33、vm_cache.c:15/:47/:59/:68/:77、vm_getrusage.c:7、vm_exit.c:25、vm_map_phys.c:33、mmap.c:49）。
- **边界**：wrapper 半（Fix #8/#9）落地后回填本条；10 篇其余章节不动。

**修复记录（Fix #8，2026-09-16）**：wrapper 半第一批评审落地——
- **范围裁决**：原计划"四 wrapper"中的 vm_info 三函数**暂缓**（DEFERRED，非充数，论证如下）：其 C 语义是"服务器经用户指针往调用者缓冲写结构"（`vm_info.c` 的 `ptr`/`next` 栏），需要缓冲语义、`VMIW_*` 常量与区域枚举迭代的设计定稿，且当前树内零消费方——先写就是 start.rs 式的投机设计（该模块正是因无消费方而漂移、被 Fix #4 删除）。已在 10 篇 §2.5 如实登记，消费方（top 类工具/RS）出现时优先落地。
- **设计**：三个新封装消费 minix-types 既有 union 臂的钉死布局（`m_lc_vm_willexit` 的 m1_i1 栏、`m_lsys_vm_unmap_phys` 的 i386 形两栏、`m_lc_vm_procctl` 的 mess_9 五栏 @16/20/24/28/32），与 VM 服务器解码侧（`VmWillexitIn`/`VmUnmapPhysIn`/`MessLcVmProcctl` 的文档锚点）互为镜像。`unmap_physical_via` 不携带 C 版长度参数——该参数在线上是死栏（wire 无长度栏，服务器按地址查区），沿用 stack.rs 删除死参数的先例并在 docstring 论证；C 版的本地特殊内存登记清除归服务端集成（10 篇既有边界）。
- **Files**：`os/libs/minix-sys/src/vm.rs`（+`will_exit_via`/`unmap_physical_via`/`process_control_clear_via`/`process_control_handlemem_via`/私有核心 `process_control_via`/两常量 `PROCESS_CONTROL_PARAM_CLEAR=1`/`HANDLE_MEM=2` pin com.h:759-760）；`os/libs/minix-sys/src/ipc.rs`（CannedTransport 增 `sent` 出站日志——补齐与 CannedKernelCallTransport 对等的出站断言能力，纯增量，构造函数全走 `new()` 无字面量构造方）。
- **测试**：新增 5 个（willexit 首整数栏、unmap 物理的两栏含 32 位截断语义、procctl 清除的五栏形状、handlemem 五栏全值、失败传播）；minix-sys 168 → 173 passed；clippy 零新增（期间引入过一次未用导入，已当场修正）。
- **Docs**：10 篇 §2.5 三层现状段刷新（三个封装落地 + vm_info 暂缓论证 + C 版长度死栏说明）。
- **边界**：cache 族（Fix #9）落地时需新调用号常量与 wire 形状核对；E-IPCWIRE 第 5 项（SHM_UNMAP）不受影响。

**修复记录（Fix #9，2026-09-16）**：cache 族四封装落地——
- **wire 权威**：`m_vmmcp` 64 位加宽布局（E-VMMCPWIRE 结算）以 VM 服务器解码侧为镜像权威（`minix-types::VmCacheIn::decode_message` 文档：dev@0/dev_offset@8/ino_offset@16/ino@24/block@32/flags_ptr@40/pages@48/flags@49）；映射的回复地址从 `m_vmmcp_reply` 栏读回（addr@0）。
- **C 前置条件的处置**：C 核心的三个对齐恐慌与 `NO_DEV` 断言（vm_cache.c:17-31）是调用方缺陷而非运行时条件，Rust 侧同形 panic（`#[should_panic]` 测试锁定），不发明错误通道；`vm_clear_cache` 无对齐检查，同 C。
- **Files**：`os/libs/minix-sys/src/vm.rs`（+`map_cacheblock_via`/`set_cacheblock_via`/`forget_cacheblock_via`/`clear_cache_via`/私有核心 `cache_call_via`（回复消息返回，映射读地址）/四调用号常量 pin com.h:682-691/`CACHE_PAGE_SIZE`/`NO_DEVICE`）。
- **测试**：新增 6 个（映射回复地址 + 出站八栏、识别的 block 与 setflags 栏、遗忘的零 inode 栏与页数换算、清空单设备栏、对齐恐慌、无设备恐慌）；minix-sys 173 → 179 passed；clippy 零新增（cache_call_via 十参与 C 核心同形，注释后 allow）。
- **Docs**：10 篇 §2.5 三层现状段二刷（缓存四件落地 + wire 权威来源）。
- **边界**：`flags_ptr` 栏当前服务器不回写——封装保留 `&mut u32` 的 C 形状并如实注释，回写随服务器 setcache 标志处理落地；VM 服务器 cache 处理器属 02-stage 轨道，未动。

#### V1-P0-3（P0-fact）`STACK_MINIMUM_BYTES=372` 与 C `STACK_MIN_SZ` 矛盾，且整段是零消费方的平行实现——✅ 已修复 2026-09-16（Fix #3）

**Rust 现状**：`os/libs/minix-rt/src/handoff.rs:291-292` 定义 `STACK_MINIMUM_BYTES: usize = 372`（自称 conservative，handoff.rs:287-288），`compute_stack_size`（handoff.rs:302-330）以它为下界。**生产事实**：栈布局的生产实现是 `os/libs/minix-sys/src/stack.rs`（`STACK_MIN_SZ = 1400` LP64，stack.rs:56-60；消费方 `os/servers/vm/src/vm_server.rs:786/:791` boot exec 路径）——该实现是本轮扫描当日（2026-09-16，commit f459852ea）刚在 edge E-BOOTFRAME 落地并完成 STACK_MIN_SZ=1400 预算裁决的，权威性进一步增强。**C 真值**：`minix3/minix/lib/libc/sys/stack_utils.c:66-71` 的 `STACK_MIN_SZ` 至少含 `PMEF_AUXVECTORS=20`（com.h:356）× 16 字节 AuxInfo + `PMEF_EXECNAMELEN1=PATH_MAX=1024`（syslimits.h:64）的 exec 名 + ps_strings 24 字节——1364 字节级。372 连 auxv 表都放不下，"conservative"断言与事实相反。

**为什么是死代码**：`compute_stack_size`/`STACK_MINIMUM_BYTES` 在 handoff.rs 之外 grep 零命中（2026-09-16 实测，仅测试自引用 :499）。handoff.rs:254-280 注释自认是"刻意的平行独立实现"（纯函数化），但平行实现的价值前提——两处各有消费者——不成立。

**修改方案**：
- 方案 A（选定建议）：删除 handoff.rs 的 `compute_stack_size` 与 `STACK_MINIMUM_BYTES` 及其测试（:499 区段），handoff 职责收缩为 kerninfo/kuserinfo 解码（01 篇主语义）；栈尺寸语义单点归 minix-sys/stack.rs。
- 方案 B（否决）：对齐常量为 1400 保留双实现。否决理由：两个 STACK_MIN 常量分属两个 crate 必然再漂移（本次 372 vs 1400 的分叉就是证据），且 VM 侧消费的是 minix-sys 版。
**验证**：删除后 `cargo test -p minix-rt` 通过（59 → 58）；`grep -rn STACK_MINIMUM_BYTES os/` 零命中。
**边界**：handoff.rs 其余（ValidatedKernInfo/字段探测）不受影响；01 篇文档若引用了该函数需同轮同步（执行时 grep 01-kernel-handoff.md 确认）。

**修复记录（Fix #3，2026-09-16）**：按方案 A 落地——
- **Files**：`os/libs/minix-rt/src/handoff.rs` 删除 `StackSizePlan` 结构体、`STACK_MINIMUM_BYTES` 常量、`compute_stack_size` 函数及其两条测试（约 100 行）；模块文档第 27 行区段改写为"栈镜像构建单点归 minix-sys 的 stack 模块，本模块只选初始栈顶"。
- **Docs**：01 篇同步两处——§3.4 整节改写（原节讲的是被删函数的溢出标志设计；改写为"填充期边界检查代替回绕标志"，如实描述 minix-sys/stack.rs 的实际机制：`StackFillError::FrameTooSmall`/`SizeMismatch` 在填充期拦截 C 溢出标志家族的失败，锚点 stack.rs:86-96；不凭记忆断言其使用 checked-add）；§5 测试表 11 → 9（删两行），统计刷新（58 = 9 + 02 篇 11 + 03 篇 9 + 06 篇 14 + 入口 4 + 07 篇 14，日期 2026-09-16），补一句"尺寸与填充测试随实现住 stack.rs"。
- **测试**：minix-rt 58 passed（60 → 58）；clippy 零新增；`grep -rn "STACK_MINIMUM_BYTES\|compute_stack_size\|StackSizePlan" os/` 零命中。
- **边界**：删前已核实 minix-sys/stack.rs 的溢出处理方式（`StackFillError` 家族），01 篇新 §3.4 的每个断言都对照该文件原文；未动 02 篇（其测试清单不含栈尺寸项）。

### 3.2 P1/P2/P3

#### V1-P1-1（P1-design-missing）诞生链整体缺口：`_start` 真实路径只走三步，start.rs/init.rs 模块悬空

**现状锚点**：真实入口 `os/libs/minix-rt/src/lib.rs:112-128` 只做 `init()`（=分配器，lib.rs:89-91）→ `main()` → `minix_sys::exit`。start.rs 整个模块（`StartupStage` :277-310、`EntryDescriptor` :67-133、`short_program_name` :148-161、`run_function_array` :206-219、`RunOnce` :235-260）与 init.rs 的 `initialize_runtime`（init.rs:225-275）在 workspace 内 grep 零生产调用方（唯一同名命中是 lwip 无关的 `StartupStage`，`os/net/lwip/src/startup.rs:20`）。对照 C `___start`（`minix3/lib/csu/common/crt0-common.c:144-192`：ps_strings 发布→environ→progname→`_libc_init`→preinit→`atexit(_fini)`→main→exit）与 relibc 的对应物（crt0 汇编 → `__libc_init` 做 TLS/auxv → init_array → main，见 §7 relibc 链接）——**argv/envp 根本送不进 main（Rust main 无参，lib.rs:118-119），入口寄存器（crt0.S:46-48 的 rdx/rcx/rbx）被丢弃**。

**注意范围**：minix-rt 的 no_std 链当前无任何二进制编译（35 个命令消费方全部默认 `std` feature），所以这是"休眠基建的完整性"问题而非运行中故障——但 00 篇主线（"进程运行时生命周期"）的第一段在 Rust 侧实际不存在。

**修改方案**：
- 方案 A（选定建议，两步走）：第一步收缩——删除 start.rs 死代码族与 `RunOnce`/`StartupStage`（见 §4 死代码清单），把"还活着的设计意图"（descriptor 检查、progname 推导）收进 init.rs 或留文档；第二步接通——E1 切片 5 通电时一次性落成完整 no_std `_start`（入口寄存器→ps_strings→init→argv main→exit），以 01/02/03 篇为规范，接线时同步裁决 V1-P1-4 的 MAGIC 分歧。
- 方案 B（否决）：现在就补全 `_start` 全链。否决理由：无真机验证面（real-trap/panic-handler 同样休眠），写了也只能靠 mock 自证，违背"通电即验证"的既有纪律（E1 切片 3+4 先例：机制面先备好，通电统一挂 boot 链）。

**验证**：第一步以 grep 证明死代码清除；第二步归 E1 切片 5 验收（首个 no_std 二进制真机跑通 argv 传递）。
**边界**：与 V1-P0-3（同文件删除）、V1-P1-4（init.rs 分歧裁决）联动执行；A-4 TLS 在方案 A 中显式登记为"接线时决定是否引入"（relibc 的 errno 依赖 TLS，本仓 A-5 Result 语义已消除该需求，登记为 ARCH 差异而非遗漏）。

**修复记录（Fix #4，2026-09-16）**：第一步按方案 A 落地（比原计划更彻底——整模块删除而非逐类型删除）——
- **裁决**：删除前核查确认 start.rs 全部公开项（含 `UNINITIALIZED_ENVIRON_SENTINEL`/`EntryDescriptor`/`short_program_name`/`environ_is_uninitialized`/`ArrayDirection`/`run_function_array`/`RunOnce`/`StartupStage`）在生产路径零调用（35 个命令的 `use minix_rt::*` glob 下无一名命中；init 命令的 `EntryError` 是它自己的同名类型）。投机性纯函数已两次暴露与现实脱节（372 栈预算、"环境哨兵检测"叙述在 C 源无对应检测点），保留价值低于接线时按真实寄存器 ABI 重写的价值——与 E-BOOTFRAME 成功先例同构（对着 C 真值与见证测试落地，而非对着投机抽象补线）。
- **Files**：删除 `os/libs/minix-rt/src/start.rs`（444 行，13 个测试随模块消失）；`lib.rs` 删 `pub mod start` 声明、crate 文档模块清单改四模块、"Relation to Redox" 段重写（原 "Redox 的 linker crate" 为虚构引用，relibc 才是 Redox 用户态运行时——顺带完成 V1-P2-3 第 4 项）。
- **Docs**：02 篇三节改写——§3 重写为"Rust 现状与接线时的设计准则"（如实记载删除史实与理由，保留五条已验证设计准则供接线时对照）；§4 从错误表改为"失败去处"叙述（EntryError 类型已不存在）；§5 改为入口路径现状说明 + 接线时的测试重建骨架；§7 参见更新并修正"7 行指令"为"6 条指令"（顺带完成 V1-P2-3 第 5 项）。03 篇 §7 参见的 start.rs 行改指 lib.rs。§2.4 末尾删除引用已删枚举的句子。
- **测试**：minix-rt 60 → 47 passed（start.rs 实际 13 个测试随模块删除；02 篇原记载 11 个，又一处既有漂移，已在篇内如实记载）；clippy minix-rt 零警告；no_std+panic-handler 形态编译通过；`grep start.rs` 在 minix-rt 与四篇文档中零残留。
- **边界**：`initialize_runtime`（init.rs）保留至第二步接线时收编（死代码清单 #2 理由不变）；`ProcessStrings`（handoff.rs）保留（01 篇域模型，ps_strings 构造校验有测试锚定）；A-4 TLS 登记为接线时裁决项。
- **溯源备注**：start.rs 文件删除的入库时间线——本线程 `git rm` 的暂存被并行线程的提交 f19ad8cfb（E1 切片 5 通电载体）一并卷入；本线程的 2d4a9e603 承载文档改写与 lib.rs 变更。内容无损，特此记录。

#### V1-P1-2（P1-design-deviation）诊断双轨：模型层与生产 panic 路径互不相连，`PanicPlan` 是幻影类型名——✅ 已修复 2026-09-16（Fix #2）

**现状锚点**：diag.rs 提供了完整模型——`DiagBuffer`（kputc 缓冲语义逐条对应 C，`diag.rs:121-130` ≙ `kputc.c:22`）、`format_panic_identity`、`PanicStage` 八阶（diag.rs:316-348，对齐 `panic.c:34-66`）、`DiagnosticSink`。但生产 panic handler（`lib.rs:287-342`）自带 256 字节 `BufferWriter` 格式化，与 diag.rs 的任何机制零调用关系；panic 末段只有自旋（lib.rs:339-341），C 的 "no message" 分支、尾换行、stacktrace 标记、abort 阶梯（panic.c:43-66）无执行器。`os/libs/minix-rt/src/diag.rs:15` 引用的 `PanicPlan` 类型全仓不存在（实际类型叫 `PanicStage`）。另有已自认的附加行为 `write_and_flush`（diag.rs:133-137，C 无此步）。

**修改方案**：
- 方案 A（选定建议）：以 A-8 阶段 2（内核诊断通道接线）为收敛点做单一 panic 路径——panic handler 改用 `DiagBuffer` 承载格式化输出（替换 lib.rs 内联 BufferWriter），`PanicStage` 落成真执行器或删枚举留注释；`PanicPlan` 引用即改 `PanicStage`。对照 C：kputc 缓冲本来就是 panic 输出的承载，Rust 侧没有理由双轨。
- 方案 B（否决）：维持双轨，文档标注"模型层仅供教学"。否决理由：双轨即漂移温床（本次 `PanicPlan` 幻影就是第一例），且 07 篇描述的正是单一阶梯。
**验证**：`grep -n PanicPlan os/libs/minix-rt/` 零命中；panic handler 与 DiagBuffer 合流后 `cargo test -p minix-rt` 全绿。
**边界**：与 V1-P0-1 同在 panic 路径，建议同轮执行（hook 修复 + 单轨化一次完成，避免两次打开 lib.rs 尾部）。

**修复记录（Fix #2，2026-09-16）**：按方案 A 变体落地（比原条目更进一步）——
- **方案对比**：A 单一格式化之家——diag.rs 新增 `format_panic_report`（位置前缀+消息+换行 → 调用者缓冲），panic handler 收缩为"调格式化 → 交发送（hook 或 SpinSink）→ 打转"三步；PanicStage 无执行器枚举删除，梯子知识归模块文档（含完整 C 阶梯引用与 A-8 步骤归属）与 panic handler 阶段注释，待 A-8 步骤 2/3 有真执行器时再立枚举（YAGNI）。原条目文字"改用 DiagBuffer 承载"修正为"format_panic_report 承载"——深想后 DiagBuffer 是 2000 字节 kputc 缓冲，panic 栈上放它不如 256 字节专用缓冲，且截断语义不同（写刷尾便利 vs 静默截断），强合流反而是 translate。B 为八阶梯写占位执行器（否决，四阶依赖未接通通道，制造新无行为代码）；C 仅改 PanicPlan 名字（否决，双轨本体保留）。Redox relibc panic 与 Linux panic 打印均为单轨单出口，同构。
- **实施中的契约修正（测试抓到）**：初版文档承诺"截断并报告全长"（照搬 format_decimal 语义），测试实测发现 ByteWriter 计数封顶于缓冲容量——深想后裁决：panic 路径是尽力而为输出，截断检测无可行动作（C 的 panic printf 同样无截断概念），正确契约为**静默截断、返回实写字节数（恒 ≤ 缓冲长度）**，文档与测试同步对齐。另 `info.message()` 实为 `PanicMessage` 非 `Arguments`（std 构建下 handler 被 cfg 掉故此前未暴露），消息参数改泛型 `impl Display`。
- **Files**：`os/libs/minix-rt/src/diag.rs`（模块文档重写：消除 `PanicPlan` 幻影与"全局实例"虚构描述；删 `PanicStage` 枚举与 `ordered()`；新增 `format_panic_report`；测试一删两增）；`os/libs/minix-rt/src/lib.rs`（panic handler 删内联 BufferWriter 与手写 format_decimal 行号渲染，改调 diag::format_panic_report）。
- **测试**：minix-rt 60 passed（13 → 14：删梯子八阶、增报告形状与静默截断两条）；三 crate 60/167/248 全绿；clippy minix-rt 零新增（工作区 7 条均为依赖 crate 既有/并行线程来源，Fix #1 已记录）。
- **Docs**：07 篇 §3.3 重写（单一格式化之家 + 静默截断契约 + handler 三步形状）、§5 测试表更新（13 → 14，梯子八阶行替换为报告形状/截断两行，统计刷新至 2026-09-16）。§4 错误表中"未知控制码 | 调用者决定"为已登记分歧，维持不变。
- **边界**：`format_panic_identity` 与 `write_and_flush` 保留（A-8 步骤 2 接通内核通道时的消费面，07 篇有记载）；`DiagCode::from_number` 返 None 为已登记分歧，不改。

#### V1-P1-3（P1-design-missing）misc 发送半缺口：nanosleep 与 svrctl 只有判定半，select 组装跨篇无主——✅ 已修复 2026-09-16（Fix #10）

**现状锚点**：`os/libs/minix-sys/src/misc.rs:80-124` 的 nanosleep 是两个纯函数（`validate_sleep_request`/`remaining_sleep`），:126-148 的 svrctl 是纯分派（`dispatch_server_control`）——对比 pm.rs 的完整形态（`fork_via(transport)` 发消息收回复），缺的是：构造 VFS_SELECT 消息（空集 + 超时）→ `perform_syscall` → 用返回时间算 remaining 的完整链。C 真值：`minix3/minix/lib/libc/sys/nanosleep.c:28-56/:70-92`。11 篇明确"验证函数的调用方（未来的等待封装）只管调"（`11-misc-syscalls.md:62`）且"等待调用归第 09 篇"（:58）——但 09 篇 Rust 侧无 select 封装声明（`09-vfs-syscalls.md:129-130` 只列读写开关定位四载荷），**等待封装两篇都没接**。

**修改方案**：
- 方案 A（选定建议）：在 09 篇补 select 载荷与 `select_via`（空集 + 超时形状即可先落），misc.rs 补 `nanosleep_via(transport, request)` 组合两者；svrctl 补 `svrctl_via`（按分派结果发 PM_SVRCTL/VFS_SVRCTL）。
- 方案 B（否决）：维持纯函数并改为显式登记 ENOSYS 形状（09 篇 open 路径先例）。否决理由：open 的 ENOSYS 有真实理由（64 位路径布局未定稿）；select 的载荷形状（timeout + 空集）C 侧无歧义，无理由等 99 篇。
**验证**：`nanosleep_via` 带 CannedTransport 回放测试（断言 select 调用号 + 超时字段编码 + remaining 计算）。
**边界**：与 VFS_SELECT 载荷布局（`09-vfs-syscalls.md:56` 列了 +30）同轮落地；不碰 E-RMIBWIRE 的 sysctl 轨道。

**修复记录（Fix #10，2026-09-16）**：按方案 A 落地（含两处计划修订）——
- **C 真值修正**：细读 `nanosleep.c` 后发现剩余时间**不依赖 select 回写**——C 用 gettimeofday 前后测挂钟差（:44-92），select 的超时缓冲被写回与否都不影响。因此组合面拆干净：发送半（本条）+ 挂钟快照测量（`remaining_sleep` 已有，组合归调用方），不需要跨服务器回写语义。
- **Files**：`os/libs/minix-sys/src/vfs.rs`（+`TimeVal` LP64 时间值、+`select_empty_via` 空集等待：nfds@0 + 三空指针栏@8/16/24 + 超时地址@32，`mess_lc_vfs_select` 的 LP64 适配、+`VFS_CALL_SERVER_CONTROL=0x12B` pin callnr.h:115）；`os/libs/minix-sys/src/misc.rs`（+`nanosleep_via` 组合验证/换算/发送、+`svrctl_via` 按分组字符路由到 PM(38)/VFS(0x12B)，`mess_lc_svrctl` 双 64 位栏 @0/@8、+两调用号常量——PM_CALL_SERVER_CONTROL 落 misc.rs 而非 pm.rs，后者正被并行线程 E9 批次实时修改，避让）。C 版 svrctl 的 request 参数在 LP64 是 64 位栏，C 源 i386 形的 4 字节布局按 E7 先例适配。
- **测试**：vfs +2（空集五栏出站断言 + EINTR 传播）；misc +4（nanosleep 消息形状与超时地址非零、非法请求零往返、svrctl 双路由字节断言、陌生分组零往返）；minix-sys 179 → 185 passed。
- **Docs**：09 篇 §3.4 新增（等待形状，含通用 select 暂缓论证——fd_set 类型待消费方）、§5 表 15 → 17 + 统计刷新；11 篇 §5 表 12 → 16 + 发送半补齐说明 + 统计刷新。
- **边界**：完整 fd_set 语义的通用 select（含集合类型设计）暂缓待消费方；E-RMIBWIRE 的 sysctl 轨道未动。

#### V1-P1-4（P1-design-deviation）kerninfo MAGIC 失配：C 容错降级 vs Rust 拒绝启动——接线前必须裁决——✅ 已裁决并落地 2026-09-16（Fix #5，对齐 C 容错）

**现状锚点**：C 在 constructor 里校验失败就 `_minix_kerninfo = NULL` 继续运行（`minix3/minix/lib/libc/sys/init.c:22-26`，含原文注释"not fatal"语义）；Rust `initialize_runtime` 在 `os/libs/minix-rt/src/init.rs:238-245` 返回 `Err(Errno::ENOEXEC)`。当前不可达（`initialize_runtime` 无生产调用方，见 V1-P1-1），但 V1-P1-1 方案 A 第二步接线后即成可达行为。

**修改方案**：
- 方案 A（选定建议）：对齐 C 容错——MAGIC 失配降级为"无 kerninfo 状态"继续运行，依赖 kerninfo 的调用点（时钟直读、IPC 向量）各自按 C 语义退回默认（C 的默认表预初始化于 init.c:10-18）。
- 方案 B（保留现状 + 登记 ARCH）：严格拒绝。仅在确认 minix-rs 世界里"无 kerninfo 的用户态二进制"不可能存在时可选，且必须三处一致标注 `[ARCH: 容错降级 → fail-fast]`（doc 03 + design + 代码）。
**验证**：裁决后补 MAGIC 失配单测（现 init.rs 测试只测成功路径与校验函数本身）。
**边界**：与 init.rs 的双重 MAGIC 校验（init.rs:238-245 先比对、:246-257 又经 `ValidatedKernInfo::new` 重复校验，第二次必然通过；:251-256 的 `NullPage` 分支不可达——`ValidatedKernInfo::new` 只返回 `BadMagic`，handoff.rs:150-157）同轮清理。

**修复记录（Fix #5，2026-09-16）**：按方案 A 落地，且完成一轮类型重表达设计——
- **裁决与设计**：C 的契约是"初始化没有致命失败，只有可诊断的状态"（`init.c:22-26` 两种异常都清零继续，默认表预装持续有效，`init.c:10-18`）。Rust 化的正解不是"Result 换个形状"而是承认**初始化必然产出状态**：`initialize_runtime` 改为直接返回 `RuntimeState`（删除 `InitError`/`InitOutcome` 二元组——Result 通道是 C 没有的控制流，属过度表达）；`RuntimeState.kerninfo` 从 `Option<ValidatedKernInfo>` 改为 `KerninfoAvailability` 三态枚举（`QueryFailed(i32)`/`BadMagic{found}`/`Available`）——C 的 NULL 全局把三种情形压成不可区分，Rust 枚举以一个变体的代价买回可诊断性。第二处语义修正：失败路径的表选择从"无表"改为"保持调用方后备表"（对齐 C 预装默认表持续有效，旧实现的 `IpcTableSelection::None` 是第二处不忠实）。
- **双重校验与 NullPage 清理**：初始化只经 `ValidatedKernInfo::new` 校验一次（外层手工比对删除）；`new` 的错误类型从 `HandoffError` 收窄为专用 `MagicMismatch { found }`（按值头部只可能魔数失败，`NullPage` 属指针路径 `select_initial_stack_pointer`）——不可达分支从类型上消除而非 `unreachable!()`。
- **Files**：`os/libs/minix-rt/src/init.rs`（状态模型重写、函数签名简化、测试 9 → 7：两条降级测试断言"状态 + 后备表存活"，删两条随类型消失的测试）；`os/libs/minix-rt/src/handoff.rs`（`MagicMismatch` 新类型 + `new` 签名收窄 + `HandoffError` 收缩为单变体 `NullPage`；测试一改一增）。
- **测试**：minix-rt 46 passed；三 crate 46/167/248 全绿；clippy 零新增；no_std+panic-handler 编译通过。
- **Docs**：03 篇 §4 重写（"初始化没有致命失败，只有可诊断的状态"，含 C 契约推理与错误号映射的去处）、§5 测试表 9 → 7 + 统计刷新；01 篇 §4 错误表（`HandoffError::BadMagic` 行改为 `MagicMismatch` 独立行 + 设计理由）、§5 计数 9 → 10；07 篇统计句刷新（46）。
- **边界**：接线时（E1 切片 5 / V1-P1-1 第二步）`_start` 消费 `RuntimeState`，`is_ready()` 即 C 的 `_minix_kerninfo != NULL` 检查惯例；`negative_to_errno`/`errno_to_negative`（A-5 负值回传约定）不受影响。

#### V1-P1-5（P1）99 篇改写是多个已登记缺口的共同解锁前置，应升格为排期项

99 篇（全局概念：endpoint/generation、message 布局、服务号常量）pending，直接阻塞三处已登记工作：09 篇 open 路径布局（64 位 40 字节内联裁决，`09-vfs-syscalls.md:72/:123`）、V1-P2-2 的 wire 单点权威收敛、08 篇"消息体布局集中归档移交第 99 篇"（`08-pm-syscalls.md:84`）。建议与 00 篇（总览，20 行骨架）同批改写；改写时以 §2 矩阵为输入，把各篇"移交 99"的悬空项一次性收口。plan.md §6.1 的"00~13、99 pending（2026-08-16）"状态行同步刷新（01~13 实际已于 2026-09-05 全部收敛，`.review/codex/runtime/STATE.md`）。

#### V1-P2-1（P2）查询族死 `None` 签名：`Option` 永远是 `Some`——✅ 已修复 2026-09-16（Fix #6）

`os/libs/minix-sys/src/vm.rs` 四个 wrapper 的签名是 `Result<Option<T>, Errno>` 且 docstring 声称失败报 `None`（mmap_via :171、remap_via :305、physical_address_via :335、reference_count_via :356——docstring 原话"A failed call reports … None"），但函数体只有 `Err`（perform_syscall 失败）与 `Ok(Some(...))` 两条路径，`None` 不可达。C 真值（`minix3/minix/lib/libc/sys/mmap.c`）：失败全部走 `_syscall` 返回值非 OK（mmap.c:44-47/:103-107/:150-153/:166-169），哨兵 MAP_FAILED/0/-1 只出现在 C 的返回值约定层，不在载荷里。10 篇文档声称"有无表示消除三个哨兵"（`10-vm-syscalls.md:62`）——方向正确，但收窄没做完。**建议**：签名改 `Result<T, Errno>`，docstring 同步；`mmap_via` 的 `Option` 包裹一并解除。**否决方案**：实现哨兵检测（读回复地址是否 -1）——否决理由：C 的失败协议就在 m_type，载荷哨兵是返回值层的表达，检测载荷反而引入 C 没有的第二失败通道。

**修复记录（Fix #6，2026-09-16）**：按建议落地——
- **Files**：`os/libs/minix-sys/src/vm.rs` 四 wrapper 签名 `Result<Option<T>, Errno>` → `Result<T, Errno>`（mmap_via/remap_via/physical_address_via/reference_count_via），docstring 的"失败报 None"表述改为"失败走 Err，C 哨兵不过此接口"；零参便捷封装 `minix-sys/src/lib.rs::mmap` 的 `.map().unwrap_or(null)` 解包简化为直接取值（同处文档表述同步）。
- **测试**：四个断言去除 `Some` 包裹；新增 `test_failed_syscall_propagates_errno`（脚本传输注入协议失败，断言 errno 原样到达调用者、哨兵值不出现）——该测试的编写过程还锁定了 transport 错误携带**负值** errno 的 C 陷阱约定（正值会被 `m_type < 0` 判定误吞为成功，已写入测试注释）。minix-sys 167 → 168 passed。
- **Docs**：10 篇 §1 概念两处（哨兵模型改为结果类型表述，补"C 的失败判定本在协议返回值"的机制说明）、§3.2 整节改写（"用结果通道消除三个哨兵"，如实记载双层签名弯路与收窄理由——接口上每个变体必须有真实路径可达）、§5 测试表 13 → 14 + 统计刷新（168）。
- **边界**：minix-types 的 `VmMmapOut`/`VmReply` 等服务端语义层不受影响；失败载荷哨兵检测（否决方案）不再考虑。

#### V1-P2-2（P2，跨层部分挂 edge E-MINTYPES-RUNTIME）wire 打包双体系与 helper 三重复

同一调用的 wire 契约存在两套表达：minix-types 的类型化路线（union 具名 arm + 语义 In/Out 结构 + `DecodeFromM1`/`EncodeToM1` codec trait，消费面：VM 服务器 encode.rs:141-144 与 dispatcher.rs:720-734——但 VM 分发主路径实际走固有方法 `decode_message`，trait 消费仅 4 处）与 minix-sys 的本地路线（每个调用族本地 `#[repr(C)]` payload + 裸字节打包，`cleared_message`/`write_payload` 在 `pm.rs:91-108`/`vm.rs:96-110` 各复制一份，rs.rs:82 又一种 raw 直写）。实例：VM_REMAP 在 `minix-types/src/ipc/vm.rs:526-527` 区段标注 "DEFERRED"，而 minix-sys `vm.rs:305-327` 已有该调用的客户端打包实现（且 `remap_via` 泛化 call 号可表达 REMAP_RO）——两处对同一契约的状态认知不一致 [待验证：两者语义层不同（服务端解码类型 vs 客户端打包），执行时按 fix-guard 复核后再定级]。**建议**：crate 内部分——`cleared_message`/`write_payload` 收敛为 minix-sys 单一内部 helper，本地 payload 补 56 字节/偏移断言（§2.6）；跨 crate 部分——99 篇定稿时裁决"布局单点归 minix-types、minix-sys 只做打包与传输"的边界（Redox 先例：syscall 契约单源 syscalls.toml + 单 crate 统一 data 模块，§7），登记 edge E-MINTYPES-RUNTIME。

#### V1-P2-3（P2）注释锚点失实批（文档-代码同步）——✅ 十处全部处置 2026-09-16（Fix #1/#2/#4/#12）

逐条 grep 复核过的失实锚点，修复须走 fix-guard（读 ±5 行、一次一条）：
| # | 位置 | 失实内容 | 事实 |
|---|------|---------|------|
| 1 | `minix-rt/src/lib.rs:109` | 引用 "Minix3's `lib/crtso`" | 全树无 crtso 文件（find 实证），应为 `lib/csu/arch/x86_64/crt0.S` + `crt0-common.c` |
| 2 | `minix-rt/src/lib.rs:124-126` | "`minix_sys::exit` loops forever（stub）" | 已过时：`minix-sys/src/lib.rs:139-141` 委托 `pm::exit_via` 发 PM_CALL_EXIT（`pm.rs:162-172`），失败才自旋 |
| 3 | `minix-rt/src/lib.rs:330-331` | "dependency direction: kernel → minix-rt" | 反了：kernel 不依赖 minix-rt，权威注册表在 minix-types（V1-P0-1 一并修） |
| 4 | `minix-rt/src/lib.rs:48-49` | "Redox ships the same shape in its `linker` crate" | Redox 无此物；对应物是 relibc（src/crt0 + src/startup.rs，§7） |
| 5 | `minix-rt/src/start.rs:5` | crt0.S "7 instructions" | 实为 6 条（crt0.S:44-49） |
| 6 | `minix-rt/src/start.rs:39-47` | "startup routine treats this exact value as…"（environ 哨兵被检测） | 本树 C 无检测点（crt0-common.c:154 无条件赋值；该说法仅溯源自 environ.c:8-11 注释） |
| 7 | `minix-rt/src/alloc.rs:38` | malloc.c:381 | pageround 实际在 malloc.c:383 |
| 8 | `minix-rt/src/alloc.rs:9-11` | "large objects come straight from memory mapping (malloc.c:317-388)" | 该区间是 sbrk 增长路径；MMAP 用于页目录（malloc.c:448/:559），非大对象直走 mmap |
| 9 | `minix-rt/src/diag.rs:15` | `PanicPlan` 类型 | 不存在，实际是 `PanicStage`（并入 V1-P1-2 修复亦可） |
| 10 | `minix-rt/src/handoff.rs:103-104` | "matching the C NULL checks in init.c and kernel_utils.c" | kernel_utils.c:52-54 对 kuserinfo 无空指针检查直接解引用——Rust 检查比 C 更严，应改为显式偏差声明 |

**修复记录（Fix #12，2026-09-16）**：余下五处落地（前五处已随 Fix #1/#2/#4 处置）——
- **Files**：`os/libs/minix-rt/src/lib.rs`（_start 文档注释的 crtso 虚构锚点改为 crt0.S + crt0-common.c 真锚点，并注明完整诞生链挂 E1 切片 5；_start 函数体内"exit loops forever（stub）"过时注释改为如实描述 PM_EXIT 消息路径与自旋兜底）；`os/libs/minix-rt/src/alloc.rs`（malloc.c:381 → 383 实测修正；模块文档的大对象归因改写——317-388 是 sbrk 增长循环，MMAP 请求在 448/559，原"大对象直走 mmap"表述失实）；`os/libs/minix-rt/src/handoff.rs`（kuserinfo 空指针检查从"matching the C NULL checks"失实声明改为 DELIBERATE DIVERGENCE 显式偏差——C kernel_utils.c:52-54 无检查直接解引用，Rust 显式化是加固决策）。
- **逐项对账**：#1 crtso→Fix #12；#2 exit 桩注释→Fix #12；#3 依赖方向→Fix #1；#4 Redox linker crate→Fix #4；#5 "7 instructions"→随 start.rs 删除失效 + 02 篇已改（Fix #4）；#6 sentinel 检测叙述→随 start.rs 删除失效（Fix #4）；#7 malloc.c:383→Fix #12；#8 mmap 归因→Fix #12；#9 PanicPlan→Fix #2；#10 kuserinfo 偏差→Fix #12。
- **测试**：minix-rt 46 passed；clippy 零新增。

#### V1-P3-1（P3）测试浅化批

三处名实不符或覆盖弱于命名（非错误，浅验证）：`alloc.rs:598` `test_big_allocation_spans_whole_pages` 只断言可写可释放，无页边界检查（改名或补对齐断言）；`diag.rs:512` panic 阶梯只断言长度 8 与首尾元素，中间错位不可发现（补全序断言）；`ipc.rs:810` `test_enqueue_marks_slot_valid_last` 在单线程测试中"VALID 最后写"不可观察，实际验证的是入队后状态（注释说明提交序由何种机制保证，或删除 "last" 措辞）。其余抽查（syscall 重试序列、路径打包边界四连、grant 的 ABA 守卫）均为扎实验证，测试分层形态与三层 crate 角色自洽（minix-sys 48% Canned 回放 / minix-types 17% 布局 pin / minix-rt 90% 纯函数语义）。

#### V1-P3-2（P3）实现细节两小项

`minix-rt/src/lib.rs:203-217` 手写 swap 票券版 Once——`core::sync::Once`（Rust 1.73 起进 core）可替，或保留但补一行"为何不用 Once"的理由注释（现有 SAFETY 注释只论证了正确性，没论证不用标准件的理由）。`os/libs/minix-sys/src/syscall.rs:190/:203/:211-217` 中文注释段与全英文正文混用，统一为英文（与 crate 其余部分一致）。

### 3.3 已登记缺口一览（防重复扫描）

以下缺口**已有归属**，本轮不立项：09 篇 open 路径 ENOSYS（待 99 篇）；07 篇诊断通道未接内核（A-8 阶段 2）；06 篇 VM 页供给第二来源（"接通后再补"）；12 篇窄 helper 有意不做；10 篇 vm_set_priv/update/memctl/prepare 归集成 stage；09 篇 socket 族归 17-stage-net；sysctl 发送半归 E-RMIBWIRE；minix-sys trap 通电归 E1 切片 5；termios/signal 大族按需（13 篇登记）；`real-trap`/`panic-handler` feature 无消费者（有意门控，通电即启用）。

---

## 4. 死代码清单（code-excellence 子目标，逐项"为何死 + 处置"）

| # | 位置 | 内容 | 为何死 | 处置建议 |
|---|------|------|--------|---------|
| 1 | ✅ `minix-rt/src/start.rs` 全模块 | `StartupStage`/`EntryDescriptor`/`short_program_name`/`run_function_array`/`RunOnce` | 生产 `_start`（lib.rs:112-128）不走 start.rs；workspace grep 零生产调用方 | **已删**（Fix #4，2026-09-16）；设计准则留存于 02 篇 §3 |
| 2 | `minix-rt/src/init.rs:225-275` | `initialize_runtime` | 零生产调用方（lib.rs:30-32 自认"将来再委托"） | 保留至 V1-P1-1 第二步接线时收编（唯一有"未来消费者"辩护的项——接线前不删，防丢设计） |
| 3 | `minix-rt/src/handoff.rs:291-330` + 测试 :499 区段 | `STACK_MINIMUM_BYTES`/`compute_stack_size` | 零消费方，且常量与 C 矛盾（V1-P0-3） | 删 |
| 4 | `minix-rt/src/lib.rs:376-412` | 本地 `PANIC_DIAGNOSTIC_HOOK` 注册表三函数 | kernel 写的是 minix-types 注册表，此表无人写（V1-P0-1） | 删，改委托 minix_types |
| 5 | `minix-rt/src/init.rs:251-256` | `NullPage` 错误分支 | `ValidatedKernInfo::new` 只返回 `BadMagic`，分支不可达 | 删（V1-P1-4 同轮） |
| 6 | ✅ `minix-rt/src/start.rs:67-133` vs `handoff.rs` `ProcessStrings` | `EntryDescriptor` 与 `ProcessStrings` 近同构（差一个字段） | 双真相源，前者随死代码批消失 | **已删**（Fix #4 随 start.rs 整模块删除，保 `ProcessStrings`——01 篇域模型） |

拿不准项（OQ 上交，不擅删）：`minix-sys/src/misc.rs` 的 `MIB_ENDPOINT_NUMBER`/`MIB_CALL_SYSCTL`/`SYSCTL_SHORT_NAME_LENGTH` 三个常量——sysctl 发送半归 E-RMIBWIRE 后，这些常量是否由 minix-sys 保留（rmib.rs 已有自身常量）需该轨道裁决。

---

## 5. edge 增补指针表

| edge 编号 | 内容 | 对应本文件条目 |
|-----------|------|---------------|
| E-MINTYPES-RUNTIME（新登记） | minix-types 布局单点权威收敛：minix-sys 本地打包的对齐方向、codec trait 消费面收窄决策、VM_REMAP/RO 双处状态标注对齐；99 篇定稿驱动，挂 campaign D 波次（布局对账与 wire 系统化） | V1-P2-2 的跨 crate 半 |
| E-MINSYS-SCOPE（新登记） | minix-sys 内六个域外 stage 客户端模块（ds.rs/devman_client.rs/inputdriver.rs/rmib.rs/socket.rs/usb_model.rs，约 3108 行）的 crate 内聚性处置（维持/feature 门控/迁出），涉 07/10/11/12/16/17 各 stage | §0 范围声明；与 E-MINSYS-HYGIENE 衔接 |
| E1（增补） | 切片 5 通电族增加"首个 no_std minix-rt 二进制"验证面：`_start` 诞生链（V1-P1-1 第二步）、panic handler + hook（V1-P0-1）、`real-trap` feature 首次真机启用 | V1-P0-1 / V1-P1-1 / V1-P1-4 |

---

## 6. Rule Discovery（Step 5.7）

本轮两个候选模式（如采纳由规则库接续编号）：
1. **平行实现无消费方**（双实现栈常量先例）：同一 C 语义在两个 crate 各有一个实现，其中生产实现之外的那个零消费方且悄然漂移（372 vs 1400）。识别命令：对"自认刻意的平行实现"注释做 grep，逐个验证第二消费者是否存在。与模式 80（mock-only 抽象）互补：80 是"只有测试实现"，本模式是"有两个实现但只有一个有生产消费者"。
2. **契约-文档-实现三角差**：plan.md 覆盖契约（§5.1）声明全集、单篇文档声明子集策略、代码是更小子集，三层各自"诚实"但缺口无人登记（VM 客户端库四文件族先例）。识别命令：review 单篇时先抽 plan.md 对应映射行，对文档 grep 契约中的每个 C 文件名。

---

## 7. gate-evidence 与 Redox 对照来源

```text
$ cargo test -p minix-rt -p minix-sys -p minix-types   # 2026-09-16
test result: ok. 59 passed; 0 failed    (minix-rt)
test result: ok. 161 passed; 0 failed   (minix-sys)
test result: ok. 246 passed; 0 failed   (minix-types)

$ cargo clippy -p minix-rt -p minix-sys -p minix-types --lib
warning: large size difference between variants   # 仅 minix-types 1 条

$ bash tools/design-coverage-check.sh fork-syscall-rewrite --stage 14-stage-runtime
# 01~13 全 PASS；00/99 缺快照 → CRITICAL（Step 0.3 登记，见 §1）

$ grep -c "cache\|vm_info\|procctl\|rusage" 10-vm-syscalls.md   # V1-P0-2 证据
0

$ grep -rn "compute_stack_size\|STACK_MINIMUM_BYTES" os/ --include="*.rs" | grep -v handoff.rs
（零命中）                                                          # V1-P0-3 证据

$ grep -rn "set_panic_diagnostic_hook" os/kernel/src/ | head -1
os/kernel/src/lib.rs:1638:    minix_types::set_panic_diagnostic_hook(...)  # V1-P0-1 证据
```

Redox 对照（联网检索，2026-09-16）：
- relibc 启动链（crt0 汇编 → `__libc_init` 的 TLS/auxv → init_array → main）：[redox-os/relibc](https://github.com/redox-os/relibc)；TLS 与 errno 的线程局部依赖见 [RSoC 2024 动态链接器报告](https://www.redox-os.org/news/01_rsoc2024_dynamic_linker/)——本仓 A-5 的 `Result<_, Errno>` 语义消除了 errno-TLS 需求，属登记过的 ARCH 差异而非缺口。
- redox-syscall 单 crate 组织（number/error/flag/data + 内联汇编 wrapper，契约单源）：[redox-os/syscall](https://github.com/redox-os/syscall)、[docs.rs/redox_syscall](https://docs.rs/redox_syscall)、[syscalls.toml ABI 规范](https://github.com/redox-os/design/blob/master/design/syscalls.toml)——V1-P2-2 与 E-MINTYPES-RUNTIME 的"布局单点权威"方向依据。

---

## 8. 修复推进顺序建议

1. **V1-P0-1**（hook 分裂）+ **V1-P1-2**（诊断单轨化）：同在 panic 路径，一轮修完，依赖最小。
2. **V1-P0-3** + **V1-P1-1 第一步**（死代码批）：纯删除，`cargo test -p minix-rt` 守护。
3. **V1-P0-2**（10 篇补清单 + wrapper 排期）：文档先行，wrapper 随消费方 stage 推进。
4. **V1-P2-1 / V1-P2-3**（签名收窄 + 注释批）：小步快修，各走 fix-guard。
5. **V1-P1-3**（nanosleep/svrctl 发送半）：随 09 篇 select 载荷排期。
6. **V1-P1-4 + V1-P1-1 第二步**：绑定 E1 切片 5 通电窗口执行。
7. **V1-P1-5**（00/99 篇改写）：解锁 09 open 路径与 V1-P2-2 收敛。
8. edge 两条 + E1 增补按 edge_todo.md campaign 波次执行（单线程轨道）。

---

## 9. 存档指引

- 本轮无历史轮次（V1 为首轮）。后续轮次按 V{N} 编号，满轮归档至 `archive/todo-V{N}-archive-日期.md`（对齐 `../02-stage-vm/todo.md` 先例）。
- 跨 stage 条目的唯一入口是 [`../edge_todo.md`](../edge_todo.md)（E-MINTYPES-RUNTIME / E-MINSYS-SCOPE / E1 增补见彼处正文）；本文件条目修复时走 todo-fix 三段式（讲明白 → 多方案对比 → 实施）+ fix-guard + 文档-代码同步。
- Review 中间产物（STATE.md 等）在 `.review/zcode/runtime/`（ZCode 工作区，与 codex/trae/claude 隔离）。
