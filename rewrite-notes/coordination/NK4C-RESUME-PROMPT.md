# NK4-C 续跑交接 prompt（初版 2026-09-27；**2026-09-29 续-50 后全面刷新**；接手者上下文 200k，务必按本文件的读法与节奏）

> 本文件是**接手者的唯一入口**。上一手 agent 已把状态固化在 git 与 `NK4C-WORKLOG.md`。
> 你的上下文只有 200k —— **不要通读 7100+ 行的 WORKLOG**，按 §2 的读法读。

---

> ## 🛑 交接状态（2026-09-29）·预算耗尽终止于续-50 实施轮（HEAD `8f6cb996e`）
>
> **x86 -smp4 早崩已根因坐实（boot DM 覆盖缺口·续-49）并完成 source-4 最修实施（续-50·host 全绿+真机 4 轮原崩态零出现）；改动故意 NOT-WIRED 留工作树待补验证链+CodeReview。新阻塞腿＝用户栈缺页（见 §3.2）。接手入口＝§2 开工仪式 → §3.1 第一动作。**
> - 取证→实施链 commit：续-47 `39a4ed49d` → 续-48 `228a4addd` → 续-49 `0b8fc3b2e` → 续-50 WORKLOG `8f6cb996e`。
> - 工作树 tracked 改动：**仅 `os/kernel/src/dm_coverage.rs`（source-4 最修·未提交·勿丢勿硬提）**＋本交接件。探针全部已回滚（grep NK48-TEMP=0）。
> - **GLM 七度核查结论**：全 ref（fetch+for-each-ref+log --all --not HEAD+stash）无任何 GLM 修复 commit；唯二 GLM 产物 `NK4C-BUG-AARCH64-VEC-CAP{,-GLM}.md` 系静态分析交接件（自陈禁下结论禁改代码·非补丁）。若用户再提「GLM 已修复可 review」：先按续-48 A 节配方核查，无则诚实上报、不编造 review，继续 §3 真前沿。
>
> 以下正文（§0/§3/§8/§11 已按 2026-09-29 前沿重写；§3.4 以下旧 frontier 留档）。

---

## 0. 你的身份、任务与终目标

你是 **NK4-C「清零者」长程自主任务的接手 agent**。任务是把 minix-rs（Minix3 内核 Rust 重写，x86_64/aarch64/riscv64 no_std）从「boot 硬 livelock」推进到**三架构跑通 OS 并跑通命令面与 minix3 测试套件**。

**终目标（§0.1 三条，全部达成才算完）**：
1. 三架构 OS 各自启动并打印 rc marker（`minix-rs rc: minimal boot script marker`）；
2. 18-stage 命令面在 OS 上跑通（echo/ls/cat 为核心）；
3. minix3 的 `tests/` 在机器上跑起来。

**当前所处位置**（2026-09-29 更新）：**续-50 实施轮因 turn 预算耗尽交接**（HEAD `8f6cb996e`·详见顶部🛑 banner 与 §3）。**终目标① 进度 = x86_64 单核手改命令下 marker 曾✅（§1.119续-7）/ 标准 xtask 启动器（-smp4）下未稳：boot DM 栈洞已击穿、新阻塞腿＝用户栈缺页（§3.2）/ aarch64 模式① 未克（§3.0）/ riscv64 未启动真机**（接入验收清单：`rewrite-notes/coordination/riscv-reviewlog.md` §A）。之后再按 §5 推进命令面 → W^X → ABI 清单 → 测试上机 → 收尾清账。

**这是长程任务**：你会连续修很多 bug、做很多轮真机复跑，**不要做一步就停下来汇报**。用户会在需要时手动让你收尾，届时才由上一手 agent 接手。你的职责是：**让工作始终可接手**（每次 commit + 写报告）。

---

## 1. 三个不可违反的硬约束

### 1.1 绝对不要修改 `AI-chats/daily.todo.md`
这是用户的私人笔记文件。**任何情况下都不许动它**（不要 add、不要 commit、不要编辑）。它在 `git status` 里显示为 modified 是历史遗留，忽略即可；提交时用 `git add <明确路径>`，**禁止 `git add -A` / `git add .`**。

### 1.2 每个逻辑单元 = 一次 commit（带完整报告）
见 §4。这是本任务最核心的纪律——没有它，长程任务会变成不可接手的一团乱麻。

