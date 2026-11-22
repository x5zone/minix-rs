# 03-kmain-cstart · Round 2 · P-REFUTE（反证者）

只读复核。本轮不修改仓库，只在本文件记录判定。复核对象是 `rewrite-notes/01-stage-kernel/03-kmain-cstart.md` 中 15 条可证伪主张，重点覆盖第一轮修改段：IST 单写者、三份真机载体的调用面、C 的 39 条门与 Rust 的 41 条门、`lgdt`/`tss_init`、LDT、`init_protection_order_pin.rs` 的四条源码钉、测试计数。

判定口径：CONFIRM 表示按文档给出的命题去构造反例失败，命题成立；REFUTE 表示找到与命题直接冲突的源码或读数；UNCLEAR 表示证据不足或以现有源码无法判定。命令均在仓库根执行；行号以本轮读到的源码为准。

一条方法声明：本轮没有读作者结论、其他角色报告或历史 panel。复核中途有一次宽域 `rg` 意外匹配到 `panel/` 下的文本，该次输出未用作任何判定的证据；下文所有证据均来自 `os/`、`minix3/` 与目标文档本身。

---

## CLAIM 01 — C 39 门与 Rust 41 门的拆解算术

文档主张（§3.7，第 1093 行；§2.5，第 689–690 行）：C 侧共 39 条门 = 16 条 PIC + 23 条异常/软中断；23 条里含向量 9、不含向量 15；DPL=3 的共 6 条；Rust 侧 41 条 = 18 条异常 + 4 条软中断 + 16 条 PIC + 3 条单独向量，41 = 39 − 1 + 3。

### 证据

```
$ awk '/gate_table_pic\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'hwint'
16
$ awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -cE '_VECTOR'
23
$ awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'USER_PRIVILEGE'
6
```

表体逐行读出：0–14 十五行、16–19 四行、32–35 四行，合计 23 行；其中 `copr_seg_overrun, COPROC_SEG_VECTOR` 落在向量 9，向量 15 无表项；`USER_PRIVILEGE` 六行分别是 `breakpoint_exception`(3)、`overflow`(4) 与四条 `*_ORIG`/`*_UM` 软中断。

`minix3/minix/include/arch/i386/include/interrupt.h:28-31` 给出四条软中断的实际编号：

```
#define KERN_CALL_VECTOR_ORIG  32
#define IPC_VECTOR_ORIG        33
#define KERN_CALL_VECTOR_UM    34
#define IPC_VECTOR_UM          35
```

Rust 侧的 41 项在 `os/arch/src/x86_64/trap_stub.rs` 的 `STUB_VECTORS` 里逐项可数：0–8（9 项）、10–14（5 项）、16–19（4 项）共 18 项异常；32–35 四项；0x50–0x57 与 0x70–0x77 共 16 项；0xF0、0xF1、0xFF 三项。18 + 4 + 16 + 3 = 41，39 − 1 + 3 = 41。

## VERDICT 01 — CONFIRM

四个拆解数字（16、23、6、41）与两个算术恒等式全部复现；向量 9 含、15 不含也与表体一致。

---

## CLAIM 02 — `STUB_VECTORS` 的 41 项计数与 `IDT_ENTRIES = 256` 的行号

文档主张（§3.7，第 1087–1090 行）：`os/arch/src/x86_64/trap_entry.rs:114` 为 `const IDT_ENTRIES: usize = 256;`；用给定 `sed`/`grep` 口径数 `STUB_VECTORS` 得 41。

### 证据

```
$ grep -n "const IDT_ENTRIES" os/arch/src/x86_64/trap_entry.rs
114:const IDT_ENTRIES: usize = 256;
$ sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'
41
```

## VERDICT 02 — CONFIRM

行号与计数都逐字命中；该 `grep` 口径只数以向量号开头的表项行，不会把 `// IPI: scheduler ...` 之类注释行算进去。

---

## CLAIM 03 — C 与 Rust 的 0xF0/0xF1 编号相反

文档主张（§3.7，第 1093 行）：C 的 0xF0 是定时器、0xF1 是调度 IPI；Rust 相反，0xF0 是调度、0xF1 是定时器。

### 证据

```
$ grep -rn "define APIC_TIMER_INT_VECTOR\|define APIC_SMP_SCHED_PROC_VECTOR" minix3/minix/kernel/arch/i386/apic.h
90:#define APIC_TIMER_INT_VECTOR		0xf0
91:#define APIC_SMP_SCHED_PROC_VECTOR	0xf1
```

