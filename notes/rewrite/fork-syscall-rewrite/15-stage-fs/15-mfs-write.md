# 15-mfs-write: MFS 写路径与块分配

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：数据通路写侧
> **源码**: `minix3/minix/fs/mfs/write.c`
> **Rust 模块**: `os/fs/mfs`（write）
> **draft 素材**: 无（新建）

## 核心点

- `write_map`：直接/间接/双重间接写入、WMAP_FREE（释放 + 空间接块回收）、new_ind/new_dbl 分配标记
- `new_block`：alloc_zone + 间接块建立（wr_indir 写入）、双间接路径
- `clear_zone`：EOF 前空洞清零（写越界前）、`zero_block`
- `wr_indir`/`empty_indir`：间接块更新与空块判定（回收）
- 空洞语义：未写块读零（read_map NO_BLOCK → 零块）；EFBIG/EROFS 写前检查（read.c 侧 fs_readwrite 写分支）

## 边界

- **前置依赖**: 09/14
- **不覆盖（移交）**: 读路径（14）、截断释放（13）
