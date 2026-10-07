> **创建**: `2026-10-08`
> **本台账是什么**: `rewrite-notes/coordination/` 全部待办与 18 份阶段待办台账的**合并视图**。条目的描述权威仍在原账本（见 `TODO-LEDGER-INDEX.md` §0 的归属声明），本台账只统一两件事：**状态**（六值词表）与**证据**（可复跑命令 + 现场读数）。
> **核实现场基线**: 代码读数全部在快照 `87784ca04` 上重跑得出，不抄任何二手计数。行号会漂，所以锚点优先给符号名；表内出现的行号只是本次扫描时点的坐标。
> **状态词表**（唯一合法取值，细则见 `TODO-LEDGER-INDEX.md` §3）：`open` 未做 ｜ `stopgap landed` 临时守卫已落、正式修法未落 ｜ `partially fixed` 部分完成 ｜ `adjudicated` 已裁决不做或有替代结论 ｜ `closed` 完成 ｜ `dropped-from-ledgers` 已从账本移出但债务仍在。
> **证据级**：`L1` 一条命令可定案 ｜ `L2` 读码 + 追调用方可定案 ｜ `待验证` 结论依赖真机跑通或需要人裁决，**本台账不给终态**。

# 未完成待办总账

## 怎么读这张表

每行是一条待办。`原始 ID` 保留它在各本账本里的编号（同一件事常有多个编号，这是本项目账本的真实形态，不做重编号以免改写历史）。`核实命令` 可以直接复制重跑；跑出来的读数与`读数`列不符，说明现场已经往前走，应当回头改本台账而不是改代码。

## §1 内核与多核（这一节直接决定 `01-stage-kernel` 的文档能不能定稿）

