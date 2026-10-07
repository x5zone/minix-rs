# NK4-C 缺陷静态分析（独立第二意见）：aarch64 子进程把栈地址当 Vec 容量

> 本文是对 `NK4C-BUG-AARCH64-VEC-CAP.md` 交接件的独立静态分析产物。
> 性质：只读代码、跨架构对照、产出可被真机探针二选一判别的候选点；
> 不下根因结论、不改任何代码。已完整阅读交接件第 3 节的已证伪清单，
> 本文所有候选均不重提清单内的死路；与清单条目有接触面处，均显式说明区别。
> 所有行号均由 Read 工具当场生成（标注为 Lnnn）。

---

## 0. 一句话结论

跨架构对照后，最能解释"计数/长度槽位里落进一个完整未拆分的 64 位栈指针"
（毒值 `0x7ffffffe25c0`）的机制不在 exec 栈镜像的字节布局里——那部分
两架构逐字节同构——而在**寄存器组的保存/恢复不对称**里：x86_64 把
"用户起源中断必须先存完整寄存器组再跑任何调度策略"当作承重不变量
（有真机前科与修复），aarch64 的同位腿没有这道保存。其余两个候选分别
覆盖 exec 初始栈指针的对齐差、以及崩溃映像归属（init 还是 /bin/sh）。

---

## 1. 候选一览

| 编号 | 候选 | 一句话机制 | 置信 |
|---|---|---|---|
| 候选一 | 用户起源中断不存帧 → 陈旧寄存器组恢复 | 恢复用上一次陷入时保存的旧寄存器组，旧值里被调用者保存寄存器装着栈上对象地址，活代码把它当长度/容量 | 主推 |
| 候选二 | exec 初始栈指针 16 字节对齐差 | x86_64 入口桩自我修正对齐，aarch64 不修正，帧大小只保证 8 字节对齐 | 低（更像放大器） |
| 候选三 | 崩溃映像归属未定 | 毒 Vec 可能在 exec 之后的 /bin/sh 里，续-33 的 ELF 符号对账无法区分共享库代码 | 定位前置 |

---

## 2. 静态考察覆盖面与排除记录

这一节先交代读了什么、哪些路走不通，避免后续分析者重复劳动。

**exec 栈镜像的字节布局两架构同构，予以排除。**
exec 帧由用户态共享 Rust 代码构造：`os/commands/sbin/init/src/execve.rs::stack_fill`
写 argc 槽（8 字节）、argv/env 指针数组、字符串、`PsStrings` 描述符
（`argv_str`@0 / `n_argv:i32`@8 / `env_str`@16 / `n_env:i32`@24），
消费端 `os/libs/minix-rt/src/crt0.rs::read_process_strings` 用完全相同的
`PsStringsRaw` 布局读回。生产与消费是同一份架构无关代码，帧字节在
x86_64 与 aarch64 上逐位相同。计数槽（`n_argv`/`n_env`）是 `i32`，
且 `os/libs/minix-rt/src/handoff.rs::ProcessStrings::from_raw` 拒绝负数
（负数走 `EINVAL` 提前退出，不是本次崩溃形态）；一个完整 64 位栈指针
即使落进 `i32` 槽也会因最高位为 1 呈负数被拒。**描述符字段本身产不出
"正的大计数"。**

**栈顶三方常量一致，帧落位与帧内指针不会错位，予以排除。**
调用者算帧内绝对指针用的栈顶来自内核信息页 `kui_user_sp`
（`os/commands/sbin/init/src/execve.rs::new_image_stack_top`，内核侧
`os/kernel/src/kerninfo.rs` 的 `kui_user_sp = kernel_info.user_sp().0`），
其值由 UEFI 载体写死为 `0x0000_7fff_ffff_f000`
（`os/boot-shim/src/uefi_helpers.rs::build_kernel_info`——x86_64 与
aarch64 共用这条载体，aarch64 只是改为跳入高半区内核镜像，
`os/boot-shim/src/main.rs` 的 aarch64 分支）；VFS 服务端放帧用的
`DEFAULT_USER_SP` 同值（`os/servers/vfs/src/main_loop.rs` L1136，工具生成）。
调用者的 `vsp` 与 VFS 的 `vsp` 因此相同
（`os/servers/vfs/src/exec_worker.rs` L364，工具生成），帧内指针与实际
落位不会错开。riscv64 的 OpenSBI 载体用 `0x0000_003f_ffff_f000`
（`os/boot-shim/src/opensbi_helpers.rs`）是另一架构的事，与本缺陷无关。
真机上实际发布值是否确为该常量仍值得探针顺带打印（见 §6）。

