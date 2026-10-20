# 01-todo — 01 篇关联代码设计审查工单（2026-10-20）

> **来源与方法**：对 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §7.3 关联代码清单的**设计合理性专项审查**（五个分片：契约层 `os/libs/minix-boot/`、boot-shim 实现层、内核 boot 路径 `os/kernel/src/`、arch 层、kernel-image + VM 消费侧），叠加对 01 篇正文与 `todo.md`（stage 权威台账）既有条目的逐条现状复核。焦点是**设计合理性**——抽象边界、trait 形状、契约钉死方式、冗余与叠床架屋、依赖方向、终态与过渡形态的错位；注释漂移仅在其暴露设计问题时登记。
> **口径**：`minix3/` C 源为真值；`os/` Rust 实现的偏差分三类——**注释失真**（改注释即闭环）、**抽象收口**（需设计裁决后动码）、**测试缺口**（补测试即闭环）。行号为登记日快照，动手前以符号锚点重新定位。
> **与权威台账的关系**：本文件是 01 篇射程内的**工单**，不是权威。既有条目（D-59/D-66/D-67/I-1/I-22/I-23/I-24/I-25）的描述与修法以 `todo.md` §7 为权威，本文件只给摘要 + 指针 + 本轮复核出的现状增量；本轮**新登记**的条目（D-68 ~ D-71、I-26、I-27）全文在本文件，待 GPT 多轮裁决（经 `01-code-design.md`）后再回填权威条目。
> **设计裁决载体**：需要设计与裁决的条目，逐条自包含简报见 `01-code-design.md`（网页端 GPT 可独立阅读，不依赖本仓库）；已有专门设计文件的（D-59 → `boot-dm-hugepage-design.md`）只给指针。

---

## 0. 使用说明（给代码迭代 agent）

1. **一次一条**（todo-fix 纪律）：每条自带现状、证据命令、设计方向与验收；动手前先跑证据命令复核现状（源码可能已前移）。
2. **裁决先行**：§3 B 批各条**先裁决后动码**——裁决载体是 `01-code-design.md` 对应节（网页端 GPT 多轮讨论的结论回填到本文件条目下，再实施）。§4 C 批互相独立、可直接实施。
3. **每条做完的通用验收**：`cargo test -p <对应 crate>` 通过；涉及共享契约层（`os/libs/minix-boot`）的条目，三架构 `cargo build` 与 qemu 引导测试（`os/qemu-tests/run_all.sh`）回归；受影响的 01 篇章节同步更新。
4. **硬约束**：`#![no_std]`（测试除外）；错误映射 Minix3 errno；注释全英文自包含（`os/` 下）；三档术语 `[ARCH: ...]` 三处一致。

---

## 1. 总览

| 批 | 条目 | 类型 | 状态 | 设计载体 |
|---|---|---|---|---|
| A | I-22 回勾、D-60 回勾 | 已落地待回勾 | 代码已闭环，台账读数过时 | — |
| B | D-59、D-66、D-67、I-1/NK1/OQ-N6 | 设计裁决（既有） | open，现状复核有增量 | `01-code-design.md` T1/T2/T3；D-59 → `boot-dm-hugepage-design.md`；I-1 → coordination 台账 |
| B | D-68、D-69、D-70、D-71 | 设计裁决（本轮新登记） | open | `01-code-design.md` T1/T4/T5 |
| C | I-26、I-27 | 注释/登记回修（本轮新登记） | open，可直修 | — |
| D | §5.3 TODO-01~07 收编 | 测试缺口 | 逐一复核仍 open | — |
| E | I-23、I-24、L1424 张力、L1422 口径、I-25 指针、读者待办 | 文档侧 | open | stage `todo.md` 详情 |

---

## 2. A 批——已落地、待回勾闭环（台账读数刷新）

### I-22 `MemoryMapSource` 丁案：代码已全部落地，`todo.md` §7.1 仍记 open

**现状（2026-10-20 复核，证据命令见下）**：

