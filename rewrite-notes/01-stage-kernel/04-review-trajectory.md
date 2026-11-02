# 04-platform-discovery Review 轨迹

> 本文是 `04-review.md` 的过程档案，按 `prompt/DOC-ITER-WORKFLOW.md` §S1/S4 与 `AI-chats/doc-gen-iterate-prompt-v2.md` §B 的要求记录：命令、输出摘要、Skill 调用、Gate 退出码、失败与返工。
> 只记增量与可重放证据；不复述报告结论。

## 0. 基线

```text
$ sha256sum rewrite-notes/01-stage-kernel/04-platform-discovery.md
2e0f39714ec83aebd0ddd79f0a644993d5fa89e97871cc1db9647d5de5507349

$ wc -l rewrite-notes/01-stage-kernel/04-platform-discovery.md
2587

$ git rev-parse HEAD
d41d76ce6949156dcca2311138c2a898c81e3108

$ git status --porcelain
 M AI-chats/daily.todo.md
```

- 受审文档与 `os/` 代码全程零改动。
- `.review/`、`.design/` 均在 `.gitignore`，本轮产物不入库。

## 1. R0 预检

### 1.1 review-init

```text
$ bash tools/review-init.sh zcode rewrite-notes/01-stage-kernel/04-platform-discovery.md zcode --design-policy strict --size-adaptive
exit=0
产物：.review/zcode/01-stage-kernel/04-platform-discovery/{scans/,SYMBOLS.md}
```

### 1.2 design/outline 4 条 ls

```text
04-outline.v1.md / 04-outline.v3.md
04-outline-review.v1.md / 04-outline-review.v3.md
04-design.v1.md
04-design-final.v*.md（无）
design-coverage-check: 04-platform-discovery PASS
```

判定：快照齐全；`04-design.v1`（2026-07-16）与现码脱节；`04-outline.v3`（2026-10-09）为最新大纲；本轮重评 v2/v4。

### 1.3 关联代码清单

```text
$ bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/04-platform-discovery.md --check --format=md
exit=0；49 个文件，存在性 49/49。
```

### 1.4 四道文档门基线

```text
$ bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/04-platform-discovery.md
anchors=69 resolved=69 zero-def=0 multi-def=0
exit=0

$ AWK=gawk bash tools/doc-ref-check.sh --check ...
✅ 内部章节引用均可解析
exit=0

$ bash tools/doc-style-lint.sh ...
命中汇总：| error级=0
exit=0

$ bash tools/doc-code-density-check.sh --check ...
✅ 承载门阻断项通过（4 个可判章均有码/命令）
exit=0
```

### 1.5 覆盖率

```text
$ python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel \
    --rust-dir os --c-dir minix3/minix/kernel --doc-file 04-platform-discovery.md \
    --semantic-map tools/coverage-extract/kernel-semantic-map.json \
    --output .review/zcode/01-stage-kernel/04-platform-discovery/SYMBOLS.md
exit=0
Coverage Summary: 84/213 (39.4%) semantic; 17/213 (8.0%) Rust; 有文档无Rust 67
```

## 2. R1 正确性

### 2.1 C 引用逐块

- `acpi_phys2vir`、`get_acpi_rsdp`、`acpi_rsdp_test`、`struct acpi_rsdp`、`acpi_init`、`acpi_madt_get_typed_item`、`acpi_get_lapic_next`、`discover_cpus`、`struct acpi_madt_hdr`、`LOCAL_APIC_DEF_ADDR`、`lapic_enable_in_msr`、`acpi_get_ioapics`、`struct machine`、`usermapped_data.c`、`cpu_identify`、`bsp_init`、`board.h` 位域、`omap_intr.c:intr_init` 全部逐行/节选对过，引用本身一致。
- 两处结论错：
  - §2.5 "C 的活路径里 `lapic_addr` 的每一处赋值都是常量"：`memory.c:940-958` 的 `arch_enable_paging` 在 `USE_APIC` 下执行 `lapic_addr = lapic_addr_vaddr`（VA 回写）。物理基址结论不变，句子需限定。
  - §2.7 "`kmain` 会读启动参数 `no_apic`"：实际在 `main.c:cstart` 的 `#ifdef USE_APIC` 块（L442-453），`kmain` 只在 L304 读开关。
- 计数复算：`acpi.c` 410 行；`ACPI_MADT_TYPE` 11 条；`sizeof(struct acpi_rsdt)=176`；`sdt_count` 的 `(-1-36)/4=1073741814`（i386 `size_t` 32 位）成立。

### 2.2 Rust 引用逐块

