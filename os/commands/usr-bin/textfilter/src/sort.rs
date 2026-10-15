//! Key parsing and comparison for `sort`.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The C extracts each
//! record's sort keys into transformed key buffers and compares those
//! buffers byte-wise (`fields.c`); the observable semantics — which this
//! module reimplements at the value level — are: key specifications
//! `-k F[.C][flags][,F[.C][flags]]` with per-key flags `b d f i M n r`
//! (unknown flag letters skip silently, `optval`'s default, init.c:229-247;
//! `.0` is rejected as an illegal offset, init.c:168-170), fields
//! separated by blank runs or by the `-t` delimiter byte, default
//! whole-line keys, the modifier transformations dictionary (`d`), fold
//! (`f`), ignore non-printable (`i`), numeric (`n`, blank-skipping sign
//! and fraction aware), month (`M`, `jan` lowest), and reverse (`r`; a
//! local `r` is dropped when a global `-r` stands — init.c:239-242).
//! Ties fall back to the whole line in byte order, and the global `-r`
//! inverts the final outcome.
//!
//! The engine (external merge over temporary files) is the execution
//! layer; this module compares slices, which is what the observable
//! output order is made of. Nothing allocates: modifiers apply as
//! predicates over the existing bytes. The option surface itself needs
//! one growable list (the key specifications), served by `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Blank bytes for field separation: space and tab (the C locale's
/// `<blank>` class).
fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// One parsed `-k` key specification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeySpec {
    /// First field (1 based).
    pub start_field: usize,
    /// First character within the field (1 based; zero means field start).
    pub start_char: usize,
    /// Skip leading blanks at the key start (`b` on the start segment).
    pub blank_start: bool,
    /// Last field (1 based; zero means the last field of the line).
    pub end_field: usize,
    /// Last character within the end field (zero means field end).
    pub end_char: usize,
    /// Skip leading blanks in the end field (`b` on the end segment).
    pub blank_end: bool,
    /// Per-key modifiers.
    pub dictionary: bool,
    pub fold: bool,
    pub ignore_nonprint: bool,
    pub numeric: bool,
    pub month: bool,
    /// This key's comparison inverts (dropped when a global `-r` stands).
    pub reverse: bool,
}

impl KeySpec {
    /// The default whole-line key (no `-k` given).
    pub fn whole_line() -> Self {
        KeySpec {
            start_field: 1,
            start_char: 1,
            blank_start: false,
            end_field: 0,
            end_char: 0,
            blank_end: false,
            dictionary: false,
            fold: false,
            ignore_nonprint: false,
            numeric: false,
            month: false,
            reverse: false,
        }
    }
}

/// Global options for one `sort` run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Key specifications (empty = one whole-line key).
    pub keys: Vec<KeySpec>,
    /// Global reverse.
    pub reverse: bool,
    pub fold: bool,
    pub dictionary: bool,
    pub ignore_nonprint: bool,
    pub numeric: bool,
    pub month: bool,
    /// Leading-blank skip as a global flag.
    pub blank_skip: bool,
    /// Suppress lines that compare equal to their predecessor.
    pub unique: bool,
    /// Check only: report disorder and duplicates, sort nothing.
    pub check: bool,
    /// Field delimiter byte (`-t`).
    pub delimiter: Option<u8>,
    /// Merge already-sorted runs instead of sorting (`-m`, C `sort -m`).
    pub merge: bool,
    /// `-o path`: where the result goes. `None` is C's null `outpath`, which writes
    /// to standard output (`sort.c:320-323`); `Some("")` is *not* the same thing —
    /// C carries the empty string into `lstat`/`fopen` and fails there, so the
    /// option has to be distinguishable from its absence. The path also decides
    /// *how* it is written, see [`decide_output`], because C replaces an existing
    /// file through a temporary rather than truncating it in place.
    pub output: Option<Vec<u8>>,
}

/// Merge several already-sorted runs into one sorted vector, the way C's
/// `sort -m` concatenates sorted inputs by merging rather than re-sorting. Each
/// run is assumed already ordered under `options`; equal keys keep the earlier
/// run's line first (a stable, left-biased merge). Because nothing is
/// re-sorted, `-m` on runs that are not each sorted yields a merge of the runs,
/// exactly as the C does.
pub fn merge_sorted(runs: &[&[String]], options: &Options) -> Vec<String> {
    let mut acc: Vec<String> = Vec::new();
    for run in runs {
        acc = merge_two(&acc, run, options);
    }
    acc
}