- trait 与 DTB 实现已落 `os/libs/minix-boot/src/memmap_source.rs:trait MemoryMapSource`（`DtbMemoryMap` + `clip_reserved` 纯函数 + 编译期护栏 + 8 个宿主测试）。
- 三链接线完成：UEFI 腿 `os/boot-shim/src/uefi_helpers.rs:struct UefiMemoryMap`（委托 `build_memmaps`，丁案 B）；opensbi 腿 `os/boot-shim/src/opensbi_helpers.rs:fn prepare_boot` 改用 `DtbMemoryMap::new(&fdt, 0).collect()`（丁案 C，缺 DTB 诚实停机）；bootface 腿 `os/kernel-image/src/bootface.rs:fn bootface` 改用 `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)`（丁案 B 迁移注释在 L187/L284/L496）。
- opensbi 侧 `DRAM_BASE`/`DEFAULT_RAM_SIZE`/`build_memmap` 与单区断言测试已删（grep 零命中）。
- 01 篇文档侧「硬编码的 QEMU virt DRAM 布局」「单条 DRAM」表述已清零（`grep -cE '硬编码的 QEMU virt DRAM 布局|单条 DRAM' 01-boot-shim-bootstrap.md` 得 0）。
- 附带落地了丁案之外的一件：`dtb_covers_conventional` 出界闸（SD-3 D1，UEFI 腿 dumpdtb.dtb 存在时对 conventional 快照做 DTB 全景覆盖校验）。

**证据命令**：

```
grep -rn "trait MemoryMapSource" os/libs/minix-boot/
grep -n 'DRAM_BASE\|DEFAULT_RAM_SIZE\|fn build_memmap' os/boot-shim/src/opensbi_helpers.rs   # 应零命中
grep -cE '硬编码的 QEMU virt DRAM 布局|单条 DRAM' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md   # 应 0
```

**遗留**：`bootface_a64` 第四链未迁（升格为本轮 D-68，见 §3）；连带文档六处的逐一核对（`04-platform-discovery.md` 分工段、`uefi_helpers.rs` 模块顶注等）未见专查记录——回勾时按 `todo.md` I-22 条目的连带清单 ①–⑦ 逐项过。

**处置**：`todo.md` §7.1 的 I-22 条目改记「代码已落地（丁案 A–F 除 aarch64 直核腿外），遗留 D-68 与文档连带核对」；本文件不再展开。

### D-60 `IDENTITY_MAP_END` 双定义：已修复，`todo.md` §7.1 仍记 open

**现状（2026-10-20 复核）**：kernel 侧已改引用 arch 权威——`os/kernel/src/lib.rs` 顶部 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END`；全仓 `const` 定义只剩 `os/arch/src/lib.rs:231` 一处。推荐修法（kernel `use` 归一）与解除条件（「全仓该字面量只剩一处定义」）均已满足；其余 `0x1_0000_0000` 命中属不同语义常量（`VM_BOOT_HANDOFF_VA`、DTB 界限闸、测试值），不在该条射程。

**证据命令**：

```
grep -rn "IDENTITY_MAP_END" os --include="*.rs" | grep "const"
# 仅 os/arch/src/lib.rs:231 一处定义；os/target/ 下为构建产物，不计
grep -n "BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END" os/kernel/src/lib.rs
```

**处置**：`todo.md` §7.1 的 D-60 条目改记「✅ 已修复（`2026-10-20` 前落地，`2026-10-20` 复核回勾）」，附上述证据。

---

## 3. B 批——设计裁决类（先裁决后动码）

> 本批每条的设计要点、候选方案与待裁决问题清单见 `01-code-design.md`（自包含简报 T1–T5）；既有条目的完整登记（现状读数、修法、DoD、现算命令）在 `todo.md` §7.1/§7.4，本节只摘与本轮审查相关的**增量**。
> **2026-10-21 外部首轮裁决已回填简报 v2**：T1 改题（wire 协议冻结为主、信封合一降为次级）、T3 的 UEFI 取参来源改判（当前镜像 `LoadOptions`，非 `Boot####` 枚举）、T4 已裁（独立小批，`fw_end=0` 带形态前提）、T5 已裁（案 1，先于 D-66 实施）、T2 补入口状态契约。实施次序裁定：**T4 → T1(wire) → T5 → T3 → T2**。各条目状态以下方行内标注为准。

### D-67 boot 信封合一 + 跨镜像布局全钉死（既有，`todo.md` §7.1）

**本轮增量（三条，均已写进简报 T1）**：

1. **`BootPrepareResult` 的文档声明已失真**：`os/libs/minix-boot/src/boot_shim.rs:struct BootPrepareResult` 的模块注释自称 "It is the **only** data crossing the boot-shim → kernel boundary"，而 `os/libs/minix-boot/src/handoff.rs:struct BootHandoff` 是第二个跨边界载荷（aarch64 跨镜像 blob）。信封合一落地时此注释随之删除；在此之前它是误导（读者会以为 `BootHandoff` 不存在或不需要钉布局）。
2. **`MemoryRegion.len: usize` 与 `kern_size/bootstrap_len: u64` 宽度不一致**：`kernel_info.rs` 对 `kern_size` 的注释明说 "Uses u64 instead of usize to avoid truncation on 32-bit targets"，而同文件 `os/libs/minix-boot/src/kernel_info.rs:struct MemoryRegion` 的 `len` 是 `usize`——跨镜像 blob 与 `#[repr(C)]` 布局钉死的射程内，`usize` 的宽度随 target 变，同一逻辑字段两种宽度没有理由。随 D-67 的切片 newtype 一并钉死（`u64`）。
3. **`KernelInfo` getter 家族缺 `param_buf`**：R-07 约定 getter 是推荐 API，14 个字段里 13 个有 getter，唯 `param_buf` 没有（`kernel_info.rs:impl KernelInfo`）。随 D-66/D-67 实施补齐。