| 条目 | 状态 / 证据 | 原始 ID 与来源 | 代码锚点 | 核实命令与读数 | 下一步，以及它在阻塞谁 |
|---|---|---|---|---|---|
| 进程还不能真正安放到次级核：解除主核钳制的三个前置条件没还，钳位本身仍在生效 | `stopgap landed` / `L2` | `SD-24`、`P-ALL-03`、`SD-37`、`SD-41`（`STRUCTURAL-DEBT-REGISTER-20261008.md` §5.1）；裁决 `PD-02`、`PD-19`；跨阶段条目 `E-SCHEDSMP` | `os/kernel/src/sched.rs:fn clamp_cpu_to_bsp`（调用点在 `fn sched_proc` 腿内）；钳位理由见同文件 `:418-428` 的中文说明块 | `grep -rn "clamp_cpu_to_bsp" os --include=*.rs` → 定义、调用、钳位专项测试三处俱在 | 补齐下面三条前置（中断入口唤醒空闲核、空闲标志清零、次级核放行邮箱生产者）并根治跨核内存腐蚀，才能撤钳。阻塞 `16-smp.md`、`11-scheduling-primitives.md`、`15-clock-timer.md` 的"进程落在次级核"叙述——按 `STAGE-KERNEL-FREEZE-READY-20261008.md` §4 的边界，这部分写成显式推迟即可，不算定稿障碍 |
| 中断入口没有"唤醒空闲核"的清零臂：空闲核被放出去之后没有任何站点把它标回忙 | `open` / `L2` | `SD-24` 的前置臂之一；`E-SCHEDSMP` | 应有落点 `os/kernel/src/trap_dispatch.rs`（中断入口）与 `os/kernel/src/clock.rs` | `grep -rn "fn context_stop_idle" os --include=*.rs` → **0 命中**（只有注释提到这个名字） | 实现"中断入口发现本核空闲即清标志并重启本地定时器"的对位逻辑（C 对位在 `minix3/minix/kernel/arch/i386/arch_clock.c` 的空闲上下文路径）。阻塞上一行 |
| 空闲标志只会被置起、从不被清掉：`cpu_is_idle` 写入点唯一，全库没有清零站点 | `open` / `L2` | 同上（是上一条的机制细节，单列以防合并时丢失） | `os/kernel/src/lib.rs`（空闲例程内置起，注释自陈依赖尚未实现的唤醒臂）、`os/kernel/src/smp.rs:fn cpu_is_ready` 相关字段 | `grep -rn "cpu_is_idle" os --include=*.rs` → 置起 1 处、结构定义与默认值各 1 处、其余为注释与测试断言；**清零赋值 0 处** | 与上一条同批修。这条是"次级核醒着但叫不醒"的直接证据 |
| 次级核放行邮箱没有生产者：`AP_GO` 只有声明和等待位，内核从不写它 | `open` / `L2` | `SD-37`；`R3.5` 评审的 `K1` 项（"全 hart 进 payload"这一前提被在树源码否证后留下的死衍生件） | `os/arch/src/riscv64/ap_early_entry.rs:static AP_GO`、`os/arch/src/arm64/ap_early_entry.rs:static AP_GO`；等待位在 `os/kernel-image/src/main.rs`、`os/kernel-image/src/bootface_a64.rs` | `grep -rn "AP_GO" os --include=*.rs` → 两处原子定义、若干读取与注释、`os/kernel/src/smp.rs` 内一处说明性注释；**写入调用 0 处** | 要么在内核发布引导记录之后写放行位，要么把等待循环改造成与 SBI/PSCI 启动语义一致的形态并删掉邮箱。阻塞 `16-smp.md` 描述次级核诞生链的章节 |
| riscv64 非确定性内存污染仍未结案：写者没有落网，它同时是"真多核"的书面前置 | `open` / `待验证` | `SD-10`（升级状态）、`P-RV-02`、裁决 `PD-16`、"问题乙"；案卷见 `CASE-RISCV64.md` | 现象落点 `os/servers/vm` 遍历页表时的缺页；判据面在 `os/arch/src/riscv64/paging.rs` | 需要真机：`os/qemu-tests/test-atf-riscv64.sh` 多轮复跑 + 仪器重建（结构债台账明写"仪器重建先于任何门测声明"） | 案卷里已经排掉六类假说（跨核 TLB、地址空间标识符、外设改写、栈指针对齐、输出体积拐点、开关未真正生效），剩余候选按量级排序在 `CASE-RISCV64.md`。阻塞真多核撤钳与 `16-smp.md` 的"多核正确性"结论 |
| x86_64 浮点归属从未被写入，于是每个进程都被当作非归属者关掉浮点单元；用户态一条浮点指令就会撞上门控停止 | `open`（fail-loud 守卫已落，恢复腿未接） / `L2` | `14-exception-interrupt.md:539`、`31-fpu-context-switching.md` 的 `X-8`/`T5` 波；`SD-23` 的 x86 对位面 | `os/kernel/src/lib.rs:fn finish_and_restore`（读 `fpu_owner` 决定 `enable`/`disable`）、`os/kernel/src/trap_dispatch.rs`（riscv64 与 aarch64 两臂各有一次归属写入） | `grep -rn "fpu_owner" os --include=*.rs` → 写入点只在 riscv64 臂、aarch64 臂与跨核迁移释放腿；**x86_64 无任何写入点**；x86 的浮点门控结果落在 `os/kernel/src/trap_dispatch.rs` 的 `FpuTrap` 分支，该分支是显式 `panic!` 而非静默继续 | 影响面需真机确认（当前用户态程序是否碰浮点）。定稿 `14`、`31` 两篇之前必须处理：要么接上恢复腿，要么在文档里把"门控生效而恢复未接"写成显式边界。**这条不属于次级核承载问题，单核同样触发** |
| 次级核本地定时器已经落地（改名并换向量），但真机载体只打印不断言 | `partially fixed` / `L2` + `待验证` | `K6`（`edge1.md`）、`smp_todo.md` §28 | `os/arch/src/x86_64/clock.rs:fn init_local_timer`、`const LAPIC_TIMER_VECTOR`（值为 `0xf1`，因为 C 的 `0xf0` 已被调度中断占用）、`os/kernel/src/clock.rs:fn rearm_local_tick`、调用链 `os/kernel/src/smp.rs:fn smp_ap_tail` | `grep -rn "init_local_timer\|LAPIC_TIMER_VECTOR" os/arch/src/x86_64/clock.rs` → 均在；`grep -n "0xf1" os/kernel/src/trap_dispatch.rs` → 该向量臂内调用量子耗尽检查 | 给 `os/qemu-tests/test-kernels/.../test-smp-aps` 补自动断言（现在靠人读串口数字）。**此前有扫描按 C 的函数名与 `0xf0` 向量去查，得出"未实现"的结论，是假阴性** |
| 每核运行队列与抢占下发的三件套 | `adjudicated` / `L2` | `K2`（`edge1.md`）、`S-6.3` 设计决策 | `os/kernel/src/proc_table.rs:fn sched_for_cpu_mut` | `grep -rn "per-CPU\|共享队列" os/kernel/src/sched.rs` 读到设计记账 | 已有结论：大内核锁下就绪队列由锁串行化，按核分队退化为单队列，因此记账取代而非实现。文档要写这个理由，否则会被后来者当成缺口重做 |
| riscv64 的软件中断与外部中断臂、aarch64 的生成中断臂尚未接入；两架构的用户同步异常还没有到信号的执行腿 | `open` / `L2` | `14-exception-interrupt.md` 自述的登记缺口、`SD-41`；`E-SCHEDSMP` 相关 | `os/arch/src/riscv64/arch_init.rs`（注释明写软件中断使能属于多核中断下发那一步，此处刻意不开）、`os/kernel/src/trap_dispatch.rs` | `grep -n "SSIE" os/arch/src/riscv64/arch_init.rs` → 只在注释里，无使能写位；`sed -n '1370,1380p' os/kernel/src/trap_dispatch.rs` → 到信号的执行腿写作"随下一波三架构" | 定稿 `14` 之前要把"哪些臂已通、哪些是登记缺口"写成分架构的对照表，不能再有笼统的"三架构对齐"结论 |
| 进入用户态前把用户栈指针对齐到十六字节：只有 x86_64 有 | `open` / `L2` | `SD-21`、`P-A64RV-03` | `os/libs/minix-rt/src/crt0.rs`（x86 臂的对齐指令；aarch64/riscv64 无对应） | `grep -rn ", -16\|align" os/libs/minix-rt/src/crt0.rs` → 仅 x86 命中 | 已推迟到用户态真跑批。写 `10-switch-to-user.md` 时要注明这是按架构的缺失项而非差异美化 |
| 数据写入之后转成指令执行时的缓存维护：aarch64 完全没有，riscv64 只有一处散落 | `open` / `L2` | `SD-20`、`SD-42`、`P-A64RV-02` | 唯一现存的清理动作在 `os/kernel/src/arch/riscv64/higher_half.rs`（`fence.i`） | `grep -rn "fence\.i\|dc *cvau\|ic *iallu\|sync_icache" os --include=*.rs` → **2 命中，全在同一个 riscv64 文件**（其一还是注释）；aarch64 侧 0 命中 | 自修改代码、加载新镜像之后必须有体系结构的指令缓存清理。当前没有任何抽象承载它，属真缺口 |
| 内核访问用户内存的特权屏蔽位模型（x86 的 SMAP、arm64 的 PAN、riscv 的 SUM） | `adjudicated` / `L2` | `SD-22`、`P-A64RV-04`、裁决 `PD-21` | 三个架构的 `protection.rs` / `boot.rs` 与 `os/kernel/src/ipc.rs`、`os/kernel/src/lib.rs` 的拷贝路径 | `grep -rn "PAN\|SUM" os/arch/src os/kernel/src --include=*.rs` → 命中全在注释与位定义，生产代码不置位 | 已有结论：软件层"不直接解引用用户虚拟地址"承担同一防线，硬件位随写下沉批次再谈。文档必须写这条边界，否则会被读成"忘了设位" |
| 重新生成 `01-stage-kernel` 文档的前置：内核与架构代码按**文件名**引用这些文档，改名或拆分会当场断链 | `open` / `L1` | 本轮新登记（原无编号）；相关门 `tools/check-rs-unwired.sh:6-16` 要求生产占位符前五行注释含文档引用 | 引用密集者：`os/` 内提及 `06-proc-init-boot-proc.md` 六十三处、`16-smp.md` 十八处、`11-scheduling-primitives.md` 十一处 | `grep -rhoE "[0-9]{2}-[a-z0-9-]+\.md" --include=*.rs os \| sort \| uniq -c \| sort -rn` | 未来重排落地时，改名必须与代码注释同一笔提交完成，并复跑 `tools/check-rs-unwired.sh`。本轮只登记不动代码 |

