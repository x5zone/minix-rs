//! `xtask image` — assembles build artifacts into a bootable machine image
//! (E-IMGPKG).
//!
//! Package shape (UEFI path): one FAT disk image containing
//!   - `/EFI/BOOT/<firmware default load option>` <- boot-shim (x86_64 =
//!     `BOOTX64.EFI`, aarch64 = `BOOTAA64.EFI`, see [`Arch::uefi_slots`])
//!   - `/EFI/minix/kernel.elf` <- kernel ELF (boot-shim reads it per
//!     `loader::KERNEL_PATH`, `os/boot-shim/src/loader.rs:31`)
//!   - `/EFI/minix/modules/<name>` x12 <- install manifest ([`crate::manifest`])
//!   - `/EFI/minix/imgrd` <- mfs root disk (mkfs_mfs seeds the minimal `/etc`
//!     set). E-IMGPKG: mfs's build.rs include_bytes!s the imgrd bytes into the
//!     BOOT_IMGRD static at compile time; the consumption path is in place.
//!
//! Same fail-fast semantics as boot-shim: a missing module is named at assembly
//! time (counterpart of the run-time check at
//! `os/boot-shim/src/loader.rs:280-288`), so a half-packed image never escapes.
//!
//! Production switch: boot-shim is built with `--no-default-features --features
//! fw-x86-uefi` (aarch64: `fw-aarch64-uefi`) — the feature vocabulary — and the
//! kernel half linked into boot-shim therefore carries no mock
//! (`boot-shim/Cargo.toml` pins `default-features = false` for minix-kernel).
//! Each module is a separate cargo invocation — this avoids the feature
//! unification trap (one multi-`-p` invocation would pull Mock and production
//! types into the same compilation) and names the failing module.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Target architecture. x86_64 and aarch64 have a UEFI install surface;
/// riscv64 boots via U-Boot fatload and has no UEFI disk layout, so assembly
/// bails honestly instead of producing an unbootable image.
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

    /// Build target triple for modules (user land).
    fn module_target(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-none",
            Self::Aarch64 => "aarch64-unknown-none",
            Self::Riscv64 => "riscv64gc-unknown-none-elf",
        }
    }

    /// Output directory name.
    pub fn slug(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }

    /// The four per-architecture UEFI install-surface constants. `None` = this
    /// architecture has no UEFI disk layout.
    ///
    /// These values used to be hardcoded in x86 form across the assembly plan;
    /// enabling aarch64 collects them into one table. The `loader` name is not
    /// arbitrary: the UEFI spec fixes the default load-option filename per CPU
    /// architecture, and AAVMF only recognizes `EFI/BOOT/BOOTAA64.EFI`.
    fn uefi_slots(self) -> Option<UefiSlots> {
        match self {
            Self::X86_64 => Some(UefiSlots {
                shim_target: "x86_64-unknown-uefi",
                shim_feature: "fw-x86-uefi",
                kernel_none_feature: "fw-x86-none",
                loader: "BOOTX64.EFI",
            }),
            Self::Aarch64 => Some(UefiSlots {
                shim_target: "aarch64-unknown-uefi",
                shim_feature: "fw-aarch64-uefi",
                kernel_none_feature: "fw-aarch64-none",
                loader: "BOOTAA64.EFI",
            }),
            Self::Riscv64 => None,
        }
    }
}

/// UEFI install-surface constants for one architecture (see
/// [`Arch::uefi_slots`]).
#[derive(Clone, Copy, Debug)]
struct UefiSlots {
    /// Build target triple for boot-shim.
    shim_target: &'static str,
    /// Public feature name of boot-shim (production switch).
    shim_feature: &'static str,
    /// Public feature name of `kernel-image` (same target triple as
    /// [`Arch::module_target`], which [`kernel_elf_path`] already assumes when
    /// picking the artifact).
    kernel_none_feature: &'static str,
    /// Firmware default load-option filename under `/EFI/BOOT/` on the ESP.
    loader: &'static str,
}

/// One step of the assembly plan. The plan comes first, execution second —
/// `--dry-run` prints the same plan, and unit tests assert on the plan
/// directly.
#[derive(Debug)]
pub enum Action {
    /// Invoke cargo at the workspace root (build steps).
    Cargo {
        args: Vec<String>,
        note: &'static str,
    },
    /// Create a directory.
    Mkdir { path: PathBuf },
    /// Write text to disk (e.g. a generated prototype file).
    Write {
        path: PathBuf,
        bytes: Vec<u8>,
        note: &'static str,
    },
    /// Copy an artifact into staging.
    Copy {
        from: PathBuf,
        to: PathBuf,
        note: &'static str,
    },
    /// External tool (mkfs_mfs / mtools).
    Tool {
        program: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        note: &'static str,
    },
    /// External tool whose failure is contractually normal: the artifact is
    /// optional by the consumer's protocol (e.g. the aarch64 machine DTB —
    /// its absence on the ESP simply means the boot-shim keeps the UEFI
    /// GetMemoryMap source). A missing binary, a nonzero exit or a timeout
    /// skips the artifact instead of failing the assembly; the consumers'
    /// gates enforce the presence where it matters.
    ToolBestEffort {
        program: String,
        args: Vec<String>,
        note: &'static str,
    },
}

/// Path environment needed for assembly. Everything is passed in by the caller
/// for testability.
pub struct Layout {
    /// Workspace root (`os/`).
    pub os_root: PathBuf,
    /// Cargo target root (`CARGO_TARGET_DIR` or `os/target`).
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

/// Path from which the kernel ELF is taken. Producer = the `kernel-image`
/// package: bin name `kernel`, and cargo emits an extension-less artifact for
/// none targets at `target/<target>/<profile>/kernel` — the default path
/// matches that; the ESP-side install name stays `/EFI/minix/kernel.elf` (the
/// `loader::KERNEL_PATH` contract slot is unchanged). `--kernel` may point at
/// an externally produced ELF (for custom kernel build flows).
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
            .join("kernel"),
    }
}

