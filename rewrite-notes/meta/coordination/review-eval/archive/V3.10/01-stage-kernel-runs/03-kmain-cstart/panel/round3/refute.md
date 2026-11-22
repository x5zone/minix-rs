# Round 3 · 反证者报告 — 03-kmain-cstart

角色：反证者。目标：构造反例推翻文档 `rewrite-notes/01-stage-kernel/03-kmain-cstart.md` 的核心主张。
方法：读文档一遍 + 针对性 grep / awk / sed 源码（`minix3/` 与 `os/`），重点打击使用强量词（唯一、只、全部、恒、相反）的句子与失败路径断言。
日期：2026-11-20。本文件只引用文档与源码，不引用作者解释与历史评审。

---

## CLAIM RD03-CLAIM-01 — 41 门构成与 C 39 门对照（§2.5、§3.7）

**主张**：Rust `STUB_VECTORS` 共 41 项 = 18 异常（0–8、10–14、16–19，跳过长模式保留的 9/15）+ 4 软中断（32–35）+ 16 PIC（0x50–0x57、0x70–0x77）+ 3 特殊向量（0xF0 调度 IPI、0xF1 LAPIC 定时器、0xFF 伪中断）；C 侧 39 = 16 PIC（`gate_table_pic` 的 hwint）+ 23 异常表（19 条 CPU 异常含向量 9、跳过 15，4 条软中断）；换算 41 = 39 − 1 + 3；C 的 APIC 向量归属 0xF0=timer / 0xF1=sched，与 Rust 相反（0xF0=sched / 0xF1=timer）。

**反证尝试**：三条路。(a) 重跑文档自己的计数命令，看是否数字造假或口径漂移；(b) 在 C 树找 0xF0/0xF1 的真实定义，检验「相反」说法是否杜撰；(c) 找反例向量——C 有而 Rust 表漏掉、或 Rust 表多出的非例外项。

**结果**：反证失败（主张成立）。

**证据**：
- `sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'` → 41；逐行列出确认：0–8、10–14、16–19（无 9、无 15）、32–35、0x50–0x57、0x70–0x77、0xF0、0xF1、0xFF。
- `awk '/gate_table_pic\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'hwint'` → 16；`awk '/gate_table_exceptions\[\]/,/};/' ... | grep -cE '_VECTOR'` → 23；`gate_table_exceptions[]` 逐行：19 条异常（0–14 含 9、16–19，无 15）+ 4 条软中断 + `{ NULL, 0, 0 }` 终止行。
- `minix3/minix/kernel/arch/i386/include/archconst.h:45`：`#define COPROC_SEG_VECTOR 9`（C 含向量 9 的实据）。
- `minix3/minix/include/arch/i386/include/interrupt.h:17-18`：`IRQ0_VECTOR 0x50`、`IRQ8_VECTOR 0x70`。
- `minix3/minix/kernel/arch/i386/apic.h:90-91`：`APIC_TIMER_INT_VECTOR 0xf0`、`APIC_SMP_SCHED_PROC_VECTOR 0xf1`——C 的 0xF0=timer/0xF1=sched 属实；Rust 侧 `os/arch/src/x86_64/trap_entry.rs` init 注释 0xF0=调度 IPI、0xF1=LAPIC 本地定时器——「相反」成立。
- 41 = 18+4+16+3 与 39 = 16+23 均现算复核；39−1+3=41 算术成立。

**若我错了**：任一表计数不符、向量 9/15 处置与文档相反、或 0xF0/0xF1 归属相同，本主张即倒。实测全部逐项吻合，无可乘之机。

---

## CLAIM RD03-CLAIM-02 — 门的 DPL 与门类型（is_trap）约定（§3.7、§4.4、§5.2）

**主张**：C 侧 `gate_table_exceptions[]` DPL=3 共 6 条（向量 3/4 两条异常 + 软中断 32–35 四条）；Rust 生产门 DPL 由 `user_accessible(vector) = {3,4,32,33,34,35}` 决定；init 元数据异常区向量 3/4 DPL=3、软中断 32–35 DPL=3、PIC 区 DPL=0；「`is_trap` 参数在 41 门上恒传 `false`（全为中断门），不是逐向量判定的」。

