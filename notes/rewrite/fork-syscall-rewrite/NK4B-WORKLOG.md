# NK4-B WORKLOG — 三架构启动 + 18-stage-commands 命令面（qwen 执行记录）

> 任务书：`NK4B-TODO.md`（同目录）。前一道弧线（NK4-A）的记录在
> `NK4A-QWEN-WORKLOG.md`，其未决前沿（x86_64 上 rs（Root Server）用户态
> RBX 被交付成 0 导致崩溃）就是本弧线 P1 的第一个工作对象。
> 记录纪律：每个 Task/Milestone 一节（模板见 NK4B-TODO §7.1）；修复另记
> `.review/zcode/edge1/FIXLOG.md`（只追加）。

**术语约定**（本文首次出现处均按此展开，后文用短名）：

- `rs` = Root Server，MINIX 的进程表/信号服务端点（本弧线上端点号为 2）；
- `vm` = VM Server，虚拟内存服务器（负责页故障服务）；
- `init` = 1 号进程，负责挂载根盘并执行启动脚本 `/etc/rc`；
- ESP = EFI System Partition，镜像上的 EFI 系统分区，装载 kernel.elf 与各模块；
- imgrd = image ramdisk，`init` 挂载为根文件系统的内存盘镜像；
- rc marker = `os/etc/rc` 里 `echo` 出的那行
  `minix-rs rc: minimal boot script marker`，本弧线的启动链终点标记；
- 探针（本文原写「布防」）= 在代码路径里插入限次串口打印以取证。

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
84347cff2 docs(edge1,nk4a,taskA): WORKLOG 落盘 + c17a/c18a/c18b 串口证据归档（A5 定性 noaddr cr2=0x0；A7 两轮复跑死锁消除→新断点 cause_sig panic 归 Task C）
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
两份存档：`evidence/20260922-nk4b-p0/host-tests-rs-rt-sys.txt` 是首次
运行原样输出（当时误用 `head -6` 取结果，kernel/arch/vm 三包的
`test result:` 行被先出现的编译警告顶出窗口，只存下警告部分）；
`host-tests-kernel-arch-vm.txt` 是为补回这三包真实计数而重跑的输出
（只留 `test result:` 与 `error` 行）。本表数值以后者 + 前者未受
截断的部分为准。

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
构建成功；完整 stdout 已归档 `evidence/20260922-nk4b-p0/smoke-stdout.txt`）。

命令输出中的 stage 判定行（`SMOKE-EXIT=1` 为本文另外用 `echo $?`
取到的退出码，其余为脚本 stdout 关键行，箭号为本文标注）：

```
SMOKE-EXIT=1
assembling bootable image … ✅ 镜像就绪（IMG 产出）
image assembled: os/target/image/x86_64/minix.img
ESP verified: kernel.elf + imgrd + all 12 module names present      ← stage 1-2 PASS
serial: scheduler hand-off reached — waiting for the T4 command marker  ← stage 3 PASS（"entering scheduler" 到达）
FAIL: T4 marker 'rc: minimal boot script marker' never appeared within 60s  ← stage 4 FAIL
```

失败时刻脚本自带的 `tail -12` 串口尾部，全 12 行逐字摘录（完整段见上述
归档文件；原文行尾带回车符 `\r`，此处不复制）：

```
nk4a: pre-restore- rip=0x00000000002014ad rsp=0x00007fffffffc0b8 rbx=0x0000000000010001
nk4a: vm-pf recv
nk4a: pf-exit noaddr cr2=0x0
<unset> 0x0000000000000002 0x0000000000216a76
boot-shim panic: panicked at kernel/src/syscall_signal.rs:300:13:
cause_sig: sig manager 2 gets lethal signal 11 for itselfkernel panic: panicked at kernel/src/syscall_signal.rs:300:13:
cause_sig: sig manager 2 gets lethal signal 11 for itselfkernel on CPU 0x0000000000000000: trap: vector 0x000000000000000d err 0x0000000000000000 rip 0x000000001ddad6f9 cs 0x0000000000000008 rflags 0x0000000000000093 rsp 0xffff8000003ff990 ss 0x0000000000000010
nk4a: pfm5-dump
boot-shim panic: panicked at kernel/src/trap_dispatch.rs:543:13:
kernel exception vector 13 at rip 0x1ddad6f9 errcode 0x0 [dispatch_body @ 0x1ddb43c0]kernel panic: panicked at kernel/src/trap_dispatch.rs:543:13:
kernel exception vector 13 at rip 0x1ddad6f9 errcode 0x0 [dispatch_body @ 0x1ddb43c0]kernel on CPU 0x0000000000000000: (stacktrace skipped: recursive panic)
qemu-system-x86_64: terminating on signal 15 from pid 1186075 (bash)
```

到达序列结论：**stage1（镜像）→ stage2（ESP 校验）→ stage3（调度器交接
"entering scheduler"）全部到达；stage4（rc marker）未到达**。失败形态
与 NK4-A 最后一轮（c24a）一致：端点 2（rs）页故障 `rip=0x216a76`、
`pf-exit noaddr cr2=0x0`、rs 自任 sig manager 收 SIGSEGV →
`syscall_signal.rs:300` panic；此后内核自己又踩到 vector 13（一般保护
故障）递归 panic。

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
| 崩溃现场解释链（RBX=0 交付 → asynsend push/pop 固化 → self=0 → 读 VA 0 → SIGSEGV） | **静态有**（未 strip 的 rs ELF 反汇编：`0x203bf0`=asynsend 首指令、`0x216a76`=endpoint_slot+0x176、`0x20d2db` 调用点 `mov %rbx,%rdi`）；**根因未证实** |
| 「IPC 状态寄存器写点把 0 写进 rs 的 RBX」 | **证伪**（c24a 五类写点对 rs 零命中） |
| 「syscall 瘦帧未填 rbx」「apply_to_trap_frame 漏拷 rbx 致交付 0」 | **静态排除**（两入口汇编都 `push rbx`；`restore_to_user` 的 RBX/GP 取自 ctx 而非 frame，`os/arch/src/x86_64/trap_return.rs` 的 `rbx_off` 装配段） |
| `ctx.rbx` 写者全集 | 已穷举：全量存帧 5 站点——`os/kernel/src/trap_dispatch.rs` 的 `mirror_irq_frame_into_proc`（:127）、`x86_trap_dispatch_body`（:585 页故障转发臂、:697 异常→信号臂）、`x86_ipc_dispatch_body`（:887）、`x86_syscall_dispatch_body`（:1103 VmSuspend 停车臂）+ `clear_ipc_status_reg`（唯一调用点 `os/kernel/src/ipc.rs` 的 plain-RECEIVE 前奏）+ `or_ipc_status_reg`（`os/kernel/src/proc.rs` 两处 `|=`）+ `set_secondary_ipc_return`（`os/kernel/src/syscall.rs`，仅 `IpcCall::KernInfo`）+ `write_user_register` offset 72（`os/arch/src/x86_64/boot.rs`）+ 出生值（同文件 `entry.ps_strings.map(...).unwrap_or(0)`）+ sigreturn（`os/arch/src/x86_64/signal.rs`） |
| 探针未覆盖的存帧站点 | 5 个存帧站点只有探针 3 个，**:697（异常→信号臂）与 :1103（syscall 腿 VmSuspend 臂）无探针** |
| `pf-save` 探针上限耗尽、覆盖不到崩溃现场 | 上限两次（8/48）均被启动前段同一 refault 循环耗尽（c24a 48 条全在崩溃行之前），**崩溃前最后一次 rs 故障入口的捕获值至今未拿到** |
| 已备好未执行的第 7 轮方案 | :697/:1103 各补一次 `nk4a_rbx_probe`；`pf-save` 改 `ep == 2`（只看 rs）过滤 + `(rip,rbx)` 去重 + 上限 64；打印 rs 出生 `ctx.rbx` 判 `unwrap_or(0)` 是否被走到 |

NK4-A 的 Task 粒度状态：A DONE、B 未触发、**C BLOCKED（六轮真机未定性，
已超铁律 3 轮上限）**、D/E 未开始（严格下游）。

### T0.5 判定：P1 不跳过

T0.3 未到达 rc marker（stage 4 FAIL，SMOKE-EXIT=1）。按 NK4B-TODO §1
T0.5 的分支：**P1 必须做**，其内容就是 NK4-A Task C 的未决前沿——
x86_64 上 rs 的 RBX 交付成 0 → init_fresh step2 的 `self=0` 空指针
→ SIGSEGV → 内核 cause_sig panic，rc marker 因此永不到达。
P1 的断点、已排除项、探针未覆盖的存帧站点、第 7 轮方案见上表（T0.4）。

- 自检：fix-guard 本轮未涉及代码修改 N/A；计数不减 ✅（未改代码）；
  两次复跑 N/A（P0 只要求实测记录）；FIXLOG 本轮无修复条目，N/A；
  WORKLOG ✅；真机/测试输出已 `git add -f` 归档到
  `evidence/20260922-nk4b-p0/`（冒烟 stdout + 六包 `test result:` 行）✅

## P1 — x86_64 抵达 rc marker（状态：PARTIAL，取证三轮已定性到写者集合，修复需架构裁决）

- 状态：**PARTIAL**（未达成 rc marker；根因候选已收窄到一个写者集合，
  但怎么修属架构级裁决，不自行定案）
- commit：探针 `3945cf5d0 debug(nk4b,p1): 第 7/8/9 轮 rs RBX 轨迹探针`、本文档与证据另一 commit
- 真机轮次：c25a（第 7 轮）/ c26a（第 8 轮）/ c27a + c27b（第 9 轮，两次独立复跑）
- 证据：`evidence/20260922-nk4b-p1-r7-9/`（四份串口日志 + 两份镜像构建日志）

### 本轮上的三个探针（均在 `os/kernel/src/trap_dispatch.rs`，全部
`#[cfg(not(feature = "mock"))]` + 限次 + 标「task1-close 裁决删除」）

| 探针 | 过滤 | 去重键 | 上限 | 调用站点 |
|------|------|--------|------|----------|
| `nk4a_rs_trace_probe` | 仅 rs（端点 2） | `(site, rip, rbx)` | 64 | pf2 / x33 / irq / sig / vms / rst |
| `nk4a_rs_anom_probe` | rs 且 `rbx < 0x10000` | `(site, rip, rbx)` | 48 | 同上 |
| `nk4a_rs_leak_probe` | rs 且「指针量级→状态量级」跃变 | 跃变次计数 | 12 | 同上 |