`os/arch/src/x86_64/trap_stub.rs` 的 `STUB_VECTORS` 末尾注释写明 `(0xF0, false)` 为 scheduler、`(0xF1, false)` 为 LAPIC local timer，并自述“C `APIC_TIMER_INT_VECTOR = 0xf0` — relocated to 0xf1 here”。

## VERDICT 03 — CONFIRM

两侧编号方向确实相反；文档还给了迁移理由（0xF0 已被调度 IPI 占用），与源码注释一致。

---

## CLAIM 04 — IST 单写者：`set_handler` 内部按向量写 IST，仓内没有 `set_gate_ist`

文档主张（§3.7，第 1076 行；§4.4，第 1260 行）：`X86_64TrapEntry::init` 只建 256 条空占位；`set_handler` 是唯一写者，DPL 取自 `user_accessible(vector)`、IST 取自 NMI/#DF 表（2→IST1、8→IST2）、类型恒为中断门；仓内没有 `set_gate_ist`。

### 证据

```
$ grep -rn "set_gate_ist" os/arch/src os/kernel/src
（无输出）
$ sed -n '270,287p' os/arch/src/x86_64/trap_entry.rs
    fn set_handler(
        &mut self,
        vector: InterruptVector,
        handler: VirBytes,
        user_accessible: bool,
    ) {
        // Sole writer of every gate field: handler, DPL, IST and type.
        ...
        let ist = match vector.get() {
            2 => 1,
            8 => 2,
            _ => 0,
        };
        self.set_gate(vector.get(), handler.get(), dpl, ist);
    }
```

`os/arch/src/x86_64/trap_stub.rs:227` 的 `has_stub` 与 `os/arch/src/x86_64/trap_stub.rs:789` 的 `test_stub_coverage_matches_idt_gates` 支撑“覆盖核对在宿主测试里”这半句；`os/arch/src/x86_64/trap_entry.rs:131` 的 `gate_present` 提供读数。

## VERDICT 04 — CONFIRM

`set_handler` 确实是三参数入口，IST 在函数体内按向量 2/8 写入；全仓 `os/` 无 `set_gate_ist`。第一轮把“`set_handler` 写 IST=0、安装器再恢复”的说法改为“单写者”，与实现一致。

---

## CLAIM 05 — §4.4 关于 `set_handler` 与 IST 只有一段说明

文档主张（§4.4 第 1321–1322 行区域，上下文为“`set_handler` 写门时把 IST 一起写”）：该节对 IST 写入门的过程只有一处交代，读者不会读到两套说法。

### 证据

```
$ sed -n '1319,1323p' rewrite-notes/01-stage-kernel/03-kmain-cstart.md
（空行）
`set_handler` 写门时把 IST 一起写：向量 2 写 1、向量 8 写 2、其余写 0。硬件按 `TSS.ist[ist-1]` 取值，所以实际用到的是 TSS 的 `ist[0]`/`ist[1]`（`setup_tss_for_cpu` 只填这两格）。`STUB_VECTORS` 的每项还带一个“是否由 CPU 压入错误码”的布尔值，但布尔值只被宿主测试 `test_errcode_classification_matches_sdm` 使用；汇编入口由 `TRAPSTUB vec err` 字面量实例化，不读这个布尔值。
`set_handler` 自己把 IST 一起写：向量 2 写 1、向量 8 写 2、其余写 0；仓内没有 `set_gate_ist`。硬件按 `TSS.ist[ist-1]` 取值，所以实际用到的是 TSS 的 `ist[0]`/`ist[1]`（`setup_tss_for_cpu` 只填这两格）。`STUB_VECTORS` 的每项还带一个“是否由 CPU 压入错误码”的布尔值，但布尔值只被宿主测试 `test_errcode_classification_matches_sdm` 使用；汇编入口由 `TRAPSTUB vec err` 字面量实例化，不读这个布尔值。
（空行）
```

两行除首句外逐字相同，是同一段落的重复粘贴；首句也只差“把 IST 一起写”与“自己把 IST 一起写”，语义相同。

### 影响

这不是事实错误，而是第一轮修改留下的编辑残留：读者在同一节读到两遍同一段，会怀疑是否有第二套机制；若后续只改其中一行，两段就会重新分叉。修复动作是删掉其中一段（保留带“仓内没有 `set_gate_ist`”的那句即可）。

