# D02 full round R2 覆盖证据

FULL-ROUND R2
DOC 02-higher-half-kernel.md

## 覆盖单元

UNIT 1 1. 概述
UNIT 1.0 1.0 本章导读
UNIT 1.1 1.1 镜像的四个时刻
UNIT 1.2 1.2 四条不变量 J1–J4
UNIT 1.3 1.3 范围与接缝
UNIT 2 2. C 源码分析
UNIT 2.0 2.0 本章导读
UNIT 2.1 2.1 kernel.lds：双地址与 `AT()`
UNIT 2.2 2.2 两段式布局：unpaged 段与高半段
UNIT 2.3 2.3 `head.S`：一次跳转里的两种寻址
UNIT 2.4 2.4 建表五步与位表
UNIT 2.5 2.5 `k_initial_stktop` 落在哪一段
UNIT 2.6 2.6 本章归纳：C 的“布局-摆放-映射-进入”四段
UNIT 3 3. Rust 设计决策
UNIT 3.0 3.0 本章导读
UNIT 3.1 3.1 链接脚本按“谁按装载地址摆放”分三种形态
UNIT 3.2 3.2 ELF 解析器自建
UNIT 3.3 3.3 栈顶从链接符号变成派生值
UNIT 3.4 3.4 `HUGE_PAGE_SIZE` 与大页能力进扩展 trait
UNIT 3.5 3.5 镜像跨距收口：布局要满足映射粒度
UNIT 3.6 3.6 自举根与映射建立的设计（J3）
UNIT 3.7 3.7 成对的换栈与跳转收进一个 trait 契约
UNIT 3.8 3.8 接收侧两条路径（J4）
UNIT 4 4. 实现详解
UNIT 4.0 4.0 本章导读
UNIT 4.1 4.1 链接脚本的落地
UNIT 4.2 4.2 ELF 解析与按装载地址摆放
UNIT 4.3 4.3 自举根的建立与开启
UNIT 4.4 4.4 三架构的成对跳转
UNIT 4.5 4.5 从装载到 `kmain` 的完整路径
UNIT 4.6 4.6 与 Minix3 C 的对照
UNIT 5 5. 测试
UNIT 5.0 5.0 本章导读
UNIT 5.1 5.1 宿主：解析器、装载器与建表
UNIT 5.2 5.2 布局契约 L0–L10（唯一权威清单；L10 仅 x86_64/all 输出）
UNIT 5.3 5.3 真机：`test-higher-half` 与入口面
UNIT 5.4 5.4 机制 → 需要证明什么（反转核对表）
UNIT 6 6. 诊断
UNIT 6.0 6.0 本章导读
UNIT 6.1 6.1 症状 → 怀疑对象 → 确认命令
UNIT 6.2 6.2 可复制命令

## 可重放
rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/panel/round2/fact.md:1
