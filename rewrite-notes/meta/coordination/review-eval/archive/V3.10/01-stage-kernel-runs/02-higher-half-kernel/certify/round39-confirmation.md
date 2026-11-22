# D02 R39 确认轮记录（2026-11-22）

> **INVALIDATED（2026-11-22）**：本文件中的 R39 是 D01 的轮次，不是本篇轮次；D-114 仅适用于 D01。原“CONVERGED”宣告作废，状态为 CONVERGENCE-CANDIDATE。纠正记录见 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/ROUND-IDENTITY-CORRECTION.md`。

## 范围与抽样

- 冷启动审阅者，不参与修复；抽样 `review-units.py sample <doc> --seed <HEAD>`，9 个冻结单元：
  3.5、4.0、4.5、5.3、6.2、3、5、3.0、5.2。
- 41/41 单元冻结、T3 PASS；未抽样单元由 R5 full survey 与机械门覆盖。

## 第一轮结果

7 条新发现（1 P1 + 5 P2 + 1 P3）；9 个单元中 3.5、4.5、6.2、3、5、5.2 涉及缺陷，4.0/5.3/3.0 PASS。

| ID | 节 | 严重度 | 问题 | 处置 |
|---|---|---|---|---|
| D1 | 3.5 | P2 | 跨篇指针 01 §3.8 不存在；换手在 §3.4 | 已改 §3.4 |
| D2 | 4.5 | P2 | “01 篇 4.7 的两种进入方式”指错节；在 §4.6 | 已改 §4.6 |
| D3 | 6.2 | P1 | `break kmain` 在 mangled 符号上不可解析；第五组没有可复制的 `qemu -s -S` 行 | 改用 `test-higher-half-riscv64` 载体，给出 `-s -S` 与高半断点；生产 ELF 用 `break minix_kernel::kmain` |
| D4 | 5.2 | P2 | 脚本头部只列 L0–L8，文档写 L0–L9 | 已改 L0–L8（L9 在内联实现处） |
| D5 | 5.2 | P2 | L10 称三个收段，只列两个 | 补 `.unpaged_bss` |
| D6 | 3 | P3 | `resume_additive` 不经过 `arch_boot` | 拆开直核 cfg 与 v1 接收泛型体 |
| D7 | 6.2 | P3 | 跨架构宿主应使用 `gdb-multiarch` | 已补说明 |

修复登记：`RD-R39-001..007`；确认轮修复计数 1/2（第一批，`--confirm-fix` 6.2）。

## 第一轮增量复核

D1/D2/D4/D5/D6/D7 `FIXED`；D3 `NOT-FIXED`：新增的 riscv64 直核生产镜像示例不能到达 `kmain`（直核镜像要求 DTB `BootFileTable` 且 12 个模块，裸 QEMU 会停机）。改为 `test-higher-half-riscv64` 载体后，实跑 transcript：

```
Breakpoint 1, 0xffffffc000200144 in ?? ()
$1 = 0xffffffc000200144
$2 = 0xffffffc000400000
串口：### TEST_RESULT: PASS test-higher-half ###
```

确认轮修复计数 2/2（第二批，`--confirm-fix` 6.2）。

## 第二轮增量复核

D3 的残余（预期输出段地址算例与 `$rip/$rsp` 未同步）修复后，冷启动审阅者复核：

- `02-higher-half-kernel.md:2059` 已为 `$sp（x86_64 用 $rsp）`；`:2068` 已为 `$pc（x86_64 用 $rip）`。
- 算例复算：`0x80200144 + (0xffffffc000000000 - 0x80000000) = 0xffffffc000200144`；`$sp=0xffffffc000400000`。
- `rg '\$rip|\$rsp|\$pc|\$sp'` 在第五组只剩有意的 x86 括注。

`DELTA: PASS`，无新矛盾。

## 确认轮出口

- 9/9 抽样单元：4.0、5.3、3.0 PASS；3.5、4.5、6.2、3、5、5.2 的 7 条发现全部修复。
- 确认轮修复 2/2 批次（第一批 5 单元、第二批 6.2；第二批含 D3 的两次同缺陷补修），未开新修改轮。
- 单元状态：41/41 frozen、reopened=0、stale=0、T3 PASS；`audit` 全部单元 open=0。
- 机械门与真机门见 `round39-gates.md`；TODO `--fail-on-open --doc 02-higher-half-kernel` 为 0。
