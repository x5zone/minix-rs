# NK4-C 接手 prompt —— Task C「清零者」追捕 → rc marker 翻绿

> 本文件是给**下一个 agent** 的开场指令。复制粘贴到新会话即可开工。
> 写作时间：2026-09-23（上一会话止于 commit `85a0d7cd8`）。

---

## 0. 一句话任务

**找出「谁在 RS 停车-唤醒窗口内物理抹掉 RS 页表页的 PTE」并修复它，直到串口出现 `minix-rs rc: minimal boot script marker`。**

终目标（不要忘记，但不在本 prompt 的完成判据里）：三架构 OS 在 QEMU 中可运行 + `18-stage-commands` 的程序可运行 + minix3 已迁移测试上机。**Task C 是这三件事的共同前置**——x86_64 的 rc marker 是第一个硬闸门。

你的上下文只有 200k：**每完成一小步就 commit + 写报告**，然后可以放心地"忘掉"细节，从报告恢复。

---

## 1. 先读这些（按序，别跳）

| 顺序 | 路径 | 读什么 |
|------|------|--------|
| 1 | `notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md` | **你自己要维护的报告文件**——已由上一个 agent 预填"当前状态 + 交接来源（根因骨架、布局观测、探针存量、已证伪假设）"，先通读 |
| 2 | `.review/zcode/edge1/FIXLOG.md` **尾部 300 行** | 迭代 27-33 全部取证结论（注意：`.review/` 被 gitignore，只在本地，**不可提交**） |
| 3 | `notes/rewrite/fork-syscall-rewrite/edge_todo.md` 第 1135-1136 行附近 | Task C / rc marker 的登记状态 |
| 4 | `notes/rewrite/fork-syscall-rewrite/NK4A-HANDOFF-STATUS.md` §1.3/§7 | 架构背景（VM handoff、boot 序） |
| 5 | `os/kernel/src/ipc.rs` 中 `impl UserCopy for KernelUserCopy` | 内核直写用户内存的**唯一**生产实现 |
| 6 | `os/kernel/src/vm.rs` 中 `cross_space_copy` / `cross_space_memset` / `cross_space_write` | 内核跨空间写的三个核心（已布防，见 §4） |

**不要读** `minix3/`（C 原版）除非需要行为对位；**不要改** `minix3/`（ground truth）。

---

## 2. 环境与命令（逐条可复制）

### 2.1 构建可启动镜像（宿主构建；docker 缺 uefi target，这是已核实的例外）

```bash
cd /home/xzhao/github/minix-rs/os && ulimit -v 3145728 && cargo run -q -p xtask -- image --arch x86_64 --release
```
- 成功标志：`✅ 镜像就绪：.../target/image/x86_64/minix.img`
- 耗时约 2-4 分钟。**注意 `cd os`**——在仓库根跑会报 `could not find Cargo.toml`。

### 2.2 跑真机（-smp 1 是关键：SMP 噪声会掩盖 Task C）

```bash
mkdir -p /tmp/nk4a && cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/nk4a/vars.fd
cd /home/xzhao/github/minix-rs/os
timeout 150 qemu-system-x86_64 -smp 1 \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=/tmp/nk4a/vars.fd \
  -drive file=target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:/tmp/nk4a/serial_<你的轮次标签>.log -display none -no-reboot -device isa-debug-exit
echo "QEMU-EXIT=$?"
```
- **退出码 124（timeout 杀）是正常的**：panic 后 guest 停机，QEMU 还活着，靠 timeout 收尸。串口日志才是产物。
- 失败签名（当前要复现的崩溃）：
  ```
  nk4a: pf-exit noaddr cr2=0x0
  cause_sig: sig manager 2 gets lethal signal 11 for itself
  kernel exception vector 13 at rip ...
  ```
- 成功签名（Task C 翻绿判据，当前**未**出现）：串口含 `minix-rs rc: minimal boot script marker`。

### 2.3 宿主测试（docker 第一优先，永不跳过）

