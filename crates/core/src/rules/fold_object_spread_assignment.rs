//! Folds property assignments that immediately follow an object-literal
//! initializer back into the literal.
//!
//! Minifiers and transpilers frequently clone-then-mutate an object:
//!
//! ```js
//! let l = { ...tt };
//! l[bt] = rules;
//! ```
//!
//! which is more idiomatically written as a single object literal:
//!
//! ```js
//! let l = { ...tt, [bt]: rules };
//! ```
//!
//! This rule recognizes a statement that binds (or assigns) an identifier to an
//! object literal, immediately followed by one or more property assignments to
//! that same identifier, and folds the assignments into the literal. Resolves
//! <https://github.com/pionxzh/wakaru/issues/184>.
//!
//! ## Safety
//!
//! The fold only fires when it preserves behavior:
//! - the initializer binds/assigns a plain identifier to a **fresh object
//!   literal** (not a reference to an existing object, which must not be
//!   spread);
//! - each folded statement is a plain `=` assignment to a static or computed
//!   member of that identifier;
//! - neither the key nor the value of a folded assignment references the target
//!   identifier (in the merged literal the target is not yet bound, so such a
//!   reference would change meaning);
//! - the property is not `__proto__`, whose object-literal and member-assignment
//!   forms are not interchangeable.
//!
//! Evaluation order is unchanged: both forms evaluate the spread/existing
//! properties, then the key, then the value.

use swc_core::atoms::Atom;
use swc_core::common::SyntaxContext;
use swc_core::ecma::ast::{
    AssignExpr, AssignOp, AssignTarget, Decl, Expr, ExprStmt, Ident, KeyValueProp, MemberExpr,
    MemberProp, ModuleItem, ObjectLit, Pat, Prop, PropName, PropOrSpread, SimpleAssignTarget, Stmt,
};
use swc_core::ecma::visit::{Visit, VisitMut, VisitMutWith, VisitWith};

type BindingId = (Atom, SyntaxContext);

pub struct FoldObjectSpreadAssignment;

impl VisitMut for FoldObjectSpreadAssignment {
    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        stmts.visit_mut_children_with(self);
        let original = std::mem::take(stmts);
        *stmts = fold_statements(original);
    }

    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        items.visit_mut_children_with(self);

        // Fold only across runs of plain statements; module declarations break a
        // run (and are left untouched).
        let original = std::mem::take(items);
        let mut out: Vec<ModuleItem> = Vec::with_capacity(original.len());
        let mut run: Vec<Stmt> = Vec::new();
        for item in original {
            match item {
                ModuleItem::Stmt(stmt) => run.push(stmt),
                other => {
                    flush_run(&mut run, &mut out);
                    out.push(other);
                }
            }
        }
        flush_run(&mut run, &mut out);
        *items = out;
    }
}

fn flush_run(run: &mut Vec<Stmt>, out: &mut Vec<ModuleItem>) {
    if run.is_empty() {
        return;
    }
    let folded = fold_statements(std::mem::take(run));
    out.extend(folded.into_iter().map(ModuleItem::Stmt));
}

fn fold_statements(input: Vec<Stmt>) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = Vec::with_capacity(input.len());
    let mut iter = input.into_iter().peekable();

    while let Some(mut stmt) = iter.next() {
        if let Some(target) = object_init_target(&stmt) {
            // Fold as many immediately-following property assignments to `target`
            // as are safe.
            while let Some(next) = iter.peek() {
                match foldable_assignment_prop(next, &target) {
                    Some(prop) => {
                        append_prop(&mut stmt, prop);
                        iter.next();
                    }
                    None => break,
                }
            }
        }
        out.push(stmt);
    }

    out
}

/// If `stmt` binds or assigns a plain identifier to a fresh object literal,
/// return that identifier's binding id.
fn object_init_target(stmt: &Stmt) -> Option<BindingId> {
    match stmt {
        Stmt::Decl(Decl::Var(var)) => {
            let [decl] = var.decls.as_slice() else {
                return None;
            };
            let Pat::Ident(name) = &decl.name else {
                return None;
            };
            match decl.init.as_deref() {
                Some(Expr::Object(_)) => Some((name.id.sym.clone(), name.id.ctxt)),
                _ => None,
            }
        }
        Stmt::Expr(ExprStmt { expr, .. }) => {
            let Expr::Assign(AssignExpr {
                op: AssignOp::Assign,
                left,
                right,
                ..
            }) = expr.as_ref()
            else {
                return None;
            };
            let AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) = left else {
                return None;
            };
            match right.as_ref() {
                Expr::Object(_) => Some((ident.id.sym.clone(), ident.id.ctxt)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// If `stmt` is a safe `target.<prop> = value` / `target[<key>] = value`
/// assignment, return the object-literal property to fold in.
fn foldable_assignment_prop(stmt: &Stmt, target: &BindingId) -> Option<Prop> {
    let Stmt::Expr(ExprStmt { expr, .. }) = stmt else {
        return None;
    };
    let Expr::Assign(AssignExpr {
        op: AssignOp::Assign,
        left,
        right,
        ..
    }) = expr.as_ref()
    else {
        return None;
    };
    let AssignTarget::Simple(SimpleAssignTarget::Member(MemberExpr { obj, prop, .. })) = left
    else {
        return None;
    };
    // The member must be on exactly the target identifier.
    let Expr::Ident(base) = obj.as_ref() else {
        return None;
    };
    if (base.sym.clone(), base.ctxt) != *target {
        return None;
    }
    // The value must not reference the target (it is not bound in the literal).
    if references_binding(right, target) {
        return None;
    }

    let key = match prop {
        MemberProp::Ident(name) => {
            if name.sym == "__proto__" {
                return None;
            }
            PropName::Ident(name.clone())
        }
        MemberProp::Computed(computed) => {
            // A computed key must not reference the target either.
            if references_binding(&computed.expr, target) {
                return None;
            }
            // Be conservative about a `"__proto__"` string key.
            if let Expr::Lit(swc_core::ecma::ast::Lit::Str(s)) = computed.expr.as_ref() {
                if s.value == "__proto__" {
                    return None;
                }
            }
            PropName::Computed(computed.clone())
        }
        MemberProp::PrivateName(_) => return None,
    };

    Some(Prop::KeyValue(KeyValueProp {
        key,
        value: right.clone(),
    }))
}

/// Append `prop` to the object literal that `stmt` initializes. `stmt` is known
/// to be an object-literal initializer (via [`object_init_target`]).
fn append_prop(stmt: &mut Stmt, prop: Prop) {
    let object = match stmt {
        Stmt::Decl(Decl::Var(var)) => var.decls.first_mut().and_then(|d| d.init.as_deref_mut()),
        Stmt::Expr(ExprStmt { expr, .. }) => match expr.as_mut() {
            Expr::Assign(assign) => Some(assign.right.as_mut()),
            _ => None,
        },
        _ => None,
    };
    if let Some(Expr::Object(ObjectLit { props, .. })) = object {
        props.push(PropOrSpread::Prop(Box::new(prop)));
    }
}

/// Whether `expr` references the identifier `target` (same name and syntax
/// context).
fn references_binding(expr: &Expr, target: &BindingId) -> bool {
    let mut finder = ReferenceFinder {
        target,
        found: false,
    };
    expr.visit_with(&mut finder);
    finder.found
}

struct ReferenceFinder<'a> {
    target: &'a BindingId,
    found: bool,
}

impl Visit for ReferenceFinder<'_> {
    fn visit_ident(&mut self, ident: &Ident) {
        if ident.sym == self.target.0 && ident.ctxt == self.target.1 {
            self.found = true;
        }
    }
}
