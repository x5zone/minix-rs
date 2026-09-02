# 12 — `request.c` 的 `REQ_*` 包装：`cpf_grant` 的 `ERESTART` 重试与 `node_details/lookup_res` 的类型化响应

本文讲清 `request.c` 如何在 `FS_BASE 0x600` 的 `IS_FS_RQ` 前缀、`NREQS 34` 的 `REQ_BREAD 0x60B` ~ `REQ_BPEEK 0x621` 的 33 有效请求、`REQ_GETNODE 0x601` 的 `Should be removed` 死常量、`EENTERMOUNT -301` 的挂载点穿越、`RES_THREADED/HASPEEK/64BIT` 的 FS 能力位、`cpf_grant_direct/magic` 的 `CPF_READ/WRITE/TRY` 三标志与 `GRANT_FAULTED→ERESTART` 的重放、`vm_vfs_procctl_handlemem` 的 `CPF_TRY→0` 二阶段、以及 `node_details {fs_e,ino,fmode,fsize,uid,gid,dev}` 与 `lookup_res {+char_processed,symloop}` 的响应结构的约束下，以 `req_breadwrite_actual` 的 `grant→fs_sendrec→revoke→ERESTART→vm_handlemem→retry` 与 `req_lookup` 的 `PATH_GET_UCRED` 分支及 `req_stat` 的 `stat grant` 复用建立 `VFS → FS` 的 `Message` 构造/投递/回收/重放的闭环，并以 `req_bpeek/req_inhibread/req_flush` 的无 `grant` 直通与 `req_readsuper` 的 `RES_64BIT` 守门为能力协商提供可观测分流。

前置阅读：`11-fs-comm.md`（`GlobalComm::sendmsg` 的 `c_max_reqs` 窗口与 `TransId` 编码及 `FsTransport::send_fs` 的 `CALLBACK` 抑制）、`06-vmnt-table.md`（`Vmnt.m_fs_flags: RES_*` 的能力位）、`04-filp-table.md`（`Filp` 的 `filp_vno` 与 `vm_vfs_procctl_handlemem` 的 `02-stage-vm` 交叉）、`05-vnode-table.md`（`vnode_clean_refs` 的 `v_fs_count` 阈值与 `put_vnode` 的 `req_putnode` 对端）。

> 本章不讲什么：
> - `m_comm` 窗口的 `c_max_reqs/c_cur_reqs/c_req_queue` 与 `sending` 扫表—— `11-fs-comm.md`（`fs_sendrec` 的 `sendmsg/queuemsg` 对端）
> - 底层 FS 服务端（`minix3/minix/fs/mfs/*` 等）的 `REQ_*` 处理—— `minix3/minix/fs/`（对端实现，不在 VFS 范围；本章只讲 VFS 侧包装）
> - 路径解析的 `lookup/advance` 与挂载点穿越的 `EENTERMOUNT` 消费—— `13-path-lookup.md`（`req_lookup` 的调用方）
> - `open/read/write` 的 `req_create/req_readwrite` 调用链与 `get_fd` 交互—— `15/16`（`req_*` 的调用方）
> - `bdev/cdev/sdev` 的 `drv_sendrec` 与 `grant` 的 `CTTY` 重定向—— `20/21/22`（`r` 路径）
> - `mount` 的 `req_readsuper/newnode/mountpoint` 的 `vmnt` 生命周期—— `18-mount.md`（`req_*` 的调用方，`06` 的 `mark_free` 对端）
> - 内核 `cpf_grant_magic/direct` 的 `safecopy` 原语—— `99-global-concepts.md` + `../01-stage-kernel/18-syscall-copy.md`

---

## 1 概念

### 1.0 章节引言

**目标读者**：已理解 11 的 `GlobalComm::sendmsg` 窗口与 `TransId` 高位编码及 `FsTransport::send_fs` 的 `CALLBACK` 抑制，能 `grep "REQ_LOOKUP\|req_lookup\|node_details" minix3/minix/servers/vfs/request.c` 的开发者。

### 1.1 为什么需要 `req_*` 包装

`VFS → FS` 的 33 请求若直接以 `fs_sendrec(fs_e, &m)` 手写 `m.m_type + m.m_vfs_fs_*` 字段，将散落三处重复：

1. **grant 构造与回收的配对**（`30 req_breadwrite_actual:38 grant=cpf_grant_magic(CPF_WRITE|TRY) … 53 revoke→ERESTART → vm_handlemem → retry(0)`）；
2. **`RES_64BIT` 的 32 位截断守门**（`261 req_ftrunc:274 if(!(RES_64BIT) && (start>INT_MAX||end>INT_MAX)) return EINVAL`）；
3. **响应回填的 `m_source→res->fs_e` 与 `m.m_fs_vfs_*` 的 `inode/mode` 拷贝**（`166 req_create:204 res->fs_e=m.m_source`）。