**出站 IPC 消息零初始化，联合体 padding 不携带旧栈数据，予以排除。**
minix-sys 的全部系统调用包装从 `Message::zeroed()` 或
`..Default::default()` 出发再写字段（`os/libs/minix-sys/src/syscall.rs::cleared_message`
及 L323/L372 等处的字面量，工具生成；联合体 `Default` 为全零），
消息缓冲虽是栈上局部，未写字段是零而非陈旧指针。read 回复的计数字段
在固定 `repr(C)` 偏移（`os/libs/minix-sys/src/vfs.rs::ReadWritePayload`），
两架构同为 LP64，布局相同。**这与交接件第 4 节(丙)的担心正面相对：
按 64 字节硬上界复核后，此路不通。**

**消费侧的字符串扫描与读文件循环产不出大分配，予以排除。**
`os/libs/minix-rt/src/crt0.rs::string_at` 按 NUL 扫描计长（与续-33 排除
一致，不重提）；/bin/sh 读 /etc/rc 的循环
（`os/commands/bin/shell/src/bin_support.rs::read_file`）以 4096 字节
分块，`chunk[..count]` 在计数越界时会先 panic 而不是请求大分配。

**对已证伪清单的一条论据修正（排除结论本身仍成立）。**
续-29/续-33 对 `execve.rs` 组帧 `Vec` 的排除，其"超大长度先触发 E2BIG
早返"论据不成立：`os/commands/sbin/init/src/execve.rs::stack_params`
的 `checked_add` 只拦截 u64 回绕，不设绝对上界，
`0x7ffffffe25c0` 量级的 `item.len()` 不会回绕、会一路算出巨大的
`frame_size` 递给 `try_reserve_exact`（execve.rs L318，工具生成）。
排除依然成立的真实理由有两条：一是失败路径优雅——`try_reserve_exact`
出错映射为 `Errno::E2BIG` 返回（execve.rs L318-320，工具生成），与观测
到的 null 解引用 SIGSEGV 不符；二是真实上界在服务端
（`os/servers/vfs/src/exec.rs::ARG_MAX` = 256 KiB，
`os/servers/vfs/src/exec_worker.rs` L341 在拷贝前拒绝）。另外组帧路径
的请求大小会是"毒值加最小帧 56 字节再加槽位"，与观测的
`size = 0x7ffffffe25c0` 恰等于毒值本身的形态对不上。**把这条论据修正
写在这里，是为了防止未来有人把"E2BIG 早返"当成已证事实继续引用。**

**陷入入口的汇编保存/恢复布局核对无误。**
`os/arch/src/arm64/trap_stub.rs` 的 `EL0BODY`/`EL1BODY` 宏保存
x0–x30 + 被打断的 SP + ELR + SPSR，帧布局由
`trap_stub.rs::test_frame_layout_frozen` 钉死；恢复侧
`os/arch/src/arm64/trap_return.rs::restore_to_user` 从 `gp_regs` 装
X1–X30、从命名域 `r0` 装 X0。逐条比对未见错位。
aarch64 各陷入腿的存帧调用点也逐一核对：SVC 的 IPC 腿在分发前存
（`os/kernel/src/trap_dispatch.rs::aarch64_ipc_dispatch_body` L2482，
工具生成）、内核调用腿在挂起前存（`aarch64_kernel_call_leg` L2420）、
缺页腿存（`aarch64_pagefault_body` L2323）、SIGSEGV 腿存（L2282）。
**唯一的空白就是下面候选一指出的用户起源 IRQ 腿。**

---

## 3. 候选一（主推）：用户起源中断不保存寄存器组，陈旧恢复把旧栈指针装进长度寄存器

**(a) 文件::符号**

- 缺陷面：`os/kernel/src/trap_dispatch.rs::aarch64_user_body` 的 IRQ 分支
  （L2017-2022，工具生成，转发 `aarch64_kernel_body`）与
  `os/kernel/src/trap_dispatch.rs::aarch64_kernel_body` 的定时器/量子臂
  （`local_tick` + `check_quantum` + 设备钩子链，全程无存帧调用）。