fn merge_two(a: &[String], b: &[String], options: &Options) -> Vec<String> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if compare(&b[j], &a[i], options) == core::cmp::Ordering::Less {
            out.push(b[j].clone());
            j += 1;
        } else {
            out.push(a[i].clone());
            i += 1;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

/// 一条不可解析的选项参数错。三条出口对位 C 的三条真出路（见各变体注释），
/// 不是一句「bad option」到底。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// libc 的 `getopt` 自己那一行（集外字母或带值选项缺值），随后才是 `sort.c:252`
    /// 的 `usage(NULL)`——所以真源是两行。两类各自带字母。
    Getopt(crate::GetoptReport),
    /// `-k` 的键形不对：`setfield` 里的 `errx(2, "illegal field descriptor")`
    /// （`minix3/usr.bin/sort/init.c:189-190`）。这一支**不接 usage**。
    IllegalFieldDescriptor,
    /// `-t` 给了第二次：`sort.c:214-215` 的 `usage("multiple field delimiters")`，
    /// 即先一句带程名的话，再接整个 usage 块。
    MultipleFieldDelimiters,
}

/// Parse global options from the argument words, returning the options
/// plus the remaining operand words (file paths in the C; the doing half
/// rejects them while the open-existing call is gated). `-k` and `-t`
/// take a value, given separately or inline; 集外字母与缺值各自成一个
/// [`ParseError::Getopt`]，键形不对与 `-t` 给第二次则是另两条出口。
///
/// 本解析器已建模的带值字母是 `k`、`o`、`t`：C 的选项串是 `sort.c:148` 的
/// `#define SORT_OPTS "bcdD:fHik:lmno:rR:sSt:T:ux"`，其中 `:` 标出的是 `D`、`k`、`o`、
/// `R`、`t`、`T` 六个；但 `D`/`R`/`T`（以及旗标 `l`/`S`/`x`）在本树尚未建模，它们现在
/// 落进「集外字母」那一支——那是**选项面**的缺口（另案登记）。分成两条 libc 报文
/// （集外字母走 `getopt.c:101-104`，缺值走 `:132-136`）的做法就体现在下面各臂里。
pub fn parse_options<'a>(args: &[&'a str]) -> Result<(Options, Vec<&'a str>), ParseError> {
    let mut options = Options::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if !arg.starts_with('-') || arg.len() <= 1 {
            operands.push(arg);
            index += 1;
            continue;
        }
        let mut chars = arg[1..].chars().peekable();
        while let Some(flag) = chars.next() {
            match flag {
                'r' => options.reverse = true,
                'f' => options.fold = true,
                'd' => options.dictionary = true,
                'i' => options.ignore_nonprint = true,
                'n' => options.numeric = true,
                'M' => options.month = true,
                'b' => options.blank_skip = true,
                'u' => options.unique = true,
                'c' => options.check = true,
                'm' => options.merge = true,
                // `-H`: a documented no-op in C — `sort.c:174-176` comments that
                // `-H` "was; use merge sort for blocks of large files — that is
                // now the default", so it accepts the flag and does nothing. We
                // match that: accept, no effect (previously it errored as bad).
                'H' => {}
                // `-s`: stable sort. C `sort.c:217-222` sets `posix_sort = 0`,
                // which its own comment notes is already the NetBSD default, and
                // `slice::sort_by` (used below) is stable — so it is a no-op here
                // too, matching C's observable behavior.
                's' => {}
                // `-o` takes a value, inline (`-oNAME`) or as the next word
                // (`sort.c:188-190`); a missing one is C's `?` and lands on the
                // usage path. A later `-o` overwrites the earlier one, as C's
                // `outpath = optarg` assignment does.
                'o' => {
                    let mut value: String = chars.by_ref().collect();
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = (*word).to_string(),
                            None => {
                                return Err(ParseError::Getopt(crate::GetoptReport::MissingValue(
                                    flag,
                                )));
                            }
                        }
                    }
                    options.output = Some(value.into_bytes());
                    break;
                }
                'k' | 't' => {
                    let mut value: String = chars.by_ref().collect();
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = (*word).to_string(),
                            None => {
                                return Err(ParseError::Getopt(crate::GetoptReport::MissingValue(
                                    flag,
                                )));
                            }
                        }
                    }
                    if flag == 'k' {
                        match parse_key(&value, &options) {
                            // C 这一支不写 usage：`setfield` 里是 `errx(2, "illegal
                            // field descriptor")`（`init.c:189-190`）。
                            Some(spec) => options.keys.push(spec),
                            None => return Err(ParseError::IllegalFieldDescriptor),
                        }
                    } else {
                        // `-t` 在 C 里只取首字节（`sort.c:219` 的 `d_mask[(u_char)*optarg]`），
                        // 从不查长度；但同一个选项给第二次就是
                        // `usage("multiple field delimiters")`（`:214-215`）。
                        if options.delimiter.is_some() {
                            return Err(ParseError::MultipleFieldDelimiters);
                        }
                        // 空值（`-t ''`）在 C 取到那个 `\0`，不报错。
                        options.delimiter = Some(value.bytes().next().unwrap_or(0));
                    }
                    break;
                }
                _ => {
                    return Err(ParseError::Getopt(crate::GetoptReport::UnknownOption(flag)));
                }
            }
        }
        index += 1;
    }
    Ok((options, operands))
}

