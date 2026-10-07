# NK4-C 评审线会话记录 + 新机迁移交接（2026-09-30，评审 agent 著）

> **本文件定位**：与 `NK4C-MIGRATION-20260930.md`（续跑 agent 著，主线权威状态）**互补**。
> 主线（NK4C 永续任务本体）的进度/配方/开场 prompt 以该文件为准；本文件记录
> **评审线**（R3 code-excellence 评审）与**主线前半**（同一评审 agent 在 2026-09-24
> 所做的 NK4C 修复工作）的全部会话内容——这些只存在于对话历史中，换机即失。
> **新机恢复顺序**：① `NK4C-MIGRATION-20260930.md`（主线状态）→ ② 本文件（评审线 + 环境搭建 + R3.1 协议）→ ③ `NK4C-WORKLOG.md` 顶部。
> **§7 附新机统一开场 prompt**（已另存 `NK4C-NEW-MACHINE-OPENING-PROMPT.txt`）。

---

## 0. 迁移即刻快照（本文件写出时刻）

- **HEAD** = `f7739a18f`（续-74）；工作树 tracked 全净。
- **评审线最后 commit** = `41d374476`（R3 报告定稿，2026-09-27）。
- ⚠️ **本机 `rewrite` 领先 `origin/rewrite` 2006 笔**——**迁移前必须 `git push`（或整仓库目录拷贝，含 724MB `minix3/` ground truth 子树）**。这是迁移第一动作，不做则本会话全部工作丢失。
- `tmp/nk4a/vars.fd`（累积 UEFI vars，复现刚需）**已被 git 跟踪** ✓（tmp/nk4a/ 下 35 个文件在跟踪中）。
- 本机 `os/.dockercargo/` 卫生问题（R3 登记）**已被续跑方消化**（.gitignore:94 + git rm --cached）——R3 待办①完成。

---

## 1. 会话工作全记录（两段）

### 1.1 第一段：NK4C 主线前半（2026-09-24，449-livelock → 交接）

任务：修复 449-livelock（PAGEFAULT→VM 投递/唤醒死锁）并推进 rc marker。**12 笔实义修复**（全部真机验证 + docker 基线），关键链：

| commit | 内容 | 一句话 |
|--------|------|--------|
| `68d97a732` | **F14** | Phase 3 drain 同步拷贝用户缓冲（C proc.c:1071-1095 对位）——449-livelock 根因 |
| `42676e165` | **F15**（v1 `5c654c959` 引回归后 v2 修订） | 队列唤醒完成码按 SENDING_FROM_KERNEL 门控——九进程 SIGSEGV 根因 |
| `128df8a5b`+`e0fc5f268` | **timer/PIC bring-up** | 8259 ICW 重映射+EOI+quantum 接线——"用户态自旋永久垄断"的总根因，133Hz 实锤 |
| `ea1cbbf9c` | 1.10l | drain 代 SENDREC 发送者停 receive 半（**后被续跑方 B24 依 C proc.c:569-583 结构性取代**——评审确认取代正确） |
| `b0f4ff54c` | **1.10z 三根因连修** | ①drain 停车 getfrom=目的地；②RS `WirePrivUpdate.s_id` i32→u16（repr(C) 错位 4B 剥 SYS_PROC → sched setalarm EPERM panic）；③sendrec 快路径 receive(dst)。+`offset_of!` 守卫范式 |
| `251c8144b` | **1.11d** | drain 同步拷贝漏盖 `m_source`（proc.c:1071-1075）——init getuid 无回执 |
| `839ecbd4b` | **1.12a** | engine wake 单槽 Option 被同 syscall 第二次唤醒覆盖 → 4 槽全量入队 |
| `d6f176451` | **1.12d WIP** | SENDA 真读首实现 + 探针链诊断（**带双缺陷入仓**，见 §1.3 诚实账） |

**交接件**：`NK4C-RESUME-PROMPT.md`（200k 上下文专用，含读法/纪律/任务分解 B1→J/命令速查/陷阱十条）。⚠️ 其 §3 frontier 叙述已过时（主线已至续-74）——**纪律与陷阱部分仍然有效**。