站点含义：`pf2` = 页故障转发前的存帧（`x86_trap_dispatch_body` 的
ForwardToVm 臂）、`sig` = 异常→信号投递臂存帧、`vms` = syscall 腿
VmSuspend 停车臂存帧、`x33` = int-33 IPC 入口存帧（`x86_ipc_dispatch_body`）、
`irq` = tick/IRQ 镜像存帧（`mirror_irq_frame_into_proc`）、`rst` = 交付侧
（`os/kernel/src/lib.rs` 的 `finish_and_restore` 紧接 iretq 前）。
第 7/8 轮的 site 名 `i33` 与 `irq` 首字节相同，被去重表合并（计数丢数据），
第 9 轮改为 `x33`。

### 三轮得到的四条定性结论（均有串口行号可查）

1. **存帧→恢复回路忠实**（c25a）：同一次循环里 `pf2`/`irq`/`x33` 采到的
   `(rip, rbx)` 与紧随其后的 `rst` 交付值逐对相等（如第 221 行
   `pf2 rbx=0x7fffffffefe0 rip=0x225287` → 第 247 行 `rst` 逐字同值；
   第 178 行 `rst` 与第 187 行 `pf2` 是同一对（`rbx=0x7fffffffefe0
   rip=0x225280`，先交付后下一次故障入口再采，值未变）；同
   `(site, rip, rbx)` 三元组的重复采样被去重舍去，故表中不再现）。
   →  NK4-A 第 5/6 轮的「内核在存帧与交付之间把 RBX 改写」分支持续
   不成立（第 6 轮静态排除之外的真机佐证）。
2. **出生值不是 0**（c25a 第 178 行 `rst n=0x0 rbx=0x7fffffffefe0`）：
   `os/arch/src/x86_64/boot.rs` 里 `entry.ps_strings.map(|v| v.0).unwrap_or(0)`
   走的是 `Some` 分支。→ NK4-A 第 7 轮方案里的「出生即 0」候选被证伪。
3. **崩溃现场已拿到**（c26a 第 3872/3893/3896 行）：
   `rs-anom pf2 rbx=0x0 rip=0x203bf0` → `rs-anom rst rbx=0x0 rip=0x203bf0`
   → `rs-anom pf2 rbx=0x0 rip=0x216a76`（崩溃指令，`mov (%rdx,%rcx,1),%rax`
   读虚存地址 0）。异常值从最早的 `rs-anom n=0x0`（第 577 行，
   `rbx=0x7 rip=0x227b29`）就存在，到崩溃共 6 次异常采样。
4. **跃变全部发生在未采样区段**（c27a 六条 + c27b 六条同形）：每次跃变
   都报在存帧站（c27b 五次 `pf2` + 一次 `irq`），而上一采样点是
   指针量级（如 c27b 第 4 条：`from_site=rst prev_rbx=0x7fffffff9d48
   prev_rip=0x225979` → `rbx=0x0 rip=0x203bf0`）。即：RBX 从用户指针变成
   小整数/0，发生在六个采样点**之间**。而内核写 ctx.rbx 的站点只有
   `clear_ipc_status_reg`（`os/kernel/src/ipc.rs` 的 plain-RECEIVE 前奏）、
   `or_ipc_status_reg`（`os/kernel/src/proc.rs` 的 `ipc_status_add_call` /
   `ipc_status_add_flags`）、`set_secondary_ipc_return`（`os/kernel/src/syscall.rs`
   的 KernInfo 腿）与 sigreturn——写的就是用户 callee-saved 寄存器的值。
   异常值形态与此吻合：c27a 的 0x7 = `SEND(1)|RECEIVE(2)|SENDREC(4)` 的
   OR 累积，c25a 第 285 行 `vms rbx=0x0000062c00007bff` 是一个用户指针
   被高位标志 OR 过的合并值。

### 为什么本轮不直接修（上交裁决）

在 x86_64 上把 RBX 当 IPC 状态寄存器使用，本身就是一个与 C 不同
的架构选择：C 只有 i386（`IPC_STATUS_REG = bx`，
`minix3/minix/include/arch/i386/include/ipcconst.h:10`）与 earm（`= r1`，
同目录 earm 版 `:7`）两个定义，仓内无 x86_64 版。i386 能安全使用 bx 的前提
是用户侧包装函数在窗口内自行保存：`minix3/minix/kernel/arch/i386/
usermapped_glo_ipc.S` 的 `IPCFUNC` 宏先 `push %ebx`、返点后 `mov %ebx,%ecx`
取状态再 `pop %ebx`。本仓对位实现是 `os/libs/minix-sys/src/arch_trap.rs`
的 `ipc_trap`（同为 push/pop 保护）。一旦内核在【非窗口内恢复点】
写这个寄存器，用户态的 callee-saved 活值（这里是 `&self.table`）就被
摧毁且会沿 push/pop 链上传。三个候选修法（需裁决，不自行定案）：

| 方案 | 做法 | 代价 / 风险 |
|------|------|------------|
| A 换寄存器 | x86_64 的状态寄存器改为调用者不保存型寄存器（caller-saved，如 r10/r11，与 earm 的 `r1` 同思路） | 语义偏最小；需同步改 `arch_trap.rs` ABI 文档、三架构一致，属 `[ARCH]` 变更 |
| B 限制写时机 | 仅当恢复点在 ipc_trap 窗口内才写/交付 RBX，否则走影子字段 | 内核要能判定“窗口内”，需额外记录与检查；易漏 |
| C 存/复分离 | ctx 里单开一个状态字段，不进用户寄存器组，`restore_to_user` 不交付它 | 用户态得改从其他渠道取状态，与 C 行为偏离最大 |

本弧线的 P2-P6 均硬依赖 x86_64 启动链翻绿（rs 不活 → init 不跑 →
rc marker 不出），而上述三案都改变对外 ABI/行为契约，属架构级裁决，
按任务书 §1 不自行定案。按铁律“同一问题 3 轮仍无定性则转下一个
独立任务”，本轮已把问题从「未定性」推到「写者集合已封闭 + 修法需裁决」，
下一步建议：裁决三案之一后按 fix-guard 单条修（并补宿主防回归测试：
断言状态写入不侵入用户 callee-saved 寄存器），同时并行推 P3（aarch64
M3.1 载体现状点电）——它与本断点无关。

- 自检：fix-guard ✅（每条修改前读目标行 ±5 行 + grep；探针为单类型
  改动单元，零删除行，`git diff --stat` 为 +228）；计数不减 ✅（kernel
  809 passed / 0 failed，与 P0 基线持平）；两次复跑 ✅（c27a/c27b 同形，
  且 c27b 首次启动产出空串口日志已复跑纠正）；IMG-EXIT=0 ✅；证据已
  `git add -f` 归档 ✅；FIXLOG 本轮无修复条目（取证，非修复），N/A；
  未动 `os/etc/rc`、未动冒烟脚本、未删既有探针 ✅。

## P3 M3.1 — aarch64 载体现状点电（状态：DONE，含一处构建断裂修复）

- 状态：**DONE**（M3.1 判据 = 跑 `test-rt-birth-aarch64.sh` 并记录过/挂与
  串口序列）。顺带修掉挡住这一步的 aarch64 构建断裂（它不属于任何
  架构级裁决，是纯 cfg 缺失）。
- commit：修复 `ab79b40ba fix(nk4b,m3.1): minix-kernel 的 x86 专属取证加架构门`；
  本文与证据、取证包壳 `tmp/nk4a/run-rt-birth-aarch64.sh` 另一 commit。
- 为什么先做 P3 而不做 P2：P2 判据要求 rc marker 之后的命令真实执行，
  硬依赖 P1；P1 的修法已上交裁决（见上节 A/B/C 三案）等待中。
  M3.1 只做现状记录，与 P1 断点无依赖。

### 现状（一）：构建就挂——17 个编译错误，全部可归位到具体 commit

首轮跑原脚本（`evidence/20260922-nk4b-p3-m31/carrier-build-before-fix.log`，
696 行）：rt-birth 用户镜像构建成功，载体 `test-rt-birth-aarch64`
（`--target aarch64-unknown-uefi --features fw-aarch64-uefi --release`）
在依赖 `minix-kernel` lib 时报 **17 errors**，脚本以
`FAIL: test-rt-birth-aarch64 build failed` 退出（exit 1）。按 `git blame`
逐行归位（三条错误类型、四个来源 commit）：

| 错误 | 条数 | 现场 | 引入 commit |
|------|------|------|-------------|
| `invalid register ecx/eax/edx` | 9 | `kernel/src/lib.rs:658/661/676` 的 `rdmsr`（GS_BASE/KERNEL_GS_BASE 采样） | `9764d4c7e`（NK4-A F0 根因修复的 gs0/gs1 探针） |
| `no field rip/rsp on AArch64ExceptionFrame` | 4 | `kernel/src/lib.rs:3346/3348`、`kernel/src/syscall_process.rs:415/417` | `91961877b5`（NK4-A pre-restore / exec-store 路标） |
| 同上 | 3 | `kernel/src/lib.rs:3364/3370/3376`（`rst` 轨迹探针传 `frame.rip`） | `3945cf5d0`（**本弧线 P1 第 7/8/9 轮，我自己上一里程碑加的**） |
| `cannot find type TrapFrame` | 1 | `kernel/src/trap_dispatch.rs:119`（`mirror_irq_frame_into_proc` 形参；该类型的 `use` 在 `:46-47` 本就 x86 门控，函数体漏了） | `443624551e`（NK4-A IRQ/tick 入口全量存帧） |

共同形态：x86_64 专属的取证代码写在三架构共用的路径上，只带
`#[cfg(not(feature = "mock"))]` 特性门、没带架构门。宿主测试与非
mock 的 x86_64 镜像构建都覆盖不到 aarch64，因此断裂一路无人发现；
`os/qemu-tests/run_all.sh:63` 的 aarch64 构建循环把失败写成
`|| echo "(build failed)"`，退出码被吞掉，后续步骤改判为 skip ——
这条记给 P6（run_all 接线）：接线时不能沿用吞掉构建失败的形态。

