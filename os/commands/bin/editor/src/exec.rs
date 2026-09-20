//! `ed` command execution: one session, one command line at a time.
//!
//! Ground truth: `minix3/bin/ed/main.c` — `exec_command` (line 465, cases
//! from 481), `extract_addr_range` (285), `check_addr_range` (864),
//! `GET_THIRD_ADDR` (391), `GET_COMMAND_SUFFIX` (428), `display_lines`
//! (1242), `put_tty_line` (`io.c:307`), `read_file`/`write_file`
//! (`io.c`), `get_filename` (941), and the main loop (198-280). The parse
//! halves live in [`crate::addr`] and [`crate::cmd`]; this module owns
//! the session state (current line, modified flag, marks, default file
//! name) and the effect of every command letter on it.
//!
//! # Design
//!
//! Execution is a step function over lines: the caller feeds one command
//! line (or one text-input line while an `a`/`i`/`c` is collecting) and
//! receives either a flow decision ([`Flow::Continue`]/[`Flow::Quit`]/
//! [`Flow::QuitModified`]) or an error carrying the C editor's `errmsg`
//! string. All output and file traffic goes through the [`EditorIo`] seam,
//! so the decision logic stays pure (scripted tests drive it without a
//! kernel) and the binary is a thin shell over `minix_sys`.
//!
//! Faithfulness notes (each anchored at its command below): display
//! advances the current line to the last displayed line
//! (`put_tty_line`'s `current_addr = from++`), `d` re-advances with
//! `INC_MOD` (`ed.h:101`), `m`/`t` take a third address and only `m`
//! rejects a destination inside the range, and `wq`/`wQ` quit after a
//! whole-buffer write. Deliberate gaps (substitute `s`, global
//! `g`/`v`/`G`/`V`, undo `u`, shell `!`) answer through the same `?`
//! channel with an explicit "not wired" message instead of pretending.

use crate::addr::{
    evaluate_with, evaluate_range_with, parse_range, AddressRange, Context, SearchProbe, MAX_MARKS,
};
use crate::cmd::{parse_command, Command, Modifiers};
use crate::store::{TextStore, MAX_LINES, MAX_TEXT};
use crate::EditorError;
use alloc::vec::Vec;
use minix_regex::sed::{apply as subst_apply, Subst, SubstScope};
use minix_regex::pattern::compile_basic;

/// `GPR`: print the (new) current line after the command (`ed.h:65`).
const GPR: u8 = 0o2;
/// `GLS`: list with visible control characters (`ed.h:66`).
const GLS: u8 = 0o4;
/// `GNP`: enumerate lines as they print (`ed.h:67`).
const GNP: u8 = 0o10;

/// `l` wrap column default (`main.c:1409`; the C editor narrows it from
/// `TIOCGWINSZ` when one answers — hosted runs keep the default).
pub const COLS: usize = 72;
/// Scroll length default (`main.c:460`, `ws_row - 2` on a real tty).
pub const ROWS: i32 = 22;
/// File-name capacity: C `MAXPATHLEN` (`get_filename`: `n - 1 >
/// MAXPATHLEN` is "filename too long", `main.c:958-960`).
pub const MAX_FILENAME: usize = 1024;

/// Why a step could not run. The C editor funnels every failure through
/// one `?` channel plus a saved `errmsg` string (`seterrmsg`); the string
/// is what `h` prints and what `H` mode repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecError {
    /// The saved `errmsg` for this failure.
    pub message: &'static str,
}

/// What the caller should do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Read the next command line.
    Continue,
    /// `q`/`Q`/`wq` accepted: exit 0.
    Quit,
    /// `q` (or plain `e`) with unsaved changes: the main loop prints `?`
    /// plus "warning: file modified" and, in script mode, exits 2
    /// (`main.c:239-249`); interactively it clears the flag and goes on.
    QuitModified,
}

/// Output and file effects of one step, injected so the decision logic
/// stays testable. `minix_sys` provides the production half; tests script
/// this trait.
pub trait EditorIo {
    /// Standard output (`put_tty_line`'s `putchar` stream, `io.c:307`).
    fn emit(&mut self, bytes: &[u8]);
    /// Standard error (`?`, warning messages, byte counts).
    fn emit_err(&mut self, bytes: &[u8]);
    /// Read a whole file into `out`; returns the byte count. A file that
    /// does not fit is [`EditorError::TooLong`] (the store could not hold
    /// it either — same capacity discipline).
    fn read_file(&mut self, name: &str, out: &mut [u8]) -> Result<usize, EditorError>;
    /// Write `data` to a file (append when asked); returns bytes written.
    fn write_file(&mut self, name: &str, data: &[u8], append: bool)
    -> Result<usize, EditorError>;
}

/// The prompt text (`-p string`, default `*`): a fixed buffer because the
/// session is a fixed-memory type like the rest of the crate.
#[derive(Debug, Clone, Copy)]
struct Prompt {
    len: usize,
    bytes: [u8; 32],
}

impl Prompt {
    fn star() -> Self {
        let mut bytes = [0; 32];
        bytes[0] = b'*';
        Prompt { len: 1, bytes }
    }

    fn from(text: &str) -> Result<Self, EditorError> {
        let bytes = text.as_bytes();
        if bytes.len() > 32 {
            return Err(EditorError::TooLong);
        }
        let mut buf = [0; 32];
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(Prompt { len: bytes.len(), bytes: buf })
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Session state: the C globals of `main.c:90-110` plus the pending
/// text-input position for `a`/`i`/`c`.
pub struct Session {
    /// `current_addr`: 0 only while the buffer is empty.
    pub current: usize,
    /// `modified`: unsaved changes (`q` refuses, `e` refuses softly).
    pub modified: bool,
    /// `scripted` (`-s`): suppress prompts and byte counts, quit on
    /// warnings instead of continuing.
    pub scripted: bool,
    /// `secure` (`-S`): shell access rejected (`main.c:141`).
    pub secure: bool,
    /// `red`: invoked under a name whose third-to-last byte is `r`
    /// (`main.c:118`) — file names may not contain `/` or `..`
    /// (`is_legal_filename`).
    pub restricted: bool,
    /// `garrulous` (`H`): explain every error.
    pub garrulous: bool,
    /// Whether the prompt prints before each command (`P` toggles).
    pub prompt_on: bool,
    /// `old_filename`: the default file name (`main.c:100`).
    pub filename_len: usize,
    pub filename: [u8; MAX_FILENAME + 1],
    /// Line marks (`'a`..`'z`). Numbers follow lines across inserts and
    /// deletes above them; a mark inside a deleted range is dropped. C
    /// hangs marks on line nodes, so marks there also survive moves —
    /// declared deviation, see 09-editors.md §5.
    pub marks: [Option<usize>; MAX_MARKS],
    /// The saved `errmsg` (`seterrmsg`), for `h`/`H`.
    pub error_msg: Option<&'static str>,
    /// While collecting text for `a`/`i`/`c`: the insert-before position
    /// of the next input line.
    pending_input: Option<usize>,
    /// `G`/`V` 交互全局的进行态（C `exec_global` 的 interact 半，
    /// glbl.c:107-134）：每个活跃行显示后等一条命令。
    pending_global: Option<PendingGlobal>,
    /// 上一次的模式字节（`s/old/…` 写入；`//` 空模式与裸 `s` 复用——C 的
    /// `pat` 全局加 `expr` 缓存，re.c:59-88）。空 len = 无。
    last_pattern: [u8; MAX_TEXT],
    last_pattern_len: usize,
    /// 上一次的替换模板（`%%<delim>` 复用形与裸 `s` 用——C 的 `rhbuf`，
    /// sub.c:44-46）。空 len = 无。
    last_replacement: [u8; MAX_TEXT],
    last_replacement_len: usize,
    /// 上一次 `s` 的作用域（裸 `s`/`sg`/`sN` 重放——C 的 `sgflag`/`sgnum`
    /// 全局）。`None` = 还没有过替换（"no previous substitution"）。
    last_scope: Option<SubstScope>,
    /// 撤销栈（C `ustack`/`u_p`，undo.c:41-43）。每个条目记一次变更；
    /// `u` 逆序回放后翻种翻转序，第二次 `u` 即重做。
    undo_stack: Vec<UndoEntry>,
    /// `u_current_addr`/`u_addr_last`（C undo.c:64-65）：变更前的现场，
    /// `None` = 尚未启用（`u` 回 "nothing to undo"）。
    undo_current: Option<usize>,
    undo_last: Option<usize>,
    /// `isglobal`（C main.c:91）：全局命令执行中——清栈被抑制（整段
    /// 全局是一条撤销单位）、嵌套 `g` 被拒、`s` 无匹配不算错、裸地址
    /// 缺省即当前行。
    is_global: bool,
    /// The suffix `gflag` of the command that opened text-input mode; the
    /// post-input display uses it (`exec_command` returns it, and the
    /// input lines are consumed inside the same call in C).
    pending_gflag: u8,
    prompt: Prompt,
    /// `-p` operand (`main.c:124`); `P` re-arms from it.
    opt_prompt: Option<Prompt>,
}

impl Session {
    /// A fresh session with the command-line flags applied (`-s`, `-S`,
    /// `-p string`); `restricted` mirrors invocation as `red`.
    pub fn new(scripted: bool, secure: bool, restricted: bool, opt_prompt: Option<&str>) -> Self {
        Session {
            current: 0,
            modified: false,
            scripted,
            secure,
            restricted,
            garrulous: false,
            prompt_on: false,
            filename_len: 0,
            filename: [0; MAX_FILENAME + 1],
            marks: [None; MAX_MARKS],
            error_msg: None,
            pending_input: None,
            pending_global: None,
            pending_gflag: 0,
            last_pattern: [0; MAX_TEXT],
            last_pattern_len: 0,
            last_replacement: [0; MAX_TEXT],
            last_replacement_len: 0,
            last_scope: None,
            undo_stack: Vec::new(),
            undo_current: None,
            undo_last: None,
            is_global: false,
            prompt: Prompt::star(),
            opt_prompt: opt_prompt.and_then(|text| Prompt::from(text).ok()),
        }
    }

    /// The default file name as text, if one is set.
    pub fn filename_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.filename[..self.filename_len]).ok()
    }

    /// The `old_filename` setter (`strlcpy(old_filename, fnp, ...)` at
    /// `main.c:189`, `:548`, `:794`): `e`/`f`/`r`/`w` maintain it and the
    /// binary's initial-file block sets it once.
    pub fn set_filename(&mut self, text: &str) -> Result<(), ExecError> {
        let bytes = text.as_bytes();
        if bytes.len() > MAX_FILENAME {
            return Err(err("filename too long"));
        }
        self.filename_len = bytes.len();
        self.filename[..bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// The prompt bytes to print (`P` re-arms from the `-p` operand, or
    /// `*` when there was none — `main.c:668-673`).
    pub fn prompt_bytes(&self) -> &[u8] {
        self.prompt.as_slice()
    }

    fn toggle_prompt(&mut self) {
        if self.prompt_on {
            self.prompt_on = false;
        } else {
            self.prompt = self.opt_prompt.unwrap_or(Prompt::star());
            self.prompt_on = true;
        }
    }

    fn last_pattern_bytes(&self) -> &[u8] {
        &self.last_pattern[..self.last_pattern_len]
    }

    fn last_replacement_bytes(&self) -> &[u8] {
        &self.last_replacement[..self.last_replacement_len]
    }

    /// 记住一个新编译的模式（C `pat = tpat`，main.c:739-743——**替换成功
    /// 与否都记**：缓存在解析时就落账）。
    fn remember_pattern(&mut self, bytes: &[u8]) -> Result<(), ExecError> {
        if bytes.len() > MAX_TEXT {
            return Err(err("out of memory"));
        }
        self.last_pattern[..bytes.len()].copy_from_slice(bytes);
        self.last_pattern_len = bytes.len();
        Ok(())
    }

    fn remember_replacement(&mut self, bytes: &[u8]) -> Result<(), ExecError> {
        if bytes.len() > MAX_TEXT {
            return Err(err("out of memory"));
        }
        self.last_replacement[..bytes.len()].copy_from_slice(bytes);
        self.last_replacement_len = bytes.len();
        Ok(())
    }
}

fn err(message: &'static str) -> ExecError {
    ExecError { message }
}

fn context_of<S: TextStore>(store: &S, sess: &Session) -> Context {
    Context { line_count: store.line_count(), current: sess.current, marks: sess.marks }
}

/// The suffix bits (`GPR`/`GLS`/`GNP`) of a parsed modifier set. A `!`
/// never survives: the C suffix macro only accepts `p`/`l`/`n` and calls
/// anything else an invalid suffix (`main.c:428-448`).
fn suffix_bits(modifiers: &Modifiers) -> Result<u8, ExecError> {
    if modifiers.force {
        return Err(err("invalid command suffix"));
    }
    let mut g = 0;
    if modifiers.print {
        g |= GPR;
    }
    if modifiers.list {
        g |= GLS;
    }
    if modifiers.number {
        g |= GNP;
    }
    Ok(g)
}

/// Scan a trailing `pln` suffix by hand (for the commands whose letter is
/// followed by a parameter first: `k`, `m`/`t`, `z`). Same grammar as
/// `GET_COMMAND_SUFFIX` (`main.c:428-448`): the run, then end of line.
fn suffix_scan(rest: &str) -> Result<u8, ExecError> {
    let bytes = rest.as_bytes();
    let mut at = 0;
    let mut g = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'p' => g |= GPR,
            b'l' => g |= GLS,
            b'n' => g |= GNP,
            _ => break,
        }
        at += 1;
    }
    if at != bytes.len() {
        return Err(err("invalid command suffix"));
    }
    Ok(g)
}