/// What a `lstat` of the requested output path tells the C, narrowed to the two
/// questions its branch asks (`sort.c:324-326`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputProbe {
    /// `st_mode & ALLPERMS`, the permission bits C copies onto the temporary before
    /// it becomes the output name (`sort.c:366`, `ALLPERMS` = `0o7777`).
    pub mode: u32,
    /// `S_ISCHR(st.st_mode) || S_ISBLK(st.st_mode)`: writing straight through is
    /// correct for a device, and C says so out loud.
    pub is_device: bool,
}

/// Where the sorted text goes, the three ways C reads `sort.c:320-355`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// No `-o`: `stdout`, and no path is ever touched.
    StandardOutput,
    /// Open the named path for writing and truncate it — the branch C takes when
    /// `lstat` fails (no such file) or reports a character/block special
    /// (`sort.c:352-355`).
    Direct { path: Vec<u8> },
    /// Write `<path>XXXXXX`, then copy the probed mode onto it, unlink the original
    /// name, hard-link the temporary to it, and unlink the temporary
    /// (`sort.c:327-379`). A crash mid-write therefore leaves the old file standing.
    ViaTemp { path: Vec<u8>, mode: u32 },
}

/// C's choice between those three, as a function of the option and the probe. An
/// absent `-o` is the only way to reach standard output; an empty one still names a
/// path, and C's `fopen("", "w")` fails on it (`sort.c:354-355`).
pub fn decide_output(output: Option<&[u8]>, probe: Option<OutputProbe>) -> Output {
    let Some(output) = output else {
        return Output::StandardOutput;
    };
    let path = output.to_vec();
    match probe {
        Some(found) if !found.is_device => Output::ViaTemp {
            path,
            mode: found.mode,
        },
        _ => Output::Direct { path },
    }
}

/// The six characters that replace the `XXXXXX` of `sort.c:338-339`. C's `mkstemp`
/// fills them from `padchar` — sixty-two characters in the order digits, uppercase,
/// lowercase (`lib/libc/stdio/gettemp.c:49-50`) — drawing one at random per slot, and
/// then carries the name forward while a create keeps failing (`:120-170`). This
/// shell takes the same alphabet and walks it the same way from a per-process seed
/// instead of a random one; all the caller relies on is that the name keeps the
/// requested prefix, stays six characters long, and differs between attempts.
pub fn temp_candidate(path: &[u8], seed: u64, attempt: usize) -> Vec<u8> {
    const PAD_CHAR: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut value = seed.wrapping_add(attempt as u64);
    let mut suffix = [0u8; 6];
    for slot in suffix.iter_mut().rev() {
        *slot = PAD_CHAR[(value % PAD_CHAR.len() as u64) as usize];
        value /= PAD_CHAR.len() as u64;
    }
    let mut out = path.to_vec();
    out.extend_from_slice(&suffix);
    out
}

/// Parses one `-k` specification. Modifiers ride the start segment, the
/// end segment, or both; unknown flag letters are skipped silently. A
/// `.0` character offset is rejected (the C's "illegal offset").
pub fn parse_key(spec: &str, global: &Options) -> Option<KeySpec> {
    // C 的形状（`init.c:181-192` 的 `setfield`）：先解开始列，然后看它停在哪个字符上——
    // 到结尾就是「键延伸到行尾」；不是逗号就是 `errx(2, "illegal field descriptor")`。
    // 旧写法把整串都吃下去（未知字母按 `optval` 的默认略过），所以 `-k x`、`-k 1x`
    // 这些在 C 会报错的形在这里被静默接受。
    let (start_field, start_char, start_flags, blank_start, rest) = parse_column(spec)?;
    let end_text = if rest.is_empty() {
        None
    } else if let Some(tail) = rest.strip_prefix(',') {
        Some(tail)
    } else {
        return None;
    };
    let mut key = KeySpec {
        start_field: if start_field == 0 { 1 } else { start_field },
        start_char,
        blank_start: blank_start || global.blank_skip,
        end_field: 0,
        end_char: 0,
        blank_end: false,
        dictionary: start_flags.contains('d') || global.dictionary,
        fold: start_flags.contains('f') || global.fold,
        ignore_nonprint: start_flags.contains('i') || global.ignore_nonprint,
        numeric: start_flags.contains('n') || global.numeric,
        month: start_flags.contains('M') || global.month,
        reverse: start_flags.contains('r') && !global.reverse,
    };
    if let Some(end_text) = end_text {
        // 结束列之后的残留 C 不再查（`setfield` 里第二个 `setcolumn` 后没有任何
        // `*pos` 判据），所以这里把 `rest` 丢掉是有依据的，不是漏写。
        let (end_field, end_char, end_flags, blank_end, _rest) = parse_column(end_text)?;
        key.end_field = end_field;
        key.end_char = end_char;
        key.blank_end = blank_end || global.blank_skip;
        key.dictionary |= end_flags.contains('d');
        key.fold |= end_flags.contains('f');
        key.ignore_nonprint |= end_flags.contains('i');
        key.numeric |= end_flags.contains('n');
        key.month |= end_flags.contains('M');
        key.reverse |= end_flags.contains('r') && !global.reverse;
    }
    if global.reverse {
        // init.c:239-242 — a local `r` does not invert a global `-r`.
        key.reverse = false;
    }
    Some(key)
}

