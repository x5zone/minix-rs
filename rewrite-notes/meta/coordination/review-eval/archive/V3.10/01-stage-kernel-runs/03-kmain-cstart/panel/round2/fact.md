# D03 R2 P-FACT 确认轮（C + Rust 合并）

## Gate 读数

| 项 | 命令 | 读数 |
|---|---|---|
| 基线行数/哈希 | `wc -l` / `sha256sum` | 1384 行；`093de2311fb54746c3841dfbe7f17dbb97719a60cf86f1f304ca88b6c861faa7`（前缀 `093de231` 与基线一致） |
| 关联代码清单 | `bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md --check` | 21 个文件，存在性 21/21 |
| 符号锚点 | `bash tools/anchor-resolve.sh --check <doc>` | `anchors=75 resolved=75 zero-def=0 multi-def=0` |
| 9 文件 `#[test]` 精确行数 | `grep -cE '^[[:space:]]*#\[test\][[:space:]]*$'` | 16/13/11/2/1/3/2/1/6，合计 55 |
| arch crate 全量 | 同一精确行数口径 | 373 测试 / 54 文件；`os/arch/tests/*.rs` 87 / 21 |
| C 侧常量/结构 | 文档 §6.2 的 grep | `#if`=0；`void prot_init`=2；`int kernel_may_alloc`=3；`hwint`=16；`_VECTOR`=23；`USER_PRIVILEGE`=6；`tss_init` 调用点=5；`arch/` 目录=2 |

## Doc Code Sync 七项

| 项 | 结果 |
|---|---|
| Sync-1 `static mut` | `rg "static mut" <doc>` 无命中 |
| Sync-2 路径与仓库 | `doc-code-map --check` 21/21；§7.2 的 26 条 C 路径、§7.3 的 21 条 Rust 路径逐一 `[ -f ]` 全部存在；跨篇引用（01 §3.4/§3.5/§3.9/§5.2.1、02 §4.2/§4.4、05/06/07/10/13/14 篇）均存在 |
| Sync-3 虚构常量 | 文档用到的 C 宏（`IDT_SIZE`/`DESC_SIZE`/`GDT_SIZE`/`TSS_INDEX`/`X86_STACK_TOP_RESERVED`/`LDT_SELECTOR`/`TSS_SELECTOR`/`TSS_TYPE`/`IRQ0_VECTOR`/`IRQ8_VECTOR`/`I386_VM_*`/`K_STACK_SIZE`/`ARM_STACK_TOP_RESERVED`/MSR 宏等）均在 `minix3/` 命中；Rust 常量（`GDT_*`/`TSS64_SIZE`/`X86_64_STACK_TOP_RESERVED`/`STUB_VECTORS`/`SFMASK_CLEAR_IF` 等）均在 21 文件内命中 |
| Sync-4 文档内 TODO | `rg "^\s*>\s*\*\*TODO" <doc>` 无命中 |
| Sync-5 符号锚点 | `anchor-resolve --check`：`anchors=75 resolved=75 zero-def=0 multi-def=0` |
| Sync-6 workspace 前缀 | `rg "kernel/src/" <doc>` 的命中全部是 `os/kernel/src/...`；无缺 `os/` 前缀的路径 |
| Sync-7 测试数量 | 全量 373/54、`os/arch/tests` 87/21、9 文件精确合计 55 均对；§5.1 的“各 3 与 7 条”与精确值 6 不符（见 FINDING-06） |

---

## FINDING RD3B-FACT-01（P0）

