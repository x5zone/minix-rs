# 13 — 路径解析：`lookup` 的 `EENTERMOUNT` 穿越与 `SYMLOOP=16` 的 `last_dir` 循环及 `DO_POSIX_PATHNAME_RES 0`

本文讲清 VFS 如何在 `PATH_MAX 1024` 的路径上界、`NAME_MAX 60` 的单分量上界、`SYMLOOP 16` 的符号链接循环阈值、`DO_POSIX_PATHNAME_RES 0` 的尾斜杠忽略的历史行为、`PATH_GET_UCRED 020` 的 `vfs_ucred_t` 透传、`PATH_RET_SYMLINK 010` 的不跟随、`l_vmnt_lock/l_vnode_lock` 的 `TLL_NONE→READ/WRITE` 锁请求、`l_path` 的 `char[PATH_MAX]` 可变缓冲区、`lookup_res` 的 `EENTERMOUNT/ELEAVEMOUNT/ESYMLINK -301..-303` 三特殊码与 `char_processed/symloop` 回带的约束下，以 `lookup_init` 的 `l_path/l_flags→NULL vmnt/vnode` 初始化与 `advance` 的 `get_free_vnode→lookup→find_vnode/dup_vnode→downgrade` 两相（命中扣 `v_fs_count++` vs 未命中建表项）及 `eat_path` 的 `/→rd vs 非/→wd` 起点分化及 `last_dir` 的 `/` 切分 `dir_entry` 与 `advance` 循环的 `symlink→rdlink→relative vs absolute→loop_start` 分化与 `lookup` 的 `req_lookup→EENTER/ELEAVE/SYMLINK→memmove→symloop++→ELOOP` 循环的 `VMNT_READ→WRITE` 锁升级建立 `Path → Vnode` 的挂载可观测解析，并以 `fetch_name` 的 `sys_safecopyfrom` 与 `copy_path` 的 `FETCH_NAME_MAX` 截断及 `canonical_path` 的 `last_dir→get_name→..` 爬升为符号链接与挂载点穿越提供可观测路径。

前置阅读：`05-vnode-table.md`（`Vnode{ v_fs_e, ino, mode, fs_count }` 的 `get_free/find/dup/put` 与 `VNODE_READ/WRITE/OPCL`）、`06-vmnt-table.md`（`Vmnt{m_fs_e,m_mounted_on,m_root_node}` 的 `find_vmnt` 与 `VMNT_READ/WRITE/EXCL`）、`07-tll-lock.md`（`TLL_NONE/READ/READSER/WRITE` 三态与 `downgrade`）、`11-fs-comm.md`（`fs_sendrec` 的 `c_max_reqs` 窗口与 `TransId` 编码）、`12-request-wrappers.md`（`FsReq::Lookup` 的 `grant_path+grant_ucred` 双 `grant`）。

> 本章不讲什么：
> - `req_lookup` 的 `FS` 端 `grant_path` 构造与 `vfs_ucred` 透传细节—— `12-request-wrappers.md`（`REQ_LOOKUP` 包装，本章只讲 `lookup` 的 `req_lookup` 调用点与三特殊码分化）
> - `vnode` 的 `get_free/find` 的 `tll` 锁等待与 `filp` 的 `get_filp` —— `05`/`04`（`advance` 的 `get_free→lock` 对端）
> - `vmnt` 的 `mark_free/clear` 与 `unmap_by_endpt` —— `06`（`lookup` 的 `EENTER` 穿越对端）
> - `open` 的 `O_CREAT` 与 `common_open` 的 `new_node` —— `15-open-close.md`（`last_dir` 的调用方）
> - `exec` 的 `is_script/patch_stack` 脚本解释—— `25-exec.md`（`lookup` 的调用方之一，但脚本路径的 `PATH_RET_SYMLINK` 分化不在本章）
> - 内核 `sys_safecopyfrom` 的跨地址空间拷贝—— `99-global-concepts.md` + `../01-stage-kernel/18-syscall-copy.md`

---

## 1 概念

### 1.0 章节引言

**目标读者**：已理解 06 的 `Vmnt{m_mounted_on,m_root_node}` 的挂载边界与 05 的 `Vnode{fs_e,ino}` 的 `find_vnode(fs_e+ino)` 命中及 07 的 `VNODE_READ→OPCL→READ` 降级，能 `grep "advance\|eat_path\|last_dir\|lookup" minix3/minix/servers/vfs/path.c` 的开发者。

### 1.1 为什么路径解析是“所有名字类调用的前置”

`open/read/write` 的 `fd` 操作、`link/unlink/rename` 的目录项操作、`chdir/stat` 的元数据操作、`mount` 的 `m_mounted_on` 绑定、`socket` 的 `do_socketpath` 创建——**全部以路径名作为入口**。`lookup` 将 `"/usr/bin/sh"` 的字符串翻译为 `vnode` 的 `fs_e+ino` 对，是 `14~31` 的 18 个系统调用的**前置依赖**（plan §3.2 的“每篇回答它在 `sef_cb_init_fresh / handle_work` 的哪个位置”使 `13` 的 `eat_path` 在 `14` 的 `get_fd` 之前可观测）。

