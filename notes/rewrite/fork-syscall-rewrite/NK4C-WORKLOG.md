# NK4-C WORKLOG（Task C「清零者」追捕 → 三架构 + 命令面 + 测试上机）

> 本文件是**记忆与交接载体**（git-tracked）。长程任务：每完成一个逻辑单元就更新顶部"当前状态" + 追加一节 + commit。
> **顶部状态必须始终是最新的**——用户会在任意时刻让 agent 收尾，接手者只读它 + `git log --oneline -20` 就要能接续。
> 详细取证历史见 `.review/zcode/edge1/FIXLOG.md` 迭代 27-33（**本地文件、被 gitignore、换工作树会丢**——关键结论在本文件 §交接来源有副本）。

---

## 当前状态（每次 commit 前更新，一屏读完）

- **阶段**：S3 ✅ **Task C 根因修复已实施，真机两次复跑（s3a/s3b）判据全过**——RS 走完全部 12 个 endpoint 的 step2，`rs-epslot self=0x7fffffffc800` 全程活值，无 SIGSEGV。下一步：本单元 commit + code-review，然后进阶段 1.3（rc marker 链）
- **根因最终版（S3 定位修正 S2 第 4 点未收敛项）**：`kernel_call_finish` 的 eager 回执直写（errno 非零时把 80 字节回执写到进程表的 `p_delivermsg_vir`）在 C 里只存在于 `kernel_call()`/SYSCALL 腿（system.c:83），且该腿每次入口都先刷新 `p_delivermsg_vir`（system.c:141），目标结构性新鲜；C 的 int33 陷阱腿（proc.c `mini_*`）从不执行这条直写（状态经 h_errno/寄存器，真回执经 MF_DELIVERMSG 投递）。minix-rs 把 int33 腿（含 SENDA）统一接进同一 finish 机器而丢了这条**门纪律**：SENDA 入口按 C 对位故意不刷新 `p_delivermsg_vir`（trap_dispatch.rs 的 `!is_senda` 存储臂），于是 SENDA 窗内的同步 errno 回执落写上一次 SYSCALL 腿调用留下的陈旧地址（帧已弹出、区域已复用）→ self 槽被回执零字抹掉 → `endpoint_slot(0)` → SIGSEGV。完整证据链与修正说明见 S3 节
- **修复（方案甲，门纪律）**：新增 `kernel_call_finish_ipc_door`（int33 腿专用，跳 eager 直写，其余簿记不变）+ `VmSuspendContext.resume_skip_eager_reply` 门标记（IPC 腿挂起的调用被 stage 3a 补完成时同样不写）；详见 S3 节
- **新停点（登记，阶段 1.3 处置）**：修复后 boot 推进到 RS 阻塞在 int33 receive（`rs_flags=0x8`，picknone 全停），旧崩溃点之后的第一个新问题；两轮流片均在 timeout 内无内核 panic
- **⚠️ 复现环境硬约束（S0 发现，仍有效）**：必须用仓库内 `tmp/nk4a/vars.fd`（累积过的 UEFI vars）的副本替换 §2.2 QEMU 命令里的 vars 槽（本文件 S0 节「做法」段已写好完整命令；QEMU 会写它，不要直接用仓库文件本体）；用全新 `OVMF_VARS_4M.fd` 会让 EFI 模块装载落点改变 → 内核 `vm_handoff free n=0` → VM 在 `boot.rs:157` assert panic → 全系统 livelock（比 Task C 更早的死法，签名完全不同；该 fresh-vars 布局鲁棒性 bug 已登记不修）；另 QEMU 命令照 S0 节模板原样跑，自行加 `-machine q35 -m 512` 会导致 QEMU 启动即退（实测）
- **已修复**（commit）：
  - `1d25f433e` AP 入口补 EFER.NXE（bit11）+ BSP `enable()` 显式置位 —— err=8 保留位风暴 20+ → 0
  - `967a903e7` 摘除金丝雀探针（它在污染生产上下文）
  - `85a0d7cd8` 四张检测网（全部零命中，见排除账）
