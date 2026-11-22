# D03 独立反证报告（P-REFUTE，round1）

角色：只读反证者。未读作者结论、其他角色报告、历史 panel。
文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
C 源：`minix3/`；Rust：`os/`。所有命令在仓库根 `/home/xzhao/github/minix-rs` 执行。

判定口径：CONFIRM = 原文主张与源码/可复算读数一致；REFUTE = 与源码或同文档另一处直接冲突；UNCLEAR = 取证不足或主张不可判定。

---

## CLAIM 01

原文 §2.2：`cstart` 的固定序列是 `prot_init()` → 读环境变量 → `init_clock()` → 读环境变量 → `intr_init(0)` → `arch_init()`。

命令与证据：

```
$ grep -n "void cstart\|prot_init()\|init_clock()\|intr_init(0)\|arch_init()" minix3/minix/kernel/main.c
403:void cstart(void)
411:  prot_init();
418:  init_clock();
472:  intr_init(0);
474:  arch_init();
```

函数体（`sed -n '403,476p'`）显示环境变量读取（`VERBOSEBOOTVARNAME`、`ac_layout`、`no_apic`/`no_smp` 等）都夹在四次调用之间，与原文一致；`strlcpy(kinfo.release/version)` 与 `kuserinfo` 初始化也在这条序列内，原文没有省略到失真。

## VERDICT: CONFIRM

---

## CLAIM 02

原文 §2.5：`prot_load_selectors` 的指令次序为 `lgdt` → `idt_init()` → `idt_reload()` → `lldt` → `ltr` → 重载 CS → 重载 DS/ES/FS/GS/SS；且 `ltr` 必须在 `lgdt` 之后。

命令与证据：

```
$ sed -n '/void prot_load_selectors/,/^}/p' minix3/minix/kernel/arch/i386/protect.c
  x86_lgdt(&gdt_desc);	/* Load gdt */
  idt_init();
  idt_reload();
  x86_lldt(LDT_SELECTOR); 	/* Load bogus ldt */
  x86_ltr(TSS_SELECTOR(booting_cpu));

  x86_load_kerncs();
  x86_load_ds(KERN_DS_SELECTOR);
  ...
```

次序逐行对上。注释 "called by both prot_init by the BSP and the early AP booting code in mpx.S by secondary CPU's" 也在源文件里逐字存在。

## VERDICT: CONFIRM

---

## CLAIM 03

原文 §2.4：`TSS_TYPE` 展开为 `(AVL_286_TSS | DESC_386_BIT)`；`tss_init` 写 `sp0 = kernel_stack - X86_STACK_TOP_RESERVED`、同一值赋给 `k_percpu_stacks[cpu]`、`iobase = sizeof(struct tss_s)`；`X86_STACK_TOP_RESERVED = 2 * sizeof(reg_t)`。

命令与证据：

```
$ grep -n "TSS_TYPE\|k_percpu_stacks\|tss_init" minix3/minix/kernel/arch/i386/protect.c
18:#define INT_GATE_TYPE	(INT_286_GATE | DESC_386_BIT)
19:#define TSS_TYPE	(AVL_286_TSS  | DESC_386_BIT)
29:u32_t k_percpu_stacks[CONFIG_MAX_CPUS];
154:int tss_init(unsigned cpu, void * kernel_stack)
176:	k_percpu_stacks[cpu] = t->sp0 = ((unsigned) kernel_stack) - X86_STACK_TOP_RESERVED;

$ grep -n "X86_STACK_TOP_RESERVED" minix3/minix/kernel/arch/i386/include/archconst.h
149:#define X86_STACK_TOP_RESERVED	(2 * sizeof(reg_t))
```

`tss_init` 函数体（`sed -n '154,205p'`）另有 `t->iobase = sizeof(struct tss_s);` 与 `*((reg_t *)(t->sp0 + 1 * sizeof(reg_t))) = cpu;`，与原文引用一致。

## VERDICT: CONFIRM

---

## CLAIM 04

原文 §2.4：`tss_init` 的返回值在 C 树里恰好有五个调用点且全部被丢弃，列出 `earm/arch_system.c:113`、`i386/protect.c:338`、`i386/include/arch_smp.h:18`（宏内）、`i386/arch_smp.c:272`、`i386/arch_system.c:256`。

命令与证据：

