# 17-备份与维护

> **状态**: pending（最小骨架，待改写）
> **定位**: 备份与维护
> **源码**: `minix3/minix/commands/{backup,cleantmp,progressbar,remsync,synctree,update_asr,update_bootcfg,updateboot,rotate,fix,mt}/`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 备份命令契约表（11 命令）：backup（文件备份）/remsync/synctree（同步）
- - 维护面：cleantmp（临时清理）/fix（修复）/rotate（轮转）
- - 引导更新面：update_asr/update_bootcfg/updateboot
- - 进度/介质：progressbar/mt（磁带）
- - 备份格式设计决策（本 stage）

## 边界

- - **前置依赖**: 14/15
- - **不覆盖（移交）**: 备份格式设计（本 stage 决策）

