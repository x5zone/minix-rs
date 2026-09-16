# 跨 Stage Edge TODO（fork-syscall-rewrite）

> 来源：2026-09-06 V11 架构审查（02-stage-vm/todo.md §14）拆分出的跨 stage 条目。
> 2026-09-06 增补：04-stage-pm 架构审查（04-stage-pm/todo.md §9）拆分出的跨 stage 条目 E6-E7。
> 2026-09-06 增补：03-stage-rs 的 §18.10 E-11 生产接线面登记为 E9（KernelApi 五域面真实传输，E-2 拆分后的接线形态）。
> 2026-09-08 增补：02-stage-vm V12 轮（02-stage-vm/todo.md §17）登记 E-VMMCPWIRE（vmmcp wire 字宽必现截断）与 E-VMMOCK（G-V12-5 余件：minix-arch default features 收口），并在 E5 增补"缺页故障完整回路"验收面。
> 2026-09-09 增补：02-stage-vm V13 轮（02-stage-vm/todo.md §18）登记 E-VMTLB（kernel 侧目标进程 TLB 刷新机制缺失，真 SMP 正确性前提），并在 E-RSWIRE 增补 VM 侧三项落地要求（call_mask u64、RS_SET_PRIV 真掩码、五消息结构专属 wire struct）。
> 2026-09-09 增补：05-stage-vfs 第二轮架构审查（05-stage-vfs/todo.md §9）登记 E-REQWIRE（REQ_* VFS↔FS 共享契约双侧独立定义，含 FS_BASE 基址已分叉的事实），并在 E-VFSWIRE 增补 VFS 侧接收半要求与绝对值断言纪律。
> 2026-09-09 增补：06-stage-sched 第二轮架构审查（06-stage-sched/todo.md V2 §5）登记 E-SCHEDNICED / E-PREEMPTFLAG / E-SCHEDSMP / E-MINTYPES-SYS 四条，并在 E5 增补 (e) PM↔SCHED 联调验收面。
> 2026-09-14 增补：07-stage-ds 首轮架构审查（07-stage-ds/todo.md）登记 E-DSWIRE（minix-sys DS 客户端模块缺失 + DS 服务器 transport 通电 + 联调零覆盖，三缺一注册），并在 E-MINTYPES-SYS 增补 DS 段两件（SI_DATA_STORE / NOTIFY_MESSAGE 常量）、E5 增补 (f) DS 发布/订阅联调验收面。
> 2026-09-15 增补：10-stage-mib 首轮架构审查（10-stage-mib/todo.md §5）登记 E-RMIBWIRE（minix-sys rmib 客户端协议半整缺）、E-MIBPROD（MIB 快照 vs kernel/PM/VFS producer 布局对账）、E-MIBGRANT（kernel grant.rs 端点常量与 C 不符）三条，并增补 E-DSWIRE（mib_get_label 消费方）、E-ISWIRE（mib 为 minix-sef 第二消费方）、E5（(g) MIB/sysctl 联调验收面）、E-MINTYPES-SYS（DS 段现状更新）、E-MINSYS-HYGIENE（锚点复核）。
> 2026-09-15 增补：11-stage-devman 首轮架构审查（11-stage-devman/todo.md §6）登记 E-DMWIRE（devman 生产接线四缺：server transport + 请求分类器 + 装配半 + client/RS 侧生产传输）与 E-DMCLIENT（minix-devman-client 孤儿 crate 处置，涉 16-stage-drivers）两条，并增补 E-REQWIRE（devman 为 VTreeFS wire 第三消费方）、E-ISWIRE（devman 为 minix-sef 第三消费方）、E-DSWIRE（devman 客户端 init 的 DS label 查询消费方）、E5（(h) devman 生命周期联调验收面）。
> 2026-09-16 增补：13-stage-ipc 首轮架构审查（13-stage-ipc/todo.md）登记 E-IPCWIRE（ipc-server 生产面接线八缺：trap 桥/SEF 层/sys_datacopy/proceventmask/VM_SHM_UNMAP/clock/getepinfo 窄 helper 七件 minix-sys 面 + minix-types 的 semid_ds/shmid_ds 布局面）。
> 2026-09-16 增补：14-stage-runtime 首轮架构审查（14-stage-runtime/todo.md V1）登记 E-MINTYPES-RUNTIME（minix-types 布局单点权威收敛，99 篇定稿驱动）与 E-MINSYS-SCOPE（minix-sys 六个域外 stage 客户端模块的内聚性处置）两条，并在 E1 增补"首个 no_std minix-rt 二进制"通电验证面。
> 2026-09-16 增补：15-stage-fs 首轮架构审查（15-stage-fs/todo.md V1）登记 E-FSRUNTIME（8 个 fs server bin 的 SEF/RS 启动握手与运行时接线）、E-FSBDEV（minix-fs 块层与真实块驱动的接缝，涉 16-stage-drivers）、E-FSVMCACHE（二级缓存零拷贝页移交与旗标机，涉 02-stage-vm）、E-FSCMDS（fsck/mkfs 命令占位认领）四条。
> 定位：**跨 stage 边界条目的唯一入口**，后续单线程逐条执行，避免并发修改各 stage 的 todo.md 时发生冲突。
> Edge 判定规则（三类）：① 共享契约/基础设施层——minix-types 布局、minix-sys trap 层与 SYS_* wrapper、os/arch 的 pt_alloc；② 对方 stage 目录里的生产代码（如 kernel 侧填充 handoff 字段）；③ 多进程联调测试（QEMU 端到端）。
> stage 内生产代码（消费既有稳定契约，含 seam + mock 测试）**不属于** edge，在所属 stage 的 todo.md 内实施。
> 执行约定：一次一条；每条完成后在本文件标注状态与日期；涉及对应 stage 的条目同步回写其 todo.md（02-stage-vm 对应 V11 条目、04-stage-pm 对应 P/D 条目）。

> **2026-09-16 全量对账（三路 grep，同 2026-09-15 做法）**：本会话闭环并提交——Wave A~E 全部、E1 切片 1-4 + 切片 5 核心通电验证（test-user-trap 真机 mailbox 断言通过，TSS 描述符悬空/xchg 编码两真 bug 修复）、E-VFSWIRE 三步、E9 SysApi/PmApi/SchedApi-KERNEL 三分域、E-VMMCPWIRE 宽度对账余件、E4 register_free、E-IPCWIRE 第 8 项（SysV IPC 布局单点权威）。**依赖等待态登记（对端状态逐项核实）**：①E1 切片 5 完整收口/E8 同场/E5 完整联调——等 01-stage-kernel 用户态入口测试进主线、PM/FS 服务器参战（04/05-stage 轨道）；②E9 VmApi——等 02-stage-vm T12/T13 的 rs.rs 对端；③E9 SchedApi SCHED 分支——等 06-stage-sched 服务器；④E-DSWIRE 通电/E-ISWIRE(2)(3)/E-DMWIRE——等各服务器 transport 通电（E1 real-trap 门控已就位，随时可接）；⑤E-SCHEDSMP——kernel SMP 窗；⑥E-ISBOOT——RS 动态加载 + TTY staging。**新增登记的并行条目**（12/13/14-stage 各轮）：E-INWIRE/E-CDRCONV/E-TTYEVENT/E-PCKBDREG/E-IPCWIRE/E-MINTYPES-RUNTIME/E-MINSYS-SCOPE——其中 E-IPCWIRE 第 8 项本轮已闭环，其余属 12/14-stage 对端轨道或待其 stage 服务层开工。**本轮未发现新的可单方面推进条目遗漏**；E1 机制面真机验证的完成使"等 E1"类依赖从内核侧转为服务器侧。

> **对账复核（2026-09-16 晚，同日第二轮）**：①01-stage-kernel——S-7 行与 I-6 记账与上轮核查一致（I-6 改判"生产主循环真实可达"已入档），qemu-tests 无新用户态入口测试进主线；②14-stage-runtime 轨道活跃中（V1-P0~P3 系列提交，minix-sys 契约面重构由该 lane 主导）——E-MINTYPES-RUNTIME 的执行半归其所有，edge 侧不重复推进；③02-stage-vm/06-stage-sched 无新对端产出。**结论维持**：全部可单方面推进项已闭环，剩余条目按上述依赖等待态登记。
> 2026-09-15 清理 campaign：已闭单四项（§0 表、E3、E-RSSTART、E-VMMCPWIRE）迁入 [edge_todo_archive.md](./edge_todo_archive.md)；同日全条目三路 grep 对账，过期锚点与已闭合子句已在各条目以「复核(2026-09-15)」修正——执行每条前仍须按 fix-guard 重新读目标行核实，不依赖本文件锚点的时效性。
> campaign 执行序（2026-09-15 批准）：依赖波次——A 内核正确性快修（E-MIBGRANT/E-PREEMPTFLAG/E-SCHEDNICED/E-VMTLB/E-DMCLIENT）→ B 共享契约收敛（E-MINTYPES-SYS/E-REQWIRE/E-MINTYPES-RS/E-FORKMSG）→ C minix-sys 客户端库（E2 / E-DSWIRE 余件+E-MINSYS-HYGIENE 同轮 / E6 / E-RMIBWIRE 纯函数层 / E-ISWIRE(1)）→ D 布局对账与 wire 系统化（E-ISPROD+E-MIBPROD / E7 / E-ISKMESS / E-KERNINFO）→ E VM 大件（E-BOOTFRAME / E-RSWIRE VM 侧 / E-VMMOCK 余件）→ F E1 trap 桥（设计先行，整条在本 campaign 做）与通电族（E8/E9/E-VFSWIRE/E-DSWIRE 通电/E-ISWIRE(2)(3)/E-DMWIRE/E5/收尾批）。每条一次一个，走 todo-fix 三段式（讲明白 → 多方案对比 Linux/Redox/OS 理论 → 实施）+ fix-guard + 文档-代码同步 + 测试 5 维自查 + 回归 review 后 commit；新发现的跨 stage 条目一律追加进本文件，不并发改各 stage 的 todo.md。

---

## 0. 已闭单条目(归档指针)

以下四项已闭单,完整原文(含判定过程)迁入 [edge_todo_archive.md](./edge_todo_archive.md),本文件不再保留正文:

| 条目 | 闭单性质 | 闭单日期 |
|---|---|---|
| §0 02-stage-vm campaign 顺序表(T1–T36) | 全部完成 | 2026-09-07 |
| E3 VmBootHandoff 补 kernel text/data span | 完成 | 2026-09-08 |
| E-RSSTART rs_start_t 字节 ABI + copy_rs_start 解码 | 改判关单(接线转入 03-stage-rs §21) | 2026-09-07 |
| E-VMMCPWIRE vmmcp reply.addr 64 位化 | 完成(宽度对账余件转低优先扫描项) | 2026-09-08 |

---

## E1 minix-sys 用户态 trap 层落地（2026-09-07 阻塞精化：前置是 01-stage-kernel 的 trap 桥）

**问题**：minix-sys 的两个 transport 都是 `-EIO` stub——`DirectTrapTransport`（`os/libs/minix-sys/src/ipc.rs` 全部方法返回 `Err(TrapStatus(EIO))`）与 `DirectKernelCallTransport`（`os/libs/minix-sys/src/syscall.rs` 返回 `-EIO`）。全仓不存在任何用户态 trap 指令序列/入口。

**2026-09-07 阻塞精化（stale-premise 复查）**：原描述"kernel 侧接收端已就绪，缺的只是真实 trap 体"**不完整**。kernel 侧现状：
- **已就绪**：`dispatch_ipc_entry` / `kernel_call_dispatch` 的 C 级分派（syscall.rs，含 BKL/权限/死锁检测，测试完备）；IDT 门配置机制（`os/arch/src/x86_64/trap_entry.rs`：gate 33 DPL=3、SYSCALL MSR、`configure_ipc_entry/configure_syscall`）。
- **未落地**：① IDT handler 地址为占位 0——"后续 boot 阶段 `set_handler()` + `load()`"的阶段尚未存在（kernel/src/lib.rs `init_protection` 注释自证）；② **trap 桥不存在**：捕获用户寄存器 → 保存用户上下文到 KProcess → 定位当前进程 → 从用户内存拷贝消息 → 调用 `dispatch_ipc_entry`/`kernel_call_dispatch` 的入口 asm/Rust 胶水（`dispatch_ipc_entry` 生产调用方为零，仅测试）；③ `switch_to_user` 仅存在于文档引用（sched.rs §3.3），无实现——内核从不返回用户态。
- **推论**：用户侧 trap asm 的寄存器/clobber 约定（C i386 先例：eax=端点/ebx=消息指针/ecx=IPC 调用号/int $33——本树 libc 仅 arm+i386 变体，无 amd64 参照）**无法对齐一个尚未设计的内核桥**。按反 guess 纪律（E-RSWIRE 先例），用户侧实现须待 01-stage-kernel 落地 trap 桥设计（入口 stub + 寄存器约定 + 用户上下文布局）后再动。

> **进度（2026-09-16，🔄 设计文档落稿待评审，01-stage-kernel/18-trap-bridge-design.md）**：Step 1 约束穷举 + Step 2 方案对比完成，四项裁决待批——①入口选 int 33 门（已配 DPL3 trap gate，trap_entry.rs:291；SYSCALL 腿不动）；②寄存器约定选 C i386 拓 64 位（RAX=端点/RBX=消息指针/RCX=调用号，RDX=SENDA 表指针；errno 走 RAX、status 走已裁决的保存 RBX 通道）；③保存区选"一律入 CpuContext、出口只有调度循环"（用户态真值单点，阻塞成平凡情形；SYSCALL 腿的 TrapFrame 直返模型不推广）；④用户 trap 体落 minix-sys arch_trap.rs，minix-rt DirectTrapSource 桩随之翻活。落地切片 5 步（asm stub→出口合一→trap 体→minix-rt→通电）。**stale-premise 修正**：原文"switch_to_user 无实现"已过期——lib.rs:2828 起为活代码，SYSCALL 腿（trap_dispatch.rs:179）也在跑；真正欠的是 int-33 腿。**按评审门规则停在此处，待批准后进实现。**

> **切片 5 进度（2026-09-16，🔄 通电载体已建，邮箱断言待下一轮排除 init 缺页）**：新增 QEMU 测试内核 `test-user-trap`（f1f9f7fbb）——仿 test-smp-aps 完整生产 boot 链（UEFI/OVMF→paging→IDT→clock→proc_table→smp_init 四核在线），用户进程搭建全通：CPL3 代码/数据/栈三页（VA 0x1_0000_0000 起，走 identity 通道手写 PTE——X86_64Paging 的 KernelDm 窗口 bootstrap 内核未映射，直用即缺页，已实测）、51 字节 int 0x21 payload（call 99→EBADCALL/call 6→kerninfo 未发布 EBADCALL，mailbox 三断言值）、build_cpu_context(ProcKind::Vm)+FullContext+rts_unset 入队、switch_to_user 交权。实测已达"entering scheduler"交接点。**已知遗留**：init_proc_and_boot 存在内核态 vector-14 缺页（-smp 1 确定性复现、-smp 4 间歇、test-smp-aps 同源码通过）——需 rip→符号解析定位（下一轮）；邮箱断言（monitor xp）待排除后收口。运行脚本 `qemu-tests/test-user-trap.sh`（OVMF+startup.nsh+monitor xp）。

> **切片 5 收口（2026-09-16，✅ 核心通电验证通过，commit 6d8e52c3d）**：干净重建后全链路真机验证通过——用户态 CPL3 payload 两次 `int 0x21` 经向量 33 门进入内核臂（int 日志 v=21 ×2），round-1（未定义调用 99）返回 EBADCALL(209)、round-2（MINIX_KERNINFO 未发布）返回 EBADCALL，两值均由用户态 payload 写回 mailbox 物理页，monitor `xp` 断言 [0xd1, 0xdead] 落盘 ✓。**本轮定位的两个真 bug**：①payload 的 0xDEAD 标记存储误用 `4c 93`（xchg rax, r11）——opcode 93 的寄存器号走 REX.B 扩展（4C 的 B=0），实际语义 xchg rax, rbx，rbx 初值 0 被换入 rax，mailbox[+8] 恒 0；改直接 `mov rax, 0xDEAD`。②TSS 描述符悬空（fe1a835a9，见前注）。**收口状态**：E1 机制面（内核臂/二次返回通道/用户 trap 体/真机通电）全部验证；E8 SCHED 通电的调度器恢复路径（switch_to_user→restore_to_user）已由本测试同场覆盖其用户态入口半；与 E5 联调的完整链（PM/FS 服务器参战）仍挂各 stage 轨道。

> **identity 通道绕过 + hz 写入路径定界（2026-09-16，commit f06637f4b）**：init_proc_and_boot 的间歇性缺页根因 = identity PTE 无 USER 位 → CPL3 读取 fault；OR USER 进 PDP[0] 后 -smp 1 稳定到达 hand-off ✓。SYSCALL 腿 GET_HZ 的 [+0x10]=0 定界为两层：①KernelUserCopy 真读写已修 ✓（内核正确读到 GET_HZ 请求）；②`copy_struct_to_caller` 的 hz 写回走 `AddressRef::Process` + DM 窗口——bootstrap 内核 DM>4GB 区域未映射，写入未到达（01-stage-kernel DM 基建缺口，非 edge 层）。E8 余项 = DM 覆盖修复后 SCHEDCTL 通电 → 闭单。

> **E8 同场扩展（2026-09-16，🔄 SYSCALL 腿 GET_HZ 部分通过，commit 5fc602c96）**：payload 增加第三轮——SYSCALL 腿（LSTAR）发送 GetInfo GET_HZ：用户态预构建消息（m_type=26/request=18/val_ptr→数据页缓冲），`mov rdi, msg_va; syscall` 经真实内核调用路径。实测 mailbox[+0x10]（hz 值落点）= 0——GET_HZ 的内核侧 hz 写入未到达用户缓冲，可能原因：caller 权限（VM proc 是否有 priv_id）/ KernelUserCopy 的真机读取路径 / ClockState.system_hz 初始化状态。int-33 桥路核心断言（[+0]=0xd1/[+8]=0xdead）不受影响。**E8 余项**：GET_HZ 真机写入路径定位 → SCHEDCTL 通电 → E8 闭单。

> **E8 深层定位（2026-09-16，🔄 KernelUserCopy 真读写已修，copy_struct_to_caller DM 依赖待解，commit 337605551）**：GET_HZ 的 [+0x10]=0 根因分两层——①内核读用户消息：KernelUserCopy 原为空壳(恒返零消息)，已修为 volatile 真读/真写(CPL0 经共享页表直读用户 VA；cfg(test) 门控 hosted 安全)。②内核写 hz 到用户缓冲：`copy_struct_to_caller` 走 `AddressRef::Process` + `CurrentDirectMap::virt_to_phys` + caller CR3 的页表遍历 + DM 窗口写入——bootstrap 测试内核的 DM 窗口未映射 >4GB 物理地址(VA 0x1_0001_0200 在 identity(0-4GB) 与 kernel higher-half 之外的间隙)，写入未到达。**此为 AddressRef/DM 基础设施与 bootstrap root 的兼容性缺口**(属 01-stage-kernel 基建，非 edge 层)。**E8 同场已达成的验证项**：int-33 桥路双 trap ✓ / mailbox[+0]=209 ✓ / SYSCALL 腿进入 kernel_call 分派 ✓ / KernelUserCopy 真读写 ✓。**下一轮**：DM 窗口覆盖 bootstrap root 的 >4GB 区域(或改用 identity 通道写入)，即可收口 E8 的 hz 写回路径。
> **E8 hz 写回贯通（2026-09-16，✅ mailbox[+0x10]=100 真机读回，四处根因同轮修复，commit 5ea7b76a8）**：gdb+QEMU monitor 逐级定位（无符号 EFI 用指令字节串回匹配 + 探针重编译），共修四处——①**dm_coverage 下溢**（kernel/src/dm_coverage.rs）：`bootstrap_tree_candidates` 的前裁剪 `start = max(base, root_end)` 无条件执行，UEFI `MaxAddress` 降序分配使 root 页（0x0e789000）落在 bump 区（0x0dfaf000+64页）**之上**，`end − start` 回绕成巨型空转范围，bump 候选零叶子安装——所有运行期页表页不在内核 DM 窗口，walker 首次访问 #PF（CR2=DM(0x0dfb4ff8)）；修复=仅 root 落在 bump 内时修剪（带 start<end 守卫），不相交时全量覆盖，+回归测试 `test_union_keeps_full_bump_when_root_above_bump`。②**copy_struct_to_caller 误用 DM 窗口换算**（kernel/src/misc.rs:622）：`&hz` 是 higher-half 内核栈地址（0xffff8000_001fef20），不在任何 DM 窗口内，`virt_to_phys` 走 VM 窗口分支减出垃圾"物理"→非规范别名→rep movsb #GP；对照 C do_getinfo.c:209-217 的 SELF 语义（内核局部直读），新增 `vm::cross_space_write` + `cross_space::write_to_process_vmcheck`（仅用户目的地走 PTE 解析+DM 别名+VMSUSPEND 簿记），`copy_struct_to_caller` 改用它——同函数全部 GetInfo 臂一并受益。③**SYSCALL 退栈缺 ss 槽跳过**（arch/src/x86_64/trap_stub.rs）：exit 序列 `pop rsp` 吃进 SS 常量 0x23 作用户栈指针，payload 带垃圾 RSP 继续跑（终修见 sysret 锚点条）。④**身份映射 USER 位未达叶**（test-user-trap main.rs）：hz 缓冲 0x0400_1200 在 64MiB 叶 PD[32]（0x0400_1200>>21=32，此前误写 PD[2]=4-6MiB），2MiB 叶 OR USER + `invlpg`（叶已被 supervisor 访问缓存）。
> **E8 round-2 收口 + SYSRET +8/+16 锚点约定（2026-09-16，✅ test-user-trap.sh 正式脚本 PASS，commit 8891e52f9）**：round-2 int-33 iret #GP(0x28) 根因=**sysret 的 CS/SS 加载语义认知错误**——QEMU 8.2.2 `seg_helper.c helper_sysret`（与 AMD APM 一致，Linux STAR 值 `__USER32_CS=0x23`→CS=0x33/SS=0x2b 印证）：sysret64 实际加载 `SS ← (STAR[63:48] + 8) | 3`、`CS ← (STAR[63:48] + 16) | 3`，即 STAR[63:48] 是"锚点选择子"而非最终 CS。我方原写 STAR[63:48]=USER_CS(0x1b) → sysret 实取 CS=0x2b=GDT[5]=**TSS 描述符**（SS=0x23 恰好命中 user data 槽，故 SS 一直"正常"而 CS 坏——定位中被 SS 误导良久）。修复采用 Linux GDT 约定：`GDT[3]=锚点（用户码描述符副本，永不选取，仅占位）、GDT[4]=user DS(0x23 不变)、GDT[5]=user CS(0x2b)、TSS→[6]`，STAR[63:48]=锚点(0x1b 值不变、语义修正)，USER_CS_SELECTOR 常量 0x2b 全链生效。**顺带真修**：syscall stub 入口推栈顺序 [rsp][ss] 与硬件 int 帧/TrapFrame 布局契约（rsp@20/ss@21）相反（探针实锤 fss=0x100020000=用户 RSP 落进 ss 槽）——改为 [ss][rsp]，exit 去掉补偿跳槽。**正式验收**：test-user-trap.sh `RESULT: PASS`——round-1 int-33 EBADCALL ✓ / SYSCALL 腿 GET_HZ hz=100 写回读回 ✓ / round-2 int-33 KernInfo(未发布) EBADCALL ✓ / payload 到达终点自旋 ✓。TEMP-DEBUG 全部拔除（trap_dispatch [ipc]/[sys]/cr2+寄存器扩展、read_star_lstar/read_cs_ss、syscall kc-denied）。**回归**：kernel 759 / arch 237 / minix-sys 192 / minix-rt 46 / minix-vm 507 全绿，clippy 对账零新增。
> **进度（2026-09-16，🔄 切片 1-4 闭环，切片 5 通电挂 boot 链）**：设计经代评审批准（commit 60a92fed9——评审修正一处初稿 guess：SENDA 寄存器角色实为 C `SENDA_ARGS` 的 EAX=count/EBX=table 复用，无新寄存器）。**切片 1 内核臂**（b6571a458）：向量 33 从通用异常分派剥离（S-8 的 TRAPSTUB 33 门控早已在——E1 原文"asm 不存在"再次过期），新 `x86_ipc_dispatch_body`：入口持久化用户寄存器进 CpuContext → 抽取 RAX/RBX/RCX → 消息先拷贝后分派 → 应答码回 RAX + status 回 frame.rbx；arch 新增 `save_frame_to_context`（gp 索引顺序逐字段见证测试）。**切片 2 返回码回写**（e30674f0d）：引擎两处唤醒点（发送方完成阻塞 RECEIVE：proc.c:969 系；接收方取走阻塞 SEND：proc.c:1097 系）给被唤醒者保存上下文 RAX 写 OK——修复"恢复后读到陈旧 RAX"缺口；写入走 PTRACE 式 write_user_register(80)。**切片 3+4 用户侧**（e54846162）：minix-sys `arch_trap.rs`——int 0x21 体（RBX 为 LLVM 保留寄存器，按 cpu_identity.rs push/pop 先例）+ SYSCALL 腿 kernel-call 体；七个 transport 方法接真分支（receive 的 IpcStatus 取自 status 寄存器，kerninfo 页地址取自二次返回 RBX）；minix-rt `DirectTrapSource::query_kerninfo` 翻真。**门控裁决（实测教训）**：编译期无法区分"我们的内核"与"宿主 Linux"——真分支在 hosted 测试构建里实际执行了 `syscall` 且宿主应答 -ENOSYS——真分支收进显式 `real-trap` feature，仅真 boot image 构建开启。**切片 5 通电**：挂内核 boot 链到达用户态调度的进度（S-6/S-7 lane，01-stage-kernel 自有轨道）；机制面已全部就位，通电即 E8 SCHED 通电的同场验证项。**验证**：kernel 758 / arch 237 / minix-sys 155 / minix-rt 59 / minix-vm 505 全绿；clippy 对账零新增。

> **增补（2026-09-16，14-stage-runtime V1 轮）**：切片 5 通电族的验收面增加"首个 no_std minix-rt 二进制"——minix-rt 的 `_start`/`#[panic_handler]`/`real-trap` 当前均无仓内构建启用（35 个命令消费方全部默认 std feature），机制面已备但从未真机执行。通电时应验证三件：入口寄存器接收（crt0.S 的 rdx/rcx/rbx 约定 → argv 可达）、kerninfo 直读链（DirectTrapSource::query_kerninfo 首次真机走通）、panic hook 真渲染——后者有一处通电前必须先修的缺陷：minix-rt 的 panic handler 读的是自己 crate 内的重复注册表（minix-rt/src/lib.rs:333 与 ：381-400），kernel 注册写入的是 minix-types 注册表（kernel/src/lib.rs:1638），hook 永不命中，详见 14-stage-runtime/todo.md V1-P0-1/V1-P1-1/V1-P1-4。

> **kerninfo 页发布（2026-09-17，✅ 内核半闭环，commit a071af5e5）**：E-KERNINFO 挂起的"kerninfo 页用户映射初始化 + MINIX_KERNINFO_USER 发布"落地——新模块 `kernel/src/kerninfo.rs`：内核镜像静态页 `KerninfoPage`（4KiB 对齐，BklProtected write-once 审计）启动时填充 kuserinfo（kui_size/user_sp，main.c:438-440 对应），经活动根 `query()` 翻译 PA（首版 kern_phys/virt 算术在 UEFI 加载形态下溢出，真机实测纠正——活动根是 VA→PA 唯一事实源），`Paging::map` 用户只读映射固定 `KERNINFO_USER_VA=0x2_0000_0000`（高于 4GiB identity、低于 Sv39 用户上限、与 VmBootHandoff 不共 PD 项），指针/magic 收尾后原子发布。**同轮 arch 真修**：`walk_alloc` 中间层此前只写 PRESENT|WRITABLE 不传播 USER——U/S 逐层 AND，用户叶子 behind 新建中间层 CPL3 必 #PF(err=5)（首版真机 mailbox[+0x30]=0 即读 fault 杀进程的伪装）；修为从叶子 PTE 派生（Linux _PAGE_USER 同型），**VmBootHandoff 页同路径潜伏缺陷一并消除**。**真机验收**：test-user-trap round-2 从 EBADCALL 翻转为 OK + RBX=0x2_0000_0000 + CPL3 读到 KERNINFO_MAGIC=0xfc3b84bf，脚本断言 3→5 项 PASS；kernel 761（+2）/ arch 237 全绿，clippy 基线 57=57、7=7 对账零新增。**E1 余项收窄为**：minix-rt `query_kerninfo` 桩翻转 + panic hook 真渲染（14-stage-runtime V1-P0-1 注册表分裂已由其 lane 先行闭环，见 runtime STATE Fix #1）——即本条目与 E-KERNINFO 共同的用户态消费半。

