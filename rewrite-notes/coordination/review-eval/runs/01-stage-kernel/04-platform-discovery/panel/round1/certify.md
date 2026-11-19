# D04 R1 certify（闭集）

- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 工作树基线：2708 行，sha256 `3b2c38bd6353a51e3c3a91b95e56e3d2e72cf8c372705a985f7608f9dae77e98`
- 日期：2026-11-12
- 范围：仅核对下列 5 项与 `reader-fix-log.md` 抽查；不重扫全文，未读历史评审、其他面板与 expected。
- 结论：5 项全部 CLOSED；新准入 2 条 P2（引用保真，不影响 5 项的功能结论）。

## 工具门（L1）

```bash
$ tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/04-platform-discovery.md
...
anchors=72 resolved=72 zero-def=0 multi-def=0
# exit 0
```

```bash
$ tools/doc-code-map.sh rewrite-notes/01-stage-kernel/04-platform-discovery.md --check
...
（共 52 个文件；存在性 52/52 通过）
# exit 0
```

```bash
$ python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/04-platform-discovery.md
✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析
# exit 0
```

```bash
$ AWK=gawk tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/04-platform-discovery.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/04-platform-discovery.md 内部章节引用均可解析
# exit 0
```

## 逐条核对

| # | 核对项 | 结论 |
|---|---|---|
| 1 | §3.1/§3.6/§4.2 的 `PlatformDescSource` 三字段、`BootStorage` 槽位、`with_optional_len`；无 `Box::leak`/`Vec` 旧实现 | CLOSED（附 RD4-CERT-01 P2，见下） |
| 2 | `find_platform_sources_into` 写入 `out` 槽位并返回条数；riscv 用 `push_platform_source` | CLOSED（附 RD4-CERT-02 P2，见下） |
| 3 | `uefi_helpers` 12 测试；`&[]` 计数 54；`vm_handoff` 3 处 | CLOSED |
| 4 | 读者 C-01（ncpus 恒 0）、C-02（54）、C-06（BSP 回退不一致已记录）、A-04/A-05（节选补行）、B-04/B-05（head -5 / 21 文件清单） | CLOSED |
| 5 | `reader-fix-log.md` 24 条编辑自洽性，抽查 6 条 | CLOSED |

### 1. PlatformDescSource / BootStorage / with_optional_len

- `os/libs/minix-boot/src/platform.rs:123-130`：`PlatformDescSource` 三字段为 `kind: PlatformDescKind`、`len: u32`、`phys_addr: PhysBytes`，与 §3.1、§3.6 的表述一致。
- `with_optional_len` 在 `platform.rs:155-164`，`None` 回落 `len = 0`；§4.2 两处（x86 的 `with_optional_len(RSDP, ...)`、riscv 的 `with_optional_len(DTB, ...)`）与源码一致。
- `BootStorage` 的平台源槽位：`os/libs/minix-boot/src/boot_storage.rs:71`（`platform_sources: [PlatformDescSource; WIRE_PLATFORM_CAP]`）、`:240`（`push_platform_source`）、`:224`（`platform_slots_mut`）；调用点 `os/boot-shim/src/uefi_helpers.rs:1035` 传 `storage.platform_slots_mut()`。
- 旧实现：正文只有 §4.2 的“不再 `Box::leak`”这一句（`1609` 行），没有把 `Box::leak`/`Vec` 当作现行平台源交接方式描述；现行路径是“写入调用者槽位并返回条数”。（源码 `uefi_helpers.rs:210` 内部仍用临时 `Vec` 收集后 `copy_from_slice` 到 `out`，但交接面已不是旧的 `Box::leak` 泄漏切片；文档这一句只否认泄漏，不否认临时分配，口径不矛盾。）

### 2. find_platform_sources_into 与 riscv 路径