- **已排除**（不要再重复排查）：DM 覆盖 / VM-内核树不一致 / 分配器双重分配·归还·底层重用 / EFER.NXE / gdb 硬件观察点路线 / **PTE 条目被抹写形态**（S2c 实证崩溃窗口内监视 VA 的页表条目全程完好、无 refault，那 48 次 lvl1=0 全是正常 lazy 缺页——『统一解释』的第 1 条骨架需按 S2 结论修正：损的是栈数据，不是页表）/ IPC 消息投递站点 `copy_msg_to_user`·viow（S1-S2 对账无直接命中，真凶是同构的 kernel_call_finish DM 直写，见 S2）
- **新登记（S0 顺带发现，暂不修）**：fresh-vars 布局下 `classify()` 产出 free n=0——与 §4.4 第 3 项「跨分配器双记账」候选直接相关，若后续修复涉及 memmap 扣减协议必须一并验证此场景
- **下一步**：S3 修复 commit + code-review；然后阶段 1.3（rc marker 链：sh 域最小版进 imgrd，`init` exec `/bin/sh`，判据 `minix-rs rc: minimal boot script marker`）；新停点（RS receive 阻塞、picknone 全停）的排查并入 1.3 推进中做；探针保留至 task1-close 裁决
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

## S0 环境自检 + Task C 崩溃复现（2026-09-23，commit 3abed3cbf）

### 目标

构建 x86_64 可启动镜像，`-smp 1` 复现 Task C 崩溃（两轮独立复跑），记录本轮布局基准。

### 做法（可复制的命令）

```bash
# 构建（宿主，docker 缺 uefi target）
cd /home/xzhao/github/minix-rs/os && ulimit -v 3145728 && cargo run -q -p xtask -- image --arch x86_64 --release
# 复现（关键：vars.fd 用仓库累积副本，不用全新 OVMF VARS）
mkdir -p /tmp/nk4a
cp /home/xzhao/github/minix-rs/tmp/nk4a/vars.fd /tmp/nk4a/vars_run.fd
cd /home/xzhao/github/minix-rs/os && timeout 150 qemu-system-x86_64 -smp 1 \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=/tmp/nk4a/vars_run.fd \
  -drive file=target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:/tmp/nk4a/serial_<标签>.log -display none -no-reboot -device isa-debug-exit
```

环境自检全过：docker OK（minix-ci 可用）、cargo 1.94.1、QEMU 8.2.2、OVMF 4M 对在位。

### 原始数据

**第一轮坑（fresh vars，serial_s0a/s0b 两轮签名一致但不是 Task C）**：全新 `OVMF_VARS_4M.fd` 下 EFI 模块装载落点改变（reserved-big base=5eb5000/6d2e000/77ff000，conv=14），内核 `vm_handoff free n=0x0 deducted=0x17` → VM 在 `servers/vm/src/boot.rs:157: BootParams: no free memory regions` assert panic → RS 挂 `BOOTINHIBIT|VMINHIBIT`（picknone rs_flags=0x10200）→ 调度器 idle 循环 livelock（vs 采样器 400 tick 同一 kernel rip）。**这本身是一个布局敏感的鲁棒性 bug，已登记（见新登记）**。

**改用仓库 `tmp/nk4a/vars.fd` 副本后（serial_s0c / serial_s0d 两轮独立复跑，签名一致）**。下面贴 s0c 死亡窗口全序列（从 `rs-epslot` 活值到最后，仅省略与因果无关的 kdst/probe 行，行序保持日志原序）：

```
kernel: vm_handoff free n=0x7 deducted=0x16   （boot-shim: memmaps conv=13 reserved=121）
nk4a: rs-step2 pt=0x2237d8 len=12 tbl=0x7fffffffc800
nk4a: rs-epslot self=0x7fffffffc800 bep=0x7fffffffc800 slot=2   ← 活值（step2 期）
nk4a: rs-epslot self=0x7fffffffc800 bep=0x7fffffffc800 slot=8   ← 最后一次活值
── 抹写窗口（无任何 pick：RS 未让出 CPU，内核正替它办事）──
nk4a: rs-anom pf2 n=0x28 rbx=0x0000000000000000 rip=0x0000000000203bf0   ← PF 入口 ctx 的 rbx 已=0，栈页+文本页均已损
nk4a: pick->0x0000000000000008                                          ← 损伤确认后才轮到 VM 填页
nk4a: sa0-0x0000000000000008 root=0x0x0000000005e27000 cur=0x0x00000000035fd000
nk4a: vm-pf recv
nk4a: vmpt2bf off=0x2bf0 ptroot=0x35fd000                               ← VM 填 asynsend 页
nk4a: vm-pf bytes 000000000048c783  fa=0x203bf0 fa8=4883ec6889f04c8d
nk4a: pick->0x0000000000000002                                          ← 切回 RS
nk4a: pre-restore- rip=0x0000000000203bf0 rsp=0x00007fffffff9d88 r10s=0x0000000000007bff rbx=0x0000000000000000
nk4a: i33-save rip=0x0000000000203c2d rbx=0x00007fffffff9d28 rsp=0x00007fffffff9d18
nk4a: rs-step2 ep=0x0 slot=0                                            ← 栈上 self 已是 0
nk4a: rs-epslot self=0x0 bep=0x0 slot=0
nk4a: pf-exit noaddr cr2=0x0
cause_sig: sig manager 2 gets lethal signal 11 for itself
kernel panic: panicked at kernel/src/syscall_signal.rs:300:13
```

