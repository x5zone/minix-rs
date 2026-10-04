# NK4-C 回溯 CodeReview 审计报告（自续-125 起 → HEAD）

> **性质**：独立审计 agent 产出的**回溯复核 + 待办登记**文档。生产码修复留给主线程（本次审计零生产码改动）。
> **权威源**：以 worktree 为权威、独立复核，不轻信 commit message 自述。所有代码/证据结论均可用本文命令重放。

## 0. 审计范围与快照

- **锚点（基线，勿改）**：续-125 `44c400229`。
- **审计截止点（本 agent 启动瞬间 HEAD）**：`5a2ad09e6`（续-132 WORKLOG 前沿滚更）。
- **区间 commit**（`git log --oneline 44c400229..5a2ad09e6`，共 10 条）：
  - 生产码 3 条：续-129 `15841fb17`、续-130 `9fc7c88f7`、续-132 `2a6c55912`。
  - 前沿滚更 2 条：续-130 `28c1d4644`、续-132 `5a2ad09e6`。
  - WORKLOG-only 取证 5 条：续-126 `a8e4ab1aa`、续-127 `1d2c8dd4a`、续-128 `cabcd9dec`/`46af18aae`、续-131 `9c51c478b`。
- **并发约束**：主工作树 `rewrite` 分支在本审计期间存在**未提交生产码 WIP**（`riscv64/fpu.rs`、`tty/*`、`trap_dispatch.rs`、`kernel-image/*`、`xtask/image.rs`），属主线程续-133 在飞工作，**不属于审计区间**，本 agent 未触碰、未 stash、未提交。独立复核一律在**隔离 detached worktree**（`.wt/audit @ 5a2ad09e6`）进行，读 commit 一律用 `git show <sha>:`（committed blob），避免脏树污染。

## 1. 验证链独立复跑（在 `.wt/audit @ 5a2ad09e6` clean 树）

| 门禁 | commit 自述 | 本 agent 独立结果 | 命令 |
|---|---|---|---|
| host 4-crate | 1400/0 | **1400 passed / 0 failed** | `cargo test -q -p minix-kernel -p minix-arch -p minix-boot -p minix-types` |
| host 6-crate（权威集） | 1771/0 | **1771 passed / 0 failed**（13 个 result 块，15 ignored） | `cargo test -q -p minix-kernel -p minix-arch -p minix-boot -p minix-types -p minix-sys -p minix-driver-tty` |
| clippy | Δ0（无新增错误） | **EXIT 0**（本作用域 `^warning:` 基线 221，均既有；无 error） | `cargo clippy -q <6-pkgs> --all-targets` |
| rustfmt（改动文件） | Δ0 | **EXIT 0**（6 包 `--check` 全净） | `cargo +nightly fmt -q <6-pkgs> -- --check` |
| check-layout 三架构 | PASS(all) | **CHECK-LAYOUT=PASS（all）**（x86_64/aarch64/riscv64 各 L0–L9 全通过；aarch64 段构建成功＝续-129 `-neon,-fp-armv8` 注入 + `compile_error!` 门在门禁构建路径下真实生效） | `bash os/kernel-image/check-layout.sh all` |
| aarch64 rc marker 真输出 + 3 轮 | marker=2/panic=0 三轮稳定 | **原始串口逐字节复核**：`tmp/nk4a/a64-pl011-6/7/8.serial`（≈4522 行，22:10/22:14/22:17 连续三轮）各 `marker=2 panic=0 MultiUser=1 WrongMessageType=0`；`a64-pl011-9.serial`（7491 行）`marker=2` | 见 §5 |
| x86 真机非回归 | marker=2/panic=0 | **原始串口复核**：`x86-pl011-1.serial`(7631 行)、`x86-pl011-2.serial`(10640 行) 各 `marker=2 panic=0 MultiUser=1` | 见 §5 |

**关于 host 数字的澄清（非差异）**：任务表述"1400 涨到 1771"指的是**不同包集**——4-crate 恒为 1400（续-129/130/132 均未向 kernel/arch/boot/types 增测；续-132 新测全在 tty/vm/minix-sys），全 workspace 6-crate 才 1771。两者本 agent 均精确复现，**claim 属实**。

**关于 `cargo test --workspace` 编译失败（区间外既有，非本区间回归）**：见 §7 AF-5。

## 2. 续-132 `2a6c55912` 审计（PL011 MMIO 臂 + map_physical_via 双 wire bug）

**判定：PASS。无回归、无幻影。**