修法与验证见 FIXLOG 同日期条目（纯 cfg 门控，不删任何探针；对位仓内
既有约定 `#[cfg(all(not(feature = "mock"), target_arch = ...))]`，同文件
`kernel/src/lib.rs:136/140/144` 已是此形）。修后：

- 载体构建成功（`target/aarch64-unknown-uefi/release/test-rt-birth-aarch64.efi`
  985088 字节）；
- 宿主 `cargo test -p minix-kernel` **809 passed / 0 failed**（与 P0 基线持平，
  `host-test-minix-kernel.txt`）；
- x86_64 侧语义零变化的证据：镜像 IMG-EXIT=0 + 真机 c28a 串口
  （`wc -l` 计 3922 行；末行无换行符，编辑器计 3923），
  `rs-tr` 64 条（上限耗尽）/`rs-anom` 12 条/`rs-leak` 6 条，终态与 P1 的
  c27a/c27b 同形（rs SIGSEGV → 内核 vector 13 递归 panic），见
  `serial_c28a.log`。

### 现状（二）：修好构建后，串口停在平台发现

原脚本第二轮（`test-script-after-fix.log`，1114 行，判据行
`### TEST_RESULT: FAIL test-rt-birth-aarch64 ###`）与包壳两次复跑
（`serial_a64_b1.log`、`serial_a64_b2.log`，各 817 字节，逐字相同）得到
同一条到达序列。包壳 `tmp/nk4a/run-rt-birth-aarch64.sh` 与版管脚本的
差别不止一处（逐条列，与它自己的输出形式匹配）：除把串口改写到持久
路径外，它**不做判据**（无 5 个 marker 的 PASS/FAIL 判定、无 SKIP 前置
检查、只打印串口路径与行数），**不保留回退路径**（固件硬写
`/usr/share/AAVMF/*`，不含版管脚本的 `qemu-efi-aarch64` 候选；无 mtools
缺失时的 `file=fat:rw:` 回退），**默认等待 75 s**（原版 90 s）。它只是
取证记录器，不能取代原版测试；本机器两个前提（AAVMF 与 mtools 齐备）
已在本宿主核实。串口全文如下（原文行尾带回车符，此处不复制）：

```
### test_rt_birth (aarch64): first minix-rt user binary on AAVMF
kernel: entering validate
kernel: v0 enter
kernel: v1 asserts ok
kernel: v2 fallback region ok
kernel: v3 pt_alloc ok
kernel: step0 validate ok
kernel: step1+2 mappings ok
kernel: step4 DM coverage ok
  paging enabled
### PANIC in test-rt-birth-aarch64: libs/minix-platform/src/global.rs:0x0000000000000111 platform::init_from_kinfo: no platform source parsed successfully and not a dev build (no QemuVirt fallback in release)
 ###
```

事实清单：

1. 生产内核的架构无关段在 aarch64 上活着：页表校验（validate → v0-v3 →
   step0/step1+2/step4）与 `paging enabled` 全部通过，即
   `os/arch/src/aarch64` 的分页与半映射把内核送进了 C 对位的早期阶段。
2. 死点是 `platform::init_from_kinfo`（`os/libs/minix-platform/src/global.rs:231-253`）：
   它遍历 `kinfo.platform_sources`，第一个解析成功的描述符胜出；全失败时
   调 `qemu_fallback_or_panic`，而该函数（同文件 `:223-224` 的注释即契约）
   **release 构建直接 panic**，只有 dev 构建才回退到 `QemuVirtDesc`。载体
   与 `run_all.sh` 都按 `--release` 构建，所以必然撞这堵墙。
3. 载体的 sources 由 `os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth-aarch64/src/main.rs:493`
   调 `boot-shim/src/uefi_helpers.rs:93` 的 `find_platform_sources()` 产生；
   aarch64 分支（同文件 `:110-125`）先找 UEFI 配置表里的 DTB
   （`DEVICE_TREE_GUID`）、再补 ACPI RSDP。**本文不能判定**本次是
   「一个 source 都没找到」还是「找到了但解析失败」——`global.rs:243-247`
   故意把 `Err(_)` 静默丢掉，串口上没有任何线索。区分它只需要一条 dev
   构建（走回退、能过）或一条打印 source 数量/解析错误的探针，属 M3.4 的
   第一个实验，不在 M3.1 范围。
4. 任务书 §3 的已知事实「AAVMF 载体必须 `gic-version=3`」在本次串口点
   **尚不起作用**：默认 `-machine virt` 与 `-machine virt,gic-version=3`
   两次串口 817 字节逐字相同，因为 panic 发生在中断控制器使用之前。
   GIC 版本要到 M3.4 的时钟/中断初始化才成为变量。

### M3.1 结论与下一步

- 与 P1 不同，这里的断点**不需要架构裁决**就能推进：M3.2（kernel-image
  aarch64 产出）与 M3.3（boot-shim aarch64 装载）都不经过
  `init_from_kinfo`；本串口点只影响 M3.4（内核点电）。
- 下一里程碑判据（NK4B-TODO §3 M3.2）：`os/kernel-image/` 现在
  是 x86_64 专用（`fw-x86-none` 门 + `x86_64.ld`），要扩出 aarch64 的
  链接脚本与入口约定，设计要点先写 WORKLOG，验证用宿主可测的
  readelf 断言。M3.1 提供的事实前提是：aarch64 的 `minix-kernel` lib
  现在能编译（本里程碑成果），而它一旦被真正装载执行，可以跑到
  `paging enabled`。
- 上交裁决：无。
- 已知边界（本里程碑不修，事实先记下）：`os/kernel/src/trap_dispatch.rs`
  的 `#[cfg(test)] mod tests` 整体不带架构门，但模块里至少 3 条测试直接
  用 x86 专属类型（`:1791`/`:1832`/`:1857` 构 `TrapFrame`，`:1816-1823` 用
  `X86_64ExceptionFrame`），即该模块在 aarch64/riscv64 宿主上
  `cargo test -p minix-kernel` 本来就编不过（早于本里程碑，不是
  `ab79b40ba` 引入）。不能简单把整模块改成 `all(test, target_arch =
  "x86_64")`：那会连带关掉同模块里架构无关的 4 条测试
  （`reply_code_none_is_a_wiring_bug_marker`、`exception_signal_maps_to_c_signum`、
  三条 `forward_pagefault_*`）。真要开多架构宿主测试，应逐条给 x86 专属
  测试加门；该工作属 P3 后面的宿主测试面，不在 M3.1 范围。
- 自检：fix-guard ✅（四处修改各自先读目标行 ±5 行 + grep 确认，一次一修，
  修后 grep 复核 cfg 行在位；`git diff --stat` 为 3 files / +17 / -4）；
  计数不减 ✅（宿主 kernel 809/0failed 持平）；两次复跑 ✅
  （b1/b2 串口 817 字节逐字相同，另加原脚本 r2 一轮）；载体构建与
  IMG-EXIT=0 ✅；证据 `git add -f` 归档 ✅；FIXLOG 同日期一条 ✅；
  未动 `minix3/`、未动 `os/etc/rc`、未放松任何冒烟判据（包壳只写串口，
  不改判据也不改版管脚本，原版脚本 `git diff` 为空）✅；未删既有探针
  ✅（只加 cfg 门）。

## P3 M3.2 — kernel-image 产出 aarch64 内核镜像（设计要点先行，实现与实测随后追加）

任务书判据（NK4B-TODO §3 M3.2）：`os/kernel-image/` 今天是 x86_64 专用
（`fw-x86-none` 特性门 + `x86_64.ld`），要扩出 aarch64 的链接脚本与入口约定；
设计要点先写本文，实现要宿主可测（产出 ELF + readelf 断言 entry/段布局）。
本节只写设计与依据，实测结果之后追加。

### 事实纠正：aarch64 的架构实现目录

任务书 §3 写「对照 `os/arch/src/aarch64`」，仓内实际目录是
`os/arch/src/arm64/`（模块名 `arm64`，Rust 目标三元组仍是 `aarch64-*`）。
本弧线引用它时一律用真实路径，避免下位读者 `ls` 落空。

### 决策一：内核虚拟基址 = `0xFFFF_8000_0000_0000`

不发明新值，取仓内既有 aarch64 载体与分页实现共同钉住的那个：

- 既有载体喂给内核的 `KernelInfo.kern_virt_base` 就是它
  （`os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth-aarch64/src/main.rs:506`，
  六个 aarch64 载体同值）；
- 分页实现为什么接受它：`os/arch/src/arm64/paging.rs:468-476` 说明本移植把
  `TTBR0_EL1` 与 `TTBR1_EL1` 钉在同一张 L0 根表上，T1SZ=16 ⇒ 高半区
  （VA ≥ `0xFFFF_0000_0000_0000`）正好落在 L0 表项 256..512；
  `paging.rs:792` 进一步把内核直接映射窗口写死在
  `0xFFFF_8000_0000_0000` = L0 slot 256（高半区第一个槽位）。
- 与 x86_64 的 `KERNEL_HIGH_BASE`（`os/kernel-image/x86_64.ld:29`）同值，
  但**语义不同**：x86_64 那是 4 级分页的 canonical 高半地址，arm64 这里是
  T1SZ=16 的 L0 slot 256。同一个数字，两套各自成立的理由，不能互相引用为据。

### 决策二：物理基址 = `0x4020_0000`（不是 x86_64 的 `0x200000`）

x86_64 的 `KERNEL_PHYS_BASE = 0x200000` 在 aarch64 上是错的：QEMU virt
（arm64）的内存从 `0x4000_0000` 起，`0x200000` 根本不是 RAM。链接脚本的
PT_LOAD 物理地址（LMA）是 boot-shim 的装载目标
（`os/boot-shim/src/loader.rs` 的 `load_segments_into_phys_memory` 按 LMA
拷段体），所以 aarch64 必须落在 RAM 内。取仓内既有常量：

- `hello-boot-aarch64/src/main.rs:43`、`test-higher-half-aarch64:42`、
  `test-kernel-map-aarch64:45`（注释明写「2MB-aligned, within RAM」）、
  `test-paging-enable-aarch64:41`、`test-protection-aarch64:325`、
  `test-shutdown-aarch64:43` 全部用 `0x4020_0000`；
- 它同时满足两件事：2MiB 对齐（arm64 4K 粒度的 stage-1 大页步长，与
  `x86_64.ld` 的 2MiB 收口同形）、且不与 QEMU virt 的外设区冲突
  （GICD 在 `0x8000_0000`，PL011 在 `0x0900_0000`）。