## VERDICT 05 — REFUTE

“只有一段说明”为假：第 1321 行与第 1322 行是近乎逐字重复的两段。

---

## CLAIM 06 — `load()` 本身不检查门的有效性，覆盖核对由宿主测试与真机载体承担

文档主张（§3.7，第 1095 行）：`gate_present` 的注释指向宿主测试 `test_stub_coverage_matches_idt_gates`；`load()` 本身不检查；真机载体在 `load` 之前调用 `install_trap_stubs`。

### 证据

```
$ sed -n '/fn load(&self)/,/^    }/p' os/arch/src/x86_64/trap_entry.rs
    fn load(&self) {
        let idtr = IdtPtr { ... };
        // SAFETY / LIFETIME CONTRACT: ...
        unsafe {
            core::arch::asm!("lidt [{idtr}]", ...);
        }
    }
```

`load()` 只构造 `IdtPtr` 并发一条 `lidt`，函数内没有 `gate_present`、没有遍历、没有断言。宿主测试名在 `os/arch/src/x86_64/trap_stub.rs:789` 确实存在。

## VERDICT 06 — CONFIRM

“`load()` 不检查门”成立；§5.3 把它列为缺口并给出补法（装载前守卫），与实现现状一致，不是把已有机制写成了缺口。

---

## CLAIM 07 — `lgdt` 先于 `ltr`，且 Rust 在 `lgdt` 之前检查目标 TSS 槽的 busy 位

文档主张（§2.5，第 619、630 行）：C 的 `prot_load_selectors` 顺序为 `lgdt` → `idt_init` → `idt_reload` → `lldt` → `ltr`；`ltr` 写在 `lgdt` 之后，因为 TR 的选择子要被新 GDT 解释。Rust 的 `load_with_tss` 在 `lgdt` 之前读目标槽 access 字节，命中 busy 就带名停机，消息为 `load called twice: TSS descriptor busy`。

### 证据

```
$ sed -n '/void prot_load_selectors/,/^}/p' minix3/minix/kernel/arch/i386/protect.c
  x86_lgdt(&gdt_desc);	/* Load gdt */
  idt_init();
  idt_reload();
  x86_lldt(LDT_SELECTOR); 	/* Load bogus ldt */
  x86_ltr(TSS_SELECTOR(booting_cpu));
  x86_load_kerncs();
  ...
```

```
$ sed -n '452,465p' os/arch/src/x86_64/protection.rs
        let gdt_idx = GDT_TSS_FIRST_INDEX + idx * GDT_SLOTS_PER_TSS;
        let tss_access = ((self.gdt[gdt_idx] >> 40) & 0xFF) as u8;
        assert!(
            (tss_access & 0x0F) != TSS64_BUSY_TYPE,
            "load called twice: TSS descriptor busy (cpu {})",
            cpu_id
        );
```

检查发生在 `let gdtr = ...; asm!("lgdt [{gdtr}]")` 之前；同一文件 `os/arch/src/x86_64/protection.rs:456-457` 的注释写明 `ltr` 会把描述符类型置为 busy，先检查是为了避免裸 #GP。

## VERDICT 07 — CONFIRM

C 侧五条装载指令的顺序与文档逐行一致；Rust 侧“先查槽、后 `lgdt`”的次序与报错文本都命中。

---

## CLAIM 08 — LDT：C 的槽号 5、限长不是 0、`lldt` 真执行；Rust 不执行 `lldt`

文档主张（§2.3，第 453–462 行；§3.1，第 888 行；§4.7，第 1406 行）：C 把 LDT 占位放在 GDT 第 5 槽（`LDT_INDEX = 5`），`init_param_dataseg(..., 0, 0, ...)` 传入的 size 0 被 `--size` 解释成最大区段，`access = PRESENT | LDT` 使它成为 LDT 类型；`prot_load_selectors` 执行 `x86_lldt(LDT_SELECTOR)`；Rust 的 `load_with_tss` 不执行 `lldt`。

### 证据

```
$ grep -n "define LDT_INDEX\|define LDT_SELECTOR\|define TSS_INDEX_FIRST" minix3/minix/kernel/arch/i386/include/archconst.h
18:#define LDT_INDEX            5
28:#define LDT_SELECTOR SEG_SELECTOR(LDT_INDEX)
19:#define TSS_INDEX_FIRST      6
```

