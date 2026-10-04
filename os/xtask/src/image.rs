//! `xtask image` — 把构建产物装配成可启动机镜像（E-IMGPKG / new_edge3
//! NS8）。
//!
//! 包形（UEFI 路径）：一张 FAT 盘镜像，内含
//!   - `/EFI/BOOT/<固件默认加载项>` ← boot-shim（x86_64 =
//!     `BOOTX64.EFI`，aarch64 = `BOOTAA64.EFI`，见 [`Arch::uefi_slots`]）
//!   - `/EFI/minix/kernel.elf` ← 内核 ELF（boot-shim 按
//!     `loader::KERNEL_PATH` 读取，`os/boot-shim/src/loader.rs:31`）
//!   - `/EFI/minix/modules/<名>` ×12 ← 装机清单（[`crate::manifest`]）
//!   - `/EFI/minix/imgrd` ← mfs 根盘（mkfs_mfs 播种 `/etc` 最小集）。
//!     B11（E-IMGPKG）：mfs 的 build.rs 在编译期 include_bytes!
//!     把 imgrd 字节喂进 BOOT_IMGRD 静态量，消费通道已落地。
//!
//! 与 boot-shim 的 fail-fast 同语义：模块缺件在装配期报错点名
//! （`os/boot-shim/src/loader.rs:280-288` 的运行期对位），不让半包
//! 镜像流出去。
//!
//! 生产开关：boot-shim 以 `--no-default-features --features
//! fw-x86-uefi`（aarch64 为 `fw-aarch64-uefi`）构建（X-2/NK5 的 feature
//! 词汇表），链接进 boot-shim 的
//! 内核半因此不带 mock（`boot-shim/Cargo.toml` 对 minix-kernel 已写死
//! `default-features = false`）。模块逐包独立调用 cargo——规避 feature
//! 统一陷阱（一次多 `-p` 会把 Mock 与生产类型拉进同一编译），也让
//! 失败点名到具体模块。

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// 目标架构。x86_64 与 aarch64 有 UEFI 装机面（后者由 NK4-B P3 M3.3
/// 放行）；riscv64 走 U-Boot fatload，不经 UEFI 盘形，装配时 honest bail
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

    /// UEFI 装机面的四个架构常量。`None` = 本架构不走 UEFI 盘形。
    ///
    /// 这些值今天散在装配计划里写死成 x86 形态，放行 aarch64 就是把它们
    /// 收成一张表（NK4-B P3 M3.3 决策四）。`loader` 名不是随意取的：
    /// UEFI 规范按 CPU 架构规定默认加载项文件名，AAVMF 只认
    /// `EFI/BOOT/BOOTAA64.EFI`。
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

