//! `exec_worker` — NS5 执行件：`pm_exec` 管线的 I/O 落地。
//!
//! C 对位 `minix3/minix/servers/vfs/exec.c:185-402`（`pm_exec` 本体）+
//! `minix3/minix/lib/libexec/exec_elf.c:131-318`（`libexec_load_elf`）+
//! `exec_general.c:78-90`（`libexec_pm_newexec`）+ `libexec.h:24-58`
//! （`struct exec_info` 的 192 字节线格式）。判定层（阶段/门/开关/清理的
//! **知识**）住在 [`crate::exec`]（25-exec.md D1-D7）；本模块只做那些知识
//! **落到线上**的部分：跨进程取帧、寻路开卷、读首块、ELF 段装载（VM 分配
//! + FS 直写 + 内核清零）、PM_EXEC_NEW 通告与终局。
//!
//! # 模型定位（ARCH A-1）
//!
//! C 的 `pm_exec` 跑在 worker 线程里，阻塞在 `fs_sendrec`/VM/PM 往返上；
//! minix-rs VFS 是单线程事件循环，exec 按 25-exec.md ARCH 决策"同步到底无
//! 续作"——臂内串完所有往返再回主循环（与 boot 段 `finish_init` 的同步
//! `WireFsClient` 同型）。代价是 exec 期间不收别的消息（C 可交错），这是
//! A-1 全卷既有的登记偏差。
//!
//! # 本波次范围（NS5）
//!
//! * **静态 ELF 全链**：identity 三检 → 首块 → 段装载 → PM_EXEC_NEW → 回 PM。
//! * `#!` 脚本分支（exec.c:259-271）与动态链接分支（exec.c:278-314）**后置**：
//!   检出即回 `ENOEXEC`——C 语义里"没有装载器肯收这段字节"的同一错误面
//!   （`libexec_load_elf` 对非 ELF/带解释器镜像都回 ENOEXEC，
//!   exec_elf.c:143-148/186-189）。差异登记在 new_edge3。
//! * `AT_*` 辅向量注入不是缺口：`stack_prepare_elf` 首行
//!   `if (!execi->is_dyn) return OK`（exec.c:417-419）——静态帧原样过山。
//! * `vmfd` 直映射路（exec.c:316-335）不设：C 本就以 `read_seg` 分段读为
//!   兜底路（25-exec.md §4.3"装载有路"不变量），行为等价、吞吐另计。
//! * `VM_MMAP` 载荷走 `MessMmap` 的 64 位车道（NS5-A 已修，曾经是 u32 截
//!   断阻塞位）：C `mess_mmap`（ipc.h:1583-1593）受 56 字节载荷断言约束
//!   只有 i386 形态（addr/len/retaddr 32 位），LP64 下同一字段表 72 字节
//!   断言必炸——minix-rs 用户态是 LP64（栈顶 `0x7fff_ffff_f000` 一族），
//!   线载诚实 64 位值，与 `m_vm_pagefault`/E-VMMCPWIRE 同一裁决。车道图
//!   单点权威在 minix-types `MessMmap` 注释。

extern crate alloc;

use crate::exec::{self, ExecError};
use crate::fproc::PID_FREE;
use crate::misc::PAGE_SIZE;
use crate::path::{self, Lookup, LookupRes, LookupStart, MountedFs, PathError, RootDir};
use crate::protect::{ForbidInput, forbidden_decision};
use crate::vnode::{VnodeId, VnodeTable};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::Cell;
use minix_sys::ipc::IpcTransport;
use minix_sys::syscall::{KernelCallTransport, sys_datacopy, sys_memset};
use minix_types::{CpFlags, Endpoint, Message};

/// 首读缓冲的页数：`static char hdr[10*PAGE_SIZE] __aligned(8)`（exec.c:738）。
const HEADER_BUF_PAGES: usize = 10;

/// `PT_INTERP`（`sys/exec_elf.h`）——动态解释器段在场即 dyn 二进制
/// （`elf_has_interpreter`，exec_elf.c:73-110）。
const PT_INTERP: u32 = 3;

/// `ET_DYN`（`elf_sane` 收 ET_EXEC 与 ET_DYN 两者，exec_elf.c:29-32）。
const ET_DYN: u16 = 3;

/// `PF_X`（exec_elf.c:289-293 的 text/data 记账判定位）。
const PF_X: u32 = 0x1;

/// mmap 标志位（权威值 = VM `MmapFlags` 位表，`os/servers/vm/src/mmap.rs:59-66`；
/// C 对位 mman.h/vm.h 的 Linux 兼容位型）。
mod map_flags {
    pub const PRIVATE: u32 = 0x0002;
    pub const FIXED: u32 = 0x0010;
    pub const ANON: u32 = 0x1000;
    pub const THIRDPARTY: u32 = 0x0080_0000;
    pub const PREALLOC: u32 = 0x0008_0000;
    pub const UNINITIALIZED: u32 = 0x0004_0000;
}

/// `PROT_READ|PROT_WRITE|PROT_EXEC`——libexec 的 alloc 回调一律三权齐
/// （exec_general.c:9-31）。
const PROT_RWX: u32 = 0x1 | 0x2 | 0x4;

/// `sizeof(struct stat)` 的 LP64 值——req_stat 的 magic grant 窗口
/// （request.c:1087 同样按 `sizeof(struct stat)` 授权）。
const STAT_BUF_SIZE: u64 = 144;

/// VFS 给**自己缓冲**建 magic grant 时的 `who_from`：必须是**具体端点**
/// `Endpoint::VFS`，绝不能用 `Endpoint::SELF` 哨兵。
///
/// **B37 根因**：内核 `verify_grant` 把 magic grant 的 `who_from` 原样装为
/// `effective_granter`（`kernel/src/grant.rs:461`），随后的数据拷贝用它在
/// 该端点的页表里解析 `start` 地址。`Endpoint::SELF` 是哨兵
/// （`ENDPOINT_SLOT_TOP - 3`，`is_valid() == false`），不是任何真实进程——
/// 内核拿它解析页表必失败，而 FS 侧 `copy_out` 吞掉拷贝错误
/// （`let _ = ipc.copy_to(..)`，`fs-rt/src/transport.rs:272`）→ grant 目标
/// 缓冲一字节未落、`req_read` 却回 `Ok`。真机 bn34p 实锤
/// `after-read hdr[0..8]=00 00 00 00`（exec 首块读）即此。C `map_header`
/// （exec.c:755）用 `VFS_PROC_NR` 具体号，本常量对齐之。段装载读走
/// `target_e`（目标进程具体端点，exec.c:709-711）本就是对的，不在此列。
const VFS_LOCAL_WHO_FROM: Endpoint = Endpoint::VFS;

/// 统一把任何符号的结果折成**负 errno**（C `pm_exec` 的 `return r` 语义：
/// worker 把返回值原样折进 `VFS_PM_STATUS`，PM 按 `result != OK` 收）。
fn neg(v: i32) -> i32 {
    if v > 0 { -v } else { v.max(-0x7fff_ffff) }
}

/// FS 往返的事务打包（B32 修复）。C 的每一条 VFS→FS 请求都在
/// `sendmsg` 里做 `TRNS_ADD_ID`（`comm.c:21`）——REQ 号移进 `m_type`
/// 高 16、transid 落低 16；回复则统一 `TRNS_DEL_ID`（`main.c:88`）从
/// 高 16 取真实 status。`exec_worker` 的同步 `ipc.sendrec(fs_e, …)`
/// 绕过了 `fs_comm::send_fs` 的戳入/拆封，直接发 `m_type = REQ_xxx`
/// （高位为 0），FS 侧 `TransactionId::decode` 按 `m_type >> 16` 路由就
/// 解析出 call=0（无法路由），而回复的 `m_type` 又被原样当 errno 折进
/// 结果——真机实锤 status=`0x4E0A1A`，低 16 恰是 `REQ_LOOKUP`（FS_BASE
/// 0xA00+26=0xA1A）的回显。这两个助手补齐这两步，**直接委托 `minix_types`
/// 的 `trns_add_id`/`trns_del_id`**（vfsif.h 三行宏的权威逐行落地，改一处
/// 须同步其它处）。transid 取 **0**（与姊妹同步腿 `request.rs` 的
/// REQ_READSUPER 同一裁决）：同步 sendrec 靠阻塞往返自匹配回复、
/// 不看 transid；且 0 不在 `VFS_TRANSID + slot` 的 worker 号空间（slot 0
/// 会撞 `encode(0)=0xB01`、与主循环 `handle_fs_reply` 按低 16 反解 worker
/// 槽相冲），错投时会被判非法 transid 丢弃而非污染 worker 0。仅 FS
/// 腿需要——VM/PM 是 taskcall 风格服务器，`m_type` 直接是结果，不打包事务。
fn fs_trans_stamp(msg: &mut Message) {
    msg.m_type = minix_types::trns_add_id(msg.m_type, 0);
}

/// 从 FS 回复的 `m_type` 取真实 status（C `TRNS_DEL_ID = (short)(m_type >> 16)`，
/// 委托 `minix_types::trns_del_id` 权威件）。
fn fs_trans_status(msg: &Message) -> i32 {
    minix_types::trns_del_id(msg.m_type)
}

/// 成功终局：PM `exec_restart` 需要的三元组（com.h:570-575 的
/// `VFS_PM_PC`/`VFS_PM_NEWSP`/`VFS_PM_NEWPS_STR`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecLoaded {
    /// `*pc` — 新入口（`e_entry + load_offset`，exec_elf.c:311）。
    pub pc: u64,
    /// `*newsp` — 新栈指针（exec.c:371）。
    pub newsp: u64,
    /// `*ps_str` — caller（`exec_via` 的 `process_strings_address`）已按**新栈**
    /// 算好的绝对 ps_strings 地址，VFS 原样带回给 PM（供内核种进子 RBX）。
    /// LP64 下须为完整 64 位（B43：旧 `i32` 会把 `0x0000_7fff_xxxx` 截断+PM 符号扩展）。
    pub newps_str: u64,
}