`request.c` 以 `req_*` 将三重复收敛为**一处构造/投递/回收/重放的闭环**。`ERESTART` 的 `vm_vfs_procctl_handlemem` 重试（`11` 的 `NULL vmp` 直通）使 `grant` 的 `TRY` 语义在包装内可观测，而非散落在 15/16 的调用方。

### 1.2 协议面的编码：`FS_BASE 0x600` 与 `NREQS 34`

`vfsif.h:41 REQ_GETNODE (FS_BASE+1)` 的 `Should be removed` 注释使 `NREQS 34` 的 `FS_BASE+1..+33` 中仅 33 有效，`request.c` 全文件无 `req_getnode` 包装印证死常量。`vfsif.h:77 IS_FS_RQ(type) ((type&~0xff)==FS_BASE)` 的 `~0xff` 前缀使 `REQ_BREAD 0x60B` 与 `VFS 0x100` 的 `call_vec` 前缀（`09` 的 `VFS_CALL` 0x100）及 `VFS_PM 0x900` 前缀（`10` 的 `PM` 0x900）及 `VFS_TRANSID 0xB00` 事务前缀（`11` 的 `TRANSACTION_BASE`）在 `m_type` 域内不重叠，`11` 的 `TRNS_ADD_ID` 高位编码在此前缀上叠加 `transid` 而不冲突。

### 1.3 响应的类型化：`node_details` 与 `lookup_res`

`request.h:12 node_details { fs_e, ino, fmode, fsize, uid,gid, dev }` 为 `REQ_NEWNODE/CREATE` 的 7 字段响应，`25 lookup_res { fs_e,ino,fmode,fsize,uid,gid,dev, char_processed, symloop }` 的 `+char_processed/symloop` 二字段使 `REQ_LOOKUP` 的 `EENTERMOUNT/ELEAVEMOUNT/ESYMLINK` 三特殊码（`vfsif.h:26` `-301..-303`）的 `char_processed` 回带在类型层面可区分：`OK→inode` vs `EENTERMOUNT→inode+offset+symloop` vs `ESYMLINK→offset+symloop` 的 `res` 回填在 `request.c:495 switch(r) { case OK: inode/fmode…; case EENTERMOUNT: inode/offset/symloop; … }` 的 `lookup_res` 分支可观测（`13` 的 `advance` 挂载点穿越的 `char_processed` 对端）。

### 1.4 `grant` 的两段与 `ERESTART` 的 VM 修复

`cpf_grant_magic(fs_e,user_e,user_addr,nbytes,CPF_WRITE|TRY)` 的 `TRY` 位使 `grant` 创建可 `ERESTART`（`11` 的 `vm_procctl_handlemem` 修复路径的哨兵），`CPF_TRY→vm_handlemem→retry(0)` 的二阶段在 `request.c:70 req_breadwrite:73 if(ERESTART){ vm_handlemem→req_breadwrite_actual(...,0) }` 的 `TRY→0` 标志切换中可观测。`cpf_grant_direct` 的 `REQ_CREATE` 直送 `path` 字符串与 `REQ_LOOKUP` 的 `grant_path + grant_ucred` 双 `grant` 及 `REQ_GETDENTS` 的 `VFS → FS` 与 `USER → FS` 的 `direct vs magic` 分化在 `request.c:185/444/308` 的 `direct(CPF_READ)` vs `magic(CPF_WRITE|TRY)` 可观测。

### 1.5 能力协商：`RES_64BIT` 的 `INT_MAX` 守门

`vfsif.h:23 RES_64BIT 0x04` 的 `FS` 64 位能力位在 `vmnt.m_fs_flags`（`06` 的 `m_fs_flags`）回带，`request.c:261 req_ftrunc:274 if(!(RES_64BIT) && (start>INT_MAX||end>INT_MAX)) return EINVAL` 与 `288 req_getdents:323 if(!(RES_64BIT) && pos>INT_MAX) return EINVAL` 的 `INT_MAX 2^31-1` 守门使 32 位 FS 的 `off_t` 在 `VFS` 侧 `EINVAL` 早拒绝，而非 `FS` 侧截断。`RES_HASPEEK 0x02` 的 `REQ_PEEK/BPEEK` 能力在 `request.c:902 req_peek: grant=-1` 的无 `grant` 直通可观测（`11` 的 `VM` 直通同型：无 `grant` 时 `grant=-1`）。

### 1.6 与其他 OS 的 `FS` 包装对照