- `os/boot-shim/src/uefi_helpers.rs:208` 签名：`pub fn find_platform_sources_into(out: &mut [PlatformDescSource]) -> usize`；函数尾部 `269-288` 行执行 `sources.into_inner()`、容量断言、`out[..sources.len()].copy_from_slice(&sources)`，最后 `sources.len()` 返回条数。§4.2 的槽位/条数描述与之一致。
- riscv：`os/boot-shim/src/opensbi_helpers.rs:417-422` 用 `storage.push_platform_source(PlatformDescSource::with_optional_len(DTB, PhysBytes(pa), len))`，与 §4.2 一致。

### 3. 12 测试 / 54 / 3

```bash
$ grep -c '#\[test\]' os/boot-shim/src/uefi_helpers.rs
12
$ rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | wc -l
54
$ rg -l 'platform_sources:\s*&\[\]' os/qemu-tests/test-kernels | wc -l
21
$ rg -o 'platform_sources:\s*&\[\]' os/kernel/src/vm_handoff.rs | wc -l
3
```

`uefi_helpers.rs` 的 12 条测试名为 `test_build_kernel_info_fields`（1 条 KernelInfo 字段）＋ `test_bootstrap_outside_memmap_*`（5 条 bootstrap memmap 边界）＋其余 6 条（`reserve_kernel_span`、4 条 `assert_range_clear_of_kernel`、`assert_modules_clear_of_kernel`）；测试模块内不出现 `find_platform_sources_into`，与 §5.3 的“均不覆盖”一致。`os/kernel/src/vm_handoff.rs` 的 3 处与 §5.3 分布表一致。

### 4. 读者条目

- **C-01**：§2.4 末尾（`653` 行）写明去掉 `if (ret->flags & 1) break;` 后第一次调用即耗尽全部 LAPIC 记录并返回 `NULL`，`discover_cpus` 的 `while` 立刻结束，`ncpus` 恒为 0，症状恒为“一颗都没有”。与 `minix3/minix/kernel/arch/i386/acpi.c:acpi_get_lapic_next` 的控制流一致。
- **C-02**：§5.3 `2530` 行“上述 54 处”；命令 `rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | wc -l` 实测 `54`，分布逐行求和 `10+9+6+2+1+1+3+1+21 = 54`。
- **C-06**：§4.9 `2243` 行记录了两处 `unwrap_or` 的回退差异（`init_smp_state` 回 `topo.bsp_id`，`init_protection` 回逻辑 `0`）、Phase B 时 `SmpState` 尚未建立、`elected_bsp_hw_id` 自身回退到 `topo.bsp_id` 且不保证命中发现集合。源码 `os/kernel/src/lib.rs:1971-1974`（`unwrap_or(topo.bsp_id as usize)`）、`:2250-2253`（`unwrap_or(0)`）、`:1951`（`topo.bsp_id` 回退）与文档一致；`init_protection` 在 `init_smp_state` 之前调用（`lib.rs:1784` 与 `:1884`）。
- **A-04**：§2.1 `320-343` 行的 `apic_single_cpu_init` 节选包含 `acpi_init();` 与 `if (!detect_ioapics()) { lapic_disable(); lapic_addr = 0x0; return 0; }`，与 `minix3/minix/kernel/arch/i386/apic.c:1125-1150` 一致；导语“第一段是 APIC 存在性检查，随后才是常量赋值”与函数体一致。
- **A-05**：§2.3 `489-533` 行为 `acpi_read_sdt_at` 全文逐字引用，含两个 `read_func(addr, tb, sizeof(...))` 失败分支、签名闸、容量闸、第二次读取与校验和闸；与 `minix3/minix/kernel/arch/i386/acpi.c:88-125` 一致。
- **B-04**：§6.2 #4 的 `head -5` 输出复跑一致，第 4/5 行为 `os/kernel/src/vm_handoff.rs:3` 与 `os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq-riscv64/src/main.rs:1`。
- **B-05**：§5.3 `2528` 行列出的 21 个极简测试内核；`rg -l 'platform_sources:\s*&\[\]' os/qemu-tests/test-kernels | wc -l` 实测 `21`，文件清单与正文列举的目录一致。

### 5. reader-fix-log 抽查 6 条

