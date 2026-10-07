# NK4-C 接续 PROMPT（20261006r）——交给 GLM 的完整接手入口

> **创建**: 2026-10-06（本文件由上一会话在 P-ALL-08 T1 全收＋T2 前四站落地之后撰写）
> **接手者**：GLM。上一会话的最后一笔是 `3ca5fd8ca`（NK4C 续-422 补记）。工作树干净（跟踪文件零改动），无 qemu 残留进程。

---

## 1. 你在接什么

大目标是把 Minix3 的语义重写到 Rust（`os/` 下 `#![no_std]` 的内核、服务器、驱动、文件系统、网络栈），并让 x86-64／aarch64／riscv64 三条架构线在真机门上齐平。本任务线（NK4-C）此刻的具体前沿是**三架构齐平台账**里的 `P-ALL-08`：SEF（System Event Framework，服务事件框架）尚未建模的语义清单，拆成 T1 到 T7 七个小项。

**已完成**：T1（信号请求的双形态解析）全收；T2（各服务接住信号接得多深）已开四站——IS、VM、uds／lwip、`minix-fs-rt` 家族。
**未完成**：T2 收尾半格、T3（`process_init` 的六段动作落点）、T4（四类拦截，待人裁决）、T5（三套 trait 收敛，交代码卓越度）、T6／T7 已结、`P-ALL-12`（消息联合体超尺寸，已定案未修）。

接手第一件事：读 `rewrite-notes/coordination/NK4C-WORKLOG.md` 的顶部状态块（第 13 行起，最新前沿＝§续-422），再读 §续-411 到 §续-422 十二节正文；然后按第 6 节的队列动手。

## 2. 接手必读三件

| 件 | 路径 | 读什么 |
|---|---|---|
| 工作日志 | `rewrite-notes/coordination/NK4C-WORKLOG.md` | 顶部「当前状态」块＝最新态；§续-411..422＝本任务线的全部取证与踩坑 |
| 平台账 | `rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md` | `P-ALL-08` 条目的「落地进度」行（T1/T2 现在到底做到哪一步、通电前置红线）、`P-ALL-12`、`P-ALL-03`、`P-X86-01`、`P-ALL-02`、`P-A64-03` |
| 设计文档 | `rewrite-notes/03-stage-rs/20-rs-sef-framework.md` | §3.5（两条臂与一条唤醒兜底）、§3.6 的 T1–T7 表、§5（测试计数与待补测试） |

## 3. 增量对照表（一节 WORKLOG ＝ 一次提交 ＝ 一次自评审）

| 节 | 提交 | 内容 |
|---|---|---|
| §续-411 | `29d43e72f` | minix-vm 宿主测试面预存断链收口（移除类改动必须跑 `cargo check --workspace --tests` 这条纪律的来源） |
| §续-412 | `b2d2c68b4` | T1 第一笔：通知载荷按 C `sigset_t` 建为 16 字节、内核信号族常量 71..=74、管理器形载荷与集中化访问器、接手件三处事实纠偏、台账入库 |
| §续-413 | `e3262084b` | 前两笔的评审回执（假陈述、错锚、死面、工具假红、clippy 对账七件事） |
| §续-414 | `4af4527cd` | T1 第二笔：`sef_receive_status` 两条臂（notify 升序展开＋管理器形拦截吞消息）＋五件宿主测试＋riscv ATF 36/36 |
| §续-415 | `475450052` | T1 收口：RS 发方补号、消费方盘点表（9 个 crate／13 站点）、文档与代码同步 |
| §续-416 | `98ec6f4a3` | 评审回执：否证「联合体尾部未初始化」的指控，但量出真缺陷并新立 `P-ALL-12` |
| §续-417 | `9eb56a892` | T2 第一站：IS 接线（`receive` 多带一个信号回调、退出决定只能在回调现场做、判定收成一份） |
| §续-418 | `ccadcdd62` | 评审回执：恒真断言换成有鉴别力的帧；把管理器形臂的死锁雷当场登记在代码与台账 |
| §续-419 | `78b277a0d` | T2 第二站：VM 按号分派（唯一换算点）＋私有 `SIGKMEM` 上收 |
| §续-420 | `2ba6887ee` + `47847c9af` | T2 第三站：uds／lwip 按号终止＋uds 首批主循环测试＋撤掉我自己加错的封堵；补记做文档同步与错字回改 |
| §续-421 | `c2152f0f1` | 评审回执：两处「C 同形」假陈述改成「本树形状」；夹具命名歧义拉平；不过早抽象的三条依据入档 |
| §续-422 | `dbdc3f3f5` + `3ca5fd8ca` | T2 第四站：`minix-fs-rt` 家族形状收敛（回调穿过包装层、钩子答案三态、刷盘交任务循环）＋抓到「C 里没有 FS 驱动注册信号处理函数」这句假陈述；补记清两条评审低危 |