此“前置”与 Redox 的 `Scheme::open(path)` 的 `resolve_path→lookup` 前置及 Linux 的 `path_openat` 的 `link_path_walk` 前置同型：VFS 的 `advance` 的 `get_free_vnode→lookup→find_vnode` 两相在 `Redox` 以 `SchemeId::resolve(path)→Arc<Vnode>` 的 `find` 命中可观测。

### 1.2 挂载点穿越：`EENTERMOUNT` 的 `char_processed` 回带

`lookup` 的 `while(r==EENTERMOUNT||ELEAVEMOUNT||ESYMLINK)` 循环使挂载点穿越在 `req_lookup` 的 `EENTERMOUNT -301` 回码中可观测：`FS` 的 `lookup` 发现路径的下一分量恰为挂载点目录的 `ino`，返回 `EENTERMOUNT` 并在 `lookup_res.inode_nr` 回带被挂载目录的 `ino`，`char_processed` 回带已解析字符数，`symloop` 回带本次 `symlink` 计数。`lookup:478` 的 `for(vmp: m_mounted_on→inode==res.inode && fs_e==res.fs_e)` 的线性扫描在 `06` 的 `vmnt[8]` 小固定数组可观测，使 `dir_vp = m_root_node` 的穿越在 `VFS` 侧 `char_processed` 的 `memmove(path, path+off)` 后 `EENTERMOUNT` 循环的下一轮 `req_lookup(fs_e=dir_vp→fs_e)` 可观测。

此“`char_processed` 的 `memmove` 回带”与 Linux 的 `link_path_walk` 的 `nd->last.name` 游标及 Redox 的 `next_component(path_off)` 游标同型：`VFS` 的 `memmove(l_path, l_path+off)` 的 `off` 游标在 `Redox` 以 `path[consumed..]` 切片可观测。

### 1.3 符号链接循环：`SYMLOOP 16` 的 `symloop` 计数

`path.c:31 DO_POSIX_PATHNAME_RES 0` 的 `0` 使 `last_dir:190 while(len>1 && path[len-1]=='/') len--` 的尾斜杠忽略在历史 Unix 行为可观测，`POSIX` 的 `1` 将使 `mkdir("dir/",…)` 因 `"."` 创建失败（`path.c:23` 注释）。`SYMLOOP 16`（`const.h:32`）的 `_POSIX_SYMLOOP_MAX 16` 阈值在 `lookup:468 if(symloop>16) return ELOOP` 与 `last_dir:349 while(symloop<_POSIX_SYMLOOP_MAX)` 的双守门中可观测，`ELOOP -40` 的 `symloop` 溢出在 `last_dir:351 err_code=ELOOP` 可观测。

此“`16` 的 `ELOOP` 溢出”与 Linux 的 `MAX_SYMLINKS 40` 及 Redox 的 `symlink_depth 8` 的阈值可对照，但 VFS 以 `16` 的小阈值在 `last_dir:288 strrchr(path,'/')` 的 `symloop++` 与 `lookup:467 symloop+=res.symloop` 的 `FS` 侧计数累加中可观测。

### 1.4 锁的升级：`VMNT_WRITE→READ` 的降级与 `VNODE_READ→OPCL`

`lookup:435 if(VMNT_READ) mnt_lock_type=VMNT_WRITE else mnt_lock_type=l_vmnt_lock` 的 `READ→WRITE` 升级使 `vmnt` 的 `c_max_reqs` 窗口在 `lookup` 内可 `vmnt->m_fs_e` 的 `FS` 端并发可观测，`554 if(VMNT_WRITE != mnt_lock_type) downgrade_vmnt_lock` 的 `WRITE→READ` 降级在 `lookup` 结束的可观测性使 `05/06` 的 `tll_downgrade` 在 `lookup` 的 `VMNT` 锁生命周期可观测。`advance:54 if(VNODE_READ) initial=OPCL else initial=l_vnode_lock` 的 `READ→OPCL` 初始锁类型使 `vnode` 的 `v_lock` 在 `advance:61 lock_vnode(new_vp, OPCL)` 的 `OPCL` 阶段与 `118 if(initial != l_vnode_lock) tll_downgrade` 的 `OPCL→READ` 降级在 `05` 的 `v_lock` 可观测。

### 1.5 响应的类型化：`OK` vs `EENTERMOUNT` 的 `lookup_res` 分化

`request.h:25 lookup_res { fs_e,ino,fmode,fsize,uid,gid,dev, char_processed, symloop }` 的 `char_processed/symloop` 二字段仅在 `EENTERMOUNT/ELEAVEMOUNT/ESYMLINK` 时有效，`OK` 时 `char_processed` 无意义。`request.c:495 switch(r){ case OK: inode/fmode…; case EENTERMOUNT: inode/offset/symloop; … }` 的 `lookup_res` 回填分化在 `12` 的 `FsReq::Lookup` 的 `LookupRes` 枚举 `Ok(LookupOk{ino,mode…}) vs EnterMount{ino,offset,symloop}` 的 `match` 穷尽可观测。

### 1.6 与其他 OS 的路径解析对照

