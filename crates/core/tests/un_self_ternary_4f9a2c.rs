mod common;

use common::{assert_eq_normalized, render};

// --- folds -------------------------------------------------------------------

#[test]
fn folds_identifier_to_logical_or() {
    assert_eq_normalized(
        &render("function f(x, y) { const r = x ? x : y; return r.length; }"),
        "function f(x, y) {\n    const r = x || y;\n    return r.length;\n}",
    );
}

#[test]
fn folds_identifier_to_logical_and() {
    assert_eq_normalized(
        &render("function f(x, y) { const r = x ? y : x; return r.length; }"),
        "function f(x, y) {\n    const r = x && y;\n    return r.length;\n}",
    );
}

#[test]
fn folds_static_member_to_logical_or() {
    assert_eq_normalized(
        &render("function f(obj, y) { const r = obj.n ? obj.n : y; return r.length; }"),
        "function f(obj, y) {\n    const r = obj.n || y;\n    return r.length;\n}",
    );
}

#[test]
fn folds_computed_member_to_logical_and() {
    assert_eq_normalized(
        &render("function f(arr, i, y) { const r = arr[i] ? y : arr[i]; return r.length; }"),
        "function f(arr, i, y) {\n    const r = arr[i] && y;\n    return r.length;\n}",
    );
}

#[test]
fn folds_with_complex_other_branch() {
    assert_eq_normalized(
        &render("function f(x, a, b) { const r = x ? x : compute(a, b); return r.length; }"),
        "function f(x, a, b) {\n    const r = x || compute(a, b);\n    return r.length;\n}",
    );
}

#[test]
fn folds_as_call_argument() {
    assert_eq_normalized(
        &render("function f(x, y) { log(x ? x : y); }"),
        "function f(x, y) {\n    log(x || y);\n}",
    );
}

#[test]
fn folds_nested_in_expression() {
    assert_eq_normalized(
        &render("function f(x, y) { return (x ? x : y).length; }"),
        "function f(x, y) {\n    return (x || y).length;\n}",
    );
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_general_ternary() {
    assert_eq_normalized(
        &render("function f(a, b, c) { const r = a ? b : c; return r.length; }"),
        "function f(a, b, c) {\n    const r = a ? b : c;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_deeper_member_condition() {
    // Base `a.b` is itself a member, so reading `a.b.c` twice is not provably safe.
    assert_eq_normalized(
        &render("function f(a, y) { const r = a.b.c ? a.b.c : y; return r.length; }"),
        "function f(a, y) {\n    const r = a.b.c ? a.b.c : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_side_effecting_computed_key() {
    assert_eq_normalized(
        &render("function f(o, y) { const r = o[k()] ? o[k()] : y; return r.length; }"),
        "function f(o, y) {\n    const r = o[k()] ? o[k()] : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_call_condition() {
    assert_eq_normalized(
        &render("function f(y) { const r = g() ? g() : y; return r.length; }"),
        "function f(y) {\n    const r = g() ? g() : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_near_miss_branch() {
    assert_eq_normalized(
        &render("function f(x, y) { const r = x ? x + 1 : y; return r; }"),
        "function f(x, y) {\n    const r = x ? x + 1 : y;\n    return r;\n}",
    );
}

#[test]
fn ignores_different_member_property() {
    // Condition is `obj.a`, branch is `obj.b` — a different place.
    assert_eq_normalized(
        &render("function f(obj, y) { const r = obj.a ? obj.b : y; return r.length; }"),
        "function f(obj, y) {\n    const r = obj.a ? obj.b : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_different_member_base() {
    // `a.x` vs `b.x` — different base objects.
    assert_eq_normalized(
        &render("function f(a, b, y) { const r = a.x ? b.x : y; return r.length; }"),
        "function f(a, b, y) {\n    const r = a.x ? b.x : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_different_computed_index() {
    // `arr[i]` vs `arr[j]` — different keys.
    assert_eq_normalized(
        &render("function f(arr, i, j, y) { const r = arr[i] ? arr[j] : y; return r.length; }"),
        "function f(arr, i, j, y) {\n    const r = arr[i] ? arr[j] : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_member_of_condition() {
    // Branch `x.foo` is a member of the condition, not the condition itself.
    assert_eq_normalized(
        &render("function f(x, y) { const r = x ? x.foo : y; return r.length; }"),
        "function f(x, y) {\n    const r = x ? x.foo : y;\n    return r.length;\n}",
    );
}

#[test]
fn folds_computed_string_key_to_logical_or() {
    assert_eq_normalized(
        &render("function f(obj, y) { const r = obj[\"x\"] ? obj[\"x\"] : y; return r.length; }"),
        "function f(obj, y) {\n    const r = obj.x || y;\n    return r.length;\n}",
    );
}

#[test]
fn folds_computed_numeric_key_to_logical_or() {
    assert_eq_normalized(
        &render("function f(obj, y) { const r = obj[0] ? obj[0] : y; return r.length; }"),
        "function f(obj, y) {\n    const r = obj[0] || y;\n    return r.length;\n}",
    );
}

#[test]
fn folds_computed_identifier_key_to_logical_or() {
    assert_eq_normalized(
        &render("function f(arr, i, y) { const r = arr[i] ? arr[i] : y; return r.length; }"),
        "function f(arr, i, y) {\n    const r = arr[i] || y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_update_expression_key() {
    // `i++` mutates: evaluating `arr[i++]` twice is not the same as once.
    assert_eq_normalized(
        &render("function f(arr, i, y) { const r = arr[i++] ? arr[i++] : y; return r.length; }"),
        "function f(arr, i, y) {\n    const r = arr[i++] ? arr[i++] : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_call_in_computed_key() {
    assert_eq_normalized(
        &render("function f(arr, y) { const r = arr[g()] ? arr[g()] : y; return r.length; }"),
        "function f(arr, y) {\n    const r = arr[g()] ? arr[g()] : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_chained_member_base() {
    // Base `obj.a` is itself a member, so reading `obj.a[i]` twice is not provably safe.
    assert_eq_normalized(
        &render("function f(obj, i, y) { const r = obj.a[i] ? obj.a[i] : y; return r.length; }"),
        "function f(obj, i, y) {\n    const r = obj.a[i] ? obj.a[i] : y;\n    return r.length;\n}",
    );
}

#[test]
fn ignores_compound_computed_key() {
    assert_eq_normalized(
        &render("function f(arr, i, y) { const r = arr[i + 1] ? arr[i + 1] : y; return r.length; }"),
        "function f(arr, i, y) {\n    const r = arr[i + 1] ? arr[i + 1] : y;\n    return r.length;\n}",
    );
}
