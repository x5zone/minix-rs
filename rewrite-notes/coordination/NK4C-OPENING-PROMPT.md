# NK4-C 接手 prompt —— 长程自主任务：Task C 追捕 → 三架构 OS + 命令面 + 测试上机

> 给**下一个 agent** 的开场指令。复制粘贴到新会话即可开工。
> 写作时间 2026-09-23（起点 commit `56d6dec4c`）。
> **本任务不需要逐步向用户汇报**——见 §0.2 的节奏约定。

---

## 0. 任务

### 0.1 终目标（唯一完成判据）

1. **三架构 OS 在 QEMU 中可正常运行**（x86_64 / aarch64 / riscv64 各出现 rc marker）；
2. **`18-stage-commands` 的程序在 OS 上可运行**（marker 后 echo/ls/cat 等核心命令）；
3. **minix3 `tests/` 中已 Rust 迁移的测试在三架构 OS 上可运行**（C 腿 94 项梯子 + Rust 腿核心域 guest 化）。

当前硬闸门 = **x86_64 的 rc marker**，它被 **Task C**（RS `self=0` 崩溃）阻塞。§6 是完整路线图。

### 0.2 工作节奏（重要）

- **长程自主**：一路推进，修完一个 bug 接着修下一个，**不要每步停下来等用户确认**。
- **每完成一个逻辑单元就 commit**（规范见 §5.2），并在 `NK4C-WORKLOG.md` 追加记录。
- **只在 §9 列出的停止条件**（架构裁决级、破坏性操作、终目标达成）才停下来问用户。
- 用户会**在任意时刻手动让你收尾**。因此：**WORKLOG 顶部"当前状态"必须始终是最新的**——接手者只读它 + `git log --oneline -20` 就能无缝接续。
- 你的上下文会被压缩：**WORKLOG 就是你的外部记忆**。压缩后先读 WORKLOG 顶部 + 最近 20 条 commit，再继续。

---

## 1. 先读这些（按序，别跳）

| 顺序 | 路径 | 读什么 |
|------|------|--------|
| 1 | `rewrite-notes/coordination/NK4C-WORKLOG.md` | **你的记忆文件**——已含"当前状态 + 交接来源 + 1.2→1.7 全部记录"（Task C 已修复，当前 frontier = 阶段 1.3 的 449-livelock）。先通读 |
| 2 | `rewrite-notes/coordination/NK4C-REVIEW-REPORT-20260923.md` | 上一轮评审报告：全量提交评审结论、进度口径重列（按计划编号）、下一步建议 |
| 3 | `.review/zcode/edge1/FIXLOG.md` **尾部 300 行** | 迭代 27-33 取证细节（`.review/` 被 gitignore，**只在本地**，不可提交；换工作树会丢，所以 WORKLOG 里有副本） |
| 4 | `rewrite-notes/coordination/edge_todo.md` §A（约 1133-1145 行） | 里程碑状态登记表（你每完成一项要更新它） |
| 5 | `rewrite-notes/coordination/NK4A-HANDOFF-STATUS.md` §1.3/§7 | 架构背景（VM handoff、boot 序） |
| 6 | `os/kernel/src/syscall.rs` 的 `kernel_call_finish_ipc_door` / `kernel_call_finish_holding_bkl` | Task C 修复的门纪律实现（理解 IPC 腿与 SYSCALL 腿的 finish 分叉） |
| 7 | `os/kernel/src/vm.rs` 的 `cross_space_copy/memset/write` | 内核跨空间写三核心（已布防 `kdst` 探针） |

**不要读** `minix3/`（C 原版）除非需要行为对位；**绝不可改** `minix3/`（ground truth）。
`CLAUDE.md` + `AGENTS.md` 是本仓项目规范，动手前扫一眼其中与 review/commit 相关条款。

---

## 2. 环境与命令（逐条可复制）

### 2.1 构建可启动镜像（宿主构建；docker 缺 uefi target，是已核实的例外）

