# D02 R4 读者面 certify 回读（round4-reader）

- 角色：独立 certify 回读者（读者面），冷启动；未参与 D02 R4 面板与修复。
- 仓库：`/home/xzhao/github/minix-rs`；最终 HEAD：`7ce60e31f`。
- 目标文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（1294 行）。
- 范围：`/tmp/r4-cert-reader.md` 列出的 12 条读者面修改点，100% 回读，不抽样。
- 方法：以当前 HEAD 的文档原文为判定依据；逐条定位修改段及邻句；用文档内算式、表格行序、前后文交叉核对。§6.2 的加法用 shell 复算。仅用 git diff 确认 R4 修改批次与旧句，不把旧版本当事实基线。未读 `panel/round4`、`minix3/`、`os/` 或历史评审。
- 判定口径：CLOSED = 原理解障碍/自相矛盾在文档原文中已消失；STILL-BROKEN = 仍能在原文中复现冲突。

## 逐条判定

### READER-A-01：STILL-BROKEN（§3.2 ↔ §4.2）

已修部分（§3.2，line 503）：

> 迭代器只把**非 `PT_LOAD` 类型**当作正常过滤（`continue`）；其余畸形一律以 `Some(Err(..))` 浮出：表项偏移溢出/越界是 `PhdrOutOfBounds`，`parse_phdr` 失败按具体变体返回，段文件字节越界是 `SegmentOutOfBounds`，`filesz > memsz` 是 `FileszExceedsMemsz`（I-48/D-084）。

已修部分（§4.2，line 851）：

> 循环体内先取 `seg_result?`（I-48 / D-084：畸形程序头由迭代器以 `Err` 项浮出，`filesz > memsz` 的守卫也已收进迭代器）

残留（§3.2，line 458）：

> 迭代器在 `next()` 里做类型过滤与范围检查，命中这些条件时 `continue`（不产出该段）；但 `SegmentIter::new` 与 `parse_phdr` 的表范围算式是未检查加法，畸形输入会 panic 而不是 `Err`（代码修复登记设计批 D2-01）

冲突：line 458 仍把“类型过滤与范围检查”合在一起说“命中这些条件时 `continue`”；紧接的代码节选（line 476）写的是“表项越界/太小是畸形，不是过滤：以 `Err` 项浮出（I-48/D-084）”，line 503 又写“只把非 `PT_LOAD` 类型当作正常过滤；其余畸形一律……`Err`”。读者在同一节内先读到旧口径，再读到与之相反的新口径；A-01 原文所说“越界输入走 `continue`”的理解障碍在引导句里仍然复现。

最小修复：把 line 458 的第一句改成“迭代器在 `next()` 里做类型过滤与范围检查：只有非 `PT_LOAD` 类型走 `continue`（不产出该段），其余畸形以 `Err` 项浮出；`SegmentIter::new` 与 `parse_phdr` 的表范围算式仍是未检查加法……”，或直接删除“与范围检查，命中这些条件时 `continue`”这半句。

### READER-A-02：CLOSED（§6.2）

引文（§6.2，line 1253）：

> 例如加载基址 `0x80200000` 时 `nm` 给出 `0x80200160`，而 `$rip` 是 `0xffffffc000000160`

复核：`delta = 0xffffffc000000000 − 0x80200000 = 0xffffffbf7fe00000`；`0x80200160 + 0xffffffbf7fe00000 = 0xffffffc000000160`（shell 复算，模 2^64）。该 delta 与 §6.2 line 1227 的 riscv64 L4 平移量 `0xffffffbf7fe00000` 同值；§4.3 line 989 的源码注释读数也用同一 delta 复算通过（`0xffffffc00000477a + 0xffffffbf7fe00000 = 0xffffff7f7fe0477a`）。A-02 指出的 `0x200` 偏差已消失。

### READER-B-01：CLOSED（§3.1）

引文（§3.1，line 366）：

> riscv64 测试载体虽然也直接调 `arch_boot`，但它链接在物理地址上、`direct leg` 加 `delta`，跳转完成时 PC 落在高半 VMA（下表第 3 行）。

指针对账：§3.1 登记表的数据行第 3 行（line 374）正是“| 载体单地址脚本 | riscv64 测试载体（直接 `arch_boot`） | 物理视图 | 高半 VMA（direct leg 加 delta） |”；原来的“第 5 行”（line 376，x86_64/aarch64 测试载体）已不再是引用目标。全文 grep 无“第 5 行”残留。表行计数口径与 §3.4 line 576 的“第 1、2、3、5 行，第 4 行……”一致（数据行，不含表头）。