因此 aarch64 的高半偏移不再是「`vaddr - 0xFFFF800000000000` 等于物理地址」
这种 x86 关系，而是 `vaddr = 0xFFFF_8000_0000_0000 + (paddr - 0x4020_0000)`。
内核页表把 `[kern_virt_base, +kern_size)` 映射到
`[kern_phys_base, +kern_size)`（`x86_64.ld` 头部契约的同一条），
`compute_kernel_layout` 取 `min(vaddr)`/`min(paddr)`，两者一致即可。

### 决策三：入口符号 `.text.boot:_start`，立栈用链接脚本预留的栈区

对齐 `x86_64.ld` 的三条布局约定，只换指令与段名细节：

1. `ENTRY(_start)`，`.text.boot` 由 `KEEP` 钉在镜像最前；
2. 引导栈在 `.bss` 里预留 64 KiB，符号
   `kernel_boot_stack_bottom` / `kernel_boot_stack_top`（与 x86 同名，
   入口与未来的栈越界检测都靠它）；
3. 末段 `. = ALIGN(0x200000)` 收口，让 `kern_size` 落在大页阶梯上
   （`x86_64.ld:70-77` 记的是 NK4-A 首亮的实断，arm64 同理）；
4. `.eh_frame`/`.comment`/`.note` 丢弃，与 x86 脚本一致。

入口指令序列用仓内 aarch64 既有写法（`test-rt-birth-aarch64/src/main.rs:250-251`
的 `adrp` + `add x9, x9, :lo12:sym` 取栈顶），停机驻留用 `wfi`
（x86 是 `cli`+`hlt`）。交付边界与 x86 版完全相同：`_start` 立栈、进 Rust 面
打一条横幅后驻留，boot-shim → 内核镜像的跳转交接协议仍未接线（NK1/OQ-N6
裁决范围，M3.3 之前不代决），所以本里程碑的判据只到「ELF 产出 + 布局断言」。

### 决策四：包一个 bin、三种目标，靠 `--target` 选架构

今天 `[[bin]] name = "kernel"` 被 `required-features = ["fw-x86-none"]`
钉死，而 `required-features` 是「全部必须开」的语义，没法让两个特性各自
打开同一个 bin。做法：新增内部特性 `fw-none-image` 由 bin 要求，
`fw-x86-none` 与 `fw-aarch64-none` 都隐含它。对外零变化：既有调用
（`os/xtask/src/image.rs:212-222` 用 `--features fw-x86-none`）与产物
文件名 `target/<triple>/<profile>/kernel` 都不变，riscv64（P4 M4.2）可同形扩展。

### 决策五：验证形式 = 独立脚本，不改 xtask、不动 run_all

M3.2 的判据是宿主可测的 readelf 断言，不是真机（真机要等 M3.3 的
boot-shim 装载）。放独立脚本 `os/kernel-image/check-layout.sh`：构建指定
架构的内核镜像，再用 `readelf` 断言 ELF 头机器类型、入口地址、PT_LOAD 段数、
每段 `vaddr - paddr` 偏移一致、首段虚拟地址等于内核高半基址、镜像跨距
2MiB 对齐。不接进 `os/qemu-tests/run_all.sh`（P6 才接线），也不改
`xtask image`：aarch64 装机仍按现状明确拒绝
（`os/xtask/src/image.rs:162-164`，缺的是 boot-shim 的 `fw-aarch64-uefi`，
正是 M3.3 的目标）。

### 实现与实测（2026-09-22 同日追加）

- 状态：**DONE**（判据 = 宿主产出 ELF + `readelf` 布局断言全绿；真机装载
  本里程碑不要求，也无从要求——没有任何代码把 aarch64 镜像送进 QEMU）
- commit：见本节末「落盘」
- 证据：`evidence/20260922-nk4b-p3-m32/`（10 份，文件名在下文逐条引用）

#### 改了哪五个文件

| 文件 | 改动 | 依据 |
|------|------|------|
| `os/kernel-image/aarch64.ld` | 新增（94 行，含头部契约注释） | 决策一、二、三 |
| `os/kernel-image/build.rs` | 由「只认 x86_64 一个 target」改为按 `TARGET` 选脚本（match 两支） | 决策四 |
| `os/kernel-image/Cargo.toml` | 新增内部特性 `fw-none-image` 由 bin 要求，`fw-x86-none` / `fw-aarch64-none` 各自隐含它 | 决策四 |
| `os/kernel-image/src/main.rs` | 早期控制台 import 与入口 `global_asm!`、`halt()` 按 `target_arch` 分道；x86 侧逐字未变 | 决策三 |
| `os/kernel-image/check-layout.sh` | 新增（宿主布局断言 L1–L8；本里程碑 CodeReview 后补了 L0，见下文两节闭环） | 决策五 |

#### 产物与关键数字（`layout-raw.txt`）

aarch64 镜像 `os/target/aarch64-unknown-none/release/kernel`（1205672 字节）：

- `Type: EXEC`、`Machine: AArch64`、入口 `0xffff800000000000`（= `_start` 符号）；
- 4 个 PT_LOAD，首段 `VirtAddr 0xffff800000000000 / PhysAddr 0x40200000`；
  每段 `vaddr - paddr` 都是 `0xffff7fffbfe00000`（= `KERNEL_VIRT_BASE -
  KERNEL_PHYS_BASE`，正是决策二推导式 `vaddr = 高半基址 + (paddr - 物理基址)`
  的常量平移）；
- 末段 `VMA 0xffff8000000ff000 + memsz 0x101000` = `0xffff800000200000` ⇒
  跨距恰好 2 MiB（`. = ALIGN(0x200000)` 收口生效）；
- `kernel_boot_stack_bottom/top` = `0xffff800000143f00` / `0xffff800000153f00`
  （差 65536 字节），`minix_kernel::arch_boot` 在符号表里 ⇒ 内核启动图未被
  `--gc-sections` 裁出镜像。

#### 与设计不符的地方（三条，都记下）

1. **两架构的首段虚拟地址并不相等，设计里没算到这一条。** x86_64 脚本写的是
   `vaddr = KERNEL_HIGH_BASE + paddr`，首段 VMA 因此是
   `0xffff800000200000`（含物理基址）；aarch64 按决策二写成
   `vaddr = KERNEL_VIRT_BASE + (paddr - KERNEL_PHYS_BASE)`，首段 VMA 就是
   `0xffff800000000000`。两者都满足 boot-shim 的 `min(vaddr)` / `min(paddr)`
   契约（`os/boot-shim/src/loader.rs` 拿到的仍是同一组
   `kern_virt_base` / `kern_phys_base`），但**断言脚本的预期表必须按架构
   分列**——`check-layout.sh` 的 `arch_expect()` 因此不能只写「首段虚拟
   地址等于内核高半基址」一条通吃（评审后扩到九列：多出的四列是链接脚本名、
   两个基址符号名与惯例标记，见下文 P2 闭环一节）。第一版脚本就是按设计写死的
   单一预期，实跑立刻报 x86_64 的 L3b/L3c 失败，这才暴露差别。
2. **仓内还有一份更早的 aarch64 内核脚本没被设计引用。**
   `os/kernel/src/arch/aarch64/link.ld` 用的正是同一对基址
   （`KERN_VIRT_BASE = 0xFFFF800000000000`、`KERN_PHYS_BASE = 0x40200000`），
   比设计里引的六个测试载体更直接；`os/kernel/src/lib.rs:4184-4209` 的宿主测试
   `test_linker_script_aarch64_constraints` 还把这两个值 + 2MiB 对齐断言成了
   契约。这反过来印证了决策一/二（值不是我选的，是仓内既有约定）。但那份脚本
   **没有 `AT()` 段分离**（LMA 等于 VMA），不能给 boot-shim 当装载用，所以
   本里程碑另写 `aarch64.ld` 而不是复用它；两份脚本的分工差异未记进任何文档，
   属 M3.3 之前该补的一条（本轮不顺手改它，避免动到 NK4-A 既有的测试语义）。
3. **`fw-none-image` 的取舍在设计里只写了一句话，实现时先走错一步。** 我第一版
   把 `required-features` 写成 `"fw-x86-none", "fw-aarch64-none"`（并列 = 两个
   都得开），那会让 `os/xtask/src/image.rs:212-222` 的既有调用直接产不出 bin，
   与「对外零变化」矛盾；已改回「内部特性 + 两个架构特性各自隐含」。最终形态下
   光传 `--features fw-x86-none` 仍可用（实测见下），riscv64 到 P4 M4.2 只需
   加 `fw-riscv64-none = ["fw-none-image"]` 与 `build.rs` 一支。

#### 验证（逐项带证据文件名）

- 宿主布局断言：`check-layout-all.log` —— x86_64 与 aarch64 各 L0–L8 共 13 条
  全 PASS（合计 26 条），`CHECK-LAYOUT=PASS（all）`，`CL-EXIT=0`。
- 断言的判别性（防「永远 PASS 的测试」）：`check-layout-negative-aarch64.log` ——
  只把 aarch64 的预期首段物理地址改成 `0x40000000`（QEMU virt 的 DRAM 起点，
  一个看起来合理的值）后重跑，结果 `[L3c] FAIL`、`CHECK-LAYOUT=FAIL`、
  退出码 1，其余断言仍 PASS。说明 L3c 真在读段表，不是摆设。
  （做法：临时副本 `os/kernel-image/.cl-neg.sh` 跑完即删，正式脚本未改）
- 两架构构建本身：`build-aarch64-unknown-none.log`、`build-x86_64-unknown-none.log`
  （各 657 / 367 行，内容全是既有 crate 的警告，`^error` 计数 0）；
  `clippy-aarch64.log` —— `cargo clippy -p kernel-image --target
  aarch64-unknown-none --features fw-aarch64-none` 退出码 0，且无任何指向
  `kernel-image/` 的警告。
- x86_64 生产装机链不受影响：`img-x86_64-build.log` —— `xtask image --arch
  x86_64 --release` 的 `IMG-EXIT=0`，镜像仍含 `kernel.elf + imgrd + 12 模块`。