**影响**：不变——全部用户态服务器的 IPC/kernel-call 真实通电挂本条；且本条真实前置是 **01-stage-kernel 的 trap 桥 + switch_to_user**（该 stage 的 V12 工作流进行中）。

**建议（更新后的执行序）**：
1. 01-stage-kernel：落地 trap 桥设计（x86_64 入口 asm：IDT gate 32/33 handler、用户上下文保存/恢复布局、switch_to_user）——寄存器约定在桥设计时定稿并文档化；
2. minix-sys（本条主体）：按定稿约定写用户侧 `int $0x21/0x20`（或 syscall）序列，`DirectTrapTransport`/`DirectKernelCallTransport` 换真实 trap 体（CannedTransport 测试路径不变）；
3. 回写 T9-T15 通电标注 + E5 联调包。

**解锁**：T9 / T10 / T11 / T13 / T14 / T15 的真实通电；E5 前置；E6/E8/E9 的传输半。

---

## E2 minix-sys SYS_* kernel-call 包装函数

**问题**：minix-sys 没有任何 `SYS_*` 内核调用包装（grep 零命中）——VM 侧需要的 SYS_FORK（`os/kernel/src/syscall_process.rs:122-215` 已真实）、SYS_UPDATE（`os/kernel/src/misc.rs:1635`，12 步全实现）、SYS_SAFECOPYFROM/TO（`os/kernel/src/syscall_copy.rs:367/:381`）、SYS_EXEC（`os/kernel/src/syscall_process.rs:231`）、SYS_DIAGCTL code 1 控制台输出（`os/kernel/src/syscall.rs:2212-2240`）都缺用户态入口。

**影响**：VM 的 KernelGateway seam（T9）只能以 mock 实现这些调用；fork/RS live-update/safecopy/exec/audit 的端到端链路缺最后一层。

**建议**：在 `os/libs/minix-sys/src/syscall.rs` 复用既有 `perform_kernel_call`（:201，ENOTREADY 重试）与 `KernelCallTransport`（:132-217）机制，为上述 6 类调用各加一个包装函数（签名对齐 kernel dispatch 的消息布局，message 构造用 minix-types 现有 union 成员）；每个包装带一个"CannedTransport 回放"单元测试。

**解锁**：T11 / T13（步骤 4）/ T14 / T15 的真实通电。

> **复核（2026-09-15）**：锚点更新——`perform_kernel_call` 现 :539（`KernelCallTransport` 区间基本未变）；minix-sys 现有 11 个 `sys_*` wrapper（kill/abort/times/sigsend/getksig/endksig/trace/runctl/resume/vircopy/clear），本条 VM 侧 6 类（fork/update/safecopyfrom/safecopyto/exec/diagctl）grep 零命中待建；minix-types `ipc/sysinfo.rs` 已有 SYS_GETINFO/SYS_DIAGCTL/SYS_SAFECOPYFROM/SYS_SAFECOPYTO 等常量可直接消费。

> **进度（2026-09-15，✅ 闭单）**：六个 wrapper 落地 `os/libs/minix-sys/src/syscall.rs`（沿既有 wrapper 约定区三条款：m_type 由 perform_kernel_call 写、载荷按 C union 成员填、负 errno 不吞）——`sys_fork`（E-FORKMSG 应答臂双出参 → `Result<(endpt, msgaddr), i32>`）、`sys_exec`（五载荷字段）、`sys_safecopyfrom`/`sys_safecopyto`（共用 `m_lsys_kern_safecopy` 臂五元组，无应答出参）、`sys_update`（M1 三字段——C do_update.c:9-11 的真实形状）、`sys_diagctl`（code/buf/len）。调用号常量**消费 minix-types::kernel_call 权威**（E-MINTYPES-SYS 产物，不添本地镜像）。CannedTransport 回放测试 ×7（六臂逐字段 wire + fork 错误直通）。**VM gateway 同轮切换**：sys_fork/diag_write/sys_exec/sys_update 四臂从内联 wire 改消费 wrapper（sys_kill 先例）——消除四份双真相源，四个死调用号常量（SYS_FORK_CALL 等）删除；SYS_VMCTL 三臂留在 gateway（VM 专用语义含应答臂解析，不在 E2 六类）。**解锁更新**：T11/T13/T14/T15 的通电面已备（pre-E1 trap 桩 -EIO 时 wrapper 诚实返回 Err，行为同今日）；safecopy wrapper 同时是 E-RSWIRE 第 3 步与 E-DSWIRE SysKernel 三拷贝桩的接续点。**验证**：minix-sys 132（+7）/ types 205 / kernel 752 / vm 503（all-features 522）全 passed；vm clippy 本体零告警。

---

## E4 pt_alloc free 注册 + 三架构 destroy 中间页回收（= 02-stage-vm V11-P2-8）

> **进度（2026-09-08，x86_64 + riscv64 完成）**：riscv64 Sv39 三级树同款回收落地——`free_child_tables` 以 V 位 + 非 leaf（R|W|X=0）判定表页（leaf = 数据页，归 region/exit 路径），L0 的子即数据故 level ≥ 2 停止下探；`channel_to_ptr` 同款 cfg(test+mock) 路由。**验证方式差异**：riscv64 模块 `target_arch` 门控，宿主不编译、QEMU/harness 均无 std——实现经 `cargo check --target riscv64gc-unknown-none-elf --no-default-features --features riscv64` 编译验证（kernel 同款 shape，0 error），运行时验证归 QEMU（E5 族）；原 riscv64 宿主测试草稿因 no_std 目标无 std harness 已删。x86_64 部分此前已完成（宿主测试在位）。**余件：aarch64（arch/src 无独立 paging 文件，目标可用时处置）+ kernel/VM 侧 `register_free` 接线**（随 VM 进程退出路径完善时补）。

> **余件闭环（2026-09-16，commit 75c88a123）**：VM 侧 `register_free` 接线落地——`alloc_page` 新增 `vm_pt_free`（镜像 `vm_pt_alloc` 的归还方向，归还 VM 页分配器），VmServer init 注册点补 `pt_alloc::register_free`（带 `is_free_registered` 重入守卫，与 register 对称）。注册后 `destroy()` 的 free_child_tables + 根页回收激活——此前未注册时 destroy 退化为只清零根，中间页表页随每次进程退出泄漏（C `pt_free` pagetable.c:1427-1437 的 Rust 对应物闭环）。kernel 侧不适用：本设计内核无常驻每进程页表（用户页表归 VM 域），boot 表常驻无 destroy 路径。aarch64 维持原注记（目标可用时处置）。**测试**：vm_pt_free 归还后分配器可再服务（不钉具体 pfn 策略——那是分配器变体的策略而非契约）；vm 507 全绿，clippy 基线 13=13。
>
> 原始问题描述（x86_64 部分已修复）：`pt_alloc.rs` 只有 `register/is_registered/alloc_pt_page`，没有 free；`x86_64/paging.rs` 的 `destroy` 只清零根 PML4——每次进程退出泄漏中间页表页（C `pt_free` pagetable.c:1427-1437 回收；Redox/Linux 同样回收）。

---

## E-VFSWIRE VFS_VMCALL 消息 wire 定稿 + VM 发送排水（T10 发送半）

**背景**：T10 入队半已落地——fdref 归零/进程退出的 FdClose、以及 mmap 的 FDLOOKUP/FDIO 均入 `VfsRequestQueue`（callback-less 或带回调）。但"把队首请求构造成 `VFS_VMCALL` 消息经 transport 发给 VFS"的发送排水未实现（历史上 transport-gated，见 doc 23 的 transport 缺口）。

**阻塞点（跨 stage）**：wire 格式属 VM↔VFS 共享契约，而 vfs 服务器（并行工作流）目前只有**决策原语**——`VmVfsReq::from_raw`（servers/vfs/src/misc.rs:266-281，101/102/103）与 `VM_VFS_REPLY=0xC1E`（:305），消息级解码（`do_vm_call` 的 union 成员/字段映射，C vfs.c:60-104 `VFS_VMCALL_REQ/FD/REQID/ENDPOINT/OFFSET/LENGTH`）尚未建。VM 侧先行编码会与 vfs 侧未来的解码漂移。

**解锁后 VM 侧工作（约半迭代）**：
1. `VfsRequestQueue::take_pending_vfs_call()`——从 active 请求构造 `VFS_VMCALL` 消息（字段映射 C vfs.c:70-78，req_id/endpoint/fd/offset/length）并标记已发送；
2. `VmServer::run_once` 排水步——`transport.send(VFS_PROC_NR, msg)`，失败保留请求下轮重试（pre-E1 每轮 -EIO，行为同今日的"永不发送"，无回归）；
3. 测试：Canned transport 上断言 VFS_VMCALL 字段逐项正确。

**依赖**：E1（trap 层）落地后发送才真实可达；vfs 侧 do_vm_call 消息面定稿（09/13-stage 工作流）后定 wire。

> **2026-09-09 增补（05-stage-vfs R2 轮）**：VFS 侧接收半现状——`VmVfsReq::from_raw` 决策原语已有（os/servers/vfs/src/misc.rs:262-281，101=FdLookup/102=FdClose/103=FdIo）与 `VM_VFS_REPLY=0xC1E`（misc.rs:305 一带），**消息级解码仍未建**（do_vm_call 的 union 字段映射，C vfs.c:60-104）。wire 定稿追加一条纪律：m_type 与字段的**绝对值**必须以 C 源断言（教训：vfs 侧 request.rs:15 把 REQ 基址写成 0x600 而 C com.h:589 为 0xA00，两侧偏移对齐掩盖基址分叉——详见 05-stage-vfs/todo.md §9.2 R2-P0-1）。


> **进度（2026-09-16，✅ 三步全闭环，commit d136491ee）**：(wire 定稿) `MessVmVfsCall` 56B 落 minix-types message.rs——m10 六域域位 offset@0/req@8/fd@12/req_id@16/endpoint@20/length@32(与既有 MessVmVfsReply 同一 mess_10 处理：i386 long 域按 u32 位保持)+ MessageUnion 增 `m_vm_vfs_call` 臂；`VFS_VMCALL=294`(callnr.h:110,VFS_BASE 0x100+38)与 `VMVFSREQ_*=101/102/103`(com.h:702-704)绝对值 pin，`VM_VFS_REPLY` 单一权威在 minix-types(vfs misc.rs 改 re-export——双址消除)。**(1) VM take**：`VfsRequest` 增 `sent` 标记，`take_pending_vfs_call()` 从 active 构造线面（vfs.c:83-90 六域映射，req_id 沿用队列分配号）。**(2) 排水步**：run_once 末尾 take → transport.send(VFS_PROC_NR)；失败 `mark_send_failed()` 下轮重试——pre-E1 恒失败，行为与今日"永不发送"无回归。**(3) vfs 解码**：`decode_vm_call` 消息级六域解码 + `VmVfsReq` 分派（m_type 不符/opcode 未知 → None），解码往返 witness 测试。**测试**：绝对值 pin+布局+字节级 C 构造复刻 ×3、transport 级发送断言+重试标记、vfs 往返；types 245 / vm 506 / vfs 362 全绿；clippy 与基线 13=13。**通电**：与 E1 切片 5 同场——真 transport 激活后排水自动成为真实发送。
---

## E-RSWIRE rprocpub 字节 ABI pinning + rproctab 解码（T9 step3 的余件）

**背景**：RS_INIT 握手的 grant 贯通与 fail-closed 已落地（Fix #34）；`ipc_call_rs_init` 目前诚实返回 `NotImplemented`（pin 测试 `test_run_once_rs_init_fails_closed_until_erswire` 定格），假成功 `Ok(RprocTab::empty())` 已消灭。

**阻塞点**：`struct rprocpub` 的字节 ABI 无法从本仓 minix3 子树 pinning——`devmajor_t`、`bitchunk_t`、`struct rs_pci` 在此树中被引用但**无定义**（rs.h/type.h 不完整）。按 Ground Truth 链（C 源 > doc > code），缺 C 事实就不能猜偏移。且该布局是 RS↔VM 共享契约（rs 服务器工作流同样消费），落点应在 minix-types。

> **拆分注记（2026-09-07，同 E-RSSTART 改判）**：阻塞前提同样不成立——`devmajor_t = int32_t`（`minix3/sys/sys/types.h:286-288`）、`bitchunk_t = uint32_t`（:124）、`struct rs_pci` 完整（`minix3/minix/include/minix/rs.h:154-162`）、`struct rprocpub` 完整（rs.h:165-183）。本条目拆两半：**RS 侧半已彻底完成**（2026-09-08，§21 R7+R12）——`rprocpub` wire（Fix #85）与 `struct rproc` 内部表 wire（Fix #89：witness 派生偏移，sizeof=3752，传递 pinning struct priv/minix_timer_t/sys_map_t/sigset_t 全部在树内核实）落地 `minix-types::ipc::{rprocpub, rproc}`，`do_getsysinfo` 三臂（SI_PROCPUB_TAB/SI_PROC_TAB/SI_PROCALL_TAB）全量 live；**VM 侧半**（下文第 3 步：`Gateway::sys_safecopyfrom` + `ipc_call_rs_init` 真实体 + pin 测试翻转）仍留本条目——属 VM stage 生产代码（edge 判定②），其 wire 消费面（`RprocPubWire`/`decode_rproc_pub`）已就绪。原文偏移表的 ILP32 假设作废：x86-64 LP64，风格模板 = `minix-types::ipc::{rs_start, rprocpub}`（Fix #81/#85）。

**解锁后工作（约一个完整迭代）**：
1. 从完整 Minix3 源码树 pin 三类型定义 → 计算偏移表（ILP32：in_use short@0、sys_flags@4、endpoint@8、old_endpoint@12、new_endpoint@16、dev_nr、nr_domain、domain[8]、label[16]、proc_name[16]、vm_call_mask[BITMAP_CHUNKS(49)]、rs_pci、devman_id）；
2. minix-types 增 `RprocpubWire`（repr(C)）+ `decode` + 偏移断言测试（手排字节十六进制锚定）；
3. VM 侧：`Gateway::sys_safecopyfrom(granter, gid, offset, buf)`（wire 已定：SYS_SAFECOPYFROM=31、`MessLsysKernSafecopy{from_to,grant_id,offset,address,bytes}`，kernel 应答 Ok(0)=成功）+ `ipc_call_rs_init` 真实体（拷贝 + 解码 → RprocTab）+ 翻转 pin 测试 + 恢复 `rs_handshake` ACL 循环为可达。

> **增补（2026-09-09，02-stage-vm V13 轮，02-stage-vm/todo.md §18.2）**：第 3 步落地时必须一并做三件 VM 侧事，否则 wire 通即引入新缺陷——
> (a) `RprocEntry.call_mask: u32 → u64`（vm_server.rs:1594-1599）：minix-types 的 wire 已是 u64（rprocpub.rs:89，2×u32 little-endian 合并），VM 内部 u32 会截断调用号 +32..+48 的 9 个调用授权（消费点 vm_server.rs:1340 `entry.call_mask as u64`）；
> (b) `RS_SET_PRIV` 真掩码：E2 的 SYS_SAFECOPYFROM 可用后，dispatcher.rs:1091-1102 的硬编码 `mask = None` 换成 C rs.c:41-56 的 sys_datacopy 语义（M2 的 m2l1 是掩码缓冲指针）；
> (c) 五个 C 消息结构（getphys/getref/info/rusage/update）补专属 wire struct + `size_of`/`offset_of` 断言（现 overlay 解码字段语义与 C 错位：getref addr C@4 vs Rust m1p1@16、info next C@16 vs Rust m2l2@24、rusage children C@8 vs Rust@4；C 锚 ipc.h:928-934/:1486-1492/:1494-1502/:1513-1520），并把其余 VM payload 缺失的 56 字节断言补齐（12 个结构仅 MessVmmcpReply 有）。

**依赖**：~~完整 Minix3 C 源码参照（或补全本树头文件）~~（已解除，同 E-RSSTART）；无 E1/E2 依赖（解码可纯单测）。

> **进度（2026-09-16，✅ VM 侧半闭环，commit 403e47022）**：第 3 步 + V13 三增补全落地——(3) gateway 新增 `sys_safecopyfrom`（minix-sys E2 wrapper 已在，堆缓冲指针即 VM 用户地址）+ `ipc_call_rs_init` 真实体：整表一次 safecopy（17×420B，granter=RS，main.c:244-247）+ `decode_rproc_pub` 逐条解码 + `IS_RPUB_BOOT_USR`=（endpoint==INIT，rs.h:188）；钉子测试翻转为正负两路——正路断言整表拷贝线面（RS, gid, offset 0, 7140B）与 VFS 槽 System ACL 落位，负路 safecopy 失败仍丢弃计数不回复。(a) `call_mask` u32→u64（wire 本是 2×u32 合并 u64，截断丢 +32..+48 授权位；消费点 cast 撤销）。(b) RS_SET_PRIV 真掩码——m2l1 非零时按 rs.c:45-58 拷 2 bitchunk 小端合并为 u64 mask，空缓冲+sys 目标 EINVAL（rs.c:55-57）；C `sys_datacopy` 由 `SYS_VIRCOPY` 同义承载（内核只有一条 copy 臂，flat 地址空间无 data/vir 分歧）。(c) 五消息 wire 结构落 minix-types ipc/vm.rs（getphys/getref/info/rusage/update，56B LP64 见证 ×5）+ MessageUnion 五臂；VM 四处解码切类型化臂——修正三处 overlay 错位（getphys addr 16→8、getref addr 16→8、rusage children 4→16；update 的 overlay 恰与 C wire 重合）。**余项**：`map_service` 的 C 语义（main.c:249-255）现以 ACL 落位承载，GET_NICE/标签类元数据消费归 E9 RS 五域切片；rs_handshake 对 decode 失败映射 InvalidParam（C 是 panic——fail-fast 语义差异记 E9 复核）。**验证**：minix-vm 505 / minix-types 242 全绿；clippy 与干净 HEAD 13=13。

---

## E-BOOTFRAME boot 初始栈帧 ABI（T14 余件：minix_stack_params/fill 复刻）

**背景**：`exec_bootproc` 的装载半（ELF 段映射）与 `sys_exec` 半已落地（Fix #41）。C main.c:381-408 在两者之间构造最小初始栈帧——`minix_stack_params(path, argv, envp, …)` 计算尺寸 + `minix_stack_fill(…)` 产出字节精确的 frame，再 `sys_datacopy(SELF, frame, endpoint, vsp, …)` 拷入目标地址空间，`psp = vsp + (psp - frame)` 作为 ps_strings 指针传 `sys_exec`。

**为何 edge**：frame 布局是 VM↔libc↔kernel 三方共享 ABI——argv/envp 数组、字符串区、`struct ps_strings {argv, argc, envp, envc}` 的排布与对齐，消费方是 minix3 libc crt0（`_start` 如何取 argc）与内核 `arch_proc_init`（sp/ps_str 的寄存器约定）。仓内无既有物（PM 的 exec frame 由 VFS 构造传入，路径不同）。复刻前必须对照 minix3 libc 源码（crt0.S + libminixfw 的 stack.c）与本项目未来用户态约定，属跨模块契约。

**解锁后 VM 侧工作（约一个迭代）**：
1. `struct PsStrings` + `stack_params`/`stack_fill` 复刻（boot 简化路径：argv={proc_name,NULL}、envp={NULL}）；
2. `exec_bootproc` 中建栈 region（`user_sp` 向下一页）+ frame 写入（Direct Map）+ `handle_memory_once` 实化；
3. `sys_exec` 的 stack/ps_str 换真值；
4. 测试：SimPaging 下断言 frame 字节布局 + `last_exec` 的 stack/ps_str。

**依赖**：minix3 libc 源码参照（crt0 约定）；与 E1/E2 无序（frame 构造纯 VM 内）。

> **进度（2026-09-16，✅ 四项全闭环，commit f459852ea）**：(1) `PsStrings` 32B LP64 布局落 minix-types（exec.h:111-116）+ PMEF_AUXVECTORS/PMEF_EXECNAMELEN1 上收 com.rs（com.h:356-357，PATH_MAX=1024 syslimits.h:64）；`stack_params`/`stack_fill` 复刻落 **minix-sys stack.rs**（libc stack_utils.c 的归位——C 就在 libc，VM/RS/execve 三方消费）。(2) exec_bootproc 尾段抽成 `install_boot_stack`：建帧（帧缓冲 = VM 栈上一页，同 C main.c:352 `char frame[VM_PAGE_SIZE]`）+ 栈 region（user_sp 向下一页覆盖 [vsp,vsp+frame_size)）+ 页 materialize + `handle_memory_once` 实化（main.c:400 调用点，写前可映射+可写门）+ DM 写帧替代 sys_datacopy（main.c:402-404；两侧皆可直接寻址）。(3) `sys_exec` 换真值——stack=vsp、ps_str=绝对地址；name 仍 0（kernel 侧 name 拷贝语义另案，C 传的是 VM 本地 progname 指针）。(4) 集成测试：SimPaging + MockGateway 下断言 `last_exec` 四元组与帧字节逐字节回读（DM 窗口读 vsp 比对独立构建的参考帧）。**连带裁决**：① handoff v3→v4 增 `user_sp`（C VM 从 kernel_boot_info 读同一值 main.c:372；kernel writer 用 KernelInfo.user_sp）；② STACK_MIN_SZ 的 C argc 槽 `sizeof(int)`(4) 与 fill 实写整字(8) 在 LP64 不一致——从 fill 实际写入裁决，budget=1400；③ ps_argvstr=vsp+sizeof(argc) 在 i386 恰为 argv[0] 槽地址——LP64 裁决为 vsp+8（槽地址语义，非首字符串地址，stack_utils.c:169 实读）。**顺手修一个通电必炸的潜在 panic**：init_proc 从未 init_page_table/init_regions，regions_mut() 的 debug 守卫会在 boot 路径崩——补上（= C pt_new/pt_bind main.c:344-347 + map_region_init main.c:468）。**验证**：minix-vm 504 / minix-sys 155 / minix-types 237 全绿；clippy 与干净 HEAD 13=13 对账零新增。

---

## E-FORKMSG kernel sys_fork 应答补 msgaddr 出参（= 02-stage-vm T33 余件，2026-09-07 登记）

**问题**：C 的 `sys_fork` 有第五个出参 `msgaddr`（fork.c:90，内核自 `p_delivermsg_vir` 报告 PM 的 fork 消息在父地址空间的位置）；`do_fork` 用它对父子两侧的交付消息缓冲做 eager CoW（fork.c:100-108，`handle_memory_once` ×2），防止内核写 fork 应答时撞上 VM 单线程死锁。minix-rs kernel 的 `dispatch_fork`（syscall_process.rs:122-215）只回 child endpoint，VM 侧 `do_fork` 的 eager-CoW 相以 `fork_msgaddr == None` 门控跳过（fork.rs，V11/T33）。

**跨 stage 文件**：`os/kernel/src/syscall_process.rs`（dispatch_fork 应答补 msgaddr——来源 `caller.p_delivermsg` 等价物）、`os/libs/minix-sys` 或 reply wire（KcallResult/`m_krn_lsys_sys_fork` 加字段）、`os/servers/vm/src/kernel_gateway.rs`（`sys_fork` 的 `None` 换真值，VM 侧零改动——消费代码已就位并有 mock 测试）。

**解锁**：T33 的 eager-CoW 相在真实硬件上生效；E5(a) PM↔VM fork 联调的正确性前提（内核写应答不撞 CoW 死锁）。

> **复核（2026-09-15）**：锚点更新——dispatch_fork 体 syscall_process.rs:133-215，应答仅 `KcallResult::Ok(child_endpoint.0)`（:214）；gateway kernel_gateway.rs:197-199 返回 `None`（注释自引本条）；fork.rs eager-CoW 门在 :364。另核实 minix-types 无 `m_krn_lsys_sys_fork` 成员——msgaddr 出参需在应答 wire 上新增字段，与本条「跨 stage 文件」第 2 项一致。

> **进度（2026-09-15，✅ 闭单）**：执行中把问题从「缺 msgaddr」修正为「应答 wire 双缺 + 误编码」——C 的应答 struct `mess_krn_lsys_sys_fork`（ipc.h:283-287）本就带 endpt+msgaddr 两出参、由 do_fork 原地写入（:111-112）后 finish 整消息拷回（system.c:81-86）；Rust 现状把 child endpoint 编码进 `KcallResult::Ok(...)` 会被 finish 写进 **m_type**（应答 m_type = 端点值而非 OK），即真实通电后 VM 把 m_type 当端点读是错的、C libc 兼容读法全落空——mock seam 掩盖了这一点。落地四件：(1) minix-types 新增请求/应答双 wire struct（`MessLsysKrnSysFork` ipc.h:1173-1177 i386/LP64 同布局；`MessKrnLsysSysFork` 应答侧 `[ARCH: A-FORKWIRE]` LP64 适配 msgaddr 4→8 字节、尾 padding 48→40，rs_start 判例）+ 两 union 臂 + size_of/offset_of 断言；(2) kernel `dispatch_fork` 签名 `&Message → &mut Message`（链路 kernel_call 起本就持有 &mut，唯一改签名的臂），请求解码从 M1 overlay 换专属臂（FIX-25 纪律），成功路径原地写 endpt/`caller.p_delivermsg_vir` 并返回 `Ok(0)`（C do_fork.c:135 返回 OK）；(3) VM `TrapKernelGateway::sys_fork` 从应答臂读出参返回 `Some(msgaddr)`（真实 eager-CoW 相由此激活——fork.rs 消费逻辑零改动）；(4) 测试翻转：kernel t12 主测试断言 Ok(0)+双出参（旧代码下失败）、VM pin 测试经 `reply_message` 整载荷脚本断言 `(Endpoint(77), Some(0x7000))`（旧代码下 None），错误路径测试构造统一换新臂。方案对比：KcallResult 扩展携带出参（否——自创协议，C libc 按 ipc.h 读 wire）；finish 特判回写（否——出参语义属 handler，C 亦然）。**验证**：kernel 752 / vm 503（三矩阵 503/503/522）/ types 205 全 passed；kernel/VM clippy 基线持平（44/0 本体）。eager-CoW 的真实生效仍需 E1/E2 通电（pre-E1 trap 桩 -EIO 先于 msgaddr 消费），端到端验收挂 E5(a)。

## E5 端到端联调测试包

**问题**：VM 与其他服务器的协作当前零端到端覆盖——`os/tests/pm_vm_fork{,_test}.rs` 正文整体注释停用（自注 "DEPRECATED: permanently disabled"）；VFS fdclose、RS live-update、QEMU VM paging 冒烟均无。

**建议**：在 E1/E2 落地后建联调包：(a) PM↔VM fork 全链路（恢复/重写旧测试，改走 minix-sys 消息层而非 crate 内类型——旧失效原因正是 `pub(crate)` 边界收紧）；(b) VM↔VFS fdclose 往返；(c) RS live-update 全链路（RS_PREPARE → UPDATE → resume）；(d) QEMU VM paging 冒烟（boot shim 拉起 VM → `init_vm_self_pt` → map/query/unmap 测试页 → 串口结果，复用 `os/qemu-tests/` 基建）。

> **验收面增补（2026-09-08，02-stage-vm V12 轮）**：(d) 冒烟必须覆盖**缺页故障完整回路**——进程触发缺页 → 内核 VM_PAGEFAULT 送达 VM → VM 解析（CoW 拷贝或新页分配）→ **VM 写进程硬件 PTE**（G-V12-8，当前缺失的最后一环）→ 内核清 RTS_PAGEFAULT 恢复进程 → 指令重执行不再故障。没有 PTE 写入的通电冒烟会以"同一地址反复缺页活锁"的形式失败，这正是该回路必须成为 (d) 的显式断言项的原因。

