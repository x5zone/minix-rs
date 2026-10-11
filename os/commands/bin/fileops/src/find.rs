//! `find` 的选项面与表达式解析（决定半第一包）。
//!
//! 真源是 `minix3/usr.bin/find/`（NetBSD 派，3315 行分七件）：`main.c` 收全局选项与
//! 路径/表达式的分界，`option.c` 是那张按字典序排过的主表（51 项，带「要不要参数」一位），
//! `find.c` 把命令行折成计划表并补隐式 `-print`，`operator.c` 再按优先级把 `()`、`!`、`-o`
//! 三段「压实」成树。本包只做这套语法与位操作，不做取值：遍历与判据（`function.c` 的
//! `f_*` 一族）在下一包，薄壳在其后。
//!
//! 值得先讲清的一处结构：C 的计划表是**链表**，相邻节点天然是「与」——`find_execute`
//! 一路求值到某个节点返回假就停（`find.c:245` 的 `for (p = plan; p && (p->eval)(p, ...); p = p->next)`），
//! 所以 `-a`/`-and` 的构造函数是 `c_null`（返回空、不产生节点，`function.c:1867`）。本包
//! 把这条隐式「与」显式成序列（`Plan`），语义与 C 同形；`!` 与 `-o` 才产生树节点。
//!
//! 另一处是 `find.c:104-107` 那条注释与代码不合：注释说只有 `-print`/`-ok`/`-fprint`/
//! `-exec`/`-exit` 算「有输出」，代码里置 `isoutput` 的其实是九处（另含 `-delete`、
//! `-execdir`、`-ls`、`-print0`、`-printx`）。按真值优先序取代码的行为。

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// `fts_open` 的选项位。位值照 `minix3/include/fts.h:71-77`；本层不传给真的 fts，
/// 下一包的遍历腿按这些位决定怎么走，所以位值只用来彼此区分。
pub mod fts {
    /// `fts.h:71`：跟随命令行上的符号链。
    pub const COMFOLLOW: u32 = 0x001;
    /// `fts.h:72`：逻辑遍历（对符号链本身取 `stat` 后的目标）。
    pub const LOGICAL: u32 = 0x002;
    /// `fts.h:74`：不为目录项取 `stat`。初值就带着它。
    pub const NOSTAT: u32 = 0x008;
    /// `fts.h:75`：物理遍历（对符号链本身取属性）。初值就带着它。
    pub const PHYSICAL: u32 = 0x010;
    /// `fts.h:77`：不跨越设备边界。
    pub const XDEV: u32 = 0x040;
}

/// 全局选项折叠出的遍历开关位，按 `main.c:88-124` 的位操作逐条照搬。
///
/// 关键的不显眼神：`-h` 与 `-L` 都做逻辑遍历，但 `-L` 先清 `COMFOLLOW|PHYSICAL`
/// （`main.c:96`）而 `-h` 只清 `PHYSICAL`（`main.c:113`）——所以 `-H -h` 留下的
/// `COMFOLLOW` 仍在，`-L -h` 则不带。把它们压成一个三值枚举就会丢掉这个差别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkOptions(pub u32);

impl Default for WalkOptions {
    /// `main.c:88`：初值 `FTS_NOSTAT | FTS_PHYSICAL`。
    fn default() -> Self {
        WalkOptions(fts::NOSTAT | fts::PHYSICAL)
    }
}

impl WalkOptions {
    /// `-H`（`main.c:91-94`）。
    pub fn set_capital_h(&mut self) {
        self.0 &= !fts::LOGICAL;
        self.0 |= fts::PHYSICAL | fts::COMFOLLOW;
    }

    /// `-L`（`main.c:95-98`）。
    pub fn set_capital_l(&mut self) {
        self.0 &= !(fts::COMFOLLOW | fts::PHYSICAL);
        self.0 |= fts::LOGICAL;
    }

    /// `-P`（`main.c:99-102`）。
    pub fn set_capital_p(&mut self) {
        self.0 &= !(fts::COMFOLLOW | fts::LOGICAL);
        self.0 |= fts::PHYSICAL;
    }

    /// `-h`（`main.c:112-115`）：只清 `PHYSICAL`，不动 `COMFOLLOW`。
    pub fn set_lower_h(&mut self) {
        self.0 &= !fts::PHYSICAL;
        self.0 |= fts::LOGICAL;
    }

    /// `-x`（`main.c:122-124`）：只加位，不清任何东西。
    pub fn set_lower_x(&mut self) {
        self.0 |= fts::XDEV;
    }

    /// 某位是否在。判据与下一包的遍历腿都用它。
    pub const fn has(&self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// 一个主表项的形态：需不需要紧跟一个参数。逐条抄 `option.c:59-110` 那张表的第四列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// 不带参数（`!`、`(`、`)`、`-print`、`-delete`…）。
    None,
    /// 带一个参数（`-name foo`、`-size +10`…）。
    One,
}

/// 主表的名字与元数（`option.c:59-110`）。`-exec`/`-ok`/`-execdir` 特殊：C 的第四列
/// 是 1，但它们的「参数」是吃到 `;`/`+` 为止的一串，由 [`Prim::Exec`] 自己收。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableEntry {
    pub name: &'static str,
    pub arity: Arity,
    /// 是否置 `isoutput`（决定要不要补隐式 `-print`）。
    pub produces_output: bool,
}

/// 那张表按字典序原样落一遍：C 用 `bsearch` 查它（`option.c:145-151`），顺序即语义前提。
pub const OPTIONS: &[TableEntry] = &[
    entry("!", Arity::None, false),
    entry("(", Arity::None, false),
    entry(")", Arity::None, false),
    entry("-a", Arity::None, false),
    entry("-amin", Arity::One, false),
    entry("-and", Arity::None, false),
    entry("-anewer", Arity::One, false),
    entry("-atime", Arity::One, false),
    entry("-cmin", Arity::One, false),
    entry("-cnewer", Arity::One, false),
    entry("-ctime", Arity::One, false),
    entry("-delete", Arity::None, true),
    entry("-depth", Arity::None, false),
    entry("-empty", Arity::None, false),
    entry("-exec", Arity::One, true),
    entry("-execdir", Arity::One, true),
    entry("-exit", Arity::None, true),
    entry("-false", Arity::None, false),
    entry("-flags", Arity::One, false),
    entry("-follow", Arity::None, false),
    entry("-fprint", Arity::One, true),
    entry("-fstype", Arity::One, false),
    entry("-group", Arity::One, false),
    entry("-iname", Arity::One, false),
    entry("-inum", Arity::One, false),
    entry("-iregex", Arity::One, false),
    entry("-links", Arity::One, false),
    entry("-ls", Arity::None, true),
    entry("-maxdepth", Arity::One, false),
    entry("-mindepth", Arity::One, false),
    entry("-mmin", Arity::One, false),
    entry("-mtime", Arity::One, false),
    entry("-name", Arity::One, false),
    entry("-newer", Arity::One, false),
    entry("-nogroup", Arity::None, false),
    entry("-nouser", Arity::None, false),
    entry("-o", Arity::None, false),
    entry("-ok", Arity::One, true),
    entry("-or", Arity::None, false),
    entry("-path", Arity::One, false),
    entry("-perm", Arity::One, false),
    entry("-print", Arity::None, true),
    entry("-print0", Arity::None, true),
    entry("-printx", Arity::None, true),
    entry("-prune", Arity::None, false),
    entry("-regex", Arity::One, false),
    entry("-rm", Arity::None, true),
    entry("-size", Arity::One, false),
    entry("-type", Arity::One, false),
    entry("-user", Arity::One, false),
    entry("-xdev", Arity::None, false),
];

const fn entry(name: &'static str, arity: Arity, produces_output: bool) -> TableEntry {
    TableEntry {
        name,
        arity,
        produces_output,
    }
}

/// 查表。C 用 `bsearch`，本层线性扫同一张表——顺序一致就结果一致。
pub fn lookup(name: &str) -> Option<&'static TableEntry> {
    OPTIONS.iter().find(|e| e.name == name)
}

