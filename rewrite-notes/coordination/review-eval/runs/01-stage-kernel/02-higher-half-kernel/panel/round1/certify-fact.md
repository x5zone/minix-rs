# D02 R1 certify-fact（事实回读）

- 目标：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 读法：只读本轮 diff 的 32 个 hunk 及邻句；未读其他面板输出、历史评审与 `expected/`
- 行号口径：一律用改动后的工作树行号；任务书给的行号与工作树有少量偏移，按 `git diff -U0` 的 hunk 定位

## 0. 基线核对

| 项 | 读数 | 复算命令 |
|---|---|---|
| 基线提交 | `5ac8bfa8f`（HEAD） | `git rev-parse HEAD` |
| 基线文档 sha256 | `2bb4a585597ffbd27cf4495292e2e85f026e1782b08fe5268426f374357e5485` | `git show HEAD:rewrite-notes/01-stage-kernel/02-higher-half-kernel.md \| sha256sum` |
| 基线行数 | 1275 | `git show HEAD:... \| wc -l` |
| 当前文档 sha256 | `5bb66c3e5e0e4d4be898551de4cca72814e72838c27a851960d0b593aac65600` | `sha256sum rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` |
| 当前行数 | 1281 | `wc -l` |
| 本轮 diff 规模 | 32 hunk，+35 / −29 | `git diff --numstat`、`git diff -U0 \| grep -c '^@@'` |

基线 sha256 前缀 `2bb4a585597ffbd2` 与任务书一致。工作树除本篇与既有协调文件外无新增改动；本轮未写任何仓库源文件。

## 1. 机械门读数

任务书命令里的 `doc` 是占位符；下面按实际文档路径执行，读数原样记录。

| 门 | 命令 | 读数 | exit |
|---|---|---|---|
| 引文对账 | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `blocks=32 checked=1 failed=0 skipped=31 warned=0` | 0 |
| 锚点解析 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `anchors=57 resolved=57 zero-def=0 multi-def=0` | 0 |
| 表结构/内部 md 链接 | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `1 个文件表结构与内部 .md 链接均可解析` | 0 |
| 章节引用 | `AWK=gawk tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `内部章节引用均可解析` | 0 |
| 关联代码清单 | `tools/doc-code-map.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --check` | `共 28 个文件；存在性 28/28 通过` | 0 |

五门全绿。`doc-code-map` 从基线的 27 条变为 28 条，增量是 §7.3 新加的 `os/kernel-image/src/bootface_common.rs`；该文件存在。

读数口径说明：`doc-citation-check` 只判了 1 个块，其余 31 块被跳过（本篇多数代码块以 `/* 节选… */` 开头、无语言标记）；因此代码节选的逐字核对以本文第 2 节的手工命令为准，门读数只作辅助。

## 2. 逐条 CLOSED / REOPEN

### 2.1 §2.2 “kmain 改名”推演 → CLOSED

旧的“反方向错法会让 `kmain` 落进低侧并静默执行”整段已删；现文只说 `objcopy --prefix-symbols` 同时改定义名与引用名、链接期会暴露，不再保留低侧副本静默执行的现场。

依据：
- `minix3/minix/kernel/arch/i386/Makefile.inc:51-67` 的 `.for unpaged_obj` 与 `objcopy --prefix-symbols=__k_unpaged_` 规则；
- 本机复现：对一个含未定义符号的对象执行 `objcopy --prefix-symbols=__k_unpaged_`，`nm` 显示未定义符号与重定位项都被改名（`U __k_unpaged_foo`，`R_X86_64_PLT32 __k_unpaged_foo`）。

保留一条邻句观察（不阻断）：`minix3/minix/kernel/proto.h:49-52` 已用 `#define kmain __k_unpaged_kmain` 给 `kmain` 一个预置前缀，且 `main.c` 同时在 `SRCS` 里；若真把 `main.o` 加进改名名单，实际形态是“原 `main.o` 仍提供 `__k_unpaged_kmain`、低侧多出一份双重前缀的死副本”，既不是“整体落低”，也不一定链接失败。现文没有再拿 `kmain` 当例子，结论“不会留下低侧副本静默执行”仍成立；只是“链接期就会暴露”的二分法不完备。见 §4 观察 O-1。

