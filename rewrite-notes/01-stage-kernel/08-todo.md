# 08-todo — doc-gen v3 复核发现的源码侧修复工单

> **来源与方法**：07 篇《跨空间初始化》与 06 篇《进程表与 boot 进程初始化》重写复核（四路：逐节实读 / 盲态闭卷 / 独立事实核查 / 旧稿块级对照）发现的**源码侧**问题汇总。每条都在登记时对过源码；本文件把散落在两篇文档"披露/登记"节里的条目收拢成可排期的工单。
> **口径**：`minix3/` C 源为真值；`os/` Rust 实现与其的偏差分两类——**注释失真**（改注释即闭环）与**语义/覆盖分歧**（需先裁决再动码）。行号为登记日快照，动手前以符号锚点重新定位。
> **下放给谁**：T-3~T-8 属注释回修批，一条 PR 可清；T-1/T-2 是语义分歧，**先裁决后动码**（各给三案）；T-9~T-11 是测试/缺口登记。每条验收都含"对应文档登记处同步更新"。
> **不讲什么**：`i8259.c:16-17` ICW4 注释与数值不合（05 篇 §2.3 已登记）需外部 8259A 规范裁定，本仓无依据，不排期；03 篇 LDT 占位条目动机（无源码依据，已标推断）同此。

---

## 0. 使用说明（给代码迭代 agent）

1. **一次一条**（todo-fix 纪律）：每条自带现状、证据命令、建议设计与验收；动手前先跑证据命令复核现状（源码可能已前移）。
2. **顺序建议**：T-1、T-2 先裁决（P0 语义分歧，裁决结果决定后面是否返工）；裁决后 T-3~T-8 一条 PR 批量清（互相独立、均为注释/命名）；T-9~T-11 按排期。
3. **每条做完的通用验收**：`cargo test -p minix-kernel`（或对应 crate）通过；受影响的 06/07 篇登记节同步更新；五门（anchor / style / ref-check / density / code-map）在受影响文档上复跑全绿。
4. **硬约束**（与本仓一致）：`#![no_std]`；错误映射 Minix3 errno；注释回修**不得顺手改代码语义**——语义分歧条目（T-1/T-2）必须先出裁决记录再实施。

---

## 1. 总览

| 优先级 | 条目 | 类型 | 位置 | 建议工作量 |
|---|---|---|---|---|
| **P0** | T-1 `RTS_SLOT_FREE` 清除语义：C 整值赋值 vs Rust 按位或 | 语义分歧 | `os/kernel/src/{syscall_process.rs:561-563, proc.rs:262-264, proc_table.rs:441-449}` + C `do_clear.c:57`、`proc.h:227` | 裁决 0.5d + 实施 0.5d |
| **P0** | T-2 用户模块 ELF 装载归属：三处说法打架 | 设计分歧 | `os/arch/src/arch/boot.rs:54-70,737` + C `protect.c:arch_boot_proc` | 裁决 0.5d + 注释 0.1d |
| **P1** | T-3 `lib.rs` `init_post_and_memory` 三条注释失真 | 注释回修 | `os/kernel/src/lib.rs:1994-2096` 一带 | 0.2d |
| **P1** | T-4 `dm_coverage.rs:81` "Called once" 已过时 | 注释回修 | `os/kernel/src/dm_coverage.rs:81` | 5min |
| **P1** | T-5 `direct_map.rs:137-139` riscv64 注释算术自相矛盾 | 注释回修 | `os/arch/src/arch/direct_map.rs:137-149` | 0.2d |
| **P1** | T-6 "PIT 输出是 IOAPIC input 0" 注释与 `isa_irq_to_pin` 0→2 矛盾 | 注释回修 | `os/plat/src/x86_64/interrupt.rs:26-31`、`os/plat/src/lib.rs:59-66` | 0.2d |
| **P1** | T-7 `boot.rs` 注释引用已删除的 `proc_arch.rs` | 注释回修 | `os/arch/src/arch/boot.rs:115-116` 一带 | 5min |
| **P2** | T-8 `EXT_REG_INITIALIZED` 常量无注释 | 可选 | `os/kernel/src/proc.rs:182` | 5min |
| **P2** | T-9 `NoLoadSegments`/`InvalidSegment` 错误变体无测试 | 测试缺口 | `os/boot-shim/src/loader.rs:144,211` | 0.5d |
| **P2** | T-10 `Quantum::allocate` 无生产调用点 | 设计裁决 | `os/kernel/src/proc.rs:394` | 裁决 0.2d |
| **P2** | T-11 C `p_stale_tlb` 字段 Rust 无对位 | 缺口登记 | C `arch_proto.h`（struct proc 内） | SMP 篇时处理 |

