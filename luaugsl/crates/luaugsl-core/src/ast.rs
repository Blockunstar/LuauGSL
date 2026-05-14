use crate::types::*;
use serde::{Deserialize, Serialize};

/// A LuauGSL source file module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub directives: Vec<ModuleDirective>,
    pub bindings: Vec<BindingDecl>,
    pub type_aliases: Vec<TypeAlias>,
    pub functions: Vec<FunctionDecl>,
    pub capabilities: Vec<CapabilityDecl>,
    pub exports: Vec<String>,
    pub entry_point: Option<FunctionDecl>,
}

/// Module-level directives like --!strict, --!shader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDirective {
    pub name: String,
    pub value: Option<String>,
}

/// A resource binding declaration: @binding(0,0) const Name = texture2d
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingDecl {
    pub attributes: Vec<Attribute>,
    pub is_const: bool,
    pub is_export: bool,
    pub name: String,
    pub value: BindingValue,
    pub ty: Type,
}

/// The value side of a binding declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BindingValue {
    Uniform { fields: Vec<FieldDef> },
    Texture { kind: TextureKind },
    Sampler,
    StorageBuffer { elem_ty: Type, access: Access },
    StorageImage { format: String },
    Expr(Expr),
}

/// Texture kind for binding declarations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextureKind {
    Texture1D,
    Texture2D,
    Texture3D,
    TextureCube,
    Texture2DArray,
    TextureCubeArray,
}

/// A type alias declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeAlias {
    pub is_export: bool,
    pub name: String,
    pub params: Vec<String>,
    pub ty: Type,
}

/// A capability declaration: @capability(...)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDecl {
    pub cap: Capability,
}

/// A function declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub attributes: Vec<Attribute>,
    pub is_export: bool,
    pub name: String,
    pub generic_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Type,
    pub body: Block,
    pub is_entry_point: bool,
    pub stage: Option<ShaderStage>,
}

/// An attribute: @name(args)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<AttrArg>,
}

/// Attribute argument values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttrArg {
    String(String),
    Number(f64),
    Ident(String),
}

/// A block of statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub ret: Option<Vec<Expr>>,
}

/// A statement in LuauGSL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    /// Empty statement: `;`
    Empty,
    /// Local variable declaration: `local x = expr` or `local x: type = expr`
    LocalDecl {
        names: Vec<(String, Option<Type>)>,
        values: Vec<Expr>,
        attributes: Vec<Attribute>,
    },
    /// Const declaration: `const MAX = 100`
    ConstDecl {
        names: Vec<(String, Option<Type>)>,
        values: Vec<Expr>,
        attributes: Vec<Attribute>,
    },
    /// Assignment: `x = expr` or `x.y = expr`
    Assign {
        target: AssignTarget,
        value: Expr,
    },
    /// Compound assignment: `x += 1`
    CompoundAssign {
        target: AssignTarget,
        op: BinOp,
        value: Expr,
    },
    /// Function call as statement
    Call(Expr),
    /// If statement
    If {
        cond: Expr,
        then_block: Block,
        elseifs: Vec<(Expr, Block)>,
        else_block: Option<Block>,
    },
    /// Numeric for loop: `for i = start, end, step do ... end`
    ForNumeric {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Block,
    },
    /// Generalized for loop: `for k, v in expr do ... end`
    ForGeneral {
        vars: Vec<String>,
        expr: Expr,
        body: Block,
    },
    /// While loop: `while cond do ... end`
    While {
        cond: Expr,
        body: Block,
    },
    /// Repeat-until: `repeat ... until cond`
    Repeat {
        body: Block,
        cond: Expr,
    },
    /// Do-end block
    DoBlock(Block),
    /// Break
    Break,
    /// Continue
    Continue,
    /// Return (handled at Block level, but can appear as stmt too)
    Return(Vec<Expr>),
    /// Type alias declaration
    TypeAlias(TypeAlias),
    /// Binding declaration
    Binding(BindingDecl),
}

/// Assignment target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssignTarget {
    /// Simple variable: `x`
    Var(String),
    /// Field access: `obj.field`
    Field { base: Box<Expr>, field: String },
    /// Index access: `arr[i]`
    Index { base: Box<Expr>, index: Box<Expr> },
    /// Swizzle assignment: `pos.xy`
    Swizzle { base: Box<Expr>, components: Vec<char> },
}

/// An expression in LuauGSL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// Literal value
    Literal(Literal),
    /// Variable reference
    Identifier(String),
    /// Binary operation
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary operation
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },
    /// Function call
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    /// Table index: `tbl[key]`
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// Field access: `obj.field`
    Member {
        base: Box<Expr>,
        member: String,
    },
    /// Table constructor: `{x = 1, y = 2}`
    Table(Vec<(Option<String>, Expr)>),
    /// Array constructor: `{1, 2, 3}`
    Array(Vec<Expr>),
    /// Type cast: `expr :: type`
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
    /// Vector swizzle: `pos.xyz`
    Swizzle {
        base: Box<Expr>,
        components: Vec<char>,
    },
    /// Interpolated string: `hello {name}`
    InterpolatedString(Vec<StringPart>),
    /// Grouped expression: `(expr)`
    Grouped(Box<Expr>),
    /// Lambda/inline function: `(x) => x + 1` — LuauGSL uses `function(...) return ... end`
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
    },
    /// Ternary: `cond ? a : b` — Luau uses if-then-else
    Ternary {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    /// Vector constructor: `vector4.create(1, 2, 3, 4)`
    VectorConstructor {
        ty: String,
        args: Vec<Expr>,
    },
    /// Matrix constructor: `mat4x4.create(...)`
    MatrixConstructor {
        ty: String,
        args: Vec<Expr>,
    },
    /// Method call: `obj:method(args)` — desugared to `obj.method(obj, args)`
    MethodCall {
        base: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    /// Return expression (used in lambda bodies)
    Return(Vec<Expr>),
}

/// A part of an interpolated string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StringPart {
    Literal(String),
    Expr(Expr),
}

/// Literal values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Concat,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnOp {
    Not,
    Neg,
    Len,
    BitNot,
}

impl Expr {
    /// Check if this expression is a literal.
    pub fn is_literal(&self) -> bool {
        matches!(self, Expr::Literal(_))
    }

    /// Get the literal value if this is a literal.
    pub fn as_literal(&self) -> Option<&Literal> {
        match self {
            Expr::Literal(lit) => Some(lit),
            _ => None,
        }
    }

    /// Check if this is a simple identifier.
    pub fn is_identifier(&self) -> bool {
        matches!(self, Expr::Identifier(_))
    }
}