- 真机两次独立复跑（同一镜像，`smoke-x86_64-r1.log` / `smoke-x86_64-r2.log`）：
  两轮都 `serial: scheduler hand-off reached` → `FAIL: T4 marker 'rc: minimal
  boot script marker' never appeared`，`SMOKE-EXIT=1`；r2 串口尾部的终态是
  `kernel/src/trap_dispatch.rs:723:13` 的 `kernel exception vector 13` +
  recursive panic —— 与 M3.1 归档的 `evidence/20260922-nk4b-p3-m31/serial_c28a.log:3921`
  同一站点，即 P1 的已知断点，**不是本里程碑引入的回退**。r1 尾部停在
  `nk4a: vm-pf recv`（PIT 抢占时序敏感，NK4A-TODO §8 记过同形抖动）。
- 宿主回归：`host-test-counts.txt` —— minix-kernel 809 passed / 0 failed、
  minix-arch 241 passed / 0 failed（与 P0 基线逐项相等）；
  `cargo test -p kernel-image` 退出码 0 且无 `test result:` 行——本包无测试，
  宿主 triple 下 bin 被 `required-features` 跳过，这正是 X-2/NK5 隔离未退化的
  观测证据（若隔离破了，裸机 bin 在宿主会因缺 `eh_personality` 报 error）。

#### 已知边界（不隐瞒）

1. aarch64 镜像**从未被执行过**：boot-shim 的 `fw-aarch64-uefi` 还不存在
   （M3.3），`xtask image --arch aarch64` 仍明确拒绝。本里程碑只证明「按契约
   产出一个布局正确的 ELF」。
2. `_start` 与内核本体之间仍未接线（NK1/OQ-N6 裁决），横幅打不出来；
   `wfi` 驻留在真实入口态下是否被陷入（HCR/TDCR 配置）要等 M3.4 才知道。
3. `check-layout.sh` 未接进 `os/qemu-tests/run_all.sh`（按任务书 P6 才接线），
   今天靠手工调用。它与 `aarch64.ld` 的两处手写常量曾无对账（CodeReview
   P2#2，见下节，已用 L0 断言闭合）；残余缺口是 L0 只对基址这一组常量对账，
   段序、`AT()` 式、引导栈大小仍是脚本与 `.ld` 各写一份、靠注释互相指认。
4. 环境事实（给下位读者）：`os/qemu-tests/test-cmd-smoke.sh` 的 `mktemp` 模板
   硬写 `/tmp/...`，在 `/tmp` 只读的受限沙箱里会先报
   `FAIL: guest never reached the scheduler hand-off`（QEMU 根本没起来）。
   本轮第一次跑就撞上这条，换成可写 `/tmp` 后同一条命令即正常。别把它误读成
   启动链回退。

- 自检：fix-guard ✅（每条改动前读目标文件全文/目标行 ±5 行 + grep 确认；
  一次一个逻辑单元，五个文件属同一里程碑但按「实现 + 验证脚本」两笔落盘）；
  计数不减 ✅（kernel 809 / arch 241 持平）；两次复跑 ✅（真机 r1/r2 同形）；
  IMG-EXIT=0 ✅；C 源对位 ✅（`aarch64.ld` 头部按 `x86_64.ld` 同一条契约写，
  常量取自仓内既有 `os/kernel/src/arch/aarch64/link.ld`，非发明）；
  FIXLOG 一条 ✅；证据 `git add -f` ✅；未动 `minix3/`、未动 `os/etc/rc`、
  未放松任何冒烟判据 ✅；未新增探针（本里程碑是产出物，无需取证）✅。

### 里程碑 CodeReview 结论与两条 P2 的闭环（2026-09-22 同日追加）

评审范围：`f38fca038..15fb4600e`（M3.2 的 feat + docs 两笔）。
结论：**无 P0 / 无 P1**。三项重点核查（x86_64 侧零回归、`aarch64.ld` 与
boot-shim 读取字段对得上、`check-layout.sh` 的断言有判别力）均附核实方法通过；
文档一致性与铁律（一逻辑单元一 commit、未碰禁区）核实通过。提出两条 P2，
本会话不递延、直接闭合：

| P2 | 风险（评审原话概括） | 闭环做法 | 实测证据 |
|----|--------------------|---------|---------|
| #1 | `nm` 若只能读宿主架构 ELF，L6b/L7/L8 会以「布局不对」的面目报错，
      实际是工具缺位 | 读工件前先探一次 `nm "$elf"`，不可解析则直接报
      「nm 无法解析……（需 binutils-multiarch 或 llvm-nm）」并退出，
      不再让段断言背锅 | `check-layout-negative-nm-unreadable.log`（用 PATH
      shim 把 `nm` 换成恒定失败的脚本）：退出码 1，输出只有 L0 PASS +
      工具缺位提示，没有一排假的 FAIL |
| #2 | 基址常量在 `.ld` 与脚本预期表里各手写一份，无一致性校验（本节
      已知边界 #3 登记过） | 预期表扩到九列（新增：链接脚本名、脚本内高半
      基址符号名、脚本内物理基址符号名、首段 VMA 惯例）；新增 `ld_const()` 从 `.ld`
      里抽 `SYM = 0x...;`，新增 **L0 对账断言**；`want_vma` 与入口预期改为
      按该惯例现场计算，不再手写拼串 | 两个漂移变体：改符号名（`KERNEL_PHYS_BASE_TYPO`）
      → 只 `[L0] FAIL`、`[L3c]` 仍 PASS（即 L0 有段断言抓不到的独立能力）；
      改值（`0x40000000`）→ L0 与 L3c 同 FAIL。证据
      `check-layout-negative-L0-symbol-drift.log` /
      `check-layout-negative-L0-value-drift.log`，均退出码 1 |

闭环后的止态：`check-layout-all.log` —— 两架构各 13 条（L0–L8）全 PASS、
`CHECK-LAYOUT=PASS（all）`、`CL-EXIT=0`（该日志由不带 `SKIP_BUILD` 的调用产出，
即脚本自己走了一遍两架构的 `cargo build` 再断言）。本轮只改了一个宿主
shell 脚本：`git diff --stat` 证明无 `.rs` / `.ld` / `Cargo.toml` 改动，且
`grep -rn check-layout os/ .github/ tools/` 零命中（该脚本尚未被任何入口
调用，影响面封闭）。仍按铁律复跑：宿主 minix-kernel 809 passed / 0 failed、
minix-arch 241 passed / 0 failed（与 P0 基线逐项相等）；
`cargo test -p kernel-image` 退出码 0 且无 `test result:` 行（X-2/NK5
隔离未退化）；两个变体与 nm shim 的临时副本跑完即删，工作树无残留。

本次编辑自己引入的两个小瑕（当场发现、当场修，不是新发现）：
`ld_const` 的行续接反斜杠多写一个（`bash -n` 报回）、L0 输出大小写混排
（`.ld` 习惯大写、预期表小写，对账行不好读，用 `tr 'A-F' 'a-f'` 归一）。

### 评审复核（第二轮）：三条 P2 + 一处上一轮评审漏抓的换行符缺陷

对修正提交 `aedbff11f` 再做一次评审，无 P0/P1，三条 P2；核查过程中另发现
一个两边评审都没抓到的真缺陷。

| 项 | 问题 | 处置 | 实测 |
|----|------|------|------|
| 评审 P2#1 | 本笔把预期表改成九列、加了 L0，但本节早期写的「`arch_expect()`
  因此有六列」与改动表里的「断言 L1–L8」没同步，同一文档里自相矛盾
  （是「改动使旧句变陈旧」的文档与代码不一致） | 两处旧句改为当前形态并标
  「评审后追加」 | grep 确认本节内不再出现与实际不符的「六列」与孤立「L1–L8」 |
| 评审 P2#2 | `ld_const` 用 sed 行级抽值，不认块注释 | **先试了评审建议的
  「先删含 `*` 的行」，量出它反而有害，改判为要求命中数恰好为 1**
  （函数改名 `ld_values`，返回全部命中；0 处或多处都判 L0 FAIL，不再拿
  「第一处」的出现顺序运气去比对） | 两格实测：删含 `*` 行会把合法写法
  `SYM = 0x..;  /* 说明 */` 误删成「实得 无」→ 假失败
  （`check-layout-sideeffect-star-filter.log`）；新判据的四格矩阵里，只有
  「多余声明写在真声明之后」一格出现新旧差异（旧默默 PASS、新 FAIL），
  而 `tailcomment`/`stardecoy` 两格合法形态仍 PASS
  （`check-layout-l0-uniqueness-matrix.log`） |
| 评审 P2#3 | `vma_mode` 的 `if/else` 把任何非 `plus_phys` 值都当 `virt` 算，
  第三架构引入新惯例时会静默算错 L3b/L6a 预期 | 改成 `case` 三分支，未知值
  直接报「未知的首段 VMA 惯例」并退出；同时补了预期表缺行时的硬失败
  （原本是拿空 triple 去报「工件缺失」，指不到真因） | 把表里的 `virt`
  改成 `plus_weird` → 退出码 1、只报惯例不合法且不出现 L3b/L6a 判定行
  （`check-layout-negative-unknown-vma-mode.log`） |
| 自查发现 | `aarch64.ld` 以 **CRLF** 换行入库（全 94 行，`git ls-files --eol`
  显示 `i/crlf`），而仓内其余 `.ld`（含 `x86_64.ld`、`os/kernel/src/arch/
  aarch64/link.ld`）都是 LF。链接器能容忍，所以构建与断言全绿——
  两侧评审都没拍到它 | 改为 LF（`sed -i 's/\r$//'`）。换行符不是语义，
  但会污染 diff、并在 Windows 检出与工具链比较时制造噪声 | 改前后
  aarch64 ELF 的 md5 **完全相同**（`44c945d8fc3a98629df20b2a8a1de1a6`），
  即产物字节级不变；重建后 13 条断言仍全 PASS |

这一轮的方法论教训记一笔（不是代码问题，是验收方法问题）：上一轮的 L0
只做了「改预期值」一个反向实测，没做「改 `.ld` 源文本」那一侧，所以抽值
函数的健壮性完全靠推理（推理出来的那个加固正是 P2#2 里被量出有害的那
个）。本轮补上的做法：加固与回退都要有一格差异实测才准入（四格矩阵里
只有 `decoy-after` 一格有差异，就把结论写成「买到的是不赌顺序」，而不是
写成「防注入注释」）。