```bash
docker run --rm --memory=2g --memory-swap=2g -u $(id -u):$(id -g) \
  -v /home/xzhao/github/minix-rs/os:/work -w /work \
  -v /home/xzhao/.cache/minix-rs-docker/cargo-home:/cargo-home -e CARGO_HOME=/cargo-home \
  minix-ci:1.94 cargo test -j 1 -p minix-arch -p minix-kernel -p minix-vm
```
**基线（不得低于）**：`minix-arch 242` / `minix-kernel 809` / `minix-vm 526`，全 0 failed。
先探 docker：`docker info >/dev/null 2>&1 && echo OK`。不可用才回退宿主 `ulimit -v 3145728` + `-j 1`，并在报告里注明。

### 2.4 格式检查（禁止整仓 `cargo fmt`——工具链漂移会污染 diff）

```bash
rustup run nightly rustfmt --edition 2024 --check <你改的每个文件>
```

### 2.5 清理残留 QEMU（**必须用这个写法**）

```bash
for p in $(ls /proc | grep -E '^[0-9]+$'); do
  if [ -r /proc/$p/comm ] && grep -q "^qemu-system-x86$" /proc/$p/comm 2>/dev/null; then kill -9 $p 2>/dev/null; fi
done
```
- **`pkill -f qemu-system-x86_64` 会杀掉你自己的 shell**（模式匹配到当前命令行）——上一会话踩过，浪费了两轮。
- 残留 QEMU 会占住镜像文件锁，表现为 `Failed to get "write" lock`。

---

## 3. 铁律（全是真金白银的教训，违反必翻车）

1. **控制台探针必须带 `#[cfg(not(feature = "mock"))]` 门**。宿主测试没有端口 I/O 控制台，不带门 = 测试进程 SIGSEGV（上一会话踩过：`dm_coverage.rs` 的 dm-cov 探针，导致 kernel 测试整体失败）。
   - 内核侧写法：`use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _}; C0::write_str("nk4a: tag "); C0::write_hex(v); C0::write_str("\n");`
   - VM 侧写法：`crate::bootmark::mark(&alloc::format!("nk4a: tag x={:#x}\n", x));`（带 `#[cfg(not(test))]`）
2. **探针必须去重或按目标过滤，否则 cap 会被启动期重复事件吃光**。上一会话连续三次踩：
   - 48 条同样的 `kdst copy pa=0x3ffc88`（内核栈周期拷贝）吃掉全部额度；
   - `vmpt2` 全量探针的 32 条被启动早期填页耗尽，真正的 refault VA 没采到。
   - **正确做法**：按 `(tag, pa, len)` 或 `(rip, rbx)` 去重（参考 `os/kernel/src/trap_dispatch.rs` 的 `nk4a_rs_trace_probe`），或直接按目标 VA/PA 过滤（参考 `cow_exec_pf.rs` 的 `vmpt2bf`）。
3. **物理布局每轮漂移，绝不跨轮硬编码地址**。已实测：RS 页表根在 0x35fd000 与 0x5e0f000 两种形态间交替；PT 页 PA 在 -smp 1 内也漂 ±0x1000-0x3000（0x1c04xxx / 0x1c05xxx / 0x1c07xxx / 0x1c08xxx）。**所有地址必须取自同一轮串口日志**，用 §7.3 的 python 离线对账。
4. **gdb 硬件观察点在本 QEMU 上不可靠**——三轮 720 秒零命中（翻译与写入自测均正常，就是不触发）。**不要再投入时间**。
5. **一逻辑单元一 commit**；不 push；不用 `git checkout <ref> -- <path>`（会静默冲掉在制改动）；不整仓 fmt。
6. **探针代码一律带注释 `（task1-close 裁决删除）`**，与既有探针风格一致，方便后续统一清理。
7. **每步做完就写报告 + commit**。报告是给"下一个你"看的：写结论、写原始数据、写下一步，不要写"我尝试了"这种流水账。

---

## 4. 当前技术状态（截至 commit 85a0d7cd8）

### 4.1 已修复并验证

| commit | 内容 | 验证 |
|--------|------|------|
| `1d25f433e` | **AP 入口补 EFER.NXE（bit11）** + BSP `enable()` 显式置位 | err=8（保留位）风暴 20+ → **0**；-smp 4 能推进到多 CPU 调度；-smp 1 干净复现 Task C |
| `967a903e7` | 摘除第 11 轮金丝雀探针（它把 `0xDEAD_BEEF_CAFE_0001` 当 RBX 交付 RS——诊断代码写生产上下文） | 交付值恢复真实 |
| `85a0d7cd8` | 四张检测网（见 §4.2） | 全部零命中 |

