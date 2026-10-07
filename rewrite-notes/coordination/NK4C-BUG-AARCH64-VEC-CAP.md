# NK4-C 缺陷分析交接件：aarch64 子进程把 INIT 栈地址当 Vec 容量致 ~4GiB 分配崩溃

> 本文件是**交给另一个工具/模型做独立静态分析**的自包含交接件。
> 阅读者被假设为：没有本项目其它上下文、只能读到本文件 + 仓库源码。
> 任务性质：**只做静态分析与跨架构代码对照，产出候选假设，禁止下结论、禁止改代码。**

---

## 0. 你要做什么（一句话）

在**不运行任何东西**的前提下，通过对比 aarch64 与 x86_64 两条启动/执行路径的代码，
找出：**为什么一个初始化栈地址会被某段 Rust 当作 `Vec` 的目标容量/长度读取**，
且这个读取**只在 aarch64 发生、x86_64 不发生**。给出 2–3 个可被真机探针判别的具体候选点。

---

## 1. 项目与运行环境背景

- 仓库 `minix-rs`：把 Minix3 微内核操作系统用 Rust 重写（`#![no_std]`，目标
  x86_64 / aarch64 / riscv64）。真值参照是原版 C 源码 `minix3/`（只读，勿改）。
- 微观结构：进程管理 PM、虚拟内存 VM、文件系统 VFS、调度器 SCHED 等都跑在**用户态
  服务器进程**里，内核只做 IPC / 中断 / 最小内存管理。服务器与内核之间靠一种**定长
  64 字节的 IPC 消息**通信（`os/libs/minix-types/src/ipc/message.rs:31`
  `pub const MESSAGE_SIZE: usize = 64;`，结构体 `Message` 见同文件 `:48`，含
  `m_source`/`m_type` 各 4 字节 + 56 字节联合负载 = 8 个 u64 字，索引 0–7）。
- 进程用 `fork` + `exec` 派生子进程。`init`（第 9 号服务器角色，运行登录脚本）会
  `fork` 出一个执行 `/bin/sh` 的子进程。
- **构建判据**：release 构建（`-O2`、默认省略帧指针）。宿主 mock 测试构建与真机构建
  用 `feature = "mock"` 区分；大量诊断/边界代码用 `#[cfg(not(feature = "mock"))]`
  门控——**这意味着"某探针/分支没触发"很可能只是被编译剪除了，不是逻辑没走到。**

## 2. 症状（精确）

aarch64 单进程启动（`-smp` 任意，`-smp 1` 也复现）跑到最后阶段确定性死锁/崩溃：

1. 内核已自举，12 个系统服务器 + `init` 全部 `exec` 成功；
2. `init` 进入 `Runcom` 态（正在跑 `/etc/rc` 登录脚本），`fork` 出子进程
   `/bin/sh`（进程表 slot 12，endpoint `0x800c`，是 init 的第一个 fork+exec 子）；
3. 该子进程在启动早期触发一次堆分配，请求体量 **≈ 4 GiB**
   （`size = 0x7ffffffe25c0`，其低 32 位 `fffe25c0`）；
4. `minix-rt` 的全局分配器 `alloc_big` 收不到这么大的堆 → 返回 null；
5. 对 null 解引用 → `SIGSEGV`（串口证据 `csig tgt=0xc sig=0xb`）；
6. 全系统转入空闲，登录标记（rc marker）永远不打印。

关键放大事实：**同一种子调用链在 x86_64 上正常。**
即 `minix-init` 的共享 Rust 逻辑在 x86_64 单核构建里能一路跑到 rc marker、零 panic；
只有在 aarch64 的 fork+exec 子进程里，才出现"把一个栈地址当容量"的读取。
⇒ **强烈指向 aarch64 专属的架构层差异（子进程初始栈 / 寄存器态 / exec 栈镜像布局），
而非共享的 init 源码逻辑。**

## 3. 已经【被真机或静态证伪】的假设 —— 请勿重复这些死路

这一段是本文件最重要的部分。前面 17 个追查轮次里，下面这些都被推翻了，重新提出等于帮倒忙：