> **验收面增补（2026-09-09，06-stage-sched V2 轮）**：(e) **PM↔SCHED 调度链**——sched 服务器是全仓唯一"每个行为都依赖他方主动来电"的服务（PM 的 START/INHERIT/STOP/SET_NICE、内核的 NO_QUANTUM、自身的 5 秒平衡闹钟），端到端零覆盖。冒烟三链：init 的 START（PM 发 SCHEDULING_START → SCHED `sys_schedctl` 接管 → `sys_schedule` 下发 → 内核实调）；fork 子进程的 INHERIT（优先级/时间片从父槽继承，06 篇）；一条 NO_QUANTUM 回环（内核 notify_scheduler → SCHED 降一级 → `sys_schedule` 回写）。前置 E1 + E8；E-PREEMPTFLAG/E-SCHEDNICED 修不修都应在冒烟中显式观察——未修时优先级 ≥1 的进程可跑通全链，恰可作"断环分界"用例（队列 0 进程验证 NO_QUANTUM 缺失）。

> **验收面增补（2026-09-14，07-stage-ds 首轮架构审查）**：(f) **DS 发布/订阅链**——C 的行为契约在 minix3/minix/tests/ds/（dstest.c 178 行：publish/retrieve/delete/getsysinfo 全类型往返；subs.c 91 行：subscribe/check/notify 回环），Rust 侧零承接。冒烟三链：(1) RS boot 映射后 `ds_retrieve_label_endpt` 可解析服务端点；(2) 客户端 A `ds_publish_u32` → 客户端 B `ds_check` 收到通知并取回键名与类型（ipc_notify 回环）；(3) label 级联删除（RS 删 label → 同名主条目与订阅全清，store.c:613-636）。前置 E-DSWIRE；A-2 regex 引擎落地后追加元字符 pattern 订阅用例（VFS 同款 `drv\.[bc]..\..*`，main.c:441）。

> **验收面增补（2026-09-15，10-stage-mib 首轮架构审查）**：(g) **MIB/sysctl 链**——冒烟四链：(1) MIB_SYSCTL 往返（sysctl(2) 调用方 → MIB 解码 → 树查找 → 数据返回，含 ENOMEM 部分拷贝 + 完整长度回写语义 main.c:341-356）；(2) rmibtest 契约（minix3/minix/tests/rmibtest/rmibtest.c：8 个远端注册拒绝场景 :126-177、遮蔽注册顺序 :183-201、handler 函数偏转 :79-85）；(3) 远程子树 ERESTART 续走（服务死亡 → mib_down → 本地续走，remote.c:455-459 + tree.c:1410-1416）；(4) minix.mib.* 统计子树读数一致（TreeCounts 计数与真实节点数对账）。前置：10-stage-mib/todo.md P1-1/P1-2（stage 内执行半）+ E1 + E-RMIBWIRE。

> **验收面增补（2026-09-15，11-stage-devman 首轮架构审查）**：(h) **devman 设备生命周期链**——冒烟四链：(1) VFS mount devman 后 `devices`/`events` 可见（懒建树触发，11-stage-devman/todo.md DM-P1-3）；(2) 驱动进程 `devman_add_device`（minix-sys 客户端）→ devman 树出现目录与 `devman_id` 文件 → 读 `events` 文件两读排空到 ADD 行；(3) RS publish → DEVMAN_BIND 转发给设备 owner → 驱动 `handle_msg` 回调 → 回 RS OK（C manager.c:840-851 全链）；(4) UNBIND + DEL → REMOVE 事件 + 树内消失 + 父设备引用级联回收。对照 C 行为契约 minix3/minix/commands/devmand/main.c 的属性匹配 DSL（13 篇）可用 usb 设备属性样例覆盖。前置：E-DMWIRE + E-REQWIRE（VTreeFS wire 面）+ E1。

---

## E6 minix-sys PM 所需 SYS_* wrapper 扩充（= 04-stage-pm/todo.md §9 抽取）

**问题**：E2 只覆盖 VM 侧 6 类 SYS_* 包装；PM 侧生产代码声明的内核调用面同样没有用户态入口。04-stage-pm/todo.md §6 的 24 项 D-XX 登记中有 6 项的解除条件是"对应内核调用经 minix-sys 可达"，但 minix-sys 侧没有任何 PM 专用 SYS_* 包装（`os/libs/minix-sys/src/` 全源仅 lib.rs 的 14 个 POSIX 形顶层函数）。

**证据**（kernel 对端现状，2026-09-06 grep 核实）：
- 对端已实现、可直接包 wrapper：SYS_TIMES（`os/kernel/src/syscall_clock.rs:90`）、SYS_CLEAR（`os/kernel/src/syscall_process.rs:366`）、SYS_KILL（`os/kernel/src/syscall_signal.rs:148`）、SYS_SIGSEND（`os/kernel/src/syscall_signal.rs:544`）、SYS_ABORT（`os/kernel/src/syscall.rs:1793`）、SYS_SAFECOPYFROM/TO（`os/kernel/src/syscall_copy.rs:361/:376`，E2 已列）。
- 对端也不存在：SYS_GETMONPARAMS/SYS_GETIMAGE（`os/kernel/src/` 全源 grep 零命中）——PM 的 `BootParams::placeholder()`（`os/servers/pm/src/main.rs:15`）没有真实对端；需先与 01-stage-kernel 裁决启动参数/映像的传递路径（boot-shim handoff 是否已取代 C 的 monitor 参数语义），再双侧新建。

**影响**：04-stage-pm/todo.md 的 D-02（启动参数）、D-09（sys_abort）、D-13（sys_kill）、D-18（sys_clear）、D-21（rusage safecopy）、D-24（sys_times/getticks）全部停在"逻辑完备、通电无门"；PM 的 `KernelIpcTransport`（`os/servers/pm/src/ipc/transport.rs:86-96` unimplemented）除 trap 层（E1）外也缺这层调用面。

**建议**：复用 E2 的 `perform_kernel_call`（`os/libs/minix-sys/src/syscall.rs:201`，ENOTREADY 重试）与 `KernelCallTransport` 机制，为对端已实现的 6 类各加一个包装函数（签名对齐 kernel dispatch 的消息布局，message 构造用 minix-types 现有 union 成员），每个包装带一个 CannedTransport 回放单元测试；SYS_GETMONPARAMS/SYS_GETIMAGE 单独立项，待 01-stage-kernel 裁决传递路径后双侧补齐。

**解锁**：04-stage-pm/todo.md D-02 / D-09 / D-13 / D-18 / D-21 / D-24 的真实通电；E5(a) PM↔VM fork 联调的信号与回收链前置。

> **进度（2026-09-06）**：`sys_kill` wrapper 已落地（`syscall.rs` 的 `sys_kill` + `SYS_KILL_CALL` 常量，wire 断言测试 ×2；`CannedKernelCallTransport` 增 `sent` 逐调用消息记录供 wire 形状断言）——消费侧 04-stage-pm/todo.md D-13/Fix #23 同轮闭环；余下进度：SYS_CLEAR（轮 24）、SYS_ABORT（轮 25）、SYS_TIMES（轮 26）、SYS_RUNCTL/SYS_RESUME（轮 28）已落地；待做 SYS_GETMONPARAMS/SYS_GETIMAGE wrapper（kernel 对端亦缺，需双侧新建）。
>
> **进度（2026-09-08，V2 轮扩充）**：已落地 wrapper 达 6 个（sys_kill/sys_clear/sys_abort/sys_times/sys_runctl+sys_resume/sys_vircopy）。04-stage-pm 第二轮审查（04-stage-pm/todo.md §11.1.1 接线批次表）给出 PM 侧完整需求清单，待做 wrapper 及 kernel 对端现状：`sys_setalarm`（kernel 对端已有 `syscall_clock.rs:160`，E8 已列）、`sys_vtimer`（ITIMER_VIRTUAL/PROF，对端待核实）、`sys_datacopy`（注意≠sys_vircopy：SELF 本地拷贝语义，getgroups/setgroups/itimer value 双向拷贝需要）、`sys_settime`/`sys_stime`（time.c do_settime/do_stime）、GETUPTIME 面（time.c getuptime 依赖，PM `ClockSource` 的生产实现）、`sys_getmcontext`/`sys_setmcontext`（mcontext.c 直通）、`sys_sprof`（profile.c，feature 门）、`sys_sigreturn`（C signal.c:176-192 do_sigreturn 调用，kernel 对端待核实）、`sys_diagctl_stacktrace`（C signal.c:556-558 coredump 诊断，04-stage-pm V2-P3-4a）、SYS_GETMONPARAMS/SYS_GETIMAGE（维持"双侧新建"结论）。
>
> **进度（2026-09-09，V3 轮增补，来源 04-stage-pm/todo.md §12 V3-P1-2 / V3-P1-3）**：清单增两项。① **`sys_delay_stop`**——13-signal-flow 的 `KernelStop` 生产实现前置（`os/servers/pm/src/signal_flow.rs:115-140` `stop_proc` 的内核停止 seam）；04-stage-pm V3-P1-3（sig_proc 的 VFS_CALL 分支接真实 stop_proc）mock 层实施不受阻，生产语义完备依赖本项。② **内核 ksig 对端核实**——C 的内核→PM 信号回环入口是 SEF 拦截 SIGKSIG 通知后调 `process_ksig`（`minix3/minix/servers/pm/main.c:121` + `minix3/minix/lib/libsys/sef_signal.c:104-108`），C 的 process_ksig 内部走 sys_getksig/sys_endksig 内核信号队列循环；Rust 侧 `process_ksig`（`os/servers/pm/src/signal.rs:374`）现无生产调用者（04-stage-pm V3-P1-2，新接线批次 H）——需核实 os/kernel 是否已实现 getksig/endksig 或等价通知面；若无，wrapper 归本条、kernel 对端归 01-stage-kernel 工作流（双侧新建，同 SYS_GETMONPARAMS 先例）。

> **进度（2026-09-09，V3-P1-1 切片）**：`sys_trace` wrapper 已落地（`os/libs/minix-sys/src/syscall.rs`：`sys_trace` + `SYS_TRACE_CALL = 5`，载荷 `m_lsys_krn_sys_trace` 布局（request@0/endpt@4/address@8/data@16，≠ m_m1），读值经同偏移写回；wire 断言测试 ×2）——kernel 对端 `dispatch_trace` 早已真实（`os/kernel/src/misc.rs:209`，非待核实项）；消费侧 04-stage-pm V3-P1-1（PTRACE 臂 + trace_stop）同轮闭环。**同轮新登记**：`sys_delay_stop`（见上①）之外再确认 kernel 侧无 SYS_TRACE 之外的 PM trace 依赖；`sys_datacopy` 语义（T_GETRANGE 的参数块取入）当前由 `KernelGateway::copy_from_user` 委托 sys_vircopy 承接（Fix #27 同型先例），独立 `sys_datacopy` wrapper 维持原清单判断。

> **进度（2026-09-09，V3-P1-2 切片）**：`sys_getksig`（SYS_GETKSIG=7）/`sys_endksig`（SYS_ENDKSIG=8）wrapper 已落地（wire 断言测试 ×2）——kernel 对端 `dispatch_getksig`/`dispatch_endksig` 早已真实（`os/kernel/src/syscall_signal.rs:393/475`），原"②内核 ksig 对端核实"结论：**getksig/endksig 已存在，缺的只是 wrapper**（本切片闭环）。消费侧 04-stage-pm V3-P1-2（SYSTEM notify 触发 + `process_sigmgr_signals` 拉取循环）同轮落地。**新增跨层登记（SigSet 128 位拓宽）**：Rust `SigSet(u64)` 装不下内核信号位 70/73/74（SIGSNDELAY/SIGKSIGSM/SIGKSIG，kernel `syscall_signal.rs:88-94` 已自声明"widening tracked separately"）——影响 GET_PROCTAB/GET_PRIVTAB/notify/GETKSIG 消息字段 + PM 镜像，属于共享契约层 wire 变更，需单独立项（建议挂本条目或 E7 追加判定）；PM 侧 batch H 的触发判定已按"SYSTEM 通知即拉取"适配并注释声明。

> **进度（2026-09-15，✅ 闭单）**：E2 轮将 wrapper 家族一次补齐九臂——`sys_setalarm`（应答臂 time_left/uptime 回读，syscall_clock.rs:265 同臂回填）、`sys_sigreturn`（m_sigcalls.endpt/sigctx）、`sys_sprof`（六字段）、`sys_settime`/`sys_stime`（wire 域 u64）、`sys_vtimer`（M2 形状 which/set/value/endpt，旧值 m2l1 回读 syscall_clock.rs:505-508）、`sys_getmcontext`/`sys_setmcontext`（同臂异调用号）、`sys_diagctl` 改 C 原型 `(code, arg1, arg2)`（STACKTRACE 用 endpt 字段；VM gateway 调用点同步）。回放测试 ×9（逐字段 wire + setalarm 应答臂 + vtimer 旧值 + stacktrace endpt）。**GETUPTIME 面如实登记**：C com.h:315-345 的 GET_* 家族无此号（minix-types 对账表同缺），PM 侧 `read_uptime_triple` 为宿主实现——需 C 对账轮裁决，不在本条。**GETMONPARAMS/GETIMAGE 双侧新建**与 **SigSet 128 位拓宽**维持独立项登记。**验证**：minix-sys 144（+9 回放）/ vm 503 全 passed；clippy 基线持平。**E6 全清单销账**：04-stage-pm 的"通电无门"项 wrapper 半全部就绪（GET_* 数据面仍挂 E-ISPROD 裁决）。

---

## E7 minix-types PM 协议面系统化（= 04-stage-pm/todo.md P1-4 + P2-3 抽取）

**问题**：minix-types 对 PM 的协议面是三个碎片。(a) `os/libs/minix-types/src/ipc/pm.rs:13-38` 的 `PmRequest`/`PmResponse` 是零使用死代码——整个工作区只有 `PmError` 被 `os/servers/pm/src/init.rs:390` 消费。(b) PM 调用号双址：`os/servers/pm/src/ipc/calls.rs:29-124` 的 `PmCall` 枚举（47 值，calls.rs:16-17 注释自述"将来内核侧需要调用号时再上移 minix-types"）与 minix-types 散落常量（如 `PM_PROCEVENTMASK`，被 `os/servers/pm/src/init.rs:368` 使用）并存，同一事实两处表达。(c) 47 个调用的消息布局没有系统化 wire 类型——C `m_lc_pm_*`/`m_pm_lc_*` union 字段（minix/com.h）当前只在 PM 主循环内联 unsafe 访问（`os/servers/pm/src/init.rs:430/438-440/459-460/480-481`）。另有调度协议常量缺失：`SEND_PRIORITY`/`SEND_TIME_SLICE`（C `minix3/minix/servers/pm/const.h:19-20`）全仓无对应。

**证据**：`os/libs/minix-types/src/ipc/pm.rs` 全文 111 行仅 Fork 一个请求变体；VM 侧同型先例是 `NR_VM_CALLS` 双定义（02-stage-vm/todo.md:1047，`minix-types/src/ipc/vm.rs:149` vs `os/servers/vm/src/vm_server.rs:1173` 私有副本）；共享层"上移 minix-types"的 OQ 决策先例见 G1（01-stage-kernel/todo.md:342，ProcNr）。

**影响**：PM 的分发接线（04-stage-pm/todo.md P1-1，40 个待点亮调用）每一臂都要先回答"消息怎么解码"；没有系统化 wire 类型，unsafe 解码将被复制约 40 份，wire 布局错误无类型层防护；调用号双址使 callnr.h 的单一真值破口随消费方（libc/commands）增多而扩大。

**建议**：与 04-stage-pm P1-4 协同一次做齐——(1) 按调用族在 `os/libs/minix-types/src/ipc/pm.rs` 建 wire 结构体（对照 C union 逐字段 + `size_of` 断言，风格对齐 `ipc/message.rs` 既有成员如 `MessPmSchedSchedulingSetNice` :1165）；(2) 处置 `PmRequest`/`PmResponse` 死代码：要么作为新 wire 层的入口枚举重构，要么删除（待 P1-4 设计时定，不允许默认保留）；(3) 调用号收敛二选一：47 个 `pub const PM_*` 上移 minix-types（PmCall 枚举随之迁移，成为 callnr.h 的 Rust 等价物，倾向此案）或 minix-types 常量清空、pm crate 为唯一真值——需 OQ 确认归属；(4) 补 `SEND_PRIORITY`/`SEND_TIME_SLICE` 常量。

**解锁**：04-stage-pm/todo.md P1-1（每臂解码）/ P1-4 / P2-3 的实施前提；未来 libc/commands 侧 PM 调用发起方的常量消费。

> **进度（2026-09-16，🔄 D/E 批 itimer 与 exec 闭环）**：`ipc/pm.rs` 新增调用号 ×4（PM_ITIMER 17/PM_EXEC 14/PM_EXEC_NEW 43/PM_EXEC_RESTART 44，callnr.h:30/:27/:56/:57，PM_BASE=0x000 绝对值 pin）与三个 wire 结构——`MessLcPmItimer` 56B（which/value/ovalue，ipc.h:468-474）、`MessLcPmExec` 56B（name/namelen/frame/framelen/ps_str 五域，ipc.h:435-443）、`MessRsPmExecRestart` 56B（endpt/result/pc/ps_str，ipc.h:1869-1876）；LP64 判例同前——指针域 4→8 字节、padding 等比收缩（itimer 44→32、exec 36→16、restart 40→32）、总长保持 56 字节。布局见证 ×3。**E 批余注**：`m_lexec_pm_exec_new`（PM_EXEC_NEW）与 PM_EXEC 共用 exec 形状，落地 PM 消费侧时按调用号分派，不另立结构。**剩余**：G 杂项 9 →(2) PmRequest/PmResponse 死代码删除 + (3) PmCall 枚举上移（待 OQ 确认归属）。
>
> **进度（2026-09-16，🔄 B 批信号控制闭环）**：`ipc/pm.rs` 新增 B 批调用号 ×6（PM_SIGACTION 20/SIGSUSPEND 21/SIGPENDING 22/SIGPROCMASK 23/SIGRETURN 24/KILL 11，callnr.h:24/:33-37 绝对值 pin）与两个 wire 结构——`MessLcPmSig` 56B（pid/nr/act/oact/ret，SIGACTION 与 KILL 共用，signal.c:48-84）+ `MessLcPmSigset` 56B（how/_pad/ctx/set，SIGPROCMASK·SIGSUSPEND·SIGPENDING 共用，signal.c:119-152；LP64 padding 32→24 吸收 vir_bytes 8 字节对齐）。布局见证 ×2。**F 批归属修正**：`SEND_PRIORITY`/`SEND_TIME_SLICE` 为 PM 内部常量（pm/const.h:19-20，仅 PM→SCHED 的 flags 语义），不属 minix-types 共享层——E7 原文「F 批常量补齐」改判为 04-stage-pm 域内工作。**C 批时间 6 调用已闭环（2026-09-16）**：`MessLcPmTime` 56B（sec u64/clk_id/now/nsec，LP64 padding 36→32）+ 调用号 ×6（STIME 7/GETTIMEOFDAY 28/CLOCK_GETRES 33/CLOCK_GETTIME 34/CLOCK_SETTIME 35/GETRUSAGE 36）+ 布局见证 ×2——共用 `mess_lc_pm_time` 一臂（time.c:31/55/79 按 clk_id 分派）。

> **进度（2026-09-16，🔄 A 批凭证片闭环）**：`ipc/pm.rs` 新增 A 批凭证调用号 ×15（PM_GETPID 4..PM_REBOOT 37，callnr.h:17-52 绝对值 pin 测试）与三个 wire 结构（`MessLcPmSetid`（SETUID/SETGID 同布局共用）、`MessLcPmGetsid`、`MessLcPmGroups`（LP64 ptr@8、padding 48→40），56 字节布局见证 ×3）。PM_BASE=0 的 A 批调用号本就无偏移基，与 com.rs 既有语义一致。**剩余**：B 信号控制 6→C 时间 6→D itimer→E exec 3（`m_lexec_pm_exec_new`/`m_rs_pm_exec_restart`）→F 调度 2（SEND_PRIORITY/SEND_TIME_SLICE 常量补齐）→G 杂项 9——按 04-stage-pm/todo.md §11.1.1 接线批次表逐批推进；(3) PmRequest/PmResponse 死代码处置（E7 设计决策：A 批 wire 落地后其去留已可判定——两者是零使用死代码，删除）与调用号收敛（PmCall 枚举上移）待 OQ 确认归属。**验证**：minix-types 212 passed；clippy 1 条 known。

> **进度（2026-09-06）**：首个切片已落地——`MessPmLcWait4 { status }` + `m_pm_lc_wait4` arm（`message.rs`，56 字节断言 `test_pm_wait4_message_layouts`），wait4 回复载荷契约（04-stage-pm/todo.md D-26/Fix #22）闭环；其余 wire 族照本切片的风格推进。
>
> **进度（2026-09-08，V2 轮增补）**：(1) **仓库内先例确立**：`minix-types::ipc::rs_start`（约 900 行 per-server 类型化 wire 模块，commit 764d738af，03-stage-rs Fix #81）为建议 (1) 的"按调用族建 wire 结构体"提供了本仓样式模板，实施时应对照该模块的组织方式（wire 结构 + `size_of` 断言 + 解码函数）。(2) **wire 成员清单按 04-stage-pm/todo.md §11.1.1 接线批次表逐批落地**：A 凭证 13 调用（`m_lc_pm_getuid`/`setuid`/`groups`/`getsid` 等）→ B 信号控制 6 → C 时间 6 → D itimer → E exec 3（`m_lexec_pm_exec_new`/`m_rs_pm_exec_restart`）→ F 调度 2（含补 `SEND_PRIORITY`/`SEND_TIME_SLICE` 常量）→ G 杂项 9。(3) A 批次 wire 设计时需决定 gid 载荷宽度以恢复 C 的 GID_MAX 拒绝语义（04-stage-pm/todo.md §11 V2-P3-2，GID_MAX=2^31-1，`minix3/sys/sys/syslimits.h:53`）。
>
> **进度（2026-09-09，V3 轮增补，来源 04-stage-pm/todo.md §12 V3-P3-5）**：双址清单增一处——`os/servers/pm/src/ipc/dispatcher.rs:32` 本地定义 `PROC_EVENT_REPLY: i32 = 0xE80`，而 minix-types 已有同值常量（`os/libs/minix-types/src/ipc/event.rs:27`，带常量锁定测试）。建议 (3) 调用号收敛时一并处置（PM 侧改为消费 minix-types 的常量，与 `VFS_PM_RS_BASE` 经 `dispatcher.rs:28` 的 re-export 先例同型）。另记一条 PM 侧预备建议：40 臂接线期间，pm crate 内建 `ipc/decode.rs` 集中各调用的 unsafe union 解码（每调用一个函数 + C ipc.h 字段对照注释 + 布局断言），E7 wire 落地后该模块改为委托 wire 类型、调用点零改动（04-stage-pm/todo.md §12.3 观察 2，属 PM stage 内工作，此处仅登记衔接关系）。

---

## E8 minix-sys SCHED 所需内核调用真实通电（= 06-stage-sched/todo.md P1-3 抽取，2026-09-06）

**问题**：SCHED 服务器的传输接缝已按最终形态落地（`os/servers/sched/src/kernel_api/transport.rs`：`IpcTransport` 收发 + `KernelApi` 五调用，打包布局与内核对端逐字段对齐），但真实端委托的 minix-sys 直传当前是 stub——`DirectTrapTransport`（`os/libs/minix-sys/src/ipc.rs:529-559`，EIO）与 `DirectKernelCallTransport`（`os/libs/minix-sys/src/syscall.rs:144-148`，-EIO）。主循环全部 79 个测试在 mock 上运行；生产二进制 `minix-sched` 一进 `run()` 即因传输失败计数到上限而 panic（诚实的失败，非静默）。

**证据**（内核对端现状，2026-09-06 核实）：SYS_SCHEDULE（`os/kernel/src/syscall.rs:821`）、SYS_SCHEDCTL（`syscall_process.rs:592`）、SYS_SETALARM（`syscall_clock.rs:160`）、GETMINFO GET_MACHINE/GET_HZ（`misc.rs:1033-1050`，指针 safecopy 契约）全部已实现；缺的只是 trap 层本体。

**影响**：SCHED 的真实通电（启动读机器 → 设闹钟 → 接收分发回复全链）；E5 联调包若含 PM↔SCHED 的 fork 接管链，本条是前置。

**建议**：随 E1 一次解决——trap 层落地后 `DirectTrapTransport`/`DirectKernelCallTransport` 换真实 trap 序列，SCHED 侧零改动（接缝已按最终形态写好）；若 E1 前需要单独验证 SCHED 通电，可在 `os/libs/minix-sys/src/syscall.rs` 复用 `perform_kernel_call` 为 SCHED 五调用加命名包装（仿 E6 模板），但当前 `transport.rs` 的打包已完整、包装层无增量价值，倾向不单独建。

**解锁**：06-stage-sched/todo.md P1-3 的真实通电；E5 的 PM↔SCHED 链路（若立项）。

> **进度（2026-09-16，✅ trap 层本体真机贯通——int-33 双 round + SYSCALL 腿 GET_HZ 全链 PASS，commits 5ea7b76a8/8891e52f9）**：`test-user-trap.sh` 正式验收 PASS——CPL3 payload 经向量 33 门（round-1 未定义调用 → EBADCALL；round-2 KernInfo 未发布 → EBADCALL）+ SYSCALL 腿（LSTAR）GetInfo GET_HZ（hz=100 内核写回、用户读回）三路全通，`DirectTrapTransport`/`DirectKernelCallTransport` 的 trap 序列底座（int 0x21 体 + syscall 体，E1 切片 3+4 已落）经真机验证可用。**同轮修复的内核真 bug**：dm_coverage bump 候选下溢、copy_struct_to_caller 对内核栈地址误用 DM 换算（新增 cross_space_write SELF 语义原语）、syscall stub 推栈顺序与 TrapFrame 契约颠倒、SYSRET +8/+16 锚点约定（GDT 用户段重排 [3]=锚点/[4]=DS/[5]=CS，TSS→[6]）。**E8 剩余**：SCHED 生产二进制真实通电（minix-sched 进 run() 的全链）——传输底座已验证，剩 06-stage-sched 服务器本身参战 E5 联调的场景化验证。

---

## E9 RS 生产接线面：KernelApi 五域面的真实传输（= 03-stage-rs/todo.md §18.10 E-11，2026-09-06）

**问题**：RS 服务器的外部边界 trait 已按最终形态拆为五个域面（2026-09-06 E-2 修复：`SysApi` 内核 8 方法 / `SchedApi` 调度器 2 / `PmApi` PM 进程生命周期 8 / `VmApi` VM 2 / `IpcApi` RS 自身 IPC 2，`KernelApi` 为 supertrait 组合，见 03-stage-rs/01-rs-boot-init.md §3.5），生产实现 `UnimplementedKernelApi` 全部 `Err(ENOSYS)` fail-closed（T7 门 PASS，主循环 `run()` 以 Err 退出而非自旋）。真实传输不存在：trap 层（edge E1）、RS 所需 SYS_* 用户态包装（E2/E6 只覆盖 VM/PM 面）、PM/VM 消息构造面全缺。

**证据**（对端现状，2026-09-06 核实）：内核侧——SYS_PRIVCTL（`os/kernel/src/` 对端已有，do_privctl 语义）、SYS_UPDATE（`misc.rs:1635`，12 步全实现，E2 已列）、SYS_KILL（`syscall_signal.rs:148`，E6 已列）、GETMINFO GET_MACHINE/GET_HZ（`misc.rs:1033-1050`，E8 已列）、SYS_SETALARM（`syscall_clock.rs:160`）；VM 侧——`os/servers/vm/src/rs.rs` 的 RS_PREPARE/RS_UPDATE 对端部分 DEFERRED（02-stage-vm T12/T13）；PM 侧——`srv_fork`/PM_GETEPINFO 对端属 04-stage-pm（部分在建）。

