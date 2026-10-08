> **创建**: `2026-10-08`
> **本台账是什么**: 已完成、已裁决、以及**判定为正当差异**的待办存证。它与 `TODO-LEDGER-OPEN.md` 分册存放：未完成的是"要干的活"，已完成的是"别再重查的凭据"。
> **核实现场基线**: 代码读数在快照 `87784ca04` 上重跑。`closed` 一节只收**能给出代码锚点、守护测试或提交号**的条目；给不出的全部退回 `TODO-LEDGER-OPEN.md` 并标 `待验证`，宁缺不伪。
> **状态词表与证据级定义**: 见 `TODO-LEDGER-OPEN.md` 头部。

# 已完成、已定案与正当差异总账

## §1 已完成（代码坐实）

这一节的价值不只是"记功"，而是**回勾账面滞后**：本项目的待办清单里，"文档说没做、代码其实已经做了"的条目数量远大于反向情形。回勾之后，`01-stage-kernel` 的文档重生成才不会把已完成的功能写成缺口。

| 条目 | 状态 | 原始 ID 与账面滞后情况 | 代码锚点 | 核实命令与读数 |
|---|---|---|---|---|
| aarch64 与 riscv64 的次级核早期入口汇编体不再是骨架 | `closed` / `L2` | `P-A64RV-01`（清单称 41 行与 40 行骨架，**账面滞后**）；对应 `SD-34` 的入口面 | `os/arch/src/arm64/ap_early_entry.rs`、`os/arch/src/riscv64/ap_early_entry.rs` | `wc -l os/arch/src/arm64/ap_early_entry.rs os/arch/src/riscv64/ap_early_entry.rs os/arch/src/x86_64/ap_early_entry.rs` → `226 / 193 / 533`，两文件均含真实的装载控制寄存器与开地址翻译序列。宿主钉住测试也在位：`os/arch/tests/riscv64_ap_early_entry_pin.rs` |
| aarch64 中间页表页的回收（进程地址空间销毁时释放中间层页表帧） | `closed` / `L2` | `P-A64-01`、`V11-P2-8`（记为 riscv 与 x86 已完成、aarch64 缺，**账面滞后**） | `os/arch/src/arm64/paging.rs:fn free_child_tables`（被销毁根的那条腿调用） | `grep -n "free_child_tables" os/arch/src/arm64/paging.rs` → 递归定义与销毁调用俱在；编码位钉测 `os/arch/tests/paging_encoding_pins.rs` |
| aarch64 浮点与向量状态跨陷入的保存与恢复 | `closed` / `L2` | `P-A64-03`、`SD-23`（记为"空操作，向量寄存器从不保存"，**账面滞后，已由裁决 `PD-05` 追认结案**）；余留上机背书见 OPEN 案卷条目 | `os/arch/src/arm64/fpu.rs`（`CPACR_EL1.FPEN` 门控、保存/恢复全套） | `grep -n "FPEN" os/arch/src/arm64/fpu.rs` → 门控语义在注释与实现里成段；裁决句"懒轮转 + 完整浮点状态已在码"落在 `PENDING-DECISIONS-3ARCH-PARITY.md` 的 `PD-05` |
| aarch64 缺宿主可发现的陷入腿与返回腿指令顺序钉住测试 | `closed` / `L1` | `P-A64-02`（清单称"宿主测试目录只有 riscv 两枚，aarch64 零兜底"，**账面滞后**） | `os/arch/tests/arm64_return_leg_pin.rs`、`os/arch/tests/arm64_trap_leg_shape.rs` | `ls os/arch/tests/ \| grep arm64` → 两枚专属钉住文件在位；目录内另有四份审计（引导异常、信号上下文、小文件、门内清单） |
| x86_64 的进程间通信状态车道不再占用被调用者保存寄存器 | `closed` / `L2` | `P-X86-05`、`SD-14`、裁决 `PD-17`（追认甲案已在代码落地；**账面滞后**，但残留一条过期注释已登记在 OPEN §8） | `os/kernel/src/ipc.rs:fn ipc_status_register` 与 `os/arch/src/x86_64/signal.rs` 的 R10 车道 | `grep -rn "ipc_status_register" os/kernel/src/ipc.rs` → 存在并被使用；裁决句在 `PD-17` |
| 引导阶段早期串口的端口读取判断与有界轮询 | `closed` / `L2` | `P-X86-02`、`SD-5`（**账面滞后**） | `os/boot-shim/src/lib.rs:fn emit_byte` | `grep -n "0x3fd" os/boot-shim/src/lib.rs` → 读线路状态端口用 `0x3FD`（即基址加 5），有界轮询；文件内注释自陈这是对 `SD-5` 的勘误 |
| 内核镜像自行清除未初始化数据段 | `closed` / `L2` | `P-X86-03`、`SD-7`（**账面滞后**） | `os/kernel-image/src/main.rs`（x86_64 与 aarch64 两条臂，riscv64 同形） | `grep -n "__bss_start" os/kernel-image/src/main.rs` → x86_64 取址、aarch64 高地址装载两处在位 |
| 消息体尺寸不变量首次为真 | `closed` / `L1` | `P-ALL-12`、`SD-35`、裁决 `PD-20`（断言链三层：成员五十六、联合体五十六、整消息六十四） | `os/libs/minix-types/src/ipc/message.rs:const` 断言 | `grep -n "size_of::<Message>()" os/libs/minix-types/src/ipc/message.rs` → `assert_eq!(core::mem::size_of::<Message>(), 64)` 在位 |
| 测试入口把构建失败吞成绿色 | `closed` / `L2` | `P-ALL-09`、`SD-28`（结构债台账已标闭并点名齐平清单滞后一档） | `os/xtask` 的测试入口 | 读 `STRUCTURAL-DEBT-REGISTER-20261008.md` 的 `SD-28` 行；本项属工具面而非内核语义 |
| 内核主循环不再是占位、计费指针联动已闭环 | `closed` / `L2` | `I-6`（`01-stage-kernel/todo.md` 的 §7.4 仍写"调度循环仍为占位"，而同一文件后段已写"全条闭合"，**文内自相矛盾**，以代码为准） | `os/kernel/src/lib.rs:fn scheduler_loop`、`os/kernel/src/lib.rs:fn pick_and_bill` | `grep -n "fn scheduler_loop" os/kernel/src/lib.rs` → 生产实现存在；调用方含 `os/kernel/src/trap_dispatch.rs` 多处与 `os/kernel/src/smp.rs` 的次级核尾部 |
| 次级核启动的编排锁与 `smp_init` 全流程 | `closed` / `L2` | `D-36`、`D-37`（`01-stage-kernel/todo.md` 的 §7.1 与 §10 仍列开放并"阻塞于多核"，**账面滞后**；`edge1.md` 的"已闭单勿领"已声明该表未刷新） | `os/kernel/src/smp.rs:fn smp_init`、同文件的启动锁段（`boot_lock`，对位 C `arch_smp.c:227`） | `grep -n "pub fn smp_init\|boot_lock" os/kernel/src/smp.rs` → 均在；`os/arch/src/x86_64/protection.rs:fn init_ap` 已是带断言的真实实现，不再是占位停止 |
| 异常入口与切换回用户态之间的大内核锁获取/释放 | `closed` / `L2` | `16-smp.md` 的三处推迟标记（`D-38①` 同源），**账面滞后** | `os/kernel/src/lib.rs`（进入早期获取，回装前释放）、`os/kernel/src/trap_dispatch.rs`（用户态起源即取锁，内核态起源按约定视作已持锁） | `grep -n "bkl_lock().transfer()\|bkl_unlock()" os/kernel/src/lib.rs` → 两站点在位；`sed -n '405,420p' os/kernel/src/trap_dispatch.rs` → 按用户态起源取锁 |
| 次级核本地定时器与量子耗尽检查的接线 | `closed`（断言缺失另计）/ `L2` | `K6`、`smp_todo.md` §28 | `os/arch/src/x86_64/clock.rs:fn init_local_timer`、`os/kernel/src/clock.rs:fn rearm_local_tick` | 见 `TODO-LEDGER-OPEN.md` §1 同名条目（该条只保留"真机载体无断言"的残余） |
| 简单字符设备信息的单一权威与虚存服务握手 | `closed` / `L2` | `SD-29` 的一处子面、`C-1` | `os/libs/minix-types/src/types/device.rs` 被虚拟文件系统消费 | 读 `new_edge4.md` §2 的 `C-1` 销账行（含提交号） |
| 跨阶段联调族中已闭的成员 | `closed` / `L2` | `E2`、`E4`、`E-BOOTFRAME`、`E-FORKMSG`、`E-VFSWIRE`、`E-REQWIRE`、`E-SCHEDNICED`、`E-MINTYPES-SYS`、`E-MIBGRANT`、`E-DMCLIENT`、`E-MINSYS-SCOPE`、`E-ISKMESS`、`E-CDRCONV`、`E-TTYEVENT`、`E-PCKBDREG`、`E-DEVWIRE`、`E-IPCWIRE`（账面还挂着进行中的状态行，正文已自宣"八缺全闭合"，属**表头与正文不同步**） | 见 `edge_todo.md` 各条目最新进度注记 | 逐条 `grep -n "<条目 ID>" rewrite-notes/coordination/edge_todo.md` 后读**最新**注记；表头普遍滞后一档 |
| 未被使用的分发死码 `dispatch_unused` 已删除 | `closed` / `L1` | OPEN §8 卫生项（原无编号；设计 `25-misc-unported.md` §4.6/§6 登记“保留待清理”；`2026-10-08 @ 56e1869ae` 翻转） | `os/kernel/src/misc.rs`（函数、测试、模块映射 `do_unused.c` 行、D2 旧注记已删；D2 改写对齐设计 §3.2） | `rg -n "dispatch_unused" os --include=*.rs` → 零命中；`cargo test -p minix-kernel --lib` → 829 通过 / 0 失败（基线 830 − 1 个被删测试）；`cargo check --workspace --tests` → 零错误 |
| 真机测试脚本收尾清理删串口证据（H7）已修 | `closed` / `L1` | OPEN §7（评审 H7，R3.5 本轮必修） | `os/qemu-tests/test-atf-aarch64.sh:archive_evidence/cleanup` | 全量门 `RESULT: PASS (36/36)`、退出码 0、归档 `serial.log` 36 行、WORK 已清、无 QEMU 残留（`2026-10-08 @ 987fc589e` 基线验证，落点见本轮提交）；riscv 侧为环境变量 opt-out 形态（`RS_ATF_WORK`）未顺手改；`concepts/qemu-environments.md` 与 `misc/` 无相关表述、`REVIEW-HISTORY.md` 系冻结存证，均无需联动 |
| riscv64 次级核入口文件头部过期自称骨架已改 Status 为 S-4 落地 | `closed` / `L1` | OPEN §8 卫生项（原 `open`/`L2`；`DONE.md` §1 的 `P-A64RV-01` 已证入口体非骨架，本轮只滚注释） | `os/arch/src/riscv64/ap_early_entry.rs:32-35` | `grep -rn "Skeleton" os/arch/src/riscv64/ap_early_entry.rs` → 0 命中；`cargo test -p minix-arch --test riscv64_ap_early_entry_pin` → 4 通过 / 0 失败（`2026-10-08 @ 613d04c40` 核实，落点见本轮提交） |
| 进程间通信 KernInfo 注释过期自称保存的 rbx 已改 R10 调用者保存车道 | `closed` / `L1` | OPEN §8 卫生项（`SD-14`/`PD-17` 已闭，本轮只滚注释；`trap_stub.rs:580` 同车道英文残留行与 `ps_strings` 另约定未动，留待后续） | `os/kernel/src/ipc.rs:104-109` | `grep -rn "saved RBX" os/kernel/src/ipc.rs` → 0 命中；`cargo test -p minix-kernel --lib` → 829 通过 / 0 失败（`2026-10-08 @ c1cfb82db` 核实，落点见本轮提交） |
| 状态寄存器读回函数过期英文段已删，中文 R10 段独留 | `closed` / `L1` | OPEN §8 卫生项（原无编号；上一轮 DONE 已记留待后续；本轮只删过期段，不增新断言） | `os/arch/src/x86_64/trap_stub.rs:577-583` | `grep -n "the IPC status register" os/arch/src/x86_64/trap_stub.rs` → 0 命中；`grep -n "saved RBX" os/arch/src/x86_64/trap_stub.rs` → 仅 `:951` 合法存活钉；`cargo test -p minix-arch` → 全绿 0 失败（`2026-10-08 @ a09b92e7a` 核实，落点见本轮提交） |
| x86_64 架构初始化文件已补 3 个单测 | `closed` / `L1` | OPEN §7 测试门（生产代码零触碰，只增 `#[cfg(test)]`；arm64/riscv64 同文件仍零测试，留待后续） | `os/arch/src/x86_64/arch_init.rs:66-99` | `grep -c "#\[test\]" os/arch/src/x86_64/arch_init.rs` → 3；`cargo test -p minix-arch --lib arch_init` → 3 通过 / 0 失败，全量 lib 252 通过 / 0 失败（`2026-10-08 @ 7fe6917f1` 核实，落点见本轮提交） |
| aarch64 架构初始化接线已钉宿主形状测试 | `closed` / `L1` | OPEN §8 卫生项（原无编号；门内模块宿主不可编译故用 `include_str!` 钉，生产代码零触碰；riscv64 同文件仍零测试，留待后续） | `os/arch/tests/arm64_arch_init_pin.rs` | `cargo test -p minix-arch --test arm64_arch_init_pin` → 3 通过 / 0 失败；`--test arch_gated_audit` → 2/2 通过；全量套件 0 失败（`2026-10-08 @ d85888054` 核实，落点见本轮提交） |
| 命令面分层边界的生产违例十四处已降为零（`minix-sys` 顶层补齐封装，命令不再命名 `ipc` 传输层与 `minix_types`） | `closed` / `L1` | 原 OPEN §5（`PD-32` 执行半、`NL7`，已搬入本行）；`new_edge4.md` §2 `C-73` 销账行 | `os/libs/minix-sys/src/lib.rs`（`errno`/`MinixKerninfo`/`KuserInfo` 再导出 + `user_stack_top`/`gettimeofday`/`sigaction`/`setitimer` 顶层封装）；`os/commands/bin/shell/src/bin/sh.rs`、`os/commands/sbin/init/src/{execve,host}.rs` | `bash tools/check-command-boundary.sh` → 0 violations（此前 14）；`cargo test -p minix-init -p minix-shell -p minix-sys`（docker `minix-ci:1.94 -m 2g -j 1`）→ 149 / 40 / 317 全绿 0 失败（新增 `test_user_stack_top_honest_eio_without_kernel`）；`cargo build --workspace` 零错；build/clippy/`cargo test --no-run` 告警数 34/40/48 与修改前逐字相等（无新增）；真机门不在本单射程（`user_stack_top` 的裸金属语义已由 `kernel_utils.c:40-49` 对位坐实，不需要新增真机载体）（`2026-10-09 @ 0570e31b4` 核实，落点见本轮提交） |

