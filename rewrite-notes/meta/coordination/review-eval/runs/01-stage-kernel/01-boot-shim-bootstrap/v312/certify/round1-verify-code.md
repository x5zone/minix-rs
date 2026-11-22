# 01-01 V3.12 R1 代码修复独立复验（R1-37 / R1-38 / R1-39 / R1-40）

- 日期：2026-11-23
- 角色：独立复验者（未参与修复；只重放台账命令、读源码、跑测试）
- 输入：`/home/xzhao/github/minix-rs/rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/defect-ledger.md`
- 范围：只判定 R1-37、R1-38、R1-39、R1-40。
- **R1-24（DESIGN-02）不在本报告判定内**：台账"未决/需主 agent 裁决"节已标 BLOCKED（设计批级，跨 crate 所有权/借用模型，`review-publication.md` §四.6），本轮不实施，不给出 PASS/FAIL。
- 未修改源码、未修改台账；本文件是唯一写入。

## 1 基线

### 1.1 HEAD 与提交时序

复验开始时 HEAD 是 `7a356f781c8ad049c6d31a05a709bb699df66250`，R1 代码批以未提交改动形式存在于工作区。复验过程中该批改动被提交：

```
9228fd5129015ac343f2b3b715c12a3550af424f 2026-11-23 03:44:59 +0800
fix(01-01): R1 code batch and remaining doc precision
```

这**不改变复验对象**：`git diff --stat 7a356f7..9228fd5` 中八个文件的增删行数，与复验开始时工作区 `git diff --stat` 完全一致；提交后这八个文件在工作区无改动（`git status --porcelain` 只剩三份 review-eval 元数据）。源码阅读与两套测试都在这一内容上完成，并在提交后用同样命令复跑，计数一致。

### 1.2 相关文件哈希

`git blob` 取自 HEAD（`git ls-tree HEAD`）；`sha256` 取自工作区，且工作区内容与 HEAD 一致。

| 文件（绝对路径） | git blob | sha256 |
|---|---|---|
| `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/memmap_source.rs` | `900548c41443e09f3aef0f46481d7941f541e786` | `ed8637cde9483940f792e34b08a03eb85cb630c6ad83f16789c2d5625477159a` |
| `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/platform.rs` | `8b2fc729519c2ec41855b3bda247eede9f6bba5d` | `35379489d80871ce287979e95bf8e9a24dc9922a20fbd7461f6ab68f11472106` |
| `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1_write.rs` | `e680054835b7c0ac8e9e868e6a10632f061255b3` | `ed96a582af4f1ba0b8fe1a122b3e1b6a67788068a698593ca3d5921ad3adb1ca` |
| `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1_read.rs` | `5a556c4960cc9489a4472e76c3c75cd7d4441ace` | `0e0a5456fc9355686cce9f7d66ebd0ac2258ebdcbed59654a05a744fd046dd86` |
| `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs` | `4907fcaa59d867e885569436f78b6c5ea47cc13d` | `7afb2ff3bcad57ba140bb1537c4585a76e3e1e0422acde44d6720b80c4ec6d0b` |
| `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs` | `becb41d23d87cf63eb4b4a248be82771526e8808` | `9a7f6d92d1699067e4c2e4dd05b38ae86c7a2faa9d9696c6cc5b07f3c27b630b` |
| `/home/xzhao/github/minix-rs/os/boot-shim/src/lib.rs` | `4f039b298fd8ef42c2116a02ce25124a78c68be8` | `43e9bf18a8f32692de3e5095721702d904bf84d236398af1f5c8c8a124f05e1f` |
| `/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `cc7c7e849848af1131c0b770132716b737ae13bf` | `b1a9bd6603c3f274a42e23ef06f38c0e767ce02c844768b78e7e755712a15623` |

注：本表是 R1 代码批复验时的基线。R1-41 文档同步修改了 `01-boot-shim-bootstrap.md`，当前哈希见 §5。

### 1.3 测试与依赖编译（全部在 `/home/xzhao/github/minix-rs/os` 执行）

```
$ cargo test -p minix-boot --lib
test result: ok. 188 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p minix-kernel --features mock --lib
test result: ok. 866 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out

$ cargo test -p minix-kernel --features mock   # 完整跑（含集成测试与 doctest）
boot_integration:           3 passed; 0 failed; 3 ignored
init_protection_order_pin:  4 passed; 0 failed
set_addr_space_no_boot_alloc_pin: 2 passed; 0 failed
store_before_alloc_pin:     1 passed; 0 failed
doctests:                   0 passed; 0 failed; 1 ignored

