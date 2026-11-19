# D03 R2 certify（闭集）

- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 版本：sha256 `e22a4889…`（工作树，1393 行；任务单给的 sha 前缀一致）
- 范围：任务单六项改动句及邻句。未重扫全文，未读历史评审、其他面板、expected。
- 结论：第 1–5 项的核心事实 CLOSED（第 4、5 项各附 1 条 P2 表述项）；第 6 项中的 §2.1「次序承重」小结 REOPEN（P0）。新增 P0×1、P2×3。

## 一、机械门

### 1. doc-citation-check

```text
$ python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md --root .
rewrite-notes/01-stage-kernel/03-kmain-cstart.md: blocks=42 checked=0 failed=0 skipped=42 warned=0
exit=0
```

**读数说明（不能当引文证据）**：`checked=0`。本篇的引文标注全部写成块注释
`/* 代码引用，… */`，没有工具要求的行注释 `//` 形式，所以 42 个围栏块全部 `SKIP`。
这个 exit 0 只说明“没有可检查的块”，不说明任何一条引文与源码一致。下面各项的引文由
人工逐条对照源码；本篇引文门的机械覆盖为零，登记为工具口径缺口，不混入本文的
P0/P1/P2 计数（若要求引文门对本篇有覆盖，需把标注改成 `//` 或扩展工具，不在本次
闭集改动范围）。

### 2. anchor-resolve --check

```text
$ tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
anchors=75 resolved=75 zero-def=0 multi-def=0
exit=0
```

### 3. doc-structure-check

```text
$ python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md
✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析
exit=0
```

### 4. doc-ref-check

```text
$ AWK=gawk tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/03-kmain-cstart.md 内部章节引用均可解析
exit=0
```

## 二、逐条核对

### 1. §4.1 `bootstrap_start/len` 已改为随 D01-F03 删除 —— CLOSED

核对对象（§4.1 末句）：“`bootstrap_start/len` 两字段已随 D01-F03 从 `KernelInfo` 删除
（`lib.rs:1858-1863`；01 篇（`01-boot-shim-bootstrap.md`）§3.5/§3.9），不再存在 `(0,0)` 占位。”

证据：

```text
$ grep -n "bootstrap" os/libs/minix-boot/src/kernel_info.rs
193:    /// QEMU bootstrap test kernels: no boot modules, no source records.
244:    /// `boot_modules` non-empty check: QEMU bootstrap test kernels legally
422:        //    bootstrap test kernels hand over no init-server image.
```

`KernelInfo` 的字段清单里没有 `bootstrap_start`/`bootstrap_len`；上面三处都是测试用
bootstrap 内核的说明文字，不是字段。被引的 `lib.rs:1858-1863` 原文：