### 4.2 已排除（不要再重复这些排查）

| 假设 | 排除方式 | 结论 |
|------|----------|------|
| 内核 DM 窗口没覆盖 PT 页 | `dm-cov` 探针打印三源候选 + 窗口裁剪 | PT 页都在覆盖内 |
| VM 与内核用的不是同一棵树 | `sas-send`（VM 发送值）vs 内核 `sa0` root | **一致**（0x35fd000），树无双轨 |
| 跨空间拷贝（copy/memset/write）写进 PT 页 | `kdst` 探针（目标 PA 全打，去重 33 组） | 零命中 |
| IPC 消息投递（`copy_msg_to_user`）写错页 | `msgw` 探针（va/root/pa 三元组 × 64） | 零命中 |
| 分配器双重分配 / 归还 PT 页 / 底层漏斗重用 | `PT_SEEN` 位图三侧检测（`ptalloc-DUP` / `alloc-reuse-PT` / `ptfree-PT`） | **零命中，分配器干净** |
| EFER.NXE 缺失导致 NX 叶保留位违例 | per-CPU EFER dump + 修复后复跑 | 已修复（§4.1） |

### 4.3 已证实的事实（根因的骨架）

1. **PTE 在物理层消失**：故障时内核侧层级 dump 显示 `lvl1=0`（PTE 不存在），而 `lvl2`（PD 项）稳定 —— 不是"没填过"，是**填过又被抹**。
   - 语义提醒：dump 里的 `lvl2pa` 是 **PD 页**，`lvl1pa` 才是 **PT 页**（上一会话一开始看错了一级，白跑一轮）。
2. **抹写窗口 = RS 停车→唤醒之间**（此时只有内核在跑，VM 在 receive 上睡着）。
3. **RS 的 asynsend 表就在 self 指针同一个 VA**：`self = 0x7fffffffc800`（栈上），而 `0x7fffffffc800` 是 RS 栈 PT 覆盖的最后一页——**栈页被抹 → 从栈重装 self 得到 0 → `endpoint_slot` 收到 self=0 → 访问 VA 0 → SIGSEGV**。这条链把"PTE 消失"与"self=0"统一了，是当前最强解释。
4. 交付时序实证（`serial_c31c`）：
   ```
   pre-restore rip=0x203bf0 rbx=0        ← RS 被交付 rbx=0（asynsend 首指令处）
   i33-save   rip=0x203c2d rbx=&msg      ← 同一进程更晚的 int-33 保存，rbx 是活值
   rs-epslot self=0x0 slot=0             ← endpoint_slot 收到 0
   pf-exit noaddr cr2=0x0
   ```

### 4.4 尚未排除的写入者（**你的追捕清单**，按优先级）

1. **内核侧「经当前 CR3 直写用户 VA」的其它站点**。已知 `copy_msg_to_user` 已排除；请**穷举** `os/kernel/src/` 下所有 `write_volatile` / `copy_nonoverlapping` / `write_bytes` 作用在用户 VA 的站点（候选：`syscall_signal.rs` 的 sigframe 写、kerninfo/ps_strings 写、diagctl 缓冲写、`syscall_copy.rs` 的 vumap 系列）。每个未布防的都加 §7.1 同形的 `(va, root, pa)` 探针。
2. **VM 侧页清零路径的 memset 目标 PA**：VM 会把"新页"清零（`alloc_big` 的 63 页 memset、`refill`、`vm_pt_alloc` 的 `write_bytes`）。虽然分配侧已证明干净，但**清零动作本身**没探过——加探针打印 memset 目标 PA，与同轮 PT 页 PA 对账。
3. **跨分配器双记账**：内核 boot bump 分配器（`os/kernel/src/boot_alloc.rs`）与 VM 池（`os/servers/vm/src/phys_mem/`）若对同一段 RAM 各记一份账，会出现"内核以为自己拿到新页、其实那是 VM 的 PT 页"。检查 `kernel_dm_pa_end` / `vm_handoff` 的扣减协议（`build_vm_handoff` 的 `deducted=`）是否覆盖 RS 页表页所在的区段。
4. **VM 的 PTE 写路径本身**：`sync_slot_pte` 的回读检查（`pte-wb-FAIL`）全程静默，说明写后立刻读是好的；但 `update_flags` / `remap` 分支在写旧槽时是否有 off-by-one（写到相邻 PT 页）——可加探针打印实际写入的 PTE 槽 PA。

