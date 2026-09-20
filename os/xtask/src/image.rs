//! `xtask image` — 把构建产物装配成可启动机镜像（E-IMGPKG / new_edge3
//! NS8）。
//!
//! 包形（x86_64，UEFI 路径）：一张 FAT 盘镜像，内含
//!   - `/EFI/BOOT/BOOTX64.EFI` ← boot-shim（固件默认加载项）
//!   - `/EFI/minix/kernel.elf` ← 内核 ELF（boot-shim 按
//!     `loader::KERNEL_PATH` 读取，`os/boot-shim/src/loader.rs:31`）
//!   - `/EFI/minix/modules/<名>` ×12 ← 装机清单（[`crate::manifest`]）
//!   - `/EFI/minix/imgrd` ← mfs 根盘（mkfs_mfs 播种 `/etc` 最小集）。
//!     imgrd 的启动期消费通道（基址/尺寸如何进内核再进 memory 驱动）
//!     归 new_edge3 NS6 设计；装机面只保证工件在包内有确定路径。
//!
//! 与 boot-shim 的 fail-fast 同语义：模块缺件在装配期报错点名
//! （`os/boot-shim/src/loader.rs:280-288` 的运行期对位），不让半包
//! 镜像流出去。
//!
//! 生产开关：boot-shim 以 `--no-default-features --features
//! fw-x86-uefi` 构建（X-2/NK5 的 feature 词汇表），链接进 boot-shim 的
//! 内核半因此不带 mock（`boot-shim/Cargo.toml` 对 minix-kernel 已写死
//! `default-features = false`）。模块逐包独立调用 cargo——规避 feature
//! 统一陷阱（一次多 `-p` 会把 Mock 与生产类型拉进同一编译），也让
//! 失败点名到具体模块。

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// 目标架构。三架构布局中 x86_64 是当前唯一有完整产出链的（boot-shim
/// 的 UEFI bin 只有 `fw-x86-uefi` 一个门）；另两架构给出 honest bail
/// 而不是装出不可启动机的镜像。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arch {
    X86_64,
    Aarch64,
    Riscv64,
}

impl Arch {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" => Ok(Self::Aarch64),
            "riscv64" => Ok(Self::Riscv64),
            other => bail!("未知架构 {other}（支持 x86_64 / aarch64 / riscv64）"),
        }
    }

    /// 模块（用户态）构建目标三元组。
    fn module_target(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-none",
            Self::Aarch64 => "aarch64-unknown-none",
            Self::Riscv64 => "riscv64gc-unknown-none-elf",
        }
    }

    /// 输出目录名。
    pub fn slug(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }
}

/// 装配计划里的一步。计划先行、执行在后——`--dry-run` 打印同一份
/// 计划，单元测试也直接对计划断言。
#[derive(Debug)]
pub enum Action {
    /// 在 workspace 根调 cargo（构建类步骤）。
    Cargo {
        args: Vec<String>,
        note: &'static str,
    },
    /// 建目录。
    Mkdir { path: PathBuf },
    /// 落盘文本（如生成的原型文件）。
    Write {
        path: PathBuf,
        bytes: Vec<u8>,
        note: &'static str,
    },
    /// 复制产物进 staging。
    Copy {
        from: PathBuf,
        to: PathBuf,
        note: &'static str,
    },
    /// 外部工具（mkfs_mfs / mtools）。
    Tool {
        program: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        note: &'static str,
    },
}

/// 装配所需的路径环境。全部由调用方传入以便测试。
pub struct Layout {
    /// workspace 根（`os/`）。
    pub os_root: PathBuf,
    /// cargo target 根（`CARGO_TARGET_DIR` 或 `os/target`）。
    pub target_root: PathBuf,
}

impl Layout {
    pub fn from_env() -> Self {
        let os_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .canonicalize()
            .expect("workspace 根必须存在");
        let target_root = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| os_root.join("target"));
        Self {
            os_root,
            target_root,
        }
    }

    fn profile_dir(&self, release: bool) -> &'static str {
        if release { "release" } else { "debug" }
    }
}

/// 内核 ELF 的取用路径（NS8-A：生产内核 bin 尚无产出者，属 new_edge1
/// 面；装机面按给定路径取件，不代产）。
fn kernel_elf_path(
    layout: &Layout,
    override_path: Option<&Path>,
    arch: Arch,
    release: bool,
) -> PathBuf {
    match override_path {
        Some(p) => p.to_path_buf(),
        None => layout
            .target_root
            .join(arch.module_target())
            .join(layout.profile_dir(release))
            .join("kernel.elf"),
    }
}