**影响**：03-stage-rs 的 19 号主线（19-rs-external-interfaces.md）通电；E-1（RS 自升级，18 号接线轮）整个链条压在本条的 `PmApi::srv_fork` 上；E5(c) RS live-update 联调链的前置。

**建议**：按 03-stage-rs A5 路线图分域推进，每域一个可独立验证的切片——
1. `IpcApi`/`SysApi` 面：依赖 edge E1（trap 层）+ 在 `os/libs/minix-sys/src/syscall.rs` 复用 `perform_kernel_call` 补 RS 所需 SYS_* 命名包装（SYS_PRIVCTL/GETPRIV/UPDATE/KILL/SETALARM/GETINFO 族，仿 E2/E6 模板，各配 CannedTransport 回放测试）；
2. `PmApi` 面：PM 消息构造（PM_SRV_FORK/PM_GETEPINFO/EXEC 面）——与 04-stage-pm 的对端工作流协同定 wire，`srv_execve` 的 C 实现是 RS 内 ELF 装载 + 内核分配/拷贝 + PM 终步（exec.c:21-64/:102/:127），装载与拷贝可先行（内核调用面），PM 终步随后；
3. `VmApi` 面：VM_RS_MEM_*（com.h:741-745）——依赖 02-stage-vm T12/T13 的 rs.rs 对端；
4. `SchedApi` 面：KERNEL 分支走 SYS_SCHEDCTL（E8 已列对端），SCHED 分支走 SCHEDULING_* 消息（依赖 06-stage-sched 服务器）。
全部落定后逐条回写 03-stage-rs/todo.md §18.10 E-11 与 A5 路线图第 6 步。

> **进度（2026-09-16，🔄 SchedApi KERNEL 分支闭环，commit d58ef9d2e）**：前置核查①——kernel `dispatch_schedctl`（syscall_process.rs:651）已实现（flags 校验/EINVAL、端点解析、KERNEL 分支 -1 哨兵 → Option 转换），满足"对端已存在"条件；②——minix-sys 新增 `sys_schedctl`（MessLsysKrnSchedctl 五域，-1 = 保持当前值），`SCHEDCTL_FLAG_KERNEL=1` 单一权威上收 minix-types com.rs（com.h:782）；Canned 回放 ×2（五域线面 + EINVAL 透传）。**S-6/S-7 复核（同日）**：I-6 已改判"bill_ptr 链路闭环、生产主循环真实可达"（lib.rs:2583/:2606-2664，占位循环仅在 mock 分支）且多轮行为由 test-smp-aps/ipi 实证——但 QEMU 测试族仍无用户进程入 CPL3 的端到端证据，E1 切片 5 通电维持待 boot 链补齐用户态入口测试后同场。**E9 收敛状态**：SysApi 面 ✅ / PmApi 面 ✅ / SchedApi KERNEL 分支 ✅；余 VmApi（挂 02-stage-vm T12/T13）、SchedApi SCHED 分支（挂 06-stage-sched 服务器）、RS 侧 KernelApi 换装（03-stage-rs 域内）。

> **VmApi/SchedApi(SCHED) 分域闭环（2026-09-16，commit d0abe5143）**：minix-sys wrapper 层五件全部落地——VmApi 三件（vm_rs_set_priv_via / vm_rs_memctl_via / vm_rs_update_via，调用号消费 minix-types 单点常量 0xC25/0xC2A/0xC29）+ SchedApi 两件（sched_start_via / sched_stop_via，SCHEDULING_START=0xF02/STOP=0xF03，四域消息构造）。域断言 ×3 入 vm.rs tests。**E9 五域面最终状态**：SysApi ✅ / PmApi ✅ / SchedApi-KERNEL ✅ / VmApi(wrapper) ✅ / SchedApi-SCHED(wrapper) ✅——wrapper 层全就绪；Canned 回放的发送侧断言与 RS 侧 KernelApi 换装（真传输激活）随各对端 stage 通电后补齐（E2/E6 先例：wrapper 先行、通电时零改动）。

> **进度（2026-09-16，🔄 切片 1b PmApi 分域闭环，commit d18be19e1）**：PmApi 面的 minix-sys 消息构造落地——minix-types pm.rs 增 `PM_SRV_FORK=41`/`PM_GETEPINFO=45`/`PM_GETPROCNR=46`（callnr.h:54/:58/:59）与四个 wire 结构（getepinfo 请求/应答 56B LP64：groups@8 对齐垫 padding 44→32；getprocnr 请求/应答；srv_fork 请求沿 message.rs 既有 `MessLsysPmSrvFork`，不重复定义）+ MessageUnion 四臂（`m_rs_pm_exec_restart` 同批补齐——exec.c:128-132）。minix-sys 客户端：`getepinfo_via`/`getnpid_via`/`getnuid_via`/`getprocnr_via`/`exec_restart_via`（`srv_fork` 客户端已有 `service_fork_via`）。Canned 回放 ×4。**对端核查记录**：PM_GETEPINFO 的 PM 侧 handler 为 partial（04-stage-pm V3-P2-4(a) 已修、(b) 组表拷出随 D-30/批次 A/G）——组表参数沿 C getepinfo 默认 NULL/0，PM 侧接通后扩参数；PM_SRV_FORK 侧 PM handler 属批次 G 未落，wire 以 C 为准先行不构成漂移（E7/VFSWIRE 同款纪律）。**余项**：VmApi（挂 02-stage-vm T12/T13）、SchedApi（挂 06-stage-sched 服务器）；RS 侧 KernelApi 换装属 03-stage-rs 域内。

> **进度（2026-09-16，🔄 切片 1 SysApi 面闭环，commit cbb7b9571）**：SysApi 九方法所需 minix-sys 包装齐备——kill/update/setalarm 三个 E2/E6 已有；本轮补六个：`sys_get_machine`/`sys_get_hz`/`sys_get_priv`（GETINFO 通用承载 `sys_getinfo_into` 逐域填 `m_lsys_krn_sys_getinfo`，GET_PRIV 的目标端点走 `val_len2_e` 域——kernel getinfo_priv:925-929）+ `sys_privctl`（M1 request/endpt/arg_ptr）+ `sys_diagctl_stacktrace`（DIAGCTL_CODE_STACKTRACE=2）。`sys_times` 发现已有同语义实现（E6 已落），get_ticks 直接消费，不重复造。六个 Canned 回放测试逐域断言出站线面与应答臂回填（minix-sys 161 全绿）。**余项**：PmApi（PM_SRV_FORK/GETEPINFO wire——与 04-stage-pm 对端协同）、VmApi（VM_RS_MEM_*——挂 02-stage-vm T12/T13）、SchedApi（SYS_SCHEDCTL——E8 对端）三分域仍开放；RS 侧 UnimplementedKernelApi 换装成真实现属 03-stage-rs 域内工作（stage 内），与本 edge 条目的 minix-sys 契约面切割。

**解锁**：03-stage-rs 19 号主线通电；E-1 自升级；E5(c) 联调链。

---

## E-KERNINFO MINIX_KERNINFO 内核信息共享 + release/version 字段（= 01-stage-kernel todo.md I-2，2026-09-07 移交）

**问题**：C 的 `MINIX_KERNINFO=6`（`minix3/minix/include/minix/ipcconst.h:12`）是 `do_ipc` 内的内核信息共享原语——调用者经它取得内核信息表（含 `release[]`/`version[]` 等）；Rust 内核侧未实现（`os/kernel/src/ipc.rs:1775` 自注 "not yet implemented"，`ipc.rs:2061` 测试钉住 `IpcCall::from_raw(6) == None`）。连带 `KernelInfo` 无 `release`/`version` 字段（C 生产者 main.c:432-433，banner 直打 OS_RELEASE 故内核自身无消费）。

**为何 edge**：真实消费方是**用户态进程**（进程初始化时取内核信息页），用户态 trap 层未落地（E1）前无法端到端验证；共享契约面（kerninfo 的 grant/映射机制与 minix-types wire 布局）符合 edge 判定①③。

**解锁后工作**：(1) minix-types 定 kerninfo wire（对照 C `struct minix_kerninfo`/`kinfo` 布局）；(2) kernel `ipc.rs` 增 KernInfo 分支（grant 共享或 safecopy，对齐 C `do_ipc` 该分支语义）+ `KernelInfo` 补 `release`/`version` 填充；(3) 翻转 `ipc.rs:2061` 钉子测试；(4) 用户态首个消费方（libc/服务器初始化）接线后端到端验证。

**依赖**：E1（trap 层）落地后才有用户态调用方；内核侧实现本身可先行（stage 内单测覆盖），但无消费方即无法验证可观察行为——按 todo-fix 规则保持 DEFERRED 登记，不假完成。

> **复核（2026-09-15）**：锚点 +2——ipc.rs:1777（"not yet implemented"）与 :2063（钉子测试）。新增现状：minix-rt 已有用户侧脚手架（init.rs:108-116 `KerninfoSource` trait、:128-136 `DirectTrapSource` 的 `query_kerninfo` 为 Err(EIO) 桩、:145-149 CannedSource 测试源）——内核臂落地后该桩有现成接缝；RS 侧已建模 trap 掩码位（os/servers/rs/src/privilege.rs:152 `MINIX_KERNINFO = 1 << 6`）。
>
> **进度（2026-09-16，🔄 内核臂闭环，端到端挂 E1）**：三项落地——(1) **wire**：minix-types 新建 `types/kerninfo.rs`——`MinixKerninfo` 88B LP64（六 u32 + 八指针，type.h:214-245；i386 为 56B，指针 4→8 自然扩至 88，六个 u32 恰 8 对齐无需补垫）+ `KuserInfo` 16B（type.h:203-208）+ `KERNINFO_MAGIC`/`MINIX_KIF_IPCVECS`/`MINIX_KIF_USERINFO`（type.h:229/:244-245）+ OS_NAME/OS_RELEASE/OS_REV/OS_CONFIG/OS_VERSION 常量（config.h:5-9）+ `strlcpy_fixed` 填充语义（char[6] 截断 + NUL，param.h:42-43/main.c:431-432），布局与截断见证 ×4。(2) **二次返回通道**：`CpuContextArch::set_secondary_ipc_return`（默认 no-op；x86-64 整值写已保存上下文 RBX——C `arch_set_secondary_ipc_return` 即 `p_reg.bx = val`，arch_system.c:184-186；与 IPC status 同寄存器族，消费者各读己方调用的返回值无交叉危害）；kernel `MINIX_KERNINFO_USER: AtomicU64`（globals.rs，哨兵 0 镜像 C glo.h:33 的"未初始化即 0"）。(3) **dispatch 臂**：`IpcCall::KernInfo=6`（ipcconst.h:12）+ dispatch_ipc 顶部臂——未发布 → EBADCALL（proc.c:687-689 行为一致），已发布 → 写通道 + OK（proc.c:691）；翻转钉子测试 ipc.rs `from_raw(6)==Some(KernInfo)`；引擎侧防御臂保持穷尽。测试 ×3（kernel dispatch 两臂 + arch RBX 整值赋值）。**余项裁决记录**：①原文猜测"grant 共享或 safecopy"不成立——C 机制是寄存器返回预映射页地址，无 grant/safecopy 参与（proc.c:685-693 实读）；②`KernelInfo`（minix-boot）补 release/version 字段**不做**——该结构是 boot→kernel 内部载体（含 `&'static [MemoryRegion]` 切片，非 C kinfo 定长布局），加零读者字段违反死代码纪律；填充机制（常量 + strlcpy_fixed）已就位，kerninfo 页装配轮（E1 通电）直接消费。**挂 E1**：kerninfo 页用户映射初始化（C memory.c:900-925 FIXEDPTR 等价物）写 `MINIX_KERNINFO_USER`；minix-rt `DirectTrapSource::query_kerninfo` 桩翻转。**验证**：kernel 757 / minix-types 235 / minix-arch 236 全绿；clippy 零新增（68=68 干净 HEAD 对账）。commit 207e30644。

> **进度（2026-09-17，🔄 内核半闭环——页构建/用户映射/发布落地，commit a071af5e5）**：上方"挂 E1"的内核半清账——`kerninfo::init_kerninfo`（kmain Phase B.5，先于 proc_init 对应 C cstart kuserinfo 填充序）：内核镜像静态 `KerninfoPage`（4KiB 对齐，BklProtected write-once）填充 kuserinfo → 活动根 query 取 PA → 用户只读映射 `KERNINFO_USER_VA=0x2_0000_0000` → 发布 `MINIX_KERNINFO_USER`。ki_flags 只置 MINIX_KIF_USERINFO（无 IPCVECS——64 位 syscall 直入，D5；其余子结构槽位 0 = minix-types wire 契约既定）。**同轮发现并修复 arch 级缺陷**：`walk_alloc` 中间层不传播 USER 位（U/S 逐层 AND → CPL3 必 #PF），VmBootHandoff 页同路径潜伏缺陷一并消除。**真机验收**：test-user-trap round-2 EBADCALL→OK、RBX=页 VA、CPL3 读 KERNINFO_MAGIC 全过（脚本五断言 PASS）；kernel 761 / arch 237 全绿，clippy 57=57、7=7 零新增；28-usermapped-data.md §3.7 增补（D4 部分修正）+ 01-stage-kernel/todo.md I-2 回写。**余项（唯一）**：minix-rt `DirectTrapSource::query_kerninfo` 桩翻转——随 T2（E1 minix-rt 半）收口后本条目闭环。

---

## E-MINTYPES-RS minix-types RS 消息层的三个可重构观察（2026-09-07 扫描登记，低优先）

**来源**：03-stage-rs 扫描轮（03-stage-rs/todo.md §20）P3 共享基建审查。三项均为
"可重构/设计脆弱"级而非 bug，当前唯一消费者是 RS（VM/PM 未来接入 rs 消息族时收益），
故登记 edge 由后续单线程裁决，不阻塞任何 stage。

1. **`Message::is_rs_req_arm` 范围守卫脆弱**（message.rs）：`m_rs_req` 臂归属用
   `RS_RQ_BASE..=RS_RQ_BASE+24` 硬编码范围 + 排除 `RS_INIT`/`RS_LU_PREPARE` 判定。
   新增 RS 调用号超出 +24 时守卫**静默失配**（返回 None 而非编译期报错）。建议：
   改枚举/表驱动（RS 消息号的 union 臂归属在类型层表达），或至少加"新增调用号时
   守卫同步"的编译期断言。
2. **message.rs 单文件 3,831 行**：union 臂约 40+，RS/LSYS/内核/PM/VFS 各族混居。
   按族拆分为 `message/{rs,lsys,kernel,pm,vfs}.rs` 子模块是纯机械重构；动手时机
   建议挂在下一次大批消息臂新增前（避免拆分与新增的冲突窗口）。
3. **COMMON_RQ_BASE 族常量寄居 ipc/rs.rs**：`SIGS_SIGNAL_RECEIVED`（com.h:597）与
   `COMMON_REQ_FI_CTL`（com.h:607）属 common 请求族（非 RS 专属），当前仅 RS 消费
   故寄居 rs 模块可接受；PM/VFS（fi_ctl 的接收方是服务自身）未来消费时应上移
   `ipc/common.rs` 或等价归属。

**解锁后工作**：三项互相独立，均为 minix-types 内部重构（接口不变、调用方零改动
或纯 re-export 调整），各约 0.5 个迭代。无 E1/E2 依赖。

> **复核（2026-09-15）**：观察 1 的守卫仍在（ipc/message.rs:364-368）但 `RS_RQ_BASE` 已提取为共享常量（ipc/rs.rs:14），范围形状（BASE..=BASE+24 排除 RS_INIT/RS_LU_PREPARE）未变——脆弱性判定不变；观察 2 的文件现为 ipc/message.rs 3941 行（继续膨胀，拆分时机与 E7 批量新增协调）；观察 3 不变。

> **进度（2026-09-15，✅ 观察一/三闭环；观察二绑入 E7 执行轮）**：
> **观察 1（守卫表驱动化）**：`is_rs_req_arm` 由数值范围改为显式成员表——13 个具名常量（RS_UP..RS_GETSYSINFO + RS_EDIT/SYSCTL/FI）逐项列名，新增 RS 调用号要么进表（可见的一行决策）要么不是 m_rs_req 消息；未分配孔洞（+10..+19）从"被范围误收"转为明确拒绝（无 C 常量命名即无消息可携带），完备性测试逐偏移钉住 0..=24 的归属（类型化臂/孔洞/成员三类）+ 族外拒绝。验证：`cargo test -p minix-types --lib` 204 passed。
> **观察 3（COMMON 族归属）**：`SIGS_SIGNAL_RECEIVED`/`COMMON_REQ_FI_CTL` 从 ipc/rs.rs 迁至 ipc/event.rs（该模块已持有 COMMON_RQ_BASE/COMMON_RS_BASE 基址——COMMON 族的既成归属地），数值改由基址派生；rs.rs 改 `use super::event::COMMON_REQ_FI_CTL` 消费，外部路径 `minix_types::SIGS_SIGNAL_RECEIVED` 经 ipc glob 保持不变（RS crate lib.rs:776 消费点零改动）。minix-types clippy 回 1 条既有基线。
> **观察 2（message.rs 拆分）**：按条目自身的时机指引（"挂在下一次大批消息臂新增前"），拆分绑入 **E7 执行轮首步**（Wave D-16）——E7 是下一个大批新增方，先拆后增即消除冲突窗口；单独提前拆只会在同一文件上制造两次大 diff。

---

## E-VMMOCK minix-arch default features 泄漏收口 + "mock" 命名澄清（02-stage-vm G-V12-5 余件，2026-09-08 登记）

**问题**：`os/servers/vm/Cargo.toml:18` 以裸 path 依赖引入 `minix-arch`（`minix-arch = { path = "../../arch" }`），绕过了 workspace 表的 `default-features = false`（`os/Cargo.toml:235`）；而 `os/arch/Cargo.toml:18` 的 `default = ["mock"]` —— mock feature 就此泄漏进 VM 生产构建。kernel 与 boot-shim 均已正确关闭默认（`os/kernel/Cargo.toml:13`、`os/boot-shim/Cargo.toml:17`），VM 是唯一漏网点。同时 `mock` feature 实际门控的是"运行时窗口基址变体"（Direct Map 窗口基址是内核动态授予的运行时值，E3 已接真值），命名与生产用途混淆——这是 02-stage-vm/todo.md §16.1 G-V12-5 的原始登记内容，原定"处置归 E3"，但 **E3 的完成注记（2026-09-08）未包含依赖收口**，为防孤儿单列本条。

**建议**：(1) `os/servers/vm/Cargo.toml` 的 minix-arch 依赖补 `default-features = false`，跑三 feature 矩阵回归（G-V12-8 之前这主要影响编译面与 arch 内 mock 项的 dead_code 噪音，预期零行为差异——若有差异即暴露了生产代码误依赖 mock 项，需逐处修正）；(2) arch 侧把 `mock` feature 更名为诚实表达运行时窗口语义的名字（如 `runtime-window`，或直接内联为非 feature 代码路径），同步 kernel/boot-shim 的引用；(3) 在 02-stage-vm/todo.md §2 表 G-V12-5 行回写闭单。

**解锁**：VM 生产依赖面的单一真相；arch 命名与语义一致。无 E1/E2 依赖，可独立先行。

> **进度（2026-09-08，建议 (1) 完成）**：`os/servers/vm/Cargo.toml` 已补 `default-features = false`，三 feature 矩阵回归零差异（490/507/490 passed）——VM 生产代码无 mock 项依赖，收口无行为影响。**余件**：建议 (2) arch 侧 `mock` 更名（涉 kernel/boot-shim 引用同步）与 (3) G-V12-5 行闭单回写（待 (2) 一并完成）。依据记录：02-stage-vm/todo.md §17.9 Fix #62。
>
> **复核（2026-09-15）**：G-V12-5 行已随 V12 归档迁至 02-stage-vm/archive/todo-V12-archive-2026-09-09.md:108（不在当前 todo.md）——余件 (3) 的回写落点改为该 archive 文件；mock 门控点实测约 119 处（集中在 os/arch/src/lib.rs 的三架构三元组），余件 (2) 是机械批量更名 + kernel/boot-shim 引用同步 + 三 feature 矩阵回归。

> **进度（2026-09-16，✅ 余件全闭环，commit 750d0843d）**：(2) arch `mock` feature 更名 `runtime-window`——arch/Cargo.toml default/features 更名 + arch/src 15 文件 119 处 cfg 门控批量更名 + kernel/Cargo.toml 的 `minix-arch` 依赖引用同步；(3) G-V12-5 行在 V12 archive 文件闭单回写。**裁决记录**：kernel 自身 umbrella feature 仍名 `mock`（= minix-arch/runtime-window + minix-plat/mock 级联）——它表达的是 kernel 侧测试姿态（含 plat 半），不属 arch 更名面；kernel/src 三文件（smp/dm_coverage/lib）的 `cfg(feature = "mock")` 门的是该 umbrella，首轮误更名已回退（E0308 类型错位暴露：那些门在 kernel feature 语义下选 arch Mock* 类型）。**矩阵回归**：arch default 236 / kernel 757 / vm 505 全绿；boot-shim 零错误；arch --no-default-features 与干净 HEAD 同为 37 错（既有状态，零回归）；clippy 无新增。

---

## E-MINSYS-HYGIENE minix-sys clippy 卫生项 5 条（03-stage-rs §22 扫描登记，2026-09-09，低优先）

**问题**：`cargo clippy` 全链仅存的 crate 本体告警集中在 minix-sys（5 条）：`misc.rs:219/:220`
与 `rmib.rs:85` collapsible-if（3 处可合并 let 链）、`syscall.rs:308` doc 注释后空行、
`rmib.rs:127` `MountTable` 缺 `Default` impl（已有 `const fn new()`）。另有 minix-types
1 条既有 known（`ipc/vm.rs:667` `VmReply` large-size-difference）与 workspace profile
声明 4 条（kernel/boot-shim 的非根 profile，属 Cargo.toml 归位）。

**为何 edge**：minix-sys 是全 stage 共享的用户态 syscall 层（edge 判定①类共享基建），
且其主要增量工作（E1 trap 层、E2 SYS_* wrapper）尚未发生——卫生清理与功能改动会触碰
同文件，分散做必然产生冲突窗口，合并到 E1/E2 的工作轮顺带清零即可。rs crate 本体
clippy 已零告警（03-stage-rs/todo.md §6.0 基线），本条不影响任何 stage 的正确性。

**建议**：E1 或 E2 动工的首轮，`cargo clippy --fix -p minix-sys` + 手工核对 3 处
collapsible-if 的合并语义（`checked_mul`/`checked_div` 链合并不改变短路行为，但需逐处
确认可读性）+ `MountTable` 补 `impl Default`（委托 `new()`）+ profile 声明上移
workspace 根。验收 = 全 workspace `cargo clippy` crate 本体告警清零（依赖宏展开的
误报除外）。

**解锁**：全 workspace clippy 零告警的 CI 门禁前提（当前"触碰文件零告警"纪律是逐轮
人工维持的，机械清零后可升级为全局门）。无前置依赖，但刻意等 E1/E2 避免冲突。

> **复核（2026-09-15，10-stage-mib 首轮架构审查）**：锚点仍有效——`cargo clippy -p minix-sys` 实测 5 条告警（rmib.rs collapsible-if 与 `MountTable` 缺 `Default` 的建议均仍在）；本轮无新增卫生项，条目维持原判。

> **进度（2026-09-15，✅ 闭单）**：随 E-DSWIRE ds.rs 工作轮顺带清零（同文件纪律如本条所愿，且提前于 E1——grant/ds 两新模块落地时工作区已零告警）。(1) misc.rs:219/:220 双 collapsible-if：**const fn 限制下的改写**——`Option::and_then` 尚非 const-stable，if-let 合并形状不可用，改嵌套 `match`（const 兼容且不触发 collapsible-if）；(2) rmib.rs:85 collapsible-if：let 链合并（Rust 2024 let-chains）；(3) rmib.rs:127 `MountTable` 补 `Default` impl（委托 `new()`）；(4) syscall.rs:444（原 :308）doc 注释后空行删除；(5) ds.rs 新增侧自检（unused Vec 导入/未读赋值随写随清）。(6) **profile 归位**：kernel/boot-shim 的非根 `[profile.*]` 段删除——cargo 对非根 package 直接忽略这些声明（死声明），根 profile 已含 `panic = "abort"`；实测两处删除后 build 行为无变化（`-p minix-kernel -p boot-shim` 组合 check 的 E0152/E0425 为干净 HEAD 既有的 feature 统一化形状，与本删除无关）。**验收口径**：`cargo clippy -p minix-sys` 本体告警 5 → **0**，workspace clippy 清单中 minix-sys 消失（minix-types 1 条为登记在案的 known）；原验收句"全 workspace 清零"当前被他 crate 既有告警（kernel 44/arch 7/ds 6/init 82 等——E-MINSYS-HYGIENE 登记后各开发轮累积，非本条范围）阻塞，如实登记不假完成。**验证**：minix-sys 136 passed；types/kernel/vm/rs/ds/is/mib/fs 九 crate 全绿。

---

## E-VMTLB kernel 侧目标进程 TLB 刷新机制缺失（02-stage-vm V13 轮登记，2026-09-09）

**问题**：VM 服务器直接写进程硬件 PTE 之后（Fix #60 起故障路径、更早起 munmap/brk/fork 路径），C 对"目标进程在其他 CPU 上的陈旧 TLB 翻译"有显式机制，Rust 内核没有任何对应物：

- C 机制：进程级 `MF_FLUSH_TLB` 标志 + 调度点刷新——`minix3/minix/kernel/proc.c:345-347`（`if (p->p_misc_flags & MF_FLUSH_TLB && get_cpulocal_var(ptproc) == p) tlb_must_refresh = 1`，随后 `switch_address_space(p)` 重载根寄存器）。
- Rust 现状：`rg "MF_FLUSH_TLB|tlb_must_refresh" os/kernel/src` 零命中；且调度器 `switch_address_space` 带"同根跳过重载"优化（os/kernel/src/syscall.rs:2410-2417 注释自述 mirror 比对语义——镜像必须跟上每次根变更，否则首次分发会无谓重载）。
- 风险窗口：真 SMP + 共享页场景（fork 后 CoW 父子页、shm 多映射方）——VM 翻 RO/解除映射时，另一 CPU 上仍在运行的共享方进程的 TLB 保留旧翻译（RO 旧项使写持续故障、已 unmap 旧项使进程继续写已回收物理页）。当前单核 + "VM 只改非运行进程 PTE"的结构纪律（IPC 阻塞 + RTS 停止）下不可达；E5 通电若开 SMP 即暴露。

**为何 edge**：机制主体在 kernel（01-stage 生产代码，edge 判定②），与 VM 的 PTE 写路径（02-stage）协同才可验收（edge 判定③）。

**解锁后工作**：(1) kernel 进程结构补 MF_FLUSH_TLB 等价标志 + 调度点刷新（对照 proc.c:345-347）；(2) 与 switch_address_space 的同根跳过优化对账（刷新标志必须在比较之前生效）；(3) 同步评估 C VM 侧四处 `sys_vmctl(SELF, VMCTL_FLUSHTLB)`（pagetable.c:119/255/319/430，宿主 pt_assert[SANITY]/vm_freepages×2/vm_pagelock[MEMPROTECT]）——它们依附 C 的"进程内存别名映射进 VM 地址空间"模型，minix-rs 的 direct map 下翻译恒定、VM 自刷不需要（该 ARCH 偏差在 02-stage-vm/todo.md §18.2 V13-P2-1(b) 登记）；(4) E5 若含 SMP 配置，冒烟须覆盖"fork 后父子并发写 CoW 页"。

**依赖**：无 E1/E2 硬依赖（机制可先行 + 单测），但验收（E5 SMP 冒烟）前置 E1/E2；与 01-stage-kernel 的 SMP 工作窗（smp_gpt 系列设计迭代）协同排期。

> **复核（2026-09-15，现状更新）**：「grep 零命中」已失效——`MiscFlagsBits::FLUSH_TLB` 已定义（proc.rs:181/:206 别名）且被 `vmctl_vminhibit_set` 设置（syscall.rs:2327），但全内核无消费者（dispatch/restore 路径不读该标志；lib.rs:2582-2584 自注 single-CPU parity 省略；vmctl_vminhibit_clear 注释 :2336 自述 "stale TLB fill not yet implemented"）。同根跳过优化本体已迁至 lib.rs:2534-2555（:2550-2552）。剩余工作不变：调度点消费（刷新先于同根比较）+ 设置点对账 + 单测。

