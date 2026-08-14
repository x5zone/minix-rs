# 24-page-cache: 页缓存

> **状态**: pending（最小骨架，待改写）
> **定位**: 跨服务协作（mapped file 的页缓存）
> **源码**: `minix3/minix/servers/vm/cache.c`（332 行）、`mem_cache.c`（324 行）
> **Rust 模块**: `page_cache.rs`
> **draft 素材**: `draft/25-page-cache.md`（素材）

## 核心点

- `cache.c` + `mem_cache.c` 全部语义：双哈希、LRU（`cache_lru_touch`）
- `find_cached_page_bydev/ino`、`addcache`/`rmcache`/`cache_freepages`/`clear_cache_bydev`/`get_stats_info`
- IPC handler：`do_mapcache`/`do_setcache`/`do_forgetcache`/`do_clearcache`

## 边界

- **前置依赖**: 12/23
- **不覆盖（移交）**: VFS 请求队列（23）
