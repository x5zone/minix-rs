# 交接 Prompt：NK4-A 生产启动链首亮 + NS12 尾款清理

> 用法：整段粘贴给任何 agent（无并发会话，**无需 claim.sh 上锁**，但所有纪律照旧）。
> 开工前按序读完：`CLAUDE.md` + `.claude/rules/review-core.md` + `review-process.md` + `fix-guard.md`
> + 本文件全文。Ground truth 链与构建纪律（docker minix-ci:1.94 第一优先；
> **x86_64-unknown-none / x86_64-unknown-uefi target 宿主 ulimit 构建**——docker 镜像无此二 target）
> 见项目 memory 与下文。

---

## 一、项目坐标（30 秒版）

minix-rs = Minix3 内核模块的 Rust 语义重写（Rewrite not Translate；x86_64/aarch64/riscv64，no_std）。
总目标：三架构 QEMU 启动 minix-rs 并跑通 18-stage-commands。账本：
`notes/rewrite/fork-syscall-rewrite/new_edge{1,2,3,4}.md`（edge1=内核/arch/boot、edge2=共享库/驱动、
edge3=服务器/FS/命令、edge4=编排/认领板）。FIXLOG：`.review/zcode/edge{1,2,3}/FIXLOG.md`（只写增量）。

## 二、上一棒已交付（全部在 rewrite 主线，无需重做）

1. **NS12 批一~三**：`os/commands` 12 包 65+3 bin（echo 模板推广 + sh/cat/ls 接线）全量双 seam
   （宿主 std / x86_64-unknown-none freestanding）。seam 模板见
   `os/commands/bin/fileops/src/bin/echo.rs` + 各包 `src/bin_support.rs`；sh 的 exec 帧构建在
   `os/commands/bin/shell/src/exec_frame.rs`（init execve.rs 判例提升，LP64 修正 + 4 布局测试）。
   三架构 spot 全过。FIXLOG edge3 #145。
2. **C-56**：minix-rt panic handler 的 no-hook 分支从 SpinSink 挂死改 SYS_DIAGCTL+exit
   （`os/libs/minix-rt/src/lib.rs`；hook registry 每地址空间私有，用户进程永查 None）。
3. **C-57 + 批四**：`os/qemu-tests/test-cmd-smoke.sh`（T4 验收契约：SKIP 门 → xtask image 装配 →
   ESP mtools 断言 → `entering scheduler` → T4 marker 窗口；`SMOKE_SKIP_BOOT=1` 为纯宿主校验模式）
   + 12 包 bin_support 的 emit/warn/write_ok 补 hosted std::io 双生（宿主工具 bin 走 minix_sys::write
   会被无核 trap 错杀——mkfs_mfs 实证）。FIXLOG #146/#147/#148。
4. **NK4-A 前置修复（本棒核心产出）**，全部已提交：
   - `8df3cfe33` **D-64② 快照顺序修正**：`os/boot-shim/src/uefi_helpers.rs` 的 `build_memmap()`
     快照必须拍在 root/bump 两次 LOADER_DATA 分配**之后**——先拍则守卫必炸（新分配在旧快照里仍是
     conventional）且内核 PMM 图把活页表页标 conventional（A2 灾难）。
   - `c9d9dc0e4`（上棒）xtask image 补 BOOTX64.EFI/startup.nsh 的 ESP mcopy。
   - `32c3c5c54` panic 诊断注册前移到 arch_boot 入口（原在 kmain Step -1，arch_boot 段 panic 全静默）。
   - 诊断路标：boot-shim `prepare_boot` 逐阶段 `uefi::println!`（EBS 前）+ `raw_serial`（EBS 后，
     COM1 0x3F8 直写，`os/boot-shim/src/lib.rs`）；内核 `arch_boot`/`boot_validate_and_prepare`/
     `kmain` Phase A-D 逐阶段 EarlyConsole 路标（`os/kernel/src/lib.rs`，`boot_stage!`/`vmark!` 宏，
     模块级定义）。

## 三、你的任务（按序）

### 任务 1（关键路径）：NK4-A 首亮——定位并修复 kmain Phase A 挂点

**现状**（2026-09-21，smoke19.log 时点）：`SMOKE_SKIP_BOOT=0 bash os/qemu-tests/test-cmd-smoke.sh`
的串口到达序列：

```
boot-shim: [raw] boot services exited
boot-shim: [raw] pt_alloc registered, entering kernel arch_boot
kernel: arch_boot entered
kernel: entering validate
kernel: v0 enter / v1 asserts ok / v2 fallback region ok / v3 pt_alloc ok
kernel: step0 validate ok / step1+2 mappings ok / step4 DM coverage ok
kernel: arch_boot_impl done, jumping to kmain
kernel: kmain Phase A enter
（此后静默 120s，QEMU 被脚本超时杀掉，exit 1）
```