### 2.2 §2.3 / §4.3 帧指针叙述 → CLOSED

- C 侧：`minix3/minix/kernel/arch/i386/head.S:70` 在 `call pre_init` 之前 `mov $0, %ebp`；EBP 是 i386 SysV 的 callee-saved，`pre_init` 返回时 EBP 必为 0（无论它是否使用帧指针），所以换栈后 `kmain` 的 `push %ebp` 保存槽是 0。源码没有与这句话冲突的证据。
- Rust 侧：全仓检索 `force-frame-pointers` 只命中 `os/arch/src/x86_64/boot.rs:420` 的一句注释，`os/.cargo/config.toml`、`os/xtask/src/image.rs` 的 RUSTFLAGS 注入都没有该开关。产物级复核：`os/target/x86_64-unknown-none/release/kernel` 的 `minix_kernel::kmain` 序言是 `push %r15/%r14/%r12/%rbx`，全函数 `%rbp` 出现次数为 0（`objdump -d --disassemble='_ZN12minix_kernel5kmain17h94a869568c2e2d66E' ... | grep -c %rbp` → `0`），证实“生产 `kmain` 不保存 `%rbp`”。该产物同日构建、符号是普通 `kmain`（无 `kmain_verify`），不是 `qemu_test` 裸函数。
- §4.3 “其三”把“链的终止点”从“第一个带序言的帧的保存槽”改为“清零后的寄存器本身”，与上述产物一致。

### 2.3 §3.3 `kern_stack_top` 第二份实现 → CLOSED

`os/libs/minix-boot/src/boot_storage.rs:356-378` 的 `BootStorage::report()` 确有

```rust
kern_stack_top: VirBytes(kern_virt_base.0 + kern_size),
```

与 `os/boot-shim/src/lib.rs:125` 的 `build_kernel_info` 同式。`report()` 由 UEFI `uefi_helpers.rs:1064/1185` 与 `opensbi_helpers.rs:449` 的 v1 发送侧调用，故“v1 发送侧的第二处、二者必须锁步”成立。

### 2.4 §4.4 `build_kernel_info` 11 参 / `place_modules` / RDI 遮蔽 → CLOSED

- `os/boot-shim/src/lib.rs:101-132` 的形参共 11 个：`memmap`、`reserved_regions`、`kern_virt_base`、`kern_phys_base`、`kern_size`、`boot_modules`、`platform_sources`、`param_buf`、`memmap_sources`、`startup_objects`、`authority`。文档节选 11 个实参逐一对应，顺序一致（`sources` 对应 `memmap_sources`，`InputObjectId::DtbBootInput` 对应 `authority`）。
- `os/kernel-image/src/bootface.rs:427-439` 的实际调用与节选逐行一致；节选后的 `...` 覆盖了 `kernel_info.validate_requiring_sources();`，随后 `say(...)` 与 `minix_kernel::arch_boot(&kernel_info, root_page)` 也与源码一致。
- `place_modules` 定义在 `os/kernel-image/src/bootface_common.rs:177`，由 `bootface.rs:296` 调用；文档已把锚点从 `bootface.rs` 迁到 `bootface_common.rs`，锚点门解析通过。
- RDI 遮蔽：`os/kernel/src/lib.rs:1687-1688` 的第一条运行语句是 `let kernel_info: &KernelInfo = crate::kernel_info().expect(...)`，与形参同名遮蔽；`arch_boot` 的源码注释（`lib.rs:242-253`）给出旧 RDI 被覆写的现场，`kmain` 的注释（`lib.rs:1676-1686`）给出第二次根因（形参被溢出到内核栈后读回）。文档叙述与两处注释一致。
- 两块临时打印：`kernel: arch_boot arg=`（`lib.rs:257`）与 `kernel: preA rdi=`（`lib.rs:1707`）均在 x86_64 生产路径，注释自标 `temporary`。

### 2.5 §4.2 `load_segments_into_phys_memory` 节选 / 源目标重叠 / 四步 → CLOSED

