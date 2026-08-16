# 01-mib-init-main: 启动入口与主循环骨架

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 启动入口
> **源码**: `minix3/minix/servers/mib/main.c:277-383,415-492`
> **Rust 模块**: `main.rs`、`lib.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `main`：`mib_startup`（SEF 注册）→ 主循环（`sef_receive_status(ANY)` → notify 拒绝 → switch → 回复）
- `mib_startup`：`sef_setcb_init_fresh/restart(mib_init)` + `sef_startup`（A-7：重启丢全部动态状态，静态树保留）
- `mib_sysctl` 消息解码：namelen 校验（0 < len ≤ CTL_MAXNAME=12）、name 短/长拷贝（≤ CTL_SHORTNAME=8 消息内嵌 / >8 `sys_datacopy`）、oldp/newp 构造、call 结构组装
- 回复语义（A-11）：r≥0 → oldlen 写回 + 溢出时 `ENOMEM`（部分拷贝 + 完整长度）；r<0 → `call_reslen`（错误携带 oldlen，如 create 冲突 EEXIST）
- notify 拒绝（打印告警 + continue）与 default → `ENOSYS`（SENDREC）/`EDONTREPLY`（非阻塞）
- `EDONTREPLY` 抑制回复；`ipc_sendnb` 失败仅告警不 panic

## 边界

- **前置依赖**: 00 + kernel `12-ipc-core`/`09-vm-boot-protocol`
- **不覆盖（移交）**: SEF init 回调体（04）、分发核心（10）、各 handler（09/13~20）
