# 03-stage 01 R1 缺陷台账（V3.12）

- edition: V3.12
- 轮次: R1 full
- 归并：按根因合并四份面板报告（P-FACT / READER-A/B/C）；DESIGN-D2 窄复核未按时落盘，设计项由 P-FACT + 同 agent 对照登记。
- 全部行 `处置类 = ①`。

| 发现 ID | 单元 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|
| R1-01（B1/A-1/C-01/FACT-02） | 1 | ① | Step 1 改为“priv 提交 + 本地属性、但不放行”；删“纯本地/不向内核提交” | `rg -n 'Step 1.*不放行' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-02（B2/C-07/C-23） | 1 | ① | Step 2 限定 SYS_PROC；Step 3 只收 Step 2 未同步收走的 ready；INIT 是 USR_F 不参加握手 | `rg -n 'SYS_PROC|USR_F' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-03（FACT-03/A-5/C-04/C-05） | 1 | ① | RS 走 `do_sef_rs_init()` 本地合成 fresh；其他服务才接收 RS_INIT；`init_type` 改为回调参数 type | `rg -n 'do_sef_rs_init' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-04（E2） | 1 | ① | 明确 `get_image` 由 `BootTables::acquire_from` 直接调、grant 走 `IpcApi::grant_read`；§5.2 补 grant 测试行 | `rg -n 'acquire_from|grant_read' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-05（H1/FACT-01） | 1 | ① | 生产状态统一为 `acquire_from` + `TrapKernelApi`；`UnimplementedKernelApi` 只作测试替身 | `rg -n 'TrapKernelApi|UnimplementedKernelApi' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-06（H3/FACT-04） | 1 | ① | BootInit 字段改 `table: RProcTable`；ServiceSlot 不再称“最小六字段” | `rg -n 'RProcTable|ServiceSlot' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-07（H6/FACT-13） | 1 | ① | 01 范围删除 dispatch 节/示例/承诺，dispatch 归 06 | `rg -n 'dispatch.rs' rewrite-notes/03-stage-rs/01-rs-boot-init.md`（仅模块清单） |
| R1-08（H5/C-06） | 1 | ① | 注明 main 固定 Fresh；grant 类型统一 `Option<u32>`，mock gid=1 | `rg -n '固定 `SefInitType::Fresh`|Option<u32>' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-09（H9） | 1 | ① | 说明 `acquire_from` 把表 `Box::leak` 成 'static 切片，生命周期到进程结束 | `rg -n 'Box::leak' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-10（D1/D3/D4/D5） | 1 | ① | 删除 §1.2 与 §2.0 重复示例；补 §3.0/§4.0 实际答案；章末只写增量 | `rg -n '§1.2|§2.0|本章回顾' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-11（S1） | 1 | ① | 章节连续重编号（2.0–2.6、3.0–3.8、4.0–4.6、5.0–5.5） | `grep -n '^### [0-9]' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-12（S2/S3/E1） | 1 | ① | 删除 §6 半截路线；plan 降为背景；99/§1.3 悬空引用改指真实文档/§3.3 | `bash tools/doc-ref-check.sh rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-13（FACT-05） | 1 | ① | `run()` 返回 `Result<(), Errno>`；main 对 Err panic | `rg -n 'pub fn run' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-14（FACT-07/C-02/A-13） | 1 | ① | 测试基线统一 359/0；三模块 42 项（boot 37/table 3/sef 2）；361 个 #[test] 含 2 个 feature-gated | `rg -n '359|361|42 项' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-15（FACT-08/C-10） | 1 | ① | 修正 step_order 与 gid 测试描述为测试体实际行为 | `rg -n 'test_init_fresh_step_order|gid=1' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-16（FACT-09/C-11/D2-01） | 1 | ① | self_update 补 update 参数；feature 打开时 init_fresh 末尾调用，默认 feature 关闭故默认 Rust 少 C 的自升级尾步 | `rg -n 'self_update' rewrite-notes/03-stage-rs/01-rs-boot-init.md`；`cargo check -p minix-rs --features live-update` |
| R1-17（FACT-10/C-24） | 1 | ① | 注明 C priv 未命中 panic 不可达；计数只查基数，集合对齐在 Step 1 | `rg -n '不可达|计数' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-18（FACT-11） | 1 | ① | step0 描述删 env_parse；rs_verbose 由 main.rs 的 parse_rs_verbose 读取 | `rg -n 'parse_rs_verbose' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-19（FACT-12） | 1 | ① | 修 rs.h:191、id.rs 路径、内核 boot 表端点锚点、Grant 类型 | `rg -n 'rs.h:191|minix-types/src/types/id.rs' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-20（C-03/C-17） | 1 | ① | priv 表序改为“RS 准备/放行遍历序”，不是内核可调度序；12 项含 INIT 用户进程 | `rg -n '遍历顺序|非内核 boot 条目' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-21（C-09/C-20） | 1 | ① | Step 3 写明 C 阻塞等待 vs Rust 占位 ENOSYS 的差异；12 落地恢复阻塞 | `rg -n 'Step 3.*阻塞|占位' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-22（C-12） | 1 | ① | machine 快照改为“Rust 提前到 step0 的时序变化”，不称对齐 C | `rg -n '时序变化' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-23（C-19） | 1 | ① | 写清 C 默认 yes vs Rust feature 默认关的外部行为差异 | `rg -n '默认 yes|默认关' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
| R1-24（C-16/C-18/C-21/C-22） | 1 | ① | 四表 API vs 三表实参、SRV_OR_USR 派生、未知/未实现同线值不同原因、rg 计数命令表述 | `rg -n '四路输出|未知调用号|361' rewrite-notes/03-stage-rs/01-rs-boot-init.md` |
