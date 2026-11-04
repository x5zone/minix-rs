//! `find`'s option surface and expression parsing (the deciding half's first
//! package).
//!
//! Ground truth: `minix3/usr.bin/find/` (the NetBSD family, 3315 lines in seven
//! files): `main.c` takes the global options and the boundary between paths and
//! the expression, `option.c` is the lexically sorted main table (51 entries
//! with a "takes an argument" bit), `find.c` folds the command line into a plan
//! and adds the implicit `-print`, and `operator.c` then compresses `()`, `!`,
//! and `-o` into a tree by precedence. This package does only that syntax and
//! bit manipulation, not the value semantics: traversal and predicates (the
//! `f_*` family in `function.c`) are the next package, and the thin shell comes
//! after it.
//!
//! One structural point worth stating first: C's plan is a **linked list**, and
//! adjacent nodes are implicitly "and" — `find_execute` stops evaluating as soon
//! as some node returns false (`find.c:252`'s `for (p = plan; p &&
//! (p->eval)(p, ...); p = p->next)`), so the constructor for `-a`/`-and` is
//! `c_null` (returns empty and produces no node, `function.c:1867`). This
//! package makes that implicit "and" explicit as a sequence (`Plan`), with the
//! same semantics as C; only `!` and `-o` produce tree nodes.
//!
//! The other point is that the comment at `find.c:104-107` disagrees with the
//! code: the comment says only `-print`/`-ok`/`-fprint`/`-exec`/`-exit` count as
//! "producing output", but nine places actually set `isoutput` (including
//! `-delete`, `-execdir`, `-ls`, `-print0`, and `-printx`). Per the truth
//! precedence order, the code's behavior wins.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use minix_sys::{Errno, Strerror};

/// Option bits of `fts_open`. The bit values follow
/// `minix3/include/fts.h:71-77`; this layer does not hand them to a real fts,
/// and the next package's traversal leg decides how to walk from these bits, so
/// the values only need to be mutually distinct.
pub mod fts {
    /// `fts.h:71`: follow symlinks given on the command line.
    pub const COMFOLLOW: u32 = 0x001;
    /// `fts.h:72`: logical walk (`stat` the target behind the symlink itself).
    pub const LOGICAL: u32 = 0x002;
    /// `fts.h:74`: do not `stat` directory entries. Part of the initial value.
    pub const NOSTAT: u32 = 0x008;
    /// `fts.h:75`: physical walk (take the attributes of the symlink itself).
    /// Part of the initial value.
    pub const PHYSICAL: u32 = 0x010;
    /// `fts.h:77`: do not cross device boundaries.
    pub const XDEV: u32 = 0x040;
}

/// The walk option bits folded from the global options, copied bit operation
/// by bit operation from `main.c:88-124`.
///
/// The key difference that is easy to miss: `-h` and `-L` both do logical
/// walks, but `-L` clears `COMFOLLOW|PHYSICAL` first (`main.c:96`) while `-h`
/// clears only `PHYSICAL` (`main.c:113`) — so `-H -h` still leaves `COMFOLLOW`
/// set, whereas `-L -h` does not. Collapsing them into a three-value enum would
/// lose that difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkOptions(pub u32);

impl Default for WalkOptions {
    /// `main.c:88`: initial value `FTS_NOSTAT | FTS_PHYSICAL`.
    fn default() -> Self {
        WalkOptions(fts::NOSTAT | fts::PHYSICAL)
    }
}

impl WalkOptions {
    /// `-H` (`main.c:91-94`).
    pub fn set_capital_h(&mut self) {
        self.0 &= !fts::LOGICAL;
        self.0 |= fts::PHYSICAL | fts::COMFOLLOW;
    }

    /// `-L` (`main.c:95-98`).
    pub fn set_capital_l(&mut self) {
        self.0 &= !(fts::COMFOLLOW | fts::PHYSICAL);
        self.0 |= fts::LOGICAL;
    }

    /// `-P` (`main.c:99-102`).
    pub fn set_capital_p(&mut self) {
        self.0 &= !(fts::COMFOLLOW | fts::LOGICAL);
        self.0 |= fts::PHYSICAL;
    }

    /// `-h` (`main.c:112-115`): clears only `PHYSICAL`, leaving `COMFOLLOW`
    /// alone.
    pub fn set_lower_h(&mut self) {
        self.0 &= !fts::PHYSICAL;
        self.0 |= fts::LOGICAL;
    }

    /// `-x` (`main.c:122-124`): sets a bit only, clears nothing.
    pub fn set_lower_x(&mut self) {
        self.0 |= fts::XDEV;
    }

    /// Whether a bit is set. Used by the predicates and by the next package's
    /// traversal leg.
    pub const fn has(&self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// The shape of a main-table entry: whether it needs a following argument.
/// Copied entry by entry from the fourth column of the table at
/// `option.c:59-110`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// No argument (`!`, `(`, `)`, `-print`, `-delete`, ...).
    None,
    /// One argument (`-name foo`, `-size +10`, ...).
    One,
}

/// Name and arity of the main table (`option.c:59-110`).
/// `-exec`/`-ok`/`-execdir` are special: C's fourth column says 1, but their
/// "argument" is a run of tokens collected up to `;`/`+`, which [`Prim::Exec`]
/// collects itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableEntry {
    pub name: &'static str,
    pub arity: Arity,
    /// Whether it sets `isoutput` (which decides whether the implicit `-print`
    /// is needed).
    pub produces_output: bool,
}

/// The table transcribed in lexical order: C searches it with `bsearch`
/// (`option.c:145-151`), so the order is a semantic precondition.
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

/// Table lookup. C uses `bsearch`; this layer scans the same table linearly —
/// the same order gives the same result.
pub fn lookup(name: &str) -> Option<&'static TableEntry> {
    OPTIONS.iter().find(|e| e.name == name)
}

/// One leaf predicate or action. Arguments are kept as-is (`&'a str`); the
/// value semantics live in the next package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prim<'a> {
    /// Entries with one string argument: `-name`, `-path`, `-perm`, `-size`,
    /// `-type`, `-inum`, `-links`, `-atime`, `-amin`, `-mmin`, `-newer`,
    /// `-user`, ... are all stored as "entry name + original text", keeping the
    /// next package's value-interpretation rules out of the parser.
    Keyword { name: &'a str, arg: &'a str },
    /// Entries without an argument (`-print`, `-delete`, `-prune`, `-depth`,
    /// `-false`, ...).
    Bare(&'a str),
    /// `-exec`/`-execdir`/`-ok`: C consumes the argument run up to a lone `;`
    /// (or a `+` immediately after `{}`, `function.c:647-660`). `is_ok` records
    /// `-ok`/`-okdir` (the second argument of `create`).
    Exec {
        name: &'a str,
        is_ok: bool,
        argv: Vec<&'a str>,
        /// Whether the terminator is `+` or `;`.
        plus: bool,
    },
    /// `-regex`/`-iregex`: C compiles at plan build (`c_regex_common`,
    /// `function.c:1571-1592`) with the pattern wrapped `^…$` — the compiled
    /// program rides the node, so a bad regexp is a parse-time fatal and the
    /// evaluation is one `is_match` over the whole path.
    Regex {
        icase: bool,
        #[allow(clippy::large_enum_variant)]
        pattern: minix_regex::pattern::Pattern,
    },
}

/// One node of the expression tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<'a> {
    /// A leaf.
    Leaf(Prim<'a>),
    /// The **one** expression `!` takes (C's `c_not` uses `yankexpr`, which
    /// takes one node at a time, `operator.c:175-183`; grouping goes through
    /// `Paren`).
    Not(Box<Node<'a>>),
    /// The sub-sequence `( … )` folds into (C's `paren_squish` turns it into
    /// `N_EXPR`, `operator.c:95-112`).
    Paren(Vec<Node<'a>>),
    /// `-o`: `left` is the segment collected before it, `right` is **the whole
    /// rest of the plan** (`or_squish`'s `next->p_data[1] = or_squish(plan)`,
    /// `operator.c:252-257`), so `-o` is right-associative. Each side is an
    /// "and" sequence.
    Or {
        left: Vec<Node<'a>>,
        right: Vec<Node<'a>>,
    },
}

/// A fully parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed<'a> {
    /// Walk option bits.
    pub walk: WalkOptions,
    /// `-d`: directories are emitted in post-order (C's `isdepth`,
    /// `main.c:103-105`, `find.c:217-222`).
    pub depth_first: bool,
    /// `-E`: regular expressions use extended syntax (C sets `regcomp_flags` to
    /// `REG_EXTENDED`, `main.c:106-108`).
    pub extended_regex: bool,
    /// `-s`: sort each level by name (`issort`).
    pub sorted: bool,
    /// `-X`: reject paths containing xargs separators (`isxargs`; the test is
    /// at `find.c:235-243`).
    pub xargs_safe_only: bool,
    /// Start paths carried in by `-f`, placed before the positional operands in
    /// order of appearance (`main.c:109-111` pushes them into the array
    /// first).
    pub extra_paths: Vec<&'a str>,
    /// Positional operands before the expression.
    pub paths: Vec<&'a str>,
    /// The expression. An empty expression corresponds to C's
    /// `plan == NULL`.
    pub expr: Vec<Node<'a>>,
    /// The expression has no output-producing entry, so the implicit `-print`
    /// must be added (`find.c:115-133`).
    pub needs_implicit_print: bool,
}

/// Parse failure. `Usage` corresponds to `main.c:155-163`; the rest correspond
/// to the individual `errx`/`err` branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindError<'a> {
    /// No start path (`main.c:147-149`: `p == start` means usage).
    Usage,
    // Note: C's `main.c:128-129` does `default: break` for an unrecognised
    // letter — getopt keeps scanning the next argument and reports nothing.
    // This layer does the same (see `ignored` in `parse`), so there is no error
    // variant for it.
    /// A word outside the table appears in the expression (`option.c:131-133`:
    /// `"%s: unknown option"`).
    UnknownOption(&'a str),
    /// An entry needs an argument and none follows (`option.c:135-137`:
    /// `"%s: requires additional arguments"`).
    NeedsArguments(&'a str),
    /// The `-exec` family has no terminating `;`/`+` (`function.c:649-651`).
    NoTerminator(&'a str),
    /// `-ok ... +`: POSIX says support is optional, and C refuses it outright
    /// (`function.c:666-669`).
    OkWithPlus,
    /// `(` has no matching `)` (`operator.c:96-98`).
    UnclosedParen,
    /// `()` is empty (`operator.c:106-108`).
    EmptyParens,
    /// The expression ends with a `)` that closes no group (`yanknode` hands it
    /// back as-is, and `find_create` does not recognise an operator after a
    /// stray `)` — this branch is covered by the out-of-bounds errors in
    /// `or_squish`/`not_squish`).
    StrayCloseParen(&'a str),
    /// `!` has no expression to take (C's `operator.c:198-199`:
    /// `node == NULL`). C lets `c_not` take NULL and blows up at run time; this
    /// layer refuses at parse time, rather than returning a tree that only fails
    /// when evaluated.
    NotWithoutOperand,
    /// `!` immediately followed by `-o` (C's `operator.c:200-201`: the yanked
    /// node type is `N_OR`) — the same `c_not` as the previous variant, but C
    /// has two separate sentences: "nothing after !" and "nothing between ! and
    /// -o". Consecutive `!` are first eliminated and counted by C's
    /// `while (node->type == N_NOT)`; this layer reaches the same test by
    /// recursion.
    NotBeforeOr,
    /// `-o` has no expression on its left (`operator.c:253-255`:
    /// `result == NULL`).
    OrWithoutLeft,
    /// `-o` has no expression on its right (`operator.c:257-259`:
    /// `or_squish(plan)` returns NULL). The two guards are ordered in C (left
    /// first, then right), so `find . -o` reports "nothing on the left", not
    /// "nothing on the right".
    OrWithoutRight,
    /// A `-regex`/`-iregex` pattern failed `regcomp` (`c_regex_common`'s
    /// `errx(1, "regexp %s: %s", regexp, errbuf)`, `function.c:1588-1591`).
    /// Carries the pattern and the fixed sentence standing in for C's
    /// `regerror` text (this build's engine answers two shapes).
    Regex(&'a str, &'static str),
}

/// Scan the global options, carve out the path section, and parse the rest as
/// the expression.
///
/// The boundary test follows `main.c:133-145`: starting from the first argument
/// that "starts with `-`" or "is exactly `!`/`(`", everything after it is the
/// expression; everything before is a start path.
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
        // Primary expressions in the expression also start with `-`: the global
        // section stops at any `-xxx` it does not recognise as a global option
        // and hands control back to the positional scan (C's getopt recognises
        // only the string `"HLPdEf:hsXx"`).
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
                        // getopt returns `?` for a missing argument, and C's
                        // default does nothing (`main.c:128-129`).
                        ignored.push('f');
                    }
                }
                other => ignored.push(other as char),
            }
            k += 1;
        }
        i += 1;
        // Does C stop consuming options when an illegal letter appears in a
        // global cluster? BSD getopt does not (it skips the illegal item and
        // keeps consuming later `-` tokens), but it also never treats `-name`
        // as a global option: when the whole token starts with `-n`, getopt
        // regards `n` as illegal and skips it — so is `-name` in
        // `find . -name foo` ignored? No: BSD getopt treats everything up to
        // the first non-option argument as the option section, and in `-name`
        // both `n` and `ame` are illegal, all going through default. What really
        // stops them is the next rule: C carves out paths/expression only after
        // getopt, and the boundary test looks at argv, so this layer likewise
        // scans positional operands first and tests the boundary afterwards —
        // see the `paths` section.
        let _ = &ignored;
    }

    // Start-path section (`main.c:133-145`).
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

    // C's start-path list = what `-f` accumulated + the positional operands;
    // when both are empty it is usage (`main.c:147-149`).
    if paths.is_empty() && extra_paths.is_empty() {
        return Err(FindError::Usage);
    }

    // `parse_plan` returns "does the expression contain an output-producing
    // entry", and the implicit `-print` is its negation (`find.c:115-133`).
    let (expr, has_output) = parse_plan(rest, extended_regex)?;
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