### 1.3 只在三种情况停下来问用户（其余都不是停止理由）
1. **架构裁决级**：改变外部契约、需要 `[ARCH: ...]` 三处一致标注的设计决策；
2. **破坏性操作**：删除/覆盖非自己创建的文件、重置分支、force push、清理他人工作树；
3. **终目标达成**（§0.1 三条全满足）——此时停下交付总结。

**bug 难、多轮失败、工作量超预期都不是停止理由**：换方向继续，并保证 WORKLOG 随时可接手。

---

## 2. 开工仪式（第一步，严格按此读，控制上下文）

按顺序执行，**不要跳步、不要扩大阅读范围**：

```bash
# ① 看仓库状态与最近提交（了解上一手做了什么）
cd /home/xzhao/github/minix-rs && git log --oneline -12 && git status --short | grep -v '^??'

# ② 读 WORKLOG 的「当前状态」段（约 60 行，一屏读完）——这是权威状态
sed -n '1,70p' rewrite-notes/coordination/NK4C-WORKLOG.md

# ③ 读 WORKLOG 最后 2 节（上一手的最后结论与下一步配方）
tail -120 rewrite-notes/coordination/NK4C-WORKLOG.md

# ④ 只读你当前要做的那个单元的路线图行（不要通读整个 §6）
sed -n '/^## 6/,/^## 7/p' rewrite-notes/coordination/NK4C-OPENING-PROMPT.md | head -25
```

**读完必须能回答**：当前 frontier 是哪个编号？上一步的结论是什么？下一轮的第一个动作是什么？
若答不出，**只补读 WORKLOG 中相关的那一节**（用 grep 定位节号，不要整篇读）。

**不要读**：`NK4C-WORKLOG.md` 全文、`NK4C-OPENING-PROMPT.md` 全文、`.review/` 下的历史、`migrate_notes_plan/`。它们在需要时用 grep 精确定位即可。

---

## 3. 当前精确 frontier（2026-09-29 交接点，HEAD `8f6cb996e`·续-50 实施轮）

### 3.0 三架构实时进度（目标① 拆解）

| 架构 | 状态 | 当前阻塞项 |
|------|------|-----------|
| x86_64 | 标准启动器（xtask 硬编码 -smp4）下 marker **未稳** | 原 boot DM 栈洞崩已被 source-4 击穿（真机 4 轮零出现）；新腿＝r3 型用户栈缺页；候选改动未提交（见 3.1） |
| aarch64 | 模式① 间歇 ~4GiB OOM（毒 String.len＝栈地址）未克 | g14：H7 纯数据流，追毒 len 标量 store 的源 GPR 生产者（对照 codec）；另缺陷 E（FPSIMD 跨陷入不保存恢复·g6）独立立项 |
| riscv64 | 未接 IPC 桥，真机未启动 | 接入验收清单 `riscv-reviewlog.md` §A；x86/riscv 同走内联 arch_boot（boot-shim/src/main.rs:86） |

### 3.1 你在接手时刻的唯一在途状态（重要）

`os/kernel/src/dm_coverage.rs` 有一笔 **uncommitted 生产改动**（续-50 实施的 DM source-4 最修：`reserved_regions_candidates` + `establish_boot_dm` source-4 循环 + `kernel_dm_pa_end` 同步 + 文件头四源注释 + 2 个新 mock 测试）。它故意不提交（验证链未走完：x86 仅 4/≥6 轮、aarch64/riscv 回归未跑、CodeReview 未走）。**接手后第一动作**：

1. 开工仪式（§2）后立即 `git diff os/kernel/src/dm_coverage.rs` 核对改动仍在；
2. 补验证链（续-51 配方，WORKLOG 顶部前沿与文末 §1.120续-50 D 节逐字可执行）：
   - host：`cargo test -p minix-kernel -p minix-arch -p minix-boot -p minix-types`（基线 825+309·只增不减）+ clippy 零新告警 + nightly rustfmt 零新增漂移（§8.4）；
   - x86 总 ≥6 轮（已跑 4：nk50-x86-r1..r4·原崩态 −0xfc84/CR2=rsp+0x2c0 零出现）再 ≥2 轮；判据：`tr -d '\0' < serial | grep -c 'minimal boot script'`；r4 已达 `birth s3 runtime ok` 全程无 panic；
   - aarch64/riscv 镜像各 ≥2 轮不回归（establish_boot_dm 三架构共用）；
3. 全链绿 → 派 CodeReview 子代理 → 通过才 `git add os/kernel/src/dm_coverage.rs rewrite-notes/coordination/NK4C-WORKLOG.md` 提交（WORKLOG 顶部前沿同步滚动）；不绿则按取证轮处置（不臆造、不硬提）。

