# C 板待裁决事项评审简报（供外部第三方评审）

本文面向**看不到代码仓库的评审人**。它把当前并行工作板（C 板）上所有等待裁决的事项整理成自包含的说明：每一项讲清机制背景、现状与实测证据、候选方案、需要裁决的具体问题，以及申请方的立场。评审人不需要读代码就能形成判断；文末附术语表与文件锚点清单，供需要深入时定位。

## 0. 阅读指引

### 0.1 项目是什么

本项目是 Minix3 的 Rust 语义重写。Minix3 是 教学操作系统（微内核结构：内核只保留进程调度、消息传递、底层内存管理，文件系统、进程管理、虚拟内存等都以用户态服务器进程形式运行）。重写不是逐行翻译：外部行为保持与 Minix3 一致，内部用 Rust 的类型系统重新表达。支持三个处理器架构：x86-64、AArch64（64 位 ARM）、RISC-V 64 位，全部以 `#![no_std]`（不用标准库）方式构建。

### 0.2 工作板与裁决纪律

工作分三条并行线（A 板、B 板、C 板）加一条编排线。C 板是共享 QEMU 环境的工单板：所有需要在 QEMU 上实际启动内核验证的工单都从这条板过，板内记录每项的实测读数。本文所说「真机」一律指 **QEMU 实际启动并跑到用户态测试框架（ATF）全量 36 项用例**的实测形态，不是模拟器单元测试。

裁决纪律来自项目的第一原则——**抽象语义，不描述机制**，以及两条配套约束：

- **真值优先序**：Minix3 C 源码的行为是最高真值。Rust 实现若要与 C 行为偏离，必须显式标注（三处一致：文档、设计、代码，标注形如 `[ARCH: ...]`），且偏离本身需要一次明确裁决——这就是本文几个裁决点的由来。偏离不是禁止的，但必须是**知情的**。
- **契约变更纪律**：跨镜像的数据结构（引导器交给内核的信息结构 `KernelInfo` 等）是冻结契约，加字段属于契约变更，同样需要裁决。

### 0.3 怎么读

第 1、2、3 节各是一项待裁决事项，每项内部结构固定：机制背景 → 现状与证据 → 候选方案 → 裁决点 → 申请方立场。第 4 节是登记在案但**不属于裁决**的等待项，列出仅供评审人对全局优先级发表意见。评审人只需对每节的「请评审人回答」给出明确结论。

## 1. 裁决一：进程根的写执行互斥（C 板行 C11，登记号 SD-12 / PD-22）

### 1.1 机制背景

操作系统里每个进程有自己的**地址空间**，由一张页表描述「哪个虚拟地址映射到哪块物理内存、允许什么访问」。本项目里这份描述叫**进程根**（进程页表的根）。内核自己的代码（文本段）映射进每个进程的地址空间的高地址半区，这样进程陷入内核时内核代码可执行。

内存页的访问权限有三类：可读（R）、可写（W）、可执行（X）。**写执行互斥**（W^X，Write XOR Execute）是一条安全原则：一页内存要么可写、要么可执行，不能既可写又可执行——否则攻击者一旦能往内核文本段写几个字节，就能直接植入代码。现代内核（Linux、Windows）普遍执行这条原则。

Minix3 的 C 原版**没有**这个概念：内核映射是单一的、粗粒度的（大页整块可读写执行）。所以给进程根的内核文本映射去写（PD-22 三步中的第一步）是一项**超出 C 原版语义的安全强化**，从落地那天起就注定是一次 `[ARCH]` 偏离——这正是它需要裁决的深层原因，而不只是工程难度问题。

### 1.2 被否决的第一试形（案底）

第一试形的思路：在链接脚本里用标记切出内核镜像的文本段边界（可执行区与可读写区的分界），然后在**引导期建根**（bootface 引导形态构造页表的阶段）把内核镜像按 4KiB 小页逐页映射，可执行区上 RX、数据区上 RW，恒等映射与高半映射两个别名各做一遍。

三架构 QEMU 真机**全数失败**：RISC-V 与 AArch64 在 root 构建段静默挂死（定位探针显示卡在第一个 4KiB 页表映射调用内部，而此前的 2MiB 大页映射都成功了——嫌疑指向引导期页表分配器与页表遍历的状态时序，根因**未诊断**）；x86 的 UEFI 载体直接取指异常转储。该试形已全量还原，还原后 RISC-V 的 ATF 全量 36/36 复活，坐实还原干净。

这次失败的实证价值：**引导期 + 4KiB 细粒度页表操作 + 页表分配器**这个组合本身有问题，与调用方是内核还是装载器无关。

### 1.3 候选二案

**甲案：装载侧按段旗标分页映射。** 引导装载器（bootface）解析内核 ELF 文件时，按每个 PT_LOAD 段（ELF 的可加载段描述）的读写执行旗标逐页上权限，内核侧零改动。
- 失败面分析：分节动作仍发生在引导期建根的同一阶段、用同一个 4KiB 映射原语——正是被否试形探测过并失败的机制组合。换调用方不换机制。
- 走甲案的硬前置：先立专项诊断「引导期 4KiB 映射挂死根因」，探针深度超过上一轮。
- 若根因解除的收益：W^X 从内核第一条指令起就成立（强形态，验收零折让）。

**乙案（申请方推荐）：交接契约加分界字段 + 内核初始化期自分割。** 引导期维持已证明可靠的 2MiB 粗映射不动；跨镜像交接契约 `KernelInfo` 增加一个文本分界字段（文本段上界，各引导形态填写）；内核在自己的初始化期（分页已开启、单核持大内核锁的受控窗口）用两个**已在真机上大量验证过**的页表原语完成分割：`split_huge`（把 2MiB 大叶分裂成 4KiB 小页集，已在引导期 ELF 装载驱逐路径上真机使用）+ `update_flags`（改页权限位，用户态测试框架每例千次量级在烤）。内核根分割后，所有进程根经继承自动获得 RX 文本。
- 诚实边界：内核**自身**的引导根在一个初始化窗口内仍是粗粒度 RWX（进程根不受影响）。「进程根里的内核文本去写」的验收射程内成立；强形态「任何时刻不存在 RWX」要接受这一窗折让。
- 成本：`KernelInfo` 公共契约 +1 字段（契约变更，故须裁决）；缺省值 0 表示「不分割」即现行为，各引导形态按 opt-in 灰度切换，配全形态测试电池与负控。

### 1.4 裁决点与申请方立场

