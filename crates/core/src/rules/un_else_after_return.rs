//! Drops a redundant `else` after an `if` branch that always exits.
//!
//! When the consequent of an `if` always leaves the surrounding statement
//! sequence — its control flow ends in `return` or `throw` — the `else` is
//! redundant: its body can only run on the fall-through, exactly as it does
//! once hoisted to follow the `if`.
//!
//! ```js
//! if (a) { return 1; } else { g(); return 2; }
//! ```
//!
//! becomes the flatter, equivalent:
//!
//! ```js
//! if (a) { return 1; }
//! g();
//! return 2;
//! ```
//!
//! `else if` chains flatten the same way, one link at a time.
//!
//! ## Safety
//!
//! The consequent must *unconditionally* exit: a bare `return`/`throw`, a block
//! whose last statement exits, or an `if`/`else` whose branches both exit. A
//! consequent that only sometimes exits (an `if` with no `else`, a loop, a plain
//! statement) leaves the `else` in place.
//!
//! Hoisting must not change scoping. A `let`, `const`, `class`, or function
//! declaration at the top level of the `else` block is block-scoped, so moving
//! it into the enclosing scope would widen its visibility (and risk a name
//! collision); such an `else` is left untouched. A `var` is already
//! function-scoped, so hoisting it changes nothing and is allowed.

use swc_core::ecma::ast::{Decl, ModuleItem, Stmt, VarDeclKind};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

pub struct UnElseAfterReturn;

impl VisitMut for UnElseAfterReturn {
    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        stmts.visit_mut_children_with(self);

        let original = std::mem::take(stmts);
        let mut out = Vec::with_capacity(original.len());
        for stmt in original {
            push_hoisted(stmt, &mut out);
        }
        *stmts = out;
    }

    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        items.visit_mut_children_with(self);

        let original = std::mem::take(items);
        let mut out = Vec::with_capacity(original.len());
        for item in original {
            match item {
                ModuleItem::Stmt(stmt) => {
                    let mut hoisted = Vec::new();
                    push_hoisted(stmt, &mut hoisted);
                    out.extend(hoisted.into_iter().map(ModuleItem::Stmt));
                }
                other => out.push(other),
            }
        }
        *items = out;
    }
}

/// Appends `stmt` to `out`, splicing in the `else` body when the `if` qualifies.
/// Recurses into the hoisted body so an `else if` chain flattens in one pass.
fn push_hoisted(stmt: Stmt, out: &mut Vec<Stmt>) {
    let Stmt::If(mut if_stmt) = stmt else {
        out.push(stmt);
        return;
    };

    let hoistable = if_stmt
        .alt
        .as_deref()
        .is_some_and(|alt| stmt_always_exits(&if_stmt.cons) && alt_is_hoistable(alt));

    if !hoistable {
        out.push(Stmt::If(if_stmt));
        return;
    }

    let alt = if_stmt.alt.take().expect("alt present when hoistable");
    out.push(Stmt::If(if_stmt));
    match *alt {
        Stmt::Block(block) => {
            for inner in block.stmts {
                push_hoisted(inner, out);
            }
        }
        other => push_hoisted(other, out),
    }
}

/// Whether a statement's control flow always leaves the enclosing sequence.
fn stmt_always_exits(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return(_) | Stmt::Throw(_) => true,
        Stmt::Block(block) => block.stmts.last().is_some_and(stmt_always_exits),
        Stmt::If(inner) => inner
            .alt
            .as_deref()
            .is_some_and(|alt| stmt_always_exits(&inner.cons) && stmt_always_exits(alt)),
        _ => false,
    }
}

/// Whether the `else` body can be hoisted without widening a block-scoped
/// binding. Only a block body can carry a top-level declaration; a single
/// statement cannot be one.
fn alt_is_hoistable(alt: &Stmt) -> bool {
    match alt {
        Stmt::Block(block) => !block.stmts.iter().any(is_block_scoped_decl),
        _ => true,
    }
}