### READER-B-02：CLOSED（§3.2）

引文（§3.2，line 408）：

> Rust 侧把 C 内联在引导器里的这段解析搬到独立 boot-shim：boot-shim 与内核镜像分属两个可执行体，装载发生在 shim 内、由 shim 自己按 `p_paddr` 摆放（§3.1 的表把 shim 记为装载方）。因此这些字段由 shim 用本节的共享解析器读，内核侧一行解析代码也没有；与 C 的差别不在“有没有外部装载者”，而在装载者是本仓代码、与内核共用同一份解析器。

复核：原负担句“Rust 侧不存在 C 那种『内核代码之外、由另一个程序按 `p_paddr` 摆放镜像』的角色”已消失；现在先肯定“搬到独立 boot-shim”，再说 shim 与内核是两个可执行体、装载在 shim 内完成，最后明确“差别不在有没有外部装载者”。前后不再自相矛盾，职责也不再重叠。

### READER-B-03：STILL-BROKEN（§3.2 ↔ §4.2）

已修部分（§3.2，line 503）：同 A-01 的“只把非 `PT_LOAD` 类型当作正常过滤；其余畸形一律以 `Some(Err(..))` 浮出”。

已修部分（§4.2，line 851）：同 A-01 的“畸形程序头由迭代器以 `Err` 项浮出”。

残留（§3.2，line 458）：

> 迭代器在 `next()` 里做类型过滤与范围检查，命中这些条件时 `continue`（不产出该段）；但 `SegmentIter::new` 与 `parse_phdr` 的表范围算式是未检查加法，畸形输入会 panic 而不是 `Err`（代码修复登记设计批 D2-01）

冲突：B-03 问的是“同一类畸形输入，panic 还是 `Err`”。line 503 已把 panic 收窄为 `SegmentIter::new`/`parse_phdr` 的表范围算式（D2-01 残留），但 line 458 的引导句同时保留了“范围检查……`continue`”和“畸形输入会 panic”两种旧口径；读者在同一段里仍会读到“`continue` / panic / `Err`”三种处置，且 line 458 没有把 `continue` 限定为非 `PT_LOAD`。因此两处说法冲突没有被完全消解，只是被部分重新标注。

最小修复：同 A-01，重写 line 458 的引导句，明确“只有非 `PT_LOAD` 走 `continue`；其余畸形以 `Err` 浮出；表范围算式的 panic 是 D2-01 残留”。这样 panic 与 `Err` 的边界就由一句话固定下来。

### READER-B-04：CLOSED（§4.5）

引文（§4.5，line 1103）：

> 脚本不变量进断言脚本，栈顶值进派生访问器，跳转前提进 `unsafe` 契约。

引文（§4.5 表，line 1099）：

> | `k_initial_stktop` 链接符号（§2.4） | `KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size` 的派生访问器） | 改型 | 值由接收方现算，内核不声明外部符号，也不在交接载荷里多传一份 |

复核：与 §3.3 line 526“v10（D-062）之前这里曾是一个独立字段 `kern_stack_top`……字段与三个派生标量一起删除，公式只留下访问器这一份”一致；不再把栈顶说成 `KernelInfo` 的现存字段。

### READER-C-01：CLOSED（§3.2 代码节选）

引文（§3.2，line 477–480）：

> let phdr = match parse_phdr(self.image, offset, self.ehdr.e_phentsize) {
>     Ok(p) => p,
>     Err(e) => return Some(Err(e)),
> };

引文（§3.2，line 476）：

> // 表项越界/太小是畸形，不是过滤：以 `Err` 项浮出（I-48/D-084）

复核：C-01 原文引用的 `Err(_) => continue` 已不存在；代码节选与 §4.2 line 851“畸形程序头由迭代器以 `Err` 项浮出”同口径。C-01 这一射程内的互斥语义已消除。注意：§3.2 line 458 的引导句仍有旧口径残留，见 A-01/B-03；该残留不属于 C-01 引用的代码节选，但会让整节读起来仍不自洽。

### READER-C-02：CLOSED（§3.1）

同 B-01。引文（§3.1，line 366）：

> ……跳转完成时 PC 落在高半 VMA（下表第 3 行）。

指针对账：数据行第 3 行（line 374）为 riscv64 测试载体；原“第 5 行”（line 376）已不再被引用。全文无“第 5 行”残留。

### READER-C-03：CLOSED（§4.5）

同 B-04。引文（§4.5，line 1103）：

