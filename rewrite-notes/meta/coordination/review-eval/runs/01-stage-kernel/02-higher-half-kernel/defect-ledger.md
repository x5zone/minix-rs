# defect-ledger

| 根因 ID | 发现 ID | 单元 | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|---|

## R1 full（V3.11，2026-11-22）

| 根因 ID | 发现 ID | 单元 | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|---|
| RD-D02-R1-001 | FACT-F1 | 3.2 | R1 | P-FACT | P2 | 文档/事实 | ① | `InvalidSegment` 返回点改为“不止三个”，补 segment_spans/loader 边界 | loader.rs:198,271,284,414-427 | rg `不止三个` 命中 |
| RD-D02-R1-002 | RA-C1 | 3.2 | R1 | READER-A | P2 | 文档/范围 | ① | “内核侧一行解析代码也没有”限定为“内核镜像装载解析不在内核镜像里” | §3.2 同段/§4.2 | rg `内核镜像的装载解析不在内核镜像里` 命中 |
| RD-D02-R1-003 | RA-C2 | 4.3 | R1 | READER-A | P1 | 文档/条件 | ① | Step 2 守卫指路句改为 `kern_virt == kern_phys` | higher_half.rs 守卫 | rg `为什么 kern_virt == kern_phys 时不能做 Step 2` 命中 |
| RD-D02-R1-004 | RA-F1、RB-F3 | 5.2 | R1 | READER-A/B | P1 | 文档/断言清单 | ① | §5.2 补 L3a/L3b/L3c、L6a/L6b 子断言 | §3.1/§4.1/§6.1 引用 | rg `L3 的子断言` 命中 |
| RD-D02-R1-005 | RA-F2 | 1.1 | R1 | READER-A | P1 | 文档/主体 | ① | 四时刻表“映射”主体改为直核/ v1 分列 | §3.8 建根者 | rg `直核路径是内核镜像；v1 路径是 boot-shim` 命中 |
| RD-D02-R1-006 | RA-F3 | 6.1 | R1 | READER-A | P2 | 文档/引用 | ① | §6.1 三时刻引用 §2.1/§4.1 改为 §2.4/§4.3 | §4.3 运行期推演 | rg `§2.4、§2.3、§4.3 与 §4.4` 命中 |
| RD-D02-R1-007 | RB-F1 | 3.1 | R1 | READER-B | P2 | 文档/分类 | ① | 第三种形态示例改为第二种形态；表格拆默认布局行 | §3.1 三形态定义 | rg `第二种形态的实例` 命中 |
| RD-D02-R1-008 | RB-F2 | 5.4 | R1 | READER-B | P2 | 文档/证据范围 | ① | “无未闭合缺口”改为“有证据或明确边界” | M3/M7/M9 部分覆盖 | rg `已登记边界，不是未登记缺口` 命中 |
| RD-D02-R1-009 | RC-F1 | 2.5 | R1 | READER-C | P1 | 文档/因果前提 | ① | 假设栈落 .unpaged_bss 的回收链限定到假设与时间窗 | bootstrap_start/len 源码 | rg `在这个假设下` 命中 |
| RD-D02-R1-010 | RC-F2 | 6.1 | R1 | READER-C | P1 | 文档/症状 | ① | “栈未切换”与“栈顶算错”拆成两行，区分静默损坏/低地址页错误 | §2.3 对照 | rg `初期无异常、随后数据静默损坏` 命中 |
| RD-D02-R1-011 | RC-F3 | 4.0 | R1 | READER-C | P1 | 文档/建根主语 | ① | §4.0 小结改为“建根者建立，接收侧只追加” | §3.8 禁止重建 | rg `自举根由建根者建立` 命中 |
| RD-D02-R1-012 | D2-F1/F2/F3 | 3.4、3.5、3.7 | R1 | P-DESIGN-D2 | P2 | 设计取舍 | FP | 不实施 D1 的根 typestate/EnterTarget/1GiB 删除，记录为设计候选；D2 自述 PARTIAL 不改外部行为 | panel/round1/design-d2.md VERDICT；os/kernel/src/lib.rs:299 现行为 | D2 自述外部行为等价，不改代码 |

## R2 full（V3.11，2026-11-22）

| 根因 ID | 发现 ID | 单元 | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|---|
| RD-D02-R2-001 | FACT-F1/F2、REFUTE-CLAIM-03 | 5.2 | R2 | P-FACT/P-REFUTE | P1 | 文档/事实 | ① | L3a 改为 PT_LOAD 段数；L6b 只比 _start=入口；补 L6c 落首段 | check-layout.sh:212,219,221,248-258 | rg `L3a =`/`L6c =` 命中 |
| RD-D02-R2-002 | RB-F1、RC-F2 | 5.4 | R2 | READER-B/C | P2 | 文档/证据边界 | ① | 六个部分覆盖 M1/M3/M4/M7/M9/M11 全部列入边界句 | §5.4 表 | rg `M1/M3/M4/M7/M9/M11` 命中 |
| RD-D02-R2-003 | RC-F1 | 2.5 | R2 | READER-C | P2 | 文档/后果 | ① | 恒等映射被移除→页错误；物理页复用→静默损坏，分开写 | §2.5 假设推演 | rg `第一次访问是页错误` 命中 |
| RD-D02-R2-004 | RA-F1 | 6.0 | R2 | READER-A | P2 | 文档/工件名 | ① | `kernel-image` 改为 `kernel` | Cargo.toml [[bin]] name=kernel | rg `release/kernel` 命中 |
| RD-D02-R2-005 | RB-F3 | 4.3 | R2 | READER-B | P2 | 文档/主语 | ① | 代码块前加“直核面 build_bootstrap_root_and_enable；v1 不走” | §3.8 对照 | rg `直核面的` 命中 |
| RD-D02-R2-006 | D2-R2-01 | 3.4 | R2 | P-DESIGN-D2 | P1 | 文档/验证范围 | ① | “map_huge 只执行已验证写入”限定内核镜像；Step1 硬编码 2MiB 单独说明 | boot_policy.rs:117-128；lib.rs Step1 | rg `Step 1 的恒等窗口是另一条路` 命中 |
| RD-D02-R2-007 | D2-R2-02 | 3.7 | R2 | P-DESIGN-D2 | P1 | 文档/前置条件 | ① | kinfo 前置条件改为“当前活动视图可读”；直核传物理视图 | higher_half.rs Safety 注释三份 | rg `活动视图里可读` 命中 |
| RD-D02-R2-008 | D2-R2-03/D2-R2-05 | 3.7 | R2 | P-DESIGN-D2 | P2 | 文档/断言权威 | ① | §3.7 增加栈顶对齐行；上游/本地断言职责分开 | x86_64/aarch64/riscv64 higher_half.rs:34-43 | rg `栈顶 16 字节对齐` 行命中 |
| RD-D02-R2-009 | D2-R2-04 | 3.4 | R2 | P-DESIGN-D2 | P2 | 文档/契约注释 | ① | HUGE_PAGE_SIZE 注明两个消费者 | paging_ext.rs:79-80；boot_policy.rs | rg `实际消费者有两处` 命中 |
