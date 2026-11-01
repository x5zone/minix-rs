# 01 · 引导链 v7 最后两项冻结核实（round-8：wire 分区边界与 WireSlice 冗余 / uefi 0.33.0 raw 路径复核）

> **生成日期**：2026-10-28。
> **用途**：七轮外部评审指出 round-7 的两处未闭合：①「载荷 ⊆ arena」验证不能区分「落在正确 wire 分区」——`WireSlice.addr` 在固定分区布局下是冗余自由度，须重评协议形态；② round-7 对「锁定版本无公开 raw 表获取器」的否定性取证不完整——漏查了 `uefi::table` 模块的 `system_table_raw`。本轮逐项重取证。事实标 **已证实 / 反证 / 尚无证据**，裁决建议独立标注。核心结论先行：**评审两点全部坐实——边界验证漏洞真实存在，协议改 B 形态（固定分区偏移 + 仅 `byte_len`，`WireSlice` 退役）；`uefi::table::system_table_raw()` 在 0.33.0 公开存在（round-7 反证成立、本报告自行推翻前论），探针编译证实配对式 GetMemoryMap/EBS 在保留 `uefi::entry`、零新增依赖下类型可达，boot-shim 的 crate 状态补丁为零——方案乙成本重估为约 50–80 行自管循环，其分类读取落在「Boot Services 活着」的规范明文窗口，改裁方案乙。**

---

## A. 单一 arena 的协议边界与 `WireSlice` 冗余（漏洞坐实；裁决采 B 形态）

### A.1 边界验证漏洞（已证实，评审指认成立）

v7 的验证规则是「全部 wire 载荷区间 ⊆ [arena_base, arena_end)」。arena 内部布局为信封 → wire 分区×5 → 语义工作分区，**该规则无法区分载荷指向何处**：`memmap` 的描述符若错误指向信封区、语义工作区、或与 `reserved` 分区重叠，「⊆ arena」检查照样通过——物理可读不等于指向该字段约定的那类数据。评审提出的三层区分（arena 整体有效 / 载荷在专属分区内 / 容量对齐重叠规则）是必要最小集；v7 只做到了第一层。

### A.2 `WireSlice.addr` 是冗余自由度（已证实）

v7 同时规定：arena 分区位置与容量 = 编译期常量、全部载荷 ⊆ arena、无 arena 外自由放置需求。此时每块载荷的绝对物理地址**完全可由 arena 基址 + 协议布局推导**——描述符再自报一次地址，是重复表达，且这个多余的自由度正是 A.1 漏洞的来源：接收方必须「信任后验证」一个本可不存在的信息。对照项目既有品味（构造保证优于验证拦截——方案甲消灭「快照后分配」类别、`cut_memmap` 页化由底层构造保证），同一原则在此的落点就是**删掉地址字段**。

### A.3 两协议比较（已证实维度的对表）

| 维度 | A：`WireSlice { addr, byte_len }` + 逐分区验证 | B：固定分区偏移 + 描述符仅 `byte_len`（裁决） |
|---|---|---|
| 「指向错误分区 / 跨分区重叠 / 指向信封或语义区」 | 可表达，靠验证拦截（每字段验 ⊆ 专属分区区间） | **不可表达**——描述符无地址，位置由协议布局唯一确定（构造性消除） |
| 验证规则 | §1.3 四条全保留（溢出、对齐、⊆、上限）+ 逐分区区间检查 | ①溢出与③⊆ 消失（无地址加法）；②对齐由分区基址编译期保证；**只剩 ④`byte_len ≤ 分区容量` + 元素大小整除（编码守恒）** |
| 零长度切片 | 还需规定零长时地址指向何处（又一自由度） | `byte_len = 0` 即该分区起点零长载荷，语义自明（platform_sources/param_buf 可空；memmap 非空由 validate） |
| 布局门禁 | wire 元素 size/align/offset 逐目标基准 | 同左 **+ 分区偏移进协议常量基准**（同一门禁收口，非新增机制） |
| 版本升级 | 分区布局变化不触发版本（地址自报） | 分区布局变化触发协议版本——但 wire 元素布局变化本就触发（v3 递归闭合裁决），**演进触发面不扩大** |
| 信封形状 | 每字段 `WireSlice`（16 B×5） | 每字段 `byte_len: u64` ×5 + `arena_len: u64` |
| 保留独立地址的真实需求 | —— | 逐项排查均不存在：arena 外自由放置（v7 已禁且无需求）、分区动态排布（无需求）、未来载荷共享（届时升版本再引入，无现行理由） |

### A.4 裁决（标注：裁决建议）：采 B；`WireSlice` 退役、正式推翻 v5 已裁条目

