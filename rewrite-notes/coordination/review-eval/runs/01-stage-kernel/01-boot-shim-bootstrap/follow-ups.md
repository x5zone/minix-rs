# D01 follow-ups —— Round 0 清债进度

> V2.1 全修策略：范围内真缺陷一律本轮修。本表追踪每条的处置状态。
> 状态：✅ 已修 / ② 裁决 / ② 相邻 / ④ 待修 / STUCK

| ID | 来源 | 事项 | 状态 | 说明 |
|---|---|---|---|---|
| F01 | 复验 | minix-arch 45 编译错误 | ✅ 已修 `6e369393a` | 289 通过 |
| F02 | DESIGN-05 | static mut 二次写入 UB | ✅ 已修 `11feb8582` | 一次性守卫 + 测试豁免 |
| F03 | DESIGN-01 | 删 bootstrap_start/len；wire 版本 +1 | ② 裁决 | 等：C struct kinfo 兼容性取舍 |
| F04 | DESIGN-02 | 删 BootShim trait | ② 相邻 | code-excellence；跨模块 |
| F05 | DESIGN-03 | 删 PagingWithId + 死常量 | ② 相邻 | code-excellence；跨模块 |
| F06 | DESIGN-04 | BootForm 删除或降级 | ② 相邻 | code-excellence |
| F07 | DESIGN-07 | startup_objects 规范集合 | ② 裁决 | 等：容量与 root/bump 取舍 |
| F08 | DESIGN-08 | memmap ledger 替换 | ② 裁决 | 等：SMP/AP 复用 add_memmap 判断 |
| F09 | DESIGN-09 | MemoryRegion 非空/对齐 | ✅ 已修 `95682d4e1` | validate_impl 断言 |
| F10 | DESIGN-11 | GET_KINFO 载荷收窄 [ARCH] | ② 裁决 | 等：外部行为取舍（最高风险） |
| F11 | DESIGN-13 | Serializer authority 必填 | ✅ 已修 `95682d4e1` | 构造器参数 + 删 write_ram_authority |
| F12 | READER-A 6 条 | §1.2 依赖边等 6 项 | ④ 待修 | 下一批 |
| F13 | READER-B 15 条 | §1.4 边界重叠等 15 项 | ④ 待修 | 下一批 |
| F14 | READER-B-03 | 961 重叠推理重复四次 | ④ 待修 | 下一批 |
| F15 | READER-B-10/C-05 | 探针结论强于证据 | ④ 待修 | 下一批 |
| F16 | READER-B-16 | 四套形态计数 | ④ 待修 | 下一批 |
| F17 | READER-B-17 | §5 三表矛盾 | ④ 待修 | 下一批 |
| F18 | READER-C 12 条 | 一致性与指针项 | ④ 待修 | 下一批 |