/// 一个叶子判据或动作。参数原样留着（`&'a str`），取值语义在下一包。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prim<'a> {
    /// 带一个字符串参数的表项：`-name`、`-path`、`-perm`、`-size`、`-type`、
    /// `-inum`、`-links`、`-atime`、`-amin`、`-mmin`、`-newer`、`-user`… 一律按
    /// 「表项名 + 原文」存，避免把下一包的取值口径提前塞进解析。
    Keyword { name: &'a str, arg: &'a str },
    /// 不带参数的表项（`-print`、`-delete`、`-prune`、`-depth`、`-false`…）。
    Bare(&'a str),
    /// `-exec`/`-execdir`/`-ok`：C 从参数串一路吃到单独的 `;`（或紧跟 `{}` 之后的
    /// `+`，`function.c:647-660`）。`is_ok` 记 `-ok`/`-okdir`（`create` 的第二实参）。
    Exec {
        name: &'a str,
        is_ok: bool,
        argv: Vec<&'a str>,
        /// 收尾用的是 `+` 还是 `;`。
        plus: bool,
    },
}

/// 表达式树的一个节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<'a> {
    /// 叶子。
    Leaf(Prim<'a>),
    /// `!` 收下的那**一个**表达式（C 的 `c_not` 用 `yankexpr` 一次只取一个节点，
    /// `operator.c:175-183`；分组靠 `Paren`）。
    Not(Box<Node<'a>>),
    /// `( … )` 折成的子序列（C 的 `paren_squish` 把它变成 `N_EXPR`，`operator.c:95-112`）。
    Paren(Vec<Node<'a>>),
    /// `-o`：`left` 是它之前收下的那一段，`right` 是**余下的整个计划**
    /// （`or_squish` 的 `next->p_data[1] = or_squish(plan)`，`operator.c:252-257`），
    /// 所以 `-o` 右结合。两支各自是一段「与」序列。
    Or {
        left: Vec<Node<'a>>,
        right: Vec<Node<'a>>,
    },
}

/// 解析好的整条命令行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed<'a> {
    /// 遍历开关位。
    pub walk: WalkOptions,
    /// `-d`：目录在后序才输出（C 的 `isdepth`，`main.c:103-105`、`find.c:217-222`）。
    pub depth_first: bool,
    /// `-E`：正则用扩展式（C 把 `regcomp_flags` 设成 `REG_EXTENDED`，`main.c:106-108`）。
    pub extended_regex: bool,
    /// `-s`：同层按名排序（`issort`）。
    pub sorted: bool,
    /// `-X`：拒绝含 xargs 分隔符的路径（`isxargs`，判据在 `find.c:235-243`）。
    pub xargs_safe_only: bool,
    /// `-f` 带上来的起始路径，按出现顺序排在位置参数之前（`main.c:109-111` 先塞进数组）。
    pub extra_paths: Vec<&'a str>,
    /// 表达式之前的位置参数。
    pub paths: Vec<&'a str>,
    /// 表达式。空表达式对应 C 的 `plan == NULL`。
    pub expr: Vec<Node<'a>>,
    /// 表达式里没有任何产生输出的表项，故要补隐式 `-print`（`find.c:115-133`）。
    pub needs_implicit_print: bool,
}

/// 解析失败。`Usage` 对应 `main.c:155-163`，其余对应 `errx`/`err` 的每一支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindError<'a> {
    /// 没有起始路径（`main.c:147-149`：`p == start` 就 usage）。
    Usage,
    // 注：C 的 `main.c:128-129` 对不认识的字母是 `default: break`——getopt 继续扫下一个
    // 参数，什么都不报。本层照此实现（见 `parse` 里的 `ignored`），故不设错误支。
    /// 表达式里出现表外的词（`option.c:131-133`：`"%s: unknown option"`）。
    UnknownOption(&'a str),
    /// 表项要参数而后面没有（`option.c:135-137`：`"%s: requires additional arguments"`）。
    NeedsArguments(&'a str),
    /// `-exec` 一族没有收尾的 `;`/`+`（`function.c:649-651`）。
    NoTerminator(&'a str),
    /// `-ok ... +`：POSIX 说不必支持，C 直接拒（`function.c:666-669`）。
    OkWithPlus,
    /// `(` 没有对应的 `)`（`operator.c:96-98`）。
    UnclosedParen,
    /// `()` 里是空的（`operator.c:106-108`）。
    EmptyParens,
    /// 表达式以 `)` 收尾却不在括号里（`yanknode` 把它原样交回，`find_create` 不认识
    /// 落单 `)` 之后的算符——这一支由 `or_squish`/`not_squish` 的越界报错承担）。
    StrayCloseParen(&'a str),
    /// `!` 后面没有可收的表达式。C 让 `c_not` 收下 NULL，运行期才炸；本层在解析期
    /// 就拒绝，免得交回一棵求值时才坏的树。
    NotWithoutOperand,
}

/// 扫全局选项、划出路径段、再把剩下的当表达式解析。
///
/// 分界判据照 `main.c:133-145`：从第一个「以 `-` 开头」或「恰为 `!`/`(`」的参数起，
/// 后面的都算表达式；在此之前的一律是起始路径。
pub fn parse<'a>(args: &'a [&'a str]) -> Result<Parsed<'a>, FindError<'a>> {
    let mut walk = WalkOptions::default();
    let mut depth_first = false;
    let mut extended_regex = false;
    let mut sorted = false;
    let mut xargs_safe_only = false;
    let mut extra_paths: Vec<&'a str> = Vec::new();
    let mut ignored: Vec<char> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        let bytes = a.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'-' {
            break;
        }
        // 表达式里的 primary 也是 `-` 开头：全局段遇到任何「不认识成全局选项」的
        // `-xxx` 就停下，交回位置参数扫描（C 的 getopt 只认那串 `"HLPdEf:hsXx"`）。
        let mut k = 1usize;
        while k < bytes.len() {
            match bytes[k] {
                b'H' => walk.set_capital_h(),
                b'L' => walk.set_capital_l(),
                b'P' => walk.set_capital_p(),
                b'd' => depth_first = true,
                b'E' => extended_regex = true,
                b's' => sorted = true,
                b'X' => xargs_safe_only = true,
                b'x' => walk.set_lower_x(),
                b'h' => walk.set_lower_h(),
                b'f' => {
                    if k + 1 < bytes.len() {
                        extra_paths.push(&a[k + 1..]);
                        k = bytes.len() - 1;
                    } else if i + 1 < args.len() {
                        i += 1;
                        extra_paths.push(args[i]);
                    } else {
                        // getopt 缺参返 `?` → C 的 default 什么都不做（`main.c:128-129`）。
                        ignored.push('f');
                    }
                }
                other => ignored.push(other as char),
            }
            k += 1;
        }
        i += 1;
        // 一个全局簇里出现非法字母，C 会就此停止吃选项吗？BSD getopt 不会（它跳过
        // 非法项继续吃后面的 `-`），但它也绝不把 `-name` 当全局选项：整段以 `-n` 开
        // 头时 getopt 视 `n` 为非法并跳过——于是 `find . -name foo` 的 `-name` 被
        // 忽略掉了吗？不会：BSD getopt 在遇到第一个非选项参数前都算选项段，
        // `-name` 里的 `n` 是非法项、`ame` 同样是，全部走 default。真正拦住它们的是
        // 下面这条：C 在 getopt 之后才划路径/表达式，分界判据看的是 argv，
        // 因此本层同样先按位置参数扫，再判分界——见 `paths` 那段。
        let _ = &ignored;
    }

    // 起始路径段（`main.c:133-145`）。
    let mut paths: Vec<&'a str> = Vec::new();
    while i < args.len() {
        let a = args[i];
        if a.starts_with('-') {
            break;
        }
        if (a == "!" || a == "(") && a.len() == 1 {
            break;
        }
        paths.push(a);
        i += 1;
    }
    let rest = &args[i..];

    // C 的起始路径表 = `-f` 攒下的 + 位置参数；两边都空就 usage（`main.c:147-149`）。
    if paths.is_empty() && extra_paths.is_empty() {
        return Err(FindError::Usage);
    }

    // `parse_plan` 交回的是「表达式里有没有产生输出的表项」，隐式 `-print` 是它的反面
    // （`find.c:115-133`）。
    let (expr, has_output) = parse_plan(rest)?;
    let needs_implicit_print = !has_output;
    Ok(Parsed {
        walk,
        depth_first,
        extended_regex,
        sorted,
        xargs_safe_only,
        extra_paths,
        paths,
        expr,
        needs_implicit_print,
    })
}

/// 把表达式扫成树。
///
/// 递归下降一次成形，与 C 的三段压实等价，因为压实次序给出的优先级就是：`()` 最紧
/// （`paren_squish` 先跑），`!` 收**一个**表达式（`operator.c:175-183` 的 `yankexpr`
/// 一次只取一个节点），相邻是「与」（`find.c:245` 的求值循环一路短路），`-o` 最松且
/// 右结合（`operator.c:252-257` 把余下整个计划挂成右支）。
fn parse_plan<'a>(argv: &'a [&'a str]) -> Result<(Vec<Node<'a>>, bool), FindError<'a>> {
    let mut cur = Cursor { argv, pos: 0 };
    let (nodes, produced) = cur.parse_or()?;
    if cur.pos != argv.len() {
        // 只可能停在 `)` 上：落单的右括号。
        return Err(FindError::StrayCloseParen(argv[cur.pos]));
    }
    Ok((nodes, produced))
}

struct Cursor<'a> {
    argv: &'a [&'a str],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.argv.get(self.pos).copied()
    }

    /// 一层 `-o`：先收「与」序列，若后面是 `-o`/`-or`，右支递归取**余下的全部**。
    fn parse_or(&mut self) -> Result<(Vec<Node<'a>>, bool), FindError<'a>> {
        let (left, mut produced) = self.parse_and()?;
        if matches!(self.peek(), Some("-o") | Some("-or")) {
            self.pos += 1;
            let (right, right_produced) = self.parse_or()?;
            produced |= right_produced;
            // C 的 `or_squish` 把整个左链搬进 `p_data[0]`，计划表的头就此变成一个
            // `N_OR` 节点——返回单元素序列正是那个形状。
            return Ok((vec![Node::Or { left, right }], produced));
        }
        Ok((left, produced))
    }

    /// 「与」序列：一路吃主项，遇 `-o`/`-or`/`)`/串尾停。`-a`/`-and` 造不出节点
    /// （C 的 `c_null`，`function.c:1867-1871`）——相邻本身就是与。
    fn parse_and(&mut self) -> Result<(Vec<Node<'a>>, bool), FindError<'a>> {
        let mut out: Vec<Node<'a>> = Vec::new();
        let mut produced = false;
        while let Some(token) = self.peek() {
            if token == "-o" || token == "-or" || token == ")" {
                break;
            }
            self.pos += 1;
            let (node, node_produced) = self.parse_primary(token)?;
            produced |= node_produced;
            if let Some(node) = node {
                out.push(node);
            }
        }
        Ok((out, produced))
    }

    /// 一个主项。返回 `None` 表示这一项在 C 里产生不出节点（`-a`/`-and`）。
    fn parse_primary(&mut self, token: &'a str) -> Result<(Option<Node<'a>>, bool), FindError<'a>> {
        match token {
            "(" => {
                let (inner, produced) = self.parse_or()?;
                if self.peek() != Some(")") {
                    return Err(FindError::UnclosedParen);
                }
                self.pos += 1;
                if inner.is_empty() {
                    // C：`errx(1, "(): empty inner expression")`（`operator.c:106-108`）。
                    return Err(FindError::EmptyParens);
                }
                Ok((Some(Node::Paren(inner)), produced))
            }
            "!" => {
                let Some(operand) = self.peek() else {
                    return Err(FindError::NotWithoutOperand);
                };
                self.pos += 1;
                let (node, produced) = self.parse_primary(operand)?;
                let Some(inner) = node else {
                    // `! -a` 这种：C 把 NULL 收进 `p_data[0]`，求值期才炸。本层在解析期
                    // 就拒，交回不出一棵求值时才坏的树。
                    return Err(FindError::NotWithoutOperand);
                };
                Ok((Some(Node::Not(Box::new(inner))), produced))
            }
            "-a" | "-and" => Ok((None, false)),
            other => {
                let entry = lookup(other).ok_or(FindError::UnknownOption(other))?;
                let produces = entry.produces_output;
                if matches!(other, "-exec" | "-execdir" | "-ok") {
                    let (collected, plus) = self.parse_exec_argv(other)?;
                    return Ok((
                        Some(Node::Leaf(Prim::Exec {
                            name: other,
                            is_ok: other == "-ok",
                            argv: collected,
                            plus,
                        })),
                        produces,
                    ));
                }
                if entry.arity == Arity::One {
                    let arg = self.peek().ok_or(FindError::NeedsArguments(other))?;
                    self.pos += 1;
                    return Ok((
                        Some(Node::Leaf(Prim::Keyword { name: other, arg })),
                        produces,
                    ));
                }
                Ok((Some(Node::Leaf(Prim::Bare(other))), produces))
            }
        }
    }

    /// `-exec` 一族的参数：吃到单独的 `;` 为止，或紧跟 `{}` 的 `+`
    /// （`function.c:647-660`）；`-ok … +` 被 C 拒掉（`function.c:666-669`）。
    fn parse_exec_argv(&mut self, name: &'a str) -> Result<(Vec<&'a str>, bool), FindError<'a>> {
        let mut collected: Vec<&'a str> = Vec::new();
        let mut prev_brace = false;
        loop {
            let Some(arg) = self.peek() else {
                return Err(FindError::NoTerminator(name));
            };
            self.pos += 1;
            if arg == ";" {
                return Ok((collected, false));
            }
            if arg == "+" && prev_brace {
                if name == "-ok" {
                    return Err(FindError::OkWithPlus);
                }
                return Ok((collected, true));
            }
            prev_brace = arg == "{}";
            collected.push(arg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed<'a>(args: &'a [&'a str]) -> Parsed<'a> {
        parse(args).unwrap_or_else(|e| panic!("{args:?} 应可解析: {e:?}"))
    }

    fn bare<'a>(name: &'a str) -> Node<'a> {
        Node::Leaf(Prim::Bare(name))
    }

    fn keyword<'a>(name: &'a str, arg: &'a str) -> Node<'a> {
        Node::Leaf(Prim::Keyword { name, arg })
    }

    #[test]
    fn the_table_is_sorted_lexically_because_c_binary_searches_it() {
        // `option.c:145-151` 用 `bsearch`，表序就是查找前提（表头的注释也这么要求）。
        let names: Vec<&str> = OPTIONS.iter().map(|e| e.name).collect();
        let mut sorted_copy = names.clone();
        sorted_copy.sort_unstable();
        assert_eq!(
            names, sorted_copy,
            "主表必须按字典序，否则 C 的 bsearch 与查表不等价"
        );
        assert_eq!(OPTIONS.len(), 51, "51 项：三个算符 + 四十八个 -xxx");
    }

    #[test]
    fn arity_and_output_flags_come_straight_from_the_table() {
        assert_eq!(lookup("-name").unwrap().arity, Arity::One);
        assert_eq!(lookup("-inum").unwrap().arity, Arity::One);
        assert_eq!(lookup("-print").unwrap().arity, Arity::None);
        assert_eq!(lookup("-delete").unwrap().arity, Arity::None);
        assert!(lookup("-delete").unwrap().produces_output);
        assert!(lookup("-ls").unwrap().produces_output);
        assert!(lookup("-execdir").unwrap().produces_output);
        assert!(lookup("-printx").unwrap().produces_output);
        assert!(!lookup("-false").unwrap().produces_output);
        assert!(lookup("-nope").is_none());
    }

    #[test]
    fn the_walk_bits_start_nostat_and_physical() {
        let w = WalkOptions::default();
        assert!(w.has(fts::NOSTAT) && w.has(fts::PHYSICAL));
        assert!(!w.has(fts::LOGICAL) && !w.has(fts::COMFOLLOW) && !w.has(fts::XDEV));
    }

    #[test]
    fn capital_h_and_lower_h_leave_different_comfollow_bits() {
        // `-H -h` 仍带 COMFOLLOW（`main.c:112-115` 只清 PHYSICAL），`-L -h` 不带
        // （`-L` 在 `main.c:96` 就把它清了）。压成三值枚举就看不见这个差别。
        let mut a = WalkOptions::default();
        a.set_capital_h();
        a.set_lower_h();
        assert!(a.has(fts::COMFOLLOW) && a.has(fts::LOGICAL));
        let mut b = WalkOptions::default();
        b.set_capital_l();
        b.set_lower_h();
        assert!(!b.has(fts::COMFOLLOW) && b.has(fts::LOGICAL));
    }

    #[test]
    fn capital_p_wins_over_earlier_logical_and_x_only_adds() {
        let mut w = WalkOptions::default();
        w.set_capital_l();
        w.set_capital_p();
        assert!(w.has(fts::PHYSICAL) && !w.has(fts::LOGICAL));
        w.set_lower_x();
        assert!(w.has(fts::XDEV));
    }

    #[test]
    fn global_switches_land_on_their_own_fields() {
        let p = parsed(&["-d", "-E", "-s", "-X", "-f", "/tmp/list", "."]);
        assert!(p.depth_first && p.extended_regex && p.sorted && p.xargs_safe_only);
        assert_eq!(p.extra_paths, vec!["/tmp/list"]);
        assert_eq!(p.paths, vec!["."]);
        // `-fVALUE` 贴簇写法同值（getopt 的取值的两种写法）。
        let hug = parsed(&["-f/tmp/list", "."]);
        assert_eq!(hug.extra_paths, vec!["/tmp/list"]);
    }

    #[test]
    fn unknown_letters_before_the_path_list_are_swallowed_by_getopt() {
        // C 的真实行为：`find -name foo .` 里 `-name` 落在 getopt 的选项段，四个字母
        // 都不在 `"HLPdEf:hsXx"` 里，`main.c:128-129` 的 default 什么都不做，于是
        // `-name` 被整段吞掉，`foo` 与 `.` 都成了起始路径，表达式为空。
        let p = parsed(&["-name", "foo", "."]);
        assert_eq!(p.paths, vec!["foo", "."]);
        assert!(p.expr.is_empty());
        assert!(p.needs_implicit_print);
    }

    #[test]
    fn the_expression_starts_at_the_first_dash_bang_or_lone_paren() {
        let p = parsed(&[".", "-name", "foo"]);
        assert_eq!(p.paths, vec!["."]);
        assert_eq!(p.expr, vec![keyword("-name", "foo")]);
        // `!` 与 `(` 单独成词也算表达式起点（POSIX 的分界判据，`main.c:138-143`）。
        assert_eq!(
            parsed(&[".", "!", "-print"]).expr,
            vec![Node::Not(Box::new(bare("-print")))]
        );
        let paren = parsed(&[".", "(", "-print", ")"]);
        assert_eq!(paren.expr, vec![Node::Paren(vec![bare("-print")])]);
        // 而 `(` 后面紧跟别的东西（如 `(` 之外的 `(`x` 形态）不是分界，仍是路径。
        assert_eq!(parsed(&[".((", "-print"]).paths, vec![".(("]);
    }

    #[test]
    fn no_starting_path_at_all_is_usage() {
        assert_eq!(parse(&[]), Err(FindError::Usage));
        assert_eq!(parse(&["-print"]), Err(FindError::Usage));
        // `-f` 带上来的也算起始路径（`main.c:109-111` 先塞进同一个数组）。
        assert_eq!(
            parse(&["-f", "/etc", "-print"]).unwrap().paths,
            Vec::<&str>::new()
        );
    }

    #[test]
    fn the_implicit_print_wrap_follows_the_code_not_the_comment() {
        // `find.c:104-107` 的注释只列了五项，代码里置 `isoutput` 的其实是九处。
        assert!(parsed(&[".", "-name", "foo"]).needs_implicit_print);
        for emitter in [
            "-print", "-print0", "-printx", "-ls", "-fprint", "-delete", "-rm", "-exec", "-exit",
        ] {
            let mut with: Vec<&str> = vec![".", emitter];
            match emitter {
                "-fprint" => with.push("/tmp/x"),
                "-exec" => {
                    with.push("true");
                    with.push(";");
                }
                _ => {}
            }
            let p = parsed(&with);
            assert!(
                !p.needs_implicit_print,
                "{emitter} 该算有输出（args {with:?}）"
            );
        }
        // `-ok` 与 `-exec` 共用构造函数，也算。
        assert!(!parsed(&[".", "-ok", "true", ";"]).needs_implicit_print);
    }

    #[test]
    fn adjacency_is_implicit_and_and_the_and_operators_leave_no_node() {
        let p = parsed(&[".", "-name", "a", "-type", "f"]);
        assert_eq!(p.expr, vec![keyword("-name", "a"), keyword("-type", "f")]);
        let with_a = parsed(&[".", "-name", "a", "-a", "-type", "f"]);
        assert_eq!(with_a.expr, p.expr);
        let with_and = parsed(&[".", "-name", "a", "-and", "-type", "f"]);
        assert_eq!(with_and.expr, p.expr);
    }

    #[test]
    fn or_is_right_associative_and_its_left_side_is_the_whole_preceding_chain() {
        let p = parsed(&[".", "-name", "a", "-o", "-name", "b", "-o", "-name", "c"]);
        let [Node::Or { left, right }] = &p.expr[..] else {
            panic!("顶层该是一个 -o 节点: {:?}", p.expr)
        };
        assert_eq!(left, &vec![keyword("-name", "a")]);
        let [
            Node::Or {
                left: inner_left,
                right: inner_right,
            },
        ] = &right[..]
        else {
            panic!("右支该是余下的整个计划: {right:?}")
        };
        assert_eq!(inner_left, &vec![keyword("-name", "b")]);
        assert_eq!(inner_right, &vec![keyword("-name", "c")]);
    }

    #[test]
    fn not_takes_exactly_one_primary_so_a_chain_still_ands_after_it() {
        let p = parsed(&[".", "!", "-name", "foo", "-print"]);
        assert_eq!(
            p.expr,
            vec![Node::Not(Box::new(keyword("-name", "foo"))), bare("-print"),]
        );
    }

    #[test]
    fn parentheses_group_and_become_a_single_primary() {
        let p = parsed(&[".", "(", "-name", "a", "-o", "-name", "b", ")", "-print"]);
        let [Node::Paren(inner), rest] = &p.expr[..] else {
            panic!("括号应折成一个节点: {:?}", p.expr)
        };
        assert_eq!(
            inner,
            &vec![Node::Or {
                left: vec![keyword("-name", "a")],
                right: vec![keyword("-name", "b")],
            }]
        );
        assert_eq!(rest, &bare("-print"));
        // `!` 作用在整个括号上（区别于上一条：单主项只吃一个）。
        let negated = parsed(&[".", "!", "(", "-false", ")"]);
        let Node::Not(inner) = &negated.expr[0] else {
            panic!("`!` 的一个主项若是括号，就该收下整组: {:?}", negated.expr)
        };
        assert!(matches!(&**inner, Node::Paren(_)));
    }

    #[test]
    fn malformed_groupings_are_refused_at_parse_time() {
        assert_eq!(parse(&[".", "(", "-print"]), Err(FindError::UnclosedParen));
        assert_eq!(parse(&[".", "(", ")"]), Err(FindError::EmptyParens));
        assert_eq!(
            parse(&[".", "-print", ")"]),
            Err(FindError::StrayCloseParen(")"))
        );
        assert_eq!(parse(&[".", "!"]), Err(FindError::NotWithoutOperand));
        assert_eq!(
            parse(&[".", "!", "-a", "-print"]),
            Err(FindError::NotWithoutOperand)
        );
    }

    #[test]
    fn unknown_options_and_missing_arguments_name_the_offender() {
        assert_eq!(
            parse(&[".", "-nope", "x"]),
            Err(FindError::UnknownOption("-nope"))
        );
        assert_eq!(
            parse(&[".", "-name"]),
            Err(FindError::NeedsArguments("-name"))
        );
    }

    #[test]
    fn exec_collects_until_a_lone_semicolon_or_a_brace_plus() {
        let p = parsed(&[".", "-exec", "grep", "{}", "x", ";", "-print"]);
        let [
            Node::Leaf(Prim::Exec {
                name,
                is_ok,
                argv,
                plus,
            }),
            rest,
        ] = &p.expr[..]
        else {
            panic!("-exec 应先成一节: {:?}", p.expr)
        };
        assert_eq!((*name, *is_ok, *plus), ("-exec", false, false));
        assert_eq!(argv, &vec!["grep", "{}", "x"]);
        assert_eq!(rest, &bare("-print"));

        let batched = parsed(&[".", "-exec", "grep", "{}", "+"]);
        let Node::Leaf(Prim::Exec { argv, plus, .. }) = &batched.expr[0] else {
            panic!("应是 -exec 节点")
        };
        assert!(*plus);
        assert_eq!(argv, &vec!["grep", "{}"]);

        // `+` 不在 `{}` 之后就不是收尾（C 的 `lastbrace` 判据，`function.c:647-660`），
        // 于是它被当成普通参数收进去，随后因缺收尾而报错。
        assert_eq!(
            parse(&[".", "-exec", "grep", "+", "-print"]),
            Err(FindError::NoTerminator("-exec"))
        );
        // `-ok … +` 被 C 明写拒绝（`function.c:666-669`）。
        assert_eq!(
            parse(&[".", "-ok", "grep", "{}", "+"]),
            Err(FindError::OkWithPlus)
        );
    }

    #[test]
    fn an_empty_expression_is_legal_and_just_prints_everything() {
        let p = parsed(&["."]);
        assert!(p.expr.is_empty());
        assert!(p.needs_implicit_print);
        assert_eq!(p.paths, vec!["."]);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 第二包：判据求值。真源 `minix3/usr.bin/find/function.c` 的 `f_*` 一族。
// 目录项由走表腿备好交进来（`Entry`），本层不碰系统调用；需要问文件系统的
// 三件事（`-newer` 的参照、`-empty` 的目录清单、`-xdev` 的起始设备）也都做成入参。
// ─────────────────────────────────────────────────────────────────────────

/// `-size` 的默认单位：512 字节的块（`minix3/usr.bin/find/find.h` 的 `FIND_SIZE`）。
pub const FIND_SIZE: i64 = 512;

/// 一分钟的秒数（`function.c` 里的 `SECSPERMIN`）。
pub const SECS_PER_MINUTE: i64 = 60;
/// 一天的秒数（`SECSPERDAY`）。
pub const SECS_PER_DAY: i64 = 86_400;

/// 一条目录项的视图。字段对应 `FTSENT::fts_statp` 与 `fts_level`、`fts_name`、
/// `fts_path`（`minix3/include/fts.h`）。
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    /// 到目前为止的完整路径（`-path` 与 `-newer` 的落点）。
    pub path: &'a str,
    /// 目录项名（`-name` 的落点）。
    pub name: &'a str,
    /// 搜索深度。`fts_level` 从 1 起（起始路径自身是 1）。
    pub level: usize,
    /// `st_mode`。
    pub mode: u32,
    /// `st_ino`。
    pub ino: u64,
    /// `st_nlink`。
    pub nlink: u32,
    /// `st_size`。
    pub size: i64,
    /// `st_uid`。
    pub uid: u32,
    /// `st_gid`。
    pub gid: u32,
    /// `st_dev`（`-xdev` 比较用）。
    pub dev: u64,
    /// `st_atim.tv_sec`。
    pub atime: i64,
    /// `st_mtim.tv_sec`。
    pub mtime: i64,
    /// `st_ctim.tv_sec`。
    pub ctime: i64,
    /// 该目次的条目名清单，`-empty` 判空目录用。拿不到（未走表或读失败）
    /// 就交回 `None`，本层按 C 的 `opendir` 失败支判假。
    pub children: Option<&'a [&'a str]>,
}

/// 求值所需的世界状态。
#[derive(Debug, Clone, Copy)]
pub struct World {
    /// `find` 起来的那一刻（C 在 `main.c:214` 用 `time(&now)` 取一次）。
    pub now: i64,
    ///  effective uid：`-user`/`-nouser` 那两支缺口令库，这里只留给 `-newer` 之外的用途。
    pub euid: u32,
    /// effective gid。
    pub egid: u32,
    /// 起始路径所在设备，`-xdev` 用它比较（C 走 `FTS_XDEV`，即遍历腿的事）。
    pub root_dev: u64,
}

/// 比较方式。C 的 `F_EQUAL`/`F_LESSTHAN`/`F_GREATER` 三态（`function.c:68-79` 的
/// `COMPARE`），另有 `-perm -mode` 用的 `F_ATLEAST`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    Equal,
    LessThan,
    Greater,
    /// `-perm -mode`：给定位必须都已置起。
    AtLeast,
}