> 栈顶值进派生访问器

引文（§4.5 表，line 1099）：

> `KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size` 的派生访问器）

复核：与 §3.3 line 526 的字段删除说明一致；全文 grep `栈顶值进结构体字段` 无命中，旧表述已清除。

### READER-C-04：CLOSED（§6.2）

同 A-02。引文（§6.2，line 1253）：

> 例如加载基址 `0x80200000` 时 `nm` 给出 `0x80200160`，而 `$rip` 是 `0xffffffc000000160`

复核：`0x80200160 + 0xffffffbf7fe00000 = 0xffffffc000000160`，与 §6.2 line 1227 的 riscv64 平移量一致；原 `0xffffffc000200160` 的 `0x200` 偏差已消失。

### READER-C-05：CLOSED（§4.1，实质为 .lrodata 孤儿）

引文（§4.1，line 740）：

> R4 之前 x86_64 工件的 `.lrodata.*` 就是未被 `.rodata` 通配收走的孤儿（986 个节、独立成一个 `PT_LOAD`，因落在收口之前而让 L5 通过）；本轮把三份脚本的归并补齐后，x86_64 的 `PT_LOAD` 从 5 个回到 4 个。

引文（§4.1，line 706）：

> `*(.rodata .rodata.* .lrodata .lrodata.*)`

引文（§6.2，line 1207）：

> 当前工件的 LOAD 行数：三架构各 4 行（x86_64 的 .lrodata.* 已在 R4 收进 .rodata；

复核：C-05 所问的“未被 `.rodata` 通配收走的孤儿、无 `AT()` 因而 LMA=VMA”的现状已不存在；三份脚本的 `.rodata` 通配已收 `.lrodata`，`PT_LOAD` 行数回到 4。**元数据异常**：`/tmp/r4-cert-reader.md` 给 C-05 的“处置”与“期望复核点”写的是 §4.5“栈顶值进派生访问器”（与 B-04/C-03 相同），与该条正文的 `.lrodata` 主题不符；本判定按正文主题核，`.lrodata` 的修复位置在 §4.1，不在 §4.5。期望的 grep `栈顶值进派生访问器` 也命中（line 1103），但它只证明 B-04/C-03 的修复，不证明 C-05。

### READER-C-06：CLOSED（§3.4）

引文（§3.4，line 603）：

> 真正会坏的有三种情形：①三者都去掉时，一个非 16 对齐的 `stack_top` 会一路传到 `kmain`；②**只去掉 `sub rsp, 8` 也会坏**……；③**只去掉 `push 0`** 时终止槽缺失，且 `call` 前 RSP 由 `and` 后的 16 边界直接减 8（`call` 压返回地址），入口 RSP 变成 0 mod 16，同样违反 System V 的 `(RSP+8) % 16 == 0`，`sp_aligned` 会在真机上当场红。

复核：第三种情形已补上，算式正确：`and` 后 RSP ≡ 0 (mod 16)，`sub rsp, 8` 后 ≡ 8，`call` 压 8 字节后入口 ≡ 0；与 §4.3 line 1012“去掉它改变的是入口栈的对齐，`sp_aligned` 断言会当场失败”一致。§4.3 line 916“`push 0` 仍是初始栈的终止槽……不在帧指针链上”不再被读者误读为“可删的遗留物”，因为 §3.4 已给出删除后的具体失败。

## 重点交叉核对（任务点名，另有非 12 条 ID 的事项）

### 1. §3.1 第 5/3 行指针

见 B-01/C-02：现为“第 3 行”（line 366），数据行第 3 行（line 374）是 riscv64 测试载体；全文无“第 5 行”残留；行计数口径与 §3.4 line 576 一致（数据行，不含表头）。CLOSED。

### 2. §3.4 两个调用点 vs 三份实现

引文（§3.4，line 588）：

> 两个生产调用点（`arch_boot_finish`、`resume_additive`）各自经 `arch_boot` 的三份 `#[cfg]` 分支选中本架构实现，所以“两个调用点”与“三份实现分支”不是同一个计数；§4.3 逐条对照。

计数解释已补上：读者不会再误以为“两个调用点”与“三份实现”必须一一对应。残留（旁及，不计入 12 条）：`resume_additive` 全文只出现这一次；同一文档在 §3.4 line 580 前置条件表、§4.4 line 1077、§5.4 M9 line 1176 都把 v1 侧调用点写作 `resume_after_v1_decode`。冷读者无法判断 `resume_additive` 是 `resume_after_v1_decode` 的子步骤、别名，还是笔误。最小修复：若 `resume_additive` 就是实际调用点，在 §3.4 首次出现处加一句它与 `resume_after_v1_decode` 的关系；否则统一改为 `resume_after_v1_decode`。