/// 一次 exec 的载荷（`VfsCall::Exec` 六域 + 栈顶解析缝）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecRequest {
    /// 目标进程（`VFS_PM_ENDPT`，m7_i1）。
    pub target: Endpoint,
    /// 路径在目标进程空间的位置（`VFS_PM_PATH`，m7_p1）。
    pub path_addr: u64,
    /// 路径长度含 NUL（`VFS_PM_PATH_LEN`，m7_i2）。
    pub path_len: usize,
    /// 参数帧在目标进程空间的位置（`VFS_PM_FRAME`，m7_p2）。
    pub frame_addr: u64,
    /// 帧长（`VFS_PM_FRAME_LEN`，m7_i3）。
    pub frame_len: usize,
    /// ps_strings 指针（`VFS_PM_PS_STR`）。LP64 下完整 64 位（B43，见 `ExecLoaded::newps_str`）。
    pub ps_str: u64,
    /// 栈顶。C 取 kerninfo `user_sp`（exec.c:205 经 `minix_get_user_sp`，
    /// kernel_utils.c:40-60）；kerninfo 用户映射归 E-KERNINFO 波次，这里
    /// 显式作缝：调用方（生产：kerninfo 读半；测试：定值）供给。
    pub stack_high: u64,
}

/// exec 装载失败——负 errno 直通 PM。
pub type ExecErrno = i32;

/// ELF 首块的字节级读数（`Elf64_Ehdr`/`Elf64_Phdr`，LP64 布局）。
///
/// C 的 phdr 表必须住在首扇区（`elf_sane`，exec_elf.c:34-38），段数据则
/// **不在**首块里——所以这里只解析表头，不做全镜像迭代器（`minix_elf` 的
/// `SegmentIter` 校验"段数据落像内"，对 exec 的分块读会把每段都跳掉）。
struct ElfHead<'a> {
    buf: &'a [u8],
}

impl<'a> ElfHead<'a> {
    /// 解析并校验 Ehdr——C `check_header`（exec_elf.c:56-68）与 `elf_sane`
    /// （:26-44）的合并：魔数/数据序/两处版本/phentsize/e_type/phoff 界。
    fn parse(buf: &'a [u8]) -> Result<Self, ExecErrno> {
        let enoexec = ExecError::NoExec.to_errno();
        if buf.len() < 64 {
            return Err(ExecError::NoExec.to_errno()); // hdr_len < sizeof(Ehdr)（elf_unpack :50-52）
        }
        let magic_ok = buf[0] == 0x7f && &buf[1..4] == b"ELF";
        let data_ok = buf[5] == 1; // EI_DATA = ELFDATA2LSB（ELF_TARG_DATA）
        let ident_ver_ok = buf[6] == 1; // EI_VERSION = EV_CURRENT
        let phentsize = u16_at(buf, 54);
        let e_version = u32_at(buf, 20);
        // elf_64.h 的 ELF_TARG_VER = EV_CURRENT = 1。
        if !magic_ok || !data_ok || !ident_ver_ok || phentsize != 56 || e_version != 1 {
            return Err(enoexec);
        }
        let e_type = u16_at(buf, 16);
        if e_type != minix_elf::ET_EXEC && e_type != ET_DYN {
            return Err(enoexec); // elf_sane（exec_elf.c:29-32）
        }
        let e_phoff = u64_at(buf, 32);
        let e_phnum = u16_at(buf, 56) as u64;
        // phdr 表必须整体住在**已加载的首块缓冲**里。C `elf_sane`
        // （exec_elf.c:34-38）以 `SECTOR_SIZE`（512）为界，但 C 是 32 位时代
        // （`__ELF_WORD_SIZE 32`、`Elf32_Phdr` 32 字节），phdr 表 ≤512 天然成立；
        // minix-rs 是 LP64（`e_phentsize`=56），正常二进制的 phdr 表（如 10 项
        // =624B）合法超过 512，而 `map_header`（exec.c:738）本就加载 10 页
        // （`buf.len()`）。故界取实际加载的头缓冲长——正是 C `elf_unpack` 里
        // 那条被 `#if 0` 停用的 `phdr + phnum >= hdr_len` 检查（exec_elf.c）的
        // 本意（32 位下 SECTOR_SIZE 够用故被注释，LP64 下必须按缓冲长校验）。
        let hdr_loaded = buf.len() as u64;
        if e_phoff > hdr_loaded || e_phoff.saturating_add(e_phnum * 56) > hdr_loaded {
            return Err(enoexec);
        }
        Ok(Self { buf })
    }

    /// 入口地址（`e_entry`，exec_elf.c:311）。
    fn entry(&self) -> u64 {
        u64_at(self.buf, 24)
    }

    /// phdr 数量（`e_phnum`）。
    fn phnum(&self) -> usize {
        u16_at(self.buf, 56) as usize
    }

    /// 第 `i` 个 phdr 的六元组
    /// `(p_type, p_flags, p_offset, p_vaddr, p_filesz, p_memsz)`。
    fn phdr(&self, i: usize) -> (u32, u32, u64, u64, u64, u64) {
        let at = u64_at(self.buf, 32) as usize + i * 56;
        let b = self.buf;
        (
            u32_at(b, at),
            u32_at(b, at + 4),
            u64_at(b, at + 8),
            u64_at(b, at + 16),
            u64_at(b, at + 32),
            u64_at(b, at + 40),
        )
    }

    /// 是否带 `PT_INTERP`（`elf_has_interpreter` 的存在性消费，
    /// exec_elf.c:73-110：>0 即动态链接）。
    fn has_interpreter(&self) -> bool {
        (0..self.phnum()).any(|i| self.phdr(i).0 == PT_INTERP)
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// 对页上取整（exec_elf.c:157-158 的 `roundup` 页算术）。
fn roundup(value: u64, unit: u64) -> u64 {
    value.div_ceil(unit) * unit
}

/// 路径末组件（`get_read_vp` 的 copyprogname 半，exec.c:109-115）。
fn progname_of(fullpath: &str) -> String {
    match fullpath.rfind('/') {
        Some(pos) => fullpath[pos + 1..].to_string(),
        None => fullpath.to_string(),
    }
}

/// `pm_exec` 的公共入口：把 [`pm_exec_inner`] 管线的失败腿统一折成**负 errno**。
///
/// **B33a 修复（符号掩盖）**：[`ExecErrno`] 的类型契约与 main_loop.rs
/// 「失败为负 errno」注释均要求 exec 失败以**负 errno** 直达 PM（PM
/// `exec_restart` 将 status 原样写进回复 `m_type`，init `perform_syscall`
/// 按 `m_type < 0` 判 `Err`）。但 inner 体内多条静态腿（`enoexec` 裸
/// 值与 `ExecError::*::to_errno()`，真机 bn28p 实锤 status=**+8** ENOEXEC）
/// 泄漏了**正值**，被 init 侧 `exec_via` 的 `Ok(_) => EIO` 成功车道吞成
/// EIO——真 errno 永远浮不出来。在此单点用 [`neg`] 折负（对已为负的
/// IPC 往返腿幂等），使真 errno（如 ENOEXEC）从 `perform_syscall` 的 Err
/// 车道浮现。选公共边界而非逐腿修：一条腿一个折叠点既易漏又 churn 高。
#[inline]
pub fn pm_exec<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    req: ExecRequest,
) -> Result<ExecLoaded, ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    pm_exec_inner(st, kernel, ipc, req).map_err(neg)
}