```
$ grep -rn "tss_init(" minix3/minix/kernel/ | grep -v "int tss_init"
minix3/minix/kernel/arch/earm/arch_system.c:113:	tss_init(0, get_k_stack_top(0));
minix3/minix/kernel/arch/i386/arch_smp.c:272:		tss_init(cpu, get_k_stack_top(cpu));
minix3/minix/kernel/arch/i386/arch_system.c:256:	tss_init(0, get_k_stack_top(0));
minix3/minix/kernel/arch/i386/protect.c:338:  tss_init(0, &k_boot_stktop);
minix3/minix/kernel/arch/i386/include/arch_smp.h:18:	  tss_init(0, get_k_stack_top(0));	\
```

五处文件与行号逐一吻合，无第六处；五处都是表达式语句，返回值均未使用。

## VERDICT: CONFIRM

---

## CLAIM 05

原文 §2.3：`GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS)`；`CONFIG_MAX_CPUS = 1` 时 `GDT_SIZE = 6 + 1 = 7`；`IDT_SIZE = 256`、`DESC_SIZE = 8`。

命令与证据：

```
$ grep -n "define IDT_SIZE\|define DESC_SIZE\|define GDT_SIZE\|define TSS_INDEX" minix3/minix/kernel/arch/i386/include/archconst.h
11:#define IDT_SIZE 256	/* the table is set to it's maximal size */
19:#define TSS_INDEX_FIRST      6
20:#define TSS_INDEX(cpu)       (TSS_INDEX_FIRST + (cpu)) /* per cpu kernel tss */
21:#define GDT_SIZE             (TSS_INDEX(CONFIG_MAX_CPUS))	/* LDT descriptor */
31:#define DESC_SIZE	8
$ grep -rn "define CONFIG_MAX_CPUS" minix3/minix/kernel/kernel.h
8:#define CONFIG_MAX_CPUS	1
```

`printf '%d\n' $((6+1))` 输出 7。算术与常量全部对上。

## VERDICT: CONFIRM

---

## CLAIM 06

原文 §2.5：`gate_table_pic` 16 条、`gate_table_exceptions` 23 条，合计 39 条门；其中 DPL=3 的门共 6 条（四条软中断 32–35 加 breakpoint/overflow 向量 3、4）。

命令与证据：

```
$ awk '/gate_table_pic\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'hwint'
16
$ awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -cE '_VECTOR'
23
$ awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'USER_PRIVILEGE'
6
```

`idt_init` 的填充源也核对：

```
$ sed -n '/^void idt_init/,/^}/p' minix3/minix/kernel/arch/i386/protect.c
void idt_init(void)
{
	idt_copy_vectors_pic();
	idt_copy_vectors(gate_table_exceptions);
}
$ grep -n "idt_copy_vectors_pic" -A3 minix3/minix/kernel/arch/i386/protect.c
255:void idt_copy_vectors_pic(void)
256-{
257-	idt_copy_vectors(gate_table_pic);
```

16 + 23 = 39 成立；"两层结构"（外层固定两表、内层循环 + `int_gate`）与源码一致。

## VERDICT: CONFIRM

---

## CLAIM 07

原文 §4.4（第 1321 行）：`set_handler` 写门时把 IST 一起写：向量 2 写 1、向量 8 写 2、其余写 0；硬件按 `TSS.ist[ist-1]` 取值，实际用到 `ist[0]`/`ist[1]`。

命令与证据：

```
$ sed -n '270,288p' os/arch/src/x86_64/trap_entry.rs
    fn set_handler(&mut self, vector: InterruptVector, handler: VirBytes, user_accessible: bool) {
        // Sole writer of every gate field: handler, DPL, IST and type.
        let dpl = if user_accessible { 3 } else { 0 };
        let ist = match vector.get() {
            2 => 1,
            8 => 2,
            _ => 0,
        };
        self.set_gate(vector.get(), handler.get(), dpl, ist);
    }
```

`install_idt_handlers` 对 41 条门逐门断言 `expected_ist`（`2 => 1, 8 => 2`），见 `os/arch/src/x86_64/trap_stub.rs:613-650`。硬件取值规则（IST 字段 1/2 对应 `TSS.ist[0]`/`ist[1]`）与 SDM 语义一致；Rust `Tss64` 只有 `ist: [u64; 7]`，`setup_tss_for_cpu` 只填 `ist[0]`/`ist[1]`。

## VERDICT: CONFIRM

---

## CLAIM 08

原文 §4.4（第 1322 行）："`set_handler` 写门时 IST 一律写 0（它没有 IST 参数），随后 2/8 两个向量由 `set_gate_ist` 恢复"；并称 `set_handler` 的调用者因此"多一次簿记"（§3.7 表格第三行亦同）。