```bash
cd /home/xzhao/github/minix-rs/os && ulimit -v 3145728 && cargo run -q -p xtask -- image --arch x86_64 --release
```
- 成功标志：`✅ 镜像就绪：.../target/image/x86_64/minix.img`；耗时 2-4 分钟。
- **必须 `cd os`**——在仓库根跑会报 `could not find Cargo.toml`。

### 2.2 跑真机（`-smp 1` 是关键：SMP 噪声会掩盖 Task C）

```bash
mkdir -p /tmp/nk4a && cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/nk4a/vars.fd
cd /home/xzhao/github/minix-rs/os
timeout 150 qemu-system-x86_64 -smp 1 \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=/tmp/nk4a/vars.fd \
  -drive file=target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:/tmp/nk4a/serial_<轮次标签>.log -display none -no-reboot -device isa-debug-exit
echo "QEMU-EXIT=$?"
```
- **退出码 124（timeout 杀）正常**：panic 后 guest 停机、QEMU 还活着，靠 timeout 收尸。**串口日志才是产物**。
- 失败签名（Task C 未修时）：
  ```
  nk4a: pf-exit noaddr cr2=0x0
  cause_sig: sig manager 2 gets lethal signal 11 for itself
  kernel exception vector 13 at rip ...
  ```
- 成功签名（Task C 翻绿判据）：串口含 `minix-rs rc: minimal boot script marker`。
- SMP 专项验证另用 `-smp 4`（Task C 修复后回归用）。

### 2.3 宿主测试（docker 第一优先，永不跳过）

```bash
docker run --rm --memory=2g --memory-swap=2g -u $(id -u):$(id -g) \
  -v /home/xzhao/github/minix-rs/os:/work -w /work \
  -v /home/xzhao/.cache/minix-rs-docker/cargo-home:/cargo-home -e CARGO_HOME=/cargo-home \
  minix-ci:1.94 cargo test -j 1 -p minix-arch -p minix-kernel -p minix-vm
```
**基线（不得低于）**：`minix-arch 242` / `minix-kernel 809` / `minix-vm 526`，全 0 failed。
先探 docker：`docker info >/dev/null 2>&1 && echo OK`。不可用才回退宿主 `ulimit -v 3145728` + `-j 1`，并在报告注明。
其它包（`minix-rt` 57 / `minix-sys` 315 / `minix-pm` / `minix-vfs` / `minix-ds`）在改动涉及时也要跑。
三架构 build 门：`minix-ci:1.94-arch` 镜像（含 aarch64-unknown-none + riscv64gc-unknown-none-elf target），每阶段末实跑。

### 2.4 格式与 lint

```bash
rustup run nightly rustfmt --edition 2024 --check <你改的每个文件>   # 禁止整仓 cargo fmt
cd os && cargo clippy -p <crate> 2>&1 | tail -20                     # 零新增告警
```

### 2.5 清理残留 QEMU（**必须用这个写法**）

```bash
for p in $(ls /proc | grep -E '^[0-9]+$'); do
  if [ -r /proc/$p/comm ] && grep -q "^qemu-system-x86$" /proc/$p/comm 2>/dev/null; then kill -9 $p 2>/dev/null; fi
done
```
- **`pkill -f qemu-system-x86_64` 会杀掉你自己的 shell**（模式匹配到当前命令行）——上一会话踩过两次。
- 残留 QEMU 占住镜像文件锁，表现为 `Failed to get "write" lock`。

---

## 3. 铁律（全是真金白银的教训，违反必翻车）

1. **控制台探针必须带 `#[cfg(not(feature = "mock"))]` 门**。宿主测试没有端口 I/O 控制台，不带门 = 测试进程 SIGSEGV（踩过：`dm_coverage.rs` 的 dm-cov 探针，导致 kernel 测试整体失败）。
   - 内核侧：`use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _}; C0::write_str("nk4a: tag "); C0::write_hex(v); C0::write_str("\n");`
   - VM 侧：`crate::bootmark::mark(&alloc::format!("nk4a: tag x={:#x}\n", x));`（配 `#[cfg(not(test))]`）
