# D03 R2 certify2（闭集复核）

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 版本：sha256 `f0a792521acaeb80ea3d5a8b6d862c389a594e0d3be333a89ee801470011c5db`（与任务给的 `f0a79252` 前缀一致；当前工作树 1393 行。任务单写 1384 行，与工作树不符；下引行号按当前 1393 行版本）。
- 范围：任务单四句及其同段邻句。未重扫全文，未读历史评审、其他面板、`expected/`。只读核对；除本文件外未写任何文件。
- 结论：第 2、4 项 CLOSED；第 1 项 REOPEN（同一 P0 的 L184 残留未改）；第 3 项的两项字面要求 CLOSED，但“具体字段名见 §4.2”的目标节没有字段名，另立新 P0-fact。新准入 1 条。

## 一、机械门

### 1. anchor-resolve --check

```text
$ tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
（逐条打印 75 条解析结果，此处略去中间行）
----
anchors=75 resolved=75 zero-def=0 multi-def=0
exit=0
```

### 2. doc-ref-check

```text
$ tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/03-kmain-cstart.md 内部章节引用均可解析
exit=0
```

读数说明：这道门只验证“§4.2 这个目标存在”，不验证目标节里是否有被承诺的内容。下面的 FINDING -01 正是在门通过的情况下仍然成立的语义断链。

## 二、逐条核对

### 1. §2.1 次序承重 —— REOPEN

被核对句（L186，本轮新增）：

> **次序承重**：步骤 2 必须早于 `cstart`（`prot_init` 读 `kinfo`）；步骤 3 依赖步骤 2（`env_get` 读 `kinfo.param_buf`）；步骤 5 在启动循环读 `image[]` 之前完成即可，`proc_init` 不读 `boot_procs`。

三句都对：

- 步骤 2 早于 `cstart`：`cstart`（`main.c:403`）的第一个实质动作是 `prot_init()`（`main.c:411`），`prot_init` 把 `&kinfo` 交给 `pg_identity`/`pg_map`（`arch/i386/protect.c:361/381`）；全局 `kinfo` 由步骤 2 的 `memcpy(&kinfo, local_cbi, sizeof(kinfo))`（`main.c:128`）填入。
- 步骤 3 依赖步骤 2：`env_get` 直接读全局 `kinfo.param_buf`（`main.c:505-508`），`param_buf` 正是步骤 2 拷进来的。
- 步骤 5 与 `proc_init`：`proc_init`（`proc.c:119`）只清进程表/特权表并调 `arch_proc_reset`，函数体内没有 `image`/`boot_procs` 的读：

```text
$ awk 'NR>=119 && NR<=148' minix3/minix/kernel/proc.c | grep -n "image\|boot_procs"
（无输出，exit=1）
$ sed -n '157,171p' minix3/minix/kernel/main.c
  proc_init();
  ...
  for (i=0; i < NR_BOOT_PROCS; ++i) {
	ip = &image[i];				/* process' attributes */
```

`proc_init` 在 `main.c:157`，读 `image[]` 的是其后 `main.c:171` 的启动循环；第二次 `memcpy(kinfo.boot_procs, image, ...)` 在 `main.c:275`，之后由用户态 VM 消费。新句的三条结论成立。

**REOPEN 的原因**：同一个 P0（RD3-R2-CERT-01）的位置包含 L184 与 L186；§2.1 的本轮 diff 只新增了 L186 的小结，L184 的旧断言原样保留：

> 第一次拷贝在循环前给 `kinfo.boot_procs` 留一份静态快照；循环与第二次拷贝都不读它，`proc_init` 读的是 `image[]`，所以真正被消费的是第二次拷贝。

“`proc_init` 读的是 `image[]`”不成立：`proc_init` 在 `main.c:157` 执行，`image[]` 的读者是它之后的 `main.c:171` 启动循环；上面的 grep 也证明 `proc_init` 体内没有 `image`。这正是上一轮 FINDING -01 的“问题三”，当时已写明要改成“`cstart` 之后的启动循环读的是 `image[]`”。新增的小结与 L184 同段并置，读者会得到两个读者归属：小结说“启动循环读 `image[]`”，前一段说“`proc_init` 读的是 `image[]`”。P0 未修完。

建议：把 L184 的“`proc_init` 读的是 `image[]`”改为“`cstart` 之后的启动循环读的是 `image[]`”，与 L186 对齐。