请裁决三点：

1. **乙案的 `KernelInfo` 加字段是否准**（契约变更）；
2. **验收形态**：PD-22 第一步「进程根的内核文本去写」接受「初始化窗后成立」与否（即是否接受 1.3 所述的内核自根一窗折让）；
3. **甲案处置**：挂起等待根因专项，还是立即立债行开始诊断。

申请方立场：三点全按乙案口径——① 准；② 接受（威胁模型针对运行期文本写入，初始化窗是固定、小、可审计的窗口；进程根这一实际射程立即全覆盖）；③ 甲案挂起但同时立债行（那个挂死根因对引导期细粒度映射是独立的知识缺口，值得诊断，只是不应阻塞乙案）。

请评审人：对三点逐一表态，并挑战上述立场（尤其第 2 点的折让是否可接受、乙案的初始化窗内是否真的没有可乘之机）。

## 2. 裁决二：用户栈的十六字节对齐（C 板行 C8，登记号 SD-21 / PD-09）

### 2.1 机制背景

三个架构的应用程序二进制接口（ABI）都对**栈指针寄存器（sp）**有对齐要求：AArch64 的 AAPCS64 要求公共接口处 sp 十六字节对齐（进程入口算公共接口）；RISC-V 的 psABI 同样要求十六字节；x86-64 的 SysV ABI 要求调用指令执行前栈指针对齐十六字节。违反对齐约定属于未定义行为——实际后果取决于编译器是否生成了依赖对齐的指令（例如 SIMD 存取指令要求对齐地址，未对齐直接触发异常）。

**进程入口**的 sp 是由内核的 exec 路径（加载并启动一个用户程序）布置的：内核把参数、环境变量、辅助向量（auxv，内核向新程序传递启动信息的一组键值对）压到用户栈顶下方，最终的 sp 就是程序的第一条指令看到的栈指针。

### 2.2 现状与证据

- Rust 侧的 C 运行时入口替身（crt0，`os/libs/minix-rt/src/crt0.rs`）：只有 x86_64 分支做了对齐修正（`and rsp, -16`）；AArch64 与 RISC-V 分支直接跳转 C 入口，不碰 sp。
- Minix3 的 C 原版（`minix3/lib/csu/arch/aarch64/crt0.S` 与 `.../riscv/crt0.S`）：同样不对齐，裸跳转。
- 逐架构对账结论（`BIRTH-CHAIN-RECON-3ARCH.md` 维度二）：Rust 与 C **逐架构一致**——这不是重写引入的分歧，而是与上游共有的 ABI 完备性问题。
- 实际影响：三架构真机全量测试一直全绿，说明现网用户程序没有触发对齐依赖的指令形状。但这不构成安全论证——换编译器版本或用户程序使用浮点/SIMD，对齐依赖可能出现。

### 2.3 一个未测的关键事实

内核 exec 路径交付的初始 sp **是否本来就十六字节对齐**，从未实测过。栈顶常量 `USER_STACK_TOP`（`os/libs/minix-types/src/types/boot.rs`）本身是页对齐的（RISC-V 为 `0x3f_ffff_f000`，其余架构为 `0x7fff_ffff_f000`），但压入参数、环境、auxv 之后的最终 sp 是否保持十六字节对齐，取决于压栈代码的对齐处理——这一步没有任何读数。

### 2.4 候选裁决与申请方立场

三个候选：

- **选项 A（维持 C 等同性 + 文档化）**：承认与上游共有，文档写明「用户程序入口 sp 对齐未保证」；论证负担重——不能只说「C 也这样」，要论证现网用户程序不依赖对齐，而这是编译器行为，论证不可闭合。
- **选项 B（`[ARCH]` 偏离：入口桩补对齐）**：AArch64 与 RISC-V 的 crt0 各加一条对齐指令（各一行汇编），x86 已有。改动极小，三处一致标注。语义上偏离 C（C 的用户程序若依赖错位 sp 会行为不同——但没有这种程序）。
- **选项 C（先实测再定）**：三架构各在用户态载荷里加一行初始 sp 读数，真机跑一次拿到对齐与否的事实，再在 A/B 之间裁。

申请方立场：**先 C 后 A/B**。若实测证明初始 sp 已对齐（exec 压栈碰巧保持了十六字节倍数），则可以「非缺口」闭合本行——入口处 sp 已满足 ABI，crt0 的对齐修正是冗余的防御；若未对齐，选项 B 的两行汇编就是明确正当的偏离。跳过实测直接裁，无论裁哪边都缺少读数支撑。

请评审人：表态是否同意先实测；若直接裁，选 A 还是 B，理由是什么。

## 3. 裁决三：A6/A9 审计遗留族的放行（C 板行 C17 余片）

### 3.1 背景与两项内容

一次全仓地址常量审计（`ADDRESS-CONSTANT-AUDIT.md`，编号 A1–A13）留下了两件与虚拟内存服务器（用户态进程 `os/servers/vm`，管理所有进程的地址空间）相关的收尾：

- **A6：mock 内核布局回退臂的拒绝断言。** 虚拟内存服务器启动时读取内核交接数据得到内核内存布局；代码里有一条「mock 布局回退臂」——交接数据版本较旧（v3/v4）不带布局真值时的回退路径。在现行 v5 交接协议下这条臂**不可达**（v5 总是带真值），但它仍是一条活代码：若未来出现「假 mock 布局被静默采用」的错配，没有任何东西拦截。要加的是一个 fail-fast 拒绝断言（不可达路径一旦真的走到就立即报错终止），纯测试语义、零生产行为改变。
- **A9 残余：用户栈顶的入口形态断言集。** 地址常量审计发现若干测试夹具硬编码了 x86 的栈顶字面量（跨架构不正确），机械修正已执行（改引架构权威常量，宿主测试全量 542/0 不变）；登记的残余是一套「入口形态断言」——断言各形态下用户栈顶的取值来源正确。

### 3.2 裁决点

这两件的代码落点都在 `os/servers/vm`——**A 板领地**。工作板纪律：跨线施工要在共享板上登记、确认对方线无在途冲突后才能动。C 板已按此登记，现在等的裁决是：**由 C 板代做这两小件**（测试语义、带登记、改完通知 A 板），还是**等 A 板自己排期**。

申请方立场：放行代做。两件都是测试语义的小增量，等待 A 板排期的机会成本大于跨线成本。

请评审人：表态放行与否；若放行，是否要求额外的隔离措施（例如单独提交、A 板事后复核）。