/// `w`/`e`/`E`/`r`/`f` read a file name right after the letter, so any
/// glued `p`/`l`/`n`/`!` (beyond `wq`) is a character the C parser would
/// have fed to its space check — always "unexpected command suffix"
/// (`main.c:508`, `:545`, `:780`, `:946`).
fn reject_glued(modifiers: &Modifiers, allow_quit: bool) -> Result<(), ExecError> {
    let noisy = modifiers.force || modifiers.print || modifiers.list || modifiers.number;
    if noisy || (modifiers.quit_after && !allow_quit) {
        return Err(err("unexpected command suffix"));
    }
    Ok(())
}

fn marks_shift_up(sess: &mut Session, at: usize, lines: usize) {
    for mark in sess.marks.iter_mut() {
        if let Some(line) = *mark
            && line >= at
        {
            *mark = Some(line + lines);
        }
    }
}

fn marks_delete(sess: &mut Session, from: usize, to: usize) {
    let span = to - from + 1;
    for mark in sess.marks.iter_mut() {
        *mark = match *mark {
            Some(line) if line < from => Some(line),
            Some(line) if line <= to => None,
            Some(line) => Some(line - span),
            None => None,
        };
    }
}

/// 一条撤销记录的种类（C `UADD`/`UDEL`，ed.h:84-85）。`u` 回放后翻转：
/// Add 变 Delete（下次 `u` 即重做删除），反之亦然。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UndoKind {
    /// 插入了 `from..=to` 行（撤销 = 删回）。
    Add,
    /// 删除了 `from..=to` 行，`text` 留有内容（撤销 = 插回）。
    Delete,
}

/// 一条撤销记录（C `undo_t`，ed.h:87-91——C 用行节点指针保身份，本模型
/// 用行号加内容：Delete 件把被删文本带在身上，`u` 时原样插回）。
#[derive(Debug, Clone)]
struct UndoEntry {
    kind: UndoKind,
    from: usize,
    to: usize,
    text: Vec<u8>,
}

/// `clear_undo_stack`（undo.c:107-120）：清栈并把现场快照进
/// `undo_current`/`undo_last`——这条快照是 `u` 的"改动前状态"。
/// 每个改动型命令（a/i/c/d/e/E/i/j/m/r/s/t）在动手前调用。
fn clear_undo<S: TextStore>(store: &S, sess: &mut Session) {
    sess.undo_stack.clear();
    sess.undo_current = Some(sess.current);
    sess.undo_last = Some(store.line_count());
}

/// `push_undo_stack` 的插入半（C 的 UADD，main.c:1085/1125/1203：每一行
/// 插入记一条）。
fn push_undo_add(sess: &mut Session, pos: usize) {
    sess.undo_stack.push(UndoEntry {
        kind: UndoKind::Add,
        from: pos,
        to: pos,
        text: Vec::new(),
    });
}

/// One inserted line: the store call plus the session bookkeeping C does
/// inside `put_sbuf_line` (`main.c:1129` — every insert marks the buffer
/// modified).
fn insert_line<S: TextStore>(
    store: &mut S,
    sess: &mut Session,
    pos: usize,
    text: &str,
) -> Result<(), ExecError> {
    store.insert(pos, text).map_err(map_store_error)?;
    marks_shift_up(sess, pos, 1);
    sess.modified = true;
    push_undo_add(sess, pos);
    Ok(())
}

/// A deleted range: the store call plus `delete_lines`' bookkeeping
/// (`main.c:1222` 的 UDEL 记账加 `main.c:1233-1235`——current 落到
/// `from - 1`，缓冲已改）。删除的文本随条目带走（C 靠节点保留，这里
/// 靠 `text`）。
fn delete_range<S: TextStore>(
    store: &mut S,
    sess: &mut Session,
    from: usize,
    to: usize,
) -> Result<(), ExecError> {
    let mut text = Vec::new();
    let mut n = from;
    while n <= to {
        let mut buf = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
        text.extend_from_slice(&buf[..used]);
        text.push(b'\n');
        n += 1;
    }
    store.delete(from, to).map_err(map_store_error)?;
    marks_delete(sess, from, to);
    sess.current = from - 1;
    sess.modified = true;
    sess.undo_stack.push(UndoEntry {
        kind: UndoKind::Delete,
        from,
        to,
        text,
    });
    Ok(())
}

/// 活跃表（C `glbl.c` 的 `active_list`，升序记内容快照）。定长缓冲：
/// 活跃内容是缓冲内容的子集，store 的容量就是表的容量上限。
struct ActiveList {
    text: [u8; MAX_TEXT],
    starts: [u16; MAX_LINES + 1],
    count: usize,
}

impl ActiveList {
    fn new() -> Self {
        ActiveList { text: [0; MAX_TEXT], starts: [0; MAX_LINES + 1], count: 0 }
    }

    fn line(&self, index: usize) -> &[u8] {
        &self.text[self.starts[index] as usize..self.starts[index + 1] as usize]
    }

    fn push(&mut self, bytes: &[u8]) {
        let start = self.starts[self.count] as usize;
        debug_assert!(self.count < MAX_LINES && start + bytes.len() <= MAX_TEXT);
        self.text[start..start + bytes.len()].copy_from_slice(bytes);
        self.count += 1;
        self.starts[self.count] = (start + bytes.len()) as u16;
    }
}

/// `G`/`V` 的进行态：活跃表加游标，加本段 G 已见过的最近命令（C 的
/// `ocmd` 静态量，`&` 重放源；`seen` 对应 C 局部 `cmd != NULL` 的
/// "no previous command" 门——每段 G 重新起算）。
struct PendingGlobal {
    active: ActiveList,
    /// 下一活跃行下标。
    next: usize,
    /// 内容重定位的扫描起点（行号）。
    relocate: usize,
    /// 当前等答案的行号。
    at: usize,
    prev: [u8; MAX_TEXT],
    prev_len: usize,
    seen: bool,
    /// 尾缀打印位（`G/…/n` 之类，C `main.c:566` 的 GET_COMMAND_SUFFIX）。
    gflag: u8,
}

/// `g`/`v`/`G`/`V` 的公共前半（C `build_active_list`，glbl.c:43-67）：
/// 范围求值（缺省整缓冲）、模式解析（空模式复用上一模式并落账）、按
/// 匹配与否收活跃表。返回表与模式后的余部起点。
fn build_active<S: TextStore>(
    store: &mut S,
    sess: &mut Session,
    line: &str,
    cursor: usize,
    range: &AddressRange,
    is_match: bool,
) -> Result<(ActiveList, usize), ExecError> {
    let count = store.line_count();
    let mut probe = StoreSearch { store, sess: &*sess };
    let (from, to) =
        evaluate_range_with(range, &context_of(store, sess), (1, count), line, &mut probe)
            .map_err(map_store_error)?;
    // 探针的借用到此为止：后面的建表走可变借用。
    let rest = &line[cursor..];
    let bytes = rest.as_bytes();
    if bytes.is_empty() || bytes[0] == b' ' {
        return Err(err("invalid pattern delimiter"));
    }
    let delim = bytes[0];
    let (pspan, after) =
        crate::addr::scan_pattern(bytes, 1, delim).map_err(map_store_error)?;
    let mut pattern_len = pspan.len as usize;
    let mut pattern_buf = [0u8; MAX_TEXT];
    pattern_buf[..pattern_len].copy_from_slice(&bytes[1..1 + pattern_len]);
    if pattern_len == 0 {
        // 空模式复用上一模式（C `get_compiled_pattern` 的 expr 缓存）。
        pattern_len = sess.last_pattern_len;
        pattern_buf[..pattern_len].copy_from_slice(sess.last_pattern_bytes());
    }
    if pattern_len == 0 {
        return Err(err("no previous pattern"));
    }
    sess.remember_pattern(&pattern_buf[..pattern_len])?;
    let pattern_text =
        core::str::from_utf8(&pattern_buf[..pattern_len]).map_err(|_| err("invalid content"))?;
    let compiled = compile_basic(pattern_text).map_err(|_| err("invalid pattern"))?;
    let mut active = ActiveList::new();
    let mut n = from;
    while n <= to {
        let mut buf = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
        let text = core::str::from_utf8(&buf[..used]).map_err(|_| err("invalid content"))?;
        if compiled.is_match(text) == is_match {
            active.push(&buf[..used]);
        }
        n += 1;
    }
    Ok((active, after))
}

/// `g`/`v` 的执行半（C `glbl.c` 全篇 + `main.c:562-574`）：
///
/// 1. 范围缺省整缓冲（`check_addr_range(1, addr_last)`）；尾形式
///    `<定界>模式<定界>子命令`（空子命令不隐含 `p`——本构建无
///    BACKWARDS，glbl.c:56-63）；空模式复用上一模式。
/// 2. 建活跃表（`build_active_list`，glbl.c:43-67）：`g` 收匹配行、
///    `v` 收不匹配行，升序记内容快照。
/// 3. 清撤销栈一次（exec_global:143）——整段全局是**一条**撤销单位，
///    子命令里的清栈被 `is_global` 抑制。
/// 4. 逐活跃行执行子命令（`exec_global:104-144`）：当前行落到活跃行
///    上；活跃行已被删/已改则跳过（定位规则见下）；子命令出错即中止
///    整段全局（已执行的变更保留，C 同）。嵌套 `g` 在派发处拒绝
///    （main.c:562-564）。
///
/// **登记偏差（活跃行的定位）**：C 用行节点身份（被删行的槽位置空、
/// `next_active_node` 跳过），本模型按匹配时的内容从上次命中处向后
/// 重定位——计数减少视为有行被删，从命中行重扫；否则从下一行起扫。
/// 重复内容的行在搬移/插入类子命令下可能与 C 差位。
fn global_command<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    cursor: usize,
    range: &AddressRange,
) -> Result<Flow, ExecError> {
    if sess.is_global {
        return Err(err("cannot nest global commands"));
    }
    // 命令字母在 `line[cursor - 1]`（`g` 或 `v`）。
    let is_match = line.as_bytes()[cursor - 1] == b'g';
    let (active, after) = build_active(store, sess, line, cursor, range, is_match)?;
    if !sess.is_global {
        clear_undo(store, sess);
    }
    let cmd = &line[cursor + after..];
    let was_global = sess.is_global;
    sess.is_global = true;
    let mut result = Ok(Flow::Continue);
    let mut cursor_pos = 1usize;
    for index in 0..active.count {
        let want = active.line(index).to_vec();
        // 内容重定位：从上一命中处向后找同文行（见函数头偏差注记）。
        let mut found = None;
        let mut n = cursor_pos;
        while n <= store.line_count() {
            let mut b = [0u8; MAX_TEXT];
            let used = store.read_line(n, &mut b).map_err(map_store_error)?;
            if &b[..used] == want.as_slice() {
                found = Some(n);
                break;
            }
            n += 1;
        }
        let Some(at) = found else { continue };
        sess.current = at;
        if cmd.is_empty() {
            // 空子命令：本构建不隐含 `p`（glbl.c:56-63 无 BACKWARDS），
            // 只落当前行。
            cursor_pos = at + 1;
            continue;
        }
        let before = store.line_count();
        result = step_inner(store, sess, cmd, io);
        if result.is_err() || matches!(result, Ok(Flow::Quit) | Ok(Flow::QuitModified)) {
            break;
        }
        // 游标推进：计数减少 = 有行被删，下一个活跃行可能就落在命中行
        // 上（重扫）；否则命中行若存活即消费掉（下一行起扫）。
        let survived = at <= store.line_count() && {
            let mut b = [0u8; MAX_TEXT];
            let used = store.read_line(at, &mut b).map_err(map_store_error)?;
            &b[..used] == want.as_slice()
        };
        cursor_pos = if store.line_count() < before || !survived { at } else { at + 1 };
    }
    sess.is_global = was_global;
    result
}

