DOC 01-boot-shim-bootstrap.md

# FACT 面板 · 01-boot-shim-bootstrap · V3.12 R1

- 目标：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2,983 行，sha256 `82271968c81aae8630a70a886e80fb031546d79e80f84cf73df9a88c1842fe90`，与简报一致）
- 范围：只核事实（锚点/路径/常量/签名/引文/测试名与计数/反事实前提/阴性声明），不评文风、不评重构。
- 只读纪律：未修改任何仓库文件；本文件是唯一写入。

## 0. 机械门读数（本轮实际执行，仓库根）

| 门 | 命令 | 读数 |
|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=90 resolved=90 zero-def=0 multi-def=0`（exit 0） |
| 关联代码 | `tools/doc-code-map.sh <doc> --check` | 48 个文件，存在性 48/48 通过（exit 0） |
| 结构 | `tools/doc-structure-check.py <doc>` | 表结构与内部 .md 链接均可解析（exit 0） |
| 内部引用 | `tools/doc-ref-check.sh <doc>` | 内部章节引用均可解析（exit 0） |
| 文风 | `tools/doc-style-lint.sh <doc>` | error 级 = 0（exit 0） |
| 引文 | `tools/doc-citation-check.py <doc> --verbose` | blocks=54 checked=21 failed=0 skipped=29 warned=4；4 个 WARN 是声明为改写/译述的块（1411-1441、1445-1466、1938-2038、2250-2279），非失败 |

引文门的覆盖缺口（工具自身读数，不是我手工重做的项）：29 个 SKIP 块不含可对账标注，其中 §2 的 C 引文块是多行 `/* … */` 标注，机械门识别不到；这些块由我逐块人工对账（见 §1 的阴性声明与发现）。

## 1. 阴性声明（每项写扫了什么、发现数、N=0 不覆盖什么）

1. **C 常量**：扫 `minix3/sys/arch/i386/include/multiboot.h`、`minix3/minix/include/minix/{param.h,const.h}`、`minix3/minix/kernel/arch/i386/include/archconst.h`、`minix3/minix/kernel/kernel.h` 中的 `MULTIBOOT_*`、`MAXMEMMAP`、`NR_MEMS`、`NR_BOOT_PROCS`、`GDT_SIZE/TSS_INDEX/IDT_SIZE/DESC_SIZE`、`CONFIG_MAX_CPUS`。发现 0 处不一致；不覆盖：文档未引用的宏值与平台变体。
2. **C 引文逐字性**：逐块对账 `head.S`（i386/earm）、`pre_init.c`（i386/earm）、`pg_utils.c`、`kernel.lds`、`arch_smp.c`、`protect.c`、`arch_system.c`、`lib/exec.c`、`multiboot.h`、`main.c`、`table.c`、`vm/main.c`、`vm/utility.c`、`vm/alloc.c`、`vm/pagetable.c`、`do_getinfo.c`、`sysenv.c`、`update_bootcfg.sh`、`boot.cfg.default`、`assert.c`（两份）、`loadfile_elf32.c`、`loadfile*.h`、`pci.c`、`dmp_kernel.c`、`procfs/root.c`、`biosmemx.S`、`trampoline.S`。发现 0 处改写（所有省略都在块首标注里声明）；不覆盖：文档未引用的 C 文件与平台。
3. **Rust 符号存在性**：抽文档全部反引号标识符（734 个候选）对 `os/`、`minix3/` 做存在性扫描；零命中项全部是文档明说的已删符号（`startup_objects`/`StartupObjects`/`WIRE_STARTUP_END`/`STARTUP_RECORD_STORE`/`VM_BOOT_HANDOFF_MAX_IDENT`/`ident_regions`/`BootShim`/`RiscvUbootBootShim`）、`struct.field` 记法、`file:sym` 标注或十六进制片段。1 处真实失效：`MapError::Policy`（FACT-01）。不覆盖：同名不同义的符号（只看名字是否出现，未看定义处语义）。
4. **测试计数与测试名**：`cargo test -p minix-boot --lib -- --list` → `188 tests`；`-p boot-shim --lib` → `136 tests`；`-p minix-kernel --features mock --lib` → `874 tests`（其中 8 个 `#[ignore]` → 866 非 ignored）；`-p minix-elf --lib` → `27 tests`。与 §5.2/§5.0 的 `188/0`、`136/0`、`866/0/8`、`27/0` 相符。文档点名的 8 个测试函数逐一命中。不覆盖：实跑通过性（只到 `--list`；未跑测试体）。
5. **依赖表**：四份 `Cargo.toml` 的 `[dependencies]` 与 `[target.'cfg(target_arch = …)'.dependencies]` 逐条对账 §3.5.2 的四行表。发现 0 处不一致。不覆盖：`[dev-dependencies]`（文档明说从略）。
6. **里程碑字符串**：扫 19 条 §6.1 文案在 `os/boot-shim/src`、`os/kernel/src`、`os/kernel-image/src` 的命中数；全部 ≥1，且 §6.1 的“仅哪个腿打印”在 `resume_after_v1_decode` 的三个 cfg 臂、`arch_boot` 的 cfg 臂、`device_window_regions` 的空表分支上逐一核对。不覆盖：实跑串口日志里的实际出现次数。
7. **门脚本判据**：核 `test-shim-bootmarks-x86.sh`、`test-shim-bootmarks-aarch64.sh`、`test-riscv64-uboot.sh`、`test-riscv64-uboot-shim.sh`、`test-kernel-image-riscv64-bootface.sh` 的 MARK 变量与 grep 文案。1 处标记串不逐字（FACT-02）；不覆盖：脚本在真实 QEMU 下的判定结果。
8. **路径存在性**：§7.2/§7.3/§7.4 的 C/Rust/脚本清单逐条 `test -e`；全部存在（含 `os/libs/minix-platform/src/`、`smp_gpt_v3.md`、`01-design-note-memmap-consolidation.md`）。不覆盖：清单是否“完备”（文档自称只列主干）。