### 3.2 下一层阻塞项（r3 型新腿·坐实后再修）

真机现象（nk50-x86-r3）：kernel #PF rip `0x7ffffffe2f88` errcode `0x11`（P|RS|US＝用户态取指/读）——正是用户栈指针 VA（与 r4 `w-fin va=0x7ffffffe2f88` 同一常量）；birth 路径切入用户态后栈/代码页在进程页表未映。r1/r2 仍现 `pagefault in VM`（trap_dispatch.rs:981）。与 boot DM 无关，需坐实：birth s3 → 首次切入用户态前谁负责映 user stack（对照 C copyall/fkinit 与 VM 侧 map 时序·minix3 锤点自行 grep）。另 aarch64 模式①（g14）与 riscv IPC 桥并行另开。

### 3.3 近期已入土结论（勿重翻）

- x86 -smp4 早崩＝boot DM 覆盖缺口（非 SMP 竞态·UEFI 栈落位漂移踩洞）：续-47/48/49 取证链 + 续-50 真机击穿。差值法（rip−db）仅可用于同二进制内钉稳定偏移；kernel.elf nm 反查 shim 内联代码归属无效（续-48 C 方法论纠正）。
- CR2 探针（KernelPanic 臂补读 cr2）用后即滚；函数级归属如需：shim /MAP（rust-lld COFF）或 PDB 工具。

### 3.4 （历史·2026-09-27 旧 frontier 原文留档）

### 3x. 旧 frontier（2026-09-27 交接点，commit `b82afee4b`）——已被续-5…50 取代，仅供史实查阅（其下 3.1/3.2/3.3 编号属旧体系）

### 3.1 已完成并真机验证的里程碑（近期，勿重做）

| commit | 内容 |
|--------|------|
| §1.85–§1.98 系列 | fork COW 写保护、exec text 缺页、premature-OOM 闭环、VM 栈 runway、DeliverMsg endpoint —— **x86_64 单核 rc marker ✅**（§1.119续-7 双跑 marker=2、panic=0） |
| `07c9e6649` | **§1.119续-7 真修复**：PM `sched_start` 补校 SCHED 回复 `m_type`（对位 C `sched_start.c:87`）→ 破五轮 EDEADEPT/SingleUser 活锁；aarch64 boot 推进到 **12 server+INIT 全 exec 成功、INIT 达 Runcom 正跑 /etc/rc**；日志 12.7万行→4295 行 |
| `07c87b3cc`→`b82afee4b` | §1.120 → 续-4 取证链（均为探针已全回滚的取证/静态轮）：aarch64 死锁逐轮锐化至下述停点 |

### 3.2 当前停点（§1.120续-4）：VFS 对 INIT 的 Stat 回裸 EIO、往返 7 轮后第 8 次停摆

真机现象：INIT（runcom 跑 /etc/rc）向 VFS(ep1) 发 `mt=0x115(277)` = **`VfsCallNum::Stat`**（call_table.rs:50）；VFS 回 **EIO(5)**；INIT 重发→再回 EIO，**7 轮全同后停摆**（第 8 次发出后 VFS 永不再回），日志冻结 ≈4295 行，rc marker 不出。

已排除/已收敛（详文见 WORKLOG 文末 §1.120续-1…续-4）：
- 续-3 内核探针已**排除 IPC 回复投递腿 bug**：VFS 确实回了、回对槽（di=0x10=INIT）、回的是 EIO；
- 续-4 静态全枚举 Stat 腿 EIO 产出点四条腿：①入口臂 `syscalls.rs:1075/1098`（经 `send_lookup_for_slot` `main_loop.rs:1928/1934/1962`）；②lookup 回复透传——**不产 EIO**（ENOENT 会以 2 现形，真机是 5，FS 语义车道排除）；③相位 2 `main_loop.rs:6879/6888`；④**传输层折叠腿 `flush_pending_fs`（`main_loop.rs:4701-4729`）：`fs_sendrec` 的 `Err(e)` 被 `let _ = e` 吞掉原始码、统一折成裸 EIO 回用户**。
- **腿④是唯一同时解释「7 次全同 EIO」+「第 8 次 rendezvous 挂起不返回」两特征的单一机制**；头号嫌疑 = vmnt 表登记的 `fs_e` endpoint 陈旧/错指；次选 = 腿①/③的 grant/vmnt 子点失败。

### 3.3 你的第一动作：腿④真机探针判别（再修）