- **协议形态 B 定案**：每块载荷在信封中的描述符 = `byte_len: u64`（实际字节数）；载荷地址 = `arena_base + 分区偏移`（协议版本固定的布局常量，与编译期常量同源）。`WireSlice { addr, byte_len }` **退役**——这是对 v5「`WireSlice` 精确表示已裁」的正式推翻，时机正当（协议未落地，正是 GPT 指出的最终判断窗口）；推翻的连带项：§1.3 区间公式收敛为「容量 + 编码守恒」两条（①②③ 升格为构造保证）。
- **`size` 字段语义钉死**：协议头 `size` = **信封结构自身大小**（v3 语义维持）；接收方需要 arena 总长的契约由新字段 `arena_len: u64` 承载——信封构造时填协议常量 `ARENA_CAP`（16 KiB），接收方验证「信封 `arena_len` == 协议常量」+「入口指针 + `arena_len` ⊆ 声明可读区间表」两个动作，无需查表推断。
- `WirePhysAddr`/`WireVirtAddr` 的**逐字段声明原则不变**（信封的 `root_page`/`bump_*` 仍是物理地址语义字段）；受影响的只是切片描述符形态。
- 分区布局进协议版本的定义：分区次序（memmap → reserved → modules → platform → param → 语义工作区）、各分区容量常量、基址对齐——全部写入 `BootHandoffV1` 协议文档并进布局门禁基准。

---

## B. uefi 0.33.0 的 raw table 获取路径（round-7 反证成立；裁决改采方案乙）

### B.1 round-7 的取证漏洞（已证实，本报告自行推翻前论）

round-7 只检索了 `uefi::system` 模块（`system.rs`），漏查 `uefi::table` 模块。锁定版本的真貌：

```console
$ grep -n "pub fn system_table_raw" uefi-0.33.0/src/table/mod.rs
20:pub fn system_table_raw() -> Option<NonNull<uefi_raw::table::system::SystemTable>>
32:pub(crate) fn system_table_raw_panicking() -> NonNull<…>   # 内部包装，非唯一入口
$ grep -n "pub boot_services" uefi-raw-0.9.0/src/table/system.rs
27:    pub boot_services: *mut BootServices,
$ grep -n "pub get_memory_map\|pub exit_boot_services" uefi-raw-0.9.0/src/table/boot.rs
    pub get_memory_map: unsafe extern "efiapi" fn(size: *mut usize, map: *mut MemoryDescriptor,
                              key: *mut usize, desc_size: *mut usize, desc_version: *mut u32) -> Status,
    pub exit_boot_services: unsafe extern "efiapi" fn(image_handle: Handle, map_key: usize) -> Status,
```

即：**`uefi::table::system_table_raw()` 公开存在**（0.31.0 加入、0.33.0 未删——与评审给出的上游发布记录一致），返回的 raw `SystemTable` 的 `boot_services` 是 pub 字段，其 `get_memory_map`（外部缓冲五参版）与 `exit_boot_services(image_handle, map_key)`（raw 配对入口）均为 pub 函数指针字段。round-7「无公开表获取器 → 必须自写 UEFI 入口」的推理链前提**不成立**。

### B.2 探针验证（已证实，最小 cargo check，不触碰生产代码）

在 `/tmp/probe-uefi-raw/`（会话产物域，不入库）建最小 `no_std` lib，仅依赖 `uefi = "=0.33.0"`（未加 `uefi-raw` 直接依赖）：调用 `system_table_raw()` → 解引用取 `boot_services` → 以外部缓冲五参形状调用 `get_memory_map` 函数指针 → 取 `exit_boot_services` 字段。`cargo check --offline` **编译通过**（两次：含与不含 `uefi-raw` 直接依赖均过——字段访问经类型推断，无需命名 raw 类型）。结论：**保留 `uefi::entry` 入口、零新增依赖，配对循环类型层可达**（运行时行为仍属待真机验证，标注：运行面尚无证据，但与 helper 内部走的是同一批规范函数）。

### B.3 crate 内部状态（已证实：补丁为零）

`uefi 0.33.0` 的 `exit_boot_services` 高层封装在调用 raw EBS 前做的唯一内部状态处理是 `helpers::exit()` → `logger::disable()`（logger feature 开启时）。boot-shim 的依赖是 `uefi = { features = ["alloc"] }`（`os/boot-shim/Cargo.toml:45`）——**logger feature 未开**，该处理为空操作。allocator 无需状态补丁：EBS 后禁用一切分配是调用纪律（方案甲/乙同要求，已裁「EBS 后零动态分配」）。`uefi::Handle` 与 raw `Handle = *mut c_void` 经 `#[repr(transparent)]` 平凡互转（`Handle::from_ptr`/`as_ptr` 族）。

### B.4 成本重估与两案比较（裁决性）

