# NK4-C 电脑迁移交接件（2026-09-30）

> **本文件用途**：用户更换电脑，新机器无 Qoder 会话历史。本文件 + 同目录
> `NK4C-WORKLOG.md`（git-tracked 记忆载体）+ `NK4C-RESUME-PROMPT.md`（开局入口，
> 注意其 §3 frontier 已过时，以本文件为准）三件套即可在新机无缝恢复 NK4-C 永续任务。
> **§末附「新机开场 prompt」**，直接粘进 goal 模式即可续跑。

---

## 0. 迁移即刻事实（本文件写出时刻的权威快照）

- **分支**：`rewrite`；`origin` = `git@github.com:x5zone/minix-rs.git`；
  `minix3-upstream` = Stichting-MINIX 官方（只读 ground truth 源）。
- **HEAD** = `beda55e02`（NK4-C 续-73）。工作树 **tracked 全净**（只有 `tmp/nk4a/*.serial`
  与两个 `NK4C-BUG-AARCH64-VEC-CAP*.md` 等 untracked 取证产物，可安全丢弃或保留）。
- **dm_coverage.rs source-4**：**早已提交入库**（续-51 `b09665415`）。RESUME-PROMPT §3.1
  所谓「未提交在途改动」是 2026-09-29 的过时叙述——**接手者勿被其误导去补验证链**，
  该验证链已在续-51 走完并提交。
- **本机跑过 host 基线复核**（见 §3）：全绿，与续-73 提交时一致。

> ⚠ 新机首次 clone 后**必须**先确认 `minix3/` 子树存在（724MB，是 ground truth C 源，
> 若在 `.gitignore`/子模块外需另行获取）；`os/` 才是 workspace 根。

---

## 1. 三条终目标 · 逐条进度（全部满足才算完）

### 目标① 三架构各自启动并打印 rc marker
串口判据字符串：`minix-rs rc: minimal boot script marker`

| 架构 | 状态 | 阻塞项 |
|------|------|--------|
| **x86_64** | ✅ **达成**（续-73：标准 xtask 启动器 `-smp4` 下 marker=2×3 轮稳定，panic=0/pfVM=0/vec6=0；`-smp1` 亦 marker=2 不回归） | 无（真 SMP 让 AP 跑用户进程是未还债，但**不阻塞 marker**——见 §5） |
| **aarch64** | ❌ 未达 | **失败模式① 间歇 ~4GiB OOM**（毒 `String.len`＝栈地址），见 §4.A |
| **riscv64** | ❌ 未启动真机 | **未接 IPC 桥** + xtask 无 riscv 装配/启动路径，见 §4.B |

### 目标② 18-stage 命令面跑通（echo/ls/cat 为核心）
- **x86_64 核心达成 ✅**：本轮（续-74 取证）从 `tmp/nk4a/nk73b-r1.serial` 确认
  rc 脚本三条命令端到端全通（走真实 VFS IPC 腿）：
  - `echo` → marker 行打印；
  - `ls /bin` → 输出 `cat\necho\nls\nsh`（getdents/readdir 腿）；
  - `cat /etc/rc` → 输出 rc 全文（open+read 腿）。
- 官方冒烟 gate = `os/qemu-tests/test-cmd-smoke.sh`：stage3 等 `entering scheduler`、
  stage4 等 `T4_MARKER`（默认即 marker 串）→ PASS。该脚本用 `-smp 1`。
- 「18-stage」全量 = 328 命令（bin/sbin/usr.bin/...+games+etc），见
  `18-stage-commands/plan.md`；**核心判据是 echo/ls/cat**，已在 x86 达成。其余命令的
  逐个上机验证属扩展项，待 aarch64/riscv 也能 boot 后统一推。

### 目标③ minix3 tests/ 上机
- ❌ 未启动。依赖目标① 三架构先能 boot + 目标② 命令面能跑（多进程 IPC 稳定）。

---

## 2. 关键提交链（新接手者 `git log --oneline` 应见到的尾部）

