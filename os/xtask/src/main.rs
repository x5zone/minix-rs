//! Minix-RS Build System (xtask)
//!
//! Usage:
//!   cargo run -p xtask build          # build the project (host dev face, full)
//!   cargo run -p xtask test           # run all tests
//!   cargo run -p xtask test --slice fork  # run the fork slice tests
//!   cargo run -p xtask check          # check the code
//!   cargo run -p xtask doc            # generate documentation
//!   cargo run -p xtask image          # assemble a bootable machine image
//!   cargo run -p xtask qemu           # boot QEMU with the image

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
    /// Build the project (host dev face: libraries + kernel + all manifest servers)
    Build {
        #[arg(short, long)]
        release: bool,
    },
    /// Run tests
    Test {
        /// Run a specific slice of tests (fork/exec/exit/...)
        #[arg(short, long)]
        slice: Option<String>,
    },
    /// Check the code
    Check,
    /// Generate documentation
    Doc,
    /// Clean build artifacts
    Clean,
    /// Assemble a bootable machine image (boot-shim + kernel + 12 modules + imgrd)
    Image {
        #[arg(short, long)]
        release: bool,
        /// Target architecture (x86_64 / aarch64 / riscv64)
        #[arg(long, default_value = "x86_64")]
        arch: String,
        /// Kernel ELF path (defaults to target/x86_64-unknown-none/<profile>/kernel,
        /// i.e. the artifact produced by the kernel-image package; the ESP install
        /// name is always /EFI/minix/kernel.elf)
        #[arg(long)]
        kernel: Option<PathBuf>,
        /// Only print the assembly plan, write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Boot QEMU with the assembled image (real-machine verification must first
    /// confirm a free QEMU window)
    Qemu {
        /// Target architecture (x86_64 / aarch64 / riscv64)
        #[arg(long, default_value = "x86_64")]
        arch: String,
        /// Disk image path (defaults to target/image/<arch>/minix.img)
        #[arg(long)]
        image: Option<PathBuf>,
        /// Serial log path (defaults to target/image/<arch>/serial.log)
        #[arg(long)]
        serial: Option<PathBuf>,
        /// Only print the command line, do not start
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate the ATF on-target face (rc variant + atf-plan.txt) — seeds
    /// architectures that do not go through the UEFI disk-image assembly
    /// (riscv64 direct OpenSBI load); the case table and injection rules share
    /// their source with `image`
    AtfFace {
        /// Target architecture (aarch64 / riscv64)
        #[arg(long, default_value = "riscv64")]
        arch: String,
        /// Only print, write nothing
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

/// Full host dev-face build: base libraries + kernel + the 12 manifest packages
/// + the disk tools.
///
/// The targets here are the host triple (the E5 host-half run shape of servers
/// and commands); the guest/UEFI target builds that produce the image belong to
/// the `image` subcommand (the production switch and `--target` are passed in
/// pairs; see the image.rs module documentation).
fn build(release: bool) -> anyhow::Result<()> {
    println!("🚀 Building Minix-RS (host dev, full manifest)...");

    // Build the libraries
    println!("📦 Building libraries...");
    run_cargo(&["build", "-p", "minix-types"], release)?;
    run_cargo(&["build", "-p", "minix-plat"], release)?;
    run_cargo(&["build", "-p", "minix-sys"], release)?;
    run_cargo(&["build", "-p", "minix-rt"], release)?;

    // Build the kernel
    println!("🔨 Building kernel...");
    run_cargo(&["build", "-p", "minix-kernel"], release)?;

    // Build every server on the manifest (whitelist semantics: the image only
    // carries packages listed there)
    println!("🔧 Building boot modules (manifest)...");
    for entry in manifest::BOOT_MODULES.iter() {
        run_cargo(&["build", "-p", entry.package], release)?;
    }

    // Disk tools (the imgrd generator)
    run_cargo(&["build", "-p", "minix-diskfmt"], release)?;

    println!("✅ Build complete!");
    Ok(())
}

fn run_tests(slice: Option<String>) -> anyhow::Result<()> {
    match slice {
        Some(s) => {
            println!("🧪 Running {s} slice tests...");
            // Run the specific slice tests
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
