# NK4-B WORKLOG — 三架构启动 + 18-stage-commands 命令面（qwen 执行记录）

> 任务书：`NK4B-TODO.md`（同目录）。前一道弧线（NK4-A）的记录在
> `NK4A-QWEN-WORKLOG.md`，其未决前沿（x86_64 上 RS 用户态 RBX 被交付
> 成 0 导致崩溃）就是本弧线 P1 的第一个工作对象。
> 记录纪律：每个 Task/Milestone 一节（模板见 NK4B-TODO §7.1）；修复另记
> `.review/zcode/edge1/FIXLOG.md`（只追加）。

## 会话开场（2026-09-22，本 session）

- 起点 commit：`388252b6b docs(edge1,nk4a): NK4-B 任务书 + 开局 prompt`
  （分支 `rewrite`，未 push）。
- 工作树既有未提交项（非本会话产物，不动）：`AI-chats/daily.todo.md`、
  `tmp/nk4a/vars.fd`（QEMU 持久变量，陷阱 §5.13）、未跟踪
  `notes/rewrite/archive_bak/`、`notes/study/`。
- 起跑前 `pgrep -f '[q]emu-system'` → 无残留。
- 本会话按 P0 → P1 → P2 → P3(aarch64 M3.1-M3.6) → P4(riscv64) →
  P5 → P6 顺序推进；上下文逼近耗尽时优先把记录写完。

## P0 核账（2026-09-22）

- 状态：**DONE**（五项全做，只记事实）
- commit：本文件所在 docs commit（见下）

### T0.1 git 现场（原样抄录）

```
$ git log --oneline -15
388252b6b docs(edge1,nk4a): NK4-B 任务书 + 开局 prompt（三架构启动 + 18-stage-commands 命令面）
236515906 docs(nk4a,taskC): 第六轮补充——ctx.rbx 写者全静态穷举（发现布防漏 2 个存帧点，排除 2 条路径）
19de2e9c2 docs(nk4a,taskC): 第五/六轮取证记录（c23a 实证内核交付 RBX=0、c24a 证伪状态寄存器写点假说）+ 两轮串口证据归档
cb6377842 chore(nk4a,taskC): 第 5/6 轮 RBX 取证探针（pre-restore rbx 交付值 + 状态寄存器写点布防）
2bc362971 docs(nk4a,taskC): 第四轮记录（IRQ 修复 + c22a 证伪，Task C 维持 BLOCKED）+ c22a 串口证据归档
443624551 fix(kernel,taskC): IRQ/tick 入口全量存帧对齐 C mpx.S SAVE_PROCESS_CTX（NK4-A 迭代20）
9424d9542 docs(edge1,nk4a,taskC): WORKLOG 二/三轮取证记录 + c19a/c20a/c21a 串口证据归档，Task C 按铁律#10 标 BLOCKED
89cd5929c debug(rs,taskC): RS null-deref 三轮取证探针（step2 迭代打点 + endpoint_slot 基址探针）
85fab23ae docs(edge1,nk4a,taskC): WORKLOG 记录 RS null-deref 第一轮取证
84347cff2 docs(edge1,nk4a,taskA): WORKLOG 落盘 + c17a/c18a/c18b 串口证据归档（A5 定性 noaddr cr2=0x0…）
febbb0c8b fix(edge1,nk4a,taskA): VM 页故障不可服务终局补 SIGSEGV+CLEAR_PAGEFAULT 收口（C pagefaults.c:89-105 对位）
5f98b1db5 diag(nk4a,taskA): VM 页故障静默出口限次探针 pf-exit（badendpt/inactive/wro/noaddr/ok-nopte/susp/accvio/clrpf）
eebe41550 docs(edge1,nk4a): qwen 接手任务书 + 开局 prompt（迭代18 后前沿与任务分解）
e4c6e8224 chore(edge1,nk4a): 迭代11-18 串口证据归档 + RS step2 槽缺失诊断
4a6570d7f fix(edge1,nk4a): 迭代18——SENDA 提到 do_ipc 权限层之前（C proc.c:673-684 switch 顺序对位）

$ git status --short
 M AI-chats/daily.todo.md
 M tmp/nk4a/vars.fd
?? notes/rewrite/archive_bak/
?? notes/study/

$ git branch --show-current
rewrite
```

### T0.2 宿主六包计数实测（本会话基线，后续只许不减）

命令：`cd os && docker run --rm -v "$PWD:/work" -w /work -m 2g
minix-ci:1.94 cargo test -j 1 -p <pkg>`（逐包，2026-09-22 实测）。