在 `os/servers/vfs/src/main_loop.rs` 的 `flush_pending_fs` 装一次性 TEMP 探针（非缺页 handler、AtomicUsize 门控、输出≤16B、task-close 全回滚），打 `fs_e / vmnt / Err 原始码 e` 三点，按 §8.6 重建 aarch64 镜像跑真机：
1. 若 7 轮全同码且第 8 轮无输出 → 再在 `fs_sendrec` 前后各一条钉死「已进入未返回」，并 dump vmnt 全表对照 boot→endpoint 实况映射（mfs 应＝ep10；映射表见 WORKLOG §1.120续-2 「boot→endpoint 映射」段）→ 定修 vmnt 注册腿；
2. 若 e 原始码指 EDEADEPT/EBUSY 类 → 回 kernel ipc 侧查；
3. 若腿④不命中 → 探针移到腿①/③三点。
修后判据：aarch64 串口出 rc marker；修复轮必跑三件套（§4.1）+ CodeReview。

**侧线状态**：riscv 前置对账已就绪（`riscv-reviewlog.md`：§A 验收清单、§D 十三笔结构债已收敛、§E 用户预裁章——riscv 轮开工前必读且 E 章纪律不可违）。

---

## 4. 工作纪律（硬约束，违反即返工）

### 4.1 每个逻辑单元 = 一次 commit，含完整报告

**一个「逻辑单元」的边界**：一次「定性 → 定位 → 修复 → 验证」的闭环，或一次纯诊断轮的结论固化。

**每个单元的固定节奏**（不要合并、不要省略）：

1. **诊断**：写探针，跑**一轮**真机，拿到数据；
2. **定位**：给出根因，**必须带 C 源码锚点**（`minix3/minix/kernel/proc.c:1071-1075` 这种格式，不许写"应该是"）；
3. **修复**：遵守 fix-guard —— 修前读目标行 ±5 行、grep 确认现状、一次只修一条；
4. **验证（三件套，缺一不可）**：
   - docker 单测基线：`cargo test -p minix-kernel -p minix-arch -p minix-vm`（基线 **813 / 242 / 526**，只许增不许减）；
   - rustfmt 零新增漂移：`rustup run nightly rustfmt --edition 2024 --check <改动文件>`，**hunk 数与 `git show HEAD:<file>` 对比持平**（此仓存量漂移很多，判据是"不新增"，不是"全绿"）；
   - **两次独立真机复跑**（签名一致才算修好）；
5. **写报告**：追加到 `NK4C-WORKLOG.md`（§4.3 模板）；
6. **commit**：`git add <明确路径>`（**禁止 `-A`**），message 用 `fix(edge1,nk4c): <编号>——<一句话根因> + 正文要点`。

> **纯诊断轮也要 commit**（用 `diag(edge1,nk4c):` 前缀）：把探针 + 结论固化，下一手才能接。

### 4.2 探针纪律（上一手在此浪费了 3 轮真机，务必遵守）

1. **diagctl 写 >16 字节会被内核静默丢弃**（实测）。所有探针行的**总长度必须 ≤16 字节**（含 `\n`）。格式建议：`nk4a: xx ` + hex。
2. **`line[..N].copy_from_slice(b"...")` 的字面量长度必须与 N 完全相等**——不匹配会 **panic**，而且 panic 位置在探针自己身上，会让你误以为是协议臂出问题（上一手因此在 s17c/s17d 白跑两轮，假 panic 被误读为 `server.rs:285`）。
   - **写探针后必须本地核对**：`python3 -c "print(len(b'nk4a: xx '))"` 之类，或数一遍。
3. **探针要有 cap**（`AtomicUsize` + `< N`），否则串口被刷爆、拖穿复跑 timeout。
4. **探针命名统一前缀 `nk4a:`**，便于 `grep` 与将来 task1-close 批量裁决删除。
5. 探针要标注用途与删除时机（注释写 `NK4C <编号> 取证探针（task1-close 裁决删除）`）。

### 4.3 WORKLOG 报告模板（追加到文件末尾，**不要改历史节**）

```markdown
---

## <编号> <一句话标题>（<日期>，serial_<标签>）

### 现象
<一轮真机的原始观察，带探针输出原文>

### 根因
<一句话根因>。C 锚点：`minix3/minix/kernel/<file>:<行>`。
<为什么 Rust 侧偏离了 C 的语义>

### 修复
<改了哪个文件哪一段，为什么这么改>。方案对比（若有多个候选）：<候选 A/B，选谁，为什么>。

### 验证
- docker：kernel <n> / arch <n> / vm <n>（基线 813/242/526）
- rustfmt：<文件> hunk 数 HEAD=<a> NEW=<b>
- 真机：serial_<x> / serial_<y> 两轮，<签名是否一致>

### 新停点
<下一个卡点是什么，第一手该看什么>
```