## 4. 登记在案但不属于本批裁决的等待项

以下各项的钥匙不在裁决，列出供评审人对全局优先级发表意见：

- **C9 服务队列**：几件 A/B 板的外来工单（方向操作数语义、随机数精度等真机读数）排队等 A/B 板发请求。码在 A/B 板改，C 板只负责跑读数回传。等 A/B 板确认这些条目是否仍然有效。
- **C17 的 xtask 构建半**：SD-3 D1（AArch64 UEFI 引导的设备树源注入）的 shim 半已真机闭环，剩构建编排工具（xtask）侧的正式注入——xtask 是 A 板领地，等 A 板让路。与第 3 节的 A6/A9 同属跨线协调。
- **C20 哨兵字段去留**：`KernelInfo` 的 `bootstrap_start/bootstrap_len` 两个字段在三架构生产路径上恒为零（为对称 C 原版结构形状而留），临时裁决「保留 + 守卫」。按公理「等第二个真实使用者出现再裁」搁置。近期落地的 U-Boot 引导链不改变前提（新接收镜像同样消费零值），维持现状。
- **C2E-riscv 窄钳的 SSWI 钥匙**：RISC-V 的调度中断目前被钳制在单核（上一轮实测定谳：本机固件不把常规软件中断委派给内核态，置位成功但永不唤醒）。待裁决之外的候选通道：QEMU 的 `-machine aclint=on` 会带 aclint-sswi 设备（内核态中断的另一条硬件通道，与已定谳的路径不同），可用小探针验证 QEMU 与固件行为——探通则撤钳解锁多核调度中断。

## 5. 术语表

| 术语 | 含义 |
|---|---|
| 编排线 / A 板 / B 板 / C 板 | 四条并行工作线。C 板=共享 QEMU 环境的工单板；A 板含虚拟内存服务器与构建编排（xtask）；编排线=跨线裁决者 |
| 真机 / ATF | 本项目语境下指 QEMU 实际启动内核并跑用户态测试框架（ATF）全量 36 项用例；「36/36」即全绿读数 |
| mock | 宿主机（非 QEMU）单元测试形态，用测试替身代替架构层 |
| 进程根 | 一个进程的页表根，描述该进程的完整虚拟地址空间 |
| RX / RW / RWX | 内存页权限：可读+可执行 / 可读+可写 / 全开。W^X 原则禁止 RWX |
| BKL（大内核锁） | 本项目内核的全局大锁，同一时刻只有一个核在临界区 |
| bootface / boot-shim | 两种引导形态：自引导面（内核镜像自己当引导体）/ 独立引导垫片（独立小可执行体为内核屏蔽固件差异） |
| `KernelInfo` | 引导器交给内核的启动信息结构，跨镜像冻结契约 |
| `map_huge` / `split_huge` / `update_flags` | 页表原语：映射大页 / 把大页叶分裂为小页集 / 改页权限位 |
| PT_LOAD | ELF 可执行文件中「可加载段」的描述项，带读写执行旗标 |
| auxv | 内核压到新程序用户栈上的辅助向量（启动信息键值对） |
| crt0 | C 运行时入口替身：进程第一条指令到 main 之间的小桩 |
| UEFI / OpenSBI / U-Boot / AAVMF | 固件形态：x86 与 AArch64 的 UEFI（AAVMF 是 AArch64 的 UEFI 固件实现）/ RISC-V 的内核态运行时 / 通用引导器 |
| xtask | 构建编排工具（A 板领地），负责产出各架构内核镜像 |
| `[ARCH: ...]` | 三处一致的偏离标注（文档、设计、代码），标记与 Minix3 C 行为的有意偏离 |

## 6. 文件锚点清单

| 断言 | 锚点 |
|---|---|
| 被否试形的实测记录与还原读数 | `rewrite-notes/coordination/PARALLEL-20261018-C-KERN-QEMU.md` 行 C11 与台账 SD-12 行 |
| 甲乙两案的裁决申请原文 | 同文件附录一 |
| x86 crt0 对齐、其余架构裸跳 | `os/libs/minix-rt/src/crt0.rs`（x86 臂 `and rsp,-16`） |
| C 原版入口桩裸跳 | `minix3/lib/csu/arch/aarch64/crt0.S`、`minix3/lib/csu/arch/riscv/crt0.S` |
| 用户栈顶常量 | `os/libs/minix-types/src/types/boot.rs` 的 `USER_STACK_TOP` |
| split_huge 真机使用 | `os/arch/src/riscv64/paging.rs` 的 `split_huge`（引导期 ELF 装载驱逐路径） |
| A6/A9 审计判定 | `rewrite-notes/coordination/TODO-LEDGER-OPEN.md` 地址常量审计行；A6 跨线登记见 C 板 §6 |
| W^X 登记出处 | `rewrite-notes/coordination/STRUCTURAL-DEBT-REGISTER-20261008.md` SD-12 行 |
| bootstrap 哨兵字段现状 | `os/libs/minix-boot/src/kernel_info.rs` 的 `KernelInfo`；消费守卫 `os/kernel/src/lib.rs` 的 reclaim guard |

## 7. 第三方评审裁决回填

外部评审对本简报三项裁决给出了结论（要点）：

- **C11**：乙案方向支持，甲案挂起但**单独立技术债**（记录原始故障现象、回归基线与未查明根因，不与乙案绑定）；暂不批准 `KernelInfo` 加字段——先核实内核能否从自身链接布局取得文本边界；初始化窗 RWX 有条件接受，条件是 ① 窗内无用户态执行、② 无其他活动核使用该根（或有可靠跨核同步）、③ 无中断/抢占路径破坏假设、④ 分割完成前不执行依赖该强化的阶段、⑤ 分割后所有新建进程根文本不可写。并指出三个未闭合的正确性问题：**页表子树共享还是逐根复制、TLB 同步、可写别名枚举**，要求先补证再实施。
- **C8**：先实测正确，但 ABI 要求本身已明确——「与 C 相同」不构成继续违反的理由；实测探针必须取自**第一条入口指令**（无函数序言）；若不对齐，修复位置在**入口栈构造处**（内核 exec 契约层），不能在各 crt0 各自补丁；修布局时不能盲目在压栈完成后下移对齐（会破坏指针基址），应在布局计算期预留对齐。
- **C17**：放行代做，约束=开工前 A 板确认无在途冲突、A6 与 A9 **分开提交**、宿主测试 + QEMU 验收、A 板复核语义、C 板回填读数。A6 倾向**消除静默回退**（协议输入验证拒绝缺布局的输入）而非末端断言；若保留回退臂则 fail-fast 不得只 debug 生效。A9 必须是**来源与契约测试**（每个形态选对架构权威常量、无跨架构字面量漂移），不是常量数值测试。
- **优先级**：P0 = C8 取证 + C11 页表正确性核实；P1 = C17 代做 + C2E-riscv 的 ACLINT SSWI 探针（QEMU `aclint=on` 走 TCG 路径创建 `riscv,aclint-sswi` 节点，与已定谳失败的 SSI 委托路径不同；探针只验证最短路径，不推广到 KVM/真板）；P2 = C9、C17-xtask 等外部；C20 维持；**D-73 是独立引导链基础设施**，不被 C20 吸收、不与上述契约修改捆绑。