- 逐字块：`Frozen`、`qemu_fallback_or_panic`、`prepare_boot`、`getinfo_machine`、`MachineStruct`、`QemuVirtDesc`、`from_fdt` 一致。
- 过期/增字：
  - §3.1 `KernelInfo` 节选：`kernel_info.rs:112-134` 现注释含 "# DTB + RSDP coexistence" 段，文档节选为旧注释。
  - §3.2 `PlatformDescEnum`：文档变体注释 "x86-64, and aarch64 since S-2b"；代码是 "x86-64 and aarch64"。
  - §4.3 `parse_by_kind`：文档在源码注释前加 `// S-2b:`；源码 `kind.rs:61` 没有。
  - §4.9 `init_smp_state`/`init_protection`：与现码不一致（见 P0-fact-staleSmpQuotes）。
- 代码行为核对：
  - `interrupt.rs:235-243`：`lapic_read/write` `let _ = base;`，用 `LAPIC_VA`；`ioapic_*` 用 `IOAPIC_VA`。
  - `lib.rs:2190-2215`：设备窗口固定 0xFEE0_0000/0xFEC0_0000。
  - `drivers/power/acpi/src/main.rs:40-50`、`service.rs:91-93,134-136`：用户态 ACPI 驱动仍读 `apic_enabled`/`acpi_rsdp`。
  - `interrupt.rs:317-321,359-376,483`：`mask_all` 0..64 → 引脚 0..63，`IOAPIC_NUM_PINS=24`。
  - `acpi.rs:505-520,545-560`：`bsp_id` = 第一条 enabled 记录；C `arch_smp.c:302` 用 `apicid2cpuid[apicid()]`。

### 2.3 测试对账

```text
$ grep -rc '#\[test\]' os/libs/minix-platform --include=*.rs | awk -F: '{s+=$2} END {print s}'
22
$ cargo test -p minix-platform
test result: ok. 14 passed; 0 failed; 0 ignored
$ cargo test -p minix-boot
test result: ok. 65 passed; 0 failed; 0 ignored
$ grep -c '#\[test\]' os/boot-shim/src/uefi_helpers.rs
4
```

- 22 是属性计数；x86_64 宿主实际运行 14（8 条 aarch64/riscv64 专用测试被 cfg 排除）。
- §5.3 "uefi_helpers.rs 只有 1 条 `#[test]`" 错误：实际 4 条。
- `test-smp-topo` 的 BSP 断言：解析器把 `bsp_id` 设为第一条 enabled 记录，断言结构恒真。

### 2.4 本轮首次定级的 P0

1. `P0-fact-lapicConsumed`（主会话读 `interrupt.rs` 的 `let _ = base;`）
2. `P0-design-machineFields`（`rg apic_enabled` 命中用户态 ACPI 驱动）
3. `P0-code-acpiValidation`（C `acpi_read_sdt_at` vs Rust `find_madt`）
4. `P0-design-bspId`（C `smp_init` vs Rust `parse_madt`）
5. `P0-fact-staleSmpQuotes`（§4.9 逐块 diff）

## 3. R2 卓越性与对抗 agent

### 3.1 A2 闭卷学生

- 输入：只读 `04-platform-discovery.md`；不读源码/规则/其他文档；500 行报告。
- 结果：12 问 11 可答、1 需推断（riscv64 不翻译理由）、0 不能答；节级 17 ① / 1 ② / 0 ③；子项 ② 12 / ③ 13。
- 独立命中：§4.1 与 §4.9 的首 panic 点冲突；`Frozen` 断言文本；`PlatformSource`/`ArchMiscDesc` 晚定义；riscv 不翻译理由缺。
- 报告：`tmp/review04/A2-student.md`。

### 3.2 A3 冷读结构编辑

- 输入：只读文档全文 2,588 行；不读源码/规则/diff/自评。
- 结果：骨架成立、弧线中段断开；§1 提前发结论、§2.1 自问自答、§3 设计章不完整、§4.10 偏离启动序、§5 两套覆盖视角；根因"按语言切章 vs 按机制论证"。
- 独立命中：LAPIC 主线缺"消费"与"消费测试"两环；`PlatformSource`/`ArchMiscDesc` 无定义；§4.10 是 §2.8 的续篇。
- 报告：`tmp/review04/A3-structure.md`。

### 3.3 A1 代码事实核查（已返回）

- 输入：文档 + 明确文件清单，禁止读 `.review`/`.design`/`tmp/review04`/`04-review*`；664 行报告。
- 结果：30 条（1 P0 / 10 P1 / 19 P2）。P0 是 `DEVICE_TREE_GUID` 非 UEFI 规范值（`b1b621d2-…` vs `b1b621d5-f19c-41a5-830b-d9152c69aae0`），主会话复核后升级为 `P0-code-dtbGuid`。
- 新发现并入：§4.8 三个零基址断言/漏 `gicr_stride`、§4.8 x86/riscv 地址翻译表过期、§3.6 `TODO-02-3` 伪造引文、§5.2 riscv 载体 a0 未使用、§4.5 ACLINT mtimecmp、§4.4 GICC UID 假设、`acpi.rs` aarch64 测试编译、§4.8 riscv `new` 漏 `pliva_base` 等。
- 报告：`tmp/review04/A1-codereview.md`。