- 文档节选与 `os/boot-shim/src/loader.rs:286-316` 的 `pub unsafe fn`、`segment_iter`、`Vec` 收集、`first_overlapping_pair` 早退、`...`、逐段 `copy_from_nonoverlapping` + `[filesz,memsz)` 清零逐行一致；`...` 对应的正是 `src_base` 与 `first_source_destination_overlap` 的 panic 段（`loader.rs:292-302`）。
- 源/目标重叠检测：`loader.rs:321-343` 用 `elf_base + offset .. + filesz` 与 `paddr .. + memsz` 判交叠，命中 `panic!`；段间重叠则返回 `ElfError::InvalidSegment`（`loader.rs:289-291`）。文档“panic 而返回值、与段间重叠不对称”的表述正确。
- 四步顺序：收集、段间重叠、源/目标重叠、拷贝+清零，与源码顺序一致；`a.paddr < b_end && b.paddr < a_end` 的半开区间判据与 `loader.rs:348-367` 一致。
- 正文“清零条件是 `memsz > filesz`；`filesz == memsz` 不额外写零、`filesz == 0` 只清零”与源码 `if seg.memsz > seg.filesz` 一致。
- 缓冲区孪生 `loader.rs:224-261` 有 `file_end <= buf.len()` / `mem_end <= buf.len()` 上界，物理版有重叠检测；文档的差异描述正确。

同节的 BSS 清零反推另有一条新事实错误，见 FINDING RD1-CERT-FACT-02。

### 2.6 §5.1 计数 22/125、六个 `first_overlapping_pair_*`、§5.4 M11 → CLOSED；§5.4 M7 计数 → REOPEN

- `grep -c '#\[test\]' os/boot-shim/src/loader.rs` → `22`；`cargo test -p boot-shim --lib -- --list` → `125 tests, 0 benchmarks`，其中 `loader::tests::` 恰 22 条。§5.1 的 22/125 与“十三个直接相关”均正确（7 个具名 + 6 个 `test_first_overlapping_pair_*`）。
- 六个 `first_overlapping_pair_*` 测试名逐一存在：`disjoint`、`overlap`、`abutting`、`ignores_zero_length`、`returns_first`、`empty`（`loader.rs:875-970`）。
- §5.4 M11 的描述列已改为“形参按 ABI 传入 RDI/X0/A0，生产 `kmain` 第一条语句即遮蔽为全局副本”，与 §4.4 及源码一致；证据列的“NK4-A 的寄存器覆写修复与注释”对应 `arch_boot` 注释里的 RDI caller-saved 现场，不构成新错。
- §5.4 M7 仍写“loader 18 测试”，与 §5.1 的 22 和实测 22 矛盾，见 FINDING RD1-CERT-FACT-03。

### 2.7 其余 P2

| 项 | 结论 | 依据 |
|---|---|---|
| `_MINIX_MAGIC` 条件 | CLOSED | `minix3/share/mk/sys.mk:25-26` 仅在 `${MKMAGIC:Uno} == "yes" \|\| ${MKASR:Uno} == "yes"` 时定义；`minix3/share/mk/bsd.own.mk:1305-1310` 的 `_MKVARS.no` 默认 `Uno`；`const.h:33-43` 的 `USR_DATATOP` 0xE0000000/0xF0000000 分支与文档一致 |
| `.usermapped` 供料方 | **REOPEN** | 文档只列 `usermapped_data_arch.c` 的三个 `minix_ipcvecs_*`；`minix3/minix/kernel/usermapped_data.c:4-15` 另有 8 个 `__section(".usermapped")` 对象，且该文件在 `kernel/Makefile:16` 的 `SRCS` 里、链接脚本 `kernel.lds:27` 用 `*(.usermapped)` 全收。见 FINDING RD1-CERT-FACT-04 |
| aarch64 `isb` 注释 | CLOSED | `os/kernel/src/arch/aarch64/higher_half.rs:47` 的注释与文档节选逐字一致；`os/arch/src/arm64/paging.rs:695-710` 确为 `msr ttbr0/1 → dsb sy; isb → sctlr \|=1 → isb` |
| 载体 `link.ld` | CLOSED | `test-higher-half/` 与 `test-higher-half-aarch64/` 只有 `Cargo.toml`、`src/` 与 `.cargo/config.toml`（后者只设 target），没有 `link.ld`、没有 `build.rs`；`os/kernel/src/arch/*/link.ld` 存在且只被 `link_ld_lockstep.rs` 读取，无生产 `-T` 读者 |