/// 一个带比较方式的整数参数（`find_parsenum`，`function.c:135-170`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumArg {
    pub compare: Compare,
    pub value: i64,
}

/// 数字解析失败。C 的两支 `errx`：非法数值、非法尾随字符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumError {
    /// `"%s: %s: illegal numeric value"`。
    IllegalValue(String),
    /// `"%s: %s: illegal trailing character"`。尾字符非空时说明还带了别的东西。
    IllegalTrailing(String, char),
}

/// `find_parsenum`：可选的 `+`/`-` 定比较方式，其余必须是十进制整数。
/// `suffix` 是允许的唯一尾字符（只有 `-size` 给 `'c'`）。
pub fn parse_num(option: &str, text: &str, suffix: Option<char>) -> Result<NumArg, NumError> {
    let mut chars = text.chars().peekable();
    let compare = match chars.peek() {
        Some('+') => {
            chars.next();
            Compare::Greater
        }
        Some('-') => {
            chars.next();
            Compare::LessThan
        }
        _ => Compare::Equal,
    };
    let digits: String = chars.collect();
    let number = digits.parse::<i64>();
    match number {
        // C 用 `strtoq`：一个数字都没有时值为 0 且 `endchar == str`，即非法。
        Ok(value) => Ok(NumArg { compare, value }),
        // C 的判据是 `value == 0 && endchar == str`：一个数字都没吃到才算非法数值。
        Err(_) if digits.is_empty() || !digits.starts_with(|c: char| c.is_ascii_digit()) => {
            Err(NumError::IllegalValue(String::from(option)))
        }
        Err(_) => {
            // 尾随字符：只有 `-size` 允许一个 `c`（`c_size` 把 `endch` 传成 'c'）。
            let last = digits.chars().last().unwrap_or('\0');
            if suffix == Some(last)
                && digits[..digits.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_digit())
                && !digits[..digits.len() - 1].is_empty()
            {
                let value = digits[..digits.len() - 1].parse::<i64>().unwrap_or(0);
                return Ok(NumArg { compare, value });
            }
            Err(NumError::IllegalTrailing(String::from(option), last))
        }
    }
}