本轮止态与回归：`check-layout-all.log`（不带 `SKIP_BUILD`，脚本自己走
了一遍两架构的 `cargo build`）—— 13 条 ×2 全 PASS、`CHECK-LAYOUT=PASS（all）`、
`CL-EXIT=0`；两个基址漂移变体与 nm shim 重跑后仍是预期结果（日志已刷新）。
未改 `.rs` / `Cargo.toml` / `x86_64.ld`，且 x86_64 镜像不含 aarch64 内核
（`xtask image --arch aarch64` 仍按 `os/xtask/src/image.rs:162-164` 拒绝），
结合上面「ELF 字节相同」的实证，本笔对装机链与宿主测试计数无可达路径；
宿主计数仍为上一笔实测的 809 / 241（本轮未重复跑，理由即此）。所有临时
副本与注入诱饵的中间日志已清理（中间那一格「诱饵行新旧对比」的日志被后面的
四格矩阵取代，已删），工作树只留三个文件的改动。

## P3 M3.3 — boot-shim 在 aarch64 上装载生产镜像（设计要点先行，实现与实测随后追加）

任务书判据（NK4B-TODO §3 M3.3）：AAVMF 下 shim 打印
`boot-shim: kernel loaded (entry staged)` 与 `boot-shim: 12 boot modules loaded`
（两行现成的路标，`os/boot-shim/src/uefi_helpers.rs:179,182`）。
只到「装载完成」，不包括「跳进内核」——那是 M3.4。

### 两个前置事实（先跑再写设计，不凭常识）

1. **12 个装机模块全部能在 `aarch64-unknown-none` 下构建**（逐个
   `cargo build -p <pkg> --target aarch64-unknown-none --release`，全部退出码 0，
   清单见 `evidence/…/m33-probe-modules.txt`）。否则 shim 根本打不出
   「12 boot modules loaded」，本里程碑得先修模块。
2. **内核侧 `arch_boot` 的 aarch64 实现不是桩**
   （`os/kernel/src/lib.rs:223-241`：与 x86_64 同形，走 `AArch64Paging` +
   `AArch64HigherHalf::jump_to_kmain`，并带 NK4-A 的那条 KernelInfo 收进
   .bss 的修复）。意义：shim 过完两个路标后会真的跳进去，M3.3 不拦它也不修它，
   卡在那里属于 M3.4 的正常现场（记录时不得把它当作 M3.3 失败）。

另外：`os/boot-shim/src/uefi_helpers.rs:93-139` 的平台发现路径已经有
`#[cfg(target_arch = "aarch64")]` 分支（优先 DTB、回落 ACPI），
`loader.rs` 与两个路标本身架构中立——所以 M3.3 不是「把 shim 重写一遍」，
而是补齐三处架构专属缺口（下面的决策一/二/三）与装机腿（决策四/五）。

### 决策一：特性门沿用 M3.2 刚证明的那个形态

`[[bin]]` 的 `required-features` 是「全部必须开」语义，两个架构特性并列会
互相否掉。所以抽出内部特性 `fw-uefi-image` 给 bin 要求，`fw-x86-uefi` 与
新增的 `fw-aarch64-uefi` 各自隐含它（与 `os/kernel-image/Cargo.toml` 的
`fw-none-image` 同构，词汇表也对齐仓内 10 个 aarch64 载体已有的
`fw-aarch64-uefi` 命名）。对外零变化：既有调用
（`os/xtask/src/image.rs` 里 `-p boot-shim --no-default-features --features
fw-x86-uefi`）与产物路径不变，宿主 `--workspace --bins` 仍靠
required-features 跳过裸机 bin（X-2/NK5 隔离）。

### 决策二：panic 与 EBS 后的裸打印——先给 PL011 补上节流，再让 shim 复用

现状：`main.rs:119-149` 的 `raw_serial_line` 与 `lib.rs:70-83` 的
`raw_serial` 都是 x86 端口 I/O（COM1 0x3f8，带 LSR 有界轮询），非 x86 上
`raw_serial` 已是空实现（`lib.rs:83`）——后果：aarch64 上 panic 与交棒前最后
几行诊断全部消失，而 shim 的 panic 本实现就是死循环，两者叠加等于整机静默，
正是 NK4-A 花力气拆掉的那个坑。

做法：boot-shim 加 `minix-plat`（`default-features = false`）依赖，非 x86 分支
改用 `os/plat/src/arm64/early_console.rs::write_str`（PL011，基址
`0x0900_0000`，与 QEMU virt 设备一致；x86 分支逐字不动）。**前提**：
`write_byte` 今天是对 `PL011_BASE` 的直接 volatile 写、**不等 TXFF（FR bit5）**，
长字符串会撞满 16 字节 FIFO 丢字；x86 侧同位置是有界轮询的。所以先给它补
与 x86 同构的「有界等待 TXFF 再写」（上限内等不到也照写，不造成死循环），
同步补一条宿主测试。对位：`os/plat/src/arm64/early_console.rs` 自身 + x86
侧 `os/boot-shim/src/main.rs:135-146` 的有界轮询写法。

退路（若 `minix-plat` 在 `default-features = false` 下为 uefi 目标编不过）：
在 shim 内自写与 x86 同形的 12 行 PL011 写入，并在两处注释里互相指名对位；
不另起第三种方案。

约定范围边界：本决策只保证「路标与 panic 能打出来」；`uefi::println!`（ConOut）
只在 EBS 前可用，这一点不变。

### 决策三：入口交接协议不在本里程碑动

`main.rs:66` 的 `minix_kernel::arch_boot(&result.kernel_info, result.root_page)`
在 aarch64 上存在且同签名（`root_page` 对 arm64 就是 TTBR 值）。但「跳过去
能不能活」属 M3.4；M3.3 不新增、不修改交接语义，也不为了让它看起来能动而
加 stub（任务书铁律：不许 stub 制造完成）。两个路标在 `prepare_boot` 内、
EBS 与交接之前（`uefi_helpers.rs:179,182`），所以判据与本决策不交叉。

### 决策四：xtask 把写死的 x86 三元组改为按架构取表

`os/xtask/src/image.rs:162-166` 现在对 aarch64 直接 bail，理由就是本里程碑补的
东西；放行后需参数化三处：shim 的 target 三元组（今天写死
`x86_64-unknown-uefi`）、特性名（写死 `fw-x86-uefi`）、ESP 内的引导文件名
（写死 `EFI/BOOT/BOOTX64.EFI`，aarch64 按 UEFI 默认加载项应为
`BOOTAA64.EFI`），以及 `startup.nsh` 里那一行自动加载脚本（同一文件名参数）。
`Arch` 枚举已有 `module_target()`（`image.rs:49-55`，aarch64 →
`aarch64-unknown-none`），本决策就同处再加两个访问器（shim target / EFI 文件名），
不发明新布局概念。产物目录与布局（`/EFI/minix/{kernel.elf,imgrd,modules/×12}`）
不变；内核镜像构建那一步（`2b`）的目标与特性同样参数化（M3.2 已备好
`aarch64-unknown-none` + `fw-aarch64-none`）。

### 决策五：真机载体 = 新增一个专用脚本，不接 run_all、不改既有冒烟脚本

判据需要 AAVMF 下真跑一次装机盘。抄 `os/qemu-tests/test-timer-irq-aarch64.sh`
的 qemu 参数（**必须 `gic-version=3`**，任务书 §3 的硬事实），再加上已组装好的
ESP 盘镜像与 `-bios` 指向 AAVMF；串口采集方式照抄 x86_64 腿
（`os/qemu-tests/test-cmd-smoke.sh`）的本仓做法。新脚本只断言两行路标，
不断言 rc marker（那是 M3.6）。按任务书 P6 才接线 run_all，本里程碑
不注册进去。注意环境陷阱：M3.2 已知边界 #4（`mktemp` 硬写 `/tmp`，受限沙箱
下会假报），新脚本自己写临时文件时避开这一坑。

### 判据与验收形式

- 宿主可测：`cargo build -p boot-shim --target aarch64-unknown-uefi
  --no-default-features --features fw-aarch64-uefi` 退出码 0；
  `cargo build --target x86_64-unknown-uefi --features fw-x86-uefi` 仍 0；
  `cargo test -p boot-shim --features test-all` 与宿主计数不减（kernel 809 /
  arch 241 基线）；xtask 既有 x86_64 装机路径 `IMG-EXIT=0` 不变；
  新增 `xtask image --arch aarch64` 能产出盘镜像（不再 bail）。
- 真机：AAVMF 串口出现两行路标（逐字），且**两次独立复跑**同形。
- 记录：设计本节 + 实现节追加；FIXLOG 按条（至少覆盖 PL011 节流这一条真修）。

### 风险（可预见的那些）与不可预知项

1. `alloc_root_page()` / `build_memmaps()` 里若有 x86 专属假设（页表页尺寸、
   LOADER_DATA 语义）会在 aarch64 真机上暴露，宿主测不出来——只有真机能定量。
2. `minix-kernel` 进链进 shim：aarch64 目标下 `arch_boot` 及其依赖链能否过
   链接（符号/段属性）未验证；失败则先修构建再谈路标。
3. PL011 在 AAVMF 手里是否已被初始化（固件用过即可用；若需自己写 LCRH/IBRD
   则要补寄存器序列，这不在 `early_console.rs` 现状里）。不预估，真机见。



## M3.3 实现（第 1 步：决策二的前置真修 —— PL011 发送节流）

状态：**DONE**。commit `1a2a8eb61`。设计节里的决策二要求「先给 PL011 补
有界等待，再让 shim 复用 `minix-plat` 的早期控制台」，本步就是那件前置事，
单独成一提交（一逻辑单元一 commit）。

### 改了什么

| 文件 | 改动 | 对位依据 |
|------|------|----------|
| `os/plat/src/early_console.rs` | 新增共享纯控制流 `tx_wait_then_send`（+ 宿主 `mod tests` 三条） | 决策二 |
| `os/plat/src/arm64/early_console.rs` | `write_byte` 改为「先有界等 `UARTFR` bit5 TXFF 再写 DR」，上限 100_000 | x86 侧 `os/boot-shim/src/main.rs:135-146` 的有界轮询 |

### 一处必须写下来的可测性事实（给后续所有 aarch64 修复）

