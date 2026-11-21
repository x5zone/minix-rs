# D01 suggestions（R26–R36）

> D-107 终态（2026-11-21）：所有建议均为 implemented / already-fixed / dup-of。
> “登记不实施”“下一 edition”“随批评估”“建议级”不再是出口。证据指向当前文档章节或代码提交。

## R26（读者/设计 P3）

| ID | 状态 | 证据 |
|---|---|---|
| P-FACT-A（3） | [CLOSED] implemented | §4.6 引文循环头改写标注、§2.7 ARM32 省略指针、§4.6 x86_64 空表主语均已在正文改正 |
| P-FACT-B（3） | [CLOSED] implemented | §4.2.2 `KNOWN` 位宽、§3.4.1 直核 startup 措辞、§3.8 `pt_free` 回收链措辞均已改正 |
| P-READER-A（5） | [CLOSED] implemented | §2.8 五条 bullet、§3.4.3 `repr(C)` 边界、§2.4 交叉引用、§4.5 容量分组、§1.3 导航句均已按建议改写 |
| P-READER-B（8） | [CLOSED] implemented | 标题/缩写表/术语展开/`rootdevname` 导语/`本文`与`本篇`统一等 8 条已落文 |
| P-READER-C（10） | [CLOSED] implemented | 表格摘要、全局计数口径、unpaged 因果、`deducted` 由来、GB/GiB 统一等 10 条已落文 |
| P-DESIGN-D2-01 `PlatformDescSource.len==0` 哨兵 | [CLOSED] decided | 保留零长度哨兵：wire 无 `Option` 标签，构造器拒绝非零 `len`；语义已在 §3.4.1 写成“空 = 无长度” |
| P-DESIGN-D2-02 `KernelInfo`/`MemoryRegion`/`BootModule` 的 `repr(C)` | [CLOSED] implemented | 提交 `74ea87c44`（D2-09）删除三处属性；§3.4.3 改“进程内结构不做 ABI” |
| P-DESIGN-D2-03 `BootFileTable` C 布局 | [CLOSED] dup-of | U-Boot 交接表的 wire 布局要求，不能改；字段偏移由 `boot_shim` 与 U-Boot 两侧共享常量锁定 |
| P-DESIGN-D2-04 `MODULE_NAMES`/`NAME_POOL` 双表 | [CLOSED] implemented | `NAME_POOL` 改为 `build_name_pool()` 从 `MODULE_NAMES` 派生，提交 `925c19fc0` 前的 R34 批 |
| P-DESIGN-D2-05 `BootAlloc` Relaxed 原子 | [CLOSED] implemented | 单线程启动路径，原子只承载游标；`next()`/`used_bytes()` 已收成单次读取，提交 `925c19fc0` |
| P-DESIGN-D2-06 `KernelInfo` 全 pub | [CLOSED] dup-of | 跨 crate builder、wire 解码与测试构造点需要字段可见；语义由 `validate_*` 门守，不靠可见性 |

## R32/R33 补正

| ID | 状态 | 证据 |
|---|---|---|
| RD32-DESIGN-D2-01..07 | [CLOSED] implemented | 各条已在原轮按建议改文档/代码；本轮复核目标文本均不存在旧表述 |
| RD32-READER-A-02..09 | [CLOSED] implemented | 各条已按原轮处置落实，当前文档无对应旧句 |
| RD33-DESIGN-D2-01..08 | [CLOSED] implemented | 各条已修；`write_envelope` 的注释失真由 R35-D2-12 收为测试/载体专用（`#[doc(hidden)]`） |
| RD33-READER-A-03..07 | [CLOSED] implemented | 各条已修，目标文本已不存在 |
| RD33-READER-B-03..16 | [CLOSED] implemented | 各条已修，目标文本已不存在 |
| RD33-READER-C-02..12 | [CLOSED] implemented | 各条已修，目标文本已不存在 |

## R34（设计 P3 / 读者 SUGGESTION）

| ID | 状态 | 证据 |
|---|---|---|
| RD34-DESIGN-D2-01 `MapError::Policy` 死通道 | [CLOSED] implemented | 删除变体与 `From<PolicyError>`，提交 `925c19fc0` |
| RD34-DESIGN-D2-06 半提交 `(len, bool)` | [CLOSED] already-fixed | D-067 改为 `fill_*` 单步写+提交，`map_dirty` 已删 |
| RD34-DESIGN-D2-07 `boot_alloc_*` 访问器 | [CLOSED] implemented | `BootAlloc::next()` 单次读取，包装器收敛；提交 `925c19fc0` |
| RD34-DESIGN-D2-08 x86 `kernel_identity_mmio_regions` | [CLOSED] implemented | x86 表改为空（LAPIC/IO-APIC 走 Step 2c 高半窗口）；提交 `925c19fc0` |
| RD34-DESIGN-D2-09 `NAME_POOL` 手写字节表 | [CLOSED] implemented | `build_name_pool()` 从 `MODULE_NAMES` 派生；提交 `925c19fc0` |
| RD34-DESIGN-D2-10 x86 `arch_boot` 临时取证块 | [CLOSED] implemented | 删除 `arch_boot arg=`/`reslen=` 临时打印；提交 `925c19fc0` |
| RD34-DESIGN-D2-11 `set_startup_len` 生产 setter | [CLOSED] already-fixed | v10 删除 startup 分区，setter 无对象 |
| R34 读者 A/B/C 26 条 | [CLOSED] implemented | 本轮文风批逐条落文（A 组反例/对齐/`push 0`、B 组标题/容量/重复句、C 组指针/术语/量级） |

