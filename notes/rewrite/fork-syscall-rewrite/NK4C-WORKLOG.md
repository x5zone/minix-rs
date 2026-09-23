# NK4-C WORKLOG（Task C「清零者」追捕 → 三架构 + 命令面 + 测试上机）

> 本文件是**记忆与交接载体**（git-tracked）。长程任务：每完成一个逻辑单元就更新顶部"当前状态" + 追加一节 + commit。
> **顶部状态必须始终是最新的**——用户会在任意时刻让 agent 收尾，接手者只读它 + `git log --oneline -20` 就要能接续。
> 详细取证历史见 `.review/zcode/edge1/FIXLOG.md` 迭代 27-33（**本地文件、被 gitignore、换工作树会丢**——关键结论在本文件 §交接来源有副本）。

---

## 当前状态（每次 commit 前更新，一屏读完）

- **阶段**：S0 待开始（环境自检 + 崩溃复现）→ 之后按 §路线图 推进（不逐项汇报）
- **已修复**（commit）：
  - `1d25f433e` AP 入口补 EFER.NXE（bit11）+ BSP `enable()` 显式置位 —— err=8 保留位风暴 20+ → 0
  - `967a903e7` 摘除金丝雀探针（它在污染生产上下文）
  - `85a0d7cd8` 四张检测网（全部零命中，见排除账）
- **已排除**（不要再重复排查）：DM 覆盖 / VM-内核树不一致 / 跨空间拷贝 `copy·memset·write` / IPC 消息投递 `copy_msg_to_user` / 分配器双重分配·归还·底层重用 / EFER.NXE / gdb 硬件观察点路线
- **下一步**：S0 构建镜像 + `-smp 1` 复现崩溃，记录本轮布局基准（root / `lvl1pa`）
- **阻塞/风险**：无阻塞；风险 = 探针采样饥饿（cap 被启动期重复事件吃光，见 prompt 铁律 2）与布局每轮漂移（禁止跨轮硬编码物理地址）

---

## 路线图（精简；完整版见 `NK4C-OPENING-PROMPT.md` §6）

| 阶段 | 内容 | 判据 |
|------|------|------|
| 1.2 | Task C：RS 越过 step2 → init_fresh → RS main | `-smp 1` 两次复跑都出现 `rs-epslot self=0x7fffffffc800`（活值） |
| **1.3** | **rc marker 链**（sh 域最小版进 imgrd，`init` exec `/bin/sh`） | 两次复跑出现 `minix-rs rc: minimal boot script marker` ← **x86_64 翻绿闸门** |
| 1.4 | F10 errno 全仓对账（P0-wire） | 每处判别测试 |
| 1.5 | P2 命令面（echo/ls/cat）+ smoke 扩展 | 两次复跑 |
| 1.6 | F3 W^X（boot-shim 段表 → 身份窗口 RX/RW 拆分） | 宿主测试 + 真机 |
| 1.7 | C 腿 ABI 对账清单（test12 前置） | 清单定稿 |
| 2.1-2.4 | aarch64：M3.4 B 案 → KernelUserCopy 丙案 → U-mode trap 腿 → **M3.6 aarch64 rc marker** | 同 1.3 判据（aarch64） |
| 3.1-3.4 | riscv64：甲案（kernel-image 接 DTB）→ SUM 丙案 → trap 腿 → **M4.5 riscv64 rc marker** | 同 1.3 判据（riscv64） |
| 4A-4D | 测试上机：C 腿基建（LP64 头/陷阱桩/crt0/clang 胶水）→ 领域梯子 W1-W13 → Rust 腿核心域 guest 化 | 每波 × 三架构脚本 |
| 5 | 收尾：E5 真机半点亮 / **task1-close 探针大裁决（删所有 `nk4a:` 探针）** / 全账本销账 | 终目标三条全绿 |

**保持登记不开工**：E5-SMP/NK6 X-8、E5(h)+E-DMWIRE+C-17、C-26、C-21/C-22、test82（外网）。

---

## 交接来源（上一个 agent，2026-09-23，起点 commit 56d6dec4c）

### 根因骨架（当前最强解释）

1. **PTE 在物理层消失**：内核侧 pf 层级 dump 显示故障时 `lvl1=0`（PTE 不存在），而 `lvl2`（PD 项）稳定——不是"没填过"，是**填过又被抹**。
   - ⚠️ dump 语义：`lvl2pa` 是 **PD 页**，`lvl1pa` 才是 **PT 页**（早前看错一级白跑一轮）。