/// Build the assembly plan, returning (action sequence, ESP disk image output
/// path).
pub fn plan(
    arch: Arch,
    release: bool,
    kernel_override: Option<&Path>,
    layout: &Layout,
) -> Result<(Vec<Action>, PathBuf)> {
    // Honest about all three architectures: riscv64 boots via U-Boot fatload
    // + BootFileTable, not the UEFI install layout (carrier mechanism in
    // qemu-tests/test-riscv64-uboot.sh); giving it an ESP disk would ship an
    // unbootable image. The four x86_64 / aarch64 constants come from the
    // single [`Arch::uefi_slots`] table (the aarch64 leg depends on boot-shim's
    // fw-aarch64-uefi gate).
    let Some(uefi) = arch.uefi_slots() else {
        bail!(
            "{} 镜像暂不可装配：启动路径不是 UEFI 盘形（riscv64 走 U-Boot fatload + \
             BootFileTable，载体机制见 os/qemu-tests/test-riscv64-uboot.sh）。",
            arch.slug()
        );
    };

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

    // 1a. Build the 11 modules + sh package by package (guest user-land
    //     target; failures name the package). mfs is deferred until after imgrd
    //     generation (build.rs include_bytes!).
    for entry in crate::manifest::BOOT_MODULES
        .iter()
        .filter(|e| e.package != "minix-fs-mfs")
    {
        cargo_release(
            &mut actions,
            "装配清单模块构建（mfs 除外）",
            &["-p", entry.package, "--target", arch.module_target()],
        );
    }
    // /bin/sh and /bin/echo are not boot modules, but imgrd seeding needs
    // their ELF binaries (referenced by path in the proto). This rewrite's
    // shell currently has only two builtins, exit/cd (the eval arm of
    // commands/bin/shell/src/bin/sh.rs), so `echo marker` in rc takes the
    // external-command leg; C's ash does have an echo builtin (`echocmd` in
    // `minix3/bin/sh/builtins.def`, with no TINY/SMALL gate), but its images
    // also install /bin/echo separately (`minix3/bin/echo/`), so seeding it
    // matches C. Making echo a builtin is left until the shell grows its
    // builtin set. The disk used to seed only sh: once the init->sh exec leg
    // was fixed, rc's echo would hit ENOENT on every PATH entry -> sh exits
    // 127 -> runcom falls back to SingleUser. That exec-leg fix was the
    // proximate cause; this missing binary was the independent downstream
    // blocker.
    cargo_release(
        &mut actions,
        "sh/echo 命令构建（imgrd /bin 播种）",
        &[
            "-p",
            "minix-shell",
            "-p",
            "minix-fileops",
            "--target",
            arch.module_target(),
        ],
    );

    // DS transfer-chain carrier for the on-metal half of E5(f)
    // (`qemu-tests/test-kernels/user/e5-ds`). Same freestanding leg as sh/echo,
    // but this package has `required-features = ["fw-user-none"]` (bare-metal
    // isolation: a host `--workspace --bins` run skips it), so --target and
    // --features must be passed together — the rt-birth/sysboot precedent.
    //
    // Installed only for enabled architectures ([`E5_DS_READY`], same list shape
    // as `ATF_BOOT_LEG_READY`): when the carrier's first taskcall hits "DS not
    // serving yet" it blocks intrinsically in sendrec, so installing it on an
    // architecture that has not passed on-metal acceptance would break that
    // architecture's existing smoke gate. The `/bin/e5-ds` line in rc is not
    // enabled together with the list (if not seeded it just prints one extra
    // "not found"); the list extends to all three architectures alongside
    // E5-ARCH.
    let e5_ds_ready = E5_DS_READY.contains(&arch.slug());
    if e5_ds_ready {
        cargo_release(
            &mut actions,
            "e5-ds 载体构建（imgrd /bin 播种，E5(f)）",
            &[
                "-p",
                "e5-ds",
                "--target",
                arch.module_target(),
                "--features",
                "fw-user-none",
            ],
        );
    }

    // 2. boot-shim (production switch: default features off, only this
    //    architecture's uefi feature on).
    cargo_release(
        &mut actions,
        "boot-shim UEFI 载体构建（生产开关）",
        &[
            "-p",
            "boot-shim",
            "--target",
            uefi.shim_target,
            "--no-default-features",
            "--features",
            uefi.shim_feature,
        ],
    );

    // 2b. Production kernel image (`-p kernel-image`, bin gate fw-<arch>-none,
    //     producing `target/<none target>/<profile>/kernel`).
    cargo_release(
        &mut actions,
        "生产内核镜像构建（kernel-image，NS8-A）",
        &[
            "-p",
            "kernel-image",
            "--target",
            arch.module_target(),
            "--features",
            uefi.kernel_none_feature,
        ],
    );

    // 3. mkfs_mfs host tool (imgrd generator; host triple).
    cargo_release(
        &mut actions,
        "mkfs_mfs 宿主工具构建",
        &["-p", "minix-diskfmt"],
    );

    // 3b. /etc prototype + mkfs_mfs seeding imgrd (before the mfs build —
    //     build.rs needs imgrd in place).
    let proto_path = image_dir.join("imgrd.proto");
    let imgrd = image_dir.join("imgrd.img");
    let bin_rel = |name: &str| format!("target/{}/{}/{name}", arch.module_target(), profile);
    // ATF C test seeding: cross-compiled ELFs are not part of the cargo
    // assembly chain (tools/build-atf-test.sh produces them), so they are
    // seeded only when the file exists; the first case is t_memchr (pure memory,
    // no fork/exec/VFS bridge) plus the p1/p2/p3 on-metal bisect probes
    // (tools/atf-c-compat/probes/, covering the bare bridge / stdio / malloc
    // legs respectively). They land on disk as /tests/<name> and rc execs them
    // at the end. Single source: the case table and rc injection rules live in
    // [`collect_atf_face`] (the `xtask atf-face` subcommand consumes the same
    // function — riscv64 does not use UEFI disk-layout assembly, so it takes the
    // same rc variant + manifest through that subcommand).
    let AtfFace { tests: atf_tests, rc_execs } = collect_atf_face(arch, layout);
    // Gate manifest (one "prog tc" per line) = the rc exec lines with the
    // `/tests/` prefix stripped (same sequence, no second join, so the two
    // cannot drift apart).
    let atf_plan: Vec<String> = rc_execs
        .iter()
        .map(|line| line.trim_start_matches("/tests/").to_string())
        .collect();
    // rc variant: the suite exec lines are injected just before `exit 0`
    // (os/etc/rc stays the static sentinel; the suite surface is single-sourced
    // in ATF_SUITE + the minix3 C sources).
    let rc_variant_rel = format!("target/image/{}/rc.imgrd", arch.slug());
    let rc_host: Option<&str> = if rc_execs.is_empty() {
        None
    } else {
        let base = std::fs::read_to_string(layout.os_root.join("etc/rc"))?;
        actions.push(Action::Write {
            path: image_dir.join("rc.imgrd"),
            bytes: inject_rc_execs(&base, &rc_execs).into_bytes(),
            note: "rc 变体（ATF 套件 exec 行注入 exit 0 前）",
        });
        Some(rc_variant_rel.as_str())
    };
    if !atf_plan.is_empty() {
        actions.push(Action::Write {
            path: image_dir.join("atf-plan.txt"),
            bytes: {
                let mut s = atf_plan.join("\n");
                s.push('\n');
                s
            }
            .into_bytes(),
            note: "ATF 门 manifest（prog tc 逐行，判数唯一真源）",
        });
    }
    let atf_refs: Vec<(&str, &str)> =
        atf_tests.iter().map(|(n, p)| (n.as_str(), p.as_str())).collect();
    // Root-disk capacity grows with suite presence (architectures without
    // suites stay byte-identical to before).
    let imgrd_blocks = if atf_tests.is_empty() { IMGRD_BLOCKS } else { IMGRD_BLOCKS_SUITE };
    let mut bins: Vec<(&str, String)> = vec![
        ("sh", bin_rel("sh")),
        ("echo", bin_rel("echo")),
        ("ls", bin_rel("ls")),
        ("cat", bin_rel("cat")),
        // `du` is read-only and not gated by the read-only imgrd root disk:
        // its on-disk readings are checked by the du assertions in
        // test-cmd-smoke*.
        ("du", bin_rel("du")),
        // `stat` is likewise read-only: the on-disk shape of all three
        // templates is covered by the stat assertions in test-cmd-smoke*, over
        // the same lstat IPC leg.
        ("stat", bin_rel("stat")),
        // `find` is read-only: the whole parse->walk->eval->print leg for file
        // operands is covered out to the serial port by the find assertions in
        // test-cmd-smoke*; the directory-walk leg is still blocked by the
        // getdents gap on the imgrd root disk (same as du/ls), so only file
        // operands are asserted.
        ("find", bin_rel("find")),
    ];
    if e5_ds_ready {
        bins.push(("e5-ds", bin_rel("e5-ds")));
    }
    let bin_refs: Vec<(&str, &str)> = bins.iter().map(|(n, p)| (*n, p.as_str())).collect();
    actions.push(Action::Write {
        path: proto_path.clone(),
        bytes: generate_etc_proto(
            &layout.os_root.join("etc"),
            &bin_refs,
            &atf_refs,
            rc_host,
            imgrd_blocks,
        )?,
        note: "imgrd 原型文件（/etc 最小集 + /bin/{sh,echo,ls,cat,du[,e5-ds]} + /tests + /dev/console）",
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
            imgrd_blocks.to_string(), // block count: x4KiB, chosen by suite presence (see const note)
            "0".into(),    // inode count 0 = mkfs default ladder (mkfs.rs proto_header docs)
            "4096".into(), // block size
            "-p".into(),
            proto_path.to_string_lossy().into_owned(),
        ],
        cwd: Some(layout.os_root.clone()),
        note: "mkfs_mfs 播种 imgrd（B11：先于 mfs 构建）",
    });

    // 3c. mfs build (build.rs can now find target/image/<arch>/imgrd.img and
    //     feed it to BOOT_IMGRD via include_bytes!). This is where E-IMGPKG
    //     lands.
    cargo_release(
        &mut actions,
        "mfs 构建（imgrd include_bytes!，B11）",
        &["-p", "minix-fs-mfs", "--target", arch.module_target()],
    );

    // 4. staging directory tree.
    let modules_dir = staging.join("EFI/minix/modules");
    actions.push(Action::Mkdir {
        path: modules_dir.clone(),
    });
    actions.push(Action::Mkdir {
        path: staging.join("EFI/BOOT"),
    });

    // 5. Take and install each of the 12 modules (missing files fail fast and
    //    are named).
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

    // 6. Kernel ELF (picked by path; produced by the kernel-image package).
    let kernel = kernel_elf_path(layout, kernel_override, arch, release);
    actions.push(Action::Copy {
        from: kernel,
        to: staging.join("EFI/minix/kernel.elf"),
        note: "内核 ELF 装机（loader::KERNEL_PATH 契约位）",
    });

    // 7. imgrd into staging (generation already happened in step 3b; this only
    //    copies).
    actions.push(Action::Copy {
        from: imgrd,
        to: staging.join("EFI/minix/imgrd"),
        note: "imgrd 入包 staging（供 ESP mcopy 消费）",
    });

    // 8. FAT disk-image assembly (mtools; same vocabulary as run_qemu.sh's
    //    real-disk-image preferred path, run_qemu.sh:77-86).
    actions.push(Action::Tool {
        program: "dd".into(),
        args: vec![
            "if=/dev/zero".into(),
            format!("of={}", esp_image.to_string_lossy()),
            "bs=1M".into(),
            "count=128".into(),
            "status=none".into(),
        ],
        cwd: None,
        note: "128MiB 空盘（B11：mfs 含 8MB imgrd 嵌入后 ESP 总需 >86MB）",
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

    // 9. boot-shim packaged as the firmware default load option + every file
    //    mcopy'd into the ESP.
    let loader_path = staging.join(format!("EFI/BOOT/{}", uefi.loader));
    actions.push(Action::Copy {
        from: layout
            .target_root
            .join(uefi.shim_target)
            .join(profile)
            .join("boot-shim.efi"),
        to: loader_path.clone(),
        note: "boot-shim 入包为固件默认加载项",
    });
    // boot-shim itself is an ESP file too: staging only fills the work tree,
    // while the firmware loads ::/EFI/BOOT/<this architecture's default load
    // option> from the disk (omitting it in the first assembly attempt landed
    // in the UEFI Shell, as the first test-cmd-smoke run showed). startup.nsh
    // is this firmware's auto-boot belt-and-suspenders (test-sysboot precedent:
    // it runs the lines one by one after the countdown).
    let startup_nsh = format!("echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\n{}\r\n", uefi.loader);
    actions.push(Action::Write {
        path: staging.join("startup.nsh"),
        bytes: startup_nsh.into_bytes(),
        note: "startup.nsh（固件自动引导脚本，test-sysboot 判例）",
    });
    let mut copy_pairs: Vec<(PathBuf, String)> = vec![
        (loader_path, format!("::EFI/BOOT/{}", uefi.loader)),
        (staging.join("startup.nsh"), "::startup.nsh".into()),
        (
            staging.join("EFI/minix/kernel.elf"),
            "::EFI/minix/kernel.elf".into(),
        ),
    ];
    copy_pairs.push((staging.join("EFI/minix/imgrd"), "::EFI/minix/imgrd".into()));
    for entry in crate::manifest::BOOT_MODULES.iter() {
        copy_pairs.push((
            modules_dir.join(entry.module),
            format!("::EFI/minix/modules/{}", entry.module),
        ));
    }
    // SD-3 D1 (C17): aarch64 images ship their QEMU machine DTB on the ESP
    // root (/dumpdtb.dtb). The boot-shim optionally reads it (loader::
    // DUMPDTB_PATH) as the gated DTB source probe — v1 semantics: gate plus
    // a source line on the console, the memmap itself stays
    // UEFI-GetMemoryMap (the DTB's /memory includes the shim's own
    // LOADER_DATA pages, so a source swap needs the self-allocation
    // accounting that is still under design). QEMU with dumpdtb does not
    // exit on its own, hence the timeout wrapper and the best-effort
    // action: no qemu-aarch64 on the host simply means the ESP carries no
    // dumpdtb (the shim's absence path).
    if matches!(arch, Arch::Aarch64) {
        let dumpdtb = staging.join("dumpdtb.dtb");
        let _ = std::fs::remove_file(&dumpdtb);
        actions.push(Action::ToolBestEffort {
            program: "timeout".into(),
            args: vec![
                "15".into(),
                "qemu-system-aarch64".into(),
                "-machine".into(),
                format!(
                    "virt,gic-version=3,dumpdtb={}",
                    dumpdtb.to_string_lossy()
                ),
                "-cpu".into(),
                "max".into(),
                "-m".into(),
                "512M".into(),
                "-display".into(),
                "none".into(),
            ],
            note: "machine DTB for ESP /dumpdtb.dtb (boot-shim gated source)",
        });
        copy_pairs.push((dumpdtb, "::dumpdtb.dtb".into()));
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

/// First batch of ATF suites: on-disk program name -> minix3 C source
/// (repo-root relative). ELFs are cross-compiled by tools/build-atf-test.sh
/// (same name under target/atf/<arch>/tests/); case names are extracted at
/// assembly time from the `ATF_TC(name);` declarations in the source (no
/// separate list — the single source of truth is the C source). The p1/p2/p3
/// bridge sentinels are not in this table (they have no ATF cases; their fixed
/// exec lines live in os/etc/rc).
const ATF_SUITE: &[(&str, &str)] = &[
    ("t_bm", "minix3/tests/lib/libc/string/t_bm.c"),
    ("t_memchr", "minix3/tests/lib/libc/string/t_memchr.c"),
    ("t_memcpy", "minix3/tests/lib/libc/string/t_memcpy.c"),
    ("t_memmem", "minix3/tests/lib/libc/string/t_memmem.c"),
    ("t_memset", "minix3/tests/lib/libc/string/t_memset.c"),
    ("t_popcount", "minix3/tests/lib/libc/string/t_popcount.c"),
    ("t_strcat", "minix3/tests/lib/libc/string/t_strcat.c"),
    ("t_strchr", "minix3/tests/lib/libc/string/t_strchr.c"),
    ("t_strcmp", "minix3/tests/lib/libc/string/t_strcmp.c"),
    ("t_strcpy", "minix3/tests/lib/libc/string/t_strcpy.c"),
    ("t_strcspn", "minix3/tests/lib/libc/string/t_strcspn.c"),
    ("t_strerror", "minix3/tests/lib/libc/string/t_strerror.c"),
    ("t_stresep", "minix3/tests/lib/libc/string/t_stresep.c"),
    ("t_strlen", "minix3/tests/lib/libc/string/t_strlen.c"),
    ("t_strpbrk", "minix3/tests/lib/libc/string/t_strpbrk.c"),
    ("t_strrchr", "minix3/tests/lib/libc/string/t_strrchr.c"),
    ("t_strspn", "minix3/tests/lib/libc/string/t_strspn.c"),
    ("t_swab", "minix3/tests/lib/libc/string/t_swab.c"),
];

/// Extract registered case names from an atf C source (`ATF_TC(name);`
/// declaration lines; HEAD/BODY definition lines do not count twice). Pure
/// function, pinned by host unit tests; when no name is extracted the caller
/// must treat it as an assembly failure.
pub fn extract_atf_tc_names(src: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("ATF_TC(") {
            if let Some(end) = rest.find(')') {
                let name = rest[..end].trim();
                if !name.is_empty() && !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// Inject the test exec lines before the `exit 0` line of the rc text (the last
/// line, if present); without `exit 0` they are appended at the end. Pure
/// function so unit tests can pin the insertion-point semantics.
pub fn inject_rc_execs(rc_text: &str, exec_lines: &[String]) -> String {
    if exec_lines.is_empty() {
        return rc_text.to_string();
    }
    let mut out = String::new();
    let mut injected = false;
    for line in rc_text.lines() {
        if !injected && line.trim() == "exit 0" {
            for l in exec_lines {
                out.push_str(l);
                out.push('\n');
            }
            injected = true;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !injected {
        for l in exec_lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// imgrd root-disk block count (x 4KiB): without suites the old value 2048
/// (=8MiB) is kept, so x86/riscv artifacts stay byte-identical to before and
/// unverified architectures do not grow their footprint; the first ATF suite's
/// 18 static ELFs (~410KB each, including libc + libatf-c) exceed the old
/// budget, so the caller passes 4096 (=16MiB; the 128MiB ESP has room, and
/// bootface's direct-copy bump allocation has no fixed cap). The proto header
/// and the mkfs argument share the same value (one place, no longer hand-written
/// twice).
pub const IMGRD_BLOCKS: u32 = 2048;
pub const IMGRD_BLOCKS_SUITE: u32 = 4096;

/// Architectures on which the E5(f) DS transfer-chain carrier (`/bin/e5-ds`) is
/// enabled, same shape as [`ATF_BOOT_LEG_READY`]. Today only x86_64 is enabled:
/// it is the only leg of the command surface / service chain that has passed
/// on-metal acceptance; when the carrier's first taskcall hits "DS not serving
/// yet" it blocks intrinsically in sendrec, so installing it on an unaccepted
/// architecture would break that architecture's existing smoke gate. Extending
/// to aarch64/riscv64 follows the three-architecture rerun of E5-ARCH.
const E5_DS_READY: &[&str] = &["x86_64"];

/// Whitelist of architectures where the custom birth leg (`-nostartfiles` +
/// startup-minix.c) is enabled. riscv64 is enabled in the same shape as aarch64
/// after the exec-stack-top Sv39 canonical-address fix — before that the riscv
/// leg linked picolibc's default crt0 (sp switched to the ELF's internal .stack
/// + main(0,NULL)), which was ruled broken. x86_64 joins once its guest leg
/// stopped linking glibc: the host cc + glibc startup needs a per-process FS
/// base (`arch_prctl(ARCH_SET_FS)`) that this kernel does not carry, so the
/// leg now links the same non-TLS picolibc as the other two and supplies the
/// x86 `_start` from startup-minix.c.
const ATF_BOOT_LEG_READY: &[&str] = &["aarch64", "riscv64", "x86_64"];

/// Collected ATF on-metal surface (the return struct of [`collect_atf_face`]).
///
/// A dedicated struct rather than a tuple: the two fields mean different things
/// (seed list vs rc injection sequence), and the tuple form would trip
/// `clippy::type_complexity` (`xtask check` runs with `-D warnings`).
struct AtfFace {
    /// `(prog, host-relative path)` — the imgrd seed list for `/tests/<prog>`
    /// (including the p1/p2/p3 bridge sentinels, seeded only when present).
    tests: Vec<(String, String)>,
    /// `/tests/<prog> <tc>` exec lines (the rc-variant injection sequence;
    /// atf-plan is derived from them by stripping the `/tests/` prefix, with no
    /// second join).
    rc_execs: Vec<String>,
}

/// Collect one architecture's ATF on-metal surface (single source).
///
/// The case table comes from [`ATF_SUITE`] (18 cases), and for each prog the
/// `ATF_TC(name)` declarations are extracted from the minix3 C source — an
/// empty extraction refuses silent assembly (panic rather than Err: this is a
/// "declaration vs extractor drift" incident on the assembly surface and must
/// be loud enough for a human to see, not a recoverable IO failure). The
/// `p1/p2/p3` bridge sentinels are appended unconditionally (seeded only when
/// present). `plan()` (UEFI disk-layout assembly) and [`atf_face()`] (the
/// artifact surface for non-UEFI boot shapes) share this function, so the rc
/// injection sequence and the manifest always come from the same source.
fn collect_atf_face(arch: Arch, layout: &Layout) -> AtfFace {
    let mut tests: Vec<(String, String)> = Vec::new();
    let mut rc_execs: Vec<String> = Vec::new();
    if !ATF_BOOT_LEG_READY.contains(&arch.slug()) {
        return AtfFace { tests, rc_execs };
    }
    let atf_test_rel = |name: &str| format!("target/atf/{}/tests/{name}", arch.slug());
    for (prog, src_rel) in ATF_SUITE {
        let elf = layout.os_root.join(atf_test_rel(prog));
        if !elf.is_file() {
            continue;
        }
        let src = layout.os_root.join("..").join(src_rel);
        let src_text = std::fs::read_to_string(&src)
            .unwrap_or_else(|e| panic!("ATF 套件 {prog}：读源 {} 失败：{e}", src.display()));
        let tcs = extract_atf_tc_names(&src_text);
        assert!(
            !tcs.is_empty(),
            "ATF 套件 {prog}：{} 未提取到任何 `ATF_TC(name);`——声明面与提取器漂移，拒绝静默装配",
            src.display()
        );
        tests.push((prog.to_string(), atf_test_rel(prog)));
        for tc in &tcs {
            rc_execs.push(format!("/tests/{prog} {tc}"));
        }
    }
    for name in ["p1", "p2", "p3"] {
        if layout.os_root.join(atf_test_rel(name)).is_file() {
            tests.push((name.to_string(), atf_test_rel(name)));
        }
    }
    AtfFace { tests, rc_execs }
}

/// Generate the ATF on-metal surface artifacts (independent of UEFI disk-layout
/// assembly).
///
/// riscv64 boots by OpenSBI direct load + BootFileTable (see
/// `os/qemu-tests/test-riscv64-boot-full.sh`) and does not use `xtask image`'s
/// ESP layout; but the two single-source artifacts the ATF suite surface needs
/// must be exactly the same as for aarch64:
///   - `target/image/<arch>/rc.imgrd`: `os/etc/rc` + `/tests/<prog> <tc>` exec
///     lines (injected before `exit 0`, rules=[`inject_rc_execs`]);
///   - `target/image/<arch>/atf-plan.txt`: the one-`prog tc`-per-line manifest
///     (the sole source of the gate's expected count).
///
/// stdout contract (the caller `os/qemu-tests/test-atf-riscv64.sh` parses this):
///   - one `rc.execs=<N>` line (the number of injected exec lines);
///   - one `tests.<prog>=<host-rel-path>` line per item to seed (rel is the host
///     path relative to `os/`, used by proto generation to write the source file
///     of `/tests/<prog>`);
///   - with `--dry-run` it only prints, writing nothing.
pub fn atf_face(arch: Arch, dry_run: bool) -> Result<()> {
    if !ATF_BOOT_LEG_READY.contains(&arch.slug()) {
        bail!(
            "{} 未在 ATF 出生腿白名单内（ATF_BOOT_LEG_READY）——先放行该架构的 \
             -nostartfiles 出生腿",
            arch.slug()
        );
    }
    let layout = Layout::from_env();
    let AtfFace { tests, rc_execs } = collect_atf_face(arch, &layout);
    let image_dir = layout.target_root.join("image").join(arch.slug());
    if rc_execs.is_empty() {
        bail!(
            "{} 无任何 ATF 套件产物（target/atf/{}/tests 为空？）——先跑 \
             tools/build-atf-test.sh",
            arch.slug(),
            arch.slug()
        );
    }
    let base = std::fs::read_to_string(layout.os_root.join("etc/rc"))
        .context("读 os/etc/rc 失败")?;
    let rc_variant = inject_rc_execs(&base, &rc_execs);
    let mut plan = rc_execs
        .iter()
        .map(|l| l.trim_start_matches("/tests/").to_string())
        .collect::<Vec<_>>()
        .join("\n");
    plan.push('\n');
    if dry_run {
        println!("📦 ATF 面上机计划（dry-run，{arch:?}）：");
    } else {
        std::fs::create_dir_all(&image_dir)
            .with_context(|| format!("创建 {} 失败", image_dir.display()))?;
        std::fs::write(image_dir.join("rc.imgrd"), rc_variant.as_bytes())
            .with_context(|| format!("写 {}", image_dir.join("rc.imgrd").display()))?;
        std::fs::write(image_dir.join("atf-plan.txt"), plan.as_bytes())
            .with_context(|| format!("写 {}", image_dir.join("atf-plan.txt").display()))?;
        println!(
            "✅ ATF 面就绪：{} + {}",
            image_dir.join("rc.imgrd").display(),
            image_dir.join("atf-plan.txt").display()
        );
    }
    println!("rc.execs={}", rc_execs.len());
    for (prog, rel) in &tests {
        println!("tests.{prog}={rel}");
    }
    Ok(())
}

/// Generate the mkfs prototype text for the minimal /etc set.
///
/// Grammar (`proto_header`/`build_image_seeded` in `os/fs/mfs/src/mkfs.rs`):
/// the first line is a boot name that is read and discarded; the second line is
/// `block-count inode-count`; the third line is the **root directory mode
/// line** (mode uid gid only, no name); then come the root directory entries,
/// with directories closed recursively by `$`; a device line is
/// `name c--perms uid gid major minor`. The console device number matches
/// TTY_MAJOR=4 of C's dmap table (`minix3/minix/include/minix/dmap.h:25`, the
/// ttys family), with the conventional minor number 0. `bin_entries` are the
/// entries to seed under /bin (on-disk name, host-relative path), always 0755 —
/// this table only seeds executables; it holds `sh` and `echo` (the rc
/// external-command leg) and is kept as a list so later command-surface work can
/// grow it as needed; non-executable files would need a mode column first. A
/// non-empty `atf_tests` additionally seeds a `/tests` directory (also the 0755
/// executable surface; the cross ELFs are produced by
/// `tools/build-atf-test.sh`, outside the cargo assembly chain, hence only
/// passed when the host file exists). With an empty table the output is
/// byte-identical to before and does not affect the existing
/// three-architecture install surface. When `rc_host` is Some, the rc entry
/// points at that host path (the assembly side already injected the suite exec
/// lines into the variant file); None still points at `etc/rc`. `blocks` is the
/// root-disk block count (the caller picks IMGRD_BLOCKS / IMGRD_BLOCKS_SUITE by
/// suite presence; the proto header and the mkfs argument share this one
/// place).
pub fn generate_etc_proto(
    etc_dir: &Path,
    bin_entries: &[(&str, &str)],
    atf_tests: &[(&str, &str)],
    rc_host: Option<&str>,
    blocks: u32,
) -> Result<Vec<u8>> {
    // Existence is verified here; the content is read from the host by
    // mkfs_mfs's StdHost at seed time.
    for required in ["rc", "ttys"] {
        let path = etc_dir.join(required);
        if !path.is_file() {
            bail!(
                "/etc 最小集缺件：{} 不存在（装机面不造缺 rc/ttys 的镜像）",
                path.display()
            );
        }
    }

    let mut proto = String::from(
        "minix-rs imgrd\n",
    );
    proto.push_str(&format!(
        "{} 0\n\
         d--755 0 0\n\
         etc d--755 0 0\n\
         ",
        blocks
    ));
    proto.push_str(&format!("rc ---755 0 0 {}\n", rc_host.unwrap_or("etc/rc")));
    proto.push_str(
        "ttys ---644 0 0 etc/ttys\n\
         $\n\
         dev d--755 0 0\n\
         console c--600 0 0 4 0\n\
         $\n\
         bin d--755 0 0\n",
    );
    for (name, host_rel) in bin_entries {
        proto.push_str(&format!("{name} ---755 0 0 {host_rel}\n"));
    }
    proto.push_str("$\n");
    if !atf_tests.is_empty() {
        proto.push_str("tests d--755 0 0\n");
        for (name, host_rel) in atf_tests {
            proto.push_str(&format!("{name} ---755 0 0 {host_rel}\n"));
        }
        proto.push_str("$\n");
    }
    proto.push_str("$\n");
    Ok(proto.into_bytes())
}

/// Execute the assembly plan. `dry_run` only prints, writing nothing.
pub fn execute(actions: &[Action], dry_run: bool) -> Result<()> {
    for action in actions {
        match action {
            Action::Cargo { args, note } => {
                println!("  [cargo] {note}");
                if dry_run {
                    println!("    > cargo {}", args.join(" "));
                    continue;
                }
                let mut cmd = Command::new("cargo");
                // The aarch64 production kernel disables NEON and scalar FP
                // (`-C target-feature=-neon,-fp-armv8`, only on this one cargo
                // invocation for the kernel; user-module builds carry no such
                // flag). C's lazy-FPU model (i386 CR0.TS / ARM64 FPEN lazy
                // trap) assumes the kernel itself does not use FPU/NEON — this
                // kernel source has no floating point, but LLVM auto-vectorizes
                // block copies into NEON (kernel.elf measured ~840 FP
                // instructions before the change), which trample the user
                // process's Q0-Q31 while the kernel runs, with nobody saving or
                // restoring them (the trap stub only saves GPRs) — on aarch64
                // this showed up directly as PM compiler constants placed in d8
                // being clobbered and the VFS_PM_INIT message header being
                // dirtied (WrongMessageType(8)). With NEON+FP disabled the
                // kernel source has no implicit V-register use left: the same
                // physical registers are preserved across traps on one CPU, and
                // cross-CPU migration is covered by the smp.rs SAVE_CTX IPI save
                // + EC=0x07 lazy restore loop. Known residual: a few V-register
                // instructions in rustup's precompiled core (char counting /
                // formatted diagnostics paths) are not rebuilt under RUSTFLAGS;
                // recorded as a build-std todo. The explicit FPSIMD primitives
                // in fpu.rs are kept via per-function target_feature. riscv64
                // gets the same family of injection (`-f,-d` = disable FP — the
                // kernel had 345 LLVM self-vectorized FP instructions trampling
                // live user f registers before the change; FS=Off gating has no
                // per-mode split, so the kernel must truly be FP-free; the
                // precompiled core residual is recorded in the fpu notes).
                let is_kernel_image = args.windows(2).any(|w| w == ["-p", "kernel-image"]);
                let is_aarch64 = args.iter().any(|a| a.contains("aarch64"));
                let is_riscv64 = args.iter().any(|a| a.contains("riscv64"));
                if is_kernel_image && (is_aarch64 || is_riscv64)
                {
                    let flag = if is_aarch64 {
                        "-C target-feature=-neon,-fp-armv8"
                    } else {
                        "-C target-feature=-f,-d"
                    };
                    let rustflags = "RUSTFLAGS";
                    let prior = std::env::var(rustflags).unwrap_or_default();
                    let injected = if prior.is_empty() {
                        flag.to_string()
                    } else {
                        format!("{prior} {flag}")
                    };
                    cmd.env(rustflags, &injected);
                    let arch_note = if is_aarch64 {
                        "aarch64 内核禁 NEON/FP"
                    } else {
                        "riscv64 内核禁 F/D"
                    };
                    println!("    [env] RUSTFLAGS={injected}（{arch_note}，SD-23）");
                }
                cmd.args(args);
                run_inherit(&mut cmd)
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
            Action::ToolBestEffort { program, args, note } => {
                println!("  [tool-best-effort] {note}: {program} {}", args.join(" "));
                if dry_run {
                    continue;
                }
                let mut cmd = Command::new(program);
                cmd.args(args);
                let status = cmd.status();
                match status {
                    Ok(st) if st.success() => {}
                    Ok(st) => println!(
                        "    [tool-best-effort] skipped ({} exited {})",
                        program, st
                    ),
                    Err(e) => println!("    [tool-best-effort] skipped ({e})"),
                }
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

/// `xtask image` entry point.
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
        let loader = arch.uefi_slots().map_or(String::from("(非 UEFI)"), |u| {
            format!("/EFI/BOOT/{}", u.loader)
        });
        println!(
            "   布局：{} + /EFI/minix/{{kernel.elf, imgrd, modules/×12}}",
            loader
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal temp-directory helper (no dev dependencies; plain std
    /// composition suffices).
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

    /// Plant the /bin host artifacts in the tempdir layout (plan's missing-file
    /// fail-fast needs them present; their content is irrelevant — the proto
    /// only references paths). The list also includes `echo` (the rc
    /// external-command seed leg).
    fn plant_sh(layout: &Layout, module_target: &str, profile: &str) {
        let dir = layout
            .target_root
            .join(module_target)
            .join(profile);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sh"), b"").unwrap();
        std::fs::write(dir.join("echo"), b"").unwrap();
    }

    /// Plan completeness: every build/install/seed/ESP-assembly stage is
    /// present and the production switch is in place.
    #[test]
    fn plan_x86_64_contains_all_contract_steps() {
        let (layout, _guard) = tempdir::create();
        plant_sh(&layout, "x86_64-unknown-none", "release");
        let (actions, esp) = plan(Arch::X86_64, true, None, &layout).unwrap();

        let text = format!("{actions:?}");
        // 12 modules built one package at a time, installed one by one (three
        // representatives are asserted here; the full count follows).
        for pkg in ["minix-ds", "minix-init", "minix-fs-mfs", "minix-driver-tty"] {
            assert!(text.contains(pkg), "装机计划应含 {pkg}");
        }
        assert_eq!(
            (actions
                .iter()
                .filter(|a| matches!(a, Action::Cargo { .. }))
                .count()),
            17,
            "11 模块(不含 mfs) + sh/echo（minix-shell+minix-fileops 一步） + e5-ds（E5(f) 载体一步） + boot-shim + kernel-image + mkfs_mfs + mfs(B11 后构) = 17 步 cargo 构建"
        );
        // Production switch and uefi target.
        assert!(text.contains("fw-x86-uefi") && text.contains("x86_64-unknown-uefi"));
        assert!(
            text.contains("--no-default-features"),
            "boot-shim 构建必须关默认 feature"
        );
        // Kernel-image build step and the guest target switch.
        assert!(
            text.contains("kernel-image") && text.contains("fw-x86-none"),
            "装配计划应含 kernel-image 构建步（fw-x86-none 门）"
        );
        // mkfs_mfs seeding and imgrd packaging.
        assert!(text.contains("mkfs_mfs") && text.contains("imgrd"));
        // Kernel ELF contract slot (ESP install name) and output location.
        assert!(text.contains("kernel.elf"));
        assert!(esp.ends_with("minix.img"));
        // mtools assembly stage present.
        assert!(text.contains("mmd") && text.contains("mcopy") && text.contains("mkfs.vfat"));
        // boot-shim enters the ESP as the firmware default load option
        // (omitting it in the first assembly landed in the UEFI Shell, as the
        // first test-cmd-smoke run showed) + the auto-boot script.
        assert!(text.contains("BOOTX64.EFI") && text.contains("startup.nsh"));
    }

    /// Non-UEFI boot shape (riscv64) bails honestly: no unbootable image is
    /// installed.
    #[test]
    fn plan_bails_honestly_for_arch_without_uefi_path() {
        let (layout, _guard) = tempdir::create();
        assert!(plan(Arch::Riscv64, true, None, &layout).is_err());
    }

    /// The aarch64 leg uses its own four constants: it does not casually reuse
    /// x86's triple and loader name — the whole table switches.
    #[test]
    fn plan_aarch64_switches_every_uefi_slot() {
        let (layout, _guard) = tempdir::create();
        plant_sh(&layout, "aarch64-unknown-none", "release");
        let (actions, esp) = plan(Arch::Aarch64, true, None, &layout).unwrap();
        let text = format!("{actions:?}");

        // Structural assertion: locate the argument value of that specific
        // step instead of relying on whole-string containment — otherwise the
        // `aarch64-unknown-none` of the 12 module steps would stand in for the
        // kernel-image step, and writing the target back to x86 would go
        // undetected.
        let step_flag = |pkg: &str, flag: &str| -> String {
            actions
                .iter()
                .find_map(|a| match a {
                    Action::Cargo { args, .. } => {
                        if !args.windows(2).any(|w| w[0] == "-p" && w[1] == pkg) {
                            return None;
                        }
                        args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{pkg} 构建步不在计划里"))
        };
        assert_eq!(step_flag("boot-shim", "--target"), "aarch64-unknown-uefi");
        assert_eq!(step_flag("boot-shim", "--features"), "fw-aarch64-uefi");
        assert_eq!(
            step_flag("kernel-image", "--target"),
            "aarch64-unknown-none"
        );
        assert_eq!(
            step_flag("kernel-image", "--features"),
            "fw-aarch64-none",
            "kernel-image 腿必须用 fw-aarch64-none（M3.2 定的词汇表）"
        );
        // Reverse assertion: none of x86's four hardcoded values may appear
        // (otherwise the table did not take effect).
        for x86_only in [
            "x86_64-unknown-uefi",
            "fw-x86-uefi",
            "fw-x86-none",
            "BOOTX64.EFI",
        ] {
            assert!(
                !text.contains(x86_only),
                "aarch64 计划里不应残留 x86 写死值 {x86_only}"
            );
        }
        // The load option and the auto-boot script carry the same name (a
        // wrong name means AAVMF never loads anything).
        assert!(
            text.contains("BOOTAA64.EFI") && text.contains("startup.nsh"),
            "ESP 默认加载项应为 BOOTAA64.EFI 且同步写进 startup.nsh"
        );
        // Everything else matches x86_64 (11 modules + sh + shim + kernel +
        // mkfs + mfs = 16).
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Cargo { .. }))
                .count(),
            16,
            "aarch64 与 x86_64 的装配步骤数必须相同"
        );
        assert!(esp.ends_with("minix.img"));
    }

    /// Byte-exact auto-boot script contract: the x86_64 leg's `startup.nsh`
    /// must stay byte-identical to before aarch64 was enabled (an external
    /// artifact of the green chain that must not be changed incidentally by
    /// tabling it), and the aarch64 leg may differ only in the load-option
    /// filename segment.
    #[test]
    fn startup_nsh_bytes_are_the_frozen_template_per_loader_name() {
        /// The x86_64 constant text from before the change (the `bytes` of
        /// `Action::Write` in `git show 873f3e947:os/xtask/src/image.rs`) — even
        /// one extra trailing backslash is behaviorally equivalent but changes
        /// the contract, so this compares bytes rather than "does it boot".
        const X86_FROZEN: &str = "echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\nBOOTX64.EFI\r\n";
        let nsh = |arch: Arch, module_target: &str| -> Vec<u8> {
            let (layout, _guard) = tempdir::create();
            plant_sh(&layout, module_target, "release");
            let (actions, _) = plan(arch, true, None, &layout).unwrap();
            actions
                .iter()
                .find_map(|a| match a {
                    Action::Write { path, bytes, .. } if path.ends_with("startup.nsh") => {
                        Some(bytes.clone())
                    }
                    _ => None,
                })
                .expect("startup.nsh 写入步不在计划里")
        };
        assert_eq!(nsh(Arch::X86_64, "x86_64-unknown-none"), X86_FROZEN.as_bytes());
        assert_eq!(
            nsh(Arch::Aarch64, "aarch64-unknown-none"),
            X86_FROZEN.replace("BOOTX64.EFI", "BOOTAA64.EFI").as_bytes()
        );
    }

    /// Prototype grammar: two header lines + recursive directory closing +
    /// console device line (major 4).
    #[test]
    fn etc_proto_grammar_and_console_device_line() {
        let (layout, _guard) = tempdir::create();
        let proto = String::from_utf8(
            generate_etc_proto(
                &layout.os_root.join("etc"),
                &[("sh", "target/sh"), ("echo", "target/echo")],
                &[],
                None,
                super::IMGRD_BLOCKS,
            )
            .unwrap(),
        )
        .unwrap();
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
            4,
            "etc/dev/bin/root 四层收口（1.10r：+bin 装载 /bin/sh）"
        );
        assert!(
            body.contains(&"console c--600 0 0 4 0"),
            "console = char special major 4（dmap.h:25 TTY_MAJOR）"
        );
        assert!(
            body.contains(&"rc ---755 0 0 etc/rc"),
            "rc 经宿主路径 etc/rc 播种"
        );
        assert!(
            body.contains(&"bin d--755 0 0") && body.contains(&"sh ---755 0 0 target/sh"),
            "/bin/sh 播种（rc marker 链：init runcom exec /bin/sh）"
        );
        assert!(
            body.contains(&"echo ---755 0 0 target/echo"),
            "/bin/echo 播种（NK4-C 1.56 B31：rc 唯一外部命令，缺它 sh 退 127）"
        );
        assert!(
            !body.iter().any(|l| l.starts_with("tests ")),
            "空 atf_tests 时不产 /tests 段（逐字节同旧，三架构装机面零影响）"
        );
    }

    /// A non-empty atf_tests adds a tests directory segment under the root (the
    /// same 0755 executable surface).
    #[test]
    fn etc_proto_seeds_atf_tests_dir() {
        let (layout, _guard) = tempdir::create();
        let proto = String::from_utf8(
            generate_etc_proto(
                &layout.os_root.join("etc"),
                &[("sh", "target/sh")],
                &[("t_memchr", "target/atf/aarch64/tests/t_memchr")],
                None,
                super::IMGRD_BLOCKS_SUITE,
            )
            .unwrap(),
        )
        .unwrap();
        let lines: Vec<&str> = proto.lines().collect();
        assert!(
            lines.contains(&"tests d--755 0 0"),
            "/tests 目录段头存在"
        );
        assert!(
            lines.contains(&"t_memchr ---755 0 0 target/atf/aarch64/tests/t_memchr"),
            "测试 ELF 经宿主相对路径播种（同 bin_entries 文法）"
        );
        // The tests segment brings its own $ closing → the total closing count
        // goes from 4 to 5.
        assert_eq!(lines.iter().filter(|l| **l == "$").count(), 5);
    }

    /// Prototype generation must fail when the minimal /etc set is missing (the
    /// install surface never ships an image without rc).
    #[test]
    fn etc_proto_requires_rc_and_ttys() {
        let (layout, guard) = tempdir::create();
        std::fs::remove_file(guard.0.join("etc/rc")).unwrap();
        assert!(
            generate_etc_proto(
                &layout.os_root.join("etc"),
                &[("sh", "target/sh")],
                &[],
                None,
                super::IMGRD_BLOCKS,
            )
            .is_err()
        );
    }

    /// The case-name extractor collects `ATF_TC(name);` declaration lines
    /// without duplicates and does not count HEAD/BODY definition lines; real
    /// suite sources yield at least one name (guarding against declaration
    /// drift).
    #[test]
    fn extract_atf_tc_names_collects_declarations_only() {
        let sample = "ATF_TC(memchr_basic);\nATF_TC_HEAD(memchr_basic, tc)\n{\n}\nATF_TC_BODY(memchr_basic, tc)\n{\n}\nATF_TC(memchr_simple);\n";
        assert_eq!(
            extract_atf_tc_names(sample),
            vec!["memchr_basic".to_string(), "memchr_simple".to_string()]
        );
        assert!(extract_atf_tc_names("/* 声明面漂移的样本 */\nint main(){}\n").is_empty());
    }

    /// exec lines must land before `exit 0` (the trailing fixed return-code
    /// semantics stay intact); an empty exec list is byte-identical to before;
    /// without an exit 0 line they are appended at the end.
    #[test]
    fn inject_rc_execs_lands_before_exit_zero() {
        let rc = "echo m\n/tests/p1\nexit 0\n";
        let out = inject_rc_execs(rc, &["/tests/t_x tc1".to_string(), "/tests/t_x tc2".to_string()]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines,
            vec!["echo m", "/tests/p1", "/tests/t_x tc1", "/tests/t_x tc2", "exit 0"]
        );
        assert_eq!(inject_rc_execs(rc, &[]), rc, "空表逐字节同旧");
        let no_exit = "echo m\n";
        assert_eq!(
            inject_rc_execs(no_exit, &["/tests/t_x tc1".to_string()]).lines().count(),
            2,
            "无 exit 0 时尾部追加"
        );
    }

    /// With rc_host Some, the proto's rc entry points at the variant host
    /// path.
    #[test]
    fn etc_proto_rc_host_override() {
        let (layout, _guard) = tempdir::create();
        let proto = String::from_utf8(
            generate_etc_proto(
                &layout.os_root.join("etc"),
                &[("sh", "target/sh")],
                &[],
                Some("target/image/aarch64/rc.imgrd"),
                super::IMGRD_BLOCKS,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            proto.contains("rc ---755 0 0 target/image/aarch64/rc.imgrd"),
            "rc 条目指向套件注入后的变体"
        );
        assert!(!proto.contains("rc ---755 0 0 etc/rc"));
    }

    /// Every ATF_SUITE entry's minix3 source really exists and yields case names
    /// (a static assembly-surface contract: the table is reconciled against repo
    /// contents so a rename/move cannot silently shrink the seeded set).
    #[test]
    fn atf_suite_entries_have_sources_with_cases() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        assert_eq!(ATF_SUITE.len(), 18, "首批 18 案清单对账");
        for (prog, src_rel) in ATF_SUITE {
            let src = repo.join(src_rel);
            let text = std::fs::read_to_string(&src)
                .unwrap_or_else(|e| panic!("{prog}: 读 {} 失败：{e}", src.display()));
            assert!(!extract_atf_tc_names(&text).is_empty(), "{prog}: 无 ATF_TC 声明");
        }
    }
}