（s0d 同窗口行序一致，仅部分计数器值不同；全量日志在本地 `/tmp/nk4a/serial_s0c.log`、`serial_s0d.log`，仓库 `tmp/nk4a/*.log` 被 gitignore，关键内容以上述摘录为准。）

本轮布局基准（s0c=s0d 同构，两轮 cr3 一致）：RS root=`0x35fd000`（历史两形态之一）、VM root=`0x5e27000`、故障 lvl1pa（PT 页）=`0x1c08000`×42 + `0x1c05000`×3、lvl2pa（PD 页）=`0x35b6000`×44 + `0x1c06000`×3。用户 VA 全部跨轮稳定（`0x7fffffffc800` / `0x203bf0`）。

### 结论

- **Task C 按交接签名稳定复现**（需 repo vars.fd 环境，判据满足：两轮同签名）。
- 抹写窗口收窄（与交接描述不同，本轮实证）：`rs-epslot` 活值（slot=8）→ `rs-anom pf2`（PF 入口 ctx rbx=0，栈与 asynsend 文本页均已损）之间**没有任何进程切换**（无 pick 行）——抹写发生在 RS 持续持有 CPU 期间，即**替 RS 执行系统调用/页故障处理的内核代码**（或 VM 醒来服务本次 PF 的内核代执行段）。VM 的 `vmpt2bf` 填页发生在损伤确认之后，不是嫌疑窗口内动作。这把 §4.4 第 1 项（内核直写用户 VA 站点）的优先级再抬高一级，且提示新线索：**内核在 RS 上下文里的 PF/IPC 处理路径自身就是嫌疑人**。另注意更早的 `rs-anom rst n=27 rbx=0`（rip=0x2266e0 恢复点）说明同类损伤在窗口前已间歇出现，计数器 27→2b 连续，值得回溯 rs-anom 探针语义。
- fresh-vars livelock 是独立的可复现环境敏感性，佐证 §4.4 第 3 项方向（VM pool 扣减与 memmap 的交互在别的布局下会把 free 清单切光）。

### 下一步

S1：穷举内核直写用户 VA 站点（§4.4 第 1 项候选清单：syscall_signal.rs sigframe 写 / kerninfo / ps_strings / diagctl / syscall_copy.rs vumap 系列），逐站点加 §7.1 (va, root, pa) 探针（带去重）；S2 真机对账。

---

## S1 直写站点穷举 + 探针（2026-09-23，commit 见 git log）

### 目标

按 §4.4 第 1 项穷举 `os/kernel/src/` 里所有「内核拿用户 VA 直写」的站点，逐站点加 (va, root, pa) 三元组去重探针（`nk4a: w-<site>`，实现 `trap_dispatch::nk4a_user_write_probe`，CAP=96），另在 `kernel_call_finish` 直写循环前后加 finw/fina 观测；真机对账找抹写者。

### 站点清单（探针已挂）

- `syscall.rs kernel_call_finish`：errno 回执 DM 直写（finw/fina，本次主嫌，已实锤）
- `pte_walk.rs copy_to_user`：SYS_VDEVIO/SYS_SDEVIO 结果回写（viow）；VIRCOPY/SAFECOPYTO 走 `cross_space_copy`，由存量 kdst 探针覆盖
- `syscall_signal.rs`：sigframe 搭建写用户栈
- kerninfo / ps_strings / diagctl 写回点

### 收尾验证（均过）

