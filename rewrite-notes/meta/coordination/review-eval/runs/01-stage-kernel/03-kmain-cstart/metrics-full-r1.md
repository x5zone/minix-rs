# D03 full round R1 覆盖证据

FULL-ROUND R1
DOC 03-kmain-cstart.md

## 覆盖单元

UNIT 1 1. 概述
UNIT 1.0 1.0 本章导读
UNIT 1.1 1.1 02 篇交出的状态：三份答案仍由引导方提供
UNIT 1.2 1.2 三张表与四类入口
UNIT 1.3 1.3 "第一次行使特权"指什么
UNIT 1.4 1.4 本篇范围与可以安全跳过的内容
UNIT 2 2. C 源码分析
UNIT 2.0 2.0 本章导读
UNIT 2.1 2.1 kmain 入口的五步
UNIT 2.2 2.2 cstart 的调用序列与顺序不变量
UNIT 2.3 2.3 prot_init：五类动作与两张表的容量
UNIT 2.4 2.4 tss_init：一条描述符、一个结构体、两条系统调用路径
UNIT 2.5 2.5 prot_load_selectors：让表生效的一串指令
UNIT 2.6 2.6 重建页表：四个调用、一条理由与一条约束、两份 vir2phys
UNIT 2.7 2.7 earm 的 protect.c：另一份独立实现
UNIT 2.8 2.8 本章归纳
UNIT 3 3. Rust 设计决策
UNIT 3.0 3.0 本章导读
UNIT 3.1 3.1 64 位下三张表的去留（→§2.3、§2.4、§2.6）
UNIT 3.2 3.2 一条真实的顺序依赖拆出两个 trait（→§2.4、§2.5）
UNIT 3.3 3.3 表装进 static 再 load：地址稳定成为接口外的契约（→§2.4）
UNIT 3.4 3.4 用户态入口：能力探测换成一条路径 + 内核自己的标签（→§2.4）
UNIT 3.5 3.5 页表重建的淘汰与恒等段的继续服役（→§2.6）
UNIT 3.6 3.6 把 cstart 拆成两个里程碑（→§2.2）
UNIT 3.7 3.7 门在第一次 load 之前就有真入口（→§2.5）
UNIT 4 4. 实现详解
UNIT 4.0 4.0 本章导读
UNIT 4.1 4.1 kmain 阶段 A 的落地（→3.6）
UNIT 4.2 4.2 init_protection 的逐步落地（→3.2、3.3、3.7）
UNIT 4.3 4.3 x86_64 的 GDT 与 TSS 装填（→3.1、3.3）
UNIT 4.4 4.4 IDT 的装填与门覆盖（→3.2、3.7）
UNIT 4.5 4.5 系统调用入口与 per-CPU 数据的落地（→3.4）
UNIT 4.6 4.6 aarch64 与 riscv64 的落地（→3.1、3.3）
UNIT 4.7 4.7 与 Minix3 C 的对照（→§2.1–§2.7）
UNIT 5 5. 测试要点
UNIT 5.0 5.0 本章导读
UNIT 5.1 5.1 宿主：arch crate 的单元测试（→4.3、4.4）
UNIT 5.2 5.2 真机：test-protection 三架构（→4.2、4.6）
UNIT 5.3 5.3 测试缺口
UNIT 5.4 5.4 机制 → 需要证明什么（反转核对表）
UNIT 6 6. 诊断
UNIT 6.0 6.0 本章导读
UNIT 6.1 6.1 症状 → 怀疑对象 → 确认命令
UNIT 6.2 6.2 可复制命令

## 可重放
rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/03-kmain-cstart/panel/round1/fact.md:1