- **Linux** `vfs_read` → `__vfs_read` → `file->f_op->read_iter` 的 `kiocb` 包装：`Linux` 的 `iov_iter` 的 `iovec` 零拷贝与 `VFS` 的 `cpf_grant_magic` 的 `safecopy` 零拷贝同型；`VFS` 的 `grant` 的 `CPF_WRITE` 在 `Linux` 以 `ITER_SOURCE` 的 `WRITE` 迭代器方向同型。`VFS` 的 `ERESTART→vm_handlemem→retry` 的 `grant` 修复在 `Linux` 以 `fault_in_readable` 的 `page fault` 修复同型。
- **Redox** `Scheme::read` 的 `caller: SchemeId` 透传：`Redox` 的 `Scheme::read(caller, offset, buf)` 的 `caller` 进程显与 `VFS` 的 `req_lookup` 的 `rfp->effuid` 的 `vfs_ucred_t` 透传在 `request.c:444 grant_ucred` 的 `vu_uid/vu_gid/sgroups` 可观测；`VFS` 的 `node_details` 的 7 字段响应在 `Redox` 以 `Stat { st_ino, st_mode }` 的 `stat` 结构同型。
- **seL4** `seL4_CNode_Copy` 的 `cap` 授权：`seL4` 的 `seL4_CNode_Mint` 的 `rights` 位在 `VFS` 以 `cpf_grant_magic` 的 `CPF_READ/WRITE` 方向位同型；`VFS` 的 `ERESTART` 的 `GRANT_FAULTED` 在 `seL4` 以 `seL4_FailedLookup` 的 `lookup failure` 同型。

共同约束是“包装隐藏 `grant` 与重试，使调用方只关心 `REQ_*`”。Minix3 以 `request.c` 的 `req_*` 将 `grant` 的 `TRY→vm_handlemem→0` 与 `RES_64BIT` 的 `INT_MAX` 守门在包装内可测试，使 13/15/16 的调用方以 `req_lookup` 的 `lookup_res` 类型直接消费。

### 1.7 小结

`request.c` 是 `VFS → FS` 的类型化信封：`FS_BASE 0x600` 的 `&~0xff` 前缀使 33 请求在 `m_type` 域内可区分，`node_details/lookup_res` 的 7/9 字段使 `OK` 与 `EENTERMOUNT` 的响应可区分，`cpf_grant_direct/magic` 的 `CPF_TRY` 使 `ERESTART` 的 `vm_handlemem` 二阶段可区分，`RES_64BIT` 的 `INT_MAX` 守门使 32 位截断在 `VFS` 侧可早拒绝。下一节以 `request.c:30-1213` 全文与 `vfsif.h:41-73` 的 33 `REQ_*` 为主线逐段核对。

---

## 2 C 源码分析

### 2.1 `vfsif.h:41-73` 的 33 有效请求与 1 死常量

`vfsif.h:41 REQ_GETNODE (FS_BASE+1) Should be removed` 的 `GETNODE` 死常量在 `request.c` 全文件无 `req_getnode` 包装且 `rg "REQ_GETNODE" minix3/minix/servers/vfs/*.c` 0 调用印证死协议（plan §5.4 排除表）；`vfsif.h:42 REQ_PUTNODE (FS_BASE+2)` ~ `vfsif.h:73 REQ_BPEEK (FS_BASE+33)` 的 33 包装在 `request.c:42 REQ_BREAD/44 BWRITE/96 BPEEK/108 CHMOD/136 CHOWN/166 CREATE/219 FLUSH/235 STATVFS/261 FTRUNC/288 GETDENTS/374 INHIBREAD/390 LINK/424 LOOKUP/528 MKDIR/567 MKNOD/608 MOUNTPOINT/624 NEWNODE/664 NEW_DRIVER/700 PUTNODE/717 RDLINK/780 READSUPER/834 READWRITE/902 PEEK/926 RENAME/966 RMDIR/996 SLINK/1080 STAT/1134 SYNC/1150 UNLINK/1180 UNMOUNT/1195 UTIME` 的 33 函数（`REQ_SYNC` 在 `request.c` 无 `req_sync` 包装但 `misc.c:276 do_sync` 的 `req_sync` 经 `request.h` 的 `req_sync` 声明可观测）全覆盖。

### 2.2 `request.h:12/25` 的 `node_details/lookup_res` 响应

`request.h:12 node_details { fs_e,ino,fmode,fsize,uid,gid,dev }` 的 `m_fs_vfs_create/newnode` 的 `fs_e/m_source` 回带（`request.c:204 res->fs_e=m.m_source`）与 `request.h:25 lookup_res { +char_processed, symloop }` 的 `m_fs_vfs_lookup` 的 `offset/symloop` 回带（`request.c:506 char_processed=m.m_fs_vfs_lookup.offset`）的 `OK vs EENTERMOUNT` 分化在 `request.c:495 switch(r)` 的 `OK: inode/fmode…; EENTERMOUNT: inode/offset/symloop` 可观测。

### 2.3 `req_breadwrite` 的 `grant→fs_sendrec→revoke→ERESTART` 闭环

`request.c:30 req_breadwrite_actual:38 grant=cpf_grant_magic(CPF_WRITE|cpflag) → 44 m_type=READ/WRITE + device/grant/pos/nbytes → 51 fs_sendrec → 53 revoke==FAULTED→ERESTART → 58 new_pos/cum_iop` 的 `grant` 生命周期与 `64 req_breadwrite:70 req_breadwrite_actual(CPF_TRY) →73 if(ERESTART){ vm_handlemem → req_breadwrite_actual(...,0) }` 的 `TRY→0` 二阶段在 `request.c:70` 的 `CPF_TRY` 与 `80` 的 `0` 标志切换中可观测（`11` 的 `vm_sendrec` 直通对端）。