/// Scan the expression into a tree.
///
/// One recursive-descent pass, equivalent to C's three compression stages,
/// because the precedence those stages produce is: `()` tightest
/// (`paren_squish` runs first), `!` takes **one** expression
/// (`operator.c:175-183`'s `yankexpr` takes one node at a time), adjacency is
/// "and" (`find.c:252`'s evaluation loop short-circuits along it), and `-o` is
/// loosest and right-associative (`operator.c:252-257` hangs the whole
/// remaining plan on the right branch).
fn parse_plan<'a>(
    argv: &'a [&'a str],
    extended_regex: bool,
) -> Result<(Vec<Node<'a>>, bool), FindError<'a>> {
    let mut cur = Cursor {
        argv,
        pos: 0,
        extended_regex,
    };
    let (nodes, produced) = cur.parse_or()?;
    if cur.pos != argv.len() {
        // The only possible stop is a `)`: a stray closing parenthesis.
        return Err(FindError::StrayCloseParen(argv[cur.pos]));
    }
    Ok((nodes, produced))
}

struct Cursor<'a> {
    argv: &'a [&'a str],
    pos: usize,
    /// `-E`'s dialect state (`regcomp_flags = REG_EXTENDED`, main.c:107):
    /// the `-regex` wrap and compile need it at expression-parse time.
    extended_regex: bool,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.argv.get(self.pos).copied()
    }

    /// One `-o` level: collect the "and" sequence first; if an `-o`/`-or`
    /// follows, the right branch recursively takes **all the rest**.
    fn parse_or(&mut self) -> Result<(Vec<Node<'a>>, bool), FindError<'a>> {
        let (left, mut produced) = self.parse_and()?;
        if matches!(self.peek(), Some("-o") | Some("-or")) {
            self.pos += 1;
            // C's first guard runs before the assignment: `result == NULL`
            // means "the left side is empty", so `find . -o …` reports
            // "-o: no expression before -o" rather than the other sentence.
            if left.is_empty() {
                return Err(FindError::OrWithoutLeft);
            }
            let (right, right_produced) = self.parse_or()?;
            // Second guard: `or_squish(plan)` returning NULL means no
            // expression on the right.
            if right.is_empty() {
                return Err(FindError::OrWithoutRight);
            }
            produced |= right_produced;
            // C's `or_squish` moves the whole left chain into `p_data[0]`, so
            // the plan head becomes an `N_OR` node — returning a one-element
            // sequence is exactly that shape.
            return Ok((vec![Node::Or { left, right }], produced));
        }
        Ok((left, produced))
    }

    /// The "and" sequence: consume primaries until `-o`/`-or`/`)` or the end of
    /// the argument list. `-a`/`-and` produce no node (C's `c_null`,
    /// `function.c:1867-1871`) — adjacency itself is the "and".
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

    /// One primary. Returning `None` means this item produces no node in C
    /// (`-a`/`-and`).
    fn parse_primary(&mut self, token: &'a str) -> Result<(Option<Node<'a>>, bool), FindError<'a>> {
        match token {
            "(" => {
                let (inner, produced) = self.parse_or()?;
                if self.peek() != Some(")") {
                    return Err(FindError::UnclosedParen);
                }
                self.pos += 1;
                if inner.is_empty() {
                    // C: `errx(1, "(): empty inner expression")`
                    // (`operator.c:106-108`).
                    return Err(FindError::EmptyParens);
                }
                Ok((Some(Node::Paren(inner)), produced))
            }
            "!" => {
                let Some(operand) = self.peek() else {
                    return Err(FindError::NotWithoutOperand);
                };
                // C's `c_not` yanks one node and then asks for its type:
                // `N_OR` means "nothing between ! and -o" (`operator.c:200-201`).
                // This layer recurses, letting an inner `!` hit the same test,
                // equivalent to C's `while (node->type == N_NOT)` eliminating and
                // counting the nots first.
                if matches!(operand, "-o" | "-or") {
                    return Err(FindError::NotBeforeOr);
                }
                self.pos += 1;
                let (node, produced) = self.parse_primary(operand)?;
                let Some(inner) = node else {
                    // The `! -a` case: C's yank gets an `N_AND`/`c_null` node
                    // (not NULL), so C does **not** report an error; this layer
                    // cannot return a node and must refuse here — a declared
                    // divergence, recorded in this module's head note and in the
                    // document-06 ledger (copying C would first require `-a` to
                    // be a participating node shape).
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
                    // `c_regex_common` (`function.c:1571-1592`): compile at
                    // plan build with the whole-path anchor wrapper — BRE
                    // wraps `^\\(re\\)$` (the escapes make the anchors bind
                    // the user's expression as one group), ERE wraps
                    // `^(re)$`. The compiled program rides the node; the
                    // engine answers two failure shapes standing in for C's
                    // `regerror` sentences.
                    if matches!(other, "-regex" | "-iregex") {
                        let icase = other == "-iregex";
                        let wrapped = if self.extended_regex {
                            format!("^({arg})$")
                        } else {
                            format!("^\\({arg}\\)$")
                        };
                        let compiled = if self.extended_regex {
                            if icase {
                                minix_regex::pattern::compile_extended_icase(&wrapped)
                            } else {
                                minix_regex::pattern::compile_extended(&wrapped)
                            }
                        } else if icase {
                            minix_regex::pattern::compile_basic_icase(&wrapped)
                        } else {
                            minix_regex::pattern::compile_basic(&wrapped)
                        };
                        let pattern = compiled.map_err(|e| {
                            let sentence = match e {
                                minix_regex::RegexError::TooComplex => "out of memory",
                                minix_regex::RegexError::InvalidPattern => {
                                    "invalid regular expression"
                                }
                            };
                            // C's errx quotes the user's pattern, not the
                            // option name (`function.c:1590`).
                            FindError::Regex(arg, sentence)
                        })?;
                        return Ok((Some(Node::Leaf(Prim::Regex { icase, pattern })), produces));
                    }
                    return Ok((
                        Some(Node::Leaf(Prim::Keyword { name: other, arg })),
                        produces,
                    ));
                }
                Ok((Some(Node::Leaf(Prim::Bare(other))), produces))
            }
        }
    }

    /// Arguments of the `-exec` family: consume up to a lone `;`, or a `+`
    /// immediately after `{}` (`function.c:647-660`); C refuses `-ok … +`
    /// (`function.c:666-669`).
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
        // `option.c:145-151` uses `bsearch`, so the table order is a
        // precondition for lookup (the table's own comment says so too).
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
        // `-H -h` still carries COMFOLLOW (`main.c:112-115` clears only
        // PHYSICAL), while `-L -h` does not (`-L` cleared it already at
        // `main.c:96`). Collapsing into a three-value enum hides this.
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
        // The attached `-fVALUE` form gives the same value (getopt's two
        // value-taking forms).
        let hug = parsed(&["-f/tmp/list", "."]);
        assert_eq!(hug.extra_paths, vec!["/tmp/list"]);
    }

    #[test]
    fn unknown_letters_before_the_path_list_are_swallowed_by_getopt() {
        // C's real behavior: in `find -name foo .` the `-name` lands in getopt's
        // option section; none of its four letters is in `"HLPdEf:hsXx"`, and
        // `main.c:128-129`'s default does nothing, so `-name` is swallowed
        // whole, `foo` and `.` both become start paths, and the expression is
        // empty.
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
        // A standalone `!` or `(` also starts the expression (the POSIX
        // boundary test, `main.c:138-143`).
        assert_eq!(
            parsed(&[".", "!", "-print"]).expr,
            vec![Node::Not(Box::new(bare("-print")))]
        );
        let paren = parsed(&[".", "(", "-print", ")"]);
        assert_eq!(paren.expr, vec![Node::Paren(vec![bare("-print")])]);
        // A token merely containing `(` with something else attached is not a
        // boundary and remains a path.
        assert_eq!(parsed(&[".((", "-print"]).paths, vec![".(("]);
    }

    #[test]
    fn no_starting_path_at_all_is_usage() {
        assert_eq!(parse(&[]), Err(FindError::Usage));
        assert_eq!(parse(&["-print"]), Err(FindError::Usage));
        // What `-f` brings in also counts as a start path (`main.c:109-111`
        // pushes it into the same array first).
        assert_eq!(
            parse(&["-f", "/etc", "-print"]).unwrap().paths,
            Vec::<&str>::new()
        );
    }

    #[test]
    fn the_implicit_print_wrap_follows_the_code_not_the_comment() {
        // The comment at `find.c:104-107` lists only five items, but nine
        // places actually set `isoutput`.
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
        // `-ok` shares the constructor with `-exec`, so it counts too.
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
        // `!` applies to the whole group (unlike the previous case, where a
        // single primary takes one item).
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
        // `c_not` and `c_or` each own two sentences in C, and the four branches
        // must not be folded into two: each branch pins its own error below.
        // (The old implementation could only report `NotWithoutOperand` or
        // nothing, letting three malformed shapes reach evaluation.)
        assert_eq!(
            parse(&[".", "!", "-o", "-print"]),
            Err(FindError::NotBeforeOr)
        );
        assert_eq!(parse(&[".", "-o", "-print"]), Err(FindError::OrWithoutLeft));
        assert_eq!(
            parse(&[".", "-print", "-o"]),
            Err(FindError::OrWithoutRight)
        );
        // C checks left before right: with a lone `-o` the left guard fires
        // first, and it must not be reported as the "after" sentence.
        assert_eq!(parse(&[".", "-o"]), Err(FindError::OrWithoutLeft));
        // After consecutive `!` are eliminated and counted, hitting `-o` still
        // reports C's "nothing in between" sentence.
        assert_eq!(
            parse(&[".", "!", "!", "-or", "-print"]),
            Err(FindError::NotBeforeOr)
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

        // A `+` not after `{}` is not a terminator (C's `lastbrace` test,
        // `function.c:647-660`), so it is collected as an ordinary argument and
        // the missing terminator is reported afterwards.
        assert_eq!(
            parse(&[".", "-exec", "grep", "+", "-print"]),
            Err(FindError::NoTerminator("-exec"))
        );
        // C explicitly refuses `-ok … +` (`function.c:666-669`).
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
// Second package: predicate evaluation. Ground truth: the `f_*` family in
// `minix3/usr.bin/find/function.c`. Directory entries are prepared by the
// table-walk leg and handed in (`Entry`), and this layer touches no system
// calls; the three things that need the file system (`-newer`'s reference,
// `-empty`'s directory listing, `-xdev`'s starting device) are inputs too.
// ─────────────────────────────────────────────────────────────────────────

/// `-size`'s default unit: 512-byte blocks (`FIND_SIZE` in
/// `minix3/usr.bin/find/find.h`).
pub const FIND_SIZE: i64 = 512;

/// Seconds in a minute (`SECSPERMIN` in `function.c`).
pub const SECS_PER_MINUTE: i64 = 60;
/// Seconds in a day (`SECSPERDAY`).
pub const SECS_PER_DAY: i64 = 86_400;

/// A view of one directory entry. The fields correspond to
/// `FTSENT::fts_statp` plus `fts_level`, `fts_name`, and `fts_path`
/// (`minix3/include/fts.h`).
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    /// Full path so far (the target of `-path` and `-newer`).
    pub path: &'a str,
    /// Entry name (the target of `-name`).
    pub name: &'a str,
    /// Search depth. `fts_level` starts at 1 (the start path itself is 1).
    pub level: usize,
    /// `st_mode`.
    pub mode: u32,
    /// `st_ino`.
    pub ino: u64,
    /// `st_nlink`.
    pub nlink: u32,
    /// `st_size`.
    pub size: i64,
    /// `st_uid`.
    pub uid: u32,
    /// `st_gid`.
    pub gid: u32,
    /// `st_dev` (used by the `-xdev` comparison).
    pub dev: u64,
    /// `st_atim.tv_sec`.
    pub atime: i64,
    /// `st_mtim.tv_sec`.
    pub mtime: i64,
    /// `st_ctim.tv_sec`.
    pub ctime: i64,
    /// The entry-name list for this directory, used by `-empty` to test for an
    /// empty directory. When unavailable (no table walk or a failed read) it is
    /// `None`, and this layer takes C's `opendir`-failure branch and answers
    /// false.
    pub children: Option<&'a [&'a str]>,
}

/// World state needed for evaluation.
#[derive(Debug, Clone, Copy)]
pub struct World {
    /// The moment `find` started (C takes it once with `time(&now)` at
    /// `main.c:214`).
    pub now: i64,
    /// Effective uid: the `-user`/`-nouser` branches lack a password database,
    /// so this is reserved for uses other than `-newer`.
    pub euid: u32,
    /// Effective gid.
    pub egid: u32,
    /// Device of the start path, used by the `-xdev` comparison (C goes through
    /// `FTS_XDEV`, which is the traversal leg's business).
    pub root_dev: u64,
}

/// Comparison mode. C's three states `F_EQUAL`/`F_LESSTHAN`/`F_GREATER` (the
/// `COMPARE` enum at `function.c:68-79`), plus `F_ATLEAST` for `-perm -mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    Equal,
    LessThan,
    Greater,
    /// `-perm -mode`: all the given bits must be set.
    AtLeast,
}

/// An integer argument with its comparison mode (`find_parsenum`,
/// `function.c:135-170`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumArg {
    pub compare: Compare,
    pub value: i64,
}

/// Numeric parse failure. C's two `errx` branches: illegal numeric value and
/// illegal trailing character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumError {
    /// `"%s: %s: illegal numeric value"`.
    IllegalValue(String),
    /// `"%s: %s: illegal trailing character"`. A non-empty trailing character
    /// means something else was attached.
    IllegalTrailing(String, char),
}

/// `find_parsenum`: an optional `+`/`-` selects the comparison mode, and the
/// rest must be a decimal integer. `suffix` is the only permitted trailing
/// character (only `-size` passes `'c'`).
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
        // C uses `strtoq`: with no digits at all the value is 0 and
        // `endchar == str`, which is illegal.
        Ok(value) => Ok(NumArg { compare, value }),
        // C's test is `value == 0 && endchar == str`: only when no digit at all
        // was consumed is it an illegal numeric value.
        Err(_) if digits.is_empty() || !digits.starts_with(|c: char| c.is_ascii_digit()) => {
            Err(NumError::IllegalValue(String::from(option)))
        }
        Err(_) => {
            // Trailing character: only `-size` permits a single `c` (`c_size`
            // passes `endch` as 'c').
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

/// `TIME_CORRECT` (`function.c:178-180`): a `-n` on a time predicate needs one
/// more bucket, or a file "less than a minute old" would fall outside "less
/// than zero minutes".
pub fn time_correct(compare: Compare, value: i64) -> i64 {
    if compare == Compare::LessThan {
        value + 1
    } else {
        value
    }
}

/// C `atoi`'s shape (used by `c_group`'s numeric fallback): skip blanks,
/// take an optional sign and the leading digits; no digits is 0. Overflow
/// is undefined in C; this build saturates.
fn atoi_id(text: &str) -> u32 {
    let t = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut v: i64 = 0;
    for b in digits.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        v = v.saturating_mul(10).saturating_add((b - b'0') as i64);
    }
    let signed = if neg { -v } else { v };
    signed as u32
}

/// `c_user`'s "no such user" guard (`function.c:1748-1749`): the name is
/// neither an account nor a numeric spelling (`atoi == 0`, not `0`-led,
/// not the `+0`/`-0` exceptions).
fn user_needs_account(name: &str) -> bool {
    atoi_id(name) == 0 && !name.starts_with('0') && name != "+0" && name != "-0"
}

/// `c_group`'s guard (`function.c:1081-1083`): no `+0`/`-0` exceptions here.
fn group_needs_entry(name: &str) -> bool {
    atoi_id(name) == 0 && !name.starts_with('0')
}

/// One `-exec … {} +` batch — C's `ep_bxp` accumulator living on the plan
/// node (`function.c:672-707`): the user's fixed words (everything before
/// the trailing `{}`, which the terminator's shape guarantees) plus the
/// paths gathered so far. C also budgets the bytes (`ARG_MAX - 4K`,
/// `MAXARG`); this build's exec frames are heap strings with no fixed
/// budget, so only the 5000-argument cap is carried.
struct ExecBatch {
    fixed: Vec<String>,
    paths: Vec<String>,
    /// The last non-zero child exit seen (C's `ep_rval`, `function.c:610-615`).
    rval: Option<i32>,
}

/// Per-node batch state for one walk. The key is the node's address: the
/// formed expression outlives the walk, so each `Prim::Exec` node keeps a
/// stable identity across entries.
#[derive(Default)]
pub struct Batches {
    slots: Vec<(usize, ExecBatch)>,
}