**设计载体**：`01-code-design.md` T1 v2（**已改题**：主问题 = 跨镜像 wire 协议冻结——`BootSlice<T>` 显式 `(ptr,len)`、`free_upper_idx` 用 `u64::MAX` 哨兵、`MemoryRegion.len`→u64、协议头 magic/version/size、跨目标布局构建门；**信封合一降为次级**——`BootPrepareResult` 去留待 wire 冻结后按语义边界裁决，不以减少结构体数量为独立目标；builder 统一 D-69 联动保留）。

### D-66 `param_buf` 生产侧接入（既有，`todo.md` §7.1）

**本轮增量（两条，已写进简报 T3/T5）**：

1. **「内核侧零改动」的登记结论需要修正**：D-66 条目写「接入只动生产侧（boot-shim/bootface），`KernelInfo` 结构与内核零改动」。本轮审查发现 param_buf 一旦非空，UEFI 腿的参数字节将落在 UEFI 池堆——与 `memmap`/`reserved_regions`/`boot_modules` 三块切片**同病**（真机取证：池堆页在切根后读回零，见 `os/kernel/src/lib.rs:fn store_kernel_info` 的 NK4-C 注释与 vh1 读数），而 `GET_MONPARAMS` 消费点在 syscall 期（远晚于首次切根）。因此 D-66 落地**必须**同时在 `store_kernel_info` 增加 param_buf 落地区（= 本轮 D-70 的 param_buf 半）。
2. **SBI 腿来源裁决的输入**：`opensbi_helpers.rs:fn prepare_boot` 现有注释把来源标为 "SBI boot hart argument"，但 bootface 直核腿的实参是 `a0`=hartid、`a1`=DTB、`a2`=BootFileTable（`os/kernel-image/src/main.rs:fn rust_image_main` riscv64 签名）——没有命令行通道。三案裁决见简报 T3。

**设计载体**：`01-code-design.md` T3 v2（**已裁**：UEFI 腿读当前镜像 `EFI_LOADED_IMAGE_PROTOCOL` 的 `LoadOptions`/`LoadOptionsSize`——非 `Boot####` 变量枚举（后者降为排查备用）；文本编码是 Minix-RS 自有契约、非 UEFI 规范承诺；参数规范化四步统一规则（先生成内部必备键 ARCH/BOARD，再并入外部输入）；SBI 腿 DTB `/chosen/bootargs`，验收须实测本仓 QEMU 启动链的 `-append` → DTB 节点通路；截断按**整条记录**丢弃、不留半截 key=value）+ T5 v2（落地区已定案，**实施顺序先于本条**）。

### D-59 大页能力/选页策略四层抽象收口（既有，`todo.md` §7.1）

**本轮增量**：现状与登记一致——双入口仍在（`os/arch/src/arch/paging_ext.rs:fn supports_1gb_page` 的 `HugePages` 默认实现 vs `os/arch/src/arch/dm_coverage.rs:fn supports_1gb_page` 的 `DmCoverageArch`），x86 侧已把两个入口同源转发到一份 CPUID 查询（`os/arch/src/x86_64/paging.rs:fn supports_1gb_page` 两处，模块注释 L1029–1030 自陈）。设计文档 `boot-dm-hugepage-design.md`（2046 行，迭代中未定稿）就绪。**不进 `01-code-design.md`**（已有专门设计文件）；本文件不重复其内容。

### I-1 / NK1 / OQ-N6：x86 跨镜像跳转协议（既有 DEFERRED + coordination 台账）

**本轮增量（已写进简报 T2）**：

