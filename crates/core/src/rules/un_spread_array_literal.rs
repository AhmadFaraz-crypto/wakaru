use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{CallExpr, Expr, ExprOrSpread, Lit, NewExpr, Number, UnaryExpr, UnaryOp};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

/// Inlines spread-over-array-literal in function call arguments.
///
/// `fn(...[a, b, ...c])` → `fn(a, b, ...c)`
///
/// When a spread argument is an array literal, we can inline each element
/// as a separate argument, preserving inner spreads. This eliminates the
/// unnecessary intermediate array allocation.
pub struct UnSpreadArrayLiteral;

impl VisitMut for UnSpreadArrayLiteral {
    fn visit_mut_call_expr(&mut self, call: &mut CallExpr) {
        call.visit_mut_children_with(self);
        inline_spread_array_args(&mut call.args);
    }

    fn visit_mut_new_expr(&mut self, new_expr: &mut NewExpr) {
        new_expr.visit_mut_children_with(self);
        if let Some(args) = &mut new_expr.args {
            inline_spread_array_args(args);
        }
    }
}

/// Walk the args list. For each `...[]` spread of an array literal,
/// inline the array's elements directly as individual arguments.
fn inline_spread_array_args(args: &mut Vec<ExprOrSpread>) {
    let mut needs_inline = false;
    for arg in args.iter() {
        if arg.spread.is_some() && is_transparent_array_literal(arg.expr.as_ref()) {
            needs_inline = true;
            break;
        }
    }

    if !needs_inline {
        return;
    }

    let old = std::mem::take(args);
    for arg in old {
        if arg.spread.is_some() {
            if let Expr::Array(arr) = strip_transparent_types_owned(*arg.expr) {
                // Inline each element of the array literal
                for elem in arr.elems {
                    match elem {
                        Some(eos) => args.push(eos),
                        // Array holes yield the undefined value when spread.
                        // Synthesize `void 0` instead of the identifier
                        // `undefined`: printed output is name-based, so a
                        // local binding named `undefined` could capture the
                        // identifier. RemoveVoid has already run, so the
                        // expression survives to output.
                        None => args.push(ExprOrSpread {
                            spread: None,
                            expr: Box::new(Expr::Unary(UnaryExpr {
                                span: DUMMY_SP,
                                op: UnaryOp::Void,
                                arg: Box::new(Expr::Lit(Lit::Num(Number {
                                    span: DUMMY_SP,
                                    value: 0.0,
                                    raw: None,
                                }))),
                            })),
                        }),
                    }
                }
                continue;
            } else {
                // Not an array literal — keep the spread as-is
                args.push(ExprOrSpread {
                    spread: arg.spread,
                    expr: arg.expr,
                });
                continue;
            }
        }
        args.push(arg);
    }
}

fn is_transparent_array_literal(expr: &Expr) -> bool {
    matches!(strip_transparent_types(expr), Expr::Array(_))
}

fn strip_transparent_types(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren) => strip_transparent_types(&paren.expr),
        Expr::TsAs(wrapper) => strip_transparent_types(&wrapper.expr),
        Expr::TsSatisfies(wrapper) => strip_transparent_types(&wrapper.expr),
        Expr::TsNonNull(wrapper) => strip_transparent_types(&wrapper.expr),
        Expr::TsTypeAssertion(wrapper) => strip_transparent_types(&wrapper.expr),
        Expr::TsInstantiation(wrapper) => strip_transparent_types(&wrapper.expr),
        Expr::TsConstAssertion(wrapper) => strip_transparent_types(&wrapper.expr),
        _ => expr,
    }
}

fn strip_transparent_types_owned(expr: Expr) -> Expr {
    match expr {
        Expr::Paren(paren) => strip_transparent_types_owned(*paren.expr),
        Expr::TsAs(wrapper) => strip_transparent_types_owned(*wrapper.expr),
        Expr::TsSatisfies(wrapper) => strip_transparent_types_owned(*wrapper.expr),
        Expr::TsNonNull(wrapper) => strip_transparent_types_owned(*wrapper.expr),
        Expr::TsTypeAssertion(wrapper) => strip_transparent_types_owned(*wrapper.expr),
        Expr::TsInstantiation(wrapper) => strip_transparent_types_owned(*wrapper.expr),
        Expr::TsConstAssertion(wrapper) => strip_transparent_types_owned(*wrapper.expr),
        expr => expr,
    }
}