impl Batches {
    /// The batch for one node, created on first touch.
    fn slot(&mut self, key: usize, argv: &[&str]) -> &mut ExecBatch {
        if self.slots.iter().any(|(k, _)| *k == key) {
            let index = self.slots.iter().position(|(k, _)| *k == key).unwrap();
            return &mut self.slots[index].1;
        }
        // C's `cnt = ap - *argvp - 1` (`function.c:676`): the trailing `{}`
        // is dropped and everything before it stays verbatim — mid-args
        // keep a literal `{}` in the plus form (no substitution here).
        let fixed: Vec<String> = if argv.last() == Some(&"{}") {
            argv[..argv.len() - 1]
                .iter()
                .map(|w| String::from(*w))
                .collect()
        } else {
            argv.iter().map(|w| String::from(*w)).collect()
        };
        let batch = ExecBatch {
            fixed,
            paths: Vec::new(),
            rval: None,
        };
        self.slots.push((key, batch));
        let index = self.slots.len() - 1;
        &mut self.slots[index].1
    }
}

impl ExecBatch {
    /// Run one batch the way `run_f_exec` does (`function.c:570-615`):
    /// `fixed ++ paths` through the run seam, then clear. `Err` is the
    /// vfork family failing (C `err(1)`); a non-zero exit is remembered
    /// for find's own exit.
    fn run(&mut self, refs: &dyn ReferenceStat) -> Result<(), (Errno, String)> {
        if self.paths.is_empty() {
            return Ok(());
        }
        let mut cmd: Vec<&str> = self.fixed.iter().map(String::as_str).collect();
        cmd.extend(self.paths.iter().map(String::as_str));
        let outcome = refs.run_utility(&cmd);
        self.paths.clear();
        match outcome {
            Ok(code) => {
                if code != 0 {
                    self.rval = Some(code);
                }
                Ok(())
            }
            Err(e) => Err((e, String::from("vfork"))),
        }
    }
}

/// Three-state evaluation of `COMPARE(a, b)`.
fn compare_with(compare: Compare, a: i64, b: i64) -> bool {
    match compare {
        Compare::Equal => a == b,
        Compare::LessThan => a < b,
        Compare::Greater => a > b,
        Compare::AtLeast => true, // only -perm uses it, handled on its own path
    }
}

/// Raise the delta by `unit - 1` before dividing (C's
/// `(now - t + UNIT - 1) / UNIT`), i.e. "a partial bucket counts as a full
/// one". Rust's `/` truncates toward zero exactly like C's.
fn in_units(delta: i64, unit: i64) -> i64 {
    (delta + unit - 1) / unit
}

/// The failure surface of predicate evaluation. Two kinds: arguments C rejects
/// with `errx` while building the plan, and actions this tree lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeError {
    /// An invalid numeric argument (`find_parsenum`'s two `errx` branches).
    BadNumber(NumError),
    /// The `-type` letter is not in the table (`c_type`'s default branch).
    UnknownType(String),
    /// The `-perm` mode string cannot be parsed (`c_perm`'s `setmode` returns
    /// empty).
    BadPerm(String),
    /// This tree does not have that leg yet:
    /// `-user`/`-group`/`-nouser`/`-nogroup` need a password database (the same
    /// prerequisite as `chown`), `-delete` needs to write the root disk,
    /// `-fstype` needs a device-name mapping, `-flags` needs Minix's flag
    /// character table, and `-regex`/`-iregex` need the `minix-regex` wiring.
    /// Every one is refused by name rather than silently answering false.
    Unwired(String),
    /// The child could not be created (C's `err(1, "vfork")`/`err(1, "fork")`,
    /// `function.c:551-553`/`:770-772`): C kills the whole find; this layer
    /// records rval and skips the entry (the thin shell's declared "a hard
    /// error only marks that operand" rule). Carries the primary's name and
    /// the spawn errno.
    Spawn(String, Errno),
}

/// The two-part state of `-perm`: the mode bits and whether it is the `-mode`
/// form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermTest {
    pub compare: Compare,
    pub bits: u32,
}

/// Parse the `-perm` argument (`c_perm`, `function.c:1444-1470`). A leading `-`
/// selects `F_ATLEAST`; the rest goes to `mode::apply_mode` with base 0,
/// equivalent to C's `getmode(set, 0)`.
pub fn parse_perm(text: &str) -> Result<PermTest, JudgeError> {
    let (compare, rest) = match text.strip_prefix('-') {
        Some(tail) => (Compare::AtLeast, tail),
        None => (Compare::Equal, text),
    };
    let bits = crate::mode::apply_mode(0, rest)
        .map_err(|_| JudgeError::BadPerm(String::from(text)))? as u32;
    Ok(PermTest { compare, bits })
}

/// `-perm` evaluation. C looks only at those 12 bits (setuid/setgid/sticky plus
/// the nine permission bits); the two archive bits lie outside `S_IFMT` and do
/// not participate (`f_perm`'s mask, `function.c:1430-1441`).
const PERM_MASK: u32 = 0o7777;

pub fn judge_perm(entry: &Entry, test: &PermTest) -> bool {
    let mode = entry.mode & PERM_MASK;
    if test.compare == Compare::AtLeast {
        (test.bits | mode) == mode
    } else {
        mode == test.bits
    }
}

/// Mapping from `-type` letters to `S_IF*` (`c_type`, `function.c:1668-1723`).
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
        // Minix defines `S_IFWHT` (`sys/stat.h:147`), and C provides both `w`
        // and `W` under `#ifdef`.
        "w" | "W" => crate::mode::S_IFWHT,
        other => return Err(JudgeError::UnknownType(String::from(other))),
    })
}

/// Whether an evaluated predicate asks to "skip this directory" or "prune".
/// `-prune` is always true and sets the flag, like C's `f_prune`
/// (`function.c:1542-1560`: set `fts_function = FTS_SKIP`, then return 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Do nothing; only the truth value matters.
    None,
    /// Do not descend into this branch.
    Prune,
    /// Emit this entry's path (the `-print` family).
    Print(PrintForm),
    /// Carry a `warn(3)`-shaped line (`-delete`'s failed `rmdir`/`unlink`).
    Warn(String),
}

/// The three print forms (`f_print`/`f_print0`/`f_printx`,
/// `function.c:1475-1500`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintForm {
    /// Path plus newline.
    Line,
    /// Path plus NUL.
    Nul,
    /// The xargs-safe escaped form: a backslash before spaces and control
    /// characters.
    Xargs,
}

/// The result of evaluating one leaf: the truth value plus any action this
/// layer must record along the way.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// The three `-newer` siblings ask for the reference file's times. The
/// table-walk leg provides this query, keeping this layer free of system
/// calls.
pub trait ReferenceStat {
    /// The reference path's `st_atim`/`st_mtim`/`st_ctim`, or `None` when
    /// unavailable (C does `err(1)` and exits inside `c_newer`, so the thin
    /// shell must treat a failure as a plan-building error first).
    fn times_of(&self, path: &str) -> Option<(i64, i64, i64)>;

    /// The `-exec` run leg (C `f_exec`'s vfork + `execvp` + `waitpid`,
    /// `function.c:550-565`): spawn `argv` in find's own working directory.
    /// `Ok(code)` is the child's wait answer in C's `rval` shape —
    /// `WEXITSTATUS` when it exited, `-1` when it died by signal
    /// (`function.c:598-601`); the child's own exec failure surfaces as
    /// `Ok(1)` (C `warn` + `_exit(1)`, `function.c:559-561`). `Err` is the
    /// vfork/waitpid family failing, where C calls `err(1)` and exits. The
    /// default `ENOSYS` face is "this build's seam is absent", which keeps
    /// fakes that never exercise `-exec` compiling untouched.
    fn run_utility(&self, argv: &[&str]) -> Result<i32, Errno> {
        let _ = argv;
        Err(Errno::ENOSYS)
    }

    /// The `-ok` prompt (C `queryuser`, `misc.c:99-117`): print the command
    /// to standard error, read the first answer byte, drain to end of line,
    /// and proceed only on `y`. The default refuses to run — the safe half
    /// of asking and getting no answer.
    fn query_ok(&self, argv: &[&str]) -> bool {
        let _ = argv;
        false
    }

    /// `getpwnam`'s scan face (`c_user`, `function.c:1752-1754`): the uid
    /// for a login name, or `None` (the same face as an unreadable or
    /// absent database). The default is the seam-absent face.
    fn account_by_name(&self, name: &str) -> Option<u32> {
        let _ = name;
        None
    }

    /// `getpwuid`'s existence face (`f_nouser`'s `user_from_uid(uid, 1)`,
    /// `pwcache.c`): whether any account carries this uid. The default
    /// "nothing known" makes `-nouser` answer true, the safe half.
    fn uid_known(&self, uid: u32) -> bool {
        let _ = uid;
        false
    }

    /// `getgrnam`'s scan face (`c_group`, `function.c:1069-1083`): the gid
    /// for a group name, or `None`.
    fn group_by_name(&self, name: &str) -> Option<u32> {
        let _ = name;
        None
    }

    /// `getgrgid`'s existence face (`f_nogroup`'s `group_from_gid(gid, 1)`):
    /// whether any group carries this gid.
    fn gid_known(&self, gid: u32) -> bool {
        let _ = gid;
        false
    }

    /// `-delete`'s file leg (`f_delete`'s `unlink`, `function.c:555-558`).
    /// The default `ENOSYS` face is the seam-absent build.
    fn unlink_entry(&self, path: &str) -> Result<(), Errno> {
        let _ = path;
        Err(Errno::ENOSYS)
    }

    /// `-delete`'s directory leg (C `rmdir`, `function.c:551-554`).
    fn rmdir_entry(&self, path: &str) -> Result<(), Errno> {
        let _ = path;
        Err(Errno::ENOSYS)
    }
}

/// Evaluate one leaf predicate. `spec` comes from the first package's parse
/// result, with arguments still carrying their text.
pub fn judge_leaf<'a>(
    world: &World,
    refs: &dyn ReferenceStat,
    entry: &Entry<'a>,
    name: &str,
    arg: Option<&'a str>,
    // The negation of C's global flag (`divsize`): if `-size Nc` appears
    // anywhere in the tree, compare by bytes. It is an input so that "one `c`
    // affects the whole tree" is visible at the call site too.
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
        // C goes through `fnmatch` with `FNM_CASEFOLD`; Minix has no `-ipath`
        // branch, so none is invented here.
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
            // C's `divsize` starts at 1 (by block); any `-size Nc` sets it to 0
            // (by byte) and it is **never set back** (`c_size`,
            // `function.c:1638-1654`) — so once a `c` is written anywhere in a
            // tree, every later `-size` compares by byte. This layer computes
            // that flag as a property of the whole tree and passes it in.
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
            // Directory: C calls `readdir` entry by entry, and anything other
            // than "only `.` and `..`" counts as non-empty (`f_empty`,
            // `function.c:444-470`). An unavailable list takes C's
            // `opendir`-failure branch and answers false.
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
        // Note: Minix3's table has only `-false`, not bash's `-true`
        // (`option.c:60-110`), so there is no `-true` branch here — it falls
        // into the by-name refusal below.
        "-false" => Ok(Outcome::truthy(false)),
        // Missing legs are refused by name and never silently answer false.
        // (`-exec`/`-execdir`/`-ok` never arrive here as keywords -- the
        // parser turns them into [`Prim::Exec`].)
        "-fstype" | "-flags" | "-newerest" | "-mindepth0" => {
            Err(JudgeError::Unwired(String::from(name)))
        }
        // (`-regex`/`-iregex` compile at parse time into `Prim::Regex` and
        // never arrive here as keywords.)
        // `f_delete` (`function.c:518-561`): the start entry (`.`) is
        // ignored outright; directories go through `rmdir` (with
        // `ENOTEMPTY` silently tolerated) and everything else through
        // `unlink`; every failure is a `warn(3)` line and the primary
        // still "succeeds" — C's exit status does not move on delete
        // failures either.
        "-delete" | "-rm" => {
            if entry.path == "." || entry.path == ".." {
                return Ok(Outcome::truthy(true));
            }
            let outcome = if entry.mode & crate::cp::S_IFMT == crate::cp::S_IFDIR {
                refs.rmdir_entry(entry.path).and(Ok(()))
            } else {
                refs.unlink_entry(entry.path)
            };
            let warning = match outcome {
                Ok(()) => None,
                Err(e) if e.to_i32() == minix_sys::ENOTEMPTY => None,
                Err(e) => {
                    let leg = if entry.mode & crate::cp::S_IFMT == crate::cp::S_IFDIR {
                        "rmdir"
                    } else {
                        "unlink"
                    };
                    Some(format!("-delete: {leg}({}): {}", entry.path, Strerror(e)))
                }
            };
            Ok(match warning {
                Some(line) => Outcome {
                    truth: true,
                    action: Action::Warn(line),
                },
                None => Outcome::truthy(true),
            })
        }
        // `c_user` (`function.c:1734-1753`): the name resolves first; the
        // numeric fallback carries `find_parsenum`'s `[+-]` compare flags.
        // The unresolvable non-numeric shapes are fatal at plan-build in C
        // (`errx(1, "-user: %s: no such user")`); `walk_paths` screens them
        // up front, so an arg reaching here always resolves or parses.
        "-user" => {
            let name = text()?;
            if let Some(uid) = refs.account_by_name(name) {
                Ok(Outcome::truthy(entry.uid == uid))
            } else {
                let a = parse_num("-user", name, None).map_err(JudgeError::BadNumber)?;
                Ok(Outcome::truthy(compare_with(
                    a.compare,
                    entry.uid as i64,
                    a.value,
                )))
            }
        }
        // `c_group` (`function.c:1069-1083`): name first; the numeric
        // fallback is a bare `atoi` with no sign semantics.
        "-group" => {
            let name = text()?;
            let gid = refs.group_by_name(name).unwrap_or_else(|| atoi_id(name));
            Ok(Outcome::truthy(entry.gid == gid))
        }
        // `f_nouser`/`f_nogroup` (`function.c:1382-1394`/`:1362-1366`):
        // `user_from_uid(uid, 1)` returns a name only when an account
        // carries the id — inverted into the truth value.
        "-nouser" => Ok(Outcome::truthy(!refs.uid_known(entry.uid))),
        "-nogroup" => Ok(Outcome::truthy(!refs.gid_known(entry.gid))),
        other => Err(JudgeError::Unwired(String::from(other))),
    }
}