### 4.5 顺带发现的功能缺口（**记录，但不要现在修**）

`os/kernel/src/ipc.rs` 里生产实现 `KernelUserCopy` 的 SENDA 两个方法是 stub：
- `read_senda_entry` 恒返回 `Err(CopyError::PageFault)`（第 506 行附近）
- `write_senda_result` 是空操作（第 515 行附近）

即**生产路径下 SENDA（asynsend）不投递任何消息**。RS 的 `asynsend` 正是崩溃点的调用。**不要顺手修**（不属于本阶段），但要写进报告；如果 §5 的 S6 修完 rc marker 仍不通，这是首要功能缺口候选。

---

## 5. 步骤梯子（每步一 commit + 一段报告）

> 每步的"验收判据"必须实测，不许口头声称。做完一步 → 写报告 → commit → 再开下一步。

### S0 环境自检 + 复现崩溃（不写代码）
- **做法**：§2.1 构建 → §2.2 跑一轮 → `grep -c "cause_sig: sig manager 2 gets lethal signal 11" /tmp/nk4a/serial_s0.log`。
- **验收**：镜像构建成功；串口出现 §2.2 的失败签名；记录本轮 `root=` 与 `lvl1pa=` 值（这是后续对账的基准）。
- **commit**：`docs(edge1,nk4c): S0 环境自检与崩溃复现（serial_s0 数据）`
- **报告**：`NK4C-WORKLOG.md` §S0：构建命令与输出尾、QEMU 命令、崩溃签名原文、本轮布局（root / PT 页 PA）、耗时。

### S1 穷举内核「写用户 VA」站点（纯阅读，不写代码）
- **做法**：`grep -rn "write_volatile\|copy_nonoverlapping\|write_bytes" os/kernel/src/ | grep -v test`，逐条判断目标是不是用户 VA（用户 VA = 经 `current_root_phys()` 走的进程地址空间）。产出**表格**：文件:行 / 机制 / 是否用户 VA / 是否已布防 / 对应探针名。
- **验收**：表格覆盖 grep 的**全部**命中，无一遗漏；已布防的标出探针名（`kdst` / `msgw`）。
- **commit**：`docs(edge1,nk4c): S1 内核写用户 VA 站点穷举清单（N 处，已布防 M 处）`
- **报告**：§S1 表格 + "未布防清单（按可疑度排序）"。

### S2 逐个给未布防站点加探针（**一处一 commit**）
- **做法**：按 S1 清单顺序，每次只改一处，用 §7.1 的 `(va, root, pa)` 三元组模板；探针名统一 `nk4a: w<站点缩写>`；带 `（task1-close 裁决删除）` 注释；带 `#[cfg(not(feature = "mock"))]`。
- **验收**：每处改完跑 §2.3 docker 测试（三包计数不低于基线）+ §2.4 单文件 rustfmt 检查。
- **commit**：`diag(edge1,nk4c): S2-<序号> <站点> 写入探针（va/root/pa 三元组）`
- **报告**：§S2 每处一行：站点、探针名、宿主测试结果。

### S3 真机取证 + 离线对账
- **做法**：跑一轮（§2.2），用 §7.3 的 python 脚本把**本轮**所有探针的 `pa` 与同轮 `lvl1pa`（PT 页）及 self VA（0x7fffffffc800）对账。
- **验收**：产出一张"探针 × PT 页"命中矩阵；命中 → 进 S6，未命中 → 进 S4。
- **commit**：`diag(edge1,nk4c): S3 真机对账（N 条探针，命中 M 条）`
- **报告**：§S3 原始数据（探针输出行原文）+ 对账矩阵 + 结论。

