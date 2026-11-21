# D04 R39 确认轮记录（2026-11-22）

## 范围与抽样

- 冷启动审阅者，不参与修复；抽样 `review-units.py sample <doc> --seed <HEAD>`，11 个冻结单元：
  4.0、4.1、4.11、3.8、5.1、3.9、1.5、5、5.2、4.5、2.0。
- 54/54 单元冻结、T3 PASS；未抽样单元由 R3 full survey 与机械门覆盖。

## 第一轮结果

15 条新发现（6 P1 + 6 P2 + 3 P3）：

| ID | 节 | severity | 问题 | 处置 |
|---|---|---|---|---|
| D1 | 4.0 | P1 | aarch64 小结写成“先 ACPI 后 DTB”，与源码相反 | 改为先 DTB 后 ACPI；AAVMF 无 DTB 命中 ACPI |
| D2 | 4.0 | P2 | 失败对照把 C 的 `sdt_count` 搬进 Rust，并说会建越界窗口 | 改为 `table_chain_windows`/`find_madt` 的限幅前提 |
| D3 | 4.1 | P2 | 早期日志说“描述符有此项但不读” | 改为描述符无此项，minix-plat 自持常量 |
| D4 | 4.11 | P3 | ⑥ 消费列只有 aarch64 列 `ArchInit` | 三列补 `ArchInit` |
| D5 | 5.1 | P1 | 单测总数 26/acpi 9/宿主 18 与源码不符 | 27/acpi 10/宿主 19，补 `table_chain_windows_reports_root_and_madt` |
| D6 | 1.5 | P1 | 早期控制台描述符方法不存在 | 改为“无此项，描述符链已移除” |
| D7 | 5 | P1 | `platform_sources &[]` 总数 54/分布漂移 | 55，分布 11/9/5/3/2/19 等 |
| D8 | 5 | P2 | M7 把 DTB 构造归到 `bootface_a64` | 改为 `bootface_a64 → run_self_boot → bootface_common` |
| D9 | 5 | P3 | 去编号后的标点残迹 | 已清 |
| D10 | 5.2 | P2 | aarch64 测试说用 `new(RSDP)` | 改为 boot-shim 扫描 `with_optional_len` |
| D11 | 4.5 | P1 | Safety 段把改写文本冒充源码引文 | 改逐字引 low 4 GB 原文 + 过时批注 |
| D12 | 2.0 | P1 | C 链写成 ACPI+DTB、XSDT、含 riscv64 | 限定 i386/earm：RSDT、无 DTB、无 riscv64 |
| D13 | 2.0 | P2 | 学习目标/示例/失败对照同样越界 | 已按 i386/earm 重写；消费者限定 apic/discover_cpus |
| D14 | 2.0 | P2 | `arch_clock`/`apic` 读 ACPI 的描述不准 | 改为仅 apic/discover_cpus；i386/earm 时钟差异注明 |
| D15 | 2.0 | P3 | `acpi_init` 说成 cstart 四次调用之一 | 改为 `arch_init()` 内一步 |

修复登记：`RD-R39-001..015`；确认轮修复计数 1/2（第一批，`--confirm-fix` 4.0）。

## 增量复核

第一轮复核：D1 小结、D3、D4、D5 主体、D7、D8/D9、D10、D11、D14/D15 等修好；剩余 `table_chain_windows` 因果、`这 26 条`、§6.2 #3 26、#4 分布、XSDT 术语、earm 括号。

第二批（同缺陷补修）：改精确限幅句、26→27、§6.2 #3/#4 分布、术语 RSDT、earm 括号。复核 `DELTA: PASS`，无新数值/事实矛盾。

## 确认轮出口

- 11/11 抽样单元：3.8、3.9 PASS；4.0、4.1、4.11、5、5.1、5.2、4.5、2.0 的 15 条全部修复。
- 确认轮修复 2/2 批次（第一批 11 单元、第二批同缺陷补修），未开新修改轮。
- 单元：54/54 frozen、reopened=0、stale=0、T3 PASS；`audit` 全部单元 open=0。
- 机械门与真机门见 `round39-gates.md`；D04 TODO 匹配 0 行。