- docker `cargo test -p minix-arch -p minix-kernel -p minix-vm`：242/809/526 全绿 0 failed（基线同前）
- rustfmt：零新增差异点判据（HEAD 本体不过，探针区按期望归位后净少 1 个差异点）
- clippy：生产形态（`--no-default-features --target x86_64-unknown-uefi`）告警集合与 HEAD 一致
- 顺带修了一个 HEAD 存量破损：`vm.rs` kdst 探针三个调用点的 cfg 门与定义门不一致（`not(test)` vs `not(feature="mock")`），宿主 mock 构建必炸 E0425，已统一为 `not(feature="mock")`

### 下一步

S2 真机复跑对账。

---

## S2 哨兵四代迭代 → 根因锁定（2026-09-23，serial_s2a…s2h）

### 目标

S2a/S2b：两轮独立复跑 + finw/viow 对账。无直接命中（写目标全在正常业务缓冲区）。转入哨兵路线：直接盯被抹的 `self` 存放槽。

### 哨兵演化链（每代被前一代的阴性结果重新定向）

1. **s2c 盯监视 VA 的 PTE 条目**：崩溃窗口内条目全程完好、无 refault → 「PTE 抹写」形态证伪（那 48 次 lvl1=0 全是正常 lazy 缺页），损伤 = **栈数据抹写**
2. **s2d 盯表首字**：`0x7fffffffc800` 是 asynsend 表本体（被 RS 活跃翻动），self 的存放槽在另一栈页
3. **s2e 按值扫描**（`nk4a_pte_watch` 重写版：对 RS 栈三页 `0x7fffffff9000/a000/c000` 扫「值==表地址」的 u64 槽，打包状态变化即打 `pw-<site>`）：**决定性**——`pw-x33 s9=0x1de0`（int33 入口 1 命中@0x9de0）→ `pw-fina s9=0x0fff`（0 命中），抹写锁定在**单次 int33 的内核代执行窗口**（用户栈不动，唯一写者=内核）
4. **s2f/s2g/s2h 现场打印**：fx（直写逐 chunk 打 va/pa/len/w0+尾字 t56/t64，cap 48→4096，48 条会在启动期耗尽——教训：判别窗口需无条件打印+足够 cap）、x33in（int33 入口打 call/r2/旧 p_delivermsg_vir/senda 标志）、pdmv-set（两个存值站点打新旧值）

### 原始数据（s2h 死亡窗口，行序保持；fx 行为节录——省略 `pa=` 字段并缩写十六进制，全量原文见本地 `/tmp/nk4a/serial_s2h.log`）

```
nk4a: pdmv-set krn m_user=0x00007fffffff9da8 old=0x00007fffffff9d88   ← 最后一次存值：同步 kernel_call 存 0x9da8
nk4a: pick->0x0000000000000008 / pre-restore VM / vm-pf recv          ← 调用被 VM 缺页处理停住，RS 未 close
nk4a: pick->0x0000000000000002
nk4a: pre-restore- rip=0x0000000000203bf0 rsp=0x00007fffffff9d88 r10s=0x0000000000007bff rbx=0x0000000000000000   ← RS 被恢复回用户态（原调用帧死亡）
nk4a: pw-x33 n=0x30 s9=0x0000000000001de0                              ← RS 重进 int33：self@0x9de0 尚活
nk4a: x33in call=0x10 r2=0x00007fffffff9d28 old=0x00007fffffff9da8 senda=0x1   ← 本次是 SENDA（不更新 delivermsg，与 C 一致），旧值仍在表里
nk4a: fx va=0x00007fffffff9da8 len=0x50 w0=0xfffffffe t56=0x0 t64=0x7fffffffe138   ← kernel_call_finish 补完成旧调用，80B 直写死帧地址
nk4a: pw-fina n=0x31 s9=0x0000000000000fff                            ← 命中清零：self 被本次写的 +56 处零抹掉
nk4a: pdmv-set krn m_user=0x00007fffffffa258 old=0x00007fffffff9da8    ← 抹写后 RS 才发起下一次调用
rs-step2 ep=0x0 slot=0 → rs-epslot self=0x0 → SIGSEGV → panic syscall_signal.rs:300
```

### 结论（根因链）