2. **探针必须去重或按目标过滤，否则 cap 被启动期重复事件吃光**。连续三次踩：48 条同样的 `kdst copy pa=0x3ffc88`（内核栈周期拷贝）吃光全部额度；`vmpt2` 全量探针 32 条被早期填页耗尽，真正的 refault VA 没采到。
   - **正确做法**：按 `(tag, pa, len)` 或 `(rip, rbx)` 去重（参考 `trap_dispatch.rs::nk4a_rs_trace_probe`），或按目标 VA/PA 过滤（参考 `cow_exec_pf.rs::vmpt2bf`）。
3. **物理布局每轮漂移，绝不跨轮硬编码地址**：RS 页表根在 `0x35fd000` / `0x5e0f000` 两形态间交替；PT 页 PA 在 `-smp 1` 内也漂 ±0x1000-0x3000。**所有地址取自同一轮串口日志**，用 §7.3 脚本离线对账。**用户态 VA 跨轮稳定**（`0x7fffffffc800`、`0x203bf0`、`0x216900`），可硬编码。
4. **gdb 硬件观察点在本 QEMU 上不可靠**——三轮 720 秒零命中（翻译与写入自测正常，就是不触发）。**不要再投入时间**。
5. **一逻辑单元一 commit**；不 push；不用 `git checkout <ref> -- <path>`（会静默冲掉在制改动）；不整仓 fmt；不改 `minix3/`。
6. **探针代码一律带注释 `（task1-close 裁决删除）`**，与既有探针同风格，便于后续统一清理（阶段 5 会做「探针大裁决」）。
7. **每步先写报告再 commit**：报告是给"下一个你"和最终接手者看的——写结论、原始数据、下一步，不写流水账。
8. **架构演进必须三处一致标注 `[ARCH: ...]`**（doc + design + code），这是本仓硬规范；遇到需要改变外部契约的设计，先按 §9 停下来问用户。

---

## 4. 当前技术状态（Task C 已修复；frontier = 阶段 1.3 的 449-livelock）

> 详细记录在 `NK4C-WORKLOG.md`（S0-S3、1.3-1.7 各节）与评审报告；此处只留骨架。

### 4.1 已修复（三笔关键）

| commit | 内容 | 验证 |
|--------|------|------|
| `1d25f433e` | AP 入口补 EFER.NXE（bit11）+ BSP 显式置位 | err=8 风暴 20+ → 0 |
| `a470a8d9c`+`6be40748f` | **Task C 根因修复（门纪律）**：int33 陷阱腿的 finish 不做 eager 回执直写（`kernel_call_finish_ipc_door`），门标记 `resume_skip_eager_reply` 随挂起上下文传递 | 三次独立真机验证：`self` 全活值、零 SIGSEGV、RS step2 走完 12 endpoint |
| `fc66eb148`/`6e51b723a`/`d9fc1f649`/`afa850c07`/`5d9d57d0f` | F10b+F10c（SYSCALL 腿数据码分离 + 内核栈 VA 走真实页表）/ F10d（privctl 绕过 RTS 宏）/ F11（VM 请求链双入链死锁）/ F12（minix-rt slab OOM 上限）/ F13（PM↔VFS 握手阻塞 send） | 每笔有真机前后对照与 C 对位 |

### 4.2 Task C 根因（已闭环，供理解）

`kernel_call_finish` 的 eager 回执直写（errno 非零时把 `size_of::<Message>()`=80 字节写到 `p_delivermsg_vir`）在 C 里只存在于 SYSCALL 腿（system.c:83，入口 ：141 必刷新，目标结构性新鲜）；minix-rs 把 int33 陷阱腿统一接进同一 finish 机器丢了门纪律——SENDA 窗口（按 C 对位不刷新 delivermsg）的同步 errno 回执落写已弹出的陈旧栈帧 → 抹掉 self 槽 → `endpoint_slot(0)` → SIGSEGV。
**教训**：交接期「PTE 物理消失」的骨架判断是错的（48 次 lvl1=0 是正常 lazy 缺页）——哨兵实验（盯槽位而非盯推论）才是决定性的。