### 4.4 顶部「当前状态」必须每次 commit 前更新

`NK4C-WORKLOG.md` 的 `## 当前状态` 段（第 9-55 行附近）是接手者 5 分钟接续的唯一依据。**每次 commit 前必须把它改成最新**（frontier 编号、一句话现象、下一手第一动作）。这是硬性要求，不是可选。

### 4.5 真机验证的环境纪律

- **必须用仓库内 `tmp/nk4a/vars.fd` 的副本**，不能用全新 OVMF VARS（会让 EFI 模块装载落点改变 → 内核 `vm_handoff free n=0` → VM assert panic → 完全不同的死法）；
- **`/tmp/nk4a` 会被清**（WSL 重启等）：复跑前先 `mkdir -p /tmp/nk4a && cp tmp/nk4a/vars.fd /tmp/nk4a/vars_run.fd`；
- **QEMU 命令照 §8 原样跑**，自行加 `-machine q35 -m 512` 会导致 QEMU 启动即退（实测）。

---

## 5. 任务分解（按序推进；每一步 = 一个可提交单元）

> 每一步都设计成「200k 上下文能装下」的规模：一轮探针 + 一次修复 + 两次复跑。
> **做完一步就 commit，然后立刻开始下一步**，不要停下来汇报。

### 单元 B：rc marker 闸门 —— x86_64 半 ✅ 已完成（2026-09-27）

- x86_64 单核两次独立真机复跑均出 `minix-rs rc: minimal boot script marker`（§1.119续-7 基线，HEAD `07c9e6649` 起未回归；每轮修公共内核/服务器代码后仍须双跑 x86 守 marker）。
- 旧 B1（SENDA 全零）及 1.10–1.12 系列候选停点均已入土，勿重翻。

### 单元 B'（当前）：aarch64 rc marker

- 入口 = §3.3（flush_pending_fs 探针判别 Stat-EIO 腿 → 修复 → 逐停点推进至 marker），节奏沿用旧 B3：每个停点一个 commit，卡住就「定性→定位（C 锚点）→修→三件套→报告→commit」。

### 单元 C：1.4 F10 errno 全仓对账（P0-wire 余项）

- **C1｜aarch64 SYSCALL 腿迁移 `reply_wire()`**
  - 现状：x86_64 已用 `reply_wire()`（数据码原样、错误码取负），aarch64/riscv 仍用 `reply_code()` 不取负 → 负 errno ABI 不一致；
  - 位置：`os/kernel/src/trap_dispatch.rs` 的 aarch64 SYSCALL 臂（grep `reply_code`）；
  - 每处判别测试 + commit。
- **C2｜riscv64 同款迁移**（同 C1，另一架构）。
- **C3｜对账收尾**：全仓 grep 确认无残留 `reply_code` 用在 SYSCALL 腿；报告 + commit。

### 单元 D：1.5 P2 命令面（echo/ls/cat）

- **D1**：核对 imgrd 播种（`os/xtask/src/image.rs` 的 `generate_etc_proto`）是否已含 `/bin/sh`、`/bin/echo`、`/bin/ls`、`/bin/cat`；缺则补；
- **D2**：真机跑 `sh /etc/rc`，逐停点修到三条命令都能执行；
- **D3**：smoke 扩展 + 两次复跑 + 报告 + commit。

### 单元 E：1.6 F3 W^X

- boot-shim 传段表（`KernelInfo` 扩展），身份窗口按节拆 RX/RW。**设计先行**（涉及 `[ARCH]` 可能触发停止条件 1）。

### 单元 F：1.7 C 腿 ABI 对账清单（test12 前置①）

- 定稿陷入面 ABI：`rax=src/r10=status/rbx=msg/rcx=callnr`、kerninfo rbx、crt0 handoff（kerninfo 页 + 栈 + 参数）；
- 产出：一份清单文档（C 陷阱桩与 crt0 按此实现）。**commit**。

### 单元 G：阶段 2 aarch64（2.1–2.4）

