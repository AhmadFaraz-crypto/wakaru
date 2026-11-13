mod common;

use common::assert_eq_normalized;
use wakaru_core::{decompile, DecompileOptions};

fn render(source: &str) -> String {
    decompile(
        source,
        DecompileOptions {
            filename: "fixture.js".to_string(),
            ..Default::default()
        },
    )
    .expect("decompile should succeed")
    .code
}

// --- flattens a redundant else ----------------------------------------------

#[test]
fn hoists_else_after_return() {
    assert_eq_normalized(
        &render("function f(a) { if (a) { return 1; } else { g(); return 2; } }"),
        "function f(a) { if (a) { return 1; } g(); return 2; }",
    );
}

#[test]
fn hoists_else_after_throw() {
    assert_eq_normalized(
        &render("function f(a) { if (a) { throw e; } else { h(); } }"),
        "function f(a) { if (a) { throw e; } h(); }",
    );
}

#[test]
fn hoists_single_statement_else() {
    assert_eq_normalized(
        &render("function f(a) { if (a) { return 1; } else g(); }"),
        "function f(a) { if (a) { return 1; } g(); }",
    );
}

#[test]
fn flattens_else_if_chain() {
    assert_eq_normalized(
        &render("function f(a, b) { if (a) { return 1; } else if (b) { return 2; } else { c(); } }"),
        "function f(a, b) { if (a) { return 1; } if (b) { return 2; } c(); }",
    );
}

#[test]
fn hoists_when_both_consequent_branches_exit() {
    assert_eq_normalized(
        &render(
            "function f(a, b) { if (a) { if (b) { return 1; } else { return 2; } } else { c(); } }",
        ),
        "function f(a, b) { if (a) { if (b) { return 1; } return 2; } c(); }",
    );
}

#[test]
fn hoists_after_nested_return_and_throw() {
    assert_eq_normalized(
        &render(
            "function f(a, b) { if (a) { if (b) { return 1; } else { throw e; } } else { g(); } }",
        ),
        "function f(a, b) { if (a) { if (b) { return 1; } throw e; } g(); }",
    );
}

#[test]
fn flattens_multiple_else_if_branches() {
    assert_eq_normalized(
        &render(
            "function f(a, b, c) { if (a) { return 1; } else if (b) { return 2; } else if (c) { throw e; } else { g(); } }",
        ),
        "function f(a, b, c) { if (a) { return 1; } if (b) { return 2; } if (c) { throw e; } g(); }",
    );
}

#[test]
fn hoists_when_declaration_is_only_nested() {
    assert_eq_normalized(
        &render(
            "function f(a) { if (a) { return 1; } else { { const x = 2; use(x); } g(); } }",
        ),
        "function f(a) { if (a) { return 1; } { const x = 2; use(x); } g(); }",
    );
}

#[test]
fn hoists_unbraced_nested_guaranteed_exit() {
    assert_eq_normalized(
        &render("function f(a, b) { if (a) if (b) return 1; else throw e; else g(); }"),
        "function f(a, b) { if (a) { if (b) { return 1; } throw e; } g(); }",
    );
}

// --- leaves everything else as written --------------------------------------

#[test]
fn keeps_else_when_consequent_does_not_exit() {
    let src = "function f(a) { if (a) { g(); } else { h(); } }";
    assert_eq_normalized(&render(src), src);
}

#[test]
fn preserves_scoped_declarations_in_partial_chain() {
    assert_eq_normalized(
        &render(
            "function f(a, b) { if (a) { return 1; } else if (b) { g(); } else { const x = 2; use(x); } }",
        ),
        "function f(a, b) { if (a) { return 1; } if (b) { g(); } else { const x = 2; use(x); } }",
    );
}

#[test]
fn preserves_outer_else_after_inner_simplification() {
    assert_eq_normalized(
        &render(
            "function f(a, b, c) { if (a) { if (b) { return 1; } else { g(); } if (c) { h(); } } else { fallback(); } }",
        ),
        "function f(a, b, c) { if (a) { if (b) { return 1; } g(); if (c) { h(); } } else { fallback(); } }",
    );
}

#[test]
fn keeps_else_when_consequent_only_sometimes_exits() {
    // The inner `if` has no `else`, so the consequent does not always exit.
    let src = "function f(a, b) { if (a) { if (b) { return 1; } } else { c(); } }";
    assert_eq_normalized(&render(src), src);
}

#[test]
fn keeps_else_with_lexical_declaration() {
    // A `const` in the else block is block-scoped; hoisting it would widen its
    // scope, so the else is left in place.
    let src = "function f(a) { if (a) { return 1; } else { const y = 2; use(y); } }";
    assert_eq_normalized(&render(src), src);
}

#[test]
fn keeps_if_without_else() {
    let src = "function f(a) { if (a) { return 1; } g(); }";
    assert_eq_normalized(&render(src), src);
}