/// Whether `-size Nc` appears anywhere in the tree, i.e. whether C's `divsize`
/// has been set to "by byte". Once set it never falls back (`c_size`,
/// `function.c:1638-1654`), so the scan covers the whole expression rather than
/// the `-size` currently being evaluated.
pub fn size_counts_bytes(nodes: &[Node<'_>]) -> bool {
    nodes.iter().any(node_asks_bytes)
}

fn node_asks_bytes(node: &Node<'_>) -> bool {
    match node {
        Node::Leaf(Prim::Keyword { name, arg }) => *name == "-size" && arg.ends_with('c'),
        Node::Leaf(Prim::Bare(_))
        | Node::Leaf(Prim::Exec { .. })
        | Node::Leaf(Prim::Regex { .. }) => false,
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
        // Only -size can carry that trailing c (c_size passes endch as 'c').
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
        // A delta of 1.5 days: C's (delta + DAY - 1) / DAY makes it 2 days.
        e.atime = world().now - 86_400 - 43_200;
        assert!(judge(&e, "-atime", Some("2")).unwrap().truth);
        assert!(!judge(&e, "-atime", Some("1")).unwrap().truth);
        // A delta of 90 seconds: the minute bucket is 2.
        e.mtime = world().now - 90;
        assert!(judge(&e, "-mmin", Some("2")).unwrap().truth);
        // `-amin -1` becomes "less than one minute" through TIME_CORRECT, so 90
        // seconds does not count as less than one minute.
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
        // -path compares the whole path, not the name.
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
        // A whiteout entry accepts both letters (C provides w and W under
        // `#ifdef S_IFWHT`).
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
        let e = entry("/etc/rc", "rc", 0o100644); // size 2048 → 4 blocks
        assert!(judge(&e, "-size", Some("4")).unwrap().truth);
        assert!(!judge(&e, "-size", Some("2048")).unwrap().truth);
        // divide_size = true: any `-size Nc` in the same tree switches the
        // whole tree to byte comparison. With the global flag set, 2048 is
        // 2048, and a block count of 4 no longer holds.
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
        // A partial block counts as a whole one: 100 bytes is 1 block.
        let mut small = entry("/etc/tiny", "tiny", 0o100644);
        small.size = 100;
        assert!(judge(&small, "-size", Some("1")).unwrap().truth);
    }

    #[test]
    fn the_divsize_scan_looks_through_groups_and_operators() {
        // Assemble a tree directly: `( -size 1c -o -name x )`.
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
        // Put a `!` in front of `( -size 3 )`: the item in the group has no c,
        // so the whole tree does not divide.
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
        // `-perm -600` is **true** on 755: `f_perm` tests "all given bits are
        // set", and both 0600 bits are in 0755 (`function.c:1430-1441`). For
        // false, pick a bit that is not set.
        assert!(judge(&e, "-perm", Some("-600")).unwrap().truth);
        assert!(!judge(&e, "-perm", Some("-4000")).unwrap().truth);
        e.mode = 0o104755; // setuid also counts in those 12 bits
        // The mask is 07777, so the four setuid bits take part in exact
        // comparison.
        assert!(judge(&e, "-perm", Some("4755")).unwrap().truth);
        assert!(!judge(&e, "-perm", Some("0755")).unwrap().truth);
        // Symbolic form: `getmode` with base 0 sets exactly those bits, and
        // `-perm u=s` demands exact equality, hence false.
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
        // A list with just one `.` still counts as empty: C's `readdir` test
        // only counts entries whose name is not `.` or that leave content
        // besides `..` (`function.c:461-466`), and a lone `.` is skipped.
        d.children = Some(&dot_only);
        assert!(judge(&d, "-empty", None).unwrap().truth);
        d.children = None; // opendir failure → C decides 0
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
        // atime 11 > reference atime 10 → true; mtime 19 < reference 20 →
        // false; ctime 31 > 30 → true.
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
        // (`-exec`/`-execdir`/`-ok` have a production face now — their pins
        // live in the `Prim::Exec` tests; the `-user` family moved to the
        // account-seam tests and `-delete`/`-rm` to the write-seam tests.)
        for name in ["-fstype", "-flags"] {
            assert_eq!(
                judge(&e, name, Some("x")),
                Err(JudgeError::Unwired(String::from(name))),
                "{name} 该按名拒绝而不是悄悄判假"
            );
        }
        // C's table has no -true either (that is bash's), so it is refused
        // too.
        assert_eq!(
            judge(&e, "-true", None),
            Err(JudgeError::Unwired(String::from("-true")))
        );
    }
}

/// The product of one print action (`f_print`/`f_print0`/`f_printx`,
/// `function.c:1475-1507`). It returns text instead of writing standard output
/// so the whole evaluator family is host-testable; the thin shell does the
/// writing.
pub fn printed(form: PrintForm, path: &str) -> String {
    match form {
        PrintForm::Line => format!("{path}\n"),
        PrintForm::Nul => format!("{path}\0"),
        // `-printx` is for xargs: prefix each of these eight characters with a
        // backslash, then add a newline at the end of the line (`f_printx`,
        // `function.c:1492-1507`). Note that this is not the same set as `-X`'s
        // `BADCH` (`find.c:236-237`, only five characters); do not merge them
        // casually.
        PrintForm::Xargs => {
            let mut out = String::new();
            for ch in path.chars() {
                if matches!(ch, '\'' | '"' | ' ' | '$' | '`' | '\t' | '\n' | '\\') {
                    out.push('\\');
                }
                out.push(ch);
            }
            out.push('\n');
            out
        }
    }
}

/// The evaluation result of one expression segment on one directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluated {
    /// The truth value of the whole segment (C's evaluation loop: stop at the
    /// first node that returns false, and the segment is false).
    pub truth: bool,
    /// Whether to prune this branch (effective only when `-prune` hits and the
    /// segment is true; as in C, the fts side looks at the next node's
    /// direction).
    pub pruned: bool,
    /// Lines to emit (possibly several: a run of `-print`s each prints once).
    pub emitted: Vec<String>,
    /// `warn(3)`-shaped lines an action raised without failing (C's
    /// `-delete` warns on a failed `rmdir`/`unlink` and still "succeeds",
    /// `function.c:551-558`).
    pub warnings: Vec<String>,
}

/// Evaluate a sequence (`Vec<Node>`): the same shape as C's
/// `for (p = plan; p && (p->eval)(p, entry); p = p->next)` — evaluate until
/// some node returns false and short-circuit there; later nodes must not emit
/// anything either (`find.c:252`).
pub fn eval_plan(
    world: &World,
    refs: &dyn ReferenceStat,
    entry: &Entry<'_>,
    nodes: &[Node<'_>],
    size_bytes: bool,
    batches: &mut Batches,
) -> Result<Evaluated, JudgeError> {
    let mut out = Evaluated {
        truth: true,
        pruned: false,
        emitted: Vec::new(),
        warnings: Vec::new(),
    };
    eval_seq(world, refs, entry, nodes, size_bytes, batches, &mut out)
}

#[allow(clippy::too_many_arguments)]
fn eval_seq(
    world: &World,
    refs: &dyn ReferenceStat,
    entry: &Entry<'_>,
    nodes: &[Node<'_>],
    size_bytes: bool,
    batches: &mut Batches,
    out: &mut Evaluated,
) -> Result<Evaluated, JudgeError> {
    for node in nodes {
        let truth = eval_node(world, refs, entry, node, size_bytes, batches, out)?;
        if !truth {
            // Short-circuit: this segment is false, and no later node is
            // touched.
            out.truth = false;
            return Ok(out.clone());
        }
    }
    Ok(out.clone())
}

#[allow(clippy::too_many_arguments)]
fn eval_node(
    world: &World,
    refs: &dyn ReferenceStat,
    entry: &Entry<'_>,
    node: &Node<'_>,
    size_bytes: bool,
    batches: &mut Batches,
    out: &mut Evaluated,
) -> Result<bool, JudgeError> {
    match node {
        Node::Leaf(Prim::Bare(name)) => {
            let outcome = judge_leaf(world, refs, entry, name, None, size_bytes)?;
            let truth = outcome.truth;
            note(outcome, entry, out);
            Ok(truth)
        }
        Node::Leaf(Prim::Keyword { name, arg }) => {
            let outcome = judge_leaf(world, refs, entry, name, Some(arg), size_bytes)?;
            let truth = outcome.truth;
            note(outcome, entry, out);
            Ok(truth)
        }
        Node::Leaf(Prim::Regex { pattern, .. }) => {
            // `f_regex` (`function.c:1567`): one regexec over the whole
            // path; the anchoring happened at compile time.
            let outcome = Outcome::truthy(pattern.is_match(entry.path));
            let truth = outcome.truth;
            note(outcome, entry, out);
            Ok(truth)
        }
        Node::Leaf(Prim::Exec {
            name,
            is_ok,
            argv,
            plus,
        }) => {
            // `{} +` batches paths into sets that outlive one entry (C keeps
            // the accumulator on the plan node, `function.c:502-534`); the
            // stateless evaluator cannot hold it, so that face is refused by
            // name until it gets its own leg.
            if *plus {
                // C `f_exec`'s `F_PLUSSET` branch (`function.c:502-534`):
                // the path joins the node's batch, the batch runs when the
                // 5000-argument budget is hit, and the primary answers true
                // whatever the child did. A non-zero exit is remembered for
                // find's own exit (`ep_rval`).
                let key = node as *const _ as usize;
                let batch = batches.slot(key, argv);
                batch.paths.push(String::from(entry.path));
                if batch.paths.len() >= 5000 {
                    batch
                        .run(refs)
                        .map_err(|(e, what)| JudgeError::Spawn(what, e))?;
                }
                return Ok(true);
            }
            // `f_exec`/`f_execdir`'s substitution: `-exec` gets the whole
            // path, `-execdir` the basename (`function.c:536-542`/`:749-754`).
            // `brace_subst` (`misc.c`) rebuilds every `{}` occurrence and
            // leaves other args alone -- `str::replace` has the same shape.
            let source = if *name == "-execdir" {
                entry.name
            } else {
                entry.path
            };
            let built: Vec<String> = argv.iter().map(|word| word.replace("{}", source)).collect();
            let cmd: Vec<&str> = built.iter().map(|s| s.as_str()).collect();
            // `-ok` asks first (`function.c:543-544`): a "no" skips the run
            // and the primary answers false.
            if *is_ok && !refs.query_ok(&cmd) {
                return Ok(false);
            }
            // `function.c:563-564`: true iff the child exited zero.
            match refs.run_utility(&cmd) {
                Ok(code) => Ok(code == 0),
                Err(e) => Err(JudgeError::Spawn(String::from(*name), e)),
            }
        }
        Node::Paren(inner) => {
            let mut sub = Evaluated {
                truth: true,
                pruned: false,
                emitted: Vec::new(),
                warnings: Vec::new(),
            };
            eval_seq(world, refs, entry, inner, size_bytes, batches, &mut sub)?;
            let truth = sub.truth;
            merge(sub, out);
            Ok(truth)
        }
        Node::Not(inner) => {
            let mut sub = Evaluated {
                truth: true,
                pruned: false,
                emitted: Vec::new(),
                warnings: Vec::new(),
            };
            let one = vec![(**inner).clone()];
            eval_seq(world, refs, entry, &one, size_bytes, batches, &mut sub)?;
            // C's `f_not` flips only the sub-plan's result; output produced
            // inside the sub-plan is still emitted.
            let truth = !sub.truth;
            merge(sub, out);
            Ok(truth)
        }
        Node::Or { left, right } => {
            let mut sub = Evaluated {
                truth: true,
                pruned: false,
                emitted: Vec::new(),
                warnings: Vec::new(),
            };
            eval_seq(world, refs, entry, left, size_bytes, batches, &mut sub)?;
            let left_truth = sub.truth;
            merge(sub, out);
            if left_truth {
                return Ok(true);
            }
            let mut right_sub = Evaluated {
                truth: true,
                pruned: false,
                emitted: Vec::new(),
                warnings: Vec::new(),
            };
            eval_seq(
                world,
                refs,
                entry,
                right,
                size_bytes,
                batches,
                &mut right_sub,
            )?;
            let truth = right_sub.truth;
            merge(right_sub, out);
            Ok(truth)
        }
    }
}

fn note(outcome: Outcome, entry: &Entry<'_>, out: &mut Evaluated) {
    match outcome.action {
        Action::None => {}
        Action::Prune => out.pruned = true,
        Action::Print(form) => out.emitted.push(printed(form, entry.path)),
        Action::Warn(line) => out.warnings.push(line),
    }
}

fn merge(sub: Evaluated, out: &mut Evaluated) {
    out.pruned |= sub.pruned;
    out.emitted.extend(sub.emitted);
    out.warnings.extend(sub.warnings);
}

// ============================================================================
// Table-walk traversal leg (the deciding half's fourth package) — counterpart of
// `find_execute` (`minix3/usr.bin/find/find.c:201-273`)
// ============================================================================

/// The `FTSENT` payload bits for one path. The evaluation half looks only at
/// `mode`/`ino`/`nlink`/`size`/`uid`/`gid`/`dev` plus the three timestamps, so
/// only those are kept here (the `fts_statp` fields of
/// `minix3/include/fts.h`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatBits {
    pub mode: u32,
    pub ino: u64,
    pub nlink: u32,
    pub size: i64,
    pub uid: u32,
    pub gid: u32,
    pub dev: u64,
    pub atime: i64,
    pub mtime: i64,
    pub ctime: i64,
}

/// The two things the walk leg asks of the system layer (inheriting `-newer`'s
/// `ReferenceStat`). The traversal core obtains all its data through this, so a
/// synthetic tree can exercise every branch on the host; the thin shell
/// implements the same surface with `minix_sys`'s `lstat`/`getdents`/`stat`.
///
/// The division of labour between the two `stat` entries follows `fts_stat`
/// (`minix3/lib/libc/gen/fts.c:921-968`): a physical walk takes `lstat_of` on
/// the link itself; a logical walk (`-L`) or a command-line follow (`-H` on the
/// start path) takes `stat_of` first (following the link), and only when
/// `stat_of` fails falls back to `lstat_of` to classify the link (`FTS_SLNONE`).
///
/// All three entries return `Result<_, Errno>` rather than `Option`: C prints
/// `warnx("%s: %s", fts_path, strerror(fts_errno))` for all of
/// `FTS_NS`/`FTS_ERR`/`FTS_DNR` (`find.c:230-231`), and that **number is the
/// content of the message**; returning only a binary "failed" would force the
/// shell to invent a sentence (the old form: `<path>: stat failed`), replacing
/// ground truth with invention. The shape matches `PathProbe` in the sibling
/// `pathchk.rs` (whose three methods are already `Result<i64, Errno>`), so this
/// is not a new lane but the walk leg brought up to the same contract.
pub trait WalkProbe: ReferenceStat {
    /// The link's own `lstat` bits (do not follow symlinks). Failure = C's
    /// `FTS_NS`, returning the `lstat` errno.
    fn lstat_of(&self, path: &str) -> Result<StatBits, Errno>;
    /// The target path's `stat` bits (following symlinks). Only called on a
    /// logical/following walk; per `fts_stat`'s rule, when `lstat` fails too the
    /// number reported is **this call's (stat's) errno** (`fts.c:950-955`'s
    /// `saved_errno`).
    fn stat_of(&self, path: &str) -> Result<StatBits, Errno>;
    /// Entry names inside a directory (C's `readdir`, may include `.`/`..`).
    /// Cannot open = C's `FTS_DNR`, returning the `open`/`getdents` errno. In
    /// logical mode, for an item that is a symlink to a directory, `open`
    /// follows the link and reads the target directory, so this entry should
    /// return the target's children.
    fn dir_children(&self, path: &str) -> Result<Vec<String>, Errno>;
}

/// The classification `fts_stat` assigns to one path (the return-code family at
/// `fts.c:921-968`). The walk leg uses it to decide evaluation and descent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsKind {
    /// The target is a descendable directory (`FTS_D`).
    Dir,
    /// The target directory shares `(dev,ino)` with an ancestor, forming a
    /// cycle (`FTS_DC`) — evaluate but do not descend.
    Cycle,
    /// An ordinary non-directory item (`FTS_F`/`FTS_SL`/`FTS_DEFAULT`) —
    /// evaluate without descending.
    NotDir,
    /// In logical mode the symlink's target does not exist (`FTS_SLNONE`) —
    /// evaluate the link's own attributes without descending.
    BrokenLink,
    /// Both `stat` and `lstat` failed (`FTS_NS`) — warn, record an error, and
    /// skip. The payload is the number C stores in `fts_errno` and renders into
    /// a sentence with `strerror(3)`.
    Failed(Errno),
}

