//! `xtask qemu` — 用装配好的镜像启动 QEMU（E-IMGPKG / new_edge3 NS8）。
//!
//! 参数形态对位 `os/qemu-tests/run_qemu.sh` 的 UEFI 路径：固件经
//! pflash 双槽挂载（code 只读 + vars 可写），镜像作第二块盘，串口落
//! 文件。`--dry-run` 只打印命令行——真机验证受 QEMU 串行纪律约束
//! （new_edge4 §1 规则 7：全仓同一时刻只允许一方占 QEMU），本命令
//! 不做占用仲裁，运行时机由调用方自行确认空闲窗口。
//!
//! riscv64 honest bail：启动路径是 U-Boot fatload + BootFileTable，
//! 不是 UEFI 直启，载体机制见 `os/qemu-tests/test-riscv64-uboot.sh`。

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::image::Arch;

/// 固件 code/vars 的候选路径（与 run_qemu.sh:14-36 同清单）。
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

/// 组装 QEMU 命令行（不含执行）。参数先聚成向量再落 Command——
/// 计划面（打印/dry-run）与执行面共享同一份参数文本。
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
            "virt".into(),
            "-cpu".into(),
            "cortex-a72".into(),
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
    // isa-debug-exit 是 x86 ISA 设备（run_qemu.sh:117 的 x86 段独有），
    // aarch64 virt 机器没有 ISA 总线，不通用。
    if arch == Arch::X86_64 {
        args.extend(["-device".into(), "isa-debug-exit".into()]);
    }

    let mut cmd = Command::new(program);
    cmd.args(&args);
    Ok((cmd, args))
}

/// vars 副本落在镜像产出目录旁（固件 vars 是可写槽，不能直接用系统
/// 只读原件——run_qemu.sh:97-99 的 /tmp 副本策略，这里落 target 树内）。
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

/// `xtask qemu` 入口。
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

    /// riscv64 honest bail：非 UEFI 路径不装样子。
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

    /// 无固件环境（CI/容器常见）报错点名缺什么，而非 panic。
    #[test]
    fn missing_firmware_errors_with_install_hint() {
        // 不改文件系统，只在固件候选全缺失的机器上成立；有固件的机器
        // 上本测试退化为只验证命令可组装。
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
