# D05 R1 certify 回读终态报告

- 结论：PASS
- 核验条数：15
- CLOSED：15；需整改项：0

回读方式：独立冷读，逐条以 `grep` / `awk` / `sed` / `git diff HEAD` 对工作树重放取证；代码侧以 `cargo check -p minix-arch` 收口（`Finished`，仅 dead_code 类 warning）。整改轨迹见下表，各条目判定与证据随后逐条列出。

## 整改轨迹

| 轮次 | 要点 |
|---|---|
| 首轮 | 15 条逐条落证：15 条闭合、3 处需整改（条目 1、9、15）；另登记 6 处观察残留（条目 2、4、5、8、10、14 的锚点差一与表述脱节） |
| 第二轮 | 条目 1 残句清零、条目 9 按更正口径复核自洽；观察残留四处清零（s14n 全量移除、`apic.c:140`、`arch/clock.rs:141`、`clock.rs:1130`、映射表 LAPIC LVT 行改写）；余条目 15 第四子项（L740）与 §4.7 use 序残留 |
| 第三轮 | L740 表格格改写为 re-export 口径（`grep '要同步'` = 0）；§4.7 引文 use 序按源码排；回归 sweep 全绿，定为终态 PASS |

## 1. §4.1 use 顺序与块注
- 判定：CLOSED
- 证据：`grep -n '跨行写法\|并为一行' 文档` = 0（残句已清零）；L1049 块注为完整一句「……开头两条 `use` 的顺序按源码原样）」；引文 L1051-1052 与 `awk 'NR>=2408&&NR<=2411' os/kernel/src/lib.rs`（`use crate::clock::ClockState;` / `use minix_arch::{ArchInit, CurrentArchInit};`）逐字一致，声明为真。

## 2. §4.7 init_timer 实参折为三行
- 判定：CLOSED
- 证据：L1689 块注含「`init_timer` 的两行实参按源码折为三行」；引文 L1696-1699 与源码 `os/kernel/src/lib.rs:3753-3759`（`clock_arch.init_timer(` / `crate::clock::DEFAULT_HZ,` / `crate::clock::current_cpuid().raw(),` / `);`）逐字一致。

## 3. §5.2 两处 `&result.kernel_info`
- 判定：CLOSED
- 证据：`grep -c '&result\.kernel_info' 文档` = 0；引文与 main.rs:179/204/210（均 `&kernel_info`）一致。

## 4. §4.7 x86 注释节选的 over-150s 处置
- 判定：CLOSED
- 证据：`grep -c 's14n' 文档` = 0——块注（L1736）为「注释为中文转述、非源码逐字，代码逐字」，记号声明全量移除；引文 L1739 写源码原文「0x50 门（源注释原文 over 150 s 全程 tick 臂 <1000 次调用）」，与 `os/kernel/src/lib.rs:3783`（`…(over 150 s`）事实一致。

## 5. §2.6 `apic.c:139` → `arch_clock.c:139`
- 判定：CLOSED
- 证据：L537 为「`arch_clock.c:139`（调用点；`apic.c:140` 是 `is_boot_apic` 宏定义）」；`grep -n 'is_boot_apic' minix3/minix/kernel/arch/i386/apic.c` → 定义在 140 行，锚与真值一致；调用点 `arch_clock.c:139`（`lapic_set_timer_one_shot(1000000 / system_hz);`）正确。

## 6. §3.6 Phase A.2 历史注记
- 判定：CLOSED
- 证据：L961 历史注记在文；`awk 'NR==1741' os/kernel/src/lib.rs` → `// Phase A.2 is intentionally empty: the kernel keeps no mutable copy of`。

## 7. §5.2 判定标准引文 `trap stub path`
- 判定：CLOSED
- 证据：L2019 判定标准引文 `PASS = uptime reaches 5 ticks with interrupts delivered through the trap stub path` 与 main.rs:12-13 逐字对应；`grep -c 'S-8 stub path' 文档` = 0；riscv64 行锚 `main.rs:360-364` 与源码相符（该区间为 uart-rx 判定打印段）。

## 8. §4.4 `init_cpu_interface_current` + 行锚 + §3.1 默认体注
- 判定：CLOSED
- 证据：§4.4 引文 L1441 与 `os/plat/src/arm64/interrupt.rs:143-152` 一致；L2019 行锚 360-364 在文；L721 默认体注指「os/arch/src/arch/clock.rs:141」，`grep -n 'fn init_local_timer' os/arch/src/arch/clock.rs` → 141 行，一致。

