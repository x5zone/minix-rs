# D01 指标

## R17

- 发现总数: 87
- 处置: ① 26 / ② 3 / ③ 10 / ④ 48 / ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 8 / 实施 8 / 登记 4
- 成本: agent 7 / token 未采集 / 耗时 约 2.5 小时（面板 + 修复）

## R18

- 发现总数: 98（文档缺陷 86 + D1 方案否决 12；按角色：FACT 12 主条 + 9 次级；READER-A 20；READER-B 20；READER-C 18；REFUTE 6 主张；DESIGN-D2 13 条）
- 处置: ① 21（FACT 16、REFUTE 4、DESIGN-D2-10 1）/ ② 0 / ③ 21（FACT 5、READER-B-15、READER-C-10、REFUTE 2、DESIGN-D2 12）/ ④ 56（READER-A 20、READER-B 19、READER-C 17）/ ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 1 / 登记 0（`KernelInfo::validate` 增加内核×模块×模块半开区间复核）
- 成本: agent 7（fact / reader A/B/C / refute / design D1 / design D2）/ token 未采集 / 耗时 约 4 小时（面板 + 修复 + 三门）
- 回读: RB17 独立回读判 NOT PASS（P0=0、P1=0），其 P2/P3 补修已并入本轮；RB18 待本轮提交后启动

## R19

- 发现总数: 80（FACT 6；READER-A 20；READER-B 20；READER-C 15；REFUTE 7；DESIGN-D2 12）
- 处置: ① 13 / ② 0 / ③ 14 / ④ 53 / ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 1 / 登记 0（`KernelInfo::validate` 跨度 `checked_add`）
- 成本: agent 7 / token 未采集 / 耗时 约 45 分钟（面板 + 修复 + 三门，不含排队）
- 回读: RB19 判 NOT PASS（FACT-06 行号）；补修后 meta R19 判 IMPROVED / NOT PASS（Trace integrity 与计数差 1），二次补修后四组探针均 FAIL、R18/R19 计数闭合

## R20

- 发现总数: 64（FACT 3；READER-A 20；READER-B 20；READER-C 8；REFUTE 6；DESIGN-D2 7）
- 处置: ① 12 / ② 3 / ③ 1 / ④ 48 / ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 1 / 登记 0（六处跨度加法 checked_add）
- 成本: agent 7 / token 未采集 / 耗时 约 40 分钟（面板 + 修复 + 三门）
- 回读: RB20 判 NOT PASS（LSR 释义/八条未同步等 2 P2 + 5 P3），补修后 minix-boot 189、三门回绿；meta R20 待跑

## R21

- 发现总数: 63（READER-A 20；READER-B 20；READER-C 11；REFUTE 7；DESIGN-D2 5；P-FACT MISSING）
- 处置: ① 14 / ② 3 / ③ 3 / ④ 43 / ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 6 / token 未采集 / 耗时 约 50 分钟
- 回读: RB21 判 NOT PASS（36/42 ④ 未修）；meta R21 判 NOT PASS（严格一致 14/37=37.8%，Precision/Teaching/Trace FAIL），R22 逐 ID 补修

## R22

- 发现总数: 67（FACT-A 3；FACT-B 2；READER-A 20；READER-B 20；READER-C 10；REFUTE 7 条主张；DESIGN-D2 5）
- 处置: ① 9 / ② 3 / ③ 5 / ④ 50 / ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 7 / token 未采集 / 耗时 约 55 分钟
- 回读: RB22 判 NOT PASS（EAB/审计表/生产腿/§7.5 等）；六轮复核后 meta R22 **IMPROVED / PASS**（50/50 ④ 全修、探针全 FAIL、67 分档闭合）；保留 60.9% 历史抽样读数

## R23

- 发现总数: 110（FACT-A 7；FACT-B 17；READER-A 24；READER-B 20；READER-C 20；REFUTE 6 条主张；DESIGN-D2 16）
- 处置: ① 61 / ② 3 / ③ 6 / ④ 40 / ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 5 / 实施 5 / 登记 3（bootface 合并、kern_ident 删除、pt_alloc release 语义转 R24；wire 偏移与 landing helper 本轮已实施）
- 成本: agent 7 / token 未采集 / 耗时 约 4 小时（面板 + 修复 + 三门）
- 回读与回归: 面板 104 条 + 6 主张全部有处置；doc 五门 PASS；宿主 196/135+5/862+8/287；QEMU x86/aarch64/riscv 三门 PASS（日志 `logs/round23-*.log`）。RB23 首轮 NOT PASS（B1 §4.2 引文旧 decode 路径、B2 假归因；Q1–Q4）→ 补修后 PASS；meta R23 首轮 IMPROVED / 运行质量 NOT PASS（严格一致率 73.3%）→ 5 项收口后第二次审计 IMPROVED / 条件 PASS（最终态 90.0%）

