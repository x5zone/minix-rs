# D01 follow-ups —— 本轮转出项与去处

> 这些是真实但不在本轮阻塞范围的事项。每条都有目的地；没有目的地的项不允许留在本文件。

| ID | 来源 | 事项 | 去处 |
|---|---|---|---|
| D01-F01 | 复验 | `os/arch/src/arch/boot.rs` 测试模块有 45 个编译错误（缺 `Box`/`Vec`/`mock` 类型，缺 `memmap_sources`/`startup_objects` 字段初始化）；`cargo test -p minix-arch` 当前不是发布门 | test-audit / code-excellence 批次；先补 import 与字段，再判测试是否仍有语义价值 |
| D01-F02 | DESIGN-05 | `memmap_source` 的 DTB statics 与三个 boot face 的 `STARTUP_OBJECTS` 从 `static mut` 返回 `&'static` 切片，二次 collect 覆盖同一存储；单元测试会多次 collect，不能直接加全局一次性守卫 | code batch：owned 快照或带测试可见 reset 的一次性守卫；D01 标 `design-debt pending` |
| D01-F03 | DESIGN-01 | 删 `bootstrap_start/len` 字段与 `free_upper_idx` 交接；wire 版本 +1 | 用户裁决后进 code-excellence；与 C `struct kinfo` 兼容性一起判 |
| D01-F04 | DESIGN-02 | 删 `BootShim` trait，保留 `prepare_boot` 函数与 `BootPrepareResult` | code-excellence 批次 |
| D01-F05 | DESIGN-03 | 删 `PagingWithId`、`PTE_HUGE_IDENTIFIER_BIT`、`HUGE_PAGE_SHIFT` | code-excellence 批次 |
| D01-F06 | DESIGN-04 | `BootForm` 删除或降级到 `#[cfg(test)]` 并重写 battery | code-excellence 批次 |
| D01-F07 | DESIGN-07 | 统一 `startup_objects` 四条腿的规范集合，补 root/bump，评估 `WIRE_STARTUP_CAP=16` 余量 | 用户裁决 + code batch |
| D01-F08 | DESIGN-08 | 用有序区间集替换 `memmap.rs` 空洞数组与 `KERNEL_MAY_ALLOC` | 用户裁决；若 SMP/AP 近期复用 `add_memmap` 则只删全局布尔 |
| D01-F09 | DESIGN-09 | `MemoryRegion` 增加非空/页对齐构造，或在 `validate` 中断言 | code batch（低优先） |
| D01-F10 | DESIGN-11 | `GET_KINFO` 只返回 48 B `KinfoStruct`，C 侧复制完整 `sizeof(struct kinfo)`；需用户裁决标 `[ARCH]` 还是恢复全量布局 | 用户裁决；最高外部风险 |
| D01-F11 | DESIGN-13 | `Serializer::new` 的 authority 改为必填；`BootPrepareResult` 私有字段 + 校验构造 | code batch |
| D01-F12 | READER-A-01/02/03/04/11/15 | §1.2 依赖边缺一条、16 字节对齐依赖链接顺序、`mb_set_param` NUL 语义、`add_memmap` 空槽顺序、DTB 取模块歧义、`kern_stack_top` 与 L7 不等价 | 下一篇闭环轮逐条取证；A-02/A-03/A-04/A-15 需源码复核 |
| D01-F13 | READER-B-01/02/04/05/07/08/09/11/12/13/14/15/18/19/20 | §1.4 边界与第 3/4 章重叠、§1.1 时序倒置、§1.2 括注歧义、§2.4.1 指代、§2 路线表、§3.1/§2.7 例外、§3.2 标题与正文、§3.5 重复表、§3.6 偏题、状态事实重述、§2.6 取舍未响应、§4.8 错误回指、§6.3 C 失败模式、§2.4.1 未标记合取、§2.4.2 16 条截断 | 教学债批次：优先 §1.4 边界与 §3.2 标题（影响读者路径），其余按章内重排处理 |
| D01-F14 | READER-B-03 | §1.2 把 §2.6 的 961 重叠推理提前给出，全文重复四次 | 教学债批次；与 D-001 四章骨架不冲突，只调章内证据顺序 |
| D01-F15 | READER-B-10 / READER-C-05 | 深拷探针否掉两个候选后引入第三个未检验解释；探针形态与真机读数不同源 | 证据债：下一轮补一枚能区分第三因的探针，或把结论降为“相关” |
| D01-F16 | READER-B-16 | §3.7 三种形态、§3.4 五条生产腿、§4.8 四种交接形态、§5.1 三形态，四套计数 | 教学债批次：先定义“形态 / 腿 / 生产路径”三个词，再统一计数 |
| D01-F17 | READER-B-17 | §5.1 M 表、§5.3 TODO 表、§5.4 自检三表对“哪些机制有 TODO”互相矛盾 | 测试/证据债批次：以 M 表为单一源，TODO 表只引用 ID |
| D01-F18 | READER-C-06/08–17 | reader-c 其余一致性与指针项（交叉引用、归属、计数） | 下一篇闭环轮逐条复核；与本轮已修的 C-01/02/03/04/07 同族 |