### 1.2 第二段：R3 评审（2026-09-27，commits `b4a02f0b4`/`17a989fa3`/`41d374476`）

按用户指令对「本会话 91 笔 + 续跑 agent 123 笔 = **214 笔**（`3e155ba2a..956e4a57f`）」做 code-excellence 评审，七阶段 P0-P7 全完成。**产物**：

- 报告：`NK4C-REVIEW-REPORT-20260927-R3.md`（含 §一机械对账 / §二自审 / §三增量深审 16 笔判定表 / §四架构专项 5 项 / §五卫生 / §六独立验证 / §七移交）
- 对账表全件：`NK4C-R3-P1-AUDIT-TABLE-20260927.txt`（214 笔逐笔 hash/体量/旗标，P1 脚本输出原件——本会话从 /tmp 抢救入仓）
- 进度账：`.review/zcode/edge1/REVIEW-R3-STATE.md`（**gitignored 本地文件，随本机报废**——内容已折叠进 R3 报告与本文件）

**结论**：无虚构取证、无验证虚报、minix3/ 零改动；续跑方**零 P0/P1**；16 笔高爆炸半径修复深审全 PASS（B39/B41/B42/B48/1.101/1.106/1.113/1.114/1.117 等）。**里程碑独立复证**：docker 820/243/531 全绿 + x86_64 真机 marker×2 panic=0。

**发现汇总（评审欠账，均已登记 R3 §7.3）**：
1. ~~`os/.dockercargo/` 误入库~~ → **续跑方已清**（§0）
2. 1.12a 注释-代码矛盾一行（ipc.rs:1116-1118 "满丢最旧" vs 实为"满丢最新"）——**待修**
3. kernel-image 模块头"交付边界"未更新双架构状态 + aarch64 入口缺 `[ARCH: boot-handoff]` 字面标注——**待修（doc）**
4. B39 分段循环多页离散帧 mock 测试（作者自登记 N3）——**待补**
5. task1-close 死探针（vmpt2bf/sas-send 等）随全量裁决——**按计划**
6. 交接方诚实账（自审发现，非待办）：1.12d WIP 双缺陷（copy 方向互换 = s18 全零真因 + SLOT=80 与断言 96 矛盾 = B7 死锁根因）——**已由续跑方 B7/1.12e 修复**；1.10x 架构偏差——**已由 B24 取代**

### 1.3 方法论教训（两段会话沉淀，全部入仓）

1. 验证通过 ≠ 架构正确——先对照 C 质疑架构再打补丁（1.10x 教训）
2. const 断言必须作为常量来源而非事后证明（1.12d 教训）
3. 探针验证了"检查通过"不等于验证了"读到的字节"——取证要闭环到数据（1.12d 教训）
4. `#[repr(C)]` 缺失/宽度不齐在跨边界结构上静默错位——offset_of! 守卫范式（WirePrivUpdate/WireAsyncSlot 双判例）
5. 环境工具陷阱：本机 grep=ugrep 对含 NUL 串口日志**计数输出被吞**——python 字节级统计或 `tr -d '\000'` 预处理（R3 P6 教训）

---

## 2. R3.1 增量评审协议（新机续作的评审线任务）

- **已审界**：`956e4a57f`（R3 截断面）。R3 定稿时初读过的 2 笔：`19feeb25c`（docs ✓）、`ad9d37429`（SMP pick 修复，初读合格）。
- **R3.1 范围**：`956e4a57f..<迁移时 HEAD>` = **当前已累积 115 笔**（含续-69..续-74 的大步进：AP stop_local_timer、schedctl→BSP 钳、-smp4 marker 稳定、目标②坐实）。
- **方法**（同 R3）：
  1. P1 机械对账——脚本原件在 `NK4C-R3-P1-AUDIT-TABLE-20260927.txt` 头部注释不可考时按下述要点重写：逐笔 `git show --numstat`，对 message 声明（"零代码改动/纯取证/探针回滚/tracked 净"）与 diff 事实对账；保护文件（minix3/、AI-chats/daily.todo.md、target/、.dockercargo）检查；段归属用 `git log --pretty=%h <range>` 建**短哈希集合**（勿用 rev-list 全哈希比对——R3 踩过）；
  2. fix 类深审（code-excellence 四件套：C 锚点/设计对比/非法态封堵/回归测试）——**重点**：续-72（idle AP stop_local_timer）、续-73（schedctl→BSP 钳）为 SMP 域新改动（R3 曾登记 NIT#4 SMP shootdown 债）；续-74 目标②取证链；
  3. 架构专项：aarch64 H7 数据流配方与 riscv64 IPC 桥六步落地时的 [ARCH] 合规；
  4. 独立验证（docker 三件套 + x86_64 -smp4 marker 复跑）。