---

## 2. P0 批——语义分歧（先裁决，后动码）

### T-1 `RTS_SLOT_FREE` 清除语义：C 整值赋值 vs Rust 按位或

**现状（已核实，证据命令见下）**：

- C（`minix3/minix/kernel/system/do_clear.c:57`）：`RTS_SETFLAGS(rc, RTS_SLOT_FREE)`，而该宏（`minix3/minix/kernel/proc.h:227-231`）是**整值赋值**：`(rp)->p_rts_flags = (f)`。语义：清槽后 `p_rts_flags` **恰好等于** `RTS_SLOT_FREE`，进程先前携带的一切其他位（`NO_ENDPOINT`/`NO_PRIV`/`RECEIVING`/…）全部隐式清除。
- Rust（`os/kernel/src/syscall_process.rs:561-563`）：`proc_table.rts_set(target_nr, RtsFlagsBits::SLOT_FREE)`，而 `rts_set`（`os/kernel/src/proc_table.rs:441-449`）走 `RtsFlags::set` = **`fetch_or`（按位或）**（`os/kernel/src/proc.rs:262-264`）。语义：`SLOT_FREE` 被置上，但**进程先前持有的其他位全部留存**。
- 消费侧用**精确等式**：`is_empty()`（`os/kernel/src/proc_table.rs:180`）= `p_rts_flags == SLOT_FREE`，与 C `isemptyp` 同口径；fork 子槽检查（`proc_table.rs:1769`）同。

**分歧的后果**：任何在清槽前携带了其他 RTS 位的进程（例如经 `system.c:551` 对位路径置过 `NO_ENDPOINT` 的），Rust 清槽后槽位永远达不到"恰好 SLOT_FREE"终态，`is_empty()` 恒假，槽**不可被复用判定识别**。今天未爆，是因为 `NO_ENDPOINT` 在 Rust 侧全仓**无置位点**（定义于 `proc.rs:137/160`，无 `set` 调用）——是潜伏差异，不是现行 bug。

**证据命令**：
```
sed -n '555,575p' os/kernel/src/syscall_process.rs     # Rust 清槽：rts_set(SLOT_FREE)
sed -n '227,231p' minix3/minix/kernel/proc.h           # C 宏：整值赋值
sed -n '176,181p' os/kernel/src/proc_table.rs          # is_empty 精确判据
grep -rn 'NO_ENDPOINT' os/kernel/src/ --include='*.rs' | grep -v test   # Rust 无置位点
```

**建议设计（三案）**：

| 案 | 内容 | 代价/收益 | 判 |
|---|---|---|---|
| **案 1（推荐，对齐 C）** | 新增 `ProcessTable::rts_setflags(nr, flags)`（赋值形式，内含 C 宏同款的 `proc_is_runnable` 出队判定），`syscall_process.rs` 清槽处改调它；`RtsFlags::set_raw` 的 "initialization only" 注释同步放宽为"初始化与整值清槽" | 与 C `do_clear` 语义逐位对齐；改动面 2 处 | **采用** |
| 案 2 | 保留 `rts_set`（或），把 `is_empty`/fork 检查改位测试 | 稀释 C 的"精确终态"不变量，06 篇 §2.2.2 已登记该判据；未来新增位都要记得改判据 | 否 |
| 案 3 | 按 C `proc.c:1271` 的 "XXX: RTS_NO_ENDPOINT should be removed" 删掉整个 `NO_ENDPOINT` 位 | 独立裁决项，超出本条 | 另立条目 |

**验收**：
1. 新增回归测试：构造一个带残留位（如 `NO_PRIV`）的槽，走清槽路径后断言 `is_empty(nr) == true`。
2. `cargo test -p minix-kernel` 全量通过。
3. 06 篇 §2.2.2 的登记句改"已按案 1 修复"。

**边界**：不动 `rts_set`/`rts_unset` 的调度联动（dequeue/enqueue 判定与 C 宏一致，已在 `proc_table.rs:439-449` 对位）；不删 `NO_ENDPOINT` 位（案 3 另议）。

---

### T-2 用户模块 ELF 装载归属：三处说法打架

**现状（已核实）**：

