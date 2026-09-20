//! 装机清单（E-IMGPKG / new_edge3 NS8）：boot image 的 12 个用户态模块
//! 分别来自哪个 workspace 包。
//!
//! 消费契约在 boot-shim 一侧：`os/boot-shim/src/loader.rs` 的
//! `MODULE_NAMES` 逐名读 `/EFI/minix/modules/<名>`，缺件即 panic
//! （NK4 引入的 fail-fast 语义）。本清单是装机面的**白名单**：`xtask
//! image` 只装这里列出的 12 个文件，其余二进制（65 个命令、其余服务
//! 器）一律不进 boot image——X-11 的"占位 bin 排除"由此成立：清单是
//! 枚举不是通配，占位物没有进入镜像的通道。
//!
//! 两道锁防漂移：
//! 1. **计数锁**：数组长度绑定 `minix_boot::NR_BOOT_MODULES`（NK7 提到
//!    minix-boot 的单一真源），任何一侧增减条目，本 crate 编译失败。
//! 2. **顺序锁**：单元测试从 boot-shim 源文本提取 `MODULE_NAMES` 与
//!    本清单逐位对账（内核按下标映射 `BOOT_MODULE_PROC_NRS[i]`，顺序
//!    错即起错进程，计数锁拦不住它）。
//!
//! 权威次序：C 内核 boot image 表 `minix3/minix/kernel/table.c:44-64`
//! 的 `image[]` 用户态段。

use minix_boot::NR_BOOT_MODULES;

/// 一条装机项：模块名 ↔ workspace 包。
///
/// 装机文件名 = [`ModuleEntry::module`]；产物文件名 = 包名（workspace
/// 规约：各包 `[[bin]] name` 与 `[package] name` 同值，如
/// `fs/pfs/Cargo.toml` 的 `minix-fs-pfs`）。
pub struct ModuleEntry {
    /// 装机文件名，等于 boot-shim `MODULE_NAMES` 的同名条目。
    pub module: &'static str,
    /// workspace 包名（也是产物 bin 名）。
    pub package: &'static str,
    /// 包目录（相对 `os/`），存在性守卫测试用（构建本身走
    /// `-p package`，不消耗此字段）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub dir: &'static str,
}

/// 装机清单本体。顺序承载内核 `BOOT_MODULE_PROC_NRS` 的下标映射，
/// 不可重排。
pub const BOOT_MODULES: [ModuleEntry; NR_BOOT_MODULES] = [
    ModuleEntry {
        module: "ds",
        package: "minix-ds",
        dir: "servers/ds",
    },
    ModuleEntry {
        module: "rs",
        package: "minix-rs",
        dir: "servers/rs",
    },
    ModuleEntry {
        module: "pm",
        package: "minix-pm",
        dir: "servers/pm",
    },
    ModuleEntry {
        module: "sched",
        package: "minix-sched",
        dir: "servers/sched",
    },
    ModuleEntry {
        module: "vfs",
        package: "minix-vfs",
        dir: "servers/vfs",
    },
    ModuleEntry {
        module: "memory",
        package: "minix-driver-memory",
        dir: "drivers/storage/memory",
    },
    ModuleEntry {
        module: "tty",
        package: "minix-driver-tty",
        dir: "drivers/tty/tty",
    },
    ModuleEntry {
        module: "mib",
        package: "minix-mib",
        dir: "servers/mib",
    },
    ModuleEntry {
        module: "vm",
        package: "minix-vm",
        dir: "servers/vm",
    },
    ModuleEntry {
        module: "pfs",
        package: "minix-fs-pfs",
        dir: "fs/pfs",
    },
    ModuleEntry {
        module: "mfs",
        package: "minix-fs-mfs",
        dir: "fs/mfs",
    },
    ModuleEntry {
        module: "init",
        package: "minix-init",
        dir: "commands/sbin/init",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// 计数锁的运行时面：编译期 assert 只在本 crate 生效，测试再钉
    /// 一次并在失败信息里点名两侧，方便定位。
    #[test]
    fn boot_module_count_matches_single_source_constant() {
        assert_eq!(
            BOOT_MODULES.len(),
            NR_BOOT_MODULES,
            "装机清单条数必须等于 minix_boot::NR_BOOT_MODULES（NK7 单一真源）"
        );
    }

    /// 顺序锁：从 boot-shim 源文本提取 `MODULE_NAMES`，与本清单逐位
    /// 对账。内核按下标把第 i 个模块映射到 `BOOT_MODULE_PROC_NRS[i]`
    /// （`os/kernel/src/lib.rs:1200-1203`），装机序错一个即起错进程。
    #[test]
    fn boot_module_order_matches_boot_shim_module_names() {
        let loader_rs = include_str!("../../boot-shim/src/loader.rs");
        let start = loader_rs
            .find("pub const MODULE_NAMES")
            .unwrap_or_else(|| panic!("boot-shim loader.rs 里找不到 MODULE_NAMES"));
        let array_start = loader_rs[start..]
            .find("= &[")
            .expect("MODULE_NAMES 应以 = &[..] 形式声明")
            + start
            + 3;
        let array_end = array_start
            + loader_rs[array_start..]
                .find(']')
                .expect("MODULE_NAMES 数组未闭合");
        let body = &loader_rs[array_start..=array_end];
        let names: Vec<&str> = body
            .split('"')
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, s)| s)
            .collect();

        let expected: Vec<&str> = BOOT_MODULES.iter().map(|m| m.module).collect();
        assert_eq!(
            names, expected,
            "装机清单顺序必须与 boot-shim MODULE_NAMES 逐位一致（table.c:44-64 权威序）"
        );
    }

    /// 包目录存在性守卫：包改名/挪位而清单未跟时，构建前就报错。
    #[test]
    fn every_manifest_entry_points_at_an_existing_package_dir() {
        let os_root = env!("CARGO_MANIFEST_DIR").to_string() + "/..";
        for entry in &BOOT_MODULES {
            let path = std::path::Path::new(&os_root)
                .join(entry.dir)
                .join("Cargo.toml");
            assert!(
                path.exists(),
                "装机清单指向的包 {}（{}）不存在：{}",
                entry.package,
                entry.dir,
                path.display()
            );
        }
    }

    /// 模块名不重复：loader 逐名建文件，重名会在 ESP 里互相覆盖。
    #[test]
    fn module_names_are_unique() {
        let mut names: Vec<&str> = BOOT_MODULES.iter().map(|m| m.module).collect();
        names.sort_unstable();
        let len = names.len();
        names.dedup();
        assert_eq!(names.len(), len, "装机清单出现重名模块");
    }
}
