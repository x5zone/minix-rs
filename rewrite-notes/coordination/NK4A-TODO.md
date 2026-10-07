# NK4-A TODO — qwen 接手任务书（2026-09-22，迭代18 后）

> 本文件是 NK4-A 生产启动链翻绿工作的**执行任务书**，配合同目录
> `NK4A-QWEN-OPENING-PROMPT.md`（开局 prompt）使用。维护纪律：
> 每完成一个 Task，把本文件 §4 对应条目的复选框打勾并注明 commit；
> 逐任务工作记录写到 `NK4A-QWEN-WORKLOG.md`（模板见 §6）。

---

## §1 当前状态快照（2026-09-22，迭代18 后）

- 分支：`rewrite`；本轮起点 = `4a6570d7f`（迭代18），证据归档 = `e4c6e8224`。
- 宿主测试基线（**任何修改后计数不得下降**，允许增长）：
  - minix-kernel **808** / minix-arch **241** / minix-vm **525**
  - minix-rs（RS 服务器）**350** / minix-rt **57** / minix-sys **315**
- 冒烟判据（翻绿唯一标准）：串口出现
  `minix-rs rc: minimal boot script marker`（冒烟脚本
  `os/qemu-tests/test-cmd-smoke.sh`，T4 窗口）。
- 已保全、**禁止触碰**：分支 `nk4a-agent-wip`、`nk4a-review-docs`；
  目录 `minix3/`（C 源真值，只读）；`.review/zcode/edge1/FIXLOG.md`
  （只允许追加，禁止改写历史条目）。

## §2 已完成（迭代11-18，六修复，全部已提交）

| # | commit | 一句话 |
|---|--------|--------|
| 11 | 6a7d513d0 | delivermsg 成功臂补返回码 OK（C proc.c:290-292）——VM 醒后 receive 拿到正确返回值 |
| 12 | 13a00244e | RECEIVE 序言补 IPC_STATUS_CLEAR（proc.c:581）——SYSTEM 通知的状态字不再被消息指针污染 |
| 13 | 835e1569f | 调度器 KCALL_RESUME 重派改走 dispatch_inner——BKL 自死锁修复 |
| 14 | 27f0a038a | memreq 读路径真正映射页面（handle_pagefault 逐页）+ 挂起统一出队（RTS_SET 出队半） |
| 15 | 72b4415d2 | exec 数据段 region 补 WRITABLE（ELF PF_W 派生）——.data 可写 |
| 16 | 5d3d8147c | boot 栈 region 扩为 DEFAULT_STACK_LIMIT 4MiB（C main.c:357 对位） |
| 17 | 1277909e1 | minix-rt 堆池 16 页 → 256 页（RS 256KB 分配 OOM） |
| 18 | 4a6570d7f | SENDA 提到 do_ipc 权限层之前（C proc.c:673-684 switch 顺序）——RS_INIT asynsend 不再 EINVAL |

详细根因链见 `.review/zcode/edge1/FIXLOG.md` 的「Fix #9 迭代11-18 补记」。

## §3 精确前沿（c16a 轮真机实证，非推测）

boot 推进到：**RS 已深度运行自己的 main**——
- RS 内核调用流水打到 kc45（kc 探针 48 上限内）；DIAGCTL 诊断通道工作；
- RS 逐页缺页-填充链真实工作：pf 计数到 #19，每次 fault 的 cr2 都不同
  （代码页 0x208a90/0x20c6a0/0x211520、数据页 0x22b000/0x22c000/0x22d000、
  栈页 0x7fffffffb1f0——真实执行推进，非死循环）；
- VM 累计收到 160+ 次 VM_PAGEFAULT 并成功填充（vm-pf recv + bytes）。

**当前停滞点**：最后一次 `nk4a: vm-pf recv` 打印后**没有**跟随
`nk4a: vm-pf bytes`（成功路径打印），RS 停在 `RTS_PAGEFAULT`（picknone
rs_flags=0x400），VM 阻塞，系统静默（PIT 采样器只见到调度器 rip），无 panic。