## §2 已裁决（人或既有设计已经替本轮做过选择）

裁决文本的权威在 `PENDING-DECISIONS-3ARCH-PARITY.md`（`PD-01`—`PD-34`，第十轮已定稿冻结）与 `new_edge4.md` §6 的 `OQ-N1`—`OQ-N6`。下表只登记"结论是什么、还剩什么属于执行"。把裁决误读成"已完成"或"不可动手"，是这个项目登记机制刻意要防的两类误读。

| 原始 ID | 裁决结论 | 执行状态 |
|---|---|---|
| `PD-01` | x86_64 **要**补进上游测试套件的上机面（甲） | 待执行；豁免方案被否，理由是"把三架构齐平偷换成两架构" |
| `PD-02` | 本轮**不**承诺完整多核调度（乙 + 子门）：允许"次级核启动 + 基础中断异常存活"作为独立基础设施门；任何撤钳子门的前置清单必含 `PD-19` 的投递腿完成屏障与跨核读对账 | 已生效，是 `TODO-LEDGER-OPEN.md` §1 钳位条目的裁决依据 |
| `PD-03` | 架构专属系统调用返回坏调用是**正当差异**，不为齐平表象发明上游没有的调用（甲） | 余下动作只是改正过期注释 |
| `PD-04` | 缺页地址登记为诊断记录，删除零调用的取值器，不回滚消费臂（修正后的定性） | **已执行（`2026-10-08`）**：`grep -rn "last_fault_addr" os --include=*.rs` → 0 命中（getter 已删，`os/kernel/src/page_fault.rs`；4 处测试断言改直读 `p_fault_addr` 字段，测试数不变）；诊断记录定性登记于 `os/kernel/src/proc.rs` 字段注释；`docker run -m 2g minix-ci:1.94 cargo test -p minix-kernel -j 1` → lib 829 通过 / 0 失败（`2026-10-08` @ 基线 c5d21dbfb 核实，落点见本轮提交） |
| `PD-06` | 三架构同等未完成的服务装配接线**不算**齐平差距，另账聚合登记（乙） | 已按此形态登记在 `TODO-LEDGER-OPEN.md` §2 |
| `PD-07` | 三种内核启动入口形态并存**不**统一、不迁移，改用"进入正式内核后的共同引导后置条件"做等价断言（乙） | 断言集待补，见 `P-ALL-11` |
| `PD-08` | 文档测试数与实测不符判**陈旧**而非虚报，按"指定包"口径现场重算并写明口径（丙） | 见 `TODO-LEDGER-OPEN.md` §7 |
| `PD-09` | 三架构进程出生链必须逐架构对账，对账表含"初始寄存器约定"与"陷入保存集"两个维度（乙） | 冻结的是"必须对账"的义务，不预裁对账结果 |
| `PD-10` | 结案后的诊断探针分三档处理：用后即滚、转正式机制、生产守卫（乙） | 三档规则已定，逐站归类属执行 |
| `PD-11` | 用户态线程模型与快进锁维持延期，每个规划周期显式重登记一次；出现第一个多线程消费者即立项（乙） | 延期生效中 |
| `PD-12` | 已登记的语义偏差本轮逐条对照 C 真源修掉，有意偏离必须三处一致标注（甲） | 部分完成，见 `P-ALL-05` |
| `PD-13` | 网络字节序与套接字事件类型的两份手抄收进共享类型库单点权威（甲） | 待执行 |
| `PD-14` | riscv64 的装机与交棒形态**就此定为终态**（甲），第三入口形态计入 `PD-07` 的形态账 | 已追认 |
| `PD-15` | **放弃** U-Boot 对照测试腿，不装包实测（丙）；对照格标"不支持的装载器路径" | 已裁。`01-stage-kernel/todo.md` 里长期挂"保持推迟"的 `T-10` 依此关闭 |
| `PD-16` | riscv64 内存污染**继续追缴**写者，不放行；止损线是覆盖完成度的三项机器判据加固定预算（甲） | 追缴中，见 OPEN §1 |
| `PD-17` | x86_64 进程间通信状态车道迁到调用者保存寄存器（甲）**已被代码实施**，本条降为追认结案 | 已闭，仅余台账刷新 |
| `PD-18` | 端点编码维持与 C 逐位一致的现状，**终局形态不预裁**，唯一触发条件是十问清单出现"已挡住当前实际需求" | 冻结生效中。旧待裁决项 `OQ-1` 由此消化 |
| `PD-19` | 跨内核与用户边界的三层屏障要分清：编译器物化不等于完成可见性、不等于跨核同步（甲）；用户侧腿已全部落地，投递腿今天同核无需动作，但撤钳前必须作为验收前置 | 已追认 |
| `PD-20` | 六个消息联合体成员超宽按严格相等修回并加编译期断言（甲） | 已闭（见 §1） |
| `PD-21` | 内核读写用户内存采两层规范：软件访问规范为第一层（消息面已落地），硬件位与窗口守卫为第二层，入安全加固批 | 第二层待执行；这解释了为什么生产代码里没有置位动作 |
| `PD-22` | 运行期进程根的内核映射实行写执行互斥，分三步（文本段去写、恒等映射去依赖并退出、可选拆分引导根）（甲） | 政策冻结，三步执行中；直接映射窗实测已是不可执行 |
| `PD-23` | 进程管理器的调用号枚举**上移**共享类型库，调用号与载荷同库同源（甲） | **已执行（`2026-10-08`）**：枚举落 `os/libs/minix-types/src/ipc/pm.rs:pub enum PmCall`（与 `Mess*` 载荷同库），`from_call_nr`/`call_nr` 与两枚钉值测试随迁；minix-types 39 个 `PM_*` 常量、`ipc/sysinfo.rs` 的 `PM_GETSYSINFO`、minix-sys 25 个 `PM_CALL_*` 全部改枚举判别值派生（手抄值不可能再漂移），pm 服务器 `misc.rs`/`time.rs` 15 个本地常量改 `pub use` 再导出（`minix_pm::misc::PM_*` 公共面不变），pm 服务器 `ipc/calls.rs` 只再导出与分发。三件套：`grep -n "pub enum PmCall" os/libs/minix-types/src/ipc/pm.rs` → 1 命中、`grep -rn "pub const PM_CALL_EXIT: i32 = [0-9]" os/libs/minix-sys/src/` → 0 命中；`cargo test -p minix-types/-p minix-sys/-p minix-pm` → 324 / 316 / 426+13 全通过 0 失败，`cargo check --workspace --tests` 零错误；clippy 三包 117=117 零新增（`2026-10-08` @ 基线 60bed40e8 + 本笔改动核实，落点见本轮提交）。旧待裁决项 `OQ-2` 由此消化 |
| `PD-24` | 服务框架的四类拦截（热更新等）本轮不建模，登记为演进项，启用前先补回归哨兵（乙） | 已裁 |
| `PD-25` | 共享信导语义与词汇表，但**不强制**统一特征类型；初始化族与根服务专属族不合并（第十轮拍板的收窄形态） | 已追认 |
| `PD-26` | 设备管理器那个从不被框架回调的自有信号钩子**删除**（C 真源无对应注册） | **已执行（§续-429，f8afd07bb，含前提修正）**：C 真源复核发现 devman 的 C main 走 `run_vtreefs`（main.c:88），libvtreefs 的 `sef_local_startup` 注册了 `got_signal`（vtreefs.c:59/:39-46）——冻结文本「未注册任何 handler」的前提被证伪；按用户实施口径「删一半接一半」：死方法 `SefHooks::on_signal` 与空壳删除（链从未接通的死面判断成立），`got_signal` 语义落 `MinixTransport::terminate` 锁存＋取消。三件套：`grep -n "fn on_signal" os/servers/devman/src/hooks.rs` → 0 命中；`cargo test -p minix-devman` → 90 通过 / 0 失败（`2026-10-08` @ 60bed40e8 核实）；偏离登记于执行提交节，冻结存证不回填 |
| `PD-27` | 补上取消逃生门时库层返回被中断形态，与 C 控制流同形（甲）；这是发方链通电的硬前置 | **已执行（§续-428，53da928d9）**：`SefCancel` 令牌＋`sef_receive_status` 两处检查（循环顶先于阻塞收、notify 臂回调后）以 `Err(EINTR)` 收口；uds/lwip/fs-rt/ipc/IS 五服务接线、15 调用点与脚本夹具迁移；「通电即死锁」红线解除，发方链通电余件仍挂 T2。三件套：`grep -n "pub struct SefCancel" os/libs/minix-sef/src/lib.rs` → :135 命中；`cargo test -p minix-sef` → 19+3 通过 / 0 失败（`2026-10-08` @ 60bed40e8 核实） |
| `PD-28` | 服务器堆终解接可增长页供给、通道选段扩展风格，引导期虚存服务自身保留静态池（甲的政策冻结版） | 首迁对象按四级固定排序机械选择 |
| `PD-29` | 内核包的替身特性**从默认里摘掉**，接受基线短期变红（甲） | 待执行，硬序是先有持续集成双腿再摘默认（双腿已落，`2026-10-08`，见 `PD-30` 行——③摘默认由此解锁） |
| `PD-30` | 持续集成**加**内核任务：宿主默认形态测试 + 生产形态交叉编译两条腿（甲） | **已执行（`2026-10-08`，落点见本轮提交）**：新增 `.github/workflows/kernel-tests.yml`（照 vm-tests 先例），两 job＝`host-default-form`（`cargo test -p minix-kernel`，默认 mock 形态：库 829+集成 6 枚首次进 CI）＋`x86_64-production-build`（`cargo build -p minix-kernel --target x86_64-unknown-none --no-default-features`，补齐 qemu-tests.yml `cross-arch-kernel-build` 门的三架构缺一）。三件套：`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/kernel-tests.yml'))"` → 解析通过、2 jobs；腿① docker `minix-ci:1.94 -m 2g -j 1` 实跑 → 库 829 通过/0 失败/8 忽略＋集成 3 通过/3 忽略（忽略项＝宿主真 COM1 保护的既有 ignore，非本轮新增），exit 0；腿② 原样命令实跑 → exit 0（77 条警告为存量基线，与另两架构生产腿同形态）。前置①（`SD-28` 吞码）此前已修（`os/qemu-tests/run_all.sh:25-30`）。`2026-10-08` @ `1bad22152`+本笔改动核实；硬序③（`PD-29`）与⑤（`PD-31` 棘轮）仍各自待执行 |
| `PD-31` | 严格模式静态检查采报告制加"新增必须为零"的棘轮，基线绑定工具链五元组（乙） | 待执行 |
| `PD-32` | 分层边界守卫豁免仅供测试编译的代码已落地；现存十四处均为生产违例，转为待办 | 裁决完成，待办在册 |
| `PD-33` | 跨架构抽象的四条设计公理写入设计宪法；采"契约形状先行、实现允许粗糙"；主守卫是类型依赖边界而非文本比对 | 已裁；两处合规事实登记为重构待办 |
| `PD-34` | 服务出生协议的统一落点统一的是**编排责任**，不是把六段实现塞进库；未建模槽位只登记不发明（甲为主、丙为前置） | 待执行 |
| `OQ-N1` | 命令全集口径以规划文件的实测三百二十八为准；另一路的二百四十七属子集口径差异，登记分歧不重测 | 已裁 |
| `OQ-N2` | 引导模块清单采"补齐对位 C 序的真十二项 + 缺件失败即止"，内核断言保持严格；放宽断言被否（会掩盖装配错误） | 已裁，解锁内核面条目 |
| `OQ-N3` | 控制台输出采"内核中转 + 串口先行"，通道与特权模型两轴分裁；驱动直持端口特权被否（C 的驱动从不持输入输出特权级） | 已裁 |
| `OQ-N4` | 运行时库的页供给通道采段扩展风格，内存映射通道留给装载与栈映射；"维持静态池加上限"被否 | 已裁 |
| `OQ-N5` | 边界守卫豁免测试代码并落地守卫脚本 | 已裁已落地 |
| `OQ-N6` | 自举载体的双镜像交换采"每进程独立根"原语；"加栈顶参数"的快案被否（段面互撞与零隔离是结构性缺陷） | 已裁，实施待领 |