## 4. 不可协商的纪律（前八条来自交接件，后三条是这两轮新增的）

1. **一小步＝一次提交＝一次对该提交的回归评审＝一节 WORKLOG**。评审发现的问题要么当场清，要么登记进台账并写明「未做的理由」。
2. **动手前先读现场**：改任何一行前读它上下各五行，并 `grep` 这个符号的全部使用点（防「改一处漏三处」）。
3. **两份案卷绝不触碰**：`NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md`、`NK4C-BUG-TRANSIENT-PTE.md`，以及 `misc_concepts.md`——它们由另一个文档会话在制。
4. **纸上的数字必须机器复核**：行号锚、测试计数、尺寸断言、门读数，凡是写进文档或提交信息的，都要有一条可复跑的命令撑着（本任务线已三次靠这个抓出自己的错：`sef.c:222-226` 错锚、`1u64<<74` 与 64 位载荷相抵、clippy 行位移归因）。
5. **拓扑真值＝设备树**：QEMU 点火参数与 `dumpdtb` 的 `-smp` 必须同值，否则测的是「假多核」（§续-407 的门内注释）。
6. **测试必须可达**：宿主测试要真编译真跑（`#[test]` 漏写、`stop_after: 0` 之类让断言恒真的形状都算未完成）。新写一把测试要做**负控**：临时改坏实现，确认它变红。
7. **qemu 收尾清零**：每笔结束跑 `ps -eo comm | grep -c '^qemu-system'`，必须为 0。
8. **需要人裁决的不代决**：`P-ALL-03`（真多核撤钳）、`P-X86-01`、`P-ALL-02`、`P-A64-03`、`P-ALL-08-T4`（四类拦截是否纳入本轮）。收口时一次性呈报清单。
9. **「同形／等价」是事实断言**：宣称「与 C 同形」必须逐句对照 C 的**控制流**（谁在哪个调用帧返回什么），只看处理函数的语义会得出方向相反的结论（§续-421 的教训）。
10. **检查命令的退出码要看**：不能凭「我没碰那一块」推定格式或告警干净；跑完检查再写结论（§续-422 补记的教训：提交信息里的「零漂移」当时是假的）。
11. **回改会生成新错字**：中文里的形近噪声（锈／锰／锛／锢／癕／障得／框析／形弌）要在**回改之后**再跑一次可疑字表扫描，不能只改不查。

## 5. 机制地图（P-ALL-08 当前的真实形状）

- **两条臂都在库里**（`os/libs/minix-sef/src/lib.rs`）：
  - 通知形：`m_source == SYSTEM` 时走 `dispatch_kernel_notify`（把 `sef_signal.c:94-113` 的升序逐位遍历原样搬出来），位图里 71..=74 每一位命中回调一次；**窗口零命中时以 `SEF_SIGNAL_REQUEST_TYPE` 兜底回调一次**——这是有意偏离 C 的行为保持线，因为内核的 `SigSet` 仍是 64 位（`os/kernel/src/syscall_signal.rs:88-94` 自己登记的限制），位 70..=73 生产不出来，而 VM 的唤醒裁决与 uds／lwip 的终止闩都靠这一次回调驱动。
  - 管理器形：`m_type == SIGS_SIGNAL_RECEIVED && source.get() < INIT_PROC_NR`（判据方向是 `<`，真源 `minix3/minix/include/minix/sef.h:265`；交接件写的 `>=` 是错的，已在 §续-412 纠偏），把载荷里的 `num` 交给回调后 `continue` 吞掉，不上浮事件。