判定：REOPEN（沿用 RD3-R2-CERT-01，不计新准入）。

### 2. §4.6 sscratch —— CLOSED

被核对句（L1177）：

> `sscratch` 是用户陷阱的交换寄存器（进入内核时第一条指令 `csrrw sp, sscratch, sp` 把用户 sp 存进去、内核栈换出来），用户陷阱进入内核后它保存的是用户 sp；`TrapReturnArch::prepare_user_return` 在每次用户返回前把调度方传入的 `stack_top` 重新写进本 hart 的 `sscratch`，下一次进入时再交换。

两边都对：

```text
$ grep -n "csrrw sp, sscratch, sp" os/arch/src/riscv64/trap_stub.rs
536:    "    csrrw sp, sscratch, sp",  // sp ← kstack top; sscratch ← user sp
659:    "    csrrw sp, sscratch, sp", // sp ← user sp (parked in sscratch); sscratch ← kstack top
$ sed -n '118,126p' os/arch/src/riscv64/trap_return.rs
    fn prepare_user_return(_cpu_id: u32, stack_top: minix_types::VirBytes) {
        ...
        if stack_top.0 != 0 {
            crate::riscv64::protection::set_entry_stack_base(stack_top);
        }
    }
$ sed -n '70,77p' os/arch/src/riscv64/protection.rs
pub(crate) fn set_entry_stack_base(base: VirBytes) {
    ...
    asm!("csrw sscratch, {}", in(reg) base.get(), options(nomem, preserves_flags));
}
```

进入（`trap_stub.rs:536`）后 `sscratch` = 用户 sp；返回前 `prepare_user_return` → `set_entry_stack_base` 把内核栈顶写回 `sscratch`；返回侧汇编（`trap_stub.rs:659`）再交换一次。原文顺序与机制一致，上一轮的“返回后保存用户 sp”已消除。

判定：CLOSED。

### 3. §4.1 七个 slice —— 两项字面要求 CLOSED；注记目标不成立（新 P0-fact）

被核对句（L966）：

> ……把启动信息的七个 slice（保留区间、memmap、模块表+名字池、平台源、参数缓冲、`memmap_sources`、`startup_objects`；具体字段名见 §4.2）……

两项字面要求都满足：七个条目恢复为中文名（其中 `memmap`、`memmap_sources`、`startup_objects` 保留代码名），并注明了“具体字段名见 §4.2”。七项与实际字段一一对应（`os/libs/minix-boot/src/kernel_info.rs`）：

| 文档条目 | 实际字段 | 行 |
|---|---|---|
| 保留区间 | `reserved_regions` | 61 |
| memmap | `memmap` | 32 |
| 模块表+名字池 | `boot_modules`（名字落进固定字节池） | 121 |
| 平台源 | `platform_sources` | 145 |
| 参数缓冲 | `param_buf` | 173 |
| `memmap_sources` | `memmap_sources` | 74 |
| `startup_objects` | `startup_objects` | 84 |

但注记指向的 §4.2 是“init_protection 的逐步落地（→3.2、3.3、3.7）”（L980-L1004），里面没有 `KernelInfo`，也没有七个字段名中的任何一个，连“字段名/slice”字样都没有：

```text
$ sed -n '/^### 4.2 /,/^### 4.3 /p' rewrite-notes/01-stage-kernel/03-kmain-cstart.md | grep -c "KernelInfo\|reserved_regions\|param_buf\|boot_modules\|platform_sources\|memmap_sources\|startup_objects\|slice\|字段名"
0
```

所以“具体字段名见 §4.2”是一句空指引：读者按它翻到 §4.2 找不到任何字段名。全文也没有第二处补上这七个代码名（§4.1 L974 与 §4.7 L1196 只指向 `store_kernel_info` 链接，正文仍用中文条目）。见 FINDING -01。

判定：字面两项 CLOSED；同一句的注记目标不成立，另立新 P0-fact。

### 4. §5.1 13 条 trap 测试 —— CLOSED

被核对句（L1234）：

> 13 个 trap 测试 = 9 条 `init` 门表元数据 + 2 条 `set_handler` + 2 条结构布局；……

与 `os/arch/src/x86_64/trap_entry.rs` 的实际测试逐条对账：

