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
    // E0: syntax.
    Syntax = "E0001";

    // E1: module, item, resolve.
    ModuleBadRoot = "E1001";
    ModuleFileNotFound = "E1002";
    ModuleAmbiguousFile = "E1003";
    ModuleFileReused = "E1004";
    ModuleInBlock = "E1005";

    ItemUnknownAttribute = "E1006";
    ItemAttributeArguments = "E1007";
    ItemBuiltinOutsideStd = "E1008";
    ItemBuiltinWithBody = "E1009";
    ItemMissingBody = "E1010";
    ItemMissingValue = "E1011";
    ItemTraitOutsideStd = "E1012";
    ItemTraitNotBuiltin = "E1013";

    ResolveDuplicate = "E1014";
    ResolveUnresolvedImport = "E1015";
    ResolveUnresolvedName = "E1016";
    ResolvePrivate = "E1017";
    ResolveAmbiguous = "E1018";
    ResolveUndeclaredLabel = "E1019";
    ResolveOutsideLoop = "E1020";
    ResolveSelfOutsideImpl = "E1021";
    ResolveBadImplTarget = "E1022";
    ResolveTraitImplOutsideStd = "E1023";
    ResolveDuplicateBinding = "E1024";
    ResolveInconsistentBinding = "E1025";
    ResolveMissingStdItem = "E1026";
    ResolveAssetNotFound = "E1027";
    ResolveEmptyAssetPath = "E1028";

    // E2: type inference, exhaustiveness and refutability.
    TypeMismatch = "E2001";
    TypeUnsupportedConst = "E2002";
    TypeConstOverflow = "E2003";
    TypeBadIntBounds = "E2004";
    TypeWrongGenericCount = "E2005";
    TypeExpectedType = "E2006";
    TypeExpectedConst = "E2007";
    TypeExpectedTrait = "E2008";
    TypeExpectedValue = "E2009";
    TypeAliasCycle = "E2010";
    TypeInferInSignature = "E2011";
    TypeBadLiteral = "E2012";
    TypeLiteralOutOfRange = "E2013";
    TypeIntMix = "E2014";
    TypeBadOperands = "E2015";
    TypeDivisorMayBeZero = "E2016";
    TypeNotEq = "E2017";
    TypeLossyInto = "E2018";
    TypeAsCast = "E2019";
    TypeUnknownField = "E2020";
    TypeMissingField = "E2021";
    TypeUnknownMethod = "E2022";
    TypeNotAMethod = "E2023";
    TypeWrongArity = "E2024";
    TypeNotCallable = "E2025";
    TypeUnsatisfiedBound = "E2026";
    TypeBadNavStart = "E2027";
    TypeAssetPathNotLiteral = "E2028";
    TypeAnnotationsNeeded = "E2029";
    TypeOpenRange = "E2030";
    TypeNotIterable = "E2031";
    TypeNotIndexable = "E2032";
    TypeBadDeref = "E2033";
    TypeBadTry = "E2034";
    TypeBadPattern = "E2035";
    TypeCapturingClosure = "E2036";
    TypeNonExhaustiveMatch = "E2037";
    TypeRefutablePattern = "E2038";
    TypeUnreachablePattern = "W2039", warning;

    // E3: kind and story rules.
    KindCallRule = "E3001";
    KindStepOutsideScene = "E3002";
    KindSceneCallOutsideScene = "E3003";
    KindUnboundedLoop = "E3004";
    KindInstantRecursion = "E3005";
    KindForNotConstant = "E3006";
    KindStoryVariableOutsideStory = "E3007";
    KindNotFinite = "E3008";
    KindTextInEntry = "E3009";
    KindNotConstant = "E3010";
    KindBadHandle = "E3011";
    KindReferenceInItem = "E3012";
    KindMetaInConst = "E3013";
    KindSecondClassReference = "E3014";
    KindSecondClassClosure = "E3015";
    KindAliasedMut = "E3016";
    KindImmutablePlace = "E3017";
    KindMisplacedAttribute = "E3018";
    KindChoiceId = "E3019";
    KindChoiceArms = "E3020";
    KindRubyReadings = "E3021";
    KindEffectNotConstant = "E3022";
    KindUnusedMustUse = "W3023", warning;
    KindRngOutsideScene = "E3024";

    // E4: const evaluation.
    EvalOverflow = "E4001";
    EvalDivisionByZero = "E4002";
    EvalBudgetExceeded = "E4003";
    EvalDomainTooLarge = "E4004";
    EvalIndexOutOfRange = "E4005";
    EvalConstCycle = "E4006";
    EvalHandleOutsideStatic = "E4007";
    EvalInvalidMotion = "E4008";
    EvalInvalidEasing = "E4009";
    EvalDuplicateMark = "E4010";
    EvalBeginTextOnMark = "E4011";
    EvalFractionOutOfRange = "E4012";
    EvalDurationOverflow = "E4013";

    // E5: whole-program analysis of the story.
    AnalysisSteplessLoop = "E5001";
    AnalysisSteplessRecursion = "E5002";
    AnalysisStuckChoice = "E5003";
    AnalysisInconsistentScreen = "W5004", warning;
    AnalysisUnboundedScreens = "E5005";
    AnalysisTooManyStates = "W5006", warning;
    AnalysisIncomplete = "W5007", warning;
    AnalysisUnreachableScene = "W5008", warning;
    AnalysisUnreachableStep = "W5009", warning;
    AnalysisUnusedStoryVariable = "W5010", warning;
    AnalysisMissingKeyframe = "W5011", warning;

    // E6: the system's mode graph.
    SystemUnreachableMode = "W6001", warning;
    SystemBackFromBase = "E6002";
    SystemTrappedMode = "E6003";
    SystemUnboundedStack = "W6004", warning;

    // E7: bytecode emission.
    EmitMissingMain = "E7001";
    EmitFunctionTooLarge = "E7002";
    EmitInvalidBytecode = "E7003";
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
pub(crate) fn line_col(db: &dyn Db, file: &FilePath, offset: TextSize) -> (usize, usize) {
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

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashSet;

    use super::{DiagnosticCode, Severity};

    #[test]
    fn codes_are_numbered() {
        let mut seen = FxHashSet::default();
        for code in DiagnosticCode::ALL {
            let text = code.as_str();
            assert!(seen.insert(text), "duplicate code {text}");
            let bytes = text.as_bytes();
            assert!(
                bytes.len() == 5
                    && matches!(bytes[0], b'E' | b'W')
                    && (b'0'..=b'7').contains(&bytes[1])
                    && bytes[2..].iter().all(u8::is_ascii_digit),
                "malformed code {text}"
            );
            assert_eq!(
                bytes[0] == b'W',
                code.default_severity() == Severity::Warning,
                "{text}: the prefix must match the severity"
            );
        }
    }
}