1. **OQ-N6 已裁，非「未裁决」**：`rewrite-notes/coordination/TODO-LEDGER-DONE.md` §2 记 `OQ-N6`——「自举载体的双镜像交换采『每进程独立根』原语；『加栈顶参数』的快案被否（段面互撞与零隔离是结构性缺陷）」，状态「已裁，实施待领」。aarch64 的 `BootHandoff`（x0 递物理指针）+ `arch_boot_resume_high_half`（在活跃根上只做 additive）+ 每进程独立根（VM 建进程页表）正是该裁决的参考实现。**剩余的 NK1（C-29）是 x86 腿的实施收口**：x86 何时从内联切到独立镜像交接、入口 ABI 用哪组寄存器递 `BootHandoff`。
2. **01 篇两处叙述与台账口径不一致**：L1422「跨镜像跳转协议还没裁决（NK1/OQ-N6，§4.8 交付边界）」与 L2624「跳转协议尚未定案（镜像头注释如此说明）」——「还没裁决」应对齐为「OQ-N6 已裁（每进程独立根原语），x86 腿的实施收口（NK1/C-29）待认领」；`os/kernel-image/src/main.rs` 头注「交付边界（诚实登记）」段的同款表述一并更新。
3. **x86 切换的存量资产**：`os/kernel-image/x86_64.ld` 布局产出与 `os/kernel-image/check-layout.sh` 的 L0–L9 校验已在位；aarch64 已把 `bootstrap_to_kernel_image`（shim 侧）与 `arch_boot_resume_high_half`（镜像侧）做成 `#[cfg(target_arch = "aarch64")]` 单架构实现——x86 版基本是把这两段的 cfg 泛化 + asm 寄存器换 SysV（`rdi` 递 blob 指针）。

**设计载体**：`01-code-design.md` T2 v2（**已裁方向**：独立镜像方案推进、`rdi` 交接、QEMU 先行短期双轨；**v2 补齐**——x86 入口状态契约成文：`_start` 是机器入口 trampoline（裸汇编）而非被 `call` 的普通函数，须自建内核栈/`cld`/`IF=0`/CR3 断言/长模式断言清单；源数据映射两证明分离（物理内容所有权 ≠ 虚拟可访问性，blob 可访问 ≠ 切片指针可访问）；`reserved_regions` 复核答复：字段保留，退场的只是 x86 内联窗口半——UEFI 腿 T2 后仍传非空表（固件占用区语义），D-69 统一签名据此保留该形参）。

### D-68（新登记）`bootface_a64` 的 memmap 链未迁 `DtbMemoryMap`——I-22 迁移漏掉的第四链

**现状**：I-22 丁案把三条链的 memmap 源收口到 `minix_boot::DtbMemoryMap`，但 aarch64 直核自引导面 `os/kernel-image/src/bootface_a64.rs` 仍保留**独立的**第四套：`fn fill_memmap`（L276 起，`all_nodes` 全树遍历 `/memory`）、`const fn clip_firmware`（L315 起，固件常驻区削界算术）、`static mut MEMMAP_STORAGE`（L141）。这与已迁入 `DtbMemoryMap` 的 `clip_reserved` 是**同一段算术的两份实现**——`memmap_source.rs` 的编译期护栏（`_FW_PURE`/`_FW_ABOVE`/`_FW_CROSS` 三条 assert）只护住库内那份，bootface_a64 这份的算术回归不会被任何测试拦住。模块头注自陈「两门全绿后的去重收敛留给独立清理增量」，本条把该增量立项。

**为什么是设计问题而不只是重复**：① memmap 源的语义出口在 I-22 裁决后只剩 `MemoryMapSource::collect()` 一处，bootface_a64 是绕过该出口的漏网链——下一次削界语义调整（例如 `/reserved-memory` 节点解析，丁案 B 预留项）会改库内不改这份，aarch64 直核腿静默漂移；② `static mut MEMMAP_STORAGE` 是 `os/` 下又一处 `static mut`（I-25 的同类形态，`memmap_source.rs` 迁移时已用 `static mut + AtomicUsize` 计数替代，bootface_a64 这份连计数都没有）；③ 顺带消灭 `MEMMAP_MAX = 8` 双定义（库内 `DTB_MEMMAP_MAX` 与 bootface_a64 各一份）。

**修法**：`bootface_a64.rs` 的 `fill_memmap`/`clip_firmware`/`MEMMAP_STORAGE` 整体删除，改调 `DtbMemoryMap::new(&fdt, fw_end).collect()`。`fw_end` 取值需裁决：bootface.rs（riscv）传 `KERN_PHYS_BASE`（OpenSBI 常驻区削除），bootface_a64 的直核腿 QEMU 不放固件常驻区于 DRAM（DTB 在 RAM 基址、镜像在 +2MiB），传 0 还是传 `KERN_PHYS_BASE` 需按 aarch64 直核的固件占位实况定（倾向 0 = 不削，理由写入裁决记录）。

**证据命令**：

