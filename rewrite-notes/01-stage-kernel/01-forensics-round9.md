# 01 · round-8 最小实现门槛核实（round-9：EBS 后析构面 / 重试缓冲策略 / 描述符遍历）

> **生成日期**：2026-10-28。
> **用途**：八轮外部评审对方案乙提出的三个实现门槛（EBS 成功后的析构与释放、重试中缓冲区不足的处置、原始描述符遍历纪律）的定向核实。只调查不改码，不重开任何架构选择。事实标 **已证实 / 反证 / 尚无证据**。核心结论先行：**三项均可写成可实施、可测试的硬性契约，无改变架构的阻塞项**；附带一处对 round-5 台账的更正（快照 A 图缓冲实际会被 FreePool）。

---

## A. EBS 后的析构、释放与 UEFI 状态（已证实盘点）

### A.1 `prepare_boot` 调用链的 Drop 盘点（逐对象）

| 对象 | 类型与 Drop 行为 | 析构时点相对 EBS | 处置 |
|---|---|---|---|
| `ScopedProtocol<SimpleFileSystem>`、`FileSystem`、`fs_proto` | RAII（drop = close_protocol，BS 调用）——`os/boot-shim/src/uefi_helpers.rs:fn read`（`UefiFileLoader` 的 FileLoader 实现）内局部 | **远早于 EBS**（文件读取区段，函数返回即 drop） | 已满足：协议句柄在循环前全部关闭 |
| 文件内容 `Vec<u8>`（`read` 返回值、段拷贝缓冲） | drop = FreePool（经全局 allocator） | 早于 EBS（装载区段内消费完） | 已满足 |
| 两表 `Vec<MemoryRegion>`、模块元数据、名字串、`platform_sources` | `Box::leak` → `&'static`，**永不 drop**（`build_memmaps` L403-404、`loader.rs:fn load_boot_modules_with_loader`、`find_platform_sources`） | 无析构 | 已满足（leak 模式在此正确） |
| `KernelLoadResult` | 纯标量四字段（`loader.rs:79-88`），无 Drop | — | 已满足 |
| `MemoryMapOwned`（`build_memmaps` 局部 `mmap`，L346） | `MemoryMapBackingMemory::drop`：**BS 活着时真调 `free_pool`**（uefi 0.33.0 `mem/memory_map/impl_.rs:339-350`，条件 `are_boot_services_active()`） | 快照 A 时点（EBS 前）drop | **round-5 台账 #7 更正**（见 D） |
| EBS 后路径：`BootPrepareResult` 构造、`raw_serial`、panic handler | 纯 move 标量 + 胖指针；`raw_serial` = 裸 `out dx,al` 端口写（`boot_shim/src/lib.rs:fn raw_serial`，无 fmt 无分配）；panic 用 `StackWriter` 栈缓冲 + `format_args!`（无堆） | EBS 后 | 已满足：零 allocator / 零 BS 访问 |

**结论（已证实）**：现状链上不存在「跨 EBS 存活且 Drop 会触 BS」的对象——RAII 协议与临时 Vec 全部在 EBS 前析构完毕，跨 EBS 的数据全是 `&'static` leak 切片、裸标量与（乙实施后的）自管裸 buffer。方案乙的实施形状与此一致：EBS 配对循环置于 `prepare_boot` 末节，前置代码的 Drop 全部完成于循环启动前；循环内只持有裸缓冲与纯计算；成功后路径（填信封、读游标、`raw_serial`、跳转）零分配零 Drop。

### A.2 `system_table_raw()` 与全局 allocator 在 raw EBS 后的防线（关键机制，已证实 + 固件依赖分层）

决定性源码（uefi 0.33.0 `mem/memory_map/impl_.rs:339`）——`MemoryMapOwned` 的 Drop 不是空操作：

```rust
// Don't drop when we use this in unit tests.
impl Drop for MemoryMapBackingMemory {
    fn drop(&mut self) {
        if boot::are_boot_services_active() {
            let res = unsafe { boot::free_pool(self.0.cast()) };   // ← BS 活着时真释放
            …
        } else {
            log::debug!("Boot services are exited. Memory map won't be freed …");  // 仅 EBS 后
        }
    }
}
```

而「BS 是否活着」的判据（`boot.rs:79-87`）：

```rust
pub(crate) fn are_boot_services_active() -> bool {
    let Some(st) = table::system_table_raw() else { return false; };
    let st = unsafe { st.as_ref() };
    !st.boot_services.is_null()      // ← 读固件管理的表字段；crate 自身从不写它
}
```

uefi 0.33.0 全 crate 无任何代码把 raw 系统表的 `boot_services` 字段置 null：`SYSTEM_TABLE: AtomicPtr<SystemTable>` 仅由 `set_system_table`（entry 时）写入。EBS 成功后该字段是否为 NULL 取决于固件：EDK2/OVMF 的 `CoreExitBootServices` 会把 `gST->BootServices` 置 NULL（这也是 crate 文档「the allocation will not be freed on drop」能成立的前提，以及上列 Drop 在 EBS 后走 log-only 分支的前提）。

**分层结论**：
- 已证实（crate 源码）：raw EBS 不经过 helper，crate 自身不做任何退出后状态处理——「BS 已退出」的可见性完全依赖固件置 NULL。
- 已证实（EDK2 行为）：OVMF 上 EBS 后 `uefi::boot::*` 误调会走 `NonNull::new(null).expect("boot services are not active")` **干净 panic**，防线有效。
- **固件行为依赖，非 crate 保证**：对不置 NULL 的固件，误调 = 对失效函数指针的 UB。因此实施契约不得依赖 panic 防线兜底——纪律条款：**raw EBS 成功后禁调一切 `uefi::boot::*` 函数与一切可能 Drop-to-FreePool 的路径**（扩展现有「EBS 后零分配」为「零分配、零释放、零 BS 调用」）。