- Rust 注释（`os/arch/src/arch/boot.rs:54-70`，`ProcKind`）：`RootService` = "ELF deferred to RS itself"、`UserService` = "ELF deferred to RS at runtime"——**装载者说是 RS**。
- C 真源：boot 期只有 VM 的 ELF 被内核装载（`minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc` 的 `p_nr == VM_PROC_NR` 分支）；其余模块的字节由 **VM** 起来后按 `kinfo.boot_procs[]` 顺序建页表装载。**装载者说是 VM**。
- Rust 的 `load_process_elf`（`os/arch/src/arch/boot.rs:737`，为非 VM 进程装载而写）**当前只有测试调用**（boot.rs:1102/1107/1131 等测试模块）。

**为什么是问题**：三处说法互相矛盾会让"第 11 个模块的 ELF 到底谁装"无法回答；且 `ProcKind` 是跨层契约（arch 层与 kernel 层都按它分工），注释指向错误的执行者会误导后续 SMP/RS 篇的设计。

**证据命令**：
```
grep -rn 'deferred to RS' os/arch/src/arch/boot.rs
grep -rn 'load_process_elf' os/ --include='*.rs' | grep -v test
sed -n '388,466p' minix3/minix/kernel/arch/i386/protect.c   # arch_boot_proc 全函数
```

**建议设计（三案）**：

| 案 | 内容 | 判 |
|---|---|---|
| **案 1（推荐，对齐 C 现状）** | 注释改为 "ELF deferred to **VM** at runtime"（与 C 一致：VM 起来后为 boot 模块建页表并装载）；`load_process_elf` 注明"为未来非 VM 直装载预留，当前生产路径由 VM 装载" | 注释与两边现状一致 |
| 案 2 | 真走 RS 装载路线（RS 从盘读 ELF 并重建地址空间）——Minix3 的 crash-revival 语义，但 boot 首装的装载者是 VM | 属架构演进 `[ARCH]`，超出注释回修，**另立条目** |
| 案 3 | kernel 直装全部模块（把 `load_process_elf` 接进生产路径） | 违背 C 的"VM 管理内存"分工，否 |

**验收**：`ProcKind` 两条注释口径与 C 一致；06 篇 §2.7 的登记句同步；`grep -rn 'deferred to RS' os/` 归零或带设计理由。

**边界**：不实现 RS 装载；不动 `EntrySpec` 的 `None` 语义。

---

## 3. P1 批——注释回修（一处 PR 批量）

### T-3 `lib.rs` `init_post_and_memory` 三条注释失真

**现状（已核实）**：三条注释与函数体/全仓现状不一致——

| Step | 注释说 | 实际 |
|---|---|---|
| Step 1 | 坐标 `(lib.rs:960-963)` | 字段写入点在 **lib.rs:1997/2000**（`init_proc_and_boot` 内）；960-963 现在是 kmain 的 rdmsr 探针 |
| Step 2 | "Record VM as the kernel-level ptproc" | **函数体在此无任何语句**；记录点在 `switch_address_space`（lib.rs:384/567 附近调用路径内），07 篇 §3.5 已核实 |
| Step 3 | "窗口在 Phase C 建立" | 窗口在 `arch_boot` **Step 4** 建立（`dm_coverage.rs:567`），Phase C 只记根地址 |

**证据命令**：
```
sed -n '1994,2096p' os/kernel/src/lib.rs                  # 函数全体与三条注释
sed -n '958,964p' os/kernel/src/lib.rs                    # 960-963 现为 rdmsr 探针
sed -n '1995,2002p' os/kernel/src/lib.rs                  # 字段写入点
grep -n 'establish_boot_dm' os/kernel/src/lib.rs          # Step 4 建立调用
```

**建议设计**：三条分别改写——Step 1 坐标改 1997/2000；Step 2 删 "Record" 标题改为"确认根已记录（记录点在 switch_address_space）"；Step 3 改"窗口已在 boot Step 4 建立（`dm_coverage.rs:establish_boot_dm`），此处只断言基址已配置"。

**验收**：`grep -n 'Phase C' os/kernel/src/lib.rs` 在该函数区间不再出现"建立"表述；三条注释与函数体逐一对照无出入；07 篇 §3.5 的两处"偏离登记"同步删除（修源后偏离消失）。

**边界**：只改注释；不动 `#[cfg(not(feature = "mock"))]` 门与断言本体。

---

### T-4 `dm_coverage.rs:81` "Called once from arch_boot_impl" 已过时

**现状**：函数文档注释写 "Called once from arch_boot_impl"，实际有**两条生产路径**（`lib.rs:384` = `arch_boot_resume_high_half`、`lib.rs:567` = `arch_boot_impl` Step 4）外加测试调用（`dm_coverage.rs:747`）。

**证据命令**：`grep -n 'establish_boot_dm' os/kernel/src/dm_coverage.rs os/kernel/src/lib.rs`