## §2 三架构齐平余件（`P-*` 中仍未闭合者，已回勾项见 `TODO-LEDGER-DONE.md`）

| 条目 | 状态 / 证据 | 原始 ID | 锚点 | 核实命令与读数 | 下一步 |
|---|---|---|---|---|---|
| x86_64 从未被放进上游自动化测试套件的装配白名单，"三架构齐平"这句话里最大的缺口 | `open` / `L1`（白名单事实）+ `待验证`（能否跑绿） | `P-X86-01`；**裁决已定**：`PD-01` 采甲（补进），豁免方案被否 | `os/xtask/src/image.rs:const ATF_BOOT_LEG_READY` | `grep -n "ATF_BOOT_LEG_READY:" os/xtask/src/image.rs` → 值为 `["aarch64", "riscv64"]`；`ls os/qemu-tests/*x86_64*` 无目标③专属门 | 装配面可静态铺（白名单 + 镜像脚本），执行时与 `PD-09` 对账表第五列（初始寄存器约定）互检；终态判据必须上机 |
| riscv64 从未在多核配置下跑过测试套件与命令面 | `open` / `待验证` | `P-RV-01` | `os/qemu-tests/test-atf-riscv64.sh`、`os/qemu-tests/test-cmd-smoke-riscv64.sh` | `grep -n "smp" os/qemu-tests/test-atf-riscv64.sh` → 全部单核 | 次级核入口体已落地（见 `TODO-LEDGER-DONE.md` §1），可以尝试 `-smp 4` 复跑；判据是每道门核数与参与架构都要写明 |
| 架构目录里被目标架构整模块门住的单元测试，宿主根本不编译它们；已用"清单冻结审计"把未知集合变成已知集合，但逐文件的宿主可达化仍在推进中 | `partially fixed` / `L1` | `P-ALL-01`、`SD-34` | 旁路审计 `os/arch/tests/arch_gated_audit.rs`（冻结清单 + 漂移即红）；专项钉住文件 `os/arch/tests/{riscv64,arm64}_return_leg_pin.rs`、`{riscv64,arm64}_trap_leg_shape.rs`、`paging_encoding_pins.rs` | `for d in os/arch/src/arm64 os/arch/src/riscv64 os/plat/src/arm64 os/plat/src/riscv64; do grep -c "#\[test\]" $d/*.rs; done` → 现算 `88`（arm64 39 + riscv64 42 + plat 3 + 4）；`ls os/arch/tests/` → 旁路文件已从两枚增至七枚 | 剩余部分：把行为级测试真正搬进宿主可发现的 `<crate>/tests/`，而不是只做源码形状审计。齐平清单记的"约 90 枚"与本台账现算的 88 枚之差属正常漂移 |
| 结案之后没有滚除的取证探针 | `stopgap landed` / `L1` | `P-ALL-04`、`SD-25`、`T8`；**三档归类规则已由 `PD-10` 裁决**（用后即滚 / 转正式机制 / 生产守卫） | 残余分布在 `os/kernel/src/*.rs`、`os/kernel-image/src/main.rs`、`os/libs/minix-rt/src/*`、`os/arch/src/*/smp.rs` | `grep -rn "用后即滚" os --include=*.rs \| wc -l` → **15 行 / 7 个文件**；`grep -rnE "nk4[ac]:" os --include=*.rs \| wc -l` → 15 行 | 按 `PD-10` 的三档逐站归类并滚除；污染案相关探针须等 `PD-16` 的结案条件。**注意：齐平清单记的"约 160 处 / 29 文件"是滚除大轮之前的读数，已过期** |
| 大量用户态服务与驱动的入口仍停在空转占位，生产传输未通电 | `open` / `L1` | `P-ALL-07`、`SD-29`（三架构同等欠缺，属功能移植进度而非齐平差距） | 各 `main.rs`、`os/servers/is/` 的占位传输 | `grep -rln "loop {}" os --include=main.rs \| wc -l` → **55 个入口**；`grep -rn "UnimplementedTransport" os --include=*.rs \| wc -l` → 5（集中在输入服务） | 逐服务接线，见 §4 的传输族条目 |
| 语义与上游 C 真源不一致的残余：正错误码经调用结果通道上线、未知虚存控制请求返回码与 C 不同 | `partially fixed` / `L2`（正负错误码与参数字段宽度两处已闭） | `P-ALL-05`、`SD-43`、评审编号 `F10`、`X-7`；**裁决 `PD-12` 采甲：本轮逐条对照 C 真源修尽，有意偏离必须三处一致标注** | `os/servers/pm/src/calls.rs`、`os/kernel/src/syscall.rs`、`os/kernel/src/arch/*/boot.rs` | 逐行读 `os/` 与 `minix3/` 对位函数比对，禁止用 grep 定案 | 需要对 C 真源的逐条判读，属人工裁决面（裁决已给方向，结论需逐臂对账） |
| 用户态服务的框架语义补齐：信号代理循环、按号分派的消费方盘点等 | `partially fixed` / `L2` | `P-ALL-08`（子项 `T1`—`T7`） | `os/libs/minix-sef/src/lib.rs` | `wc -l os/libs/minix-sef/src/lib.rs` → 千行级框架核已就位 | 子项进度以齐平清单的落地段为准；逃生门缺失导致的死锁已在案 |
| 内核包的默认特性仍带替身特性，宿主与真机形态可能不是同一个二进制 | `open` / `L1` | `P-ALL-10`、`SD-30`、`SD-31`；**裁决 `PD-29` 采甲：从默认里摘掉**，硬序是先建持续集成双腿（`PD-30`）再摘默认 | `os/kernel/Cargo.toml` | `grep -n "^default" os/kernel/Cargo.toml` → `default = ["mock"]` | 先把构建特性分裂导致的崩溃查清，再按 `PD-29` 的次序摘除默认替身特性 |
| 启动入口的多种形态并存，断言电池未补齐 | `open` / `L2` | `P-ALL-11`、`SD-1`—`SD-4`（`SD-2` 已裁决 `PD-07`） | 各架构链接脚本与 `os/kernel/src/arch/*` | 读 `STRUCTURAL-DEBT-REGISTER-20261008.md` §5.1 对应四行的下一步动作 | 补齐"三架构入口形态一致性"的断言集，并清扫地址常量残留 |

