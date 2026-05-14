use luaugsl_core::ast::*;
use luaugsl_core::types::*;

/// High-level intermediate representation for LuauGSL.
/// This is a simplified CFG-friendly IR between AST and SPIR-V.
#[derive(Debug, Clone)]
pub struct HIRModule {
    pub stage: Option<ShaderStage>,
    pub entry_point: Option<String>,
    pub bindings: Vec<Binding>,
    pub functions: Vec<HIRFunction>,
    pub globals: Vec<(String, Type)>,
}

#[derive(Debug, Clone)]
pub struct HIRFunction {
    pub name: String,
    pub stage: Option<ShaderStage>,
    pub params: Vec<Param>,
    pub return_type: Type,
    pub body: Vec<HIRStmt>,
    pub workgroup_size: Option<[u32; 3]>,
    pub is_export: bool,
}

#[derive(Debug, Clone)]
pub enum HIRStmt {
    /// Variable declaration: `let name: ty = value`
    Declare {
        name: String,
        ty: Type,
        value: HIRExpr,
        is_const: bool,
    },
    /// Assignment: `target = value`
    Assign {
        target: AssignTarget,
        value: HIRExpr,
    },
    /// Compound assignment: `target op= value`
    CompoundAssign {
        target: AssignTarget,
        op: BinOp,
        value: HIRExpr,
    },
    /// Expression statement (e.g., function call, barrier)
    Expr(HIRExpr),
    /// If-then-else
    If {
        cond: HIRExpr,
        then_branch: Vec<HIRStmt>,
        else_branch: Vec<HIRStmt>,
    },
    /// Loop: while cond { body }
    Loop {
        cond: Option<HIRExpr>,
        body: Vec<HIRStmt>,
    },
    /// For loop: `for i in start..end { body }`
    For {
        var: String,
        start: HIRExpr,
        end: HIRExpr,
        step: HIRExpr,
        body: Vec<HIRStmt>,
    },
    /// Return from function
    Return(Vec<HIRExpr>),
    /// Break out of loop
    Break,
    /// Continue to next iteration
    Continue,
    /// Workgroup barrier
    WorkgroupBarrier,
    /// Storage barrier
    StorageBarrier,
    /// Texture barrier
    TextureBarrier,
    /// Memory barrier
    MemoryBarrier,
    /// Discard (fragment only)
    Discard,
    /// Optimizer-generated block expansion
    Block(Vec<HIRStmt>),
}

#[derive(Debug, Clone)]
pub enum HIRExpr {
    /// Literal value
    Literal(Literal),
    /// Local variable reference
    Local(String),
    /// Global/binding reference
    Global(String),
    /// Binary operation
    Binary {
        op: BinOp,
        left: Box<HIRExpr>,
        right: Box<HIRExpr>,
    },
    /// Unary operation
    Unary {
        op: UnOp,
        operand: Box<HIRExpr>,
    },
    /// Function/intrinsic call
    Call {
        func: String,
        args: Vec<HIRExpr>,
    },
    /// Namespaced call: noise.perlin, bit32.band, etc.
    NamespacedCall {
        namespace: String,
        func: String,
        args: Vec<HIRExpr>,
    },
    /// Field access: `obj.field`
    FieldAccess {
        base: Box<HIRExpr>,
        field: String,
    },
    /// Index access: `arr[i]`
    Index {
        base: Box<HIRExpr>,
        index: Box<HIRExpr>,
    },
    /// Type cast: `expr :: ty`
    Cast {
        expr: Box<HIRExpr>,
        ty: Type,
    },
    /// Swizzle: `vec.xyz`
    Swizzle {
        base: Box<HIRExpr>,
        components: Vec<char>,
    },
    /// Vector constructor
    VectorConstruct {
        ty: Type,
        args: Vec<HIRExpr>,
    },
    /// Matrix constructor
    MatrixConstruct {
        ty: Type,
        args: Vec<HIRExpr>,
    },
    /// Table/array constructor
    Construct {
        ty: Type,
        fields: Vec<(Option<String>, HIRExpr)>,
    },
    /// Ternary: `cond ? a : b`
    Select {
        cond: Box<HIRExpr>,
        then_val: Box<HIRExpr>,
        else_val: Box<HIRExpr>,
    },
    /// Built-in derivative (fragment only)
    Derivative { kind: DerivativeKind, val: Box<HIRExpr> },
}

#[derive(Debug, Clone)]
pub enum DerivativeKind {
    Dx,
    Dy,
    Fwidth,
}

