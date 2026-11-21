# D03 R39 确认轮记录（2026-11-22）

## 范围与抽样

- 冷启动审阅者，不参与修复；抽样 `review-units.py sample <doc> --seed <HEAD>`，9 个冻结单元：
  5.3、6.2、3.6、1.0、2.5、3.5、4.0、3.0、5.2。
- 44/44 单元冻结、T3 PASS；未抽样单元由 R3 full survey 与机械门覆盖。

## 第一轮结果

13 条新发现（4 P1 + 6 P2 + 3 P3）；3.6、2.5、3.0、5.2 PASS，其余单元涉及缺陷。

| ID | 节 | 严重度 | 问题 | 处置 |
|---|---|---|---|---|
| D1 | 6.2 | P1 | §6.2 第二组测试计数与实际不符（16/13/11/2/1/3/2/1/7） | 改为 16/13/12/0/1/3/0/1/7 |
| D2 | 6.2 | P1 | GDB 块缺 `target remote`；`run_qemu.sh` 不转发 `-s -S` | 改用带符号生产 ELF + `target remote :1234`，写明 UEFI PE 无符号 |
| D3 | 6.2 | P1 | `break init_protection` 在 test-protection.efi 上不可命中 | 生产 ELF 上 `break minix_kernel::init_protection` |
| D4 | 6.2 | P1 | `info registers gdtr idtr tr` 不是 GDB 可用命令 | 改 `monitor info registers` |
| D5 | 6.2 | P2 | 预期 `gdtr.base` 落在高半，与 test-protection 栈上实例矛盾 | 改读生产 ELF 的 GDT 高半基址 |
| D6 | 5.3 | P1 | 称 `init/load` 顺序“无测试”，与 `init_protection_order_pin.rs` 冲突 | 改为“无负向测试；正向顺序钉见 …” |
| D7 | 5.3 | P2 | 称 `fn init` 有门清单与 C 39 条对齐，实际只建 256 条空占位 | 改为 41 真门由 `install_trap_stubs` 装 |
| D8 | 3.5 | P2 | 01 篇 §3.9 不存在；freepde 处置描述不准 | 改 §3.4.3，改为字段删除/报告位 0/DM+软件页表走查 |
| D9 | 4.0 | P2 | `ltr` type 判定与高半丢失后果写反 | 改为“必须可用 1001；高半丢失时 ltr 能过，故障潜伏到首次 CPL3 读 sp0” |
| D10 | 1.0 | P2 | “第一次行使特权”与 §1.3 定义互斥 | 统一为 `lgdt`/`lidt`/`lldt`/`ltr` 的装载动作 |
| D11 | 3.5 | P3 | `BOOT_IDENTITY_MAP_END` 行号 217 漂移 | 改 221 |
| D12 | 3.5 | P3 | 02 篇 §4.4 指针不准；Step 1 在 §4.3 | 改 §4.3 |
| D13 | 5.3/5.4 | P3 | `R03-缺口N` 是过程编号（检查器 R03 漏网） | 去掉 R03- 前缀，改为“缺口N” |

修复登记：`RD-R39-001..013`；确认轮修复计数 1/2（第一批，`--confirm-fix` 6.2）。

## 增量复核

第一轮复核：D1/D4/D6/D7/D8/D9/D10/D11/D12/D13 `FIXED`；D2/D3/D5 主体已改，但 `-S` 下软件断点写高半不可达、`monitor info registers` 无 TR、§6.1 仍指示 GDB 读 `idtr/gdtr`。

第二批修复：改用 `hbreak minix_kernel::init_protection`；删除 TR 预期，只要求 GDT/IDT；§6.1 改 QEMU monitor。复核 `FIXED`，仅剩组标题“三个表寄存器” P3。

第三批（同缺陷措辞）：标题改“GDT/IDT 两个表寄存器”，复核 `DELTA: PASS`，无新矛盾。

## 确认轮出口

- 9/9 抽样单元：3.6、2.5、3.0、5.2 PASS；5.3、6.2、1.0、3.5、4.0 的 13 条发现全部修复。
- 确认轮修复 2/2 批次（第一批 8 单元、第二批 6.2；含 D2 的同缺陷补修），未开新修改轮。
- 单元状态：44/44 frozen、reopened=0、stale=0、T3 PASS；`audit` 全部单元 open=0。
- 机械门与真机门见 `round39-gates.md`；D03 TODO 账本匹配 0 行，无未闭环项。
