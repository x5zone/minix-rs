//! Installation manifest (E-IMGPKG): which workspace package each of the boot
//! image's 12 user-space modules comes from.
//!
//! The consumption contract is on the boot-shim side: `MODULE_NAMES` in
//! `os/boot-shim/src/loader.rs` reads `/EFI/minix/modules/<name>` name by name
//! and panics when one is missing (fail-fast semantics). This manifest is the
//! **whitelist** of the installation side: `xtask image` installs only the 12
//! files listed here; every other binary (65 commands and the remaining
//! servers) stays out of the boot image — the placeholder-bin exclusion holds
//! because of that: the list is an enumeration, not a glob, so placeholders
//! have no channel into the image.
//!
//! Two locks prevent drift:
//! 1. **Count lock**: the array length is bound to
//!    `minix_boot::NR_BOOT_MODULES` (the single source of truth in
//!    minix-boot); adding or removing an entry on either side breaks this
//!    crate's build.
//! 2. **Order lock**: a unit test extracts `MODULE_NAMES` from the boot-shim
//!    source text and reconciles it position by position against this
//!    manifest (the kernel maps `BOOT_MODULE_PROC_NRS[i]` by index; a wrong
//!    order starts the wrong process, and the count lock cannot catch it).
//!
//! Authoritative order: the user-space section of `image[]` in the C kernel
//! boot image table `minix3/minix/kernel/table.c:44-64`.

use minix_boot::NR_BOOT_MODULES;

/// One installation entry: module name ↔ workspace package.
///
/// Installed file name = [`ModuleEntry::module`]; artifact file name = package
/// name (workspace rule: each package's `[[bin]] name` equals its
/// `[package] name`, e.g. `minix-fs-pfs` in `fs/pfs/Cargo.toml`).
pub struct ModuleEntry {
    /// Installed file name, equal to the same-named boot-shim `MODULE_NAMES` entry.
    pub module: &'static str,
    /// Workspace package name (also the artifact bin name).
    pub package: &'static str,
    /// Package directory (relative to `os/`), used by the existence guard test
    /// (the build itself goes through `-p package` and does not use this field).
    #[cfg_attr(not(test), allow(dead_code))]
    pub dir: &'static str,
}

/// The manifest proper. Its order carries the kernel `BOOT_MODULE_PROC_NRS`
/// index mapping and must not be reordered.
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

    /// Runtime face of the count lock: the compile-time assert only guards this
    /// crate, so the test pins it once more and names both sides in the failure
    /// message for easy triage.
    #[test]
    fn boot_module_count_matches_single_source_constant() {
        assert_eq!(
            BOOT_MODULES.len(),
            NR_BOOT_MODULES,
            "装机清单条数必须等于 minix_boot::NR_BOOT_MODULES（NK7 单一真源）"
        );
    }

    /// Order lock: extract `MODULE_NAMES` from the boot-shim source text and
    /// reconcile it position by position with this manifest. The kernel maps the
    /// i-th module to `BOOT_MODULE_PROC_NRS[i]` (`os/kernel/src/lib.rs:1200-1203`);
    /// one wrong slot in the install order starts the wrong process.
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

    /// Package-directory existence guard: a renamed or moved package whose
    /// manifest entry did not follow fails before the build.
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

    /// Module names are unique: the loader creates one file per name, and
    /// duplicates would overwrite each other in the ESP.
    #[test]
    fn module_names_are_unique() {
        let mut names: Vec<&str> = BOOT_MODULES.iter().map(|m| m.module).collect();
        names.sort_unstable();
        let len = names.len();
        names.dedup();
        assert_eq!(names.len(), len, "装机清单出现重名模块");
    }
}
