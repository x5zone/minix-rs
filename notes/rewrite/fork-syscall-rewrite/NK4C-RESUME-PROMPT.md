# NK4-C 续跑交接 prompt（2026-09-24 交接；接手者上下文 200k，务必按本文件的读法与节奏）

> 本文件是**接手者的唯一入口**。上一手 agent 已把状态固化在 git 与 `NK4C-WORKLOG.md`。
> 你的上下文只有 200k —— **不要通读 1400 行的 WORKLOG**，按 §2 的读法读。

---

## 0. 你的身份、任务与终目标

你是 **NK4-C「清零者」长程自主任务的接手 agent**。任务是把 minix-rs（Minix3 内核 Rust 重写，x86_64/aarch64/riscv64 no_std）从「boot 硬 livelock」推进到**三架构跑通 OS 并跑通命令面与 minix3 测试套件**。

**终目标（§0.1 三条，全部达成才算完）**：
1. 三架构 OS 各自启动并打印 rc marker（`minix-rs rc: minimal boot script marker`）；
2. 18-stage 命令面在 OS 上跑通（echo/ls/cat 为核心）；
3. minix3 的 `tests/` 在机器上跑起来。

**当前所处位置**：阶段 1.3 的 rc marker 闸门（x86_64 翻绿），即 **单元 B**。之后再按 §6 推进单元 C-K（aarch64/riscv 迁移 → 命令面 → W^X → ABI 清单 → 三架构 marker → 测试上机 → 收尾清账）。

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

# ② 读 WORKLOG 的「当前状态」段（约 50 行，一屏读完）——这是权威状态
sed -n '1,60p' notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md

# ③ 读 WORKLOG 最后 2 节（上一手的最后结论与下一步配方）
tail -120 notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md

# ④ 只读你当前要做的那个单元的路线图行（不要通读整个 §6）
sed -n '/^## 6/,/^## 7/p' notes/rewrite/fork-syscall-rewrite/NK4C-OPENING-PROMPT.md | head -25
```

**读完必须能回答**：当前 frontier 是哪个编号？上一步的结论是什么？下一轮的第一个动作是什么？
若答不出，**只补读 WORKLOG 中相关的那一节**（用 grep 定位节号，不要整篇读）。

**不要读**：`NK4C-WORKLOG.md` 全文、`NK4C-OPENING-PROMPT.md` 全文、`.review/` 下的历史、`migrate_notes_plan/`。它们在需要时用 grep 精确定位即可。

---

## 3. 当前精确 frontier（2026-09-24 交接点，commit `d6f176451`）

### 3.1 已完成并真机验证的修复（单元 A 及 1.10–1.12 系列）

boot 已从「449 轮缺页后硬 livelock」推进到「**12 服务全部出生 + PM↔VFS barrier 通过 + init 用户态跑通前两个 PM 调用**」。关键修复（都已在 git 里，勿重做）：

| commit | 内容 |
|--------|------|
| `a470a8d9c`/`6be40748f` | Task C 根因：int33 陷阱腿恢复 C 门纪律 |
| F11 `d9fc1f649` | VM 请求链双入链自环死锁 |
| F12/F13 | minix-rt slab OOM；PM↔VFS 握手改阻塞 send |
| F14/F15 | Phase 3 drain 同步拷贝；队列唤醒完成码按 FROM_KERNEL 门控 |
| `b0f4ff54c` | **1.10z 三根因**：①drain 停车 `getfrom=目的地`（原 ANY 致伪 reply）；②RS `WirePrivUpdate.s_id` i32→u16（repr(C) 头部错位 4B 使 s_flags 恒 0 → 全服务 SYS_PROC 被剥 → sched setalarm EPERM panic）；③sendrec 快路径 receive 半 `ANY→目的地`（原致 ENOSYS ping-pong） |
| `251c8144b` | 1.11d：drain 同步拷贝漏盖 `m_source`（C proc.c:1071-1075） |
| `839ecbd4b` | 1.12a：engine 唤醒记录单槽 `Option` 被同 syscall 第二次唤醒覆盖 → 改 4 槽全量入队（F10d 家族新形态） |
| `d6f176451` | 1.12d：SENDA 真读实现 + 探针链诊断闭合（**当前 frontier**） |

### 3.2 当前停点（1.12d）：SENDA 表读回全零

**现象**：`init` 的第一个 VFS 请求卡住；VFS 在 boot 期 `receive(RS)` 等 `RS_INIT`，而 RS 的 `RS_INIT` 经 `asynsend→senda` 全部丢失。

**已闭合的诊断链**（探针全部在仓，`git grep 'nk4a: sa'` 可查）：

| 探针 | 结论 |
|------|------|
| `sa-in` | senda **被调用**（`c=0x2 n=0x1`，RS，count=1） |
| `sa-out` | 四条早退门（e1-idx/e2-priv/e3-nosys/e4-clr）**都没走** |
| `sa-readfail` | 表读**没有失败**（`Err` 臂零输出） |
| `saent` | 循环体**从未执行到** dst 解析点 |
| `apend` | VFS 的 `s_asyn_pending` 位**从未被设置** |

**推论（已闭合）**：`senda` 入口门全过、表读成功返回，但 `flags` 解出 **0 = `AMF_EMPTY`** → `continue` → 循环结束 → `done` 仍 true → 返回 `Delivered`，**一条消息都没投**。
⇒ **`copy_via_root_pages` 读回了全零内容**（不是读失败，是读到了零）。

**上一手已实现的部分（方向正确，真机已证实安全）**：
- `KernelUserCopy::read_senda_entry`/`write_senda_result` 从**永久 stub**（恒 `PageFault`）改为真实现；
- 关键设计：**必须走「发送者 root 翻译 → 物理地址 → Direct Map 窗口」**，绝不直接解引用用户 VA —— 因为 `deliver_async` 跑在**接收者的 receive 陷入**里，current CR3 是接收者的，同一 VA 会落到错误地址空间（这就是上一手首次实现时 GP fault vector 13 的根因，s17t 实锤；改走 DM 窗口后 s18d **零 GP fault**）；
- 新辅助函数 `copy_via_root_pages<D>`（`os/kernel/src/ipc.rs`）：逐页 `CurrentPteWalk::walk(root, va)` → `pa` → `D::kernel_phys_to_virt(pa)` → `copy_nonoverlapping`；
- ⚠️ **已知陷阱**：x86_64 的 `walk_translate` 返回的 `pa` **已经包含页内偏移**（`pte & ADDR_MASK | vaddr & 0xFFF`，见 `arch/src/x86_64/paging.rs:319-334`）——再叠一次 `page_off` 会错位（首版踩过，已修）；
- `UserCopy` trait 的这两个方法已加 `root: minix_types::PhysBytes` 参数，全部实现点已同步；
- `minix-sys` 的 `AsyncSlot` 已补 `#[repr(C)]`，配 `WireAsyncSlot` 镜像 + `offset_of!` 守卫。