- **Linux** `link_path_walk`：`Linux` 的 `nameidata { path, last, inode, flags }` 的 `nd->path` 游标与 `VFS` 的 `lookup.l_path` 的 `memmove` 游标同型；`VFS` 的 `EENTERMOUNT` 的 `char_processed` 回带在 `Linux` 以 `nd->last.name` 的 `hash_len` 回带同型；`VFS` 的 `SYMLOOP 16` 在 `Linux` 以 `MAX_SYMLINKS 40` 的 `nd->depth++` 同型。
- **Redox** `Scheme::resolve`：`Redox` 的 `SchemeId::resolve(path) -> Result<INode, Error>` 的 `Scheme` 边界在 `VFS` 以 `vmnt.m_mounted_on→m_root_node` 的 `dir_vp = m_root_node` 穿越同型；`VFS` 的 `copy_path/fetch_name` 的 `sys_safecopyfrom` 在 `Redox` 以 `UserSlice::copy_from_user` 的 `Result` 守门同型。
- **seL4** `seL4_CNode_Copy` 的 `cap` 派生：`seL4` 的 `seL4_CNode_Lookup` 的 `cap` 路径解析在 `VFS` 以 `lookup` 的 `vnode` 路径解析同型；`VFS` 的 `DO_POSIX_PATHNAME_RES 0` 的尾斜杠忽略在 `seL4` 以 `seL4_CapNull` 的空能力显式同型。

共同约束是“路径解析的挂载可观测”。Minix3 以 `EENTERMOUNT` 的 `char_processed` 回带与 `SYMLOOP` 的 `symloop` 计数使 `last_dir` 的符号链接循环不影响 `vmnt` 的小固定数组线性扫描的可观测性。

### 1.7 小结

路径解析是 `Path → Vnode` 的挂载可观测翻译：`PATH_MAX` 的路径上界与 `NAME_MAX` 的分量上界及 `SYMLOOP` 的 `ELOOP` 阈值与 `DO_POSIX` 的尾斜杠历史行为在 `lookup_init` 的 `NULL vmnt/vnode` 初始化可观测，`advance` 的 `get_free→lookup→find/dup→downgrade` 两相在 `VFS -A-10` 的 `DO_POSIX 0` 可观测，`lookup` 的 `EENTER/ELEAVE/SYMLINK` 循环在 `char_processed` 的 `memmove` 回带可观测，`fetch_name/copy_path` 的 `safecopy` 在 `99` 的 `sys_datacopy_wrapper` 可观测。下一节以 `path.c:40-933` 全文与 `path.h:4` 的 `lookup` 结构为主线逐段核对。

---

## 2 C 源码分析

### 2.1 `path.h:4` 的 `lookup` 结构

`path.h:4 struct lookup { char *l_path; int l_flags; tll_access_t l_vmnt_lock; l_vnode_lock; vmnt **l_vmp; vnode **l_vnode; }` 的 `l_path` 可变缓冲区（`char[PATH_MAX]` 的 `resolve->l_path` 传入者在 `open.c:common_open` 的 `user_fullpath` 可观测）与 `l_flags` 的 `PATH_RET_SYMLINK 010` / `PATH_GET_UCRED 020` 位（`vfsif.h:12`）及 `l_vmnt/l_vnode_lock` 的 `TLL_NONE→READ/WRITE` 锁请求及 `l_vmp/l_vnode` 的 `vmnt*/vnode*` 输出指针在 `lookup_init:574 l_path=path; l_flags=flags; l_vmp=vmp; l_vnode=vp; *vmp=NULL; *vp=NULL` 的 `NULL` 初始化可观测。

### 2.2 `utility.c:24-93` 的 `copy_path/fetch_name`

`utility.c:24 copy_path(src, len, dst)` 的 `len>PATH_MAX → ENAMETOOLONG` 守门与 `60 fetch_name(len, addr, buf)` 的 `len>PATH_MAX || len==0 → EINVAL` 守门及 `72 sys_safecopyfrom(who_e, addr, SELF, buf, len)` 的跨地址空间拷贝在 `99` 的 `sys_datacopy_wrapper` 可观测，`13` 的 `canonical_path` 调用 `copy_path` 与 `14` 的 `filedes` 调用 `fetch_name` 的分化在 `open.c` 的 `common_open` 可观测。

### 2.3 `advance:40-127` 的 `get_free→lookup→find/dup→downgrade` 两相

`advance:54 initial=VNODE_READ?OPCL:lock` 的 `READ→OPCL` 初始与 `60 get_free_vnode→NULL→return NULL` 的 `NR_VNODES 1024` 空闲 `vnode` 分配及 `61 lock_vnode(new_vp,initial)` 的 `OPCL` 阶段及 `64 lookup(dirp, resolve, &res, rfp)→r!=OK→err_code=r; unlock; return NULL` 的 `lookup` 调用及 `71 find_vnode(res.fs_e,res.inode)→NULL vs !NULL` 的两相分化：命中 `72 unlock(new_vp) →73 lock(vp,initial)→do_downgrade=(lock!=EBUSY)` 的 `EBUSY` 守门与 `78 if(v_ref_count==0) { vp->v_fs_count=1 } else v_fs_count++` 的 `FS` 计数 vanished 修复及未命中 `94 new_vp->v_fs_e=res.fs_e … v_vmnt=find_vmnt(fs_e) → v_dev=m_dev; v_fs_count=1; vp=new_vp` 的 `vmnt` 绑定，`112 dup_vnode(vp)` 的 `VFS` 计数 `v_ref_count++` 与 `113 if(do_downgrade){ *l_vnode=vp; if(initial!=l_vnode_lock) downgrade }` 的 `OPCL→READ` 降级。

