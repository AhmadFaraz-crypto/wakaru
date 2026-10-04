mod common;

use common::{assert_eq_normalized, render_rule};
use wakaru_core::rules::UnCompoundAssignment;

fn apply(input: &str) -> String {
    render_rule(input, |_| UnCompoundAssignment)
}

// --- folds -------------------------------------------------------------------

#[test]
fn folds_add_on_identifier() {
    assert_eq_normalized(&apply("x = x + 1;"), "x += 1;");
}

#[test]
fn folds_multiply() {
    assert_eq_normalized(&apply("total = total * k;"), "total *= k;");
}

#[test]
fn folds_subtract_with_expression_rhs() {
    assert_eq_normalized(&apply("x = x - y;"), "x -= y;");
}

#[test]
fn folds_static_member() {
    assert_eq_normalized(&apply("obj.n = obj.n + 1;"), "obj.n += 1;");
}

#[test]
fn folds_computed_literal_key() {
    assert_eq_normalized(&apply("obj[\"n\"] = obj[\"n\"] + 1;"), "obj[\"n\"] += 1;");
}

#[test]
fn folds_computed_identifier_key() {
    assert_eq_normalized(&apply("arr[i] = arr[i] + 1;"), "arr[i] += 1;");
}

#[test]
fn folds_exponent() {
    assert_eq_normalized(&apply("x = x ** 2;"), "x **= 2;");
}

#[test]
fn folds_bitwise_and() {
    assert_eq_normalized(&apply("x = x & 3;"), "x &= 3;");
}

// --- guards (must NOT fold) --------------------------------------------------

#[test]
fn does_not_fold_target_on_the_right() {
    // `x = 1 + x` is not `x += 1` (differs for strings / non-commutative ops).
    assert_eq_normalized(&apply("x = 1 + x;"), "x = 1 + x;");
}

#[test]
fn does_not_fold_target_as_right_operand_variable() {
    assert_eq_normalized(&apply("x = y + x;"), "x = y + x;");
}

#[test]
fn does_not_fold_different_variable() {
    assert_eq_normalized(&apply("x = y + 1;"), "x = y + 1;");
}

#[test]
fn does_not_fold_side_effecting_base() {
    assert_eq_normalized(
        &apply("getObj().n = getObj().n + 1;"),
        "getObj().n = getObj().n + 1;",
    );
}

#[test]
fn does_not_fold_side_effecting_computed_key() {
    assert_eq_normalized(&apply("o[k()] = o[k()] + 1;"), "o[k()] = o[k()] + 1;");
}

#[test]
fn does_not_fold_deeper_member_base() {
    assert_eq_normalized(&apply("a.b.c = a.b.c + 1;"), "a.b.c = a.b.c + 1;");
}

#[test]
fn does_not_fold_different_member_property() {
    assert_eq_normalized(&apply("a.x = a.y + 1;"), "a.x = a.y + 1;");
}

#[test]
fn does_not_fold_logical_operator() {
    // Logical assignment is the other rule's territory.
    assert_eq_normalized(&apply("x = x || y;"), "x = x || y;");
}

#[test]
fn does_not_fold_non_compound_operator() {
    assert_eq_normalized(&apply("x = x == 1;"), "x = x == 1;");
}
