# 01-stage 01 R2 缺陷台账（V3.12，post-DESIGN-02）

- edition: V3.12
- 轮次: R2 full
- 基线: doc sha `19c0ad9a…`（2,951 行），commit `76ac760b4`
- 面板 FINDING 25：P2 11 / P3 14；P-REFUTE 2 个 P2 观察（其余反证失败）。
- 处置：① 23 / ⑤ 2（DESIGN-01/02 待作者）/ FP 0；未闭环 2，轮结果 `BLOCKED`。

| 发现 ID | 单元 | 处置类 | 处置 | 复验 | 根因 ID | origin |
|---|---|---|---|---|---|---|
| READER-A-02、READER-B-02 | 4.8 | ① | §4.8 的 v1 链改为 `resume_additive` 内 `boot_validate_and_prepare → store_kernel_info → establish_boot_dm → jump_to_kmain`；“两条腿”改“两组腿”。 | rg -n 'v1 接收面在 `resume_additive` 内是' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-01 | R2 |
| READER-C-02 | 4.5 | ① | §4.5 开头改为“`arch_boot_finish` 在前置门之后深拷”，不再说第一步是深拷。 | rg -n '`arch_boot_finish` 在前置门之后把发送侧' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-02 | R2 |
| READER-A-01、READER-B-03 | 4.1, 3.2.3 | ① | §4.1 的 `BootWindows` 指针改指 §3.6 窗口组装 + §4.4 窗口边界；§3.2.3 的 source 规范集合指针改 §3.3。 | rg -n '窗口构造点见 §3.6' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1）；rg -n '§3.3 的 source 规范集合' 同文件（期望 1） | R2-03 | R2 |
| READER-A-03 | 3.1 | ① | §3.1 UEFI 识别可执行文件格改为“PE/COFF（boot-shim 自身；被装载的 kernel.elf 仍是 ELF）”。 | rg -n 'boot-shim 自身；被装载的 `kernel.elf` 仍是 ELF' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-04 | R2 |
| READER-A-04 | 3.2 | ① | §3.2 的“跨镜像 / 直核 / 三形态”改为“三种交接形态（同镜像内调用 / 跨镜像跳转 / 直核自引导）”。 | rg -n '三种交接形态（同镜像内调用' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-05 | R2 |
| READER-C-01 | 1.1, 2.3 | ① | §2.3 的“§1.1 那两条依赖”改为“那条依赖”。 | rg -n '这正是 §1.1 那条依赖的根据' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-06 | R2 |
| READER-C-03 | 3.4.1 | ① | §3.4.1 的生产镜像清单补 aarch64 直核 `fw-aarch64-none`，判据结论覆盖五条生产腿。 | rg -n 'aarch64 直核镜像 `fw-aarch64-none`' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-07 | R2 |
| READER-B-01 | 2.1 | ① | §2.1 的 load_mods 反序段拆成“正常分配”与“模块表分配失败”两句。 | rg -n '正常分配下反序：表仍会建' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-08 | R2 |
| DESIGN-01 | 3.6, 4.3 | ⑤ | 待作者裁决：是否把 `MODULE_NAMES` 与 `BOOT_MODULE_PROC_NRS` 合并成一张 `{name, proc_nr}` 表。案件卷 `R2-DESIGN-CASE.md`。 | 待裁决；当前两张表的位置关系由 §4.3 反事实与测试描述覆盖 | R2-11 | DESIGN-D2 R2 |
| DESIGN-02 | 3.2, 4.4.2 | ⑤ | 待作者裁决：`kmain` 的 `validate()` 是否改 `validate_requiring_sources()`。案件卷 `R2-DESIGN-CASE.md`。 | 待裁决；当前 `kmain` 走非 strict `validate()` | R2-12 | DESIGN-D2 R2 |
| READER-A-05 | 3.5.1 | ① | §3.5.1 的 `prepare_boot` 统一为“不属于任何 trait 的自由函数”。 | rg -n '`prepare_boot` 是不属于任何 trait 的自由函数' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-13 | R2 |
| READER-A-06 | 3.2.1 | ① | §3.2.1 的字段计数补全 16 → 13 → 14 → 10 链条。 | rg -n '字段计数的完整链条是 16' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-14 | R2 |
| READER-C-04 | 4.5 | ① | §4.5 的 wrapper 引文注明“一次性守卫在 land_and_store 入口，本引文省略”。 | rg -n '一次性守卫在 land_and_store 入口' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-15 | R2 |
| READER-C-05 | 3.2.2 | ① | §3.2.2 开头补三套数字（22 / 16→13→14→10 / 现役 10）的口径与交集。 | rg -n '**先分清三套数字**' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-16 | R2 |
| READER-C-06 | 1.1 | ① | §1.1 “内核位置记录”的“为什么必须交接”格补原因：接收侧无 ELF 头，跨度由发送侧解析后交来。 | rg -n '接收侧没有 ELF 头可读' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-17 | R2 |
| READER-C-07 | 4.6 | ① | §4.6 的 bump 整段扣除补“UEFI 腿空操作、自引导/U-Boot 腿实际扣”。 | rg -n 'UEFI 腿这条是空操作' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-18 | R2 |
| READER-C-08 | 4.5 | ① | §4.5 容量表补分类器 wire 容量 `WIRE_MEMMAP_CAP=128`、`WIRE_RESERVED_CAP=256` 与 fail-closed。 | rg -n 'WIRE_MEMMAP_CAP = 128' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-19 | R2 |
| READER-C-09 | 6.1 | ① | §6.1 的“§5.5 未闭环项”改为“§5.5 的已知诊断边界（已裁决不增打印）”。 | rg -n '已知诊断边界，已裁决不增打印' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-20 | R2 |
| READER-C-10 | 5.1 | ① | §5.1 补四个计数的成员集合（生产腿/门链/有门链的腿/aarch64 直核 SMP 门）。 | rg -n '四个计数的成员不同' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-21 | R2 |
| DESIGN-03 | 3.7 | ① | §3.7 的“半提交 → 第二次 fill panic”改为“重复提交 → 第二次 fill panic；半提交不可表示”。 | rg -n '重复提交 → 走 `fill_\*` 的腿第二次调用 panic' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-22 | DESIGN-D2 R2 |
| DESIGN-04 | kernel_info.rs | ① | `KernelInfo::source_audit()` 改为 `#[cfg(test)]`；删除无消费者的 `Default for BootStorage`/`Default for BootAlloc`。 | rg -n 'pub fn source_audit|impl Default for BootStorage|impl Default for BootAlloc' os（期望 0） | R2-23 | DESIGN-D2 R2 |
| DESIGN-05 | handoff_v1_write.rs, uboot_main.rs | ① | 删除三处 startup 分区/记录残留注释；`(d73)` 串口串保留（QEMU 脚本依赖），记 FP。 | rg -n 'startup-object|named by a startup' os/libs/minix-boot/src/handoff_v1_write.rs os/boot-shim/src/uboot_main.rs（期望 0） | R2-24 | DESIGN-D2 R2 |
| FACT-01 | 3.4.2, 4.2 | ① | 两处“代码引用/节选”块注明代码表达式按语义重排、非逐字。 | rg -n '代码行按语义重排，非逐字|代码表达式也按语义重排' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 2） | R2-25 | P-FACT R2 |
| REFUTE-02 | 1.2, 5.1 | ① | I4 明确限定为接收侧；v1 发送侧 `prepare_kernel_image_v1` 的窗口消费不在门链内。 | rg -n '校验先于消费（接收侧）' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R2-26 | P-REFUTE R2 |
| REFUTE-06 | 1.4, 3.7 | ① | §1.4 栈顶分工补“名字与初始分工归 02、01 只展开接收侧语义”；§3.7 的 `fill_source_records` 改为 U-Boot shim 与两个直核腿使用、UEFI 腿不走。 | rg -n 'names 与初始分工归 02|`fill_source_records` 在 U-Boot shim 与两个直核腿使用' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 ≥1） | R2-27 | P-REFUTE R2 |

