//! Rewrites a self-referential ternary back into the logical operator a
//! minifier expanded it from:
//!
//! ```js
//! x ? x : y            // -> x || y
//! x ? y : x            // -> x && y
//! obj.n ? obj.n : y    // -> obj.n || y
//! arr[i] ? y : arr[i]  // -> arr[i] && y
//! ```
//!
//! `c ? c : y` returns `c` when it is truthy and `y` otherwise, which is exactly
//! `c || y`; `c ? y : c` returns `y` when `c` is truthy and `c` otherwise, which
//! is `c && y`.
//!
//! The fold only fires when the condition is a **side-effect-free place** — a
//! plain identifier, a static member of an identifier (`obj.n`), or a computed
//! member of an identifier with a literal or identifier key (`arr[i]`) — and the
//! repeated branch is that same place. Such a place can be read twice (as the
//! ternary does) or once (as `||`/`&&` do) with identical observable behavior. A
//! call condition, a member on a call result or deeper chain (`a.b.c`), or a
//! computed key that could have side effects is left unchanged, as is any
//! ternary whose branches are not the condition.

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{BinExpr, BinaryOp, Expr, Ident, Lit, MemberExpr, MemberProp};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

pub struct UnSelfTernary;

impl VisitMut for UnSelfTernary {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        expr.visit_mut_children_with(self);

        let replacement = {
            let Expr::Cond(cond) = &*expr else {
                return;
            };
            if !is_side_effect_free_place(&cond.test) {
                return;
            }
            if same_place(&cond.test, &cond.cons) {
                // c ? c : y  ->  c || y
                Some((BinaryOp::LogicalOr, cond.test.clone(), cond.alt.clone()))
            } else if same_place(&cond.test, &cond.alt) {
                // c ? y : c  ->  c && y
                Some((BinaryOp::LogicalAnd, cond.test.clone(), cond.cons.clone()))
            } else {
                None
            }
        };

        if let Some((op, left, right)) = replacement {
            *expr = Expr::Bin(BinExpr {
                span: DUMMY_SP,
                op,
                left,
                right,
            });
        }
    }
}

/// A plain identifier, `ident.prop`, or `ident[literal|identifier]` — a place
/// whose evaluation has no side effect.
fn is_side_effect_free_place(expr: &Expr) -> bool {
    match expr {
        Expr::Ident(_) => true,
        Expr::Member(member) => {
            matches!(member.obj.as_ref(), Expr::Ident(_))
                && match &member.prop {
                    MemberProp::Ident(_) => true,
                    MemberProp::Computed(key) => is_simple_key(&key.expr),
                    MemberProp::PrivateName(_) => false,
                }
        }
        _ => false,
    }
}

fn is_simple_key(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Lit(Lit::Str(_)) | Expr::Lit(Lit::Num(_)) | Expr::Ident(_)
    )
}

/// Whether `a` and `b` are the same side-effect-free place.
fn same_place(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Ident(a), Expr::Ident(b)) => same_ident(a, b),
        (Expr::Member(a), Expr::Member(b)) => member_eq(a, b),
        _ => false,
    }
}

fn member_eq(a: &MemberExpr, b: &MemberExpr) -> bool {
    let (Expr::Ident(a_base), Expr::Ident(b_base)) = (a.obj.as_ref(), b.obj.as_ref()) else {
        return false;
    };
    if !same_ident(a_base, b_base) {
        return false;
    }
    match (&a.prop, &b.prop) {
        (MemberProp::Ident(a_prop), MemberProp::Ident(b_prop)) => a_prop.sym == b_prop.sym,
        (MemberProp::Computed(a_key), MemberProp::Computed(b_key)) => {
            key_eq(&a_key.expr, &b_key.expr)
        }
        _ => false,
    }
}

fn key_eq(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Lit(Lit::Str(a)), Expr::Lit(Lit::Str(b))) => a.value == b.value,
        (Expr::Lit(Lit::Num(a)), Expr::Lit(Lit::Num(b))) => a.value == b.value,
        (Expr::Ident(a), Expr::Ident(b)) => same_ident(a, b),
        _ => false,
    }
}

fn same_ident(a: &Ident, b: &Ident) -> bool {
    a.sym == b.sym && a.ctxt == b.ctxt
}