```
beda55e02 续-73: 过渡守卫钳 schedctl cpu→BSP——x86_64 标准 -smp4 启动器首次稳定达 rc marker
a251ba77c 续-72: 根因坐实+成修——idle() 补 C proc.c:195 AP stop_local_timer 分支，崩溃归零
cd7976b57 续-70: DM per-page guard negative + kernel_call_finish audit clean（第 12 枚探针阴性）
0a94e7dbb 续-69 追加: (A)写腿 IPC-deliver 候选静态证伪
bc1c4d521 续-69: 复现与再定界——最小复现=-smp2
... （续-66~58 是 SMP VM 腐蚀十六轮硬骨头取证链，全 WORKLOG-only + 探针回滚）
b09665415 续-51: boot DM source-4 验证链补完并提交  ← dm_coverage 在途改动在此入土
```

---

## 3. 每轮验证链（硬约束·照抄执行）

**workspace 根在 `os/`，所有 cargo 命令须 `cd os`**（仓库根无 Cargo.toml，从根跑会报
`could not find Cargo.toml`）。

```bash
cd /home/xzhao/github/minix-rs/os

# ① host 基线（只增不减）：minix-kernel 827 / arch 243 / boot 17 / types 309（=569）
cargo test -p minix-kernel 2>&1 | grep 'test result'
cargo test -p minix-arch -p minix-boot -p minix-types 2>&1 | grep 'test result'

# ② clippy 零新告警（基线 136，含 kernel crate 124；用 stash 同上下文公平对比）
cargo clippy --workspace --all-targets 2>&1 | tail -3

# ③ nightly rustfmt 零新增漂移
cargo +nightly fmt --all -- --check 2>&1 | head

# ④ 镜像重建必须 --release
cargo run -q -p xtask -- image --arch x86_64 --release

# ⑤ 真机签名一致：xtask 硬编码 -smp4（qemu.rs:67）
cargo run -q -p xtask -- run --arch x86_64        # 或手改 qemu 命令跑 -smp1
```

**真机判据命令**（xtask -smp4 起 QEMU，串口采集后）：
```bash
tr -d '\0' < serial.log | grep -c 'minimal boot script'   # 期望 ≥1（marker 达）
tr -d '\0' < serial.log | grep -cE 'panic|pagefault for VM|vector 6'  # 期望 0
```

`os/qemu-tests/test-cmd-smoke.sh` 是一键端到端冒烟（装配+ESP 校验+boot+marker）。

**含代码更改的 commit 必走 CodeReview 子代理**；WORKLOG 更新（顶部🛑前沿 + 文末新节）+
commit 用明确文件路径（**禁 `git add -A` / `git add .`**）；探针用后即滚（`git checkout`），
缺页 handler 内严禁页表 walk；未坐实不成修、不臆造未验证生产改。

---

## 4. 两大剩余前沿 · 深度技术分析

### 4.A aarch64 失败模式①（间歇 ~4GiB OOM）

**现象**：aarch64 boot 推进到 shell 装载后，运行时堆分配器收到一个 ~4GiB 分配请求
（`nk4c OOM-RT`，size 值 = 某栈指针低 32 位，如 `0xfffe25c0`），`marker=0`。间歇复现。

**已证伪的全部假设（续-42~45 七轮，勿重翻）**：
- H1 stale-TLB（`set_active_mm` 全量刷 `tlbi vmalle1is` + 父进程同 VA 同毒值反证跨进程污染）
- H2 丢 store（三轮串口字节数完全相同＝纯确定性非弱序竞态）
- H3 池共帧（新帧全生命周期只出 2 次同一 VA，fork 按值复制不共享）
- H5 Q0-Q7 / H6 宽 store 重放携错寄存器 / Q 全 store 面
- 「缺陷 E（FPSIMD 跨陷入不保存恢复）＝模式①因」——静态反汇编证伪：INIT 向量 store 全落
  `[sp]`，堆/池页写全经标量 `memcpy`/`memset` + `str x?`，GPR 被忠实保存恢复，Q 覆写进不了
  `String.len`。（缺陷 E 仍是**独立真缺陷**，值得单开修+CodeReview，但不在本 OOM 关键路径。）

