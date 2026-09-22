# NK4 两次迭代回归评审报告（迭代11-18 + qwen 两次迭代，2026-09-22）

> 评审对象：`6a7d513d0^..a476b86f9`（49 commit；本会话迭代11-18 七笔修复 +
> 证据/docs 三笔 + qwen 第一次迭代 NK4A Task A/C + 第二次迭代 NK4B P0→M3.4）。
> 快照：HEAD = `a476b86f9`（评审期间 qwen 会话仍在进行，其后 commit 留待增量）。
> 评审方式：**只读、提交态静态审**（`git show` 读快照，非工作树）；
> 执行 skill = code-excellence（设计分层/非法态封堵/死代码/C 对位抽查）。

---

## 一、并发安全答复：只读 review 是否需要 worktree 与锁？

**基本不需要，但有四条边界**（本报告即按此执行）：

1. **读提交态而非工作树态**——qwen 正在改文件（评审开始时
   `NK4B-WORKLOG.md` 处于已修改未提交态），一切代码内容用
   `git show <commit>:<path>` / `git show <commit>` 读快照；绝不
   checkout/reset/stash（会砸 qwen 的工作树）。
2. **不跑 QEMU**（无锁资源，会与 qwen 的真机轮次互踩）；本轮零真机操作。
3. **构建类验证本轮跳过**：cargo 有 target/ 文件锁，并发构建安全但串行
   变慢，且 docker -m 2g 双开有内存压力。本报告为纯静态审——**局限声明**：
   各 commit 的宿主测试计数以当时 commit 信息自述为准（逐条附原始声明，
   未复跑）；incremental review 时可补跑。
4. **报告提交**：只 `git add` 本报告新文件 + commit；若撞
   `index.lock`（与 qwen 的 commit 竞争）则重试——git 索引锁原子性保证
   不损坏。结论：**不需要 worktree，不需要 claim.sh 锁**；上述边界内
   只读评审与执行会话可安全并行。

## 二、总评

**两次迭代全部 23 个代码 commit 定性为可信/优秀，未发现 P0/P1 正确性
问题**；发现 P2 级改进点 2 处、观察项 4 处（见 §5）。亮点超出预期：

- **记录纪律 fully implemented**：WORKLOG 双册在位且按模板逐节；
  FIXLOG 追加不变量（`grep -c` 头计数 = 1）保持；qwen 自发做了
  「M3.3 里程碑 CodeReview」并修掉自己两条 P1/P2（2b33ee480）——
  评审闭环内化。
- **判别性测试文化形成**（这是本弧线最值得沉淀的资产）：
  - 迭代20：删镜像/删 CPL 门各断一组，双向判别（443624551）；
  - M3.3：startup.nsh 字节冻结测试，**变异验证**（写回漂移字节 →
    FAILED，还原 → 12 passed）（2b33ee480）；
  - PL011：MMIO 不可宿主测，把"判断力"抽成宿主可测纯函数（1a2a8eb61）。
- **诚实度高**：ab79b40ba 把 aarch64 载体编译断裂归因到探针时**包含
  自己上一里程碑的探针**；443624551 自证"本修非 Task C 崩溃根因"；
  riscv64 在 xtask 面保留 honest bail 不假放行（2b6d98ffd）。
- **C 对位抽查三处全真**：febbb0c8b ↔ pagefaults.c:97-105（SIGSEGV+
  CLEAR_PAGEFAULT 双联）逐行核实；迭代20 ↔ mpx.S SAVE_PROCESS_CTX；
  迭代11-18 的对位前一轮已验。

## 三、逐 commit 定性表

### 3.1 本会话迭代11-18（自审，新鲜眼复核）

| commit | 定性 | 复核要点与结论 |
|--------|------|----------------|
| 6a7d513d0 迭代11 | 可信 | delivermsg 返回码写入；CONTEXT_SET 门控对位 proc.c:290；毒化 RAX 判别测试 |
| 13a00244e 迭代12 | 可信 | RECEIVE 序言清状态寄存器+REPLY_PEND；**复核通过**：SENDREC 不清（C `if (call_nr == RECEIVE)` 同构）；Phase-0 首检路径先清后 OR 顺序同 C；用户 rbx 由 minix-sys wrapper 自存自恢复，ps_strings 不受影响 |
| 835e1569f 迭代13 | 可信 | finish_holding_bkl 恰一个新调用方（stage 3a），正常路径 kernel_call_finish 未破坏；VmSuspend/完成两处解锁条件化正确 |
| 27f0a038a 迭代14 | 可信 | dequeue_if_blocked 幂等（runnable+in-queue 双门）；handle_pagefault 逐页 fail-closed 登记诚实 |
| 72b4415d2 迭代15 | 可信（P2-1） | PF_W 魔数 `seg.flags & 0x2`——minix-elf 未导出常量，建议补 `pub const PF_W: u32 = 2`（§5 P2-1） |
| 5d3d8147c 迭代16 | 可信（P2-2） | `user_sp.0 - 4MiB` 无下溢防护——生产 user_sp=0x7ffffffff000 安全，建议 saturating_sub+assert（§5 P2-2） |
| 1277909e1 迭代17 | 可信 | .bss 按需物化论证成立；模块 memsz 增大对 boot-shim 装载为额外零页，容量充足 |
| 4a6570d7f 迭代18 | 可信 | SendA 上移后不可达防御臂保穷尽；senda 自带 SYS_PROC+逐条检查已核 |

