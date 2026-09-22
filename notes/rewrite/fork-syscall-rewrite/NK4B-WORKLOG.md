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
| `os/kernel-image/check-layout.sh` | 新增（宿主布局断言 L1–L8） | 决策五 |

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
   分列**——`check-layout.sh` 的 `arch_expect()` 因此有六列而不是设计里说的
   「首段虚拟地址等于内核高半基址」一条通吃。第一版脚本就是按设计写死的
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

- 宿主布局断言：`check-layout-all.log` —— x86_64 与 aarch64 各 L1–L8 全 PASS，
  `CHECK-LAYOUT=PASS（all）`，`CL-EXIT=0`。
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
   今天靠手工调用；它与 `aarch64.ld` 的预期值是两处手写同一批常量，改脚本
   基址时必须同步改脚本预期（没做自动一致性检查，登记为风险）。
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