- 2.1 M3.4 B 案：显式描述符通道（**设计先行，可能触发停止条件 1**）；
- 2.2 内核读用户内存丙案（riscv64/aarch64 共用）：`KernelUserCopy` 改 VA→PA 走 DM 窗口（`ipc.rs` + 跨页分段）——**注意：单元 B 的 senda 修复可能已经部分完成这件事，先查再改**；〔状态更新 续-76a：**已实施**——`copy_msg_from_user`/`copy_msg_to_user` 两腿均改 `copy_via_root_pages`（walk→PA→DM），x86 真机 ×3 marker=2 端到端走新腿，`[ARCH: user-copy-via-dm]` 已标〕
- 2.3 aarch64 生产 U-mode trap 腿（VBAR EL0 接入 `init_protection`）；
- 2.4 VM handoff → **M3.6 aarch64 rc marker**。

### 单元 H：阶段 3 riscv64（3.1–3.4）

- 3.1 甲案：kernel-image riscv64 接 `a1` DTB → 解 memmap + 模块装载源 + `.bss` 清零 → 调 `arch_boot`；
- 3.2 SUM 丙案（= 2.2 的 riscv64 半）；〔状态更新 续-76a：**已实施**（丙案即消灭 SUM 依赖的通道改造，见上）〕
- 3.3 riscv64 生产 U-mode trap 腿（sscratch 交换腿）；
- 3.4 VM handoff → **M4.5 riscv64 rc marker**。

### 单元 I：阶段 4 测试全量上机（4A–4D）

- **4A C 腿基建**（三架构各一套）：LP64 头适配、三架构陷阱桩（按单元 F 的清单）、crt0 接 minix-rt handoff、clang 交叉构建胶水；
- **4B 领域梯子 W1–W13**（x86_64 先行，每波完成即三架构跟随）：W1 PM/VM 基础（test12/test1/test6/test44）→ W2 文件系统（27 项，最大价值区）→ W3 信号 → W4 exec/spawn → W5 pipe/select → W6 mmap → W7 tty → W8 时间 → W9 UDS → W10 网络 → W11 块设备 → W12 SysV IPC → W13 ptrace；
  - 边界：test57/62/47（i386 专属）需决策；**test82（外网）永久排除**；
- **4C Rust 腿核心域 guest 化**：五域优先（进程/信号/凭证/管道 select/目录链接）；
- **4D 每波 × 三架构判据脚本**；宿主 33 测试保持 CI。

### 单元 J：阶段 5 收尾清账

- E5(a)-(g) 真机半点亮 + E-KERNINFO 桩翻转 + NS5-B；E-VMTLB 非 SMP 余件；
- **task1-close 探针大裁决**：删除所有 `nk4a:` 探针（`git grep -n 'nk4a:'` 全清）；
- 全账本销账 + FIXLOG/账本终版。

### 明确不要开工的（保持登记）

E5-SMP / NK6 X-8（SMP 波）；E5(h) + E-DMWIRE + C-17（devman）；C-26（sffs）；C-21/C-22（随批次）；test82。

---

## 6. 交接规范（为「你被收尾后上一手接手」服务）

用户让你收尾时，**你必须让以下三样处于最新状态**（收尾前自查）：

1. **`NK4C-WORKLOG.md` 顶部「当前状态」**：frontier 编号 + 一句话现象 + **下一手第一动作**（具体到命令/探针）；
2. **最后一次 commit** 已包含你所有未提交的改动（`git status --short | grep -v '^??'` 应为空，除 `AI-chats/daily.todo.md` 外）；
3. **每节的「新停点」段**写清了下一手的入口。

**收尾时额外做**：写一份 `NK4C-HANDOFF-<日期>.md`，内容 = 本轮的单元清单（编号 + commit + 一句话结论）+ 当前 frontier 的完整现场 + 下一手的前 3 个动作。这是给上一手 agent 的快速接手件。

---

## 7. 停止条件（重申，只有三种）

见 §1.3。**其余情况一律继续**。

---

## 8. 命令速查（可直接复制；不要自己发明命令）

### 8.1 构建镜像（宿主，docker 缺 uefi target）

```bash
cd /home/xzhao/github/minix-rs/os && ulimit -v 3145728 && cargo run -q -p xtask -- image --arch x86_64 --release
# 产物：target/image/x86_64/minix.img
```

### 8.2 真机复跑（x86_64·xtask 标启动器＝硬编码 -smp4，qemu.rs:67；当前主战字段）

