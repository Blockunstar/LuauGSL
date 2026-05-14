/// The complete token type for LuauGSL.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Number(f64),
    String(String),
    Interpolated(Vec<super::StringPart>),
    Nil,
    True,
    False,

    // Lua/Luau keywords
    And,
    Break,
    Continue,
    Do,
    Else,
    Elseif,
    End,
    For,
    Function,
    If,
    In,
    Local,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    Until,
    While,
    Const,
    Type,
    Export,
    Import,

    // Shader keywords
    Uniform,
    Sampler,
    F32,
    I32,
    U32,
    Bool,
    Vector2,
    Vector3,
    Vector4,
    Vector2i,
    Vector3i,
    Vector4i,
    Vector2u,
    Vector3u,
    Vector4u,
    BVector2,
    BVector3,
    BVector4,
    Mat2x2,
    Mat3x3,
    Mat4x4,
    Texture2D,
    Texture3D,
    TextureCube,
    Texture2DArray,
    TextureCubeArray,
    StorageBuffer,
    StorageImage,

    // Barrier builtins
    WorkgroupBarrier,
    MemoryBarrier,
    StorageBarrier,
    TextureBarrier,

    // Operators
    Plus,        // +
    Minus,       // -
    Star,        // *
    Slash,       // /
    FloorDiv,    // //
    Percent,     // %
    Caret,       // ^
    Hash,        // #
    Concat,      // ..
    Ellipsis,    // ...
    Eq,          // =
    EqEq,        // ==
    TildeEq,     // ~=
    Lt,          // <
    Le,          // <=
    Gt,          // >
    Ge,          // >=
    PlusEq,      // +=
    MinusEq,     // -=
    StarEq,      // *=
    SlashEq,     // /=
    FloorDivEq,  // //=
    PercentEq,   // %=
    CaretEq,     // ^=
    ConcatEq,    // ..=
    Tilde,       // ~
    Pipe,        // |
    Ampersand,   // &

    // Compound logical (parsed as identifiers then recognized)

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Dot,
    Colon,
    DoubleColon, // ::
    Arrow,       // ->

    // Attributes
    Attribute(String), // @name

    // Identifiers
    Ident(String),

    // End of file
    Eof,
}

impl Token {
    /// Check if this token is an identifier.
    pub fn is_ident(&self) -> bool {
        matches!(self, Token::Ident(_))
    }

    /// Get the identifier name if this is an identifier token.
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            Token::Ident(s) => Some(s),
            _ => None,
        }
    }

    /// Check if this token can start a statement.
    pub fn is_stmt_start(&self) -> bool {
        matches!(self,
            Token::Local | Token::Const | Token::If | Token::For | Token::While
            | Token::Repeat | Token::Do | Token::Return | Token::Break | Token::Continue
            | Token::Function | Token::Ident(_) | Token::Attribute(_) | Token::Semicolon
            | Token::Type | Token::Export
        )
    }
}