## 9. §1.4 位置自洽（口径更正后）
- 判定：CLOSED
- 证据：L129 与源码三方对上——lib.rs:3723-3725 头注释 `// Step 6: boot_cpu_init_timer(system_hz) / C: … called from bsp_finish_booting (main.c:73)`、调用点 3890、其后 Step 7 `fpu_init`（3892）/ Step 8 `kernel_may_alloc`（3916）/ Step 9 `switch_to_user`（3933-3940）与正文主干列举一致；载体句与 main.rs（210 `init_proc_and_boot` → 216 `boot_init_timer` → 228 `sti`）一致。原 D05 清单误并入的 D04 两条经口径更正后不再构成 D05 义务。

## 10. 其余杂项
- 判定：CLOSED
- 证据：misc.rs:1065（`tsc_per_tick` 用 DEFAULT_HZ）属实；§2.4「第四步」、§4.7 推演 1「机制同、后果不同」、§4.1 Step 2a 两条 assert_eq 省略注（lib.rs:2447-2462 实存两条）均在文；§3.2 推演锚 `clock.rs:1130` 与 `hz: DEFAULT_HZ` 实际行一致。

## 11. `disable_timer_irq` 删除
- 判定：CLOSED
- 证据：`grep -rn disable_timer_irq os/ --include=*.rs` = 0；trait（arch/timer_irq_gate.rs）与 x86_64/arm64/riscv64 三 impl 已删；`cargo check -p minix-arch` → `Finished`（仅 dead_code 类 warning）。

## 12. kernel 侧 DEFAULT_HZ 单一权威定义
- 判定：CLOSED
- 证据：`os/kernel/src/clock.rs:710` = `pub use minix_arch::arch::clock::DEFAULT_HZ;`；文件内无 `pub const DEFAULT_HZ`。

## 13. arm64/riscv64 重挂回退分支 `debug_assert!`
- 判定：CLOSED
- 证据：两文件回退分支前 `debug_assert!(false, "…TICK_INTERVAL is 0 (wiring bug)")` 在文，回退逻辑保留并注明 debug 构建在断言处当场报错——release 行为不变（debug_assert 编译期剔除）。

## 14. arch/clock.rs 注释对齐实现
- 判定：CLOSED
- 证据：方法表 L82 x86 `init_timer()` 格为「8254 PIT divisor (LAPIC LVT timer gate is handled by timer_irq_gate)」，与实现一致（x86 init_timer 只编程 PIT）；riscv64 三格 SBI/`time` CSR 口径与实现一致；`cargo check -p minix-arch` → `Finished`。

## 15. 文档同步四项
- 判定：CLOSED
- 证据：
  - §3.7：L989-996 trait 示意只剩 `enable_timer_irq`，[DEL] RD05-DESIGN-D2-04 注明删除原因，与 arch/timer_irq_gate.rs 现状一致。
  - §3.2：L752「权威定义在 arch crate，kernel 侧 re-export（……RD05-DESIGN-D2-05 已合并）」+ 引文注释 + L780 锚点均指 arch 定义。
  - §4.1：L1069-1071「之后才是 init()——源码次序是 new → 注入 → init」。
  - 第四子项：§3.8 K-3 行（L1032）锚指 `os/arch/src/arch/clock.rs:const DEFAULT_HZ` 并注明 re-export；L740 代价格为「`hz` 常量单一权威定义在 arch、kernel re-export（§3.2；RD05-DESIGN-D2-05 已消除双副本）」，`grep -c '要同步' 文档` = 0，与 §3.2 口径一致。

## 残留复核（两处，均兑现）

1. **L740 表格格**：`grep -c '要同步' 文档` = 0；L740 现文与 §3.2（L752）、§3.8（L1032）三方口径一致。
2. **§4.7 use 顺序**：引文 L1691-1693 为 `use minix_arch::{…};` / `use minix_platform::platform_desc;` / `use minix_types::Endpoint;`，与源码 `os/kernel/src/lib.rs:3750-3752` 逐行一致；块注「两条 `use` 的顺序按源码原样」与源码相符。

回归 sweep（第三遍末次执行）：s14n/跨行写法 = 0、`apic.c:140` = 1、`clock.rs:141` = 1、`clock.rs:1130` = 1、LAPIC LVT 表述 = 1、`disable_timer_irq` 双文件 = 0、`cargo check -p minix-arch` = Finished。

结论：回读验证通过，15 条逐条落证。
