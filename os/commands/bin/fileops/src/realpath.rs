//! `realpath(3)` 的移植：把一条路径里的 `.`、`..` 与符号链逐段折成绝对路径。
//!
//! 真源是 `minix3/lib/libc/gen/getcwd.c:64-215`（BSD 系实现，`realpath` 与
//! `getcwd` 同处一件）。本层把那条腿要做的事全部收进一个 trait
//! [`PathProbe`]（`lstat`、`readlink`、`getcwd` 三个动作），所以折叠算法本身
//! 可以在宿主上逐分支测；真正的系统调用侧在薄壳里实现这个 trait。
//!
//! 三处值得先说清的语义，都是 C 原文的形状而不是本层的发明：
//! - **相对路径先接 cwd**（`getcwd.c:112-119`）。取 cwd 失败时 C 并不放弃，
//!   而是把 `resolved` 写成 `"."` 再继续——错误只在 `goto out` 那一支才交回。
//! - **符号链目标是绝对路径就整体重开**（`getcwd.c:203-205`：`p = resolved`），
//!   已积累的段全部作废；目标是相对路径则接在当前段之后。
//! - **末段允许不存在**：只有 `lstat` 本身失败才报错，普通文件走完最后一段
//!   就返回（`getcwd.c:174-176` 的失败出口在 `lstat` 上），而「中间段不是目录」
//!   另有 `ENOTDIR` 一支（`getcwd.c:207-209`，判据是这一段后面还有 `/`）。

use alloc::string::String;

/// `minix3/include/limits.h` 的 `PATH_MAX`，本树在 `os/libs/minix-sys/src/lib.rs:269-270`
/// 已经按同一个值定过线上传输上限，这里沿用同一条。
pub const MAXPATHLEN: usize = 1024;

/// `minix3/sys/sys/param.h:358` 的 `MAXSYMLINKS`。
pub const MAXSYMLINKS: u32 = 32;

/// 折叠失败的原因。名字跟着 C 在 `goto out` 前设的 `errno`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealPathError {
    /// 空路径（`getcwd.c:104-107`）。
    NoEntry,
    /// 攒出来的路径超过 `MAXPATHLEN`（`getcwd.c:159-165`）。
    NameTooLong,
    /// 符号链层数超过 `MAXSYMLINKS`（`getcwd.c:180-183`）。
    Loop,
    /// 中间一段不是目录而后还有内容（`getcwd.c:207-209`）。
    NotDir,
    /// `lstat` 失败。C 直接把那个 errno 交回；本层保留错误码，好让调用方
    /// 像 C 那样只在告警里报出来。
    StatFailed,
    /// `readlink` 失败（`getcwd.c:185-191`，返回 0 也按 `ENOENT` 走）。
    ReadLinkFailed,
    /// 相对路径而 cwd 取不到（`getcwd.c:112-118`）。
    NoWorkingDirectory,
}

/// 折叠一条路径要问文件系统的事。
pub trait PathProbe {
    /// `lstat` 的 `st_mode`；失败交回 `None`。
    fn lstat_mode(&self, path: &str) -> Option<u32>;
    /// `readlink` 的内容；失败或空串交回 `None`（C 把 `n == 0` 也当错误）。
    fn read_link(&self, path: &str) -> Option<String>;
    /// 当前工作目录。
    fn working_directory(&self) -> Option<String>;
}

/// `S_ISLNK`/`S_IFDIR`/`S_IFMT` 三个判据要用的模式位，
/// `minix3/sys/sys/stat.h:138-158`（本树 `cp.rs:29-36` 已按同一组值定义过）。
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFDIR: u32 = 0o040_000;

