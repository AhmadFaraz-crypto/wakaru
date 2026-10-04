//! Folds a self-referential assignment back into a compound assignment:
//!
//! ```js
//! x = x + 1;        // -> x += 1
//! total = total * k // -> total *= k
//! obj.n = obj.n - 1 // -> obj.n -= 1
//! ```
//!
//! The fold fires only when it preserves behavior exactly:
//! - the assignment target is the **left** operand of the binary expression.
//!   `x = 1 + x` is left alone, because `x += 1` is `x = x + 1`, which differs
//!   for strings (`"a" = 1 + "a"` is `"1a"`, but `"a" += 1` is `"a1"`) and for
//!   every other non-commutative operator;
//! - the target is a side-effect-free place: a plain identifier, a static member
//!   of an identifier (`obj.n`), or a computed member of an identifier with a
//!   literal or identifier key (`obj[k]`). A member on a call result
//!   (`get().n = get().n + 1`) or a side-effecting computed key (`o[k()]`) would
//!   be evaluated once by `+=` but twice by the expanded form, so it is left;
//! - the operator is an arithmetic or bitwise one that has a compound form
//!   (`+ - * / % ** << >> >>> & | ^`). Logical `&& || ??` are handled by the
//!   logical-assignment rule and are not folded here.

use swc_core::ecma::ast::{
    AssignOp, AssignTarget, BinExpr, BinaryOp, Expr, Ident, Lit, MemberExpr, MemberProp,
    SimpleAssignTarget,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

pub struct UnCompoundAssignment;

impl VisitMut for UnCompoundAssignment {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        expr.visit_mut_children_with(self);

        let Expr::Assign(assign) = expr else {
            return;
        };
        if assign.op != AssignOp::Assign {
            return;
        }

        // Inspect the `= <left> <op> <rhs>` shape without moving anything yet.
        let (compound_op, new_right) = {
            let Expr::Bin(BinExpr {
                op, left, right, ..
            }) = assign.right.as_ref()
            else {
                return;
            };
            let Some(compound_op) = compound_assign_op(*op) else {
                return;
            };
            let AssignTarget::Simple(target) = &assign.left else {
                return;
            };
            if !place_matches(target, left) {
                return;
            }
            (compound_op, right.clone())
        };

        assign.op = compound_op;
        assign.right = new_right;
    }
}

/// Map a binary operator to its compound-assignment form, or `None` for
/// operators without one (comparisons, logical operators, `in`, `instanceof`).
fn compound_assign_op(op: BinaryOp) -> Option<AssignOp> {
    Some(match op {
        BinaryOp::Add => AssignOp::AddAssign,
        BinaryOp::Sub => AssignOp::SubAssign,
        BinaryOp::Mul => AssignOp::MulAssign,
        BinaryOp::Div => AssignOp::DivAssign,
        BinaryOp::Mod => AssignOp::ModAssign,
        BinaryOp::Exp => AssignOp::ExpAssign,
        BinaryOp::LShift => AssignOp::LShiftAssign,
        BinaryOp::RShift => AssignOp::RShiftAssign,
        BinaryOp::ZeroFillRShift => AssignOp::ZeroFillRShiftAssign,
        BinaryOp::BitAnd => AssignOp::BitAndAssign,
        BinaryOp::BitOr => AssignOp::BitOrAssign,
        BinaryOp::BitXor => AssignOp::BitXorAssign,
        _ => return None,
    })
}

/// Whether the assignment target is the same side-effect-free place as `expr`
/// (the left operand of the binary expression).
fn place_matches(target: &SimpleAssignTarget, expr: &Expr) -> bool {
    match (target, expr) {
        (SimpleAssignTarget::Ident(target), Expr::Ident(other)) => same_ident(&target.id, other),
        (SimpleAssignTarget::Member(target), Expr::Member(other)) => member_matches(target, other),
        _ => false,
    }
}

fn member_matches(target: &MemberExpr, other: &MemberExpr) -> bool {
    // The base must be the same plain identifier (so it carries no side effect
    // and is evaluated identically by both forms).
    let (Expr::Ident(target_base), Expr::Ident(other_base)) =
        (target.obj.as_ref(), other.obj.as_ref())
    else {
        return false;
    };
    if !same_ident(target_base, other_base) {
        return false;
    }
    match (&target.prop, &other.prop) {
        (MemberProp::Ident(target_prop), MemberProp::Ident(other_prop)) => {
            target_prop.sym == other_prop.sym
        }
        (MemberProp::Computed(target_key), MemberProp::Computed(other_key)) => {
            side_effect_free_key_matches(&target_key.expr, &other_key.expr)
        }
        _ => false,
    }
}

/// Computed keys match only when they are identical **and** side-effect-free (a
/// literal or an identifier). A call or other expression never matches, so
/// `o[k()] = o[k()] + 1` is left alone.
fn side_effect_free_key_matches(target: &Expr, other: &Expr) -> bool {
    match (target, other) {
        (Expr::Lit(Lit::Str(a)), Expr::Lit(Lit::Str(b))) => a.value == b.value,
        (Expr::Lit(Lit::Num(a)), Expr::Lit(Lit::Num(b))) => a.value == b.value,
        (Expr::Ident(a), Expr::Ident(b)) => same_ident(a, b),
        _ => false,
    }
}

fn same_ident(a: &Ident, b: &Ident) -> bool {
    a.sym == b.sym && a.ctxt == b.ctxt
}
