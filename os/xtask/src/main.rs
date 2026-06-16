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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { release } => build(release),
        Commands::Test { slice } => run_tests(slice),
        Commands::Check => check(),
        Commands::Doc => doc(),
        Commands::Clean => clean(),
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