| 维度 | 方案甲（helper 返回 `MemoryMapOwned`，EBS 后分类） | 方案乙（配对循环内分类，裁决改采） |
|---|---|---|
| 真实改动 | 捕获返回值（约一行）+ 分类输入换图 | 自管循环约 50–80 行 unsafe：外部缓冲 `GetMemoryMap` → 分类双写 arena → `EBS(map_key)`；`BUFFER_TOO_SMALL` 分支扩 buffer；失败重取重分类（仅成功 MapKey 对应的结果有效） |
| 入口 / 依赖 / crate 状态 | 无变化 | **无变化**（探针证实：`uefi::entry` 保留、零新依赖、状态补丁为零） |
| 分类读取的规范地位 | EBS 后窗口——**规范沉默区 + 实现事实**（round-7 双标签如实登记） | **Boot Services 活着**的窗口——规范明文保证的运行环境（任何合法分配可访问）。「执行代码」的前提两案共有（切根/跳转同样依赖固件页表），但**分类数据作为正确性输入的读取**在乙下完全落在明文区——这是 round-7「前提两案共有」论述里被低估的质的差别 |
| 图与分类的配对 | 隐式（helper 内部 GetMemoryMap 紧贴 EBS） | **MapKey 显式配对**（MapKey 匹配 ⟹ 分类图 = EBS 成功时点的图；round-7 已证等价性）——重试循环正是 UEFI 规范钦定的 loader 模式（Linux EFI stub 同款先例） |
| map buffer 所有权 | helper 内部（`AllocatePool`，EBS 前一刻分配，行为上游管理） | 自管：EBS 前经 `boot::memory_map_size()` 实查 + `desc_size×8` 余量单独 `AllocatePool(LOADER_DATA)`（helper 同款策略），终图记一段 LOADER_DATA，分类后死页常驻 |
| unsafe 面 | crate 内（上游维护） | 本项目约 50–80 行——但 raw 表字段 = **UEFI 规范 ABI**（函数指针表与规范原型一一对应，比 crate 高层 API 更稳定，uefi-raw 0.9 演进风险≈0） |
| 重试纪律义务 | 上游 | 本项目成文：循环体零 Boot Services 调用（GetMemoryMap/EBS 除外）、零分配——写进代码契约与验收测试 |
| 真机验证 | 现状路径形态已跑 | 新路径需一轮真机验证（走的是同一批规范函数） |

### B.5 裁决（标注：裁决建议）：改采方案乙；方案甲的「捕获返回值」不再实施

- 正确性论证等级：乙把「分类读取」从「实现事实 + 规范沉默区」搬进「规范明文窗口」，并把图配对从「helper 内部行为的隐式信任」升级为「MapKey 显式验证」——与本项目一贯的品味同向（时序巧合 → 结构保证、快照 A → 终图）。
- 成本：round-7 的高估（自写入口 + 双真值源）经探针证伪；真实成本 ≈ 50–80 行规范 ABI 面的循环 + 一轮真机验证。
- 方案甲的既有论证（暴露窗口毫秒级 + 真机实证）仍成立，但乙在同成本量级下把同一窗口整体消除——「简单性优势」只剩「少写 50 行」，不足以抵消正确性论证等级的差距。
- 四步时序随乙更新：**① EBS 前**：arena（4 页）+ map buffer 分配；序列化全部**非内存图**载荷（模块名、平台来源、param——它们不依赖内存图，BS 活着时全部完成）→ **② EBS 配对循环**：外部缓冲 `GetMemoryMap` → 分类双写（`WireMemoryRegion` 进 wire 分区、`MemoryRegion` 进语义分区）→ 填信封静态字段 → `EBS(map_key)`；失败重取重分类 → **③ 切根**（此后不读任何固件映射数据——比甲更强，序列化已全部完成）→ **④ 切根后**：arena 信封原地补运行期字段（bump 三件套），跳转 `x0 = arena 基址`。镜像侧不变。
- 方案甲降级为乙的退化形态记录于版本史；若真机验证乙失败且短期不可修，回退路径 = 甲（改动仍是一行捕获）。

---

## 汇总：会改变 v7 设计的决定与剩余阻塞

| 项 | 处置 |
|---|---|
| `WireSlice { addr, byte_len }` | **退役**（推翻 v5 已裁条目，协议未落地时的正式推翻）；描述符改 `byte_len: u64` ×5，地址由 arena 基址 + 协议固定分区偏移推导 |
| 协议头 `size` / arena 总长 | `size` = 信封结构大小（语义钉死）；新增信封字段 `arena_len`（= 协议常量，验证相等） |
| §1.3 区间验证公式 | 收敛为两条：`byte_len ≤ 分区容量` + 元素大小整除；溢出/对齐/⊆ 升格为布局构造保证 |
| 分区布局 | 进协议版本定义 + 布局门禁基准（次序、容量常量、对齐） |
| 内存图权威时序 | **方案甲 → 方案乙**：分类移入 GetMemoryMap→EBS 配对循环（规范明文窗口 + MapKey 显式配对）；`MemoryMapOwned` 捕获不再实施；map buffer 自管（EBS 前单独 LOADER_DATA 分配） |
| 四步时序 | 更新为乙形态（步骤①含非图载荷序列化；步骤②为配对循环） |
| 剩余冻结阻塞 | **无概念级项**。实施首批义务：分区常量与信封字段钉死（随布局门禁）、乙循环的规范 ABI 面 + 重试纪律成文、一轮真机验证（乙路径 + arena 交接端到端）——均属实施验收非设计 |