/// `G`/`V` 的启动半（C `main.c:560-574` 的 G/V 支路）：与 `g`/`v` 同一
/// 建表半（`build_active_list`），随后读 p/l/n 尾缀、清栈一次、置
/// 输入态并显示第一个活跃行——余下的交互跨多次 [`step`] 进行。
fn global_interactive_start<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    cursor: usize,
    range: &AddressRange,
) -> Result<Flow, ExecError> {
    if sess.is_global {
        return Err(err("cannot nest global commands"));
    }
    let is_match = line.as_bytes()[cursor - 1] == b'G';
    let (active, after) = build_active(store, sess, line, cursor, range, is_match)?;
    let gflag = suffix_scan(&line[cursor + after..])?;
    if !sess.is_global {
        clear_undo(store, sess);
    }
    sess.is_global = true;
    let mut pending = PendingGlobal {
        active,
        next: 0,
        relocate: 1,
        at: 0,
        prev: [0; MAX_TEXT],
        prev_len: 0,
        seen: false,
        gflag,
    };
    if global_resume(store, sess, io, &mut pending)? {
        sess.pending_global = Some(pending);
        Ok(Flow::Continue)
    } else {
        sess.is_global = false;
        Ok(Flow::Continue)
    }
}

/// 定位并显示下一个活跃行（C `exec_global` 循环头的 `display_lines`，
/// glbl.c:107-110）。返回假表示活跃表走尽——整个 G 收束。
fn global_resume<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    pending: &mut PendingGlobal,
) -> Result<bool, ExecError> {
    while pending.next < pending.active.count {
        let want = pending.active.line(pending.next).to_vec();
        // 内容重定位：从上一命中处向后找同文行（批次三十同式）。
        let mut found = None;
        let mut n = pending.relocate;
        while n <= store.line_count() {
            let mut b = [0u8; MAX_TEXT];
            let used = store.read_line(n, &mut b).map_err(map_store_error)?;
            if &b[..used] == want.as_slice() {
                found = Some(n);
                break;
            }
            n += 1;
        }
        let Some(at) = found else {
            pending.next += 1;
            continue;
        };
        sess.current = at;
        pending.at = at;
        display(store, sess, io, at, at, pending.gflag)?;
        return Ok(true);
    }
    Ok(false)
}

/// 一条 `G`/`V` 的回答（C glbl.c:111-134）：空行跳过、`&` 重放、其余
/// 按全局文法执行。`Ok(None)` = 继续等下一活跃行的回答；`Ok(Some)`
/// = G 收束并带出流向；`Err` = G 整段中止（已执行的变更保留，C 同）。
fn global_interact_answer<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    pending: &mut PendingGlobal,
) -> Result<Option<Flow>, ExecError> {
    let at = pending.at;
    let want = pending.active.line(pending.next).to_vec();
    if line.is_empty() {
        // `n == 1 && ibuf == "\n"`：这一行跳过（glbl.c:119-120）。
        pending.next += 1;
        pending.relocate = at + 1;
        return if global_resume(store, sess, io, pending)? {
            Ok(None)
        } else {
            Ok(Some(Flow::Continue))
        };
    }
    let cmd: &str;
    if line == "&" {
        if !pending.seen {
            // C glbl.c:121-125：本段还没给过命令。
            return Err(err("no previous command"));
        }
        cmd = core::str::from_utf8(&pending.prev[..pending.prev_len])
            .map_err(|_| err("invalid content"))?;
    } else {
        if line.len() > MAX_TEXT {
            return Err(err("out of memory"));
        }
        pending.prev[..line.len()].copy_from_slice(line.as_bytes());
        pending.prev_len = line.len();
        pending.seen = true;
        cmd = line;
    }
    let before = store.line_count();
    match step_inner(store, sess, cmd, io) {
        Err(e) => Err(e),
        Ok(flow @ (Flow::Quit | Flow::QuitModified)) => Ok(Some(flow)),
        Ok(Flow::Continue) => {
            // C `append_lines` 的 isglobal 支路（main.c:1059-1064）：正文
            // 从命令串里读——一段回答收不到正文行，`a`/`i`/`c` 落空即过。
            if sess.pending_input.take().is_some() {
                sess.pending_gflag = 0;
            }
            // 重定位记账：计数减少 = 有行被删（重扫）；否则命中行若
            // 存活即消费掉（下一行起扫）。
            let survived = at <= store.line_count() && {
                let mut b = [0u8; MAX_TEXT];
                let used = store.read_line(at, &mut b).map_err(map_store_error)?;
                &b[..used] == want.as_slice()
            };
            pending.relocate = if store.line_count() < before || !survived { at } else { at + 1 };
            pending.next += 1;
            if global_resume(store, sess, io, pending)? {
                Ok(None)
            } else {
                Ok(Some(Flow::Continue))
            }
        }
    }
}

/// `pop_undo_stack`（undo.c:71-105）：`u` 的本体——逆序回放撤销栈，把
/// 每条变更翻回去，然后**翻转种类、倒转栈序、互换现场快照**（第二次
/// `u` 即重做；C 的 `type ^= 1` 加 USWAP，undo.c:117-123）。
///
/// 回放走裸 store 调用（不再入栈），标记按原命令同款平移/摘除；结束
/// 后当前行与缓冲规模取自改动前快照（`u_current_addr`/`u_addr_last`）。
fn pop_undo<S: TextStore>(store: &mut S, sess: &mut Session) -> Result<(), ExecError> {
    // `undo_last` 是重做态的缓冲规模：回放本身把它恢复出来（C 的
    // `addr_last = u_addr_last` 互换），这里用它做回放完整性的断言。
    let (Some(undo_current), Some(undo_last)) = (sess.undo_current, sess.undo_last) else {
        return Err(err("nothing to undo"));
    };
    // C：`else if (u_p) modified = 1;`——有账可翻就把缓冲标脏。
    if !sess.undo_stack.is_empty() {
        sess.modified = true;
    }
    let o_current = sess.current;
    let o_last = store.line_count();
    for i in (0..sess.undo_stack.len()).rev() {
        let (kind, from, to) = {
            let e = &sess.undo_stack[i];
            (e.kind, e.from, e.to)
        };
        match kind {
            UndoKind::Add => {
                // 撤销插入：先把 `from..=to` 的现行文本收进条目（翻转后
                // 是 Delete，重做时要用），再删回。
                let mut text = Vec::new();
                let mut n = from;
                while n <= to {
                    let mut buf = [0u8; MAX_TEXT];
                    let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
                    text.extend_from_slice(&buf[..used]);
                    text.push(b'\n');
                    n += 1;
                }
                store.delete(from, to).map_err(map_store_error)?;
                marks_delete(sess, from, to);
                let e = &mut sess.undo_stack[i];
                e.kind = UndoKind::Delete;
                e.text = text;
            }
            UndoKind::Delete => {
                // 撤销删除：把带的文本按行插回 `from`，翻成 Add。
                let text = sess.undo_stack[i].text.clone();
                let line_text =
                    core::str::from_utf8(&text).map_err(|_| err("invalid content"))?;
                let pieces = LinesOf { rest: line_text };
                let mut pos = from;
                let mut k = 0;
                for piece in pieces {
                    store.insert(pos, piece).map_err(map_store_error)?;
                    pos += 1;
                    k += 1;
                }
                marks_shift_up(sess, from, k);
                let e = &mut sess.undo_stack[i];
                e.kind = UndoKind::Add;
                e.from = from;
                e.to = from + k - 1;
            }
        }
    }
    sess.undo_stack.reverse();
    sess.current = undo_current;
    sess.undo_current = Some(o_current);
    sess.undo_last = Some(o_last);
    debug_assert_eq!(store.line_count(), undo_last, "回放应把缓冲还原到改动前规模");
    Ok(())
}

fn map_store_error(e: EditorError) -> ExecError {
    match e {
        // The C editor's buffer-exhaustion face (`io.c` sbuf full).
        EditorError::TooLong => err("out of memory"),
        EditorError::InvalidArgument => err("invalid address"),
        // C `get_matching_node_addr`/`search_and_replace` 的 "no match"
        // （main.c:938，sub.c:175-179）。
        EditorError::NoMatch => err("no match"),
    }
}

/// `display_lines` + `put_tty_line` (`main.c:1242`, `io.c:307`). Display
/// advances the current line: `put_tty_line` is called with
/// `current_addr = from++`, so the last displayed line stays current.
fn display<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    from: usize,
    to: usize,
    gflag: u8,
) -> Result<(), ExecError> {
    if from == 0 {
        return Err(err("invalid address"));
    }
    let mut buf = [0u8; MAX_TEXT];
    let mut n = from;
    while n <= to {
        let len = store.read_line(n, &mut buf).map_err(map_store_error)?;
        put_tty_line(io, &buf[..len], n, gflag);
        sess.current = n;
        n += 1;
    }
    Ok(())
}

fn put_tty_line<I: EditorIo>(io: &mut I, line: &[u8], number: usize, gflag: u8) {
    let listing = gflag & GLS != 0;
    let mut col = 0usize;
    if gflag & GNP != 0 {
        // C `printf("%ld\t", n)` (`io.c:315`).
        let mut digits = [0u8; 20];
        let used = write_decimal(number as u64, &mut digits);
        io.emit(&digits[..used]);
        io.emit(b"\t");
        col = 8;
    }
    for &byte in line {
        if listing {
            col += 1;
            if col > COLS {
                io.emit(b"\\\n");
                col = 1;
            }
        }
        if !listing || ((0x20..0x7f).contains(&byte) && byte != b'\\') {
            io.emit(&[byte]);
        } else {
            // `ESCAPES "\a\b\f\n\r\t\v\\"` maps to `ESCCHARS "abfnrtv\\"`
            // (`io.c:302-303`); anything else is three octal digits
            // (`io.c:342-345`).
            io.emit(b"\\");
            let named = match byte {
                0x07 => Some(b'a'),
                0x08 => Some(b'b'),
                0x0c => Some(b'f'),
                0x0a => Some(b'n'),
                0x0d => Some(b'r'),
                0x09 => Some(b't'),
                0x0b => Some(b'v'),
                b'\\' => Some(b'\\'),
                _ => None,
            };
            match named {
                Some(c) => io.emit(&[c]),
                None => {
                    let oct = [
                        ((byte & 0o300) >> 6) + b'0',
                        ((byte & 0o070) >> 3) + b'0',
                        (byte & 0o007) + b'0',
                    ];
                    io.emit(&oct);
                    col += 2;
                }
            }
        }
    }
    if listing {
        io.emit(b"$");
    }
    io.emit(b"\n");
}

/// Decimal writer for the `%ld` faces (line numbers, byte counts).
fn write_decimal(mut value: u64, out: &mut [u8]) -> usize {
    if value == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut at = out.len();
    while value > 0 && at > 0 {
        at -= 1;
        out[at] = (value % 10) as u8 + b'0';
        value /= 10;
    }
    let used = out.len() - at;
    out.copy_within(at.., 0);
    used
}

/// The main-loop tail: a nonzero suffix `gflag` prints the (new) current
/// line (`main.c:228-233`).
fn finish<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    gflag: u8,
) -> Result<Flow, ExecError> {
    if gflag != 0 {
        display(store, sess, io, sess.current, sess.current, gflag)?;
    }
    Ok(Flow::Continue)
}

/// A file-name operand, unescaped into the caller's buffer.
enum FileName {
    /// Nothing typed: use `old_filename` (error when unset).
    Default,
    /// Bytes `taken[..len]` hold the operand.
    Fresh(usize),
}

