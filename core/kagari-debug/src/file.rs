//! The two kinds of `.kagari-cache` file and their root tables.

use kagari_archive::save::{Save, Scalar, TableMut, TableRef, ValueRef};

use crate::codec::{as_table, bool_field, i64_field, list, str_field, table_field, u32_field};
use crate::debug_info::DebugInfo;
use crate::graph::StoryGraph;
use crate::{Error, SCHEMA};

/// What a cache file holds; its root field `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Analysis,
    Debug,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Analysis => "analysis",
            FileKind::Debug => "debug",
        }
    }
}

/// The story analysis of a program, reused while its sources are unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisCache {
    /// The toolchain version that wrote the file.
    pub toolchain: String,
    /// The program's source fingerprint (lowercase hex SHA-256): every input the analysis
    /// reads. A reader that computes another fingerprint must ignore the file.
    pub fingerprint: String,
    /// The diagnostics of the analysis (compilation stage 5), resolved to source ranges.
    pub diagnostics: Vec<DiagnosticRecord>,
    /// The explored story graph; `None` when the program has no entry scene to explore.
    pub story: Option<StoryGraph>,
}

/// What a debug build writes next to `game.kagari`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugArtifact {
    pub toolchain: String,
    /// The source fingerprint of the program, as in [`AnalysisCache::fingerprint`].
    pub fingerprint: String,
    /// The script hash (lowercase hex SHA-256 of the bytes) of the matching `game.kagari`. A
    /// reader must reject the artifact for any other bytecode.
    pub script_hash: String,
    pub story: Option<StoryGraph>,
    pub debug: DebugInfo,
}

/// A diagnostic with its location resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticRecord {
    pub file: SourceRef,
    /// Byte offsets into the file.
    pub start: u32,
    pub end: u32,
    pub severity: Severity,
    /// The diagnostic code, e.g. `W5009`.
    pub code: String,
    pub message: String,
    pub related: Vec<RelatedRecord>,
}

/// A secondary location of a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedRecord {
    pub file: SourceRef,
    pub start: u32,
    pub end: u32,
    pub message: String,
}