/// Lower an AST module to HIR.
pub fn lower_module(module: &Module) -> HIRModule {
    let mut hir = HIRModule {
        stage: module.entry_point.as_ref().and_then(|f| f.stage),
        entry_point: module.entry_point.as_ref().map(|f| f.name.clone()),
        bindings: module.bindings.iter().map(|b| Binding {
            set: b.attributes.iter().find(|a| a.name == "binding").and_then(|a| a.args.first()).and_then(|a| match a {
                AttrArg::Number(n) => Some(*n as u32),
                _ => None,
            }).unwrap_or(0),
            binding: b.attributes.iter().find(|a| a.name == "binding").and_then(|a| a.args.get(1)).and_then(|a| match a {
                AttrArg::Number(n) => Some(*n as u32),
                _ => None,
            }).unwrap_or(0),
            name: b.name.clone(),
            ty: b.ty.clone(),
            stage: None,
        }).collect(),
        functions: Vec::new(),
        globals: Vec::new(),
    };

    for func in &module.functions {
        let workgroup_size = func.attributes.iter().find(|a| a.name == "workgroupSize").map(|a| {
            let mut sizes = [1u32; 3];
            for (i, arg) in a.args.iter().enumerate().take(3) {
                if let AttrArg::Number(n) = arg {
                    sizes[i] = *n as u32;
                }
            }
            sizes
        });

        let body = lower_block(&func.body);

        hir.functions.push(HIRFunction {
            name: func.name.clone(),
            stage: func.stage,
            params: func.params.clone(),
            return_type: func.return_type.clone(),
            body,
            workgroup_size,
            is_export: func.is_export,
        });
    }

    hir
}

fn lower_block(block: &Block) -> Vec<HIRStmt> {
    let mut stmts = Vec::new();
    for stmt in &block.stmts {
        lower_stmt(stmt, &mut stmts);
    }
    if let Some(ref ret) = block.ret {
        let ret_exprs: Vec<_> = ret.iter().map(lower_expr).collect();
        stmts.push(HIRStmt::Return(ret_exprs));
    }
    stmts
}

fn lower_stmt(stmt: &Stmt, out: &mut Vec<HIRStmt>) {
    match stmt {
        Stmt::Empty => {}
        Stmt::LocalDecl { names, values, .. } => {
            for (i, (name, ty)) in names.iter().enumerate() {
                let value = values.get(i).map(lower_expr).unwrap_or(HIRExpr::Literal(Literal::Nil));
                out.push(HIRStmt::Declare {
                    name: name.clone(),
                    ty: ty.clone().unwrap_or(Type::Unknown),
                    value,
                    is_const: false,
                });
            }
        }
        Stmt::ConstDecl { names, values, .. } => {
            for (i, (name, ty)) in names.iter().enumerate() {
                let value = values.get(i).map(lower_expr).unwrap_or(HIRExpr::Literal(Literal::Nil));
                out.push(HIRStmt::Declare {
                    name: name.clone(),
                    ty: ty.clone().unwrap_or(Type::Unknown),
                    value,
                    is_const: true,
                });
            }
        }
        Stmt::Assign { target, value } => {
            out.push(HIRStmt::Assign {
                target: target.clone(),
                value: lower_expr(value),
            });
        }
        Stmt::CompoundAssign { target, op, value } => {
            out.push(HIRStmt::CompoundAssign {
                target: target.clone(),
                op: *op,
                value: lower_expr(value),
            });
        }
        Stmt::Call(expr) => {
            out.push(HIRStmt::Expr(lower_expr(expr)));
        }
        Stmt::If { cond, then_block, elseifs, else_block } => {
            let mut else_branch = else_block.as_ref().map(|b| {
                let mut stmts = Vec::new();
                for s in &b.stmts { lower_stmt(s, &mut stmts); }
                stmts
            }).unwrap_or_default();

            // Process elseifs in reverse order
            for (c, b) in elseifs.iter().rev() {
                let mut then_stmts = Vec::new();
                for s in &b.stmts { lower_stmt(s, &mut then_stmts); }
                else_branch = vec![HIRStmt::If {
                    cond: lower_expr(c),
                    then_branch: then_stmts,
                    else_branch,
                }];
            }

            let mut then_stmts = Vec::new();
            for s in &then_block.stmts { lower_stmt(s, &mut then_stmts); }

            out.push(HIRStmt::If {
                cond: lower_expr(cond),
                then_branch: then_stmts,
                else_branch,
            });
        }
        Stmt::ForNumeric { var, start, end, step, body } => {
            let step = step.as_ref().map(lower_expr).unwrap_or(HIRExpr::Literal(Literal::Number(1.0)));
            let body_stmts = lower_block(body);
            out.push(HIRStmt::For {
                var: var.clone(),
                start: lower_expr(start),
                end: lower_expr(end),
                step,
                body: body_stmts,
            });
        }
        Stmt::ForGeneral { vars: _, expr, body } => {
            let body_stmts = lower_block(body);
            out.push(HIRStmt::Loop {
                cond: Some(lower_expr(expr)),
                body: body_stmts,
            });
        }
        Stmt::While { cond, body } => {
            let body_stmts = lower_block(body);
            out.push(HIRStmt::Loop {
                cond: Some(lower_expr(cond)),
                body: body_stmts,
            });
        }
        Stmt::Repeat { body, cond } => {
            let body_stmts = lower_block(body);
            out.push(HIRStmt::If {
                cond: lower_expr(cond),
                then_branch: body_stmts,
                else_branch: vec![HIRStmt::Break],
            });
        }
        Stmt::DoBlock(block) => {
            out.extend(lower_block(block));
        }
        Stmt::Break => out.push(HIRStmt::Break),
        Stmt::Continue => out.push(HIRStmt::Continue),
        Stmt::Return(exprs) => {
            out.push(HIRStmt::Return(exprs.iter().map(lower_expr).collect()));
        }
        Stmt::TypeAlias(_) => {}
        Stmt::Binding(_) => {}
    }
}