**反证尝试**：(a) 数 C 表 `USER_PRIVILEGE` 行；(b) 读 Rust `user_accessible` 谓词比对集合；(c) 逐行列出 init 的全部 `set_gate` 调用，检验「恒传 false」是否覆盖两个写入者（init 与 set_handler）；(d) 追 `is_trap` 是否真的编码进门类型字节。

**结果**：非阻断（P2）——DPL 部分反证失败（主张成立）；「is_trap 恒传 false」作为生产路径（`set_handler`）的陈述成立，但该表述漏掉了一个真实的门类型双来源，构成 P2：

1. `X86_64TrapEntry::init` 的门表在 8 个向量上传 `is_trap=true`（trap 门）：向量 0、1、3、4、32、33、34、35（`os/arch/src/x86_64/trap_entry.rs` init 内 `entry.set_gate(0, 0, 0, 0, true)` 等 8 行）。文档 §3.7 只声明「两个 DPL 来源（init 的表与 user_accessible）」，对 is_trap 维度的双来源只字未提；§4.4 描述 init 门清单时只点出 PIC 行的 `false`，未提异常/软中断行的 `true`。
2. 后果落在测试载体上：x86_64 `test-protection`（`os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs:75-79`）调用 `CurrentTrapEntry::init()` + `trap.load()` 而不调 `install_trap_stubs`——它加载的正是 init 元数据，因此该真机载体上活的 IDT 并非「全为中断门」（8 个 trap 门）。文档 §5.2 只说「门里的入口保持占位 0」，漏掉门类型差异。生产路径（`install_trap_stubs` → `set_handler` → `set_gate(..., false)`，`trap_stub.rs:333-341`）不受影响，post-install 确为 41 门全中断门。
严重度：P2

**证据**：
- `os/arch/src/x86_64/trap_entry.rs` `IdtEntry64::new`（94-105 行）：`let type_bits = if is_trap { GATE_TYPE_TRAP } else { GATE_TYPE_INTERRUPT };`——is_trap 真实编码进门类型，非死参数。
- init 门表逐行（同文件）：8 行 `true`（0/1/3/4/32/33/34/35）、其余 `false`。
- `os/arch/src/x86_64/trap_stub.rs:240-242`：`user_accessible = matches!(vector, 3 | 4 | 32 | 33 | 34 | 35)`——与文档完全一致。
- `awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -n 'USER_PRIVILEGE'` → 恰 6 行（breakpoint/overflow + 四条软中断）。
- `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs:75-79`：`prot.load()` / `trap.load()`，全文件无 `install_trap_stubs`。

**若我错了**：若 `set_gate` 忽略 is_trap 或 test-protection 载体另有 stub 安装步骤，则观察崩塌——已核 `IdtEntry64::new` 的编码与载体调用序列，观察成立。DPL 主张部分无任何反例。

---

## CLAIM RD03-CLAIM-03 — 初始化次序契约（§3.2、§3.3、§3.7、§4.2）

**主张**：`init_protection` 的固定次序：`CurrentProtection::init` → `store_protection` → `refresh_descriptors` → `prot.load()` → `program_entry_base` → `CurrentTrapEntry::init()` → `install_trap_stubs` → `configure_syscall` → `store_trap_entry` → `trap.load()`；两个 trait 的文档契约写明 `ProtectionArch::load()` 必须先于 `TrapEntryArch::init()`（门的 selector 字段写 `KERN_CS_SELECTOR`，门的可用性以 GDT 已装载为前提）。

**反证尝试**：通读 `init_protection` 全函数找乱序或隐藏的提前装载；找契约注释是否真的存在于 trait 源码（而非仅文档自述）；核 `set_gate` 是否真写 `KERN_CS_SELECTOR`；寻找任何绕过该次序的生产调用点。

**结果**：反证失败（主张成立）。