- **位置**：§4.1，`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:965`（“`bootstrap_start/len` 仍为 `(0, 0)`”）。
- **严重度**：P0-fact（描述与现行代码/同族文档不符）。
- **问题**：`KernelInfo` 已没有 `bootstrap_start`/`bootstrap_len` 两个字段。现行 `os/libs/minix-boot/src/kernel_info.rs:22` 的 `KernelInfo` 无这两个字段；`os/kernel/src/lib.rs:1858-1863` 的 Phase F 注释明确写 “the two fields were removed from `KernelInfo` (D01-F03), so there is nothing to reclaim and no field to validate”；`rg -n "bootstrap_start|bootstrap_len" os/` 只命中注释/历史说明，没有任何字段定义或读写。01 篇 §3.5/§3.9 也把该字段对记为“最后停在删除”。
- **影响**：读者会以为 Phase A.2 仍保留一对恒为 `(0,0)` 的字段（甚至仍可被 `validate` 检查），与现行 `KernelInfo` 的 14 字段布局和 D01-F03 裁决相反；这是“字段已删除”之后的残留叙述。
- **证据**：
  - `rg -n "pub .*bootstrap" os/libs/minix-boot/src/kernel_info.rs` → 无命中；
  - `rg -n "bootstrap_start|bootstrap_len" os/` → 仅 `os/boot-shim/src/opensbi_helpers.rs:876`、`os/kernel/src/lib.rs:1680/1691/1860`、`os/libs/minix-boot/src/handoff_v1.rs:276` 等注释；
  - `os/kernel/src/lib.rs:1861-1863` 的 “the two fields were removed from `KernelInfo` (D01-F03)”；
  - `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1680`、`:1826` 的字段删除记载。
- **最小修复**：删除该句，或改成“`bootstrap_start/len` 已从 `KernelInfo` 删除（D01-F03；01 篇 §3.5/§3.9），Rust 侧没有对应字段/回收动作”。
- **置信度**：高。
- **是否需源码核实**：否（已核实）。
- **若我错了**：只有当现行树另有同名 `KernelInfo` 版本或 wire 字段保留；但当前结构体、`rg` 与 Phase F 注释三处一致指向“已删除”。
- **判据**：`rg -n "pub .*bootstrap" os/libs/minix-boot/src/kernel_info.rs` 无命中；`rg -n "bootstrap_start|bootstrap_len" os/` 无字段/调用；`lib.rs:1858-1863` 文字。

---

## FINDING RD3B-FACT-02（P0）

- **位置**：§3.4，`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:871`（“三个字段由 `program_gs` 在 BSP/AP 启动时写入；入口汇编只读”）。
- **严重度**：P0-fact（GS 区写者描述与代码相反）。
- **问题**：`GsArea.user_rsp` 是入口汇编的运行时写目标：`os/arch/src/x86_64/trap_stub.rs:376` 的 `mov qword ptr gs:{gs_user_rsp}, rsp`（“park user RSP”），随后 `:382` 从同一槽读回；结构体自身的注释 `:864` 就写着 “Scratch slot where the entry parks the user RSP”。`program_gs` 只是在 `:692` 把它初始化为 0。三个字段中至少 `user_rsp` 不是“入口只读”。
- **影响**：把 GS 区说成启动期写完后只读，会掩盖每次 SYSCALL 进入时对 `gs:0x8` 的写；读者会错误理解该槽的所有权/竞争窗口与 `program_gs` 的职责边界。
- **证据**：
  - `os/arch/src/x86_64/trap_stub.rs:376`（写 `gs:{gs_user_rsp}`）、`:382`（读）；
  - `:864` 的字段注释 “Scratch slot where the entry parks the user RSP”；
  - `:684-692` 的 `program_gs` 写 `kernel_stack_top`、`user_rsp = 0`、`cpu_id`。
- **最小修复**：改成“三个字段的初始值由 `program_gs` 在 BSP/AP 启动时写入；入口汇编读 `gs:0x0`（内核栈顶）与 `gs:0x10`（CPU 号），并在每次 SYSCALL 进入时把用户 RSP 存进 `gs:0x8`（该槽的运行时写者）”。
- **置信度**：高。
- **是否需源码核实**：否（已核实）。
- **若我错了**：除非“只读”仅指 `kernel_stack_top`/`cpu_id` 两个字段，但原句明确说“三个字段”。
- **判据**：`trap_stub.rs:376/382/864`；`grep -n "gs_user_rsp"` 的写/读指令。

---

## FINDING RD3B-FACT-03（P0）