/// `TIME_CORRECT`（`function.c:178-180`）：时间类判据的 `-n` 要再加一格，
/// 否则「少于一分钟」那种文件会被判到「少于零分钟」外面去。
pub fn time_correct(compare: Compare, value: i64) -> i64 {
    if compare == Compare::LessThan {
        value + 1
    } else {
        value
    }
}

/// `COMPARE(a, b)` 的三态求值。
fn compare_with(compare: Compare, a: i64, b: i64) -> bool {
    match compare {
        Compare::Equal => a == b,
        Compare::LessThan => a < b,
        Compare::Greater => a > b,
        Compare::AtLeast => true, // 只有 -perm 用，另路处理
    }
}

/// 整除前先把差值抬高 `unit - 1`（C 的 `(now - t + UNIT - 1) / UNIT`），
/// 也就是「不足一格按一格算」。Rust 的 `/` 与 C 一样向零取整。
fn in_units(delta: i64, unit: i64) -> i64 {
    (delta + unit - 1) / unit
}

/// 判据求值的失败面。两类：C 在建计划期就 `errx` 的参数，以及本树缺腿的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeError {
    /// 数字类参数不合法（`find_parsenum` 的两支 `errx`）。
    BadNumber(NumError),
    /// `-type` 的字母不在表里（`c_type` 的 default 支）。
    UnknownType(String),
    /// `-perm` 的模式串解析不了（`c_perm` 的 `setmode` 返回空）。
    BadPerm(String),
    /// 本树尚无该腿：`-user`/`-group`/`-nouser`/`-nogroup` 要口令库（同 `chown` 的前置），
    /// `-exec`/`-ok`/`-execdir` 要进程创建，`-delete` 要写根盘，`-fstype` 要设备名对照，
    /// `-flags` 要 Minix 的旗标字符表，`-regex`/`-iregex` 要 `minix-regex` 的接法。
    /// 一律按名拒绝而不是悄悄判假。
    Unwired(String),
}