### 3.4 主会话 R2 补充

- 结构：`structure.md` 12 节全填；Gate D-6 PASS（带 P1/P2）。
- 代码卓越：死字段/死函数清单（`ioapic_version`、`PitDesc.lapic_base`、`X86_64ClockArch.lapic_base`、`ApicDesc.lapic_base`/`ioapic_base`）；非法态封堵在 x86 上为空。
- 文风：`doc-style-lint` error=0，但 `S-2b`/`S-6.1 (D-40)`/`I-22`/`NK4-C 1.114` 未被 SL-11 覆盖；`doc-code-density` 4/4 通过。

## 4. R3 综合与验证

### 4.1 Issue List

- 43 条：P0=6、P1=18、P2=19（A1 的 30 条去重并入）。
- 与主报告 §9 同一清单。

### 4.2 Gate G 机械化检查

```text
$ python3 tools/verify-check.py --scan .../scan.md --state .../STATE.md --symbols .../SYMBOLS.md \
    --sample-ratio 0.2 --seed 20261102 --output .../VERIFY-CHECK.md
✅ VERIFY-CHECK.md 已生成
exit=0

$ python3 tools/verify-check.py --scan .../scan.md --self-check
✅ 9 个锚段齐全
Gate A/B/C/D/D-6/E/G/H 全部 ✅
总 issue 数: 43；抽样 8
✅ VERIFY-SELF PASS
exit=0
```

### 4.3 人工反向验证

- 抽样 8 条：`P2-fact-sswiPsci`、`P0-fact-staleSmpQuotes`、`P2-fact-sliceDistribution`、`P1-fact-uefiTestCount`、`P1-fact-noApicFn`、`P2-code-sectionRef`、`P0-code-acpiValidation`、`P1-fact-kernelInfoQuote`。
- 8/8 CONSISTENT；每条重放源码/grep。
- 同 agent 局限已标注；不宣称异构独立。

### 4.4 QEMU 三架构

```text
$ tmp/review04/run-topo.sh
x86_64:  nr_cpus=0x4, bsp=0x0 → ### TEST_RESULT: PASS test-smp-topo ###
aarch64: nr_cpus=0x4, bsp=0x0 → ### TEST_RESULT: PASS test-smp-topo-aarch64 ###
riscv64: nr_cpus=0x4, bsp=0x0 → ### TEST_RESULT: PASS test-smp-topo-riscv64 ###
```

日志：`tmp/review04/qemu-topo.log`。

### 4.5 返工记录

| # | 失败 | 根因 | 修复 | 复验 |
|---|---|---|---|---|
| 1 | Gate G 机械检查最初无法生成 | `scan.md` 未落盘 | 先写 scan.md 再跑 `verify-check.py` | exit=0 |
| 2 | SYMBOLS `--check-filled` 失败 | 84 处 `_待AI补充_` | 按语义类别批量填充 + 抽检 | exit=0 |
| 3 | QEMU 构建首次包含大量 warning | 仓库基线 warning（60 条） | 记录为基线债，不归因 04 | build rc=0 |
| 4 | `run_all.sh` 行号核对 | 文档行号与当前脚本漂移 | 实跑 `rg -n` 记录 96/102/108/131/211/246 | P1-fact-runAllLines |
| 5 | A1 长尾 | 输入切片过大 | A1 最终返回 30 条；其 P0/P1 已并入 | 1 P0 + 10 P1 复核 |

## 5. R4/R5 产物

```text
rewrite-notes/01-stage-kernel/04-review.md
rewrite-notes/01-stage-kernel/04-review-trajectory.md
.review/zcode/01-stage-kernel/04-platform-discovery/scan.md
.review/zcode/01-stage-kernel/04-platform-discovery/SYMBOLS.md
.review/zcode/01-stage-kernel/04-platform-discovery/structure.md
.review/zcode/01-stage-kernel/04-platform-discovery/VERIFY-CHECK.md
rewrite-notes/01-stage-kernel/.design/04-design.v2.md
rewrite-notes/01-stage-kernel/.design/04-outline.v4.md
rewrite-notes/01-stage-kernel/.design/04-outline-review.v4.md
tmp/review04/A1-codereview.md
tmp/review04/A2-student.md
tmp/review04/A3-structure.md
tmp/review04/qemu-topo.log
```

## 6. 复盘（本轮 review 自身的失败与改进）