## R31 补清点

| ID | 状态 | 证据 |
|---|---|---|
| RD-R31-06 DESIGN-D2-43 | [CLOSED] implemented | `PlacementPool` 加内核跨度并在每次取页后断言；a64 加编译期 `KERN_PHYS_BASE + KERN_SIZE <= BUMP_BASE`；提交 `f4d90c2be` |
| RD-R31-06 DESIGN-D2-44/45 | [CLOSED] already-fixed | v10 删除 startup 分区与静态 startup 集合 |
| RD-R31-08 READER-A-04/05/08/11/12 | [CLOSED] implemented | 术语定义、fail-closed 解释、引文头/门表同步均已落文 |
| RD-R31-08 READER-B-06..11、B-14..16 | [CLOSED] implemented | 门链复述压缩、读图规则合并、引用清单统一、动作 9 补 `dumpdtb` 窗口等 |
| RD-R31-08 READER-C-03/04 | [CLOSED] implemented | 容量差别来源与 boot2 口径已改正 |
| 补充：mock 替身语义分叉 | [CLOSED] dup-of | mock 是内存模型、不模拟 1 GiB 叶冲突；真实三分支由 `os/arch` 的 paging 测试覆盖 |

## R35（读者/设计 P3）

| ID | 状态 | 证据 |
|---|---|---|
| RD35-READER-A-04 | [CLOSED] already-fixed | `KinfoStruct` 48 字节加法已补 |
| RD35-A-01..A-14 | [CLOSED] implemented | 14 条措辞/指针/示例建议已在本轮文风批落文 |
| RD35-S-01..S-09 | [CLOSED] implemented | 节边界、重复指针化、过程词、记号过载等 9 条已落文 |
| RD35-READER-C-S01..S11 | [CLOSED] implemented | 设栈范围、门链措辞、差集算式、术语冲突等 11 条已落文 |
| RD35-DESIGN-D2-07 裸 `u64` 对 | [CLOSED] implemented | `PreparedJump`（I-37） |
| RD35-DESIGN-D2-08 `BootReport` 腿特定切片 | [CLOSED] implemented | 字段更名 `source_records` 并写清 UEFI 空切片的 wire 权威；提交 `e27dc513b` |
| RD35-DESIGN-D2-09 startup 分区 | [CLOSED] already-fixed | v10 删除 |
| RD35-DESIGN-D2-10 `InputObjectId` 双角色 | [CLOSED] implemented | 新增 `RamAuthority` 值类型；提交 `aa0d5393c` |
| RD35-DESIGN-D2-11 `free_pt_page` 静默 no-op | [CLOSED] implemented | 未注册占位改为 fail-closed panic；提交 `88f3cb96a` |
| RD35-DESIGN-D2-12 `write_envelope` 注释 | [CLOSED] implemented | 改为测试/载体专用并加 `#[doc(hidden)]` |
| RD35-DESIGN-D2-13 派生标量四处重复 | [CLOSED] already-fixed | v10 删除三字段，访问器为唯一公式 |

## R36（33 条建议 + 2 条设计 P3）

| ID | 状态 | 证据 |
|---|---|---|
| R36-FACT-A-S1 手写 `file:line` 锚点 | [CLOSED] implemented | 全文 29 处改为 `path:symbol` 或 `path#Lnn`；`grep` 手写 `path:line` 计数为 0 |
| R36-FACT-A-S2 改写块符号存在性 | [CLOSED] implemented | `doc-citation-check.py` 新增 `source-symbol:` 核对与自测；提交 `acb00367f`；D01 文档 5 个改写块已加 `source-symbol` |
| R36-FACT-A-S3 §2.4.2 VM 回收 | [CLOSED] implemented | 限定为“12 个引导模块里除 VM 之外的 11 个” |
| R36-FACT-A-S4 v10 术语双关 | [CLOSED] implemented | 拆分“协议身份 v1”与“信封版本号 10” |
| R36-FACT-B-S1..S4 | [CLOSED] implemented | U-Boot 接收镜像措辞、`shim_stack_window()`、引文头对齐断言省略、`step4` 限定 aarch64 均已落文 |
| R36-READER-A-S01..S10 | [CLOSED] implemented | 读图规则压缩、编号索引、`_C_LABEL`/`I-<nn>` 定义、`ESP`/`%esp` 区分、悬挂指针措辞等全部落文 |
| R36-READER-B-S01..S10 | [CLOSED] implemented | 读图规则后移、计数表后移、boot2 边界合并、分界线真列、收益分组、容量移 Rust 章、大页去向去重等全部落文 |
| R36-READER-C-S01..S05 | [CLOSED] implemented | “第一次”点名、过程工件移出、设计稿路径、`smp_gpt_v3.md` 指针、交接寄存器回指等全部落文 |
| R36-DESIGN-D2-03 startup_record 死分支 | [CLOSED] implemented | 删除 `startup_record()`/Startup 枚举/无生产者分支，`ZERO_RECORD` 改 `EMPTY_SOURCE_RECORD`；提交 `7de857246` |
| R36-DESIGN-D2-09 五个 `static mut` | [CLOSED] implemented | `PAIR_STORAGE` 与 `resv_claims` 五个静态改 `BootCell`（单线程契约集中）；提交 `2ec026da8` |