/// `get_filename` minus the shell branch (`main.c:941-984`): a glued
/// non-space is "unexpected command suffix", blanks then end of line is
/// "invalid filename", backslashes escape the next byte, `!` reaches for
/// a shell. The whole tail after the blanks is the name (the C reader
/// runs to end of line), so no suffix can follow a file name.
fn take_filename(rest: &str, sess: &Session, taken: &mut [u8]) -> Result<FileName, ExecError> {
    let bytes = rest.as_bytes();
    if bytes.is_empty() {
        return Ok(FileName::Default);
    }
    // `if (!isspace(*ibufp)) → unexpected command suffix` — the caller
    // strips the trailing newline, so blanks-then-end is the only blank
    // shape that reaches here.
    if bytes[0] != b' ' && bytes[0] != b'\t' {
        return Err(err("unexpected command suffix"));
    }
    let mut at = 0;
    while at < bytes.len() && (bytes[at] == b' ' || bytes[at] == b'\t') {
        at += 1;
    }
    if at >= bytes.len() {
        // `SKIP_BLANKS` ran into the newline (`main.c:947-950`).
        return Err(err("invalid filename"));
    }
    if bytes[at] == b'!' {
        if sess.secure || sess.restricted {
            return Err(err("shell access restricted"));
        }
        return Err(err("shell access not wired"));
    }
    let mut len = 0;
    let mut scan = at;
    while scan < bytes.len() {
        let b = bytes[scan];
        if b == b'\\' {
            // `strip_escapes` (`main.c`): a backslash quotes one byte.
            scan += 1;
            if scan >= bytes.len() {
                break;
            }
        }
        if len >= MAX_FILENAME || len >= taken.len() {
            return Err(err("filename too long"));
        }
        taken[len] = bytes[scan];
        len += 1;
        scan += 1;
    }
    // `is_legal_filename` under `red`: no leading `!`, no `..`, no `/`.
    if sess.restricted
        && (len == 0 || out_starts_bang(taken, len) || &taken[..len] == b".." || taken[..len].contains(&b'/'))
    {
        return Err(err("shell access restricted"));
    }
    if len == 0 {
        return Ok(FileName::Default);
    }
    Ok(FileName::Fresh(len))
}

fn out_starts_bang(taken: &[u8], len: usize) -> bool {
    len > 0 && taken[0] == b'!'
}

/// Resolve an operand (or the default name) into `name_out`, so the
/// borrow does not pin the session while the store mutates. The store
/// world is `&str`: non-UTF-8 names are rejected through the same `?`
/// channel as every other failure.
fn resolve_name<'a>(
    sess: &Session,
    kind: FileName,
    taken: &[u8],
    name_out: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let source: &[u8] = match kind {
        FileName::Default => {
            if sess.filename_len == 0 {
                return Err(err("no current filename"));
            }
            &sess.filename[..sess.filename_len]
        }
        FileName::Fresh(len) => &taken[..len],
    };
    if source.len() > name_out.len() {
        return Err(err("filename too long"));
    }
    name_out[..source.len()].copy_from_slice(source);
    core::str::from_utf8(&name_out[..source.len()]).map_err(|_| err("invalid content"))
}

/// The `r`/`e` read target: the typed name, or the just-maintained
/// default (`*fnp ? fnp : old_filename`, `main.c:534`/`:797`).
fn target_name<'a>(
    sess: &Session,
    typed: &str,
    buf: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let source: &[u8] = if typed.is_empty() {
        &sess.filename[..sess.filename_len]
    } else {
        typed.as_bytes()
    };
    if source.len() > buf.len() {
        return Err(err("filename too long"));
    }
    buf[..source.len()].copy_from_slice(source);
    core::str::from_utf8(&buf[..source.len()]).map_err(|_| err("invalid content"))
}

/// Split `text` into lines the way the store holds them (newline
/// terminated; a final piece without one still counts).
struct LinesOf<'a> {
    rest: &'a str,
}

impl<'a> Iterator for LinesOf<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        if self.rest.is_empty() {
            return None;
        }
        match self.rest.find('\n') {
            Some(at) => {
                let piece = &self.rest[..at];
                self.rest = &self.rest[at + 1..];
                Some(piece)
            }
            None => {
                let piece = self.rest;
                self.rest = "";
                Some(piece)
            }
        }
    }
}

/// Read lines `from..=to` into `buf` with no separators at all — the `j`
/// face (`join_lines`' newline glue would just re-insert the range).
fn concat_lines<'a, S: TextStore>(
    store: &S,
    from: usize,
    to: usize,
    buf: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let mut len = 0;
    let mut n = from;
    while n <= to {
        let mut line = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut line).map_err(map_store_error)?;
        if len + used > buf.len() {
            return Err(err("out of memory"));
        }
        buf[len..len + used].copy_from_slice(&line[..used]);
        len += used;
        n += 1;
    }
    core::str::from_utf8(&buf[..len]).map_err(|_| err("invalid content"))
}

/// Read lines `from..=to` into `buf`, newline separated, as text.
fn join_lines<'a, S: TextStore>(
    store: &S,
    from: usize,
    to: usize,
    buf: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let mut len = 0;
    let mut n = from;
    while n <= to {
        let mut line = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut line).map_err(map_store_error)?;
        if len + used + 1 > buf.len() {
            return Err(err("out of memory"));
        }
        buf[len..len + used].copy_from_slice(&line[..used]);
        len += used;
        buf[len] = b'\n';
        len += 1;
        n += 1;
    }
    core::str::from_utf8(&buf[..len]).map_err(|_| err("invalid content"))
}

fn ensure_line_end(rest: &str) -> Result<(), ExecError> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(err("invalid command suffix"))
    }
}

fn parse_range_err(line: &str) -> Result<(AddressRange, usize), ExecError> {
    parse_range(line).map_err(|_| err("invalid address"))
}

/// 缓冲上的搜索探针（`addr::SearchProbe` 的 store 半）：模式交给 08 篇
/// 的 BRE 引擎，扫描次序照抄 C `get_matching_node_addr`
/// （main.c:919-938）——从当前行的下一行（反向：上一行）起，
/// `INC_MOD`/`DEC_MOD`（ed.h:101-102）绕整缓冲一圈，当前行最后被访问；
/// 空缓冲没有可扫的行，按无效地址回答。
struct StoreSearch<'a, S: TextStore> {
    store: &'a S,
    sess: &'a Session,
}

impl<S: TextStore> SearchProbe for StoreSearch<'_, S> {
    fn find_line(
        &mut self,
        pattern: &[u8],
        forward: bool,
    ) -> Result<Option<usize>, EditorError> {
        let count = self.store.line_count();
        if count == 0 {
            return Err(EditorError::InvalidArgument);
        }
        let text = core::str::from_utf8(pattern).map_err(|_| EditorError::InvalidArgument)?;
        let compiled = compile_basic(text).map_err(|_| EditorError::InvalidArgument)?;
        let mut n = self.sess.current;
        loop {
            n = if forward {
                if n + 1 > count {
                    0
                } else {
                    n + 1
                }
            } else if n == 0 {
                count
            } else {
                n - 1
            };
            if n != 0 {
                let mut buf = [0u8; MAX_TEXT];
                let used = self.store.read_line(n, &mut buf)?;
                let line =
                    core::str::from_utf8(&buf[..used]).map_err(|_| EditorError::InvalidArgument)?;
                if compiled.is_match(line) {
                    return Ok(Some(n));
                }
            }
            if n == self.sess.current {
                return Ok(None);
            }
        }
    }
}

/// Execute one command line (or one text-input line while collecting).
///
/// `line` carries no trailing newline (the caller strips it, matching the
/// parse halves). Errors set the session's saved message (`seterrmsg`) —
/// `h` and `H` read it back — and the caller prints `?`.
pub fn step<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    line: &str,
    io: &mut I,
) -> Result<Flow, ExecError> {
    let result = step_inner(store, sess, line, io);
    if let Err(e) = &result {
        sess.error_msg = Some(e.message);
    }
    result
}