- 对照面：`os/kernel/src/trap_dispatch.rs::x86_trap_dispatch_body`
  （`#[cfg(target_arch = "x86_64")]` 门内，两处
  `save_irq_frame_to_context` 调用：L624 的本地 APIC 时钟 vector 0xF1 臂、
  L709 的通用 IRQ 臂）及其测试核心
  `os/kernel/src/trap_dispatch.rs::mirror_irq_frame_into_proc`
  （L130 的 `#[cfg(target_arch = "x86_64")]`——**aarch64 上这个函数
  根本不存在**）。

**(b) aarch64 相对 x86_64 具体差在哪**

x86_64 上，每一次打断用户态的中断都在运行任何调度策略**之前**，把被
打断的完整用户寄存器组（含被调用者保存寄存器）镜像进进程表项的
`cpu_context`。x86 臂内注释明言这道保存的目的："do it before the
quantum check so a preempt-and-wake dispatch never reads a stale register
file (NK4-A Task C)"——并且记录了没有它时的真机后果：RS 的 RBX
（被调用者保存寄存器）在缺页/IPC 往返后回卷成 0，`&self.table` 丢失、
`endpoint_slot` 崩溃（真机编号 c19a-c21a）。这是本仓库已经付过学费的
同型缺陷。

aarch64 上，用户起源 IRQ 分支直接 `return aarch64_kernel_body(frame, class)`，
该臂运行量子检查——`check_quantum` 可经
`os/kernel/src/proc_table.rs::sched_proc_no_time` 做 `rts_set(NO_QUANTUM)`
把当前进程**出运行队列**并从内核给用户态调度器发通知——以及设备
钩子链，全程不调用 `save_frame_to_context`。分支注释自我声明依赖一个
口头承诺："Timer-driven preemption does not yet switch here (returns
PARK_NONE); quantum flags are set and the process is rescheduled at its
next voluntary IPC"。也就是说：同一形状的不变量，x86_64 用代码保证，
aarch64 用"暂时不会切"保证。当前出队后靠 `eret` 返回用户态继续跑、
下一次自愿陷入时再被调度的设计暂时自洽，但只要这条腿下发生任何一次
上下文切换（钩子链将来加入会阻塞的操作、量子检查行为演化、或多处理器
下对当前进程 `cpu_context` 的并发写读组合），被切断的进程稍后经
`os/kernel/src/lib.rs::finish_and_restore` 恢复时，读到的就是它**上一次
SVC 陷入时**保存的寄存器组——比真实打断点旧了若干层调用帧。

**(c) 什么值落进计数/长度槽、为何恰是栈地址量级**

Rust release 构建把切片长度、`Vec` 的容量、循环上界常驻在被调用者
保存寄存器（aarch64 的 x19–x28）里。旧一次的保存里，这些寄存器曾经
装着栈上局部对象的地址——INIT 与它的 fork 子共享同一套用户栈虚地址
布局（每个进程的栈顶都是同一个常量，`os/servers/vm/src/vm_server.rs`
的栈窗口按 `user_sp` 取），且 fork 已把父栈内容连同其中的旧指针一并
复制给子（`os/kernel/src/proc.rs::fork_from` 整体复制地址空间）。旧值
的形状正是 `0x7ffffffe2xxx`——与毒值 `0x7ffffffe25c0` 同区。恢复后，
活代码把旧地址当长度/容量使用，`Vec::reserve` / `Vec::with_capacity`
以 `0x7ffffffe25c0` 为容量走 `alloc::raw_vec::RawVec::finish_grow` →
`minix_rt::alloc` → `alloc_big`——正是串口看到的
`nk4a: rs-bigalloc size=0x7ffffffe25c0`；分配返回 null 后调用方解引用
null（`cr2=0x38`）成 SIGSEGV。这个机制同时解释三件事：值恰好是完整
未拆分的栈地址、为什么只在 aarch64、以及为什么共享 init 逻辑在 x86_64
完全正常。

与已证伪清单的关系：续-27/28/31 被证伪的是"消息高位字污染 INIT 栈、
经 CoW 遗传"这条**数据通路**；本候选是**寄存器组**通路——毒值不经
任何消息、不经栈上交付足迹，直接从内核保存的旧寄存器组回到活代码。
续-25 排除的是"父子共享陈旧 delivermsg 物理页"，与寄存器组无关。