/// `-perm` 的两位状态：模式位与是否 `-mode` 形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermTest {
    pub compare: Compare,
    pub bits: u32,
}

/// 解析 `-perm` 的参数（`c_perm`，`function.c:1444-1470`）。前导 `-` 定 `F_ATLEAST`；
/// 其余交给 `mode::apply_mode` 以 0 为基数，等价于 C 的 `getmode(set, 0)`。
pub fn parse_perm(text: &str) -> Result<PermTest, JudgeError> {
    let (compare, rest) = match text.strip_prefix('-') {
        Some(tail) => (Compare::AtLeast, tail),
        None => (Compare::Equal, text),
    };
    let bits = crate::mode::apply_mode(0, rest)
        .map_err(|_| JudgeError::BadPerm(String::from(text)))? as u32;
    Ok(PermTest { compare, bits })
}

/// `-perm` 求值。C 只看那 12 位（setuid/setgid/sticky 加九位权限），
/// 归档两位在 `S_IFMT` 之外故不参与（`f_perm` 的掩码，`function.c:1430-1441`）。
const PERM_MASK: u32 = 0o7777;

pub fn judge_perm(entry: &Entry, test: &PermTest) -> bool {
    let mode = entry.mode & PERM_MASK;
    if test.compare == Compare::AtLeast {
        (test.bits | mode) == mode
    } else {
        mode == test.bits
    }
}

