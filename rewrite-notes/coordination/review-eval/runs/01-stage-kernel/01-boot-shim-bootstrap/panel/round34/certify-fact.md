# R34 certify 事实回读（certify-fact）

## 一、基线复核

任务给的基线是 3565 行、sha256 前缀 `d909c236a3736366`。实测工作区文件与它对不上，先说清楚原因：

| 对象 | 行数 | sha256 |
|---|---|---|
| 任务基线（= `git show HEAD:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`） | 3565 | `d909c236a373636678e66b13c0ad5ebde769ead738f09d81a846803647fcb98c` |
| 当前工作区（R34 改动已应用，未提交） | 3579 | `b3156fc4f0a4ff00f7fc294d4ddd660d9dda6af2d73b8c7fa032bd9e8eb18034` |

`git diff --stat` 为 `47 insertions(+), 33 deletions(-)`，净增 14 行，正好解释 3565 → 3579。因此：任务给的 3565/`d909c236` 是**改动前**基线，当前工作区是**改动后**状态；本文所有行号按任务要求取改动后（当前工作区）文件里的行号，逐条只读改动段及邻句。

## 二、四门机械检查读数

在仓库根 `/home/xzhao/github/minix-rs` 执行，目标文件 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`：

| 门 | 命令 | 读数 | 退出码 |
|---|---|---|---|
| 引文机械对账 | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `blocks=69 checked=29 failed=0 skipped=39 warned=1` | 0 |
| 锚点解析 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=105 resolved=105 zero-def=0 multi-def=0` | 0 |
| 表结构与内部链接 | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `1 个文件表结构与内部 .md 链接均可解析` | 0 |
| 内部章节引用 | `tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `内部章节引用均可解析` | 0 |

引文门唯一警告是 `[WARN] block 1316-1328 节选`，落在 §2.6 的 `earm` `overlaps` 引文块，不在本轮改动段内；`failed=0`。

## 三、逐条核对（1–15）

### 1. §2.3 的 `arch=` 读者面 —— CLOSED

改后原文（doc:365）：「`arch=` 在内核侧只有写点、没有读点（内核的架构身份来自编译期），但用户态 RAMDS 的启动脚本会经 `sysenv arch` 读它（`minix3/minix/drivers/storage/ramdisk/rc:9`、`:46`）」；doc:457 补「走的是与 `rootdevname` 同形的 `PMGETPARAM` 路径」。

源码核对：

- `minix3/minix/drivers/storage/ramdisk/rc:9`：`if [ X\`sysenv arch\` = Xi386 ]`；同文件 `:46`：`if [ X\`sysenv arch\` = Xearm ]`。两处引用行号准确。
- `minix3/minix/commands/sysenv/sysenv.c:59`：`if (svrctl(PMGETPARAM, &sysgetenv) == -1) {`，即所述 `PMGETPARAM` 路径的发起端。
- `minix3/minix/servers/pm/main.c:169`：`if ((s=sys_getmonparams(monitor_params, sizeof(monitor_params))) != OK)`，PM 启动期缓存 `param_buf` 的起点。
- `minix3/minix/servers/pm/misc.c:374`：`if (p >= local_params && (val_start = find_param(search_key)) == NULL)`；`minix3/minix/servers/pm/utility.c:57-70` 的 `find_param` 从头扫描、返回第一个匹配。
- 内核侧读点核查：`grep -rn "ARCHVARNAME" minix3/minix` 只有 `arch/i386/pre_init.c:149` 与 `arch/earm/pre_init.c:274` 两处 `mb_set_param` 写点；`env_get` 的调用点（`kernel/main.c:133/414/421/443/448/456/464`、`kernel/clock.c:56`、`i386/memory.c:781/918`）没有 `arch`。
- `GET_MONPARAMS` 的载荷确为 `kinfo.param_buf` 全缓冲：`minix3/minix/kernel/system/do_getinfo.c:143-146`。

改后句子与源码一致。

### 2. §2.3 的 `xboard=2` 例子前提 —— CLOSED

改后原文（doc:464）：「例如 `param_buf` 里先有 `xboard=2`（它不是首条记录，前面还有别的键），内核再写 `board=` 时，中段 `board=2` 会被删掉，后一条 `arch=i386` 的前半截被补进原位，留下伪记录 `xarch=i386`」。

源码核对（`minix3/minix/kernel/arch/i386/pre_init.c`）：

- `:47-59` 的删除扫描：`while (*p)` 外层逐记录走，命中条件 `strncmp(p, name, namelen) == 0 && p[namelen] == '='`，不要求 `p` 位于记录起点。
- `:61-73` 的追加格式：每条记录写 `name=value` 后只留一个 NUL 分隔（`p[namelen+valuelen+1]=0`、`p[namelen+valuelen+2]=0`，下一次追加从后一个 NUL 起写，实际分隔为单 NUL）。
- 扫描推进是 `while (*p++) ; p++;`（`:56-58`）：越过一条记录后 `p` 停在“下一条记录起点 + 1”。因此只要 `xboard=2` **前面还有一条记录**，扫到它时 `p` 正落在其第二个字符 `b` 上，`strncmp("board=2","board",5)==0` 且 `p[5]=='='`，中段命中成立；若 `xboard=2` 是首条，`p` 从 0 看到 `x`，不会中段命中。新增前提「它不是首条记录，前面还有别的键」是必要且正确的。
- 写入次序：`:149` 先写 `arch=i386`，`:150` 再写内核的 `board=` 值（doc 例中即 `board=intel`）；所以中段删除发生时被左移一字节补进原位的下一条记录确实是 `arch=i386`，得到 `xarch=i386`。`get_value`（`kernel/main.c:492-499`）没有那次多余的 `p++`，从头按记录边界扫描，仍会找到内核随后追加的 `board=`，故「不改写 `machine.board_id`」成立。

### 3. 两处引文块省略标记 —— CLOSED

- `earm/head.S` 块（doc:1242-1243）新增「代码引用（节选，止于 `bx r2`；其后的 `hang` 段不引）」。源码 `minix3/minix/kernel/arch/earm/head.S:46` 是 `bx r2`，`:48-50` 是 `/* not reached */`、`hang:`、`b hang`。所述省略边界准确。
- `i386/kernel.lds` 块（doc:1787）把原来的空行改成 `...`。源码 `minix3/minix/kernel/arch/i386/kernel.lds` 第 6 行是 `_kern_offset = ...`，第 8-10 行是三条 `__k_unpaged__kern_offset/vir_base/phys_base` 别名，第 11 行空行，第 12 行 `SECTIONS`。`...` 盖住 8-11 行，标记是诚实的；块内另外两个 `...`（doc:1800、1802）分别盖住 24-28 行（`usermapped_*`）与 30-31 行（`.data`、对齐行），同样成立。
- 备注（不计新准入）：块头写「保留段划分与偏移量相关的行」，但被 `...` 省掉的 8-10 行恰是带 `kern_offset/vir_base/phys_base` 名字的别名；块头没有声称穷举，故不是错句，只是取景说明不够精确。

### 4. `boot_image image[NR_BOOT_PROCS]` —— CLOSED

改后原文（doc:2523）：「顺序按 C 的 boot image 表（`kernel/table.c` 里 `struct boot_image image[NR_BOOT_PROCS]` 的用户态段）排列」。

源码 `minix3/minix/kernel/table.c:44`：`struct boot_image image[NR_BOOT_PROCS] = {`，与改后写法逐字一致；用户态段（`ds` 到 `init`）共 12 项，与 doc 列出的 12 个名字一致。

### 5. `protect.c:33-39` —— CLOSED

改后原文（doc:1072）：「它的 `vir2phys` 解析到 `protect.c:33-39` 的减 `_kern_offset` 版本」。

源码 `minix3/minix/kernel/arch/i386/protect.c:33-39`：

```c
phys_bytes vir2phys(void *vir)
{
	extern char _kern_vir_base, _kern_phys_base;	/* in kernel.lds */
	u32_t offset = (vir_bytes) &_kern_vir_base -
		(vir_bytes) &_kern_phys_base;
	return (phys_bytes)vir - offset;
}
```

函数起止行与行为都对得上；`pre_init.c:24` 的恒等版是另一份 unpaged 副本。

### 6. `kinfo` 落在 `.unpaged_bss` —— CLOSED

改后原文（doc:499）：「`kinfo` 作为全局对象落在 `.unpaged_bss`（`pre_init.o` 的 `.bss` 输入节被 `kernel.lds:19` 收进这个输出节）」。

源码核对：

- `minix3/minix/kernel/arch/i386/pre_init.c:20`：`kinfo_t kinfo;`，无初值，属 `.bss`。
- `minix3/minix/kernel/arch/i386/kernel.lds:19`：`.unpaged_bss  ALIGN(4096) : { unpaged_*.o(.bss COMMON) }`。
- `minix3/minix/kernel/arch/i386/Makefile.inc:51-54` 把 `pre_init.o` 列入 `unpaged_obj` 循环；`:69` 生成 `unpaged_pre_init.o`；`:83` 只有 `${UNPAGED_OBJS}` 进 `OBJS.kernel`，未改名的 `pre_init.o` 不进链接。因此引导期实际只有 `__k_unpaged_kinfo` 这一份，落在 `.unpaged_bss`，零初值前提成立。

### 7. `VM_BOOT_HANDOFF_VERSION = 9` 与 `BOOT_HANDOFF_V1_VERSION` 的区分 —— CLOSED

改后原文（doc:740）：「v9（`VmBootHandoff` 的 `VM_BOOT_HANDOFF_VERSION = 9`；它与 §3.4.3 的 arena 信封版本 `BOOT_HANDOFF_V1_VERSION` 是两个独立计数器）已删除恒空的 `kern_ident`、`kern_ident_count` 与 `VM_BOOT_HANDOFF_MAX_IDENT`」。

源码核对：

- `os/libs/minix-types/src/types/boot.rs:96-100`：v9 注释明确写「`kern_ident` / `kern_ident_count` / `VM_BOOT_HANDOFF_MAX_IDENT` removed…The field had been always empty since v8」，紧接着 `pub const VM_BOOT_HANDOFF_VERSION: u32 = 9;`。
- `os/libs/minix-boot/src/handoff_v1.rs:282-288`：v9 注释讲的是 arena 里 6144 B 语义工作分区被删、arena 从九页缩到八页，`pub const BOOT_HANDOFF_V1_VERSION: u32 = 9;`。两个常量的变更内容不同，确系独立计数器；两者数值同为 9 不构成同一版本。
- `grep -rn "kern_ident\|VM_BOOT_HANDOFF_MAX_IDENT" os --include=*.rs` 只剩五处注释性提及（`os/kernel/src/globals.rs`、`os/arch/src/arch/paging.rs`、`os/libs/minix-boot/src/kernel_info.rs`、`os/libs/minix-types/src/types/boot.rs` 的版本史、`os/servers/vm/src/boot.rs`），字段与常量已不存在，删除说法成立。

### 8. 五处 Rust 引文重排与 UEFI 块头声明 —— CLOSED

五处引文与源码逐字比对（只比排版，不比省略段）：

| doc 行 | 源码位置 | 核对结果 |
|---|---|---|
| 2130-2133 | `os/arch/src/arch/pt_alloc.rs:129-132` | 四行多行 `assert!` 与源码逐字一致（doc 缩进 4/8 空格，源码同） |
| 2607-2608 | `os/boot-shim/src/opensbi_helpers.rs:204-205` | `let slice =` + `unsafe { slice::from_raw_parts(...) };` 逐字一致 |
| 2699-2706 | `os/kernel/src/lib.rs:867-882` | 三个多行 `assert!`（phys/virt/stack_top）逐字一致；前面的 `assert!(ks > 0, ...)` 单行也与 `:864` 一致 |
| 2881-2884 | `os/kernel/src/lib.rs:1245-1248` | `raw_end.checked_add(...).expect(...) & !(DEV_PIECE - 1)` 的排版逐字一致 |
| 3411-3412 | `os/boot-shim/src/main.rs:110-111` | `let message =` + `core::str::from_utf8(...)` 逐字一致 |

UEFI 块头（doc:2307）新增「注释与排版有改写/删节」，与源码 `os/boot-shim/src/uefi_helpers.rs:953-1142` 比对成立：doc 把源码的 `expect("...")` 长消息压成 `expect(...)`、把注释改成中文改述、把 `serializer.write_*` 多行调用合并，确属排版/注释层改写。块头列出的未展开段也都能在源码找到对应：参数统计 `println`（`:1039-1044`）、`system_table`/`boot_services`/`ops` 构造（`:1078-1088`）、arena 的 `assert_range_clear_of_kernel`（`:1049-1055`）、`NeedBiggerBuffer` 扩容断言分支（`:1130-1135`）、`dumpdtb_blob` 的 `#[cfg]` 构造（`:965-968`）；文末 doc:2402 另交代了配对循环内的 `preflight_dtb`（`:1099-1112`）与 EBS 后 startup 写入段（`:1144-1179`）。保留项的顺序与源码一致（entry 打印 → shim 窗口 → ELF 读取/布局 → 预留跨度 → root/bump → 段拷贝 → 模块/平台源/参数 → arena/序列化 → EBS 配对 → 跳转）。

备注（不计新准入）：源码 `uefi_helpers.rs:976-979` 的 `assert_eq!(layout.entry_point, layout.kern_virt_base.0, ...)` 在引文里被省掉，但块头的「未展开段」清单没有点它的名。块头有「节选」总限定，不构成错句；该断言本身在 doc:2674 的表格里有交代，建议下一轮补进清单。

### 9. 三个镜像入口栈对齐的来源 —— CLOSED

改后原文（doc:2983）：「三个内核镜像的引导栈顶由链接脚本的 `. = ALIGN(16)` 保证 16 字节对齐；x86_64 的入口 asm 另显式 `and rsp, -16`；`jump_to_kmain` 三架构均再对齐一次，x86_64 另按 psABI 做 `sub rsp, 8` 调整。」

源码核对：

- 三份链接脚本都在 `kernel_boot_stack_bottom` 之前 `. = ALIGN(16)`：`os/kernel-image/x86_64.ld:86`、`aarch64.ld:85`、`riscv64.ld:93`；随后 `. += 0x10000`，栈顶同样 16 字节对齐。
- `os/kernel-image/src/main.rs:162-163`：`lea rsp, [rip + kernel_boot_stack_top]`、`and rsp, -16`；aarch64 入口（`:216-219`）与 riscv64 入口（`:296` 起的 `_start`）只 `mov sp, ...`，没有 `and`，与「x86_64 另显式」相符。
- 三份 `jump_to_kmain` 都再对齐一次：`os/kernel/src/arch/x86_64/higher_half.rs:45-49`（`and rsp, -16` 后 `sub rsp, 8`）、`aarch64/higher_half.rs:41-45`（经 `x2` 做 `and`，SP 不能直接作 `and` 目的）、`riscv64/higher_half.rs:100-102`（`li t0, -16; and sp, sp, t0`）。

### 10. §3.8 的 `VmBootHandoff.modules` 与 C `kinfo.module_list[]` 关系 —— CLOSED

改后原文（doc:2199-2200 引文注释）：「the handoff module list (`VmBootHandoff.modules`, built by `build_vm_handoff`) excludes VM as a Rust-side choice — C's `kinfo.module_list[]` keeps VM and zeroes its entry after reclaim.」；doc:2218 中文同步改为「C 的 `kinfo.module_list[]` 本身含 VM，VM 条目在回收后被置零，见 §2.4.2」。

源码核对：

- `os/kernel/src/vm_handoff.rs:31-34` 与 doc 引文逐字一致；`build_vm_handoff` 的模块循环在 `:537-540` 以 `i == vm_module_idx` 跳过 VM；扣除记录仍在 `classify` 里保留 VM 的 blob（`:424-428` 的注释与 `for_each_live_bootstrap` 路径）。
- C 侧 `minix3/minix/kernel/arch/i386/protect.c:449-451`：`/* Free VM blob that was just copied into existence. */ add_memmap(&kinfo, mod->mod_start, ...); mod->mod_end = mod->mod_start = 0;`；`earm/protect.c:176-178` 同形。即 C 的 `module_list[]` 含 VM，回收后把该条置零。
- doc:749 的「还 VM 镜像」行与 doc:775 的置零说明与上述 C 代码一致，doc:2218 的「见 §2.4.2」指向有效。

### 11. §3.4.2 的 `BOARD`/`board` 审计 —— REOPEN（见 FINDING RD34-CERT-FACT-01）

改后原文（doc:1710）：「仓内消费者核查：Rust 侧 `os/kernel` 与 `minix-boot` 对 `BOARD`/`board` 两种拼写检索均零命中，无读者受害；C 内核的 `get_value` 读 `board=` 属 C 侧消费者，不在本审计范围。」

核对结果：

- 「无读者受害」这一结论成立：四条生产调用 `normalize_param_buf` 全部传 `None`（`os/boot-shim/src/uefi_helpers.rs:313`、`os/boot-shim/src/opensbi_helpers.rs:338`、`os/kernel-image/src/bootface.rs:334`、`os/kernel-image/src/bootface_a64.rs:308`），Rust 侧没有按 `BOARD`/`board` 键从 `param_buf` 取值的 reader。
- 但「两种拼写检索均零命中」作为证据陈述不成立：`os/libs/minix-boot/src/boot_params.rs:137` 有 `for &byte in b"BOARD="`（有来源时生成 `BOARD=` 记录），`:174` 有 `key.eq_ignore_ascii_case(b"BOARD")`（拒绝外部覆盖）；同文件 `:12`、`:19`、`:21` 的模块文档也直接写 `BOARD`。测试与其它字面量另有命中：`boot_params.rs:218/239/250`、`os/kernel/src/lib.rs:5819` 的 `b"ARCH=x86_64\0BOARD=qemu\0"`；`os/kernel/src/misc.rs:315-316` 的 `board_id` 是 KINFO 报告字段，不是参数键，但也是拼写命中。
- 机械计数：`grep -rn "BOARD\|board" os/kernel --include=*.rs | grep -v /target/ | wc -l` 得 10；`grep -rn "BOARD\|board" os/libs/minix-boot --include=*.rs | wc -l` 得 16。
- 该句还与同节 doc:1697/1702/1705 自相矛盾：那几行明确说 `BOARD` 由 `minix-boot` 按来源生成、外部 `BOARD`/`board` 键被大小写不敏感地拒绝。REOPEN，计入本轮新准入 1 条；修法见 FINDING。

### 12. §5.2.2 的 `NoKernelPageSize` 落点 —— CLOSED

改后原文（doc:3280）：「一个跨距为 `2MiB + 4KiB` 的镜像在规划层 `plan_boot_pages`/`plan_kernel_mapping` 里既进不了 1GiB 档也进不了 2MiB 档……落到 `KernelMapError::NoKernelPageSize`（启动入口把它转成可读 panic）。」

源码核对：

- `os/kernel/src/boot_policy.rs:98-140` 的 `plan_kernel_mapping`：先 `preferred`（1GiB）再 `fallback`（2MiB），两者都不满足 `layout_satisfies` 时在 `:132` 返回 `KernelMapError::NoKernelPageSize`；`plan_boot_pages` 在 `:206-212` 直接转调它。档位次序由 `os/arch/src/arch/paging_ext.rs:309-311` 的断言（`GIB_1` 为 preferred、`MIB_2` 为 fallback）坐实。
- `boot_validate_and_prepare`（`os/kernel/src/lib.rs:839-909`）只做非地址断言、`require_boot_region`、`pt_alloc` 注册与 `detect_boot_page_support`，返回值是 `BootPageSupport<A>`；`:900-904` 的注释明说「Capability facts only: no page-size choice happens here」，体内不选尺寸。
- 两个入口把错误转成 panic：`os/kernel/src/lib.rs:458-459`（`enter_kernel_image_v1`）与 `:946-947`（`arch_boot_impl`）都是 `unwrap_or_else(|e| panic!("... page-size policy: {e}"))`，`KernelMapError` 的 `Display`（`boot_policy.rs:43-53`）给出可读文案。

### 13. §3.4.1/§4.4 的 aarch64 L9 覆盖措辞 —— CLOSED

改后原文（doc:1653）：「riscv64 直核面的跨距由 `check-layout.sh` 的 L9 对账，aarch64 直核面目前只有 L0/L3 的间接基址对账，跨距没有等价断言（缺口登记见 `OPEN-DEBT.md` 的 F-R34-01）」；doc:2674 表格同步为「riscv64 跨距由 L9 对账，aarch64 跨距暂无等价断言（`OPEN-DEBT.md` 的 F-R34-01）」。

源码核对：

- `os/kernel-image/check-layout.sh:248-260`：L9 整段位于 `if [ "$arch" = "riscv64" ]` 分支内，`:255` 从 `src/bootface.rs` 抽 `KERN_SIZE` 与 ELF 实测跨距比对；没有 aarch64 分支。
- aarch64 侧只有 L0（`:119-135`，`.ld` 的 `KERNEL_VIRT_BASE`/`KERNEL_PHYS_BASE` 与脚本预期表对账）与 L3b/L3c（`:188-190`，首段 VMA/LMA 等于预期基址）；`aarch64.ld:116` 的 `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000, ...)` 是上界，不是与 `bootface_a64.rs:83` 的 `KERN_SIZE = 0x40_0000` 的等值断言，故「跨距没有等价断言」成立。
- `OPEN-DEBT.md` 的 `F-R34-01` 存在（实际路径 `rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/OPEN-DEBT.md:21`），内容为「L9 只覆盖 riscv64 `bootface.rs`，aarch64 `bootface_a64.rs` 的 `KERN_SIZE` 无等价构建期对账」。
- 备注（不计新准入）：L0/L3 并不读取 `bootface_a64.rs` 的常量，只对账 `.ld` 与 ELF 一侧；「间接」一词承担了这个限定。若按字面要求「三个常量都要有对账」，缺口范围比 F-R34-01 只列 `KERN_SIZE` 更宽；当前句子的「跨距没有等价断言」是准确的。

### 14. §3.3 的收敛层措辞 —— CLOSED

改后原文（doc:1571）：「与 UEFI 独立 shim 共享同一套取件库；内核引导收敛层按腿分两处（直核 `arch_boot_finish`、跨镜像 `arch_boot_resume_v1`，最终同落 `jump_to_kmain`，见 §3.7/§4.8）——统一发生在层内（类型、取件库、收敛层）」。

源码核对：

- `os/kernel/src/lib.rs:214-232`：`arch_boot_finish` 先 `validate_before_tables`/`check_boot_postconditions`/`store_kernel_info`，调 `arch_boot_impl` 后 `H::jump_to_kmain(kinfo, kinfo.kern_stack_top)`；x86_64/aarch64/riscv64 三个 `arch_boot` 分支都委托它（`:285`、`:298`、`:792`）。
- `os/kernel/src/lib.rs:645` 起的 `arch_boot_resume_v1` 解码 arena 后经 `resume_after_v1_decode`/`resume_additive`，`:375` 同样落 `H::jump_to_kmain`。
- 文档 §3.7（doc:1952 附近）与 §4.8（doc:3093 附近）都按「直核家族收敛在 `arch_boot_finish`、跨镜像三腿收敛在 `arch_boot_resume_v1`……最后落到同一个 `jump_to_kmain`」展开，与改后句一致。

### 15. §4.4 的非 strict 来源覆盖 —— CLOSED

改后原文（doc:2726）：「非 strict 在 `memmap_sources` 非空时也查“来源记录覆盖投影内存图”（条件性检查，见 §3.4.1）」；doc:2742 表格行同义。

源码核对：

- `os/libs/minix-boot/src/kernel_info.rs:503-513`：`} else if !self.memmap_sources.is_empty() { match crate::memmap_source::audit_source_coverage(...) { Ok(()) => {} Err(error) => panic!("KernelInfo: source records do not cover the projected map: {error:?}") } }`。这正是「非 strict + sources 非空时做覆盖检查」的条件性行为。
- 同节 `:443-502` 的 strict 分支另做 authority 绑定与 `audit_source_coverage_for(..., Some(self.authority))`；doc:1625 对两档差别的描述与源码一致，`见 §3.4.1` 指向有效。

## FINDING RD34-CERT-FACT-01（P0）

类别：P0-fact。

- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1710`（R34 改动句）。
- 原文：「仓内消费者核查：Rust 侧 `os/kernel` 与 `minix-boot` 对 `BOARD`/`board` 两种拼写检索均零命中，无读者受害……」
- 反证（命令与读数）：
  - `grep -rn "BOARD\|board" os/kernel --include=*.rs | grep -v /target/ | wc -l` → `10`
  - `grep -rn "BOARD\|board" os/libs/minix-boot --include=*.rs | wc -l` → `16`
  - 语义命中：`os/libs/minix-boot/src/boot_params.rs:137`（`b"BOARD="` 生成）、`:174`（`eq_ignore_ascii_case(b"BOARD")` 拒绝外部覆盖）、`:12/:19/:21`（模块文档）；测试字面量 `boot_params.rs:218/:239/:250`、`os/kernel/src/lib.rs:5819`。
- 性质：这是可机械复核的仓库检索读数被写成「零命中」，属事实错误；同节 doc:1697/1702/1705 自己已经写明 `minix-boot` 会生成 `BOARD` 并拒绝外部 `BOARD`/`board`，句子与本节上下文矛盾。
- 建议修法：把证据句收窄为「Rust 生产代码没有按 `BOARD`/`board` 键从 `param_buf` 取值的 reader；键名只出现在 `minix-boot::boot_params` 的生成与外部覆盖拒绝名单（及测试字面量）」。结论「无读者受害」可保留。
- 准入：计入本轮，`1` 条。

## 备注（不计新准入）

1. doc:1653 的「aarch64 直核面目前只有 L0/L3 的间接基址对账」中，L0/L3 并不读取 `bootface_a64.rs` 的常量（它们只核对 `.ld`/ELF 一侧）；「间接」承担了限定，`F-R34-01` 只登记 `KERN_SIZE`。若未来要求三个常量都进构建期对账，台账范围需要扩写。
2. doc:2307 的 UEFI 块头「未展开段」清单没有点名 `uefi_helpers.rs:976-979` 的 `assert_eq!(layout.entry_point, ...)`；块头「节选」总限定使其不构成错句，doc:2674 另有交代。
3. doc:1787 的 `i386/kernel.lds` 块头写「保留段划分与偏移量相关的行」，`...` 省掉的第 8-10 行是 `__k_unpaged__kern_offset/vir_base/phys_base` 三条别名；标记诚实，取景说明可再精确一点。
4. doc:1714 的「本文出现的“版本 8→9”都是后者」按字面仍成立（带箭头的短语只指 wire 信封版本）；doc:740 新增的第二个 v9（`VM_BOOT_HANDOFF_VERSION`）已在该句内显式区分，读者按 740 的括注读即可。

## 总裁定

新准入 = 1（RD34-CERT-FACT-01，P0-fact：§3.4.2 的 `BOARD`/`board`「检索零命中」证据句不成立；其余 14 项 CLOSED）。

## certify #2（定点回读）

范围：只重读 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 的 464-465 行与 1710 行（含邻句），并对账 `os/libs/minix-boot/src/boot_params.rs:130-176`。未重扫全文，未读其他面板输出。

### ① 检索句与 `boot_params.rs:130-176` 的一致性 —— CLOSED

改后原文（doc:1710）：「Rust 侧 `os/kernel` 与 `minix-boot` 没有把 `BOARD`/`board` 当板卡身份读取的消费者——命中只在 `boot_params.rs:137`（生成内部 `BOARD=`）与 `:174`（拒绝外部同名键）以及测试字面量；C 内核的 `get_value` 读 `board=` 属 C 侧消费者，不在本审计范围。」

源码核对：

- `boot_params.rs:132-156`：`if let Some(board) = board { ... }` 分支里，`:137` 是 `for &byte in b"BOARD=" {`，随后把 `board` 的值逐字节拼进记录并经 `append_record` 写入输出。这正是「生成内部 `BOARD=`」，行号与描述一致。
- `boot_params.rs:158-177`：输入记录先按 `=` 切出 `key`（`:172-173`），`:174` 是 `if key.eq_ignore_ascii_case(b"ARCH") || key.eq_ignore_ascii_case(b"BOARD") {`，命中即 `rejected += 1; continue;`（`:175-176`）。这正是「拒绝外部同名键」，且对大小写不敏感，行号与描述一致。
- 四条生产调用全部传 `None`（certify #1 已核：`uefi_helpers.rs:313`、`opensbi_helpers.rs:338`、`bootface.rs:334`、`bootface_a64.rs:308`），Rust 侧没有按 `BOARD`/`board` 键回读板卡身份的消费者；`os/kernel/src/misc.rs:315-316` 的 `board_id` 是 GET_MACHINE 的 C 布局字段，`machine_record` 用 `..Default::default()` 恒填 0（同文件 `:3079` 的测试断言 `board_id == 0`），不是参数键读者。结论句成立。
- `C 内核的 get_value 读 board=` 的归属说明与 `minix3/minix/kernel/main.c:133`（`env_get(BOARDVARNAME)`）、`:481-499`（`get_value` 从 `param_buf` 首条匹配返回）一致，划出审计范围的说法成立。

### ② 旧措辞残留检查 —— 无残留

在两条改动段各自做字面检索：

- `sed -n '460,470p' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | grep -n "零命中\|无读者受害\|xboard=1"` → 无输出，退出码 1。
- `sed -n '1705,1716p' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | grep -n "零命中\|无读者受害\|xboard=1"` → 无输出，退出码 1。

doc:465 仍有「内核侧没有读者」，但主语是 C 内核的 `arch=`（`grep -rn "ARCHVARNAME" minix3/minix` 只有两处写点，无 reader），与本次撤销的 BOARD 断言无关，certify #1 第 1 项已核实。`xboard=1` 已改为 `xboard=2`，与同句示例和末句「不是 `xboard=2` 的后果」一致。

### ③ 新事实错误检查 —— 未发现新的 P0/P1/P2

- doc:464-465：本轮只改了「（它不是首条记录，前面还有别的键）」前提与末句 `xboard=1` → `xboard=2`；这两处在 certify #1 第 2 项已按 `pre_init.c:47-59` 的扫描推进与单 NUL 记录格式验证，结论仍成立，未引入新错误。
- doc:1710：没有新的数字、路径或行为断言；两处行号 `:137`、`:174` 与 `boot_params.rs:130-176` 逐行对得上；删除「零命中」「无读者受害」后，句子与同节 doc:1697/1702/1705 对 `BOARD` 生成与拒绝的描述也不再矛盾。
- 非准入观察：句中的「命中只在……」是就「把 `BOARD`/`board` 当板卡身份读取」这一语义范围而言；若按裸 grep 读，仍会看到 `boot_params.rs:105/132/133` 的 `board` 形参、同文件 `:12/:19/:21/:103/:104` 的模块文档提及，以及 `os/kernel` 侧 `board_id` 字段与注释里的 incidental 命中。它们都不是读取 `BOARD` 键的消费者，句子的前置限定已把它们排除，不构成新的事实错误；不重开。

认证 #2 新准入 = 0