**(d) 真机探针配方（二选一判别）**

在 `os/arch/src/arm64/trap_stub.rs::save_frame_to_context` 的每个调用
站点、以及 `finish_and_restore` 恢复 aarch64 寄存器组之前，各打一行
`(进程号, elr, sp_el0, x19, x24, x28)`（走既有 SYS_DIAGCTL 直达串口
管道，与 nk4a 系探针同法；限流、去重，避免时序扰动）。

- 候选一**排除**的判据：每次恢复行的五元组都精确等于该进程最近一次
  保存行（elr 落在 svc 指令的后继）。
- 候选一**坐实**的判据：任一恢复行的 elr 不是任何保存行出现过的地址
  （恢复点不在陷入边界上），或寄存器组相对上次保存出现"旧值复活"——
  例如 x19 重新变成 `0x7ffffffe2` 区指针、而该值只在更早的保存行出现过。

按交接件第 7 节纪律：两次独立复跑签名一致才作数；探针打印自身会改变
时序，判据以"恢复点是否为陷入边界"这一结构性事实为主，不依赖单次
寄存器值的巧合。

---

## 4. 候选二（低置信）：exec 初始栈指针的 16 字节对齐差

**(a) 文件::符号**

`os/libs/minix-rt/src/crt0.rs::_start` 的 aarch64 腿（L440-451，工具
生成）对照同文件 x86_64 腿（L396-409）；对齐粒度源头
`os/commands/sbin/init/src/execve.rs::stack_params`（L103，工具生成，
`div_ceil(SLOT) * SLOT`，SLOT=8）。

**(b) aarch64 相对 x86_64 具体差在哪**

x86_64 入口桩先 `and rsp, -16` 再 `call`，无论内核给的初始栈指针奇偶
如何，都把对齐修正为 16 的倍数；aarch64 入口桩只有裸 `bl {birth}`，
把返回地址压到初始栈指针减 8 后不做任何对齐修正。而初始栈指针 =
栈顶常量减 `frame_size`，`frame_size` 只保证 8 字节对齐。帧内容是
架构无关字节，所以同一份 argv/env 在两架构算出的 `frame_size` 奇偶
相同：当 `frame_size` 是 16 的倍数时，aarch64 新进程整个生命周期栈指针
都差 8 字节对齐，x86_64 却被入口桩悄悄治好。**同一输入、两架构行为
确定性地不同**——这是本次对照中 exec 栈布局上唯一找到的确定性差异。

**(c) 什么值会落进计数槽、为何量级可疑**

诚实地说，这条候选从对齐差到毒值的链条是三个候选里最间接的：QEMU
默认关闭严格对齐检查（SCTLR_EL1.A = 0），通用寄存器的普通访存不会
因差 8 字节对齐出错；真正的风险面是编译器按"函数入口栈指针必为 16
的倍数"这一 AAPCS64 前提生成的栈相对寻址推导，以及内核压信号帧时的
取整方向——它们在错位时可造成**静默的半格偏移读**（把相邻槽位的
内容当本槽位读），栈上相邻槽位恰恰是指针与长度交错排列的地方。
它被列入是因为修复成本几乎为零且与"只在 aarch64"完全一致，更可能
作为放大器（把候选一的旧寄存器值或别的偏移错位半格）而非独立根因。

**(d) 真机探针配方（二选一判别）**

在 `os/kernel/src/syscall_process.rs::dispatch_exec` 已有的
`nk4a: exec` 打印行（L298-306，工具生成）里补打 `exec_msg.stack` 及
`exec_msg.stack % 16`。

- 候选二**排除**的判据：崩溃复现次初始栈指针恒为 16 对齐（余 0）。
- 候选二**保持怀疑**的判据：恒差 8。此时再做一次对照复跑：给 init 的
  exec 环境表追加一个奇数字节长度的变量，翻转 `frame_size` 的奇偶，
  看崩溃是否随之出现/消失——出现/消失跟随奇偶翻转即为对齐参与实锤。

---

## 5. 候选三（定位前置）：崩溃映像归属未定——毒 Vec 可能在 exec 之后的 /bin/sh 里

**(a) 文件::符号**

