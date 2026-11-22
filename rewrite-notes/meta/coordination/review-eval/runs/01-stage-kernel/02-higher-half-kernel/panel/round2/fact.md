# D02 R2 独立事实审查（P-FACT）

- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（2111 行）
- 关联代码：`runs/01-stage-kernel/02-higher-half-kernel/related-code.txt`（30 文件，全部存在）
- 模式：只读；未读取 round1 / defect-ledger / archive / expected
- 本轮定位：机械门复跑 + R1 修复点复核 + 事实/引文/常量/测试名与计数/路径核对

## 一、机械门（全部通过）

| 门 | 命令 | 结果 |
|---|---|---|
| anchor | `tools/anchor-resolve.sh --check <doc>` | `anchors=65 resolved=65 zero-def=0 multi-def=0`，exit 0 |
| doc-code-map | `tools/doc-code-map.sh <doc> --check` | 30 个文件，存在性 30/30，exit 0 |
| structure | `python3 tools/doc-structure-check.py <doc>` | 表结构与内部 `.md` 链接均可解析，exit 0 |
| ref | `AWK=gawk tools/doc-ref-check.sh --check <doc>` | 内部章节引用均可解析，exit 0 |
| style | `tools/doc-style-lint.sh <doc>` | error 级命中 0，exit 0 |
| citation | `python3 tools/doc-citation-check.py <doc>` | `blocks=47 checked=27 failed=0 skipped=15 warned=5`，exit 0 |
| density | `tools/doc-code-density-check.sh --check <doc>` | 4 个可判章均有码/命令，exit 0 |

## 二、FINDINGS

### F1（P1，事实错误）§5.2 对 L3a 的描述与脚本实现不符

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1895`
- 原文：「L3 的子断言：L3a = 首段 VMA/LMA 等于该架构预期基址；L3b = 按脚本基址常量算出的预期首段 VMA 与实测首段 VMA 相等；L3c = 首段 LMA 等于预期装载基址（x86_64 为 `0x200000`）。」
- 证据：`os/kernel-image/check-layout.sh:212` 的 L3a 实际是段数检查：
  `check "L3a" "PT_LOAD 段数 >= 1（实得 $nload）" "$([ "$nload" -ge 1 ] && echo 1 || echo 0)"`；
  首段 VMA 比较是 `check "L3b" "首段 VMA = 0x$want_vma…"`（`:219-220`），首段 LMA 比较是 L3c（`:221-222`）。文档把「VMA/LMA 等于基址」整体挂到 L3a 名下，既漏了段数检查，也与 L3b/L3c 的实际分工重叠。
- 最小修复：把该句改为「L3a = 至少一个 `PT_LOAD` 段；L3b = 实测首段 VMA 等于按脚本基址常量算出的预期值；L3c = 实测首段 LMA 等于预期装载基址（x86_64 为 `0x200000`）」。
- 置信度：高（脚本原文可直接对读）。
- 若我错了：若项目对 L3a 另有一套文档口径（例如把 L3a 定义为 L3 条目的语义分组而非脚本断言编号），需要在 §5.2 明示这层映射；当前正文没有这层说明。

### F2（P1，事实错误）§5.2 对 L6b 的描述把 L6c 的内容并入，且遗漏 L6c

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1895`（同段末尾）；相关表述在 `:1889` 的 L6 表行
- 原文：「L6 的子断言：L6a = 入口等于首段 VMA；L6b = `_start` 符号等于入口且落在首段内。」
- 证据：`os/kernel-image/check-layout.sh:248-258`：
  - `check "L6a" "入口 = 0x$entry_want…"`（入口等于按脚本常量算出的预期值，`entry_want=want_vma`，见 `:152`）；
  - `check "L6b" "符号 _start = 入口…"`（只比较符号与入口）；
  - `check "L6c" "入口 … 落在首段 […) 内"`（“落在首段内”是 L6c，且检查对象是入口，不是 `_start`）。
  文档把 L6c 的职责写进 L6b，并且完全没有列出 L6c。