即：**EBS 成功、arch_boot 全链成功（validate/映射/开分页/DM 全过）、kmain Phase A 体内挂死**
（"Phase A enter" 后、"A/A.2 memmap+modules ok" 标记前）。无 panic 输出（诊断注册已前移，
panic 会可见——所以是真 hang 不是 panic）。无三重错（-d int 已验证过非异常进入）。

**已修掉的三个坑（勿重查）**：①D-64② 快照顺序（上）；②xtask 装配漏 BOOTX64/startup.nsh 入 ESP；
③镜像跨距非 2MiB 对齐——`os/kernel-image/x86_64.ld` 已补
`.lbss/.ldata` 归属（rustc 拆分节成孤儿会撑破 ALIGN）+ `. = ALIGN(0x200000)` 收口
（**改 .ld 后必须 touch kernel-image 源码强制重链**——cargo 不追踪 ld 为依赖输入，已两次踩）。

**下一步打法**：
1. 在 kmain Phase A 体内逐语句插 `boot_stage!` 路标（宏已在 `os/kernel/src/lib.rs` 模块级），
   重建（`touch kernel-image/src/main.rs` 后 `cargo build --release -p kernel-image
   --target x86_64-unknown-none --features fw-x86-none`）→ 跑冒烟 → 读下一个标记。
2. Phase A 涉嫌面：FREE_MEMMAP 初始化（KernelInfo.memmap 遍历；注意 shim 侧 memmap 现在是
   **分配后快照**，conventional-only——若 A.2 的模块区域裁剪假设了 LOADER_DATA 区也在图里，
   语义需要重对）+ boot 模块物理区域切割循环。
3. 修一处跑一次；翻绿判据 = 串口出现 `/etc/rc` 的 `rc: minimal boot script marker`
   （T4_MARKER，OQ-3 已认可基线）→ 脚本 exit 0。
4. 收尾：把 `boot_stage!`/`vmark!`/raw_serial 诊断保留还是降级为 feature 门，做一次
   code-excellence 式裁决（建议保留但统一前缀与开关），并按 fix-guard 写 FIXLOG edge1
   下一条（#9 续）。

**每次 QEMU 前核验**：`pgrep -f qemu-system-x86_64` 为空（串口串行纪律）。构建宿主跑
（ulimit -v 3145728），docker 只用于 x86_64-none 之外的常规 crate。

### 任务 2（任务 1 翻绿后）：run_all 接线 + T4 翻绿

- `os/qemu-tests/run_all.sh` 增 test-cmd-smoke 块（house shape 照 test-sysboot 块；
  共享文件——照 new_edge4 §1 规则 3 先登记一句）。
- run_all 全量跑一次；NS12/new_edge3 行 + new_edge1 NK4-A 行翻 ✅（附 PASS 计数）。

### 任务 3（独立小件）：exec_frame 双份收敛

- `os/commands/bin/shell/src/exec_frame.rs`（NS12 提升版，带 4 布局测试）与
  `os/commands/sbin/init/src/execve.rs` 的 stack_params/stack_fill/PsStrings 双份并存。
- 目标归宿：`os/libs/minix-rt`（crt0 消费半的镜像孪生；edge2 领地，动手前在 new_edge4 §2
  登记）或按 code-excellence 对比后的更优落点（≥2 方案 + 对照 Redox/Linux 理由）。
- init 侧改消费共享实现；`cargo test -p minix-shell -p minix-init` 双绿。

### 任务 4（可选，前置已解锁）：NS5-B + NL3①

- NS5-B：vfs exec_worker 的 `stack_high` 生产源从 boot 契约缺省值改 kerninfo 读半
  （init execve.rs `new_image_stack_top` 判例已在库；NK1 已收口解锁）。
- NL3①：minix-rt 页供应商走 VM_BRK taskcall（OQ-N4 已裁决，C 对位 libc brk 直发 VM_PROC_NR）。

## 四、硬纪律（违反即返工）

- Rewrite not translate；错误码映射 Minix3 errno；`#![no_std]` 除 cfg(test)；
  内核=SMP+BKL、用户态服务器=单线程事件循环。
- 一切事实断言带 file:line 锚点，动手前重读目标行（fix-guard）。
- git commit 一逻辑单元一提交，message 沿仓风格（中文摘要 + 锚点 + 验证计数）；
  账本（new_edgeX.md）改完即 commit；FIXLOG 只写增量、追加前 grep 守卫。
- 整仓 `cargo fmt` 禁止；新代码 `rustup run nightly rustfmt --edition 2024 --check` 单文件核验。
- 不假完成：stub/DEFERRED 诚实登记；测试不绿不标 ✅；诊断打印的去留做显式裁决并记录。
- 完成定义：所改 crate 测试全绿 + clippy 对账零新增 + 文档同步（NS12/NK4-A 行、FIXLOG）。