- **通电前置（红线，未定计前不得接发方链）**：C 的处理函数要退出时直接 `exit(0)` 或调 `sef_cancel()`（`sef.c:161-162` 是逃生门），永不回到那个 `continue`；本树缺这道逃生门，所以「服务把退出决定等在这次收信之后」的形状一旦通电就会死锁。定计落在 `20-rs-sef-framework.md` §5.2 第 5 项（返 `EINTR` 还是返一个事件变体）。
- **服务侧的形状**（两种，都已对齐 C）：IS／`minix-fs-rt` 走「回调穿过包装层直达库」（`receive` 多一个 `&mut dyn FnMut(i32)` 参数）；VM／uds／lwip 在闭包里自己判定。判定按各服务自己的 C 实体，**不要上提成公共谓词**（§续-421 登记了三条理由）。
- **文件服务族的三态**：`SignalAction{Ignore, Terminate, SyncThenTerminate}`；刷盘由任务循环代做（`Incoming::SyncThenCancelled` → `driver.synchronized()`），因为只有那一层持有挂载态。
- **未建模**：`SIGKSIG`／`SIGKSIGSM` 的信号管理器代理循环（`sef_signal.c:27-83` 三段 `sys_getksig`／`sys_endksig` 循环）；PM 侧管理器形的发送臂（只差 `asynsend3` 的重试表，登记在 `os/servers/pm/src/signal.rs:314-330`）。

## 6. 按序待办队列（每一步都按第 4 节的纪律成笔）

**第一步：T2 收尾半格（建议 1–2 笔）**
- 站点名单与闭包形态见 §续-415 ③ 的盘点表。本轮新增的确证事实：`servers/sched`、`servers/vfs`、`servers/mib`、`servers/devman`、`servers/pm` 在 C 里**一处都没注册**信号处理函数，库默认值就是空函数 `sef_cb_signal_handler_null`（`sef.h:287` ＋ `sef_signal.c:156-158`），所以这些服务的空闭包是忠实的——把它们各自的注释改成带锚的明证即可（一笔纯注释＋文档，零生产语义，可不跑真机门）。
- `servers/ipc` 是唯一注册的（`minix3/minix/servers/ipc/main.c:132`，实体 `:101`），要单独对照它的 C 实体决定接不接。
- **`devman` 是断链，不是空闭包问题**：`os/servers/devman/src/ipc/minix.rs` 给库传空闭包，而它自有的 `SefHooks::on_signal`（`hooks.rs` 声明与默认实现）从不被回调。C 里 devman 未注册处理函数 ⇒ 这个自有钩子要么是死面要么得按 C 重新定位。**接或删属语义取舍，按纪律第 8 条停下呈人**，别自己定。
- 顺带可清：`os/servers/sched`、`os/servers/vfs` 里两处 `wait_for_init` 一类的握手细节（若与 C 的 `sef_startup` 有出入，先读 `minix3/minix/lib/libsys/sef.c` 再定）。

**第二步：T3 `process_init` 六段落点（先勘察后动手）**
- C 真源：`minix3/minix/lib/libsys/sef_init.c:43-130` 六段——撤 IPC 过滤、0 号进程授权约定、调试标志短路、按类型分派、回报、卸载映射与三次内核登记。
- 已知可动笔：`SYS_STATE_CLEAR_IPC_FILTERS` 内核侧已实现（`os/kernel/src/syscall_process.rs:839`）而**全仓零调用方**。先写勘察节（每段在 Rust 侧的对应物与缺因），跨阶段接缝需要裁决的部分登记不代决。

**第三步：呈报需要人裁决的清单**（一次性交给用户，不要散着问）：`P-ALL-08-T4`（四类拦截是否纳入本轮）、`P-ALL-08-T5`（三套 trait 是否收敛）、`P-ALL-03`（真多核撤钳的甲／乙案）、`P-X86-01`、`P-ALL-02`、`P-A64-03`、`P-ALL-12` 的修法三案（六个联合体成员超 56 字节，改结构体必带三架构上机对账）、`devman` 的 `SefHooks::on_signal` 接或删。

**第四步之后**：`P-ALL-07`、`P-ALL-11`（演进级，不挡终目标）。

## 7. 工具配方（照抄可用）