1. **抹写者实锤**：`kernel_call_finish` 对陈旧 `p_delivermsg_vir`（0x9da8，存于已弹出的同步调用帧）的 80 字节 DM 直写；写入内容 = errno 回执（m_type=-2），其 +56 处零字正落在 self 槽（0x9de0 = buf+0x38）。
2. **与 C 的双重分叉**：①时机——C 的 VMSUSPEND 停车调用者阻塞在 RTS_VMREQUEST 不返回用户态（system.c:61-69），栈帧必活；minix-rs 停车后把 RS 恢复回用户态继续事件循环，帧死。②长度——C `copy_msg_to_user` 钉死 64B（klib.S:284）；Rust 写 `size_of::<Message>()`=80B（LP64 加宽，minix-types 测试 `test_message_total_size_pinned` 钉死）。两者叠加把陈旧指针的危害从「写回旧缓冲」放大成「踩死复用区」。
3. **SENDA 自身无罪**：`x33in senda=1` 实证窗口那次 int33 不碰 delivermsg（与 C mini_senda 一致），它只是把旧炸弹带进了完成时机。
4. 未收敛：停车后恢复 RS 的具体路径（pre-restore rip=0x203bf0 那条是哪条唤醒语义）与旧调用的补完成站点——S3 代码定位。
   > **S3 修正（2026-09-23）**：本节“补完成旧调用”的归因不准确。逐行对照 s2i 全量日志（见 S3 节）证实：崩溃窗口的 80 字节直写不是停住旧调用的延迟补完成（stage 3a），而是**当前 int33 SENDA 调用的同步完成**；s2h/s2i 窗口里那次“停车→恢复”是正常的需求分页服务回路。真正的分叉只有一条：门纪律（见 S3 节根因）。

### 下一步

S3：定位上述两处代码 → 甲/乙/丙多方案对比（对齐 C 阻塞语义 / 完成时失效陈旧 delivermsg / 写长边界）→ 修复单独 commit → 两次复跑判据（rs-epslot 活值 + 越过 step2）。

---

## S3 根因修复：int33 陷阱腿门纪律（2026-09-23）

### 目标

把 S2 的两个未收敛点在代码里定位到行，多方案对比后修复根因，真机两次复跑达判据。

### 定位过程（读码 + s2i/s3a 全量窗口逐行对照）

1. 读完调度循环 stage 3a（`os/kernel/src/lib.rs` L3716-3824：KCALL_RESUME 消费 → `vm::kernel_call_resume` → `kernel_call_dispatch_inner` → `kernel_call_finish_holding_bkl` → 完成臂 `set_ipc_return_code`+`clear_vm_suspend` 后正常 restore）与 `vm.rs` 的 `memreq_reply`（VM 回复后置 KCALL_RESUME、清 VMREQUEST）：停车调用者的恢复链路本身合规（停排期间不会被选回用户态，s2h 看到的“停车→恢复”是正常需求分页服务回路）。
2. 关键修正来自 s2i 全量日志的配对节奏：SYSCALL 腿的正常完成永远是「`pdmv-set` 紧接同址 `fx`」成对出现（L4201-4227 连续多对，地址轮转 0x9d88/0x9da8/0xa258/0x9bf0，全部新鲜）；而崩溃窗口（下列原文，`pa=` 字段省略）里抹写的 `fx` 紧跟在 `x33in` 之后、**没有任何同轮 `pdmv-set`**——写者不是停住调用的延迟补完成，而是**当前 int33 SENDA 调用的同步 finish**：
   ```
   nk4a: pdmv-set krn m_user=0x00007fffffff9da8 old=0x00007fffffff9d88   ← SYSCALL 腿调用存值
   nk4a: fx va=0x00007fffffff9da8 ... t64=0x000000000026c000                ← 该调用同步完成，回执写同一地址（合法，帧活）
   ……… RS 回用户态，帧弹出，0x9da8 区域被后续帧复用 ………
   nk4a: x33in call=0x0000000000000010 r2=0x00007fffffff9d28 old=0x00007fffffff9da8 senda=0x1   ← int33 SENDA（按 C 对位不刷新 delivermsg）
   nk4a: i33-save rip=0x0000000000203c2d rbx=0x00007fffffff9d28 rsp=0x00007fffffff9d18
   nk4a: fx va=0x00007fffffff9da8 ... t56=0x0000000000000000                ← SENDA 的同步 errno 回执落写陈旧地址
   nk4a: pw-fina s9=0x0000000000000fff                                     ← self@0x9de0 被 +56 处零字抹掉
   nk4a: rs-epslot self=0x0 ... → SIGSEGV
   ```
