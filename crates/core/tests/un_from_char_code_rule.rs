mod common;

use common::{assert_eq_normalized, render_rule};
use wakaru_core::rules::UnFromCharCode;

fn apply(input: &str) -> String {
    render_rule(input, |mark| UnFromCharCode::new(mark))
}

// --- folds -------------------------------------------------------------------

#[test]
fn folds_ascii_sequence() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(72, 101, 108, 108, 111);"),
        "const x = \"Hello\";",
    );
}

#[test]
fn folds_single_code_unit() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(65);"),
        "const x = \"A\";",
    );
}

#[test]
fn folds_zero_arguments_to_empty_string() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode();"),
        "const x = \"\";",
    );
}

#[test]
fn folds_bmp_code_unit() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(0x263a);"),
        "const x = \"\u{263a}\";",
    );
}

#[test]
fn folds_surrogate_pair_to_astral_character() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(0xd83d, 0xde00);"),
        "const x = \"\u{1f600}\";",
    );
}

// --- guards (must NOT fold) --------------------------------------------------

#[test]
fn does_not_fold_non_literal_argument() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(code);"),
        "const x = String.fromCharCode(code);",
    );
}

#[test]
fn does_not_fold_lone_surrogate() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(0xd83d);"),
        "const x = String.fromCharCode(0xd83d);",
    );
}

#[test]
fn does_not_fold_out_of_range_value() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(0x10000);"),
        "const x = String.fromCharCode(0x10000);",
    );
}

#[test]
fn does_not_fold_negative_value() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(-1);"),
        "const x = String.fromCharCode(-1);",
    );
}

#[test]
fn does_not_fold_fractional_value() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(65.9);"),
        "const x = String.fromCharCode(65.9);",
    );
}

#[test]
fn does_not_fold_other_static_method() {
    assert_eq_normalized(
        &apply("const x = String.fromCodePoint(65);"),
        "const x = String.fromCodePoint(65);",
    );
}

#[test]
fn does_not_fold_spread_arguments() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(...codes);"),
        "const x = String.fromCharCode(...codes);",
    );
}

#[test]
fn does_not_fold_shadowed_local_string() {
    // `String` is a local parameter here, not the global constructor.
    assert_eq_normalized(
        &apply("function f(String) {\n    return String.fromCharCode(72);\n}"),
        "function f(String) {\n    return String.fromCharCode(72);\n}",
    );
}

#[test]
fn does_not_fold_partially_non_literal_arguments() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(72, code);"),
        "const x = String.fromCharCode(72, code);",
    );
}

#[test]
fn does_not_fold_on_other_receiver() {
    assert_eq_normalized(
        &apply("const x = obj.fromCharCode(72);"),
        "const x = obj.fromCharCode(72);",
    );
}

#[test]
fn does_not_fold_reversed_surrogate_pair() {
    // Low surrogate before high is not a valid pair.
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(0xde00, 0xd83d);"),
        "const x = String.fromCharCode(0xde00, 0xd83d);",
    );
}

#[test]
fn does_not_fold_unpaired_high_surrogate() {
    assert_eq_normalized(
        &apply("const x = String.fromCharCode(0xd83d, 0x41);"),
        "const x = String.fromCharCode(0xd83d, 0x41);",
    );
}