/// 一个架构的 UEFI 装机面常量（见 [`Arch::uefi_slots`]）。
#[derive(Clone, Copy, Debug)]
struct UefiSlots {
    /// boot-shim 的构建目标三元组。
    shim_target: &'static str,
    /// boot-shim 的对外特性名（生产开关）。
    shim_feature: &'static str,
    /// `kernel-image` 的对外特性名（目标三元组同 [`Arch::module_target`]，
    /// 与 [`kernel_elf_path`] 的取件路径已经是同一假设）。
    kernel_none_feature: &'static str,
    /// ESP 内 `/EFI/BOOT/` 下的固件默认加载项文件名。
    loader: &'static str,
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

/// 内核 ELF 的取用路径。产出者 = `kernel-image` 包（NS8-A，new_edge1
/// 面）：bin 名 `kernel`，cargo 对 none 目标产无扩展名工件
/// `target/<target>/<profile>/kernel`——缺省路径与之对齐；ESP 侧安装名
/// 仍是 `/EFI/minix/kernel.elf`（`loader::KERNEL_PATH` 契约位不变）。
/// `--kernel` 可指向任意外部产出的 ELF（自定义内核构建流程用）。
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

/// 生成装配计划，返回（动作序列，ESP 盘镜像产出位）。
pub fn plan(
    arch: Arch,
    release: bool,
    kernel_override: Option<&Path>,
    layout: &Layout,
) -> Result<(Vec<Action>, PathBuf)> {
    // 三架构诚实话：riscv64 的启动路径是 U-Boot fatload + BootFileTable，
    // 不经 UEFI 装机形（载体机制见 qemu-tests/test-riscv64-uboot.sh），
    // 给它装一张 ESP 盘就是装不可启动机的镜像。x86_64 / aarch64 的四个
    // 架构常量由 [`Arch::uefi_slots`] 一张表供值（aarch64 腿 = NK4-B P3
    // M3.3 决策四放行，依赖 boot-shim 的 fw-aarch64-uefi 门）。
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

    // 1a. 逐包构建 11 模块 + sh（guest 用户态目标；失败点名到包）。
    //     mfs 推迟到 imgrd 生成后构建（B11：build.rs include_bytes!）。
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
    // /bin/sh、/bin/echo 不是 boot 模块，但 imgrd 播种需要其 ELF 二进制
    // （proto 路径引用）。NK4-C 1.56 B31：本重写的 shell 目前只有
    // exit/cd 两个内建（commands/bin/shell/src/bin/sh.rs 的 eval 臂），
    // rc 里的 `echo marker` 因此走外部命令腿；C 的 ash 虽有 echo 内建
    // （`minix3/bin/sh/builtins.def` 的 `echocmd`，无 TINY/SMALL 门），
    // 但其镜像同样独立装 /bin/echo（`minix3/bin/echo/`），播种即对位
    // ——内建化留待 shell 扩内建面时裁决。此前盘上只播 sh：一旦
    // init→sh 的 exec 腿（B32）修通，rc 的 echo 会 PATH 全 ENOENT →
    // sh 退 127 → runcom 回落 SingleUser。当前循环的近因是 B32，
    // 本缺件是它下游的独立实锤障碍。
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

    // 2. boot-shim（生产开关：关默认 feature，只开本架构的 uefi 特性）。
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

    // 2b. 生产内核镜像（NS8-A 产出者：`-p kernel-image`，bin 门
    //     fw-<arch>-none，产出 `target/<none target>/<profile>/kernel`）。
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

    // 3. mkfs_mfs 宿主工具（imgrd 生成器；宿主三元组，S32 批次三十一交付）。
    cargo_release(
        &mut actions,
        "mkfs_mfs 宿主工具构建",
        &["-p", "minix-diskfmt"],
    );

    // 3b. /etc 原型 + mkfs_mfs 播种 imgrd（先于 mfs 构建——build.rs 需 imgrd 就位）。
    let proto_path = image_dir.join("imgrd.proto");
    let imgrd = image_dir.join("imgrd.img");
    let bin_rel = |name: &str| format!("target/{}/{}/{name}", arch.module_target(), profile);
    // NK4-C 续-277：目标③ ATF C 测试播种——交叉 ELF 不在 cargo 装配链内（由
    // tools/build-atf-test.sh 产出），文件存在才播；首案 t_memchr（纯内存面，
    // 不依赖 fork/exec/VFS 桥）+ p1/p2/p3 上机二分探针（tools/atf-c-compat/probes/，
    // 分别验裸桥/stdio/malloc 腿）。盘上落 /tests/<name>，rc 尾段 exec。
    // 单源：用例表/rc 注入规则 = [`collect_atf_face`]（`xtask atf-face` 子命令
    // 同源消费——riscv64 不走 UEFI 盘形装配，用该子命令拿同一套 rc 变体+manifest）。
    let (atf_tests, rc_execs) = collect_atf_face(arch, layout)?;
    // 门 manifest（"prog tc" 逐行）= rc exec 行去掉 `/tests/` 前缀（同一序列，
    // 不做第二份拼接，防两处漂移）。
    let atf_plan: Vec<String> = rc_execs
        .iter()
        .map(|line| line.trim_start_matches("/tests/").to_string())
        .collect();
    // rc 变体：套件 exec 行注进 `exit 0` 之前（os/etc/rc 保持静态哨兵面，
    // 套件面单源在 ATF_SUITE+minix3 C 源）。
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
    // 根盘容量随套件存在性放大（无套件架构逐字节同旧，CodeReview 续-278）。
    let imgrd_blocks = if atf_tests.is_empty() { IMGRD_BLOCKS } else { IMGRD_BLOCKS_SUITE };
    actions.push(Action::Write {
        path: proto_path.clone(),
        bytes: generate_etc_proto(
            &layout.os_root.join("etc"),
            &[
                ("sh", &bin_rel("sh")),
                ("echo", &bin_rel("echo")),
                ("ls", &bin_rel("ls")),
                ("cat", &bin_rel("cat")),
            ],
            &atf_refs,
            rc_host,
            imgrd_blocks,
        )?,
        note: "imgrd 原型文件（/etc 最小集 + /bin/{sh,echo,ls,cat} + /tests + /dev/console）",
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
            imgrd_blocks.to_string(), // 块数：×4KiB，随套件存在性选值（见常量注）
            "0".into(),    // inode 数 0 = mkfs 缺省阶梯（mkfs.rs proto_header 文档）
            "4096".into(), // 块大小
            "-p".into(),
            proto_path.to_string_lossy().into_owned(),
        ],
        cwd: Some(layout.os_root.clone()),
        note: "mkfs_mfs 播种 imgrd（B11：先于 mfs 构建）",
    });