```bash
cd /home/xzhao/github/minix-rs/os && cargo run -q -p xtask -- image --arch x86_64 --release
for i in 1 2 3 4 5 6; do timeout 60 cargo run -q -p xtask -- qemu --arch x86_64 --serial ../tmp/nk4a/<标签>-r$i.serial >/dev/null 2>&1; done
# 判据（串口含 NUL 必须 tr -d）：
for i in 1 2 3 4 5 6; do f=../tmp/nk4a/<标签>-r$i.serial; echo "r$i marker=$(tr -d '\0' < $f | grep -c 'minimal boot script') panic=$(tr -d '\0' < $f | grep -cE 'panic|pagefault')"; done
# 历史对照：修前 2/4 早崩（rip−db＝−0xfc84·CR2=rsp+0x2c0）；续-50 修后 4 轮该崩态零出现，r4 达 birth s3 runtime ok
```

旧单口径（isa-debug-exit -smp 1 手改命令，§1.119续-7 marker 基线守用，非当前主战场）：

```bash
mkdir -p /tmp/nk4a && cp /home/xzhao/github/minix-rs/tmp/nk4a/vars.fd /tmp/nk4a/vars_run.fd
cd /home/xzhao/github/minix-rs/os && timeout 150 qemu-system-x86_64 -smp 1 \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=/tmp/nk4a/vars_run.fd \
  -drive file=target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:/tmp/nk4a/serial_<标签>.log -display none -no-reboot -device isa-debug-exit
# 看结果：grep -n 'rc: minimal\|panic-enter\|vector 13' /tmp/nk4a/serial_<标签>.log
```

### 8.3 docker 单测基线（**优先 docker，不要用宿主 cargo test**）

```bash
cd /home/xzhao/github/minix-rs/os && timeout 580 docker run --rm -v "$PWD:/work" -w /work -m 2g \
  minix-ci:1.94 cargo test -j 1 -p minix-kernel -p minix-arch -p minix-vm 2>&1 | grep -E '^test result'
# 基线：kernel 813 / arch 242 / vm 526，0 failed
```

### 8.4 rustfmt 零新增漂移检查（**必须用 nightly**，宿主 stable 不认 let-chains）

```bash
cd /home/xzhao/github/minix-rs/os
rustup run nightly rustfmt --edition 2024 --check <file> 2>/dev/null | grep -cE '^Diff in'
git show HEAD:os/<file> > /tmp/f_head.rs
rustup run nightly rustfmt --edition 2024 --check /tmp/f_head.rs 2>/dev/null | grep -cE '^Diff in'
# 两个数字相等 = 零新增漂移（此仓存量漂移很多，判据是"不新增"）
```

### 8.5 探针检索与核对

```bash
cd /home/xzhao/github/minix-rs/os && git grep -n 'nk4a:' -- '*.rs' | head -40   # 现有探针
python3 -c "print(len(b'nk4a: xx '))"                                          # 核对字面量长度
```

### 8.6 aarch64 镜像构建 + 真机复跑（当前战场主命令）

```bash
# 构建（必须 --release：debug 触发 dm_coverage.rs debug_assert）
cd /home/xzhao/github/minix-rs/os && cargo run -q -p xtask -- image --arch aarch64 --release
# 跑（xtask 内部按架构选 qemu 与 -smp；日志落 target/image/aarch64/serial.log）
cargo run -q -p xtask -- qemu --arch aarch64
# 判据：grep -n 'rc: minimal\|panic\|DLGRAPH\|vfs2init' os/target/image/aarch64/serial.log
# 基线现场：日志冻结 ≈4295 行 = 死锁（非慢）；每轮跑两次、签名一致才算数
```

---

## 9. 已知陷阱清单（上一手踩过的，别重复）

1. **探针 `copy_from_slice` 长度不匹配 → panic，且 panic 点看起来像协议 bug**（浪费 2 轮真机）。写探针后必核对长度。
2. **diagctl 写 >16 字节被静默丢弃** → 探针行必须 ≤16B。
3. **`/tmp/nk4a` 会被清** → 复跑前重建目录 + 拷 vars.fd。
4. **必须用 `tmp/nk4a/vars.fd` 的副本**，不能用全新 OVMF VARS。
5. **x86_64 `walk_translate` 返回的 pa 已含页内偏移**，不要再叠 offset。
6. **跨地址空间读不能直接解引用用户 VA**：`deliver_async` 跑在接收者陷入里，current CR3 是接收者的 → 必须走「发送者 root 翻译 → PA → DM 窗口」。
7. **`#[repr(C)]` 缺失会静默错位**：Rust 默认 repr 不保证字段顺序。跨内核/用户边界的结构体必须有 `#[repr(C)]` + `offset_of!` 守卫（`WirePrivUpdate` 与 `WireAsyncSlot` 都因此踩过坑）。
8. **改 RTS 标志要用 `rts_set`/`rts_unset`**（含入队/出队半），裸 `p_rts_flags.clear()` 会让进程 runnable 却不入调度队（F10d 家族，已出现三种变体）。
9. **`git add -A` 会带上 `AI-chats/daily.todo.md`** → 用明确路径。
10. **改完必须跑 docker 基线**：宿主 cargo test 可能因 target/feature 组合不同而给出误导性结果。