### 4.3 当前阻塞：阶段 1.3 的 449-livelock（已定性，待修）

boot 推进到第 **449** 轮缺页服务后完全停摆（150s vs 60s 字节级一致 = 硬 livelock）：**10 个进程 PAGEFAULT(to=VM)** + VM 处于 `RECEIVING(from=ANY)` 却永不 rendezvous + 5 内核 task 正常 PROC_STOP。tail-dump（name/to/from）已实锤矛盾形态：**缺页请求标着 to=VM 但未真正入 VM 的 caller 队列/未唤醒**。三个排查入口（按嫌疑排序）：
1. **P1-ipc `clear_ipc_refs`**（`syscall.rs` ≈L1283）：裸 `p_rts_flags.clear(SENDING|RECEIVING)` 绕过 C `RTS_UNSET` 的入队半——与 F10d 同族、与缺页往返强相关（WORKLOG 早登记"1.4 开工优先验证"）。
2. PAGEFAULT 腿 vs VMREQUEST 腿的投递代码路径比对——缺页腿是否漏了 `vm_enqueue_and_notify_vm`？
3. `vm-pf recv` 449 次后新缺页请求是否入队（VM 已在 receive(ANY)，问题在请求侧）。

### 4.4 其它登记（别丢）

- **P1-arch**：aarch64/riscv SYSCALL 腿仍用 `reply_code()` 未取负——阶段 2.1/3.1 开工前必须迁移 `reply_wire()`。
- **P1-guard**：`vm_enqueue_and_notify_vm` 无重复入链运行期守卫（C 有活 assert）——同类故障复发仍是静默死锁。
- **P1-trace**：`do_trace` 裸 set/clear 绕过 rts 协议（仅 trace 场景）。
- **P2-diag**：内核栈→PA 的 `kern_phys_base` 偏移假定散布多处，应收敛 `AddressRef::Process`。
- **fresh-vars 布局敏感 bug**：全新 OVMF vars 下 `vm_handoff free n=0` → VM boot panic（登记不修，但复现必须用仓库 `tmp/nk4a/vars.fd` 副本）。
- **SENDA stub**：`KernelUserCopy` 的 `read_senda_entry` 恒 PageFault / `write_senda_result` 空操作——生产路径 SENDA 不投递（Task C 修复后若 rc marker 仍不通，首要功能缺口候选）。

---

## 5. 工作节奏、报告与 commit 规范

### 5.1 WORKLOG（你的记忆与交接载体）

`rewrite-notes/coordination/NK4C-WORKLOG.md`（**git-tracked**，随 commit 一起提交）。

**每次 commit 前**：
1. 更新顶部"当前状态"（阶段 / 已修复 / 已排除 / 下一步 / 阻塞）；
2. 追加一节记录本次工作。

节模板：

```markdown
## S<n> <标题>（<日期>，commit <hash>）
### 目标
### 做法（可复制的命令）
### 原始数据（串口片段 / 探针输出原文）
### 结论
### 下一步
```

**每完成一个大阶段**（如 Task C 翻绿、阶段 1 全绿）追加一次「交接自检」：假装自己是新来的，只读 WORKLOG 顶部能否接手？不能就补。

### 5.2 commit 规范

- 前缀：`diag(...)` 探针 / `fix(...)` 修复 / `docs(...)` 报告 / `style(...)` 纯格式 / `feat(...)` 新功能。
- scope：`(edge1,nk4c)`。
- 标题中文，一句话说清"做了什么 + 结论/根因"。
- 正文：根因链、证据轮次（`serial_c33f` 这类标签）、验证命令与结果、基线计数。
- **一次提交只做一件事**；**修复必须单独 commit**（不与探针混）。
- 不 push。

### 5.3 每步的验证纪律（不可裁剪）

- 代码改动 → §2.3 docker 测试（计数不低于基线）+ §2.4 单文件 rustfmt。
- 真机判据 → **两次独立复跑**（不同 boot、布局不同）都通过才算过。
- 跨架构改动 → 阶段末跑 `minix-ci:1.94-arch` 的 build 门。

