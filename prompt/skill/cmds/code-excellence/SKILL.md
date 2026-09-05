---
name: "code-excellence"
description: "代码卓越度专项：多层设计审视、对比 Redox/Linux/OS 理论/Rust 最佳实践、多方案优中选优、死代码消除。当用户要求代码卓越/重构设计/消除死代码/对照 Redox 找改进点时调用。"
---

# code-excellence

## 目标
分层审视设计（整体→模块→trait→函数），多方案对比优中选优；死代码消除是显式子目标。

## scope
`file` / `module` / `dir`（按模块组织，逐文件落地）。

## 执行
按 `prompt/review-rules/review-cmds.md` §四：每层问"如果今天重写会怎么设计"；每个改进点 ≥2 方案 + 对照 Redox/Linux/OS 理论 + 理由；架构级改动标 `[ARCH: ...]` 三处一致；死代码每项给"为何死 + 消除影响"，拿不准标 OQ 上交。

## 强制门
translate 防线（模式 16/17/65）+ 架构抽象族检查（模式 79-82）+ fix-guard + 文档-代码同步 + `cargo test -p {crate}`。

## 不做
不改外部可观察行为（Rewrite 边界）；不修正确性 bug（发现 → 记录交 full-review——卓越建立在正确之上）。

## 产物
设计对比表 + 死代码清单（含理由）+ 文档同步记录 + 测试通过。