/// Parses `F[.C]` plus trailing modifier flags. Returns the field, the
/// character (`.C`; absent and `.1` are both offset zero, `.0` is an
/// error), the modifier letters, whether a `b` modifier was present, and
/// **where it stopped**（`setcolumn` 返回的 `pos`，`init.c:191`/`:205`）。
fn parse_column(text: &str) -> Option<(usize, usize, String, bool, &str)> {
    let bytes = text.as_bytes();
    let mut position = 0;
    let mut field = 0usize;
    while position < bytes.len() && bytes[position].is_ascii_digit() {
        field = field * 10 + (bytes[position] - b'0') as usize;
        position += 1;
    }
    let mut character_given = false;
    let mut character = 0usize;
    if position < bytes.len() && bytes[position] == b'.' {
        position += 1;
        character_given = true;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            character = character * 10 + (bytes[position] - b'0') as usize;
            position += 1;
        }
        if character == 0 {
            // The C's indent decrement turns `.0` negative: illegal offset.
            return None;
        }
    }
    let mut flags = String::new();
    let mut blank = false;
    while position < bytes.len() {
        match bytes[position] {
            b'b' => blank = true,
            b'd' | b'f' | b'i' | b'n' | b'M' | b'r' => flags.push(bytes[position] as char),
            // C 的 `for (; (tmp = optval(*pos, …)); pos++)`（`init.c:204`）在**第一个
            // 不是修饰字母的字符上停下**，那个字符就成了 `setfield` 眼里的 `*pos`；
            // 它不是「略过继续」。旧写法把每个未知字符都跳掉，于是 `-k x`、`-k 1 2`
            // 这些在 C 报 `illegal field descriptor` 的形被静默接受。
            _ => break,
        }
        position += 1;
    }
    let _ = character_given;
    Some((field, character, flags, blank, &text[position..]))
}

/// The span of the `field`-th field of a line (1 based), byte offsets
/// exclusive at the end.
///
/// With a delimiter the pieces split on every delimiter byte (empty
/// pieces are real fields). Without one, fields are the maximal
/// non-blank runs; the first field starts at byte zero so leading blanks
/// ride with it (the reason plain `sort` often needs `-b`), and a field
/// beyond the last run is the empty span at end of line.
fn field_span(line: &[u8], field: usize, delimiter: Option<u8>) -> (usize, usize) {
    match delimiter {
        Some(delimiter) => {
            let mut start = 0usize;
            let mut index = 1usize;
            let mut cursor = 0usize;
            while cursor < line.len() {
                if line[cursor] == delimiter {
                    if index == field {
                        return (start, cursor);
                    }
                    index += 1;
                    start = cursor + 1;
                }
                cursor += 1;
            }
            if index == field {
                (start, line.len())
            } else {
                (line.len(), line.len())
            }
        }
        None => {
            if field == 1 {
                let mut cursor = 0usize;
                while cursor < line.len() && is_blank(line[cursor]) {
                    cursor += 1;
                }
                while cursor < line.len() && !is_blank(line[cursor]) {
                    cursor += 1;
                }
                return (0, cursor);
            }
            // Field `field` is the (`field` - 1)th non-blank run,
            // 0 based (field 1 is handled above with its leading blanks).
            let target = field - 1;
            let mut cursor = 0usize;
            let mut run = 0usize;
            while cursor < line.len() {
                while cursor < line.len() && is_blank(line[cursor]) {
                    cursor += 1;
                }
                if cursor >= line.len() {
                    break;
                }
                let start = cursor;
                while cursor < line.len() && !is_blank(line[cursor]) {
                    cursor += 1;
                }
                if run == target {
                    return (start, cursor);
                }
                run += 1;
            }
            (line.len(), line.len())
        }
    }
}