## §3 结构债中尚未闭合、且未被上面两节覆盖的条目

描述与判据一律看 `STRUCTURAL-DEBT-REGISTER-20261008.md`，此处只登记"仍未闭合"这一事实，不复制论证。

| 条目 | 状态 | 一句话 |
|---|---|---|
| `SD-11` | `open` | 虚存服务在大块申请上的失败会复现，需要执行与跟踪分离的修法 |
| `SD-13` | `partially fixed` | 大部分已闭，剩余面需要给出对应的齐平清单行号 |
| `SD-15` | `open` | 陷入帧与处理器上下文两套结构靠约定对齐，缺撕裂寄存器的校验（低风险） |
| `SD-27` | `open` | 从账本移出后债务仍在，需要批量纠正并加工具化守卫 |
| `SD-36` | `open` | 内存污染案卷的第三面，绑定在物理地址边界校验上 |
| `SD-38` | `open` | 设备树消费者与固件路径的分叉尚未收口 |
| `SD-39` | `open` | 信号集位宽拓宽（六十四位到一百二十八位）成批待做 |
| `SD-40` | `open` | 被门控的诊断面需要结案批次命名 |
| `SD-43` | `open` | 见 §2 的语义对真源条目 |
| `SD-12` | `adjudicated`，执行未竟 | 运行期进程根的写执行互斥收紧已由 `PD-22` 裁决为三步；第一步文本段去写需要静态核验无运行期写入者 |
| `SD-19` | `dropped-from-ledgers`，执行未竟 | 网络字节序与套接字事件的两处手抄按 `PD-13` 归共享类型库单点权威；从账本移出后债务仍在 |
| 裁决已给但尚未动手的四个小项 | `adjudicated`，执行未竟 | 按 `PD-04` 删除零调用的缺页地址取值器；按 `PD-26` 删除设备管理器从不被回调的自有信号钩子；按 `PD-27` 以被中断形态补上取消逃生门（发方链通电的硬前置）；按 `PD-23` 上移进程管理调用号枚举 |