/// `pm_exec` 本体（exec.c:185-402）。
///
/// 借用全来自 [`crate::main_loop::VfsState`]（ARCH A-4 聚合）——以独立的
/// `impl` 块挂在该类型上，判定知识留 [`crate::exec`]。成功返回
/// [`ExecLoaded`]；失败返回 mixed-sign errno（正值静态腿由 [`pm_exec`] 边界统一折负）。
///
/// 全程铁律：任何失败路径都先归还已领的 vnode 引用（C `pm_execfinal` 的
/// `unlock_vnode + put_vnode`，exec.c:392-395——归还经 `PutNodeSink` 进
/// `pending_puts` 队列，主循环统一投）。装载中途失败的再拆半装镜像
/// （exec_elf.c:279-281/293-295 的失败支重跑 clearproc）。
///
/// 注：本体内多条静态失败腿（`enoexec` 裸值、`ExecError::*::to_errno()`）
/// 返回**正值**，统一由 [`pm_exec`] 边界折负——见该函数 B33a 契约说明。
fn pm_exec_inner<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    req: ExecRequest,
) -> Result<ExecLoaded, ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    let enoexec = ExecError::NoExec.to_errno();

    // 目标槽与 fproc——C 的 `fp`（exec.c:190 一带的 `isokendpt_fproc`）。
    let slot = req.target.to_user_slot().ok_or(enoexec)?;
    {
        let fp = st.fproc_table.get(slot).ok_or(enoexec)?;
        if fp.pid == PID_FREE {
            return Err(enoexec);
        }
    }
    let (target_e, uid, gid) = {
        let fp = st.fproc_table.get(slot).expect("checked above");
        (fp.endpoint, fp.eff_uid, fp.eff_gid)
    };

    // ── 取帧（exec.c:226-238）：先于一切破坏性步骤；帧太大即 ENOMEM
    // （exec.c:227-229 的 ARG_MAX 门在拷贝之前）。
    if req.frame_len > exec::ARG_MAX {
        return Err(neg(minix_types::ENOMEM));
    }
    // C 的 mbuf 是函数级 static（BSS 零初始化，exec.c:207）；这里保持零
    // 填安全默认（datacopy 失败即弃帧，无未初始化读取），lint 建议的
    // set_len 属 unsafe，不值得为一处瞬时缓冲引入。
    #[allow(clippy::slow_vector_initialization)]
    let mut frame = {
        let mut v = alloc::vec::Vec::with_capacity(req.frame_len);
        v.resize(req.frame_len, 0);
        v
    };
    sys_datacopy(
        kernel,
        target_e.get(),
        req.frame_addr,
        Endpoint::SELF.get(),
        frame.as_mut_ptr() as u64,
        req.frame_len as u64,
    )
    .map_err(neg)?;

    // vsp 在栈补丁前定死（exec.c:243-245 注释）。
    let vsp = req.stack_high - req.frame_len as u64;

    // 凭证缺省：保留旧 eff uid/gid（exec.c:247-248）。
    let mut new_uid = uid;
    let mut new_gid = gid;

    // ── 取名（exec.c:242 `fetch_name`；utility.c:60-90）。
    let fullpath = fetch_name(kernel, target_e, req.path_addr, req.path_len)?;

    // 首块缓冲（exec.c:738 的函数级 static 在单线程模型里是本函数局部——
    // 生命周期覆盖 identity 三检、ELF 解析与 PM_EXEC_NEW，往返期间地址
    // 稳定满足 grant 窗口）。
    let mut hdr_buf = alloc::boxed::Box::new([0u8; HEADER_BUF_PAGES * PAGE_SIZE as usize]);

    // ── 开卷 + 三检 + 首块（`get_read_vp`，exec.c:89-154；sugid=1，
    // exec.c:254）。setuid/setgid 位在场时改写 new_uid/new_gid 提议。
    let vnode_id = open_exec(
        st,
        kernel,
        ipc,
        &fullpath,
        target_e,
        hdr_buf.as_mut_slice(),
        &mut new_uid,
        &mut new_gid,
    )?;
    let suid_proposal = {
        let v = st.vnode_table.get(VnodeId(vnode_id)).expect("opened above");
        v.mode & (0o4000 | 0o2000) != 0 // I_SET_UID_BIT | I_SET_GID_BIT（exec.c:141-149）
    };

    // 失败清扫：归还 vnode 引用（pm_execfinal，exec.c:392-395）。
    macro_rules! bail {
        ($errno:expr) => {{
            release_vnode(st, VnodeId(vnode_id));
            return Err($errno);
        }};
    }

    // ── 首块字节定名分（`is_script`，exec.c:522-529；判定层同函数）。
    if exec::is_script(hdr_buf.as_slice()) {
        bail!(enoexec); // 脚本分支后置（NS5 登记）
    }

    // ── ELF 解析 + dyn 拒收（`elf_unpack`/`elf_has_interpreter`）。
    let elf = match ElfHead::parse(hdr_buf.as_slice()) {
        Ok(e) => e,
        Err(e) => bail!(e),
    };
    if elf.has_interpreter() {
        bail!(enoexec); // exec.c:278-314 分支后置（NS5 登记）
    }

    // ── 全 phdr 预检（exec_elf.c:184-191）：先验明再拆旧地址空间。
    let v_size = st
        .vnode_table
        .get(VnodeId(vnode_id))
        .map(|v| v.size)
        .unwrap_or(0);
    for i in 0..elf.phnum() {
        let (_, _, p_offset, _, p_filesz, _) = elf.phdr(i);
        if p_offset.saturating_add(p_filesz) > v_size {
            bail!(enoexec);
        }
    }

    // ── 拆旧地址空间（`clearproc` → `libexec_clearproc_vm_procctl`，
    // exec_elf.c:194；VM_PROCCTL 的 VMPPARAM_CLEAR，exit.c:129-137）。
    if let Err(e) = vm_procctl_clear(ipc, target_e) {
        bail!(e);
    }

    // ── 段装载（exec_elf.c:197-306 的 read_seg 路）。借用拆分：段读要
    // 同时触 vnode 表（不可变）与 grant 表（可变）——字段级拆借。
    {
        let vnodes: &VnodeTable = &st.vnode_table;
        let grants = &mut st.grants;
        if let Err(e) = load_elf_segments(
            kernel,
            ipc,
            vnodes,
            grants,
            VnodeId(vnode_id),
            target_e,
            &elf,
        ) {
            // C 的失败清扫再跑一次 clearproc（exec_elf.c:279-281/293-295），
            // 防止半装镜像滞留。
            let _ = vm_procctl_clear(ipc, target_e);
            bail!(e);
        }
    }

    // ── 栈分配（exec_elf.c:308-313 的 `allocmem_ondemand(stacklow,
    // stack_size)`；execi.stack_high/stack_size 的页整，exec_elf.c:156-158）。
    // 栈顶 0x7fff_ffff_f000 一族 > 4 GiB：VM_MMAP 的 NS5-A 64 位车道
    // 承载，无截断（u32 臂时代这里是真机阻塞位）。
    let stack_high_paged = req.stack_high - req.stack_high % PAGE_SIZE;
    let stack_size_paged = roundup(exec::DEFAULT_STACK_LIMIT, PAGE_SIZE);
    let stacklow = stack_high_paged - stack_size_paged;
    if let Err(e) = vm_mmap(ipc, target_e, stacklow, stack_size_paged, 0, PROT_RWX) {
        let _ = vm_procctl_clear(ipc, target_e);
        bail!(e);
    }

    // ── PM_EXEC_NEW（exec.c:359；exec_general.c:78-90）。回程 suid 位 =
    // PM 对提议的裁决（do_newexec 的 tracer 门 + VFS 提议位，
    // pm/exec.c:90-122），决定收尾要不要换凭证（exec.c:378-383）。
    let allow_setuid = match pm_newexec(
        ipc,
        target_e,
        hdr_buf.as_ptr() as u64,
        hdr_buf.len(),
        &progname_of(&fullpath),
        new_uid,
        new_gid,
        suid_proposal,
        stack_high_paged,
        stack_size_paged,
        req.frame_len,
    ) {
        Ok(allow) => allow,
        Err(e) => bail!(e),
    };

    // ── 备栈：静态执行体不做 aux 注入（exec.c:417-419 `!is_dyn → OK`），
    // 帧原样落位。──拷帧（exec.c:368-369）。
    if let Err(e) = sys_datacopy(
        kernel,
        Endpoint::SELF.get(),
        frame.as_ptr() as u64,
        target_e.get(),
        vsp,
        req.frame_len as u64,
    ) {
        bail!(neg(e));
    }

    // ── 收尾（exec.c:374-384）：clo_exec → 换凭证 → 记名。
    {
        let fp = st.fproc_table.get_mut(slot).expect("checked above");
        exec::clo_exec(fp, &mut st.filp_table);
        let (u, g) = exec::settle_ids(allow_setuid, uid, gid, new_uid, new_gid);
        fp.eff_uid = u;
        fp.eff_gid = g;
        let mut name = [0u8; crate::fproc::PROC_NAME_LEN];
        let pn = progname_of(&fullpath);
        let n = pn.len().min(crate::fproc::PROC_NAME_LEN);
        name[..n].copy_from_slice(&pn.as_bytes()[..n]);
        fp.name = name;
    }

    release_vnode(st, VnodeId(vnode_id));
    Ok(ExecLoaded {
        pc: elf.entry(),
        newsp: vsp,
        newps_str: req.ps_str,
    })
}

/// 失败/终局共用的 vnode 归还（C `unlock_vnode + put_vnode`，exec.c:392-395；
/// 慢路径的 REQ_PUTNODE 进 `pending_puts` 统一投）。
fn release_vnode(st: &mut crate::main_loop::VfsState, id: VnodeId) {
    let mut sink = crate::main_loop::put_node_sink(&mut st.pending_puts);
    let _ = st.vnode_table.put(id, &mut sink);
}

/// `fetch_name`（utility.c:60-90 的执行对照）：目标进程空间取路径。
fn fetch_name<K: KernelCallTransport>(
    kernel: &K,
    who: Endpoint,
    addr: u64,
    len: usize,
) -> Result<String, ExecErrno> {
    if len > path::PATH_MAX {
        return Err(neg(minix_types::ENAMETOOLONG));
    }
    if len == 0 {
        // 与 path.rs `SysPathFetcher` 的取数门同约定（utility.c 两门中
        // 的短长度侧 → EINVAL）。
        return Err(neg(minix_types::EINVAL));
    }
    let mut buf = [0u8; path::PATH_MAX];
    sys_datacopy(
        kernel,
        who.get(),
        addr,
        Endpoint::SELF.get(),
        buf.as_mut_ptr() as u64,
        len as u64,
    )
    .map_err(neg)?;
    let n = len.min(path::PATH_MAX);
    let end = buf[..n].iter().position(|&b| b == 0).unwrap_or(n);
    if end == 0 {
        return Err(ExecError::NoExec.to_errno()); // 空路径
    }
    core::str::from_utf8(&buf[..end])
        .map(str::to_string)
        .map_err(|_| ExecError::NoExec.to_errno())
}