**收敛到的唯一自洽方向 = H7 纯数据流**：某条标量 `str x?` 把「值本身即栈地址」的 GPR 写进
池页的 `String.len`（16B granule，2 相邻字坏）；该 GPR 值来自上游一次「本应读长度却装栈地址」
的错偏移/未初始化读（`MaybeUninit`/`set_len`/`from_raw_parts`）。x86 得零/良性、aarch64 复用帧
保留栈地址 → 解释间歇且架构相关。

**续-45 入口配方**：
1. `rust-objdump -d` 反查那条标量 store 的源 GPR 赋值生产者（IPC 交付缓冲 / Vec 搬迁 /
   结构体解码偏移）；
2. 对照 `minix3` C `stack_utils.c` + `codec` 验「长度读偏移 == 写偏移」；
3. 若偏移一致，转查 `MaybeUninit`/`set_len`/`from_raw_parts` 未写先读；
4. 定位点候选：RS 启动期对池页 `0x250000-0x259000` 调 memcpy/memset 的 caller 回溯，或内核
   IPC 交付 `ipc.rs DELIVERMSG` 代拷贝（按目标 VA 落池 granule 过滤打调用者 PC+内容，区分
   毒值已在交付内容 vs 本地逻辑未初始化读）。

**注意失败模式②（另一正交 bug）**：exec 成功后子进程对首文本页 `0x200000` 无限重复缺页
（`memreq start=0x200000 ok=1` 洪流 ~74015 次）。GLM53-PROMPT 曾专攻②并明确「①本轮不修」。
接手者需先分清当前 aarch64 卡的是①还是②（看串口是 OOM-RT 还是 memreq 洪流）。

**aarch64 真机跑法**：`cargo run -q -p xtask -- run --arch aarch64`（xtask 已含 `-machine
virt,gic-version=3` 契约，qemu.rs:66+）；需 `qemu-efi-aarch64`（OVMF for arm）。

### 4.B riscv64 IPC 桥接入（目标① riscv + 目标②/③ 前置）

**当前实况**（`trap_dispatch.rs:1693-1891`）：
- `riscv64_user_body`：`ecall`（scause==8）时，a7==0 走 `riscv64_kernel_call_leg`；
  **a7!=0 的 raw IPC 腿（SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/KERNINFO/SENDA）直接答
  `-ENOSYS`**（`ENOSYS_CODE=38`，line 1798）——这是 §A.6 登记的 gap。
- `riscv64_kernel_call_leg`（line 1887）仍用 `result.reply_code()`，**未迁 `reply_wire()`**
  （负 errno 线上编码未迁移，x86/aarch64 腿都已迁，见 reviewlog C2 项）。
- 用户侧 `minix-sys/src/ipc.rs` 的 `DirectTrapTransport` 六原语门控 =
  `any(x86_64, aarch64)`，**riscv64 被 CodeReview W3 有意排除**（因内核腿未实现，放宽只会
  弄反符号）。

**riscv64 IPC trap ABI**（`arch_trap.rs:161-174`，已实现用户侧 `ipc_trap`）：
- 入：`a7`(gpr[17])=call_nr、`a0`(gpr[10])=endpoint/操作数1、`a1`(gpr[11])=消息指针/操作数2
- 出：`a0`(gpr[10])=errno（0=OK，负=错误）、`a1`(gpr[11])=次返回（IPC 状态 / kerninfo 页 VA）
- `ecall` **不推进 sepc**，内核体必须 `frame.sepc += 4`（已在 line 1790 处理）。

**架构级差异 = 真正的工作量**（关键！）：
- **aarch64** `aarch64_ipc_dispatch_body`（line 2446）返回 `PARK_NONE`/`PARK_RESCHEDULE`，
  asm epilogue 读该决策：blocked IPC（RECEIVE 无消息）→ `PARK_RESCHEDULE` → 跳 resched thunk
  调度他进程，绝不 eret 回阻塞进程；稍后投递到达经 `finish_and_restore` 唤醒。
- **riscv64 当前无 park 机制**：`DispatchFn = unsafe extern "C" fn(&mut Riscv64TrapFrame)`
  返回 **unit**（`trap_stub.rs:99`）；user leg epilogue（`trap_stub.rs:286-330`）在
  `call riscv64_user_trap_dispatch` 返回后**无条件走 restore+sret**。阻塞 IPC 无法实现。