### 3.2 qwen 第一次迭代（NK4A Task A/C）

| commit | 定性 | 要点 |
|--------|------|------|
| 5f98b1db5 pf-exit 探针 | 可信 | 8 cap + mock 门纪律；**一发定性真停滞**（noaddr cr2=0，推翻 Suspended 假设——探针方法论胜利） |
| febbb0c8b VM pf 终局收口 | **优秀** | pf_fail_segv 去重（code-excellence 式）；C pagefaults.c:97-105 逐行对位属实；堵死"RS 永停 PAGEFAULT"死锁态 |
| 443624551 迭代20 IRQ 存帧 | **优秀** | 真内核级缺陷（抢占后从陈旧 ctx 恢复）；mpx.S SAVE_PROCESS_CTX 全 stub 对位；x86 门（吸取 aarch64 回归教训）；双向判别测试；kernel 809 |
| 89cd5929c / cb6377842 / 3945cf5d0 取证探针 | 可信 | cap+门纪律；RBX 三重身份（ps_strings/IPC 状态/活值）分析链有价值 |
| 84347cff2/9424d9542/2bc362971/19de2e9c2/236515906 记录+证据 | 可信 | Task C 按 BLOCKED 诚实停机（铁律 10 执行）；第六轮静态穷举 ctx.rbx 写者 |

### 3.3 qwen 第二次迭代（NK4B，截至 a476b86f9）

| commit | 定性 | 要点 |
|--------|------|------|
| 43ad2dbec P0 核账 | 可信 | 六包实测计数落盘；P1 不跳过判定——核账机制按任务书执行 |
| 3945cf5d0 P1 第 7/8/9 轮探针 | 可信 | 存帧/交付配对取证 |
| ab79b40ba aarch64 载体编译门 | **优秀** | 纯 cfg 门零删除；归因含自己探针（诚实）；x86 语义零变化论证 |
| 073003e8a M3.2 kernel-image aarch64 | **优秀** | 基址不发明（对位既有 arch 实现）；宿主 readelf 断言=可测退出判据 |
| aedbff11f / 7c323a039 M3.2 修复 | 可信 | 里程碑 CodeReview P2 闭环；vma_mode 未知值硬失败（fail-fast） |
| 873f3e947 M3.3 设计先行 | 可信 | 五决策+风险清单先于实现——任务书"设计要点先写 WORKLOG"执行 |
| 1a2a8eb61 PL011 FIFO 节流 | **优秀** | 有界轮询对齐 x86 先例同数量级；判断逻辑抽宿主可测（架构抽象族正例） |
| 5ac5625f1 boot-shim aarch64 放行 | 可信 | fw-uefi-image 内部特性解 required-features 全开语义——同 M3.2 踩坑复用 |
| 2b6d98ffd xtask 装机表 | **优秀** | Arch::uefi_slots() 表驱动；BOOTAA64.EFI 是 UEFI 规范默认名非自创；riscv64 honest bail |
| 6e80b5e9c AAVMF 载体 | 可信 | 判据=M3.3 定义的两行路标不越权；gic-version=3 硬要求带出处 |
| 2b33ee480 startup.nsh 回退 | **优秀** | 自捕 P1（字节漂移）+ 字节冻结测试 + 变异验证判别力 |
| 07b9afe7f parse 失败打印变体 | 可信 | 可观测性小步 |

### 3.4 docs/evidence commit（eebe41550、e4c6e8224、388252b6b 及 qwen 的 docs 系列）

可信任：任务书/证据归档/WORKLOG 记录；内容与代码 commit 对应关系抽查一致。

## 四、设计层审视（code-excellence 视角）

### 4.1 非法态封堵清单（本弧线触及的状态机，逐条）