### 2.4 `eat_path:133-140` 的 `/→rd vs 非/→wd`

`eat_path:138 start_dir = l_path[0]=='/' ? fp_rd : fp_wd` 的 `/` 前缀分化与 `139 return advance(start_dir, resolve, rfp)` 的 `advance` 委托在 `13` 的 `canonical_path` 与 `15` 的 `common_open` 的 `eat_path` 调用可观测。

### 2.5 `last_dir:145-378` 的 `/` 切分与 `symlink` 循环

`last_dir:176 do{ start_dir = loop_start?loop_start: (l_path[0]=='/'?rd:wd); len=strlen(l_path); if(len==0) ENOENT; #if !DO_POSIX while(len>1 && path[len-1]=='/') len-- → path[len]='\0'; cp=strrchr(l_path,'/'); if(cp==NULL){ strlcpy(dir_entry,path) ; l_path[0]='.' } else if(cp[1]=='\0'){ dir_entry="." } else { strlcpy(dir_entry,cp+1); cp[1]='\0' } ; while(cp>path && cp[0]=='/') cp[0]='\0'; resolve->l_flags&=~PATH_RET_SYMLINK; if((res_vp=advance(start_dir))==NULL) break; strlcpy(l_path,dir_entry); lookup_init(symlink, l_path, RET_SYMLINK) → sym_vp=advance(res_vp) → if(S_ISLNK){ if(ret_on_symlink) break; req_rdlink→r<0→err_code=r; unlock/put→NULL; break; l_path[r]='\0'; if(strrchr(l_path,'/')){ symloop++; if(l_path[0]!='/'){ loop_start=res_vp } else { unlock/put(res_vp) } continue } } else { symloop=0; if(v_fs_e != res_vp->v_fs_e){ unlock/put(sym_vp) + unlock/put(*l_vnode) + lock(m_root_node) + dup + *l_vnode=m_root_node; strlcpy(dir_entry,".") } } break; } while(symloop<_POSIX_SYMLOOP_MAX); if(symloop>=16) ELOOP; if(sym_vp) unlock/put; if(loop_start) unlock/put; strlcpy(l_path,dir_entry); if(ret_on_symlink) flags|=RET_SYMLINK; return res_vp` 的 `/` 尾斜杠忽略与 `strrchr` 切分及 `symlink→rdlink` 的 `relative vs absolute` 的 `loop_start` 分化及 `ELOOP` 阈值及 `EENTER` 的 `m_root_node` 重新加锁。

### 2.6 `lookup:384-569` 的 `req_lookup→EENTER/ELEAVE/SYMLINK` 循环

`lookup:405 if(l_path[0]=='\0') ENOENT` 的空路径守门与 `410 if(!rd||!wd) ENOENT` 的 `fp_rd/wd` 空守门及 `415 fs_e=dir_vp->v_fs_e; dir_ino=dir_vp->v_inode_nr; vmpres=find_vmnt(fs_e); if(NULL)EIO` 的 `vmnt` 查找及 `423 if(rd→v_dev==dir_vp→v_dev) root_ino=rd→ino else 0` 的 `chroot` 根分化及 `429 uid=ACCESS?real:eff; gid同` 的 `ACCESS` 凭证分化及 `434 lock_vmnt(vmpres, READ?WRITE:lock)` 的 `READ→WRITE` 升级及 `449 req_lookup(fs_e,dir_ino,root_ino,uid,gid,resolve,&res,rfp)` 的 `PATH_GET_UCRED` 双 `grant` 包装及 `451 if(r!=OK && r!=EENTER && r!=ELEAVE && r!=ESYMLINK){ unlock; return r }` 的 `OK` vs `E*` 分化及 `459 while(EENTER||ELEAVE||ESYMLINK){ path_off=res.char_processed; memmove(l_path, l_path+off, len); symloop+=res.symloop; if(symloop>16) ELOOP; if(ESYMLINK) dir_vp=fp_rd; else if(EENTER) for(vmp: m_mounted_on→ino==res.inode) dir_vp=m_root_node; else { vmp=find_vmnt(res.fs_e); dir_vp=m_mounted_on; } fs_e=dir_vp→fs_e; dir_ino=dir_vp→ino; if(rd→dev==dir_vp→dev) root_ino=rd→ino else 0; unlock(vmpres); vmpres=find_vmnt(fs_e); lock(vmpres); r=req_lookup(…) ; if(r!=OK&&r!=E*){ unlock; return r } }` 的 `char_processed` 回带与 `symloop` 计数。

### 2.7 `lookup_init:574-588` 的 `NULL` 初始化

`lookup_init:577 assert(vmp!=NULL); assert(vp!=NULL); 580 l_path=path; 581 l_flags=flags; 582 l_vmp=vmp; 583 l_vnode=vp; 584 l_vmnt_lock=NONE; 585 l_vnode_lock=NONE; 586 *vmp=NULL; 587 *vp=NULL` 的 `TLL_NONE` 锁请求初始化与 `NULL` 输出指针初始化在 `open.c:common_open` 的 `lookup_init(&resolve, user_fullpath, 0, &vmp, &vp)` 可观测。

