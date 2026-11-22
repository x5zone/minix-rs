# D02 R2 认证事实回读（02-higher-half-kernel）

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 取证时 sha256 前缀：`f2be56a4`；行数 1280
- 回读范围：用 `git diff -U0` 定位的本轮 17 行改动及邻句（doc:8、58、126、207、261、265、320、404、419、454、457、531、757、796、874、1124、1147）。未重扫全文，未读历史评审与其他面板。
- 只读声明：仓库文件未改动；全部验证实验在 `/tmp` 的临时目录进行。

## 一、工具证据

### 1. doc-citation-check.py

```text
$ python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
rewrite-notes/01-stage-kernel/02-higher-half-kernel.md: blocks=32 checked=1 failed=0 skipped=31 warned=0
RC=0
```

覆盖说明（不属于文档事实缺陷）：工具第 43 行的引文标注正则只匹配 `//` 开头的行；本篇多数块用 `/* 代码引用 … */` 或 `/* 节选（…）… */`，因此被当作“无标注”跳过。对照实验（`/tmp/citetest`）用一个 `/* 代码引用，… */` 标注块写入源码里不存在的代码行，工具仍输出 `checked=0 … RC=0`。所以这次的 `failed=0` 只对 597–610 那一块构成证据；本轮四处“注释有改写”声明所在块的逐行对账由本次人工核对承担（见 RD2-CERT-06）。该工具覆盖问题不计入认证 #2 新准入。

### 2. anchor-resolve.sh --check

```text
$ bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
anchors=55 resolved=55 zero-def=0 multi-def=0
RC=0
```

### 3. doc-structure-check.py

```text
$ python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析
RC=0
```

### 4. doc-ref-check.sh

```text
$ AWK=gawk bash tools/doc-ref-check.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/02-higher-half-kernel.md 内部章节引用均可解析
REPORT_RC=0
$ AWK=gawk bash tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/02-higher-half-kernel.md 内部章节引用均可解析
CHECK_RC=0
```

## 二、逐条回读

### RD2-CERT-01 — 修点 1：LOADADDR(p_vaddr) 与 p_paddr 的射程声明 — CLOSED

正文位置：doc:58（§1.2）、doc:404（§3.2）。

依据：

1. `minix3/sys/arch/x86/include/loadfile_machdep.h:40` 是 `#define LOADADDR(a) ((((u_long)(a)) & 0x07ffffff) + offset)`。`0x07ffffff` 是低 27 位掩码；`0xF0400000 & 0x07ffffff = 0x00400000`。
2. 树内装载器确实把 `p_vaddr` 送进 `LOADADDR`：`minix3/sys/lib/libsa/loadfile_elf32.c:326` 的 `marks[MARK_DATA] = LOADADDR(phdr[i].p_vaddr)`、`:336` 的 `READ(fd, phdr[i].p_vaddr, …)`、`:363` 的 `BZERO(phdr[i].p_vaddr + phdr[i].p_filesz, …)`。
3. `offset` 在 i386 引导路径上为 0：`minix3/sys/arch/i386/stand/boot/boot2.c:260` 调 `exec_netbsd(filename, 0, …)`，`minix3/sys/arch/i386/stand/lib/exec.c:299` 把它写进 `marks[MARK_START]`，`loadfile_elf32.c:268` 再读成 `offset`。于是 `LOADADDR(p_vaddr)` 在这里退化为低 27 位。
4. 树内 i386 内核装载路径没有第二个读 `p_paddr` 的 C 装载器：`grep -rn "p_paddr" minix3 --include=*.c --include=*.h --include=*.S` 在装载相关代码里只命中 ELF 结构体定义、libkvm 的同名无关字段与 `usr.sbin/installboot/arch/pmax.c`（另一架构的 installboot）。`p_paddr` 属于外部 multiboot 装载器（GRUB 一类）的读法，与正文的射程声明一致。