---

## 10. 每轮真机复跑前的自查（贴在心里）

- [ ] 探针行 ≤16B，字面量长度已核对？
- [ ] 探针有 cap？
- [ ] 改动遵守 fix-guard（读了目标行 ±5 行、grep 确认）？
- [ ] docker 基线跑过（813/242/526，只增不减）？
- [ ] rustfmt hunk 数无新增？
- [ ] 真机跑**两次**（签名一致才算数）？
- [ ] WORKLOG 追加了一节 + 顶部状态更新？
- [ ] commit 用明确路径（没带 daily.todo.md）？

---

## 11. 新会话开场 prompt（复制整块作首条消息 + `/goal`，缺了 goal 注入它不会自续）

> ⚠️ **关键机制**：本任务的「一直迭代」不靠正文语气，靠 goal 系统的跨轮自动续跑（objective 每轮重新注入 + "目标不得缩小"约束）。普通聊天里粘文档，agent 做完一个请求就会结束回合等用户——那不是偷懒，是 harness 行为。所以开场必须：①把下方整块作为首条消息；②按提示确认进入 goal 模式；③turn budget 给足（建议 ≥40；本段只写「探测一轮就停」的会话，多半是目标被当成了单轮任务或没进 goal 模式）。

```text
你是 NK4-C「清零者」永续自主任务 agent。Turn budget=40（全新会话）。
三条终目标（全部满足才算完，不是某个小节点）：
1. 三架构（x86_64/aarch64/riscv64）OS 各自启动并打印 rc marker（串口出现 minix-rs rc: minimal boot script marker）。当前实况（2026-09-29）：x86 标准 xtask 启动器（-smp4）下 marker 未稳（boot DM 栈洞已击穿、新腿＝用户栈缺页）；aarch64 模式① 间歇 OOM 未克；riscv64 未接 IPC 桥。
2. 18-stage 命令面跑通（echo/ls/cat 为核心）。
3. minix3 的 tests/ 上机跑起来。
难/多轮失败/工作量超预期/连续崩都不是收尾理由。只在三种情况停下问用户：架构裁决级决策 / 破坏性操作 / 终目标三条全满足。
硬约束：中文回复；绝不动 AI-chats/daily.todo.md；禁 git add -A / git add .（只 add 明确文件路径）；Ground Truth 优先链：Minix3 C 源 > design doc > Rust 码；每轮验证链：host 测试基线只增不减（minix-kernel 825 / arch+boot+types 309）/ clippy 零新告警 / nightly rustfmt 零新增漂移 / 镜像重建+真机签名一致；含代码更改的 commit 必走 CodeReview；WORKLOG 更新（顶部前沿+文末新节）+ commit；探针纪律：用后即滚、缺页 handler 内严禁页表 walk；code-excellence 全程；镜像构建必须 --release；未坐实不成修、不臆造未验证生产改。
接手入口（先读后干）：rewrite-notes/coordination/NK4C-RESUME-PROMPT.md ——先执行其 §2 开工仪式，再从 §3.1 开始（工作树有一笔未提交的 DM source-4 改动·第一动作是补完它的验证链而非重写它）。**§3.1 只是第一个动作，不是任务边界；任务边界是上面三条终目标**。每轮交付后立即自行开下一轮：定性→定位（C 锚点）→修→验证链→报告→commit→下一个停点；禁止以「完成一个探测轮/取证轮」为交付停止，禁止把目标改写成更小 subset。若用户提「GLM 已修复可 review」：先按交接件 §3/续-48 A 配方核查全 ref，无新 commit 则诚实上报继续真前沿，不编造 review。
立即开始，不要写计划不要询问。
/goal 按需调用 /debug skill，每个含代码更改的 commit 都需调用 /code-review，全程注意 /code-excellence，注意代码质量和品质。
```

---

**开始吧。第一步 = §2 的开工仪式；第一个动作 = §3.1 的 source-4 验证链补全（非重写）；但任务边界 = §0.1 三条终目标，每轮交付后自动续下一轮，不得停在单轮。**