```
$ sed -n '/void sdesc/,/^}/p' minix3/minix/kernel/arch/i386/protect.c
  --size;			/* convert to a limit, 0 size means 4G */
```

`prot_init` 的两行在 `minix3/minix/kernel/arch/i386/protect.c` 中为 `init_param_dataseg(&gdt[LDT_INDEX], (phys_bytes) 0, 0, INTR_PRIVILEGE); /* unusable LDT */` 与 `gdt[LDT_INDEX].access = PRESENT | LDT;`。

Rust 侧：

```
$ grep -rn "lldt" os/arch/src/x86_64/protection.rs
531:            // Note: C loads LDTR via lldt(LDT_SELECTOR), but 64-bit mode
533:            // (slots 3/5 are the user-CS anchor/copy), so there is no lldt
535:            // C: x86_lldt(LDT_SELECTOR) — protect.c:313
```

`os/arch/src/x86_64/protection.rs` 的 GDT 常量把第 5 槽给用户 CS（`GDT_USER_CS_INDEX = 5`），没有 LDT 类型描述符。

## VERDICT 08 — CONFIRM

槽号、限长语义、`lldt` 的执行与 Rust 的缺席全部成立。文档没有替 LDT 编造用途，只按两条源码注释（`unusable LDT`、`Load bogus ldt`）叙述，这一点也与源码字面一致。

---

## CLAIM 09 — `tss_init`：五处 grep 命中、返回值全部丢弃、`iobase` 与限长关系

文档主张（§2.4，第 588 行与第 551 行）：`grep -rn "tss_init(" minix3/minix/kernel/ | grep -v "int tss_init"` 命中五处，返回值全部被丢弃；`iobase = sizeof(struct tss_s)` 高于 TSS 限长（限长为 `sizeof - 1`），即“没有 I/O 位图”。

### 证据

```
$ grep -rn "tss_init(" minix3/minix/kernel/ | grep -v "int tss_init"
minix3/minix/kernel/arch/earm/arch_system.c:113:	tss_init(0, get_k_stack_top(0));
minix3/minix/kernel/arch/i386/arch_smp.c:272:		tss_init(cpu, get_k_stack_top(cpu));
minix3/minix/kernel/arch/i386/arch_system.c:256:	tss_init(0, get_k_stack_top(0));
minix3/minix/kernel/arch/i386/protect.c:338:  tss_init(0, &k_boot_stktop);
minix3/minix/kernel/arch/i386/include/arch_smp.h:18:	  tss_init(0, get_k_stack_top(0));	\
```

四处调用语句与一处宏内复用，五处都是表达式语句或宏体，没有赋值或传参消费返回值。`tss_init` 的 `iobase = sizeof(struct tss_s);` 与 `sdesc` 的 `--size` 组合给出限长 `sizeof - 1`，`iobase` 恰好比限长大 1 字节。

## VERDICT 09 — CONFIRM

五处命中与文档列出的文件名、行号逐条对上；限长关系由 `sdesc` 的实现直接推出。

---

## CLAIM 10 — GDT 长度：C 为 7，Rust 为 70，槽位 3/4/5 的分配

文档主张（§2.3 第 441 行；§3.1 第 907–916 行）：C 的 `GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS) = 6 + 1 = 7`；Rust 的 `GDT_ENTRIES = GDT_TSS_FIRST_INDEX + MAX_CPUS * GDT_SLOTS_PER_TSS = 6 + 32 × 2 = 70`；Rust 第 3 槽是 SYSRET anchor、第 4 槽用户 DS、第 5 槽用户 CS。

### 证据

```
$ grep -n "define TSS_INDEX\|define GDT_SIZE" minix3/minix/kernel/arch/i386/include/archconst.h
20:#define TSS_INDEX(cpu)       (TSS_INDEX_FIRST + (cpu)) /* per cpu kernel tss */
21:#define GDT_SIZE             (TSS_INDEX(CONFIG_MAX_CPUS))	/* LDT descriptor */
$ grep -n "define CONFIG_MAX_CPUS" minix3/minix/kernel/kernel.h
8:#define CONFIG_MAX_CPUS	1
```