- **位置**：§3.7，`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:947`（“Rust 侧相应增加”）；同段 “再加 spurious/错误向量” 的 +3 枚举；§4.7 表格 `:1198`（“APIC 路径另加 IOAPIC 门”）。
- **严重度**：P0-fact（把 C 的 64 门 APIC 表当成 Rust 也有的扩展）。
- **问题**：这段先给出 C 的 APIC/SMP 算术 `39 − 16 + 64 + 3/5 = 90/92`，随后说“Rust 侧相应增加”；但现行 Rust 没有 `gate_table_ioapic` 对应物，`X86_64TrapEntry::init`/`STUB_VECTORS` 在所有模式下始终是 41 门；IOAPIC 只把 16 条 ISA IRQ（0–15）路由到既有 `0x50–0x57`/`0x70–0x77`，pins ≥16 保持屏蔽、vector=0。Rust IDT 不会在 APIC 模式扩到 90/92 门。其次，C 的 +3 是 timer `0xF0`、spurious `0xFF`、error `0xFE` 三个向量，正文只写了 “spurious/错误向量”，枚举少了 timer（算术 3/5 本身是对的）。
- **影响**：读者会把 C 的 APIC 门数当成 Rust 的 IDT 门数，进而误读 §5.1 的“41 门/41 项”测试覆盖与 §5.4 M3 的覆盖结论；也会以为 Rust 存在一张 64 项 IOAPIC 门表。
- **证据**：
  - `sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'` → `41`；
  - `rg -n "gate_table_ioapic" os/` → 无命中；
  - `os/plat/src/x86_64/interrupt.rs:217` `IOAPIC_NUM_PINS: u8 = 24`；`:349-357` `ioapic_route_and_mask` 只把 `0..=7`/`8..=15` 映射到 `0x50+i`/`0x50+0x20+(i-8)`，其余 `_ => 0` 并带 MASK；`:490-493` `mask_all` 只遍历 `min(16)`；
  - C 侧 `minix3/minix/kernel/arch/i386/apic.c:913` 的 `int_gate_idt(APIC_TIMER_INT_VECTOR, ...)`，`apic.h:90-94` 的 `APIC_TIMER_INT_VECTOR=0xf0`、`APIC_ERROR_INT_VECTOR=0xfe`、`APIC_SPURIOUS_INT_VECTOR=0xff`、`APIC_SMP_SCHED_PROC_VECTOR=0xf1`、`APIC_SMP_CPU_HALT_VECTOR=0xf2`。
- **最小修复**：删掉“Rust 侧相应增加”，改为“Rust 不采用 64 门表：IDT 保持 41 门，IOAPIC 复用 16 条 ISA 线的既有向量，pins ≥16 屏蔽”；把“再加 spurious/错误向量”改成“再加 timer/spurious/error 三个向量（0xF0/0xFF/0xFE；SMP 另加 0xF1/0xF2）”；§4.7 的括号明确标成“（C 的 APIC 路径；Rust 无此扩表）”。
- **置信度**：高。
- **是否需源码核实**：否（已核实）。
- **若我错了**：只有当 `os/` 另有一张未在 `STUB_VECTORS`/`gate_table_ioapic` 名下出现的 64 门安装路径；`rg "gate_table_ioapic" os/` 与 `STUB_VECTORS` 计数都不支持。
- **判据**：`STUB_VECTORS` 41；`rg "gate_table_ioapic" os/` 0；`os/plat/src/x86_64/interrupt.rs:349-357/490-493` 的 16 线映射与 ≥16 屏蔽。

---

## FINDING RD3B-FACT-04（P2）

- **位置**：§3.6，`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:910`（“Phase F 的 `bsp_finish_booting` 只放行时钟门。‘software only’指的是时钟门……”）。
- **严重度**：P2（表述与代码/同节引文冲突，至少需要消歧）。
- **问题**：按字面“只放行时钟门”，Phase F 被描述成只做 gate 一步；但 `os/kernel/src/lib.rs:3722-3795` 的 `boot_init_timer()` 明确做三步：`clock_arch.init_timer(...)`（x86_64 写 PIT divisor，属硬件编程）、`register_hook(...)`（注册 handler，并按 first-handler 规则打开 IOAPIC input 2 / GICR bit / `sie.STIE`）、`enable_timer_irq()`（模块门；x86_64 为文档化 no-op）。同节自己引用的 `kmain` 注释 `:907` 也写作 “Phase F: bsp_finish_booting (timer program+register+gate) + switch_to_user”。Phase B 的 `init_clock_and_interrupts` 做的是控制器 `init` + `mask_all`，而 “software only” 只能指 Phase B 的时钟只初始化软件状态（`ClockState::new`），不是“时钟门”。
- **影响**：读者会把硬件定时器编程误放到 Phase B，或以为 Phase F 只做一次 unmask；这与 `05-clock-interrupt-init.md` 的“配置→注册→使能”时序相反。
- **证据**：
  - `os/kernel/src/lib.rs:3722-3795` 的 `boot_init_timer` 三步；
  - `:907` 的 “timer program+register+gate”；
  - `minix3/minix/kernel/clock.c:294` 的 `boot_cpu_init_timer` 位置；
  - `os/arch/src/x86_64/timer_irq_gate.rs:1-30` 的 “gate opened by `IrqManager::register_hook` / `enable_timer_irq` no-op”。
