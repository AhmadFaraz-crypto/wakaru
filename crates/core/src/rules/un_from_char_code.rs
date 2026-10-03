//! Folds a constant `String.fromCharCode(...)` call into the string literal it
//! produces.
//!
//! Minifiers and obfuscators hide string literals behind `String.fromCharCode`:
//!
//! ```js
//! String.fromCharCode(72, 101, 108, 108, 111) // -> "Hello"
//! ```
//!
//! Each argument is a UTF-16 **code unit** (not a code point), so a surrogate
//! pair such as `fromCharCode(0xD83D, 0xDE00)` decodes to a single astral
//! character (`"\u{1F600}"`).
//!
//! The fold only fires when it is provably sound and representable:
//! - the callee is the **global** `String.fromCharCode` (a shadowed local
//!   `String`, or `fromCharCode` on any other object, is left alone);
//! - every argument is a non-negative integer literal `<= 0xFFFF` (a variable,
//!   a spread, a negative/fractional/out-of-range number — all of which
//!   JavaScript would coerce with `ToUint16` — are left alone rather than
//!   emulated);
//! - the resulting code-unit sequence is valid UTF-16 (a lone surrogate, which
//!   has no UTF-8 representation, is left alone).

use swc_core::common::{Mark, DUMMY_SP};
use swc_core::ecma::ast::{Callee, Expr, Lit, MemberProp, Str};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

pub struct UnFromCharCode {
    unresolved_mark: Mark,
}

impl UnFromCharCode {
    pub fn new(unresolved_mark: Mark) -> Self {
        Self { unresolved_mark }
    }

    /// If `expr` is a constant global `String.fromCharCode(...)` call, return
    /// the decoded string.
    fn decode(&self, expr: &Expr) -> Option<String> {
        let Expr::Call(call) = expr else {
            return None;
        };
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Member(member) = callee.as_ref() else {
            return None;
        };
        // The receiver must be the global `String`, not a shadowing local.
        let Expr::Ident(object) = member.obj.as_ref() else {
            return None;
        };
        if object.sym != "String" || object.ctxt.outer() != self.unresolved_mark {
            return None;
        }
        let MemberProp::Ident(method) = &member.prop else {
            return None;
        };
        if method.sym != "fromCharCode" {
            return None;
        }

        let mut units: Vec<u16> = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            if arg.spread.is_some() {
                return None;
            }
            let Expr::Lit(Lit::Num(number)) = arg.expr.as_ref() else {
                return None;
            };
            let value = number.value;
            if value < 0.0 || value.fract() != 0.0 || value > u16::MAX as f64 {
                return None;
            }
            units.push(value as u16);
        }

        // `Err` here means a lone surrogate, which has no UTF-8 form: bail.
        String::from_utf16(&units).ok()
    }
}

impl VisitMut for UnFromCharCode {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        expr.visit_mut_children_with(self);

        if let Some(decoded) = self.decode(expr) {
            *expr = Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: decoded.into(),
                raw: None,
            }));
        }
    }
}