| 包 | 实测 passed | failed | ignored | 与 NK4B-TODO §1 引用的旧数（808/241/525/350/57/315）之差 |
|----|------------|--------|---------|------------------------------------------------------------|
| minix-kernel | **809**（另有 bin 测试 3 passed） | 0 | 8 | +1（NK4-A 迭代20 的 IRQ 存帧防回归测试） |
| minix-arch | **241** | 0 | 0 | 0 |
| minix-vm | **526** | 0 | 0 | +1（NK4-A Task A 的 unknown-region 收口防回归测试） |
| minix-rs | **350** | 0 | 0 | 0 |
| minix-rt | **57** | 0 | 0 | 0 |
| minix-sys | **315** | 0 | 0 | 0 |

宿主构建警告存量（非本会话引入，属既有事实）：kernel/arch/vm 有
`unused import`/`unused variable` 若干；镜像装配时 minix-ds 报 3 条
（2 条 `unnecessary unsafe block` + 1 条 dead_code
`was_live_wide`）、minix-pm 报 `unused variable` 若干。clippy 对齐要求
按既有规则「零新增」执行，存量不归本弧线处理。

### T0.3 x86_64 冒烟实跑（exit code + 串口到达序列）

命令：`SMOKE_SKIP_BOOT=0 bash os/qemu-tests/test-cmd-smoke.sh`
（脚本自身跑 `xtask image --arch x86_64 --release`，未加宿主 ulimit 也
构建成功；完整 stdout 在本会话工作文件 `/tmp/nk4b_p0_smoke.txt`）。

```
SMOKE-EXIT=1
assembling bootable image … ✅ 镜像就绪（IMG 产出）
image assembled: os/target/image/x86_64/minix.img
ESP verified: kernel.elf + imgrd + all 12 module names present      ← stage 1-2 PASS
serial: scheduler hand-off reached — waiting for the T4 command marker  ← stage 3 PASS（"entering scheduler" 到达）
FAIL: T4 marker 'rc: minimal boot script marker' never appeared within 60s  ← stage 4 FAIL
```

失败时刻串口尾部（脚本自带的 `tail -12`，原文）：

```
nk4a: sa1-after cr3=0x0x000000001e76d000
nk4a: pre-restore- rip=0x00000000002014ad rsp=0x00007fffffffc0b8 rbx=0x0000000000010001
nk4a: vm-pf recv
nk4a: pf-exit noaddr cr2=0x0
<unset> 0x0000000000000002 0x0000000000216a76
boot-shim panic: panicked at kernel/src/syscall_signal.rs:300:13:
cause_sig: sig manager 2 gets lethal signal 11 for itself…
```

到达序列结论：**stage1（镜像）→ stage2（ESP 校验）→ stage3（调度器交接
"entering scheduler"）全部到达；stage4（rc marker）未到达**。失败形态
与 NK4-A 最后一轮（c24a）逐字段一致：endpoint 2（RS）故障
`rip=0x216a76`、`pf-exit noaddr cr2=0x0`、RS 自任 sig manager 收
SIGSEGV → `syscall_signal.rs:300` panic。

附带事实：该脚本把串口日志写在 `mktemp` 且 `trap cleanup EXIT` 删除，
**跑完不保留 serial 文件**；需要串口全文取证时另用
`tmp/nk4a/run-qemu.sh <轮次>`（写 `tmp/nk4a/serial_<轮次>.log`）。

### T0.4 前棒记录的事实清单（只记事实，不评价）

读 `NK4A-QWEN-WORKLOG.md`（全 471 行，`wc -l` 实测）与
`.review/zcode/edge1/FIXLOG.md` 尾部（1018 行，读迭代19/20/21 三条 +
Fix #9 系列）。

改了哪些文件（NK4-A 本弧线全部 commit 的实质代码面）：
- `os/servers/vm/src/vm_server.rs`：页故障「不可服务」三个终局补
  SIGSEGV + 清挂起位（`febbb0c8b`，对位 C
  `minix3/minix/servers/vm/pagefaults.c:99-104/112-116/146-151`），
  新增宿主测试 1 条（VM 525→526）。**有真机证据**：c17a 定性 +
  c18a/c18b 两次复跑死锁消除（`evidence/20260922-nk4a-taskA-c17a-c18/`）。
- `os/kernel/src/trap_dispatch.rs`：tick 0xF1 臂与 IRQ 臂入口加全量存帧
  （`443624551`，对位 C `mpx.S:77/137/355/540` 的
  `SAVE_PROCESS_CTX` + `sconst.h:75-89`），新增宿主测试
  `irq_entry_mirrors_user_frame_but_skips_kernel_origin`（kernel
  808→809）。**有真机证据**：c22a 复跑显示崩溃序列未变——该偏差
  本身成立并修复，但不是这次崩溃的根因。