/// exec 开卷三检之二：执行权限判决（C `exec.c:128` `forbidden(fp, vp, X_BIT)`）。
///
/// 抽成纯函数，是为了让「`access` 必须落在低 3 位三元组」这个 B33b 缺陷点
/// 直接进入单测射程：`access` 常量藏在内联的 `ForbidInput` 构造里时，回退
/// 成 9 位模式位（`0o100`）不会让任何测试变红（既有单测都绕过三检门直调
/// `load_elf_segments`）。本函数把带常量的构造与判决封进一个可用原语驱动
/// 的单元，改错 `access` 立即测红。
fn exec_forbid_check(
    file_mode: u32,
    file_uid: u32,
    file_gid: u32,
    real_uid: u32,
    real_gid: u32,
    eff_uid: u32,
    eff_gid: u32,
) -> Result<(), crate::protect::ProtectError> {
    forbidden_decision(&ForbidInput {
        real_uid,
        real_gid,
        eff_uid,
        eff_gid,
        is_access_call: false,
        file_uid,
        file_gid,
        mode: file_mode & 0o777,
        // C `forbidden` 的 `access_desired` 传 `X_BIT`（=0o1，`const.h:119`），
        // 与 `perm_bits`（root 为 `R|W|X`、否则 `(mode >> shift) & RWX_BITS`）
        // 同处低 3 位三元组，末判据 `(perm | access) != perm` 要 access ⊆ perm。
        // 传 9 位模式的 owner-X 位（`0o100`）会使 root/owner 的 `perm=0o7` 恒判
        // 失败 → 误拒 EACCES（真机实锤 mode=0o100755、euid=0、root 自有文件被拒）。
        // 全仓其余 `forbidden_decision` 调用点（`main_loop.rs` 11 处）传的均为
        // `crate::open::*` 低 3 位常量（`R_BIT`/`W_BIT`/`X_BIT` 或其 OR），无一
        // 用 9 位模式值；exec 按 C `exec.c:128` 取 `X_BIT`。
        access: crate::protect::X_BIT,
        is_dir: false,
        supp: &[],
        readonly_fs: false,
    })
}

/// 开卷 + 三项检查 + suid/sgid 提议 + 首块读
/// （`get_read_vp`，exec.c:89-154；`map_header`，:736-763）。
///
/// 成功返回 vnode 槽位；`new_uid`/`new_gid` 出参在 setuid/setgid 位在场时
/// 被改写（C 的 `execi.args.new_uid/new_gid`，exec.c:141-150）。
#[allow(clippy::too_many_arguments)]
fn open_exec<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    fullpath: &str,
    target_e: Endpoint,
    hdr_buf: &mut [u8],
    new_uid: &mut u32,
    new_gid: &mut u32,
) -> Result<usize, ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    // 寻路环境取目标进程 fproc（worker 上下文里 C 的 `fp` 即目标进程）。
    let slot = target_e.to_user_slot().ok_or(ExecError::Io.to_errno())?;
    let (rd, wd, uid, gid, real_uid, real_gid) = {
        let fp = st.fproc_table.get(slot).ok_or(ExecError::Io.to_errno())?;
        let rd = fp
            .root_dir
            .and_then(|idx| st.vnode_table.get(VnodeId(idx)))
            .map(|v| RootDir {
                ino: v.ino,
                fs: v.fs,
                dev: v.dev,
            })
            .unwrap_or(RootDir {
                ino: 1,
                fs: Endpoint::NONE,
                dev: 0,
            });
        let wd = fp
            .work_dir
            .and_then(|idx| st.vnode_table.get(VnodeId(idx)))
            .map(|v| LookupStart {
                ino: v.ino,
                fs: v.fs,
                dev: v.dev,
            })
            .unwrap_or(LookupStart {
                ino: 1,
                fs: Endpoint::NONE,
                dev: 0,
            });
        (rd, wd, fp.eff_uid, fp.eff_gid, fp.real_uid, fp.real_gid)
    };
    let mut resolve = Lookup::new(fullpath.to_string(), path::LookupFlags::NOFLAGS)
        .map_err(|e: PathError| e.to_errno())?;
    let mounts: Vec<MountedFs> = st.mounted_fs_list();

    // 同步 lookup 缝：一次 REQ_LOOKUP 往返 = C `req_lookup`（request.c:430-500）
    // 的三步舞。路径 grant 带 `CPF_READ|CPF_WRITE`——FS 要把剩余路径写回
    // （EENTERMOUNT/ELEAVEMOUNT/ESYMLINK 的进度报告方法）。传输层/FS 侧
    // 失败的原始 errno 经 `last_errno` 保真（PathError 词表没有 Io，
    // 从缝里直接带出去，避免降格成 EINVAL）。
    let last_errno: Cell<i32> = Cell::new(0);
    let path_scratch: core::cell::RefCell<[u8; path::PATH_MAX]> =
        core::cell::RefCell::new([0; path::PATH_MAX]);
    let vnode_id = {
        let walk = path::eat_path(
            &mut resolve,
            rd,
            wd,
            uid,
            gid,
            &mounts,
            &mut st.vnode_table,
            &mut |fs_e, dir_ino, root_ino, lk: &mut Lookup| {
                let mut scratch = path_scratch.borrow_mut();
                let bytes = lk.path.as_bytes();
                let n = bytes.len().min(path::PATH_MAX - 1);
                scratch[..n].copy_from_slice(&bytes[..n]);
                scratch[n] = 0;
                let grant = st
                    .grants
                    .grant_direct(
                        kernel,
                        fs_e.get(),
                        scratch[..].as_ptr() as u64,
                        (n + 1) as u64,
                        CpFlags::READ | CpFlags::WRITE,
                    )
                    .map_err(|_e| PathError::Inval)?;
                let mut msg =
                    crate::request::encode_lookup(grant, n + 1, dir_ino, root_ino, lk.flags.bits());
                fs_trans_stamp(&mut msg);
                let r = ipc.sendrec(fs_e, &mut msg);
                let _ = st.grants.revoke(grant);
                if let Err(t) = r {
                    last_errno.set(neg(t.0));
                    return Err(PathError::Inval);
                }
                // 回读 FS 写回的剩余路径——C 的 grant 就是 VFS 的 `l_path`
                // 本体，FS 写完 VFS 直接继续走（C request.c:494-499）。
                let end = scratch[..].iter().position(|&b| b == 0).unwrap_or(0);
                let status = fs_trans_status(&msg);
                match crate::request::decode_lookup_reply(status, &msg) {
                    Some(found) => {
                        if !matches!(found, LookupRes::Ok { .. })
                            && let Ok(tail) = core::str::from_utf8(&scratch[..end])
                        {
                            lk.path = tail.to_string();
                        }
                        Ok(found)
                    }
                    None => {
                        // 负 errno（或不可解的回复）——原始值留给上层。
                        last_errno.set(neg(status));
                        Err(PathError::Inval)
                    }
                }
            },
        );
        match walk {
            Ok(id) => id,
            Err(e) => {
                let preserved = last_errno.get();
                return Err(if preserved != 0 {
                    preserved
                } else {
                    e.to_errno()
                });
            }
        }
    };

    if st.vnode_table.get(vnode_id).is_none() {
        return Err(ExecError::Io.to_errno());
    }
    // 开卷成功后，本函数的一切失败都要先归还 vnode 引用（C 的
    // pm_execfinal 对 execi.vp 的 put_vnode，exec.c:392-395）。
    macro_rules! bail_open {
        ($errno:expr) => {{
            release_vnode(st, vnode_id);
            return Err($errno);
        }};
    }
    let v = st.vnode_table.get(vnode_id).expect("checked above");
    // 三检之一：非普通文件 → ENOEXEC（exec.c:126-127）。
    if (v.mode & crate::open::S_IFMT) != crate::open::S_IFREG {
        bail_open!(ExecError::NoExec.to_errno());
    }
    // 三检之二：执行权限（exec.c:128-129 `forbidden(fp, vp, X_BIT)`）。
    if let Err(e) = exec_forbid_check(v.mode, v.uid, v.gid, real_uid, real_gid, uid, gid) {
        bail_open!(e.to_errno());
    }

    // 三检之三：req_stat（exec.c:131-134）——失败即 EIO（`IdCheck::StatFailed`
    // 的错误面）。数据进 VFS 的 `sb`（execi.sb）；静态装载路只消费成败位
    // （尺寸走 vnode 的 v_size、凭证走 fproc），完整 stat 消费面随脚本/dyn
    // 波次接上。
    {
        let (fs_e, ino) = (v.fs, v.ino);
        let grant = st
            .grants
            .grant_magic(
                kernel,
                fs_e.get(),
                VFS_LOCAL_WHO_FROM.get(),
                hdr_buf.as_ptr() as u64,
                STAT_BUF_SIZE,
                CpFlags::WRITE,
            )
            .map_err(neg)?;
        let mut msg = crate::request::encode_stat(ino, grant);
        fs_trans_stamp(&mut msg);
        let r = ipc.sendrec(fs_e, &mut msg);
        let _ = st.grants.revoke(grant);
        match r {
            Err(t) => bail_open!(neg(t.0)),
            Ok(()) if fs_trans_status(&msg) != minix_types::OK => {
                bail_open!(neg(fs_trans_status(&msg)))
            }
            Ok(()) => {}
        }
    }

    // suid/sgid 提议（exec.c:137-147：`v_mode` 的 setuid/setgid 位 →
    // `args.new_uid/new_gid`；提议位随 PM_EXEC_NEW 发给 PM 裁决）。
    let v = st.vnode_table.get(vnode_id).expect("checked above");
    if v.mode & 0o4000 != 0 {
        *new_uid = v.uid;
    }
    if v.mode & 0o2000 != 0 {
        *new_gid = v.gid;
    }

    // 首块（`map_header`，exec.c:736-763）：min(v_size, 10 pages) 直读进
    // VFS 缓冲——`req_readwrite` 的 user 是 VFS 自己（exec.c:755 的
    // `VFS_PROC_NR`，即 [`VFS_LOCAL_WHO_FROM`] 具体端点，非 SELF 哨兵）。
    let hdr_len = (v.size as usize).min(hdr_buf.len());
    if let Err(e) = req_read(
        st,
        kernel,
        ipc,
        vnode_id,
        0,
        hdr_buf.as_ptr() as u64,
        hdr_len,
        VFS_LOCAL_WHO_FROM,
    ) {
        bail_open!(e);
    }
    Ok(vnode_id.0)
}