- **一切 cargo 命令走容器**（宿主机工具链与内存都不够，且必须固定镜像版本）：
  ```
  docker run --rm -m 2g -u $(id -u):$(id -g) -v /home/xzhao/github/minix-rs:/home/xzhao/github/minix-rs \
    -w /home/xzhao/github/minix-rs/os minix-ci:1.94 cargo test -q -j 1 -p <crate>
  ```
  收尾必跑 `cargo check -q -j 1 --workspace --tests`（移除类改动只有它能兜住）。`target/` 若被 root 占住导致写失败，先容器内 `chown -R`。
- **三门**（生产语义变更必须背书；纯注释与文档改动可省，但要写明省的理由）：
  - `RS_ATF_WORK=tmp/atf_work_<编号> bash os/qemu-tests/test-atf-riscv64.sh` → 判据 `RESULT: PASS (36/36 ...)`；点火与 `dumpdtb` 的 `-smp` 必须同值（现为 2）。
  - `bash os/qemu-tests/test-cmd-smoke.sh`（x86）与 `bash os/qemu-tests/test-cmd-smoke-riscv64.sh` → 判据 `RESULT: PASS (18-stage ...)`。
  - aarch64 侧 `bash os/qemu-tests/test-smp-aps-aarch64.sh`（只在动载荷线形或内核 IPC 时才需要）。
  - 串口里要看见 SMP 证据链（`nk4c: sbi-hs ret=0` → 桩 `A1A2` → `nk4c: ap-arrived hart=…`）才算真多核跑过。
- **格式对账**（关键：整仓格式未清，必须与基线比而不是看绝对值）：
  ```
  git worktree add -q tmp/wt<编号> HEAD
  rustup run nightly rustfmt --edition 2024 --check <本笔碰过的 .rs>   # 工作树与基线各跑一次，比 "Diff in" 行数
  git worktree remove tmp/wt<编号> --force
  ```
- **clippy 对账**：容器内 `cargo clippy -q -j 1 -p <crate> --all-targets`；要判定某枚告警是否本笔引入，用 `git stash` 回到干净树再跑一次比行号（位移要能对上）。
- **文档门**：`bash tools/doc-style-lint.sh --diff`（error 必须 0；会命中的写法包括：优先级加序号的评审编号、正文里的日期、叙述修复历史的措辞、把文档自身的编排约定写进正文的元注释、评审工具术语）。改一行密集表格会把整行变成新增行、连带那行自带的存量命中被一并算进来——先想清楚是否真的需要改那一行（§续-422 就因为这一点没改 `edge3.md` 的 S29 行）。
- **裸 unsafe 门**：`bash tools/unsafe-audit.sh --diff`（增量模式会补读上下文行认 `// SAFETY:`；`total`／`bare` 应为 0）。
- **文本陷阱**：仓库里有 CRLF 文件（台账曾出现过），用文本模式整文件读写会刷成全文件改动；`git diff --stat` 看行数是否异常。中文形近字表扫描见纪律第 11 条。
- **工作目录**：每条命令显式 `cd /home/xzhao/github/minix-rs`（跨工具复用 shell 时目录会漂）。

## 8. 诚实边界（上一会话自陈的未做与不可达）

- 通知位的展开**今天在生产跑不到**（内核 `SigSet` 位宽），宿主测试可驱动；因此各站点的可观察效果都在宿主面，真机门跑的是「未退化」而不是「新行为生效」。
- 管理器形的**发方链未通电**（PM 卡 `asynsend3` 重试表、RS 的 `signal_manager` 无生产调用者），所以 SIGTERM 经由管理器送达这条路径只在宿主可达；且因本树缺 `sef_cancel`，一旦通电会出现「已收帧无人回报」，这是登记过的红线，不是本任务线要顺手解决的。
- §续-422 的门只证到「四个文件服务器的请求路在三把门里真跑且未退化」；刷盘那条臂（`SyncThenCancelled`）尚无真机背书（不可达）。
- 上一会话的评审工具本身也复跑过宿主与静态面（含负控），但**未复跑真机门**——门的数据只由我方命令产出，两处记录互补。

## 9. 什么时候该停

达到以下任一条件就收口并交还用户：本轮队列（第 6 节的第一步或第二步）做完并成笔；或撞到第 4 节第 8 条列举的裁决点；或三架构任一门翻红且不能在本步内定位到本笔引入的原因。收口动作固定四件：WORKLOG 顶块更新到最新一节＋台账状态同步＋`ps -eo comm | grep -c '^qemu-system'` 为 0＋`git status --porcelain` 无跟踪文件改动。