## R24

- 发现总数: 118（FACT-A 15；FACT-B 21；READER-A 20；READER-B 20；READER-C 20；REFUTE 6 条主张；DESIGN-D2 16）
- 处置: ① 114 / ② 0 / ③ 4 / ④ 0 / ⑤ 0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 4 / 接受 3 / 实施 3 / 登记 0（`ident_regions` 死接口删除、公共窗口 helper、`PlacementPool`；bootface 整段合并不做）
- 成本: agent 7 / token 未采集 / 耗时 约 2 小时（面板 + 修复 + 宿主/QEMU）
- 回读: 宿主 boot-shim 136+6、kernel 865+8、types 339、boot 196、vm 548、arch 288；QEMU x86/aarch64/riscv U-Boot shim、riscv bootface、riscv kernel-image、aarch64 SMP AP 六门 PASS；doc 五门 PASS；RB24 首轮 71/80=88.8% NOT PASS；B1/B2+N1–N11 共 13 项补修后，二次定向回读 23/23=100% PASS（13 定向 + 10 重放；不是 118 条整体抽样）；提交后 RB-post 随机 29 条 26/29=89.7% NOT PASS → 补跨度测试后二次 13/14=92.9% PASS；修复提交链 `90428654f`→`99dac96b4`→`162a86478`

## R25

- 发现总数: 124（FACT-A 14；FACT-B 17；READER-A 20；READER-B 31；READER-C 20；REFUTE 6 条主张；DESIGN-D2 16）
- 处置: ① 98 / ② 0 / ④ 15 / ⑤ 0 / ⑥ 3 / FP 8
- 未闭环: 0
- certify 新准入: 0
- 新 in-scope 真缺陷: 0
- 轮型: discovery
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 3 / 接受 3 / 实施 3（`PlatformDescSource` 可选长度、`map_kernel` 收 `KernelLayout`、`CheckedWindows`/`PhysRange`/直核 `BootStorage` 三项 DESIGN-02/09/10 已落码）
- 成本: agent 7 / token 未采集 / 耗时 约 2 小时（面板 + P0/P1 修复 + 宿主/QEMU）
- 回读: R25 收口批：7 项读者项按 §4.2.1/§4.2.2/§1.4/§2.4.1.3/§3.7/§3.8/§4.6 逐项修 + 冷读；3 项 DESIGN 以 ⑥ 实施（CheckedWindows、BootStorage 统一、PhysRange）；RB25 46 条 44/46=95.7% PASS；meta25 40/59=67.8% NOT PASS（FLAT / 文档 NOT CONVERGED，收口后以本轮 certify 为准）；宿主 136+6/865+8/339/196/548/288；QEMU 六门 `logs/round25-*.log` + 收口重跑 riscv64 bootface / aarch64 SMP AP PASS；doc 五门 PASS

## R26

- 发现总数: 59
- 唯一根因数: 49
- 本轮新准入: 49
- certify 新准入: 0
- 重复合并: 10
- 自引入: 2
- 自引入明细: DESIGN-02 `CheckedWindows` 落码后文档 5 处旧 API 未回填；D2-06 `store_kernel_info` 返回类型变更后 §3.7 引文块未同步
- 处置: ① 11 / ② 11 / ④ 35 / ⑤ 0 / ⑥ 1 / FP 1
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 12 / 接受 12 / 实施 1 / 登记 11（D2-06 已实施；D2-01..05、D2-07..12 经代理裁决转入 R27 code batch）
- 成本: agent 7 / token 未采集 / 耗时 约 3 小时（面板 + 归因 + 修复 + 验证）

## R28 结构轮