## §4 跨阶段联调条目（`edge_todo.md` 里仍未闭合的部分）

描述权威在 `rewrite-notes/coordination/edge_todo.md`（该文件被十七个内核与服务端源码按名字引用，因此原地保留）。

| 条目 | 状态 | 差什么 |
|---|---|---|
| `E5` 端到端联调测试包（含 `a`—`h` 各子面：挂根、缺页完整回路、多服务器参战、信号、虚存控制、数据存储、系统信息、设备管理生命周期） | `open` / `待验证` | 需要三架构真机各服务参战；这是验收阶梯 T2—T5 的判据本体 |
| `E1` 用户态陷入腿的完整收口 | `partially fixed` / `待验证` | 直通陷入传输已真发中断与系统调用，收口与同场联调待真机 |
| `E6`、`E7` 进程管理的调用包装与协议面扩充 | `open` | 少数调用号与载荷成员仍缺；信号集位宽拓宽成批挂在 `E7` |
| `E8` 调度服务的真实通电 | `partially fixed` / `待验证` | 同场测试里取频结果为 0，需设备管理覆盖修复后复跑 |
| `E9` 根服务的生产接线面（进程、虚存、调度三域接口） | `open` | 三域同型批未做，占位返回码是刻意的失败即止 |
| `E-RSWIRE`、`E-KERNINFO`、`E-MINTYPES-RS` | `partially fixed` | 虚存侧已真解码根服务表，其余解码与运行时查询桩待翻转 |
| `E-VMTLB` 目标进程地址空间标识刷新 | `partially fixed` / `待验证` | 机制三件套已闭，多核验收挂 `E5` |
| `E-PREEMPTFLAG` 可抢占标志的生产可达性 | `partially fixed` | 消费腿已接，调用方全传空值导致分支不可达的余件仍在 |
| `E-DSWIRE`、`E-ISWIRE`、`E-ISPROD`、`E-ISBOOT`、`E-RMIBWIRE`、`E-MIBPROD`、`E-DMWIRE`、`E-INWIRE`、`E-NETSTART`、`E-INITSYS`、`E-FSBDEV`、`E-FSRUNTIME`、`E-FSVMCACHE`、`E-FSCMDS`、`E-SDEVOWN`、`E-DEVWIRE`、`E-DMABUF` | `open` 或 `partially fixed` | 服务传输通电族与文件系统块层接缝；逐个的缺件清单在原账本，未通电的共同原因是 `E1` 收口与 `E5` 端到端 |

