//! Diagnostics produced by every stage, and their text rendering.

use std::fmt::Write as _;
use std::path::Path;

use kagari_syntax::{TextRange, TextSize};

use crate::db::Db;
use crate::input::{FilePath, SourceFile};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

/// A secondary location attached to a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Related {
    pub file: FilePath,
    pub range: TextRange,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub file: FilePath,
    pub range: TextRange,
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub related: Vec<Related>,
}

impl Diagnostic {
    /// A diagnostic whose severity is the code's default.
    pub fn new(
        file: FilePath,
        range: TextRange,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            file,
            range,
            severity: code.default_severity(),
            code,
            message: message.into(),
            related: Vec::new(),
        }
    }

    pub fn with_related(
        mut self,
        file: FilePath,
        range: TextRange,
        message: impl Into<String>,
    ) -> Diagnostic {
        self.related.push(Related {
            file,
            range,
            message: message.into(),
        });
        self
    }
}

/// A location inside a source file, cheap to copy into HIR and IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: SourceFile,
    pub range: TextRange,
}

impl Span {
    pub fn new(file: SourceFile, range: TextRange) -> Span {
        Span { file, range }
    }

    pub fn diagnostic(
        self,
        db: &dyn Db,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic::new(self.file.path(db).clone(), self.range, code, message)
    }

    pub fn related(self, db: &dyn Db, message: impl Into<String>) -> Related {
        Related {
            file: self.file.path(db).clone(),
            range: self.range,
            message: message.into(),
        }
    }
}

macro_rules! codes {
    ($($variant:ident = $code:literal $(, $warn:ident)?;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum DiagnosticCode {
            $($variant,)*
        }

        impl DiagnosticCode {
            pub const ALL: &'static [DiagnosticCode] = &[$(DiagnosticCode::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(DiagnosticCode::$variant => $code,)*
                }
            }

            pub fn default_severity(self) -> Severity {
                match self {
                    $(DiagnosticCode::$variant => codes!(@severity $($warn)?),)*
                }
            }
        }
    };
    (@severity) => { Severity::Error };
    (@severity warning) => { Severity::Warning };
}