/// A source file: bundled with the toolchain's std, or on disk.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SourceRef {
    /// The path below the std directory, `/`-separated.
    Std(String),
    /// An absolute path.
    Disk(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl AnalysisCache {
    /// The file's bytes: a compacted `.kisave`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut save = Save::new();
        {
            let mut root = save.root_mut();
            header(
                &mut root,
                FileKind::Analysis,
                &self.toolchain,
                &self.fingerprint,
            )?;
            {
                let mut a = root.array("diagnostics")?;
                for diagnostic in &self.diagnostics {
                    let mut d = a.push_table()?;
                    diagnostic.put(&mut d)?;
                }
            }
            put_story(&mut root, self.story.as_ref())?;
        }
        Ok(save.compact().into_bytes())
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<AnalysisCache, Error> {
        let save = Save::from_bytes(bytes)?;
        let root = save.root();
        let (toolchain, fingerprint) = read_header(root, FileKind::Analysis)?;
        Ok(AnalysisCache {
            toolchain,
            fingerprint,
            diagnostics: list(root, "diagnostics", |v| {
                DiagnosticRecord::read(as_table(v, "diagnostics")?)
            })?,
            story: read_story(root)?,
        })
    }
}

impl DebugArtifact {
    /// The file's bytes: a compacted `.kisave`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut save = Save::new();
        {
            let mut root = save.root_mut();
            header(
                &mut root,
                FileKind::Debug,
                &self.toolchain,
                &self.fingerprint,
            )?;
            root.set("script_hash", Scalar::String(&self.script_hash))?;
            put_story(&mut root, self.story.as_ref())?;
            let mut debug = root.table("debug")?;
            self.debug.put(&mut debug)?;
        }
        Ok(save.compact().into_bytes())
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<DebugArtifact, Error> {
        let save = Save::from_bytes(bytes)?;
        let root = save.root();
        let (toolchain, fingerprint) = read_header(root, FileKind::Debug)?;
        Ok(DebugArtifact {
            toolchain,
            fingerprint,
            script_hash: str_field(root, "script_hash")?.to_owned(),
            story: read_story(root)?,
            debug: DebugInfo::read(table_field(root, "debug")?)?,
        })
    }
}

fn header(
    root: &mut TableMut<'_>,
    kind: FileKind,
    toolchain: &str,
    fingerprint: &str,
) -> Result<(), Error> {
    root.set("kind", Scalar::String(kind.as_str()))?;
    root.set("schema", Scalar::I64(SCHEMA))?;
    root.set("toolchain", Scalar::String(toolchain))?;
    root.set("fingerprint", Scalar::String(fingerprint))?;
    Ok(())
}

/// Checks the kind and the schema; returns the toolchain and the fingerprint.
fn read_header(root: TableRef<'_>, kind: FileKind) -> Result<(String, String), Error> {
    let found = str_field(root, "kind")?;
    if found != kind.as_str() {
        return Err(Error::Kind {
            expected: kind,
            found: found.to_owned(),
        });
    }
    let schema = i64_field(root, "schema")?;
    if schema != SCHEMA {
        return Err(Error::Schema(schema));
    }
    Ok((
        str_field(root, "toolchain")?.to_owned(),
        str_field(root, "fingerprint")?.to_owned(),
    ))
}

fn put_story(root: &mut TableMut<'_>, story: Option<&StoryGraph>) -> Result<(), Error> {
    match story {
        Some(story) => story.put(&mut root.table("story")?),
        None => Ok(root.set("story", Scalar::Null)?),
    }
}

fn read_story(root: TableRef<'_>) -> Result<Option<StoryGraph>, Error> {
    match root.get("story") {
        None | Some(ValueRef::Null) => Ok(None),
        Some(v) => StoryGraph::read(as_table(v, "story")?).map(Some),
    }
}

impl SourceRef {
    fn put(&self, t: &mut TableMut<'_>) -> Result<(), Error> {
        let (std, path) = match self {
            SourceRef::Std(path) => (true, path),
            SourceRef::Disk(path) => (false, path),
        };
        t.set("file", Scalar::String(path))?;
        t.set("std", Scalar::Bool(std))?;
        Ok(())
    }

    fn read(t: TableRef<'_>) -> Result<SourceRef, Error> {
        let path = str_field(t, "file")?.to_owned();
        Ok(match bool_field(t, "std")? {
            true => SourceRef::Std(path),
            false => SourceRef::Disk(path),
        })
    }
}

impl DiagnosticRecord {
    fn put(&self, d: &mut TableMut<'_>) -> Result<(), Error> {
        self.file.put(d)?;
        d.set("start", Scalar::I64(i64::from(self.start)))?;
        d.set("end", Scalar::I64(i64::from(self.end)))?;
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        d.set("severity", Scalar::String(severity))?;
        d.set("code", Scalar::String(&self.code))?;
        d.set("message", Scalar::String(&self.message))?;
        let mut related = d.array("related")?;
        for note in &self.related {
            let mut r = related.push_table()?;
            note.file.put(&mut r)?;
            r.set("start", Scalar::I64(i64::from(note.start)))?;
            r.set("end", Scalar::I64(i64::from(note.end)))?;
            r.set("message", Scalar::String(&note.message))?;
        }
        Ok(())
    }

    fn read(d: TableRef<'_>) -> Result<DiagnosticRecord, Error> {
        Ok(DiagnosticRecord {
            file: SourceRef::read(d)?,
            start: u32_field(d, "start")?,
            end: u32_field(d, "end")?,
            severity: match str_field(d, "severity")? {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                _ => return Err(Error::Malformed("severity")),
            },
            code: str_field(d, "code")?.to_owned(),
            message: str_field(d, "message")?.to_owned(),
            related: list(d, "related", |v| {
                let r = as_table(v, "related")?;
                Ok(RelatedRecord {
                    file: SourceRef::read(r)?,
                    start: u32_field(r, "start")?,
                    end: u32_field(r, "end")?,
                    message: str_field(r, "message")?.to_owned(),
                })
            })?,
        })
    }
}