**实施步骤（decision-complete 配方）**：
1. **改 asm epilogue 支持决策分支**（`os/arch/src/riscv64/trap_stub.rs` user leg）：
   - `riscv64_user_trap_dispatch` 及 `DispatchFn` 签名改为返回 `u64`（PARK_NONE=0/
     PARK_RESCHEDULE=1，对位 `os/arch/src/arm64/trap_stub.rs` 的 PARK_* 常量）；
   - user leg 在 `call` 后检查返回值：==RESCHEDULE 则「切 stvec 回 kernel leg + 释放
     sscratch 交换 + 跳已注册的 resched thunk（重取 BKL 跑 scheduler_loop）」，
     ==NONE 才走现有 restore+sret；
   - 参照 aarch64 trap_stub 的 `EL0BODY`/switch-after-pop 结构。
2. **实现 `riscv64_ipc_dispatch_body`**（新函数，镜像 line 2446 aarch64 版）：
   - 从 `frame.gpr[17]` 读 call_nr，`IpcCall::from_raw` 预解码（未知 → EBADCALL，
     C proc.c:602-606）；
   - `bkl_lock_or_inherit` → `save_frame_to_context`（`caller.trap_style=FullContext`）→
     填 `p_defer.r2/r3`+`p_delivermsg_vir` → `copy_msg_from_user`（EFAULT→SIGSEGV，
     C system.c:152-155）→ 释 BKL → `dispatch_ipc_entry` → `kernel_call_finish_ipc_door`；
   - 有 `reply_code` → 写 a0(errno)+`sync_status_register_to_frame` 写 a1(状态) → PARK_NONE；
     无（NoReply 阻塞）→ PARK_RESCHEDULE。
3. **修 `riscv64_kernel_call_leg`**：`reply_code()` → `reply_wire()`（负 errno 车道，
   对位 aarch64 line 2400），并加 VmSuspend 臂（`save_frame_to_context`+park，见 aarch64
   line 2405-2428）。
4. **`riscv64_user_body` a7!=0 分支**改调 `riscv64_ipc_dispatch_body`，去 `ENOSYS_CODE`。
5. **放宽用户门控**：`minix-sys/src/ipc.rs` 六原语 + `query_kerninfo_page` 的
   `any(x86_64, aarch64)` → `any(x86_64, aarch64, riscv64)`（`kernel_trap` 下）。
6. **装机/启动面**：riscv64 非 UEFI 盘形——xtask `image.rs:195`/`qemu.rs:52` 目前 honest
   bail。启动载体走 **U-Boot fatload + bootelf**（`os/qemu-tests/test-riscv64-uboot.sh`），
   或 OpenSBI+直接 bootelf。真机需装 `u-boot-qemu`/`u-boot-tools(mkimage)`/`dosfstools`/
   `mtools`。x86/riscv 同走内联 `arch_boot`（boot-shim/src/main.rs:86）。
   接入验收清单全文见 `riscv-reviewlog.md` §A（A.2 split_huge/grant_user_walk 缺口只影响
   VM ELF 装载腿、boot 主干不撞；A.5 write_pte_dm 通道门控是单点缺口，与 aarch64 §1.111 同构）。

> **riscv64 是纯实现工作、无未克 Heisenbug**，比 aarch64 OOM 更可预测——若 goal 预算有限，
> **优先推 riscv64 IPC 桥**（步骤 1-6）能最快把目标① 从 1/3 推到 2/3，并顺带打通目标② riscv
> 命令面。aarch64 OOM 需 H7 数据流深挖、不确定性高。

---

## 5. 真 SMP 未还债（不阻塞 marker·x86 已用过渡守卫绕过）

