//! Minix-RS Build System (xtask)
//!
//! 用法:
//!   cargo run -p xtask build          # 编译项目（宿主开发面，全量）
//!   cargo run -p xtask test           # 运行所有测试
//!   cargo run -p xtask test --slice fork  # 运行 fork 切片测试
//!   cargo run -p xtask check          # 检查代码
//!   cargo run -p xtask doc            # 生成文档
//!   cargo run -p xtask image          # 装配可启动机镜像（E-IMGPKG）
//!   cargo run -p xtask qemu           # 用镜像启动 QEMU

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

mod image;
mod manifest;
mod qemu;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Minix-RS build system")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 编译项目（宿主开发面：库 + 内核 + 装机清单全量服务器）
    Build {
        #[arg(short, long)]
        release: bool,
    },
    /// 运行测试
    Test {
        /// 指定切片测试（fork/exec/exit/...）
        #[arg(short, long)]
        slice: Option<String>,
    },
    /// 检查代码
    Check,
    /// 生成文档
    Doc,
    /// 清理构建产物
    Clean,
    /// 装配可启动机镜像（boot-shim + 内核 + 12 模块 + imgrd）
    Image {
        #[arg(short, long)]
        release: bool,
        /// 目标架构（x86_64 / aarch64 / riscv64）
        #[arg(long, default_value = "x86_64")]
        arch: String,
        /// 内核 ELF 路径（缺省取 target/x86_64-unknown-none/<profile>/kernel，
        /// 即 kernel-image 包的产出工件；ESP 安装名恒为 /EFI/minix/kernel.elf）
        #[arg(long)]
        kernel: Option<PathBuf>,
        /// 只打印装配计划，不落盘
        #[arg(long)]
        dry_run: bool,
    },
    /// 用装配好的镜像启动 QEMU（真机验证须先确认 QEMU 空闲窗口）
    Qemu {
        /// 目标架构（x86_64 / aarch64 / riscv64）
        #[arg(long, default_value = "x86_64")]
        arch: String,
        /// 盘镜像路径（缺省取 target/image/<arch>/minix.img）
        #[arg(long)]
        image: Option<PathBuf>,
        /// 串口日志路径（缺省取 target/image/<arch>/serial.log）
        #[arg(long)]
        serial: Option<PathBuf>,
        /// 只打印命令行，不启动
        #[arg(long)]
        dry_run: bool,
    },
    /// 生成 ATF 上机面（rc 变体 + atf-plan.txt）——给不走 UEFI 盘形装配的
    /// 架构（riscv64 OpenSBI 直载）播种；用例表/注入规则与 `image` 同源
    AtfFace {
        /// 目标架构（aarch64 / riscv64）
        #[arg(long, default_value = "riscv64")]
        arch: String,
        /// 只打印不落盘
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { release } => build(release),
        Commands::Test { slice } => run_tests(slice),
        Commands::Check => check(),
        Commands::Doc => doc(),
        Commands::Clean => clean(),
        Commands::Image {
            release,
            arch,
            kernel,
            dry_run,
        } => {
            let arch = image::Arch::parse(&arch)?;
            image::run(release, arch, kernel.as_deref(), dry_run)
        }
        Commands::Qemu {
            arch,
            image: img,
            serial,
            dry_run,
        } => {
            let arch = image::Arch::parse(&arch)?;
            qemu::run(arch, img.as_deref(), serial.as_deref(), dry_run)
        }
        Commands::AtfFace { arch, dry_run } => {
            let arch = image::Arch::parse(&arch)?;
            image::atf_face(arch, dry_run)
        }
    }
}

/// 宿主开发面全量构建：基础库 + 内核 + 装机清单 12 包 + 盘工具。
///
/// 这里的目标是宿主三元组（服务器/命令的 E5 宿主半运行形态）；镜像
/// 产出的 guest/UEFI 目标构建归 `image` 子命令（生产开关与
/// `--target` 成对传递，见 image.rs 模块文档）。
fn build(release: bool) -> anyhow::Result<()> {
    println!("🚀 Building Minix-RS (host dev, full manifest)...");

    // 编译库
    println!("📦 Building libraries...");
    run_cargo(&["build", "-p", "minix-types"], release)?;
    run_cargo(&["build", "-p", "minix-plat"], release)?;
    run_cargo(&["build", "-p", "minix-sys"], release)?;
    run_cargo(&["build", "-p", "minix-rt"], release)?;

    // 编译内核
    println!("🔨 Building kernel...");
    run_cargo(&["build", "-p", "minix-kernel"], release)?;

    // 编译装机清单全量服务器（X-11 白名单语义：镜像只装清单内包）
    println!("🔧 Building boot modules (manifest)...");
    for entry in manifest::BOOT_MODULES.iter() {
        run_cargo(&["build", "-p", entry.package], release)?;
    }

    // 盘工具（imgrd 生成器）
    run_cargo(&["build", "-p", "minix-diskfmt"], release)?;

    println!("✅ Build complete!");
    Ok(())
}

fn run_tests(slice: Option<String>) -> anyhow::Result<()> {
    match slice {
        Some(s) => {
            println!("🧪 Running {s} slice tests...");
            // 运行特定切片测试
            run_cargo(&["test", "-p", "minix-pm", &s], false)?;
        }
        None => {
            println!("🧪 Running all tests...");
            run_cargo(&["test", "--workspace"], false)?;
        }
    }
    println!("✅ Tests complete!");
    Ok(())
}

fn check() -> anyhow::Result<()> {
    println!("🔍 Checking code...");
    run_cargo(&["check", "--workspace"], false)?;
    run_cargo(&["clippy", "--workspace", "--", "-D", "warnings"], false)?;
    println!("✅ Check complete!");
    Ok(())
}

fn doc() -> anyhow::Result<()> {
    println!("📚 Generating documentation...");
    run_cargo(&["doc", "--workspace", "--no-deps"], false)?;
    println!("✅ Documentation generated!");
    Ok(())
}

fn clean() -> anyhow::Result<()> {
    println!("🧹 Cleaning build artifacts...");
    run_cargo(&["clean"], false)?;
    println!("✅ Clean complete!");
    Ok(())
}

fn run_cargo(args: &[&str], release: bool) -> anyhow::Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(args);
    if release {
        cmd.arg("--release");
    }

    println!("  > cargo {}", args.join(" "));

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("Command failed: cargo {}", args.join(" "));
    }

    Ok(())
}