### 2.8 §3.4 `syscall_entry` / `stack_top`、§3.2 静默跳过与 `InvalidSegment` → CLOSED

- `syscall_entry`：生产安装点 `os/kernel/src/lib.rs:2276` 用 `minix_arch::syscall_entry_va()`；`KernelInfo` 字段只在 `kernel_info.rs:458-461` 的严格门做等值断言、在 `handoff_v1_write.rs` 序列化。全仓唯一把它当入口地址消费的是 `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-riscv64/src/main.rs:148` 的 `trap.configure_syscall(info.syscall_entry)`。与文档一致。
- `stack_top` 冗余参数：生产 `jump_to_kmain` 调用点只有 `os/kernel/src/lib.rs:232`（direct）与 `:375`（v1 resume），两处都传 `kinfo.kern_stack_top`；`boot_validate_and_prepare` 的 16 字节断言（`lib.rs:878-882`）守的也是该值。参数可单独传别的值而不被类型拦住，与文档一致。
- §3.2 静默跳过：`os/libs/minix-elf/src/lib.rs:352-399` 的 `SegmentIter::next` 在 `p_offset + p_filesz` 越界、表项解析失败等情况下 `continue`，不产出该段、不报错；`compute_kernel_layout` 只对收到的段取极值。文档“段比预期少而不是段坏了”的表述正确。
- `InvalidSegment` 归一化：`loader.rs:189-207` 的 `filesz > memsz`、`paddr + memsz` 溢出、`kern_end_phys - kern_phys_base` 下溢，以及 `loader.rs:289-291` 的段间重叠，都返回同一个 `InvalidSegment`；源/目标重叠 `panic!`。文档“归一化丢失现场、分层属 D2-06”正确。

## 3. FINDING

## FINDING RD1-CERT-FACT-01（P2）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:934`（§4.3 riscv64 节选）
- 事实：节选写 `kinfo.kern_virt_base()`（方法调用），源码是字段访问 `kinfo.kern_virt_base`。
- 证据：
  - 文档 932-938 行：`let delta = kinfo` / `.kern_virt_base()` / `.0` / `.checked_sub(kinfo.kern_phys_base.0)`
  - 源码 `os/kernel/src/arch/riscv64/higher_half.rs:92-96`：`let delta = kinfo` / `.kern_virt_base` / `.0` / `.checked_sub(kinfo.kern_phys_base.0)`
  - `KernelInfo` 的字段定义在 `os/libs/minix-boot/src/kernel_info.rs:88`；`grep -rn "fn kern_virt_base" os/libs/minix-boot os/kernel/src` 无结果，不存在同名访问器
- 影响：节选不是源码原文，照抄不可编译。同段另两处本轮已从 `kern_phys_base()`/`kern_virt_base()` 改为字段形式，这一处漏改；属于同一修复的半闭合。
- 建议：删掉 `()`，改为 `.kern_virt_base`。

## FINDING RD1-CERT-FACT-02（P2）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:872`（§4.2 “把 BSS 清零这一步从两个环节里都去掉”）
- 事实：文档把两个自引导面的 `BootStorageCell`（`bootface.rs` 与 `bootface_a64.rs` 的 `STORAGE`）列为 `.bss` 内容；实际它在 `.data`。
- 证据：
  - `os/boot-shim/src/lib.rs:152-153` 的类型注释明写：An empty cell; usable as a static initializer (`.data`, since the contained `BootStorage::new()` is not all-zero).
  - aarch64 产物：`readelf -sW os/target/aarch64-unknown-none/release/kernel | grep STORAGE` → Ndx 4；`readelf -SW` 的 `[4] .data` 起点正是 `ffff80000006d000`，符号地址 `ffff80000006d000`
  - riscv64 产物：同类符号 Ndx 3，地址 `ffffffc000068000` 与 `[3] .data` 起点一致
  - 同段的 `BUMP_PTR` 部分正确：`nm` 显示 `d`，地址 `ffff800000070490` 落在 `.data` 区间