- **产物**：`NK4C-REVIEW-REPORT-R31-<日期>.md`（沿 R3 格式）；发现滚动登记。
- **评审纪律**：不修正确性 bug（发现→记录交主线）；`git add` 明确路径；评审 agent 不碰主线在制工作树。

---

## 3. 新机环境搭建清单（迁移必读）

| # | 项 | 动作 |
|---|-----|------|
| 1 | **git 迁移** | 本机 push（领先 origin 2006 笔）或整目录拷贝；新机 clone 后确认 `minix3/` 子树在（724MB）且 `git log` 见 `f7739a18f` |
| 2 | **docker 镜像 `minix-ci:1.94`**（2.33GB，**仓内无 Dockerfile**） | 本机导出：`docker save minix-ci:1.94 \| gzip > minix-ci-1.94.tar.gz` → 新机 `docker load`；或重建（debian trixie + rust 1.94 + clippy，`docker history` 可考分层） |
| 3 | nightly rustfmt（宿主 fmt 核验用） | `rustup toolchain install nightly`；核验命令 `rustup run nightly rustfmt --edition 2024 --check <file>` |
| 4 | QEMU + OVMF | qemu-system-x86_64 ≥8.2 + `/usr/share/OVMF/OVMF_CODE_4M.fd`（4M 变体） |
| 5 | vars.fd | 已 git 跟踪 ✓；复跑前 `mkdir -p /tmp/nk4a && cp tmp/nk4a/vars.fd /tmp/nk4a/vars_run.fd`（QEMU 会写副本，勿用本体） |
| 6 | 宿主构建回退 | 无 docker 时 `ulimit -v 3145728` 宿主构建（UEFI 载体两镜像缺 target 本就只能宿主构建） |

## 4. 不会随 git 迁移的本地状态（本机报废即失——已抢救或确认可弃）

| 本地项 | 处置 |
|--------|------|
| `.review/zcode/edge1/REVIEW-R3-STATE.md`（评审进度账） | 内容已折叠进 R3 报告 + 本文件 §1.2/§2 |
| `.review/zcode/edge1/FIXLOG.md`（NK4-A/B 时代取证史） | 关键结论已在 WORKLOG 各节有副本（WORKLOG 头部注明"本地文件会丢"）——确认可弃 |
| `/tmp/nk4c_r3_p1_audit.txt`（P1 对账表） | **已抢救入仓** = `NK4C-R3-P1-AUDIT-TABLE-20260927.txt` |
| agent 记忆目录（`~/.zcode/.../memories/`，含 docker 铁律/fmt 工具链等） | 关键事实已全部入仓（RESUME-PROMPT §8/§9 + MIGRATION-20260930 + 本文件）——确认可弃 |
| `tmp/nk4a/` 其余 untracked（serial 日志/vars 副本/取证 md） | 可弃（vars.fd 本体已跟踪；serial 日志为历史取证原料） |

## 5. 关键文件地图（新机速查）