/// `req_read`（`req_readwrite_actual` 的 READING 半，request.c:834-873）：
/// magic grant + `REQ_READ` + revoke。`who_from` 按 C 的 user 参数化——
/// 首块读是 VFS 自己（exec.c:755，具体端点 [`VFS_LOCAL_WHO_FROM`]，B37：
/// 绝不能用 `Endpoint::SELF` 哨兵，否则内核按哨兵解析页表失败、FS 吞错、
/// 缓冲零字节而本函数仍回 `Ok`），段装载读是**目标进程**
/// （exec.c:709-711：FS 把文件字节直写目标的新地址空间，VFS 不中转）。
#[allow(clippy::too_many_arguments)]
fn req_read<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    vnode_id: VnodeId,
    off: u64,
    seg_addr: u64,
    seg_bytes: usize,
    who_from: Endpoint,
) -> Result<(), ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    if seg_bytes == 0 {
        return Ok(());
    }
    let (fs_e, ino, v_size) = {
        let v = st
            .vnode_table
            .get(vnode_id)
            .ok_or(ExecError::Io.to_errno())?;
        (v.fs, v.ino, v.size)
    };
    // exec.c:700-702 的两道门：LONG_MAX 界与文件界。
    if off.saturating_add(seg_bytes as u64) > exec::LONG_MAX_U64
        || v_size < off.saturating_add(seg_bytes as u64)
    {
        return Err(ExecError::Io.to_errno());
    }
    let grant = st
        .grants
        .grant_magic(
            kernel,
            fs_e.get(),
            who_from.get(),
            seg_addr,
            seg_bytes as u64,
            CpFlags::WRITE,
        )
        .map_err(neg)?;
    let mut msg = crate::request::encode_read(ino, grant, off as i64, seg_bytes);
    fs_trans_stamp(&mut msg);
    let r = ipc.sendrec(fs_e, &mut msg);
    let _ = st.grants.revoke(grant);
    match r {
        Err(t) => Err(neg(t.0)),
        Ok(()) if fs_trans_status(&msg) != minix_types::OK => Err(neg(fs_trans_status(&msg))),
        Ok(()) => Ok(()),
    }
}

/// ELF 段装载（`libexec_load_elf` 的 read_seg 路，exec_elf.c:197-306）。
///
/// 返回 `(text_size, data_size)` 记账（`args.text_size/data_size`；本波次
/// 无消费者，随 dyn/信号波次接）。
fn load_elf_segments<K, T>(
    kernel: &K,
    ipc: &T,
    vnodes: &VnodeTable,
    grants: &mut minix_sys::grant::GrantTable,
    vnode_id: VnodeId,
    target_e: Endpoint,
    elf: &ElfHead<'_>,
) -> Result<(u64, u64), ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    let (fs_e, ino, v_size) = {
        let v = vnodes.get(vnode_id).ok_or(ExecError::Io.to_errno())?;
        (v.fs, v.ino, v.size)
    };
    let mut first = true;
    let mut startv = 0u64;
    let mut text_size = 0u64;
    let mut data_size = 0u64;

    for i in 0..elf.phnum() {
        let (p_type, p_flags, p_offset, p_vaddr, p_filesz, p_memsz) = elf.phdr(i);
        if p_type != minix_elf::PT_LOAD || p_memsz == 0 {
            continue; // exec_elf.c:217
        }
        // 页算术（exec_elf.c:228-254）。
        let page_offset = p_vaddr % PAGE_SIZE;
        let vaddr = p_vaddr - page_offset;
        let seg_membytes = roundup(p_memsz + page_offset, PAGE_SIZE);
        let vfileend = p_vaddr + p_filesz;

        if first || startv > vaddr {
            startv = vaddr; // exec_elf.c:256-258 的 load_base 记录
        }
        first = false;
        if (p_flags & PF_X) != 0 && text_size < seg_membytes {
            text_size = seg_membytes;
        } else {
            data_size = seg_membytes;
        }

        // allocmem_prealloc_junk（exec_elf.c:273-282；回调 exec_general.c:22-31：
        // ANON|PREALLOC|UNINITIALIZED|FIXED，R|W|X；`minix_mmap_for` 的
        // forwhom = 目标进程）。
        vm_mmap(
            ipc,
            target_e,
            vaddr,
            seg_membytes,
            map_flags::ANON | map_flags::PREALLOC | map_flags::UNINITIALIZED | map_flags::FIXED,
            PROT_RWX,
        )?;

        // copymem = read_seg（exec_elf.c:285-287）：整段文件字节直写目标
        // 进程（exec.c:709-711）。read_seg 自己的两道门（exec.c:700-702）
        // 在此逐段生效。
        if p_offset.saturating_add(p_filesz) > exec::LONG_MAX_U64
            || v_size < p_offset.saturating_add(p_filesz)
        {
            return Err(ExecError::Io.to_errno());
        }
        let grant = grants
            .grant_magic(
                kernel,
                fs_e.get(),
                target_e.get(),
                p_vaddr,
                p_filesz,
                CpFlags::WRITE,
            )
            .map_err(neg)?;
        let mut msg = crate::request::encode_read(ino, grant, p_offset as i64, p_filesz as usize);
        fs_trans_stamp(&mut msg);
        let r = ipc.sendrec(fs_e, &mut msg);
        let _ = grants.revoke(grant);
        match r {
            Err(t) => return Err(neg(t.0)),
            Ok(()) if fs_trans_status(&msg) != minix_types::OK => {
                return Err(neg(fs_trans_status(&msg)));
            }
            Ok(()) => {}
        }

        // clearmem 头/尾间隙（exec_elf.c:294-309；`libexec_clear_sys_memset`
        // → sys_memset 内核调用，exec_general.c:66-71）。
        let vmemend = vaddr + seg_membytes;
        let head = p_vaddr - vaddr;
        if head > 0 {
            sys_memset(kernel, target_e.get(), 0, vaddr, head).map_err(neg)?;
        }
        let tail = vmemend - vfileend;
        if tail > 0 {
            sys_memset(kernel, target_e.get(), 0, vfileend, tail).map_err(neg)?;
        }
    }
    Ok((text_size, data_size))
}

/// `VM_MMAP` 的 ANON 分配（`minix_mmap_for` 的 VM_MMAP 腿；m_mmap 联合臂
/// 布局与 VM 侧 `VmMmapIn::decode_message` 对偶）。地址/长度走 NS5-A 的
/// 64 位车道——C 头是 i386 指针宽（56 字节断言在 LP64 必炸），minix-rs
/// 用户态是 LP64，栈顶 0x7fff_ffff_f000 一族只有 64 位车道装得下。
fn vm_mmap<T: IpcTransport>(
    ipc: &T,
    forwhom: Endpoint,
    addr: u64,
    len: u64,
    extra_flags: u32,
    prot: u32,
) -> Result<(), ExecErrno> {
    let mut msg = Message {
        m_type: minix_types::VM_MMAP as i32,
        ..Message::default()
    };
    {
        // SAFETY: m_mmap 是 VM_MMAP 的文档化载荷臂（VM 侧
        // `VmMmapIn::decode_message` 读同一臂；C ipc.h:1583-1593）。
        let m = unsafe { &mut msg.m_u.m_mmap };
        m.offset = 0;
        m.addr = addr;
        m.len = len;
        m.prot = prot as i32;
        // B38 修复（两个遗漏的必带位）：exec 为**目标子进程**建地址空间
        // 映射（段与栈），本 helper 手写 `m_mmap` 臂、`forwhom = target_e`。
        //   • `MAP_THIRDPARTY`（`0x800000`）：VM 仅按此位决定映射落进谁的
        //     地址空间（`os/servers/vm/src/mmap.rs:283-291` `target = if
        //     THIRDPARTY { forwhom } else { caller }`，`caller = m_source`
        //     即 VFS 自身）。缺此位会把子的段静默装进 VFS 自己、子仍空。
        //     C `minix_mmap_for`（`mmap.c:36-38`）在 `forwhom != SELF` 时必
        //     加此位；本路径 `forwhom` 恒为子（非 VFS），故无条件置。VFS 是
        //     execpriv（`mmap.rs:280`），THIRDPARTY 放行。
        //   • `MAP_PRIVATE`：minix-rs VM `is_valid`（`mmap.rs:88-92`，doc 20
        //     §3.6 收紧）要求 SHARED/PRIVATE 恰有其一，否则回 EINVAL。而
        //     `to_vr_flags`（`mmap.rs:102-126`）不消费 PRIVATE（新映射永不
        //     置 VR_SHARED，与 C `do_mmap` 同语义），故携 PRIVATE 产出的
        //     region 与 C 逐位一致（C 不传此位但也不校验）。
        // 真机 bn40/bn41 实锤：旧代码无 THIRDPARTY，携 PRIVATE 后 mmap 不再
        // EINVAL（过 is_valid）但仍落错对象→子空→`exec_via` 取指 noaddr→
        // PM↔VFS 乒乓活锁；补 THIRDPARTY 后方真正把镜像装入子。
        m.flags = (map_flags::ANON
            | map_flags::FIXED
            | map_flags::PRIVATE
            | map_flags::THIRDPARTY
            | extra_flags) as i32;
        m.fd = -1;
        m.forwhom = forwhom.get();
    }
    ipc.sendrec(Endpoint::VM, &mut msg).map_err(|t| neg(t.0))?;
    if msg.m_type != minix_types::OK {
        return Err(neg(msg.m_type));
    }
    Ok(())
}

