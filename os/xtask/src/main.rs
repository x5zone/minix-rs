//! Minix-RS Build System (xtask)
//!
//! 用法:
//!   cargo run -p xtask build          # 编译项目
//!   cargo run -p xtask test           # 运行所有测试
//!   cargo run -p xtask test --slice fork  # 运行 fork 切片测试
//!   cargo run -p xtask check          # 检查代码
//!   cargo run -p xtask doc            # 生成文档

use clap::{Parser, Subcommand};
use std::process::Command;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Minix-RS build system")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 编译项目
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
    /// 打包 boot image / rootfs 镜像（骨架）
    Image {
        #[arg(short, long)]
        release: bool,
    },
    /// 启动 QEMU 并跑验收（骨架）
    Qemu,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { release } => build(release),
        Commands::Test { slice } => run_tests(slice),
        Commands::Check => check(),
        Commands::Doc => doc(),
        Commands::Clean => clean(),
        Commands::Image { release } => image(release),
        Commands::Qemu => qemu(),
    }
}

fn build(release: bool) -> anyhow::Result<()> {
    println!("🚀 Building Minix-RS...");

    // 编译库
    println!("📦 Building libraries...");
    run_cargo(&["build", "-p", "minix-types"], release)?;
    run_cargo(&["build", "-p", "minix-plat"], release)?;
    run_cargo(&["build", "-p", "minix-sys"], release)?;
    run_cargo(&["build", "-p", "minix-rt"], release)?;

    // 编译内核
    println!("🔨 Building kernel...");
    run_cargo(&["build", "-p", "minix-kernel"], release)?;

    // 编译服务器
    println!("🔧 Building servers...");
    run_cargo(&["build", "-p", "minix-pm"], release)?;

    println!("✅ Build complete!");
    Ok(())
}

fn run_tests(slice: Option<String>) -> anyhow::Result<()> {
    match slice {
        Some(s) => {
            println!("🧪 Running {} slice tests...", s);
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

fn image(release: bool) -> anyhow::Result<()> {
    println!("📦 Packaging boot image (skeleton)...");
    let _ = release;
    // TODO(skeleton): 占位——实装于 15-stage-fs / 16-stage-drivers 之前需要的基础设施：
    //   1) 收集 boot image 进程 ELF（kernel + ds/rs/pm/sched/vfs/tty/memory/mib/vm/pfs/mfs/init，
    //      对照 minix3/minix/kernel/table.c:36 boot_image 数组）
    //   2) 按 minix3 boot image 格式打包（参考 minix3/releasetools/mkboot + distrib/common/bootimage）
    //   3) 生成 rootfs/ramdisk 镜像（mkfs + fstab，参考 minix3/distrib/common/bootimage/fstab.in）
    //   4) 输出到 os/target/image/
    println!("    [skeleton] 计划输出: os/target/image/boot_image + rootfs");
    Ok(())
}

fn qemu() -> anyhow::Result<()> {
    println!("🚀 Launching QEMU (skeleton)...");
    // TODO(skeleton): 组合 QEMU 参数并启动（-nographic -kernel <boot_image> -hda <rootfs> ...），
    //   参考 os/qemu-tests/run_qemu.sh；验收标准 = boot_to_sample 集成测试。
    println!("    [skeleton] 待实现：参考 os/qemu-tests/run_qemu.sh");
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
