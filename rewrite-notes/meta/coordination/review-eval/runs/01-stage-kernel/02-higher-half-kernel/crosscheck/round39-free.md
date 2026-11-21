# D02 跨模型复核（2026-11-22）

- 模型：`opencode/hy4-flash-free`（免费端点，独立于当前 ZCode 的 `deepseek-v4.1-flash`）；`--agent plan` 只读。
- 断言包：`tmp/d02-crosscheck/brief.md`；原始输出 `tmp/d02-crosscheck/out-hy4.log`。
- 范围：5 条断言（C i386 链接脚本、x86_64 生产链接脚本、check-layout L4/L5、J3 证据边界、test-higher-half 判据）。

## 结果

| 断言 | 结论 | 证据 | 说明 |
|---|---|---|---|
| C1 | CONFIRM | `minix3/minix/kernel/arch/i386/kernel.lds:4-6,17-19,25-32` | 三常量与 unpaged/high 段布局一致 |
| C2 | REFUTE（断言措辞） | `os/kernel-image/x86_64.ld:26-27,33,58,66,74` | 实际写 `AT(ADDR(sec) - KERNEL_HIGH_BASE)`；与 `AT(KERNEL_PHYS_BASE + offset)` 数学等价，但断言的字面写法不精确 |
| C3 | CONFIRM | `os/kernel-image/check-layout.sh:224-242` | L4 逐段核对平移量；L5 判 2 MiB 对齐 |
| C4 | CONFIRM | `02-higher-half-kernel.md:1802,1833` | §5 明写 J3 无独立断言、只有代码审查与链尾证据 |
| C5 | REFUTE（断言措辞） | `os/kernel/src/lib.rs:2288-2314,2319-2325` | SP 高半、三载体、TEST_RESULT 判定属实；PC 只在 riscv64 并入 PASS，判据是 `pc>=kern_high` 而非“与 nm kmain 一致”；FP 判据是 `fp==0` 而非“链终止” |

## 判读

- C2/C5 的反驳针对**断言包的概括措辞**，不是 D02 文档：文档的 x86_64 链接脚本引文保留源码写法，§5.3/§5.4 已按架构区分 PC 判据，`test-higher-half` 的 SP/PC/FP 读法以 `kmain_verify` 源码为准。
- 交叉复核没有发现必须改代码或文档的项（模型原话：“必须改代码/文档：无”）；C2/C5 提示后续写断言包时不要把等价写法或分架构判据概括成单一公式。
- 该结果关闭 `D02-RUN-R5-05` 的“同模型面板/认证”方法论缺口；D02 的最终确认仍由冷启动独立审阅者执行。
