# D04 R2 读者面板修复日志

- 目标文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 基线：2708 行，`sha256 = 773c10f1bf69de6f3604a690bc97850282b7077705ac760d8b493087d56675e3`
- 修复后：2729 行（新增 44 行、删除 23 行，净增 21 行；未整节重写），`sha256 = 5050072e3260907f2e65cd7defcbe581af1923ce3e5a60baec13c435636edb24`
- 输入：`tmp/review-eval/01-stage-kernel/04-platform-discovery/panel/round2/reader-abc.md` 的 18 条 `RD4B-READER-*`
- 范围：只改上述一篇文档；未改代码、未改其他文档
- 位置列的行号为修复后行号；范围类条目给节号

## 逐条记录

| ID | 位置 | 旧 → 新（摘要） | 源码依据与说明 |
|---|---|---|---|
| A-01 | §4.1，1555 | 「APIC 初始化与 CPU 拓扑发现，外加用户态驱动」→ 枚举里补「FACP 电源表读取（`acpi_init_poweroff`，§1.4 ⑤ 已列）」，与 §1.4 的三处内核内消费者同口径 | `minix3/minix/kernel/arch/i386/acpi.c:230` 的 `acpi_init_poweroff` 在 242 行读 `acpi_get_table_base("FACP")`；无需再猜 |
| A-02 | §4.9，2263；§4.10，2347；§5.4，2591 | 新增一段说明 `CpuTopology.bsp_id` 的单位是「第一条 enabled 记录的硬件 ID」（hw_id），不是逻辑槽位下标；指出 `unwrap_or(topo.bsp_id as usize)` 只在 hw_id 等于槽位下标时成立、解析器不保证该不变量、`CpuId::new_unchecked` 不查上界；§4.10 的 hint 一格补「其值是 hw_id」；§5.4 新增 R04-T12（有界保守回退） | `os/libs/minix-boot/src/platform.rs:270` 的字段注释写明 hw_id 语义；`os/libs/minix-platform/src/device_tree.rs:475` 与 `os/libs/minix-platform/src/acpi.rs:538` 都填第一条 enabled 记录的 hw_id；`os/kernel/src/proc.rs:511` 的 `CpuId::new_unchecked` 只包 newtype |
| A-03 | §2.7，816 | 「`kmain` 只读取这两个开关」→ 补出实际条件式 `if (config_no_apic) … else if (config_no_smp) …`，并说明 `config_no_apic` 先判、所以第一行 fallback 归因不受 `config_no_smp` 覆盖为 0 影响 | `minix3/minix/kernel/main.c:304-307` 为 `if (config_no_apic)` / `else if (config_no_smp)`；443-468 为 `cstart` 的赋值段 |
| A-04 | §4.7，2126 | 在「只有 `DeviceTree` 一臂是真的」后补：ACPI 来源返回 `None`；`os/kernel/src/smp.rs:fn smp_init` 只把 `Some(Smc)` 当 SMC，其余（含 `None`）按 `hvc` 处理并传给 `set_psci_conduit_smc`；`None` 表示「本来源没有 DTB 声明」，不是「不支持 PSCI」 | `os/kernel/src/smp.rs:1289` 为 `== Some(PsciConduit::Smc)`；`os/arch/src/arm64/smp.rs:175` 的静态默认值 `false` 即 `hvc`，模块注释写明「default conduit」 |
| A-05 | §4.1，1581 | 「确切名单就是这三个构造点」→ 改成「构造点名单是这三个」，并补「不以 `new(desc)` 形式读取、但同样必须晚于发现的还有 `init_protection` 的 `platform_desc().cpu_topology()`」 | 文档内 §4.1 下一段已断言 `init_protection` 无条件读 `platform_desc()`；`os/kernel/src/lib.rs:2249` 可见该读取 |
| A-06 | §4.11，2375 | aarch64 的 ⑥ 消费格由「`AArch64InterruptController` + `ArchInit` + `init_smp_state`」改为「`AArch64InterruptController` + `AArch64ClockArch` + `ArchInit` + `init_smp_state`」 | `os/arch/src/lib.rs:609` 把 `CurrentClockArch` 在 aarch64 上别名为 `AArch64ClockArch`；`os/arch/src/arm64/clock.rs:82` 的 `new` 期望 `ArmGenericTimerDesc` |
| A-07 | §3.5，1330；§5.3，2550 | 原建议是「把 QEMU 集成测试拆成两类并把 §5.3 改成『消费的是兜底描述符』」。源码核实推翻了「21 个空切片测试会触发兜底」这一前提：这些内核不调用 `init_from_kinfo`，发现阶段不运行。因此改为：§3.5 的 QEMU 行补明「走 boot-shim 的测试解析真实路径；`&[]` 的极简测试内核不调用 `init_from_kinfo`，兜底不会被构造」；§5.3 的判据句补「也不调用 `init_from_kinfo`」 | `rg -l 'platform_sources:\s*&\[\]' os/qemu-tests/test-kernels` = 21；与 `rg -l init_from_kinfo os/qemu-tests/test-kernels` 求交为空集；3 个调用 `platform_desc()` 的测试内核（`test-smp-aps`、`test-smp-ipi`、`test-smp-ipi-riscv64`）不在 21 名单内 |
| B-01 | §4.2，1623 | 在扫描顺序表后补一段：注释「prefer ACPI 2.0+」的意图大于实现；单循环 + `||` + `break` 由配置表项顺序决定；rev 1 项在前时会取到 rev 1 RSDP、按 §4.4 回落 RSDT、XSDT 不可达；兑现注释需要两段独立循环 | `os/boot-shim/src/uefi_helpers.rs:217` 确为单循环 `if e.guid == ACPI2_GUID || e.guid == ACPI_GUID` + `break`；aarch64 分支 232-257 才是两段独立循环 |
| B-02 | 教学目标，10；§1.3，87；§1.4，99；§4.2，1643；§4.11，2371 | 「只搬地址」→「只搬地址与描述长度」；§1.4 的例外由两处改为三处，补 `rsdp_total_size` 读 RSDP 签名/revision(+15)/length(+20)、`dtb_total_size` 读 DTB magic/totalsize；§4.2 的例外段同步改为三处并说明只读表头不碰表体；§4.11 的第 ② 段改为「只搬物理地址与表头长度，不解析表体」 | `os/libs/minix-boot/src/platform.rs:194-238`：`dtb_total_size` 读 magic + totalsize，`rsdp_total_size` 读 8 字节签名、+15 revision、+20 length；两者都在 `find_platform_sources_into` 的扫描循环里被调用 |
| B-03 | §5.3，2525 | 「12 条（1 条测 `KernelInfo` 字段、5 条测 bootstrap memmap 边界）」→ 「12 条：1 条测 `KernelInfo` 字段、5 条测 bootstrap memmap 边界、6 条测 kernel-image 保留区的对齐与模块/区间重叠断言」 | `os/boot-shim/src/uefi_helpers.rs` 的 12 个 `#[test]`：`test_build_kernel_info_fields` 1 条；`test_bootstrap_outside_memmap_*` 5 条；`test_reserve_kernel_span_rejects_unaligned_span` 1 条；`test_assert_range_clear_of_kernel_*` 4 条；`test_assert_modules_clear_of_kernel_rejects_byte_overlap` 1 条 |
| B-04 | §3.4，1277；§4.8，2186 | §3.4 的 RISC-V 时钟样本后补一句：三个 CLINT 字段当前不参与 MMIO，`read_ticks` 走 `time` CSR、`init_timer`/`stop_local_timer` 走 SBI `TIME` ecall，per-CPU `mtimecmp_addr` 只在描述符与单测里；§4.8 的 riscv64 行把「CLINT 定时器走 SBI TIME」细化为「读计数走 `time` CSR、设置/停止走 SBI，三字段无 MMIO 消费者」 | `os/arch/src/riscv64/clock.rs:145` 的 `read_ticks` 调 `read_time_csr`；`sbi_set_timer` 在 `init_timer`/`stop_local_timer`/`local_timer_eoi` 里使用；全仓 `mtime_addr`/`mtimecmp_base`/`mtimecmp_stride` 除构造与注释外无读取点，`mtimecmp_addr` 只出现在描述符与单测 |
| B-05 | §4.11，2376 | x86-64 的「真机证据」格删去 `test-protection`，保留 `test-smp-topo`、`test-timer-irq` | `test-protection/src/main.rs` 的 `KernelInfo.platform_sources` 是 `&[]`，不调用 `init_from_kinfo`，只直接调 `CurrentProtection::init`；它不覆盖 ①–④ 的发现链 |
| C-01 | §4.6，2048 | 在 `new()`/`default()` 分工段末补调用点规则：运行时兜底与测试都用 `default()`；`new()` 目前只被三个 `default()` 内部调用，riscv64 上它产生的四颗 CPU 是 `CpuInfo::zero()`（`mtimecmp_addr: None`），不能直接当运行时描述符 | 全仓检索 `QemuVirtDesc::new` 无外部调用点；`os/libs/minix-platform/src/global.rs:281` 的兜底与三个 per-arch 测试模块都用 `default()`；`os/libs/minix-platform/src/arch/riscv64.rs:149` 是 `Self::new()` 的 `Default` 内部调用 |
| C-02 | §4.6，2046 | 在 `Default` 的逐字引用后加注：它用两个局部常量而非 `d.timer.mtimecmp_base` / `d.timer.mtimecmp_stride`，两处常量若漂移会让 per-CPU 地址与 `timer` 字段不一致。代码改动不在本次范围（只改文档） | `os/libs/minix-platform/src/arch/riscv64.rs:149-158` 与 `new()` 的 `ClintDesc` 字面量对读可见重复 |
| C-03 | §5.1，2384 | 标题「三个解析器的输入从哪来」→「两个解析器与 QemuVirt 常量的输入从哪来」 | 本篇可称解析器的只有 `AcpiDesc::parse` 与 `DeviceTreeDesc::parse`；`QemuVirtDesc` 是三份常量表（§4.6） |
| C-04 | §3.8，1486 | 早期控制台一行补写入侧来源：x86-64 的 `AcpiDesc` 不解析 SPCR，`IsaSerialDesc.port_base` 是 COM1 常量 `0x3F8`；aarch64 的 `AcpiDesc` 返回 `None`，早期控制台用 `minix-plat` 的 PL011 常量 | `os/libs/minix-platform/src/acpi.rs:264-265` 构造 `Some(IsaSerialDesc { port_base: 0x3F8 })`；同文件 317-321 的 aarch64 分支返回 `None` 并注明硬编码 PL011 |
| C-05 | §3.1，1092 | 在 `KernelInfo` 注释引文后加一句：引文里的 `See … §4.6` 指向常量表本身，兜底判定实际在 §3.5 与 §4.7 | 文档结构对读；`§4.6` 内容是三份 `QemuVirtDesc` 常量表，兜底代码在 `global.rs:fn qemu_fallback_or_panic`（§3.5）与 `init_from_kinfo`（§4.7） |
| C-06 | §3.7，1426-1431；§4.1，1583 | 在 §3.7 补出 `get` 的 Panics 注释原文（`/// Panics if freeze has not run yet ("frozen before freeze").`）；§4.1 的括号改为「`get` 的文档注释原文见 §3.7」，不再留无法定位的转述 | `os/libs/minix-types/src/types/cell.rs:246` 确有该行文档注释；断言文本在同文件 250 行 |