命令与证据：

```
$ grep -rn "set_gate_ist" os/ --include=*.rs
（无匹配；仅 target/ 增量目录的权限告警）
$ sed -n '276,287p' os/arch/src/x86_64/trap_entry.rs
        // Sole writer of every gate field: handler, DPL, IST and type.
        // The installer passes the canonical `user_accessible(vector)` and
        // this function maps NMI/#DF to IST1/IST2. `init()` writes only
        // absent placeholders.
        let dpl = if user_accessible { 3 } else { 0 };
        let ist = match vector.get() { 2 => 1, 8 => 2, _ => 0 };
        self.set_gate(vector.get(), handler.get(), dpl, ist);
```

三重矛盾：

1. `set_gate_ist` 函数在全仓不存在；`set_handler` 有 IST 逻辑，自己写 1/2。
2. 同一节第 1321 行（CLAIM 07）已经正确写出"`set_handler` 写门时把 IST 一起写"，第 1322 行是与它重复且相反的陈旧段落。
3. §3.7 的表格"IST 恢复：`set_handler` 写 IST=0 后由安装器恢复"因此也不成立。

这是文档内的直接事实错误，且同一机制在相邻两段给出互相排斥的说法。

## VERDICT: REFUTE

---

## CLAIM 09

原文 §3.7（第 1095 行）："`load()` 本身不检查，真机载体不调 `install_trap_stubs`（§5.2）、门里保持占位 0 也能通过，说明 `load()` 本身不检查门有效性"。

命令与证据：

```
$ grep -n "install_trap_stubs" os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs
18:use minix_arch::{ProtectionArch, TrapEntryArch, CurrentProtection, CurrentTrapEntry, install_trap_stubs};
86:install_trap_stubs(&mut trap);
```

同一文档 §5.2（第 1491 行）也写 x86_64 载体"还调 `install_trap_stubs`……门里是真入口"。文档内部两处互斥：§3.7 的"真机载体不调"为假。`load()` 不检查门有效性这一结论本身由源码可证（`trap_entry.rs:239-264` 的 `load` 只构造 `IdtrPtr` 并发 `lidt`，无 `gate_present` 校验），但原文用错误证据支撑它。

## VERDICT: REFUTE

---

## CLAIM 10

原文 §3.7/§4.4：Rust IDT 共 256 项，实际装 41 条门；41 = 18 异常（0–8、10–14、16–19）+ 4 软中断（32–35）+ 16 PIC（0x50–0x57、0x70–0x77）+ 3 单独向量（0xF0、0xF1、0xFF）；`install_idt_handlers` 安装后断言 DPL/IST/type 并核对 gate presence 与 stub 覆盖相等。

命令与证据：

```
$ grep -n "const IDT_ENTRIES" os/arch/src/x86_64/trap_entry.rs
113:const IDT_ENTRIES: usize = 256;
$ sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'
41
$ sed -n '168,230p' os/arch/src/x86_64/trap_stub.rs
# STUB_VECTORS 实测含 0–8、10–14、16–19；32–35；80–87、112–119；240、241、255
```

`install_idt_handlers`（`trap_stub.rs:613-650`）包含三段断言：`dpl != user_accessible`、`ist != expected_ist`、`kind != GATE_TYPE_INTERRUPT`，以及 `gate_present(vector) == has_stub(vector)` 全 0..=255 循环。计数与断言结构均对上。

## VERDICT: CONFIRM

---

## CLAIM 11

原文 §5.1：九个 arch 文件的 `#[test]` 计数为 16 / 13 / 12 / 0 / 1 / 3 / 0 / 1 / 7；arch crate `src/**/*.rs` 属性口径 376 个 / 52 文件；riscv64 `trap_stub.rs` 的 grep 7 条里含一条注释命中。

命令与证据：