    // 3c. mfs 构建（build.rs 此时可发现 target/image/<arch>/imgrd.img，
    //     经 include_bytes! 喂 BOOT_IMGRD）。E-IMGPKG 落地。
    cargo_release(
        &mut actions,
        "mfs 构建（imgrd include_bytes!，B11）",
        &["-p", "minix-fs-mfs", "--target", arch.module_target()],
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

    // 7. imgrd 入 staging（生成已在 step 3b 完成，此处只拷贝）。
    actions.push(Action::Copy {
        from: imgrd,
        to: staging.join("EFI/minix/imgrd"),
        note: "imgrd 入包 staging（供 ESP mcopy 消费）",
    });

    // 8. FAT 盘镜像组装（mtools；与 run_qemu.sh 的真盘镜像优先路径同词汇，
    //    run_qemu.sh:77-86）。
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

    // 9. boot-shim 入包为固件默认加载项 + 全部文件 mcopy 进 ESP。
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
    // boot-shim 本体也是 ESP 文件：staging 只进了工作树，固件加载的是
    // 盘上的 ::/EFI/BOOT/<本架构默认加载项>（首装配漏列即 UEFI Shell 落地，
    // test-cmd-smoke 首跑实证）。startup.nsh 是本固件的自动引导 belt-
    // and-suspenders（test-sysboot 判例：倒计时后逐行执行）。
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

/// 目标③首批 ATF 套件（NK4-C 续-278）：盘上程序名 → minix3 C 源（仓根相对）。
/// ELF 由 tools/build-atf-test.sh 交编（同名落 target/atf/<arch>/tests/）；
/// 用例名装配时从源文件 `ATF_TC(name);` 声明行提取（不另立清单，单一真源=C 源）。
/// p1/p2/p3 桥面哨兵不在此表（无 atf 用例，固定 exec 行在 os/etc/rc 里）。
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

/// 从 atf C 源提取注册用例名（`ATF_TC(name);` 声明行；HEAD/BODY 定义行不重复
/// 计名）。纯函数，宿主单测钉住；提不到任何名字时调用方须视装配失败。
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

/// 在 rc 文本的 `exit 0` 行（尾行，若存在）之前注入测试 exec 行；无 `exit 0`
/// 则追加到末尾。纯函数便于单测钉住插入位置语义。
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

/// imgrd 根盘块数（× 4KiB）：无套件时保持旧值 2048（=8MiB，x86/riscv 装配
/// 产物逐字节同旧，CodeReview 续-278 建议：不给未验证架构扩足迹）；首批 ATF
/// 套件 18 个静态 ELF（每个 ~410KB 含 libc+libatf-c）超旧预算时由调用方按
/// 套件存在性传 4096（=16MiB；ESP 128MiB 容得下，bootface 源直拷 bump 分配
/// 无固定上限）。proto 头部与 mkfs 参数共用同一值（单点，不再两处手写）。
pub const IMGRD_BLOCKS: u32 = 2048;
pub const IMGRD_BLOCKS_SUITE: u32 = 4096;

/// 自定义出生腿（`-nostartfiles` + startup-minix.c）已放行的架构白名单。
/// riscv64 于 §续-338（exec 栈顶 Sv39 规范地址修复）后与 aarch64 同形启用——
/// 此前 riscv 腿链 picolibc 默认 crt0（sp 切 ELF 内 .stack + main(0,NULL)），
/// §续-277 已定谳为坏。
const ATF_BOOT_LEG_READY: &[&str] = &["aarch64", "riscv64"];

/// 收集一台架构的 ATF 上机面（单源）：返回
/// `(tests[(prog, host-rel-path)], rc_exec_lines)`。
///
/// 用例表来自 [`ATF_SUITE`]（18 案清单）并对每个 prog 从 minix3 C 源提取
/// `ATF_TC(name)` 声明——提取为空即拒绝静默装配。`p1/p2/p3` 桥面哨兵无条件
/// 追加（存在才播）。`plan()`（UEFI 盘形装配）与 [`atf_face()`]（非 UEFI 启动
/// 形的产物面）共用本函数，保证 rc 注入序列与 manifest 永远同源。
fn collect_atf_face(arch: Arch, layout: &Layout) -> Result<(Vec<(String, String)>, Vec<String>)> {
    let mut tests: Vec<(String, String)> = Vec::new();
    let mut rc_execs: Vec<String> = Vec::new();
    if !ATF_BOOT_LEG_READY.contains(&arch.slug()) {
        return Ok((tests, rc_execs));
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
    Ok((tests, rc_execs))
}

/// 生成 ATF 上机面产物（不依赖 UEFI 盘形装配）。
///
/// riscv64 的启动形是 OpenSBI 直载 + BootFileTable（见
/// `os/qemu-tests/test-riscv64-boot-full.sh`），不经 `xtask image` 的 ESP 盘形；
/// 但 ATF 套件面需要的两件单源产物必须与 aarch64 完全同源：
///   - `target/image/<arch>/rc.imgrd`：`os/etc/rc` + `/tests/<prog> <tc>` exec 行
///     （注在 `exit 0` 前，规则=[`inject_rc_execs`]）；
///   - `target/image/<arch>/atf-plan.txt`：`prog tc` 逐行 manifest（门的期望
///     数唯一真源）。
///
/// stdout 每行打印一个待播种 prog 名——调用方据此把 `/tests/<prog>` 播进
/// imgrd（riscv 装配脚本的 proto 生成用）。
pub fn atf_face(arch: Arch, dry_run: bool) -> Result<()> {
    if !ATF_BOOT_LEG_READY.contains(&arch.slug()) {
        bail!(
            "{} 未在 ATF 出生腿白名单内（ATF_BOOT_LEG_READY）——先放行该架构的 \
             -nostartfiles 出生腿",
            arch.slug()
        );
    }
    let layout = Layout::from_env();
    let (tests, rc_execs) = collect_atf_face(arch, &layout)?;
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

/// 生成 /etc 最小集的 mkfs 原型文本。
///
/// 文法（`os/fs/mfs/src/mkfs.rs` 的 proto_header/build_image_seeded）：
/// 第一行引导名读到即弃；第二行 `块数 inode数`；第三行是**根目录模式
/// 行**（仅 mode uid gid，无名字）；其后是根目录条目，目录递归以 `$`
/// 收口；设备行 `名 c--权限 uid gid major minor`。console 设备号对位
/// C dmap 表的 TTY_MAJOR=4（`minix3/minix/include/minix/dmap.h:25`，
/// ttys 族），惯例次设备号 0。`bin_entries` 是 /bin 下要播种的条目
/// （盘上名, 宿主相对路径），一律 0755——本表只播可执行文件；
/// NK4-C 1.56 B31 起含 `sh` 与 `echo`（rc 外部命令腿），列表化以便
/// 后续命令面（18-stage）按需上收；非 exec 文件需先带 mode 入表。
/// `atf_tests`（NK4-C 续-277，目标③）非空时额外播 `/tests` 目录（同 0755
/// 可执行面；交叉 ELF 由 `tools/build-atf-test.sh` 产出，不在 cargo 装配链
/// 内，故只在宿主文件存在时传入）。空表时产物逐字节同旧，不影响既有
/// 三架构装机面。`rc_host`（续-278）Some 时 rc 条目改指该宿主路径（装配方
/// 已把套件 exec 行注入的变体文件）；None 仍指 `etc/rc`。`blocks` 是根盘块数
///（调用方按套件存在性选 IMGRD_BLOCKS / IMGRD_BLOCKS_SUITE，proto 头与 mkfs
/// 参数同源单点）。
pub fn generate_etc_proto(
    etc_dir: &Path,
    bin_entries: &[(&str, &str)],
    atf_tests: &[(&str, &str)],
    rc_host: Option<&str>,
    blocks: u32,
) -> Result<Vec<u8>> {
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
                let mut cmd = Command::new("cargo");
                // aarch64 生产内核禁 NEON 与标量 FP（`-C target-feature=
                // -neon,-fp-armv8`，仅内核链这一次 cargo 调用；用户模块构
                // 建不带此旗）。C 内核的 lazy-FPU 模型（i386 CR0.TS / ARM64
                // FPEN 懒陷阱）依赖「内核自身不用 FPU/NEON」前提——本内核
                // 源码无浮点，但 LLVM 会把块拷贝自动向量化成 NEON（改前
                // kernel.elf 实测 ~840 条 FP 指令），内核执行期间踩进用户
                // 进程的 Q0-Q31 且无人保存恢复（trap stub 只存 GPR，
                // SD-23）——aarch64 上直接表现为 PM 编译器放进 d8 的常量
                // 被踩、VFS_PM_INIT 消息头被写脏（WrongMessageType(8)）。
                // 禁 NEON+FP 后内核源码再无隐式 V 寄存器使用：同 CPU 物理
                // 寄存器跨陷入自动保持；跨 CPU 迁移由 smp.rs SAVE_CTX IPI
                // save + EC=0x07 懒 restore 闭环兜底。已知残差：rustup 预
                // 编译 core 里的少量 V 寄存器指令（字符计数/格式化诊断路
                // 径）不受 RUSTFLAGS 重建，登记为 build-std 待办。fpu.rs
                // 的显式 FPSIMD 原语经 per-function target_feature 保留。
                // NK4-C 续-133：riscv64 同族注入（`-f,-d`＝禁 FP——内核
                // 改前 345 条 LLVM 自向量化 FP 指令踩用户 f 寄存器活值；
                // FS=Off 门控无 per-mode 分裂，内核必须真无 FP，含预编译
                // core 残差见 fpu 注释登记）。
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

    /// NK4-C 1.10r：在 tempdir 布局中植出 /bin 宿主产物（plan 的
    /// 缺件 fail-fast 需要它们存在；内容无关——proto 只引用路径）。
    /// NK4-C 1.56 B31：+echo（rc 外部命令播种腿）。
    fn plant_sh(layout: &Layout, module_target: &str, profile: &str) {
        let dir = layout
            .target_root
            .join(module_target)
            .join(profile);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sh"), b"").unwrap();
        std::fs::write(dir.join("echo"), b"").unwrap();
    }

    /// 计划完整性：构建/装机/播种/ESP 组装各环节齐全，生产开关在位。
    #[test]
    fn plan_x86_64_contains_all_contract_steps() {
        let (layout, _guard) = tempdir::create();
        plant_sh(&layout, "x86_64-unknown-none", "release");
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
            16,
            "11 模块(不含 mfs) + sh/echo（minix-shell+minix-fileops 一步） + boot-shim + kernel-image + mkfs_mfs + mfs(B11 后构) = 16 步 cargo 构建"
        );
        // 生产开关与 uefi 目标。
        assert!(text.contains("fw-x86-uefi") && text.contains("x86_64-unknown-uefi"));
        assert!(
            text.contains("--no-default-features"),
            "boot-shim 构建必须关默认 feature"
        );
        // 内核镜像构建步（NS8-A 产出者）与 guest 目标开关。
        assert!(
            text.contains("kernel-image") && text.contains("fw-x86-none"),
            "装配计划应含 kernel-image 构建步（fw-x86-none 门）"
        );
        // mkfs_mfs 播种与 imgrd 入包。
        assert!(text.contains("mkfs_mfs") && text.contains("imgrd"));
        // 内核 ELF 契约位（ESP 安装名）与产出位。
        assert!(text.contains("kernel.elf"));
        assert!(esp.ends_with("minix.img"));
        // mtools 组装段在位。
        assert!(text.contains("mmd") && text.contains("mcopy") && text.contains("mkfs.vfat"));
        // boot-shim 入 ESP 为固件默认加载项（首装配漏列 = UEFI Shell
        // 落地，test-cmd-smoke 首跑实证）+ 自动引导脚本。
        assert!(text.contains("BOOTX64.EFI") && text.contains("startup.nsh"));
    }

    /// 非 UEFI 启动形（riscv64）honest bail：装机面不装不可启动机的镜像。
    #[test]
    fn plan_bails_honestly_for_arch_without_uefi_path() {
        let (layout, _guard) = tempdir::create();
        assert!(plan(Arch::Riscv64, true, None, &layout).is_err());
    }

    /// aarch64 腿走自己的四常量（NK4-B P3 M3.3 决策四）：不是把 x86 的
    /// 三元组与加载项名“顺手共用了”，而是整张表都切换。
    #[test]
    fn plan_aarch64_switches_every_uefi_slot() {
        let (layout, _guard) = tempdir::create();
        plant_sh(&layout, "aarch64-unknown-none", "release");
        let (actions, esp) = plan(Arch::Aarch64, true, None, &layout).unwrap();
        let text = format!("{actions:?}");

        // 结构断言：定位到具体那一步的参值，不靠整串包含——否则 12 个
        // 模块步骤的 `aarch64-unknown-none` 会替 kernel-image 步骤“顶包”，
        // 把目标写回 x86 也测不出来。
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
        // 反向断言：x86 的四个写死值一个也不许出现（否则就是表没生效）。
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
        // 加载项与自动引导脚本同名（名字写错 = AAVMF 根本不加载）。
        assert!(
            text.contains("BOOTAA64.EFI") && text.contains("startup.nsh"),
            "ESP 默认加载项应为 BOOTAA64.EFI 且同步写进 startup.nsh"
        );
        // 其余构形与 x86_64 同调（11 模块 + sh + shim + kernel + mkfs + mfs = 16）。
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

    /// 自动引导脚本逐字节契约：x86_64 腿的 `startup.nsh` 必须与放行 aarch64
    /// 之前逐字节相同（NK4-A 翻绿链路的外部产物，不得被「表化」顺手改掉），
    /// aarch64 腿只能差异在加载项文件名那一段。
    #[test]
    fn startup_nsh_bytes_are_the_frozen_template_per_loader_name() {
        /// 放行前的 x86_64 常量原文（`git show 873f3e947:os/xtask/src/image.rs`
        /// 里 `Action::Write` 的 `bytes`）——多一个尾反斜杠也是行为等价但契约
        /// 已变，所以比的是字节而不是「能不能启动」。
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

    /// 原型文法：两行头 + 目录递归收口 + console 设备行（major 4）。
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

    /// NK4-C 续-277：atf_tests 非空 → 根下多一个 tests 目录段（同 0755 可执行面）。
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
        // tests 段带自己的 $ 收口 → 总收口数从 4 变 5。
        assert_eq!(lines.iter().filter(|l| **l == "$").count(), 5);
    }

    /// /etc 最小集缺件时原型生成必须报错（装机面不造缺 rc 的镜像）。
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

    /// NK4-C 续-278：用例名提取器对 `ATF_TC(name);` 声明行去重收集，
    /// HEAD/BODY 定义行不计名；真实套件源至少提 1 名（防声明面漂移）。
    #[test]
    fn extract_atf_tc_names_collects_declarations_only() {
        let sample = "ATF_TC(memchr_basic);\nATF_TC_HEAD(memchr_basic, tc)\n{\n}\nATF_TC_BODY(memchr_basic, tc)\n{\n}\nATF_TC(memchr_simple);\n";
        assert_eq!(
            extract_atf_tc_names(sample),
            vec!["memchr_basic".to_string(), "memchr_simple".to_string()]
        );
        assert!(extract_atf_tc_names("/* 声明面漂移的样本 */\nint main(){}\n").is_empty());
    }

    /// 续-278：exec 行必须落在 `exit 0` 之前（尾行固定返回码语义不被破坏）；
    /// 空 exec 列表逐字节同旧；无 exit 0 行时追加到尾部。
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

    /// 续-278：rc_host Some 时 proto 的 rc 条目改指变体宿主路径。
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

    /// 续-278：ATF_SUITE 每一项的 minix3 源真实存在且能提到用例名（装配面
    /// 静态契约：表与仓内容对账，防改名/移动后静默少播）。
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
