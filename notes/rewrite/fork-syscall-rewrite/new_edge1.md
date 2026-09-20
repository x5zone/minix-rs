# new_edge1 — 内核 · 架构 · boot 线（新一轮三线并行之一）

> **定位**：T1 收尾 + T2/T5 内核面的**新一轮临时分工索引**（前轮 [edge1.md](edge1.md) 已于 2026-09-20 收线冻结，K 组全 ✅/🚫；本轮携带其遗留与 2026-09-20 五路扫描收敛的新条目）。条目权威描述在 [edge_todo.md](edge_todo.md)（2026-09-20 节）与 stage todo；本文件只做范围圈定、前置标注与状态记账。归档规则见 [new_edge4.md](new_edge4.md) §8。
>
> **所有权（本线可独占修改，清单外触碰走 new_edge4 §2 认领板）**：`os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/qemu-tests/`、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/`、`00-master-plan/`。
>
> **并发规则**：见 [new_edge4.md](new_edge4.md) §1（沿旧 edge4 §1 三条 + FIXLOG 增量纪律）。
> **领取规则**：开工任何条目前先 `tools/claim.sh claim <ID> <owner>` 领取——分支 `claim/<ID>-<owner>` 即排他锁（同名/同 ID 已存在即被领走，`list` 看全量，完成后 `release` 销账）；状态列同步标 🔄。文件头规则是提示，分支才是锁。

状态图例：☐ 未开工 ｜ 🔄 进行中 ｜ ⏸ 等待（注明等谁）｜ ✅ 完成（日期+commit）｜ 🚫 维持登记不排期

---

## NK 组条目（T1 收尾 + 多进程 boot）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| NK1 | C-29：boot loader 的 per-process 地址空间/栈放置（→ C-27 收口） | [edge4.md](edge4.md) C-29 ｜ [edge_todo.md](edge_todo.md) E-BOOTMODS 关联 | `os/arch/src/arch/boot.rs:471` 按全局 `user_sp()` 定栈 + `os/kernel/src/lib.rs:1112/1288/1372` 仅 VM 分支装载。真机实证：第二镜像撞栈窗口 `MappingFailed`。修法候选：①`load_vm_elf` 增 per-process `stack_high`（快，共享根段面互撞仍在）②per-process 根（C `arch_boot_proc` protect.c:388 对位，**设计裁决先行**→OQ-N6）。载体 test-sysboot + sysboot-rx/tx + 判定脚本已入库即插即跑 | OQ-N6 裁决 | ☐ |
| NK2 | S-6/S-7：用户态页故障/异常转发臂接线 | [edge_todo.md](edge_todo.md) E-3ARCHTRAP ｜ [01-stage-kernel/todo.md](01-stage-kernel/todo.md) 2026-09-20 节 | trap_dispatch.rs:196-251 用户源 outcome panic；`ForwardToVm(VmPagefaultIn)` 发送臂 + faulting endpoint 填充 + `ExceptionOutcome::Signal` 消费臂（exception_dispatcher.rs:220-243 分类已备）。VM 回路已真；E5(d) 载体 test-paging-faultloop 已入 run_all，真机 PASS 即闭 | 无 | ✅ 2026-09-20（zcode-glm，5037491ff——内核臂接线+vmctl 重入队修复；787 测试全绿；载体真机 PASS ×2；E5(d) 端到端余量挂 T2，见 .review/zcode/edge1/FIXLOG.md Fix #1） |
| NK3 | 三架构生产 trap 腿 + timer-irq 真机载体 | [edge_todo.md](edge_todo.md) E-3ARCHTRAP | riscv64 trap_vector 诊断桩（trap_entry.rs:66-78）→ 完整帧保存 + cause 分发 + 用户往返；arm64 下半 EL exc_bad_mode（:88-102）→ VBAR_EL1 全量 + SP_EL0 交换；arch 门面 install_trap_stubs/syscall_entry_va 实化（arch/lib.rs:262-263/:324-327）。配套 `test-timer-irq-{riscv64,aarch64}` 载体（notes/TODO.md L4/L5；CLINT/Generic Timer 驱动已写）。顺带修 lib.rs:247-255 过时注释（K9 已落 global_asm） | NK1 的载体经验可复用；彼此独立 | 🔄 2026-09-20（zcode_glm_1） |
| NK4 | E-BOOTMODS 内核半：12 模块契约 + TEMP-DEBUG 拔除 | [edge_todo.md](edge_todo.md) E-BOOTMODS | 内核是契约方：`assert_eq!(len, 12)`（lib.rs:1131-1137）+ 纯下标 `BOOT_MODULE_PROC_NRS[i]`（:1185-1194）。配合 OQ-N2 裁决（补齐清单 vs 放宽断言）；boot-shim 侧清单修正归本线（loader.rs 所有权在内）。随批拔 DIAGCTL 五处 TEMP-DEBUG（syscall.rs:2659/2662/2718/2733/2740——污染载体 PASS 标记） | OQ-N2 | ☐ |
| NK5 | X-2：workspace `--bins` 全量构建断裂修复 | [edge_todo.md](edge_todo.md) 缺陷批 X-2 | test-shutdown-riscv64 交叉目标泄漏（riscv64 符号 unresolved + duplicate panic_impl）打断 `cargo build --workspace --bins`；per-crate 不受影响。修法：测试内核 bin 的 target/feature 隔离（E5-ARCH 构建矩阵的前置） | 无 | 🔄 2026-09-20（qorder_1） |
| NK6 | X-7/X-8 + T5 内核残留（维持登记，不排期至 T5 波） | [edge_todo.md](edge_todo.md) 缺陷批 | X-7 未知 VMCTL 参数 ENOSYS vs C EINVAL（syscall.rs:2219，收敛或 [ARCH] 标注）；X-8 fork 父 FPU 卸载 no-op（proc.rs:1670）+ 非 BSP TSS sp0 占位（protection.rs:370）；riscv64 PMP entry0（notes/TODO.md） | T5 波 | 🚫→T5 |

## 携带的前轮遗留

- K12b/K10/K11/K9 全 ✅（前轮 edge1.md 状态列为准）；"已闭单勿领"清单照旧（E1/E2/E6/E9 wrapper、E-VMTLB 机制半、E-PREEMPTFLAG、K1-K20 全组）。
- notes/TODO.md 的 kernel 设计级清理 backlog（C-D-1~5）维持低优先登记。

## 本轮新登记（防遗忘 · 未开工/待验证，每条自带复核命令）

> 收敛规则见 [new_edge4.md](new_edge4.md) §1 规则 7 与 §8：本小节条目由 edge4 批量并回 edge_todo.md。复核命令绿 = 已闭可划掉，红 = 开工。

| 标记 | 发现 | 现状证据 | 复核命令（跑一次即知是否仍成立） | 归属 |
|---|---|---|---|---|
| NK5-A（X-2 范围更正） | 宿主 `--workspace --bins` 泄漏**不是**只 test-shutdown-riscv64，而是 `os/qemu-tests/test-kernels/` 整类裸机 bin（x86_64-uefi / x86_64-none user / aarch64 / riscv64）——它们 `no_main`+自带 panic_handler/global_allocator，宿主三元组下必崩；对自己真实 `--target` 则编译通过（riscv64 实测 Finished，rt-birth 仅缺脚本 env） | `cargo build -p test-shutdown-riscv64 --bins` → E0432/E0433；`-p test-memmap --bins` → duplicate panic_impl；`-p test-shutdown-riscv64 --target riscv64gc-unknown-none-elf` → Finished | 隔离落地后：`cargo build --workspace --bins` 不再命中任何 `test-kernels` 包（见 NK5 状态列 commit） | new_edge1（本条随 NK5 闭） |
| NK5-B（commands 宿主半待验） | commands/ 下用户命令 bin（fileops/shell/init…）是否也泄漏宿主 `--bins` **未测**（构建先停在 RS 未走到）。若同为 no_std 裸机用户镜像，可能需纳入同一 feature 隔离或另开条目 | 未采集 | `cargo build -p fileops -p shell --bins`（宿主）——红则登记新条目，绿则划掉 | new_edge1 |
| NK5-C（RS 拦路 bug · 跨线） | `cargo build --workspace --bins` 实际**先停在** servers/rs：`crate::boot::parse_rs_verbose` 路径应为 `minix_rs::boot::…`。edge3 今日 f5fcef73f 引入，非本线所有权，只登记不代改 | os/servers/rs/src/main.rs:39 | `cargo build -p minix-rs --bins`（宿主）——绿则 edge3 已自修，划掉 | **new_edge3**（已同步 [new_edge4.md](new_edge4.md) §2 C-34） |

## 本线在验收阶梯中的位置（全文见 [new_edge4.md](new_edge4.md) §7）

T1 收尾 = NK2/NK3；T2 载体半 = NK1/NK4；T5 = NK3 复跑 + NK6。解锁下游：NK1 → new_edge3 NS1（RS boot 链有真载体）；NK2 → 一切用户程序的内存故障安全网；NK3 → new_edge4 E5-ARCH。