```
$ for f in os/arch/src/x86_64/{protection,trap_entry,trap_stub}.rs \
           os/arch/src/arm64/{protection,trap_entry,trap_stub}.rs \
           os/arch/src/riscv64/{protection,trap_entry,trap_stub}.rs; do
    printf "%s: " "$f"; grep -c '#\[test\]' "$f"; done
os/arch/src/x86_64/protection.rs: 16
os/arch/src/x86_64/trap_entry.rs: 13
os/arch/src/x86_64/trap_stub.rs: 12
os/arch/src/arm64/protection.rs: 0
os/arch/src/arm64/trap_entry.rs: 1
os/arch/src/arm64/trap_stub.rs: 3
os/arch/src/riscv64/protection.rs: 0
os/arch/src/riscv64/trap_entry.rs: 1
os/arch/src/riscv64/trap_stub.rs: 7
$ grep -rh '#\[test\]' os/arch/src --include=*.rs | wc -l
377
$ grep -rn '#\[test\]' os/arch/src --include=*.rs | grep -vE '^\S+:[0-9]+:\s*#\[test\]\s*$'
os/arch/src/riscv64/trap_stub.rs:906:    // `#[cfg(target_arch = "riscv64")]`, so a `#[test]` written here
$ grep -rl '#\[test\]' os/arch/src --include=*.rs | wc -l
52
```

377 行 grep 命中减 1 条注释命中 = 376 个真实属性，52 个文件；"7 含一条注释命中"也精确对上。

## VERDICT: CONFIRM

---

## CLAIM 12

原文 §2.7：`grep -c '#if'` 于 i386 `protect.c` 输出 0；两份 `protect.c` 各自定义一次 `prot_init`；`minix3/minix/kernel/arch/` 只有 `i386` 与 `earm` 两个目录。

命令与证据：

```
$ grep -c '#if' minix3/minix/kernel/arch/i386/protect.c
0
$ grep -rn "^void prot_init" minix3/minix/kernel/arch/*/protect.c
minix3/minix/kernel/arch/earm/protect.c:77:void prot_init(void)
minix3/minix/kernel/arch/i386/protect.c:321:void prot_init(void)
$ ls minix3/minix/kernel/arch/
earm
i386
```

三条读数全部与原文一致。earm `prot_init` 的函数体（`write_vbar` + 四行页表重建 + `prot_init_done = 1`）也与 §2.7 的引用一致。

## VERDICT: CONFIRM

---

## CLAIM 13

原文 §2.2：`init_clock` 只做三件事（`memset(kclockinfo)`、读 `hz` 并夹在 `[2, 50000]`、`memset(kloadinfo)`），不编程硬件定时器；`intr_init(0)` 只按 ICW 序列编程两片 8259，master 掩码写 `~(1 << CASCADE_IRQ)`、slave 收尾写 `~0`，不填 IDT。

命令与证据：

```
$ sed -n '/^init_clock(void)/,/^}/p' minix3/minix/kernel/clock.c   # 40-68 行
	memset(&kclockinfo, 0, sizeof(kclockinfo));
	value = env_get("hz");
	if (value != NULL) kclockinfo.hz = atoi(value);
	if (value == NULL || kclockinfo.hz < 2 || kclockinfo.hz > 50000)
		kclockinfo.hz = DEFAULT_HZ;
	memset(&kloadinfo, 0, sizeof(kloadinfo));
$ sed -n '28,60p' minix3/minix/kernel/arch/i386/i8259.c
      outb( INT_CTLMASK, ~(1 << CASCADE_IRQ)); /* IRQ 0-7 mask */
      ...
      outb( INT2_CTLMASK, ~0);		/* IRQ 8-15 mask */
```

`init_clock` 函数体没有 `outb`/定时器编程；`intr_init` 没有对 `idt[]` 的任何写入。

## VERDICT: CONFIRM

---

## CLAIM 14

原文 §4.3/§2.5：`load_with_tss` 在 `lgdt` 之前读目标槽的 access 字节，命中 busy（0x0B）就带名停机；装载序列为 `lgdt` → 远跳重载 CS → 重载 DS/ES/SS → `xor`+`mov fs/gs` → `ltr`。

命令与证据：

```
$ sed -n '451,535p' os/arch/src/x86_64/protection.rs
        let gdt_idx = GDT_TSS_FIRST_INDEX + idx * GDT_SLOTS_PER_TSS;
        let tss_access = ((self.gdt[gdt_idx] >> 40) & 0xFF) as u8;
        assert!((tss_access & 0x0F) != TSS64_BUSY_TYPE,
            "load called twice: TSS descriptor busy (cpu {})", cpu_id);
        ...
        asm!("lgdt [{gdtr}]", ...);
        // 2. Far jump to reload CS ... mov/push/push/retfq
        // 3. mov ds/es/ss
        // 4. xor {0:e}; mov fs; mov gs
        // 5. ltr {tr_sel}