/// `-type` 的字母到 `S_IF*` 的映射（`c_type`，`function.c:1668-1723`）。
pub fn type_mask(letter: &str) -> Result<u32, JudgeError> {
    use crate::cp::*;
    Ok(match letter {
        "b" => S_IFBLK,
        "c" => S_IFCHR,
        "d" => S_IFDIR,
        "f" => S_IFREG,
        "l" => S_IFLNK,
        "p" => S_IFIFO,
        "s" => S_IFSOCK,
        // Minix 定义了 `S_IFWHT`（`sys/stat.h:147`），C 在 `#ifdef` 里连给 `w` 与 `W` 两个字母。
        "w" | "W" => crate::mode::S_IFWHT,
        other => return Err(JudgeError::UnknownType(String::from(other))),
    })
}

/// 一条已求值的判据是否要求「跳过本目次」或「剪枝」。`-prune` 恒真并置标志，
/// 与 C 的 `f_prune` 一样（`function.c:1542-1560`：设 `fts_function = FTS_SKIP` 后返 1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 什么都不做，只是真值。
    None,
    /// 不再下探这一支。
    Prune,
    /// 输出这一条的路径（`-print` 一族）。
    Print(PrintForm),
}

/// 三种打印形（`f_print`/`f_print0`/`f_printx`，`function.c:1475-1500`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintForm {
    /// 路径加换行。
    Line,
    /// 路径加 NUL。
    Nul,
    /// 经 xargs 安全转义的形态：空格与控制字符前加反斜杠。
    Xargs,
}

/// 求值一条叶子的结果：真假，外加本层要顺带记下的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub truth: bool,
    pub action: Action,
}

impl Outcome {
    fn truthy(truth: bool) -> Outcome {
        Outcome {
            truth,
            action: Action::None,
        }
    }
}

/// `-newer` 三兄弟要问参照文件的时间。走表腿提供这个查询，本层保持无系统调用。
pub trait ReferenceStat {
    /// 参照路径的 `st_atim`/`st_mtim`/`st_ctim`，取不到交回 `None`
    /// （C 在 `c_newer` 里就 `err(1)` 退出，故薄壳侧要先把失败当成建计划期错误处理）。
    fn times_of(&self, path: &str) -> Option<(i64, i64, i64)>;
}

/// 求值一条叶子判据。`spec` 来自第一包的解析结果，参数原样带着文本。
pub fn judge_leaf<'a>(
    world: &World,
    refs: &dyn ReferenceStat,
    entry: &Entry<'a>,
    name: &str,
    arg: Option<&'a str>,
    // C 的那个全局旗标（`divsize`）的反面：整棵树里出现过 `-size Nc` 就按字节比。
    // 做成入参是为了让「一处 `c` 影响全树」这条在调用处也看得见。
    size_counts_bytes: bool,
) -> Result<Outcome, JudgeError> {
    use crate::fnmatch as glob;
    let text = || {
        arg.ok_or(JudgeError::BadNumber(NumError::IllegalValue(String::from(
            name,
        ))))
    };
    match name {
        "-name" => Ok(Outcome::truthy(glob::matches_bool(text()?, entry.name, 0))),
        "-iname" => Ok(Outcome::truthy(glob::matches_bool(
            text()?,
            entry.name,
            glob::flags::CASEFOLD,
        ))),
        "-path" => Ok(Outcome::truthy(glob::matches_bool(text()?, entry.path, 0))),
        // C 用 `FNM_CASEFOLD` 走 `fnmatch`，`-ipath` 这一支 Minix 没有，故不发明。
        "-type" => {
            let mask = type_mask(text()?)?;
            Ok(Outcome::truthy(entry.mode & crate::cp::S_IFMT == mask))
        }
        "-inum" => {
            let a = parse_num("-inum", text()?, None).map_err(JudgeError::BadNumber)?;
            Ok(Outcome::truthy(compare_with(
                a.compare,
                entry.ino as i64,
                a.value,
            )))
        }
        "-links" => {
            let a = parse_num("-links", text()?, None).map_err(JudgeError::BadNumber)?;
            Ok(Outcome::truthy(compare_with(
                a.compare,
                entry.nlink as i64,
                a.value,
            )))
        }
        "-size" => {
            let a = parse_num("-size", text()?, Some('c')).map_err(JudgeError::BadNumber)?;
            // C 的 `divsize` 初值为 1（按块），任何一处 `-size Nc` 把它置成 0（按字节）
            // 且**不再置回**（`c_size`，`function.c:1638-1654`）——于是一棵树里只要写过
            // 一次 `c`，之后所有 `-size` 都按字节比。本层把该旗标算成整棵树的属性传入。
            let measured = if size_counts_bytes {
                entry.size
            } else {
                (entry.size + FIND_SIZE - 1) / FIND_SIZE
            };
            Ok(Outcome::truthy(compare_with(a.compare, measured, a.value)))
        }
        "-atime" => {
            let a = parse_num("-atime", text()?, None).map_err(JudgeError::BadNumber)?;
            let v = time_correct(a.compare, a.value);
            Ok(Outcome::truthy(compare_with(
                a.compare,
                in_units(world.now - entry.atime, SECS_PER_DAY),
                v,
            )))
        }
        "-mtime" => {
            let a = parse_num("-mtime", text()?, None).map_err(JudgeError::BadNumber)?;
            let v = time_correct(a.compare, a.value);
            Ok(Outcome::truthy(compare_with(
                a.compare,
                in_units(world.now - entry.mtime, SECS_PER_DAY),
                v,
            )))
        }
        "-ctime" => {
            let a = parse_num("-ctime", text()?, None).map_err(JudgeError::BadNumber)?;
            let v = time_correct(a.compare, a.value);
            Ok(Outcome::truthy(compare_with(
                a.compare,
                in_units(world.now - entry.ctime, SECS_PER_DAY),
                v,
            )))
        }
        "-amin" => {
            let a = parse_num("-amin", text()?, None).map_err(JudgeError::BadNumber)?;
            let v = time_correct(a.compare, a.value);
            Ok(Outcome::truthy(compare_with(
                a.compare,
                in_units(world.now - entry.atime, SECS_PER_MINUTE),
                v,
            )))
        }
        "-mmin" => {
            let a = parse_num("-mmin", text()?, None).map_err(JudgeError::BadNumber)?;
            let v = time_correct(a.compare, a.value);
            Ok(Outcome::truthy(compare_with(
                a.compare,
                in_units(world.now - entry.mtime, SECS_PER_MINUTE),
                v,
            )))
        }
        "-cmin" => {
            let a = parse_num("-cmin", text()?, None).map_err(JudgeError::BadNumber)?;
            let v = time_correct(a.compare, a.value);
            Ok(Outcome::truthy(compare_with(
                a.compare,
                in_units(world.now - entry.ctime, SECS_PER_MINUTE),
                v,
            )))
        }
        "-newer" | "-anewer" | "-cnewer" => {
            let (at, mt, ct) = refs
                .times_of(text()?)
                .ok_or_else(|| JudgeError::BadNumber(NumError::IllegalValue(String::from(name))))?;
            let mine = match name {
                "-anewer" => entry.atime,
                "-cnewer" => entry.ctime,
                _ => entry.mtime,
            };
            let reference = match name {
                "-anewer" => at,
                "-cnewer" => ct,
                _ => mt,
            };
            Ok(Outcome::truthy(mine > reference))
        }
        "-perm" => {
            let test = parse_perm(text()?)?;
            Ok(Outcome::truthy(judge_perm(entry, &test)))
        }
        "-empty" => {
            if entry.mode & crate::cp::S_IFMT == crate::cp::S_IFREG && entry.size == 0 {
                return Ok(Outcome::truthy(true));
            }
            if entry.mode & crate::cp::S_IFMT != crate::cp::S_IFDIR {
                return Ok(Outcome::truthy(false));
            }
            // 目录：C 逐条 `readdir`，凡是「不是只有 `.` 与 `..`」的条目都算不空
            // （`f_empty`，`function.c:444-470`）。清单拿不到按 C 的 `opendir` 失败支判假。
            let empty = match entry.children {
                None => return Ok(Outcome::truthy(false)),
                Some(list) => list.iter().all(|n| *n == "." || *n == ".."),
            };
            Ok(Outcome::truthy(empty))
        }
        "-maxdepth" | "-mindepth" => {
            let depth: usize = text()?
                .parse()
                .map_err(|_| JudgeError::BadNumber(NumError::IllegalValue(String::from(name))))?;
            Ok(Outcome::truthy(if name == "-maxdepth" {
                entry.level <= depth
            } else {
                entry.level >= depth
            }))
        }
        "-prune" => Ok(Outcome {
            truth: true,
            action: Action::Prune,
        }),
        "-print" => Ok(Outcome {
            truth: true,
            action: Action::Print(PrintForm::Line),
        }),
        "-print0" => Ok(Outcome {
            truth: true,
            action: Action::Print(PrintForm::Nul),
        }),
        "-printx" => Ok(Outcome {
            truth: true,
            action: Action::Print(PrintForm::Xargs),
        }),
        // 注：Minix3 的表里只有 `-false`，没有 bash 的 `-true`（`option.c:60-110`），
        // 故这里不设 `-true` 一支——它会落到下面的按名拒绝里。
        "-false" => Ok(Outcome::truthy(false)),
        // 缺腿的按名拒绝，绝不悄悄判假。
        "-user" | "-group" | "-nouser" | "-nogroup" | "-exec" | "-execdir" | "-ok" | "-delete"
        | "-rm" | "-fstype" | "-flags" | "-regex" | "-iregex" | "-newerest" | "-mindepth0" => {
            Err(JudgeError::Unwired(String::from(name)))
        }
        other => Err(JudgeError::Unwired(String::from(other))),
    }
}