- **最小修复**：改成“Phase F 的 `bsp_finish_booting` 做时钟源的 program→register→gate 三步；它不碰 LAPIC/IOAPIC 的控制器侧（那在 Phase B 的 `intr.init()`）。‘software only’ 指 Phase B 只初始化时钟软件状态（`ClockState`），不编程硬件定时器——门在 Phase F 才开。”
- **置信度**：高。
- **是否需源码核实**：否（已核实）。
- **若我错了**：除非“只放行时钟门”是对“不碰控制器”的省略，但“只”与同节引文的 “program+register+gate” 直接冲突。
- **判据**：`lib.rs:3722-3795`；`:907`；`timer_irq_gate.rs` 注释。

---

## FINDING RD3B-FACT-05（P2）

- **位置**：§4.6，`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1168`（“所以返回之后它不能继续当作‘内核栈基址’用”）。
- **严重度**：P2（机制时序描述反了）。
- **问题**：用户腿 `csrrw sp, sscratch, sp` 之后，**在内核里** `sscratch` 暂存的是用户 sp，不能当内核栈基址；用户腿的返回交换会把内核栈顶写回 `sscratch`。需要 `prepare_user_return` 重锚的真正原因是：嵌套的内核陷阱/`park`/`resched` 路径会让 `sscratch` 保持用户 sp，所以每次返回用户前都要重写——不是“返回之后”它仍不能当基址。
- **影响**：把重锚的必要性归因到“返回之后”，会让读者误解 `sscratch` 在用户态/内核态两种上下文里的值；排查返回栈错误时会找错方向。
- **证据**：
  - `os/arch/src/riscv64/trap_stub.rs:536` 用户腿 `csrrw sp, sscratch, sp`（sp←kstack top，sscratch←user sp）；
  - `:659` 返回腿再次 `csrrw`（sp←user sp，sscratch←kstack top）；
  - `os/arch/src/riscv64/trap_return.rs:118-126` 的 `prepare_user_return` 注释 “park/resched path leaves `sscratch` holding the parked user sp, so a refresh is required”；
  - `os/arch/tests/riscv64_percpu_trap_stack_pin.rs` 的 per-hart pin。
- **最小修复**：改成“进入内核（用户腿交换）之后它暂存用户 sp，不能继续当内核栈基址；返回用户前由 `prepare_user_return` 把 `stack_top` 重新写回本 hart 的 `sscratch`——嵌套内核陷阱/resched 会留下用户 sp，故每次返回都要重锚”。
- **置信度**：高。
- **是否需源码核实**：否（已核实）。
- **若我错了**：若“返回之后”被理解为“trap 进入内核之后”的口语说法；按字面“返回”是回到用户态，此时 `sscratch` 已被返回腿写回内核栈顶，原句不成立。
- **判据**：`trap_stub.rs:536/659`；`trap_return.rs:118-126`。

---

## FINDING RD3B-FACT-06（P2）

- **位置**：§5.1，`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1227`（“两份 `trap_stub.rs` 各 3 与 7 条”）。
- **严重度**：P2（测试计数与同段去重口径矛盾）。
- **问题**：`arm64/trap_stub.rs` 的 `#[test]` 精确行数是 3；`riscv64/trap_stub.rs` 是 **6**，不是 7。`grep -c '#\[test\]'` 得到的 7 含 `riscv64/trap_stub.rs:906` 的一行注释（该行不是 `#[test]` 属性）。同段刚说“`riscv64/trap_stub.rs` 的一条注释命中已剔除……合计 55”，随后又写 7，自相矛盾。
- **影响**：读者按 §5.1 的分布无法复算出 55；会高估 riscv64 trap_stub 的测试数，影响“浅测试覆盖”的评估。
- **证据**：
  - `grep -cE '^[[:space:]]*#\[test\][[:space:]]*$' os/arch/src/riscv64/trap_stub.rs` → `6`；
  - `grep -c '#\[test\]' os/arch/src/riscv64/trap_stub.rs` → `7`；
  - `:906` 为 `// ... a #[test] written here` 注释；
  - 9 文件精确行数合计 `16+13+11+2+1+3+2+1+6 = 55`。
