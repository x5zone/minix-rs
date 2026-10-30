//! `xtask qemu` — start QEMU with the assembled image.
//!
//! The argument shape mirrors the UEFI path of `os/qemu-tests/run_qemu.sh`:
//! the firmware is mounted as a two-slot pflash pair (code read-only, vars
//! writable), the image is the second disk, and the serial port goes to a
//! file. `--dry-run` only prints the command line — hardware verification
//! is subject to the QEMU serial discipline (only one party may occupy
//! QEMU at a time across the whole repository); this command performs no
//! occupancy arbitration, so the caller must confirm a free window before
//! running it.
//!
//! riscv64 honest bail: the boot path is U-Boot fatload + BootFileTable,
//! not a direct UEFI boot; the carrier mechanism is in
//! `os/qemu-tests/test-riscv64-uboot.sh`.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::image::Arch;

/// Candidate paths for the firmware code/vars (the same list as
/// run_qemu.sh:14-36).
fn firmware(arch: Arch) -> Option<(PathBuf, PathBuf)> {
    let (code_candidates, vars_candidates): (&[&str], &[&str]) = match arch {
        Arch::X86_64 => (
            &[
                "/usr/share/OVMF/OVMF_CODE_4M.fd",
                "/usr/share/OVMF/OVMF_CODE.fd",
                "/usr/share/ovmf/OVMF.fd",
            ],
            &[
                "/usr/share/OVMF/OVMF_VARS_4M.fd",
                "/usr/share/OVMF/OVMF_VARS.fd",
            ],
        ),
        Arch::Aarch64 => (
            &[
                "/usr/share/AAVMF/AAVMF_CODE.fd",
                "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd",
            ],
            &["/usr/share/AAVMF/AAVMF_VARS.fd"],
        ),
        Arch::Riscv64 => (&[], &[]),
    };
    let pick = |list: &[&str]| list.iter().map(PathBuf::from).find(|p| p.is_file());
    let code = pick(code_candidates)?;
    let vars = pick(vars_candidates)?;
    Some((code, vars))
}

/// Assemble the QEMU command line (without running it). Arguments are
/// collected into a vector first and only then handed to `Command` — the
/// planning face (printing/dry-run) and the execution face share the same
/// argument text.
pub fn command(arch: Arch, image: &Path, serial: &Path) -> Result<(Command, Vec<String>)> {
    if arch == Arch::Riscv64 {
        bail!(
            "riscv64 无 UEFI 直启路径：走 U-Boot fatload + BootFileTable，\
             载体机制见 os/qemu-tests/test-riscv64-uboot.sh。"
        );
    }
    let (fw_code, fw_vars) = firmware(arch).with_context(|| {
        format!("{arch:?} 固件未找到（x86_64 需装 ovmf，aarch64 需装 qemu-efi-aarch64）")
    })?;

    let program = match arch {
        Arch::X86_64 => "qemu-system-x86_64",
        Arch::Aarch64 => "qemu-system-aarch64",
        Arch::Riscv64 => unreachable!("riscv64 已在上方 bail"),
    };
    let mut args: Vec<String> = Vec::new();
    match arch {
        Arch::X86_64 => args.extend(["-smp".into(), "4".into()]),
        Arch::Aarch64 => args.extend([
            "-machine".into(),
            // `gic-version=3` is a boot contract, not a tuning knob:
            // `AcpiDesc::parse`'s aarch64 gate (`check_gic_madt`) rejects
            // GICv1/v2 (`GicVersionUnsupported`) because the driver is
            // GICv3-only (per-CPU redistributor via GICR stride, ICCT
            // MMIO), and QEMU virt defaults to GICv2. Without this flag
            // the kernel panics in `init_from_kinfo` on "no platform
            // source parsed successfully".
            "virt,gic-version=3".into(),
            "-cpu".into(),
            "max".into(),
            "-smp".into(),
            "4".into(),
        ]),
        Arch::Riscv64 => unreachable!("riscv64 已在上方 bail"),
    }
    let vars_local = vars_copy_target(arch, fw_vars)?;
    for (unit, file, readonly) in [(0, fw_code, true), (1, vars_local, false)] {
        let mut drive = format!(
            "if=pflash,format=raw,unit={unit},file={}",
            file.to_string_lossy()
        );
        if readonly {
            drive.push_str(",readonly=on");
        }
        args.extend(["-drive".into(), drive]);
    }
    args.extend([
        "-drive".into(),
        format!("file={},format=raw,media=disk", image.to_string_lossy()),
        "-serial".into(),
        format!("file:{}", serial.to_string_lossy()),
        "-display".into(),
        "none".into(),
        "-no-reboot".into(),
    ]);
    // isa-debug-exit is an x86 ISA device (x86-only branch,
    // run_qemu.sh:117); the aarch64 virt machine has no ISA bus, so it is
    // not universal.
    if arch == Arch::X86_64 {
        args.extend(["-device".into(), "isa-debug-exit".into()]);
    }

    let mut cmd = Command::new(program);
    cmd.args(&args);
    Ok((cmd, args))
}