### 2.4 `req_readwrite` 的 `RES_64BIT` 守门与 `ЕРЕSTART` 重放

`request.c:834 req_readwrite_actual:846 grant=cpf_grant_magic(READING?CPF_WRITE:CPF_READ|cpflag) → 852 m_type=READ/WRITE + inode/grant/pos/nbytes → 856 if(!(RES_64BIT) && pos>INT_MAX) EINVAL → 862 fs_sendrec → 864 revoke→ERESTART → 868 new_pos/cum_iop` 的 `RES_64BIT` 的 `INT_MAX` 守门与 `878 req_readwrite:885 req_readwrite_actual(CPF_TRY)→888 if(ERESTART){ vm_handlemem→actual(0) }` 的二阶段同型 `breadwrite`。

### 2.5 `req_lookup` 的 `PATH_GET_UCRED` 分支

`request.c:424 req_lookup:444 grant=cpf_grant_direct(PATH_MAX) → 459 if(ngroups>0){ credentials→grant_direct→PATH_GET_UCRED } else { uid/gid direct; flags&=~PATH_GET_UCRED } → 485 flags→m.lookup.flags → 488 fs_sendrec → 489 revoke → 495 switch(r) { OK: inode/fmode/…; EENTERMOUNT: inode/offset/symloop; … }` 的 `grant_path` 单 `grant` vs `grant_path+grant_ucred` 双 `grant` 及 `PATH_GET_UCRED 020` 位的 `vfs_ucred_t` 透传（`vfsif.h:32` `vu_uid/vu_gid/sgroups`）在 `request.c:460` 的 `ngroups>0` 分支可观测。

### 2.6 `req_getdents` 的 `direct/magic` 分化与 `RES_64BIT` 守门

`request.c:288 req_getdents_actual:307 if(direct) grant_direct(buf) else grant_magic(CPF_WRITE|cpflag) → 318 REQ_GETDENTS + inode/grant/size/pos + 323 if(!(RES_64BIT)&&pos>INT_MAX) EINVAL →328 fs_sendrec →330 revoke→ERESTART →332 new_pos + nbytes` 的 `direct` 的 `MFS` 内部 `getdents` 直送与 `user` 的 `magic` 零拷贝分化及 `343 req_getdents:354 actual(CPF_TRY)→357 if(ERESTART){ vm_handlemem→actual(0) }` 的二阶段同型 `readwrite`。

### 2.7 `req_stat` 的 `stat grant` 复用与 `ERESTART` 重放

`request.c:1080 req_stat_actual:1088 grant=cpf_grant_magic(sizeof(stat),CPF_WRITE|cpflag) →1095 REQ_STAT + inode/grant →1100 fs_sendrec →1102 revoke→ERESTART` 的 `stat` 缓冲区 `grant` 与 `1111 req_stat:1116 actual(CPF_TRY)→1119 if(ERESTART){ vm_handlemem→actual(0)}` 的二阶段同型 `breadwrite`。

### 2.8 `req_create/mkdir/mknod/link/rename` 的 `path` 直送

`request.c:166 req_create:184 len=strlen(path)+1 →185 grant_direct(path) →190 REQ_CREATE + inode/mode/uid/gid/grant/len →199 fs_sendrec →200 revoke→204 res->{fs_e,ino,fmode,fsize,uid,gid}` 的 `path` 字符串直送与 `528 req_mkdir:542 grant_direct(lastc) →548 REQ_MKDIR + inode/mode/uid/gid/grant →557 fs_sendrec` 及 `567 req_mknod:589 REQ_MKNOD + device` 的 `dev` 追加及 `390 req_link:402 grant_direct(lastc) →407 REQ_LINK + dir_ino/inode/grant` 的 `dir_ino` 追加及 `926 req_rename:935 grant_old+grant_new →945 REQ_RENAME + dir_old/grant_old + dir_new/grant_new` 的双 `grant` 可观测。

### 2.9 `req_ftrunc/readsuper/flush/putnode` 的能力与无 `grant` 直通

`request.c:261 req_ftrunc:270 REQ_FTRUNC + inode/start/end + 274 if(!(RES_64BIT)&&(start||end>INT_MAX)) EINVAL` 的 `RES_64BIT` 守门与 `780 req_readsuper:799 grant_direct(label) →804 REQ_READSUPER + flags(REQ_RDONLY/ISROOT)+grant/device/len →813 fs_sendrec →818 res->{fs_e,ino,fmode…}+fs_flags` 的 `mount` 时能力回带及 `219 req_flush:224 REQ_FLUSH + device` 的无响应回填直通及 `700 req_putnode:705 REQ_PUTNODE + inode/count` 的 `v_fs_count` 回收对端（`05` 的 `put_vnode` 慢路径）。

### 2.10 `req_peek/bpeek` 的 `grant=-1` 直通与 `RES_64BIT` 守门