/// Extracts one key's bytes from a line per the specification.
fn key_bytes<'a>(line: &'a str, key: &KeySpec, delimiter: Option<u8>) -> &'a str {
    let bytes = line.as_bytes();
    let (start_field_start, start_field_end) = field_span(bytes, key.start_field, delimiter);
    let mut start = start_field_start;
    if key.blank_start {
        while start < start_field_end && is_blank(bytes[start]) {
            start += 1;
        }
    }
    start = start
        .saturating_add(key.start_char.saturating_sub(1))
        .min(line.len());

    if key.end_field == 0 {
        return &line[start..];
    }
    let (end_field_start, end_field_end) = field_span(bytes, key.end_field, delimiter);
    let mut base = end_field_start;
    if key.blank_end {
        while base < end_field_end && is_blank(bytes[base]) {
            base += 1;
        }
    }
    let mut end = if key.end_char > 0 {
        (base + key.end_char).min(end_field_end)
    } else {
        end_field_end
    };
    if end < start {
        end = start;
    }
    &line[start..end]
}

/// A byte passes this key's `d`/`i` filters; `f` folds through the
/// mapping below.
fn visible_and_folded(byte: u8, key: &KeySpec) -> Option<u8> {
    if key.dictionary && !(byte.is_ascii_alphanumeric() || is_blank(byte)) {
        return None;
    }
    if key.ignore_nonprint && !(0x20..=0x7E).contains(&byte) {
        return None;
    }
    Some(if key.fold {
        byte.to_ascii_uppercase()
    } else {
        byte
    })
}

/// Compares two key slices through the `d`/`f`/`i` filters without
/// materialising them: both streams are filtered lazily and compared in
/// lockstep, then by visible length.
fn cmp_filtered(a: &[u8], b: &[u8], key: &KeySpec) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let mut ai = a.iter().filter_map(|b| visible_and_folded(*b, key));
    let mut bi = b.iter().filter_map(|b| visible_and_folded(*b, key));
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
            }
        }
    }
}

/// Month names, `jan` lowest through `dec`; anything shorter than three
/// bytes or outside the names is zero (blank and unknown fields sort
/// first, per the C month table).
fn month_of(key_bytes: &[u8]) -> u32 {
    const MONTHS: [&[u8]; 12] = [
        b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep", b"oct", b"nov",
        b"dec",
    ];
    if key_bytes.len() < 3 {
        return 0;
    }
    for (index, name) in MONTHS.iter().enumerate() {
        if key_bytes[..3].eq_ignore_ascii_case(name) {
            return index as u32 + 1;
        }
    }
    0
}

/// A `-n` numeric key parsed in place: blank-skipping optional sign,
/// digit run, optional fraction; an empty parse is zero.
#[derive(Debug, PartialEq, Eq)]
struct NumericKey<'a> {
    negative: bool,
    integer: &'a [u8],
    fraction: &'a [u8],
}

fn numeric_key(bytes: &[u8]) -> NumericKey<'_> {
    let mut cursor = 0usize;
    while cursor < bytes.len() && is_blank(bytes[cursor]) {
        cursor += 1;
    }
    let negative = bytes.get(cursor) == Some(&b'-');
    if negative {
        cursor += 1;
    }
    let integer_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    let integer = &bytes[integer_start..cursor];
    let fraction_start = cursor + 1;
    if cursor < bytes.len() && bytes[cursor] == b'.' {
        cursor += 1;
        let fraction = &bytes[fraction_start
            ..cursor
                + bytes[fraction_start..]
                    .iter()
                    .take_while(|b| b.is_ascii_digit())
                    .count()];
        return NumericKey {
            negative,
            integer,
            fraction,
        };
    }
    NumericKey {
        negative,
        integer,
        fraction: &[],
    }
}

fn trim_leading_zeros(mut digits: &[u8]) -> &[u8] {
    while digits.len() > 1 && digits[0] == b'0' {
        digits = &digits[1..];
    }
    digits
}

fn is_numeric_zero(key: &NumericKey<'_>) -> bool {
    trim_leading_zeros(key.integer) == b"0" && {
        let mut fraction = key.fraction;
        while fraction.last() == Some(&b'0') {
            fraction = &fraction[..fraction.len() - 1];
        }
        fraction.is_empty()
    }
}