## §5 并行编排仍在制（`new_edge4.md` 认领板与待裁决队列）

| 条目 | 状态 | 内容 |
|---|---|---|
| `C-17` 设备管理客户端的生产传输 | `open` | 授权、撤销、收发三条腿的生产实现 |
| `C-18` 虚拟文件系统把用户缓冲交给文件服务器的魔法授权 | `open` | 与直接授权同纪律的新入口 |
| `C-21` 文件系统运行时 crate | `partially fixed` | 家族已收敛，装配待 |
| `C-22` 系统信息取表半的传输面扩展 | `partially fixed` | 剩余段在内核行的生产半 |
| `C-23` 终端属性传输权威与终端控制请求号 | `open` | 真机往返验证挂 `E5` |
| `C-24` 网络栈外部依赖引入 | `open` | 离线策略已盘点，落 `17-stage-net` |
| `C-26` 简单文件系统的语义核心入库 | `open` | 属性、表、服务端三段 |
| `C-29`（已改编号 `NK1`）全系统自举载体的双镜像交换 | `open` | 撞引导栈窗口，采"每进程独立根"方案，设计轮已裁 |
| `C-35` 三个已领取条目没有专属工作树 | `partially fixed` | 领取锁机制的补树 |
| `C-46` 挂载与卸载的剩余一批 | `partially fixed` | 第一周目待 |
| `C-57` 命令冒烟门 | `open` / `待验证` | 随第十二批落地 |
| 三条旧待裁决项的现在归属 | `open`（仅余一条真待决） | 端点编码演进**已由 `PD-18` 收口**（维持现状、终局不预裁）；进程管理枚举上移**已由 `PD-23` 采甲**（转为执行项）；只剩"规则集维护批"（旧 `OQ-8`）仍等批量维护会话。新一轮的 `OQ-N1`—`OQ-N6` 已全部裁决完毕，结论在 `PENDING-DECISIONS-3ARCH-PARITY.md` 与 `new_edge4.md` §6 |

## §6 十八份阶段待办索引（本台账不复制条目，只给状态与跳转）

各阶段条目的描述与修法权威仍在 `{阶段}/todo.md`；下表的状态计数是本轮对照代码核实后的结果，抽样口径见 `TODO-LEDGER-INDEX.md` §5。