**证据**：
- `os/kernel/src/lib.rs` `pub fn init_protection`：函数体逐行与文档 §4.2 节选一致，`prot.load()`（`with_protection(|prot| prot.load())`）在 `CurrentTrapEntry::init()` 之前，`install_trap_stubs(&mut trap)` 在 `with_trap_entry(|trap| trap.load())` 之前。
- `os/arch/src/arch/protection.rs:99-101`：`...TrapEntryArch::init() is called, because trap entry descriptors ... reference segment selectors that must be valid in the GDT.`；`os/arch/src/arch/trap_entry.rs:109-111`：`ProtectionArch::load() must be called before TrapEntryArch::init(), ... reference segment selectors that must be valid in the GDT.`——契约注释两处都在源码里。
- `os/arch/src/x86_64/trap_entry.rs` `set_gate`：`self.idt[vector as usize] = IdtEntry64::new(handler, KERN_CS_SELECTOR, dpl, ist, is_trap)`——门 selector 恒写内核 CS，支撑「lidt 先于 lgdt 则查旧表必炸」的反事实。

**若我错了**：若任一生产调用点把 `trap.load()` 提到 `prot.load()` 或 `install_trap_stubs` 之前，或契约注释不存在，§3.2/§3.7 即倒——实测次序与注释逐字吻合，反证不成立。

---

## CLAIM RD03-CLAIM-04 — STAR/SYSRET anchor 槽位与 configure_syscall 次序（§3.1、§4.3、§4.5）

**主张**：Rust GDT 布局：槽 3 = SYSRET anchor、槽 4 = 用户 DS、槽 5 = 用户 CS、槽 6 起每 CPU 两个 TSS 槽（`GDT_SLOTS_PER_TSS=2`，表长 6 + 8×2 = 22）；`STAR[63:48]` = anchor 选择子（`GDT_USER_ANCHOR_INDEX*8 | 3` = 0x1B）、`STAR[47:32]` = `KERN_CS_SELECTOR`，sysret64 由 anchor+16/+8 推出用户 CS/SS；`configure_syscall` 内部次序 STAR → LSTAR → SFMASK → EFER.SCE；SFMASK 只置 0x200（IF）；注释记录的 #GP(0x28) 故事算术自洽（旧布局 anchor+16 命中槽 5 = TSS 描述符，选择子 5*8=0x28）。

**反证尝试**：核全部常量取值；按 AMD SDM 规则手工重算 sysret 的 CS/SS 选择子看是否落对槽；核写入次序有无颠倒（尤其 EFER 先于 LSTAR 的窗口）；核 C 侧 STAR 的 hi/lo 参数去向是否真如 §2.4 所述（hi=选择子对、lo=入口地址）。

**结果**：反证失败（主张成立）。

**证据**：
- `os/arch/src/x86_64/protection.rs:60-63`：`GDT_USER_ANCHOR_INDEX=3`、`GDT_USER_DS_INDEX=4`、`GDT_USER_CS_INDEX=5`、`GDT_TSS_FIRST_INDEX=6`；`:71` `GDT_SLOTS_PER_TSS=2`；`:85` `GDT_ENTRIES = GDT_TSS_FIRST_INDEX + MAX_CPUS * GDT_SLOTS_PER_TSS`；`:81` `USER_STAR_ANCHOR_SELECTOR = ((3*8)|3) = 0x1B`。
- 手工重算：SYSCALL CS=STAR[47:32]=0x08（内核 CS 槽 1）；sysret64 CS=0x1B+16=0x2B（槽 5 | RPL3=用户 CS）、SS=0x1B+8=0x23（槽 4 | RPL3=用户 DS）——落点正确。#GP(0x28) 故事：若用户 CS 占槽 3，anchor+16 = 5*8=0x28 = 旧布局 TSS 槽，注释算术自洽。
- `os/arch/src/x86_64/trap_entry.rs` `configure_syscall`：`wrmsr(MSR_STAR,...)` → `wrmsr(MSR_LSTAR,...)` → `wrmsr(MSR_SFMASK, SFMASK_CLEAR_IF)` → `EFER | EFER_SCE`；`:67` `SFMASK_CLEAR_IF: u64 = 0x200`。
- C 侧同构：`minix3/minix/kernel/arch/i386/klib.S:588-591`——`mov 12(%ebp), %edx`（hi）+ `mov 16(%ebp), %eax`（lo）+ `wrmsr`，即 hi 进 EDX、lo 进 EAX；`protect.c` 的 `set_star_cpu` 传 hi=选择子对、lo=入口地址——§2.4 的参数去向主张属实。