评审同时确认本简报已足够第三方理解，无需重新调查全量，只需按其 §五任务单补足 C11 页表行为与 C8 入口 sp 事实——即下一节的取证。

## 8. 补充取证与诊断结果（按评审 §五 A/B/C 任务单，只调查不改码）

每条标注【已证实】/【反证】/【尚无证据】。

本节全部结论可用以下命令复算（仓库根执行）：

```bash
# A1 进程根构造器与调用点（逐根重建，非共享子树）
grep -n "pub fn map_kernel" os/arch/src/arch/paging.rs
grep -rn "map_kernel(" os/servers/vm/src/vmproc/vmproc_handle.rs | head -2
# A1b 文本边界通道：v5 字段 + 内核占位值
grep -n "kern_text_pages\|kern_data_pages" os/libs/minix-types/src/types/boot.rs | head -4
grep -n "kern_text_pages\|kern_data_pages" os/kernel/src/vm_handoff.rs
# A2 原语三架构在位 + 写路径本地 TLB 失效
grep -rn "fn split_huge\|fn update_flags" os/arch/src/{x86_64,arm64,riscv64}/paging.rs
grep -n "sfence.vma\|tlbi\|invlpg" os/arch/src/riscv64/paging.rs os/arch/src/arm64/paging.rs | head -6
# A3 旗标词表：RX 已存在、DM 无 X、恒等窗 RWX
sed -n '110,155p' os/arch/src/arch/paging.rs
# A4 AP 释放点（分割窗放在它之前）
grep -n "smp::smp_init()" os/kernel/src/lib.rs
# A5 链接脚本：无文本符号、节 4K 对齐
grep -n "PROVIDE\|__text\|ALIGN(4K)" os/kernel-image/riscv64.ld | head -6
# B1-B3 帧构造器八字节圆整与 vsp 推导
sed -n '106,132p' os/libs/minix-sys/src/stack.rs
grep -n "div_ceil(SLOT)" os/commands/bin/shell/src/exec_frame.rs
# B5 参数经 ps_strings 寄存器（sp 仅调用栈）
sed -n '386,425p' os/libs/minix-rt/src/crt0.rs
# C1 A6 回退臂分叉现状（riscv panic / 其余静默 mock）
sed -n '655,692p' os/servers/vm/src/vm_server.rs
```

### 8.1 C11：乙案的页表正确性闭环

**A1｜进程根的内核高半是逐根重建，不是共享子树【已证实】。**
进程根的内核映射由 `map_kernel` 在每个进程构造时逐页现建（`os/arch/src/arch/paging.rs` 的 `map_kernel`，调用点 `os/servers/vm/src/vmproc/vmproc_handle.rs` 的 `init_page_table`；C 对位 `pt_mapkernel()`）。修改内核初始化根的 PTE **不会**传播到任何进程根——也不需要传播：RX 落点在构造器本身（`map_kernel` 的文本段旗标切换）。简报原文「继承自动获得」表述**有误**，评审的质疑成立；正确表述是「进程根在构造时从 `map_kernel` 获得内核映射，文本 RX 随构造器旗标改变生效，新建根即时可见」。普通进程根、VM 自根、测试根（`#[cfg(test)]` 走 `SimPaging`）同走此构造器；直核与 BootShim 各引导形态只影响引导根，不影响进程根构造路径。

**A1b｜文本边界的通道已经存在，无需新契约【已证实】。**
跨镜像交接 `VmBootHandoff` v5 起就携带 `kern_text_pages`/`kern_data_pages` 两个字段（`os/libs/minix-types/src/types/boot.rs`），并经 `KernelLayout::new_with_ident` 转成 VM 全局 `kernel_layout`（`os/servers/vm/src/global.rs` 的 `set_kernel_layout`，`VmServer::init` 时写入）——`map_kernel` 的文本页数入参就来自它。缺口只在**数值侧**：内核生产交接时填的是占位值「整段当文本」（`os/kernel/src/vm_handoff.rs`：`kern_text_pages = kern_size/4096, kern_data_pages = 0`），真实的文本/数据分界从未产生。

**A5｜linker 符号：现有脚本无文本边界符号，但补两行即得【已证实】。**
三份链接脚本（`os/kernel-image/{x86_64,aarch64,riscv64}.ld`）导出 `__bss_start/__bss_end`，**没有** `__text_start/__text_end`；但节布局逐节 `ALIGN(4K)`（`.rodata` 等均页对齐），文本→只读数据边界**按构造即页对齐**。在 `.text` 节末补 `PROVIDE(__text_end = .;)`（每脚本一行）即导出边界——这是构建层两行改动，**不是**线协议变更。

**A2｜分割原语：三架构在位，本地 TLB 失效内建在写路径【已证实】。**
`split_huge` 与 `update_flags` 三架构加 mock 全部在位（x86 `arch/src/x86_64/paging.rs:852/909`、arm64 `:829/913`、riscv `:890/983`）。PTE 写经 `write_pte_dm`，KernelDm 通道**写后即刻本地 TLB 失效**（riscv：`sfence.vma vaddr`，`arch/src/riscv64/paging.rs:256-275`；arm64：`dsb sy; tlbi vae1is` 广播变体，`split_huge` 明文要求 EL1 KernelDm 句柄；x86：逐地址 invlpg 形状）。`update_flags` 本体不额外刷（写路径已带）。**跨核广播缺失【已证实】**——但见 A4，放置正确则不需要。arm64 的 `split_huge` 附带运行条件：只在 KernelDm 通道可用（VmDm 通道 EL1 指令会 fault），内核初始化期天然满足。