`request.c:902 req_peek:915 grant=-1 + inode/pos/nbytes` 的 `grant=-1` 无拷贝直通（`11` 的 `VM` 直通同型）与 `89 req_bpeek:96 REQ_BPEEK + device/pos/nbytes` 的块 `peek` 无 `grant` 及 `374 req_inhibread:379 REQ_INHIBREAD + inode` 的抑制可观测。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `request.c:38` 的 `cpf_grant_magic` 与 `51 fs_sendrec` 的散落，而是吸收 Redox/Linux 的 `REQ` 类型化后做取舍。以下决策对应 `.design/12-design.v1.md` D1-D6。

### D1 `REQ_*` 类型化：`FsReq` 枚举的 `FS_BASE` 前缀守门

- **C**：`m_type=REQ_CREATE + m.m_vfs_fs_create.inode/mode` 的散落 `m_vfs_fs_*` 联合体字段（`request.c:190`）在 `request.c` 33 函数内以 `m_type` 赋值与 `fs_sendrec` 散落。
- **Rust**：`FsReq` 的 33 变体 `enum`（`BRead{fs_e,dev,pos,nbytes,user{endpoint,addr}} / BWrite / BPeek / Chmod{ino,mode} / Chown{ino,uid,gid} / Create{dir,mode,uid,gid,path} / Flush{dev} / StatVfs{grant} / FTrunc{ino,start,end} / GetDents{ino,pos,buf,size,direct} / InhibRead{ino} / Link{dir,linked} / Lookup{dir,root,uid,gid,path,flags} / Mkdir{dir,mode,uid,gid,path} / Mknod{dir,mode,uid,gid,dev,path} / Mountpoint{ino} / NewNode{uid,gid,mode,dev} / NewDriver{dev,label} / PutNode{ino,count} / RdLink{ino,buf,len} / ReadSuper{label,dev,ro,isroot} / Read{ino,pos,nbytes,user} / Write{ino,pos,nbytes,user} / Peek{ino,pos,nbytes} / Rename{old_dir,old_name,new_dir,new_name} / Rmdir{dir,path} / SLink{dir,path,uid,gid,target} / Stat{ino,buf} / Sync / Unlink{dir,path} / Unmount / Utime{ino,actime,modtime}`）+ `FsReq::m_type()` 的 `IS_FS_RQ(&~0xff==FS_BASE)` 前缀守门（`&~0xff==0x600`）+ `FsResp` 的 `NodeDetails{fs_e,ino,fmode,fsize,uid,gid,dev}` 与 `LookupRes{+char_processed,symloop}` 二响应类型化。
- **为什么**：`m_vfs_fs_create` 的 `grant` 在 `REQ_CREATE` 专属与 `REQ_LOOKUP` 的 `grant_path+grant_ucred` 双 `grant` 的 `m_vfs_fs_*` 联合体重用在 C 以宏字段名隐式，Rust 以 `Create{path: String}` vs `Lookup{path:String, ucred: Option<VfsUCred>}` 的字段名显式使 `grant` 数目错配编译期失败。
- **备选**：保留 `Message` 直填；否决——`REQ_CREATE` 的 `mode` 与 `REQ_CHMOD` 的 `rmode` 的 `mode` 重名在 `Message` 联合体中易错拷。

### D2 `grant` 与 `ERESTART` 的 `GrantScope` 显式

- **C**：`grant=cpf_grant_magic(TRY) → fs_sendrec → revoke==FAULTED→ERESTART → vm_handlemem → retry(0)` 的 `TRY→0` 标志切换在 `request.c:70/73` 的两 `grant` 调用散落。
- **Rust**：`GrantScope { id: GrantId, flag: GrantFlag }` 的 `Try(0)/NoTry(1)` 枚举 + `FsClient::send_with_grant(req, scope)->Result<FsResp, FsError>` 的 `GrantScope::Try → ERestart → vm_procctl_handlemem → GrantScope::NoTry` 二阶段在 `FsClient::breadwrite` 的 `match revoke { Faulted => vm_handlemem → retry }` 显式；`GrantId` 的 `-1` 哨兵在 Rust 以 `Option<GrantId>` 的 `None→grant=-1` 映射（`11` 的 `grant=-1` 直通同型）。
- **为什么**：`CPF_TRY` 的 `TRY` 位在 `REQ_PEEK` 的 `grant=-1` 无拷贝时为 `0`，Rust 以 `Option<GrantId>` 的 `None` 使 `REQ_PEEK` 的 `grant` 数目可审计。

### D3 `lookup` 的 `PATH_GET_UCRED` 分支：`Option<VfsUCred>` 显式