/// Whether a statement is a block-scoped declaration whose scope would change if
/// it were lifted out of its block. `var` is function-scoped and excluded.
fn is_block_scoped_decl(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Decl(Decl::Var(var)) => !matches!(var.kind, VarDeclKind::Var),
        Stmt::Decl(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::UnElseAfterReturn;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, Globals, SourceMap, GLOBALS};
    use swc_core::ecma::codegen::text_writer::JsWriter;
    use swc_core::ecma::codegen::Emitter;
    use swc_core::ecma::parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
    use swc_core::ecma::visit::VisitMutWith;

    /// Parse, run the rule, and print — isolating this rewrite from the rest of
    /// the pipeline. The rule restructures statements, not bindings, so no
    /// resolver pass is needed.
    fn run_rule(source: &str) -> String {
        GLOBALS.set(&Globals::new(), || {
            let cm: Lrc<SourceMap> = Default::default();
            let fm = cm.new_source_file(
                FileName::Custom("test.js".into()).into(),
                source.to_string(),
            );
            let lexer = Lexer::new(
                Syntax::Es(EsSyntax::default()),
                Default::default(),
                StringInput::from(&*fm),
                None,
            );
            let mut module = Parser::new_from(lexer).parse_module().expect("parse");

            module.visit_mut_with(&mut UnElseAfterReturn);

            let mut buf = Vec::new();
            {
                let mut emitter = Emitter {
                    cfg: Default::default(),
                    cm: cm.clone(),
                    comments: None,
                    wr: JsWriter::new(cm.clone(), "\n", &mut buf, None),
                };
                emitter.emit_module(&module).expect("emit");
            }
            String::from_utf8(buf).unwrap()
        })
    }

    fn normalize(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn assert_rewrites(source: &str, expected: &str) {
        assert_eq!(normalize(&run_rule(source)), normalize(expected));
    }

    #[test]
    fn hoists_else_after_return() {
        assert_rewrites(
            "function f(a) { if (a) { return 1; } else { g(); return 2; } }",
            "function f(a) { if (a) { return 1; } g(); return 2; }",
        );
    }

    #[test]
    fn hoists_else_after_throw() {
        assert_rewrites(
            "function f(a) { if (a) { throw e; } else { h(); } }",
            "function f(a) { if (a) { throw e; } h(); }",
        );
    }

    #[test]
    fn hoists_single_statement_else() {
        assert_rewrites(
            "function f(a) { if (a) { return 1; } else g(); }",
            "function f(a) { if (a) { return 1; } g(); }",
        );
    }

    #[test]
    fn flattens_else_if_chain() {
        assert_rewrites(
            "function f(a, b) { if (a) { return 1; } else if (b) { return 2; } else { c(); } }",
            "function f(a, b) { if (a) { return 1; } if (b) { return 2; } c(); }",
        );
    }

    #[test]
    fn hoists_when_both_consequent_branches_exit() {
        // The consequent always exits (both inner branches return), so the outer
        // `else` hoists; the inner `else` flattens too in the same pass.
        assert_rewrites(
            "function f(a, b) { if (a) { if (b) { return 1; } else { return 2; } } else { c(); } }",
            "function f(a, b) { if (a) { if (b) { return 1; } return 2; } c(); }",
        );
    }

    #[test]
    fn hoists_var_declaration_from_else() {
        assert_rewrites(
            "function f(a) { if (a) { return 1; } else { var z = 2; use(z); } }",
            "function f(a) { if (a) { return 1; } var z = 2; use(z); }",
        );
    }

    #[test]
    fn removes_empty_else() {
        assert_rewrites(
            "function f(a) { if (a) { return 1; } else {} }",
            "function f(a) { if (a) { return 1; } }",
        );
    }

    #[test]
    fn hoists_at_module_top_level() {
        assert_rewrites(
            "if (a) { throw e; } else { g(); }",
            "if (a) { throw e; } g();",
        );
    }

    #[test]
    fn keeps_else_when_consequent_does_not_exit() {
        let src = "function f(a) { if (a) { g(); } else { h(); } }";
        assert_rewrites(src, src);
    }

    #[test]
    fn keeps_else_with_let_declaration() {
        let src = "function f(a) { if (a) { return 1; } else { let x = 2; use(x); } }";
        assert_rewrites(src, src);
    }

    #[test]
    fn keeps_else_with_const_declaration() {
        let src = "function f(a) { if (a) { return 1; } else { const y = 2; use(y); } }";
        assert_rewrites(src, src);
    }

    #[test]
    fn keeps_else_with_function_declaration() {
        let src = "function f(a) { if (a) { return 1; } else { function h() {} h(); } }";
        assert_rewrites(src, src);
    }

    #[test]
    fn keeps_if_without_else() {
        let src = "function f(a) { if (a) { return 1; } }";
        assert_rewrites(src, src);
    }

    #[test]
    fn keeps_else_when_consequent_only_sometimes_exits() {
        // The consequent's `if` has no `else`, so it does not always exit.
        let src = "function f(a, b) { if (a) { if (b) { return 1; } } else { c(); } }";
        assert_rewrites(src, src);
    }
}