`os/commands/bin/shell/src/bin/sh.rs::main`（/bin/sh 是独立二进制，
链接同一份 minix-rt）与 `os/commands/sbin/init/src/execve.rs::exec_command`
（fork 到 exec 之间的窗口，运行 minix-init 镜像）。

**(b) 差异不在两架构之间，而在归属判断的证据强度上**

续-33 的有界扫栈把 `finish_grow@0x220d58`、`minix_rt::alloc` 全局分配器
壳 `@0x22bb68` 对到 minix-init 的 ELF 符号，得出"init 二进制自身的某个
Vec"。但这两个符号是 init 与 sh **共享的库代码**，两个 ELF 都包含它们；
若返回地址恰好落在共享库函数的公共区间，符号对账无法区分两份镜像。
串口顺序上 `rs-bigalloc` 之后才是 `w-finw`/`pf-exit`/`csig`，exec 是否
已完成没有被独立证明。

**(c) 归属如何改变"计数槽"的搜索面**

若崩溃在 exec 之后：消费面换成 sh 的启动链——
`os/commands/bin/shell/src/bin/sh.rs::run_status` 把 argv/env 收集为
`Vec<String>`、`os/commands/bin/shell/src/bin_support.rs::read_file`
读整个 /etc/rc（已核实该腿自身产不出大分配：分块读、越界先 panic）。
这些消费面自身都写不出 `0x7ffffffe25c0` 量级的请求，所以"计数槽落
栈指针"仍只能来自寄存器组或帧错位类机制（候选一/二），**但后续取证
与修复的代码位置完全不同**。若崩溃在 exec 之前：维持在"init 在
fork→exec 窗口内某 Vec"的现有框架。

**(d) 真机探针配方（二选一判别）**

崩溃现场（`pf-exit` / `csig tgt=0xc` 打印的同处）补一行内核侧只读
打印：slot 12 的 `p_seg` 文本段虚地址区间；与 minix-init、/bin/sh 两份
当场重建的 ELF 的 `.text` 区间核对。

- 区间落在 sh 的 ELF → 归属改为 exec 后，候选一/二的探针照做，解读
  对象换成 sh 的调用链。
- 区间落在 minix-init → 崩溃在 fork→exec 窗口内，现有框架维持。

镜像必须从当前源码树当场重建（交接件第 7 节纪律），旧镜像跑出的
符号区间不作数。

---

## 6. 附：两条几何疑点，建议随探针顺带取证

**栈深偏大。** INIT 的投递缓冲在 `0x7ffffffe2578`、子进程崩溃点栈指针
在 `0x7ffffffc63a8`。若栈顶确为 `0x7ffffffff000`，两者分别距顶约
121 KiB 与 232 KiB——对正常 Rust 调用链偏深。两个解释都值得排查：
要么真机载体发布给用户态的 `kui_user_sp` 与常量不符（那么落帧几何
全部要重算，§2 的"栈顶一致"排除要重新验证——探针只需在
`new_image_stack_top` 的消费端打一行实际读到的值）；要么子进程确实
下探了异常深的栈（大栈帧或递归，本身可能就是错位的症状）。

**boot 装机路径的 ps_strings 布局与 exec 路径不同。**
`os/arch/src/arch/boot.rs` 的装机腿用 `sizeof(i32)`=4 作 argc 槽宽
（`argvstr = sp + 4`），exec 路径的 `execve.rs` 用 8（LP64 修正）。
由于装机路径的 `n_argv`/`n_env` 恒为 0（boot.rs L660-662，工具生成），
计数不迭代、指针不被消费，**当前无害**；但它是一座埋着的布局分叉，
任何未来给 boot 镜像传参的改动都会踩到，记录在此备查。

---

## 7. 边界声明

- 本文全部结论为静态推断，按"未坐实不成修"纪律，三个候选都必须先过
  §3/§4/§5 的探针判别，任何一步都不构成修复依据。
- 候选一若坐实，其修复形态是把 x86_64 的"中断先存帧"不变量补到
  aarch64 的用户起源 IRQ 腿（一处存帧调用 + `mirror` 的 aarch64 版），
  属局部改动，不波及全系统 IPC 或内存管理；候选二的修复形态是入口桩
  对齐或 `frame_size` 升到 16 字节对齐，同样是单点改动。此为范围预估，
  非修复方案。
- 本文与交接件已证伪清单的接触面已在 §2/§3 显式声明；除论据修正那条
  外，未发现清单条目需要推翻。