## 未采用的原建议与原因

- **A-07**：原建议的「21 个测试会走 `QemuVirtDesc` 兜底」不成立。源码核实是 21 个测试内核全部不调用 `init_from_kinfo`，`platform_desc()` 也不会被读到；因此没有把 §3.5 的 QEMU 行拆成「触发兜底」的一类，也没有把 §5.3 改成「消费的是兜底描述符」（那会写成错误事实）。改为补明「发现阶段不运行」这一真实原因。
- **B-01 / B-02**：原建议给了「改代码」与「改文档口径」两个选项；本次硬约束只改文档，因此采用后者。B-01 记录单循环的实际顺序语义与 rev 1 后果；B-02 把表头读取列入例外并写明读哪些字段。
- **C-02**：原建议的 `Default` 改用 `d.timer.*` 属代码改动，超出范围；按建议的备选加注说明重复常量的漂移风险。

## 跳过条目

无。18 条全部有对应文档编辑；没有 `SKIP-NEED-SOURCE`。

## 校验命令与结果

```bash
$ git diff --check -- rewrite-notes/01-stage-kernel/04-platform-discovery.md
（无输出，exit 0）

$ tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/04-platform-discovery.md
anchors=77 resolved=77 zero-def=0 multi-def=0

$ AWK=gawk tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/04-platform-discovery.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/04-platform-discovery.md 内部章节引用均可解析

$ tools/doc-style-lint.sh --diff HEAD
diff 范围：2 个文件 / 56 个新增行（RANGE=HEAD，限 rewrite-notes redesign-notes study-notes；规则文件不在增量门范围）
命中汇总：| error级=0

$ tools/doc-code-density-check.sh rewrite-notes/01-stage-kernel/04-platform-discovery.md
✅ 承载门阻断项通过（4 个可判章均有码/命令）
```

最终行数与哈希：

```bash
$ wc -l rewrite-notes/01-stage-kernel/04-platform-discovery.md
2729

$ sha256sum rewrite-notes/01-stage-kernel/04-platform-discovery.md
5050072e3260907f2e65cd7defcbe581af1923ce3e5a60baec13c435636edb24
```

## 计数

- 条目总数：18（A 面板 7、B 面板 5、C 面板 6）
- 已修改：18
- 跳过：0
- 其中按源码事实调整原建议措辞：2（A-07、C-02）
