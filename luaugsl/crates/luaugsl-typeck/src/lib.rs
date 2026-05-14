use luaugsl_core::ast::*;
use luaugsl_core::error::{CompileError, CompileResult, DiagnosticBag};
use luaugsl_core::types::*;
use std::collections::HashMap;

/// The type checking context.
pub struct TypeChecker {
    pub symbols: HashMap<String, Type>,
    pub globals: HashMap<String, Type>,
    pub errors: DiagnosticBag,
    pub current_stage: Option<ShaderStage>,
    pub loop_depth: u32,
    pub call_graph: Vec<String>,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        // Register standard types
        globals.insert("nil".into(), Type::Nil);
        globals.insert("true".into(), Type::Bool);
        globals.insert("false".into(), Type::Bool);

        TypeChecker {
            symbols: HashMap::new(),
            globals,
            errors: DiagnosticBag::new(),
            current_stage: None,
            loop_depth: 0,
            call_graph: Vec::new(),
        }
    }

    pub fn check_module(&mut self, module: &mut Module) -> Result<(), CompileError> {
        // First pass: collect all declarations
        for binding in &module.bindings {
            self.globals.insert(binding.name.clone(), binding.ty.clone());
        }

        for alias in &module.type_aliases {
            self.globals.insert(alias.name.clone(), alias.ty.clone());
        }

        for func in &module.functions {
            let param_types: Vec<_> = func.params.iter().map(|p| p.ty.clone()).collect();
            let func_ty = Type::Function(param_types, Box::new(func.return_type.clone()));
            self.globals.insert(func.name.clone(), func_ty);
        }

        // Second pass: type check each function body
        for func in &mut module.functions {
            self.current_stage = func.stage;
            self.call_graph.clear();

            if let Err(e) = self.check_function(func) {
                return Err(e);
            }
        }

        if self.errors.has_errors {
            Err(CompileError::General("type checking failed".into()))
        } else {
            Ok(())
        }
    }

    fn check_function(&mut self, func: &mut FunctionDecl) -> CompileResult<()> {
        // Push parameter symbols
        let mut saved = Vec::new();
        for param in &func.params {
            if let Some(existing) = self.symbols.insert(param.name.clone(), param.ty.clone()) {
                saved.push((param.name.clone(), Some(existing)));
            } else {
                saved.push((param.name.clone(), None));
            }
        }

        // Check body
        let _body_ty = self.check_block(&mut func.body)?;

        // Pop parameter symbols
        for (name, old) in saved {
            match old {
                Some(ty) => { self.symbols.insert(name, ty); }
                None => { self.symbols.remove(&name); }
            }
        }

        // Check that return type matches (or void)
        if func.return_type != Type::Nil && func.return_type != Type::Unknown {
            // Body should return the right type
        }

        Ok(())
    }

    fn check_block(&mut self, block: &mut Block) -> CompileResult<Type> {
        for stmt in &mut block.stmts {
            self.check_statement(stmt)?;
        }

        if let Some(ref mut ret) = block.ret {
            let mut ret_types = Vec::new();
            for expr in ret.iter_mut() {
                ret_types.push(self.infer_expr(expr)?);
            }
            if let Some(first) = ret_types.into_iter().next() {
                return Ok(first);
            }
        }

        Ok(Type::Nil)
    }

    fn check_statement(&mut self, stmt: &mut Stmt) -> CompileResult<()> {
        match stmt {
            Stmt::Empty => Ok(()),
            Stmt::LocalDecl { names, values, .. } => {
                for (i, (name, annot)) in names.iter().enumerate() {
                    let inferred = if let Some(val) = values.get_mut(i) {
                        self.infer_expr(val)?
                    } else {
                        Type::Nil
                    };
                    let ty = annot.clone().unwrap_or(inferred);
                    self.symbols.insert(name.clone(), ty);
                }
                Ok(())
            }
            Stmt::ConstDecl { names, values, .. } => {
                for (i, (name, annot)) in names.iter().enumerate() {
                    let inferred = if let Some(val) = values.get_mut(i) {
                        self.infer_expr(val)?
                    } else {
                        Type::Nil
                    };
                    let ty = annot.clone().unwrap_or(inferred);
                    self.symbols.insert(name.clone(), ty);
                }
                Ok(())
            }
            Stmt::Assign { target, value } => {
                let val_ty = self.infer_expr(value)?;
                match target {
                    AssignTarget::Var(name) => {
                        if let Some(sym) = self.symbols.get(name) {
                            // Type consistency check
                            let _ = sym;
                        } else if let Some(sym) = self.globals.get(name) {
                            let _ = sym;
                        }
                    }
                    AssignTarget::Field { .. } => {}
                    AssignTarget::Index { .. } => {}
                    AssignTarget::Swizzle { components, .. } => {
                        // Check for duplicate components
                        let mut seen = std::collections::HashSet::new();
                        for c in &*components {
                            if !seen.insert(*c) {
                                return Err(CompileError::SwizzleDuplicateComponents(
                                    components.iter().collect::<String>(),
                                ));
                            }
                        }
                    }
                }
                let _ = val_ty;
                Ok(())
            }
            Stmt::CompoundAssign { target, value, .. } => {
                let _ = self.infer_expr(value)?;
                match target {
                    AssignTarget::Var(name) => {
                        if self.symbols.contains_key(name) || self.globals.contains_key(name) {
                            // ok
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
            Stmt::Call(expr) => {
                self.infer_expr(expr)?;
                Ok(())
            }
            Stmt::If { cond, then_block, elseifs, else_block } => {
                self.infer_expr(cond)?;
                self.check_block(then_block)?;
                for (c, b) in elseifs {
                    self.infer_expr(c)?;
                    self.check_block(b)?;
                }
                if let Some(b) = else_block {
                    self.check_block(b)?;
                }
                Ok(())
            }
            Stmt::ForNumeric { var, start, end, step, body } => {
                self.loop_depth += 1;
                self.infer_expr(start)?;
                self.infer_expr(end)?;
                if let Some(s) = step {
                    self.infer_expr(s)?;
                }
                self.symbols.insert(var.clone(), Type::I32);
                self.check_block(body)?;
                self.symbols.remove(var);
                self.loop_depth -= 1;
                Ok(())
            }
            Stmt::ForGeneral { vars, expr, body } => {
                self.loop_depth += 1;
                self.infer_expr(expr)?;
                for var in &*vars {
                    self.symbols.insert(var.clone(), Type::Unknown);
                }
                self.check_block(body)?;
                for var in &*vars {
                    self.symbols.remove(var);
                }
                self.loop_depth -= 1;
                Ok(())
            }
            Stmt::While { cond, body } => {
                self.loop_depth += 1;
                self.infer_expr(cond)?;
                self.check_block(body)?;
                self.loop_depth -= 1;
                Ok(())
            }
            Stmt::Repeat { body, cond } => {
                self.loop_depth += 1;
                self.check_block(body)?;
                self.infer_expr(cond)?;
                self.loop_depth -= 1;
                Ok(())
            }
            Stmt::DoBlock(block) => self.check_block(block).map(|_| ()),
            Stmt::Break => Ok(()),
            Stmt::Continue => Ok(()),
            Stmt::Return(exprs) => {
                for e in exprs.iter_mut() {
                    self.infer_expr(e)?;
                }
                Ok(())
            }
            Stmt::TypeAlias(alias) => {
                self.globals.insert(alias.name.clone(), alias.ty.clone());
                Ok(())
            }
            Stmt::Binding(binding) => {
                self.globals.insert(binding.name.clone(), binding.ty.clone());
                Ok(())
            }
        }
    }

    pub fn infer_expr(&mut self, expr: &mut Expr) -> CompileResult<Type> {
        match expr {
            Expr::Literal(lit) => Ok(self.lit_type(lit)),
            Expr::Identifier(name) => {
                if let Some(ty) = self.symbols.get(name) {
                    Ok(ty.clone())
                } else if let Some(ty) = self.globals.get(name) {
                    Ok(ty.clone())
                } else {
                    Err(CompileError::UnknownIdentifier(name.clone()))
                }
            }
            Expr::Binary { op, left, right } => {
                let lty = self.infer_expr(left)?;
                let _rty = self.infer_expr(right)?;
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::FloorDiv
                    | BinOp::Mod | BinOp::Pow => {
                        if lty.is_numeric_scalar() || lty.is_float_vector() || lty.is_matrix() {
                            Ok(lty.clone())
                        } else {
                            Ok(Type::F32)
                        }
                    }
                    BinOp::Concat => Ok(Type::String),
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Ok(Type::Bool)
                    }
                    BinOp::And | BinOp::Or => Ok(Type::Bool),
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
                        Ok(Type::U32)
                    }
                }
            }
            Expr::Unary { op, operand } => {
                let ty = self.infer_expr(operand)?;
                match op {
                    UnOp::Not => Ok(Type::Bool),
                    UnOp::Neg => Ok(ty),
                    UnOp::Len => Ok(Type::I32),
                    UnOp::BitNot => Ok(Type::U32),
                }
            }
            Expr::Call { func, args } => {
                // Resolve intrinsic FIRST — before infer_expr(func) which would error on unknown identifiers
                let intrinsic_ret = match func.as_ref() {
                    Expr::Identifier(name) => resolve_intrinsic_return(name, args.len()),
                    Expr::Member { base, member } => {
                        if let Expr::Identifier(ns) = base.as_ref() {
                            let full_name = format!("{}.{}", ns, member);
                            resolve_intrinsic_return(&full_name, args.len())
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                for arg in args.iter_mut() {
                    self.infer_expr(arg)?;
                }
                if let Some(ret) = intrinsic_ret {
                    return Ok(ret);
                }
                let func_ty = self.infer_expr(func)?;
                match func_ty {
                    Type::Function(_, ret) => Ok(*ret),
                    _ => Ok(Type::Unknown),
                }
            }
            Expr::Index { base, index } => {
                self.infer_expr(base)?;
                self.infer_expr(index)?;
                Ok(Type::Unknown)
            }
            Expr::Member { base, member } => {
                // Check for namespaced intrinsic field (e.g. color.ACESFilm used as value) before
                // calling infer_expr(base) which would fail for unknown namespace identifiers
                if let Expr::Identifier(ns) = base.as_ref() {
                    let full_name = format!("{}.{}", ns, member);
                    if let Some(ret) = resolve_intrinsic_return(&full_name, 0) {
                        return Ok(ret);
                    }
                }
                let base_ty = self.infer_expr(base)?;
                // Swizzle check
                if base_ty.is_float_vector() && is_swizzle(member) {
                    match member.len() {
                        1 => Ok(Type::F32),
                        2 => Ok(Type::Vec2),
                        3 => Ok(Type::Vec3),
                        4 => Ok(Type::Vec4),
                        _ => Ok(Type::Unknown),
                    }
                } else {
                    Ok(Type::Unknown)
                }
            }
            Expr::Table(fields) => {
                let field_defs: Vec<FieldDef> = fields
                    .iter_mut()
                    .map(|(name, val)| FieldDef {
                        name: name.clone().unwrap_or_default(),
                        ty: self.infer_expr(val).unwrap_or(Type::Unknown),
                        access: None,
                        offset: None,
                        align: None,
                        location: None,
                        interpolate: None,
                        builtin: None,
                    })
                    .collect();
                Ok(Type::Table(field_defs, None))
            }
            Expr::Array(elements) => {
                let elem_ty = if let Some(e) = elements.first_mut() {
                    self.infer_expr(e)?
                } else {
                    Type::Unknown
                };
                Ok(Type::Array(Box::new(elem_ty), Some(elements.len() as u32)))
            }
            Expr::Cast { expr: inner, ty } => {
                self.infer_expr(inner)?;
                Ok(ty.clone())
            }
            Expr::Swizzle { base, components } => {
                let base_ty = self.infer_expr(base)?;
                match components.len() {
                    1 => Ok(base_ty.vector_scalar_type().unwrap_or(Type::F32)),
                    2 => {
                        let scalar = base_ty.vector_scalar_type().unwrap_or(Type::F32);
                        Ok(match scalar {
                            Type::F32 => Type::Vec2,
                            Type::I32 => Type::Vec2i,
                            Type::U32 => Type::Vec2u,
                            Type::Bool => Type::BVec2,
                            _ => Type::Vec2,
                        })
                    }
                    3 => {
                        let scalar = base_ty.vector_scalar_type().unwrap_or(Type::F32);
                        Ok(match scalar {
                            Type::F32 => Type::Vec3,
                            Type::I32 => Type::Vec3i,
                            Type::U32 => Type::Vec3u,
                            Type::Bool => Type::BVec3,
                            _ => Type::Vec3,
                        })
                    }
                    4 => {
                        let scalar = base_ty.vector_scalar_type().unwrap_or(Type::F32);
                        Ok(match scalar {
                            Type::F32 => Type::Vec4,
                            Type::I32 => Type::Vec4i,
                            Type::U32 => Type::Vec4u,
                            Type::Bool => Type::BVec4,
                            _ => Type::Vec4,
                        })
                    }
                    _ => Ok(Type::Unknown),
                }
            }
            Expr::InterpolatedString(_) => Ok(Type::String),
            Expr::Grouped(inner) => self.infer_expr(inner),
            Expr::Lambda { body, .. } => self.infer_expr(body),
            Expr::Ternary { cond, then_branch, else_branch } => {
                self.infer_expr(cond)?;
                let t_ty = self.infer_expr(then_branch)?;
                let e_ty = self.infer_expr(else_branch)?;
                if t_ty == e_ty {
                    Ok(t_ty)
                } else {
                    Ok(Type::Unknown)
                }
            }
            Expr::VectorConstructor { ty, args } => {
                for a in args.iter_mut() {
                    self.infer_expr(a)?;
                }
                match ty.as_str() {
                    "vector2" => Ok(Type::Vec2),
                    "vector3" => Ok(Type::Vec3),
                    "vector4" => Ok(Type::Vec4),
                    "vector2i" => Ok(Type::Vec2i),
                    "vector3i" => Ok(Type::Vec3i),
                    "vector4i" => Ok(Type::Vec4i),
                    "vector2u" => Ok(Type::Vec2u),
                    "vector3u" => Ok(Type::Vec3u),
                    "vector4u" => Ok(Type::Vec4u),
                    "bvector2" => Ok(Type::BVec2),
                    "bvector3" => Ok(Type::BVec3),
                    "bvector4" => Ok(Type::BVec4),
                    _ => Ok(Type::Unknown),
                }
            }
            Expr::MatrixConstructor { ty, .. } => {
                match ty.as_str() {
                    "mat2x2" => Ok(Type::Mat2x2),
                    "mat3x3" => Ok(Type::Mat3x3),
                    "mat4x4" => Ok(Type::Mat4x4),
                    _ => Ok(Type::Unknown),
                }
            }
            Expr::MethodCall { base, method: _, args } => {
                self.infer_expr(base)?;
                for a in args.iter_mut() {
                    self.infer_expr(a)?;
                }
                Ok(Type::Unknown)
            }
            Expr::Return(exprs) => {
                for e in exprs.iter_mut() {
                    self.infer_expr(e)?;
                }
                Ok(Type::Nil)
            }
        }
    }

    fn lit_type(&self, lit: &Literal) -> Type {
        match lit {
            Literal::Nil => Type::Nil,
            Literal::Bool(_) => Type::Bool,
            Literal::Number(n) => {
                if n.fract() == 0.0 && *n >= 0.0 && *n <= (u32::MAX as f64) {
                    // Could be i32, u32, or f32 — default to f32 for GPU
                    Type::F32
                } else {
                    Type::F32
                }
            }
            Literal::String(_) => Type::String,
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

fn is_swizzle(s: &str) -> bool {
    s.chars().all(|c| matches!(c, 'x' | 'y' | 'z' | 'w' | 'r' | 'g' | 'b' | 'a' | 's' | 't' | 'p' | 'q'))
        && !s.is_empty()
        && s.len() <= 4
}

/// Quick resolution of intrinsic return types.
fn resolve_intrinsic_return(name: &str, _arg_count: usize) -> Option<Type> {
    let result = match name {
        // Scalar math -> f32
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2" | "sinh" | "cosh" | "tanh"
        | "sqrt" | "rsqrt" | "pow" | "exp" | "exp2" | "log" | "log2"
        | "abs" | "sign" | "floor" | "ceil" | "round" | "trunc" | "fract"
        | "mod" | "min" | "max" | "clamp" | "lerp" | "smoothstep" | "step" | "fma" => Type::F32,

        // Geometric -> f32
        "length" | "distance" | "dot" => Type::F32,

        // Cross product -> vec3
        "cross" | "normalize" | "reflect" | "refract" | "faceforward" => Type::Vec3,

        // Vector constructors
        "vector2.create" => Type::Vec2,
        "vector3.create" => Type::Vec3,
        "vector4.create" => Type::Vec4,
        "vector2i.create" => Type::Vec2i,
        "vector3i.create" => Type::Vec3i,
        "vector4i.create" => Type::Vec4i,
        "vector2u.create" => Type::Vec2u,
        "vector3u.create" => Type::Vec3u,
        "vector4u.create" => Type::Vec4u,
        "bvector2.create" => Type::BVec2,
        "bvector3.create" => Type::BVec3,
        "bvector4.create" => Type::BVec4,

        // Matrix constructors
        "mat2x2.create" => Type::Mat2x2,
        "mat3x3.create" => Type::Mat3x3,
        "mat4x4.create" => Type::Mat4x4,

        // Transforms
        "mat4x4.createTRS" | "mat4x4.createTranslation" | "mat4x4.createRotation"
        | "mat4x4.createScale" | "mat4x4.createLookAt" | "mat4x4.createPerspective"
        | "mat4x4.createOrthographic" | "transpose" | "inverse" => Type::Mat4x4,

        "determinant" => Type::F32,

        // Texture sampling -> vec4
        "sample" | "sampleLod" | "sampleGrad" | "sampleBias" | "textureFetch" | "imageRead" => Type::Vec4,
        "sampleCompare" => Type::F32,

        "imageDimensions" | "getDimensions" => Type::Vec2i,
        "getMipLevels" | "bufferLength" => Type::U32,

        // Buffer -> generic, but return u32 for atomic ops
        "atomicAdd" | "atomicSub" | "atomicMin" | "atomicMax" | "atomicAnd"
        | "atomicOr" | "atomicXor" | "atomicExchange" | "atomicCompareExchange" => Type::U32,

        // Bit32 -> u32
        "bit32.band" | "bit32.bor" | "bit32.bxor" | "bit32.bnot" | "bit32.lshift"
        | "bit32.rshift" | "bit32.arshift" | "bit32.lrotate" | "bit32.rrotate"
        | "bit32.extract" | "bit32.replace" | "bit32.countlz" | "bit32.counttz"
        | "bit32.countbits" | "bit32.reverse" | "bit32.byteswap" | "bit32.packf32"
        | "bit32.packUnorm4x8" | "bit32.packSnorm4x8" | "bit32.packHalf2x16"
        | "bit32.packUint2x16" | "bit32.unpackf32" | "bit32.unpackUnorm4x8"
        | "bit32.unpackSnorm4x8" | "bit32.unpackHalf2x16" | "bit32.unpackUint2x16"
        | "hash" => Type::U32,

        // Noise -> f32 or vec2
        "noise.perlin" | "noise.simplex" | "noise.worleyF1" | "noise.worleyF2"
        | "noise.fbm" | "noise.turbulence" | "noise.ridge" | "noise.value" | "noise.hashf" | "noise.white" => Type::F32,
        "noise.worley" | "noise.gradient" => Type::Vec2,

        // Color -> vec3
        "color.sRGBToLinear" | "color.linearTosRGB" | "color.ACESFilm" | "color.reinhard"
        | "color.uncharted2" | "color.contrast" | "color.saturation" | "color.brightness"
        | "color.hueShift" => Type::Vec3,
        "color.luminance" => Type::F32,
        "color.blend" => Type::Vec4,

        // Geo -> various
        "geo.raySphere" | "geo.barycentric" | "geo.closestPointOnSegment"
        | "geo.sampleHemisphereCosine" | "geo.sampleHemisphereUniform" | "geo.sampleSphereUniform" => Type::Vec3,
        "geo.rayAABB" | "geo.rayPlane" => Type::Vec2,
        "geo.rayTriangle" => Type::Vec3,
        "geo.pointInTriangle" | "geo.sphereAABB" | "geo.aabbAABB" | "geo.subgroupElect"
        | "geo.subgroupAll" | "geo.subgroupAny" | "any" | "all" => Type::Bool,
        "geo.distancePointToLine" | "geo.distancePointToSegment" => Type::F32,
        "geo.sampleDiskConcentric" => Type::Vec2,

        // Utility
        "select" | "saturate" | "dFdx" | "dFdy" | "fwidth" => Type::F32,
        "isinf" | "isnan" | "isfinite" => Type::Bool,
        "subgroupAdd" | "subgroupMin" | "subgroupMax" | "subgroupAnd"
        | "subgroupOr" | "subgroupXor" | "subgroupBroadcast" | "subgroupShuffle" => Type::F32,
        "subgroupBallot" => Type::Vec4u,

        _ => return None,
    };
    Some(result)
}
