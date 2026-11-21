# P-SUBTRACT · round38

只读；输入仅 `design-d1.md`、`design-d2.md`、`related-code.txt` 所列实现、目标文档第 3/4 章、`review-code-checklist.md` §3.5/§2.5、`review-code-excellence.md` §5.1.5。未读历史评审、作者解释、其他面板输出。

## SUBTRACTION-TABLE

| 提案 | 减法替代 | 减法拦不住的理由 | 保障强度 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| D2 ARCH：删 `pt_alloc` 六静态，`&mut EarlyMemory` 穿进 `Paging` | 每槽一个 `AtomicUsize` 哨兵（0=未注册，CAS 单写），6 静态降 2（DESIGN-09）；`register`/`assert!` 原样保留 | 无。被防故障（双注册、未注册取页）由 `assert!`（两种构建档都生效）与 CAS 拦住；boot 单线程、VM 独立地址空间，不是并发风险；改 `Paging`/`HugePages` 宽 trait 与三架构实现（D2 自估 242 处）是加法不是减法 | 等价（断言在 release 也生效；`pt_alloc.rs:107-108`） | 0 | 否 |
| DESIGN-07a 只删 crate 私有 `MemRange`（`vm_handoff.rs:309`），换 `PhysRange` | 一个文件、零 wire：`FreeRangeSet` 内部改用 `PhysRange::new(..).expect(..)` | 无（`MemRange` 本就是私有重复） | 强 | 0 | 是 |
| DESIGN-07b `PhysRange` 全局统一，删 `MemoryRegion` | 不做：保留 `MemoryRegion` 为 wire/报告类型，只做 07a | 统一不是纯减法：`MemoryRegion{base,len:usize}` 能表达 `end=2^64`（`SourceRange.end_exclusive` 用 u128 正是为此），`PhysRange{start,end:PhysBytes}` 不能；统一会把 wire 域合法端点变成不可表示态。改动面约 30 文件，超 §3.5 代价门槛 | 中（局部 07a 强） | 0（但把一个 wire 概念换成内部概念） | 否（交裁决） |
| DESIGN-08 `SourceRecord` 72 B 收窄到 4 字段，wire v11 | 不做：72 B 已由 21312 B 容量断言与 3584 B 余量兜住；字段无读者不构成运行期缺陷 | 无运行期缺陷，纯概念/容量债。收窄后 `range` 仍占 24 B（u64+u128），296×32+7872≈17344 B，arena 只从 8 页降到 5 页；要到 4 页须把 `end` 降成 u64，重新丢掉 2^64 端点。收益不足单独触发版本+收发两侧+容量表全改 | 不做=强；收窄=中（v11 双端精确比对） | 0 | 否（交裁决） |
| DESIGN-01 分类器只落 storage，wire 由 report 一次编码 | 删 `reset_map_partitions`（`handoff_v1_write.rs:382`）、`Classification` 双轨返回与双写断言；分类器只写 caller 数组，`report` 一次 `push_memmap/push_reserved` | 无。双写同源同一步，今天不发散，但无交叉检查，任一容量/过滤变化静默分叉；删一份写者即删分叉面。注：D2 称 `fill_map_tables` 无 once 守卫与源码不符——`boot_storage.rs:145` 有 `assert!(!self.map_committed)` | 强（单写者；wire 与 storage 同源由构造保证） | 0（少一个入参） | 是（动 UEFI 主链，交确认） |
| DESIGN-03 v1 接收与直核共用单一 `publish_landed` | 把 `store_kernel_info` 尾段（六处 `from_raw_parts` + `KERNEL_INFO` 写入，`lib.rs:1689-1694`）抽成私有 fn；v1 解码后调它，直核经 `store_kernel_info` 调同一段，删 v1 手写约 30 行（`lib.rs:700-741`） | 重复可删，但两份实现有行为差：直核名字截断 15、v1 截 16；直核非法 UTF-8 回退最长前缀、v1 `expect` panic。减法消不掉“选哪个语义” | 中（统一后两腿一致；一次性发布守卫仍在） | 0（私有 fn） | 是（15/16 与 UTF-8 策略交裁决） |
| DESIGN-02 合并 `prepare`/`jump`，删 `PreparedJump` | 纯删：`enter_kernel_image_v1` 内联两段，两函数降私有 | 无。`PreparedJump` 是 §3.5 D-073 类型化正例，但合并后 `VirBytes`/`PhysBytes` 不再同处一个签名，风险面随值对象一起消失——减法严格优于类型 | 强 | -1（删一个类型） | 是 |
| DESIGN-04 删 `resv_claims` 五个 `BootCell` | `capture` 返回栈上 `ClaimSet`（约 1.5 KB），`check(&claims, descs)` 收值；重试循环同函数，栈值天然存活 | 无（隐式全局顺序与 `test_serial` 锁需求一并消失） | 强（栈深 1.5 KB 需确认 shim 栈） | -1（删静态组，不加类型） | 是 |
| DESIGN-05 删三字段 `BootScalars` 与两个构造器 | 签名改收三个值（`VirBytes`/`PhysBytes`/`u64` 互异，不可能交换） | 无。§3.5 已把它的收益定为“收敛抄写面”P2；v10 删第四参后收益进一步缩小 | 强 | -1 | 是 |
| DESIGN-06 `requires_startup_kernel_record` 改名，删 `validate_at` | 纯改名 + 删一层转发 | 无 | 强 | 0 | 是 |
| DESIGN-09 `pt_alloc` 六静态降二 | 见 ARCH 行：每槽一个 `AtomicUsize` 哨兵 | 无（“注册中”第三态不存在） | 强 | 0 | 是 |
| DESIGN-10 内核入口收 `&[PhysRange]` | 不做代码改动，只改文档措辞 | 无：`shim_windows` 处在 boot-shim（固件侧）→ 内核边界上，正是 §4.4“裸地址对只允许出现在固件/分配器边界”的实例，不是自我豁免 | 强 | 0 | 否（文档一句） |
| DESIGN-11 两处锚点校正 | 纯文档改名，不新增内容 | 无 | 强 | 0 | 是 |
| D1 `BootFacts<'a>` | 删 `BootReport`，`BootStorage` 自身作借用视图（3 个标量字段 + 现有切片），`&BootStorage` 传给内核 | 视图与可填存储合一后，内核拿到的是可 `fill_*` 的类型（只读使用）；是公开 API 改名 | 中（少一个名字，代价是 API） | -1 | 是（交确认） |
| D1 `EarlyMemory` | 不做：现仓 boot 只 bump 发页（`boot_alloc.rs`），账只在 `vm_handoff::classify` 以 `FreeRangeSet` 出现，类型无消费者 | 无；D1 的两个发放方法（`take_lowest`/`take_page_top`）是 C 账本形状的回流，现设计已用 bump + classify 取代 | 强 | 0 | 否 |
| D1 `BootError` + errno | 保留各层域错误枚举；入口是 `!`（`arch_boot_resume_v1`/`enter_kernel_image_v1`），errno 无消费者 | 无；统一枚举会把十处本地精确类型换成一层跨层概念 | 强 | -多个名字，但 +1 跨层枚举 | 否 |
| D1 `ModuleDesc::Reclaimed` | 不表示回收态：模块表交接后只读，VM 排除发生在 `build_vm_handoff` | 无（现状无生产者；C 的置零已由“整段保留”取代） | 强 | -1 | 否 |
| D1 `Span{source}` | 保留两表 + `audit_source_coverage`（`memmap_source.rs:337`），判据相同 | 无；改成单集合会删掉独立来源表，同时丢掉 `id`/`origin`/`raw_type` 的取证细节，不是纯概念赢 | 中 | 0 | 否 |