| 状态机 | 新封堵 | 判定 |
|--------|--------|------|
| IPC 状态字（RBX 三重身份） | RECEIVE 序言清零 → 完成时 OR 得干净状态字；迭代20 IRQ 全量镜像保活值 | 封住"指针|状态"非法态 ✅ |
| RTS_PAGEFAULT 生命周期 | 内核置位 ↔ VM 成功填充/`pf_fail_segv` 双联清除 | 封住永停死锁 ✅（此前缺口已修） |
| VmSuspendContext（Pending/Fetched/Completed × KCALL_RESUME） | stage 3a 仅 Completed 重派；Pending/Fetched 跳过；重挂起走 finish 簿记 | 封住陈旧重派 ✅ |
| 运行队列出队/入队对称 | vm_enqueue_and_notify_vm 顶部统一出队半 | 封住 spin-pick 饿死 ✅ |
| BKL 所有权 | dispatch（自取锁链）vs stage 3a（环境见证链）两形态分离，holding_bkl 变体解锁条件化 | 封住自死锁/丢锁 ✅ |
| 待关注 | VM Fetched 后若 VM 死亡则请求永悬（无 watchdog）——与 C 同态，非本轮引入 | 平价，不立项 |

### 4.2 架构抽象族（模式 79-82）

- PL011 节流的"判断力抽出"（1a2a8eb61）与 xtask `Arch::uefi_slots()`
  表驱动（2b6d98ffd）是抽象正例；
- kernel-image aarch64.ld 复用 x86_64.ld 契约形状（LMA/VMA 对齐关系），
  基址取自既有 arch 实现而非发明——符合"基址不发明"原则；
- 无架构违规：新 unsafe 仅 1 处（迭代20，SAFETY 注释+契约论证在位）。

### 4.3 死代码

- 零新增死代码；`TODO/FIXME/dbg!` 残留扫描 = 0；
- `handle_memory_once` 生产调用面收窄后仍被 exit.rs 使用，未死；
- `indexed_endpoints()`（诊断，标注 task1-close 裁决）按纪律存在。

## 五、发现清单

- **P2-1**（72b4415d2）：`seg.flags & 0x2` 魔数。minix-elf 未导出 PF_W；
  建议补 `pub const PF_W: u32 = 2;` 并引用（与既有 PT_LOAD 同族）。
- **P2-2**（5d3d8147c）：栈 region 基址 `user_sp.0 - DEFAULT_STACK_LIMIT`
  无下溢防护。生产值安全；建议 `saturating_sub` + debug_assert。
- **观察-1**：迭代20 后每次用户态 IRQ/tick 全量镜像 + BKL 下表访问——
  C 同构（每 stub SAVE_PROCESS_CTX），SMP 扩展时注意该路径成本。
- **观察-2**：探针存量大幅增长（pf-exit/pf 定性/RBX 轨迹/拓扑 parse/
  boot-shim 路标等新组）——task1-close 裁决清单需按 FIXLOG 最新
  存量段落重盘（评审方职责，此处登记）。
- **观察-3**：Task C（RS step2 端点 0 槽缺失，时序敏感）与 M3.4
  （platform panic 钉在 check_gic_madt）两线 BLOCKED/进行中——增量
  review 的首批对象。
- **观察-4**：宿主计数本轮未复跑（并发约束）；incremental review 时以
  qwen WORKLOG 实测计数为基线抽验。

## 六、增量 review 协议（qwen 后续 commit）

1. 增量范围 = 本次快照 `a476b86f9` 之后的新 commit；
2. 只审代码类（fix/feat/debug/test），docs 抽查 WORKLOG 一致性；
3. 每个新 fix 仍按本报告 §3 的三查：C 对位真实（grep minix3）、
   探针/测试纪律（cap+门+判别性）、诚实度（BLOCKED/边界登记）；
4. 真机验证类声明在 incremental review 时补跑宿主测试抽验；
5. 架构级「上交裁决」项出现时由评审方裁决后再合入主线使用。

## 七、方法与局限

- 静态审（git show 提交态快照），零工作树写入、零 QEMU、零构建——
  与执行会话并行的代价是不能复跑测试；所有"全绿"陈述引自各 commit
  信息自述（保留原文可查）。
- C 对位抽查 3 处（pagefaults.c:97-105 / mpx.S SAVE_PROCESS_CTX /
  proc.c:578-583 前轮已验）全真；未逐一复核全部 23 commit 的 C 引用。
- 证据目录：本报告即证（引用 commit hash 均可 `git show` 复核）。
