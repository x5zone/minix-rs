DOC: 01-boot-shim-bootstrap.md
CHAPTER: 第 4 章 实现详解
VERDICT: FAITHFUL

核对基准：摘要 52 行，sha256 `2d2edcb9…`；切片 440 行，sha256 `a1b899d7…`（哈希与行数均相符）。
① 已修：摘要 L26 明确三条 v1 腿的模块顺序无编译期检查，直核腿由 `place_modules` 在装载时按 `NAME_POOL` 逐槽核对 basename，互换当场停机；与切片 L175、L231 一致。
② 已修：摘要不再出现 `PlacementPool`，也无 aarch64 专段；五条腿表述与切片 L80、L235 一致。
③ 已修：摘要 L20 按步 1 → 步 2（`RefuseToBoot`）→ `checked_shape` → 步 3（`Excluded`/`DevicePath`）→ `checked_len` → 步 4（`Side`）→ 步 5 排列，与切片 L142–146 一致。
④ 已修：摘要 L39 将一次性守卫（第二次调用直接 panic，不是同址 no-op）与同址指针相等跳过拷贝分开，并说明 `ptr::copy` 只用于部分重叠；与切片 L340 一致。
⑤ 已修：摘要 L50 把失败现场限定为直核腿 `place_modules` 停机、v1 腿没有同名跨二进制检查；与切片 L231 一致。
⑥ 已修：摘要 L41 补 arena 生命周期（解码为最后一次读取、`LastRead(arena) ≺ Publish(VMFreePages)`、UEFI 腿的 arena 不在 free list）；L42 补 `validate_before_tables` 不查空模块表而由 strict 门单独查。
通读未发现会改变下游判断的实质失真；入口、九动作、分类、五腿、门链、落地区与失败落点均与切片一致。