### 2.1 map_physical_via 双 wire bug（`os/libs/minix-sys/src/vm.rs`）
- **根因坐实（机制层）**：旧实现手写 raw 覆盖请求（target@0 / physical u64@8 / length u64@16），而 VM 端 `VmMapPhysIn::decode_message` 按 `m_lsys_vm_map_phys` 臂解 `ep@0 / phaddr(u32)@4 / len(u32)@8`——偏移 4..8 被旧 raw 留空（cleared_message 的 0）⇒ VM 解出 **phaddr=0**（DIRECT zerobase 缺页臂拒绝）。回包旧读 `raw[0..8]`＝`m_source(4)|m_type(4)` 垃圾指针 ⇒ TTY 崩。
- **对偶端核对（本 agent 独立）**：
  - C Ground-Truth：`minix3/minix/include/minix/ipc.h:1504-1510` `mess_lsys_vm_map_phys { endpoint_t ep; phys_bytes phaddr; size_t len; void *reply; u8 padding[40]; }`——i386 下 phaddr/len 皆 u32，wire ep@0/phaddr@4/len@8。新 `MessLsysVmMapPhys`（`minix-types/ipc/message.rs`）字段类型与偏移逐字对齐。✅
  - VM 解码：`minix-types/ipc/vm.rs:1045` `decode_message` 读 `m_lsys_vm_map_phys.{ep,phaddr,len}`。✅ 与请求端同臂同偏移。
  - VM 回包：`servers/vm/src/ipc/encode.rs:183` `VmReply::MapPhys(out) => out.encode(m1)` → `EncodeToM1 for VmMapPhysOut`（vm.rs:1098）写 `m1.m1p1`。✅ 调用方新读 `message.m_u.m_m1.m1p1` 成立。
  - `perform_taskcall`（`minix-sys/src/syscall.rs:128`）经 `transport.sendrec(dst, message)` 把回复回填**同一 message 缓冲**（SENDREC 语义）——读 `message` 正确。✅
- **S2 高位 PA 拒绝**：`physical.0 > u32::MAX → EINVAL`——防 ≥4GiB 静默截断映射到错误物理页，诚实且必要。✅
- **测试**：`test_map_physical_returns_virtual_address` 增请求 wire 断言（ep/phaddr/len）+ 回包改 m1.p1，双向对账。✅

### 2.2 PL011 臂（`os/drivers/tty/tty/src/serial.rs` +230）
- 寄存器：`DR@0x00 / FR@0x18 / TXFF=bit5(0x20)`，符合 ARM PL011 DDI0183。QEMU-virt PA `0x0900_0000` 一页覆盖 DR/FR。✅
- 访问模型：驱动 init 期经 `VM_MAP_PHYS`（即 §2.1 修复的通道，PL011 为其**首个真机消费者**）把 UART 页映射入自身，volatile 读写；无 16550 式端口车道（aarch64 无端口 I/O），与 C ARM 板经 `vm_map_phys` 设备映射的串口后端对位。✅
- 单线程事件循环下"有界轮询 + 诚实部分写"（`transmitter_ready` 预算 `THRE_POLL_BUDGET`，卡死返回已写计数）——符合用户态服务器 `!Send`/单线程执行模型，无跨 CPU `Rc/RefCell`。✅
- 4 脚本化测试（drains all / full FIFO 0 moved / flags 探测失败停摆 / push 失败返回已写）逻辑自洽。✅
- `lib.rs` init `cfg` 分派（x86_64=16550 / aarch64=PL011，映射失败 fail-fast expect）——x86 路径逐字不变（非回归源）。✅

### 2.3 VM 内容探针对 DIRECT 跳过（`os/servers/vm/src/vm_server.rs`）
- `region.is_direct()`（`region/vir_region.rs:208`＝`flags.contains(VrFlags::DIRECT)`）在 `handle_pagefault` 前置，探针 `probe_ok = !is_direct_region && matches!(...)`。✅
- **前提核实**：DIRECT flag 生产唯一设置点 `map_phys.rs:93` `VrFlags::WRITABLE | VrFlags::DIRECT`——即 §2.1 的 MAP_PHYS 建的透传区确带 DIRECT，故 is_direct()=true、探针正确跳过。✅ 与 S1 注释一致（不能用 `matches!(param, Direct)`，匿名/文件区 param 默认 Direct{0} 会误杀全部匿名探针——判定正确）。
- 机理正确：MMIO 页不在 VM DM 窗（读即 VM 自缺页致命）且 DR 读弹 FIFO（吞用户输入），探针只对 demand-paged file/anon 内容有意义。✅

### 2.4 幻影猎捕：模式① 是否被续-132 "顺带消解" 成假象？
- **结论：marker 达成是真实的，且模式① 未复现——但团队判定为"归并至 FPSIMD 根因、幻影/不单写博客"是诚实的（见 §4/§6）。** 逐字节证据：
  - 修复后 pl011-6/7/8/9 中 `rs-bigalloc` 各 7 次，尺寸**全部合法**（0x15000/0x18000/0x1a000/0x1e000/0x3ea00/0x111000，ptr 非空）；`OOM-RT=0`、毒尺寸 `fffe25c0=0`、`fffe25/7ffffffe=0`；启动**越过**旧 OOM 停点直达 marker。
  - 旧模式①（续-38~41 四轮证伪 SP 对齐/TLBI/CoW 物化，毒值恒 `0x7ffffffe25c0`＝"经正确指针读到确定性写坏内存字·通路 A"）与续-129 定谳的 **`str d8` 踩 PM 消息头**同一机制：一个被踩的 FP 常量既能毁 `m_type` 头、又能毁 `String.len` 字。修复源头（内核禁 NEON + 懒 FPU 轮转）后两者同灭。