3. 代码坐实：`trap_dispatch.rs` int33 腿（`x86_ipc_dispatch_body`）对**所有** IPC 调用（含 SENDA）统一走 `dispatch_ipc_entry` + `kernel_call_finish`，后者的 errno 臂无条件 eager 直写 `p_delivermsg_vir`。对照 C：`copy_msg_to_user(p_delivermsg_vir)` 只在 `kernel_call()` 腿（system.c:83，入口 system.c:141 必刷新）；int33 陷阱腿（proc.c `mini_send`/`mini_receive`/`mini_senda`）状态经 h_errno/寄存器返回，从不写调用者消息缓冲。另注意：即使写长是 C 的 64B，抹写点 buf+56 仍在范围内——**长度不是本 bug 的本质，陈旧才是**（方案丙因此降级，见下）。

### 多方案对比

| 方案 | 内容 | 判定 |
|------|------|------|
| 甲（选） | 门纪律：int33 腿的 finish 不做 eager 回执直写；IPC 腿挂起的调用被 stage 3a 补完成时同样跳过（门标记随挂起上下文保存） | 精确对齐 C「copy 只在 system.c 腿」；errno 本就经 RAX 交付（int33 出口已有），直写是重复交付；封死所有 int33 陈旧可达形态（含 parked SENDA 恢复轮） |
| 乙 | finish 写前校验 delivermsg 新鲜度（代际计数），陈旧则按 C 的 WARNING+SIGSEGV 路径 | C 无此机制，是给错误复用机器打补丁；且新鲜场景在 int33 腿本就不该写，乙仍多写一次 |
| 丙 | 写长对齐 C 的 64B | 不解决陈旧（buf+56 在 64B 内照样被抹）；且 Rust 用户侧 Message 本就是 80B，截 64 会丢合法字段。否决 |

### 实施（改动点）

- `vm.rs`：`VmSuspendContext` 新增 `resume_skip_eager_reply: bool`（门标记，含 C 对位注释）；`proc.rs` 两个构造函数 + `proc_table.rs`/`misc.rs` 测试构造共 8 处同步补字段
- `syscall.rs`：`kernel_call_finish_holding_bkl` 新增参数 `eager_reply_copy: bool`；新增 `kernel_call_finish_ipc_door`（release_bkl=true、eager=false，带完整 C 对位文档）；VmSuspend 臂在 `!eager_reply_copy` 时粘性置位门标记；errno 回执块用 `result.reply_code().filter(...)` 门控（`eager_reply_copy && !ctx.resume_skip_eager_reply`）
- `trap_dispatch.rs`：int33 腿改调 `kernel_call_finish_ipc_door`
- `lib.rs`：stage 3a 的 `kernel_call_finish_holding_bkl` 调用点补 eager=true（门归属由 ctx 标记裁决）
- 探针零新增（S1/S2 探针原样保留至 task1-close 裁决）

### 判据（真机两次复跑，同一镜像）

- **s3a**：`rs-step2` 走完 ep=0x0..0xb 全部 12 个 endpoint（旧行为：slot=0 即崩）；`rs-epslot self=0x7fffffffc800` 全程活值；`grep -c "self=0x0 "` = 0；无 panic/SIGSEGV；QEMU 跑满 150s timeout（旧行为提前 panic 退出）
- **s3b**（不同 vars 副本，布局漂移下重复验证）：签名与 s3a 一致，同样全过
- 新停点：两轮最终都停在 RS 阻塞 int33 receive（`rbxw recv-clear` 后 `picknone rs_flags=0x8`）——旧崩溃点之后的新问题，登记待阶段 1.3 处置
- 验证链：docker `minix-arch/minix-kernel/minix-vm` 242/809/526 全绿；宿主 mock 809 全绿；rustfmt 差异数 syscall.rs=105、trap_dispatch.rs=31 均=HEAD；clippy 生产形态 collapsible_if 5 处全存量（arch/boot.rs、trap_dispatch.rs:1180、misc.rs:1036、lib.rs:2803、lib.rs:3797），too_many_arguments 3 处均存量（新参数未触顶：finish_holding_bkl 恰好 7 参数）；`tools/unsafe-audit.sh --diff` bare=0（本次零新 unsafe）

### 下一步

本单元 commit + code-review；然后阶段 1.3（rc marker 链），新停点（RS receive 阻塞、picknone 全停）的排查并入推进。

<!-- 追加示例：
## 1.2-<n> <标题>（<日期>，commit <hash>）
### 目标
### 做法（可复制的命令）
### 原始数据（串口片段 / 探针输出原文）
### 结论
### 下一步
-->