1. **A1 长尾**：代码事实核查 agent 在报告落盘前返回（30 条）；其 P0 与主会话独立重合。改进：下一轮把 A1 输入切成"每个文件一个切片"，缩短长尾。
2. **QEMU 范围**：只跑 `test-smp-topo` 三架构；`test-timer-irq`/`test-smp-aps` 未跑。改进：回归轮补完整套件。
3. **覆盖率分母**：39.4% 是语义域并集；本篇射程无静默漏写。改进：给 semantic-map 加"本篇射程"过滤。
4. **快照过期**：`04-design.v1` 与现码脱节；本轮 v2/v4 重评。改进：每轮 review 重评快照。
5. **同模型局限**：A2/A3 与主会话同模型；用材料隔离 + 重放补偿，不宣称异构。
6. **报告长度**：43 条 issue 全量展开；用户要求深度全面，保留。改进：按章拆 scan，主报告只留结论与证据索引。
7. **零发现探针**：抽 3 条断言重跑，2 条发现问题（uefi 测试数、run_all 行号），1 条零发现（`from_fdt` 引用）；探针有效。

## 7. 与 03 的关系

- 03 已于 `25d8f23ac` 完成并提交；本轮 `/goal` 标题写 03，但实际执行 04 计划，未重复 03。
- 03 的 `program_entry_base` 修复导致 04 §4.9 引用过期（P0-fact-staleSmpQuotes）；这是跨篇同步缺口，不是 04 原始事实错误。
- 03 的 `P0-code-apTssSp0` 与 04 的 `P0-design-bspId` 同属"per-CPU 身份"族；建议后续修复时一起设计 x86 BSP 身份解析。

## 8. 修复轮与 R2 回归（2026-11-03）

### 8.1 修复批次

- **P0 代码侧**：`DEVICE_TREE_GUID` 改规范值 + legacy；LAPIC/IOAPIC 描述符与 MSR/设备窗口 check-and-reject；`GET_MACHINE` 填 `apic_enabled`/`acpi_rsdp`；ACPI RSDP/根表/MADT 校验和与容量闸；x86 BSP 用 CPUID APIC ID、aarch64/riscv election。
- **P0 文档侧**：§2.5/§4.8/§6.1 改为 check-and-reject；§4.9 按现码重写；§4.10 machine 字段改为保留发布；§4.4 补校验策略；§5.2/§5.4 改 BSP 契约。
- **P1/P2**：18 条 P1、19 条 P2 全部处置（清单见 `VERIFY-CHECK-R2.md`）；新增 §3.9 设计索引、§4.11 六段表、`PlatformSource`/`ArchMiscDesc` 定义。
- **测试**：新增 ACPI 4 条负向测试、`machine_record` 宿主测试；三个 topo 载体加执行 CPU 身份断言；aarch64 测试 cfg。

### 8.2 回归命令与读数

```text
anchor-resolve: anchors=73 resolved=73 zero-def=0
doc-ref-check: exit=0
doc-style-lint: error=0
doc-code-density: 4/4
cargo test -p minix-platform: 18 passed
cargo test -p minix-boot: 65 passed
cargo test -p minix-kernel --features mock: 857 passed / 8 ignored
QEMU test-smp-topo x86_64/aarch64/riscv64: PASS (nr_cpus=4, bsp=0)
QEMU test-timer-irq x86_64: PASS
QEMU test-protection x86_64: PASS
QEMU test-smp-ipi-riscv64 (aclint=on): PASS
QEMU test-smp-aps-aarch64 (-dtb): PASS
QEMU test-smp-aps x86_64: FAIL (baseline; reproduced with 04 code stashed)
```

### 8.3 失败与返工

| # | 失败 | 根因 | 处置 |
|---|---|---|---|
| 1 | ACPI 负向测试首次 SIGSEGV | RSDT 32 位指针截断（栈地址 >4 GiB） | 改用 rev2 XSDT 64 位指针 |
| 2 | aarch64 test-smp-topo 编译失败 | `dump_acpi_parse_error` 未覆盖新增错误变体 | 加 `Err(_)` 兜底 |
| 3 | `test-smp-aps` x86 回归失败 | `protection.rs:451` stamp 断言；stash 04 代码后同样失败 | 记录为基线失败，不归因 04 |
| 4 | `PitDesc.lapic_base` 删除后多处测试引用 | 字段删除的连带修改 | 删除测试断言/构造字段；`cargo test` 全绿 |

### 8.4 复盘

- 修复轮 43/43 关闭；P0 的"设计限制"类（LAPIC 重定位、多 IOAPIC）以显式 TODO 关闭，不假装已支持。
- 基线 `test-smp-aps` 失败是当前 HEAD 的既有问题（stash 对照证实）；04 的修复没有引入新的 P0/P1。
- 同 agent Gate G 8/8；A1 的 GUID P0 与主会话独立重合，仍不宣称异构。