/// The output of one walk: lines to write (each with its newline/NUL), warning
/// text, and the exit-code contribution.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WalkResult {
    pub emitted: Vec<String>,
    pub warnings: Vec<String>,
    pub rval: u8,
}

/// Walk the start-path set, following the options extracted from `Parsed`, and
/// evaluate each entry. Physical mode: symlinks are not followed, and a
/// directory is descended into only when its own `dev` equals the start path's
/// (`-x`). `-d` makes directories evaluate in post-order, `-s` sorts each level
/// by name, `-X` rejects paths containing xargs separators, and the implicit
/// `-print` is added when the expression has no output entry.
/// Whether a `-delete`/`-rm` primary sits anywhere in the tree (C's
/// `c_delete` runs at plan build, so the forcing is unconditional there).
fn delete_present(nodes: &[Node<'_>]) -> bool {
    nodes.iter().any(|node| match node {
        // Arity::None primaries parse as `Bare` (the judge matches them by
        // name there too).
        Node::Leaf(Prim::Bare(name)) | Node::Leaf(Prim::Keyword { name, .. }) => {
            *name == "-delete" || *name == "-rm"
        }
        Node::Leaf(Prim::Exec { .. }) | Node::Leaf(Prim::Regex { .. }) => false,
        Node::Not(inner) => delete_present(core::slice::from_ref(inner)),
        Node::Paren(inner) => delete_present(inner),
        Node::Or { left, right } => delete_present(left) || delete_present(right),
    })
}

/// The `-user`/`-group` keyword args anywhere in the tree, in order — the
/// pre-pass resolves each the way C's plan constructors do at build time.
fn user_family_args<'n, 'a>(nodes: &'n [Node<'a>]) -> Vec<(&'n str, &'a str)> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Leaf(Prim::Keyword { name, arg }) if *name == "-user" || *name == "-group" => {
                out.push((*name, *arg))
            }
            Node::Leaf(_) => {}
            Node::Not(inner) => out.extend(user_family_args(core::slice::from_ref(inner))),
            Node::Paren(inner) => out.extend(user_family_args(inner)),
            Node::Or { left, right } => {
                out.extend(user_family_args(left));
                out.extend(user_family_args(right));
            }
        }
    }
    out
}

pub fn walk_paths(world: &World, probe: &dyn WalkProbe, parsed: &Parsed<'_>) -> WalkResult {
    // The implicit `-print` is formed per the tail of `find_formplan`
    // (`find.c:103-118`): an empty expression becomes just `-print`; otherwise
    // the expression is wrapped in `Paren` and a `-print` appended (the wrap
    // keeps `-o`'s short-circuit inside the parentheses so the implicit print
    // always runs).
    let effective: Vec<Node<'_>> = if parsed.needs_implicit_print {
        if parsed.expr.is_empty() {
            vec![Node::Leaf(Prim::Bare("-print"))]
        } else {
            let mut v = Vec::with_capacity(parsed.expr.len() + 1);
            v.push(Node::Paren(parsed.expr.clone()));
            v.push(Node::Leaf(Prim::Bare("-print")));
            v
        }
    } else {
        parsed.expr.clone()
    };
    let size_bytes = size_counts_bytes(&effective);
    // `c_delete` (`function.c:565-578`) forces the shape at plan build:
    // `-delete` implies `-depth`, disables `-follow`'s logical walk, and
    // turns output on (the option table's `produces_output` already
    // carries that half).
    let has_delete = delete_present(&effective);
    let depth = parsed.depth_first || has_delete;
    if has_delete {
        // `f_delete`'s paranoia check (`function.c:537-541`): any start
        // path whose accpath carries a `/` is
        // `-delete: <path>: relative path potentially not safe`, fatal
        // before anything is touched. (C checks every evaluated entry's
        // accpath; this walk never chdirs, so a child's pass string is the
        // root-derived path and only the start entry can trip the check.)
        let mut bad = parsed
            .extra_paths
            .iter()
            .chain(parsed.paths.iter())
            .filter(|p| p.contains('/'));
        if let Some(path) = bad.next() {
            return WalkResult {
                rval: 1,
                warnings: vec![format!(
                    "-delete: {path}: relative path potentially not safe"
                )],
                ..WalkResult::default()
            };
        }
    }
    // C's `c_user`/`c_group` resolve their names at plan-build time and
    // `errx(1)` before anything walks: an unresolvable non-numeric name is
    // `-user: <name>: no such user` / `-group: <name>: no such group`, and
    // a numeric spelling that fails `find_parsenum` is
    // `-user: <name>: illegal numeric value`. Screen the tree's user-family
    // args up front with the same sentences and skip the walk — the bin
    // prints the warning and exits with the rval.
    for (name, arg) in user_family_args(&effective) {
        let name: &str = name;
        let arg: &str = arg;
        let fatal = match name {
            "-user" if probe.account_by_name(arg).is_some() => None,
            "-user" if user_needs_account(arg) => Some("no such user"),
            "-user" => {
                if parse_num("-user", arg, None).is_err() {
                    Some("illegal numeric value")
                } else {
                    None
                }
            }
            "-group" if probe.group_by_name(arg).is_some() => None,
            "-group" if group_needs_entry(arg) => Some("no such group"),
            _ => continue,
        };
        if let Some(sentence) = fatal {
            return WalkResult {
                rval: 1,
                warnings: vec![format!("{name}: {arg}: {sentence}")],
                ..WalkResult::default()
            };
        }
    }
    let cfg = WalkCfg {
        depth,
        sorted: parsed.sorted,
        xdev: parsed.walk.has(fts::XDEV),
        xargs_safe_only: parsed.xargs_safe_only,
        // `-L` walks logically (everything is followed); `-H` follows only the
        // start path (`fts.c:171` passes `ISSET(FTS_COMFOLLOW)` for the root,
        // while `fts_build` passes follow=0 for children), so comfollow only
        // affects visit_start. `-delete` clears the logical half (C's
        // `c_delete` strips `FTS_LOGICAL`).
        logical: parsed.walk.has(fts::LOGICAL) && !has_delete,
        comfollow: parsed.walk.has(fts::COMFOLLOW),
    };
    let mut w = Walk {
        probe,
        world,
        cfg,
        root_dev: 0,
        ancestors: Vec::new(),
        batches: Batches::default(),
        out: WalkResult::default(),
    };
    let ctx = Ctx {
        nodes: &effective,
        size_bytes,
    };
    let mut paths: Vec<&str> = Vec::with_capacity(parsed.extra_paths.len() + parsed.paths.len());
    // Paths brought in by `-f` come before the positional operands in order of
    // appearance (`main.c:109-111` pushes them into the array first).
    paths.extend(parsed.extra_paths.iter().copied());
    paths.extend(parsed.paths.iter().copied());
    for p in &paths {
        w.root_dev = 0;
        w.visit_start(p, ctx);
    }
    // C `find_traverse(plan, plan_cleanup, NULL)` (`find.c:269-271`): any
    // batch with leftover paths runs here, and a non-zero child exit (from
    // the leftovers or a mid-walk full batch) becomes find's exit.
    for (_, batch) in w.batches.slots.iter_mut() {
        if let Err((e, what)) = batch.run(probe) {
            w.out.warnings.push(format!("{what}: {}", Strerror(e)));
            w.out.rval = 1;
        }
        if let Some(code) = batch.rval.filter(|code| *code != 0) {
            w.out.rval = u8::try_from(code).unwrap_or(1);
        }
    }
    w.out
}

/// The set of bits a walk needs (extracted from `Parsed` so the whole
/// expression tree's ownership does not go into the recursion).
#[derive(Debug, Clone, Copy)]
struct WalkCfg {
    depth: bool,
    sorted: bool,
    xdev: bool,
    xargs_safe_only: bool,
    logical: bool,
    comfollow: bool,
}

/// Inputs that stay constant throughout the recursion: the formed expression
/// (implicit `-print` included) and the `divsize` flag. They are bundled into
/// one `Copy` struct to keep `eval_entry`'s parameter count under clippy's limit
/// (≤7).
#[derive(Clone, Copy)]
struct Ctx<'n> {
    nodes: &'n [Node<'n>],
    size_bytes: bool,
}

struct Walk<'p> {
    probe: &'p dyn WalkProbe,
    world: &'p World,
    cfg: WalkCfg,
    /// Device of the start path, used by the `-x` comparison (set separately
    /// for each start path).
    root_dev: u64,
    /// `(dev,ino)` of the directory chain currently being descended, used by the
    /// logical mode's `FTS_DC` cycle check (`fts.c:955-961`). Physical mode does
    /// not follow links and a real directory inode is unique, so no cycle can
    /// form; this stack is read and written only under `logical`.
    ancestors: Vec<(u64, u64)>,
    /// The `-exec … {} +` accumulators, keyed by node address (C keeps them
    /// on the plan nodes; the walker outlives every entry's evaluation, so
    /// this is where the state lives).
    batches: Batches,
    out: WalkResult,
}

impl Walk<'_> {
    /// A start path: `fts_read` reports `fts_level == 1` for the root, with the
    /// name taken from the last segment (`.` is kept as-is). The start path's
    /// follow bit = `ISSET(FTS_LOGICAL) || ISSET(FTS_COMFOLLOW)` (`fts.c:171`).
    fn visit_start(&mut self, path: &str, ctx: Ctx<'_>) {
        let name = base_name(path);
        let follow = self.cfg.logical || self.cfg.comfollow;
        let (kind, st) = self.classify(path, follow);
        match kind {
            // C prints `warnx("%s: %s", path, strerror(number))` for `FTS_NS`,
            // so the suffix is the sentence itself, not a phrase describing the
            // failure kind.
            FtsKind::Failed(err) => {
                self.out.warnings.push(format!("{path}: {}", Strerror(err)));
                self.out.rval = 1;
            }
            _ => {
                self.root_dev = st.dev;
                self.dispatch(kind, st, path, name, 1, ctx);
            }
        }
    }

    /// Route a classified item as `find_execute`'s `switch (fts_info)` does
    /// (`find.c:216-235`): directories go to `visit_dir`; cycles, broken links,
    /// and non-directories all fall into the default branch, evaluated once
    /// without descending.
    fn dispatch(
        &mut self,
        kind: FtsKind,
        st: StatBits,
        path: &str,
        name: &str,
        level: usize,
        ctx: Ctx<'_>,
    ) {
        match kind {
            FtsKind::Dir => self.visit_dir(path, name, level, st, ctx),
            _ => {
                self.eval_entry(path, name, level, st, None, ctx);
            }
        }
    }

    /// Classify a path (`fts_stat`, `fts.c:921-968`). With `follow`, take
    /// `stat_of` first (following); on failure take `lstat_of`: success means a
    /// broken link (`FTS_SLNONE`), and failure too means `FTS_NS`. Without
    /// `follow`, take only `lstat_of`. If the target is a directory, compare
    /// against ancestor `(dev,ino)` only in logical mode to detect a cycle
    /// (`FTS_DC`) — physical mode does not follow links and a real directory
    /// inode is unique, so no cycle can form; gating on `logical` also avoids a
    /// false positive from synthetic trees with all-zero inodes.
    fn classify(&self, path: &str, follow: bool) -> (FtsKind, StatBits) {
        let bits = if follow {
            match self.probe.stat_of(path) {
                Ok(b) => b,
                Err(stat_err) => match self.probe.lstat_of(path) {
                    Ok(b) => return (FtsKind::BrokenLink, b),
                    // The link itself cannot be read either: C reports **the
                    // stat call's number** (`fts.c:950-955` stores
                    // `saved_errno` first and uses it as `fts_errno` only when
                    // `lstat` fails too), not the lstat number. When the two
                    // differ (typically link missing vs permission denied),
                    // taking the wrong one prints the wrong sentence.
                    Err(_) => return (FtsKind::Failed(stat_err), StatBits::default()),
                },
            }
        } else {
            match self.probe.lstat_of(path) {
                Ok(b) => b,
                Err(lstat_err) => return (FtsKind::Failed(lstat_err), StatBits::default()),
            }
        };
        if bits.mode & crate::cp::S_IFMT == crate::cp::S_IFDIR {
            if self.cfg.logical && self.ancestors.iter().any(|a| *a == (bits.dev, bits.ino)) {
                return (FtsKind::Cycle, bits);
            }
            (FtsKind::Dir, bits)
        } else {
            (FtsKind::NotDir, bits)
        }
    }

    /// A directory: `FTS_D` (pre-order) or `FTS_DP` (post-order), depending on
    /// `-d`. Unreadable children = `FTS_DNR`: C warns and `continue`s (no
    /// evaluation, no descent), and this layer does the same.
    fn visit_dir(&mut self, path: &str, name: &str, level: usize, st: StatBits, ctx: Ctx<'_>) {
        let children = match self.probe.dir_children(path) {
            Ok(c) => c,
            // `FTS_DNR` and `FTS_NS` share C's one `warnx` line: the prefix is
            // the path and the suffix is the sentence for the number.
            Err(err) => {
                self.out.warnings.push(format!("{path}: {}", Strerror(err)));
                self.out.rval = 1;
                return;
            }
        };
        // `-x`: a directory on a different device than the start path is
        // reported but not descended into (`main.c:122-124`'s `FTS_XDEV`).
        let can_recurse = !self.cfg.xdev || st.dev == self.root_dev;
        if self.cfg.depth {
            // Post-order: descend first (`FTS_D` is suppressed by
            // `find.c:218-220`'s `isdepth`), then evaluate the directory
            // itself.
            if can_recurse {
                self.enter(path, &children, level, st, ctx);
            }
            self.eval_entry(path, name, level, st, Some(&children), ctx);
        } else {
            // Pre-order: evaluate the directory itself first; do not descend if
            // `-prune` hits or `-x` keeps us out.
            let pruned = self.eval_entry(path, name, level, st, Some(&children), ctx);
            if !pruned && can_recurse {
                self.enter(path, &children, level, st, ctx);
            }
        }
    }

    /// Descend for real: in logical mode push this directory onto the ancestor
    /// stack before walking the children and pop it on return (for cycle
    /// detection); physical mode descends directly.
    fn enter(&mut self, path: &str, children: &[String], level: usize, st: StatBits, ctx: Ctx<'_>) {
        if self.cfg.logical {
            self.ancestors.push((st.dev, st.ino));
            self.descend(path, children, level, ctx);
            self.ancestors.pop();
        } else {
            self.descend(path, children, level, ctx);
        }
    }

    /// Descend into a directory's children. Skip `.`/`..`
    /// (`dents::is_descendable`); with `-s`, sort by name. Children are
    /// classified with the logical-mode `follow` (`fts_build` passes follow=0;
    /// the `FTS_LOGICAL` flag itself decides whether to follow). A failed
    /// classification = `FTS_NS` (warn, set rval, continue).
    fn descend(&mut self, dir: &str, children: &[String], level: usize, ctx: Ctx<'_>) {
        let mut names: Vec<&str> = children
            .iter()
            .map(|s| s.as_str())
            .filter(|n| crate::dents::is_descendable(n))
            .collect();
        if self.cfg.sorted {
            names.sort();
        }
        for name in names {
            let child = fts_join(dir, name);
            let (kind, st) = self.classify(&child, self.cfg.logical);
            match kind {
                FtsKind::Failed(err) => {
                    self.out
                        .warnings
                        .push(format!("{child}: {}", Strerror(err)));
                    self.out.rval = 1;
                    continue;
                }
                _ => self.dispatch(kind, st, &child, name, level + 1, ctx),
            }
        }
    }

    /// Evaluate the whole expression on one directory entry and report whether
    /// `-prune` pruned it. `-X` (`isxargs`) first rejects paths by `BADCH`
    /// (`find.c:235-243`); an off-table or missing leg during evaluation is
    /// reported by name, sets rval, and emits nothing.
    fn eval_entry(
        &mut self,
        path: &str,
        name: &str,
        level: usize,
        st: StatBits,
        children: Option<&Vec<String>>,
        ctx: Ctx<'_>,
    ) -> bool {
        if self.cfg.xargs_safe_only && path.chars().any(|c| BADCH.contains(c)) {
            self.out.warnings.push(format!("{path}: illegal path"));
            self.out.rval = 1;
            return false;
        }
        let kids: Vec<&str> = match children {
            Some(v) => v.iter().map(|s| s.as_str()).collect(),
            None => Vec::new(),
        };
        let entry = Entry {
            path,
            name,
            level,
            mode: st.mode,
            ino: st.ino,
            nlink: st.nlink,
            size: st.size,
            uid: st.uid,
            gid: st.gid,
            dev: st.dev,
            atime: st.atime,
            mtime: st.mtime,
            ctime: st.ctime,
            children: children.map(|_| kids.as_slice()),
        };
        match eval_plan(
            self.world,
            self.probe,
            &entry,
            ctx.nodes,
            ctx.size_bytes,
            &mut self.batches,
        ) {
            Ok(ev) => {
                self.out.emitted.extend(ev.emitted);
                self.out.warnings.extend(ev.warnings);
                ev.pruned && ev.truth
            }
            Err(_) => {
                // Evaluation-time error (off-table / missing leg / invalid
                // argument): C terminates with errx while building the plan or
                // evaluating, while this layer records rval and skips that path
                // instead of ending the whole run (the same rule as the du/find
                // packages' "a hard error only marks that operand"; the
                // divergence is recorded in the thin shell's head note).
                self.out.rval = 1;
                false
            }
        }
    }
}