### 3. §3.4 push 0 第三种坏情形

见 C-06：line 603 已有 ③，算术与 §4.3 line 1012 一致。CLOSED。

### 4. §4.5 栈顶字段句改为访问器

见 B-04/C-03：line 1103 与 line 1099 均已改“派生访问器”；grep `栈顶值进结构体字段` 无命中。CLOSED。

### 5. §6.2 riscv GDB 算例闭合

见 A-02/C-04：line 1253 的 `0xffffffc000000160` 与 delta 复算一致；line 1227 的 riscv64 L4 平移量 `0xffffffbf7fe00000` 同值；line 989 的源码注释读数也用同一 delta 复算通过。CLOSED。

## 复算与检索记录

以下命令均可在仓库根重放；行号对应当前 HEAD `7ce60e31f`。

```bash
printf '0x%x\n' $(( (0x80200160 + 0xffffffbf7fe00000) & 0xffffffffffffffff ))
# 0xffffffc000000160

printf '0x%x\n' $(( (0xffffffc000000000 - 0x80200000) & 0xffffffffffffffff ))
# 0xffffffbf7fe00000

printf '0x%x\n' $(( (0xffffffc00000477a + 0xffffffbf7fe00000) & 0xffffffffffffffff ))
# 0xffffff7f7fe0477a

grep -n '非 `PT_LOAD` 类型' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 503

grep -n '0xffffffc000000160' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 1253

grep -n '下表第 3 行' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 366

grep -c '下表第 5 行' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 0

grep -n '栈顶值进派生访问器' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 1103

grep -n '只去掉 `push 0`' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 603

grep -n '把 C 内联在引导器里的这段解析搬到独立 boot-shim' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 408

grep -n '迭代器在 `next()` 里做类型过滤与范围检查，命中这些条件时 `continue`' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
# 458
```

## 汇总

- 12 条中：10 条 CLOSED；2 条 STILL-BROKEN（READER-A-01、READER-B-03）。两条是同一处残留：§3.2 line 458 的引导句仍写“类型过滤与范围检查，命中这些条件时 `continue`”，与同节 line 476 的代码注释、line 503 的新段落、§4.2 line 851 的 `Err` 口径冲突。
- 修复是一条一句话的编辑：把 line 458 的“类型过滤与范围检查，命中这些条件时 `continue`”改为“只有非 `PT_LOAD` 类型走 `continue`，其余畸形以 `Err` 项浮出”，并保留表范围算式的 D2-01 残留说明。改完后 A-01/B-03 可复判；C-01 的代码节选已是新口径，不受影响。
- C-05 的登记元数据有错位（处置/复核点写成了 §4.5 访问器），但正文主题（`.lrodata` 孤儿）已在 §4.1 修复，故按正文判 CLOSED；建议修登记表，避免下一轮用错误的 grep 复核 C-05。
- 补充观察：§3.4 line 588 的 `resume_additive` 与全文其余处的 `resume_after_v1_decode` 命名不一致（见“重点交叉核对”第 2 条）。这不属于 12 条 ID，但它是同一句“两个调用点”解释里的读者障碍。

## 局限

- 冷启动、只读文档；没有读 `os/` 源码，因此 `resume_additive` 是否真实存在、`SegmentIter::new`/`parse_phdr` 的未检查加法是否仍如文档所述，均按文档自述处理，未做代码复核。
- 未读 `panel/round4`、`minix3/` 或历史评审；本报告只判断“文档原文是否还让读者撞上原来的理解障碍/自相矛盾”，不判断修复是否与 C 源码或 Rust 实现完全一致。
- §6.2 的加法用 shell 复算；表行指针按文档自己的数据行计数口径核对（§3.4 line 576 提供了同口径样例）。§3.2 的“三类畸形”/“四类 Err”计数未被列为 12 条之一，未展开判定。
- 本回读由单 agent 冷启动完成；判定基于文档原文与 shell 复算，未做跨模型复核；正文已列出可重放的 grep 目标与加法算式。
- 12 条按 `/tmp/r4-cert-reader.md` 的条目逐条回读；C-05 条目的正文被截断在“它的 VMA 在高半（`”，其“处置”字段又明显错位，故 C-05 的判定以可见正文主题为准，并单独标注了元数据问题。