- 影响：反推例子包含一个不需要 BSS 清零、由文件字节初始化的 `.data` 对象。结论仍由 `IMAGE_HEAP`（`nm` 显示 `b`，`.bss`）与内核启动全局支撑，但“这些格子里会是上一位使用者的字节”不适用于 `BootStorageCell`。
- 建议：从 `.bss` 例子中删掉 `BootStorageCell`，或改写为“`IMAGE_HEAP` 与内核 `.bss` 全局需要清零；`BootStorageCell`/`BUMP_PTR` 在 `.data`，由装载的文件字节初始化”。

## FINDING RD1-CERT-FACT-03（P2）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1162`（§5.4 M7）
- 事实：M7 仍写“loader 18 测试”，与 §5.1 的 22 和实测 22 不符。
- 证据：
  - `grep -c '#\[test\]' os/boot-shim/src/loader.rs` → `22`
  - `cd os && cargo test -p boot-shim --lib -- --list` → `125 tests, 0 benchmarks`，`grep -c '^loader::tests::'` → `22`
  - 文档 1112 行已改为 22；1162 行仍为 18
- 影响：同一篇文档两处计数互相矛盾；本轮只同步了 §5.1，漏了 §5.4 M7。
- 建议：18 → 22。

## FINDING RD1-CERT-FACT-04（P2）

