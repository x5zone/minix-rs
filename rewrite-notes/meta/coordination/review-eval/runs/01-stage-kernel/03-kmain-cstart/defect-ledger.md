# defect-ledger

| 根因 ID | 发现 ID | 单元 | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|---|

## R1 full（V3.11，2026-11-22）

| 根因 ID | 发现 ID | 单元 | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|---|
| RD-D03-R1-001 | FACT-02、REFUTE-1 | 4.4 | R1 | P-FACT/P-REFUTE | P1 | 文档/事实 | ① | 删除 set_gate_ist 旧段；set_handler 自己写 IST 2→1/8→2 | trap_entry.rs:330-343 | rg `set_gate_ist` 零命中 |
| RD-D03-R1-002 | FACT-01、REFUTE-2 | 3.7 | R1 | P-FACT/P-REFUTE | P1 | 文档/事实 | ① | 真机载体确实调 install_trap_stubs；改正文 | test-protection/main.rs:85-89 | rg `真机载体.*install_trap_stubs` 命中 |
| RD-D03-R1-003 | FACT-03、RA-F5 | 4.4 | R1 | P-FACT/READER-A | P2 | 文档/计数 | ① | C 39 门不含 0xFF/0xF0/0xF1；Rust 41 门另加减 | protect.c 16+23 | rg `不含.*0xFF/0xF0/0xF1` 命中 |
| RD-D03-R1-004 | FACT-04 | 4.2 | R1 | P-FACT | P2 | 文档/引文标注 | ① | 省略号说明补 program_entry_base 与 Step2 | lib.rs:2398-2412 | rg `program_entry_base.*Step 2` 命中 |
| RD-D03-R1-005 | FACT-05 | 5.1 | R1 | P-FACT | P2 | 文档/测试计数 | ① | 三条→四条源码钉，补 BSP 身份 | init_protection_order_pin.rs 4 个 test | rg `四条启动顺序源码钉` 命中 |
| RD-D03-R1-006 | FACT-S1 | 2.4 | R1 | P-FACT | P3 | 文档/计数 | ① | 五个调用点→五处命中（四处调用+宏内） | grep tss_init 五处 | rg `五处命中` 命中 |
| RD-D03-R1-007 | RA-F2 | 2.1 | R1 | READER-A | P2 | 文档/前提 | ① | bss_test 限定“同一镜像再次进入”；重新装载会再清零 | §2.3 对照 | rg `同一镜像被再次进入` 命中 |
| RD-D03-R1-008 | RA-F4 | 2.1 | R1 | READER-A | P2 | 文档/术语 | ① | 首次出现 _TABLE 补定义 | glo.h EXTERN 宏 | rg `_TABLE.*惯例开关` 命中 |
| RD-D03-R1-009 | RA-F6 | 2.8 | R1 | READER-A | P3 | 文档/引用 | ① | §4.7 指针改为实际位置 | §4.7 无 smp_init | rg 新指针命中 |
| RD-D03-R1-010 | RA-F3 | 4.1 | R1 | READER-A | P2 | 文档/证据强度 | ① | 别名覆写补“未单独定位；按预防性不变量处理” | 01 篇 §3.2.4/§5.3 | rg `覆写者在当前仓库没有单独定位` 命中 |
| RD-D03-R1-011 | RC-C2 | 2.0、6.1 | R1 | READER-C | P1 | 文档/时序 | ① | lgdt 后取指安全；#GP 在 CS 重载或 ltr；tss_init 首坏点改 ltr | §2.5/§6.1 | rg `lgdt 之后取指仍使用缓存的旧 CS 描述符` 命中 |
| RD-D03-R1-012 | RC-C3 | 2.5 | R1 | READER-C | P2 | 文档/矛盾 | ① | “不可类比”限定取指轴；异常递送轴同型 | §2.5 相邻两段 | rg `异常递送轴.*同型` 命中 |
| RD-D03-R1-013 | D2-A | 3.1、4.7 | R1 | P-DESIGN-D2 | P0 | 文档/陈旧锚点 | ① | 删除不存在的 LDT 注释引文 | os/arch/src/x86_64/protection.rs:451 | rg `LDT descriptor is filled` 零命中 |
| RD-D03-R1-014 | D2-B | 3.1、4.4 | R1 | P-DESIGN-D2 | P1 | 代码/ARCH 标注 | ① | protection.rs IST 注释补 [ARCH: IST 取代 i386 同栈嵌套] | protection.rs:248-256 | rg `IST replaces i386` 命中 |
| RD-D03-R1-015 | D2-C | 3.2、4.2 | R1 | P-DESIGN-D2 | P2 | 设计取舍 | FP | 非 x86 refresh_descriptors no-op 不可达抽象：现有 cfg 是 x86 专用搬迁收尾，aarch64/riscv 无此步；不改 | lib.rs:2405；protection.rs 三架构 | 记录为设计候选，不阻断 |
