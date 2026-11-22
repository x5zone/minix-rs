# D05 R39 确认轮记录（2026-11-22）

## 范围与抽样

- 冷启动审阅者，不参与修复；抽样 `review-units.py sample <doc> --seed <HEAD>`，10 个冻结单元：
  6、5.3、4.2、3.0、3.6、4.8、6.2、2.6、2.5、4.7。
- 47/47 单元冻结、T3 PASS。

## 第一轮结果

3 条新发现（1 P1 + 1 P2 + 1 P3）：

| ID | 节 | severity | 问题 | 处置 |
|---|---|---|---|---|
| D1 | 3.0/4.7 | P1 | `TimerIrqGate` 被写成“两个方法/可关闭/含注册”；实际只有一个 `enable_timer_irq` | 三处统一为单方法；关闭不存在；注册不在 trait |
| D2 | 4.7 | P2 | “全篇唯一碰定时器硬件的函数”过强 | 改为“boot 路径的唯一编排点”，MMIO/CSR 在 `ClockArch::init_timer`/EOI |
| D3 | 6.0 | P3 | 学习目标承诺 GDB/MMIO，章内没有对应命令 | 改为 `grep`/源码读数与 QEMU 日志；前置概念同步 |

另修 doc:1652 的改写事故“滑对照时”→“最容易忽略的地方”。

修复登记：`RD-R39-001..003`；确认轮修复计数 1/2（`--confirm-fix` 3.0）。

## 增量复核

第一轮复核：D1/D2/D3 与 doc:1652 均 FIXED；同源残留 doc:1137“两个方法都是空操作” P1，§6.0 前置概念仍提 GDB P3。

第二批（同缺陷补修）：doc:1137 改“唯一的 `enable_timer_irq` 是空操作”；§6.0 前置概念改“QEMU 串口日志与源码读数”。复核 `DELTA: PASS`。

## 测试载体修复（L0.9 门内，非面板发现）

- `test-timer-irq-riscv64`：`kern_virt_base/kern_phys_base/kern_size` 改 Sv39 高半 + DRAM 4 MiB 覆盖，消除 `AlreadyMapped`；timer 载体不加载 VM ELF（stub 无 PT_LOAD），不调 `init_proc_and_boot`。
- `uart_irq::enable_uart_rx_selftest`：THR 写后轮询 LSR.DR 再清 LOOP，消除首次源 1/3 抖动；连续 4 次 PASS。
- 登记：`RD-R39-004`（P1，test/carrier）。

## 确认轮出口

- 10/10 抽样单元：5.3、4.2、3.6、4.8、6.2、2.6、2.5 PASS；6、3.0、4.7 的 3 条全部修复。
- 确认轮修复 2/2 批次（第一批 3.0、第二批同缺陷补修），未开新修改轮。
- 单元：47/47 frozen、reopened=0、stale=0、T3 PASS；`audit` 全部单元 open=0。
