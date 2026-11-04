# CONTEXT.md — 内存图政策术语表

本文件只放**已敲定的术语**，不放规格、不放实现细节。术语在 `rewrite-notes/01-stage-kernel/01-adjudication-brief-memmap-open-items.md` 里首次定义，这里登记规范名与一句话含义。

## 输入域

| 术语 | 一句话含义 |
|---|---|
| **物理来源域** | 回答「哪些地址被来源声明过」：UEFI 内存图、非 UEFI `/memory`、设备树保留声明（`/reserved-memory` 与 MRB）、固件常驻前缀。它产生 `SourceDomain`、PMM 统计域与来源政策。 |
| **启动对象义务域** | 回答「本项目自己拥有哪些对象、在什么阶段必须怎样访问」：由 `StartupObjectInventory` 产生对象排除、四态访问义务、映射义务与对象冲突检查。**它不是物理内存来源，不参与 `SourceDomain`。** |
| **设备授权与使用域** | 回答「哪些设备窗口/端口被授权、项目代码实际怎么访问」：由 `DeviceWindowPolicy`、`PortWindowPolicy` 与实际访问点产生设备窗口、映射义务与实际访问合同。 |

## 设备记录

| 术语 | 一句话含义 |
|---|---|
| `DeviceWindowPolicy` | **唯一授权权威**：哪个来源允许哪个物理窗口，允许的树/阶段/路径与访问语义范围。 |
| `DeviceMappingRequirements` | **实际映射义务**：物理范围、树、阶段、虚拟关系、精确权限、映射内存语义、`AuthorityId` 引用。不存访问合同。 |
| `DeviceAccessRequirements` | **实际访问合同**：读写方向、宽度、对齐、幂等性与副作用、排序、推测、原子。第一版为规划端审计输入，不跨界。 |
| `PortWindowPolicy` | 端口地址域的授权政策，与 `DeviceWindowPolicy` 同构；第一版为编译期常量表，不跨界。 |
| `AccessUseId` | 每个实际设备访问点的稳定身份；属性含 `(tree, phase, path, window)`。一条映射义务可被多个 `AccessUseId` 引用。 |
| `AccessUseId` | 访问用途实例的稳定主键（MMIO 与端口 I/O 共用）；`(tree, phase, path)`（MMIO）与 `(caller, phase)`（端口）只是它的属性，不能当唯一键 |

## 集合与判定

| 术语 | 一句话含义 |
|---|---|
| `SourceDomain` | 来源声明过的物理范围（按启动方式构造），只回答「收到了什么」。 |
| `PmmPolicyUniverse` | `SourceDomain` 减去两份域外集合后的统计域。 |
| `ObjectBackingPages(object, boot_mode, source_records)` | 某对象可用的合法物理页 backing 判定结果；`R24b` 的右端，不是 `SourceDomain` 的同义词。 |
| `FinalRootRequiredMappings` | `OrdinaryMemoryMappingRequirements(S3) ∪ OrdinaryMemoryMappingRequirements(S4)`；最终根在 S3 构造点与 S4 都要满足的完整目标。 |
| `TreePhasePairs` | 合法树×阶段组合域：`(引导根,{S2,S3,S4}) ∪ (最终根,{S3,S4})`。S1 不属于任何项目树，其访问条件进平台假设清单。 |
| **顶层断言记录** | 在材料册追溯表里有独立 `test_id`、可独立失败的一条具名断言。`R13a–e` 是 `R13` 的分解，不计。当前 **39 条 / 24 个关系组**。 |