```text
$ grep -c '^\s*#\[test\]' os/arch/src/x86_64/trap_entry.rs
13
$ grep -A1 '^\s*#\[test\]' os/arch/src/x86_64/trap_entry.rs | grep -c 'fn idt_init_'
9
$ grep -A1 '^\s*#\[test\]' os/arch/src/x86_64/trap_entry.rs | grep -c 'fn set_handler_'
2
$ grep -A1 '^\s*#\[test\]' os/arch/src/x86_64/trap_entry.rs | grep -c 'fn idt_entry64_size_is_16_bytes\|fn idt_ptr_size_is_10_bytes'
2
```

9 条 `idt_init_*`：`idt_init_sets_exception_gates`、`idt_init_reserved_vectors_not_present`、`idt_init_sets_syscall_ipc_vectors`、`idt_init_sets_pic_vectors`、`idt_init_breakpoint_has_dpl3`、`idt_init_overflow_has_dpl3`、`idt_init_double_fault_uses_ist2`、`idt_init_nmi_uses_ist1`、`idt_init_kernel_exceptions_have_dpl0`；2 条 `set_handler_sets_dpl_correctly`/`set_handler_writes_handler_address`；2 条 `idt_entry64_size_is_16_bytes`/`idt_ptr_size_is_10_bytes`。分类与计数完全对应。

判定：CLOSED。

## FINDING RD3-R2-CERT2-01（P0-fact）

位置：§4.1，L966。

原文：“……（保留区间、memmap、模块表+名字池、平台源、参数缓冲、`memmap_sources`、`startup_objects`；具体字段名见 §4.2）……”

事实：§4.2 是 `init_protection` 的落地小节（L980-L1004），内容为 `CurrentProtection::init`、`store_protection`、`refresh_descriptors`、`prot.load`、`install_trap_stubs`、`configure_syscall` 与 `trap.load`；它没有 `KernelInfo` 结构，没有七个字段名中的任何一个。命令与输出见上（`grep -c` 得 0）。七个字段名在本篇里没有任何一处完整列出；能查到它们的地方是 `os/libs/minix-boot/src/kernel_info.rs:32/61/74/84/121/145/173` 与 `os/kernel/src/lib.rs` 的 `store_kernel_info`（`lib.rs:1453` 起，逐块 `land_slice`）。

影响：上一轮 FINDING -03 的目的是让读者不必二次对表；现文本把读者从“要对表”改成“按指引翻到 §4.2 后仍找不到”。按 `prompt/review-rules/review.md` 的 P0-fact 口径——“文档写‘Ch3 §3.2 设计 X’，但 §3.2 实际无相关内容”——这是引用与事实不符，判 P0-fact。`doc-ref-check` 通过只说明 §4.2 存在，不说明它含有所承诺的内容；本条的成立不受该门影响。

建议修法（只读核对，未改动）：把“具体字段名见 §4.2”改为本节可直接自查的写法，例如“（`reserved_regions`、`memmap`、`boot_modules`、`platform_sources`、`param_buf`、`memmap_sources`、`startup_objects`，落地点见本节 `store_kernel_info` 段）”；或指向 `KernelInfo` 定义（`os/libs/minix-boot/src/kernel_info.rs`），不要指向 §4.2。

## 准入汇总

| 项 | 核对对象 | 判定 | 说明 |
|---|---|---|---|
| 1 | §2.1 次序承重 | REOPEN | 新小结三句都对；同段 L184“`proc_init` 读的是 `image[]`”未改，同一 P0 未闭合 |
| 2 | §4.6 sscratch | CLOSED | 进入存用户 sp、返回前重写，与 `trap_stub.rs:536`/`trap_return.rs:118-126` 一致 |
| 3 | §4.1 七个 slice | CLOSED（字面）/ 新 P0 | 中文名与注记都在；注记目标 §4.2 无字段名，见 FINDING -01 |
| 4 | §5.1 13 条 trap 测试 | CLOSED | 9 init + 2 set_handler + 2 布局，与 `trap_entry.rs` 逐条对应 |

- 新准入：P0-fact × 1（RD3-R2-CERT2-01，§4.1 的 §4.2 空指引）。
- 非新准入的阻塞项：RD3-R2-CERT-01 REOPEN（§2.1 L184 的 `proc_init`/`image[]` 归属），上一轮已入账，不重复计数。

最终认证新准入 = 1
