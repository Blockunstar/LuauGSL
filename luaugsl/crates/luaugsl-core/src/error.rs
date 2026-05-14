use serde::{Deserialize, Serialize};
use std::fmt;

/// Position in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SourcePos {
    pub line: u32,
    pub column: u32,
    pub offset: usize,
}

impl fmt::Display for SourcePos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A source span from start to end position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: SourcePos,
    pub end: SourcePos,
}

impl Span {
    pub fn new(start: SourcePos, end: SourcePos) -> Self {
        Span { start, end }
    }
    pub fn single(pos: SourcePos) -> Self {
        Span { start: pos, end: pos }
    }
}

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

/// A single compiler diagnostic (error, warning, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    pub code: String,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, code: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span: None,
            code: code.into(),
        }
    }
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

/// The main compilation error type.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Serialize, Deserialize)]
pub enum CompileError {
    // Lexer errors
    #[error("unexpected character '{0}' at {1}")]
    UnexpectedChar(char, SourcePos),
    #[error("unterminated string starting at {0}")]
    UnterminatedString(SourcePos),
    #[error("unterminated block comment starting at {0}")]
    UnterminatedComment(SourcePos),
    #[error("invalid numeric literal '{0}' at {1}")]
    InvalidNumber(String, SourcePos),
    #[error("invalid escape sequence at {0}")]
    InvalidEscape(SourcePos),

    // Parser errors
    #[error("expected {expected}, found {found} at {pos}")]
    UnexpectedToken {
        expected: String,
        found: String,
        pos: SourcePos,
    },
    #[error("expected {0} at {1}")]
    ExpectedToken(String, SourcePos),
    #[error("unexpected end of file")]
    UnexpectedEof,
    #[error("invalid assignment target at {0}")]
    InvalidAssignmentTarget(SourcePos),
    #[error("duplicate swizzle component in l-value at {0}")]
    DuplicateSwizzleComponent(SourcePos),
    #[error("recursive function call detected: {0}")]
    RecursionDetected(String),
    #[error("variadic parameter '...' must be last")]
    VariadicNotLast,

    // Type errors
    #[error("type mismatch: expected {expected}, found {found}")]
    TypeMismatch {
        expected: String,
        found: String,
        span: Option<Span>,
    },
    #[error("unknown type: {0}")]
    UnknownType(String),
    #[error("unknown identifier: '{0}'")]
    UnknownIdentifier(String),
    #[error("cannot assign to '{0}' — it is declared as const")]
    AssignToConst(String),
    #[error("swizzle l-value cannot have duplicate components: '{0}'")]
    SwizzleDuplicateComponents(String),
    #[error("no matching overload for '{name}' with argument types [{args}]")]
    NoOverload {
        name: String,
        args: String,
        span: Option<Span>,
    },
    #[error("function '{name}' expects {expected} arguments but {got} were provided")]
    ArgCountMismatch {
        name: String,
        expected: usize,
        got: usize,
        span: Option<Span>,
    },
    #[error("f64/double is not supported in GPU contexts")]
    F64NotAllowed,
    #[error("pairs() and ipairs() are not allowed in shader stages")]
    PairsNotAllowed,
    #[error("'{0}' is only available in fragment stage")]
    FragmentOnly(String),
    #[error("'{0}' is only available in compute stage")]
    ComputeOnly(String),
    #[error("loops must have a statically verifiable upper bound")]
    UnboundedLoop,
    #[error("module loading uses require(), not import")]
    ImportNotAllowed,

    // Capability errors
    #[error("capability '{cap}' is not available in stage '{stage}'")]
    CapabilityNotAvailable { cap: String, stage: String },
    #[error("missing capability declaration for '{0}'")]
    MissingCapability(String),
    #[error("safety limit exceeded: {limit} (max {max})")]
    SafetyLimitExceeded { limit: String, max: u32 },

    // Codegen errors
    #[error("SPIR-V generation failed: {0}")]
    SpirvGenError(String),
    #[error("unsupported feature for target: {0}")]
    UnsupportedFeature(String),

    // Artifact errors
    #[error("artifact verification failed: {0}")]
    ArtifactVerifyFailed(String),
    #[error("artifact signature invalid")]
    InvalidSignature,

    // General
    #[error("{0}")]
    General(String),
    #[error("multiple errors occurred")]
    MultipleErrors(Vec<CompileError>),
}

impl CompileError {
    pub fn with_span(self, span: Span) -> Self {
        // For variants that have a span field, set it
        match self {
            CompileError::TypeMismatch { expected, found, .. } => {
                CompileError::TypeMismatch {
                    expected,
                    found,
                    span: Some(span),
                }
            }
            CompileError::NoOverload { name, args, .. } => {
                CompileError::NoOverload {
                    name,
                    args,
                    span: Some(span),
                }
            }
            CompileError::ArgCountMismatch {
                name, expected, got, ..
            } => CompileError::ArgCountMismatch {
                name,
                expected,
                got,
                span: Some(span),
            },
            _ => self,
        }
    }
}

/// Result type used throughout the compiler.
pub type CompileResult<T> = Result<T, CompileError>;

/// A collection of diagnostics from a compilation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticBag {
    pub diagnostics: Vec<Diagnostic>,
    pub has_errors: bool,
}

impl DiagnosticBag {
    pub fn new() -> Self {
        DiagnosticBag {
            diagnostics: Vec::new(),
            has_errors: false,
        }
    }
    pub fn error(&mut self, message: impl Into<String>, code: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(message, code));
        self.has_errors = true;
    }
    pub fn warning(&mut self, message: impl Into<String>, code: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            message: message.into(),
            span: None,
            code: code.into(),
        });
    }
    pub fn extend(&mut self, other: DiagnosticBag) {
        if other.has_errors {
            self.has_errors = true;
        }
        self.diagnostics.extend(other.diagnostics);
    }
}
