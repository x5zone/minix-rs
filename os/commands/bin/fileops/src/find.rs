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
    /// `-ok ... +`：POSIX 说不必支持，C 直接拒（`function.c:672-675`）。
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
    /// （`function.c:645-660`）；`-ok … +` 被 C 拒掉（`function.c:672-675`）。
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

        // `+` 不在 `{}` 之后就不是收尾（C 的 `lastbrace` 判据，`function.c:647-657`），
        // 于是它被当成普通参数收进去，随后因缺收尾而报错。
        assert_eq!(
            parse(&[".", "-exec", "grep", "+", "-print"]),
            Err(FindError::NoTerminator("-exec"))
        );
        // `-ok … +` 被 C 明写拒绝（`function.c:672-675`）。
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