续-73 用 `clamp_cpu_to_bsp`（`sched.rs:307`）在 `sched_proc` 里把 SYS_SCHEDCTL/SYS_SCHEDULE
请求的 cpu 一律钳到 BSP，绕过「INIT 被 schedctl 分到 AP 饿死死锁」。这是**过渡偏离**（对位 C
non-SMP `system.c:686-689` 在 `#ifdef CONFIG_SMP` 内忽略 cpu 参数）。CONTRACT 三件套登记在
`lib.rs` idle() 注释：arm#3（cpu 亲和守卫）已 LAND，**arm#1/#2 未还**：
- arm#1 = `context_stop_idle`（`minix3 arch_clock.c:351-376`：中断入口清 `cpu_is_idle=0` +
  `restart_local_timer`；port `cpu_is_idle` 只置 true 从无清除点）；
- arm#2 = enqueue 唤醒 IPI（`proc.c:1644-1650`：enqueue 到 idle CPU → `smp_schedule`）；
- 且 AP 跑用户进程会触发跨 CPU VM 腐蚀崩溃（续-57~72 十六轮硬骨头未根治）。

三件套 + 腐蚀根治落地后才能移除 `clamp_cpu_to_bsp`。**这是「让 AP 安全跑用户进程」的真 SMP
前沿，非目标①②③ 的当前阻塞**——marker 已能达、命令面已能跑。除非用户要真多核吞吐，否则
不优先。

---

## 6. 关键文件锚点速查

| 主题 | 文件:行 |
|------|---------|
| 过渡守卫 clamp_cpu_to_bsp | `os/kernel/src/sched.rs:307`（调用点 :427-428） |
| idle() CONTRACT 三件套 | `os/kernel/src/lib.rs:3243-3320` |
| AP 唤醒三件套 ground truth | `minix3/minix/kernel/proc.c:190-206`（idle）、`:1644-1650`（enqueue）、`minix3/minix/kernel/arch/i386/arch_clock.c:351-376`（stop_idle） |
| riscv64 IPC gap | `os/kernel/src/trap_dispatch.rs:1784`（user_body）、`:1861`（kernel_call_leg）、`:1718`（ENOSYS_CODE） |
| aarch64 IPC 桥范本（待镜像到 riscv） | `os/kernel/src/trap_dispatch.rs:2446`（ipc_dispatch_body）、`:2375`（kernel_call_leg） |
| riscv64 trap stub asm + DispatchFn | `os/arch/src/riscv64/trap_stub.rs:99`（type）、`:240-330`（user leg） |
| aarch64 trap stub PARK 机制范本 | `os/arch/src/arm64/trap_stub.rs`（PARK_NONE/PARK_RESCHEDULE + epilogue 分支） |
| 用户 IPC 门控 | `os/libs/minix-sys/src/ipc.rs:589,614,640,659,677,699,725`（`any(x86_64,aarch64)`） |
| riscv64 trap ABI | `os/libs/minix-sys/src/arch_trap.rs:161`（ipc_trap）、`:186`（kernel_call_trap） |
| xtask 架构门 | `os/xtask/src/qemu.rs:52,63,67`（-smp4 硬编码 / riscv bail）、`os/xtask/src/image.rs:195-203` |
| 冒烟 gate | `os/qemu-tests/test-cmd-smoke.sh`（-smp1，stage4 等 marker） |
| rc 脚本（echo/ls/cat） | `os/etc/rc` |
| 命令 bin | `os/commands/bin/fileops/src/bin/{echo,ls,cat}.rs` + `proctools`/`shell`/`termctl`/`sysinfo`/`editor`/`diskimg` |
| riscv 接入验收全清单 | `notes/rewrite/fork-syscall-rewrite/riscv-reviewlog.md` §A |

---

## 7. 环境依赖（新机需装）

- **Rust**：stable 1.94.1（active default）+ nightly（`cargo +nightly fmt` 用）。
- **Targets**：`x86_64-unknown-none`、`x86_64-unknown-uefi`、`aarch64-unknown-none`、
  `aarch64-unknown-uefi`、`riscv64gc-unknown-none-elf`。