`os/plat/src/lib.rs:32` 的 `pub mod arm64` 带 `#[cfg(target_arch = "aarch64")]`
⇒ **宿主编译单元里不存在该模块**，写在它内部的 `#[cfg(test)]` 永不执行。
同目录 `arm64/interrupt.rs:252` 那个 `mod tests` 就是这种状态：宿主
`cargo test -p minix-plat` 的 4 条全部来自 `x86_64::interrupt`。所以任何想
被宿主测到的 aarch64 逻辑，都必须把判断部分放到 cfg 之外的共享模块里，
寄存器访问以闭包注入——本步就是这么做的（否则只能得到「编译通过、测试为零」
的假收敛）。

### 验证（全部实跑，输出归档）

| 项 | 结果 | 证据 |
|----|------|------|
| `cargo test -j 1 -p minix-plat`（宿主 docker） | 4 → **7 passed**，`PLAT-EXIT=0` | `pl011-host-tests.txt` |
| `cargo test -j 1 -p minix-kernel -p minix-arch` | **809 / 241**（P0 基线持平，不减） | 同上文件后半 |
| `cargo build --target aarch64-unknown-none -p kernel-image --features fw-aarch64-none --release` | `A64-BUILD-EXIT=0`，`early_console` 相关告警 0 条 | `pl011-aarch64-build.txt` |
| `bash check-layout.sh aarch64`（含重新构建 + 13 条布局断言） | `CL=0`（L0–L8 全 PASS） | 同上（本轮未另存，值为 0 已记于此） |
| 反向变异 A：`while tx_full() && false`（= 修复前「完全不等」） | 2 条 FAIL / 1 条 ok（空闲路径两实现同形，本就该 ok） | `pl011-throttle-negative-mutation.log` |
| 反向变异 B：删掉 `if spins >= limit { break; }` | 1 条 FAIL，21.6 秒后死于假设备读计数器 u32 溢出 | 同上 |
| 还原复跑 | 3 passed + `RESTORE-OK`（源文件与变异前备份逐字节相同） | 同上 |

判别性说明（不夸写）：变异 B 在宿主上表现为「计数跑飞后 panic」而不是
「超时挂死」——`FakeUart` 的读计数器是 `Cell<u32>`，debug 构建下先溢出。
真机上同一缺陷的表现是 MMIO 读循环永不退出。两条测试对「先等后写」的时序
断言靠 `reads_at_first_write`（第一次写入前已完成几次读取），只数读/写次数
量不出「先写后等」。

### 顺带量到的一件事（解释 ELF 变小，免得下位读者怀疑构建发虚）

改后 aarch64 `kernel` 工件 1203016 字节，M3.2 记录的是 1205672 字节。
用 `git show HEAD~1` 的两个旧版 `early_console.rs` 重现构建，实测回到
1205672 ⇒ 差值确由本步引入。原因（合理推测，非断言）：release 下
`write_byte` 原本是三指令 MMIO 写，会被内联进内核里每一处早期控制台调用；
现在它含循环与闭包调用，不再内联，调用点从内联体退化为一次跳转，净体积变小。
这是本步唯一的产物字节变化，布局断言（L0–L8）全通过说明契约面未动。

### 划界（本步不做，理由写清楚）

`os/plat/src/x86_64/early_console.rs:64` 的 `com1_write_byte` 至今是
**无上限** `while (inb(COM1_BASE + 5) & 0x20) == 0 {}`——同一类缺陷在生产
内核 x86 路径上仍然留着。不在本步修：它不在 M3.3 关键路径上，x86_64 装机链
已翻绿，动它要按铁律补真机两次复跑，应与 P1 的 x86 侧修复批次一起做。
## M3.3 实现（第 2 步：决策一 + 决策二落码 —— boot-shim 放行 aarch64 UEFI 腿）

状态：**DONE**。commit `5ac5625f1`。

### 改了什么

| 文件 | 改动 | 为什么这样做 |
|------|------|--------------|
| `os/boot-shim/Cargo.toml` | 新增内部特性 `fw-uefi-image = ["uefi"]`，`fw-x86-uefi` / `fw-aarch64-uefi` 各自隐含它；bin 的 `required-features` 改为 `["fw-uefi-image"]`；新增依赖 `minix-plat`（`default-features = false`） | `required-features` 是「全部必须开」，两个架构专属名打不开同一个 bin（M3.2 已踩过）。与 `kernel-image` 的 `fw-none-image` 同形 |
| `os/boot-shim/src/lib.rs` | `raw_serial` 按架构三分（x86 原样 / aarch64 走 plat / 其余空）；`raw_serial_line` 从 bin 搬进来，内层发射器抽为 `emit_byte`（同三分） | 架构分道只需要一个落点，否则 `raw_serial` 与 `raw_serial_line` 各写一遍 cfg |
| `os/boot-shim/src/main.rs` | 删除本文件内的 `raw_serial_line`（其 x86 端口 asm 是无条件的，aarch64 编不过），改为 `use boot_shim::{raw_serial_line, UefiBootShim}` | 同上；panic handler 三处调用点逐字未变 |

`fw-uefi-image` 与 M3.2 那个 `fw-none-image` 有一处实打实的差别：它**隐含
`uefi`**，所以单独打开也自洽，不存在「打开了内部特性但缺依赖」的组合。这条
差别不是设计美学，是 `main.rs` 与 `uefi_helpers` 全部挂在 `#[cfg(feature =
"uefi")]` 上、而 bin 一定要它们——写文档时不能照抄 M3.2 的 caveat。

### 验证（全部实跑）

| 项 | 结果 | 证据 |
|----|------|------|
| aarch64 裸机构建 `--target aarch64-unknown-uefi --features fw-aarch64-uefi --release` | `A64-SHIM-EXIT=0`，工件 `PE32+ executable (EFI application) Aarch64`、924160 字节 | `m33-shim-gates.log` 第 1 节 |
| x86_64 腿（对外入口名未变） | `X64-SHIM-EXIT=0`，`PE32+ EFI application x86-64` | 同上第 2 节 |
| 宿主裸机隔离（X-2/NK5） | `cargo build --workspace --bins` `WS-BINS-EXIT=0`、`eh_personality`/`^error` 关键词 **0 条** | 同上第 3 节 |
| 宿主测试计数 | 默认 14 passed、`--features test-all` 27 passed，均 0 failed | 同上第 3 节 |
| x86 腿零语义变化（脚本对账） | ① `raw_serial` x86 函数体与 HEAD 去空白逐字符相同；② 发射指令行（`asm!`/端口 `0x3f8`/`0x3f9`/LSR bit5/上限 `100_000`）与 HEAD 的 `raw_serial_line` 逐字符相同 | `shim-move-x86-equiv.txt`（`RESULT: PASS`） |
| 反向判别：门的有效性 | 门改回 `fw-x86-uefi` → 删工件后重建**不再生**（`Compiling boot-shim` 0 次）、退出码仍 0；门复原 → 工件再生 | `m33-shim-gate-negative.log` |

### 一次自己造出来的假证据（记录以免下位读者重犯）

第一版反向判别用 `cargo clean -p boot-shim` 造「无工件」现场，跑出来
**正例与负例都有工件**（时间戳与链接数都指向同一次构建）——`clean -p` 没有
删掉 `target/aarch64-unknown-uefi/release/` 下的 `.efi`，于是那节证据什么也
没证明。处理方式：不删日志，在其尾部追加「第 4 节作废」的补记并指向替代品
（`m33-shim-gates.log` 尾部）；重做版改成显式 `rm -f` 工件，并且**先证明
「删了会再生」**再证明「门变异后删了不再生」——少了第一步，第二步的「没有」
可能只是构建系统没动作。附带量到的坑：cargo 对裸机工件用硬链接复用
`deps/` 下的同一份文件（链接数 2、mtime 保持原构建时刻），所以**不能用
`date -r` 的时间戳判断是否重新产出**。

### 顺带排除的一条嫌疑

aarch64 构建有一条 `warning: unreachable expression`（`main.rs:69` 的
`unreachable!()` 跟在 `-> !` 的 `arch_boot` 之后）。重跑 x86_64 腿确认同一条
告警也在——非本笔引入、也不是架构差异（三架构的 `arch_boot` 都是 `-> !`），
不记为缺陷。

## M3.3 实现（第 3 步：决策四落码 —— xtask 装机面按架构取表，放行 aarch64）

状态：**DONE**。commit `2b6d98ffd`。

### 改了什么

`os/xtask/src/image.rs` 的装配计划里有四处把 x86 形态写死：boot-shim 的构建
目标三元组、boot-shim 的特性名、`kernel-image` 的特性名、ESP 内的固件默认加载
项文件名。`plan()` 开头还有一段对 aarch64 的 `bail!`，理由正是「boot-shim 无
`fw-aarch64-uefi` 产出」——那个前提在 `5ac5625f1` 之后已经不成立，所以本笔是同
一处把前提和结论一起换掉。

四处收成一个 `Arch::uefi_slots()` 访问器，返回 `UefiSlots` 结构（表见下），
`riscv64` 返回 `None`，`plan()` 里的 `match` 换成 `let Some(uefi) = ... else {
bail!(按架构 slug 打印原因) }`。这样「不走 UEFI 盘形」这件事只有一处表达，以
后 riscv64（P4）要接装机面是往表里加一行，不是再改一次控制流。

| 架构 | shim 目标 | shim 特性 | kernel-image 特性 | ESP 加载项 |
|------|-----------|-----------|-------------------|------------|
| x86_64 | `x86_64-unknown-uefi` | `fw-x86-uefi` | `fw-x86-none` | `BOOTX64.EFI` |
| aarch64 | `aarch64-unknown-uefi` | `fw-aarch64-uefi` | `fw-aarch64-none` | `BOOTAA64.EFI` |
| riscv64 | 无（honest bail） | — | — | — |

对位关系：加载项文件名不是自创，UEFI 规范按 CPU 架构规定默认加载项，AAVMF 只
认 `EFI/BOOT/BOOTAA64.EFI`；本仓 `os/xtask/src/qemu.rs:32-38` 早已按架构取固件
路径表（AAVMF_CODE.fd / qemu-efi-aarch64），本笔是同一思路用在装机面上。
`kernel-image` 的 `--target` 改用 `Arch::module_target()`：`kernel_elf_path()`
（同文件 `image.rs:142`）取件时用的就是这个三元组，此前只是没人把它接进构建
步骤，两处写死成同一个值所以没暴露。

顺带把打印布局那行（`run()` 末尾）也参数化——它原来无条件打印 `BOOTX64.EFI`，
放行后会对 aarch64 说假话。

### 验证（全部实跑）