## §3 正当差异与被排除项（**防重查**：下一轮不许把这些当缺口去"修平"）

出处 `TODO-3ARCH-PARITY-20261006.md` §三（该清单与待办清单同等重要）。这一节单独存在的理由：本项目多次出现过"把架构机制差异当成缺陷去补齐"的返工。

| 被判定为差异的事物 | 为什么不算缺口 | 可复核的凭据 |
|---|---|---|
| riscv64 没有高级配置与电源接口，走设备树 | 两者语义都是"告诉内核有多少内存、哪些中断源"，只是承载机制不同 | `os/libs/minix-platform/src/acpi.rs` 内 riscv 缺席属预期 |
| 三架构页表层级不同（riscv 三级、x86 四级、arm64 自身格式） | 层级与索引位是指令集规定，语义都是虚拟地址翻物理地址 | `os/arch/src/{riscv64,x86_64,arm64}/paging.rs` |
| 只有 x86_64 有独立的页表位文件 | 另两架构把位定义内联在分页文件里；组织不同，语义等价 | `grep -c "VALID\|V_R_W_X\|AP1" os/arch/src/*/paging.rs` |
| 只有 x86_64 有端口输入输出实现 | 端口输入输出是 x86 独有指令；顶层特征在另两架构实现为空操作、设备 I/O 系统调用返回坏调用 | `os/plat/src/port_io.rs` 的注释与 `os/plat/src/x86_64/port_io.rs` |
| aarch64 线程控制块头偏移要手动减、riscv64 由链接器折进重定位不用减 | 都在"让线程指针指向正确的线程局部存储基址" | `os/libs/minix-rt/src/crt0.rs` 与两架构的运行时腿 |
| 三处 x86 相关代码被自动比对误判为"x86 缺席" | 自动比对按目标三元组字面搜，而那三处用"x86-64"、"i386"、"16550 端口"指代 x86，行为都在 | `os/kernel/src/clock.rs` 的 `rdtsc` 与中断控制器抽象、`os/drivers/tty/tty/src/serial.rs` 的 `COM1_BASE` |
| 架构专属系统调用在 aarch64、riscv64 一律返回坏调用 | 那些调用在上游本就属于 i386；按真源为正确，不是待实现（裁决 `PD-03`） | `os/kernel/src/syscall.rs` 的当前架构分派与注释 |
| x86_64 不在持续集成的交叉构建任务里 | 运行器本身就是 x86_64，再为它交叉构建是冗余。与 `P-X86-01` 区分：那条讲的是上机测试面 | `.github/workflows/` 的交叉构建任务定义 |
| 三架构同等未完成的服务装配面 | 属功能移植进度，不是架构间差距，只在聚合条目里登记一次（裁决 `PD-06`） | `TODO-LEDGER-OPEN.md` §2 的聚合行 |
| riscv64 独有的直接映射旁路 | 为绕开某条虚存服务时序的特有机制，另两端不需要；有实现且语义等价 | `os/servers/vm/src/vmdm_bridge.rs` |
