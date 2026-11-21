# 02-design.v4 — 内核镜像：布局、映射与进入高半（2026-11-21 重写轴）

> 状态：DESIGN；轴：02 = 内核镜像；01 = 交接载荷。
> 前序：`02-design.v3.md`（事实来源，非结构契约）。

## Ch1. 核心抽象

镜像有四个时刻，每个时刻有一条可证伪的不变量：

| 时刻 | 对象 | 不变量 | 证据 |
|---|---|---|---|
| 链接 | VMA/LMA、`AT()`、段表 | J1 平移一致 | L4；`readelf -lW` |
| 摆放 | 装载器按 `p_paddr` 写段体 | J1 + J2 跨距 | L3c/L5/L9 |
| 映射 | 自举根/内核高半/设备窗口 | J3 映射先于进入 | Step1–4；`test-higher-half` |
| 进入 | 换栈 + 跳转 | J4 成对且不返回 | `HigherHalf`；SP/PC/FP |

## Ch2. 四条不变量

| ID | 不变量 | 判据 | 失败落点 |
|---|---|---|---|
| J1 | 所有 `PT_LOAD` 的 `vaddr−paddr` 相等 | L4；`kernel.lds`/`*.ld` 的 `AT()` 一致 | 段拷贝到错误物理地址 |
| J2 | 跨距 2 MiB 对齐，栈在末段 | L5/L9；`*_stack_*` 符号 | `NoKernelPageSize`；栈落 `.bss` 中间 |
| J3 | 映射先于进入 | Step1 恒等/Step2 内核/Step2b/2c/Step3 enable | 开分页后取指 fault |
| J4 | 换栈与跳转成对且不返回 | 三份 asm；`-> !`；SP/PC/FP 断言 | 返回地址在新栈；SP 低半 |

## Ch3. 决策清单

| # | 决策 | 理由 | 当前落点 |
|---|---|---|---|
| M-1 | 生产脚本保留 `AT()` | 装载器按 `p_paddr` 摆放 | `os/kernel-image/*.ld` |
| M-2 | 三形态链接脚本 | 谁按装载地址摆放决定形态 | `x86_64.ld`/`aarch64.ld`/`riscv64.ld` |
| M-3 | ELF 解析器自建 | 输入字节无对齐保证；错误变体可测 | `minix-elf/src/lib.rs` |
| M-4 | 栈顶由 `kern_virt_base+kern_size` 派生 | 符号值随形态变；测试载体无符号 | `kernel_info.rs:fn kern_stack_top` |
| M-5 | `HugePages` 扩展 trait | 大页是能力，策略与机制分离 | `paging_ext.rs:trait HugePages` |
| M-6 | 2 MiB 收口 + 链接期 ASSERT | 满足大页阶梯；L5/L9 | 三份 `.ld` |
| M-7 | 自举根 additive 恢复 | 接收侧不重建活动根 | `resume_additive` |
| M-8 | 换栈+跳转 trait | 契约单一落点；`-> !` | `boot/higher_half.rs` |
| M-9 | riscv64 条件 `delta` | direct/resume 两腿 PC 视图不同 | `riscv64/higher_half.rs` |

## Ch4. 接缝声明

- 02 管：链接布局、ELF 摆放、运行期映射、换栈进入。
- 01 管：wire 字段、内存账来源、bootstrap 资源、发送侧。
- `pre_init`：`pg_*` 五步在 02；`get_parameters` 在 01。
- `head.S`：第一跳在 01；第二跳（`k_initial_stktop → kmain`）在 02。
- L0–L9 只在 02 有权威清单；五条生产腿计数只在 01。