### 2.8 `get_name:593-642` 的 `req_getdents` 循环与 `canonical_path:648-798` 的 `..` 爬升

`get_name:593 get_name(dirp, entry, ename) { pos=0; do{ req_getdents(dirp→fs_e,dirp→ino,pos,buf,DIR_ENTRY_SIZE*8,&new_pos,1) → r==0→ENOENT; r<0→r; consumed=0; do{ cur=buf+consumed; name_len=reclen-offsetof(d_name)-1; if(inode==d_fileno) strlcpy(ename)→OK; consumed+=reclen } while(consumed<totalbytes); pos=new_pos } while(1) }` 的 `getdents` 循环与 `canonical_path:648 canonical_path(orig_path,rfp){ strlcpy(temp,orig); do{ last_dir(&resolve)→dir_vp; rdlink_direct→symloop++ } while(symloop<16); while(dir_vp!=rd){ memmove(orig+len+1, orig) }` 的 `last_dir→rdlink→..` 爬升在 `13` 的 `canonical_path` 调用可观测（`99` 的 `PATH_MAX` 上界）。

### 2.9 `DO_POSIX_PATHNAME_RES 0` 的尾斜杠历史

`path.c:31 DO_POSIX 0` 的 `0` 使 `last_dir:190 while(len>1 && path[len-1]=='/')` 的尾斜杠忽略在 `mkdir("dir/",…)` 的 `dir` 创建可观测，`POSIX 1` 将使 `l_path` 的尾 `/` 视为 `append "."` 的 `dir_entry="."` 分化（`path.c:23` 注释的 `IEEE 1003.1 2004` 引注）。

### 2.10 `copy_path/fetch_name` 的 `sys_safecopyfrom` 与 `NAME_MAX` 截断

`utility.c:24 copy_path: len>PATH_MAX→ENAMETOOLONG` 与 `60 fetch_name: len>PATH_MAX||len==0→EINVAL` 及 `72 sys_safecopyfrom(who_e, addr, SELF, buf, len)` 的跨地址空间拷贝在 `99` 的 `sys_datacopy_wrapper` 可观测（`02-stage-vm/20` 交叉已标注移交）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `path.c:40` 的 `advance` 与 `lookup` 的 `while(EENTER)`，而是吸收 Redox/Linux 的路径解析模型后做取舍。以下决策对应 `.design/13-design.v1.md` D1-D6。

### D1 路径解析状态机显式：`Lookup::resolve` 的 `PathState` 显式

- **C**：`lookup:384` 的 `while(EENTER||ELEAVE||ESYMLINK){ memmove; symloop+=res.symloop; dir_vp=...; lock_vmnt }` 的隐式循环与 `l_path` 可变缓冲区的 `memmove` 游标。
- **Rust**：`Lookup { path: PathBuf, flags: LookupFlags, vmnt_lock: LockKind, vnode_lock: LockKind, output: Option<ResolveOutput> }` 的 `PathState::Start/Resolving/EnteredMount/LeftMount/Symlink/Loop` 枚举 + `Lookup::step(&mut self, fs: &FsClient, vmnt: &VmntTable) -> Result<StepOutcome, LookupError>` 的 `StepOutcome::Continue{ consumed }` 显式 `char_processed` 回带的 `drain(..off)` 切片而非 `memmove`。
- **为什么**：`memmove` 的 `char_processed` 游标在 Rust 以 `PathBuf::drain(..off)` 的 `Range` 显式，`symloop` 的 `u8` 计数在 `Lookup::symloop: u8` 的 `>16→ELOOP` 守门可测试。
- **备选**：保留 `memmove`；否决——`String` 的 `drain` 使 `PATH_MAX` 上界在 `Lookup::new(path) -> Result<Self, PathError::TooLong>` 的 `len>PATH_MAX→Err` 守门可测试。

### D2 挂载点穿越的类型化：`VmntId` 的 `try_enter/try_leave` 显式

- **C**：`lookup:482 for(vmp: m_mounted_on→ino==res.inode && fs_e==res.fs_e) dir_vp=m_root_node` 的线性扫描 `O(8)`。
- **Rust**：`VmntTable::try_enter_mount(ino, fs_e) -> Option<VnodeId>` 的 `find(|v| v.mounted_on==ino && fs==fs_e).map(|v| v.root)` 显式及 `try_leave_mount(fs_e) -> Option<VnodeId>` 的 `find_vmnt(fs_e).map(|v| v.mounted_on)` 显式，使 `EENTER` 的 `m_root_node` 穿越在 `Lookup::EnteredMount{ root }` 的 `resolve.root` 可测试。
- **为什么**：`m_mounted_on` 的 `vnode*` 裸指针在 Rust 以 `VmntId` 的 `Option<VnodeId>` 索引使 `vmnt_unmap_by_endpt` 的 `m_root_node` 回收在 `VmntTable::alloc` 的 `clear` 可观测。

### D3 锁请求的 `TLL_NONE→READ/WRITE` 升级显式