- **QEMU**：`qemu-system-x86_64` / `-aarch64` / `-riscv64`。
- **固件**：x86/arm OVMF（`/usr/share/OVMF/OVMF_CODE_4M.fd` + VARS；arm 需 `qemu-efi-aarch64`）。
- **mtools**：`mcopy`/`mdir`（ESP 校验用）。
- **riscv 启动链**（做 §4.B 才需）：`u-boot-qemu`、`u-boot-tools`(mkimage)、`dosfstools`(mkfs.vfat)、`mtools`。
- CI 镜像 `minix-ci:1.94-arch` 已含 aarch64+riscv64gc target（若走 docker）。

---

## 8. 新机开场 prompt（保存此段，新机开 goal 模式直接粘）

```text
你是 NK4-C「清零者」永续自主任务 agent（新电脑·无缝续跑）。先读
notes/rewrite/fork-syscall-rewrite/NK4C-MIGRATION-20260930.md（本机迁移交接件·权威状态），
再读同目录 NK4C-WORKLOG.md 顶部🛑前沿段。RESUME-PROMPT.md 的 §3 frontier 已过时（dm_coverage
source-4 早在续-51 commit `b09665415` 提交，勿再补其验证链）。

三条终目标（全满足才算完）：
① 三架构 x86_64/aarch64/riscv64 各打印 rc marker `minix-rs rc: minimal boot script marker`。
   实况：x86_64 已达成（续-73 标准 -smp4 稳定）；aarch64 失败模式① 间歇 ~4GiB OOM 未克；
   riscv64 未接 IPC 桥、真机未起。
② 18-stage 命令面跑通（echo/ls/cat 核心）：x86_64 核心已端到端达成（走 VFS IPC 腿）。
③ minix3 tests/ 上机。

下一前沿优先级：先推 riscv64 IPC 桥（迁移件 §4.B 有 decision-complete 六步配方，纯实现无
Heisenbug，最快把目标① 推到 2/3）；aarch64 OOM 是 H7 纯数据流深挖（§4.A·七假设已全证伪，
勿重翻），不确定性高，量力排期。

硬约束：中文回复；绝不动 AI-chats/daily.todo.md；禁 git add -A/.（只 add 明确路径）；
Ground Truth 优先链 Minix3 C 源 > design doc > Rust 码；每轮验证链 host 基线只增不减
（minix-kernel 827 / arch+boot+types 569=243+17+309）/ clippy 零新告警(基线136) /
nightly rustfmt 零新增漂移 / 镜像 --release 重建+真机签名一致；含代码改 commit 必走
CodeReview 子代理；WORKLOG 更新(顶部前沿+文末新节)+commit；探针用后即滚、缺页 handler 内
严禁页表 walk；code-excellence 全程；未坐实不成修、不臆造未验证生产改。**workspace 根在 os/，
cargo 命令须 cd os**。

难/多轮失败/工作量超预期不是收尾理由。只在三种情况停下问用户：架构裁决级决策 / 破坏性操作 /
终目标三条全满足。goal 模式必须开（跨轮注入）。每轮交付后立即自开下一轮，禁止以「完成一个
探测/取证轮」为停止，禁止把目标改写成更小 subset。立即开始，不要写计划不要询问。
```

---

## 9. 本轮（续-74·迁移前小节点）交付定性

- **纯取证/核实轮，无生产码改**（迁移交接件 = 文档，未 commit 至生产码）：
  - 复核 HEAD `beda55e02` 工作树 tracked 净；
  - 复核 host 基线：minix-kernel **827/0**、arch **243**/boot **17**/types **309**（=569）全绿；
  - 确认 dm_coverage source-4 早已续-51 提交（破除 RESUME-PROMPT §3.1 过时叙述）；
  - **目标② x86 核心达成坐实**：从 `tmp/nk4a/nk73b-r1.serial` 行 7526 marker 之后确认
    `ls /bin`→`cat/echo/ls/sh`、`cat /etc/rc`→全文，echo/ls/cat 三命令端到端全通；
  - 深度分析 riscv64 IPC 桥 gap（§4.B）与 aarch64 OOM H7 方向（§4.A），给出 decision-complete
    续跑配方。
- 迁移交接件落盘 `notes/rewrite/fork-syscall-rewrite/NK4C-MIGRATION-20260930.md`（本文件），
  随主仓 git-tracked，新机 clone 即在。