### S4 VM 侧清零面探针（S3 未命中时）
- **做法**：给 VM 的 memset 站点（`os/servers/vm/src/` 下搜 `write_bytes` / `memset` / `alloc_big` / `refill`）加目标 PA 探针，cap 用去重；同样对账 PT 页 PA。
- **验收**：宿主测试绿；真机对账矩阵。
- **commit / 报告**：同 S2/S3 格式（编号 S4-*）。

### S5 跨分配器双记账检查（S4 未命中时）
- **做法**：静态阅读 `os/kernel/src/boot_alloc.rs` 的区段与 `build_vm_handoff` 的扣减协议；加探针打印内核侧每次分配的 PA 区段，与 VM 池 PFN 段求交集。
- **验收**：给出"有/无重叠"的实测证据（不是推理）。
- **commit / 报告**：同格式（S5-*）。

### S6 命中 → 根因修复
- **做法**：先写**根因分析**（机制链：谁写、何时、为什么写到那、为什么 PTE 变 0），再动手改。修复必须是"消除错误写入"而不是"事后补救"。
- **验收**：**两次独立复跑**（不同 boot，布局不同）都越过 RS step2（串口出现 `rs-epslot self=0x7fffffffc800` 之类的活值，不再出现 `self=0x0`）。
- **commit**：`fix(edge1,nk4c): <一句话根因> —— <修复手段>`（正文含根因链、证据轮次、验证结果）
- **报告**：§S6 根因分析全文 + 两次复跑的串口关键行。

### S7 rc marker 验证（Task C 翻绿判据）
- **做法**：修复后复跑，`grep "minix-rs rc: minimal boot script marker"`。
- **验收**：**两次独立复跑**都出现该 marker。
- **commit**：`docs(edge1,nk4c): rc marker 达成（两次复跑证据）`（若同时有代码改动则合并进 fix commit）
- **报告**：§S7 两次串口证据 + `edge_todo.md` 第 1135-1136 行的状态更新（改为 ✅ 并附 commit）。

### S8 rc marker 未达 → 继续按总计划推进
- **做法**：读 §1 表格第 3 项的 `edge_todo.md` 与总计划（`notes/rewrite/fork-syscall-rewrite/00-master-plan/`），按 **阶段 1.3 → 1.7** 顺序推进：
  - 1.3 rc marker 链（sh 域最小版进 imgrd）
  - 1.4 F10 errno 全仓对账
  - 1.5 核心命令面（echo/ls/cat）
  - 1.6 F3 W^X（boot-shim 段表 → 身份窗口 RX/RW 拆分）
  - 1.7 C 腿 ABI 对账清单
- 每小项独立 commit + 报告。

### S9 交接收尾（**每完成 S6 或 S7 就做**）
- **做法**：更新 `NK4C-WORKLOG.md` 的"当前状态"节（一屏能读完：已修/未修/下一步/风险）；更新 `edge_todo.md` 对应行；把 FIXLOG（`.review/zcode/edge1/FIXLOG.md`，本地、gitignored）追加一段迭代小结。
- **验收**：一个新人只读 WORKLOG 顶部就能接手。
- **commit**：`docs(edge1,nk4c): 交接状态更新（S<当前>）`

---

## 6. 报告与 commit 规范

### 6.1 报告文件

`notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md`（**git-tracked**，会被 commit；已由上一个 agent 预填，你只需**更新顶部"当前状态"并追加你的 S 节**）。

> ⚠️ `.review/zcode/edge1/FIXLOG.md` 在 `.review/` 下，被 gitignore —— 可以写（本地留档），但**不能**作为交接载体（换工作树就丢）。

骨架：

```markdown
# NK4-C WORKLOG（Task C 清零者追捕）

## 当前状态（每步更新，一屏读完）
- 阶段：S<当前步>
- 已修复：<commit 列表 + 一句话>
- 已排除：<清单>
- 下一步：<一句话>
- 阻塞/风险：<一句话>

## S0 <标题>（<日期>，commit <hash>）
### 目标
### 做法（可复制的命令）
### 原始数据（串口片段 / 探针输出原文）
### 结论
### 下一步

## S1 ...
```

### 6.2 commit 规范