- **C**：`lookup:435 if(VMNT_READ) mnt_lock_type=VMNT_WRITE else l_vmnt_lock` 的 `READ→WRITE` 升级与 `554 if(VMNT_WRITE != mnt_lock_type) downgrade` 的 `WRITE→READ` 降级。
- **Rust**：`Lookup::mnt_lock: LockKind` 的 `Read→Write` 升级在 `Lookup::acquire_vmnt(vmnt, flags)` 的 `match self.mnt_lock { Read => Write, other => other }` 显式，`VMNT_READ` 的 `try_lock` 在 `VmntTable::try_lock(id, LockKind::Write)` 的 `Result` 守门可测试（`tll::TllError::Busy` 的 `EBUSY` 分支）。
- **为什么**：`VMNT_READ` 的 `TLL_READ` 多读者在 Rust 以 `VmntLock::Read(1)` 的 `n` 可观测，`VMNT_WRITE` 的 `TLL_READSER` 串行在 `LifoQueue` 可观测（`07` 的 `LockKind` 对端）。

### D4 符号链接循环的 `SYMLOOP 16` 守门显式

- **C**：`lookup:467 if(symloop>16) return ELOOP` 与 `last_dir:349 while(symloop<16)` 的双守门及 `res.symloop` 的 `FS` 侧计数累加。
- **Rust**：`Lookup::symloop: u8` 的 `checked_add(res.symloop).ok_or(ELOOP)` 使 `u8` 的 `16` 阈值在 `Lookup::step` 的 `symloop>16 → Err(ELOOP)` 守门可测试，`res.symloop` 的 `u8` 在 `LookupRes` 的 `symloop: u8` 可观测。
- **为什么**：`symloop` 的 `int` 在 C 以 `FS` 的 `char symloop` 8 位回带，Rust 以 `u8` 的 `checked_add` 显式溢出检查。

### D5 路径拷贝的 `sys_safecopy` 显式与 `DO_POSIX` 常量

- **C**：`utility.c:24 copy_path→strlen→PATH_MAX→ENAMETOOLONG` 与 `60 fetch_name→safecopy` 的 `who_e` 透传。
- **Rust**：`PathFetcher` trait 的 `fetch(path_addr: VirAddr, len: usize) -> Result<String, PathError>` + `DirectFetcher`（`grant_direct`）与 `SafecopyFetcher`（`sys_safecopy`）双实现；`DO_POSIX_PATHNAME_RES: bool = false` 的 `const` 使 `last_dir` 的 `while(len>1 && path.ends_with('/')) { pop }` 的尾斜杠忽略在 `HistoricalPath` vs `PosixPath` 的 `strip_trailing_slash` 可测试。
- **为什么**：`DO_POSIX 0` 的历史行为在 Rust 以 `const DO_POSIX: bool` 的 `if DO_POSIX { append_dot } else { strip }` 显式，使 `A-10` 的架构演进在代码注释三处一致标注（`const` + doc D5 + 设计§6 表）。

### D6 响应类型化：`LookupRes` 的 `Ok vs EnterMount vs Symlink` 枚举