**若我错了**：STAR 高半放错槽、或 EFER 先于 LSTAR 且存在用户态窗口，都会在首次 SYSRET/SYSCALL 现场炸——常量与次序实测无误，反证不成立。

---

## CLAIM RD03-CLAIM-05 — 三架构 trap entry/return 形态差异（§4.6）

**主张**：aarch64：`ProtectionArch::init` 写 `TPIDR_EL1`（经 `set_entry_sp`，带 `base != 0` 哨兵分支）而非 `SP_EL1`，`set_kernel_stack` 是文档化 no-op，`ProtectionArch::load` 只发 `isb`，`TrapEntryArch::load` 写 `VBAR_EL1` + `isb`；riscv64：`set_entry_stack_base` 写 `sscratch`，用户腿入口第一条指令无条件 `csrrw sp, sscratch, sp`（不按 SPP 判定），stvec 在内核腿/用户腿间切换（Direct 模式），SPP 只在返回侧重锚 stvec 时读。

**反证尝试**：在 arm64 侧找任何写 `SP_EL1` 的生产路径（若存在则「不写 SP_EL1」主张倒）；在 riscv64 用户腿入口找 SPP 条件分支或先于交换的指令；找共享标量存储替代 per-hart CSR 的残留。

**结果**：反证失败（主张成立）。

**证据**：
- `os/arch/src/arm64/protection.rs` `init`：注释与函数体确认 `if kernel_stack_top.0 != 0 { set_entry_sp(kernel_stack_top); }`（写 `TPIDR_EL1`）；`set_kernel_stack` 函数体自述 "documented no-op"，仅 `let _ = stack_top;`；`load` 仅 ISB；`os/arch/src/arm64/trap_entry.rs` `load`：`asm!("msr vbar_el1, ...")` + ISB。
- `os/arch/src/riscv64/trap_stub.rs:536`：用户腿 `"    csrrw sp, sscratch, sp",  // sp ← kstack top; sscratch ← user sp`——无条件交换；`:531` 上下文注释确认该腿先翻 stvec 到内核腿。
- `os/arch/src/riscv64/trap_entry.rs:55-66`：注释确认 "the vector is SWITCHED between two asm legs instead of testing"（Direct 模式双腿切换，入口不测 SPP）；`os/arch/src/riscv64/protection.rs` `set_entry_stack_base` 只写本 hart CSR，注释 "each hart writes only its own CSR, so no cross-hart alias exists"；回归守卫在 `os/arch/tests/riscv64_percpu_trap_stack_pin.rs`。

**若我错了**：任何一条「arm64 生产代码写 SP_EL1」「riscv64 入口按 SPP 分支」「sscratch 存在共享单元」都会推翻形态差异表——三处均未找到，主张成立。

---

## CLAIM RD03-CLAIM-06 — 测试覆盖主张（§5.1、§5.2）

**主张**：x86_64 `test-protection` 复用生产 trait 方法（`CurrentProtection::init/load`、`CurrentTrapEntry::init/configure_syscall(syscall_entry_va())/load`）但不调 `install_trap_stubs`（门入口保持占位 0），读 GDTR/IDTR/TR/CS/DS 五处寄存器与 STAR/LSTAR/SFMASK/EFER 四个 MSR；aarch64 版是独立探针——不调 `init_protection`/`CurrentProtection::init`/`CurrentTrapEntry`，自装 `VBAR_EL1`；riscv64 版复用生产 trait；宿主测试计数 16/13/11/2/1/3/2/1/7。

**反证尝试**：grep 三份 main.rs 找被禁调用（`install_trap_stubs`、`CurrentProtection::init`、`CurrentTrapEntry`）；核对寄存器/MSR 读数清单是否虚构；重跑九文件 `#[test]` 计数循环。

**结果**：反证失败（主张成立）。