/// 生成装配计划，返回（动作序列，ESP 盘镜像产出位）。
pub fn plan(
    arch: Arch,
    release: bool,
    kernel_override: Option<&Path>,
    layout: &Layout,
) -> Result<(Vec<Action>, PathBuf)> {
    // 三架构诚实话：aarch64 的 boot-shim UEFI bin 未产出（feature 门只有
    // fw-x86-uefi），riscv64 走 U-Boot fatload + BootFileTable，不经 UEFI
    // 装机形（载体机制见 qemu-tests/test-riscv64-uboot.sh）。布局参数在
    // Arch 枚举里已备，产出链补齐后此处放行。
    match arch {
        Arch::X86_64 => {}
        Arch::Aarch64 => bail!(
            "aarch64 镜像暂不可装配：boot-shim 无 fw-aarch64-uefi 产出（os/boot-shim/Cargo.toml \
             仅有 fw-x86-uefi 门）。补齐 shim 后布局参数（aarch64-unknown-none 模块 + \
             BOOTAA64.EFI）已备。"
        ),
        Arch::Riscv64 => bail!(
            "riscv64 镜像暂不可装配：启动路径是 U-Boot fatload + BootFileTable（非 UEFI 盘），\
             载体机制见 os/qemu-tests/test-riscv64-uboot.sh。"
        ),
    }

    let image_dir = layout.target_root.join("image").join(arch.slug());
    let staging = image_dir.join("staging");
    let esp_image = image_dir.join("minix.img");
    let profile = layout.profile_dir(release);
    let mut actions: Vec<Action> = Vec::new();
    let cargo_release = |actions: &mut Vec<Action>, note: &'static str, extra: &[&str]| {
        let mut args = vec!["build".to_string()];
        if release {
            args.push("--release".into());
        }
        args.extend(extra.iter().map(|s| s.to_string()));
        actions.push(Action::Cargo { args, note });
    };

    // 1. 逐包构建 12 模块（guest 用户态目标；失败点名到包）。
    for entry in crate::manifest::BOOT_MODULES.iter() {
        cargo_release(
            &mut actions,
            "装配清单模块构建",
            &["-p", entry.package, "--target", arch.module_target()],
        );
    }

    // 2. boot-shim（生产开关：关默认 feature，只开 fw-x86-uefi）。
    cargo_release(
        &mut actions,
        "boot-shim UEFI 载体构建（生产开关）",
        &[
            "-p",
            "boot-shim",
            "--target",
            "x86_64-unknown-uefi",
            "--no-default-features",
            "--features",
            "fw-x86-uefi",
        ],
    );

    // 3. mkfs_mfs 宿主工具（imgrd 生成器；宿主三元组，S32 批次三十一交付）。
    cargo_release(
        &mut actions,
        "mkfs_mfs 宿主工具构建",
        &["-p", "minix-diskfmt"],
    );

    // 4. staging 目录树。
    let modules_dir = staging.join("EFI/minix/modules");
    actions.push(Action::Mkdir {
        path: modules_dir.clone(),
    });
    actions.push(Action::Mkdir {
        path: staging.join("EFI/BOOT"),
    });

    // 5. 12 模块逐件取包装机（缺件 fail-fast 点名）。
    for entry in crate::manifest::BOOT_MODULES.iter() {
        actions.push(Action::Copy {
            from: layout
                .target_root
                .join(arch.module_target())
                .join(profile)
                .join(entry.package),
            to: modules_dir.join(entry.module),
            note: "模块装机（文件名 = boot-shim MODULE_NAMES 条目）",
        });
    }

    // 6. 内核 ELF（NS8-A：按路径取件，产出者归 new_edge1 面）。
    let kernel = kernel_elf_path(layout, kernel_override, arch, release);
    actions.push(Action::Copy {
        from: kernel,
        to: staging.join("EFI/minix/kernel.elf"),
        note: "内核 ELF 装机（loader::KERNEL_PATH 契约位）",
    });

    // 7. /etc 原型 + mkfs_mfs 播种 imgrd。
    let proto_path = image_dir.join("imgrd.proto");
    let imgrd = image_dir.join("imgrd.img");
    actions.push(Action::Write {
        path: proto_path.clone(),
        bytes: generate_etc_proto(&layout.os_root.join("etc"))?,
        note: "imgrd 原型文件（/etc 最小集 + /dev/console）",
    });
    actions.push(Action::Tool {
        program: layout
            .target_root
            .join(profile)
            .join("mkfs_mfs")
            .to_string_lossy()
            .into_owned(),
        args: vec![
            imgrd.to_string_lossy().into_owned(),
            "2048".into(), // 块数：2048 × 4KiB = 8MiB，装机最小集宽裕
            "0".into(),    // inode 数 0 = mkfs 缺省阶梯（mkfs.rs proto_header 文档）
            "4096".into(), // 块大小
            "-p".into(),
            proto_path.to_string_lossy().into_owned(),
        ],
        cwd: Some(layout.os_root.clone()), // 原型里的宿主文件路径相对 os/ 根
        note: "mkfs_mfs 播种 imgrd（装机消费 S32 决定半）",
    });
    actions.push(Action::Copy {
        from: imgrd,
        to: staging.join("EFI/minix/imgrd"),
        note: "imgrd 入包（消费通道归 NS6 设计）",
    });

    // 8. FAT 盘镜像组装（mtools；与 run_qemu.sh 的真盘镜像优先路径同词汇，
    //    run_qemu.sh:77-86）。
    actions.push(Action::Tool {
        program: "dd".into(),
        args: vec![
            "if=/dev/zero".into(),
            format!("of={}", esp_image.to_string_lossy()),
            "bs=1M".into(),
            "count=64".into(),
            "status=none".into(),
        ],
        cwd: None,
        note: "64MiB 空盘",
    });
    actions.push(Action::Tool {
        program: "mkfs.vfat".into(),
        args: vec![esp_image.to_string_lossy().into_owned()],
        cwd: None,
        note: "FAT 文件系统",
    });
    let mut mmd_args = vec!["-i".to_string(), esp_image.to_string_lossy().into_owned()];
    mmd_args.extend([
        "::EFI".to_string(),
        "::EFI/BOOT".to_string(),
        "::EFI/minix".to_string(),
        "::EFI/minix/modules".to_string(),
    ]);
    actions.push(Action::Tool {
        program: "mmd".into(),
        args: mmd_args,
        cwd: None,
        note: "ESP 目录树",
    });

    // 9. boot-shim 入包为固件默认加载项 + 全部文件 mcopy 进 ESP。
    actions.push(Action::Copy {
        from: layout
            .target_root
            .join("x86_64-unknown-uefi")
            .join(profile)
            .join("boot-shim.efi"),
        to: staging.join("EFI/BOOT/BOOTX64.EFI"),
        note: "boot-shim 入包为固件默认加载项",
    });
    let mut copy_pairs: Vec<(PathBuf, String)> = vec![(
        staging.join("EFI/minix/kernel.elf"),
        "::EFI/minix/kernel.elf".into(),
    )];
    copy_pairs.push((staging.join("EFI/minix/imgrd"), "::EFI/minix/imgrd".into()));
    for entry in crate::manifest::BOOT_MODULES.iter() {
        copy_pairs.push((
            modules_dir.join(entry.module),
            format!("::EFI/minix/modules/{}", entry.module),
        ));
    }
    for (from, to) in copy_pairs {
        actions.push(Action::Tool {
            program: "mcopy".into(),
            args: vec![
                "-i".into(),
                esp_image.to_string_lossy().into_owned(),
                from.to_string_lossy().into_owned(),
                to,
            ],
            cwd: None,
            note: "文件入 ESP",
        });
    }

    Ok((actions, esp_image))
}