- **保留观察项**（非确认在开 bug）：若 riscv/x86 后续出现**与 FP 无关**的同类毒 len/cap 覆写，须单列复核，勿以"marker 达成"推定所有内存字陈旧路径皆已定谳。→ §7 AF-6。

## 3. 续-130 `9fc7c88f7` 审计（exec EINVAL 双根因）

**判定：PASS。无回归、无幻影。**

- **根因1 SYSCALL 消息腿屏障**（`minix-sys/src/syscall.rs` +`ipc.rs` pub(crate)）：`DirectKernelCallTransport::kernel_call` 在 trap 前插 `commit_message_to_memory`，与续-22 IPC 三腿同构，三架构 cfg 统一。机制：`message.m_type = call_number` 的 store 对 LLVM asm 不可见（eager 回执覆写整缓冲⇒死存储），可在 svc/ecall/syscall 前被消除/寄存器化。**诚实性**：commit 明确标注本屏障是"契约级防御·与 d9-FP-常量被踩根因（续-129/130 懒 FPU）**互补**"，未把它谎称为唯一根因。✅
- **根因2 懒 FPU 轮转从未生效**（`arch/src/arm64/boot.rs` 移除 CPACR 写 + 删 `mod cpacr`；`fpu.rs` init 0b11→0b01）：
  - 旧 `apply_to_trap_frame` 每次 dispatch `orr #(0x3<<20)`＝FPEN 实写 **0b11**（注释误标 0b01）且 orr 清不掉已置位 ⇒ 每次 dispatch 无条件全开 FP ⇒ EC=0x07 腿**永不触发**、归属策略被覆盖。**"删死 cpacr 模块"属实**——删除在 boot.rs 内嵌 `mod cpacr`（aarch64 + stub 两支）+ `use cpacr::*`，非独立文件，故 diffstat 无文件级删除（本 agent 曾疑，已核实）。✅
  - 最小正确性：策略唯一所有者改为 `finish_and_restore`（enable/disable，对位 C proc.c:443-446）+ EC=0x07 轮转。移除覆盖写而非改 0b01（后者会令 owner 重调度后自陷阱零恢复摧毁活值——RS boot panic 实锤）。✅
  - `fpu.rs::init` 现 FPEN=0b01（清 bit20-21 后置 bit20）＝"trap EL0 / EL1 放行"，编码正确；但该 init **未接线**（S1 裁决，真机 a64-fix-7 撞 pm fb201 墙后不冒险），0b01 门控由首次 `finish_and_restore` disable() 建立。→ 见 §7 AF-2。
- **验证**：commit 称 exec EINVAL 绝迹（ms-in 实证 raw=0x60d）、INIT 首达 MultiUser、panic=0、x86 不回归——与本 agent §5 串口复核（MultiUser 达成、WrongMessageType=0）一致。✅

## 4. 续-129 `15841fb17` 审计（FPSIMD 跨陷入不保存 → 禁 NEON/FP + 懒 FPU 闭环）

**判定：PASS。根因机器码级钉死、修复忠实对位 C、无回归。残边均为作者已登记跟踪项（§7）。**

### 4.1 根因（非幻影，逐层钉到指令与生产者）
A1「VFS barrier mt=8」＝ SD-23/A3「aarch64 FPSIMD 跨陷入从不保存恢复」的现行犯：PM `PmServer::init` encode 循环用 `ldr d8,[字面量池]`+`str d8,[x8]` 拷 Message 头 8 字节（aarch64 无 64 位立即数装载、LLVM 以 FP 寄存器做常量拷贝；x86 用 movabs GPR 故无毒，解释单架构性）；Vec push 触发堆页 demand-fault→陷入→调度切换→回来 d8＝他上下文残留 (1,8)，写脏头。gdb 读 hit 时刻 `d8=0x0000000800000001 ≠ 编译期常量` 坐实。

### 4.2 修复四处 + Ground-Truth 忠实度（本 agent 独立核 C 源码）
| Rust 改动 | C 对位 | 核实 |
|---|---|---|
| `xtask/src/image.rs`：`-p kernel-image`+aarch64 注入 `RUSTFLAGS=-C target-feature=-neon,-fp-armv8`；`kernel-image/main.rs` `compile_error!` 门（`all(aarch64, target_feature="neon")`）；`check-layout.sh` 同款注入 | C 内核懒 FPU「内核不用 FPU」前提（i386 CR0.TS/ARM FPEN） | 门逻辑正确（未带 `-neon`→cfg true→拒收）；三处构建路径一致 | ✅ |
| `trap_dispatch.rs` `aarch64_fpu_trap_body`（EC=0x07） | `copr_not_available_handler` **`minix3/minix/kernel/proc.c:1922-1962`**（本 agent 全树 grep 定位，**非** `kernel/src/kernel/proc.c`） | 逐行镜像：`disable_fpu_exception`→`get_cpulocal_var(proc_ptr)`→`local_fpu_owner` 非空且 `assert(!=p)`（→debug_assert）→`save_local_fpu(*owner, FALSE)`（retain=FALSE，EXT_REG_INITIALIZED 门）→restore→`*owner=p`→返回重执行（PARK_NONE，ELR 不前进）。✅ **非跨架构归因挪用**（proc.c 腿架构中立，C 仅 i386 有 trap 向量；earm arch_system.c fpu 为空 stub） | ✅ |
| `lib.rs finish_and_restore` fpu_owner `bsp_cpu_id()`→`current_cpu_id()` | `get_cpulocal_var(fpu_owner)`（`cpulocals.h:73` per-CPU）+ proc.c:443-446 | 修非-BSP CPU 见 BSP owner 跳过 disable 的双解锁洞；C:443-446 命名反相映射（C"enable_fpu_exception"=开陷阱=FPEN 0b01=Rust disable()）正确 | ✅ |
| `fpu.rs` save/restore 全 Q0–Q31 + FPSR/FPCR（抽 `#[target_feature(neon)]` per-fn）；restore clobber **有意豁免 v8–v15** | — | 见 §4.3 |