---

## B. 重试中的缓冲区不足（契约可直接落文）

**项目 UEFI revision 支持声明：无**（全仓 grep `uefi_revision`/`UEFI 2.` 零命中）——因此按「兼容所有受支持 UEFI 版本」的最保守集合设计：EBS 首次失败后仅允许 GetMemoryMap 与重试 EBS（UEFI 2.8- 的限制集合；2.9+ 允许的失败后 AllocatePool 不使用——向下全兼容）。三段式处置（评审表格原样采纳）：

| 发生时点 | 允许的处理 |
|---|---|
| 首次 EBS 尝试之前，`GetMemoryMap` 报 `BUFFER_TOO_SMALL` | 允许扩容（`AllocatePool`）并重新取图——BS 活着，正常路径 |
| 任一 EBS 尝试失败后的重取 | **只复用原缓冲区，零新分配**（2.8- 兼容集合） |
| 复用重取仍 `BUFFER_TOO_SMALL` | **fail-closed：明确报错并停止启动**——不截断内存图、不静默继续、不进入版本感知扩容分支（将来若正式限定 UEFI 2.9+ 再评估，现在不引入状态分支） |

配套条款：**重试预算固定为总尝试 2 次**（helper 同款 `for _ in 0..2`，`boot.rs:fn exit_boot_services` 循环），超限走冷复位（helper 同款 `runtime::reset(ResetType::COLD, …)`）——不无限循环；**只有与成功退出所用 MapKey 配对的那次分类结果被接受**（v8 已裁，维持）。buffer 初始容量 = EBS 前 `boot::memory_map_size()` 实查 + `desc_size×8` 余量（`MemoryMapBackingMemory::new` 同款策略）。现有类型零调整——纯新增循环 + 契约 + 测试。

---

## C. 原始描述符遍历（契约 + 最小实现形状）

依据（已证实）：`uefi_raw::table::boot::MemoryDescriptor` 是 `#[repr(C)]`（ty:u32+pad + phys/virt/pages/att 四个 u64），带 `pub const VERSION: u32 = 1`；UEFI 规范允许描述符结构扩展（尾部追加），步长必须用 `GetMemoryMap` 返回的 `DescriptorSize`。

契约四条 + 实现形状：

```rust
// 循环前置（失败即 fail-closed 拒绝启动）：
//   version == MemoryDescriptor::VERSION（=1）           —— 不猜未来版本语义
//   descriptor_size >= size_of::<MemoryDescriptor>()      —— 比已知布局小 = 异常
//   map_size <= buffer.len() && map_size % descriptor_size == 0  —— 边界与整除守恒
// 步长 = descriptor_size（非 size_of）——扩展结构不误按旧布局遍历、不丢描述符；
// 遍历只读前缀字段（规范保证追加不改前缀布局）：
for off in (0..map_size).step_by(descriptor_size) {
    let d: &MemoryDescriptor = unsafe { &*(buf.as_ptr().add(off) as *const _) };
    // 分类：CONVENTIONAL → conventional 分区；MMIO/MMIO_PORT_SPACE → 跳过；
    //        其余 → reserved 分区（复刻 build_memmaps 的既有规则，含 ram_top 过滤）
}
// 循环体纪律：零分配、零释放、零 Boot Services 调用（GetMemoryMap/EBS 除外）、
// 零 fmt/日志——诊断计数留到循环外经 raw_serial 输出（裸端口写，无 BS 依赖）；
// uefi::println! 禁入循环（ConOut 是协议调用，可能扰动内存图/MapKey）。
```

---

## D. 附带更正：round-5 台账 #7

round-5 A 批 #7 称快照 A 图缓冲「与两张输出 Vec 一样是池分配、永不释放」。**反证成立、更正**：`build_memmaps` 的 `mmap: MemoryMapOwned` 是普通局部变量，函数返回即 drop，而 `MemoryMapBackingMemory::drop` 在 BS 活着时（快照 A 时点正是如此）真调 `free_pool`——即快照 A 缓冲在分类完成后**被释放**，该页回到 conventional（图再变一次；无人再读，无害；EBS 终图记它 conventional）。当时把项目代码「不显式 FreePool」（`UefiPoolAllocator::dealloc` 注释语境）误推为「crate 类型的 Drop 也不释放」。此误差不影响任何已下裁决（快照 A 在 v5 起已整体取消；乙用自管裸 buffer，无此 Drop 面），仅台账口径更正。`MemoryMapOwned` 的「drop 不释放」文档语义仅对 **EBS 后** 的 drop 成立（且依赖固件置 NULL，见 A.2）。

---

## 交付结论

1. **三项均形成可实施、可测试的硬性契约**：A = 「EBS 循环前完成全部 RAII 清理 + 循环与成功后路径零分配零释放零 BS 调用 + 防线依赖固件置 NULL 的分层说明」；B = 三段式缓冲处置 + 总尝试预算 2 + fail-closed；C = 四条前置检查 + descriptor_size 步长 + 循环体纪律。对应测试断言：重试零分配（map buffer 复用）、复用不足即停机、坏 version/小 desc_size/不整除拒绝、扩展 desc_size（> size_of）按步长正确遍历。
2. **无改变架构设计的阻塞项**。方案乙可冻结为实施契约；架构与协议形态维持 round-8 终裁（固定分区 + `byte_len` 描述符 + `arena_len` + 配对循环）。