/// `find.c:236`'s `BADCH` (for `-X`/`isxargs`): space, tab, newline, backslash,
/// single quote, double quote. Note that it is not the same set as `-printx`'s
/// escape table (`function.c:1492-1507`, eight characters); do not merge
/// them.
const BADCH: &str = " \t\n\\'\"";

/// `FTSENT::fts_name`: the last path component. `.` and paths ending in `/`
/// each take their last segment.
fn base_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        // The path is `/` itself (or several slashes): fts gives the root an
        // empty name.
        return "";
    }
    match trimmed.rsplit_once('/') {
        Some((_, tail)) => tail,
        None => trimmed,
    }
}

/// BSD `fts`'s path assembly rule (different from the `ls` dotdir rule in
/// `dents::join_path`): a child always carries the parent prefix, so a child of
/// the start `.` is `./foo` (exactly what `fts_read` puts in `fts_path`), and
/// no double slash appears under the root `/`. find prints this full path, so
/// the de-prefixing implementation used by `ls` cannot be reused.
fn fts_join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        let mut out = String::from(dir);
        out.push_str(name);
        out
    } else {
        let mut out = String::from(dir);
        out.push('/');
        out.push_str(name);
        out
    }
}

#[cfg(test)]
mod eval_tests {
    use super::*;

    struct NoRefs;
    impl ReferenceStat for NoRefs {
        fn times_of(&self, _path: &str) -> Option<(i64, i64, i64)> {
            None
        }
    }

    fn entry() -> Entry<'static> {
        Entry {
            path: "/etc/rc.d",
            name: "rc.d",
            level: 2,
            mode: 0o100644,
            ino: 7,
            nlink: 1,
            size: 512,
            uid: 0,
            gid: 0,
            dev: 1,
            atime: 0,
            mtime: 0,
            ctime: 0,
            children: None,
        }
    }

    fn world() -> World {
        World {
            now: 0,
            euid: 0,
            egid: 0,
            root_dev: 1,
        }
    }

    /// Feed "start path + expression" in `find`'s argv shape to the first
    /// package's parser, then evaluate it against a fixed directory entry —
    /// that is how the two packages chain together in tests.
    fn run(src: &[&str]) -> Result<Evaluated, JudgeError> {
        // The argv shape is "start path + expression": that is exactly what the
        // first package's parser consumes.
        let mut owned: Vec<String> = vec![String::from(".")];
        owned.extend(src.iter().map(|s| String::from(*s)));
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let parsed = parse(&refs).unwrap();
        let bytes = size_counts_bytes(&parsed.expr);
        eval_plan(
            &world(),
            &NoRefs,
            &entry(),
            &parsed.expr,
            bytes,
            &mut Batches::default(),
        )
    }

    #[test]
    fn a_sequence_short_circuits_at_the_first_false_test() {
        // `-name rc.d -print`: both are true, so one line is emitted.
        let got = run(&["-name", "rc.d", "-print"]).unwrap();
        assert!(got.truth);
        assert_eq!(got.emitted, vec![String::from("/etc/rc.d\n")]);
        // The first test is false: the later -print must not be touched at
        // all.
        let quiet = run(&["-name", "nope", "-print"]).unwrap();
        assert!(!quiet.truth);
        assert!(quiet.emitted.is_empty());
    }

    #[test]
    fn every_true_test_in_a_chain_prints_once_per_print() {
        let two = run(&["-type", "f", "-print", "-print0"]).unwrap();
        assert_eq!(
            two.emitted,
            vec![String::from("/etc/rc.d\n"), String::from("/etc/rc.d\0")]
        );
    }

    #[test]
    fn or_takes_the_right_branch_only_when_the_left_is_false() {
        // The left side is true: the right branch is not touched, so its
        // -print emits nothing.
        let left = run(&["-name", "rc.d", "-print", "-o", "-name", "x", "-print"]).unwrap();
        assert_eq!(left.emitted, vec![String::from("/etc/rc.d\n")]);
        // The left side is false: the right branch takes over.
        let right = run(&["-name", "nope", "-print", "-o", "-type", "f", "-print"]).unwrap();
        assert!(right.truth);
        assert_eq!(right.emitted, vec![String::from("/etc/rc.d\n")]);
    }

    #[test]
    fn not_flips_the_truth_but_keeps_the_output_it_produced() {
        // C's f_not flips only the sub-plan's result: the -print inside the
        // sub-plan still emits.
        let got = run(&["!", "(", "-name", "rc.d", "-print", ")"]).unwrap();
        assert!(!got.truth);
        assert_eq!(got.emitted, vec![String::from("/etc/rc.d\n")]);
        let flipped = run(&["!", "(", "-name", "nope", ")"]).unwrap();
        assert!(flipped.truth);
    }

    #[test]
    fn prune_is_reported_without_changing_the_truth() {
        let got = run(&["-type", "d", "-prune"]).unwrap();
        assert!(!got.truth && !got.pruned); // not a directory, so -prune is never reached
        let deep = run(&["-type", "f", "-prune", "-print"]).unwrap();
        assert!(deep.truth && deep.pruned);
        assert_eq!(deep.emitted, vec![String::from("/etc/rc.d\n")]);
    }

    #[test]
    fn the_xargs_print_form_escapes_exactly_the_chars_c_lists() {
        // f_printx escapes these eight: `' " space $ ` \t \n \\` (function.c:1495-1500).
        let raw = "/tmp/a b'c\"d$e`f\n\tg\\";
        let escaped = printed(PrintForm::Xargs, raw);
        assert_eq!(escaped, "/tmp/a\\ b\\'c\\\"d\\$e\\`f\\\n\\\tg\\\\\n");
        // -X's BADCH has only five characters; do not merge the two sets
        // (find.c:235).
        assert_eq!(printed(PrintForm::Line, "a b"), String::from("a b\n"));
    }

    #[test]
    fn one_c_suffix_switches_every_size_test_in_the_tree() {
        // `-size 1c -o -size 1`: that one `c` makes the whole tree compare by
        // byte, so neither test holds for a 512-byte entry (this globality is
        // exactly C's `divsize`, `function.c:1638-1654`).
        let refs: Vec<&str> = vec![".", "-size", "1c", "-o", "-size", "1"];
        let parsed = parse(&refs).unwrap();
        let nodes = parsed.expr.clone();
        assert!(size_counts_bytes(&nodes));
        let world = world();
        let e = entry(); // size 512
        assert!(
            !eval_plan(&world, &NoRefs, &e, &nodes, true, &mut Batches::default())
                .unwrap()
                .truth
        );
        // Replace the entry with `c` by a block-form one: the flag is not set,
        // so 1 block (512 bytes) holds.
        let plain: Vec<&str> = vec![".", "-size", "2c", "-o", "-size", "1"];
        let plain = parse(&plain).unwrap();
        assert!(size_counts_bytes(&plain.expr));
        assert!(
            !eval_plan(
                &world,
                &NoRefs,
                &e,
                &plain.expr,
                true,
                &mut Batches::default()
            )
            .unwrap()
            .truth
        );
        let no_c: Vec<&str> = vec![".", "-size", "1"];
        let no_c = parse(&no_c).unwrap();
        assert!(!size_counts_bytes(&no_c.expr));
        assert!(
            eval_plan(
                &world,
                &NoRefs,
                &e,
                &no_c.expr,
                false,
                &mut Batches::default()
            )
            .unwrap()
            .truth
        );
    }

    #[test]
    fn exec_leg_refuses_rather_than_reporting_true() {
        // With no run seam scripted (`NoRefs`' default is the ENOSYS face),
        // `-exec` cannot pretend to succeed: the whole segment is an error,
        // not "true but silent".
        let got = run(&["-exec", "true", ";"]).unwrap_err();
        assert!(matches!(got, JudgeError::Spawn(_, _)));
        // The `{} +` form answers true at eval time — accumulation only;
        // the batch itself runs at the walker's cleanup (walk-level pins
        // below).
        assert!(run(&["-exec", "echo", "{}", "+"]).unwrap().truth);
    }
}

// ============================================================================
// Host tests for the table-walk traversal leg: a synthetic tree pinning
// `find_execute`'s routing branch by branch
// ============================================================================
#[cfg(test)]
mod walk_tests {
    use super::*;
    use crate::cp::{S_IFDIR, S_IFLNK, S_IFREG};
    use alloc::collections::{BTreeMap, BTreeSet};
    use std::cell::RefCell;

    struct FakeFs {
        dirs: BTreeMap<String, Vec<String>>,
        /// `lstat` results (the link's own bits).
        lstats: BTreeMap<String, StatBits>,
        /// Symlink → target (one hop); `stat_of`/`dir_children` resolve
        /// through it.
        symlinks: BTreeMap<String, String>,
        /// Unopenable directories (C's `FTS_DNR`).
        unreadable: BTreeSet<String>,
        /// Paths `lstat` cannot read (C's `FTS_NS`).
        unstatable: BTreeSet<String>,
        /// Per-path "which call fails and with which number": three slots
        /// `[stat, lstat, dnr]`, where 0 means that entry does not fail.
        /// `unstatable`/`unreadable` can only give a blanket ENOENT/EACCES, but
        /// C's warn line reports the number itself, so the three entries must be
        /// settable separately.
        fails: BTreeMap<String, [i32; 3]>,
        /// Auto-incrementing ino allocator: gives every registered item a unique
        /// `(dev,ino)`, avoiding false positives in logical-mode cycle
        /// detection.
        next_ino: u64,
        /// Scripted `-exec` answers, keyed by `argv[0]`: `Ok(code)` is the
        /// child's exit status, `Err` is the spawn itself failing (C
        /// `err(1, "vfork")`). A name outside the table answers `ENOSYS` —
        /// the seam-absent face — so a test only scripts what it asserts.
        runs: BTreeMap<String, Result<i32, Errno>>,
        /// Scripted `-ok` answers: `argv[0]` → whether the user said `y`.
        /// A name outside the table means "no answer" (the default false).
        ok_answers: BTreeMap<String, bool>,
        /// Every `run_utility` call, in order: the argv the judge handed
        /// over. The substitution assertions read this.
        run_log: RefCell<Vec<Vec<String>>>,
        /// Scripted accounts (name → uid) and groups (name → gid) for the
        /// `-user` family's seams.
        users: BTreeMap<String, u32>,
        groups: BTreeMap<String, u32>,
        /// Scripted `-delete` failures, keyed by path.
        delete_fails: BTreeMap<String, Errno>,
        /// Every successful deletion, in order.
        deleted: RefCell<Vec<String>>,
    }

    /// The three slots of `fails`. Naming them separately is what makes it
    /// testable that "in logical mode, when both `stat` and `lstat` fail, the
    /// number reported is stat's" (`fts.c:950-955`).
    #[derive(Clone, Copy)]
    enum WhichCall {
        Stat = 0,
        Lstat = 1,
        Dnr = 2,
    }

    impl FakeFs {
        /// Whether a given entry fails for a path, and with which number
        /// (0 = does not fail).
        fn fail_code(&self, path: &str, which: WhichCall) -> Option<i32> {
            self.fails
                .get(path)
                .map(|codes| codes[which as usize])
                .filter(|code| *code != 0)
        }

        /// Make one entry fail for a path with the given number (the other two
        /// stay non-failing).
        fn fail(&mut self, path: &str, which: WhichCall, code: i32) {
            self.fails.entry(String::from(path)).or_insert([0, 0, 0])[which as usize] = code;
        }

        fn new() -> FakeFs {
            FakeFs {
                dirs: BTreeMap::new(),
                lstats: BTreeMap::new(),
                symlinks: BTreeMap::new(),
                unreadable: BTreeSet::new(),
                unstatable: BTreeSet::new(),
                fails: BTreeMap::new(),
                next_ino: 1,
                runs: BTreeMap::new(),
                ok_answers: BTreeMap::new(),
                run_log: RefCell::new(Vec::new()),
                users: BTreeMap::new(),
                groups: BTreeMap::new(),
                delete_fails: BTreeMap::new(),
                deleted: RefCell::new(Vec::new()),
            }
        }

        /// Script one delete failure.
        fn delete_fails(&mut self, path: &str, errno: Errno) {
            self.delete_fails.insert(String::from(path), errno);
        }

        /// Clear one delete script (the table persists across runs).
        fn delete_ok(&mut self, path: &str) {
            self.delete_fails.remove(path);
        }

        /// Script one account.
        fn with_user(&mut self, name: &str, uid: u32) {
            self.users.insert(String::from(name), uid);
        }

        /// Script one group.
        fn with_group(&mut self, name: &str, gid: u32) {
            self.groups.insert(String::from(name), gid);
        }

        /// Give one registered path a different owner uid (the default
        /// fixtures are all uid 0).
        fn set_uid(&mut self, path: &str, uid: u32) {
            if let Some(bits) = self.lstats.get_mut(path) {
                bits.uid = uid;
            }
        }

        /// Script one utility: `argv[0]` exits `code`.
        fn run_answers(&mut self, name: &str, code: i32) {
            self.runs.insert(String::from(name), Ok(code));
        }

        /// Script a spawn failure for `argv[0]`.
        fn run_fails(&mut self, name: &str, errno: Errno) {
            self.runs.insert(String::from(name), Err(errno));
        }