/// 逐段折叠 `path`。语义与 `realpath(3)` 一致：绝对路径从根开始，
/// 相对路径从 cwd 开始；`.` 丢掉、`..` 退一段、符号链把目标接到未解部分前面。
pub fn real_path<T: PathProbe + ?Sized>(probe: &T, path: &str) -> Result<String, RealPathError> {
    if path.is_empty() {
        // `getcwd.c:104-107`：空串是 `ENOENT`，不是 `EINVAL`。
        return Err(RealPathError::NoEntry);
    }

    let mut resolved = String::new();
    if !path.starts_with('/') {
        let cwd = probe
            .working_directory()
            .ok_or(RealPathError::NoWorkingDirectory)?;
        resolved.push_str(cwd.trim_end_matches('/'));
    }

    let mut unresolved = String::from(path);
    let mut links_seen = 0u32;
    // C 用两个缓冲轮流装「链接目标 + 余下路径」；本层直接用 String 换持，
    // 效果是同一条：解析出的目标串接在未解部分之前继续循环。
    loop {
        while unresolved.starts_with('/') {
            unresolved.remove(0);
        }
        if unresolved.is_empty() {
            if resolved.is_empty() {
                resolved.push('/');
            }
            return Ok(resolved);
        }
        let head_end = unresolved.find('/').unwrap_or(unresolved.len());
        let component = &unresolved[..head_end];
        let rest = &unresolved[head_end..];

        if component == "." {
            unresolved = String::from(rest);
            continue;
        }
        if component == ".." {
            trim_last(&mut resolved);
            unresolved = String::from(rest);
            continue;
        }

        let next_len = resolved.len() + 1 + component.len() + 1;
        if next_len > MAXPATHLEN {
            return Err(RealPathError::NameTooLong);
        }
        // `p` 在 C 里停在刚写下的那个 `/` 上，链分支靠「不推进它」来覆写链名。
        let prefix_len = resolved.len();
        let mut candidate = resolved.clone();
        candidate.push('/');
        candidate.push_str(component);

        let mode = probe
            .lstat_mode(&candidate)
            .ok_or(RealPathError::StatFailed)?;
        if mode & S_IFMT == S_IFLNK {
            links_seen += 1;
            if links_seen > MAXSYMLINKS {
                return Err(RealPathError::Loop);
            }
            let mut target = probe
                .read_link(&candidate)
                .ok_or(RealPathError::ReadLinkFailed)?;
            if target.is_empty() {
                return Err(RealPathError::NoEntry);
            }
            // 「把未解部分接在链接目标之后，然后换用它」——`getcwd.c:193-200`。
            target.push_str(rest);
            unresolved = target;
            // C 在链这一支不推进 `p`（`getcwd.c:174-206` 走完就 `goto loop`，
            // 推进那一行在 `:212`），于是下一段就**覆写**掉链名自己。绝对目标另
            // 把 `p` 拉回串首（`getcwd.c:203-205`），前缀整体作废。
            resolved.truncate(prefix_len);
            if unresolved.starts_with('/') {
                resolved = String::new();
            }
            continue;
        }
        // 这一段后面还有内容而它自己不是目录，就是 `ENOTDIR`（`getcwd.c:207-209`）。
        if !rest.is_empty() && mode & S_IFMT != S_IFDIR {
            return Err(RealPathError::NotDir);
        }
        resolved = candidate;
        unresolved = String::from(rest);
    }
}

/// 退掉最后一段。C 的写法是 `while (*--p != '/')`（`getcwd.c:150-152`），
/// 也就是至少要留一个 `/` 在根上；已经到根（`p == resolved`）就原地不动。
fn trim_last(resolved: &mut String) {
    if resolved.is_empty() {
        return;
    }
    match resolved.rfind('/') {
        // 根之上没有可退的段：`/..` 还是 `/`。
        Some(0) => resolved.clear(),
        Some(at) => {
            resolved.truncate(at);
        }
        // 只有 cwd 里带段的绝对路径才会走到这里；相对起点不退回。
        None => resolved.clear(),
    }
}

/// 一个只在宿主测里用的最小文件系统视图：路径 → （模式位，链接目标）。
#[cfg(test)]
pub(crate) struct Fixture {
    pub entries: Vec<(String, u32, Option<String>)>,
    pub cwd: String,
}