fn step_inner<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    line: &str,
    io: &mut I,
) -> Result<Flow, ExecError> {
    // Text-input mode: every line is literal text until a lone `.`.
    if let Some(pos) = sess.pending_input {
        if line == "." {
            sess.pending_input = None;
            let g = sess.pending_gflag;
            sess.pending_gflag = 0;
            return finish(store, sess, io, g);
        }
        insert_line(store, sess, pos, line)?;
        sess.current = pos;
        sess.pending_input = Some(pos + 1);
        return Ok(Flow::Continue);
    }

    // `G`/`V` 交互全局：每行输入是当前活跃行的回答（C `exec_global` 的
    // interact 半跨多次 step——每个匹配行显示后等一条命令）。
    if sess.pending_global.is_some() {
        let mut pending = sess.pending_global.take().unwrap();
        match global_interact_answer(store, sess, io, line, &mut pending) {
            Ok(None) => {
                sess.pending_global = Some(pending);
                return Ok(Flow::Continue);
            }
            Ok(Some(flow)) => {
                sess.is_global = false;
                return Ok(flow);
            }
            Err(e) => {
                sess.is_global = false;
                return Err(e);
            }
        }
    }

    let (range, consumed) = parse_range_err(line)?;
    let rest = &line[consumed..];
    // 搜索探针：不可变借用 store 与 sess；NLL 保证各臂在末次使用之后即可
    // 可变访问（求值都在变更之前）。
    let mut probe = StoreSearch { store, sess: &*sess };
    if rest.is_empty() {
        // A bare address (or bare newline): display the second address,
        // defaulting to the line after current (`main.c:884-890`,
        // `check_addr_range(1, current_addr + 1)`). Display moves current
        // to that line, which is how a lone `3` navigates.
        let step_past = usize::from(!sess.is_global);
        let (_, to) = evaluate_range_with(
            &range,
            &context_of(store, sess),
            (1, sess.current + step_past),
            line,
            &mut probe,
        )
        .map_err(map_store_error)?;
        display(store, sess, io, to, to, 0)?;
        return Ok(Flow::Continue);
    }

    let (command, modifiers, cursor) =
        parse_command(line, consumed).map_err(|_| err("unknown command"))?;
    let ctx = context_of(store, sess);
    let count = store.line_count();
    // Position right after the command letter, for the commands whose
    // parameter starts there (`k`, `m`/`t`, `z`, file names).
    let letter_end = consumed + 1;

    match command {
        Command::Append => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            sess.pending_input = Some(second + 1);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Insert => {
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            if second == 0 {
                return Err(err("invalid address"));
            }
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            sess.pending_input = Some(second);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Change => {
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            delete_range(store, sess, from, to)?;
            sess.pending_input = Some(from);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Delete => {
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            delete_range(store, sess, from, to)?;
            // `INC_MOD(current_addr, addr_last)` (`ed.h:101`) then
            // `if (addr != 0)`: slide to the line after the deleted block
            // while one exists (`main.c:498-502`).
            let next = sess.current + 1;
            if next <= store.line_count() {
                sess.current = next;
            }
            finish(store, sess, io, g)
        }
        Command::Print | Command::List | Command::Number => {
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            let bits = match command {
                Command::Print => GPR,
                Command::List => GLS,
                _ => GNP,
            } | g;
            display(store, sess, io, from, to, bits)?;
            Ok(Flow::Continue)
        }
        Command::LineNumber => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            // `printf("%ld\n", addr_cnt ? second_addr : addr_last)`
            // (`main.c:866-871`).
            let number = if range.first.is_some() || range.second.is_some() {
                let which = range.second.or(range.first).ok_or_else(|| err("invalid address"))?;
                evaluate_with(which, &ctx, line, &mut probe).map_err(map_store_error)?
            } else {
                store.line_count()
            };
            let mut digits = [0u8; 20];
            let used = write_decimal(number as u64, &mut digits);
            io.emit(&digits[..used]);
            io.emit(b"\n");
            finish(store, sess, io, g)
        }
        Command::Quit => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if sess.modified && !sess.scripted {
                // C returns EMOD; the main loop prints `?` plus the
                // warning, quits in script mode, and otherwise clears the
                // flag and carries on (`main.c:239-249`).
                sess.error_msg = Some("warning: file modified");
                return Ok(Flow::QuitModified);
            }
            let _ = g;
            Ok(Flow::Quit)
        }
        Command::Edit | Command::EditForce => {
            if consumed > 0 {
                return Err(err("unexpected address"));
            }
            reject_glued(&modifiers, false)?;
            // Plain `e` refuses unsaved changes softly (`main.c:506-509`);
            // the main loop's EMOD branch clears the flag interactively.
            if command == Command::Edit && sess.modified && !sess.scripted {
                sess.error_msg = Some("warning: file modified");
                return Ok(Flow::QuitModified);
            }
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            let typed_len = typed.len();
            if typed_len > 0 {
                sess.set_filename(typed)?;
            }
            let mut target_buf = [0u8; MAX_FILENAME + 1];
            let target = target_name(sess, typed, &mut target_buf)?;
            // C 522/537：e/E 无条件清撤销栈（在删旧缓冲之前）。
            if !sess.is_global {
                clear_undo(store, sess);
            }
            if count >= 1 {
                delete_range(store, sess, 1, count)?;
            }
            sess.marks = [None; MAX_MARKS];
            let last = read_into_store(store, sess, io, target, 0)?;
            sess.current = last;
            sess.modified = false;
            Ok(Flow::Continue)
        }
        Command::Read => {
            reject_glued(&modifiers, false)?;
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, count), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            if !typed.is_empty() && sess.filename_len == 0 {
                // `if (*old_filename == '\0' && *fnp != '!')`
                // (`main.c:792-794`): the first `r` names the file.
                sess.set_filename(typed)?;
            }
            let mut target_buf = [0u8; MAX_FILENAME + 1];
            let target = target_name(sess, typed, &mut target_buf)?;
            // C inserts after `second_addr` and leaves current on the last
            // inserted line (`read_stream`'s add loop).
            let last = read_into_store(store, sess, io, target, second)?;
            sess.current = last;
            Ok(Flow::Continue)
        }
        Command::Write | Command::WriteAppend => {
            // `wq`/`wQ` glue is legal (`main.c:804-807`); any other glued
            // modifier would have been a file-name character in C.
            reject_glued(&modifiers, true)?;
            let (from, to) = if count == 0 {
                (0, 0)
            } else {
                evaluate_range_with(&range, &ctx, (1, count), line, &mut probe)
                .map_err(|_| err("invalid address"))?
            };
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            if !typed.is_empty() && sess.filename_len == 0 {
                sess.set_filename(typed)?;
            }
            let mut target_buf = [0u8; MAX_FILENAME + 1];
            let target = target_name(sess, typed, &mut target_buf)?;
            let mut data = [0u8; MAX_TEXT];
            let mut len = 0;
            let mut n = from;
            while n <= to {
                let mut line = [0u8; MAX_TEXT];
                let used = store.read_line(n, &mut line).map_err(map_store_error)?;
                if len + used + 1 > data.len() {
                    return Err(err("out of memory"));
                }
                data[len..len + used].copy_from_slice(&line[..used]);
                len += used;
                data[len] = b'\n';
                len += 1;
                n += 1;
            }
            let written = io
                .write_file(target, &data[..len], command == Command::WriteAppend)
                .map_err(|e| match e {
                    EditorError::TooLong => err("out of memory"),
                    EditorError::InvalidArgument => err("cannot open output file"),
                    EditorError::NoMatch => err("no match"),
                })?;
            if !sess.scripted {
                let mut digits = [0u8; 20];
                let used = write_decimal(written as u64, &mut digits);
                io.emit_err(&digits[..used]);
                io.emit_err(b"\n");
            }
            // `else if (addr == addr_last) modified = 0` (`main.c:811-812`):
            // a whole-buffer write clears the flag. `addr` there is the
            // line count written (`m - n + 1`, `io.c` write_file tail).
            let lines_written = to.saturating_sub(from) + 1;
            let whole = count == 0 || lines_written == count;
            if whole {
                sess.modified = false;
            }
            if modifiers.quit_after {
                // `wq` on a partial write keeps the EMOD dance
                // (`main.c:813-815`): warn interactively, quit in script.
                if sess.modified && !sess.scripted {
                    sess.error_msg = Some("warning: file modified");
                    return Ok(Flow::QuitModified);
                }
                return Ok(Flow::Quit);
            }
            Ok(Flow::Continue)
        }
        Command::Filename => {
            // `main.c:541-556`: an address is rejected, a glued modifier
            // would have been the name's first character, and the current
            // name prints whether or not a new one was typed.
            if consumed > 0 {
                return Err(err("unexpected address"));
            }
            reject_glued(&modifiers, false)?;
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            if !typed.is_empty() {
                sess.set_filename(typed)?;
            }
            let name = sess.filename_str().unwrap_or("");
            io.emit(name.as_bytes());
            io.emit(b"\n");
            Ok(Flow::Continue)
        }
        Command::Mark => {
            // `k` reads its mark letter unconditionally (`main.c:618-620`),
            // then the usual suffix check applies.
            let mark = line.as_bytes().get(letter_end).copied();
            let g = suffix_scan(&line[letter_end + mark.is_some() as usize..])?;
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            if second == 0 {
                return Err(err("invalid address"));
            }
            if let Some(c @ b'a'..=b'z') = mark {
                sess.marks[(c - b'a') as usize] = Some(second);
            }
            finish(store, sess, io, g)
        }
        Command::Move | Command::Transfer => {
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            // `GET_THIRD_ADDR` (`main.c:391-407`): the destination is the
            // second address of a fresh extraction; none is "destination
            // expected", beyond the last line is "invalid address".
            let (third, used) = parse_range_err(&line[letter_end..])?;
            let Some(dest_spec) = third.second.or(third.first) else {
                return Err(err("destination expected"));
            };
            // `GET_THIRD_ADDR` (`main.c:401-404`) rejects only negative
            // and beyond-last, so `m0`/`t0` (move to the front) are legal
            // even though a command address may not be zero.
            let dest = match dest_spec.base {
                crate::addr::Base::Number(0) if dest_spec.offset == 0 => 0,
                _ => evaluate_with(dest_spec, &ctx, line, &mut probe).map_err(map_store_error)?,
            };
            if dest > count {
                return Err(err("invalid address"));
            }
            let g = suffix_scan(&line[letter_end + used..])?;
            let len = to - from + 1;
            let mut buf = [0u8; MAX_TEXT];
            let joined = join_lines(store, from, to, &mut buf)?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            if command == Command::Move {
                if dest + 1 == from || dest == to {
                    // `move_lines`' no-op shape (`main.c:1141`): the block
                    // already sits where it is asked to go; current moves
                    // to the second address (`main.c:1144`).
                    sess.current = to;
                    return finish(store, sess, io, g);
                }
                if from <= dest && dest < to {
                    return Err(err("invalid destination"));
                }
                delete_range(store, sess, from, to)?;
                let before = if dest < from { dest + 1 } else { dest + 1 - len };
                store.insert(before, joined).map_err(map_store_error)?;
                marks_shift_up(sess, before, len);
                sess.modified = true;
                // `current_addr = addr + (addr < first ? len : 0)`
                // (`main.c:1167-1169`): the last line of the moved block.
                sess.current = before - 1 + len;
            } else {
                store.insert(dest + 1, joined).map_err(map_store_error)?;
                marks_shift_up(sess, dest + 1, len);
                sess.modified = true;
                // The copies end at `dest + len` (copy_lines walks its
                // duplicates to the end, `main.c:1180-1215`).
                sess.current = dest + len;
            }
            finish(store, sess, io, g)
        }
        Command::Join => {
            let (from, to) = evaluate_range_with(
                &range,
                &ctx,
                (sess.current, sess.current + 1),
                line,
                &mut probe,
            )
                .map_err(|_| err("invalid address"))?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if from != to {
                // `join_lines` glues with newlines (`add_line_node` per
                // line), but `j` needs ONE line: no separators at all
                // (`main.c:602-610`), then a single insert.
                let mut buf = [0u8; MAX_TEXT];
                let joined = concat_lines(store, from, to, &mut buf)?;
                delete_range(store, sess, from, to)?;
                store.insert(from, joined).map_err(map_store_error)?;
                marks_shift_up(sess, from, 1);
                sess.modified = true;
                sess.current = from;
            }
            finish(store, sess, io, g)
        }
        Command::Help => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if let Some(message) = sess.error_msg {
                io.emit_err(message.as_bytes());
                io.emit_err(b"\n");
            }
            finish(store, sess, io, g)
        }
        Command::HelpMode => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            sess.garrulous = !sess.garrulous;
            if sess.garrulous
                && let Some(message) = sess.error_msg
            {
                io.emit_err(message.as_bytes());
                io.emit_err(b"\n");
            }
            finish(store, sess, io, g)
        }
        Command::PromptToggle => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            sess.toggle_prompt();
            finish(store, sess, io, g)
        }
        Command::Crypt => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            let _ = g;
            // The non-DES build's answer, verbatim (`main.c:843-845`).
            Err(err("crypt unavailable"))
        }
        Command::Scroll => {
            // `main.c:848-864`: an optional row count, then the window
            // from the addressed line (second defaults to current + 1).
            let bytes = &line[letter_end..];
            let mut at = 0;
            let mut rows = ROWS;
            while at < bytes.len() && bytes.as_bytes()[at].is_ascii_digit() {
                rows = rows
                    .saturating_mul(10)
                    .saturating_add((bytes.as_bytes()[at] - b'0') as i32);
                at += 1;
            }
            let (_, second) = evaluate_range_with(&range, &ctx, (1, sess.current + 1), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            let g = suffix_scan(&bytes[at..])?;
            let to = count.min(second.saturating_add(rows.max(0) as usize));
            display(store, sess, io, second, to, g)?;
            Ok(Flow::Continue)
        }
        Command::Substitute => {
            substitute_command(store, sess, io, line, cursor, &modifiers, &range)
        }
        Command::Undo => {
            // C main.c:794-800：地址即 "unexpected address"，后缀照读，
            // 然后 `pop_undo_stack`——**不清栈**（清栈是改动型命令的事）。
            if range.first.is_some() || range.second.is_some() {
                return Err(err("unexpected address"));
            }
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            pop_undo(store, sess)?;
            finish(store, sess, io, g)
        }
        Command::Global => {
            global_command(store, sess, io, line, cursor, &range)
        }
        Command::GlobalInteractive => {
            global_interactive_start(store, sess, io, line, cursor, &range)
        }
        Command::Shell => {
            if sess.secure || sess.restricted {
                Err(err("shell access restricted"))
            } else {
                Err(err("shell access not wired"))
            }
        }
    }
}