> **进度（2026-09-15，✅ 机制半闭环；SMP 拓扑验收仍挂 E5）**：C 机制三件套全数落地——(1) **pick 点判定**（C proc.c:345-347）：scheduler_loop 在 proc_ptr 记录之后、switch_address_space 之前（C 同序，因后者会改写 ptproc）计算 `tlb_must_refresh = picked.needs_tlb_refresh(current_ptproc_nr())`，新 KProcess 方法钉住"旗标置位 **且** 是本 CPU ptproc"的双半判据；(2) **switch_to_user 消费**（C proc.c:458-464）：finish_and_restore step 5 由"CONFIG_SMP-only 省略"改为实装——`consume_flush_tlb_flag` 助手（闭包注入 flush 动词，宿主可测旗标生命周期，proc_cr3 注入同型）在旗标置位且常驻时执行 `TlbArch::flush_all()`（C refresh_tlb 等价物，arch 已有），旗标无条件清除；(3) **设置点**：vmctl_vminhibit_set 本地路径已设旗标（先在），对账完成。**裁决记录**：C 的旗标设置与消费整体 `#if CONFIG_SMP`，C 单核构建根本不设此位；Rust 的 vmctl 设置是无条件的（先在事实），若消费侧继续缺席，旗标将成为只增不清的死位——故消费无条件实装，与 C-SMP 行为一致；对用户代码语义透明（TLB 刷新只改变缓存哪些翻译，不改变翻译的值），不构成外部行为偏差。**余件**：(a) SMP IPI 路径（schedule_vminhibit）的旗标设置完备性 + 真拓扑验证归 E5 SMP 冒烟（"fork 后父子并发写 CoW 页"用例）；(b) C VM 侧四处 `sys_vmctl(SELF, VMCTL_FLUSHTLB)` 的评估——按 02-stage-vm §18.2 V13-P2-1(b) 既定 ARCH 偏差（direct map 下翻译恒定、VM 自刷不需要）不移植，已登记。验证：`cargo test -p minix-kernel --lib` 751 passed（新增 ×4：判定两半 ×3 场景 + 消费生命周期 ×3 场景）；clippy 44=44 零新增；既有 finish_and_restore 三测试穿参 `false` 保持宿主安全。

---

## E-REQWIRE REQ_* VFS↔FS 消息契约双侧独立定义（05-stage-vfs R2 轮登记，2026-09-09）

**问题**：VFS 侧与 FS 驱动侧对同一 REQ 协议各自独立定义，之间无任何编译期或测试期联动：

- VFS 侧：`os/servers/vfs/src/request.rs:15` `FS_BASE=0x600`（**错值**，C 为 0xA00，止血见 05-stage-vfs/todo.md §9.2 R2-P0-1）+ `enum FsReq` 32 变体（request.rs:122）+ `m_type()` 编码（:318）。
- FS 侧：`os/libs/minix-fs/src/protocol.rs:25` `FS_BASE=0xA00`（正确）+ `enum RequestNumber`（:58）+ `TransactionId`（:226）；mfs 经 `use minix_fs::protocol::RequestNumber` 消费（os/fs/mfs/src/table.rs:12），pfs 同样引用 minix-fs。
- `os/servers/vfs/Cargo.toml` 不依赖 minix-fs（grep 零命中）；两侧数值当前纯靠人工对齐——偏移对齐（26=Lookup 两侧一致）掩盖了基址分叉，R2-P0-1 就是该模式的第一个实际事故。transid 侧机制相容（VFS fs_comm.rs:30-56 的 0xB00/0xB01 与 minix-fs TransactionId 的 `(type<<16)|id` 打包），但 id 语义未联动约束。

**为何 edge**：edge 判定①共享契约/基础设施层——minix-fs 是 FS 驱动族（mfs/pfs 已消费）的共享 crate，REQ wire 是 VFS 与全部 FS 驱动的双向契约；收敛动作落在单个 stage 之外。

**解锁后工作**：
1. 方案 A（推荐）：REQ face 迁 minix-types（先例：PM↔VFS 面 `minix-types/src/ipc/vfs.rs`）——`FS_BASE`/`REQ_*` 常量、请求/响应类型化编码收一处；vfs 与 minix-fs（可 re-export）共消本地定义；沿用 rs_start_t 先例的 offset_of 布局见证 + 编译期断言。
2. 方案 B（最低限度）：保留双侧定义 + 全量对账测试——断言 `FsReq::m_type()` 与 `RequestNumber` 逐项相等、`FS_BASE` 相等、transid 打包一致。
3. 附带：request.rs:120 的 "33 variants" 计数注释随迁移/对账修正（05-stage-vfs todo R2-P3-1）。

**依赖**：无硬前置（方案 B 的对账测试立即可写）；执行顺序与 05-stage-vfs R2-P0-1 协调——先在 VFS 侧止血基址，再做本条结构收敛。验收挂钩 E5：VFS↔mfs 第一条真实 REQ 往返（REQ_READSUPER）即本条验收面。

> **复核（2026-09-15，数值半已愈）**：R2-P0-1 已修（05-stage-vfs/todo.md §9.9 Fix #1，2026-09-09）——vfs request.rs:21 与 minix-fs protocol.rs:25 均 0xA00，两侧各有 C 绝对值 pin 测试（vfs `test_fs_wire_values_match_c_absolute` :569 / minix-fs protocol.rs:474-497）。剩余为**结构半**：仍无双侧逐项对账测试、vfs 不依赖 minix-fs、devman 第三消费方（见下方增补）待接；"33 variants" 注释与实际 32 变体的计数修正随迁移一并做。

> **进度（2026-09-15，✅ 闭单，方案 A 执行）**：新建 `minix-types::ipc/fs_driver.rs` 单一权威——FS_BASE（com.h:589）+ NREQS（vfsif.h:75）+ 33 个 REQ_*（vfsif.h:42-73，逐个带锚点）+ `is_fs_rq`（vfsif.h:77），wire 域统一 **i32**（Message::m_type 同域）；33 常量绝对值全量 pin 测试 + is_fs_rq 门测试（0x600 旧事故基址/0xB00 transid 带/设备 RS 命名空间区分）。两侧切换：(1) vfs request.rs 删 33+3 本地定义改 import；`m_type()`/`is_known`/`FsError::UnknownReq` 签名 u32→i32（wire 域统一，全部消费点在模块内）；33 对消费侧 pin 保留（防 VFS 编码接错常量）；(2) minix-fs protocol.rs：FS_BASE/NREQS 改 re-export（REQUEST_TABLE_SIZE 别名保留）、VFS_ENDPOINT 改派生 `Endpoint::VFS.get()`、新增 RequestNumber 33 对联动钉子（双侧任一漂移即编译期后首测即爆）。方案对比：方案 B 双侧对账测试（已被本方案吸收为消费侧 pin）；Redox/Linux 对照同 E-MINTYPES-SYS。**验证**：vfs 360 / minix-fs 90 / mfs 98 / pfs 12 / types 201 全 passed；vfs clippy 0 告警、types/minix-fs 基线持平。**devman 第三消费方**：落地时直接 import 本模块（E-DMWIRE 指针已声明以本条裁决为准）。**回写**：05-stage-vfs/todo.md §0 指针行。

> **增补（2026-09-15，11-stage-devman 首轮架构审查）**：devman 是本契约的**第三消费方**——其内联 VTreeFS（os/servers/devman/src/vtreefs/mod.rs:50-56）自注"生产分类器（raw IPC → Request）需要 VFS 侧 wire 布局（fsdriver_data、REQ_* 字段宏），lands with the IPC transport"，dirent/stat 的 wire 编码同样声明"属传输层"（mod.rs:41-42/:198）。该传输层就是本条的 REQ_* 契约域：REQ 收敛到 minix-types（方案 A）后，devman 的分类器与 dirent/stat 编码直接消费同一权威，不再自建。登记于 E-DMWIRE 第 2 缺，wire 权威以本条裁决为准。

---

## E-SCHEDNICED kernel 侧 SYS_SCHEDULE 丢弃线上 niced 字段（06-stage-sched V2 轮登记，2026-09-09）

**问题**：C 的 do_schedule 从 SYS_SCHEDULE 消息读 niced（`niced = !!(m_ptr->m_lsys_krn_schedule.niced)`，minix3/minix/kernel/system/do_schedule.c:27），sched_proc 据此写 MF_NICED（system.c:692-694）；该标志在时钟中断的 CPU 记账分类中消费（minix3/minix/kernel/arch/i386/arch_clock.c:318 `else if (p->p_misc_flags & MF_NICED) counter = CP_NICE`）。Rust 内核的 dispatch_schedule 读出线上 niced 后丢弃、硬编码 false（os/kernel/src/syscall.rs:923 `let niced = false;`），注释把理由归于"SYS_NICE is not yet wired up"——但 **SYS_NICE 在 C 树中不存在**（rg 全树仅 system.h:12 的 2005 年变更日志提到 nice(2) kernel call），C 的真实数据源就是 SYS_SCHEDULE 消息本身。SCHED 服务器每次 fanout 都诚实发送 niced（os/servers/sched/src/kernel_api/schedule.rs:152-154 `wire_niced`，值取 `is_niced(max_priority)`，同 C schedule.c:319 公式）；kernel 侧 sched_proc 的落盘机制已备（os/kernel/src/sched.rs:413-423 Step 8 写 `MiscFlagsBits::NICED`，测试 sched.rs:649-657）——断环只在 dispatch_schedule 一行。另有下半：Rust 的 clock.rs 记账没有 CP_NICE/CP_USER/CP_SYS 分类（rg "niced" os/kernel/src/clock.rs 零命中），即使传值也无人消费——修复是"接线 + 消费端评估"两半。

**影响**：niced 进程的 CPU 记账归入 CP_USER 而非 CP_NICE（utilization/cpuavg 统计失真）；无调度行为影响（C 的 MF_NICED 不参与抢占与队列决策）。

**为何 edge**：kernel 生产代码（01-stage-kernel 域，edge 判定②）；SCHED 侧 wire 半已正确、零改动。

**解锁后工作**：
1. dispatch_schedule 改 `let niced = sched.niced != 0;` 传入 sched_proc（os/kernel/src/syscall.rs:923），删除"SYS_NICE"注释、改引 do_schedule.c:27（连带修正 sched.rs:286-294 SchedParams 文档里对 "SYS_NICE" 的同款虚构引用）；
2. 评估 clock.rs 记账是否补 CP_NICE 分类（对照 i386 arch_clock.c:315-319 的 counter 选择链）；若 Rust 记账面尚无此分类，先登记不实现——MF_NICED 位本身先正确落盘；
3. 测试：dispatch_schedule 收 niced=1 的 SYS_SCHEDULE → 目标进程 NICED 位置位（sched_proc 半已有测试，缺 dispatch 半的翻转）。

**依赖**：无 E1/E2 依赖（纯内核侧行为 + 单测可验）。
**解锁**：12 篇契约（06-stage-sched）的 niced 半闭环；06-stage-sched/todo.md P2-3 指针表的对应行。

> **复核（2026-09-15，下半已闭合）**：clock.rs 记账半已落地——`CP_NICE`（clock.rs:451）、`classify_cpu_state` 读 `MiscFlagsBits::NICED` 归桶（:479，测试 :1527）、`sched_proc` 真实置/清 MF_NICED（sched.rs:418-421，测试 :649）。剩余仅上半接线：dispatch_schedule 的 `let niced = false;`（syscall.rs:923）与 `let _niced = sched.niced;`（:893）换真值；`classify_cpu_state` 尚无生产调用点（lib.rs 生产路径只记 CP_INTR）——接线时一并评估，不接则登记理由。sched.rs:286-294 的 "SYS_NICE" 虚构引用仍在。

> **进度（2026-09-15，✅ 闭单）**：dispatch_schedule 删 `let niced = false;` 硬编码与 `_niced` 丢弃绑定，改 `let niced = sched.niced != 0;`（C do_schedule.c:27 `!!` 布尔强转的直译面；数据源就是 SYS_SCHEDULE 消息本身——C 无 SYS_NICE 内核调用，仅 system.h:12 的 2005 年变更日志提及）；sched.rs SchedParams 的 "future SYS_NICE" 虚构 rationale 重写为 do_schedule.c:27 + do_schedctl.c:37（SCHEDCTL 路径恒 FALSE）双锚点。测试新增 `test_dispatch_schedule_niced_wire_bit_sets_mf_niced`（SYS_PROC caller + wire niced=1 → 目标进程 MF_NICED 置位，旧代码下失败）。classify_cpu_state 生产接线评估：**登记不实现**——分类器与 CP_NICE 桶已备，但生产记账路径（lib.rs 调度循环只记 CP_INTR）的结构与 C 的 arch_clock counter 选择链不同，接线属 clock 记账重构窗口；MF_NICED 位现已从 wire 端到端正确落盘，符合原条目"先登记不实现"的预授权。验证：`cargo test -p minix-kernel --lib` 747 passed；clippy 44=44 零新增。
> **非 edge 观察（不顺手修）**：`test_sched_proc_niced_flag_set_and_clear` 在过滤单独运行时因全局 CLOCK_STATE 未初始化而 panic（lib.rs:1538 read_tsc 路径）——干净 HEAD 复现，全量运行时靠 clock 测试先行初始化才通过，属顺序依赖的既有脆弱性；修复归 01-stage-kernel 自己的 todo（clock 测试全局装配纪律），不属 edge。

---

## E-PREEMPTFLAG PREEMPTIBLE 特权标志缺失——priority != 0 近似当成了语义（06-stage-sched V2 轮登记，2026-09-09）

**问题**：C 有两处调度决策以特权标志 PREEMPTIBLE 为门：(1) quantum 耗尽只对"用户调度 + PREEMPTIBLE"进程通知调度者（minix3/minix/kernel/proc.c:1895 `if (!proc_kernel_scheduler(p) && priv(p)->s_flags & PREEMPTIBLE)`）；(2) enqueue 抢占只对 PREEMPTIBLE 的当前进程生效（proc.c:1638）。Rust 内核两处都以 `priority != 0` 近似（os/kernel/src/proc_table.rs:766 `let pre = p.get_priority().get() != 0;`；:674（enqueue Phase 3 元组第三元素的 `cur.get_priority().get() != 0`））；连 sched.rs:233 的 `is_preemptible` 助手——文档声称"C: priv(p)->s_flags & PREEMPTIBLE"——实现也是同一优先级近似，且 `#[allow(dead_code)]` 未接线。近似与 C 语义的分叉点：MAX_USER_Q == TASK_Q == 0（minix3 config.h:67-68，用户进程可合法登顶队列 0），SCHED 接管的进程经 START/NICE 可达 priority 0（os/servers/sched/src/scheduling/start.rs:116 的门放行 0..15）——此后 quantum 耗尽走"内核调度者续量"分支，**SCHED 永远收不到 NO_QUANTUM，该进程被无限续量**，MLFQ 的降级臂与平衡回升臂对它双双失效；C 下它仍走 notify_scheduler。

**影响**：priority-0 的 SCHED 接管进程脱离用户态调度策略（不降级、无需回升——它从未降过）；enqueue 抢占对 priority-0 的当前进程永不发生（同因）。单核现状即可达，非 SMP 专属。

**为何 edge**：kernel 生产代码（01-stage-kernel 域，edge 判定②）；PRIV_TABLE 的标志位归 kernel 域。

**解锁后工作**：
1. priv 表补 PREEMPTIBLE 位（对照 C 的 USER_PRIV 模板：用户进程默认置位、内核任务缺省；落点 os/kernel/src/priv_table.rs + 进程初始化路径）；
2. `is_preemptible` 改读特权标志（os/kernel/src/sched.rs:233），接线到 sched_proc_no_time（proc_table.rs:766 替换优先级近似）与 sched_enqueue Phase 3（:674）；
3. 文档修正：sched.rs:231 的声称与实现对齐（消除"文档说 priv、代码读优先级"的漂移）；
4. 测试：priority-0 + 用户调度 → proc_no_time 走通知分支（现测试 test_proc_no_time_user_scheduled_preemptible 用 USER_Q=7，未覆盖 0）；enqueue 抢占对非 PREEMPTIBLE 当前进程的抑制。

**依赖**：无 E1/E2 依赖（特权位 + 单测可先行）。
**解锁**：SCHED 的 NO_QUANTUM 契约对所有优先级成立；E5(e) 冒烟的"断环分界"用例转正。

> **复核（2026-09-15，前提已变）**：PREEMPTIBLE 位**已存在**——capability.rs:69-70（0x0000_0002，已入 SRV_F/USR_F 模板）+ kpriv.rs:164-173 的 `KPrivFlags::is_preemptible()`（`#[allow(dead_code)]`，注释自述 "scheduler preemption check not yet wired"）+ KPriv 包装（:491-495）。原「步骤 1 priv 表补位」作废；余下：两处优先级近似（proc_table.rs:679 enqueue Phase 3、:771 sched_proc_no_time，锚点从 674/766 漂移）改读特权标志、sched.rs:232-233 助手接线并修文档漂移、补 priority-0 + 用户调度用例与非 PREEMPTIBLE 抑制用例。

> **进度（2026-09-15，🔄 live 半闭环）**：NO_QUANTUM 门（sched_proc_no_time，C proc.c:1893-1910，经 check_quantum ← 调度主循环 lib.rs 生产可达）已完整修复——新增 `ProcessTable::preemptible(nr, &PrivTable)` 助手（镜像 lib.rs is_billable 的查表形状：priv_id → priv_table.get → `KPriv::is_preemptible()`，缺 priv 即不可抢占），`&PrivTable` 穿参（is_billable/process_misc_flags 先例），`priority != 0` 近似删除。方案裁决：全局 priv_table 追踪（否——测试用本地表而全局表为空，既有 NO_QUANTUM 测试将被迫变异全局状态致并行互扰）；KProcess 缓存位（否——第二真相源 + 约 10 处 priv 挂接点的同步义务）。sched.rs 死助手 is_preemptible（"文档声称 priv、实现读优先级"的漂移源）删除。测试：新增 ×2（priority-0 + USR_F → 通知，钉住 MAX_USER_Q==TASK_Q==0 的合法可抢占；TSK_F 缺 PREEMPTIBLE → 续量不发通知）+ 既有 ×2 补 priv 构造。验证：`cargo test -p minix-kernel --lib` 746 passed；clippy 44=44 零新增。回写：06-stage-sched/todo.md §2。
>
> **余项登记（enqueue Phase 3 抢占门，proc_table.rs:669-686）**：本轮执行发现该分支**生产不可达**——全部调用方（rts_unset:433、requeue_if_preempted:lib.rs:2381）传 `current_nr=None`，而 C 的 enqueue() 自己读 CPU 本地 proc_ptr（proc.c:1633）恒有 current。原条目"enqueue 抢占对 priority-0 当前进程永不发生（同因）"据此修正：抢占从不被评估，不只是标志源错。激活需两件同做：(a) current 来源改读 CpuLocal.proc_ptr（SMP 工作窗，与 E-SCHEDSMP 协同）；(b) 抢占门消费特权标志（proc.c:1638）。不先穿参修死分支的理由：27+ 站点的投机性签名泛化服务于一个今天不可能执行的分支，且 (a) 的设计可能重排该签名。本条目维持 open（🔄）至 Phase 3 激活。

---

## E-SCHEDSMP SCHED cpu 下发链的 SMP 三件套缺口（01-stage-kernel SMP 工作窗，06-stage-sched V2 轮升级登记，2026-09-09）

**问题**：06-stage-sched 第一轮 todo 的"边界外观察"第 2/3 条（观察无去向即丢失的实例），本轮升级登记。SCHED 服务器的 CPU 选择与下发（10/09 篇）在内核侧断三环：
1. 每核运行队列不存在——`sched_for_cpu`/`sched_for_cpu_mut` 恒返 BSP 队列（os/kernel/src/proc_table.rs:573-586，TODO 自注）；SCHED 下发的 cpu 字段被 dispatch_schedule 存入 p_sched.cpu（sched.rs:407-411）但 pick/enqueue 不按它分发；
2. EBADCPU 不可达——sched_proc 的 CPU 校验是桩（os/kernel/src/sched.rs:355-360 `let _ = params.cpu;`；C: system.c:650-654 的 cpu_is_ready 检查），SCHED 服务器侧的 EBADCPU 重试环（os/servers/sched/src/server.rs:391-399，C schedule.c:227-231）在当前内核上永不触发；
3. 跨 CPU 迁移未接线——sched.rs:308-315 文档声称"dispatch_schedule calls schedule_migrate_proc when the CPU changes"，但 dispatch_schedule 无该调用（C: system.c:673-677 `p->p_cpu != cpuid && cpu != -1 && cpu != p->p_cpu → smp_schedule_migrate_proc`）；schedule_migrate_proc 本体已实现（os/kernel/src/smp.rs:699）。

**影响**：单核下全部不可观察（cpu 恒 0、单队列恒正确）；多核落地时 SCHED 的负载均衡与死核规避（pick_cpu/balance 的全部产出）对内核无效。

**为何 edge**：kernel 生产代码（01-stage-kernel 域，edge 判定②），且与 01-stage SMP 工作窗（smp_gpt 系列设计迭代）协同排期。

**解锁后工作**：随 01-stage-kernel SMP 落地一并——(1) per-CPU Scheduler 入 CpuLocal，sched_for_cpu 按 cpu_id 分发；(2) sched_proc 补 cpu_is_ready 校验（EBADCPU）与迁移调用（对照 system.c:650-654 / :673-677）；(3) SCHED 侧零改动（重试环已按最终形态写好并有测试 `test_start_retries_after_dead_cpu` 钉住）；(4) 联调验收挂 E5(e)。

**依赖**：01-stage-kernel SMP 工作窗；E5(e) 验收。
**解锁**：06 篇重试环与 10 篇 pick 的生产语义；S-11 组合层在全拓扑下的行为完备。

---

## E-MINTYPES-SYS SYS_* kernel-call 调用号常量 minix-types 缺位（06-stage-sched V2 轮登记，2026-09-09，低优先）

**问题**：kernel-call 调用号（C com.h:210-262 一族）目前多处各自表达：minix-types 无 SYS_* 常量（grep 零命中）；SCHED 服务器本地镜像两个（os/servers/sched/src/kernel_api/transport.rs:41 `SYS_SCHEDULE = 0x600 + 3`、:45 `SYS_SCHEDCTL = 0x600 + 54`，注释自述 "minix-types' absence"）；kernel 侧以枚举成员相对 KERNEL_CALL 表达（os/kernel/src/syscall.rs:69 `Schedule = 3`、:114 `Schedctl = 54`）。minix-sys 的 SYS_* wrapper 家族（E2/E6/E8）落地时还会再添消费方。与 06-stage-sched/todo.md P2-3（SUSPEND 本地定义）同型：com.h 常量一个走共享镜像、一个各处手抄。

**影响**：数值当前有 wire 断言测试钉住，无漂移事故；成本是每新增一个 SYS_* 消费 crate 就多一份手抄对账对象。

**为何 edge**：minix-types 是共享契约层（edge 判定①）；收敛动作跨 kernel/sched/未来 crate。

**建议**：minix-types 补 SYS_* 调用号常量模块（对照 com.h:210-262 全族，先例：types/com.rs 的 SCHEDULING 系常量镜像），kernel 枚举与 SCHED 镜像改消费 + 删本地定义；与 P2-3 的 SUSPEND 上移合并为一轮 minix-types 常量收敛。

> **增补（2026-09-14，07-stage-ds 首轮架构审查）**：DS 段两件同型并入本轮收敛——(a) `SI_DATA_STORE = 5`（sysinfo.h:13）现本地定义于 os/servers/ds/src/getsysinfo.rs:27，11 篇 D2 自注「minix-types 暂无 sysinfo 模块（A-1 余部）」；(b) `NOTIFY_MESSAGE = 0x1000`（com.h:92，is_notify 的常量基）现硬编码于 os/servers/ds/src/dispatch.rs:94-96 的 is_notify 实现。DS 侧消费面（getsysinfo.rs / dispatch.rs）随 minix-types 落地改走共享常量并删本地定义。
>
> **现状更新（2026-09-15，10-stage-mib 首轮架构审查复核）**：(a) **半解**——minix-types 已建 sysinfo 模块并有 `SI_DATA_STORE`（os/libs/minix-types/src/ipc/sysinfo.rs:137-138），但 ds 侧本地定义仍在（getsysinfo.rs:26-27）且 server.rs:1058 消费的是本地常量，切换未做；(b) **维持**——minix-types 仍无 `NOTIFY_MESSAGE` 常量（仅 ipc/notify.rs:28 文档注释提及），ds dispatch.rs:94-96 仍内联 `0x1000`。

**依赖**：无。
**解锁**：06-stage-sched/todo.md P2-3；E2/E6/E8 wrapper 的常量消费面。

> **复核（2026-09-15，主体半解）**：minix-types `ipc/sysinfo.rs` 已有 SYS_* 常量模块——SYS_GETINFO/SYS_DIAGCTL/SYS_SETALARM/SYS_TIMES/SYS_SAFECOPYFROM/SYS_SAFECOPYTO/SYS_VSAFECOPY/SYS_SETGRANT/SYS_EXIT/SYS_STATECTL/SYS_SAFEMEMSET（:17-59）+ SI_DATA_STORE（:138）。「minix-types 无 SYS_* 常量」主体判定作废，剩余收敛面：SYS_SCHEDULE/SYS_SCHEDCTL 等缺号对照 com.h:210-262 补齐；NOTIFY_MESSAGE 仍无常量（仅 ipc/notify.rs:28 注释提及）；四处本地定义/内联待切换删除——sched transport.rs:41/:45、ds getsysinfo.rs:27（server.rs:1058 消费本地常量）、ds dispatch.rs:95 内联 0x1000。

> **进度（2026-09-15，✅ 闭单）**：新建 `minix-types::ipc/kernel_call.rs` 作为 KERNEL_CALL 全族单一权威（com.h:204-269：KERNEL_CALL 基 + 46 个 SYS_* 常量含缺口注释 + NR_SYS_CALLS + SYS_BASIC_CALLS 自 sysinfo.rs 迁入；Linux uapi/Redox scheme-number 同型——一处定义全树消费），sysinfo.rs 改为词汇域纯模块（模块 doc 声明迁址，ipc glob re-export 保持 `minix_types::SYS_*` 路径不变）；NOTIFY_MESSAGE = 0x1000 落 ipc/notify.rs（com.h:90，无符号回绕判定的告诫随 doc）。四处本地定义全数切换删除：sched transport.rs 双镜像改 import、sched dispatch.rs 的 SUSPEND 本地定义删除（minix-types ipc_server.rs:76 既有权威 + com.h:1151 钉子，P2-3 就此闭环）、ds getsysinfo.rs 改 re-export（server.rs 消费点零改动）、ds dispatch.rs is_notify 改消费 NOTIFY_MESSAGE。新增 kernel 联动钉子：46 对枚举成员 ↔ 共享权威全量断言（枚举保留相对判别式为 kernel 内部惯用法，设计裁决记录于测试 doc）。验证：types 201 / rs 333 / sched 81 / ds 111 / kernel 752 / is 106 / mib 150 全 passed；clippy 基线持平（types 1、sys 5、ds 6 既有）。**附带修复（编译阻断，非邻近 TODO）**：mib.rs:376 测试构造缺 head_flags/head_csize/head_clen 三字段（分支上既有，干净 HEAD 复现，阻断 types 测试构建）——按 decode_register 透传语义补默认零值。**回写**：06-stage-sched/todo.md P2-3 + §2。

---

## E-DSWIRE DS 传输面三缺：minix-sys 客户端模块、服务器 transport 通电、联调零覆盖（07-stage-ds 首轮架构审查登记，2026-09-14）

**问题**：DS 判定层（plan/apply/verdict，os/servers/ds 18 文件 3572 行、81 个在跑测试）已成形，但执行半整层缺席，三个落点分属不同域：