**A3｜同一文本页的全部映射与承诺口径【已证实，承诺必须限定为映射级】。**
进程根内文本页的映射清单：① 高半文本映射——现旗标 `kernel_executable_writable()`（RWX+G），目标改为 `kernel_executable()`（RX+G，**该旗标已存在**于旗标词表，注释明言「W^X principle」，只是 `map_kernel` 未用它）；② Direct Map——`kernel_read_write()`（RW+G，**无 X 位**），覆盖全部物理内存，是文本页的**可写不可执行别名**；物理页级 W^X（无任何可写别名）在 DM 覆盖全部 RAM 的内核形态下**按设计不可达**，除非放弃 DM 这个基础机制；③ 恒等窗口——RWX（`[ARCH]` 已登记：UEFI 快照无 W^X 意图，收窄需装载器节级来源），仅 x86-UEFI 腿非空，是文本页的**可写可执行别名**，把 x86 的可承诺上限钉死在映射级；④ 引导根——粗 RWX 直到分割。承诺口径因此精确为：**映射级 W^X**（高半文本映射不可写）；DM 可写别名按设计存在（不可执行）；x86-UEFI 的恒等窗维持既有 `[ARCH]` 登记。

**A4｜初始化窗：放置在 `smp_init` 之前即可满足全部五条件【已证实】。**
kmain 流水：阶段 A/B/C/D → `init_smp_state` → 取 BKL → **`smp_init`（AP 由此释放进调度环，`os/kernel/src/lib.rs:1525`）** → `bsp_finish_booting` → 首个用户进程。分割放在取 BKL 之后、`smp_init` 之前：单核在跑、无用户态、AP 尚未启动（评审条件 ①②③ 满足）；先于任何用户服务（④⑤ 满足）。AP 的 TLB 条目在其启动后首次取指时新鲜建立（RX），无陈旧 RWX 残留，跨核 shootdown 不需要。

**C11 结论（回答评审四问之一、之二）**：**乙案可进入实施**，形态为「乙案′」——不新增 `KernelInfo` 字段（评审预判成立）；边界来源 = 链接脚本补 `__text_end` 符号；通道 = **既有** v5 字段填真值；内核自身引导根分割 = `split_huge` + `update_flags(RX)` 放 `smp_init` 前；进程根 = `map_kernel` 文本页旗标切 `kernel_executable()`。承诺口径 = 映射级 W^X（DM 可写别名与 x86 恒等窗如实声明）。甲案按裁决立独立技术债行。

### 8.2 C8：初始用户 sp 的 ABI 对齐

**B1-B2｜栈构造与最终 sp 写入点【已证实】。**
进程初始栈帧由两个构造器生成：引导进程走 `minix_sys::stack::stack_params/stack_fill`（`libs/minix-sys/src/stack.rs`，VM 的 `install_boot_stack` 调用，`os/servers/vm/src/vm_server.rs:1032`）；execve 客户端（shell、init 先例）走 `commands/bin/shell/src/exec_frame.rs` 的同构对。内核 exec 路径（`dispatch_exec`）不自算布局——收 VM/调用方建好的帧。初始 sp = `vsp = user_sp − frame_size`，经 `build_cpu_context` 写入进程上下文。

**B3-B4｜对齐性质：违反可静态证明【已证实——不是尚无证据】。**
两个构造器的圆整都是**八字节**（`stack_params`：`(size + WORD - 1) & !(WORD - 1)`；shell 版：`div_ceil(SLOT) * SLOT`，`SLOT=8`）。`user_sp` 来自页对齐的 `USER_STACK_TOP`。因此 `vsp` 的十六对齐当且仅当 `frame_size ≡ 0 (mod 16)`，而 `frame_size` 随 argv/env 串长逐进程变化——**奇串长参数的进程初始 sp 必然只八字节对齐**。评审要求的入口探针（无序言汇编取首条指令的 sp）仍值得跑一次拿真机读数，但「违约是否存在」已可由构造静态回答：存在。

**B5｜四个 sp 点的实际语义【已证实，与评审假设有一处出入】。**
本项目的参数传递约定与主流不同：出生链经 **ps_strings 寄存器**取参数（x86 `rbx→rdi`、riscv `a0`、aarch64 `x0`，`libs/minix-rt/src/crt0.rs` 三臂），**不经 sp 读 argc/argv**。因此：exec 交付 sp = vsp；crt0 首指令 sp = vsp（riscv/aarch64 裸 `call`/`bl` 原样传递）；x86 crt0 `and rsp,-16` 下移至多八字节——**安全**，因为 sp 只作调用栈用、帧内容全经 ps_strings 寻址；「C main 处 sp」= 各臂 birth 链自身。评审担心的「对齐破坏帧指针」在本约定下不存在——修复无需动帧内布局。

**B6｜修复点与形状【已证实可行】。**
按评审原则（修在契约层、不在 crt0 各自打补丁），落点是**两个帧构造器的圆整 8→16**（各一行：`stack_params` 的圆整掩码由 7 改 15、shell 版 `div_ceil(SLOT)` 改 `div_ceil(16)`）。效果：`vsp` 恒十六对齐；帧内所有指针以 vsp 为基自洽；ps_strings 约定不变；x86 的 `and rsp,-16` 变为无操作（保留为防御）。对合规程序零行为变化（帧内容逐字节不变，仅 vsp 低移 0-8 字节）；与 C 的八字节圆整属不可观察的实现差异，标 `[ARCH]` 即可。crt0 三臂不动。

**C8 结论（评审四问之三）**：**实际 ABI 违约存在**（静态可证，aarch64/riscv64 奇串长帧必然八字节对齐交付）；修复 = 两构造器圆整改十六字节，属入口契约层修正；真机探针按评审要求补一个无序言读数作确认证据（低成本）。

### 8.3 C17：A6/A9 的最小变更边界

**C1｜A6 可达性现状【已证实】。**
回退臂在 `os/servers/vm/src/vm_server.rs`（`VmServer::init` 的 `kernel_layout.unwrap_or_else`）：交接版本低于 v5 时 `None` → 回退 mock 常量。现状分叉：**riscv64 生产路径已经 panic**（fail-fast 在位，注释明言「真交接必为 v5」）；**x86_64/aarch64 生产路径仍静默采用 mock x86 形状**（仅 `audit_log!` 打一行），aarch64 上那是非规范形状——A6 的实质残余就是这两个臂。宿主测试经 `BootParams::simple` 依赖该 mock 臂（删除会破宿主测试）。