/// `s` 的执行半（C `main.c:698-770` + `sub.c:49-240`）：
///
/// 1. **前导旗标**（可连写；起手后遇到非旗标即 "invalid command suffix"）：
///    `g` 全局、`p` 替换行即打印、`r` 复用上一替换、数字 = 第 N 个匹配。
///    空尾（裸 `s`）= 整体重放。
/// 2. **尾形式** `<delim>pat<delim>repl<delim?>[g|N]?`：定界符任取（空格
///    即 "invalid pattern delimiter"），`repl` 处 `%%<delim>` 复用上一替
///    换（"no previous substitution"），结束定界符行尾可省；其后的 `p`/
///    `l`/`n` 与修饰位同族（C 的 GET_COMMAND_SUFFIX）。
/// 3. **模式缓存**：非空模式编入并立即落账（C `pat = tpat`，main.c:739）；
///    空模式（`//`）复用上一模式（"no previous pattern"，re.c:69）。范围
///    缺省当前行；替换发生的最后一行成为新的当前行；全程无替换回
///    "no match"（C `search_and_replace`，sub.c:141-181）。
fn substitute_command<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    cursor: usize,
    modifiers: &Modifiers,
    range: &AddressRange,
) -> Result<Flow, ExecError> {
    let rest = &line[cursor..];
    let bytes = rest.as_bytes();
    // ── 前导旗标 ──
    let mut scope_override: Option<SubstScope> = None;
    let mut reuse_replacement = false;
    let mut print_replaced = false;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'g' => {
                scope_override = Some(SubstScope::Global);
                at += 1;
            }
            b'p' => {
                print_replaced = true;
                at += 1;
            }
            b'r' => {
                reuse_replacement = true;
                at += 1;
            }
            b'0'..=b'9' => {
                let start = at;
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    at += 1;
                }
                let n: u32 = rest[start..at].parse().map_err(|_| err("invalid command suffix"))?;
                if n == 0 {
                    return Err(err("invalid command suffix"));
                }
                scope_override = Some(SubstScope::Nth(n));
            }
            _ => break,
        }
    }
    if at > 0 && at < bytes.len() {
        // 旗标起手后只许旗标到行尾（C `while (sflags && *ibufp != '\n')`
        // 的 default 臂，main.c:719-722）。
        return Err(err("invalid command suffix"));
    }
    let flagged = at > 0;
    // ── 模式与替换的来源 ──（本 crate 无堆：来源先抄进局部定长缓冲）
    let mut pattern_buf = [0u8; MAX_TEXT];
    let mut pattern_len: usize;
    let mut repl_buf = [0u8; MAX_TEXT];
    let repl_len: usize;
    let mut scope;
    if reuse_replacement {
        // `sr`（C main.c:730-737 的 SGR 半）：从尾里读**新模式**，替换与
        // 作用域沿用上一次（`sgflag`/`sgnum` 不动）；空尾等同裸 `s`。
        let Some(prev) = sess.last_scope else {
            return Err(err("no previous substitution"));
        };
        scope = prev;
        repl_len = sess.last_replacement_len;
        repl_buf[..repl_len].copy_from_slice(sess.last_replacement_bytes());
        pattern_len = 0;
        if !bytes.is_empty() {
            let delim = bytes[0];
            if delim == b' ' {
                return Err(err("invalid pattern delimiter"));
            }
            let (pspan, _after) =
                crate::addr::scan_pattern(bytes, 1, delim).map_err(map_store_error)?;
            pattern_len = pspan.len as usize;
            pattern_buf[..pattern_len].copy_from_slice(&bytes[1..1 + pattern_len]);
        }
    } else if bytes.is_empty() || flagged {
        // 裸/旗标形式：整段复用（C 的 sflags 门：`if (sflags && !pat)`
        // → "no previous substitution"，main.c:725-728）。
        let Some(prev_scope) = sess.last_scope else {
            return Err(err("no previous substitution"));
        };
        pattern_len = sess.last_pattern_len;
        pattern_buf[..pattern_len].copy_from_slice(sess.last_pattern_bytes());
        repl_len = sess.last_replacement_len;
        repl_buf[..repl_len].copy_from_slice(sess.last_replacement_bytes());
        scope = scope_override.unwrap_or(prev_scope);
    } else {
        let delim = bytes[0];
        if delim == b' ' {
            return Err(err("invalid pattern delimiter"));
        }
        let (pspan, after_pattern) =
            crate::addr::scan_pattern(bytes, 1, delim).map_err(map_store_error)?;
        pattern_len = pspan.len as usize;
        pattern_buf[..pattern_len].copy_from_slice(&bytes[1..1 + pattern_len]);
        let mut r_at = after_pattern;
        if bytes.get(r_at) == Some(&b'%') && bytes.get(r_at + 1) == Some(&delim) {
            // `%%<delim>`：复用上一替换（C `extract_subst_template` 的
            // rhbuf 分支，sub.c:82-89）。
            if sess.last_replacement_len == 0 {
                return Err(err("no previous substitution"));
            }
            repl_len = sess.last_replacement_len;
            repl_buf[..repl_len].copy_from_slice(sess.last_replacement_bytes());
            r_at += 2;
        } else {
            let (rspan, r_next) =
                crate::addr::scan_pattern(bytes, r_at, delim).map_err(map_store_error)?;
            repl_len = rspan.len as usize;
            repl_buf[..repl_len].copy_from_slice(&bytes[r_at..r_at + repl_len]);
            r_at = r_next;
        }
        scope = SubstScope::First;
        match bytes.get(r_at) {
            Some(b'g') => {
                scope = SubstScope::Global;
            }
            Some(b'0'..=b'9') => {
                let start = r_at;
                while r_at < bytes.len() && bytes[r_at].is_ascii_digit() {
                    r_at += 1;
                }
                let n: u32 = rest[start..r_at].parse().map_err(|_| err("invalid pattern delimiter"))?;
                if n == 0 {
                    return Err(err("invalid pattern delimiter"));
                }
                scope = SubstScope::Nth(n);
            }
            _ => {}
        }
    }
    // 空模式复用上一模式（C re.c:66-72）。
    if pattern_len == 0 {
        pattern_len = sess.last_pattern_len;
        pattern_buf[..pattern_len].copy_from_slice(sess.last_pattern_bytes());
    }
    if pattern_len == 0 {
        return Err(err("no previous pattern"));
    }
    // 落账（C main.c:739-743 与 sub.c 的 rhbuf：解析成功即记，替换成败
    // 不回头）。
    sess.remember_pattern(&pattern_buf[..pattern_len])?;
    sess.remember_replacement(&repl_buf[..repl_len])?;
    sess.last_scope = Some(scope);
    let pattern_text =
        core::str::from_utf8(&pattern_buf[..pattern_len]).map_err(|_| err("invalid content"))?;
    let replacement_text =
        core::str::from_utf8(&repl_buf[..repl_len]).map_err(|_| err("invalid content"))?;
    let compiled = compile_basic(pattern_text).map_err(|_| err("invalid pattern"))?;
    let template = Subst {
        pattern_text: "",
        replacement: replacement_text,
        scope,
        print: false,
    };
    // 范围缺省当前行（C `check_addr_range(current_addr, current_addr)`，
    // main.c:760-761）。没有当前行（缓冲空）时 C 会去扫它的 0 号头行
    // （空串），无匹配即 "no match"——本模型没有 0 号行，如实折同一错误
    // （登记偏差：能匹配空串的模式在 C 里会真插一行，未知行为面）。
    if sess.current == 0 && range.first.is_none() && range.second.is_none() {
        return Err(err("no match"));
    }
    // C `main.c:684`：全局里 `s` 的清栈被 isglobal 抑制——整段全局是
    // 一条撤销单位。
    if !sess.is_global {
        clear_undo(store, sess);
    }
    let mut probe = StoreSearch { store, sess: &*sess };
    let (from, to) = evaluate_range_with(
        range,
        &context_of(store, sess),
        (sess.current, sess.current),
        line,
        &mut probe,
    )
    .map_err(map_store_error)?;
    // 探针的借用到此为止：后面的删插走可变借用（NLL 分路径结清）。
    let original_current = sess.current;
    let mut last_changed: Option<usize> = None;
    let mut n = from;
    while n <= to {
        let mut buf = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
        let line_text = core::str::from_utf8(&buf[..used]).map_err(|_| err("invalid content"))?;
        let mut out = [0u8; MAX_TEXT];
        let (len, replaced) = subst_apply(&compiled, &template, line_text, &mut out);
        if replaced {
            // C：delete_lines(current, current) + 逐行 put_sbuf_line
            // （sub.c:148-163）；替换模板在行内不会引入换行（模板里的
            // 反斜杠换行是交互续行面，本模型的行不带换行），单行删插。
            // 删与插都走带撤销记账的助手（UDEL + UADD，sub.c:152）。
            let text = core::str::from_utf8(&out[..len]).map_err(|_| err("invalid content"))?;
            delete_range(store, sess, n, n)?;
            insert_line(store, sess, n, text)?;
            last_changed = Some(n);
        }
        n += 1;
    }
    sess.current = last_changed.unwrap_or(original_current);
    // C `sub.c:175` 的 `!(gflag & GLB)`：全局里 `s` 无匹配不算错
    // （main.c:755 置 GLB）。
    if last_changed.is_none() && !sess.is_global {
        return Err(err("no match"));
    }
    // 后缀打印（C：SGP 折 GPR 并清 GLS|GNP，main.c:752-754；其余与
    // GET_COMMAND_SUFFIX 同族）。
    let mut bits = suffix_bits(modifiers)?;
    if print_replaced {
        bits = GPR;
    }
    finish(store, sess, io, bits)
}