### 4.3 restore v8–v15 clobber 豁免（unsafe 契约审查）
- `fpsimd_restore_raw` 的 `out(...)` 声明 v0–v7、v16–v31，**不声明 v8–v15**（虽 `ldp q8..q15` 实写它们）。注释论证：若声明 callee-saved v8–v15 为 clobber，LLVM 会在 prologue spill、**在 ldp 之后的 epilogue reload d8–d15** 覆盖刚恢复的用户车道，复活本模块要修的腐蚀；豁免安全性依赖"全内核 `-neon,-fp-armv8`⇒本模块外无 V 活值跨调用"。B-1 反汇编实锤后已删。
- **本 agent 审计意见**：这是**已声明放弃的安全抽象**，其正确性依赖一条窄论据（"fpsimd_restore_raw 的调用方均在 -neon 模块内，跨该调用边界无活 d8"）。但该论据的前提"whole kernel -neon"被作者自认的 **build-std 残差**（rustup 预编译 core ~11 条 V 指令，字符计数/格式化路径）削弱。当前调用图下豁免成立（core fmt 与 EC=0x07 restore 路径不相交），但**不变量无测试守护**（B-1 承诺的"双 FP 用户回归用例"未落地）。→ §7 AF-1。

### 4.4 no_std / SMP+BKL / 内存安全
- EC=0x07 腿临界区：仅 `fpu.save/restore`（寄存器 asm）+ proc_table 读写 + CPACR 写，**无 sleep/schedule/IPC**，BKL 守跨进程 fpu_state 访问，`debug_assert!(fpu_bkl)` 防内核态误入。✅
- `fpu.disable()/enable()` 在 `bkl_unlock()` 之后：仅操作 **current_cpu 自身**的 CPACR + `cpu_local(current_cpu_id()).fpu_owner`，per-CPU 局部性⇒无跨 CPU 数据竞争（续-129 的 bsp→current_cpu 改动实际是**增强**而非引入风险）。✅
- 内核侧无 `Rc/RefCell` 跨 CPU 引入；`DirectTrapTransport` 别名 + `minix_types` 导入正确。✅

## 5. aarch64 rc marker 真输出逐字节核验（幻影猎捕核心）

对 `tmp/nk4a/a64-pl011-*.serial`（续-132 时段 Oct-1 原始串口）独立解析（不轻信 commit 摘要）：
- `a64-pl011-1..5`（6181–6200 行）`marker=0`——双 wire bug 未通，用户态 console 从未产出（与续-131 定谳"tty x86-only ⇒ aarch64 用户态输出从未存在"吻合）。
- `a64-pl011-6/7/8`（≈4522 行，22:10/14/17 连续三轮）：`marker=2 / panic=0 / MultiUser=1 / WrongMessageType=0`——**"三轮稳定"有 3 份独立 marker 正证据支撑，claim 成立**。
- `a64-pl011-9`（7491 行）：`marker=2`，两处命中为**两种不同真实形态**：
  - 第 6173 行 `minix-rs rc: minimal boot script marker`（裸文本）＝**echo 命令真实 stdout 经新 PL011 后端达控制台**；
  - 第 7375 行 `echo "minix-rs rc: minimal boot script marker"`（带引号）＝**cat /etc/rc 回显源文件行**。
  - 二者非 grep 误命中、非幻影；PL011 臂端到端成立（MAP_PHYS→TTY→volatile MMIO→QEMU console）。
- x86 非回归：`x86-pl011-1/2`（22:24/22:46）各 `marker=2 / panic=0 / MultiUser=1`。✅

> **局限声明（VERIFY-CHECK 同 agent 局限）**：本 agent **未在隔离 worktree 重新完整 build+boot aarch64/x86 全镜像**（主线程此刻在同一 host 活跃推进续-133、共享 `os/target/image/` 为其脏 WIP 镜像，fresh boot 反验到非快照态且争抢 CPU/QEMU）。真机结论以**独立逐字节解析已提交时段原始串口**为准（marker 计数、双形态、panic/MultiUser/WrongMT 均为本 agent 亲跑 `grep -a` 重放，非转述）。host 验证链则在 clean 隔离 worktree 全量复跑（§1）。

## 6. WORKLOG-only 取证（续-126/127/128/131）事实一致性