| 阶段 | 待办文件 | 仍未闭合（核实后） | 最关键的两条 |
|---|---|---|---|
| 01 内核 | [`todo.md`](../01-stage-kernel/todo.md) | 见 §1 与 `STAGE-KERNEL-FREEZE-READY-20261008.md` | 浮点归属、次级核承载 |
| 02 虚存 | [`todo.md`](../02-stage-vm/todo.md) | `open` 2、`stopgap landed` 3 | 实时更新的活-重启状态机；启动进程的可执行文件装载链 |
| 03 根服务 | [`todo.md`](../03-stage-rs/todo.md) | `open` 2 | 二十四个符号的生产接线面；根服务自升级 |
| 04 进程管理 | [`todo.md`](../04-stage-pm/todo.md) | `open` 3 | 核心转储路径契约；调度服务缺席导致的假端点 |
| 05 虚拟文件系统 | [`todo.md`](../05-stage-vfs/todo.md) | `open` 1 | 六十四个请求臂的执行绑定层缺穷举分派（真缺口） |
| 06 调度 | [`todo.md`](../06-stage-sched/todo.md) | 阶段内 0 | 残余全压在跨阶段 `E5` 与调度服务缺席边界 |
| 07 数据存储 | [`todo.md`](../07-stage-ds/todo.md) | `open` 2 | 标签发布与编码的纯逻辑已备，无生产调用方 |
| 08 中断服务 | [`todo.md`](../08-stage-is/todo.md) | 阶段内 0 | 残余为跨阶段传输族 |
| 09 初始化 | [`todo.md`](../09-stage-init/todo.md) | `open` 1 | 崩溃处理器的提供形式（归编排 `C-21`） |
| 10 系统信息 | [`todo.md`](../10-stage-mib/todo.md) | `open` 1 | 交换格式的应用二进制接口待裁决 |
| 11 设备管理 | [`todo.md`](../11-stage-devman/todo.md) | `open` 1 | 设备拥有权与生产四缺 |
| 12 输入 | [`todo.md`](../12-stage-input/todo.md) | `open` 1 | 边界测试族缺口 |
| 13 进程间通信 | [`todo.md`](../13-stage-ipc/todo.md) | 阶段内 0 | 残余为 `E-IPCWIRE` 的装配面 |
| 14 运行时 | [`todo.md`](../14-stage-runtime/todo.md) | `open` 1 + 真机 2 | 内核信息魔术失配的错误码选型（需对 C 行为裁决） |
| 15 文件系统 | [`todo.md`](../15-stage-fs/todo.md) | `partially fixed` 2 | 八个文件服务器入口未接事件循环 |
| 16 驱动 | [`todo.md`](../16-stage-drivers/todo.md) | `open` 2 + `stopgap landed` 3 | 二十六个纯占位 crate；两套字符驱动收敛 |
| 17 网络 | [`todo.md`](../17-stage-net/todo.md) | `open` 3 | 套接字标识基值缺 Rust 侧权威；协议栈本体 |
| 18 命令 | [`todo.md`](../18-stage-commands/todo.md) | `open` 2 + 占位 5 | 命令面尚未接真实系统调用返回 |

## §7 测试与门面对账（账面数字与代码实测不符，或判据本身有问题）