2. **抹写窗口 = RS 停车→唤醒之间**（此时只有内核在跑，VM 在 receive 上睡着）。
3. **统一解释**：RS 的 asynsend 表就在 `self` 指针同一 VA（`0x7fffffffc800`，栈上，也是栈 PT 覆盖的最后一页）。**栈页被抹 → 从栈重装 self 得 0 → `endpoint_slot(self=0)` → 访问 VA 0 → SIGSEGV**。这把"PTE 消失"与"self=0"合成一条链。
4. 交付时序实证（`serial_c31c`，原文）：
   ```
   nk4a: pre-restore- rip=0x0000000000203bf0 rsp=0x00007fffffff9d88 r10s=0x0000000000007bff rbx=0x0000000000000000
   nk4a: i33-save rip=0x0000000000203c2d rbx=0x00007fffffff9d28 rsp=0x00007fffffff9d18
   nk4a: rs-epslot self=0x0 bep=0x0 slot=0
   nk4a: pf-exit noaddr cr2=0x0
   cause_sig: sig manager 2 gets lethal signal 11 for itself
   ```

### 布局观测（每轮漂移，仅供理解量级，**不可硬编码**）

| 量 | 观测值（多轮） |
|----|----------------|
| RS 页表根 | `0x35fd000` / `0x5e0f000`（两种形态交替） |
| RS 文本 PT 页 | `0x1c08000` / `0x1c07000`（-smp 1 内也漂） |
| RS 栈 PT 页 | `0x1c05000` / `0x1c04000` |
| self / asynsend 表 VA | `0x7fffffffc800`（用户 VA，跨轮稳定） |
| asynsend 首指令 | `0x203bf0`（用户 VA，跨轮稳定） |
| `endpoint_slot` | `0x216900`（用户 VA，跨轮稳定） |

**用户态 VA 跨轮稳定；物理地址每轮变**——对账必须用同一轮日志。

### 探针存量（都在 `#[cfg(not(feature = "mock"))]` 门下，task1-close 统一裁决删除）

| 探针 | 位置 | 作用 |
|------|------|------|
| `dm-cov` / `dm-mem` / `dm-bump` / `dm-mod` | `os/kernel/src/dm_coverage.rs` | VM DM 窗口三源候选 |
| `pf-save` / `pf#`（含 `cr3=` / `lvl4..lvl1` / `lvl1pa`） | `os/kernel/src/trap_dispatch.rs` | 故障现场 + 层级 dump |
| `i33-save` | `os/kernel/src/trap_dispatch.rs` | int-33 入口保存点 |
| `kdst` | `os/kernel/src/vm.rs` | cross_space 三个写核心的目标 PA |
| `msgw` | `os/kernel/src/ipc.rs` | `copy_msg_to_user` 的 (va, root, pa) |
| `sas-send` | `os/servers/vm/src/vm_server.rs` | SetAddrSpace 发送值 |
| `ptalloc-DUP` / `alloc-reuse-PT` / `ptfree-PT` + `PT_SEEN` 位图 | `os/servers/vm/src/alloc_page.rs` | 分配器三侧检测 |
| `vmpt2bf` / `pte-wb-FAIL` | `os/servers/vm/src/cow_exec_pf.rs` | VM 填页 + 写后回读 |
| `rs-step2` / `rs-epslot` / `rs-anom` | RS 自身（`os/servers/rs/src/`） | RS 侧 self/slot 值 |

### 顺带发现的功能缺口（**记录，不要现在修**）

`os/kernel/src/ipc.rs` 的 `impl UserCopy for KernelUserCopy` 中：
- `read_senda_entry` 恒返回 `Err(CopyError::PageFault)`（≈506 行）
- `write_senda_result` 是空操作（≈515 行）

即**生产路径下 SENDA（asynsend）不投递任何消息**。RS 的 asynsend 正是崩溃点的调用。若 Task C 修复后 rc marker 仍不通，这是首要功能缺口候选。

### 已证伪的旧假设（省得重走）

- 「PT 页不在 VM 分配清单」→ 探针 cap-64 采样伪影（每进程 `map_kernel` 消耗 ~640 PT 页）
- 「金丝雀证明保存后被内核改写」→ 金丝雀自己就是污染源，已摘除
- 「gdb 观察点能抓写入者」→ QEMU gdbstub 对目标 VA 触发不可靠

---

## 记录（按时间顺序追加；每节模板见下）

<!-- 追加示例：
## 1.2-<n> <标题>（<日期>，commit <hash>）
### 目标
### 做法（可复制的命令）
### 原始数据（串口片段 / 探针输出原文）
### 结论
### 下一步
-->