- **C**：`request.c:495 switch(r){ case OK: inode/fmode…; case EENTERMOUNT: inode/offset/symloop }` 的 `m_fs_vfs_lookup` 联合体回填分化。
- **Rust**：`LookupRes::Ok{ ino, mode, size, dev } / EnterMount{ ino, offset, symloop } / LeaveMount{ offset, symloop } / Symlink{ offset, symloop }` 的 `enum` 使 `char_processed` 的 `offset` 仅在 `Enter/Symlink` 时有效在类型层面不可误用（`Ok` 无 `offset`）。
- **为什么**：`lookup_res` 的 `char_processed` 字段在 `OK` 时无意义，Rust 以 `EnterMount{offset}` 的载荷在 `match` 穷尽可审计。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-10 `DO_POSIX 0` 历史行为 | `HistoricalPath::strip_trailing_slash` | `path.rs:DO_POSIX: bool` + 本文档 D5 + 13 正文 1.3 |
| A-8 64 位 | `FsFlags::IS64BIT` 的 `check_64bit` 复用 | `request.rs:FsFlags` + 本文档 D4 + 13 正文 1.5 |
| A-4 全局聚合 | `Lookup { path, flags, vmnt_lock, vnode_lock }` 的 `LookupOutput` | `path.rs:Lookup` + 本文档 D1 + 13 正文 1.7 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── path.rs             — PathError/SYMLOOP/DO_POSIX + Lookup{path,flags,vmnt_lock,vnode_lock,vmnt,vnode,symloop} + LookupRes(Ok/Enter/Leave/Symlink) + PathFetcher trait (Direct vs Safecopy) + Historical vs Posix 尾斜杠
├── fs_comm.rs          — GlobalComm::sendmsg 的 TransId 编码（11）
├── vmnt.rs             — Vmnt.m_root_node/m_mounted_on 的 try_enter/try_leave（06）
├── vnode.rs            — Vnode{ ino, mode, fs_count } 的 get_free/find/dup（05）
└── fproc.rs            — FProc{fp_rd, fp_wd} 的 /→rd vs 非/→wd 起点（02）
```

### 4.2 `path.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `DO_POSIX 0` | `path.c:31` | `path.rs:DO_POSIX: bool = false` | `strip_trailing_slash` 的 `while(path.ends_with('/')) pop` |
| `SYMLOOP 16` | `const.h:32` | `path.rs:SYMLOOP: usize = 16` | `symloop>16→ELOOP` 的 `checked_add` |
| `PATH_MAX 1024` | `limits.h` | `path.rs:PATH_MAX: usize = 1024` | `Lookup::new(path) len>PATH_MAX→TooLong` |
| `lookup` | `path.h:4` | `path.rs:Lookup { path: String, flags: LookupFlags, vmnt_lock: LockKind, vnode_lock: LockKind, vmnt: Option<VmntId>, vnode: Option<VnodeId>, symloop: u8 }` | `lookup_init` 的 `None` 初始化 |
| `lookup_init` | `path.c:574` | `Lookup::new(path, flags) -> Result<Self, PathError>` | `path→String, flags, vmnt/vnode=None, symloop=0` |
| `advance` | `path.c:40` | `PathResolver::advance(dir, lookup) -> Result<VnodeId, PathError>` | `get_free→lookup→find/dup→downgrade` 两相 |
| `eat_path` | `path.c:133` | `PathResolver::eat_path(lookup, fproc) -> Result<VnodeId, PathError>` | `/→rd vs 非/→wd` 起点 |
| `last_dir` | `path.c:145` | `PathResolver::last_dir(lookup, fproc) -> Result<(VnodeId, String), PathError>` | `strrchr('/')` 切分 + `symlink→rdlink` 循环 |
| `lookup` | `path.c:384` | `Lookup::resolve(fs, vmnt, cred) -> Result<LookupRes, LookupError>` | `req_lookup→EENTER/ELEAVE/SYMLINK→memmove→symloop→ELOOP` |
| `copy_path` | `utility.c:24` | `PathFetcher::copy(path, len) -> Result<String, PathError>` | `len>PATH_MAX→TooLong` |
| `fetch_name` | `utility.c:60` | `PathFetcher::fetch(addr, len) -> Result<String, PathError>` | `safecopy→String` + `PATH_MAX` 截断 |
| `canonical_path` | `path.c:648` | `PathResolver::canonical(path, fproc) -> Result<String, PathError>` | `last_dir→rdlink→..` 爬升 |
| `get_name` | `path.c:593` | `PathResolver::get_name(dir, entry) -> Result<String, PathError>` | `req_getdents` 循环的 `dirent` 解析 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 路径上界 `len ≤ PATH_MAX` | `Lookup::new` | `len>PATH_MAX→TooLong` | `path.c:405` 空路径 `ENOENT` 的 `len==0` 守门 |
| 环阈 `symloop ≤ 16` | `Lookup::resolve` | `checked_add(symloop)→>16→ELOOP` | `path.c:468` |
| 锁升级 `READ→WRITE` | `Lookup::acquire_vmnt` | `if(READ) WRITE else lock` | `path.c:435` |
| 尾斜杠 `DO_POSIX==false → strip` | `HistoricalPath` | `while(path.ends_with('/')) pop` | `path.c:190` |
| 响应分化 `Ok vs Enter` | `LookupRes` | `match LookupRes::Ok{ino} vs Enter{offset}` | `request.c:495` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **178 passed / 0 failed**（`fproc` 13 + `main_loop` 35 + `worker` 19 + `call_table` 8 + `filp` 7 + `vnode` 7 + `vmnt` 7 + `tll` 6 + `ipc/dispatcher` 16 + `fs_comm` 19 + `request` 20 + `path` 15 = 172 → `cargo test` 实测 178；`minix-types` 108 独立）。
> 本章直接影响 `15` 项新增（`lookup_init_null/symloop_e_loop/do_posix_strip/advance_two_phase/eat_path_slash/last_dir_split/canonical_path/path_max/fetch_name_copy/empty_path/loop_init_flags/get_name/mount_enter/path_fetcher_two_impls/slash_handler_two_impls`），`minix-vfs --lib` 总计 163 → 178。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_lookup_init_null` | `path.c:574` | `l_vmp/vnode==None` 初始化 | `path.rs` |
| `test_symloop_e_loop` | `path.c:468` | `symloop 16→ELOOP, 15→Ok` 的 `checked_add` | `path.rs` |
| `test_do_posix_strip` | `path.c:31` | `DO_POSIX false→strip vs true→append` | `path.rs` |
| `test_advance_two_phase` | `path.c:40` | `find hit→v_fs_count++ vs miss→build` 两相 | `path.rs` |
| `test_eat_path_slash` | `path.c:138` | `/→rd vs a→wd` 起点分化 | `path.rs` |
| `test_last_dir_split` | `path.c:198` | `strrchr('/')` 的 `dir_entry` 切分与 `ELOOP` 阈值 | `path.rs` |
| `test_canonical_path` | `path.c:648` | `last_dir→rdlink→..` 爬升的 `PATH_MAX` 上界 | `path.rs` |
| `test_path_max` | `utility.c:24` | `len>PATH_MAX→TooLong` | `path.rs` |
| `test_fetch_name_copy` | `utility.c:60` | `safecopy → String` 的 `PATH_MAX` 截断 | `path.rs` |
| `test_empty_path` | `path.c:405` | `l_path==""→ENOENT` | `path.rs` |
| `test_loop_init_flags` | `path.c:574` | `PATH_RET_SYMLINK` 与 `PATH_GET_UCRED` 位的 `LookupFlags` | `path.rs` |
| `test_get_name` | `path.c:593` | `req_getdents` 循环的 `dirent` 解析 | `path.rs` |
| `test_mount_enter` | `path.c:478` | `EENTERMOUNT→m_root_node` 穿越的 `vmnt` 线性扫描 | `path.rs` |
| `test_path_fetcher_two_impls` | `path.c:40` | `PathFetcher` trait `Direct vs Safecopy` 的 `fetch` 行为差异 | `path.rs` |
| `test_slash_handler_two_impls` | `path.c:31` | `SlashHandler` trait `Historical vs Posix` 的 `strip` 行为差异 | `path.rs` |

测试策略：`Lookup` 的 `NULL` 初始化以 `lookup_init(path,0)→vmnt/vnode==None` 样本覆盖；`SYMLOOP` 以 `symloop=16→ELOOP` 的 `checked_add` 溢出样本覆盖；`DO_POSIX` 以 `false→strip "/"` vs `true→append "."` 的 `Historical vs Posix` 双实现差异样本覆盖；`advance` 以 `find hit→count++` 的 `EEXIST` 样本覆盖；`PathFetcher` 以 `Direct vs Safecopy` 的 `fetch` 长度差异与 `SlashHandler` 以 `Historical strip vs Posix append` 的 `dyn` 行为差异样本覆盖。

---

## 6 过渡

本篇在 `12-request-wrappers` 的 `FsReq::Lookup` 的 `grant_path` 包装与 `TransId` 高位编码之后，`path` 将 `Path → Vnode` 的挂载可观测解析建立为 `13` 的 `lookup` 路径解析，是 `12` 的 `req_lookup` 对端之前、15 的 `common_open` 的 `eat_path` 前置、18 的 `mount` 的 `m_mounted_on` 绑定对端。

```
12-request-wrappers: FsReq(33)/FsResp(Node/LookupRes)/GrantScope 二阶段  （REQ_LOOKUP 的 grant_path 包装）
  │
  └─► 本章: path.rs 的 Lookup{path,flags,vmnt_lock,vnode_lock} + advance/eat_path/last_dir/lookup/canonical_path + SYMLOOP 16 + DO_POSIX 0  （EENTERMOUNT 的 char_processed 回带与 SYMLOOP 的 symloop 计数）
         │
         ├─► 15-open-close: common_open 的 eat_path→lookup_init→advance 的 open 链  （本章 eat_path 的 /→rd 起点对端）
         ├─► 18-mount: mount 的 m_mounted_on→m_root_node 穿越  （本章 lookup 的 EENTER 循环对端）
         └─► 25-exec: exec 的 is_script 的 PATH_RET_SYMLINK 分化  （本章 lookup_init 的 flags 透传对端）
