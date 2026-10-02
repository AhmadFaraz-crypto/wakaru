mod common;

use common::{assert_eq_normalized, render_rule};
use wakaru_core::rules::FoldObjectSpreadAssignment;

fn apply(input: &str) -> String {
    render_rule(input, |_| FoldObjectSpreadAssignment)
}

// --- folds -------------------------------------------------------------------

#[test]
fn folds_computed_assignment_into_spread_literal() {
    let input = "let l = {\n    ...tt\n};\nl[bt] = rules;";
    let expected = "let l = {\n    ...tt,\n    [bt]: rules\n};";
    assert_eq_normalized(&apply(input), expected);
}

#[test]
fn folds_static_assignment_into_spread_literal() {
    let input = "let l = {\n    ...tt\n};\nl.foo = 2;";
    let expected = "let l = {\n    ...tt,\n    foo: 2\n};";
    assert_eq_normalized(&apply(input), expected);
}

#[test]
fn folds_multiple_consecutive_assignments() {
    let input = "let l = {\n    ...tt\n};\nl[a] = 1;\nl[b] = 2;";
    let expected = "let l = {\n    ...tt,\n    [a]: 1,\n    [b]: 2\n};";
    assert_eq_normalized(&apply(input), expected);
}

#[test]
fn folds_into_plain_assignment_form() {
    let input = "l = {\n    ...tt\n};\nl[bt] = rules;";
    let expected = "l = {\n    ...tt,\n    [bt]: rules\n};";
    assert_eq_normalized(&apply(input), expected);
}

#[test]
fn folds_into_empty_object_literal() {
    let input = "let l = {};\nl.a = 1;";
    let expected = "let l = {\n    a: 1\n};";
    assert_eq_normalized(&apply(input), expected);
}

#[test]
fn folds_after_existing_properties() {
    let input = "let l = {\n    x: 1\n};\nl.y = 2;";
    let expected = "let l = {\n    x: 1,\n    y: 2\n};";
    assert_eq_normalized(&apply(input), expected);
}

#[test]
fn stops_folding_at_first_non_matching_statement() {
    // The first assignment folds; the unrelated call stops the run, so the
    // second assignment stays.
    let input = "let l = {\n    ...tt\n};\nl.a = 1;\nf();\nl.b = 2;";
    let expected = "let l = {\n    ...tt,\n    a: 1\n};\nf();\nl.b = 2;";
    assert_eq_normalized(&apply(input), expected);
}

// --- guards (must NOT fold) --------------------------------------------------

#[test]
fn does_not_fold_when_value_references_target() {
    // `l` is not yet bound inside the literal, so folding would change meaning.
    let input = "let l = {\n    ...tt\n};\nl.self = l;";
    assert_eq_normalized(&apply(input), input);
}

#[test]
fn does_not_fold_when_computed_key_references_target() {
    let input = "let l = {\n    ...tt\n};\nl[l.k] = 1;";
    assert_eq_normalized(&apply(input), input);
}

#[test]
fn does_not_fold_non_adjacent_assignment() {
    let input = "let l = {\n    ...tt\n};\ng();\nl.a = 1;";
    assert_eq_normalized(&apply(input), input);
}

#[test]
fn does_not_fold_compound_assignment() {
    let input = "let l = {\n    ...tt\n};\nl.a += 1;";
    assert_eq_normalized(&apply(input), input);
}

#[test]
fn does_not_fold_when_initializer_is_not_an_object_literal() {
    // `l` aliases an existing object; it must not be spread into a new literal.
    let input = "let l = tt;\nl.a = 1;";
    assert_eq_normalized(&apply(input), input);
}

#[test]
fn does_not_fold_proto_assignment() {
    let input = "let l = {\n    ...tt\n};\nl.__proto__ = p;";
    assert_eq_normalized(&apply(input), input);
}

#[test]
fn does_not_fold_assignment_to_different_target() {
    let input = "let l = {\n    ...tt\n};\nm.a = 1;";
    assert_eq_normalized(&apply(input), input);
}
