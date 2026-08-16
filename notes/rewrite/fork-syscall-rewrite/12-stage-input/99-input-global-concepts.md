# 99-input-global-concepts: INPUT 全局概念

> **状态**: pending（最小骨架，待改写）
> **定位**: 常量总表与跨服务引用收口（阶段 99 全局概念）
> **源码**: `com.h:877-893`、`minix/input.h`、`minix/inputdriver.h`、`minix/chardriver.h`、`dmap.h:78`、`sys/kbdio.h`、`sys/sys/ttycom.h:174`
> **Rust 模块**: `minix-types`
> **draft 素材**: `draft/README.md` + `../../../../tmp/input/`（素材）

## 核心点

- 消息常量总表：TTY_INPUT_UP/EVENT（0x1302/0x1303）、INPUT_CONF/SETLEDS（0x1500/0x1501）、INPUT_EVENT（0x1580）、CDEV_*（0x400 段）
- 事件码总表：INPUT_PAGE_*/INPUT_KEY_*/INPUT_LED_*/INPUT_BUTTON_*/INPUT_CONS_*（锚 04）
- 设备编号总表：minor（0/1-4/64/65-68）+ DEV 下标（0-9）+ `INPUT_DEV_MAX=10` + `INPUT_MAJOR=64`
- 错误码汇总：ENXIO/EBUSY/EIO/EAGAIN/EINTR/ENOTTY/EINVAL + EDONTREPLY 伪回复（A-11）
- endpoint/DS label 约定：`drv.inp.<label>`（驱动发布）、`drv.chr.<label>`（chardriver announce）、`input`（ds label 查询）、DSF_INITIAL
- 全局状态：`devs[10]`（opened/suspended/selector/leds 跨 handler 共享）
- 跨服务引用：VFS（CDEV 请求 + /dev 节点）、DS（注册/移除）、TTY（UP/EVENT/SETLEDS）、pckbd（驱动侧）、RS（加载 + system.conf 权限）

## 边界

- **前置依赖**: 全部（01~14）
- **不覆盖（移交）**: 各机制细节（01~14）