- 探针（无行为改动，全部 `#[cfg(not(feature="mock"))]` + AtomicUsize
  限次）：`os/kernel/src/lib.rs`（pre-restore 路标加 `rbx=` 交付值）、
  `os/kernel/src/trap_dispatch.rs`（新增 `nk4a_rbx_probe` +
  irq-save/int33-save/int33-ret/pf-save）、`os/kernel/src/proc.rs`
  （add-call/add-flags）、`os/kernel/src/ipc.rs`（recv-clear）。
  commit `cb6377842`；真机产物 c23a/c24a
  （`evidence/20260922-nk4a-taskC-c23/`、`-c24/`）。

声称修了什么、哪些有真机证据（逐条对照）：

| 声称 | 证据状态 |
|------|---------|
| VM 页故障静默死锁消除 | **有**：c18a/c18b 两次复跑 + 新增宿主测试 |
| IRQ/tick 入口存帧对齐 C（独立 C 偏差） | **有**：宿主测试判别双向 FAIL；真机证明它**不是**本崩溃根因 |
| 崩溃现场解释链（RBX=0 交付 → asynsend push/pop 固化 → self=0 → 读 VA 0 → SIGSEGV） | **静态有**（未 strip 的 RS ELF 反汇编：`0x203bf0`=asynsend 首指令、`0x216a76`=endpoint_slot+0x176、`0x20d2db` 调用点 `mov %rbx,%rdi`）；**根因未证实** |
| 「IPC 状态寄存器写点把 0 写进 RS 的 RBX」 | **证伪**（c24a 五类写点对 RS 零命中） |
| 「syscall 瘦帧未填 rbx」「apply_to_trap_frame 漏拷 rbx 致交付 0」 | **静态排除**（两入口汇编都 `push rbx`；`restore_to_user` 的 RBX/GP 取自 ctx 而非 frame，`arch/src/x86_64/trap_return.rs:109/118-131`） |
| `ctx.rbx` 写者全集 | 已穷举：全量存帧 5 点（`kernel/src/trap_dispatch.rs:127/585/697/887/1103`）+ `clear_ipc_status_reg`（唯一站点 `kernel/src/ipc.rs:2087`）+ `or_ipc_status_reg`（`kernel/src/proc.rs:1790/1818`）+ `set_secondary_ipc_return`（`kernel/src/syscall.rs:811`，仅 KernInfo）+ `write_user_register` offset 72（`arch/src/x86_64/boot.rs:227`）+ 出生（`boot.rs:143` `ps_strings.unwrap_or(0)`）+ sigreturn（`arch/src/x86_64/signal.rs:321`） |
| 第 6 轮布防缺口 | 5 个存帧点只布防了 3 个，**:697（异常→信号臂）与 :1103（syscall 腿 VmSuspend 臂）未布防** |
| 取证方法缺陷 | `pf-save` 探针上限两次（8/48）被启动前段同一 refault 循环耗尽（c24a 48 条全在崩溃行之前），**崩溃前最后一次 RS 故障入口的捕获值至今未拿到** |
| 已备好未执行的第 7 轮方案 | :697/:1103 各补一次 `nk4a_rbx_probe`；`pf-save` 改 `ep==2` 过滤 + `(rip,rbx)` 去重 + 上限 64；打印 RS 出生 `ctx.rbx` 判 `boot.rs:143` 的 `unwrap_or(0)` 是否被走到 |

NK4-A 的 Task 粒度状态：A DONE、B 未触发、**C BLOCKED（六轮真机未定性，
已超铁律 3 轮上限）**、D/E 未开始（严格下游）。

### T0.5 判定：P1 不跳过

T0.3 未到达 rc marker（stage 4 FAIL，SMOKE-EXIT=1）。按 NK4B-TODO §1
T0.5 的分支：**P1 必须做**，其内容就是 NK4-A Task C 的未决前沿——
x86_64 上 RS 的 RBX 交付成 0 → init_fresh step2 的 `self=0` 空指针
→ SIGSEGV → 内核 cause_sig panic，rc marker 因此永不到达。
P1 的断点、已排除项、未布防站点、第 7 轮方案见上表（T0.4）。

- 自检：fix-guard 本轮未涉及代码修改 N/A；计数不减 ✅（未改代码）；
  两次复跑 N/A（P0 只要求实测记录）；FIXLOG 本轮无修复条目，N/A；
  WORKLOG ✅