1. **minix-sys 无 DS 客户端模块**（edge 判定①）。C 的 libsys/ds.c（minix3/minix/lib/libsys/ds.c，219 行，18 个 API：ds_publish_u32/str/mem/label、ds_retrieve_u32/str/mem/label_name/label_endpt、ds_delete_u32/str/mem/label、ds_subscribe、ds_check，加上 do_invoke_ds 的 grant 生命周期 ds.c:7-33）是**全体 DS 消费方的公共入口**（RS manager.c:513,800、VFS main.c:441 + misc.c:960-985、input.c:665、storage/filter main.c:385、i2c.c:452、IS dmp_ds.c）。minix-sys 现有 13 个模块 6284 行（vm/vfs/rs/pm/socket 等）但**无 ds.rs**（ls os/libs/minix-sys/src/ 实测）。plan.md:182 的 A-8 描述已过时（"minix-sys 是 stub，sendrec/notify 为 todo!()"——现 grep todo!/unimplemented 零命中）。服务器侧契约镜像已就位（os/servers/ds/src/client.rs：key_grant 尺寸/方向 ds.c:13-19、terminate 钉尾 ds.c:84/:155、flags 组装、CheckReply 回信复用 ds.c:209-219），缺的只是 minix-sys 的传输半。附带：os/servers/ds/Cargo.toml 声明了 minix-sys 依赖但 src 零引用（grep 零命中，死依赖——A-8 落地时转正或删除）。
2. **DS 服务器 transport 未通电**（edge 判定②+①复合）。main.rs:27-28 是空转 loop；C 主循环（main.c:45-88：get_work → is_notify 拒绝 → 七路分派 → reply/EDONTREPLY）的收发装配、get_key_name 的 sys_safecopyfrom（store.c:167-172）、do_getsysinfo 的 sys_datacopy 整表搬运（store.c:672-675）、update_subscribers 的 ipc_notify 发送（store.c:222）、sef_cb_init_fresh 的 rproctab grant 拉取（store.c:267-269）——Rust 侧全部只有判定半（dispatch::triage、plan_*/apply_*），缺 seam trait + 装配线（SCHED 先例：IpcTransport + KernelApi 双 trait + MockIpc/MockKernel，os/servers/sched/src/server.rs）。其中 sef_receive/send 的用户态 wrapper 归 minix-sys（E1/E2 面），safecopy/datacopy 的内核对端归 kernel（E2 家族）——两处对端就绪前，DS 侧以 seam + mock 达到「逻辑完备」，真实通电挂本条。
3. **联调零覆盖**（edge 判定③）。C 的行为契约 minix3/minix/tests/ds/（dstest.c 178 行 + subs.c 91 行）无 Rust 承接 → E5(f)。

**影响**：服务器不能服务（空转自白，main.rs:25-26 注释「must not pretend otherwise」）；全部消费方无客户端库可用；与 A-2 叠加时 VFS 同款订阅场景（main.c:441）双缺同根。

**为何 edge**：① minix-sys DS 模块是 VFS/PM/RS/驱动/IS 全体消费方的共享契约层；② safecopy/datacopy/notify 的内核对端是 01-stage-kernel 生产代码；③ 多进程联调。DS stage 内生产代码（seam trait、装配线、mock 测试）不属本条——留 07-stage-ds/todo.md P1-4。

**建议**（执行序）：
- 方案 A（推荐，沿 SCHED 先例）：07-stage 先落 transport seam（IpcTransport 形 trait + 脚本化 mock 测试，stage 内 P1-4）；随后 minix-sys 补 ds.rs（消息构造 + `_taskcall` + grant 生命周期，复用 client.rs 三契约）；E1/E2 通电后接真实传输收尾本条。
- 方案 B（否决）：minix-sys ds.rs 只做纯函数消息层、传输无限期留置——Rust 侧消费方（VFS/IS 重写）仍无法编译，违背「各 stage 可独立完整实现」。
- 传输基建上移评估（06-stage-sched/todo.md V2 §4.1 预留的触发点）：DS 将成为第 4 个用户态服务器 seam 消费者；本轮判定**暂不上移**（四台服务器的 seam 形状各异且 DS 面最窄，纯 receive/send + safecopy），等 E1 trap 层落定后随本条执行时再评估一次公共 UserSpaceTransport 抽取。

**依赖**：seam + mock 阶段无依赖；真实传输前置 E1/E2。
**解锁**：07-stage-ds/todo.md P1-4 执行半；E5(f) 联调；A-2 regex 决策的端到端验证面；minix-sys 死依赖处置（ds crate Cargo.toml）。

> **进度（2026-09-15，07-stage-ds 执行轮）**：DS 侧 seam 已落地（`server.rs` 双 trait + 七臂 + run/run_once，`heap.rs` A-3 固定池），真实端 `SysIpc` 委托 minix-sys `IpcTransport`（E1 通电即活）、`SysKernel` 三拷贝动词 EIO 诚实桩（等 minix-sys 补 SYS_SAFECOPY*/SYS_DATACOPY 包装）；minix-types MessageUnion 已补 `m_ds_req`/`m_ds_reply` 两臂（IS 轮同款先例）。ds crate 的 minix-sys 死依赖已转正（server.rs 真实端消费）。**剩余**：minix-sys ds.rs 客户端模块（libsys/ds.c 18 API）、E1/E2 通电、E5(f) 联调。
>
> **进度（2026-09-15，ds.rs 客户端模块 ✅；真实通电仍挂 E1）**：minix-sys 补齐两件——(1) **`grant.rs` 用户态授权表**（C libsys safecopies.c 的重写：slots+freelist+生长即注册，`grant_direct`/`revoke` 纯表编辑不陷阱；槽布局 `cp_grant_t` 下沉至 `minix-types::types::grant` 单一权威——kernel `grant.rs` 改消费 re-export，用户态库与内核读同一 wire 布局，E-REQWIRE 纪律）；(2) **`ds.rs` 客户端**（`DsClient<T>` 18 API：publish_label/u32/str/mem、retrieve_u32/label_endpt/str/mem、delete（四 arm 归一）、subscribe、check——键 grant 方向/长度按 ds.c:13-19（CHECK/RETRIEVE_LABEL 写入 80 字节房间，其余 READ strlen+1），值 grant 覆盖 publish_raw/retrieve_raw，check 应答骑请求车道（ds.c:215-216））。测试：grant 表生命周期 ×4（布局/回收+序列号/非法 access/注册载荷）+ ds 回放待 E1 后补端到端（现以 grant 表测试覆盖关键半）。**mib_get_label 消费点**（remote.rs:204-209）的执行半随本条可接。**剩余**：E1/E2 通电、E5(f) 联调。
>
> **增补（2026-09-15，10-stage-mib 首轮架构审查）**：新增消费方——MIB 服务器的 `mib_get_label`（C remote.c:88 `ds_retrieve_label_name`）是远程子树注册的第一步；os/servers/mib/src/remote.rs:204-209 目前只有 label 界判定（ENAMETOOLONG），DS 查询执行半随本条 minix-sys ds.rs 落地后接线（10-stage-mib/todo.md P1-1 的 `MibServices` seam 消费）。

> **增补（2026-09-15，11-stage-devman 首轮架构审查）**：新增消费方——devman 驱动侧客户端的 `init`（minix-sys/src/devman_client.rs:135-137，注入闭包形式）与 RS 侧 `RsTransport::devman_endpoint`（os/servers/devman/src/rs_contract.rs:24-26）都依赖 `ds_retrieve_label_endpt("devman")`（C generic.c:193 / manager.c:841/898）；两处的生产接线随本条 minix-sys ds.rs 落地（E-DMWIRE 第 4 缺消费）。

---

## E-ISWIRE IS 生产 transport 接线：minix-sef/minix-sys 替换 fail-closed 占位（08-stage-is V1 轮登记，2026-09-14）

**问题**：IS 的生产传输面全部未接——`os/servers/is/src/main.rs:17-23` 用 `IsServer::new(UnimplementedTransport, UnimplementedFkeyCtl)` fail-closed（运行即 panic，注释自述接线 pending）；`SefTransport` 的 `startup/receive/send`（os/servers/is/src/sef.rs:128-139）与 `FkeyCtlTransport`（os/servers/is/src/tty_fkey.rs:178-182）只有 panic 占位实现。依赖侧：`os/libs/minix-sef` 是 5 行占位 crate（`sef_startup`/`sef_receive` 的 ping 透明拦截逻辑——A-11 的 `sef.c:208-214` 语义——无着落）；`minix-sys` 的 `_taskcall` 属 E2/E6 wrapper 家族。

**影响**：IS 无法出生（main 直接 panic）；86 个单测全部跑在 fake 上，生产语义零验证。

**为何 edge**：minix-sef/minix-sys 是共享基建（edge 判定①）；SEF ping 拦截与 `_taskcall` 由多个 server 共享，IS 只是首个深度消费方。

**解锁后工作**：(1) minix-sef 实装 `sef_startup`/`sef_receive`（含 SEF_PING_REQUEST_TYPE 拦截应答，对照 `minix3/minix/lib/libsys/sef.c:150,208-214` + `sef_ping.c:21`）；(2) minix-sys `_taskcall` 可用（依赖 E1/E2）；(3) 写 `SefTransport`/`FkeyCtlTransport` 生产 impl 并替换 main.rs 占位（IS 侧是 stage 内生产代码，随本条目一并做）；(4) 重跑 `cargo test -p minix-is` 基线对照 86 passed + 复原 V1-P2-1 删掉的 minix-sys 依赖。

**依赖**：E1（trap 层）、E2（SYS_* wrapper）。
**解锁**：E-ISBOOT；08-stage-is/todo.md V1-P2-1 的复原。

> **进度（2026-09-15，🔄 (1) 框架核 3/4 闭环）**：`minix-sef` 从 5 行占位实装为 SEF 接收循环库——`SefIpc` trait 注入 receive/notify 动词(生产 = minix-sys `IpcTransport`,E1 通电即活;宿主测试 = `CannedSefIpc` 脚本);`sef_receive_status`(C sef.c:149-260)实装:循环 receive → `is_ipc_notify`(CALL_NOTIFY 低 16 位)→ 按源分类——SYSTEM→`SefEvent::Signal` 上浮(服务器分派自己的 signal handler)、RS+`NOTIFY_MESSAGE`→ping 拦截(`sef_cb_ping_reply_pong` 回 pong 后 continue 吞掉,sef_ping.c:21-38)、普通消息返回 `Call`;协议常量 SEF_PING/SIGNAL/INIT_REQUEST_TYPE 对齐 sef.h:32/:122/:263。测试 ×4(ping 拦截吞掉+普通消息上浮/SYSTEM 信号/RS 非通知直通/错误直通)。**剩余**:(2)(3) `_taskcall` 与 IS/MIB/devman 三消费方的生产 impl 替换(依赖 E1/E2,本条目保持 open 至通电);LU/ST 拦截路径(C `INTERCEPT_SEF_LU_REQUESTS`/ST)依赖 LU/ST campaign,登记不实装。**验证**:minix-sef 4 passed;is/mib/devman/sys/types/kernel 全回归通过;clippy 基线持平。

> **增补（2026-09-15，10-stage-mib 首轮架构审查）**：minix-sef 的第二消费方浮出——os/servers/mib/src/sef.rs:13-14 声称 SEF transport 留在 minix-sef，但 mib crate 的 Cargo.toml 未声明该依赖；MIB 侧 `sef_startup`/`sef_receive_status` 接线（10-stage-mib/todo.md P1-1 的 `MibIpc`，含 status 字的 notify 检测——与 DS 的 call-number 猜测路径不同）随本条 (1) 一并做。

> **增补（2026-09-15，11-stage-devman 首轮架构审查）**：minix-sef 的**第三消费方**是 devman——`SefHooks` trait（os/servers/devman/src/hooks.rs:268-271：init_server/on_signal）与 `SefLifecycle` 枚举（:257-263）目前只有测试替身实现（hooks.rs:336-365），生产实现被 STATE.md OQ-1 挡在 minix-sef 门前（hooks.rs:255 自注 "minix-sef is currently a stub"）。C 参照是 vtreefs.c:54-59 的三注册 + sef_local_startup（devman 侧经 libvtreefs 间接消费 libsys/sef）。devman 侧 `SefHooks` 生产 impl 随本条 (1) 一并做；若 minix-sef 长期不到场，11-stage-devman/todo.md DM-P1-2 的分派面统一将连带评估该 trait 的去留（模式 80：无生产实现者的占位抽象）。

> **MIB/devman 接线收口（2026-09-16，✅ 不依赖 A-6 的捆绑件全部落地，commit b756afc9e）**：三件——①**MIB 接收路径接入 `minix_sef::sef_receive_status`**：mib crate 补 minix-sef workspace 依赖；`MibIpc` 增 `notify` 动词（SysIpc→transport.notify，测试 Mock 记录）；`run_once` 的接收半改走 SEF 库——RS ping（NOTIFY 通知 + `SEF_PING_REQUEST_TYPE`）在库内 pong 并吞掉，永不到达 triage（此前 RS ping 会落进 NotifyRefusal 被**静默丢弃且不回 pong**，RS 永远等不到活性应答——SEF 活性探测语义缺陷，此为其一）；`Server` 的 run/run_once impl 块收紧 `I: MibIpc + SefIpc` 界。②**status_is_notify 宏语义 C 修正**：原实现 `(status − 0x1000) < 0x100` 是 com.h:94 `is_notify`（m_type 带），而 C mib main.c:449 调用的是 com.h:93 `is_ipc_notify`（`IPC_STATUS_CALL(status) == NOTIFY`，低 0x3F 位 == 4，ipcconst.h:10/21-22）——宏张冠李戴修正为 `(status & 0x3F) == CALL_NOTIFY`。③**devman 生产 `SefHooks`**（`DevmanSef`）：init_server = `Server::new` 重建框架+树（C init_hook/init_inodes；restart 在 C 注册为 `SEF_CB_INIT_RESTART_STATEFUL`——状态随 RS 镜像恢复，回调体为空，本 hook 仅 fresh 触发）；on_signal 仅 SIGTERM 锁存 `terminate` 停机旗（C got_signal，vtreefs.c:39-46），+生产行为测试。**新测试**：MIB ×2（RS ping pong+吞掉/ ping 后真实调用照常应答）、devman ×1（重建+SIGTERM 锁存）。**验证**：sef 4 / mib 152（+2）/ devman 80（+1）/ is 106 / kernel 759 全绿；clippy 触及 crate 对账持平（5=5）。**E-ISWIRE 剩余**：(3) 的 IS main.rs 生产替换（挂 A-6 诊断通道裁决）+ MIB/devman 的 main 装配随各自 P1-6 传输窗口。

---

## E-ISPROD GETSYSINFO/GET_*/VM_INFO producer 布局与 IS 快照对齐（08-stage-is V1 轮登记，2026-09-14）

**问题**：IS 六组 `#[repr(C)]` 快照（`KProcSnap`/`KPrivSnap`/`MProcSnap`/`FProcSnap`/`DmapSnap`/`RprocpubSnap`/`RprocSnap`/`DsEntrySnap`/`Vm*Snap`，A-4 wire 契约提案）与现存 producer 是双源，其中 kernel 侧已坐实二进制不兼容：kernel `ProcInfoStruct`（os/kernel/src/misc.rs:371 起，GET_PROCTAB 生产布局：p_nr 打头、时间字段 u64、含 p_misc_flags/p_cpu/p_cpu_time_left/p_cycles/p_pending）与 IS `KProcSnap`（os/servers/is/src/dump_kernel.rs:28：p_rts_flags 打头、时间字段 i32、无上述五字段）字段序、字段集、类型三重不一致。PM/VFS/RS 的 `do_getsysinfo` 生产侧布局同样待对账（IS 侧 6 个 TODO(P1) 注释：dump_pm.rs:13、dump_vfs.rs:13、dump_rs.rs:13、dump_ds.rs:13、dump_vm.rs:12、dump_kernel.rs:16）；DS 生产侧已有模块（os/servers/ds/src/lib.rs:70）。附带一处常量双源：os/servers/vfs/src/misc.rs:66 本地定义 `SI_PROC_TAB: u32 = 2`，与 minix-types 权威（i32）重复。

**影响**：`run_dump` 填体（08-stage-is/todo.md V1-P1-2）后，IS 按 `KProcSnap` 解释 producer 拷来的字节会全盘错位——这是行为级事故而不只是卫生问题。

**为何 edge**：对端 stage 的生产代码（edge 判定②）：kernel `do_getinfo` 的输出布局、PM/VFS/RS/DS 的 `do_getsysinfo` payload、VM 的 `vm_info` 三结构都归各自 stage 所有权。

**解锁后工作**：(1) 先裁决快照权威：上收 minix-types 单一权威（kernel `ProcInfoStruct` 迁入 + IS 删 `KProcSnap` 改 import）vs 各 crate `#[repr(C)]` 对齐约定 + wire 断言测试互钉（两案需在 02-stage-vm/04-stage-pm/05-stage-vfs/03-stage-rs/07-stage-ds 各 todo 留同步指针）；(2) kernel GET_PROCTAB/GET_KINFO/GET_IMAGE/GET_PRIVTAB/GET_IRQHOOKS/GET_IRQACTIDS/GET_MONPARAMS/GET_MACHINE 八臂布局对齐；(3) PM/VFS/RS/DS/VM 五 producer 逐一对齐 + 删 VFS 本地 SI_PROC_TAB；(4) 双侧补布局断言测试（`size_of` + 字段偏移，minix-types tty.rs:103 的 `_ASSERT_MSG_SIZE` 先例）。

**依赖**：无硬依赖（裁决可先行）；实施建议在 E-ISWIRE 之后（有真实 IPC 才能端到端断言）。
**解锁**：08-stage-is/todo.md V1-P1-2 的实施半（设计半不依赖本条）。

> **进度（2026-09-16，🔄 proc-tab 面 1/2 闭环）**：快照权威裁决落地（方案 A）——`ProcInfoStruct` 上收 `minix-types::types::proc_info` 单一权威（repr(C) 104 字节布局见证 ×offset/size + 手写 Default 保留 p_priv_id=-1 哨兵），kernel `misc.rs` 改消费（`ProcInfoBuild` 扩展 trait 承载 KProcess 读取半，14 个消费点零改动），IS `dump_kernel.rs` 的 `KProcSnap` 删除改 import（字段三重错位自此消除：顺序/字段集/宽度以 kernel 生产者为准），`acquire.rs` seam 签名随迁，IS fixture 用 `..Default::default()` 适配。(3) 的 VFS 半：`SI_PROC_TAB`/`SI_DMAP_TAB`/`SI_CALL_STATS`/`SI_PROCLIGHT_TAB` 本地副本删除改 re-export（minix_types i32 权威，u32 消费视图显式 cast）。
> **进度（2026-09-16，✅ kernel 四臂 2/2 闭环，commit 987773bb3）**：GET_PRIVTAB/GET_IRQHOOKS/GET_IMAGE/GET_KINFO 四臂同型上收——`minix_types::types` 新增 `priv_info.rs`（PrivInfoStruct 56B 布局见证 + s_proc_nr=-1 哨兵 Default；宽度以 kernel 生产者为准：u32 flags/mask、u64 ipc_to）、`irq_hook.rs`（C kernel/type.h 48B 全镜像 + irq=-1/proc_nr_e=NONE 哨兵）、`boot_image.rs`（C type.h:148-154 全镜像 40B；PROC_NAME_LEN 复用 `boot` 常量避 glob 歧义）、`kinfo.rs`（C kinfo 尾部子集 48B：freepde_start/user_sp/vir_kern_start i64 + nr_procs/nr_tasks i32 + release/version[6]）。**GET_KINFO 语义修正**：原 M4 寄存器编码（五字段进消息寄存器）背离 C struct-copy 契约且无法携带 release/version——改 `KinfoStruct` 结构拷贝（release/version 自 minix_types::OS_RELEASE/OS_VERSION 填充，char[6] NUL 截断语义）。**顺带修复**：getinfo_proc_tab/getinfo_priv_tab 的 chunked 拷贝对内核栈局部用 `virt_to_phys`（E8 同类 DM 误用活 bug）——改 `write_to_process_vmcheck`（copy_struct_to_caller SELF 语义）。IS 侧：四个 Snap 删除改 import，dump 适配（s_flags/s_trap_mask i16→u32 字符编码、s_ipc_to u64 拆双 %08x 字、policy/notify_id u32→u64）、TODO(P1) 删除。**余项**：PM/VFS/RS/DS/VM 五 producer 对账（未建者由 E-MIBPROD 跟踪，建时直接消费 minix-types）；GET_MACHINE/IRQACTIDS/MONPARAMS 无 IS 消费方，无对账对象（待消费方出现再评估）。**验证**：types 257（+5 布局见证）/kernel 759/is 106 全绿；clippy 三 crate 对账持平（70=70）。

---

## E-ISKMESS A-3 的 GET_KMESSAGES 等价通道：kernel 侧新子请求（08-stage-is V1 轮登记，2026-09-14）

**问题**：C 的 `kmessages_dmp` 经 `.usermapped` 直读内核消息环形缓冲（`get_minix_kerninfo()->kmessages`，dmp_kernel.c:71）；minix-rs 64 位不移植 `.usermapped`（01-stage-kernel/28-usermapped-data.md 既定），04 篇 A-3 设计定为「新增 `GET_KMESSAGES` 等价 `sys_getinfo` 子请求（kernel 拷贝环形缓冲）」。现状：kernel 侧无该子请求、minix-types 无 GET_KMESSAGES 常量（grep 实测双零命中）；IS 侧 `KerninfoTransport::kmessages_available` fail-closed panic（os/servers/is/src/acquire.rs:195-199）。消费面已备好：`KmessagesSnap`/`kmess_start`/`KMESS_BUF_SIZE`（dump_kernel.rs:99-110/264-272）。

**影响**：F7（内核消息转储）永远不可用；acquire.rs 的 fail-closed 是 86 测试里唯一锁住「通道缺失」的活 panic 点。

**为何 edge**：kernel `do_getinfo` 新增子请求是 kernel 生产代码（判定②），minix-types 增常量是共享契约（判定①）。

**解锁后工作**：(1) minix-types `ipc::sysinfo` 增 `GET_KMESSAGES` 常量（值需对照 kernel com.h GET_* 未用号段裁决，缺号注释体例同 04 篇 §2.1）；(2) kernel `do_getinfo`（os/kernel/src/misc.rs）增臂：拷贝 kmessages 环形缓冲到调用方（布局含 km_next/km_size + 缓冲体，对照 05 篇 §4.1 快照）；(3) IS `KerninfoTransport` 填实（建议签名扩为数据出口形态，与 08-stage-is/todo.md V1-P1-2 的方案①同型）；(4) F7 链路单测。

**依赖**：E2（`_kernel_call` wrapper）；与 E-KERNINFO（kerninfo 共享家族）交叉引用——按 04 篇既定决策走 sys_getinfo 子请求，不依赖 MINIX_KERNINFO 映射机制。
**解锁**：08-stage-is DumpId::Kmessages 臂。

> **复核（2026-09-15）**：IS 侧锚点更新——fail-closed panic 现于 acquire.rs:289（should_panic 测试 :644-645）；kernel 侧与 minix-types 的 GET_KMESSAGES 仍双零命中（仅 syscall.rs:2591/2604-2609/3828 关于 kmess 缓冲已移除的注释）。

---

## E-ISBOOT IS 启动链与端到端联调：条件启动 + RS 动态加载 + TTY 观察者（08-stage-is V1 轮登记，2026-09-14）

**问题**：全仓库零消费方引用 minix-is（`rg -rln "minix_is|minix-is" os` 实测仅 crate 自身与 Cargo.lock）——IS 没有任何启动路径。C 侧完整链是三件：rc 条件启动（`etc/rc.minix:117` `up -n is -period 5HZ`，仅 `sysenv debug_fkeys != 0`）、RS 运行时加载 + 动态分配 endpoint（A-9，无 IS_PROC_NR）、TTY 侧 fkey 观察者（`drivers/tty` keyboard.c，02 篇 §2.7/§2.8 契约）。minix-rs 侧三件均无着落：rc/system.conf 等价物不存在、RS 动态加载服务的能力归 03-stage-rs campaign、TTY crate 未 staging。

**影响**：IS 的外部行为（唯一的存在理由）零验证；86 个单测只覆盖库内语义。

**为何 edge**：多进程联调（判定③）+ 对方 stage 生产代码（RS 加载面、TTY 观察者面，判定②）。

**解锁后工作**：(1) E5 联调包内立 IS 用例：起 IS → 模拟 TTY notify → F-key 位图 → dump 输出断言；(2) RS 动态加载 IS 的注册面（依赖 03-stage-rs campaign 的 service 加载能力）；(3) rc/system.conf 等价物的 debug_fkeys 条件启动语义（归集成配置层，随 E5 定载体）；(4) TTY crate 落地后补 `do_fkey_ctl` 真实对端（现 FakeTty 镜像退役）。

**依赖**：E5（端到端联调包）、E-ISWIRE、03-stage-rs 动态加载；TTY 观察者面依赖 TTY staging（无条目，遥领）。
**解锁**：IS 外部行为验收（A-11 的 `-period 5HZ` 存活 ping 端到端）。

---

## E-RMIBWIRE minix-sys rmib 客户端协议半整缺（10-stage-mib 首轮架构审查登记，2026-09-15）

**问题**：`os/libs/minix-sys/src/rmib.rs`（259 行）只有簿记半——常量（RMIB_MAX_SUBTREES/STACKBUF/FLAG_AUTH，rmib.rs:24-44）、稀疏查找判定（:70-107）、`MountTable` 槽占用位（:116-179）；`MountTable` 仅 `used: [bool; 16]`，缺 C 槽三元组的另外两半（挂载路径 `rno_name[CTL_SHORTNAME]` + 根节点 `rno_node`，rmib.c:53-57）。C `libsys/rmib.c`（1089 行）的协议半整缺，分四组：(a) 注册/注销/重注册——`rmib_register`/`rmib_deregister`/`rmib_reregister`/`rmib_send_reg`（rmib.c:888-980,862-882，`asynsend3(MIB_PROC_NR, AMF_NOREPLY)` 驱动，注册失败 panic :881）；(b) 请求处理——`rmib_process`（:1036-1089，来源门禁 :1044 + COMMON_MIB_INFO/CALL 分派 + REPLY 回信 :1082-1084）、`rmib_call` 下行遍历（:678-824）、`rmib_query`/`rmib_describe`（:267-380/:414-475）、打包层 `rmib_copyout_node`（剥 CTLFLAG_SPARSE :209-214）与 `rmib_copyout_desc`（:324-380）；(c) grant 拷贝族——`rmib_copyout`/`rmib_vcopyout`/`rmib_copyin`/`rmib_readwrite`（:94-188,652-669，sys_safecopyto/sys_safecopyfrom/sys_vsafecopy）；(d) 纯函数层三件理论可先行但未提供——`rmib_inrange`/`rmib_getoldlen`/`rmib_getptr`（:64-87,:482-509）。消费方：C 侧 ipc（main.c:91/:112/:249）、lwip（mibtree.c:56/:61/:66 + bpfdev.c:136 + rtsock.c:102 + lwip.c:348）、uds（stat.c:174/:185 + uds.c:1408）；Rust 侧 os/servers/ipc-server 死依赖 minix-sys 且文档声称复用 `MountTable` 但代码零引用（`ipc-server/src/mib_tree.rs:16` 注释声称、:18 实际 import 仅 minix-types；13-stage-ipc/03 篇 §2.5/§3 D4/§4.1 三处同款失真，需 13-stage 侧同步修正）。

**影响**：13-stage-ipc 的 kern.ipc 远程注册、17-stage-net 的 lwip/uds 子树注册与转发应答全部无客户端库可用；MIB 服务器侧远程子树挂载（10-stage-mib 12 篇）无对端；E5(g) 的 rmibtest 契约链无承载。

**为何 edge**：edge 判定①——minix-sys 是全体 RMIB 消费方（ipc/lwip/uds/未来驱动）的共享契约层；客户端与服务器的 COMMON_MIB_* 消息构造两侧目前各自为政（客户端 rmib.rs 甚至未 import 这些常量，服务器 remote.rs 用布尔参数替代消息类型判定 remote.rs:179-187），收敛必须落在共享层。

