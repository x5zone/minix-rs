//! Assembler symbol extraction vocabulary.
//!
//! Ground truth: `minix3/usr.bin/genassym/genassym.sh`. Two mode flags select
//! the output: `-c` generates a temporary C file (compile it, run the result),
//! `-f` generates Forth code. The remaining arguments form the compiler
//! command. Definition lines follow `name value` shape. The compiler calls
//! stay with the execution layer; this module owns the mode flags, the
//! definition lines, and the symbol table.

use crate::PkgError;

/// Output mode selected by the flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenMode {
    /// Default: plain definitions.
    #[default]
    Plain,
    /// `-c`: C code mode.
    CCode,
    /// `-f`: Forth code mode.
    Forth,
}

/// Parse clustered mode flags (`c`, `f`); empty means plain mode.
pub fn parse_gen_mode(word: &str) -> Result<GenMode, PkgError> {
    if word.is_empty() {
        return Ok(GenMode::Plain);
    }
    let mut mode = GenMode::Plain;
    for byte in word.bytes() {
        match byte {
            b'c' => {
                if mode == GenMode::Forth {
                    return Err(PkgError::InvalidArgument);
                }
                mode = GenMode::CCode;
            }
            b'f' => {
                if mode == GenMode::CCode {
                    return Err(PkgError::InvalidArgument);
                }
                mode = GenMode::Forth;
            }
            _ => return Err(PkgError::InvalidArgument),
        }
    }
    Ok(mode)
}

/// One symbol definition (`name value`, both non empty).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolDef<'a> {
    /// Symbol name.
    pub name: &'a str,
    /// Textual value.
    pub value: &'a str,
}

/// Parse one definition line.
pub fn parse_symbol_line(line: &str) -> Result<SymbolDef<'_>, PkgError> {
    let mut words = line.split_ascii_whitespace();
    let name = words.next().ok_or(PkgError::InvalidArgument)?;
    let value = words.next().ok_or(PkgError::InvalidArgument)?;
    if words.next().is_some() {
        return Err(PkgError::InvalidArgument);
    }
    if !name.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }) || name.is_empty()
    {
        return Err(PkgError::InvalidArgument);
    }
    Ok(SymbolDef { name, value })
}

/// Symbol table behind extraction.
pub trait SymbolTable<'a> {
    /// Value for `name`, or `None` when absent.
    fn get(&self, name: &str) -> Option<&'a str>;
    /// Insert or replace a definition.
    fn insert(&mut self, def: SymbolDef<'a>) -> Result<(), PkgError>;
}

/// Table backed by parallel borrowed slices with room for sixteen symbols.
pub struct SliceSymbols<'a> {
    names: [&'a str; 16],
    values: [&'a str; 16],
    count: usize,
}

impl<'a> SliceSymbols<'a> {
    /// An empty table.
    pub fn new() -> Self {
        SliceSymbols {
            names: [""; 16],
            values: [""; 16],
            count: 0,
        }
    }
}

impl<'a> Default for SliceSymbols<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> SymbolTable<'a> for SliceSymbols<'a> {
    fn get(&self, name: &str) -> Option<&'a str> {
        for index in 0..self.count {
            if self.names[index] == name {
                return Some(self.values[index]);
            }
        }
        None
    }

    fn insert(&mut self, def: SymbolDef<'a>) -> Result<(), PkgError> {
        for index in 0..self.count {
            if self.names[index] == def.name {
                self.values[index] = def.value;
                return Ok(());
            }
        }
        if self.count >= self.names.len() {
            return Err(PkgError::InvalidArgument);
        }
        self.names[self.count] = def.name;
        self.values[self.count] = def.value;
        self.count += 1;
        Ok(())
    }
}

/// Empty table (every lookup misses, every insert is denied).
pub struct EmptySymbols;

impl<'a> SymbolTable<'a> for EmptySymbols {
    fn get(&self, _name: &str) -> Option<&'a str> {
        None
    }

    fn insert(&mut self, _def: SymbolDef<'a>) -> Result<(), PkgError> {
        Err(PkgError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modes_parse() {
        assert_eq!(parse_gen_mode(""), Ok(GenMode::Plain));
        assert_eq!(parse_gen_mode("c"), Ok(GenMode::CCode));
        assert_eq!(parse_gen_mode("f"), Ok(GenMode::Forth));
        assert_eq!(parse_gen_mode("cf"), Err(PkgError::InvalidArgument));
        assert_eq!(parse_gen_mode("x"), Err(PkgError::InvalidArgument));
    }

    #[test]
    fn test_symbol_lines_parse() {
        let def = parse_symbol_line("PAGE_SIZE 4096").unwrap();
        assert_eq!(def.name, "PAGE_SIZE");
        assert_eq!(def.value, "4096");
        assert_eq!(parse_symbol_line("ONLYNAME"), Err(PkgError::InvalidArgument));
        assert_eq!(
            parse_symbol_line("a b c"),
            Err(PkgError::InvalidArgument)
        );
        assert_eq!(parse_symbol_line("has-dash 1"), Err(PkgError::InvalidArgument));
    }

    #[test]
    fn test_slice_table_round_trip() {
        let mut table = SliceSymbols::new();
        table.insert(SymbolDef { name: "A", value: "1" }).unwrap();
        assert_eq!(table.get("A"), Some("1"));
        table.insert(SymbolDef { name: "A", value: "2" }).unwrap();
        assert_eq!(table.get("A"), Some("2"));
        assert_eq!(table.get("B"), None);
    }

    #[test]
    fn test_empty_table_denies() {
        let mut table = EmptySymbols;
        assert_eq!(table.get("A"), None);
        assert_eq!(
            table.insert(SymbolDef { name: "A", value: "1" }),
            Err(PkgError::NotFound)
        );
    }
}