| 条目 | 状态 / 证据 | 内容 | 核实命令 |
|---|---|---|---|
| 文档记的测试数与代码实测数不一致的已知三处 | `open` / `L1` | 内核时钟章记 44 实测 47；网络实测远大于记数；调度服务文档内部自相矛盾（概览记 77、命令记 27、实测 27） | `grep -rc "#\[test\]" os/kernel/src/clock.rs`；`grep -rc "#\[test\]" os/servers/sched/src/` |
| 上游测试套件的建立证据没有钉校验和与构建提交，干净重建后引导腿会崩 | `open` / `待验证` | 评审 `H9` 项的后遗症：单采样门读数不可判读 | 见 `os/qemu-tests/test-atf-riscv64.sh` 与 `REVIEW-HISTORY.md` 的 `R3.3`—`R3.4` 段 |
| 真机测试脚本的收尾清理会删掉串口证据文件 | `open` / `L1` | 评审 `H7` 升级为第四次断链，本轮必修；现场仍在：`os/qemu-tests/test-atf-aarch64.sh` 的清理段 | `grep -n "rm\|cleanup" os/qemu-tests/test-atf-aarch64.sh` |
| 页表走表的错粒度检查被配置门挡住，模糊测试生成器一扩就炸 | `open` / `L2` | 评审 `J1` 项，落 `os/arch/src/riscv64_walk.rs` | 读该文件对应行的 `#[cfg]` 与测试生成器口径 |
| 地址常量清扫的十三条发现：修复轮已落注记，残余需要按工具复扫逐条判读 | `待验证` / `L1`（工具在，读数未判） | 出处 `ADDRESS-CONSTANT-AUDIT.md` 的 `A1`—`A13` 与修复轮注记 | `python3 tools/address-constant-scan.py os`（输出需按该审计的豁免清单过滤，否则会把测试夹具当违规） |
| 跨架构常驻编译检查门未落地：同一类事故（公共路径的取证改动打断另两架构构建）已发生两次 | `open` / `L1`；**裁决 `PD-30` 采甲**（加任务：宿主默认形态测试 + 生产形态交叉编译两条腿），`PD-31` 定了静态检查的棘轮口径；现状是工作流只为两架构建生产形态 | 出处回归评审的 `OQ-1`；按 `PD-30` 加任务也是 `PD-29` 摘默认替身特性的前置 | `grep -n "cross-arch\|aarch64\|riscv64" .github/workflows/*.yml` |
| 次级核定时器载体的真机断言缺失（见 §1 同条目） | `open` / `待验证` | 靠人读串口数字，不能进回归 | `grep -n "assert\|TEST_RESULT" os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-aps/src/main.rs` |
| x86_64 架构初始化文件零测试 | `open` / `L1` | `grep -c "#\[test\]" os/arch/src/x86_64/arch_init.rs` → 0 | 与 §2 的门测可达化同批 |

## §8 滚除、卫生与文档重生成前置

| 条目 | 状态 / 证据 | 内容 | 核实命令 |
|---|---|---|---|
| 探针残骸块（体只剩计数与引入） | `stopgap landed` / `L1` | 见 §2 探针条目，`L1` 读数 15 行 | `grep -rn "用后即滚" os --include=*.rs` |
| 过期注释：riscv64 次级核入口文件头部仍自称骨架，实为已完成体 | `open` / `L2` | 会误导下一轮扫描做出"未实现"的假阴性 | `sed -n '30,40p' os/arch/src/riscv64/ap_early_entry.rs` |
| 过期注释：进程间通信里"保存的 rbx"表述已被寄存器迁移取代 | `open` / `L2` | `SD-14` 已闭，注释未滚 | `grep -n "saved RBX" os/kernel/src/ipc.rs` |
| 无生产调用方的调试设施 | `open` / `L2`（消费者归属需人定） | 调试与转储函数只被集成测试调用，文档把它当内核设施描述 | `grep -rn "runqueues_ok" os --include=*.rs \| grep -v "os/kernel/src/debug.rs"` → 命中全在测试内 |
| 消息环的字符写入口没有喂入者（内核打印未注入环） | `open` / `L1` | `grep -rn "console_write_str" os --include=*.rs` → 只有定义 | 要么接线，要么在文档写明"只由系统调用侧喂" |
| 未被使用的分发死码 | `open` / `L1` | `grep -rn "dispatch_unused" os --include=*.rs` → 定义加测试，无分派引用 | 删除或写明保留理由 |
| 锚点底账里的行号大面积漂移 | `open` / `L1` | 工具派生行号提示在 `01-stage-kernel` 的复刻块里大面积失配，符号名不漂移 | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/16-smp.md` |
| 已删文件的引用残留在可再生的基线账目里 | `open` / `L1` | `tools/anchor-suspect-baseline-c.txt` 仍列本轮清理掉的案卷路径。该文件由另一条线程正在修改，本轮**不代跑再生**，交回锚点线 | `grep -c "coordination/" tools/anchor-suspect-baseline-c.txt` |
| 阶段正式文档的数量口径不一致 | `open` / `L1` | `rewrite-notes/README.md` 记 38，本轮按编号文件实测 38，其中 `06-todo.md`、`07-paging_init_gpt.md`、`18-trap-bridge-design.md` 属待办与设计稿，正式文档为 35 篇 | `ls rewrite-notes/01-stage-kernel/[0-9]*.md \| wc -l` |