**判定：PASS——与事实一致、自纠诚实、riscv (A) 正确保持 OPEN，无"被后续推翻未纠偏"。**
- 续-131→132 链条闭合：续-131 定谳 tty x86-16550-only ⇒ 续-132 加 PL011 臂达 marker。✅
- 续-127 对续-126/123 的"VM 堆裸野指针不经页表"过度断言**显式"特此更正"**（证据推翻即改），把 riscv (A) 收敛到"页表帧 use-after-free/别名 vs 内联漏守"两模型交 gdb。✅
- 续-128 静态排除候选 (b)（表帧未清零）：本 agent 抽验其锚点真实——`alloc-reuse-PT` 检测器在 `servers/vm/src/alloc_page.rs`（`#[cfg(not(test))]`，位图界 `base_pfn < 512*64`＝自述"128MB 内帧"逐字吻合）；`vm_pt_alloc` 的 `write_bytes(virt, 0, CLICK_SIZE)` 在 `alloc_page.rs:68`（候选 b 排除依据）。✅
- **未坐实不成修**纪律保持：riscv (A) 明确留待 QEMU `-s -S`+gdb 硬件写 watchpoint 定谳（终目标① riscv 段仍 ❌，符合三目标现状）。

## 7. 待办登记（Findings & TODO，交主线程处置；本 agent 未修）

> 严重度沿用项目 P0/P1/P2 口径。多数为作者**已跟踪项的独立确认**（标注），少数为本审计新增观测。

- **AF-1（P1·invariant·作者已跟踪 B-1）**：`fpu.rs::fpsimd_restore_raw` 的 v8–v15 clobber 豁免**无回归测试守护**。承诺的"双 FP 用户回归用例"（两进程均活跃用 FP、跨陷入/跨 CPU 轮转）未落地。建议：写该用例 + 消除 build-std core V 指令残差（见 AF-4），把"窄调用图论据"升级为"编译期不变量 + 测试"。
- **AF-2（P2·completeness·作者已跟踪 S1）**：`AArch64FpuArch::init()`（FPEN=0b01）刻意未接线，门控靠首次 `finish_and_restore` disable()。当前可接受（注释充分）；若未来接 arch init，需重跑 a64-fix-7 类 pm-fb201 回归验证。
- **AF-3（P1·correctness-completeness·作者已跟踪 SF-2）**：`release_fpu` 在 aarch64 **exec/clear/sigsend 未接线**（`kernel/src/syscall_process.rs:452/599/1483` 均为注释 `// C: do_clear.c:60-61 release_fpu`；`x86` do_mcontext 标 x86-only）。风险：进程持有 FPU 归属时 exec ⇒ 新程序继承 stale Q 寄存器。C 对位 `proc.c:1960 release_fpu` + `do_exec.c:57` 等四点。建议按 SF-2 补齐并加用例。
- **AF-4（P2·build·作者已跟踪）**：build-std 残差——rustup 预编译 core ~11 条 V 指令不受 `RUSTFLAGS` 重建（AF-1 豁免不变量的已知削弱源）。建议引入 build-std 或等价手段清零，登记为独立工作单元。
- **AF-5（P1·process/coverage·本审计新增·区间外既有）**：`cargo test --workspace` **无法编译 `minix-tests` 集成测试**：`os/tests/pm_sched.rs` 的 `impl PmIpc for PmSide` 缺 PM `IpcTransport` trait 的 `send_blocking`/`sendnb`（该二法于锚点前 `5d9d57d0f` F13 引入 PM trait，mock 未同步）。因项目权威 host 测试用显式 `-p` 子集（排除 `tests/` 成员），**此断裂对绿色门禁不可见**（`5d9d57d0f` 起即存在，非 续-129/130/132 引入）。建议：修 stale mock 或把 `minix-tests` 纳入某条 CI 门，避免集成测试静默腐坏。
- **AF-6（观察项·非确认 bug）**：模式①（`String.len`/Vec cap 被确定性覆成栈址的"通路 A"）当前判为 FPSIMD 根因的下游症状（§2.4/§4），修复后 4 份串口无复现。**勿以"marker 达成"推定所有内存字陈旧路径皆已定谳**；若 riscv/x86 出现与 FP 无关的同类毒，须单列复核。
- **AF-7（相邻·区间外既有）**：aarch64/riscv `p_fault_addr` 填充腿架构无关但消费腿仅 x86（续-40 登记的真实设计缺口，无观测独立缺陷前不落地）；WORKLOG 行 3111 记 BKL 两步写在 idle-wake IF=1 窗口的同 CPU 自死锁面（须先于多核压力验证关闭，建议单 `AtomicIsize` 合并）。均非本区间引入，登记备查。
- **NIT-1（卫生）**：`vir_region.rs` `is_direct()`/`is_anon()` 仍带 `#[allow(dead_code)]`，但 `is_direct()` 已被 vm_server.rs:2024 消费（属性可移除）；WORKLOG 对 riscv `paging.rs` 的行号引用存在路径漂移（模式 #66 RCPD 卫生项）。

## 8. 一致性 / 可追溯核对