        /// Script the `-ok` answer for `argv[0]`.
        fn ok_answers(&mut self, name: &str, says_yes: bool) {
            self.ok_answers.insert(String::from(name), says_yes);
        }
        fn fresh_ino(&mut self) -> u64 {
            let n = self.next_ino;
            self.next_ino += 1;
            n
        }
        fn dir(&mut self, path: &str, dev: u64, children: &[&str]) -> &mut FakeFs {
            let ino = self.fresh_ino();
            self.lstats.insert(
                String::from(path),
                StatBits {
                    mode: S_IFDIR | 0o755,
                    ino,
                    dev,
                    nlink: 2,
                    ..Default::default()
                },
            );
            self.dirs.insert(
                String::from(path),
                children.iter().map(|s| String::from(*s)).collect(),
            );
            self
        }
        fn file(&mut self, path: &str, dev: u64) -> &mut FakeFs {
            let ino = self.fresh_ino();
            self.lstats.insert(
                String::from(path),
                StatBits {
                    mode: S_IFREG | 0o644,
                    ino,
                    dev,
                    size: 0,
                    ..Default::default()
                },
            );
            self
        }
        /// An ordinary symlink (no target; in physical mode it is simply not
        /// descended into).
        fn link(&mut self, path: &str, dev: u64) -> &mut FakeFs {
            let ino = self.fresh_ino();
            self.lstats.insert(
                String::from(path),
                StatBits {
                    mode: S_IFLNK | 0o777,
                    ino,
                    dev,
                    ..Default::default()
                },
            );
            self
        }
        /// Symlink `path` → `target` (for logical-mode resolution).
        fn symlink_to(&mut self, path: &str, target: &str, dev: u64) -> &mut FakeFs {
            self.link(path, dev);
            self.symlinks
                .insert(String::from(path), String::from(target));
            self
        }
    }

    impl ReferenceStat for FakeFs {
        fn times_of(&self, _path: &str) -> Option<(i64, i64, i64)> {
            None
        }
        fn run_utility(&self, argv: &[&str]) -> Result<i32, Errno> {
            self.run_log
                .borrow_mut()
                .push(argv.iter().map(|s| String::from(*s)).collect());
            match argv.first() {
                Some(name) => self.runs.get(*name).copied().unwrap_or(Err(Errno::ENOSYS)),
                None => Err(Errno::EINVAL),
            }
        }
        fn query_ok(&self, argv: &[&str]) -> bool {
            match argv.first() {
                Some(name) => self.ok_answers.get(*name).copied().unwrap_or(false),
                None => false,
            }
        }
        fn account_by_name(&self, name: &str) -> Option<u32> {
            self.users.get(name).copied()
        }
        fn uid_known(&self, uid: u32) -> bool {
            self.users.values().any(|&u| u == uid)
        }
        fn group_by_name(&self, name: &str) -> Option<u32> {
            self.groups.get(name).copied()
        }
        fn gid_known(&self, gid: u32) -> bool {
            self.groups.values().any(|&g| g == gid)
        }
        fn unlink_entry(&self, path: &str) -> Result<(), Errno> {
            if let Some(code) = self.delete_fails.get(path) {
                return Err(*code);
            }
            if !self.lstats.contains_key(path) {
                return Err(Errno::ENOENT);
            }
            self.deleted.borrow_mut().push(String::from(path));
            Ok(())
        }
        fn rmdir_entry(&self, path: &str) -> Result<(), Errno> {
            if let Some(code) = self.delete_fails.get(path) {
                return Err(*code);
            }
            if !self.dirs.contains_key(path) {
                return Err(Errno::from_i32(minix_sys::ENOTEMPTY));
            }
            self.deleted.borrow_mut().push(String::from(path));
            Ok(())
        }
    }

    impl WalkProbe for FakeFs {
        fn lstat_of(&self, path: &str) -> Result<StatBits, Errno> {
            if let Some(code) = self.fail_code(path, WhichCall::Lstat) {
                return Err(Errno::from_i32(code));
            }
            if self.unstatable.contains(path) {
                return Err(Errno::from_i32(minix_sys::ENOENT));
            }
            self.lstats
                .get(path)
                .copied()
                .ok_or_else(|| Errno::from_i32(minix_sys::ENOENT))
        }
        fn stat_of(&self, path: &str) -> Result<StatBits, Errno> {
            if let Some(code) = self.fail_code(path, WhichCall::Stat) {
                return Err(Errno::from_i32(code));
            }
            if self.unstatable.contains(path) {
                return Err(Errno::from_i32(minix_sys::ENOENT));
            }
            // One-hop follow: resolve the link to the target's `lstat` bits (a
            // real stat may hop further; the test tree does not rely on that).
            let target = match self.symlinks.get(path) {
                Some(t) => t.as_str(),
                None => path,
            };
            self.lstats
                .get(target)
                .copied()
                .ok_or_else(|| Errno::from_i32(minix_sys::ENOENT))
        }
        fn dir_children(&self, path: &str) -> Result<Vec<String>, Errno> {
            if let Some(code) = self.fail_code(path, WhichCall::Dnr) {
                return Err(Errno::from_i32(code));
            }
            if self.unreadable.contains(path) {
                return Err(Errno::from_i32(minix_sys::EACCES));
            }
            if let Some(c) = self.dirs.get(path) {
                return Ok(c.clone());
            }
            // In logical mode open(link) follows through to the target
            // directory.
            match self.symlinks.get(path) {
                Some(target) => self
                    .dirs
                    .get(target)
                    .cloned()
                    .ok_or_else(|| Errno::from_i32(minix_sys::ENOENT)),
                None => Err(Errno::from_i32(minix_sys::ENOENT)),
            }
        }
    }

    /// `.` contains `a` (a regular file) and `sub` (a directory containing
    /// `b`), all on the same device.
    fn tree() -> FakeFs {
        let mut fs = FakeFs::new();
        fs.dir(".", 1, &["a", "sub"]);
        fs.file("./a", 1);
        fs.dir("./sub", 1, &["b"]);
        fs.file("./sub/b", 1);
        fs
    }

    fn run(args: &[&str], fs: &FakeFs) -> WalkResult {
        let owned: Vec<String> = args.iter().map(|s| String::from(*s)).collect();
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let parsed = parse(&refs).unwrap();
        let world = World {
            now: 0,
            euid: 0,
            egid: 0,
            root_dev: 1,
        };
        walk_paths(&world, fs, &parsed)
    }

    // ── the `-exec` family (`f_exec`/`f_execdir`/`queryuser`) ──

    #[test]
    fn exec_substitutes_every_brace_pair_with_the_path() {
        let mut fs = tree();
        fs.run_answers("echo", 0);
        let got = run(&[".", "-exec", "echo", "a{}b", "{}", ";"], &fs);
        // `-exec` is an output producer itself (C `c_exec` sets `isoutput`),
        // so no implicit `-print` is appended and nothing is emitted.
        assert_eq!(got.rval, 0);
        assert_eq!(got.emitted, Vec::<String>::new());
        // `brace_subst` rebuilds every `{}` occurrence, other args alone
        // (`misc.c`); one call per entry, whole path.
        let log = fs.run_log.borrow();
        assert_eq!(log.len(), 4);
        assert_eq!(log[0], ["echo", "a.b", "."]);
        assert_eq!(log[1], ["echo", "a./ab", "./a"]);
    }

    #[test]
    fn exec_answers_the_childs_exit_status() {
        let mut fs = tree();
        fs.run_answers("grep", 3);
        // A non-zero exit is false: the implicit print never runs and the
        // walk stays at rval 0 (a false primary is not an error).
        let got = run(&[".", "-name", "a", "-exec", "grep", "x", "{}", ";"], &fs);
        assert_eq!(got.emitted, Vec::<String>::new());
        assert_eq!(got.rval, 0);
        // A zero exit is true (the log is cumulative, so clear it first).
        fs.run_answers("grep", 0);
        fs.run_log.borrow_mut().clear();
        let got = run(&[".", "-name", "a", "-exec", "grep", "x", "{}", ";"], &fs);
        // `-exec` is its own output producer: no implicit print either way.
        assert_eq!(got.emitted, Vec::<String>::new());
        assert_eq!(
            fs.run_log.borrow().len(),
            1,
            "只有 -name 命中的那一项跑了一次"
        );
    }

    #[test]
    fn exec_spawn_failure_marks_the_operand_and_moves_on() {
        let mut fs = tree();
        fs.run_fails("grep", Errno::EIO);
        // C `err(1, "vfork")` kills the whole find; this layer records rval
        // and skips the entry (the thin shell's declared rule).
        let got = run(&[".", "-exec", "grep", "{}", ";", "-print"], &fs);
        assert_eq!(got.rval, 1);
        assert_eq!(got.emitted, Vec::<String>::new());
    }

    #[test]
    fn ok_asks_first_and_a_no_skips_the_run() {
        let mut fs = tree();
        fs.run_answers("rm", 0);
        fs.ok_answers("rm", false);
        let got = run(&[".", "-name", "a", "-ok", "rm", "{}", ";"], &fs);
        // A "no" is a false primary, not a spawn: nothing ran, nothing printed.
        assert!(fs.run_log.borrow().is_empty());
        assert_eq!(got.emitted, Vec::<String>::new());
        assert_eq!(got.rval, 0);
        // A `y` runs the utility (still no implicit print: `-ok` counts as
        // an output producer too).
        fs.ok_answers("rm", true);
        let got = run(&[".", "-name", "a", "-ok", "rm", "{}", ";"], &fs);
        assert_eq!(fs.run_log.borrow().len(), 1);
        assert_eq!(got.emitted, Vec::<String>::new());
    }

    #[test]
    fn execdir_substitutes_the_basename_only() {
        let mut fs = tree();
        fs.run_answers("basename", 0);
        let got = run(&["./sub", "-execdir", "basename", "{}", ";"], &fs);
        assert_eq!(got.rval, 0);
        let log = fs.run_log.borrow();
        // `f_execdir` (function.c:749-754): the name after the last `/`.
        assert_eq!(log[0], ["basename", "sub"]);
        assert_eq!(log[1], ["basename", "b"]);
    }

    // ── the `-user` family (`c_user`/`c_group`/`f_nouser`/`f_nogroup`) ──

    #[test]
    fn user_matches_by_account_name_and_by_numeric_spelling() {
        let mut fs = tree();
        fs.with_user("root", 0);
        fs.with_user("orphan", 7);
        fs.set_uid("./sub/b", 7);
        // Name hit: the file's uid compares equal (`function.c:1752-1754`).
        let got = run(&[".", "-user", "orphan"], &fs);
        assert_eq!(got.emitted, vec!["./sub/b\n"]);
        let got = run(&[".", "-user", "root"], &fs);
        assert_eq!(
            got.emitted,
            vec![".\n", "./a\n", "./sub\n"],
            "uid 0 的三项（sub/b 已改 7）"
        );
        // Numeric fallback with find_parsenum's sign flags.
        let got = run(&["./a", "-user", "+0"], &fs);
        assert_eq!(
            got.emitted,
            Vec::<String>::new(),
            "+0 是 greater-than，uid 0 不命中"
        );
        let got = run(&["./a", "-user", "0"], &fs);
        assert_eq!(got.emitted, vec!["./a\n"], "0 是 equal，命中");
        let got = run(&[".", "-user", "7"], &fs);
        assert_eq!(got.emitted, vec!["./sub/b\n"], "7 是 equal，uid 7 命中");
        let got = run(&[".", "-user", "+6"], &fs);
        assert_eq!(
            got.emitted,
            vec!["./sub/b\n"],
            "+6 是 greater-than，uid 7 命中"
        );
    }

    #[test]
    fn group_and_the_negative_family_follow_their_c_halves() {
        let mut fs = tree();
        fs.with_group("wheel", 0);
        fs.with_user("root", 0);
        // `-group` numeric fallback is a bare atoi (no sign semantics).
        let got = run(&["./a", "-group", "wheel"], &fs);
        assert_eq!(got.emitted, vec!["./a\n"]);
        let got = run(&["./a", "-group", "0"], &fs);
        assert_eq!(got.emitted, vec!["./a\n"]);
        // `-nouser`: true iff no account carries the file's uid.
        let got = run(&["./a", "-nouser"], &fs);
        assert_eq!(
            got.emitted,
            Vec::<String>::new(),
            "uid 0 有账户，nouser 为假"
        );
        fs.set_uid("./a", 7);
        let got = run(&["./a", "-nouser"], &fs);
        assert_eq!(got.emitted, vec!["./a\n"], "uid 7 无账户，nouser 为真");
        // `-nogroup` 同形（夹具 gid 全 0 且 wheel 已登记）。
        let got = run(&["./a", "-nogroup"], &fs);
        assert_eq!(got.emitted, Vec::<String>::new());
    }

    // ── the `-exec … {} +` batch (`f_exec`'s F_PLUSSET branch + cleanup) ──

    #[test]
    fn plus_form_accumulates_the_whole_set_into_one_run() {
        let mut fs = tree();
        fs.run_answers("echo", 0);
        let got = run(&[".", "-exec", "echo", "{}", "+"], &fs);
        // One run for the whole walk, paths in walk order; the trailing
        // `{}` is dropped and the primary stays true.
        assert_eq!(fs.run_log.borrow().len(), 1);
        assert_eq!(
            fs.run_log.borrow()[0],
            ["echo", ".", "./a", "./sub", "./sub/b"]
        );
        assert_eq!(got.rval, 0);
        assert_eq!(got.emitted, Vec::<String>::new());
    }

    #[test]
    fn plus_form_keeps_a_mid_arg_brace_literal_and_folds_the_exit() {
        let mut fs = tree();
        // `c_exec`'s plus branch copies the user's words verbatim
        // (`function.c:691`) — no brace substitution outside the trailing
        // `{}`.
        fs.run_answers("echo", 3);
        let got = run(&[".", "-exec", "echo", "a{}b", "{}", "+"], &fs);
        assert_eq!(
            fs.run_log.borrow()[0],
            ["echo", "a{}b", ".", "./a", "./sub", "./sub/b"]
        );
        // `run_f_exec` remembers the non-zero exit for find's own exit
        // (`ep_rval`, `function.c:610-615`).
        assert_eq!(got.rval, 3);
    }

    #[test]
    fn plus_form_respects_the_short_circuit_and_runs_leftovers_at_cleanup() {
        let mut fs = tree();
        fs.run_answers("echo", 0);
        // Only entries whose segment reaches the primary accumulate.
        let got = run(&[".", "-name", "a", "-exec", "echo", "{}", "+"], &fs);
        assert_eq!(fs.run_log.borrow()[0], ["echo", "./a"]);
        assert_eq!(got.rval, 0);
        // The spawn failing at cleanup is C's `err(1, "vfork")` — this
        // layer records rval and a `vwarn`-shaped warning.
        let mut fs = tree();
        fs.run_fails("echo", Errno::EIO);
        let got = run(&[".", "-exec", "echo", "{}", "+"], &fs);
        assert_eq!(got.rval, 1);
        assert_eq!(got.warnings.len(), 1);
        assert!(got.warnings[0].starts_with("vfork: "));
    }

    // ── `-delete`/`-rm` (`c_delete`/`f_delete`) ──

    #[test]
    fn delete_implies_depth_and_removes_children_before_their_directory() {
        let fs = tree();
        let got = run(&[".", "-delete"], &fs);
        // Post-order: the deepest entries go first, the root last — and the
        // root is `.` which `f_delete` ignores outright
        // (`function.c:521-524`).
        assert_eq!(fs.deleted.borrow().clone(), ["./a", "./sub/b", "./sub"]);
        assert_eq!(got.rval, 0);
        assert_eq!(got.warnings, Vec::<String>::new());
        assert_eq!(
            got.emitted,
            Vec::<String>::new(),
            "-delete 自身算输出生产者"
        );
    }