#[cfg(test)]
impl PathProbe for Fixture {
    fn lstat_mode(&self, path: &str) -> Option<u32> {
        self.entries
            .iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, mode, _)| *mode)
    }

    fn read_link(&self, path: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(p, _, _)| p == path)
            .and_then(|(_, _, target)| target.clone())
    }

    fn working_directory(&self) -> Option<String> {
        Some(self.cwd.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const REG: u32 = 0o100_644;
    const DIR: u32 = S_IFDIR | 0o755;
    const LINK: u32 = S_IFLNK | 0o777;

    fn fixture(cwd: &str, entries: &[(&str, u32, Option<&str>)]) -> Fixture {
        let mut owned = Vec::new();
        for (path, mode, target) in entries {
            owned.push((String::from(*path), *mode, (*target).map(String::from)));
        }
        Fixture {
            entries: owned,
            cwd: String::from(cwd),
        }
    }

    #[test]
    fn an_absolute_path_of_plain_directories_comes_back_cleaned() {
        let f = fixture("/", &[("/a", DIR, None), ("/a/b", DIR, None)]);
        assert_eq!(real_path(&f, "/a/b"), Ok(String::from("/a/b")));
        // 重复斜杠与尾斜杠都收掉（`getcwd.c:133-135`、`:137-141`）。
        assert_eq!(real_path(&f, "///a//b/"), Ok(String::from("/a/b")));
        assert_eq!(real_path(&f, "/a/b/"), Ok(String::from("/a/b")));
    }

    #[test]
    fn dot_and_dotdot_are_folded_without_asking_the_file_system() {
        let f = fixture("/", &[("/a", DIR, None), ("/a/b", DIR, None)]);
        assert_eq!(real_path(&f, "/a/./b"), Ok(String::from("/a/b")));
        assert_eq!(real_path(&f, "/a/b/../b"), Ok(String::from("/a/b")));
        // 根之上没有可退的段。
        assert_eq!(real_path(&f, "/../../a"), Ok(String::from("/a")));
    }

    #[test]
    fn a_missing_component_is_the_stat_failure() {
        let f = fixture("/", &[("/a", DIR, None)]);
        assert_eq!(real_path(&f, "/a/nope"), Err(RealPathError::StatFailed));
        // 空串走 ENOENT 那一支（`getcwd.c:104-107`）。
        assert_eq!(real_path(&f, ""), Err(RealPathError::NoEntry));
    }

    #[test]
    fn a_relative_path_starts_from_the_working_directory() {
        let f = fixture("/tmp", &[("/tmp", DIR, None), ("/tmp/x", REG, None)]);
        assert_eq!(real_path(&f, "x"), Ok(String::from("/tmp/x")));
        assert_eq!(real_path(&f, "./x"), Ok(String::from("/tmp/x")));
    }

    #[test]
    fn an_absolute_symlink_restarts_the_accumulated_prefix() {
        // `/l -> /a/b`：跳到绝对目标时已积累的 `/x` 整体作废。
        let f = fixture(
            "/",
            &[
                ("/x", DIR, None),
                ("/x/l", LINK, Some("/a/b")),
                ("/a", DIR, None),
                ("/a/b", DIR, None),
            ],
        );
        assert_eq!(real_path(&f, "/x/l"), Ok(String::from("/a/b")));
    }

    #[test]
    fn a_relative_symlink_lands_inside_its_own_directory() {
        // 相对目标覆写掉链名自己：C 在链这一支不推进 `p`（见 `real_path` 内注），
        // 所以 `/d/l` 且 `l -> e` 的答案是 `/d/e`，不是 `/d/l/e`。
        let f = fixture(
            "/",
            &[
                ("/d", DIR, None),
                ("/d/l", LINK, Some("e")),
                ("/d/e", REG, None),
            ],
        );
        assert_eq!(real_path(&f, "/d/l"), Ok(String::from("/d/e")));
    }

    #[test]
    fn a_symlink_chain_is_followed_to_the_end() {
        let f = fixture(
            "/",
            &[
                ("/l1", LINK, Some("/l2")),
                ("/l2", LINK, Some("/l3")),
                ("/l3", REG, None),
            ],
        );
        assert_eq!(real_path(&f, "/l1"), Ok(String::from("/l3")));
    }

    #[test]
    fn too_many_symlink_hops_is_a_loop() {
        let mut entries = Vec::new();
        for i in 0..=MAXSYMLINKS {
            entries.push((
                alloc::format!("/l{}", i),
                LINK,
                Some(alloc::format!("/l{}", i + 1)),
            ));
        }
        entries.push((alloc::format!("/l{}", MAXSYMLINKS + 1), REG, None));
        let f = Fixture {
            entries,
            cwd: String::from("/"),
        };
        // 第 MAXSYMLINKS+1 跳才判环（C 的 `nlnk++ >= MAXSYMLINKS`，`getcwd.c:180-183`）。
        assert_eq!(real_path(&f, "/l0"), Err(RealPathError::Loop));
    }

    #[test]
    fn a_non_directory_in_the_middle_is_enotdir() {
        let f = fixture("/", &[("/f", REG, None)]);
        assert_eq!(real_path(&f, "/f/sub"), Err(RealPathError::NotDir));
        // 末段是普通文件则正当（`getcwd.c:207` 的判据带上「后面还有 `/`」）。
        assert_eq!(real_path(&f, "/f"), Ok(String::from("/f")));
    }

    #[test]
    fn a_path_beyond_the_buffer_is_enametooolong() {
        let long = alloc::format!("/{}", "a".repeat(MAXPATHLEN));
        let f = fixture("/", &[]);
        assert_eq!(real_path(&f, &long), Err(RealPathError::NameTooLong));
    }

    #[test]
    fn a_broken_link_target_reports_through_the_readlink_leg() {
        // `lstat` 说它是链，`readlink` 却给不出内容：两支错误都要能报出来。
        let f = Fixture {
            entries: alloc::vec![
                (String::from("/l"), LINK, None),
                (String::from("/e"), LINK, Some(String::new())),
            ],
            cwd: String::from("/"),
        };
        assert_eq!(real_path(&f, "/l"), Err(RealPathError::ReadLinkFailed));
        assert_eq!(real_path(&f, "/e"), Err(RealPathError::NoEntry));
    }

    #[test]
    fn the_root_alone_resolves_to_itself() {
        let f = fixture("/", &[("/", DIR, None)]);
        assert_eq!(real_path(&f, "/"), Ok(String::from("/")));
        assert_eq!(real_path(&f, "//"), Ok(String::from("/")));
        assert_eq!(real_path(&f, "/.."), Ok(String::from("/")));
    }
}