$ cargo check -p boot-shim --lib
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.26s   # 13 条 warning，未逐条归因
```

最后一条不是台账要求，是我为确认 `with_len`/`len()` 的公开 API 变更没有打断依赖方而补的；`boot-shim` 只使用未改签名的 `with_optional_len`，编译通过。

## 2 逐根复验

| 根因 ID | 状态 | 复验命令 | 实际输出摘要 | 语义复核 |
|---|---|---|---|---|
| R1-37（DESIGN-03） | PASS | `rg -n -e 'SourceClass' -e 'physical_range' os/libs/minix-boot/src/memmap_source.rs`；`cargo test -p minix-boot --lib` | grep 命中 1 行：`2645: fn physical_range_expresses_the_top_page()`。该测试名在修复前（`7a356f7:2688`）已存在，测试体只用仍然保留的 `PhysicalRange` 类型，不调用被删的 `SourceRecord::physical_range()`。`SourceClass`、`fn class(`、`SourceRange::physical` 均 0 命中。测试 188 passed / 0 failed / 0 ignored。 | 删除面完整：四个死符号的定义与全部调用点都已消失（全仓 `os/` 与 `os/qemu-tests` 无残留）。`PhysicalRange` 类型本身保留是正确的，DTB 采集与 `projectable_end` 仍在使用。测试改为 `r.kind != SourceKind::FirmwarePrefix`，与原 `class() == InputDeclaration` 等价；重命名后的 `source_range_keeps_domain_and_numeric_extent` 直接断言 `range.start`/`end_exclusive`/`address_domain`。台账这条命令的"期望 0"与一个**预存且无关**的测试名冲突，属命令措辞不精确，不是代码缺陷（见 §4.3）。 |
| R1-38（DESIGN-04） | PASS | `rg -n 'Option<NonZeroU32>' os/libs/minix-boot/src/platform.rs`；`rg -n 'has_len' os/libs/minix-boot/src/platform.rs os/kernel/src/lib.rs`；`cargo test -p minix-boot --lib`；`cargo test -p minix-kernel --features mock --lib` | 第一条**字面命令 0 命中**：源码写作 `Option<core::num::NonZeroU32>`；改用 `rg -n 'NonZeroU32'` 命中 `:128`、`:150`、`:167`、`:184`。`has_len` 0 命中。188/0；866/0/8。 | 逐点核实通过：进程内 `len: Option<core::num::NonZeroU32>`（`platform.rs:128`）；`with_len` 收 `NonZeroU32`（`:147`）；`with_optional_len` 把 `None` 与 `Some(0)` 都归一到 `None`（`:161-173`）；`len()` 返回 Option（`:184`）；`has_len` 删除。`const _` 编译期断言 `size_of::<PlatformDescSource>() == 16`（`:132`），`repr(C)` 下 4+4+8 = 16、无填充。wire 写侧 `len().map_or(0, |n| n.get())`（`handoff_v1_write.rs:244`），读侧 `with_optional_len(..., Some(len))` 使 0 回到 `None`（`handoff_v1_read.rs:311`）。内核窗口消费改成 `if let Some(len) = source.len()`（`kernel/src/lib.rs:1213`）；`platform_sources_span` 用 `map_or(0, ...)`（`kernel_info.rs:277`），与旧 `len()` 对未知长度返回 0 的行为一致。生产调用点（`uefi_helpers`、`opensbi_helpers`、`bootface_common`）都用签名未变的 `with_optional_len`；显式 `with_len` 只剩两处测试位置（内核 `kernel/src/lib.rs:5892` 一处、wire 写侧 `handoff_v1_write.rs:632`/`:637` 两次），均已改 `NonZeroU32::new(...).unwrap()`。文档同步已由 R1-41 补齐；追加复验见 §5（原缺口保留在 §4.1 的历史记录中）。 |
| R1-39（DESIGN-05） | PASS | `rg -n 'startup-object record' os/kernel/src/lib.rs`；`rg -n 'memmap\(\) accessor' os/libs/minix-boot/src/kernel_info.rs`；`rg -n 'Every boot leg calls this function' os/boot-shim/src/lib.rs` | 三条均 0 命中。`LastRead(arena) ≺ Publish(VMFreePages)` 保留（`kernel/src/lib.rs:649`）；`vm_handoff::classify` 存在（`vm_handoff.rs:452`）、`vm_boot_exclusions` 存在（`kernel/src/lib.rs:2714`）；`build_kernel_info` 生产调用点只有 `kernel-image/src/bootface_common.rs:558`。 | 三处注释改正都成立，且新陈述本身经核实：① `arch_boot_resume_v1` 的生命周期合同删掉了"arena 仍被 startup-object record 命名"的旧句，保留了 `LastRead(arena) ≺ Publish(VMFreePages)` 与"跳转后不得再读 arena"的约束；旧句已不成立（v10 删除 startup 分区，全仓无 `StartupObject` 类型），正式文档 §4.6（`:2539`）也早已写明"内核不再保留 startup-object 记录"。② `kernel_info.rs` 的 `memmap` 注释改为指向 `vm_handoff::classify` 与 `vm_boot_exclusions`，两个符号都在代码中存在，旧的 `memmap()` 访问器则不存在。③ `build_kernel_info` 注释"两个直核面调用、三条 v1 腿在接收侧解码"与调用点一致：`bootface_common` 只被 `bootface.rs`（riscv64）与 `bootface_a64.rs`（aarch64）两个直核面复用；两条 UEFI 腿与 U-Boot shim 腿都走 `arch_boot_resume_v1` 解码，`uefi_helpers`/`opensbi_helpers` 中对 `build_kernel_info` 的引用只在测试模块里。附带核实新评论"最早的重发者是 `VmBootAllocator`"：它在 `init_proc_and_boot` 内构造（`lib.rs:2945` mock / `:3004` real），该函数由 `kmain:1938` 调用，先分配 VM ELF 段帧与 handoff 帧，再到 `:3126` 的 `build_vm_handoff`；`kmain` 在此之前没有用 `kernel_info.memmap` 发页的消费者。 |
| R1-40（DESIGN-06） | PASS | (a) `rg -n 'PlatformDescSource'`…（同段含 `repr(C)` 例外句）；(b) `rg -n 'mm_type'`（≥1）；(c) `rg -n 'bootface 的 .PT_BUMP_PAGES.'`（期望 0）；(d) `rg -n 'write_byte'`（上下文含"`init` 可选"） | (a) 5 处命中，`1282` 行的 §3.2.3 例外段同时含 `PlatformDescSource` 与 `#[repr(C)]`；代码确有 `#[repr(C)]`（`platform.rs:123`）、16 B 断言（`:132`）、对应的 `WirePlatformDescSource`（`handoff_v1.rs:64`，自带 16 B/8 B 对齐断言）。(b) 4 处命中（`472`、`484`、`522`、`1330`）。(c) 旧短语 0 命中；§4.1 现写 `bootface_common` 的 `PT_BUMP_PAGES = 256`，常量确在 `kernel-image/src/bootface_common.rs:35`。(d) `1256` 行现写"每架构至少实现 `write_byte`（`init` 可选；x86-64 用它初始化 COM1）"；trait 的 `fn init() {}` 有默认实现（`plat/src/early_console.rs:61`），`write_byte` 是必需方法（`:64`），`X86_64EarlyConsole::init` 调 `ser_init()`（`plat/src/x86_64/early_console.rs:113-116`）。 | 四项都对着代码/C 源核实成立。补充证据：i386 的 `multiboot_memory_map_t` 是 `struct multiboot_mmap` 的 typedef，字段为 `mm_base_addr`/`mm_length`/`mm_type`（`minix3/sys/arch/i386/include/multiboot.h:212-217`、`:240`），内核里唯一写点是 `cbi->memmap[m].mm_type = MULTIBOOT_MEMORY_AVAILABLE`（`minix3/minix/kernel/arch/i386/pg_utils.c:109`），"类型标签固定 AVAILABLE、无来源记录"成立。两条精度观察见 §4.1（同一文档另一处的 `len: u32` 曾过期，已由 R1-41 关闭）与 §4.4（`mm_type` 是 i386 专属字段名，earm 的 typedef 字段叫 `type`，同样固定 AVAILABLE）。 |

R1-24（DESIGN-02）为 BLOCKED，未在本表判定。

## 3 结论

按本任务列出的四根验收标准（台账命令的语义意图与两套测试计数），四根全部通过。

**VERDICT: PASS**

- 判据：R1-37 PASS、R1-38 PASS、R1-39 PASS、R1-40 PASS。
- R1-24 为 BLOCKED（设计批级，§四.6），不在本判定内。
- 边界：原 §4.1 的文档-代码不同步已由 R1-41 关闭（追加复验见 §5）；本轮四根与文档同步均通过，无剩余出版阻断项。

## 4 残余风险与实际发现

### 4.1 已关闭：`PlatformDescSource` 文档同步（R1-41）

原发现：`01-boot-shim-bootstrap.md:1262` 曾写进程内句柄 `{ kind, len: u32, phys_addr }` 且 `len = 0` 表示未知，与 R1-38 之后的 `len: Option<core::num::NonZeroU32>`（`None` 表示未知）冲突。

R1-41 已修改该句：`:1262` 现写 `{ kind: PlatformDescKind, len: Option<NonZeroU32>, phys_addr: PhysBytes }`、`len = None` 表示未知，并注明 wire 侧仍写 `u32`、`None` 编码为 0、解码 0 还原为 `None`；`:1316` 把 16 B 布局限定为"平台源 **wire 记录**"，与进程内类型区分。追加复验证据见 §5。

注意 `:1316` 的 wire `u32` 描述本身正确，不应改成 `Option`。

### 4.2 wire 往返覆盖：新映射只有隐式覆盖

- 写侧测试只断言 `Some(n)` 被写成 `n`（`handoff_v1_write.rs:630-658` 的 `0x1234`/`36`）。
- 读侧往返测试的 `SOURCES` 用 `PlatformDescSource::new`（即 `None`，`handoff_v1_read.rs:435-438`），它实际走过了 `None → wire 0 → None`，但测试**没有断言**解码后的 `len()`。
- 没有专项测试断言 `with_optional_len(Some(0)) → None`，也没有测试断言解码结果的 `len()` 是 `Some(n)`。

结论：代码路径经我逐行核实正确，但该映射目前靠既有测试"顺带经过"而非显式断言；若后续重构动了 `with_optional_len`，现有测试不会变红。建议补一条往返断言（不需要改本轮已通过的代码语义）。

### 4.3 台账命令的字面输出与"期望"不一致（命令措辞问题，不是代码缺陷）

1. R1-37 的 `rg -n -e 'SourceClass' -e 'physical_range'` 期望 0，实际 1：命中修复前就存在的测试名 `physical_range_expresses_the_top_page`。它测的是仍保留的 `PhysicalRange` 类型，与被删的 `SourceRecord::physical_range()` 无关。
2. R1-38 的 `rg -n 'Option<NonZeroU32>'` 期望 ≥1，实际 0：源码统一写全限定路径 `Option<core::num::NonZeroU32>`。
3. R1-41 的 `rg -n 'len = 0'` 会把 `1276: ... namelen = 0 ...` 当成命中：这是子串误命中，进程句柄段（`:1262`）的 `len = 0` 实际为 0。按字面执行时应加词边界或限定段落。

若这些命令进入机械门按字面执行，应改台账命令（例如排除测试名、把模式改成 `NonZeroU32`），**不要改代码去迎合 grep**。

### 4.4 R1-40(b) 的字段路径是 i386 专属

`multiboot_memory_map_t.mm_type` 在 i386 成立（typedef 指向 `struct multiboot_mmap`）。earm 的同名 typedef 指向 `struct multiboot_mmap_entry`，字段名是 `type`（`minix3/minix/include/arch/earm/include/multiboot.h:340-348`），`add_memmap` 同样固定写 `MULTIBOOT_MEMORY_AVAILABLE`（`minix3/minix/kernel/arch/earm/pg_utils.c:99`）。§3.3 的句子是跨架构通述，读者在 aarch64 侧会找不到 `.mm_type`。这是措辞精度问题，不影响"类型标签固定 AVAILABLE、没有来源记录"的结论；若要消歧，可写成 `mm_type`（i386）或 `type`（earm）。

### 4.5 另一份正式文档仍引用被删 API

`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-adjudication-brief-memmap-open-items.md` 在 `:125`、`:1165`、`:1175` 仍把 `SourceRecord::physical_range()` 当作签名 API 引用。该文档自称"送审正文/现行规则来源"，但它描述的 wire 还是 v7（当前 v10），说明它整体已落后于本轮代码，不是 R1-37 新引入的漂移。记录在案，供协调者决定是否另开文档修复；不影响本轮四根判定。

### 4.6 §4.6 对重发时点的表述比代码注释松

代码注释现在写"最早的重发者是 `VmBootAllocator`"，正式文档 §4.6（`:2539`）仍写"VM 自由页清单在 `kmain → build_vm_handoff` 才发布"。两者不矛盾（发布清单与重发页是两个时点），但文档没有携带代码注释里更紧的截止线。REFUTE-05 的观察不在 R1-39 的处置条目内，这里只记录，不据此改判。

### 4.7 本轮文档 diff 夹带其他根因的改动

`01-boot-shim-bootstrap.md` 的 diff 除了 R1-40 的四处，还包含 §2.4.1.1 编号、§2.4.2.4 回指、§3.4.3/§6.1 标题、删除 `D-067`、去重连续 `---` 等改动，属其他根因。本报告只判定 R1-40 的四处，未评审这些夹带改动；它们应各自落到对应根因的复验里。

### 4.8 测试噪声

`cargo test -p minix-kernel --features mock` 的编译阶段有大量 unused import/variable warning。四个被改文件的 diff 没有新增 import 或绑定，我没有把任何 warning 归因到本轮；也没有做修复前后的 warning 基线对比（不在验收标准内）。

## 5 追加复验：R1-41 文档同步（2026-11-23）

触发：修复方称 §3.2.2.3 的 `platform_sources` 条目与 arena 段落已按 R1-38 的新类型同步，并新增 R1-41（自引入）。本次只复验这两处文档改动，按指示不重跑 cargo 套件。

### 5.1 基线

- HEAD 仍为 `9228fd5129015ac343f2b3b715c12a3550af424f`。
- 代码未动：`git status --porcelain -- os/` 为空，R1-37/38/39/40 的代码与 §1.2 的哈希一致。
- 文档工作区有新改动：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 的 sha256 由 `b1a9bd6603c3f274a42e23ef06f38c0e767ce02c844768b78e7e755712a15623` 变为 `c9f2952333486d20f5af48164421a07aae9a4b0809841d790918cd9e0657cb3c`。

### 5.2 复验命令与输出

```
$ rg -n 'len: Option<NonZeroU32>' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
1262: - `platform_sources`：… 每个 `PlatformDescSource` 是 16 B 句柄
      `{ kind: PlatformDescKind, len: Option<NonZeroU32>, phys_addr: PhysBytes }`；
      `len = None` 表示生产者未记录 blob 长度 … wire 侧仍写 `u32`：
      `None` 编码为 0，解码时 0 还原为 `None` …

$ rg -n 'len: u32' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
（0 命中）

$ rg -n 'len = 0' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
1276: - 其八，空键——… C 的删除循环在 `namelen = 0` 时 …
```

第三条的唯一命中是 `namelen = 0` 的子串误命中，与平台源句柄无关；进程句柄段（`:1262`）没有 `len = 0`。arena 段（`:1316`）用"旧生产者写 0 表示未知"描述 wire 语义，没有 `len = 0` 字样，且已加限定语：

```
$ rg -n '平台源 \*\*wire 记录\*\*' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
1316: … 平台源 **wire 记录** 是 16 B 的 `kind u32 | len u32 | addr u64`：`len` 复用旧 `reserved` 槽位，旧生产者写 0 表示未知 …
```

### 5.3 语义复核

- `:1262` 与代码逐项对齐：`platform.rs:128` 是 `len: Option<core::num::NonZeroU32>`；`with_optional_len` 把 `Some(0)`/`None` 归一到 `None`（`:161-173`）；wire 写侧 `None` 写 0（`handoff_v1_write.rs:244`），读侧 0 经 `with_optional_len(Some(len))` 还原为 `None`（`handoff_v1_read.rs:311`）。文档对进程内形状、未知语义与 wire 映射三件事的陈述全部正确。
- `:1316` 的 16 B `kind u32 | len u32 | addr u64` 前面加了"平台源 **wire 记录**"，读者不会再把它当成进程内 `PlatformDescSource`；该 wire 描述与 `WirePlatformDescSource` 的 16 B 形状一致，保持原样是正确的。
- 旁证：本次文档 diff 还包含 `:1190` 一处 `LOADER_DATA` 定义回指改动（`LMA/VMA 的定义见 §3.4/§2.4，LOADER_DATA 的定义见 §3.1`），属 R1-14 的文档精度项，不是 R1-41；本追加复验未评审它。

### 5.4 结论

- 原 §4.1 的文档-代码不同步已关闭。
- **R1-38 状态：PASS（维持）**；其语义复核中的"文档同步缺口"保留条件解除，不再指向任何未完成项。
- **追加 VERDICT：PASS**（R1-37、R1-38、R1-39、R1-40 与原判定一致；R1-41 文档同步通过）。