fn compare_numeric(a: &NumericKey<'_>, b: &NumericKey<'_>) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    if is_numeric_zero(a) && is_numeric_zero(b) {
        return Ordering::Equal;
    }
    match (a.negative, b.negative) {
        (false, true) => return Ordering::Greater,
        (true, false) => return Ordering::Less,
        _ => {}
    }
    let magnitude = {
        let ai = trim_leading_zeros(a.integer);
        let bi = trim_leading_zeros(b.integer);
        let by_length = ai.len().cmp(&bi.len());
        let by_lex = ai.cmp(bi);
        if by_length != Ordering::Equal {
            by_length
        } else {
            by_lex
        }
    };
    let magnitude = match magnitude {
        Ordering::Equal => {
            let mut longer = a.fraction.len().max(b.fraction.len());
            let mut index = 0usize;
            let mut ordering = Ordering::Equal;
            while index < longer {
                let x = a.fraction.get(index).copied().unwrap_or(b'0');
                let y = b.fraction.get(index).copied().unwrap_or(b'0');
                if x != y {
                    ordering = x.cmp(&y);
                    break;
                }
                index += 1;
            }
            longer = 0;
            let _ = longer;
            ordering
        }
        other => other,
    };
    if a.negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Compares two lines per the options: each key spec contributes a