**你的第一个动作**（§5 的 B1）：定位「读回全零」。

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

### 单元 B：rc marker 闸门（x86_64 翻绿）

- **B1｜定位「SENDA 表读回全零」**（诊断轮）
  - 在 `copy_via_root_pages` 内加三联探针（≤16B）：`walk` 出的 `pa`、DM 窗口 VA、读回首 4 字节；
  - 优先核对：①`self.procs[caller_idx].p_seg.phys_root` 是否等于活 root（对当前进程应与 `crate::current_root_phys()` 一致）；②DM 窗口是否真映射了该物理页（读回零是"映射到零页"还是"窗口偏移错"）；
  - 产出：根因 + C 锚点。**commit（diag 前缀）**。

- **B2｜修复并验证 RS_INIT 送达 VFS**
  - 修 B1 的根因；
  - 真机验证：VFS 侧应出现 `RS_INIT` 的接收（用现有 `vfm` 探针，`m_type=0x714`）；
  - 验证三件套 + 两次复跑。**commit**。

- **B3…Bn｜逐停点推进（循环）**
  - 每次卡住 → 按「定性 → 定位（C 锚点）→ 修 → 三件套验证 → 报告 → commit」走一遍；
  - 已知的下一批候选停点（上一手登记，未必按序）：
    - `1.11e`：`MinixSchedCtl::taskcall` 的 ELOCKED 重试读的是 **reply 语义**（`rv == 208`），而内核 deadlock 检查以 **syscall 错误**返回（走 `Err(_) => -EIO` 臂）——重试从未生效；
    - `1.11a`：sched 的 `do_start` 里 `SYS_SCHEDCTL` 对 fork 子进程报 EPERM（`p_scheduler` 字段时序缺位，对照 C `sched_init_proc`）；
    - `P1-ipc`：`clear_ipc_refs`（`syscall.rs`）裸 `p_rts_flags.clear(SENDING|RECEIVING)` 绕过 C `RTS_UNSET` 的入队半（runnable 却不入队）；
    - `P1-trace`：`do_trace` 裸 set/clear 绕过 rts_set/rts_unset；
    - `P2-diag`：`dispatch_diagctl` 内核栈→PA 走 `kern_phys_base + (va - kern_virt_base)` 的隐式假定。
  - **每个停点一个 commit**。

- **B-final｜rc marker 达成**
  - 判据：**两次独立真机复跑**串口都出现 `minix-rs rc: minimal boot script marker`；
  - 达成后更新 WORKLOG 顶部状态为「单元 B 完成」，**commit**，然后进入单元 C。

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
- 2.2 内核读用户内存丙案（riscv64/aarch64 共用）：`KernelUserCopy` 改 VA→PA 走 DM 窗口（`ipc.rs` + 跨页分段）——**注意：单元 B 的 senda 修复可能已经部分完成这件事，先查再改**；
- 2.3 aarch64 生产 U-mode trap 腿（VBAR EL0 接入 `init_protection`）；
- 2.4 VM handoff → **M3.6 aarch64 rc marker**。

### 单元 H：阶段 3 riscv64（3.1–3.4）

- 3.1 甲案：kernel-image riscv64 接 `a1` DTB → 解 memmap + 模块装载源 + `.bss` 清零 → 调 `arch_boot`；
- 3.2 SUM 丙案（= 2.2 的 riscv64 半）；
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

### 8.2 真机复跑（x86_64）

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

**开始吧。第一步 = §2 的开工仪式，然后 §5 的 B1。**
