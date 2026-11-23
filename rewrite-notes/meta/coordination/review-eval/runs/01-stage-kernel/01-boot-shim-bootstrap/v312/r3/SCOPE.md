# D01 内容范围裁决（V3.12）

## 文档

- `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 标题：01 · 引导交接：载荷、内存账与五条生产腿
- 规模：2,946 行；48 个单元（6 章 + 42 节）

## 输入

1. 当前文档正文（唯一讲解对象）。
2. 关联代码清单：`r1-post-design02/related-code.txt`，49 个 Rust 文件（较旧清单新增
   `os/arch/src/x86_64/ap_early_entry.rs`、`os/kernel/src/proc_table.rs`、`os/servers/rs/src/trap_api.rs`），覆盖
   `os/boot-shim/`、`os/kernel-image/`、`os/libs/minix-boot/`、`os/libs/minix-types/`、
   `os/arch/`、`os/kernel/` 的启动交接面、`os/plat/early_console.rs`、`os/servers/vm/` 的
   `boot.rs`/`alloc_page.rs`/`vm_server.rs`。
3. Minix3 C 真值：文档引用的 `minix3/minix/kernel/`（`main.c`、`table.c`、`glo.h`、
   `arch/i386/head.S`、`arch/i386/pre_init.c`、`arch/i386/pg_utils.c`、`arch/earm/` 等）、
   `minix3/sys/arch/i386/stand/`（`boot2.c`、`exec.c`、`biosmemx.S`）、
   `minix3/sys/arch/x86/include/` 与 `minix3/minix/servers/vm/` 的相关片段。
4. `DECISIONS.md` 中与本篇相关的冻结决策；冲突时以 C 源码行为与 `DECISIONS.md` 为准。
5. 不使用 V3.11 的旧面板、旧台账、旧冻结证据、旧 CONVERGED 结论。

## 最终范围

- 第 1 章 概述（含 1.0–1.4）：1 个章单元 + 5 个节单元。
- 第 2 章 C 源码分析（2.0–2.8）：1 + 9。
- 第 3 章 Rust 设计决策（3.0–3.7）：1 + 8。
- 第 4 章 实现详解（4.0–4.8）：1 + 9。
- 第 5 章 测试（5.0–5.5）：1 + 6。
- 第 6 章 诊断（6.0–6.4）：1 + 5。
- 合计 48 个单元；全部纳入 R1 full survey。

第 7 章「参见」是索引与入口清单，不进入单元账本（`review-units.py` 的 48 个单元只含 6 章 + 42 节）。
它仍受文档级机械门（引用、锚点、符号漂移）检查；若发现内容问题，按文档级发现处理，不单独建单元。

## 不在范围

- `02-stage-vm/` 与 `03-stage-rs/` 的任何文档（它们只是 workflow 验证数据）。
- 与启动交接无关的驱动、文件系统、网络、用户态服务器内部实现。
- V3.11 历史轮次的结论、面板、冻结状态。
- `minix3/` 的任何修改；C 源码只读。
- 超出 49 个关联代码文件的相邻模块；若发现相邻问题，按 `adjacent` 记账，不阻塞本篇。

## 生成约束

1. 先讲 C 行为，再讲 Rust 保留、改变或重设计；不得先摆 Rust 抽象再要求读者相信。
2. 事实断言必须有符号锚点或 C 源码锚点；行号只用工具派生形式。
3. 正式正文不写 review 编号、迭代叙事、修复史、临时承诺、占位内容。
4. 术语首次出现处负责定义；同一概念不换词；概念密度按节控制。
5. 分析章与实现章必须有真实代码节选与机制推演；诊断章必须有可复制命令。
6. 读者画像与学习范围见 `rewrite-notes/meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`；
   该表必须由作者确认后才可用于 G18。
7. 本轮以当前文档结构为范围，不重新做五份 rerank 输入的共识裁决；这是对既有文档的
   V3.12 重审，范围以本文档章节 + 关联代码清单为准。若后续需要改变章节范围，按新
   edition 处理，不在轮次中途改。
