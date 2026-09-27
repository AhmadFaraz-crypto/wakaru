use swc_core::common::Mark;
use swc_core::ecma::ast::{
    ArrowExpr, ArrowFunctionBody, EmptyStmt, Expr, ExprStmt, Function, IfStmt, ReturnStmt, Stmt,
    SwitchStmt, UnaryExpr, UnaryOp,
};
use swc_core::ecma::visit::{Visit, VisitMut, VisitMutWith, VisitWith};

use super::expr_utils::is_unresolved_undefined;

pub struct UnReturn {
    unresolved_mark: Mark,
}

impl UnReturn {
    pub fn new(unresolved_mark: Mark) -> Self {
        Self { unresolved_mark }
    }
}

impl VisitMut for UnReturn {
    fn visit_mut_function(&mut self, function: &mut Function) {
        function.visit_mut_children_with(self);
        if let Some(body) = &mut function.body {
            simplify_tail_return(
                &mut body.stmts,
                function.is_async && function.is_generator,
                self.unresolved_mark,
            );
        }
    }

    fn visit_mut_arrow_expr(&mut self, arrow: &mut ArrowExpr) {
        arrow.visit_mut_children_with(self);
        if let ArrowFunctionBody::FunctionBody(block) = &mut *arrow.body {
            simplify_tail_return(&mut block.stmts, false, self.unresolved_mark);
        }
    }
}

fn simplify_tail_return(stmts: &mut Vec<Stmt>, preserve_value_return: bool, unresolved_mark: Mark) {
    let Some(last_stmt) = stmts.pop() else {
        return;
    };

    match last_stmt {
        Stmt::Return(ReturnStmt { span, arg }) => {
            if let Some(stmt) = simplified_return(span, arg, preserve_value_return, unresolved_mark)
            {
                stmts.push(stmt);
            }
        }
        mut stmt => {
            simplify_terminal_statement(&mut stmt, preserve_value_return, unresolved_mark);
            stmts.push(stmt);
        }
    }
}

fn simplified_return(
    span: swc_core::common::Span,
    arg: Option<Box<Expr>>,
    preserve_value_return: bool,
    unresolved_mark: Mark,
) -> Option<Stmt> {
    match arg {
        None => None,
        // Async-generator `return expression` awaits its value, even when the
        // expression is `undefined` or `void 0`. Falling through does not add
        // that promise-resolution turn, so only a bare return is redundant.
        Some(expr) if preserve_value_return => Some(Stmt::Return(ReturnStmt {
            span,
            arg: Some(expr),
        })),
        Some(expr) if is_unresolved_undefined(&expr, unresolved_mark) => None,
        Some(expr) => {
            if let Expr::Unary(UnaryExpr {
                op: UnaryOp::Void,
                arg,
                ..
            }) = *expr
            {
                Some(Stmt::Expr(ExprStmt { span, expr: arg }))
            } else {
                Some(Stmt::Return(ReturnStmt {
                    span,
                    arg: Some(expr),
                }))
            }
        }
    }
}

fn simplify_terminal_statement(
    stmt: &mut Stmt,
    preserve_value_return: bool,
    unresolved_mark: Mark,
) {
    match stmt {
        Stmt::Block(block) => {
            simplify_tail_return(&mut block.stmts, preserve_value_return, unresolved_mark);
        }
        Stmt::If(IfStmt { cons, alt, .. }) => {
            simplify_branch(cons, preserve_value_return, unresolved_mark);
            if let Some(alt) = alt {
                simplify_branch(alt, preserve_value_return, unresolved_mark);
            }
        }
        Stmt::Try(try_stmt) => {
            simplify_tail_return(
                &mut try_stmt.block.stmts,
                preserve_value_return,
                unresolved_mark,
            );
            if let Some(handler) = &mut try_stmt.handler {
                simplify_tail_return(
                    &mut handler.body.stmts,
                    preserve_value_return,
                    unresolved_mark,
                );
            }
            let can_simplify_finalizer =
                !has_value_return(&try_stmt.block, preserve_value_return, unresolved_mark)
                    && try_stmt.handler.as_ref().map_or(true, |handler| {
                        !has_value_return(&handler.body, preserve_value_return, unresolved_mark)
                    });
            if can_simplify_finalizer {
                if let Some(finalizer) = &mut try_stmt.finalizer {
                    simplify_tail_return(
                        &mut finalizer.stmts,
                        preserve_value_return,
                        unresolved_mark,
                    );
                }
            }
        }
        Stmt::Switch(SwitchStmt { cases, .. }) => {
            simplify_terminal_switch(cases, preserve_value_return, unresolved_mark);
        }
        _ => {}
    }
}

fn simplify_terminal_switch(
    cases: &mut [swc_core::ecma::ast::SwitchCase],
    preserve_value_return: bool,
    unresolved_mark: Mark,
) {
    for case in cases.iter_mut() {
        if case.cons.is_empty() {
            continue;
        }
        if case.cons.len() != 1 {
            return;
        }
        let Stmt::Return(return_stmt) = &case.cons[0] else {
            return;
        };
        if !is_redundant_return(
            return_stmt.arg.as_deref(),
            preserve_value_return,
            unresolved_mark,
        ) {
            return;
        }
    }
    for case in cases.iter_mut() {
        if !case.cons.is_empty() {
            case.cons[0] = Stmt::Empty(EmptyStmt {
                span: swc_core::common::DUMMY_SP,
            });
        }
    }
}

fn is_redundant_return(
    arg: Option<&Expr>,
    preserve_value_return: bool,
    unresolved_mark: Mark,
) -> bool {
    match arg {
        None => true,
        Some(_expr) if preserve_value_return => false,
        Some(expr) => is_unresolved_undefined(expr, unresolved_mark),
    }
}

struct ValueReturnFinder {
    preserve_value_return: bool,
    unresolved_mark: Mark,
    found: bool,
}

impl Visit for ValueReturnFinder {
    fn visit_return_stmt(&mut self, stmt: &ReturnStmt) {
        if let Some(expr) = &stmt.arg {
            if (self.preserve_value_return || !is_unresolved_undefined(expr, self.unresolved_mark))
                && !matches!(
                    &**expr,
                    Expr::Unary(UnaryExpr {
                        op: UnaryOp::Void,
                        ..
                    })
                )
            {
                self.found = true;
            }
        }
    }

    fn visit_function(&mut self, _: &Function) {}

    fn visit_arrow_expr(&mut self, _: &ArrowExpr) {}
}

fn has_value_return<T: VisitWith<ValueReturnFinder>>(
    node: &T,
    preserve_value_return: bool,
    unresolved_mark: Mark,
) -> bool {
    let mut finder = ValueReturnFinder {
        preserve_value_return,
        unresolved_mark,
        found: false,
    };
    node.visit_with(&mut finder);
    finder.found
}

fn simplify_branch(stmt: &mut Stmt, preserve_value_return: bool, unresolved_mark: Mark) {
    let replacement = match std::mem::replace(
        stmt,
        Stmt::Empty(EmptyStmt {
            span: swc_core::common::DUMMY_SP,
        }),
    ) {
        Stmt::Return(ReturnStmt { span, arg }) => {
            simplified_return(span, arg, preserve_value_return, unresolved_mark).unwrap_or_else(
                || {
                    Stmt::Empty(EmptyStmt {
                        span: swc_core::common::DUMMY_SP,
                    })
                },
            )
        }
        mut other => {
            simplify_terminal_statement(&mut other, preserve_value_return, unresolved_mark);
            other
        }
    };
    *stmt = replacement;
}