## FINDING FACT-01（P2）
- 位置：文档 §4.2.2（“另有一条与分流正交的界”段），文档第 2133 行。
- 严重度：P2
- 问题：文档写越界拒绝“经 `PolicyError::OutOfPhysRange`，经 `MapError::Policy`/`RejectedDescriptor` 上抛”，但 `MapError` 没有 `Policy` 这个变体。
- 影响：读者按名查 `MapError::Policy` 会零命中，误以为存在第二条上抛路径；实际只有 `MapError::RejectedDescriptor(DescriptorRejection { failed: PolicyError, .. })`。这削弱“分类错误如何上报”这段的可核性。
- 证据或推理（E1，命令+输出）：`grep -n 'enum MapError' -A 25 os/boot-shim/src/ebs_map.rs` 给出变体全集 `UnsupportedVersion / DescriptorTooSmall / DescriptorSizeNotAligned / MapSizeBeyondBuffer / MapSizeNotMultiple / DescriptorBeyondMap / TooManyDescriptors / RejectedDescriptor(DescriptorRejection) / Capacity(HandoffV1WriteError) / TooManySideEntries`，无 `Policy`；`grep -rn 'MapError::Policy' os --include='*.rs'` 只命中另一 crate 的同名枚举（`os/servers/vfs/src/device_map.rs`，与 boot-shim 无关）。`ebs_map.rs:674` 的实际包装是 `MapError::RejectedDescriptor(DescriptorRejection { …, failed })`。
- 最小修复：把该括号改成“（`PolicyError::OutOfPhysRange`，由 `classify` 包成 `MapError::RejectedDescriptor` 上抛）”；删掉 `MapError::Policy` 的提法。
- 置信度：高
- 是否需源码核实：已核实（`os/boot-shim/src/ebs_map.rs:343-360`、`:665-680`）
- 若我错了：若 `MapError` 在别处有 `type Policy = …` 或 `use … as Policy` 的别名，我漏看；但 `grep -rn 'MapError::Policy' os --include='*.rs'` 除 VFS 同名枚举外零命中，且 `ebs_map.rs` 无 `impl From<PolicyError> for MapError` 之外的别名定义。

## FINDING FACT-02（P3）
- 位置：文档 §5.4 表格 “riscv64 直核 bootface” 行（文档第 2674 行）。
- 严重度：P3
- 问题：该行把判据路标写成 `table holds 0x0 entries`，但代码与门脚本实际匹配的是 `table holds 0x0000000000000000 entries`（`0x` 后 16 个十六进制位）。
- 影响：低。读者若按文档字符串去 grep 串口日志会零命中；按脚本实际字符串才能命中。§4.3 引文里的 `say("  table holds 0x")` + 十六进制打印是对的，只有 §5.4 的判据缩写不成字面。
- 证据或推理（E1）：`grep -n 'table holds' os/kernel-image/src/bootface_common.rs` → `say("  table holds 0x")`（随后 `say_hex(entries.len() as u64)`，见第 219-224 行）；`grep -n 'table holds' os/qemu-tests/test-kernel-image-riscv64-bootface.sh` → 第 124 行 `grep -an "table holds 0x0000000000000000 entries"`。
- 最小修复：把 §5.4 该格改成 `table holds 0x` + 十六进制条目数的形式（或直接引脚本第 124 行的完整串），与 §4.3 的打印分解保持一致。
- 置信度：高
- 是否需源码核实：已核实（`os/kernel-image/src/bootface_common.rs:219-224`、`os/qemu-tests/test-kernel-image-riscv64-bootface.sh:124`）
- 若我错了：若脚本另有对短串 `0x0` 的匹配分支；已看第 124 行与其上下文（第 120-131 行），只有全 16 位串。

## FINDING FACT-03（P3）
- 位置：文档 §4.4（`boot_validate_and_prepare` 引文前的说明句，文档第 2334 行）。
- 严重度：P3
- 问题：文档写“签名是 `fn boot_validate_and_prepare<A: BootArch>(kern_virt_base: VirBytes, kern_phys_base: PhysBytes, kern_size: u64)`”，漏了返回类型 `-> BootPageSupport<A>`；源码签名三参且返回 `BootPageSupport<A>`。
- 影响：低。参数个数与类型正确，但“签名是”这句话不完整；读者若据此写调用点会少拿返回值（`... let support = ...`）。文档引文体内 `let support =` 与末尾 `support` 已隐含返回值，故只是说明句不完整。
- 证据或推理（E1）：`sed -n '863,868p' os/kernel/src/lib.rs` 显示 `fn boot_validate_and_prepare<A: BootArch>(kern_virt_base: VirBytes, kern_phys_base: PhysBytes, kern_size: u64,) -> BootPageSupport<A> {`。
- 最小修复：在说明句末尾补 `-> BootPageSupport<A>`。
- 置信度：高
- 是否需源码核实：已核实（`os/kernel/src/lib.rs:863-868`）
- 若我错了：若文档此处刻意只描述入参形状并在别处给了返回类型；§4.4 全文未再给该函数的返回类型（`grep -n 'BootPageSupport' 文档` 零命中），故仍属说明不完整。