**建议**：
- 方案 A（推荐）：rmib.rs 就地扩协议半，两层推进——纯函数层先行（rmib_call 下行遍历/rmib_query/rmib_describe 的判定可单测，sysctlnode 打包复用 10-stage-mib/todo.md P1-3 的布局锚定产物；`MountTable` 扩为三元组槽，客户端树由各服务自持、槽存路径 + 根索引）；传输半（asynsend3/sendrec wrapper + grant 拷贝）挂 E1/E2 通电。CannedTransport 回放测试对齐 E2/E6 先例。
- 方案 B（否决）：各消费服务自建注册/应答逻辑——C 把 rmib.c 做成公共库正是因为三个服务共用同一协议，复制 ×3 违背单一真值，且 13/17 两个 stage 并行实现必然漂移。

**依赖**：E1（asynsend3/sendrec wrapper 真实通电）；10-stage-mib/todo.md P1-3（sysctlnode/sysctldesc 布局锚定先行）。纯函数层无硬依赖。
**解锁**：13-stage-ipc 03 篇接线及其文档失真修正；17-stage-net lwip/uds 注册面；10-stage-mib 12 篇的远程挂载对端；E5(g) 联调。

> **进度（2026-09-15，🔄 纯函数基础层 1/2 闭环）**：sysctl 协议常量地基 + rmib.rs 拷出原语落地——(1) `minix-types::sysctl_abi` 补缺失常量并消与 `types::sysctl` 的双定义（执行中发现该文件已有全家族常量，当场去重——双 glob 歧义即 E-REQWIRE 退役的双真相模式）；(2) `minix-sys::rmib` 新增 `RmibOldp`/`RmibNewp`/`RmibCall`/`RmibNode`（C rmib_oldp/newp/call/rmib_node 形状，rmib.c:24-31 + rmib.h:22-90）+ `rmib_inrange`/`rmib_getoldlen`/`rmib_copyout`（钳制三态：全量/尾部截断/窗口外不动，rmib.c:97-126）/`rmib_copyout_node`（版本戳+SPARSE 剥离+immediate 分派+PRIVATE 可见性门+NODE 特则 csize/clen/NODE_FN，rmib.c:200-260）/`rmib_copyout_desc`（PRIVATE 跳过返回 0，rmib.c:355-364）/`rmib_lookup`——拷出动词闭包注入，wire 语义宿主全测。(3) **余项 2/2（下轮继续，同条目）**：`rmib_call` 下行遍历（rmib.c:678-824）与 `rmib_register`/`rmib_deregister`/`rmib_reregister`（rmib.c:862-994，asynsend3 半）+ `MountTable` 槽扩展（名称+根引用）——遍历与注册簿记一次性对齐 C 560 行遍历为宜。**验证**：minix-sys 149 passed（+5 纯函数测试）；clippy 本体零告警。
>
> **状态修正（2026-09-16，c7d2ea150）**：上方「余项 2/2」已闭环——`rmib_call` 下行遍历（rmib.c:678-824：根查找未注册 ERESTART/前缀拼名/grant 拷入名字/元标识符 QUERY|DESCRIBE|CREATE|DESTROY 分派/PRIVATE 门/叶后 ENOTDIR/写权限门 READWRITE·ANYWRITE/函数驱动 EOPNOTSUPP 登记）+ `rmib_read`/`rmib_write`/`rmib_readwrite`/`getptr` 数据面（rmib.c:482-668，临时缓冲防半途毁值/长度按类型校验/串自补 NUL）+ `register`/`deregister`/`reregister` 簿记（EEXIST/ENOMEM/ENOENT，消息产出与 asynsend3 发送分离——发送归 E1）。grant 拷入拷出经 `RmibIo` trait 注入（宿主内存版可测）。**条目代码工作全部完成**：剩余仅 E1 通电后的端到端验收（挂 E5(g) rmibtest 链）。条目标题「纯函数基础层 1/2」同步作废——2/2 的遍历与注册簿记已在 c7d2ea150 完成。

> **复核（2026-09-15，前置解除）**：10-stage-mib P1-3 已闭环（minix-types::sysctl_abi：SysctlNode 96B/SysctlDesc 16B/KinfoLwp 128B/KinfoProc2 680B，offset_of 钉死，196 passed）——纯函数层的 sysctlnode 打包锚定已就绪，可立即开工；P1-4（transport 双 trait seam）亦已闭环（126 passed）。本条现状核实：rmib.rs 仍簿记半（无 rmib_register/rmib_process/rmib_call），MountTable 仅 `used: [bool; 16]`；传输半仍挂 E1。

---

## E-MIBPROD MIB 快照消费面 vs kernel/PM/VFS producer 布局对账（10-stage-mib 首轮架构审查登记，2026-09-15）

**问题**：MIB 的进程信息五篇（16~20）消费三张跨服务快照（proc_tab/mproc_tab/fproc_tab，C proc.c:34-38），producer 与 consumer 双侧现状错位：
(a) kernel 侧 GET_PROCTAB/GET_PRIVTAB 已实现 chunked 整表拷贝（os/kernel/src/misc.rs:894/:934，`ProcInfoStruct` 布局 misc.rs:371 起，与 E-ISPROD 对 IS 的警告同源）；MIB 侧快照结构未定（P1-3 裁决前无法对账）。
(b) PM 侧 getsysinfo 的 SI_PROC_TAB 数据路径 fail-closed ENOSYS（04-stage-pm todo D-29 已知），SI_PROCLIGHT_TAB 无 producer。
(c) VFS 侧 fproc_tab producer 无着落。
(d) 服务查询面对端未建/未核实：auth 的 `getnuid`（PM，main.c:265-268）、CTL_VM 子树的 `vm_info_stats`/`vm_info_usage`（VM，vm.c 依赖）、`svrctl(PMGETPARAM)`（PM）；DS label 查询归 E-DSWIRE。
C 的消费侧语义：`update_tables` 每 tick 至多一次 + 失败闩锁（tabs_valid=FALSE 不再重试）+ magic 校验（PMAGIC/MP_MAGIC）——Rust 判定半已备（os/servers/mib/src/proc/tables.rs:35-73），执行半缺（stage 内 P1-5）。

**影响**：P1-5 表拉取执行半无数据源；通电后若双侧布局未对账，MIB 按猜测布局解释 producer 字节 = E-ISPROD 对 IS 警告的同型行为级事故（字段序/字段集/宽度三重错位）。

**为何 edge**：edge 判定②（kernel/PM/VFS 的生产代码归各自 stage）+①（快照布局权威裁决跨 stage，与 E-ISPROD 的裁决是同一次决策）。

**建议**：布局权威裁决与 E-ISPROD 合并一次做（方案 A 上收 minix-types 单一权威 repr(C) 结构 vs 方案 B 各 crate repr(C) + 跨 crate 布局断言互钉——沿用 E-ISPROD 已列两案，不另立第三案）；PM/VFS producer 缺位由本条跟踪、在 04/05 各自 todo 的对应条目闭合后划账。MIB 侧消费结构随 10-stage-mib/todo.md P1-3 裁决产出。

**依赖**：10-stage-mib/todo.md P1-3（布局裁决先行）；无 E1 硬依赖（布局断言测试可先行）。
**解锁**：10-stage-mib/todo.md P1-5（表拉取执行半）；E5(g) 的进程信息用例；E-ISPROD 的裁决复用。

> **进度（2026-09-16，裁决半落地）**：proc_tab 快照权威已随 E-ISPROD 裁决为 `minix-types::types::proc_info::ProcInfoStruct`（104 字节，kernel 生产者与 IS/MIB 消费者共用）；mib 消费面的 `proc_tab` 结构落位时直接 import 该权威，不再等独立裁决。mproc_tab/fproc_tab 的 PM/VFS producer 仍随各自 stage 跟踪。

> **复核（2026-09-15）**：10-stage-mib P1-3 已闭环，但产出范围是 **sysctl ABI**（minix-types::sysctl_abi 四结构）——proc_tab/mproc_tab/fproc_tab 的快照布局裁决仍开放，随 E-ISPROD 合并轮一并定；P1-4（transport seam）与 P1-5（Tables 拉取状态机，seam 半）亦已闭环——P1-5 的生产数据源仍随本条对账后接通。

---

## E-MIBGRANT kernel grant.rs magic-grant 门端点常量与 C 不符（10-stage-mib 首轮架构审查登记，2026-09-15）

**问题**：`os/kernel/src/grant.rs:293` `const VFS_PROC_NR: i32 = 4`、`:295` `const MIB_PROC_NR: i32 = 8`，而 C com.h:59-78 的权威值是 VFS=1、MIB=7（MIB_PROC_NR 见 com.h:66）。两值自首次提交（e1d2c4977）即如此，非中途改动；全仓其余各处一律 C 原值（minix-types `types/endpoint.rs:61-72`、kernel `proc.rs:68-118` proc_nr 模块、pm/vm/vfs/ds/sched/ipc-server 各侧）——单点偏差，非系统性重编号。消费点 `grant.rs:546` 的 magic grant 门 `granter.0 != VFS_PROC_NR && granter.0 != MIB_PROC_NR`（对照 C do_safecopy.c:221 同一判断用 VFS=1/MIB=7）：现行值把 SCHED(4) 与 VM(8) 的 grant 误放行、真正的 VFS(1)/MIB(7) grant 被拒。文件内无测试钉这两个值。

**影响**：magic grant 语义反转——VFS/MIB 的 magic grant 会被拒（MIB 的 remote relay 三 grant 通电后必然失败，阻塞 10-stage-mib/todo.md P1-4），SCHED/VM 的普通 grant 反而获得豁免待遇（安全面松动）。

**为何 edge**：kernel 生产代码（edge 判定②），修复落点 os/kernel/src/grant.rs；MIB stage 不代修。

**建议**：两常量删除硬编码改用 `minix_types::Endpoint::VFS.get()`/`Endpoint::MIB.get()`（单一真值，endpoint.rs 已有）；补门测试（VFS/MIB endpoint 的 grant 通过、SCHED/VM 不通过）钉死。

**依赖**：无。可独立先行，小时级。
**解锁**：E-MIBPROD 的 grant 链正确性；10-stage-mib/todo.md P1-4（relay 执行半）通电前提。

> **复核（2026-09-15，锚点确认 + 解锁行更新）**：常量在 grant.rs:292-295、门在 :546（分支起 :542），grant.rs 全部 8 个测试无一覆盖 MAGIC 分支——补测试空间确认；minix-types 权威值 endpoint.rs:62 `VFS = Endpoint(1)` / :68 `MIB = Endpoint(7)`，且 `granter` 参数类型即 `Endpoint`（grant.rs:355），门可直接比较；本仓 C 学习笔记 02-stage-vm/22-vm-exit.md:275 同证 VFS_PROC_NR=1。解锁行更新：P1-4 已于 2026-09-15 闭环 seam 半，本条仍是 relay 通电前提。

> **进度（2026-09-15，✅ 闭单）**：错误常量删除，门改走新谓词 `may_create_magic_grant(granter: Endpoint) -> bool`——直接消费 `Endpoint::VFS`/`Endpoint::MIB`（minix-types 单一真值，kernel errno.rs:19 re-export 同型先例）。方案对比：就地改值 1/7（否——保留第二真相源，恰是本次手抄漂移的事故根因模式）/ kernel proc.rs ProcNr 常量（否——ProcNr 与 Endpoint 语义不同，端点含代数位，类型混用是应避免的混淆）；Redox/Linux 对照：uapi 常量一处定义全树消费，minix-types 即本项目 uapi 等价物。谓词抽取理由：门在 `data_copy_vmcheck` 跨空间读取之后，全路径无宿主测试通路（proc_cr3 返回 None 即先行 EPERM）；C 的语义顺序（先读 grant 条目后判 MAGIC，do_safecopy.c:218-226）保持不变。测试 ×2：`test_magic_granter_endpoints_match_c_com_h`（C 绝对值 pin：VFS=1 com.h:60 / MIB=7 com.h:66）+ `test_may_create_magic_grant_vfs_mib_only`（VFS/MIB 过，SCHED(4)/VM(8)——旧错误值恰对应的两个端点——PM/RS/任意端点全拒）。验证：`cargo test -p minix-kernel --lib` 744 passed 0 failed；grant.rs clippy 零告警。回写：10-stage-mib/todo.md §5 指针表。

---

## E-DMWIRE devman 生产接线四缺：server transport、请求分类器、装配半、client/RS 侧生产传输（11-stage-devman 首轮架构审查登记，2026-09-15）

**问题**：devman 语义面 84 个测试全部跑在注入 seam 上（78 server + 6 client），生产执行半整层缺席，四个落点分属不同域：

1. **server 侧生产 transport 不存在**（edge 判定①+③复合）。`main.rs:35-37` 是自旋停车（注释自认 "still `todo!()` — calling it would panic, so park"）；`VTreeFs::run`（os/servers/devman/src/vtreefs/mod.rs:322）与 `Server::handle_other`（server.rs:91）两条分派面的生产传输半都空——`Transport` 只有 VecTransport 测试实现（STATE.md P1-1T），生产 impl 需要 minix-sys 的 receive/send 面（E1 前置）。
2. **请求分类器（raw IPC → `Request`/`DevmanMsg`）不存在**（edge 判定①）。VFS 面的 `Request` 分类需要 FS 驱动协议 wire（REQ_* 字段宏、fsdriver dirent/stat 编码）——该 wire 归 E-REQWIRE 契约域，devman 是消费方；DEVMAN 面的 ADD 还需要 `sys_safecopyfrom` grant 拷贝（C device.c:239-240，add_device.rs:7-9 注释自认"transport business"），内核对端与 wrapper 归 E2 家族。
3. **装配半不存在**（stage 内与 edge 的接缝）。`Server::new` 之后到事件循环之间的装配线（SEF 启动 → mount → 建树 → 循环 → OutAction 执行）没有任何代码承载；其结构定案在 11-stage-devman/todo.md DM-P1-2（双分派面统一）/DM-P1-3（启动序列裁决），stage 内实施不属本条，本条只管"真实通电"。
4. **client 与 RS 侧生产传输**（edge 判定①+②）。`ClientTransport` 的 grant/sendrec 生产 impl（C：cpf_grant_direct + ipc_sendrec，generic.c:112-122）需要 minix-sys 的 grant wrapper 与 E1；`RsTransport` 的生产 impl（C：ds_retrieve_label_endpt + ipc_sendrec，manager.c:841-849）需要 minix-sys ds.rs（E-DSWIRE）+ E1，落点在 RS stage（os/servers/rs/src/publish.rs:39-70 的 devman 臂目前只有决策半）。

**影响**：devman 不能出生（停车自白）；驱动侧 `add_device`/`del_device`/`handle_msg`（minix-sys/src/devman_client.rs:144/:189/:224）全部只能跑 FakeTransport；RS 的 publish/unpublish devman 臂无法端到端验证。

**为何 edge**：① minix-sys 传输/grant/DS wrapper 是共享基建（E1/E-DSWIRE 域）；② RS 侧生产代码归 03-stage-rs；③ 与 VFS 的 wire 契约归 E-REQWIRE。devman stage 内的装配与 seam 实施（DM-P1-2/P1-3）不属本条。

**建议**（执行序，沿 DS 先例 E-DSWIRE 的两段式）：
- 先 stage 内：11-stage-devman/todo.md DM-P1-2 定死 `Server::run` 单分派面 + Reply 错误通道形状，DM-P1-3 定死懒启动序列——传输实现者据此写生产 `Transport`（否则两套分派面的歧义会被带进传输层）。
- 后 edge：minix-sys 侧补 IPC receive/send 与 grant 拷贝 wrapper（E1 通电即活）；devman 侧写生产 `Transport`（classify raw message → Request / DevmanMsg）+ `main.rs` 装配替换停车循环 + `ClientTransport`/`RsTransport` 生产 impl；RS 侧 publish.rs 的 devman 臂接 `RsTransport` 生产端。
- minix-devman-client crate 的处置（删或收编）归 E-DMCLIENT，不阻塞本条。

**依赖**：E1（trap 层）；VFS wire 面 E-REQWIRE；DS 查询 E-DSWIRE；grant 拷贝 wrapper 归 E2 家族（SYS_SAFECOPY* 同款先例）。
**解锁**：E5(h) devman 生命周期联调；STATE.md backlog P1-6/P1-1T/P1-10/P1-12 全部关单；12-gpio-devman 的驱动注册链真实化。

---

## E-DMCLIENT minix-devman-client 孤儿 crate 处置：与 minix-sys 客户端职责重叠（11-stage-devman 首轮架构审查登记，2026-09-15）

**问题**：`os/libs/minix-devman-client/`（3 文件 395 行，6 测试）在仓库里没有消费者：workspace 成员注册（os/Cargo.toml:194）之外，无任何 Cargo.toml 依赖它，无任何 .rs 引用其符号（grep 全仓实测）。它声称的职责是"驱动侧设备记账 + USB 跟踪 + 序列化尺寸"（device.rs:1-8、usb.rs:1-8），但 doc 10/11 钦定的正式实现在 `minix-sys/src/devman_client.rs`（encode_device 字节兼容 serialize_dev + ClientTransport + handle_msg）与 `usb_model.rs`（UsbDevice/属性生成/add_usb/remove_usb）——两侧存在概念级重叠（device.rs `Registry`/`DeviceRecord` vs devman_client `ClientDevice`；usb.rs `UsbDevice`/`UsbTracker` vs usb_model `UsbDevice`；device.rs `serialized_size` vs encode_device 整体编码），且孤儿侧功能是子集（无 wire 编码、无 IPC）。唯一的外部引用是 16-stage-drivers 的评审文档 `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/12-gpio-devman.md:200` 把它列为测试目标（`cargo test -p minix-driver-gpio -p minix-devman-client --lib`），但 gpio 驱动 crate（os/drivers/system/gpio）的 Cargo.toml 并不依赖它——连带 gpio 的 lib.rs:5-6 文档注释字面写的"see the `minix-devman` crate"指向的是 **server crate 的包名**（驱动依赖服务器 crate 是分层违例），实际意图应是 minix-sys 的 devman_client 模块；引用关系三方（孤儿 crate、server 包名、正式客户端模块）纠缠不清。

**影响**：第二真相源存活——未来驱动作者搜"devman client"会先撞到孤儿 crate，按它记账（handle 复用语义与 devman 服务端的 dev_id 单调不复用**直接矛盾**：device.rs:101-111 的 `add` 复用已删 handle，而服务端 device_tree.rs:155 的 id 永不复用），写出的驱动注册逻辑与真实协议不符。

**为何 edge**：处置决策涉 16-stage-drivers（12-gpio-devman 的测试引用与 gpio 注释清理）；crate 本体在共享 libs/ 目录（edge 判定①②交界）。

**建议**：
- **方案 A（推荐）**：删除孤儿 crate（`os/libs/minix-devman-client/` 目录 + os/Cargo.toml:194 成员行），16-stage-drivers 的 12-gpio-devman.md:200 命令去掉 `-p minix-devman-client`，gpio lib.rs:6 注释改指 `minix-sys::devman_client`。理由：doc 10/11 已钦定 minix-sys 为客户端唯一实现，孤儿侧无消费者、语义有偏差（handle 复用 vs dev_id 单调）、其"未来给 gpio 用"的假想需求应由正式客户端承接。
- **方案 B**：保留并转正为 gpio 的记账层（gpio 声明依赖、文档更新）。仅当 16-stage-drivers 明确要"纯记账、不带 wire"的分层时成立；届时必须先修 handle 复用语义（改为 dev_id 单调对齐服务端）。
- 两侧共同动作：gpio lib.rs:6 的指代注释必须澄清（无论 A/B）。

**依赖**：无。
**解锁**：E-DMWIRE 的 client 侧实现者不再有两套 API 可选；16-stage-drivers 的驱动注册链设计定案。

> **复核（2026-09-15）**：现状与条目基本一致，两点修正——(1) 该 crate 已在 workspace 成员表（os/Cargo.toml:194，与条目记载一致），但仍零依赖者；(2) gpio lib.rs:4-6 注释引用的是 server crate 包名 `minix-devman`（引用对象本身就不该是任何 devman crate，应是 minix-sys 的 devman_client 模块）。方案 A 删除时四处一并清：crate 目录 + 成员行 + 12-gpio-devman.md:200 测试命令 + gpio 注释。

> **进度（2026-09-15，✅ 闭单，方案 A 执行）**：(1) `os/libs/minix-devman-client/`（4 文件）git rm + workspace 成员行删除；(2) gpio lib.rs 文档注释改为指认 `minix-sys` 的 `devman_client`/`usb_model` 模块（消除指向 server crate 包名的分层违例失真）；(3) 12-gpio-devman.md 四处引用改指 minix-sys 两模块（模块行/测试命令/rg 清单/参见）+ 顶部改版注记（§3.4–3.6 等客户端叙事的 Rust 侧映射重排登记为 16-stage-drivers stage 内工作，C 分析仍有效）；(4) 11-stage-devman/todo.md 范围行/基线命令（历史记录保留并标注删除）/OQ 注/§6 指针行四处同步。**验证**：`cargo test -p minix-driver-gpio --lib` 8 passed、`cargo test -p minix-sys --lib` 125 passed（devman_client 7 + usb_model 5 承接原孤儿测试的语义面：注册/注销/序列化尺寸/接口绑定——第二真相源消除后由权威实现唯一承载）；全仓 grep `minix-devman-client` 仅剩历史记录与本条目闭单注记。**观察登记（不顺手修）**：doc 12 的 §1.4–1.6/§3.4–3.6 客户端教学叙事仍以孤儿 crate 的概念命名（Registry/DeviceRecord）展开，重排归 16-stage-drivers 工作流（doc 顶部改版注记已声明）。

---

## E-INWIRE input 服务器生产传输接线四缺：事件循环、announce、编解码器消费、端到端联调（12-stage-input 首轮架构审查登记，2026-09-15）

**问题**：`os/servers/input` 的决策核心完整（66 个纯函数测试全过，对账见 12-stage-input/todo.md §1.0），但"服务器"实体未组装：`main.rs:31-32` 是带注释的空 `loop {}`；生产代码零状态（`InputTable::fresh()`（structs.rs:315）的调用者全部是测试）；`minix-types` 的五个消息编解码器（`ipc/input.rs` 的 conf/setleds/input_event/tty_event/tty_up 构造与解码）在 server 内零消费——server 今天无法构造发给驱动的 `INPUT_CONF`，也无法解出驱动发来的 `INPUT_EVENT`。四个落点：

1. **事件循环落地方式**。`minix-sef` 目前是 5 行占位 stub（os/libs/minix-sef/src/lib.rs，"SEF 服务框架……待实装"），E-ISWIRE 已跟踪其缺口。input 需要裁决：等 minix-sef 实装后走 `sef_receive_status` 等价物（C 侧 `chardriver_task` 的收信循环，chardriver.c:549-573），还是先用 `minix-sys::receive`（lib.rs:86，已存在）直写最小循环。倾向后者先行、minix-sef 实装后切换——input 的判决面已全在纯函数里，循环壳随时可换。
2. **chardriver_announce 等价物**。DS 发布 `drv.chr.<label>` + 释放上一代阻塞调用者 + 清开门集合（C chardriver.c:99-127）。状态半已在 minix-chardriver `CharServer::announce`（driver.rs:346），DS 发布半可用 minix-sys ds.rs 的 `publish_label`（ds.rs:128）——缺的是把两者与 `sys_statectl CLEAR_IPC_REFS` 等价面串起来的生产代码。
3. **五消息编解码器接入 + m_source 回填**。构造器统一置 `Endpoint::NONE`（ipc/input.rs:247,280,311,347,377），发送方在传输时回填真实来源——该回填动作目前无执行者。DS 客户端全套餐已备（ds.rs:187 `retrieve_u32`/:197 `retrieve_label_endpt`/:264 `subscribe`/:271 `check`，恰为 `input_check` 全部所需），asynsend 表已备（ipc.rs `AMF_NOREPLY` 面）——缺口是组装而非等待。
4. **端到端联调**。input↔pckbd↔TTY 事件链与 input↔VFS 读链的真实通电验证，挂靠 E5。

**影响**：input 不能出生（与 E-DMWIRE 的 devman 同款停车自白）；E5 的输入链联调无从谈起；`/dev/kbd*`、`/dev/mouse*` 无服务端。

**为何 edge**：① minix-sys 传输/DS/asynsend 是共享基建域（E1/E-DSWIRE 已立）；② minix-sef 落地方式是跨 stage 决策（E-ISWIRE 同一 shared crate）；③ 联调归 E5。stage 内的六件纯决策组装（IN-P1-1）不属本条，见 12-stage-input/todo.md。

**建议**：IN-P1-1 的决策面先落地（尤其 `input_other` 分派与离去检测——它们定义了传输层必须提供的回调形状），再按 1→3→2 顺序接线；循环壳先 `minix-sys::receive` 直写、标注 minix-sef 切换点。

**依赖**：E-ISWIRE（minix-sef 决策，若选择等待）；E-CDRCONV（若收敛裁决改变 announce 归属）；E5（联调）。
**解锁**：E5 输入链；`input` 服务进程真实化；13-stage-ipc 的消息面有一个真实生产消费者。

---

## E-CDRCONV chardriver 框架双实现收敛（A-1 收口）+ minix-chardriver CDEV_REPLY_BASE 错值（12-stage-input 首轮架构审查登记，2026-09-15）

**问题一（架构收敛，plan.md A-1 的收口时机已到）**：C 只有一个 `libchardriver`（chardriver.c 600 行，input、tty、十余个字符驱动共用），Rust 出现两套同源实现——`os/servers/input/src/framework.rs`（605 行：`classify_request`:190 / `gate_character_request`:228 / `decide_reply`:335 三判决 + 常量 + 应答结构）与 `os/libs/minix-chardriver`（1014 行：`CharDriver` trait driver.rs:180 + `classify` driver.rs:273 + `CharServer` driver.rs:326）。后者是**零依赖方孤儿**：grep 全 workspace，无任何 Cargo.toml 依赖它（仅 os/Cargo.toml:186 的成员声明），无任何 .rs 引用其符号。重复物清单：`CharacterRequest`（framework.rs:49）vs `CdevRequest`（protocol.rs:59）；`OpenDeviceSet` ×2（framework.rs:246 / protocol.rs:165，256 槽线性数组两份）；`ReplyDecision`（framework.rs:307）vs `reply_decision`（driver.rs:85 区域）；`NotifySource` ×2（framework.rs:175 / driver.rs:32）；CDEV 常量两份（framework.rs:29-36 / protocol.rs:16-48）。plan.md §7.3 的 A-1 建议是"新建共享框架 crate；input 内部最小等价实现可先行"——前半句已建成（16-stage 01-chardriver-framework.md 与库同时交付）、后半句已执行（12-stage 02-chardriver-framework.md 以 framework.rs 为实现锚点），但两半从未合拢，且 12-stage/02 与 16-stage/01 两篇文档各自成立、互不引用。

**方案对比**：
- **方案 A**：input 迁入 minix-chardriver——input 删 framework.rs 的框架半、依赖共享库、保留业务判决。收益：单一权威立即成立。代价：input 已验证的"三判决纯函数"风格与 trait 回调风格需磨合（`decide_reply` 的判决表 vs `reply_decision` 的判决函数语义相近但形状不同）；input 的 66 个测试需随迁改写。
- **方案 B（推荐）**：minix-chardriver 重构为消费纯函数核——把 framework.rs 式 decide 函数提升为库 API（判决皆纯函数、皆可测），`CharDriver` trait + `CharServer` 作可选表皮（供未来直接式驱动用）。收益：与 input 已验证模式一致、与 16-stage 已写的 trait 面兼容、tty 等后续驱动两种风格都可落。代价：库内 API 重排，16-stage/01 文档 §3-4 章同步。
- **方案 C**：双轨并存 + 文档豁免——在 12-stage/02 与 16-stage/01 双文档声明各自边界。代价：违背单一权威先例（E-MINTYPES-SYS/E-REQWIRE 的收敛方向），两套 `OpenDeviceSet` 重启门语义靠人工保持一致，是最贵的一项。
- 无论何者：`[ARCH: ...]` 三处一致（12-stage/02、16-stage/01、两侧代码），属架构演进需用户批准。