- 前缀：`diag(...)` 探针 / `fix(...)` 修复 / `docs(...)` 报告 / `style(...)` 纯格式。
- scope：`(edge1,nk4c)`。
- 标题中文，一句话说清"做了什么 + 结论/根因"。
- 正文（可选但推荐）：根因链、证据轮次、验证命令与结果、基线计数。
- **一次提交只做一件事**；探针与报告可以同一个 commit，但**修复必须单独 commit**。
- 不 push；不 amend 已推送的提交（本仓没有远程推送流程）。

---

## 7. 探针代码模板（直接抄）

### 7.1 内核侧：写入三元组探针（推荐形态）

```rust
// NK4-C S2 取证探针（task1-close 裁决删除）：<站点> 的目标 (VA, 当前 root,
// walk 得到的物理页)。若 pa 落在 PT 页（与同轮 lvl1pa 对账）即错页写实锤。
#[cfg(not(feature = "mock"))]
{
    use core::sync::atomic::{AtomicU64, Ordering as AtomicOrd};
    static NW: AtomicU64 = AtomicU64::new(0);
    if NW.fetch_add(1, AtomicOrd::Relaxed) < 64 {   // ← 必须去重或过滤，见铁律 2
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
// NK4-C S4 取证探针（task1-close 裁决删除）
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

# 本轮所有探针的 (tag, pa, len)
probes = set()
for m in re.finditer(r"nk4a: (w\w+|kdst|msgw) .*?pa=0x([0-9a-f]+)", txt):
    probes.add((m.group(1), int(m.group(2), 16) >> 12))

print("PT pages:", sorted(f"{p << 12:#x}" for p in pt))
hits = [(t, f"{p << 12:#x}") for (t, p) in probes if p in pt]
print("HITS:", hits if hits else "NONE")
```

**用法**：每轮跑完立刻对账，命中就把该轮日志存一份到 `/tmp/nk4a/` 并在报告里贴原始行。

---

## 8. 决策树

```
S0 复现成功？
├─ 否 → 检查构建/QEMU 命令行（§2.1/§2.2），确认 -smp 1；仍失败 → 报告"环境异常"并停
└─ 是 → S1
S3 命中 PT 页？
├─ 是 → S6 修根因 → S7 验证 rc marker
└─ 否 → S4（VM 清零面）→ S5（跨分配器）→ 仍无命中 → 回到 S1 扩充清单（含 VM 侧写用户内存的站点）并重复
S6 修复后两次复跑都越过 step2？
├─ 是 → S7
└─ 否 → 检查是否修错（把修复 revert 回测一轮），写报告说明"假设被证伪"，回 S3
S7 rc marker 出现？
├─ 是 → S9 交接收尾（Task C 翻绿，转阶段 1.3 及以后）
└─ 否 → S8（继续按总计划推进；把 SENDA stub（§4.5）列为候选功能缺口）
```

**卡住怎么办**：不要反复跑同一条路。每轮真机都要带**新的判别信息**（新探针 / 新过滤 / 新对账维度）。如果连续两轮无新信息，停下来在报告里写"当前方法失效，需要 X 级新手段"，并明确列出你试过什么、为什么不行。

---

## 9. 报告质量要求（供上一个 agent 回来接手）

你要假设**读报告的人没有你的上下文**，并且要能在 5 分钟内决定下一步：

1. **原始数据必须贴原文**（串口行、探针输出、命令输出），不要只写"看到了 X"。
2. **每个结论必须能追溯到某轮日志**（写轮次标签，如 `serial_c33f`）。
3. **失败与证伪要写清楚**：试过什么、为什么排除。这比成功更省后来者的时间。
4. **每步结尾写"下一步"**：一句话，带具体命令或具体文件:行。
5. **不要写过程流水账**（"我先看了 A 又看了 B"）；写结论与证据。

---

## 10. 完成判据（本 prompt 的成功标准）

- **Task C 翻绿**：两次独立复跑串口都出现 `minix-rs rc: minimal boot script marker`。
- 或者：**在两次独立复跑中都精确定位到抹写者**（探针实测 PA 落在 PT 页，写出写入者的函数与调用链），并给出修复方案与验证——即使 rc marker 还差后续阶段。
- 无论哪种，`NK4C-WORKLOG.md` 顶部"当前状态"必须能让下一个 agent 无缝接手。