- 最小修复：改为「L6a = 入口等于预期首段 VMA；L6b = `_start` 符号等于入口；L6c = 入口落在首段范围内」，并在 L6 表行补一句「含 L6c 的首段范围检查」。
- 置信度：高。
- 若我错了：若 L6c 是在 R1 之后新增、文档有意只列 L6a/L6b，则需在 §5.2 说明 L6c 不在清单内的理由；但正文自称“唯一权威清单”，遗漏一条实际断言与定位不符。

### F3（SUGGESTION，表述风险）§6.1 首行的“三个时刻”引用面偏宽

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:2000`
- 原文：「（出现时刻三选一：分页开启后立刻／恒等映射被移除时／首次切地址空间后——三个时刻的区分见 §2.4、§2.3、§4.3 与 §4.4 的反向推演）」
- 证据：三个时刻分别落在 §4.3（`enable` 与 Step 1 窗口）、§4.4 其二（riscv64 `delta` 缺失导致切地址空间后取指落低半）、§2.3/§2.4（C 侧 `pg_identity` 与换栈次序）。引用本身都能解析（ref 门通过），但“见 §2.4、§2.3、§4.3 与 §4.4”没有把三个时刻与四个小节一一对应，读者无法判断哪个时刻看哪一节。
- 最小修复：改为「分页开启后立刻见 §4.3；恒等映射被移除见 §4.4 其二；C 侧对照见 §2.3/§2.4」，或按三个时刻分列。
- 置信度：中（属可读性/锚点精确度，不是硬事实错误）。
- 若我错了：若该句的本意就是“四个小节合起来覆盖三类症状”，则不算缺陷，仅建议拆分。

### F4（SUGGESTION，证据强度）§5.4 M12 的“已覆盖”依赖一次性手工复跑

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1944`
- 原文：「M12 | 布局断言的持续成立（§5.2） | L0–L10 全过 | 已覆盖（手工复跑） | `check-layout.sh` 三架构实跑（本轮复跑全 PASS）；这是一次性复跑，不是常驻 CI 断言」
- 证据：同格已自陈“不是常驻 CI 断言”，但“覆盖状态”列仍写“已覆盖”，与 §5.0 小结「测试计数要按当前源码复算」及全篇“证据边界”口径存在张力；`check-layout.sh` 需要对应 toolchain 与构建，宿主测试门不包含它。
- 最小修复：覆盖状态改为「已覆盖（一次性手工，非常驻）」或「部分覆盖（手工复跑）」。
- 置信度：中。
- 若我错了：若项目把“手工复跑通过”也计入“已覆盖”，则应在 §5.4 开头统一声明覆盖状态的口径。

## 三、R1 修复点复核（未发现新缺陷的部分）