    #[test]
    fn delete_warns_on_failure_and_still_succeeds() {
        let mut fs = tree();
        // `function.c:555-558`: a failed `unlink` is a `warn(3)` line; the
        // primary answers true and C's exit status does not move.
        fs.delete_fails("./a", Errno::EPERM);
        let got = run(&[".", "-delete"], &fs);
        assert_eq!(got.rval, 0, "C 的 f_delete 不动退出码");
        assert_eq!(
            got.warnings,
            ["-delete: unlink(./a): Operation not permitted"]
        );
        // A directory failure uses the rmdir leg name; `ENOTEMPTY` is
        // silently tolerated (`function.c:551-554`).
        fs.delete_ok("./a");
        fs.delete_fails("./sub", Errno::from_i32(minix_sys::ENOTEMPTY));
        let got = run(&[".", "-delete"], &fs);
        assert_eq!(got.warnings, Vec::<String>::new(), "ENOTEMPTY 容忍");
        fs.delete_fails("./sub", Errno::EPERM);
        let got = run(&[".", "-delete"], &fs);
        assert_eq!(
            got.warnings,
            ["-delete: rmdir(./sub): Operation not permitted"]
        );
        assert_eq!(got.rval, 0);
    }

    #[test]
    fn delete_refuses_a_slash_carrying_start_path_before_the_walk() {
        // `function.c:537-541`: the paranoia check is fatal at the first
        // evaluated entry with a `/` in its accpath — the start entry, so
        // nothing is ever touched.
        let fs = tree();
        let got = run(&["./sub", "-delete"], &fs);
        assert_eq!(fs.deleted.borrow().len(), 0);
        assert_eq!(got.rval, 1);
        assert_eq!(
            got.warnings,
            ["-delete: ./sub: relative path potentially not safe"]
        );
        // A bare-name start (`.` included) passes; `-rm` is the same node.
        let got = run(&[".", "-rm"], &fs);
        assert_eq!(got.rval, 0);
        assert_eq!(fs.deleted.borrow().len(), 3);
    }

    // ── `-regex`/`-iregex` (`c_regex_common` + `f_regex`) ──

    #[test]
    fn regex_matches_the_whole_path_anchored() {
        let fs = tree();
        // BRE whole-path anchor: `a` alone does not match `./a` (the wrap
        // makes it a full-string match), `\./a$`-shaped patterns do.
        let got = run(&[".", "-regex", "./a"], &fs);
        assert_eq!(got.emitted, vec!["./a\n"]);
        let got = run(&[".", "-regex", "a"], &fs);
        assert_eq!(got.emitted, Vec::<String>::new(), "未锚住的裸 a 不整串命中");
        // A BRE group inside the user's pattern still works under the
        // wrapper (`^\(./sub/b\)$`).
        let got = run(&[".", "-regex", "./sub/b"], &fs);
        assert_eq!(got.emitted, vec!["./sub/b\n"]);
    }

    #[test]
    fn iregex_folds_case_and_e_switches_to_ere() {
        let mut fs = tree();
        fs.with_user("root", 0);
        // `-iregex` folds the whole-path comparison.
        let got = run(&[".", "-iregex", "./A"], &fs);
        assert_eq!(got.emitted, vec!["./a\n"]);
        // `-E` switches the dialect (an option, so it precedes the path):
        // `+` becomes a quantifier — `./a+` then matches `./a`.
        let got = run(&["-E", ".", "-regex", "./a+"], &fs);
        assert_eq!(got.emitted, vec!["./a\n"]);
        // Without -E the `+` is a literal in the BRE spelling.
        let got = run(&[".", "-regex", "a+"], &fs);
        assert_eq!(got.emitted, Vec::<String>::new());
    }

    #[test]
    fn a_bad_regexp_is_fatal_at_parse_with_the_c_sentence_shape() {
        // `c_regex_common`'s `errx(1, "regexp %s: %s")` fires at plan build:
        // the parse itself fails before anything walks (the bin turns the
        // error into the errx line; the face test pins that half).
        let owned: Vec<String> = vec![String::from("."), String::from("-regex"), String::from("[")];
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        assert_eq!(
            parse(&refs),
            Err(FindError::Regex("[", "invalid regular expression"))
        );
    }

    #[test]
    fn unresolvable_names_are_fatal_before_any_walking() {
        // C `errx(1, "-user: %s: no such user")` fires at plan build: no
        // walk output, exit 1, one warning with the same sentence.
        let fs = tree();
        let got = run(&[".", "-user", "nosuch"], &fs);
        assert_eq!(got.emitted, Vec::<String>::new());
        assert_eq!(got.rval, 1);
        assert_eq!(got.warnings, ["-user: nosuch: no such user"]);
        // `-group`'s guard has no `+0`/`-0` exceptions (`function.c:1081-1083`).
        let got = run(&[".", "-group", "nosuch"], &fs);
        assert_eq!(got.warnings, ["-group: nosuch: no such group"]);
        // A numeric spelling that fails find_parsenum is the other fatal.
        let got = run(&[".", "-user", "12x"], &fs);
        assert_eq!(got.warnings, ["-user: 12x: illegal numeric value"]);
        // C's guard quirks: `0`-led and `+0`/`-0` spellings pass the guard
        // and go to the numeric path even with no account behind them.
        let got = run(&[".", "-user", "+0"], &fs);
        assert_eq!(got.rval, 0);
        assert_eq!(got.warnings, Vec::<String>::new());
    }

    #[test]
    fn default_print_is_preorder_and_includes_the_start() {
        let got = run(&["."], &tree());
        assert_eq!(got.emitted, vec![".\n", "./a\n", "./sub\n", "./sub/b\n"]);
        assert_eq!(got.rval, 0);
    }

    #[test]
    fn depth_option_flips_directories_to_postorder() {
        let got = run(&["-d", "."], &tree());
        assert_eq!(got.emitted, vec!["./a\n", "./sub/b\n", "./sub\n", ".\n"]);
    }

    #[test]
    fn prune_skips_the_subtree_at_preorder() {
        // No output entry, so the implicit -print runs only when the segment
        // is true; only the named sub therefore emits a line.
        let got = run(&[".", "-name", "sub", "-prune"], &tree());
        assert_eq!(got.emitted, vec!["./sub\n"]);
        // b is never visited because of the prune.
        assert!(!got.emitted.iter().any(|e| e.contains("/b")));
    }

    #[test]
    fn xdev_reports_a_cross_device_dir_but_does_not_descend() {
        let mut fs = FakeFs::new();
        fs.dir(".", 1, &["a", "sub"]);
        fs.file("./a", 1);
        // sub is on another device (1 vs 2), so its children are no longer
        // visited.
        fs.dir("./sub", 2, &["b"]);
        fs.file("./sub/b", 2);
        let plain = run(&["."], &fs);
        assert!(plain.emitted.iter().any(|e| e.contains("./sub/b")));
        let gated = run(&["-x", "."], &fs);
        assert_eq!(gated.emitted, vec![".\n", "./a\n", "./sub\n"]);
    }

    #[test]
    fn unreadable_dir_warns_and_is_not_emitted() {
        let mut fs = tree();
        fs.unreadable.insert(String::from("./sub"));
        let got = run(&["."], &fs);
        // FTS_DNR: sub itself is not evaluated (continue), so only a warning
        // is reported.
        assert_eq!(got.emitted, vec![".\n", "./a\n"]);
        assert_eq!(got.rval, 1);
        assert_eq!(
            got.warnings,
            vec!["./sub: Permission denied"],
            "整行钉死：只查「含不含路径」会放过任何编造的后缀"
        );
    }

    /// C prints `warnx("%s: %s", path, strerror(number))` for both `FTS_NS` and
    /// `FTS_DNR` (`find.c:230-231`), so the suffix must be **the sentence for
    /// the number**, not a phrase describing the failure kind. The authority for
    /// the sentence is `minix-types`' errno table (counterpart of C's
    /// `compat_errlist.c`), re-exported here through `minix_sys::Strerror`; an
    /// off-table code takes C's own `Unknown error: %u`.
    #[test]
    fn walk_warning_line_ends_with_the_strerror_sentence() {
        let mut fs = tree();
        fs.dir(".", 1, &["a", "gone"]);
        fs.fail("./gone", WhichCall::Lstat, minix_sys::ENOENT);
        let got = run(&[".", "-name", "nope"], &fs);
        assert_eq!(
            got.warnings,
            vec!["./gone: No such file or directory"],
            "`FTS_NS` 的句子取自号，不取自失手类型"
        );
        assert_eq!(got.rval, 1);

        // The same shape on the `FTS_DNR` side: EACCES's sentence is
        // "Permission denied".
        let mut fs2 = tree();
        fs2.fail("./sub", WhichCall::Dnr, minix_sys::EACCES);
        let got2 = run(&["."], &fs2);
        assert_eq!(got2.warnings, vec!["./sub: Permission denied"]);

        // Off-table code: C's `strerror` prints "Unknown error: <unsigned
        // value>", and this tree has the same shape.
        let mut fs3 = tree();
        fs3.dir(".", 1, &["a", "gone"]);
        fs3.fail("./gone", WhichCall::Lstat, 140);
        let got3 = run(&[".", "-name", "nope"], &fs3);
        assert_eq!(got3.warnings, vec!["./gone: Unknown error: 140"]);
    }

    /// In logical mode, when `stat` fails and `lstat` fails too, C reports **the
    /// stat call's number**: `fts.c:950-955` first stores
    /// `saved_errno = errno` (stat's) and uses it as `fts_errno` only when
    /// `lstat` fails too. The two numbers give different sentences, so taking
    /// the wrong one prints the wrong message.
    #[test]
    fn logical_walk_reports_the_stat_errno_not_the_lstat_one() {
        let mut fs = tree();
        fs.fail(".", WhichCall::Stat, minix_sys::EPERM);
        fs.fail(".", WhichCall::Lstat, minix_sys::ENOENT);
        let got = run(&["-L", "."], &fs);
        assert_eq!(
            got.warnings,
            vec![".: Operation not permitted"],
            "报的是 stat 的号（EPERM），不是 lstat 的号（ENOENT）"
        );
        assert_eq!(got.rval, 1);
    }

    #[test]
    fn unstatable_child_warns_and_skips_only_itself() {
        let mut fs = tree();
        fs.dir(".", 1, &["a", "gone", "sub"]);
        fs.unstatable.insert(String::from("./gone"));
        let got = run(&[".", "-name", "nope", "-print"], &fs);
        // gone reports FTS_NS and rval=1; everything else proceeds normally
        // (-name nope is always false, so nothing is emitted).
        assert_eq!(got.rval, 1);
        assert_eq!(
            got.warnings,
            vec!["./gone: No such file or directory"],
            "整行钉死：后缀必须是 `strerror` 的句子"
        );
        assert!(got.emitted.is_empty());
    }

    #[test]
    fn xargs_reject_reports_illegal_path() {
        let mut fs = FakeFs::new();
        fs.dir(".", 1, &["a b"]);
        fs.file("./a b", 1);
        let got = run(&["-X", "."], &fs);
        assert_eq!(got.emitted, vec![".\n"]); // a path with a space is rejected
        assert_eq!(got.rval, 1);
        assert!(
            got.warnings
                .iter()
                .any(|w| w.contains("./a b: illegal path"))
        );
    }

    #[test]
    fn physical_walk_does_not_follow_a_symlink_into_a_dir() {
        let mut fs = FakeFs::new();
        fs.dir(".", 1, &["link"]);
        fs.link("./link", 1);
        // Phantom children: if a physical walk wrongly descended into the
        // symlink as a directory, they would show up.
        fs.dir("./link", 1, &["deep"]);
        fs.file("./link/deep", 1);
        // Register the link's own `lstat` bits last (`FakeFs.lstats` lets a
        // later write of the same key override the earlier one) so `./link`'s
        // type is S_IFLNK — it must not be descended into even though it has
        // children as a directory.
        fs.link("./link", 1);
        let got = run(&["."], &fs);
        assert_eq!(got.emitted, vec![".\n", "./link\n"]);
    }

    #[test]
    fn type_filter_selects_dirs_and_leaves_files_unprinted() {
        // `-type d`: only directories emit lines; the implicit -print does not
        // stack on top (here -type d has no output, so print is added, but when
        // -type d is false the short-circuit means print does not run).
        let got = run(&[".", "-type", "d"], &tree());
        assert_eq!(got.emitted, vec![".\n", "./sub\n"]);
    }

    #[test]
    fn logical_follows_symlink_to_dir_and_descends() {
        let mut fs = FakeFs::new();
        fs.dir(".", 1, &["sub", "lnk"]);
        fs.dir("./sub", 1, &["f"]);
        fs.file("./sub/f", 1);
        // ./lnk is a symlink to ./sub; in logical mode open reaches the target
        // and children come out as ./lnk/f.
        fs.file("./lnk/f", 1);
        fs.symlink_to("./lnk", "./sub", 1);
        let logical = run(&["-L", "."], &fs);
        assert!(logical.emitted.iter().any(|e| e == "./lnk\n"));
        assert!(logical.emitted.iter().any(|e| e == "./lnk/f\n"));
        // Physical mode (the default) does not follow: lnk emits one line as
        // the link itself, and its children do not appear.
        let physical = run(&["."], &fs);
        assert!(physical.emitted.iter().any(|e| e == "./lnk\n"));
        assert!(!physical.emitted.iter().any(|e| e.contains("./lnk/f")));
    }

    #[test]
    fn logical_detects_cycle_and_terminates() {
        let mut fs = FakeFs::new();
        fs.dir(".", 1, &["sub"]);
        fs.dir("./sub", 1, &["back"]);
        // ./sub/back -> . forms a cycle: logical mode must recognise FTS_DC,
        // evaluate it, and not descend forever.
        fs.symlink_to("./sub/back", ".", 1);
        let got = run(&["-L", "."], &fs);
        // back itself is evaluated once (FTS_DC falls into find.c's default
        // branch) but not descended into, so no deeper cycle path appears.
        assert!(got.emitted.iter().any(|e| e == "./sub/back\n"));
        assert!(!got.emitted.iter().any(|e| e.contains("./sub/back/sub")));
        assert_eq!(
            got.emitted.iter().filter(|e| **e == "./sub/back\n").count(),
            1
        );
    }

    #[test]
    fn logical_broken_symlink_is_evaluated_as_a_leaf() {
        let mut fs = FakeFs::new();
        fs.dir(".", 1, &["bad"]);
        fs.symlink_to("./bad", "./gone", 1); // the target does not exist
        let got = run(&["-L", "."], &fs);
        // FTS_SLNONE: stat fails and lstat succeeds → evaluate the link itself
        // (one line), without descending and without reporting NS.
        assert_eq!(got.emitted, vec![".\n", "./bad\n"]);
        assert_eq!(got.rval, 0);
        assert!(got.warnings.is_empty());
    }

    #[test]
    fn comfollow_follows_only_the_start_path() {
        // `-H`: if the start path is a symlink to a directory it is followed
        // (`fts.c:171`), but children follow physical mode (no following).
        let mut fs = FakeFs::new();
        fs.dir("./real", 1, &["x"]);
        fs.file("./entry/x", 1);
        fs.symlink_to("./entry", "./real", 1);
        let got = run(&["-H", "./entry"], &fs);
        // The start ./entry is followed to a directory → descend → ./entry/x
        // appears.
        assert!(got.emitted.iter().any(|e| e == "./entry\n"));
        assert!(got.emitted.iter().any(|e| e == "./entry/x\n"));
    }
}
