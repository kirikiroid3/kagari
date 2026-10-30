//! Debug information for a debugger: per bytecode function, the names of its locals, the
//! registers that hold them over code ranges, their source scopes, and a hash of the body.
//!
//! The bytecode's own Debug section (`0x80`) keeps only code-word → source-offset pairs; this
//! is the rest, kept out of the bytecode so that a debug and a release build emit the same
//! `game.kagari`.

use kagari_archive::save::{Scalar, TableMut, TableRef, ValueRef};

use crate::Error;
use crate::codec::{
    as_str, as_table, bool_field, list, opt_id, opt_u32_field, str_field, u32_field,
};

/// The debug information of a module, parallel to its function table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DebugInfo {
    pub functions: Vec<FunctionDebug>,
}

/// The debug information of one bytecode function.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionDebug {
    /// The function's path in the bytecode.
    pub path: String,
    /// The source file of the function, as the bytecode's Debug section names it; `None` for
    /// constructors and builtins.
    pub file: Option<String>,
    /// SHA-256 (lowercase hex) of the function's body as the compiler sees it: unchanged by
    /// edits elsewhere, by comments or by layout.
    pub body_hash: String,
    /// The lexical scopes of the body; a scope's parent comes before it.
    pub scopes: Vec<ScopeDebug>,
    /// The named locals (parameters included), in declaration order.
    pub locals: Vec<LocalDebug>,
}

/// A lexical scope: a source byte range of the function's file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeDebug {
    /// The enclosing scope, an index into [`FunctionDebug::scopes`].
    pub parent: Option<u32>,
    pub start: u32,
    pub end: u32,
}

/// A named local of the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDebug {
    pub name: String,
    /// The scope it is visible in, an index into [`FunctionDebug::scopes`].
    pub scope: u32,
    /// Where it lives; empty when the local never needs a register (it is never read).
    pub ranges: Vec<RegisterRange>,
}

/// A register holding a local over a half-open range of code words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisterRange {
    /// The first code word (an offset into the function's code, as in the Debug section).
    pub start: u32,
    /// The code word after the last one.
    pub end: u32,
    pub register: u32,
    /// The register holds a reference to the local's heap cell rather than its value.
    pub cell: bool,
}

impl DebugInfo {
    pub(crate) fn put(&self, t: &mut TableMut<'_>) -> Result<(), Error> {
        let mut functions = t.array("functions")?;
        for function in &self.functions {
            let mut f = functions.push_table()?;
            f.set("path", Scalar::String(&function.path))?;
            match &function.file {
                Some(file) => f.set("file", Scalar::String(file))?,
                None => f.set("file", Scalar::Null)?,
            }
            f.set("body_hash", Scalar::String(&function.body_hash))?;
            {
                let mut scopes = f.array("scopes")?;
                for scope in &function.scopes {
                    let mut s = scopes.push_table()?;
                    s.set("parent", opt_id(scope.parent))?;
                    s.set("start", Scalar::I64(i64::from(scope.start)))?;
                    s.set("end", Scalar::I64(i64::from(scope.end)))?;
                }
            }
            let mut locals = f.array("locals")?;
            for local in &function.locals {
                let mut l = locals.push_table()?;
                l.set("name", Scalar::String(&local.name))?;
                l.set("scope", Scalar::I64(i64::from(local.scope)))?;
                let mut ranges = l.array("ranges")?;
                for range in &local.ranges {
                    let mut r = ranges.push_table()?;
                    r.set("start", Scalar::I64(i64::from(range.start)))?;
                    r.set("end", Scalar::I64(i64::from(range.end)))?;
                    r.set("register", Scalar::I64(i64::from(range.register)))?;
                    r.set("cell", Scalar::Bool(range.cell))?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn read(t: TableRef<'_>) -> Result<DebugInfo, Error> {
        Ok(DebugInfo {
            functions: list(t, "functions", |v| {
                let f = as_table(v, "functions")?;
                Ok(FunctionDebug {
                    path: str_field(f, "path")?.to_owned(),
                    file: match f.get("file") {
                        None | Some(ValueRef::Null) => None,
                        Some(v) => Some(as_str(v, "file")?.to_owned()),
                    },
                    body_hash: str_field(f, "body_hash")?.to_owned(),
                    scopes: list(f, "scopes", |v| {
                        let s = as_table(v, "scopes")?;
                        Ok(ScopeDebug {
                            parent: opt_u32_field(s, "parent")?,
                            start: u32_field(s, "start")?,
                            end: u32_field(s, "end")?,
                        })
                    })?,
                    locals: list(f, "locals", |v| {
                        let l = as_table(v, "locals")?;
                        Ok(LocalDebug {
                            name: str_field(l, "name")?.to_owned(),
                            scope: u32_field(l, "scope")?,
                            ranges: list(l, "ranges", |v| {
                                let r = as_table(v, "ranges")?;
                                Ok(RegisterRange {
                                    start: u32_field(r, "start")?,
                                    end: u32_field(r, "end")?,
                                    register: u32_field(r, "register")?,
                                    cell: bool_field(r, "cell")?,
                                })
                            })?,
                        })
                    })?,
                })
            })?,
        })
    }
}