- **最小修复**：改为“各 3 与 6 条（`grep -c` 的 7 含一条注释命中）”。
- **置信度**：高。
- **是否需源码核实**：否（已核实）。
- **若我错了**：除非把注释也计作“测试”，但这与“测试”及同段的去重口径不符。
- **判据**：上述两条 `grep -c` 的 6/7 差异；`:906` 注释行。

---

## SUGGESTION

1. **§4.1 `:965` 的 `cut_memmap` 不是 Rust 符号**：`vm_handoff::classify` 实际调用 `FreeRangeSet::cut`（`os/kernel/src/vm_handoff.rs:324` 定义、`:443` 调用）；`cut_memmap` 在 Rust 侧只出现在注释里（`vm_handoff.rs:322` “old `cut_memmap` contract”、`lib.rs:2827`）。建议改成“用 `FreeRangeSet::cut`（C `cut_memmap` 的同一契约）扣除并记录”，避免读者 grep 一个不存在的函数。
2. **§3.7 `:947`/§4.7 `:1198` 的 APIC 门数已按 FINDING-03 处理**；另建议在 §3.7 的 “0xF0 调度 IPI、0xF1 LAPIC 本地定时器” 旁加一句：C 侧 `apic.h:90-91` 是 `0xF0=timer`、`0xF1=SMP sched`，Rust 在 `trap_stub.rs:207-214` 明确把 timer 重定位到 `0xF1`（“carrier convention”），否则读者对照 C 会看到角色互换。
3. **§5.2 `:1245` 的“前缀匹配”**：`os/qemu-tests/run_qemu.sh:191` 实际是未锚定的 `grep -q "### TEST_RESULT: PASS"`（子串匹配），不是行首/前缀匹配。建议改成“按串口日志中出现 `### TEST_RESULT: PASS` 判定”，或把脚本改成 `grep -q '^### TEST_RESULT: PASS'` 后再写“前缀”。
4. **§3.5 `:882` 的 `kinfo.freepde_start` “字段保留、消费者搬走”**：01 篇 §3.9 的实际处置是 `KernelInfo` 字段删除、C 面向的 `KinfoStruct.freepde_start` 报告位保留并固定填 0（`01-boot-shim-bootstrap.md:1680`、`:1826`）。建议明确写成“`KernelInfo` 字段删除；`KinfoStruct` 报告位保留、固定 0”，避免读者把它读成 `KernelInfo` 仍保留该字段。
5. **§4.1 `:965`/§4.7 `:1189` 与代码注释的漂移**：`os/kernel/src/lib.rs:2826-2828` 仍写 “VM module's physical memory is reclaimed via `add_memmap` (undoes the `cut_memmap` from Phase A.2)”，`lib.rs:3701/3706` 仍写 “Set kernel_may_alloc = false / kernel_may_alloc uses AtomicBool”。文档的“没有 `add_memmap` 调用”“`kernel_may_alloc` 无对应物”与代码行为一致，但这两处代码注释是过期的；建议同步删除/改写代码注释，或在文档中注明“代码内仍有过期注释，勿按它找符号”。
6. **§2.3 `:346` 的 `CONFIG_MAX_CPUS` 口径**：`minix3/minix/kernel/kernel.h:7-9` 的默认值是 1，但 SMP 构建由 `minix3/share/mk/bsd.own.mk:79` 传 `-DCONFIG_MAX_CPUS=${CONFIG_MAX_CPUS}`。建议把“在这份源码里是 1”限定为“默认（非 SMP）配置”，并补一句 SMP 构建下 `GDT_SIZE = 6 + CONFIG_MAX_CPUS` 随之增长。
7. **§4.7 `:1198` 的“长模式保留 9/15”**：建议写成“长模式把 9/15 保留（reserved），故 Rust 省略”，以免“保留”被读成“Rust 仍保留这两个向量”。