| ID | 核对点 | 结论 |
|---|---|---|
| RD4-READER-A-01 | §1.1 表 1 第 2 行改为引 §3.8 的 PIT 端口 `0x40`/`0x43`，CPU 主频引 §2.9 | 自洽（§3.8 表确有 PIT 端口行，§2.9 `cpu_identify` 段存在） |
| RD4-READER-A-03 | §1.4 ② 的 9 行清单缩为 2 行摘要，细节移入 §4.2 末段 | 自洽（§4.2 `1627` 行保留 `DtbMemoryMap`/`preflight`/`authorize_and_copy`/`cross_validate_config_dtb`/5 条 panic 路径） |
| RD4-READER-A-06 | §2.7 新增“启动参数 → config_no_apic → config_no_smp → 实际分支”表 | 自洽（`main.c:443-468` 与 `304-307` 的分支逻辑一致） |
| RD4-READER-B-01 | 头部教学目标 ④ 与 §3 导语改为“八项核心 + 七项实现层索引” | 自洽（§3.1–§3.8 八节，§3.9 索引表七行） |
| RD4-READER-C-05 | §3.3 表行改为“IRQ 向量空间有多大”，表后补 `nr_irqs` 容量与 `IOAPIC_NUM_PINS = 24` 的区分 | 自洽（`os/plat/src/x86_64/interrupt.rs:217` 为 `24`，`NR_IRQ_VECTORS = 64`） |
| RD4-READER-C-09 | §4.4 错误清单补 `AcpiParseError` 其余变体 | 自洽（`acpi.rs:781-814` 共 13 个变体，文档 6 + 7 全覆盖） |

## FINDING RD4-CERT-01（P2）

**§3.1 的 `KernelInfo.platform_sources` 字段注释引用与源码不保真。**

- 文档 `1061` 行块头标注“节选（本字段的文档注释；省略号处为其余字段）”，`1073` 行写：
  `/// Each \`PlatformDescSource\` is pure data \`(kind, len: u32, phys_addr: PhysBytes)\` —`
- 源码 `os/libs/minix-boot/src/kernel_info.rs:132` 原文：
  `/// Each \`PlatformDescSource\` is pure data \`(u32 kind, u32 len, u64 phys_addr)\``
- 源码 `134-136` 行紧接着的 `` `len = 0` means the producer did not record the blob extent; the window builder then falls back to one 2 MiB piece. `` 在文档中被无省略号删除，而块头把省略号限定为“其余字段”。

判定：P2（语义不矛盾，属引用保真/文档-代码同步）。建议把源码行逐字抄回，或把块头改成“改写示意”并用 `...` 标出删除位置。

## FINDING RD4-CERT-02（P2）

**§4.2 两处代码节选的文本与源文件不一致，“注释保留”不成立。**

1. x86 节选 `1598` 行：
   `assert!(sources.len() <= out.len(), "boot-shim: platform-source slot array too small");`
   源码 `os/boot-shim/src/uefi_helpers.rs:270-275` 为多行断言，消息是
   `"boot-shim: platform-source slot array has {} slots, firmware supplied {}", out.len(), sources.len()`。
2. riscv 节选 `1615-1622` 行的注释在 `os/boot-shim/src/opensbi_helpers.rs:408-421` 中不存在：源码注释为
   `// OpenSBI passes the DTB physical address in register a1; the entry`
   `// trampoline saved it via \`install_dtb_ptr(a1)\`. On RISC-V only DTB`
   `// is supported (no ACPI), so the list is \`[DTB]\` or empty.`
   文档另写了“If no DTB was passed (e.g., unit test), this returns None …”一段，并省略了源码的 `let len = unsafe { minix_boot::dtb_total_size(PhysBytes(pa)) };` 与 `.expect("boot-shim: platform-source partition over capacity")`。

判定：P2（“写入槽位并返回条数”与“riscv 用 `push_platform_source`”的功能结论仍成立；问题是引用文本不可 grep、与“注释保留”的标注不符）。建议按源码同步，或把块头改为“改写/示意”并显式标注省略。

---

certify 新准入 = 2