## P-REFUTE 附加

| claim ID | 结果 | 处置 | 根因 |
|---|---|---|---|
| REFUTE-01（CLAIM 01） | 反证失败（I1/深拷成立） | FP；不改文档 | — |
| REFUTE-02（CLAIM 02） | 非阻断 P2：I4 未覆盖 v1 发送侧窗口消费 | ① 限定接收侧并写明发送侧缺口 | R2-26 |
| REFUTE-03（CLAIM 03） | 反证失败（UEFI 预留与扣除成立） | FP；不改文档 | — |
| REFUTE-04（CLAIM 04） | 反证失败（alloc_lowest 第三行成立） | FP；不改文档 | — |
| REFUTE-05（CLAIM 05） | 反证失败（ARM 闭区间成立） | FP；不改文档 | — |
| REFUTE-06（CLAIM 06） | 非阻断 P2：§1.4 边界句与 §3.7 fill_source_records 作用域各一处 | ① 两处都改写 | R2-27 |

## 建议（不进入主表）

| 建议 ID | 来源 | 处置 |
|---|---|---|
| READER-A/B/C/D2/FACT SUGGESTION | R2 面板 | 26 条按读者建议处理：本轮修了与 P2/P3 同段的项；纯风格/重复项留 R3，不冒充完成 |
