# NK4-A QWEN WORKLOG — 逐任务工作记录

> 任务书：`NK4A-TODO.md`（同目录）。记录格式见 TODO §6.1。
> 每个 Task 一节，追加，不改写历史条目。

---

## 会话开场（2026-09-22，qwen 接手）

- 起始 commit：`eebe41550`（docs 提交，其父 `4a6570d7f` = 迭代18 修复尖端，
  `e4c6e8224` = 迭代11-18 证据归档）。分支 `rewrite`，未 push。
- 现场核对：无 QEMU 残留；`git status` 仅 `AI-chats/daily.todo.md` 与
  `tmp/nk4a/vars.fd` 既有改动（按任务书不动）。
- 已读输入：`NK4A-TODO.md` 全文；`.review/zcode/edge1/FIXLOG.md` 末 300 行
  （迭代11-18 六修复链 + 前沿定性）。
- 本轮目标：Task A（VM 页故障服务停滞定位与修复）起步，停在
  `minix-rs rc: minimal boot script marker`（Task E）。
- 计划：A1 读 vm_server.rs VM_PAGEFAULT 臂列出口 → A2 读 cow_exec_pf.rs +
  memtype.rs → A3 查帧池 → A4 加限次探针 → A5 复跑定性 → A6 修 →
  A7 宿主全绿+真机两次复跑 → A8 双记录。

## Task A — 定位并修复 VM 页故障服务停滞（2026-09-22）

### A1-A3 静态定性（读码，未上真机）

- **A1 出口表**（`vm_server.rs` dispatch_pagefault，探针前 1785-1920 行）——
  跳过 `vm-pf bytes` 打印的全部出口：
  | 出口 | 行为 | 是否清 PAGEFAULT | 是否可见 |
  |------|------|------|------|
  | L1793 badendpt（vm_isokendpt 失败） | Error(InvalidProcess) | 否 | 静默 |
  | L1797 inactive（get_active 失败） | Error(InvalidProcess) | 否 | 静默 |
  | L1835 wro（写只读 region） | SIGSEGV+clear，Error(AccessViolation) | **是** | 可见 |
  | L1858 noaddr（regions.find_mut 失败） | Error(InvalidAddress) | **否** | 静默 |
  | Ok(Suspended) | VmReply::Ok，inc_major | 否（设计如此，等 VFS） | 静默 |
  | Ok(AccessViolation) | VmReply::Ok | 否 | 静默 |
  | Err(e) | `vm-pf err` 打印后 Error | 否 | 半可见（已有探针） |
- **A2 回答**：ANON 的 `ev_pagefault`（memtype.rs:294-324）只返回
  NeedNewPage/Handled/NeedCow/AccessViolation，**永不 NeedVfsIo**；
  NeedVfsIo 唯一产地是 MappedFile（memtype.rs:1084）。exec_bootproc 全部
  region 均建为 MEM_TYPE_ANON（vm_server.rs:788-793 段、916-920 栈）→
  **boot 期 Suspended 假设被静态否定**（TODO §3 的头号嫌疑排除）。
- **A3 帧池**：生产形态无固定小池——`create_default_allocator`
  （vm_server.rs:308）以 kernel handoff 的全部 free 物理区建
  BitmapAllocator（QEMU `-m 512M`，池 = 512MiB 扣 kernel/模块/UEFI 后
  的 free 页，~10 万页量级；fix19 时代的"24 页池"是 mock 参数，非生产）。
  消耗 = 12 模块 eager 物化 + 运行期按需填充 + minix-rt 堆 256 页/服务。
  OOM 的全部路径（alloc_and_map / cow_resolve）都收敛到
  CowError::NoMemory → `vm-pf err` 打印 → **枯竭假设待真机读数排除**。
- 探针 commit：5f98b1db5（pf-exit 七出口，cap 8/出口，宿主 VM 525 全绿，
  警告基线 17 不变）。

### A5 真机定性（c17a 轮，2026-09-22）