---

## 6. 路线图（完整嵌入；总计划原件在 gitignored 的 `.zcode/plans/`，你读不到）

> 按顺序推进。每项完成 → 更新 WORKLOG + `edge_todo.md` 对应行 + commit。

### 阶段 1：x86_64 翻绿（关键路径）

- **1.1 Task C A 案 [ARCH]** ✅ 已完成（状态寄存器 rbx→r10）。
- **1.2 真机复跑：RS 越过 step2 → init_fresh 完成 → RS main** ✅ 已完成（Task C 根因修复 `a470a8d9c`+`6be40748f`，三次独立真机验证）。
- **1.3 rc marker 链** ← **你当前的位置**。boot 已推进到 12 服务器全 exec、449 轮缺页服务，但**硬 livelock**（§4.3）：修好 PAGEFAULT→VM 投递/唤醒腿后，sh 域最小版进 imgrd、`init` exec `/bin/sh` 达成 marker。
  判据：两次复跑串口出现 `minix-rs rc: minimal boot script marker`。**这是 x86_64 翻绿闸门**。
- **1.4 F10 errno 全仓对账（P0-wire）**：x86_64 部分已由 F10b/c/d 完成；**余项 = aarch64/riscv SYSCALL 腿迁移 `reply_wire()`**（P1-arch）。dispatch 臂 + `reply_code` 映射 + 网关 `reply<0` 检查逐点对齐 C 负 errno，每处判别测试。
- **1.5 P2 命令面**：marker 后核心命令（echo/ls/cat）执行，smoke 扩展，两次复跑。
- **1.6 F3 W^X**：boot-shim 传段表（`KernelInfo` 扩展），身份窗口按节拆 RX/RW。
- **1.7 C 腿 ABI 对账清单（test12 前置①）**：定稿陷入面 ABI（`rax=src/r10=status/rbx=msg/rcx=callnr`、kerninfo rbx、crt0 handoff：kerninfo 页 + 栈 + 参数）——C 陷阱桩与 crt0 按此实现。
- **阶段 1 附属登记**（不单独占阶段，随关联阶段清偿）：P1-guard（VM 链重复入链运行期守卫）、P1-trace（do_trace 裸 rts 绕过）、P2-diag（内核栈→PA 偏移假定收敛）。

### 阶段 2：aarch64（M3.4 → M3.6）

- **2.1 M3.4 B 案**：显式描述符通道（ESP 描述符文件 vs KernelInfo 扩展，设计先行后定）；`check_gic_madt` 适配；`init_from_kinfo` release 面认之。
- **2.2 内核读用户内存丙案 [ARCH 注记]**（riscv64/aarch64 共用）：`KernelUserCopy` 改 VA→PA 走 DM 窗口（`ipc.rs:452/:473` + 跨页分段）。
- **2.3 aarch64 生产 U-mode trap 腿**（NK3 载体 VBAR EL0 形状接入 `init_protection`）。
- **2.4 M3.5 VM handoff → M3.6 aarch64 rc marker**。

### 阶段 3：riscv64（M4.4 → M4.5，甲案）

- **3.1 甲案**：kernel-image riscv64 接 `a1` DTB → 解 memmap + 模块装载源 + `.bss` 清零 → 调 `arch_boot`（过 validate 真门槛）。
- **3.2 SUM 丙案**（= 2.2 的 riscv64 半）。
- **3.3 riscv64 生产 U-mode trap 腿**（sscratch 交换腿）。
- **3.4 VM handoff → M4.5 riscv64 rc marker**。

### 阶段 4：测试全量上机