```
grep -n "fn fill_memmap\|const fn clip_firmware\|MEMMAP_STORAGE\|DtbMemoryMap" os/kernel-image/src/bootface_a64.rs
# fill_memmap/clip_firmware/MEMMAP_STORAGE 各命中、DtbMemoryMap 零命中
grep -n "clip_reserved" os/libs/minix-boot/src/memmap_source.rs
```

**验收**：bootface_a64 构建（aarch64 target）通过；`test-kernel-image-riscv64-bootface` 类比测试或真机 aarch64 `-kernel` 直核冒烟通过；`grep -n "MEMMAP_STORAGE\|clip_firmware" os/kernel-image/src/bootface_a64.rs` 归零。**2026-10-21 外部首轮裁决：采纳，独立小批实施（五项中最先）；`fw_end=0` 可接受但必须带形态前提成文——「此引导腿不存在需固件前缀裁剪的区间」，且与内核侧 `cut_memmap`（镜像自占排除）是两个机制、不互相替岗；不为它单独引入策略抽象。**

**边界**：不合并 bootface.rs 与 bootface_a64.rs 的其余重复（名字池、常量——模块头注已登记为独立清理增量，超出本条）。

### D-69（新登记）`build_kernel_info` 双版本漂移面——同函数名、不同签名、语义分叉

**现状**：两个同名函数并存：

- `os/boot-shim/src/opensbi_helpers.rs:fn build_kernel_info`（8 参，**不含** `reserved_regions`，函数体内硬编码 `reserved_regions: &[]`）——被 bootface.rs（riscv）与 bootface_a64.rs（aarch64）两个直核面复用；
- `os/boot-shim/src/uefi_helpers.rs:fn build_kernel_info`（9 参，多一个 `reserved_regions: &'static [MemoryRegion]` 形参）——UEFI 腿专用。

名字相同、签名不同、UEFI 版多出的字段恰是「x86 内联执行模型特有的运行期恒等窗口」（01 篇 §3.4 `reserved_regions` 条目 + L1488 读者 TODO 标记的「x86_64 待退场」项）。风险：两个直核面若将来需要非空 `reserved_regions`（例如 aarch64 直核腿的 MMIO 窗口），改 opensbi 版签名会牵动两个调用面，而 UEFI 版的 `reserved_regions` 形参在 x86 切跨镜像后要退场——三处消费面共享一个名字却无统一形状，是 D-67 信封合一落地时必然撞上的缝合面。

**修法（随 D-67 同批）**：信封合一后 `build_kernel_info` 收敛为单一构造面——候选见简报 T1 §4（统一签名带 `reserved_regions` 形参 vs 拆 per-leg builder trait）。

**证据命令**：

```
grep -rn "fn build_kernel_info" os --include="*.rs"
grep -rn "opensbi_helpers::.*build_kernel_info\|build_kernel_info," os/kernel-image/src/
```

**设计载体**：`01-code-design.md` T1 §4。

### D-70（新登记）`store_kernel_info` 深拷落地区缺 `platform_sources` 与 `param_buf` 两块切片

**现状**：`os/kernel/src/lib.rs:fn store_kernel_info` 把三块切片（`reserved_regions`/`memmap`/`boot_modules`）深拷进 `.bss` 落地区（`globals.rs` 的 `RESERVED_REGION_STORE`/`MEMMAP_REGION_STORE`/`BOOT_MODULE_STORE` + `BOOT_MODULE_NAME_STORE` 字节池），复制的根因是 NK4-C 真机取证——boot-shim UEFI 池堆的页在首次切根后不再被内核页表重放（`reserved_regions`）或被当空闲复用清零（`memmap`，vh1 读数）。但 `KernelInfo` 有五块切片字段，剩下两块没落地：

- **`platform_sources`**：UEFI 腿由 `find_platform_sources()` `Box::leak` 进 UEFI 池堆（`os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources`）。当前安全的原因是**消费点全部在首次切根之前**（平台发现跑在 kmain Phase A–D，首次切根在 Phase F `switch_to_user` 之后）——「消费一次、时点恰好在映射失效前」是无结构保证的时序巧合，与 NK4-C 三块切片当年「Step 4 还完好、分类点读零」的发病时序同型。
- **`param_buf`**：当前恒空切片（D-66 未落地），但 `GET_MONPARAMS` 消费点 `os/kernel/src/misc.rs:fn monparams_snapshot` 在 syscall 期执行——D-66 一旦接入 UEFI 腿参数，param_buf 指向的池堆页在消费时点早已失效，是 **D-66 的硬前置**。

**修法（两案，简报 T5 展开）**：