/// `read_file`'s store half (`io.c`): read `name`, insert its lines after
/// line `after`, report the byte count (unless scripted), warn about an
/// appended final newline. Returns the last line number now in the
/// buffer (`after` when the file is empty).
fn read_into_store<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    name: &str,
    after: usize,
) -> Result<usize, ExecError> {
    let mut scratch = [0u8; MAX_TEXT];
    let size = io.read_file(name, &mut scratch).map_err(|e| match e {
        EditorError::TooLong => err("out of memory"),
        EditorError::InvalidArgument => err("cannot open input file"),
        EditorError::NoMatch => err("no match"),
    })?;
    let text = core::str::from_utf8(&scratch[..size]).map_err(|_| err("invalid content"))?;
    let mut at = after;
    let lines = LinesOf { rest: text };
    for piece in lines {
        at += 1;
        insert_line(store, sess, at, piece)?;
    }
    if size > 0 && !text.as_bytes().ends_with(b"\n") && !sess.scripted {
        // `read_stream`'s "newline appended" notice (`io.c`), simplified
        // to the text-file branch (binary files are rejected above).
        io.emit_err(b"newline appended\n");
    }
    if !sess.scripted {
        // `read_file` reports the byte count on stderr (`io.c`).
        let mut digits = [0u8; 20];
        let used = write_decimal(size as u64, &mut digits);
        io.emit_err(&digits[..used]);
        io.emit_err(b"\n");
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{GapStore, LineTable};
    use std::collections::BTreeMap;

    /// Scripted I/O: captures output, serves an in-memory file table.
    struct ScriptIo {
        out: Vec<u8>,
        err_out: Vec<u8>,
        files: BTreeMap<String, Vec<u8>>,
    }

    impl ScriptIo {
        fn new() -> Self {
            ScriptIo { out: Vec::new(), err_out: Vec::new(), files: BTreeMap::new() }
        }

        fn with_file(name: &str, content: &str) -> Self {
            let mut io = Self::new();
            io.files.insert(name.to_string(), content.as_bytes().to_vec());
            io
        }

        fn out_text(&self) -> String {
            String::from_utf8(self.out.clone()).unwrap()
        }

        fn out_lines(&self) -> Vec<String> {
            self.out_text()
                .split('\n')
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        }
    }

    impl EditorIo for ScriptIo {
        fn emit(&mut self, bytes: &[u8]) {
            self.out.extend_from_slice(bytes);
        }
        fn emit_err(&mut self, bytes: &[u8]) {
            self.err_out.extend_from_slice(bytes);
        }
        fn read_file(&mut self, name: &str, out: &mut [u8]) -> Result<usize, EditorError> {
            let data = self.files.get(name).ok_or(EditorError::InvalidArgument)?;
            if data.len() > out.len() {
                return Err(EditorError::TooLong);
            }
            out[..data.len()].copy_from_slice(data);
            Ok(data.len())
        }
        fn write_file(
            &mut self,
            name: &str,
            data: &[u8],
            append: bool,
        ) -> Result<usize, EditorError> {
            let entry = self.files.entry(name.to_string()).or_default();
            if append {
                entry.extend_from_slice(data);
            } else {
                *entry = data.to_vec();
            }
            Ok(data.len())
        }
    }

    fn seeded(lines: &[&str]) -> (GapStore, Session) {
        let mut store = GapStore::new();
        for (i, line) in lines.iter().enumerate() {
            store.insert(i + 1, line).unwrap();
        }
        (store, Session::new(false, false, false, None))
    }

    fn feed<S: TextStore>(store: &mut S, sess: &mut Session, io: &mut ScriptIo, lines: &[&str]) {
        for line in lines {
            step(store, sess, line, io)
                .unwrap_or_else(|e| panic!("step {line:?} failed: {e:?}"));
        }
    }

    #[test]
    fn test_append_collects_until_dot_and_prints() {
        let (mut store, mut sess) = seeded(&[]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["a", "first", "second", ".", "1,$n"]);
        assert_eq!(io.out_lines(), ["1\tfirst", "2\tsecond"]);
        assert_eq!(sess.current, 2, "display moves current to the last line");
        assert!(sess.modified, "inserts mark the buffer modified");
    }

    #[test]
    fn test_print_list_formats_match_put_tty_line() {
        let (mut store, mut sess) = seeded(&["plain", "a\tb", "hi"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,$l"]);
        // `l`: tabs named, line-end dollar (`io.c:302-347`).
        assert_eq!(io.out_lines(), ["plain$", "a\tb$".replace('\t', "\\t").as_str(), "hi$"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2n"]);
        // `n`: `%ld\t` prefix (`io.c:315`) and RAW bytes after it — only
        // `l` escapes; current moves to line 2.
        assert_eq!(io.out_lines(), ["2\ta\tb"]);
        assert_eq!(sess.current, 2);
    }

    #[test]
    fn test_delete_readvances_with_inc_mod() {
        let (mut store, mut sess) = seeded(&["one", "two", "three"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1d"]);
        // `INC_MOD(0, 2) = 1`: the line after the deleted block.
        assert_eq!(sess.current, 1);
        assert_eq!(store.line_count(), 2);
        feed(&mut store, &mut sess, &mut io, &["$d"]);
        // Deleting the last line: no next line, current stays at from - 1.
        assert_eq!(sess.current, 1);
    }

    #[test]
    fn test_change_replaces_range_in_place() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2c", "B1", "B2", ".", "1,$p"]);
        assert_eq!(io.out_lines(), ["a", "B1", "B2", "c"]);
    }

    #[test]
    fn test_insert_before_line_one_of_empty_is_rejected() {
        let (mut store, mut sess) = seeded(&["x"]);
        let mut io = ScriptIo::new();
        assert_eq!(
            step(&mut store, &mut sess, "0i", &mut io).unwrap_err().message,
            "invalid address"
        );
    }

    #[test]
    fn test_move_reorders_and_rejects_inside_destination() {
        let (mut store, mut sess) = seeded(&["a", "b", "c", "d"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,2m4", "1,$p"]);
        assert_eq!(io.out_lines(), ["c", "d", "a", "b"]);
        assert_eq!(sess.current, 4, "current rides the last moved line");
        // `1,3m2` lands inside the range → "invalid destination"
        // (`main.c:638-641`).
        assert_eq!(
            step(&mut store, &mut sess, "1,3m2", &mut io).unwrap_err().message,
            "invalid destination"
        );
        // `1,2m2` is the no-op shape (`addr == second_addr`,
        // `main.c:1141`) — legal, current parks on the second address.
        feed(&mut store, &mut sess, &mut io, &["1,2m2"]);
        assert_eq!(sess.current, 2);
    }

    #[test]
    fn test_transfer_duplicates_block() {
        let (mut store, mut sess) = seeded(&["a", "b"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,2t0", "1,$n"]);
        // `t0` copies the block to the front (`main.c:757-763` allows 0).
        assert_eq!(io.out_lines(), ["1\ta", "2\tb", "3\ta", "4\tb"]);
        assert_eq!(sess.current, 4);
    }

    #[test]
    fn test_join_merges_range_into_one_line() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,3j", ".,p"]);
        assert_eq!(io.out_lines(), ["abc"]);
        assert_eq!(sess.current, 1);
    }

    #[test]
    fn test_marks_survive_and_die_with_their_line() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2kx"]);
        // `'x` resolves through the mark table (`addr.rs` Context).
        feed(&mut store, &mut sess, &mut io, &["'xp"]);
        assert_eq!(io.out_lines(), ["b"]);
        // Deleting the marked line drops the mark; `'x` then fails.
        feed(&mut store, &mut sess, &mut io, &["2d"]);
        assert_eq!(
            step(&mut store, &mut sess, "'xp", &mut io).unwrap_err().message,
            "invalid address"
        );
    }

    #[test]
    fn test_line_number_prints_second_or_last() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["="]);
        assert_eq!(io.out_lines(), ["3"], "no address prints addr_last");
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2="]);
        assert_eq!(io.out_lines(), ["2"], "an explicit address prints it");
    }

    #[test]
    fn test_quit_modified_then_quiet_quit() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1d"]);
        assert_eq!(step(&mut store, &mut sess, "q", &mut io).unwrap(), Flow::QuitModified);
        // The main loop clears the flag on the interactive warning
        // (`main.c:243`); the next `q` succeeds.
        sess.modified = false;
        assert_eq!(step(&mut store, &mut sess, "q", &mut io).unwrap(), Flow::Quit);
    }

    #[test]
    fn test_wq_quits_after_whole_write_only() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::with_file("out.txt", "");
        feed(&mut store, &mut sess, &mut io, &["1,2c", "X", "Y", "."]);
        // Partial write + modified: EMOD, not quit (`main.c:813-815`).
        assert_eq!(
            step(&mut store, &mut sess, "2,3wq out.txt", &mut io).unwrap(),
            Flow::QuitModified
        );
        // Whole-buffer write clears modified and quits.
        assert_eq!(step(&mut store, &mut sess, "wq out.txt", &mut io).unwrap(), Flow::Quit);
        assert_eq!(io.files["out.txt"], b"X\nY\nc\n".to_vec());
    }

    #[test]
    fn test_write_reports_and_clears_modified() {
        // Scripted: `-s` suppresses the byte-count notice (`io.c`).
        let mut store = GapStore::new();
        for (i, line) in ["a", "b"].iter().enumerate() {
            store.insert(i + 1, line).unwrap();
        }
        let mut sess = Session::new(true, false, false, None);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["w out.txt"]);
        assert_eq!(io.files["out.txt"], b"a\nb\n".to_vec());
        assert!(!sess.modified, "whole-buffer write clears the flag");
        assert!(io.err_out.is_empty(), "scripted sessions skip byte counts");
    }

    #[test]
    fn test_read_inserts_after_address_and_names_the_file() {
        let mut io = ScriptIo::with_file("part.txt", "p\nq\n");
        let (mut store, mut sess) = seeded(&["a", "b"]);
        feed(&mut store, &mut sess, &mut io, &["1r part.txt", "1,$n"]);
        assert_eq!(io.out_lines(), ["1\ta", "2\tp", "3\tq", "4\tb"]);
        assert_eq!(sess.filename_str(), Some("part.txt"), "first `r` names the file");
    }

    #[test]
    fn test_edit_swaps_buffer_and_reports_newlines_added() {
        let mut io = ScriptIo::with_file("f.txt", "l1\nl2");
        let (mut store, mut sess) = seeded(&["old"]);
        feed(&mut store, &mut sess, &mut io, &["e f.txt", "1,$p"]);
        assert_eq!(io.out_lines(), ["l1", "l2"]);
        assert!(!sess.modified, "`e` ends with a clean buffer");
        // Non-scripted sessions get the newline notice AND the byte count
        // (`read_stream` tail, then `read_file`'s `%lu`).
        assert_eq!(io.err_out, b"newline appended\n5\n".to_vec());
        assert_eq!(sess.filename_str(), Some("f.txt"));
    }

    #[test]
    fn test_edit_refuses_modified_softly() {
        let mut io = ScriptIo::with_file("f.txt", "x\n");
        let (mut store, mut sess) = seeded(&["old1", "old2"]);
        feed(&mut store, &mut sess, &mut io, &["1d"]);
        assert_eq!(store.line_count(), 1);
        assert_eq!(step(&mut store, &mut sess, "e f.txt", &mut io).unwrap(), Flow::QuitModified);
        assert_eq!(store.line_count(), 1, "the buffer is untouched by the refused `e`");
        // `E` discards without asking (`main.c:510`).
        feed(&mut store, &mut sess, &mut io, &["E f.txt"]);
        assert_eq!(store.line_count(), 1);
        assert!(!sess.modified);
    }

    #[test]
    fn test_filename_prints_and_sets() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["f a.txt"]);
        assert_eq!(io.out_lines(), ["a.txt"]);
        assert_eq!(sess.filename_str(), Some("a.txt"));
        // Bare `f` prints the current name (`main.c:548-552`).
        feed(&mut store, &mut sess, &mut io, &["f"]);
        assert_eq!(io.out_lines(), ["a.txt", "a.txt"]);
    }

    #[test]
    fn test_help_reads_the_saved_message() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        let _ = step(&mut store, &mut sess, "0a", &mut io).unwrap_err();
        feed(&mut store, &mut sess, &mut io, &["h"]);
        assert_eq!(io.err_out, b"invalid address\n".to_vec());
    }

    /// `s` 的尾形式（C main.c:698-770 + sub.c）：首替、`g` 全局、`N` 第
    /// N 个、`p` 打印；范围缺省当前行；替换行成为新当前行；无替换回
    /// "no match"。
    #[test]
    fn test_substitute_tail_forms() {
        let (mut store, mut sess) = seeded(&["foo boo", "bar"]);
        let mut io = ScriptIo::new();
        // `2s/a/0/`：作用第二行，首替。
        step(&mut store, &mut sess, "2s/a/0/", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..3], b"b0r");
        // 当前行的 `s`：`1s/o/0/` 只换第一个 o。
        step(&mut store, &mut sess, "1s/o/0/", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"f0o boo");
        assert_eq!(sess.current, 1, "替换行成为当前行");
        // `g` 全局。
        step(&mut store, &mut sess, "1s/o/0/g", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"f00 b00");
        // `N`：第二个匹配。
        let (mut store, mut sess) = seeded(&["foo boo"]);
        sess.current = 1;
        step(&mut store, &mut sess, "s/o/0/2", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"fo0 boo");
        // 无匹配："no match"。
        let (mut store, mut sess) = seeded(&["abc"]);
        assert_eq!(
            step(&mut store, &mut sess, "s/z/q/", &mut io).unwrap_err().message,
            "no match"
        );
    }

    /// 裸 `s` 与 `sg`/`sN` 重放上一次替换（C 的 sflags 门：没有上一替换
    /// 即 "no previous substitution"；空模式 `//` 复用上一模式，re.c:66）。
    #[test]
    fn test_substitute_replay_and_pattern_cache() {
        let (mut store, mut sess) = seeded(&["aa", "ab"]);
        let mut io = ScriptIo::new();
        sess.current = 1;
        step(&mut store, &mut sess, "s/a/x/", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..2], b"xa");
        // 裸 `s`：重放（范围仍是当前行）。
        sess.current = 2;
        step(&mut store, &mut sess, "s", &mut io).unwrap();
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..2], b"xb");
        // 空模式 `//` 复用上一模式。
        let (mut store, mut sess) = seeded(&["k1", "k2"]);
        sess.current = 1;
        step(&mut store, &mut sess, "s/k/z/", &mut io).unwrap();
        step(&mut store, &mut sess, "2s//z/", &mut io).unwrap();
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..2], b"z2");
        // 无上一替换即拒。
        let (mut store, mut sess) = seeded(&["x"]);
        assert_eq!(
            step(&mut store, &mut sess, "s", &mut io).unwrap_err().message,
            "no previous substitution"
        );
        // 无上一模式即拒。
        assert_eq!(
            step(&mut store, &mut sess, "s//y/", &mut io).unwrap_err().message,
            "no previous pattern"
        );
    }

    /// 替换模板的 `&` 与分组回放（C 的 regsub 语义，08 篇引擎同源）。
    #[test]
    fn test_substitute_replacement_replay() {
        let (mut store, mut sess) = seeded(&["hello world"]);
        let mut io = ScriptIo::new();
        sess.current = 1;
        // `\+` 是扩展正则的量词（pattern.rs:305 只在 extended 收）；BRE 用
        // 字面组。
        step(&mut store, &mut sess, "s/\\(ll\\)/[&]/", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..13], b"he[ll]o world");
    }

    /// 搜索地址求值：正向、反向、绕圈、`+N` 偏移与 "no match"（C
    /// `get_matching_node_addr` main.c:919-938 的绕行次序）。
    #[test]
    fn test_search_addresses_evaluate() {
        let (mut store, mut sess) = seeded(&["alpha", "beta", "gamma", "beta"]);
        let mut io = ScriptIo::new();
        sess.current = 4;
        // 正向：4 之后没有 beta，绕回到 2。
        step(&mut store, &mut sess, "/beta/", &mut io).unwrap();
        assert_eq!(sess.current, 2, "裸搜索地址显示并把当前行移过去");
        // 反向：从 2 往回是 4。
        sess.current = 2;
        step(&mut store, &mut sess, "?beta?d", &mut io).unwrap();
        assert_eq!(store.line_count(), 3, "反向搜到的行（4）被删除");
        // 偏移跟在搜索基后。
        let (mut store, mut sess) = seeded(&["alpha", "mid", "omega"]);
        sess.current = 1;
        step(&mut store, &mut sess, "/alpha/+1", &mut io).unwrap();
        assert_eq!(sess.current, 2);
        // 无匹配。
        sess.current = 1;
        assert_eq!(
            step(&mut store, &mut sess, "/zebra/", &mut io).unwrap_err().message,
            "no match"
        );
        // 空缓冲无可搜。
        let (mut store, mut sess) = seeded(&[]);
        assert_eq!(
            step(&mut store, &mut sess, "/x/", &mut io).unwrap_err().message,
            "invalid address"
        );
    }

    /// `u` 的撤销与重做（C undo.c:71-105 的回放与快照互换）：`d` 后
    /// `u` 恢复行与当前行，再次 `u` 重做删除；`modified` 在撤销时回脏。
    #[test]
    fn test_undo_restores_delete_then_redoes() {
        let (mut store, mut sess) = seeded(&["alpha", "beta", "gamma"]);
        sess.current = 1;
        step(&mut store, &mut sess, "2,3d", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 1);
        // 撤销：行回来，当前行回到改动前（1）。
        step(&mut store, &mut sess, "u", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 3);
        let mut line = [0u8; 64];
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"beta");
        assert_eq!(sess.current, 1, "当前行取改动前快照");
        assert!(sess.modified, "撤销把缓冲标脏（C undo.c:91）");
        // 重做：再删一次。
        step(&mut store, &mut sess, "u", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 1);
        // 第三次 `u` 再撤销——翻转后的栈继续循环。
        step(&mut store, &mut sess, "u", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 3);
    }

    /// 插入的撤销（`a` 的 UADD 件）与 "nothing to undo" 起手。
    #[test]
    fn test_undo_restores_append_and_reports_empty() {
        let (mut store, mut sess) = seeded(&["one"]);
        sess.current = 1;
        let mut io = io_none();
        assert_eq!(
            step(&mut store, &mut sess, "u", &mut io).unwrap_err().message,
            "nothing to undo",
            "未做任何改动前 `u` 拒绝（C undo.c:76-79）"
        );
        // `a` 插两行，`u` 一次全收掉（每行一条 UADD，回放逆序各删）。
        step(&mut store, &mut sess, "a", &mut io).unwrap();
        step(&mut store, &mut sess, "two", &mut io).unwrap();
        step(&mut store, &mut sess, "three", &mut io).unwrap();
        step(&mut store, &mut sess, ".", &mut io).unwrap();
        assert_eq!(store.line_count(), 3);
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 1, "两行插入被整批撤销");
        // 重做把两行放回。
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 3);
        let mut line = [0u8; 64];
        store.read_line(3, &mut line).unwrap();
        assert_eq!(&line[..5], b"three");
    }

    /// 替换的撤销：原文恢复（C `search_and_replace` 的 UDEL+UADD 对，
    /// sub.c:148-163）。
    #[test]
    fn test_undo_restores_substitute() {
        let (mut store, mut sess) = seeded(&["foo boo", "keep"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "1,2s/o/0/g", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"f00 b00");
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"foo boo");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"keep", "未替换的行不受影响");
        // 新改动清掉旧撤销账（C：每个改动型命令先 clear_undo_stack）。
        step(&mut store, &mut sess, "2d", &mut io).unwrap();
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 2, "撤销指向最近一次改动（d）");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"keep");
    }

    /// `g/pat/cmd`：匹配行逐个执行子命令（C `glbl.c` 的 exec_global）。
    /// 经典三件：删除全部匹配、逐行替换、打印。
    #[test]
    fn test_global_delete_substitute_and_print() {
        // `g/x/d`：删光匹配行。
        let (mut store, mut sess) = seeded(&["x1", "keep", "x2", "also"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "g/x/d", &mut io).unwrap();
        assert_eq!(store.line_count(), 2);
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..4], b"keep");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"also");
        // `v/x/d`：删光不匹配行（反向）。
        let (mut store, mut sess) = seeded(&["x1", "keep", "x2"]);
        sess.current = 1;
        step(&mut store, &mut sess, "v/x/d", &mut io).unwrap();
        assert_eq!(store.line_count(), 2);
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..2], b"x1");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..2], b"x2");
        // `g/o/s/o/0/`：逐行替换（每行首替），后缀 `p` 的打印走子命令
        // 自己的通道。
        let (mut store, mut sess) = seeded(&["foo", "boo"]);
        sess.current = 1;
        step(&mut store, &mut sess, "g/o/s/o/0/", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..3], b"f0o");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..3], b"b0o");
    }

    /// 全局的撤销：整段全局是**一条**撤销单位（C exec_global:143 清栈
    /// 一次，子命令里的清栈被 isglobal 抑制）。
    #[test]
    fn test_global_undo_is_one_unit() {
        let (mut store, mut sess) = seeded(&["x1", "keep", "x2"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "g/x/d", &mut io).unwrap();
        assert_eq!(store.line_count(), 1);
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 3, "一次 `u` 撤销整段全局");
        // 嵌套 `g` 被拒（C main.c:562-564）。
        step(&mut store, &mut sess, "g/x/g/x/d", &mut io).unwrap_err();
        // 错误中止：子命令出错即中止整段（已执行的变更保留，C 同）。
        let (mut store, mut sess) = seeded(&["a1", "a2"]);
        sess.current = 1;
        // `y` 没有对应的命令字母（C 的分派表里无此支）。
        assert_eq!(
            step(&mut store, &mut sess, "g/a/y", &mut io).unwrap_err().message,
            "unknown command"
        );
    }

    /// 空子命令不隐含 `p`（本构建无 BACKWARDS，glbl.c:56-63）：只落
    /// 当前行到最后一个活跃行。
    #[test]
    fn test_global_empty_cmd_moves_current_only() {
        let (mut store, mut sess) = seeded(&["m1", "mid", "m2"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "g/m/", &mut io).unwrap();
        assert_eq!(sess.current, 3, "当前行落到最后一个活跃行");
        assert_eq!(
            io.out_text(),
            "",
            "空子命令不打印（非 BACKWARDS 构建）"
        );
    }

    fn io_none() -> ScriptIo {
        ScriptIo::new()
    }

    /// 读回整个缓冲（测试断言助手）。
    fn buffer_lines<S: TextStore>(store: &S) -> Vec<String> {
        let mut buf = [0u8; MAX_TEXT];
        let mut lines = Vec::new();
        for n in 1..=store.line_count() {
            let used = store.read_line(n, &mut buf).unwrap();
            lines.push(String::from_utf8(buf[..used].to_vec()).unwrap());
        }
        lines
    }

    #[test]
    fn test_interactive_global_edits_each_match_and_replays() {
        let (mut store, mut sess) = seeded(&["one", "two", "three", "four"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["G/o", "s/o/0", "&", ""]);
        // 逐活跃行先显示后执行；`&` 重放上一条；空行跳过（glbl.c:111-134）。
        assert_eq!(io.out_lines(), ["one", "two", "four"]);
        assert_eq!(buffer_lines(&store), ["0ne", "tw0", "three", "four"]);
        assert_eq!(sess.current, 4, "最后一个活跃行落当前行");
    }

    #[test]
    fn test_interactive_global_v_inverse_and_number_suffix() {
        let (mut store, mut sess) = seeded(&["one", "two", "three", "four"]);
        let mut io = ScriptIo::new();
        // `V` 收不匹配行，`n` 尾缀给显示加编号（C main.c:566 的后缀）。
        feed(&mut store, &mut sess, &mut io, &["V/e/n", "", ""]);
        assert_eq!(io.out_lines(), ["2\ttwo", "4\tfour"]);
        assert_eq!(buffer_lines(&store), ["one", "two", "three", "four"], "全跳过：缓冲不动");
    }

    #[test]
    fn test_interactive_global_amp_needs_a_previous_command() {
        let (mut store, mut sess) = seeded(&["a", "b"]);
        let mut io = ScriptIo::new();
        // 起手 `&`：本段还没给过命令（glbl.c:121-125）——整段中止。
        assert_eq!(step(&mut store, &mut sess, "G/a", &mut io).unwrap(), Flow::Continue);
        assert_eq!(
            step(&mut store, &mut sess, "&", &mut io).unwrap_err().message,
            "no previous command"
        );
        assert_eq!(store.line_count(), 2, "中止不动缓冲");
        // seen 每段 G 重新起算：新一段的起手 `&` 同样被拒。
        feed(&mut store, &mut sess, &mut io, &["G/a", "d"]);
        assert_eq!(store.line_count(), 1);
        assert_eq!(step(&mut store, &mut sess, "G/b", &mut io).unwrap(), Flow::Continue);
        assert_eq!(
            step(&mut store, &mut sess, "&", &mut io).unwrap_err().message,
            "no previous command"
        );
    }

    #[test]
    fn test_interactive_global_delete_relocates_and_rejects_nesting() {
        let (mut store, mut sess) = seeded(&["a", "b", "a", "b"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["G/a", "d", "d"]);
        assert_eq!(buffer_lines(&store), ["b", "b"], "两个活跃行都按内容重定位后删除");
        // 回答里的嵌套 g 被拒（C main.c:562-564 的 isglobal 门）。
        assert_eq!(step(&mut store, &mut sess, "G/b", &mut io).unwrap(), Flow::Continue);
        assert_eq!(
            step(&mut store, &mut sess, "g/x/d", &mut io).unwrap_err().message,
            "cannot nest global commands"
        );
        // 中止后回到正常派发：下一行不再是回答。
        let before = io.out_lines();
        assert_eq!(step(&mut store, &mut sess, "1,$p", &mut io).unwrap(), Flow::Continue);
        assert_eq!(io.out_lines()[before.len()..], ["b", "b"]);
    }

    #[test]
    fn test_interactive_global_undo_is_one_unit() {
        let (mut store, mut sess) = seeded(&["one", "two", "four"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["G/o", "s/o/0", "&", "&"]);
        // 清栈一次——整段 G 是一条撤销单位（C exec_global:143）。
        feed(&mut store, &mut sess, &mut io, &["u"]);
        assert_eq!(buffer_lines(&store), ["one", "two", "four"], "一次 u 翻掉整段 G");
        feed(&mut store, &mut sess, &mut io, &["u"]);
        assert_eq!(buffer_lines(&store), ["0ne", "tw0", "f0ur"], "再 u 即重做");
    }

    #[test]
    fn test_interactive_global_unknown_answer_aborts_session() {
        let (mut store, mut sess) = seeded(&["x", "y"]);
        let mut io = ScriptIo::new();
        assert_eq!(step(&mut store, &mut sess, "G/x", &mut io).unwrap(), Flow::Continue);
        // 坏回答（未知命令）中止整段 G；已执行的变更保留（C 同）。
        assert_eq!(
            step(&mut store, &mut sess, "~", &mut io).unwrap_err().message,
            "unknown command"
        );
        assert_eq!(
            step(&mut store, &mut sess, "1,$p", &mut io).unwrap(),
            Flow::Continue,
            "G 中止后输入回到正常派发"
        );
    }

    #[test]
    fn test_declared_gaps_answer_through_the_question_channel() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        assert_eq!(
            step(&mut store, &mut sess, "!ls", &mut io).unwrap_err().message,
            "shell access not wired"
        );
        let mut secure = Session::new(false, true, false, None);
        assert_eq!(
            step(&mut store, &mut secure, "!ls", &mut io).unwrap_err().message,
            "shell access restricted"
        );
        // The non-DES build's crypt answer, verbatim (`main.c:843-845`).
        assert_eq!(
            step(&mut store, &mut sess, "x", &mut io).unwrap_err().message,
            "crypt unavailable"
        );
    }

    #[test]
    fn test_suffix_rules_follow_get_command_suffix() {
        let (mut store, mut sess) = seeded(&["a", "b"]);
        let mut io = ScriptIo::new();
        // A trailing `!` is an invalid suffix everywhere (`main.c:448`).
        assert_eq!(
            step(&mut store, &mut sess, "d!", &mut io).unwrap_err().message,
            "invalid command suffix"
        );
        // `dp` deletes and then prints the new current line
        // (`main.c:228-233`).
        feed(&mut store, &mut sess, &mut io, &["1dp"]);
        assert_eq!(io.out_lines(), ["b"]);
    }

    #[test]
    fn test_percent_and_bare_addresses_navigate() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["3"]);
        assert_eq!(sess.current, 3, "a bare address displays and moves current");
        feed(&mut store, &mut sess, &mut io, &["%d"]);
        assert_eq!(store.line_count(), 0, "`%` names the whole buffer");
    }

    #[test]
    fn test_scroll_walks_a_window() {
        let mut store = GapStore::new();
        let mut sess = Session::new(false, false, false, None);
        for i in 0..30usize {
            store.insert(i + 1, "x").unwrap();
        }
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["z"]);
        assert_eq!(io.out_lines().len(), 23, "second + ROWS lines inclusive (`main.c:859`)");
        assert_eq!(sess.current, 23);
    }

    #[test]
    fn test_double_backend_agreement_through_exec() {
        // The exec layer only speaks `TextStore`, so both backends must
        // produce the same transcript (store.rs `test_stores_agree` at the
        // execution level).
        let script = ["1,2m4", "3d", "1,3j", "1,$n"];
        let mut gaps = GapStore::new();
        for (i, line) in ["a", "b", "c", "d"].iter().enumerate() {
            gaps.insert(i + 1, line).unwrap();
        }
        let mut table = LineTable::new();
        table.load("a\nb\nc\nd\n").unwrap();
        let mut sess_a = Session::new(true, false, false, None);
        let mut sess_b = Session::new(true, false, false, None);
        let mut io_a = ScriptIo::new();
        let mut io_b = ScriptIo::new();
        for line in script {
            step(&mut gaps, &mut sess_a, line, &mut io_a).unwrap();
            step(&mut table, &mut sess_b, line, &mut io_b).unwrap();
        }
        assert_eq!(io_a.out_text(), io_b.out_text());
        assert_eq!(sess_a.current, sess_b.current);
    }
}