**C2｜三案比较与最小变更【已证实可行】。**
评审倾向「协议输入验证拒绝」优于「末端断言」。结合现状，最小且不动宿主测试的形状：回退臂收窄为**测试专用**（`#[cfg(test)]` 保留 mock 供 `BootParams::simple` 宿主用例，语义=显式注入的测试夹具），生产配置（`#[cfg(not(test))]`）把 panic 从 riscv64 扩到**三架构**——与 riscv 既有形状同构，删除静默路径。不使用 debug-only 断言（评审红线）。

**C3｜A9 的断言范围【已证实可行】。**
最小范围两项来源测试：① VM 侧断言启动路径 `ctx.user_sp` 恒等于架构权威 `minix_types::USER_STACK_TOP`（不出现任何架构字面量）；② 帧构造器侧（与 C8 修复同笔）断言 `vsp` 恒十六对齐（把 C8 的契约写成测试）。跨架构字面量漂移由既有机械换引（已完成的 A9 主体）+ 这两项来源断言共同封死。

**C17 结论（评审四问之四）**：**可以由 C 板代做**，按评审约束表执行（A 板确认无在途 → A6、A9 分项提交 → 宿主测试 + QEMU 验收 → A 板复核 → C 板回填）。A9 的对齐断言与 C8 修复同笔最省；A6 独立成笔。

## 9. 第一轮取证后的结论（部分被第二轮评审与 §12 修订）

| 评审问题 | 结论 | 依据 |
|---|---|---|
| C11 乙案能否进入实施 | **能**，形态为乙案′：无契约变更，边界走链接符号 + 既有 v5 字段，分割点前置 `smp_init`，承诺口径钉死在映射级 W^X | §8.1 A1/A1b/A2/A4/A5 |
| `KernelInfo` 是否加字段 | **不加**（评审裁决维持）；真实通道是 `VmBootHandoff` v5 已有字段填真值 + 链接脚本补一行 `__text_end` | §8.1 A1b/A5 |
| C8 是否存在实际违约 | **存在**（aarch64/riscv64 奇串长帧交付八字节对齐 sp，静态可证）；修复 = 两帧构造器圆整 8→16（契约层），探针补真机读数 | §8.2 B1-B6 |
| C17 是否可代做 | **可**，按评审约束表；A6 = 生产三架构 fail-fast + 测试显式夹具；A9 = 来源断言 + vsp 对齐断言 | §8.3 C1-C3 |

推进顺序采纳评审建议：P0 = C8 修复 + 探针、C11 乙案′实施（核实已由本节完成，直接解锁）；P1 = C17 代做、SSWI 探针；P2 = C9、C17-xtask；C20 维持；D-73 独立推进不与本批契约修改捆绑。

## 10. 第二轮评审裁决回填

第二轮评审认可三项方向收敛（乙案′无需契约字段、C8 修复层次在帧构造层、A6/A9 代做收敛），但要求实施前补五件局部证据，并给出裁决：

- **C11：方向放行**，三个实施前精确问题——① `__text_end` 是否足以无歧义计算 `kern_text_pages`（可执行区是否为镜像起点起的连续前缀、地址空间是否一致）；② 分割时点的 AP 真实生命周期（"未进调度环"不等于"未运行任何内核代码"）；③ W^X 承诺口径须写明恒等映射的所属根与生命周期（恒等窗若在进程根内，"进程根文本 W^X"的说法就不成立，只能说"进程根的高半文本映射为 RX"）。
- **C8：违约结论支持**，修复层次在帧构造层；但两个算术前提须补：① 完整帧大小的模 16 推导 + 一个最小违约输入（"奇串长"不能直接推出整帧 8 mod 16）；② 确认 8→16 圆整是**总帧级**还是**逐对象级**——逐对象会改变帧内偏移，"各一行修改"在证明是总帧级之前不算最小等价方案。指针槽保持八字节。首指令探针保留为实现后确认。
- **C17：有条件放行**——A6 不要用 `cfg(test)` 回退臂充当显式夹具（`cfg(test)` 只覆盖本 crate 单元测试，集成测试按普通依赖编译；且测试应显式注入 mock 布局，缺失布局应表示输入不完整并被拒绝）；A9 必须先澄清 `ctx.user_sp` 语义（帧构造前栈顶 vs 构造后入口 sp 是两个量，`entry_sp = user_sp − frame_size`），来源断言与对齐断言分开。
- 推进顺序：P0 = C8（先推导后改码）+ C11（补齐 linker 边界与 AP 证据、明确承诺范围）；P1 = C17 收紧后代做、SSWI 探针；P2 = C9/xtask；C20 维持；D-73 独立。

## 11. 第二轮补证与诊断（五件，只调查不改码）

复算命令（仓库根）：

```bash
# 11.1 三脚本同形与节序
grep -n "KEEP(\*(.text.boot))\|^    \.\|ALIGN(4K)\|ALIGN(0x200000)" os/kernel-image/riscv64.ld | head -8
# 11.1 实际 ELF：首个 PT_LOAD 为 R E 且起于镜像基址（入口=基址）
readelf -lW os/target/riscv64gc-unknown-none-elf/release/kernel | grep -E "LOAD|Entry"
# 11.2 三架构释放机制都在 smp_init 内
grep -n "INIT/SIPI\|hart_start\|psci" os/kernel/src/smp.rs | head -6
# 11.3 恒等窗候选只有 MMIO 表（T2 后镜像低址窗退役）
sed -n '470,480p' os/kernel/src/vm_handoff.rs
grep -n "IO-APIC\|PLIC\|empty" os/arch/src/lib.rs | head -5
# 11.3 引导根恒等映射范围与旗标
sed -n '858,862p' os/kernel/src/lib.rs
grep -n "while addr < IDENTITY_MAP_END" os/kernel/src/lib.rs
# 11.4 基准常量与圆整
sed -n '44,58p' os/libs/minix-sys/src/stack.rs
grep -n "div_ceil(SLOT)" os/commands/bin/shell/src/exec_frame.rs os/commands/sbin/init/src/execve.rs
# 11.6 BootParams::simple 全调用点与 ctx.user_sp 语义
grep -rn "BootParams::simple" os/servers/vm/src/ | head -6
grep -n "user_sp" os/servers/vm/src/vm_server.rs | sed -n '3,7p'
```

### 11.1 C11 问题一：`__text_end` 单符号足够【已证实】

