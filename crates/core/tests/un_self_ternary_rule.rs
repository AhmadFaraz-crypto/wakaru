mod common;

use common::{assert_eq_normalized, render_rule};
use wakaru_core::rules::UnSelfTernary;

fn apply(input: &str) -> String {
    render_rule(input, |_| UnSelfTernary)
}

// --- folds -------------------------------------------------------------------

#[test]
fn folds_identifier_to_logical_or() {
    assert_eq_normalized(&apply("x ? x : y;"), "x || y;");
}

#[test]
fn folds_identifier_to_logical_and() {
    assert_eq_normalized(&apply("x ? y : x;"), "x && y;");
}

#[test]
fn folds_static_member_to_logical_or() {
    assert_eq_normalized(&apply("obj.n ? obj.n : y;"), "obj.n || y;");
}

#[test]
fn folds_computed_member_to_logical_and() {
    assert_eq_normalized(&apply("arr[i] ? y : arr[i];"), "arr[i] && y;");
}

#[test]
fn folds_with_complex_other_branch() {
    assert_eq_normalized(&apply("x ? x : compute(a, b);"), "x || compute(a, b);");
}

#[test]
fn folds_in_return_position() {
    assert_eq_normalized(
        &apply("function f(x, y) {\n    return x ? x : y;\n}"),
        "function f(x, y) {\n    return x || y;\n}",
    );
}

// --- guards (must NOT fold) --------------------------------------------------

#[test]
fn does_not_fold_general_ternary() {
    assert_eq_normalized(&apply("a ? b : c;"), "a ? b : c;");
}

#[test]
fn does_not_fold_call_condition() {
    assert_eq_normalized(&apply("f() ? f() : y;"), "f() ? f() : y;");
}

#[test]
fn does_not_fold_deeper_member() {
    // Base `a.b` is itself a member, so reading `a.b.c` twice is not provably safe.
    assert_eq_normalized(&apply("a.b.c ? a.b.c : y;"), "a.b.c ? a.b.c : y;");
}

#[test]
fn does_not_fold_side_effecting_computed_key() {
    assert_eq_normalized(&apply("o[k()] ? o[k()] : y;"), "o[k()] ? o[k()] : y;");
}

#[test]
fn does_not_fold_near_miss_branch() {
    assert_eq_normalized(&apply("x ? x + 1 : y;"), "x ? x + 1 : y;");
}

#[test]
fn does_not_fold_different_member_property() {
    assert_eq_normalized(&apply("obj.a ? obj.b : y;"), "obj.a ? obj.b : y;");
}

#[test]
fn does_not_fold_different_member_base() {
    assert_eq_normalized(&apply("a.x ? b.x : y;"), "a.x ? b.x : y;");
}

#[test]
fn does_not_fold_different_computed_index() {
    assert_eq_normalized(&apply("arr[i] ? arr[j] : y;"), "arr[i] ? arr[j] : y;");
}

#[test]
fn does_not_fold_member_of_condition() {
    assert_eq_normalized(&apply("x ? x.foo : y;"), "x ? x.foo : y;");
}

#[test]
fn folds_computed_string_key() {
    assert_eq_normalized(&apply("obj[\"x\"] ? obj[\"x\"] : y;"), "obj[\"x\"] || y;");
}

#[test]
fn folds_computed_numeric_key() {
    assert_eq_normalized(&apply("obj[0] ? obj[0] : y;"), "obj[0] || y;");
}

#[test]
fn folds_computed_identifier_key_or() {
    assert_eq_normalized(&apply("arr[i] ? arr[i] : y;"), "arr[i] || y;");
}

#[test]
fn does_not_fold_update_expression_key() {
    assert_eq_normalized(
        &apply("arr[i++] ? arr[i++] : y;"),
        "arr[i++] ? arr[i++] : y;",
    );
}

#[test]
fn does_not_fold_call_in_computed_key() {
    assert_eq_normalized(
        &apply("arr[g()] ? arr[g()] : y;"),
        "arr[g()] ? arr[g()] : y;",
    );
}

#[test]
fn does_not_fold_chained_member_base() {
    assert_eq_normalized(
        &apply("obj.a[i] ? obj.a[i] : y;"),
        "obj.a[i] ? obj.a[i] : y;",
    );
}

#[test]
fn does_not_fold_compound_computed_key() {
    assert_eq_normalized(
        &apply("arr[i + 1] ? arr[i + 1] : y;"),
        "arr[i + 1] ? arr[i + 1] : y;",
    );
}
