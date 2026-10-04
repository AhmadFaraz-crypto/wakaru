mod common;

use common::{assert_eq_normalized, render};

// --- folds -------------------------------------------------------------------

#[test]
fn folds_add() {
    assert_eq_normalized(
        &render("function f(x) { x = x + 1; return x; }"),
        "function f(x) {\n    x += 1;\n    return x;\n}",
    );
}

#[test]
fn folds_multiply() {
    assert_eq_normalized(
        &render("function f(total, k) { total = total * k; return total; }"),
        "function f(total, k) {\n    total *= k;\n    return total;\n}",
    );
}

#[test]
fn folds_subtract() {
    assert_eq_normalized(
        &render("function f(x, y) { x = x - y; return x; }"),
        "function f(x, y) {\n    x -= y;\n    return x;\n}",
    );
}

#[test]
fn folds_exponent() {
    assert_eq_normalized(
        &render("function f(x) { x = x ** 2; return x; }"),
        "function f(x) {\n    x **= 2;\n    return x;\n}",
    );
}

#[test]
fn folds_bitwise_and() {
    assert_eq_normalized(
        &render("function f(x) { x = x & 3; return x; }"),
        "function f(x) {\n    x &= 3;\n    return x;\n}",
    );
}

#[test]
fn folds_static_member() {
    assert_eq_normalized(
        &render("function f(obj) { obj.n = obj.n + 1; return obj; }"),
        "function f(obj) {\n    obj.n += 1;\n    return obj;\n}",
    );
}

#[test]
fn folds_computed_member_with_identifier_key() {
    assert_eq_normalized(
        &render("function f(obj, i) { obj[i] = obj[i] + 1; return obj; }"),
        "function f(obj, i) {\n    obj[i] += 1;\n    return obj;\n}",
    );
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_target_as_right_operand_literal() {
    // `x = 1 + x` is not `x += 1` (differs for strings and non-commutative ops).
    assert_eq_normalized(
        &render("function f(x) { x = 1 + x; return x; }"),
        "function f(x) {\n    x = 1 + x;\n    return x;\n}",
    );
}

#[test]
fn ignores_target_as_right_operand_variable() {
    assert_eq_normalized(
        &render("function f(x, y) { x = y + x; return x; }"),
        "function f(x, y) {\n    x = y + x;\n    return x;\n}",
    );
}

#[test]
fn ignores_different_variable() {
    assert_eq_normalized(
        &render("function f(x, y) { x = y + 1; return x; }"),
        "function f(x, y) {\n    x = y + 1;\n    return x;\n}",
    );
}

#[test]
fn ignores_side_effecting_base() {
    assert_eq_normalized(
        &render("function f() { getObj().n = getObj().n + 1; }"),
        "function f() {\n    getObj().n = getObj().n + 1;\n}",
    );
}

#[test]
fn ignores_side_effecting_computed_key() {
    assert_eq_normalized(
        &render("function f(o) { o[k()] = o[k()] + 1; return o; }"),
        "function f(o) {\n    o[k()] = o[k()] + 1;\n    return o;\n}",
    );
}

#[test]
fn ignores_deeper_member_base() {
    assert_eq_normalized(
        &render("function f(a) { a.b.c = a.b.c + 1; return a; }"),
        "function f(a) {\n    a.b.c = a.b.c + 1;\n    return a;\n}",
    );
}

#[test]
fn ignores_different_member_property() {
    assert_eq_normalized(
        &render("function f(a) { a.x = a.y + 1; return a; }"),
        "function f(a) {\n    a.x = a.y + 1;\n    return a;\n}",
    );
}

#[test]
fn ignores_logical_operator() {
    // Logical assignment is handled by a different rule.
    assert_eq_normalized(
        &render("function f(x, y) { x = x || y; return x; }"),
        "function f(x, y) {\n    x = x || y;\n    return x;\n}",
    );
}

#[test]
fn ignores_non_compound_operator() {
    assert_eq_normalized(
        &render("function f(x) { x = x == 1; return x; }"),
        "function f(x) {\n    x = x == 1;\n    return x;\n}",
    );
}