- 案 1（推荐）：`globals.rs` 增 `PLATFORM_SOURCE_STORE`（`[PlatformDescSource; N]`，N=4 足够 DTB+RSDP）与 `PARAM_BUF_STORE`（`[u8; MULTIBOOT_PARAM_BUF_SIZE]`）两块落地区，`store_kernel_info` 同步落地两块——与既有三块同构，类型系统不再依赖「消费时点」论证。
- 案 2：显式化「consume-before-first-root-switch」契约——在 `platform_sources` 字段文档与消费点写明时序约束并加 debug 断言。省 264+1024 字节 `.bss`，但把不变量留给注释而非结构。

**证据命令**：

```
grep -n "fn store_kernel_info" os/kernel/src/lib.rs
sed -n '764,837p' os/kernel/src/lib.rs          # 落地区仅三块
grep -n "PLATFORM_SOURCE_STORE\|PARAM_BUF_STORE" os/kernel/src/globals.rs   # 应零命中
grep -n "monparams_snapshot\|param_buf" os/kernel/src/misc.rs | head
```

**验收**：案 1 落地后 `store_kernel_info` 五块切片全部 repoint 到 `.bss`；`test_store_kernel_info_lands_firmware_slices` 扩断言（platform_sources/param_buf 指针变化）；`cargo test -p minix-kernel --features mock` 全绿。**2026-10-21 外部首轮裁决：采案 1；`PLATFORM_SOURCE_STORE_LEN=4` 为候选值——须先核实「最多两源」是类型与生产路径的真实上限还是常见情况，容量作为有意设计的上限写入并配超容量测试；param_buf 落地保持「清零再复制」语义（幂等重存不得暴露上次输入遗留）；本条为 D-66 的硬前置，实施顺序 T4 → T1(wire) → **T5** → T3 → T2。**

### D-71（新登记）`BootForm::source_path()` 悬空引用 + 形态登记测试无存在性检查

**现状**：`os/libs/minix-boot/src/boot_form.rs:fn source_path` 给 `BootForm::Aarch64UefiCrossImage` 的落点是 `"os/boot-shim/src/handoff.rs"`——该路径**不存在**（`handoff.rs` 实际在 `os/libs/minix-boot/src/handoff.rs`，boot-shim crate 源目录只有 main/lib/loader/uefi_helpers/opensbi_helpers 五个文件）。既有测试 `test_each_boot_form_has_distinct_source` 只断言「非空 + 互异」，不检查文件真实存在，所以悬空引用不被测试拦截。`source_path()` 的模块级自述是「该形态在树上的落点……缺失即说明某形态被搬走或改名而台账未跟上」——这个自述对 Aarch64UefiCrossImage 已经失真，恰是它声称要防的病。

**修法**：① 路径改正为 `os/libs/minix-boot/src/handoff.rs`；② 测试补存在性断言（宿主测试可用 `env!("CARGO_MANIFEST_DIR")` 拼仓库相对路径检查 `Path::new(...).exists()`，五条落点逐一校验）；③ 顺带核对其余四条落点——`X86UefiInline`→`os/boot-shim/src/main.rs`、`RiscvOpensbiSelfBoot`→`os/kernel-image/src/bootface.rs`、`Aarch64QemuDirectCore`→`os/kernel-image/src/bootface_a64.rs`、`RiscvEarlyHartMailbox`→`os/arch/src/riscv64/ap_early_entry.rs`（grep 复核后如有第二处悬空一并修）。

**证据命令**：

```
ls os/boot-shim/src/handoff.rs; ls os/libs/minix-boot/src/handoff.rs
grep -n "Aarch64UefiCrossImage =>" os/libs/minix-boot/src/boot_form.rs
grep -rn "fn source_path" os/libs/minix-boot/src/boot_form.rs
```

**验收**：`test_each_boot_form_has_distinct_source` 扩存在性断言后通过；`cargo test -p minix-boot` 全绿。

---

## 4. C 批——注释/登记回修（独立小修，可直修）

### I-26（新登记）`opensbi_helpers.rs` 过时注释：孤儿 doc 注释 + "hardcoded" 顶注与 I-22 实况矛盾

**现状**：两处：

- `os/boot-shim/src/opensbi_helpers.rs` L61–L64：`/// QEMU virt DRAM base address.` 与 `/// Default QEMU virt RAM size (128 MB).` 两条 doc 注释后面**没有常量**——I-22 丁案 C 删除 `DRAM_BASE`/`DEFAULT_RAM_SIZE` 时漏删了它们的注释，成为孤儿 doc 注释。
- 同文件 `impl BootShim for OpenSbiBootShim` 的 doc 注释（L185–189 区段）仍写 "Memory map → hardcoded (no SBI call to enumerate RAM; QEMU virt is fixed)"，而同函数体 L205–223 已是 `DtbMemoryMap::collect()`——注释与代码在同一函数内自相矛盾。这正是 `todo.md` I-22 连带清单 ④ 要改的位置，落地时漏改。