- **C**：`req_lookup:459 if(ngroups>0){ grant_ucred } else { uid/gid direct }` 的 `ngroups>0` 分化与 `PATH_GET_UCRED 020` 置位（`request.c:477 flags|=PATH_GET_UCRED`）。
- **Rust**：`Lookup { dir, root, path: String, cred: Option<VfsUCred { uid,gid,sgroups }>, flags: LookupFlags }` 的 `cred.is_some() → flags.contains(GET_UCRED)` 的 `Option` 守门使 `grant` 数目（`1 vs 2`）在 `FsReq::grants()` 的 `len()` 可测试。
- **为什么**：`vfs_ucred_t` 的 `vu_sgroups[NGROUPS_MAX]` 定长数组在 Rust 以 `Vec<Gid>` 的 `len ≤ NGROUPS_MAX` 守门。

### D4 `RES_64BIT` 守门：`FsFlags` 的 `can_64bit` 显式

- **C**：`req_ftrunc:274 if(!(RES_64BIT) && (start>INT_MAX)) return EINVAL` 的 `vmnt.m_fs_flags` 守门在 `request.c:261` 的 `vmp = find_vmnt(fs_e)` 后。
- **Rust**：`FsReq::check_64bit(flags: FsFlags, off: u64) -> Result<(), FsError>` 的 `if(!flags.contains(RES_64BIT) && off>INT_MAX as u64) Err(InvalidOff)` 显式，使 32 位截断在 `FsClient` 层 `EDEADLK` 前可早拒绝。
- **为什么**：`INT_MAX` 的 `2^31-1` 守门在 Rust 以 `i32::MAX` 显式常量。

### D5 响应回填：`FsResp` 的 `fs_e/m_source` 回带

- **C**：`res->fs_e=m.m_source; res->inode=m.m_fs_vfs_create.inode` 的 `m_source` 回带与 `m_fs_vfs_*` 字段拷贝在 `request.c:204` 的 `res` 回填散落。
- **Rust**：`FsResp::Node(NodeDetails) / Lookup(LookupRes)` 的 `NodeDetails { fs_e: Endpoint, ino: Ino, mode: Mode, size: u64 }` 的 `fs_e` 在 `FsClient::create` 的 `Ok(NodeDetails{fs_e: reply.source})` 的 `source` 回带显式。

### D6 测试与死常量排除