/// 生成 /etc 最小集的 mkfs 原型文本。
///
/// 文法（`os/fs/mfs/src/mkfs.rs` 的 proto_header/build_image_seeded）：
/// 第一行引导名读到即弃；第二行 `块数 inode数`；第三行是**根目录模式
/// 行**（仅 mode uid gid，无名字）；其后是根目录条目，目录递归以 `$`
/// 收口；设备行 `名 c--权限 uid gid major minor`。console 设备号对位
/// C dmap 表的 TTY_MAJOR=4（`minix3/minix/include/minix/dmap.h:25`，
/// ttys 族），惯例次设备号 0。
pub fn generate_etc_proto(etc_dir: &Path) -> Result<Vec<u8>> {
    // 存在性在此验证；内容由 mkfs_mfs 的 StdHost 播种时从宿主读取。
    for required in ["rc", "ttys"] {
        let path = etc_dir.join(required);
        if !path.is_file() {
            bail!(
                "/etc 最小集缺件：{} 不存在（装机面不造缺 rc/ttys 的镜像）",
                path.display()
            );
        }
    }

    let proto = "minix-rs imgrd\n\
                 2048 0\n\
                 d--755 0 0\n\
                 etc d--755 0 0\n\
                 rc ---755 0 0 etc/rc\n\
                 ttys ---644 0 0 etc/ttys\n\
                 $\n\
                 dev d--755 0 0\n\
                 console c--600 0 0 4 0\n\
                 $\n\
                 $\n";
    Ok(proto.as_bytes().to_vec())
}