- **4A C 腿基建**（三架构各一套，固定成本）：LP64 头适配（滚动，按域拉入）；三架构陷阱桩（按 1.7 清单对位 minix-sys 陷入面）；crt0 接 minix-rt handoff；clang 交叉构建胶水（sysroot）。验收 = 串口 marker（沿用 rt-birth 模式）。
- **4B 领域梯子**（x86_64 先行，每波完成即三架构跟随）——按服务器前提排波：
  - W1 PM/VM 基础：test12 → test1 → test6 → test44
  - W2 文件系统语义（27 项最大价值区）：test4 → test14-36 → test43/50/54/55/58/61/70/78
  - W3 信号：test5/37/41/53 → test38/68
  - W4 exec/spawn/权限：test10/11/84/86/65/46
  - W5 pipe/select：test7/8/13/19/20/29/40/52/79
  - W6 mmap 高级：test64/75/87
  - W7 tty/termios：test74/77（前置 C-23 termios wire + tty 面）
  - W8 时间：test69
  - W9 UDS：test56
  - W10 网络 INET：C-24 smoltcp 接入 → test48/67/76/80/81
  - W11 块设备：test85
  - W12 SysV IPC：test88
  - W13 ptrace：test42（1509 行套件最大单文件）
  - 边界：test57/62/47（i386 专属）x86_64 适配决策；**test82（外网）永久排除**出自动跑
- **4C Rust 腿核心域 guest 化**：五域优先（进程/信号/凭证/管道 select/目录链接），真 IPC 面对真服务器，DIAGCTL marker。
- **4D 每波 × 三架构判据脚本**；宿主 33 测试保持 CI。

### 阶段 5：收尾清账

- E5(a)-(g) 真机半点亮 + E-KERNINFO 桩翻转 + NS5-B；E-VMTLB 非 SMP 余件；**task1-close 探针大裁决**（删除所有 `nk4a:` 探针）；全账本销账 + FIXLOG/账本终版。

### 不在本计划（保持登记，不要开工）

E5-SMP / NK6 X-8（SMP 波）；E5(h) + E-DMWIRE + C-17（devman）；C-26（sffs）；C-21/C-22（随批次）；test82。

---

## 7. 探针与对账模板（直接抄）

### 7.1 内核侧：写入三元组探针

```rust
// NK4-C 取证探针（task1-close 裁决删除）：<站点> 的目标 (VA, 当前 root,
// walk 得到的物理页)。若 pa 落在 PT 页（与同轮 lvl1pa 对账）即错页写实锤。
#[cfg(not(feature = "mock"))]
{
    use core::sync::atomic::{AtomicU64, Ordering as AtomicOrd};
    static NW: AtomicU64 = AtomicU64::new(0);
    if NW.fetch_add(1, AtomicOrd::Relaxed) < 64 {   // ← 必须去重/过滤，见铁律 2
        use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
        let root = crate::current_root_phys().map(|r| r.0).unwrap_or(0);
        let pa = minix_arch::CurrentPteWalk::walk(minix_types::PhysBytes(root), va)
            .map(|(pa, _)| pa.0 & !0xFFF)
            .unwrap_or(0);
        C0::write_str("nk4a: w<tag> va=");
        C0::write_hex(va.0);
        C0::write_str(" root=");
        C0::write_hex(root);
        C0::write_str(" pa=");
        C0::write_hex(pa);
        C0::write_str("\n");
    }
}
```

### 7.2 VM 侧：带过滤的探针

```rust
// NK4-C 取证探针（task1-close 裁决删除）
#[cfg(not(test))]
if <过滤条件，例如 proc_endpoint.0 == 2 && fault_addr.0 == 0x203bf0> {
    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
    static N: AtomicUsize = AtomicUsize::new(0);
    if N.fetch_add(1, AtomicOrd::Relaxed) < 48 {
        crate::bootmark::mark(&alloc::format!("nk4a: <tag> x={:#x}\n", x));
    }
}
```

### 7.3 离线对账脚本（探针 PA vs 同轮 PT 页）