即：VM 的 `handle_pagefault`（`os/servers/vm/src/cow_exec_pf.rs:25`）走了
某个**不打印 bytes 的出口后系统死等**。已知出口：
- `Ok(Suspended)`（NeedVfsIo → enqueue_fdio 后返回）——**头号嫌疑**：
  VM 把页挂到 VFS I/O 队列后返回主循环阻塞 receive，但 VFS 尚未启动，
  永远等不到完成消息 → RS 永远停在 PAGEFAULT；
- `Ok(AccessViolation)` / `Err(_)`——应走 SIGSEGV（会 panic 可见），已基本排除；
- VM 在 handler 内部死循环/分配失败静默——次级嫌疑。

另有一个**时序敏感未决**：c15a 轮曾在 step2 报 endpoint 0（PM）槽缺失，
c16a 同代码未复现。已埋带端点号的 panic 诊断（boot.rs:1054）+
`RProcTable::indexed_endpoints()`。**若复现，按 §4 Task B 处理；不主动追**。

## §4 任务分解（按序执行；每个 Task 的粒度=一次可验证的交付）

### Task A — 定位并修复 VM 页故障服务停滞【当前最高优先】

目标判据：RS 不再停在 PAGEFAULT；vm-pf recv 与 vm-pf bytes 成对出现；
RS 继续向 boot 完成推进（kc 流水增长）。

- [x] **A1 读代码**：读 `os/servers/vm/src/vm_server.rs` 1740-1860
  （VM_PAGEFAULT 臂：`vm-pf recv` 打印 → `handle_pagefault` →
  `probe_ok` → `vm-pf bytes` 打印 → 结果分派）。列出所有
  **跳过 bytes 打印**的出口（每个出口注明行号与返回值去向）。
- [x] **A2 读子层**：读 `os/servers/vm/src/cow_exec_pf.rs` 的
  `handle_pagefault` + `sync_slot_pte` + `enqueue_fdio`；再读
  `os/servers/vm/src/memtype.rs` 中 ANON 的 `ev_pagefault`——
  回答：ANON 区域能否返回 `NeedVfsIo`？哪些 memtype/路径会返回 Suspended？
- [x] **A3 查帧池**：找出 VM 生产形态的 `PageFrames` 池大小与
  `alloc_pfn` 失败路径（VmContext 初始化处，搜 `page_frames`/`page_alloc`
  的构造）。估算当前消耗（12 个 boot 模块 exec 时**每段逐页 eagerly
  物化**——见 vm_server.rs exec_bootproc 物化循环——加 RS 运行期按需填充），
  判断枯竭是否可能。把池大小数字写进 WORKLOG。
- [x] **A4 加探针**：在 A1 列出的每个非成功出口加**限次探针**
  （模式见 §5.4），打印：出口类型、fault 地址、`page_alloc` 剩余帧数
  （若可取）。命名 `nk4a: pf-exit <类型> cr2=…`。
- [x] **A5 复跑**：重建镜像 + QEMU（命令见 §7）。预期：停滞时刻串口
  出现 pf-exit 行 → 停滞原因定性。**把定性结论写进 WORKLOG 再动手修**。
- [x] **A6 修复**：按定性结果修。对照 C 语义（`minix3/minix/servers/vm/`
  的 pagefaults.c / main.c 对应路径）写明对位关系。一次修复一个 commit。
  - 若是 Suspended-等-VFS：boot 期 ANON 页不应走 VFS——查该页所在
    region 的 memtype 为何不是 ANON（exec/栈安装是否漏设），或
    NeedVfsIo 判定条件过宽；修最小处。
  - 若是帧池枯竭：对照 C 的 alloc_cycle 补充机制或扩池，附容量计算。
- [x] **A7 回归验证**：宿主 minix-vm/minix-kernel 全绿（计数不减）+
  真机复跑 2 次（PIT 抢占时序敏感，**单次通过不算数**）。