| 复核点 | 结论 | 证据 |
|---|---|---|
| §3.2 `InvalidSegment` 计数 | 通过 | `os/libs/minix-elf/src/lib.rs:153-193` 共 12 个变体，四组划分（5+2+2+3）与文档一致；`loader.rs:410-433` 的 `segment_spans` 确有 `paddr+filesz`、`mem_end<file_end`、两处 `usize::try_from` 与 `src_start+src_len` 检查，文档“不止三个返回点”成立；“三处”指名 `compute_kernel_layout` 的 `checked_add`/`checked_sub` 与段间重叠，见 `loader.rs:271,284,198` |
| §4.3 Step 2 守卫条件 | 通过 | `os/kernel/src/lib.rs:1409` 起 `if kern_virt != kern_phys { … }`，注释说明 riscv64 同 VPN[2] 槽二次写入风险；文档 §4.3 的两处推演（相等时跳过、照做会 `AlreadyMapped` panic）与 `.expect("kernel map: map_huge failed — kernel image mismatch")` 对得上 |
| §5.2 L3a/b/c 与 L6a/b | **见 F1/F2** | `check-layout.sh:212-222,248-258` |
| §1.1 映射主体 | 通过 | v1 路径由 boot-shim 建根（`§3.6`/`§4.5` 与 `enter_kernel_image_v1` 的 `enable` 叙述一致）；直核路径由内核镜像 `arch_boot` 建根；与 `os/kernel/src/lib.rs` 的 `arch_boot_impl`/`resume_additive` 分工一致 |
| §6.1 三时刻引用与栈症状拆行 | 引用可解析；精确度建议见 F3 | 两行栈症状分别对应 §2.3“只跳不切”（静默损坏）与 §3.3（栈顶算错导致低地址页错误），拆分自洽 |
| §3.1 三形态 | 通过 | `os/kernel-image/x86_64.ld` 的 `KERNEL_HIGH_BASE=0xFFFF800000000000`、`KERNEL_PHYS_BASE=0x200000`；`aarch64.ld`/`riscv64.ld` 各有 4 条 `: AT(`（放宽 `AT(` 时各 5 条），与 §6.2 的读数一致；测试载体 `test-higher-half-riscv64/link.ld` 存在 |
| §5.4 证据边界 | 通过（M12 表述见 F4） | 三份 `jump_to_kmain` 各有一条 `kern_stack_top` 16 字节对齐断言（`x86_64/higher_half.rs:34-37`、`aarch64:33-36`、`riscv64:36-39`），x86_64 另有 `stack_top-24 & 0xF == 8` 断言（`:39-42`），与 M13 描述一致 |
| §2.5 假设/时间窗 | 通过 | `mpx.S` 的 `LABEL(__k_unpaged_k_initial_stktop)` 与 `head.S` 的 `$k_initial_stktop` 引用均在；L10 机械检查 `mpx.o` 不在名单、三个 `.unpaged_*` 收段存在（`check-layout.sh:47-75`）；“……引导结束后会被 `add_memmap` 交回”与 `main.c` 的注释一致（引文由 citation 门验证） |
| §4.0 建根主语 | 通过 | “v1 是 boot-shim、直核是内核镜像、接收侧只做 additive”与 §3.6/§3.8 及 `arch_boot_resume_v1` → `resume_additive` / `arch_boot_impl` 的代码分工一致 |

## 四、常量与计数复核

| 项 | 文档值 | 复核结果 |
|---|---|---|
| minix-elf 测试数 | 27 | `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` = 27 |
| loader.rs 测试数 | 32 | `grep -c '#\[test\]' os/boot-shim/src/loader.rs` = 32 |
| 六个 `first_overlapping_pair_*` | 6 | 实有 disjoint/overlap/abutting/ignores_zero_length/returns_first/empty 六个 |
| `0xF0400000 / 0x400000` | 961 | 复算 = 961 |
| riscv64 VPN[2] | 256 / 0 / 0 | `0xFFFFFFC000000000→0x100`，`0xFFFFFC…→0x0`，`0xFFFFC…→0x0` |
| `KERN_SIZE` | `0x40_0000` | `os/kernel-image/src/bootface.rs:113` |
| AT 表达式条数 | x86_64 4、另两份 5（放宽） | 实测 `4/5, 4/5, 4/4` |
| run_qemu 退出码 | 0=PASS/SKIP，1=失败 | `run_qemu.sh:209-225` |
| riscv64 GDB 算例 | `0x80200144 + delta = 0xffffffc000200144` | `delta = 0xffffffbf80000000`，求和一致 |

## 五、结论

机械门全绿；R1 重点复核项中，§3.2、§4.3、§2.5、§4.0、§3.1、§5.4（除 M12 表述）均与代码对得上。新发现两处高置信度事实错误，集中在 §5.2 对 `check-layout.sh` 子断言编号的定义（L3a、L6b/L6c），以及两处 SUGGESTION（§6.1 时刻引用精确度、§5.4 M12 覆盖状态措辞）。F1/F2 属“文档自称唯一权威清单却说错断言编号”，建议按 P1 修正后再进入下一轮。