| 编号 | 曾被提的假设 | 怎么被证伪 |
|---|---|---|
| 续-22（**已修**） | 发送方 `m_type` 的 store 跨陷入未物化（编译器 Heisenbug） | `read_volatile` 屏障落地后，本轮**已 0 次 m_type=0**，本缺陷类清零。当前问题与它**无关**。 |
| 续-25 | fork 父子共享/陈旧 delivermsg 物理页导致 | PA 物理地址对比针排除 |
| 续-27/28/31 | 内核延迟投递把"非法 m_type + 携 INIT 栈指针的高位字"写进 INIT 栈、经 CoW 遗传给子 | **续-32 静态证伪**：探针读的 `w9` 偏移 72 > `size_of::<Message>()`=64，**越界读到了紧邻的内核字段 `p_delivermsg_vir`**（那里本来就存用户栈指针）；PM 回复正文实为全零干净。因果链失去支柱。 |
| 续-29 | poison Vec 来自 `init` 的 `host::read_file` | 用户侧地址探针 `nk4j` 在 OOM 前从未触发 ⇒ `read_file` 根本没被调用 |
| 续-29/续-33 排除 | poison Vec 来自 `execve.rs` 组装 exec frame | `os/commands/sbin/init/src/execve.rs:87-104` 的 `stack_params` 全程用 `checked_add` + `ok_or(E2BIG)`，超大长度会**先 E2BIG 早返**、且 E2BIG 是优雅报错非 SIGSEGV ⇒ 不可能把 `0x7ffffffe25c0` 递进 `try_reserve_exact` |
| 续-33 排除 | 来自 `crt0` 读 argv/envp 的字符串切片 | `os/libs/minix-rt/src/crt0.rs:260-269`/`375-382` 的 `string_at`/`argv_storage_of` 按 **NUL 扫描**计长（遇 0 即停），不是指针相减，产不出栈地址量级的长度 |
| 若干轮 | 归因于 SMP 竞态 / 架构特异性 | 曾一度"钉死 aarch64 架构特异"，后证是 Heisenbug 时序 + 越界读伪影的叠加，SMP 被 `-smp 1` 复现排除 |

## 4. 当前的最优假设（起点，不是结论）

**有界扫栈（续-33）定谳**：毒分配发起自 `minix-init` 二进制内部的
`alloc::raw_vec::RawVecInner::finish_grow` → `__rust_alloc` → `minix_rt::alloc`
→ `Allocator::alloc` → `alloc_big`。也就是说：**init 里有一个 `Vec` 在 grow，
目标容量 = `0x7ffffffe25c0`（＝一个 INIT 栈地址量级的值）。**

结合"x86 正常 / aarch64 崩"，最可能的机制（择一或组合）：

- **(甲) exec/子诞生时初始栈布局差异**：aarch64 上 `ps_strings` / argv / envp / auxv
  在用户栈上的摆放，本应写入"计数/小值"的槽位实际落进了一个栈指针，或某字段宽度/
  对齐与消费侧读取假设不一致（x86_64 的 64 位 vs aarch64 的字宽/对齐细节）。
- **(乙) 继承栈偏移错一格**：fork 子初始 SP / 寄存器态使子从父栈拷贝来的 argv/env
  边界指针被当成元素个数或切片长度来 `Vec::with_capacity` / `reserve`。
- **(丙) 某个从 IPC 回复里读的 `length/count` 字段**，在 aarch64 上落到消息 64 字节
  联合负载的**错误偏移**（注意：绝不能再假设"越界"，务必以 `size_of::<Message>()=64`
  为硬上界核对每个字段偏移）。

## 5. 建议你精读的具体代码区（跨架构对照优先）

1. **crt0 入口 stub 的架构分支**：`os/libs/minix-rt/src/crt0.rs`
   —— 找所有 `#[cfg(target_arch = ...)]` 的 entry / stack 解析腿，逐段对比 aarch64
   与 x86_64 如何从初始 SP 读出 argc/argv/envp/auxv。
2. **架构层启动与陷入返回**：
   `os/arch/src/arm64/boot.rs`、`os/arch/src/arm64/trap_return.rs`、
   `os/arch/src/arm64/exception.rs`
   对照 `os/arch/src/x86_64/boot.rs`、`os/arch/src/x86_64/trap_return.rs`。
   重点：子进程首次进入用户态时，aarch64 放进 SP/x0..x30 的初始栈内容布局，与
   crt0 消费侧假设是否一字节不差地对齐。
3. **内核 exec / 栈填充消费侧**：`os/kernel/src/syscall_process.rs`
   （搜 `ps_strings` / `stack` / `auxv` / `arg` 相关）—— 装载 ELF 后如何在用户栈上
   摆 argv/env/auxv，aarch64 与 x86_64 有无宽度/顺序/对齐差异。