一处描述偏差（不是文档事实错误）：本轮修点文字写“§1.2/§4.2”，但正文的射程声明实际在 §1.2（doc:58）与 §3.2（doc:404）；§4.2 讲的是 Rust 侧按 `p_paddr` 摆放，没有对 C 侧装载者读法的断言。

### RD2-CERT-02 — 修点 2：GNU ld 的 LMA 重叠与 `.bss` 尾段 — CLOSED

正文位置：doc:126。

依据：

1. 用宿主 GNU ld 2.42 复现“两个 PROGBITS 输出段同 LMA”：

```text
$ ld -T bad3.ld t.o -o out3
ld: section .s2 LMA [0000000000001000,0000000000001003] overlaps section .s1 LMA [0000000000001000,0000000000001003]
（out3 未产出）
```

2. 用仓库真实 `kernel.lds` 的副本，把五处 `AT(ADDR(...) - _kern_offset)` 全改成 `AT(_kern_phys_base)`，配两个哑对象链接：

```text
ld: section .text LMA [00400000,00400005] overlaps section .unpaged_text LMA [00400000,00400009]
ld: section .data LMA [00400000,00400003] overlaps section .text LMA [00400000,00400005]
（未产出 ELF）
```

结论“现行 GNU ld 对 LMA 重叠报错并拒绝产出”在该场景成立。

3. `.bss` 是 `.data` 所在 RW 段的 NOBITS 尾段：同一份真实 `kernel.lds` 配哑对象链接后，`readelf -lW` 给出：

```text
LOAD           0x003000 0xf0402000 0x00402000 0x00004 0x01420 RW  0x1000
Section to Segment mapping:
  02     .data .bss
```

`.data` 是 PROGBITS（FileSiz 4），`.bss` 是 NOBITS 尾段（MemSiz 0x1420），两者同属一个 RW 段；`kernel.lds:30,32` 的 `AT(ADDR - _kern_offset)` 相同，虚拟地址连续，装载地址也连续。

精度备注（不影响判定）：隔离实验里只有 NOBITS 段与 PROGBITS 段同 LMA 时，ld 2.42 不报错；本篇场景的报错由多个 PROGBITS 段同 LMA 保证。文档句子在本场景内成立，不是对“任意 NOBITS 重叠”的普适断言。

### RD2-CERT-03 — 修点 3：`pg_utils.o` 移出改名名单 — CLOSED

正文位置：doc:207。

依据：

1. 名单在 `minix3/minix/kernel/arch/i386/Makefile.inc:51-52` 的 `.for unpaged_obj in head.o pre_init.o … pg_utils.o …`；同一文件 `:76` 的 `SRCS+= … pg_utils.c` 说明普通 `pg_utils.o` 仍会入链。
2. 五个函数都定义在 `pg_utils.c`：`pg_identity:162`、`pg_mapkernel:186`、`vm_enable_paging:204`、`pg_load:247`、`pg_clear:254`；调用点在 `pre_init.c:230-234`，依次调用这五个。
3. 直接实验（`/tmp/preinit_test`）：把 `pre_init.o` 用 `objcopy --prefix-symbols=__k_unpaged_` 改名，配普通 `pg_utils.o` 做最终链接：

```text
ld: … undefined reference to `__k_unpaged_pg_clear'
ld: … undefined reference to `__k_unpaged_pg_mapkernel'
```

`nm -u unpaged_pre_init.o` 同样显示这两项未定义。这与正文“链接期会得到 `__k_unpaged_pg_*` 未定义符号”一致，也排除了“函数落高半、分页前执行”的旧判断。

4. `pre_init.c` 不在任何 `SRCS` 里：`grep -rn "pre_init" minix3 --include=Makefile --include=Makefile.inc --include=*.mk` 只命中两个架构 Makefile.inc 的注释与 `.for` 块，没有源码清单项；正文“去掉名单项会连唯一生成规则一起消失”成立。