三份链接脚本**完全同形**（`os/kernel-image/{x86_64,aarch64,riscv64}.ld`）：`.text` 是**首个输出节**，起于镜像基址（`KERNEL_VIRT_BASE` / `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`），节内 `KEEP(*(.text.boot))` 居首（入口 `_start` 就在镜像第一字节）；其后依次 `.rodata`/`.data`/`.bss`，逐节 `ALIGN(4K)`，**没有任何后置可执行输出节**（`/DISCARD/` 清掉 `.eh_frame` 等）。实际 ELF 复核（riscv64 接收镜像）：首个 `PT_LOAD` 即 `R E` 旗标、VMA 起于镜像基址、入口 `0xffffffc000000000` = 基址。

由此：**可执行区 = [镜像基址， `__text_end`) = 镜像起点的连续前缀**，评审要求的四个条件逐一成立——① `.text` 起点就是计算 `kern_text_pages` 所用的镜像起点（`kern_virt_base` = ELF min vaddr = 基址）；② 镜像起点之前无内容（入口居首），`.text` 之后无可执行节；③ 三脚本同形，三架构 ELF 同构（aarch64 脚本头部明言"布局契约与 aarch64.ld 完全同形"，x86 同构）；④ 地址空间一致——`__text_end` 是链接 VMA 符号，`kern_text_pages = (__text_end − kern_virt_base) / 4096` 全程在 VMA 空间计算，物理侧经既有的"全段 `vaddr − paddr` 一致"不变量（check-layout L4）映射，不混淆三种地址。

**结论：一个 `__text_end` 足够，无需符号体系扩充。**

### 11.2 C11 问题二：AP 生命周期——释放前从未执行内核代码【已证实】

三架构的 AP 释放机制**全部位于 `smp_init` 函数体内**（`os/kernel/src/smp.rs:1088`）：x86 = 自举记录 + INIT-SIPI 序列（`:1113` 起）；riscv64 = SBI HSM `hart_start`（`:1374` 起的函数注释明言"wake secondary cores through SBI HSM hart_start"，AP 桩读记录后 **"installs the BSP root"** 再进收敛点）；aarch64 = PSCI `cpu_on`（`:1260` 选 conduit、`arch/src/arm64/smp.rs:448` 执行）。

释放前 AP 的执行状态按架构：riscv64 的次级 hart 停在 OpenSBI HSM 的 stopped 态（只执行固件代码）；aarch64 停在 PSCI 固件 hold 态；x86 停在等待 SIPI 的停机态（真机电源模型）。riscv 直核入口另有一条多 hart 齐入的防御选举臂（`kernel-image/src/main.rs` 的 `park: wfi` 循环）——落选 hart 在**开分页之前**停车（`satp` 未装载、未清 `.bss`、未立栈），同样不持有内核根 TLB 条目；门读数（test-smp-aps-riscv64 的 ap-arrived=1）表明生产链无落选者，该臂纯为兜底。三者共同点：**AP 在 `smp_init` 释放调用之前从未执行过一条开启分页的内核指令、从未装载过内核根，因此不可能持有内核根的任何 TLB 条目**。评审要求的证据不是"没进调度环"，而是"没碰过内核根"——成立，且比前者强：AP 的第一次内核取指发生在分割之后，装载的即已分割的根（riscv 注释的 "installs the BSP root" 直陈此点），TLB 条目全部新鲜。跨核 shootdown 由此不需要，BKL 论证不进入该证明。

### 11.3 C11 问题三：恒等窗归属与精确承诺条款【含一处对 §8.1 的修正】

**【修正】** §8.1 A3 说"x86-UEFI 恒等窗是文本页的 RWX 别名"——依据是 `map_kernel` 第四段的注释（"the kernel's live code/GDT/IDT/TSS sit at the firmware-relocated low addresses"）。深查生产者后该注释是 **T2 之前内联执行模型的陈旧文字**：`build_identity_windows`（`os/kernel/src/vm_handoff.rs:481`）的候选**只有** `kernel_identity_mmio_regions()`，其文档明言 T2 拆分后固件 `reserved_regions` 输入已退役、"no low-address code/data window needs replaying"。

各架构恒等窗实况：x86_64 = IO-APIC + local-APIC 两页；riscv64 = QEMU virt PLIC 2 MiB 窗；aarch64 = 空。**全部是 MMIO 区间，无一与内核文本页物理重叠**。因此修正后的映射清单：

- **进程根**：高半文本映射（分割后 RX）+ Direct Map（RW、无 X）+ 恒等窗（仅 MMIO，非文本别名）→ **进程根射程内不存在文本页的 RWX 别名**——这比 §8 的"映射级"口径更强。
- **引导根**（内核自根，全部引导形态）：低 4 GiB 恒等映射，`id_flags = kernel_read_write() | EXECUTABLE`（RWX 粗 2 MiB 叶，`os/kernel/src/lib.rs:858-861` 起，`while addr < IDENTITY_MAP_END`），覆盖内核镜像物理装载区；**生命周期 = 引导根全程**（设计明言活动根永不重建）。文本页在引导根上的 RWX 别名由本方案**维持现状**（高半别名变 RX，恒等别名不动）。

精确验收条款（替代"映射级 W^X"的笼统说法）：

> 进程根中的内核高半文本映射为 RX；Direct Map 对应别名为 RW/NX；进程根的恒等窗仅覆盖 MMIO 区间，不构成文本页别名；引导根的低 4 GiB 恒等映射（RWX）在引导根生命周期内维持现状。本方案不承诺物理页级 W^X，也不宣称引导根射程内不存在 RWX。

连带一项文档债：`map_kernel` 第四段的陈旧注释应随乙案′实施批改为现口径（T2 后恒等窗=仅 MMIO）。

### 11.4 C8 问题一：完整模 16 推导与最小反例【已证实——评审质疑成立且更强】

`minix_sys::stack_params` 的完整公式（`os/libs/minix-sys/src/stack.rs`）：

```
frame_size = align8( STACK_MIN_SZ + Σ_argv(len_i + 9) + Σ_env(len_j + 9) )
STACK_MIN_SZ = 8(argc) + 16(两个 NULL 槽) + 320(auxv 预算 16B×20)
             + 1024(可执行名预算) + 32(PsStrings) = 1400
1400 mod 16 = 8   ← 基准常量本身就偏 8
vsp = USER_STACK_TOP − frame_size      （USER_STACK_TOP 页对齐 → ≡ 0 mod 16）
```