- 性质：既有 P2 项的 REOPEN（不是本轮新引入）
- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:124`（§2.1），并涉及 §7.2 的关联 C 源码清单
- 事实：文档称 `.usermapped` 的供料方是 `usermapped_data_arch.c` 里带 `__section(".usermapped")` 的三个 `minix_ipcvecs_*` 对象；实际还有 `minix3/minix/kernel/usermapped_data.c` 的 8 个同类对象。
- 证据：
  - `minix3/minix/kernel/usermapped_data.c:4-15`：`minix_kerninfo`、`kinfo`、`machine`、`kmessages`、`loadinfo`、`kuserinfo`、`arm_frclock`、`kclockinfo` 共 8 个 `__section(".usermapped")` 定义
  - `minix3/minix/kernel/Makefile:16`：`SRCS+= ... usermapped_data.c`
  - `minix3/minix/kernel/arch/i386/kernel.lds:27`：`.usermapped : AT(...) { *(.usermapped) }`
  - 文档 §7.2 的 C 源清单列了 `usermapped_data_arch.c` 与 `usermapped_glo_ipc.S`，未列 `usermapped_data.c`
- 影响：供料方清单不完整；“两个非空输出段”的结论不受影响，但事实陈述错误，且关联源码清单漏项。
- 建议：把 `usermapped_data.c` 补进 `.usermapped` 的供料方说明与 §7.2。

## 4. 观察（不纳入本轮准入）

- **O-1（§2.2，P3）**：`proto.h:49-52` 已把 `kmain` 的 C 定义改名为 `__k_unpaged_kmain`，且 `main.c` 同时在 `SRCS` 里。若把 `main.o` 加进改名名单，实际会链接出“高层原定义 + 低侧双重前缀死副本”，调用仍落到高层；既不是“整体落低”，也不必然链接失败。现文没有再拿 `kmain` 当例子，结论“不会留下低侧副本静默执行”成立；“链接期就会暴露”的二分法不完备。
- **O-2（§4.4，P3）**：节选块头的“省略号处为 DTB 解析、模块表、模块装载”没有提到 `build_kernel_info` 调用之后的 `kernel_info.validate_requiring_sources();`；该行被块内的 `...` 省略，但不在块头的省略范围说明里。
- **O-3（§4.3，P3）**：riscv64 节选块头写“省略号处为注释段”，块内实际没有 `...`，只是删掉了 asm 内的注释行。
- **O-4（§5.1，P3）**：13 条“直接相关”测试清单没有列源/目标重叠的三条测试（`test_first_source_destination_overlap_*` 两条、`test_source_destination_overlap_stops_before_any_segment_write`），而 §4.2 本轮新增的正是源/目标重叠段落。不是错误陈述，只是清单与新增段落的覆盖面没有对齐。
- **O-5（§4.4，P3）**：`.bss` 副本的措辞是源码注释级 shorthand。产物里 `KERNEL_INFO` 与 `BOOT_MODULE_STORE` 在 `.data`，`RESERVED_REGION_STORE`/`MEMMAP_REGION_STORE`/`PLATFORM_SOURCE_STORE` 在 `.bss`；若要把“副本”严格落到 section，需要再分一次。本轮任务书自身也用 `.bss` 副本这一口径，故只记观察。

## 5. 结论

- CLOSED：§2.2 推演删除、§2.3/§4.3 帧指针、§3.3 第二份公式、§4.4 11 参/迁址/RDI、§4.2 节选/重叠/四步、§5.1 22/125 与六个 overlap、§5.4 M11、`_MINIX_MAGIC`、aarch64 `isb`、载体 `link.ld`、§3.4 `syscall_entry`/`stack_top`、§3.2 静默跳过与 `InvalidSegment`。
- REOPEN：1 条（`.usermapped` 供料方，既有 P2 项；FINDING RD1-CERT-FACT-04）。
- 新发现：3 条 P2（RD1-CERT-FACT-01/02/03）。

**certify 新准入 = 3**（另 REOPEN 1 条既有 P2 项，不重复计入新准入）。

---

## certify #2

- 基线：本轮工作树 sha256 `0888ffb0bbb27b4f67560058c91e99e0749addcc72868e41130d44ceb3a557b2`、1280 行（上轮 `5bb66c3e...`、1281 行）。
- 范围：只读本轮 4 条 certify 修复与 12 条 reader 修复涉及的改动句及邻句，未重扫全文。
- 机械门（同一套五门，命令同 §1）：

| 门 | 读数 | exit |
|---|---|---|
| `doc-citation-check` | `blocks=32 checked=1 failed=0 skipped=31 warned=0` | 0 |
| `anchor-resolve.sh --check` | `anchors=56 resolved=56 zero-def=0 multi-def=0` | 0 |
| `doc-structure-check` | 通过 | 0 |
| `doc-ref-check --check` | 通过 | 0 |
| `doc-code-map --check` | `28/28` 存在 | 0 |

锚点数由上轮的 57 变为本轮的 56，全部门读数通过。

### 上轮 4 条

- **RD1-CERT-FACT-01（riscv64 残留 getter）→ CLOSED**：文档 934 行已是 `.kern_virt_base.0`（字段直读 + tuple 下标），与 `os/kernel/src/arch/riscv64/higher_half.rs:92-96` 的 `kinfo.kern_virt_base` 字段访问语义一致；`grep "kern_virt_base()"` 不再命中。
- **RD1-CERT-FACT-02（`BootStorageCell`/`BUMP_PTR` 的 `.bss` 例子）→ CLOSED**：文档 872 行改为“两者初值非零、落在 `.data`，不是这个例子的成员”。依据：`os/boot-shim/src/lib.rs:152-153` 明写 `.data`；aarch64/riscv64 产物的 `STORAGE` 符号分别在 Ndx 4/3（`.data`），`BUMP_PTR` 也是 `d` 且地址落在 `.data`。
- **RD1-CERT-FACT-03（§5.4 M7 的 18）→ CLOSED**：文档 1161 行改为“loader 的 22 个测试中 13 个直接相关”，与 §5.1 的 22/13 一致；`grep -c '#\[test\]' os/boot-shim/src/loader.rs` = 22，`cargo test -p boot-shim --lib -- --list` = `125 tests`，其中 `loader::tests::` 22 条。
- **RD1-CERT-FACT-04（`.usermapped` 供料方）→ CLOSED**：文档 124 行补了 `kernel/usermapped_data.c` 的 8 个对象；`minix3/minix/kernel/usermapped_data.c:4-15` 确为 8 个 `__section(".usermapped")` 定义，`kernel/Makefile:16` 把它列入 `SRCS`，`kernel.lds:27` 用 `*(.usermapped)` 全收；§7.2 的 1258 行也补了该文件。

### reader 侧 12 条

- **§2.2 改名两种情形 → CLOSED**：146-157 行补了 `.for`/`objcopy` 节选与 27 个对象的分批口径。显式 9 个 + `MINLIB_OBJS_UNPAGED` 3 个 + `MINC_OBJS_UNPAGED` 13 个 + `SYS_OBJS_UNPAGED` 2 个 = 27；复算 awk 输出 13、两条单行 grep 输出 3 与 2，均与文档一致。`head.o` 的 `MINIX`/`pre_init` 前缀、`main.c` 由 `proto.h:49-52` 宏改名、`asm.h:64-65` 在 `__ELF__` 下 `_C_LABEL` 恒等，均与源码一致。207 行两种反向情形（前缀不匹配 → 未定义符号；所有引用一起改名 → 函数落低）与源码机制一致。
- **帧指针边界 → CLOSED**：229 行新增“清零只保证入口起点的终止；未启用帧指针时 Rust 侧不保证链深度可回溯”。与 `os/arch/src/arch/stacktrace.rs` 的“frame pointers enabled / cannot unwind”注释及 `os/target/x86_64-unknown-none/release/kernel` 里 `kmain` 序言不保存 `%rbp` 的产物读数一致。
- **§3.4 指针对象拆分 → CLOSED**：567 行把“`kinfo` 在高地址可访问”拆成“跳转传入的指针”与“`kmain` 消费的副本”；生产 `kmain` 第一条语句遮蔽为全局副本（`os/kernel/src/lib.rs:1687-1688`），两个生产 `jump_to_kmain` 调用点（`lib.rs:232`、`:375`）都传 `kinfo.kern_stack_top`。
- **三处节选头 → CLOSED**：556（trait）、907（aarch64）、932（riscv64）三处块头与各自保留的块内容对应；trait 块只含签名，aarch64/riscv64 块只含 asm 与 delta 计算，源码侧无对应缺失。
- **`place_modules` 语法 → CLOSED**：1060 行改为“源字节直接拷进 bump 页；不做整页清零，只把文件字节之后的尾部余量清零，名字槽从定长字节池拷进一页物理名字池”。与 `os/kernel-image/src/bootface_common.rs:245-259` 的 `copy_nonoverlapping` + `write_bytes(dest+len, 0, pages*4096-len)` + 16B 名字槽拷贝一致；名字池由 `bootface.rs:276` / `bootface_a64.rs:253` 各分配一页。
- **模块文档指针 → CLOSED**：1060 行把视图约束指到 `bootface.rs` 的模块文档；`os/kernel-image/src/bootface.rs:42-51` 的 Execution-view invariant 确有这段说明。
- **`NoLoadSegments` 测试名 → CLOSED**：498 行改为 `test_compute_kernel_layout_rejects_image_without_load_segments`，与 `os/boot-shim/src/loader.rs:604` 的实际测试名一致。
- **aarch64 `ldr` 视图 → CLOSED**：927 行改为“`ldr` 取链接期绝对值，literal 的读取位置随视图；低地址视图读出的仍是链接期绝对值，目标正确”。与 `os/kernel/src/arch/aarch64/higher_half.rs` 的 `ldr x1, ={kmain}` + `br x1` 及绝对字面量池语义一致。

### 新准入

本轮未发现新的 P0/P1/P2。留作 P3 观察、不计入准入：

- aarch64 节选头（907 行）写“注释未摘录”，但块内保留了源码行尾注释 `// defensive barrier; enable() already synchronized TTBR/SCTLR`；属于摘录说明的边界措辞。
- 157 行“名单里的库对象（`klib.o`、`io_inb.o`、`assert.o` 等）”把内核对象 `klib.o`/`io_inb.o` 与库对象并列；其“通常还会以普通对象形式链接一次”本身成立。
- 207 行反向两分法对 `kmain` 这个特例仍不覆盖（`proto.h` 已预置前缀且 `main.c` 同时在 `SRCS`，把 `main.o` 加入名单会得到高层原定义 + 低侧双重前缀死副本）；154 行已解释宏改名，属未展开的边界，不是与源码矛盾。
- 567 行“`.bss` 副本”仍是源码注释级 shorthand：产物中 `KERNEL_INFO` 与 `BOOT_MODULE_STORE` 在 `.data`，`RESERVED_REGION_STORE`/`MEMMAP_REGION_STORE`/`PLATFORM_SOURCE_STORE` 在 `.bss`（沿用 #1 的 O-5，非本轮新引入）。

**认证 #2 新准入 = 0**