**建议设计**：注释改为 "Called from both boot paths (`arch_boot_impl` Step 4 and `arch_boot_resume_high_half`)"。

**验收**：`grep -n 'Called once' os/kernel/src/dm_coverage.rs` 归零；07 篇 §4.4 的登记句同步删除。

---

### T-5 `direct_map.rs:137-139` riscv64 注释算术自相矛盾

**现状**：注释写旧基址 `0xFFFF_FC00_0000_0000` "non-canonical … whose 39-bit payload decodes to VPN[2] = 256"——**两个说法不能同时成立**：该值的 39 位载荷是 `0x0`（VPN[2]=0），它是非规范地址没错，但不"解码到 256 号槽"。（能解出 256 的是内核镜像基址 `0xFFFF_FFC0_0000_0000` 本身。）07 篇 §4.1 已按算术改写正文并登记此矛盾。

**证据命令**：
```
python3 -c "v=0xFFFF_FC00_0000_0000; print('VPN[2]=', (v>>30)&0x1FF, ' payload=', hex(v&((1<<39)-1)))"
# → VPN[2]= 0  payload= 0x0
sed -n '137,149p' os/arch/src/arch/direct_map.rs
```

**建议设计**：注释改写为"旧值非规范、不落在任何合法 Sv39 槽；写它会让 DM 公式减出垃圾（与 x86-64 旧值撞镜像槽是不同型的错）"。同时 grep 全仓确认没有其他文档/注释复述"256"说法。

**验收**：`grep -rn 'VPN\[2\] = 256' os/ | grep -v test` 归零（或改写后的新注释保留正确算术）；07 篇 §4.1 的登记句删除。

---

### T-6 "PIT 输出是 IOAPIC input 0" 注释与 `isa_irq_to_pin` 0→2 矛盾

**现状（已核实）**：`os/plat/src/x86_64/interrupt.rs:26-31` 注释写 "the 8254 PIT output is IOAPIC input 0, so the timer hook registers under IRQ 0"；`os/plat/src/lib.rs:59-66` 同类表述。而同文件 `isa_irq_to_pin`（:294-300）把 ISA IRQ 0 映射到 **IOAPIC input 2**（input 2 → 0 是交换对）——这是 PC 虚拟线模式的行业标准接线，代码是对的，**注释把 ISA IRQ 号与 IOAPIC 输入引脚号混为一谈**。

**证据命令**：
```
sed -n '26,31p;294,300p' os/plat/src/x86_64/interrupt.rs
sed -n '57,66p' os/plat/src/lib.rs
```

**建议设计**：两处注释改为"the 8254 PIT output reaches the IOAPIC on **input 2** (virtual-wire mode swaps ISA IRQ 0 ↔ input 2; see `isa_irq_to_pin`)"。顺带 grep 全文件确认无第三处同类表述。

**验收**：`grep -rn 'input 0' os/plat/src/ | grep -iv 'irq\|swap'` 归零（注释层面的"input 0"字面清除）；07 篇 §4.9 的登记句删除。

---

### T-7 `boot.rs` 注释引用已删除的 `proc_arch.rs`

**现状**：`os/arch/src/arch/boot.rs:115-116` 一带（`load_vm_elf` 文档注释）写 "verified against `os/arch/src/{x86_64,arm64,riscv64}/proc_arch.rs` prior to refactor"——那三份文件已在重构中**删除**（`os/arch/src/arch/mod.rs:32` 有删除记录），注释引用了一个不存在的路径。07 篇 §4.1 已用省略号+声明绕开。

**证据命令**：`sed -n '113,120p' os/arch/src/arch/boot.rs`；`grep -rn 'proc_arch' os/arch/src/ | head -4`

**建议设计**：注释改为 "verified against the three per-arch implementations prior to refactor (removed; see `arch/mod.rs` note)"——不指向已删除的路径。

**验收**：`grep -rn 'proc_arch' os/arch/src/` 只剩 `arch/mod.rs` 的删除记录行；07 篇 §4.1 的绕行声明删除。

---

### T-8 `EXT_REG_INITIALIZED` 常量无注释

**现状**：`os/kernel/src/proc.rs:182` 的 `EXT_REG_INITIALIZED = 0x1000` 是 `MiscFlagsBits` 里唯一没有行尾注释的成员（其余 17 个位都有一句语义）。

**建议设计**：补一句与消费面对齐的注释（该位的置位点是 fork 继承路径 `proc.rs:1749` 一带，语义"扩展寄存器（FPU）状态已从父进程复制"）。

