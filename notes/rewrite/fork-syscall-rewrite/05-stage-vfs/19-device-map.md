# 19-device-map: 设备表与驱动映射

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 — 设备 I/O：dmap/smap 表
> **源码**: `dmap.c` 全文件、`smap.c` 全文件、`device.c` 全文件、`dmap.h`
> **Rust 模块**: （未实现）dmap/smap 模块
> **draft 素材**: `draft/09-globals-const.md` 部分（素材）

## 核心点

- dmap[NR_DEVICES]：主设备号 → 驱动 endpoint 表
- init_dmap/init_smap（main.c:451-452 调用点）
- do_mapdriver/map_driver/map_service：驱动注册（RS boot 服务映射）
- dmap_* 族：lock/unlock/unmap_by_endpt/driver_match/endpt_up/get_by_endpt
- smap 表（socket 驱动）：smap_map/make_smap_dev/get_smap_by_*
- do_ioctl/make_ioctl_grant：IOCTL 分派（BLK/CHR/SOCK 三路）与 grant 构造
- CTTY_ENDPT 语义、设备恢复（endpt_up）

## 边界

- 具体驱动 I/O 协议不覆盖（20~22）