/// The vars copy lands next to the image output directory (firmware vars
/// is a writable slot, so the system's read-only original cannot be used
/// directly — the /tmp copy strategy of run_qemu.sh:97-99, landed inside
/// the target tree here).
fn vars_copy_target(arch: Arch, src_vars: PathBuf) -> Result<PathBuf> {
    let dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"))
        .join("image")
        .join(arch.slug());
    std::fs::create_dir_all(&dir).with_context(|| format!("建目录失败：{}", dir.display()))?;
    let dst = dir.join("fw_vars.fd");
    std::fs::copy(&src_vars, &dst).with_context(|| {
        format!(
            "复制固件 vars 失败：{} → {}",
            src_vars.display(),
            dst.display()
        )
    })?;
    Ok(dst)
}

/// The `xtask qemu` entry point.
pub fn run(arch: Arch, image: Option<&Path>, serial: Option<&Path>, dry_run: bool) -> Result<()> {
    let layout_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"))
        .join("image")
        .join(arch.slug());
    let image = match image {
        Some(p) => p.to_path_buf(),
        None => layout_dir.join("minix.img"),
    };
    let serial = match serial {
        Some(p) => p.to_path_buf(),
        None => layout_dir.join("serial.log"),
    };
    if !dry_run && !image.is_file() {
        bail!(
            "镜像不存在：{}（先跑 `cargo run -p xtask image --arch {}`）",
            image.display(),
            arch.slug()
        );
    }

    let (mut cmd, shown) = command(arch, &image, &serial)?;
    println!("🚀 QEMU 命令行（{arch:?}）：");
    println!("   qemu-system-* {}", shown.join(" "));
    if dry_run {
        println!("✅ dry-run：命令未执行（真机验证请先确认 QEMU 空闲窗口，new_edge4 §1 规则 7）");
        return Ok(());
    }
    println!("   串口日志：{}（Ctrl-C 结束）", serial.display());
    let status = cmd.status().context("无法启动 qemu")?;
    if !status.success() {
        bail!("qemu 以非零状态退出：{status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// riscv64 honest bail: no pretending on a non-UEFI path.
    #[test]
    fn riscv64_bails_honestly() {
        let err = command(
            Arch::Riscv64,
            Path::new("/tmp/x.img"),
            Path::new("/tmp/s.log"),
        )
        .expect_err("riscv64 应 bail");
        assert!(err.to_string().contains("U-Boot"), "报错要点名 U-Boot 路径");
    }

    /// An environment without firmware (common in CI/containers) reports
    /// what is missing instead of panicking.
    #[test]
    fn missing_firmware_errors_with_install_hint() {
        // Does not modify the file system, so it only holds on machines
        // where every firmware candidate is missing; on a machine that has
        // firmware this test degrades to verifying that the command can be
        // assembled.
        match command(
            Arch::X86_64,
            Path::new("/tmp/x.img"),
            Path::new("/tmp/s.log"),
        ) {
            Err(err) => assert!(err.to_string().contains("ovmf"), "报错要给安装提示"),
            Ok((cmd, shown)) => {
                assert!(shown.windows(2).any(|w| w[0] == "-drive"));
                assert!(cmd.get_program().to_string_lossy().contains("x86_64"));
            }
        }
    }
}