**修法**：删两条孤儿注释；顶注 "Memory map → hardcoded…" 改为 "Memory map → `DtbMemoryMap::collect(dtb_ptr())` (DTB `/memory`; missing DTB halts)"。顺带 grep 全文件确认无第三处 "hardcoded" 残留（L376 区段 "no firmware to call; everything is hardcoded" 的分节标题按实况收窄为「分配与取件无固件服务」——memmap 已不是 hardcoded）。

**证据命令**：

```
grep -n "hardcoded\|QEMU virt DRAM base address\|Default QEMU virt RAM size" os/boot-shim/src/opensbi_helpers.rs
```

**验收**：上述 grep 仅剩语义正确处；`cargo check -p boot-shim`（riscv64 target 或 opensbi feature）通过。

### I-27（新登记）01 篇两处「跳转协议还没裁决」与 OQ-N6 已裁口径不一致

**现状**：`01-boot-shim-bootstrap.md` L1422（§3.4）「跨镜像跳转协议还没裁决（NK1/OQ-N6，§4.8 交付边界）」、L2624（§4.8）「跳转协议尚未定案（镜像头注释如此说明）」。而 `rewrite-notes/coordination/TODO-LEDGER-DONE.md` §2 记 OQ-N6 已裁（每进程独立根原语、加栈顶参数快案被否）。两处「还没裁决」应改为「OQ-N6 已裁决（每进程独立根原语，aarch64 的 `BootHandoff` 交接即其实现）；x86 腿从内联切换到该形态的实施收口是 NK1（C-29），待认领」。`os/kernel-image/src/main.rs` 头注「交付边界（诚实登记）」段的同款表述（"是 NK1（C-29，OQ-N6 设计轮）的裁决范围，这里不代决"）语义正确但可顺一句「OQ-N6 已裁」。

**连带**：与 L1424 的张力登记（§3.1/§3.3 终态叙述 vs §3.4 过渡形态标注，见 E 批）同批处理——三处都动 §3/§4 的「现状 vs 终态」叙述，一次改完避免 diff 冲突。

**证据命令**：

```
grep -n "还没裁决\|尚未定案" rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
grep -n "每进程独立根" rewrite-notes/coordination/TODO-LEDGER-DONE.md
```

---

## 5. D 批——测试缺口（01 篇 §5.3 TODO 交接表现状复核）

> 01 篇 §5.3 的七条以该表为唯一登记处；本节逐条复核后的现状如下，实施时回填 01 篇 §5.3 的「覆盖状态」列与本表的验收记录。优先级参照 `todo.md` §7.3 T 系惯例（T-8 QEMU 半挂 CI、真机项随硬件窗口）。

| ID | 机制 | 断言 | 复核现状（2026-10-20） | 去向 |
|---|---|---|---|---|
| TODO-01 | M4 深拷先于一切动用空闲清单者 | 三入口在首个页分配前完成 `store_kernel_info` | 仍 open：`test_store_kernel_info_lands_firmware_slices`（`os/kernel/src/lib.rs` 测试区）只测落地区行为与幂等，调用序无自动断言；x86 入口 `arch_boot` L224 的 `store_kernel_info` 先于 `arch_boot_impl`（首个分配点）属实但靠人读 | 宿主可修：三入口（x86/aarch64 inline、aarch64 resume、riscv）各自补一条「入口首个动作 = store」的断言或测试（mock 下调用序记录） |
| TODO-02 | M7 分配器写一次 | 二次 `register` debug 失败、release 拒改写 | 仍 open：`grep -c "#\[test\]" os/arch/src/arch/pt_alloc.rs` = 0，该模块零测试 | 宿主可修：`register` 二次调用断言 + `is_registered`/`register_free` 契约测试 |
| TODO-03 | M8 落地区容量与 fail-fast | 超容量输入 fail-fast（不截断） | **半开**：三块落地区的 `exceeds the .bss landing pad` 断言在 `lib.rs:781/:799/:809`，无测试构造溢出；落地区**行为**已有测试 | 宿主可修：构造超容量 `memmap`/`boot_modules` 断言 panic 消息（D-70 落地后随第五块补） |
| TODO-04 | M12 EBS 阶段边界 | EBS 后路径不出现 `uefi::` 调用 | 仍 open：无 CI 静态检查；`uefi_helpers.rs:fn exit_boot_services`（L536）之后的代码路径人工审计过（`prepare_boot` 尾部 raw_serial + 返回），但没有机制防回归 | 半宿主可修：lint 脚本或测试断言 `exit_boot_services` 调用点之后的行不含 `uefi::` 符号（文本级检查即可起步） |
| TODO-05 | §4.6 大页回退档 | 2MiB 回退档下真机启动 | 仍 open：真机矩阵依赖（x86 产线 kern 布局按 2MiB 对齐即走回退档；当前真机链走 1GiB 档） | 真机窗口项，随 x86 跨镜像切换（NK1）一并验 |
| TODO-06 | M14 x86_64 路标链 | `kernel loaded`/`12 boot modules loaded`/`handing control` 三路标出现 | 仍 open：`os/qemu-tests/run_all.sh` x86 段只判 `TEST_RESULT`，不判三条 shim 路标 | CI 可修：x86 测试脚本的串口判定加三行 grep |
| TODO-07 | §4.2 UEFI 侧宿主可测面 | `build_memmaps` 两列表互斥完备、root/bump 不与 free 相交 | 仍 open：`build_memmaps` 调用点单根（Boot Services），宿主无法构造；`assert_bootstrap_outside_memmap` 是纯函数可直接测但无测试 | 半宿主可修：`assert_bootstrap_outside_memmap` 的三案（相交/不相交/边界贴合）纯函数测试先行；`build_memmaps` 的过滤逻辑抽纯函数后同批补 |