- 串口计数：`vm-pf recv` 163 / `vm-pf bytes` 162 / `vm-pf err` 0 /
  `pf-exit noaddr` 1。差值恰好一条：最后一条 recv 跟随
  **`nk4a: pf-exit noaddr cr2=0x0`**，随后 `picknone rs_flags=0x400`
  （RS 永停 RTS_PAGEFAULT），系统静默死锁——停滞出口 = **noaddr**
  （fault_addr=0x0 不属于任何 region），不是 Suspended，不是帧池枯竭
  （`vm-pf err` 零命中 + 池 ~10 万页）。
- RS 侧：停滞前 pre-restore rip=0x2014ad（drop_in_place RProcTable 内
  `test %rax,%rax`，恢复执行后某指令 deref 了 NULL → cr2=0）。这属
  RS 自身逻辑问题（Task C/D 前沿）；**Task A 的缺陷是 VM 对不可服务
  fault 的收口违反 C**：C pagefaults.c:89-105 对 unknown region 必然
  `sys_kill(SIGSEGV)` + `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` 后才返回，
  Rust noaddr 出口两条全漏 → 挂起位无人清 → 死锁。
- 修法（A6）：按 C handle_pagefault 出口表补齐——noaddr /
  Ok(AccessViolation) / Err 三个"不可服务"终局统一走
  SIGSEGV+clear_pagefault（提取共用 helper，wro 臂同体重构）；
  Suspended 臂保留挂起（C 的 SUSPEND 语义如此）。
### A6-A8 修复与验证（2026-09-22）

- 状态：**DONE**（Task A 判据达成：RS 不再停在 PAGEFAULT——noaddr 收口
  后系统从静默死锁推进到 C 语义的致命信号处置；成功服务的 recv/bytes
  162 对全配对；kc 流水维持 45+ 无回退）。
- 改动文件：`os/servers/vm/src/vm_server.rs`
  （helper `pf_fail_segv` L1786-1806 新增；noaddr/accvio/Err 三终局
  接入；wro 臂体重构为调 helper；新增测试
  `test_pagefault_unknown_region_sigsegv_and_clears_park`）。
- 命令与结果：宿主 docker -j1 minix-vm **526**（基线 525+1 新增）/
  minix-kernel **808** 全绿；IMG-EXIT=0；复跑轮次 c18a、c18b。
- 真机证据：serial_c18a.log / serial_c18b.log（两轮一致）关键行：
  ```
  nk4a: vm-pf recv
  nk4a: pf-exit noaddr cr2=0x0
  <unset> 0x0000000000000002 0x000000000020d0d5
  boot-shim panic: panicked at kernel/src/syscall_signal.rs:300:13:
  cause_sig: sig manager 2 gets lethal signal 11 for itself
  ```
- 新断点定性（属 Task C，非本修复遗留）：RS 用户态 deref NULL →
  SIGSEGV → RS 是 sig manager，致命信号自受 = C system.c:430 同款
  内核 panic。停滞形态从"静默死锁"变为"可见 panic"——排查能力恢复。
  下一轮从 0x20d0d5 符号化 + RProcTable drop 路径 + 数据页填充内容
  三方向取证。
- commit：5f98b1db5（A4 探针）+ febbb0c8b（A6 修复+测试）。
- 未决问题：RS 为何在 drop_in_place RProcTable（0x2014ad 附近）deref
  NULL——是 RS 逻辑 bug 还是 exec 填充数据错位（vm-pf bytes 探针可续查），
  Task C 处理。
- 自检：fix-guard 四步 ✅；测试计数不减 ✅（526/808）；两次复跑 ✅
  （c18a/c18b）；FIXLOG 已补记 ✅（迭代19 条目）。

## Task B — step2 槽缺失复现定性（条件触发）

- 状态：**未触发**——A 修复后两轮真机（c18a/c18b）未出现 boot.rs:1054
  panic；新断点为 RS null-deref → cause_sig panic，走 Task C 取证循环。


