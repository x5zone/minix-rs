# 14-mfs-read: MFS 读路径与块寻址

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：数据通路读侧
> **源码**: `minix3/minix/fs/mfs/read.c`
> **Rust 模块**: `os/fs/mfs`（read）
> **draft 素材**: 无（新建）

## 核心点

- `fs_readwrite`（读侧）：find_inode、块大小、i_seek 清除、普通/非普通文件分支
- `read_map`：**直接（7 个 i_zone）/单重间接/双重间接块寻址**（zone 数 → block 换算：s_log_zone_size 缩放）、NO_ZONE/NO_BLOCK 空洞语义、PEEK（opportunistic）vs NORMAL
- `rd_indir`（间接块读取 + 字节序）、`get_block_map`（逐级遍历）
- `rahead`：预读（readahead 块队列）、`rw_chunk`：块内读写循环（chunk 边界处理）
- `fs_getdents`：目录项序列化输出（struct dirent 格式）

## 边界

- **前置依赖**: 09/11
- **不覆盖（移交）**: 写路径（15）、请求适配（02）