**验收**：`sed -n '181,183p' os/kernel/src/proc.rs` 显示注释；`cargo test -p minix-kernel --lib` 通过（纯注释不影响行为）。

---

## 4. P2 批——测试与缺口

### T-9 `NoLoadSegments`/`InvalidSegment` 错误变体无测试

**现状**：`os/boot-shim/src/loader.rs:144`（`NoLoadSegments`）与 `:211`（`InvalidSegment`）两个错误变体全仓**没有任何测试构造**——01 篇 §5.1 如实披露过；loader 的其他七个错误变体各有宿主测试。

**建议设计**：按 loader.rs 现有测试族模式（内存表 `MockLoader` + 断言错误变体）各补一条：构造零 `PT_LOAD` 的 ELF 头断言 `NoLoadSegments`；构造 `p_offset + p_filesz > p_memsz` 类非法段断言 `InvalidSegment`。

**验收**：`cargo test -p boot-shim --features test-all --lib` 两新测试通过；07/01 篇 §5.1 的"无测试条目"披露更新。

**边界**：不新增生产代码分支——两个变体已由解析器产出，只补测试。

---

### T-10 `Quantum::allocate` 无生产调用点

**现状**：`os/kernel/src/proc.rs:394` 的 `Quantum::allocate` 只有测试调用（proc.rs:2052/2059）；生产路径 `sched_proc` 内联写 `cpu_time_left`。配套的 `Quantum::consume` **有**生产调用点（`clock.rs` 的 `decrement_quantum_in_with_delta` ← lib.rs 计时路径）——两个方法调用面不对称。

**建议设计（二案）**：案 1（推荐）：`allocate` 保留并在注释写明"当前生产路径由 `sched_proc` 内联写 `cpu_time_left`，本方法是测试与未来对称化的接口"；案 2：把 `sched_proc` 的内联写改走 `allocate`（行为等价性需先证）。

**验收**：注释/接线落地；`grep -rn 'fn allocate' os/kernel/src/proc.rs` 的调用面表述与实际一致。

---

### T-11 C `p_stale_tlb` 字段 Rust 无对位

**现状**：C 的 `struct proc` 带 `p_stale_tlb[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]`（"哪些 CPU 上可能有本进程的陈旧 TLB 条目"，SMP 写路径用）；Rust `KProcess` 无对位字段，`ProcessSegments` 只有 `phys_root`/`virt_root`。

**建议设计**：**登记不移植**——Rust 侧当前无 SMP 写路径消费它；待 SMP 篇（`16-smp` 方向）设计时决定"软件页表走查 + DM 窗口"模型下是否需要等价机制。本条只在 06 篇 §4.4 的"无对位"登记处补一句指向本文件。

**验收**：06 篇登记句加"归 08-todo T-11"。

---

## 5. 跨篇已登记、不进本批的项（指向）

| 项 | 登记处 | 不进本批的原因 |
|---|---|---|
| `i8259.c:16-17` ICW4 主/从值差 bit 2、尾注同为 "not SFNM" | 05 篇 §2.3 | 需外部 8259A 规范裁定，本仓无依据 |
| 03 篇 LDT 占位条目（base 0/limit 0/PRESENT）动机 | 03 篇 §2.x（推断已标注） | 无源码依据，不排期 |
| 04 篇：`early_console()` 无活消费者、`arch_misc().acpi_tables` 无消费者、`find_platform_sources` 无单测、aarch64 DTB 分支无真机门 | 04 篇 §5.3 | 属各自专题的排期项，已有披露 |
| 06 篇 §4.2 状态表把 C 时代事实与 Rust 实现混列 | 06 篇 §4.4（已加口径） | 已闭环 |

---

## 6. 验收总则（每条通用）

1. **证据先行**：动手前先跑该条的"证据命令"确认现状；与工单描述不一致时以源码为准，并在条目回报里写明偏离。
2. **文档同步**：每条完成后同步更新 06/07 篇对应登记节（登记句改"已按本条修复"）；涉及 01 篇的（T-1 的 `is_empty` 口径、T-2 的 `EntrySpec`）同步 01 篇对应节。
3. **门禁**：受影响文档五门（anchor/style/ref-check/density/code-map）复跑全绿；代码侧 `cargo test -p <crate>` 与 `cargo clippy` 通过。
4. **不跳过 hook、不 `git add -A`**：逐路径点名提交；一条逻辑单元一笔。
5. **[T-1/T-2 特例]**：先出**裁决记录**（选项对比 + 选择理由）再实施；裁决记录进本文件对应条目下，不另开文档。