/// comparison, ties fall through to the next key and then to the whole
/// line in byte order, and the global `-r` inverts the outcome.
pub fn compare(a: &str, b: &str, options: &Options) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    // Without `-k` the global modifiers apply to the whole line: one
    // whole-line key carrying them (and the global reverse) is built
    // here.
    let owned_key;
    let keys: &[KeySpec] = if options.keys.is_empty() {
        owned_key = KeySpec {
            fold: options.fold,
            dictionary: options.dictionary,
            ignore_nonprint: options.ignore_nonprint,
            numeric: options.numeric,
            month: options.month,
            blank_start: options.blank_skip,
            ..KeySpec::whole_line()
        };
        alloc::slice::from_ref(&owned_key)
    } else {
        &options.keys[..]
    };
    let mut result = Ordering::Equal;
    for key in keys {
        let ka = key_bytes(a, key, options.delimiter);
        let kb = key_bytes(b, key, options.delimiter);
        let ordering = if key.numeric {
            compare_numeric(&numeric_key(ka.as_bytes()), &numeric_key(kb.as_bytes()))
        } else if key.month {
            month_of(ka.as_bytes()).cmp(&month_of(kb.as_bytes()))
        } else {
            cmp_filtered(ka.as_bytes(), kb.as_bytes(), key)
        };
        let ordering = if key.reverse {
            ordering.reverse()
        } else {
            ordering
        };
        if ordering != Ordering::Equal {
            result = ordering;
            break;
        }
    }
    if result == Ordering::Equal {
        // The C's last-resort comparison: the raw whole lines in byte
        // order, with the global reverse applied (the modifiers belong
        // to the keys, not to this fallback).
        let raw = a.as_bytes().cmp(b.as_bytes());
        return if options.reverse { raw.reverse() } else { raw };
    }
    if options.reverse {
        result.reverse()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    fn opts(extra: &str) -> Options {
        let mut words: Vec<&str> = Vec::new();
        for flag in extra.split_whitespace() {
            words.push(flag);
        }
        match parse_options(&words) {
            Ok((options, _operands)) => options,
            Err(_) => panic!("options should parse"),
        }
    }

    #[test]
    fn test_plain_byte_order() {
        let options = opts("");
        assert_eq!(compare("apple", "banana", &options), Ordering::Less);
        assert_eq!(compare("ab", "ab", &options), Ordering::Equal);
    }

    #[test]
    fn test_reverse_inverts() {
        let options = opts("-r");
        assert_eq!(compare("apple", "banana", &options), Ordering::Greater);
    }

    #[test]
    fn test_numeric_with_signs_and_fractions() {
        let options = opts("-n");
        assert_eq!(compare("  -3.5", "2", &options), Ordering::Less);
        assert_eq!(compare("2.10", "2.5", &options), Ordering::Less);
        // POSIX -n has no plus sign: "+7" is not a number and sorts as
        // zero, equal to "007"; the whole-line fallback then separates
        // them by raw bytes ("+" sorts before "0").
        assert_eq!(compare("+7", "007", &options), Ordering::Less);
        // Non-numeric prefixes compare as zero; the whole-line fallback
        // then orders them by raw bytes.
        assert_eq!(compare("abc", "  ", &options), Ordering::Greater);
    }

    #[test]
    fn test_fold_and_dictionary() {
        // The fold makes the keys equal; the C's last-resort comparison
        // then falls back to the raw whole line, where 'A' < 'a'.
        let options = opts("-f");
        assert_eq!(compare("Apple", "apple", &options), Ordering::Less);
        assert_eq!(compare("Apple", "Apple", &options), Ordering::Equal);
        let dictionary = opts("-d");
        // The '+' byte filters away on both sides: "ab" versus "abc".
        assert_eq!(compare("a+b", "a+bc", &dictionary), Ordering::Less);
    }

    #[test]
    fn test_month_table() {
        let options = opts("-M");
        assert_eq!(compare("jan 1", "feb 1", &options), Ordering::Less);
        assert_eq!(compare("DEC", "jan", &options), Ordering::Greater);
        // Blank or unknown months sort first.
        assert_eq!(compare("???", "jan", &options), Ordering::Less);
    }

    #[test]
    fn test_key_spec_field_selection_with_delimiter() {
        let global = opts("-t:");
        let spec = parse_key("2,2", &global).unwrap();
        let mut options = opts("-t:");
        options.keys.clear();
        options.keys.push(spec);
        // -t: -k2,2 compares the second colon field only; different keys
        // order by it, and the whole line breaks the tie.
        assert_eq!(compare("x:b:zz", "a:a:yy", &options), Ordering::Greater);
        assert_eq!(compare("x:b:zz", "a:b:yy", &options), Ordering::Greater);
        assert_eq!(compare("x:b:1", "x:b:2", &options), Ordering::Less);
    }

    #[test]
    fn test_key_spec_dot_zero_rejected() {
        assert!(parse_key("1.0", &opts("")).is_none());
        assert!(parse_key("1.2", &opts("")).is_some());
    }

    #[test]
    fn test_local_reverse_drops_under_global() {
        // init.c:239-242: a local `r` does not invert a global `-r`; the
        // key parses without reversal.
        let global = opts("-r");
        let key = parse_key("1r", &global).unwrap();
        assert!(!key.reverse);
    }

    #[test]
    fn test_fields_split_on_blank_runs() {
        let global = opts("");
        let spec = parse_key("2,2", &global).unwrap();
        let mut options = opts("");
        options.keys.clear();
        options.keys.push(spec);
        assert_eq!(compare("aa b cc", "xx y zz", &options), Ordering::Less);
        assert_eq!(compare("a y z", "b x w", &options), Ordering::Greater);
    }

    #[test]
    fn test_tie_falls_back_to_whole_line() {
        let global = opts("");
        let spec = parse_key("1,1.1", &global).unwrap();
        let mut options = opts("");
        options.keys.clear();
        options.keys.push(spec);
        // The one-byte key ties; the whole line breaks the tie.
        assert_eq!(compare("a123", "a456", &options), Ordering::Less);
    }

    #[test]
    fn test_parse_options_takes_the_output_path() {
        // `-o` takes its value inline or as the next word, and a later one replaces
        // the earlier, because C's `case 'o'` is a plain `outpath = optarg`
        // assignment (`sort.c:188-190`). Without the option the field stays `None`,
        // which is what distinguishes standard output from `-o ""`.
        let (options, operands) = parse_options(&["-o", "out.txt", "in.txt"]).unwrap();
        assert_eq!(options.output.as_deref(), Some("out.txt".as_bytes()));
        assert_eq!(operands, vec!["in.txt"]);
        let (inline, _) = parse_options(&["-oout", "in"]).unwrap();
        assert_eq!(inline.output.as_deref(), Some("out".as_bytes()));
        let (replaced, _) = parse_options(&["-oa", "-ob"]).unwrap();
        assert_eq!(replaced.output.as_deref(), Some("b".as_bytes()));
        let (none, _) = parse_options(&["-n"]).unwrap();
        assert_eq!(none.output, None);
        assert_eq!(
            parse_options(&["-o"]),
            Err(ParseError::Getopt(crate::GetoptReport::MissingValue('o')))
        );
    }

    /// 三条出口各自一个型，而且只有两条前面带 libc 那一行（壳根据型选行数）。
    /// 集外字母那一支另有一枚测（`test_parse_options_rejects_unknown`），不在此重复。
    #[test]
    fn test_parse_options_splits_the_three_error_paths() {
        use crate::GetoptReport;
        // 缺值：与集外字母同属 `Getopt`，但句子不同。
        assert_eq!(
            parse_options(&["-t"]),
            Err(ParseError::Getopt(GetoptReport::MissingValue('t')))
        );
        // 键形不对：C 是 `errx(2, "illegal field descriptor")`，不接 usage。
        assert_eq!(
            parse_options(&["-k", "x"]),
            Err(ParseError::IllegalFieldDescriptor)
        );
        assert_eq!(
            parse_options(&["-k", "1 2"]),
            Err(ParseError::IllegalFieldDescriptor)
        );
        // `-t` 给第二次才是 `usage("multiple field delimiters")`；
        // 而多字节本身**不是**错（C 只取首字节）。
        let (one, _) = parse_options(&["-t,", "f"]).unwrap();
        assert_eq!(one.delimiter, Some(b','));
        let (two, _) = parse_options(&["-t::", "f"]).unwrap();
        assert_eq!(two.delimiter, Some(b':'), "多字节取首字节，不报错");
        assert_eq!(
            parse_options(&["-t,", "-t;"]).err(),
            Some(ParseError::MultipleFieldDelimiters)
        );
        // 空值取到那个 `\0`（C 的 `*optarg`）。
        let (nul, _) = parse_options(&["-t", "", "f"]).unwrap();
        assert_eq!(nul.delimiter, Some(0));
    }

    #[test]
    fn test_output_target_follows_what_lstat_said() {
        // The three answers of `sort.c:320-355`, and the fourth row that keeps
        // `-o ""` distinct from no option at all: an empty path is still a path, so
        // it goes to the direct branch and the open fails there.
        assert_eq!(decide_output(None, None), Output::StandardOutput);
        assert_eq!(
            decide_output(Some(b"new"), None),
            Output::Direct {
                path: b"new".to_vec()
            }
        );
        assert_eq!(
            decide_output(Some(b""), None),
            Output::Direct { path: Vec::new() }
        );
        assert_eq!(
            decide_output(
                Some(b"dev"),
                Some(OutputProbe {
                    mode: 0o666,
                    is_device: true
                }),
            ),
            Output::Direct {
                path: b"dev".to_vec()
            }
        );
        assert_eq!(
            decide_output(
                Some(b"old"),
                Some(OutputProbe {
                    mode: 0o640,
                    is_device: false
                }),
            ),
            Output::ViaTemp {
                path: b"old".to_vec(),
                mode: 0o640
            }
        );
    }

    #[test]
    fn test_temporary_names_keep_the_prefix_and_walk_forward() {
        // C writes `%sXXXXXX` and lets `mkstemp` fill the six slots from a
        // sixty-two-character alphabet (`gettemp.c:49-50`), carrying forward while a
        // create collides. What the caller relies on is the prefix, the length, and
        // that consecutive attempts differ.
        let first = temp_candidate(b"out.txt", 0x1234, 0);
        let second = temp_candidate(b"out.txt", 0x1234, 1);
        assert_eq!(first.len(), b"out.txt".len() + 6);
        assert!(first.starts_with(b"out.txt"));
        assert_ne!(first, second, "consecutive attempts must differ");
        for byte in &first[b"out.txt".len()..] {
            assert!(
                b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".contains(byte),
                "suffix byte {byte:#04x} is outside C's padchar set"
            );
        }
    }

    #[test]
    fn test_parse_options_rejects_unknown() {
        assert_eq!(
            parse_options(&["-Z"]),
            Err(ParseError::Getopt(crate::GetoptReport::UnknownOption('Z')))
        );
    }

    #[test]
    fn test_parse_options_accepts_merge() {
        let (options, operands) = parse_options(&["-m", "a.txt"]).unwrap();
        assert!(options.merge);
        assert_eq!(operands, ["a.txt"]);
    }

    #[test]
    fn test_parse_options_accepts_h_and_s_as_noops() {
        // C accepts `-H` ("now the default") and `-s` (stable = default) with no
        // observable effect; they must parse cleanly (not BadOption) and change
        // no flag — the previous behavior errored them, diverging from C.
        let (options, operands) = parse_options(&["-H", "-s", "data.txt"]).unwrap();
        assert!(!options.merge);
        assert!(!options.unique);
        assert!(!options.check);
        assert!(!options.numeric);
        assert_eq!(operands, ["data.txt"]);
        // Clustered in one argument too (getopt treats each letter separately).
        let (clustered, _) = parse_options(&["-sH"]).unwrap();
        assert!(!clustered.numeric);
    }

    #[test]
    fn test_merge_sorted_two_runs_interleave() {
        let options = Options::default();
        let a = ["a1".to_string(), "a3".to_string(), "a5".to_string()];
        let b = ["a2".to_string(), "a4".to_string()];
        let merged = merge_sorted(&[&a, &b], &options);
        assert_eq!(
            merged,
            ["a1", "a2", "a3", "a4", "a5"].map(String::from).to_vec()
        );
    }

    #[test]
    fn test_merge_sorted_equal_keys_keep_earlier_run_first() {
        let options = Options::default();
        let a = ["x".to_string(), "y".to_string()];
        let b = ["x".to_string()];
        let merged = merge_sorted(&[&a, &b], &options);
        // run a's "x" precedes run b's "x": a stable, left-biased merge.
        assert_eq!(merged, ["x", "x", "y"].map(String::from).to_vec());
    }
}