codes! {
    Syntax = "syntax";

    CollectEntryNotKis = "collect/entry-not-kis";
    CollectBadRoot = "collect/bad-root";
    CollectFileNotFound = "collect/file-not-found";
    CollectBadSrc = "collect/bad-src";
    CollectBadExtension = "collect/bad-extension";
    CollectIncludeCycle = "collect/include-cycle";
    CollectIncludeRoot = "collect/include-root";
    CollectImportScope = "collect/import-scope";

    KisMissingAttribute = "kis/missing-attribute";
    KisUnknownAttribute = "kis/unknown-attribute", warning;
    KisVarConflict = "kis/var-conflict";
    KisUnknownElement = "kis/unknown-element", warning;
    KisElementNotAllowed = "kis/element-not-allowed";
    KisUnexpectedText = "kis/unexpected-text";
    KisBadCases = "kis/bad-cases";
    KisBadName = "kis/bad-name";
    KisBadValue = "kis/bad-value";
    KisValueOutOfRange = "kis/value-out-of-range";
    KisBadRange = "kis/bad-range";
    KisPatternArity = "kis/pattern-arity";
    KisDuplicateEnum = "kis/duplicate-enum";
    KisDuplicateVariant = "kis/duplicate-variant";
    KisDuplicateRepr = "kis/duplicate-repr";
    KisDuplicateVariable = "kis/duplicate-variable";
    KisDuplicateTemplate = "kis/duplicate-template";
    KisDuplicateScene = "kis/duplicate-scene";
    KisDuplicateMeta = "kis/duplicate-meta";
    KisDuplicateEntry = "kis/duplicate-entry";
    KisDuplicateChoiceId = "kis/duplicate-choice-id";
    KisDuplicateArmId = "kis/duplicate-arm-id";
    KisDuplicateLabel = "kis/duplicate-label";
    KisUnknownEnum = "kis/unknown-enum";
    KisUnknownMeta = "kis/unknown-meta";
    KisUnknownVariable = "kis/unknown-variable";
    KisUnknownLabel = "kis/unknown-label";
    KisUnknownScene = "kis/unknown-scene";
    KisUnknownTemplate = "kis/unknown-template";
    KisMetaEnumUnsupported = "kis/meta-enum-unsupported";
    KisMetaTypeMismatch = "kis/meta-type-mismatch";
    KisLocalShadowsGlobal = "kis/local-shadows-global";
    KisLabelOutOfScope = "kis/label-out-of-scope";
    KisNonExhaustiveMatch = "kis/non-exhaustive-match";
    KisNotACounter = "kis/not-a-counter";
    KisTemplateRecursion = "kis/template-recursion";
    KisSnipCount = "kis/snip-count";
    KisBadEntryExit = "kis/bad-entry-exit";
    KisNoEntry = "kis/no-entry";
    KisSteplessLoop = "kis/stepless-loop";
    KisSteplessCallCycle = "kis/stepless-call-cycle";
    KisBadVoice = "kis/bad-voice";

    KixDuplicateDefinition = "kix/duplicate-definition";
    KixReservedName = "kix/reserved-name";
    KixBadImplTarget = "kix/bad-impl-target";
    KixAliasCycle = "kix/alias-cycle";
    KixItemNotAllowed = "kix/item-not-allowed";
    KixBadAttribute = "kix/bad-attribute";
    KixUnknownAttribute = "kix/unknown-attribute", warning;
    KixMissingBody = "kix/missing-body";
    KixBuiltinWithBody = "kix/builtin-with-body";
    KixBuiltinOutsideStd = "kix/builtin-outside-std";
    KixBadAssetType = "kix/bad-asset-type";
    KixBadAssetPath = "kix/bad-asset-path";
    KixRecursion = "kix/recursion";
    KixConstCycle = "kix/const-cycle";

    KirpMissingRender = "kirp/missing-render";
    KirpDuplicateRender = "kirp/duplicate-render";
    KirpBadRenderSignature = "kirp/bad-render-signature";

    TypeUnresolvedType = "type/unresolved-type";
    TypeWrongGenericCount = "type/wrong-generic-count";
    TypeCannotInfer = "type/cannot-infer";
    TypeLiteralOutOfRange = "type/literal-out-of-range";
    TypeUnresolvedName = "type/unresolved-name";
    TypeSelfOutsideImpl = "type/self-outside-impl";
    TypeWrongArgCount = "type/wrong-arg-count";
    TypeMismatch = "type/mismatch";
    TypeNotCallable = "type/not-callable";
    TypeNoMethod = "type/no-method";
    TypeNoField = "type/no-field";
    TypeMissingFields = "type/missing-fields";
    TypeInvalidOperator = "type/invalid-operator";
    TypeInvalidCast = "type/invalid-cast";
    TypeNonExhaustiveMatch = "type/non-exhaustive-match";
    TypeBadPattern = "type/bad-pattern";
    TypeUnappliedChange = "type/unapplied-change", warning;
}

/// Render diagnostics as `{path}:{line}:{col}: {severity}[{code}]: {message}` lines,
/// with one indented `note:` line per related location. Lines and columns are 1-based;
/// columns count chars.
pub fn render_diagnostics(db: &dyn Db, diags: &[Diagnostic], base: Option<&Path>) -> String {
    if diags.is_empty() {
        return "no diagnostics\n".to_owned();
    }
    let mut out = String::new();
    for diag in diags {
        let (line, col) = line_col(db, &diag.file, diag.range.start());
        let severity = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let _ = writeln!(
            out,
            "{}:{line}:{col}: {severity}[{}]: {}",
            diag.file.display(base),
            diag.code.as_str(),
            diag.message
        );
        for related in &diag.related {
            let (line, col) = line_col(db, &related.file, related.range.start());
            let _ = writeln!(
                out,
                "  note: {}:{line}:{col}: {}",
                related.file.display(base),
                related.message
            );
        }
    }
    out
}

/// 1-based line and char column of `offset` in `file`.
fn line_col(db: &dyn Db, file: &FilePath, offset: TextSize) -> (usize, usize) {
    let source = db.source_file(file.clone());
    let text = source.text(db).clone().unwrap_or_else(|| "".into());
    let offset = usize::from(offset).min(text.len());
    let mut line: usize = 1;
    let mut col: usize = 1;
    for (index, ch) in text.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line = line.saturating_add(1);
            col = 1;
        } else {
            col = col.saturating_add(1);
        }
    }
    (line, col)
}
