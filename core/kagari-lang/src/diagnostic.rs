//! Diagnostics produced by every stage, and their text rendering.

use std::fmt::Write as _;
use std::path::Path;

use kagari_syntax::{TextRange, TextSize};

use crate::db::Db;
use crate::hir::{
    ExprId, PatId, StoreSourceMap, TypeRefId, body_source_map_of, signature_source_map_of,
};
use crate::input::{FilePath, SourceFile};
use crate::item_tree::{ItemId, ItemSrc, item_source_map};

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

/// The store a HIR node belongs to: an item's signature or its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Signature(ItemId),
    Body(ItemId),
}

impl Owner {
    pub fn item(self) -> ItemId {
        match self {
            Owner::Signature(item) | Owner::Body(item) => item,
        }
    }
}

/// A node of an [`ExprStore`]. `PatBound(pat, 0 | 1)` is the lower or upper bound of a
/// range pattern, used for the path segments it contains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeId {
    Expr(ExprId),
    Pat(PatId),
    Type(TypeRefId),
    PatBound(PatId, u8),
}

/// A location that survives edits elsewhere in the file: it names what it points at and is
/// resolved to a text range only when diagnostics are reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// A raw range; only from parsing and item-tree-level checks.
    Text(SourceFile, TextRange),
    ItemName(ItemId),
    ItemWhole(ItemId),
    /// Attribute `i` of an item.
    ItemAttr(ItemId, u32),
    /// Variant `i` of an enum.
    Variant(ItemId, u32),
    /// Import `i` of a `use` item.
    Import(ItemId, u32),
    /// Segment `j` of the path of import `i` of a `use` item.
    ImportSegment(ItemId, u32, u32),
    /// Generic parameter `i` of an item.
    GenericParam(ItemId, u32),
    Node(Owner, NodeId),
    /// Segment `j` of the path owned by a node.
    Segment(Owner, NodeId, u32),
}

/// A diagnostic inside a query result: anchored, resolved to text by [`Diag::resolve`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Diag {
    pub anchor: Anchor,
    pub code: DiagnosticCode,
    pub message: String,
    pub related: Vec<(Anchor, String)>,
    /// `{@N}` in `message` is replaced with the `line:col` of `message_anchors[N]`.
    pub message_anchors: Vec<Anchor>,
}

impl Diag {
    pub fn new(anchor: Anchor, code: DiagnosticCode, message: impl Into<String>) -> Diag {
        Diag {
            anchor,
            code,
            message: message.into(),
            related: Vec::new(),
            message_anchors: Vec::new(),
        }
    }

    pub fn with_related(mut self, anchor: Anchor, message: impl Into<String>) -> Diag {
        self.related.push((anchor, message.into()));
        self
    }

    pub fn severity(&self) -> Severity {
        self.code.default_severity()
    }

    /// The diagnostic with its anchors resolved to text ranges.
    pub fn resolve(&self, db: &dyn Db) -> Diagnostic {
        let (file, range) = resolve_anchor(db, &self.anchor);
        let message = self.resolve_message(db);
        let mut out = Diagnostic::new(file.path(db).clone(), range, self.code, message);
        for (anchor, message) in &self.related {
            let (file, range) = resolve_anchor(db, anchor);
            out = out.with_related(file.path(db).clone(), range, message.clone());
        }
        out
    }

    /// The message with `{@N}` replaced by the `line:col` of `message_anchors[N]` and `{#N}`
    /// by its source text.
    fn resolve_message(&self, db: &dyn Db) -> String {
        if self.message_anchors.is_empty() {
            return self.message.clone();
        }
        let mut out = String::with_capacity(self.message.len());
        let mut rest = self.message.as_str();
        while let Some(start) = rest.find('{') {
            let (before, after) = rest.split_at(start);
            out.push_str(before);
            let placeholder = after.find('}').and_then(|end| {
                let inner = after.get(1..end)?;
                let (kind, index) = (inner.get(..1)?, inner.get(1..)?.parse::<usize>().ok()?);
                let anchor = self.message_anchors.get(index)?;
                let (file, range) = resolve_anchor(db, anchor);
                let text = match kind {
                    "@" => {
                        let (line, col) = line_col(db, file.path(db), range.start());
                        format!("{line}:{col}")
                    }
                    "#" => file
                        .text(db)
                        .as_deref()
                        .and_then(|text| {
                            text.get(usize::from(range.start())..usize::from(range.end()))
                        })
                        .unwrap_or_default()
                        .to_owned(),
                    _ => return None,
                };
                Some((text, end))
            });
            match placeholder {
                Some((text, end)) => {
                    out.push_str(&text);
                    rest = after.get(end.saturating_add(1)..).unwrap_or_default();
                }
                None => {
                    out.push('{');
                    rest = after.get(1..).unwrap_or_default();
                }
            }
        }
        out.push_str(rest);
        out
    }
}

/// The file and text range an anchor points at; an empty range when it no longer exists.
pub(crate) fn resolve_anchor(db: &dyn Db, anchor: &Anchor) -> (SourceFile, TextRange) {
    let range = match *anchor {
        Anchor::Text(file, range) => return (file, range),
        Anchor::ItemName(item) => item_src(db, item).map(|src| src.name),
        Anchor::ItemWhole(item) => item_src(db, item).map(|src| src.ptr.text_range()),
        Anchor::ItemAttr(item, index) => {
            item_src(db, item).and_then(|src| src.attrs.get(index as usize).copied())
        }
        Anchor::Variant(item, index) => {
            item_src(db, item).and_then(|src| src.variants.get(index as usize).copied())
        }
        Anchor::Import(item, index) => item_src(db, item)
            .and_then(|src| src.imports.get(index as usize))
            .map(|(range, _)| *range),
        Anchor::ImportSegment(item, index, segment) => item_src(db, item)
            .and_then(|src| src.imports.get(index as usize))
            .and_then(|(_, segments)| segments.get(segment as usize).copied()),
        Anchor::GenericParam(item, index) => signature_source_map_of(db, item)
            .and_then(|map| map.generic_params.get(index as usize).copied()),
        Anchor::Node(owner, node) => store_map(db, owner).map(|map| match node {
            NodeId::Expr(expr) => map.expr(expr),
            NodeId::Pat(pat) | NodeId::PatBound(pat, _) => map.pat(pat),
            NodeId::Type(ty) => map.type_ref(ty),
        }),
        Anchor::Segment(owner, node, segment) => {
            store_map(db, owner).and_then(|map| map.segment(node, segment as usize))
        }
    };
    (anchor.file(), range.unwrap_or_default())
}

impl Anchor {
    /// The file the anchor is in.
    pub fn file(&self) -> SourceFile {
        match *self {
            Anchor::Text(file, _) => file,
            Anchor::ItemName(item)
            | Anchor::ItemWhole(item)
            | Anchor::ItemAttr(item, _)
            | Anchor::Variant(item, _)
            | Anchor::Import(item, _)
            | Anchor::ImportSegment(item, _, _)
            | Anchor::GenericParam(item, _) => item.file,
            Anchor::Node(owner, _) | Anchor::Segment(owner, _, _) => owner.item().file,
        }
    }
}

fn item_src(db: &dyn Db, item: ItemId) -> Option<&ItemSrc> {
    item_source_map(db, item.file).items.get(item.index.idx())
}

fn store_map(db: &dyn Db, owner: Owner) -> Option<&StoreSourceMap> {
    match owner {
        Owner::Signature(item) => signature_source_map_of(db, item),
        Owner::Body(item) => body_source_map_of(db, item),
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
    ResolveAssetOutsideRoot = "E1029";

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