---

## 6. E 批——文档侧（01 篇正文条目，改文档不动码）

| 条目 | 内容 | 与谁同批 |
|---|---|---|
| I-23 | §3.7 补三形态引导矩阵与 `fw_payload` 真板用途段（~10 行 + `[ARCH]` 三处一致）——详情以 `todo.md` §7.4 I-23 为权威 | I-22 文档连带、I-24 |
| I-24 | §3.1「内核自带 stub」否决行重写为「UEFI 形态被否 / opensbi 形态采纳」两行——详情以 `todo.md` §7.4 I-24 为权威 | I-22、I-23 |
| L1424 张力登记 | §3.4 的 TODO 块：§3.1/§3.3 终态叙述缺「当前产线形态与终态的差距见 §3.4」指针；候选 ① 补指针（随 I-27 一起改）或 ② 等 NK1 落地后删括注 | I-27、NK1 |
| I-27 | L1422/L2624「还没裁决」口径对齐 OQ-N6 已裁（本文件 §4） | L1424 |
| I-25 | `os/` 全仓注释语言/锚点整改——低优，权威在 `todo.md` §7.4 I-25；本轮审查确认的同类新样例：`memmap_source.rs`/`boot_form.rs` 的中英混排注释、`opensbi_helpers.rs` 协作锚点（NK4-C 续-88 等）按 I-25 口径 a/b 处理 | 全仓批 |
| 读者待办 L1488 | 「TODO，x86_64 的待退场，此处待核实，并透彻学习」——`reserved_regions` x86 内联窗口随 x86 切跨镜像退场的路径。**用户侧学习项**，AI 不代决；可回读 `01-code-design.md` T2（x86 切换后 UEFI 腿 `reserved_regions` 退场的具体形态）辅助 | NK1 |
| 读者待办 L1532 | §3.4 末「TODO 这个是TODO是交互式文档类TODO，我读不懂？？」——该 TODO 块（L1424）的性质是**登记文档-代码张力**（不是代码任务）：正文叙述以终态口径写、代码实况是过渡形态，两者差距要求正文标注。处理方式见上一行。**用户侧澄清项** | L1424 |

---

## 7. 验收总则（每条通用）

1. **证据先行**：动手前跑该条「证据命令」确认现状；与工单描述不一致时以源码为准并在回报里写明偏离。
2. **裁决先行**：B 批各条以 `01-code-design.md` 对应节与 GPT 的裁决结论为实施前提；裁决记录回填条目下方，不另开文档。
3. **文档同步**：涉 01 篇正文的（D-66 §3.4 param_buf 表述、D-67 §3.4 两信封叙述、I-27/L1424/L1422、E 批各条）同批更新；§5.3 交接表回填覆盖状态。
4. **门禁**：`cargo test -p <crate>` 与 `cargo clippy` 通过；共享契约层（minix-boot）条目加三架构构建与 qemu 引导冒烟；文档五门（anchor/style/ref-check/density/code-map）在受影响文档复跑全绿。
5. **不跳过 hook、不 `git add -A`**：逐路径点名提交；一条逻辑单元一笔。