/// `VM_PROCCTL` 的 `VMPPARAM_CLEAR`（exec_elf.c:194 的 clearproc 回调
/// `libexec_clearproc_vm_procctl`；param=1，exit.c:129-137——RS/VFS 调用者
/// 白名单在 VM 侧，os/servers/vm/src/ipc/dispatcher.rs:1674）。
fn vm_procctl_clear<T: IpcTransport>(ipc: &T, who: Endpoint) -> Result<(), ExecErrno> {
    let mut msg = Message {
        m_type: minix_types::VM_PROCCTL as i32,
        ..Message::default()
    };
    {
        // SAFETY: m_lc_vm_procctl 是 VM_PROCCTL 的文档化载荷臂（VM 侧
        // `VmProcctlIn::decode_message` 读同一臂；com.h:753-757）。
        let p = unsafe { &mut msg.m_u.m_lc_vm_procctl };
        p.ull1 = 0;
        p.ull2 = 0;
        p.param = 1; // VMPPARAM_CLEAR
        p.who = who.get();
        p.m1 = 0;
        p.len = 0;
        p.flags = 0;
    }
    ipc.sendrec(Endpoint::VM, &mut msg).map_err(|t| neg(t.0))?;
    if msg.m_type != minix_types::OK {
        return Err(neg(msg.m_type));
    }
    Ok(())
}

/// `libexec_pm_newexec`（exec_general.c:78-90）：PM_EXEC_NEW sendrec。
///
/// `exec_info` 的 192 字节线格式（libexec.h:24-58，LP64；PM 侧解码即
/// `EXEC_INFO_COPY_SIZE` 与域偏移——proc_e@0/hdr@8/hdr_len@16/frame_len@24/
/// progname@32(16)/new_uid@48/new_gid@52/allow_setuid@56/stack_size@64/
/// load_offset@72/text_size@80/data_size@88/filesize@96/…/stack_high@184）。
/// PM 只拷标量、不间接引用 `hdr` 指针（do_newexec 只读
/// progname/uid/gid/allow_setuid/stack_high/frame_len）。回程 `m_type` 是
/// 结果码，载荷 suid@0 即裁决后的 `allow_setuid`（pm/exec.c:120-122）。
#[allow(clippy::too_many_arguments)]
fn pm_newexec<T: IpcTransport>(
    ipc: &T,
    target_e: Endpoint,
    hdr_addr: u64,
    hdr_len: usize,
    progname: &str,
    new_uid: u32,
    new_gid: u32,
    suid_proposal: bool,
    stack_high: u64,
    stack_size: u64,
    frame_len: usize,
) -> Result<bool, ExecErrno> {
    let mut info = alloc::boxed::Box::new([0u8; 192]);
    info[0..4].copy_from_slice(&target_e.get().to_le_bytes()); // proc_e@0
    info[8..16].copy_from_slice(&hdr_addr.to_le_bytes()); // hdr@8（exec.c:745）
    info[16..24].copy_from_slice(&(hdr_len as u64).to_le_bytes()); // hdr_len@16（exec.c:748-750）
    info[24..32].copy_from_slice(&(frame_len as u64).to_le_bytes()); // frame_len@24
    {
        // progname@32（PROC_NAME_LEN=16，C strncpy 语义：填到满或 NUL 止）
        let n = progname.len().min(16);
        info[32..32 + n].copy_from_slice(&progname.as_bytes()[..n]);
    }
    info[48..52].copy_from_slice(&new_uid.to_le_bytes()); // new_uid@48
    info[52..56].copy_from_slice(&new_gid.to_le_bytes()); // new_gid@52
    info[56..60].copy_from_slice(&(i32::from(suid_proposal)).to_le_bytes()); // allow_setuid@56（VFS 提议）
    info[64..72].copy_from_slice(&stack_size.to_le_bytes()); // stack_size@64
    info[184..192].copy_from_slice(&stack_high.to_le_bytes()); // stack_high@184

    let mut msg = Message {
        m_type: minix_types::PM_EXEC_NEW,
        ..Message::default()
    };
    {
        // SAFETY: m_lexec_pm_exec_new 的 endpt@0/ptr@8（ipc.h:966-973；
        // PM 侧 decode::exec_new 读同域）。
        let raw = unsafe { &mut msg.m_u.raw };
        raw[0..4].copy_from_slice(&target_e.get().to_le_bytes());
        raw[8..16].copy_from_slice(&(info.as_ptr() as u64).to_le_bytes());
    }
    // sendrec 的语义保证 PM 在回复前完成对 `info` 的 192 字节拷贝
    // （do_newexec:82-84 的 sys_datacopy 在应答之前），缓冲活到本函数
    // 返回即可，无需静态存储。
    let r = ipc.sendrec(Endpoint::PM, &mut msg).map_err(|t| neg(t.0));
    drop(info);
    r?;
    if msg.m_type != minix_types::OK {
        return Err(neg(msg.m_type));
    }
    // SAFETY: 回程载荷 suid@0（pm/exec.c:120-122）按 raw 字节读。
    let suid = unsafe { i32::from_le_bytes(msg.m_u.raw[0..4].try_into().unwrap()) };
    Ok(suid != 0)
}