```
$ sed -n '40,80p' os/arch/src/x86_64/protection.rs
const GDT_USER_ANCHOR_INDEX: usize = 3;
const GDT_USER_DS_INDEX: usize = 4;
const GDT_USER_CS_INDEX: usize = 5;
const GDT_TSS_FIRST_INDEX: usize = 6;
const GDT_SLOTS_PER_TSS: usize = 2;
const GDT_ENTRIES: usize = GDT_TSS_FIRST_INDEX + MAX_CPUS * GDT_SLOTS_PER_TSS;
$ grep -rn "pub const MAX_CPUS" os/libs/minix-types/src/types/com.rs
51:pub const MAX_CPUS: usize = 32;
```

## VERDICT 10 — CONFIRM

7 与 70 都由源码常量直接复算；三个槽位索引逐字命中。

---

## CLAIM 11 — `init_protection_order_pin.rs` 的四条源码钉

文档主张（§5.1，第 1481 行）：四条启动顺序源码钉分别是 `store_protection → refresh_descriptors → load → program_entry_base`、`install_trap_stubs → trap.load`、`smp_init` 的 `set_kernel_stack → boot_ap`、BSP 逻辑号只来自 `elected_bsp_hw_id`（`bsp_identity_has_one_source`）。

### 证据

`os/kernel/tests/init_protection_order_pin.rs` 的四个 `#[test]`：

- `bsp_identity_has_one_source`：断言 `CurrentProtection::init(bsp_logical` 存在、`CurrentProtection::init(0` 不存在、`program_entry_base(bsp_logical` 存在；
- `protection_store_refresh_load_entry_base_order`：`store_protection(prot)` < `refresh_descriptors()` < `with_protection(|prot| prot.load())` < `program_entry_base(`；
- `trap_stubs_installed_before_idt_load`：`install_trap_stubs(&mut trap)` < `with_trap_entry(|trap| trap.load())`；
- `ap_stack_programmed_before_sipi`：`set_kernel_stack(logical` < `boot_ap(`（在 `smp.rs` 上做同样的源码切片）。

四条与文档的四句逐条对应，文件里的 `fn_body` 以 `include_str!` 读取 `../src/lib.rs` 与 `../src/smp.rs`。

## VERDICT 11 — CONFIRM

四条钉的名字、次序断言与目标函数全部命中；文档说“宿主不能执行 `lgdt`/`ltr`/`lidt`，源码序是这里诚实的仪器”也与文件头注释一致。

---

## CLAIM 12 — 九个文件的小计测试计数

文档主张（§5.1，第 1465–1475 行）：九个文件按 `grep -c '#\[test\]'` 依次为 16 / 13 / 12 / 0 / 1 / 3 / 0 / 1 / 7，且 riscv64 的 7 里含一条注释命中、实际 6 条。

### 证据

```
$ for f in os/arch/src/x86_64/{protection,trap_entry,trap_stub}.rs os/arch/src/arm64/{protection,trap_entry,trap_stub}.rs os/arch/src/riscv64/{protection,trap_entry,trap_stub}.rs; do printf "%s: " "$f"; grep -c '#\[test\]' "$f"; done
os/arch/src/x86_64/protection.rs: 16
os/arch/src/x86_64/trap_entry.rs: 13
os/arch/src/x86_64/trap_stub.rs: 12
os/arch/src/arm64/protection.rs: 0
os/arch/src/arm64/trap_entry.rs: 1
os/arch/src/arm64/trap_stub.rs: 3
os/arch/src/riscv64/protection.rs: 0
os/arch/src/riscv64/trap_entry.rs: 1
os/arch/src/riscv64/trap_stub.rs: 7
$ grep -n '#\[test\]' os/arch/src/riscv64/trap_stub.rs
780, 791, 805, 844, 873, 894：属性行
906:  // `#[cfg(target_arch = "riscv64")]`, so a `#[test]` written here
```

906 行是注释里的字样，不是属性；实际测试 6 条。

## VERDICT 12 — CONFIRM

九个数字逐个复现；“7 含一条注释命中”这一说明把表面矛盾解释掉了，读者不会把 7 当成可运行测试数。

---

## CLAIM 13 — 汇总计数：`os/arch/src` 376 个属性/52 文件；`os/arch/tests` 87 个属性/21 文件；`--lib --list` 266 个

文档主张（§5.1，第 1477 行）：`os/arch/src/**/*.rs` 按 `#[test]` 属性口径是 376 个/52 文件；`cargo test -p minix-arch --lib -- --list` 列出 266 个；`os/arch/tests/*.rs` 静态属性 87 个/21 文件。

### 证据