4. **init 侧 exec**：`os/commands/sbin/init/src/execve.rs`
   （尤其 `:296 try_reserve_exact(frame_size)` 及其上游 `stack_params`），确认毒值
   是否真的进不去（续-33 认为进不去，请你复核这个排除是否成立）。
5. **分配器诊断腿**：`os/libs/minix-rt/src/alloc.rs`
   （`alloc` 入口、`nk4a: rs-bigalloc` 已提交的大分配打印腿）——理解 `size` 是从哪个
   调用者灌进来的。
6. **C 真值对照**：`minix3/minix/kernel/system/do_exec.c` 与
   `minix3/minix/kernel/` 里 `ps_strings`/栈初始化相关实现，看原版 C 如何在各架构摆放
   用户栈初始帧，作为"正确形状"的基准。

## 6. 输出契约（务必遵守）

- **产出 2–3 个候选点**，每个候选写成四元组：
  1. `文件路径::符号`（越精确越好）；
  2. aarch64 相对 x86_64 **具体差在哪**（宽度/对齐/偏移/字段顺序/初值）；
  3. 在这个差异下，**什么值会落进"本应是计数/长度"的槽位**、为什么恰好是栈地址量级；
  4. **一条真机探针如何判别它**（在哪个点读什么、期望两种假设各看到什么）——因为最终
     裁决仍要靠真机，不是你的静态结论。
- **禁止**：直接改代码、给出"根因就是它"的定论、提出波及全系统 IPC/内存的大改作为"修复"。
  本项目铁律是**"未坐实不成修"**——你的候选必须能被探针二选一验证。
- 如果某候选与第 3 节的已证伪项重叠，请明确标注你已读到该证伪、并说明你的候选与它的区别。

## 7. 会再次绊倒人的坑（前车之鉴）

- **`Message` 只有 64 字节 / 8 个 u64 字**：任何"消息高位字/word[9]/offset 0x48"式读数
  都是**越界读到相邻内核字段**，续-27/28/31 就栽在这（`proc.rs` 里 `p_delivermsg`
  紧接 `p_delivermsg_vir`，后者存的正是用户栈指针，极像"消息里回音了栈地址"）。
  **核对字段一律以 `size_of::<Message>()` 为硬上界。**
- **release 省略帧指针 + 泛型 `grow` 高度内联**：靠扫栈/返回地址链追调用者很可能够不到
  app 帧（本机 `objdump` 又不认 aarch64、无 `llvm-objdump`），别把"扫不到"当成"不存在"。
- **`cfg(feature="mock")` / `cfg(not(test))` 门控**：诊断分支在某个构建里可能被整段编译
  剪除，"0 次命中"最便宜的解释往往是"代码根本没编进去"，不是"逻辑没走到"。
- **镜像新鲜度**：任何结论都必须建立在从当前源码树当场重建的二进制上；用旧镜像跑出的
  "证据"一律作废。
- **Heisenbug 陷阱**：探针自身的串口打印会改变时序/寄存器分配，让缺陷时隐时现；判据要
  两次独立复跑签名一致。

## 8. 名词与端点速查

- 端点（boot 序 slot→endpoint）：`ds=6, rs=2, pm=0, sched=4, vfs=1, memory=3,
  tty=5, mib=7, pfs=9, mfs=10, init=11`。
- `slot 12 / nr 0xc / endpoint 0x800c`＝init 的第一个 fork+exec 子（跑 `/bin/sh`），
  即本 bug 的真受害者。
- `0x7ffffffe25c0`＝出现在崩溃里的 poison 值，形态是一个 INIT 栈区地址（`0x7ffffffe2`
  区），被当成容量/长度；子自身栈在更低的 `0x7ffffffc6` 区。
- "rc marker"＝启动成功判据字符串 `minix-rs rc: minimal boot script marker`。
- 完整取证历史在 `notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md`
  §1.120 续-22 至 续-33（本文件是其"已收敛结论 + 待查候选"的压缩版）。

---

**一句话总结现状**：投递/消息污染类已排除（续-32），缺陷域收窄到
**aarch64 架构层的子进程初始栈/exec 栈镜像**；下一步是静态对照 aarch64 vs x86_64
的 crt0 入口与内核栈填充，找出"哪个本该是计数的槽位落了栈指针"。请你独立做这件事，
给出可被探针判别的候选点。