**问题二（wire 错值，正确性问题——发现于本轮，交 full-review/todo-fix 修，本条只登记）**：`os/libs/minix-chardriver/src/protocol.rs:22` `CDEV_REPLY_BASE: i32 = 0x500`，注释自引"C: `CDEV_RS_BASE`（com.h:934）"，但 C 的真值是 `CDEV_RS_BASE 0x480`（com.h:920；0x500 是 `BDEV_RQ_BASE`，com.h:963）。servers/input 的 `CHARACTER_RESPONSE_BASE = 0x480` 是对的（framework.rs:31，测试 framework.rs:448-450 锁 0x480/0x481/0x482 三值）。该常量当前无消费者（grep 仅定义处），属潜伏错误——恰因孤儿化而未爆。收敛时随方案 A/B 一并消除（这也是"孤儿库让 bug 隐形"的实证）。

**为何 edge**：① minix-chardriver 是共享基建（16-stage 所有、未来十余个字符驱动共用）；② 涉 12-stage 与 16-stage 两篇框架文档的三处一致改写；③ wire 错值修复属正确性动作（本条不执行）。

**依赖**：无硬依赖（可在 input 通电前独立收口，且越早越便宜——input 通电后迁移成本上升）。
**解锁**：E-INWIRE 的 announce 归属定案；tty 驱动（16-stage 06）接入框架时不再有二选一困惑；12-stage/02 与 16-stage/01 文档合一。

---

## E-TTYEVENT TTY_INPUT_UP / TTY_INPUT_EVENT 消费侧零实现（12-stage-input 首轮架构审查登记，2026-09-15）

**问题**：C 的 input↔TTY 契约是三件事——input 启动时向 TTY 发 `TTY_INPUT_UP` 握手（input.c:671-677）；设备与其 mux 都未被打开时，事件转发 `TTY_INPUT_EVENT` 给 TTY（input.c:408-421）；TTY 侧 `do_input` 消费（`INPUT_PAGE_KEY` 过滤 + `NR_SCAN_CODES` 边界 + 释放位，keyboard.c:148-176），并经 `INPUT_SETLEDS` 回设灯（keyboard.c:369-384；input 侧只接受 TTY 来源，input.c:630-635）。Rust 现状：**生产侧已备**——servers/input 的 `forward_to_terminal`（produce.rs:131）、init 的 `NotifyTerminal` 步骤（init.rs）、minix-types 两消息号与 `tty_event_msg`/`tty_up_msg` 编解码（ipc/input.rs:37/:41/:337-383）齐备；**消费侧零实现**——`os/drivers/tty/tty` 全 crate grep 无 `TTY_INPUT_UP`/`TtyEvent` 消费（session.rs:221 的 "tty_events" 只是 select 唤醒的计数变量名），minix-types/ipc/tty.rs 只有 FKEY 观察者协议（tty.rs:1-40），与 input 无关。16-stage 06-tty-driver 文档面也无对应条目 [待验证]（其 STATE 记该篇九门收口，但代码 grep 零命中——收口范围可能未含此契约）。

**影响**：input↔TTY 握手链单向断裂：即使 E-INWIRE 给 input 通电，键盘事件也无法到达行律（`/dev/console` 无键盘输入）；LED 回设链（TTY → INPUT_SETLEDS → input → 驱动）上游无生产者，input 的 SETLEDS-from-TTY 分支将永远走不到。

**为何 edge**：消费侧归 16-stage-drivers（06-tty-driver 的实现域）；wire 字段本身已定稿（ipc/input.rs），无需再议契约；两侧属不同 stage 的生产代码，需联动验证。

**建议**：16-stage 06-tty-driver 补三件事——UP 握手（保存 input endpoint，供 EVENT/SETLEDS 双向寻址）、EVENT 消费（过滤 + 边界 + 释放位入 inbuf）、set_leds 回发（asynsend INPUT_SETLEDS）。均可在 minix-types 既有编解码器上以纯函数决策先行，传输接线随该 stage 自己的通电件。

**依赖**：E-INWIRE（对端通电后才能联调，实现本身不依赖）。
**解锁**：E5 输入链的控制台半；`/dev/console` 键盘可用；LED 回设链闭环。

---

## E-PCKBDREG pckbd 邻接面移交登记：一处行为分歧 + 三处缺口 + 双轨重编码（12-stage-input 首轮架构审查登记，2026-09-15）

**所有权声明**：`os/drivers/hid/pckbd`（892 行）属 16-stage-drivers（其 13-pckbd-driver.md 已九门收口，17 测试全过）。以下为 12-stage 视角（文档 14-pckbd-driver.md 的契约面对账）扫描发现的移交项，登记于此供 16-stage 排期；第 1 项是正确性问题，修复走 full-review/todo-fix，本条不执行。

1. **行为分歧（正确性）**：C `kbd_process` 的状态 3 遇非 NumLock 索引时 FALLTHROUGH 穿透到 default，仍查 `scanmap_normal` 发出普通键事件（pckbd.c:343-361）；Rust `ScancodeState::feed` 状态 3 非 NumLock 一律吞掉（scancode.rs:119-131）。序列 E1 1D 1C 在 C 发出 ENTER 按下、Rust 丢弃。现有测试只覆盖状态 2 断裂（scancode.rs:282 `test_broken_pause_prelude_resets`），状态 3 断裂未锁。E1 1D 后接 0xE0/0xE1 的前缀重启路径同样未测且行为存疑。
2. **FLAG_RELATIVE 死常量**：C 鼠标位移事件带 `INPUT_FLAG_REL`（pckbd.c:409）；Rust `MouseEvent::Motion` 无 flags 字段（mouse.rs:48），`FLAG_RELATIVE`（mouse.rs:40）零代码路径引用——事件下游（input 服务器 `stored_event`，produce.rs:175）按 C 语义是要透传 flags 的，此处缺失会使相对位移事件在线上丢标志。
3. **扫描码全表缺席**：C 两张 0x80 项表 `scanmap_normal`/`scanmap_escaped`（table.c:11-169）；Rust `ReferenceMap` 仅 4 项 + PAUSE（scancode.rs:176-208），全表在 workspace 任何 crate 中不存在（grep `scanmap` 仅命中文档注释）。16-stage 文档称"全表是数据（随服务数据走）"（16-stage-drivers/13-pckbd-driver.md:125），但数据无处可走；`EmptyMap`（scancode.rs:210）因数据缺席成为 mock-only 抽象（模式 80）。另与 12-stage todo IN-P2-2 联动：表的内容若落地，键码词汇应消费 minix-types 权威（迁移后），不是本地重述。
4. **ACK 条件缺位**：C 的 ACK 有效需状态口无超时位 `!(sb & 0x40)`（pckbd.c:129）；Rust `LedOutbox::note_ack` 无状态参数（led.rs:123-130），无法表达该条件——硬件半落地时需补参数或改签名。
5. **双轨重编码**：pckbd `InputBridge`（bridge.rs:54-130，bound + 两 id）与 minix-sys `DriverRegistration` + `decide_report`（inputdriver.rs:37-44/:161-174，Option 三元组）编码同一份 C do_conf/send-event 门控逻辑；bridge 缺 `is_disabled` 等价物（configure 两 id 均无效时返回值语义与 minix-sys 版不一致，bridge.rs:82-90 vs inputdriver.rs:81-83）。pckbd 的 Cargo.toml 声明依赖 minix-types/minix-sys 但 6 个源文件零 use——依赖边只存在于清单。收敛方向：bridge 改为 minix-sys 决策函数的消费方，或删除 bridge（其消费路径本 crate 内为零）。

**为何 edge**：全部落点在 16-stage-drivers 所属 crate；第 5 项涉共享 crate minix-sys 的消费关系；12-stage 自身无处置权。

**依赖**：第 3 项与 12-stage-input/todo.md IN-P2-2（键码词汇归属迁移）联动；其余无。
**解锁**：E5 输入链的驱动半；pckbd 硬件落地时不再踩状态 3 与 ACK 条件两颗雷。

---

## E-IPCWIRE ipc-server 生产面接线八缺（13-stage-ipc 首轮架构审查登记，2026-09-16）

**问题**：13-stage-ipc 的 Rust 判定层已完整（`os/servers/ipc-server/` 17 文件 4945 行，83 测试全过，见 13-stage-ipc/todo.md §0），但生产面八件全部缺席，ipc-server 至今无法对外提供任何一个 SysV IPC 调用（main.rs:27 直接 panic）。逐项锚点：

1. **trap 桥（阻塞于 E1）**：`DirectTrapTransport` 的 `IpcTransport` 实现全方法返回 `Err(TrapStatus(EIO))`（`os/libs/minix-sys/src/ipc.rs:527-559`），注释自认"真实 trap 指令序列待 64 位 trap 接线落地（stage plan item A-6）"（ipc.rs:519-525）。
2. **SEF 层整体缺失**：`sef_startup`/`sef_receive_status`/`sef_local_startup` 全仓库无实现（grep 零命中）；ipc-server 的 `IpcServer::init()` 只设一个 bool（`os/servers/ipc-server/src/server.rs:200-202`），C 的三条回调注册（main.c:124-134）无处落地。C 侧参照 `minix3/minix/servers/ipc/main.c:80-134`。
3. **sys_datacopy 命名 wrapper**：C 语义 = SELF + sys_vircopy；`SELF` 常量已备（minix-sys/src/syscall.rs:254）、`sys_vircopy` 已备（syscall.rs:481-500），缺命名版本——semop 操作数组拷入、semctl/shmctl 缓冲拷出的边界动词（C sem.c:680、shm.c:308 等）无承载。
4. **proceventmask 客户端 wrapper**：minix-sys 全模块 grep 零命中；消息构造器已备（`os/libs/minix-types/src/ipc/event.rs:168` 的 `proceventmask_msg`），taskcall 组装无人做——ipc-server `events.rs` 的 `SyncAction` 执行半缺位（C main.c:163-166 的订阅/退订调用）。
5. **VM_CALL_SHARED_UNMAP wrapper**：常量已定义（minix-sys/src/vm.rs:62）无 wrapper；同族 `mmap_via`/`munmap_via`/`physical_address_via`/`reference_count_via` 先例（vm.rs:171/:207/:335/:356）——shm 侧 `SweepPlan::unmaps`（ipc-server refcount.rs:36-42）的执行动词。
6. **时钟客户端**：ipc-server 全部 `now: u64` 为注入参数（sem/table.rs:219、shm/segment.rs:102 等），无 clock_time 来源（C utility 面 `clock_time(NULL)`，sem.c:138 等 5 处）。
7. **getnuid/getngid/getnpid 窄 helper**：minix-sys 只提供全元组版 `endpoint_identities_via`（rs.rs:170），注释明说窄版未提供（rs.rs:167-169）；ipc-server `perms.rs:49-53` 声明的 Identity 查询半缺位（C utility.c:10-11、sem.c:720 的 getnpid）。
8. **minix-types 补 `struct semid_ds`/`struct shmid_ds` 二进制布局**：IPC_STAT 拷出与 IPC_SET 拷入的 wire 契约（C `sys/sem.h`、`sys/shm.h` 布局，用户态 ipcs(1) 依赖），minix-types grep 零命中——E-REQWIRE/E-INWIRE 同款"wire 类型缺口"模式。无它则服务层无法搬运 stat/set 的结构体字节（13-stage-ipc/todo.md IPC-P1-1 边界契约）。

**影响**：ipc-server 判定层 4945 行无法对外服务；13-stage-ipc/todo.md IPC-P1-1（服务层组装）的全部边界契约悬空；E5 的 IPC 联调验收面无前置。

**为何 edge**：edge 判定①——minix-sys 的 trap 层与 SYS_*/VM_* wrapper 是全体用户态服务的共享契约层（E1/E2/E-DSWIRE/E-DMWIRE 同域），第 1-7 项归此；第 8 项落 minix-types 布局面（判定规则①原文点名"minix-types 布局"）；ipc-server 自身只是首个深度消费者。MIB 客户端不在本条——E-RMIBWIRE 的代码半已闭环（c7d2ea150）。

**建议**：
- 方案 A（推荐）：按 campaign 波次拆批——第 8 项（布局）无依赖，13-stage 服务层开工前即可落地（`offset_of` 钉死布局，10-stage-mib P1-3 先例：SysctlNode 96B 的做法）；第 3-7 项为纯 wrapper/组装，C 波（minix-sys 客户端库轮）与 E-DSWIRE/E-DMWIRE 同型批量处理，全部可用 CannedTransport 形状做宿主单测（E2/E6 先例）；第 1/2 项随 F 波 E1 trap 桥与通电族落地。
- 方案 B（否决）：ipc-server 内自建这八件。wrapper 层重复 ×N 服务（与 E-RMIBWIRE 方案 B 同款否决理由：C 把这些做成 libsys 公共库正是因为多服务共用）；semid_ds/shmid_ds 布局放服务 crate 则 ipcs 兼容性契约散落，且未来 lwip/uds 等消费者各抄一份必然漂移。

**依赖**：第 8 项无依赖；第 3-7 项无硬依赖（可宿主测试）；第 1/2 项依赖 E1（trap 桥）。
**解锁**：13-stage-ipc/todo.md IPC-P1-1（服务层接线）全链；ipc-server main.rs 去 panic；E5 IPC 联调验收面前置。

> **进度（2026-09-16，🔄 第 8 项布局闭环，minix-types sysvipc 模块）**：`IpcPerm` 24B（五 u32 域 + seq + 垫，ipc.h:54-66）+ `SemidDs` 56B（perm@0/nsems@24/otime@32/ctime@40/私有指针槽@48，sem.h:55-66，LP64）+ `ShmidDs` 80B（perm@0/segsz@24/lpid@32/cpid@36/nattch@40/atime@48/dtime@56/ctime@64/私有指针槽@72，shm.h:99-114）落 `minix-types/src/ipc/sysvipc.rs`，布局见证 ×3（offset_of 全钉）。C 头按 i386 书写，本重写沿用 64-bit-overlay 判例（time_t 8 字节、指针 8 字节，与 MessVmmcpReply 同款）。**余项**：第 3-7 项（wrapper/组装，CannedTransport 可测）随 E9 三分域同型批量；第 1/2 项（trap/SEF）随 E1 real-trap 门控的通电族落地。**验证**：minix-types 251 全绿。

---

## E-MINTYPES-RUNTIME minix-types 布局单点权威收敛（14-stage-runtime V1 轮登记，2026-09-16）

**问题**：同一调用的 wire 契约存在两套表达。minix-types 走类型化路线（MessageUnion 具名 arm + 语义 In/Out 结构 + `DecodeFromM1`/`EncodeToM1` 编解码 trait，minix-types/src/ipc/vm.rs:820-833），但消费面窄：trait 方法调用全仓仅 4 处（vm 服务器 encode.rs:141-144 与 dispatcher.rs:720-734），VM 分发主路径实际走固有方法 `decode_message`。minix-sys 走本地路线：每个调用族在文件内定义 `#[repr(C)]` payload 后经裸字节打包写入（`pm.rs:91-108` 与 `vm.rs:96-110` 各自复制一份 `cleared_message`/`write_payload`，rs.rs:82 又是第三种 raw 直写），本地 payload 的布局断言薄弱（全仓约 321 条 size/offset 断言中 minix-sys 仅占 8 条）。已出现状态认知分叉实例：VM_REMAP 在 `minix-types/src/ipc/vm.rs:526-527` 区段标注 DEFERRED，而 `minix-sys/src/vm.rs:305-327` 已有该调用的客户端打包实现，且 `remap_via` 泛化 call 号可表达 VM_REMAP_RO。

**影响**：同一布局两层各改各的漂移风险（VM_REMAP 分叉已是第一例）；99 篇（布局权威文档，现 pending）定稿时无单一权威可依；codec trait 若维持窄消费面将成为半弃抽象（模式 80 邻近形态）。

**为何 edge**：判定①——minix-types 布局被约 125 个 crate 依赖，收敛方向影响全部服务；与 E-MINTYPES-SYS（kernel-call 常量单一权威，已闭单）同方向，与 E-IPCWIRE 第 8 项（semid_ds/shmid_ds 布局）、E-RSWIRE 五消息臂（403e47022）同域。

**建议**：
- 方案 A（推荐）：99 篇定稿时确立"布局单点归 minix-types、minix-sys 只做打包与传输"边界——minix-sys 本地 payload 逐族改为消费 minix-types 结构，或就地补齐 56 字节/字段偏移 pin 断言（vm.rs `test_map_payload_matches_c_field_order` 是现成样板）；`cleared_message`/`write_payload` 收敛为 minix-sys 单一内部 helper；codec trait 的去留（统一编解码入口 vs 删除保留固有方法）在同一裁决中定。Redox 先例：syscall 契约单源（design 仓 syscalls.toml）+ 单 crate 统一 data/number/error/flag 模块（github.com/redox-os/syscall）。
- 方案 B（否决）：承认两层布局合法并存、仅文档标注。否决理由：VM_REMAP 的标注分叉证明标注会过期，双真相源必然再漂移。

**依赖**：14-stage-runtime/todo.md V1-P1-5（99 篇改写）为前置；执行时与 E-IPCWIRE 第 8 项同轮（同为 minix-types 布局面）。
**解锁**：09 篇 open 路径布局落地（40 字节内联的 64 位裁决）；14-stage-runtime/todo.md V1-P2-2 收敛；VM_REMAP_RO 客户端 wrapper 定稿。

---

## E-MINSYS-SCOPE minix-sys 内域外 stage 客户端模块的内聚性处置（14-stage-runtime V1 轮登记，2026-09-16）

**问题**：minix-sys（14-stage-runtime 的实现 crate，域内对应文档 04~12）混装六个其它 stage 的客户端模块：ds.rs（303 行，07-stage-ds）、rmib.rs（1287 行，10-stage-mib）、devman_client.rs（423 行，11-stage-devman）、inputdriver.rs（512 行，12-stage-input）、usb_model.rs（437 行，16-stage-drivers）、socket.rs（146 行，17-stage-net），合计约 3108 行，占 crate 全量（9319 行）的三分之一。各模块无 feature 门控，任何消费者（60+ 个 Cargo.toml 依赖方）都编译全量。

**影响**：crate 增长无界且所有权模糊——其它 stage 演进客户端协议时必须修改 14-stage 的 crate；域内 review 与域外 review 的边界只能靠临时约定维持（14-stage-runtime/todo.md V1 轮即按"域内深审/域外共享层扫描"切分）。

**为何 edge**：判定①——minix-sys 是全体用户态的共享契约层；处置方案影响 07/10/11/12/16/17 六个 stage 的排期与编译面，单一 stage 无权裁决。

**建议**：
- 方案 A（推荐）：维持单 crate，域外模块收进 feature 门控（按 stage 分组 feature，默认全开保持现有 Cargo.toml 兼容），lib.rs 增加模块归属表（模块 → 所属 stage 文档 → 对应 edge 条目）。
- 方案 B（否决，暂）：迁出为 per-stage 客户端 crate。否决理由：与 C libsys 单库先例相悖，拆分成本与 E-DSWIRE/E-DMWIRE/E-RMIBWIRE 等通电条目叠加；待方案 A 运行一个阶段后若单 crate 仍失控再复议。

**依赖**：E-MINSYS-HYGIENE（同 crate 卫生轮，同场处置）；E-RMIBWIRE/E-DMWIRE/E-DSWIRE 执行时顺带评估各自模块的 feature 归置。
**解锁**：14-stage 域内/域外 review 边界长期化；各 stage 客户端协议演进时的明确落点与编译面收敛。

> **进度（2026-09-16，✅ 方案 A 落地，feature 门控 + 模块归属表）**：执行前复核——六个域外模块（ds 303/rmib 1287/devman_client 423/inputdriver 512/usb_model 437/socket 146 行）**全仓零外部消费者**（grep 60+ 依赖方仅命中核心模块 ipc/syscall），唯一 crate 内依赖 usb_model→devman_client（super::devman_client 四处）。实施：Cargo.toml 六 feature（`ds`/`rmib`/`devman`/`input`/`usb=["devman"]`/`socket`，default 全开保持既有 Cargo.toml 零改动），lib.rs 逐模块 `#[cfg]` 门控 + 模块归属表（模块→feature→所属 stage 文档→关联 edge 条目）写入 crate 文档头。**验证矩阵**：default（192 测试全绿）/ `--no-default-features`（仅核心编译 ✓）/ `--features usb`（依赖传递拉入 devman ✓）/ `--features ds,rmib,devman,input,socket` 组合 ✓；依赖方抽查 minix-sched/minix-ds/minix-mib/minix-sef 零错误。方案 B（per-stage crate 拆分）维持否决状态，待方案 A 运行观察。

---

## E-FSRUNTIME 8 个 fs server bin 的 SEF/RS 启动握手与运行时接线（15-stage-fs V1 轮登记，2026-09-16）

**问题**：8 个 fs server 的 bin 全部未接事件循环与进程握手：mfs 是 `fn main() { loop {} }`（os/fs/mfs/src/main.rs:8-10，模块注释声明归服务运行时轨道）；ext2/isofs/vbfs/hgfs/procfs/ptyfs 的 main.rs 第 4-6 行是同义 TODO 注释；pfs 构建 server 后停放（os/fs/pfs/src/main.rs:8-10）。C 对应物是各 server main.c 的三段：`env_setargs`（mfs/main.c:19）、`sef_local_startup`（:31-42，含 `sef_setcb_init_restart` 状态化重启）、`fsdriver_task(&mfs_table)` 分发主循环（:23）。框架内事件循环与 `impl FsDriver` 装配归 15-stage-fs/todo.md V1-P0-1，本条只管进程侧：SEF 回调注册、RS 启动握手、参数解析、信号接线。

**影响**：mfs 纵有 26/31 个已实现的请求处理函数，也无从作为进程对外服务；boot 挂载链（VFS `do_init_root` → `mount_pfs()` → `mount_fs(DEV_IMGRD, "/", MFS)`，15-stage-fs/00-fs-overview.md:13）的 fs 侧永远等不到。

**为何 edge**：判定②③——SEF/RS 握手属服务运行时轨道的生产代码（与 E-ISBOOT、E-INWIRE 同型），且 boot 挂载联调需要 VFS + kernel + fs 多进程参战。

**建议**：
- 方案 A（推荐）：按 E-ISBOOT/E-INWIRE 先例，等服务运行时轨道提供 startup 面（sef 回调注册 + RS_INIT 握手 + 信号循环）后逐 server 接线；mfs 第一个接（V1-P0-1 的装配产物直接可用），pfs 第二个（boot 链需要）。
- 方案 B（否决）：各 fs crate 自带完整握手实现。否决理由：与 C 的 libfsdriver/sef 分层相悖，8 个 server 重复实现同一握手。

**依赖**：15-stage-fs/todo.md V1-P0-1（装配先行）；服务运行时轨道基建（E-ISBOOT 同类）。
**解锁**：boot 内存盘挂载链联调；8 server 全部可被 RS 启动。

---

## E-FSBDEV minix-fs 块层与真实块驱动的接缝（15-stage-fs V1 轮登记，2026-09-16）

**问题**：`os/libs/minix-fs/src/bio.rs` 的 `DeviceInfo` trait（bio.rs:40-46）生产实现只有 `RamDisk`；真块驱动相关的五件事全部悬空：分区尺寸查询（C `bdev_ioctl` DIOCGETP，bio.c:146-147）、驱动标签绑定（bdev_driver）、短末块部分读写（`lmfs_get_partial_block`）、聚散 I/O（`rw_scattered` + `bdev_gather`，cache.c:840、cache.c:742-757）、mount 的 `bdev_open`/`bdev_close`（mount.c:23,35,66；os/fs/mfs/src/mount.rs:8-9 自述缺席）。minix-bdev 与 minix-blockdriver 两 crate 归 16-stage-drivers（16-stage-drivers/02-blockdriver-framework.md、04-bdev-client.md）。

**影响**：FS 只能跑在内存盘上；磁盘 server（mfs/ext2/isofs）无法访问真实设备；脏块批量回写与短末块设备缺优化路径。

**为何 edge**：判定②——生产实现落在对方 stage 目录（os/drivers/storage 与 minix-bdev/minix-blockdriver，16-stage-drivers 轨道）；边界 trait（DeviceInfo）与本 stage 的 RamDisk 不动。

**建议**：
- 方案 A（推荐）：16-stage 落 minix-bdev 客户端通电后，在 fs 侧实现 `DeviceInfo` 的真实形态（包装 bdev 传输）并补短末块路径；本 stage 保持 trait 与 RamDisk 现状。
- 方案 B（否决）：15-stage 直接在 minix-fs 内实现驱动传输。否决理由：与 C 的 libminixfs/bdev 分层相悖，且 minix-bdev 归属已定。

**依赖**：16-stage-drivers 的 blockdriver 框架与 bdev 客户端两篇实施；E-FSRUNTIME（进程在才谈驱动连接）。
**解锁**：磁盘 server 的真机联调；15-stage-fs/todo.md V1-P2-8 的短末块与聚散 I/O 项。

---

## E-FSVMCACHE FS 二级缓存的零拷贝页移交与旗标机（15-stage-fs V1 轮登记，2026-09-16）

**问题**：C libminixfs 的 vmcache 是零拷贝页所有权移交（`vm_map_cacheblock`/`vm_set_cacheblock`，cache.c:443-451、cache.c:562-587）+ `VMMC_EVICTED`/`VMMC_BLOCK_LOCKED`/`VMSF_ONCE` 旗标机（cache.c:345-388）+ 按块 inode/offset 标签索引（libminixfs.h:29-30）+ 按 blocksize 对齐自动启停（cache.c:1236-1239）；Rust 的 `SecondLevelCache`（os/libs/minix-fs/src/cache.rs:97-110）只是拷贝型影子缓存钩子，unmount 路径的 `vm_clear_cache` 调用方也缺（C 在 call.c:83-84）。cache.rs:15-22 自述归属 VM 轨道。

**影响**：FS 与 VM 页缓存无法共享页；`REQ_PEEK` 的 HAS_PEEK 能力与写时零拷贝不可用；mfs 的 peek 只能走 read 仿真（15-stage-fs/todo.md V1-P1-3）。

**为何 edge**：判定①②——vm_* 传输面属 minix-types/minix-sys 共享契约 + VM 服务器生产代码（02-stage-vm 轨道）。

**建议**：
- 方案 A（推荐）：VM 轨道提供页移交/逐出回报传输面后，把 `SecondLevelCache` 升级为真实现（含标签索引与对齐门控），mfs 侧接 HAS_PEEK。
- 方案 B（否决）：在 minix-fs 内先做拷贝型二级缓存撑场面。否决理由：拷贝型无性能收益反而增加整块拷贝，属伪实现。

**依赖**：02-stage-vm 页缓存轨道；E-MINTYPES-RUNTIME（vm 消息布局同源权威）。
**解锁**：HAS_PEEK 能力、FS↔VM 共享页、V1-P1-3 方案 B。

---

## E-FSCMDS fsck/mkfs 命令占位的轨道认领（15-stage-fs V1 轮登记，2026-09-16）

**问题**：`os/commands/sbin/{fsck,mkfs}/src/main.rs` 各 14/15 行，纯参数解析占位；C 侧对应按文件系统分型的离线工具族（mkfs.mfs、fsck.mfs 等）。15-stage-fs 的 26 篇文档范围是 8 server + 4 框架库（plan.md §1.1），不含离线工具；命令轨道尚未建立，归属悬空。

**影响**：无法离线制作与校验镜像；测试与装机只能依赖预制镜像。

**为何 edge**：判定②——实现落点（os/commands/）不在 15-stage-fs 的 crate 集内；且 mkfs/fsck 需要跨 crate 复用各 fs 的盘上结构。

**建议**：
- 方案 A（推荐）：等 mfs/ext2 的盘上结构层稳定（15-stage-fs/todo.md V1-P2-10 排期）后由命令轨道认领，直接复用 minix-fs-mfs/minix-fs-ext2 的解析与写入函数（Rust 侧天然可共享；C 的 mkfs/fsck 是独立复制的实现，Rust 侧可做得更好）。
- 方案 B（否决）：15-stage 提前认领。否决理由：命令 crate 的归属与 sbin 分型构建形态未定，提前认领会重演域外混装（对账 E-MINSYS-SCOPE）。

**依赖**：V1-P2-10（盘上结构层稳定）；命令轨道建立。
**解锁**：镜像制作/校验自动化、装机流程。