/// 执行装配计划。`dry_run` 只打印不落盘。
pub fn execute(actions: &[Action], dry_run: bool) -> Result<()> {
    for action in actions {
        match action {
            Action::Cargo { args, note } => {
                println!("  [cargo] {note}");
                if dry_run {
                    println!("    > cargo {}", args.join(" "));
                    continue;
                }
                run_inherit(Command::new("cargo").args(args))
                    .with_context(|| format!("cargo {} 失败", args.join(" ")))?;
            }
            Action::Mkdir { path } => {
                println!("  [mkdir] {}", path.display());
                if !dry_run {
                    std::fs::create_dir_all(path)
                        .with_context(|| format!("建目录失败：{}", path.display()))?;
                }
            }
            Action::Write { path, bytes, note } => {
                println!("  [write] {note} → {}", path.display());
                if dry_run {
                    continue;
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("建父目录失败：{}", parent.display()))?;
                }
                std::fs::write(path, bytes)
                    .with_context(|| format!("写文件失败：{}", path.display()))?;
            }
            Action::Copy { from, to, note } => {
                println!("  [copy] {note}: {} → {}", from.display(), to.display());
                if dry_run {
                    continue;
                }
                if !from.exists() {
                    bail!(
                        "装配缺件（fail-fast，对位 boot-shim loader 的缺件 panic）：{}",
                        from.display()
                    );
                }
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("建父目录失败：{}", parent.display()))?;
                }
                std::fs::copy(from, to)
                    .with_context(|| format!("复制失败：{} → {}", from.display(), to.display()))?;
            }
            Action::Tool {
                program,
                args,
                cwd,
                note,
            } => {
                println!("  [tool] {note}");
                println!("    > {} {}", program, args.join(" "));
                if dry_run {
                    continue;
                }
                let mut cmd = Command::new(program);
                cmd.args(args);
                if let Some(dir) = cwd {
                    cmd.current_dir(dir);
                }
                run_inherit(&mut cmd).with_context(|| format!("{program} 执行失败"))?;
            }
        }
    }
    Ok(())
}

fn run_inherit(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("无法启动 {}", cmd.get_program().to_string_lossy()))?;
    if !status.success() {
        bail!(
            "{} 以非零状态退出：{}",
            cmd.get_program().to_string_lossy(),
            status
        );
    }
    Ok(())
}