fn lower_expr(expr: &Expr) -> HIRExpr {
    match expr {
        Expr::Literal(lit) => HIRExpr::Literal(lit.clone()),
        Expr::Identifier(name) => HIRExpr::Local(name.clone()),
        Expr::Binary { op, left, right } => HIRExpr::Binary {
            op: *op,
            left: Box::new(lower_expr(left)),
            right: Box::new(lower_expr(right)),
        },
        Expr::Unary { op, operand } => HIRExpr::Unary {
            op: *op,
            operand: Box::new(lower_expr(operand)),
        },
        Expr::Call { func, args } => {
            let func_name = match func.as_ref() {
                Expr::Identifier(name) => name.clone(),
                Expr::Member { base, member } => {
                    if let Expr::Identifier(ns) = base.as_ref() {
                        return HIRExpr::NamespacedCall {
                            namespace: ns.clone(),
                            func: member.clone(),
                            args: args.iter().map(lower_expr).collect(),
                        };
                    }
                    member.clone()
                }
                _ => "call".into(),
            };
            HIRExpr::Call {
                func: func_name,
                args: args.iter().map(lower_expr).collect(),
            }
        }
        Expr::Index { base, index } => HIRExpr::Index {
            base: Box::new(lower_expr(base)),
            index: Box::new(lower_expr(index)),
        },
        Expr::Member { base, member } => HIRExpr::FieldAccess {
            base: Box::new(lower_expr(base)),
            field: member.clone(),
        },
        Expr::Table(fields) => HIRExpr::Construct {
            ty: Type::Unknown,
            fields: fields.iter().map(|(n, e)| (n.clone(), lower_expr(e))).collect(),
        },
        Expr::Array(elements) => HIRExpr::Construct {
            ty: Type::Unknown,
            fields: elements.iter().enumerate().map(|(i, e)| (Some(i.to_string()), lower_expr(e))).collect(),
        },
        Expr::Cast { expr: inner, ty } => HIRExpr::Cast {
            expr: Box::new(lower_expr(inner)),
            ty: ty.clone(),
        },
        Expr::Swizzle { base, components } => HIRExpr::Swizzle {
            base: Box::new(lower_expr(base)),
            components: components.clone(),
        },
        Expr::InterpolatedString(parts) => {
            // Desugar to concatenation
            let mut result = HIRExpr::Literal(Literal::String("".into()));
            for part in parts {
                match part {
                    StringPart::Literal(s) => {
                        result = HIRExpr::Binary {
                            op: BinOp::Concat,
                            left: Box::new(result),
                            right: Box::new(HIRExpr::Literal(Literal::String(s.clone()))),
                        };
                    }
                    StringPart::Expr(e) => {
                        result = HIRExpr::Binary {
                            op: BinOp::Concat,
                            left: Box::new(result),
                            right: Box::new(lower_expr(e)),
                        };
                    }
                }
            }
            result
        }
        Expr::Grouped(inner) => lower_expr(inner),
        Expr::Lambda { .. } => HIRExpr::Literal(Literal::Nil), // Should not appear
        Expr::Ternary { cond, then_branch, else_branch } => HIRExpr::Select {
            cond: Box::new(lower_expr(cond)),
            then_val: Box::new(lower_expr(then_branch)),
            else_val: Box::new(lower_expr(else_branch)),
        },
        Expr::VectorConstructor { ty, args } => {
            let ty = match ty.as_str() {
                "vector2" => Type::Vec2,
                "vector3" => Type::Vec3,
                "vector4" => Type::Vec4,
                "vector2i" => Type::Vec2i,
                "vector3i" => Type::Vec3i,
                "vector4i" => Type::Vec4i,
                "vector2u" => Type::Vec2u,
                "vector3u" => Type::Vec3u,
                "vector4u" => Type::Vec4u,
                "bvector2" => Type::BVec2,
                "bvector3" => Type::BVec3,
                "bvector4" => Type::BVec4,
                _ => Type::Unknown,
            };
            HIRExpr::VectorConstruct {
                ty,
                args: args.iter().map(lower_expr).collect(),
            }
        }
        Expr::MatrixConstructor { ty, args } => {
            let ty = match ty.as_str() {
                "mat2x2" => Type::Mat2x2,
                "mat3x3" => Type::Mat3x3,
                "mat4x4" => Type::Mat4x4,
                _ => Type::Unknown,
            };
            HIRExpr::MatrixConstruct {
                ty,
                args: args.iter().map(lower_expr).collect(),
            }
        }
        Expr::MethodCall { base, method, args } => {
            if let Expr::Identifier(ns) = base.as_ref() {
                // Namespace functions: color.ACESFilm, noise.perlin, bit32.band, geo.raySphere
                let ns_lower = ns.to_lowercase();
                if matches!(ns_lower.as_str(), "color" | "noise" | "bit32" | "geo") {
                    return HIRExpr::NamespacedCall {
                        namespace: ns.clone(),
                        func: method.clone(),
                        args: args.iter().map(lower_expr).collect(),
                    };
                }
                // Vector constructors: vector3.create, bvector2.create
                if ns.starts_with("vector") || ns.starts_with("bvector") {
                    let ty = match ns.as_str() {
                        "vector2" => Type::Vec2,
                        "vector3" => Type::Vec3,
                        "vector4" => Type::Vec4,
                        "vector2i" => Type::Vec2i,
                        "vector3i" => Type::Vec3i,
                        "vector4i" => Type::Vec4i,
                        "vector2u" => Type::Vec2u,
                        "vector3u" => Type::Vec3u,
                        "vector4u" => Type::Vec4u,
                        "bvector2" => Type::BVec2,
                        "bvector3" => Type::BVec3,
                        "bvector4" => Type::BVec4,
                        _ => Type::Unknown,
                    };
                    return HIRExpr::VectorConstruct {
                        ty,
                        args: args.iter().map(lower_expr).collect(),
                    };
                }
                // Matrix constructors: mat4x4.create, etc.
                if ns.starts_with("mat") {
                    let ty = match ns.as_str() {
                        "mat2x2" => Type::Mat2x2,
                        "mat3x3" => Type::Mat3x3,
                        "mat4x4" => Type::Mat4x4,
                        _ => Type::Unknown,
                    };
                    return HIRExpr::MatrixConstruct {
                        ty,
                        args: args.iter().map(lower_expr).collect(),
                    };
                }
            }
            let mut all_args = vec![lower_expr(base)];
            all_args.extend(args.iter().map(lower_expr));
            HIRExpr::Call {
                func: method.clone(),
                args: all_args,
            }
        }
        Expr::Return(_exprs) => HIRExpr::Literal(Literal::Nil), // Should be in Return stmt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaugsl_core::ast::Block;

    #[test]
    fn test_lower_simple() {
        let block = Block {
            stmts: vec![],
            ret: Some(vec![Expr::Literal(Literal::Number(1.0))]),
        };
        let hir = lower_block(&block);
        assert!(matches!(hir[0], HIRStmt::Return(_)));
    }

    #[test]
    fn test_lower_vector_construct() {
        let block = Block {
            stmts: vec![Stmt::LocalDecl {
                names: vec![("v".into(), Some(Type::Vec3))],
                values: vec![Expr::VectorConstructor {
                    ty: "vector3".into(),
                    args: vec![
                        Expr::Literal(Literal::Number(1.0)),
                        Expr::Literal(Literal::Number(2.0)),
                        Expr::Literal(Literal::Number(3.0)),
                    ],
                }],
                attributes: vec![],
            }],
            ret: None,
        };
        let hir = lower_block(&block);
        assert!(matches!(hir[0], HIRStmt::Declare { .. }));
    }
}