// ════════════════════════════════════════════════════════════════════
// 测试（NS5）——分层：① ELF 头解析纯件；② 段装载/PM 通告/VM 清除的
// 线形状（canned 传输逐条断言）；③ `pm_exec` 编排的开卷段（lookup/
// stat/read 三步舞 + 失败路径 + vnode 归还）。FS 数据背流（FS 把文件
// 字节写进 grant 指向的 VFS 缓冲）在宿主上不可达（canned 传输不搬
// 字节），与全仓 FS 数据路径测试同一边界，按线形状+分段驱动覆盖。
// ════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use crate::main_loop::VfsState;
    use alloc::format;

    /// B33b 调用点回归 pin：`exec_forbid_check` 的 `access` 必须是低 3 位
    /// `X_BIT`。既有单测都绕过三检门直调 `load_elf_segments`，从不驱动这段
    /// `ForbidInput` 构造；本测直接驱动它，跨 root/owner/其他档验证“执行
    /// 意图”。若把内部 `access` 改回 9 位模式位 `0o100`，root/owner 执行
    /// `0o755` 会被恒误拒，本测立即变红（真机缺陷为 mode=0o100755、euid=0）。
    #[test]
    fn test_exec_forbid_check_uses_low_three_bit_access() {
        // root 执行自己 owned 的 0o755（真机场景）：应放行。
        assert_eq!(
            exec_forbid_check(0o100755, 0, 0, 0, 0, crate::link::SU_UID, 0),
            Ok(())
        );
        // 非 root owner 执行自己 0o755（owner 档 shift=6 → perm=0o7）：应放行。
        assert_eq!(exec_forbid_check(0o755, 100, 100, 100, 100, 100, 100), Ok(()));
        // 非 root owner 执行无可执行位的 0o644（perm=0o6，X 不属包含）：应拒。
        assert_eq!(
            exec_forbid_check(0o644, 100, 100, 100, 100, 100, 100),
            Err(crate::protect::ProtectError::Acces)
        );
    }

    /// 最小 ELF64 镜像头（Ehdr 64B + phdr 表 56B/项，全部落首扇区）。
    fn make_test_elf(entry: u64, phdrs: &[(u32, u32, u64, u64, u64, u64)]) -> Vec<u8> {
        let mut e = vec![0u8; 64 + phdrs.len() * 56];
        e[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        e[4] = 2; // ELFCLASS64
        e[5] = 1; // ELFDATA2LSB
        e[6] = 1; // EV_CURRENT
        e[16..18].copy_from_slice(&minix_elf::ET_EXEC.to_le_bytes()); // e_type
        e[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
        e[24..32].copy_from_slice(&entry.to_le_bytes()); // e_entry
        e[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        e[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        e[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes()); // e_phnum
        for (i, (ty, flags, off, vaddr, filesz, memsz)) in phdrs.iter().enumerate() {
            let at = 64 + i * 56;
            e[at..at + 4].copy_from_slice(&ty.to_le_bytes());
            e[at + 4..at + 8].copy_from_slice(&flags.to_le_bytes());
            e[at + 8..at + 16].copy_from_slice(&off.to_le_bytes());
            e[at + 16..at + 24].copy_from_slice(&vaddr.to_le_bytes());
            e[at + 32..at + 40].copy_from_slice(&filesz.to_le_bytes());
            e[at + 40..at + 48].copy_from_slice(&memsz.to_le_bytes());
        }
        e
    }

    /// 线形状①：Ehdr 解析的收与拒（`check_header`/`elf_sane` 合并语义）。
    #[test]
    fn test_elfhead_parse_accepts_static_and_rejects_broken() {
        let good = make_test_elf(
            0x401_000,
            &[(minix_elf::PT_LOAD, PF_X, 0x1000, 0x401_000, 0x200, 0x200)],
        );
        let head = ElfHead::parse(&good).expect("static ELF must parse");
        assert_eq!(head.entry(), 0x401_000);
        assert_eq!(head.phnum(), 1);
        assert!(!head.has_interpreter());

        // 坏魔数。
        let mut bad = good.clone();
        bad[0] = b'M';
        assert!(ElfHead::parse(&bad).is_err());
        // phdr 表越过加载缓冲（本例 120B，e_phoff=600 在外）→ 拒。
        let mut far = good.clone();
        far[32..40].copy_from_slice(&600u64.to_le_bytes());
        assert!(ElfHead::parse(&far).is_err());
        // PT_INTERP 在场 = 动态链接（本波次拒收面）。
        let dynimg = make_test_elf(0x1000, &[(PT_INTERP, 0, 0x200, 0, 0x10, 0)]);
        let dhead = ElfHead::parse(&dynimg).expect("dyn header parses");
        assert!(dhead.has_interpreter());
        // ET_DYN 与 ET_EXEC 同收（elf_sane）。
        let mut et_dyn = good.clone();
        et_dyn[16..18].copy_from_slice(&ET_DYN.to_le_bytes());
        assert!(ElfHead::parse(&et_dyn).is_ok());
        // 截断镜像。
        assert!(ElfHead::parse(&good[..32]).is_err());
    }

    /// B37b 回归 pin：LP64 的 10 项 phdr 表（`e_phoff` 64 + 10×56 = 624 > 512）
    /// 必须能 parse——真实 `/bin/sh` 首块（`map_header` 加载 10 页 = 40960
    /// 缓冲）装得下；旧 `SECTOR_SIZE`（512）界是 32 位时代遗留（`Elf32_Phdr`
    /// 32B），会把合法 LP64 头误判 ENOEXEC（真机 bn37p 实锤：首块字节已落入
    /// `7f454c46` 后仍 ENOEXEC 的那条腿）。新界 = 实际加载的头缓冲长。
    #[test]
    fn test_elfhead_parse_lp64_phdr_table_beyond_sector_size() {
        let phdrs: Vec<(u32, u32, u64, u64, u64, u64)> = (0..10)
            .map(|i| {
                (
                    minix_elf::PT_LOAD,
                    PF_X,
                    0x1000 + i as u64 * 0x100,
                    0x401_000 + i as u64 * 0x100,
                    0x100,
                    0x100,
                )
            })
            .collect();
        let img = make_test_elf(0x401_000, &phdrs);
        assert_eq!(img.len(), 64 + 10 * 56, "10 项 LP64 phdr 表 = 624B > 旧 512 界");
        let head = ElfHead::parse(&img)
            .expect("LP64 10 项 phdr 表必须 parse（超 512 但 ≤ 缓冲长）");
        assert_eq!(head.phnum(), 10);
        // 真越界（e_phoff 顶到加载缓冲之外）仍须拒。
        let mut oob = img.clone();
        oob[32..40].copy_from_slice(&2000u64.to_le_bytes());
        assert!(ElfHead::parse(&oob).is_err(), "phdr 表越过加载缓冲界必须拒");
    }

    /// 脚本化 IPC：按对端 m_type 落 canned 回复，并记录全部 sendrec。
    struct ExecScriptedIpc {
        sent: core::cell::RefCell<Vec<(Endpoint, i32)>>,
        /// VM 端 sendrec 的完整消息快照（NS5-A 车道 pin 用）。
        vm_sent: core::cell::RefCell<Vec<Message>>,
    }

    impl ExecScriptedIpc {
        fn new() -> Self {
            Self {
                sent: core::cell::RefCell::new(Vec::new()),
                vm_sent: core::cell::RefCell::new(Vec::new()),
            }
        }
        fn types_of(&self, e: Endpoint) -> Vec<i32> {
            self.sent
                .borrow()
                .iter()
                .filter(|(pe, _)| *pe == e)
                .map(|(_, t)| *t)
                .collect()
        }
    }

    impl minix_sys::ipc::IpcTransport for ExecScriptedIpc {
        fn send(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn receive(
            &self,
            _s: Endpoint,
            _m: &mut Message,
        ) -> Result<minix_sys::ipc::IpcStatus, minix_sys::ipc::TrapStatus> {
            unimplemented!("exec 测试不收消息")
        }
        fn sendrec(&self, d: Endpoint, m: &mut Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            let ty = m.m_type;
            if d == Endpoint::VM {
                self.vm_sent.borrow_mut().push(*m);
            }
            self.sent.borrow_mut().push((d, ty));
            m.m_type = match (d, ty) {
                // VM 两路：清除与分配都成功。
                (Endpoint::VM, _) => minix_types::OK,
                // PM_EXEC_NEW：成功回 OK（suid=0，不换凭证）。
                (Endpoint::PM, minix_types::PM_EXEC_NEW) => {
                    // 回程载荷 suid@0（pm/exec.c:120-122）——清零＝不换凭证。
                    // SAFETY: 测试注入 canned 回复载荷。
                    unsafe {
                        m.m_u.raw[0..4].copy_from_slice(&0i32.to_le_bytes());
                    }
                    minix_types::OK
                }
                // FS 三路（lookup/stat/read）都成功。
                (Endpoint::MFS, _) => minix_types::OK,
                _ => minix_types::OK,
            };
            Ok(())
        }
        fn notify(&self, _d: Endpoint) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn sendnb(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn senda(
            &self,
            _table: &[minix_sys::ipc::AsyncSlot],
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn query_kerninfo_page(&self) -> Result<u64, minix_sys::ipc::TrapStatus> {
            Err(minix_sys::ipc::TrapStatus(1))
        }
    }

    /// 线形状②：段装载路——VM_MMAP 分配 + REQ_READ 直写目标 +
    /// 头/尾 clearmem（sys_memset）逐条对上（exec_elf.c:197-306）。
    #[test]
    fn test_load_elf_segment_wires() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        // 目标进程的 exec 卷 vnode（mfs、普通文件、尺寸盖过段）。
        let vid = state.vnode_table.alloc().unwrap();
        {
            let vn = state.vnode_table.get_mut(vid).unwrap();
            vn.fs = Endpoint::MFS;
            vn.ino = 42;
            vn.dev = 1;
            vn.mode = crate::open::S_IFREG | 0o755;
            vn.size = 0x2000;
            vn.ref_count = 1;
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let elfimg = make_test_elf(
            0x401_000,
            &[(
                minix_elf::PT_LOAD,
                PF_X | 0x2,
                0x1000,
                0x401_000,
                0x100,
                0x300,
            )],
        );
        let elf = ElfHead::parse(&elfimg).unwrap();
        let grants = &mut state.grants;
        let vnodes: &VnodeTable = &state.vnode_table;
        let (text, data) =
            load_elf_segments(&kernel, &ipc, vnodes, grants, vid, Endpoint::INIT, &elf)
                .expect("segment load must succeed");
        // PF_X 段记 text，无 data 段 → data_size 0（exec_elf.c:289-293）。
        assert_eq!(text, PAGE_SIZE); // roundup(memsz 0x300, 4096)
        assert_eq!(data, 0);
        // VM_MMAP 一条：forwhom=目标、页对齐 vaddr、ANON|PREALLOC|UNINIT|FIXED。
        let vm_types = ipc.types_of(Endpoint::VM);
        assert_eq!(vm_types, vec![minix_types::VM_MMAP as i32]);
        // B38 回归 pin：exec 段映射为**子进程**建（非 VFS 自身），
        // VM_MMAP 必同时带 MAP_THIRDPARTY（否则 VM 把段静默装进 caller=VFS）
        // 与 MAP_PRIVATE（否则被 VM `is_valid` 拒 EINVAL）。真机 bn40/bn41
        // 实锤这两个遗漏位共同导致子地址空间为空→`exec_via` noaddr 活锁。
        {
            let vm_msgs = ipc.vm_sent.borrow();
            let mmap_flags = unsafe { vm_msgs[0].m_u.m_mmap.flags };
            assert_eq!(
                mmap_flags,
                (map_flags::ANON
                    | map_flags::FIXED
                    | map_flags::PRIVATE
                    | map_flags::THIRDPARTY
                    | map_flags::PREALLOC
                    | map_flags::UNINITIALIZED) as i32,
            );
            // 映射受益人必须是目标子（INIT），不是 VFS 自己。
            let forwhom = unsafe { vm_msgs[0].m_u.m_mmap.forwhom };
            assert_eq!(forwhom, Endpoint::INIT.get());
        }
        // REQ_READ 一条：FS 写目标进程内存（REQ_READ @ inode 42）。
        // 出向 m_type 已按 B32 修复做事务打包（TRNS_ADD_ID：REQ 号
        // 进高 16、transid 进低 16）——用真机线形状常量
        // `trns_add_id(REQ_READ, 0)` pin 住形状，回归裸发（m_type=REQ_READ）
        // 即测红。
        let fs_types = ipc.types_of(Endpoint::MFS);
        assert_eq!(fs_types, vec![minix_types::trns_add_id(minix_types::REQ_READ, 0)]);
        // clearmem：页内头隙 0、尾隙 4096-0x100=0xF00 → 恰 1 次 sys_memset。
        let memsets = kernel
            .sent
            .borrow()
            .iter()
            .filter(|m| m.m_type == minix_types::SYS_MEMSET)
            .count();
        assert_eq!(memsets, 1, "tail clear only (vaddr page-aligned)");
    }

    /// B37 回归 pin：exec 首块读 / stat 的 VFS-local magic grant 用**具体
    /// 端点** [`VFS_LOCAL_WHO_FROM`]（= `Endpoint::VFS`）作 `who_from`，绝不能
    /// 用 `Endpoint::SELF` 哨兵——内核把 `magic.who_from` 原样装为
    /// `effective_granter`（`kernel/src/grant.rs:461`）解析页表；SELF 非合法
    /// 端点（`is_valid() == false`）→ 拷贝失败被 FS 吞（transport.rs:272）
    /// → hdr_buf 零字节而 `req_read` 仍回 `Ok`（真机 bn34p 实锤，翻案前
    /// 误疑 phdr 表超 512）。
    #[test]
    fn test_vfs_local_who_from_is_concrete_endpoint() {
        assert_eq!(VFS_LOCAL_WHO_FROM, Endpoint::VFS);
        assert!(
            VFS_LOCAL_WHO_FROM.is_valid(),
            "VFS-local grant 的 who_from 必须是具体端点，否则内核页表解析失败"
        );
        assert!(
            !Endpoint::SELF.is_valid(),
            "SELF 哨兵不可作 magic grant who_from（B37 回归即此）"
        );
    }

    /// 驱动 `req_read` 首块读腿（VFS-local）：seeded vnode + 具体 `who_from`
    /// 应恰发一条打包后的 `REQ_READ` 到 MFS 并回 `Ok`（canned 传输不搬字
    /// 节，故只 pin 线形状与不报错；数据背流由真机双跑验证）。
    #[test]
    fn test_req_read_first_block_wires_with_concrete_who_from() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        let vid = state.vnode_table.alloc().unwrap();
        {
            let vn = state.vnode_table.get_mut(vid).unwrap();
            vn.fs = Endpoint::MFS;
            vn.ino = 8;
            vn.dev = 1;
            vn.mode = crate::open::S_IFREG | 0o755;
            vn.size = 0x2000;
            vn.ref_count = 1;
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let mut buf = [0u8; 512];
        let r = req_read(
            &mut state,
            &kernel,
            &ipc,
            vid,
            0,
            buf.as_mut_ptr() as u64,
            buf.len(),
            VFS_LOCAL_WHO_FROM,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(
            ipc.types_of(Endpoint::MFS),
            vec![minix_types::trns_add_id(minix_types::REQ_READ, 0)],
            "首块读应恰发一条打包后的 REQ_READ 到 MFS"
        );
    }

    /// B32 回归 pin：VFS→FS 事务打包往返。出向 `fs_trans_stamp` 把
    /// REQ 号移到高 16（FS 按 `m_type >> 16` 路由）、低 16 落 0（同步腿
    /// 自匹配、不占 worker transid 空间）；入向 `fs_trans_status` 从高 16
    /// 取真实 status。真机缺陷形态是 status 字段被误当成裸 errno——
    /// 旧代码直接读 `msg.m_type`（=0x4E0A1A）会折出 5114394 的伪 errno；
    /// 修复后应只看到高 16 的真实 status（=78）。
    #[test]
    fn test_fs_transaction_pack_roundtrip() {
        // 出向：REQ_LOOKUP 移到高 16，低 16 = 0（非 worker transid 空间）。
        let mut req = Message {
            m_type: minix_types::REQ_LOOKUP,
            ..Message::default()
        };
        fs_trans_stamp(&mut req);
        assert_eq!(
            minix_types::trns_del_id(req.m_type),
            minix_types::REQ_LOOKUP,
            "FS 路由用的 call 必须在高 16"
        );
        assert_eq!(
            minix_types::trns_get_id(req.m_type),
            0,
            "同步腿 transid 取 0，不能落入 VFS_TRANSID+slot 的 worker 号空间"
        );
        // 入向：模拟 FS 成功回复（status=0）。
        let ok = Message {
            m_type: minix_types::trns_add_id(0, 0),
            ..Message::default()
        };
        assert_eq!(fs_trans_status(&ok), minix_types::OK);
        // 入向：模拟真机伪值 0x4E0A1A（=(78<<16)|REQ_LOOKUP）——
        // 提取出的 status = 78（而非旧代码的裸值 5114394）。
        let bogus = Message {
            m_type: 0x4E0A_1A,
            ..Message::default()
        };
        assert_eq!(fs_trans_status(&bogus), 0x4E);
        assert_eq!(neg(fs_trans_status(&bogus)), -0x4E, "折成负 errno 而非指针值");
    }

    /// NS5-A 车道 pin：栈映射的 VM_MMAP 请求在 64 位车道上承载 >4 GiB
    /// 栈顶，且 VM 侧 `VmMmapIn::decode_message`（真 wire 上 VM 跑的同一
    /// 解码）原样取回——u32 臂时代这里静默截断（T2 真机阻塞位，已修）。
    #[test]
    fn test_vm_mmap_stack_lanes_carry_above_4gib() {
        let ipc = ExecScriptedIpc::new();
        // 缺省栈顶 0x7fff_ffff_f000 − DEFAULT_STACK_LIMIT(4 MiB)，页整。
        let stack_low = 0x7fff_ffff_f000 - exec::DEFAULT_STACK_LIMIT;
        assert_eq!(stack_low % PAGE_SIZE, 0);
        assert!(stack_low > 4 * 1024 * 1024 * 1024);
        vm_mmap(
            &ipc,
            Endpoint(5),
            stack_low,
            exec::DEFAULT_STACK_LIMIT,
            0,
            PROT_RWX,
        )
        .expect("stack mmap succeeds on canned VM");
        let sent = ipc.vm_sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, minix_types::VM_MMAP as i32);
        // 发送臂写哪条车道，decode_message（VM 真机同一解码）就从哪条读。
        let req = minix_types::VmMmapIn::decode_message(&sent[0]);
        assert_eq!(req.addr.0, stack_low);
        assert_eq!(req.length.0, exec::DEFAULT_STACK_LIMIT);
        assert_eq!(req.forwhom, Endpoint(5));
        assert_eq!(req.prot, PROT_RWX);
        // B38：helper 统一补齐 `MAP_PRIVATE`（过 is_valid）与
        // `MAP_THIRDPARTY`（映射落到 forwhom=目标而非 caller）。车道
        // 断言同步包含两位。
        assert_eq!(
            req.flags,
            map_flags::ANON | map_flags::FIXED | map_flags::PRIVATE | map_flags::THIRDPARTY
        );
        assert_eq!(req.fd, -1);
        // 截断回归钉：旧 u32 臂会把请求写坏成低 32 位。
        assert_ne!((stack_low as u32) as u64, stack_low);
    }

    /// 线形状②：PM_EXEC_NEW 载荷与裁决回程（exec_general.c:78-90）。
    #[test]
    fn test_pm_newexec_wire_and_verdict() {
        let ipc = ExecScriptedIpc::new();
        let allow = pm_newexec(
            &ipc,
            Endpoint(5),
            0xdead,
            4096,
            "sh",
            1000,
            100,
            true,
            0x7fff_ffff_f000,
            4 * 1024 * 1024,
            256,
        )
        .expect("pm_newexec succeeds");
        assert!(!allow, "scripted PM replies suid=0");
        let types = ipc.types_of(Endpoint::PM);
        assert_eq!(types, vec![minix_types::PM_EXEC_NEW]);
    }

    /// 编排③：`pm_exec` 的开卷段——lookup/stat/read 三步舞逐条落在
    /// MFS 上，首块全零 → ENOEXEC（无装载器肯收），vnode 引用归还进
    /// pending_puts（C pm_execfinal 的 put_vnode 半）。
    #[test]
    fn test_pm_exec_open_phase_wires_and_bail() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        // 目标进程槽（endpoint 5 → slot 5）。
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(5))
                .expect("slot exists");
            fp.pid = 500;
            fp.endpoint = Endpoint(5);
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(5),
            path_addr: 0x7000, // canned 内核不搬字节；路径来自进程空间——
            // 宿主 datacopy 不可达，见下方路径来源注记。
            path_len: 0,
            frame_addr: 0x8000,
            frame_len: 16,
            ps_str: 0x1234,
            stack_high: 0x7fff_ffff_f000,
        };
        // fetch_name 的 sys_datacopy 在宿主 canned 内核下拿到的路径缓冲
        // 是零填充 → 空路径 → 诚实拒（这一步就断开，三步舞的线形状由
        // lookup 缝测试另行覆盖——见 test_lookup_closure_wire）。
        let r = pm_exec(&mut state, &kernel, &ipc, req);
        // 取名门（path.rs fetch 同约定：空路径 EINVAL）先断——VM/PM 无流量。
        assert_eq!(r, Err(neg(minix_types::EINVAL)));
        assert!(ipc.types_of(Endpoint::VM).is_empty());
        assert!(ipc.types_of(Endpoint::PM).is_empty());
    }

    /// 编排③：帧上限门（exec.c:227-229 的 ARG_MAX 门在一切往返之前）。
    #[test]
    fn test_pm_exec_frame_gate_enomem() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(5))
                .expect("slot exists");
            fp.pid = 500;
            fp.endpoint = Endpoint(5);
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(5),
            path_addr: 0,
            path_len: 0,
            frame_addr: 0,
            frame_len: exec::ARG_MAX + 1,
            ps_str: 0,
            stack_high: 0x7fff_ffff_f000,
        };
        assert_eq!(
            pm_exec(&mut state, &kernel, &ipc, req),
            Err(neg(minix_types::ENOMEM))
        );
        assert!(ipc.types_of(Endpoint::VM).is_empty());
        assert!(ipc.types_of(Endpoint::PM).is_empty());
    }

    /// 编排③：坏目标槽（无 fproc 行）诚实拒，零线上流量。
    #[test]
    fn test_pm_exec_rejects_dead_target() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(7), // slot 7 无 fproc 行
            path_addr: 0,
            path_len: 0,
            frame_addr: 0,
            frame_len: 8,
            ps_str: 0,
            stack_high: 0x7fff_ffff_f000,
        };
        // 死目标走 `enoexec` 裸正腿（inner 体），公共边界 `pm_exec` 折负（B33a）：
        // 断言 -ENOEXEC（原固化成 +8 的泄漏已消除）。
        assert_eq!(
            pm_exec(&mut state, &kernel, &ipc, req),
            Err(-ExecError::NoExec.to_errno())
        );
        assert!(ipc.types_of(Endpoint::VM).is_empty());
    }

    /// B33a 回归：inner 体的正值静态腿（`enoexec`/`ExecError::*::to_errno()`）
    /// 经公共 `pm_exec` 边界必折成负 errno——否则真 errno 会被 init 侧
    /// `exec_via` 的 `Ok(_) => EIO` 成功车道吞成 EIO，永远浮不出来。
    #[test]
    fn test_pm_exec_boundary_folds_positive_errno_negative() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(7), // 无 fproc 行 → inner 泄漏 +ENOEXEC
            path_addr: 0,
            path_len: 0,
            frame_addr: 0,
            frame_len: 8,
            ps_str: 0,
            stack_high: 0x7fff_ffff_f000,
        };
        match pm_exec(&mut state, &kernel, &ipc, req) {
            // 折叠点前 inner 返 +8（真机 bn28p 实测），折叠后必为负。
            Err(errno) => assert!(
                errno < 0,
                "exec 失败必须以负 errno 直达 PM，实得 {errno}"
            ),
            Ok(_) => panic!("死目标应失败"),
        }
    }
}