## FINDING

- **SUB-01** D2 的 `EarlyMemory` 减法行目标错位：它把“删注册槽与三个原子、`&mut BumpRegion` 穿进 `Paging`”当成 `EarlyMemory` 的替代，而 D1 的 `EarlyMemory` 是“跨度集合 + 两个发放方法”的内存账（MINIMAL-DESIGN 第 4 项），不是页表页分配器。真实减法更省：不做，类型无消费者。结论（拒绝）不变，但理由必须改，否则会把“拒绝 EarlyMemory”误读成“必须动 Paging”。
- **SUB-02** DESIGN-07 的“`PhysRange` 统一 = 删两类型两套校验”不是纯减法：`MemoryRegion` 与 `SourceRange` 都能表达 `end=2^64`，`PhysRange` 不能（`address.rs:204` 只拒反向，端点仍是 `PhysBytes`）。统一会把 wire 域合法端点变成不可表示态；最小减法只有删私有 `MemRange`。
- **SUB-03** DESIGN-10 的前提是误读：`shim_windows: &[(u64,u64)]` 是固件侧生产者 → 内核的边界形参，正是 §4.4 边界规则的实例，不是“自我豁免”。减法 = 文档把“例外”改成“实例”，代码零改动。

## VERDICT

- **必须用户裁决**：DESIGN-01（UEFI 主链单写，需确认 EBS 后编码时序）；DESIGN-03（合并落库，且 15 vs 16 字节截断与非法 UTF-8 是回退还是 panic 只能选一个）；DESIGN-07b（全局统一，建议否）；DESIGN-08（wire v11，建议本轮否）；D1 `BootFacts` 若采纳“删 `BootReport`”属公开 API 改名。
- **可当轮删**：DESIGN-02、04、05、06、09、11；DESIGN-07a（只删 `MemRange`）；DESIGN-10 仅改文档措辞。
- **不做**：D2 ARCH-PROPOSAL（减法由 DESIGN-09 覆盖，ARCH 留给下一次 `Paging` 重构窗口再议）；D1 的 `EarlyMemory`、`BootError` 加 errno、`ModuleDesc::Reclaimed`、`Span{source}`。
- **对 D2 的修正**：DESIGN-01 的“`fill_map_tables` 无 once 守卫、第二次提交可覆盖 storage”与源码不符（`boot_storage.rs:145` 有守卫）；双写无对账这一点仍成立。SUB-01/02/03 是减法分析本身的缺陷，不是代码缺陷。
- 证据强度：全部锚点为逐行读源（L1），未编译、未做跨工具只读复核；DESIGN-07b/08 的改动面为文件数与容量估算。