```
$ grep -rE '^[[:space:]]*#\[test\][[:space:]]*$' os/arch/src --include='*.rs' | wc -l
376
$ grep -rl '#\[test\]' os/arch/src --include='*.rs' | wc -l
52
$ cd os && cargo test -p minix-arch --lib -- --list | tail -1
266 tests, 0 benchmarks
$ grep -rE '^[[:space:]]*#\[test\][[:space:]]*$' os/arch/tests --include='*.rs' | wc -l
86
$ ls os/arch/tests/*.rs | wc -l
21
$ grep -rE '#\[test\]' os/arch/tests --include='*.rs' | wc -l
94
```

`src` 的 376 与 52、`--lib` 的 266 都命中。`tests` 目录按同一条严格口径是 86 条属性、按宽松口径是 94 条（多出的 8 条是注释或字符串里的字样，例如 `arch_gated_audit.rs:199` 的 `split("#[test]")` 与 `riscv64_return_leg_pin.rs:138` 的注释）；两种口径都得不出 87。文件数 21 正确。

### 影响

87 只出现在这一句的“静态属性”统计里，不影响任何机制结论；但它是一个可被读者复算推翻的数字，且与同一句里 376 的统计口径不自洽（同一方法在 `tests` 上应得 86）。同句的 355（含 `os/arch/tests` 的 `--list` 总数）本轮没有重跑，无法单独判定；355 − 266 = 89，与 86 条严格属性之间还差 3，可能来自参数化或展开，文档未交代。

## VERDICT 13 — REFUTE（局部）

`os/arch/tests` 的 87 与两种可复现口径（86 / 94）都不符；同句的 376、52、266 成立。建议把 87 改为 86（严格口径）并注明 355 的口径来源，或删掉 87。

---

## CLAIM 14 — 三份真机载体的调用面

文档主张（§5.2，第 1489、1491 行；§5.4 M2）：x86_64 与 riscv64 版复用生产 trait 方法；aarch64 版是独立探针，不调 `init_protection`/`CurrentProtection::init`/`CurrentTrapEntry`；x86_64 版还调 `install_trap_stubs` 与 `program_entry_base`，并在 `configure_syscall` 后用 `rdmsr` 校验四个 MSR；实例留在栈上（`main` 不返回），所以载体不覆盖 `store_protection` + `refresh_descriptors`。

### 证据

```
$ grep -n "CurrentProtection\|CurrentTrapEntry\|install_trap_stubs\|program_entry_base\|configure_syscall\|store_protection\|refresh_descriptors\|rdmsr" os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs
18:use minix_arch::{ProtectionArch, TrapEntryArch, CurrentProtection, CurrentTrapEntry, install_trap_stubs};
79:    let prot = CurrentProtection::init(0, info.kern_stack_top());
83:    prot.program_entry_base(0, info.kern_stack_top());
85:    let mut trap = CurrentTrapEntry::init();
86:    install_trap_stubs(&mut trap);
87:    trap.configure_syscall(minix_arch::syscall_entry_va());
95:    let star = unsafe { rdmsr(0xC000_0081) };
101:    let lstar = unsafe { rdmsr(0xC000_0082) };
106:    let sfmask = unsafe { rdmsr(0xC000_0084) };
111:    let efer = unsafe { rdmsr(0xC000_0080) };
```

该文件没有 `store_protection` 或 `refresh_descriptors` 的调用。

```
$ grep -n "CurrentProtection\|CurrentTrapEntry\|init_protection" os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs
（仅在模块注释中出现 init_protection，无 trait 调用）
```

aarch64 文件头自述 “NOTE: UEFI firmware sets HCR_EL2.TSP=1, which traps all SP_EL1 accesses (MRS/MSR) from EL1 to EL2”，载体自己装 `VBAR_EL1`（`msr VBAR_EL1, ...`，第 211 行）与探针向量表。

riscv64 载体第 139–149 行调用 `CurrentProtection::init(0, ...)`、`prot.load()`、`CurrentTrapEntry::init()`、`configure_syscall(minix_arch::syscall_entry_va())`、`trap.load()`。

## VERDICT 14 — CONFIRM

三份载体的调用面与文档描述一致；x86_64 版确实没有 `store_protection`/`refresh_descriptors`，因此“搬迁收尾不由此载体覆盖、由 `test-user-trap` 覆盖”的限定是必要的，文档也写了。

---

## CLAIM 15 — aarch64 的 `SP_EL1` 在 EL1 可访问（`HCR_EL2.TSP=0`）