- **一节一 commit**：区间内每逻辑单元独立成 commit（含前沿滚更单独成 commit），符合 WORKLOG 交接约定。✅
- **WORKLOG 顶部前沿**：滚更至 续-132（`5a2ad09e6`），与 HEAD 一致。✅
- **锚点纪律**：生产 commit 均带 `file:line` + C 锚点（ipc.h:1504-1510 / proc.c:1922,443-446 / do_exec.c:57）；本 agent 逐条 grep 复核 C 锚点真实（更正 proc.c 路径见 §4.2）。✅
- **三处一致 ARCH 标注**：续-129/130/132 均为 **Rewrite/Refactor**（保持外部行为 / 死码清理 / bug 修正），非 Architectural Evolution，故**不触发 `[ARCH: ...]` 三处一致强制标注**要求；本 agent 核对其语义定位准确（懒 FPU 闭环是 C 既有模型的架构中立移植，非自创机制）。✅
- **修复顺序/最小正确性**：P0（消息头/FPU/双 wire）优先，改动面小、对偶端齐、无夹带重构。✅

## 9. 总体结论

| commit | 正确性 | 验证链真实性 | 幻影风险 | 回归 | 结论 |
|---|---|---|---|---|---|
| 续-129 `15841fb17` | ✅ 忠实对位 C proc.c 懒 FPU | ✅ 全链本 agent 复跑绿 | ✅ 非幻影（机器码钉到 `str d8`） | 无 | **PASS** |
| 续-130 `9fc7c88f7` | ✅ CPACR 覆盖移除 + 屏障互补 | ✅ 1400/check-layout PASS | ✅ 屏障诚实标注为互补防御 | 无 | **PASS** |
| 续-132 `2a6c55912` | ✅ 双 wire + PL011 + DIRECT 护栏 | ✅ 1771/marker 逐字节验真 | ✅ marker 真输出·模式① 判定诚实 | 无（x86 不回归） | **PASS** |
| 续-126/127/128/131 | ✅ 取证与事实一致 | 锚点抽验真实 | ✅ 自纠诚实·riscv(A) 正确 OPEN | N/A | **PASS** |

三条生产改动**均为真修、非幻影、无引入回归**，验证链数字与真机 marker 独立复核全部对得上。残边（release_fpu / 双 FP 用例 / build-std）作者已登记、本审计确认恰当。终目标① aarch64 段"首次真机 rc marker"达成属实。

## 10. Rule Discovery（可泛化经验）

1. **commit message 的 C 锚点须以全树 grep 定位、勿按直觉路径**：本审计一度按 `minix3/kernel/src/kernel/proc.c` grep `copr_not_available_handler` 落空（疑锚点造假），实际在 `minix3/minix/kernel/proc.c`。**锚点"未命中"≠"造假"**，须换路径重搜再判（否则误报 P0-fact）。
2. **host 测试数字须绑定其 `-p` 包集**：同一 commit 的 "1400/0" 与 "1771/0" 并非矛盾，而是 4-crate 子集 vs 6-crate 权威集。审计复跑若用 `--workspace` 会牵入未纳入正式门禁的 `tests/` 成员（见经验 3），产生假回归。**先确认权威测试命令的精确包作用域再对账**。
3. **显式 `-p` 子集式门禁会遮蔽 workspace 内其它成员的编译断裂**：`minix-tests/pm_sched` 长期编不过却因权威测试不含它而"全绿"（AF-5）。门禁设计应至少含一条 `cargo build --workspace --all-targets`（或编译期 lint job）兜住集成测试腐坏。
4. **"症状消失"与"marker 达成"不足以定谳独立根因**：模式① 被续-129 单一 FPSIMD 根因归并解释、且修复后不复现，团队据此判"幻影/不单独写博客"是**恰当的克制**（未冒认独立根因）。反面教训见续-38~41：四轮"修复点定位错误"的真机证伪，全靠毒值恒定（`0x7ffffffe25c0`）这一特征反证"经正确指针读到确定性写坏内存字"，才排除 SP 对齐/TLBI 误归因——**排除法收敛≠定谳，须钉到具体 store 指令及生产者**。
5. **安全契约豁免须有测试锚定不变量**：`fpsimd_restore_raw` 的 v8–v15 clobber 豁免正确但依赖窄调用图论据，且前提被 build-std 残差削弱；"承诺的回归用例未落地"= 一条无守护的隐性 UB 面。凡"故意不实声明 asm 破坏的寄存器"都必须配一条能捕获其破口的测试（对应 fix-guard 之外，属 unsafe 契约演进规则候选）。

## 11. 追加审计：续-133~续-137（主线程新推进区间 `5a2ad09e6..91a708653`，2026-10-02）

> 第二轮审计。截止点更新为 HEAD `91a708653`（续-137）。新增 9 commit；含生产码的两个：**续-133 `f89b7c731`**（riscv 懒 FPU + 控制台臂，9 文件 +386/−117）、**续-135 `80eb217a3`**（pm/vfs/vm，探针轮，4 文件 +72/−18）。余为 WORKLOG-only 取证（续-134/136×2/137）+ 前沿滚更×2。
> **复核方式**：派 1 个 CodeReview 子代理审区间生产码（完整性/正确性/影响面一体），**本 agent 独立复验其承重结论**（P0 / 探针遗留 / 幻影），逐条读 HEAD blob + `git show --stat` 重放，**未盲信子代理、亦未轻信 commit message**。当前脏树仅主线程续-138 WIP（`trap_dispatch.rs` 帧 GPR 探针），未触碰。