- [x] **A8 FIXLOG + WORKLOG**：按 §6 格式补记。

### Task B — step2 槽缺失复现定性【条件触发：A 修复后若出现 boot.rs:1054 panic 才做】

- [ ] **B1**：panic 信息里有端点号与 indexed 列表。对照 step1 循环
  （boot.rs `step1_set_attrs`）找出该端点为何没被 activate（跳过条件/
  lookup 失败/duplicate 覆盖），把分析写进 WORKLOG。
- [ ] **B2**：根因若是时序（PIT 抢占打断 step1 的某个内核调用），
  对照 C：C 的 RS boot 期是否可被抢占、内核调用挂起后 RS 状态如何恢复。

### Task C — RS boot 完成推进【A 完成后】

目标判据：串口出现 RS 完成 fresh boot 的迹象（`server.init(Fresh)` 返回
Ok——RS 的 step0-4 全过；kc 流水里出现 step3/step4 的调用；RS 进入
`run()` 主循环阻塞在 sef_receive）。

- [ ] **C1**：A 修复后复跑，观察 RS 是否抵达新的断点。RS 侧源码
  `os/servers/rs/src/`（boot.rs/lib.rs/trap_api.rs），**RS 的 rip 可离线
  符号化**：`objdump -d os/target/image/x86_64/staging/EFI/minix/modules/rs`
  （vaddr 即执行视图）。
- [ ] **C2**：每个新断点按 §8 的取证循环处理（读代码 → 定性 → ≥1 方案
  对比 → 修 → 验证）。已知可能的下一层：step1 的 sys_privctl/getpriv
  返回值语义、step2 的 asynsend 后续、step3 catch_init_ready、step4。
- [ ] **C3**：RS boot 完成后，观察 RS 发出的 RS_INIT（给 VM）与 VM 的
  响应——VM 收到 RS_INIT 会继续 init 其余服务（DS/PM/…逐个 exec 后
  的 bootinh 清除链）。

### Task D — init execve /etc/rc 推进【C 完成后】

- [ ] **D1**：init（endpoint 11）被 RS 放行后执行 `/etc/rc`
  （`os/etc/rc`）。判据：串口出现 rc 脚本的执行痕迹（rc 脚本很短，
  第一行 marker）。
- [ ] **D2**：此阶段 PM/VFS/等服务器首次处理真实请求，可能暴露新的
  IPC/调度缺陷——同样按 §8 循环处理。

### Task E — rc marker 达成后【停止点】

- [ ] **E1**：串口出现 `minix-rs rc: minimal boot script marker` →
  **立即停止**，WORKLOG 写 DONE + 证据行，FIXLOG 补记。**不做**
  task1-close（探针去留裁决、run_all 接线、账本 ✅ 是评审方的工作，
  越权 = 返工）。

## §5 关键技术事实与陷阱（每条都有真机教训，违者重蹈）

1. **宿主测试一律 docker**：`docker run --rm -v "$PWD:/work" -w /work
   -m 2g minix-ci:1.94 cargo test -j 1 -p <pkg>`（在 `os/` 目录下执行，
   `$PWD` 是 os/）。内存小；`-j 1` 防崩。
2. **none/uefi 构建（镜像组装用）走宿主 ulimit**：见 §7 命令；
   IMG-EXIT 必须 = 0 才跑 QEMU（旧镜像会产出假进展——先确认退出码，
   必要时 `strings BOOTX64.EFI | grep <新符号>` 验证新镜像）。
3. **改 ld 必须重链**：改 `os/kernel-image/x86_64.ld` 后
   `touch os/kernel-image/src/main.rs`。
4. **QEMU 残留**：每次起跑前 `pkill -f '[q]emu-system'`（**注意方括号**：
   `pgrep -f qemu-system` 会匹配到包含该字符串的自身 shell，误报有进程）。