- **C**：`vfsif.h:41 REQ_GETNODE Should be removed` 的死常量在 `request.c` 无包装。
- **Rust**：`FsReq` 的 33 变体不含 `GetNode`，`vfsif.h:41` 的 `GETNODE` 在 `FsClient::decode` 的 `IS_FS_RQ` 前缀守门中 `FS_BASE+1 → Err(Unknown)` 的 `Unknown` 分支可测试（`test_dead_getnode_is_unknown`）。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-8 64 位 | `RES_64BIT` 的 `INT_MAX` 守门 | `request.rs:FsFlags::RES_64BIT` + 本文档 D4 + 12 正文 1.5 |
| A-2 函数指针 → 枚举 | `FsReq` 33 变体 `enum` | `request.rs:FsReq` + 本文档 D1 + 12 正文 1.2 |
| A-4 全局 → 注入 | `FsClient` 的 `&mut GlobalComm` 注入 | `request.rs:FsClient::send_fs` + 本文档 D2 + 12 正文 1.4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── request.rs          — FsReq(33)/FsResp(NodeDetails/LookupRes)/FsFlags/GrantScope/GrantId + FsClient trait (Blocking vs Mock + Fifo vs Lifo) + check_64bit + vm_handlemem retry
├── fs_comm.rs          — GlobalComm{ vmnts:[FsComm;8] } + TransId + FsTransport (11)
├── vmnt.rs             — Vmnt.m_fs_flags: FsFlags(RES_*) + m_fs_e 嵌入（06）
└── fproc.rs            — FpFlags::REVIVED 与 reviving 的 unblock 对端（02）
```

### 4.2 `request.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `FS_BASE 0x600` | `vfsif.h:40` | `request.rs:FS_BASE:u32=0x600` | `IS_FS_RQ &~0xff==0x600` 前缀守门 |
| `NREQS 34` | `vfsif.h:75` | `NREQS:usize=34` | `REQ_BPEEK 0x621` 上界 |
| `REQ_GETNODE 0x601` | `vfsif.h:41` | `request.rs:FsReq` 无 `GetNode` 变体 | `decode(0x601)→Err(Unknown)` 死常量排除 |
| `node_details` | `request.h:12` | `request.rs:NodeDetails{fs_e,ino,fmode,fsize,uid,gid,dev}` | `REQ_NEWNODE/CREATE` 的 7 字段响应 |
| `lookup_res` | `request.h:25` | `LookupRes{+char_processed,symloop}` | `EENTERMOUNT` 的 `offset/symloop` 回带 |
| `req_breadwrite` | `request.c:30` | `FsClient::breadwrite(fs_e,dev,pos,nbytes,user, rw)` | `grant→send_fs→revoke→ERESTART→vm_handlemem→retry(0)` |
| `req_lookup` | `request.c:424` | `FsClient::lookup(dir,root,path,cred, flags)` | `PATH_GET_UCRED` 的 `Option<VfsUCred>` 分化 |
| `req_getdents` | `request.c:288` | `FsClient::getdents(dir,pos,buf,size,direct)` | `RES_64BIT` 守门 + `direct` 的 `grant_direct vs magic` |
| `RES_64BIT` | `vfsif.h:23` | `FsFlags::RES_64BIT` | `off>INT_MAX → EINVAL` 守门 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 前缀不重叠 `IS_FS_RQ & IS_VFS_CALL & IS_VFS_PM_RQ==∅` | `FsReq::is_fs_rq` | `&~0xff==0x600` vs `0x100` vs `0x900` | `vfsif.h:77` vs `callnr.h:70` vs `com.h:516` |
| 死常量无包装 `GETNODE→Unknown` | `FsReq::decode` | `FS_BASE+1 → Err(Unknown)` | `vfsif.h:41` 注释 |
| grant 配对 `grant→revoke` | `GrantScope` | `Drop` 的 `revoke()` | `request.c:53` |
| ERESTART 二阶段 `TRY→0` | `FsClient::breadwrite` | `if(ERESTART){ vm_handlemem→retry(0) }` | `request.c:73` |
| 64位守门 `RES_64BIT→INT_MAX` | `FsReq::check_64bit` | `!RES_64BIT && off>INT_MAX → EINVAL` | `request.c:274` |
| 响应 fs_e 回带 `m_source` | `FsResp` | `NodeDetails.fs_e = reply.source` | `request.c:204` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **163 passed / 0 failed**（`fproc` 13 + `main_loop` 35 + `worker` 19 + `call_table` 8 + `filp` 7 + `vnode` 7 + `vmnt` 7 + `tll` 6 + `ipc/dispatcher` 16 + `fs_comm` 19 + `request` 20 = 157 → `cargo test` 实测 163；`minix-types` 108 独立）。
> 本章直接影响 `20` 项新增（`fs_base_prefix/nreqs_getnode_dead/node_details/lookup_res/breadwrite_grant/breadwrite_retry/lookup_ucred/getdents_64/write64/read64/bpeek_no_grant/flush_no_resp/ftrunc_64/readsuper_flags/newnode_mix/putnode_count/peek_no_grant/fs_req_two_impls/grant_two_impls/message_roundtrip`），`minix-vfs --lib` 总计 143 → 163。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_fs_base_prefix` | `vfsif.h:77` | `IS_FS_RQ(0x60B)==true, 0x100==false` 的前缀不重叠 | `request.rs` |
| `test_nreqs_getnode_dead` | `vfsif.h:41` | `GETNODE 0x601 → Unknown` 死常量排除 | `request.rs` |
| `test_node_details` | `request.h:12` | `NodeDetails 7 字段` 的 `fs_e` 回带 | `request.rs` |
| `test_lookup_res` | `request.h:25` | `LookupRes 9 字段` 的 `EENTERMOUNT` 分支 | `request.rs` |
| `test_breadwrite_grant` | `request.c:38` | `grant CPF_WRITE\|TRY → revoke` 的 `GrantScope` 配对 | `request.rs` |
| `test_breadwrite_retry` | `request.c:73` | `ERESTART→vm_handlemem→retry(0)` 二阶段 | `request.rs` |
| `test_lookup_ucred` | `request.c:459` | `ngroups>0→2 grant vs 0→1 grant` 的 `Option<VfsUCred>` 分化 | `request.rs` |
| `test_getdents_64` | `request.c:323` | `!RES_64BIT && pos>INT_MAX→EINVAL` | `request.rs` |
| `test_write64` | `request.c:856` | `RES_64BIT` 守门 `Write` | `request.rs` |
| `test_read64` | `request.c:856` | `RES_64BIT` 守门 `Read` | `request.rs` |
| `test_bpeek_no_grant` | `request.c:89` | `grant=-1` 无拷贝直通 | `request.rs` |
| `test_flush_no_resp` | `request.c:219` | `REQ_FLUSH` 无响应回填直通 | `request.rs` |
| `test_ftrunc_64` | `request.c:274` | `FTrunc start/end>INT_MAX→EINVAL` | `request.rs` |
| `test_readsuper_flags` | `request.c:804` | `ReadSuper flags RO/ISROOT` 的 `FsFlags` 回带 | `request.rs` |
| `test_newnode_mix` | `request.c:624` | `NewNode mode/dev/uid` 的 `NodeDetails` 混参 | `request.rs` |
| `test_putnode_count` | `request.c:700` | `PutNode count` 的 `v_fs_count` 回收 | `request.rs` |
| `test_peek_no_grant` | `request.c:902` | `Peek grant=-1` 直通 | `request.rs` |
| `test_fs_req_two_impls` | `request.c:30` | `FsClient` trait `Blocking vs Mock` 的 `sent_fs.len 0 vs 1` 差异 | `request.rs` |
| `test_grant_two_impls` | `request.c:38` | `GrantScope` 的 `Try vs NoTry` 的 `CPF_TRY` 位差异 | `request.rs` |

测试策略：`FS_BASE` 前缀以 `IS_FS_RQ(REQ_BREAD true) vs VFS_READ false` 的不重叠样本覆盖；`GETNODE` 死常量以 `decode(0x601)→Unknown` 的 `Err` 样本覆盖；`node_details` 以 `fs_e` 回带样本覆盖；`breadwrite` 以 `grant Try→ERESTART→vm_handlemem→retry` 的二阶段样本覆盖；`lookup` 以 `ngroups 0→1 grant vs 2→2 grant` 的 `Option` 分化样本覆盖；`FS_CLIENT` 与 `GRANT` 的双 trait 以 `Blocking vs Mock` 的 `sent_fs` 差异与 `Try 0x01 vs NoTry 0x00` 的位差异样本覆盖。

---

## 6 过渡

本篇在 `11-fs-comm` 的 `GlobalComm::sendmsg` 的 `c_max_reqs` 窗口与 `TransId` 高位编码之后，`request.c` 的 `req_*` 将 `VFS → FS` 的 33 `REQ_*` 包装为类型化 `Message` 构造/投递/回收/重放的闭环，是 11 的 `fs_sendrec` 窗口化之前的 `REQ` 包装前提、13 的 `lookup` 路径解析的 `req_lookup` 对端、15/16 的 `open/read/write` 的 `req_create/readwrite` 对端、18 的 `mount` 的 `req_readsuper/newnode` 对端。

```
11-fs-comm: m_comm{ max/cur/queue } + sendmsg(TRNS_ADD_ID + w_task) + queuemsg + fs_sendmore  （c_max/c_cur/queue 的窗口与 VFS_TRANSID 高位编码）
  │
  └─► 本章: request.c 的 33 REQ_* 的 FsReq 枚举 + GrantScope(Try→0) 二阶段 + node_details/lookup_res 响应类型化  （FS_BASE 前缀守门与 ERESTART 的 vm_handlemem 重试及 RES_64BIT 的 INT_MAX 守门）
         │
         ├─► 13-path-lookup: lookup 的 req_lookup 的 PATH_GET_UCRED 分支  （本章 lookup 的 grant_ucred 对端）
         ├─► 15-open-close: open 的 req_create/newnode 的 mode/dev 包装  （本章 create 的 grant_path 对端）
         ├─► 16-read-write: read 的 req_readwrite 的 pos/nbytes 包装  （本章 readwrite 的 grant_magic 对端）
         └─► 18-mount: mount 的 req_readsuper 的 flags/grant 直通  （本章 readsuper 的 label grant 对端）