### 11.1 续-133 `f89b7c731` — 部分真修 + 一处**已确认 P0** + 幻影 (A)
- **真修（kernel↔user FP 面）**：kernel-image **riscv64** `-C target-feature=-f,-d` 注入，三处构建路径一致（`xtask/src/image.rs:539-554` / `check-layout.sh:147-149` / `main.rs:53-60` `compile_error!` 门）⇒ 消除"内核 345 条 FP 指令踩用户寄存器"（SD-23 riscv 同族、aarch64 续-129 的 riscv 对偶）。tty **riscv64 NS16550A MMIO 臂**（`serial.rs:292-339`、`lib.rs:66-75`）PASS：与 aarch64 WirePl011 同构（VM_MAP_PHYS 单页 + `read/write_volatile`、`map()->Result<_,i32>` fail-fast），差异仅寄存器宽度（byte-wide，`reg-io-width=1`），用户态单线程执行模型合规。
- **P0（本 agent 独立确认，非子代理臆断）— 懒 FPU 门控不可达**：`os/kernel/src/lib.rs:3714-3719` 给**非 owner** patch `ctx.sstatus.FS = FS_INITIAL(0b01<<13)`。但本 crate **自己的** `os/arch/src/riscv64/fpu.rs:15-19` 头 doc 明示 `0b00 (Off): FPU instructions trap / 0b01 (Initial): FPU available`；谓词 `riscv64_illegal_insn_is_fpu_trap`（`trap_dispatch.rs:1743`）判 `entry_sstatus & (0b11<<13) == 0` 即 **FS==Off** 才算门控陷阱。⇒ 非 owner 返回 user 后 FS=Initial ⇒ 首条 FP **不陷阱**（谓词亦会拒）⇒ `riscv64_fpu_trap_body`（`trap_dispatch.rs:2307+`）轮转腿＝**不可达死代码**。叠加代码自述"riscv FS 住 sstatus 帧内、活写被 trap stub 出口帧回装冲销、载体是 ctx"（lib.rs:3707-3713）⇒ `fpu.disable()` 的活写对 user 模式亦不持久。**后果**：riscv **user↔user FP 未受保护**（禁 FP 只护 kernel↔user）。注释声称"Initial→首条 FP 陷阱轮转"与 ISA + 自身文档直接矛盾。**修法（交主线程）**：非 owner 用 `FS_OFF = 0b00<<13`。
- **P1 名值不符（同处确认）**：`FS_DIRTY = 0b10<<13`（lib.rs:3714 与 `trap_dispatch.rs:2409` 帧 patch）——按自身 doc `0b10=Clean / 0b11=Dirty`，冒名（owner 路径 0b10/0b11 皆不陷阱故暂无功能 bug，但常量名与 ISA 值冲突）。
- **P1 幻影修复（(A) 签名消失）**：续-133 自述"真机 gh5/gh6 连续两轮 panic=0/pagefault-for-VM=0（(A) 签名消失）"，被**续-137 `91a708653` 明文作废**："修正续-133 签名消失误判（gh5/gh6 行数未到）"；WORKLOG 顶栏改写为 gh3/gh4/gh7 (A) 家族签名三现（`0x10bd2cabac` 确定性同址），gh5/gh6 仅行数未到 16k 未触达。⇒ **riscv (A) 根因仍 OPEN**，续-137 重归因到"GPR 恢复破坏（非 FP）"。典型"症状偶然未复现冒认根因"——**但续-137 已诚实自纠，纠偏纪律到位**。
- **P1 验证链假绿**：commit 称 "host1771+829/0·clippy55·rustfmtΔ0·check-layout PASS"——该全绿**恰因** P0 死门控在 host 单测与编译/布局门禁中运行期不触发、不可见 ⇒ 绿灯为该 P0 提供假信心（见 §11.6 模式 85）。本 agent 未对续-133 全镜像重跑交叉 boot（主线程活跃 + 共享 os/target），但即便重跑亦不能暴露此 P0。

### 11.2 续-135 `80eb217a3` — 探针轮 + **P1 过程违规（自述 tracked 净 不实）**
- 四处均**取证探针**（非生产逻辑）：`PMR_N`（`pm/init.rs:448`，门控 64 + `sys_diagctl_write` 打 14B）、`VFM_N`（`vfs/main_loop.rs:7564`，门控 64）、`PF_RECV_N/PF_BYTES_N`（`vm/vm_server.rs:1985/2052`，first-200 门控）。commit message 亦自述探针工作。
- **本 agent 独立确认探针存活 HEAD**：`FTRAP_N`（`trap_dispatch.rs:2345`，`nk4a: rvfpu-trap`，且因 §11.1 P0 **永不打印**）/ `PMR_N` / `VFM_N` / `PF_RECV_N` / `PF_BYTES_N` 全在 HEAD blob。**续-137 `91a708653` 的 `git show --stat` 显示仅改 `NK4C-WORKLOG.md` 1 文件、代码零改动**，然其 message + WORKLOG 断言"探针已滚 tracked 净"＝**自述与 git 事实不符**，违反"探针用后即滚、tracked 净"纪律（P1-process-fact）。
- 探针用 `AtomicUsize + Relaxed`，用户态单线程服务器不违规；真正问题是**该滚未滚**，非并发模型。