5. **探针纪律**：全部限次/带 cap；注释标注「task1-close 裁决删除」；
   必须 `#[cfg(not(feature = "mock"))]` 门（宿主测试下 EarlyConsole 是
   真端口写，会 SIGSEGV）。内核侧用 `crate::ipc::probe_mark("…")`
   （全局共享 8 次上限）；VM 侧用
   `crate::bootmark::mark(&alloc::format!("…"))`（自配 AtomicUsize 计数）。
6. **PIT 抢占时序敏感**：同一代码两轮串口可能不同（step2 槽缺失 c15a
   有 c16a 无）。修复验证必须**两次独立复跑**；不要为单轮现象过度设计。
7. **RS/命令模块 rip 可离线符号化**（模块 ELF vaddr=执行视图）；
   内核本体 PE 无符号，只能探针自报。
8. **SIGKMEM=71 超出 64 位 SigSet**：通知的 sigset 恒空，VM 靠
   sef_receive_status 的 source==SYSTEM(-2) 分类路由（D-20 契约：
   VM 经 MEMREQ_GET 探测，不读信号号）。
9. **vm_memreq_reply 状态机**：Pending →(memreq_get)→ Fetched →
   (reply)→ Completed+KCALL_RESUME；状态不符 InvalidState。
10. **kernel call 正 errno 经 `KcallResult::Ok(errno)` 上线为正数**
    （F10 未修）：检查返回值时注意 C 约定（负 errno）与此处的差异，
    涉及返回值语义的修改先 grep 既有对账（syscall.rs dispatch 层）。
11. **整仓 cargo fmt 禁止**（会污染 diff）；单文件核验用
    `rustup run nightly rustfmt --edition 2024 --check <file>`
    （宿主 stable 不认 let-chains）。
12. **git push 禁止**；`*.log` 被 .gitignore 忽略，归档证据用
    `git add -f <path>`（20260921-2320 与 20260922-nk4a-iter11-18 为先例）。
13. **tmp/nk4a/vars.fd 是 QEMU 持久变量**，每轮被改写，留在工作区
    不提交（历史会话以 ledger commit 顺带提交过，无需专门处理）。

## §6 记录格式（两份，缺一即返工）

### 6.1 WORKLOG（`rewrite-notes/coordination/NK4A-QWEN-WORKLOG.md`，每 Task 一节，追加）

```markdown
## Task A — <标题>（YYYY-MM-DD）
- 状态：DONE | BLOCKED | PARTIAL
- 定性结论：<一段话；数据说话>
- 改动文件：<文件:行区间，逐个列出>
- 命令与结果：<宿主测试计数；IMG-EXIT；复跑轮次名>
- 真机证据：<serial 文件名> 关键行：
  <原文摘录 ≤5 行>
- commit：<hash>（一逻辑单元一 commit）
- 未决问题：<如有>
- 自检：fix-guard 四步 ✅/❌；测试计数不减 ✅/❌；两次复跑 ✅/❌；
  FIXLOG 已补记 ✅/❌
```

### 6.2 FIXLOG（`.review/zcode/edge1/FIXLOG.md`，只追加；动手前自检
`grep -c "^# edge1 线修复日志" FIXLOG.md` 必须为 1）

每个修复一条，格式（**含测试小节**——这是后续「如何写测试」学习素材，
必须写清楚判别性断言为什么能抓住该缺陷）：

```markdown
### Fix #9 迭代<N> 补记——<标题>（YYYY-MM-DD，commit <hash>）

- **修前现状（fix-guard ±5 行）**：<目标行现状，引用行号>
- **根因**：<数据说话的定性，C 对位关系（文件:行）>
- **修法**：<改了什么，为什么最小、为什么对位 C>
- **测试（防回归）**：<新增/修改了什么测试；判别性断言是什么；
  为什么没有该断言时旧代码也能通过>
- **验证**：<宿主计数；真机轮次与关键串口行>
- **边界/登记**：<未修的相邻问题如实列出>
```