- 发现总数: 37（R27 backlog 的收口映射，见 `panel/round28/closure-manifest.md`；不是 R28 新面板）
- 唯一根因数: 32
- 本轮新准入: 0（全部 `origin: R27`）
- certify 新准入: 0
- 重复合并: 5（A-07/B-01、A-12/B-08/C-07、B-09/C-05、B-12/B-14）
- 自引入: 2
- 自引入明细: R28 编辑中一处 `kinfo` 拼写为 `k_inf`（当场修）；一处“当前版本/早期版本”修复史叙事触发 SL-5（当场改为必要条件表述）
- 处置: ① 0 / ② 0 / ④ 31 / ⑤ 0 / ⑥ 4 / FP 2
- 未闭环: 0
- 轮型: full
- edition: V3.1
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 4 / 接受 4 / 实施 4（D2-16 返回值统一、D2-17 窗口边界负控、D2-18 双所有者声明、D2-19 `.data` 口径）
- 成本: agent 1（结构轮不重开全量面板）/ token 未采集 / 耗时 约 2 小时（结构重写 + 机械门 + 宿主/QEMU 复核）
- 回读与回归: 文档门 citation 69 块/29 检查/0 失败、anchor 106/106、ref/structure/style/density 全绿；宿主 `cargo test -p minix-kernel --features mock --lib boot_windows` 3 passed；三个 kernel-image 目标 `cargo check` 通过；QEMU 沿用 R27 的 riscv64 bootface 与 aarch64 SMP AP 日志（本轮只改注释与文档，未改启动行为）

## R29 完整 survey

- 发现总数: 55（FACT-A 3；FACT-B 8；READER-A 10；READER-B 15；READER-C 10；DESIGN-D2 9）
- 唯一根因数: 55
- 本轮新准入: 55（全部 origin R29）
- certify 新准入: 0
- 重复合并: 0（本轮按发现逐条立根因，未合并）
- 自引入: 2
- 自引入明细: R28 把 §4.6 x86 的 `kernel_identity_mmio_regions()` 改成“返回空表”（R29 FACT-B-02 抓到，本轮修）；R28 把 §6.1 `step1+2` 的读法写成“跳转后接收端没出声”（R29 READER-C-03 抓到，本轮修）
- 处置: ① 10 / ② 0 / ④ 35 / ⑤ 0 / ⑥ 9 / FP 1
- 未闭环: 0
- 轮型: full
- edition: V3.1
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 9 / 接受 9 / 实施 9（D2-20..28 全部落码）
- 成本: agent 6（FACT-A/B、READER A/B/C、DESIGN-D2）/ token 未采集 / 耗时 约 3 小时（面板 + P1/P2 修复 + 复核）
- 回读与回归: 文档门 citation 69 块/29 检查/0 失败、anchor 106/106、ref/structure/style/density 全绿；`cargo test -p minix-kernel --features mock` 866 passed / 8 ignored；`cargo check -p kernel-image` 三目标通过

## R30 完整 survey

- 发现总数: 57（FACT-A 4；FACT-B 6；READER-A 12；READER-B 15；READER-C 12；DESIGN-D2 8）
- 唯一根因数: 57
- 本轮新准入: 57（全部 origin R30）
- certify 新准入: 0
- 重复合并: 0（逐发现立根因）
- 自引入: 3
- 自引入明细: R29 改 `map_huge` 为 `match` 后 §4.6 代码引文未同步（R30 FACT-B-01）；R29 改 startup 归属后 §3.4.5 与 §4.2 仍互相矛盾（R30 READER-C-02）；R29 改 §6.1 里程碑表时漏了 U-Boot shim 的 `param_buf` 腿（R30 FACT-B-04）
- 处置: ① 10 / ② 0 / ④ 39 / ⑤ 0 / ⑥ 8 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.1
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 8 / 实施 8（D2-29..36；含 entry_point 交叉校验、BootStorage push 提交断言、依赖图修正）
- 成本: agent 6 / token 未采集 / 耗时 约 3 小时（面板 + 修复 + 复核）
- 回读与回归: 文档六门全绿；kernel 866 passed / 8 ignored；boot-shim 136 passed；minix-boot 196 passed；QEMU 5 PASS + 1 环境 SKIP（U-Boot 直载链宿主装载故障）

## R32 完整 survey