### 11.3 WORKLOG-only（续-134/136×2/137）与一致性
- 乒乓环 `0xFFFF0FB2`（m_type=-1 作 i16 / tid=4018=VFS_TRANSID）泄漏进 PM/VFS 主循环投递车道的取证，逐层收窄且续-136 追加机制精化（`is_vfs_pm_rs=false` 落 `dispatch_message` ENOSYS 回程腿）——内部自洽、未见被推翻未纠偏。
- WORKLOG 顶栏滚到续-137 ✓；一节一 commit（续-136 两 commit 合写一节属"追加"模式，可接受）；C 锚点 `proc.c:1923-1962` 系 **off-by-one**（实际起始 1922，本 agent 首轮已定位，卫生 NIT）。

### 11.4 待办登记（AF-8+，交主线程处置；本 agent 未修）
- **AF-8（P0·code-bug·本 agent 独立确认）**：riscv 非 owner ctx-patch `FS_INITIAL(0b01)`→应 `FS_OFF(0b00)`；否则懒 FPU 轮转死代码、riscv user↔user FP 未护。`os/kernel/src/lib.rs:3714-3719`。
- **AF-9（P1）**：`FS_DIRTY=0b10`→`0b11`（或改名 Clean），`lib.rs:3714` + `trap_dispatch.rs:2409` 两处 + 同步 `boot.rs:44-52` doc。
- **AF-10（P1）**：加 host 单测钉"非 owner→FS=Off / owner→FS=Dirty"（mock 表检 `sstatus & 0x6000`），防门控语义再静默破。
- **AF-11（P1·process）**：清理 5 处探针遗留（FTRAP_N/PMR_N/VFM_N/PF_RECV_N/PF_BYTES_N + 相应 `nk4a:` marker），使 HEAD tracked 真净；纠正续-137 WORKLOG"探针已滚"表述。
- **AF-12（NIT/文案）**：`xtask/src/image.rs:553` riscv 分支误打"aarch64 内核禁 NEON/FP"日志，按 arch 分支文案；C 锚点 `1923`→`1922`。
- **OPEN**：riscv (A) 真根因（续-137 归 GPR 恢复破坏）待续-138 帧 GPR dump 定谳（主线程在飞，HEAD 之外）。

### 11.5 新区间结论
| commit | 正确性 | 幻影风险 | 结论 |
|---|---|---|---|
| 续-133 `f89b7c731` | 禁 FP(kernel↔user)+控制台臂＝真修；懒 FPU 轮转＝**P0 死代码** | **(A) 签名消失＝幻影**（续-137 已自纠）；验证链假绿 | **有实质缺陷，不可放行 PASS** |
| 续-135 `80eb217a3` | 探针轮（无生产逻辑） | 自述 tracked 净 **不实** | **P1 过程违规·待清理** |
| 续-134/136/137 | 与事实一致 | 续-137 诚实自纠续-133 幻影 ✓ | **PASS**（纠的是结论，P0 码 bug + 探针遗留仍未跟） |

### 11.6 Rule Discovery（新区间新增模式候选）
- **模式候选 84「门控位态-语义错配」**：跨架构移植懒陷阱时 ctx/frame 写了"看似合理"的非触发位值（Initial），未对照同 crate 头 doc/ISA 验证"哪个位值才触发"。检查：`rg 'FS_INITIAL|FS_OFF|set_fs_field|riscv64_illegal_insn_is_fpu_trap' os/ -t rust` 交叉 常量名→位值→谓词要求是否闭合。
- **模式候选 85「编译/单测门禁遮蔽运行期门控语义」**：永不触发的 trap 腿（死代码）在 host 单测 + check-layout 中全绿不可见→假信心。运行期可达性须靠**真机正证据**（探针命中计数>0）坐实；"探针 0 命中"应判"腿未通"而非"无事发生"。
- **模式候选 86「'tracked 净/已滚'类自述须配代码 diff 证明」**：任何"探针已滚"断言必用 `git show <sha> --stat` 证代码文件真被动 + `git show HEAD:<file> | grep <marker>` 证已消失。本轮续-137 改 WORKLOG-only 却声称代码滚净＝不符。
- （承 §10 经验 1「C 锚点全树 grep 定位」——续-133 `proc.c` 1923 off-by-one 再次印证锚点须逐条对表。）

---
*本 agent 全程中文、未改/未提交 `AI-chats/daily.todo.md`、未 `git add -A/.`、未触碰主线程脏树 WIP。§0–§10 为首轮（截至续-132 `5a2ad09e6`，隔离 worktree 复跑 host/clippy/fmt/check-layout + 原始串口逐字节验真）；§11 为追加轮（续-133~137，CodeReview 子代理审区间生产码 + 本 agent 独立复验 P0/探针/幻影）。*