```

次序逐条对应；`TSS64_BUSY_TYPE = 0x0B`（`protection.rs:110`），busy 检查发生在任何 `lgdt` 之前。测试 `load_rejects_busy_tss_descriptor`（`protection.rs:654`）为这条提供宿主侧证据。

## VERDICT: CONFIRM

---

## CLAIM 15

原文 §4.6：aarch64 的 `ProtectionArch::init` 写 `TPIDR_EL1` 而非 `SP_EL1`，`set_kernel_stack` 是文档化 no-op，`load` 只发一条 `isb`；riscv64 的 `ProtectionArch::load` 没有需要执行的步骤；riscv 用户腿第一条指令无条件 `csrrw sp, sscratch, sp`，不在入口按 `SPP` 判定。

命令与证据：

```
$ sed -n '28,31p;90,101p;133,163p' os/arch/src/arm64/protection.rs
//! | `init` ... program `TPIDR_EL1` ...（注释原文）
fn set_entry_sp(base: VirBytes) { ... }        # 写 TPIDR_EL1
fn set_kernel_stack(...) { /* no-op under EL1h ... */ }
fn load(&self) { ... asm!("isb"); }
$ sed -n '127,132p' os/arch/src/riscv64/protection.rs
fn load(&self) {
    // On RISC-V, sscratch is already set by init().
    // stvec is set by TrapEntryArch::load().
}
$ grep -n "csrrw\|spp" os/arch/src/riscv64/trap_stub.rs
536:    "    csrrw sp, sscratch, sp",  // sp ← kstack top; sscratch ← user sp
659:    "    csrrw sp, sscratch, sp", // sp ← user sp ...
405-...（返回侧重锚处才读 SPP，见 489-492 一带）
```

用户腿 536 行是入口第一条实质指令且无 `SPP` 条件分支；`SPP` 仅在返回侧重锚 `stvec` 时读取（`sed -n '480,500p'` 显示 `la t2, riscv64_kernel_trap_vector` 的条件路径）。三处陈述均与实现一致。

## VERDICT: CONFIRM

---

## 汇总

| # | 主张 | 判定 |
|---|---|---|
| 01 | cstart 调用序列与次序 | CONFIRM |
| 02 | prot_load_selectors 指令次序、ltr 在 lgdt 后 | CONFIRM |
| 03 | TSS_TYPE / sp0 / k_percpu_stacks / iobase | CONFIRM |
| 04 | tss_init 五处调用点、返回值全丢弃 | CONFIRM |
| 05 | GDT_SIZE=7 / IDT_SIZE=256 / DESC_SIZE=8 | CONFIRM |
| 06 | 39 条门 = 16 + 23，DPL=3 共 6 条 | CONFIRM |
| 07 | set_handler 自己写 IST（2→1、8→2） | CONFIRM |
| 08 | "set_handler 写 IST=0，随后 set_gate_ist 恢复" | **REFUTE** |
| 09 | "真机载体不调 install_trap_stubs" | **REFUTE** |
| 10 | 41 门构成与安装后断言 | CONFIRM |
| 11 | 测试计数 16/13/12/0/1/3/0/1/7、376/52 | CONFIRM |
| 12 | protect.c 无 #if、两份 prot_init、两架构目录 | CONFIRM |
| 13 | init_clock 三件事 / intr_init 只编程 8259 | CONFIRM |
| 14 | busy 检查在 lgdt 前、五步装载次序 | CONFIRM |
| 15 | aarch64 TPIDR_EL1/仅 isb、riscv load 空、用户腿无条件交换 | CONFIRM |

两条 REFUTE 都指向同一类问题：文档里残留了与现行代码不符的旧叙述，且都与其同文档另一处正确叙述直接冲突。

- CLAIM 08：`minix3-rs` 现码中不存在 `set_gate_ist`；`set_handler` 是"handler/DPL/IST/type 全字段唯一写者"。文档 §4.4 第 1321 行已正确描述，第 1322 行整段（含 §3.7 表格"IST 恢复"行）是陈旧重复段，应删除或改写。位置：文档第 1322 行、§3.7 第 1082 行"IST 恢复"表格行。
- CLAIM 09：`os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs:86` 明确调用 `install_trap_stubs`，与 §3.7 第 1095 行的"真机载体不调"相反；文档 §5.2 第 1491 行是正确的。该句应删除或改为"仅由 `test-user-trap` 的生产 `init_protection` 覆盖真机门安装"。

补充（未单列为 CLAIM，供归因）：这两条不是源码与文档的跨源偏差，而是文档内部同一机制的两处叙述互斥，说明 §3.7 与 §4.4 在描述 `init/load` 顺序时是两份独立改写、未做对账。建议修复时以源码为唯一来源统一重写这两处。
