DOC: 01-boot-shim-bootstrap.md
CHAPTER: 第 4 章 实现详解
VERDICT: FAITHFUL

核对基准：规范摘要 52 行，sha256 `2d2edcb9…`；对照切片 `ch4-slice-candidate.md`（440 行，sha256 `a1b899d7…`，哈希与行数均相符）。

逐条复核：
1. 第 26 行已改为「三条 v1 腿的模块顺序没有编译期检查……编译期常量只核对池自身（截断/NUL）」，并保留直核 `place_modules` 的 basename 核对；与切片第 175、231 行一致。
2. 第 26 行已删除 `PlacementPool`。
3. 第 39 行已把一次性守卫的「第二次调用直接 panic，不是同址 no-op」与「同址由指针相等检查跳过拷贝」分开；与切片第 340 行一致。
4. 第 48–50 行已不再出现两个 aarch64 产物分叉与 `x0 == 0`；失败现场改为直核 `place_modules` 停机、v1 腿无同名跨二进制检查，与切片第 231 行一致。
5. 第 20 行已按步 1/2/闸/3/闸/4/5 重写，补 `RefuseToBoot`、`checked_shape`、`checked_len`、`TYPE_INDEPENDENT` 与类型表；与切片第 142–146 行一致。
6. 第 50 行已限定「直核腿模块名对不上由 `place_modules` 停机，v1 腿没有同名跨二进制检查」；与切片第 231 行一致。
7. 第 13 行已补「UEFI 腿的 bump 区不在 conventional 列表里，这条扣除对 free list 是空操作」；与切片第 76 行一致。
8. 第 41 行已补 arena 生命周期合同：解码是最后一次读取、`LastRead(arena) ≺ Publish(VMFreePages)`、UEFI 腿的 arena 不在 free list；与切片第 372 行一致。
9. 第 42 行已补「`validate_before_tables` 不查 `boot_modules` 非空（QEMU 测试内核合法携带空模块表）；模块非空由 strict 门单独查」；与切片第 279、308 行一致。

非阻断精确化建议：
10. 第 25/31/32/34/52 行的 `read` 返回 `Option`、`KernelInfo` 去掉 `authority`、`fill_source_records`、`validate_view`、「本章不写页表建立与映射细节」仍使用切片未出现的措辞；它们不改变本章主结论，建议下版改用切片措辞或删除。
11. 第 19 行可补切片第 126 行的「第一次」；第 20 行可补第 148 行的「同一地址域」、并把 `Available` 改用第 146 行的 `CONVENTIONAL` 可分配；第 21 行可补第 162 行的「越界整张图作废，不做截断」。

结论：11 条的主干已落实，摘要与切片在入口、九动作、分类顺序、五腿、门链、落地区、arena 与失败落点上一致；未发现新的实质性失真。第 10、11 条为措辞与边界精确化，不影响 FAITHFUL。
