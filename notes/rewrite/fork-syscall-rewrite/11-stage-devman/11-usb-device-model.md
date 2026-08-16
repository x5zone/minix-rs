# 11-usb-device-model: USB 设备建模

> **状态**: pending（最小骨架，待改写）
> **定位**: libdevman 的 USB 高层封装（阶段 5 客户端契约）
> **源码**: `minix3/minix/lib/libdevman/usb.c`（301 行）+ `minix/include/minix/devman.h` lib 侧结构
> **Rust 模块**: `usb_model.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `devman_usb_dev`/`devman_usb_interface` 模型（dev_id/desc/configuration/intf_count/interfaces[32]/cb_data）
- 属性生成：`add_device_attributes`（bDeviceClass…dev_type="USB_DEV"）、`add_interface_attributes`（bInterfaceNumber…dev_type="USB_INTF"）
- `devman_usb_device_new`/`delete`（:143/:174）、`devman_usb_device_add`/`remove`（:219/:276：设备 + N 接口逐个注册）
- bind/unbind 回调接线：`devman_usb_bind_cb`/`unbind_cb` + `devman_usb_init`（:295）
- dev_type 属性是 devmand 判定的契约（13）

## 边界

- **前置依赖**: 10
- **不覆盖（移交）**: devmand 匹配（13）、usbd 驱动内部
