# 13-devmand-consumer: devmand 消费契约

> **状态**: pending（最小骨架，待改写）
> **定位**: devmand 守护进程对 /sys 的消费（阶段 6 外部消费者，**外部契约不实现**）
> **源码**: `minix3/minix/commands/devmand/`（main.c 942 + usb.y 134 + usb_scan.l 43）+ `etc/devmand/`（cfg + scripts）+ `etc/rc.minix:202`
> **Rust 模块**: 无（用户态守护进程，不在 server 重写范围）
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `main_loop`（:878-932）：轮询 `<path>/events`（默认 /sys）、ENFILE 容错、fgets 单行
- `handle_event`（:803-872）：解析 `"ADD <path> 0x%x"`/`"REMOVE …"`、`dev_type` 判定（USB_DEV/USB_INTF）
- `generate_usb_device_id`（:633-680）：8 属性读取（idVendor/idProduct/bDeviceClass/bDeviceSubClass/bDeviceProtocol/bInterfaceClass/bInterfaceSubClass/bInterfaceProtocol）
- 驱动匹配：`match_usb_driver`/`match_usb_id`（:238-296）、usb.y/usb_scan.l DSL（usb_driver/id{}/binary/devprefix/upscript/downscript）
- 驱动启停：`start_driver`/`stop_driver`（minix-service up/down <binary> -major -devid -label）、`run_upscript`/`run_downscript`/`run_cleanscript`（mknod/rm /dev/<label>，block/singlechar 脚本契约）
- major 位图（get_major/put_major，:592-628）、create_pid_file（/var/run/devmand.pid）、cleanup

## 边界

- **前置依赖**: 06（事件格式）+ 04（路径/属性）
- **不覆盖（移交）**: server 内部（01~09）、devmand 本体实现（外部程序）