```

`DO_POSIX_PATHNAME_RES 0` 的尾斜杠忽略在 `HistoricalPath` 的 `strip` 可观测，为 `99` 的 `PATH_MAX` 常量与 `const.h:32 SYMLOOP` 提供 `A-10` 的历史行为回收可观测。阅读顺序提示：若关心“`open` 如何从路径到 `vnode`”，下一站 `15-open-close.md`；若关心“`mount` 如何建立穿越点”，下一站 `18-mount.md`。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/path.c:40-933`（`advance:40` 的 `get_free→lookup→find/dup→downgrade`、`133 eat_path` 的 `/→rd`、`145 last_dir` 的 `strrchr('/')` 切分与 `symlink→rdlink` 循环、`384 lookup` 的 `req_lookup→EENTER/ELEAVE/SYMLINK→memmove`、`574 lookup_init` 的 `NULL` 初始化、`593 get_name` 的 `req_getdents` 循环、`648 canonical_path` 的 `..` 爬升、`31 DO_POSIX 0`）、`minix3/minix/servers/vfs/path.h:4`（`lookup` 结构 5 字段）、`minix3/minix/servers/vfs/utility.c:24-93`（`copy_path` 的 `PATH_MAX` 截断与 `fetch_name` 的 `safecopy`）、`minix3/minix/include/minix/vfsif.h:12`（`PATH_GET_UCRED 020`）、`minix3/minix/servers/vfs/const.h:32`（`SYMLOOP 16`）、`minix3/minix/include/sys/param.h`（`PATH_MAX 1024`）
- 阶段文档：`05-vnode-table.md`（`Vnode{fs_e,ino}` 的 `find_vnode` 命中）、`06-vmnt-table.md`（`Vmnt{m_mounted_on,m_root_node}` 的 `try_enter`）、`11-fs-comm.md`（`GlobalComm::sendmsg` 的 `c_max_reqs` 窗口）、`12-request-wrappers.md`（`FsReq::Lookup` 的 `grant_path` 包装）、`99-global-concepts.md`（`PATH_MAX/SYMLOOP/DO_POSIX` 常量）
- Rust 实现：`os/servers/vfs/src/path.rs:1`（`Lookup{path,flags,vmnt_lock,vnode_lock,symloop}` + `LookupRes(Ok/Enter/Leave/Symlink)` + `PathFetcher trait (Direct vs Safecopy) + Historical vs Posix`）、`os/servers/vfs/src/fs_comm.rs:1`（`GlobalComm` 的 `TransId` 编码对端）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（`sys_safecopyfrom` 的跨地址空间拷贝）