```
rewrite-notes/
  NK4C-MIGRATION-20260930.md          ← 主线权威交接（续跑 agent 著；§末主线开场 prompt）
  NK4C-REVIEWER-SESSION-RECORD-20260930.md ← 本文件（评审线 + 环境清单 + R3.1 协议）
  NK4C-NEW-MACHINE-OPENING-PROMPT.txt ← 新机统一开场 prompt（用户另存文本文件用）
  NK4C-WORKLOG.md                     ← 永续记忆载体（5400+ 行；顶部状态始终最新）
  NK4C-RESUME-PROMPT.md               ← 200k 上下文专用入口（§3 过时；纪律/陷阱/命令速查仍有效）
  NK4C-REVIEW-REPORT-20260927-R3.md   ← R3 评审报告（定稿）
  NK4C-R3-P1-AUDIT-TABLE-20260927.txt ← R3 P1 对账表全件（214 笔）
  NK4C-REVIEW-REPORT-20260923.md/-R2.md ← 前两轮评审（格式先例）
  NK4C-OPENING-PROMPT.md              ← 任务原始指令（§6 路线图/§9 停止条件仍权威）
  00-master-plan..19-stage-integration/ ← 18-stage 重写文档树
.review/zcode/edge{1,2,3,4}/          ← gitignored 本地工作状态（随机器失，关键内容已入仓）
```

## 6. 停止条件（重申，两线通用）

只有三类情况停下问用户：①架构裁决级（外部契约变更 / `[ARCH]` 三处一致设计决策）；②破坏性操作；③终目标达成（三架构 marker + 命令面 + tests 上机）。其余一律继续并保持 WORKLOG 可接手。

---

## 7. 新机统一开场 prompt

（见 `NK4C-NEW-MACHINE-OPENING-PROMPT.txt`；同时复制于下方，直接粘进 goal 模式即可）

```text
读三份文件按序恢复工作：
1. rewrite-notes/coordination/NK4C-MIGRATION-20260930.md（主线权威状态：三终目标进度/两大前沿配方/主线开场 prompt）
2. rewrite-notes/coordination/NK4C-REVIEWER-SESSION-RECORD-20260930.md（评审线 R3 状态、R3.1 增量协议、新机环境搭建清单）
3. rewrite-notes/coordination/NK4C-WORKLOG.md 顶部「当前状态」

背景：minix-rs（Minix3 内核 Rust 重写）NK4-C 长程任务跨机器迁移续跑。三条终目标：
①三架构各自 boot 打印 rc marker（x86_64 已达成 -smp4 稳定；aarch64 卡 H7 数据流间歇 OOM；
  riscv64 未接 IPC 桥）②18-stage 命令面（x86 echo/ls/cat 已走真 VFS IPC 达成；aarch64/
  riscv64 待 boot 后统一推）③minix3 tests/ 上机（未启动，依赖①②）。

两条工作线，先主线后评审线穿插：
【主线】按 MIGRATION-20260930 的配方推进：aarch64 H7 数据流（间歇 ~4GiB OOM 毒 String.len
  取证）与 riscv64 IPC 桥六步接线。每逻辑单元=一次 commit（message 带 C 锚点与验证三件套
  声明）+ NK4C-WORKLOG.md 追加一节 + 顶部状态同步。
【评审线】R3.1 增量评审：范围 = git rev-list --count 956e4a57f..HEAD（当前 115 笔），方法
  与 P1 对账脚本要点见 REVIEWER-SESSION-RECORD §2，产物 NK4C-REVIEW-REPORT-R31-<日期>.md。
  在主线到达自然停点时插入执行，勿与主线在制工作树互相踩踏。

硬纪律（违反即返工）：绝不修改 AI-chats/daily.todo.md；git add 用明确路径禁 -A；
docker 三件套基线只增不减（kernel 820+ / arch 243 / vm 531+，docker run --rm -v "$PWD:/work"
-w /work -m 2g minix-ci:1.94 cargo test -j 1）；rustfmt 核验用 nightly --edition 2024、
判据是与 HEAD 对比零新增 hunk；真机复跑用 tmp/nk4a/vars.fd 副本（QEMU 写副本勿用本体）；
探针 ≤16B 且 line[..N].copy_from_slice 字面量长度逐一核对；改 RTS 标志必走 rts_set/rts_unset；
跨地址空间访问必走「目标 root 翻译→PA→Direct Map 窗口」绝不直接解引用用户 VA；
只在架构裁决级/破坏性操作/终目标达成三类情况停下问用户，其余情况换方向继续。
开工先做环境自检（REVIEWER-SESSION-RECORD §3 清单：docker 镜像在位、nightly 在位、
OVMF 4M 在位、minix3/ 子树在位），任何一项缺失先报告再继续。
```