/// 整棵树里是否出现过 `-size Nc`，即 C 的 `divsize` 是否已被置成「按字节」。
/// 一旦置起就不再回落（`c_size`，`function.c:1638-1654`），所以扫的是整棵表达式
/// 而不是当前正在求值的那一条 `-size`。
pub fn size_counts_bytes(nodes: &[Node<'_>]) -> bool {
    nodes.iter().any(node_asks_bytes)
}

fn node_asks_bytes(node: &Node<'_>) -> bool {
    match node {
        Node::Leaf(Prim::Keyword { name, arg }) => *name == "-size" && arg.ends_with('c'),
        Node::Leaf(Prim::Bare(_)) | Node::Leaf(Prim::Exec { .. }) => false,
        Node::Not(inner) => node_asks_bytes(inner),
        Node::Paren(inner) => size_counts_bytes(inner),
        Node::Or { left, right } => size_counts_bytes(left) || size_counts_bytes(right),
    }
}

#[cfg(test)]
mod judge_tests {
    use super::*;

    fn entry<'a>(path: &'a str, name: &'a str, mode: u32) -> Entry<'a> {
        Entry {
            path,
            name,
            level: 2,
            mode,
            ino: 77,
            nlink: 3,
            size: 2048,
            uid: 0,
            gid: 0,
            dev: 1,
            atime: 0,
            mtime: 0,
            ctime: 0,
            children: None,
        }
    }

    struct NoRefs;
    impl ReferenceStat for NoRefs {
        fn times_of(&self, path: &str) -> Option<(i64, i64, i64)> {
            (path == "/ref").then_some((10, 20, 30))
        }
    }

    fn world() -> World {
        World {
            now: 86_400 * 3,
            euid: 0,
            egid: 0,
            root_dev: 1,
        }
    }

    fn judge<'a>(e: &Entry<'a>, name: &str, arg: Option<&'a str>) -> Result<Outcome, JudgeError> {
        judge_leaf(&world(), &NoRefs, e, name, arg, false)
    }

    #[test]
    fn num_args_carry_the_three_comparisons() {
        assert_eq!(
            parse_num("-size", "10", None).unwrap(),
            NumArg {
                compare: Compare::Equal,
                value: 10
            }
        );
        assert_eq!(
            parse_num("-size", "+10", None).unwrap().compare,
            Compare::Greater
        );
        assert_eq!(
            parse_num("-size", "-10", None).unwrap().compare,
            Compare::LessThan
        );
        assert!(matches!(
            parse_num("-size", "x", None),
            Err(NumError::IllegalValue(_))
        ));
        assert!(matches!(
            parse_num("-inum", "10k", None),
            Err(NumError::IllegalTrailing(_, 'k'))
        ));
        // 只有 -size 带得动那个尾字符 c（c_size 把 endch 传成 'c'）。
        assert_eq!(parse_num("-size", "10c", Some('c')).unwrap().value, 10);
        assert!(matches!(
            parse_num("-inum", "10c", None),
            Err(NumError::IllegalTrailing(_, 'c'))
        ));
    }

    #[test]
    fn time_correction_only_moves_the_less_than_case() {
        assert_eq!(time_correct(Compare::LessThan, 1), 2);
        assert_eq!(time_correct(Compare::Equal, 1), 1);
        assert_eq!(time_correct(Compare::Greater, 1), 1);
    }

    #[test]
    fn day_and_minute_buckets_round_up_before_dividing() {
        let mut e = entry("/etc/rc", "rc", 0o100644);
        // 差 1.5 天：C 的 (delta + DAY - 1) / DAY 把它算成 2 天。
        e.atime = world().now - 86_400 - 43_200;
        assert!(judge(&e, "-atime", Some("2")).unwrap().truth);
        assert!(!judge(&e, "-atime", Some("1")).unwrap().truth);
        // 差 90 秒：分钟格算成 2。
        e.mtime = world().now - 90;
        assert!(judge(&e, "-mmin", Some("2")).unwrap().truth);
        // `-amin -1` 经 TIME_CORRECT 变成「少于一分钟」，故 90 秒不算少于一分钟。
        e.ctime = world().now - 90;
        assert!(!judge(&e, "-cmin", Some("-1")).unwrap().truth);
        e.ctime = world().now - 30;
        assert!(judge(&e, "-cmin", Some("-1")).unwrap().truth);
    }

    #[test]
    fn wildcards_route_through_the_fnmatch_port() {
        let e = entry("/etc/rc.d", "rc.d", 0o100644);
        assert!(judge(&e, "-name", Some("rc?d")).unwrap().truth);
        assert!(judge(&e, "-name", Some("rc.d")).unwrap().truth);
        assert!(judge(&e, "-iname", Some("RC.D")).unwrap().truth);
        assert!(!judge(&e, "-name", Some("RC.D")).unwrap().truth);
        // -path 比的是整条路径，不是名字。
        assert!(judge(&e, "-path", Some("/etc/*")).unwrap().truth);
        assert!(!judge(&e, "-name", Some("/etc/*")).unwrap().truth);
    }

    #[test]
    fn type_letters_map_to_the_same_masks_c_uses() {
        let dir = entry("/etc", "etc", crate::cp::S_IFDIR | 0o755);
        assert!(judge(&dir, "-type", Some("d")).unwrap().truth);
        assert!(!judge(&dir, "-type", Some("f")).unwrap().truth);
        let sock = entry("/dev/x", "x", crate::cp::S_IFSOCK);
        assert!(judge(&sock, "-type", Some("s")).unwrap().truth);
        let wht = entry("/w", "w", crate::mode::S_IFWHT);
        // 白底条目两个字母都收（C 在 `#ifdef S_IFWHT` 里连给 w 与 W）。
        assert!(judge(&wht, "-type", Some("w")).unwrap().truth);
        assert!(judge(&wht, "-type", Some("W")).unwrap().truth);
        assert_eq!(
            judge(&dir, "-type", Some("q")).unwrap_err(),
            JudgeError::UnknownType(String::from("q"))
        );
    }

    #[test]
    fn inode_and_link_counts_compare_in_three_ways() {
        let e = entry("/etc/rc", "rc", 0o100644);
        assert!(judge(&e, "-inum", Some("77")).unwrap().truth);
        assert!(judge(&e, "-inum", Some("+70")).unwrap().truth);
        assert!(!judge(&e, "-inum", Some("+80")).unwrap().truth);
        assert!(judge(&e, "-links", Some("-4")).unwrap().truth);
        assert!(!judge(&e, "-links", Some("-3")).unwrap().truth);
    }

    #[test]
    fn size_counts_512_byte_blocks_until_a_c_suffix_switches_the_flag() {
        let e = entry("/etc/rc", "rc", 0o100644); // size 2048 → 4 块
        assert!(judge(&e, "-size", Some("4")).unwrap().truth);
        assert!(!judge(&e, "-size", Some("2048")).unwrap().truth);
        // divide_size = true：同一棵树里任何一处 `-size Nc` 都会把整棵切到按字节比。
        // 全局旗标置起后：2048 就是 2048，块数 4 反而不成立。
        assert!(
            judge_leaf(&world(), &NoRefs, &e, "-size", Some("2048"), true)
                .unwrap()
                .truth
        );
        assert!(
            !judge_leaf(&world(), &NoRefs, &e, "-size", Some("4"), true)
                .unwrap()
                .truth
        );
        // 不足一块按一块算：100 字节是 1 块。
        let mut small = entry("/etc/tiny", "tiny", 0o100644);
        small.size = 100;
        assert!(judge(&small, "-size", Some("1")).unwrap().truth);
    }

    #[test]
    fn the_divsize_scan_looks_through_groups_and_operators() {
        // 直接拼一棵树：`( -size 1c -o -name x )`。
        let tree = vec![Node::Or {
            left: vec![Node::Leaf(Prim::Keyword {
                name: "-size",
                arg: "1c",
            })],
            right: vec![Node::Leaf(Prim::Keyword {
                name: "-name",
                arg: "x",
            })],
        }];
        assert!(size_counts_bytes(&tree));
        // `( -size 3 )` 前面加一个 `!`：组里那条不带 c，故整棵树都不除。
        let inner = vec![Node::Leaf(Prim::Keyword {
            name: "-size",
            arg: "3",
        })];
        let nested = vec![Node::Not(Box::new(Node::Paren(inner)))];
        assert!(!size_counts_bytes(&nested));
    }
    #[test]
    fn perm_matches_exact_at_least_and_symbolic_forms() {
        let mut e = entry("/bin/sh", "sh", 0o100755);
        assert!(judge(&e, "-perm", Some("755")).unwrap().truth);
        assert!(!judge(&e, "-perm", Some("644")).unwrap().truth);
        assert!(judge(&e, "-perm", Some("-700")).unwrap().truth);
        // `-perm -600` 在 755 上是**真**：`f_perm` 比的是「给定位都已置起」，
        // 0600 两位都在 0755 里（`function.c:1430-1441`）。要假得挑一个没置起的位。
        assert!(judge(&e, "-perm", Some("-600")).unwrap().truth);
        assert!(!judge(&e, "-perm", Some("-4000")).unwrap().truth);
        e.mode = 0o104755; // setuid 也算进那 12 位
        // 掩码是 07777，setuid 那四位参与精确比较。
        assert!(judge(&e, "-perm", Some("4755")).unwrap().truth);
        assert!(!judge(&e, "-perm", Some("0755")).unwrap().truth);
        // 符号式：以 0 为基数的 `getmode` 就是那几位被置起，`-perm u=s` 要求精确相等故假。
        assert!(!judge(&e, "-perm", Some("u=s")).unwrap().truth);
        assert!(judge(&e, "-perm", Some("-u=s")).unwrap().truth);
        assert!(matches!(
            judge(&e, "-perm", Some("zzz")),
            Err(JudgeError::BadPerm(_))
        ));
    }

    #[test]
    fn empty_covers_regular_zero_dirs_and_the_opendir_failure() {
        let mut zero = entry("/etc/empty", "empty", 0o100000);
        zero.size = 0;
        assert!(judge(&zero, "-empty", None).unwrap().truth);
        assert!(
            !judge(&entry("/etc/rc", "rc", 0o100644), "-empty", None)
                .unwrap()
                .truth
        );
        let dir = crate::cp::S_IFDIR | 0o755;
        let one_file: [&str; 1] = ["a"];
        let only_dots: [&str; 2] = [".", ".."];
        let dot_only: [&str; 1] = ["."];
        let mut d = entry("/etc", "etc", dir);
        d.children = Some(&one_file);
        assert!(!judge(&d, "-empty", None).unwrap().truth);
        d.children = Some(&only_dots);
        assert!(judge(&d, "-empty", None).unwrap().truth);
        // 只有一条 `.` 也算空：C 的 `readdir` 判据把「名字不是 . 开头，或者是
        // `..` 之外还有内容」才算条目（`function.c:461-466`），单独的 `.` 被跳过。
        d.children = Some(&dot_only);
        assert!(judge(&d, "-empty", None).unwrap().truth);
        d.children = None; // opendir 失败 → C 判 0
        assert!(!judge(&d, "-empty", None).unwrap().truth);
    }

    #[test]
    fn depth_tests_read_the_level_c_reports() {
        let e = entry("/etc/rc", "rc", 0o100644); // level 2
        assert!(judge(&e, "-maxdepth", Some("2")).unwrap().truth);
        assert!(!judge(&e, "-maxdepth", Some("1")).unwrap().truth);
        assert!(judge(&e, "-mindepth", Some("2")).unwrap().truth);
        assert!(!judge(&e, "-mindepth", Some("3")).unwrap().truth);
        assert!(matches!(
            judge(&e, "-maxdepth", Some("x")),
            Err(JudgeError::BadNumber(_))
        ));
    }

    #[test]
    fn actions_come_back_with_the_truth() {
        let e = entry("/etc/rc", "rc", 0o100644);
        assert_eq!(
            judge(&e, "-prune", None).unwrap(),
            Outcome {
                truth: true,
                action: Action::Prune
            }
        );
        assert_eq!(
            judge(&e, "-print", None).unwrap().action,
            Action::Print(PrintForm::Line)
        );
        assert_eq!(
            judge(&e, "-print0", None).unwrap().action,
            Action::Print(PrintForm::Nul)
        );
        assert_eq!(
            judge(&e, "-printx", None).unwrap().action,
            Action::Print(PrintForm::Xargs)
        );
        assert_eq!(judge(&e, "-false", None).unwrap(), Outcome::truthy(false));
    }

    #[test]
    fn the_newer_family_reads_its_reference_through_the_view() {
        let mut e = entry("/etc/rc", "rc", 0o100644);
        e.atime = 11;
        e.mtime = 19;
        e.ctime = 31;
        // atime 11 > 参照的 atime 10 → 真；mtime 19 < 参照的 20 → 假；ctime 31 > 30 → 真。
        assert!(judge(&e, "-anewer", Some("/ref")).unwrap().truth);
        assert!(judge(&e, "-cnewer", Some("/ref")).unwrap().truth);
        assert!(!judge(&e, "-newer", Some("/ref")).unwrap().truth);
        assert!(matches!(
            judge(&e, "-newer", Some("/missing")),
            Err(JudgeError::BadNumber(_))
        ));
    }

    #[test]
    fn legs_the_tree_does_not_have_are_refused_by_name() {
        let e = entry("/etc/rc", "rc", 0o100644);
        for name in [
            "-user", "-group", "-nouser", "-nogroup", "-exec", "-execdir", "-ok", "-delete", "-rm",
            "-fstype", "-flags", "-regex", "-iregex",
        ] {
            assert_eq!(
                judge(&e, name, Some("x")),
                Err(JudgeError::Unwired(String::from(name))),
                "{name} 该按名拒绝而不是悄悄判假"
            );
        }
        // C 的表里也没有 -true（bash 的），同样落拒绝。
        assert_eq!(
            judge(&e, "-true", None),
            Err(JudgeError::Unwired(String::from("-true")))
        );
    }
}