/// `xtask image` 入口。
pub fn run(release: bool, arch: Arch, kernel_override: Option<&Path>, dry_run: bool) -> Result<()> {
    let layout = Layout::from_env();
    let (actions, esp_image) = plan(arch, release, kernel_override, &layout)?;
    if dry_run {
        println!("📦 镜像装配计划（dry-run，{arch:?}）：");
    } else {
        println!("📦 装配 {arch:?} 镜像…");
    }
    execute(&actions, dry_run)?;
    if dry_run {
        println!("✅ 计划打印完毕（未落盘）。产出位：{}", esp_image.display());
    } else {
        println!("✅ 镜像就绪：{}", esp_image.display());
        println!("   布局：/EFI/BOOT/BOOTX64.EFI + /EFI/minix/{{kernel.elf, imgrd, modules/×12}}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 极简临时目录助手（不引 dev 依赖，std 组合即可）。
    mod tempdir {
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicU64, Ordering};

        static SEQ: AtomicU64 = AtomicU64::new(0);

        pub struct TempDir(pub PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        pub fn create() -> (super::super::Layout, TempDir) {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let base =
                std::env::temp_dir().join(format!("xtask-image-test-{}-{n}", std::process::id()));
            std::fs::create_dir_all(base.join("etc")).expect("建临时 etc");
            std::fs::write(base.join("etc/rc"), "#!/bin/sh\n").expect("写 rc");
            std::fs::write(
                base.join("etc/ttys"),
                "console \"/sbin/getty default\" minix on secure\n",
            )
            .expect("写 ttys");
            let layout = super::super::Layout {
                os_root: base.clone(),
                target_root: base.join("target"),
            };
            (layout, TempDir(base))
        }
    }

    /// 计划完整性：构建/装机/播种/ESP 组装各环节齐全，生产开关在位。
    #[test]
    fn plan_x86_64_contains_all_contract_steps() {
        let (layout, _guard) = tempdir::create();
        let (actions, esp) = plan(Arch::X86_64, true, None, &layout).unwrap();

        let text = format!("{actions:?}");
        // 12 模块逐包构建 + 逐件装机（抽三个代表断言，全量计数见下）。
        for pkg in ["minix-ds", "minix-init", "minix-fs-mfs", "minix-driver-tty"] {
            assert!(text.contains(pkg), "装机计划应含 {pkg}");
        }
        assert_eq!(
            (actions
                .iter()
                .filter(|a| matches!(a, Action::Cargo { .. }))
                .count()),
            14,
            "12 模块 + boot-shim + mkfs_mfs = 14 步 cargo 构建"
        );
        // 生产开关与 uefi 目标。
        assert!(text.contains("fw-x86-uefi") && text.contains("x86_64-unknown-uefi"));
        assert!(
            text.contains("--no-default-features"),
            "boot-shim 构建必须关默认 feature"
        );
        // mkfs_mfs 播种与 imgrd 入包。
        assert!(text.contains("mkfs_mfs") && text.contains("imgrd"));
        // 内核 ELF 契约位与 ESP 产出位。
        assert!(text.contains("kernel.elf"));
        assert!(esp.ends_with("minix.img"));
        // mtools 组装段在位。
        assert!(text.contains("mmd") && text.contains("mcopy") && text.contains("mkfs.vfat"));
    }

    /// 非 x86_64 架构 honest bail：装机面不装不可启动机的镜像。
    #[test]
    fn plan_bails_honestly_for_arch_without_bootshim_output() {
        let (layout, _guard) = tempdir::create();
        assert!(plan(Arch::Aarch64, true, None, &layout).is_err());
        assert!(plan(Arch::Riscv64, true, None, &layout).is_err());
    }

    /// 原型文法：两行头 + 目录递归收口 + console 设备行（major 4）。
    #[test]
    fn etc_proto_grammar_and_console_device_line() {
        let (layout, _guard) = tempdir::create();
        let proto =
            String::from_utf8(generate_etc_proto(&layout.os_root.join("etc")).unwrap()).unwrap();
        let mut lines = proto.lines();
        assert_eq!(
            lines.next().unwrap(),
            "minix-rs imgrd",
            "第一行引导名（读到即弃）"
        );
        assert_eq!(
            lines.next().unwrap(),
            "2048 0",
            "第二行 = 块数 inode数（0 = 缺省阶梯）"
        );
        assert_eq!(
            lines.next().unwrap(),
            "d--755 0 0",
            "第三行 = 根目录模式行（无名字）"
        );
        let body: Vec<&str> = lines.collect();
        assert_eq!(
            body.iter().filter(|l| **l == "$").count(),
            3,
            "etc/dev/root 三层收口"
        );
        assert!(
            body.contains(&"console c--600 0 0 4 0"),
            "console = char special major 4（dmap.h:25 TTY_MAJOR）"
        );
        assert!(
            body.contains(&"rc ---755 0 0 etc/rc"),
            "rc 经宿主路径 etc/rc 播种"
        );
    }

    /// /etc 最小集缺件时原型生成必须报错（装机面不造缺 rc 的镜像）。
    #[test]
    fn etc_proto_requires_rc_and_ttys() {
        let (layout, guard) = tempdir::create();
        std::fs::remove_file(guard.0.join("etc/rc")).unwrap();
        assert!(generate_etc_proto(&layout.os_root.join("etc")).is_err());
    }
}