```python
import re, sys
txt = open(sys.argv[1], errors="replace").read()   # 本轮串口日志

# 本轮 PT 页 PA（来自内核 pf 层级 dump；注意 lvl1pa 才是 PT 页）
pt = {int(m, 16) >> 12 for m in re.findall(r"lvl1pa=0x([0-9a-f]+)", txt) if int(m, 16)}

# 本轮所有探针的 (tag, pa)
probes = set()
for m in re.finditer(r"nk4a: (w\w+|kdst|msgw) .*?pa=0x([0-9a-f]+)", txt):
    probes.add((m.group(1), int(m.group(2), 16) >> 12))

print("PT pages:", sorted(f"{p << 12:#x}" for p in pt))
hits = [(t, f"{p << 12:#x}") for (t, p) in probes if p in pt]
print("HITS:", hits if hits else "NONE")
```

每轮跑完立刻对账；命中就把该轮日志留档并在 WORKLOG 贴原始行。

---

## 8. 决策树（当前位置：阶段 1.3 的 449-livelock）

```
复跑一轮（§2.2），确认尾态仍是「449 轮 + 10×PAGEFAULT(to=VM) + VM RECEIVING(from=ANY)」？
├─ 否（形态变了）→ 先按新形态定性（WORKLOG「当前状态」对照），再入下述分支
└─ 是 → 修 PAGEFAULT→VM 投递/唤醒腿，按嫌疑顺序：
   ① P1-ipc clear_ipc_refs（syscall.rs ≈L1283 裸 clear 绕过入队）
      → 对位 F10d 修法：换调度器感知 rts_set/rts_unset；对照 C clear_ipc（proc.h RTS_* 宏语义）
   ② 比对 PAGEFAULT 腿与 VMREQUEST 腿的投递代码路径（缺页腿漏 vm_enqueue_and_notify_vm？）
   ③ 查第 449 轮前后 VM 队列状态（vm-pf recv 封顶时新请求是否入队）
修复后两次复跑？
├─ 越过 449 且继续推进 → 遇到下一个停点就按同法处理（每个停点：定性 → 定位 → 修 → 两次复跑）
│   直到 rc marker 出现 → 阶段 1.4（补 aarch64/riscv reply_wire）→ 1.5 → 1.6 → 1.7 → 阶段 2 → 3 → 4 → 5
└─ 仍卡 449 → 修复 revert 回测，写"假设被证伪"，换 ②/③ 入口
rc marker 仍不通（livelock 已解但 marker 链断）？
└─ 查 §4.4 的 SENDA stub（生产路径 asynsend 不投递）与 rc 链组件缺口（WORKLOG 1.3 开局节已盘点：
   缺 /bin/sh 进 imgrd——`xtask image` 的 generate_etc_proto 只播 /etc/{rc,ttys}）
```

**卡住怎么办**：不要反复跑同一条路。每轮真机都要带**新的判别信息**（新探针 / 新过滤 / 新对账维度）。连续两轮无新信息 → 停下，在 WORKLOG 写"当前方法失效，需要 X 级新手段"，列清试过什么、为什么不行，然后**换一个方向**（不要在同一方向继续消耗）。

---

## 9. 停止条件（**只有这三种情况才停下来问用户**）

1. **架构裁决级**：需要改变外部契约、需要 `[ARCH: ...]` 三处标注的设计决策（如阶段 2.1 的 M3.4 描述符通道方案选择、阶段 2.2/3.2 的 KernelUserCopy 改法）。
2. **破坏性操作**：删除/覆盖非自己创建的文件、重置分支、force push、清理他人工作树。
3. **终目标达成**（§0.1 三条全部满足）：此时停下来交付总结。

其余情况（bug 难、多轮失败、工作量超预期）**都不是停止理由**——换方向继续，并保证 WORKLOG 随时可接手。

---

## 10. 完成判据

- **Task C 翻绿**：两次独立复跑都出现 `minix-rs rc: minimal boot script marker`；
- 或**在两次独立复跑中精确定位抹写者**（探针实测 PA 落在 PT 页，写出写入者函数与调用链）并给出修复与验证；
- 之后按 §6 继续推进到终目标（§0.1 三条）。

无论推进到哪一步，`NK4C-WORKLOG.md` 顶部"当前状态"必须始终能让接手者（用户或上一个 agent）在 5 分钟内无缝接续。