- 发现总数: 19（FACT-A 1；FACT-B 1；READER-A 9；READER-C 1；DESIGN-D2 7）
- 面板建议（未入台账）: 24（READER-B 全部为建议级）
- 唯一根因数: 19
- 本轮新准入: 19（全部 origin R32）
- certify 新准入: 0
- 重复合并: 0（逐发现立根因）
- 自引入: 0
- 处置: ① 3 / ② 0 / ④ 2 / ⑤ 0 / ⑥ 0 / FP 1 / SUGG 13
- 未闭环: 0
- 轮结果: ROUND-CLOSED
- 代裁决: 0
- ARCH 提案: 0
- 用户否决: 0
- 轮型: full
- edition: V3.1
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 7 / 接受 7 / 实施 0（D2-01..07 记为文档改进方向，不阻塞本轮）
- 成本: agent 6 / token 未采集 / 耗时 约 2.5 小时
- 回读与回归: 文档六门全绿；kernel 866 passed / 8 ignored；boot-shim 136 passed；minix-boot 196 passed

## R33 完整 survey

- 发现总数: 46（FACT-A 3；FACT-B 0；READER-A 7；READER-B 16；READER-C 12；DESIGN-D2 8）
- 唯一根因数: 46
- 本轮新准入: 46（全部 origin R33）
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 3 / ② 0 / ④ 5 / ⑤ 0 / ⑥ 2 / FP 0 / SUGG 36
- 未闭环: 0
- 轮结果: ROUND-CLOSED
- 代裁决: 0
- ARCH 提案: 0
- 用户否决: 0
- 轮型: full
- edition: V3.1
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 8 / 实施 3（D2-01 栈窗口修复、D2-02 DTB 排序、D2-03 死标志清理）
- 成本: agent 6 / token 未采集 / 耗时 约 2 小时
- 回读与回归: 文档六门全绿；kernel 866 / boot-shim 136 / minix-boot 196 passed；QEMU 5 PASS + 1 SKIP

## R34 确认轮

- 发现总数: 28（survey 26：FACT-A 6、FACT-B 3、DESIGN-D2 11、READER-A 1、READER-B 3、READER-C 2；certify #1 新准入 2：FACT 1 P0、READER 1 P3）
- 唯一根因数: 28
- 本轮新准入: 28（全部 origin R34）
- certify 新准入: 0（certify #1 的 2 条已在本轮修完；certify #2 重读为 0）
- 重复合并: 0
- 自引入: 1（RD34-CERT-FACT-01 由本轮 §3.4.2 修复句引入，origin R34；已修）
- 处置: ① 10 / ② 4 / ④ 6 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 8
- 未闭环: 0
- 轮结果: CONVERGED
- 代裁决: 1（D-049 批准 R34-design-batch 与 F-R34-01 延期）
- ARCH 提案: 0
- 用户否决: 0
- 轮型: confirmation
- edition: V3.3
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 4 / 接受 4 / 实施 0 / 登记 4
- 成本: agent 8 / token 未采集 / 耗时 约 4 小时
- 回读与回归: 文档六门全绿；certify #1 新准入 2（已修）、certify #2 新准入 0；`cargo test -p minix-kernel --features mock` 866 passed / 0 failed / 8 ignored；QEMU 本轮未跑（无 arch/boot 行为改动）

## R31 完整 survey

- 发现总数: 53（FACT-A 8、FACT-B 3、DESIGN-D2 9、READER-A 12、READER-B 16、READER-C 5）
- 唯一根因数: 24（RD-R31-01..24）
- 本轮新准入: 24
- certify 新准入: 0
- 重复合并: 29
- 自引入: 0
- 处置: ① 17 / ② 0 / ④ 10 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 26
- 未闭环: 0
- 轮型: full
- 轮结果: ROUND-CLOSED
- 代裁决: 0
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.3
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1（独立冷读）/ token 未采集 / 耗时 约 3 小时
- 回读与回归: 文档六门绿（anchor 105/105、citation failed=0、structure/ref/style/density 通过）；独立冷读 `certify/round31-backfill-coldread.md` 判 CLOSED 33 / STILL-BROKEN 20；STILL-BROKEN 的 20 条中 12 条本轮修文档、8 条按读者/设计 P3 门槛登记 SUGG；`cargo test -p minix-kernel --features mock` 863 passed / 3 failed 是 D2-39 收紧尝试的实测反证，代码已回退

## R35 确认轮