**最小反例（现实输入，非构造）**：`install_boot_stack` 的实参恒为 `argv=[进程名], envp=[]`。取 `argv=["rs"]`：size = 1400 + (2+9) = 1411 → align8 → **frame_size = 1416 ≡ 8 (mod 16)** → vsp ≡ 8 (mod 16)，八字节错位。更强的事实：十二个引导进程名（rs/ds/pm/sched/vfs/memory/tty/mib/vm/pfs/mfs/init，长度 2–6）逐一代入，`1409+len ∈ [1411,1415]` 经 align8 全部落在 **1416**——**现行全部出生进程的入口 sp 都是八字节错位**，违约不是边缘输入而是缺省路径。shell/init 版（`STACK_MIN_BYTES = 56 ≡ 8 (mod 16)`）同病。评审"奇串长不能直接推出"的质疑成立：错位的真正根因是**基准常量 ≡ 8 (mod 16)**，与串长无关。

### 11.5 C8 问题二：圆整位置——总帧级，"各一行"成立【已证实，修正构造器数量】

三处构造器（**不是两处**：`minix_sys::stack`、`commands/bin/shell/src/exec_frame.rs`、`commands/sbin/init/src/execve.rs`）的圆整全部是**求和后对总帧一次性圆整**（`(size+7)&!7` 或 `total.div_ceil(SLOT)*SLOT`），不存在逐对象圆整——评审担心的"多对象各自多 padding"不存在。`stack_fill` 的内部偏移（`min_strings_off` 基准 + 逐串 `len+1` 推进 + `ps_off = round8`）**全部独立于 frame_size**；frame_size 只进入三个上界守卫。因此把总圆整 8→16：指针槽保持八字节（评审红线满足）、帧内容逐字节不变、内部指针以 vsp 为基自洽、vsp 整体下移 0–8 字节。**最小等价成立，修复 = 三个构造器各一行总圆整改动**。

### 11.6 C17 两件

**BootParams::simple 全调用点【已证实】**：`vm_server.rs:222`（`new()`，**已 `#[cfg(test)]` 门控**，文档明言防生产使用）、`:245`（`new_for_test`，同门控）、`boot.rs:497/504/513/522`（`#[cfg(test)]` 单元测试）；无 `tests/` 集成目录；全仓无其它 crate 调用。评审担心的"集成测试经普通依赖编译、`cfg(test)` 不生效"**当前不成立**（零集成调用点）——但按评审偏好仍收窄：`BootParams::simple` 改为**显式携带 mock `KernelLayout`**（夹具语义：用简单构造器 = 明确要 mock 布局），生产 `init` 的 `unwrap_or_else` 换成三架构统一拒绝（riscv 既有 panic 泛化），并补"None 布局输入被拒"负控测试——缺失布局从此只表示输入不完整，测试不掩盖协议错误。

**ctx.user_sp 语义【已证实】**：`VmContext.user_sp` ← `params.user_sp` ← 交接 `kinfo.user_sp`（C 对位 `pre_init.c:156` 的 `USR_STACKTOP`）——是**帧构造前的栈顶输入**，不是入口 sp；入口 sp = `vsp` 经 `sys_exec` 交付内核 `build_cpu_context`。A9 的两条断言因此分立且各自语义正确：① 来源断言（`ctx.user_sp` 恒等于架构权威 `USER_STACK_TOP`）落在栈顶输入语义上，正确；② 入口 sp 对齐断言（`entry_sp == vsp` 且 16 对齐）属 C8 修复的验证面，与 A9 分开记账。

## 12. 修订后的最终结论（取代 §9）

| 评审问题 | 修订结论 | 依据 |
|---|---|---|
| C11 乙案′能否实施 | **能**：三个实施前问题全部闭合——`__text_end` 单符号充分（可执行区=基址起连续前缀、VMA 空间一致）；AP 释放前从未执行内核代码（三架构释放机制全在 `smp_init` 内，TLB 论证不依赖 BKL）；承诺条款按 §11.3 精确文字（进程根射程无文本 RWX 别名，引导根恒等 RWX 维持现状） | §11.1-11.3 |
| `KernelInfo` 是否加字段 | **不加**（两轮一致）；`__text_end` 一行/脚本 + 既有 v5 字段填真值 | §11.1、§8.1 A1b |
| C8 违约与修复形状 | **违约存在且是缺省路径**（基准常量 1400 ≡ 8 mod 16，全部十二个出生进程入口 sp 八字节错位）；修复 = **三个**帧构造器的总帧圆整 8→16（总帧级已证实，指针槽不动、帧内偏移不变）；无序言探针为实现后确认读数 | §11.4-11.5 |
| C17 可否代做 | **可**，条件收紧为：A6 = `BootParams::simple` 显式携带 mock 布局 + 生产三架构统一拒绝 + None 负控；A9 = 两断言分立（来源归 A9、对齐归 C8）；分开提交、A 板复核 | §11.6 |

推进顺序维持评审建议：P0 = C8（推导已闭合，直接改码 + 探针）、C11 乙案′实施（承诺条款按 §11.3）；P1 = C17 代做（按 §11.6 边界）、SSWI 探针；P2 = C9/xtask；C20 维持；D-73 独立。连带文档债两项随批：`map_kernel` 第四段陈旧注释（§11.3）、本轮新增的链接符号与圆整改动落码时同步更新本简报的锚点行号。

**C8 落地注记（C8ALIGN 批）**：修复实际覆盖**四个**交付点——§11.5 所列三构造器之外，入口探针在真机上抓获第四点：`arch/src/arch/boot.rs::load_elf_into` 的内核出生帧（`sp = stack_high−56`，8 mod 16，全架构；其历史注释显示先前一轮只修到八字节对齐）。修复 = 该处 sp 16 对齐下取（`ps_strings == sp+24` 关系保持）。探针形态：minix-rt 三 naked 臂首指令 stash（x86 记录 `and rsp,-16` 之前的原值）+ `entry_sp()` + rt-birth 载体 DIAGCTL 行 `rt-birth entry_sp=0x…`，三条 `test-rt-birth*` 门内嵌 16 对齐断言。门读数：minix-sys 342/0（含 `test_frame_size_sixteen_byte_aligned` 反例钉）、shell 61/0、init 149/0、arch runtime-window 287/0（两枚布局钉随新契约更新为 −64/−40）、mock 850/0/8、三架构 rt-birth PASS、riscv ATF 36/36。