非准入精度备注：未定义符号集合里还有 `__k_unpaged_vm_enable_paging`（不以 `pg_` 开头），正文用 `__k_unpaged_pg_*` 指代这一族，属于示例式简写，不是错误。

### RD2-CERT-04 — 修点 4：迭代器 `continue` 与未检查加法 — CLOSED

正文位置：doc:454 与节选 doc:456-494。

依据：

1. `os/libs/minix-elf/src/lib.rs:355-386` 的 `next()`：`checked_mul`/`checked_add` 溢出时执行 `self.index += 1; continue`；`parse_phdr` 返回 `Err(_)` 时 `continue`；`p_type != PT_LOAD` 时 `continue`；`seg_end > image.len()` 时 `continue`。正文“做类型过滤与范围检查，命中这些条件时 `continue`”逐条对上。
2. 未检查加法：`new()` 的 `:333-337` 是 `ehdr.e_phoff as usize + (ehdr.e_phnum as usize) * (ehdr.e_phentsize as usize)`；`parse_phdr()` 的 `:250` 是 `offset + (entsize as usize) > image.len()`。两处都是裸 `+`。
3. 畸形输入的行为：宿主测试默认走 dev profile（溢出检查开启），`e_phoff` 接近 `usize::MAX` 时 `new()` 的加法溢出即 panic；release profile 未显式设 `overflow-checks`（`os/Cargo.toml:301-305` 只有 `panic = "abort"`），同一输入会先回绕，随后在 `&image[offset..]` 处因下标越界 panic。两种构建的可观察结果都是 panic 而不是 `Err`，正文的结论成立；精确表述应理解为“触发未检查加法溢出的那类畸形输入”。

### RD2-CERT-05 — 修点 5：`syscall_entry` 的消费者 — CLOSED

正文位置：doc:531。

依据：

1. 生产入口不读字段：`os/kernel/src/lib.rs:2274-2276` 用 `minix_arch::{install_trap_stubs, syscall_entry_va}`，`trap.configure_syscall(syscall_entry_va())`；同一文件 `:2268` 的注释写明 `KernelInfo.syscall_entry is reference-only boot metadata`。
2. `validate_requiring_sources` 的 strict 档断言相等：`os/libs/minix-boot/src/kernel_info.rs:452-460` 的 `assert_eq!(self.syscall_entry.0, self.kern_virt_base.0, …)`，由 `:264-265` 的 `validate_at(ValidationLevel::Strict)` 进入。
3. `check_boot_postconditions` 断言落在镜像内：同文件 `:576-590`，先算 `entry_off = syscall_entry.checked_sub(kern_virt_base)`，再 `assert!(entry_off < kern_size, …)`。
4. `handoff_v1_write` 的 `debug_assert` 断言相等：`os/libs/minix-boot/src/handoff_v1_write.rs:143` 的 `debug_assert_eq!(report.syscall_entry.0, report.kern_virt_base.0);`。
5. 全仓 `configure_syscall` 调用点共五处：生产 `os/kernel/src/lib.rs:2276` 与三个测试内核（`test-protection`、`test-proc-init`、`test-timer-irq`）都传 `syscall_entry_va()`；只有 `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-riscv64/src/main.rs:148` 传 `info.syscall_entry`。正文“全仓唯一把它当入口传入 `configure_syscall` 的读者是 test-protection-riscv64 载体”成立。
6. “线协议 v2 计划删除该槽位（设计批 D2-03）”是设计计划陈述；本轮只核对到字段仍在线协议里（`handoff_v1_write.rs:178,189` 的 `SYSCALL_ENTRY` 偏移与写入），与“计划删除”不冲突。

### RD2-CERT-06 — 修点 6：四处“注释有改写”声明 — CLOSED

正文位置：doc:419、457、796、1124。

依据：