- 发现总数: 78（FINDING 35 + SUGGESTION 37 + CLAIM 6，与 `panel/round35/INDEX.md` 同源）
- 唯一根因数: 78（RD-R35-01..78，逐发现立根因）
- 本轮新准入: 76
- certify 新准入: 0
- 重复合并: 0（逐发现立根因；跨轮同根关系由面板原文与 OPEN-DEBT 指针表达）
- 自引入: 2（§4.1 的「四条架构」由 R31 补清点批引入、R35 的 READER-A-02 与 FACT-B-S01 两处同源抓到；两行 origin 记 `R31；last_seen: R35`，故本轮新准入 = 78 − 2 = 76）
- 处置: ① 17 / ② 13 / ④ 8 / ⑤ 0 / ⑥ 0 / FP 5 / SUGG 35
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 1（D-060：批准 6 条设计 P2 与 1 条 ARCH 提案延期到 `R35-design-batch`）
- ARCH 提案: 1（ARCH-PROPOSAL-01：aarch64 `arena_pa == 0` 形态判别改链接期分叉，待用户裁决）
- 用户否决: 0
- edition: V3.4
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 13 / 接受 13 / 实施 0 / 登记 13
- 成本: agent 8（六角色 + 两次 certify）/ token 未采集 / 耗时 约 5 小时
- 回读与回归: 文档六门绿（anchor 108/108、citation `checked=29 failed=0`、structure/ref/style/density 通过、doc-code-map 54/54）；独立回执 `certify/round35-fact.md` 21/21 CLOSED、`certify/round35-reader.md` 12/12 CLOSED；`cargo test -p minix-kernel --features mock` 的 D2-39 收紧尝试已回退，本轮无代码行为改动，QEMU 未跑

## R36 确认轮

- 发现总数: 93（ID 承载：FINDING 62 + 编号 SUGGESTION 25 + CLAIM 6；另有 8 条 fact 角色未编号建议，单列不参与分档）
- 唯一根因数: 93（RD-R36-001..093；逐 ID 立根因，R35 同形；跨角色同根关系由 action/evidence 的组文本表达）
- 本轮新准入: 93
- certify 新准入: 0（round36-fact 与 round36-reader 两份独立回执全 CLOSED；无新增修改点）
- 重复合并: 0（逐 ID 立根因；8 条未编号 fact 建议另计）
- 自引入: 0（certify 回执未发现本轮修复引入的新根因）
- 处置: ① 44 / ② 0 / ④ 16 / ⑤ 0 / ⑥ 0 / FP 3 / SUGG 30
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 1（D-104 修正：D2-05 判 re-litigation，采 D-064 路线 B，撤回延期）
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.4
- re-litigation: 1（DESIGN-D2-05 命中 D-064）
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 0 / 登记 1
- 减法: 提案 8 / 采纳 4（D2 SUBTRACTION-TABLE）
- 成本: agent 13（7 面板 + 6 次 certify 回执）/ token 未采集 / 耗时 约 7 小时
- 回读与回归: 文档六门绿（anchor 110/110、citation checked=29 failed=0、structure/ref/style error=0/density、doc-code-map 54/54）；`--stale-lines a0261fdd1 --strict` PASS；minix-boot 185/0、boot-shim 146/0、minix-elf 27/0、kernel mock 866/0/8；test-sysboot PASS、test-riscv64-uboot-shim 6/6 PASS；aarch64 rt-birth 门在同一处 user-chain Data Abort（stash 对照，已知 A2 遗留，非本轮因果）

## R37 重写基线（V3.4；full）

- 发现总数: 46（FINDING 40 + CLAIM 6）
- 唯一根因数: 46（RD-R37-001..046；逐 ID 立根因）
- 本轮新准入: 46
- certify 新准入: 0（四份独立 certify 回执全 CLOSED；见 loop §5）
- 重复合并: 0（逐 ID 立根因；跨角色同根关系在处置文本中说明）
- 自引入: 0（certify 未发现本轮修复引入的新根因）
- 处置: ① 40 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 3 / SUGG 3
- 未闭环: 0
- 轮型: full
- 轮结果: ROUND-CLOSED
- 代裁决: 0
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 减法: 提案 0 / 采纳 0
- 成本: agent 7（5 面板 + 2 certify）/ token 未采集 / 耗时 约 6 小时
- 回读与回归: 文档七门绿（citation blocks=48 checked=21 failed=0；anchor 89/89；structure/ref/style error=0/density 通过；doc-code-map 48/48）；publication 无新增；todo-ledger `--doc 01-boot-shim-bootstrap` OK；tutorial-rubric 6/6 章缺项与 v3 基线同型（存量）；code-publication 126 处存量过程注释（`os/` 冻结，不改）；QEMU 未重跑（文档-only）