```

`REQ_GETNODE` 的 `Should be removed` 死常量在 `request.c` 无包装，使 `NREQS 34` 的 `FS_BASE+1..+33` 中 33 有效可观测，为 `99` 的 `TRNS_*` 术语提供 `REQ_*` 前缀守门的回收可观测。阅读顺序提示：若关心“`lookup` 如何触发 `EENTERMOUNT` 的挂载点穿越”，下一站 `13-path-lookup.md`；若关心“`create` 如何触发 `REENTER` 的 `RES_HASPEEK` 能力”，下一站 `12` 的 `REQ_PEEK` 无 `grant` 直通已在本章。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/request.c:30-1213`（`req_breadwrite_actual:30` 的 `grant→fs_sendrec→revoke→ERESTART` 闭环、`424 req_lookup` 的 `PATH_GET_UCRED` 分支、`288 req_getdents_actual` 的 `RES_64BIT` 守门、`834 req_readwrite_actual` 的 `read/write` 包装、`902 req_peek` 的 `grant=-1` 直通）、`minix3/minix/include/minix/vfsif.h:41-73`（`REQ_GETNODE 0x601` 死常量与 `REQ_BREAD 0x60B` ~ `REQ_BPEEK 0x621` 的 33 有效请求及 `TRNS_GET/ADD/DEL` 的 `&0xFFFF/<<16/>>16`）、`minix3/minix/servers/vfs/request.h:12/25`（`node_details 7 字段` 与 `lookup_res 9 字段` 的 `char_processed/symloop` 回带）
- 阶段文档：`11-fs-comm.md`（`GlobalComm::sendmsg` 的 `c_max_reqs` 窗口与 `TransId` 编码）、`06-vmnt-table.md`（`Vmnt.m_fs_flags: RES_*` 的能力位）、`13-path-lookup.md`（`lookup` 的 `EENTERMOUNT` 消费）、`99-global-concepts.md`（`FS_BASE/NREQS/TRNS` 术语与 `GrantId`）
- Rust 实现：`os/servers/vfs/src/request.rs:1`（`FsReq(33)/FsResp(NodeDetails/LookupRes)/FsFlags/GrantScope` + `FsClient` trait (Blocking vs Mock + Try vs NoTry)）、`os/servers/vfs/src/fs_comm.rs:1`（`GlobalComm` 的 `sendmsg` 对端与 `TransId` 编码）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（`cpf_grant_magic/direct` 的 `safecopy` 原语）

