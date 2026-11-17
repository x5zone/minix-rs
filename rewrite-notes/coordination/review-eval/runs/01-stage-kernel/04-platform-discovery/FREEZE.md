# D04 FREEZE —— 04-platform-discovery（CONVERGED）

- 冻结日期：2026-11-12
- 工作流 edition：V3.3
- 确认轮：R2（`轮型: confirmation`）
- 冻结提交：`bc232e627`
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 行数：2735
- 文档 sha256：`a6a7cba7f803a35f4f0c2da1c7e787ca9e4029ec43d055ce017179219d898375`
- 关联代码：`tools/doc-code-map.sh --check` 52/52 通过

## 收敛证据

- `eval-trace-check.py --round R2`：PASS（12 PASS / 2 WARN / 0 FAIL）。WARN 是 `D04-design-batch` 的已批准延期（D-054）。
- R2 台账：43 条（survey 40 + certify 3），处置 ① 18 / ② 6 / SUGG 19；`未闭环 = 0`；`certify 新准入 = 0`（3 条已修，同 agent grep 复核）。
- R1 台账：55 条，3 P0（`find_platform_sources`、`PlatformDescSource`、uefi 测试计数）全修；`--round R1` PASS。
- 文档门：anchor 77/77；doc-code-map 52/52；structure/ref/style 通过；citation 86 块全部跳过（`/* */`，MANUAL_FALLBACK）。
- 测试：`cargo test -p minix-platform` 18 passed；uefi_helpers 12 条；QEMU 未跑。

## 未解决事项（不阻断）

- `D04-design-batch`（D-054）：R1/R2 共 12 条代码/架构项（死接口、解析器默认值、`CpuInfo`/`ArchMiscDesc` 残留、GICD 双表示、`Frozen<T>` Sync bound、`DeviceTreeDesc` 缺 `psci_conduit`、`platform_sources` 无入口校验、`PlicDesc.context` 硬编码、x86 MMIO 全局原子、ARCH `DeviceWindow`）。
- 读者 P3 9 条与设计 P3 10 条进 suggestions。
- citation 工具对 `/* */` 的盲区留在 workflow 边界。

## 限制

- `CONVERGED` 是确认轮口径下的“无 open 项 + 无未销账债务”，不是零未知缺陷保证。
- 同 agent 复核；R2 读者项由修复代理按最小修复落地，主 agent 复核门与 diff。
- 设计代码项延期到 D04-design-batch；实施后需重新固定基线。

## 重开条件

- 出现新的 P0/P1 事实或行为缺陷证据；
- D04-design-batch 实施导致基线改变；
- 用户明确要求重开。

## 回归 review（2026-11-13）

- 回归审查判 PASS，附 REG-D04-01 P2：两处 `PlatformDescSource::new(DTB, …)` 应为 `with_optional_len`；另有 P3 `low 4 GiB` 与源码 `low 4 GB` 的措辞差。
- 两处已改为 `with_optional_len`，Safety 引文恢复 `low 4 GB` 并在块头标注旧前提；`anchor 77/77`、`doc-code-map 52/52`、`--round R2` 仍 PASS（12/2/0）。

## 回归 review（2026-11-17，恢复独立读者后重做）

用户裁决 D-055 后重做回归：

- **独立三角色**共报 17 条（READER-A 3、B 12、C 2，含 4 条 P1）；合并版当时 14 条里没有「恒等映射前提三处互斥」「§1.3 规则自我否定」「§1.1 与 §1.2 结论相反」这些结构矛盾。
- **独立冷读 certify 三轮**：7 条、6 条、2 条新准入。按 §4.0b 记 `BLOCKED` 并写 `STUCK-REPORT.md`。
- 结构性重写：§2.3 两层因果改为正面陈述「6 处 return -1、三处可达」；§1.3/§4.2 例外清单统一为（甲）（乙）；§3.1 承认 `SourceRecord` 跨 ABI；§2.10 列头改 `C i386`。
- 独立闭集冷读判 `§2.3 验证 = PASS`、`结构性重写验证 = PASS`（其余三点）。
- 新增真实缺口并登记：`R04-T13`（ACPI 表链 RSDT/XSDT/MADT 不在任何已注册恒等窗口内，QEMU 通过靠 2 MiB 片取整的布局偶然）。
- 最终文档 sha256 `a6a7cba7…`；anchor 77/77、doc-code-map 52/52、structure/ref/style 全绿。

**结论：回归通过，D04 维持 CONVERGED**，另附 `R04-T13` 为已登记的相邻缺口（D-054 延期）。
