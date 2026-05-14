use luaugsl_hir::{HIRExpr, HIRFunction, HIRModule, HIRStmt};
use luaugsl_core::ast::Literal;

/// Simplified optimization pipeline.
/// In a full implementation, this would include dead code elimination,
/// constant folding, loop unrolling, algebraic simplification, etc.
pub fn optimize_module(module: &mut HIRModule) {
    for func in &mut module.functions {
        optimize_function(func);
    }
}

fn optimize_function(func: &mut HIRFunction) {
    let mut changed = true;
    while changed {
        changed = false;
        changed |= constant_fold_stmts(&mut func.body);
        changed |= remove_dead_code(&mut func.body);
        changed |= simplify_expressions(&mut func.body);
    }
}

// === Constant Folding ===

fn constant_fold_stmts(stmts: &mut Vec<HIRStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HIRStmt::Declare { value, .. } => {
                changed |= constant_fold_expr(value);
            }
            HIRStmt::Assign { value, .. } => {
                changed |= constant_fold_expr(value);
            }
            HIRStmt::CompoundAssign { value, .. } => {
                changed |= constant_fold_expr(value);
            }
            HIRStmt::Expr(expr) => {
                changed |= constant_fold_expr(expr);
            }
            HIRStmt::If { cond, then_branch, else_branch } => {
                changed |= constant_fold_expr(cond);
                changed |= constant_fold_stmts(then_branch);
                changed |= constant_fold_stmts(else_branch);
                // If cond is literal, convert to unconditional
                if let HIRExpr::Literal(Literal::Bool(true)) = cond {
                    *stmt = HIRStmt::Block(then_branch.clone());
                    changed = true;
                } else if let HIRExpr::Literal(Literal::Bool(false)) = cond {
                    *stmt = HIRStmt::Block(else_branch.clone());
                    changed = true;
                }
            }
            HIRStmt::Loop { cond, body } => {
                if let Some(c) = cond {
                    changed |= constant_fold_expr(c);
                }
                changed |= constant_fold_stmts(body);
            }
            HIRStmt::For { start: _, end, step, body, .. } => {
                changed |= constant_fold_expr(end);
                changed |= constant_fold_expr(step);
                changed |= constant_fold_stmts(body);
            }
            HIRStmt::Return(exprs) => {
                for e in exprs {
                    changed |= constant_fold_expr(e);
                }
            }
            _ => {}
        }
    }
    changed
}

fn constant_fold_expr(expr: &mut HIRExpr) -> bool {
    let result = match expr {
        HIRExpr::Binary { op, left, right } => {
            let l_changed = constant_fold_expr(left);
            let r_changed = constant_fold_expr(right);
            let changed = l_changed || r_changed;

            // Try to fold
            if let (HIRExpr::Literal(l_lit), HIRExpr::Literal(r_lit)) = (left.as_ref(), right.as_ref()) {
                if let Some(result) = fold_binary(*op, l_lit, r_lit) {
                    *expr = HIRExpr::Literal(result);
                    return true;
                }
            }

            changed
        }
        HIRExpr::Unary { op, operand } => {
            let changed = constant_fold_expr(operand);
            if let HIRExpr::Literal(lit) = operand.as_ref() {
                if let Some(result) = fold_unary(*op, lit) {
                    *expr = HIRExpr::Literal(result);
                    return true;
                }
            }
            changed
        }
        HIRExpr::Select { cond, then_val, else_val } => {
            let c = constant_fold_expr(cond);
            let t = constant_fold_expr(then_val);
            let e = constant_fold_expr(else_val);
            if let HIRExpr::Literal(Literal::Bool(true)) = cond.as_ref() {
                let folded = then_val.as_ref().clone();
                *expr = folded;
                return true;
            }
            if let HIRExpr::Literal(Literal::Bool(false)) = cond.as_ref() {
                let folded = else_val.as_ref().clone();
                *expr = folded;
                return true;
            }
            c || t || e
        }
        HIRExpr::FieldAccess { base, .. } => {
            constant_fold_expr(base)
        }
        HIRExpr::Index { base, index } => {
            constant_fold_expr(base) || constant_fold_expr(index)
        }
        HIRExpr::Cast { expr: inner, .. } => {
            constant_fold_expr(inner)
        }
        HIRExpr::Swizzle { base, .. } => {
            constant_fold_expr(base)
        }
        HIRExpr::Call { args, .. } | HIRExpr::NamespacedCall { args, .. } => {
            let mut changed = false;
            for a in args {
                changed |= constant_fold_expr(a);
            }
            changed
        }
        HIRExpr::VectorConstruct { args, .. } | HIRExpr::MatrixConstruct { args, .. } => {
            let mut changed = false;
            for a in args {
                changed |= constant_fold_expr(a);
            }
            changed
        }
        HIRExpr::Construct { fields, .. } => {
            let mut changed = false;
            for (_, a) in fields {
                changed |= constant_fold_expr(a);
            }
            changed
        }
        _ => false,
    };
    result
}

