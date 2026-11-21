# D02 R5 certify — fact / refute 回读

> 独立冷启动实例只读回读；对象为当前工作树 `02-higher-half-kernel.md`。本文件由主 agent 从回执转录，逐 ID 保留判定与关键证据；未覆盖面见文末。

| ID | 判定 | 位置 | 证据/说明 |
|---|---|---|---|
| FACT-01 | CLOSED | §4.5、§5.4 | 阶段占位与 R02-阶段 已清；句可读、指向可解析 |
| FACT-02 | CLOSED | 全文 | `设计裁决`/`评审发现` 计数为 0；保留的是真实 D2/I 锚点 |
| FACT-03 | CLOSED | §4.3 | x86_64 `kernel_identity_mmio_regions()` 为空表；LAPIC/IO-APIC 归 Step 2c |
| FACT-04 | CLOSED | §4.3 | Step 2 两档 `AlreadyMapped` + `.expect` panic，与实现一致 |
| FACT-05 | CLOSED | §6.1 | `arch_boot entered` 与 `preA rdi` 的 temporary 归属已分开 |
| FACT-06 | CLOSED | §3.6/§3.8 | 重复标题已删，各出现一次 |
| FACT-07 | CLOSED | §5.3 | PC 判定按 x86_64/aarch64 与 riscv64 分开 |
| FACT-08 | CLOSED | §2.2 | `head.o` 行删 `.unpaged_bss` |
| FACT-09 | CLOSED | §4.3 | “窗口建表失败”悬空指针改为“下条 map_huge 两类结果” |
| FACT-10 | CLOSED | §4.5 | 策略归 `arch_boot_impl`，finish 只校验/落地 |
| CLAIM-01 | CLOSED | §1.2、§2.1、§5.2 | J1 收窄为组内一致，L4 只对 Rust 生产镜像 |
| CLAIM-02 | CLOSED | §3.3、§5.3 | 生产公式保留；测试载体 `kern_size` 注明 2 MiB 取整窗口 |
| CLAIM-03 | CLOSED（复读） | §4.3 | 三份 `map_huge` 同尺寸叶已存在均返回 `AlreadyMapped`；Step 1 的 `Ok|AlreadyMapped` 合并臂吸收；`lib.rs` 旧注释以实现为准 |
| CLAIM-04 | CLOSED | §4.3 | 删守卫的后果是建表点 panic，不是静默覆盖 |
| CLAIM-05 | CLOSED | §4.3 | Step 1 窗口含调用者栈窗口（SP 向上、4 MiB 下、2 MiB 对齐） |
| CLAIM-06 | CLOSED | §3.1、§5.2 | x86_64 走 `bootface.rs` 分支只为共享常量，不存在直核面 |
| 附注 1 | CLOSED | §3.6/§3.8 | 标题重复已删 |
| 附注 2 | CLOSED（复读） | §2.4 | `pg_mapkernel` 返回值的下游改为“属 VM 页表初始化，本篇不展开” |
| 附注 3 | CLOSED | §4.5 | 信封 + 六分区 = 源码 seven payloads 的算术已显式 |
| 附注 4 | CLOSED（复读） | §4.1/§6.2 | 补 `.eh_frame_hdr` 为链接器合成节、不受 `/DISCARD/` 输入通配影响的说明 |

## 回执结论

- 编号项 16：15 CLOSED / 1 STILL-BROKEN（CLAIM-03）；修复后 3/3 复读 CLOSED。
- 附注 4：2 CLOSED / 2 STILL-BROKEN（附注 2、4）；修复后 3/3 复读 CLOSED。
- 最终：编号项 + 附注全部 CLOSED。
- 未覆盖：未运行 cargo/QEMU/机械门；未逐条复验全文其它语义断言；FACT-08 未看 `objdump` 产物。


结论: PASS（20/20 CLOSED，0 STILL-BROKEN）。
