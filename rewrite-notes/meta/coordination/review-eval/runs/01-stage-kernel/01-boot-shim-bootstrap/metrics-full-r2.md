# D01 full round R2 覆盖证据

FULL-ROUND R2
DOC 01-boot-shim-bootstrap.md

## 覆盖单元

UNIT 1 1. 概述
UNIT 1.0 1.0 本章导读
UNIT 1.1 1.1 内核接手时拿到什么
UNIT 1.2 1.2 四条不变量 I1–I4
UNIT 1.3 1.3 与 Minix3 的对应
UNIT 1.4 1.4 范围、术语与接缝
UNIT 2 2. C 源码分析
UNIT 2.0 2.0 本章导读
UNIT 2.1 2.1 引导器契约与启动信息
UNIT 2.2 2.2 `head.S` 入口：`multiboot_init` 与 `load_stack_start → pre_init`
UNIT 2.3 2.3 `get_parameters`：把别人的数据变成自己的记录
UNIT 2.4 2.4 内存账：`add_memmap`/`cut_memmap` 与条目来源
UNIT 2.5 2.5 `overlaps` 与 `kern_mod`：内核把自己算成第 M+1 个模块
UNIT 2.6 2.6 模块装载的检查与失败模式
UNIT 2.7 2.7 ARM32 对照：自己造一份 multiboot
UNIT 2.8 2.8 本章归纳：C 交付物的形状
UNIT 3 3. Rust 设计决策
UNIT 3.0 3.0 本章导读
UNIT 3.1 3.1 协议换代：固件接口替换 Multiboot
UNIT 3.2 3.2 按值交接：`KernelInfo` wire 与字段台账（I1）
UNIT 3.3 3.3 两张内存表与来源记录（I2）
UNIT 3.4 3.4 bootstrap 区间与分配器换手
UNIT 3.5 3.5 取件抽象与 crate 依赖方向
UNIT 3.6 3.6 五条生产腿与发送侧（接缝定义）
UNIT 3.7 3.7 所有权与校验门链（I3/I4）
UNIT 4 4. 实现详解
UNIT 4.0 4.0 本章导读
UNIT 4.1 4.1 入口与调用链
UNIT 4.2 4.2 UEFI 路径：九个动作与产出归属
UNIT 4.3 4.3 两种文件来源共用的取件函数
UNIT 4.4 4.4 `KernelInfo` 构造的两条产出入径
UNIT 4.5 4.5 `store_kernel_info` 的六个落地区
UNIT 4.6 4.6 arena 与分配器换手的落地
UNIT 4.7 4.7 与 Minix3 C 的函数落点对照
UNIT 4.8 4.8 交接后的 Direct Map（概念入口）
UNIT 5 5. 测试：四条不变量各自的证据
UNIT 5.0 5.0 本章导读
UNIT 5.1 5.1 四条不变量的证据矩阵
UNIT 5.2 5.2 宿主测试的边界
UNIT 5.3 5.3 机制 → 需要证明什么
UNIT 5.4 5.4 真机门链
UNIT 5.5 5.5 未闭环项交接
UNIT 6 6. 诊断
UNIT 6.0 6.0 本章导读
UNIT 6.1 6.1 EBS 分界与裸串口通道内的两种写法
UNIT 6.2 6.2 panic 路径：从静默到可读
UNIT 6.3 6.3 典型失败模式与诊断信号
UNIT 6.4 6.4 可复制命令

## 可重放
rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/panel/round2/fact.md:1
