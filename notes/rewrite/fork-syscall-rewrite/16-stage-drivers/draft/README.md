# 16-stage-drivers — 驱动（占位）

> **状态**: 占位（57 个 crate 已建，待实装）
> **定位**: `minix3/minix/drivers/` 全量驱动占位；tty/memory/log 是 boot 关键路径

## 范围（按启动优先级）
- boot 关键: `tty`、`memory`、`log`
- 终端/键盘链路: `pckbd`、`pty`、`readclock`、`random`
- QEMU 常用: `virtio_blk`、`ramdisk`、`pci`、`fb`、`acpi`、`virtio_net`
- 其余 47 个: 占位后置
- 框架: `os/libs/minix-chardriver`、`minix-blockdriver`、`minix-netdriver`、`minix-bdev`、`minix-virtio`、`minix-usb`

## C 对应
- `minix3/minix/drivers/`

## 占位现状
- `os/drivers/{audio,bus,clock,eeprom,examples,hid,iommu,net,power,printer,sensors,storage,system,tty,usb,video,vmm_guest}/` 57 个 crate（stub）