**证据**：
- `test-protection/src/main.rs`：74-79 行调用 `CurrentProtection::init` / `prot.load()` / `CurrentTrapEntry::init` / `trap.configure_syscall(minix_arch::syscall_entry_va())` / `trap.load()`；86-102 行 `rdmsr(0xC000_0081/82/84/80)`（STAR/LSTAR/SFMASK/EFER）；116/134/150 行 `sgdt`/`sidt`/`str`；全文件无 `install_trap_stubs`。
- `test-protection-aarch64/src/main.rs`：无 `CurrentProtection::init` / `CurrentTrapEntry` 调用；377 行 `asm!("msr vbar_el1, ...")` 自装探针表（62 行注释 "this test installs its own recoverable probe table"）。
- `test-protection-riscv64/src/main.rs`：144-149 行调用生产 trait（`CurrentProtection::init`/`prot.load()`/`CurrentTrapEntry::init`/`configure_syscall`/`trap.load()`）。
- 九文件计数循环输出：`protection.rs: 16 / trap_entry.rs: 13 / trap_stub.rs: 11`（x86_64），`2 / 1 / 3`（arm64），`2 / 1 / 7`（riscv64）——与文档 §5.1 的 16/13/11/2/1/3/2/1/7 逐位一致。

**若我错了**：任一载体多调/少调、MSR 读数虚构、或计数不符，§5.1/§5.2 即倒——实测全部对上，主张成立。（附注：CLAIM-02 的 P2 指出该载体加载的门类型差异未入文档，属遗漏而非此主张的错误。）

---

## 跨节复述与失败路径检查（任务第 4 项）

**跨节复述冲突排查**：39 门（§2.5）、41 门构成（§3.7）、41/39 对照（§4.7 表）、9+13+11 测试分布（§5.1）、`init_protection` 次序（§4.2 与 §3.2/§3.3/§3.7 各自引用）在文档内多处复述——本轮实测各处互相一致，未发现数字或次序口径漂移。GDT 槽位表（§3.1）与常量（§4.3 引用）一致。STAR 的 C 侧描述（§2.4「上半是选择子对、下半是入口地址」）与 §4.5「C 写的是同一个 STAR」经 klib.S 实证一致。

**失败路径是否被省略或写成「不会发生」**：
- 文档大量使用「把 X 删掉/提前/换序会怎样」的反事实段，失败路径覆盖反而是其强项。本轮抽查的三个失败路径断言全部实证成立：①「`intr_init(0)` 只编程 8259 不填 IDT，master 掩码 `~(1 << CASCADE_IRQ)` 在序列中段、slave 收尾 `~0`」（`i8259.c` intr_init 逐行核实）；②「head.S 的 `push $0`+`popf` 清 IF」（`head.S:71-72` 实证）；③「VM 的 `pt_init` 复制循环跳过 BIGPAGE 条目」（`vm/pagetable.c:1286` `if((entry & ARCH_VM_BIGPAGE)) continue;` 实证）。
- 唯一检查到写成「不会发生」的断言：§4.2「该窗口内没有 CPL3 用户态（configure_syscall 在 Phase B，IF=0），SYSCALL 不会到达」——与 `init_protection` 注释 "kernel runs IF=0 except at explicitly controlled sti points" 及 Phase 次序一致，反证未成功。
- 附带发现（文档未涉及，不构成对文档主张的反驳）：C 源 `arch_smp.c:89` `memcpy(&__ap_idt_tab, gdt, sizeof(idt))` 把 GDT 拷进 IDT 副本，疑似 C 树自身笔误；文档 §2.8「`prot_init_done` 的唯一读者 copy_trampoline」经全树 grep 核实成立（唯一读点 `arch_smp.c:87`，位于 68 行起的 `copy_trampoline` 内），但 earm 侧的 `prot_init_done`（earm/protect.c:22 定义、:92 置位）全树无读者，文档未提。

---

## CLAIM 11
- 结果：反证失败（对应上文 RD03-CLAIM-01，主张成立）

## CLAIM 12
- 结果：非阻断（对应上文 RD03-CLAIM-02，P2：is_trap 双来源与测试载体门类型未入文档）

## CLAIM 13
- 结果：反证失败（对应上文 RD03-CLAIM-03，主张成立）

## CLAIM 14
- 结果：反证失败（对应上文 RD03-CLAIM-04，主张成立）

## CLAIM 15
- 结果：反证失败（对应上文 RD03-CLAIM-05，主张成立）

## CLAIM 16
- 结果：反证失败（对应上文 RD03-CLAIM-06，主张成立）

- 本次反证：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5）。
