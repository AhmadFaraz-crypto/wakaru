//! Recovers `export * from "..."` from TypeScript/tslib's `__exportStar` helper.
//!
//! The TypeScript compiler (with `importHelpers`) lowers a re-export
//!
//! ```ts
//! export * from "./mod";
//! ```
//!
//! to a call against tslib's `__exportStar` helper in its CommonJS output:
//!
//! ```js
//! __exportStar(require("./mod"), exports);
//! // or, via the tslib namespace:
//! tslib_1.__exportStar(require("./mod"), exports);
//! ```
//!
//! This rule recognizes a top-level `__exportStar(require("<spec>"), exports)`
//! call (bare or as a member of any object) and rewrites it back to
//! `export * from "<spec>"`. Resolves
//! <https://github.com/pionxzh/wakaru/issues/55>.
//!
//! It fires only on that exact shape: the callee's method name is
//! `__exportStar`, the first argument is a call to the global `require` with a
//! string-literal specifier, and the second argument is the global `exports`
//! object. Anything else is left unchanged.

use swc_core::common::{Mark, DUMMY_SP};
use swc_core::ecma::ast::{
    Callee, ExportAll, Expr, ExprStmt, Lit, MemberProp, Module, ModuleDecl, ModuleItem, Stmt, Str,
};

/// Rewrite every top-level `__exportStar(require("..."), exports)` into an
/// `export * from "..."` declaration.
pub(crate) fn run(module: &mut Module, unresolved_mark: Mark) {
    for item in &mut module.body {
        let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = item else {
            continue;
        };
        let Some(spec) = export_star_specifier(expr, unresolved_mark) else {
            continue;
        };
        *item = ModuleItem::ModuleDecl(ModuleDecl::ExportAll(ExportAll {
            span: DUMMY_SP,
            src: Box::new(Str {
                span: DUMMY_SP,
                value: spec.into(),
                raw: None,
            }),
            type_only: false,
            with: None,
        }));
    }
}

/// If `expr` is `__exportStar(require("<spec>"), exports)`, return `<spec>`.
fn export_star_specifier(expr: &Expr, unresolved_mark: Mark) -> Option<String> {
    let Expr::Call(call) = expr else {
        return None;
    };
    if !is_export_star_callee(&call.callee) {
        return None;
    }
    let [module_arg, exports_arg] = call.args.as_slice() else {
        return None;
    };
    if module_arg.spread.is_some() || exports_arg.spread.is_some() {
        return None;
    }
    // Second argument must be the global `exports`.
    let Expr::Ident(exports) = exports_arg.expr.as_ref() else {
        return None;
    };
    if exports.sym != "exports" || exports.ctxt.outer() != unresolved_mark {
        return None;
    }
    // First argument must be `require("<spec>")` against the global `require`.
    require_specifier(&module_arg.expr, unresolved_mark)
}

/// Whether `callee` is `__exportStar` or `<obj>.__exportStar`.
fn is_export_star_callee(callee: &Callee) -> bool {
    let Callee::Expr(expr) = callee else {
        return false;
    };
    match expr.as_ref() {
        Expr::Ident(ident) => ident.sym == "__exportStar",
        Expr::Member(member) => {
            matches!(&member.prop, MemberProp::Ident(name) if name.sym == "__exportStar")
        }
        _ => false,
    }
}

/// If `expr` is `require("<spec>")` against the global `require`, return the
/// string specifier.
fn require_specifier(expr: &Expr, unresolved_mark: Mark) -> Option<String> {
    let Expr::Call(call) = expr else {
        return None;
    };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(require) = callee.as_ref() else {
        return None;
    };
    if require.sym != "require" || require.ctxt.outer() != unresolved_mark {
        return None;
    }
    let [arg] = call.args.as_slice() else {
        return None;
    };
    if arg.spread.is_some() {
        return None;
    }
    match arg.expr.as_ref() {
        Expr::Lit(Lit::Str(s)) => s.value.as_str().map(str::to_owned),
        _ => None,
    }
}