| 项 | 结果 | 证据 |
|----|------|------|
| 宿主测试 `cargo test -p xtask` | 10 → **11 passed**，0 failed（新增 `plan_aarch64_switches_every_uefi_slot`） | `m33-xtask-aarch64-image.log` 第 1 节 |
| aarch64 真装机 `xtask image --arch aarch64 --release` | `IMG-EXIT=0`；ESP 内 `BOOTAA64.EFI`（924160 字节）、`kernel.elf` 是 AArch64 ELF（入口 `0xffff800000000000`）、12 模块与 imgrd 全在 | 同上第 2 节（全量日志 `tmp/nk4a/img-a64-1.log`） |
| x86_64 腿不回归 | `IMG-EXIT=0`，ESP 内仍是 `BOOTX64.EFI`（946176 字节） | 同上第 3 节 |
| 判别性变异三条 | loader 名回退 `BOOTX64.EFI`、kernel-image 的 `--target` 回退 x86、boot-shim 的 `--target` 回退 x86——各自都让新测试 `FAILED`（10 passed / 1 failed），还原后回到 11 passed | `m33-xtask-table-mutation.log` |
| 格式与 clippy | `cargo fmt -p xtask -- --check` 干净（该包只有 `image.rs` 需要格式化，不牵动其他文件）；`cargo clippy -p xtask --all-targets` 无本包告警（仅有的 2 条来自 `minix-types`，既有） | `tmp/nk4a/fmt-xtask-host.txt`、`tmp/nk4a/clip-xtask.txt` |

新测试的一条设计取舍值得写下：一开始用 `text.contains("aarch64-unknown-none")`
这种「整串包含」断言，判别性不够——12 个模块构建步都带这个三元组，把
`kernel-image` 那一步的目标写回 x86 也照样「包含」，测不出来。改成按 `-p`
定位到具体那一步、再取它的 `--target` / `--features` 值比对（变异 B 就是为这条
准备的）。

### 与 FIXLOG 的关系

决策四不是修一个已存在的错误行为（x86_64 路径的对外契约一字未变），所以按
FIXLOG 的口径不单独记一条修复；它的全部证据在本节与上表。真修的两条（PL011
节流、boot-shim 的 x86 端口汇编挡住 aarch64 腿）在 FIXLOG 的 M3.3 补记与补记二。

## M3.3 实现（第 4 步：决策五落码 —— AAVMF 载体 + 真机两行路标）

状态：**DONE，M3.3 判据达成**。commit `6e80b5e9c`。

### 判据（NK4B-TODO §3 原文）

> AAVMF 下 shim 打印 `kernel loaded` + `12 boot modules loaded`（现有路标）。

真机串口逐字出现这两行：

```
boot-shim: kernel loaded (entry staged)
boot-shim: 12 boot modules loaded
```

### 新载体 `os/qemu-tests/test-shim-bootmarks-aarch64.sh`

三段式，每段都能单独 FAIL（退出码 0 = PASS / 1 = FAIL / 2 = 前置缺失 SKIP）：

| 段 | 做什么 | 抄谁 |
|----|--------|------|
| 1 装盘 | `xtask image --arch aarch64 --release`；调用方给了 `IMG` 就跳过装盘直接用它 | `test-cmd-smoke.sh` Stage 1 |
| 2 只读验盘 | `mdir` 查 `/EFI/BOOT/BOOTAA64.EFI`、`/EFI/minix/kernel.elf`、12 个模块名 | 同上 Stage 2（含它那条「mdir 会把长文件名撑开成两列，先 `tr -s ' '` 再匹配」的处理） |
| 3 点火等路标 | AAVMF 下拉虚拟机，轮询串口直到两行都出现 | `test-timer-irq-aarch64.sh` 的 qemu 参数 |

三处偏离被抄对象，都在脚本头注释里写了理由：内存取 512M（与 x86_64 冒烟腿
一致，因为这里真装 kernel.elf + 12 模块 + bump 区，timer 载体的 256M 是给不
装东西的测试内核用的）；临时文件落 `target/image/aarch64/` 内而非 `mktemp`
（M3.2 记过的坑：`mktemp` 硬写 `/tmp`，受限沙箱下报假失败）；`-net none` 与
`pkill -f '[q]emu-system'` 前置（任务书铁律）。`IMG` 覆盖口不是为方便而加，
是为了能喂「故意缺件的盘」做反向判别。

按任务书 §6，本脚本**不注册进 run_all.sh**（接线归 P6）。

### 真机取证

| 项 | 结果 | 证据 |
|----|------|------|
| 两次独立复跑 | `RUN=m33d`、`RUN=m33e` 均 `EXIT=0` + `### TEST_RESULT: PASS ###`；两行路标各出现 1 次；EBS 后的裸写腿 `[raw] boot services exited` 各出现 1 次 | `m33-carrier-pass.txt` 第 1 节、`serial_m33d.log`、`serial_m33e.log` |
| 两轮同形 | 串口的 boot-shim/kernel 行（28 行）逐行 diff **无差异** | 同上 |
| 反向判别 N1 | 喂 x86_64 的盘（无 BOOTAA64.EFI）→ Stage 2 `FAIL`，退出码 1 | `m33-carrier-negative.log` |
| 反向判别 N2 | aarch64 盘 `mdel` 掉 kernel.elf → Stage 2 抓到，退出码 1 | 同上 |
| 反向判别 N3 | kernel.elf 名字在、内容换成 10 字节垃圾 → 过了 Stage 2，两行路标都「未出现」，Stage 3 `FAIL` 退出码 1 | 同上 |

N3 是关键那条：它证明「等路标」不是一句走过场的 grep——同一张盘在固件眼里
完全合法（BOOTAA64.EFI 被加载、Shim 真的跑起来了），只在 shim 解析 ELF 失败
处停住，此时脚本必须报 FAIL。

### 超出判据但必须记录的真机事实

1. **决策二的裸机串口在真机可用**：`ExitBootServices` 之后 shim 与内核打印的
   每一行都经 PL011（基址 `0x0900_0000`）落到 `-serial file:`，且带 `^M`
   （CRLF 转换生效）。风险清单第 3 条「AAVMF 是否已初始化 PL011」实测为
   **是**，本仓不需要自己写 LCRH/IBRD 序列。
2. **aarch64 一路走到 `kernel: kmain A/A.2 memmap+modules ok`**：validate 全
   步、step0/step1+2/step4、kmain Phase A 的 A.1/A.1b/A.2a/A.2b 全部打印。
   这已越过 M3.3、进到 M3.4 的地界，取证一次省下位重复劳动。
3. **下一堵墙（M3.4 的起点）**：紧接着一行 panic —
   `platform::init_from_kinfo: no platform source parsed successfully and not
   a dev build (no QemuVirt fallback in release)`，对位
   `os/libs/minix-platform/src/global.rs:250-279`：ACPI/DTB 两条来源都没解析
   出描述符时，dev 构建回退 `QemuVirtDesc`、release 构建直接 panic。
4. **一条文字噪声（非缺陷，记录即可）**：`uefi::println!` 里的省略号 `…`
   在 AAVMF 的 ConOut 上打成 `&`（可见 `loading kernel.elf from ESP&`）。
   只影响日志可读性，不影响任何判据（脚本匹配的都是纯 ASCII 子串）。留待处
   理，不在本里程碑动。

### 上交裁决（M3.4 的架构级选择，本会话不自行定案）

release 装机 + QEMU 载体这条组合下，平台描述符从哪来？三案：

| 案 | 做法 | 代价 |
|----|------|------|
| A | 载体脚本改用 dev 构建（`xtask image` 去 `--release`），走既有 `QemuVirtDesc` 回退 | 最快点电；但 M3.4 验的是 dev 形状的内核，与 P1 的 release 生产链不是同一件产物 |
| B | 生产链补一条「QEMU virt 显式描述符」通道（装机面写入 KernelInfo 或 ESP 上的一个描述符文件，release 也认） | 语义最干净，但要动 KernelInfo 契约位，跨 shim/kernel/platform 三处 |
| C | 让 AAVMF 提供可解析的来源：从 UEFI 配置表取 ACPI（现状取不到 GICR）或改由 `-kernel`/DTB 注入 | 最接近真机语义；被已知事实「AAVMF 的 ACPI GICR 恒 0」直接挡着，得先证实还能不能拿到别的字段 |

推荐 B（A 只作 M3.4 的第一步点电手段，不作为交付形态）。等评审方定案后再进
M3.4 实现。

## M3.3 收口（五决策状态与提交清单）

状态：**DONE**。判据（AAVMF 下两行路标）真机两次独立复跑达成。

| 决策 | 内容 | 状态 | commit |
|------|------|------|--------|
| 一 | boot-shim 抽架构无关内部特性 `fw-uefi-image`，新增对外 `fw-aarch64-uefi` | DONE | `5ac5625f1` |
| 二 | 串口发射按架构分道，aarch64 复用 `minix-plat` 的 PL011；前置真修 = PL011 发送前有界等 TXFF | DONE（真机追加确认：EBS 后裸写腿可用，见第 4 步事实 1） | `1a2a8eb61` + `5ac5625f1` |
| 三 | 入口交接协议本里程碑不动 | 遵守：未动 `arch_boot` 及其交接语义，也未为「看起来能动」加桩 | — |
| 四 | xtask 装机面按架构取 UEFI 四常量，放行 aarch64 | DONE | `2b6d98ffd` |
| 五 | 新增专用 AAVMF 载体，只断言两行路标，不接 run_all | DONE | `6e80b5e9c` |

宿主计数（相对 P0 基线只涨不跌）：`minix-plat` 4 → **7**、`xtask` 10 → **11**、
`boot-shim` 默认 14 / `test-all` 27（持平）、`minix-kernel` **809**、
`minix-arch` **241**（后两项在 `1a2a8eb61` 复测）。

风险清单三条的最终判定：#1（`alloc_root_page`/`build_memmaps` 里的 x86 专属
假设）——真机未暴露，两行路标之后的 memmap 与 KernelInfo 都走完；#2（aarch64
下 `minix-kernel` 链进 shim 能否过链接）——已排除，`boot-shim.efi` 产出并加载；
#3（AAVMF 是否已初始化 PL011）——实测为「已初始化」，无需自写寄存器序列。

下一步：M3.4（内核点电）。起点就是本里程碑取证到的那行 platform panic，且
需先有上面的「上交裁决」定案。
