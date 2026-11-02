//! Lockstep check between the two link-script families.
//!
//! `os/kernel/src/arch/*/link.ld` has no production link reader (see
//! 02-higher-half-kernel §3.1), but its constants are still documentation of
//! the intended layout. This test keeps them from silently drifting away from
//! the production scripts in `os/kernel-image/`.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_kernel_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn parse_hex_value(text: &str, key: &str) -> u64 {
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = rest
            .split(|c: char| c == ';' || c.is_whitespace())
            .next()
            .unwrap_or("");
        let value = value.trim_start_matches("0x").trim_start_matches("0X");
        return u64::from_str_radix(value, 16).unwrap_or_else(|_| {
            panic!("cannot parse {key} value {value:?} in link script");
        });
    }
    panic!("missing {key} in link script");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

#[test]
fn arch_link_ld_constants_match_production_scripts() {
    let root = repo_kernel_dir();
    let arch_dir = root.join("src/arch");
    let image_dir = root.join("../kernel-image");

    let cases = [
        ("x86_64", "KERN_VIRT_BASE", "KERNEL_HIGH_BASE"),
        ("aarch64", "KERN_VIRT_BASE", "KERNEL_VIRT_BASE"),
        ("riscv64", "KERN_VIRT_BASE", "KERNEL_VIRT_BASE"),
    ];

    for (arch, arch_virt_key, prod_virt_key) in cases {
        let arch_script = read(&arch_dir.join(arch).join("link.ld"));
        let prod_script = read(&image_dir.join(format!("{arch}.ld")));

        let arch_virt = parse_hex_value(&arch_script, arch_virt_key);
        let arch_phys = parse_hex_value(&arch_script, "KERN_PHYS_BASE");
        let prod_virt = parse_hex_value(&prod_script, prod_virt_key);
        let prod_phys = parse_hex_value(&prod_script, "KERNEL_PHYS_BASE");

        assert_eq!(
            arch_virt, prod_virt,
            "{arch}: arch link.ld {arch_virt_key} != production {prod_virt_key}"
        );
        assert_eq!(
            arch_phys, prod_phys,
            "{arch}: arch link.ld KERN_PHYS_BASE != production KERNEL_PHYS_BASE"
        );
    }
}
