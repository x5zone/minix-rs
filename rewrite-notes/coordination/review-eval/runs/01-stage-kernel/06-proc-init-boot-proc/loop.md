# D06 循环轨迹 —— 06-proc-init-boot-proc

## R1 完整修改轮（V3.4 四态模型；D-076 授权）

### R1 §0 基线

- 轮型: full
- edition: V3.4
- schema: V3.4 from R1
- 基线提交：D05 收尾提交后的工作树（面板启动时点）
- 文档：`rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md`
- 文档基线 sha256：`a4d87613a7b8e2e1f0debf26e07e75784ac62feaaf612d9159a9d9e66dc820bf`
- 行数：2976（修复后约 2979）
- 设计快照：06 组齐备
- 关联代码：`doc-code-map --check` 23/23
- 语料条目：D06（`CORPUS.md` 第二节）
- 范围声明：进程表与 boot 进程初始化 + 23 个关联文件 + 本轮修复触碰的文件；不在范围 = 相邻模块、结构结构性重构
- 重开依据：D-076

### R1 §1 面板

- 六角色全量、独立子 agent；读者 A/B/C 独立（文档 2976 行 ≥ 500）：
  - P-FACT：18 条 FINDING + 2 SUGGESTION（fact.md）——P1 1（default=["mock"] 构建形态）、P2 10（引文保真三处内部编号、排除集容量过时、C 坐标错位等）
  - P-DESIGN-D2：14 条 FINDING + 2 ARCH-PROPOSAL（design-d2.md）——P1 4（行为级）
  - P-READER-A：6 + 15；P-READER-B：1 + 14；P-READER-C：10 + 8
  - P-REFUTE：6 条 CLAIM（阻断 1 / 非阻断 1 / 反证失败 4）——阻断条与 FACT-015 同根
- **VARIANCE（派发）**：并发 3+3 两波；fact 一次派发即成。
- 制品入库：`panel/round1/` + `INDEX.md`。
- certify 回执：`certify/round1-certify.md`（首轮 8 CLOSED / 4 需整改；按证据改正 4 处后复核）。

### R1 §2 发现

- 94 条机器可计：FINDING 49（fact 18、design-d2 14、reader-a 6、reader-b 1、reader-c 10）+ SUGGESTION 39 + CLAIM 6。
- 严重度：P0 = 0；P1 = 5（fact 1 + design 4）；P2 = 30 余；P3 = 40 余。
- 面板要点：fact 的引文保真发现集中在「仓库内部编号（D-58/NK4-A/SD-17）被当作源码内容按原文引用」一族（三处同根）+ 一条 P1 构建形态错误；设计侧抓到 4 条行为级 P1（能力 flag 集偏离 C、内核 task 优先级偏离 C、访问器别名、文档与拆分 API 矛盾）；refute 独立复核出与 FACT-015 同根的阻断条。

### R1 §3 归因

- 41 个根因（RD-R1-01..41）。
- 冻结决策检查：无命中；4 条行为级设计 P1 按 D-043/D-049 先例记 ② 延期（D-076）；ARCH 提案 RD06-A1/A2 待裁；READER-C-01（rts_set 语义）经源码核验（`proc.rs` 的 `set` 是 `fetch_or`）判 FP；CLAIM 11..15 反证失败记 FP、CLAIM-16 与 FACT-015 同根修后记 ①。
- 范围：全部 in-scope。

### R1 §4 修复

- 文档侧（12 处修改点）：§5.1 构建形态句（P1）、三处内部编号引文清洗（D-58/NK4-A/SD-17）、§3.8 排除集容量改 `VM_BOOT_EXCLUSION_CAP`、§1.3「两个字段→三个」、§2.1 补 40 计数配置口径、§2.1.2 p_stale_tlb 三处写点、§2.1.6 MF 位 boot 期口径、§2.9 名字登记行、§6.2 命令 3 口径注释、§2.6 表格 4 列修复（进面板前已修）、§4.2 引文注记。
- 代码侧（2 处小修，非 ⑥——是 lib.rs 的 C 坐标注释勘误）：main.c:226→253、267-270→264-267。
- 文档六门：anchor 118/118、citation `checked=34 failed=0`、structure/ref/style(error=0)/density 通过、doc-code-map 23/23。
- 宿主：`cargo check -p minix-kernel --features mock` 绿。

### R1 §5 复验

- 独立回执 `certify/round1-certify.md`：首轮 12 条判 8 CLOSED / 4 需整改（default 句未应用、SD-17 标注缺失、NK4 块注未改、L134 未动）；四处按证据改正后复核（复核以回执为准）。
- `certify 新准入 = 0`（闭集 = 12 个修改点全部覆盖）。
- 代码行为：lib.rs 只改注释常量；`cargo check` 绿；QEMU 未跑（无行为改动）。

### R1 §6 指标

- 与 `metrics.md` 的 `## R1 完整修改轮` 块一致：发现总数 94、唯一根因数 41、本轮新准入 41、certify 新准入 0、处置 ① 12 / ② 4 / ④ 13 / FP 4 / SUGG 65、未闭环 0。
- 轮型 `full`；轮结果 `ROUND-CLOSED` → `CONVERGENCE-CANDIDATE`。

### R1 §7 状态

- `ROUND-CLOSED`。收敛声明权在确认轮（本篇按用户限定「只做一轮完整 review」，确认轮未排期）。
- 未闭环 0（OPEN-DEBT 的 `R1-design-batch` 有 D-076 判决行与日期，按 §4.0f 出口 2 销账）。

### R1 §8 给 meta-review 的不确定点

1. 4 条行为级设计 P1 延期到 design batch——若 meta 认为其中 CapabilityTemplate flag 集（VM_F|SRV_F vs VM_F）是正确性缺陷而非行为取舍，本轮应改 BLOCKED 而非 ROUND-CLOSED。
2. READER-C-01（rts_set 覆盖语义）判 FP 的依据是 `proc.rs` 的 `fetch_or`——面板四个角色都没读源码，只有主 agent 事后核验；如果该核验有误，§3.3/§3.9 的文档陈述与代码一致性的结论都要重审。
3. 文档 2976 行只修了 12 处修改点 + 4 条延期——读者/设计的 P3 长尾（65 条 SUGG）是六篇里最大的一批，density 下降是否符合预期需 meta 判断。

### L6

- 同 agent 收口（另有独立 certify 回执，见 §5）。流程质量结论：`IMPROVED`（从无 run 记录到完整修改轮收口）。
- BLIND-SPOT-DELTA：新增候选「行为级设计项在文档评审轮的延期判决模板」（D-076 形态）。

### L6.5

- 三问：① 不变量是「修改轮的 P0/P1/P2 事实与同步项全部本轮修、行为级设计项合法延期」；② 前提是 D-076 授权 + 文档六门绿；③ 证据是 certify 回执与 OPEN-DEBT 判决行。
- 资源重读：查 `fresh-eyes-review`；无新检测模式，并入既有候选。