```text
    // Phase F: bsp_finish_booting.
    // C reclaims the unpaged-kernel section here (`add_memmap(&kinfo,
    // bootstrap_start, bootstrap_len)`); the Rust port runs higher-half from
    // the first instruction and the two fields were removed from
    // `KernelInfo` (D01-F03), so there is nothing to reclaim and no field to
    // validate.
```

01 篇两个被引小节都存在：`§3.5 bootstrap_start/len：从“可回收区间”到“必须为 0”，再到字段删除`
与 `§3.9 free_upper_idx：从字段到报告位（不再跨 handoff 携带；字段删除）`。`KernelInfo`
定义与 `os/` 生产源码里，`bootstrap_start`/`bootstrap_len` 只剩注释与历史叙述（`os/target/`
下 `fmtchk` 的旧镜像副本不算生产源码），没有字段定义，`(0,0)` 占位不存在。

判定：CLOSED。

### 2. §3.4 `GsArea.user_rsp` 由 SYSCALL 入口写入 —— CLOSED

核对对象（§3.4）：“两个栈字段由 `program_gs` 在 BSP/AP 启动时写入；`user_rsp` 由 SYSCALL
入口写（`trap_stub.rs:376` 的 `mov gs:{gs_user_rsp}, rsp`），入口对该字段是读写而非只读。”

证据：

```text
$ sed -n '640,650p' os/arch/src/x86_64/trap_stub.rs
pub struct GsArea {
    /// Kernel stack top the SYSCALL entry switches to (GS_SLOT_KERNEL_STACK).
    pub kernel_stack_top: u64,
    /// Scratch slot where the entry parks the user RSP (GS_SLOT_USER_RSP).
    pub user_rsp: u64,
    /// This CPU's logical id (AP self-identity readback).
    pub cpu_id: u64,
}
```

```text
$ grep -n "gs_user_rsp" os/arch/src/x86_64/trap_stub.rs
376:    "  mov qword ptr gs:{gs_user_rsp}, rsp",  // park user RSP
382:    "  push qword ptr gs:{gs_user_rsp}",      // frame.rsp (user)
645:    /// Scratch slot where the entry parks the user RSP (GS_SLOT_USER_RSP).
646:    pub user_rsp: u64,
```

（上面只摘与 `user_rsp` 读写直接相关的四行；完整命中还含常量定义与布局测试。）

入口先写后读，`reader`/`writer` 两向都成立。`program_gs`（`trap_stub.rs:684-694`）写
`kernel_stack_top` 并把 `user_rsp` 初始化为 0；运行期的用户 sp 由入口那条 `mov` 写入。
原文“两个栈字段由 `program_gs` … 写入”在“初始化写入”的意义上成立，后一分句把运行期
写入者点明，不构成事实冲突。

判定：CLOSED。

### 3. §3.7/§4.7 Rust IDT 41 门、IOAPIC 只路由 16 条 ISA IRQ —— CLOSED

证据一，41 门：

```text
$ sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'
41
$ grep -n "const IDT_ENTRIES" os/arch/src/x86_64/trap_entry.rs
114:const IDT_ENTRIES: usize = 256;
```

41 的构成与原文一致：18 条异常（0–8、10–14、16–19，跳过 9/15）+ 4 条软中断（32–35）
+ 16 条 PIC 门（0x50–0x57、0x70–0x77）+ 3 条单独向量（0xF0、0xF1、0xFF）。

证据二，IOAPIC 只路由 16 条 ISA IRQ：

```text
$ sed -n '346,358p' os/plat/src/x86_64/interrupt.rs   # 节选
/// C programs the same mapping when the APIC is initialized
/// (apic.c RTE setup, VECTOR(irq) = 0x50+irq / 0x70+irq-8). Pins ≥ 16
/// (QEMU's extra IOAPIC inputs) stay masked with the reset vector 0 —
/// no handler exists for them and nothing unmasks them.
...
let irq = match pin {
    0 => Some(2u8), // ISA cascade enters the IOAPIC at pin 0
    2 => Some(0u8), // ISA IRQ0 (PIT) enters at pin 2
    i @ 1..=15 => Some(i),
    _ => None,      // PCI-level pins: no kernel IRQ number, masked
};
```

16 条 ISA 线有向量映射；pin ≥ 16 落 `None`、向量 0 且保持屏蔽；`mask_all` 也把循环
夹在 `min(16)`（`interrupt.rs:487-494`）。Rust 侧没有 C 的 `gate_table_ioapic` 64 门表。

证据三，0xF0/0xF1 归属：

```text
$ grep -n "APIC_TIMER_INT_VECTOR\|APIC_SMP_SCHED_PROC_VECTOR" minix3/minix/kernel/arch/i386/apic.h
90:#define APIC_TIMER_INT_VECTOR		0xf0
91:#define APIC_SMP_SCHED_PROC_VECTOR	0xf1
```

Rust 侧 `STUB_VECTORS` 的注释是 0xF0 调度 IPI、0xF1 LAPIC 本地定时器，与原文“C 的
0xF0=timer/0xF1=sched，Rust 相反”一致。§4.7 的“APIC 路径另加 IOAPIC 门”位于 C 侧
对照的括注里，§3.7 已明写 Rust “没有 64 门表”，两节不冲突。

判定：CLOSED。

### 4. §3.6 Phase F 的 `boot_init_timer` 三步；§4.6 `sscratch` —— CLOSED（机制），附 P2 措辞

证据一，Phase F 三步：

```text
$ sed -n '3720,3795p' os/kernel/src/lib.rs | grep -nE "init_timer|register_hook|enable_timer_irq"
36:    clock_arch.init_timer(
50:        .register_hook(
60:    <CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq();
69:    // PIT + register_hook + enable_timer_irq): PIC remapping only changes the
```

第 36 行是 (a) 编程时钟源，第 50 行是 (b) 注册 IRQ hook，第 60 行是 (c) 打开 timer gate；
`lib.rs:1669` 的 Phase F 注释原文就是
`/// Phase F: bsp_finish_booting (timer program+register+gate) + switch_to_user`。

证据二，`sscratch` 的两个时刻：

```text
$ grep -n "csrrw sp, sscratch, sp" os/arch/src/riscv64/trap_stub.rs
536:    "    csrrw sp, sscratch, sp",  // sp ← kstack top; sscratch ← user sp
```

```text
$ sed -n '118,126p' os/arch/src/riscv64/trap_return.rs
    fn prepare_user_return(_cpu_id: u32, stack_top: minix_types::VirBytes) {
        // The per-hart CSR is the only store: an AP's `init_ap` writes its
        // own `sscratch`, never a shared cell. The scheduler calls this on
        // every dispatch because the park/resched path leaves `sscratch`
        // holding the parked user sp.
        if stack_top.0 != 0 {
            crate::riscv64::protection::set_entry_stack_base(stack_top);
        }
    }
```

进入内核时第一条指令把用户 sp 存入 `sscratch`、把内核栈顶换进 `sp`；每次用户返回前
`prepare_user_return` 把 `stack_top` 重写进本 hart 的 `sscratch`。机制层与原文一致。
但“所以用户陷阱返回后它保存的是用户 sp”这一句把时刻说反了，见 FINDING -02（P2）。

判定：§3.6 CLOSED；§4.6 机制 CLOSED，措辞 P2。

### 5. §5.1 的 3/6 计数与 373/87/55 口径 —— CLOSED（计数），附 P2 描述

逐文件计数（`grep -c '#[test]'`）：

```text
$ for f in os/arch/src/x86_64/{protection,trap_entry,trap_stub}.rs \
           os/arch/src/arm64/{protection,trap_entry,trap_stub}.rs \
           os/arch/src/riscv64/{protection,trap_entry,trap_stub}.rs; do
    printf "%s: " "$f"; grep -c '#\[test\]' "$f"; done
os/arch/src/x86_64/protection.rs: 16
os/arch/src/x86_64/trap_entry.rs: 13
os/arch/src/x86_64/trap_stub.rs: 11
os/arch/src/arm64/protection.rs: 2
os/arch/src/arm64/trap_entry.rs: 1
os/arch/src/arm64/trap_stub.rs: 3
os/arch/src/riscv64/protection.rs: 2
os/arch/src/riscv64/trap_entry.rs: 1
os/arch/src/riscv64/trap_stub.rs: 7
```

`riscv64/trap_stub.rs` 的 7 里有一条是注释命中（`trap_stub.rs:906` 的
`// ... a #[test] written here ...`），按行首锚定 `#[test]` 计数是 6。九个文件按锚定
计数合计 16+13+11+2+1+3+2+1+6 = 55。全量口径：

```text
$ grep -rh '^\s*#\[test\]' os/arch/src --include='*.rs' | wc -l
373
$ grep -rl '^\s*#\[test\]' os/arch/src --include='*.rs' | wc -l
54
$ grep -rh '^\s*#\[test\]' os/arch/tests/*.rs | wc -l
87
$ ls os/arch/tests/*.rs | wc -l
21
```

3/6、55、373/54、87/21 全部与原文一致。原文对 13 条 trap 测试的构成描述过宽，见
FINDING -04（P2）。

判定：CLOSED（计数口径）；描述 P2。

### 6. 其余六项

#### 6.1 §2.4 TSS `cs` 行 —— CLOSED

C 源 `i386/protect.c:169` 是 `t->cs = KERN_CS_SELECTOR;`；TSS 的 `cs` 字段只在硬件任务
切换时由 CPU 装载，Minix3 不用硬件任务切换，门转场用门里的 `selector`。C 树里没有读
TSS 的 `cs` 字段的代码：`t->cs` 只有 protect.c:169 这一处写入；grep `->cs` 的其余命中
都是栈帧或进程寄存器（`frame->cs`、`pr->p_reg.cs`），不是 TSS。行文与值都正确。

判定：CLOSED。

#### 6.2 §2.1 次序承重小结 —— REOPEN（P0）

见 FINDING -01。小结里的“步骤 5 必须早于 `proc_init`（boot image 表的读者）”与“步骤 3
与前后无依赖”都与 C 源不符；同段前一句“`proc_init` 读的是 `image[]`”同样是错的。

判定：REOPEN。

#### 6.3 §5.2 五处寄存器读数 —— CLOSED

`os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs` 依次读
`gdtr_base`（`sgdt`）、`idtr_base`（`sidt`）、`tr`（`str`）、`cs`、`ds` 五处；四个
SYSCALL MSR（STAR/LSTAR/SFMASK/EFER）单列。与原文“五处寄存器读数（GDTR/IDTR 的基址、
TR 的选择子、CS/DS 的选择子）”完全对应。

判定：CLOSED。

#### 6.4 §4.1 slice 字段名 —— CLOSED（字段对应正确），附 P2 标注

`KernelInfo` 的七个切片字段是 `memmap`、`reserved_regions`、`memmap_sources`、
`startup_objects`、`boot_modules`、`platform_sources`、`param_buf`；原文的七个条目与
之一一对应。但“代码字段名：”只带出了其中三个真正的字段名，另外四个是中文描述，见
FINDING -03（P2）。

判定：CLOSED（内容）；标注 P2。

#### 6.5 §1.1 引导方/装载方定义 —— CLOSED

§1.1 首句定义：“本文的‘引导方’指进入内核前提供表的固件/GRUB/boot-shim；‘装载方’在
§2.1 谈 BSS 清零时指同一方。”§2.1 的 BSS 清零段（L139）用“装载方”指执行清零的
装载器，与定义一致；同一段也出现“装载器行为”的同义用法，没有第三个所指。

判定：CLOSED。

#### 6.6 `丢弃后重建` 去重 —— CLOSED

```text
$ grep -c "丢弃后重建" rewrite-notes/01-stage-kernel/03-kmain-cstart.md
1
```

全文只剩 §4.2 一处（L1001）；§5.3 已改为“次序细节见 §4.2，不重复”，不再复述该短语。

判定：CLOSED。

## FINDING RD3-R2-CERT-01（P0）

位置：§2.1，L184 与 L186（“次序承重”小结及其前一段）。

原文：

> 第一次拷贝在循环前给 `kinfo.boot_procs` 留一份静态快照；循环与第二次拷贝都不读它，
> `proc_init` 读的是 `image[]`，所以真正被消费的是第二次拷贝。
>
> **次序承重**：步骤 2 必须早于 `cstart`（`prot_init` 读 `kinfo`）；步骤 5 必须早于
> `proc_init`（boot image 表的读者）；步骤 3 与前后无依赖。

问题一：步骤 3 不是“与前后无依赖”。步骤 3
`machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));` 通过 `env_get` 读
全局 `kinfo.param_buf`：

```text
$ sed -n '505,508p' minix3/minix/kernel/main.c
char *env_get(const char *name)
{
	return get_value(kinfo.param_buf, name);
}
```

全局 `kinfo` 的 `param_buf` 由步骤 2 的 `memcpy(&kinfo, local_cbi, sizeof(kinfo))`
（main.c:128）填入；把步骤 3 提前到步骤 2 之前，`param_buf` 尚未被填入（0 或未初始化
字节），`get_value`（main.c:481）会在 `for (envp = (char *) params; *envp != 0;)` 处
解引用这个值。步骤 3 必须晚于步骤 2，这是一条真实的承重依赖。

问题二：步骤 5 与 `proc_init` 没有承重关系。`proc_init`（proc.c:119）只遍历进程表与
特权表、调用 `arch_proc_reset`，不读 `image[]`，也不读 `kinfo.boot_procs`：

```text
$ sed -n '119,140p' minix3/minix/kernel/proc.c | grep -n "image\|boot_procs"
（无输出）
```

`image[]` 的读者是 `proc_init` 之后的启动循环（main.c:171 `ip = &image[i];`）；
`kinfo.boot_procs` 在启动循环之后由第二次 `memcpy`（main.c:275）更新，随后由用户态
VM 读取（`servers/vm/main.c:266`、`:498`）。第一次拷贝在第二次拷贝之前没有内核读者。
因此“步骤 5 必须早于 `proc_init`（boot image 表的读者）”是把“启动循环”误写成
`proc_init`，同时把不存在的依赖写成了承重项。

问题三：前一句“`proc_init` 读的是 `image[]`”同样不成立；应改为“`cstart` 之后的启动
循环读的是 `image[]`”。

修法建议：把小结改为“步骤 2 必须早于步骤 3（`env_get` 读 `kinfo.param_buf`）与
`cstart`（`prot_init` 读 `kinfo`）；步骤 5 的第一份拷贝没有内核读者，第二份拷贝在启动
循环之后由 VM 消费；`proc_init` 不读 boot image 表”。同时修正 L184 的读者归属。

判级依据：这是本段新增的事实陈述，直接错置了“次序承重”的两条依赖（一条漏、一条虚构），
属于 P0-fact 的“概念错误、虚构事实”。

## FINDING RD3-R2-CERT-02（P2）

位置：§4.6，L1177。

原文：“……所以用户陷阱返回后它保存的是用户 sp，不再是内核栈基址；`TrapReturnArch::
prepare_user_return` 在每次用户返回前把调度方传入的 `stack_top` 重新写进本 hart 的
`sscratch`，下一次进入时再交换。”

问题：时刻说反了。`riscv64_user_trap_vector` 的第一条指令
`csrrw sp, sscratch, sp`（trap_stub.rs:536）发生在**进入内核时**，此时 `sscratch` 变成
用户 sp；**返回用户态之前**，`prepare_user_return`（trap_return.rs:118-124）调用
`set_entry_stack_base(stack_top)` 把 `sscratch` 重写为内核栈顶。所以 `sret` 之后
`sscratch` 是内核栈基址，不是用户 sp。后半句“每次用户返回前重写”本身正确，前一句应改为
“用户陷阱进入内核后它保存的是用户 sp”。

影响：写返回路径的人若按字面理解，会在返回后把 `sscratch` 当用户 sp 用；P2 是因为紧接着
的分句已经把正确重写时机说清，且 M8 也记了返回前重锚。

## FINDING RD3-R2-CERT-03（P2）

位置：§4.1，L966。

原文：“……把启动信息的七个 slice（代码字段名：`memmap`、`memmap_sources`、
`startup_objects`、模块表、平台源、参数缓冲、保留区间）落进 `.bss` 里的固定存储……”

问题：引导语说“代码字段名”，但七项里只有三项是真字段名；`模块表`、`平台源`、
`参数缓冲`、`保留区间` 是中文描述，对应的字段是 `boot_modules`、`platform_sources`、
`param_buf`、`reserved_regions`。同一节 L974 又用另一套标签
（“保留区间、memmap、模块表+名字池、平台源列表、启动参数缓冲、`memmap_sources`、
`startup_objects`”）列了同样的七块，读者需要二次对表。

修法建议：七项全部写成代码字段名；或把引导语改成“其中三个易混字段的代码名是
`memmap`、`memmap_sources`、`startup_objects`”，并把四个中文条目明确标为释义。

## FINDING RD3-R2-CERT-04（P2）

位置：§5.1，L1234。

原文：“13 个 trap 测试覆盖的是 `init` 写出的门表元数据（这些门随后会被
`install_trap_stubs` 覆盖）：异常区存在性、保留向量非存在、软中断 DPL=3、PIC 区
DPL=0、NMI/#DF 的 IST；post-install 的 41 门、`user_accessible` 与 `set_gate_ist`
没有直接断言（设计批 D2-06）。”

问题：13 这个数字对（`grep -c '#\[test\]'` 输出 13），但构成不是全部 `init` 门表元数据。
其中 9 条是 `idt_init_*`（init 写出的门表），2 条是 `set_handler_sets_dpl_correctly`/
`set_handler_writes_handler_address`（set_handler 的写入），2 条是
`idt_entry64_size_is_16_bytes`/`idt_ptr_size_is_10_bytes`（`IdtEntry64`/`IdtPtr` 的
结构布局）。冒号后的清单只列了 9 条 init 测试的主题，读者会以为 13 条都在测 init 元数据。

修法建议：改为“13 条里 9 条覆盖 `init` 写出的门表元数据；另有 2 条 `set_handler` 与
2 条结构布局”，或把主语收窄为“门表相关测试里，9 条覆盖……”。

## 准入汇总

| 编号 | 级别 | 位置 | 一句话 |
|---|---|---|---|
| RD3-R2-CERT-01 | P0 | §2.1 | “次序承重”小结漏掉步骤 3→步骤 2 的真实依赖，并把启动循环误写成 `proc_init` 的步骤 5 依赖 |
| RD3-R2-CERT-02 | P2 | §4.6 | “用户陷阱返回后它保存的是用户 sp”把进入时刻说成返回时刻 |
| RD3-R2-CERT-03 | P2 | §4.1 | “代码字段名”只覆盖七项中的三项，同节后文另用一套标签 |
| RD3-R2-CERT-04 | P2 | §5.1 | 13 条 trap 测试的构成被概括为全部 init 元数据，实际含 2 条 set_handler 与 2 条布局测试 |

certify 新准入 = 4
