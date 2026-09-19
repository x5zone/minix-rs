//! RS 服务表快照 —— `SI_PROCPUB_TAB` / `SI_PROC_TAB` 的 wire 权威
//! （`[ARCH: A-4]` 单一权威）。
//!
//! C ground truth: `minix3/minix/servers/rs/type.h:56-108`（`rproc[NR_SYS_PROCS]`
//! 与它的 `r_pub` 公共半 `struct rprocpub`，rs.h:165-184）。快照只带**使用
//! 字段子集**（C 声明序，`#[repr(C)]`）——与 `FProcSnap`/`MProcSnap`/
//! `DsEntrySnap` 同一裁定：跨 wire 无意义的指针（`r_exec`/`r_argv`）、
//! PCI ACL 细节（只有 devman 用，且走 RS 自己的接口）、`struct priv`
//! （内核运行时状态）都不进快照。
//!
//! 两个消费方各有半边需求，快照取并集（`ProcInfoStruct` 的多读者先例）：
//! - IS `dmp_rs`：`label`/`endpoint`/`dev_nr`/`sys_flags`（PUB 表），
//!   `r_pid`/`r_restarts`/`r_flags`/`r_period`/`r_alive_tm`/`r_args`（PRIV 表）；
//! - VM 的 `RS_INIT` 握手：`in_use`/`endpoint`/`vm_call_mask`（公共表按
//!   `rproctab_gid` 授权读出，`main.c:244-255`）。
//!
//! 生产者（`os/servers/rs/src/shell_request.rs` 的三个 copy-out）按本结构
//! 逐槽序列化；C 的逐行 `struct` 字节镜像（`rprocpub_off`/`rproc_off`
//! 偏移表）随之退役。

/// C `RS_MAX_LABEL_LEN 16` — rs/const.h。
pub const RS_MAX_LABEL_LEN: usize = 16;
/// C `MAX_COMMAND_LEN 512` — rs/const.h:18（`r_args` 的宽度）。
pub const RS_MAX_COMMAND: usize = 512;
/// C `NR_SYS_PROCS 64`（sys_config.h:9）——`rproc[NR_SYS_PROCS]` 的槽数。
pub const RS_TABLE_LEN: usize = 64;

/// 公共表行快照（`struct rprocpub` 的使用字段子集）。
///
/// 同一条形服务两个读者：IS 打印身份列（label/endpoint/dev_nr/sys_flags），
/// VM 在 `RS_INIT` 握手里读 `in_use`/`endpoint`/`vm_call_mask` 建服务 ACL。
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RprocpubSnap {
    /// C: `in_use` (rs.h:166)。
    pub in_use: i32,
    /// C: `sys_flags` (rs.h:167)。
    pub sys_flags: u32,
    /// C: `endpoint` (rs.h:168)。
    pub endpoint: i32,
    /// C: `dev_nr` (rs.h:172；主设备号或 `NO_DEV`)。
    pub dev_nr: i32,
    /// C: `label[RS_MAX_LABEL_LEN]` (rs.h:176)。
    pub label: [u8; RS_MAX_LABEL_LEN],
    /// C: `vm_call_mask[VM_CALL_MASK_SIZE]`——两个 32 位块合成位掩码
    /// （rs.h:179；位 *i* 即调用号 *i*，与 C 的分块小端布局同位）。
    pub vm_call_mask: u64,
}

/// 私有表行快照（`struct rproc` 的使用字段子集；`r_args` 是尾列 `%s`）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RprocSnap {
    /// C: `r_pid` (type.h:63)。
    pub r_pid: i32,
    /// C: `r_restarts` (type.h:66)。
    pub r_restarts: i32,
    /// C: `r_flags` (type.h:68；`RS_IN_USE 0x001` 为在用判据)。
    pub r_flags: u32,
    /// C: `r_period` (type.h:71，`int` 心跳周期)。
    pub r_period: i32,
    /// C: `r_alive_tm` (type.h:73，`clock_t`——树内 `Clock = i64`)。
    pub r_alive_tm: i64,
    /// C: `r_args[MAX_COMMAND_LEN]` (type.h:79)——NUL 分隔的原始命令行；
    /// `rproc_dmp` 把它作尾列逐字节打印（dmp_rs.c:64）。
    pub r_args: [u8; RS_MAX_COMMAND],
}

impl Default for RprocSnap {
    // Manual: `[u8; 512]` has no `Default` — all-zero bytes are the C BSS
    // initialiser (vacant rows render all-zero).
    fn default() -> Self {
        Self {
            r_pid: 0,
            r_restarts: 0,
            r_flags: 0,
            r_period: 0,
            r_alive_tm: 0,
            r_args: [0; RS_MAX_COMMAND],
        }
    }
}

#[cfg(test)]
mod rs_snap_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// 布局见证：PUB 行 5 个 4 字节域 + label(16) + 8 字节掩码 = 40；
    /// PRIV 行 4 个 4 字节域 + `Clock`(i64, 8 对齐) + args(512) = 536。
    #[test]
    fn test_row_layouts() {
        assert_eq!(size_of::<RprocpubSnap>(), 40);
        assert_eq!(offset_of!(RprocpubSnap, in_use), 0);
        assert_eq!(offset_of!(RprocpubSnap, sys_flags), 4);
        assert_eq!(offset_of!(RprocpubSnap, endpoint), 8);
        assert_eq!(offset_of!(RprocpubSnap, dev_nr), 12);
        assert_eq!(offset_of!(RprocpubSnap, label), 16);
        assert_eq!(offset_of!(RprocpubSnap, vm_call_mask), 32);

        assert_eq!(size_of::<RprocSnap>(), 536);
        assert_eq!(offset_of!(RprocSnap, r_pid), 0);
        assert_eq!(offset_of!(RprocSnap, r_alive_tm), 16);
        assert_eq!(offset_of!(RprocSnap, r_args), 24);
    }

    /// 表宽度 = 槽数 × 行宽（生产者与消费者的同一算式：IS 的
    /// `SI_PROCPUB_TAB` 与 `SI_PROC_TAB` 请求尺寸、`SI_PROCALL_TAB` 的两段和）。
    #[test]
    fn test_table_widths() {
        assert_eq!(RS_TABLE_LEN, 64);
        assert_eq!(size_of::<RprocpubSnap>() * RS_TABLE_LEN, 2_560);
        assert_eq!(size_of::<RprocSnap>() * RS_TABLE_LEN, 34_304);
        assert_eq!(
            size_of::<RprocSnap>() * RS_TABLE_LEN + size_of::<RprocpubSnap>() * RS_TABLE_LEN,
            36_864
        );
    }
}