fn fold_binary(op: luaugsl_core::ast::BinOp, left: &Literal, right: &Literal) -> Option<Literal> {
    match (left, right) {
        (Literal::Number(l), Literal::Number(r)) => {
            let result = match op {
                luaugsl_core::ast::BinOp::Add => l + r,
                luaugsl_core::ast::BinOp::Sub => l - r,
                luaugsl_core::ast::BinOp::Mul => l * r,
                luaugsl_core::ast::BinOp::Div if *r != 0.0 => l / r,
                luaugsl_core::ast::BinOp::FloorDiv if *r != 0.0 => (l / r).floor(),
                luaugsl_core::ast::BinOp::Mod if *r != 0.0 => l % r,
                luaugsl_core::ast::BinOp::Pow => l.powf(*r),
                luaugsl_core::ast::BinOp::Eq => return Some(Literal::Bool((l - r).abs() < 1e-10)),
                luaugsl_core::ast::BinOp::NotEq => return Some(Literal::Bool((l - r).abs() >= 1e-10)),
                luaugsl_core::ast::BinOp::Lt => return Some(Literal::Bool(l < r)),
                luaugsl_core::ast::BinOp::Le => return Some(Literal::Bool(l <= r)),
                luaugsl_core::ast::BinOp::Gt => return Some(Literal::Bool(l > r)),
                luaugsl_core::ast::BinOp::Ge => return Some(Literal::Bool(l >= r)),
                _ => return None,
            };
            Some(Literal::Number(result))
        }
        (Literal::Bool(l), Literal::Bool(r)) => {
            match op {
                luaugsl_core::ast::BinOp::And => Some(Literal::Bool(*l && *r)),
                luaugsl_core::ast::BinOp::Or => Some(Literal::Bool(*l || *r)),
                luaugsl_core::ast::BinOp::Eq => Some(Literal::Bool(l == r)),
                luaugsl_core::ast::BinOp::NotEq => Some(Literal::Bool(l != r)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn fold_unary(op: luaugsl_core::ast::UnOp, operand: &Literal) -> Option<Literal> {
    match (op, operand) {
        (luaugsl_core::ast::UnOp::Neg, Literal::Number(n)) => Some(Literal::Number(-n)),
        (luaugsl_core::ast::UnOp::Not, Literal::Bool(b)) => Some(Literal::Bool(!b)),
        (luaugsl_core::ast::UnOp::Not, Literal::Nil) => Some(Literal::Bool(true)),
        (luaugsl_core::ast::UnOp::Not, Literal::Number(n)) => Some(Literal::Bool(*n == 0.0)),
        (luaugsl_core::ast::UnOp::Len, Literal::String(s)) => Some(Literal::Number(s.len() as f64)),
        (luaugsl_core::ast::UnOp::BitNot, Literal::Number(n)) => Some(Literal::Number((!(*n as i64)) as f64)),
        _ => None,
    }
}

// === Dead Code Elimination ===

fn remove_dead_code(stmts: &mut Vec<HIRStmt>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let is_terminal = matches!(&stmts[i], HIRStmt::Return(_) | HIRStmt::Break | HIRStmt::Continue);
        if is_terminal {
            if i + 1 < stmts.len() {
                stmts.truncate(i + 1);
                changed = true;
            }
            break;
        }
        match &mut stmts[i] {
            HIRStmt::If { then_branch, else_branch, .. } => {
                if remove_dead_code(then_branch) { changed = true; }
                if remove_dead_code(else_branch) { changed = true; }
            }
            HIRStmt::Loop { body, .. } | HIRStmt::For { body, .. } => {
                if remove_dead_code(body) { changed = true; }
            }
            _ => {}
        }
        i += 1;
    }
    changed
}

// === Expression Simplification ===

fn simplify_expressions(stmts: &mut Vec<HIRStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HIRStmt::Declare { value, .. } => changed |= simplify_expr(value),
            HIRStmt::Assign { value, .. } => changed |= simplify_expr(value),
            HIRStmt::CompoundAssign { value, .. } => changed |= simplify_expr(value),
            HIRStmt::Expr(expr) => changed |= simplify_expr(expr),
            HIRStmt::If { cond, then_branch, else_branch } => {
                changed |= simplify_expr(cond);
                changed |= simplify_expressions(then_branch);
                changed |= simplify_expressions(else_branch);
            }
            HIRStmt::Loop { cond, body } => {
                if let Some(c) = cond { changed |= simplify_expr(c); }
                changed |= simplify_expressions(body);
            }
            HIRStmt::For { start, end, step, body, .. } => {
                changed |= simplify_expr(start);
                changed |= simplify_expr(end);
                changed |= simplify_expr(step);
                changed |= simplify_expressions(body);
            }
            HIRStmt::Return(exprs) => {
                for e in exprs { changed |= simplify_expr(e); }
            }
            _ => {}
        }
    }
    changed
}

fn is_num(e: &HIRExpr, val: f64) -> bool {
    matches!(e, HIRExpr::Literal(Literal::Number(n)) if *n == val)
}

fn simplify_expr(expr: &mut HIRExpr) -> bool {
    match expr {
        HIRExpr::Binary { op: luaugsl_core::ast::BinOp::Mul, left, right } => {
            let mut changed = simplify_expr(left) || simplify_expr(right);
            // x * 1 -> x, 1 * x -> x
            if is_num(right, 1.0) {
                *expr = *left.clone();
                changed = true;
            } else if is_num(left, 1.0) {
                *expr = *right.clone();
                changed = true;
            } else if is_num(left, 0.0) {
                *expr = HIRExpr::Literal(Literal::Number(0.0));
                changed = true;
            }
            changed
        }
        HIRExpr::Binary { op: luaugsl_core::ast::BinOp::Add, left, right } => {
            let mut changed = simplify_expr(left) || simplify_expr(right);
            // x + 0 -> x, 0 + x -> x
            if is_num(right, 0.0) {
                *expr = *left.clone();
                changed = true;
            } else if is_num(left, 0.0) {
                *expr = *right.clone();
                changed = true;
            }
            changed
        }
        HIRExpr::Binary { op: luaugsl_core::ast::BinOp::Sub, left, right } => {
            let mut changed = simplify_expr(left) || simplify_expr(right);
            // x - 0 -> x
            if is_num(right, 0.0) {
                *expr = *left.clone();
                changed = true;
            }
            changed
        }
        HIRExpr::Binary { op: luaugsl_core::ast::BinOp::Div, left, right } => {
            let mut changed = simplify_expr(left) || simplify_expr(right);
            // x / 1 -> x
            if is_num(right, 1.0) {
                *expr = *left.clone();
                changed = true;
            }
            changed
        }
        HIRExpr::Binary { left, right, .. } => {
            simplify_expr(left) || simplify_expr(right)
        }
        HIRExpr::Unary { operand, .. } => simplify_expr(operand),
        HIRExpr::Select { cond, then_val, else_val } => {
            simplify_expr(cond) || simplify_expr(then_val) || simplify_expr(else_val)
        }
        HIRExpr::Cast { expr: inner, .. } => simplify_expr(inner),
        HIRExpr::FieldAccess { base, .. } => simplify_expr(base),
        HIRExpr::Index { base, index } => simplify_expr(base) || simplify_expr(index),
        HIRExpr::Swizzle { base, .. } => simplify_expr(base),
        HIRExpr::Call { args, .. } | HIRExpr::NamespacedCall { args, .. } => {
            args.iter_mut().fold(false, |acc, a| acc || simplify_expr(a))
        }
        HIRExpr::VectorConstruct { args, .. } | HIRExpr::MatrixConstruct { args, .. } => {
            args.iter_mut().fold(false, |acc, a| acc || simplify_expr(a))
        }
        HIRExpr::Construct { fields, .. } => {
            fields.iter_mut().fold(false, |acc, (_, a)| acc || simplify_expr(a))
        }
        _ => false,
    }
}

// HIRStmt::Block is used during optimization but doesn't exist — let me add it

// Actually, the constant_fold_stmts references HIRStmt::Block which doesn't exist in the HIR enum.
// Let me fix that by removing the Block variant reference and using a different approach.
// The dead code elimination for If already handles it via truncation.