1. `grep -n "注释有改写"` 恰好四行，与修点数目一致；`grep -n "注释有删改"` 为零。
2. 四处声明与实际改写的对应：
   - doc:419（`Elf64Ehdr`/`Elf64Phdr`/`LoadSegment`）：源文件 `os/libs/minix-elf/src/lib.rs:80-140` 用 `///` 文档注释，节选改成行内 `//` 并压缩措辞，声明属实。
   - doc:457（`impl Iterator for SegmentIter`）：源文件 `:369` 的注释是 `// parse_phdr already validated bounds in new(), but be defensive`，节选改成 `// 防御性解析：表项越界/太小时跳过`；声明属实。
   - doc:796（`compute_kernel_layout`）：源文件 `os/boot-shim/src/loader.rs:186-188` 有一段三行注释，节选没有保留；声明“注释有改写”把该块标为非逐字引文，读者不会被误导为逐字引文。
   - doc:1124（`kmain_verify`）：源文件 `os/kernel/src/lib.rs:2147-2149` 的两行注释被压缩成两行架构分述，声明属实。
3. 其余改动了注释的节选用的是“注释有删节”（doc:276、677、702、881 等）或“省略号处为…”，声明类型与修改类型一致。

非准入精度备注：`compute_kernel_layout` 处的源注释是“未保留”而非“改写”，措辞上更贴近“删节”；但 D-045 要求的显式声明（标为非逐字引文）已满足。

### RD2-CERT-07 — 修点 7：五步建表与“只切栈不跳” — CLOSED

正文位置：doc:261、265。

依据：

1. `minix3/minix/kernel/arch/i386/pre_init.c:230-234`：`pg_clear(); pg_identity(&kinfo); kinfo.freepde_start = pg_mapkernel(); pg_load(); vm_enable_paging();`，五步依次执行，`pre_init` 返回后 `head.S` 才换栈。正文“由 `pre_init` 的五步建表在此之前完成”成立。
2. `minix3/minix/kernel/arch/i386/head.S:87-91`：`call _C_LABEL(kmain)` 之后是 `/* not reached */ hang: jmp hang`。只换栈不跳时 PC 落到 `hang` 循环；恒等映射仍在，取指正常，主流程停住，VM 服务不会运行，因此原先设想的“恒等映射被移除后再出页错误”不可达。正文“直接症状是停机，不是页错误”成立。

### RD2-CERT-08 — 修点 8：段间覆盖与源/目标重叠 — CLOSED

正文位置：doc:874。

依据：

1. 段间重叠：`os/boot-shim/src/loader.rs:289-291` 的 `first_overlapping_pair(&segments)` 命中即返回 `ElfError::InvalidSegment`；`:348-372` 的谓词是 `a.paddr < b_end && b.paddr < a_end`（半开区间，相邻不算重叠），其中 `a_end = paddr + memsz`。
2. 源/目标重叠：`:297-302` 的 `first_source_destination_overlap(src_base, &segments)` 命中即 `panic!("boot-shim: kernel ELF segment … source overlaps segment … destination LMA")`；`:321-345` 的射程是“每个段的目标 `[paddr, paddr+memsz)`”对“每个段的文件字节 `[elf_base+offset, +filesz)`”，覆盖 BSS 清零段。
3. “后一个段覆盖前一个段的字节是数据损坏，不是 `copy_from_nonoverlapping` 的 UB”：该函数的 `unsafe` 契约是单次调用的 `src`/`dest` 不相交；两次独立调用写重叠的目标是数据损坏，不是该函数触发的 UB。源与目标落在同一地址区间时才是 UB，而这一情形由 `first_source_destination_overlap` 的 panic 先行拦下。正文的三段因果关系成立。

### RD2-CERT-09 — 修点 9：`plan_kernel_mapping` 的 panic 位置、L9 射程、ARM 证据状态 — CLOSED

正文位置：doc:757（§4.1）、doc:1147（§5.3）、doc:320（§2.5）。

依据：