文档主张（§4.6，第 1383 行）：`SP_EL1` 在 EL1 可访问（`HCR_EL2.TSP=0`）；真正的问题是直接 `msr SP_EL1, kernel_stack_top` 会把当前活动栈指针换掉。

### 证据

生产源码 `os/arch/src/arm64/protection.rs` 的 `init` 注释支持前半句：

```
        // EL1h model (`SPSel=1`): `SP_EL1` *is* the active stack pointer. The
        // encoding is architecturally accessible from EL1 (HCR_EL2.TSP=0);
        // the boot trap observed during bring-up came from replacing the
        // active stack pointer mid-function ...
```

但同一仓库的真机载体文件头给出相反的环境事实：

```
//! NOTE: UEFI firmware sets HCR_EL2.TSP=1, which traps all SP_EL1
//! accesses (MRS/MSR) from EL1 to EL2. This means we cannot directly
//! set or read SP_EL1 in the UEFI test environment.
```

`test-protection-aarch64/src/main.rs` 第 57 行还写明 `SPSel=1` 时来自 `HCR_EL2` 的陷入会落在 Entry 4，这正是该载体把 `SP_EL1` 探针放进 `SPSel=0` 窗口的原因。

### 影响

文档这句话没有限定范围。按架构规则（EL2 不设 `TSP` 时 EL1 可访问 `SP_EL1`）它是真的，生产源码的注释也这么写；但在本篇实际使用的 UEFI 真机环境里 `HCR_EL2.TSP=1`，`SP_EL1` 的 MRS/MSR 会被陷入 EL2。读者若把这句话当成“测试环境里也能读写 `SP_EL1`”，会与载体文件头的说明冲突。建议改成“架构上可访问（EL2 未设 `TSP` 时）；UEFI 测试环境设了 `HCR_EL2.TSP=1`，所以载体只在 `SPSel=0` 窗口内做只读探针”，并把两个来源都标出来。

## VERDICT 15 — REFUTE（口径不完整）

“`SP_EL1` 在 EL1 可访问”作为无条件的断言与本仓 `test-protection-aarch64` 文件头的环境说明冲突；作为架构规则成立，但文档没有交代测试环境的 `TSP=1`，属于把前提写成了事实。

---

## VERDICT

15 条判定汇总：

| 编号 | 主题 | 判定 |
|---|---|---|
| 01 | C 39 / Rust 41 拆解算术 | CONFIRM |
| 02 | `STUB_VECTORS` 41 与 `IDT_ENTRIES` 行号 | CONFIRM |
| 03 | C 与 Rust 的 0xF0/0xF1 相反 | CONFIRM |
| 04 | IST 单写者、无 `set_gate_ist` | CONFIRM |
| 05 | §4.4 只有一段 IST 说明 | REFUTE（1321/1322 重复段） |
| 06 | `load()` 不校验门 | CONFIRM |
| 07 | `lgdt` 先于 `ltr`、Rust busy 预检 | CONFIRM |
| 08 | LDT 槽号/限长/`lldt` 与 Rust 缺席 | CONFIRM |
| 09 | `tss_init` 五处命中、返回值丢弃、`iobase` | CONFIRM |
| 10 | GDT 长度 7 / 70 与槽位 3/4/5 | CONFIRM |
| 11 | 四条源码钉 | CONFIRM |
| 12 | 九文件测试小计 16/13/12/0/1/3/0/1/7 | CONFIRM |
| 13 | `os/arch/tests` 87 个属性 | REFUTE（严格 86、宽松 94） |
| 14 | 三份真机载体调用面 | CONFIRM |
| 15 | `SP_EL1` 可访问（`TSP=0`） | REFUTE（UEFI 环境 `TSP=1`） |

12 条 CONFIRM、3 条 REFUTE、0 条 UNCLEAR。

三条 REFUTE 里，第 05 条是编辑残留（重复段落，事实本身正确），第 13 条是一个可复算的统计数字错误，第 15 条是前提未限定范围。都不是机制层面的反证：第一轮改动的核心事实（IST 单写者、C/Rust 门数、`lgdt`/`ltr` 次序、LDT 处置、四条源码钉）在本轮构造反例全部失败，命题成立。

未覆盖项：`355`（含 `os/arch/tests` 的 `--list` 总数）未重跑；`test-protection` 的真实 QEMU 运行未执行（本轮只读静态源码，不启动构建）。