示例（真实条目，迭代11 的浓缩版）：
> 修前：`ipc::delivermsg` 成功臂只清 DELIVERMSG 不写返回码；VM 被
> SYSTEM 通知唤醒后 receive 返回陷入时旧 RAX（rcv-err 31744=ANY 实证）。
> C proc.c:290-292 在 delivermsg 成功路径写 `retreg=OK`（MF_CONTEXT_SET
> 门控）。修：成功臂补 `set_ipc_return_code(proc, OK)`，门控同步对位。
> 测试：test_deliver_message_success 先用 set_ipc_return_code 毒化 RAX
> 为 0x7c00，断言 delivery 后读回 == 0——**毒化+读回是判别性断言**，
> 不写返回码时旧代码同样通过该测试的前半部分。验证：kernel 808 全绿；
> 真机 rcv3 src=-2 type=0x1000。

## §7 常用命令（照抄可用；仓库根 = /home/xzhao/github/minix-rs）

```bash
# 0) 现场确认（每次动手前）
cd /home/xzhao/github/minix-rs
pgrep -f '[q]emu-system' && pkill -f '[q]emu-system'   # 方括号防自匹配
git log --oneline -3

# 1) 宿主测试（docker，在 os/ 目录）
cd os
docker run --rm -v "$PWD:/work" -w /work -m 2g minix-ci:1.94 \
  cargo test -j 1 -p minix-vm          # 换 -p 逐包跑：minix-kernel/minix-arch/minix-rs/minix-rt/minix-sys

# 2) 组装镜像（宿主 ulimit；必须 IMG-EXIT=0）
cd /home/xzhao/github/minix-rs/os
(ulimit -v 3145728; cargo run -q -p xtask -- image --arch x86_64 --release); echo IMG-EXIT=$?

# 3) 起真机（run 名=轮次号，如 c17a）
pkill -f '[q]emu-system'
rm -f /home/xzhao/github/minix-rs/tmp/nk4a/serial_<RUN>.log
bash /home/xzhao/github/minix-rs/tmp/nk4a/run-qemu.sh <RUN>
sleep 45
grep -v "vs0x" /home/xzhao/github/minix-rs/tmp/nk4a/serial_<RUN>.log | tail -60

# 4) RS rip 符号化
objdump -d /home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/rs | grep -B8 "<地址>:"

# 5) 归档证据（*.log 被 gitignore，需 -f）
git add -f tmp/evidence/<目录名>/ && git commit -m "..."
```

## §8 标准取证循环（每个新断点都走这一套）

1. 读串口尾部（排除 `vs0x` 采样行），记录最后事件与计数器状态；
2. rip 符号化（RS 用 objdump；内核 rip 无法离线符号化→上探针）；
3. 读对应源码 ±40 行，列全部出口；
4. 若静态可定性 → 直接修；否则加限次探针 → 复跑 → 读数；
5. 修复方案 ≥1 段 C 对位论证（grep `minix3/minix/` 原始 C）；
6. fix-guard：修前读目标 ±5 行、grep 确认、一次一修、修后 grep 验证；
7. 宿主测试全绿 → commit → FIXLOG 补记 → WORKLOG 补记；
8. 真机复跑两次验证。

## §9 禁止事项（违反即返工）

- 禁止 reset/rebase/force-push；禁止 push 远端；
- 禁止动 `minix3/`、`nk4a-agent-wip`、`nk4a-review-docs`、`os/etc/rc`、
  `os/qemu-tests/test-cmd-smoke.sh`；
- 禁止删除/改写既有探针与 FIXLOG 历史条目（只追加）；
- 禁止 task1-close（探针去留、run_all 接线、账本 ✅——评审方职责）；
- 禁止 stub/DEFERRED 假完成；测试不绿不标 DONE；
- 一个逻辑单元一个 commit；修完必须 FIXLOG+WORKLOG 双记录。