1. panic 位置：`os/kernel/src/lib.rs:925` 的 `arch_boot_impl` 在 `:946-948` 调 `plan_boot_pages::<A>(…)` 并 `.unwrap_or_else(|e| panic!("arch_boot: page-size policy: {e}"))`；`os/kernel/src/boot_policy.rs:206-214` 的 `plan_boot_pages` 再调 `plan_kernel_mapping`；`:125-133` 的选择阶梯在两个粒度都不满足时返回 `Err(NoKernelPageSize)`，`:51` 的显示文本是 `no kernel page size satisfies the layout`。所以“规划层 `plan_kernel_mapping` 在 `arch_boot` 里 panic”成立：规划函数本身返回错误，panic 发生在 `arch_boot_impl` 的解包处，位置表述正确。
2. L9 射程：`os/kernel-image/check-layout.sh:252-260` 的 L9 整段包在 `if [ "$arch" = "riscv64" ]` 里；doc:1147 的“在 L9 覆盖的 riscv64 分支上，任一处单独改动都会让其中一条变红”把“任一处”限定在 riscv64，aarch64 只留 `os/kernel-image/aarch64.ld:116` 的 `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000, …)`（上界断言，无等值对账），与 §5.4 M10 的覆盖状态一致。
3. ARM 结论限定：doc:320 末尾“i386 侧已按 §2.4 证据落地，ARM 侧按同一判据、改名名单与 `.data` 收段证据待补”确实把 ARM 侧留在待补状态，没有把 i386 的落段证据外推到 ARM；§2.4 的三条证据（doc:302-306）也都只指向 i386 的 `mpx.S`/`kernel.lds`。

### RD2-CERT-10 — 附带回读：本轮 diff 第 8 行（前置知识按腿起点） — CLOSED

正文位置：doc:8。这一行不在九条修点清单里，但属于本轮 `git diff` 改动，按“本轮改动段”一并回读。

依据：

1. v1 send 交接前分页已开：`os/kernel/src/lib.rs:493` 的 `prepare_kernel_image_v1` 调 `build_bootstrap_root_and_enable`，`:523-531` 的 `enter_kernel_image_v1` 随后才 `jump_kernel_image_v1`；shim 的恒等窗口在 `windows` 表里建立，跳到镜像高半入口前 shim 仍在低恒等视图。
2. riscv64 自引导 Step 3 才开分页：`os/kernel/src/lib.rs:1399-1401` 的 `// Step 3: Enable paging.` 与 `paging.enable()`；Step 1 只建恒等窗口，Step 2 建高半映射。
3. aarch64 direct-core 低地址进入、物理视图；inline 测试载体低地址视图：与 §4.4 的四路径表（doc:1029-1034）以及 `arch_boot_impl` 的 Step 顺序一致。

## 三、非准入观察（不计入新准入）

1. 引文门覆盖：`doc-citation-check.py` 的标注正则只认 `//` 开头（工具第 43 行），本篇 32 个块里 31 个用 `/* … */`，被静默跳过；对照实验证明这类块即使含源码不存在的行也判 PASS。建议工作流侧决定是否扩展工具或统一标注格式；本轮四处“注释有改写”块的逐行对账由 RD2-CERT-06 的人工核对承担。
2. `__k_unpaged_pg_*` 是族名简写，实际未定义集合还含 `__k_unpaged_vm_enable_paging`（见 RD2-CERT-03）。
3. `compute_kernel_layout` 的源注释是未保留，声明写作“注释有改写”；D-045 的显式声明目的已达到（见 RD2-CERT-06）。
4. 修点文字里的“§4.2”在正文没有对应的 boot monitor 射程句；正文实际的射程声明在 §1.2 与 §3.2，两处事实正确（见 RD2-CERT-01）。

## 四、结论

九条修点全部 CLOSED；附带回读的 diff 第 8 行 CLOSED。没有新的 P0/P1/P2 文档事实缺陷。

认证 #2 新准入 = 0
